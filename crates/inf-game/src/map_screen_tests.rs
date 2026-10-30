use std::path::{Path, PathBuf};

use super::*;
use crate::frontend_renderer::FrontendRenderContext;
use crate::frontend_scene::SceneProjection;
use crate::input::DeviceButtons;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[derive(Debug, Default)]
struct FactoryHost;

impl MapFactoryHost for FactoryHost {
    type Error = &'static str;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        let source = input
            .create_device(DeviceButtons::default())
            .map_err(|_| "device failed")?;
        Ok(vec![FrontendDevice {
            native_type: 0,
            source,
        }])
    }
}

struct Harness {
    registry: ScreenRegistry<FrontendRenderContext, InputContextId>,
    input: InputRuntime,
    screen: MapScreen,
}

impl Harness {
    fn new(vehicle_words: Option<[u16; 2]>) -> Self {
        let assets = FrontendAssets::open(&root()).expect("retail Infest.exe is required");
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(FrontendRenderContext {
                projection: SceneProjection {
                    focal: [320, 240],
                    viewport: [640, 480],
                    fade: [0, 0],
                },
            }),
        };
        let mut input = InputRuntime::new(0);
        let mut screen = MapScreen::new(
            MapScreenConfig {
                attachment,
                callback_context: MAP_CALLBACK_CONTEXT,
            },
            &assets,
        );
        // The Gameplay -> Phase7 transition: current 4, requested 7.
        let state = SessionState::pending_for_tests(SessionPhase::Gameplay, SessionPhase::Phase7);
        screen
            .initialize(
                &mut MapFactoryServices {
                    registry: &mut registry,
                    input: &mut input,
                    assets: &assets,
                },
                &mut FactoryHost,
                &state,
                vehicle_words,
            )
            .unwrap();
        Self {
            registry,
            input,
            screen,
        }
    }

    fn callback(&mut self, address: u32, value: u32) -> MapInputAction {
        self.screen
            .digital_callback(InputCallback {
                address,
                context: MAP_CALLBACK_CONTEXT,
                value,
                timestamp_micros: 0,
            })
            .unwrap()
    }

    fn tick(&mut self, delta_micros: i32, elapsed_micros: i32) {
        self.screen
            .tick(
                SchedulerTick {
                    delta_micros,
                    elapsed_micros,
                },
                &self.input,
            )
            .unwrap();
    }
}

const UP: u32 = 0x0045_20c0;
const DOWN: u32 = 0x0045_20f0;
const LEFT: u32 = 0x0045_2120;
const RIGHT: u32 = 0x0045_2150;
const ZOOM_IN: u32 = 0x0045_2180;
const ZOOM_OUT: u32 = 0x0045_21b0;

#[test]
fn class_and_reuse_match_the_native_queries() {
    assert_eq!(MAP_CLASS_ID, 7);
    assert!(map_reuses_phase(SessionPhase::Phase7));
    for phase in [
        SessionPhase::Gameplay,
        SessionPhase::Phase5,
        SessionPhase::Phase8,
    ] {
        assert!(!map_reuses_phase(phase));
    }
}

#[test]
fn retail_phase7_keyboard_rows_reach_the_map_callbacks() {
    let assets = FrontendAssets::open(&root()).expect("retail Infest.exe is required");
    let keyboard = assets.phase7_input_tables().digital[0].as_ref().unwrap();
    let rows: Vec<_> = keyboard
        .records
        .iter()
        .map(|row| (row.descriptor.callback_address, row.required[0][0]))
        .collect();
    // Esc, Space, Tab, Return and M return; the arrows steer; Insert and
    // Delete zoom.
    for key in [0x01, 0x39, 0x0f, 0x1c, 0x32] {
        assert!(rows.contains(&(MAP_RETURN_CALLBACK, key)));
    }
    for (callback, key) in [
        (UP, 0xc8),
        (DOWN, 0xd0),
        (LEFT, 0xcb),
        (RIGHT, 0xcd),
        (ZOOM_IN, 0xd2),
        (ZOOM_OUT, 0xd3),
    ] {
        assert!(rows.contains(&(callback, key)), "{callback:#x}");
    }
}

