use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_overlay, parse_terrain_bank, HandlerFraming, HandlerRegion, OverlayKey, Preload,
    RequestKind, TerrainBank, TERRAIN_GRID_SIDE, TERRAIN_HANDLER,
};

fn infestation_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn load_preload(root: &Path) -> Preload {
    Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap()
}

fn terrain_region<'a>(regions: &'a [HandlerRegion], name: &str) -> &'a HandlerRegion {
    regions
        .iter()
        .find(|region| region.handler.get() == TERRAIN_HANDLER)
        .unwrap_or_else(|| panic!("{name} has no handler 12 region"))
}

fn load_terrain(root: &Path, preload: &Preload, level: u8) -> (Vec<u8>, TerrainBank) {
    let name = format!("{level}.ovl");
    let key = OverlayKey::parse_filename(&name).unwrap();
    let requests = preload
        .leaf(key.level)
        .unwrap()
        .requests(key.request_kind());
    let bytes = fs::read(root.join("Overlay").join(&name)).unwrap();
    let layout = parse_overlay(&bytes, requests).unwrap();
    let bank = parse_terrain_bank(&bytes, terrain_region(&layout.regions, &name)).unwrap();
    (bytes, bank)
}

#[test]
fn retail_terrain_corpus_matches_handler_12_invariants() {
    let root = infestation_root();
    let preload = load_preload(&root);
    let mut active_levels = preload
        .leaves()
        .filter(|leaf| leaf.base.get(TERRAIN_HANDLER as usize).unwrap().count != 0)
        .map(|leaf| leaf.overlay_id)
        .collect::<Vec<_>>();
    active_levels.sort_unstable();
    assert_eq!(active_levels, (11u8..=43).collect::<Vec<_>>());
    for leaf in preload.leaves() {
        assert_eq!(
            leaf.base.get(TERRAIN_HANDLER as usize).unwrap().count,
            i16::from((11..=43).contains(&leaf.overlay_id))
        );
        for kind in [
            RequestKind::Difficulty,
            RequestKind::Language,
            RequestKind::DifficultyLanguage,
        ] {
            assert_eq!(
                leaf.requests(kind)
                    .get(TERRAIN_HANDLER as usize)
                    .unwrap()
                    .count,
                0
            );
        }
    }

    let expected_sparse_counts = [
        218usize, 203, 204, 86, 229, 297, 88, 224, 112, 192, 271, 121, 41, 86, 277, 360, 60, 293,
        79, 166, 154, 232, 119, 85, 0, 66, 82, 191, 157, 271, 1, 164, 246,
    ];
    let mut material_counts = [0u64; 4];
    let mut shade_histogram = [0u64; 16];
    let mut high_bit_counts = [0u64; 6];
    let mut sparse_count = 0usize;

    for (level, expected_sparse) in (11u8..=43).zip(expected_sparse_counts) {
        let (_, bank) = load_terrain(&root, &preload, level);
        assert_eq!(bank.entries.len(), 1, "level {level}");
        let entry = &bank.entries[0];
        assert_eq!(entry.cells().len(), TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE);
        assert_eq!(entry.buckets().len(), TERRAIN_GRID_SIDE);
        assert!(
            entry.pointer_placeholders().iter().all(|&value| value == 0),
            "level {level}"
        );
        let level_sparse = entry
            .buckets()
            .iter()
            .map(|bucket| bucket.records.len())
            .sum::<usize>();
        assert_eq!(level_sparse, expected_sparse, "level {level}");
        sparse_count += level_sparse;

        for cell in entry.cells() {
            material_counts[usize::from(cell.material())] += 1;
            shade_histogram[usize::from(cell.shade4())] += 1;
            for (relative_bit, count) in high_bit_counts.iter_mut().enumerate() {
                if cell.raw() & (1 << (10 + relative_bit)) != 0 {
                    *count += 1;
                }
            }
        }
    }

    assert_eq!(sparse_count, 5_375);
    assert_eq!(
        material_counts,
        [2_888_527, 1_889_015, 1_896_353, 1_976_857]
    );
    assert_eq!(
        high_bit_counts,
        [5_375, 0, 3_427_100, 4_669_476, 4_239_630, 2_634_691]
    );
    assert_eq!(
        shade_histogram,
        [
            503_252, 48_368, 892_258, 338_014, 504_214, 1_059_692, 1_655_897, 1_014_366, 1_255_783,
            604_617, 408_633, 360_197, 3_594, 1_756, 21, 90,
        ]
    );
}

#[test]
fn demo_terrain_regions_are_byte_identical_to_retail() {
    let retail_root = infestation_root();
    let demo_root = retail_root.join("infestation_demo");
    let retail_preload = load_preload(&retail_root);
    let demo_preload = load_preload(&demo_root);

    for level in [12, 28, 35] {
        let (retail_bytes, retail_bank) = load_terrain(&retail_root, &retail_preload, level);
        let (demo_bytes, demo_bank) = load_terrain(&demo_root, &demo_preload, level);
        assert_eq!(retail_bank, demo_bank, "level {level}");
        assert_eq!(
            retail_bank.payload.get(&retail_bytes).unwrap(),
            demo_bank.payload.get(&demo_bytes).unwrap(),
            "level {level}"
        );
    }
}

#[test]
fn every_retail_terrain_region_uses_standard_framing_and_one_entry() {
    let root = infestation_root();
    let preload = load_preload(&root);

    for level in 11u8..=43 {
        let name = format!("{level}.ovl");
        let requests = &preload.leaf(level).unwrap().base;
        let bytes = fs::read(root.join("Overlay").join(&name)).unwrap();
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = terrain_region(&layout.regions, &name);
        assert_eq!(region.requested_count, 1, "level {level}");
        assert!(matches!(region.framing, HandlerFraming::Standard { .. }));
        parse_terrain_bank(&bytes, region)
            .unwrap_or_else(|error| panic!("failed to parse level {level}: {error}"));
    }
}
