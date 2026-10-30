use std::path::Path;

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    d3d_texture::NativeTextureUploadState,
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::{
        draw_footer, FrontendScene, FrontendSceneHost, ModelCallbackPolicy, SceneCommand,
        SceneFrame, SceneProjection, SceneSetup, SceneView,
    },
    menu::MenuBasis,
    model_lighting::ModelLighting,
    scene_resources::ResidentSceneHost,
    software_frame::{D3dTextureSampling, FlatTriangle, FlatVertex},
    Installation, ResidencyRequest, ResidentRuntime,
};

fn resident() -> (ResidentRuntime, FrontendAssets, PrimitiveTables) {
    resident_variant(None)
}

/// `variant` selects the display tier before boot, as the display tests do.
fn resident_variant(variant: Option<u8>) -> (ResidentRuntime, FrontendAssets, PrimitiveTables) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    if let Some(difficulty) = variant {
        resident
            .apply(ResidencyRequest::Select {
                selection: crate::ResourceSelection {
                    difficulty,
                    language: 0,
                },
            })
            .unwrap();
    }
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    (resident, assets, tables)
}

fn config() -> SoftwareSceneConfig {
    SoftwareSceneConfig {
        environment: ModelEnvironment {
            view: SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            projection: ModelProjection {
                focal: [320, 240],
                viewport: [640, 480],
                center: [320, 240],
                fade: [0x10_0000; 2],
            },
            coordinate_shift: 0,
            fog_factor: 0x100_0000,
            lighting: ModelLighting {
                direction: [0.0, 0.0, 1.0],
                directional_word: 0x0080_8080,
                ambient: [32; 3],
                ambient_word: 0xab20_2020,
            },
        },
        primitive_mode: 0,
        far_color: 0,
        effect_color: 0xcd00_0000,
        color_layout: NativeColorLayout {
            red_shift: 10,
            green_shift: 5,
            blue_shift: 0,
        },
        // Explicit depth-enabled CPU display policy, using portable opaque
        // allocation identities rather than native process addresses.
        renderer: D3dFlatState {
            material_shifts: [6, 1, 4],
            near_depth: 64,
            manages_depth: true,
            depth_enable: 1,
            depth_write: 1,
        },
        pipeline: D3dRasterState {
            viewport: [0, 0, 639, 479],
            depth_enabled: false,
            depth_write: true,
            depth_function: 4,
            alpha_blend_enabled: true,
            shade_mode: 2,
            cull_mode: 1,
        },
        texture_state: SoftwareTextureState {
            upload: NativeTextureUploadState {
                source_rgb_shifts: [10, 5, 0],
                global_policy: 8,
            },
            sampling: D3dTextureSampling {
                min_filter: 1,
                mag_filter: 1,
                texture_blend: 4,
                address: 1,
                color_key_enabled: false,
                alpha_test_enabled: false,
            },
            secondary_texture: false,
        },
        queue_capacity: 0xc8000,
        execution_limits: ModelExecutionLimits {
            instructions: 100_000,
            invocation_depth: 16,
        },
    }
}

/// An arena below the heap's fresh-page threshold, whose bytes are residue.
const SMALL_ARENA: u32 = 0x4_0000;

#[test]
fn world_flash_uses_actual_prior_command_carry_and_the_existing_pixel_consumer() {
    let (mut runtime, assets, tables) = resident_variant(Some(2));
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            config().environment,
        ),
        resources: RenderResources::new(&runtime),
        tables: &tables,
        frame: &mut frame,
        config: SoftwareSceneConfig {
            queue_capacity: SMALL_ARENA,
            ..config()
        },
    })
    .unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut flash = crate::world_flash::WorldFlashState {
        sequence_5b8: 1,
        color_5bc: Some(0x7fff),
        ..Default::default()
    };
    let unknown = scene.draw_world_flash(&mut flash, [640, 480], exe);
    assert!(
        matches!(
            unknown,
            Err(crate::world_flash::WorldFlashError::Service {
                error: SoftwareFlashError::UnknownAllocationMode,
                ..
            })
        ),
        "{unknown:?}"
    );
    assert_eq!(flash.sequence_5b8, 1);
    assert_eq!(scene.queue.state().command_count, 0);
    assert!(!scene.queue.state().stopped);
    // An actual admitted star producer writes mode0 into this same allocation.
    scene
        .star(
            StarQuad {
                corners: [[1, 1], [3, 1], [3, 3], [1, 3]],
                color_argument: 0,
            },
            0x7fff,
        )
        .unwrap();
    scene
        .submit_inner(ResolvedSceneCommand::FlushBatch)
        .unwrap();
    scene
        .submit_inner(ResolvedSceneCommand::BeginBatch)
        .unwrap();
    assert_eq!(scene.queue.next_payload_word(0x20), Some(0));
    assert!(scene.draw_world_flash(&mut flash, [640, 480], exe).unwrap());
    assert_eq!(flash.sequence_5b8, 2);
    scene
        .submit_inner(ResolvedSceneCommand::FlushBatch)
        .unwrap();
    assert_eq!(scene.queue.state().last_flush_completed, 1);
    assert!(scene.queue.is_fully_replayed());
    scene.replace_command_arena(SMALL_ARENA).unwrap();
    assert_eq!(scene.queue.next_payload_word(0x20), None);
    // Retail's own 0xC8000 arena comes from fresh zero pages.
    scene.replace_command_arena(0xc8000).unwrap();
    assert_eq!(scene.queue.next_payload_word(0x20), Some(0));
}

