//! Actual process-view projection/copy adapter contracts.
//! Supplied bytes below discriminate exact write/copy widths; they are not
//! captured runtime initializers or substitutes for native process startup.

use std::convert::Infallible;

use super::*;
use crate::{
    frontend_scene::SceneView,
    menu::MenuBasis,
    model_context::{ModelDispatch, ModelProjection, ModelTransform},
    model_projection::ModelProjectionBackend,
    model_scratch::{ModelScratchDrawExit, RegisteredMarkUnwindApproximation},
};

fn instruction(raw: u16, args: &[u16]) -> ModelInstruction {
    let mut words = vec![raw];
    words.extend_from_slice(args);
    words.push(0);
    inf_formats::decode_model_program(&words)
        .unwrap()
        .instructions
        .remove(0)
}

fn projection(dispatch: ModelDispatch) -> ModelProjectionBackend {
    ModelProjectionBackend {
        transform: ModelTransform {
            view_position: [0; 3],
            view_basis: MenuBasis::INITIAL,
        },
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        projection: ModelProjection {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [128, 384],
        },
        dispatch,
        coordinate_shift: 0,
        fog_factor: 0x1_0000,
    }
}

#[test]
fn process_view_world_stores_and_actual_near_far_projection_preserve_unwritten_bytes() {
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scope = scratch.begin_draw().unwrap();
    let allocation = scratch.allocate(&scope, 1, 2).unwrap();
    let initial: [Option<u8>; 24] = std::array::from_fn(|i| Some(0x40 + i as u8));
    scratch.write_vertex(&allocation, 0, 0, initial).unwrap();
    let mut vertices = ModelVertices::in_scratch(&scratch, allocation.clone()).unwrap();
    vertices.write_world(0, [32, -16, 256]).unwrap();
    let world = [32_i32, -16, 256].map(i32::to_le_bytes).concat();
    let after_world = scratch.read_vertex::<24>(&allocation, 0, 0).unwrap();
    assert_eq!(
        &after_world[..12],
        world.iter().copied().map(Some).collect::<Vec<_>>()
    );
    assert_eq!(
        &after_world[12..],
        &initial[12..],
        "three DWORD world stores only"
    );
    vertices
        .project::<Infallible, _>(
            0,
            ModelVertexInput::World([32, -16, 256]),
            &mut projection(ModelDispatch::Far),
        )
        .unwrap();
    let far = scratch.read_vertex::<24>(&allocation, 0, 0).unwrap();
    assert_eq!(&far[..12], &after_world[..12]);
    assert_eq!(vertices.slot(0).unwrap().projection.read_u32(4), Some(256));
    assert_eq!(far[20], Some(0x12));
    assert_eq!(far[21], Some(128));
    assert_eq!(&far[22..24], &initial[22..24]);
    vertices
        .project::<Infallible, _>(
            0,
            ModelVertexInput::World([32, -16, 256]),
            &mut projection(ModelDispatch::Near),
        )
        .unwrap();
    let near = scratch.read_vertex::<24>(&allocation, 0, 0).unwrap();
    assert_eq!(
        near, far,
        "actual Near does not overwrite retained Far fog/padding"
    );
    scratch.retire(&scope, allocation.mark()).unwrap();
    scratch
        .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
    assert_eq!(
        scratch.read_pool::<24>(0).unwrap(),
        near,
        "normal retirement never clears"
    );
}

struct PartialFailure {
    scratch: ProcessModelScratch,
    allocation: ModelScratchAllocation,
}

impl ModelVertexProjector for PartialFailure {
    type Error = &'static str;
    fn project(
        &mut self,
        _: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        // Reacquire the SAME arena inside the callback: a RefCell borrow must
        // not span this call. The returned error is a declared projector fixture.
        self.scratch
            .read_vertex::<24>(&self.allocation, 0, 0)
            .unwrap();
        output.write_u32(0, 0x1234_5678);
        output.bytes[8] = Some(0x31);
        Err("declared projector failure after XY and clip")
    }
}

