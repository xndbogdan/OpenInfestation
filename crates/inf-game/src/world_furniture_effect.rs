//! Furniture damage 00436020 and retained pending effects 00436A60.
//!
//! Damage admission queues a real process-owned record; it does not remove
//! furniture immediately. The 00436700 executor owns timers, program traversal
//! and the settled queue/terrain callbacks; particle, audio and world services
//! remain required hosts. The copied effect byte +3 is opaque:
//! 0042A8F0 reads only signed strength +0 and channel +2.

mod camera_callback;
mod chain;
mod executor;
mod marker;
mod particle_callback;
mod pickup_callback;
mod process;
mod queue;
mod radius_callback;
mod replacement;
pub mod resident;
pub(crate) use replacement::{run_furniture_replacement, FurnitureReplacement};
mod resources;

pub(crate) use process::AuxiliaryRawSource;
#[cfg(test)]
pub(crate) use process::FurnitureQueryTableAlias;

pub use camera_callback::{
    furniture_camera_flash_admitted, FurnitureCameraCallbackHost, FurnitureCameraCallbackInputs,
};
pub use particle_callback::{
    run_furniture_particle_callback, FurnitureParticleCallbackHost,
    FurnitureParticleCallbackInputs, FurnitureParticleEffect,
};
pub use pickup_callback::{
    furniture_pickup_choice, run_furniture_pickup_callback, FurniturePickupCallbackHost,
    FurniturePickupCallbackInputs, FurniturePickupError,
};
pub use process::{
    FurnitureQueryRecord, LiteralRetExitRegistrationApproximation, ProcessFurnitureQueryState,
};

pub use chain::{
    queue_furniture_program, run_furniture_chain, FurnitureChainError, FurnitureChainLoan,
    FurnitureChainResources, ResidentFurnitureChainResources, FENCE_KIND,
};
pub use executor::{
    ExecutableFurnitureEffectPrograms, FurnitureCallbackContext, FurnitureCallbackRequest,
    FurnitureEffectExecutorError, FurnitureEffectExecutorHost, FurnitureExecutionPlacement,
    FurnitureExecutorOutcome,
};
pub use marker::{
    remove_furniture_chain, remove_marked_furniture, FurnitureMarkerError, MarkerFurniture,
};

pub use queue::{
    FurnitureEffectProgram, FurnitureEffectQueue, FurnitureQueueError, FurnitureQueueOutcome,
    FurnitureQueueRequest, QueuedFurnitureEffect,
};
#[cfg(test)]
pub(crate) use radius_callback::RequiredTestFurnitureRadius;
pub use radius_callback::{
    run_furniture_radius_callback, FurnitureRadiusCallbackError, FurnitureRadiusCallbackHost,
    FurnitureRadiusCallbackInputs,
};
pub use resources::{
    ExecutableFurnitureDamageResources, FurnitureDamageResourceError,
    FURNITURE_DAMAGE_TABLE_ADDRESS,
};

use inf_formats::{MissionPlacementSlot, TerrainResourceLookup};
use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    terrain_runtime::{FurnitureDamageRequest, TerrainRuntime},
};

/// One four-byte 0042A8F0 channel entry. Both fields are unsigned WORDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageResistance {
    pub threshold: u16,
    pub multiplier: u16,
}

