//! Selector10's custom vertex 00425870 and its helper +0C 004463E0, with the
//! base child's leg setup 00445F70 and knee 004460B0 and the terrain segment
//! probe 0043A380. The helper saves and restores the published transform
//! block 0055F190..0055F26F around the solve; nothing in between writes it.
//!
//! The x87 runs with control word 027F (53-bit precision, nearest), so every
//! product of two f32 values is exact in f64 and each stored f32 is the
//! nearest-rounded f64 result. __ftol truncates toward zero.

use inf_formats::{native_basis::isqrt_48efc0, FrontendExecutable};
use thiserror::Error;

use super::{Selector10Helper, Selector10HelperChild, Selector10HelperChildClass};
use crate::model_vertices::{FurnitureCustomTrail, ModelVertexFault, ModelVertices};

/// The terrain and water samplers the legs reach, with their native inputs.
pub trait Selector10LegGround {
    /// 00450A30's signed low AX; only the low WORDs of x and z are read.
    fn height(&mut self, x: u16, z: u16, clock: u32) -> i16;
    /// 00410F60(x, z, clock, H14 flags & 0x40, terrain height).
    fn water(&mut self, x: u16, z: u16, clock: u32, flags: u32, terrain: i16) -> i16;
    /// Port setting, not retail: the ground a wheeled craft's wheels meet,
    /// sunk into the puddles' carve where the remaster sets one
    /// (`TerrainRuntime::wheel_height_at`); retail's height otherwise.
    fn wheel_height(&mut self, x: u16, z: u16, clock: u32) -> i16 {
        self.height(x, z, clock)
    }
}

/// A ground whose height is the wheels' (`Selector10LegGround::wheel_height`),
/// for the wheels' segment probe.
pub struct WheelGround<'a, G: ?Sized>(pub &'a mut G);

impl<G: Selector10LegGround + ?Sized> Selector10LegGround for WheelGround<'_, G> {
    fn height(&mut self, x: u16, z: u16, clock: u32) -> i16 {
        self.0.wheel_height(x, z, clock)
    }
    fn water(&mut self, x: u16, z: u16, clock: u32, flags: u32, terrain: i16) -> i16 {
        self.0.water(x, z, clock, flags, terrain)
    }
}

