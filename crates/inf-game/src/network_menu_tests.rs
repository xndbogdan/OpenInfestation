use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;

use super::*;
use crate::frontend_assets::{FrontendAssets, FrontendMenuCatalog};
use crate::menu::{MenuControllerSpec, MenuSource};

const SETTINGS: u32 = 0x0054_e8a8;

#[derive(Default)]
struct Fake {
    catalog: Option<FrontendMenuCatalog>,
    calls: Vec<String>,
    sessions: Vec<SessionInfo>,
    settings: BTreeMap<u32, u32>,
    joins: bool,
    slot: (u8, i32),
    host_slot: bool,
    ready: bool,
    close: CloseToken,
}

impl NetworkMenuServices for Fake {
    type Error = String;
    fn close_token(&mut self) -> Result<CloseToken, String> {
        Ok(self.close.clone())
    }
    fn network_game_ready(&mut self) -> bool {
        self.ready
    }
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), String> {
        Ok(())
    }
    fn menu_data(&mut self, address: u32) -> Result<MenuData, String> {
        self.catalog
            .as_mut()
            .unwrap()
            .menu_data(address)
            .map_err(|error| error.to_string())
    }
    fn connections(&mut self) -> Result<Vec<String>, String> {
        Ok(vec![crate::network::udp::CONNECTION_NAME.to_string()])
    }
    fn initialize_connection(&mut self, target: SearchTarget) -> Result<(), String> {
        self.calls.push(format!("connect {target:?}"));
        Ok(())
    }
    fn enumerate_sessions(&mut self) -> Result<Vec<SessionInfo>, String> {
        self.calls.push("enumerate".into());
        Ok(self.sessions.clone())
    }
    fn join(&mut self, session: &SessionInfo) -> Result<bool, String> {
        self.calls.push(format!("join {}", session.name));
        Ok(self.joins)
    }
    fn host(&mut self) -> Result<bool, String> {
        self.calls.push("host".into());
        Ok(true)
    }
    fn host_slot(&mut self) -> Result<bool, String> {
        Ok(self.host_slot)
    }
    fn wait_receive(&mut self, clear_world: bool) -> Result<(u8, i32), String> {
        self.calls.push(format!("receive {clear_world}"));
        Ok(self.slot)
    }
    fn reset_net_game(&mut self) -> Result<(), String> {
        self.calls.push("reset".into());
        Ok(())
    }
    fn close_session(&mut self) -> Result<(), String> {
        self.calls.push("close".into());
        Ok(())
    }
    fn play_cue(&mut self, cue: i16) -> Result<(), String> {
        self.calls.push(format!("cue {cue}"));
        Ok(())
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), String> {
        self.calls.push(format!("phase {phase}"));
        Ok(())
    }
    fn set_transition_flag(&mut self) -> Result<(), String> {
        self.calls.push("transition".into());
        Ok(())
    }
    fn setting(&mut self, offset: u32) -> Result<u32, String> {
        Ok(self.settings.get(&offset).copied().unwrap_or(0))
    }
    fn set_setting(&mut self, offset: u32, value: u32) -> Result<(), String> {
        self.settings.insert(offset, value);
        Ok(())
    }
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), String> {
        let offset = address - SETTINGS;
        let value = self.settings.entry(offset).or_insert(0);
        match command {
            MenuCommand::AdjustPrevious => *value = value.saturating_sub(1),
            MenuCommand::AdjustNext => *value = (*value + 1).min(max),
            _ => {}
        }
        Ok(())
    }
    fn report(&mut self, message: &str) -> Result<(), String> {
        self.calls.push(format!("report {message}"));
        Ok(())
    }
}

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn fake() -> (Fake, NetworkMenuTemplates) {
    let assets = FrontendAssets::open(&root()).unwrap();
    let templates = NetworkMenuTemplates::parse(assets.executable_bytes()).unwrap();
    (
        Fake {
            catalog: Some(FrontendMenuCatalog::new(assets)),
            joins: true,
            host_slot: true,
            ready: true,
            ..Fake::default()
        },
        templates,
    )
}