#[test]
fn construction_seeds_the_zoom_and_the_vehicles_centre() {
    let h = Harness::new(Some([0x5a1d, 0x9387]));
    assert_eq!(h.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(
        *h.screen.state(),
        MapState {
            age_c0: 0,
            held_ec: 0,
            center: [0x5a1d, 0x9387],
            velocity: [0, 0],
            zoom_f8: 1,
        }
    );
    // Without a vehicle the centre is zero.
    assert_eq!(Harness::new(None).screen.state().center, [0, 0]);
}

#[test]
fn callbacks_hold_their_bits_and_the_return_requests_on_press() {
    let mut h = Harness::new(None);
    for (address, bit) in MAP_HELD_CALLBACKS {
        assert_eq!(h.callback(address, 1), MapInputAction::None);
        assert_eq!(h.screen.state().held_ec, bit);
        assert_eq!(h.callback(address, 0), MapInputAction::None);
        assert_eq!(h.screen.state().held_ec, 0);
    }
    assert_eq!(
        h.callback(MAP_RETURN_CALLBACK, 1),
        MapInputAction::RequestGameplay
    );
    assert_eq!(h.callback(MAP_RETURN_CALLBACK, 0), MapInputAction::None);
    assert!(matches!(
        h.screen.digital_callback(InputCallback {
            address: 0x0043_7d30,
            context: MAP_CALLBACK_CONTEXT,
            value: 1,
            timestamp_micros: 0,
        }),
        Err(MapScreenError::UnknownCallback(0x0043_7d30))
    ));
    assert!(matches!(
        h.screen.digital_callback(InputCallback {
            address: UP,
            context: 5,
            value: 1,
            timestamp_micros: 0,
        }),
        Err(MapScreenError::Context {
            expected: MAP_CALLBACK_CONTEXT,
            actual: 5
        })
    ));
}

#[test]
fn tick_adds_raw_elapsed_steers_moves_damps_and_zooms() {
    let mut h = Harness::new(Some([100, 200]));
    h.callback(RIGHT, 1);
    h.callback(UP, 1);
    // step = delta >> 7 = 260.
    h.tick(33_333, 40_000);
    let state = *h.screen.state();
    assert_eq!(state.age_c0, 40_000);
    // Moved by the new velocity, which then damps to 7/8: 1820 / 8.
    assert_eq!(state.center, [360, 460]);
    assert_eq!(state.velocity, [227, 227]);
    h.callback(RIGHT, 0);
    h.callback(UP, 0);
    h.callback(LEFT, 1);
    h.callback(DOWN, 1);
    h.tick(33_333, 40_000);
    let state = *h.screen.state();
    // 227 - 260 = -33 moves, then damps toward zero: -231 / 8 = -28.
    assert_eq!(state.center, [327, 427]);
    assert_eq!(state.velocity, [-28, -28]);
    assert_eq!(state.age_c0, 80_000);
    // The centre wraps as a WORD.
    let mut h = Harness::new(Some([10, 0xfff0]));
    h.callback(LEFT, 1);
    h.callback(UP, 1);
    h.tick(33_333, 40_000);
    assert_eq!(
        h.screen.state().center,
        [10u16.wrapping_sub(260), 0xfff0u16.wrapping_add(260)]
    );
}

#[test]
fn zoom_steps_by_two_between_minus_one_and_129() {
    let mut h = Harness::new(None);
    h.callback(ZOOM_IN, 1);
    for _ in 0..100 {
        h.tick(10_000, 10_000);
    }
    assert_eq!(h.screen.state().zoom_f8, 129);
    h.callback(ZOOM_IN, 0);
    h.callback(ZOOM_OUT, 1);
    for _ in 0..100 {
        h.tick(10_000, 10_000);
    }
    assert_eq!(h.screen.state().zoom_f8, -1);
    // Both held: in, then out, in one tick.
    h.callback(ZOOM_IN, 1);
    h.tick(10_000, 10_000);
    assert_eq!(h.screen.state().zoom_f8, -1);
}

#[test]
fn destroy_releases_the_screen() {
    let mut h = Harness::new(None);
    h.screen.destroy(&mut h.registry, &mut h.input).unwrap();
    assert_eq!(h.screen.lifecycle(), ScreenLifecycle::Destroyed);
    assert_eq!(h.screen.id(), None);
    h.screen.destroy(&mut h.registry, &mut h.input).unwrap();
}

#[test]
fn layout_follows_00451250_at_640_by_480() {
    let layout = map_layout([640, 480], 1);
    assert_eq!(layout.column, 240);
    assert_eq!(layout.center, [384, 240]);
    assert_eq!(layout.map_clip, [160, 0, 639, 479]);
    assert_eq!(layout.full_clip, [0, 0, 639, 479]);
    // 129 * 240 * 2 / 128 and 129 * 220 * 2 / 128, truncated.
    assert_eq!(layout.tile, [483, 443]);
    assert_eq!(layout.frame, [320, 293]);
    assert_eq!(map_layout([640, 480], -1).tile, [476, 436]);
    assert_eq!(map_layout([640, 480], 129).tile, [963, 883]);
}

#[test]
fn frame_quads_put_the_centre_last_and_the_outer_corner_second() {
    let quads = map_frame_quads([384, 240], [320, 293]);
    let corners: Vec<_> = quads.iter().map(|quad| quad.corners).collect();
    assert_eq!(
        corners,
        [
            [[64, 240], [64, -53], [384, -53], [384, 240]],
            [[384, -53], [704, -53], [704, 240], [384, 240]],
            [[704, 240], [704, 533], [384, 533], [384, 240]],
            [[384, 533], [64, 533], [64, 240], [384, 240]],
        ]
    );
    for quad in quads {
        assert_eq!(quad.sprite_slot, 0x29e);
        assert_eq!(quad.vertex_depths, [0x6000; 4]);
        assert_eq!(quad.descriptor_word, 0);
        assert_eq!(quad.mode, DirectSpriteMode::Textured);
    }
}

#[test]
fn pulse_and_legend_colours_follow_the_signed_256ths() {
    assert_eq!(map_pulse(0), 0);
    assert_eq!(map_pulse(25_600), -10);
    assert_eq!(map_pulse(655_350), -256);
    assert_eq!(map_pulse(655_360), 0);
    assert_eq!(map_pulse(-10), -256);
    // White RGB555 under the display shifts (6, 1, 4).
    let white = 0x7fff;
    let shifts = [6, 1, 4];
    assert_eq!(
        legend_blip_color(0xabff_ffff, white, shifts, 0),
        0xab00_0000
    );
    // -127 * 248 / 256 = -123, wrapped.
    assert_eq!(
        legend_blip_color(0xabff_ffff, white, shifts, -128),
        0xab85_8585
    );
    assert_eq!(
        legend_blip_color(0xabff_ffff, white, shifts, -256),
        0xab09_0909
    );
    // A small pulse wraps to the brightest: -2 * 248 / 256 = -1.
    assert_eq!(
        legend_blip_color(0xabff_ffff, white, shifts, -3),
        0xabff_ffff
    );
    // Channels come from the material: pure red keeps only the low byte.
    assert_eq!(
        legend_blip_color(0x00ff_ffff, 0x7c00, shifts, -256),
        0x0000_0009
    );
}

#[derive(Debug, PartialEq, Eq)]
enum Op {
    Prepare,
    Batch,
    Clip([i16; 4]),
    Scanner,
    Map(MapTilesRequest),
    Quad(DirectSpriteQuad),
    Style(i32),
    Material(i32),
    String(i32),
    Width(Vec<u8>),
    Raw {
        text: Vec<u8>,
        position: [i32; 2],
        width: i32,
        color: u32,
        age: i32,
    },
    Line {
        text: Vec<u8>,
        position: [i32; 2],
        color: u32,
    },
}

struct Recorder {
    ops: Vec<Op>,
    /// Lines each raw text wraps to, in call order.
    lines: Vec<i32>,
}

impl MapDrawHost for Recorder {
    type Error = ();
    fn prepare(&mut self) -> Result<[i32; 2], ()> {
        self.ops.push(Op::Prepare);
        Ok([640, 480])
    }
    fn begin_batch(&mut self) -> Result<(), ()> {
        self.ops.push(Op::Batch);
        Ok(())
    }
    fn clip(&mut self, rect: [i16; 4]) -> Result<(), ()> {
        self.ops.push(Op::Clip(rect));
        Ok(())
    }
    fn resource_scanner(&mut self) -> Result<bool, ()> {
        self.ops.push(Op::Scanner);
        Ok(true)
    }
    fn map(&mut self, request: MapTilesRequest) -> Result<(), ()> {
        self.ops.push(Op::Map(request));
        Ok(())
    }
    fn quad(&mut self, quad: DirectSpriteQuad) -> Result<(), ()> {
        self.ops.push(Op::Quad(quad));
        Ok(())
    }
    fn style(&mut self, slot: i32) -> Result<u32, ()> {
        self.ops.push(Op::Style(slot));
        Ok(0x8000_0000 | slot as u32)
    }
    fn material(&mut self, slot: i32) -> Result<u32, ()> {
        self.ops.push(Op::Material(slot));
        Ok(0x7fff)
    }
    fn material_shifts(&mut self) -> [u32; 3] {
        [6, 1, 4]
    }
    fn string(&mut self, id: i32) -> Result<Vec<u8>, ()> {
        self.ops.push(Op::String(id));
        Ok(match id {
            0x11a => b"Map of\0".to_vec(),
            0x36a => b"Terra Nova\0".to_vec(),
            _ => format!("text {id:x}").into_bytes(),
        })
    }
    fn text_width(&mut self, text: &[u8]) -> Result<i16, ()> {
        self.ops.push(Op::Width(text.to_vec()));
        Ok(150)
    }
    fn raw_text(
        &mut self,
        text: &[u8],
        position: &mut [i32; 2],
        width: i32,
        color_word: u32,
        age_millis: i32,
    ) -> Result<(), ()> {
        self.ops.push(Op::Raw {
            text: text.to_vec(),
            position: *position,
            width,
            color: color_word,
            age: age_millis,
        });
        // Each wrapped line ends 18 pixels lower, at the column's start.
        let lines = self.lines.remove(0);
        *position = [position[0], position[1] + 18 * lines];
        Ok(())
    }
    fn line_text(&mut self, text: &[u8], position: [i32; 2], color_word: u32) -> Result<(), ()> {
        self.ops.push(Op::Line {
            text: text.to_vec(),
            position,
            color: color_word,
        });
        Ok(())
    }
}

#[test]
fn draw_follows_00451250_in_native_order() {
    let state = MapState {
        age_c0: 3_456_789,
        held_ec: 0,
        center: [0x1234, 0x8765],
        velocity: [0, 0],
        zoom_f8: 1,
    };
    // The title, then objectives wrapping to 0, 1, 0 and 0 extra lines.
    let mut recorder = Recorder {
        ops: Vec::new(),
        lines: vec![0, 0, 1, 0, 0],
    };
    draw_map_screen(&state, 3 * 3600 + 25 * 60 + 7, &mut recorder).unwrap();
    let pulse = map_pulse(3_456_789);
    assert_eq!(pulse, -(345_678 & 0xffff) >> 8);
    let radius = 480 * pulse / 0x4000;
    let mut expected = vec![
        Op::Prepare,
        Op::Batch,
        Op::Clip([160, 0, 639, 479]),
        Op::Scanner,
        Op::Map(MapTilesRequest {
            center: [384, 240],
            size: [483, 443],
            camera_words: [0x1234, 0x8765],
            scan_age: 3456,
            resource_scanner: true,
        }),
        Op::Clip([0, 0, 639, 479]),
    ];
    expected.extend(map_frame_quads([384, 240], [320, 293]).map(Op::Quad));
    for (material, y) in [(0x59, 403), (0x56, 417), (0x57, 432)] {
        expected.push(Op::Material(material));
        expected.push(Op::Style(0x0e));
        expected.push(Op::Quad(crate::briefing_markers::blip(
            [332, y],
            radius,
            legend_blip_color(0x8000_000e, 0x7fff, [6, 1, 4], pulse),
        )));
    }
    expected.extend([
        Op::Style(5),
        Op::String(0x36a),
        Op::String(0x11a),
        Op::Width(b"Map of Terra Nova".to_vec()),
        Op::Raw {
            text: b"Map of Terra Nova".to_vec(),
            position: [245, 24],
            width: 640,
            color: 0x8000_0005,
            age: 3456,
        },
    ]);
    // Each objective starts 45 pixels below where the last one ended.
    for (id, y) in [(0x380, 60), (0x381, 105), (0x382, 168), (0x383, 213)] {
        expected.push(Op::String(id));
        expected.push(Op::Raw {
            text: format!("text {id:x}").into_bytes(),
            position: [20, y],
            width: 240,
            color: 0x8000_0005,
            age: 3456,
        });
    }
    expected.push(Op::Line {
        text: b"03:25:07".to_vec(),
        position: [500, 24],
        color: 0x8000_0005,
    });
    for (text, style, y) in [(0x11c, 0x0e, 408), (0x11d, 0x2a, 422), (0x11e, 0x28, 436)] {
        expected.push(Op::String(text));
        expected.push(Op::Style(style));
        expected.push(Op::Line {
            text: format!("text {text:x}").into_bytes(),
            position: [352, y],
            color: 0x8000_0000 | style as u32,
        });
    }
    assert_eq!(recorder.ops, expected);
}

/// Level 1 (world 18) drawn whole at 640x480 over its loaded world: the map
/// fills the right three quarters, the title and the four objectives the
/// left. `INF_MAP_SCREEN_PNG` names a file to write the frame to.
#[test]
fn level_1_map_screen_draws_its_map_frame_legend_and_text() {
    use crate::{
        frontend_display::frontend_startup,
        frontend_renderer::FrontendRendererState,
        model_primitives::PrimitiveTables,
        render_resources::RenderResources,
        software_frame::SoftwareFrame,
        software_scene::{SoftwareScene, SoftwareSceneRequest},
    };
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_at_tier(
        18,
        crate::terrain_runtime::TerrainSpacing::Units128,
        2,
    );
    let tables = PrimitiveTables::from_executable(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let resources = RenderResources::new(&runtime);
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup.scene.environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    let player = world.arena.first_player();
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    let words = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let state = MapState {
        // Three seconds in: every reveal has finished.
        age_c0: 3_000_000,
        held_ec: 0,
        center: [words[0], words[2]],
        velocity: [0, 0],
        zoom_f8: 1,
    };
    let map_state = crate::world_map::WorldMapState::new();
    let mut renderer = FrontendRendererState::default();
    let rng_before = world.rng;
    software::SoftwareMapFrame {
        scene: &mut scene,
        resources,
        state: &mut renderer,
        world: software::MapWorld {
            arena: &world.arena,
            map_state: &map_state,
            network_active: false,
            rng: &mut world.rng,
            player,
        },
    }
    .draw(&state, 754)
    .unwrap();
    assert_eq!(world.rng, rng_before, "no jitter draws no RNG");
    let rgba = scene.pixels.frame.rgba().to_vec();
    let lit = |x0: usize, y0: usize, x1: usize, y1: usize| {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| rgba[(y * 640 + x) * 4..][..3] != [0, 0, 0])
            .count()
    };
    // The map and its frame light most of the right three quarters.
    assert!(
        lit(160, 0, 640, 480) > 480 * 240,
        "{}",
        lit(160, 0, 640, 480)
    );
    // Title, objectives and clock text in the left column's rows.
    assert!(lit(0, 60, 160, 240) > 0);
    if let Ok(path) = std::env::var("INF_MAP_SCREEN_PNG") {
        // RGB: the frame's darkening blend also darkens its alpha.
        let rgb: Vec<u8> = rgba
            .chunks_exact(4)
            .flat_map(|pixel| pixel[..3].to_vec())
            .collect();
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, 640, 480);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgb)
            .unwrap();
    }
}
