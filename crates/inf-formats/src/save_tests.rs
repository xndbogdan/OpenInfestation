use super::*;

fn patterned_record(name: &[u8]) -> SaveRecord {
    let mut payload = std::array::from_fn(|index| (index as u8).wrapping_mul(37));
    payload[..name.len()].copy_from_slice(name);
    payload[name.len()] = 0;
    SaveRecord::from_payload(payload).unwrap()
}

#[test]
fn crc_uses_native_initial_zero_and_no_final_xor() {
    assert_eq!(save_crc32(b""), 0);
    assert_eq!(save_crc32(&[0; 124]), 0);
    assert_eq!(save_crc32(b"123456789"), 0x2dfd_2d88);
}

#[test]
fn serializer_offsets_are_unaligned_after_variable_name() {
    let record = patterned_record(b"AB");
    let bytes = record.payload();
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let half = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let mission = record.mission();
    assert_eq!(mission.mission_selection, word(3));
    assert_eq!(mission.elapsed_seconds, word(7));
    assert_eq!(mission.owner_b0[99], word(407));
    assert_eq!(mission.owner_240[0], word(411));
    assert_eq!(mission.owner_3c[24], word(907));
    assert_eq!(mission.owner_a4, half(911));
    let player = record.player();
    let start = 913;
    assert_eq!(player.owner_100[5], word(start + 20));
    assert_eq!(player.owner_118, word(start + 24));
    assert_eq!(player.owner_11c, half(start + 28));
    assert_eq!(player.owner_11e, half(start + 30));
    assert_eq!(player.owner_120[3], word(start + 44));
    assert_eq!(player.owner_144, word(start + 48));
    assert_eq!(player.owner_148, word(start + 52));
    assert_eq!(player.owner_14c, word(start + 56));
    assert_eq!(player.owner_150[15], word(start + 120));
    assert_eq!(player.owner_13c[1], word(start + 128));
    assert_eq!(player.owner_190[2], half(start + 136));
    assert_eq!(player.owner_130[2], word(start + 146));
    assert_eq!(player.owner_a0[0], word(start + 150));
    assert_eq!(player.owner_a0[5], word(start + 170));
    assert_eq!(record.tail(), &bytes[1087..]);
    assert_eq!(
        SaveRecord::from_state(b"AB", &mission, &player, record.tail()).unwrap(),
        record,
    );
}

#[test]
fn native_write_emits_nine_matching_pairs_and_retains_all_payload_bytes() {
    let records = [patterned_record(b"one"), patterned_record(b"second")];
    let bytes = encode_save_records(&records).unwrap();
    assert_eq!(bytes.len(), SAVE_RECORD_BYTES * 2);
    for pair in bytes.chunks_exact(256) {
        assert_eq!(&pair[..128], &pair[128..]);
    }
    let parsed = SaveArchive::parse(&bytes).unwrap();
    assert_eq!(parsed.records(), records);
    assert!(parsed
        .block_sources()
        .iter()
        .flatten()
        .all(|v| *v == SaveBlockSource::Primary));
}

#[test]
fn primary_success_skips_backup_even_when_backup_differs_or_is_corrupt() {
    let record = patterned_record(b"primary");
    let mut bytes = encode_save_records(std::slice::from_ref(&record)).unwrap();
    bytes[128 + 17] ^= 0x80;
    // A different but valid backup is also ignored.
    bytes[384 + 18] ^= 0x40;
    let crc = save_crc32(&bytes[384..508]);
    bytes[508..512].copy_from_slice(&crc.to_le_bytes());
    let archive = SaveArchive::parse(&bytes).unwrap();
    assert_eq!(archive.records(), &[record]);
    assert_eq!(archive.block_sources()[0], [SaveBlockSource::Primary; 9]);
}

#[test]
fn bad_primary_recovers_backup_and_both_bad_stop_at_completed_prefix() {
    let first = patterned_record(b"first");
    let second = patterned_record(b"second");
    let mut bytes = encode_save_records(&[first.clone(), second.clone()]).unwrap();
    let offset = SAVE_RECORD_BYTES + 4 * 256;
    bytes[offset + 12] ^= 1;
    let archive = SaveArchive::parse(&bytes).unwrap();
    assert_eq!(archive.records(), &[first.clone(), second]);
    assert!(matches!(
        archive.block_sources()[1][4],
        SaveBlockSource::Backup { .. }
    ));
    bytes[offset + 128 + 12] ^= 2;
    let scan = SaveArchive::scan_directory(&bytes);
    assert_eq!(scan.records, vec![first]);
    assert!(matches!(scan.end, SaveDirectoryEnd::Unreadable {
        record_index: 1, block_index: 4, copy: None,
        error: SaveError::CorruptPair { offset: found, .. },
    } if found == offset));
    assert!(matches!(
        SaveArchive::parse(&bytes),
        Err(SaveError::CorruptPair { .. })
    ));
}