fn stack() -> MenuStack {
    MenuStack::new(MenuControllerSpec {
        fade_rate: 65535,
        viewport: [0, 0, 1024, 1024],
    })
}

fn session(name: &str) -> SessionInfo {
    SessionInfo {
        instance: name.len() as u64,
        name: name.into(),
        address: "127.0.0.1:47630".parse::<SocketAddr>().unwrap(),
        players: 1,
        max_players: 16,
    }
}

fn activate(stack: &mut MenuStack, callback: u32, arguments: [u32; 2]) -> MenuActionRequest {
    let menu = stack.current().unwrap();
    MenuActionRequest {
        menu_source: menu.data().source(),
        item_index: 0,
        callback_address: callback,
        command: MenuCommand::Activate,
        arguments,
        context: 1,
    }
}

fn network(stack: &MenuStack) -> &NetworkMenu {
    match stack.current_owner() {
        Some(MenuOwner::Network(menu)) => menu,
        other => panic!("expected a network menu, found {other:?}"),
    }
}

#[test]
fn the_templates_and_both_host_menus_are_retails() {
    let (mut fake, templates) = fake();
    assert_eq!(
        (
            templates.header.draw_callback,
            templates.header.draw_arguments[0]
        ),
        (0x0045_f520, 0x32)
    );
    assert_eq!(templates.host.draw_arguments[0], 0x21);
    assert_eq!(templates.host.action_arguments[0], u32::MAX);
    for row in [&templates.session, &templates.connection] {
        assert_eq!(
            (row.draw_callback, row.action_callback),
            (NAME_ROW, LIST_ACTION)
        );
    }
    let host = fake.menu_data(HOST_MENU).unwrap();
    let relaid = fake.menu_data(HOST_MENU_RELAID).unwrap();
    // Both bind 00538510: a flag written through one is the other's.
    host.records_mut().items[5].flags = 0x55;
    assert_eq!(relaid.records().items[5].flags, 0x55);
    assert_eq!(host.records().items.len(), 10);
    assert_eq!(host.records().items[8].action_callback, START_ACTION);
}

/// Network opens the connections; choosing one asks for the address, which
/// takes typed keys; confirming it connects and lists the sessions.
#[test]
fn a_connection_asks_for_an_address_then_lists_the_sessions() {
    let (mut fake, templates) = fake();
    let layouts = fake.menu_data(HOST_MENU).unwrap().layouts();
    let mut stack = stack();
    stack.push(NetworkMenu::connections(
        &templates,
        layouts.clone(),
        fake.connections().unwrap(),
    ));
    assert_eq!(network(&stack).row_text(0), "Internet TCP/IP Connection");
    assert_eq!(
        (
            layouts.records()[1].item_count,
            layouts.records()[1].visible_rows
        ),
        (1, 1)
    );
    let request = activate(&mut stack, LIST_ACTION, [0, 0]);
    list_action(&mut stack, request, &templates, &mut fake).unwrap();
    let MenuOwner::Network(prompt) = stack.current_owner_mut().unwrap() else {
        panic!()
    };
    for key in "127.0.0.19".chars() {
        prompt.type_key(TypedKey::Char(key));
    }
    prompt.type_key(TypedKey::Backspace);
    assert_eq!(
        prompt.row_text(0),
        "Host address (blank: local network): 127.0.0.1_"
    );
    fake.sessions = vec![session("ALPHA")];
    let request = activate(&mut stack, LIST_ACTION, [0, 0]);
    list_action(&mut stack, request, &templates, &mut fake).unwrap();
    assert!(fake.calls[0].starts_with("connect SearchTarget { broadcast: false"));
    assert_eq!(fake.calls[1], "enumerate");
    let list = network(&stack);
    assert_eq!(list.row_text(0), "ALPHA");
    assert_eq!(list.row_text(1), "No more");
    assert_eq!(
        list.base().data().source(),
        MenuSource::Derived {
            constructor: LIST_CONSTRUCTOR,
            vtable: 0x0052_6208
        }
    );
    assert_eq!(list.base().data().records().items.len(), 3);
}

