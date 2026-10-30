use super::*;
use inf_formats::ByteSpan;

fn inputs(current_world_a0: u32) -> WorldMissionInputs {
    WorldMissionInputs {
        current_world_a0,
        dword_1c: 0x2800,
        byte_13: Some(1),
        record_0c: None,
    }
}

fn header(flags: u32, word_08: u32, word_0c: u16, word_16: u16) -> WorldResource {
    let mut words = [0; 14];
    words[1] = flags;
    words[2] = word_08;
    words[3] = u32::from(word_0c);
    words[5] = u32::from(word_16) << 16 | 4;
    WorldResource {
        local_index: 0,
        entry_pointer_span: ByteSpan::new(0, 4),
        allocation_offset: 0,
        header_span: ByteSpan::new(0, 0x3c),
        header_words_00_to_34: words,
        flags,
        placement_pointer_span: ByteSpan::new(0x3c, 0),
        placements: vec![],
    }
}

#[test]
fn offline_first_load_matches_the_recorded_mission_prelude() {
    // Recorded 0048DC30 entry: A0 = 1, +30 = 22, all per-world words zero.
    let mut mission = WorldMission::new(MissionSaveState::new_game(), inputs(1));
    mission.begin_world_load().unwrap();
    assert_eq!(mission.current_world_a0, 22);
    assert_eq!(mission.byte_a6, Some(0));
    assert_eq!(mission.dword_38, Some(0));
    assert_eq!(mission.save, MissionSaveState::new_game());
    assert_eq!(mission.current_world_completion().unwrap(), [0; 4]);
    assert_eq!(mission.inhibit_byte_08, 0); //0048D9F0, not allocation history.
}

#[test]
fn placement_callbacks_observe_the_retained_cleanup_inhibition_byte() {
    let mut mission = WorldMission::new(MissionSaveState::new_game(), inputs(22));
    assert_eq!(mission.inhibit_byte_08, 0);
    //00478CD0 writes1 before actor callbacks. Any nonzero byte inhibits;
    // neither mission/header setup nor statistics custody substitutes zero.
    for inhibit in [1, 255] {
        mission.inhibit_byte_08 = inhibit;
        assert!(!mission
            .record_placement_death(PlacementDeathRequest {
                index: 7,
                inhibit_byte_08: mission.inhibit_byte_08,
                force: 0,
            })
            .unwrap());
        assert_eq!(mission.current_world_completion().unwrap(), [0; 4]);
    }
    assert!(mission
        .record_placement_death(PlacementDeathRequest {
            index: 7,
            inhibit_byte_08: mission.inhibit_byte_08,
            force: 1,
        })
        .unwrap());
    mission.inhibit_byte_08 = 0; //00478CD0's reached normal-return writer.
    assert!(mission
        .record_placement_death(PlacementDeathRequest {
            index: 8,
            inhibit_byte_08: mission.inhibit_byte_08,
            force: 0,
        })
        .unwrap());
    assert_eq!(
        mission.current_world_completion().unwrap(),
        [0x180, 0, 0, 0]
    );
}

#[test]
fn previous_world_status_gains_bit_two_only_when_its_low_byte_has_bit_one() {
    let mut save = MissionSaveState::new_game();
    save.owner_3c[3] = 0x0000_0102;
    save.owner_3c[4] = 0x0000_0200;
    save.mission_selection = 4;
    let mut mission = WorldMission::new(save, inputs(3));
    mission.begin_world_load().unwrap();
    assert_eq!(mission.save.owner_3c[3], 0x0000_0106);
    assert_eq!(mission.current_world_a0, 4);
    // Bit 9 is outside the tested low byte, so world 4 is left unchanged.
    mission.save.mission_selection = 5;
    mission.begin_world_load().unwrap();
    assert_eq!(mission.save.owner_3c[4], 0x0000_0200);
}

