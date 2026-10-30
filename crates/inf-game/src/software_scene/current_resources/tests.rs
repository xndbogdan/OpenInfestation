use std::path::Path;

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    d3d_texture::TextureAllocationId, frontend_assets::FrontendAssets,
    frontend_display::frontend_startup, Installation, ResidencyRequest, ResidentRuntime,
    ResourceSelection,
};

struct Fixture {
    old: ResidentRuntime,
    current: ResidentRuntime,
    tables: PrimitiveTables,
}

impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let tables = PrimitiveTables::from_executable(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let mut old = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        old.apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
        old.apply(ResidencyRequest::Boot).unwrap();
        old.apply(ResidencyRequest::Activate { level: 47 }).unwrap();
        let mut current = old.clone();
        current
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        Self {
            old,
            current,
            tables,
        }
    }

    fn scene<'a>(&'a self, frame: &'a mut SoftwareFrame) -> SoftwareScene<'a, 'a> {
        SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (frontend_startup(
                    &self.old,
                    crate::presentation::NativeFrame::original([640, 480]),
                )
                .unwrap()
                .scene)
                    .environment,
            ),
            resources: RenderResources::new(&self.old),
            tables: &self.tables,
            frame,
            config: frontend_startup(
                &self.old,
                crate::presentation::NativeFrame::original([640, 480]),
            )
            .unwrap()
            .scene,
        })
        .unwrap()
    }
}

fn prefix(scene: &mut SoftwareScene<'_, '_>) {
    scene
        .queue
        .submit(DrawSubmission {
            depth: 100,
            payload_blocks: 3,
            receiver_offset: 0,
            command: star_command(
                crate::frontend_scene::StarQuad {
                    corners: [[1, 2], [3, 2], [3, 4], [1, 4]],
                    color_argument: 0,
                },
                0x7fff,
                100,
            ),
        })
        .unwrap();
}

fn cache_old_sprite(scene: &mut SoftwareScene<'_, '_>) -> TextureAllocationId {
    let sprite = scene
        .resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 7,
        })
        .unwrap();
    scene.texture_identities.resolve(sprite).unwrap();
    scene
        .pixels
        .textures
        .resolve(
            sprite,
            SpriteRenderMode::Mode0,
            scene.pixels.texture_state.upload,
        )
        .unwrap()
        .unwrap()
        .allocation_id
}

fn quad() -> DirectFlatQuad {
    DirectFlatQuad {
        corners: [[10, 10], [30, 10], [30, 30], [10, 30]],
        vertex_depths: [64; 4],
        descriptor_word: 0,
        color_word: 0x7fff,
    }
}

#[test]
fn error_before_native_reset_keeps_actual_queue_caches_and_renderer() {
    let fixture = Fixture::new();
    let mut frame = SoftwareFrame::new(640, 480, [1, 2, 3, 255]);
    let mut scene = fixture.scene(&mut frame);
    prefix(&mut scene);
    let allocation = cache_old_sprite(&mut scene);
    scene.stopped = true;
    scene.aborted_presentation = true;
    scene.set_process_rng(Some(0x1234_5678));
    let id = scene.id();
    let queue = scene.queue.state();
    let pixels = scene.pixels.frame.rgba().to_vec();
    let state = format!(
        "{:?}{:?}{:?}{:?}{:?}",
        scene.environment,
        scene.primitive_state,
        scene.pixels.renderer,
        scene.pixels.pipeline,
        scene.pixels.texture_state
    );
    let result = scene.with_current_resources(
        CurrentResourceDraw {
            resources: RenderResources::new(&fixture.current),
            batch_policy: ResourceBatchPolicy::DirectOnly,
        },
        |loan| {
            assert_eq!(loan.renderer_id(), id);
            assert_eq!(loan.dimensions(), [640, 480]);
            assert!(matches!(
                loan.scene_after_reset(),
                Err(ResourceLoanPhaseError::BatchNotReset)
            ));
            Err::<(), _>("surface preparation failed")
        },
    );
    assert!(matches!(
        result,
        Err(ResourceLoanError::Callback("surface preparation failed"))
    ));
    assert_eq!(scene.id(), id);
    assert_eq!(scene.queue.state(), queue);
    assert_eq!(scene.pixels.frame.rgba(), pixels);
    assert_eq!(
        format!(
            "{:?}{:?}{:?}{:?}{:?}",
            scene.environment,
            scene.primitive_state,
            scene.pixels.renderer,
            scene.pixels.pipeline,
            scene.pixels.texture_state
        ),
        state
    );
    assert_eq!(cache_old_sprite(&mut scene), allocation);
    assert_eq!(scene.pixels.textures.len(), 1);
    assert_eq!(scene.texture_identities.entries.len(), 1);
    assert_eq!(scene.process_rng(), Some(0x1234_5678));
    assert!(scene.stopped && scene.aborted_presentation);
    assert_eq!(scene.stats.begins, 0);
}