#[test]
fn world_flash_over_unknown_carry_fills_the_display_and_leaves_the_carry_unknown() {
    let (mut runtime, assets, tables) = resident_variant(Some(2));
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    // 004A49FF: the display's clip rectangle ends at its inclusive dimensions.
    let config = SoftwareSceneConfig {
        pipeline: D3dRasterState {
            viewport: [0, 0, 640, 480],
            ..config().pipeline
        },
        queue_capacity: SMALL_ARENA,
        ..config()
    };
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            config.environment,
        ),
        resources: RenderResources::new(&runtime),
        tables: &tables,
        frame: &mut frame,
        config,
    })
    .unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut flash = crate::world_flash::WorldFlashState {
        sequence_5b8: 1,
        color_5bc: Some(0x80_8080),
        ..Default::default()
    };
    assert_eq!(scene.queue.next_payload_word(0x20), None);
    assert!(scene.draw_world_flash(&mut flash, [640, 480], exe).unwrap());
    assert_eq!(flash.sequence_5b8, 2);
    scene
        .submit_inner(ResolvedSceneCommand::FlushBatch)
        .unwrap();
    assert_eq!(scene.queue.state().last_flush_completed, 1);
    // Sprite 554's one texel, 7FFF without bit replication, at full diffuse.
    let frame = &scene.pixels.frame;
    for y in 0..480 {
        for x in 0..640 {
            assert_eq!(
                frame.pixel_rgba(x, y),
                Some([255, 255, 248, 255]),
                "{x},{y}"
            );
        }
    }
    // The packet never wrote +20: the next frame's allocation there is still
    // unknown carry, not a word the port chose.
    scene
        .submit_inner(ResolvedSceneCommand::BeginBatch)
        .unwrap();
    assert_eq!(scene.queue.next_payload_word(0x20), None);
}

#[test]
fn feature_recovery_retains_deferred_prefix_and_refuses_failed_queue() {
    let (runtime, _assets, tables) = resident();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources: RenderResources::new(&runtime),
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    let prefix = star_command(
        StarQuad {
            corners: [[1, 2], [3, 2], [3, 4], [1, 4]],
            color_argument: 0,
        },
        0x7fff,
        100,
    );
    scene
        .queue
        .submit(DrawSubmission {
            depth: 100,
            payload_blocks: 3,
            receiver_offset: 0,
            command: prefix,
        })
        .unwrap();
    let state = scene.queue.state();
    let view = scene.environment.view;
    let far_color = scene.primitive_state.far_color;
    scene.stopped = true;
    scene.continue_after_feature_error().unwrap();
    assert!(!scene.stopped);
    assert_eq!(scene.queue.state(), state);
    assert_eq!(scene.environment.view, view);
    assert_eq!(scene.primitive_state.far_color, far_color);
    scene
        .queue
        .submit(DrawSubmission {
            depth: 101,
            payload_blocks: usize::MAX,
            receiver_offset: 0,
            command: PrimitiveCommand::new(PrimitiveCallback::FlatQuadNear, 101, 0),
        })
        .unwrap_err();
    assert!(scene.queue.state().stopped);
    assert!(scene.continue_after_feature_error().is_err());
    assert_eq!(scene.queue.state().command_count, 1);
}

#[test]
fn failed_direct_flat_fill_preserves_pending_deferred_pixels() {
    let (resident, _, tables) = resident();
    let resources = RenderResources::new(&resident);
    let background = [2, 3, 5, 255];
    let mut frame = SoftwareFrame::new(640, 480, background);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    backend.submit(ResolvedSceneCommand::BeginBatch).unwrap();
    backend
        .star(
            StarQuad {
                corners: [[30, 30], [34, 30], [34, 34], [30, 34]],
                color_argument: 0,
            },
            0x7fff,
        )
        .unwrap();
    let pending = backend.queue.state();
    // Deliberately invalid zero depths exercise a pixel-consumer failure.
    // They are not an observed native BB40 fill packet.
    assert!(backend
        .draw_flat_quad_direct(DirectFlatQuad {
            corners: [[10, 10], [20, 10], [20, 20], [10, 20]],
            vertex_depths: [0; 4],
            descriptor_word: 0,
            color_word: 0x7fff,
        })
        .is_err());
    assert!(!backend.stopped());
    assert_eq!(backend.queue.state(), pending);
    assert_eq!(backend.pixels.frame.pixel_rgba(31, 31), Some(background));
    backend.submit(ResolvedSceneCommand::FlushBatch).unwrap();
    assert_ne!(backend.pixels.frame.pixel_rgba(31, 31), Some(background));
}