#[test]
fn truncation_is_not_crc_recovery_and_scan_allows_native_skipped_final_backup() {
    let record = patterned_record(b"one");
    let bytes = encode_save_records(std::slice::from_ref(&record)).unwrap();
    for length in [1, 123, 124, 127, 129, 255, 256, 2047, 2048, 2175] {
        let scan = SaveArchive::scan_directory(&bytes[..length]);
        assert!(scan.records.is_empty(), "length {length}");
        assert!(
            matches!(
                scan.end,
                SaveDirectoryEnd::Unreadable {
                    copy: Some(SaveCopy::Primary),
                    error: SaveError::TruncatedCopy { .. },
                    ..
                }
            ),
            "length {length}: {:?}",
            scan.end
        );
    }
    for length in [2176, 2200, 2303] {
        let scan = SaveArchive::scan_directory(&bytes[..length]);
        assert_eq!(scan.records, vec![record.clone()]);
        assert_eq!(scan.end, SaveDirectoryEnd::EndOfFile);
        assert!(matches!(
            SaveArchive::parse(&bytes[..length]),
            Err(SaveError::FileLength { .. })
        ));
    }
    let mut bad = bytes[..2176].to_vec();
    bad[2048 + 12] ^= 1;
    assert!(matches!(
        SaveArchive::scan_directory(&bad).end,
        SaveDirectoryEnd::Unreadable {
            block_index: 8,
            copy: Some(SaveCopy::Backup),
            ..
        }
    ));
}

#[test]
fn empty_first_block_terminates_before_unread_remaining_blocks() {
    let first = patterned_record(b"one");
    let mut bytes = encode_save_records(std::slice::from_ref(&first)).unwrap();
    bytes.extend_from_slice(&[0; 128]); // Valid zero payload/CRC, missing backup/body.
    let scan = SaveArchive::scan_directory(&bytes);
    assert_eq!(scan.records, vec![first]);
    assert_eq!(scan.end, SaveDirectoryEnd::EmptyName { record_index: 1 });
    bytes.resize(SAVE_RECORD_BYTES * 2, 0x5a);
    assert_eq!(SaveArchive::parse(&bytes), Err(SaveError::EmptyName));
}

#[test]
fn capacity_and_bounded_byte_names_contain_native_overreads() {
    let mut payload = [b'x'; SAVE_PAYLOAD_BYTES];
    assert_eq!(
        SaveRecord::from_payload(payload),
        Err(SaveError::NameTerminator)
    );
    payload[31] = 0;
    let record = SaveRecord::from_payload(payload).unwrap();
    assert_eq!(record.name().len(), 31);
    assert_eq!(record.tail().len(), 0);
    assert_eq!(record.renamed_copy(b""), Err(SaveError::EmptyName));
    assert_eq!(
        record.renamed_copy(b"x\0y"),
        Err(SaveError::NameContainsNul)
    );
    assert!(matches!(
        record.renamed_copy(&[b'x'; 32]),
        Err(SaveError::NameLength { .. })
    ));
    assert_eq!(record.renamed_copy(&[0xff]).unwrap().name(), &[0xff]);
    let records = vec![record; 14];
    let mut bytes = encode_save_records(&records).unwrap();
    assert_eq!(SaveArchive::parse(&bytes).unwrap().records().len(), 14);
    bytes.push(1);
    let scan = SaveArchive::scan_directory(&bytes);
    assert_eq!(scan.records.len(), 14);
    assert_eq!(scan.end, SaveDirectoryEnd::CapacityExceeded);
    assert_eq!(SaveArchive::parse(&bytes), Err(SaveError::Capacity));
    assert_eq!(
        encode_save_records(&vec![records[0].clone(); 15]),
        Err(SaveError::Capacity)
    );
    assert!(SaveArchive::parse(&[]).unwrap().records().is_empty());
}

