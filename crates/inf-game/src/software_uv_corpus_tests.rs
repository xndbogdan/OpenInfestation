//! Real close-range frontend geometry through near clipping, deferred UV packets,
//! resident texture upload and CPU pixels. The small camera/device setup is an
//! explicit inspection fixture, not an authored camera or retail image match.

use std::{collections::BTreeSet, path::Path, sync::Arc};

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    draw_queue::DrawQueue,
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::{ModelCallbackPolicy, SceneModelSubmission, SceneView},
    menu::MenuBasis,
    model_commands::{ModelCommandBackend, ModelCommandServices},
    model_context::{ModelContext, ModelProjection},
    model_execution::{ModelDrawRequest, ModelEnvironment, ModelExecutionLimits, ModelExecutor},
    model_lighting::ModelLighting,
    model_primitives::{PrimitiveState, PrimitiveTables, TextureSortIdentity},
    model_values::{ModelValueCallback, ModelValues},
    pixel_backend::flush_pixels,
    render_resources::{RenderResourceData, RenderResources},
    Installation, ResidencyRequest, ResidentRuntime,
};

struct UnbatchedSceneFixture(SceneModelSubmission);

impl ModelValueCallback for UnbatchedSceneFixture {
    type Error = &'static str;

    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        self.0
            .callback_value(selector)
            .ok_or("pure virtual callback")
    }
}

impl<'a> ModelCommandServices<'a> for UnbatchedSceneFixture {
    fn prepare_draw(
        &mut self,
        _: &mut DrawQueue<PrimitiveCommand<'a>>,
    ) -> Result<bool, Self::Error> {
        Ok(false) // Explicit fixture policy: no material batching.
    }

    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        Err("unbatched inspection must not ask for fabricated texture addresses")
    }
}

