use std::path::Path;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_display::frontend_startup,
    frontend_scene::SceneProjection,
    menu_text::MenuTextResources,
    model_primitives::PrimitiveTables,
    render_resources::RenderResources,
    software_frame::SoftwareFrame,
    software_scene::{SoftwareScene, SoftwareSceneRequest},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

fn inputs() -> BriefingMapInputs {
    BriefingMapInputs {
        frame: BriefingSceneFrame {
            projection: SceneProjection {
                focal: [320, 240],
                viewport: [640, 480],
                fade: [0x100000; 2],
            },
            age_millis: 2000,
            start_remaining_millis: 0,
            animation: AnimationClock::default(),
            selector_one: 1,
        },
        animation: BriefingPanelAnimation {
            current_stage: 2,
            requested_stage: 2,
            remaining_micros: 0,
            duration_micros: 500000,
            current_scale_word: 0,
            requested_scale_word: 0,
        },
        map_scale_word: 0,
        camera_xz_words: [0, 0],
    }
}

#[test]
fn map_tiles_keep_unsigned_wrap_thresholds_and_asymmetric_clip_animation() {
    let source = inputs();
    let plan = briefing_map_backdrop(source, 0xab123456).unwrap();
    assert_eq!(plan.clip_rect, [337, 27, 625, 243]);
    assert_eq!(
        plan.tiles[0].backdrop.corners,
        [[481, -121], [737, -121], [737, 135], [481, 135]]
    );
    assert_eq!(plan.tiles[1].backdrop.corners[0], [225, -121]);
    assert_eq!(plan.tiles[2].backdrop.corners[0], [481, 135]);
    for tile in plan.tiles {
        let quad = tile.backdrop;
        assert_eq!(quad.sprite_slot, 0x5b7);
        assert_eq!(quad.vertex_depths, [0x8000; 4]);
        assert_eq!(quad.descriptor_word, 0x100);
        assert_eq!(
            quad.mode,
            DirectSpriteMode::Effect {
                color_word: 0xab123456
            }
        );
    }
    for (camera, x_step, y_step) in [
        ([0x7ffe, 0x7fff], -256, 256),
        ([0x7fff, 0x8000], 256, -256),
        ([0xffff, 0xffff], 256, -256),
    ] {
        let plan = briefing_map_backdrop(
            BriefingMapInputs {
                camera_xz_words: camera,
                ..source
            },
            0,
        )
        .unwrap();
        assert_eq!(
            plan.tiles[1].backdrop.corners[0][0] - plan.tiles[0].backdrop.corners[0][0],
            x_step
        );
        assert_eq!(
            plan.tiles[2].backdrop.corners[0][1] - plan.tiles[0].backdrop.corners[0][1],
            y_step
        );
    }
    let mut opening = source;
    opening.animation.current_stage = 0;
    opening.animation.remaining_micros = 250000;
    assert_eq!(
        briefing_map_backdrop(opening, 0).unwrap().clip_rect,
        [409, 81, 553, 189]
    );
    opening.animation.current_stage = 1;
    opening.animation.requested_stage = 0;
    assert_eq!(
        briefing_map_backdrop(opening, 0).unwrap().clip_rect,
        [409, 81, 553, 189]
    );
    opening.animation.current_stage = 2;
    assert_eq!(
        briefing_map_backdrop(opening, 0).unwrap().clip_rect,
        plan.clip_rect
    );
    // C390 bypasses the countdown division outside the two transition
    // branches, so a fully open panel does not require a nonzero duration.
    opening.animation.duration_micros = 0;
    assert_eq!(
        briefing_map_backdrop(opening, 0).unwrap().clip_rect,
        plan.clip_rect
    );
    opening.animation.requested_stage = 2;
    assert_eq!(
        briefing_map_backdrop(opening, 0).unwrap().clip_rect,
        plan.clip_rect
    );
    opening.animation.current_stage = 0;
    opening.animation.requested_stage = 0;
    assert!(briefing_map_backdrop(opening, 0).is_none());
}

#[test]
fn original_map_backdrop_pixels_obey_native_clip_at_open_and_transition_gates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
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
        for (current, requested, remaining) in [(0, 2, 250000), (1, 0, 250000), (2, 2, 0)] {
            let mut source = inputs();
            source.frame.projection = startup.projection;
            source.animation.current_stage = current;
            source.animation.requested_stage = requested;
            source.animation.remaining_micros = remaining;
            // Synthetic screen camera precondition at the exact WORD wrap boundary.
            source.camera_xz_words = [0x7fff, 0x8000];
            source.map_scale_word = 0x8000;
            let plan = briefing_map_backdrop(source, resources.style(0x3b).unwrap()).unwrap();
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
            scene.pixels.pipeline.viewport = plan.clip_rect;
            for tile in plan.tiles {
                scene.draw_sprite_direct(tile.backdrop).unwrap();
            }
            let mut changed = 0;
            for (index, pixel) in scene.pixels.frame.rgba().chunks_exact(4).enumerate() {
                if pixel == [17, 23, 31, 255] {
                    continue;
                }
                changed += 1;
                let x = (index % dimensions[0] as usize) as i16;
                let y = (index / dimensions[0] as usize) as i16;
                assert!(
                    x >= plan.clip_rect[0]
                        && x <= plan.clip_rect[2]
                        && y >= plan.clip_rect[1]
                        && y <= plan.clip_rect[3],
                    "map pixels escaped native clip"
                );
            }
            assert!(
                changed > 0,
                "tier {tier} stage {current} must rasterize the original map backdrop"
            );
            assert_eq!(scene.stats.direct_sprites, 4);
            assert!(!scene.stopped());
        }
    }
}
