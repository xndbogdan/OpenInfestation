//! Supplied intrinsic fixtures on the actual shared byte owner. These tests
//! establish shade width/history/custody, not native frame timing or pixels.

use super::*;
use crate::{
    model_scratch::{ModelScratchDrawExit, ModelScratchDrawState},
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    Installation, ResidencyRequest, ResidentRuntime,
};

fn light() -> ModelLighting {
    ModelLighting {
        direction: [0.0, 0.0, 1.0],
        directional_word: 0x00aa_d2ff,
        ambient: [10, 6, 5],
        ambient_word: 0xc105_060a,
    }
}

fn resident() -> ResidentRuntime {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
}

#[test]
fn process_common_shade_seed_replaces_only_ambient_words_and_keeps_real_prior_bytes() {
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let first_scope = scratch.begin_draw().unwrap();
    let previous = scratch.allocate(&first_scope, 0, 4).unwrap();
    for slot in 0..4u16 {
        let word = 0xb400_0000 | (0x0043_2165 + u32::from(slot));
        scratch
            .write_shade(&previous, slot, 0, word.to_le_bytes().map(Some))
            .unwrap();
    }
    let previous_bytes = scratch.read_pool::<16>(previous.layout.shade_base).unwrap();
    scratch.retire(&first_scope, previous.mark()).unwrap();
    scratch
        .finish_draw(&first_scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();

    let scope = scratch.begin_draw().unwrap();
    let allocation = scratch.allocate(&scope, 0, 4).unwrap();
    assert_eq!(allocation.layout.shade_base, previous.layout.shade_base);
    let shades =
        ModelShades::in_process_scratch(&scratch, allocation.clone(), light().ambient_word)
            .unwrap();
    let ambient = light().ambient_word.to_le_bytes().map(Some);
    let mut expected = previous_bytes;
    expected[..4].copy_from_slice(&ambient);
    expected[4..8].copy_from_slice(&ambient);
    assert_eq!(
        scratch
            .read_pool::<16>(allocation.layout.shade_base)
            .unwrap(),
        expected
    );
    for slot in 0..2u16 {
        assert_eq!(
            shades.shade(ModelSlotRef::from_raw(slot)).unwrap(),
            ModelShade::from_word(light().ambient_word)
        );
    }
    for slot in 2..4u16 {
        let offset = usize::from(slot) * 4;
        assert_eq!(
            shades
                .shade(ModelSlotRef::from_raw(slot))
                .unwrap()
                .high_byte,
            previous_bytes[offset + 3]
        );
        assert_eq!(
            scratch.read_shade::<4>(&allocation, slot, 0).unwrap(),
            previous_bytes[offset..offset + 4]
        );
    }
    scratch.retire(&scope, allocation.mark()).unwrap();
    scratch
        .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}

#[test]
fn process_normal_rgb_store_keeps_high_byte_and_prior_rows_on_later_source_error() {
    let runtime = resident();
    let resource = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 747,
        })
        .unwrap();
    let RenderResourceData::Model(original) = resource.data else {
        panic!("model resource")
    };
    let mut model = original.clone();
    // Supplied normal ranges consume slot2 first, then fail at slot4. They do
    // not alter canonical source/resource bytes or claim original747 cadence.
    model.header.reserved_1a = 2;
    model.header.shade_ranges = [6, 6];
    let start = model.normals_span.start;
    let mut source = resource.source.bytes.to_vec();
    for (axis, value) in [0i16, 0, -4096].into_iter().enumerate() {
        source[start + axis * 2..start + axis * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }

    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scope = scratch.begin_draw().unwrap();
    let allocation = scratch.allocate(&scope, 0, 6).unwrap();
    for slot in 0..6u16 {
        let word = 0xb400_0000 | (0x0043_2165 + u32::from(slot));
        scratch
            .write_shade(&allocation, slot, 0, word.to_le_bytes().map(Some))
            .unwrap();
    }
    let mut shades =
        ModelShades::in_process_scratch(&scratch, allocation.clone(), light().ambient_word)
            .unwrap();
    let before = scratch
        .read_pool::<24>(allocation.layout.shade_base)
        .unwrap();
    assert_eq!(
        shades.initialize_normals(&model, &source[..start + 6], &MenuBasis::INITIAL, light()),
        Err(ModelLightingError::Normal(4))
    );
    assert!(shades.stopped());
    let mut expected = before;
    expected[8..11].copy_from_slice(&[Some(10), Some(6), Some(5)]);
    assert_eq!(
        scratch
            .read_pool::<24>(allocation.layout.shade_base)
            .unwrap(),
        expected
    );
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(2)).unwrap(),
        ModelShade {
            rgb: Some([10, 6, 5]),
            high_byte: before[11],
        }
    );
    assert_eq!(
        scratch.read_shade::<4>(&allocation, 4, 0).unwrap(),
        before[16..20]
    );
    scratch
        .finish_draw(&scope, ModelScratchDrawExit::StoppedOnError)
        .unwrap();
    assert!(matches!(
        scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
    assert_eq!(
        shades.initialize_normals(&model, &source, &MenuBasis::INITIAL, light()),
        Err(ModelLightingError::Stopped)
    );
    assert_eq!(
        scratch
            .read_pool::<24>(allocation.layout.shade_base)
            .unwrap(),
        expected
    );
}

#[test]
fn process_signed_shade_reads_vertex_bytes_and_out_of_local_retained_history() {
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let old_scope = scratch.begin_draw().unwrap();
    let old = scratch.allocate(&old_scope, 0, 12).unwrap();
    let prior_word = 0x9241_6283u32;
    scratch
        .write_shade(&old, 10, 0, prior_word.to_le_bytes().map(Some))
        .unwrap();
    scratch.retire(&old_scope, old.mark()).unwrap();
    scratch
        .finish_draw(&old_scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();

    let scope = scratch.begin_draw().unwrap();
    let allocation = scratch.allocate(&scope, 1, 4).unwrap();
    let vertex: [u8; 24] = std::array::from_fn(|i| 0x31u8.wrapping_add(i as u8));
    scratch
        .write_vertex(&allocation, 0, 0, vertex.map(Some))
        .unwrap();
    let shades =
        ModelShades::in_process_scratch(&scratch, allocation.clone(), light().ambient_word)
            .unwrap();
    assert_eq!(allocation.layout.shade_base, 24);
    for (reference, offset) in [(-6, 0usize), (-1, 20)] {
        let bytes: [u8; 4] = vertex[offset..offset + 4].try_into().unwrap();
        assert_eq!(
            shades.scratch_shade(reference).unwrap(),
            ModelShade::from_word(u32::from_le_bytes(bytes))
        );
    }
    // Reference4 is outside the local four shade slots, at actual byte40. It
    // consumes previous allocation bytes, not an unknown local-cache default.
    assert_eq!(
        shades.scratch_shade(4).unwrap(),
        ModelShade::from_word(prior_word)
    );
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(4)),
        Err(ModelLightingError::Slot(4))
    );
    // Below the pool: 005645E0..00564728, zero in a Direct3D run, then
    // nothing mapped.
    for reference in [-7, -88] {
        assert_eq!(
            shades.scratch_shade(reference).unwrap(),
            ModelShade::from_word(0)
        );
    }
    assert!(matches!(
        shades.scratch_shade(-89),
        Err(ModelLightingError::Scratch(
            ModelScratchError::MappedShade {
                slot: -89,
                offset: -332
            }
        ))
    ));
    scratch.retire(&scope, allocation.mark()).unwrap();
    scratch
        .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}