#[test]
fn native_arena_abort_permits_one_present_tail_but_keeps_drawing_stopped() {
    let (resident, _, tables) = resident();
    let resources = RenderResources::new(&resident);
    let mut frame = SoftwareFrame::new(640, 480, [2, 3, 5, 255]);
    let mut initial = config();
    initial.queue_capacity = 64;
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (initial).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: initial,
    })
    .unwrap();
    let error = ResidentSceneHost {
        resources,
        backend: &mut backend,
    }
    .submit(SceneCommand::Star(StarQuad {
        corners: [[30, 30], [31, 30], [31, 31], [30, 31]],
        color_argument: 0,
    }))
    .unwrap_err();
    let crate::scene_resources::SceneResourceError::Backend(error) = error else {
        panic!("actual queue failure expected");
    };
    assert_eq!(error.native_error_code(), Some(0x301));
    assert!(backend.stopped());
    assert!(backend.after_present([0; 4]).is_err());
    assert!(!backend.recover_frame_error(&SoftwareSceneError::Custody));
    assert!(backend.recover_frame_error(&error));
    assert_eq!(backend.pixels.frame.pixel_rgba(0, 0), Some([2, 3, 5, 255]));
    backend.after_present([9, 8, 7, 255]).unwrap();
    assert!(backend.stopped());
    assert_eq!(backend.pixels.frame.pixel_rgba(0, 0), Some([9, 8, 7, 255]));
    assert!(backend.after_present([0; 4]).is_err());
    assert!(backend
        .submit(ResolvedSceneCommand::AmbientLight([1; 3]))
        .is_err());
    backend.replace_command_arena(0xc8000).unwrap();
    backend
        .submit(ResolvedSceneCommand::AmbientLight([1; 3]))
        .unwrap();
    assert!(!backend.stopped());

    let nested = SoftwareSceneError::Model(ModelExecutionError::Host(ModelCommandError::Queue(
        DrawQueueFault::ArenaFull {
            requested: 64,
            available: 32,
        },
    )));
    assert_eq!(nested.native_error_code(), Some(0x301));
    assert_eq!(SoftwareSceneError::Custody.native_error_code(), None);
}

#[test]
fn light_setup_preserves_native_order_and_untouched_high_bytes() {
    let (resident, _, tables) = resident();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    backend
        .submit(ResolvedSceneCommand::DirectionalLight {
            direction: [-1.0, 0.0, 0.0],
            rgb: [255, 210, 170],
        })
        .unwrap();
    assert_eq!(backend.environment.lighting.direction, [-1.0, 0.0, 0.0]);
    assert_eq!(
        backend.environment.lighting.directional_word & 0xff_ffff,
        0xaa_d2ff
    );
    assert_eq!(backend.primitive_state.effect_color, 0xcd75_899f);
    backend
        .submit(ResolvedSceneCommand::AmbientLight([10, 6, 5]))
        .unwrap();
    assert_eq!(backend.environment.lighting.ambient, [10, 6, 5]);
    assert_eq!(backend.environment.lighting.ambient_word, 0xab05_060a);
    assert_eq!(backend.primitive_state.effect_color, 0xcd75_899f);
    backend
        .submit(ResolvedSceneCommand::DirectionalLight {
            direction: [0.0, 1.0, 0.0],
            rgb: [255, 210, 170],
        })
        .unwrap();
    assert_eq!(backend.primitive_state.effect_color, 0xcd5a_6f89);
    let setup = SceneSetup {
        focal: [800, 600],
        viewport: [640, 480],
        center: [320, 240],
        fade: [123, 456],
        fade_factor: 50_382,
    };
    backend.submit(ResolvedSceneCommand::Setup(setup)).unwrap();
    backend
        .submit(ResolvedSceneCommand::CoordinateShift(33))
        .unwrap();
    assert_eq!(backend.environment.coordinate_shift, 33);
    assert_eq!(backend.environment.fog_factor, 50_382);
    assert_eq!(backend.environment.projection.focal, [800, 600]);
    assert_eq!(
        backend.environment.projection,
        backend.primitive_state.projection
    );
}

