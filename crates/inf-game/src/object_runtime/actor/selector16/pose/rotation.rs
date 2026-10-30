//! 0047A760 -> 004909E0 -> 00490FC0/00491D10. Local rotation-vector
//! transformation followed by one axis/angle rotation, not Euler calls.

use inf_formats::native_basis::{
    normalize_words, rotate_axis_vector, NativeBasis, NativeLengthPrecision,
};

pub(in crate::object_runtime::actor::selector16) fn rotate_local_vector(
    basis: &mut NativeBasis,
    local: [u16; 3],
    table: &[u16; 4096],
) {
    let world = basis.transform_words(local.map(|word| word as i16));
    rotate_world_vector(basis, world, table);
}

/// 0047A790 invokes 00490FC0 directly; its vector is already in world space.
/// Keep the common axis/angle consumer identical to the camera's local wrapper.
pub(in crate::object_runtime::actor) fn rotate_world_vector(
    basis: &mut NativeBasis,
    world: [u16; 3],
    table: &[u16; 4096],
) {
    let [x, y, z] = world.map(|word| f64::from(word as i16));
    // 00490FFC..1014 accumulates Z/Y before X and retains the length.
    let length = ((z * z + y * y) + x * x).sqrt();
    let axis = if length == 0.0 {
        [1.0_f32, 0.0, 0.0]
    } else {
        [x, y, z].map(|value| (value / length) as f32)
    };
    let angle = length.round_ties_even() as u32;
    for first in [1, 4, 7] {
        let vector = std::array::from_fn::<_, 3, _>(|i| f32::from_bits(basis.words[first + i]));
        for (i, value) in rotate_axis_vector(vector, axis, angle as i32, table)
            .into_iter()
            .enumerate()
        {
            basis.words[first + i] = value.to_bits();
        }
    }
    basis.words[0] = basis.words[0].wrapping_add(3);
    if basis.words[0] > 100 {
        renormalize_rotation_vector(basis);
    }
}

fn quantize(vector: [f32; 3]) -> [i16; 3] {
    vector.map(|value| {
        let integral = (f64::from(value) * 4096.0).round_ties_even();
        if integral.is_finite()
            && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&integral)
        {
            integral as i64 as i16
        } else {
            // Masked invalid FISTP writes QWORD8000000000000000, lowWORD0.
            0
        }
    })
}

fn read(basis: &NativeBasis, first: usize) -> [f32; 3] {
    std::array::from_fn(|i| f32::from_bits(basis.words[first + i]))
}

fn write(basis: &mut NativeBasis, first: usize, vector: [f32; 3]) {
    for (i, value) in vector.into_iter().enumerate() {
        basis.words[first + i] = value.to_bits();
    }
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    let [lx, ly, lz] = left.map(f64::from);
    let [rx, ry, rz] = right.map(f64::from);
    [
        (ly * rz - lz * ry) as f32,
        (lz * rx - lx * rz) as f32,
        (lx * ry - ly * rx) as f32,
    ]
}

fn renormalize_rotation_vector(basis: &mut NativeBasis) {
    // 004910E9/1128/119D all call 00417DA0, retaining their sqrt divisor.
    // Euler 00491290's inlined X normalization instead spills that divisor;
    // it cannot replace this sequence without changing the low float bits.
    let z = normalize_words(quantize(read(basis, 7)), NativeLengthPrecision::Retained);
    write(basis, 7, z);
    let y = normalize_words(quantize(read(basis, 4)), NativeLengthPrecision::Retained);
    write(basis, 4, y);
    let x = normalize_words(quantize(cross(y, z)), NativeLengthPrecision::Retained);
    write(basis, 1, x);
    write(basis, 4, cross(z, x));
    basis.words[0] = 0;
}

#[cfg(test)]
mod tests;