#[test]
fn renamed_copy_shifts_owned_tail_but_working_serialization_preserves_fixed_tail() {
    let record = patterned_record(b"long-name");
    let shorter = record.renamed_copy(b"new").unwrap();
    assert_eq!(shorter.mission(), record.mission());
    assert_eq!(shorter.player(), record.player());
    assert_eq!(&shorter.tail()[..record.tail().len()], record.tail());
    assert_eq!(&shorter.tail()[record.tail().len()..], &[0; 6]);
    let longer = record.renamed_copy(b"longer-name").unwrap();
    assert_eq!(longer.tail(), &record.tail()[..record.tail().len() - 2]);
    let working = record
        .with_state(b"new", &record.mission(), &record.player())
        .unwrap();
    assert_eq!(working.mission(), record.mission());
    assert_eq!(working.player(), record.player());
    assert_eq!(working.tail(), &record.payload()[1088..]);
    assert_ne!(working.tail(), shorter.tail());
}

#[test]
fn mission_reset_clears_every_progress_word_and_preserves_full_dword_selection() {
    let mut before = patterned_record(b"before").mission();
    before.elapsed_seconds = 0x8000_ff01;
    assert!(before
        .owner_b0
        .iter()
        .chain(&before.owner_240)
        .chain(&before.owner_3c)
        .all(|word| *word != 0));
    assert_ne!(before.owner_a4, 0);
    for selection in [0, 22, 0x101, 0x8000_0001, u32::MAX] {
        for elapsed_policy in [MissionElapsedPolicy::Preserve, MissionElapsedPolicy::Clear] {
            let mut state = before.clone();
            state.reset(MissionResetRequest {
                selection,
                elapsed_policy,
            });
            assert_eq!(
                state,
                MissionSaveState {
                    mission_selection: selection,
                    elapsed_seconds: match elapsed_policy {
                        MissionElapsedPolicy::Preserve => before.elapsed_seconds,
                        MissionElapsedPolicy::Clear => 0,
                    },
                    owner_b0: [0; 100],
                    owner_240: [0; 100],
                    owner_3c: [0; 25],
                    owner_a4: 0,
                }
            );
        }
    }
}

#[test]
fn mission_reset_roundtrip_changes_only_the_serialized_mission_body() {
    let before = patterned_record(b"before");
    let mission_start = before.name().len() + 1;
    let mission_end = mission_start + MISSION_SAVE_BYTES;
    let player = before.player();
    for elapsed_policy in [MissionElapsedPolicy::Preserve, MissionElapsedPolicy::Clear] {
        let mut mission = before.mission();
        mission.reset(MissionResetRequest {
            selection: 0xfedc_3210,
            elapsed_policy,
        });
        let after =
            SaveRecord::from_state(before.name(), &mission, &player, before.tail()).unwrap();
        assert_eq!(
            &after.payload()[..mission_start],
            &before.payload()[..mission_start]
        );
        assert_eq!(
            &after.payload()[mission_end..],
            &before.payload()[mission_end..]
        );
        assert_eq!(
            &after.payload()[mission_start..mission_start + 4],
            &0xfedc_3210u32.to_le_bytes()
        );
        assert_eq!(
            &after.payload()[mission_start + 4..mission_start + 8],
            &mission.elapsed_seconds.to_le_bytes()
        );
        assert!(after.payload()[mission_start + 8..mission_end]
            .iter()
            .all(|byte| *byte == 0));
        let bytes = encode_save_records(std::slice::from_ref(&after)).unwrap();
        let archive = SaveArchive::parse(&bytes).unwrap();
        assert_eq!(archive.records(), &[after]);
        assert_eq!(archive.records()[0].mission(), mission);
        assert_eq!(archive.records()[0].player(), player);
        assert_eq!(archive.records()[0].tail(), before.tail());
    }
}

#[test]
fn explicit_new_state_requires_correct_tail_length() {
    let mission = MissionSaveState::new_game();
    let player = PlayerSaveState::default();
    assert_eq!(mission.mission_selection, 22);
    assert_eq!(mission.elapsed_seconds, 0);
    assert_eq!(mission.owner_a4, 0);
    assert!(mission
        .owner_b0
        .iter()
        .chain(&mission.owner_240)
        .chain(&mission.owner_3c)
        .all(|v| *v == 0));
    assert!(matches!(
        SaveRecord::from_state(b"one", &mission, &player, &[]),
        Err(SaveError::TailLength {
            expected: 28,
            actual: 0
        })
    ));
    let record = SaveRecord::from_state(b"one", &mission, &player, &[0xa5; 28]).unwrap();
    assert_eq!(record.tail(), &[0xa5; 28]);
}

