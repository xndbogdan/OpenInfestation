//! Native intro Escape completion and the bounded phase-8 continuation.

use super::*;
use crate::gameplay_menu::{menu_key_effects, MenuKeyTerminal};
use crate::gameplay_screen::{GameplayInputAction, GameplayScreen};
use crate::intro_results_screen::{
    IntroResultsMenuServices, IntroResultsSkippedFeature, IntroResultsTickHost,
    IntroResultsTickServices,
};
use crate::world_backend::WorldSession;
use crate::world_completion::{
    complete_world, WorldCompletionInputs, WorldCompletionOutcome, WorldCompletionRequest,
    INTRO_COMPLETION,
};
use crate::world_mission::WorldMission;

impl PortLinked<'_> {
    pub fn poll_gameplay(
        &mut self,
        state: &mut SessionState,
        pass: PortInputPass<'_>,
        world: &mut WorldSession,
        sounds: &mut crate::process_sound::ProcessSoundRuntime,
        exit_requested: &mut bool,
        settings: &mut [u32; crate::frontend_settings::SETTINGS_WORDS],
        cheats_84: u32,
    ) -> Result<(), GameplayLinkedError> {
        let owner = self.attachment.context_owner;
        let screen = self
            .gameplay
            .as_deref_mut()
            .ok_or(LinkedScreenError::Unlinked)?;
        let input = &mut *self.input;
        let executable =
            inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes()).ok();
        let clock = crate::frontend_camera::AnimationClockSource::animation_value(self.process);
        let result = self.registry.with_context_dispatch(owner, |registry| {
            require_id(registry, owner, screen.id())?;
            let mut router = GameplayRouter {
                now_micros: pass.now_micros,
                screen,
                state,
                world,
                sounds,
                attract: &mut *self.attract,
                factory_process: self.factory_process,
                terrain_policy: &mut *self.terrain_policy,
                executable,
                exit_requested,
                settings,
                cheats_84,
                clock,
            };
            deliver(pass, input, &mut router)
        });
        flatten_registry(result)
    }

    pub fn poll_intro_results(
        &mut self,
        pass: PortInputPass<'_>,
        exit_requested: &mut bool,
        profiles: Option<&mut crate::profile_store::ProfileStore>,
    ) -> Result<(), GameplayLinkedError> {
        let owner = self.attachment.context_owner;
        let owner_byte = self.owner_byte;
        let screen = self
            .intro_results
            .as_deref_mut()
            .ok_or(LinkedScreenError::Unlinked)?;
        let input = &mut *self.input;
        let result = self.registry.with_context_dispatch(owner, |registry| {
            require_id(registry, owner, screen.id())?;
            let now_micros = pass.now_micros;
            deliver(
                pass,
                input,
                &mut ResultsRouter {
                    now_micros,
                    screen,
                    exit_requested,
                    profiles,
                    owner_byte,
                },
            )
        });
        flatten_registry(result)
    }

    /// The interface cues the results screen's profile menu played.
    pub fn intro_results_cues(&mut self) -> Vec<i16> {
        self.intro_results
            .as_deref_mut()
            .map(crate::intro_results_screen::IntroResultsScreen::take_cues)
            .unwrap_or_default()
    }

    pub fn tick_intro_results(
        &mut self,
        state: &mut SessionState,
        tick: SchedulerTick,
        world: &mut crate::world_backend::WorldSession,
        profiles: Option<&mut crate::profile_store::ProfileStore>,
    ) -> Result<(), GameplayLinkedError> {
        let classification = self.profile_name_classification;
        let owner = self.attachment.context_owner;
        let owner_byte = self.owner_byte;
        let screen = self
            .intro_results
            .as_deref_mut()
            .ok_or(LinkedScreenError::Unlinked)?;
        let result = self.registry.with_context_dispatch(owner, |registry| {
            require_id(registry, owner, screen.id())?;
            let elapsed_seconds = state.gameplay_time().seconds;
            screen
                .tick(
                    tick,
                    &mut IntroResultsTickServices {
                        menu: IntroResultsMenuServices {
                            profiles,
                            owner_byte,
                            input: &mut *self.input,
                        },
                        process: self.process,
                        mission: &world.mission,
                        attract: *self.attract,
                    },
                    &mut ResultsTick {
                        state,
                        save: ResultsSave {
                            mission: &world.mission,
                            arena: &world.arena,
                            classification,
                            elapsed_seconds,
                        },
                    },
                )
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))
        });
        flatten_registry(result)
    }
}

