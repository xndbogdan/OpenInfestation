//! 0048770C services the process furniture queue before actor callbacks.

use super::{PortPlatform, PortSession};
use crate::{
    frontend_camera::AnimationClockSource,
    object_runtime::{
        ActorHandle, ObjectArena, ProcessActorBehaviorTables, TargetNullHandleSource,
    },
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest, ParticleBurstRequest},
        factories::{
            ParticleEmitterFactoryRequest, ParticleFactoryContext, ParticleFactoryProcessState,
            ParticleFactorySources,
        },
        ParticleVectorBytes, ProcessParticleRuntime,
    },
    process_sound::ProcessParticleSoundQueue,
    process_tasks::ProcessTaskRuntime,
    session::SessionState,
    world_furniture_effect::{
        resident::ResidentFurnitureExecutor, FurnitureChainLoan, FurnitureParticleCallbackHost,
        FurnitureParticleCallbackInputs, FurnitureParticleEffect, FurnitureRadiusCallbackHost,
        FurnitureRadiusCallbackInputs,
    },
    world_mission::WorldMission,
    world_radius_effect::WorldRadiusEffectRequest,
    ResidentRuntime,
};
use inf_formats::FrontendExecutable;

use super::particles::{
    damage::WorldDamageOwners,
    radius::{apply_radius_effect, WorldRadiusOrigin},
};

/// Disjoint world loans. The active executor lends its queue, terrain, sound
/// pool and RNG only at the reached callback, so nested effects retain their
/// real owners and the queue's saved-next traversal remains authoritative.
pub(super) struct WorldFurnitureEffectsHost<'a, 'exe> {
    pub arena: &'a mut ObjectArena,
    pub mission: &'a mut WorldMission,
    pub tasks: &'a mut ProcessTaskRuntime,
    pub particles: &'a mut ProcessParticleRuntime,
    pub factory_process: &'a mut ParticleFactoryProcessState,
    pub behavior_tables: &'a mut ProcessActorBehaviorTables,
    pub animation_clock: &'a dyn AnimationClockSource,
    pub session: &'a SessionState,
    pub particle_load_factor: u32,
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'exe>,
    /// 004375D0's 00478E00 camera and 00487D70 world flash owners.
    pub flash: &'a mut crate::world_flash::WorldFlashState,
    pub listeners: &'a mut crate::object_runtime::CreationListeners,
    pub world_flags: u32,
    pub camera_settings: crate::object_runtime::CameraSettings,
    pub sine: &'a [u16; 4096],
    /// 00437880's pickups go through the world factory and the prepared H12.
    pub world_start: &'a mut crate::world_start::WorldStartState,
    pub terrain_source: &'a crate::world_actor_terrain::WorldTerrainSource,
    /// The grant text a reached new grant formats (004750C0); None stops it.
    pub grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
}

impl TargetNullHandleSource for WorldFurnitureEffectsHost<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.factory_process.process_null_handle
    }
}

impl FurnitureRadiusCallbackHost for WorldFurnitureEffectsHost<'_, '_> {
    type Error = String;

    fn apply_radius(
        &mut self,
        inputs: FurnitureRadiusCallbackInputs<'_>,
        source: ActorHandle,
        replicate: bool,
    ) -> Result<(), String> {
        apply_radius_effect(
            self.arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::FurnitureCallback {
                    context: inputs.context,
                    block_address: inputs.block_address,
                },
                damage_source: source,
                replicate,
            },
            &mut WorldDamageOwners {
                grant_text: self.grant_text,
                camera_settings: None,
                flash: None,
                mission: self.mission,
                tasks: self.tasks,
                terrain: inputs.terrain,
                particles: self.particles,
                sounds: inputs.sounds,
                furniture: inputs.queue,
                residency: self.residency,
                executable: self.executable,
                factory_process: self.factory_process,
                behavior_tables: self.behavior_tables,
                animation_clock: self.animation_clock,
                session: self.session,
                particle_load_factor: self.particle_load_factor,
                behavior_factory: Some(super::particles::damage::BehaviorFactoryLoan {
                    listeners: &mut *self.listeners,
                    sine: self.sine,
                    world_flags: self.world_flags,
                }),
                map: None,
            },
            inputs.rng,
        )?;
        eprintln!(
            "World furniture radius: clock {}, block {:08X}, source {:?}, position {:?}",
            self.animation_clock.animation_value(),
            inputs.block_address,
            source,
            inputs.context.position_words,
        );
        Ok(())
    }
}

