//! Source-bounded composed batch regressions. Original resources are unchanged.
//! The camera/clock/RNG and malformed-header/small-arena cases are declared
//! fixture inputs, not observed retail values or a complete briefing claim.

use super::*;
use crate::{
    draw_queue::{DrawQueue, DrawQueueError, DrawSubmission},
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    model_scratch::{ModelScratchDrawState, ProcessModelScratch},
    software_frame::SoftwarePixelError,
};

#[path = "map_models.rs"]
mod map_models;

// The existing pixel helper requires a healthy scene on return. This fixture
// instead lends the actual owner through a caller that may deliberately stop
// the real queue/pixel backend. It reuses the original resource/terrain/effect
// constructors, and creates no successful substitute renderer operation.
fn with_batch_fixture<T>(
    tier: u8,
    dimensions: [u32; 2],
    draw: impl FnOnce(&mut SoftwareBriefingFrame<'_, '_, '_>, BriefingSceneFrame, &MenuBasisMath) -> T,
) -> T {
    let (assets, tables, resident) = level1_fixture(tier);
    let resources = RenderResources::new(&resident);
    let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original(dimensions),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let zero = executable.loader_zero_data(0x0056_4728, 0xfa004).unwrap();
    let scratch = ProcessModelScratch::native_loader_zero(&zero).unwrap();
    let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], PIXEL_GATE_CLEAR);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            scratch,
            startup.scene.environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    scene.set_terrain(Some(placements.terrain().clone()));
    scene.set_process_rng(Some(0)); // explicit stable fixture input
    scene.environment.coordinate_shift = 8; // inherited caller fixture policy
    scene.graphics.publish_shift(8).unwrap(); // explicit supplied fixture publication
    let mut state = FrontendRendererState::default();
    let mut arena = crate::object_runtime::ObjectArena::new();
    let map_state = crate::world_map::WorldMapState::new();
    let mut terrain = owned_terrain(&placements);
    let terrain_source =
        crate::world_actor_terrain::WorldTerrainSource::retain(placements.terrain_source());
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
            terrain_source: &terrain_source,
            arena: &mut arena,
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
        object_draw_failure_policy:
            crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
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
    )
}

fn pane_inputs(frame: BriefingSceneFrame) -> BriefingTerrainInputs {
    BriefingTerrainInputs {
        frame,
        animation: open_panel(),
        // Declared screen-camera input; no capture-derived actor pose.
        camera_position_words: [0x1000, 400, 0x2000],
        heading: 0x1234,
    }
}

#[test]
fn closed_terrain_leaf_keeps_one_outer_begin_and_depth_reset_only() {
    with_batch_fixture(2, [640, 480], |owner, frame, math| {
        // Genuine earlier model commands demonstrate that the upstream Begin
        // is an actual reset, even when BB40's leaf immediately returns.
        owner
            .panel_frame(BriefingPanel::Terrain, open_panel(), frame, math)
            .unwrap();
        assert!(owner.scene.queue.state().command_count > 0);
        let begins = owner.scene.stats.begins;
        let resets = owner.scene.stats.depth_resets;
        let models = owner.scene.stats.models;
        let flushes = owner.scene.stats.flushes;
        let before_environment = format!("{:?}", owner.scene.environment);
        let before_rng = owner.scene.process_rng();
        let before_pixels = owner.scene.pixels.frame.rgba().to_vec();
        let before_arena = format!("{:?}", owner.world.arena);
        let mut inputs = pane_inputs(frame);
        inputs.animation.current_stage = 0;
        inputs.animation.requested_stage = 0;
        assert!(!panel_visible(inputs.animation));
        owner
            .terrain_panel(
                inputs,
                BriefingTerrainEffects {
                    camera_disabled: false,
                    pulse_remaining_micros: 0,
                    report_omissions: false,
                },
                math,
                &FrontendProcessState::default(),
            )
            .unwrap();
        assert_eq!(owner.scene.stats.begins, begins + 1);
        assert_eq!(owner.scene.stats.depth_resets, resets + 1);
        assert_eq!(owner.scene.stats.models, models);
        assert_eq!(owner.scene.stats.flushes, flushes);
        assert_eq!(owner.scene.queue.state().command_count, 0);
        assert!(owner.scene.queue.has_only_implicit_root());
        assert_eq!(format!("{:?}", owner.scene.environment), before_environment);
        assert_eq!(owner.scene.process_rng(), before_rng);
        assert_eq!(owner.scene.pixels.frame.rgba(), before_pixels);
        assert_eq!(format!("{:?}", owner.world.arena), before_arena);
        assert!(!owner.scene.stopped());
    });
}

