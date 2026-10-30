use std::convert::Infallible;

use inf_formats::{ByteSpan, Font, FontMode, GlyphMetrics};
use serde::Deserialize;
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_scene::{FooterQuad, FrontendSceneHost, SceneCommand, SceneProjection, StarQuad},
    menu::MenuColor,
    menu_basis::MenuBasisMath,
    menu_text::{LineTextRequest, MenuTextResources, RawMenuTextRequest},
    name_entry_draw::{draw_name_entry_overlay, NameEntryDrawFrame, NameEntryDrawHost},
    name_entry_scene::{draw_name_entry_scene, NameEntrySceneFrame},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    Renderer(usize),
    Arena,
    Begin(char),
    Publish,
    Surface(usize),
    Context,
    Reset,
    Validate,
    Backdrop,
    Model(i16),
    Overlay,
    Star(usize),
    Flush,
    Highlight,
    Text(usize),
    Finish,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("injected failure at {point:?}, native code {code:?}")]
struct Fault {
    point: Point,
    code: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InputSnapshot {
    name: Vec<u8>,
    fields: [i32; 6],
    error: bool,
    animation: AnimationClock,
    selector_one: u16,
}

impl From<NameEntryFrameInput<'_>> for InputSnapshot {
    fn from(input: NameEntryFrameInput<'_>) -> Self {
        Self {
            name: input.state.name.to_vec(),
            fields: [
                input.age_millis,
                input.state.selection,
                input.state.row_anim,
                input.state.completion_delay,
                input.state.feedback,
                input.animation.value() as i32,
            ],
            error: input.state.error,
            animation: input.animation,
            selector_one: input.selector_one,
        }
    }
}

// These are required effect receipts, not pixels. The backdrop and overlay
// themselves run their concrete owners; only renderer/resource effects are
// supplied here. Their detailed arithmetic has separate native fixtures.
struct Recorder {
    mutate: bool,
    calls: Vec<Point>,
    receipts: Vec<Value>,
    failure: Option<Fault>,
    renderer_reads: usize,
    surface_reads: usize,
    surface: [i32; 2],
    current_context: usize,
    context_renderers: [char; 2],
    arena_replacements: Vec<(char, usize)>,
    published: Option<char>,
    locked: bool,
    stars: usize,
    text: usize,
    snapshots: Vec<InputSnapshot>,
}

impl Recorder {
    fn new(mutate: bool) -> Self {
        Self {
            mutate,
            calls: Vec::new(),
            receipts: Vec::new(),
            failure: None,
            renderer_reads: 0,
            surface_reads: 0,
            surface: [640, 480],
            current_context: 0,
            context_renderers: [if mutate { 'E' } else { 'A' }, 'Z'],
            arena_replacements: Vec::new(),
            published: None,
            locked: true,
            stars: 0,
            text: 0,
            snapshots: Vec::new(),
        }
    }

    fn visit(&mut self, point: Point) -> Result<(), Fault> {
        self.calls.push(point);
        match self.failure {
            Some(fault) if fault.point == point => Err(fault),
            _ => Ok(()),
        }
    }

    fn overlay_renderer(&self) -> char {
        if self.mutate {
            'D'
        } else {
            'A'
        }
    }
}

impl ScreenRendererPreparationHost<Fault> for Recorder {
    type Renderer = char;

    fn renderer(&mut self) -> Result<char, Fault> {
        self.renderer_reads += 1;
        self.visit(Point::Renderer(self.renderer_reads))?;
        let renderer = if self.mutate {
            ['A', 'B', 'C', 'D'][self.renderer_reads - 1]
        } else {
            'A'
        };
        self.receipts.push(json!(["renderer", renderer]));
        Ok(renderer)
    }

    fn replace_command_arena(&mut self, renderer: char, bytes: usize) -> Result<(), Fault> {
        self.visit(Point::Arena)?;
        self.arena_replacements.push((renderer, bytes));
        self.receipts.push(json!(["replace", renderer, bytes]));
        Ok(())
    }

    fn begin_batch(&mut self, renderer: char) -> Result<(), Fault> {
        self.visit(Point::Begin(renderer))?;
        self.receipts.push(json!(["begin", renderer]));
        Ok(())
    }

