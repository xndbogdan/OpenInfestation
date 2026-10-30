//! Actual custom-row pool access and typed custody through model wrappers.
// A deliberately retired actual view discriminates typed custody propagation;
// it is not a claim that native furniture intentionally reads retired storage.
#[test]
fn actual_furniture_custom_row_scratch_fault_remains_renderer_custody() {
    use crate::model_scratch::{ModelScratchDrawExit, ModelScratchError, ProcessModelScratch};
    use crate::model_vertices::{
        compute_furniture_custom_vertex, FurnitureCustomTrail, FurnitureCustomVertexHost,
        ModelCustomVertexError, ModelCustomVertexRequest, ModelVertexError, ModelVertexFault,
        ModelVertices,
    };

    struct UnreachedServices;
    impl FurnitureCustomVertexHost for UnreachedServices {
        type Error = crate::model_commands::ModelCommandError<
            crate::software_scene::SoftwareSceneServiceError,
        >;
        fn load_factor(&mut self) -> Result<i32, Self::Error> {
            panic!("row custody must fail before load factor");
        }
        fn random(&mut self) -> Result<u32, Self::Error> {
            panic!("row custody must fail before RNG");
        }
        fn birth_trail(&mut self, _: FurnitureCustomTrail) -> Result<(), Self::Error> {
            panic!("row custody must fail before birth");
        }
    }

    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scope = scratch.begin_draw().unwrap();
    let allocation = scratch.allocate(&scope, 1, 0).unwrap();
    let raw = [12_i32, -34, 56].map(i32::to_le_bytes).concat();
    let bytes: [Option<u8>; 12] = std::array::from_fn(|i| Some(raw[i]));
    scratch.write_vertex(&allocation, 0, 0, bytes).unwrap();
    let mut vertices = ModelVertices::in_scratch(&scratch, allocation.clone()).unwrap();
    let pool_before = scratch.read_pool::<24>(0).unwrap();
    scratch.retire(&scope, allocation.mark()).unwrap();

    let mut output = Some([91, 92, 93]);
    let error = compute_furniture_custom_vertex(
        ModelCustomVertexRequest {
            selector: 7,
            extras: &[0, 1],
        },
        &mut vertices,
        &mut output,
        &mut UnreachedServices,
    )
    .unwrap_err();
    assert!(matches!(
        &error,
        ModelCustomVertexError::Fault(ModelVertexFault::Scratch(
            ModelScratchError::RetiredAllocation(_)
        ))
    ));
    // Same wrapper chain used by ModelExecutor's common custom2D branch.
    let scene_error = crate::software_scene::SoftwareSceneError::Model(
        crate::model_execution::ModelExecutionError::Vertex(ModelVertexError::Custom(error)),
    );
    assert!(scene_error.is_model_scratch_fault());
    assert!(scene_error.is_renderer_custody_fault());
    assert_eq!(output, Some([91, 92, 93]));
    assert_eq!(scratch.cursor().unwrap(), 0);
    assert_eq!(scratch.read_pool::<24>(0).unwrap(), pool_before);
    scratch
        .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}
