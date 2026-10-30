//! Native basis rotations 00491290/00491530/004917D0, normalization 00417DA0,
//! and the integer sine folds used by menus and the name-entry draw.
//!
//! The supported CRT selects 53-bit x87 arithmetic with nearest-even rounding.
//! f64 operations model retained registers; explicit f32 conversions model
//! memory stores. The original sine table is required, never regenerated.

/// Native counter followed by the three stored X/Y/Z float vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeBasis {
    pub words: [u32; 10],
}

impl NativeBasis {
    /// 0040D6A0 / 00439630: zero counter and identity vectors.
    pub const INITIAL: Self = Self {
        words: [0, 0x3f80_0000, 0, 0, 0, 0x3f80_0000, 0, 0, 0, 0x3f80_0000],
    };

    /// 00490520 copies oriented vectors and preserves the destination counter.
    /// Low bits six/seven are identity here, not the child's saved-basis mode.
    pub fn copy_oriented_from(&mut self, source: Self, orientation: u16) {
        let destinations = match orientation & 7 {
            1 => [1, 2, 0],
            2 => [2, 0, 1],
            3 => [2, 1, 0],
            4 => [1, 0, 2],
            5 => [0, 2, 1],
            _ => [0, 1, 2],
        };
        for (source_axis, destination_axis) in destinations.into_iter().enumerate() {
            for component in 0..3 {
                let word = source.words[1 + source_axis * 3 + component];
                self.words[1 + destination_axis * 3 + component] =
                    if orientation & (0x20 >> source_axis) == 0 {
                        word
                    } else {
                        (-f32::from_bits(word)).to_bits()
                    };
            }
        }
    }

    /// 004909E0: signed WORD transform, (X product + Y product) + Z,
    /// nearest-even QWORD FISTP and independent low WORD output stores.
    /// This differs from the DWORD transform_point accumulation order.
    pub fn transform_words(self, local: [i16; 3]) -> [u16; 3] {
        let [x, y, z] = local.map(f64::from);
        std::array::from_fn(|axis| {
            let coefficient = |first| f64::from(f32::from_bits(self.words[first + axis]));
            let integral =
                ((x * coefficient(1) + y * coefficient(4)) + z * coefficient(7)).round_ties_even();
            if integral.is_finite()
                && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&integral)
            {
                integral as i64 as u16
            } else {
                0
            }
        })
    }

    /// 00490900: (Z product + Y product) + X product in retained precision,
    /// then nearest-even FISTP QWORD and low DWORD. Origin addition is separate.
    pub fn transform_point(self, local: [i32; 3]) -> [i32; 3] {
        std::array::from_fn(|component| {
            let coefficient = |first| f64::from(f32::from_bits(self.words[first + component]));
            let [x, y, z] = local.map(f64::from);
            let value = (z * coefficient(7) + y * coefficient(4)) + x * coefficient(1);
            let integral = value.round_ties_even();
            const BOUND: f64 = 9_223_372_036_854_775_808.0;
            if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
                integral as i64 as i32
            } else {
                0 // masked x87 integer-indefinite has low DWORD zero
            }
        })
    }

    /// 00490DC0: `source`'s rows taken into this basis's frame. Each row
    /// (00490710) is dotted with each of this basis's rows (004921B0,
    /// 004921E0, 00492210), ordered (Z product + Y product) + X product at
    /// 53 bits and stored as f32. The counter is the source's plus one; past
    /// 100 the result renormalizes (00490E1D..00490FB1, the same steps as
    /// [`renormalize`]).
    pub fn compose(self, source: Self) -> Self {
        let mut composed = Self { words: [0; 10] };
        for row in 0..3 {
            let [x, y, z] = std::array::from_fn(|component| {
                f64::from(f32::from_bits(source.words[1 + row * 3 + component]))
            });
            for axis in 0..3 {
                let coefficient =
                    |component| f64::from(f32::from_bits(self.words[1 + axis * 3 + component]));
                let value = (z * coefficient(2) + y * coefficient(1)) + x * coefficient(0);
                composed.words[1 + row * 3 + axis] = (value as f32).to_bits();
            }
        }
        composed.words[0] = source.words[0].wrapping_add(1);
        if composed.words[0] > 100 {
            renormalize(&mut composed);
        }
        composed
    }

    /// 00490BC0: `source`'s rows taken out of this basis's frame. Each row
    /// (00490640 over 00492120, 00492150 and 00492180) is this basis's rows
    /// weighted by the row's components, ordered (Z product + Y product) +
    /// X product at 53 bits and stored as f32. The counter is the source's plus one; past
    /// 100 the result renormalizes (00490C1D..00490DB1, the same steps as
    /// [`renormalize`]).
    pub fn expand(self, source: Self) -> Self {
        let mut expanded = Self { words: [0; 10] };
        for row in 0..3 {
            let [x, y, z] = std::array::from_fn(|component| {
                f64::from(f32::from_bits(source.words[1 + row * 3 + component]))
            });
            for axis in 0..3 {
                let coefficient =
                    |first: usize| f64::from(f32::from_bits(self.words[first + axis]));
                let value = (z * coefficient(7) + y * coefficient(4)) + x * coefficient(1);
                expanded.words[1 + row * 3 + axis] = (value as f32).to_bits();
            }
        }
        expanded.words[0] = source.words[0].wrapping_add(1);
        if expanded.words[0] > 100 {
            renormalize(&mut expanded);
        }
        expanded
    }

    /// 00490A80: dot each stored row with signed WORD XYZ, ordered X/Y/Z,
    /// then nearest-even QWORD FISTP and low WORD extraction.
    pub fn inverse_words(self, world: [i16; 3]) -> [i16; 3] {
        let [x, y, z] = world.map(f64::from);
        [1_usize, 4, 7].map(|first| {
            let coefficient = |offset| f64::from(f32::from_bits(self.words[first + offset]));
            let integral =
                ((x * coefficient(0) + y * coefficient(1)) + z * coefficient(2)).round_ties_even();
            const BOUND: f64 = 9_223_372_036_854_775_808.0;
            if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
                integral as i64 as i16
            } else {
                0 // Masked integer-indefinite has zero low WORD.
            }
        })
    }
}

