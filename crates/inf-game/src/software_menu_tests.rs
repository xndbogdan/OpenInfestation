use super::*;
use std::{convert::Infallible, path::Path};

use inf_formats::FrontendExecutable;

use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::FrontendEntry,
    frontend_control::{FrontendControl, FrontendControlConfig},
    frontend_display::frontend_startup,
    menu::{
        MenuActionRequest, MenuColor, MenuData, MenuDrawInput, MenuDrawPlacement, MenuHost,
        MenuRowDescriptor, MenuStack, MenuVisibility,
    },
    menu_basis::MenuBasisMath,
    menu_text::{MenuGlyphPass, MenuTextResources},
    model_primitives::PrimitiveTables,
    software_frame::SoftwareFrame,
    software_scene::SoftwareSceneRequest,
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

/// Renderer +24 (004AE870): the clip rectangle clamps to the surface, and
/// an inverted one is refused.
#[test]
fn the_clip_rectangle_clamps_to_the_surface() {
    let (mut runtime, _, tables) = corpus();
    select(&mut runtime, 2);
    let resources = RenderResources::new(&runtime);
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let process = process(resources);
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup.scene.environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    let mut state = SoftwareMenuState::default();
    let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
        scene: &mut scene,
        resources,
        focal: startup.projection.focal,
        process: &process,
        state: &mut state,
    })
    .unwrap();
    menu.set_clip_rect([-5, -3, 700, 500]).unwrap();
    assert_eq!(menu.clip_rect().unwrap(), [0, 0, 640, 480]);
    menu.set_clip_rect([10, 20, 300, 200]).unwrap();
    assert_eq!(menu.clip_rect().unwrap(), [10, 20, 300, 200]);
    assert!(matches!(
        menu.set_clip_rect([10, 20, 5, 200]),
        Err(SoftwareMenuError::InvertedClip(_))
    ));
}

#[test]
fn generated_rows_and_raw_text_draw_same_pixels_at_every_display_tier() {
    use crate::menu_text::{
        MenuTextEmission, MenuTextPlacement, MenuTextReveal, INITIAL_GLYPH_BASE_WORD,
    };
    let (mut runtime, _, tables) = corpus();
    let text = b"1. Alpha - Beta 02:03";
    for (variant, dimensions) in [[320, 240], [512, 384], [640, 480], [800, 600], [1024, 768]]
        .into_iter()
        .enumerate()
    {
        select(&mut runtime, variant as u8);
        let resources = RenderResources::new(&runtime);
        let startup = frontend_startup(
            &runtime,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let process = process(resources);
        let clear = [17, 23, 31, 255];
        let mut frame = SoftwareFrame::new(dimensions[0], dimensions[1], clear);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        let published = scene.primitive_state.projection.center;
        let mut state = SoftwareMenuState::default();
        let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut scene,
            resources,
            focal: startup.projection.focal,
            process: &process,
            state: &mut state,
        })
        .unwrap();
        menu.begin_controller(setup(dimensions.map(|v| v as i32)))
            .unwrap();
        assert_eq!(menu.published_origin_pixels(), published);
        let mut raw = RawMenuTextRequest {
            text,
            font: resources.font(0).unwrap(),
            position_pixels: [30, 40],
            width_pixels: 80,
            color: MenuColor::from_rgb([128; 3]),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: published,
            emission: MenuTextEmission::Draw,
            reveal: MenuTextReveal::Complete,
        };
        menu.draw_raw_text(&mut raw).unwrap();
        assert_eq!(menu.published_origin_pixels(), published);
        let raw_pixels = menu.scene.pixels.frame.rgba().to_vec();
        assert!(raw_pixels.chunks_exact(4).any(|p| p != clear));
        assert_eq!(menu.state.glyph_passes, text.len() * 2);
        assert_eq!(menu.state.rows, 0);
        menu.scene.pixels.frame.clear_color(clear);
        let mut descriptor = row(0).descriptor;
        descriptor.extent_pixels[0] = 80;
        let outcome = menu
            .draw_row_text(MenuRowTextRequest {
                descriptor: &mut descriptor,
                text,
                placement: MenuTextPlacement::Left,
                context_origin_pixels: [-123_456, 789_012],
            })
            .unwrap();
        assert_eq!(
            menu.scene.pixels.frame.rgba(),
            raw_pixels,
            "variant {variant}"
        );
        assert_eq!(menu.published_origin_pixels(), [-123_456, 789_012]);
        assert_eq!(descriptor.position_pixels[1] as i32, raw.position_pixels[1]);
        assert_eq!(outcome.advance_from_origin, raw.position_pixels[0] - 30);
        assert_eq!(menu.state.rows, 1);
        assert_eq!(menu.state.glyph_passes, text.len() * 4);
    }
}

