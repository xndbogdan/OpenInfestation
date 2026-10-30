//! Actor manager radius dispatch 00479870, after terrain/furniture 00436390.
//!
//! The damage service is the complete 0047C6E0 call, including its virtual +B4
//! and later source/task/statistics effects. A missing owner returns an error;
//! this manager never replaces those effects with successful empty callbacks.

mod math;

use thiserror::Error;

use crate::{
    object_runtime::{ActorHandle, ObjectArena, ObjectError, ObjectId},
    world_radius_effect::{scale_radius_effect, RadiusEffectBlock, ScaledRadiusEffect},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorRadiusEffectRequest<S> {
    /// Identity of the original block and position allocations. Each source
    /// read remains live; the initial radius-squared alone is cached.
    pub origin: S,
    /// Original source handle passed unchanged to every 0047C6E0 call.
    pub damage_source: ActorHandle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorRadiusDamageRequest {
    pub actor: ObjectId,
    /// Copied first effect DWORD; byte +3 remains opaque if unwritten.
    pub effect: ScaledRadiusEffect,
    pub source: ActorHandle,
}

pub trait ActorRadiusEffectHost {
    type Error;
    type Source: Copy;

    fn source_block(
        &mut self,
        arena: &ObjectArena,
        source: Self::Source,
    ) -> Result<RadiusEffectBlock, Self::Error>;
    fn source_position_words(
        &mut self,
        arena: &ObjectArena,
        source: Self::Source,
    ) -> Result<[u16; 3], Self::Error>;
    /// Required complete 0047C6E0. Reached mutations remain in the arena when
    /// this returns an error. Successful calls may append/dispose actors.
    fn apply_damage(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorRadiusDamageRequest,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// Current process 0055F448, queried only by the mass >=128 impulse arm.
    fn network_active(&mut self) -> Result<bool, Self::Error>;
    /// Current settings +10 through 00463AB0; called only with active network.
    fn network_role(&mut self) -> Result<i32, Self::Error>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorRadiusEffectOutcome {
    /// Native return count: accepted hits whose current alternate flag
    /// 80000000 skipped local damage. Ordinary damage is not included.
    pub alternate_hits: u32,
}

#[derive(Debug, Error)]
pub enum ActorRadiusEffectError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor radius effect host operation failed")]
    Host(E),
    #[error("actor radius candidate {0:?} has unwritten position WORDs")]
    MissingPosition(ObjectId),
    #[error("actor radius callback destroyed current native list node {0:?}")]
    RemovedCurrentNode(ObjectId),
}

/// 00479870's complete ordered scan and impulse consumer. This walks the live
/// actor list, not a captured ID vector: later appends are reached, disposal
/// retains nodes, and removing earlier nodes does not skip the next candidate.
pub fn apply_actor_radius_effect<H: ActorRadiusEffectHost>(
    arena: &mut ObjectArena,
    request: ActorRadiusEffectRequest<H::Source>,
    host: &mut H,
    rng: &mut u32,
) -> Result<ActorRadiusEffectOutcome, ActorRadiusEffectError<H::Error>> {
    let radius = u32::from(
        host.source_block(arena, request.origin)
            .map_err(ActorRadiusEffectError::Host)?
            .outer_radius,
    );
    let radius_squared = radius.wrapping_mul(radius);
    let mut outcome = ActorRadiusEffectOutcome::default();
    let mut index = 0;
    while let Some(&actor) = arena.actor_list().get(index) {
        // Portable containment for a retained, incomplete constructor. Its
        // native node and fields remain intact, but it cannot receive damage.
        if arena.construction_is_stopped(actor)? {
            index += 1;
            continue;
        }
        let flags = arena.object(actor)?.base.flags;
        if flags & 4 != 0 {
            let position = arena
                .object(actor)?
                .base
                .position_words
                .ok_or(ActorRadiusEffectError::MissingPosition(actor))?;
            let origin = host
                .source_position_words(arena, request.origin)
                .map_err(ActorRadiusEffectError::Host)?;
            let delta =
                std::array::from_fn(|axis| position[axis].wrapping_sub(origin[axis]) as i16);
            let square = math::squared_word_distance(delta);
            if square < radius_squared {
                let mobile = flags & 8 == 0 && arena.actor(actor)?.word_8c < 0x800;
                // Direct 0048EFC0 on the signed DWORD square, then sign extend
                // low AX. Unlike terrain 00462DA0, no divide-by-four branch.
                let distance = math::direct_word_distance(square);
                let block = host
                    .source_block(arena, request.origin)
                    .map_err(ActorRadiusEffectError::Host)?;
                let effect = scale_radius_effect(block, distance, mobile.then_some(delta));
                if let Some(effect) = effect.filter(|effect| effect.accepted()) {
                    if arena.object(actor)?.base.flags & 0x8000_0000 == 0 {
                        host.apply_damage(
                            arena,
                            ActorRadiusDamageRequest {
                                actor,
                                effect,
                                source: request.damage_source,
                            },
                            rng,
                        )
                        .map_err(ActorRadiusEffectError::Host)?;
                    } else {
                        outcome.alternate_hits = outcome.alternate_hits.wrapping_add(1);
                    }
                    // 00479983 rereads mass after damage; flag8 is not retested.
                    if mobile && arena.actor(actor)?.word_8c < 0x800 {
                        let mass = arena.actor(actor)?.word_8c;
                        let scale = if mass < 128 {
                            256 // This arm consumes neither network query.
                        } else {
                            impulse_scale(host).map_err(ActorRadiusEffectError::Host)?
                        };
                        let impulse = math::impulse_vectors(
                            arena.actor(actor)?.rotating.basis,
                            delta,
                            effect
                                .impulse_vector
                                .expect("mobile falloff supplied a destination"),
                            mass,
                            scale,
                        );
                        // Native B510 precedes B470. These helpers own their
                        // WORD/DWORD wrapping and deliberately change no dirty bits.
                        arena.add_actor_linear_velocity(actor, impulse.linear)?;
                        arena.add_actor_angular_velocity(actor, impulse.angular)?;
                    }
                }
            }
        }
        // Native follows the current node's next pointer after callbacks.
        // Actual destruction of that node would invalidate the native read;
        // report the feature boundary rather than fabricate its successor.
        index = arena
            .actor_list()
            .iter()
            .position(|&candidate| candidate == actor)
            .ok_or(ActorRadiusEffectError::RemovedCurrentNode(actor))?
            + 1;
    }
    Ok(outcome)
}

fn impulse_scale<H: ActorRadiusEffectHost>(host: &mut H) -> Result<i32, H::Error> {
    if host.network_active()? {
        if host.network_role()? == 2 {
            return Ok(800);
        }
        // Native rereads network state and settings for this second branch.
        if host.network_active()? && host.network_role()? == 3 {
            return Ok(64);
        }
    }
    Ok(256)
}

#[cfg(test)]
mod tests;
