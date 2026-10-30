//! 004476D0, mode 4's helper +04: the nemian's and the rhino's two extended
//! legs (selector 21 on selector 10's chain). The forward speed picks a gait
//! from 00536A78 (005368E0 for any other child count), the stride phase +34
//! advances by it, gait 0 runs the common 00445CD0 and the others the gait
//! step 00446C80, and each child's +5C follows its gait's fourth WORD.
//!
//! The x87 runs with control word 027F (53-bit precision, nearest), so every
//! product of a WORD and an f32 is exact in f64 and each sum rounds as an f64
//! sum does. __ftol truncates toward zero.
use super::super::legs::ftol;
use super::{table_word, Selector10HelperTickError, Selector10HelperTickRequest};
use crate::{
    object_runtime::{ObjectArena, ObjectId},
    platform_audio::ResidentSoundResource,
    process_sound::{
        ProcessSoundConstructionError, ProcessSoundRecordConstructionRequest,
        ProcessSoundRecordKind,
    },
};
use inf_formats::FrontendExecutable;

/// The gait table for a two-child descriptor and for any other. Byte 0
/// counts legs and byte 1 gaits; each 100-byte gait holds a threshold WORD,
/// a period WORD and, per leg, four WORDs and four (key, value) pairs.
const TWO_LEG_GAITS: u32 = 0x0053_6a78;
const OTHER_GAITS: u32 = 0x0053_68e0;
const GAIT_BYTES: u32 = 100;
const LEG_BYTES: u32 = 24;
/// 00446C80's footstep: resource 0x18 at the landing foot, volume 0x4000,
/// pitch 0x10000.
const FOOTSTEP: i16 = 0x18;

/// The gait the speed reached: its period and each leg's four WORDs.
struct Stride {
    period: u16,
    legs: Vec<[u16; 4]>,
}

fn word(exe: FrontendExecutable<'_>, address: u32) -> Result<u16, Selector10HelperTickError> {
    Ok(table_word(exe, address)?)
}

fn signed_byte(
    exe: FrontendExecutable<'_>,
    address: u32,
) -> Result<i32, Selector10HelperTickError> {
    Ok(i32::from(exe.mapped(address, 1)?.bytes[0] as i8))
}

/// 00447AE0: a + ((b - a) * t >> 15) over signed WORDs; callers keep the
/// low WORD.
fn lerp_447ae0(a: u16, b: u16, t: i32) -> u16 {
    let delta = i32::from(b.wrapping_sub(a) as i16);
    a.wrapping_add((delta.wrapping_mul(i32::from(t as i16)) >> 15) as u16)
}

/// 00447B00: 00447AE0 the short way round a 0x8000 circle; a difference
/// of 0x4000 or more either way flips by 0x8000 first.
fn turn_447b00(a: u16, b: u16, t: i32) -> u16 {
    let mut delta = b.wrapping_sub(a) as i16;
    if !(-0x4000 < delta && delta < 0x4000) {
        delta = (delta as u16).wrapping_sub(0x8000) as i16;
    }
    a.wrapping_add((i32::from(delta).wrapping_mul(i32::from(t as i16)) >> 15) as u16)
}

/// 0048F080: the vector at Q31 length (0x7FFFFFFF is 1). Halving brings the
/// OR of the magnitudes under 0x6882, a bitwise root of the sum of squares
/// follows, and each component is shifted up 31 and divided by the root plus
/// one, saturating by sign where it is at least that divisor.
pub(crate) fn unit_48f080(vector: [i32; 3]) -> [i32; 3] {
    let [mut x, mut y, mut z] = vector;
    let mut magnitude = x.unsigned_abs() | y.unsigned_abs() | z.unsigned_abs();
    while magnitude > 0x6882 {
        magnitude >>= 1;
        x >>= 1;
        y >>= 1;
        z >>= 1;
    }
    let squared = z
        .wrapping_mul(z)
        .wrapping_add(y.wrapping_mul(y))
        .wrapping_add(x.wrapping_mul(x));
    let divisor = if squared > 0 { root_48f0ed(squared) } else { 0 } + 1;
    [x, y, z].map(|component| {
        if component.wrapping_abs() >= divisor {
            ((component ^ divisor) >> 31) ^ 0x7fff_ffff
        } else {
            ((i64::from(component) << 31) / i64::from(divisor)) as i32
        }
    })
}

