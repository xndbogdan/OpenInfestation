use super::*;
use crate::object_runtime::{
    ObjectArena, PlayerId, PlayerMessageOperation, PlayerMessageRequest, ShownMenuOwner,
};
use serde_json::Value;

fn number(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn boxes(arena: &mut ObjectArena, player: PlayerId) -> &mut PlayerMessageBoxes {
    &mut arena
        .players
        .message_boxes
        .iter_mut()
        .find(|(owner, _)| *owner == player)
        .unwrap()
        .1
}

/// The draw sets a controller's +25 between updates; mirror it.
fn sync_drawn(arena: &mut ObjectArena, player: PlayerId, state: &Value) {
    if !arena
        .players
        .message_boxes
        .iter()
        .any(|(owner, _)| *owner == player)
    {
        return;
    }
    let boxes = boxes(arena, player);
    for (live, native) in [
        (boxes.lines.as_mut(), &state["lines"]),
        (boxes.hints.as_mut(), &state["hints"]),
    ] {
        if let (Some(live), Some(native)) = (live, native.as_array()) {
            if number(&native[2]) != 0 {
                live.stack.mark_drawn();
            }
            // The menu draw also writes each row's height (+1C).
            if let (Some(menu), Some(rows)) = (live.stack.current(), native[20].as_array()) {
                let mut records = menu.data().records_mut();
                for (item, row) in records.items.iter_mut().zip(rows) {
                    item.row_height = number(&row[2]) as u16;
                }
            }
        }
    }
}

/// The fields of one box the fixture records, in its order.
fn snapshot(live: &MessageBox) -> Vec<Value> {
    let menu = live.stack.current();
    let mut out = vec![
        live.stack.fade().into(),
        u8::from(live.stack.active()).into(),
        Value::Null,
        live.display_micros.into(),
        live.next.into(),
        u8::from(live.kind == MessageBoxKind::Hints).into(),
        (live.center[0] as u16).into(),
        (live.center[1] as u16).into(),
        live.panel.duration_micros.into(),
        live.panel.remaining_micros.into(),
        live.panel.current_scale_word.into(),
        live.panel.requested_scale_word.into(),
        live.extent.into(),
        live.panel.current_stage.into(),
        live.panel.requested_stage.into(),
        live.frame_flag.into(),
        live.rotation.into(),
    ];
    match menu {
        None => out.extend([Value::Null, Value::Null, Value::Null, Value::Null]),
        Some(menu) => {
            out.push(menu.selection().into());
            out.push(menu.age_millis().into());
            out.push(menu.basis.words.to_vec().into());
            let records = menu.data().records();
            out.push(
                records
                    .items
                    .iter()
                    .map(|item| {
                        Value::from(vec![
                            Value::from(item.draw_arguments[0]),
                            Value::from(u8::from(item.draw_arguments[1] != 0)),
                            Value::from(item.row_height),
                            Value::from(item.age),
                        ])
                    })
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
    }
    out.push(
        live.ring
            .iter()
            .map(|text| match text {
                Some(bytes) => Value::from(String::from_utf8_lossy(bytes).into_owned()),
                None => Value::Null,
            })
            .collect::<Vec<_>>()
            .into(),
    );
    out
}

fn compare(label: &str, live: Option<&MessageBox>, native: &Value, failures: &mut Vec<String>) {
    match (live, native.as_array()) {
        (None, None) => {}
        (Some(_), None) => failures.push(format!("{label}: port has a box, native none")),
        (None, Some(_)) => failures.push(format!("{label}: native has a box, port none")),
        (Some(live), Some(native)) => {
            let port = snapshot(live);
            for (index, (port, native)) in port.iter().zip(native).enumerate() {
                // +25 is synced from the draw, not simulated here.
                if index != 2 && port != native {
                    failures.push(format!(
                        "{label} field {index}: port {port} native {native}"
                    ));
                }
            }
        }
    }
}

/// Every enabled message send (22), message update (717) and message
/// command (523) Infest01 made in Level 1, in recording order from the
/// briefing teardown, on a port player whose boxes start empty. The draw's
/// writes (+25 and the row heights) are mirrored between calls.
#[test]
fn message_boxes_follow_infest01_level1() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/player_message_boxes_native.json"
    ))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let mut arena = ObjectArena::new();
    let player = arena.add_world_player([0; 6]);
    let recipient = arena.player_message(player).unwrap().identity();
    // 0040AA79: the briefing's teardown enables the message.
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    arena.set_player_message_environment(PlayerMessageEnvironment {
        menus: PlayerMessageMenus::parse(&assets).unwrap(),
        math: MenuBasisMath::new(assets.sine_table().values),
        network: 0,
        visibility: MenuVisibility::OwnerFlagSet,
        pause_on_hints: false,
        attract: false,
        ui_state: 4,
        cheats: Default::default(),
    });
    let calls = fixture["calls"].as_array().unwrap();
    let mut failures = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        sync_drawn(&mut arena, player, &call["in"]);
        let result = match call["kind"].as_str().unwrap() {
            "send" => {
                let mut text = call["text"].as_str().unwrap().as_bytes().to_vec();
                text.push(0);
                arena.apply_player_message(PlayerMessageRequest {
                    recipient,
                    operation: PlayerMessageOperation::Send {
                        text: Some(&text),
                        flag: number(&call["a2"]) as u32,
                    },
                })
            }
            "update" => arena.update_player_message(player, number(&call["a1"]) as i32, false),
            _ => {
                arena.players.message_environment.as_mut().unwrap().ui_state =
                    number(&call["ui"]) as u32;
                arena.player_message_command(
                    player,
                    number(&call["a1"]) as u32,
                    &mut std::collections::BTreeSet::new(),
                )
            }
        };
        if let Err(error) = result {
            failures.push(format!("call {index}: {error}"));
            break;
        }
        // A call that nests the next one (the shop's action sending a hint
        // inside a select) returns with that call's writes; compare after it.
        if call["nests"].as_bool().unwrap() {
            continue;
        }
        let mut found = Vec::new();
        let live = arena
            .players
            .message_boxes
            .iter()
            .find(|(owner, _)| *owner == player)
            .map(|(_, boxes)| boxes);
        compare(
            "lines",
            live.and_then(|boxes| boxes.lines.as_ref()),
            &call["out"]["lines"],
            &mut found,
        );
        compare(
            "hints",
            live.and_then(|boxes| boxes.hints.as_ref()),
            &call["out"]["hints"],
            &mut found,
        );
        if !found.is_empty() {
            failures.push(format!("call {index}: {}", found.join("; ")));
            if failures.len() > 8 {
                break;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// 00413870 with flag 1 (the paused tick 00438950): the line box never
/// updates, and the hint box only under UI state 6.
#[test]
fn paused_update_ages_only_the_hint_box_under_ui_state_6() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let mut arena = ObjectArena::new();
    let player = arena.add_world_player([0; 6]);
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    arena.set_player_message_environment(PlayerMessageEnvironment {
        menus: PlayerMessageMenus::parse(&assets).unwrap(),
        math: MenuBasisMath::new(assets.sine_table().values),
        network: 0,
        visibility: MenuVisibility::OwnerFlagSet,
        pause_on_hints: true,
        attract: false,
        ui_state: 4,
        cheats: Default::default(),
    });
    for (text, flag) in [
        (&b"Collected Tokens\0"[..], 0),
        (&b"Pausing on hints\0"[..], 1),
    ] {
        arena
            .apply_player_message(PlayerMessageRequest {
                recipient,
                operation: PlayerMessageOperation::Send {
                    text: Some(text),
                    flag,
                },
            })
            .unwrap();
    }
    // The hint asked for the paused phase.
    assert_eq!(
        arena.take_player_message_effects(),
        [PlayerMessageEffect::RequestUiState(6)]
    );
    let displays = |arena: &mut ObjectArena| {
        let boxes = boxes(arena, player);
        (
            boxes.lines.as_ref().unwrap().display_micros,
            boxes.hints.as_ref().unwrap().display_micros,
        )
    };
    let start = displays(&mut arena);
    // Outside UI state 6 the paused update changes neither box.
    arena.update_player_message(player, 100_000, true).unwrap();
    assert_eq!(displays(&mut arena), start);
    arena.players.message_environment.as_mut().unwrap().ui_state = 6;
    arena.update_player_message(player, 100_000, true).unwrap();
    assert_eq!(displays(&mut arena), (start.0, start.1 - 100_000));
    // The gameplay update (flag 0) ages both.
    arena.update_player_message(player, 100_000, false).unwrap();
    assert_eq!(displays(&mut arena), (start.0 - 100_000, start.1 - 200_000));
}

fn computer_arena(
    cheat_menus: bool,
) -> (
    ObjectArena,
    PlayerId,
    crate::frontend_assets::FrontendAssets,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let mut arena = ObjectArena::new();
    let player = arena.add_world_player([0; 6]);
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    arena.set_player_message_environment(PlayerMessageEnvironment {
        menus: PlayerMessageMenus::parse(&assets).unwrap(),
        math: MenuBasisMath::new(assets.sine_table().values),
        network: 0,
        visibility: MenuVisibility::OwnerFlagSet,
        pause_on_hints: false,
        attract: false,
        ui_state: if cheat_menus { 5 } else { 4 },
        cheats: Default::default(),
    });
    (arena, player, assets)
}

/// Tab (00413410) shows the computer, its cheats row flagged only for a
/// cheat bit outside 0xC0, and hides it again.
#[test]
fn tab_shows_the_computer_and_hides_it_again() {
    for (cheat_word, flags) in [(0, 0), (0xc0, 0), (0x01, 1)] {
        let (mut arena, player, _) = computer_arena(false);
        assert!(arena.toggle_player_computer(player, cheat_word).unwrap());
        assert_eq!(arena.world_player(player).unwrap().message_byte_00(), 1);
        let menu = arena.player_shown_menu(player).unwrap();
        assert_eq!(menu.current_owner(), Some(ShownMenuOwner::Plain));
        let current = menu.stack.current().unwrap();
        let records = current.data().records();
        assert_eq!(records.items.len(), 7);
        assert_eq!(records.items[1].action_callback, 0x0041_5170);
        assert_eq!(records.items[6].flags & 1, flags, "{cheat_word:#x}");
        drop(records);
        assert!(menu.stack.active());
        assert!(arena.toggle_player_computer(player, cheat_word).unwrap());
        assert_eq!(arena.world_player(player).unwrap().message_byte_00(), 0);
        assert!(!arena.player_shown_menu(player).unwrap().stack.active());
    }
}

/// The phase-5 screen's Escape (004133E0) pops a pushed child, then the
/// in-world menu itself, whose destructor waits with its return phase, and
/// hides the message once no menu is left.
#[test]
fn escape_backs_out_of_the_in_world_menu_into_its_destructor() {
    let (mut arena, player, assets) = computer_arena(false);
    let menu = |address| {
        crate::menu::MenuState::new(
            MenuData::new(assets.menu_definition(address).unwrap()),
            crate::menu::MenuBasis::INITIAL,
        )
    };
    arena
        .show_player_menu(
            player,
            0,
            menu(0x0053_5348),
            ShownMenuOwner::InWorld { return_phase: 4 },
        )
        .unwrap();
    arena
        .push_player_menu(player, menu(0x0053_5280), ShownMenuOwner::Plain)
        .unwrap();
    arena.back_player_menu(player).unwrap();
    assert_eq!(arena.world_player(player).unwrap().message_byte_00(), 1);
    assert_eq!(
        arena.take_destroyed_menus(),
        [(player, ShownMenuOwner::Plain)]
    );
    arena.back_player_menu(player).unwrap();
    assert_eq!(arena.world_player(player).unwrap().message_byte_00(), 0);
    assert_eq!(
        arena.take_destroyed_menus(),
        [(player, ShownMenuOwner::InWorld { return_phase: 4 })]
    );
}

/// The database (00414520) steps through the entries the player knows,
/// skipping flagged ones and wrapping over 1..0xAE, and shows the entry's
/// text in its title row; codes other than Previous and Next end there.
/// The hint list (00414610) steps through the hints the +130 mask has seen.
#[test]
fn the_database_and_hint_list_step_through_what_the_player_knows() {
    let (mut arena, player, assets) = computer_arena(false);
    let database = PlayerMessageMenus::parse(&assets).unwrap().database;
    let listed: Vec<u16> = (1..0xaf)
        .filter(|&index| database[index as usize].flags & 2 == 0)
        .collect();
    let (first, second) = (listed[3], listed[10]);
    {
        let state = arena.world_player_mut(player).unwrap();
        state.grants_9c = [0; inf_formats::GRANT_MASK_WORDS];
        for index in [first, second] {
            state.grants_9c[usize::from(index >> 5)] |= 1 << (index & 31);
        }
        state.command_mask_130 = [0; 3];
        state.command_mask_130[0] |= 1 << 3;
        state.command_mask_130[1] |= 1 << 8;
    }
    let menu = crate::menu::MenuState::new(
        MenuData::new(assets.menu_definition(0x0052_fa60).unwrap()),
        crate::menu::MenuBasis::INITIAL,
    );
    arena
        .show_player_menu(player, 0, menu, ShownMenuOwner::Database { entry: 1 })
        .unwrap();
    let entry = |arena: &ObjectArena| arena.player_shown_menu(player).unwrap().current_owner();
    assert!(arena.step_shown_list(player, 1).unwrap());
    assert_eq!(
        entry(&arena),
        Some(ShownMenuOwner::Database { entry: first })
    );
    assert!(arena.step_shown_list(player, 1).unwrap());
    assert_eq!(
        entry(&arena),
        Some(ShownMenuOwner::Database { entry: second })
    );
    // Next wraps past 0xAE to the first known entry again.
    assert!(arena.step_shown_list(player, 1).unwrap());
    assert_eq!(
        entry(&arena),
        Some(ShownMenuOwner::Database { entry: first })
    );
    assert!(arena.step_shown_list(player, 0).unwrap());
    assert_eq!(
        entry(&arena),
        Some(ShownMenuOwner::Database { entry: second })
    );
    {
        let shown = arena.player_shown_menu(player).unwrap();
        let records = shown.stack.current().unwrap().data().records();
        assert_eq!(records.items[1].draw_arguments[0], u32::from(second));
        assert_eq!(
            records.items[0].draw_arguments[0],
            u32::from(database[usize::from(second)].text)
        );
    }
    for code in [2, 3, 4, 5] {
        assert!(!arena.step_shown_list(player, code).unwrap(), "{code}");
    }

    let menu = crate::menu::MenuState::new(
        MenuData::new(assets.menu_definition(0x0052_fae8).unwrap()),
        crate::menu::MenuBasis::INITIAL,
    );
    arena
        .push_player_menu(player, menu, ShownMenuOwner::HintList { hint: 0x25 })
        .unwrap();
    for (code, hint) in [(1, 40), (3, 3), (4, 40), (0, 3), (2, 40)] {
        assert!(arena.step_shown_list(player, code).unwrap());
        assert_eq!(
            entry(&arena),
            Some(ShownMenuOwner::HintList { hint }),
            "{code}"
        );
    }
    assert!(!arena.step_shown_list(player, 5).unwrap());
}

/// Retail's database draws only the current world's models (00414C80). With
/// Level 1's overlays resident, the entries whose model another world
/// brings resolve in a second residency activated at the first overlay
/// PreLoad registers the model for (the port's database model fix).
#[test]
fn another_worlds_database_models_resolve_in_a_second_residency() {
    use crate::{Installation, ResidencyRequest, ResidentRuntime};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let database = PlayerMessageMenus::parse(&assets).unwrap().database;
    let mut world = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    world.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 32] {
        world.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    let handler = crate::render_resources::RenderResourceKind::Model.handler();
    let mut missing = std::collections::BTreeMap::<u8, Vec<u16>>::new();
    for entry in &database[1..] {
        if entry.model != 0 && world.resource(handler, entry.model).is_none() {
            let leaves = world.leaves_registering(handler, entry.model);
            let leaf = *leaves
                .first()
                .unwrap_or_else(|| panic!("model {:#x} has no overlay in PreLoad", entry.model));
            missing.entry(leaf).or_default().push(entry.model);
        }
    }
    assert!(!missing.is_empty());
    for (leaf, models) in &missing {
        let mut second = ResidentRuntime::new(world.installation().clone()).unwrap();
        second.apply(ResidencyRequest::Boot).unwrap();
        second
            .apply(ResidencyRequest::Activate { level: *leaf })
            .unwrap();
        for &model in models {
            assert!(
                second.resource(handler, model).is_some(),
                "model {model:#x} from overlay {leaf}"
            );
        }
    }
}
