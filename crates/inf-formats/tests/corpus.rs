use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_handler10_bank, parse_handler5_bank, parse_mission_parameters, parse_mission_placements,
    parse_overlay, parse_sound_bank, parse_string_bank, parse_word_resource_bank, BranchKind,
    HandlerFraming, OverlayKey, Preload, SceneNode, SoundEntryKind,
};

fn infestation_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn load_preload(root: &Path) -> Preload {
    Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap()
}

#[test]
fn retail_preload_matches_decompiled_grammar() {
    let root = infestation_root();
    let bytes = fs::read(root.join("PreLoad.dat")).unwrap();
    assert_eq!(bytes.len(), 18_280);

    let preload = Preload::parse(&bytes).unwrap();
    assert_eq!(preload.header.difficulty_flag, 0);
    assert_eq!(preload.header.language_flag, 0);
    assert_eq!(
        preload.header.handler_capacities,
        [950, 12, 2, 1828, 1, 7, 60, 122, 760, 76, 3, 1, 1, 146, 1, 1]
    );
    assert_eq!(preload.header.handler_3_allocation, 5_273_600);
    assert_eq!(preload.header.node_allocation, 49);

    let ids = preload
        .leaves()
        .map(|leaf| leaf.overlay_id)
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 46);
    assert_eq!(ids, (0u8..=43).chain([47, 48]).collect::<HashSet<_>>());

    let level_one_sounds = preload.leaf(1).unwrap().base.get(9).unwrap();
    assert_eq!(level_one_sounds.segments.len(), 3);
    assert_eq!(level_one_sounds.segments[1].global_delta, -19);
    assert_eq!(level_one_sounds.global_index(38), Some(19));
    assert_eq!(level_one_sounds.global_index(39), Some(68));
}

#[test]
fn retail_scene_branches_preserve_additive_and_exclusive_ownership() {
    fn children(node: &SceneNode, expected: BranchKind) -> &[SceneNode] {
        let SceneNode::Branch {
            branch_kind,
            children,
        } = node
        else {
            panic!("expected {expected:?} branch");
        };
        assert_eq!(*branch_kind, expected);
        children
    }

    fn overlay_id(node: &SceneNode) -> u8 {
        let SceneNode::Leaf(leaf) = node else {
            panic!("expected overlay leaf");
        };
        leaf.overlay_id
    }

    let preload = load_preload(&infestation_root());
    // FUN_004EF490 builds the all-child vtable 0x00529290 for the root.
    // Its second child is tag 1 / exclusive vtable 0x00529274: boot loads
    // 0 + 2; requesting frontend 47 replaces 2 while preserving 0.
    let root = children(&preload.root, BranchKind::Group);
    assert_eq!(root.len(), 2);
    assert_eq!(overlay_id(&root[0]), 0);
    let frontend_or_world = children(&root[1], BranchKind::Select);
    assert_eq!(frontend_or_world.len(), 3);
    assert_eq!(overlay_id(&frontend_or_world[0]), 2);
    assert_eq!(overlay_id(&frontend_or_world[1]), 47);

    // Tag 2 / all-child: shared bank 1 coexists with one selected world.
    let world = children(&frontend_or_world[2], BranchKind::Group);
    assert_eq!(world.len(), 2);
    assert_eq!(overlay_id(&world[0]), 1);
    let families = children(&world[1], BranchKind::Select);
    assert_eq!(families.len(), 9);
    let first_family = children(&families[0], BranchKind::Group);
    assert_eq!(overlay_id(&first_family[1]), 6);
    let first_family_levels = children(&first_family[0], BranchKind::Select);
    assert_eq!(
        first_family_levels
            .iter()
            .map(overlay_id)
            .collect::<Vec<_>>(),
        [11, 19, 22, 28, 35, 25, 43]
    );
}

#[test]
fn retail_and_demo_preload_files_are_identical() {
    let root = infestation_root();
    assert_eq!(
        fs::read(root.join("PreLoad.dat")).unwrap(),
        fs::read(root.join("infestation_demo/PreLoad.dat")).unwrap()
    );
}