pub(super) fn require_id(
    registry: &ScreenRegistry<FrontendRenderContext, InputContextId>,
    owner: crate::screen_registry::ContextOwnerId,
    expected: Option<ScreenId>,
) -> Result<(), GameplayLinkedError> {
    let actual = registry.linked_screen(owner)?;
    if expected.is_none() || expected != actual {
        return Err(match actual {
            Some(id) => LinkedScreenError::Foreign(id),
            None => LinkedScreenError::Unlinked,
        }
        .into());
    }
    Ok(())
}

pub(super) fn flatten_registry<T>(
    result: Result<T, RegistryCallbackError<GameplayLinkedError>>,
) -> Result<T, GameplayLinkedError> {
    match result {
        Ok(value) => Ok(value),
        Err(RegistryCallbackError::Registry(error)) => Err(GameplayLinkedError::Registry(error)),
        Err(RegistryCallbackError::Callback(error)) => Err(error),
    }
}

pub(super) fn deliver(
    pass: PortInputPass<'_>,
    input: &mut InputRuntime,
    router: &mut impl InputHost<Error = GameplayLinkedError>,
) -> Result<(), GameplayLinkedError> {
    let flatten = |error| match error {
        InputError::State(error) => GameplayLinkedError::Tick(error.to_string()),
        InputError::Host(error) => error,
    };
    if !pass.notifications.is_empty() {
        let keyboard = pass
            .keyboard
            .ok_or_else(|| GameplayLinkedError::Tick("missing keyboard adapter".into()))?;
        for notification in pass.notifications.drain(..) {
            keyboard
                .update(notification, input, router)
                .map_err(flatten)?;
        }
    }
    input.poll(router).map_err(flatten)
}

pub(super) fn system_callback(event: InputCallback, exit: &mut bool) -> bool {
    match event.address {
        EXIT_CALLBACK => {
            if event.value != 0 {
                *exit = true;
            }
            true
        }
        NOOP_CALLBACK => true,
        FULLSCREEN_TOGGLE_CALLBACK => {
            if event.value != 0 {
                eprintln!(
                    "Gameplay fullscreen toggle skipped: display callback owner remains open"
                );
            }
            true
        }
        _ => false,
    }
}

/// The phase-5 held slots' commands on the first player's message: codes
/// through `00413950`, Back through `004133E0`. The message's UI state is
/// synced before them and its phase requests follow, as around the player's
/// keys.
pub(in crate::session_platform) fn run_in_world_menu_commands(
    screen: &mut GameplayScreen,
    world: &mut WorldSession,
    state: &mut SessionState,
) -> Result<(), GameplayLinkedError> {
    let commands = screen.take_in_world_menu_commands();
    if commands.is_empty() {
        return Ok(());
    }
    let Some(player) = world.arena.first_player() else {
        return Ok(());
    };
    crate::session_platform::intro::sync_player_message_ui_state(world, state);
    for command in commands {
        match command {
            crate::gameplay_screen::InWorldMenuCommand::Code(code) => {
                let omissions = world
                    .arena
                    .in_world_menu_code(player, code)
                    .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
                world.actor_updates.log_key_omissions(omissions);
            }
            crate::gameplay_screen::InWorldMenuCommand::Back => world
                .arena
                .back_player_menu(player)
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?,
        }
    }
    crate::session_platform::intro::request_player_message_phases(world, state);
    Ok(())
}