/// Process services of 00425870 beyond the ground samplers.
pub trait Selector10CustomServices: Selector10LegGround {
    /// Clock DWORD 005471C0.
    fn clock(&self) -> u32;
    /// H14/0 header DWORD +4.
    fn world_flags(&self) -> u32;
    /// Process byte 0053B890: 00478F44 clears it, 00479827 sets it.
    fn update_byte(&self) -> u8;
    /// One 0048F060 draw from the shared process RNG.
    fn random(&mut self) -> u32;
    /// 00489D90 with a valid type; its NULL return is not an error.
    fn birth_trail(&mut self, trail: FurnitureCustomTrail) -> Result<(), String>;
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum Selector10LegError {
    #[error(transparent)]
    Row(#[from] ModelVertexFault),
    #[error("selector10 custom vertex needs extra WORD {index}")]
    RequiredExtra { index: usize },
    #[error("selector10 helper child {index} is absent")]
    MissingChild { index: i32 },
    #[error("selector10 helper child {index} needs its unwritten +{offset:X}")]
    UnknownField { index: i32, offset: u16 },
    #[error("selector10 knee mode {mode} leaves its bend vector unwritten")]
    UnknownKneeMode { mode: u32 },
    #[error("selector10 helper descriptor word at {address:08X} is unmapped")]
    Descriptor { address: u32 },
    #[error("selector10 {site} divides by zero or overflows")]
    Division { site: &'static str },
    #[error("selector10 actor needs its unwritten +{offset:X}")]
    ActorField { offset: u16 },
    #[error("selector10 trail service failed: {0}")]
    Trail(String),
}

/// MSVCRT __ftol at 00511BA8: truncation to a QWORD, of which callers keep
/// the low DWORD (or WORD). Masked invalid conversions give the integer
/// indefinite, whose low DWORD is zero.
pub(super) fn ftol(value: f64) -> i32 {
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    let truncated = value.trunc();
    if truncated.is_finite() && (-BOUND..BOUND).contains(&truncated) {
        truncated as i64 as i32
    } else {
        0
    }
}

fn product(a: f32, b: f32) -> f64 {
    f64::from(a) * f64::from(b)
}

/// 00447EB0: a x b, each component the f32 of an f64 difference.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        (product(b[2], a[1]) - product(a[2], b[1])) as f32,
        (product(a[2], b[0]) - product(b[2], a[0])) as f32,
        (product(a[0], b[1]) - product(b[0], a[1])) as f32,
    ]
}

/// 00447B90: each component truncates the product with the signed WORD.
fn scale(vector: [f32; 3], factor: i16) -> [u16; 3] {
    vector.map(|axis| ftol(f64::from(factor) * f64::from(axis)) as u16)
}

fn signed(word: u16) -> i32 {
    i32::from(word as i16)
}

fn add(a: [u16; 3], b: [u16; 3]) -> [u16; 3] {
    std::array::from_fn(|i| a[i].wrapping_add(b[i]))
}

fn squared_distance(a: [u16; 3], b: [u16; 3]) -> i32 {
    (0..3).fold(0_i32, |sum, i| {
        let delta = signed(a[i].wrapping_sub(b[i]));
        sum.wrapping_add(delta.wrapping_mul(delta))
    })
}

/// 0043A380's inputs. Out and in may alias natively; every input word is
/// read before any output word is written, so a copy is equivalent.
#[derive(Clone, Copy, Debug)]
pub struct SegmentProbe {
    pub input: [u16; 3],
    pub direction: [f32; 3],
    /// Pushed DWORD; the probe doubles it and keeps the signed low WORD.
    pub length: i32,
    /// Pushed DWORD whose low byte enables the water sampler.
    pub water: u32,
    pub iterations: u32,
    pub clock: u32,
    pub world_flags: u32,
}

/// 0043A380: the point along `input -/+ 2*length*direction` where the ground
/// (and water, when enabled) crosses, by regula falsi. A direction whose Y is
/// negative or unordered returns the low end at once. AL is false for that
/// and for a low end already above ground, true otherwise.
pub fn segment_probe_43a380<G: Selector10LegGround + ?Sized>(
    ground: &mut G,
    probe: SegmentProbe,
) -> Result<([u16; 3], bool), Selector10LegError> {
    let factor = f64::from(probe.length.wrapping_mul(2) as i16);
    let offset = probe
        .direction
        .map(|axis| ftol(factor * f64::from(axis)) as u16);
    let low: [u16; 3] = std::array::from_fn(|i| probe.input[i].wrapping_sub(offset[i]));
    if !(probe.direction[1] >= 0.0) {
        return Ok((low, false));
    }
    let mut sample = |point: [u16; 3]| {
        let terrain = ground.height(point[0], point[2], probe.clock);
        if probe.water & 0xff == 0 {
            return i32::from(terrain);
        }
        let water = ground.water(
            point[0],
            point[2],
            probe.clock,
            probe.world_flags & 0x40,
            terrain,
        );
        i32::from(terrain.max(water))
    };
    let mut below = low;
    let mut below_ground = sample(below);
    if below_ground <= signed(below[1]) {
        return Ok((below, false));
    }
    let mut above = add(probe.input, offset);
    let mut above_ground = sample(above);
    if above_ground >= signed(above[1]) {
        return Ok((above, true));
    }
    let mut output = interpolate(below, below_ground, above, above_ground)?;
    let mut remaining = probe.iterations;
    while remaining != 0 {
        remaining = remaining.wrapping_sub(1);
        let height = sample(output);
        let difference = signed(output[1]).wrapping_sub(height);
        if difference < -1 {
            below = output;
            below_ground = height;
        } else if difference <= 1 {
            break;
        } else {
            above = output;
            above_ground = height;
        }
        output = interpolate(below, below_ground, above, above_ground)?;
    }
    Ok((output, true))
}

fn interpolate(
    below: [u16; 3],
    below_ground: i32,
    above: [u16; 3],
    above_ground: i32,
) -> Result<[u16; 3], Selector10LegError> {
    let divisor = signed(above[1])
        .wrapping_sub(signed(below[1]))
        .wrapping_sub(above_ground)
        .wrapping_add(below_ground);
    let rise = below_ground.wrapping_sub(signed(below[1]));
    if divisor == 0 {
        let middle = |a: u16, b: u16| (signed(a) + signed(b)) / 2;
        return Ok([
            middle(above[0], below[0]) as u16,
            middle(below[1], above[1]) as u16,
            middle(above[2], below[2]) as u16,
        ]);
    }
    let lerp = |axis: usize| {
        signed(above[axis].wrapping_sub(below[axis]))
            .wrapping_mul(rise)
            .checked_div(divisor)
            .map(|step| step.wrapping_add(i32::from(below[axis])) as u16)
            .ok_or(Selector10LegError::Division {
                site: "0043A380 interpolation",
            })
    };
    Ok([lerp(0)?, lerp(1)?, lerp(2)?])
}

/// Byte-level custody of a base child: None is a native byte no writer has
/// reached yet. +14 and +36 are always written by 00445A70.
struct Child<'c> {
    child: &'c mut Selector10HelperChild,
    index: i32,
}

