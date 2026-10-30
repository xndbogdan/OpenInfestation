use inf_formats::native_basis::{isqrt_48efc0, NativeBasis};

pub(super) fn squared_word_distance(delta: [i16; 3]) -> u32 {
    delta.into_iter().fold(0_u32, |sum, word| {
        let word = i32::from(word);
        sum.wrapping_add(word.wrapping_mul(word) as u32)
    })
}

pub(super) fn direct_word_distance(square: u32) -> i32 {
    isqrt_48efc0(square as i32) as i16 as i32
}

pub(super) struct ImpulseVectors {
    pub linear: [u16; 3],
    pub angular: [u32; 3],
}

/// 0047A700 ->00490640/00492120/150/180 sums Z/Y/X and spills each result
/// to f32. The input is stored f32 from signed integer ratios in 00402680.
fn forward_float(basis: NativeBasis, local: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = local.map(f64::from);
    std::array::from_fn(|axis| {
        let coefficient = |first: usize| f64::from(f32::from_bits(basis.words[first + axis]));
        ((z * coefficient(7) + y * coefficient(4)) + x * coefficient(1)) as f32
    })
}

fn ftol_low_word(value: f64) -> i16 {
    let integral = value.trunc();
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
        integral as i64 as i16
    } else {
        0 // Native masked integer-indefinite QWORD has zero low WORD.
    }
}

pub(super) fn impulse_vectors(
    basis: NativeBasis,
    original_delta: [i16; 3],
    impulse: [i16; 3],
    mass: u16,
    linear_scale: i32,
) -> ImpulseVectors {
    let local = basis.inverse_words(original_delta);
    let direction = forward_float(
        basis,
        [
            (if local[0] < 0 { 1672.0 } else { -1672.0 }) / 4096.0,
            (if local[1] < -100 { 1672.0 } else { -1672.0 }) / 4096.0,
            (if local[2] < 0 { 3344.0 } else { -3344.0 }) / 4096.0,
        ],
    );
    let [x, y, z] = impulse.map(|word| f64::from(f32::from(word)));
    let [dx, dy, dz] = direction.map(f64::from);
    // Native __ftol chops before keeping each signed low WORD. It computes
    // negative cross-Z, negative cross-X, negative cross-Y in that order.
    let cross_z = ftol_low_word(y * dx - x * dy);
    let cross_x = ftol_low_word(z * dy - y * dz);
    let cross_y = ftol_low_word(x * dz - z * dx);
    let (linear, angular_scale) = if mass < 128 {
        (impulse.map(|word| word as u16), 256)
    } else {
        (
            impulse.map(|word| (i32::from(word) * linear_scale / i32::from(mass)) as u16),
            0x8000 / i32::from(mass),
        )
    };
    ImpulseVectors {
        linear,
        angular: [cross_x, cross_y, cross_z]
            .map(|word| i32::from(word).wrapping_mul(angular_scale).wrapping_neg() as u32),
    }
}
