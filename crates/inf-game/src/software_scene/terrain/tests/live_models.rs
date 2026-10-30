//! The renderer epoch stays deliberately stale while the real model callback
//! binding loans terrain changed by the screen's 004502C0 policy owner.
use super::*;
use crate::model_vertices::{
    ModelSurfaceCallbackInputs, ModelSurfaceVertexRequest, ModelVertexCommand, ModelVertexInput,
    ModelVertexProjection, ModelVertexProjector, ModelVertices,
};
use crate::terrain_runtime::{ProcessTerrainPolicy, TerrainDetailContext, TerrainSpacing};

struct NoProjection;
impl ModelVertexProjector for NoProjection {
    type Error = std::convert::Infallible;
    fn project(
        &mut self,
        _: ModelVertexInput,
        _: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        unreachable!("mode one writes world coordinates without projection")
    }
}

#[test]
fn live_model_terrain_2b_2c_observe_changed_policy_without_replacing_renderer_epoch() {
    let (resident, _, tables) = resident();
    let mut frame = SoftwareFrame::new(64, 48, [0; 4]);
    let config = config();
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
    let mut live = crate::terrain_runtime::native_probe_fixture_terrain(&serde_json::json!({
        "spacing":256, "height_shift":5, "plane_shift":8, "plane_base":0,
        "xz":[192,192], "heights":[8,8,8,8], "alternate":false
    }));
    scene.set_terrain(Some(live.clone()));
    scene.install_terrain_surface_callbacks(0x20, 17);
    let original_height = live.height_at([192, 192]);
    assert_eq!(original_height, 256);
    let mut policy = ProcessTerrainPolicy::supplied(TerrainSpacing::Units256);
    TerrainDetailContext {
        process: &mut policy,
        world_flags: Some(0x20),
        terrain: Some(&mut live),
    }
    .configure(1)
    .unwrap();
    assert_ne!(live.height_at([192, 192]), original_height);
    assert_eq!(
        scene.terrain.as_ref().unwrap().sampling().spacing,
        TerrainSpacing::Units256
    );
    assert_eq!(live.sampling().spacing, TerrainSpacing::Units128);

    let RenderResourceData::Model(original) = scene
        .resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 751,
        })
        .unwrap()
        .data
    else {
        unreachable!()
    };
    let mut model = original.clone();
    model.vertices = vec![inf_formats::ModelVectorI16 {
        x: 192,
        y: -500,
        z: 192,
    }];
    let context = ModelContext::new([0; 3], MenuBasis::INITIAL);
    let mut vertices = ModelVertices::new_invocation(4);
    vertices
        .initialize_world_ranges(&model, &context, [0, 2, 2])
        .unwrap();
    let source = vertices.world(0).unwrap();
    let read = ModelTerrainRead::Live(&live);
    let resolved = read.resolve(scene.terrain.as_ref()).unwrap();
    assert!(std::ptr::eq(resolved, &live));
    let mut callbacks = ModelSurfaceCallbackInputs {
        binding: scene
            .model_surface_binding(ModelSurfaceTableOrigin::InheritedRenderer, Some(resolved)),
        table: None,
    };
    vertices
        .execute_ground_2b(
            ModelSurfaceVertexRequest {
                command: ModelVertexCommand::RendererWord(0x012b),
                destination: 2,
                source: 0,
            },
            &mut callbacks,
            &mut NoProjection,
        )
        .unwrap();
    vertices
        .execute_project_2c(
            ModelSurfaceVertexRequest {
                command: ModelVertexCommand::RendererWord(0x012c),
                destination: 3,
                source: 0,
            },
            &mut callbacks,
            &mut NoProjection,
        )
        .unwrap();
    let expected = [source[0], i32::from(live.height_at([192, 192])), source[2]];
    assert_eq!(vertices.world(2).unwrap(), expected);
    assert_eq!(vertices.world(3).unwrap(), expected);
    assert_ne!(vertices.world(2).unwrap()[1], i32::from(original_height));
    assert_eq!(
        scene.terrain.as_ref().unwrap().height_at([192, 192]),
        original_height
    );
}
