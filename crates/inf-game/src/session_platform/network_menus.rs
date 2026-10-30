//! The network game's menus in the world. On the gameplay screen T
//! (`00442CF0`) lists the arena's teams in Capture the Banner and Powerball
//! (modes 0 and 4), and Y (`00442EE0`) its vehicles in mode 0, each as the
//! first player's menu (`005350D0`, `005351D8`, their row counts set from
//! the arena table `0053C570` before `00452310` reads them). A team row
//! (`00442C60`) sets settings `+14` and, unless the player's unit already
//! plays for it, retires the unit and respawns at one of the team's
//! markers (`0046A890(1)`); a vehicle row (`00442DF0`) sets `+18` for the
//! next respawn and says so in the player's message.

use inf_formats::FrontendExecutable;

use super::{
    cheats::pickup_text,
    particles::damage::{world_retirement_host, BehaviorFactoryLoan, WorldDamageOwners},
    world_actor_factory::{WorldCameraFactoryContext, WorldCameraInputs},
    world_actor_update::WorldRespawnHost,
    world_owners::current_grant_text,
    PortPlatform, PortSession,
};
use crate::{
    menu::{MenuBasis, MenuData, MenuState},
    object_runtime::{
        HeldCameraHost, ObjectArena, PickupTables, PlayerMessageOperation, PlayerMessageRequest,
        ShownMenuOwner,
    },
    session::SessionState,
};

/// The keys: T (`004435F0`, to the screen's `+5C`, the team menu
/// `00442CF0`) and Y (`00443620`, to `+60`, the vehicle menu `00442EE0`).
pub(crate) const TEAM_MENU: u32 = 0x0044_35f0;
pub(crate) const VEHICLE_MENU: u32 = 0x0044_3620;
/// Their rows' actions.
pub(super) const TEAM_ROW: u32 = 0x0044_2c60;
pub(super) const VEHICLE_ROW: u32 = 0x0044_2df0;
/// `005350D0`'s rows and layouts, then `005351D8`'s.
const TEAM_ROWS: u32 = 0x0053_4f80;
const TEAM_LAYOUTS: u32 = 0x0053_50a0;
const VEHICLE_ROWS: u32 = 0x0053_50e8;
const VEHICLE_LAYOUTS: u32 = 0x0053_51a8;
/// Animation-clock ticks before the key opens its menu again (screen `+F0`
/// for T, `+EC` for Y).
pub(crate) const MENU_WAIT: u32 = 250;
/// The vehicle row's line: "%s %s" (`0052FE2C`) of H0 `0x78` and the
/// vehicle's name, H0 `0x54` on.
const VEHICLE_LINE: u16 = 0x78;
const VEHICLE_NAMES: u16 = 0x54;

const NO_WORLD: &str = "no world holds the network menu";

/// The menu T or Y opens (`00442CF0`, `00442EE0`) with the game's settings
/// `+00..+4F`: None when the mode has no such menu (T: modes 0 and 4; Y:
/// mode 0 with more than one vehicle). Its rows are the header and one a
/// team or vehicle, the arena's count (`00488A80`, `00488A40`); the second
/// layout counts and shows them all; the selection starts on the first.
pub(crate) fn network_menu(
    address: u32,
    executable: FrontendExecutable<'_>,
    settings: &[u32; 20],
) -> Result<Option<MenuData>, String> {
    let mode = settings[0x10 / 4];
    let arena = crate::network_menu::arena_at(settings[0x1c / 4]);
    let (rows, layouts, count) = match address {
        TEAM_MENU if mode == 0 || mode == 4 => (TEAM_ROWS, TEAM_LAYOUTS, arena.word_0c + 1),
        VEHICLE_MENU if mode == 0 && arena.word_08 + 1 > 2 => {
            (VEHICLE_ROWS, VEHICLE_LAYOUTS, arena.word_08 + 1)
        }
        _ => return Ok(None),
    };
    let data = crate::frontend_assets::menu_parts(executable.data(), rows, count, layouts)
        .map_err(|error| error.to_string())?;
    {
        let mut records = data.records_mut();
        records.initial_selection = 1;
        if let Some(layout) = records.layouts.get_mut(1) {
            layout.item_count = (count - 1) as u8;
            layout.visible_rows = (count - 1) as u8;
        }
    }
    Ok(Some(data))
}

