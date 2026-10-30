//! The grenade's and the mine's services from the world tick: the smoke
//! (00489D90 type 0x0B), the radius effect of a block (004887D0), a spent
//! grenade's spark (004665D0 type 1), the default burst's visual (its +4C)
//! and debris (0048A8B0), the explosion (0048A0F0 type 1), the messages
//! (the alert 0047DBF0, the chain 0047DD20 and the mine's word to its
//! thrower 0047DD40), and the detonation through the lifecycle 0047D670,
//! whose states 1 and 0 come back to the grenade or the mine, and the
//! burn's status visual (0047DFD0, shared with the burning actors).

use crate::{
    object_runtime::{
        ActorChildRequest, ActorChildResource, ActorHandle, ActorMessageWalk,
        ActorStateChangeEffect, ActorStateChangeHost, ActorStateChangeQuery,
        ActorStateChangeRequest, CameraSettings, CreationListenerEffects, Grenade26Host,
        ObjectArena, ObjectId,
    },
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest, ParticleBurst},
        factories::{
            ParticleCreationNotice, ParticleFactoryContext, ParticleFactoryHost,
            ParticleFactoryImpactRequest, ParticleFactoryKind, ParticleFactoryRequest,
            ParticleFactorySources, ParticleVelocitySource,
        },
        ParticleVectorBytes, ProcessParticleRuntime,
    },
    process_sound::ProcessParticleSoundQueue,
    session_platform::particles::{
        damage::{birth_world_burst, WorldDamageOwners},
        radius::{apply_radius_effect, WorldRadiusOrigin},
    },
    session_platform::world_actor_factory::WorldActorChildHost,
    world_radius_effect::WorldRadiusEffectRequest,
    world_start::WorldStartState,
};
use inf_formats::native_basis::NativeBasisMath;

/// 00455902's smoke: type 0x0B, flags 0x800.
const SMOKE: u32 = 0x0b;
const SMOKE_FLAGS: u32 = 0x800;
/// 0045649D's spark factory selector and 00456519's explosion type.
const SPARK_SELECTOR: u32 = 1;
const EXPLOSION: u32 = 1;

pub(crate) struct WorldGrenadeHost<'a, 'exe> {
    pub owners: WorldDamageOwners<'a, 'exe>,
    /// The child factory's own inputs; it borrows the owners' terrain.
    pub world_flags: u32,
    pub clock: u32,
    pub sine: &'a [u16; 4096],
    pub math: &'a NativeBasisMath,
    pub camera_settings: CameraSettings,
    pub world_start: &'a mut WorldStartState,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
    /// Settings +7C, which the mine's target refresh reads.
    pub settings_7c: u32,
    /// The inverse sine table 0053FF00, which the missile's turns read.
    pub inverse_sine: [i16; 1024],
    /// The voices a released steered missile's sound stop (00487FE0) tears
    /// down; None stops it.
    pub voices: Option<&'a mut dyn crate::process_sound::ProcessSoundVoiceTeardown>,
}

/// 004665D0(1, position, velocity, owner, 0), a spark: the grenade's when
/// it is spent, and the Alien Brain's debris.
pub(super) fn spark(
    owners: &mut WorldDamageOwners<'_, '_>,
    position: [u16; 3],
    velocity: [u16; 3],
    owner: ActorHandle,
    rng: &mut u32,
) -> Result<(), String> {
    let mut sources = ParticleFactorySources {
        position: ParticleVectorBytes::from_words(position),
        separate_velocity: Some(ParticleVectorBytes::from_words(velocity)),
    };
    let mut process = *owners.factory_process;
    let phase = Some(u32::from(owners.session.current_phase().id()));
    owners
        .particles
        .create_factory_particle(
            ParticleFactoryRequest {
                kind: ParticleFactoryKind::Factory4665D0,
                selector: SPARK_SELECTOR,
                source_handle: owner,
                flags: 0,
                velocity_source: ParticleVelocitySource::Separate,
            },
            &mut ParticleFactoryContext {
                process: &mut process,
                rng,
                executable: owners.executable,
            },
            &mut sources,
            &mut SparkServices { phase },
        )
        .map(|_| ())
        .map_err(|error| format!("004665D0 spark: {error:?}"))
}

/// 004665D0's spark reaches only the phase read.
struct SparkServices {
    phase: Option<u32>,
}

