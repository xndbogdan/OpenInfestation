//! Deaths that run inside the lifecycle (`0047D670`'s state 1) over the
//! world's owners it lent: the child factory's visuals (`0047AAF0`), bursts
//! (`0048A8B0`), radius effects (`004887D0`) and message calls
//! (`0047DD20`). The visuals' factory has no world start or camera
//! settings, which no death's visual reads; the calls have no world map,
//! which a vehicle's radar jam needs.

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    object_runtime::{
        ActorChildRequest, ActorChildResource, ActorHandle, ActorMessageWalk, EyeDeathHost,
        ObjectArena, ObjectId, Selector44DeathHost, Selector57DeathHost, Selector61DeathHost,
        EYE_DEATH_CHILD, EYE_STALK_MODEL,
    },
    process_particles::{
        audio::ParticleSoundRequest,
        births::{ParticleBirthInputs, ParticleBirthRequest, ParticleBurst},
    },
    session_platform::{
        intro::retirement::ProductionRetirementHost,
        particles::{
            damage::{birth_world_burst, WorldDamageOwners},
            radius::{apply_radius_effect, WorldRadiusOrigin},
        },
        world_actor_factory::{RequiredListenerEffects, WorldActorChildHost},
    },
    world_radius_effect::WorldRadiusEffectRequest,
};

use super::broadcast_world_actor_message;

/// `+4C(0, resource, 0)` from `parent` through a child factory over the
/// world's damage owners: their creation listeners and camera settings, and
/// no world start.
pub(crate) fn world_visual(
    owners: &mut WorldDamageOwners<'_, '_>,
    arena: &mut ObjectArena,
    parent: ObjectId,
    resource: ActorChildResource,
    rng: &mut u32,
) -> Result<(), String> {
    world_child(owners, arena, parent, resource, None, rng).map(|_| ())
}

/// `parent`'s `0047AAF0(&out, resource, position)`: the child it builds
/// (its out slot), at `position` or else at the parent.
fn world_child(
    owners: &mut WorldDamageOwners<'_, '_>,
    arena: &mut ObjectArena,
    parent: ObjectId,
    resource: ActorChildResource,
    position: Option<[u16; 3]>,
    rng: &mut u32,
) -> Result<Option<ObjectId>, String> {
    let clock = owners.animation_clock.animation_value();
    let loan = owners
        .behavior_factory
        .as_mut()
        .ok_or("a death's visual needs the world's creation listeners")?;
    let math = NativeBasisMath::new(*loan.sine);
    let mut factory = WorldActorChildHost {
        residency: owners.residency,
        executable: owners.executable,
        process: &*owners.factory_process,
        terrain: &*owners.terrain,
        world_flags: loan.world_flags,
        clock,
        sine: loan.sine,
        math: &math,
        camera_settings: owners.camera_settings,
        world_start: None,
        listeners: &mut *loan.listeners,
        listener_effects: &mut RequiredListenerEffects,
    };
    let mut out = None;
    let built = arena
        .create_actor_child(
            parent,
            ActorChildRequest { resource, position },
            Some(&mut out),
            &mut factory,
            rng,
        )
        .map_err(|error| format!("0047AAF0 visual {resource:?}: {error}"))?;
    Ok(built.and(out))
}

/// `+4C(0, resource, 0)` from `parent` through the lifecycle's factory.
fn lifecycle_visual(
    host: &mut ProductionRetirementHost<'_>,
    arena: &mut ObjectArena,
    parent: ObjectId,
    resource: u32,
) -> Result<(), String> {
    let ProductionRetirementHost { owners, rng } = host;
    let mut owners = owners
        .world_damage_owners()
        .ok_or("a death's visual needs the world's factory")?;
    world_visual(
        &mut owners,
        arena,
        parent,
        ActorChildResource::Full(resource),
        rng,
    )
}

/// `0048A8B0` over the world's particles.
fn lifecycle_burst(
    host: &mut ProductionRetirementHost<'_>,
    burst: ParticleBurst,
) -> Result<(), String> {
    let ProductionRetirementHost { owners, rng } = host;
    let mut owners = owners
        .world_damage_owners()
        .ok_or("a death's burst needs the world's damage owners")?;
    birth_world_burst(&mut owners, burst, rng).map_err(|error| format!("0048A8B0: {error}"))
}

/// The rock's death (`00453F30`).
pub(crate) struct RockDeathHost<'h, 'a> {
    pub host: &'h mut ProductionRetirementHost<'a>,
}

impl Selector44DeathHost for RockDeathHost<'_, '_> {
    type Error = String;

    fn blast(
        &mut self,
        arena: &mut ObjectArena,
        rock: ObjectId,
        block_address: u32,
        source: ActorHandle,
    ) -> Result<(), String> {
        let ProductionRetirementHost { owners, rng } = &mut *self.host;
        let mut owners = owners
            .world_damage_owners()
            .ok_or("the rock's blast needs the world's damage owners")?;
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::Actor {
                    actor: rock,
                    block_address,
                },
                damage_source: source,
                replicate: true,
            },
            &mut owners,
            rng,
        )
        .map_err(|error| format!("004887D0 rock blast {block_address:08X}: {error}"))
    }

    fn burst(&mut self, _: &mut ObjectArena, burst: ParticleBurst) -> Result<(), String> {
        lifecycle_burst(self.host, burst).map_err(|error| format!("rock {error}"))
    }

    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        rock: ObjectId,
        resource: u32,
    ) -> Result<(), String> {
        lifecycle_visual(self.host, arena, rock, resource).map_err(|error| format!("rock {error}"))
    }
}