#[test]
fn completion_reads_the_current_worlds_four_words_not_the_selection() {
    let mut save = MissionSaveState::new_game();
    save.owner_240[88..92].copy_from_slice(&[1, 2, 3, 4]);
    save.owner_240[4..8].copy_from_slice(&[9, 9, 9, 9]);
    save.mission_selection = 1;
    let mission = WorldMission::new(save, inputs(22));
    assert_eq!(mission.current_world_completion().unwrap(), [1, 2, 3, 4]);
}

#[test]
fn placement_death_writes_current_world_and_preserves_unrelated_bits() {
    let mut save = MissionSaveState::new_game();
    save.mission_selection = 18;
    save.owner_240[88] = 1;
    save.owner_240[72] = 0x1234;
    let mut mission = WorldMission::new(save, inputs(22));
    for index in [31, 32, 63, 64, 127] {
        assert!(mission
            .record_placement_death(PlacementDeathRequest {
                index,
                inhibit_byte_08: 0,
                force: 0,
            })
            .unwrap());
    }
    assert_eq!(
        mission.current_world_completion().unwrap(),
        [0x8000_0001, 0x8000_0001, 1, 0x8000_0000]
    );
    assert_eq!(mission.save.owner_240[72], 0x1234);
    assert_eq!(mission.save.mission_selection, 18);
}

#[test]
fn placement_death_preserves_native_signed_index_and_inhibition_gates() {
    let mut mission = WorldMission::new(MissionSaveState::new_game(), inputs(22));
    for index in [i32::MIN, -33, -32, -1, 128, 799, i32::MAX] {
        assert!(!mission
            .record_placement_death(PlacementDeathRequest {
                index,
                inhibit_byte_08: 0,
                force: 0,
            })
            .unwrap());
    }
    for index in [-31, -2] {
        assert!(mission
            .record_placement_death(PlacementDeathRequest {
                index,
                inhibit_byte_08: 0,
                force: 0,
            })
            .unwrap());
    }
    assert_eq!(mission.current_world_completion().unwrap()[0], 0x4000_0002);
    assert!(!mission
        .record_placement_death(PlacementDeathRequest {
            index: 0,
            inhibit_byte_08: 255,
            force: 0,
        })
        .unwrap());
    assert!(mission
        .record_placement_death(PlacementDeathRequest {
            index: 0,
            inhibit_byte_08: 255,
            force: 128,
        })
        .unwrap());
    assert_eq!(mission.current_world_completion().unwrap()[0], 0x4000_0003);
}

#[test]
fn placement_death_checks_world_bounds_only_after_native_no_write_gates() {
    let mut mission = WorldMission::new(MissionSaveState::new_game(), inputs(25));
    let before = mission.clone();
    assert!(!mission
        .record_placement_death(PlacementDeathRequest {
            index: -1,
            inhibit_byte_08: 0,
            force: 0,
        })
        .unwrap());
    assert!(!mission
        .record_placement_death(PlacementDeathRequest {
            index: 10,
            inhibit_byte_08: 1,
            force: 0,
        })
        .unwrap());
    assert_eq!(
        mission.record_placement_death(PlacementDeathRequest {
            index: 10,
            inhibit_byte_08: 0,
            force: 0,
        }),
        Err(WorldMissionError::ArenaCompletion(25))
    );
    assert_eq!(mission, before);
}

#[test]
fn out_of_range_worlds_fail_before_any_write() {
    let mut mission = WorldMission::new(MissionSaveState::new_game(), inputs(ARENA_WORLDS.end));
    let before = mission.clone();
    assert_eq!(
        mission.begin_world_load(),
        Err(WorldMissionError::WorldOutOfRange(34))
    );
    assert_eq!(mission, before);
    assert_eq!(
        mission.current_world_completion(),
        Err(WorldMissionError::WorldOutOfRange(34))
    );
}

