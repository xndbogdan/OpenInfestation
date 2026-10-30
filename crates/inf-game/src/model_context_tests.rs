use std::{path::Path, sync::Arc};

use inf_formats::{ModelFlags, ModelInstructionKind};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    model_values::{ModelValueCallback, ModelValues},
    resources::HandlerResources,
    Installation, ResidencyRequest, ResidentRuntime,
};

fn basis(counter: u32, axes: [[f32; 3]; 3]) -> MenuBasis {
    let mut basis = MenuBasis::INITIAL;
    basis.words[0] = counter;
    for (axis, values) in axes.into_iter().enumerate() {
        for (component, value) in values.into_iter().enumerate() {
            basis.words[1 + axis * 3 + component] = value.to_bits();
        }
    }
    basis
}

fn identity_view() -> SceneView {
    SceneView {
        eye: [0; 3],
        basis: MenuBasis::INITIAL,
    }
}

#[test]
fn saved_world_point_basis_uses_full_wrapping_deltas_and_replaces_counter() {
    let original = basis(91, [[2.0, 3.0, 5.0], [7.0, 11.0, 13.0], [17.0, 19.0, 23.0]]);
    let mut context = ModelContext::new([1, 2, 3], original);
    context.saved_basis.words[0] = 99;
    // The wrapped full-DWORD delta is +65536, whose LOWWORD is zero. This
    // distinguishes04's 004A0940 from09's LOWWORD417DA0 normalization.
    context.save_basis_from_world_points(
        [i32::MAX, 0, 0],
        [i32::MAX, 0, 65536],
        [i32::MIN + 65535, 65536, 0],
    );
    assert_eq!(context.saved_basis.words[0], 0);
    assert_eq!(context.saved_basis.words[7..10], [0, 0, 1.0f32.to_bits()]);
    assert_eq!(context.position, [1, 2, 3]);
    assert_eq!(context.basis, original);
    assert!(f32::from_bits(context.saved_basis.words[4]) > 0.5);
    assert!(f32::from_bits(context.saved_basis.words[5]) > 0.5);
}

#[test]
fn world_difference_normalization_spills_y_z_and_denominator_but_retains_x() {
    assert_eq!(normalize_world_difference([0; 3]), [1.0, 0.0, 0.0]);
    let [x, y, z] = [16_777_219i32, -33_554_439, 67_108_877];
    let denominator = (((f64::from(z) * f64::from(z as f32) + f64::from(y as f32).powi(2))
        + f64::from(x).powi(2))
    .sqrt() as f32) as f64;
    let expected = [
        (f64::from(x) / denominator) as f32,
        (f64::from(y as f32) / denominator) as f32,
        (f64::from(z as f32) / denominator) as f32,
    ];
    assert_eq!(
        normalize_world_difference([x, y, z]).map(f32::to_bits),
        expected.map(f32::to_bits)
    );
    assert_ne!(
        expected[0].to_bits(),
        ((f64::from(x as f32) / denominator) as f32).to_bits()
    );

    // FST at004A0993/004A0A2F leaves raw Z retained for the immediately
    // following FMUL. Squaring spilled Z changes the spilled denominator
    // for this independently supplied full-DWORD vector.
    let [x, y, z] = [-381_995_530i32, -252_267_299, 1_855_664_280];
    let spilled_y = f64::from(y as f32);
    let spilled_z = f64::from(z as f32);
    let denominator = (((f64::from(z) * spilled_z + spilled_y * spilled_y)
        + f64::from(x) * f64::from(x))
    .sqrt() as f32) as f64;
    let wrong_denominator = (((spilled_z * spilled_z + spilled_y * spilled_y)
        + f64::from(x) * f64::from(x))
    .sqrt() as f32) as f64;
    assert_eq!(denominator, 1_911_295_232.0);
    assert_eq!(wrong_denominator, 1_911_295_104.0);
    let expected = [
        (f64::from(x) / denominator) as f32,
        (spilled_y / denominator) as f32,
        (spilled_z / denominator) as f32,
    ];
    assert_eq!(
        normalize_world_difference([x, y, z]).map(f32::to_bits),
        expected.map(f32::to_bits)
    );
    assert_ne!(
        expected[2].to_bits(),
        ((spilled_z / wrong_denominator) as f32).to_bits()
    );
}

#[test]
fn opcode04_complete_original_body_guard_is_stable() {
    use sha2::{Digest, Sha256};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let original = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&original)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(&original).unwrap();
    // Native code is read from its authenticated raw file span, never mapped
    // through the data-only executable view.
    assert_eq!(
        format!("{:x}", Sha256::digest(&original[0xa0940..0xa0ab3])),
        "aa99824c799de2c7200dd93838c27503a509dfde0e056dd1f1cdc302ddd2ce78"
    );
}