#[test]
fn close_range_resident_models_reach_uv_pixels_through_actual_queue() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = ResidentRuntime::new(
        Installation::open(&root).expect("retail Infestation data is required"),
    )
    .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let resources = RenderResources::new(&resident);
    let environment = ModelEnvironment {
        view: SceneView {
            // Raising the eye exposes the XZ quads while preserving their
            // mixed near-plane depths at model position z=2000.
            eye: [0, 1000, 0],
            basis: MenuBasis::INITIAL,
        },
        projection: ModelProjection {
            focal: [64; 2],
            viewport: [256; 2],
            center: [128; 2],
            fade: [1_000_000, 2_000_000],
        },
        coordinate_shift: 0,
        fog_factor: 16,
        lighting: ModelLighting {
            direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            directional_word: 0x00aa_d2ff,
            ambient: [10, 6, 5],
            ambient_word: 0xab05_060a,
        },
    };
    for model in [751, 755] {
        assert_eq!(resident.resource(8, model).unwrap().registration.level, 47);
        let submission = SceneModelSubmission {
            model_slot: model as i16,
            position: [0, 0, 2000],
            basis: MenuBasis::INITIAL,
            animation: AnimationClock::default(),
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            policy: ModelCallbackPolicy::Frontend,
        };
        let mut queue = DrawQueue::new(128 * 1024).unwrap();
        let mut commands = ModelCommandBackend::new(
            resources,
            &tables,
            PrimitiveState {
                mode: 0,
                far_color: 0,
                effect_color: 0,
                projection: environment.projection,
            },
            &mut queue,
            UnbatchedSceneFixture(submission),
        );
        let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = ModelExecutor::new(
            resources,
            Arc::clone(&tables.sine_table),
            ModelExecutionLimits {
                instructions: 100_000,
                invocation_depth: 16,
            },
        );
        executor
            .draw(
                ModelDrawRequest {
                    scratch: &scratch,
                    root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                    model_argument: i32::from(model),
                    context: ModelContext::new(submission.position, submission.basis),
                    environment,
                    surface: crate::model_vertices::ModelSurfaceBinding::Default,
                },
                &mut commands,
            )
            .unwrap();
        assert!(!commands.stopped());
        assert!(!executor.stopped());
        let command_count = commands.outcome().commands;
        drop(commands);

        let clear = [2, 3, 5, 255];
        let mut frame = SoftwareFrame::new(256, 256, clear);
        let mut pixels = SoftwarePixelBackend::new(
            &mut frame,
            D3dFlatState {
                material_shifts: [6, 1, 4],
                near_depth: 64,
                manages_depth: true,
                depth_enable: 1,
                depth_write: 1,
            },
            D3dRasterState {
                viewport: [0, 0, 256, 256],
                depth_enabled: true,
                depth_write: true,
                depth_function: 4,
                alpha_blend_enabled: true,
                shade_mode: 2,
                cull_mode: 1,
            },
            super::tests::texture_state(),
        );
        let mut uv_commands = 0;
        let mut uv_fragments = 0;
        let mut uv_pixel_changes = 0;
        let mut clipped_depths = BTreeSet::new();
        let mut texture_entries = BTreeSet::new();
        assert_eq!(
            queue
                .flush(|invocation, command| {
                    if let Some((_, texture)) = command.texture {
                        let original = resources.resolve(texture.request).unwrap();
                        assert!(std::ptr::eq(
                            texture.source.registration,
                            original.source.registration
                        ));
                        assert!(std::ptr::eq(texture.source.bytes, original.source.bytes));
                        assert!(matches!(texture.data, RenderResourceData::Sprite(_)));
                        texture_entries.insert((
                            texture.source.registration.level,
                            texture.source.registration.local_index,
                        ));
                    }
                    let uv = matches!(
                        command.callback,
                        PrimitiveCallback::TexturedUvTriangleNear
                            | PrimitiveCallback::TexturedUvQuadNear
                    );
                    let before_fragments = pixels.stats.fragments;
                    let before = uv.then(|| pixels.frame.rgba().to_vec());
                    let depth_before = uv.then(|| pixels.frame.depth.clone());
                    let flags = if uv {
                        uv_commands += 1;
                        let primitive = read_textured_uv(command).unwrap().unwrap();
                        let count = if matches!(
                            command.callback,
                            PrimitiveCallback::TexturedUvTriangleNear
                        ) {
                            3
                        } else {
                            4
                        };
                        for i in 0..count {
                            clipped_depths.insert(command.read_u32(i * 8 + 4).unwrap());
                        }
                        // Native near packet has no shade/tail data to invent.
                        assert!(command.payload[count * 16 + 8..]
                            .iter()
                            .all(Option::is_none));
                        Some(primitive.flags)
                    } else {
                        None
                    };
                    pixels.draw(invocation, command)?;
                    if let Some(flags) = flags {
                        uv_fragments += pixels.stats.fragments - before_fragments;
                        uv_pixel_changes += before
                            .unwrap()
                            .chunks_exact(4)
                            .zip(pixels.frame.rgba().chunks_exact(4))
                            .filter(|(a, b)| a != b)
                            .count();
                        assert_eq!(pixels.pipeline.depth_write, flags & 0x39 == 0);
                        if flags & 0x39 != 0 {
                            assert_eq!(pixels.frame.depth, depth_before.unwrap());
                        }
                    }
                    Ok::<_, SoftwarePixelError>(())
                })
                .unwrap(),
            command_count
        );
        let covered = pixels
            .frame
            .rgba()
            .chunks_exact(4)
            .filter(|pixel| *pixel != clear)
            .count();
        assert!(uv_commands > 0, "model {model}");
        assert!(
            clipped_depths.contains(&64),
            "native near intersection for model {model}"
        );
        assert!(
            clipped_depths.iter().any(|&depth| depth > 64),
            "front-side vertices for model {model}"
        );
        assert!(uv_fragments > 0 && uv_pixel_changes > 0, "model {model}");
        assert!(covered > 100 && covered < 256 * 256, "model {model}");
        assert_eq!(
            pixels.stats.textured_uv_triangles + pixels.stats.textured_uv_quads,
            uv_commands
        );
        assert_eq!(pixels.stats.unsupported, 0);
        assert_eq!(pixels.stats.skipped, 0);
        assert_eq!(pixels.textures.len(), texture_entries.len());
        assert!(!queue.state().stopped);

        // Successful replay preserves the commands and resident cache. Clear
        // only the target and replay the same real queue to prove stable output.
        let first_rgba = pixels.frame.rgba().to_vec();
        let first_depth = pixels.frame.depth.clone();
        let cache_count = pixels.textures.len();
        pixels.frame.clear_color(clear);
        pixels.frame.clear_depth();
        assert_eq!(
            flush_pixels(&mut queue, &mut pixels).unwrap(),
            command_count
        );
        assert_eq!(pixels.textures.len(), cache_count);
        assert_eq!(pixels.frame.rgba(), first_rgba);
        assert_eq!(pixels.frame.depth, first_depth);
    }
}