/// 0042A8F0: negative input survives unchanged; positive damage uses a logical
/// DWORD shift after multiplication. A nonzero modifier takes product bits
/// 31..62 through IMUL/SHL/RCL. The unknown copied byte +3 is never inspected.
pub fn resisted_effect_damage(strength: i16, resistance: DamageResistance, modifier: i32) -> i32 {
    let mut damage = if strength < 0 { i32::from(strength) } else { 0 };
    if i32::from(strength) > i32::from(resistance.threshold) {
        let product = (i32::from(strength) - i32::from(resistance.threshold))
            .wrapping_mul(i32::from(resistance.multiplier));
        damage = ((product as u32) >> 8) as i32;
        if modifier != 0 {
            damage = ((i64::from(damage) * i64::from(modifier)) >> 31) as i32;
        }
    }
    damage
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureDamageRule {
    pub resistance: DamageResistance,
    /// Signed DWORD +4 of the furniture-type descriptor at 00532F88.
    pub threshold: i32,
    /// Native zero skips queueing while retaining accepted damage.
    pub program: Option<FurnitureEffectProgram>,
}

pub trait FurnitureDamageResources {
    type Error;

    /// H11 slot 0, indexed by the raw H12 lookup byte. Absence must mean a
    /// proven native null address; missing portable resources are errors.
    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<MissionPlacementSlot>, Self::Error>;

    /// H11 second WORD's low byte selects the descriptor; effect channel
    /// selects its resistance entry. The concrete resolver reads original
    /// file-backed data and rejects genuinely unmapped addresses.
    fn damage_rule(
        &mut self,
        furniture_kind: u8,
        channel: u8,
    ) -> Result<FurnitureDamageRule, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnitureDamageRejection {
    NetworkSession,
    NoFurniture,
    NullPlacement,
    SpecialLookup,
    NonpositiveDamage,
    RandomThreshold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnitureDamageOutcome {
    Rejected(FurnitureDamageRejection),
    /// 00436020 returns one even when a duplicate prevents another queued
    /// record. Queue acceptance is separate from damage acceptance.
    Accepted {
        damage: i32,
        queue: Option<FurnitureQueueOutcome>,
    },
}

impl FurnitureDamageOutcome {
    pub const fn accepted(self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

#[derive(Debug, Error)]
pub enum FurnitureDamageError<E> {
    #[error("required furniture damage resource failed")]
    Resource(E),
    #[error(transparent)]
    Queue(#[from] FurnitureQueueError),
}

/// One concrete 00436020 service context. Network state is read before H12 or
/// H11 and is supplied by the current session, never inferred from a mission.
pub struct FurnitureDamageRuntime<'a, R> {
    pub network_active: bool,
    pub resources: &'a mut R,
    pub queue: &'a mut FurnitureEffectQueue,
}

impl<R: FurnitureDamageResources> FurnitureDamageRuntime<'_, R> {
    pub fn apply(
        &mut self,
        terrain: &TerrainRuntime,
        request: FurnitureDamageRequest,
        rng: &mut u32,
    ) -> Result<FurnitureDamageOutcome, FurnitureDamageError<R::Error>> {
        use FurnitureDamageOutcome as Outcome;
        use FurnitureDamageRejection as Reject;
        if self.network_active {
            return Ok(Outcome::Rejected(Reject::NetworkSession));
        }
        let Some((_, record)) = terrain.sparse_record_at(request.xz) else {
            return Ok(Outcome::Rejected(Reject::NoFurniture));
        };
        let lookup = record.resource_lookup;
        if lookup.raw() == 0 {
            return Ok(Outcome::Rejected(Reject::NoFurniture));
        }
        // 004360C1 resolves H11 before the FD/FE/FF tests at 004360F1.
        let Some(slot) = self
            .resources
            .placement_slot(lookup)
            .map_err(FurnitureDamageError::Resource)?
        else {
            return Ok(Outcome::Rejected(Reject::NullPlacement));
        };
        if lookup.is_special() {
            return Ok(Outcome::Rejected(Reject::SpecialLookup));
        }
        let rule = self
            .resources
            .damage_rule(slot.parameter_word as u8, request.effect.channel)
            .map_err(FurnitureDamageError::Resource)?;
        let damage = resisted_effect_damage(request.effect.strength, rule.resistance, 0);
        if damage < 1 {
            return Ok(Outcome::Rejected(Reject::NonpositiveDamage));
        }
        if damage < rule.threshold {
            // 00436148 shifts/subtracts in a DWORD before signed IDIV, then
            // compares the quotient as unsigned against the RNG low WORD.
            let numerator = damage.wrapping_shl(16).wrapping_sub(damage);
            let ratio = (numerator / rule.threshold) as u32;
            let sample = name_entry_rng_next(rng) & 0xffff;
            if sample > ratio {
                return Ok(Outcome::Rejected(Reject::RandomThreshold));
            }
        }
        // 00436164 rereads/searches H12 but writes no sparse payload. The
        // result does not gate 0043620A's optional queue or the return one.
        let _ = terrain.sparse_record_at(request.xz);
        let queue = rule
            .program
            .map(|program| {
                self.queue.enqueue(
                    terrain,
                    FurnitureQueueRequest {
                        xz: request.xz,
                        program,
                    },
                )
            })
            .transpose()?;
        Ok(Outcome::Accepted { damage, queue })
    }
}

#[cfg(test)]
mod tests;
