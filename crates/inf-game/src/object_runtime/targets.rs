//! Shared native targets used by cameras and cinematic route helpers.
//!
//! Type 2 is 004825D0/00528038; type 4 is 004827D0/00528060; type 8 is
//! 00402730/0051F154; type 16 is 004073A0/0051FB6C.
//! These resources own copied vectors and referent identities, never the
//! referent's allocation. Unknown native heap words remain unknown.

use super::{ActorHandle, ObjectArena, ObjectError, ObjectId};

mod heap;
#[cfg(test)]
pub use heap::FixtureActorServiceAllocation;
pub use heap::{
    ActorServiceAllocationPolicy, ActorServiceAllocationRequest, ActorServiceHeap,
    ActorServiceHeapError, ActorServiceResourceKind, ActorServiceResourceState, NativeHeapRef,
    NativeWeaponFireNode,
};
mod fixed;
pub use fixed::{
    FixedPositionTarget, FurniturePositionTarget, NativeTargetQueryError, PROCESS_VELOCITY_0055F0F8,
};
mod weighted;
pub use weighted::{
    NativeTarget, NativeTargetClass, NativeTargetError, NativeWeightedTarget,
    TargetNullHandleSource, WeightedTargetConstructorNull, WeightedTargetReferent,
    WeightedTargetRequest,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowTargetPoint {
    /// Null explicit-point pointer: resolve the handle and copy raised XYZ.
    ResolveActor,
    /// Non-null explicit-point pointer, including a clone of unknown words.
    ExplicitPoint(Option<[u16; 3]>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FollowTargetRequest {
    pub handle: ActorHandle,
    pub point: FollowTargetPoint,
}

impl FollowTargetRequest {
    pub fn resolve_actor(handle: ActorHandle) -> Self {
        Self {
            handle,
            point: FollowTargetPoint::ResolveActor,
        }
    }
}

/// Native type 2's 0x20-byte allocation. Refresh alone writes velocity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorFollowTarget {
    pub handle_04: ActorHandle,
    pub focus_08: Option<[u16; 3]>,
    pub word_10: u32,
    pub focus_14: Option<[u16; 3]>,
    pub velocity_1a: Option<[u16; 3]>,
}

impl ActorFollowTarget {
    fn explicit_point(handle: ActorHandle, point: Option<[u16; 3]>) -> Self {
        Self {
            handle_04: handle,
            focus_08: None,
            word_10: 2,
            focus_14: point,
            velocity_1a: None,
        }
    }

    /// 004828D0 reconstructs through the explicit-point constructor. It
    /// neither resolves the handle nor copies refreshed +08 or velocity.
    pub fn clone_native(&self) -> Self {
        Self::explicit_point(self.handle_04, self.focus_14)
    }
}

/// Native type 16's 0x18-byte allocation. +14 is an arena object identity,
/// independent from the process null ActorHandle at +04. A route node and
/// actor remain distinct arena classes; each caller validates its own domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectPositionTarget {
    pub handle_04: ActorHandle,
    pub focus_08: [u16; 3],
    pub word_10: u32,
    pub object_14: ObjectId,
}

impl ObjectPositionTarget {
    pub(crate) fn from_position(handle: ActorHandle, point: [u16; 3], object: ObjectId) -> Self {
        Self {
            handle_04: handle,
            focus_08: point,
            word_10: 16,
            object_14: object,
        }
    }
}

impl ObjectArena {
    /// 004825D0: ResolveActor writes both +08 and +14 when live. An explicit
    /// point writes +14 alone and does not even require an actor registry.
    pub fn follow_target(
        &mut self,
        request: FollowTargetRequest,
    ) -> Result<ActorFollowTarget, ObjectError> {
        if let FollowTargetPoint::ExplicitPoint(point) = request.point {
            return Ok(ActorFollowTarget::explicit_point(request.handle, point));
        }
        let resolved = self
            .resolve_actor_handle(request.handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        let focus = match resolved {
            Some(object) => {
                let [x, y, z] = self
                    .object(object)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(object))?;
                Some([x, y.wrapping_add(self.actor(object)?.word_74), z])
            }
            None => None,
        };
        Ok(ActorFollowTarget {
            handle_04: request.handle,
            focus_08: focus,
            word_10: 2,
            focus_14: focus,
            velocity_1a: None,
        })
    }

    /// Virtual +18, 00482690. A stale handle returns zero without writes;
    /// settings +7C gates only the copy to +14, after focus and velocity.
    pub fn refresh_follow_target(
        &mut self,
        target: &mut ActorFollowTarget,
        settings_7c: u32,
    ) -> Result<u32, ObjectError> {
        let Some(object) = self
            .resolve_actor_handle(target.handle_04)
            .map_err(|_| ObjectError::MissingActorRegistry)?
        else {
            return Ok(0);
        };
        let base = &self.object(object)?.base;
        let [x, y, z] = base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(object))?;
        let actor = self.actor(object)?;
        let focus = [x, y.wrapping_add(actor.word_74), z];
        target.focus_08 = Some(focus);
        target.velocity_1a = Some(if base.flags & 0x0004_0000 != 0 {
            actor.words_8e_92.map(u16::wrapping_neg)
        } else {
            actor.words_8e_92
        });
        if settings_7c != 0 {
            target.focus_14 = Some(focus);
        }
        Ok(actor.word_c4)
    }

    /// Virtual +0C, 00482740: live actor basis or the native identity.
    pub fn follow_target_basis(
        &mut self,
        target: &ActorFollowTarget,
    ) -> Result<inf_formats::native_basis::NativeBasis, ObjectError> {
        match self
            .resolve_actor_handle(target.handle_04)
            .map_err(|_| ObjectError::MissingActorRegistry)?
        {
            Some(object) => Ok(self.actor(object)?.rotating.basis),
            None => Ok(inf_formats::native_basis::NativeBasis::INITIAL),
        }
    }

    /// Virtual +20, 00482760. The input's +84 is written before resolution.
    /// A stale handle succeeds and preserves both cached vectors. A live
    /// replacement writes +14 alone, raised by unsigned size +28 divided by
    /// four; constructor and refresh use the distinct actor +74 policy.
    pub fn replace_follow_target(
        &mut self,
        target: &mut ActorFollowTarget,
        actor: Option<ObjectId>,
    ) -> Result<bool, ObjectError> {
        target.handle_04 = match actor {
            Some(actor) => self.actor(actor)?.handle,
            None => self
                .actor_handles()
                .ok_or(ObjectError::MissingActorRegistry)?
                .null_handle(),
        };
        if let Some(actor) = self
            .resolve_actor_handle(target.handle_04)
            .map_err(|_| ObjectError::MissingActorRegistry)?
        {
            let base = &self.object(actor)?.base;
            let [x, y, z] = base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))?;
            target.focus_14 = Some([x, y.wrapping_add(base.word_28 >> 2), z]);
        }
        Ok(true)
    }

    fn target_object_position(&self, object: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(object)?
            .base
            .position_words
            .ok_or(ObjectError::TargetObjectPositionUnknown(object))
    }

    /// 004073A0/00462CC0 share the same constructor fields. Clone reads the
    /// live referent's +32, so Rust's Copy is not the native clone operation.
    pub fn object_position_target(
        &self,
        object: ObjectId,
    ) -> Result<ObjectPositionTarget, ObjectError> {
        let handle = self
            .actor_handles()
            .ok_or(ObjectError::MissingActorRegistry)?
            .null_handle();
        let focus = self.target_object_position(object)?;
        Ok(ObjectPositionTarget::from_position(handle, focus, object))
    }

    pub fn clone_object_position_target(
        &self,
        target: &ObjectPositionTarget,
    ) -> Result<ObjectPositionTarget, ObjectError> {
        self.object_position_target(target.object_14)
    }

    /// Virtual +24, 00482860: identical identity succeeds without reading
    /// XYZ. Changed identity is published before the reached position read.
    pub fn replace_object_position_target(
        &self,
        target: &mut ObjectPositionTarget,
        object: ObjectId,
    ) -> Result<bool, ObjectError> {
        if target.object_14 != object {
            target.object_14 = object;
            target.focus_08 = self.target_object_position(object)?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;

impl ObjectArena {
    pub fn actor_service_heap(&self) -> &ActorServiceHeap {
        &self.actor_services
    }
    pub fn actor_service_heap_mut(&mut self) -> &mut ActorServiceHeap {
        &mut self.actor_services
    }
    pub fn allocate_actor_service(
        &mut self,
        request: ActorServiceAllocationRequest,
    ) -> Result<Option<NativeHeapRef>, ActorServiceHeapError> {
        self.actor_services.allocate(request)
    }
    pub fn native_target(&self, reference: NativeHeapRef) -> Result<NativeTarget, ObjectError> {
        Ok(self.actor_services.target(reference)?)
    }
    pub fn construct_native_target(
        &mut self,
        reference: NativeHeapRef,
        data: NativeTarget,
    ) -> Result<(), ObjectError> {
        Ok(self.actor_services.construct_target(reference, data)?)
    }
    pub fn write_native_target(
        &mut self,
        reference: NativeHeapRef,
        data: NativeTarget,
    ) -> Result<(), ObjectError> {
        Ok(self.actor_services.write_target(reference, data)?)
    }
    pub fn release_actor_service(&mut self, reference: NativeHeapRef) -> Result<(), ObjectError> {
        Ok(self.actor_services.release(reference)?)
    }
    pub fn follow_target_resource(
        &mut self,
        request: FollowTargetRequest,
    ) -> Result<Option<NativeHeapRef>, ObjectError> {
        let Some(reference) = self.allocate_actor_service(ActorServiceAllocationRequest {
            kind: ActorServiceResourceKind::Target(NativeTargetClass::ActorFollow),
        })?
        else {
            return Ok(None);
        };
        let data = self.follow_target(request)?;
        self.construct_native_target(reference, NativeTarget::ActorFollow(data))?;
        Ok(Some(reference))
    }
    /// A test's stand-in for a behavior's target choice: the actor's
    /// current target +148.
    #[cfg(test)]
    pub(crate) fn set_target_148_for_tests(&mut self, actor: ObjectId, target: NativeHeapRef) {
        self.derived_actor_mut(actor).unwrap().target_148 = Some(target);
    }
    #[cfg(test)]
    pub fn import_fixture_target(&mut self, data: NativeTarget) -> NativeHeapRef {
        let reference = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(data.class()),
            })
            .expect("fixture allocation custody")
            .expect("fixture requires real reservation");
        self.construct_native_target(reference, data)
            .expect("fixture constructor");
        reference
    }
}