#[test]
fn focused_corpus_exercises_each_special_framing() {
    let root = infestation_root();
    let preload = load_preload(&root);
    let cases = [
        "0.ovl", "1.ovl", "0d0.ovl", "0l0.ovl", "3.ovl", "12.ovl", "47.ovl",
    ];

    for name in cases {
        let key = OverlayKey::parse_filename(name).unwrap();
        let leaf = preload.leaf(key.level).unwrap();
        let bytes = fs::read(root.join("Overlay").join(name)).unwrap();
        let layout = parse_overlay(&bytes, leaf.requests(key.request_kind())).unwrap();
        assert_eq!(layout.covered_bytes(), bytes.len(), "{name}");
    }

    let key = OverlayKey::parse_filename("1.ovl").unwrap();
    let layout = parse_overlay(
        &fs::read(root.join("Overlay/1.ovl")).unwrap(),
        preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind()),
    )
    .unwrap();
    let models = layout
        .regions
        .iter()
        .find(|region| region.handler.get() == 8)
        .unwrap();
    assert!(matches!(
        &models.framing,
        HandlerFraming::ModelSubBlocks { total_entries: 278, blocks } if blocks.len() == 28
    ));
    let entities = layout
        .regions
        .iter()
        .find(|region| region.handler.get() == 13)
        .unwrap();
    assert!(matches!(
        &entities.framing,
        HandlerFraming::RelocationTail(tail)
            if tail.main_payload.len() == 9_716 && tail.relocation_count == 1
    ));

    let empty_key = OverlayKey::parse_filename("48.ovl").unwrap();
    let empty_requests = preload
        .leaf(empty_key.level)
        .unwrap()
        .requests(empty_key.request_kind());
    assert!(!empty_requests.has_active_handlers());
    assert!(fs::read(root.join("Overlay/48.ovl")).unwrap().is_empty());
}

#[test]
fn focused_demo_corpus_uses_the_same_framing_grammar() {
    let retail_root = infestation_root();
    let demo_root = retail_root.join("infestation_demo");
    let preload = load_preload(&demo_root);

    for name in ["0.ovl", "1.ovl", "0d0.ovl", "0l0.ovl", "12.ovl", "47.ovl"] {
        let key = OverlayKey::parse_filename(name).unwrap();
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        let bytes = fs::read(demo_root.join("Overlay").join(name)).unwrap();
        let layout = parse_overlay(&bytes, requests).unwrap();
        assert_eq!(layout.covered_bytes(), bytes.len(), "demo {name}");
    }

    assert_eq!(
        fs::metadata(demo_root.join("Overlay/1.ovl")).unwrap().len(),
        fs::metadata(retail_root.join("Overlay/1.ovl"))
            .unwrap()
            .len()
            + 88
    );
}

#[test]
fn retail_sound_banks_match_the_handler_9_runtime_layout() {
    let root = infestation_root();
    let preload = load_preload(&root);
    let mut pcm_entries = 0usize;
    let mut alias_entries = 0usize;
    let mut pcm_bytes = 0u64;

    for (name, expected_entries, expected_pcm, expected_alias) in
        [("0.ovl", 30, 24, 6), ("1.ovl", 47, 34, 13)]
    {
        let key = OverlayKey::parse_filename(name).unwrap();
        let leaf = preload.leaf(key.level).unwrap();
        let requests = leaf.requests(key.request_kind());
        let request = requests.get(9).unwrap();
        assert_eq!(request.count, expected_entries);

        let bytes = fs::read(root.join("Overlay").join(name)).unwrap();
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 9)
            .unwrap();
        let bank = parse_sound_bank(&bytes, region).unwrap();
        assert_eq!(bank.entries.len(), expected_entries as usize);

        let mut bank_pcm = 0usize;
        let mut bank_alias = 0usize;
        for entry in &bank.entries {
            assert!(matches!(entry.raw_type, 1 | 5));
            assert!(
                request.global_index(entry.local_index as u16).is_some(),
                "{name} local sound {} has no global mapping",
                entry.local_index
            );
            match entry.kind {
                SoundEntryKind::Pcm { data, byte_len, .. } => {
                    assert!(data.start >= bank.payload.start);
                    assert!(data.end <= bank.payload.end);
                    assert_eq!(data.len(), byte_len as usize);
                    assert_eq!(entry.fields_04_to_10[2..], [0, 0]);
                    bank_pcm += 1;
                    pcm_entries += 1;
                    pcm_bytes += u64::from(byte_len);
                }
                SoundEntryKind::Alias {
                    raw_target,
                    target_global_slot,
                    ..
                } => {
                    // `FUN_0050F760` casts field 04 directly to a signed short;
                    // retail stores no additional information in the high word.
                    assert_eq!(raw_target >> 16, 0);
                    assert!(target_global_slot >= 0);
                    bank_alias += 1;
                    alias_entries += 1;
                }
                SoundEntryKind::Unknown { .. } => unreachable!("unexpected retail sound type"),
            }
        }
        assert_eq!(bank_pcm, expected_pcm);
        assert_eq!(bank_alias, expected_alias);
    }

    assert_eq!(pcm_entries, 58);
    assert_eq!(alias_entries, 19);
    assert_eq!(pcm_bytes, 2_541_972);

    let level_one_request = preload.leaf(1).unwrap().base.get(9).unwrap();
    assert_eq!(level_one_request.global_index(0), Some(30));
    assert_eq!(level_one_request.global_index(37), Some(67));
    assert_eq!(level_one_request.global_index(38), Some(19));
    assert_eq!(level_one_request.global_index(39), Some(68));
    assert_eq!(level_one_request.global_index(46), Some(75));
}

