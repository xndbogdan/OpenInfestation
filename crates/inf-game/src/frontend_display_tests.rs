//! Corpus checks require the retail installation, as enforced by the workspace
//! doctor. These assertions establish source ownership and startup policy, not
//! an observed post-title retail frame or rendered pixels.

use std::path::Path;

use inf_formats::RequestKind;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::{FrontendScene, FrontendSceneHost, SceneCommand, SceneFrame},
    Installation, ResidencyRequest, ResourceSelection,
};

// Literal H5 records in 0.ovl and H2 slots 0/1 in 0d0..0d4.ovl. These expected
// values are independent of the dimension selector and focal lookup under test.
const DISPLAY_TIERS: [([u32; 2], i32); 5] = [
    ([320, 240], 160),
    ([512, 384], 256),
    ([640, 480], 320),
    ([800, 600], 400),
    ([1024, 768], 512),
];

fn inactive_runtime() -> ResidentRuntime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    ResidentRuntime::new(Installation::open(&root).expect("retail Infestation data is required"))
        .unwrap()
}

fn select(runtime: &mut ResidentRuntime, variant: u8) {
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: variant,
                language: 0,
            },
        })
        .unwrap();
}

fn frontend_runtime(variant: u8) -> ResidentRuntime {
    let mut runtime = inactive_runtime();
    select(&mut runtime, variant);
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
}

#[test]
fn retail_display_tiers_resolve_authored_h2_focal_h5_dimensions_and_h7_clear() {
    let mut runtime = frontend_runtime(0);
    for (variant, (dimensions, focal)) in DISPLAY_TIERS.into_iter().enumerate() {
        select(&mut runtime, variant as u8);
        assert_eq!(frontend_display_variant(dimensions).unwrap(), variant as u8);

        let display = runtime.resource(5, variant as u16).unwrap();
        assert_eq!(display.registration.level, 0);
        assert_eq!(display.registration.phase, RequestKind::Base);
        let (_, HandlerResources::Records5(bank)) = display.overlay.handler(5).unwrap() else {
            panic!("H5 display record")
        };
        let record = &bank.records[usize::from(display.registration.local_index)];
        assert_eq!(record.words[..2], dimensions);
        assert_eq!(record.span.start, 34091 + variant * 20);

        for slot in 0..2 {
            let source = runtime.resource(2, slot).unwrap();
            assert_eq!(source.registration.level, 0);
            assert_eq!(source.registration.phase, RequestKind::Difficulty);
            assert_eq!(source.registration.local_index, slot);
            let (_, HandlerResources::Words(bank)) = source.overlay.handler(2).unwrap() else {
                panic!("H2 focal DWORD")
            };
            let word = &bank.entries[usize::from(source.registration.local_index)];
            assert_eq!(word.raw, focal as u32);
            assert_eq!(word.span.start, 4 + usize::from(slot) * 4);
            assert_eq!(word.span.end, 8 + usize::from(slot) * 4);
        }

        let clear = RenderResources::new(&runtime)
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Material,
                argument: 32,
            })
            .unwrap();
        assert_eq!(clear.source.registration.level, 0);
        assert_eq!(clear.source.registration.phase, RequestKind::Base);
        assert_eq!(clear.source.registration.handler, 7);
        assert_eq!(clear.source.registration.local_index, 32);
        let RenderResourceData::Material(material) = clear.data else {
            panic!("H7 clear material")
        };
        assert_eq!(material.raw(), 0);

        let startup = frontend_startup(
            &runtime,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        assert_eq!(startup.clear_rgba, [0, 0, 0, 255]);
        assert_eq!(startup.projection.focal, [focal; 2]);
        assert_eq!(startup.projection.viewport, dimensions.map(|v| v as i32));
        assert_eq!(startup.projection.fade, [0x10_0000; 2]);
        let projection = startup.scene.environment.projection;
        assert_eq!(projection.focal, [focal; 2]);
        assert_eq!(projection.viewport, dimensions.map(|v| v as i32));
        assert_eq!(projection.center, dimensions.map(|v| v as i32 / 2));
        assert_eq!(projection.fade, [0x10_0000; 2]);
        // Native right/bottom are inclusive dimensions, not width/height - 1.
        assert_eq!(
            startup.scene.pipeline.viewport,
            [0, 0, dimensions[0] as i16, dimensions[1] as i16]
        );
    }
}

