use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_overlay, parse_sprite_bank, AtlasRecord, ByteSpan, HandlerFraming, HandlerId,
    HandlerRegion, OverlayKey, Preload, Rgba8, SpriteBlendMode, SpriteEncoding, SpriteError,
    SpriteRenderMode, SpriteShade,
};

const ATLAS_X: u16 = 10;
const ATLAS_Y: u16 = 2;
const COLORS: u16 = 2;
const PALETTE_BYTES: usize = COLORS as usize * 66;

fn infestation_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn indexed_fixture(
    selectors: [u8; 2],
    variant_flags: &[u16],
    indices: &[u8],
) -> (Vec<u8>, HandlerRegion) {
    let table_len = 4 + variant_flags.len() * 0x1c;
    let main_len = table_len + variant_flags.len() * PALETTE_BYTES;
    let mut source = vec![0u8; main_len];
    source[0] = selectors[0];
    source[1] = selectors[1];

    for (index, &flags) in variant_flags.iter().enumerate() {
        let start = 4 + index * 0x1c;
        let palette_offset = table_len + index * PALETTE_BYTES;
        write_variant(
            &mut source,
            start,
            VariantSpec {
                sprite_id: 100 + index as u16,
                flags,
                color_count: COLORS,
                atlas_offset: u32::from(ATLAS_Y) * 2048 + u32::from(ATLAS_X),
                palette_offset: palette_offset as u32,
                width: indices.len() as u16,
                height: 1,
            },
        );
        write_test_palette(&mut source, palette_offset);
    }

    let pixel_start = source.len();
    source.extend_from_slice(indices);
    let pixel_end = source.len();
    let region = sprite_region(
        main_len,
        variant_flags.len().min(1) as u32,
        vec![AtlasRecord {
            span: ByteSpan::new(pixel_start, pixel_end),
            x_start: ATLAS_X,
            y_start: ATLAS_Y,
            x_end: ATLAS_X + indices.len() as u16,
            y_end: ATLAS_Y + 1,
            pixels: ByteSpan::new(pixel_start, pixel_end),
        }],
        source.len(),
    );
    (source, region)
}

fn direct_fixture(flags: u16, color_count: u16, atlas_x: u16) -> (Vec<u8>, HandlerRegion) {
    let main_len = 4 + 0x1c;
    let mut source = vec![0u8; main_len];
    source[0] = 0;
    source[1] = 0;
    write_variant(
        &mut source,
        4,
        VariantSpec {
            sprite_id: 200,
            flags,
            color_count,
            atlas_offset: u32::from(ATLAS_Y) * 2048 + u32::from(atlas_x),
            palette_offset: 0,
            width: 2,
            height: 1,
        },
    );
    let pixel_start = source.len();
    source.extend_from_slice(&0u16.to_le_bytes());
    source.extend_from_slice(&0x7c00u16.to_le_bytes());
    let pixel_end = source.len();
    let region = sprite_region(
        main_len,
        1,
        vec![AtlasRecord {
            span: ByteSpan::new(pixel_start, pixel_end),
            x_start: atlas_x,
            y_start: ATLAS_Y,
            x_end: atlas_x + 4,
            y_end: ATLAS_Y + 1,
            pixels: ByteSpan::new(pixel_start, pixel_end),
        }],
        source.len(),
    );
    (source, region)
}

fn sprite_region(
    main_len: usize,
    requested_count: u32,
    records: Vec<AtlasRecord>,
    source_len: usize,
) -> HandlerRegion {
    HandlerRegion {
        handler: HandlerId::ATLAS,
        requested_count,
        span: ByteSpan::new(0, source_len),
        framing: HandlerFraming::SpriteAtlas {
            main_payload: ByteSpan::new(0, main_len),
            records,
            terminator: ByteSpan::new(source_len, source_len),
        },
    }
}

#[derive(Clone, Copy)]
struct VariantSpec {
    sprite_id: u16,
    flags: u16,
    color_count: u16,
    atlas_offset: u32,
    palette_offset: u32,
    width: u16,
    height: u16,
}

fn write_variant(bytes: &mut [u8], start: usize, spec: VariantSpec) {
    put_u16(bytes, start, spec.sprite_id);
    put_u16(bytes, start + 2, 0);
    put_u16(bytes, start + 4, spec.flags);
    put_u16(bytes, start + 6, spec.color_count);
    put_u32(bytes, start + 8, spec.atlas_offset);
    put_u32(bytes, start + 0x0c, spec.palette_offset);
    put_u16(bytes, start + 0x10, spec.width);
    put_u16(bytes, start + 0x12, spec.height);
    put_u32(bytes, start + 0x14, 0);
    put_u32(bytes, start + 0x18, 0);
}

