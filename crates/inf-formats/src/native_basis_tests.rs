use std::path::Path;

use super::*;

#[test]
fn oriented_copy_maps_source_signs_and_preserves_destination_counter() {
    let mut source = NativeBasis::INITIAL;
    for (index, word) in source.words.iter_mut().enumerate().skip(1) {
        *word = (index as f32).to_bits();
    }
    let mut target = NativeBasis::INITIAL;
    target.words[0] = 99;
    target.copy_oriented_from(source, 1 | 0x20 | 0x08);
    assert_eq!(target.words[0], 99);
    assert_eq!(vector(&target, 1), [-7.0, -8.0, -9.0]);
    assert_eq!(vector(&target, 4), [-1.0, -2.0, -3.0]);
    assert_eq!(vector(&target, 7), [4.0, 5.0, 6.0]);
    for orientation in [6, 7, 0x4000] {
        target.copy_oriented_from(source, orientation);
        assert_eq!(&target.words[1..], &source.words[1..]);
        assert_eq!(target.words[0], 99);
    }
}

#[test]
fn point_transform_rounds_ties_even_and_keeps_fistp_low_dword() {
    let mut basis = NativeBasis::INITIAL;
    basis.words[1] = 0.5f32.to_bits();
    assert_eq!(basis.transform_point([1, 2, 3]), [0, 2, 3]);
    assert_eq!(basis.transform_point([3, 2, 3]), [2, 2, 3]);
    assert_eq!(basis.transform_point([-3, 2, 3]), [-2, 2, 3]);
    basis.words[1] = 2.0f32.to_bits();
    assert_eq!(basis.transform_point([i32::MAX, 0, 0]), [-2, 0, 0]);
    basis.words[1] = f32::INFINITY.to_bits();
    assert_eq!(basis.transform_point([1, 0, 0]), [0, 0, 0]);
    // Original retained order is (Z + Y) + X, not X + Y + Z.
    basis.words[1] = 1.0f32.to_bits();
    basis.words[4] = (-2.0f32.powi(60)).to_bits();
    basis.words[7] = 2.0f32.powi(60).to_bits();
    assert_eq!(basis.transform_point([1, 1, 1])[0], 1);
}

