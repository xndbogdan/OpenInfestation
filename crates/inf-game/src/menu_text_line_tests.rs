use super::{tests::*, *};

fn line_request<'a>(font: &'a Font, text: &'a [u8]) -> LineTextRequest<'a> {
    LineTextRequest {
        text,
        font,
        position_pixels: [10, 20],
        color: MenuColor::from_rgb([200, 100, 50]),
        base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
        context_origin_pixels: [123_456, -789_012],
    }
}

#[test]
fn spaces_and_hyphens_stay_on_one_line_without_lookahead() {
    let r = Resources::new(b"unused H0");
    let mut q = line_request(&r.font, b"A BB-C\0ignored");
    let mut host = RecordingBackend::default();
    draw_line_text(&mut q, &r, &mut host).unwrap();
    // Six glyphs of (600 - 100) / 100 pixels; Y never moves.
    assert_eq!(q.position_pixels, [40, 20]);
    assert_eq!(host.quads.len(), 12);
    for (index, pair) in host.quads.chunks(2).enumerate() {
        let x = 10 + 5 * index as i16;
        assert_eq!(
            pair[0].corners,
            [[x, 11], [x + 5, 11], [x + 5, 20], [x, 20]]
        );
        assert_eq!(pair[0].pass, MenuGlyphPass::Base);
        assert_eq!(pair[1].pass, MenuGlyphPass::Color);
        assert_eq!(pair[1].color, q.color);
        assert_eq!(pair[0].context_origin_pixels, q.context_origin_pixels);
    }
    // Only the glyph callback runs: no H0, font or shadow style lookups.
    assert!(r.calls.borrow().iter().all(|(kind, _)| *kind == "sprite"));
    assert_eq!(r.calls.borrow().len(), 6);
}

#[test]
fn substitution_maps_high_bytes_to_bang_then_empty_sprites_to_fallback() {
    let mut r = Resources::new(b"");
    r.font.maximum_byte = 90;
    r.font.sprite_words[usize::from(b'C')] = 0;
    let mut q = line_request(&r.font, b"\xC8C");
    q.base_color = MenuColor::from_word(0x1122_3344);
    let mut host = RecordingBackend::default();
    draw_line_text(&mut q, &r, &mut host).unwrap();
    let arguments: Vec<i32> = host.quads.iter().map(|quad| quad.sprite_argument).collect();
    // Sprite words are byte + 1 in this font.
    assert_eq!(arguments, [34, 34, 64, 64]);
    assert_eq!(host.quads[0].color, MenuColor::from_word(0x1122_3344));
}

#[test]
fn glyph_failure_keeps_prior_submissions_and_the_position() {
    let r = Resources::new(b"");
    let mut q = line_request(&r.font, b"ABC");
    let mut host = RecordingBackend {
        fail_at: Some(3),
        ..Default::default()
    };
    assert_eq!(
        draw_line_text(&mut q, &r, &mut host),
        Err(MenuTextError::Backend("backend refused submission"))
    );
    assert_eq!(host.quads.len(), 3);
    assert_eq!(q.position_pixels, [10, 20]);
}

#[test]
fn writeback_narrows_both_quotients_to_signed_words() {
    let r = Resources::new(b"");
    let mut q = line_request(&r.font, b"A");
    q.position_pixels = [32_765, 40_000];
    let mut host = RecordingBackend::default();
    draw_line_text(&mut q, &r, &mut host).unwrap();
    // 004A3AFA/004A3B08 MOVSX: X advances past 32767 and Y is unchanged.
    assert_eq!(
        q.position_pixels,
        [i32::from(32_770u16 as i16), i32::from(40_000u16 as i16)]
    );
}

#[test]
fn y_division_fault_follows_the_published_x() {
    let mut r = Resources::new(b"");
    r.font.coordinate_divisor = -1;
    let mut q = line_request(&r.font, b"");
    // The published X differs from the input only through narrowing, so a
    // writer that skipped X or divided Y first would leave 40000 here.
    q.position_pixels = [40_000, i32::MIN];
    let mut host = RecordingBackend::default();
    assert_eq!(
        draw_line_text(&mut q, &r, &mut host),
        Err(MenuTextError::Layout(MenuTextLayoutError::Division {
            numerator: i32::MIN,
            divisor: -1
        }))
    );
    assert_eq!(q.position_pixels, [i32::from(40_000u16 as i16), i32::MIN]);
}

