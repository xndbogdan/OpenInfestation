//! Required services for the shared family virtual +24 prefixes. The native
//! positive-delay arm calls 0047B1E0(delta,1F) after publishing actor +144.
//! Weapon timers consume the actual +140 owner; damage, status particles and retirement remain boundaries;
//! this host does not turn their presence into successful no-op callbacks.

use thiserror::Error;

use super::{ActorMotionError, ActorPhysicsRequest, FamilyUpdateEffect, FamilyUpdateHost};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{ActorBehaviorTickError, ObjectArena, ObjectId},
};

/// Scene-independent process services. Both inputs are their actual retained
/// process owners: there is no actor-local RNG, sampled stream or fake sine map.
pub struct NativeFamilyUpdateHost<'a> {
    pub process_rng: &'a mut u32,
    pub sine_quarter: &'a [u16; 4096],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeFamilyUpdateError {
    #[error(transparent)]
    Physics(#[from] ActorMotionError),
    #[error(transparent)]
    Behavior(#[from] ActorBehaviorTickError),
    #[error("required family update effect remains unowned: {0:?}")]
    UnownedEffect(FamilyUpdateEffect),
    #[error("live family update service {phase} failed: {reason}")]
    LiveService { phase: &'static str, reason: String },
}

impl FamilyUpdateHost for NativeFamilyUpdateHost<'_> {
    type Error = NativeFamilyUpdateError;

    fn process_null_handle(&self) -> Option<crate::object_runtime::ActorHandle> {
        //This intrinsic physics/timer host has no process source loan.
        //The complete WorldFamilyUpdateHost supplies that retained owner.
        None
    }

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(self.process_rng))
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        match effect {
            FamilyUpdateEffect::DelayedPhysics { microseconds, mask } => {
                arena.tick_actor_physics(
                    actor,
                    ActorPhysicsRequest {
                        microseconds,
                        mask: mask as u8,
                        sine_quarter: self.sine_quarter,
                    },
                )?;
                Ok(())
            }
            FamilyUpdateEffect::WeaponTick { microseconds } => {
                arena.tick_actor_behavior(actor, microseconds)?;
                Ok(())
            }
            effect => Err(NativeFamilyUpdateError::UnownedEffect(effect)),
        }
    }
}

#[cfg(test)]
mod tests;