#[test]
fn reset_clears_only_depth_and_keeps_native_renderer_fields() {
    let (resident, _, tables) = resident();
    let mut frame = SoftwareFrame::new(640, 480, [2, 3, 5, 255]);
    let mut initial = config();
    initial.renderer.depth_enable = 1;
    initial.pipeline.depth_enabled = true;
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (initial).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut frame,
        config: initial,
    })
    .unwrap();
    let triangle = FlatTriangle {
        vertices: [[1.0, 1.0], [20.0, 1.0], [1.0, 20.0]].map(|[x, y]| FlatVertex {
            x,
            y,
            depth: 100,
        }),
        mode: 0,
        material: inf_formats::MaterialValue::from_raw(0x7fff),
        rgb: [[128; 3]; 3],
    };
    backend
        .pixels
        .frame
        .fill_flat_triangle(
            &triangle,
            backend.pixels.renderer,
            &mut backend.pixels.pipeline,
        )
        .unwrap();
    assert!(backend.pixels.frame.pixel_depth(2, 2).unwrap() < 1.0);
    let rgba = backend.pixels.frame.rgba().to_vec();
    backend
        .submit(ResolvedSceneCommand::ResetAfterFirstModel)
        .unwrap();
    assert_eq!(backend.pixels.frame.pixel_depth(2, 2), Some(1.0));
    assert_eq!(backend.pixels.frame.rgba(), rgba);
    assert!(!backend.pixels.pipeline.depth_enabled);
    assert!(backend.pixels.pipeline.depth_write);
    assert_eq!(backend.pixels.renderer.depth_enable, 1);
    assert_eq!(backend.pixels.renderer.depth_write, 1);
    assert_eq!(backend.pixels.renderer.near_depth, 64);
    assert!(backend.material_sorting()); // device ZENABLE and renderer+3C differ
    backend.pixels.renderer.manages_depth = false;
    backend.pixels.pipeline.depth_enabled = true;
    backend.pixels.pipeline.depth_write = false;
    backend
        .submit(ResolvedSceneCommand::ResetAfterFirstModel)
        .unwrap();
    assert!(backend.pixels.pipeline.depth_enabled);
    assert!(!backend.pixels.pipeline.depth_write);
    assert!(!backend.material_sorting());
}

#[test]
fn fade_only_publication_preserves_context_view_focal_shift_and_light() {
    let (resident, _, tables) = resident();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    let context = SceneSetup::context(SceneProjection {
        focal: [340, 304],
        viewport: [633, 473],
        fade: [0x1800, 0x1a00],
    });
    backend
        .submit(ResolvedSceneCommand::Setup(context))
        .unwrap();
    let before = backend.environment;
    let fade = crate::frontend_scene::SceneFade::new([i32::MAX; 2]);
    backend
        .submit(ResolvedSceneCommand::PublishFade(fade))
        .unwrap();
    assert_eq!(backend.environment.projection.fade, [i32::MAX; 2]);
    assert_eq!(backend.environment.fog_factor, 0x100_0000);
    assert_eq!(backend.primitive_state.projection.fade, [i32::MAX; 2]);
    assert_eq!(context.fade, [0x1800, 0x1a00]);
    assert_eq!(
        backend.environment.projection.focal,
        before.projection.focal
    );
    assert_eq!(
        backend.environment.projection.viewport,
        before.projection.viewport
    );
    assert_eq!(
        backend.environment.projection.center,
        before.projection.center
    );
    assert_eq!(backend.environment.view, before.view);
    assert_eq!(
        backend.environment.coordinate_shift,
        before.coordinate_shift
    );
    assert_eq!(backend.environment.lighting, before.lighting);
    backend
        .submit(ResolvedSceneCommand::PublishFade(
            crate::frontend_scene::SceneFade::new(context.fade),
        ))
        .unwrap();
    assert_eq!(backend.environment.projection.fade, context.fade);
    assert_eq!(backend.environment.fog_factor, context.fade_factor);
}