#[test]
fn original_backdrop_fill_and_ground_share_the_native_content_batch_at_both_tiers() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let fill_only = with_batch_fixture(tier, dimensions, |owner, frame, math| {
            let pane = briefing_terrain_pane(pane_inputs(frame), math)
                .unwrap()
                .unwrap();
            owner.scene.pixels.pipeline.viewport = pane.clip_rect;
            let color = owner.material_word(0x72).unwrap();
            owner
                .scene
                .draw_flat_quad_direct(pane.far_fill(color))
                .unwrap();
            owner.scene.pixels.frame.rgba().to_vec()
        });
        with_batch_fixture(tier, dimensions, |owner, frame, math| {
            let pane = briefing_terrain_pane(pane_inputs(frame), math)
                .unwrap()
                .unwrap();
            let begins = owner.scene.stats.begins;
            let flushes = owner.scene.stats.flushes;
            let resets = owner.scene.stats.depth_resets;
            owner.terrain_backdrop(pane, frame).unwrap();
            assert_eq!(owner.scene.stats.models, 1, "actual original H8/278");
            assert!(owner.scene.stats.model_commands > 0);
            assert_eq!(owner.scene.stats.direct_flat_quads, 1, "actual H7/72 fill");
            let pending = owner.scene.queue.state();
            assert!(pending.command_count > 0);
            assert_eq!(pending.last_flush_completed, 0);
            assert!(owner.scene.queue.has_only_implicit_root());
            assert_eq!(owner.scene.stats.begins, begins);
            assert_eq!(owner.scene.stats.flushes, flushes);
            assert_eq!(owner.scene.stats.depth_resets, resets);
            assert_eq!(
                owner.scene.pixels.frame.rgba(),
                fill_only,
                "deferred H8/278 has not overwritten the immediate fill"
            );
            assert!(fill_only
                .chunks_exact(4)
                .any(|rgba| rgba != PIXEL_GATE_CLEAR));
            assert_eq!(
                owner.scene.scratch.state().unwrap(),
                ModelScratchDrawState::Ready
            );
            assert_eq!(owner.scene.scratch.cursor().unwrap(), 0);

            owner
                .terrain_ground(pane, frame, false, &FrontendProcessState::default(), false)
                .unwrap();
            assert!(
                owner.scene.stats.terrain_commands > 0,
                "real original H12 producer"
            );
            assert_eq!(owner.scene.stats.flushes, flushes + 1);
            assert_eq!(owner.scene.stats.begins, begins + 1);
            assert_eq!(owner.scene.stats.depth_resets, resets + 1);
            assert_eq!(owner.scene.queue.state().command_count, 0);
            assert!(owner.scene.queue.has_only_implicit_root());
            assert!(
                owner
                    .scene
                    .pixels
                    .frame
                    .rgba()
                    .chunks_exact(4)
                    .zip(fill_only.chunks_exact(4))
                    .any(|(actual, fill)| actual != fill),
                "actual queued content must contribute pixels beyond the immediate fill"
            );

            // The actual enclosing frame is a later native batch, after the
            // content flush/depth reset. No full screen/campaign claim follows.
            owner.scene.pixels.pipeline.viewport =
                [0, 0, dimensions[0] as i16 - 1, dimensions[1] as i16 - 1];
            let models_before_frame = owner.scene.stats.models;
            owner
                .panel_frame(BriefingPanel::Terrain, open_panel(), frame, math)
                .unwrap();
            assert_eq!(owner.scene.stats.models, models_before_frame + 1);
            assert!(owner.scene.queue.state().command_count > 0);
            owner
                .flush_feature("actual enclosing terrain frame")
                .unwrap();
            assert_eq!(owner.scene.stats.flushes, flushes + 2);
            assert!(!owner.scene.stopped());
        });
    }
}

