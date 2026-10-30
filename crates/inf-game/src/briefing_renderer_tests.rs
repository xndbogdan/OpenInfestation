use std::path::Path;

use inf_formats::{BriefingPayload, FrontendExecutable};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_display::frontend_startup,
    menu::MenuBasis,
    menu::MenuDrawColors,
    model_context::{ModelContext, ModelDispatch},
    model_primitives::PrimitiveTables,
    model_projection::ModelProjectionBackend,
    model_vertices::ModelVertices,
    software_frame::SoftwareFrame,
    software_scene::SoftwareSceneRequest,
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

// These isolated renderer fixtures supply a stable authored policy. They do not
// claim a process settings owner or native interleaved settings mutations.
struct FixtureObjectLightPolicy(crate::world_lighting::TerrainLightPolicy);

impl FixtureObjectLightPolicy {
    fn new(enabled: bool, spacing: crate::terrain_runtime::TerrainSpacing) -> Self {
        Self(crate::world_lighting::TerrainLightPolicy {
            enabled,
            spacing,
            color: crate::world_lighting::TerrainLightColor::Unmodified,
        })
    }
}

impl crate::object_runtime::ObjectLightPolicySource for FixtureObjectLightPolicy {
    fn dynamic_lighting(&mut self) -> bool {
        self.0.enabled
    }

    fn terrain_light_policy(&mut self) -> crate::world_lighting::TerrainLightPolicy {
        self.0
    }
}

#[path = "briefing_renderer_tests/annotation_pixels.rs"]
mod annotation_pixels;
#[path = "briefing_renderer_tests/batching.rs"]
mod batching;
#[path = "briefing_renderer_tests/construction_stop.rs"]
mod construction_stop;
#[path = "briefing_renderer_tests/lighting.rs"]
mod lighting;
#[path = "briefing_renderer_tests/water_pixels.rs"]
mod water_pixels;

/// Allocate the original H12 terrain instead of cloning the prepared runtime.
fn owned_terrain(placements: &WorldPlacements<'_>) -> crate::terrain_runtime::TerrainRuntime {
    let source = placements.terrain_source();
    let crate::HandlerResources::Terrain(bank) =
        &source.overlay.handlers()[source.registration.region_index]
    else {
        panic!("H12 terrain")
    };
    crate::terrain_runtime::TerrainRuntime::new(
        &bank.entries[usize::from(source.registration.local_index)],
        placements.terrain().sampling(),
    )
}

struct BriefingTestEffects<'a> {
    object_draw_process: crate::object_runtime::ObjectDrawProcessState,
    state: crate::terrain_furniture::draw::FurnitureDrawState,
    pending: crate::world_furniture_effect::FurnitureEffectQueue,
    lights: crate::world_lighting::WorldLightGrid,
    particles: crate::process_particles::ProcessParticleRuntime,
    executable: FrontendExecutable<'a>,
    process: crate::process_particles::factories::ParticleFactoryProcessState,
}
impl<'a> BriefingTestEffects<'a> {
    fn new(assets: &'a FrontendAssets) -> Self {
        Self {
            object_draw_process: crate::object_runtime::ObjectDrawProcessState::native_startup(
                FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            )
            .unwrap(),
            state: crate::terrain_furniture::draw::FurnitureDrawState::new(
                &inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values),
            ),
            pending: Default::default(),
            lights: Default::default(),
            particles: Default::default(),
            executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            process: crate::process_particles::factories::ParticleFactoryProcessState {
                network_flag: 0,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0)),
            },
        }
    }
    fn inputs(&mut self) -> BriefingProcessEffects<'_> {
        BriefingProcessEffects {
            pickups: None,
            firing: None,
            object_draw_process: &mut self.object_draw_process,
            attract_demo: &false,
            state: &mut self.state,
            pending: &self.pending,
            light_grid: &mut self.lights,
            particles: &mut self.particles,
            executable: self.executable,
            // These isolated renderer fixtures have no installed actor registry.
            // Supplied source0 and offline phase3 are explicit test inputs.
            birth_inputs: crate::process_particles::births::ParticleBirthInputs {
                network_active: false,
                phase: Some(3),
                process: &self.process,
            },
            load_factor: 4,
        }
    }
}

#[test]
fn native_terrain_backdrop_draws_original_model_in_its_screen_camera_context() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    let math = MenuBasisMath::new(assets.sine_table().values);
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
        let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
        let mut presentation =
            crate::world_backend::load_world_presentation(&resident, &placements).unwrap();
        let startup = frontend_startup(
            &resident,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
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
        let mut state = FrontendRendererState::default();
        let mut arena = crate::object_runtime::ObjectArena::new();
        let map_state = crate::world_map::WorldMapState::new();
        let mut terrain = owned_terrain(&placements);
        let mut effects = BriefingTestEffects::new(&assets);
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(true, terrain.sampling().spacing);
        let mut owner = SoftwareBriefingFrame {
            scene: &mut scene,
            resources,
            sine_quarter: &assets.sine_table().values,
            template_projection: startup.projection,
            state: &mut state,
            selector_one: 1,
            world: BriefingWorld {
                residency: &resident,
                terrain_source: &crate::world_actor_terrain::WorldTerrainSource::retain(placements.terrain_source()),
                arena: &mut arena,
                map_state: &map_state,
                terrain: &mut terrain,
                presentation: &mut presentation,
                world_flags: placements.world().flags,
                network_active: false,
            },
            effects: effects.inputs(),
            dynamic_lighting: true,
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
            infrared: false,
        };
        let frame = BriefingSceneFrame {
            projection: startup.projection,
            age_millis: 2000,
            start_remaining_millis: 0,
            animation: AnimationClock::default(),
            selector_one: 1,
        };
        let inputs = BriefingTerrainInputs {
            frame,
            animation: BriefingPanelAnimation {
                current_stage: 2,
                requested_stage: 2,
                remaining_micros: 0,
                duration_micros: 500_000,
                current_scale_word: 0,
                requested_scale_word: 0,
            },
            camera_position_words: [0x1000, 400, 0x2000],
            heading: 0x1234,
        };
        let pane = briefing_terrain_pane(inputs, &math).unwrap().unwrap();
        owner.begin(true).unwrap();
        owner.terrain_backdrop(pane, frame).unwrap();
        assert_eq!(owner.scene.stats.models, 1, "original H8/278 must execute");
        assert!(owner.scene.stats.model_commands > 0);
        assert_eq!(owner.scene.stats.direct_flat_quads, 1);
        assert_eq!(owner.scene.environment.view, pane.view);
        assert_eq!(owner.scene.environment.projection.focal, pane.context.focal);
        assert_eq!(
            owner.scene.environment.projection.fade,
            [i32::MAX; 2],
            "BB40 does not restore these globals before AA10"
        );
        assert_eq!(owner.scene.environment.coordinate_shift, 16);
        let clip = pane.clip_rect;
        let mut changed = 0;
        for (index, pixel) in owner.scene.pixels.frame.rgba().chunks_exact(4).enumerate() {
            let x = (index % dimensions[0] as usize) as i32;
            let y = (index / dimensions[0] as usize) as i32;
            if pixel != [17, 23, 31, 255] {
                changed += 1;
                assert!(
                    x >= i32::from(clip[0])
                        && x <= i32::from(clip[2])
                        && y >= i32::from(clip[1])
                        && y <= i32::from(clip[3])
                );
            }
        }
        assert!(
            changed > 0,
            "native pane must draw pixels at {dimensions:?}"
        );
    }
}