#[test]
fn two_frames_clear_after_presentation_and_preserve_live_scene_state() {
    let (resident, _, tables) = resident();
    let resources = RenderResources::new(&resident);
    let texture = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1803,
        })
        .unwrap();
    let mut frame = SoftwareFrame::new(640, 480, [2, 3, 5, 255]);
    let mut initial = config();
    initial.queue_capacity = 0x1000;
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (initial).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: initial,
    })
    .unwrap();
    let renderer = backend.pixels.renderer;
    let inherited_view = SceneView {
        eye: [17, 29, 43],
        basis: MenuBasis::INITIAL,
    };
    let triangle = FlatTriangle {
        vertices: [[1.0, 1.0], [20.0, 1.0], [1.0, 20.0]].map(|[x, y]| FlatVertex {
            x,
            y,
            depth: 100,
        }),
        mode: 0,
        material: inf_formats::MaterialValue::from_raw(0x7fff),
        rgb: [[128; 3]; 3],
    };
    let mut background = [2, 3, 5, 255];
    for frame_index in 0..2 {
        backend.begin_frame().unwrap();
        assert_eq!(backend.queue.state().capacity, 0xc8000);
        assert_eq!(backend.queue.state().command_count, 0);
        assert_eq!(backend.queue.state().open_groups, 1);
        assert_eq!(backend.pixels.frame.pixel_rgba(2, 2), Some(background));
        assert_eq!(backend.pixels.frame.pixel_depth(2, 2), Some(1.0));
        assert!(!backend.pixels.pipeline.depth_enabled);
        assert!(backend.pixels.pipeline.depth_write);
        assert_eq!(backend.pixels.renderer, renderer);
        assert_eq!(
            backend.texture_identities.resolve(texture).unwrap(),
            TextureSortIdentity::PortableEntryOrdinal(0)
        );
        if frame_index == 1 {
            assert_eq!(backend.environment.view, inherited_view);
            assert_eq!(backend.environment.coordinate_shift, 7);
            assert_eq!(backend.environment.lighting.ambient_word, 0xab05_060a);
            assert_eq!(backend.primitive_state.effect_color, 0xcd75_899f);
        }
        backend
            .submit(ResolvedSceneCommand::DirectionalLight {
                direction: [-1.0, 0.0, 0.0],
                rgb: [255, 210, 170],
            })
            .unwrap();
        assert_eq!(
            backend.primitive_state.effect_color,
            if frame_index == 0 {
                0xcd75_899f
            } else {
                0xcd5a_6f89
            }
        );
        backend
            .submit(ResolvedSceneCommand::AmbientLight([10, 6, 5]))
            .unwrap();
        backend
            .submit(ResolvedSceneCommand::PublishView(inherited_view))
            .unwrap();
        backend
            .submit(ResolvedSceneCommand::CoordinateShift(7))
            .unwrap();
        backend
            .pixels
            .frame
            .fill_flat_triangle(&triangle, renderer, &mut backend.pixels.pipeline)
            .unwrap();
        let presented_pixel = backend.pixels.frame.pixel_rgba(2, 2).unwrap();
        assert_ne!(presented_pixel, background);
        assert!(backend.pixels.frame.pixel_depth(2, 2).unwrap() < 1.0);
        // Queue storage is deliberately left populated to prove that the next
        // outer begin replaces it. The direct triangle above is already drawn.
        backend
            .star(
                StarQuad {
                    corners: [[30, 30], [31, 30], [31, 31], [30, 31]],
                    color_argument: 0,
                },
                0x7fff,
            )
            .unwrap();
        assert_eq!(backend.queue.state().command_count, 1);
        background = [19 + frame_index, 37, 71, 255];
        backend.after_present(background).unwrap();
        assert_ne!(presented_pixel, background);
        assert!(backend
            .pixels
            .frame
            .rgba()
            .chunks_exact(4)
            .all(|pixel| pixel == background));
        assert_eq!(backend.pixels.frame.pixel_depth(2, 2), Some(1.0));
        assert!(!backend.pixels.pipeline.depth_enabled);
        assert!(backend.pixels.pipeline.depth_write);
        assert_eq!(backend.pixels.renderer, renderer);
        assert_eq!(backend.texture_identities.entries.len(), 1);
    }
    assert_eq!(backend.stats.frame_begins, 2);
    assert_eq!(backend.stats.presentation_clears, 2);
    assert_eq!(backend.stats.depth_resets, 2);

    // DirectDraw color-fill does not touch device/depth state. Make depth
    // nontrivial using the supported D3D callback before switching ownership.
    backend
        .pixels
        .frame
        .fill_flat_triangle(&triangle, renderer, &mut backend.pixels.pipeline)
        .unwrap();
    let old_depth = backend.pixels.frame.pixel_depth(2, 2);
    assert!(old_depth.unwrap() < 1.0);
    backend.pixels.renderer.manages_depth = false;
    backend.pixels.pipeline.depth_enabled = true;
    backend.pixels.pipeline.depth_write = false;
    backend.begin_frame().unwrap();
    assert_eq!(backend.pixels.frame.pixel_depth(2, 2), old_depth);
    assert_ne!(backend.pixels.frame.pixel_rgba(2, 2), Some(background));
    backend.after_present([59, 83, 127, 255]).unwrap();
    assert_eq!(
        backend.pixels.frame.pixel_rgba(2, 2),
        Some([59, 83, 127, 255])
    );
    assert_eq!(backend.pixels.frame.pixel_depth(2, 2), old_depth);
    assert!(backend.pixels.pipeline.depth_enabled);
    assert!(!backend.pixels.pipeline.depth_write);
}