/// 0048F0ED..0048F187: the largest power of two whose square fits, then
/// each lower bit kept while (root + bit)^2 stays within the sum.
fn root_48f0ed(squared: i32) -> i32 {
    let mut root = 0x8000_i32;
    let mut square = 0x4000_0000_i32;
    let mut shift = 15_u32;
    if squared < 0x4000_0000 {
        loop {
            root >>= 1;
            square >>= 2;
            shift -= 1;
            if square <= squared {
                break;
            }
        }
    }
    let mut bit = root >> 1;
    while bit != 0 {
        let step = (root << shift).wrapping_add(1 << (2 * (shift - 1)));
        if square <= squared.wrapping_sub(step) {
            root += bit;
            square = square.wrapping_add(step);
        }
        bit >>= 1;
        shift -= 1;
    }
    root
}

/// (value * (microseconds << 10)) >> 31's low WORD against the period: a
/// whole period steps 0x7FFF, anything else (x << 15) / period.
fn phase_step(
    actor: ObjectId,
    value: i32,
    microseconds: u32,
    period: u16,
    address: u32,
) -> Result<u16, Selector10HelperTickError> {
    let scaled = i64::from(value).wrapping_mul(i64::from(microseconds.wrapping_shl(10) as i32));
    let low = (scaled >> 31) as u16;
    if low == period {
        return Ok(0x7fff);
    }
    let divisor = i32::from(period as i16);
    if divisor == 0 {
        return Err(Selector10HelperTickError::Division { actor, address });
    }
    Ok(((i32::from(low as i16) << 15) / divisor) as u16)
}

