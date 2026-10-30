use std::path::Path;

use inf_formats::{BriefingPayload, FrontendExecutable};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_display::frontend_startup,
    model_primitives::PrimitiveTables,
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    scene_resources::ResidentSceneHost,
    software_frame::SoftwareFrame,
    software_scene::{SoftwareScene, SoftwareSceneRequest},
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

#[derive(Default)]
struct Recording(Vec<SceneCommand>);

impl FrontendSceneHost for Recording {
    type Error = std::convert::Infallible;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.0.push(command);
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        panic!("briefing layers do not own the frontend footer")
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        panic!("live dimensions are inputs to the layer")
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        panic!("these briefing layers submit models")
    }
}

fn assets() -> FrontendAssets {
    FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap()
}

fn frame() -> BriefingSceneFrame {
    let mut animation = AnimationClock::default();
    animation.advance(40_000);
    BriefingSceneFrame {
        projection: SceneProjection {
            focal: [320, 240],
            viewport: [640, 480],
            fade: [0x10_0000; 2],
        },
        age_millis: 2000,
        start_remaining_millis: 0,
        animation,
        selector_one: 1,
    }
}

fn state(
    current_stage: u8,
    requested_stage: u8,
    remaining_micros: i32,
    scale: u16,
) -> BriefingPanelAnimation {
    BriefingPanelAnimation {
        current_stage,
        requested_stage,
        remaining_micros,
        duration_micros: 500_000,
        current_scale_word: scale,
        requested_scale_word: scale,
    }
}

#[test]
fn panel_callback_keeps_native_stage_asymmetry_and_countdown_quantization() {
    let shape = PanelFrame::briefing(BriefingPanel::Text);
    // Expected return WORDs follow the branches at 0045537F/3EA/44C. The
    // 250000/500000 shift-before-divide ratio is exactly244/488.
    for (current, requested, remaining, expected) in [
        (0, 0, 0, [0xffff, 0xffff, 0]),
        (0, 2, 500_000, [0xffff, 0xffff, 0]),
        (0, 2, 250_000, [0xbfff, 0x7fff, 0]),
        (1, 2, 250_000, [0x8000, 0, 0x1400]),
        (2, 2, 0, [0x8000, 0, 0x2800]),
        (2, 0, 250_000, [0x8000, 0, 0x1400]),
        (1, 0, 250_000, [0xc000, 0x8000, 0]),
    ] {
        let submission = SceneModelSubmission {
            model_slot: 99,
            position: [0; 3],
            basis: MenuBasis::INITIAL,
            animation: frame().animation,
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            policy: panel_callback(shape, state(current, requested, remaining, 0x2800)),
        };
        assert_eq!(
            [1, 2, 3].map(|selector| submission.callback_value(selector).unwrap()),
            expected
        );
        assert_eq!(submission.callback_value(0), Some(2));
        for selector in [4, 5, 6, 0x40] {
            assert_eq!(submission.callback_value(selector), Some(0));
        }
    }
    let callback = panel_callback(shape, state(0, 2, 499_999, 0x2800));
    let ModelCallbackPolicy::Panel { unfold, .. } = callback else {
        unreachable!()
    };
    assert_eq!(unfold, 65535);
    let callback = panel_callback(shape, state(0, 2, 499_711, 0x2800));
    let ModelCallbackPolicy::Panel { unfold, .. } = callback else {
        unreachable!()
    };
    assert_eq!(unfold, (65535u32 * 487 / 488) as u16);
}

#[test]
fn frame_uses_authored_center_focal_rotation_and_closed_gate() {
    let math = MenuBasisMath::new(assets().sine_table().values);
    let mut host = Recording::default();
    draw_briefing_panel_frame(
        BriefingPanel::Header,
        state(0, 0, 0, 0x3000),
        frame(),
        &math,
        &mut host,
    )
    .unwrap();
    assert!(host.0.is_empty());
    draw_briefing_panel_frame(
        BriefingPanel::Header,
        state(2, 2, 0, 0x3000),
        frame(),
        &math,
        &mut host,
    )
    .unwrap();
    assert_eq!(host.0.len(), 6);
    let SceneCommand::DirectionalLight { direction, rgb } = host.0[0] else {
        unreachable!()
    };
    assert_eq!(
        direction,
        [-2365.0 / 4096.0, 2356.0 / 4096.0, -2356.0 / 4096.0]
    );
    assert_eq!(rgb, [128; 3]);
    let SceneCommand::Setup(setup) = host.0[2] else {
        unreachable!()
    };
    assert_eq!(setup.focal, [10240, 7680]);
    assert_eq!(setup.center, [62, 60]);
    assert!(host
        .0
        .iter()
        .all(|command| !matches!(command, SceneCommand::CoordinateShift(_))));
    assert!(matches!(host.0[3], SceneCommand::FarColor(0)));
    let SceneCommand::Model(model) = host.0[5] else {
        unreachable!()
    };
    assert_eq!(model.model_slot, 99);
    assert_eq!(
        model.surface_table,
        crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault
    );
    assert_eq!(model.position, [0, 0, 9600]);
    assert_eq!(
        [1, 2, 3, 4].map(|selector| model.callback_value(selector).unwrap()),
        [0xc000, 0, 0x3000, 0]
    );
    let mut expected = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut expected, -16384, RotationPlane::Yz);
    assert_eq!(model.basis, expected);
}

