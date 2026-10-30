//! The Cheats Menu's rows through the session's dispatch, on loaded worlds.

use super::*;
use crate::menu::{MenuActionRequest, MenuCommand, MenuSource};
use crate::object_runtime::{
    ActorComponentQueryMode, ActorComponentQueryRequest, ObjectArena, ObjectClass, ObjectList,
    PlayerId, ShownMenuAction, ShownMenuOwner,
};
use crate::session::{TickCallback, TransitionEffect};
use crate::session_platform::cheats::*;

/// The Cheats Menu's definition, the computer's row 6 argument.
const CHEATS_DEFINITION: u32 = 0x0052_f9d8;

/// `world` loaded (Level 1, world 18, without one) under the cheat word,
/// ticked once so its first player's message has its environment.
fn loaded(world: Option<u32>, cheats: u32) -> (PortSession<Recording>, SessionState, PlayerId) {
    let (mut session, _) = fixture();
    session.platform.services.cheats_84 = cheats;
    session.mission_override = world;
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session
        .tick(TickCallback::Gameplay, 40_000, 40_000, &mut state)
        .unwrap();
    let player = session
        .world
        .as_ref()
        .unwrap()
        .arena
        .first_player()
        .unwrap();
    (session, state, player)
}

fn act(
    session: &mut PortSession<Recording>,
    state: &mut SessionState,
    player: PlayerId,
    callback_address: u32,
    command: MenuCommand,
    arguments: [u32; 2],
) -> Result<(), String> {
    session.run_shown_menu_action(
        ShownMenuAction {
            player,
            owner: ShownMenuOwner::Plain,
            request: MenuActionRequest {
                menu_source: MenuSource::Derived {
                    constructor: 0,
                    vtable: 0,
                },
                item_index: 0,
                callback_address,
                command,
                arguments,
                context: 0,
            },
        },
        state,
    )
}

fn arena(session: &mut PortSession<Recording>) -> &mut ObjectArena {
    &mut session.world.as_mut().unwrap().arena
}

fn known(arena: &ObjectArena, player: PlayerId, id: u16) -> bool {
    let grants = arena.world_player(player).unwrap().grants_9c;
    inf_formats::grant_is_owned(&grants, u32::from(id)).unwrap()
}

fn owned(arena: &ObjectArena, player: PlayerId, id: u16) -> bool {
    arena.player_upgrade_owned(player, u32::from(id)).unwrap()
}

/// The flag-0 list's objects of `resource`.
fn listed(arena: &ObjectArena, resource: u16) -> usize {
    arena
        .list(ObjectList::Flag0Primary)
        .iter()
        .filter(|node| arena.object(node.object).unwrap().base.resource_word == resource)
        .count()
}

/// The live aliens the sweep retires: side 0x12, no factory, not resource
/// 0xF, lifecycle 2 or 3.
fn live_aliens(arena: &ObjectArena) -> usize {
    arena
        .list(ObjectList::Flag0Primary)
        .iter()
        .map(|node| node.object)
        .filter(|&object| {
            let Ok(actor) = arena.actor(object) else {
                return false;
            };
            let factory = arena
                .query_actor_component(ActorComponentQueryRequest {
                    actor: object,
                    mode: ActorComponentQueryMode::Self50,
                })
                .unwrap();
            actor.word_b4 == 0x12
                && factory.is_none()
                && arena.object(object).unwrap().base.resource_word != 0x0f
                && matches!(actor.word_c4, 2 | 3)
        })
        .count()
}

/// The vehicle's free hold slots.
fn free_slots(session: &mut PortSession<Recording>, player: PlayerId) -> usize {
    let null = session.factory_process.process_null_handle.unwrap();
    let arena = arena(session);
    let vehicle = arena.player_actor(player).unwrap().unwrap();
    arena
        .selector2_actor(vehicle)
        .unwrap()
        .handles_1a4
        .handles
        .iter()
        .filter(|&&handle| handle == null)
        .count()
}