#[test]
fn leaving_an_arena_world_marks_what_lies_past_the_statuses() {
    let mut save = MissionSaveState::new_game();
    save.owner_a4 = 2;
    save.owner_b0[0] = 2;
    save.owner_b0[4] = 3;
    save.mission_selection = 5;
    let load = |world: u32| {
        let mut mission = WorldMission::new(save.clone(), inputs(world));
        mission.begin_world_load().unwrap();
        assert_eq!(mission.current_world_a0, 5);
        mission.save
    };
    // W+A0 (25 itself) and the override words (27, 28) change nothing.
    for world in [25, 27, 28] {
        let after = load(world);
        assert_eq!(
            (after.owner_a4, after.owner_b0, after.owner_3c),
            (2, save.owner_b0, save.owner_3c)
        );
    }
    assert_eq!(load(26).owner_a4, 6);
    assert_eq!(load(29).owner_b0[0], 6);
    assert_eq!(load(33).owner_b0[4], 7);
    assert_eq!(load(30).owner_b0, save.owner_b0);
    let arena = WorldMission::new(save.clone(), inputs(26));
    assert_eq!(
        arena.current_world_completion(),
        Err(WorldMissionError::ArenaCompletion(26))
    );
    assert_eq!(arena.status_byte(25), Ok(26));
    assert_eq!(arena.status_byte(26), Ok(2));
    assert_eq!(
        arena.status_byte(27),
        Err(WorldMissionError::ArenaStatus(27))
    );
    assert_eq!(arena.status_byte(33), Ok(3));
    assert_eq!(
        arena.status_byte(34),
        Err(WorldMissionError::WorldOutOfRange(34))
    );
}

#[test]
fn header_prelude_matches_the_recorded_first_world_values() {
    let mut mission = WorldMission::new(MissionSaveState::new_game(), inputs(1));
    mission.apply_world_header(&header(0x20, 0x500, 0, 0x180));
    assert_eq!(mission.bytes_14_16, Some([0; 3]));
    assert_eq!(mission.dword_24, Some(0));
    assert_eq!(mission.byte_13, Some(1));
    assert_eq!((mission.word_28, mission.dword_2c), (Some(0), Some(0)));
    assert_eq!(mission.dword_20, Some(0x500));
}

#[test]
fn header_prelude_sign_extends_clears_byte_13_and_clamps_with_signed_quarters() {
    let mut mission = WorldMission::new(MissionSaveState::new_game(), inputs(1));
    mission.dword_1c = 0x10;
    mission.apply_world_header(&header(0x1000, 0x500, 0xfff0, 1));
    assert_eq!(mission.dword_24, Some(0xffff_fff0));
    assert_eq!(mission.byte_13, Some(0));
    assert_eq!(mission.dword_20, Some(12));
    // A negative +1C truncates toward zero: -5 * 3 / 4 = -3, not -4.
    mission.dword_1c = (-5_i32) as u32;
    mission.apply_world_header(&header(0, 0x500, 0, 1));
    assert_eq!(mission.dword_20, Some((-3_i32) as u32));
    // Header +8 is compared as a signed DWORD.
    mission.dword_1c = 0x100;
    mission.apply_world_header(&header(0, 0x8000_0000, 0, 1));
    assert_eq!(mission.dword_20, Some(0x8000_0000));
}

#[test]
fn tail_divides_signed_and_clears_a_present_record() {
    let mut record = Box::new([0xa5; RECORD_0C_BYTES]);
    record[0] = 1;
    let mut mission = WorldMission::new(
        MissionSaveState::new_game(),
        WorldMissionInputs {
            record_0c: Some(record),
            ..inputs(1)
        },
    );
    mission
        .finish_world_load(&header(0x20, 0x500, 0, 0x180))
        .unwrap();
    assert_eq!(mission.word_10, Some(0xaa));
    assert_eq!(mission.byte_12, Some(1));
    assert_eq!(mission.record_0c.as_deref(), Some(&[0; RECORD_0C_BYTES]));
    mission.finish_world_load(&header(0, 0, 0, 0xffff)).unwrap();
    assert_eq!(mission.word_10, Some(0));
    mission.finish_world_load(&header(0, 0, 0, 0xfffd)).unwrap();
    assert_eq!(mission.word_10, Some((-21845_i32) as u16));
    let before = mission.clone();
    assert_eq!(
        mission.finish_world_load(&header(0, 0, 0, 0)),
        Err(WorldMissionError::ZeroTailDivisor)
    );
    assert_eq!(mission, before);
}