fn projection() -> ModelProjection {
    ModelProjection {
        focal: [100, 100],
        viewport: [200, 100],
        center: [100, 50],
        fade: [1000, 2000],
    }
}

fn header(extent: u32, flags: u8) -> ModelHeader {
    ModelHeader {
        visibility_extent: extent,
        command_word_count: 1,
        auxiliary_count: 0,
        flags: ModelFlags::from_raw(flags),
        vertex_slot_count: 0,
        raw_vertex_count: 0,
        shade_slot_count: 2,
        raw_0e: 0,
        reserved_10: 0,
        vertex_ranges: [0; 4],
        reserved_1a: 0,
        shade_ranges: [0; 2],
    }
}

fn transform(position: [i32; 3]) -> ModelTransform {
    ModelTransform {
        view_position: position,
        view_basis: MenuBasis::INITIAL,
    }
}

#[test]
fn identity_save_keeps_saved_counter_and_restore_copies_all_ten_words() {
    let original = basis(91, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
    let mut context = ModelContext::new([10, 20, 30], original);
    context.saved_basis = basis(17, [[9.0; 3], [8.0; 3], [7.0; 3]]);
    context.save_identity_basis();
    let mut expected = MenuBasis::INITIAL;
    expected.words[0] = 17;
    assert_eq!(*context.saved_basis(), expected);
    assert_eq!(context.basis, original);
    context.restore_saved_basis();
    assert_eq!(context.basis, expected);
    assert_eq!(context.position, [10, 20, 30]);
}

#[test]
fn saved_orientation_writes_nine_words_and_keeps_counter() {
    let source = basis(91, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
    let mut context = ModelContext::new([10, 20, 30], source);
    assert_eq!(*context.saved_basis(), MenuBasis::INITIAL);
    context.saved_basis.words[0] = 17;
    for (orientation, expected) in [
        (0, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]),
        (1, [[7.0, 8.0, 9.0], [1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]),
        (2, [[4.0, 5.0, 6.0], [7.0, 8.0, 9.0], [1.0, 2.0, 3.0]]),
        (3, [[7.0, 8.0, 9.0], [4.0, 5.0, 6.0], [1.0, 2.0, 3.0]]),
        (4, [[4.0, 5.0, 6.0], [1.0, 2.0, 3.0], [7.0, 8.0, 9.0]]),
        (5, [[1.0, 2.0, 3.0], [7.0, 8.0, 9.0], [4.0, 5.0, 6.0]]),
        (6, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]),
        (7, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]),
        (
            0x421,
            [[7.0, 8.0, 9.0], [-1.0, -2.0, -3.0], [4.0, 5.0, 6.0]],
        ),
        (0x12, [[-4.0, -5.0, -6.0], [7.0, 8.0, 9.0], [1.0, 2.0, 3.0]]),
        (0x0b, [[-7.0, -8.0, -9.0], [4.0, 5.0, 6.0], [1.0, 2.0, 3.0]]),
    ] {
        context.save_oriented_basis(orientation);
        assert_eq!(
            *context.saved_basis(),
            basis(17, expected),
            "orientation {orientation:#x}"
        );
        assert_eq!(context.basis, source);
    }
}