fn corpus() -> (ResidentRuntime, FrontendAssets, PrimitiveTables) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    (runtime, assets, tables)
}

fn select(runtime: &mut ResidentRuntime, difficulty: u8) {
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
}

fn process(resources: RenderResources<'_>) -> FrontendProcessState {
    let process = FrontendProcessState::default();
    process.publish_colors(MenuDrawColors {
        selected_start: resources.style(3).unwrap(),
        ordinary: resources.style(2).unwrap(),
        selected_end: resources.style(2).unwrap(),
    });
    process
}

fn setup(dimensions: [i32; 2]) -> MenuControllerDrawSetup {
    MenuControllerDrawSetup {
        surface_pixels: dimensions,
        context_4c: 0,
        context_64: 2048,
        context_68: 4096,
        light_direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
        light_color: [128; 3],
        ambient_color: [32; 3],
    }
}

fn row(argument: u32) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: crate::menu::MenuSource::AuthoredDefinition {
            address: 0x53c088,
            span: inf_formats::ByteSpan::new(0, 20),
        },
        layout_index: 0,
        item_index: 0,
        callback_address: 0x453a80,
        arguments: [argument, 0],
        context_origin_pixels: [100_000, -90_000],
        placement: MenuDrawPlacement::Linear,
        descriptor: MenuRowDescriptor {
            position_pixels: [30, 40],
            extent_pixels: [280, 200],
            reveal: u16::MAX,
            flags: 0x40,
            age_millis: 10_000,
            color: MenuColor::from_rgb([128; 3]),
        },
    }
}

struct Tick(MenuBasisMath);
impl MenuHost for Tick {
    type Error = Infallible;
    fn action(&mut self, _: MenuActionRequest, _: &mut MenuStack) -> Result<(), Self::Error> {
        panic!("menu draw must not trigger actions")
    }
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.0.rotate(basis, angle);
        Ok(())
    }
}

