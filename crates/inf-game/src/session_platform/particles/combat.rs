//! Live projectile collision, damage and deletion services. All mutable owners
//! are borrowed from the current world and process; none are copied for a shot.

use inf_formats::FrontendExecutable;

use crate::{
    frontend_camera::AnimationClockSource,
    model_process::ModelProcessOwners,
    object_runtime::{ActorDamagePacket, ActorHandle, ObjectArena, ProcessActorBehaviorTables},
    process_particles::{
        audio::ParticleSoundRequest,
        factories::{
            ParticleCreationNotice, ParticleDeleteRequest, ParticleFactoryContext,
            ParticleFactoryHost, ParticleFactoryImpactRequest, ParticleFactorySources,
        },
        impact::collision::{
            ParticleActorCollisionCandidate, ParticleActorGeometryRequest,
            ParticleCollisionContext, ParticleFurnitureContact, ParticleProjectileHost,
        },
        update::{ParticleUpdateError, ParticleUpdateHost, ParticleUpdateRequest},
        ParticleId, ProcessParticleRuntime,
    },
    process_sound::ProcessSoundRuntime,
    process_tasks::ProcessTaskRuntime,
    session::SessionState,
    terrain_furniture::draw::FurnitureDrawState,
    terrain_runtime::FurnitureDamageRequest,
    world_actor_terrain::WorldTerrainSource,
    world_furniture_effect::{
        FurnitureDamageRuntime, FurnitureEffectQueue, ProcessFurnitureQueryState,
    },
    world_mission::WorldMission,
    world_projectile_collision::ProjectileGeometryOwners,
    world_projectile_furniture::WorldProjectileFurnitureQuery,
    world_radius_effect::ScaledRadiusEffect,
    world_statistics::WorldStatistics,
    ResidentRuntime,
};

pub(in crate::session_platform) struct WorldProjectileHost<'a> {
    pub arena: &'a mut ObjectArena,
    pub mission: &'a mut WorldMission,
    pub tasks: &'a mut ProcessTaskRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub furniture: &'a mut FurnitureEffectQueue,
    pub residency: &'a ResidentRuntime,
    /// Pelagia's borrowed Command Centre bank, for actor models the
    /// world's own does not register (its ships' 665).
    pub model_fallback: Option<&'a crate::ResidentRuntime>,
    pub model_process: &'a ModelProcessOwners,
    pub furniture_query: &'a mut ProcessFurnitureQueryState,
    pub furniture_draw: &'a FurnitureDrawState,
    pub terrain_source: &'a WorldTerrainSource,
    pub behavior_tables: &'a mut ProcessActorBehaviorTables,
    pub animation_clock: &'a dyn AnimationClockSource,
    pub session: &'a SessionState,
    pub listeners: &'a mut crate::object_runtime::CreationListeners,
    pub sine: &'a [u16; 4096],
    pub world_flags: u32,
    pub contacts: usize,
    pub impact_births: usize,
    pub ground_bursts: usize,
    /// The grant text a reached new grant formats (004750C0); None stops it.
    pub grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
    /// Non-native: the guns' bolts that met furniture this tick, for the
    /// crystals' flares.
    pub furniture_hits: &'a mut Vec<crate::crystal_flares::FurnitureHit>,
}