#[test]
fn offline_player_owns_prefix_constants_and_the_grant_closure_mask() {
    let mask = [0x2472, 0x0000, 0x4100, 0x1440_0000, 0x0008, 0x0000];
    let player = PlayerSaveState::new_player(mask);
    assert_eq!(player.owner_144, 1);
    assert_eq!(player.owner_148, 0);
    assert_eq!(player.owner_14c, 8);
    assert_eq!(player.owner_150[0], 0xFFFF_FFFF);
    assert_eq!(&player.owner_150[1..], &[0; 15]);
    assert_eq!(player.owner_a0, mask);
    assert_eq!(player.owner_100, [0; 6]);
    assert_eq!(player.owner_120, [0; 4]);
    // The constructor output round-trips through the mission/player codec.
    let mission = MissionSaveState::new_game();
    let record = SaveRecord::from_state(b"Jameson", &mission, &player, &[0x0f; 24]).unwrap();
    assert_eq!(record.player(), player);
    assert_eq!(record.mission().mission_selection, 22);
}

#[test]
fn working_write_retains_native_mask_when_strict_nested_capacity_check_fails() {
    let record = patterned_record(b"old");
    let mission = MissionSaveState::new_game();
    let player = PlayerSaveState::default();
    let rewritten = record.with_state(&[b'x'; 31], &mission, &player).unwrap();
    assert_eq!(rewritten.mission(), mission);
    assert_eq!(&rewritten.payload()[1092..], &record.payload()[1092..]);
    assert_eq!(rewritten.player().owner_144, 0);
    let complete = SaveRecord::from_state(&[b'x'; 31], &mission, &player, &[]).unwrap();
    assert_eq!(complete.player(), player);
    assert_ne!(rewritten.player().owner_a0, complete.player().owner_a0);
    let fits = record.with_state(&[b'x'; 30], &mission, &player).unwrap();
    assert_eq!(fits.player(), player);
    assert_eq!(fits.tail(), &record.payload()[1115..]);
}

#[test]
fn mission_copy_uses_signed_inclusive_capacity_and_absolute_failure_cursor() {
    let reference = patterned_record(b"P");
    let mission = reference.mission();
    let source_before = mission.clone();
    for remaining_bytes in [909, 0x8000_0000, u32::MAX] {
        let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
        let before = payload;
        // Capacity failure must precede this deliberately unbacked cursor.
        let mut cursor = u32::MAX;
        assert_eq!(
            copy_save_state(SaveStateCopyRequest {
                payload: &mut payload,
                cursor: &mut cursor,
                remaining_bytes,
                source: SaveStateCopySource::Mission(&mission),
            }),
            Ok(false)
        );
        assert_eq!(cursor, 910);
        assert_eq!(payload, before);
    }
    let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
    let mut cursor = 3;
    assert_eq!(
        copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes: 910,
            source: SaveStateCopySource::Mission(&mission),
        }),
        Ok(true)
    );
    assert_eq!(cursor, 913);
    assert_eq!(&payload[3..913], &reference.payload()[2..912]);
    assert_eq!(&payload[..3], &[0xa5; 3]);
    assert!(payload[913..].iter().all(|byte| *byte == 0xa5));
    assert_eq!(mission, source_before);
}

#[test]
fn player_copy_strict_signed_prefix_failure_replaces_cursor_without_writes() {
    let reference = patterned_record(b"P");
    let player = reference.player();
    for remaining_bytes in [149, 150, 0x8000_0000, u32::MAX] {
        let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
        let before = payload;
        let mut cursor = u32::MAX;
        assert_eq!(
            copy_save_state(SaveStateCopyRequest {
                payload: &mut payload,
                cursor: &mut cursor,
                remaining_bytes,
                source: SaveStateCopySource::Player(&player),
            }),
            Ok(false)
        );
        assert_eq!(cursor, 150);
        assert_eq!(payload, before);
    }
}

#[test]
fn player_prefix_survives_nested_failure_and_cursor_becomes_absolute24() {
    let reference = patterned_record(b"P");
    let player = reference.player();
    let source_before = player.clone();
    for remaining_bytes in [151, 174] {
        let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
        let mut cursor = 27;
        assert_eq!(
            copy_save_state(SaveStateCopyRequest {
                payload: &mut payload,
                cursor: &mut cursor,
                remaining_bytes,
                source: SaveStateCopySource::Player(&player),
            }),
            Ok(false)
        );
        assert_eq!(cursor, 24);
        assert_eq!(&payload[27..177], &reference.payload()[912..1062]);
        assert_eq!(&payload[..27], &[0xa5; 27]);
        assert!(payload[177..].iter().all(|byte| *byte == 0xa5));
    }
    assert_eq!(player, source_before);
}

