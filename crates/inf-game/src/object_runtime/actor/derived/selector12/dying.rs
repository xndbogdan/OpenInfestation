//! Selector12 dying change 004678A0 and update 004679C0. Services observe each
//! reached prefix; neither a failed callback nor missing input rolls it back.

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        mark_flag_change, ActorStateChangeRequest, DerivedActorError, ObjectArena, ObjectError,
        ObjectId, TargetNullHandleSource,
    },
    process_particles::births::ParticleBurst,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector12DyingRequest {
    /// Only the low byte is tested by 004678A0, before receiver access.
    Change { entering: u32 },
    /// The native stack delta is unused; +188 already contains elapsed time.
    Update,
}

pub trait Selector12DyingHost: TargetNullHandleSource {
    type Error;

    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;

    fn birth_burst(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;

    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorStateChangeRequest,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector12DyingError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("selector12 {actor:?} requires field+{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("selector12 dying update requires process NULL0054E894 at004679D5")]
    MissingProcessNull,
    #[error("selector12 dying model refresh failed: {0}")]
    Refresh(E),
    #[error("selector12 dying burst failed: {0}")]
    Burst(E),
    #[error("selector12 dying state change failed: {0}")]
    StateChange(E),
}

impl ObjectArena {
    pub fn run_selector12_dying<H: Selector12DyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        request: Selector12DyingRequest,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector12DyingError<H::Error>> {
        match request {
            Selector12DyingRequest::Change { entering } if entering as u8 == 0 => Ok(()),
            Selector12DyingRequest::Change { .. } => {
                self.selector12_actor(actor)?;
                let radius = self.object(actor)?.base.word_28;
                let mut position = self.object(actor)?.base.position_words.ok_or(
                    Selector12DyingError::RequiredField {
                        actor,
                        offset: 0x34,
                        address: 0x0046_78b9,
                    },
                )?;
                position[1] = position[1].wrapping_add(radius);
                self.owned_mut(actor)?.state.base.position_words = Some(position);
                let base = &mut self.owned_mut(actor)?.state.base;
                mark_flag_change(base, base.flags & !0x40);
                let velocity = &mut self.actor_mut(actor)?.words_8e_92;
                velocity[1] = velocity[1].wrapping_add(radius);

                // Both IMULs keep low32 before arithmetic SAR16. A large
                // unsigned radius/random product can therefore become negative.
                let half = i32::from(radius) / 2;
                let x = half.wrapping_sub(
                    ((name_entry_rng_next(rng) & 0xffff).wrapping_mul(u32::from(radius)) as i32)
                        >> 16,
                );
                let velocity = &mut self.actor_mut(actor)?.words_8e_92;
                velocity[0] = velocity[0].wrapping_add(x as u16);
                let z = half.wrapping_sub(
                    ((name_entry_rng_next(rng) & 0xffff).wrapping_mul(u32::from(radius)) as i32)
                        >> 16,
                );
                let velocity = &mut self.actor_mut(actor)?.words_8e_92;
                velocity[2] = velocity[2].wrapping_add(z as u16);
                let angular = 0x8000u32.wrapping_sub(name_entry_rng_next(rng) & 0xffff);
                let current = self.actor(actor)?.words_94_9c[1];
                self.owned_mut(actor)?.state.base.dirty_word |= 0x100;
                self.actor_mut(actor)?.rotating.visual.model_word = 0x00f8;
                self.actor_mut(actor)?.words_94_9c[1] = current.wrapping_add(angular);
                host.refresh_model(self, actor, rng)
                    .map_err(Selector12DyingError::Refresh)?;

                // XYZ is copied after the refresh callback. Scales are then
                // published before burst48A8B0, with no normal-return suffix.
                let position = self.object(actor)?.base.position_words.ok_or(
                    Selector12DyingError::RequiredField {
                        actor,
                        offset: 0x32,
                        address: 0x0046_7951,
                    },
                )?;
                self.derived_actor_mut(actor)?.word_16c = Some(0x8000);
                self.derived_actor_mut(actor)?.word_170 = Some(0x8000);
                host.birth_burst(
                    self,
                    actor,
                    ParticleBurst {
                        debris_count: 16,
                        position,
                        placement_radius: 20,
                        central_type: 4,
                        debris_types: [5, 5],
                        speed: 8,
                        flags: 0,
                    },
                    rng,
                )
                .map_err(Selector12DyingError::Burst)
            }
            Selector12DyingRequest::Update => {
                self.selector12_actor(actor)?;
                let elapsed = self.family428650_actor(actor)?.words_188_19c[0].ok_or(
                    Selector12DyingError::RequiredField {
                        actor,
                        offset: 0x188,
                        address: 0x0046_79c0,
                    },
                )?;
                if elapsed as i32 <= 10_000_000 || self.object(actor)?.base.flags & 0x0200_0000 != 0
                {
                    return Ok(());
                }
                let source = host
                    .target_null_handle()
                    .ok_or(Selector12DyingError::MissingProcessNull)?;
                host.change_state(
                    self,
                    actor,
                    ActorStateChangeRequest {
                        target_state: 0,
                        argument: 0,
                        source,
                    },
                    rng,
                )
                .map_err(Selector12DyingError::StateChange)
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_receipts;