/// T's `004133E0` then `004137D0(0, menu)` on the first player's message;
/// Y shows without the first. False without a first player.
pub(crate) fn show_network_menu(
    arena: &mut ObjectArena,
    address: u32,
    data: MenuData,
) -> Result<bool, String> {
    let Some(player) = arena.first_player() else {
        return Ok(false);
    };
    if address == TEAM_MENU {
        arena
            .back_player_menu(player)
            .map_err(|error| error.to_string())?;
    }
    arena
        .show_player_menu_from_unit(
            player,
            0,
            MenuState::new(data, MenuBasis::INITIAL),
            ShownMenuOwner::Plain,
        )
        .map_err(|error| error.to_string())?;
    Ok(true)
}

impl<P: PortPlatform> PortSession<P> {
    /// A settings word the network game reads (`00463AB0`), in the live
    /// block and in the process's copy of it.
    fn set_network_setting(&mut self, offset: usize, value: u32) {
        if let Some(network) = self.platform.network_platform() {
            network.settings.words_mut()[offset / 4] = value;
        }
        self.factory_process.settings[offset / 4] = value;
    }

    /// `00442C60`, a team row's Activate.
    pub(super) fn choose_team(
        &mut self,
        team: u32,
        state: &mut SessionState,
    ) -> Result<(), String> {
        self.set_network_setting(0x14, team);
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let Some(first) = world.arena.first_player() else {
            return Ok(());
        };
        let unit = world
            .arena
            .player_actor(first)
            .map_err(|error| error.to_string())?;
        let stays = match unit {
            Some(unit) => {
                world
                    .arena
                    .actor(unit)
                    .map_err(|error| error.to_string())?
                    .word_b4
                    == team.wrapping_add(2)
            }
            None => false,
        };
        if !stays {
            if let Some(unit) = unit {
                self.retire_unit(unit, state)?;
            }
            self.respawn_first_player(first, state)?;
        }
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        world
            .arena
            .back_player_menu(first)
            .map_err(|error| error.to_string())?;
        // 00442CC0: the screen's T opens again at once.
        if stays {
            if let Some(screen) = self.gameplay.as_mut() {
                screen.team_menu_f0 = 0;
            }
        }
        Ok(())
    }

    /// `00442DF0`, a vehicle row's Activate.
    pub(super) fn choose_vehicle(&mut self, vehicle: u32) -> Result<(), String> {
        self.set_network_setting(0x18, vehicle);
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let Some(first) = world.arena.first_player() else {
            return Ok(());
        };
        let name = |index: u16| -> Result<Vec<u8>, String> {
            let source = crate::object_runtime::h0_text::capture_h0_text(
                crate::object_runtime::h0_text::H0TextCaptureRequest {
                    index_word: index,
                    resident: Some(&self.residency),
                },
            )
            .map_err(|error| format!("H0 {index:#x}: {error:?}"))?;
            match source {
                crate::grant_text::GrantTextSource::Record(Some(text)) => {
                    Ok(text.split(|&byte| byte == 0).next().unwrap_or(&[]).to_vec())
                }
                _ => Err(format!(
                    "00442DF0 formats H0 {index:#x}, which holds no text"
                )),
            }
        };
        let mut line = name(VEHICLE_LINE)?;
        line.push(b' ');
        line.extend(name(VEHICLE_NAMES.wrapping_add(vehicle as u16))?);
        line.push(0);
        let recipient = world
            .arena
            .player_message(first)
            .map_err(|error| error.to_string())?
            .identity();
        world
            .arena
            .apply_player_message(PlayerMessageRequest {
                recipient,
                operation: PlayerMessageOperation::Send {
                    text: Some(&line),
                    flag: 0,
                },
            })
            .map_err(|error| error.to_string())?;
        world
            .arena
            .back_player_menu(first)
            .map_err(|error| error.to_string())?;
        if let Some(screen) = self.gameplay.as_mut() {
            screen.vehicle_menu_ec = 0;
        }
        Ok(())
    }