#[test]
fn reached_reset_direct_pixels_and_fresh_sources_keep_actual_id_and_frame() {
    let fixture = Fixture::new();
    let mut frame = SoftwareFrame::new(640, 480, [1, 2, 3, 255]);
    let mut scene = fixture.scene(&mut frame);
    prefix(&mut scene);
    let allocation = cache_old_sprite(&mut scene);
    let id = scene.id();
    let frame_address = &*scene.pixels.frame as *const SoftwareFrame;
    let capacity = scene.queue.state().capacity;
    scene
        .with_current_resources(
            CurrentResourceDraw {
                resources: RenderResources::new(&fixture.current),
                batch_policy: ResourceBatchPolicy::DirectOnly,
            },
            |loan| {
                assert_eq!(loan.renderer_id(), id);
                loan.reset_batch_40e6fd().unwrap();
                assert_eq!(
                    loan.reset_batch_40e6fd(),
                    Err(ResourceLoanPhaseError::BatchAlreadyReset)
                );
                let view = loan.scene_after_reset().unwrap();
                assert!(std::ptr::eq(
                    view.resources.resident_runtime(),
                    &fixture.current
                ));
                assert_eq!(&*view.pixels.frame as *const SoftwareFrame, frame_address);
                assert_eq!(view.id(), id);
                assert_eq!(view.queue.state().capacity, capacity);
                assert_eq!(view.queue.state().command_count, 0);
                view.draw_flat_quad_direct(quad())?;
                assert!(view.pixels.stats.fragments > 0);
                assert!(view
                    .pixels
                    .frame
                    .rgba()
                    .chunks_exact(4)
                    .any(|p| p != [1, 2, 3, 255]));
                Ok::<_, SoftwareSceneError>(())
            },
        )
        .unwrap();
    assert_eq!(scene.id(), id);
    assert_eq!(&*scene.pixels.frame as *const SoftwareFrame, frame_address);
    assert_eq!(scene.queue.state().command_count, 0);
    assert_eq!(scene.queue.state().capacity, capacity);
    assert_eq!(scene.stats.begins, 1);
    assert_eq!(scene.stats.frame_begins, 0);
    assert_eq!(scene.stats.direct_flat_quads, 1);
    assert!(scene.pixels.stats.fragments > 0);
    assert_eq!(cache_old_sprite(&mut scene), allocation);
    assert_eq!(scene.pixels.textures.len(), 1);
    assert_eq!(scene.texture_identities.entries.len(), 1);
}

