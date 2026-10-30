use super::*;
use crate::{
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    Installation, ResidencyRequest, ResidentRuntime,
};

fn lighting() -> ModelLighting {
    ModelLighting {
        direction: [0.0, 0.0, 1.0],
        directional_word: 0x00aa_d2ff,
        ambient: [10, 6, 5],
        ambient_word: 0xab05_060a,
    }
}

#[test]
fn normal_lighting_retains_ambient_and_scales_all_channels_together() {
    assert_eq!(
        shade_normal([0, 0, -4096], &MenuBasis::INITIAL, lighting()),
        [10, 6, 5]
    );
    assert_eq!(
        shade_normal([0, 0, 4096], &MenuBasis::INITIAL, lighting()),
        [255, 207, 168]
    );
    let mut light = lighting();
    light.ambient = [-10, -20, -30];
    assert_eq!(shade_normal([0, 0, 0], &MenuBasis::INITIAL, light), [0; 3]);
    light.ambient = [i32::MAX, 0, 0];
    light.directional_word = 0;
    // Native numerator multiplies in 32 bits before signed division.
    assert_eq!(shade_normal([0, 0, 0], &MenuBasis::INITIAL, light), [0; 3]);
}

#[test]
fn transformed_normals_round_before_the_lighting_dot_product() {
    let mut basis = MenuBasis::INITIAL;
    basis.words[9] = 0.5f32.to_bits();
    let light = ModelLighting {
        direction: [0.0, 0.0, 4096.0],
        directional_word: 0x010101,
        ambient: [0; 3],
        ambient_word: 0,
    };
    assert_eq!(shade_normal([0, 0, 1], &basis, light), [0; 3]); // ties-to-even 0.5 ->0
    assert_eq!(shade_normal([0, 0, 3], &basis, light), [2; 3]); // 1.5 ->2 before dot
    basis.words[9] = f32::NAN.to_bits();
    assert_eq!(shade_normal([0, 0, 3], &basis, light), [0; 3]); // QWORD indefinite lowDWORD0
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
fn prefix_normal_and_mirrored_ranges_preserve_partial_color_words() {
    let runtime = resident();
    let resource = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 747,
        })
        .unwrap();
    let RenderResourceData::Model(model) = resource.data else {
        unreachable!()
    };
    let mut model = model.clone();
    model.header.shade_ranges = [0, 4];
    let mut bytes = resource.source.bytes.to_vec();
    let start = model.normals_span.start;
    for (base, vector) in [(start - 6, [4096i16, 0, 0]), (start, [0, 4096, 0])] {
        for (i, value) in vector.into_iter().enumerate() {
            bytes[base + i * 2..base + i * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
    let light = ModelLighting {
        direction: [1.0, 0.0, 0.0],
        directional_word: 0x646464,
        ambient: [0; 3],
        ambient_word: 0xab00_0000,
    };
    let mut shades = ModelShades::new_invocation(4, light.ambient_word).unwrap();
    shades
        .initialize_normals(&model, &bytes, &MenuBasis::INITIAL, light)
        .unwrap();
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(0)).unwrap(),
        ModelShade {
            rgb: Some([100; 3]),
            high_byte: Some(0xab)
        }
    );
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(1)).unwrap(),
        ModelShade {
            rgb: Some([0; 3]),
            high_byte: Some(0xab)
        }
    );
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(2)).unwrap(),
        ModelShade {
            rgb: Some([0; 3]),
            high_byte: None
        }
    );
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(3)).unwrap(),
        ModelShade {
            rgb: Some([0; 3]),
            high_byte: None
        }
    );

    model.header.shade_ranges = [4, 4];
    let mut failed = ModelShades::new_invocation(4, light.ambient_word).unwrap();
    assert_eq!(
        failed.initialize_normals(&model, &bytes[..start], &MenuBasis::INITIAL, light),
        Err(ModelLightingError::Normal(2))
    );
    assert_eq!(
        failed.shade(ModelSlotRef::from_raw(0)).unwrap().rgb,
        Some([100; 3])
    );
    assert_eq!(failed.shade(ModelSlotRef::from_raw(2)).unwrap().rgb, None);
    assert_eq!(
        failed.initialize_normals(&model, &bytes, &MenuBasis::INITIAL, light),
        Err(ModelLightingError::Stopped)
    );
}

