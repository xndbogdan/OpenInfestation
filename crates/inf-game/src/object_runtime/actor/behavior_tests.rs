use super::{build_actor_behavior, select_actor_behavior, ActorBehaviorTable};

#[test]
fn every_guard_profile_preserves_native_table_parameters_and_array_initialization() {
    // Independent literal oracle: 00426CC8..00427034 profile immediates,
    // plus 0048CC70 allocation counts and exact constructor-written bytes.
    let expected = [
        (0x0053_0e60, 2, 8_000, 150_000),
        (0x0053_0f10, 2, 3_000, 150_000),
        (0x0053_0f68, 1, 4_000, 150_000),
        (0x0053_0f98, 2, 2_500, 0),
        (0x0053_0ff0, 2, 1_500, 0),
        (0x0053_1048, 2, 500, 0),
        (0x0053_10a0, 1, 0, 2_000_000),
        (0x0053_10d0, 1, 0, 2_000_000),
        (0x0053_1100, 1, 0, 2_000_000),
        (0x0053_1130, 1, 2_560, 5_000_000),
        (0x0053_1160, 1, 8_000, 500_000),
        (0x0053_1190, 1, 8_000, 500_000),
        (0x0053_0eb8, 2, 4_000, 250_000),
    ];
    for (index, &(table, entries, first, second)) in expected.iter().enumerate() {
        for high_bit in [0, 0x80] {
            let state = build_actor_behavior(index as u8 + 1 | high_bit, 0, 3).unwrap();
            let groups = 1 + usize::from(high_bit != 0);
            assert_eq!(state.table_14 as u32, table);
            assert_eq!((state.word_04, state.word_08), (first, second));
            assert_eq!(state.byte_0c, 1);
            assert_eq!(state.byte_0d as usize, groups);
            assert_eq!(state.byte_0e as usize, entries);
            assert_eq!(state.byte_0f, 0);
            assert_eq!(state.byte_10, 0);
            assert_eq!(state.byte_11 as usize, entries);
            assert_eq!(
                state.entries_18,
                vec![[u32::MAX, 0, 0, 0, 0, 0, 0, 0]; entries]
            );
            assert_eq!(state.groups_1c.len(), groups);
            for group in &state.groups_1c {
                assert_eq!(group.coordinates_00_0a, [None; 6]);
                assert_eq!(group.word_0c, 4);
            }
            assert_eq!(state.words_24, vec![None; groups]);
            assert_eq!((state.word_20, state.word_2c, state.word_38), (0, 0, 0));
            assert_eq!((state.word_30, state.word_34), (None, None));
        }
    }
}

#[test]
fn selection_obeys_flag_guard_native_selector_cases_and_entry_modulus() {
    for mode in 1..=13 {
        for selector in [0, 1, 2, 3, 4, 18, u32::MAX] {
            let mut state = build_actor_behavior(mode, 0x2000, selector).unwrap();
            assert_eq!(state.byte_10, 0);
            let expected = if matches!(selector, 3 | 18) || state.byte_0e == 1 {
                0
            } else {
                1
            };
            select_actor_behavior(&mut state, 0, selector);
            assert_eq!(state.byte_10, expected);
            // An already populated selection is preserved by flag 2000.
            state.byte_10 = 5;
            select_actor_behavior(&mut state, 0x2000, selector);
            assert_eq!(state.byte_10, 5);
            select_actor_behavior(&mut state, 0, selector);
            assert_eq!(state.byte_10, expected);
        }
    }
}

#[test]
fn outside_guard_range_does_not_request_replacement_and_owned_arrays_do_not_alias() {
    for mode in 0..=u8::MAX {
        assert_eq!(
            build_actor_behavior(mode, 0, 0).is_some(),
            (1..=13).contains(&(mode & 0x7f))
        );
    }
    let mut first = build_actor_behavior(0x81, 0, 1).unwrap();
    let second = build_actor_behavior(0x81, 0, 1).unwrap();
    first.entries_18[0][1] = 123;
    first.groups_1c[0].coordinates_00_0a[0] = Some(456);
    first.groups_1c[0].coordinates_00_0a[1] = Some(0);
    first.words_24[0] = Some(789);
    assert_eq!(second.entries_18[0][1], 0);
    assert_eq!(second.groups_1c[0].dwords_00_08()[0], None);
    assert_eq!(second.words_24[0], None);
    assert_eq!(second.table_14, ActorBehaviorTable::Mode01);
}

#[test]
fn guard_state_matches_original_segments_with_supplied_postallocation_storage() {
    use serde_json::{json, Value};

    // Each receipt has four separately checked allocation boundaries. Native
    // prefixes/tails and 00428390 execute unchanged; malloc/free do not run.
    // This does not promote supplied storage into a complete guard receipt.
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/actor_behavior_allocation_tails_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 208);
    assert_eq!(fixture["coverage"]["postallocation_tails"], 832);
    assert_eq!(fixture["coverage"]["selection_00428390"], 208);
    let word = |value: &Value| u32::try_from(value.as_u64().unwrap()).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mode = word(&case["mode_byte"]) as u8;
        let flags = word(&case["flags"]);
        let selector = word(&case["selector_b4"]);
        let state = build_actor_behavior(mode, flags, selector).unwrap();
        let groups: Vec<_> = state
            .groups_1c
            .iter()
            .map(|group| {
                json!({
                    "words_00_08": group.dwords_00_08(),
                    "word_0c": group.word_0c,
                })
            })
            .collect();
        let actual = json!({
            "word_04": state.word_04,
            "word_08": state.word_08,
            "byte_0c": state.byte_0c,
            "byte_0d": state.byte_0d,
            "byte_0e": state.byte_0e,
            "byte_0f": state.byte_0f,
            "byte_10": state.byte_10,
            "byte_11": state.byte_11,
            "table_14": state.table_14 as u32,
            "entries_18": state.entries_18,
            "groups_1c": groups,
            "word_20": state.word_20,
            "words_24": state.words_24,
            "word_2c": state.word_2c,
            "word_30": state.word_30,
            "word_34": state.word_34,
            "word_38": state.word_38,
        });
        assert_eq!(
            actual, case["state"],
            "mode{mode:02x} flags{flags:08x} selector{selector}"
        );
    }
}
