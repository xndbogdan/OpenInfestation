use super::*;

fn grant_table() -> GrantTable {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    inf_formats::parse_grant_table(executable).unwrap()
}

/// An empty state is a new game's player at Level 1's first start; its
/// player keeps the fresh prefix.
#[test]
fn the_default_start_is_level_1_with_a_fresh_player() {
    let start: HarnessStart = serde_json::from_str("{}").unwrap();
    let mission = start.mission();
    assert_eq!(mission.mission_selection, 18);
    assert_eq!(mission.owner_a4, 0);
    assert_eq!(mission, {
        let mut fresh = MissionSaveState::new_game();
        fresh.mission_selection = 18;
        fresh
    });
    let grants = grant_table();
    let fresh = PlayerSaveState::new_player([1, 2, 3, 4, 5, 6]);
    let mut player = fresh.clone();
    start.apply_player(&mut player, &grants).unwrap();
    assert_eq!(player, fresh);
    assert_eq!(start.spawn(), None);
    assert_eq!(HarnessStart::load(None).unwrap(), start);
}

/// Each field lands in its saved slot (SAVE_FORMAT.md's tables).
#[test]
fn each_field_writes_its_saved_slot() {
    let start: HarnessStart = serde_json::from_str(
        r#"{
            "world": 5, "start": 2, "position": [24000, 31000], "heading": -90,
            "elapsed_seconds": 600, "world_status": {"18": 6, "22": 4},
            "upgrades": [3, 50, 191], "hints_seen": [0, 33, 72],
            "tokens": 12, "crystals": [30, 20, 10], "turbo": 3,
            "energy": [1, 2, 3, 4, 5, -1, 7], "acams_lost": 2, "rank": 9,
            "form": 4, "primary": 3, "secondary": 9,
            "ammo": {"8": 20, "15": -1}, "hold": [12, 0, 12]
        }"#,
    )
    .unwrap();
    start.validate().unwrap();
    let mission = start.mission();
    assert_eq!(
        (
            mission.mission_selection,
            mission.owner_a4,
            mission.elapsed_seconds
        ),
        (5, 2, 600)
    );
    assert_eq!((mission.owner_3c[18], mission.owner_3c[22]), (6, 4));
    let grants = grant_table();
    let mut player = PlayerSaveState::new_player([0; 6]);
    player.owner_13c = [0, 0xab00_0000];
    start.apply_player(&mut player, &grants).unwrap();
    assert_eq!(player.owner_100, [1 << 3, 1 << 18, 0, 0, 0, 1 << 31]);
    assert_eq!(player.owner_130, [1, 1 << 1, 1 << 8]);
    assert_eq!(player.owner_118, 12);
    assert_eq!(player.owner_120, [30, 20, 10, 3]);
    assert_eq!(
        player.owner_13c,
        [0x0403_0201, 0xab07_ff05],
        "+143 keeps its byte"
    );
    assert_eq!((player.owner_11c, player.owner_11e), (2, 9));
    assert_eq!(
        (player.owner_144, player.owner_148, player.owner_14c),
        (4, 3, 9)
    );
    assert_eq!(
        player.owner_150[0], 0xffff_ffff,
        "unlisted weapons keep theirs"
    );
    assert_eq!(
        (player.owner_150[8], player.owner_150[15]),
        (20, 0xffff_ffff)
    );
    assert_eq!(player.owner_190, [12, 0, 12]);
    assert_eq!(
        start.spawn(),
        Some(HarnessSpawn {
            position: Some([24000, 31000]),
            heading: Some(-90),
        })
    );
}