impl crate::world_furniture_effect::FurniturePickupCallbackHost
    for WorldFurnitureEffectsHost<'_, '_>
{
    type Error = String;

    fn first_player_owns(&mut self, id: u16) -> Result<Option<bool>, String> {
        let Some(player) = self.arena.first_player() else {
            return Ok(None);
        };
        let mask = self
            .arena
            .world_player(player)
            .map_err(|error| error.to_string())?
            .grants_9c;
        inf_formats::grant_is_owned(&mask, u32::from(id))
            .map(Some)
            .map_err(|error| error.to_string())
    }

    fn birth_pickup(
        &mut self,
        kind: u16,
        value: u16,
        inputs: crate::world_furniture_effect::FurniturePickupCallbackInputs<'_>,
    ) -> Result<(), String> {
        let math = inf_formats::native_basis::NativeBasisMath::new(*self.sine);
        let mut host = super::world_actor_factory::WorldPickupHost {
            factory: super::world_actor_factory::WorldActorChildHost {
                residency: self.residency,
                executable: self.executable,
                process: self.factory_process,
                terrain: inputs.terrain,
                world_flags: self.world_flags,
                clock: self.animation_clock.animation_value(),
                sine: self.sine,
                math: &math,
                camera_settings: Some(self.camera_settings),
                world_start: Some(&mut *self.world_start),
                listeners: self.listeners,
                listener_effects: &mut super::world_actor_factory::RequiredListenerEffects,
            },
            terrain_source: self.terrain_source,
        };
        let position = &mut inputs.context.position_words;
        self.arena
            .birth_timed_pickup(
                crate::object_runtime::PickupBirthRequest {
                    position,
                    kind: u32::from(kind),
                    value,
                },
                &mut host,
                inputs.rng,
            )
            .map_err(|error| error.to_string())?;
        eprintln!(
            "World furniture pickup: kind {kind:#x}, value {value}, position {:?}",
            inputs.context.position_words
        );
        Ok(())
    }
}

impl crate::world_furniture_effect::FurnitureCameraCallbackHost
    for WorldFurnitureEffectsHost<'_, '_>
{
    type Error = String;

    fn camera_flash(
        &mut self,
        radius: u32,
        inputs: crate::world_furniture_effect::FurnitureCameraCallbackInputs<'_>,
    ) -> Result<(), String> {
        use crate::world_flash::{WorldFlashHost, WorldFlashPosition, WorldFlashRequest};
        let position = inputs.context.position_words;
        let mut camera = super::world_actor_factory::WorldCameraFactoryContext::new(
            super::world_actor_factory::WorldCameraInputs {
                residency: self.residency,
                sine_quarter: self.sine,
                settings: self.camera_settings,
                process: Some(&*self.factory_process),
                animation_clock: self.animation_clock.animation_value(),
            },
        );
        let mut host = super::world_flash_host::WorldFlashCameraHost {
            camera: camera.factory(inputs.terrain, self.world_flags, &mut *self.listeners),
            residency: self.residency,
        };
        // 004375DA acquires the camera before testing the argument.
        let acquired = host.acquire_camera(self.arena, inputs.rng)?;
        let arena = &*self.arena;
        let admitted = crate::world_furniture_effect::furniture_camera_flash_admitted(
            position,
            || acquired.and_then(|camera| arena.object(camera).ok()?.base.position_words),
            radius,
        )
        .ok_or("004375EB requires the acquired camera's XYZ")?;
        if admitted {
            self.flash
                .trigger(
                    self.arena,
                    WorldFlashRequest {
                        position: WorldFlashPosition::Words(position),
                        radius: 0xf00,
                        style_argument: 0,
                    },
                    &mut host,
                    inputs.rng,
                )
                .map_err(|error| error.to_string())?;
        }
        eprintln!(
            "World furniture camera flash: clock {}, argument {radius}, position {position:?}, flashed {admitted}",
            self.animation_clock.animation_value()
        );
        Ok(())
    }
}

impl FurnitureParticleCallbackHost for WorldFurnitureEffectsHost<'_, '_> {
    type Error = String;

