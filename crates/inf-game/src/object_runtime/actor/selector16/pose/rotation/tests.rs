use super::*;

fn table() -> [u16; 4096] {
    let mut table = [0; 4096];
    table[1] = 12000;
    table[4094] = 30000;
    table[4095] = 32767;
    table
}

#[test]
fn rotation_vector_quarter_turn_uses_axis_angle_cross_sign_and_q31_scale() {
    let mut basis = NativeBasis::INITIAL;
    rotate_local_vector(&mut basis, [0, 0x4000, 0], &table());
    let unit = (f64::from(0x7fff_7fff_i32) / 2_147_483_648.0) as f32;
    assert_eq!(read(&basis, 1), [0.0, 0.0, -unit]);
    assert_eq!(read(&basis, 4), [0.0, 1.0, 0.0]);
    assert_eq!(read(&basis, 7), [unit, 0.0, 0.0]);
    assert_eq!(basis.words[0], 3);
}

#[test]
fn zero_vector_still_applies_native_cosine_and_local_transform_rounds_ties_even() {
    let mut basis = NativeBasis::INITIAL;
    rotate_local_vector(&mut basis, [0; 3], &table());
    assert_eq!(read(&basis, 1), [1.0, 0.0, 0.0]);
    assert!(read(&basis, 4)[1] < 1.0);
    assert_eq!(basis.words[0], 3);
    let mut source = NativeBasis::INITIAL;
    source.words[1] = 0.5_f32.to_bits();
    let mut even = source;
    let mut tie = source;
    let mut next = source;
    rotate_local_vector(&mut even, [4, 0, 0], &table());
    rotate_local_vector(&mut tie, [5, 0, 0], &table());
    rotate_local_vector(&mut next, [7, 0, 0], &table());
    assert_eq!(even, tie); // 2.5 -> even 2, then table index zero
    assert_ne!(tie, next); // 3.5 -> even 4, then table index one
}

#[test]
fn counter_adds_three_wraps_and_renormalizes_strictly_above_one_hundred() {
    let mut boundary = NativeBasis::INITIAL;
    boundary.words[0] = 97;
    rotate_local_vector(&mut boundary, [0; 3], &table());
    assert_eq!(boundary.words[0], 100);
    let mut crossing = NativeBasis::INITIAL;
    crossing.words[0] = 98;
    rotate_local_vector(&mut crossing, [0; 3], &table());
    assert_eq!(crossing, NativeBasis::INITIAL);
    let mut wrapping = NativeBasis::INITIAL;
    wrapping.words[0] = u32::MAX;
    rotate_local_vector(&mut wrapping, [0; 3], &table());
    assert_eq!(wrapping.words[0], 2);
    assert_ne!(wrapping.words[5], NativeBasis::INITIAL.words[5]);
}
