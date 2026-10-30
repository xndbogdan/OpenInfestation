//! Family death descriptors over the current world, resource and process
//! owners. The actor identity is retained across nested effects and factories.
use super::{WorldFamilyBehaviorError, WorldFamilyBehaviorHost};
use crate::{
    object_runtime::{
        ActorChildRequest, ActorChildResource, ActorHandle, CameraSettings, CreationListeners,
        FamilyDyingEffect, FamilyDyingHost, FamilyDyingRequest, FamilyDyingSpawnRequest,
        ObjectArena, ObjectClass, ObjectId, PickupBirthHost, PickupBirthRequest,
        Selector34DyingEffect, Selector34DyingHost, Selector34DyingRequest,
        Selector35LifecycleEffect, Selector35LifecycleHost, Selector35LifecycleRequest,
        TargetNullHandleSource,
    },
    process_particles::births::{ParticleBirthInputs, ParticleBurstRequest},
    process_sound::{ProcessParticleSoundQueue, ProcessSoundVoiceTeardown},
    session_platform::{
        particles::{
            damage::WorldDamageOwners,
            radius::{apply_radius_effect, WorldRadiusOrigin},
        },
        world_actor_factory::{
            RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
            WorldFactoryPosition,
        },
    },
    world_actor_terrain::WorldTerrainSource,
    world_furniture_effect::FurnitureEffectQueue,
    world_mission::WorldMission,
    world_radius_effect::WorldRadiusEffectRequest,
    world_start::WorldStartState,
};
use inf_formats::native_basis::{NativeBasis, NativeBasisMath};

pub(crate) struct WorldFamilyDyingContext<'a> {
    pub mission: &'a mut WorldMission,
    pub furniture: &'a mut FurnitureEffectQueue,
    pub world_start: &'a mut WorldStartState,
    pub listeners: &'a mut CreationListeners,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
    pub world_flags: u32,
    pub camera_settings: CameraSettings,
    pub up: &'a [f32; 3],
    pub flash: &'a mut crate::world_flash::WorldFlashState,
}

pub(super) enum DyingDescriptor {
    Family(FamilyDyingRequest),
    Selector34(Selector34DyingRequest),
    Selector35(Selector35LifecycleRequest),
}

impl WorldFamilyBehaviorHost<'_, '_> {
    pub(super) fn run_dying_descriptor(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: DyingDescriptor,
    ) -> Result<(), WorldFamilyBehaviorError> {
        let mut host = DyingServices {
            owners: WorldDamageOwners {
                grant_text: self.grant_text,
                camera_settings: None,
                flash: None,
                mission: self.dying.mission,
                tasks: self.tasks,
                terrain: self.terrain,
                particles: self.particles,
                sounds: self.sounds,
                furniture: self.dying.furniture,
                residency: self.residency,
                executable: self.executable,
                factory_process: self.factory_process,
                behavior_tables: self.behavior_tables,
                animation_clock: self.animation_clock,
                session: self.session,
                particle_load_factor: self.particle_load_factor,
                // The dying arms reach no target selection.
                behavior_factory: None,
                map: None,
            },
            terrain_source: self.terrain_source,
            sine: self.sine_quarter,
            world_start: self.dying.world_start,
            listeners: self.dying.listeners,
            voices: self.dying.voices,
            world_flags: self.dying.world_flags,
            camera_settings: self.dying.camera_settings,
            up: self.dying.up,
            flash: self.dying.flash,
        };
        let entered = match request {
            DyingDescriptor::Family(request) => {
                arena
                    .run_family_dying(actor, request, &mut host, self.process_rng)
                    .map_err(|e| WorldFamilyBehaviorError::owner("family dying descriptor", e))?;
                matches!(request, FamilyDyingRequest::Change { entering } if entering as u8 != 0)
            }
            DyingDescriptor::Selector34(request) => {
                arena
                    .run_selector34_dying(actor, request, &mut host, self.process_rng)
                    .map_err(|e| {
                        WorldFamilyBehaviorError::owner("selector34 dying descriptor", e)
                    })?;
                matches!(request, Selector34DyingRequest::Change { entering } if entering as u8 != 0)
            }
            DyingDescriptor::Selector35(request) => {
                arena
                    .run_selector35_lifecycle(actor, request, &mut host, self.process_rng)
                    .map_err(|e| {
                        WorldFamilyBehaviorError::owner("selector35 lifecycle descriptor", e)
                    })?;
                request == (Selector35LifecycleRequest::DyingChange { entering: true })
            }
        };
        if entered {
            eprintln!(
                "World actor dying: clock {}, {actor:?} family descriptor entry completed",
                self.animation_clock.animation_value()
            );
        }
        Ok(())
    }
}