#[test]
fn every_cross_tier_mismatch_is_rejected_without_rescaling_focal() {
    let mut runtime = frontend_runtime(0);
    for actual in 0..5 {
        select(&mut runtime, actual);
        for (expected, (dimensions, _)) in DISPLAY_TIERS.into_iter().enumerate() {
            if expected as u8 == actual {
                continue;
            }
            assert!(matches!(
                frontend_startup(&runtime, crate::presentation::NativeFrame::original(dimensions)),
                Err(FrontendDisplayError::Variant {
                    dimensions: found, expected: wanted, actual: selected,
                }) if found == dimensions && wanted == expected as u8 && selected == actual
            ));
        }
    }
}

#[test]
fn unsupported_dimensions_and_missing_resident_focal_have_no_fallback() {
    let mut runtime = inactive_runtime();
    select(&mut runtime, 2);
    for dimensions in [[0, 0], [640, 481], [1280, 1024], [u32::MAX, 480]] {
        assert!(matches!(
            frontend_display_variant(dimensions),
            Err(FrontendDisplayError::Dimensions(found)) if found == dimensions
        ));
        assert!(matches!(
            frontend_startup(&runtime, crate::presentation::NativeFrame::original(dimensions)),
            Err(FrontendDisplayError::Dimensions(found)) if found == dimensions
        ));
    }
    assert!(matches!(
        frontend_startup(
            &runtime,
            crate::presentation::NativeFrame::original([640, 480])
        ),
        Err(FrontendDisplayError::Focal(0))
    ));
}

#[derive(Default)]
struct SetupHost {
    commands: Vec<SceneCommand>,
}

impl FrontendSceneHost for SetupHost {
    type Error = ();

    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.commands.push(command);
        // Stop before stars/models: this recorder tests startup's handoff to
        // the actual scene setup without pretending to be a pixel backend.
        if matches!(command, SceneCommand::StarView(_)) {
            Err(())
        } else {
            Ok(())
        }
    }

    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        panic!("setup does not query footer resources")
    }

    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        panic!("setup uses the explicit projection viewport")
    }

    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        panic!("setup does not query sprite resources")
    }
}

#[test]
fn fresh_pe_state_reaches_scene_with_zero_view_and_exactly_one_focal_doubling() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infest.exe is required");
    let scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let mut runtime = frontend_runtime(0);
    for (variant, (dimensions, focal)) in DISPLAY_TIERS.into_iter().enumerate() {
        select(&mut runtime, variant as u8);
        let startup = frontend_startup(
            &runtime,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let config = &startup.scene;
        let environment = config.environment;
        assert_eq!(environment.view.eye, [0; 3]);
        assert_eq!(environment.view.basis.words, [0; 10]);
        assert_eq!(environment.coordinate_shift, 0);
        assert_eq!(environment.fog_factor, 0);
        assert_eq!(environment.lighting.direction, [0.; 3]);
        assert_eq!(environment.lighting.directional_word, 0);
        assert_eq!(environment.lighting.ambient, [0; 3]);
        assert_eq!(environment.lighting.ambient_word, 0);
        assert_eq!(
            (config.primitive_mode, config.far_color, config.effect_color),
            (0, 0, 0)
        );
        assert_eq!(config.renderer.near_depth, 64);
        assert_eq!(
            (config.renderer.depth_enable, config.renderer.depth_write),
            (1, 1)
        );
        assert_eq!(config.renderer.material_shifts, [6, 1, 4]);
        assert!(!config.texture_state.secondary_texture);

        let mut host = SetupHost::default();
        assert_eq!(
            scene.draw_scene(
                SceneFrame {
                    projection: startup.projection,
                    inherited_view: environment.view,
                    // The recorder stops before the camera is read/published.
                    camera_eye: [0; 3],
                    camera_look_at: [0; 3],
                    animation: AnimationClock::default(),
                    age_millis: 0,
                    exit_millis: 0,
                },
                &mut host,
            ),
            Err(())
        );
        assert_eq!(host.commands.len(), 6);
        let SceneCommand::Setup(setup) = host.commands[3] else {
            panic!("native setup precedes inherited star view")
        };
        assert_eq!(setup.focal, [focal * 2; 2]);
        assert_eq!(setup.viewport, dimensions.map(|v| v as i32));
        assert_eq!(setup.center, dimensions.map(|v| v as i32 / 2));
        assert_eq!(setup.fade, [0x10_0000; 2]);
        assert_eq!(setup.fade_factor, 0x100_0000);
        assert!(
            matches!(host.commands[5], SceneCommand::StarView(view) if view == environment.view)
        );
        assert_eq!(startup.projection.focal, [focal; 2]);
    }
}