#[test]
fn original_camera_static_rasterizes_clipped_strips_and_commits_native_rng_order() {
    use crate::name_entry::name_entry_rng_next;
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        let pixels = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                let pane = briefing_terrain_pane(
                    BriefingTerrainInputs {
                        frame,
                        animation: open_panel(),
                        camera_position_words: [0; 3],
                        heading: 0,
                    },
                    math,
                )
                .unwrap()
                .unwrap();
                owner.scene.pixels.pipeline.viewport = pane.clip_rect;
                let mut expected = 0;
                let mut count = 0;
                for _ in pane.clip_rect[1]..pane.clip_rect[3] {
                    let mut x = i32::from(pane.clip_rect[0]);
                    while x < i32::from(pane.clip_rect[2]) {
                        let slot = 0xbf + ((name_entry_rng_next(&mut expected) >> 4) & 3) as u16;
                        let width = owner.resources.sprite_dimensions(i32::from(slot)).unwrap()[0];
                        assert!(width > 0);
                        name_entry_rng_next(&mut expected);
                        x += i32::from(width);
                        count += 1;
                    }
                }
                owner
                    .terrain_static(pane.clip_rect, BriefingNoiseRows::All)
                    .unwrap();
                assert_eq!(owner.scene.process_rng(), Some(expected));
                assert_eq!(owner.scene.stats.direct_sprites, count);
                assert_eq!(
                    owner.scene.stats.models, 0,
                    "camera disabled draws no world models"
                );
            },
        );
        let mut changed = 0;
        let clip = [
            dimensions[0] as i32 * 0x348 >> 10,
            dimensions[1] as i32 * 0x32f >> 10,
        ];
        let half = [
            dimensions[0] as i32 * 34 / 100 / 2,
            dimensions[1] as i32 * 38 / 100 / 2,
        ];
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            if pixel == PIXEL_GATE_CLEAR {
                continue;
            }
            let x = (index % dimensions[0] as usize) as i32;
            let y = (index / dimensions[0] as usize) as i32;
            assert!(x >= clip[0] - half[0] && x <= clip[0] + half[0]);
            assert!(y >= clip[1] - half[1] && y <= clip[1] + half[1]);
            changed += 1;
        }
        assert!(
            changed > 0,
            "original camera-static sprites must draw at tier{tier}"
        );
    }
}

#[test]
fn native_briefing_ground_consumes_retained_pane_context_after_backdrop_fade_override() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                let pane = briefing_terrain_pane(
                    BriefingTerrainInputs {
                        frame,
                        animation: open_panel(),
                        camera_position_words: [0x1000, 400, 0x2000],
                        heading: 0x1234,
                    },
                    math,
                )
                .unwrap()
                .unwrap();
                owner.terrain_backdrop(pane, frame).unwrap();
                assert_eq!(owner.scene.environment.projection.fade, [i32::MAX; 2]);
                let before = owner.scene.pixels.frame.rgba().to_vec();
                let depth_resets = owner.scene.stats.depth_resets;
                owner
                    .terrain_ground(pane, frame, false, &FrontendProcessState::default(), false)
                    .unwrap();
                assert_eq!(
                    owner.scene.stats.depth_resets,
                    depth_resets + 1,
                    "0040C21C clears depth before the camera-static overlay"
                );
                assert_eq!(
                    owner.scene.environment.projection.fade,
                    [8320, 9984],
                    "AA10 far intervals derive from retained pane fade [0x1800,0x1a00]"
                );
                assert_eq!(owner.scene.environment.view, pane.view);
                assert_eq!(owner.scene.environment.projection.focal, pane.context.focal);
                assert!(
                    owner.scene.pixels.stats.d6_quads + owner.scene.pixels.stats.colored_quads > 0,
                    "native ground consumers must execute at tier{tier}"
                );
                assert!(before.chunks_exact(4).zip(owner.scene.pixels.frame.rgba().chunks_exact(4))
                .any(|(before, after)| before != after),
                "shared native ground must contribute pixels beyond backdrop/fill at tier{tier}");
            },
        );
    }
}

