use std::path::Path;

use super::*;
use crate::frontend_assets::FrontendAssets;

#[derive(Default)]
struct Recording(Vec<SceneCommand>);

impl FrontendSceneHost for Recording {
    type Error = std::convert::Infallible;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.0.push(command);
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        panic!("panel frames do not own the frontend footer")
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        panic!("live dimensions are inputs to the frame")
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        panic!("panel frames submit models")
    }
}

fn math() -> MenuBasisMath {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    MenuBasisMath::new(FrontendAssets::open(&root).unwrap().sine_table().values)
}

/// Infest01's message-box frames (00455080 on a box's +4C panel, 1024x768,
/// focal words 512): the hint box at AF5E35:11EB and the line box at
/// B0D81D:14C1. At each call's end the panel keeps slot +38, position
/// +4..+C and basis +10..+34; 0055F22C holds focal x32, viewport and centre.
#[test]
fn message_box_frames_match_infest01() {
    let math = math();
    let projection = SceneProjection {
        focal: [512, 512],
        viewport: [1024, 768],
        fade: [0x800, 0x1000],
    };
    for (shape, state, center, basis) in [
        (
            PanelFrame {
                model_slot: PanelFrame::model_for(1),
                center_1024: [300, 640],
                maximum_extent_word: 0xa000,
                flag: 0,
                orientation: 1,
            },
            PanelAnimation {
                current_stage: 0,
                requested_stage: 2,
                remaining_micros: 0x1bd50,
                duration_micros: 0x249f0,
                current_scale_word: 0x4000,
                requested_scale_word: 0x4000,
            },
            [300, 480],
            [1, 0x3f80_0000, 0, 0, 0, 0, 0xbf80_0000, 0, 0x3f80_0000, 0],
        ),
        (
            PanelFrame {
                model_slot: PanelFrame::model_for(1),
                center_1024: [235, 0],
                maximum_extent_word: 0x5c00,
                flag: 0,
                orientation: 2,
            },
            PanelAnimation {
                current_stage: 0,
                requested_stage: 2,
                remaining_micros: 0x249f0,
                duration_micros: 0x249f0,
                current_scale_word: 0x100,
                requested_scale_word: 0x2d00,
            },
            [235, 0],
            [2, 0, 0xbf80_0000, 0, 0, 0, 0xbf80_0000, 0x3f80_0000, 0, 0],
        ),
    ] {
        let mut host = Recording::default();
        draw_panel_frame(
            shape,
            state,
            projection,
            AnimationClock::default(),
            &math,
            &mut host,
        )
        .unwrap();
        let SceneCommand::Setup(setup) = host.0[2] else {
            panic!("the frame publishes its context third")
        };
        assert_eq!(setup.focal, [0x4000, 0x4000]);
        assert_eq!(setup.viewport, [1024, 768]);
        assert_eq!(setup.center, center);
        let SceneCommand::Model(model) = host.0[5] else {
            panic!("the frame ends with its model")
        };
        assert_eq!(model.model_slot, 0x63);
        assert_eq!(model.position, [0, 0, 0x2580]);
        assert_eq!(model.basis.words, basis);
    }
}

/// 00455240: orientation 0 and 3 turn the frame by -0x4000 and 0x8000 in XZ;
/// any value other than 0, 2 and 3 leaves the YZ-turned basis alone.
#[test]
fn orientation_selects_the_native_xz_turn() {
    let math = math();
    let open = PanelAnimation {
        current_stage: 2,
        requested_stage: 2,
        remaining_micros: 0,
        duration_micros: 150_000,
        current_scale_word: 0x100,
        requested_scale_word: 0x100,
    };
    let basis = |orientation| {
        let mut host = Recording::default();
        draw_panel_frame(
            PanelFrame {
                model_slot: 99,
                center_1024: [0, 0],
                maximum_extent_word: 0x5c00,
                flag: 0,
                orientation,
            },
            open,
            SceneProjection {
                focal: [512, 512],
                viewport: [1024, 768],
                fade: [0x800, 0x1000],
            },
            AnimationClock::default(),
            &math,
            &mut host,
        )
        .unwrap();
        let SceneCommand::Model(model) = host.0[5] else {
            panic!("the frame ends with its model")
        };
        model.basis
    };
    let mut expected = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut expected, -0x4000, RotationPlane::Yz);
    for orientation in [1, 4, 0xffff_ffff] {
        assert_eq!(basis(orientation), expected);
    }
    for (orientation, angle) in [(0, -0x4000), (2, 0x4000), (3, 0x8000)] {
        let mut turned = expected;
        math.rotate_in_plane(&mut turned, angle, RotationPlane::Xz);
        assert_eq!(basis(orientation), turned);
    }
}