#[test]
fn late_resource_error_returns_real_scalar_and_pixel_prefix_without_old_cache_reset() {
    let fixture = Fixture::new();
    let mut frame = SoftwareFrame::new(640, 480, [1, 2, 3, 255]);
    let mut scene = fixture.scene(&mut frame);
    let allocation = cache_old_sprite(&mut scene);
    let error = scene
        .with_current_resources(
            CurrentResourceDraw {
                resources: RenderResources::new(&fixture.current),
                batch_policy: ResourceBatchPolicy::DirectOnly,
            },
            |loan| {
                loan.reset_batch_40e6fd().unwrap();
                let view = loan.scene_after_reset().unwrap();
                view.draw_flat_quad_direct(quad())?;
                view.environment.coordinate_shift = 13;
                view.graphics.publish_shift(13).unwrap(); // explicit supplied fixture publication
                view.primitive_state.effect_color = 0xdead_beef;
                view.pixels.renderer.depth_write = 0;
                view.pixels.pipeline.alpha_blend_enabled = false;
                view.pixels.texture_state.sampling.address = 3;
                view.set_process_rng(Some(0xace0_2468));
                view.install_terrain_surface_callbacks(0x1234, 47);
                view.furniture_omissions.insert((5, 2, "reached".into()));
                view.water_omissions.insert("reached".into());
                view.draw_sprite_direct(DirectSpriteQuad {
                    sprite_slot: u16::MAX,
                    corners: quad().corners,
                    vertex_depths: [64; 4],
                    descriptor_word: 0,
                    mode: DirectSpriteMode::Textured,
                })
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ResourceLoanError::Callback(SoftwareSceneError::Resource(
            RenderResourceError::NegativeIndex {
                request: RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: 65535
                },
            }
        ))
    ));
    assert_eq!(scene.environment.coordinate_shift, 13);
    assert_eq!(scene.primitive_state.effect_color, 0xdead_beef);
    assert_eq!(scene.pixels.renderer.depth_write, 0);
    assert!(!scene.pixels.pipeline.alpha_blend_enabled);
    assert_eq!(scene.pixels.texture_state.sampling.address, 3);
    assert_eq!(scene.process_rng(), Some(0xace0_2468));
    assert_eq!(
        scene.surface_callbacks,
        RendererSurfaceCallbacks::Terrain {
            world_flags: 0x1234,
            wave_clock: 47
        }
    );
    assert!(scene
        .furniture_omissions
        .contains(&(5, 2, "reached".into())));
    assert!(scene.water_omissions.contains("reached"));
    assert_eq!(scene.stats.direct_flat_quads, 1);
    assert!(scene.pixels.stats.fragments > 0);
    assert!(scene.stopped);
    assert_eq!(scene.queue.state().command_count, 0);
    assert_eq!(cache_old_sprite(&mut scene), allocation);
}

#[test]
fn fresh_direct_upload_is_scoped_and_keeps_previous_epoch_allocations() {
    let fixture = Fixture::new();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = fixture.scene(&mut frame);
    let allocation = cache_old_sprite(&mut scene);
    scene
        .with_current_resources(
            CurrentResourceDraw {
                resources: RenderResources::new(&fixture.current),
                batch_policy: ResourceBatchPolicy::DirectOnly,
            },
            |loan| {
                loan.reset_batch_40e6fd().unwrap();
                let view = loan.scene_after_reset().unwrap();
                assert!(view.pixels.textures.is_empty());
                let resource = view.resources.resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: 7,
                })?;
                view.pixels
                    .textures
                    .resolve(
                        resource,
                        SpriteRenderMode::Mode0,
                        view.pixels.texture_state.upload,
                    )
                    .map_err(SoftwarePixelError::from)?;
                assert_eq!(view.pixels.textures.len(), 1);
                Ok::<_, SoftwareSceneError>(())
            },
        )
        .unwrap();
    assert_eq!(scene.pixels.textures.len(), 1);
    assert_eq!(cache_old_sprite(&mut scene), allocation);
    assert_eq!(scene.texture_identities.entries.len(), 1);
}

#[test]
fn unsupported_deferred_prefix_is_typed_and_retains_the_callback_error() {
    let fixture = Fixture::new();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = fixture.scene(&mut frame);
    prefix(&mut scene);
    let result = scene.with_current_resources(
        CurrentResourceDraw {
            resources: RenderResources::new(&fixture.current),
            batch_policy: ResourceBatchPolicy::DirectOnly,
        },
        |loan| {
            loan.reset_batch_40e6fd().unwrap();
            let view = loan.scene_after_reset().unwrap();
            prefix(view);
            Err::<(), _>("after deferred prefix")
        },
    );
    assert!(matches!(
        result,
        Err(ResourceLoanError::DeferredCommands {
            commands: 1,
            callback_error: Some("after deferred prefix"),
        })
    ));
    assert_eq!(scene.queue.state().command_count, 0);
    assert_eq!(scene.stats.begins, 1);
    assert!(scene.stopped);
}

