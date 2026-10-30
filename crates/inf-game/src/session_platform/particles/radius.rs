//! 004887D0 over genuine projectile or furniture callback sources. Both retain
//! their original EXE block VA and reread its bytes at the native consumer.
//! Particle XYZ is reread from the live cell (including scratch overwrite);
//! furniture XYZ comes from the callback's borrowed stack context. Neither
//! source falls back to the other or substitutes copied coordinates.
//!
//! Terrain always precedes actors, including zero terrain hits. H11 remains
//! lazy after network/no-furniture guards; replication requires alternate hits
//! and the caller's byte, independently of lazy network impulse settings.
//! Failures preserve reached owner writes and RNG. Direct contact is separate.

use inf_formats::{
    FrontendExecutable, MissionPlacementSlot, MissionPlacementTable, TerrainResourceLookup,
};

use super::damage::{apply_world_actor_damage, WorldDamageOwners};
use crate::{
    object_runtime::ObjectArena,
    process_particles::{ParticleId, ProcessParticleRuntime},
    terrain_runtime::{FurnitureDamageRequest, TerrainFurnitureEffectHost, TerrainRuntime},
    world_actor_radius_effect::{
        apply_actor_radius_effect, ActorRadiusDamageRequest, ActorRadiusEffectHost,
        ActorRadiusEffectOutcome, ActorRadiusEffectRequest,
    },
    world_furniture_effect::{
        resident::{resident_furniture_model_radius, resident_placement_table},
        ExecutableFurnitureDamageResources, FurnitureCallbackContext, FurnitureDamageResources,
        FurnitureDamageRule, FurnitureDamageRuntime, FurnitureEffectQueue,
    },
    world_radius_effect::{
        apply_world_radius_effect, RadiusEffectBlock, WorldRadiusEffectHost,
        WorldRadiusEffectRequest,
    },
    ResidentRuntime,
};

/// Original block identity plus the actual position owner. The borrowed
/// furniture context already includes any preceding callback's XYZ writes.
#[derive(Clone, Copy)]
pub(crate) enum WorldRadiusOrigin<'a> {
    Particle {
        particle: ParticleId,
        block_address: u32,
    },
    FurnitureCallback {
        context: &'a FurnitureCallbackContext,
        block_address: u32,
    },
    Actor {
        actor: crate::object_runtime::ObjectId,
        block_address: u32,
    },
    /// A block in the actor's own bytes or built on its caller's stack (the
    /// grenade's +13E, its burn's block), at the actor's live position. No
    /// phase's callbacks write those bytes.
    ActorBlock {
        actor: crate::object_runtime::ObjectId,
        block: [u8; 10],
    },
    /// An actor's block from a point on its caller's stack: the flat mine's
    /// burst 1500 above itself.
    PointBlock { position: [u16; 3], block: [u8; 10] },
}

impl WorldRadiusOrigin<'_> {
    fn block(self, executable: FrontendExecutable<'_>) -> Result<RadiusEffectBlock, String> {
        let block_address = match self {
            Self::Particle { block_address, .. }
            | Self::FurnitureCallback { block_address, .. }
            | Self::Actor { block_address, .. } => block_address,
            Self::ActorBlock { block, .. } | Self::PointBlock { block, .. } => {
                return Ok(RadiusEffectBlock::from_authored_bytes(block))
            }
        };
        let bytes = executable
            .mapped(block_address, 10)
            .map_err(|e| e.to_string())?
            .bytes;
        Ok(RadiusEffectBlock::from_authored_bytes(
            bytes.try_into().unwrap(),
        ))
    }

    fn position(
        self,
        particles: &ProcessParticleRuntime,
        arena: &ObjectArena,
    ) -> Result<[u16; 3], String> {
        match self {
            Self::Particle { particle, .. } => particles
                .record(particle)
                .and_then(|record| record.position())
                .map_err(|e| e.to_string()),
            Self::FurnitureCallback { context, .. } => Ok(context.position_words),
            Self::PointBlock { position, .. } => Ok(position),
            Self::Actor { actor, .. } | Self::ActorBlock { actor, .. } => arena
                .object(actor)
                .map_err(|e| e.to_string())?
                .base
                .position_words
                .ok_or_else(|| format!("radius actor {actor:?} requires its live position")),
        }
    }
}

pub(crate) fn apply_radius_effect(
    arena: &mut ObjectArena,
    request: WorldRadiusEffectRequest<WorldRadiusOrigin<'_>>,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), String> {
    apply_world_radius_effect(
        arena,
        request,
        &mut WorldRadiusHost {
            owners,
            context_lifetime: std::marker::PhantomData,
        },
        rng,
    )
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

struct WorldRadiusHost<'a, 'owners, 'exe, 'context> {
    owners: &'a mut WorldDamageOwners<'owners, 'exe>,
    // The associated Source carries this borrow; storage remains in the
    // request, not a second owner or a snapshot on this service host.
    context_lifetime: std::marker::PhantomData<&'context FurnitureCallbackContext>,
}

impl<'context> WorldRadiusEffectHost for WorldRadiusHost<'_, '_, '_, 'context> {
    type Error = String;
    type Source = WorldRadiusOrigin<'context>;

    fn furniture_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        rng: &mut u32,
    ) -> Result<u32, String> {
        self.owners
            .terrain
            .apply_furniture_radius_effect(
                request.origin,
                &mut RadiusFurnitureHost {
                    arena,
                    particles: self.owners.particles,
                    residency: self.owners.residency,
                    executable: self.owners.executable,
                    queue: self.owners.furniture,
                    process: self.owners.factory_process,
                    context_lifetime: std::marker::PhantomData,
                },
                rng,
            )
            .map_err(|error| format!("{error:?}"))
    }

    fn actor_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        rng: &mut u32,
    ) -> Result<ActorRadiusEffectOutcome, String> {
        apply_actor_radius_effect(
            arena,
            ActorRadiusEffectRequest {
                origin: request.origin,
                damage_source: request.damage_source,
            },
            self,
            rng,
        )
        .map_err(|error| format!("{error:?}"))
    }

    /// Reached only after a nonzero alternate-hit count.
    fn replicate_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        _: &mut u32,
    ) -> Result<(), String> {
        let position = request.origin.position(self.owners.particles, arena)?;
        let block = request.origin.block(self.owners.executable)?;
        let send =
            crate::world_radius_effect::radius_effect_send(request.damage_source, position, block)?;
        arena.queue_network_send(19, send);
        Ok(())
    }
}