/// 0045FDE0: a second's ticks search again; a changed count rebuilds the
/// rows and the shared layout's counts (four rows show at most).
#[test]
fn the_session_list_searches_each_second_and_rebuilds_on_a_new_count() {
    let (mut fake, templates) = fake();
    let layouts = fake.menu_data(HOST_MENU).unwrap().layouts();
    let mut stack = stack();
    stack.push(NetworkMenu::sessions(&templates, layouts.clone(), vec![]));
    fake.sessions = (0..5).map(|index| session(&format!("S{index}"))).collect();
    let MenuOwner::Network(list) = stack.current_owner_mut().unwrap() else {
        panic!()
    };
    list.tick(600_000, &templates, &mut fake).unwrap();
    assert!(fake.calls.is_empty());
    list.tick(600_000, &templates, &mut fake).unwrap();
    assert_eq!(fake.calls, ["enumerate"]);
    assert_eq!(list.base().data().records().items.len(), 7);
    assert_eq!(
        (
            layouts.records()[1].item_count,
            layouts.records()[1].visible_rows
        ),
        (6, 4)
    );
    assert_eq!(list.row_text(4), "S4");
}

/// The host row opens 00538650 with its own row counts and the arena's mode
/// rows; an arena change lays the rows out again (00538668), and Start
/// hosts, records the mode and asks for phase 9.
#[test]
fn hosting_sets_up_the_arena_and_starts_the_session() {
    let (mut fake, templates) = fake();
    let layouts = fake.menu_data(HOST_MENU).unwrap().layouts();
    let mut stack = stack();
    stack.push(NetworkMenu::sessions(&templates, layouts.clone(), vec![]));
    fake.settings.insert(SETTING_ARENA, 7);
    let request = activate(&mut stack, LIST_ACTION, [u32::MAX, 0]);
    list_action(&mut stack, request, &templates, &mut fake).unwrap();
    assert_eq!(
        stack.current().unwrap().data().source().authored_address(),
        Some(HOST_MENU)
    );
    assert_eq!(
        (
            layouts.records()[1].item_count,
            layouts.records()[1].visible_rows
        ),
        (9, 10)
    );
    let flags = |stack: &MenuStack| {
        let items = &stack.current().unwrap().data().records().items;
        [items[5].flags, items[6].flags, items[7].flags]
    };
    // Four Play Heaven, mode 0: grenades and score.
    assert_eq!(flags(&stack), [4, 0, 4]);
    let mut request = activate(&mut stack, ARENA_ACTION, [0xa, SETTINGS + SETTING_ARENA]);
    request.command = MenuCommand::AdjustNext;
    host_menu_action(&mut stack, request, &mut fake).unwrap();
    // Jet Race 1, mode 3: catchup, and grenades set.
    assert_eq!(fake.settings[&SETTING_ARENA], 8);
    assert_eq!(
        stack.current().unwrap().data().source().authored_address(),
        Some(HOST_MENU_RELAID)
    );
    assert_eq!(flags(&stack), [0, 4, 0]);
    assert_eq!(fake.settings[&SETTING_GRENADES], 1);
    // Ten arenas without the cheat bit: 9 stops it.
    for _ in 0..3 {
        host_menu_action(&mut stack, request, &mut fake).unwrap();
    }
    assert_eq!(fake.settings[&SETTING_ARENA], 9);
    // The winning score steps by 1000 in mode 3, from 1000 to the row's max.
    fake.settings.insert(SETTING_WINNING_SCORE, 1000);
    let mut limit = activate(
        &mut stack,
        LIMIT_ACTION,
        [100_000, SETTINGS + SETTING_WINNING_SCORE],
    );
    limit.command = MenuCommand::AdjustPrevious;
    host_menu_action(&mut stack, limit, &mut fake).unwrap();
    assert_eq!(fake.settings[&SETTING_WINNING_SCORE], 1000);
    limit.command = MenuCommand::AdjustNext;
    host_menu_action(&mut stack, limit, &mut fake).unwrap();
    assert_eq!(fake.settings[&SETTING_WINNING_SCORE], 2000);
    // Left on Start Session starts it too: 0045F320 takes no command.
    let mut start = activate(&mut stack, START_ACTION, [0, 0]);
    start.command = MenuCommand::AdjustPrevious;
    fake.calls.clear();
    host_menu_action(&mut stack, start, &mut fake).unwrap();
    assert_eq!(fake.calls, ["host", "transition", "reset", "phase 9"]);
    assert_eq!(fake.settings[&SETTING_MODE], 3);
}

