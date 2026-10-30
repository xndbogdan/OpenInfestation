//! Production services for the player vehicle's update 00416170: the
//! process sound records for its one-shots (00487EC0) and loops
//! (00487F60/00487FC0/00487FE0), the process RNG, the status trail of
//! 0047C990, and the deep-water hit (0047C7C0 through the vehicle's +B4
//! 004212F0 and the derived damage), and the item drop (00412880: the
//! dropped actor's target search and its resource-46 attachment 00405420
//! through the world factory), the aim's sight test (0043AD90) and damage
//! predicate (0048D820), and its weapons' shots (0048D340, 0048D0D0) into
//! the behavior's fire chain. A lethal hit runs the lifecycle `0047D670`
//! to state 1 with the mission's statistics and the vehicle's death
//! `00420B90`, and so does a status burn that outlasts the health
//! (`0047CE90` from `0047C990`).

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    frontend_camera::AnimationClockSource,
    object_runtime::{
        ActorAttachmentHost, ActorChildResource, ActorDamageCallback, ActorDamageReactionEffect,
        ActorDamageReactionHost, ActorExtension, ActorHandle, ActorStateChangeEffect,
        ActorStateChangeHost, ActorStateChangeQuery, ActorStateChangeRequest, ActorTimedDamageHost,
        ActorTimedDamageRequest, ActorWeaponFireContext, ActorWeaponFireRequest,
        BehaviorParticleAdapter, BehaviorPredicateContext, BehaviorPredicateRequest,
        BehaviorRowSelection, CameraSettings, CreationListeners, DerivedActorClass,
        FamilySelectionRequest, FamilyUpdateEffect, ObjectArena, ObjectClass, ObjectError,
        ObjectId, PlayerActorLookupContext, PlayerActorLookupRequest, PlayerCommandContext,
        PlayerId, ProcessActorBehaviorTables, TargetNullHandleSource, VehicleCarryContext,
        VehicleCarryEffect, VehicleCarryHost, VehicleDamageHost, VehicleDriveHost, VehicleDropHost,
        VehicleUpdateHost, VEHICLE_COLLISION_PITCH, VEHICLE_COLLISION_SOUND,
    },
    process_particles::{
        audio::ParticleSoundRequest,
        births::{ParticleBirthInputs, ParticleBirthRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    },
    process_sound::{ProcessSoundRef, ProcessSoundRuntime, ProcessSoundVoiceTeardown},
    process_tasks::ProcessTaskRuntime,
    session::SessionState,
    session_platform::{
        intro::retirement::{lifecycle_attribute, lifecycle_query, lifecycle_selector18_statistic},
        world_actor_behavior::particles::WorldSelectionServices,
        world_actor_factory::{
            BehaviorResourceOwners, RequiredListenerEffects, WorldActorChildHost,
            WorldActorFactoryRequest, WorldFactoryInitialization, WorldFactoryPosition,
        },
    },
    terrain_runtime::TerrainRuntime,
    world_actor_terrain::WorldTerrainSource,
    world_mission::{PlacementDeathRequest, WorldMission},
    world_sight::{Sight, SightActors, WorldSightQuery},
    world_start::WorldStartState,
    ResidentRuntime,
};

/// `00405427`: the drop's attachment is H13 resource 46 (selector 66).
const DROP_ATTACHMENT_RESOURCE: u32 = 46;
/// `00417B5E`: the water splash is H13 resource 0x3B (selector 65).
const WATER_SPLASH_RESOURCE: u32 = 0x3b;

/// The take's and the drop's lower effects: the slot flash `00489A90`
/// through the particle pool and the one-shots `00487EC0` through the
/// process sounds. The vehicle update and the pair pass both lend them.
pub(crate) struct CarryEffects<'a, 'exe> {
    pub particles: &'a mut ProcessParticleRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub residency: &'a ResidentRuntime,
    pub executable: inf_formats::FrontendExecutable<'exe>,
    pub process: &'a ParticleFactoryProcessState,
    pub phase: u32,
}