/// 00528CE0, as loaded by FMUL m32. This is not 1/32768.
const TRIG_SCALE: f32 = f32::from_bits(0x3800_0100);
/// 00528CE8, the double 65536 / 2pi that turns radians into WORD angles.
const RADIANS_TO_WORD: f64 = f64::from_bits(0x40c4_5f30_6dcc_48b9);
/// 005205B0: quantization before signed-word vector normalization.
const NORMALIZE_SCALE: f32 = f32::from_bits(0x4580_0000);

/// Native local-vector pairs, including their original signed rotation policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotationPlane {
    /// 00491290: Y = sin*Z + cos*Y; Z = cos*Z - sin*Y.
    Yz,
    /// 00491530: X = cos*X - sin*Z; Z = sin*X + cos*Z.
    Xz,
    /// 004917D0: X = sin*Y + cos*X; Y = cos*Y - sin*X.
    Xy,
    /// 00491A70: about world Y rather than a local axis. Within each row,
    /// x = cos*x - sin*z; z = sin*x + cos*z.
    WorldY,
}

/// Original integer sine path used by circular menu rows and model1B effects.
/// This preserves the replicated WORD and Q31 quadrant policy; basis rotations
/// use a distinct floating-point conversion and must not substitute for it.
pub fn sine_q31(angle: i32, table: &[u16; 4096]) -> i32 {
    let sample = u32::from(quarter_sample(angle, table));
    let value = (sample | (sample << 16)) as i32;
    if angle & 0x8000 != 0 {
        value.wrapping_neg()
    } else {
        value
    }
}