    fn publish_current_renderer(&mut self, renderer: char) -> Result<(), Fault> {
        self.visit(Point::Publish)?;
        self.published = Some(renderer);
        self.receipts.push(json!(["publish", renderer]));
        Ok(())
    }
}

impl NameEntryFrameHost for Recorder {
    type Error = Fault;
    type Context = usize;

    fn surface_pixels(&mut self) -> Result<[i32; 2], Fault> {
        self.surface_reads += 1;
        self.visit(Point::Surface(self.surface_reads))?;
        self.receipts.push(json!(["width", self.surface[0]]));
        self.receipts.push(json!(["height", self.surface[1]]));
        Ok(self.surface)
    }

    fn capture_caller_context(&mut self) -> Result<usize, Fault> {
        self.visit(Point::Context)?;
        self.receipts.push(json!([
            "capture_context",
            if self.current_context == 0 {
                "original"
            } else {
                "replacement"
            }
        ]));
        Ok(self.current_context)
    }

    fn reset_renderer(&mut self, renderer: char) -> Result<(), Fault> {
        self.visit(Point::Reset)?;
        self.receipts.push(json!(["reset", renderer, 0]));
        Ok(())
    }

    fn validate_surface(&mut self) -> Result<(), Fault> {
        self.visit(Point::Validate)?;
        self.receipts.push(json!(["validate"]));
        if self.mutate {
            self.surface = [800, 600];
            self.current_context = 1;
            self.context_renderers[0] = 'F';
        }
        Ok(())
    }

    fn draw_backdrop(
        &mut self,
        context: usize,
        input: NameEntryFrameInput<'_>,
    ) -> Result<(), Fault> {
        self.visit(Point::Backdrop)?;
        self.snapshots.push(input.into());
        self.receipts.push(json!([
            "backdrop",
            if context == 0 {
                "original"
            } else {
                "replacement"
            }
        ]));
        let viewport = self.surface_pixels()?;
        let renderer = self.context_renderers[context];
        draw_name_entry_scene(
            NameEntrySceneFrame {
                projection: SceneProjection {
                    focal: [320, 320],
                    viewport,
                    fade: [0x100000; 2],
                },
                age_millis: input.age_millis,
                animation: input.animation,
                selector_one: input.selector_one,
            },
            &MenuBasisMath::new([0; 4096]),
            &mut SceneSink {
                owner: self,
                renderer,
                viewport,
            },
        )
    }

    fn draw_overlay(
        &mut self,
        renderer: char,
        input: NameEntryFrameInput<'_>,
        surface_pixels: [i32; 2],
    ) -> Result<(), Fault> {
        self.visit(Point::Overlay)?;
        self.snapshots.push(input.into());
        assert_eq!(renderer, self.overlay_renderer());
        // Validation changed the current dimensions; overlay layout must use
        // the dimensions captured before validation, just as the PE does.
        assert_eq!(surface_pixels, [640, 480]);
        let resources = Resources::new();
        draw_name_entry_overlay(
            NameEntryDrawFrame {
                state: input.state,
                age_millis: input.age_millis,
                surface_pixels,
                focal: [320, 320],
                context_origin_pixels: [320, 240],
                base_color: MenuColor::from_word(0x456789ab),
                sine_quarter: &[0; 4096],
            },
            &resources,
            &mut SceneSink {
                owner: self,
                renderer,
                viewport: surface_pixels,
            },
        )
        .map_err(|error| match error {
            crate::name_entry_draw::NameEntryDrawError::Host(error) => error,
            _ => panic!("unexpected layout/resource error: {error:?}"),
        })
    }

    fn finish_surface_draw(&mut self) -> Result<(), Fault> {
        self.visit(Point::Finish)?;
        self.receipts.push(json!(["finish_surface", true]));
        if self.locked {
            self.receipts.push(json!(["unlock", true, 0]));
            self.locked = false;
        }
        Ok(())
    }
}

struct SceneSink<'a> {
    owner: &'a mut Recorder,
    renderer: char,
    viewport: [i32; 2],
}