#[test]
fn player_remaining175_copies_nonzero_mask_and_preserves_fixed_tail() {
    let reference = patterned_record(b"P");
    let player = reference.player();
    let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
    let mut cursor = 5;
    assert_eq!(
        copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes: 175,
            source: SaveStateCopySource::Player(&player),
        }),
        Ok(true)
    );
    assert_eq!(cursor, 179);
    assert_eq!(&payload[5..179], &reference.payload()[912..1086]);
    assert_eq!(&payload[..5], &[0xa5; 5]);
    assert!(payload[179..].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn mission_late_physical_range_error_keeps_first_scalar_and_advanced_cursor() {
    let reference = patterned_record(b"P");
    let mission = reference.mission();
    let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
    let mut cursor = 1112;
    // Supplied native capacity and physical backing are distinct. This is a
    // reached typed block boundary, not an original partial-fault assertion.
    assert_eq!(
        copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes: 910,
            source: SaveStateCopySource::Mission(&mission),
        }),
        Err(SaveError::StateCopyRange {
            cursor: 1116,
            bytes: 4
        })
    );
    assert_eq!(cursor, 1116);
    assert_eq!(&payload[1112..], &reference.payload()[2..6]);
    assert!(payload[..1112].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn player_late_prefix_range_error_retains_completed_array_and_no_nested_store() {
    let reference = patterned_record(b"P");
    let player = reference.player();
    let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
    let mut cursor = 1092;
    assert_eq!(
        copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes: 175,
            source: SaveStateCopySource::Player(&player),
        }),
        Err(SaveError::StateCopyRange {
            cursor: 1116,
            bytes: 4
        })
    );
    assert_eq!(cursor, 1116);
    assert_eq!(&payload[1092..], &reference.payload()[912..936]);
    assert!(payload[..1092].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn player_scalar120_range_failure_preserves_the_first_dword_and_advance() {
    let reference = patterned_record(b"P");
    let player = reference.player();
    let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
    let mut cursor = 1080;
    assert_eq!(
        copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes: 175,
            source: SaveStateCopySource::Player(&player),
        }),
        Err(SaveError::StateCopyRange {
            cursor: 1116,
            bytes: 4,
        })
    );
    assert_eq!(cursor, 1116);
    // The first32 prefix bytes and +120 scalar complete before +124 stops.
    assert_eq!(&payload[1080..], &reference.payload()[912..948]);
    assert!(payload[..1080].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn nested_capacity_gate_precedes_mask_range_and_late_range_keeps_full_prefix() {
    let reference = patterned_record(b"P");
    let player = reference.player();
    for remaining_bytes in [174, 175] {
        let mut payload = [0xa5; SAVE_PAYLOAD_BYTES];
        let mut cursor = 966;
        let result = copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes,
            source: SaveStateCopySource::Player(&player),
        });
        if remaining_bytes == 174 {
            assert_eq!(result, Ok(false));
            assert_eq!(cursor, 24);
        } else {
            assert_eq!(
                result,
                Err(SaveError::StateCopyRange {
                    cursor: 1116,
                    bytes: 24
                })
            );
            assert_eq!(cursor, 1116);
        }
        assert_eq!(&payload[966..], &reference.payload()[912..1062]);
        assert!(payload[..966].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn mission_only_composition_preserves_all_bytes_of_the_absent_player_suffix() {
    let reference = patterned_record(b"P");
    let mission = reference.mission();
    let prior = patterned_record(b"prior");
    let mut payload = *prior.payload();
    // A future live Serialize supplies emitted count+1; the copy API neither
    // interprets this name nor selects/fabricates an optional player.
    payload[..4].copy_from_slice(b"A%B\0");
    let mut cursor = 4;
    assert_eq!(
        copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes: 1112,
            source: SaveStateCopySource::Mission(&mission),
        }),
        Ok(true)
    );
    assert_eq!(cursor, 914);
    assert_eq!(&payload[..4], b"A%B\0");
    assert_eq!(&payload[4..914], &reference.payload()[2..912]);
    // Absent player means no Player call; its entire fixed suffix survives.
    assert_eq!(&payload[914..], &prior.payload()[914..]);
}

