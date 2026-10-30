use super::*;

fn native_pool() -> ProcessModelScratch {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let exe = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    let proof = exe
        .loader_zero_data(NATIVE_POOL, (NATIVE_CURSOR_END - NATIVE_POOL) as usize)
        .unwrap();
    ProcessModelScratch::native_loader_zero(&proof).unwrap()
}

#[test]
fn once_loader_zero_and_explicit_unknown_are_distinct_storage_domains() {
    let pool = native_pool();
    assert_eq!(pool.read_pool::<24>(0).unwrap(), [Some(0); 24]);
    assert_eq!(
        pool.read_pool::<4>(MODEL_SCRATCH_BYTES - 4).unwrap(),
        [Some(0); 4]
    );
    assert_eq!(pool.cursor().unwrap(), 0);
    assert_eq!(
        ProcessModelScratch::isolated_unknown_fixture()
            .read_pool::<24>(0)
            .unwrap(),
        [None; 24]
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    let exe = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    let short = exe
        .loader_zero_data(NATIVE_POOL, MODEL_SCRATCH_BYTES)
        .unwrap();
    assert!(matches!(
        ProcessModelScratch::native_loader_zero(&short),
        Err(ModelScratchError::LoaderZeroProof { .. })
    ));
    assert!(exe.mapped(NATIVE_POOL, 1).is_err());
}

#[test]
fn retired_shades_become_vertex_bytes_and_signed_reads_reach_the_same_pool() {
    let pool = native_pool();
    let first = pool.begin_draw().unwrap();
    let a = pool.allocate(&first, 2, 2).unwrap();
    pool.write_shade(&a, 1, 0, [Some(0x31), Some(0x72), None, Some(0xbc)])
        .unwrap();
    pool.retire(&first, a.mark()).unwrap();
    pool.finish_draw(&first, ModelScratchDrawExit::NormalReturn)
        .unwrap();
    let second = pool.begin_draw().unwrap();
    let b = pool.allocate(&second, 3, 2).unwrap();
    assert_eq!(
        pool.read_vertex::<4>(&b, 2, 4).unwrap(),
        [Some(0x31), Some(0x72), None, Some(0xbc)]
    );
    assert_eq!(
        pool.read_mapped_shade(&b, -5).unwrap(),
        [Some(0x31), Some(0x72), None, Some(0xbc)]
    );
    assert!(matches!(
        pool.read_mapped_shade(&b, i32::MIN),
        Err(ModelScratchError::MappedShade { .. })
    ));
    pool.retire(&second, b.mark()).unwrap();
    pool.finish_draw(&second, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}

#[test]
fn normal_retirement_requires_live_owner_and_lifo_marks_without_clearing() {
    let pool = native_pool();
    let scope = pool.begin_draw().unwrap();
    let parent = pool.allocate(&scope, 1, 2).unwrap();
    let child = pool.allocate(&scope, 2, 2).unwrap();
    pool.write_vertex(&child, 1, 13, [Some(0x47)]).unwrap();
    assert!(matches!(
        pool.retire(&scope, parent.mark()),
        Err(ModelScratchError::AllocationOrder { .. })
    ));
    let other = native_pool();
    assert!(matches!(
        other.allocate(&scope, 1, 2),
        Err(ModelScratchError::ForeignScope)
    ));
    pool.retire(&scope, child.mark()).unwrap();
    assert_eq!(pool.cursor().unwrap(), parent.layout.end);
    assert!(matches!(
        pool.write_vertex(&child, 0, 0, [Some(1)]),
        Err(ModelScratchError::RetiredAllocation(_))
    ));
    pool.retire(&scope, parent.mark()).unwrap();
    pool.finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
    assert_eq!(
        pool.read_pool::<1>(child.layout.start + 24 + 13).unwrap(),
        [Some(0x47)]
    );
}

#[test]
fn stopped_prefix_requires_named_recovery_and_bytes_survive_renderer_handle_reuse() {
    let pool = native_pool();
    let scene_handle = pool.clone();
    let scope = scene_handle.begin_draw().unwrap();
    let parent = scene_handle.allocate(&scope, 1, 2).unwrap();
    let child = scene_handle.allocate(&scope, 3, 2).unwrap();
    scene_handle
        .write_vertex(&child, 2, 21, [Some(0x5c)])
        .unwrap();
    scene_handle
        .finish_draw(&scope, ModelScratchDrawExit::StoppedOnError)
        .unwrap();
    drop(scene_handle);
    assert_eq!(pool.cursor().unwrap(), child.layout.end);
    assert!(matches!(
        pool.begin_draw(),
        Err(ModelScratchError::DrawState { .. })
    ));
    assert!(matches!(
        pool.write_vertex(&parent, 0, 0, [Some(0)]),
        Err(ModelScratchError::DrawState { .. })
    ));
    pool.recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)
        .unwrap();
    assert_eq!(pool.cursor().unwrap(), 0);
    let recreated = pool.clone();
    assert_eq!(
        recreated
            .read_pool::<1>(child.layout.start + 48 + 21)
            .unwrap(),
        [Some(0x5c)]
    );
    assert_eq!(recreated.state().unwrap(), ModelScratchDrawState::Ready);
}

#[test]
fn exact_capacity_is_admitted_and_excess_does_not_register_or_advance() {
    let pool = ProcessModelScratch::isolated_unknown_fixture();
    let scope = pool.begin_draw().unwrap();
    let a = pool.allocate(&scope, 32767, 59398).unwrap();
    assert_eq!(a.layout.end, MODEL_SCRATCH_BYTES);
    assert!(
        matches!(pool.allocate(&scope, 1, 0), Err(ModelScratchError::Capacity { end }) if end == MODEL_SCRATCH_BYTES + 24)
    );
    assert_eq!(pool.cursor().unwrap(), MODEL_SCRATCH_BYTES);
    pool.retire(&scope, a.mark()).unwrap();
    pool.finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}

#[test]
fn forged_layout_and_cross_field_writes_fail_before_mutation() {
    let pool = native_pool();
    let scope = pool.begin_draw().unwrap();
    let a = pool.allocate(&scope, 1, 2).unwrap();
    let mut forged = a.clone();
    forged.layout.start = 4;
    assert!(matches!(
        pool.write_vertex(&forged, 0, 0, [Some(7)]),
        Err(ModelScratchError::LayoutMismatch)
    ));
    assert!(matches!(
        pool.write_vertex(&a, 0, 23, [Some(1), Some(2)]),
        Err(ModelScratchError::SlotRange { .. })
    ));
    assert_eq!(pool.read_pool::<24>(0).unwrap(), [Some(0); 24]);
}

// Existing crate sha2 dependency and strict FrontendExecutable parser are reused.
#[test]
fn native_model_scratch_allocation_cleanup_and_once_loader_storage_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(&raw).unwrap();
    // Fixed supported-build header guard, not another runtime PE parser.
    let data_header: usize = 0x230;
    assert_eq!(&raw[data_header..data_header + 8], b".data\0\0\0");
    let dword = |offset: usize| u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap());
    let fields = [0, 4, 8, 12].map(|offset| dword(data_header + 8 + offset));
    // VirtualSize, RVA, SizeOfRawData, PointerToRawData.
    assert_eq!(fields, [0x132c38, 0x12e000, 0x19000, 0x12e000]);
    let virtual_start = 0x0040_0000_u32 + fields[1];
    let raw_end = virtual_start + fields[2];
    let virtual_end = virtual_start + fields[0];
    assert_eq!(raw_end, 0x0054_7000);
    assert_eq!(virtual_end, 0x0066_0c38);
    assert!(raw_end <= 0x0056_4728);
    assert!(0x0065_e72c <= virtual_end);
    assert_eq!(0x0065_e728_u32 - 0x0056_4728, 0x000f_a000);
    // This proves strict fresh-process loader storage; allocation/rewind never reseeds it.
    for (address, length, expected) in [
        // Complete scratch_allocate; end-exclusive 004EBEDF.
        (
            0x004EBE60_usize,
            127_usize,
            "b1421130aff125ac2808500e7bc368e72819da3e201290edcda3729f91048731",
        ),
        // Complete scratch_retire; end-exclusive 004EBEF7.
        (
            0x004EBEE0_usize,
            23_usize,
            "4af09e63c3a81a6f810c7664fb4ee24d0fa04bf55be8c11805f2864e9a7f15c5",
        ),
        // Complete cleanup_marker; end-exclusive 004948B4.
        (
            0x004948A0_usize,
            20_usize,
            "e9ac37e3eff0afc5ac5d3b3b352e2d28898ab55e3be584db1ea7f87eb89b9963",
        ),
        // Complete cleanup_register; end-exclusive 0049490D.
        (
            0x004948C0_usize,
            77_usize,
            "1d87cc835aa271b31efd20173f7a41e4d41c23eb51f63605125de1845addbce8",
        ),
        // Complete cleanup_unregister; end-exclusive 0049498C.
        (
            0x00494910_usize,
            124_usize,
            "5dfbf0b69117d2256d28bf380ade5fd761fb53a2a279fcb627c5b29ee918b9cf",
        ),
        // Complete managed_nonlocal_error; end-exclusive 004949F6.
        (
            0x004949A0_usize,
            86_usize,
            "22f35b1b090d1544fc692e8e12c6dee10fd32cbcf93896a267046054c7660d7d",
        ),
        // Complete normal_cleanup_frame_pop; end-exclusive 00494A48.
        (
            0x00494A00_usize,
            72_usize,
            "617a71bfe05778a31d8ccb9d3168f1f38e01f09f59efdd848a98fc1b1983c6d2",
        ),
    ] {
        // Strict .text profile has identical VA-minus-image-base and file offset.
        let start = address - 0x0040_0000;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[start..start + length])),
            expected
        );
    }
    // Reached phase byte guards retain order and write widths within those full bodies.
    for (start, expected) in [
        // marker_before_allocate
        (0xebe68_usize, &[0xe8, 0x33, 0x8a, 0xfa, 0xff][..]),
        // capacity_error_before_own_registration
        (
            0xebe81_usize,
            &[
                0x81, 0xfa, 0x00, 0xa0, 0x0f, 0x00, 0x7e, 0x10, 0xb9, 0xd8, 0x91, 0x52, 0x00, 0xe8,
                0x0d, 0x8b, 0xfa, 0xff,
            ][..],
        ),
        // publish_vertex_and_cursor
        (
            0xebea2_usize,
            &[
                0x89, 0x03, 0x8b, 0x15, 0x28, 0xe7, 0x65, 0x00, 0x8d, 0x04, 0xca, 0xa3, 0x28, 0xe7,
                0x65, 0x00,
            ][..],
        ),
        // publish_shades_then_register_old_mark
        (
            0xebeb8_usize,
            &[
                0x89, 0x43, 0x04, 0x8b, 0x0d, 0x28, 0xe7, 0x65, 0x00, 0x8d, 0x14, 0xa9, 0xb9, 0xe0,
                0xbe, 0x4e, 0x00, 0x89, 0x15, 0x28, 0xe7, 0x65, 0x00, 0x8b, 0xd7, 0xe8, 0xea, 0x89,
                0xfa, 0xff,
            ][..],
        ),
        // register_three_dwords
        (
            0x948f4_usize,
            &[
                0xc7, 0x06, 0x02, 0x00, 0x00, 0x00, 0xa1, 0xe8, 0xf4, 0x55, 0x00, 0x5e, 0x89, 0x48,
                0x04, 0x8b, 0x0d, 0xe8, 0xf4, 0x55, 0x00, 0x89, 0x51, 0x08, 0xc3,
            ][..],
        ),
        // unregister_tombstones
        (
            0x9496a_usize,
            &[0x89, 0x08, 0x89, 0x48, 0x04, 0x89, 0x48, 0x08][..],
        ),
        // managed_advance_before_callback_then_fresh_pointer
        (
            0x949dd_usize,
            &[
                0x8b, 0xd0, 0x83, 0xc0, 0x0c, 0xa3, 0xe8, 0xf4, 0x55, 0x00, 0x8b, 0x4a, 0x08, 0xff,
                0x52, 0x04, 0xa1, 0xe8, 0xf4, 0x55, 0x00,
            ][..],
        ),
        // frame_pop_seventeen_dwords
        (
            0x94a23_usize,
            &[
                0xb9, 0x11, 0x00, 0x00, 0x00, 0xb8, 0x03, 0x00, 0x00, 0x00, 0xf3, 0xab,
            ][..],
        ),
    ] {
        assert_eq!(&raw[start..start + expected.len()], expected);
    }
}