#[test]
fn disabled_camera_pane_draws_original_blinking_label_under_its_frame() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let literal = executable
            .mapped(0x0052_f1d4, CAMERA_DISABLED_TEXT.len() + 1)
            .unwrap();
        assert_eq!(
            &literal.bytes[..CAMERA_DISABLED_TEXT.len()],
            CAMERA_DISABLED_TEXT
        );
        assert_eq!(literal.bytes[CAMERA_DISABLED_TEXT.len()], 0);
        let mut draws = Vec::new();
        let mut noise_rng = None;
        for blinking in [false, true] {
            draws.push(level1_layer_pixels(
                &assets,
                &tables,
                &resident,
                dimensions,
                |owner, mut frame, math| {
                    // Hold screen age fixed: the blink consumes005471C0,
                    // whose counter advances once per20000 microseconds.
                    frame.age_millis = 2000;
                    if blinking {
                        frame.animation.advance(500_000);
                    }
                    let process = FrontendProcessState::default();
                    process.publish_colors(MenuDrawColors {
                        selected_start: owner.resources.style(4).unwrap(),
                        ordinary: owner.resources.style(5).unwrap(),
                        selected_end: owner.resources.style(6).unwrap(),
                    });
                    owner
                        .terrain_panel(
                            BriefingTerrainInputs {
                                frame,
                                animation: open_panel(),
                                camera_position_words: [0; 3],
                                heading: 0,
                            },
                            BriefingTerrainEffects {
                                camera_disabled: true,
                                pulse_remaining_micros: 0,
                                report_omissions: false,
                            },
                            math,
                            &process,
                        )
                        .unwrap();
                    assert_eq!(owner.scene.stats.models, 1, "only original H8/100 frame");
                    if blinking {
                        assert!(owner.state.menu.glyph_passes > 0);
                        assert_eq!(owner.scene.process_rng(), noise_rng);
                    } else {
                        assert_eq!(owner.state.menu.glyph_passes, 0);
                        noise_rng = owner.scene.process_rng();
                    }
                    assert_eq!(
                        owner.scene.pixels.pipeline.viewport,
                        [0, 0, dimensions[0] as i16 - 1, dimensions[1] as i16 - 1]
                    );
                },
            ));
        }
        let without_label = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, mut frame, math| {
                // Same clock, RNG, original static and H8/100 as the lit
                // blink: omit ONLY 004A3AB0 so frame animation cannot supply
                // the pixels attributed to the label.
                frame.animation.advance(500_000);
                let pane = briefing_terrain_pane(
                    BriefingTerrainInputs {
                        frame,
                        animation: open_panel(),
                        camera_position_words: [0; 3],
                        heading: 0,
                    },
                    math,
                )
                .unwrap()
                .unwrap();
                owner.begin(true).unwrap();
                owner.scene.pixels.pipeline.viewport = pane.clip_rect;
                owner
                    .terrain_static(pane.clip_rect, BriefingNoiseRows::All)
                    .unwrap();
                owner.scene.pixels.pipeline.viewport =
                    [0, 0, dimensions[0] as i16 - 1, dimensions[1] as i16 - 1];
                owner
                    .panel_frame(BriefingPanel::Terrain, open_panel(), frame, math)
                    .unwrap();
                owner
                    .flush_feature("disabled camera without label reference")
                    .unwrap();
                assert_eq!(owner.scene.process_rng(), noise_rng);
            },
        );
        assert!(
            without_label
                .chunks_exact(4)
                .zip(draws[1].chunks_exact(4))
                .any(|(a, b)| a != b),
            "Camera disabled glyphs must survive the original terrain frame at tier{tier}"
        );
    }
}

#[test]
fn native_start_overlay_rasterizes_and_suppresses_milestone_sprites() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let table = assets.sine_table().values;
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
        let process = FrontendProcessState::default();
        process.publish_colors(MenuDrawColors {
            selected_start: resources.style(4).unwrap(),
            ordinary: resources.style(5).unwrap(),
            selected_end: resources.style(6).unwrap(),
        });
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
        let mut state = FrontendRendererState::default();
        let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
        let mut map_arena = crate::object_runtime::ObjectArena::new();
        let map_state = crate::world_map::WorldMapState::new();
        let mut terrain = owned_terrain(&placements);
        let mut effects = BriefingTestEffects::new(&assets);
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(true, terrain.sampling().spacing);
        let mut owner = SoftwareBriefingFrame {
            scene: &mut scene,
            resources,
            sine_quarter: &table,
            template_projection: startup.projection,
            state: &mut state,
            selector_one: 1,
            world: BriefingWorld {
                residency: &resident,
                terrain_source: &crate::world_actor_terrain::WorldTerrainSource::retain(placements.terrain_source()),
                arena: &mut map_arena,
                map_state: &map_state,
                terrain: &mut terrain,
                presentation: &mut crate::world_backend::load_world_presentation(&resident, &placements)
                    .unwrap(),
                world_flags: placements.world().flags,
                network_active: false,
            },
            effects: effects.inputs(),
            dynamic_lighting: true,
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
            infrared: false,
        };
        let frame = BriefingSceneFrame {
            projection: startup.projection,
            age_millis: 6000,
            start_remaining_millis: 1500,
            animation: AnimationClock::default(),
            selector_one: 1,
        };
        owner.tail(frame, Some(3), &process).unwrap();
        assert_eq!(
            owner.scene.stats.direct_sprites, 0,
            "Start must suppress all progress submissions"
        );
        assert!(
            owner.state.menu.glyph_passes > 0,
            "tier {tier} must execute the original H0/166 font passes"
        );
        assert!(
            owner
                .scene
                .pixels
                .frame
                .rgba()
                .chunks_exact(4)
                .any(|pixel| pixel != [17, 23, 31, 255]),
            "tier {tier} must rasterize the native Start overlay"
        );
        let glyph_passes = owner.state.menu.glyph_passes;
        let before = owner.scene.pixels.frame.rgba().to_vec();
        owner
            .tail(
                BriefingSceneFrame {
                    start_remaining_millis: 0,
                    ..frame
                },
                Some(3),
                &process,
            )
            .unwrap();
        assert_eq!(owner.scene.stats.direct_sprites, 3);
        assert_eq!(
            owner.state.menu.glyph_passes, glyph_passes,
            "unarmed tail must not emit Start text"
        );
        assert_ne!(
            owner.scene.pixels.frame.rgba(),
            before,
            "tier {tier} original milestone sprites must add pixels"
        );
        assert!(!owner.scene.stopped());
    }
}

fn level1_fixture(tier: u8) -> (FrontendAssets, PrimitiveTables, ResidentRuntime) {
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
    (assets, tables, resident)
}

