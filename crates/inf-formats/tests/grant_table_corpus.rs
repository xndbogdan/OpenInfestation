use std::{fs, path::Path};

use inf_formats::{
    grant_closure, parse_grant_table, FrontendExecutable, GRANT_FIRST_ID, GRANT_RECORD_COUNT,
    PLAYER_INITIAL_GRANTS,
};

fn executable() -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    fs::read(root.join("Infest.exe")).expect("retail Infest.exe is required")
}

#[test]
fn retail_grant_table_holds_26_high_records_with_proven_dependencies() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let table = parse_grant_table(source).unwrap();
    assert_eq!(table.virtual_address, 0x53_B088);
    assert_eq!(table.records.len(), GRANT_RECORD_COUNT);
    assert_eq!(table.records.first().unwrap().id, GRANT_FIRST_ID);
    assert_eq!(
        table.records.last().unwrap().id,
        GRANT_FIRST_ID + GRANT_RECORD_COUNT as u32 - 1
    );
    // First record's raw extent confirms the +8/+A/+C dependency layout.
    let first = table.records.first().unwrap();
    assert_eq!(&first.raw[0..2], &[0x96, 0x01]);
    assert_eq!(first.dependencies, [0x60, 0, 0]);
    assert!(table.records.iter().all(|record| record
        .dependencies
        .iter()
        .all(|&dependency| u32::from(dependency) < inf_formats::GRANT_MASK_BITS)));
}

#[test]
fn offline_closure_grants_exactly_the_documented_twelve_ids() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let table = parse_grant_table(source).unwrap();
    // Recorded in SAVE_FORMAT.md: the installed table's rescan adds no IDs,
    // so the closure is exactly the twelve 00413140 construction grants.
    assert_eq!(
        grant_closure(&table, &PLAYER_INITIAL_GRANTS).unwrap(),
        [0x2472, 0x0000, 0x4100, 0x1440_0000, 0x0008, 0x0000]
    );
}

#[test]
fn retail_grant_dependencies_are_unsigned_and_merge_visits_source_before_target_in_order() {
    let bytes = executable();
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let half = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize;
    let pe = word(0x3c);
    let sections = pe + 24 + half(pe + 20);
    let section = (0..half(pe + 6))
        .map(|index| sections + index * 40)
        .find(|&at| &bytes[at..at + 8] == b".text\0\0\0")
        .unwrap();
    let code = |address: u32, length: usize| {
        let rva = address as usize - 0x0040_0000;
        assert!(
            rva >= word(section + 12) && rva + length <= word(section + 12) + word(section + 16)
        );
        let offset = word(section + 20) + rva - word(section + 12);
        &bytes[offset..offset + length]
    };
    assert_eq!(
        code(0x0047_5295, 16),
        [0x66, 0x8b, 6, 0x66, 0x85, 0xc0, 0x74, 0x0c, 0x50, 0x8b, 0xcd, 0xe8, 0x3b, 0, 0, 0]
    );
    assert_eq!(code(0x0047_52f0, 5), [0x25, 0xff, 0xff, 0, 0]);
    // Source argument→EBX; target this→EDI; loop ESI from 0 through 0xAE.
    // Each source ownership query precedes a conditional target grant call.
    assert_eq!(
        code(0x0047_5350, 46),
        [
            0x53, 0x8b, 0x5c, 0x24, 8, 0x56, 0x57, 0x8b, 0xf9, 0x33, 0xf6, 0x56, 0x8b, 0xcb, 0xe8,
            0x7d, 0xff, 0xff, 0xff, 0x84, 0xc0, 0x74, 8, 0x56, 0x8b, 0xcf, 0xe8, 0x51, 0xfd, 0xff,
            0xff, 0x46, 0x81, 0xfe, 0xaf, 0, 0, 0, 0x7c, 0xe3, 0x5f, 0x5e, 0x5b, 0xc2, 4, 0,
        ]
    );
}

#[test]
fn vehicle_behavior_table_reads_the_guard_copy_source() {
    let bytes = executable();
    let table =
        inf_formats::parse_vehicle_behavior_table(FrontendExecutable::parse(&bytes).unwrap())
            .unwrap();
    // 0041A620 copies 00530158..00530418; the first record opens like this.
    assert_eq!(
        table[..12],
        [
            0x0bb8_0001,
            0x0001_003b,
            0x0003_0d40,
            0x0001_86a0,
            0,
            0,
            0x400,
            0x0003_0d40,
            0x0100_0001,
            0,
            0,
            0x0bb8_0005
        ]
    );
    assert_eq!(table.len(), 16 * 11);
}

#[test]
fn grant_texts_resolve_low_and_high_record_words() {
    let bytes = executable();
    let texts = inf_formats::parse_grant_texts(FrontendExecutable::parse(&bytes).unwrap()).unwrap();
    // 00475320: eight-byte records at 0053ABD8 below 0x95.
    assert_eq!(
        [texts[1], texts[4], texts[0x32], texts[0x33]],
        [384, 370, 0, 538]
    );
    assert_eq!(texts.len(), 0xaf);
}

#[test]
fn grant_models_are_each_record_word_two() {
    let bytes = executable();
    let models =
        inf_formats::parse_grant_models(FrontendExecutable::parse(&bytes).unwrap()).unwrap();
    // Level 1's Truck mode (the vehicle's model 0x75) and Turbo boost,
    // product 0x11, the first high record and a product without a model.
    assert_eq!(
        [
            models[2],
            models[3],
            models[0x11],
            models[0x95],
            models[0x2a]
        ],
        [0x75, 0x107, 0x3c, 0xaf, 0]
    );
}