#[test]
fn failed_real_pixel_replay_keeps_pixels_queue_and_stop_without_a_new_begin() {
    with_batch_fixture(2, [640, 480], |owner, frame, math| {
        let pane = briefing_terrain_pane(pane_inputs(frame), math)
            .unwrap()
            .unwrap();
        owner.terrain_backdrop(pane, frame).unwrap();
        assert!(owner.scene.queue.state().command_count > 0);
        // This supplied malformed header exercises the real pixel receiver
        // validation. Its callback never reports successful fake raster work.
        // Minimum signed sort word puts it after the real backdrop payloads.
        let bad = PrimitiveCommand::new(PrimitiveCallback::FlatQuadNear, i32::MIN as u32, 3);
        owner
            .scene
            .queue
            .submit(DrawSubmission {
                depth: bad.depth,
                payload_blocks: bad.payload_blocks,
                receiver_offset: 0x10,
                command: bad,
            })
            .unwrap();
        let before_queue = owner.scene.queue.state();
        let before_pixels = owner.scene.pixels.frame.rgba().to_vec();
        let before_fragments = owner.scene.pixels.stats.fragments;
        let begins = owner.scene.stats.begins;
        let resets = owner.scene.stats.depth_resets;
        let flushes = owner.scene.stats.flushes;
        let error = owner
            .flush_feature("supplied invalid pixel receiver")
            .unwrap_err();
        assert!(
            matches!(
                error,
                SoftwareSceneError::Flush(DrawQueueError::Host(SoftwarePixelError::Receiver {
                    offset: 0x10
                }))
            ),
            "{error:?}"
        );
        let after_queue = owner.scene.queue.state();
        assert_eq!(after_queue.command_count, before_queue.command_count);
        assert!(
            after_queue.command_cursor >= before_queue.command_cursor,
            "root closure writes stay retained"
        );
        assert_eq!(after_queue.pending_cursor, after_queue.capacity);
        assert_eq!(after_queue.open_groups, 0);
        assert!(after_queue.last_flush_completed > 0);
        assert!(after_queue.stopped && owner.scene.stopped());
        assert!(owner.scene.pixels.stats.fragments > before_fragments);
        assert_ne!(owner.scene.pixels.frame.rgba(), before_pixels);
        assert_eq!(owner.scene.stats.begins, begins);
        assert_eq!(owner.scene.stats.depth_resets, resets);
        assert_eq!(owner.scene.stats.flushes, flushes);
        let retained_pixels = owner.scene.pixels.frame.rgba().to_vec();
        assert!(matches!(
            owner.flush_feature("no automatic retry"),
            Err(SoftwareSceneError::Stopped)
        ));
        assert_eq!(owner.scene.queue.state(), after_queue);
        assert_eq!(owner.scene.pixels.frame.rgba(), retained_pixels);
        assert_eq!(owner.scene.stats.begins, begins);
    });
}

#[test]
fn actual_backdrop_allocator_301_survives_the_enclosing_terrain_catch() {
    with_batch_fixture(2, [640, 480], |owner, frame, math| {
        // A declared small native-style arena, not an authored asset change.
        // The actual H8/278 producer must allocate its real first command.
        owner.scene.queue = DrawQueue::new(16).unwrap();
        let begins = owner.scene.stats.begins;
        let resets = owner.scene.stats.depth_resets;
        let flushes = owner.scene.stats.flushes;
        let result = owner.terrain_panel(
            pane_inputs(frame),
            BriefingTerrainEffects {
                camera_disabled: false,
                pulse_remaining_micros: 0,
                report_omissions: false,
            },
            math,
            &FrontendProcessState::default(),
        );
        let error = result.unwrap_err();
        let BriefingRendererError::Scene(error) = error else {
            panic!("{error:?}")
        };
        assert_eq!(error.native_error_code(), Some(0x301), "{error:?}");
        assert!(owner.scene.queue.state().stopped);
        assert!(owner.scene.stopped());
        assert_eq!(
            owner.scene.stats.begins,
            begins + 1,
            "only the actual outer bracket"
        );
        assert_eq!(owner.scene.stats.depth_resets, resets + 1);
        assert_eq!(owner.scene.stats.flushes, flushes);
        assert_eq!(
            owner.scene.stats.direct_flat_quads, 0,
            "no fabricated normal fill suffix"
        );
        assert_eq!(owner.scene.stats.models, 0);
        assert_eq!(owner.scene.queue.state().capacity, 16);
        assert_eq!(owner.scene.queue.state().command_cursor, 0);
        assert_eq!(owner.scene.queue.state().pending_cursor, 16);
        assert!(matches!(
            owner.scene.scratch.state().unwrap(),
            ModelScratchDrawState::Stopped(_)
        ));
        // Queue/pixel/scratch custody stays stopped. This test does not consume
        // the native301 outer presentation permit or claim game unwind parity.
    });
}