fn write_test_palette(bytes: &mut [u8], start: usize) {
    for shade in 0..32 {
        put_u16(bytes, start + shade * 2, 0x001f);
        put_u16(bytes, start + (32 + shade) * 2, 0x03e0);
    }
    put_u16(bytes, start + 128, 0x7c00);
    put_u16(bytes, start + 130, 0x7fff);
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn explicit_modes_select_physical_variants_and_palette_layout() {
    let (source, mut region) = indexed_fixture([0, 1], &[0x0014, 0x0024], &[0, 1]);
    region.requested_count = 1;
    let bank = parse_sprite_bank(&source, &region).unwrap();
    let entry = &bank.entries[0];

    assert_eq!(entry.selectors.physical_variant_count(), 2);
    assert_eq!(
        entry.variant(SpriteRenderMode::Mode0).unwrap().sprite_id,
        100
    );
    assert_eq!(
        entry.variant(SpriteRenderMode::Mode1).unwrap().sprite_id,
        101
    );

    let mode0 = entry.variant(SpriteRenderMode::Mode0).unwrap();
    let SpriteEncoding::Indexed { palette, .. } = mode0.encoding else {
        panic!("expected indexed sprite");
    };
    assert_eq!(palette.shade_ramps.len(), 128);
    assert_eq!(palette.base_colors.len(), 4);
    assert_eq!(palette.base_rgb555(&source, 0).unwrap(), 0x7c00);
    assert_eq!(palette.shade_rgb555(&source, 0, 17).unwrap(), 0x001f);

    assert_eq!(
        mode0.decode_rgba(&source, SpriteShade::Base).unwrap(),
        vec![
            Rgba8 {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            Rgba8 {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
        ]
    );
    assert_eq!(
        mode0.decode_rgba(&source, SpriteShade::Level(17)).unwrap()[0],
        Rgba8 {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );
}

#[test]
fn transparency_is_controlled_by_the_masked_flag() {
    let (unmasked_source, unmasked_region) = indexed_fixture([0, 0], &[0x0004], &[0, 1]);
    let unmasked = parse_sprite_bank(&unmasked_source, &unmasked_region).unwrap();
    assert_eq!(
        unmasked.entries[0].variants[0]
            .decode_rgba(&unmasked_source, SpriteShade::Base)
            .unwrap()[0]
            .alpha,
        255
    );

    let (masked_source, masked_region) = indexed_fixture([0, 0], &[0x0005], &[0, 1]);
    let masked = parse_sprite_bank(&masked_source, &masked_region).unwrap();
    assert_eq!(
        masked.entries[0].variants[0]
            .decode_rgba(&masked_source, SpriteShade::Base)
            .unwrap()[0]
            .alpha,
        0
    );
}

#[test]
fn direct_rgb555_uses_word_width_and_masked_zero() {
    let (source, region) = direct_fixture(0x0003, 0, 12);
    let bank = parse_sprite_bank(&source, &region).unwrap();
    let variant = &bank.entries[0].variants[0];
    assert!(matches!(
        variant.encoding,
        SpriteEncoding::DirectRgb555 { .. }
    ));
    assert_eq!(variant.atlas_origin.x_bytes, 12);
    let decoded = variant.decode_rgba(&source, SpriteShade::Base).unwrap();
    assert_eq!(decoded[0].alpha, 0);
    assert_eq!(
        decoded[1],
        Rgba8 {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        }
    );
}

#[test]
fn selector_and_encoding_invariants_fail_closed() {
    let (source, mut region) = indexed_fixture([1, 0], &[0x0004], &[0]);
    region.requested_count = 1;
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::InvalidModeSelector {
            mode: SpriteRenderMode::Mode0,
            selector: 1,
            variant_count: 1,
            ..
        })
    ));

    let (source, region) = indexed_fixture([0, 0], &[0x0004], &[2]);
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::PixelIndexOutOfRange {
            index: 2,
            color_count: 2,
            ..
        })
    ));

    let (source, region) = direct_fixture(0x0002, 1, 12);
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::DirectColorCount { color_count: 1, .. })
    ));

    let (source, region) = direct_fixture(0x0002, 0, 11);
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::MisalignedDirectAtlasOffset { offset, .. }) if offset & 1 == 1
    ));
}

