use std::path::Path;

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_display::frontend_startup,
    frontend_screen::FrontendProcessState,
    menu::{MenuDrawColors, MenuVisibility},
    menu_text::MenuTextResources,
    model_primitives::PrimitiveTables,
    object_runtime::{MessageBoxKind, PlayerMessageMenus},
    render_resources::RenderResources,
    software_frame::SoftwareFrame,
    software_menu::{SoftwareMenuRequest, SoftwareMenuState},
    software_scene::{SoftwareScene, SoftwareSceneRequest},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

const BLANK: [u8; 4] = [17, 23, 31, 255];

struct Drawn {
    pixels: Vec<u8>,
    glyph_passes: usize,
    models: usize,
}

/// A line box over Level 1's resources at 1024x768: the frame is the
/// panel's one model, and the ring text draws only while its row points at
/// the ring.
#[test]
fn line_box_frames_itself_and_draws_its_ring_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 4,
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 28] {
        resident
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([1024, 768]),
    )
    .unwrap();
    let process = FrontendProcessState::default();
    process.publish_colors(MenuDrawColors {
        selected_start: resources.style(3).unwrap(),
        ordinary: resources.style(2).unwrap(),
        selected_end: resources.style(2).unwrap(),
    });
    let math = MenuBasisMath::new(assets.sine_table().values);
    let visibility = MenuVisibility::OwnerFlagSet;
    let draw = |message_box: &mut MessageBox| {
        let mut frame = SoftwareFrame::new(1024, 768, BLANK);
        let mut state = SoftwareMenuState::default();
        let models = {
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
            scene.begin_frame().unwrap();
            let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
                scene: &mut scene,
                resources,
                focal: startup.projection.focal,
                process: &process,
                state: &mut state,
            })
            .unwrap();
            draw_message_box(
                message_box,
                MenuDrawInput {
                    surface_pixels: [1024, 768],
                    visibility,
                    sine_quarter: &assets.sine_table().values,
                },
                &mut menu,
                MessageFrameInputs {
                    projection: startup.projection,
                    animation: AnimationClock::default(),
                    math: &math,
                },
            )
            .unwrap();
            scene.stats.models
        };
        Drawn {
            pixels: frame.rgba().to_vec(),
            glyph_passes: state.glyph_passes,
            models,
        }
    };

    let menus = PlayerMessageMenus::parse(&assets).unwrap();
    let mut lines = MessageBox::new(MessageBoxKind::Lines, &menus, 0).unwrap();
    lines
        .add_text(b"Collected Tokens - Use these", visibility)
        .unwrap();
    // 24 ticks of 25 ms: full fade after 21, panel stage 2 after 300 ms.
    for _ in 0..24 {
        lines.update(25_000, visibility, &math).unwrap();
    }
    assert_eq!(lines.stack.fade(), 65535);
    assert_eq!(lines.panel.current_stage, 2);
    // The rows age, revealing their text, only once a draw has set +25.
    let unrevealed = draw(&mut lines);
    assert!(lines.stack.drawn());
    assert_eq!(unrevealed.models, 1);
    assert_eq!(unrevealed.glyph_passes, 0);
    for _ in 0..20 {
        lines.update(25_000, visibility, &math).unwrap();
    }
    let text = draw(&mut lines);
    assert_eq!(text.models, 1);
    assert!(text.glyph_passes > 0);
    assert_eq!(unrevealed.pixels.len(), text.pixels.len());
    assert_ne!(unrevealed.pixels, text.pixels);

    // Unhooked rows (argument 1 clear) leave only the frame.
    for item in lines
        .stack
        .current()
        .unwrap()
        .data()
        .records_mut()
        .items
        .iter_mut()
    {
        item.draw_arguments[1] = 0;
    }
    let frame_only = draw(&mut lines);
    assert_eq!(frame_only.models, 1);
    assert_eq!(frame_only.glyph_passes, 0);
    assert!(frame_only
        .pixels
        .chunks_exact(4)
        .any(|pixel| pixel != BLANK));
    assert_eq!(frame_only.pixels, unrevealed.pixels);
}
