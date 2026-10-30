//! 00487410/0048EBA0 offline preparation over synthetic grant tables; the
//! installed executable's exact mask stays pinned by the grant-table corpus.

use super::*;
use inf_formats::{
    ByteSpan, GrantRecord, GrantTable, GRANT_FIRST_ID, GRANT_RECORD_BYTES, GRANT_RECORD_COUNT,
};

fn empty_table() -> GrantTable {
    GrantTable {
        virtual_address: 0x53_B088,
        records: (0..GRANT_RECORD_COUNT)
            .map(|index| GrantRecord {
                id: GRANT_FIRST_ID + index as u32,
                file_span: ByteSpan::new(0, GRANT_RECORD_BYTES),
                raw: [0; GRANT_RECORD_BYTES],
                dependencies: [0; 3],
            })
            .collect(),
    }
}

#[test]
fn offline_mission_defaults_are_selection_22() {
    let game = offline_game(&empty_table()).unwrap();
    assert_eq!(game.mission.mission_selection, OFFLINE_MISSION_SELECTION);
    assert_eq!(game.mission.elapsed_seconds, 0);
    assert!(game.mission.owner_b0.iter().all(|word| *word == 0));
    assert!(game.mission.owner_240.iter().all(|word| *word == 0));
    assert!(game.mission.owner_3c.iter().all(|word| *word == 0));
    assert_eq!(game.mission.owner_a4, 0);
}

#[test]
fn offline_mission_override_selects_another_world() {
    // Debug-only `--mission` path: only the selection differs.
    let game = offline_game_with_selection(&empty_table(), Some(18)).unwrap();
    assert_eq!(game.mission.mission_selection, 18);
    assert_eq!(game.mission.elapsed_seconds, 0);
    let game = offline_game_with_selection(&empty_table(), None).unwrap();
    assert_eq!(game.mission.mission_selection, OFFLINE_MISSION_SELECTION);
}

#[test]
fn offline_player_owns_the_documented_prefix_and_closure() {
    let game = offline_game(&empty_table()).unwrap();
    assert_eq!(game.player.owner_144, 1);
    assert_eq!(game.player.owner_14c, 8);
    assert_eq!(game.player.owner_150[0], 0xFFFF_FFFF);
    assert_eq!(&game.player.owner_150[1..], &[0; 15]);
    // SAVE_FORMAT.md: the installed table's rescan adds no IDs, so the twelve
    // construction grants close to these exact words even for the empty table
    // (only the twelve IDs set bits).
    assert_eq!(game.player.owner_a0, [0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    assert_eq!(game.player.owner_100, [0; 6]);
}

#[test]
fn dependency_grants_fold_into_the_player_mask() {
    // One high record depends on ID 0x48, which is among the initial grants
    // (bit 8 of word 2): the rescan must set its own bit too.
    let mut table = empty_table();
    table.records[0].dependencies[0] = 0x48;
    let game = offline_game(&table).unwrap();
    assert_eq!(
        game.player.owner_a0,
        [0x2472, 0, 0x4100, 0x1440_0000, 8 | (1 << 21), 0]
    );
}

#[test]
fn out_of_mask_dependencies_stay_explicit_errors() {
    let mut table = empty_table();
    table.records[0].dependencies[0] = u16::MAX;
    assert!(matches!(
        offline_game(&table),
        Err(OfflineGameError::Grant(GrantClosureError::Id(65535)))
    ));
}