    /// `0047CE90` on the player's unit.
    fn retire_unit(
        &mut self,
        unit: crate::object_runtime::ObjectId,
        state: &SessionState,
    ) -> Result<(), String> {
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let grant_text = current_grant_text(&self.profile_name_classification);
        let camera_settings = self.platform.world_tick_inputs().camera_settings;
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let mut owners = WorldDamageOwners {
            grant_text: Some(&grant_text),
            camera_settings: Some(camera_settings),
            flash: Some(&mut world.flash),
            mission: &mut world.mission,
            tasks: &mut self.tasks,
            terrain: &mut world.terrain,
            particles: &mut self.particles,
            sounds: &mut self.sounds,
            furniture: &mut self.furniture,
            residency: &self.residency,
            executable,
            factory_process: &mut self.factory_process,
            behavior_tables: &mut self.actor_behavior_tables,
            animation_clock: &self.process,
            session: state,
            particle_load_factor: world.fade.performance.particle_load_factor,
            behavior_factory: Some(BehaviorFactoryLoan {
                listeners: &mut world.listeners,
                sine: &self.catalog.assets().sine_table().values,
                world_flags: world.world_flags,
            }),
            map: None,
        };
        let mut host = world_retirement_host(&mut owners, &mut world.rng);
        let null = host.owners.factory_process.process_null_handle;
        world
            .arena
            .retire_actor(unit, null, &mut host)
            .map(|_| ())
            .map_err(|error| format!("0047CE90: {error:?}"))
    }

    /// `0046A890(1)` for the first player, as the held camera's respawn
    /// runs it.
    fn respawn_first_player(
        &mut self,
        player: crate::object_runtime::PlayerId,
        state: &SessionState,
    ) -> Result<(), String> {
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let sine = &self.catalog.assets().sine_table().values;
        let grants =
            inf_formats::parse_grant_table(executable).map_err(|error| error.to_string())?;
        let texts =
            inf_formats::parse_grant_texts(executable).map_err(|error| error.to_string())?;
        let limits = inf_formats::parse_vehicle_upgrade_limits(executable)
            .map_err(|error| error.to_string())?;
        let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable)
            .map_err(|error| error.to_string())?;
        let inverse_sine = super::network_game::inverse_sine_table(executable)?;
        let grant_text = current_grant_text(&self.profile_name_classification);
        let pickup_text = pickup_text(&self.profile_name_classification);
        let inputs = self.platform.world_tick_inputs();
        let clock = crate::frontend_camera::AnimationClockSource::animation_value(&self.process);
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let current_world_a0 = world.mission.current_world_a0;
        let mut camera = WorldCameraFactoryContext::new(WorldCameraInputs {
            residency: &self.residency,
            sine_quarter: sine,
            settings: inputs.camera_settings,
            process: Some(&self.factory_process),
            animation_clock: clock,
        });
        let mut factory = camera.factory(&world.terrain, world.world_flags, &mut world.listeners);
        let mut host = WorldRespawnHost {
            residency: &self.residency,
            terrain: &world.terrain,
            world_start: &mut world.world_start,
            rng: &mut world.rng,
            process: &self.factory_process,
            sounds: &mut self.sounds,
            particles: &mut self.particles,
            executable,
            grant_text: Some(&grant_text),
            pickup_text: Some(&pickup_text),
            sine,
            clock,
            world_flags: world.world_flags,
            camera_settings: inputs.camera_settings,
            phase: u32::from(state.current_phase().id()),
            header: world.respawn_header,
            loss_limit: world.briefing_milestone_count,
            mission: &mut world.mission,
            current_world_a0,
            settings_84: inputs.settings_84,
            pickups: PickupTables {
                grants: &grants,
                texts: &texts,
                upgrade_limits: &limits,
                network_active: self.factory_process.network_flag != 0,
            },
            vehicle_table: &vehicle_table,
            inverse_sine: &inverse_sine,
        };
        host.respawn(&mut world.arena, player, &mut factory)
    }
}