/// Skip to's tables as the executable holds them: world 2 is the first
/// stage, with Command Centre (18); Altair Prime (5) the fourth; Alien
/// Brain (19) the last named; a world no stage names takes the last
/// stage opened.
#[test]
fn skip_to_reads_its_stages_grants_and_tokens() {
    let (session, _) = fixture();
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let stages = table_bytes(executable, SKIP_STAGES).unwrap();
    let grants = table_bytes(executable, SKIP_GRANTS).unwrap();
    let tokens = table_bytes(executable, SKIP_TOKENS).unwrap();
    for (world, stage) in [
        (18, 1),
        (2, 1),
        (20, 2),
        (5, 4),
        (3, 10),
        (22, 10),
        (19, 19),
        (1, 20),
    ] {
        assert_eq!(skip_stage(world, &stages), stage, "world {world}");
    }
    assert_eq!(skip_grants(1, &grants), [0x02, 0x03, 0x25]);
    assert_eq!(
        skip_grants(4, &grants),
        [0x02, 0x03, 0x25, 0xa8, 0x11, 0x84, 0x9b, 0x85, 0x1f, 0x18]
    );
    // Stage 12 lists no ID of its own.
    assert_eq!(skip_grants(12, &grants), skip_grants(11, &grants));
    assert_eq!(skip_token_level(1, &tokens), 77 / 7);
    assert_eq!(skip_token_level(4, &tokens), (77 + 52 + 22 + 23) / 7);
    assert_eq!(skip_token_level(19, &tokens), 538 / 7);
}

/// `00413410`'s rows: each cheat bit opens its rows, the selection the
/// first open one; bits only outside 0xC0... leave the menu as it was.
#[test]
fn the_cheats_menu_opens_the_rows_of_the_cheat_word() {
    let (mut session, mut state, player) = loaded(None, 0);
    for (cheats, open, selection) in [
        (0x27, [true; 8], 1),
        (
            0x04,
            [false, true, true, true, false, false, false, false],
            2,
        ),
        (
            0x22,
            [false, false, false, false, true, false, false, true],
            5,
        ),
        // 0xC0 alone changes nothing: the last word's rows and selection.
        (
            0xc0,
            [false, false, false, false, true, false, false, true],
            5,
        ),
        (
            0x01,
            [true, false, false, false, false, true, true, false],
            1,
        ),
    ] {
        session.platform.services.cheats_84 = cheats;
        act(
            &mut session,
            &mut state,
            player,
            CHEATS_MENU,
            MenuCommand::Activate,
            [3, CHEATS_DEFINITION],
        )
        .unwrap();
        let shown = arena(&mut session).player_shown_menu(player).unwrap();
        assert_eq!(shown.current_owner(), Some(ShownMenuOwner::Plain));
        let records = shown.stack.current().unwrap().data().records();
        let flags: Vec<bool> = (1..9).map(|row| records.items[row].flags == 1).collect();
        assert_eq!(flags, open, "{cheats:#x}");
        assert_eq!(records.initial_selection, selection, "{cheats:#x}");
    }
}

/// Skip to's Left and Right step its world through 2 to 24, wrapping, and
/// the row's world reaches the message's environment, which its draw reads.
#[test]
fn skip_to_steps_its_world_and_wraps() {
    let (mut session, mut state, player) = loaded(None, 1);
    assert_eq!(session.computer_cheats.skip_world, 2);
    for (command, world) in [
        (MenuCommand::AdjustPrevious, 24),
        (MenuCommand::AdjustNext, 2),
        (MenuCommand::AdjustNext, 3),
        (MenuCommand::Previous, 3),
    ] {
        act(
            &mut session,
            &mut state,
            player,
            SKIP_TO,
            command,
            [0, SKIP_WORLD_CELL],
        )
        .unwrap();
        assert_eq!(session.computer_cheats.skip_world, world);
        let environment = arena(&mut session)
            .player_message_environment_mut()
            .unwrap();
        assert_eq!(environment.cheats.skip_world, world);
    }
}