/// 00491D10 rotates one stored float vector around the supplied float axis.
/// It neither normalizes the axis nor updates a basis counter. Explicit f32
/// stores preserve the parallel/orthogonal/cross spills under FPCW027F.
pub fn rotate_axis_vector(
    vector: [f32; 3],
    axis: [f32; 3],
    angle: i32,
    table: &[u16; 4096],
) -> [f32; 3] {
    const Q31_SCALE: f64 = 1.0 / 2_147_483_648.0; // float00528CE4
    let cosine = f64::from(sine_q31(angle.wrapping_add(0x4000), table)) * Q31_SCALE;
    let sine = f64::from(sine_q31(angle, table)) * Q31_SCALE;
    let [ax, ay, az] = axis.map(f64::from);
    let [vx, vy, vz] = vector.map(f64::from);
    let dot = (ax * vx + az * vz) + ay * vy;
    let parallel = [ax, ay, az].map(|value| (dot * value) as f32);
    let orthogonal: [f32; 3] =
        std::array::from_fn(|i| (f64::from(vector[i]) - f64::from(parallel[i])) as f32);
    let [ox, oy, oz] = orthogonal.map(f64::from);
    //00491D79 stores Z without popping it; cross-X uses the retained
    //subtraction, cross-Y reloads stored Z. Cross-Z remains unspilled.
    let retained_oz = vz - f64::from(parallel[2]);
    let cross = [
        f64::from((retained_oz * ay - oy * az) as f32),
        f64::from((ox * az - oz * ax) as f32),
        oy * ax - ox * ay,
    ];
    std::array::from_fn(|i| {
        ((sine * cross[i] + cosine * f64::from(orthogonal[i])) + f64::from(parallel[i])) as f32
    })
}

/// Plain integer sine of the name-entry draw (0045948E, 00459909..00459986):
/// the zero-extended table WORD, negated in the upper half-turn. It is not
/// replicated into the high half like [`sine_q31`].
pub fn sine_q15(angle: i32, table: &[u16; 4096]) -> i32 {
    let sample = i32::from(quarter_sample(angle, table));
    if angle & 0x8000 != 0 {
        -sample
    } else {
        sample
    }
}