impl FrontendSceneHost for SceneSink<'_> {
    type Error = Fault;

    fn submit(&mut self, command: SceneCommand) -> Result<(), Fault> {
        match command {
            SceneCommand::Model(model) => {
                self.owner.visit(Point::Model(model.model_slot))?;
                self.owner.receipts.push(json!([
                    "model",
                    self.renderer,
                    model.model_slot,
                    self.viewport
                ]));
                let input = self.owner.snapshots.last().unwrap();
                assert_eq!(model.animation, input.animation);
                assert_eq!(model.callback_value(1), Some(input.selector_one));
            }
            SceneCommand::FlushBatch => {
                self.owner.visit(Point::Flush)?;
                self.owner
                    .receipts
                    .push(json!(["flush", self.renderer, self.owner.stars]));
            }
            SceneCommand::Setup(setup) => assert_eq!(setup.viewport, self.viewport),
            SceneCommand::DirectionalLight { .. }
            | SceneCommand::AmbientLight(_)
            | SceneCommand::PublishView(_) => {}
            other => panic!("unexpected name-entry command: {other:?}"),
        }
        Ok(())
    }

    fn footer_style_word(&mut self) -> Result<u32, Fault> {
        panic!("no frontend footer")
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Fault> {
        panic!("already captured by frame host")
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Fault> {
        panic!("highlight uses text metrics")
    }
}

impl NameEntryDrawHost for SceneSink<'_> {
    fn star(&mut self, _: StarQuad) -> Result<(), Fault> {
        self.owner.visit(Point::Star(self.owner.stars + 1))?;
        self.owner.stars += 1;
        Ok(())
    }

    fn highlight(&mut self, _: FooterQuad) -> Result<(), Fault> {
        self.owner.visit(Point::Highlight)
    }

    fn line_text(&mut self, _: &mut LineTextRequest<'_>) -> Result<(), Fault> {
        self.owner.visit(Point::Text(self.owner.text + 1))?;
        self.owner.text += 1;
        Ok(())
    }

    fn raw_text(&mut self, _: &mut RawMenuTextRequest<'_>) -> Result<(), Fault> {
        self.owner.visit(Point::Text(self.owner.text + 1))?;
        self.owner.text += 1;
        Ok(())
    }
}

struct Resources {
    font: Font,
}

impl Resources {
    fn new() -> Self {
        Self {
            font: Font {
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
                metrics: vec![
                    GlyphMetrics {
                        advance: 500,
                        spacing: 0,
                        offset: [0, -800]
                    };
                    256
                ],
                sprite_words: (1..=256).collect(),
            },
        }
    }
}

impl MenuTextResources for Resources {
    type Error = Infallible;
    fn string(&self, argument: i32) -> Result<&[u8], Infallible> {
        Ok(match argument {
            224 => b"Cheat accepted",
            277 => b"Name",
            279 => b"DEL",
            280 => b"OK",
            354 => b"Confirmed",
            871 => b"Error saving profile",
            39 => b"Press R to retry",
            _ => panic!("unexpected string slot {argument}"),
        })
    }
    fn font(&self, argument: i32) -> Result<&Font, Infallible> {
        assert_eq!(argument, 0);
        Ok(&self.font)
    }
    fn style(&self, argument: i32) -> Result<u32, Infallible> {
        Ok(match argument {
            2 => 0x13579bc1,
            3 => 0x2468ace2,
            7 => 0x3579bdf3,
            _ => panic!("style {argument}"),
        })
    }
    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Infallible> {
        panic!("text pixels are recorded by separate fixtures")
    }
}

fn input() -> NameEntryFrameInput<'static> {
    let mut animation = AnimationClock::default();
    animation.advance(123_456);
    NameEntryFrameInput {
        state: NameEntryDrawState {
            name: b"Frame",
            selection: 77,
            row_anim: 0,
            completion_delay: 0,
            feedback: 0,
            error: false,
        },
        age_millis: 9000,
        animation,
        selector_one: 7,
    }
}

#[derive(Deserialize)]
struct NativeCase {
    mutate: bool,
    receipts: Vec<Value>,
    stars: usize,
}

