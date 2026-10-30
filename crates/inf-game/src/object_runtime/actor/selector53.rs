//! Selector 53, the pickup spawner: the shared 0047A7A0 with the final table
//! 00526648. Every few seconds it drops a pickup near itself (0046D870)
//! until its count runs out. Its table is the shared actor's 00527C00 but
//! for +00 (004110E0, the same 0047AA60 teardown), +04 (the commands
//! 00454A30), +24 (the update 004548E0), +70 (00404610, which returns the
//! spawner) and +98 (00454AB0, which acts on message 0xF only).

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};
use thiserror::Error;

use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ObjectArena, ObjectError, ObjectId, PickupBirthHost, PickupBirthRequest, PickupError,
    },
};

/// The constructor's writes after 0047A7A0 (00461091..004610AD).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector53State {
    /// Signed pickup kind; the spawner drops pickup kind +138 + 13.
    pub kind_138: u8,
    /// Signed count of pickups left to drop.
    pub count_139: u8,
    /// Signed seconds between drops.
    pub seconds_13a: u8,
    /// Nonzero restarts the timer after each drop.
    pub repeat_13b: u8,
    /// Signed microseconds to the next drop. Zero arms the timer.
    pub timer_13c: u32,
}

impl Selector53State {
    fn new() -> Self {
        Self {
            kind_138: 1,
            count_139: 10,
            seconds_13a: 3,
            repeat_13b: 0,
            timer_13c: 0,
        }
    }

    /// The signed seconds BYTE times 1,000,000, as a wrapping DWORD.
    fn period(&self) -> u32 {
        (i32::from(self.seconds_13a as i8)).wrapping_mul(1_000_000) as u32
    }
}

#[derive(Debug, Error)]
pub enum Selector53Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("pickup spawner {0:?} has no position +32")]
    MissingPosition(ObjectId),
    #[error("pickup spawner {spawner:?} birth 0046D870: {error}")]
    Birth {
        spawner: ObjectId,
        error: PickupError<E>,
    },
}

impl ObjectArena {
    /// 0046107C..004610B7: 0x140 bytes, the shared 0047A7A0, then the
    /// spawner's defaults: kind 1, ten pickups, 3 s apart, no repeat and a
    /// zero timer.
    pub fn allocate_selector53(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector53(Box::new(Selector53State::new()));
        Ok(object)
    }

    pub fn selector53(&self, object: ObjectId) -> Result<&Selector53State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector53(state) => Ok(state),
            _ => Err(ObjectError::NotSelector53(object)),
        }
    }

    fn selector53_mut(&mut self, object: ObjectId) -> Result<&mut Selector53State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector53(state) => Ok(state),
            _ => Err(ObjectError::NotSelector53(object)),
        }
    }

    /// 00454A30: the shared 0047ADA0 through code 15, one BYTE each for
    /// codes 16..19, and a refusal above.
    pub fn apply_selector53_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(53) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector53(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let state = self.selector53_mut(object)?;
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (16, Selector53Kind138(value)) => state.kind_138 = value.value,
            (17, Selector53Count139(value)) => state.count_139 = value.value,
            (18, Selector53Seconds13a(value)) => state.seconds_13a = value.value,
            (19, Selector53Repeat13b(value)) => state.repeat_13b = value.value,
            (16..=19, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 00454AB0, message 0xF (a mining rig's call): a spawner whose timer is
    /// zero and whose count is nonzero arms the timer (signed seconds times
    /// 1,000,000) and joins the update list (00479F20(1)). It then answers
    /// its signed kind BYTE when its flag 2 is set, and 0 otherwise.
    pub fn call_selector53(&mut self, spawner: ObjectId) -> Result<u32, ObjectError> {
        let state = self.selector53_mut(spawner)?;
        if state.timer_13c == 0 && state.count_139 != 0 {
            state.timer_13c = state.period();
            self.set_flag1(spawner, true)?;
        }
        let kind = self.selector53(spawner)?.kind_138;
        Ok(if self.object(spawner)?.base.flags & 2 != 0 {
            i32::from(kind as i8) as u32
        } else {
            0
        })
    }

    /// 004548E0, the spawner's +24. A timer above the delta (signed) counts
    /// down. A zero timer leaves the update list (00479F20(0)) unless the
    /// repeat flag is set, which arms it. Otherwise the spawner takes one
    /// from its count (00454B10(1)); a nonzero take drops pickup kind +138 +
    /// 13 with value 1 at its own position moved by two process RNG draws
    /// in X then Z, each -0x200..0x1FF, and an emptied count leaves the
    /// update list. The timer then restarts, or stays zero without the
    /// repeat flag.
    pub fn tick_selector53<H: PickupBirthHost + ?Sized>(
        &mut self,
        spawner: ObjectId,
        delta: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector53Error<H::Error>> {
        let state = self.selector53_mut(spawner)?;
        let timer = state.timer_13c as i32;
        if timer > delta as i32 {
            state.timer_13c = timer.wrapping_sub(delta as i32) as u32;
            return Ok(());
        }
        if timer == 0 {
            if state.repeat_13b == 0 {
                self.set_flag1(spawner, false)?;
            } else {
                state.timer_13c = state.period();
            }
            return Ok(());
        }
        // 00454B10(1): take min(1, count), signed, from the count BYTE.
        let count = i32::from(state.count_139 as i8);
        let taken = count.min(1);
        state.count_139 = state.count_139.wrapping_sub(taken as u8);
        if taken != 0 {
            let kind = i32::from(state.kind_138 as i8).wrapping_add(13) as u16;
            let mut position = self
                .object(spawner)?
                .base
                .position_words
                .ok_or(Selector53Error::MissingPosition(spawner))?;
            for axis in [0, 2] {
                let offset = (name_entry_rng_next(rng) & 0x3ff) as u16;
                position[axis] = position[axis].wrapping_add(offset).wrapping_sub(0x200);
            }
            self.birth_timed_pickup(
                PickupBirthRequest {
                    position: &mut position,
                    kind: u32::from(kind),
                    value: 1,
                },
                host,
                rng,
            )
            .map_err(|error| Selector53Error::Birth { spawner, error })?;
            if self.selector53(spawner)?.count_139 == 0 {
                self.set_flag1(spawner, false)?;
            }
        }
        let state = self.selector53_mut(spawner)?;
        state.timer_13c = if state.repeat_13b == 0 {
            0
        } else {
            state.period()
        };
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector53_tests.rs"]
mod tests;