fn retail_math() -> NativeBasisMath {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = crate::FrontendExecutable::parse(&bytes).unwrap();
    let table =
        crate::parse_frontend_sine_table(executable.data(), crate::FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap();
    NativeBasisMath::new(table.values)
}

#[test]
fn original_table_uses_native_scale_unsigned_samples_and_quadrant_bits() {
    let math = retail_math();
    assert_eq!(math.sine(0), 0.0);
    assert_eq!(math.sine(4), 12.0 * f64::from(TRIG_SCALE));
    assert_eq!(math.sine(0x1000), 12539.0 * f64::from(TRIG_SCALE));
    assert_eq!(math.sine(0x4000), 32767.0 * f64::from(TRIG_SCALE));
    assert_ne!(math.sine(0x4000), 32767.0 / 32768.0);
    assert_eq!(math.sine(0x8000), 0.0);
    assert_eq!(math.sine(0xc000), -math.sine(0x4000));
    assert_eq!(math.sine(-4), math.sine(0xfffc));
    for angle in [0, 4, 0x1234, 0x3ffc, 0x4000, 0xfffc] {
        for low in 0..4 {
            assert_eq!(math.sine(angle), math.sine(angle + low));
        }
        assert_eq!(math.sine(angle), math.sine(angle + 0x10000));
    }
    let mut table = [0_u16; 4096];
    table[4095] = 65535;
    let unsigned = NativeBasisMath::new(table);
    assert_eq!(unsigned.sine(0x4000), 65535.0 * f64::from(TRIG_SCALE));
}

#[test]
fn inverse_sine_lookup_uses_signed_samples_saturation_and_five_bit_indices() {
    let mut table = [0_i16; 1024];
    table[0] = 7;
    table[1] = -20;
    table[1023] = i16::MIN;
    for value in [0, 1, 31] {
        assert_eq!(inverse_sine_q15(value, &table), 7);
    }
    for value in [32, 63] {
        assert_eq!(inverse_sine_q15(value, &table), -20);
        assert_eq!(inverse_sine_q15(-value, &table), 20);
    }
    for value in [32736, 32767, 32768, i32::MAX] {
        assert_eq!(inverse_sine_q15(value, &table), -32768);
        assert_eq!(inverse_sine_q15(-value, &table), 32768);
    }
    assert_eq!(inverse_sine_q15(i32::MIN, &table), 32768);

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = crate::FrontendExecutable::parse(&bytes).unwrap();
    let retail = crate::parse_frontend_inverse_sine_table(
        executable.data(),
        crate::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    assert_eq!(&retail.values[..5], &[0, 10, 20, 30, 40]);
    assert_eq!(inverse_sine_q15(32, &retail.values), 10);
    assert_eq!(inverse_sine_q15(-32, &retail.values), -10);
    assert_eq!(
        inverse_sine_q15(i32::MIN, &retail.values),
        -i32::from(retail.values[1023])
    );
}

#[test]
fn q31_inverse_sine_uses_all_signed_word_buckets_without_q15_saturation() {
    let table = std::array::from_fn(|index| (index as i16).wrapping_mul(127));
    for (index, &word) in table.iter().enumerate() {
        let first = (index as i32) << 21;
        let last = first | ((1 << 21) - 1);
        for input in [first, last] {
            assert_eq!(inverse_sine_q31(input, &table), Ok(i32::from(word)));
            if input != 0 {
                assert_eq!(inverse_sine_q31(-input, &table), Ok(-i32::from(word)));
            }
        }
    }
    assert_eq!(inverse_sine_q31(1 << 21, &table), Ok(i32::from(table[1])));
    assert_eq!(inverse_sine_q15(1 << 21, &table), i32::from(table[1023]));
    let mut signed = table;
    signed[1] = i16::MIN;
    assert_eq!(inverse_sine_q31(1 << 21, &signed), Ok(-32768));
    assert_eq!(inverse_sine_q31(-(1 << 21), &signed), Ok(32768));
    assert_eq!(
        inverse_sine_q31(i32::MIN, &table),
        Err(InverseSineQ31Error::OutOfTable {
            input: i32::MIN,
            index: -1024
        })
    );
}

#[test]
fn actual_q31_ratios_use_the_original_inverse_table_with_twenty_one_bit_indices() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = crate::FrontendExecutable::parse(&bytes).unwrap();
    let table = crate::parse_frontend_inverse_sine_table(
        executable.data(),
        crate::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap()
    .values;
    assert_eq!(inverse_sine_q31(0, &table), Ok(0));
    assert_eq!(inverse_sine_q31((1 << 21) - 1, &table), Ok(0));
    assert_eq!(inverse_sine_q31(1 << 21, &table), Ok(10));
    assert_eq!(inverse_sine_q31(-(1 << 21), &table), Ok(-10));
    for numerator in [i32::MIN, -32768, -1, 0, 1, 32767, i32::MAX] {
        for denominator in [i32::MIN, -56754, -1, 0, 1, 56754, i32::MAX] {
            let ratio = q31_divide(numerator as u32, denominator as u32) as i32;
            assert_ne!(ratio, i32::MIN);
            let index = (ratio.unsigned_abs() >> 21) as usize;
            let expected = i32::from(table[index]) * if ratio < 0 { -1 } else { 1 };
            assert_eq!(inverse_sine_q31(ratio, &table), Ok(expected));
        }
    }
}

#[test]
fn plain_q15_sine_is_the_signed_table_word_without_replication() {
    let math = retail_math();
    let table = &math.sine_quarter;
    for (angle, expected) in [
        (0, 0),
        (4, 12),
        (7, 12),
        (0x1000, 12539),
        (0x4000, 32767),
        // The second quarter mirrors through 0x3FFC, not 0x4000.
        (0x7ffc, 0),
        (0x8000, 0),
        (0x9000, -12539),
        (0xc000, -32767),
        (0x1_1000, 12539),
        (-0x4000, -32767),
    ] {
        assert_eq!(sine_q15(angle, table), expected, "angle {angle:#x}");
    }
    // The Q31 path replicates the same word into both halves.
    for angle in [0x1000, 0x9000, 0x4000] {
        let q15 = sine_q15(angle, table);
        let replicated = (q15.unsigned_abs() | (q15.unsigned_abs() << 16)) as i32;
        let expected = if q15 < 0 { -replicated } else { replicated };
        assert_eq!(sine_q31(angle, table), expected);
    }
}

/// 00417DA0 executed unmodified at FPCW 027F (.tmp/inf-line-text-oracle.py).
/// The first rows are the name-entry light 4000, 4000, 6000 - min(age, 16000).
const NATIVE_NORMALIZE: [([i16; 3], [u32; 3]); 20] = [
    ([4000, 4000, 6000], [0x3ef85b42, 0x3ef85b42, 0x3f3a4472]),
    ([4000, 4000, 5999], [0x3ef860de, 0x3ef860de, 0x3f3a40b4]),
    ([4000, 4000, 5001], [0x3f079ec6, 0x3f079ec6, 0x3f298f26]),
    ([4000, 4000, 4000], [0x3f13cd3a, 0x3f13cd3a, 0x3f13cd3a]),
    ([4000, 4000, 1], [0x3f3504f3, 0x3f3504f3, 0x39395d22]),
    ([4000, 4000, 0], [0x3f3504f3, 0x3f3504f3, 0x00000000]),
    ([4000, 4000, -1], [0x3f3504f3, 0x3f3504f3, 0xb9395d22]),
    ([4000, 4000, -10000], [0x3eb2416a, 0x3eb2416a, 0xbf5ed1c4]),
    ([0, 0, 0], [0x3f800000, 0x00000000, 0x00000000]),
    ([0, 0, -1], [0x00000000, 0x00000000, 0xbf800000]),
    (
        [-32768, -32768, -32768],
        [0xbf13cd3a, 0xbf13cd3a, 0xbf13cd3a],
    ),
    ([32767, 32767, 32767], [0x3f13cd3a, 0x3f13cd3a, 0x3f13cd3a]),
    ([3, 4, 12], [0x3e6c4ec5, 0x3e9d89d9, 0x3f6c4ec5]),
    ([1, 2, 3], [0x3e88d677, 0x3f08d677, 0x3f4d41b3]),
    ([-7, 11, -13], [0xbec2a7fa, 0x3f18f1b2, 0xbf34c08d]),
    ([15857, 11435, -4784], [0x3f49b0b9, 0x3f11720a, 0xbe7365a1]),
    ([8082, -19711, 19720], [0x3e8e8b1a, 0xbf2dd2a5, 0x3f2de6f6]),
    (
        [13689, -28707, -15343],
        [0x3ec67c30, 0xbf501ebe, 0xbede77ab],
    ),
    (
        [13143, -28444, -23769],
        [0x3eab19d5, 0xbf3925d4, 0xbf1ab79d],
    ),
    ([19607, -14532, 15244], [0x3f2e6fc9, 0xbf014941, 0x3f079ede]),
];

#[test]
fn word_normalization_matches_native_00417da0_receipts() {
    for (words, expected) in NATIVE_NORMALIZE {
        assert_eq!(
            normalize_words(words, NativeLengthPrecision::Retained).map(f32::to_bits),
            expected,
            "words {words:?}"
        );
    }
}

#[test]
fn native_quarter_turn_rotates_x_and_z_and_preserves_y() {
    let math = retail_math();
    let mut basis = NativeBasis::INITIAL;
    store_vector(&mut basis, 1, [1.0, 2.0, 3.0]);
    store_vector(&mut basis, 4, [7.0, 8.0, 9.0]);
    store_vector(&mut basis, 7, [4.0, 5.0, 6.0]);
    math.rotate(&mut basis, 0x4000);
    assert_eq!(basis.words[0], 1);
    assert_eq!(vector(&basis, 1), [-4.0, -5.0, -6.0]);
    assert_eq!(vector(&basis, 4), [7.0, 8.0, 9.0]);
    assert_eq!(vector(&basis, 7), [1.0, 2.0, 3.0]);

    let mut negative = NativeBasis::INITIAL;
    math.rotate(&mut negative, -0x4000);
    assert_eq!(vector(&negative, 1), [0.0, 0.0, 1.0]);
    assert_eq!(vector(&negative, 7), [-1.0, 0.0, 0.0]);
}

#[test]
fn frontend_planes_preserve_native_signs_and_held_vector() {
    let math = retail_math();
    for (plane, expected) in [
        (
            RotationPlane::Yz,
            [[1.0, 2.0, 3.0], [7.0, 8.0, 9.0], [-4.0, -5.0, -6.0]],
        ),
        (
            RotationPlane::Xy,
            [[4.0, 5.0, 6.0], [-1.0, -2.0, -3.0], [7.0, 8.0, 9.0]],
        ),
    ] {
        let mut basis = NativeBasis::INITIAL;
        store_vector(&mut basis, 1, [1.0, 2.0, 3.0]);
        store_vector(&mut basis, 4, [4.0, 5.0, 6.0]);
        store_vector(&mut basis, 7, [7.0, 8.0, 9.0]);
        math.rotate_in_plane(&mut basis, 0x4000, plane);
        assert_eq!(basis.words[0], 1);
        assert_eq!(
            [vector(&basis, 1), vector(&basis, 4), vector(&basis, 7)],
            expected
        );
    }
}

/// 00491A70 turns about world Y: within each row x = cos*x - sin*z and
/// z = sin*x + cos*z, so a quarter turn takes each row's (x, z) to (-z, x).
#[test]
fn world_y_turn_mixes_each_rows_x_and_z() {
    let math = retail_math();
    let mut basis = NativeBasis::INITIAL;
    store_vector(&mut basis, 1, [1.0, 2.0, 3.0]);
    store_vector(&mut basis, 4, [4.0, 5.0, 6.0]);
    store_vector(&mut basis, 7, [7.0, 8.0, 9.0]);
    math.rotate_in_plane(&mut basis, 0x4000, RotationPlane::WorldY);
    assert_eq!(basis.words[0], 1);
    assert_eq!(
        [vector(&basis, 1), vector(&basis, 4), vector(&basis, 7)],
        [[-3.0, 2.0, 1.0], [-6.0, 5.0, 4.0], [-9.0, 8.0, 7.0]]
    );
}

#[test]
fn frontend_planes_run_the_same_101st_call_normalization() {
    let math = retail_math();
    for plane in [RotationPlane::Yz, RotationPlane::Xy, RotationPlane::WorldY] {
        let mut basis = NativeBasis::INITIAL;
        basis.words[0] = 100;
        store_vector(&mut basis, 1, [2.0, 3.0, 4.0]);
        math.rotate_in_plane(&mut basis, 0, plane);
        assert_eq!(basis, NativeBasis::INITIAL);
        basis.words[0] = u32::MAX;
        math.rotate_in_plane(&mut basis, 0, plane);
        assert_eq!(basis.words[0], 0);
    }
}

#[test]
fn renormalization_starts_on_call_101_and_keeps_unsigned_counter_wrap() {
    let math = retail_math();
    let mut basis = NativeBasis::INITIAL;
    basis.words[0] = 99;
    store_vector(&mut basis, 1, [2.0, 3.0, 4.0]);
    math.rotate(&mut basis, 0);
    assert_eq!(basis.words[0], 100);
    assert_eq!(vector(&basis, 1), [2.0, 3.0, 4.0]);
    math.rotate(&mut basis, 0);
    assert_eq!(basis, NativeBasis::INITIAL);

    basis.words[0] = u32::MAX;
    store_vector(&mut basis, 1, [2.0, 3.0, 4.0]);
    math.rotate(&mut basis, 0);
    assert_eq!(basis.words[0], 0);
    assert_eq!(vector(&basis, 1), [2.0, 3.0, 4.0]);
}

#[test]
fn fistp_quantization_rounds_ties_even_then_keeps_signed_low_word() {
    for (scaled, expected) in [
        (0.5, 0),
        (1.5, 2),
        (2.5, 2),
        (-0.5, 0),
        (-1.5, -2),
        (-2.5, -2),
        (32767.0, 32767),
        (32768.0, -32768),
        (65535.0, -1),
        (65536.0, 0),
    ] {
        assert_eq!(quantize_word(scaled / 4096.0), expected);
    }
    for invalid in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
    ] {
        assert_eq!(quantize_word(invalid), 0);
    }
}

#[test]
fn retained_helper_length_and_inline_spilled_length_have_distinct_bits() {
    let source = [1.0 / 4096.0, 2.0 / 4096.0, 3.0 / 4096.0];
    let retained = normalize_quantized(source, NativeLengthPrecision::Retained).map(f32::to_bits);
    let spilled = normalize_quantized(source, NativeLengthPrecision::Spilled).map(f32::to_bits);
    assert_eq!(retained, [0x3e88_d677, 0x3f08_d677, 0x3f4d_41b3]);
    assert_eq!(spilled, [0x3e88_d677, 0x3f08_d677, 0x3f4d_41b2]);
    for precision in [
        NativeLengthPrecision::Retained,
        NativeLengthPrecision::Spilled,
    ] {
        assert_eq!(normalize_quantized([0.0; 3], precision), [1.0, 0.0, 0.0]);
    }
}

#[test]
fn normalization_rebuilds_x_then_y_without_re_normalizing_y() {
    let mut basis = NativeBasis::INITIAL;
    // Deliberately nonorthogonal Y/Z: normalizing a whole matrix by a generic
    // library routine would hide the original quantize/cross ordering.
    store_vector(&mut basis, 4, [0.0, 3.0 / 4096.0, 4.0 / 4096.0]);
    store_vector(&mut basis, 7, [1.0 / 4096.0, 2.0 / 4096.0, 3.0 / 4096.0]);
    let expected_z = normalize_quantized(vector(&basis, 7), NativeLengthPrecision::Retained);
    let expected_y_seed = normalize_quantized(vector(&basis, 4), NativeLengthPrecision::Retained);
    renormalize(&mut basis);
    assert_eq!(vector(&basis, 7), expected_z);
    assert_eq!(
        vector(&basis, 4),
        cross_stored(expected_z, vector(&basis, 1))
    );
    assert_ne!(vector(&basis, 4), expected_y_seed);
    assert_eq!(basis.words[0], 0);
}

// These architecture-specific checks execute small x87 arithmetic sequences,
// not retail executable code. They independently check rounding/store behavior
// against hardware while saving/restoring the caller's control word.
#[cfg(target_arch = "x86_64")]
mod x87 {
    use super::*;
    use std::arch::asm;

    fn control_words() -> (u16, u16) {
        let mut old = 0_u16;
        // SAFETY: valid writable u16, no live FPU stack entries are changed.
        unsafe {
            asm!("fnstcw word ptr [{old}]", old = in(reg) &mut old, options(nostack));
        }
        // 53-bit precision, nearest-even, masked exceptions as in retail CRT.
        (old, (old & !0x0f00) | 0x023f)
    }

    fn quantize(value: f32) -> i16 {
        let (old, control) = control_words();
        let mut result = 0_i64;
        let scale = 4096_f32;
        // SAFETY: all pointers address correctly sized local values. The FPU
        // push is consumed, and the original control word is restored.
        unsafe {
            asm!(
                "fldcw word ptr [{control}]",
                "fld dword ptr [{value}]",
                "fmul dword ptr [{scale}]",
                "fistp qword ptr [{result}]",
                "fldcw word ptr [{old}]",
                control = in(reg) &control, old = in(reg) &old,
                value = in(reg) &value, scale = in(reg) &scale,
                result = in(reg) &mut result,
                out("st(0)") _, options(nostack),
            );
        }
        result as i16
    }

    fn normalize(input: [i16; 3], spill: bool) -> [u32; 3] {
        assert!(input != [0; 3]);
        let (old, control) = control_words();
        let input = input.map(i32::from);
        let mut output = [0_f32; 3];
        let mut divisor = 0_f32;
        // SAFETY: stack-local arrays cover all addressed offsets. Quantized
        // i16 coordinates convert exactly, so FIMUL by i32 matches FMUL by
        // their f32 spills. The FPU stack is balanced along both branches.
        unsafe {
            asm!(
                "fldcw word ptr [{control}]",
                "fild dword ptr [{input} + 8]",
                "fimul dword ptr [{input} + 8]",
                "fild dword ptr [{input} + 4]",
                "fimul dword ptr [{input} + 4]",
                "faddp st(1), st(0)",
                "fild dword ptr [{input}]",
                "fimul dword ptr [{input}]",
                "faddp st(1), st(0)",
                "fsqrt",
                "fst dword ptr [{divisor}]",
                "test {spill:e}, {spill:e}",
                "jz 2f",
                "fild dword ptr [{input}]",
                "fdiv dword ptr [{divisor}]",
                "fstp dword ptr [{output}]",
                "fild dword ptr [{input} + 4]",
                "fdiv dword ptr [{divisor}]",
                "fstp dword ptr [{output} + 4]",
                "fild dword ptr [{input} + 8]",
                "fdiv dword ptr [{divisor}]",
                "fstp dword ptr [{output} + 8]",
                "jmp 3f",
                "2:",
                "fild dword ptr [{input}]",
                "fdiv st(0), st(1)",
                "fstp dword ptr [{output}]",
                "fild dword ptr [{input} + 4]",
                "fdiv st(0), st(1)",
                "fstp dword ptr [{output} + 4]",
                "fild dword ptr [{input} + 8]",
                "fdiv st(0), st(1)",
                "fstp dword ptr [{output} + 8]",
                "3:",
                "fstp st(0)",
                "fldcw word ptr [{old}]",
                control = in(reg) &control, old = in(reg) &old,
                input = in(reg) input.as_ptr(), output = in(reg) output.as_mut_ptr(),
                divisor = in(reg) &mut divisor, spill = in(reg) i32::from(spill),
                out("st(0)") _, out("st(1)") _, options(nostack),
            );
        }
        output.map(f32::to_bits)
    }

    #[test]
    fn quantization_matches_x87_nearest_even_and_signed_word_narrowing() {
        for scaled in [
            0.5_f32, 1.5, 2.5, -0.5, -1.5, -2.5, 32767.0, 32768.0, 65535.0, 65536.0,
        ] {
            let value = scaled / 4096.0;
            assert_eq!(quantize_word(value), quantize(value));
        }
    }

    #[test]
    fn normalization_memory_spills_match_x87_at_supported_crt_precision() {
        for source in [
            [1, 2, 3],
            [8191, -32768, 17007],
            [0, 32767, 1],
            [-123, 456, -789],
        ] {
            let vector = source.map(|value| f32::from(value) / 4096.0);
            for (precision, spill) in [
                (NativeLengthPrecision::Retained, false),
                (NativeLengthPrecision::Spilled, true),
            ] {
                assert_eq!(
                    normalize_quantized(vector, precision).map(f32::to_bits),
                    normalize(source, spill)
                );
                assert_eq!(
                    normalize_words(source, precision).map(f32::to_bits),
                    normalize(source, spill)
                );
            }
        }
    }
}

#[test]
fn q31_divide_matches_the_saturated_restoring_division() {
    // Hand-computed from the 0048EF30 listing.
    let cases: &[(u32, u32, u32)] = &[
        (1, 1, 0x7fff_ffff),
        (1, 2, 0x4000_0000),
        (0, 5, 0),
        (5, 0, 0x7fff_ffff),
        (0, 0, 0x7fff_ffff),
        (0x8000_0000, 1, 0x8000_0001),
        (0x8000_0000, 0x8000_0000, 0x7fff_ffff),
        (3, 6, 0x4000_0000),
        // Differing signs negate; saturation is asymmetric (MIN+1).
        (6u32.wrapping_neg(), 3, 0x8000_0001),
        (3, 6u32.wrapping_neg(), 0xc000_0000),
        (6u32.wrapping_neg(), 6u32.wrapping_neg(), 0x7fff_ffff),
        (1, 3, 0x2aaa_aaaa),
        (2, 3, 0x5555_5555),
        (0xffff_ffff, 1, 0x8000_0001),
        (1, 0xffff_ffff, 0x8000_0001),
    ];
    for &(a, b, expected) in cases {
        assert_eq!(q31_divide(a, b), expected, "q31_divide({a:#x}, {b:#x})");
    }
    // Brute-force oracle over small magnitudes: restoring division with
    // saturation matches i128 closed forms wherever they are defined.
    for a in (0..512u32).chain([0x7fff_ffff, 0x8000_0000, 0xffff_ffff]) {
        for b in (0..512u32).chain([0x7fff_ffff, 0x8000_0000, 0xffff_ffff]) {
            let (sa, sb) = (a as i32 as i128, b as i32 as i128);
            let expected = if sb == 0 || sa.abs() >= sb.abs() {
                if (sa < 0) != (sb < 0) {
                    0x8000_0001u32
                } else {
                    0x7fff_ffffu32
                }
            } else {
                let magnitude = ((sa.abs() << 31) / sb.abs()) as u32;
                if (sa < 0) != (sb < 0) {
                    magnitude.wrapping_neg()
                } else {
                    magnitude
                }
            };
            assert_eq!(q31_divide(a, b), expected, "q31_divide({a:#x}, {b:#x})");
        }
    }
}

#[test]
fn isqrt_48efc0_matches_floor_square_root() {
    // u64 Newton oracle: exact for the whole u32 domain, no float path.
    fn oracle(x: u32) -> u32 {
        if x == 0 {
            return 0;
        }
        let mut root = 1u32 << ((32 - x.leading_zeros() + 1) / 2);
        loop {
            let next = ((root as u64 + x as u64 / root as u64) / 2) as u32;
            if next >= root {
                break;
            }
            root = next;
        }
        while (root as u64 + 1) * (root as u64 + 1) <= x as u64 {
            root += 1;
        }
        while (root as u64) * (root as u64) > x as u64 {
            root -= 1;
        }
        root
    }
    // Hand-computed: the 07 expander's (3,4) edge normalizes through 5.
    assert_eq!(isqrt_48efc0(25), 5);
    assert_eq!(isqrt_48efc0(0), 0);
    assert_eq!(isqrt_48efc0(-1), 0);
    assert_eq!(isqrt_48efc0(i32::MIN), 0);
    assert_eq!(isqrt_48efc0(1), 1);
    assert_eq!(isqrt_48efc0(2), 1);
    // Exhaustive over both sides of the 0x40000000 prescale boundary.
    for value in 0..=131_072i32 {
        assert_eq!(
            isqrt_48efc0(value),
            oracle(value as u32),
            "isqrt_48efc0({value})"
        );
    }
    // Perfect squares and neighbors at magnitude boundaries, plus the
    // full i32 maximum the callers can feed it.
    let mut edges = vec![0x3fff_ffffi32, 0x4000_0000, 0x4000_0001, i32::MAX];
    for root in [0u32, 1, 2, 255, 256, 32767, 32768, 46340, 46341] {
        let square = root as u64 * root as u64;
        for delta in [-1i64, 0, 1] {
            let value = square as i64 + delta;
            if (0..=i64::from(i32::MAX)).contains(&value) {
                edges.push(value as i32);
            }
        }
    }
    // Deterministic xorshift sweep over the positive i32 range.
    let mut state = 0x243F_6A88u32;
    for _ in 0..20_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        edges.push((state & 0x7fff_ffff) as i32);
    }
    for value in edges {
        assert_eq!(
            isqrt_48efc0(value),
            oracle(value as u32),
            "isqrt_48efc0({value:#x})"
        );
    }
}