#[test]
fn child_uses_world_slot_origin_and_has_its_own_initialized_saved_basis() {
    let source = basis(91, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
    let mut parent = ModelContext::new([1000, 2000, 3000], source);
    parent.save_oriented_basis(0x21);
    parent.saved_basis.words[0] = 42;
    let child = parent.child([-17, 20, 33], 0x43e);
    assert_eq!(child.position, [-17, 20, 33]);
    assert_eq!(child.basis, *parent.saved_basis()); // sign/high bits ignored
    assert_eq!(*child.saved_basis(), MenuBasis::INITIAL);
    let ordinary_child = parent.child([7, 8, 9], 0);
    assert_eq!(ordinary_child.basis.words[0], 0);
    assert_eq!(ordinary_child.basis.words[1..], source.words[1..]);
    assert_eq!(parent.position, [1000, 2000, 3000]);
}

#[test]
fn publication_composes_axes_in_native_order_without_mutating_world_state() {
    let context = ModelContext::new(
        [3, 5, 7],
        basis(7, [[2.0, 3.0, 5.0], [7.0, 11.0, 13.0], [17.0, 19.0, 23.0]]),
    );
    let view = SceneView {
        eye: [1, 2, 4],
        basis: basis(
            99,
            [[29.0, 31.0, 37.0], [41.0, 43.0, 47.0], [53.0, 59.0, 61.0]],
        ),
    };
    let published = context.publish(view, 0);
    assert_eq!(published.view_position, [262, 352, 466]);
    assert_eq!(
        published.view_basis,
        basis(
            8,
            [
                [336.0, 446.0, 588.0],
                [1025.0, 1371.0, 1813.0],
                [1933.0, 2595.0, 3425.0]
            ]
        )
    );
    assert_eq!(context.publish(view, 0), published); // duplicate child publication
    assert_eq!(context.basis.words[0], 7);
}

#[test]
fn publication_retains_f64_products_and_uses_nearest_even_low_dword_translation() {
    let all_ones = basis(0, [[1.0; 3]; 3]);
    let context = ModelContext::new(
        [33_554_432, 1, -33_554_432],
        basis(0, [[33_554_432.0, 1.0, -33_554_432.0], [0.0; 3], [0.0; 3]]),
    );
    let view = SceneView {
        eye: [0; 3],
        basis: all_ones,
    };
    let published = context.publish(view, 0);
    assert_eq!(published.view_position, [1; 3]);
    assert_eq!(&published.view_basis.words[1..4], &[1.0f32.to_bits(); 3]);

    let half_view = SceneView {
        eye: [0; 3],
        basis: basis(0, [[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]]),
    };
    assert_eq!(
        ModelContext::new([1, 3, 5], MenuBasis::INITIAL)
            .publish(half_view, 0)
            .view_position,
        [0, 2, 2]
    );
    assert_eq!(fistp_low32(4_294_967_299.0), 3);
    assert_eq!(fistp_low32(-4_294_967_299.0), -3);
    assert_eq!(fistp_low32(f64::NAN), 0);
    assert_eq!(fistp_low32(f64::INFINITY), 0);
    assert_eq!(fistp_low32(9_223_372_036_854_775_808.0), 0);
}

#[test]
fn coordinate_shift_sign_extends_wrapping_delta_and_masks_shift_count() {
    let context = ModelContext::new([0x0080_0000, 0x00ff_ffff, i32::MIN], MenuBasis::INITIAL);
    assert_eq!(
        context.publish(identity_view(), 8).view_position,
        [-0x0080_0000, -1, 0]
    );
    assert_eq!(
        context.publish(identity_view(), 40),
        context.publish(identity_view(), 8)
    );
    let view = SceneView {
        eye: [1, 0, 0],
        ..identity_view()
    };
    assert_eq!(
        ModelContext::new([i32::MIN, 0, 0], MenuBasis::INITIAL)
            .publish(view, 0)
            .view_position[0],
        i32::MAX
    );
}

#[test]
fn published_counter_controls_renormalization_without_modifying_source() {
    for (counter, expected_counter, normalized) in
        [(99, 100, false), (100, 0, true), (u32::MAX, 0, false)]
    {
        let context = ModelContext::new(
            [0; 3],
            basis(counter, [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]]),
        );
        let published = context.publish(identity_view(), 0);
        assert_eq!(published.view_basis.words[0], expected_counter);
        if normalized {
            assert_eq!(published.view_basis, MenuBasis::INITIAL);
        } else {
            assert_eq!(published.view_basis.words[1..], context.basis.words[1..]);
        }
        assert_eq!(context.basis.words[0], counter);
    }
}

#[test]
fn frustum_edges_are_asymmetric_and_force_near_does_not_bypass_them() {
    let projection = projection();
    for (position, visible) in [
        ([-100, 0, 100], true),
        ([-101, 0, 100], false),
        ([99, 0, 100], true),
        ([100, 0, 100], false),
        ([0, 50, 100], true),
        ([0, 51, 100], false),
        ([0, -49, 100], true),
        ([0, -50, 100], false),
        ([0, 0, 63], false),
        ([0, 0, 64], true),
    ] {
        let model = transform(position);
        assert_eq!(model.in_frustum(0, &projection), visible, "{position:?}");
        assert_eq!(
            model.admission(header(0, 0x20), &projection).is_some(),
            visible
        );
    }
    assert!(!transform([0, 0, i32::MAX]).in_frustum(1, &projection));
    assert!(!transform([0, 0, 100]).in_frustum(u32::MAX - 36, &projection)); // signed -37
}

#[test]
fn frustum_x_and_y_radius_have_distinct_f32_store_boundaries() {
    let projection = ModelProjection {
        focal: [3; 2],
        center: [0; 2],
        ..projection()
    };
    let extent = 16_777_217;
    let z = 64 - extent as i32;
    // X uses FILD radius in its product before the FST rounded copy is used
    // by Y. Radii become50331652 versus50331648, respectively.
    assert!(transform([-16_777_217, 0, z]).in_frustum(extent, &projection));
    assert!(!transform([0, 16_777_217, z]).in_frustum(extent, &projection));
}