impl Child<'_> {
    fn byte(&self, offset: u16) -> Option<u8> {
        let at = usize::from(offset);
        match offset {
            0x04..=0x13 => self.child.unknown_04_13[at - 0x04],
            0x14..=0x17 => Some(self.child.word_14.to_le_bytes()[at - 0x14]),
            0x18..=0x35 => self.child.unknown_18_35[at - 0x18],
            0x36..=0x37 => Some(self.child.word_36.to_le_bytes()[at - 0x36]),
            0x38..=0x4f => self.child.unknown_38_4f[at - 0x38],
            0x50..=0x73 => match &self.child.class {
                Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } => {
                    unknown_50_73[at - 0x50]
                }
                Selector10HelperChildClass::Base445a70 => None,
            },
            _ => None,
        }
    }

    fn set_byte(&mut self, offset: u16, value: u8) {
        let at = usize::from(offset);
        match offset {
            0x04..=0x13 => self.child.unknown_04_13[at - 0x04] = Some(value),
            0x14..=0x17 => {
                let mut bytes = self.child.word_14.to_le_bytes();
                bytes[at - 0x14] = value;
                self.child.word_14 = u32::from_le_bytes(bytes);
            }
            0x18..=0x35 => self.child.unknown_18_35[at - 0x18] = Some(value),
            0x36..=0x37 => {
                let mut bytes = self.child.word_36.to_le_bytes();
                bytes[at - 0x36] = value;
                self.child.word_36 = u16::from_le_bytes(bytes);
            }
            0x38..=0x4f => self.child.unknown_38_4f[at - 0x38] = Some(value),
            0x50..=0x73 => match &mut self.child.class {
                Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } => {
                    unknown_50_73[at - 0x50] = Some(value)
                }
                Selector10HelperChildClass::Base445a70 => unreachable!("base child extent"),
            },
            _ => unreachable!("extended child extent"),
        }
    }

    fn read<const N: usize>(&self, offset: u16) -> Result<[u8; N], Selector10LegError> {
        let mut bytes = [0; N];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = self
                .byte(offset + i as u16)
                .ok_or(Selector10LegError::UnknownField {
                    index: self.index,
                    offset,
                })?;
        }
        Ok(bytes)
    }

    fn word(&self, offset: u16) -> Result<u16, Selector10LegError> {
        Ok(u16::from_le_bytes(self.read(offset)?))
    }

    fn dword(&self, offset: u16) -> Result<i32, Selector10LegError> {
        Ok(i32::from_le_bytes(self.read(offset)?))
    }

    fn vector(&self, offset: u16) -> Result<[u16; 3], Selector10LegError> {
        Ok([
            self.word(offset)?,
            self.word(offset + 2)?,
            self.word(offset + 4)?,
        ])
    }

    fn set_word(&mut self, offset: u16, value: u16) {
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.set_byte(offset + i as u16, byte);
        }
    }

    fn set_dword(&mut self, offset: u16, value: i32) {
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.set_byte(offset + i as u16, byte);
        }
    }

    fn set_vector(&mut self, offset: u16, vector: [u16; 3]) {
        for (i, word) in vector.into_iter().enumerate() {
            self.set_word(offset + 2 * i as u16, word);
        }
    }
}

/// 00445F70, the base child's +4: the leg's rest lengths from its hip,
/// rest foot and rest knee rows. It writes +04..+09 and +38..+4F.
fn leg_setup_445f70(child: &mut Child<'_>, hip: [u16; 3], foot: [u16; 3], knee: [u16; 3]) {
    child.set_vector(0x04, hip);
    let thigh = squared_distance(hip, knee);
    let shin = squared_distance(knee, foot);
    child.set_dword(0x44, thigh);
    child.set_dword(0x48, shin);
    let span = isqrt_48efc0(squared_distance(hip, foot)) as u16;
    child.set_word(0x38, span);
    let reach = isqrt_48efc0(shin).wrapping_add(isqrt_48efc0(thigh)) as u16;
    child.set_word(0x3a, reach);
    child.set_dword(0x4c, signed(reach).wrapping_mul(signed(reach)));
    let half = (signed(span) / 2) as u16;
    child.set_word(0x3c, half);
    let stride = (signed(half).wrapping_mul(3) >> 2) as u16;
    child.set_word(0x3e, stride);
    child.set_dword(0x40, signed(stride).wrapping_mul(signed(stride)));
}

