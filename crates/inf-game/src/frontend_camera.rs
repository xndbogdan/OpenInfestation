//! Frontend camera consumers 00483750/004837B0/00484AC0/00484B60.
//!
//! Positions and velocities are native signed words, not floating world units.
//! The spring preserves wrapping integer products, Q31 extraction, and the
//! original x87 single-precision spills. CRT initialization at 00513FE1 selects
//! 53-bit arithmetic precision; f64 models retained registers at that precision.

use inf_formats::FrontendCameraPreset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendEntry {
    First,
    Returning,
}

/// 00484E00 selects these policies from global 0055F14D and screen +11C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMotion {
    FirstEntry,
    Returning,
    Accelerated,
}

impl CameraMotion {
    fn speeds(self, scale: i32) -> [i32; 2] {
        let [eye, look_at]: [i32; 2] = match self {
            Self::FirstEntry => [15, 12],
            Self::Returning => [40, 25],
            Self::Accelerated => [200, 140],
        };
        [scale.wrapping_mul(eye), scale.wrapping_mul(look_at)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraSpring {
    pub position: [i32; 3],
    pub target: [i32; 3],
    pub velocity: [i32; 3],
}

impl CameraSpring {
    fn advance(&mut self, speed: i32, delta_micros: i32) {
        let time_q31 = delta_micros.wrapping_shl(11);
        let difference =
            std::array::from_fn(|axis| self.target[axis].wrapping_sub(self.position[axis]));
        // 0048F4C0 computes a length, then 0048EFC0 takes its square root.
        let desired_magnitude = integer_sqrt(vector_length(difference)).wrapping_mul(speed);
        let desired = normalized_scaled(difference, desired_magnitude);
        let mut change =
            std::array::from_fn(|axis| desired[axis].wrapping_sub(self.velocity[axis]));
        let limit = multiply_q31(speed.wrapping_mul(speed), time_q31);
        if vector_length(change) > limit {
            change = normalized_scaled(change, limit);
        }
        for (axis, change) in change.into_iter().enumerate() {
            self.velocity[axis] = self.velocity[axis].wrapping_add(change);
            let step = multiply_q31(time_q31, self.velocity[axis]);
            let remaining = self.position[axis].wrapping_sub(self.target[axis]);
            // Signed comparisons of wrapping abs are intentional, including
            // the exceptional i32::MIN result and strict overshoot inequality.
            self.position[axis] = if remaining.wrapping_abs() < step.wrapping_abs() {
                self.target[axis]
            } else {
                self.position[axis].wrapping_add(step)
            };
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresetSelection {
    /// 00484AC0 writes screen +120 = 1 only for preset index 9. Other selections
    /// leave that timer untouched; ownership and subsequent aging are external.
    pub restart_timer_0120: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendCamera {
    eye: CameraSpring,
    look_at: CameraSpring,
    selected_preset: u32,
}

impl FrontendCamera {
    /// 004837B0's initial pose, followed by preset 0. Returning construction
    /// copies target coordinates into the current pose at 004838F3..00483926.
    pub fn new(
        preset_zero: &FrontendCameraPreset,
        position_scale: i32,
        entry: FrontendEntry,
    ) -> Self {
        let mut camera = Self {
            eye: CameraSpring {
                position: [
                    (-1200i32).wrapping_mul(position_scale),
                    0,
                    7000i32.wrapping_mul(position_scale),
                ],
                target: [0; 3],
                velocity: [0; 3],
            },
            look_at: CameraSpring {
                position: [
                    2000i32.wrapping_mul(position_scale),
                    0,
                    5000i32.wrapping_mul(position_scale),
                ],
                target: [0; 3],
                velocity: [0; 3],
            },
            selected_preset: 0,
        };
        camera.select_preset(0, preset_zero, position_scale);
        if entry == FrontendEntry::Returning {
            camera.eye.position = camera.eye.target;
            camera.look_at.position = camera.look_at.target;
        }
        camera
    }

    pub fn eye(&self) -> &CameraSpring {
        &self.eye
    }
    pub fn look_at(&self) -> &CameraSpring {
        &self.look_at
    }
    pub fn selected_preset(&self) -> u32 {
        self.selected_preset
    }

    /// The caller resolves/bounds-checks the authored table index. Selection
    /// changes targets without resetting current positions or either velocity.
    pub fn select_preset(
        &mut self,
        index: u32,
        preset: &FrontendCameraPreset,
        position_scale: i32,
    ) -> PresetSelection {
        self.eye.target = preset.eye.map(|value| value.wrapping_mul(position_scale));
        self.look_at.target = preset
            .target
            .map(|value| value.wrapping_mul(position_scale));
        self.selected_preset = index;
        PresetSelection {
            restart_timer_0120: index == 9,
        }
    }

    /// 00483750 gates on eye position only, strictly below 100 on each axis.
    /// Look-at convergence and residual velocity do not participate. Frontend
    /// tick checks this BEFORE advancing the camera at the end of that tick.
    pub fn ready(&self) -> bool {
        self.eye
            .target
            .into_iter()
            .zip(self.eye.position)
            .all(|(target, current)| target.wrapping_sub(current).wrapping_abs() < 100)
    }

    pub fn advance(&mut self, delta_micros: i32, motion: CameraMotion, speed_scale: i32) {
        let [eye_speed, look_speed] = motion.speeds(speed_scale);
        self.eye.advance(eye_speed, delta_micros);
        self.look_at.advance(look_speed, delta_micros);
    }
}

/// Signed product followed by SHL low/RCL high: bits 31..62, not division with
/// truncation toward zero and not a saturating fixed-point conversion.
fn multiply_q31(left: i32, right: i32) -> i32 {
    ((i64::from(left) * i64::from(right)) >> 31) as i32
}

/// 0048EFC0's integer square root. Nonpositive signed inputs return zero.
pub(crate) fn integer_sqrt(value: i32) -> i32 {
    if value <= 0 {
        return 0;
    }
    let mut root = 0u32;
    let mut bit = 1u32 << 15;
    while bit != 0 {
        let candidate = root | bit;
        if u64::from(candidate) * u64::from(candidate) <= value as u64 {
            root = candidate;
        }
        bit >>= 1;
    }
    root as i32
}

/// 0048F370 scales by the OR of wrapping absolute component words, using signed
/// comparisons, before summing wrapping squares and restoring the length scale.
fn vector_length(vector: [i32; 3]) -> i32 {
    let mut magnitude_bits = vector
        .into_iter()
        .fold(0, |bits, value| bits | value.wrapping_abs());
    let mut shift = 0;
    for (threshold, amount) in [
        (0x34410000, 16),
        (0x344100, 8),
        (0x34410, 4),
        (0xd104, 2),
        (0x6882, 1),
    ] {
        if magnitude_bits > threshold {
            shift += amount;
            magnitude_bits >>= amount;
        }
    }
    let [x, y, z] = vector.map(|value| value >> shift);
    let square = z
        .wrapping_mul(z)
        .wrapping_add(y.wrapping_mul(y))
        .wrapping_add(x.wrapping_mul(x));
    integer_sqrt(square).wrapping_shl(shift)
}

/// The two normalization blocks in 00484B60 preserve X in an x87 register while
/// spilling Y/Z, denominator, and the Y/Z scale to f32. Keep the asymmetric
/// instruction order: replacing this with an ordinary vector normalize changes
/// integer velocity results. __ftol truncates to i64, whose low word is used.
fn normalized_scaled(vector: [i32; 3], magnitude: i32) -> [i32; 3] {
    let [x, y, z] = vector.map(f64::from);
    let y_spill = f64::from(y as f32);
    let z_spill = f64::from(z as f32);
    let length = ((z * z_spill + y_spill * y_spill) + x * x).sqrt();
    let denominator = f64::from(length as f32);
    let direction = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [
            x / denominator,
            f64::from((y_spill / denominator) as f32),
            f64::from((z_spill / denominator) as f32),
        ]
    };
    let scale_spill = f64::from(magnitude as f32);
    [
        (direction[0] * f64::from(magnitude)).trunc() as i64 as i32,
        (direction[1] * scale_spill).trunc() as i64 as i32,
        (direction[2] * scale_spill).trunc() as i64 as i32,
    ]
}

/// One fresh 005471C0 read. Model callbacks and actor interactions borrow the
/// same process owner; no scene-entry scalar copy replaces its live custody.
pub trait AnimationClockSource {
    fn animation_value(&self) -> u32;
    /// The microseconds the value counts in 20 ms steps (`00546F30/34`), for
    /// presentation that moves more smoothly than the steps.
    fn animation_micros(&self) -> u64 {
        u64::from(self.animation_value()) * 20_000
    }
}

impl AnimationClockSource for AnimationClock {
    fn animation_value(&self) -> u32 {
        self.value()
    }
    fn animation_micros(&self) -> u64 {
        self.accumulated_micros()
    }
}

/// Globals 00546F30/34 and 005471C0, owned explicitly in the port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationClock {
    accumulated_micros: u64,
    value: u32,
}

impl AnimationClock {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn accumulated_micros(&self) -> u64 {
        self.accumulated_micros
    }
    pub fn value(&self) -> u32 {
        self.value
    }

    /// 00439560 adds an UNSIGNED argument with carry, then performs signed i64
    /// division by 20,000 and retains the quotient's low 32-bit word.
    pub fn advance(&mut self, delta_micros: u32) {
        self.accumulated_micros = self
            .accumulated_micros
            .wrapping_add(u64::from(delta_micros));
        self.value = ((self.accumulated_micros as i64) / 20_000) as u32;
    }

    /// 00483C40 consumes the selector already masked by model decoder 004ED4B0
    /// (operand & 0xFF3F). Only selector zero returns the clock's low 16 bits.
    pub fn callback_value(&self, selector: u32) -> u16 {
        if selector == 0 {
            self.value as u16
        } else {
            0
        }
    }
}

#[cfg(test)]
#[path = "frontend_camera_tests.rs"]
mod tests;