#[test]
fn table_selection_uses_wrapping_signed_extent_and_force_near_policy() {
    let projection = projection();
    for (z, flags, expected) in [
        (899, 0, Some(ModelDispatch::Near)),
        (900, 0, Some(ModelDispatch::Far)),
        (2099, 0, Some(ModelDispatch::Far)),
        (2100, 0, None),
        (2100, 0x20, Some(ModelDispatch::Near)),
    ] {
        assert_eq!(
            transform([0, 0, z]).dispatch(header(100, flags), &projection),
            expected
        );
    }
    assert_eq!(
        transform([0, 0, i32::MIN]).dispatch(header(1, 0), &projection),
        None
    );
    // A model behind the eye, drawn for the tracer outside the view, takes
    // the table and the cut-off of its distance, not of its negative depth.
    let behind = transform([1200, 0, -1600]);
    assert_eq!(
        behind.dispatch(header(100, 0), &projection),
        Some(ModelDispatch::Near)
    );
    assert_eq!(behind.at_distance().view_position, [1200, 0, 2000]);
    assert_eq!(
        behind.at_distance().dispatch(header(100, 0), &projection),
        Some(ModelDispatch::Far)
    );
    assert_eq!(
        transform([0, -2200, -10])
            .at_distance()
            .dispatch(header(100, 0), &projection),
        None
    );
}

struct Clock(u16);
impl ModelValueCallback for Clock {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        assert_eq!(selector, 0);
        Ok(self.0)
    }
}

#[test]
fn retail_saved_basis_commands_connect_value_execution_to_child_inheritance() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let table = Arc::new(assets.sine_table().values);
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    for clock in [0u16, 1024, 2048] {
        for id in [745, 747, 749, 755, 756] {
            let resource = runtime.resource(8, id).unwrap();
            let HandlerResources::Models(bank) =
                &resource.overlay.handlers()[resource.registration.region_index]
            else {
                panic!("retail model")
            };
            let model = &bank.entries[usize::from(resource.registration.local_index)];
            let mut context = ModelContext::new([100, 200, 300], MenuBasis::INITIAL);
            let mut values = ModelValues::new_invocation(Arc::clone(&table));
            let mut callback = Clock(clock);
            let mut saved_count = 0;
            let mut child_count = 0;
            // This exercises authored value/03/0E state transfer only; geometry
            // and primitive execution remain the enclosing walker's work.
            for instruction in &model.program.instructions {
                match &instruction.kind {
                    ModelInstructionKind::RegisterOperation(_) => {
                        values.execute_register(instruction, &mut callback).unwrap();
                    }
                    ModelInstructionKind::Opaque(command) if instruction.raw_command as u8 == 3 => {
                        let [orientation, axis, raw_angle] = command.arguments.as_slice() else {
                            panic!("saved basis arguments")
                        };
                        context.save_oriented_basis(*orientation);
                        if *raw_angle != 0 {
                            let angle = values.evaluate_packed(*raw_angle, &mut callback).unwrap();
                            context.rotate_saved_basis(*axis as i16, angle, &math);
                        }
                        saved_count += 1;
                    }
                    ModelInstructionKind::ModelInstance(instance) => {
                        assert_eq!(instance.orientation.raw(), 6);
                        let child = context.child([11, 22, 33], instance.orientation.raw());
                        assert_eq!(child.basis, *context.saved_basis());
                        assert_eq!(*child.saved_basis(), MenuBasis::INITIAL);
                        child_count += 1;
                    }
                    _ => {}
                }
            }
            assert_eq!((saved_count, child_count), (1, 1));
            let mut expected = MenuBasis::INITIAL;
            math.rotate_in_plane(
                &mut expected,
                if id == 745 {
                    0xc000
                } else {
                    i32::from(clock.wrapping_shl(4))
                },
                if id == 745 {
                    RotationPlane::Yz
                } else {
                    RotationPlane::Xz
                },
            );
            assert_eq!(*context.saved_basis(), expected);
            assert_eq!(context.basis, MenuBasis::INITIAL);
        }
    }
}

#[test]
fn fistp_low32_matches_the_portable_rounding() {
    let bound = 9_223_372_036_854_775_808.0_f64;
    let mut values = vec![
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.5,
        -1.5,
        2.5,
        -2.5,
        0.49999999999999994,
        -0.49999999999999994,
        2147483647.5,
        2147483648.5,
        -2147483648.5,
        -2147483649.5,
        4294967296.5,
        1e18,
        -1e18,
        bound,
        -bound,
        bound * 2.0,
        -bound * 2.0,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
        f64::MIN_POSITIVE,
        5e-324,
    ];
    values.extend([
        f64::from_bits(bound.to_bits() - 1),
        -f64::from_bits(bound.to_bits() - 1),
    ]);
    values.extend((-2000..2000).map(|i| f64::from(i) * 0.25 + 1e9));
    for value in values {
        assert_eq!(
            super::fistp_low32(value),
            super::fistp_low32_portable(value),
            "{value:e}"
        );
    }
}