/// 0048F4D0: signed input, saturated absolute magnitude, then five-bit index
/// reduction into the original signed inverse-sine WORDs at 0053FF00.
pub fn inverse_sine_q15(value: i32, table: &[i16; 1024]) -> i32 {
    let index = (value.unsigned_abs().min(32767) >> 5) as usize;
    let angle = i32::from(table[index]);
    if value < 0 {
        -angle
    } else {
        angle
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InverseSineQ31Error {
    #[error("Q31 inverse sine input {input:#010x} reads index {index} outside the supplied 1024-WORD table")]
    OutOfTable { input: i32, index: i32 },
}

/// 0048F520: wrapping NEG for a negative Q31 input, then arithmetic SAR21
/// and sign extension of the original inverse-sine WORD. Unlike 0048F4D0,
/// this consumer does not saturate or use the Q15 five-bit index policy.
///
/// i32::MIN retains its sign after NEG and reads index -1024, before this
/// supplied table. Reject that bounded-backing request, without claiming a
/// native memory fault. The actual 0048EF30 ratio consumer never emits MIN.
pub fn inverse_sine_q31(value: i32, table: &[i16; 1024]) -> Result<i32, InverseSineQ31Error> {
    let magnitude = if value < 0 {
        value.wrapping_neg()
    } else {
        value
    };
    let index = magnitude >> 21;
    let word = table
        .get(index as usize)
        .ok_or(InverseSineQ31Error::OutOfTable {
            input: value,
            index,
        })?;
    let angle = i32::from(*word);
    Ok(if value < 0 { -angle } else { angle })
}

/// Shared quadrant fold: the low two angle bits and all bits above bit 15 are
/// ignored, and the second quarter mirrors the table.
fn quarter_sample(angle: i32, table: &[u16; 4096]) -> u16 {
    let mut quarter = angle as u32 & 0x3ffc;
    if angle & 0x4000 != 0 {
        quarter = 0x3ffc - quarter;
    }
    table[(quarter >> 2) as usize]
}

/// Three signed WORD arguments convert exactly to f32; Z*Z + Y*Y + X*X
/// accumulates at 53 bits before FSQRT. The actual consumer selects retained
/// or spilled length, and a zero length yields (1, 0, 0).
pub fn normalize_words(words: [i16; 3], precision: NativeLengthPrecision) -> [f32; 3] {
    normalize_integral(words, precision)
}

/// 00490370: copy the supplied forward/up vectors into Z/Y, rebuild and
/// normalize X = Y cross Z, then rebuild either Z = X cross Y (nonzero native
/// argument) or Y = Z cross X (zero). The supplied vectors are not normalized.
/// X quantization uses nearest-even FISTP and its length is spilled to f32.
pub fn orient_from_forward(
    basis: &mut NativeBasis,
    forward: [f32; 3],
    up: [f32; 3],
    rebuild_forward: bool,
) {
    store_vector(basis, 7, forward);
    store_vector(basis, 4, up);
    let x = normalize_quantized(cross_stored(up, forward), NativeLengthPrecision::Spilled);
    store_vector(basis, 1, x);
    if rebuild_forward {
        store_vector(basis, 7, cross_stored(x, up));
    } else {
        store_vector(basis, 4, cross_stored(forward, x));
    }
    basis.words[0] = 0;
}

#[derive(Clone, Debug)]
pub struct NativeBasisMath {
    sine_quarter: std::sync::Arc<[u16; 4096]>,
}

impl NativeBasisMath {
    pub fn new(sine_quarter: [u16; 4096]) -> Self {
        Self::shared(std::sync::Arc::new(sine_quarter))
    }

    /// [`Self::new`] over a table already shared, without copying it.
    pub fn shared(sine_quarter: std::sync::Arc<[u16; 4096]>) -> Self {
        Self { sine_quarter }
    }

    /// Mutates the counter and persistent basis. A 16-bit turn has 65536 units;
    /// the lookup ignores the low two angle bits and all bits above bit 15.
    /// Renormalization runs after incremented unsigned counter >100, including
    /// on a zero-angle call. Counter wrap to zero bypasses normalization.
    pub fn rotate(&self, basis: &mut NativeBasis, angle: i32) {
        self.rotate_in_plane(basis, angle, RotationPlane::Xz);
    }

    pub fn rotate_in_plane(&self, basis: &mut NativeBasis, angle: i32, plane: RotationPlane) {
        let cosine = self.sine(angle.wrapping_add(0x4000));
        let sine = self.sine(angle);
        basis.words[0] = basis.words[0].wrapping_add(1);

        if plane == RotationPlane::WorldY {
            // Each row reads its x and z before either f32 store.
            for row in [1, 4, 7] {
                let x = f64::from(f32::from_bits(basis.words[row]));
                let z = f64::from(f32::from_bits(basis.words[row + 2]));
                basis.words[row] = ((cosine * x - sine * z) as f32).to_bits();
                basis.words[row + 2] = ((sine * x + cosine * z) as f32).to_bits();
            }
            if basis.words[0] > 100 {
                renormalize(basis);
            }
            return;
        }
        // Each component reads both old coordinates before either f32 store.
        let (first, second) = match plane {
            RotationPlane::Yz => (4, 7),
            RotationPlane::Xz => (1, 7),
            RotationPlane::Xy => (1, 4),
            RotationPlane::WorldY => unreachable!("handled above"),
        };
        for component in 0..3 {
            let a = f64::from(f32::from_bits(basis.words[first + component]));
            let b = f64::from(f32::from_bits(basis.words[second + component]));
            let (next_a, next_b) = match plane {
                RotationPlane::Xz => (cosine * a - sine * b, sine * a + cosine * b),
                RotationPlane::Yz | RotationPlane::Xy => {
                    (sine * b + cosine * a, cosine * b - sine * a)
                }
                RotationPlane::WorldY => unreachable!("handled above"),
            };
            basis.words[first + component] = (next_a as f32).to_bits();
            basis.words[second + component] = (next_b as f32).to_bits();
        }
        if basis.words[0] > 100 {
            renormalize(basis);
        }
    }

    fn sine(&self, angle: i32) -> f64 {
        // FILD retains the integer; FMUL uses the original single-precision
        // constant without rounding its product to f32 before the matrix work.
        f64::from(sine_q15(angle, &self.sine_quarter)) * f64::from(TRIG_SCALE)
    }

    /// [`sine_q15`] over this table.
    pub fn sine_q15(&self, angle: i32) -> i32 {
        sine_q15(angle, &self.sine_quarter)
    }

    /// 004911F0: the world axes 0055F290, 0055F280 and 0055F270 (the
    /// CRT-initialized identity) replace the rows, keeping the counter. Then
    /// 00491530 turns by `(packed & 3FF) << 6`, 00491290 by
    /// `(packed >> 4) & FFC0` and 004917D0 by `(packed >> 14) & FFC0`.
    pub fn orient_from_packed(&self, basis: &mut NativeBasis, packed: u32) {
        let counter = basis.words[0];
        *basis = NativeBasis::INITIAL;
        basis.words[0] = counter;
        self.rotate_in_plane(basis, ((packed & 0x3ff) << 6) as i32, RotationPlane::Xz);
        self.rotate_in_plane(basis, ((packed >> 4) & 0xffc0) as i32, RotationPlane::Yz);
        self.rotate_in_plane(basis, ((packed >> 14) & 0xffc0) as i32, RotationPlane::Xy);
    }

    /// 00491E80: the basis's three angles in the 10-bit fields
    /// [`orient_from_packed`](Self::orient_from_packed) reads. The first is
    /// the CRT `asin` (005128A0) of Z.x over Z's horizontal length (an f32
    /// spill), placed in its half-turn by the sign of Z.z; the second is the
    /// `asin` of -Z.y over that length, and outside asin's domain (past 45
    /// degrees) its NaN becomes zero; the third is the inverse sine table
    /// (0048F4D0) of the rows' Y.x/Y.z lift by the first angle, placed by
    /// the X row's.
    pub fn packed_angles(&self, basis: &NativeBasis, inverse_sine: &[i16; 1024]) -> u32 {
        let row = |index: usize| f64::from(f32::from_bits(basis.words[index]));
        let (z_x, z_y, z_z) = (row(7), row(8), row(9));
        let root = (z_z * z_z + z_x * z_x).sqrt();
        let horizontal = f64::from(root as f32);
        let mut yaw = 0;
        if root != 0.0 {
            yaw = fistp_low(crt_asin(z_x / horizontal) * RADIANS_TO_WORD);
            yaw = if yaw > 0 {
                if z_z < 0.0 {
                    0x8000 - yaw
                } else {
                    yaw
                }
            } else if z_z <= 0.0 {
                0x8000 - yaw
            } else {
                yaw + 0x10000
            };
        }
        let pitch = if horizontal == 0.0 {
            if z_y < 0.0 {
                0x4000
            } else {
                0xc000
            }
        } else {
            let pitch = fistp_low(crt_asin(-(z_y / horizontal)) * RADIANS_TO_WORD);
            if pitch < 0 {
                pitch + 0x10000
            } else {
                pitch
            }
        };
        let sine = f64::from(sine_q15(yaw, &self.sine_quarter));
        let cosine = f64::from(-sine_q15(yaw.wrapping_add(0x4000), &self.sine_quarter));
        let across = fistp_low(sine * row(3) + cosine * row(1));
        let lift = fistp_low(sine * row(6) + cosine * row(4)).clamp(-0x7fff, 0x7fff);
        let roll = inverse_sine_q15(lift, inverse_sine);
        let roll = if roll > 0 {
            if across > 0 {
                0x8000 - roll
            } else {
                roll
            }
        } else if across > 0 {
            0x8000 - roll
        } else {
            roll + 0x10000
        };
        ((roll as u32 & 0xffc0) << 14)
            | ((pitch as u32 & 0xffc0) << 4)
            | ((yaw >> 6) as u32 & 0x3ff)
    }
}

/// The CRT's `_CIasin` (005128A0): `fpatan` of x over sqrt((1+x)(1-x)),
/// NaN outside [-1, 1] after its domain error.
fn crt_asin(value: f64) -> f64 {
    value.asin()
}

/// FISTP QWORD at nearest-even, read back as the low DWORD. Masked invalid
/// conversion stores the integer indefinite, whose low DWORD is zero.
fn fistp_low(value: f64) -> i32 {
    let integral = value.round_ties_even();
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
        integral as i64 as i32
    } else {
        0
    }
}

fn vector(basis: &NativeBasis, first: usize) -> [f32; 3] {
    std::array::from_fn(|component| f32::from_bits(basis.words[first + component]))
}

fn store_vector(basis: &mut NativeBasis, first: usize, vector: [f32; 3]) {
    for (component, value) in vector.into_iter().enumerate() {
        basis.words[first + component] = value.to_bits();
    }
}

pub fn renormalize(basis: &mut NativeBasis) {
    // 00491627..004916A9: quantize/normalize Z, then Y, through 00417DA0.
    let z = normalize_quantized(vector(basis, 7), NativeLengthPrecision::Retained);
    store_vector(basis, 7, z);
    let y = normalize_quantized(vector(basis, 4), NativeLengthPrecision::Retained);
    store_vector(basis, 4, y);

    // 004916A9..0049178D: rebuild X = Y cross Z, with each component stored
    // before re-quantization. This inline normalization SPILLS its length;
    // `00492060` is the same steps as a function, which 0048FB40's copy of
    // this renormalization calls for its X row.
    let cross_x = cross_stored(y, z);
    store_vector(basis, 1, cross_x);
    let x = normalize_quantized(cross_x, NativeLengthPrecision::Spilled);
    store_vector(basis, 1, x);

    // 0049178D..004917C5: rebuild Y = Z cross X without another normalization.
    store_vector(basis, 4, cross_stored(z, x));
    basis.words[0] = 0;
}

/// Products and subtraction are retained at 53 bits, then each coordinate
/// becomes a single-precision memory word. Do not replace with f32 arithmetic
/// or fused multiply-add. Native stores Y, Z, X; the operands do not alias.
fn cross_stored(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    let [lx, ly, lz] = left.map(f64::from);
    let [rx, ry, rz] = right.map(f64::from);
    let x = ly * rz - ry * lz;
    let y = lz * rx - lx * rz;
    let z = ry * lx - ly * rx;
    [x as f32, y as f32, z as f32]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLengthPrecision {
    /// 00417DA0 keeps FSQRT in a register for all three divisions.
    Retained,
    /// 0049174E and 0048322B/004832DF store FSQRT as f32 and use that
    /// memory divisor for each division.
    Spilled,
}

fn normalize_quantized(vector: [f32; 3], precision: NativeLengthPrecision) -> [f32; 3] {
    normalize_integral(vector.map(quantize_word), precision)
}

fn normalize_integral(words: [i16; 3], precision: NativeLengthPrecision) -> [f32; 3] {
    let [x, y, z] = words.map(f64::from);
    // Signed words convert exactly to both f32 and f64. The assembly's
    // Z*Z + Y*Y + X*X ordering is retained before FSQRT.
    let length = ((z * z + y * y) + x * x).sqrt();
    if length == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let divisor = match precision {
        NativeLengthPrecision::Retained => length,
        NativeLengthPrecision::Spilled => f64::from(length as f32),
    };
    [
        (x / divisor) as f32,
        (y / divisor) as f32,
        (z / divisor) as f32,
    ]
}

/// Direct FISTP QWORD, then MOVSX of the low WORD. This is nearest-even,
/// unlike __ftol's truncation. Masked invalid conversion yields x87 integer
/// indefinite (i64::MIN), whose low signed word is zero. It does not saturate.
fn quantize_word(value: f32) -> i16 {
    let rounded = (f64::from(value) * f64::from(NORMALIZE_SCALE)).round_ties_even();
    const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    if !rounded.is_finite() || !(-I64_UPPER_EXCLUSIVE..I64_UPPER_EXCLUSIVE).contains(&rounded) {
        return 0;
    }
    rounded as i64 as i16
}

/// `0048EF30(a, b)`: saturated Q31 divide. Computes `(|a| << 31) / |b|`
/// by restoring division with wrapping multiplies, saturating to
/// `0x7FFFFFFF` (same input signs) or `0x80000001` (differing signs)
/// when `|b| <= |a|`, and negating the quotient for differing input
/// signs. Pure integer arithmetic; feeds terrain slope factors among
/// other shared consumers.
pub fn q31_divide(a: u32, b: u32) -> u32 {
    fn abs(value: u32) -> u32 {
        if (value as i32) < 0 {
            (value as i32).wrapping_neg() as u32
        } else {
            value
        }
    }
    let signs = a ^ b;
    let mut numerator = abs(a);
    let denominator = abs(b);
    if denominator <= numerator {
        return ((((signs as i32) >= 0) as u32).wrapping_sub(1) & 2).wrapping_add(0x7fff_ffff);
    }
    let mut unit = 0x8000_0000u32;
    let mut quotient = 0u32;
    loop {
        numerator = numerator.wrapping_mul(2);
        unit >>= 1;
        if denominator <= numerator {
            numerator = numerator.wrapping_sub(denominator);
            quotient |= unit;
        }
        if numerator == 0 || unit == 0 {
            break;
        }
    }
    if (signs as i32) < 0 {
        quotient.wrapping_neg()
    } else {
        quotient
    }
}

/// `0048F240(x, y)`: the length of a vector by a digit-by-digit root. Both
/// components shift right (arithmetic) far enough that the larger
/// magnitude's square fits, the root is taken over the shifted squares and
/// shifted back; a non-positive sum of squares is 0.
pub fn length_48f240(x: i32, y: i32) -> i32 {
    let magnitude = |value: i32| {
        if value > 0 {
            value
        } else {
            value.wrapping_neg()
        }
    };
    let mut probe = magnitude(x) | magnitude(y);
    let mut shift = 0u32;
    for (limit, step) in [
        (0x3441_0000, 16),
        (0x34_4100, 8),
        (0x3_4410, 4),
        (0xd104, 2),
    ] {
        if probe > limit {
            shift += step;
            probe >>= step;
        }
    }
    if probe > 0x6882 {
        shift += 1;
    }
    let (x, y) = (x >> shift, y >> shift);
    let sum = y.wrapping_mul(y).wrapping_add(x.wrapping_mul(x));
    if sum <= 0 {
        return 0;
    }
    let mut root = 0x8000u32;
    let mut square = 0x4000_0000i32;
    let mut count = 15i32;
    if sum < 0x4000_0000 {
        loop {
            root >>= 1;
            square >>= 2;
            count -= 1;
            if square <= sum {
                break;
            }
        }
    }
    let mut step = root >> 1;
    count -= 1;
    let (mut high, mut low) = (count + 1, count.wrapping_mul(2));
    while step != 0 {
        let term = (root << (high as u32 & 31)).wrapping_add(1u32 << (low as u32 & 31)) as i32;
        if square <= sum.wrapping_sub(term) {
            root = root.wrapping_add(step);
            square = square.wrapping_add(term);
        }
        high -= 1;
        step >>= 1;
        low -= 2;
    }
    (root << (shift & 31)) as i32
}

/// `0048F540(x, y)`: the WORD angle of (x, y) from +x toward +y: the
/// inverse sine of `y` over the length ([`length_48f240`], 0 when not
/// positive), mirrored for a negative `x` and kept within +/-0x8000.
pub fn angle_48f540(x: i32, y: i32, table: &[i16; 1024]) -> Result<i32, InverseSineQ31Error> {
    let length = length_48f240(x, y);
    if length <= 0 {
        return Ok(0);
    }
    let angle = inverse_sine_q31(q31_divide(y as u32, length as u32) as i32, table)?;
    if x >= 0 {
        return Ok(angle);
    }
    let mirrored = 0x8000 - angle;
    Ok(if mirrored > 0x8000 {
        mirrored - 0x10000
    } else {
        mirrored
    })
}

/// `0048EFC0(value)`: integer square root (floor) by binary digit
/// recurrence. Returns 0 for non-positive inputs. The 07 edge/line
/// expander feeds it the squared edge length (`dx*dx + dy*dy`) and
/// normalizes the direction through the result plus one. All
/// arithmetic wraps like the original; the byte counters wrap mod
/// 256 with the `& 0x1f` shift masks applied exactly as listed.
pub fn isqrt_48efc0(value: i32) -> u32 {
    if value <= 0 {
        return 0;
    }
    let mut scale_bits = 0x0fu8;
    let mut root = 0x8000u32;
    let mut remainder = 0x4000_0000i32;
    if value < 0x4000_0000 {
        while value < remainder {
            root >>= 1;
            remainder >>= 2;
            scale_bits = scale_bits.wrapping_sub(1);
        }
    }
    let mut bit = root >> 1;
    if bit != 0 {
        let mut trial_bits = scale_bits.wrapping_sub(1).wrapping_mul(2);
        while {
            let trial = root
                .wrapping_shl(u32::from(scale_bits & 0x1f))
                .wrapping_add(1u32.wrapping_shl(u32::from(trial_bits & 0x1f)))
                as i32;
            if remainder <= value.wrapping_sub(trial) {
                root += bit;
                remainder = remainder.wrapping_add(trial);
            }
            bit >>= 1;
            scale_bits = scale_bits.wrapping_sub(1);
            trial_bits = trial_bits.wrapping_sub(2);
            bit != 0
        } {}
    }
    root
}

/// Sine-table fold shared by `00410F60`'s three samples: mirror on bit
/// 14, table lookup, negate on bit 15. Parallel to [`quarter_sample`]
/// with the reflected+negated policy this consumer requires.
fn folded_sample(value: u32, table: &[u16; 4096]) -> u32 {
    let mut index = value & 0x3ffc;
    if value & 0x4000 != 0 {
        index = 0x3ffc - index;
    }
    let mut sample = table[(index >> 2) as usize] as u32;
    if value & 0x8000 != 0 {
        sample = sample.wrapping_neg();
    }
    sample
}

/// `00410F60`: sine-table water sample over an H12 entry DWORD. The five
/// stack arguments are X, Z, process wave clock, world wave gate, and
/// signed terrain height.0044FFA0 pushes that fifth argument at4500C3
/// and45016D; it is never an unspecified caller-stack word. `shift` is the
/// load-time `005483BC` policy and `table` the original `0053DF00`
/// sine table. All arithmetic wraps like the original; the opening
/// shift is arithmetic (`sar`).
pub fn spacing_sample(
    base: u32,
    x: u32,
    z: u32,
    wave_clock: u32,
    wave_gate: u32,
    terrain_height: u32,
    shift: u32,
    table: &[u16; 4096],
) -> u32 {
    let result = ((base as i32) >> (((shift as u8) & 0x1f) as i32)) as u32;
    if wave_gate != 0 && (terrain_height as i32) < (result as i32) {
        let narrow = x & 0xffff;
        let first = folded_sample(
            wave_clock
                .wrapping_mul(0x40)
                .wrapping_add(narrow.wrapping_mul(0xc))
                .wrapping_mul(2),
            table,
        );
        let second = folded_sample(
            (z & 0xffff)
                .wrapping_add(narrow)
                .wrapping_mul(0x20)
                .wrapping_add(wave_clock.wrapping_mul(0x140)),
            table,
        );
        let third = folded_sample(
            wave_clock
                .wrapping_mul(0x40)
                .wrapping_add((z & 0xffff).wrapping_sub(narrow).wrapping_mul(6))
                .wrapping_mul(8),
            table,
        );
        let low = (third >> 6) as i32;
        let high = ((first.wrapping_add(second)) as i32 >> 5) as i16 as i32;
        let combined = (low.wrapping_add(high)) as i16;
        let scaled = (combined as i32 as u32)
            .wrapping_mul(result.wrapping_sub(terrain_height).wrapping_add(0x200));
        let value = (((scaled as i32) >> 0xf) as i16) as i32 as u32;
        let value = value.wrapping_add(result);
        if (terrain_height as i32) <= (value as i32) {
            return value & 0xffff;
        }
        return terrain_height;
    }
    result
}

#[cfg(test)]
#[path = "native_basis_tests.rs"]
mod tests;