#[test]
fn spacing_sample_follows_the_00410f60_gates_and_folds() {
    let identity: [u16; 4096] = std::array::from_fn(|index| index as u16);
    // Zero gate: the fifth slot is dead, the shifted base returns as-is.
    assert_eq!(
        spacing_sample(0x1234_5678, 1, 2, 3, 0, 0x9999, 8, &identity),
        0x0012_3456
    );
    // Arithmetic shift, not logical: the high bit propagates.
    assert_eq!(
        spacing_sample(0x8765_4321, 0, 0, 0, 0, 0, 4, &identity),
        0xf876_5432
    );
    // Threshold gate: terrain height at or above the shifted base returns it.
    assert_eq!(
        spacing_sample(0x100, 0, 0, 0, 1, 0x100, 0, &identity),
        0x100
    );
    // Full path under the identity table: mirror+negate fold on the third
    // sample, combined scaling, clamped maximum selection.
    assert_eq!(
        spacing_sample(0x1000, 0x10, 0, 0, 1, 0x100, 0, &identity),
        0x1000
    );
}

#[test]
fn spacing_sample_selects_terrain_height_below_a_negative_scaled_value() {
    // Only entry 0 carries 0x800; every other fold reads zero. The first
    // two samples negate to -0x800 each, the third stays zero, and the
    // combined -128 scales the extent down past the terrain height.
    let mut table = [0u16; 4096];
    table[0] = 0x800;
    // wave_clock=0x100 drives the first fold to negated entry 0; Z=0x200 drives
    // the second there too while the third folds onto a zero entry.
    assert_eq!(
        spacing_sample(0x1_0000, 0, 0x200, 0x100, 1, 0xfffe, 0, &table),
        0xfffe
    );
    // Same geometry with a zero terrain height takes the clamped maximum instead.
    assert_eq!(
        spacing_sample(0x1_0000, 0, 0x200, 0x100, 1, 0, 0, &table),
        0xfefe
    );
}