#[test]
fn resident_authored_scene_and_footer_reach_concrete_pixels() {
    let (resident, assets, tables) = resident();
    let scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let camera = &assets.camera_presets()[0];
    let scene_frame = SceneFrame {
        projection: SceneProjection {
            focal: [160, 120],
            viewport: [640, 480],
            fade: [0x10_0000; 2],
        },
        inherited_view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        camera_eye: camera.eye.map(|v| v * assets.position_scale()),
        camera_look_at: camera.target.map(|v| v * assets.position_scale()),
        animation: AnimationClock::default(),
        age_millis: 2000,
        exit_millis: 0,
    };
    let clear = [2, 3, 5, 255];
    let mut frame = SoftwareFrame::new(640, 480, clear);
    let resources = RenderResources::new(&resident);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    let outcome;
    {
        let mut host = ResidentSceneHost {
            resources,
            backend: &mut backend,
        };
        outcome = scene.draw_scene(scene_frame, &mut host).unwrap();
        draw_footer(&mut host).unwrap();
        host.submit(SceneCommand::FlushBatch).unwrap();
    }
    assert_eq!(outcome.submitted_models, 9);
    assert_eq!(backend.stats.models, 9);
    assert_eq!(backend.stats.stars, outcome.submitted_stars as usize);
    assert!(backend.stats.stars > 0);
    assert_eq!(backend.stats.begins, 11);
    assert_eq!(backend.stats.flushes, 11);
    assert_eq!(backend.stats.depth_resets, 1);
    assert_eq!(backend.stats.footers, 6);
    assert_eq!(backend.pixels.stats.unsupported, 0);
    assert!(backend.pixels.stats.fragments > 0);
    assert!(backend.pixels.textures.len() > 1);
    assert_eq!(backend.queue.state().command_count, 0); // footer is immediate
    assert!(!backend.stopped());
    let changed = backend
        .pixels
        .frame
        .rgba()
        .chunks_exact(4)
        .filter(|pixel| *pixel != clear)
        .count();
    assert!(changed > 1000);
    if let Some(path) = std::env::var_os("INF_SCENE_PIXEL_SMOKE_PPM") {
        let mut ppm = b"P6\n640 480\n255\n".to_vec();
        for pixel in backend.pixels.frame.rgba().chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}

#[test]
fn resident_name_entry_backdrop_reaches_concrete_pixels() {
    use crate::name_entry_scene::{draw_name_entry_scene, NameEntrySceneFrame};
    // Display variant 2 is the 640x480 tier whose H2 words give the focal pair.
    let (resident, assets, tables) = resident_variant(Some(2));
    let startup = crate::frontend_display::frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let clear = [2, 3, 5, 255];
    let mut frame = SoftwareFrame::new(640, 480, clear);
    let resources = RenderResources::new(&resident);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    {
        let mut host = ResidentSceneHost {
            resources,
            backend: &mut backend,
        };
        // 00459310 begins the batch before 0045A560 and flushes it after the
        // stars; this smoke keeps only that bracket.
        host.submit(SceneCommand::BeginBatch).unwrap();
        draw_name_entry_scene(
            NameEntrySceneFrame {
                projection: startup.projection,
                age_millis: 9000,
                animation: AnimationClock::default(),
                selector_one: 1,
            },
            &crate::menu_basis::MenuBasisMath::new(assets.sine_table().values),
            &mut host,
        )
        .unwrap();
        host.submit(SceneCommand::FlushBatch).unwrap();
    }
    assert_eq!(backend.stats.models, 3);
    assert_eq!(backend.stats.begins, 1);
    assert_eq!(backend.stats.flushes, 1);
    assert_eq!(
        backend.environment.projection.focal,
        startup.projection.focal
    );
    assert_eq!(backend.environment.lighting.ambient, [9, 5, 4]);
    assert_eq!(backend.pixels.stats.unsupported, 0);
    assert!(backend.pixels.stats.fragments > 0);
    assert!(!backend.stopped());
    let changed = backend
        .pixels
        .frame
        .rgba()
        .chunks_exact(4)
        .filter(|pixel| *pixel != clear)
        .count();
    assert!(changed > 1000, "{changed} changed pixels");
    if let Some(path) = std::env::var_os("INF_NAME_ENTRY_BACKDROP_PPM") {
        let mut ppm = b"P6\n640 480\n255\n".to_vec();
        for pixel in backend.pixels.frame.rgba().chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}

#[test]
fn wrong_resolved_source_stops_without_rolling_back_published_state() {
    let (resident, _, tables) = resident();
    let resources = RenderResources::new(&resident);
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut initial = config();
    initial.renderer.depth_enable = 1;
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (initial).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: initial,
    })
    .unwrap();
    backend.submit(ResolvedSceneCommand::BeginBatch).unwrap();
    backend
        .submit(ResolvedSceneCommand::CoordinateShift(7))
        .unwrap();
    let wrong = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 751,
        })
        .unwrap();
    let RenderResourceData::Model(model) = wrong.data else {
        unreachable!()
    };
    let failure = backend
        .submit(ResolvedSceneCommand::Model {
            submission: SceneModelSubmission {
                model_slot: 744,
                position: [0, 0, 100_000],
                basis: MenuBasis::INITIAL,
                animation: AnimationClock::default(),
                primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
                failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
                surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
                policy: ModelCallbackPolicy::Frontend,
            },
            source: wrong.source,
            model,
        })
        .unwrap_err();
    assert!(matches!(failure, SoftwareSceneError::Custody));
    assert!(backend.stopped());
    assert_eq!(backend.environment.coordinate_shift, 7);
    assert!(matches!(
        backend.submit(ResolvedSceneCommand::FlushBatch),
        Err(SoftwareSceneError::Stopped)
    ));
    backend.submit(ResolvedSceneCommand::BeginBatch).unwrap();
    assert!(!backend.stopped());
    assert_eq!(backend.queue.state().command_count, 0);
}

