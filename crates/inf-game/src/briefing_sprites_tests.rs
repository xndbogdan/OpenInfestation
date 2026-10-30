use std::path::Path;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_display::frontend_startup,
    frontend_scene::SceneProjection,
    model_primitives::PrimitiveTables,
    render_resources::RenderResources,
    software_frame::SoftwareFrame,
    software_scene::{SoftwareScene, SoftwareSceneRequest},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

fn assets() -> FrontendAssets {
    FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap()
}

fn frame(age_millis: i32) -> BriefingSceneFrame {
    BriefingSceneFrame {
        projection: SceneProjection {
            focal: [320, 240],
            viewport: [640, 480],
            fade: [0x10_0000; 2],
        },
        age_millis,
        start_remaining_millis: 0,
        animation: AnimationClock::default(),
        selector_one: 1,
    }
}

#[test]
fn background_keeps_native_rotation_transpose_wedges_and_quarter_style() {
    let math = MenuBasisMath::new(assets().sine_table().values);
    let quads = briefing_background_quads(frame(0), 0xab80_4020, &math);
    assert_eq!(quads[0].sprite_slot, 0x5b7);
    assert_eq!(
        quads[0].corners,
        [[-509, -82], [349, -82], [349, 562], [-509, 562]]
    );
    assert_eq!(
        quads[0].mode,
        DirectSpriteMode::Effect {
            color_word: 0xab20_1008
        }
    );
    assert_eq!(
        quads[1].corners,
        [[-677, 240], [-677, -208], [-80, -208], [-80, 240]]
    );
    assert_eq!(
        quads[2].corners,
        [[-80, -208], [517, -208], [517, 240], [-80, 240]]
    );
    assert_eq!(
        quads[3].corners,
        [[517, 240], [517, 688], [-80, 688], [-80, 240]]
    );
    assert_eq!(
        quads[4].corners,
        [[-80, 688], [-677, 688], [-677, 240], [-80, 240]]
    );
    for quad in quads {
        assert_eq!(quad.descriptor_word, 0);
        assert_eq!(quad.vertex_depths, [100_000; 4]);
    }
    let rotated = briefing_background_quads(frame(0x4000), 0, &math);
    assert_eq!(
        rotated[0].corners,
        [[349, -82], [349, 562], [-509, 562], [-509, -82]]
    );
}

#[test]
fn progress_uses_world_count_reveal_timing_and_start_suppression() {
    let table = assets().sine_table().values;
    for (age, count) in [(1999, 0), (2000, 1), (2499, 1), (2500, 2), (7000, 2)] {
        assert_eq!(
            briefing_progress_quads(frame(age), 2, 0xab80_4020, &table).len(),
            count
        );
    }
    assert!(briefing_progress_quads(frame(7000), 0x8000_0000, 0, &table).is_empty());
    let mut starting = frame(7000);
    starting.start_remaining_millis = 2000;
    assert!(briefing_progress_quads(starting, 2, 0, &table).is_empty());
    let quad = briefing_progress_quads(frame(65536), 1, 0xab80_4020, &table)[0];
    assert_eq!(quad.corners, [[75, 249], [99, 249], [99, 275], [75, 275]]);
    assert_eq!(quad.vertex_depths, [0x4000; 4]);
    assert_eq!(
        quad.mode,
        DirectSpriteMode::Effect {
            color_word: 0xab40_2010
        }
    );
}

#[test]
fn original_background_and_progress_sprites_reach_both_native_raster_paths() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = assets();
    let table = assets.sine_table().values;
    let math = MenuBasisMath::new(table);
    let tables = PrimitiveTables::from_executable(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        resident
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty: tier,
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
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let frame = BriefingSceneFrame {
            projection: startup.projection,
            ..frame(2500)
        };
        let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
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
        let style = |slot| {
            let resource = resources
                .resolve(crate::render_resources::RenderResourceRequest {
                    kind: crate::render_resources::RenderResourceKind::Style,
                    argument: slot,
                })
                .unwrap();
            let crate::render_resources::RenderResourceData::Style(word) = resource.data else {
                unreachable!()
            };
            word
        };
        for quad in briefing_background_quads(frame, style(0x3b), &math) {
            scene.draw_sprite_direct(quad).unwrap();
        }
        assert_eq!(scene.stats.direct_sprites, 5);
        assert_eq!(scene.stats.footers, 0);
        assert!(
            scene
                .pixels
                .frame
                .rgba()
                .chunks_exact(4)
                .any(|pixel| pixel != [17, 23, 31, 255]),
            "tier{tier} original background must rasterize"
        );
        let before = scene.pixels.frame.rgba().to_vec();
        for quad in briefing_progress_quads(frame, 2, style(5), &table) {
            scene.draw_sprite_direct(quad).unwrap();
        }
        assert_eq!(scene.stats.direct_sprites, 7);
        assert_ne!(
            scene.pixels.frame.rgba(),
            before,
            "tier{tier} original progress sprites must rasterize"
        );
        assert!(!scene.stopped());
    }
}