#[test]
fn banked_actor1_angles_reproduce_ctx_basis() {
    // Retail acceptance (not behavior): the banked actor-1 yaw 562
    // (packed 0x04) and pitch 0xFDCE (packed 0x03) through the
    // executable sine table must reproduce the banked ctx basis
    // words bit-exact. Tracked driver
    // `runtime_re/scripts/level1-2d-rotate-oracle.ps1`, bundle
    // `.tmp/infestation-ttd/20260929-202157-164-Level1Oracle2DB/`
    // (angles), bundle
    // `.tmp/infestation-ttd/20260929-201450-899-Level1Oracle2DX/`
    // (basis). Four adjacent pitch values match (the lookup
    // ignores the low two angle bits); 0xFDCE is the banked one.
    let math = retail_math();
    let mut saved552 = NativeBasis::INITIAL;
    math.rotate_in_plane(&mut saved552, 562, RotationPlane::Xz);
    assert_eq!(&saved552.words[1..4], &[0x3F7FA1FF, 0x00000000, 0xBD5BC1B8]);
    let mut saved553 = NativeBasis::INITIAL;
    saved553.copy_oriented_from(saved552, 0);
    math.rotate_in_plane(&mut saved553, 0xFDCE, RotationPlane::Yz);
    assert_eq!(
        &saved553.words[4..10],
        &[0xBB3CA503, 0x3F7FA1FF, 0xBD5B7106, 0x3D5B7106, 0x3D5BC1B8, 0x3F7F4421,]
    );
}

