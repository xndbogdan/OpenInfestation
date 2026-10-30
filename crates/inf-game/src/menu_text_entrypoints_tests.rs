use super::{tests::*, *};

fn raw_request<'a>(font: &'a Font, text: &'a [u8]) -> RawMenuTextRequest<'a> {
    RawMenuTextRequest {
        text,
        font,
        position_pixels: [10, 20],
        width_pixels: 13,
        color: MenuColor::from_rgb([200, 100, 50]),
        base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
        context_origin_pixels: [123_456, -789_012],
        emission: MenuTextEmission::Draw,
        reveal: MenuTextReveal::Complete,
    }
}

#[test]
fn generated_row_uses_bytes_without_h0_and_keeps_early_y_on_failure() {
    let r = Resources::new(b"unused H0");
    let mut q = request(false);
    q.descriptor.extent_pixels[0] = 13;
    q.descriptor.reveal = 0;
    let mut host = RecordingBackend {
        fail_at: Some(1),
        ..Default::default()
    };
    assert_eq!(
        draw_row_text(
            MenuRowTextRequest {
                descriptor: &mut q.descriptor,
                text: b"A BB",
                placement: MenuTextPlacement::Left,
                context_origin_pixels: q.context_origin_pixels,
            },
            &r,
            &mut host,
        ),
        Err(MenuTextError::Backend("backend refused submission"))
    );
    assert_eq!(q.descriptor.position_pixels, [10, 32]);
    assert_eq!(host.quads.len(), 1);
    assert!(!r.calls.borrow().iter().any(|(kind, _)| *kind == "string"));
}

#[test]
fn raw_text_has_no_shadow_alignment_or_resource_font_lookup() {
    let r = Resources::new(b"unused H0");
    let mut q = raw_request(&r.font, b"A BB\0ignored");
    q.base_color = MenuColor::from_word(0x11223344);
    let mut host = RecordingBackend::default();
    draw_raw_text(&mut q, &r, &mut host).unwrap();
    assert_eq!(q.position_pixels, [20, 32]);
    assert_eq!(host.quads.len(), 8);
    assert_eq!(host.quads[0].corners[0], [10, 11]);
    assert_eq!(host.quads[0].color, MenuColor::from_word(0x11223344));
    assert_eq!(host.quads[1].color, q.color);
    assert_eq!(host.quads[0].context_origin_pixels, q.context_origin_pixels);
    assert!(r.calls.borrow().iter().all(|(kind, _)| *kind == "sprite"));
}

#[test]
fn raw_measurement_and_reveal_preserve_complete_layout() {
    let r = Resources::new(b"");
    let mut q = raw_request(&r.font, b"A BB");
    q.emission = MenuTextEmission::Measure;
    let mut host = RecordingBackend::default();
    draw_raw_text(&mut q, &r, &mut host).unwrap();
    assert_eq!(q.position_pixels, [20, 32]);
    assert!(host.quads.is_empty());
    assert!(r.calls.borrow().is_empty());
    q.position_pixels = [10, 20];
    q.emission = MenuTextEmission::Draw;
    q.reveal = MenuTextReveal::Timed { age_millis: 25 };
    draw_raw_text(&mut q, &r, &mut host).unwrap();
    assert_eq!(q.position_pixels, [20, 32]);
    assert_eq!(host.quads.len(), 2);
    assert_eq!(host.quads[1].color.rgb, [206, 103, 51]);
}

#[test]
fn raw_failures_preserve_prior_packets_and_native_coordinate_write_order() {
    let mut r = Resources::new(b"");
    let mut q = raw_request(&r.font, b"AB");
    let mut host = RecordingBackend {
        fail_at: Some(1),
        ..Default::default()
    };
    assert!(matches!(
        draw_raw_text(&mut q, &r, &mut host),
        Err(MenuTextError::Backend(_))
    ));
    assert_eq!(host.quads.len(), 1);
    assert_eq!(q.position_pixels, [10, 20]);

    r.font.coordinate_divisor = -1;
    let mut q = raw_request(&r.font, b"");
    q.position_pixels = [40_000, i32::MIN];
    assert!(matches!(
        draw_raw_text(&mut q, &r, &mut RecordingBackend::default()),
        Err(MenuTextError::Layout(MenuTextLayoutError::Division {
            numerator: i32::MIN,
            divisor: -1
        }))
    ));
    assert_eq!(q.position_pixels, [i32::from(40_000u16 as i16), i32::MIN]);
}

#[derive(serde::Deserialize)]
struct NativeTextCase {
    kind: String,
    grid: bool,
    divisor: i32,
    position: [i32; 2],
    metric: [i16; 4],
    dimensions: [u16; 2],
    color: u32,
    origin: [u32; 2],
    width: i32,
    age: i32,
    text: String,
    flags: u32,
    animated: bool,
    right: bool,
    base_color: u32,
    shadow: u32,
    emit: bool,
    reveal: bool,
    draws: Vec<[u32; 11]>,
    lookups: Vec<i32>,
    returned: Vec<i64>,
}

struct NativeResources {
    font: Font,
    dimensions: [u16; 2],
    shadow: u32,
}