#[test]
fn preview_keeps_signed_payload_units_and_decoration_uses_style_bytes() {
    let math = MenuBasisMath::new(assets().sine_table().values);
    let mut host = Recording::default();
    let authored = BriefingModel {
        model_slot: 7,
        position_xy: [-32768, 32767],
        depth_bias: -200,
        screen_shift_per_thousand: [-125, 125],
    };
    draw_briefing_model_preview(authored, 0, 1000, false, frame(), &math, &mut host).unwrap();
    let SceneCommand::Setup(setup) = host.0[2] else {
        unreachable!()
    };
    assert_eq!(setup.center, [170, 113]);
    assert_eq!(setup.fade, [400, 1300]);
    assert!(host
        .0
        .iter()
        .all(|command| !matches!(command, SceneCommand::CoordinateShift(_))));
    assert!(matches!(host.0[3], SceneCommand::FarColor(0)));
    let SceneCommand::PublishView(view) = host.0[4] else {
        unreachable!()
    };
    assert_eq!(view.eye, [0, 560, 0]);
    let SceneCommand::Model(model) = host.0[5] else {
        unreachable!()
    };
    assert_eq!(model.position, [-32768, 32767, 1400]);
    assert_eq!(
        model.surface_table,
        crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault
    );
    assert_eq!(
        [0, 1, 2, 6].map(|selector| model.callback_value(selector).unwrap()),
        [2, 1, 0xffff, 1]
    );
    host.0.clear();
    draw_briefing_decoration(1000, 0xab30_2010, frame(), &math, &mut host).unwrap();
    let SceneCommand::DirectionalLight { rgb, .. } = host.0[0] else {
        unreachable!()
    };
    assert_eq!(rgb, [0x10, 0x20, 0x30]);
    let SceneCommand::AmbientLight(rgb) = host.0[1] else {
        unreachable!()
    };
    assert_eq!(rgb, [4, 8, 12]);
    let SceneCommand::Setup(setup) = host.0[2] else {
        unreachable!()
    };
    assert_eq!(setup.center, [350, 487]);
    assert_eq!(setup.fade, [-1000, 2000]);
    assert!(host
        .0
        .iter()
        .all(|command| !matches!(command, SceneCommand::CoordinateShift(_))));
    assert!(matches!(host.0[3], SceneCommand::FarColor(0)));
    let SceneCommand::Model(model) = host.0[5] else {
        unreachable!()
    };
    assert_eq!(model.model_slot, 98);
    assert_eq!(
        model.surface_table,
        crate::frontend_scene::ModelSurfaceTableOrigin::InheritedRenderer
    );
    assert_eq!(model.position, [0, 0, 1000]);
}