#[test]
fn name30_and31_keep_distinct_native_cursor_and_complete_intrinsic_mask_policies() {
    let reference = patterned_record(b"P");
    let mission = reference.mission();
    let player = reference.player();
    let prior = patterned_record(b"old");
    for name_len in [30, 31] {
        let name = vec![b'x'; name_len];
        let mut payload = *prior.payload();
        payload[..name_len].copy_from_slice(&name);
        payload[name_len] = 0;
        let mut cursor = (name_len + 1) as u32;
        let remaining_bytes = 1116 - cursor;
        assert_eq!(
            copy_save_state(SaveStateCopyRequest {
                payload: &mut payload,
                cursor: &mut cursor,
                remaining_bytes,
                source: SaveStateCopySource::Mission(&mission),
            }),
            Ok(true)
        );
        assert_eq!(cursor, (name_len + 911) as u32);
        let remaining_bytes = 1116 - cursor;
        let result = copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes,
            source: SaveStateCopySource::Player(&player),
        });
        let player_start = name_len + 911;
        assert_eq!(
            &payload[player_start..player_start + 150],
            &reference.payload()[912..1062]
        );
        if name_len == 31 {
            assert_eq!(result, Ok(false));
            assert_eq!(cursor, 24);
            assert_eq!(&payload[1092..], &prior.payload()[1092..]);
            let complete = SaveRecord::from_state(&name, &mission, &player, &[]).unwrap();
            assert_eq!(
                &complete.payload()[1092..],
                &reference.payload()[1062..1086]
            );
            assert_eq!(complete.player(), player);
        } else {
            assert_eq!(result, Ok(true));
            assert_eq!(cursor, 1115);
            assert_eq!(&payload[1091..1115], &reference.payload()[1062..1086]);
            assert_eq!(payload[1115], prior.payload()[1115]);
        }
        assert_eq!(
            prior
                .with_state(&name, &mission, &player)
                .unwrap()
                .payload(),
            &payload
        );
    }
}

#[test]
fn state_read_uses_emitted_cursor_without_reinterpreting_the_raw_name() {
    let record = patterned_record(b"A%%B");
    let source = *record.payload();
    let prior = patterned_record(b"prior");
    let mut mission = prior.mission();
    // Actual formatting emits A%B (count3): Restore reads the ORIGINAL
    // backing at4, while the intrinsic raw-name body still starts at5.
    let mut cursor = 4;
    read_save_state(SaveStateReadRequest {
        payload: record.payload(),
        cursor: &mut cursor,
        target: SaveStateReadTarget::Mission(&mut mission),
    })
    .unwrap();
    assert_eq!(cursor, 914);
    assert_eq!(mission.mission_selection, source_word(&source, 4));
    assert_eq!(mission.elapsed_seconds, source_word(&source, 8));
    assert_eq!(mission.owner_b0[99], source_word(&source, 408));
    assert_eq!(mission.owner_240[0], source_word(&source, 412));
    assert_eq!(mission.owner_3c[24], source_word(&source, 908));
    assert_eq!(mission.owner_a4, source_half(&source, 912));
    assert_eq!(record.mission().mission_selection, source_word(&source, 5));
    assert_ne!(mission, record.mission());

    let mut player = prior.player();
    read_save_state(SaveStateReadRequest {
        payload: record.payload(),
        cursor: &mut cursor,
        target: SaveStateReadTarget::Player(&mut player),
    })
    .unwrap();
    assert_eq!(cursor, 1088);
    assert_eq!(player.owner_100[0], source_word(&source, 914));
    assert_eq!(player.owner_a0[5], source_word(&source, 1084));
    assert_eq!(record.player().owner_100[0], source_word(&source, 915));
    assert_ne!(player, record.player());
    assert_eq!(record.name(), b"A%%B");
    assert_eq!(record.payload(), &source);
}