/// 004460B0, the base child's +8: the knee +1E..+22 from the hip and the
/// foot +18..+1C. The bend leans on a basis row chosen by the low WORD of
/// `mode`: modes 0..2 start from unit x row, 3..5 from row x unit, with rows
/// 1, 2 and 0 in turn; both then cross the unit again. Native modes above 5
/// leave the bend uninitialized.
fn knee_4460b0(
    child: &mut Child<'_>,
    hip: [u16; 3],
    basis: [f32; 9],
    mode: u32,
) -> Result<(), Selector10LegError> {
    let foot = child.vector(0x18)?;
    let delta: [f32; 3] = std::array::from_fn(|i| signed(foot[i].wrapping_sub(hip[i])) as f32);
    let length =
        (product(delta[2], delta[2]) + product(delta[1], delta[1]) + product(delta[0], delta[0]))
            .sqrt();
    let unit = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        delta.map(|axis| (f64::from(axis) / length) as f32)
    };
    let distance = ftol(length);
    let mode = mode & 0xffff;
    let row = |index: usize| [basis[3 * index], basis[3 * index + 1], basis[3 * index + 2]];
    let bend = match mode {
        0 => cross(unit, row(1)),
        1 => cross(unit, row(2)),
        2 => cross(unit, row(0)),
        3 => cross(row(1), unit),
        4 => cross(row(2), unit),
        5 => cross(row(0), unit),
        _ => return Err(Selector10LegError::UnknownKneeMode { mode }),
    };
    let bend = cross(bend, unit);
    let distance = if distance == 0 { 1 } else { distance };
    let thigh = child.dword(0x44)?;
    let shin = child.dword(0x48)?;
    let offset = shin
        .wrapping_sub(thigh)
        .checked_div(distance.wrapping_mul(2))
        .ok_or(Selector10LegError::Division {
            site: "004460B0 knee",
        })?;
    let along = (distance / 2).wrapping_sub(offset);
    let height_squared = thigh.wrapping_sub(along.wrapping_mul(along));
    let height = if height_squared < 0 {
        0
    } else {
        isqrt_48efc0(height_squared) as u16 as i16
    };
    let base = add(hip, scale(unit, along as i16));
    child.set_vector(0x1e, add(base, scale(bend, height)));
    Ok(())
}

/// 00446EE0, the extended child's +4: a three-segment leg from the hip, the
/// knee row, the ankle row and the rest foot. It stores each segment's
/// squared length and root (+44/+6C hip-knee, +48/+70 knee-ankle, +60/+64
/// ankle-foot), the reach +3A as their sum and the lower two's squared sum
/// +68, the hip-foot span +38 and a quarter of it +5C, and the reach's
/// square +4C. The first two legs (`front`) take half the thigh as +3C and
/// +3E; the others the reach's excess over the span as +3C and three
/// quarters of it as +3E. Unlike 00445F70 it keeps no copy of the hip.
fn leg_setup_446ee0(
    child: &mut Child<'_>,
    hip: [u16; 3],
    foot: [u16; 3],
    knee: [u16; 3],
    ankle: [u16; 3],
    front: bool,
) {
    let root = |squared: i32| i32::from(isqrt_48efc0(squared) as u16 as i16);
    let thigh_squared = squared_distance(hip, knee);
    child.set_dword(0x44, thigh_squared);
    let thigh = root(thigh_squared);
    child.set_dword(0x6c, thigh);
    let shin_squared = squared_distance(knee, ankle);
    child.set_dword(0x48, shin_squared);
    let shin = root(shin_squared);
    child.set_dword(0x70, shin);
    let toe_squared = squared_distance(ankle, foot);
    child.set_dword(0x60, toe_squared);
    let toe = root(toe_squared);
    child.set_dword(0x64, toe);
    let reach = (toe as u16)
        .wrapping_add(shin as u16)
        .wrapping_add(thigh as u16);
    child.set_word(0x3a, reach);
    let lower = toe.wrapping_add(shin);
    child.set_dword(0x68, lower.wrapping_mul(lower));
    let span = isqrt_48efc0(squared_distance(hip, foot)) as u16;
    child.set_word(0x38, span);
    child.set_word(0x5c, (signed(span) / 4) as u16);
    child.set_dword(0x4c, signed(reach).wrapping_mul(signed(reach)));
    let stride = if front {
        let half = (root(thigh_squared) / 2) as u16;
        child.set_word(0x3c, half);
        half
    } else {
        let excess = reach.wrapping_sub(span);
        child.set_word(0x3c, excess);
        (signed(excess).wrapping_mul(3) / 4) as u16
    };
    child.set_word(0x3e, stride);
    child.set_dword(0x40, signed(stride).wrapping_mul(signed(stride)));
}

