use super::*;
use crate::menu::MenuBasis;

fn backend(dispatch: ModelDispatch) -> ModelProjectionBackend {
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

fn project(backend: &mut ModelProjectionBackend, input: ModelVertexInput) -> ModelVertexProjection {
    let mut result = ModelVertexProjection::default();
    let depth = backend.project(input, &mut result).unwrap();
    assert_eq!(result.read_u32(4), Some(depth));
    result
}

fn xy(projection: &ModelVertexProjection) -> [i16; 2] {
    let packed = projection.read_u32(0).unwrap();
    [packed as i16, (packed >> 16) as i16]
}

fn basis(rows: [[f32; 3]; 3]) -> MenuBasis {
    let mut basis = MenuBasis::INITIAL;
    for (target, source) in basis.words[1..].iter_mut().zip(rows.into_iter().flatten()) {
        *target = source.to_bits();
    }
    basis
}

#[test]
fn all_four_callbacks_publish_xy_depth_and_clip_with_exact_defined_bytes() {
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        for input in [
            ModelVertexInput::Local([32, -16, 256]),
            ModelVertexInput::World([32, -16, 256]),
        ] {
            let output = project(&mut backend(dispatch), input);
            assert_eq!(xy(&output), [360, 255]);
            assert_eq!(output.read_u32(4), Some(256));
            assert_eq!(output.bytes[8], Some(0x12));
            assert_eq!(
                output.bytes[9],
                (dispatch == ModelDispatch::Far).then_some(128)
            );
            assert_eq!(&output.bytes[10..], &[None, None]);
        }
    }
}

#[test]
fn depth_below_64_keeps_unprojected_coordinates_and_preserves_fog_and_padding() {
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        for input in [
            ModelVertexInput::Local([70000, -70000, 63]),
            ModelVertexInput::World([70000, -70000, 63]),
        ] {
            let mut output = ModelVertexProjection {
                bytes: [Some(0x57); 12],
            };
            assert_eq!(backend(dispatch).project(input, &mut output), Ok(63));
            assert_eq!(xy(&output), [4464, -4464]);
            assert_eq!(output.bytes[8], Some(0xc0));
            assert_eq!(&output.bytes[9..], &[Some(0x57); 3]);
        }
    }
    let output = project(
        &mut backend(ModelDispatch::Near),
        ModelVertexInput::Local([1, 1, 64]),
    );
    assert_eq!(xy(&output), [325, 236]);
    assert_eq!(output.bytes[8], Some(0x12));
}

#[test]
fn near_preserves_an_existing_far_fog_byte() {
    let mut output = project(
        &mut backend(ModelDispatch::Far),
        ModelVertexInput::Local([0, 0, 256]),
    );
    backend(ModelDispatch::Near)
        .project(ModelVertexInput::Local([1, 1, 64]), &mut output)
        .unwrap();
    assert_eq!(output.bytes[9], Some(128));
}