impl ParticleFactoryHost for SparkServices {
    type Error = String;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase)
    }
    fn network_notice(
        &mut self,
        notice: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        crate::process_particles::factories::queue_creation_notice(particles, notice, sources)
            .map_err(String::from)
    }
    fn impact_489600(
        &mut self,
        _: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        Err("the spark's impact 00489600".into())
    }
}

/// 0047EBF0's services for a remote grenade, charge, mine or missile: the
/// status visual's load factor and trail, as the family update's.
impl crate::object_runtime::StatusTrailHost for WorldGrenadeHost<'_, '_> {
    type Error = String;

    fn status_load_factor(&self) -> i32 {
        self.owners.particle_load_factor as i32
    }

    fn status_trail(
        &mut self,
        _: &mut ObjectArena,
        effect: crate::object_runtime::FamilyUpdateEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        let crate::object_runtime::FamilyUpdateEffect::StatusTrail {
            particle_type,
            position,
            velocity,
            flags,
        } = effect
        else {
            return Err(format!("0047EBF0 effect {effect:?}"));
        };
        let owners = &mut self.owners;
        let request = ParticleBirthRequest {
            particle_type,
            position,
            velocity: Some(velocity),
            flags,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_trail(request, owners.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("00489D90 status trail: {error:?}"))
    }
}

impl Grenade26Host for WorldGrenadeHost<'_, '_> {
    type Error = String;

    fn load_factor(&self) -> u32 {
        self.owners.particle_load_factor
    }

    fn terrain(
        &mut self,
        _: &mut ObjectArena,
    ) -> Result<&crate::terrain_runtime::TerrainRuntime, String> {
        Ok(&*self.owners.terrain)
    }

    fn water_level(&mut self, _: &mut ObjectArena) -> Result<i16, String> {
        let terrain = &*self.owners.terrain;
        Ok((terrain.plane_base() >> terrain.plane_shift()) as i16)
    }

    fn smoke(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        let owners = &mut self.owners;
        let request = ParticleBirthRequest {
            particle_type: SMOKE,
            position,
            velocity: Some(velocity),
            flags: SMOKE_FLAGS,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_trail(request, owners.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("00489D90 grenade smoke: {error:?}"))
    }

    fn alert(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        radius: u32,
        event: u32,
        team: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        super::broadcast_world_actor_message(
            arena,
            grenade,
            ActorMessageWalk::Team { radius, team },
            event as i32,
            0,
            &mut self.owners,
            // The vehicle's radar jam takes the owners' map (G+1B4).
            None,
            rng,
        )
        .map(|_| ())
        .map_err(|error| format!("0047DBF0 alert: {error}"))
    }

    fn burst(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        block: [u8; 10],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::ActorBlock {
                    actor: grenade,
                    block,
                },
                damage_source: owner,
                replicate: true,
            },
            &mut self.owners,
            rng,
        )
        .map_err(|error| format!("004887D0 grenade radius effect: {error}"))
    }

    fn spark(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        spark(&mut self.owners, position, velocity, owner, rng)
            .map_err(|error| format!("grenade {error}"))
    }

    fn spawn(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        resource: u16,
        rng: &mut u32,
    ) -> Result<(), String> {
        let loan = self
            .owners
            .behavior_factory
            .as_mut()
            .ok_or("the default burst's visual needs the world factory")?;
        let mut factory = WorldActorChildHost {
            residency: self.owners.residency,
            executable: self.owners.executable,
            process: &*self.owners.factory_process,
            terrain: &*self.owners.terrain,
            world_flags: self.world_flags,
            clock: self.clock,
            sine: self.sine,
            math: self.math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: &mut *loan.listeners,
            listener_effects: &mut *self.listener_effects,
        };
        arena
            .create_actor_child(
                grenade,
                ActorChildRequest {
                    resource: ActorChildResource::RegisterLowWord(resource),
                    position: None,
                },
                None,
                &mut factory,
                rng,
            )
            .map(|_| ())
            .map_err(|error| format!("00458D36 burst visual {resource}: {error}"))
    }

    fn debris(
        &mut self,
        _: &mut ObjectArena,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), String> {
        birth_world_burst(&mut self.owners, burst, rng)
            .map_err(|error| format!("0048A8B0 grenade debris: {error}"))
    }

