use super::*;
use crate::loading_runtime::LoadingProcessState;
use crate::{Installation, ResidencyRequest, ResidentRuntime};
use inf_formats::{ByteSpan, Font, FontMode, GlyphMetrics};
use std::{cell::RefCell, path::Path, rc::Rc};

type Events = Rc<RefCell<Vec<String>>>;

struct Resources {
    font: Font,
    events: Events,
    fail: Option<(&'static str, i32)>,
    title: Option<Vec<u8>>,
}

impl Resources {
    fn supplied(events: Events) -> Self {
        Self {
            font: Font {
                local_index: 0,
                header_span: ByteSpan::new(0, 64),
                raw_header: [0; 16],
                mode: FontMode::Direct,
                coordinate_divisor: 100,
                line_step_terms: [1000, 200],
                maximum_byte: 255,
                fallback_byte: b'?',
                metric_span: ByteSpan::new(64, 2112),
                sprite_span: ByteSpan::new(2112, 3136),
                metrics: vec![
                    GlyphMetrics {
                        advance: 600,
                        spacing: -100,
                        offset: [0, 0]
                    };
                    256
                ],
                sprite_words: (1..=256).collect(),
            },
            events,
            fail: None,
            title: None,
        }
    }
    fn reached(&self, kind: &'static str, slot: i32) -> Result<(), &'static str> {
        self.events.borrow_mut().push(format!("{kind}{slot}"));
        if self.fail == Some((kind, slot)) {
            Err(kind)
        } else {
            Ok(())
        }
    }
}
impl MenuTextResources for Resources {
    type Error = &'static str;
    fn string(&self, _: i32) -> Result<&[u8], Self::Error> {
        panic!("optional owner required")
    }
    fn font(&self, slot: i32) -> Result<&Font, Self::Error> {
        self.reached("font", slot)?;
        Ok(&self.font)
    }
    fn style(&self, slot: i32) -> Result<u32, Self::Error> {
        self.reached("style", slot)?;
        Ok(0x99112233)
    }
    fn sprite_dimensions(&self, slot: i32) -> Result<[u16; 2], Self::Error> {
        self.reached("glyphSprite", slot)?;
        Ok([20, 34])
    }
}
impl LoadingDrawResources for Resources {
    fn required_sprite(&self, slot: i32) -> Result<(), Self::Error> {
        self.reached("sprite", slot)
    }
    fn attract_sprite(&self, slot: i32) -> Result<bool, Self::Error> {
        self.reached("sprite", slot)?;
        Ok(true)
    }
    fn optional_loading_string(
        &self,
        slot: i32,
    ) -> Result<Option<&[u8]>, LoadingStringError<Self::Error>> {
        self.reached("string", slot)
            .map_err(LoadingStringError::Source)?;
        Ok(self.title.as_deref())
    }
}

struct Host {
    events: Events,
    width: i32,
    height: i32,
    attract: u8,
    modes: Vec<u8>,
    mode_reads: usize,
    fail: Option<&'static str>,
    fail_quad: Option<usize>,
    quads: Vec<DirectSpriteQuad>,
    glyphs: Vec<crate::menu_text::MenuGlyphQuad>,
}
impl Host {
    fn supplied(events: Events, mode: u8) -> Self {
        Self {
            events,
            width: 640,
            height: 480,
            attract: 0,
            modes: vec![mode],
            mode_reads: 0,
            fail: None,
            fail_quad: None,
            quads: Vec::new(),
            glyphs: Vec::new(),
        }
    }
    fn reached(&mut self, phase: &'static str) -> Result<(), &'static str> {
        self.events.borrow_mut().push(phase.into());
        if self.fail == Some(phase) {
            Err(phase)
        } else {
            Ok(())
        }
    }
}
impl MenuTextHost for Host {
    type Error = &'static str;
    fn submit_glyph(&mut self, glyph: crate::menu_text::MenuGlyphQuad) -> Result<(), Self::Error> {
        self.reached("glyph")?;
        self.glyphs.push(glyph);
        Ok(())
    }
}
impl LoadingDrawHost for Host {
    fn acquire_renderer(&mut self) -> Result<(), Self::Error> {
        self.reached("renderer")
    }
    fn surface_width(&mut self) -> Result<i32, Self::Error> {
        self.reached("width")?;
        Ok(self.width)
    }
    fn surface_height(&mut self) -> Result<i32, Self::Error> {
        self.reached("height")?;
        Ok(self.height)
    }
    fn acquire_surface(&mut self) -> Result<(), Self::Error> {
        self.reached("surface")
    }
    fn reset_batch_40e6fd(&mut self) -> Result<(), Self::Error> {
        self.reached("resetBatch")
    }
    fn reset_surface_40e70a(&mut self) -> Result<(), Self::Error> {
        self.reached("resetSurface")
    }
    fn current_attract_mode(&mut self) -> u8 {
        self.events.borrow_mut().push("attract".into());
        self.attract
    }
    fn current_loading_mode(&mut self) -> u8 {
        self.events.borrow_mut().push("mode".into());
        let value = self.modes[self.mode_reads.min(self.modes.len() - 1)];
        self.mode_reads += 1;
        value
    }
    fn draw_model_background(
        &mut self,
        _: &LoadingProcessState,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.reached("model117")
    }
    fn finish_background_495770(&mut self) -> Result<(), Self::Error> {
        self.reached("finish")
    }
    fn flush_batch_4a4460(&mut self) -> Result<(), Self::Error> {
        self.reached("flush")
    }
    fn submit_loading_quad(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error> {
        self.events
            .borrow_mut()
            .push(format!("quad{}", self.quads.len()));
        if self.fail_quad == Some(self.quads.len()) {
            return Err("quad");
        }
        self.quads.push(quad);
        Ok(())
    }
}

fn state() -> LoadingProcessState {
    LoadingProcessState {
        counter_546918: 1990,
        counter_54691c: 2000,
        counter_546920: 0,
        previous_clock: 123,
        raw_progress: 345,
        age_micros: 678,
        pulse_direction: 1,
        selector: 4,
        pulse: 0xabcd1234,
    }
}
fn draw<R: LoadingDrawResources>(
    s: &mut LoadingProcessState,
    effects: &mut [u8; 16],
    rng: &mut u32,
    delta: i32,
    status: i32,
    resources: &R,
    host: &mut Host,
) -> Result<(), LoadingDrawError<R::Error, &'static str>> {
    draw_loading_tick_40e6b0(
        LoadingDrawRequest {
            tick: LoadingTickRequest {
                state: s,
                effect_bytes: effects,
                rng,
                delta_micros: delta,
                caller_status: status,
            },
            context_origin_pixels: [319, 237],
        },
        resources,
        host,
    )
}

#[test]
fn ordinary_mode_zero_draws_background_then_flushes_and_draws_centered_title() {
    let events = Events::default();
    let mut r = Resources::supplied(events.clone());
    r.title = Some(b"AB".to_vec());
    let mut h = Host::supplied(events.clone(), 0);
    let mut s = state();
    let before = s.clone();
    let mut effects = [21; 16];
    let mut rng = 99;
    draw(&mut s, &mut effects, &mut rng, 7800, 1, &r, &mut h).unwrap();
    assert_eq!(
        &events.borrow()[..13],
        [
            "renderer",
            "width",
            "height",
            "surface",
            "resetBatch",
            "resetSurface",
            "attract",
            "mode",
            "model117",
            "finish",
            "flush",
            "mode",
            "font0"
        ]
    );
    assert_eq!(s, before);
    assert_eq!(effects, [21; 16]);
    assert_eq!(rng, 99);
    assert!(h.quads.is_empty());
    assert_eq!(h.glyphs.len(), 4);
    assert_eq!(
        h.glyphs[0].corners,
        [[315, 231], [320, 231], [320, 240], [315, 240]]
    );
    assert_eq!(h.glyphs[0].context_origin_pixels, [319, 237]);
    assert!(events.borrow().iter().any(|e| e == "string354"));
}

#[test]
fn equalizer_submits_raw_word_geometry_before_each_rng_and_signed_byte_decay() {
    let events = Events::default();
    let r = Resources::supplied(events.clone());
    let mut h = Host::supplied(events, 1);
    let mut s = state();
    let before = s.clone();
    let mut effects = [120; 16];
    effects[1] = 0x80;
    let mut rng = 0;
    let mut expected_rng = 0;
    for _ in 0..16 {
        name_entry_rng_next(&mut expected_rng);
    }
    draw(&mut s, &mut effects, &mut rng, 32768, 1, &r, &mut h).unwrap();
    assert_eq!(h.quads.len(), 16);
    assert_eq!(
        h.quads[0],
        DirectSpriteQuad {
            sprite_slot: 7,
            corners: [[405, 360], [413, 360], [413, 264], [405, 264]],
            vertex_depths: [64; 4],
            descriptor_word: 0,
            mode: DirectSpriteMode::Effect {
                color_word: 0x99112233
            }
        }
    );
    assert_eq!(
        h.quads[1].corners,
        [[395, 261], [403, 261], [403, 363], [395, 363]]
    );
    assert_eq!(h.quads[15].corners[0][0], 245);
    assert_eq!(effects[0], 105);
    assert_eq!(rng, expected_rng);
    assert_eq!(s, before);
}

#[test]
fn failed_direct_submission_retains_only_completed_bar_effects_and_rng() {
    for completed in [0usize, 5, 15] {
        let events = Events::default();
        let r = Resources::supplied(events.clone());
        let mut h = Host::supplied(events.clone(), 1);
        h.fail_quad = Some(completed);
        let mut s = state();
        let before = s.clone();
        let mut effects = [120; 16];
        let mut rng = 0;
        let mut expected = [120u8; 16];
        let mut expected_rng = 0;
        for value in &mut expected[..completed] {
            if name_entry_rng_next(&mut expected_rng) & 15 == 0 {
                *value = 127;
            }
            *value = (*value).wrapping_sub((i32::from(*value) * 4 / 32) as u8);
        }
        assert!(matches!(
            draw(&mut s, &mut effects, &mut rng, 32768, 1, &r, &mut h),
            Err(LoadingDrawError::Host("quad"))
        ));
        assert_eq!(h.quads.len(), completed);
        assert_eq!(effects, expected);
        assert_eq!(rng, expected_rng);
        assert_eq!(s, before);
        assert!(!events.borrow().iter().any(|e| e == "font0"));
    }
}

#[test]
fn fresh_mode_reads_choose_equalizer_after_flush_and_title_independently() {
    let events = Events::default();
    let r = Resources::supplied(events.clone());
    let mut h = Host::supplied(events.clone(), 0);
    h.modes = vec![0, 7, 0];
    let mut s = state();
    let mut effects = [0; 16];
    let mut rng = 0;
    draw(&mut s, &mut effects, &mut rng, 7800, 1, &r, &mut h).unwrap();
    assert_eq!(h.quads.len(), 16);
    assert_eq!(h.mode_reads, 3);
    assert!(events.borrow().iter().any(|e| e == "model117"));
    assert!(events.borrow().iter().any(|e| e == "string354"));
    assert!(!events.borrow().iter().any(|e| e == "string283"));
}

#[test]
fn model_phase_failure_keeps_state_and_stops_before_common_flush_and_titles() {
    let events = Events::default();
    let resources = Resources::supplied(events.clone());
    let mut host = Host::supplied(events.clone(), 0);
    host.fail = Some("model117");
    let mut state = state();
    let before = state.clone();
    let mut effects = [23; 16];
    let mut rng = 7;
    assert!(matches!(
        draw(
            &mut state,
            &mut effects,
            &mut rng,
            7800,
            1,
            &resources,
            &mut host
        ),
        Err(LoadingDrawError::Host("model117"))
    ));
    assert_eq!(events.borrow().last().unwrap(), "model117");
    assert_eq!(state, before);
    assert_eq!(effects, [23; 16]);
    assert_eq!(rng, 7);
    assert!(host.quads.is_empty() && host.glyphs.is_empty());
}

#[test]
fn attract_counter_branch_is_signed_strict_lazy_and_never_consumes_titles_or_effects() {
    for mode in [0u8, 8] {
        let events = Events::default();
        let mut r = Resources::supplied(events.clone());
        r.fail = Some(("font", 0));
        let mut h = Host::supplied(events.clone(), mode);
        h.attract = 2;
        let mut s = state();
        let mut effects = [0xff; 16];
        let mut rng = 17;
        draw(&mut s, &mut effects, &mut rng, 10000, 3, &r, &mut h).unwrap();
        assert_eq!(
            [s.counter_546918, s.counter_54691c, s.counter_546920],
            [2000, 0, 0]
        );
        assert_eq!(effects, [0xff; 16]);
        assert_eq!(rng, 17);
        // Mode 0 draws 0040E140's four sprites; style 14 at fade 1 (the
        // 00546918 counter 1990 of 2000 left) keeps only the alpha byte.
        if mode == 0 {
            assert_eq!(
                h.quads
                    .iter()
                    .map(|quad| quad.sprite_slot)
                    .collect::<Vec<_>>(),
                [8, 9, 10, 11]
            );
            assert!(h.quads.iter().all(|quad| quad.mode
                == DirectSpriteMode::Effect {
                    color_word: 0x9900_0000
                }));
        } else {
            assert!(h.quads.is_empty());
        }
        h.quads.clear();
        assert!(h.glyphs.is_empty());
        assert!(!events
            .borrow()
            .iter()
            .any(|e| e.starts_with("font") || e.starts_with("string")));
        assert_eq!(events.borrow().iter().any(|e| e == "flush"), mode == 0);
        s.counter_546918 = u32::MAX;
        s.counter_54691c = 0;
        s.counter_546920 = 1;
        draw(&mut s, &mut effects, &mut rng, -1000, 0, &r, &mut h).unwrap();
        assert_eq!(
            [s.counter_546918, s.counter_54691c, s.counter_546920],
            [u32::MAX - 1, 0, 0]
        );
    }
}

#[test]
fn required_renderer_and_resource_stops_preserve_exact_unreached_scalar_owners() {
    let phases = [
        "renderer",
        "width",
        "height",
        "surface",
        "resetBatch",
        "resetSurface",
        "finish",
        "flush",
    ];
    for phase in phases {
        let events = Events::default();
        let r = Resources::supplied(events.clone());
        let mut h = Host::supplied(events.clone(), 1);
        h.fail = Some(phase);
        let mut s = state();
        let before = s.clone();
        let mut effects = [31; 16];
        let mut rng = 13;
        assert!(
            matches!(draw(&mut s,&mut effects,&mut rng,32768,1,&r,&mut h),Err(LoadingDrawError::Host(e)) if e==phase)
        );
        assert_eq!(s, before);
        assert_eq!(effects, [31; 16]);
        assert_eq!(rng, 13);
        assert!(h.quads.is_empty());
        assert_eq!(events.borrow().last().unwrap(), phase);
    }
    for (source, mode) in [("sprite", 1), ("font", 0)] {
        let events = Events::default();
        let mut r = Resources::supplied(events.clone());
        r.fail = Some((source, if source == "sprite" { 7 } else { 0 }));
        let mut h = Host::supplied(events.clone(), mode);
        let mut s = state();
        let before = s.clone();
        let mut effects = [31; 16];
        let mut rng = 13;
        assert!(
            matches!(draw(&mut s,&mut effects,&mut rng,32768,1,&r,&mut h),Err(LoadingDrawError::Resource(e)) if e==source)
        );
        assert_eq!(s, before);
        assert_eq!(effects, [31; 16]);
        assert_eq!(rng, 13);
        assert!(h.quads.is_empty());
        if source == "font" {
            assert!(!events.borrow().iter().any(|e| e == "style4"));
        }
    }
}

#[test]
fn optional_null_still_captures_font_style_and_status_three_reaches_second_string() {
    for status in [0, 1, 2, 3, i32::MAX] {
        let events = Events::default();
        let r = Resources::supplied(events.clone());
        let mut h = Host::supplied(events.clone(), 0);
        let mut s = state();
        let mut effects = [0; 16];
        let mut rng = 7;
        draw(&mut s, &mut effects, &mut rng, 7800, status, &r, &mut h).unwrap();
        let tail: Vec<_> = events
            .borrow()
            .iter()
            .filter(|e| e.starts_with("font") || e.starts_with("style") || e.starts_with("string"))
            .cloned()
            .collect();
        let expected = if status >= 3 {
            vec!["font0", "style4", "string354", "string218"]
        } else {
            vec!["font0", "style4", "string354"]
        };
        assert_eq!(tail, expected);
        assert!(h.glyphs.is_empty());
    }
}

#[test]
fn real_current_resources_draw_equalizer_and_keep_optional_capacity_and_holes_distinct() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    // Genuine allocated pools before Boot have no resident slot0. This owns
    // the lazy-source stop without assuming the retail Boot bank has a hole.
    assert!(matches!(
        RenderResources::new(&runtime).optional_loading_string(0),
        Err(LoadingStringError::Source(
            RenderResourceError::NotResident { .. }
        ))
    ));
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let resources = RenderResources::new(&runtime);
    let capacity = resources.pool_capacity(RenderResourceKind::String);
    assert!(matches!(
        resources.optional_loading_string(capacity as i32),
        Err(LoadingStringError::CapacityBoundary { .. })
    ));
    assert_eq!(
        resources
            .optional_loading_string(capacity as i32 + 1)
            .unwrap(),
        None
    );
    let events = Events::default();
    let mut h = Host::supplied(events, 1);
    let mut s = state();
    let before = s.clone();
    let mut effects = [64; 16];
    let mut rng = 99;
    draw(&mut s, &mut effects, &mut rng, 7800, 1, &resources, &mut h).unwrap();
    assert_eq!(h.quads.len(), 16);
    assert!(!h.glyphs.is_empty());
    assert_eq!(s, before);
    assert!(h
        .glyphs
        .iter()
        .all(|glyph| glyph.context_origin_pixels == [319, 237]));
}

/// `0040E140` against `check_attract_fade.py`: each case's quads, colours
/// and early ends, with the counters, surface, style and pool the receipt
/// supplied.
#[test]
fn the_attract_fade_matches_0040e140() {
    struct Pool {
        count: i32,
        vacant: Vec<i32>,
        style: u32,
        font: Font,
    }
    impl MenuTextResources for Pool {
        type Error = &'static str;
        fn string(&self, _: i32) -> Result<&[u8], Self::Error> {
            Err("string")
        }
        fn font(&self, _: i32) -> Result<&Font, Self::Error> {
            Ok(&self.font)
        }
        fn style(&self, slot: i32) -> Result<u32, Self::Error> {
            assert_eq!(slot, 14);
            Ok(self.style)
        }
        fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
            Err("dimensions")
        }
    }
    impl LoadingDrawResources for Pool {
        fn required_sprite(&self, _: i32) -> Result<(), Self::Error> {
            Err("required")
        }
        fn attract_sprite(&self, slot: i32) -> Result<bool, Self::Error> {
            Ok(slot <= self.count && !self.vacant.contains(&slot))
        }
        fn optional_loading_string(
            &self,
            _: i32,
        ) -> Result<Option<&[u8]>, LoadingStringError<Self::Error>> {
            Ok(None)
        }
    }
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/attract_fade_native.json"
    ))
    .unwrap();
    let int = |value: &serde_json::Value| value.as_i64().unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let pool = Pool {
            count: int(&input["h3_count"]) as i32,
            vacant: input["vacant"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| int(v) as i32)
                .collect(),
            style: int(&input["style"]) as u32,
            font: Resources::supplied(Events::default()).font,
        };
        let mut host = Host::supplied(Events::default(), 0);
        let mut state = state();
        [
            state.counter_546920,
            state.counter_54691c,
            state.counter_546918,
        ] = std::array::from_fn(|i| int(&input["counters"][i]) as u32);
        draw_attract_fade_40e140(
            &state,
            int(&input["surface"][0]) as i32,
            int(&input["surface"][1]) as i32,
            &pool,
            &mut host,
        )
        .unwrap();
        let expected: Vec<serde_json::Value> = case["quads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|quad| {
                assert_eq!(quad["callback"], "0x40ef30");
                serde_json::json!({
                    "sprite": quad["sprite"],
                    "corners": quad["corners"],
                    "dwords": quad["dwords"],
                    "colour": quad["colour"],
                })
            })
            .collect();
        let actual: Vec<serde_json::Value> = host
            .quads
            .iter()
            .map(|quad| {
                let DirectSpriteMode::Effect { color_word } = quad.mode else {
                    panic!("{quad:?}");
                };
                assert_eq!(quad.descriptor_word, 0);
                serde_json::json!({
                    "sprite": quad.sprite_slot,
                    "corners": quad.corners,
                    "dwords": quad.vertex_depths,
                    "colour": color_word,
                })
            })
            .collect();
        assert_eq!(actual, expected, "{input}");
    }
}
