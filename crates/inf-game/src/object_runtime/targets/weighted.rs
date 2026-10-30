//! Shared weighted target publishers 00427110, 00427220 and 00427390. Their Boolean
//! result reports a previously empty pointer slot, including allocation null;
//! it does not report score changes or successful replacement.

use super::{
    ActorFollowTarget, FixedPositionTarget, FollowTargetRequest, FurniturePositionTarget,
    ObjectPositionTarget,
};
use super::{
    ActorServiceAllocationRequest, ActorServiceHeapError, ActorServiceResourceKind, NativeHeapRef,
};
use crate::object_runtime::{ActorHandle, ObjectArena, ObjectError, ObjectId};
use thiserror::Error;

/// Concrete target classes, independent from the referent's actor/object class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTarget {
    ActorFollow(ActorFollowTarget),
    FurniturePosition(FurniturePositionTarget),
    FixedPosition(FixedPositionTarget),
    ObjectPosition(ObjectPositionTarget),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTargetClass {
    ActorFollow,
    FurniturePosition,
    FixedPosition,
    ObjectPosition,
}

impl NativeTargetClass {
    pub fn allocation_bytes(self) -> u32 {
        match self {
            Self::ActorFollow => 0x20,
            Self::FurniturePosition => 0x1c,
            Self::FixedPosition => 0x14,
            Self::ObjectPosition => 0x18,
        }
    }
}

impl NativeTarget {
    pub fn class(self) -> NativeTargetClass {
        match self {
            Self::ActorFollow(_) => NativeTargetClass::ActorFollow,
            Self::FurniturePosition(_) => NativeTargetClass::FurniturePosition,
            Self::FixedPosition(_) => NativeTargetClass::FixedPosition,
            Self::ObjectPosition(_) => NativeTargetClass::ObjectPosition,
        }
    }

    /// Raw +04 retains its own meaning: types8/16 contain the process null
    /// handle, while its actual common-object identity is the distinct +14.
    pub fn handle_04(self) -> ActorHandle {
        match self {
            Self::ActorFollow(target) => target.handle_04,
            Self::FurniturePosition(target) => target.handle_04,
            Self::FixedPosition(target) => target.handle_04,
            Self::ObjectPosition(target) => target.handle_04,
        }
    }