// Append to the held batching child after root promotion. This is a declared
// sibling-composition fixture, not a claim that native BB40 invokes this preview.
#[test]
fn pending_original_backdrop_survives_actual_preview_feature_containment() {
    with_batch_fixture(2, [640, 480], |owner, frame, math| {
        let model = leaf12_thumper_preview();
        let pane = briefing_terrain_pane(pane_inputs(frame), math)
            .unwrap()
            .unwrap();
        owner.terrain_backdrop(pane, frame).unwrap();
        let pending_backdrop = owner.scene.queue.state();
        assert!(pending_backdrop.command_count > 0);
        assert!(owner.scene.queue.has_only_implicit_root());
        assert_eq!(
            owner.scene.scratch.state().unwrap(),
            ModelScratchDrawState::Ready
        );
        let before_preview_environment = format!("{:?}", owner.scene.environment);
        let before_pixels = owner.scene.pixels.frame.rgba().to_vec();
        let begins = owner.scene.stats.begins;
        let flushes = owner.scene.stats.flushes;
        let resets = owner.scene.stats.depth_resets;

        // Capture actual healthy root/scratch custody before the real producer.
        // The failure is the thumper's unowned register read, not a test host
        // returning a successful substitute or an instruction-budget stop.
        let entry = owner.feature_entry().unwrap();
        let error = draw_briefing_model_preview(
            model,
            2000,
            model_extent(owner.resources, model.model_slot).unwrap(),
            false,
            frame,
            math,
            &mut ResidentSceneHost {
                resources: owner.resources,
                backend: &mut *owner.scene,
            },
        )
        .unwrap_err();
        assert!(
            is_thumper_register_stop(&error),
            "actual thumper preview boundary: {error:?}"
        );
        let errored_queue = owner.scene.queue.state();
        assert!(
            !errored_queue.stopped,
            "producer failure is not failed replay"
        );
        assert!(owner.scene.stopped());
        assert!(errored_queue.command_count >= pending_backdrop.command_count);
        assert!(errored_queue.command_cursor >= pending_backdrop.command_cursor);
        assert!(errored_queue.pending_cursor <= pending_backdrop.pending_cursor);
        assert_eq!(errored_queue.last_flush_completed, 0);
        assert_eq!(owner.scene.pixels.frame.rgba(), before_pixels);
        assert_ne!(
            format!("{:?}", owner.scene.environment),
            before_preview_environment,
            "the real preview's reached setup/view/fade publications are retained"
        );
        assert_eq!(
            owner.scene.primitive_state.far_color, 0,
            "B840's reached FarColor publication precedes the model error"
        );
        assert!(matches!(
            owner.scene.scratch.state().unwrap(),
            ModelScratchDrawState::Stopped(_)
        ));
        let reached_environment = format!("{:?}", owner.scene.environment);
        let reached_primitive_state = format!("{:?}", owner.scene.primitive_state);
        let reached_viewport = owner.scene.pixels.pipeline.viewport;
        let reached_rng = owner.scene.process_rng();
        let reached_scratch = owner.scene.scratch.read_pool::<384>(0).unwrap();

        // Consume the production classifier and custody guard. Only this
        // model's registered marks/new groups may retire; queued siblings,
        // reached bytes and scalar publications survive without Begin/Flush.
        owner
            .omit_feature(
                "actual thumper preview pending sibling fixture",
                entry,
                error,
            )
            .unwrap();
        let recovered_queue = owner.scene.queue.state();
        assert_eq!(recovered_queue.command_count, errored_queue.command_count);
        assert!(recovered_queue.command_cursor >= errored_queue.command_cursor);
        assert!(recovered_queue.pending_cursor >= errored_queue.pending_cursor);
        assert!(recovered_queue.pending_cursor <= pending_backdrop.pending_cursor);
        assert_eq!(recovered_queue.last_flush_completed, 0);
        assert!(owner.scene.queue.has_only_implicit_root());
        assert!(!owner.scene.stopped());
        assert_eq!(
            owner.scene.scratch.state().unwrap(),
            ModelScratchDrawState::Ready
        );
        assert_eq!(owner.scene.scratch.cursor().unwrap(), 0);
        assert_eq!(
            owner.scene.scratch.read_pool::<384>(0).unwrap(),
            reached_scratch
        );
        assert_eq!(
            format!("{:?}", owner.scene.environment),
            reached_environment
        );
        assert_eq!(
            format!("{:?}", owner.scene.primitive_state),
            reached_primitive_state
        );
        assert_eq!(owner.scene.pixels.pipeline.viewport, reached_viewport);
        assert_eq!(owner.scene.process_rng(), reached_rng);
        assert_eq!(owner.scene.pixels.frame.rgba(), before_pixels);
        assert_eq!(owner.scene.stats.begins, begins);
        assert_eq!(owner.scene.stats.flushes, flushes);
        assert_eq!(owner.scene.stats.depth_resets, resets);

        // Real AA10/lighting/object producers and the native content flush
        // follow. This proves the retained batch remains usable, without
        // claiming normal preview completion, full pane or retail ordering.
        let fragments = owner.scene.pixels.stats.fragments;
        owner
            .terrain_ground(pane, frame, false, &FrontendProcessState::default(), false)
            .unwrap();
        assert!(owner.scene.stats.terrain_commands > 0);
        assert!(owner.scene.pixels.stats.fragments > fragments);
        assert_ne!(owner.scene.pixels.frame.rgba(), before_pixels);
        assert_eq!(owner.scene.stats.flushes, flushes + 1);
        assert_eq!(owner.scene.stats.begins, begins + 1);
        assert_eq!(owner.scene.stats.depth_resets, resets + 1);
        assert!(owner.scene.queue.has_only_implicit_root());
        assert_eq!(owner.scene.queue.state().command_count, 0);
        assert_eq!(
            owner.scene.scratch.state().unwrap(),
            ModelScratchDrawState::Ready
        );
        assert!(!owner.scene.stopped());
    });
}
