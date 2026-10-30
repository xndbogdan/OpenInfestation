use super::*;
use crate::{frontend_assets::FrontendAssets, frontend_camera::AnimationClock};

fn fixture() -> (BriefingTerrainInputs, MenuBasisMath) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    (
        BriefingTerrainInputs {
            frame: BriefingSceneFrame {
                projection: SceneProjection {
                    focal: [1000, 800],
                    viewport: [640, 480],
                    fade: [12, 34],
                },
                age_millis: 2000,
                start_remaining_millis: 0,
                animation: AnimationClock::default(),
                selector_one: 3,
            },
            animation: BriefingPanelAnimation {
                current_stage: 2,
                requested_stage: 2,
                remaining_micros: 0,
                duration_micros: 500_000,
                current_scale_word: 0,
                requested_scale_word: 0,
            },
            camera_position_words: [0xffff, 0x8000, 0x7fff],
            heading: 0,
        },
        MenuBasisMath::new(assets.sine_table().values),
    )
}

#[test]
fn pane_has_its_own_signed_camera_focal_viewport_and_fade() {
    let (inputs, math) = fixture();
    let pane = briefing_terrain_pane(inputs, &math).unwrap().unwrap();
    assert_eq!(pane.clip_rect, [417, 291, 633, 473]);
    assert_eq!(pane.context.focal, [340, 304]);
    assert_eq!(pane.context.center, [525, 382]);
    assert_eq!(pane.context.viewport, [633, 473]);
    assert_eq!(pane.context.fade, [0x1800, 0x1a00]);
    assert_eq!(pane.context.fade_factor, 32768);
    assert_eq!(pane.view.eye, [-1, -32768, 32767]);
    let mut expected = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut expected, 0x1000, RotationPlane::Yz);
    // BB40 invokes yaw even when its angle is zero, so both calls count.
    expected.words[0] = 2;
    assert_eq!(pane.view.basis, expected);
    assert_eq!(inputs.frame.projection.fade, [12, 34]);
}

#[test]
fn pane_opening_closing_and_closed_gate_preserve_native_quantization() {
    let (mut inputs, math) = fixture();
    inputs.animation.current_stage = 0;
    inputs.animation.requested_stage = 1;
    inputs.animation.remaining_micros = 375_000;
    let pane = briefing_terrain_pane(inputs, &math).unwrap().unwrap();
    assert_eq!(pane.clip_rect, [500, 361, 550, 403]);
    assert_eq!(pane.context.focal, [80, 72]);
    inputs.animation.current_stage = 1;
    inputs.animation.requested_stage = 0;
    let pane = briefing_terrain_pane(inputs, &math).unwrap().unwrap();
    assert_eq!(pane.clip_rect, [445, 315, 605, 449]);
    assert_eq!(pane.context.focal, [250, 224]);
    inputs.animation.duration_micros = 999;
    assert_eq!(
        briefing_terrain_pane(inputs, &math),
        Err(BriefingTerrainError::DurationDivision)
    );
    inputs.animation.current_stage = 0;
    assert_eq!(briefing_terrain_pane(inputs, &math), Ok(None));
}

#[test]
fn pane_keeps_raw_context_edges_separate_from_renderer_word_clip() {
    let (mut inputs, math) = fixture();
    inputs.frame.projection.viewport = [100_000, 600];
    let pane = briefing_terrain_pane(inputs, &math).unwrap().unwrap();
    assert_eq!(pane.context.center, [82_031, 477]);
    assert_eq!(pane.context.viewport, [99_031, 591]);
    assert_eq!(pane.clip_rect[2], 99_031_i32 as i16);
    assert_ne!(pane.context.viewport[0], i32::from(pane.clip_rect[2]));
}

#[test]
fn backdrop_uses_header_height_identity_basis_and_generic_callback() {
    let (inputs, math) = fixture();
    let pane = briefing_terrain_pane(inputs, &math).unwrap().unwrap();
    let backdrop = pane.backdrop(-3456, inputs.frame);
    assert_eq!(backdrop.position, [-1, -3456, 32767]);
    assert_eq!(backdrop.basis, MenuBasis::INITIAL);
    assert_eq!(backdrop.model_slot, 278);
    assert_eq!(backdrop.callback_value(1), Some(3));
    assert_eq!(backdrop.callback_value(2), Some(0xffff));
    assert_eq!(backdrop.callback_value(6), Some(1));
    assert_eq!(
        pane.far_fill(0x1234),
        DirectFlatQuad {
            corners: [[417, 291], [633, 291], [633, 473], [417, 473]],
            vertex_depths: [100_000; 4],
            descriptor_word: 0,
            color_word: 0x1234,
        }
    );
}