#[test]
fn state_read_restores_the_full_mask_at_both_name30_and_name31() {
    let reference = patterned_record(b"P");
    let mission = reference.mission();
    let player = reference.player();
    let before = patterned_record(b"old");
    for name_len in [30, 31] {
        let name = vec![b'x'; name_len];
        let tail = vec![0x79; SAVE_PAYLOAD_BYTES - name_len - 1 - STATE_BYTES];
        let record = SaveRecord::from_state(&name, &mission, &player, &tail).unwrap();
        let source = *record.payload();
        let mut restored_mission = before.mission();
        let mut restored_player = before.player();
        let mut cursor = (name_len + 1) as u32;
        read_save_state(SaveStateReadRequest {
            payload: record.payload(),
            cursor: &mut cursor,
            target: SaveStateReadTarget::Mission(&mut restored_mission),
        })
        .unwrap();
        assert_eq!(cursor, (name_len + 911) as u32);
        read_save_state(SaveStateReadRequest {
            payload: record.payload(),
            cursor: &mut cursor,
            target: SaveStateReadTarget::Player(&mut restored_player),
        })
        .unwrap();
        assert_eq!(cursor, (name_len + 1085) as u32);
        assert_eq!(restored_mission, mission);
        assert_eq!(restored_player, player);
        assert_eq!(record.mission(), mission);
        assert_eq!(record.player(), player);
        assert_eq!(record.payload(), &source);
    }
}

#[test]
fn mission_read_errors_distinguish_first_scalar_from_reached_elapsed() {
    let source = *patterned_record(b"source").payload();
    let before = patterned_record(b"old").mission();
    for start in [1112, 1108] {
        let mut mission = before.clone();
        let mut cursor = start;
        let expected_error = if start == 1112 {
            SaveError::StateReadRange {
                cursor: 1116,
                bytes: 4,
            }
        } else {
            SaveError::StateReadRange {
                cursor: 1116,
                bytes: 400,
            }
        };
        assert_eq!(
            read_save_state(SaveStateReadRequest {
                payload: &source,
                cursor: &mut cursor,
                target: SaveStateReadTarget::Mission(&mut mission),
            }),
            Err(expected_error)
        );
        assert_eq!(cursor, 1116, "no absolute910 copy-failure cursor");
        let mut expected = before.clone();
        expected.mission_selection = source_word(&source, start as usize);
        if start == 1108 {
            expected.elapsed_seconds = source_word(&source, 1112);
        }
        assert_eq!(mission, expected);
    }
}

#[test]
fn mission_read_late_array_and_word_errors_retain_only_completed_blocks() {
    let source = *patterned_record(b"source").payload();
    let before = patterned_record(b"old").mission();
    for (start, completed_arrays, failed_bytes, failed_cursor) in
        [(700, 1, 400, 1108), (300, 2, 100, 1108), (208, 3, 2, 1116)]
    {
        let mut mission = before.clone();
        let mut cursor = start as u32;
        assert_eq!(
            read_save_state(SaveStateReadRequest {
                payload: &source,
                cursor: &mut cursor,
                target: SaveStateReadTarget::Mission(&mut mission),
            }),
            Err(SaveError::StateReadRange {
                cursor: failed_cursor,
                bytes: failed_bytes,
            })
        );
        assert_eq!(cursor, failed_cursor);
        let mut expected = before.clone();
        expected.mission_selection = source_word(&source, start);
        expected.elapsed_seconds = source_word(&source, start + 4);
        expected.owner_b0 =
            std::array::from_fn(|index| source_word(&source, start + 8 + index * 4));
        if completed_arrays >= 2 {
            expected.owner_240 =
                std::array::from_fn(|index| source_word(&source, start + 408 + index * 4));
        }
        if completed_arrays == 3 {
            expected.owner_3c =
                std::array::from_fn(|index| source_word(&source, start + 808 + index * 4));
        }
        assert_eq!(mission, expected);
    }
}

#[test]
fn player_read_scalar120_error_retains_first_dword_and_prior_fields() {
    let source = *patterned_record(b"source").payload();
    let before = patterned_record(b"old").player();
    let mut player = before.clone();
    let mut cursor = 1080;
    assert_eq!(
        read_save_state(SaveStateReadRequest {
            payload: &source,
            cursor: &mut cursor,
            target: SaveStateReadTarget::Player(&mut player),
        }),
        Err(SaveError::StateReadRange {
            cursor: 1116,
            bytes: 4,
        })
    );
    assert_eq!(cursor, 1116);
    let mut expected = before;
    expected.owner_100 = std::array::from_fn(|index| source_word(&source, 1080 + index * 4));
    expected.owner_118 = source_word(&source, 1104);
    expected.owner_11c = source_half(&source, 1108);
    expected.owner_11e = source_half(&source, 1110);
    expected.owner_120[0] = source_word(&source, 1112);
    assert_eq!(player, expected);
}

