use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{parse_constructor_bank, parse_overlay, parse_world_bank, Preload};

fn corpus() -> (PathBuf, Preload) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let preload =
        Preload::parse(&fs::read(root.join("PreLoad.dat")).expect("retail PreLoad.dat")).unwrap();
    (root, preload)
}

#[test]
fn retail_constructor_prefixes_cover_every_bank_without_class_payload_guesses() {
    let (root, preload) = corpus();
    let mut selectors = BTreeSet::new();
    let mut total = 0;
    let mut relocations = 0;
    for (level, expected_count) in [(1, 131), (6, 1), (29, 12), (30, 12), (32, 6)] {
        let bytes = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&bytes, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 13)
            .unwrap();
        let bank = parse_constructor_bank(&bytes, region).unwrap();
        assert_eq!(bank.entries.len(), expected_count);
        for entry in &bank.entries {
            assert_eq!(entry.prefix_span.len(), 4);
            assert_eq!(
                entry.payload_offset as usize,
                entry.allocation_offset as usize + 4
            );
            selectors.insert(entry.constructor_selector);
        }
        total += bank.entries.len();
        relocations += bank.relocations.len();
    }
    assert_eq!(total, 162);
    assert_eq!(selectors.len(), 85);
    assert_eq!(selectors.first(), Some(&2));
    assert_eq!(selectors.last(), Some(&94));
    assert_eq!(relocations, 1);
}

#[test]
fn retail_world_graphs_cover_all_4486_authored_placements() {
    let (root, preload) = corpus();
    let expected_counts = [
        74, 142, 101, 179, 160, 228, 106, 153, 183, 148, 264, 128, 149, 173, 128, 132, 118, 219,
        108, 158, 127, 118, 98, 143, 49, 140, 74, 134, 264, 22, 52, 138, 76,
    ];
    let mut total = 0;
    for (level, expected) in (11..=43).zip(expected_counts) {
        let bytes = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&bytes, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 14)
            .unwrap();
        let bank = parse_world_bank(&bytes, region).unwrap();
        assert_eq!(bank.entries.len(), 1, "level {level}");
        let world = &bank.entries[0];
        assert_eq!(world.placements.len(), expected, "level {level}");
        assert_eq!(world.placement_pointer_span.len(), expected * 4);
        for (index, placement) in world.placements.iter().enumerate() {
            assert_eq!(placement.index as usize, index);
            assert_eq!(placement.prefix_span.len(), 16);
            assert_eq!(
                placement.payload_offset as usize,
                placement.allocation_offset as usize + 16
            );
            let pointer = &bytes[placement.pointer_span.start..placement.pointer_span.end];
            assert_eq!(
                u32::from_le_bytes(pointer.try_into().unwrap()),
                placement.allocation_offset
            );
            let field_offset = (placement.pointer_span.start - bank.main_payload.start) as u32;
            assert!(bank
                .relocations
                .iter()
                .any(|r| r.field_offset == field_offset
                    && r.target_offset == placement.allocation_offset));
        }
        if level == 11 {
            let first = &world.placements[0];
            assert_eq!(first.resource_argument, 0xffff_0002);
            assert_eq!(first.constructor_slot(), 2);
            assert_eq!(first.position_words, [39036, 0, 28200]);
            assert_eq!(first.rotation_words, [0, 0, 0]);
        }
        total += world.placements.len();
    }
    assert_eq!(total, 4486);
}
