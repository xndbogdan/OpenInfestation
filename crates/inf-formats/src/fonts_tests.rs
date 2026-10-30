use super::*;
use crate::{parse_overlay, HandlerId, OverlayKey, Preload};
use std::{fs, path::Path};

fn region(len: usize, count: u32) -> HandlerRegion {
    HandlerRegion {
        handler: HandlerId::new(4).unwrap(),
        requested_count: count,
        span: ByteSpan::new(0, len),
        framing: HandlerFraming::Standard {
            payload: ByteSpan::new(0, len),
        },
    }
}

fn direct_font() -> Vec<u8> {
    let mut bytes = vec![0; 0x40 + 3 * 8 + 3 * 4];
    for (word, value) in [
        (0, 0xabcdef01),
        (1, 1),
        (2, 3),
        (4, 100),
        (5, 1400),
        (6, 200),
        (10, 2),
        (11, 1),
        (14, 0xfffffff0),
        (15, 0xcafebabe),
    ] {
        bytes[word * 4..word * 4 + 4].copy_from_slice(&u32::to_le_bytes(value));
    }
    for (i, words) in [[100i16, 20, -2, 3], [500, -10, 4, -5], [200, 30, -6, -7]]
        .into_iter()
        .enumerate()
    {
        for (j, word) in words.into_iter().enumerate() {
            bytes[0x40 + i * 8 + j * 2..0x40 + i * 8 + j * 2 + 2]
                .copy_from_slice(&word.to_le_bytes());
        }
    }
    for (i, word) in [0u32, 101, 0xffff_ffff].into_iter().enumerate() {
        bytes[0x58 + i * 4..0x5c + i * 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[test]
fn font_parser_keeps_signed_metrics_and_ignores_disk_pointer_placeholders() {
    let bytes = direct_font();
    let bank = parse_font_bank(&bytes, &region(bytes.len(), 1)).unwrap();
    let font = &bank.fonts[0];
    assert_eq!(font.raw_header[0], 0xabcdef01);
    assert_eq!(font.raw_header[14..], [0xfffffff0, 0xcafebabe]);
    assert_eq!(
        font.metrics[1],
        GlyphMetrics {
            advance: 500,
            spacing: -10,
            offset: [4, -5]
        }
    );
    assert_eq!(font.sprite_words, [0, 101, 0xffff_ffff]);
    assert_eq!(font.metric_span, ByteSpan::new(0x40, 0x58));
    assert_eq!(font.sprite_span, ByteSpan::new(0x58, 0x64));
    assert_eq!(bank.trailing.len(), 0);
}

#[test]
fn font_parser_rejects_unbounded_counts_zero_divisor_and_unknown_modes() {
    for (word, value) in [(1, 3u32), (2, u32::MAX), (2, 0x7fffffff), (4, 0)] {
        let mut bytes = direct_font();
        bytes[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
        assert!(parse_font_bank(&bytes, &region(bytes.len(), 1)).is_err());
    }
    let bytes = direct_font();
    assert!(parse_font_bank(&bytes, &region(bytes.len(), 2)).is_err());
    assert!(parse_font_bank(&bytes[..bytes.len() - 1], &region(bytes.len() - 1, 1)).is_err());
}

#[test]
fn position_grid_sizes_all_sprite_planes_and_retains_trailing_bytes() {
    let mut bytes = direct_font();
    bytes[4..8].copy_from_slice(&2u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&2u32.to_le_bytes());
    bytes[0x34..0x38].copy_from_slice(&3u32.to_le_bytes());
    bytes.resize(0x58 + 3 * 2 * 3 * 4 + 7, 0xcc);
    let bank = parse_font_bank(&bytes, &region(bytes.len(), 1)).unwrap();
    assert_eq!(
        bank.fonts[0].mode,
        FontMode::PositionGrid {
            columns: 2,
            rows: 3
        }
    );
    assert_eq!(bank.fonts[0].sprite_words.len(), 18);
    assert_eq!(bank.trailing.len(), 7);
}

#[test]
fn multiple_fonts_follow_one_header_table_and_per_font_metrics_then_sprites() {
    let one = direct_font();
    let mut bytes = vec![0xee; 12];
    bytes.extend_from_slice(&one[..64]);
    bytes.extend_from_slice(&one[..64]);
    bytes.extend_from_slice(&one[64..]);
    bytes.extend_from_slice(&one[64..]);
    let mut region = region(bytes.len(), 2);
    region.framing = HandlerFraming::Standard {
        payload: ByteSpan::new(12, bytes.len()),
    };
    let bank = parse_font_bank(&bytes, &region).unwrap();
    assert_eq!(bank.header_table, ByteSpan::new(12, 140));
    assert_eq!(bank.fonts[0].metric_span, ByteSpan::new(140, 164));
    assert_eq!(bank.fonts[0].sprite_span, ByteSpan::new(164, 176));
    assert_eq!(bank.fonts[1].metric_span, ByteSpan::new(176, 200));
    assert_eq!(bank.fonts[1].sprite_span, ByteSpan::new(200, 212));
    assert_eq!(bank.fonts[0].metrics, bank.fonts[1].metrics);
    assert_eq!(bank.fonts[0].sprite_words, bank.fonts[1].sprite_words);
}

#[test]
fn all_five_retail_font_regions_decode_from_original_framing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut records = Vec::new();
    for entry in fs::read_dir(root.join("Overlay")).unwrap() {
        let path = entry.unwrap().path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(key) = OverlayKey::parse_filename(name) else {
            continue;
        };
        let source = fs::read(&path).unwrap();
        let layout = parse_overlay(
            &source,
            preload
                .leaf(key.level)
                .unwrap()
                .requests(key.request_kind()),
        )
        .unwrap();
        for region in layout.regions.iter().filter(|r| r.handler.get() == 4) {
            let bank = parse_font_bank(&source, region).unwrap();
            assert_eq!(bank.fonts.len(), 1);
            assert_eq!(bank.trailing.len(), 0);
            let f = &bank.fonts[0];
            assert_eq!(
                [0, 32, 45, 127].map(|i| f.sprite_words[i]),
                [112, 17, 30, 112]
            );
            records.push((
                name.to_owned(),
                f.mode,
                f.metrics.len(),
                f.coordinate_divisor,
                f.line_step_terms,
                f.maximum_byte,
                f.fallback_byte,
                f.sprite_words.len(),
            ));
        }
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        records,
        [
            (
                "0d0.ovl".to_owned(),
                FontMode::Direct,
                128,
                400,
                [6477, 1216],
                127,
                127,
                128
            ),
            (
                "0d1.ovl".to_owned(),
                FontMode::Direct,
                256,
                500,
                [6477, 1216],
                255,
                127,
                256
            ),
            (
                "0d2.ovl".to_owned(),
                FontMode::Direct,
                256,
                400,
                [6477, 1216],
                255,
                127,
                256
            ),
            (
                "0d3.ovl".to_owned(),
                FontMode::Direct,
                256,
                400,
                [6477, 1216],
                255,
                127,
                256
            ),
            (
                "0d4.ovl".to_owned(),
                FontMode::Direct,
                256,
                400,
                [6477, 1216],
                255,
                127,
                256
            ),
        ]
    );
}
