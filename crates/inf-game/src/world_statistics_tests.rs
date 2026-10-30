use super::*;
const CATEGORIES: [ActorStatisticCategory; 5] = [
    ActorStatisticCategory::Flag10,
    ActorStatisticCategory::B4Eighteen,
    ActorStatisticCategory::OtherB4,
    ActorStatisticCategory::B4One,
    ActorStatisticCategory::B4ZeroOrUnresolved,
];

#[test]
fn combat_particle_hit_statistics_match_all_native_481050_cells() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/combat_particle_statistics_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25);
    for case in cases {
        let input = &case["input"];
        let categories = ActorRetirementAttribution {
            target: CATEGORIES[input["target"].as_u64().unwrap() as usize],
            source: CATEGORIES[input["source"].as_u64().unwrap() as usize],
        };
        let mut record = [0; RECORD_0C_BYTES];
        for (out, value) in record
            .chunks_exact_mut(4)
            .zip(input["words"].as_array().unwrap())
        {
            out.copy_from_slice(&(value.as_u64().unwrap() as u32).to_le_bytes());
        }
        let mut statistics = WorldStatistics::from_record(&mut record);
        statistics.record_particle_hit(categories);
        let offset = ((input["target"].as_u64().unwrap() + 2) * 5
            + input["source"].as_u64().unwrap()) as usize;
        assert_eq!(
            statistics.particle_hit_count(categories),
            case["output"][offset].as_u64().unwrap() as u32
        );
        let words: Vec<_> = record
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        let expected: Vec<_> = case["output"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(words, expected, "categories{categories:?}");
    }
}

#[test]
fn firing_statistics_keep_particle_and_actor_resource_banks_distinct_and_wrap() {
    let mut record = [0xa5; RECORD_0C_BYTES];
    record[..0x28].fill(0xff);
    for category in CATEGORIES {
        let mut statistics = WorldStatistics::from_record(&mut record);
        statistics.record_particle_firing(category);
        assert_eq!(statistics.particle_firing_count(category), 0);
        statistics.record_actor_firing(category);
    }
    assert_eq!(&record[..0x28], &[0; 0x28]);
    assert_eq!(&record[0x28..], &[0xa5; RECORD_0C_BYTES - 0x28]);
}

#[test]
fn category_flag_precedes_runtime_classification_and_unsigned_ids_stay_distinct() {
    for b4 in [0, 1, 18, 11, u32::MAX] {
        assert_eq!(
            ActorStatisticCategory::from_actor(0x10, b4),
            ActorStatisticCategory::Flag10
        );
    }
    for (b4, expected) in [
        (0, ActorStatisticCategory::B4ZeroOrUnresolved),
        (1, ActorStatisticCategory::B4One),
        (18, ActorStatisticCategory::B4Eighteen),
        (11, ActorStatisticCategory::OtherB4),
        (u32::MAX, ActorStatisticCategory::OtherB4),
    ] {
        assert_eq!(ActorStatisticCategory::from_actor(0x8000, b4), expected);
    }
}

#[test]
fn retirement_matrix_is_target_major_wrapping_and_preserves_other_statistics() {
    let mut record = [0xa5; RECORD_0C_BYTES];
    for (row, target) in CATEGORIES.into_iter().enumerate() {
        for (column, source) in CATEGORIES.into_iter().enumerate() {
            let attribution = ActorRetirementAttribution { target, source };
            let offset = (35 + row * 5 + column) * 4;
            assert_eq!(attribution.byte_offset(), offset);
            record[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            let mut statistics = WorldStatistics::from_record(&mut record);
            statistics.record_attribution(attribution);
            assert_eq!(statistics.attribution_count(attribution), 0);
        }
    }
    assert_eq!(&record[..0x8c], &[0xa5; 0x8c]);
    assert_eq!(&record[0x8c..0xf0], &[0; 0x64]);
    assert_eq!(&record[0xf0..], &[0xa5; RECORD_0C_BYTES - 0xf0]);
    record[0x154..].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut statistics = WorldStatistics::from_record(&mut record);
    statistics.increment_selector18_retirements();
    assert_eq!(statistics.selector18_retirements(), 0);
    assert_eq!(&record[0xf0..0x154], &[0xa5; 0x64]);
}

#[test]
fn damage_matrix_is_target_major_and_signed_amounts_wrap_as_native_dwords() {
    let mut record = [0xa5; RECORD_0C_BYTES];
    for (row, target) in CATEGORIES.into_iter().enumerate() {
        for (column, source) in CATEGORIES.into_iter().enumerate() {
            let categories = ActorRetirementAttribution { target, source };
            let statistic = ActorDamageStatistic {
                amount: 2,
                categories,
            };
            let offset = (60 + row * 5 + column) * 4;
            assert_eq!(statistic.byte_offset(), offset);
            record[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            let mut statistics = WorldStatistics::from_record(&mut record);
            statistics.record_damage(statistic);
            assert_eq!(statistics.damage_total(categories), 1);
            statistics.record_damage(ActorDamageStatistic {
                amount: -2,
                categories,
            });
            assert_eq!(statistics.damage_total(categories), u32::MAX);
        }
    }
    assert_eq!(&record[..0xf0], &[0xa5; 0xf0]);
    assert_eq!(&record[0xf0..0x154], &[0xff; 0x64]);
    assert_eq!(&record[0x154..], &[0xa5; RECORD_0C_BYTES - 0x154]);
}