#[test]
fn presentation_clear_resolves_h7_32_per_residency_or_forced_black() {
    for variant in 0..5 {
        let mut runtime = inactive_runtime();
        select(&mut runtime, variant);
        runtime.apply(ResidencyRequest::Boot).unwrap();
        let dimensions = DISPLAY_TIERS[variant as usize].0;
        let boot = presentation_clear_rgba(
            RenderResources::new(&runtime),
            PresentationClear::Standard,
            false,
        )
        .unwrap();
        assert_eq!(
            frontend_startup(
                &runtime,
                crate::presentation::NativeFrame::original(dimensions)
            )
            .unwrap()
            .clear_rgba,
            boot
        );
        runtime
            .apply(ResidencyRequest::Activate { level: 47 })
            .unwrap();
        // Leaf 47 does not re-register H7/32: the colour resolved at startup
        // before the frontend factory equals the per-call resolution after it.
        assert_eq!(
            presentation_clear_rgba(
                RenderResources::new(&runtime),
                PresentationClear::Standard,
                false
            )
            .unwrap(),
            boot
        );
        assert_eq!(
            presentation_clear_rgba(
                RenderResources::new(&runtime),
                PresentationClear::Standard,
                true
            )
            .unwrap(),
            [0, 0, 0, 255]
        );
    }
    assert!(matches!(
        presentation_clear_rgba(
            RenderResources::new(&inactive_runtime()),
            PresentationClear::Standard,
            false
        ),
        Err(RenderResourceError::NotResident { .. })
    ));
}

/// The gameplay screens' 00440930 hands 00438120 H7 slot 0x72, the world's
/// sky colour, which fills the open top of the sky dome. Infest01's
/// presentations read 2B58 on Level 1 and 6A48 in the intro (RGB565), the
/// same colours as these RGB555 words; the standard slot 0x20 stays black.
#[test]
fn gameplay_presentation_clears_with_the_world_sky_colour() {
    for (selection, word, rgba) in [
        (18, 0x15b8, [41, 107, 197, 255]),
        (22, 0x3528, [107, 74, 66, 255]),
    ] {
        let (resident, _, _) = crate::test_worlds::offline_world(
            selection,
            crate::terrain_runtime::TerrainSpacing::Units128,
        );
        let resources = RenderResources::new(&resident);
        assert_eq!(
            resident_material_word(resources, PresentationClear::Gameplay.slot()).unwrap(),
            word,
            "selection {selection}"
        );
        assert_eq!(
            presentation_clear_rgba(resources, PresentationClear::Gameplay, false).unwrap(),
            rgba
        );
        assert_eq!(
            presentation_clear_rgba(resources, PresentationClear::Standard, false).unwrap(),
            [0, 0, 0, 255]
        );
        assert_eq!(
            presentation_clear_rgba(resources, PresentationClear::Gameplay, true).unwrap(),
            [0, 0, 0, 255]
        );
    }
}