#[test]
fn orient_from_forward_preserves_the_selected_authored_axis() {
    let forward = [0.0, 0.0, 2.0];
    let up = [0.0, 3.0, 0.0];
    let mut basis = NativeBasis::INITIAL;
    basis.words[0] = 87;
    orient_from_forward(&mut basis, forward, up, false);
    assert_eq!(basis.words[0], 0);
    assert_eq!(vector(&basis, 1), [1.0, 0.0, 0.0]);
    assert_eq!(vector(&basis, 4), [0.0, 2.0, 0.0]);
    assert_eq!(vector(&basis, 7), forward);

    orient_from_forward(&mut basis, forward, up, true);
    assert_eq!(vector(&basis, 4), up);
    assert_eq!(vector(&basis, 7), [0.0, 0.0, 3.0]);
}

#[test]
fn orient_from_forward_quantizes_low_words_and_handles_parallel_vectors() {
    let mut basis = NativeBasis::INITIAL;
    // 8*4096 becomes signed WORD -32768, reversing normalized X.
    orient_from_forward(&mut basis, [0.0, 0.0, 8.0], [0.0, 1.0, 0.0], false);
    assert_eq!(vector(&basis, 1), [-1.0, 0.0, 0.0]);
    assert_eq!(vector(&basis, 4), [0.0, -8.0, 0.0]);
    // A quantized zero cross uses the native (1,0,0) fallback.
    orient_from_forward(&mut basis, [0.0, 1.0, 0.0], [0.0, 1.0, 0.0], false);
    assert_eq!(vector(&basis, 1), [1.0, 0.0, 0.0]);
    assert_eq!(vector(&basis, 4), [0.0, 0.0, -1.0]);
}

