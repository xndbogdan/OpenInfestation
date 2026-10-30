//! Shared steering correction 0042A2C0 and obstacle scan 00429FF0. Actual
//! cache probes and the virtual +48 basis query remain mutable host boundaries.

use inf_formats::native_basis::NativeBasis;
use thiserror::Error;

use super::kinematics::ftol;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

mod scan;
pub use scan::{FamilyObstacleProbeHost, FamilyObstacleScanError};

pub trait FamilyAvoidanceHost {
    type Error;

    /// 00429FF0 clears/rebuilds +22E from actual terrain/collision probes.
    /// It may mutate the actor; all following fields are read afterwards.
    fn scan(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        reverse: bool,
    ) -> Result<(), Self::Error>;

    /// Actual virtual +48 result, after the displacement was captured.
    fn basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyAvoidanceError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires its position at 0042A39B")]
    MissingPosition(ObjectId),
    #[error("family avoidance host failed at {effect}: {error}")]
    Host { effect: &'static str, error: E },
}

#[derive(Clone, Copy)]
enum ForwardPolicy {
    Minimum(i32),
    Stop,
    Half,
    Reverse,
}

#[derive(Clone, Copy)]
enum TurnPolicy {
    Preserve,
    AngularSign,
    Set(i32),
    MinimumMagnitude,
}

/// The complete retail flag matrix, including intentional asymmetries between
/// near/far probe rows. Only its two authored branches add +22E bit 0020.
fn policies(flags: u16) -> (ForwardPolicy, TurnPolicy, bool) {
    use ForwardPolicy as F;
    use TurnPolicy as T;
    let sides = flags & 0x180;
    let rear = flags & 0x6000;
    if flags & 0x0fc0 == 0x0fc0 {
        return if rear == 0 {
            (F::Reverse, T::Set(0), false)
        } else if flags & 0x1000 == 0 {
            (F::Minimum(256), T::Set(0), false)
        } else {
            (F::Stop, T::AngularSign, false)
        };
    }
    if flags & 0x0e00 == 0x0e00 {
        let forward = if rear == 0 { F::Reverse } else { F::Stop };
        let turn = match sides {
            0 => match rear {
                0 | 0x6000 => T::MinimumMagnitude,
                _ => T::Set(if flags & 0x2000 != 0 { 6144 } else { -6144 }),
            },
            0x180 => {
                if rear == 0 {
                    T::Preserve
                } else {
                    T::AngularSign
                }
            }
            _ => T::Set(if flags & 0x80 != 0 { 12288 } else { -12288 }),
        };
        return (forward, turn, true);
    }
    if flags & 0x40 != 0 {
        return if flags & 0x0e00 == 0 {
            (F::Minimum(256), T::Preserve, true)
        } else if flags & 0x0c00 == 0x0c00 {
            (F::Minimum(256), T::Set(0), true)
        } else {
            (
                F::Half,
                T::Set(if flags & 0x400 != 0 { 6144 } else { -6144 }),
                true,
            )
        };
    }
    if flags & 0x200 != 0 {
        return if sides != 0 {
            (
                F::Stop,
                T::Set(if flags & 0x80 != 0 { 6144 } else { -6144 }),
                false,
            )
        } else if flags & 0x0c00 != 0 {
            (
                F::Half,
                T::Set(if flags & 0x400 != 0 { 6144 } else { -6144 }),
                false,
            )
        } else {
            (F::Stop, T::Preserve, false)
        };
    }
    if flags & 0x0e00 == 0 {
        (F::Minimum(256), T::Preserve, false)
    } else if flags & 0x0c00 == 0x0c00 {
        (F::Half, T::Set(0), false)
    } else {
        (
            F::Stop,
            T::Set(if flags & 0x400 != 0 { 12288 } else { -12288 }),
            false,
        )
    }
}

impl ObjectArena {
    /// 0042A2C0. Controls are ordered [turn, forward] signed DWORDs. False
    /// means the caller must still perform its native velocity/motion tail.
    /// Failures retain reached actor writes and synchronous host mutations.
    pub fn adjust_family_avoidance<H: FamilyAvoidanceHost>(
        &mut self,
        actor: ObjectId,
        controls: &mut [i32; 2],
        microseconds: u32,
        host: &mut H,
    ) -> Result<bool, FamilyAvoidanceError<H::Error>> {
        let reverse = controls[1] < 0;
        if controls[1] == 0 {
            return Ok(false);
        }
        self.family429e20_actor(actor)?;
        host.scan(self, actor, reverse)
            .map_err(|error| FamilyAvoidanceError::Host {
                effect: "00429FF0 probe scan",
                error,
            })?;
        let flags = self.family428650_actor(actor)?.word_22e;
        if flags & 0xffe0 == 0 {
            return Ok(false);
        }
        let positive = i32::from(flags & 0x80 != 0) + i32::from(flags & 0x400 != 0);
        let negative = -i32::from(flags & 0x100 != 0) - i32::from(flags & 0x800 != 0);
        let side = if positive == 0 {
            negative
        } else if negative == 0 {
            positive
        } else {
            0
        };
        if side != 0 {
            // This consumer uses signed SAR10; the cooldown below uses SHR10.
            let mut displacement = i32::from(self.object(actor)?.base.word_28)
                .wrapping_mul((microseconds as i32) >> 10)
                .wrapping_mul(side)
                >> 11;
            if reverse {
                displacement = displacement.wrapping_neg();
            }
            let basis = host
                .basis(self, actor)
                .map_err(|error| FamilyAvoidanceError::Host {
                    effect: "virtual +48 basis",
                    error,
                })?;
            let position = self
                .object(actor)?
                .base
                .position_words
                .ok_or(FamilyAvoidanceError::MissingPosition(actor))?;
            let units = f64::from(displacement as i16);
            let moved = std::array::from_fn(|axis| {
                position[axis].wrapping_add(ftol(
                    units * f64::from(f32::from_bits(basis.words[1 + axis])),
                ) as u16)
            });
            // Native direct XYZ stores do not set the position dirty bits.
            self.owned_mut(actor)?.state.base.position_words = Some(moved);
        }
        let previous = self.family429e20_actor(actor)?.word_238;
        if previous != 0 && self.family428650_actor(actor)?.word_22e & 0x240 == 0 {
            let step = microseconds >> 10;
            self.family429e20_actor_mut(actor)?.word_238 = if u32::from(previous) > step {
                previous.wrapping_sub(step as u16)
            } else {
                0
            };
            controls[1] = 0;
            return Ok(true);
        }
        let flags = self.family428650_actor(actor)?.word_22e;
        let (forward, turn, add_flag) = policies(flags);
        if add_flag {
            self.family428650_actor_mut(actor)?.word_22e |= 0x20;
        }
        let magnitude = controls[1].wrapping_abs();
        controls[1] = match forward {
            ForwardPolicy::Minimum(minimum) => magnitude.max(minimum),
            ForwardPolicy::Stop => 0,
            ForwardPolicy::Half => magnitude / 2,
            ForwardPolicy::Reverse => magnitude.max(128).wrapping_neg(),
        };
        if reverse {
            controls[1] = controls[1].wrapping_neg();
        }
        controls[0] = match turn {
            TurnPolicy::Preserve => controls[0],
            TurnPolicy::AngularSign => {
                if self.actor(actor)?.words_94_9c[1] as i32 > 0 {
                    12288
                } else {
                    -12288
                }
            }
            TurnPolicy::Set(value) => value,
            TurnPolicy::MinimumMagnitude => {
                if controls[0] < 0 {
                    controls[0].min(-4096)
                } else {
                    controls[0].max(4096)
                }
            }
        };
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
