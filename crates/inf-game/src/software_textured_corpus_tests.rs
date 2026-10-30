//! Original resident frontend child751 commands through native texture upload and
//! the CPU Direct3D target. Camera/device values are explicit inspection
//! fixtures, not an authored frontend view or matched retail screenshot.

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
    render_resources::{RenderResource, RenderResourceData, RenderResourceKind, RenderResources},
    Installation, ResidencyRequest, ResidentRuntime,
};

struct UnbatchedFrontendFixture;

impl ModelValueCallback for UnbatchedFrontendFixture {
    type Error = &'static str;

    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        Err("original frontend child751 does not invoke an animation callback")
    }
}

impl<'a> ModelCommandServices<'a> for UnbatchedFrontendFixture {
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
        Err("unbatched original frontend commands must not require fabricated texture addresses")
    }
}

#[test]
fn original_resident_frontend_quads_reach_textured_pixels() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let model_source = resident.resource(8, 751).unwrap();
    assert_eq!(model_source.registration.level, 47);
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let resources = RenderResources::new(&resident);
    let environment = ModelEnvironment {
        view: SceneView {
            eye: [0, 100_000, 0],
            // Look down world -Y with world +Z as screen-up. These four
            // original quads occupy the XZ plane.
            basis: MenuBasis {
                words: [
                    0,
                    1.0f32.to_bits(),
                    0,
                    0,
                    0,
                    0,
                    1.0f32.to_bits(),
                    0,
                    (-1.0f32).to_bits(),
                    0,
                ],
            },
        },
        projection: ModelProjection {
            focal: [2048; 2],
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
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let request = ModelDrawRequest {
        scratch: &scratch,
        root_mode: crate::model_execution::ModelRootMode::CommonVisual,
        model_argument: 751,
        context: ModelContext::new([0; 3], MenuBasis::INITIAL),
        environment,
        surface: crate::model_vertices::ModelSurfaceBinding::Default,
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
        UnbatchedFrontendFixture,
    );
    let mut executor = ModelExecutor::new(
        resources,
        Arc::clone(&tables.sine_table),
        ModelExecutionLimits {
            instructions: 100_000,
            invocation_depth: 16,
        },
    );
    executor.draw(request, &mut commands).unwrap();
    assert_eq!(commands.outcome().commands, 4);
    assert_eq!(commands.outcome().rejected, 0);
    assert!(!commands.stopped());
    drop(commands);

    assert_eq!(
        queue
            .flush(|_, command| {
                assert_eq!(command.callback, PrimitiveCallback::TexturedQuadNear);
                let (offset, texture) = command.texture.unwrap();
                assert_eq!(offset, 0x24);
                assert_eq!(texture.request.kind, RenderResourceKind::Sprite);
                assert_eq!(texture.request.slot(), 1803);
                assert_eq!(texture.source.registration.level, 47);
                assert_eq!(texture.source.registration.local_index, 22);
                assert!(matches!(texture.data, RenderResourceData::Sprite(_)));
                let original = resources.resolve(texture.request).unwrap();
                assert!(std::ptr::eq(
                    texture.source.registration,
                    original.source.registration
                ));
                assert!(std::ptr::eq(texture.source.bytes, original.source.bytes));
                assert!(command.payload[offset..offset + 4]
                    .iter()
                    .all(Option::is_none));
                let quad = read_textured_quad(command).unwrap().unwrap();
                assert_eq!(quad.mode, 0);
                assert_eq!(quad.flags, 0x14); // native additive, no depth writes
                assert!(quad.vertices.iter().all(|vertex| vertex.depth == 100_000));
                Ok::<(), std::convert::Infallible>(())
            })
            .unwrap(),
        4
    );

    let clear = [2, 3, 5, 255];
    let mut frame = SoftwareFrame::new(256, 256, clear);
    let renderer = D3dFlatState {
        material_shifts: [6, 1, 4],
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    };
    let pipeline = D3dRasterState {
        viewport: [0, 0, 255, 255],
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
    assert_eq!(flush_pixels(&mut queue, &mut pixels).unwrap(), 4);
    assert_eq!(pixels.stats.textured_quads, 4);
    assert_eq!(pixels.stats.triangles, 0);
    assert_eq!(pixels.stats.skipped, 0);
    assert_eq!(pixels.stats.unsupported, 0);
    assert_eq!(pixels.textures.len(), 1);
    assert!(pixels.stats.fragments > 0);
    let covered = pixels
        .frame
        .rgba()
        .chunks_exact(4)
        .filter(|pixel| *pixel != clear)
        .count();
    assert!(covered > 1000 && covered < 256 * 256);
    let colors = pixels
        .frame
        .rgba()
        .chunks_exact(4)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        colors.len() > 16,
        "original texture must retain color variation"
    );
    // Descriptor0014 selects additive blending and disables depth writes.
    assert!(!pixels.pipeline.depth_write);
    assert!(pixels.frame.depth.iter().all(|&depth| depth == 1.0));
    assert_eq!(pixels.frame.pixel_rgba(0, 0), Some(clear));
    assert!(!queue.state().stopped);
    if let Some(path) = std::env::var_os("INF_TEXTURED_CORPUS_PIXEL_SMOKE_PPM") {
        let mut ppm = b"P6\n256 256\n255\n".to_vec();
        for pixel in pixels.frame.rgba().chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}
