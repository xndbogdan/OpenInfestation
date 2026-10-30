use std::{collections::BTreeMap, path::Path};

use inf_formats::{ByteSpan, FontMode, GlyphMetrics};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    menu_text::{draw_line_text, draw_raw_text, MenuGlyphQuad, MenuTextHost},
};

#[derive(serde::Deserialize)]
struct Fixture {
    strings: BTreeMap<i32, Vec<u8>>,
    styles: BTreeMap<i32, u32>,
    base_color: u32,
    texts: Vec<Vec<u8>>,
    cases: Vec<NativeCase>,
}

#[derive(serde::Deserialize)]
struct NativeCase {
    age: i32,
    completion: i32,
    feedback: i32,
    error: bool,
    selection: i32,
    row_anim: i32,
    name: Vec<u8>,
    surface: [i32; 2],
    focal: [i32; 2],
    events: Vec<Value>,
}

fn fixture() -> Fixture {
    let mut fixture: Fixture = serde_json::from_str(include_str!(
        "../../tests/fixtures/name_entry_draw_native.json"
    ))
    .unwrap();
    for case in &mut fixture.cases {
        for event in &mut case.events {
            if event[0] == "line" || event[0] == "raw" {
                event[1] = json!(fixture.texts[event[1].as_u64().unwrap() as usize]);
            }
        }
    }
    fixture
}

fn sine() -> [u16; 4096] {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    FrontendAssets::open(&root).unwrap().sine_table().values
}

fn font() -> Font {
    Font {
        local_index: 0,
        header_span: ByteSpan::new(0, 0),
        raw_header: [0; 16],
        mode: FontMode::Direct,
        coordinate_divisor: 100,
        line_step_terms: [1100, 900],
        maximum_byte: 255,
        fallback_byte: 63,
        metric_span: ByteSpan::new(0, 0),
        sprite_span: ByteSpan::new(0, 0),
        metrics: (0..256)
            .map(|byte| {
                if byte == 0 {
                    GlyphMetrics {
                        advance: 0,
                        spacing: 0,
                        offset: [0, 0],
                    }
                } else {
                    GlyphMetrics {
                        advance: 450 + (byte % 7) * 17,
                        spacing: -31 + (byte % 3) * 9,
                        offset: [0, -800],
                    }
                }
            })
            .collect(),
        sprite_words: (1..=256).collect(),
    }
}

struct Resources<'a> {
    fixture: &'a Fixture,
    font: Font,
    fail_style: Option<i32>,
}

impl MenuTextResources for Resources<'_> {
    type Error = &'static str;

    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.fixture
            .strings
            .get(&argument)
            .map(Vec::as_slice)
            .ok_or("missing string")
    }

    fn font(&self, argument: i32) -> Result<&Font, Self::Error> {
        assert_eq!(argument, 0);
        Ok(&self.font)
    }

    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        if self.fail_style == Some(argument) {
            return Err("style failure");
        }
        self.fixture
            .styles
            .get(&argument)
            .copied()
            .ok_or("missing style")
    }

    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        Ok([7, 9])
    }
}

fn word(color: MenuColor) -> u32 {
    let [r, g, b] = color.rgb;
    u32::from_le_bytes([r, g, b, color.high_byte.unwrap()])
}

struct GlyphSink;
impl MenuTextHost for GlyphSink {
    type Error = &'static str;
    fn submit_glyph(&mut self, _: MenuGlyphQuad) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct Host<'a> {
    resources: &'a Resources<'a>,
    events: Vec<Value>,
    stars: u32,
    star_hash: u64,
    fail_star: Option<u32>,
}

impl<'a> Host<'a> {
    fn new(resources: &'a Resources<'a>) -> Self {
        Self {
            resources,
            events: Vec::new(),
            stars: 0,
            star_hash: 0xcbf2_9ce4_8422_2325,
            fail_star: None,
        }
    }
}

impl FrontendSceneHost for Host<'_> {
    type Error = &'static str;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        assert!(matches!(command, SceneCommand::FlushBatch), "{command:?}");
        self.events
            .push(json!(["stars", self.stars, self.star_hash]));
        self.events.push(json!(["flush"]));
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        panic!("name entry resolves its own highlight style")
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        panic!("draw receives the surface dimensions")
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        panic!("highlight dimensions come from EN text measurement")
    }
}

