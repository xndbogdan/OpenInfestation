//! Original resident geometry through the complete near-flat pipeline. The
//! camera, viewport and device state below are explicit synthetic fixtures;
//! they are neither an authored menu scene nor a matched retail screenshot.

use std::{path::Path, sync::Arc};

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    draw_queue::DrawQueue,
    frontend_assets::FrontendAssets,
    frontend_scene::SceneView,
    menu::MenuBasis,
    model_commands::{ModelCommandBackend, ModelCommandServices},
    model_context::{ModelContext, ModelProjection},
    model_execution::{ModelDrawRequest, ModelEnvironment, ModelExecutionLimits, ModelExecutor},
    model_lighting::ModelLighting,
    model_primitives::{PrimitiveState, PrimitiveTables},
    model_values::{ModelValueCallback, ModelValues},
    pixel_backend::flush_pixels,
    render_resources::{RenderResource, RenderResources},
    Installation, ResidencyRequest, ResidentRuntime,
};

struct UnbatchedFixture;

impl ModelValueCallback for UnbatchedFixture {
    type Error = &'static str;

    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        Err("static original model169 does not invoke an animation callback")
    }
}

impl<'a> ModelCommandServices<'a> for UnbatchedFixture {
    fn prepare_draw(
        &mut self,
        _: &mut DrawQueue<PrimitiveCommand<'a>>,
    ) -> Result<bool, Self::Error> {
        Ok(false) // explicit fixture policy: no material batching
    }

    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        Err("flat-only original model169 has no texture allocation identity")
    }
}

#[test]
fn original_resident_flat_model_reaches_pixels_with_explicit_synthetic_camera() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 11 })
        .unwrap();
    let model_source = resident.resource(8, 169).unwrap();
    assert_eq!(model_source.registration.level, 1);
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let resources = RenderResources::new(&resident);
    let environment = ModelEnvironment {
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        projection: ModelProjection {
            focal: [4096; 2],
            viewport: [128; 2],
            center: [64; 2],
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
        UnbatchedFixture,
    );
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let mut executor = ModelExecutor::new(
        resources,
        Arc::clone(&tables.sine_table),
        ModelExecutionLimits {
            instructions: 10_000,
            invocation_depth: 16,
        },
    );
    executor
        .draw(
            ModelDrawRequest {
                scratch: &scratch,
                root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                model_argument: 169,
                context: ModelContext::new([0, 0, 3000], MenuBasis::INITIAL),
                environment,
                surface: crate::model_vertices::ModelSurfaceBinding::Default,
            },
            &mut commands,
        )
        .unwrap();
    assert_eq!(commands.outcome().commands, 18);
    assert_eq!(commands.outcome().rejected, 102);
    assert!(!commands.stopped());
    drop(commands);

    // Inspect without substituting geometry, material or shade data. A later
    // flush replays the same original packets into the concrete pixel target.
    let mut colored_corner = false;
    assert_eq!(
        queue
            .flush(|_, command| {
                assert_eq!(command.callback, PrimitiveCallback::FlatTriangleNear);
                assert!(command.texture.is_none());
                let triangle = read_flat_triangle(command).unwrap();
                assert_eq!(triangle.mode, 0);
                colored_corner |= triangle.rgb.iter().any(|rgb| *rgb != [0; 3]);
                Ok::<(), std::convert::Infallible>(())
            })
            .unwrap(),
        18
    );
    assert!(colored_corner);

    let clear = [2, 3, 5, 255];
    let mut frame = SoftwareFrame::new(128, 128, clear);
    let renderer = D3dFlatState {
        material_shifts: [6, 1, 4],
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    };
    let pipeline = D3dRasterState {
        viewport: [0, 0, 127, 127],
        depth_enabled: true,
        depth_write: true,
        depth_function: 4,
        alpha_blend_enabled: true,
        shade_mode: 2,
        cull_mode: 1,
    };
    let mut pixels = SoftwarePixelBackend::new(
        &mut frame,
        renderer,
        pipeline,
        super::tests::texture_state(),
    );
    assert_eq!(flush_pixels(&mut queue, &mut pixels).unwrap(), 18);
    assert_eq!(pixels.stats.triangles, 18);
    assert!(pixels.stats.fragments > 0);
    let mut covered = 0;
    for y in 0..128 {
        for x in 0..128 {
            let depth = pixels.frame.pixel_depth(x, y).unwrap();
            assert!(depth.is_finite());
            if depth < 1.0 {
                covered += 1;
                assert!(depth > 0.9);
                assert_ne!(pixels.frame.pixel_rgba(x, y), Some(clear));
            }
        }
    }
    assert!(covered > 0 && covered < 128 * 128);
    let rgba = pixels.frame.rgba().to_vec();
    let depth = pixels.frame.depth.clone();
    let fragments = pixels.stats.fragments;
    assert_eq!(flush_pixels(&mut queue, &mut pixels).unwrap(), 18);
    assert_eq!(pixels.stats.triangles, 36);
    assert_eq!(pixels.stats.fragments, fragments * 2);
    assert_eq!(pixels.frame.rgba(), rgba);
    assert_eq!(pixels.frame.depth, depth);
    assert!(!queue.state().stopped);
    if let Some(path) = std::env::var_os("INF_CORPUS_PIXEL_SMOKE_PPM") {
        let mut ppm = b"P6\n128 128\n255\n".to_vec();
        for pixel in pixels.frame.rgba().chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}
