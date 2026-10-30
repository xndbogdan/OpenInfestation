use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_material_bank, parse_overlay, HandlerRegion, MaterialBank, OverlayKey, Preload,
    MATERIAL_HANDLER, TERRAIN_MATERIAL_PALETTE_ENTRIES,
};

fn infestation_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn load_preload(root: &Path) -> Preload {
    Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap()
}

fn material_region<'a>(regions: &'a [HandlerRegion], name: &str) -> &'a HandlerRegion {
    regions
        .iter()
        .find(|region| region.handler.get() == MATERIAL_HANDLER)
        .unwrap_or_else(|| panic!("{name} has no handler 7 region"))
}

fn load_material_bank(root: &Path, preload: &Preload, level: u8) -> (Vec<u8>, MaterialBank) {
    let name = format!("{level}.ovl");
    let key = OverlayKey::parse_filename(&name).unwrap();
    let requests = preload
        .leaf(key.level)
        .unwrap()
        .requests(key.request_kind());
    let bytes = fs::read(root.join("Overlay").join(&name)).unwrap();
    let layout = parse_overlay(&bytes, requests).unwrap();
    let bank = parse_material_bank(&bytes, material_region(&layout.regions, &name)).unwrap();
    (bytes, bank)
}

#[test]
fn every_retail_and_demo_handler_7_region_has_exact_u32_stride() {
    assert_material_inventory(&infestation_root(), 35, 378, 1_512);
    assert_material_inventory(&infestation_root().join("infestation_demo"), 5, 138, 552);
}

#[test]
fn retail_terrain_material_palettes_match_the_exact_corpus() {
    let root = infestation_root();
    let preload = load_preload(&root);
    let mut fingerprint = 0xcbf2_9ce4_8422_2325u64;

    for level in 11u8..=43 {
        let (_, bank) = load_material_bank(&root, &preload, level);
        assert_eq!(bank.entries.len(), TERRAIN_MATERIAL_PALETTE_ENTRIES);
        assert!(bank.entries.iter().all(|entry| entry.high_word() == 0));
        // These banks come from the same base OVLs as proven H12 terrain.
        let palette = bank.palette8().unwrap();
        assert_eq!(palette.entries().len(), TERRAIN_MATERIAL_PALETTE_ENTRIES);

        for entry in palette.entries() {
            for byte in entry.raw().to_le_bytes() {
                fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
            }
        }
    }

    assert_eq!(fingerprint, 0x2b00_24d8_554a_18cf);
    let (_, level_11) = load_material_bank(&root, &preload, 11);
    assert_eq!(
        level_11
            .entries
            .iter()
            .map(|entry| entry.raw())
            .collect::<Vec<_>>(),
        [0x2128, 0x318c, 0x318c, 0x08cc, 0x3de7, 0x2529, 0x1d84, 0x314a,]
    );
    let (_, level_29) = load_material_bank(&root, &preload, 29);
    let palette_29 = level_29.palette8().unwrap();
    assert_eq!(palette_29.entries()[0].raw(), 0x0000);
    assert_eq!(palette_29.entries()[3].raw(), 0x4066);
    assert_eq!(palette_29.entries()[4].raw(), 0x7c00);
    assert_eq!(
        palette_29.entries()[5..]
            .iter()
            .map(|entry| entry.raw())
            .collect::<Vec<_>>(),
        [0x7b35, 0x59ad, 0x5650]
    );
}

#[test]
fn demo_terrain_material_palettes_are_byte_identical_to_retail() {
    let retail_root = infestation_root();
    let demo_root = retail_root.join("infestation_demo");
    let retail_preload = load_preload(&retail_root);
    let demo_preload = load_preload(&demo_root);

    for level in [12, 28, 35] {
        let (retail_bytes, retail_bank) = load_material_bank(&retail_root, &retail_preload, level);
        let (demo_bytes, demo_bank) = load_material_bank(&demo_root, &demo_preload, level);
        assert_eq!(retail_bank, demo_bank, "level {level}");
        assert_eq!(
            retail_bank.payload.get(&retail_bytes).unwrap(),
            demo_bank.payload.get(&demo_bytes).unwrap(),
            "level {level}"
        );
    }
}

fn assert_material_inventory(
    root: &Path,
    expected_regions: usize,
    expected_entries: usize,
    expected_bytes: usize,
) {
    let preload = load_preload(root);
    let mut paths = fs::read_dir(root.join("Overlay"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort();

    let mut regions = 0usize;
    let mut entries = 0usize;
    let mut bytes = 0usize;
    for path in paths {
        let name = path.file_name().unwrap().to_str().unwrap();
        let key = OverlayKey::parse_filename(name).unwrap();
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        let source = fs::read(&path).unwrap();
        let layout = parse_overlay(&source, requests).unwrap();
        for region in layout
            .regions
            .iter()
            .filter(|region| region.handler.get() == MATERIAL_HANDLER)
        {
            let bank = parse_material_bank(&source, region)
                .unwrap_or_else(|error| panic!("failed to parse {name}: {error}"));
            regions += 1;
            entries += bank.entries.len();
            bytes += bank.payload.len();
        }
    }

    assert_eq!(regions, expected_regions);
    assert_eq!(entries, expected_entries);
    assert_eq!(bytes, expected_bytes);
}