/// `0043B987..0043BA01` and `0048E32A..0048E51A`: a timed target's
/// knockout starts the countdown (seconds, four times over after 0xC) and
/// clears +17; one that falls while a countdown no longer than its own
/// seconds runs adds half its time; a longer one is replaced. The tick
/// counts down by the delta and ends at 0; with +17 set it holds.
#[test]
fn timed_targets_feed_the_countdown_the_tick_runs_down() {
    let mut mission = WorldMission::native_constructor();
    assert_eq!(
        (mission.byte_17, mission.byte_18, mission.dword_24),
        (Some(1), Some(0), Some(0))
    );
    mission.add_target_countdown(30, false).unwrap();
    assert_eq!(
        (mission.dword_24, mission.byte_17),
        (Some(30_000_000), Some(0))
    );
    assert_eq!(
        mission.tick_countdown(10_000_000, false).unwrap(),
        MissionCountdownTick::Counted
    );
    assert_eq!(mission.dword_24, Some(20_000_000));
    // 20 s left of a 30 s target: the next adds half of 30 s.
    mission.add_target_countdown(30, false).unwrap();
    assert_eq!(mission.dword_24, Some(35_000_000));
    // Longer than a 20 s target's own time: replaced by four times it.
    mission.add_target_countdown(20, true).unwrap();
    assert_eq!(mission.dword_24, Some(80_000_000));
    assert_eq!(
        mission.tick_countdown(80_000_000, false).unwrap(),
        MissionCountdownTick::Counted
    );
    assert_eq!(mission.dword_24, Some(0));
    assert_eq!(
        mission.tick_countdown(1, false).unwrap(),
        MissionCountdownTick::Idle
    );
    // +18 keeps the merge from running.
    mission.dword_24 = Some(5_000_000);
    mission.byte_18 = Some(1);
    mission.add_target_countdown(30, false).unwrap();
    assert_eq!(mission.dword_24, Some(30_000_000));
    // With +17 it counts down and, with a player unit, reports the
    // explosions under 5 s.
    mission.byte_17 = Some(1);
    mission.byte_18 = Some(0);
    assert_eq!(
        mission.tick_countdown(1, true).unwrap(),
        MissionCountdownTick::Counted
    );
    assert_eq!(
        mission.tick_countdown(25_000_000, true).unwrap(),
        MissionCountdownTick::Explosions
    );
    assert_eq!(mission.dword_24, Some(4_999_999));
    assert_eq!(
        mission.tick_countdown(1, false).unwrap(),
        MissionCountdownTick::Counted
    );
    // With a unit the end's first stage writes +24 = 0 and +18 and hands
    // the unit to its caller (0048E505); the caller arms the 3 s after.
    assert_eq!(
        mission.tick_countdown(5_000_000, true).unwrap(),
        MissionCountdownTick::UnitEnd
    );
    assert_eq!((mission.dword_24, mission.byte_18), (Some(0), Some(1)));
    mission.arm_doom_last_seconds();
    assert_eq!(mission.dword_24, Some(3_000_000));
    mission.current_world_a0 = 6;
    assert_eq!(
        mission.tick_countdown(3_000_000, true).unwrap(),
        MissionCountdownTick::MissionFailed { world: 6 }
    );
    assert_eq!(
        (mission.dword_24, mission.byte_18, mission.bytes_14_16),
        (Some(0), Some(0), Some([0, 1, 0]))
    );
    // Without a unit the end takes its two stages: 3 s more, then the world
    // ends into itself with +15 set ("Mission Failed").
    mission.bytes_14_16 = Some([0, 0, 0]);
    mission.dword_24 = Some(4_999_998);
    assert_eq!(
        mission.tick_countdown(5_000_000, false).unwrap(),
        MissionCountdownTick::LastSeconds
    );
    assert_eq!(
        (mission.dword_24, mission.byte_18),
        (Some(3_000_000), Some(1))
    );
    assert_eq!(
        mission.tick_countdown(3_000_000, false).unwrap(),
        MissionCountdownTick::MissionFailed { world: 6 }
    );
    assert_eq!(
        (mission.dword_24, mission.byte_18, mission.bytes_14_16),
        (Some(0), Some(0), Some([0, 1, 0]))
    );
}

