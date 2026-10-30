//! Shared 00424280/00424590 dying state, with the actual selector10 and
//! selector36 virtual wrappers. Lower services retain the live actor and RNG.

mod audio;
mod entry;
mod stage;
mod update;

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod selector9_tests;

use crate::{
    object_runtime::{
        ActorHandle, ActorStateChangeRequest, DerivedActorError, ObjectArena, ObjectError,
        ObjectId, TargetNullHandleSource,
    },
    process_particles::births::ParticleBurst,
};
use inf_formats::native_basis::NativeBasis;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyDyingRequest {
    Change { entering: u32 },
    Update { microseconds: u32 },
    Stage { entering: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyDyingAudio {
    OneShot { sound: i32, volume: u32, pitch: u32 },
    CreatePersistent { sound: u32, volume: u32, pitch: u32 },
    UpdatePersistent { volume: u32, pitch: u32 },
    StopPersistent,
}

/// Each case denotes a real reached service. Actor-relative positions and
/// sound ownership are read by that service; stack burst/pickup values are
/// captured here at their original producer instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyDyingEffect {
    RefreshModel,
    Burst(ParticleBurst),
    Radius {
        block_address: u32,
        source: ActorHandle,
    },
    CameraEffect {
        strength: u32,
        argument: u32,
    },
    Audio(FamilyDyingAudio),
    StatusVisual {
        microseconds: u32,
    },
    ChangeState(ActorStateChangeRequest),
    Pickup {
        position: [u16; 3],
        kind: u32,
        value: u16,
    },
    AttachSpawnedActor {
        child: ObjectId,
    },
    Selector36Child,
    StageChild {
        resource: u32,
        position: [u16; 3],
    },
    Trail {
        kind: u32,
        velocity: [u16; 3],
        flags: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyDyingSpawnRequest {
    pub resource: u32,
    pub position: [u16; 3],
    pub source: ActorHandle,
}

pub trait FamilyDyingHost: TargetNullHandleSource {
    type Error;

    /// The actual +244 timer table and the mutable process scalar globals.
    fn read_dword(&mut self, address: u32) -> Result<u32, Self::Error>;
    /// Actual virtual +138. Production dispatches FamilyDyingRequest::Stage
    /// on the same host; this boundary permits native callback mutations.
    fn stage_change(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        entering: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyDyingEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    fn sound_present(&mut self, sound: i16) -> Result<bool, Self::Error>;
    /// Actual 0046B030 player lookup, only reached by the spawn-count branch.
    fn player_position(&mut self, arena: &mut ObjectArena)
        -> Result<Option<[u16; 3]>, Self::Error>;
    /// 00460310 with NULL command/rotation, placement -1 and mode zero.
    fn create_death_object(
        &mut self,
        arena: &mut ObjectArena,
        request: FamilyDyingSpawnRequest,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error>;
    fn resolve_spawned_actor(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error>;
    /// Actual +48; its returned basis is copied by the pickup producer.
    fn actor_basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, Self::Error>;
    /// 00491D10 on a stack float vector with the indicated startup axis.
    fn rotate_pickup_vector(
        &mut self,
        vector: &mut [u32; 3],
        axis_address: u32,
        angle: u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyDyingError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("dying actor {actor:?} requires +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("dying actor spawn requires current process NULL at00424508/23")]
    MissingProcessNull,
    #[error("dying service at {address:08X} failed: {error}")]
    Service { address: u32, error: E },
    #[error("dying arithmetic division trap at {address:08X}")]
    Division { address: u32 },
}

fn service<E>(address: u32, error: E) -> FamilyDyingError<E> {
    FamilyDyingError::Service { address, error }
}

impl ObjectArena {
    pub fn run_family_dying<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        request: FamilyDyingRequest,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        match request {
            FamilyDyingRequest::Change { entering } => {
                self.family_dying_entry(actor, entering, host, rng)
            }
            FamilyDyingRequest::Update { microseconds } => {
                self.family_dying_update(actor, microseconds, host, rng)
            }
            FamilyDyingRequest::Stage { entering } => {
                self.family_dying_stage(actor, entering, host, rng)
            }
        }
    }

    fn dying_position<E>(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<[u16; 3], FamilyDyingError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(FamilyDyingError::RequiredField {
                actor,
                offset: 0x32,
                address,
            })
    }

    fn dying_effect<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        effect: FamilyDyingEffect,
        address: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        host.effect(self, actor, effect, rng)
            .map_err(|error| service(address, error))
    }
}

fn burst(
    position: [u16; 3],
    debris_count: u16,
    central_type: u32,
    debris_types: [u32; 2],
) -> ParticleBurst {
    ParticleBurst {
        debris_count,
        position,
        placement_radius: 20,
        central_type,
        debris_types,
        speed: 8,
        flags: 0,
    }
}