impl ParticleUpdateHost for WorldProjectileHost<'_> {
    type Error = String;

    fn impact_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), String> {
        self.sounds
            .enqueue(self.residency, request)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn collide_projectile(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        movement: [u16; 3],
        radius: i32,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<bool, ParticleUpdateError<String>> {
        //004884F0 builds this temporary list in the current G+48 order
        //(a node each through 0047DAC0, all freed by 0047DA30). Only
        //builder fields are copied; precise queries and consequences are live.
        let candidates = self
            .arena
            .actor_list()
            .iter()
            .filter_map(|&id| {
                let candidate = (|| -> Result<_, String> {
                    // The failed dynamic factory already reported this object.
                    // Its retained prefix is not an admitted combat target.
                    if self
                        .arena
                        .construction_is_stopped(id)
                        .map_err(|e| e.to_string())?
                    {
                        return Ok(None);
                    }
                    let base = &self.arena.object(id).map_err(|e| e.to_string())?.base;
                    if base.flags & 4 == 0 || base.word_28 == 0 {
                        return Ok(None);
                    }
                    let actor = self.arena.actor(id).map_err(|e| e.to_string())?;
                    Ok(Some(ParticleActorCollisionCandidate {
                        actor: actor.handle,
                        flags_2c: base.flags,
                        radius_28: base.word_28,
                        position_32: base
                            .position_words
                            .ok_or_else(|| format!("projectile candidate {id:?} has no XYZ"))?,
                        source_84: actor.handle.raw(),
                    }))
                })();
                candidate.transpose()
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(ParticleUpdateError::CollisionService)?;
        let hit = particles.collide_projectile_ordered(
            id,
            movement,
            radius,
            &candidates,
            request,
            executable,
            rng,
            self,
        )?;
        self.contacts += usize::from(hit);
        Ok(hit)
    }

    fn delete_projectile(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleUpdateError<String>> {
        particles
            .delete_factory_particle(
                ParticleDeleteRequest {
                    particle: id,
                    flags: 1,
                },
                &mut ParticleFactoryContext {
                    process: request.birth_inputs.process,
                    executable,
                    rng,
                },
                self,
            )
            .map_err(|error| ParticleUpdateError::Projectile(format!("{error:?}")))
    }

    fn retire_factory_particle(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleUpdateError<String>> {
        let record = particles.record(id)?;
        if record.read_dword(0x3c)? == 1 {
            // 004661CD: 00460310(0x36, &particle+4, NULL, -1, NULL, 0054E894),
            // the blast a spent grenade's spark leaves where it lands.
            let position = record.position()?;
            let terrain = request.terrain.as_ref().ok_or_else(|| {
                ParticleUpdateError::RetirementFactory("the blast needs the world's terrain".into())
            })?;
            let math = inf_formats::native_basis::NativeBasisMath::new(*self.sine);
            return crate::session_platform::world_actor_factory::WorldActorChildHost {
                residency: self.residency,
                executable,
                process: &*request.birth_inputs.process,
                terrain: &*terrain.terrain,
                world_flags: self.world_flags,
                clock: self.animation_clock.animation_value(),
                sine: self.sine,
                math: &math,
                camera_settings: None,
                world_start: None,
                listeners: &mut *self.listeners,
                listener_effects: &mut crate::session_platform::world_actor_factory::RequiredListenerEffects,
            }
            .create_object(
                self.arena,
                crate::session_platform::world_actor_factory::WorldActorFactoryRequest {
                    resource: crate::object_runtime::ActorChildResource::Full(0x36),
                    position: crate::session_platform::world_actor_factory::WorldFactoryPosition::Local(position),
                    spawn_mode_1: false,
                    network_handle: None,
                    initialization:
                        crate::session_platform::world_actor_factory::WorldFactoryInitialization::Bare,
                },
                rng,
            )
            .map(|_| ())
            .map_err(|error| {
                ParticleUpdateError::RetirementFactory(format!("004661CD 00460310(0x36): {error}"))
            });
        }
        particles
            .retire_factory_particle(
                id,
                request,
                executable,
                rng,
                &mut crate::process_sound::ProcessParticleSoundQueue {
                    sounds: self.sounds,
                    residency: self.residency,
                },
            )
            .map_err(|error| error.map_service(|error| format!("{error:?}")))
    }
}

impl ParticleFactoryHost for WorldProjectileHost<'_> {
    type Error = String;
    fn current_phase(&mut self) -> Result<Option<u32>, String> {
        Ok(Some(u32::from(self.session.current_phase().id())))
    }
    fn network_notice(
        &mut self,
        notice: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), String> {
        crate::process_particles::factories::queue_creation_notice(particles, notice, sources)
            .map_err(String::from)
    }
    fn impact_489600(
        &mut self,
        request: ParticleFactoryImpactRequest,
        particles: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), String> {
        self.impact_births += usize::from(super::factory::birth_factory_impact(
            request,
            particles,
            context,
            self.session,
        )?);
        Ok(())
    }
}

impl ParticleProjectileHost for WorldProjectileHost<'_> {
    fn actor_geometry(
        &mut self,
        request: ParticleActorGeometryRequest,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<bool, String> {
        let actor = self
            .arena
            .resolve_actor_handle(request.actor)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                format!(
                    "retained projectile candidate {:?} was removed",
                    request.actor
                )
            })?;
        ProjectileGeometryOwners {
            residency: self.residency,
            model_fallback: self.model_fallback,
            model_process: self.model_process,
            furniture_query: self.furniture_query,
        }
        .actor_contact(
            crate::object_runtime::ObjectModelCallbacks {
                arena: self.arena,
                source: actor,
                animation: self.animation_clock,
                executable: context.executable,
            },
            request.center,
            request.radius,
        )
        .map_err(|error| format!("actor {actor:?} precise projectile geometry: {error}"))
    }
    fn furniture_query(
        &mut self,
        center: [u16; 3],
        radius: i32,
        wave_clock: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<Option<ParticleFurnitureContact>, String> {
        WorldProjectileFurnitureQuery {
            residency: self.residency,
            source: self.terrain_source,
            terrain: context
                .request
                .terrain
                .as_ref()
                .ok_or("projectile furniture query requires terrain")?
                .terrain,
            model_process: self.model_process,
            process: self.furniture_query,
            furniture: self.furniture_draw,
        }
        .query(center, radius, wave_clock)
        .map(|hit| hit.map(|hit| hit.contact))
        .map_err(|error| error.to_string())
    }
    fn actor_impact(
        &mut self,
        particle: ParticleId,
        target: ActorHandle,
        packet: u32,
        source: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), String> {
        let actor = self
            .arena
            .resolve_actor_handle(target)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("projectile hit lost actor {target:?}"))?;
        let bytes = context
            .executable
            .mapped(packet, 4)
            .map_err(|e| e.to_string())?
            .bytes;
        let before = self.arena.actor(actor).map_err(|e| e.to_string())?.health;
        super::damage::apply_world_actor_damage(
            self.arena,
            actor,
            ActorDamagePacket {
                strength: i16::from_le_bytes([bytes[0], bytes[1]]),
                channel: bytes[2],
                byte_03: Some(bytes[3]),
            },
            ActorHandle::from_raw(source),
            &mut super::damage::WorldDamageOwners {
                grant_text: self.grant_text,
                camera_settings: None,
                flash: None,
                mission: self.mission,
                tasks: self.tasks,
                terrain: context
                    .request
                    .terrain
                    .as_mut()
                    .ok_or("projectile damage requires terrain")?
                    .terrain,
                particles: context.particles,
                sounds: self.sounds,
                furniture: self.furniture,
                residency: self.residency,
                executable: context.executable,
                factory_process: context.request.birth_inputs.process,
                behavior_tables: self.behavior_tables,
                animation_clock: self.animation_clock,
                session: self.session,
                particle_load_factor: context.request.load_factor as u32,
                behavior_factory: Some(super::damage::BehaviorFactoryLoan {
                    listeners: &mut *self.listeners,
                    sine: self.sine,
                    world_flags: self.world_flags,
                }),
                map: None,
            },
            context.rng,
        )
        .map_err(|error| format!("{error:?}"))?;
        let after = self.arena.actor(actor).map_err(|e| e.to_string())?.health;
        eprintln!("World projectile damage: clock {}, actor {target:?}, channel {}, health {before}->{after}",
            self.animation_clock.animation_value(), bytes[2]);
        //4660A7 rereads the manager statistics pointer after damage. Classification
        //then resolves the particle's current source, not its earlier argument.
        if let Some(record) = self.mission.record_0c.as_mut() {
            let source = context
                .particles
                .record(particle)
                .map_err(|e| e.to_string())?
                .read_dword(0x14)
                .map_err(|e| e.to_string())?;
            let categories = self
                .arena
                .retirement_attribution(actor, ActorHandle::from_raw(source))
                .map_err(|e| e.to_string())?;
            WorldStatistics::from_record(record).record_particle_hit(categories);
        }
        Ok(())
    }
    fn furniture_contact(
        &mut self,
        id: ParticleId,
        contact: ParticleFurnitureContact,
        at: [u16; 3],
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) {
        let bolt = context
            .particles
            .record(id)
            .and_then(|record| record.read_dword(0))
            .is_ok_and(|vtable| vtable == crate::process_particles::splashes::BOLT_VTABLE);
        if bolt {
            self.furniture_hits
                .push(crate::crystal_flares::FurnitureHit {
                    cell: contact.xz,
                    at,
                });
        }
    }
    fn furniture_impact(
        &mut self,
        contact: ParticleFurnitureContact,
        packet: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), String> {
        let bytes = context
            .executable
            .mapped(packet, 4)
            .map_err(|e| e.to_string())?
            .bytes;
        FurnitureDamageRuntime {
            network_active: context.request.birth_inputs.process.network_flag != 0,
            resources: &mut super::radius::ProjectileFurnitureResources::new(
                self.residency,
                context.executable,
            ),
            queue: self.furniture,
        }
        .apply(
            context
                .request
                .terrain
                .as_ref()
                .ok_or("projectile furniture damage requires terrain")?
                .terrain,
            FurnitureDamageRequest {
                xz: contact.xz,
                effect: ScaledRadiusEffect {
                    strength: i16::from_le_bytes([bytes[0], bytes[1]]),
                    channel: bytes[2],
                    byte_03: Some(bytes[3]),
                    impulse_magnitude: 0,
                    impulse_vector: None,
                },
            },
            context.rng,
        )
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
    }
    fn terrain_impact(
        &mut self,
        selector: u32,
        position: [u16; 3],
        strength: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), String> {
        context
            .particles
            .birth_ground_impact(
                selector,
                position,
                strength as i32,
                context.request.load_factor,
                context.request.birth_inputs.borrowed(),
                context.executable,
                context.rng,
            )
            .map_err(|error| format!("{error:?}"))?;
        self.ground_bursts += 1;
        Ok(())
    }
    fn radial_impact(
        &mut self,
        packet: u32,
        particle: ParticleId,
        source: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), String> {
        super::radius::apply_radius_effect(
            self.arena,
            crate::world_radius_effect::WorldRadiusEffectRequest {
                origin: super::radius::WorldRadiusOrigin::Particle {
                    particle,
                    block_address: packet,
                },
                damage_source: ActorHandle::from_raw(source),
                replicate: true,
            },
            &mut super::damage::WorldDamageOwners {
                grant_text: self.grant_text,
                camera_settings: None,
                flash: None,
                mission: self.mission,
                tasks: self.tasks,
                terrain: context
                    .request
                    .terrain
                    .as_mut()
                    .ok_or("projectile radius effect requires terrain")?
                    .terrain,
                particles: context.particles,
                sounds: self.sounds,
                furniture: self.furniture,
                residency: self.residency,
                executable: context.executable,
                factory_process: context.request.birth_inputs.process,
                behavior_tables: self.behavior_tables,
                animation_clock: self.animation_clock,
                session: self.session,
                particle_load_factor: context.request.load_factor as u32,
                behavior_factory: Some(super::damage::BehaviorFactoryLoan {
                    listeners: &mut *self.listeners,
                    sine: self.sine,
                    world_flags: self.world_flags,
                }),
                map: None,
            },
            context.rng,
        )
    }
}