#[test]
fn portable_texture_identity_shares_aliases_but_retains_source_ownership() {
    let (resident, _, _) = resident();
    let resources = RenderResources::new(&resident);
    let texture = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1803,
        })
        .unwrap();
    let other = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1804,
        })
        .unwrap();
    let copied_source = texture.source.bytes.to_vec();
    let mut identities = TextureIdentities::default();
    assert_eq!(
        identities.resolve(texture).unwrap(),
        TextureSortIdentity::PortableEntryOrdinal(0)
    );
    let mut alias = texture;
    alias.request.argument = 27; // different pool alias, same original entry/source
    assert_eq!(
        identities.resolve(alias).unwrap(),
        TextureSortIdentity::PortableEntryOrdinal(0)
    );
    assert_eq!(
        identities.resolve(other).unwrap(),
        TextureSortIdentity::PortableEntryOrdinal(1)
    );
    let mut other_source = texture;
    other_source.source.bytes = &copied_source;
    assert_eq!(
        identities.resolve(other_source).unwrap(),
        TextureSortIdentity::PortableEntryOrdinal(2)
    );
    assert_eq!(
        identities.resolve(texture).unwrap(),
        TextureSortIdentity::PortableEntryOrdinal(0)
    );
    assert_eq!(identities.entries.len(), 3);
}

/// The hold display's carried actors draw over the default pair (retail's
/// `004EBCD0`, not the world's `0044FF50`), and the world's selection
/// comes back after them.
#[test]
fn carried_actors_draw_over_the_default_pair_and_restore_the_worlds() {
    let (resident, _, tables) = resident();
    let mut frame = SoftwareFrame::new(640, 480, [2, 3, 5, 255]);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    backend.install_terrain_surface_callbacks(0x40, 1234);
    let selection = backend.select_default_surface_callbacks();
    assert!(matches!(
        backend.model_surface_binding(ModelSurfaceTableOrigin::InheritedRenderer, None),
        crate::model_vertices::ModelSurfaceBinding::Default
    ));
    backend.restore_surface_callbacks(selection);
    assert!(matches!(
        backend.model_surface_binding(ModelSurfaceTableOrigin::InheritedRenderer, None),
        crate::model_vertices::ModelSurfaceBinding::Terrain {
            world_flags: 0x40,
            wave_clock: 1234,
            ..
        }
    ));
}

#[test]
fn retained_state_rebinds_to_a_snapshot_without_resetting_the_renderer() {
    let (resident, _, tables) = resident();
    let mut frame = SoftwareFrame::new(640, 480, [2, 3, 5, 255]);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    backend.refresh_terrain_surface_globals(0x40, 1234);
    assert!(matches!(
        backend.model_surface_binding(ModelSurfaceTableOrigin::InheritedRenderer, None),
        crate::model_vertices::ModelSurfaceBinding::Default
    ));
    backend.install_terrain_surface_callbacks(0x40, 1234);
    assert!(matches!(
        backend.model_surface_binding(ModelSurfaceTableOrigin::TemplateDefault, None),
        crate::model_vertices::ModelSurfaceBinding::Default
    ));
    let id = backend.id();
    backend.replace_command_arena(0x2000).unwrap();
    backend.reset_renderer();
    backend.environment.view.eye = [17, 29, 43];
    backend.environment.lighting.ambient = [4, 5, 6];
    backend.primitive_state.far_color = 0x1234;
    backend.pixels.pipeline.depth_enabled = true;
    backend.pixels.stats.fragments = 99;
    backend.after_present([9, 8, 7, 255]).unwrap();
    let stats = backend.stats;
    assert_eq!(stats.presentation_clears, 1);
    let state = backend.into_state();
    assert_eq!(state.id(), id);

    // A clone is a distinct registration table sharing the same banks.
    let snapshot = resident.clone();
    let mut backend = SoftwareScene::resume(
        state,
        SoftwareSceneResume {
            resources: RenderResources::new(&snapshot),
            tables: &tables,
            frame: &mut frame,
        },
    )
    .unwrap();
    assert_eq!(backend.id(), id);
    assert_eq!(backend.environment.view.eye, [17, 29, 43]);
    // A closed pane refreshes process inputs while retaining its earlier
    // installer. Missing terrain is retained for the reached callback error;
    // it never silently switches this inherited draw to the template table.
    backend.refresh_terrain_surface_globals(0x80, 5678);
    assert!(matches!(
        backend.model_surface_binding(ModelSurfaceTableOrigin::InheritedRenderer, None),
        crate::model_vertices::ModelSurfaceBinding::Terrain {
            terrain: None,
            world_flags: 0x80,
            wave_clock: 5678,
            renderer_xyz: [17, 29, 43],
        }
    ));
    assert!(matches!(
        backend.model_surface_binding(ModelSurfaceTableOrigin::TemplateDefault, None),
        crate::model_vertices::ModelSurfaceBinding::Default
    ));
    assert_eq!(backend.environment.lighting.ambient, [4, 5, 6]);
    assert_eq!(backend.primitive_state.far_color, 0x1234);
    // after_present's depth-surface reset and all counters survive.
    assert!(!backend.pixels.pipeline.depth_enabled);
    assert!(backend.pixels.pipeline.depth_write);
    assert_eq!(backend.pixels.stats.fragments, 99);
    assert_eq!(backend.stats, stats);
    // Custody bound to the retired snapshot is gone; the arena keeps its size.
    assert_eq!(backend.pixels.textures.len(), 0);
    assert_eq!(backend.queue.state().capacity, 0x2000);
    assert_eq!(backend.queue.state().command_count, 0);
    assert_eq!(backend.pixels.frame.rgba()[..4], [9, 8, 7, 255]);
    backend.begin_frame().unwrap();
    assert_eq!(backend.stats.frame_begins, stats.frame_begins + 1);
}