/// The Pursuit Ship's death (`00412FA0`).
pub(crate) struct PursuitShipDeathHost<'h, 'a> {
    pub host: &'h mut ProductionRetirementHost<'a>,
}

impl Selector61DeathHost for PursuitShipDeathHost<'_, '_> {
    type Error = String;

    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        ship: ObjectId,
        resource: u32,
    ) -> Result<(), String> {
        lifecycle_visual(self.host, arena, ship, resource)
            .map_err(|error| format!("Pursuit Ship {error}"))
    }

    fn burst(&mut self, _: &mut ObjectArena, burst: ParticleBurst) -> Result<(), String> {
        lifecycle_burst(self.host, burst).map_err(|error| format!("Pursuit Ship {error}"))
    }

    fn blast(
        &mut self,
        arena: &mut ObjectArena,
        ship: ObjectId,
        block_address: u32,
        source: ActorHandle,
    ) -> Result<(), String> {
        let ProductionRetirementHost { owners, rng } = &mut *self.host;
        let mut owners = owners
            .world_damage_owners()
            .ok_or("the Pursuit Ship's blast needs the world's damage owners")?;
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::Actor {
                    actor: ship,
                    block_address,
                },
                damage_source: source,
                replicate: true,
            },
            &mut owners,
            rng,
        )
        .map_err(|error| format!("004887D0 Pursuit Ship blast {block_address:08X}: {error}"))
    }

    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        particle_type: u32,
        position: [u16; 3],
    ) -> Result<(), String> {
        let ProductionRetirementHost { owners, rng } = &mut *self.host;
        let owners = owners
            .world_damage_owners()
            .ok_or("the Pursuit Ship's explosion needs the world's damage owners")?;
        let request = crate::process_particles::births::ParticleBirthRequest {
            particle_type,
            position,
            velocity: None,
            flags: 0,
            inputs: crate::process_particles::births::ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_central(
                request,
                owners.executable,
                rng,
                &mut crate::process_sound::ProcessParticleSoundQueue {
                    sounds: owners.sounds,
                    residency: owners.residency,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0048A0F0 Pursuit Ship explosion: {error:?}"))
    }
}

/// The Powerball's death (`00486420`).
pub(crate) struct PowerballDeathHost<'h, 'a> {
    pub host: &'h mut ProductionRetirementHost<'a>,
}

impl Selector57DeathHost for PowerballDeathHost<'_, '_> {
    type Error = String;

    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        ball: ObjectId,
        resource: u32,
    ) -> Result<(), String> {
        lifecycle_visual(self.host, arena, ball, resource)
            .map_err(|error| format!("Powerball {error}"))
    }

    fn burst(&mut self, _: &mut ObjectArena, burst: ParticleBurst) -> Result<(), String> {
        lifecycle_burst(self.host, burst).map_err(|error| format!("Powerball {error}"))
    }

    fn call(
        &mut self,
        arena: &mut ObjectArena,
        ball: ObjectId,
        radius: u32,
        code: i32,
        argument: u32,
    ) -> Result<(), String> {
        let ProductionRetirementHost { owners, rng } = &mut *self.host;
        let mut owners = owners
            .world_damage_owners()
            .ok_or("the Powerball's call needs the world's damage owners")?;
        broadcast_world_actor_message(
            arena,
            ball,
            ActorMessageWalk::Radius { radius },
            code,
            argument,
            &mut owners,
            None,
            rng,
        )
        .map(|_| ())
        .map_err(|error| format!("0047DD20 Powerball call {code:#x}: {error}"))
    }
}

/// 00408560's sound: 00487F10(0x28, tip, 0x10000, 0x10000).
const EYE_DEATH_SOUND: u32 = 0x28;

/// The eyes' death (`00408560`).
pub(crate) struct EyeDeathWorldHost<'h, 'a> {
    pub host: &'h mut ProductionRetirementHost<'a>,
    pub math: NativeBasisMath,
}

impl EyeDeathHost for EyeDeathWorldHost<'_, '_> {
    type Error = String;

    fn burst(&mut self, kind: u32, position: [u16; 3], velocity: [u16; 3]) -> Result<(), String> {
        let ProductionRetirementHost { owners, rng } = &mut *self.host;
        let request = ParticleBirthRequest {
            particle_type: kind,
            position,
            velocity: Some(velocity),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: owners.phase,
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_trail(request, owners.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("00489D90 eye burst: {error:?}"))
    }

    fn stalk_length(&mut self) -> Result<Option<u16>, String> {
        crate::object_runtime::resident_model_metadata(self.host.owners.residency, EYE_STALK_MODEL)
            .map(|metadata| metadata.map(|metadata| metadata.radius_word))
            .map_err(|error| format!("the eye's stalk model: {error}"))
    }

    fn sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        let owners = &mut self.host.owners;
        owners
            .sounds
            .enqueue(
                owners.residency,
                ParticleSoundRequest {
                    sound: EYE_DEATH_SOUND,
                    position,
                    volume: 0x10000,
                    pitch: 0x10000,
                    flags: 0,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("00487F10 eye sound: {error:?}"))
    }

    fn child(
        &mut self,
        arena: &mut ObjectArena,
        eye: ObjectId,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, String> {
        let ProductionRetirementHost { owners, rng } = &mut *self.host;
        let mut owners = owners
            .world_damage_owners()
            .ok_or("the eye's child needs the world's factory")?;
        world_child(
            &mut owners,
            arena,
            eye,
            ActorChildResource::Full(EYE_DEATH_CHILD),
            Some(position),
            rng,
        )
    }

    fn basis_math(&self) -> &NativeBasisMath {
        &self.math
    }
}

#[cfg(test)]
#[path = "lifecycle_effects_tests.rs"]
mod tests;