struct GameplayRouter<'a> {
    now_micros: u64,
    screen: &'a mut GameplayScreen,
    state: &'a mut SessionState,
    world: &'a mut WorldSession,
    sounds: &'a mut crate::process_sound::ProcessSoundRuntime,
    attract: &'a mut bool,
    factory_process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
    terrain_policy: &'a mut crate::terrain_runtime::ProcessTerrainPolicy,
    /// The executable, whose shop tables a key's unit broadcast can reach.
    executable: Option<inf_formats::FrontendExecutable<'a>>,
    exit_requested: &'a mut bool,
    /// The live settings: F12 flips First Person (`+68`).
    settings: &'a mut [u32; crate::frontend_settings::SETTINGS_WORDS],
    /// The cheat word (`+84`) as Tab's computer reads it
    /// ([`crate::frontend_settings::PortFixes::menu_cheat_word`]).
    cheats_84: u32,
    /// The animation clock `005471C0`, which the network menus' waits read.
    clock: u32,
}
impl InputHost for GameplayRouter<'_> {
    type Error = GameplayLinkedError;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }
    fn callback(&mut self, event: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        if system_callback(event, self.exit_requested) {
            return Ok(());
        }
        let action = if event.context == crate::gameplay_screen::IN_WORLD_MENU_CALLBACK_CONTEXT {
            // The phase-5 screen's wrappers press its held bits; their slots
            // reach the first player's message at once (00439430).
            let action = self
                .screen
                .in_world_menu_callback(event)
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
            run_in_world_menu_commands(self.screen, self.world, self.state)?;
            action
        } else if event.context != self.screen.callback_context() {
            // The +0xFC sub-object's rows call the first player's 00469450..
            // 004698E0 family with the player as context.
            crate::session_platform::intro::sync_player_message_ui_state(self.world, self.state);
            self.player_key(event);
            crate::session_platform::intro::request_player_message_phases(self.world, self.state);
            return Ok(());
        } else {
            self.screen
                .digital_callback(event)
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?
        };
        match action {
            GameplayInputAction::NoAction => {}
            // 00442BF0: First Person (+68) flips; 0040F740 then gives it,
            // with the Camera Style, to the world's camera.
            GameplayInputAction::FirstPerson => {
                let first_person = &mut self.settings[0x68 / 4];
                *first_person = crate::gameplay_screen::first_person_toggled(*first_person);
                let settings = crate::object_runtime::CameraSettings::from_words(&*self.settings);
                let refresh = self
                    .world
                    .arena
                    .refresh_world_camera(settings)
                    .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
                if refresh == crate::object_runtime::WorldCameraRefresh::NeedsCamera {
                    eprintln!(
                        "First Person refresh: the world has no camera yet; it takes the setting when built"
                    );
                }
            }
            // 00442C30: the HUD setting (+60) flips.
            GameplayInputAction::Hud => {
                let hud = &mut self.settings[0x60 / 4];
                *hud = crate::gameplay_screen::hud_toggled(*hud);
            }
            // 00442BC0: the map (phase 7) unless the world is complete or a
            // network session runs.
            GameplayInputAction::Map => {
                if let Some(phase) = crate::gameplay_menu::map_requested(
                    1,
                    self.world.mission.current_world_a0,
                    self.factory_process.network_flag != 0,
                ) {
                    let _ = self.state.request_phase(phase);
                }
            }
            // 00442FD0: back to the frontend (phase 2).
            GameplayInputAction::ReturnToFrontend => {
                if let Some(phase) = crate::gameplay_menu::return_to_frontend_requested(1) {
                    let _ = self.state.request_phase(phase);
                }
            }
            // 00442FF0: the world ends for the next one (00487560), as the
            // world-complete Escape ends it.
            GameplayInputAction::NextWorld => {
                let request = WorldCompletionRequest {
                    next_selection: crate::gameplay_screen::next_world_selection(
                        self.world.mission.current_world_a0,
                    ),
                    start_ordinal: 1,
                    actor: None,
                };
                let outcome = complete_world(
                    &mut self.world.mission,
                    &mut self.world.arena,
                    request,
                    WorldCompletionInputs {
                        network_active: self.factory_process.network_flag != 0,
                        process: Some(self.factory_process),
                    },
                    &mut *self.sounds,
                )
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
                if let WorldCompletionOutcome::Accepted { request_phase } = outcome {
                    let _ = self.state.request_phase(request_phase);
                }
            }
            // 00443020: terrain detail (+70) flips, then 004502C0 reselects
            // the spacing and prepares the terrain.
            GameplayInputAction::TerrainDetail => {
                let detail = &mut self.settings[0x70 / 4];
                *detail = crate::gameplay_screen::terrain_detail_toggled(*detail);
                crate::terrain_runtime::TerrainDetailContext {
                    process: &mut *self.terrain_policy,
                    world_flags: Some(self.world.world_flags),
                    terrain: Some(&mut self.world.terrain),
                }
                .configure(*detail)
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
            }
            // 00442CF0 (T) and 00442EE0 (Y): a network game's team or
            // vehicle menu, once the screen's wait has passed.
            GameplayInputAction::NetworkMenu { address } => {
                let wait = match address {
                    crate::session_platform::network_menus::TEAM_MENU => {
                        &mut self.screen.team_menu_f0
                    }
                    _ => &mut self.screen.vehicle_menu_ec,
                };
                if self.factory_process.network_flag != 0 && *wait <= self.clock {
                    let executable = self.executable.ok_or_else(|| {
                        GameplayLinkedError::Tick("the network menus need the executable".into())
                    })?;
                    let menu = crate::session_platform::network_menus::network_menu(
                        address,
                        executable,
                        &self.factory_process.settings,
                    )
                    .map_err(GameplayLinkedError::Tick)?;
                    if let Some(data) = menu {
                        if crate::session_platform::network_menus::show_network_menu(
                            &mut self.world.arena,
                            address,
                            data,
                        )
                        .map_err(GameplayLinkedError::Tick)?
                        {
                            *wait = self
                                .clock
                                .wrapping_add(crate::session_platform::network_menus::MENU_WAIT);
                            crate::session_platform::world_actor_update::run_key_broadcasts(
                                &mut self.world.arena,
                                &mut self.world.actor_updates.shop_stop,
                                &mut self.world.draw_omissions_logged,
                                self.factory_process,
                                executable,
                            );
                        }
                    }
                }
            }
            GameplayInputAction::Scores { held } => self.screen.scores_f8 = held,
            GameplayInputAction::Unsupported { address, value } => {
                if value != 0 {
                    eprintln!(
                        "Gameplay input {address:#010x} skipped: native callback remains open"
                    );
                }
            }
            // 00443700 on the phase-5 screen (P, Shift+Esc): its +64,
            // 004430E0, is 004133C0 on the message, which hides it whole.
            // Escape alone is the screen's Back (004133E0), one menu.
            GameplayInputAction::MenuKey
                if self.state.current_phase() == crate::session::SessionPhase::Phase5 =>
            {
                if let Some(player) = self.world.arena.first_player() {
                    crate::session_platform::intro::sync_player_message_ui_state(
                        self.world, self.state,
                    );
                    self.world
                        .arena
                        .hide_player_message(player)
                        .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
                    crate::session_platform::intro::request_player_message_phases(
                        self.world, self.state,
                    );
                }
            }
            GameplayInputAction::MenuKey => {
                // 00443070's modal: 00488AA0's player with its message shown.
                let modal = self.world.arena.first_player().filter(|&player| {
                    self.world
                        .arena
                        .world_player(player)
                        .is_ok_and(|state| state.message_byte_00() != 0)
                });
                let effects = menu_key_effects(
                    *self.attract,
                    self.world.mission.current_world_a0,
                    modal.is_some(),
                );
                if effects.clear_attract {
                    *self.attract = false;
                }
                if let Some(phase) = effects.frontend_request {
                    let _ = self.state.request_phase(phase);
                }
                match effects.terminal {
                    MenuKeyTerminal::WorldUpdate { .. } => {
                        let outcome = complete_world(
                            &mut self.world.mission,
                            &mut self.world.arena,
                            INTRO_COMPLETION,
                            WorldCompletionInputs {
                                network_active: self.factory_process.network_flag != 0,
                                process: Some(self.factory_process),
                            },
                            &mut *self.sounds,
                        )
                        .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
                        if let WorldCompletionOutcome::Accepted { request_phase } = outcome {
                            let _ = self.state.request_phase(request_phase);
                        }
                    }
                    // 004133C0 on the shown message.
                    MenuKeyTerminal::PlayerModal => {
                        if let Some(player) = modal {
                            self.world
                                .arena
                                .hide_player_message(player)
                                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?;
                        }
                    }
                    // 00438380(5): the in-world menu.
                    MenuKeyTerminal::RequestMenu => {
                        let _ = self.state.request_phase(crate::gameplay_menu::MENU_PHASE);
                    }
                }
            }
        }
        Ok(())
    }
}