/// Leaf 12's authored thumper preview (H8/46, which the Level 1 residency
/// also holds). Its first 26 interpolation reads register 1 before the
/// program writes it. Native reads the stack context's residue there; the
/// port stops at that omittable feature. No Level 1 preview fails any more.
fn leaf12_thumper_preview() -> inf_formats::BriefingModel {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let preload =
        inf_formats::Preload::parse(&std::fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let bytes = std::fs::read(root.join("Overlay").join("12.ovl")).unwrap();
    let layout = inf_formats::parse_overlay(&bytes, &preload.leaf(12).unwrap().base).unwrap();
    let region = layout
        .regions
        .iter()
        .find(|region| region.handler.get() == 14)
        .unwrap();
    let bank = inf_formats::parse_world_bank(&bytes, region).unwrap();
    let program = inf_formats::parse_briefing_program(&bytes, &bank, &bank.entries[0]).unwrap();
    program
        .records
        .iter()
        .find_map(|record| match record.payload {
            BriefingPayload::Model(model) if model.model_slot == 46 => Some(model),
            _ => None,
        })
        .expect("leaf 12's authored thumper preview")
}

/// The thumper preview's omittable unowned-register stop.
fn is_thumper_register_stop(
    error: &crate::scene_resources::SceneResourceError<SoftwareSceneError>,
) -> bool {
    matches!(
        error,
        crate::scene_resources::SceneResourceError::Backend(SoftwareSceneError::Model(
            crate::model_execution::ModelExecutionError::Vertex(
                crate::model_vertices::ModelVertexError::Value(
                    crate::model_values::ModelValueError::Fault(
                        crate::model_values::ModelValueFault::UninitializedRegister { index: 1 }
                    )
                )
            )
        ))
    )
}

#[test]
fn later_authored_briefing_previews_complete_with_native_far_quad_pixels() {
    use crate::{model_primitives::PrimitiveCallback, pixel_backend::PixelBackend};
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
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
            .filter_map(|record| match record.payload {
                BriefingPayload::Model(model) if matches!(model.model_slot, 117 | 250 | 294) => {
                    Some(model)
                }
                _ => None,
            })
            .collect();
        assert_eq!(models.len(), 3);
        for model in models {
            let screen_age = 7500;
            // CE40 resets +240 on kind2/4 even at a later absolute screen
            // age. B840's model-age fade selects Far during this opening.
            // H8/117's500-ms pose exercises both Far DE and ordinary quads;
            // the three2000-ms poses retain complete Near-path coverage.
            for model_age in [500, 2000] {
                if model_age == 500 && model.model_slot != 117 {
                    continue;
                }
                let exercises_far = model_age == 500;
                let mut draws = Vec::new();
                for omitted_consumer in [
                    None,
                    Some(PrimitiveCallback::SharedShadeTriangleFar),
                    Some(PrimitiveCallback::TexturedQuadFar),
                ] {
                    if omitted_consumer.is_some() && !exercises_far {
                        continue;
                    }
                    let label = format!(
                        "tier{tier} H8/{} screen_age{screen_age} model_age{model_age} omitted_consumer={omitted_consumer:?}",
                        model.model_slot
                    );
                    let pixels = level1_layer_pixels(
                        &assets,
                        &tables,
                        &resident,
                        dimensions,
                        |owner, frame, math| {
                            // Explicit synthetic late screen/model clocks. Shift16
                            // is the BB40 geometry branch's retained global publication,
                            // not a camera-derived substitution or universal frame rule.
                            owner.scene.environment.coordinate_shift = 16;
                            owner.scene.graphics.publish_shift(16).unwrap(); // explicit supplied fixture publication
                            let frame = BriefingSceneFrame {
                                age_millis: screen_age,
                                ..frame
                            };
                            let extent = model_extent(owner.resources, model.model_slot).unwrap();
                            draw_briefing_model_preview(
                                model,
                                model_age,
                                extent,
                                false,
                                frame,
                                math,
                                &mut ResidentSceneHost {
                                    resources: owner.resources,
                                    backend: &mut *owner.scene,
                                },
                            )
                            .unwrap_or_else(|error| panic!("{label}: {error:?}"));
                            // The actual draw uses the strict backend flush. The
                            // negative pixel controls omit one explicit Far consumer;
                            // every other command retains its queue order and policy.
                            let result = if let Some(omitted) = omitted_consumer {
                                let (queue, pixels) =
                                    (&mut owner.scene.queue, &mut owner.scene.pixels);
                                queue
                                    .flush(|invocation, command| {
                                        if command.callback == omitted {
                                            Ok(())
                                        } else {
                                            pixels.draw(invocation, command)
                                        }
                                    })
                                    .map(|_| ())
                                    .map_err(SoftwareSceneError::Flush)
                            } else {
                                owner.scene.submit(ResolvedSceneCommand::FlushBatch)
                            };
                            result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
                            assert_eq!(owner.scene.pixels.stats.unsupported, 0, "{label}");
                            if exercises_far {
                                if omitted_consumer
                                    != Some(PrimitiveCallback::SharedShadeTriangleFar)
                                {
                                    assert!(
                                        owner.scene.pixels.stats.shared_shade_far_triangles > 0,
                                        "{label}: original DE Far must execute"
                                    );
                                }
                                if omitted_consumer != Some(PrimitiveCallback::TexturedQuadFar) {
                                    assert!(
                                        owner.scene.pixels.stats.ordinary_far_quads > 0,
                                        "{label}: original ordinary Far quad must execute"
                                    );
                                }
                            }
                        },
                    );
                    assert!(
                        pixels
                            .chunks_exact(4)
                            .any(|pixel| pixel != PIXEL_GATE_CLEAR),
                        "{label}: original preview must contribute pixels"
                    );
                    draws.push(pixels);
                }
                if exercises_far {
                    for (control, consumer) in draws[1..]
                        .iter()
                        .zip(["Far DE triangle", "ordinary Far quad"])
                    {
                        let attributable = draws[0]
                            .chunks_exact(4)
                            .zip(control.chunks_exact(4))
                            .filter(|(actual, without_consumer)| actual != without_consumer)
                            .count();
                        assert!(attributable > 0,
                            "tier{tier} H8/117 screen_age7500 model_age500: original {consumer} must contribute its own pixels");
                    }
                }
            }
        }
    }
}

fn open_panel() -> BriefingPanelAnimation {
    BriefingPanelAnimation {
        current_stage: 2,
        requested_stage: 2,
        remaining_micros: 0,
        duration_micros: 500_000,
        current_scale_word: 0,
        requested_scale_word: 0,
    }
}