/// Skip to's Select gives the first player the campaign's grants up to the
/// world's stage, its token levels, a hold full of scientists and full
/// crystal tanks, and ends this world into that one, the message silent
/// meanwhile and speaking again after.
#[test]
fn skip_to_gives_the_stage_and_ends_the_world_into_it() {
    let (mut session, mut state, player) = loaded(None, 1);
    session.computer_cheats.skip_world = 5;
    assert!(free_slots(&mut session, player) > 0);
    act(
        &mut session,
        &mut state,
        player,
        SKIP_TO,
        MenuCommand::Activate,
        [0, SKIP_WORLD_CELL],
    )
    .unwrap();
    let world = session.world.as_ref().unwrap();
    for id in [0x02, 0x03, 0x25, 0xa8, 0x11, 0x84, 0x9b, 0x85, 0x1f, 0x18] {
        assert!(known(&world.arena, player, id), "{id:#x} known");
        assert!(owned(&world.arena, player, id), "{id:#x} owned");
    }
    assert!(
        !owned(&world.arena, player, 0xa0),
        "a later stage's Rockets"
    );
    let state_after = world.arena.world_player(player).unwrap();
    assert_eq!(state_after.bytes_13c_143[..7], [24; 7]);
    assert_eq!(state_after.bytes_13c_143[7], 0);
    assert_eq!(state_after.dwords_120_128, [30; 3]);
    assert_eq!(
        world
            .arena
            .player_message(player)
            .unwrap()
            .suppression_byte(),
        0
    );
    assert_eq!(world.mission.save.mission_selection, 5);
    assert_eq!(state.requested_phase(), SessionPhase::Phase8);
    assert_eq!(free_slots(&mut session, player), 0);
}

/// Show Full Database flips its byte whatever the code. While it is set,
/// the database lists an entry the player does not know, and its Select
/// grants the shown entry.
#[test]
fn the_full_database_lists_and_grants_every_entry() {
    let (mut session, mut state, player) = loaded(None, 4);
    for (command, full) in [
        (MenuCommand::Activate, true),
        (MenuCommand::AdjustNext, false),
        (MenuCommand::AdjustPrevious, true),
    ] {
        act(
            &mut session,
            &mut state,
            player,
            SHOW_FULL_DATABASE,
            command,
            [0, FULL_DATABASE_CELL],
        )
        .unwrap();
        assert_eq!(session.computer_cheats.full_database, full);
        let environment = arena(&mut session)
            .player_message_environment_mut()
            .unwrap();
        assert_eq!(environment.cheats.full_database, full);
    }
    // Access Database: entry 1, then Next steps to the next listed entry.
    act(
        &mut session,
        &mut state,
        player,
        0x0041_4830,
        MenuCommand::Activate,
        [3, 0x0052_fa60],
    )
    .unwrap();
    let database = arena(&mut session)
        .player_message_environment_mut()
        .unwrap()
        .menus
        .database
        .clone();
    let next = (2..0xaf)
        .find(|&index| database[index as usize].flags & 2 == 0)
        .unwrap();
    assert!(
        !known(arena(&mut session), player, next),
        "{next:#x} unknown at the start"
    );
    assert!(arena(&mut session).step_shown_list(player, 1).unwrap());
    assert_eq!(
        arena(&mut session)
            .player_shown_menu(player)
            .unwrap()
            .current_owner(),
        Some(ShownMenuOwner::Database { entry: next })
    );
    assert!(arena(&mut session).step_shown_list(player, 4).unwrap());
    let actions = arena(&mut session).take_shown_menu_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].request.callback_address,
        crate::object_runtime::DATABASE_SELECT
    );
    session
        .run_shown_menu_action(actions[0], &mut state)
        .unwrap();
    assert!(known(arena(&mut session), player, next));
    assert!(owned(arena(&mut session), player, next));
}

