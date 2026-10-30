//! Complete world-load session phase over retained process/application loans.
use super::*;

pub(crate) enum WorldLoadPresentation<'a> {
    NoCallbacks,
    Interactive(&'a mut dyn crate::loading_integration::LoadingPresentationHost),
}

impl<P: PortPlatform> PortSession<P> {
    pub(crate) fn load_world_effect(
        &mut self,
        state: &mut SessionState,
        presentation: WorldLoadPresentation<'_>,
    ) -> Result<(), PortSessionErrorOf<P>> {
        crate::crash_log::step(crate::crash_log::Step::Loading);
        self.retain_offline_world_load(state)?;
        if self.world_rng_custody == WorldRngCustody::LoadedWorld {
            if let Some(world) = self.world.as_ref() {
                *self.platform.process_rng() = world.rng;
            }
        }
        self.world_rng_custody = WorldRngCustody::Platform;
        if self.world.is_some() && self.pending_world.is_some() {
            return Err(PortSessionError::WorldOwnerBoundary(
                "two actual world owners",
            ));
        }
        if let Some(world) = self.world.take() {
            self.pending_world = Some(world.into_load_owner());
        }
        let owner = self
            .pending_world
            .as_mut()
            .ok_or(PortSessionError::WorldOwnerBoundary(
                "LoadWorld requires its actual prepared owner",
            ))?;
        let resume_after_load = owner.mission.save.mission_selection != 22;
        // Port option: Pelagia's rework starts the ACAM out of its moved
        // return gate; every other world keeps its own start.
        owner.world_start.port_spawn = crate::pelagia::spawn(
            owner.mission.save.mission_selection,
            self.platform.prototype_world(),
            self.platform.pelagia_rework(),
        );
        let text_data = world_owners::current_grant_text(&self.profile_name_classification);
        let mut listener_effects = crate::world_backend::EmptyListenerEffects;
        // Settings +70, which a furniture marker's 00450370 rereads, and +84,
        // which the player's respawn reads.
        let settings = self.platform.world_tick_inputs();
        // Port setting: Ultra ground detail, which the placements stand on.
        let ultra = self.platform.ultra_ground();
        let crate::loading_integration::LoadingPlatformLoans {
            mode,
            rng,
            mut voices,
        } = self.platform.loading_platform();
        // The start block's RNG word at 0048DC30's entry (the lab's runs).
        if let Some(word) = self
            .harness_start
            .as_mut()
            .and_then(|start| start.load_rng.take())
        {
            *rng = word;
        }
        let reads = match presentation {
            WorldLoadPresentation::NoCallbacks => {
                crate::loading_integration::WorldLoadReads::NoCallbacks
            }
            WorldLoadPresentation::Interactive(presentation) => {
                crate::loading_integration::WorldLoadReads::Interactive(
                    crate::loading_integration::WorldLoadingInputs {
                        reads: &mut self.loading_reads,
                        process: &mut self.loading_process,
                        effects: &mut self.text_process.effect_bytes,
                        context: crate::loading_integration::LoadingContextSource {
                            registry: &self.registry,
                            owner: self.attachment.context_owner,
                        },
                        current_attract: &self.attract,
                        current_mode: mode,
                        presentation,
                    },
                )
            }
        };
        let metadata = crate::world_backend::load_offline_world(
            &mut self.residency,
            crate::world_backend::OfflineWorldLoadInputs {
                terrain_policy: &mut self.terrain_policy,
                sound: crate::object_runtime::SoundTeardownService::Available {
                    sounds: &mut self.sounds,
                    voices: &mut voices,
                },
                owner,
                assets: self.catalog.assets(),
                furniture_radius: &mut self.furniture_radius,
                process: Some(&self.factory_process),
                map: crate::world_map::MapLoadContext {
                    process: &mut self.maps,
                    state: &mut self.map_load,
                },
                rng,
                reads,
                listener_effects: &mut listener_effects,
                policy: crate::world_backend::OfflineWorldLoadPolicy::RetainedManager {
                    text_data: Some(text_data),
                },
                particles: Some(&mut self.particles),
                phase: Some(u32::from(state.current_phase().id())),
                furniture: Some(crate::world_backend::OfflineFurnitureLoan {
                    detail: settings.settings_70,
                    query: &mut self.furniture_query,
                }),
                settings_84: settings.settings_84,
                ultra,
            },
        )?;
        let owner = self
            .pending_world
            .take()
            .expect("successful borrowed load retains owner");
        self.world = Some(WorldSession::from_loaded(owner, metadata));
        // Port option: Pelagia, the prototype world. The rework (world 1's
        // lagoon, crystals, return gate and factory stock) is its own flag;
        // off, world 1 keeps the briefing and the retail map.
        if self.platform.prototype_world() {
            if let Some(world) = self.world.as_mut() {
                let placements = world.report.placements.clone();
                crate::pelagia::prepare_world(
                    &mut world.arena,
                    world.mission.current_world_a0,
                    &placements,
                    &self.residency,
                    &mut world.terrain,
                    self.platform.pelagia_rework(),
                );
                // The rework's night falls in a quarter of the time.
                if self.platform.pelagia_rework()
                    && world.mission.current_world_a0 == crate::pelagia::WORLD
                {
                    world.tint_schedule.period =
                        crate::pelagia::rework_tint_period(world.tint_schedule.period);
                }
            }
        }
        // Port option: Pelagia borrows the Command Centre's model bank for
        // its ships. Anything else drops the loan, so only world 1's draws
        // can see it.
        let loan = self.world.as_ref().is_some_and(|world| {
            crate::pelagia::model_loan_wanted(
                self.platform.prototype_world(),
                world.mission.current_world_a0,
            )
        });
        if loan {
            let installation = self.residency.installation().clone();
            match crate::pelagia::borrowed_model_residency(&installation) {
                Ok(models) => self.pelagia_models = Some(models),
                Err(error) => {
                    eprintln!("Pelagia: the Command Centre bank stays out of reach: {error:?}");
                    self.pelagia_models = None;
                }
            }
        } else {
            self.pelagia_models = None;
        }
        // Port setting: Ultra ground rises under the world's buildings.
        if let Some(world) = self.world.as_mut() {
            let footings = crate::ultra_footings::world_footings(
                &self.residency,
                &world.arena,
                &world.terrain,
                &self.furniture_draw,
                &self.object_draw_process,
                crate::modern_tuning::current().ultra(),
            );
            world.terrain.set_ultra_footings(footings);
        }
        self.world_rng_custody = WorldRngCustody::LoadedWorld;
        // 0048DC30's terrain preparation 0044FDD0 clears process 005483B0.
        self.camera_shake_enabled = 0;
        // 0048DD5B clears infrared vision's request; +5C3 keeps the last
        // world's until the first tick takes it.
        self.camera_globals.infrared_5c4.set(false);
        self.camera_globals.ironbow.set(false);
        // The start block's tint timer, over the zero the header wrote
        // (a probe of a world's sunset or night).
        if let Some(clock) = self
            .harness_start
            .as_mut()
            .and_then(|start| start.day_clock_ms.take())
        {
            if let Some(world) = self.world.as_mut() {
                world.mission.dword_2c = Some(clock);
            }
        }
        if let Some(running) = self
            .harness_start
            .as_mut()
            .and_then(|start| start.tint_running.take())
        {
            if let Some(world) = self.world.as_mut() {
                world.mission.byte_13 = Some(u8::from(running));
            }
        }
        // Port option: a world that rains or snows rolls this visit's
        // weather (crate::world_weather); the start block may set it. A
        // network game keeps retail's: none.
        let forced = self
            .harness_start
            .as_mut()
            .and_then(|start| start.weather.take());
        let networked = self.factory_process.network_flag != 0;
        if let Some(world) = self.world.as_mut() {
            let index = world.mission.current_world_a0;
            let kind = crate::modern_tuning::current().sky(index).precip.kind;
            let seed = crate::world_weather::visit_seed(index, world.mission.save.elapsed_seconds);
            world.weather = if networked {
                crate::world_weather::WorldWeather::retail(kind)
            } else {
                crate::world_weather::WorldWeather::arrive(kind, seed, forced)
            };
        }
        let world = self.world.as_ref().expect("published completed owner");
        if state.current_phase() != SessionPhase::Phase8 {
            state.restore_mission_elapsed_seconds(world.mission.save.elapsed_seconds);
        }
        let offline = self
            .offline_game
            .as_mut()
            .expect("admitted prepared projection");
        offline.mission = world.mission.save.clone();
        if let Some(player) = world.arena.first_player() {
            offline.player = world
                .arena
                .export_player_save_state(player)
                .map_err(crate::world_loader::WorldLoaderError::Player)
                .map_err(WorldBackendError::Loader)?;
        }
        self.platform
            .session_music(SessionMusicRequest::Select { native_argument: 1 })
            .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))?;
        if resume_after_load {
            self.platform
                .session_music(SessionMusicRequest::Resume)
                .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))?;
        }
        Ok(())
    }
}