impl NameEntryDrawHost for Host<'_> {
    fn star(&mut self, quad: StarQuad) -> Result<(), Self::Error> {
        if self.fail_star == Some(self.stars) {
            return Err("star failure");
        }
        assert!(self.events.is_empty(), "all direct stars precede the flush");
        assert!((37..=45).contains(&quad.color_argument));
        let mut packet = Vec::with_capacity(10);
        for [x, y] in quad.corners {
            packet.push(u32::from(x as u16) | (u32::from(y as u16) << 16));
            packet.push(0x7fff_ffff);
        }
        packet.extend([0, 0xa000_0000 | quad.color_argument as u32]);
        // FNV-1a over every byte of all 1000 native +84 packets. This keeps
        // full-stream coverage without retaining 32000 near-identical quads.
        for byte in packet.into_iter().flat_map(u32::to_le_bytes) {
            self.star_hash = (self.star_hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
        self.stars += 1;
        Ok(())
    }

    fn highlight(&mut self, quad: FooterQuad) -> Result<(), Self::Error> {
        assert_eq!(quad.sprite_slot, 7);
        let mut event = vec![json!("highlight")];
        for [x, y] in quad.corners {
            event.push(json!(u32::from(x as u16) | (u32::from(y as u16) << 16)));
            event.push(json!(quad.vertex_attribute));
        }
        event.extend([json!(0), json!(quad.style_word)]);
        self.events.push(Value::Array(event));
        Ok(())
    }

    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error> {
        assert_eq!(word(request.base_color), self.resources.fixture.base_color);
        assert_eq!(request.context_origin_pixels, [12345, -54321]);
        let mut event = vec![
            json!("line"),
            json!(c_string(request.text)),
            json!(request.position_pixels[0]),
            json!(request.position_pixels[1]),
            json!(word(request.color)),
        ];
        draw_line_text(request, self.resources, &mut GlyphSink).unwrap();
        event.extend(request.position_pixels.map(|v| json!(v)));
        self.events.push(Value::Array(event));
        Ok(())
    }

    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        assert_eq!(word(request.base_color), self.resources.fixture.base_color);
        assert_eq!(request.context_origin_pixels, [12345, -54321]);
        assert_eq!(request.emission, MenuTextEmission::Draw);
        let MenuTextReveal::Timed { age_millis } = request.reveal else {
            panic!("native raw text always reveals")
        };
        let mut event = vec![
            json!("raw"),
            json!(c_string(request.text)),
            json!(request.position_pixels[0]),
            json!(request.position_pixels[1]),
            json!(word(request.color)),
            json!(request.width_pixels),
            json!(2),
            json!(age_millis),
        ];
        draw_raw_text(request, self.resources, &mut GlyphSink).unwrap();
        event.extend(request.position_pixels.map(|v| json!(v)));
        self.events.push(Value::Array(event));
        Ok(())
    }
}

fn frame<'a>(
    case: &'a NativeCase,
    fixture: &Fixture,
    sine: &'a [u16; 4096],
) -> NameEntryDrawFrame<'a> {
    NameEntryDrawFrame {
        state: NameEntryDrawState {
            name: &case.name,
            selection: case.selection,
            row_anim: case.row_anim,
            completion_delay: case.completion,
            feedback: case.feedback,
            error: case.error,
        },
        age_millis: case.age,
        surface_pixels: case.surface,
        focal: case.focal,
        context_origin_pixels: [12345, -54321],
        base_color: MenuColor::from_word(fixture.base_color),
        sine_quarter: sine,
    }
}

/// 00459310, 004A3AB0, 004A3B20 and 004A4240 execute unmodified PE code
/// in .tmp/inf-name-entry-draw-oracle.py. OS/render calls, the independently
/// covered backdrop, glyph resources/rendering and sprintf are stubbed.
/// Text input AND native returned cursor coordinates are checked per call.
#[test]
fn complete_overlay_matches_native_00459310_receipts() {
    let fixture = fixture();
    let sine = sine();
    assert_eq!(fixture.cases.len(), 32);
    let resources = Resources {
        fixture: &fixture,
        font: font(),
        fail_style: None,
    };
    for (index, case) in fixture.cases.iter().enumerate() {
        let mut host = Host::new(&resources);
        draw_name_entry_overlay(frame(case, &fixture, &sine), &resources, &mut host).unwrap();
        assert_eq!(
            host.events.len(),
            case.events.len(),
            "event count case {index}"
        );
        for (event, (actual, expected)) in host.events.iter().zip(&case.events).enumerate() {
            assert_eq!(actual, expected, "case {index}, event {event}");
        }
    }
}

#[test]
fn direct_star_failure_stops_before_flush_and_text() {
    let fixture = fixture();
    let sine = sine();
    let resources = Resources {
        fixture: &fixture,
        font: font(),
        fail_style: None,
    };
    let mut host = Host::new(&resources);
    host.fail_star = Some(17);
    assert!(matches!(
        draw_name_entry_overlay(
            frame(&fixture.cases[0], &fixture, &sine),
            &resources,
            &mut host
        ),
        Err(NameEntryDrawError::Host("star failure"))
    ));
    assert_eq!(host.stars, 17);
    assert!(host.events.is_empty());
}

#[test]
fn ring_resource_failure_preserves_all_stars_and_backdrop_flush() {
    let fixture = fixture();
    let sine = sine();
    let resources = Resources {
        fixture: &fixture,
        font: font(),
        fail_style: Some(7),
    };
    let mut host = Host::new(&resources);
    assert!(matches!(
        draw_name_entry_overlay(
            frame(&fixture.cases[0], &fixture, &sine),
            &resources,
            &mut host
        ),
        Err(NameEntryDrawError::Resource("style failure"))
    ));
    assert_eq!(host.stars, 1000);
    assert_eq!(host.events, fixture.cases[0].events[..2]);
}

#[test]
fn invalid_selection_and_zero_surface_are_contained_before_drawing() {
    let fixture = fixture();
    let sine = sine();
    let resources = Resources {
        fixture: &fixture,
        font: font(),
        fail_style: None,
    };
    let mut input = frame(&fixture.cases[0], &fixture, &sine);
    let mut host = Host::new(&resources);
    input.state.selection = 78;
    assert!(matches!(
        draw_name_entry_overlay(input, &resources, &mut host),
        Err(NameEntryDrawError::State(NameEntryError::Selection(78)))
    ));
    input.state.selection = 0;
    input.surface_pixels[0] = 0;
    assert!(matches!(
        draw_name_entry_overlay(input, &resources, &mut host),
        Err(NameEntryDrawError::Layout(MenuTextLayoutError::Division {
            divisor: 0,
            ..
        }))
    ));
    assert_eq!(host.stars, 0);
    assert!(host.events.is_empty());
}