/// "known" closes over the grant table as a granted ID does (`004750C0`);
/// "all" and "none" fill or clear a list.
#[test]
fn known_grants_close_over_the_table_and_words_fill_lists() {
    let grants = grant_table();
    let mut player = PlayerSaveState::new_player([0; 6]);
    let all: HarnessStart =
        serde_json::from_str(r#"{"known": "all", "upgrades": "all", "hints_seen": "all"}"#)
            .unwrap();
    all.apply_player(&mut player, &grants).unwrap();
    // ID 0 is always owned, so the closure never sets its bit.
    assert_eq!(
        player.owner_a0,
        [!1, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX]
    );
    assert_eq!(player.owner_100, [u32::MAX; 6]);
    assert_eq!(player.owner_130, [u32::MAX, u32::MAX, 0x1ff]);
    let none: HarnessStart =
        serde_json::from_str(r#"{"known": "none", "hints_seen": "none"}"#).unwrap();
    none.apply_player(&mut player, &grants).unwrap();
    assert_eq!(player.owner_a0, [0; 6]);
    assert_eq!(player.owner_130, [0; 3]);
    // One ID brings what the table makes it imply.
    let closure = inf_formats::grant_closure(&grants, &[0x32]).unwrap();
    let one: HarnessStart = serde_json::from_str(r#"{"known": [50]}"#).unwrap();
    one.apply_player(&mut player, &grants).unwrap();
    assert_eq!(player.owner_a0, closure);
}

/// Out-of-range values are refused before anything loads.
#[test]
fn out_of_range_states_are_refused() {
    let refused = |json: &str| {
        serde_json::from_str::<HarnessStart>(json)
            .map_err(|error| error.to_string())
            .and_then(|start| start.validate().map_err(|error| error.to_string()))
            .unwrap_err()
    };
    assert!(refused(r#"{"world": 25}"#).contains("world 25"));
    assert!(refused(r#"{"world_status": {"30": 1}}"#).contains("world 30"));
    assert!(refused(r#"{"upgrades": [192]}"#).contains("ID 192"));
    assert!(refused(r#"{"hints_seen": [73]}"#).contains("ID 73"));
    assert!(refused(r#"{"known": "some"}"#).contains("\"some\""));
    assert!(refused(r#"{"ammo": {"16": 1}}"#).contains("weapon 16"));
    assert!(refused(r#"{"wolrd": 5}"#).contains("unknown field"));
}

/// Capture nodes taken at the first tick parse, and are no part of a save.
#[test]
fn captured_nodes_are_a_start_alone() {
    let start: HarnessStart =
        serde_json::from_str(r#"{"world": 19, "captured_nodes": [0, 1, 2, 3, 4]}"#).unwrap();
    start.validate().unwrap();
    assert_eq!(start.captured_nodes, Some(vec![0, 1, 2, 3, 4]));
    let refused = start.save_record(b"Lab", &grant_table()).unwrap_err();
    assert!(refused.to_string().contains("captured_nodes"));
}

/// The lab's saved-game entry: the record reads back with the state's
/// mission and player fields, and a position cannot be saved.
#[test]
fn a_start_state_saves_as_a_profile_the_loader_reads_back() {
    let grants = grant_table();
    let start: HarnessStart = serde_json::from_str(
        r#"{"world": 5, "start": 1, "tokens": 12, "crystals": [3, 4, 5], "form": 2}"#,
    )
    .unwrap();
    let record = start.save_record(b"Lab", &grants).unwrap();
    let bytes = inf_formats::encode_save_records(&[record]).unwrap();
    let archive = inf_formats::SaveArchive::parse(&bytes).unwrap();
    let saved = &archive.records()[0];
    assert_eq!(saved.name(), b"Lab");
    assert_eq!(saved.mission(), start.mission());
    let mut player = crate::offline_prepare::offline_game_with_selection(&grants, Some(5))
        .unwrap()
        .player;
    start.apply_player(&mut player, &grants).unwrap();
    assert_eq!(saved.player(), player);
    assert_eq!(saved.player().owner_118, 12);
    assert!(saved.tail().iter().all(|&byte| byte == 0));

    let placed: HarnessStart = serde_json::from_str(r#"{"position": [24000, 31000]}"#).unwrap();
    assert!(placed
        .save_record(b"Lab", &grants)
        .unwrap_err()
        .to_string()
        .contains("not part of a saved game"));
}
