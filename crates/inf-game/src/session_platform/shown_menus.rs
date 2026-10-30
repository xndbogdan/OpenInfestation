//! The session's half of the shown menus the arena does not own: the
//! phase-5 in-world menu (`00443190` builds `004433C0` over `00535348`) and
//! the rows of it, of Tab's computer (`00413410`) and of the children they
//! push. The arena records each row action with its menu's owner; this owner
//! holds what the actions reach: the menu definitions, the phase requests
//! and the player's message.

use super::*;
use crate::menu::{MenuActionRequest, MenuBasis, MenuCommand, MenuState};
use crate::object_runtime::{ShownMenuAction, ShownMenuOwner};

/// `00535348`: the in-world menu (Resume, Options, Quit).
const IN_WORLD_MENU: u32 = 0x0053_5348;

/// `00453BE0`: push the row's argument 1 definition.
const PUSH_CHILD: u32 = 0x0045_3be0;
/// `00453C50`: pop the current menu.
const POP_MENU: u32 = 0x0045_3c50;
/// `00463B50`: a settings row's adjust (`00453B80`), then the world refresh
/// `0040F740`.
const ADJUST_AND_REFRESH: u32 = 0x0046_3b50;
/// `00443150`: Resume, the callback object's `+64` (`004430E0`, `004133C0`
/// on the screen's message).
const RESUME: u32 = 0x0044_3150;
/// `00443170`: Quit's Yes, the callback object's `+54(1, 0, 0)` (`00442FD0`,
/// a request for phase 2).
const QUIT: u32 = 0x0044_3170;
/// `00415150`: the computer's "Repeat last message", `004135B0`.
const REPEAT_MESSAGE: u32 = 0x0041_5150;
/// `00415170`: "Map and Objectives", `004133C0` then phase 7.
const MAP_AND_OBJECTIVES: u32 = 0x0041_5170;
/// `004151A0`: "Image enhancement mode", the owner's `+3F` then `004133C0`.
const IMAGE_ENHANCEMENT: u32 = 0x0041_51a0;
/// `00414830`: "Access Database", the database `00414420` over the row's
/// definition, shown, then phase 6, whatever the code.
const ACCESS_DATABASE: u32 = 0x0041_4830;
/// `00414720`: "List Hints", the hint list over the row's definition with
/// `+5C` = 0x25, shown, then phase 6, whatever the code.
const LIST_HINTS: u32 = 0x0041_4720;
/// `00414420`'s title (`0052FA28`) and first entry (`0052FA48`, `+5C`).
const DATABASE_TITLE: u32 = 0x180;
const DATABASE_FIRST_ENTRY: u16 = 1;
/// `00414720`'s `+5C`. The row's own argument (`0052FAC8`) stays 0 until a
/// step writes it, so the list first shows hint 0.
const HINT_LIST_START: i32 = 0x25;

impl<P: PortPlatform> PortSession<P> {
    /// `00443190`'s input (InWorldMenuScreenReuse): the gameplay screen takes
    /// the phase-5 screen's context and tables, so the player's own message
    /// keys go quiet and held keys repeat as retail's phase 5 does.
    pub(super) fn enter_in_world_menu_input(&mut self) -> Result<(), PortSessionErrorOf<P>> {
        let Some(screen) = self.gameplay.as_mut() else {
            return Ok(());
        };
        let devices =
            FrontendFactoryHost::enumerate_calibrated_devices(&mut self.platform, &mut self.input)
                .map_err(|error| PortSessionError::Gameplay(GameplayScreenError::Host(error)))?;
        screen
            .enter_in_world_menu(&mut self.input, self.catalog.assets(), &devices)
            .map_err(map_gameplay_teardown)
    }

    /// Back to phase 4 or 6: the gameplay screen's own context again.
    pub(super) fn leave_in_world_menu_input(&mut self) -> Result<(), PortSessionErrorOf<P>> {
        match self.gameplay.as_mut() {
            Some(screen) => screen
                .leave_in_world_menu(&mut self.input)
                .map_err(map_gameplay_teardown),
            None => Ok(()),
        }
    }