#[derive(serde::Deserialize)]
struct NativeLineCase {
    mode: u32,
    columns: i32,
    rows: i32,
    divisor: i32,
    maximum_byte: i32,
    fallback: u8,
    metrics: Vec<[i16; 4]>,
    sprites: Vec<u32>,
    text: Vec<u8>,
    dimensions: [u16; 2],
    position: [i32; 2],
    color: u32,
    base_color: u32,
    lookups: Vec<u32>,
    draws: Vec<[u32; 11]>,
    returned: [i32; 2],
}

struct LineResources {
    dimensions: [u16; 2],
    lookups: std::cell::RefCell<Vec<u32>>,
}

impl MenuTextResources for LineResources {
    type Error = std::convert::Infallible;
    fn string(&self, _: i32) -> Result<&[u8], Self::Error> {
        panic!("004A3AB0 receives its text from the caller")
    }
    fn font(&self, _: i32) -> Result<&Font, Self::Error> {
        panic!("004A3AB0 receives its font from the caller")
    }
    fn style(&self, _: i32) -> Result<u32, Self::Error> {
        panic!("004A3AB0 has no shadow pass")
    }
    fn sprite_dimensions(&self, argument: i32) -> Result<[u16; 2], Self::Error> {
        self.lookups.borrow_mut().push(argument as u32);
        Ok(self.dimensions)
    }
}

/// 004A3AB0 executed unmodified with a recording glyph callback and +A4
/// (.tmp/inf-line-text-oracle.py); native reads outside a font's tables are
/// not retained.
#[test]
fn line_text_matches_native_004a3ab0_receipts() {
    let cases: Vec<NativeLineCase> =
        serde_json::from_str(include_str!("../tests/fixtures/line_text_native.json")).unwrap();
    assert_eq!(cases.len(), 48);
    for (index, case) in cases.iter().enumerate() {
        let mut font = Resources::new(b"").font;
        font.mode = match case.mode {
            1 => FontMode::Direct,
            2 => FontMode::PositionGrid {
                columns: case.columns,
                rows: case.rows,
            },
            other => panic!("unexpected mode {other}"),
        };
        font.coordinate_divisor = case.divisor;
        font.maximum_byte = case.maximum_byte;
        font.fallback_byte = case.fallback;
        font.metrics = case
            .metrics
            .iter()
            .map(|m| GlyphMetrics {
                advance: m[0],
                spacing: m[1],
                offset: [m[2], m[3]],
            })
            .collect();
        font.sprite_words = case.sprites.clone();
        let resources = LineResources {
            dimensions: case.dimensions,
            lookups: Default::default(),
        };
        let mut host = RecordingBackend::default();
        let mut request = LineTextRequest {
            text: &case.text,
            font: &font,
            position_pixels: case.position,
            color: MenuColor::from_word(case.color),
            base_color: MenuColor::from_word(case.base_color),
            context_origin_pixels: [0, 0],
        };
        draw_line_text(&mut request, &resources, &mut host).unwrap();
        assert_eq!(
            request.position_pixels, case.returned,
            "return case {index}"
        );
        assert_eq!(
            *resources.lookups.borrow(),
            case.lookups,
            "lookups case {index}"
        );
        assert_eq!(host.quads.len(), case.draws.len(), "packets case {index}");
        for (packet, (quad, expected)) in host.quads.iter().zip(&case.draws).enumerate() {
            for (vertex, corner) in quad.corners.iter().enumerate() {
                assert_eq!(
                    u32::from(corner[0] as u16) | (u32::from(corner[1] as u16) << 16),
                    expected[vertex * 2],
                    "corner case {index} packet {packet}"
                );
                assert_eq!(expected[vertex * 2 + 1], 0x40);
            }
            assert_eq!(quad.pass as u32, expected[8], "pass case {index}");
            assert_eq!(quad.sprite_argument as u32, case.lookups[packet / 2]);
            let rgb = quad.color.rgb;
            assert_eq!(
                u32::from(rgb[0])
                    | (u32::from(rgb[1]) << 8)
                    | (u32::from(rgb[2]) << 16)
                    | (u32::from(quad.color.high_byte.unwrap()) << 24),
                expected[10],
                "color case {index} packet {packet}"
            );
        }
    }
}