/// 00490DC0 receipts from the actor model ray test (0047D281, Infest01):
/// the ray basis (ECX), the actor's basis and the composed result.
const COMPOSE_RECEIPTS: [[[u32; 10]; 3]; 6] = [
    [
        [
            0, 0x3d656cb6, 0, 0xbf7f991d, 0xbd7ae140, 0x3f7f8491, 0xbb613089, 0x3f7f1d7d,
            0x3d7b463d, 0x3d656ca7,
        ],
        [3, 0x3f800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0x3f800000],
        [
            4, 0x3d656cb6, 0xbd7ae140, 0x3f7f1d7d, 0, 0x3f7f8491, 0x3d7b463d, 0xbf7f991d,
            0xbb613089, 0x3d656ca7,
        ],
    ],
    [
        [
            0, 0x3df692b5, 0, 0xbf7e2348, 0xbcc8af3c, 0x3f7fec0c, 0xbb42b5fc, 0x3f7e0f47,
            0x3cca27af, 0x3df68c12,
        ],
        [3, 0x3f800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0x3f800000],
        [
            4, 0x3df692b5, 0xbcc8af3c, 0x3f7e0f47, 0, 0x3f7fec0c, 0x3cca27af, 0xbf7e2348,
            0xbb42b5fc, 0x3df68c12,
        ],
    ],
    [
        [
            0, 0x3e5ac5fe, 0, 0xbf7a16da, 0x39aac483, 0x3f800000, 0x38956263, 0x3f7a15d5,
            0xb9aecdbb, 0x3e5ad891,
        ],
        [3, 0x3f800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0x3f800000],
        [
            4, 0x3e5ac5fe, 0x39aac483, 0x3f7a15d5, 0, 0x3f800000, 0xb9aecdbb, 0xbf7a16da,
            0x38956263, 0x3e5ad891,
        ],
    ],
    [
        [
            0, 0x3e5ac5fe, 0, 0xbf7a16da, 0x39aac483, 0x3f800000, 0x38956263, 0x3f7a15d5,
            0xb9aecdbb, 0x3e5ad891,
        ],
        [3, 0xbf800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0xbf800000],
        [
            4, 0xbe5ac5fe, 0xb9aac483, 0xbf7a15d5, 0, 0x3f800000, 0xb9aecdbb, 0x3f7a16da,
            0xb8956263, 0xbe5ad891,
        ],
    ],
    [
        [
            0, 0x3e6ec1a5, 0, 0xbf78f1b7, 0xbc34a3c1, 0x3f7ffbca, 0xbb2d3f47, 0x3f78ee71,
            0x3c39c270, 0x3e6eaff4,
        ],
        [3, 0x3f800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0x3f800000],
        [
            4, 0x3e6ec1a5, 0xbc34a3c1, 0x3f78ee71, 0, 0x3f7ffbca, 0x3c39c270, 0xbf78f1b7,
            0xbb2d3f47, 0x3e6eaff4,
        ],
    ],
    [
        [
            0, 0x3e6ec1a5, 0, 0xbf78f1b7, 0xbc34a3c1, 0x3f7ffbca, 0xbb2d3f47, 0x3f78ee71,
            0x3c39c270, 0x3e6eaff4,
        ],
        [3, 0xbf800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0xbf800000],
        [
            4, 0xbe6ec1a5, 0x3c34a3c1, 0xbf78ee71, 0, 0x3f7ffbca, 0x3c39c270, 0x3f78f1b7,
            0x3b2d3f47, 0xbe6eaff4,
        ],
    ],
];