/// 004470C0, the extended child's +8: the knee +1E..+22 and the ankle
/// +56..+5A from the hip and the foot +18..+1C. A foot at or beyond the
/// reach is pulled onto the hip-foot line at the reach, the knee and ankle
/// on it. Otherwise the bend is chosen as 004460B0's and the knee lies by
/// the law of cosines against the foot (`sense` zero) or against the point
/// the toe short of it, its lift capped by the toe; the ankle then lies on
/// the knee-foot line by the same law, bent along the same first cross
/// product (against it when `sense` is zero).
fn knee_4470c0(
    child: &mut Child<'_>,
    hip: [u16; 3],
    basis: [f32; 9],
    mode: u32,
    sense: u16,
) -> Result<(), Selector10LegError> {
    let foot = child.vector(0x18)?;
    let (unit, distance) = unit_between(hip, foot);
    if distance >= 1 {
        let reach = child.word(0x3a)? as i16;
        if distance >= i32::from(reach) {
            let foot = add(hip, scale(unit, reach));
            child.set_vector(0x18, foot);
            let thigh = child.word(0x6c)? as i16;
            child.set_vector(0x1e, add(hip, scale(unit, thigh)));
            let toe = scale(unit, child.word(0x64)? as i16);
            child.set_vector(0x56, std::array::from_fn(|i| foot[i].wrapping_sub(toe[i])));
            return Ok(());
        }
    }
    let distance = distance.max(1);
    let row = |index: usize| [basis[3 * index], basis[3 * index + 1], basis[3 * index + 2]];
    let first = match mode & 0xffff {
        0 => cross(unit, row(1)),
        1 => cross(unit, row(2)),
        2 => cross(unit, row(0)),
        3 => cross(row(1), unit),
        4 => cross(row(2), unit),
        5 => cross(row(0), unit),
        mode => return Err(Selector10LegError::UnknownKneeMode { mode }),
    };
    let bend = cross(first, unit);
    let division = || Selector10LegError::Division {
        site: "004470C0 knee",
    };
    let along = if sense != 0 {
        let reduced = distance.wrapping_sub(child.dword(0x64)?);
        if reduced < 1 {
            0
        } else {
            let shin_squared = child.dword(0x48)?;
            reduced
                .wrapping_mul(reduced)
                .wrapping_sub(shin_squared)
                .wrapping_add(child.dword(0x44)?)
                .checked_div(reduced.wrapping_mul(2))
                .ok_or_else(division)?
        }
    } else {
        let lower_squared = child.dword(0x68)?;
        distance
            .wrapping_mul(distance)
            .wrapping_sub(lower_squared)
            .wrapping_add(child.dword(0x44)?)
            .checked_div(distance.wrapping_mul(2))
            .ok_or_else(division)?
    };
    let thigh = child.dword(0x6c)?;
    let rest = thigh.wrapping_sub(along);
    let (offset, height) = if rest <= 0 {
        (thigh, 0)
    } else {
        let toe = child.dword(0x64)?;
        let lift = (rest.wrapping_mul(2).wrapping_sub(toe / 2) / 2)
            .max(0)
            .min(toe);
        let offset = thigh.wrapping_sub(lift);
        let height_squared = thigh
            .wrapping_mul(thigh)
            .wrapping_sub(offset.wrapping_mul(offset));
        if height_squared > 0 {
            (
                offset,
                i32::from(isqrt_48efc0(height_squared) as u16 as i16),
            )
        } else {
            (thigh, 0)
        }
    };
    let along_hip = scale(unit, offset as i16);
    let lifted = scale(bend, height as i16);
    let knee: [u16; 3] =
        std::array::from_fn(|i| hip[i].wrapping_add(lifted[i]).wrapping_add(along_hip[i]));
    child.set_vector(0x1e, knee);
    let foot = child.vector(0x18)?;
    let (unit, distance) = unit_between(knee, foot);
    let bend = cross(first, unit);
    let toe = child.dword(0x64)?;
    let lower = child.dword(0x70)?.wrapping_add(toe);
    let (offset, height) = if distance >= lower {
        (toe, 0)
    } else {
        let offset = if distance < 1 {
            0
        } else {
            let shin_squared = child.dword(0x48)?;
            distance
                .wrapping_mul(distance)
                .wrapping_sub(shin_squared)
                .wrapping_add(child.dword(0x60)?)
                .checked_div(distance.wrapping_mul(2))
                .ok_or_else(division)?
        };
        let room = child.dword(0x60)?.wrapping_sub(offset.wrapping_mul(offset));
        if room > 0 {
            let height = i32::from(isqrt_48efc0(room) as u16 as i16);
            (offset, if sense != 0 { height } else { -height })
        } else {
            (offset.min(toe), 0)
        }
    };
    let back = scale(unit, offset as i16);
    let lifted = scale(bend, height as i16);
    child.set_vector(
        0x56,
        std::array::from_fn(|i| foot[i].wrapping_sub(lifted[i]).wrapping_sub(back[i])),
    );
    Ok(())
}

