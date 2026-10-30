//! Shared actor virtual+4C, 0047AAF0. The factory's raw object and its
//! virtual+18 actor component are distinct; only the component is published
//! through the optional caller slot and receives source/pose inheritance.

use inf_formats::native_basis::NativeBasis;
use thiserror::Error;

use super::DerivedActorError;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId, RuntimeAllocationError};

/// The constructor keeps the full DWORD even though its resident lookup uses
/// signed LOWWORD. In 004824A0, LEA EAX,[ESP+8] at00482511 followed by MOV AX
/// [payload+78] supplies a stack-address high half, not an authored zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorChildResource {
    Full(u32),
    /// Symbolic native stack-address bits. A factory may admit this only for
    /// a resident route whose reached consumers use LOWWORD exclusively.
    StackPositionLowWord(u16),
    /// An authored WORD loaded into a register's low half (0046AD11's
    /// `mov ax`); the high half keeps that register's earlier value. Admitted
    /// on the same LOWWORD-only routes as `StackPositionLowWord`.
    RegisterLowWord(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorChildRequest {
    pub resource: ActorChildResource,
    /// Actual optional caller position, copied after source virtual+48.
    pub position: Option<[u16; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorChildFactoryCall {
    /// Native factory receives parent+32. The service reads this live source
    /// at construction, before the wrapper applies its caller's override.
    pub parent: ObjectId,
    pub resource: ActorChildResource,
    pub spawn_mode_1: bool,
}

pub trait ActorChildHost {
    type Error;

    /// 00460310: actual current process NULL, no command suffix, NULL
    /// rotation, placement FFFFFFFF. All construction uses the loaned RNG.
    fn create_child_object(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorChildFactoryCall,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error>;

    /// The returned object's actual virtual+18, independently of raw result.
    fn resolve_child_actor(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error>;

    /// Parent virtual+48. Selector36 uses 004026B0, returning parent+44.
    fn child_source_basis(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<Option<NativeBasis>, Self::Error>;

    /// 0047A110 after the caller has already stored dirty/model fields.
    fn refresh_child_parent_model(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<(), Self::Error>;

    /// 00431EF0's 004600F0: registry resource `resource` with the process
    /// NULL, no command list and no position (resource 0x82, the
    /// flamethrower). None is a refused construction.
    fn create_registry_object(
        &mut self,
        arena: &mut ObjectArena,
        resource: u16,
    ) -> Result<Option<ObjectId>, Self::Error>;

    /// 0048CAF9's 0046AE90: the shooter's player takes its new shot (one
    /// with flag 0x200, the implant), and the camera follows it.
    fn take_shot(
        &mut self,
        arena: &mut ObjectArena,
        player: crate::object_runtime::PlayerId,
        shot: crate::object_runtime::ActorHandle,
    ) -> Result<bool, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorChildError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Allocation(#[from] RuntimeAllocationError),
    #[error("actor {actor:?} requires field+{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("actor child service failed: {0}")]
    Service(E),
}

impl ObjectArena {
    /// 0047AAF0. Errors preserve every preceding factory and wrapper write,
    /// including an already-published out-slot; no child rollback is implied.
    pub fn create_actor_child<H: ActorChildHost + ?Sized>(
        &mut self,
        parent: ObjectId,
        request: ActorChildRequest,
        out_actor: Option<&mut Option<ObjectId>>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, ActorChildError<H::Error>> {
        let spawn_mode_1 = self.object(parent)?.base.flags & 0x8000_0000 == 0
            && self.actor(parent)?.handle.raw() & 0x0400_0000 != 0;
        let raw = host
            .create_child_object(
                self,
                ActorChildFactoryCall {
                    parent,
                    resource: request.resource,
                    spawn_mode_1,
                },
                rng,
            )
            .map_err(ActorChildError::Service)?;
        let child = match raw {
            Some(object) => host
                .resolve_child_actor(self, object)
                .map_err(ActorChildError::Service)?,
            None => None,
        };
        if let Some(out) = out_actor {
            *out = child;
        }
        let Some(child) = child else { return Ok(raw) };
        let source = self.actor(parent)?.handle;
        self.owned_mut(child)?.state.base.dirty_word |= 4;
        self.actor_mut(child)?.owner_handle = source;
        let dirty = self.object(child)?.base.dirty_word | 2;
        let owner = self.actor(parent)?.word_b4;
        self.actor_mut(child)?.word_b4 = owner;
        self.owned_mut(child)?.state.base.dirty_word = dirty;
        let basis = host
            .child_source_basis(self, parent)
            .map_err(ActorChildError::Service)?;
        if let Some(position) = request.position {
            self.owned_mut(child)?.state.base.position_words = Some(position);
        }
        if let Some(basis) = basis {
            self.actor_mut(child)?.rotating.basis = basis;
        }
        Ok(raw)
    }

    /// 0047B540 -> 0047A6C0 -> 004909E0, then three wrapping WORD adds.
    /// Uses the existing X+Y then Z, FPCW027F transform; no dirty publication.
    pub fn add_actor_local_linear_velocity(
        &mut self,
        actor: ObjectId,
        increment: [u16; 3],
    ) -> Result<(), ObjectError> {
        let world = self.transform_visual_vector_words(actor, increment)?;
        self.add_actor_linear_velocity(actor, world)
    }
}