impl ObjectArena {
    pub fn actor_follow_target(
        &self,
        reference: NativeHeapRef,
    ) -> Result<ActorFollowTarget, ObjectError> {
        match self.native_target(reference)? {
            NativeTarget::ActorFollow(data) => Ok(data),
            _ => Err(ActorServiceHeapError::Kind(reference).into()),
        }
    }
    pub fn object_position_target_data(
        &self,
        reference: NativeHeapRef,
    ) -> Result<ObjectPositionTarget, ObjectError> {
        match self.native_target(reference)? {
            NativeTarget::ObjectPosition(data) => Ok(data),
            _ => Err(ActorServiceHeapError::Kind(reference).into()),
        }
    }
    pub fn refresh_follow_target_resource(
        &mut self,
        reference: NativeHeapRef,
        settings_7c: u32,
    ) -> Result<u32, ObjectError> {
        let mut data = self.actor_follow_target(reference)?;
        let result = self.refresh_follow_target(&mut data, settings_7c);
        self.write_native_target(reference, NativeTarget::ActorFollow(data))?;
        result
    }
    pub fn object_position_target_resource(
        &mut self,
        object: ObjectId,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<Option<NativeHeapRef>, NativeTargetError> {
        let Some(reference) = self.allocate_actor_service(ActorServiceAllocationRequest {
            kind: ActorServiceResourceKind::Target(NativeTargetClass::ObjectPosition),
        })?
        else {
            return Ok(None);
        };
        let handle = constructor_null.resolve(self)?;
        let point = self.target_object_position(object)?;
        self.construct_native_target(
            reference,
            NativeTarget::ObjectPosition(ObjectPositionTarget::from_position(
                handle, point, object,
            )),
        )?;
        Ok(Some(reference))
    }
}