#[test]
fn absent_mode_selectors_follow_the_loader_count_rule() {
    let (source, region) = indexed_fixture([u8::MAX, 0], &[0x0004], &[1]);
    let bank = parse_sprite_bank(&source, &region).unwrap();
    let entry = &bank.entries[0];
    assert!(entry.variant(SpriteRenderMode::Mode0).is_none());
    assert!(entry.variant(SpriteRenderMode::Mode1).is_some());

    let (source, region) = indexed_fixture([0, u8::MAX], &[0x0004], &[1]);
    let bank = parse_sprite_bank(&source, &region).unwrap();
    let entry = &bank.entries[0];
    assert!(entry.variant(SpriteRenderMode::Mode0).is_some());
    assert!(entry.variant(SpriteRenderMode::Mode1).is_none());

    let source = vec![u8::MAX, u8::MAX, 0, 0];
    let region = sprite_region(source.len(), 1, Vec::new(), source.len());
    let bank = parse_sprite_bank(&source, &region).unwrap();
    assert!(bank.entries[0].variants.is_empty());
    assert!(bank.entries[0].variant(SpriteRenderMode::Mode0).is_none());
    assert!(bank.entries[0].variant(SpriteRenderMode::Mode1).is_none());
}

#[test]
fn descriptor_palette_and_atlas_corruption_fail_closed() {
    let (mut source, region) = indexed_fixture([0, 0], &[0x0004], &[1]);
    put_u32(&mut source, 4 + 0x14, 1);
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::NonZeroRuntimeSlots { .. })
    ));

    let (mut source, region) = indexed_fixture([0, 0], &[0x0004], &[1]);
    put_u32(&mut source, 4 + 0x0c, u32::MAX);
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::PaletteOutOfBounds { .. })
    ));

    let (source, mut region) = indexed_fixture([0, 0], &[0x0004], &[1]);
    let HandlerFraming::SpriteAtlas { records, .. } = &mut region.framing else {
        unreachable!();
    };
    records[0].x_end += 1;
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::AtlasDimensionsMismatch { .. })
    ));

    let (mut source, region) = indexed_fixture([0, 0], &[0x0004], &[1]);
    put_u16(&mut source, 4 + 6, 0);
    assert!(matches!(
        parse_sprite_bank(&source, &region),
        Err(SpriteError::IndexedColorCount { color_count: 0, .. })
    ));
}

#[test]
fn shared_palette_requires_a_non_shared_owner() {
    let table_len = 64usize;
    let main_len = table_len + PALETTE_BYTES;
    let mut source = vec![0u8; main_len];
    for entry in 0..2 {
        let start = entry * 32;
        source[start] = 0;
        source[start + 1] = 0;
        write_variant(
            &mut source,
            start + 4,
            VariantSpec {
                sprite_id: 300 + entry as u16,
                flags: if entry == 0 { 0x0004 } else { 0x8004 },
                color_count: COLORS,
                atlas_offset: u32::from(ATLAS_Y) * 2048 + u32::from(ATLAS_X),
                palette_offset: table_len as u32,
                width: 1,
                height: 1,
            },
        );
    }
    write_test_palette(&mut source, table_len);
    let pixel_start = source.len();
    source.push(1);
    let region = sprite_region(
        main_len,
        2,
        vec![AtlasRecord {
            span: ByteSpan::new(pixel_start, pixel_start + 1),
            x_start: ATLAS_X,
            y_start: ATLAS_Y,
            x_end: ATLAS_X + 1,
            y_end: ATLAS_Y + 1,
            pixels: ByteSpan::new(pixel_start, pixel_start + 1),
        }],
        source.len(),
    );
    let bank = parse_sprite_bank(&source, &region).unwrap();
    assert!(!bank.entries[0].variants[0].flags.palette_is_shared());
    assert!(bank.entries[1].variants[0].flags.palette_is_shared());

    let mut ownerless = source.clone();
    put_u16(&mut ownerless, 4 + 4, 0x8004);
    assert!(matches!(
        parse_sprite_bank(&ownerless, &region),
        Err(SpriteError::SharedPaletteWithoutOwner { .. })
    ));
}

#[test]
fn blend_bits_preserve_the_original_d3d_precedence() {
    assert_eq!(
        inf_formats::SpriteFlags::new(0x0004).blend_mode(),
        SpriteBlendMode::OpaqueOrMasked
    );
    assert_eq!(
        inf_formats::SpriteFlags::new(0x000c).blend_mode(),
        SpriteBlendMode::SourceAlpha
    );
    assert_eq!(
        inf_formats::SpriteFlags::new(0x001c).blend_mode(),
        SpriteBlendMode::Additive
    );
    assert_eq!(
        inf_formats::SpriteFlags::new(0x0024).blend_mode(),
        SpriteBlendMode::DarkenByInverseSourceColor
    );
}