#[test]
fn a_host_without_a_slot_closes_the_session_and_pops() {
    let (mut fake, _) = fake();
    let mut stack = stack();
    stack.push(MenuState::new(
        fake.menu_data(HOST_MENU).unwrap(),
        MenuBasis::INITIAL,
    ));
    stack.push(MenuState::new(
        fake.menu_data(HOST_MENU).unwrap(),
        MenuBasis::INITIAL,
    ));
    fake.host_slot = false;
    let start = activate(&mut stack, START_ACTION, [0, 0]);
    host_menu_action(&mut stack, start, &mut fake).unwrap();
    assert!(fake.calls.contains(&"close".to_string()));
    assert_eq!(stack.len(), 1);
}

/// Joining opens the wait; it waits for a slot, resets the game's network
/// state, then waits for the host's world and asks for phase 9.
#[test]
fn a_join_waits_for_its_slot_then_the_hosts_world() {
    let (mut fake, templates) = fake();
    let layouts = fake.menu_data(HOST_MENU).unwrap().layouts();
    let mut stack = stack();
    stack.push(NetworkMenu::sessions(
        &templates,
        layouts,
        vec![session("ALPHA"), session("BETA")],
    ));
    let request = activate(&mut stack, LIST_ACTION, [1, 0]);
    list_action(&mut stack, request, &templates, &mut fake).unwrap();
    assert_eq!(fake.calls, ["join BETA", "transition"]);
    assert_eq!(
        stack.current().unwrap().data().source().authored_address(),
        Some(WAIT_MENU)
    );
    let MenuOwner::Network(wait) = stack.current_owner_mut().unwrap() else {
        panic!()
    };
    fake.calls.clear();
    wait.tick(16_000, &templates, &mut fake).unwrap();
    fake.slot = (2, 0);
    wait.tick(16_000, &templates, &mut fake).unwrap();
    wait.tick(16_000, &templates, &mut fake).unwrap();
    fake.slot = (2, 25);
    wait.tick(16_000, &templates, &mut fake).unwrap();
    assert_eq!(
        fake.calls,
        [
            "receive false",
            "receive false",
            "reset",
            "receive true",
            "receive true",
            "phase 9"
        ]
    );
}

/// 0045FBE0 and its siblings close the session when a menu is dropped
/// without moving forward; moving forward keeps it.
#[test]
fn backing_out_closes_the_session_and_moving_forward_keeps_it() {
    use std::sync::atomic::Ordering;
    let (mut fake, templates) = fake();
    let layouts = fake.menu_data(HOST_MENU).unwrap().layouts();
    let mut stack = stack();
    stack.push(
        NetworkMenu::connections(&templates, layouts, fake.connections().unwrap())
            .with_close(fake.close.clone()),
    );
    let request = activate(&mut stack, LIST_ACTION, [0, 0]);
    list_action(&mut stack, request, &templates, &mut fake).unwrap();
    assert!(network(&stack).is_address_prompt());
    assert!(!fake.close.load(Ordering::Relaxed));
    stack.pop(1);
    assert!(fake.close.load(Ordering::Relaxed));
}