#[test]
fn depth_policy_mirrors_renderer_plus10_enable_write_pair() {
    let (resident, _, tables) = resident();
    let mut frame = SoftwareFrame::new(640, 480, [2, 3, 5, 255]);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config()).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut frame,
        config: config(),
    })
    .unwrap();
    // Retail frame 1 runs 004BC9B0(1,1): device and saved words together.
    backend.set_depth_policy(1, 1).unwrap();
    assert_eq!(backend.pixels.renderer.depth_enable, 1);
    assert_eq!(backend.pixels.renderer.depth_write, 1);
    assert!(backend.pixels.pipeline.depth_enabled);
    assert!(backend.pixels.pipeline.depth_write);
    backend.set_depth_policy(0, 0).unwrap();
    assert!(!backend.pixels.pipeline.depth_enabled);
    assert!(!backend.pixels.pipeline.depth_write);
    assert!(backend.set_depth_policy(2, 1).is_err());
    assert!(backend.set_depth_policy(1, 7).is_err());
}

#[path = "software_scene/terrain/tests/live_models.rs"]
mod live_models;

/// Port widescreen: a 4:3 canvas keeps its own clip rectangle across its
/// draws, and the whole frame keeps its own, so a canvas screen's full-canvas
/// clip never narrows the next full-frame screen.
#[test]
fn canvas_and_frame_keep_separate_clip_rectangles() {
    let (resident, _assets, tables) = resident();
    let mut frame = SoftwareFrame::new(1365, 768, [0, 0, 0, 255]);
    let canvas = frame.four_three_canvas().unwrap();
    let mut config = config();
    config.pipeline.viewport = [0, 0, 1365, 768];
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            config.environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut frame,
        config,
    })
    .unwrap();
    scene.enter_canvas(canvas);
    assert_eq!(scene.pixels.pipeline.viewport, [0, 0, 1024, 768]);
    assert_eq!(scene.pixels.frame.width(), 1024);
    // The briefing restores its whole-display clip, inclusive.
    scene.pixels.pipeline.viewport = [0, 0, 1023, 767];
    scene.leave_canvas();
    assert_eq!(scene.pixels.pipeline.viewport, [0, 0, 1365, 768]);
    assert_eq!(scene.pixels.frame.width(), 1365);
    assert_eq!(scene.pixels.frame.canvas(), None);
    scene.pixels.pipeline.viewport = [10, 20, 1300, 700];
    scene.enter_canvas(canvas);
    assert_eq!(scene.pixels.pipeline.viewport, [0, 0, 1023, 767]);
    scene.leave_canvas();
    assert_eq!(scene.pixels.pipeline.viewport, [10, 20, 1300, 700]);
    // Leaving without a canvas changes nothing.
    scene.leave_canvas();
    assert_eq!(scene.pixels.pipeline.viewport, [10, 20, 1300, 700]);
}

#[test]
fn a_refused_shade_read_omits_its_model_and_keeps_custody() {
    use crate::model_lighting::ModelLightingError;
    use crate::model_scratch::ModelScratchError;
    let refused = || {
        ModelLightingError::Scratch(ModelScratchError::MappedShade {
            slot: -900,
            offset: -332,
        })
    };
    // Both wrappings a primitive's lighting can arrive in.
    for error in [
        SoftwareSceneError::Model(ModelExecutionError::Host(ModelCommandError::Primitive(
            crate::model_primitives::PrimitiveError::Lighting(refused()),
        ))),
        SoftwareSceneError::Model(ModelExecutionError::Lighting(refused())),
    ] {
        assert!(error.is_unmapped_shade_read());
        assert!(!error.is_model_scratch_fault());
        assert!(!error.is_renderer_custody_fault());
    }
    // A real scratch custody fault keeps stopping the scene.
    let custody = SoftwareSceneError::Model(ModelExecutionError::Lighting(
        ModelLightingError::Scratch(ModelScratchError::BorrowConflict),
    ));
    assert!(custody.is_model_scratch_fault());
    assert!(custody.is_renderer_custody_fault());
}