impl VehicleCarryHost for CarryEffects<'_, '_> {
    type Error = String;
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: VehicleCarryEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        match effect {
            VehicleCarryEffect::Particle {
                selector,
                position,
                flags,
            } => self
                .particles
                .birth_pickup_flash(
                    ParticleBirthRequest {
                        particle_type: selector,
                        position,
                        velocity: None,
                        flags,
                        inputs: ParticleBirthInputs {
                            network_active: self.process.network_flag != 0,
                            phase: Some(self.phase),
                            process: self.process,
                        },
                    },
                    self.executable,
                    rng,
                )
                .map(|_| ())
                .map_err(|error| format!("00489A90 carry flash: {error:?}")),
            VehicleCarryEffect::Sound(request) => self
                .sounds
                .enqueue(self.residency, request)
                .map_err(|error| format!("00487EC0 carry sound: {error:?}")),
        }
    }
}

pub(crate) struct WorldVehicleHost<'a, 'exe> {
    pub sounds: &'a mut ProcessSoundRuntime,
    pub residency: &'a ResidentRuntime,
    /// Pelagia's borrowed Command Centre bank, for actor models the
    /// world's own does not register (its ships' 665).
    pub model_fallback: Option<&'a crate::ResidentRuntime>,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'exe>,
    pub factory_process: &'a ParticleFactoryProcessState,
    pub session: &'a SessionState,
    pub rng: &'a mut u32,
    /// Mission +10, which `0048E7C0` returns as the damage multiplier.
    pub difficulty_10: Option<u16>,
    /// The lifecycle's statistics (W+0C) and placement death at the
    /// vehicle's death.
    pub mission: &'a mut WorldMission,
    pub animation: &'a dyn AnimationClockSource,
    /// The capture's grant notices when the vehicle takes an actor.
    pub grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
    /// The drop's target search (`00468350`) and attachment (`00405420`).
    pub behavior_tables: &'a mut ProcessActorBehaviorTables,
    pub tasks: &'a mut ProcessTaskRuntime,
    pub terrain: &'a TerrainRuntime,
    pub terrain_source: &'a WorldTerrainSource,
    pub world_start: &'a mut WorldStartState,
    pub listeners: &'a mut CreationListeners,
    pub camera_settings: CameraSettings,
    pub sine: &'a [u16; 4096],
    pub math: &'a NativeBasisMath,
    pub world_flags: u32,
    /// The sight test's model programs and furniture.
    pub model_process: &'a crate::model_process::ModelProcessOwners,
    pub furniture_query: &'a mut crate::world_furniture_effect::ProcessFurnitureQueryState,
    pub furniture_draw: &'a crate::terrain_furniture::draw::FurnitureDrawState,
    /// Process `0055F154`, the particle load factor the hovercraft's spray
    /// scales by.
    pub load_factor: i32,
    /// The world flash a hit in a first-person view starts (`00487D70`).
    pub flash: &'a mut crate::world_flash::WorldFlashState,
}

impl WorldVehicleHost<'_, '_> {
    fn ribbon_factory(&self) -> crate::object_runtime::Selector67FactoryRequest<'_> {
        crate::object_runtime::Selector67FactoryRequest {
            runtime: self.residency,
            process: self.factory_process,
            network_active: self.factory_process.network_flag != 0,
        }
    }
}

fn position(arena: &ObjectArena, vehicle: ObjectId) -> Result<[u16; 3], ObjectError> {
    arena
        .object(vehicle)?
        .base
        .position_words
        .ok_or(ObjectError::ActorPositionUnknown(vehicle))
}

impl VehicleDriveHost for WorldVehicleHost<'_, '_> {
    type Error = String;