    fn emit(
        &mut self,
        effect: FurnitureParticleEffect,
        inputs: FurnitureParticleCallbackInputs<'_>,
    ) -> Result<(), String> {
        let phase = Some(u32::from(self.session.current_phase().id()));
        match effect {
            FurnitureParticleEffect::Burst(burst) => self
                .particles
                .birth_burst(
                    ParticleBurstRequest {
                        burst,
                        load_factor: self.particle_load_factor,
                        inputs: ParticleBirthInputs {
                            network_active: self.factory_process.network_flag != 0,
                            phase,
                            process: self.factory_process,
                        },
                    },
                    self.executable,
                    inputs.rng,
                    &mut ProcessParticleSoundQueue {
                        sounds: inputs.sounds,
                        residency: self.residency,
                    },
                )
                .map_err(|error| format!("{error:?}"))?,
            FurnitureParticleEffect::Central { selector } => {
                self.particles
                    .birth_central(
                        ParticleBirthRequest {
                            particle_type: selector,
                            position: inputs.context.position_words,
                            velocity: None,
                            flags: 0,
                            inputs: ParticleBirthInputs {
                                network_active: self.factory_process.network_flag != 0,
                                phase,
                                process: self.factory_process,
                            },
                        },
                        self.executable,
                        inputs.rng,
                        &mut ProcessParticleSoundQueue {
                            sounds: inputs.sounds,
                            residency: self.residency,
                        },
                    )
                    .map_err(|error| format!("{error:?}"))?;
            }
            FurnitureParticleEffect::TerrainFlash { selector } => {
                self.particles
                    .birth_terrain_flash(
                        ParticleBirthRequest {
                            particle_type: selector,
                            position: inputs.context.position_words,
                            velocity: None,
                            flags: 0,
                            inputs: ParticleBirthInputs {
                                network_active: self.factory_process.network_flag != 0,
                                phase,
                                process: self.factory_process,
                            },
                        },
                        self.executable,
                        inputs.rng,
                    )
                    .map_err(|error| format!("{error:?}"))?;
            }
            FurnitureParticleEffect::Emitter { selector } => {
                self.particles
                    .create_emitter_particle(
                        ParticleEmitterFactoryRequest { selector, flags: 0 },
                        &mut ParticleFactoryContext {
                            process: self.factory_process,
                            rng: inputs.rng,
                            executable: self.executable,
                        },
                        &mut ParticleFactorySources {
                            position: ParticleVectorBytes::from_words(
                                inputs.context.position_words,
                            ),
                            separate_velocity: None,
                        },
                        &mut super::particles::factory::WorldParticleFactoryHost {
                            session: self.session,
                        },
                    )
                    .map_err(|error| format!("{error:?}"))?;
            }
        }
        eprintln!(
            "World furniture particles: clock {}, effect {:?}, position {:?}",
            self.animation_clock.animation_value(),
            effect,
            inputs.context.position_words
        );
        Ok(())
    }
}

impl<P: PortPlatform> PortSession<P> {
    /// A stabiliser's death (`0047653F`) writes process DWORD 005483B0 = 1
    /// inside the world tick; its lifecycle host carries the write on the
    /// mission. Nothing reads the word between the write and here, and the
    /// camera pose and the terrain draw read it after.
    pub(super) fn apply_terrain_event(&mut self) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        if std::mem::take(&mut world.mission.terrain_event_5483b0) {
            self.camera_shake_enabled = 1;
        }
    }

    /// 00450210, reached by 0048E890 after W+12 is cleared and before
    /// the separately required player/task cleanup callbacks.
    pub(super) fn clear_world_effects(&mut self) {
        self.furniture.clear();
        self.furniture_failure = None;
        self.camera_shake_enabled = 0;
    }

    pub(super) fn tick_furniture_effects(&mut self, microseconds: u32, state: &SessionState) {
        if self.furniture_failure.is_some() {
            return;
        }
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let result = (|| {
            let executable =
                inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes())
                    .map_err(|error| error.to_string())?;
            let sine = &self.catalog.assets().sine_table().values;
            let camera_settings = self.platform.world_tick_inputs().camera_settings;
            let detail = self.platform.world_tick_inputs().settings_70;
            let network_active = self.factory_process.network_flag != 0;
            let grant_text =
                super::world_owners::current_grant_text(&self.profile_name_classification);
            let mut effects = WorldFurnitureEffectsHost {
                arena: &mut world.arena,
                mission: &mut world.mission,
                tasks: &mut self.tasks,
                particles: &mut self.particles,
                factory_process: &mut self.factory_process,
                behavior_tables: &mut self.actor_behavior_tables,
                animation_clock: &self.process,
                session: state,
                particle_load_factor: world.fade.performance.particle_load_factor,
                residency: &self.residency,
                executable,
                flash: &mut world.flash,
                listeners: &mut world.listeners,
                world_flags: world.world_flags,
                camera_settings,
                sine,
                world_start: &mut world.world_start,
                terrain_source: &world.terrain_source,
                grant_text: Some(&grant_text),
            };
            crate::crash_log::step(crate::crash_log::Step::Furniture);
            self.furniture
                .tick(
                    &mut world.terrain,
                    microseconds,
                    &mut ResidentFurnitureExecutor {
                        resident: &self.residency,
                        executable,
                        sounds: &mut self.sounds,
                        process: &mut self.furniture_query,
                        effects: &mut effects,
                        chain: FurnitureChainLoan {
                            policy: &mut self.terrain_policy,
                            detail,
                            world_flags: Some(world.world_flags),
                            network_active,
                        },
                    },
                    &mut world.rng,
                )
                .map_err(|error| format!("{error:?}"))
        })();
        if let Err(reason) = result {
            eprintln!("Furniture effect feature stopped after reached writes: {reason}");
            self.furniture_failure = Some(reason);
        }
    }
}