impl GameplayRouter<'_> {
    /// A player-context key runs its callback on the player whose
    /// construction holds that context. A failed callback stops the player's
    /// keys for the rest of the world; omitted calls are logged once.
    fn player_key(&mut self, event: InputCallback) {
        use crate::object_runtime::{is_player_key_callback, PlayerKeyEvent, PlayerKeyInputs};
        if self.world.actor_updates.player_keys_stop.is_some() {
            return;
        }
        let player = self
            .world
            .constructions
            .iter()
            .find(|construction| construction.callback_context() == event.context)
            .map(|construction| construction.player());
        let Some(player) = player.filter(|_| is_player_key_callback(event.address)) else {
            if event.value != 0 {
                eprintln!(
                    "Gameplay player input {:#010x} skipped: no player callback owns it",
                    event.address
                );
            }
            return;
        };
        let result = self
            .screen
            .double_tap_window()
            .ok_or_else(|| "the gameplay screen has not read 0053A278".to_string())
            .and_then(|window| {
                self.world
                    .arena
                    .player_key_callback(
                        player,
                        PlayerKeyEvent {
                            address: event.address,
                            value: event.value,
                            timestamp_micros: event.timestamp_micros,
                        },
                        PlayerKeyInputs {
                            network: self.factory_process.network_flag,
                            double_tap_window: window,
                            cheat_word_84: self.cheats_84,
                        },
                    )
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(omissions) => self.world.actor_updates.log_key_omissions(omissions),
            Err(error) => {
                eprintln!(
                    "Gameplay player keys stopped at {:#010x}: {error}",
                    event.address
                );
                self.world.actor_updates.player_keys_stop = Some(error);
            }
        }
        // Tab's computer and Escape broadcast from the unit inside their
        // callbacks (0047DD20); run them before the next input event.
        match self.executable {
            Some(executable) => crate::session_platform::world_actor_update::run_key_broadcasts(
                &mut self.world.arena,
                &mut self.world.actor_updates.shop_stop,
                &mut self.world.draw_omissions_logged,
                self.factory_process,
                executable,
            ),
            None => {
                if !self.world.arena.take_unit_broadcasts().is_empty() {
                    eprintln!("The unit's broadcast skipped: the executable is unavailable");
                }
            }
        }
    }
}