    fn play_sound(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<(), String> {
        let position = position(arena, vehicle).map_err(|error| error.to_string())?;
        self.sounds
            .enqueue(
                self.residency,
                ParticleSoundRequest {
                    sound,
                    position,
                    volume: gain,
                    pitch,
                    flags: 0,
                },
            )
            .map_err(|error| format!("00487EC0 sound {sound:#x}: {error:?}"))
    }

    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, String> {
        self.sounds
            .create_persistent(self.residency, sound as i16, gain, pitch, 0, || {
                position(arena, vehicle)
            })
            .map_err(|error| format!("00487F60 sound {sound:#x}: {error:?}"))
    }

    fn update_sound(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        reference: ProcessSoundRef,
        gain: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.sounds
            .records_mut()
            .update(reference, gain, pitch, Some(|| position(arena, vehicle)))
            .map_err(|error| format!("00487FC0: {error:?}"))
    }

    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.sounds
            .delete_record(reference, &mut *self.voices)
            .map_err(|error| format!("00487FE0: {error:?}"))
    }
}

impl VehicleUpdateHost for WorldVehicleHost<'_, '_> {
    fn create_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<ObjectId>, String> {
        crate::object_runtime::construct_selector67_ribbon(arena, &self.ribbon_factory(), source)
            .map_err(|error| format!("00486600 wake ribbon: {error}"))
    }
    fn extend_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        ribbon: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), String> {
        let mut host = super::selector34::WorldRibbonHost {
            factory: self.ribbon_factory(),
            executable: self.executable,
        };
        arena
            .update_selector67_from_ship(ribbon, delta, point, self.executable, &mut host)
            .map_err(|error| format!("004869B0 wake ribbon: {error}"))
    }
    #[track_caller]
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(self.rng))
    }

    fn world_camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, String> {
        let mut context =
            crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                crate::session_platform::world_actor_factory::WorldCameraInputs {
                    residency: self.residency,
                    sine_quarter: self.sine,
                    settings: self.camera_settings,
                    process: Some(self.factory_process),
                    animation_clock: self.animation.animation_value(),
                },
            );
        let mut factory = context.factory(self.terrain, self.world_flags, &mut *self.listeners);
        arena
            .world_camera_with_rng(&mut factory, Some(&mut *self.rng))
            .map_err(|error| error.to_string())
    }

    fn rng_word(&self) -> Option<u32> {
        Some(*self.rng)
    }

    fn hull_spray(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        strength: i32,
    ) -> Result<(), String> {
        let position = position(arena, vehicle).map_err(|error| error.to_string())?;
        let actor = arena.actor(vehicle).map_err(|error| error.to_string())?;
        let (velocity, basis) = (actor.words_8e_92, actor.rotating.basis);
        let plane = (self.terrain.plane_base() >> (self.terrain.plane_shift() & 31)) as i16;
        self.particles
            .emit_oriented_particles(
                crate::process_particles::oriented::OrientedParticleRequest {
                    position: &position,
                    velocity: &velocity,
                    basis: &basis,
                    strength,
                    load_factor: self.load_factor,
                    plane,
                    sine_quarter: self.sine,
                },
                ParticleBirthInputs {
                    network_active: self.factory_process.network_flag != 0,
                    phase: Some(u32::from(self.session.current_phase().id())),
                    process: self.factory_process,
                },
                self.executable,
                self.rng,
            )
            .map_err(|error| format!("00488E90 hull spray: {error:?}"))
    }

    fn water_splash(&mut self, arena: &mut ObjectArena, vehicle: ObjectId) -> Result<(), String> {
        let mut factory = WorldActorChildHost {
            residency: self.residency,
            executable: self.executable,
            process: self.factory_process,
            terrain: self.terrain,
            world_flags: self.world_flags,
            clock: self.animation.animation_value(),
            sine: self.sine,
            math: self.math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: &mut *self.listeners,
            listener_effects: &mut RequiredListenerEffects,
        };
        arena
            .create_actor_child(
                vehicle,
                crate::object_runtime::ActorChildRequest {
                    resource: ActorChildResource::Full(WATER_SPLASH_RESOURCE),
                    position: None,
                },
                None,
                &mut factory,
                self.rng,
            )
            .map(|_| ())
            .map_err(|error| format!("0047AAF0 water splash: {error}"))
    }

    fn wake_particle(
        &mut self,
        kind: crate::object_runtime::WakeParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String> {
        let request = ParticleBirthRequest {
            particle_type: 0,
            position,
            velocity: Some(velocity),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: self.factory_process.network_flag != 0,
                phase: Some(u32::from(self.session.current_phase().id())),
                process: self.factory_process,
            },
        };
        match kind {
            crate::object_runtime::WakeParticle::Splash => self
                .particles
                .birth_impact(request, self.executable, self.rng)
                .map(|_| ()),
            crate::object_runtime::WakeParticle::Thrown => self
                .particles
                .birth_material_sprite(request, true, self.executable, self.rng)
                .map(|_| ()),
        }
        .map_err(|error| format!("00422400 wake: {error:?}"))
    }

    fn tow_beam(&mut self, particle_type: u32, position: [u16; 3]) -> Result<(), String> {
        let request = ParticleBirthRequest {
            particle_type,
            position,
            velocity: None,
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: self.factory_process.network_flag != 0,
                phase: Some(u32::from(self.session.current_phase().id())),
                process: self.factory_process,
            },
        };
        self.particles
            .birth_short_smoke(request, self.executable, self.rng)
            .map(|_| ())
            .map_err(|error| format!("004183CC tow beam: {error:?}"))
    }

    fn status_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), String> {
        match effect {
            // 0047CE90: state 1 from the process NULL, a lethal hit's lifecycle.
            FamilyUpdateEffect::Retire => arena
                .retire_actor(
                    vehicle,
                    self.factory_process.process_null_handle,
                    &mut VehicleLifecycleHost {
                        mission: &mut *self.mission,
                        command: PlayerCommandContext {
                            process: Some(self.factory_process),
                            executable: Some(self.executable),
                            resident: Some(self.residency),
                        },
                    },
                )
                .map(|_| ())
                .map_err(|error| format!("0047CE90 vehicle status retirement: {error:?}")),
            FamilyUpdateEffect::StatusTrail {
                particle_type,
                position,
                velocity,
                flags,
            } => self
                .particles
                .birth_trail(
                    ParticleBirthRequest {
                        particle_type,
                        position,
                        velocity: Some(velocity),
                        flags,
                        inputs: ParticleBirthInputs {
                            network_active: self.factory_process.network_flag != 0,
                            phase: Some(u32::from(self.session.current_phase().id())),
                            process: self.factory_process,
                        },
                    },
                    self.executable,
                    self.rng,
                )
                .map(|_| ())
                .map_err(|error| format!("00489D90 status trail: {error:?}")),
            other => Err(format!(
                "vehicle {vehicle:?} status effect {other:?} is not ported"
            )),
        }
    }

    fn timed_damage(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), String> {
        arena
            .apply_actor_timed_damage(vehicle, request, self)
            .map_err(|error| format!("0047C7C0 deep-water hit: {error}"))
    }

    fn drop_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        index: usize,
    ) -> Result<(), String> {
        let context = self.carry_context();
        let terrain = self.terrain;
        // The drop draws through its own RNG argument, never the host's.
        let mut rng = *self.rng;
        let result = arena.drop_vehicle_item(vehicle, index, terrain, context, self, &mut rng);
        *self.rng = rng;
        result.map_err(|error| format!("00412880 vehicle drop: {error}"))
    }

    fn carry_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        item: ObjectId,
    ) -> Result<bool, String> {
        let context = self.carry_context();
        // The take draws through its own RNG argument, never the host's.
        let mut rng = *self.rng;
        let result = arena.attach_vehicle_item(vehicle, item, context, self, &mut rng);
        *self.rng = rng;
        result.map_err(|error| format!("00421810 vehicle take: {error}"))
    }

    fn player_text(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        text: u16,
    ) -> Result<(), String> {
        arena
            .deliver_player_text(player, text, self.residency)
            .map_err(|error| format!("004135D0 text {text:#x}: {error}"))
    }

    fn fire_weapon(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: ActorWeaponFireRequest,
    ) -> Result<(), String> {
        arena
            .fire_actor_weapon(
                vehicle,
                request,
                &mut ActorWeaponFireContext {
                    tables: &mut *self.behavior_tables,
                    rng: &mut *self.rng,
                    process_velocity: None,
                },
            )
            .map_err(|error| format!("0048D340 vehicle shot: {error}"))
    }

    fn queue_weapon(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        microseconds: u32,
        direction: [u32; 3],
        fraction: u32,
    ) -> Result<(), String> {
        arena
            .queue_actor_weapon_direction(
                vehicle,
                microseconds,
                direction,
                fraction,
                &mut ActorWeaponFireContext {
                    tables: &mut *self.behavior_tables,
                    rng: &mut *self.rng,
                    process_velocity: None,
                },
            )
            .map_err(|error| format!("0048D0D0 vehicle shot: {error}"))
    }

    fn sight(
        &mut self,
        arena: &ObjectArena,
        origin: [u16; 3],
        target: [u16; 3],
    ) -> Result<Sight, String> {
        WorldSightQuery {
            residency: self.residency,
            model_fallback: self.model_fallback,
            source: self.terrain_source,
            terrain: self.terrain,
            model_process: self.model_process,
            process: &mut *self.furniture_query,
            furniture: self.furniture_draw,
        }
        .sight(
            SightActors {
                arena,
                animation: self.animation,
                executable: self.executable,
            },
            origin,
            target,
        )
        .map_err(|error| format!("0043AD90 sight test: {error}"))
    }

    fn damage_predicate(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        candidate: ObjectId,
    ) -> Result<bool, String> {
        let selection = {
            let behavior = arena
                .derived_actor(vehicle)
                .map_err(|error| error.to_string())?
                .behavior_140
                .as_ref()
                .ok_or("0048D820 requires the vehicle's behavior +140")?;
            BehaviorRowSelection::from_table(behavior.table_14, behavior.byte_10, vehicle)
        };
        // As the drop's target search: a scratch copy of the process owner.
        let mut process = *self.factory_process;
        let mut context = BehaviorPredicateContext {
            tables: &mut *self.behavior_tables,
            process: &mut process,
            rng: &mut *self.rng,
        };
        let mut resources = BehaviorResourceOwners {
            residency: self.residency,
            process: *self.factory_process,
            terrain: self.terrain,
            world_flags: self.world_flags,
            clock: self.animation.animation_value(),
            sine: self.sine,
            listeners: &mut *self.listeners,
        };
        let mut services = WorldSelectionServices {
            tasks: &mut *self.tasks,
            animation_clock: self.animation,
            session: self.session,
            resources: Some(&mut resources),
        };
        let mut host = BehaviorParticleAdapter {
            particles: &mut *self.particles,
            executable: self.executable,
            services: &mut services,
        };
        arena
            .test_behavior_damage_candidate(
                BehaviorPredicateRequest {
                    selection,
                    candidate: Some(candidate),
                },
                &mut context,
                &mut host,
            )
            .map_err(|error| format!("0048D820 aim candidate: {error}"))
    }
}

