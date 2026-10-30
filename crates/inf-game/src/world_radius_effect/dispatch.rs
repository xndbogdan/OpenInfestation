//! 004887D0: terrain/furniture, actor-radius damage, then conditional event 0x13.

use thiserror::Error;

use crate::{
    object_runtime::{ActorHandle, ObjectArena},
    world_actor_radius_effect::ActorRadiusEffectOutcome,
};

/// The original 004887D0 operands. `origin` identifies both the effect-block
/// and position allocations; it must not snapshot their contents. Required
/// services retain the distinct live rereads of 00436390 and 00479870.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldRadiusEffectRequest<S> {
    pub origin: S,
    /// Original source ActorHandle, forwarded unchanged to actor damage and
    /// the first DWORD of the conditional network record.
    pub damage_source: ActorHandle,
    /// Native byte argument: zero suppresses only the final network delivery.
    pub replicate: bool,
}

/// Complete required phases, not optional notifications. A production host
/// composes TerrainRuntime::apply_furniture_radius_effect and
/// apply_actor_radius_effect with their real data/resource/damage services.
/// Any failure preserves reached terrain, actor, shared-RNG and host writes.
pub trait WorldRadiusEffectHost {
    type Error;
    type Source: Copy;

    fn furniture_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        rng: &mut u32,
    ) -> Result<u32, Self::Error>;

    fn actor_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        rng: &mut u32,
    ) -> Result<ActorRadiusEffectOutcome, Self::Error>;

    /// Required 00492500 event 0x13 (decimal 19), mode 1. Its three-DWORD record
    /// retains the original damage source, position identity and block identity,
    /// in that order. Do not serialize values captured before earlier callbacks.
    fn replicate_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldRadiusEffectOutcome {
    /// Diagnostic receipt only: 004887D0 does not branch on 00436390's count.
    pub furniture_count: u32,
    pub alternate_hits: u32,
    pub replicated: bool,
}

#[derive(Debug, Error)]
pub enum WorldRadiusEffectError<E> {
    #[error("required terrain/furniture radius effect failed")]
    Terrain(E),
    #[error("required actor radius effect failed")]
    Actors(E),
    #[error("required radius-effect network delivery failed")]
    Network(E),
}

/// Native 004887D0 order. The terrain result never suppresses the actor scan;
/// only the actor manager's nonzero alternate-hit return and replication byte
/// admit network event 0x13. The dispatcher never reads or freezes source data.
pub fn apply_world_radius_effect<H: WorldRadiusEffectHost>(
    arena: &mut ObjectArena,
    request: WorldRadiusEffectRequest<H::Source>,
    host: &mut H,
    rng: &mut u32,
) -> Result<WorldRadiusEffectOutcome, WorldRadiusEffectError<H::Error>> {
    let furniture_count = host
        .furniture_radius_effect(arena, request, rng)
        .map_err(WorldRadiusEffectError::Terrain)?;
    let actors = host
        .actor_radius_effect(arena, request, rng)
        .map_err(WorldRadiusEffectError::Actors)?;
    let replicated = actors.alternate_hits != 0 && request.replicate;
    if replicated {
        host.replicate_radius_effect(arena, request, rng)
            .map_err(WorldRadiusEffectError::Network)?;
    }
    Ok(WorldRadiusEffectOutcome {
        furniture_count,
        alternate_hits: actors.alternate_hits,
        replicated,
    })
}

#[cfg(test)]
mod tests;