impl Selector34DyingHost for DyingServices<'_, '_> {
    type Error = String;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: Selector34DyingEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        let effect = match effect {
            Selector34DyingEffect::StatusVisual { microseconds } => {
                FamilyDyingEffect::StatusVisual { microseconds }
            }
            Selector34DyingEffect::ChangeState(request) => FamilyDyingEffect::ChangeState(request),
        };
        FamilyDyingHost::effect(self, arena, actor, effect, rng)
    }
}

impl Selector35LifecycleHost for DyingServices<'_, '_> {
    type Error = String;
    fn particle_load_factor(&mut self) -> Result<u32, String> {
        Ok(self.owners.particle_load_factor)
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: Selector35LifecycleEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        match effect {
            Selector35LifecycleEffect::Burst(burst) => {
                FamilyDyingHost::effect(self, arena, actor, FamilyDyingEffect::Burst(burst), rng)
            }
            Selector35LifecycleEffect::Radius {
                block_address,
                source,
            } => FamilyDyingHost::effect(
                self,
                arena,
                actor,
                FamilyDyingEffect::Radius {
                    block_address,
                    source,
                },
                rng,
            ),
            Selector35LifecycleEffect::ChangeState(request) => FamilyDyingHost::effect(
                self,
                arena,
                actor,
                FamilyDyingEffect::ChangeState(request),
                rng,
            ),
            Selector35LifecycleEffect::Trail {
                kind,
                position,
                velocity,
            } => self
                .owners
                .particles
                .birth_trail(
                    crate::process_particles::births::ParticleBirthRequest {
                        particle_type: kind,
                        position,
                        velocity: Some(velocity),
                        flags: 0,
                        inputs: ParticleBirthInputs {
                            network_active: self.owners.factory_process.network_flag != 0,
                            phase: Some(u32::from(self.owners.session.current_phase().id())),
                            process: self.owners.factory_process,
                        },
                    },
                    self.owners.executable,
                    rng,
                )
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Selector35LifecycleEffect::TerrainScatter => {
                // 47E190 captures X/Z before its H12 getter. The admitted
                // prepared resource getter does not mutate actor coordinates.
                let position_words = arena
                    .object(actor)
                    .map_err(|e| e.to_string())?
                    .base
                    .position_words
                    .ok_or_else(|| {
                        format!("selector35 {actor:?} requires current terrain-scatter position")
                    })?;
                let current = self
                    .owners
                    .residency
                    .resource(12, 0)
                    .ok_or("selector35 terrain scatter requires current H12")?;
                if !self.terrain_source.is_current(&current) {
                    return Err("selector35 terrain scatter prepared H12 replaced".into());
                }
                self.owners.terrain.scatter_cell_flags(
                    crate::terrain_runtime::TerrainFlagScatterRequest {
                        position_words,
                        count_minus_one: 0,
                    },
                    rng,
                );
                Ok(())
            }
        }
    }
}

/// 0043D0F0's Dormant exit during a world's cleanup walk, which pauses for
/// it (`WorldCleanupError::DestructionExitPending`): `0043E120(false)` over
/// the world's live owners.
pub(crate) fn run_selector35_dormant_exit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), String> {
    arena
        .exit_selector35_dormant_for_destruction(actor, &mut DormantExitServices { owners }, rng)
        .map_err(|error| error.to_string())
}

/// The burst and radius `0043E120` reaches; its false exit reaches nothing else.
struct DormantExitServices<'b, 'a, 'exe> {
    owners: &'b mut WorldDamageOwners<'a, 'exe>,
}

impl TargetNullHandleSource for DormantExitServices<'_, '_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.owners.factory_process.process_null_handle
    }
}