impl<'a, 'exe> WorldVehicleHost<'a, 'exe> {
    /// The context outlives this borrow, so the host can lend itself next.
    fn carry_context<'c>(&self) -> VehicleCarryContext<'c>
    where
        'a: 'c,
        'exe: 'c,
    {
        VehicleCarryContext {
            process: self.factory_process,
            executable: self.executable,
            resident: self.residency,
            text_data: self.grant_text,
        }
    }
}

impl VehicleDropHost for WorldVehicleHost<'_, '_> {
    fn call_mines(
        &mut self,
        arena: &mut ObjectArena,
        rig: ObjectId,
        radius: u32,
    ) -> Result<u32, String> {
        super::message::call_world_mines(arena, rig, radius, self.factory_process)
    }

    fn select_targets(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), String> {
        DroppedTargetSearch {
            tables: &mut *self.behavior_tables,
            process: self.factory_process,
            residency: self.residency,
            terrain: self.terrain,
            world_flags: self.world_flags,
            animation: self.animation,
            sine: self.sine,
            listeners: &mut *self.listeners,
            tasks: &mut *self.tasks,
            session: self.session,
            particles: &mut *self.particles,
            executable: self.executable,
        }
        .run(arena, actor, rng)
    }

    fn attach_effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), String> {
        arena
            .attach_actor_effect(Some(actor), self, rng)
            .map_err(|error| format!("00405420 dropped actor's attachment: {error}"))
    }
}