impl MenuTextResources for NativeResources {
    type Error = std::convert::Infallible;
    fn string(&self, _: i32) -> Result<&[u8], Self::Error> {
        panic!("generated text does not resolve H0")
    }
    fn font(&self, argument: i32) -> Result<&Font, Self::Error> {
        assert_eq!(argument, 0);
        Ok(&self.font)
    }
    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        assert_eq!(argument, 7);
        Ok(self.shadow)
    }
    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        Ok(self.dimensions)
    }
}

fn compare_native(cases: &[NativeTextCase]) {
    for (index, case) in cases.iter().enumerate() {
        let mut font = Resources::new(b"").font;
        font.mode = if case.grid {
            FontMode::PositionGrid {
                columns: 2,
                rows: 2,
            }
        } else {
            FontMode::Direct
        };
        font.coordinate_divisor = case.divisor;
        font.metrics.fill(GlyphMetrics {
            advance: case.metric[0],
            spacing: case.metric[1],
            offset: [case.metric[2], case.metric[3]],
        });
        font.sprite_words = (1..=1024).collect();
        let resources = NativeResources {
            font,
            dimensions: case.dimensions,
            shadow: case.shadow,
        };
        let mut host = RecordingBackend::default();
        if case.kind == "raw" {
            let mut q = RawMenuTextRequest {
                text: case.text.as_bytes(),
                font: &resources.font,
                position_pixels: case.position,
                width_pixels: case.width,
                color: MenuColor::from_word(case.color),
                base_color: MenuColor::from_word(case.base_color),
                context_origin_pixels: case.origin.map(|v| v as i32),
                emission: if case.emit {
                    MenuTextEmission::Draw
                } else {
                    MenuTextEmission::Measure
                },
                reveal: if case.reveal {
                    MenuTextReveal::Timed {
                        age_millis: case.age,
                    }
                } else {
                    MenuTextReveal::Complete
                },
            };
            draw_raw_text(&mut q, &resources, &mut host).unwrap();
            assert_eq!(
                q.position_pixels.map(|v| v as u32 as i64).as_slice(),
                case.returned,
                "raw return case {index}"
            );
        } else {
            let mut descriptor = request(false).descriptor;
            descriptor.position_pixels = case.position.map(|v| v as i16);
            descriptor.extent_pixels = [case.width as i16, 200];
            descriptor.reveal = if case.animated { 0 } else { u16::MAX };
            descriptor.flags = case.flags;
            descriptor.age_millis = case.age;
            descriptor.color = MenuColor::from_word(case.color);
            let outcome = draw_row_text(
                MenuRowTextRequest {
                    descriptor: &mut descriptor,
                    text: case.text.as_bytes(),
                    placement: if case.right {
                        MenuTextPlacement::Right
                    } else {
                        MenuTextPlacement::Left
                    },
                    context_origin_pixels: case.origin.map(|v| v as i32),
                },
                &resources,
                &mut host,
            )
            .unwrap();
            let actual = [
                i64::from(descriptor.position_pixels[0]),
                i64::from(descriptor.position_pixels[1]),
                i64::from(descriptor.extent_pixels[0]),
                i64::from(descriptor.extent_pixels[1]),
                i64::from(descriptor.reveal),
                0,
                i64::from(descriptor.flags),
                descriptor.age_millis as u32 as i64,
                i64::from(case.color),
                outcome.advance_from_origin as u32 as i64,
            ];
            assert_eq!(actual.as_slice(), case.returned, "row return case {index}");
        }
        assert_eq!(host.quads.len(), case.draws.len(), "packets case {index}");
        for (packet_index, (quad, expected)) in host.quads.iter().zip(&case.draws).enumerate() {
            for (vertex, corner) in quad.corners.iter().enumerate() {
                assert_eq!(
                    u32::from(corner[0] as u16) | (u32::from(corner[1] as u16) << 16),
                    expected[vertex * 2],
                    "corner case {index} packet {packet_index}"
                );
                assert_eq!(expected[vertex * 2 + 1], 64);
            }
            assert_eq!(quad.pass as u32, expected[8], "pass case {index}");
            let rgb = quad.color.rgb;
            assert_eq!(
                u32::from(rgb[0])
                    | (u32::from(rgb[1]) << 8)
                    | (u32::from(rgb[2]) << 16)
                    | (u32::from(quad.color.high_byte.unwrap()) << 24),
                expected[10],
                "color case {index} packet {packet_index}"
            );
            assert_eq!(
                quad.sprite_argument,
                case.lookups[packet_index / 2],
                "H3 case {index}"
            );
        }
    }
}

#[test]
fn text_entrypoints_match_original_consumer_receipts() {
    compare_native(
        &serde_json::from_str::<Vec<NativeTextCase>>(include_str!(
            "../tests/fixtures/menu_text_entrypoints_native.json"
        ))
        .unwrap(),
    );
}

#[test]
#[ignore = "requires local original-PE oracle output"]
fn text_entrypoints_match_all_1024_original_consumer_receipts() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/inf-text-entrypoints-native.json");
    let cases: Vec<NativeTextCase> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 1024);
    compare_native(&cases);
}