/// Get Full Research Tree knows every ID; Get Full Ship Upgrades owns every
/// one but Invulnerability, which its own row grants; Fill resource
/// crystal tanks fills the three counters to 30 (20 units, two each).
#[test]
fn the_grant_rows_give_what_they_name() {
    let (mut session, mut state, player) = loaded(None, 0x27);
    act(
        &mut session,
        &mut state,
        player,
        FULL_RESEARCH_TREE,
        MenuCommand::AdjustNext,
        [0, 0],
    )
    .unwrap();
    assert!((0..0xaf).all(|id| known(arena(&mut session), player, id)));
    let tanks = arena(&mut session)
        .world_player(player)
        .unwrap()
        .dwords_120_128;
    act(
        &mut session,
        &mut state,
        player,
        FULL_SHIP_UPGRADES,
        MenuCommand::AdjustNext,
        [0, 0],
    )
    .unwrap();
    assert!(!owned(arena(&mut session), player, 0x98), "Select only");
    act(
        &mut session,
        &mut state,
        player,
        FULL_SHIP_UPGRADES,
        MenuCommand::Activate,
        [0, 0],
    )
    .unwrap();
    assert!((0..0xaf)
        .filter(|&id| id != 0x29)
        .all(|id| owned(arena(&mut session), player, id)));
    assert!(!owned(arena(&mut session), player, 0x29));
    act(
        &mut session,
        &mut state,
        player,
        INVULNERABILITY,
        MenuCommand::Activate,
        [0, 0],
    )
    .unwrap();
    assert!(owned(arena(&mut session), player, 0x29));
    assert_eq!(
        arena(&mut session)
            .world_player(player)
            .unwrap()
            .dwords_120_128,
        tanks.map(|tank| (tank + 16).min(30)),
        "the ship upgrades' 8 units, two each"
    );
    act(
        &mut session,
        &mut state,
        player,
        FILL_CRYSTAL_TANKS,
        MenuCommand::Activate,
        [0, 0],
    )
    .unwrap();
    assert_eq!(
        arena(&mut session)
            .world_player(player)
            .unwrap()
            .dwords_120_128,
        [30; 3]
    );
}

/// Complete Level on Terra Nova: no live alien is left, pickups are taken
/// and the scientists fill the hold. Scavange Level then teaches every
/// actor's grant.
#[test]
fn complete_and_scavange_level_sweep_terra_nova() {
    let (mut session, mut state, player) = loaded(Some(2), 1);
    let (aliens, pickups) = (
        live_aliens(arena(&mut session)),
        listed(arena(&mut session), 9),
    );
    assert!(
        aliens > 0 && pickups > 0,
        "{aliens} aliens, {pickups} pickups"
    );
    let free = free_slots(&mut session, player);
    act(
        &mut session,
        &mut state,
        player,
        COMPLETE_LEVEL,
        MenuCommand::AdjustNext,
        [0, 0],
    )
    .unwrap();
    assert_eq!(live_aliens(arena(&mut session)), aliens, "Select only");
    act(
        &mut session,
        &mut state,
        player,
        COMPLETE_LEVEL,
        MenuCommand::Activate,
        [0, 0],
    )
    .unwrap();
    assert_eq!(live_aliens(arena(&mut session)), 0);
    assert!(listed(arena(&mut session), 9) < pickups);
    assert!(free_slots(&mut session, player) < free);
    let grants: Vec<u16> = {
        let arena = arena(&mut session);
        arena
            .list(ObjectList::Flag0Primary)
            .iter()
            .filter(|node| {
                matches!(
                    arena.object(node.object).unwrap().class,
                    ObjectClass::Actor(_)
                )
            })
            .map(|node| arena.actor(node.object).unwrap().word_7a)
            .filter(|&id| id != 0)
            .collect()
    };
    assert!(!grants.is_empty());
    act(
        &mut session,
        &mut state,
        player,
        SCAVANGE_LEVEL,
        MenuCommand::Activate,
        [0, 0],
    )
    .unwrap();
    for id in grants {
        assert!(known(arena(&mut session), player, id), "{id:#x}");
    }
}