#[test]
fn compose_matches_the_actor_ray_receipts() {
    for (index, [ray, source, out]) in COMPOSE_RECEIPTS.into_iter().enumerate() {
        let composed = NativeBasis { words: ray }.compose(NativeBasis { words: source });
        assert_eq!(composed.words, out, "receipt {index}");
    }
}

#[test]
fn compose_keeps_the_source_counter_and_renormalizes_past_100() {
    let mut ray = NativeBasis::INITIAL;
    ray.words[1..].copy_from_slice(&COMPOSE_RECEIPTS[0][0][1..]);
    let mut source = NativeBasis::INITIAL;
    source.words[0] = 99;
    assert_eq!(ray.compose(source).words[0], 100);
    source.words[0] = 100;
    let composed = ray.compose(source);
    let mut expected = NativeBasis {
        words: COMPOSE_RECEIPTS[0][2],
    };
    renormalize(&mut expected);
    assert_eq!(composed, expected);
    assert_eq!(composed.words[0], 0);
    source.words[0] = u32::MAX;
    assert_eq!(ray.compose(source).words[0], 0);
}

/// 00491E80 against Infest01's and Infest02's three spawn saves (0046AF80
/// from the pads), and 004911F0 against Infest02's respawn, which rebuilds
/// the vehicle's basis from the saved angles (`savespawn` queries).
#[test]
fn packed_angles_match_the_recorded_spawn_saves() {
    let math = retail_math();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = crate::FrontendExecutable::parse(&bytes).unwrap();
    let inverse = crate::parse_frontend_inverse_sine_table(
        executable.data(),
        crate::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap()
    .values;
    for (words, packed) in [
        (
            [
                0xc, 0xbf7a659f, 0xbce2fe82, 0xbe52d4a6, 0xbb59b702, 0x3f7e3113, 0xbdf18ce1,
                0x3e54cc92, 0xbdeaf714, 0xbf78a822,
            ],
            0x3fb049dd,
        ),
        (
            [
                0x6, 0xbf7a3126, 0xbcf67c7a, 0xbe5661a8, 0xbbc97b2d, 0x3f7e502c, 0xbde99a39,
                0x3e587ba6, 0xbde1b1c3, 0xbf78972b,
            ],
            0x3fb049dd,
        ),
        (
            [
                0x5d, 0xbdf89197, 0xbd2c27ea, 0x3f7dcfc9, 0x3c9e5c27, 0x3f7f9bd7, 0x3d36d6fb,
                0xbf7df820, 0x3cc9c744, 0xbdf6bb5a,
            ],
            0x3f9feeec,
        ),
    ] {
        let basis = NativeBasis { words };
        assert_eq!(math.packed_angles(&basis, &inverse), packed);
    }
    let mut basis = NativeBasis {
        words: [
            0x3, 0xbeb5abee, 0xbf427707, 0xbf0b6797, 0x3f6eaf02, 0xbe7b6094, 0xbe87a1eb,
            0x3d8a6042, 0xbf1a1334, 0x3f4ba72b,
        ],
    };
    math.orient_from_packed(&mut basis, 0x3fb049dd);
    assert_eq!(
        basis.words,
        [
            0x6, 0xbf7a3126, 0xbcf67c7a, 0xbe5661a8, 0xbbc97b2d, 0x3f7e502c, 0xbde99a39,
            0x3e587ba6, 0xbde1b1c3, 0xbf78972b,
        ]
    );
}
