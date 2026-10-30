//! 004627F0 route selection/integration and 00462DA0 integer distance.

use inf_formats::native_basis::{isqrt_48efc0, normalize_words, NativeLengthPrecision};

use super::{ObjectArena, ObjectError, ObjectId, Selector90Error, Selector90MotionMode};

pub(super) fn delta(target: [u16; 3], position: [u16; 3]) -> [i16; 3] {
    std::array::from_fn(|axis| target[axis].wrapping_sub(position[axis]) as i16)
}

fn squared(delta: [i16; 3]) -> u32 {
    delta.into_iter().fold(0_u32, |sum, value| {
        let value = i32::from(value);
        sum.wrapping_add(value.wrapping_mul(value) as u32)
    })
}

fn distance(delta: [i16; 3]) -> i32 {
    let value = squared(delta);
    if value & 0x8000_0000 == 0 {
        isqrt_48efc0(value as i32) as i32
    } else {
        isqrt_48efc0((value >> 2) as i32).wrapping_mul(2) as i32
    }
}

/// MSVCRT __ftol at 00511BA8 truncates a retained product to QWORD; these
/// consumers use only EAX. Masked integer-indefinite has low DWORD zero.
fn ftol_low_dword(value: f64) -> i32 {
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    let truncated = value.trunc();
    if truncated.is_finite() && (-BOUND..BOUND).contains(&truncated) {
        truncated as i64 as i32
    } else {
        0
    }
}

impl ObjectArena {
    pub(super) fn advance_selector90_route(
        &mut self,
        helper: ObjectId,
        microseconds: u32,
        mode: Selector90MotionMode,
    ) -> Result<bool, Selector90Error> {
        let Some(target) = self.selector90(helper)?.route_150 else {
            return Ok(false);
        };
        if microseconds < 1000 {
            return Ok(false);
        }
        let position = self
            .object(helper)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(helper))?;
        let mut route = self.object_position_target_data(target)?.object_14;
        let mut point = self.route_position(route)?;
        let mut direction = delta(point, position);
        let mut length = distance(direction);
        let threshold = self.selector90(helper)?.word_138.max(256);
        let next = if length < threshold {
            self.route_next(route)?
        } else if self.route_node(route)?.distance_squared == 0 {
            None
        } else if let Some(next) = self.route_next(route)? {
            let next_delta = delta(self.route_position(next)?, position);
            (squared(next_delta) < self.route_node(route)?.distance_squared).then_some(next)
        } else {
            None
        };
        if let Some(next) = next {
            route = next;
            self.route_node(route)?;
            let reference = self.selector90(helper)?.route_150.unwrap();
            let mut stored = self.object_position_target_data(reference)?;
            let replacement = self.replace_object_position_target(&mut stored, route);
            // Commit the native +14 prefix even when the later XYZ read fails.
            self.write_native_target(
                reference,
                crate::object_runtime::NativeTarget::ObjectPosition(stored),
            )?;
            replacement?;
            point = self.route_position(route)?;
            direction = delta(point, position);
            length = distance(direction);
        }
        if length <= 2 {
            self.actor_mut(helper)?.words_8e_92 = [0; 3];
            self.owned_mut(helper)?.state.base.position_words = Some(point);
            self.selector90_mut(helper)?.remainders_144_14c = [0; 3];
            return Ok(self.route_next(route)?.is_some());
        }
        let state = self.selector90(helper)?;
        let mut speed = state.word_13c;
        if self.route_next(route)?.is_none() {
            let milliseconds = (microseconds / 1000) as i32;
            let cap = length
                .wrapping_mul(1000)
                .checked_div(milliseconds)
                .ok_or(Selector90Error::ArithmeticFault)?;
            speed = speed.min(cap);
            if length < state.word_138 {
                let reduced = state
                    .word_13c
                    .wrapping_mul(length)
                    .checked_div(state.word_138)
                    .ok_or(Selector90Error::ArithmeticFault)?;
                speed = speed.min(reduced.max(32));
            }
        }
        let normalized = normalize_words(direction, NativeLengthPrecision::Retained);
        // Native IMUL first wraps to a DWORD; the following magic multiply
        // divides that unsigned DWORD, including a negative signed speed.
        let product = (speed as u32).wrapping_mul(microseconds);
        let mut next_position = position;
        match mode {
            Selector90MotionMode::FractionalThousandths => {
                let thousandths = (product / 1000) as i32;
                let increments = normalized
                    .map(|component| ftol_low_dword(f64::from(component) * f64::from(thousandths)));
                let state = self.selector90_mut(helper)?;
                for axis in 0..3 {
                    let accumulated = state.remainders_144_14c[axis].wrapping_add(increments[axis]);
                    let units = accumulated / 1000;
                    next_position[axis] = next_position[axis].wrapping_add(units as u16);
                    state.remainders_144_14c[axis] =
                        accumulated.wrapping_sub(units.wrapping_mul(1000));
                }
            }
            Selector90MotionMode::WholeUnits => {
                let units = (product / 1_000_000) as u16 as i16;
                for axis in 0..3 {
                    let increment = ftol_low_dword(f64::from(normalized[axis]) * f64::from(units));
                    next_position[axis] = next_position[axis].wrapping_add(increment as u16);
                }
            }
        }
        self.owned_mut(helper)?.state.base.position_words = Some(next_position);
        Ok(true)
    }
}