const PIXEL_GATE_CLEAR: [u8; 4] = [17, 23, 31, 255];

/// Each comparison starts with a fresh renderer and the same original world.
/// Clock2000, inherited shift8 and RNG0 are synthetic test preconditions; the
/// display focal words, model programs, terrain, tint and light are resident.
fn level1_layer_pixels<'r>(
    assets: &'r FrontendAssets,
    tables: &'r PrimitiveTables,
    resident: &'r ResidentRuntime,
    dimensions: [u32; 2],
    draw: impl FnOnce(&mut SoftwareBriefingFrame<'_, '_, 'r>, BriefingSceneFrame, &MenuBasisMath),
) -> Vec<u8> {
    let resources = RenderResources::new(resident);
    let placements = WorldPlacements::new(resident, TerrainSpacing::Units256).unwrap();
    let startup = frontend_startup(
        resident,
        crate::presentation::NativeFrame::original(dimensions),
    )
    .unwrap();
    let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], PIXEL_GATE_CLEAR);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    scene.set_terrain(Some(placements.terrain().clone()));
    scene.set_process_rng(Some(0));
    scene.environment.coordinate_shift = 8;
    scene.graphics.publish_shift(8).unwrap(); // explicit supplied fixture publication
    let mut state = FrontendRendererState::default();
    let mut arena = crate::object_runtime::ObjectArena::new();
    let map_state = crate::world_map::WorldMapState::new();
    let mut terrain = owned_terrain(&placements);
    let mut effects = BriefingTestEffects::new(&assets);
    let mut object_light_policy = FixtureObjectLightPolicy::new(true, terrain.sampling().spacing);
    let mut owner = SoftwareBriefingFrame {
        scene: &mut scene,
        resources,
        sine_quarter: &assets.sine_table().values,
        template_projection: startup.projection,
        state: &mut state,
        selector_one: 1,
        world: BriefingWorld {
            residency: &resident,
            terrain_source: &crate::world_actor_terrain::WorldTerrainSource::retain(placements.terrain_source()),
            arena: &mut arena,
            map_state: &map_state,
            terrain: &mut terrain,
            presentation: &mut crate::world_backend::load_world_presentation(resident, &placements)
                .unwrap(),
            world_flags: placements.world().flags,
            network_active: false,
        },
        effects: effects.inputs(),
        dynamic_lighting: true,
        object_light_policy: &mut object_light_policy,
        object_light_failure_policy:
            crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        infrared: false,
    };
    owner.begin(true).unwrap();
    draw(
        &mut owner,
        BriefingSceneFrame {
            projection: startup.projection,
            age_millis: 2000,
            start_remaining_millis: 0,
            animation: AnimationClock::default(),
            selector_one: 1,
        },
        &MenuBasisMath::new(assets.sine_table().values),
    );
    assert!(!owner.scene.stopped());
    owner.scene.pixels.frame.rgba().to_vec()
}

/// Require pixels attributable to this layer, rather than a model counter or
/// any changed pixel that could have come from its background or another frame.
fn assert_layer_pixels_survive(
    label: &str,
    without_layer: &[u8],
    with_layer: &[u8],
    complete: &[u8],
) {
    assert_eq!(without_layer.len(), with_layer.len());
    assert_eq!(with_layer.len(), complete.len());
    let surviving = without_layer
        .chunks_exact(4)
        .zip(with_layer.chunks_exact(4))
        .zip(complete.chunks_exact(4))
        .filter(|((before, expected), actual)| {
            before != expected && *expected != PIXEL_GATE_CLEAR && expected == actual
        })
        .count();
    assert!(
        surviving > 0,
        "{label} must contribute its own surviving pixels"
    );
}

#[test]
fn complete_terrain_pane_keeps_backdrop_pixels_and_draws_its_frame() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        let inputs = |frame| BriefingTerrainInputs {
            frame,
            animation: open_panel(),
            // Synthetic screen-owned camera, not a sampled live-world pose.
            camera_position_words: [0x1000, 400, 0x2000],
            heading: 0x1234,
        };
        let fill_only = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                let pane = briefing_terrain_pane(inputs(frame), math).unwrap().unwrap();
                owner.scene.pixels.pipeline.viewport = pane.clip_rect;
                let color = owner.material_word(0x72).unwrap();
                owner
                    .scene
                    .draw_flat_quad_direct(pane.far_fill(color))
                    .unwrap();
            },
        );
        let contents = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                let pane = briefing_terrain_pane(inputs(frame), math).unwrap().unwrap();
                owner.terrain_backdrop(pane, frame).unwrap();
                // Standalone pixel references own their explicit content flush.
                owner
                    .scene
                    .submit(ResolvedSceneCommand::FlushBatch)
                    .unwrap();
            },
        );
        let fill_and_frame = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                let pane = briefing_terrain_pane(inputs(frame), math).unwrap().unwrap();
                owner.scene.pixels.pipeline.viewport = pane.clip_rect;
                // BB40's same pane publications and direct fill, with only
                // optional H8/278 omitted. Preserve the actual pre-frame
                // light/ambient and shift so a lighting difference cannot
                // falsely count as surviving backdrop pixels.
                let rgb = owner.world.presentation.tint_word.to_le_bytes();
                owner
                    .scene
                    .submit(ResolvedSceneCommand::DirectionalLight {
                        direction: owner.world.presentation.terrain_light_direction,
                        rgb: [rgb[0], rgb[1], rgb[2]],
                    })
                    .unwrap();
                owner
                    .scene
                    .submit(ResolvedSceneCommand::AmbientLight(
                        [rgb[0], rgb[1], rgb[2]].map(|value| i32::from(value >> 3)),
                    ))
                    .unwrap();
                owner
                    .scene
                    .submit(ResolvedSceneCommand::Setup(pane.context))
                    .unwrap();
                let far_color = owner.material_word(0x74).unwrap();
                owner
                    .scene
                    .submit(ResolvedSceneCommand::FarColor(far_color))
                    .unwrap();
                owner
                    .scene
                    .submit(ResolvedSceneCommand::PublishView(pane.view))
                    .unwrap();
                owner
                    .scene
                    .submit(ResolvedSceneCommand::CoordinateShift(16))
                    .unwrap();
                let color = owner.material_word(0x72).unwrap();
                owner
                    .scene
                    .draw_flat_quad_direct(pane.far_fill(color))
                    .unwrap();
                owner.flush_feature("fill-only pane reference").unwrap();
                owner.scene.pixels.pipeline.viewport =
                    [0, 0, dimensions[0] as i16 - 1, dimensions[1] as i16 - 1];
                owner
                    .panel_frame(BriefingPanel::Terrain, open_panel(), frame, math)
                    .unwrap();
                owner
                    .flush_feature("fill-only terrain frame reference")
                    .unwrap();
            },
        );
        let frame_reference = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                let pane = briefing_terrain_pane(inputs(frame), math).unwrap().unwrap();
                owner.terrain_backdrop(pane, frame).unwrap();
                // Standalone pixel references own their explicit content flush.
                owner
                    .scene
                    .submit(ResolvedSceneCommand::FlushBatch)
                    .unwrap();
                // Native BB40's content flush is followed by4A4400. Draw
                // the original H8/100 over its real contents: this frame
                // blends, so a clear-background frame is not a color oracle.
                owner.begin(true).unwrap();
                owner.scene.pixels.pipeline.viewport =
                    [0, 0, dimensions[0] as i16 - 1, dimensions[1] as i16 - 1];
                owner
                    .panel_frame(BriefingPanel::Terrain, open_panel(), frame, math)
                    .unwrap();
                owner.flush_feature("terrain frame reference").unwrap();
            },
        );
        let complete = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                owner
                    .terrain_panel(
                        inputs(frame),
                        BriefingTerrainEffects {
                            camera_disabled: false,
                            pulse_remaining_micros: 0,
                            report_omissions: false,
                        },
                        math,
                        &FrontendProcessState::default(),
                    )
                    .unwrap();
                assert_eq!(
                    owner.scene.pixels.pipeline.viewport,
                    [0, 0, dimensions[0] as i16 - 1, dimensions[1] as i16 - 1],
                    "BB40 must restore the full viewport before its frame"
                );
            },
        );
        assert_layer_pixels_survive(
            &format!("tier{tier} original H8/278 must draw beyond the H7/72 fill"),
            &fill_only,
            &contents,
            &contents,
        );
        assert_layer_pixels_survive(
            &format!("tier{tier} original H8/278 must affect the fully composited pane"),
            &fill_and_frame,
            &complete,
            &complete,
        );
        assert_layer_pixels_survive(
            &format!("tier{tier} enclosing H8/100 terrain frame"),
            &contents,
            &frame_reference,
            &complete,
        );
    }
}