    /// The phase-5 screen tick's base input (`004392B0`): its repeat
    /// dispatches every held key each 100 ms after the first 250, and the
    /// slots' commands reach the first player's message.
    pub(super) fn tick_in_world_menu_input(
        &mut self,
        delta_micros: i32,
        elapsed_micros: i32,
        state: &mut SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        let (Some(screen), Some(world)) = (self.gameplay.as_mut(), self.world.as_mut()) else {
            return Ok(());
        };
        if !screen.serves_in_world_menu() {
            return Ok(());
        }
        screen
            .tick_in_world_menu(
                crate::scheduler::SchedulerTick {
                    delta_micros,
                    elapsed_micros,
                },
                &self.input,
            )
            .map_err(map_gameplay_teardown)?;
        if let Err(error) = super::linked::run_in_world_menu_commands(screen, world, state) {
            eprintln!("In-world menu input stopped: {error}");
        }
        Ok(())
    }

    /// `00443190`'s menu: `004433C0(00535348, session +64C)` shown through
    /// `004137D0(0, menu)` on the first player's message.
    pub(super) fn show_in_world_menu(&mut self, previous: SessionPhase) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let Some(player) = world.arena.first_player() else {
            eprintln!("In-world menu skipped: no first player holds the message");
            return;
        };
        let data = match self.catalog.menu_data(IN_WORLD_MENU) {
            Ok(data) => data,
            Err(error) => {
                eprintln!("In-world menu skipped: {error}");
                return;
            }
        };
        let menu = MenuState::new(data, MenuBasis::INITIAL);
        let owner = ShownMenuOwner::InWorld {
            return_phase: u32::from(previous.id()),
        };
        if let Err(error) = world.arena.show_player_menu(player, 0, menu, owner) {
            eprintln!("In-world menu skipped: {error}");
        }
    }

    /// [`DatabaseModels::EveryWorld`]: while the database shows an entry
    /// whose model the current world has not loaded, keep a second residency
    /// activated at the first overlay that loads it. Loading happens here,
    /// in the tick, so the draw only reads it.
    pub(super) fn prefetch_database_model(&mut self) {
        if self.platform.database_models() != DatabaseModels::EveryWorld {
            return;
        }
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let Some(player) = world.arena.first_player() else {
            return;
        };
        let Some(ShownMenuOwner::Database { entry }) = world
            .arena
            .player_shown_menu(player)
            .and_then(|menu| menu.current_owner())
        else {
            return;
        };
        let Some(model) = world
            .arena
            .player_message_environment_mut()
            .and_then(|environment| environment.menus.database.get(usize::from(entry)).copied())
            .map(|entry| entry.model)
        else {
            return;
        };
        let handler = crate::render_resources::RenderResourceKind::Model.handler();
        if model == 0 || self.residency.resource(handler, model).is_some() {
            return;
        }
        if let Some(models) = &self.database_models {
            if models.runtime.resource(handler, model).is_some() {
                return;
            }
        }
        let Some(&level) = self.residency.leaves_registering(handler, model).first() else {
            return;
        };
        let loaded = (|| -> crate::Result<ResidentRuntime> {
            let mut runtime = ResidentRuntime::new(self.residency.installation().clone())?;
            runtime.apply(crate::ResidencyRequest::Boot)?;
            runtime.apply(crate::ResidencyRequest::Activate { level })?;
            Ok(runtime)
        })();
        match loaded {
            Ok(runtime) => {
                self.database_models = Some(DatabaseModelResidency { level, runtime });
                self.database_models_generation += 1;
            }
            Err(error) => {
                if world
                    .draw_omissions_logged
                    .insert(format!("database model level {level}"))
                {
                    eprintln!(
                        "Database model {model:#x}: overlay {level} would not load ({error}); \
                         the entry shows its text"
                    );
                }
            }
        }
    }

    /// The rest of the input poll (`004F0200`): the shown menus' commands the
    /// key callbacks queued (`00413950`) and the rows they reach, which
    /// native runs inside the callbacks, before the pass's transition and
    /// ticks. A phase a row requests is thus taken before the world updates.
    pub fn after_input_poll(&mut self, state: &mut SessionState) {
        self.drain_shown_menu_commands(state);
        self.dispatch_shown_menu_actions(state);
    }

    /// The game as the next crash log describes it: the phase, the world
    /// (its name once per world, H0 `0xE0 + world`), the mission's elapsed
    /// seconds and the first player's unit.
    pub(super) fn note_crash_state(&mut self, state: &SessionState) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let current = world.mission.current_world_a0;
        if !crate::crash_log::world_name_noted(current) {
            use crate::menu_text::MenuTextResources;
            let resources = crate::render_resources::RenderResources::new(&self.residency);
            if let Ok(bytes) = resources.string(0xe0 + current as i32) {
                let name = bytes.split(|&byte| byte == 0).next().unwrap_or(&[]);
                crate::crash_log::note_world_name(
                    current,
                    String::from_utf8_lossy(name).into_owned(),
                );
            }
        }
        let arena = &mut world.arena;
        let unit = arena
            .first_player()
            .and_then(|player| arena.player_actor(player).ok().flatten())
            .and_then(|unit| {
                let actor = arena.actor(unit).ok()?;
                let vehicle = arena.selector2_actor(unit).ok().map(|vehicle| {
                    let rows = arena
                        .derived_actor(unit)
                        .ok()
                        .and_then(|derived| derived.behavior_140.as_ref())
                        .map_or((0, 0), |behavior| (behavior.byte_10, behavior.byte_11));
                    (vehicle.dwords_1e8_1f4[0], rows.0, rows.1)
                });
                Some(crate::crash_log::UnitState {
                    position: arena.object(unit).ok()?.base.position_words,
                    health: actor.health,
                    maximum_health: actor.maximum_health,
                    vehicle,
                })
            });
        crate::crash_log::note_game(crate::crash_log::GameState {
            phase: state.current_phase().id(),
            world: Some(current),
            elapsed_seconds: world.mission.save.elapsed_seconds,
            unit,
        });
    }

    /// The shown menus' recorded row actions, in order. A failed action is
    /// logged once and the rest still run.
    pub(super) fn dispatch_shown_menu_actions(&mut self, state: &mut SessionState) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let actions = world.arena.take_shown_menu_actions();
        for action in actions {
            if let Err(error) = self.run_shown_menu_action(action, state) {
                let Some(world) = self.world.as_mut() else {
                    return;
                };
                if world.draw_omissions_logged.insert(format!(
                    "shown menu action {:#x}",
                    action.request.callback_address
                )) {
                    eprintln!(
                        "Shown menu action {:#010x} stopped: {error}",
                        action.request.callback_address
                    );
                }
            }
        }
        self.prefetch_database_model();
    }

    pub(super) fn run_shown_menu_action(
        &mut self,
        action: ShownMenuAction,
        state: &mut SessionState,
    ) -> Result<(), String> {
        let ShownMenuAction {
            player, request, ..
        } = action;
        let MenuActionRequest {
            callback_address,
            command,
            arguments,
            ..
        } = request;
        let activate = command == MenuCommand::Activate;
        let world = self.world.as_mut().ok_or("no world holds the menu")?;
        match callback_address {
            PUSH_CHILD => {
                if activate {
                    let data = self
                        .catalog
                        .menu_data(arguments[1])
                        .map_err(|error| error.to_string())?;
                    world
                        .arena
                        .push_player_menu(
                            player,
                            MenuState::new(data, MenuBasis::INITIAL),
                            ShownMenuOwner::Plain,
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            POP_MENU => {
                if activate {
                    world
                        .arena
                        .pop_player_menu(player, 1)
                        .map_err(|error| error.to_string())?;
                }
            }
            RESUME => {
                if activate {
                    world
                        .arena
                        .hide_player_message(player)
                        .map_err(|error| error.to_string())?;
                }
            }
            QUIT => {
                if activate {
                    let _ = state.request_phase(SessionPhase::Frontend);
                }
            }
            REPEAT_MESSAGE => {
                if activate {
                    world
                        .arena
                        .replay_player_message(player)
                        .map_err(|error| error.to_string())?;
                }
            }
            MAP_AND_OBJECTIVES => {
                if activate {
                    world
                        .arena
                        .hide_player_message(player)
                        .map_err(|error| error.to_string())?;
                    // The map screen (00450FC0) replaces the gameplay
                    // screen; its Esc requests phase 4 again.
                    let _ = state.request_phase(SessionPhase::Phase7);
                }
            }
            IMAGE_ENHANCEMENT => {
                if activate {
                    world
                        .arena
                        .set_player_control_byte(player, 0x3f, 1)
                        .map_err(|error| error.to_string())?;
                    world
                        .arena
                        .hide_player_message(player)
                        .map_err(|error| error.to_string())?;
                }
            }
            ACCESS_DATABASE | LIST_HINTS => {
                let data = self
                    .catalog
                    .menu_data(arguments[1])
                    .map_err(|error| error.to_string())?;
                let owner = if callback_address == ACCESS_DATABASE {
                    // 00414420 writes the static title and entry rows.
                    let mut records = data.records_mut();
                    records.items[0].draw_arguments[0] = DATABASE_TITLE;
                    records.items[1].draw_arguments[0] = u32::from(DATABASE_FIRST_ENTRY);
                    drop(records);
                    ShownMenuOwner::Database {
                        entry: DATABASE_FIRST_ENTRY,
                    }
                } else {
                    ShownMenuOwner::HintList {
                        hint: HINT_LIST_START,
                    }
                };
                let menu = MenuState::new(data, MenuBasis::INITIAL);
                self.show_computer_menu(player, menu, owner)?;
                let _ = state.request_phase(SessionPhase::Phase6);
            }
            crate::object_runtime::DATABASE_SELECT => {
                self.grant_database_entry(player, arguments[0] as u16)?;
            }
            address if super::cheats::is_cheat_action(address) => {
                self.run_cheat_action(player, request, state)?;
            }
            super::network_menus::TEAM_ROW => {
                if activate {
                    self.choose_team(arguments[0], state)?;
                }
            }
            super::network_menus::VEHICLE_ROW => {
                if activate {
                    self.choose_vehicle(arguments[0])?;
                }
            }
            other => {
                if !self.platform.shown_menu_setting(request)? {
                    return Err(format!(
                        "row action {other:#010x} ({command:?}) is not ported in gameplay;                          the setting stays"
                    ));
                }
                // 00463B50: after the adjust, 0040F740 gives the First
                // Person and Camera Style settings to the world's camera.
                if other == ADJUST_AND_REFRESH {
                    let inputs = self.platform.world_tick_inputs();
                    let refresh = world
                        .arena
                        .refresh_world_camera(inputs.camera_settings)
                        .map_err(|error| error.to_string())?;
                    if refresh == crate::object_runtime::WorldCameraRefresh::NeedsCamera {
                        eprintln!(
                            "Options refresh: the world has no camera yet; it takes the settings when built"
                        );
                    }
                    // Retail leaves a new Detail Level to the next world; the
                    // port takes it at once, as Alt+F3 does (00443020,
                    // 004502C0; user, 2026-10-22).
                    if arguments[1] == crate::frontend_settings::SETTINGS_STATE_ADDRESS + 0x70 {
                        crate::terrain_runtime::TerrainDetailContext {
                            process: &mut self.terrain_policy,
                            world_flags: Some(world.world_flags),
                            terrain: Some(&mut world.terrain),
                        }
                        .configure(inputs.settings_70)
                        .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        Ok(())
    }
}