struct ResultsRouter<'a> {
    now_micros: u64,
    screen: &'a mut crate::intro_results_screen::IntroResultsScreen,
    exit_requested: &'a mut bool,
    profiles: Option<&'a mut crate::profile_store::ProfileStore>,
    owner_byte: bool,
}
impl InputHost for ResultsRouter<'_> {
    type Error = GameplayLinkedError;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }
    fn callback(
        &mut self,
        event: InputCallback,
        input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        if system_callback(event, self.exit_requested) {
            return Ok(());
        }
        if event.context != crate::intro_results_screen::INTRO_RESULTS_CALLBACK_CONTEXT {
            if event.value != 0 {
                eprintln!(
                    "Results input {:#010x} in context {} skipped",
                    event.address, event.context
                );
            }
            return Ok(());
        }
        self.screen
            .input_callback(
                event,
                &mut IntroResultsMenuServices {
                    profiles: self.profiles.as_deref_mut(),
                    owner_byte: self.owner_byte,
                    input,
                },
            )
            .map_err(|error| GameplayLinkedError::Tick(error.to_string()))
    }
}

/// 0042D150: the working profile serialized (00449C10) and written
/// (0044A110). A missing store or a failed step is the error scope's arm.
struct ResultsSave<'a> {
    mission: &'a WorldMission,
    arena: &'a crate::object_runtime::ObjectArena,
    classification: &'a crate::profile_name_format::ProfileNameClassification,
    elapsed_seconds: u32,
}

struct ResultsTick<'a> {
    state: &'a mut SessionState,
    save: ResultsSave<'a>,
}
impl IntroResultsTickHost for ResultsTick<'_> {
    type Error = std::convert::Infallible;
    fn request_phase(
        &mut self,
        _: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error> {
        let _ = self.state.request_phase(requested);
        Ok(())
    }
    fn skip_feature(&mut self, feature: IntroResultsSkippedFeature) -> Result<(), Self::Error> {
        eprintln!("Results screen: skipped {feature:?}; its native owner is not ported");
        Ok(())
    }
    fn save_profile(
        &mut self,
        profiles: Option<&mut crate::profile_store::ProfileStore>,
    ) -> Result<bool, Self::Error> {
        let save = &mut self.save;
        let Some(profiles) = profiles else {
            eprintln!("Results save failed: no profile store is lent");
            return Ok(false);
        };
        let result = profiles
            .serialize_current_working(crate::profile_store::WorkingProfileSerializeRequest {
                mission: &save.mission.save,
                elapsed_seconds: save.elapsed_seconds,
                arena: save.arena,
                classification: save.classification,
            })
            .map_err(|error| error.to_string())
            .and_then(|()| profiles.save_working().map_err(|error| error.to_string()));
        match result {
            Ok(()) => {
                eprintln!("Results screen: game saved");
                Ok(true)
            }
            Err(error) => {
                eprintln!("Results save failed: {error}");
                Ok(false)
            }
        }
    }
}