impl<'context> ActorRadiusEffectHost for WorldRadiusHost<'_, '_, '_, 'context> {
    type Error = String;
    type Source = WorldRadiusOrigin<'context>;

    fn source_block(
        &mut self,
        _: &ObjectArena,
        source: Self::Source,
    ) -> Result<RadiusEffectBlock, String> {
        source.block(self.owners.executable)
    }
    fn source_position_words(
        &mut self,
        arena: &ObjectArena,
        source: Self::Source,
    ) -> Result<[u16; 3], String> {
        source.position(self.owners.particles, arena)
    }
    fn apply_damage(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorRadiusDamageRequest,
        rng: &mut u32,
    ) -> Result<(), String> {
        apply_world_actor_damage(
            arena,
            request.actor,
            request.effect.into(),
            request.source,
            self.owners,
            rng,
        )
        .map_err(|error| format!("{error:?}"))
    }
    fn network_active(&mut self) -> Result<bool, String> {
        Ok(self.owners.factory_process.network_flag != 0)
    }
    /// Settings `+10` (`00463AB0`), the game's mode.
    fn network_role(&mut self) -> Result<i32, String> {
        Ok(self.owners.factory_process.settings[0x10 / 4] as i32)
    }
}

struct RadiusFurnitureHost<'a, 'exe, 'context> {
    arena: &'a ObjectArena,
    particles: &'a ProcessParticleRuntime,
    residency: &'a ResidentRuntime,
    executable: FrontendExecutable<'exe>,
    queue: &'a mut FurnitureEffectQueue,
    process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
    context_lifetime: std::marker::PhantomData<&'context FurnitureCallbackContext>,
}

impl<'context> TerrainFurnitureEffectHost for RadiusFurnitureHost<'_, '_, 'context> {
    type Error = String;
    type Source = WorldRadiusOrigin<'context>;

    fn source_block(&mut self, source: Self::Source) -> Result<RadiusEffectBlock, String> {
        source.block(self.executable)
    }
    fn source_position_words(&mut self, source: Self::Source) -> Result<[u16; 3], String> {
        source.position(self.particles, self.arena)
    }
    fn furniture_model_radius(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<u16>, String> {
        resident_furniture_model_radius(self.residency, lookup).map_err(|error| error.to_string())
    }
    fn apply_furniture_damage(
        &mut self,
        terrain: &mut TerrainRuntime,
        request: FurnitureDamageRequest,
        rng: &mut u32,
    ) -> Result<bool, String> {
        FurnitureDamageRuntime {
            network_active: self.process.network_flag != 0,
            resources: &mut ProjectileFurnitureResources::new(self.residency, self.executable),
            queue: self.queue,
        }
        .apply(terrain, request, rng)
        .map(|outcome| outcome.accepted())
        .map_err(|error| format!("{error:?}"))
    }
}

///436020 resolves H11 only after network/H12 admission. Keep that lookup inside
///the actual resource callback so absent resources cannot preempt those gates.
pub(in crate::session_platform) struct ProjectileFurnitureResources<'a, 'exe> {
    residency: &'a ResidentRuntime,
    executable: FrontendExecutable<'exe>,
    resolved_table: Option<&'a MissionPlacementTable>,
}
impl<'a, 'exe> ProjectileFurnitureResources<'a, 'exe> {
    pub(in crate::session_platform) fn new(
        residency: &'a ResidentRuntime,
        executable: FrontendExecutable<'exe>,
    ) -> Self {
        Self {
            residency,
            executable,
            resolved_table: None,
        }
    }
}
impl FurnitureDamageResources for ProjectileFurnitureResources<'_, '_> {
    type Error = String;
    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<MissionPlacementSlot>, String> {
        let table = resident_placement_table(self.residency).map_err(|e| e.to_string())?;
        self.resolved_table = Some(table);
        ExecutableFurnitureDamageResources {
            placement_table: table,
            executable: self.executable,
        }
        .placement_slot(lookup)
        .map_err(|e| e.to_string())
    }
    fn damage_rule(
        &mut self,
        furniture_kind: u8,
        channel: u8,
    ) -> Result<FurnitureDamageRule, String> {
        let table = self
            .resolved_table
            .ok_or("furniture damage rule reached before H11 resolution")?;
        ExecutableFurnitureDamageResources {
            placement_table: table,
            executable: self.executable,
        }
        .damage_rule(furniture_kind, channel)
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod source_tests;