/// The unit from `from` to `to` (f32 components of the double length, or
/// (1, 0, 0) at zero length) and the length truncated.
fn unit_between(from: [u16; 3], to: [u16; 3]) -> ([f32; 3], i32) {
    let delta: [f32; 3] = std::array::from_fn(|i| signed(to[i].wrapping_sub(from[i])) as f32);
    let length =
        (product(delta[2], delta[2]) + product(delta[1], delta[1]) + product(delta[0], delta[0]))
            .sqrt();
    let unit = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        delta.map(|axis| (f64::from(axis) / length) as f32)
    };
    (unit, ftol(length))
}

/// Inputs of 00425870 drawn from the selector10 actor.
pub struct Selector10CustomInputs<'a> {
    pub selector: i32,
    pub extras: &'a [u16],
    pub rows: &'a ModelVertices,
    /// Actor +48..+68: basis rows 0, 1 and 2 (+44 is the counter).
    pub basis: [f32; 9],
    /// Signed lifecycle +C4.
    pub lifecycle: i32,
    /// +188, read only on a trail's way.
    pub word_188: Option<u32>,
    pub word_240: u32,
    pub table_244: u32,
    pub executable: FrontendExecutable<'a>,
    /// The original quarter-sine WORD table at 0053DF00.
    pub sine: &'a [u16; 4096],
}

fn extra(extras: &[u16], index: usize) -> Result<u16, Selector10LegError> {
    extras
        .get(index)
        .copied()
        .ok_or(Selector10LegError::RequiredExtra { index })
}