#[test]
fn resident_briefing_models_render_pixels_at_open_and_transition_gates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = assets();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
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
            ..frame()
        };
        let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
        let program = inf_formats::parse_briefing_program(
            placements.world_source().overlay.bytes(),
            &placements.assets().worlds,
            placements.world(),
        )
        .unwrap();
        let models: Vec<_> = program
            .records
            .iter()
            .filter_map(|record| {
                if let BriefingPayload::Model(model) = record.payload {
                    Some(model)
                } else {
                    None
                }
            })
            .collect();
        assert!(!models.is_empty(), "Level1 has authored model records");
        assert_eq!(
            models
                .iter()
                .map(|model| model.model_slot)
                .collect::<Vec<_>>(),
            [117, 157, 250, 686, 294, 223, 62]
        );
        // 62's selector-8 writer (slot 118) returns 0043A2D0 stack residue,
        // and a real EE consumer keys a depth group on it. The writer leaves
        // the slot unknown and the group takes the farthest key
        // (CustomVertexStackResidueApproximation).
        let child = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 63,
            })
            .unwrap();
        let RenderResourceData::Model(child) = child.data else {
            unreachable!()
        };
        let writer = child
            .program
            .instructions
            .iter()
            .find(|i| i.span.start == 26670)
            .unwrap();
        assert_eq!(writer.raw_command, 0x222d);
        assert!(matches!(&writer.kind,
            inf_formats::ModelInstructionKind::UnresolvedDynamic(command)
                if command.arguments == [118, 8, 44, 0]));
        let reader = child
            .program
            .instructions
            .iter()
            .find(|i| i.span.start == 27328)
            .unwrap();
        assert_eq!(reader.raw_command, 0x00ee);
        assert!(matches!(&reader.kind,
            inf_formats::ModelInstructionKind::Opaque(command)
                if command.arguments == [118, 65336, 65535]));
        // 686's child copies a complete parent projection tail through 2A,
        // then consumes its fog byte in a far edge. A near parent's +15 is
        // unknown; this pins that custody boundary without inventing zero.
        // The native loader-zero scratch knows that stale byte, so the
        // production preview draws.
        let child = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 687,
            })
            .unwrap();
        let RenderResourceData::Model(child) = child.data else {
            unreachable!()
        };
        let copy = child
            .program
            .instructions
            .iter()
            .find(|i| i.span.start == 515970)
            .unwrap();
        assert_eq!(copy.raw_command, 0x0a2a);
        assert!(matches!(&copy.kind,
            inf_formats::ModelInstructionKind::UnresolvedDynamic(command)
                if command.arguments == [18, 1]));
        let edge = child
            .program
            .instructions
            .iter()
            .find(|i| i.span.start == 516128)
            .unwrap();
        assert!(matches!(&edge.kind,
            inf_formats::ModelInstructionKind::Edge(edge)
                if edge.vertices.map(|slot| slot.raw()) == [18, 16]));
        // Original constructors keep scale0 for the map/terrain/model frames,
        // 2800 for text and3000 for header. The callback, not a mesh patch,
        // controls both mid-transition and fully unfolded geometry.
        for (panel, scale) in [
            (BriefingPanel::Map, 0),
            (BriefingPanel::Terrain, 0),
            (BriefingPanel::Model, 0),
            (BriefingPanel::Text, 0x2800),
            (BriefingPanel::Header, 0x3000),
        ] {
            for (current, requested, remaining) in [(0, 0, 0), (1, 2, 250_000), (2, 2, 0)] {
                let mut pixels =
                    SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
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
                scene.environment.coordinate_shift = 8;
                scene.graphics.publish_shift(8).unwrap(); // explicit supplied fixture publication
                scene.primitive_state.far_color = 0x7fff;
                scene.begin_frame().unwrap();
                let mut host = ResidentSceneHost {
                    resources,
                    backend: &mut scene,
                };
                draw_briefing_panel_frame(
                    panel,
                    state(current, requested, remaining, scale),
                    frame,
                    &math,
                    &mut host,
                )
                .unwrap();
                host.submit(SceneCommand::FlushBatch).unwrap();
                let changed = host
                    .backend
                    .pixels
                    .frame
                    .rgba()
                    .chunks_exact(4)
                    .filter(|pixel| *pixel != [17, 23, 31, 255])
                    .count();
                if current == 0 {
                    assert_eq!(host.backend.environment.coordinate_shift, 8);
                    assert_eq!(host.backend.primitive_state.far_color, 0x7fff);
                    assert_eq!(changed, 0);
                    assert_eq!(host.backend.stats.models, 0);
                } else {
                    assert_eq!(host.backend.environment.coordinate_shift, 8);
                    assert_eq!(host.backend.primitive_state.far_color, 0);
                    assert!(
                        changed > 0,
                        "tier{tier} {panel:?} stage{current} must rasterize original H8 frame"
                    );
                    assert_eq!(host.backend.stats.models, 1);
                }
            }
        }
        let mut supported_previews = 0;
        let mut unsupported_previews = 0;
        let mut unexpected = Vec::new();
        for model in models {
            let resource = resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: i32::from(model.model_slot),
                })
                .unwrap();
            let RenderResourceData::Model(model_resource) = resource.data else {
                unreachable!()
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
            scene.set_terrain(Some(placements.terrain().clone()));
            scene.environment.coordinate_shift = 8;
            scene.graphics.publish_shift(8).unwrap(); // explicit supplied fixture publication
            scene.primitive_state.far_color = 0x7fff;
            // Synthetic process precondition, not a captured production seed.
            scene.set_process_rng(Some(0));
            scene.begin_frame().unwrap();
            let mut host = ResidentSceneHost {
                resources,
                backend: &mut scene,
            };
            let result = draw_briefing_model_preview(
                model,
                2000,
                model_resource.header.visibility_extent,
                false,
                frame,
                &math,
                &mut host,
            );
            assert_eq!(host.backend.environment.coordinate_shift, 8);
            assert_eq!(host.backend.primitive_state.far_color, 0);
            if let Err(error) = result {
                eprintln!(
                    "tier {tier} actual Level1 preview H8/{}: {error:?}",
                    model.model_slot
                );
                let expected = match (model.model_slot, &error) {
                    (
                        686,
                        crate::scene_resources::SceneResourceError::Backend(
                            crate::software_scene::SoftwareSceneError::Model(
                                crate::model_execution::ModelExecutionError::Host(
                                    crate::model_commands::ModelCommandError::Primitive(
                                        crate::model_primitives::PrimitiveError::Unknown {
                                            field: "fog byte",
                                            index: 18,
                                        },
                                    ),
                                ),
                            ),
                        ),
                    ) => true,
                    _ => false,
                };
                if expected {
                    unsupported_previews += 1;
                } else {
                    unexpected.push((model.model_slot, format!("{error:?}")));
                }
                continue;
            }
            assert!(
                matches!(model.model_slot, 117 | 157 | 223 | 250 | 294 | 62),
                "the preview gap matrix changed for H8/{}; update its evidence",
                model.model_slot
            );
            host.submit(SceneCommand::FlushBatch).unwrap();
            assert_eq!(host.backend.stats.models, 1);
            assert!(
                host.backend
                    .pixels
                    .frame
                    .rgba()
                    .chunks_exact(4)
                    .any(|pixel| pixel != [17, 23, 31, 255]),
                "tier {tier} authored model {} preview must rasterize",
                model.model_slot
            );
            eprintln!(
                "tier {tier} actual Level1 preview H8/{}: rasterized",
                model.model_slot
            );
            supported_previews += 1;
        }
        assert!(
            unexpected.is_empty(),
            "new Level1 preview boundaries: {unexpected:?}"
        );
        assert_eq!(unsupported_previews, 1);
        assert_eq!(supported_previews, 6);
        eprintln!("tier {tier} complete Level1 preview coverage: {supported_previews} rasterized, {unsupported_previews} unsupported");
        // A strict preview adapter pixel gate uses the original H8/98 mesh
        // with a synthetic payload. It establishes the B840 transform and
        // raster path, not completeness of any authored Level1 preview.
        let resource = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 98,
            })
            .unwrap();
        let RenderResourceData::Model(model_resource) = resource.data else {
            unreachable!()
        };
        let mut preview_pixels =
            SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
        let mut preview_scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut preview_pixels,
            config: startup.scene,
        })
        .unwrap();
        preview_scene.begin_frame().unwrap();
        let mut preview_host = ResidentSceneHost {
            resources,
            backend: &mut preview_scene,
        };
        draw_briefing_model_preview(
            BriefingModel {
                model_slot: 98,
                position_xy: [0, 0],
                depth_bias: 0,
                screen_shift_per_thousand: [0, 0],
            },
            2000,
            model_resource.header.visibility_extent,
            false,
            frame,
            &math,
            &mut preview_host,
        )
        .unwrap();
        preview_host.submit(SceneCommand::FlushBatch).unwrap();
        assert!(
            preview_host
                .backend
                .pixels
                .frame
                .rgba()
                .chunks_exact(4)
                .any(|pixel| pixel != [17, 23, 31, 255]),
            "tier {tier} original mesh under the synthetic preview payload must rasterize"
        );
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
        let mut host = ResidentSceneHost {
            resources,
            backend: &mut scene,
        };
        let resource = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 98,
            })
            .unwrap();
        let RenderResourceData::Model(model_resource) = resource.data else {
            unreachable!()
        };
        let resource = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Style,
                argument: 5,
            })
            .unwrap();
        let RenderResourceData::Style(style) = resource.data else {
            unreachable!()
        };
        // 7500 ms represents sixty 125000-us replay ticks. It is a
        // synthetic timer precondition, not a captured screen-age value.
        for (age_millis, inherited_shift) in [2000, 7500]
            .into_iter()
            .flat_map(|age| [0, 8, 16].map(|shift| (age, shift)))
        {
            host.submit(SceneCommand::BeginBatch).unwrap();
            // Synthetic inherited global-shift precondition. AD00/AAF0 do
            // not overwrite0055F250 while preparing their Far commands.
            host.backend.environment.coordinate_shift = inherited_shift;
            host.backend
                .graphics
                .publish_shift(inherited_shift)
                .unwrap(); // explicit supplied fixture publication
            host.backend.primitive_state.far_color = 0x7fff;
            host.backend.pixels.frame.clear_color([17, 23, 31, 255]);
            host.backend.pixels.frame.clear_depth();
            let effects_before = host.backend.pixels.stats.effect_quads;
            draw_briefing_decoration(
                model_resource.header.visibility_extent,
                style,
                BriefingSceneFrame {
                    age_millis,
                    ..frame
                },
                &math,
                &mut host,
            )
            .unwrap();
            assert_eq!(host.backend.environment.coordinate_shift, inherited_shift);
            assert_eq!(host.backend.primitive_state.far_color, 0);
            host.submit(SceneCommand::FlushBatch).unwrap();
            assert!(!host.backend.stopped());
            assert!(host.backend.pixels.stats.effect_quads > effects_before);
            assert!(
                host.backend.pixels.frame.rgba().chunks_exact(4)
                    .any(|pixel| pixel != [17, 23, 31, 255]),
                "tier {tier} original decoration at age {age_millis} must rasterize its Far effects",
            );
        }
        assert_eq!(host.backend.stats.models, 6);
    }
}