/// The owners of a dropped actor's fresh target search (selector 12's
/// +104, `00468350`, with both arguments zero).
pub(crate) struct DroppedTargetSearch<'a, 'exe> {
    pub tables: &'a mut ProcessActorBehaviorTables,
    pub process: &'a ParticleFactoryProcessState,
    pub residency: &'a ResidentRuntime,
    pub terrain: &'a TerrainRuntime,
    pub world_flags: u32,
    pub animation: &'a dyn AnimationClockSource,
    pub sine: &'a [u16; 4096],
    pub listeners: &'a mut CreationListeners,
    pub tasks: &'a mut ProcessTaskRuntime,
    pub session: &'a SessionState,
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'exe>,
}

impl DroppedTargetSearch<'_, '_> {
    pub(crate) fn run(
        self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), String> {
        // Only the native-receipt harnesses write the process state; the
        // search borrows a scratch copy of the immutable owner.
        let mut process = *self.process;
        let mut context = BehaviorPredicateContext {
            tables: self.tables,
            process: &mut process,
            rng,
        };
        let mut resources = BehaviorResourceOwners {
            residency: self.residency,
            process: *self.process,
            terrain: self.terrain,
            world_flags: self.world_flags,
            clock: self.animation.animation_value(),
            sine: self.sine,
            listeners: self.listeners,
        };
        let mut services = WorldSelectionServices {
            tasks: self.tasks,
            animation_clock: self.animation,
            session: self.session,
            resources: Some(&mut resources),
        };
        let mut host = BehaviorParticleAdapter {
            particles: self.particles,
            executable: self.executable,
            services: &mut services,
        };
        arena
            .select_family_actor_list_targets(
                FamilySelectionRequest { actor },
                &mut context,
                &mut host,
            )
            .map(|_| ())
            .map_err(|error| format!("00468350 dropped actor's target search: {error}"))
    }
}