#[test]
fn every_handler_0_string_bank_follows_the_runtime_c_string_walk() {
    for (root, expected) in [
        (infestation_root(), (164, 8_126, 375_362)),
        (
            infestation_root().join("infestation_demo"),
            (32, 5_702, 184_309),
        ),
    ] {
        let preload = load_preload(&root);
        let mut paths = fs::read_dir(root.join("Overlay"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        paths.sort();
        let mut banks = 0usize;
        let mut entries = 0usize;
        let mut payload_bytes = 0usize;
        let mut trailing_bytes = 0usize;
        for path in paths {
            let name = path.file_name().unwrap().to_str().unwrap();
            let key = OverlayKey::parse_filename(name).unwrap();
            let requests = preload
                .leaf(key.level)
                .unwrap()
                .requests(key.request_kind());
            if requests.get(0).unwrap().count == 0 {
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            let layout = parse_overlay(&bytes, requests).unwrap();
            let region = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == 0)
                .unwrap();
            let bank = parse_string_bank(&bytes, region).unwrap();
            for entry in &bank.entries {
                assert!(entry.raw(&bytes).is_some());
                assert_eq!(entry.record.end, entry.bytes.end + 1);
                assert_eq!(bytes[entry.bytes.end], 0);
            }
            banks += 1;
            entries += bank.entries.len();
            payload_bytes += bank.payload.len();
            trailing_bytes += bank.trailing.len();
        }
        assert_eq!(
            (banks, entries, payload_bytes),
            expected,
            "{}",
            root.display()
        );
        assert_eq!(trailing_bytes, 0, "{}", root.display());
    }
}

#[test]
fn mission_handlers_use_their_fixed_runtime_strides() {
    for (root, expected) in [
        (infestation_root(), (8, 8, 536, 8, 8)),
        (
            infestation_root().join("infestation_demo"),
            (2, 2, 193, 2, 2),
        ),
    ] {
        let preload = load_preload(&root);
        let mut paths = fs::read_dir(root.join("Overlay"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        paths.sort();
        let mut placement_banks = 0usize;
        let mut placement_entries = 0usize;
        let mut nonzero_slots = 0usize;
        let mut parameter_banks = 0usize;
        let mut parameter_entries = 0usize;
        for path in paths {
            let name = path.file_name().unwrap().to_str().unwrap();
            let key = OverlayKey::parse_filename(name).unwrap();
            let requests = preload
                .leaf(key.level)
                .unwrap()
                .requests(key.request_kind());
            let has_11 = requests.get(11).unwrap().count != 0;
            let has_15 = requests.get(15).unwrap().count != 0;
            if !has_11 && !has_15 {
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            let layout = parse_overlay(&bytes, requests).unwrap();
            if has_11 {
                let region = layout
                    .regions
                    .iter()
                    .find(|region| region.handler.get() == 11)
                    .unwrap();
                let bank = parse_mission_placements(&bytes, region).unwrap();
                placement_banks += 1;
                placement_entries += bank.entries.len();
                nonzero_slots += bank
                    .entries
                    .iter()
                    .flat_map(|entry| &entry.slots)
                    .filter(|slot| slot.resource_word != 0 || slot.parameter_word != 0)
                    .count();
            }
            if has_15 {
                let region = layout
                    .regions
                    .iter()
                    .find(|region| region.handler.get() == 15)
                    .unwrap();
                let bank = parse_mission_parameters(&bytes, region).unwrap();
                parameter_banks += 1;
                parameter_entries += bank.entries.len();
            }
        }
        assert_eq!(
            (
                placement_banks,
                placement_entries,
                nonzero_slots,
                parameter_banks,
                parameter_entries,
            ),
            expected,
            "{}",
            root.display()
        );
    }
}

#[test]
fn handlers_1_2_and_6_are_exact_four_byte_resource_banks() {
    for (root, expected_banks, expected_entries) in [
        (infestation_root(), [33, 5, 35], [152, 10, 135]),
        (
            infestation_root().join("infestation_demo"),
            [3, 2, 5],
            [12, 4, 67],
        ),
    ] {
        let preload = load_preload(&root);
        let mut paths = fs::read_dir(root.join("Overlay"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        paths.sort();
        let mut banks = [0usize; 3];
        let mut entries = [0usize; 3];
        for path in paths {
            let name = path.file_name().unwrap().to_str().unwrap();
            let key = OverlayKey::parse_filename(name).unwrap();
            let requests = preload
                .leaf(key.level)
                .unwrap()
                .requests(key.request_kind());
            if ![1usize, 2, 6]
                .into_iter()
                .any(|handler| requests.get(handler).unwrap().count != 0)
            {
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            let layout = parse_overlay(&bytes, requests).unwrap();
            for (slot, handler) in [1u8, 2, 6].into_iter().enumerate() {
                let Some(region) = layout
                    .regions
                    .iter()
                    .find(|region| region.handler.get() == handler)
                else {
                    continue;
                };
                let bank = parse_word_resource_bank(&bytes, region).unwrap();
                banks[slot] += 1;
                entries[slot] += bank.entries.len();
            }
        }
        assert_eq!(banks, expected_banks, "{}", root.display());
        assert_eq!(entries, expected_entries, "{}", root.display());
    }
}

#[test]
fn handlers_5_and_10_follow_their_runtime_record_tables() {
    for (root, expected_banks, expected_entries, expected_trailing) in [
        (infestation_root(), [1, 5], [7, 15], 288_020),
        (
            infestation_root().join("infestation_demo"),
            [1, 1],
            [7, 3],
            75_664,
        ),
    ] {
        let preload = load_preload(&root);
        let mut paths = fs::read_dir(root.join("Overlay"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        paths.sort();
        let mut banks = [0usize; 2];
        let mut entries = [0usize; 2];
        let mut handler10_trailing = 0usize;
        for path in paths {
            let name = path.file_name().unwrap().to_str().unwrap();
            let key = OverlayKey::parse_filename(name).unwrap();
            let requests = preload
                .leaf(key.level)
                .unwrap()
                .requests(key.request_kind());
            if requests.get(5).unwrap().count == 0 && requests.get(10).unwrap().count == 0 {
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            let layout = parse_overlay(&bytes, requests).unwrap();
            if let Some(region) = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == 5)
            {
                let bank = parse_handler5_bank(&bytes, region).unwrap();
                banks[0] += 1;
                entries[0] += bank.records.len();
            }
            if let Some(region) = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == 10)
            {
                let bank = parse_handler10_bank(&bytes, region).unwrap();
                banks[1] += 1;
                entries[1] += bank.records.len();
                handler10_trailing += bank.trailing_data.len();
            }
        }
        assert_eq!(banks, expected_banks, "{}", root.display());
        assert_eq!(entries, expected_entries, "{}", root.display());
        assert_eq!(handler10_trailing, expected_trailing, "{}", root.display());
    }
}

#[test]
fn every_retail_overlay_has_exact_handler_coverage() {
    assert_inventory(
        &infestation_root(),
        InventoryExpectation {
            files: 1_932,
            non_empty: 257,
            empty: 1_675,
            bytes: 158_707_834,
            framing: FramingExpectation {
                standard_regions: 334,
                atlas_regions: 86,
                atlas_records: 8_419,
                model_regions: 36,
                model_blocks: 457,
                model_entries: 5_142,
                relocation_regions: 38,
                relocation_records: 16_894,
            },
        },
    );
}

#[test]
fn every_demo_overlay_has_exact_handler_coverage() {
    assert_inventory(
        &infestation_root().join("infestation_demo"),
        InventoryExpectation {
            files: 147,
            non_empty: 44,
            empty: 103,
            bytes: 26_066_257,
            framing: FramingExpectation {
                standard_regions: 60,
                atlas_regions: 11,
                atlas_records: 1_348,
                model_regions: 6,
                model_blocks: 86,
                model_entries: 848,
                relocation_regions: 5,
                relocation_records: 1_494,
            },
        },
    );
}

struct InventoryExpectation {
    files: usize,
    non_empty: usize,
    empty: usize,
    bytes: u64,
    framing: FramingExpectation,
}

struct FramingExpectation {
    standard_regions: usize,
    atlas_regions: usize,
    atlas_records: usize,
    model_regions: usize,
    model_blocks: usize,
    model_entries: u64,
    relocation_regions: usize,
    relocation_records: u64,
}

fn assert_inventory(root: &Path, expected: InventoryExpectation) {
    let preload = load_preload(root);
    let mut paths = fs::read_dir(root.join("Overlay"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort();

    let mut non_empty = 0;
    let mut empty = 0;
    let mut input_bytes = 0u64;
    let mut covered_bytes = 0u64;
    let mut standard_regions = 0;
    let mut atlas_regions = 0;
    let mut atlas_records = 0;
    let mut model_regions = 0;
    let mut model_blocks = 0;
    let mut model_entries = 0u64;
    let mut relocation_regions = 0;
    let mut relocation_records = 0u64;
    for path in &paths {
        let name = path.file_name().unwrap().to_str().unwrap();
        let key = OverlayKey::parse_filename(name).unwrap();
        let requests = preload
            .leaf(key.level)
            .unwrap_or_else(|| panic!("missing leaf for {name}"))
            .requests(key.request_kind());
        let bytes = fs::read(path).unwrap();
        input_bytes += bytes.len() as u64;
        if bytes.is_empty() {
            empty += 1;
            assert!(!requests.has_active_handlers(), "{name}");
        } else {
            non_empty += 1;
            assert!(requests.has_active_handlers(), "{name}");
            let layout = parse_overlay(&bytes, requests)
                .unwrap_or_else(|error| panic!("failed to frame {name}: {error}"));
            assert_eq!(layout.covered_bytes(), bytes.len(), "{name}");
            covered_bytes += layout.covered_bytes() as u64;
            for region in &layout.regions {
                match &region.framing {
                    HandlerFraming::Standard { .. } => standard_regions += 1,
                    HandlerFraming::SpriteAtlas { records, .. } => {
                        atlas_regions += 1;
                        atlas_records += records.len();
                    }
                    HandlerFraming::ModelSubBlocks {
                        total_entries,
                        blocks,
                    } => {
                        model_regions += 1;
                        model_blocks += blocks.len();
                        model_entries += u64::from(*total_entries);
                    }
                    HandlerFraming::RelocationTail(tail) => {
                        relocation_regions += 1;
                        relocation_records += u64::from(tail.relocation_count);
                    }
                }
            }
        }
    }

    assert_eq!(paths.len(), expected.files);
    assert_eq!(non_empty, expected.non_empty);
    assert_eq!(empty, expected.empty);
    assert_eq!(input_bytes, expected.bytes);
    assert_eq!(covered_bytes, expected.bytes);
    assert_eq!(standard_regions, expected.framing.standard_regions);
    assert_eq!(atlas_regions, expected.framing.atlas_regions);
    assert_eq!(atlas_records, expected.framing.atlas_records);
    assert_eq!(model_regions, expected.framing.model_regions);
    assert_eq!(model_blocks, expected.framing.model_blocks);
    assert_eq!(model_entries, expected.framing.model_entries);
    assert_eq!(relocation_regions, expected.framing.relocation_regions);
    assert_eq!(relocation_records, expected.framing.relocation_records);
}
