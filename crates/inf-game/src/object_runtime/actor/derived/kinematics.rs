//! Shared actor motion consumers 00426760/00426860/00426AE0. Requests retain
//! the caller's signed control DWORD, power, table words and delta units.
//! CRT 00511BA8 truncates floating-point values; the basis-vector adapters
//! 00490870/004907E0 instead use nearest-even FISTP.

use inf_formats::native_basis::NativeBasis;
use thiserror::Error;

use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorForwardMotionTable {
    pub maximum: i32,
    pub minimum: i32,
    pub acceleration: i32,
    pub deceleration: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorLateralMotionTable {
    /// 004267C9 consumes this word with a logical shift, unlike the rate.
    pub threshold: u32,
    pub rate: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorAngularMotionTable {
    pub maximum: i32,
    pub response: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorMotionTables {
    pub forward: ActorForwardMotionTable,
    pub lateral: ActorLateralMotionTable,
    pub angular: ActorAngularMotionTable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorForwardMotionRequest {
    pub power: i32,
    pub desired_velocity: i32,
    /// The caller supplies delta >> 5 on 00467DF0, but other callers use
    /// delta >> 3 or a contact-adjusted step. No conversion is implicit.
    pub step_units: u32,
    pub table: ActorForwardMotionTable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorLateralMotionRequest {
    pub power: i32,
    pub step_units: u32,
    pub table: ActorLateralMotionTable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ActorAngularAxis {
    /// Null final argument: 0047B4B0 transforms the local Y DWORD vector.
    LocalY,
    /// Nonnull final argument: truncate products with this world vector,
    /// then 0047B470 adds directly to the angular DWORDs.
    WorldVector([f32; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorAngularMotionRequest {
    pub power: i32,
    pub desired_turn: i32,
    pub step_units: u32,
    pub table: ActorAngularMotionTable,
    pub axis: ActorAngularAxis,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorMotionError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} has no initialized position WORDs")]
    MissingPosition(ObjectId),
    #[error("native signed division traps at {address:08X}: {numerator} / {denominator}")]
    Division {
        address: u32,
        numerator: i32,
        denominator: i32,
    },
    #[error("actor {0:?} has no supported selector-12 motion table binding")]
    UnsupportedSelector12Tables(ObjectId),
}

impl ObjectArena {
    /// Constructor 00467070 stores these executable table identities. The
    /// values below are static .data words, not sampled actor state. Reject a
    /// different binding rather than resolving it as the constructor default.
    pub fn selector12_motion_tables(
        &self,
        actor: ObjectId,
    ) -> Result<ActorMotionTables, ActorMotionError> {
        Ok(ActorMotionTables {
            lateral: self.selector12_lateral_table(actor)?,
            forward: self.selector12_forward_table(actor)?,
            angular: self.selector12_angular_table(actor)?,
        })
    }

    pub(super) fn selector12_forward_table(
        &self,
        actor: ObjectId,
    ) -> Result<ActorForwardMotionTable, ActorMotionError> {
        let state = self
            .selector12_actor(actor)
            .map_err(|_| ActorMotionError::UnsupportedSelector12Tables(actor))?;
        if state.table_290 != 0x0053_a080 {
            return Err(ActorMotionError::UnsupportedSelector12Tables(actor));
        }
        Ok(ActorForwardMotionTable {
            maximum: 0x3000,
            minimum: -0x1000,
            acceleration: 0x2000,
            deceleration: 0x2000,
        })
    }

    pub(super) fn selector12_angular_table(
        &self,
        actor: ObjectId,
    ) -> Result<ActorAngularMotionTable, ActorMotionError> {
        let state = self
            .selector12_actor(actor)
            .map_err(|_| ActorMotionError::UnsupportedSelector12Tables(actor))?;
        if state.table_294 != 0x0053_a090 {
            return Err(ActorMotionError::UnsupportedSelector12Tables(actor));
        }
        Ok(ActorAngularMotionTable {
            maximum: 0x10000,
            response: 0x20000,
        })
    }

    pub(super) fn selector12_lateral_table(
        &self,
        actor: ObjectId,
    ) -> Result<ActorLateralMotionTable, ActorMotionError> {
        let state = self
            .selector12_actor(actor)
            .map_err(|_| ActorMotionError::UnsupportedSelector12Tables(actor))?;
        if state.table_28c != 0x0052_fe40 {
            return Err(ActorMotionError::UnsupportedSelector12Tables(actor));
        }
        Ok(ActorLateralMotionTable {
            threshold: 10_000,
            rate: 1000,
        })
    }

    /// 00426860. Project signed velocity on Z, bound size-normalized control,
    /// then add the limited world-space forward increment with WORD wrapping.
    pub fn apply_actor_forward_motion(
        &mut self,
        actor: ObjectId,
        request: ActorForwardMotionRequest,
    ) -> Result<(), ActorMotionError> {
        let size = i32::from(self.object(actor)?.base.word_28);
        let table = request.table;
        let desired = divide(request.desired_velocity.wrapping_shl(12), size, 0x0042_6874)?;
        let desired = if desired > table.maximum {
            table.maximum
        } else if desired < table.minimum {
            table.minimum
        } else {
            desired
        };
        let state = self.actor(actor)?;
        let basis = state.rotating.basis;
        let projected = project_velocity(state.words_8e_92, basis, 7);
        let mut current = divide(projected.wrapping_shl(12), size, 0x0042_68db)?;
        let mut difference = desired.wrapping_sub(current);
        if difference == 0 {
            return Ok(());
        }
        if difference > 0 {
            current = current.max(0);
            let mut limit =
                (table.acceleration.wrapping_mul(request.step_units as i32) >> 15) as i16 as i32;
            if current < table.maximum {
                limit = limit.wrapping_add(divide(
                    table.maximum.wrapping_sub(current).wrapping_mul(limit),
                    table.maximum,
                    0x0042_6911,
                )?);
            }
            if difference > limit {
                difference = limit;
            }
        } else {
            let mut limit =
                (table.deceleration.wrapping_mul(request.step_units as i32) >> 15) as i16 as i32;
            if table.minimum < 0 {
                current = current.min(0);
                if current > table.minimum {
                    limit = limit.wrapping_add(divide(
                        table.minimum.wrapping_sub(current).wrapping_mul(limit),
                        table.minimum,
                        0x0042_6948,
                    )?);
                }
            }
            if difference < limit.wrapping_neg() {
                difference = limit.wrapping_neg();
            }
        }
        let increment = (difference.wrapping_mul(request.power) >> 16).wrapping_mul(size) >> 12;
        let velocity = &mut self.actor_mut(actor)?.words_8e_92;
        for (component, word) in velocity.iter_mut().enumerate() {
            *word = word
                .wrapping_add(ftol(f64::from(increment) * coefficient(basis, 7, component)) as u16);
        }
        Ok(())
    }

    /// 00426760. Its returned signed lateral correction is also the amount
    /// projected back onto X and subtracted from the three velocity WORDs.
    pub fn apply_actor_lateral_motion(
        &mut self,
        actor: ObjectId,
        request: ActorLateralMotionRequest,
    ) -> Result<i32, ActorMotionError> {
        let state = self.actor(actor)?;
        let basis = state.rotating.basis;
        let mut correction = project_velocity(state.words_8e_92, basis, 1);
        if correction == 0 {
            return Ok(0);
        }
        let threshold = ((request.table.threshold >> 5) as i32).wrapping_mul(request.power) >> 16;
        let limit = || {
            ((request.table.rate.wrapping_mul(request.step_units as i32) >> 15) as i16 as i32)
                .wrapping_mul(request.power)
        };
        if correction < threshold.wrapping_neg() {
            correction = limit().wrapping_neg() >> 16;
        } else if correction > threshold {
            correction = limit() >> 16;
        }
        let velocity = &mut self.actor_mut(actor)?.words_8e_92;
        for (component, word) in velocity.iter_mut().enumerate() {
            *word = word.wrapping_sub(
                ftol(f64::from(correction) * coefficient(basis, 1, component)) as u16,
            );
        }
        Ok(correction)
    }

    /// 00426AE0, including its null-axis local-Y and nonnull world-axis paths.
    /// Power zero returns before any actor/table read or signed divide.
    pub fn apply_actor_angular_motion(
        &mut self,
        actor: ObjectId,
        request: ActorAngularMotionRequest,
    ) -> Result<(), ActorMotionError> {
        if request.power == 0 {
            return Ok(());
        }
        let table = request.table;
        let normalized = divide(
            request.desired_turn.wrapping_shl(10),
            table.maximum,
            0x0042_6b03,
        )?;
        let magnitude = normalized.wrapping_abs();
        let threshold = divide(table.maximum.wrapping_shl(10), table.response, 0x0042_6b12)?;
        let mut desired = table.maximum;
        if magnitude <= threshold {
            desired = divide(desired.wrapping_mul(magnitude), threshold, 0x0042_6b21)?;
        }
        if request.desired_turn < 0 {
            desired = desired.wrapping_neg();
        }
        let state = self.actor(actor)?;
        let basis = state.rotating.basis;
        // 00490870 projects the world angular vector onto each local axis;
        // this DWORD-input path rounds nearest-even, unlike __ftol above.
        let current = transform_world_dwords(basis, state.words_94_9c.map(|word| word as i32))[1];
        let mut difference = desired.wrapping_sub(current);
        let mut limit = ((table.response.wrapping_mul(request.step_units as i32) >> 15) as i16
            as i32)
            .wrapping_mul(request.power)
            >> 16;
        if (current ^ difference) < 0 {
            limit = limit.wrapping_mul(6) >> 2;
        }
        if difference < 0 {
            limit = limit.wrapping_neg();
            if difference < limit {
                difference = limit;
            }
        } else if difference > limit {
            difference = limit;
        }
        let increment = match request.axis {
            ActorAngularAxis::LocalY => transform_local_dwords(basis, [0, difference, 0]),
            ActorAngularAxis::WorldVector(vector) => {
                vector.map(|value| ftol(f64::from(difference) * f64::from(value)))
            }
        };
        let angular = &mut self.actor_mut(actor)?.words_94_9c;
        for (word, increment) in angular.iter_mut().zip(increment) {
            *word = word.wrapping_add(increment as u32);
        }
        Ok(())
    }
}

pub(super) fn divide(
    numerator: i32,
    denominator: i32,
    address: u32,
) -> Result<i32, ActorMotionError> {
    numerator
        .checked_div(denominator)
        .ok_or(ActorMotionError::Division {
            address,
            numerator,
            denominator,
        })
}

fn coefficient(basis: NativeBasis, first: usize, component: usize) -> f64 {
    f64::from(f32::from_bits(basis.words[first + component]))
}

fn project_velocity(words: [u16; 3], basis: NativeBasis, first: usize) -> i32 {
    let [x, y, z] = words.map(|word| f64::from(word as i16));
    ftol(
        (y * coefficient(basis, first, 1) + z * coefficient(basis, first, 2))
            + x * coefficient(basis, first, 0),
    )
}

/// 00511BA8 temporarily selects truncation, writes a QWORD, then returns
/// the low DWORD. Masked invalid conversion therefore returns DWORD zero.
pub(crate) fn ftol(value: f64) -> i32 {
    integral_low_dword(value.trunc())
}
fn fistp(value: f64) -> i32 {
    integral_low_dword(value.round_ties_even())
}
fn integral_low_dword(value: f64) -> i32 {
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if value.is_finite() && (-BOUND..BOUND).contains(&value) {
        value as i64 as i32
    } else {
        0
    }
}

/// Complete 00490870 signed-DWORD projection, nearest-even QWORD/lowDWORD.
pub(in crate::object_runtime::actor) fn transform_world_dwords(
    basis: NativeBasis,
    world: [i32; 3],
) -> [i32; 3] {
    let [x, y, z] = world.map(f64::from);
    std::array::from_fn(|axis| {
        fistp(
            (x * coefficient(basis, 1 + axis * 3, 0) + y * coefficient(basis, 1 + axis * 3, 1))
                + z * coefficient(basis, 1 + axis * 3, 2),
        )
    })
}

/// Complete 004907E0 local-DWORD transformation, ordered X/Y/Z products.
pub(in crate::object_runtime::actor) fn transform_local_dwords(
    basis: NativeBasis,
    local: [i32; 3],
) -> [i32; 3] {
    let [x, y, z] = local.map(f64::from);
    std::array::from_fn(|component| {
        fistp(
            (x * coefficient(basis, 1, component) + y * coefficient(basis, 4, component))
                + z * coefficient(basis, 7, component),
        )
    })
}

/// 00490970: each row uses (Z*rowZ + Y*rowY) + X*rowX, then nearest-even
/// QWORD FISTP and the low DWORD. 00490A80's WORD result and order differ.
pub(super) fn inverse_point_490970(basis: NativeBasis, world: [i16; 3]) -> [i32; 3] {
    let [x, y, z] = world.map(f64::from);
    [1usize, 4, 7].map(|first| {
        let c = |offset| f64::from(f32::from_bits(basis.words[first + offset]));
        let value = ((z * c(2) + y * c(1)) + x * c(0)).round_ties_even();
        const BOUND: f64 = 9_223_372_036_854_775_808.0;
        if value.is_finite() && (-BOUND..BOUND).contains(&value) {
            value as i64 as i32
        } else {
            0
        }
    })
}

#[cfg(test)]
#[path = "kinematics_tests.rs"]
mod tests;