    /// Virtual +14: types2/4/8 are native 00401300, type16 is 004073F0.
    pub fn object_referent(self) -> Option<ObjectId> {
        match self {
            Self::ActorFollow(_) | Self::FurniturePosition(_) | Self::FixedPosition(_) => None,
            Self::ObjectPosition(target) => Some(target.object_14),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeTargetError {
    #[error(transparent)]
    Heap(#[from] ActorServiceHeapError),
    #[error("target constructor requires current process NULL0054E894")]
    MissingProcessNull,
    #[error(transparent)]
    Object(#[from] ObjectError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeWeightedTarget {
    pub target: Option<NativeHeapRef>,
    pub score: u32,
}
impl NativeWeightedTarget {
    pub const fn new(target: Option<NativeHeapRef>, score: u32) -> Self {
        Self { target, score }
    }
    ///A released raw ref is still nonnull; dereference checks live heap custody.
    pub fn native_pointer_present(self) -> bool {
        self.target.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeightedTargetReferent {
    /// Original 00427110 takes an actual actor pointer, then reads its +84.
    Actor(Option<ObjectId>),
    /// Original 00427220 takes a common-object pointer, not an actor handle.
    Object(Option<ObjectId>),
    ///00427390 takes an actual six-byte point pointer. None is pointer null;
    /// copied words are supplied data, independent from target heap padding.
    Point(Option<[u16; 3]>),
}

impl WeightedTargetReferent {
    fn is_nonnull(self) -> bool {
        match self {
            Self::Actor(object) | Self::Object(object) => object.is_some(),
            Self::Point(point) => point.is_some(),
        }
    }

    fn class(self) -> NativeTargetClass {
        match self {
            Self::Actor(_) => NativeTargetClass::ActorFollow,
            Self::Object(_) => NativeTargetClass::ObjectPosition,
            Self::Point(_) => NativeTargetClass::FixedPosition,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WeightedTargetRequest<'a> {
    pub referent: WeightedTargetReferent,
    pub score: u32,
    pub constructor_null: WeightedTargetConstructorNull<'a>,
}

///Fresh actual owner read, made only at the reached constructor after allocation.
pub trait TargetNullHandleSource {
    fn target_null_handle(&self) -> Option<ActorHandle>;
}
impl TargetNullHandleSource for crate::process_particles::factories::ParticleFactoryProcessState {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.process_null_handle
    }
}
#[derive(Clone, Copy)]
pub enum WeightedTargetConstructorNull<'a> {
    ///Explicit decoded/native fixture mode; production uses the process owner.
    ActorRegistry,
    Process(&'a dyn TargetNullHandleSource),
    #[cfg(test)]
    Supplied(Option<ActorHandle>),
}
impl std::fmt::Debug for WeightedTargetConstructorNull<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActorRegistry => f.write_str("ActorRegistry"),
            Self::Process(_) => f.write_str("Process(live owner)"),
            #[cfg(test)]
            Self::Supplied(value) => f.debug_tuple("Supplied").field(value).finish(),
        }
    }
}
impl WeightedTargetConstructorNull<'_> {
    pub(crate) fn resolve(self, arena: &ObjectArena) -> Result<ActorHandle, NativeTargetError> {
        match self {
            Self::ActorRegistry => Ok(arena
                .actor_handles()
                .ok_or(ObjectError::MissingActorRegistry)?
                .null_handle()),
            Self::Process(source) => source
                .target_null_handle()
                .ok_or(NativeTargetError::MissingProcessNull),
            #[cfg(test)]
            Self::Supplied(handle) => handle.ok_or(NativeTargetError::MissingProcessNull),
        }
    }
}

impl ObjectArena {
    /// Publishers' distinct replacement policies: actor/object invoke their
    /// class virtuals, point tests the actual kind DWORD and copies raw +08.
    /// Null publisher inputs are rejected before reaching this phase.
    pub(crate) fn try_replace_weighted_target(
        &mut self,
        target: &mut NativeTarget,
        referent: WeightedTargetReferent,
    ) -> Result<bool, ObjectError> {
        match referent {
            WeightedTargetReferent::Point(Some(point)) if target.kind_word() == 8 => {
                target.set_focus_08_words(point);
                Ok(true)
            }
            WeightedTargetReferent::Actor(actor) => {
                if let NativeTarget::ActorFollow(target) = target {
                    self.replace_follow_target(target, actor)
                } else {
                    Ok(false)
                }
            }
            WeightedTargetReferent::Object(Some(object)) => {
                if let NativeTarget::ObjectPosition(target) = target {
                    self.replace_object_position_target(target, object)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    /// Target virtual +10 resolves type 2's handle on every query. Type 16's
    /// 00401300 returns null without even requiring an actor registry.
    pub fn resolve_target_actor(
        &mut self,
        target: &NativeTarget,
    ) -> Result<Option<ObjectId>, ObjectError> {
        match target {
            NativeTarget::ActorFollow(target) => self
                .resolve_actor_handle(target.handle_04)
                .map_err(|_| ObjectError::MissingActorRegistry),
            NativeTarget::FurniturePosition(_)
            | NativeTarget::FixedPosition(_)
            | NativeTarget::ObjectPosition(_) => Ok(None),
        }
    }

    /// Target virtual +04 always allocates a new resource. Type 2 copies its
    /// explicit +14 and preserves unknown +08/velocity; type4 (00482890)
    /// copies +08 and +14 and type8 +08, each resetting its handle; type16
    /// rereads live object position. Rust Copy is
    /// never used as this native operation.
    pub fn clone_native_target(
        &mut self,
        reference: NativeHeapRef,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<Option<NativeHeapRef>, NativeTargetError> {
        let target = self.native_target(reference)?;
        let Some(clone) = self.allocate_actor_service(ActorServiceAllocationRequest {
            kind: ActorServiceResourceKind::Target(target.class()),
        })?
        else {
            return Ok(None);
        };
        let data = match target {
            NativeTarget::ActorFollow(target) => NativeTarget::ActorFollow(target.clone_native()),
            NativeTarget::FurniturePosition(target) => {
                NativeTarget::FurniturePosition(FurniturePositionTarget::new(
                    constructor_null.resolve(self)?,
                    target.focus_08,
                    Some(target.focus_14),
                ))
            }
            NativeTarget::FixedPosition(target) => NativeTarget::FixedPosition(
                FixedPositionTarget::from_point(constructor_null.resolve(self)?, target.focus_08),
            ),
            NativeTarget::ObjectPosition(target) => {
                let handle = constructor_null.resolve(self)?;
                let point = self.target_object_position(target.object_14)?;
                NativeTarget::ObjectPosition(ObjectPositionTarget::from_position(
                    handle,
                    point,
                    target.object_14,
                ))
            }
        };
        self.construct_native_target(clone, data)?;
        Ok(Some(clone))
    }

    pub fn publish_weighted_target(
        &mut self,
        pair: &mut NativeWeightedTarget,
        request: WeightedTargetRequest<'_>,
    ) -> Result<bool, NativeTargetError> {
        if !request.referent.is_nonnull() || request.score == 0 {
            if let Some(reference) = pair.target {
                self.release_actor_service(reference)?;
                pair.target = None;
                pair.score = 0;
            }
            return Ok(false);
        }
        pair.score = request.score;
        let initially_empty = pair.target.is_none();
        if let Some(reference) = pair.target {
            let mut data = self.native_target(reference)?;
            let accepted = self.try_replace_weighted_target(&mut data, request.referent);
            //Replacement can publish identity before a reached XYZ fault.
            self.write_native_target(reference, data)?;
            if accepted? {
                return Ok(false);
            }
            self.release_actor_service(reference)?;
            //Raw old ref stays nonnull/Released until completed publication.
        }
        let replacement = if let Some(reference) =
            self.allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(request.referent.class()),
            })? {
            let data = match request.referent {
                WeightedTargetReferent::Actor(Some(object)) => {
                    NativeTarget::ActorFollow(self.follow_target(
                        FollowTargetRequest::resolve_actor(self.actor(object)?.handle),
                    )?)
                }
                WeightedTargetReferent::Object(Some(object)) => {
                    let handle = request.constructor_null.resolve(self)?;
                    let point = self.target_object_position(object)?;
                    NativeTarget::ObjectPosition(ObjectPositionTarget::from_position(
                        handle, point, object,
                    ))
                }
                WeightedTargetReferent::Point(Some(point)) => {
                    NativeTarget::FixedPosition(FixedPositionTarget::from_point(
                        request.constructor_null.resolve(self)?,
                        Some(point),
                    ))
                }
                _ => unreachable!("nonnull input checked before allocation"),
            };
            self.construct_native_target(reference, data)?;
            Some(reference)
        } else {
            None
        };
        pair.target = replacement;
        Ok(initially_empty)
    }
}

#[cfg(test)]
mod tests;
