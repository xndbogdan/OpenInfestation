//! Selector 37, 004099A0 over 0040D800 over the shared actor (final
//! 0051FF54). It reshapes a terrain strip over time; Level 1's three course
//! targets link to it. Construction, commands 0040A1D0, postinitialization
//! 0040E000, after-load 0040DFC0 and the 0040A180 link messages are owned,
//! and the +24 terrain update 00409A20 is in [`bridge`].

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};

use super::{
    family40d800::{Actor40d800State, FamilyLinkChange},
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension, FactoryError, FactoryStaffHost,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

mod bridge;
pub use bridge::{BridgeTerrain, Selector37Error, Selector37Host, BRIDGE_SOUND};

/// 004099A0's writes after 0040D800, with case 0x25's arguments 0x400,
/// 0x100 and 0. The terrain heights +174/+178 are first written by the
/// update and stay unknown until then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector37State {
    pub intermediate: Actor40d800State,
    /// Strip length and half-width in world units.
    pub word_158: u32,
    pub word_15c: u32,
    /// Signed height offset added to both sampled ends.
    pub word_160: i32,
    /// Seconds for the reshaping; +170 counts it down in microseconds.
    pub byte_164: u8,
    /// Strip direction 0..3.
    pub byte_165: u8,
    pub byte_166: u8,
    pub byte_167: u8,
    /// Set once the update has sampled both strip ends.
    pub byte_168: u8,
    pub word_16c: u32,
    pub word_170: u32,
    pub word_174: Option<i32>,
    pub word_178: Option<i32>,
    /// Signed WORD message index; 0xFFFF disables it.
    pub word_17c: u16,
    /// Grant gate passed to 004752E0 by the update.
    pub word_17e: u16,
}

impl Selector37State {
    fn new() -> Self {
        Self {
            intermediate: Actor40d800State::new(),
            word_158: 0x400,
            word_15c: 0x100,
            word_160: 0,
            byte_164: 4,
            byte_165: 0,
            byte_166: 0,
            byte_167: 0,
            byte_168: 0,
            word_16c: 0,
            word_170: 4_000_000,
            word_174: None,
            word_178: None,
            word_17c: u16::MAX,
            word_17e: 0,
        }
    }
}

impl ObjectArena {
    pub fn allocate_selector37(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector37(Box::new(Selector37State::new()));
        Ok(object)
    }

    pub fn selector37(&self, object: ObjectId) -> Result<&Selector37State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector37(state) => Ok(state),
            _ => Err(ObjectError::NotSelector37(object)),
        }
    }

    fn selector37_mut(&mut self, object: ObjectId) -> Result<&mut Selector37State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector37(state) => Ok(state),
            _ => Err(ObjectError::NotSelector37(object)),
        }
    }

    /// 0040A1D0: shared commands through 15, 0040D9C0 for 16..20, then the
    /// class's eight scalar stores.
    pub fn apply_selector37_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(37) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector37(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let state = self.selector37_mut(object)?;
        if let Some(result) = state.intermediate.apply_command(code, &command.payload) {
            let dirty_0200 = result.map_err(ActorCommandError::Payload)?;
            if dirty_0200 {
                self.owned_mut(object)?.state.base.dirty_word |= 0x0200;
            }
            return Ok(true);
        }
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (21, Selector37Word158(value)) => state.word_158 = u32::from(value.value),
            (22, Selector37Word15c(value)) => state.word_15c = u32::from(value.value),
            (23, Selector37SignedWord160(value)) => state.word_160 = i32::from(value.value),
            (24, Selector37Seconds164(value)) => {
                state.byte_164 = value.value;
                state.word_170 = u32::from(value.value) * 1_000_000;
            }
            (25, Selector37Flag166(value)) => state.byte_166 = u8::from(value.value != 0),
            (26, Selector37Word17c(value)) => state.word_17c = value.value,
            (27, Selector37Byte165(value)) => state.byte_165 = value.value,
            (28, Selector37Word17e(value)) => state.word_17e = value.value,
            (21..=28, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// `00468160`'s target arm against the bridge, whose `+D0` is
    /// `0040DAB0` itself: a bridge short of staff takes the scientist (its
    /// team, the slot flash, the scientist's disposal, one staff and, once
    /// full, its placement's record); a refusal sets the scientist's `+22E`
    /// bit 0.
    pub fn scientist_reaches_bridge<H: FactoryStaffHost>(
        &mut self,
        scientist: ObjectId,
        bridge: ObjectId,
        host: &mut H,
    ) -> Result<bool, FactoryError<H::Error>> {
        self.selector37(bridge)?;
        if self.take_scientist_staff(bridge, scientist, host)? {
            return Ok(true);
        }
        self.scientist_refused(scientist)?;
        Ok(false)
    }

    /// 0040A180, selector 37's +98. Message 2 with +167 clear, or message 3
    /// with +167 set, restarts +170 from the raw +164 BYTE (not scaled to
    /// microseconds). Every message then reaches 0040DE70.
    pub(super) fn selector37_message(
        &mut self,
        object: ObjectId,
        code: i32,
    ) -> Result<FamilyLinkChange, ObjectError> {
        let state = self.selector37_mut(object)?;
        let restart = match code {
            2 => state.byte_167 == 0,
            3 => state.byte_167 != 0,
            _ => false,
        };
        if restart {
            state.word_170 = u32::from(state.byte_164);
        }
        self.family40d800_message(object, code)
    }
}

#[cfg(test)]
#[path = "selector37_tests.rs"]
mod tests;