impl TargetNullHandleSource for WorldVehicleHost<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.factory_process.process_null_handle
    }
}

impl ActorAttachmentHost for WorldVehicleHost<'_, '_> {
    type Error = String;
    fn create_attachment(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
        _source: ActorHandle,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        WorldActorChildHost {
            residency: self.residency,
            executable: self.executable,
            process: self.factory_process,
            terrain: self.terrain,
            world_flags: self.world_flags,
            clock: self.animation.animation_value(),
            sine: self.sine,
            math: self.math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: &mut *self.listeners,
            listener_effects: &mut RequiredListenerEffects,
        }
        .create_object(
            arena,
            WorldActorFactoryRequest {
                resource: ActorChildResource::Full(DROP_ATTACHMENT_RESOURCE),
                position: WorldFactoryPosition::Parent(parent),
                spawn_mode_1: false,
                network_handle: None,
                initialization: WorldFactoryInitialization::Bare,
            },
            rng,
        )
        .map_err(|error| error.to_string())
    }
    fn resolve_attachment_actor(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        Ok(matches!(
            &arena.object(raw).map_err(|error| error.to_string())?.class,
            ObjectClass::Actor(_)
        )
        .then_some(raw))
    }
    fn update_attachment(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        let mut host = super::WorldSelector66Host {
            residency: self.residency,
            terrain: self.terrain,
            terrain_source: self.terrain_source,
            particles: &mut *self.particles,
            process: self.factory_process,
            executable: self.executable,
            phase: u32::from(self.session.current_phase().id()),
        };
        arena
            .tick_selector66(actor, microseconds, &mut host, rng)
            .map_err(|error| error.to_string())
    }
}