/// Every world's sweeps, Skip to and the ship upgrades, each from a fresh
/// load, run to their end but for one held stop: Shak'tahh's stabiliser,
/// whose link +16C retail leaves to heap residue (`StabiliserLinkResidueBoundary`).
/// (The Alien Brain's eyes die through `00408560` since 2026-10-18.)
#[test]
fn every_world_takes_every_cheat() {
    let mut stops = Vec::new();
    for world in 2..=24 {
        for callback in [COMPLETE_LEVEL, SCAVANGE_LEVEL, SKIP_TO, FULL_SHIP_UPGRADES] {
            let (mut session, mut state, player) = loaded(Some(world), 0x27);
            let arguments = if callback == SKIP_TO {
                [0, SKIP_WORLD_CELL]
            } else {
                [0, 0]
            };
            if let Err(error) = act(
                &mut session,
                &mut state,
                player,
                callback,
                MenuCommand::Activate,
                arguments,
            ) {
                stops.push((world, callback, error));
            }
        }
    }
    let held = |world: u32, error: &str| {
        world == 17 && error.contains("linked actor +16C is allocation residue")
    };
    let unexpected: Vec<_> = stops
        .iter()
        .filter(|(world, callback, error)| {
            !(matches!(*callback, COMPLETE_LEVEL | SCAVANGE_LEVEL) && held(*world, error))
        })
        .collect();
    assert!(unexpected.is_empty(), "{unexpected:#?}");
    assert_eq!(stops.len(), 2, "{stops:#?}");
}

/// The port's Teleport to world shows only while all eight of retail's
/// rows are open, as a ninth row; its list holds every world 0 to 24 under
/// the menu's title.
#[test]
fn teleport_to_world_lists_every_world_with_all_cheats() {
    let (mut session, mut state, player) = loaded(None, 0x01);
    let rows = |session: &mut PortSession<Recording>| {
        let shown = arena(session).player_shown_menu(player).unwrap();
        let records = shown.stack.current().unwrap().data().records();
        records.items.to_vec()
    };
    act(
        &mut session,
        &mut state,
        player,
        CHEATS_MENU,
        MenuCommand::Activate,
        [3, CHEATS_DEFINITION],
    )
    .unwrap();
    assert_eq!(rows(&mut session).len(), 9);
    session.platform.services.cheats_84 = 0x27;
    act(
        &mut session,
        &mut state,
        player,
        CHEATS_MENU,
        MenuCommand::Activate,
        [3, CHEATS_DEFINITION],
    )
    .unwrap();
    let cheats = rows(&mut session);
    assert_eq!(cheats.len(), 10);
    assert_eq!(cheats[9].action_callback, TELEPORT_TO);
    assert_eq!(cheats[9].flags, 1);
    act(
        &mut session,
        &mut state,
        player,
        TELEPORT_TO,
        MenuCommand::Activate,
        [0, CHEATS_DEFINITION],
    )
    .unwrap();
    let list = rows(&mut session);
    assert_eq!(list.len(), 26);
    assert_eq!(list[0], cheats[0], "the menu's own title");
    let worlds: Vec<u32> = list[1..]
        .iter()
        .map(|row| row.action_arguments[0])
        .collect();
    assert_eq!(worlds, (0..25).collect::<Vec<u32>>());
    assert!(list[1..]
        .iter()
        .all(|row| row.action_callback == TELEPORT_WORLD));
    // Retail names every world from H0; with Pelagia on, world 1 is the
    // port's to name.
    assert!(list[1..].iter().all(|row| row.draw_arguments[1] == 0));
    session.platform.services.prototype_world = true;
    act(
        &mut session,
        &mut state,
        player,
        TELEPORT_TO,
        MenuCommand::Activate,
        [0, CHEATS_DEFINITION],
    )
    .unwrap();
    let named: Vec<u32> = rows(&mut session)[1..]
        .iter()
        .filter(|row| row.draw_arguments[1] == 1)
        .map(|row| row.draw_arguments[0])
        .collect();
    assert_eq!(named, [1]);
}