#[test]
fn complete_map_then_authored_preview_preserves_both_frame_layers() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
        let program = inf_formats::parse_briefing_program(
            placements.world_source().overlay.bytes(),
            &placements.assets().worlds,
            placements.world(),
        )
        .unwrap();
        let model = program
            .records
            .iter()
            .find_map(|record| match record.payload {
                BriefingPayload::Model(model) if model.model_slot == 117 => Some(model),
                _ => None,
            })
            .expect("Level1's original successful H8/117 preview payload");
        let map_inputs = |frame| BriefingMapInputs {
            frame,
            animation: open_panel(),
            // Explicit synthetic map framing, shared by every comparison.
            map_scale_word: 0,
            camera_xz_words: [0, 0],
        };
        let map_tiles = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, _| {
                owner
                    .map_backdrop(map_inputs(frame), &FrontendProcessState::default())
                    .unwrap()
            },
        );
        let map_prefix =
            |owner: &mut SoftwareBriefingFrame<'_, '_, '_>, frame, math: &MenuBasisMath| {
                owner
                    .map_backdrop(map_inputs(frame), &FrontendProcessState::default())
                    .unwrap();
                owner
                    .panel_frame(BriefingPanel::Map, open_panel(), frame, math)
                    .unwrap();
                owner.flush_feature("map frame reference").unwrap();
            };
        let preview =
            |owner: &mut SoftwareBriefingFrame<'_, '_, '_>, frame, math: &MenuBasisMath| {
                draw_briefing_model_preview(
                    model,
                    2000,
                    model_extent(owner.resources, model.model_slot).unwrap(),
                    true,
                    frame,
                    math,
                    &mut ResidentSceneHost {
                        resources: owner.resources,
                        backend: &mut *owner.scene,
                    },
                )
                .expect("actual H8/117 preview must execute after the map frame flush");
                owner.flush_feature("preview reference").unwrap();
            };
        let map_frame = level1_layer_pixels(&assets, &tables, &resident, dimensions, map_prefix);
        let map_and_preview = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                map_prefix(owner, frame, math);
                preview(owner, frame, math);
            },
        );
        let frame_reference = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                map_prefix(owner, frame, math);
                preview(owner, frame, math);
                owner.scene.pixels.frame.clear_color(PIXEL_GATE_CLEAR);
                owner
                    .panel_frame(BriefingPanel::Model, open_panel(), frame, math)
                    .unwrap();
                owner.flush_feature("model frame reference").unwrap();
            },
        );
        let complete = level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                owner
                    .scene_panels(
                        &[
                            (BriefingPanel::Map, open_panel()),
                            (BriefingPanel::Model, open_panel()),
                        ],
                        Some(model),
                        2000,
                        map_inputs(frame),
                        math,
                        &FrontendProcessState::default(),
                    )
                    .unwrap();
            },
        );
        assert_layer_pixels_survive(
            &format!("tier{tier} original H8/100 map frame"),
            &map_tiles,
            &map_frame,
            &complete,
        );
        assert_layer_pixels_survive(
            &format!("tier{tier} actual H8/117 preview after the map frame"),
            &map_frame,
            &map_and_preview,
            &complete,
        );
        assert_layer_pixels_survive(
            &format!("tier{tier} original H8/99 model frame after the successful preview"),
            &map_and_preview,
            &frame_reference,
            &complete,
        );
    }
}