impl VehicleCarryHost for WorldVehicleHost<'_, '_> {
    type Error = String;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: VehicleCarryEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        CarryEffects {
            particles: &mut *self.particles,
            sounds: &mut *self.sounds,
            residency: self.residency,
            executable: self.executable,
            process: self.factory_process,
            phase: u32::from(self.session.current_phase().id()),
        }
        .effect(arena, effect, rng)
    }
}

impl ActorTimedDamageHost for WorldVehicleHost<'_, '_> {
    type Error = String;
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), String> {
        // The reaction draws through its own RNG argument, never the host's.
        let mut rng = *self.rng;
        let result = arena.react_vehicle_damage(actor, callback, &mut rng, self);
        *self.rng = rng;
        result.map_err(|error| format!("004212F0 vehicle damage: {error}"))
    }
}

impl ActorDamageReactionHost for WorldVehicleHost<'_, '_> {
    type Error = String;
    type RecentSource = ObjectId;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.factory_process.process_null_handle
    }
    fn network_active(&mut self) -> Result<bool, String> {
        Ok(self.factory_process.network_flag != 0)
    }
    fn difficulty_multiplier(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<i16, String> {
        // 0048E7C0 is MOV AX,[ECX+10]; RET.
        self.difficulty_10
            .map(|value| value as i16)
            .ok_or_else(|| "0048E7C0 mission+10 is unwritten".to_string())
    }
    fn source_bonuses(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<[i8; 6]>, String> {
        let player = arena
            .find_player_for_actor_handle(
                PlayerActorLookupRequest { handle: source },
                PlayerActorLookupContext {
                    process: Some(self.factory_process),
                },
            )
            .map_err(|error| format!("0046B090 source player: {error}"))?;
        player
            .map(|player| {
                arena
                    .world_player(player)
                    .map(|state| std::array::from_fn(|index| state.bytes_13c_143[index] as i8))
                    .map_err(|error| error.to_string())
            })
            .transpose()
    }
    fn animation_tick(&mut self) -> Result<u32, String> {
        Ok(self.animation.animation_value())
    }
    /// Settings `+10` (`00463AB0`), the game's mode.
    fn network_role(&mut self) -> Result<i32, String> {
        Ok(self.factory_process.settings[0x10 / 4] as i32)
    }
    fn local_player_byte(&mut self) -> Result<u8, String> {
        Ok(self.factory_process.local_slot)
    }
    fn excludes_status(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<bool, String> {
        // The vehicle's +68 (00520860) is 00401300.
        arena.actor(actor).map_err(|error| error.to_string())?;
        Ok(false)
    }
    fn recent_damage_source(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        // The vehicle's +64 (00520860) is 00404610, itself.
        let vehicle = matches!(
            &arena.actor(actor).map_err(|error| error.to_string())?.extension,
            ActorExtension::Derived(derived)
                if matches!(derived.class, DerivedActorClass::Selector2(_))
        );
        Ok(vehicle.then_some(actor))
    }
    fn recent_source_tick(
        &mut self,
        arena: &mut ObjectArena,
        source: ObjectId,
    ) -> Result<u32, String> {
        arena
            .vehicle_attacked_clock(source)
            .map_err(|error| format!("source+5A8: {error}"))
    }
    fn recent_source_owner_word(
        &mut self,
        arena: &mut ObjectArena,
        source: ObjectId,
    ) -> Result<u16, String> {
        arena
            .vehicle_attacker_word(source)
            .map_err(|error| format!("source+5A6: {error}"))
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: ActorDamageReactionEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        match effect {
            ActorDamageReactionEffect::Retaliation { source } => arena
                .propagate_actor_retaliation(
                    actor,
                    source,
                    self.factory_process.process_null_handle,
                    rng,
                )
                .map_err(|error| format!("0047C840 retaliation: {error:?}")),
            ActorDamageReactionEffect::Lethal { callback } => {
                let mut host = VehicleLifecycleHost {
                    mission: &mut *self.mission,
                    command: PlayerCommandContext {
                        process: Some(self.factory_process),
                        executable: Some(self.executable),
                        resident: Some(self.residency),
                    },
                };
                // Target state 1 does not reach +B0, the only consumer of
                // the packet-address argument.
                arena
                    .change_actor_state(
                        actor,
                        ActorStateChangeRequest {
                            target_state: 1,
                            argument: 0,
                            source: callback.source,
                        },
                        &mut host,
                    )
                    .map_err(|error| format!("0047D670 vehicle death: {error:?}"))
            }
            other => Err(format!(
                "the vehicle's damage reaches {other:?}, whose owners the update does not lend"
            )),
        }
    }
}

/// `0047D670` when the vehicle's own update kills it: the mission's
/// statistics and placement death, and the vehicle's +AC `00420B90`.
struct VehicleLifecycleHost<'a> {
    mission: &'a mut WorldMission,
    command: PlayerCommandContext<'a>,
}

impl ActorStateChangeHost for VehicleLifecycleHost<'_> {
    type Error = String;
    type AttributionOwner = ();

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        lifecycle_query(arena, actor, query)
    }

    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<(), String> {
        lifecycle_attribute(self.mission, arena, actor, source)
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        match effect {
            ActorStateChangeEffect::IncrementSelector18Statistic { owner: () } => {
                lifecycle_selector18_statistic(self.mission)
            }
            ActorStateChangeEffect::RecordPlacementDeath { index } => {
                let inhibit = self.mission.inhibit_byte_08;
                self.mission
                    .record_placement_death(PlacementDeathRequest {
                        index: i32::from(index),
                        inhibit_byte_08: inhibit,
                        force: 0,
                    })
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            ActorStateChangeEffect::StateOne { actor, .. }
                if arena.selector2_actor(actor).is_ok() =>
            {
                arena
                    .retire_vehicle(actor, &self.command)
                    .map_err(|error| format!("00420B90 vehicle death: {error}"))
            }
            other => Err(format!(
                "the vehicle's lifecycle reaches {other:?}, whose owner the update does not lend"
            )),
        }
    }
}

