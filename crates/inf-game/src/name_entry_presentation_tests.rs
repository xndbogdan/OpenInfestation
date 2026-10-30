use std::{fs::File, io::BufWriter, path::Path};

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_display::frontend_startup,
    menu::MenuColor,
    menu_basis::MenuBasisMath,
    menu_text::INITIAL_GLYPH_BASE_WORD,
    model_primitives::PrimitiveTables,
    name_entry_draw::{draw_name_entry_overlay, NameEntryDrawFrame, NameEntryDrawState},
    name_entry_scene::{draw_name_entry_scene, NameEntrySceneFrame},
    software_frame::SoftwareFrame,
    software_scene::SoftwareSceneRequest,
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

fn corpus() -> (ResidentRuntime, FrontendAssets, PrimitiveTables) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2, // Infestation's authored 640x480 display tier.
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    (resident, assets, tables)
}

#[test]
fn immediate_star_and_highlight_leave_backdrop_queue_and_projection_untouched() {
    let (resident, assets, tables) = corpus();
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [2, 3, 5, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    let mut host = SoftwareNameEntry::new(&mut scene, resources);
    host.submit(SceneCommand::BeginBatch).unwrap();
    draw_name_entry_scene(
        NameEntrySceneFrame {
            projection: startup.projection,
            age_millis: 9000,
            animation: AnimationClock::default(),
            selector_one: 1,
        },
        &MenuBasisMath::new(assets.sine_table().values),
        &mut host,
    )
    .unwrap();
    let queued = host.scene.queue.state();
    assert!(queued.command_count > 0);
    let projection = host.scene.environment.projection;
    let fragments = host.scene.pixels.stats.fragments;
    host.star(StarQuad {
        corners: [[10, 10], [14, 10], [14, 14], [10, 14]],
        color_argument: 0x3f,
    })
    .unwrap();
    assert!(host.scene.pixels.stats.fragments > fragments);
    assert_eq!(host.scene.queue.state(), queued);
    assert_eq!(host.scene.environment.projection, projection);
    assert_eq!(host.scene.stats.flushes, 0);
    assert_eq!(host.scene.stats.stars, 1);
    let fragments = host.scene.pixels.stats.fragments;
    host.highlight(FooterQuad {
        sprite_slot: 7,
        corners: [[200, 100], [300, 100], [300, 200], [200, 200]],
        style_word: 0x0080_8080,
        vertex_attribute: 0x4000,
    })
    .unwrap();
    assert!(host.scene.pixels.stats.fragments > fragments);
    assert_eq!(host.scene.queue.state(), queued);
    assert_eq!(host.scene.environment.projection, projection);
    assert_eq!(host.scene.stats.flushes, 0);
    assert_eq!(host.scene.stats.footers, 1);
    host.submit(SceneCommand::FlushBatch).unwrap();
    assert_eq!(host.scene.queue.state().command_count, queued.command_count);
    assert!(host.scene.queue.state().last_flush_completed > 0);
}

#[test]
fn resident_name_entry_overlay_reaches_concrete_pixels() {
    let (resident, assets, tables) = corpus();
    let resources = RenderResources::new(&resident);
    let sine = assets.sine_table().values;
    let math = MenuBasisMath::new(sine);
    let states = [
        ("editing", 0, 0, false),
        ("feedback", 0, 500, false),
        ("completing", 800, 0, false),
        ("complete", 1, 0, false),
        ("missing-error-resource", 0, 0, true),
    ];
    let mut earlier = None;
    for (label, completion_delay, feedback, error) in states {
        let startup = frontend_startup(
            &resident,
            crate::presentation::NativeFrame::original([640, 480]),
        )
        .unwrap();
        let clear = [2, 3, 5, 255];
        let mut pixels = SoftwareFrame::new(640, 480, clear);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        scene.begin_frame().unwrap();
        let mut host = SoftwareNameEntry::new(&mut scene, resources);
        host.submit(SceneCommand::BeginBatch).unwrap();
        draw_name_entry_scene(
            NameEntrySceneFrame {
                projection: startup.projection,
                age_millis: 9000,
                animation: AnimationClock::default(),
                selector_one: 1,
            },
            &math,
            &mut host,
        )
        .unwrap();
        let result = draw_name_entry_overlay(
            NameEntryDrawFrame {
                state: NameEntryDrawState {
                    name: b"Jameson",
                    selection: 0,
                    row_anim: 0,
                    completion_delay,
                    feedback,
                    error,
                },
                age_millis: 9000,
                surface_pixels: [640, 480],
                focal: startup.projection.focal,
                context_origin_pixels: [320, 240],
                base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
                sine_quarter: &sine,
            },
            &resources,
            &mut host,
        );
        if error {
            // H0/871 belongs to leaf1's Language bank (local179), which is
            // absent from the frontend's {0,47} residency. Loading leaf1
            // replaces47 and removes the backdrop models. The native H0
            // fallback (00527598 -> 004EFBF0) returns null; 004A4244 then
            // dereferences it. Report the missing resource explicitly.
            assert!(matches!(result,
                Err(crate::name_entry_draw::NameEntryDrawError::Resource(
                    RenderResourceError::NotResident { request }
                )) if request.kind == crate::render_resources::RenderResourceKind::String
                    && request.argument == 0x367
            ));
            assert_eq!(host.scene.stats.stars, 1000);
            assert_eq!(host.scene.stats.flushes, 1);
            assert_eq!(host.scene.stats.footers, 1);
            assert!(host.glyph_passes > 0);
            continue;
        }
        result.unwrap();
        assert_eq!(host.scene.stats.models, 3, "{label}");
        assert_eq!(host.scene.stats.stars, 1000, "{label}");
        assert_eq!(host.scene.stats.flushes, 1, "{label}");
        assert_eq!(
            host.scene.stats.footers,
            usize::from(completion_delay != 1),
            "{label}"
        );
        assert!(host.glyph_passes > 0, "{label}");
        assert_eq!(host.scene.pixels.stats.unsupported, 0, "{label}");
        // Native flush replays, but does not consume, the model arena.
        assert!(host.scene.queue.state().last_flush_completed > 0, "{label}");
        assert!(!host.scene.stopped(), "{label}");
        let rgba = host.scene.pixels.frame.rgba();
        let changed = rgba.chunks_exact(4).filter(|pixel| *pixel != clear).count();
        assert!(changed > 1000, "{label}: {changed} changed pixels");
        if let Some(previous) = earlier.replace(rgba.to_vec()) {
            assert_ne!(
                rgba, previous,
                "{label} must differ from the previous frame"
            );
        }
        // Optional visual output is diagnostic and remains outside versioned assets.
        if let Some(directory) = std::env::var_os("INF_NAME_ENTRY_OVERLAY_SMOKE_DIR") {
            let path = Path::new(&directory).join(format!("name-entry-{label}.png"));
            let mut encoder =
                png::Encoder::new(BufWriter::new(File::create(path).unwrap()), 640, 480);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(rgba).unwrap();
            writer.finish().unwrap();
        }
    }
}