/// `0046A8C6..0046A8D7` at the loss limit, then the mission tick: no unit
/// remains, so the first update takes the 3 s stage and the world ends when
/// it runs out, into the current world with +15 set.
#[test]
fn the_loss_limit_ends_the_world_after_the_last_three_seconds() {
    let mut mission = WorldMission::native_constructor();
    mission.current_world_a0 = 3;
    mission.byte_17 = Some(0);
    mission.start_doom_countdown(1);
    assert_eq!((mission.dword_24, mission.byte_17), (Some(1), Some(1)));
    assert_eq!(
        mission.tick_countdown(125_000, false).unwrap(),
        MissionCountdownTick::LastSeconds
    );
    for _ in 0..23 {
        assert_eq!(
            mission.tick_countdown(125_000, false).unwrap(),
            MissionCountdownTick::Counted
        );
    }
    assert_eq!(mission.dword_24, Some(125_000));
    assert_eq!(
        mission.tick_countdown(125_000, false).unwrap(),
        MissionCountdownTick::MissionFailed { world: 3 }
    );
    assert_eq!(mission.bytes_14_16, Some([0, 1, 0]));
}

/// `0048E32A..0048E521` against the 40 countdowns of
/// `world_interactions_native.json`, run on the original instructions with
/// no player unit: +24, +18 and +15 after each delta and the world's end
/// call, including both stages of the doom's end.
#[test]
fn the_countdown_matches_the_native_receipt() {
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap();
    let cases = fixture["mission_countdown"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    let (mut last_seconds, mut failed) = (0, 0);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let number = |key: &str| case[key].as_u64().unwrap();
        let mut mission = WorldMission::native_constructor();
        mission.current_world_a0 = 5;
        mission.byte_17 = Some(number("doom") as u8);
        mission.byte_18 = Some(number("second") as u8);
        mission.dword_24 = Some(number("countdown") as u32);
        let (countdown, delta) = (number("countdown") as u32, number("delta") as u32);
        let tick = mission.tick_countdown(delta, false).unwrap();
        assert_eq!(
            u64::from(mission.dword_24.unwrap()),
            number("countdown_after"),
            "{name}"
        );
        assert_eq!(
            u64::from(mission.byte_18.unwrap()),
            number("second_after"),
            "{name}"
        );
        assert_eq!(
            u64::from(mission.bytes_14_16.unwrap()[1]),
            number("byte_15_after"),
            "{name}"
        );
        let calls = case["calls"].as_array().unwrap();
        let ending = countdown != 0 && countdown as i32 <= delta as i32;
        let expected = if countdown == 0 {
            MissionCountdownTick::Idle
        } else if number("doom") == 0 || !ending {
            MissionCountdownTick::Counted
        } else if number("second") != 0 {
            failed += 1;
            MissionCountdownTick::MissionFailed { world: 5 }
        } else {
            last_seconds += 1;
            MissionCountdownTick::LastSeconds
        };
        assert_eq!(tick, expected, "{name}");
        let ends_world = matches!(tick, MissionCountdownTick::MissionFailed { .. });
        assert_eq!(
            calls
                .iter()
                .map(|call| call.as_str().unwrap())
                .collect::<Vec<_>>(),
            if ends_world {
                vec!["world_end"]
            } else {
                vec![]
            },
            "{name}"
        );
    }
    assert_eq!((last_seconds, failed), (5, 1));
}