#[test]
fn retail_golden_entries_lock_the_recovered_layout() {
    let root = infestation_root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();

    let (zero_bytes, zero) = load_sprite_bank(&root, &preload, "0.ovl");
    assert_eq!(zero.entries.len(), 8);
    assert_eq!(
        zero.entries
            .iter()
            .map(|entry| entry.variants.len())
            .sum::<usize>(),
        8
    );
    let first_record = zero.entries[0].variants[0].atlas_record_index;
    for (index, entry) in zero.entries.iter().enumerate() {
        let variant = &entry.variants[0];
        assert_eq!(variant.sprite_id, index as u16);
        assert_eq!(variant.flags.raw(), 0x0014);
        assert_eq!(variant.color_count, 62);
        assert_eq!((variant.width, variant.height), (32, 32));
        assert_eq!(variant.atlas_record_index, first_record);
        assert_eq!(variant.palette_offset, 0x100 + index as u32 * 0x0ffc);
        assert_eq!(
            variant
                .decode_rgba(&zero_bytes, SpriteShade::Base)
                .unwrap()
                .len(),
            1024
        );
    }

    let (_, d0) = load_sprite_bank(&root, &preload, "0d0.ovl");
    assert_eq!(d0.entries.len(), 177);
    assert_eq!(
        d0.entries
            .iter()
            .map(|entry| entry.variants.len())
            .sum::<usize>(),
        350
    );
    assert_eq!(
        d0.entries
            .iter()
            .filter(|entry| entry.variants.len() == 2)
            .count(),
        173
    );
    for entry in d0.entries.iter().filter(|entry| entry.variants.len() == 2) {
        assert_eq!([entry.selectors.mode0, entry.selectors.mode1], [0, 1]);
        assert_eq!(entry.variants[0].flags.raw(), 0x0014);
        assert_eq!(entry.variants[1].flags.raw(), 0x0024);
        assert_eq!(entry.variants[0].sprite_id, entry.variants[1].sprite_id);
        assert_eq!(
            entry.variants[0].atlas_offset_bytes,
            entry.variants[1].atlas_offset_bytes
        );
        assert_eq!(
            (entry.variants[0].width, entry.variants[0].height),
            (entry.variants[1].width, entry.variants[1].height)
        );
    }
    let direct = d0.entries[0]
        .variants
        .iter()
        .find(|variant| matches!(variant.encoding, SpriteEncoding::DirectRgb555 { .. }))
        .unwrap();
    assert_eq!((direct.width, direct.height), (64, 63));

    let (_, one_d0) = load_sprite_bank(&root, &preload, "1d0.ovl");
    let large_direct = &one_d0.entries[200].variants[0];
    assert!(matches!(
        large_direct.encoding,
        SpriteEncoding::DirectRgb555 { .. }
    ));
    assert_eq!((large_direct.width, large_direct.height), (704, 1344));
    let record = &one_d0.atlas_records[large_direct.atlas_record_index];
    assert_eq!(record.x_end - record.x_start, 1408);
}

#[test]
fn retail_and_demo_sprite_corpora_have_exact_semantic_totals() {
    assert_eq!(
        collect_sprite_stats(&infestation_root()),
        SpriteStats {
            files: 86,
            logical_entries: 9_832,
            physical_variants: 10_697,
            indexed_variants: 10_613,
            direct_variants: 84,
            shared_palettes: 1_150,
            single_variant_entries: 8_967,
            dual_variant_entries: 865,
            atlas_records: 8_419,
            unreferenced_records: 3,
            orphan_entries: 3,
        }
    );
    assert_eq!(
        collect_sprite_stats(&infestation_root().join("infestation_demo")),
        SpriteStats {
            files: 11,
            logical_entries: 1_657,
            physical_variants: 2_003,
            indexed_variants: 1_984,
            direct_variants: 19,
            shared_palettes: 168,
            single_variant_entries: 1_311,
            dual_variant_entries: 346,
            atlas_records: 1_348,
            unreferenced_records: 1,
            orphan_entries: 1,
        }
    );
}