/// The thumper preview stops without the map frame (its register 1 unknown)
/// and is omitted; after the map frame it starts with that frame's bases,
/// draws, and the frame's completed pixels survive.
#[test]
fn thumper_preview_after_the_map_frame_preserves_its_pixels() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        let resources = RenderResources::new(&resident);
        let startup = frontend_startup(
            &resident,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
        let model = leaf12_thumper_preview();
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
        scene.set_process_rng(Some(0));
        scene.begin_frame().unwrap();
        let mut state = FrontendRendererState::default();
        let mut map_arena = crate::object_runtime::ObjectArena::new();
        let map_state = crate::world_map::WorldMapState::new();
        let mut terrain = owned_terrain(&placements);
        let mut effects = BriefingTestEffects::new(&assets);
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(true, terrain.sampling().spacing);
        let mut owner = SoftwareBriefingFrame {
            scene: &mut scene,
            resources,
            sine_quarter: &assets.sine_table().values,
            template_projection: startup.projection,
            state: &mut state,
            selector_one: 1,
            world: BriefingWorld {
                residency: &resident,
                terrain_source: &crate::world_actor_terrain::WorldTerrainSource::retain(placements.terrain_source()),
                arena: &mut map_arena,
                map_state: &map_state,
                terrain: &mut terrain,
                presentation: &mut crate::world_backend::load_world_presentation(&resident, &placements)
                    .unwrap(),
                world_flags: placements.world().flags,
                network_active: false,
            },
            effects: effects.inputs(),
            dynamic_lighting: true,
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
            infrared: false,
        };
        let frame = BriefingSceneFrame {
            projection: startup.projection,
            age_millis: 2000,
            start_remaining_millis: 0,
            animation: AnimationClock::default(),
            selector_one: 1,
        };
        let math = MenuBasisMath::new(assets.sine_table().values);
        let preview_entry = owner.feature_entry().unwrap();
        let result = draw_briefing_model_preview(
            model,
            2000,
            model_extent(resources, model.model_slot).unwrap(),
            false,
            frame,
            &math,
            &mut ResidentSceneHost {
                resources,
                backend: &mut *owner.scene,
            },
        );
        assert!(
            result.as_ref().is_err_and(is_thumper_register_stop),
            "actual thumper boundary: {result:?}"
        );
        owner
            .omit_feature("fixture preview prefix", preview_entry, result.unwrap_err())
            .unwrap();
        owner.scene.pixels.frame.clear_color([17, 23, 31, 255]);
        owner.begin(true).unwrap();
        // Explicit synthetic camera/zoom preconditions for the sibling-pixel gate.
        let map_inputs = BriefingMapInputs {
            frame,
            animation: open_panel(),
            map_scale_word: 0,
            camera_xz_words: [0, 0],
        };
        owner
            .map_backdrop(map_inputs, &FrontendProcessState::default())
            .unwrap();
        owner
            .panel_frame(BriefingPanel::Map, open_panel(), frame, &math)
            .unwrap();
        owner.flush_feature("baseline map frame").unwrap();
        let baseline = owner.scene.pixels.frame.rgba().to_vec();
        owner.scene.pixels.frame.clear_color([17, 23, 31, 255]);
        let closed = BriefingPanelAnimation {
            current_stage: 0,
            requested_stage: 0,
            ..open_panel()
        };
        owner
            .scene_panels(
                &[
                    (BriefingPanel::Map, open_panel()),
                    (BriefingPanel::Model, open_panel()),
                    (BriefingPanel::Terrain, closed),
                ],
                Some(model),
                2000,
                map_inputs,
                &math,
                &FrontendProcessState::default(),
            )
            .unwrap();
        let preserved = baseline
            .chunks_exact(4)
            .zip(owner.scene.pixels.frame.rgba().chunks_exact(4))
            .enumerate()
            .filter(|(index, (before, after))| {
                index % dimensions[0] as usize >= dimensions[0] as usize * 3 / 5
                    && *before != [17, 23, 31, 255]
                    && before == after
            })
            .count();
        assert!(
            preserved > 0,
            "the preview must retain the Map frame's completed pixels"
        );
        assert!(!owner.scene.stopped());
    }
}