impl Selector35LifecycleHost for DormantExitServices<'_, '_, '_> {
    type Error = String;
    fn particle_load_factor(&mut self) -> Result<u32, String> {
        Ok(self.owners.particle_load_factor)
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: Selector35LifecycleEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        match effect {
            Selector35LifecycleEffect::Burst(burst) => {
                crate::session_platform::particles::damage::birth_world_burst(
                    self.owners,
                    burst,
                    rng,
                )
            }
            Selector35LifecycleEffect::Radius {
                block_address,
                source,
            } => apply_radius_effect(
                arena,
                WorldRadiusEffectRequest {
                    origin: WorldRadiusOrigin::Actor {
                        actor,
                        block_address,
                    },
                    damage_source: source,
                    replicate: true,
                },
                self.owners,
                rng,
            ),
            other => Err(format!("0043E120's exit reaches no {other:?}")),
        }
    }
}

struct DyingServices<'a, 'exe> {
    owners: WorldDamageOwners<'a, 'exe>,
    terrain_source: &'a WorldTerrainSource,
    sine: &'a [u16; 4096],
    world_start: &'a mut WorldStartState,
    listeners: &'a mut CreationListeners,
    voices: &'a mut dyn ProcessSoundVoiceTeardown,
    world_flags: u32,
    camera_settings: CameraSettings,
    up: &'a [f32; 3],
    flash: &'a mut crate::world_flash::WorldFlashState,
}

impl DyingServices<'_, '_> {
    fn factory<'a>(
        &'a mut self,
        math: &'a NativeBasisMath,
        effects: &'a mut RequiredListenerEffects,
    ) -> WorldActorChildHost<'a, 'a> {
        WorldActorChildHost {
            residency: self.owners.residency,
            executable: self.owners.executable,
            process: self.owners.factory_process,
            terrain: self.owners.terrain,
            world_flags: self.world_flags,
            clock: self.owners.animation_clock.animation_value(),
            sine: self.sine,
            math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: self.listeners,
            listener_effects: effects,
        }
    }
    fn create(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        let math = NativeBasisMath::new(*self.sine);
        self.factory(&math, &mut RequiredListenerEffects)
            .create_object(
                arena,
                WorldActorFactoryRequest {
                    resource: ActorChildResource::Full(resource),
                    position: WorldFactoryPosition::Local(position),
                    spawn_mode_1: false,
                    network_handle: None,
                    initialization: crate::session_platform::world_actor_factory::WorldFactoryInitialization::Bare,
                },
                rng,
            )
            .map_err(|e| e.to_string())
    }
}

impl TargetNullHandleSource for DyingServices<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.owners.factory_process.process_null_handle
    }
}