fn descriptor_word(
    executable: FrontendExecutable<'_>,
    address: u32,
) -> Result<u16, Selector10LegError> {
    let bytes = executable
        .mapped(address, 2)
        .map_err(|_| Selector10LegError::Descriptor { address })?
        .bytes;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// 004463E0, helper +0C, for low-byte selectors 0 (foot), 1 (knee) and 5
/// (child +0, NULL for a base child, which leaves the output unwritten).
/// The leg named by the selector's high bits solves once per +14 bit 1;
/// later calls in the same frame reuse its foot and knee.
pub fn helper_vertex_4463e0<S: Selector10CustomServices + ?Sized>(
    helper: &mut Selector10Helper,
    inputs: &Selector10CustomInputs<'_>,
    services: &mut S,
) -> Result<Option<[i32; 3]>, Selector10LegError> {
    let low = inputs.selector & 0xff;
    let index = inputs.selector >> 8;
    let byte_0c = helper.byte_0c;
    let byte_0d = helper.byte_0d;
    let table = helper.table_10;
    let child = usize::try_from(index)
        .ok()
        .and_then(|slot| helper.children_14.get_mut(slot))
        .and_then(Option::as_deref_mut)
        .ok_or(Selector10LegError::MissingChild { index })?;
    let extended = matches!(
        child.class,
        Selector10HelperChildClass::Extended446bc0 { .. }
    );
    let mut child = Child { child, index };
    let flags = child.child.word_14;
    if flags & 3 != 3 {
        let foot_rest = inputs
            .rows
            .row_words_439e10(extra(inputs.extras, 0)? >> 8)?;
        let hip = inputs
            .rows
            .row_words_439e10(extra(inputs.extras, 1)? & 0xff)?;
        if flags & 1 == 0 {
            let knee = inputs
                .rows
                .row_words_439e10(extra(inputs.extras, 1)? >> 8)?;
            if extended {
                let ankle = inputs
                    .rows
                    .row_words_439e10(extra(inputs.extras, 0)? & 0xff)?;
                leg_setup_446ee0(&mut child, hip, foot_rest, knee, ankle, index < 2);
            } else {
                leg_setup_445f70(&mut child, hip, foot_rest, knee);
            }
            let span = signed(child.word(0x38)?);
            child.set_dword(0x0c, span);
            let half = signed(child.word(0x3c)?);
            child.set_dword(0x10, half);
        }
        let up = [inputs.basis[3], inputs.basis[4], inputs.basis[5]];
        if byte_0c == 0 {
            child.set_vector(0x18, foot_rest);
        } else if flags & 0x10 == 0 {
            child.set_vector(0x18, foot_rest);
            child.child.word_14 |= 0x10;
        } else if child.child.word_36 == 0 {
            stand(&mut child, hip, foot_rest, up, byte_0d, flags, services)?;
        } else {
            step(&mut child, foot_rest, up, byte_0d, inputs.sine, services)?;
        }
        let record = table.wrapping_add((index as u32).wrapping_mul(16));
        let sense = descriptor_word(inputs.executable, record.wrapping_add(0x12))?;
        let mode = descriptor_word(inputs.executable, record.wrapping_add(16))?;
        if extended {
            knee_4470c0(&mut child, hip, inputs.basis, u32::from(mode), sense)?;
        } else {
            knee_4460b0(&mut child, hip, inputs.basis, u32::from(mode))?;
        }
        child.child.word_14 |= 3;
    }
    // Any other selector reads the child's +0 virtual: NULL for a base
    // child, which leaves the output unwritten; the extended child itself,
    // whose ankle +56..+5A is the output.
    let vector = match low {
        1 => child.vector(0x1e)?,
        0 => child.vector(0x18)?,
        _ if extended => child.vector(0x56)?,
        _ => return Ok(None),
    };
    Ok(Some(vector.map(signed)))
}

/// 004463E0's standing arm (+36 zero): ground the foot, probe the rest
/// foot along the up row, clamp the step to 3/4 of a strange length, then
/// keep the foot within reach of the hip.
fn stand<S: Selector10CustomServices + ?Sized>(
    child: &mut Child<'_>,
    hip: [u16; 3],
    foot_rest: [u16; 3],
    up: [f32; 3],
    water: u8,
    flags: u32,
    services: &mut S,
) -> Result<(), Selector10LegError> {
    let clock = services.clock();
    let (x, z) = (child.word(0x18)?, child.word(0x1c)?);
    let terrain = services.height(x, z, clock);
    let height = if water == 0 {
        terrain
    } else {
        let flags = services.world_flags() & 0x40;
        terrain.max(services.water(x, z, clock, flags, terrain))
    };
    child.set_word(0x1a, height as u16);
    let (rest, _) = segment_probe_43a380(
        services,
        SegmentProbe {
            input: foot_rest,
            direction: up,
            length: signed(child.word(0x3c)?),
            water: u32::from(water),
            iterations: 0,
            clock,
            world_flags: services.world_flags(),
        },
    )?;
    let foot = child.vector(0x18)?;
    let step: [u16; 3] = std::array::from_fn(|i| foot[i].wrapping_sub(rest[i]));
    child.set_vector(0x24, step);
    if flags & 8 != 0 {
        child.child.word_14 &= !8;
    } else {
        let squared = squared_distance(step, [0; 3]);
        if squared > child.dword(0x40)? {
            // 0044671B negates before an unsigned QWORD load: the root of
            // 2^32 - d^2, not of d^2, scales the shortened step.
            let length = f64::from(squared.wrapping_neg() as u32).sqrt();
            child.child.word_14 |= 4;
            let (x, y, z) = if length == 0.0 {
                (1.0, 0.0_f32, 0.0_f32)
            } else {
                (
                    f64::from(signed(step[0])) / length,
                    (f64::from(signed(step[1])) / length) as f32,
                    (f64::from(signed(step[2])) / length) as f32,
                )
            };
            let factor = f64::from((ftol(length).wrapping_mul(3) >> 2) as i16);
            child.set_vector(
                0x24,
                [
                    ftol(x * factor) as u16,
                    ftol(f64::from(y) * factor) as u16,
                    ftol(f64::from(z) * factor) as u16,
                ],
            );
        }
    }
    let foot = child.vector(0x18)?;
    let reach: [u16; 3] = std::array::from_fn(|i| foot[i].wrapping_sub(hip[i]));
    let squared = squared_distance(reach, [0; 3]);
    if squared > child.dword(0x4c)? {
        let length = f64::from(squared as u32).sqrt();
        let rounded = length as f32;
        let (x, y, z) = if length == 0.0 {
            (1.0, 0.0_f32, 0.0_f32)
        } else {
            let divide = |axis: u16| f64::from(signed(axis)) / f64::from(rounded);
            (
                divide(reach[0]),
                divide(reach[1]) as f32,
                divide(reach[2]) as f32,
            )
        };
        let factor = f64::from(child.word(0x3a)? as i16);
        let offset = [
            ftol(x * factor) as u16,
            ftol(f64::from(y) * factor) as u16,
            ftol(f64::from(z) * factor) as u16,
        ];
        child.set_vector(0x18, add(hip, offset));
    }
    Ok(())
}

/// 004463E0's stepping arm (+36 nonzero): probe the step target, then move
/// the foot from its start +30 toward it by +36/32768, lifted by a sine arc.
fn step<S: Selector10CustomServices + ?Sized>(
    child: &mut Child<'_>,
    foot_rest: [u16; 3],
    up: [f32; 3],
    water: u8,
    sine: &[u16; 4096],
    services: &mut S,
) -> Result<(), Selector10LegError> {
    let target = add(foot_rest, child.vector(0x24)?);
    child.set_vector(0x2a, target);
    let clock = services.clock();
    let (target, _) = segment_probe_43a380(
        services,
        SegmentProbe {
            input: target,
            direction: up,
            length: signed(child.word(0x3c)?),
            water: u32::from(water),
            iterations: 0,
            clock,
            world_flags: services.world_flags(),
        },
    )?;
    child.set_vector(0x2a, target);
    let phase = child.child.word_36;
    let start = child.vector(0x30)?;
    let blend = |axis: usize| {
        signed(target[axis].wrapping_sub(start[axis])).wrapping_mul(i32::from(phase)) >> 15
    };
    child.set_word(0x18, (blend(0) as u16).wrapping_add(start[0]));
    let mut index = phase & 0x3ffc;
    if phase & 0x4000 != 0 {
        index = 0x3ffc - index;
    }
    let sine = u32::from(sine[usize::from(index >> 2)]);
    let mut wave = (sine | sine << 16) as i32;
    if phase & 0x8000 != 0 {
        wave = wave.wrapping_neg();
    }
    let rest = signed(child.word(0x38)?) / 4;
    let lift = ((i64::from(rest) * i64::from(wave)) << 1 >> 32) as i32;
    child.set_word(
        0x1a,
        (blend(1).wrapping_add(lift) as u16).wrapping_add(start[1]),
    );
    child.set_word(0x1c, (blend(2) as u16).wrapping_add(start[2]));
    Ok(())
}

/// 00425870 for low-byte selectors 0, 1 and 5; the caller forwards every
/// other low byte to 004260C0 unchanged. A helper solves the leg first.
/// Lifecycle 1 then fetches the hip row (the output when there is no
/// helper) and, while byte 0053B890 is clear, may birth one trail at it.
pub fn selector10_custom_425870<S: Selector10CustomServices + ?Sized>(
    helper: Option<&mut Selector10Helper>,
    inputs: &Selector10CustomInputs<'_>,
    services: &mut S,
) -> Result<Option<[i32; 3]>, Selector10LegError> {
    let helped = helper.is_some();
    let mut output = None;
    if let Some(helper) = helper {
        output = helper_vertex_4463e0(helper, inputs, services)?;
        if inputs.lifecycle != 1 {
            return Ok(output);
        }
    }
    let hip = inputs
        .rows
        .row_words_439e10(extra(inputs.extras, 1)? & 0xff)?;
    if !helped {
        output = Some(hip.map(signed));
    }
    if services.update_byte() != 0 || inputs.lifecycle != 1 {
        return Ok(output);
    }
    if services.random() as u16 >= 0x400 {
        return Ok(output);
    }
    let second = i32::from(services.random() as u16);
    // 00425949 reads the DWORD and divides before the index range check.
    let address = inputs
        .table_244
        .wrapping_add(inputs.word_240.wrapping_mul(4));
    let entry = inputs
        .executable
        .word(address)
        .map_err(|_| Selector10LegError::Descriptor { address })?;
    let dividend = inputs
        .word_188
        .ok_or(Selector10LegError::ActorField { offset: 0x188 })? as i32;
    let quotient =
        dividend
            .checked_div((entry as i32) >> 16)
            .ok_or(Selector10LegError::Division {
                site: "00425870 trail",
            })?;
    let particle_type = match inputs.word_240 {
        0 => {
            if second <= quotient {
                11
            } else {
                8
            }
        }
        1 => {
            if second >= quotient / 4 && second > quotient / 2 {
                return Ok(output);
            }
            if second <= quotient {
                6
            } else {
                5
            }
        }
        2 => 6,
        3 => {
            if second < quotient {
                return Ok(output);
            }
            5
        }
        _ => return Ok(output),
    };
    let x = 0x20 - ((services.random() as u16) >> 11);
    let y = ((services.random() as u16) >> 9) + 0x40;
    let z = 0x20 - ((services.random() as u16) >> 11);
    services
        .birth_trail(FurnitureCustomTrail {
            particle_type,
            position: hip,
            velocity: [x, y, z],
            flags: 0x800,
        })
        .map_err(Selector10LegError::Trail)?;
    Ok(output)
}

#[cfg(test)]
mod tests;
