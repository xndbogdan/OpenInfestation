//! Published model projection versus the distinct actor caller context.
//!
//! Original H8/161 initializes projection rows before its first wordcode. A
//! zero-instruction portable limit observes that real prefix without replacing
//! primitives or claiming complete model, briefing-pane, or retail execution.

use super::*;
use crate::{
    briefing_runtime::BriefingPanelAnimation,
    briefing_scene::BriefingSceneFrame,
    briefing_terrain::{briefing_terrain_pane, BriefingTerrainInputs},
    frontend_scene::{FrontendSceneHost, SceneCommand, SceneFade, SceneProjection},
    menu_basis::MenuBasisMath,
    model_context::{fistp_low32, ModelContext},
    model_execution::{ModelExecutionError, ModelRootMode},
    model_projection::ModelProjectionBackend,
    model_scratch::{ModelScratchDrawState, ProcessModelScratch},
    model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector},
    scene_resources::ResidentSceneHost,
};

fn original161_published_projection_prefix(divergent_caller_view: bool) {
    let (assets, tables, resident) = fixture(); // actual Boot->48, parent1 H8
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let zero = executable.loader_zero_data(0x0056_4728, 0xfa004).unwrap();
    let scratch = ProcessModelScratch::native_loader_zero(&zero).unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut config = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap()
    .scene;
    // The native initializer precedes this explicitly supplied portable stop.
    config.execution_limits.instructions = 0;
    let clock = AnimationClock::default();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let pane = briefing_terrain_pane(
        BriefingTerrainInputs {
            frame: BriefingSceneFrame {
                projection: SceneProjection {
                    focal: config.environment.projection.focal,
                    viewport: config.environment.projection.viewport,
                    fade: config.environment.projection.fade,
                },
                age_millis: 0,
                start_remaining_millis: 0,
                animation: clock,
                selector_one: 0,
            },
            animation: BriefingPanelAnimation {
                current_stage: 2,
                requested_stage: 2,
                remaining_micros: 0,
                duration_micros: 500_000,
                current_scale_word: 0x7800,
                requested_scale_word: 0x7800,
            },
            // Declared caller pose; these words are not captured birth defaults.
            camera_position_words: [16_000, 2_000, 16_000],
            heading: 0,
        },
        &math,
    )
    .unwrap()
    .unwrap();
    assert_eq!(pane.context.fade, [6144, 6656]);

    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            scratch.clone(),
            (config).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut pixels,
        config,
    })
    .unwrap();
    scene.begin_frame().unwrap();
    {
        let mut host = ResidentSceneHost {
            resources: RenderResources::new(&resident),
            backend: &mut scene,
        };
        host.submit(SceneCommand::CoordinateShift(16)).unwrap();
        host.submit(SceneCommand::Setup(pane.context)).unwrap();
        host.submit(SceneCommand::PublishView(pane.view)).unwrap();
        // Actual 0048FB00 publication shape at the owned terrain Far phase.
        // The complete terrain producer is covered by the separate composed
        // test; this first regression supplies that source-derived interval.
        host.submit(SceneCommand::PublishFade(SceneFade::new([8320, 9984])))
            .unwrap();
    }
    let published = scene.environment;
    assert_eq!(published.projection.fade, [8320, 9984]);
    assert_eq!(published.fog_factor, SceneFade::new([8320, 9984]).factor);
    let mut caller = ObjectDrawContext {
        view: pane.view,
        near: pane.context.fade[0],
        far: pane.context.fade[1],
        far_color: 0,
    };
    if divergent_caller_view {
        // A supplied disagreement isolates global-view custody from fade.
        // No native caller writer or complete pane timeline is inferred.
        caller.near = published.projection.fade[0];
        caller.far = published.projection.fade[1];
        for axis in 0..3 {
            let forward = f64::from(f32::from_bits(published.view.basis.words[7 + axis]));
            caller.view.eye[axis] =
                caller.view.eye[axis].wrapping_add(fistp_low32(forward * 20_000.0));
        }
        assert_ne!(caller.view, published.view);
    } else {
        assert_ne!([caller.near, caller.far], published.projection.fade);
    }

    let RenderResourceData::Model(model) = scene
        .resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 161,
        })
        .unwrap()
        .data
    else {
        panic!("real parent1 H8/161");
    };
    assert_eq!(model.identity.global_index, 161);
    assert_eq!(model.header.flags.raw(), 0x0c);
    assert!(!model.header.flags.forces_near_table());
    assert_eq!(model.header.reserved_10, 0);
    assert_eq!(model.header.vertex_ranges, [2, 2, 4, 18]);
    let index = model.header.reserved_10;
    let row = model.vertices[usize::from(index / 2)];
    let raw = [i32::from(row.x), i32::from(row.y), i32::from(row.z)];
    let depth = 9152.0;
    let position = std::array::from_fn(|axis| {
        let forward = f64::from(f32::from_bits(published.view.basis.words[7 + axis]));
        published.view.eye[axis]
            .wrapping_add(fistp_low32(forward * depth))
            .wrapping_sub(raw[axis])
    });
    let model_context = ModelContext::new(position, MenuBasis::INITIAL);
    let transform = model_context.publish(published.view, published.coordinate_shift);
    assert!(transform.in_frustum(model.header.visibility_extent, &published.projection));
    assert_eq!(
        transform.dispatch(model.header, &published.projection),
        Some(ModelDispatch::Far)
    );
    let caller_transform = model_context.publish(caller.view, published.coordinate_shift);
    let mut caller_projection = published.projection;
    caller_projection.fade = [caller.near, caller.far];
    assert!(
        !caller_transform.in_frustum(model.header.visibility_extent, &caller_projection)
            || caller_transform
                .dispatch(model.header, &caller_projection)
                .is_none(),
        "the former caller overwrite bypasses the native projection prefix"
    );
    let offset = usize::from(index) * 24 + 12;
    let before = scratch.read_pool::<12>(offset).unwrap();
    let mut expected = ModelVertexProjection { bytes: before };
    let mut projector = ModelProjectionBackend {
        transform,
        view: published.view,
        projection: published.projection,
        dispatch: ModelDispatch::Far,
        coordinate_shift: published.coordinate_shift,
        fog_factor: published.fog_factor,
    };
    let actual_depth = projector
        .project(ModelVertexInput::Local(raw), &mut expected)
        .unwrap();
    assert!(actual_depth >= 64);
    assert!(expected.bytes[9].is_some());
    assert_ne!(expected.bytes, before);

    let mut arena = arena();
    let source = arena.allocate_actor(allocation()).unwrap();
    let arena_before = format!("{arena:?}");
    let queue_before = scene.queue.state();
    let rng_before = scene.process_rng;
    let result = scene.draw_object_model(SoftwareObjectModelDrawRequest {
        terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
        packet: ObjectModelPacket {
            source,
            model_slot: 161,
            position,
            basis: MenuBasis::INITIAL,
        },
        root_mode: ModelRootMode::CommonVisual,
        context: &caller,
        arena: &mut arena,
        animation: &clock,
        executable,
    });
    assert!(
        matches!(
            result,
            Err(SoftwareSceneError::Model(ModelExecutionError::Limit))
        ),
        "{result:?}"
    );
    assert_eq!(scratch.read_pool::<12>(offset).unwrap(), expected.bytes);
    assert_eq!(
        &scratch.read_pool::<12>(offset).unwrap()[10..],
        &before[10..]
    );
    assert_eq!(scene.environment.view, published.view);
    assert_eq!(scene.environment.projection, published.projection);
    assert_eq!(scene.environment.fog_factor, published.fog_factor);
    assert_eq!(scene.queue.state(), queue_before);
    assert_eq!(scene.process_rng, rng_before);
    assert_eq!(format!("{arena:?}"), arena_before);
    assert!(matches!(
        scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
    assert_eq!(
        scratch.cursor().unwrap(),
        usize::from(model.header.vertex_slot_count) * 24
            + usize::from(model.header.shade_slot_count) * 4
    );
}

#[test]
fn published_far_interval_survives_common_actor_handoff_and_projects_retained_bytes() {
    original161_published_projection_prefix(false);
}

#[test]
fn published_global_view_survives_a_divergent_actor_caller_eye() {
    original161_published_projection_prefix(true);
}

#[test]
fn actual_level1_terrain_far_publication_survives_the_next_common_object_prefix() {
    use crate::{
        model_context::ModelProjection,
        process_particles::{
            births::ParticleBirthInputs, factories::ParticleFactoryProcessState,
            ProcessParticleRuntime,
        },
        software_scene::{SoftwareFurnitureInputs, SoftwareTerrainDrawRequest},
        terrain_draw::{terrain_draw_plan, TerrainDrawInputs, TerrainDrawRequest, TerrainGrid},
        terrain_furniture::draw::FurnitureDrawState,
        world_furniture_effect::FurnitureEffectQueue,
        world_lighting::{terrain_lighting_inputs, WorldLightGrid},
        world_placement::WorldPlacements,
    };
    let (assets, tables, mut resident) = fixture();
    resident
        .apply(ResidencyRequest::Activate { level: 28 })
        .unwrap();
    let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
    let mut terrain = placements.terrain().clone(); // actual original28 H12
    let world_flags = placements.world().flags;
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let zero = executable.loader_zero_data(0x0056_4728, 0xfa004).unwrap();
    let scratch = ProcessModelScratch::native_loader_zero(&zero).unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let config = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap()
    .scene;
    let clock = AnimationClock::default();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let pane = briefing_terrain_pane(
        BriefingTerrainInputs {
            frame: BriefingSceneFrame {
                projection: SceneProjection {
                    focal: config.environment.projection.focal,
                    viewport: config.environment.projection.viewport,
                    fade: config.environment.projection.fade,
                },
                age_millis: 0,
                start_remaining_millis: 0,
                animation: clock,
                selector_one: 0,
            },
            animation: BriefingPanelAnimation {
                current_stage: 2,
                requested_stage: 2,
                remaining_micros: 0,
                duration_micros: 500_000,
                current_scale_word: 0x7800,
                requested_scale_word: 0x7800,
            },
            camera_position_words: [16_000, 2_000, 16_000],
            heading: 0,
        },
        &math,
    )
    .unwrap()
    .unwrap();
    let caller = ObjectDrawContext {
        view: pane.view,
        near: pane.context.fade[0],
        far: pane.context.fade[1],
        far_color: 0,
    };
    let caller_before = caller;
    let retained_projection = ModelProjection {
        focal: pane.context.focal,
        viewport: pane.context.viewport,
        center: pane.context.center,
        fade: pane.context.fade,
    };
    let grid = TerrainGrid {
        spacing: terrain.sampling().spacing.units(),
        mask: terrain.sampling().spacing.units() - 1,
    };
    let plan = terrain_draw_plan(
        pane.view.basis,
        retained_projection.fade,
        grid,
        crate::terrain_draw::TerrainFootprintWidth::Retail,
    )
    .unwrap();
    assert_eq!(plan.far_fade, [8320, 9984]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            scratch.clone(),
            (config).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut pixels,
        config,
    })
    .unwrap();
    scene.begin_frame().unwrap();
    {
        let mut host = ResidentSceneHost {
            resources: RenderResources::new(&resident),
            backend: &mut scene,
        };
        host.submit(SceneCommand::CoordinateShift(16)).unwrap();
        host.submit(SceneCommand::Setup(pane.context)).unwrap();
        host.submit(SceneCommand::PublishView(pane.view)).unwrap();
    }
    // Bind the actual decoded terrain to its selected renderer callback table.
    scene.set_terrain(Some(terrain.clone()));
    scene.install_terrain_surface_callbacks(world_flags, clock.value());
    let lights = WorldLightGrid::default();
    let mut furniture = FurnitureDrawState::new(&math);
    let pending = FurnitureEffectQueue::default();
    let mut particles = ProcessParticleRuntime::default();
    let process = ParticleFactoryProcessState::native_startup();
    let terrain_result = scene
        .draw_terrain(SoftwareTerrainDrawRequest {
            traversal: TerrainDrawRequest {
                terrain: &mut terrain,
                inputs: TerrainDrawInputs {
                    view: pane.view,
                    context_projection: retained_projection,
                    footprint: crate::terrain_draw::TerrainFootprintWidth::for_surface(
                        retained_projection.viewport,
                    ),
                    coordinate_shift: 16,
                    // Explicit stable graphics fixture policy, not a sampled color.
                    tint: [255; 3],
                    lighting: terrain_lighting_inputs(&lights, false, executable).unwrap(),
                    clock: clock.value(),
                    world_flags,
                    night: None,
                    particles_enabled: false,
                    tables: &tables,
                },
            },
            furniture: SoftwareFurnitureInputs {
                state: &mut furniture,
                pending: &pending,
                light_grid: &lights,
                particles: &mut particles,
                executable,
                birth_inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &process,
                },
                load_factor: 0,
                crystal_flares: None,
            },
        })
        .unwrap();
    assert!(scene.stats.terrain_commands > 0, "{terrain_result:?}");
    assert_eq!(caller, caller_before);
    assert_eq!(scene.environment.projection.fade, plan.far_fade);
    assert_eq!(
        scene.environment.fog_factor,
        SceneFade::new(plan.far_fade).factor
    );
    assert_eq!(scene.environment.view, pane.view);
    assert_eq!(scratch.state().unwrap(), ModelScratchDrawState::Ready);
    assert_eq!(scratch.cursor().unwrap(), 0);

    // No flush, new batch, scene replacement or scratch reset separates the
    // actual AA10 producer from this actual object-model adapter invocation.
    let published = scene.environment;
    let RenderResourceData::Model(model) = scene
        .resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 161,
        })
        .unwrap()
        .data
    else {
        panic!("original parent1 H8/161");
    };
    assert_eq!(model.header.reserved_10, 0);
    assert_eq!(model.header.vertex_ranges, [2, 2, 4, 18]);
    assert!(!model.header.flags.forces_near_table());
    let row = model.vertices[0];
    let raw = [i32::from(row.x), i32::from(row.y), i32::from(row.z)];
    let position = std::array::from_fn(|axis| {
        let forward = f64::from(f32::from_bits(published.view.basis.words[7 + axis]));
        published.view.eye[axis]
            .wrapping_add(fistp_low32(forward * 9152.0))
            .wrapping_sub(raw[axis])
    });
    let transform = ModelContext::new(position, MenuBasis::INITIAL).publish(published.view, 16);
    assert!(transform.in_frustum(model.header.visibility_extent, &published.projection));
    assert_eq!(
        transform.dispatch(model.header, &published.projection),
        Some(ModelDispatch::Far)
    );
    assert!(transform
        .dispatch(model.header, &retained_projection)
        .is_none());
    let before = scratch.read_pool::<12>(12).unwrap();
    let mut expected = ModelVertexProjection { bytes: before };
    let mut projector = ModelProjectionBackend {
        transform,
        view: published.view,
        projection: published.projection,
        dispatch: ModelDispatch::Far,
        coordinate_shift: 16,
        fog_factor: published.fog_factor,
    };
    assert!(
        projector
            .project(ModelVertexInput::Local(raw), &mut expected)
            .unwrap()
            >= 64
    );
    let mut arena = arena();
    let source = arena.allocate_actor(allocation()).unwrap();
    let arena_before = format!("{arena:?}");
    let queue_before = scene.queue.state();
    let rng_before = scene.process_rng;
    scene.execution_limits.instructions = 0; // explicit stop after native initialization
    let result = scene.draw_object_model(SoftwareObjectModelDrawRequest {
        terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
        packet: ObjectModelPacket {
            source,
            model_slot: 161,
            position,
            basis: MenuBasis::INITIAL,
        },
        root_mode: ModelRootMode::CommonVisual,
        context: &caller,
        arena: &mut arena,
        animation: &clock,
        executable,
    });
    assert!(
        matches!(
            result,
            Err(SoftwareSceneError::Model(ModelExecutionError::Limit))
        ),
        "{result:?}"
    );
    assert_eq!(scratch.read_pool::<12>(12).unwrap(), expected.bytes);
    assert_eq!(&scratch.read_pool::<12>(12).unwrap()[10..], &before[10..]);
    assert_eq!(scene.queue.state(), queue_before);
    assert_eq!(scene.process_rng, rng_before);
    assert_eq!(format!("{arena:?}"), arena_before);
    assert_eq!(scene.environment.view, published.view);
    assert_eq!(scene.environment.projection, published.projection);
    assert_eq!(scene.environment.fog_factor, published.fog_factor);
    assert!(matches!(
        scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
    // The test owns this initialized/error prefix; the earlier native terrain
    // primitives are genuine queued work, not a complete replay/preview claim.
}