impl FamilyDyingHost for DyingServices<'_, '_> {
    type Error = String;
    fn read_dword(&mut self, address: u32) -> Result<u32, String> {
        // Authored death timers and scalar data retain EXE identity; no frame
        // capture, fixture timer or synthetic elapsed value enters this read.
        self.owners
            .executable
            .word(address)
            .map_err(|e| e.to_string())
    }
    fn stage_change(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        entering: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        arena
            .run_family_dying(actor, FamilyDyingRequest::Stage { entering }, self, rng)
            .map_err(|e| e.to_string())
    }
    fn sound_present(&mut self, sound: i16) -> Result<bool, String> {
        Ok(u16::try_from(sound)
            .ok()
            .and_then(|slot| self.owners.residency.resource(9, slot))
            .is_some())
    }
    fn player_position(&mut self, arena: &mut ObjectArena) -> Result<Option<[u16; 3]>, String> {
        arena
            .first_player_primary_actor()
            .map_err(|e| e.to_string())?
            .map(|actor| {
                arena
                    .object(actor)
                    .map_err(|e| e.to_string())?
                    .base
                    .position_words
                    .ok_or_else(|| format!("first player actor {actor:?} requires position"))
            })
            .transpose()
    }
    fn create_death_object(
        &mut self,
        arena: &mut ObjectArena,
        request: FamilyDyingSpawnRequest,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        // The source handle is an argument to460310. Its admitted offline
        // mode0 construction route does not consume it for source inheritance.
        self.create(arena, request.resource, request.position, rng)
    }
    fn resolve_spawned_actor(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        Ok(matches!(
            &arena.object(object).map_err(|e| e.to_string())?.class,
            ObjectClass::Actor(_)
        )
        .then_some(object))
    }
    fn actor_basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, String> {
        arena.family4236d0_actor(actor).map_err(|e| e.to_string())?;
        Ok(arena
            .actor(actor)
            .map_err(|e| e.to_string())?
            .rotating
            .basis)
    }
    fn rotate_pickup_vector(
        &mut self,
        vector: &mut [u32; 3],
        axis_address: u32,
        angle: u32,
    ) -> Result<(), String> {
        let axis = match axis_address {
            0x0055_f280 => *self.up,
            // CRT48F630→402690 initializes the shared X unit vector; no
            // admitted writer changes this axis (see model_context policy).
            0x0055_f290 => [1.0, 0.0, 0.0],
            _ => return Err(format!("required pickup rotation axis {axis_address:08X}")),
        };
        *vector = inf_formats::native_basis::rotate_axis_vector(
            vector.map(f32::from_bits),
            axis,
            angle as i32,
            self.sine,
        )
        .map(f32::to_bits);
        Ok(())
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyDyingEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        match effect {
            FamilyDyingEffect::AttachSpawnedActor { child } => arena
                .attach_actor_effect(Some(child), self, rng)
                .map_err(|e| e.to_string()),
            FamilyDyingEffect::RefreshModel => arena
                .refresh_visual_model(actor, self.owners.residency)
                .map_err(|e| e.to_string()),
            FamilyDyingEffect::CameraEffect { strength, argument } => {
                let mut camera =
                    crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                        crate::session_platform::world_actor_factory::WorldCameraInputs {
                            residency: self.owners.residency,
                            sine_quarter: self.sine,
                            settings: self.camera_settings,
                            process: Some(self.owners.factory_process),
                            animation_clock: self.owners.animation_clock.animation_value(),
                        },
                    );
                let mut host = crate::session_platform::world_flash_host::WorldFlashCameraHost {
                    camera: camera.factory(self.owners.terrain, self.world_flags, self.listeners),
                    residency: self.owners.residency,
                };
                self.flash
                    .trigger(
                        arena,
                        crate::world_flash::WorldFlashRequest {
                            position: crate::world_flash::WorldFlashPosition::Object(actor),
                            radius: strength,
                            style_argument: argument,
                        },
                        &mut host,
                        rng,
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            FamilyDyingEffect::Burst(burst) => self
                .owners
                .particles
                .birth_burst(
                    ParticleBurstRequest {
                        burst,
                        load_factor: self.owners.particle_load_factor,
                        inputs: ParticleBirthInputs {
                            network_active: self.owners.factory_process.network_flag != 0,
                            phase: Some(u32::from(self.owners.session.current_phase().id())),
                            process: self.owners.factory_process,
                        },
                    },
                    self.owners.executable,
                    rng,
                    &mut ProcessParticleSoundQueue {
                        sounds: self.owners.sounds,
                        residency: self.owners.residency,
                    },
                )
                .map(|_| ())
                .map_err(|e| e.to_string()),
            FamilyDyingEffect::Radius {
                block_address,
                source,
            } => apply_radius_effect(
                arena,
                WorldRadiusEffectRequest {
                    origin: WorldRadiusOrigin::Actor {
                        actor,
                        block_address,
                    },
                    damage_source: source,
                    replicate: true,
                },
                &mut self.owners,
                rng,
            ),
            FamilyDyingEffect::Audio(operation) => arena
                .run_family_dying_audio(
                    actor,
                    operation,
                    self.owners.residency,
                    self.owners.sounds,
                    self.voices,
                )
                .map_err(|e| e.to_string()),
            FamilyDyingEffect::ChangeState(request) => arena
                .change_actor_state(actor, request, &mut super::dying::DyingStateZero)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            FamilyDyingEffect::StatusVisual { microseconds } => {
                let load_factor = self.owners.particle_load_factor as i32;
                let mut host = crate::session_platform::world_actor_update::WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: self.owners.grant_text,
                        camera_settings: None,
                        flash: None,
                        mission: self.owners.mission,
                        tasks: self.owners.tasks,
                        terrain: self.owners.terrain,
                        particles: self.owners.particles,
                        sounds: self.owners.sounds,
                        furniture: self.owners.furniture,
                        residency: self.owners.residency,
                        executable: self.owners.executable,
                        factory_process: self.owners.factory_process,
                        behavior_tables: self.owners.behavior_tables,
                        animation_clock: self.owners.animation_clock,
                        session: self.owners.session,
                        particle_load_factor: self.owners.particle_load_factor,
                        behavior_factory: None,
                        map: None,
                    },
                    rng,
                    sine_quarter: self.sine,
                };
                arena
                    .tick_status_visual(actor, microseconds, Some(load_factor), &mut host)
                    .map_err(|e| e.to_string())
            }
            FamilyDyingEffect::Trail {
                kind,
                velocity,
                flags,
            } => {
                let position = arena
                    .object(actor)
                    .map_err(|e| e.to_string())?
                    .base
                    .position_words
                    .ok_or_else(|| {
                        format!("dying actor {actor:?} requires current trail position")
                    })?;
                self.owners
                    .particles
                    .birth_trail(
                        crate::process_particles::births::ParticleBirthRequest {
                            particle_type: kind,
                            position,
                            velocity: Some(velocity),
                            flags,
                            inputs: ParticleBirthInputs {
                                network_active: self.owners.factory_process.network_flag != 0,
                                phase: Some(u32::from(self.owners.session.current_phase().id())),
                                process: self.owners.factory_process,
                            },
                        },
                        self.owners.executable,
                        rng,
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            FamilyDyingEffect::Pickup {
                mut position,
                kind,
                value,
            } => arena
                .birth_timed_pickup(
                    PickupBirthRequest {
                        position: &mut position,
                        kind,
                        value,
                    },
                    self,
                    rng,
                )
                .map(|_| ())
                .map_err(|e| e.to_string()),
            FamilyDyingEffect::Selector36Child => {
                let math = NativeBasisMath::new(*self.sine);
                arena
                    .spawn_selector36_child(
                        actor,
                        &mut self.factory(&math, &mut RequiredListenerEffects),
                        rng,
                    )
                    .map_err(|e| e.to_string())
            }
            FamilyDyingEffect::StageChild { resource, position } => {
                let math = NativeBasisMath::new(*self.sine);
                arena
                    .create_actor_child(
                        actor,
                        ActorChildRequest {
                            resource: ActorChildResource::Full(resource),
                            position: Some(position),
                        },
                        None,
                        &mut self.factory(&math, &mut RequiredListenerEffects),
                        rng,
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
    }
}

impl crate::object_runtime::ActorAttachmentHost for DyingServices<'_, '_> {
    type Error = String;
    fn create_attachment(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
        _source: ActorHandle,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        let math = NativeBasisMath::new(*self.sine);
        self.factory(&math, &mut RequiredListenerEffects)
            .create_object(
                arena,
                WorldActorFactoryRequest {
                    resource: ActorChildResource::Full(46),
                    position: WorldFactoryPosition::Parent(parent),
                    spawn_mode_1: false,
                    network_handle: None,
                    initialization: crate::session_platform::world_actor_factory::WorldFactoryInitialization::Bare,
                },
                rng,
            )
            .map_err(|e| e.to_string())
    }
    fn resolve_attachment_actor(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        self.resolve_spawned_actor(arena, raw)
    }
    fn update_attachment(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        let mut host = crate::session_platform::world_actor_update::WorldSelector66Host {
            residency: self.owners.residency,
            terrain: self.owners.terrain,
            terrain_source: self.terrain_source,
            particles: self.owners.particles,
            process: self.owners.factory_process,
            executable: self.owners.executable,
            phase: u32::from(self.owners.session.current_phase().id()),
        };
        arena
            .tick_selector66(actor, microseconds, &mut host, rng)
            .map_err(|e| e.to_string())
    }
}

impl PickupBirthHost for DyingServices<'_, '_> {
    type Error = String;
    fn create_pickup(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: &mut [u16; 3],
        _source: ActorHandle,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        self.create(arena, resource, *position, rng)
    }
    fn refresh_model(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), String> {
        arena
            .refresh_visual_model(object, self.owners.residency)
            .map_err(|e| e.to_string())
    }
    fn ground_height(&mut self, _arena: &mut ObjectArena, xz: [u16; 2]) -> Result<u16, String> {
        let current = self
            .owners
            .residency
            .resource(12, 0)
            .ok_or("pickup requires current H12/0")?;
        if !self.terrain_source.is_current(&current) {
            return Err("pickup H12 allocation replaced the prepared terrain".into());
        }
        //450A30 samples terrain only; the native third argument is zero.
        //This path never clamps the answer to the independent water plane.
        Ok(self.owners.terrain.height_at(xz) as u16)
    }
}