impl VehicleDamageHost for WorldVehicleHost<'_, '_> {
    fn player_process(&self) -> &ParticleFactoryProcessState {
        self.factory_process
    }
    fn collision_sound(&mut self, position: [u16; 3], volume: u32) -> Result<(), String> {
        self.sounds
            .enqueue(
                self.residency,
                ParticleSoundRequest {
                    sound: VEHICLE_COLLISION_SOUND,
                    position,
                    volume,
                    pitch: VEHICLE_COLLISION_PITCH,
                    flags: 0,
                },
            )
            .map_err(|error| format!("00487EC0 collision sound: {error:?}"))
    }
    /// 0054ED70 holds the race.
    fn selector70_alive(&mut self, arena: &ObjectArena) -> Result<bool, String> {
        Ok(arena.race().is_some())
    }
    fn hit_flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        style: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        super::super::world_flash_host::trigger_world_flash(
            self.flash,
            arena,
            super::super::world_flash_host::point_flash(
                position,
                crate::object_runtime::VEHICLE_HIT_FLASH_RADIUS,
                style,
            ),
            super::super::world_flash_host::WorldFlashCamera {
                residency: self.residency,
                sine_quarter: self.sine,
                settings: self.camera_settings,
                process: self.factory_process,
                animation_clock: self.animation.animation_value(),
                terrain: self.terrain,
                world_flags: self.world_flags,
                listeners: &mut *self.listeners,
            },
            rng,
        )
        .map_err(|error| format!("00421411 vehicle hit flash: {error}"))
    }
}