#[test]
fn corpus_shade_ranges_leave_unwritten_odd_slots_unknown() {
    let runtime = resident();
    for id in [
        744, 745, 746, 747, 748, 749, 751, 752, 753, 754, 755, 756, 757,
    ] {
        let resource = RenderResources::new(&runtime)
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: id,
            })
            .unwrap();
        let RenderResourceData::Model(model) = resource.data else {
            unreachable!()
        };
        let mut shades =
            ModelShades::new_invocation(model.header.shade_slot_count, lighting().ambient_word)
                .unwrap();
        shades
            .initialize_normals(
                model,
                resource.source.bytes,
                &MenuBasis::INITIAL,
                lighting(),
            )
            .unwrap();
        if model.header.shade_ranges == [0, 0] {
            assert_eq!(
                shades.slots,
                [ModelShade::from_word(lighting().ambient_word); 2]
            );
        } else {
            assert_eq!(shades.slots.len(), 534);
            assert!(shades.slots.iter().step_by(2).all(|s| s.rgb.is_some()));
            assert!(shades
                .slots
                .iter()
                .skip(3)
                .step_by(2)
                .all(|s| *s == ModelShade::default()));
            assert_eq!(shades.slots[0].high_byte, Some(0xab));
            assert_eq!(shades.slots[2].high_byte, None);
        }
    }
}

#[test]
fn modulation_uses_unsigned_record_bytes_and_preserves_unknown_input() {
    assert_eq!(
        ModelShade::from_word(0x7f_80_ff_10).modulated_rgb([128, 255, 64]),
        Some([16, 255, 64])
    );
    assert_eq!(ModelShade::default().modulated_rgb([255; 3]), None);
    assert_eq!(
        ModelShades::new_invocation(1, 0).unwrap_err(),
        ModelLightingError::AmbientSlots(1)
    );
}

#[test]
fn scratch_shades_distinguish_mapped_unknown_words_from_initialized_local_colors() {
    let allocation = ModelScratchAllocation::new(0, 22, 2).unwrap();
    assert_eq!(allocation.shade_base, 528);
    assert_eq!(allocation.end, 536);
    let shades = ModelShades::in_scratch(allocation, 0xab05_060a).unwrap();
    assert_eq!(
        shades.scratch_shade(0).unwrap(),
        ModelShade::from_word(0xab05_060a)
    );
    for reference in [10, 226, 1814, -132] {
        assert_eq!(
            shades.scratch_shade(reference).unwrap(),
            ModelShade::default()
        );
    }
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(10)),
        Err(ModelLightingError::Slot(10))
    );
    assert_eq!(
        shades.scratch_shade(-133),
        Err(ModelLightingError::ScratchRead { slot: -133 })
    );
    let last = ((MODEL_SCRATCH_BYTES - allocation.shade_base) / 4 - 1) as i32;
    assert_eq!(shades.scratch_shade(last).unwrap(), ModelShade::default());
    assert_eq!(
        shades.scratch_shade(last + 1),
        Err(ModelLightingError::ScratchRead { slot: last + 1 })
    );
}

#[test]
fn scratch_allocation_enforces_native_capacity_including_all_four_shade_bytes() {
    let final_allocation = ModelScratchAllocation::new(MODEL_SCRATCH_BYTES - 56, 2, 2).unwrap();
    assert_eq!(final_allocation.end, MODEL_SCRATCH_BYTES);
    let shades = ModelShades::in_scratch(final_allocation, 0).unwrap();
    assert!(shades.scratch_shade(1).is_ok());
    assert_eq!(
        shades.scratch_shade(2),
        Err(ModelLightingError::ScratchRead { slot: 2 })
    );
    assert!(matches!(
        ModelScratchAllocation::new(MODEL_SCRATCH_BYTES - 55, 2, 2),
        Err(ModelLightingError::ScratchAllocation { .. })
    ));
}