impl ObjectArena {
    /// The helper's +34, which neither 00446BC0 nor 00445A90 writes.
    fn walker_phase(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<u16, Selector10HelperTickError> {
        match self.selector10_helper_ref(actor, address)?.class {
            super::Selector10HelperClass::Family446bc0 {
                phase_34: Some(phase),
            } => Ok(phase),
            _ => Err(Selector10HelperTickError::RequiredHelperField {
                actor,
                offset: 0x34,
                address,
            }),
        }
    }

    fn set_walker_phase(
        &mut self,
        actor: ObjectId,
        phase: u16,
        address: u32,
    ) -> Result<(), Selector10HelperTickError> {
        match &mut self.selector10_helper_mut(actor, address)?.class {
            super::Selector10HelperClass::Family446bc0 { phase_34 } => {
                *phase_34 = Some(phase);
                Ok(())
            }
            super::Selector10HelperClass::Family445ae0 => {
                Err(Selector10HelperTickError::RequiredHelperField {
                    actor,
                    offset: 0x34,
                    address,
                })
            }
        }
    }

    /// The extended child's WORD +5C.
    fn set_extended_word(
        &mut self,
        actor: ObjectId,
        index: u32,
        offset: usize,
        value: u16,
        address: u32,
    ) -> Result<(), Selector10HelperTickError> {
        let child = self.selector10_child_mut(actor, index, address)?;
        match &mut child.class {
            super::Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } => {
                for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
                    unknown_50_73[offset - 0x50 + i] = Some(byte);
                }
                Ok(())
            }
            super::Selector10HelperChildClass::Base445a70 => {
                Err(Selector10HelperTickError::BaseChildExtension {
                    actor,
                    index,
                    address,
                })
            }
        }
    }

    /// 004476D0 (thiscall helper; the actor and microseconds; RET 8).
    pub(super) fn tick_selector10_walker(
        &mut self,
        actor: ObjectId,
        mut request: Selector10HelperTickRequest<'_>,
    ) -> Result<u32, Selector10HelperTickError> {
        let exe = request.executable;
        // The H12 acquire at entry finds the terrain 00425400's contact used.
        let descriptor = self.selector10_helper_ref(actor, 0x4476f8)?.table_10;
        let count = table_word(exe, descriptor)?;
        let table = if count == 2 {
            TWO_LEG_GAITS
        } else {
            OTHER_GAITS
        };
        // Each child's +00 (00404610) returns the child itself.
        for index in 0..u32::from(count) {
            self.selector10_child(actor, index, 0x44772a)?;
        }
        // The actor's +48 (004026B0) returns its basis; the result is unused.
        let state = self.actor(actor)?;
        let velocity = state.words_8e_92.map(|w| f64::from(w as i16));
        let basis = state.rotating.basis;
        let row = |i: usize| f64::from(f32::from_bits(basis.words[7 + i]));
        let forward = ftol((velocity[2] * row(2) + velocity[1] * row(1)) + velocity[0] * row(0));
        let scale = i32::from(self.object(actor)?.base.word_28);
        if scale == 0 {
            return Err(Selector10HelperTickError::Division {
                actor,
                address: 0x4477a4,
            });
        }
        let speed = forward.wrapping_abs().wrapping_shl(12) / scale;
        let legs = signed_byte(exe, table)?;
        let gaits = signed_byte(exe, table + 1)?;
        let mut gait = 0;
        while gait < gaits {
            let threshold = i32::from(word(exe, table + 2 + GAIT_BYTES * gait as u32)? as i16);
            if threshold >= speed {
                break;
            }
            gait += 1;
        }
        self.selector10_helper_mut(actor, 0x4477cc)?.word_04 = Some(gait as u32);
        let leg_words = |record: u32| -> Result<Vec<[u16; 4]>, Selector10HelperTickError> {
            (0..legs.max(0) as u32)
                .map(|leg| {
                    let base = record + LEG_BYTES * leg;
                    Ok([
                        word(exe, base)?,
                        word(exe, base + 2)?,
                        word(exe, base + 4)?,
                        word(exe, base + 6)?,
                    ])
                })
                .collect()
        };
        let (result, stride) = if gait == 0 {
            let period = word(exe, table + 4)?;
            let phase = self.walker_phase(actor, 0x4477d9)?;
            if phase != 0 {
                let threshold = i32::from(word(exe, table + 2)? as i16);
                let mean = threshold.wrapping_add(speed) / 2;
                let step = phase_step(actor, mean, request.microseconds, period, 0x44783b)?;
                let mut next = phase.wrapping_add(step);
                if (next as i16) < 0 {
                    next = 0;
                }
                self.set_walker_phase(actor, next, 0x44783d)?;
            }
            let stride = Stride {
                period,
                legs: leg_words(table + 6)?,
            };
            (self.tick_common_445cd0(actor, request)?, stride)
        } else {
            let record = table + GAIT_BYTES * gait as u32;
            let (stride, address) = if gait == gaits {
                let stride = Stride {
                    period: word(exe, record - 0x60)?,
                    legs: leg_words(record - 0x5e)?,
                };
                (stride, 0x4478f5)
            } else {
                let low = i32::from(word(exe, record - 0x62)? as i16);
                let high = i32::from(word(exe, record + 2)? as i16);
                let span = high.wrapping_sub(low);
                let reach = speed.wrapping_sub(low);
                let t = if reach == span {
                    0x7fff
                } else {
                    reach.wrapping_shl(15).checked_div(span).ok_or(
                        Selector10HelperTickError::Division {
                            actor,
                            address: 0x447977,
                        },
                    )?
                };
                let period = lerp_447ae0(word(exe, record - 0x60)?, word(exe, record + 4)?, t);
                // 00447B30 also reads each leg's keyframes at the phase and
                // discards the value; no table key pair divides by zero.
                let legs = (0..legs.max(0) as u32)
                    .map(|leg| {
                        let from = record - 0x5e + LEG_BYTES * leg;
                        let to = from + GAIT_BYTES;
                        Ok([
                            turn_447b00(word(exe, from)?, word(exe, to)?, t),
                            turn_447b00(word(exe, from + 2)?, word(exe, to + 2)?, t),
                            lerp_447ae0(word(exe, from + 4)?, word(exe, to + 4)?, t),
                            lerp_447ae0(word(exe, from + 6)?, word(exe, to + 6)?, t),
                        ])
                    })
                    .collect::<Result<Vec<_>, Selector10HelperTickError>>()?;
                (Stride { period, legs }, 0x4479d5)
            };
            let phase = self.walker_phase(actor, address)?;
            let step = phase_step(actor, speed, request.microseconds, stride.period, address)?;
            let next = phase.wrapping_add(step) & 0x7fff;
            self.set_walker_phase(actor, next, address)?;
            (
                self.gait_step_446c80(actor, &stride, next, &mut request)?,
                stride,
            )
        };
        for (index, words) in stride.legs.iter().enumerate() {
            let index = index as u32;
            let rest = i32::from(self.child_word(actor, index, 0x38, 0x447aa8)? as i16);
            let lift = (rest.wrapping_mul(i32::from(words[3] as i16)) >> 15) as u16;
            self.set_extended_word(actor, index, 0x5c, lift, 0x447ab2)?;
        }
        Ok(result)
    }

    /// 00446C80(actor, table, stride, phase): the horizontal velocity as a
    /// unit vector scaled by +28 and the period, a target for each swinging
    /// leg, and each landing counted with a footstep for the first two.
    fn gait_step_446c80(
        &mut self,
        actor: ObjectId,
        stride: &Stride,
        phase: u16,
        request: &mut Selector10HelperTickRequest<'_>,
    ) -> Result<u32, Selector10HelperTickError> {
        let state = self.actor(actor)?;
        let words = state.words_8e_92.map(|w| i32::from(w as i16));
        let velocity = words.map(f64::from);
        let basis = state.rotating.basis;
        let up = |i: usize| f64::from(f32::from_bits(basis.words[4 + i]));
        let vertical =
            ftol((velocity[1] * up(1) + velocity[2] * up(2)) + velocity[0] * up(0)) as i16;
        let along = f64::from(vertical);
        let horizontal: [i32; 3] =
            std::array::from_fn(|i| words[i].wrapping_sub(i32::from(ftol(along * up(i)) as i16)));
        let unit = unit_48f080(horizontal);
        let scale = i32::from(self.object(actor)?.base.word_28)
            .wrapping_mul(i32::from(stride.period as i16))
            >> 12;
        let reach = unit.map(|c| (i64::from(scale).wrapping_mul(i64::from(c)) >> 31) as i32);
        let mut landed = 0_u32;
        for (index, words) in stride.legs.iter().enumerate() {
            let index = index as u32;
            // 00450A30 samples the ground under the foot; the height and the
            // terrain's first DWORD shifted by 005483BC are both discarded.
            self.child_word(actor, index, 0x1c, 0x446dba)?;
            self.child_word(actor, index, 0x18, 0x446dbe)?;
            let start = i32::from(words[0] as i16);
            let swing = (i32::from(phase as i16) - start) & 0x7fff;
            let length = (i32::from(words[1] as i16) - start) & 0x7fff;
            if swing < length {
                if self.selector10_child(actor, index, 0x446dfd)?.word_36 == 0 {
                    self.child_copy_xyz(actor, index, 0x18, 0x30, 0x446e0a, 0x446e0e)?;
                }
                self.selector10_child_mut(actor, index, 0x446e21)?.word_36 =
                    ((swing << 15) / length) as u16;
                for (axis, offset) in [0x24_u16, 0x26, 0x28].into_iter().enumerate() {
                    let target = i32::from(words[2] as i16).wrapping_mul(reach[axis]) >> 8;
                    let store = [0x446e31, 0x446e41, 0x446e51][axis];
                    self.child_store(actor, index, offset, (target as u16).to_le_bytes(), store)?;
                }
            } else {
                if self.selector10_child(actor, index, 0x446e59)?.word_36 != 0 && index < 2 {
                    let resource = ResidentSoundResource::capture(request.residency, FOOTSTEP);
                    let result = request.sounds.records_mut().construct(
                        ProcessSoundRecordConstructionRequest {
                            resource,
                            kind: ProcessSoundRecordKind::OneShot,
                            volume: 0x4000,
                            pitch: 0x10000,
                            control_flag: 0,
                            // 00487EC0 reads the foot +18..+1C after linkage.
                            position: || -> Result<[u16; 3], Selector10HelperTickError> {
                                Ok([
                                    self.child_word(actor, index, 0x18, 0x446e7a)?,
                                    self.child_word(actor, index, 0x1a, 0x446e7a)?,
                                    self.child_word(actor, index, 0x1c, 0x446e7a)?,
                                ])
                            },
                        },
                    );
                    match result {
                        Ok(_) => (),
                        Err(ProcessSoundConstructionError::Record(error)) => {
                            return Err(error.into())
                        }
                        Err(ProcessSoundConstructionError::Source(error)) => return Err(error),
                    }
                }
                let child = self.selector10_child_mut(actor, index, 0x446e88)?;
                if child.word_36 != 0 {
                    child.word_36 = 0;
                    landed += 1;
                    child.word_14 |= 8;
                }
            }
            self.selector10_child_mut(actor, index, 0x446eb6)?.word_14 &= !2;
        }
        Ok(landed)
    }
}

#[cfg(test)]
mod tests;