/// Every world's authored kind-2 previews at tier 2, 640x480, on the native
/// loader-zero scratch, each after the map panel's frame as the briefing
/// draws them. Residue-fed custom vertices (CustomVertexStackResidue
/// Approximation) no longer stop any preview, and nemian H8/308 draws its D4
/// quads. Thumper H8/46 (leaves 12, 20, 25) interpolates by register 1
/// before writing it; the map frame's context bases set it to 0
/// (`ModelStackRegisterPolicy::BriefingPreviewMapFrameBases`), so it draws.
#[test]
fn every_world_briefing_preview_draws() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = assets();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let zero = executable.loader_zero_data(0x0056_4728, 0xfa004).unwrap();
    let dimensions = [640, 480];
    let mut thumpers = Vec::new();
    let mut drawn = 0;
    for leaf in 12u8..=34 {
        let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        resident
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty: 2,
                    language: 0,
                },
            })
            .unwrap();
        resident.apply(ResidencyRequest::Boot).unwrap();
        for level in [47, 48, leaf] {
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
            ..frame()
        };
        let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
        let program = inf_formats::parse_briefing_program(
            placements.world_source().overlay.bytes(),
            &placements.assets().worlds,
            placements.world(),
        )
        .unwrap();
        for record in &program.records {
            let BriefingPayload::Model(model) = record.payload else {
                continue;
            };
            let resource = resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: i32::from(model.model_slot),
                })
                .unwrap();
            let RenderResourceData::Model(model_resource) = resource.data else {
                unreachable!()
            };
            let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
            let mut scene = SoftwareScene::new(SoftwareSceneRequest {
                model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                    crate::model_scratch::ProcessModelScratch::native_loader_zero(&zero).unwrap(),
                    (startup.scene).environment,
                ),
                resources,
                tables: &tables,
                frame: &mut pixels,
                config: startup.scene,
            })
            .unwrap();
            scene.set_terrain(Some(placements.terrain().clone()));
            scene.environment.coordinate_shift = 8;
            scene.graphics.publish_shift(8).unwrap(); // explicit supplied fixture publication
            scene.primitive_state.far_color = 0x7fff;
            // Synthetic process precondition, not a captured production seed.
            scene.set_process_rng(Some(0));
            scene.begin_frame().unwrap();
            let mut host = ResidentSceneHost {
                resources,
                backend: &mut scene,
            };
            draw_briefing_model_preview(
                model,
                2000,
                model_resource.header.visibility_extent,
                true,
                frame,
                &math,
                &mut host,
            )
            .unwrap_or_else(|error| panic!("leaf {leaf} H8/{}: {error:?}", model.model_slot));
            host.submit(SceneCommand::FlushBatch).unwrap();
            assert!(
                host.backend
                    .pixels
                    .frame
                    .rgba()
                    .chunks_exact(4)
                    .any(|pixel| pixel != [17, 23, 31, 255]),
                "leaf {leaf} H8/{} preview must rasterize",
                model.model_slot
            );
            drawn += 1;
            if model.model_slot == 46 {
                thumpers.push(leaf);
            }
        }
    }
    assert_eq!(thumpers, [12, 20, 25]);
    assert_eq!(drawn, 100);
}