#[test]
fn player_read_compound_errors_preserve_whole_unreached_blocks() {
    let source = *patterned_record(b"source").payload();
    let before = patterned_record(b"old").player();
    // Source bytes would permit PART of each compound native store. This
    // typed containment bounds the entire cursor-advance block first.
    for (start, failed_bytes, failed_cursor) in [(988, 8, 1112), (979, 6, 1111), (969, 12, 1107)] {
        let mut player = before.clone();
        let mut cursor = start;
        assert_eq!(
            read_save_state(SaveStateReadRequest {
                payload: &source,
                cursor: &mut cursor,
                target: SaveStateReadTarget::Player(&mut player),
            }),
            Err(SaveError::StateReadRange {
                cursor: failed_cursor,
                bytes: failed_bytes,
            })
        );
        assert_eq!(cursor, failed_cursor);
        let start = start as usize;
        assert_eq!(player.owner_100[0], source_word(&source, start));
        assert_eq!(player.owner_118, source_word(&source, start + 24));
        assert_eq!(player.owner_120[3], source_word(&source, start + 44));
        assert_eq!(player.owner_150[15], source_word(&source, start + 120));
        if failed_bytes == 8 {
            assert_eq!(player.owner_13c, before.owner_13c);
        } else {
            assert_eq!(
                player.owner_13c,
                std::array::from_fn(|index| source_word(&source, start + 124 + index * 4))
            );
        }
        if failed_bytes == 12 {
            assert_eq!(
                player.owner_190,
                std::array::from_fn(|index| source_half(&source, start + 132 + index * 2))
            );
        } else {
            assert_eq!(player.owner_190, before.owner_190);
        }
        assert_eq!(player.owner_130, before.owner_130);
        assert_eq!(player.owner_a0, before.owner_a0);
    }
}

#[test]
fn player_read_late_mask_error_keeps_complete_prefix_without_absolute24_cursor() {
    let source = *patterned_record(b"source").payload();
    let before = patterned_record(b"old").player();
    let mut player = before.clone();
    let mut cursor = 960;
    assert_eq!(
        read_save_state(SaveStateReadRequest {
            payload: &source,
            cursor: &mut cursor,
            target: SaveStateReadTarget::Player(&mut player),
        }),
        Err(SaveError::StateReadRange {
            cursor: 1110,
            bytes: 24,
        })
    );
    assert_eq!(cursor, 1110);
    assert_eq!(player.owner_a0, before.owner_a0);
    // The existing complete copy direction independently checks that every
    // preceding read field represents the actual150-byte source prefix.
    let mut encoded = [0; SAVE_PAYLOAD_BYTES];
    let mut output_cursor = 0;
    assert_eq!(
        copy_save_state(SaveStateCopyRequest {
            payload: &mut encoded,
            cursor: &mut output_cursor,
            remaining_bytes: 175,
            source: SaveStateCopySource::Player(&player),
        }),
        Ok(true)
    );
    assert_eq!(&encoded[..150], &source[960..1110]);
}

#[test]
fn state_read_invalid_cursor_preserves_source_target_and_cursor() {
    let source = *patterned_record(b"source").payload();
    let original = source;
    let before = patterned_record(b"old");
    for start in [1116, 1117, u32::MAX] {
        let old_mission = before.mission();
        let mut mission = old_mission.clone();
        let mut cursor = start;
        assert_eq!(
            read_save_state(SaveStateReadRequest {
                payload: &source,
                cursor: &mut cursor,
                target: SaveStateReadTarget::Mission(&mut mission),
            }),
            Err(SaveError::StateReadRange {
                cursor: start,
                bytes: 4,
            })
        );
        assert_eq!(cursor, start);
        assert_eq!(mission, old_mission);

        let old_player = before.player();
        let mut player = old_player.clone();
        assert_eq!(
            read_save_state(SaveStateReadRequest {
                payload: &source,
                cursor: &mut cursor,
                target: SaveStateReadTarget::Player(&mut player),
            }),
            Err(SaveError::StateReadRange {
                cursor: start,
                bytes: 24,
            })
        );
        assert_eq!(cursor, start);
        assert_eq!(player, old_player);
        assert_eq!(source, original);
    }
}

fn source_word(source: &[u8; SAVE_PAYLOAD_BYTES], offset: usize) -> u32 {
    u32::from_le_bytes(source[offset..offset + 4].try_into().unwrap())
}

fn source_half(source: &[u8; SAVE_PAYLOAD_BYTES], offset: usize) -> u16 {
    u16::from_le_bytes(source[offset..offset + 2].try_into().unwrap())
}
