//! Selector 52, the portable mining rig: the shared 0047A7A0 with the final
//! table 00526714 (case 0x34, 0x144 bytes). Its table is the shared actor's
//! 00527C00 but for +00 (004110E0, the same 0047AA60 teardown), +04 (the
//! commands 00454880), +14 (the model values 00454720), +24 (the update
//! 00454630), +8C (004547E0, its pickup and drop, which the vehicle's carry
//! owns for selector 12 alone) and +98 (004546F0, messages 7 and 8).
//!
//! A course target's after-load sends its linked rigs message 7 (0043B720),
//! and while any link holds a rig it calls the resource mines within 2000
//! with message 0xF every update, which starts their drops. A free rig that
//! runs (+138) drains its level +140, or fills it while +142 is set; full,
//! it calls the mines within 0x400 and stops filling when none answers.

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};
use thiserror::Error;

use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension,
};
use crate::object_runtime::{mark_flag_change, ObjectArena, ObjectError, ObjectId};

/// 00454641: a held rig calls the mines within 2000.
const HELD_CALL_RADIUS: u32 = 2000;
/// 004546A0: a full rig calls the mines within 0x400.
const FULL_CALL_RADIUS: u32 = 0x400;
/// The message both calls send (0045463F, 0045469E), which the mines'
/// 00454AB0 answers.
pub const SELECTOR52_MINE_CALL: i32 = 0xf;

/// Case 0x34's writes after 0047A7A0 (0046102E..0046104F).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector52State {
    /// BYTE +138, 1 at construction. Nonzero runs a free rig; command 16
    /// stores its BYTE == 0.
    pub running_138: u8,
    /// DWORD +13C, the links holding the rig: message 7 adds one and 8
    /// takes one.
    pub links_13c: u32,
    /// WORD +140, the level the update drains or fills, 0..0xFFFF.
    pub level_140: u16,
    /// BYTE +142: nonzero fills the level.
    pub filling_142: u8,
}

impl Selector52State {
    fn new() -> Self {
        Self {
            running_138: 1,
            links_13c: 0,
            level_140: 0,
            filling_142: 0,
        }
    }
}

/// The rig's calls to the mines.
pub trait Selector52Host {
    type Error;
    /// 0047DD20(radius, 0xF, 0) from the rig: every actor within the radius
    /// gets its +98, and the answers add up.
    fn call_mines(
        &mut self,
        arena: &mut ObjectArena,
        rig: ObjectId,
        radius: u32,
    ) -> Result<u32, Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector52Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("mining rig {rig:?} call 0047DD20({radius}, 0xF, 0): {error}")]
    Call {
        rig: ObjectId,
        radius: u32,
        error: E,
    },
}

impl ObjectArena {
    /// Case 0x34: 0x144 bytes, the shared 0047A7A0, then the rig's running
    /// BYTE 1 and its zero links, level and filling BYTE.
    pub fn allocate_selector52(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector52(Box::new(Selector52State::new()));
        Ok(object)
    }

    pub fn selector52(&self, object: ObjectId) -> Result<&Selector52State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector52(state) => Ok(state),
            _ => Err(ObjectError::NotSelector52(object)),
        }
    }

    pub(in crate::object_runtime) fn selector52_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector52State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector52(state) => Ok(state),
            _ => Err(ObjectError::NotSelector52(object)),
        }
    }

    /// 00454880: the shared 0047ADA0 through code 15 (signed), then code 16
    /// stores its BYTE == 0 at +138 and clears flag 0x40, marking the low
    /// nine bits' change; any other code refuses without a read.
    pub fn apply_selector52_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(52) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector52(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        match (code, &command.payload) {
            (16, InstanceCommandPayload::Selector52Stopped(value)) => {
                self.selector52_mut(object)?.running_138 = u8::from(value.value == 0);
                let base = &mut self.owned_mut(object)?.state.base;
                let flags = base.flags & !0x40;
                mark_flag_change(base, flags);
                Ok(true)
            }
            (16, _) => Err(ActorCommandError::Payload(code)),
            _ => Ok(false),
        }
    }

    /// 004546F0, the rig's +98: message 7 adds a link and 8 takes one,
    /// each wrapping; every message answers 0.
    pub fn selector52_message(&mut self, rig: ObjectId, code: i32) -> Result<(), ObjectError> {
        let state = self.selector52_mut(rig)?;
        match code {
            7 => state.links_13c = state.links_13c.wrapping_add(1),
            8 => state.links_13c = state.links_13c.wrapping_sub(1),
            _ => {}
        }
        Ok(())
    }

    /// 00454630, the rig's +24. A held rig calls the mines within 2000 and
    /// does nothing else. A free running rig takes `delta / 100` (unsigned)
    /// units: draining, a level above them (signed) loses them, else it
    /// empties; filling, a level below 0xFFFF less them (signed) gains them,
    /// else it is full (0xFFFF) and calls the mines within 0x400, and a zero
    /// answer stops the filling.
    pub fn tick_selector52<H: Selector52Host + ?Sized>(
        &mut self,
        rig: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector52Error<H::Error>> {
        let call = |arena: &mut ObjectArena, host: &mut H, radius| {
            host.call_mines(arena, rig, radius)
                .map_err(|error| Selector52Error::Call { rig, radius, error })
        };
        let state = self.selector52(rig)?;
        if state.links_13c != 0 {
            call(self, host, HELD_CALL_RADIUS)?;
            return Ok(());
        }
        if state.running_138 == 0 {
            return Ok(());
        }
        let units = delta / 100;
        let level = state.level_140;
        if state.filling_142 == 0 {
            self.selector52_mut(rig)?.level_140 = if i32::from(level) > units as i32 {
                level.wrapping_sub(units as u16)
            } else {
                0
            };
            return Ok(());
        }
        if i32::from(level) < 0xffff_u32.wrapping_sub(units) as i32 {
            self.selector52_mut(rig)?.level_140 = level.wrapping_add(units as u16);
            return Ok(());
        }
        self.selector52_mut(rig)?.level_140 = 0xffff;
        if call(self, host, FULL_CALL_RADIUS)? == 0 {
            self.selector52_mut(rig)?.filling_142 = 0;
        }
        Ok(())
    }
}

/// 00454720, the rig's +14: model values from its level, for selectors 1
/// to 9 (a jump table at 004547BC); every other selector answers 0. The
/// ramps clamp to 0..0xFFFF as signed DWORDs.
pub(crate) fn selector52_value(state: &Selector52State, selector: u32) -> u16 {
    let level = i32::from(state.level_140);
    let clamp = |value: i32| value.clamp(0, 0xffff) as u16;
    match selector {
        1 => clamp((3 * level - 0x7ffe) << 1),
        2 => clamp((3 * level - 0xfffc) << 1),
        3 => (6 * level).clamp(0x1800, 0xffff) as u16,
        4 => clamp((3 * level - 0x17ffa) << 1),
        5 => clamp((3 * level - 0x1bff9) << 2),
        8 => clamp((3 * level - 0x1fff8) << 1),
        9 => u16::from(state.level_140 == 0xffff),
        _ => 0,
    }
}

#[cfg(test)]
#[path = "selector52_tests.rs"]
mod tests;