/// Until the network game is ported, Start hosts, then closes the session
/// and stays in the menu.
#[test]
fn start_holds_until_the_network_game_is_ported() {
    let (mut fake, _) = fake();
    fake.ready = false;
    let mut stack = stack();
    stack.push(NetworkMenu::host(fake.menu_data(HOST_MENU).unwrap()));
    let start = activate(&mut stack, START_ACTION, [0, 0]);
    host_menu_action(&mut stack, start, &mut fake).unwrap();
    assert!(fake.calls.contains(&"close".to_string()));
    assert!(!fake.calls.iter().any(|call| call.starts_with("phase")));
    assert_eq!(stack.len(), 1);
}

#[test]
fn the_address_prompt_types_hosts_and_ports() {
    assert_eq!(typed_key("A", false), Some(TypedKey::Char('a')));
    assert_eq!(typed_key("A", true), Some(TypedKey::Char('A')));
    assert_eq!(typed_key("7", false), Some(TypedKey::Char('7')));
    assert_eq!(typed_key("Keypad 7", false), Some(TypedKey::Char('7')));
    assert_eq!(typed_key(".", false), Some(TypedKey::Char('.')));
    assert_eq!(typed_key("Keypad .", false), Some(TypedKey::Char('.')));
    assert_eq!(typed_key("-", false), Some(TypedKey::Char('-')));
    assert_eq!(typed_key(";", true), Some(TypedKey::Char(':')));
    assert_eq!(typed_key(";", false), None);
    assert_eq!(typed_key("Backspace", false), Some(TypedKey::Backspace));
    assert_eq!(typed_key("Return", false), None);
    assert_eq!(typed_key("Escape", false), None);
}

/// `0045EE20`'s previews at 1280x1024, against corners worked out from the
/// native arithmetic over retail's own sine table: ten arenas without the
/// cheat word's bit 7, the selected one drawn twice, each its arena's sprite
/// as a textured quad at depth 0x40 with packet word 4. The selected
/// Powerball spans W/16 each way about (232, 237), on the 45% arc at angle
/// 0x2000; Buggyball, on the 35% arc, and Capture the Banner span W/20. At
/// clock 8 the pulse peaks and the selected one widens by W/80 less one.
/// With the bit, Buggy Race comes eleventh.
#[test]
fn the_arena_previews_follow_0045ee20() {
    use crate::scene_sprite::DirectSpriteMode;
    let assets = FrontendAssets::open(&root()).unwrap();
    let sine = &assets.sine_table().values;
    let corners = |quad: &crate::scene_sprite::DirectSpriteQuad| quad.corners;
    let quads = arena_previews([1280, 1024], 0, 0, false, sine);
    assert_eq!(quads.len(), 11);
    assert_eq!(quads[0], quads[1]);
    assert_eq!(
        quads[1..]
            .iter()
            .map(|quad| quad.sprite_slot)
            .collect::<Vec<_>>(),
        ARENAS[..10]
            .iter()
            .map(|arena| arena.sprite)
            .collect::<Vec<_>>()
    );
    assert!(quads.iter().all(|quad| quad.vertex_depths == [0x40; 4]
        && quad.descriptor_word == 4
        && quad.mode == DirectSpriteMode::Textured));
    assert_eq!(
        corners(&quads[0]),
        [[152, 157], [312, 157], [312, 317], [152, 317]]
    );
    assert_eq!(
        corners(&quads[2]),
        [[312, 209], [440, 209], [440, 337], [312, 337]]
    );
    assert_eq!(
        corners(&quads[3]),
        [[314, 89], [442, 89], [442, 217], [314, 217]]
    );
    let pulse = arena_previews([1280, 1024], 0, 8, false, sine);
    assert_eq!(
        corners(&pulse[0]),
        [[137, 142], [327, 142], [327, 332], [137, 332]]
    );
    let all = arena_previews([1280, 1024], 3, 0, true, sine);
    assert_eq!(all.len(), 12);
    assert_eq!(all[3], all[4]);
    assert_eq!(all.last().unwrap().sprite_slot, 0x71d);
}