/// A world of the list loads at its first start, world 1 included: no
/// completion of the world left, no results page, none of Skip to's grants.
#[test]
fn teleport_to_world_loads_the_chosen_one() {
    let (mut session, mut state, player) = loaded(None, 0x27);
    let left = session.world.as_ref().unwrap().mission.current_world_a0 as usize;
    act(
        &mut session,
        &mut state,
        player,
        TELEPORT_WORLD,
        MenuCommand::Activate,
        [1, 0],
    )
    .unwrap();
    let world = session.world.as_ref().unwrap();
    assert_eq!(world.mission.save.mission_selection, 1);
    assert_eq!(world.mission.save.owner_a4, 0);
    assert_eq!(world.mission.save.owner_3c[left] & 1, 0, "not completed");
    assert_eq!(state.requested_phase(), SessionPhase::Phase3);
    assert!(!owned(&world.arena, player, 0x9b), "no campaign grants");
}

/// Leaving Helniazine (world 6) by Teleport to world stopped the session
/// (a crash, 2026-10-24): the next world's load drained the world
/// left, whose furniture marker's cleanup 00435360 needs that world's
/// terrain, already replaced. The world left now ends first, as before its
/// results (the phase machine's Gameplay to Phase 3).
#[test]
fn teleport_leaves_a_world_with_a_furniture_marker() {
    let (mut session, mut state, player) = loaded(Some(6), 0x27);
    let arena = &session.world.as_ref().unwrap().arena;
    assert!(arena.list(ObjectList::Parking).iter().any(|node| matches!(
        arena.object(node.object).map(|object| &object.class),
        Ok(ObjectClass::Selector75(_))
    )));
    act(
        &mut session,
        &mut state,
        player,
        TELEPORT_WORLD,
        MenuCommand::Activate,
        [9, 0],
    )
    .unwrap();
    for effect in [
        TransitionEffect::PrepareResults,
        TransitionEffect::LoadWorld,
    ] {
        session.transition_effect(effect, &mut state).unwrap();
    }
    let world = session.world.as_ref().unwrap();
    assert_eq!(world.mission.current_world_a0, 9);
    assert_eq!(world.mission.save.owner_3c[6] & 1, 0, "not completed");
}

/// Teleport to world lands at the world's own start, not where a save pad
/// last saved the vehicle in the world left (carrying the position of the current
/// world over is dangerous).
#[test]
fn teleport_lands_at_the_worlds_start_not_the_last_checkpoint() {
    let (mut session, mut state, player) = loaded(Some(6), 0x27);
    let checkpoint = [0x1234, 0x100, 0x5678];
    {
        let assets = session.catalog.assets();
        let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
        let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let inverse = inf_formats::parse_frontend_inverse_sine_table(
            executable.data(),
            inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
        )
        .unwrap()
        .values;
        let arena = &mut session.world.as_mut().unwrap().arena;
        let vehicle = arena.first_player_primary_actor().unwrap().unwrap();
        arena.copy_actor_position(vehicle, checkpoint).unwrap();
        arena
            .save_player_spawn(player, 0x6c6, &math, &inverse)
            .unwrap();
        assert_eq!(arena.world_player(player).unwrap().byte_ed, 1);
    }
    act(
        &mut session,
        &mut state,
        player,
        TELEPORT_WORLD,
        MenuCommand::Activate,
        [9, 0],
    )
    .unwrap();
    assert_eq!(
        session
            .world
            .as_ref()
            .unwrap()
            .arena
            .world_player(player)
            .unwrap()
            .byte_ed,
        0,
        "the checkpoint goes with the world left"
    );
    for effect in [
        TransitionEffect::PrepareResults,
        TransitionEffect::LoadWorld,
    ] {
        session.transition_effect(effect, &mut state).unwrap();
    }
    session
        .tick(TickCallback::Gameplay, 40_000, 40_000, &mut state)
        .unwrap();
    let world = session.world.as_mut().unwrap();
    assert_eq!(world.mission.current_world_a0, 9);
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    let [x, _, z] = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_ne!(
        [x, z],
        [checkpoint[0], checkpoint[2]],
        "not at the old checkpoint"
    );
}
