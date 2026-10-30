use super::*;
use crate::{MissionSaveState, PlayerSaveState, SaveRecord, SAVE_PAYLOAD_BYTES};

#[test]
fn bit_sets_expand_into_their_indices() {
    assert_eq!(bits(&[0b101, 0, 1 << 31]), [0, 2, 95]);
    assert!(bits(&[0; 4]).is_empty());
}

#[test]
fn worlds_name_their_status_bits_and_masks() {
    let mut mission = MissionSaveState::new_game();
    mission.owner_3c[2] = world_status::LEFT_BY_GATE | world_status::COMPLETED;
    mission.owner_b0[2 * 4] = 1 << 5;
    mission.owner_240[2 * 4 + 1] = 1;
    mission.owner_3c[7] = world_status::STAGE_ONE | world_status::ALREADY_COMPLETED;
    mission.owner_a4 = 3;
    let report = MissionReport::new(&mission);
    assert_eq!(report.next_selection, 22);
    assert_eq!(report.start_point, 3);
    assert_eq!(report.worlds.len(), 2);
    let world = &report.worlds[0];
    assert_eq!(world.world, 2);
    assert!(world.left_by_gate && world.completed);
    assert!(!world.already_completed && !world.stage_one_complete);
    assert_eq!(world.collected_pickups, [5]);
    assert_eq!(world.finished_placements, [32]);
    let other = &report.worlds[1];
    assert_eq!(other.world, 7);
    assert!(other.already_completed && other.stage_one_complete && !other.completed);
}

#[test]
fn the_player_report_follows_the_saved_order() {
    let mut player = PlayerSaveState::new_player([0x12, 0, 0, 0, 0, 0]);
    player.owner_100[0] = 1 << 2;
    player.owner_118 = 71;
    player.owner_11c = 1;
    player.owner_11e = 2;
    player.owner_120 = [4, 30, 0, 3];
    player.owner_130 = [0, 1, 0];
    player.owner_13c = [
        u32::from_le_bytes([10, 20, 30, 40]),
        u32::from_le_bytes([50, 60, 0xff, 9]),
    ];
    player.owner_190 = [2, 0, 0];
    let report = PlayerReport::new(&player);
    assert_eq!(report.upgrades, [2]);
    assert_eq!(report.tokens, 71);
    assert_eq!((report.acams_lost, report.rank), (1, 2));
    assert_eq!(report.crystals, [4, 30, 0]);
    assert_eq!(report.turbo_charges, 3);
    assert_eq!(report.hints_shown, [32]);
    assert_eq!(report.energy_levels, [10, 20, 30, 40, 50, 60, -1]);
    assert_eq!(report.byte_143, 9);
    assert_eq!(report.vehicle_mode, 1);
    assert_eq!((report.primary_weapon, report.secondary_weapon), (0, 8));
    assert_eq!(report.weapon_counts[0], -1);
    assert_eq!(report.hold_items, [2, 0, 0]);
    assert_eq!(report.known, [1, 4]);
}

#[test]
fn a_profile_reports_its_name_and_tail() {
    let mission = MissionSaveState::new_game();
    let player = PlayerSaveState::new_player([0; 6]);
    let name = b"Jameson";
    let tail_len =
        SAVE_PAYLOAD_BYTES - name.len() - 1 - crate::MISSION_SAVE_BYTES - crate::PLAYER_SAVE_BYTES;
    let record = SaveRecord::from_state(name, &mission, &player, &vec![0x0f; tail_len]).unwrap();
    let report = SaveProfileReport::new(&record);
    assert_eq!(report.name, "Jameson");
    assert_eq!(report.tail, vec![0x0f; tail_len]);
    assert!(report.mission.worlds.is_empty());
}