#[test]
fn root_and_quit_labels_draw_actual_pixels_at_every_authored_display_tier() {
    let (mut runtime, assets, tables) = corpus();
    let dimensions = [[320, 240], [512, 384], [640, 480], [800, 600], [1024, 768]];
    for (variant, dimensions) in dimensions.into_iter().enumerate() {
        select(&mut runtime, variant as u8);
        let resources = RenderResources::new(&runtime);
        let startup = frontend_startup(
            &runtime,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let process = process(resources);
        let mut frame = SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        let mut control = FrontendControl::new(FrontendControlConfig {
            camera_presets: assets.camera_presets().to_vec(),
            position_scale: assets.position_scale(),
            motion_scale: assets.motion_scale(),
            entry: FrontendEntry::Returning,
            context: 1,
        })
        .unwrap();
        control.menus_mut().push(MenuState::new(
            MenuData::new(assets.root_menu().clone()),
            MenuBasis::INITIAL,
        ));
        let mut tick = Tick(MenuBasisMath::new(*tables.sine_table));
        control.menus_mut().tick(16_000, &mut tick).unwrap();
        let initial_clip = scene.pixels.pipeline.viewport;
        let initial_view = scene.environment.view;
        let initial_fade = scene.environment.projection.fade;
        let initial_fog = scene.environment.fog_factor;
        let mut state = SoftwareMenuState::default();
        let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut scene,
            resources,
            focal: startup.projection.focal,
            process: &process,
            state: &mut state,
        })
        .unwrap();
        let input = MenuDrawInput {
            surface_pixels: dimensions.map(|v| v as i32),
            visibility: MenuVisibility::OwnerFlagClear,
            sine_quarter: &assets.sine_table().values,
        };
        // The real first draw unlocks menu ticking. Never manufacture +25;
        // complete the original age/reveal policy after this acknowledged draw.
        control.draw_menu(input, &mut menu).unwrap().unwrap();
        for _ in 0..100 {
            control.menus_mut().tick(16_000, &mut tick).unwrap();
        }
        let outcome = control.draw_menu(input, &mut menu).unwrap().unwrap();
        assert!(outcome.menu_drawn);
        assert_eq!(outcome.rows, 3);
        assert_eq!(state.rows, 6);
        assert!(state.glyph_passes > 30);
        assert!(scene.pixels.stats.effect_quads > 15);
        assert!(scene.pixels.stats.fragments > 100);
        assert_eq!(scene.pixels.stats.unsupported, 0);
        assert_eq!(scene.pixels.pipeline.viewport, initial_clip);
        assert_eq!(scene.environment.view, initial_view);
        assert_eq!(scene.environment.projection.fade, initial_fade);
        assert_eq!(scene.environment.fog_factor, initial_fog);
        assert_eq!(scene.environment.projection.focal, startup.projection.focal);
        assert!(scene
            .pixels
            .frame
            .rgba()
            .chunks_exact(4)
            .any(|p| p != [17, 23, 31, 255]));

        control.menus_mut().push(MenuState::new(
            MenuData::new(assets.menu_definition(0x0053c130).unwrap()),
            MenuBasis::INITIAL,
        ));
        scene.pixels.frame.clear_color([17, 23, 31, 255]);
        let before = state.glyph_passes;
        let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut scene,
            resources,
            focal: startup.projection.focal,
            process: &process,
            state: &mut state,
        })
        .unwrap();
        control.draw_menu(input, &mut menu).unwrap().unwrap();
        for _ in 0..100 {
            control.menus_mut().tick(16_000, &mut tick).unwrap();
        }
        let outcome = control.draw_menu(input, &mut menu).unwrap().unwrap();
        assert!(outcome.menu_drawn && outcome.rows > 0);
        assert!(state.glyph_passes > before + 20);
        assert_eq!(scene.pixels.pipeline.viewport, initial_clip);
        assert!(scene
            .pixels
            .frame
            .rgba()
            .chunks_exact(4)
            .any(|p| p != [17, 23, 31, 255]));
    }
}

#[test]
fn glyph_packet_preserves_absolute_words_pass_order_and_unknown_color_byte() {
    let (mut runtime, _, _) = corpus();
    select(&mut runtime, 2);
    let resources = RenderResources::new(&runtime);
    let font = resources.font(0).unwrap();
    let argument = font.sprite_words[usize::from(b'A')] as i32;
    let corners = [[-32768, 32767], [-11, 32767], [-11, -23], [-32768, -23]];
    for (pass, color) in [
        (MenuGlyphPass::Base, MenuColor::from_word(0x00808080)),
        (MenuGlyphPass::Color, MenuColor::from_rgb([15, 97, 203])),
    ] {
        let command = glyph_command(
            resources,
            MenuGlyphQuad {
                sprite_argument: argument,
                corners,
                context_origin_pixels: [i32::MAX, i32::MIN],
                pass,
                color,
            },
        )
        .unwrap();
        for (index, [x, y]) in corners.into_iter().enumerate() {
            assert_eq!(
                command.read_u32(index * 8),
                Some(u32::from(x as u16) | (u32::from(y as u16) << 16))
            );
            assert_eq!(command.read_u32(index * 8 + 4), Some(0x40));
        }
        assert_eq!(command.read_u32(0x20), Some(pass as u32));
        assert_eq!(command.texture.unwrap().1.request.argument, argument);
        assert_eq!(command.payload[0x28..0x2b], color.rgb.map(Some));
        assert_eq!(command.payload[0x2b], color.high_byte);
        assert!(command.payload[0x2c..].iter().all(Option::is_none));
    }
}

