//!0047AD30, the shared actor+24 countdown inherited by selector16, and
//! 0047EBF0, the shared +3C of an actor another machine owns.
use thiserror::Error;

use crate::object_runtime::{
    ActorMotionError, ActorPhysicsRequest, FamilyUpdateEffect, FamilyUpdateHost, ObjectArena,
    ObjectError, ObjectId,
};

/// What 0047EBF0 reaches: the process RNG and the status visual's trail.
pub trait RemoteActorHost {
    type Error: std::fmt::Display;
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// The trail's particle ([`FamilyUpdateEffect::StatusTrail`]).
    fn status_trail(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error>;
}

/// What 0047EBF0 needs of a host that takes the process RNG with each
/// call: the status visual's load factor (0055F154) and its trail
/// (00489D90).
pub trait StatusTrailHost {
    type Error;
    fn status_load_factor(&self) -> i32;
    fn status_trail(
        &mut self,
        arena: &mut ObjectArena,
        effect: FamilyUpdateEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

/// A [`StatusTrailHost`] and the process RNG as 0047EBF0's services.
pub(crate) struct RngRemoteHost<'h, H> {
    pub host: &'h mut H,
    pub rng: &'h mut u32,
}

impl<H: StatusTrailHost> RemoteActorHost for RngRemoteHost<'_, H>
where
    H::Error: std::fmt::Display,
{
    type Error = H::Error;
    fn random(&mut self) -> Result<u32, H::Error> {
        Ok(crate::name_entry::name_entry_rng_next(self.rng))
    }
    fn status_trail(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), H::Error> {
        self.host.status_trail(arena, effect, self.rng)
    }
}

/// A family update's services as 0047EBF0's: its RNG and its effects.
pub(crate) struct FamilyRemoteHost<'h, H>(pub &'h mut H);

impl<H: FamilyUpdateHost> RemoteActorHost for FamilyRemoteHost<'_, H>
where
    H::Error: std::fmt::Display,
{
    type Error = H::Error;
    fn random(&mut self) -> Result<u32, H::Error> {
        self.0.random()
    }
    fn status_trail(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), H::Error> {
        self.0.effect(arena, actor, effect)
    }
}

#[derive(Debug, Error)]
pub enum RemoteActorError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error("0047DFD0 on a remote actor: {0}")]
    Status(String),
    #[error("remote actor service failed: {0}")]
    Host(E),
}

impl ObjectArena {
    ///Unsigned WORD+C8 and unsigned scheduler delta>>5. This is independent
    ///of the camera's later pose phase and changes no flags or memberships.
    pub fn tick_actor_common_timer(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
    ) -> Result<(), ObjectError> {
        let remaining = self.actor(actor)?.word_c8;
        if remaining != 0 {
            let elapsed = microseconds >> 5;
            self.actor_mut(actor)?.word_c8 = if u32::from(remaining) > elapsed {
                remaining.wrapping_sub(elapsed as u16)
            } else {
                0
            };
        }
        Ok(())
    }

    /// 0047EBF0: unless `+2C` bit 3 holds it, 0047B1E0(delta, 0x18) moves
    /// the actor on its velocity and spin; with dirty bit 0x400 its status
    /// visual (0047DFD0) runs; `+C8` counts down by the whole delta.
    pub fn tick_remote_actor<H: RemoteActorHost>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        sine_quarter: &[u16; 4096],
        load_factor: Option<i32>,
        host: &mut H,
    ) -> Result<(), RemoteActorError<H::Error>> {
        if self.object(actor)?.base.flags & 8 == 0 {
            self.tick_actor_physics(
                actor,
                ActorPhysicsRequest {
                    microseconds,
                    mask: 0x18,
                    sine_quarter,
                },
            )?;
        }
        if self.object(actor)?.base.dirty_word & 0x400 != 0 {
            let packet = self
                .status_visual_trail(actor, microseconds, load_factor, || host.random())
                .map_err(|error| match error {
                    crate::object_runtime::FamilyUpdateError::StatusVisualRandom {
                        error, ..
                    } => RemoteActorError::Host(error),
                    other => RemoteActorError::Status(other.to_string()),
                })?;
            if let Some(packet) = packet {
                let effect = FamilyUpdateEffect::StatusTrail {
                    particle_type: packet.particle_type,
                    position: packet.position,
                    velocity: packet.velocity,
                    flags: 0,
                };
                host.status_trail(self, actor, effect)
                    .map_err(RemoteActorError::Host)?;
            }
        }
        let state = self.actor_mut(actor)?;
        state.word_c8 = if microseconds < u32::from(state.word_c8) {
            state.word_c8 - microseconds as u16
        } else {
            0
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests;
