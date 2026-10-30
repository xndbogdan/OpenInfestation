use super::*;
use inf_formats::{FontMode, HandlerFraming, HandlerId, HandlerRegion};

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn scratch(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp")
        .join(format!("inf-font-{label}-{}-{nonce}", std::process::id()))
}

#[test]
fn retail_export_preserves_every_font_field_source_byte_and_global_slot() {
    let root = data_root();
    let output = scratch("retail");
    let first = export(&root, &output).unwrap();
    assert_eq!(
        first,
        FontExportReport {
            sources: 5,
            fonts: 5,
            glyph_metrics: 1152,
            sprite_arguments: 1152,
            payload_bytes: 14_144,
            framed_bytes: 14_164,
        }
    );
    let manifest_path = output.join("fonts/manifest.json");
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let sources = manifest["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 5);
    for (difficulty, source) in sources.iter().enumerate() {
        let name = source["ovl"].as_str().unwrap();
        assert_eq!(name, format!("0d{difficulty}.ovl"));
        let bytes = fs::read(root.join("Overlay").join(name)).unwrap();
        let key = OverlayKey::parse_filename(name).unwrap();
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 4)
            .unwrap();
        let bank = parse_font_bank(&bytes, region).unwrap();
        let raw = fs::read(
            output
                .join("fonts")
                .join(source["raw_framed_region"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(raw, region.span.get(&bytes).unwrap());
        assert_eq!(
            source["framed_region"],
            serde_json::to_value(region.span).unwrap()
        );
        assert_eq!(
            source["payload"],
            serde_json::to_value(bank.payload).unwrap()
        );
        assert_eq!(
            source["header_table"],
            serde_json::to_value(bank.header_table).unwrap()
        );
        assert_eq!(
            source["trailing"],
            serde_json::to_value(bank.trailing).unwrap()
        );
        assert_eq!(
            source["request"],
            serde_json::to_value(requests.get(4).unwrap()).unwrap()
        );
        let record = &source["fonts"][0];
        assert_eq!(record["global_slot"], 0);
        assert_eq!(record["local_index"], 0);
        assert_eq!(record["mode"], "Direct");
        assert_eq!(record["line_step_terms"], serde_json::json!([6477, 1216]));
        assert_eq!(record["fallback_byte"], 127);
        let mut font_value = record.clone();
        font_value.as_object_mut().unwrap().remove("global_slot");
        assert_eq!(font_value, serde_json::to_value(&bank.fonts[0]).unwrap());
        for (i, word) in bank.fonts[0].raw_header.iter().enumerate() {
            let offset = bank.fonts[0].header_span.start - region.span.start + i * 4;
            assert_eq!(
                *word,
                u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
            );
        }
    }
    assert_eq!(export(&root, &output).unwrap(), first);
    assert_eq!(fs::read(&manifest_path).unwrap(), manifest_bytes);
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn serialized_grid_retains_signed_metrics_full_sprite_arguments_and_placeholders() {
    let mut bytes = vec![0; 64 + 8 + 6 * 4 + 3];
    for (index, value) in [
        (1, 2u32),
        (2, 1),
        (4, 100),
        (10, 0),
        (12, 2),
        (13, 3),
        (14, 0x1234_5678),
        (15, 0xffff_ffff),
    ] {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (index, value) in [-12i16, -5, -7, 9].into_iter().enumerate() {
        bytes[64 + index * 2..66 + index * 2].copy_from_slice(&value.to_le_bytes());
    }
    for (index, value) in [0x1234_0070u32, 0xffff_ffff, 3, 4, 5, 6]
        .into_iter()
        .enumerate()
    {
        bytes[72 + index * 4..76 + index * 4].copy_from_slice(&value.to_le_bytes());
    }
    let region = HandlerRegion {
        handler: HandlerId::new(4).unwrap(),
        requested_count: 1,
        span: ByteSpan::new(0, bytes.len()),
        framing: HandlerFraming::Standard {
            payload: ByteSpan::new(0, bytes.len()),
        },
    };
    let mut bank = parse_font_bank(&bytes, &region).unwrap();
    assert_eq!(bank.trailing.len(), 3);
    let record = FontRecord {
        global_slot: 31,
        font: bank.fonts.remove(0),
    };
    assert_eq!(
        record.font.mode,
        FontMode::PositionGrid {
            columns: 2,
            rows: 3
        }
    );
    let json = serde_json::to_value(record).unwrap();
    assert_eq!(json["global_slot"], 31);
    assert_eq!(
        json["mode"],
        serde_json::json!({"PositionGrid": {"columns": 2, "rows": 3}})
    );
    assert_eq!(
        json["metrics"][0],
        serde_json::json!({"advance": -12, "spacing": -5, "offset": [-7, 9]})
    );
    assert_eq!(
        json["sprite_words"],
        serde_json::json!([0x1234_0070u32, 0xffff_ffffu32, 3, 4, 5, 6])
    );
    assert_eq!(json["raw_header"][14], 0x1234_5678u32);
    assert_eq!(json["raw_header"][15], u32::MAX);
}

#[test]
fn failed_font_decode_leaves_published_family_intact() {
    let root = data_root();
    let scratch = scratch("failure");
    let output = scratch.join("output");
    let fixture = scratch.join("fixture");
    export(&root, &output).unwrap();
    let manifest_path = output.join("fonts/manifest.json");
    let before = fs::read(&manifest_path).unwrap();
    let source_bytes = fs::read(output.join("fonts/sources/0d4.bin")).unwrap();
    fs::create_dir_all(fixture.join("Overlay")).unwrap();
    fs::copy(root.join("PreLoad.dat"), fixture.join("PreLoad.dat")).unwrap();
    for difficulty in 0..5 {
        let name = format!("0d{difficulty}.ovl");
        fs::copy(
            root.join("Overlay").join(&name),
            fixture.join("Overlay").join(name),
        )
        .unwrap();
    }
    let manifest: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let divisor_offset = manifest["sources"][4]["fonts"][0]["header_span"]["start"]
        .as_u64()
        .unwrap() as usize
        + 16;
    let broken_path = fixture.join("Overlay/0d4.ovl");
    let mut broken = fs::read(&broken_path).unwrap();
    broken[divisor_offset..divisor_offset + 4].fill(0);
    fs::write(&broken_path, broken).unwrap();
    assert!(matches!(
        export(&fixture, &output),
        Err(FontExportError::Fonts {
            source: FontError::ZeroDivisor { font: 0 },
            ..
        })
    ));
    assert_eq!(fs::read(&manifest_path).unwrap(), before);
    assert_eq!(
        fs::read(output.join("fonts/sources/0d4.bin")).unwrap(),
        source_bytes
    );
    assert_eq!(
        fs::read_dir(&output).unwrap().count(),
        1,
        "failed staging cleaned up"
    );
    fs::remove_dir_all(scratch).unwrap();
}