#[test]
fn empty_vertex_view_still_validates_live_owner_and_layout() {
    use crate::model_vertices::{ModelVertexFault, ModelVertices};
    let pool = native_pool();
    let scope = pool.begin_draw().unwrap();
    let a = pool.allocate(&scope, 0, 2).unwrap();
    assert!(ModelVertices::in_scratch(&pool, a.clone())
        .unwrap()
        .slots()
        .is_empty());
    let foreign = native_pool();
    assert!(matches!(
        ModelVertices::in_scratch(&foreign, a.clone()),
        Err(ModelVertexFault::Scratch(
            ModelScratchError::ForeignAllocation
        ))
    ));
    let mut forged = a.clone();
    forged.layout.start = 4;
    assert!(matches!(
        ModelVertices::in_scratch(&pool, forged),
        Err(ModelVertexFault::Scratch(ModelScratchError::LayoutMismatch))
    ));
    pool.retire(&scope, a.mark()).unwrap();
    assert!(matches!(
        ModelVertices::in_scratch(&pool, a),
        Err(ModelVertexFault::Scratch(
            ModelScratchError::RetiredAllocation(_)
        ))
    ));
}

/// The view shown's keeping: a draw between a save and its restore leaves
/// no residue, though it allocates further than any draw before it; the
/// cursor and the residue the draw before left come back.
#[test]
fn a_restored_save_puts_back_every_byte_the_draws_since_allocated_over() {
    let pool = native_pool();
    let first = pool.begin_draw().unwrap();
    let a = pool.allocate(&first, 2, 2).unwrap();
    pool.write_shade(&a, 1, 0, [Some(0x31), Some(0x72), None, Some(0xbc)])
        .unwrap();
    pool.retire(&first, a.mark()).unwrap();
    pool.finish_draw(&first, ModelScratchDrawExit::NormalReturn)
        .unwrap();
    let pool_bytes = || -> Vec<_> {
        (0..96)
            .map(|at| pool.read_pool::<1>(at).unwrap()[0])
            .collect()
    };
    let before = pool_bytes();
    let save = pool.save().unwrap();
    assert_eq!(pool.save().err(), Some(ModelScratchError::SaveOpen));
    let band = pool.begin_draw().unwrap();
    let b = pool.allocate(&band, 1, 0).unwrap();
    let c = pool.allocate(&band, 2, 4).unwrap();
    pool.write_vertex(&b, 0, 0, [Some(9); 24]).unwrap();
    pool.write_shade(&c, 3, 0, [Some(7); 4]).unwrap();
    pool.retire(&band, c.mark()).unwrap();
    pool.retire(&band, b.mark()).unwrap();
    pool.finish_draw(&band, ModelScratchDrawExit::NormalReturn)
        .unwrap();
    assert_ne!(pool_bytes(), before);
    pool.restore(save).unwrap();
    assert_eq!(pool_bytes(), before);
    assert_eq!(pool.cursor().unwrap(), 0);
    assert_eq!(pool.state().unwrap(), ModelScratchDrawState::Ready);
    // A failed restore closes the save, so the next save opens.
    let _open = pool.save().unwrap();
    assert_eq!(
        pool.restore(native_pool().save().unwrap()).err(),
        Some(ModelScratchError::ForeignScope)
    );
    let save = pool.save().unwrap();
    pool.restore(save).unwrap();
    let next = pool.begin_draw().unwrap();
    let d = pool.allocate(&next, 3, 2).unwrap();
    assert_eq!(
        pool.read_vertex::<4>(&d, 2, 4).unwrap(),
        [Some(0x31), Some(0x72), None, Some(0xbc)]
    );
}