#[test]
fn briefing_draw_commits_real_rng_prefix_on_later_failure_and_clears_copy() {
    let (assets, tables, resident) = level1_fixture(2);
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let gate = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 63,
        })
        .unwrap();
    let RenderResourceData::Model(gate) = gate.data else {
        unreachable!()
    };
    let jitter = gate
        .program
        .instructions
        .iter()
        .find(|instruction| instruction.span.start == 26692)
        .expect("original gatebod's0222 instruction");
    assert_eq!(jitter.raw_command, 0x0222);
    let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
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
    let mut state = FrontendRendererState::default();
    let mut map_arena = crate::object_runtime::ObjectArena::new();
    let map_state = crate::world_map::WorldMapState::new();
    let mut terrain = owned_terrain(&placements);
    let mut effects = BriefingTestEffects::new(&assets);
    let mut object_light_policy = FixtureObjectLightPolicy::new(true, terrain.sampling().spacing);
    let mut owner = SoftwareBriefingFrame {
        scene: &mut scene,
        resources,
        sine_quarter: &assets.sine_table().values,
        template_projection: startup.projection,
        state: &mut state,
        selector_one: 1,
        world: BriefingWorld {
            residency: &resident,
            terrain_source: &crate::world_actor_terrain::WorldTerrainSource::retain(placements.terrain_source()),
            arena: &mut map_arena,
            map_state: &map_state,
            terrain: &mut terrain,
            presentation: &mut crate::world_backend::load_world_presentation(&resident, &placements)
                .unwrap(),
            world_flags: placements.world().flags,
            network_active: false,
        },
        effects: effects.inputs(),
        dynamic_lighting: true,
        object_light_policy: &mut object_light_policy,
        object_light_failure_policy:
            crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        infrared: false,
    };
    // Synthetic starting state and context, original model63 initializer and
    // original0222[120,44,25] instruction. Full Level1 gate execution still
    // stops at the preceding unowned custom selector8, so this fixture isolates
    // the authored jitter segment instead of claiming that whole path reaches it.
    let initial = 0x1234_5678;
    let mut process_rng = initial;
    let mut consumed_prefix = initial;
    let result: Result<(), BriefingRendererError> =
        owner.with_process_rng(&mut process_rng, |owner| {
            let environment = owner.scene.environment;
            let context = ModelContext::new([0, 0, 1000], MenuBasis::INITIAL);
            let mut projection = ModelProjectionBackend {
                transform: context.publish(environment.view, environment.coordinate_shift),
                view: environment.view,
                projection: environment.projection,
                dispatch: ModelDispatch::Near,
                coordinate_shift: environment.coordinate_shift,
                fog_factor: environment.fog_factor,
            };
            let mut vertices = ModelVertices::new_invocation(gate.header.vertex_slot_count);
            vertices
                .initialize(gate, &context, &mut projection)
                .unwrap();
            let mut rng = owner.scene.process_rng().unwrap();
            vertices
                .execute_jitter_22(jitter, Some(&mut rng), &mut projection)
                .unwrap();
            assert!(vertices.slot(120).unwrap().projection.read_u32(4).is_some());
            owner.scene.set_process_rng(Some(rng));
            consumed_prefix = rng;
            assert_ne!(
                consumed_prefix, initial,
                "original gatebod jitter instruction must consume its RNG prefix"
            );
            // A deliberately injected later owner error probes custody; it is
            // not represented as an observed native control-flow failure.
            Err(BriefingRendererError::MissingControls)
        });
    assert!(matches!(
        result,
        Err(BriefingRendererError::MissingControls)
    ));
    assert_eq!(process_rng, consumed_prefix);
    assert_eq!(owner.scene.process_rng(), None);
    assert!(!owner.scene.stopped());
}
#[test]
fn live_furniture_contributes_pixels_to_the_shared_briefing_ground_at_both_tiers() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        let mut images = Vec::new();
        for enabled in [false, true] {
            images.push(level1_layer_pixels(
                &assets,
                &tables,
                &resident,
                dimensions,
                |owner, frame, _| {
                    let placements =
                        crate::world_furniture_effect::resident::resident_placement_table(
                            owner.resources.resident_runtime(),
                        )
                        .unwrap();
                    let (bucket, record) = owner
                        .world
                        .terrain
                        .buckets()
                        .iter()
                        .enumerate()
                        .flat_map(|(bucket, row)| {
                            row.records.iter().map(move |record| (bucket, record))
                        })
                        .find(|(_, record)| {
                            if record.resource_lookup.raw() == 0 || record.x & 1 != 0 {
                                return false;
                            }
                            let Some(slot) = placements
                                .slots
                                .get(usize::from(record.resource_lookup.raw()))
                            else {
                                return false;
                            };
                            let Ok(resource) = owner.resources.resolve(RenderResourceRequest {
                                kind: RenderResourceKind::Model,
                                argument: i32::from(slot.resource_word),
                            }) else {
                                return false;
                            };
                            let RenderResourceData::Model(model) = resource.data else {
                                return false;
                            };
                            model.program.instructions.iter().any(|instruction| {
                                matches!(
                                    instruction.kind,
                                    inf_formats::ModelInstructionKind::Triangle(_)
                                        | inf_formats::ModelInstructionKind::Quad(_)
                                )
                            })
                        })
                        .map(|(bucket, record)| (bucket, *record))
                        .expect("authored furniture with faces");
                    let xz = [record.x.wrapping_shl(7), (bucket as u16).wrapping_shl(7)];
                    let focus = [
                        i32::from(xz[0] as i16),
                        i32::from(owner.world.terrain.raw_height_at(xz)),
                        i32::from(xz[1] as i16),
                    ];
                    let eye = [focus[0], focus[1] + 512, focus[2] - 2048];
                    let view = crate::frontend_scene::SceneView {
                        eye,
                        basis: crate::frontend_scene::camera_basis(eye, focus),
                    };
                    if !enabled {
                        let records: Vec<_> = owner
                            .world
                            .terrain
                            .buckets()
                            .iter()
                            .enumerate()
                            .flat_map(|(bucket, row)| {
                                row.records.iter().enumerate().map(move |(index, record)| {
                                    (
                                        crate::terrain_runtime::TerrainSparseRecordId {
                                            bucket: bucket as u16,
                                            index,
                                        },
                                        record.state,
                                    )
                                })
                            })
                            .collect();
                        for (record, state) in records {
                            owner
                                .world
                                .terrain
                                .write_sparse_payload(
                                    record,
                                    crate::terrain_runtime::TerrainSparsePayload {
                                        state,
                                        resource_lookup:
                                            inf_formats::TerrainResourceLookup::from_raw(0),
                                    },
                                )
                                .unwrap();
                        }
                    }
                    // Supplied view/fade isolate furniture pixels in actual H12/H11/H8
                    // data. They do not assert the retail pane's camera timeline.
                    let setup = crate::frontend_scene::SceneSetup {
                        focal: frame.projection.focal,
                        viewport: frame.projection.viewport,
                        center: frame.projection.viewport.map(|v| v / 2),
                        fade: [8192, 9216],
                        fade_factor: 65536,
                    };
                    owner
                        .scene
                        .submit(ResolvedSceneCommand::Setup(setup))
                        .unwrap();
                    owner
                        .scene
                        .submit(ResolvedSceneCommand::PublishView(view))
                        .unwrap();
                    owner
                        .scene
                        .submit(ResolvedSceneCommand::CoordinateShift(16))
                        .unwrap();
                    owner
                        .terrain_ground(
                            BriefingTerrainPane {
                                clip_rect: [
                                    0,
                                    0,
                                    dimensions[0] as i16 - 1,
                                    dimensions[1] as i16 - 1,
                                ],
                                context: setup,
                                view,
                            },
                            frame,
                            false,
                            &FrontendProcessState::default(),
                            false,
                        )
                        .unwrap();
                    owner
                        .scene
                        .submit(ResolvedSceneCommand::FlushBatch)
                        .unwrap();
                    if enabled {
                        assert!(
                            owner.scene.stats.models > 0,
                            "tier{tier} actual furniture H1"
                        );
                    } else {
                        assert_eq!(owner.scene.stats.models, 0);
                    }
                    assert!(owner.scene.stats.terrain_commands > 0);
                    assert!(!owner.scene.stopped());
                },
            ));
        }
        assert!(
            images[0]
                .chunks_exact(4)
                .zip(images[1].chunks_exact(4))
                .any(|(a, b)| a != b),
            "tier{tier}: actual furniture must add pixels to the same briefing terrain consumer"
        );
    }
}
