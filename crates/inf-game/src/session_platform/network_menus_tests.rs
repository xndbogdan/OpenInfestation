use super::*;
use crate::menu::MenuData;
use crate::session::{TickCallback, TransitionEffect};
use crate::session_platform::network_menus::*;
use inf_formats::FrontendExecutable;

fn executable() -> FrontendExecutable<'static> {
    static ASSETS: std::sync::OnceLock<FrontendAssets> = std::sync::OnceLock::new();
    let assets = ASSETS.get_or_init(|| {
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap()
    });
    FrontendExecutable::parse(assets.executable_bytes()).unwrap()
}

/// Settings with the mode `+10` and the arena `+1C`.
fn settings(mode: u32, arena: u32) -> [u32; 20] {
    let mut words = [0; 20];
    words[0x10 / 4] = mode;
    words[0x1c / 4] = arena;
    words
}

/// The rows' action callbacks and arguments, after the header.
fn rows(data: &MenuData) -> Vec<(u32, u32)> {
    data.records().items[1..]
        .iter()
        .map(|row| (row.action_callback, row.action_arguments[0]))
        .collect()
}

#[test]
fn t_and_y_list_the_arenas_teams_and_vehicles_by_mode() {
    // Capture the Banner (arena 2: two teams, three vehicles, mode 0).
    let banner = settings(0, 2);
    let teams = network_menu(TEAM_MENU, executable(), &banner)
        .unwrap()
        .unwrap();
    assert_eq!(rows(&teams), [(TEAM_ROW, 0), (TEAM_ROW, 1)]);
    let records = teams.records();
    assert_eq!(records.initial_selection, 1);
    assert_eq!(
        (
            records.layouts[1].item_count,
            records.layouts[1].visible_rows
        ),
        (2, 2)
    );
    drop(records);
    let vehicles = network_menu(VEHICLE_MENU, executable(), &banner)
        .unwrap()
        .unwrap();
    assert_eq!(
        rows(&vehicles),
        [(VEHICLE_ROW, 0), (VEHICLE_ROW, 1), (VEHICLE_ROW, 2)]
    );
    // Powerball (mode 4) has teams but no vehicle menu.
    assert!(network_menu(TEAM_MENU, executable(), &settings(4, 0))
        .unwrap()
        .is_some());
    assert!(network_menu(VEHICLE_MENU, executable(), &settings(4, 0))
        .unwrap()
        .is_none());
    // Buggy Madness (mode 2) has neither; Four Play Heaven's four teams.
    assert!(network_menu(TEAM_MENU, executable(), &settings(2, 5))
        .unwrap()
        .is_none());
    assert!(network_menu(VEHICLE_MENU, executable(), &settings(2, 5))
        .unwrap()
        .is_none());
    let four = network_menu(TEAM_MENU, executable(), &settings(0, 7))
        .unwrap()
        .unwrap();
    assert_eq!(rows(&four).len(), 4);
}

/// The host of Capture the Banner (world 33) in the world, its first player
/// spectating from the camera.
fn banner_host() -> (PortSession<Recording>, SessionState) {
    let (mut session, _) = fixture();
    session.mission_override = Some(33);
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session.factory_process.network_flag = 1;
    session.factory_process.host_flag = 1;
    session.factory_process.local_slot = 1;
    session.factory_process.settings = settings(0, 2);
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session
        .tick(TickCallback::Gameplay, 40_000, 40_000, &mut state)
        .unwrap();
    (session, state)
}

/// 00442C60: choosing team 1 respawns the player's vehicle for state 3 at
/// one of that team's markers; choosing it again keeps the vehicle.
#[test]
fn a_team_row_respawns_the_player_with_that_team() {
    let (mut session, mut state) = banner_host();
    session.choose_team(1, &mut state).unwrap();
    assert_eq!(session.factory_process.settings[0x14 / 4], 1);
    let arena = &mut session.world.as_mut().unwrap().arena;
    let first = arena.first_player().unwrap();
    let vehicle = arena.player_actor(first).unwrap().unwrap();
    assert!(arena.selector2_actor(vehicle).is_ok());
    assert_eq!(arena.actor(vehicle).unwrap().word_b4, 3);
    let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
    assert!(arena.spawn_markers().iter().any(|&marker| {
        let state = arena.selector45(marker).unwrap();
        let at = arena.object(marker).unwrap().base.position_words.unwrap();
        state.team_84 == 3
            && [0, 2].map(|axis| at[axis].wrapping_add(state.offset_78[axis]))
                == [position[0], position[2]]
    }));
    session.choose_team(1, &mut state).unwrap();
    let arena = &mut session.world.as_mut().unwrap().arena;
    assert_eq!(arena.player_actor(first).unwrap(), Some(vehicle));
}

/// 00442DF0: a vehicle row sets the next respawn's vehicle (+18).
#[test]
fn a_vehicle_row_sets_the_next_vehicle() {
    let (mut session, _state) = banner_host();
    session.choose_vehicle(2).unwrap();
    assert_eq!(session.factory_process.settings[0x18 / 4], 2);
}