#[test]
fn row_origin_is_published_without_translating_text_and_failures_remain_explicit() {
    let (mut runtime, assets, tables) = corpus();
    select(&mut runtime, 2);
    let resources = RenderResources::new(&runtime);
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let process = process(resources);
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    scene.environment.lighting.ambient_word = 0xa1030201;
    scene.primitive_state.effect_color = 0xb4000000;
    let mut state = SoftwareMenuState::default();
    let mut menu_state = MenuState::new(
        MenuData::new(assets.root_menu().clone()),
        MenuBasis::INITIAL,
    );
    let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
        scene: &mut scene,
        resources,
        focal: startup.projection.focal,
        process: &process,
        state: &mut state,
    })
    .unwrap();
    let mut request = row(assets.root_menu().items[0].draw_arguments[0]);
    assert!(matches!(
        menu.draw_row(&mut request, &mut menu_state),
        Err(SoftwareMenuError::Context)
    ));
    menu.begin_controller(setup([640, 480])).unwrap();
    assert_eq!(menu.scene.primitive_state.effect_color, 0xb4434241);
    assert_eq!(menu.scene.environment.lighting.ambient_word, 0xa1202020);
    assert_eq!(menu.state.context.unwrap().origin, [320, 0]);
    process.publish_colors(MenuDrawColors {
        selected_start: 11,
        ordinary: 12,
        selected_end: 13,
    });
    assert_eq!(menu.colors().ordinary, 12);
    menu.draw_row(&mut request, &mut menu_state).unwrap();
    assert_eq!(menu.scene.environment.projection.center, [100_000, -90_000]);
    assert_eq!(menu.state.context.unwrap().origin, [100_000, -90_000]);
    assert!(menu.scene.pixels.stats.fragments > 0);
    let written = menu.scene.pixels.frame.rgba().to_vec();
    let before = menu.state.glyph_passes;
    request.callback_address = 0x00448890;
    assert!(matches!(
        menu.draw_row(&mut request, &mut menu_state),
        Err(SoftwareMenuError::Text(MenuTextError::UnsupportedCallback(
            0x00448890
        )))
    ));
    assert_eq!(menu.state.glyph_passes, before);
    assert_eq!(menu.scene.pixels.frame.rgba(), written);
    request.callback_address = 0x453a80;
    request.descriptor.flags |= 8;
    menu.draw_row(&mut request, &mut menu_state).unwrap();
    assert_eq!(menu.state.glyph_passes, before);

    let options = assets.menu_definition(0x00539378).unwrap();
    let two_word_label = options
        .items
        .iter()
        .find(|item| {
            let text = resources.string(item.draw_arguments[0] as i32).unwrap();
            text.iter().filter(|&&b| b == b' ').count() == 1 && !text.contains(&b'-')
        })
        .unwrap();
    request.arguments = two_word_label.draw_arguments;
    request.descriptor.flags = 0x40;
    request.descriptor.position_pixels = [30, 40];
    request.descriptor.extent_pixels[0] = 0;
    request.descriptor.reveal = 0;
    menu.draw_row(&mut request, &mut menu_state).unwrap();
    assert_eq!(request.descriptor.position_pixels[1], 59); // 40 + (6477+1216)/400
    assert!(menu.state.glyph_passes > before);

    // The installed frontend descriptors have both passes. Selector-FF's
    // early suppression is covered with synthetic inputs by d3d_effect_tests;
    // do not claim that the font corpus exercised that descriptor policy.
}

#[derive(serde::Deserialize)]
struct NativeGlyphCase {
    position: [i16; 2],
    metric: [i16; 4],
    dimensions: [u16; 2],
    color: u32,
    origin: [u32; 2],
    draws: Vec<[u32; 11]>,
    returned: [u32; 2],
}

struct NativeFont {
    font: inf_formats::Font,
    dimensions: [u16; 2],
}
impl MenuTextResources for NativeFont {
    type Error = Infallible;
    fn string(&self, _: i32) -> Result<&[u8], Self::Error> {
        Ok(b"A")
    }
    fn font(&self, _: i32) -> Result<&inf_formats::Font, Self::Error> {
        Ok(&self.font)
    }
    fn style(&self, _: i32) -> Result<u32, Self::Error> {
        panic!("shadow disabled")
    }
    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        Ok(self.dimensions)
    }
}

struct NativePacketHost<'r> {
    resources: RenderResources<'r>,
    commands: Vec<PrimitiveCommand<'r>>,
}
impl MenuTextHost for NativePacketHost<'_> {
    type Error = RenderResourceError;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), Self::Error> {
        self.commands.push(glyph_command(self.resources, glyph)?);
        Ok(())
    }
}