#[test]
fn repeated_scopes_reset_only_at_marker_and_preserve_actual_draw_state() {
    let fixture = Fixture::new();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = fixture.scene(&mut frame);
    let id = scene.id();
    for attempt in 0..3 {
        scene
            .with_current_resources(
                CurrentResourceDraw {
                    resources: RenderResources::new(&fixture.current),
                    batch_policy: ResourceBatchPolicy::DirectOnly,
                },
                |loan| {
                    assert_eq!(loan.renderer_id(), id);
                    loan.reset_batch_40e6fd().unwrap();
                    let view = loan.scene_after_reset().unwrap();
                    assert_eq!(view.stats.direct_flat_quads, attempt);
                    view.draw_flat_quad_direct(quad())?;
                    Ok::<_, SoftwareSceneError>(())
                },
            )
            .unwrap();
    }
    assert_eq!(scene.id(), id);
    assert_eq!(scene.stats.begins, 3);
    assert_eq!(scene.stats.direct_flat_quads, 3);
    assert_eq!(scene.stats.frame_begins, 0);
}

#[test]
fn completed_loading_batch_retires_only_under_the_named_policy() {
    let fixture = Fixture::new();
    let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = fixture.scene(&mut frame);
    let id = scene.id();
    let allocation = cache_old_sprite(&mut scene);
    scene
        .with_current_resources(
            CurrentResourceDraw {
                resources: RenderResources::new(&fixture.current),
                batch_policy: ResourceBatchPolicy::CompletedResourceBatchRetirementApproximation,
            },
            |loan| {
                loan.reset_batch_40e6fd().unwrap();
                let view = loan.scene_after_reset().unwrap();
                prefix(view);
                view.submit_inner(ResolvedSceneCommand::FlushBatch)?;
                assert_eq!(view.queue.state().command_count, 1);
                assert!(view.queue.is_fully_replayed());
                Ok::<_, SoftwareSceneError>(())
            },
        )
        .unwrap();
    assert_eq!(scene.id(), id);
    assert_eq!(scene.stats.flushes, 1);
    assert_eq!(scene.stats.retired_resource_batches, 1);
    assert!(scene.pixels.stats.fragments > 0);
    assert_eq!(scene.queue.state().command_count, 0);
    assert!(!scene.stopped);
    assert_eq!(cache_old_sprite(&mut scene), allocation);
}

#[test]
fn retirement_never_admits_unflushed_direct_only_or_later_error_prefixes() {
    for (policy, flush, error) in [
        (
            ResourceBatchPolicy::CompletedResourceBatchRetirementApproximation,
            false,
            false,
        ),
        (ResourceBatchPolicy::DirectOnly, true, false),
        (
            ResourceBatchPolicy::CompletedResourceBatchRetirementApproximation,
            true,
            true,
        ),
    ] {
        let fixture = Fixture::new();
        let mut frame = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
        let mut scene = fixture.scene(&mut frame);
        let result = scene.with_current_resources(
            CurrentResourceDraw {
                resources: RenderResources::new(&fixture.current),
                batch_policy: policy,
            },
            |loan| {
                loan.reset_batch_40e6fd().unwrap();
                let view = loan.scene_after_reset().unwrap();
                prefix(view);
                if flush {
                    view.submit_inner(ResolvedSceneCommand::FlushBatch).unwrap();
                }
                if error {
                    Err("after replay")
                } else {
                    Ok(())
                }
            },
        );
        assert!(matches!(result, Err(ResourceLoanError::DeferredCommands {
            commands: 1, callback_error,
        }) if callback_error == error.then_some("after replay")));
        assert_eq!(scene.stats.flushes, usize::from(flush));
        assert_eq!(scene.stats.retired_resource_batches, 0);
        assert!(scene.stopped);
    }
}
