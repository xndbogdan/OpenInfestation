//! Selector 11: constructor 004014F0, commands 00402330 and postinit 00402590.
//! Vtable 0051F080 inherits 00426BF0 payload/context initialization and 00426C80
//! admission. Its +34 after-load method is the actual no-op 00401310; update
//! 004019E0 and retarget 004015F0 live in `update`.

use inf_formats::{
    ConditionalInstanceDword, InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView,
};

use super::{DerivedActorClass, DerivedActorConstructionRequest, DerivedActorError};
use crate::object_runtime::{
    actor::{
        behavior::select_actor_behavior, family40d800::FamilyLinkChange, ActorCommandContext,
        ActorCommandError, ActorConstructionError, ActorStateChangeRequest,
    },
    ObjectArena, ObjectError, ObjectId,
};

mod retirement;
mod update;
pub use retirement::{
    Selector11RadiusBlock, Selector11RadiusError, Selector11RadiusOrigin,
    Selector11RetirementError, Selector11RetirementHost,
};
pub use update::{
    Selector11ScanHost, Selector11TickRequest, Selector11UpdateError, Selector11UpdateHost,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector11State {
    pub word_184: Option<u32>,
    pub word_188: u32,
    pub word_18c: u32,
    pub word_190: u32,
    pub word_194: u32,
    pub word_198: u32,
    pub word_19c: Option<u32>,
    pub word_1a0: u16,
    pub byte_1a2: u8,
    /// Constructor leaves this byte untouched; command 20 copies it verbatim.
    pub byte_1a3: Option<u8>,
    pub word_1a4: u16,
    pub word_1a6: u16,
    pub word_1a8: u16,
    pub word_1aa: Option<u16>,
    pub word_1ac: Option<u16>,
    pub word_1ae: u16,
    pub word_1b0: u16,
    pub word_1b2: u16,
}

impl ObjectArena {
    /// 004014F0 adds no RNG draw or owned allocation above 00425E60. Unknown
    /// allocation bytes remain unknown rather than borrowing constructor defaults.
    pub fn allocate_selector11_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_derived_actor(request, rng)?;
        let word_19c = self
            .actor_allocation
            .initial_dword(super::super::allocation::ActorAllocationDwordCarry::Selector11Aim19c);
        self.derived_actor_mut(object)?.class =
            DerivedActorClass::Selector11(Box::new(Selector11State {
                word_184: None,
                word_188: 0,
                word_18c: 0,
                word_190: 0,
                word_194: 0,
                word_198: 0,
                word_19c,
                word_1a0: 2000,
                byte_1a2: 9,
                byte_1a3: None,
                word_1a4: 50,
                word_1a6: 512,
                word_1a8: 256,
                word_1aa: None,
                word_1ac: None,
                word_1ae: 0,
                word_1b0: 0,
                word_1b2: 0,
            }));
        Ok(object)
    }

    pub fn selector11_actor(
        &self,
        object: ObjectId,
    ) -> Result<&Selector11State, DerivedActorError> {
        match &self.derived_actor(object)?.class {
            DerivedActorClass::Selector11(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector11(object)),
        }
    }

    /// 00402420, selector 11's +98, for the link codes. Message 7 counts a
    /// link in DWORD +198 and, in state 2, locks through 0047D670(0, +84, 3).
    /// Message 8 clears +190 and uncounts; the last link unlocks state 3
    /// through 0047D670(0, +84, 2). Its other arms (codes B..D and 0x15) are
    /// not link codes.
    pub(in crate::object_runtime) fn selector11_link_message(
        &mut self,
        object: ObjectId,
        code: i32,
    ) -> Result<FamilyLinkChange, ObjectError> {
        let lifecycle = self.actor(object)?.word_c4 as i32;
        let source = self.actor(object)?.handle;
        let DerivedActorClass::Selector11(state) = &mut self.derived_actor_mut(object)?.class
        else {
            return Err(ObjectError::LinkMessageBoundary(object));
        };
        let request = |target_state| ActorStateChangeRequest {
            target_state,
            argument: 0,
            source,
        };
        match code {
            7 => {
                state.word_198 = state.word_198.wrapping_add(1);
                if lifecycle == 2 && (state.word_198 as i32) > 0 {
                    return Ok(FamilyLinkChange::Lock(request(3)));
                }
            }
            8 => {
                state.word_190 = 0;
                state.word_198 = state.word_198.wrapping_sub(1);
                if (state.word_198 as i32) < 1 && lifecycle == 3 {
                    return Ok(FamilyLinkChange::Unlock(request(2)));
                }
            }
            _ => return Err(ObjectError::LinkMessageBoundary(object)),
        }
        Ok(FamilyLinkChange::None)
    }

    /// 00402330 delegates signed codes <=18 to the inherited command owner.
    /// The behavior-null branch of code 21 succeeds before reading its operand.
    pub fn apply_selector11_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(11)
            || !matches!(
                self.derived_actor(object)?.class,
                DerivedActorClass::Selector11(_)
            )
        {
            return Err(ActorCommandError::WrongTarget);
        }
        let code = command.code.value;
        if code <= 18 {
            return self.apply_derived_actor_command(object, command, context);
        }
        if code == 21 {
            let Some(behavior) = &mut self.derived_actor_mut(object)?.behavior_140 else {
                return Ok(true);
            };
            let InstanceCommandPayload::Selector11BehaviorWord04(value) = &command.payload else {
                return Err(ActorCommandError::Payload(code));
            };
            let value = match value {
                ConditionalInstanceDword::Available(field) => field.value,
                ConditionalInstanceDword::OutsideAllocation { allocation_offset } => {
                    return Err(ActorCommandError::MissingConditionalDword {
                        code,
                        allocation_offset: *allocation_offset,
                    });
                }
            };
            behavior.word_04 = value;
            // 004023AA/004023AF read/write +08 unchanged, then +0C becomes 1.
            behavior.byte_0c = 1;
            return Ok(true);
        }
        let DerivedActorClass::Selector11(state) = &mut self.derived_actor_mut(object)?.class
        else {
            unreachable!("class checked before command");
        };
        use InstanceCommandPayload::*;
        let toggle = match (code, &command.payload) {
            (19, Selector11Flag02(value)) => Some((2, value.value != 0)),
            (20, Selector11Block1a0(value)) => {
                let bytes = value.value;
                state.word_1a0 = u16::from_le_bytes([bytes[0], bytes[1]]);
                state.byte_1a2 = bytes[2];
                state.byte_1a3 = Some(bytes[3]);
                state.word_1a4 = u16::from_le_bytes([bytes[4], bytes[5]]);
                state.word_1a6 = u16::from_le_bytes([bytes[6], bytes[7]]);
                state.word_1a8 = u16::from_le_bytes([bytes[8], bytes[9]]);
                None
            }
            (22, Selector11Flag04(value)) => Some((4, value.value != 0)),
            (23, Selector11Flag10(value)) => Some((0x10, value.value != 0)),
            (19..=23, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        };
        if let Some((mask, set)) = toggle {
            state.word_1ae = (state.word_1ae & !mask) | if set { mask } else { 0 };
        }
        Ok(true)
    }

    /// 00402590 selects the current behavior, then runs shared 0047AAD0.
    /// Factory publication and the distinct +40 network method are separate.
    pub fn finish_selector11_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        self.selector11_actor(object)?;
        let flags = self.object(object)?.base.flags;
        let selector_b4 = self.actor(object)?.word_b4;
        if let Some(behavior) = &mut self.derived_actor_mut(object)?.behavior_140 {
            select_actor_behavior(behavior, flags, selector_b4);
        }
        self.finish_actor_initialization(object)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector11_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "selector11_native_tests.rs"]
mod native_tests;