#[test]
fn retail_9d_variants_expose_the_entry_ignored_by_the_loader_count() {
    let root = infestation_root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    for name in ["9d2.ovl", "9d3.ovl", "9d4.ovl"] {
        let (_, bank) = load_sprite_bank(&root, &preload, name);
        assert_eq!(bank.entries.len(), 92, "{name}");
        assert_eq!(bank.unreferenced_record_indices.len(), 1, "{name}");
        assert_eq!(bank.orphan_entries.len(), 1, "{name}");
        let orphan = &bank.orphan_entries[0];
        assert_eq!(orphan.variants.len(), 1, "{name}");
        let variant = &orphan.variants[0];
        assert_eq!(variant.sprite_id, 1445, "{name}");
        assert_eq!(variant.flags.raw(), 0x0014, "{name}");
        assert_eq!(variant.color_count, 256, "{name}");
        assert_eq!(variant.atlas_offset_bytes, 0x0041_6ee0, "{name}");
        assert_eq!(variant.palette_offset, 0x0012_cd20, "{name}");
        assert_eq!((variant.width, variant.height), (128, 128), "{name}");
        assert_eq!(
            variant.atlas_record_index, bank.unreferenced_record_indices[0],
            "{name}"
        );
        let SpriteEncoding::Indexed { palette, .. } = variant.encoding else {
            panic!("{name} orphan should be indexed");
        };
        assert_eq!(palette.span.end, bank.main_payload.end, "{name}");
    }
}

fn load_sprite_bank(
    root: &Path,
    preload: &Preload,
    name: &str,
) -> (Vec<u8>, inf_formats::SpriteBank) {
    let key = OverlayKey::parse_filename(name).unwrap();
    let requests = preload
        .leaf(key.level)
        .unwrap()
        .requests(key.request_kind());
    let bytes = fs::read(root.join("Overlay").join(name)).unwrap();
    let layout = parse_overlay(&bytes, requests).unwrap();
    let region = layout
        .regions
        .iter()
        .find(|region| region.handler == HandlerId::ATLAS)
        .unwrap();
    let bank = parse_sprite_bank(&bytes, region).unwrap();
    (bytes, bank)
}

#[derive(Debug, PartialEq, Eq)]
struct SpriteStats {
    files: usize,
    logical_entries: usize,
    physical_variants: usize,
    indexed_variants: usize,
    direct_variants: usize,
    shared_palettes: usize,
    single_variant_entries: usize,
    dual_variant_entries: usize,
    atlas_records: usize,
    unreferenced_records: usize,
    orphan_entries: usize,
}

fn collect_sprite_stats(root: &Path) -> SpriteStats {
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut stats = SpriteStats {
        files: 0,
        logical_entries: 0,
        physical_variants: 0,
        indexed_variants: 0,
        direct_variants: 0,
        shared_palettes: 0,
        single_variant_entries: 0,
        dual_variant_entries: 0,
        atlas_records: 0,
        unreferenced_records: 0,
        orphan_entries: 0,
    };

    for entry in fs::read_dir(root.join("Overlay")).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let key = OverlayKey::parse_filename(name).unwrap();
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        if requests.get(3).unwrap().count == 0 {
            continue;
        }
        let bytes = fs::read(&path).unwrap();
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler == HandlerId::ATLAS)
            .unwrap();
        let bank = parse_sprite_bank(&bytes, region)
            .unwrap_or_else(|error| panic!("failed to decode {name}: {error}"));
        stats.files += 1;
        stats.logical_entries += bank.entries.len();
        stats.atlas_records += bank.atlas_records.len();
        stats.unreferenced_records += bank.unreferenced_record_indices.len();
        stats.orphan_entries += bank.orphan_entries.len();
        for sprite in &bank.entries {
            match sprite.variants.len() {
                1 => stats.single_variant_entries += 1,
                2 => stats.dual_variant_entries += 1,
                count => panic!("unexpected {count}-variant entry in {name}"),
            }
            assert_eq!(
                sprite.selectors.mode0, 0,
                "unexpected mode-0 selector in {name}"
            );
            assert_eq!(
                sprite.selectors.mode1,
                (sprite.variants.len() - 1) as u8,
                "unexpected mode-1 selector in {name}"
            );
            for variant in &sprite.variants {
                stats.physical_variants += 1;
                if variant.flags.palette_is_shared() {
                    stats.shared_palettes += 1;
                }
                match variant.encoding {
                    SpriteEncoding::Indexed { .. } => stats.indexed_variants += 1,
                    SpriteEncoding::DirectRgb555 { .. } => stats.direct_variants += 1,
                }
            }
        }
    }
    stats
}