    fn broadcast(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        radius: u32,
        event: u32,
        data: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        super::broadcast_world_actor_message(
            arena,
            grenade,
            ActorMessageWalk::Radius { radius },
            event as i32,
            data,
            &mut self.owners,
            // The vehicle's radar jam takes the owners' map (G+1B4).
            None,
            rng,
        )
        .map(|_| ())
        .map_err(|error| format!("0047DD20 chain: {error}"))
    }

    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        let owners = &mut self.owners;
        let request = ParticleBirthRequest {
            particle_type: EXPLOSION,
            position,
            velocity: None,
            flags: 0,
            inputs: ParticleBirthInputs {
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
                &mut ProcessParticleSoundQueue {
                    sounds: owners.sounds,
                    residency: owners.residency,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0048A0F0 grenade explosion: {error:?}"))
    }

    fn detonate(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        arena
            .change_actor_state(
                grenade,
                ActorStateChangeRequest {
                    target_state: 0,
                    argument: 0,
                    source,
                },
                &mut GrenadeLifecycle { host: self, rng },
            )
            .map_err(|error| format!("0047D670 grenade detonation: {error:?}"))
    }

    fn settings_7c(&self) -> u32 {
        self.settings_7c
    }

    fn burst_at(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
        point: [u16; 3],
        block: [u8; 10],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::PointBlock {
                    position: point,
                    block,
                },
                damage_source: owner,
                replicate: true,
            },
            &mut self.owners,
            rng,
        )
        .map_err(|error| format!("004887D0 raised mine burst: {error}"))
    }

    fn owner_message(
        &mut self,
        arena: &mut ObjectArena,
        sender: ObjectId,
        event: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        // 0047DD40 resolves +88 first; a lost owner hears nothing.
        let owner = arena
            .actor(sender)
            .map_err(|error| error.to_string())?
            .owner_handle;
        let Some(owner) = arena
            .resolve_actor_handle(owner)
            .map_err(|error| format!("0047DD40 owner: {error:?}"))?
        else {
            return Ok(());
        };
        super::send_world_actor_message(
            arena,
            sender,
            owner,
            0,
            event as i32,
            0,
            &mut self.owners,
            // The vehicle's radar jam takes the owners' map (G+1B4).
            None,
            rng,
        )
        .map(|_| ())
        .map_err(|error| format!("0047DD40 message {event:#x}: {error}"))
    }

    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        delta: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        let load_factor = Some(self.owners.particle_load_factor as i32);
        let trail = arena
            .status_visual_trail(grenade, delta, load_factor, || {
                Ok::<_, std::convert::Infallible>(crate::name_entry::name_entry_rng_next(rng))
            })
            .map_err(|error| format!("0047DFD0 burn visual: {error:?}"))?;
        let Some(trail) = trail else {
            return Ok(());
        };
        let owners = &mut self.owners;
        let request = ParticleBirthRequest {
            particle_type: trail.particle_type,
            position: trail.position,
            velocity: Some(trail.velocity),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_trail(request, owners.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("00489D90 burn visual: {error:?}"))
    }
}

/// 0047D670 on a grenade: its +68 (00404610) answers itself, so state 2
/// skips the attribution; it has no +58 component. State 1 is its +AC and
/// state 0 its +B0.
struct GrenadeLifecycle<'h, 'a, 'exe> {
    host: &'h mut WorldGrenadeHost<'a, 'exe>,
    rng: &'h mut u32,
}

impl ActorStateChangeHost for GrenadeLifecycle<'_, '_, '_> {
    type Error = String;
    type AttributionOwner = ();

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        crate::session_platform::intro::retirement::lifecycle_query(arena, actor, query)
    }

    fn attribute(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        _: ActorHandle,
    ) -> Result<(), String> {
        Err(format!("grenade {actor:?} skips the attribution (+68)"))
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        match effect {
            ActorStateChangeEffect::StateOne { actor, .. } => arena
                .grenade26_state_one(actor)
                .map_err(|error| format!("00458F80 grenade state 1: {error}")),
            ActorStateChangeEffect::StateZero { actor, .. } if arena.is_mine38(actor) => arena
                .detonate_mine38(actor, self.rng, self.host)
                .map_err(|error| format!("004566A0 mine detonation: {error}")),
            ActorStateChangeEffect::StateZero { actor, .. } => arena
                .detonate_grenade26(actor, self.rng, self.host)
                .map_err(|error| format!("004564B0 grenade detonation: {error}")),
            other => Err(format!("grenade lifecycle reaches no {other:?}")),
        }
    }
}

#[cfg(test)]
#[path = "grenade26_tests.rs"]
mod tests;