/// The fixture records unmodified 00459310, 00439270, 0045A560 and
/// 004957F0/0050DCC0. Native text walkers also ran, with synthetic resources;
/// effects and model execution were hooked. The second case changes dimensions,
/// replaces session+648, and mutates the old context's renderer during validation.
/// See .tmp/inf-name-entry-frame-oracle.py. This checks custody and order, not
/// hardware pixels or undocumented surface-recreation behavior.
#[test]
fn frame_boundaries_match_native_identity_and_dimension_receipts() {
    let cases: Vec<NativeCase> = serde_json::from_str(include_str!(
        "../../tests/fixtures/name_entry_frame_native.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let mut host = Recorder::new(case.mutate);
        let request = input();
        draw_name_entry_frame(request, &mut host).unwrap();
        assert_eq!(host.receipts, case.receipts, "mutate={}", case.mutate);
        assert_eq!(host.stars, case.stars);
        assert_eq!(host.renderer_reads, 4);
        assert_eq!(host.surface_reads, 2);
        assert_eq!(host.arena_replacements, [('A', 0xc8000)]);
        assert_eq!(host.snapshots, vec![InputSnapshot::from(request); 2]);
        assert!(!host.locked);
        if case.mutate {
            assert_eq!(host.current_context, 1);
            assert_eq!(host.context_renderers, ['F', 'Z']);
            assert_eq!(host.published, Some('C'));
        }
    }
}

#[test]
fn every_frame_failure_propagates_without_unlock_or_native_301_recovery() {
    let mut complete = Recorder::new(true);
    draw_name_entry_frame(input(), &mut complete).unwrap();
    let points = [
        Point::Renderer(1),
        Point::Arena,
        Point::Renderer(2),
        Point::Begin('B'),
        Point::Renderer(3),
        Point::Publish,
        Point::Surface(1),
        Point::Context,
        Point::Renderer(4),
        Point::Begin('D'),
        Point::Reset,
        Point::Validate,
        Point::Backdrop,
        Point::Surface(2),
        Point::Model(748),
        Point::Model(757),
        Point::Model(744),
        Point::Overlay,
        Point::Star(17),
        Point::Flush,
        Point::Text(2),
        Point::Highlight,
        Point::Finish,
    ];
    for point in points {
        let index = complete
            .calls
            .iter()
            .position(|actual| *actual == point)
            .unwrap();
        for code in [None, Some(0x301), Some(0x711)] {
            let fault = Fault { point, code };
            let mut host = Recorder::new(true);
            host.failure = Some(fault);
            assert!(
                matches!(draw_name_entry_frame(input(), &mut host), Err(NameEntryFrameError::Host(actual)) if actual == fault)
            );
            assert_eq!(host.calls, complete.calls[..=index], "{fault}");
            assert!(
                host.locked,
                "failure cannot manufacture successful surface unlock: {fault}"
            );
            assert!(!host
                .receipts
                .iter()
                .any(|event| event[0] == "finish_surface" || event[0] == "unlock"));
            let published = complete
                .calls
                .iter()
                .position(|p| *p == Point::Publish)
                .unwrap();
            assert_eq!(
                host.published,
                (index > published).then_some('C'),
                "completed publication survives: {fault}"
            );
            if point == Point::Star(17) {
                assert_eq!(host.stars, 16);
            }
            if point == Point::Flush {
                assert_eq!((host.stars, host.text), (1000, 0));
            }
            if point == Point::Text(2) {
                assert_eq!((host.stars, host.text), (1000, 1));
            }
        }
    }
}

#[test]
fn outer_draw_forwards_unusual_screen_state_without_advancing_it() {
    let mut request = input();
    request.state.name = b"A B";
    request.state.selection = 75;
    request.state.row_anim = -2520;
    request.state.completion_delay = 700;
    request.state.feedback = -1;
    request.state.error = true;
    request.age_millis = -5000;
    request.selector_one = 0xffff;
    let expected = InputSnapshot::from(request);
    let mut host = Recorder::new(true);
    draw_name_entry_frame(request, &mut host).unwrap();
    assert_eq!(host.snapshots, [expected.clone(), expected]);
}