fn compare_native_glyphs(cases: &[NativeGlyphCase]) {
    let (mut runtime, _, _) = corpus();
    select(&mut runtime, 2);
    let resources = RenderResources::new(&runtime);
    let sprite_argument = resources.font(0).unwrap().sprite_words[65];
    for case in cases {
        // Synthetic font values are inputs to the ORIGINAL 004A3B20/004A3E30
        // execution. Its lookup and+A4 are intercepted, not layout arithmetic.
        let mut font = resources.font(0).unwrap().clone();
        font.coordinate_divisor = 400;
        font.sprite_words[65] = sprite_argument;
        font.metrics[65] = inf_formats::GlyphMetrics {
            advance: case.metric[0],
            spacing: case.metric[1],
            offset: [case.metric[2], case.metric[3]],
        };
        let input = NativeFont {
            font,
            dimensions: case.dimensions,
        };
        let mut request = row(0);
        request.descriptor.position_pixels = case.position;
        request.descriptor.extent_pixels[0] = 500;
        request.descriptor.color = MenuColor::from_word(case.color);
        request.context_origin_pixels = case.origin.map(|v| v as i32);
        let mut host = NativePacketHost {
            resources,
            commands: vec![],
        };
        let outcome = draw_menu_text_row(&mut request, &input, &mut host).unwrap();
        assert_eq!(host.commands.len(), case.draws.len());
        for (actual, expected) in host.commands.iter().zip(&case.draws) {
            for (word, &expected) in expected.iter().enumerate() {
                if word == 9 {
                    continue;
                } // native pointer vs borrowed H3 relocation
                assert_eq!(
                    actual.read_u32(word * 4),
                    Some(expected),
                    "packet word {word}"
                );
            }
        }
        assert_eq!(
            request.descriptor.position_pixels[1] as i32 as u32,
            case.returned[1]
        );
        assert_eq!(
            outcome.advance_from_origin,
            (case.returned[0] as i32).wrapping_sub(i32::from(case.position[0]))
        );
    }
}

#[test]
fn glyph_packets_match_original_text_walker_receipts() {
    compare_native_glyphs(
        &serde_json::from_str::<Vec<NativeGlyphCase>>(include_str!(
            "../tests/fixtures/menu_glyph_native.json"
        ))
        .unwrap(),
    );
}

#[test]
#[ignore = "requires local original-PE oracle output"]
fn glyph_packets_match_all_128_original_text_walker_receipts() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/inf-menu-glyph-native.json");
    let cases: Vec<NativeGlyphCase> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 128);
    compare_native_glyphs(&cases);
}

#[derive(serde::Deserialize)]
struct NativeQuitCase {
    width: i32,
    original: [u32; 7],
    local: [u32; 7],
}

fn compare_native_quit(cases: &[NativeQuitCase]) {
    for case in cases {
        let mut request = row(123);
        request.callback_address = 0x483550;
        let original = case.original;
        request.descriptor = MenuRowDescriptor {
            position_pixels: [original[0] as i16, (original[0] >> 16) as i16],
            extent_pixels: [original[1] as i16, (original[1] >> 16) as i16],
            reveal: original[2] as u16,
            flags: original[3],
            age_millis: original[4] as i32,
            color: MenuColor::from_word(original[5]),
        };
        let mut expected = request;
        expected.callback_address = 0x4834f0;
        expected.descriptor.position_pixels[0] = case.local[0] as i16;
        let local = quit_label_request(request, case.width);
        assert_eq!(local, expected);
        assert_eq!(case.local[0] >> 16, original[0] >> 16);
        assert_eq!(case.local[1..], original[1..]);
    }
}

#[test]
fn quit_local_descriptor_matches_original_wrapper() {
    compare_native_quit(
        &serde_json::from_str::<Vec<NativeQuitCase>>(include_str!(
            "../tests/fixtures/menu_quit_native.json"
        ))
        .unwrap(),
    );
}

#[test]
#[ignore = "requires local original-PE oracle output"]
fn quit_local_descriptor_matches_all_256_original_wrappers() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/inf-quit-label-native.json");
    let cases: Vec<NativeQuitCase> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 256);
    compare_native_quit(&cases);
}