#[test]
fn local_composed_columns_and_world_camera_rows_are_distinct_contracts() {
    let mut projector = backend(ModelDispatch::Near);
    let matrix = basis([[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
    projector.transform.view_basis = matrix;
    projector.transform.view_position = [4, 8, 0];
    projector.view.basis = matrix;
    projector.view.eye = [10, 20, 30];
    let local = project(&mut projector, ModelVertexInput::Local([32, 16, 256]));
    assert_eq!(xy(&local), [305, 202]); // local becomes[-12,40,256], Y=37.5=>38
    let world = project(&mut projector, ModelVertexInput::World([42, 36, 286]));
    assert_eq!(xy(&world), [340, 270]); // camera rows yield[16,-32,256]
}

#[test]
fn world_delta_wraps_then_sign_extends_with_masked_x86_shift_count() {
    let mut projector = backend(ModelDispatch::Near);
    projector.view.eye = [i32::MAX, 0, 0];
    projector.coordinate_shift = 48; // native CL masks to16, signed lowWORD
    let output = project(
        &mut projector,
        ModelVertexInput::World([i32::MIN, 65535, 256]),
    );
    assert_eq!(xy(&output), [321, 241]);
    projector.coordinate_shift = 32;
    let output = project(&mut projector, ModelVertexInput::World([i32::MIN, 1, 256]));
    assert_eq!(xy(&output), [321, 239]);
}

#[test]
fn transform_and_perspective_round_halfway_values_to_even() {
    let mut projector = backend(ModelDispatch::Near);
    projector.transform.view_basis = basis([[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 1.0]]);
    let output = project(&mut projector, ModelVertexInput::Local([5, -3, 63]));
    assert_eq!(xy(&output), [2, -2]);
    projector.transform.view_basis = MenuBasis::INITIAL;
    projector.projection.focal = [128; 2];
    projector.projection.center = [0; 2];
    for (input, expected) in [([5, 3, 256], [2, -2]), ([-5, -3, 256], [-2, 2])] {
        assert_eq!(
            xy(&project(&mut projector, ModelVertexInput::Local(input))),
            expected
        );
    }
}

#[test]
fn dot_keeps_53_bit_arithmetic_and_z_y_x_addition_order() {
    // Each product is exact; losing the low unit to a premature f32 spill
    // would return0 instead of1 after the cancellation.
    assert_eq!(
        dot(
            [1, 1, 1],
            [-16_777_216.0_f32, 1.0, 16_777_216.0].map(f32::to_bits)
        ),
        1
    );
    // At2^53, Z+Y loses1 before subtracting X, while X+Z+Y would retain1.
    assert_eq!(
        dot(
            [-1, 1, 1],
            [9_007_199_254_740_992.0_f32, 1.0, 9_007_199_254_740_992.0].map(f32::to_bits)
        ),
        0
    );
}

#[test]
fn qword_conversion_uses_low_dword_and_masked_invalid_value() {
    assert_eq!(
        dot([i32::MAX, 0, 0], [2.0_f32, 0.0, 0.0].map(f32::to_bits)),
        -2
    );
    assert_eq!(
        dot([1, 0, 0], [f32::INFINITY, 0.0, 0.0].map(f32::to_bits)),
        0
    );
    assert_eq!(dot([1, 0, 0], [f32::NAN, 0.0, 0.0].map(f32::to_bits)), 0);
    let mut projector = backend(ModelDispatch::Near);
    projector.transform.view_position = [i32::MAX, 0, 0];
    let output = project(&mut projector, ModelVertexInput::Local([1, 0, 63]));
    assert_eq!(xy(&output), [0, 0]);
}

#[test]
fn projection_preserves_signed_integer_precision_before_division() {
    let mut projector = backend(ModelDispatch::Near);
    projector.projection.focal = [32; 2];
    projector.projection.center = [-8_259_552, 0];
    // Casting input/focal products to f32 would lose these low bits.
    let output = project(&mut projector, ModelVertexInput::Local([16_777_217, 0, 65]));
    assert_eq!(xy(&output), [1, 0]);
    projector.projection.focal = [i32::MAX, 0];
    projector.projection.center = [0; 2];
    let output = project(&mut projector, ModelVertexInput::Local([i32::MAX, 0, 64]));
    assert_eq!(xy(&output), [-16384, 0]); // projected lowDWORD is-67108864
}

#[test]
fn reduction_uses_inclusive_32767_threshold_and_signed_abs_or() {
    let mut projector = backend(ModelDispatch::Near);
    projector.projection.focal = [64; 2];
    projector.projection.center = [0; 2];
    for (input, expected) in [
        ([32766, 0, 64], [32766, 0]),
        ([32767, 0, 64], [16383, 0]),
        ([-32767, 0, 64], [-16384, 0]),
        ([i32::MIN, -7, 64], [0, 7]),
    ] {
        assert_eq!(
            xy(&project(&mut projector, ModelVertexInput::Local(input))),
            expected
        );
    }
}

#[test]
fn clip_classification_is_unsigned_against_viewport_then_signed_outside() {
    let mut projector = backend(ModelDispatch::Near);
    projector.projection.focal = [64; 2];
    projector.projection.center = [0; 2];
    for (input, clip) in [
        ([-1, 1, 64], 0x09),
        ([0, 0, 64], 0x12),
        ([639, -479, 64], 0x12),
        ([640, -480, 64], 0x24),
    ] {
        assert_eq!(
            project(&mut projector, ModelVertexInput::Local(input)).bytes[8],
            Some(clip)
        );
    }
    projector.projection.viewport = [-1; 2];
    assert_eq!(
        project(&mut projector, ModelVertexInput::Local([-2, 2, 64])).bytes[8],
        Some(0x12)
    );
}

#[test]
fn far_fog_uses_live_coefficient_low_product_and_signed_interval_order() {
    let mut projector = backend(ModelDispatch::Far);
    for (depth, fog) in [(127, 0), (128, 0), (129, 1), (383, 255), (384, 255)] {
        assert_eq!(
            project(&mut projector, ModelVertexInput::Local([0, 0, depth])).bytes[9],
            Some(fog)
        );
    }
    projector.fog_factor = -65536;
    assert_eq!(
        project(&mut projector, ModelVertexInput::World([0, 0, 129])).bytes[9],
        Some(255)
    );
    projector.fog_factor = i32::MAX;
    assert_eq!(
        project(&mut projector, ModelVertexInput::World([0, 0, 130])).bytes[9],
        Some(255)
    );
    projector.projection.fade = [300, 100];
    assert_eq!(
        project(&mut projector, ModelVertexInput::Local([0, 0, 200])).bytes[9],
        Some(0)
    );
    assert_eq!(
        project(&mut projector, ModelVertexInput::Local([0, 0, 300])).bytes[9],
        Some(255)
    );
}