#[test]
fn process_view_projection_error_keeps_partial_bytes_without_success_eax_store() {
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scope = scratch.begin_draw().unwrap();
    let allocation = scratch.allocate(&scope, 1, 0).unwrap();
    let initial: [Option<u8>; 24] = std::array::from_fn(|i| Some(0x80 + i as u8));
    scratch.write_vertex(&allocation, 0, 0, initial).unwrap();
    let mut vertices = ModelVertices::in_scratch(&scratch, allocation.clone()).unwrap();
    let result = vertices.project::<Infallible, _>(
        0,
        ModelVertexInput::Local([1, 2, 3]),
        &mut PartialFailure {
            scratch: scratch.clone(),
            allocation: allocation.clone(),
        },
    );
    assert!(matches!(
        result,
        Err(ModelVertexError::Projection(
            "declared projector failure after XY and clip"
        ))
    ));
    let after = scratch.read_vertex::<24>(&allocation, 0, 0).unwrap();
    let mut expected = initial;
    expected[12..16].copy_from_slice(&0x1234_5678_u32.to_le_bytes().map(Some));
    expected[20] = Some(0x31);
    assert_eq!(after, expected);
    assert_eq!(vertices.slot(0).unwrap().projection.bytes, expected[12..24]);
    // This is the direct adapter error domain, not a normal model return.
    scratch
        .finish_draw(&scope, ModelScratchDrawExit::StoppedOnError)
        .unwrap();
    scratch
        .recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)
        .unwrap();
    assert_eq!(scratch.read_pool::<24>(0).unwrap(), expected);
}

#[test]
fn process_parent_2a_reads_fresh_full24_bytes_after_clone_including_partial_world() {
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scope = scratch.begin_draw().unwrap();
    let parent_allocation = scratch.allocate(&scope, 2, 2).unwrap();
    let parent_rows: [[Option<u8>; 24]; 2] =
        std::array::from_fn(|slot| std::array::from_fn(|i| Some(0x20 + slot as u8 * 32 + i as u8)));
    for (slot, row) in parent_rows.into_iter().enumerate() {
        scratch
            .write_vertex(&parent_allocation, slot as u16, 0, row)
            .unwrap();
    }
    let parent = ModelVertices::in_scratch(&scratch, parent_allocation.clone()).unwrap();
    let cloned_parent = parent.clone();
    // Explicit supplied prior writes AFTER cloning. The semantic cache remains
    // stale; native 2A must read the real source row, not encode that cache.
    scratch
        .write_vertex(&parent_allocation, 0, 2, [None])
        .unwrap();
    scratch
        .write_vertex(&parent_allocation, 0, 23, [Some(0xee)])
        .unwrap();
    let actual_parent = [0_u16, 1].map(|slot| {
        scratch
            .read_vertex::<24>(&parent_allocation, slot, 0)
            .unwrap()
    });
    let child_allocation = scratch.allocate(&scope, 2, 2).unwrap();
    let mut child = ModelVertices::in_scratch(&scratch, child_allocation.clone()).unwrap();
    child
        .execute_copy_2a(
            &instruction(0x0a2a, &[0, 0]),
            &[0],
            Some(&cloned_parent),
            MirrorCopySource::Partner,
            [0; 3],
            &mut super::tests::RecordingProjection::default(),
        )
        .unwrap();
    for (slot, expected) in actual_parent.into_iter().enumerate() {
        assert_eq!(
            scratch
                .read_vertex::<24>(&child_allocation, slot as u16, 0)
                .unwrap(),
            expected
        );
        assert_eq!(
            child.slot(slot as i16).unwrap().projection.bytes,
            expected[12..24]
        );
    }
    assert!(matches!(
        child.world(0),
        Err(ModelVertexFault::UnknownWorld { .. })
    ));
    assert_eq!(
        scratch.read_vertex::<1>(&child_allocation, 0, 23).unwrap(),
        [Some(0xee)]
    );
    scratch.retire(&scope, child_allocation.mark()).unwrap();
    scratch.retire(&scope, parent_allocation.mark()).unwrap();
    scratch
        .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}
