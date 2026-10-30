//! `00407BB0`, the eyes' `+A4` (selector 77's table `0051FBB0` and selector
//! 79's `0051FC8C`): a thrown weapon striking an eye's tip marks the eye with
//! its thrower, and the eye's next update deals itself 10,000 damage from
//! that source (`00407CA0`, `00408AB0`).

use inf_formats::native_basis::isqrt_48efc0;

use super::super::{DerivedActorClass, DerivedActorError};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

/// `00407C51`: the tip's reach.
const TIP_REACH: i32 = 500;
/// `00407C7F`: the square of the reach from the partner's own axis, 150.
const AXIS_REACH_SQUARED: i32 = 0x57e4;

impl ObjectArena {
    /// `00407BB0` for a partner whose `+68` answers, from the eye's state 2
    /// on, given the stalk's tip (`+D8`): within 500 of the tip, and with the
    /// tip within 150 of the partner's own axis (the tip in its frame,
    /// `0047A660`), the eye keeps the partner's `+88` at `+1B0`.
    pub fn selector77_struck(
        &mut self,
        eye: ObjectId,
        partner: ObjectId,
        tip: [u16; 3],
    ) -> Result<(), DerivedActorError> {
        let position = self
            .object(partner)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(partner))?;
        let offset: [i16; 3] =
            std::array::from_fn(|axis| tip[axis].wrapping_sub(position[axis]) as i16);
        let squared = offset.iter().fold(0_i32, |sum, &axis| {
            sum.wrapping_add(i32::from(axis) * i32::from(axis))
        });
        // 00407C38: a sum past 2^31 takes the halved root, doubled.
        let distance = if squared < 0 {
            (isqrt_48efc0(((squared as u32) >> 2) as i32) << 1) as i32
        } else {
            isqrt_48efc0(squared) as i32
        };
        if distance >= TIP_REACH {
            return Ok(());
        }
        let [x, y, _] = self.actor(partner)?.rotating.basis.inverse_words(offset);
        if i32::from(x) * i32::from(x) + i32::from(y) * i32::from(y) >= AXIS_REACH_SQUARED {
            return Ok(());
        }
        let thrower = self.actor(partner)?.owner_handle;
        match &mut self.derived_actor_mut(eye)?.class {
            DerivedActorClass::Selector77(state) => state.handle_1b0 = thrower,
            _ => return Err(DerivedActorError::NotSelector77(eye)),
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "contact_tests.rs"]
mod tests;
