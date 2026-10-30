//! Exact shared actor virtual +04, 0047ADA0, and its 00479E10 command walk.

use inf_formats::{
    grant_is_owned, ConditionalInstanceWord, GrantClosureError, InstanceCommandPayload,
    InstanceCommandTarget, InstanceCommandView, GRANT_MASK_WORDS,
};
use thiserror::Error;

use super::super::{
    construction::{run_commands, CommandPolicy},
    ObjectArena, ObjectCommandRejection, ObjectConstructionError, ObjectError, ObjectId,
    ObjectInstanceCommands,
};
use super::initialization::map_actor_selector;
use super::{ActorExtension, InstanceCommandAllocationError};
use crate::ResidentRuntime;

#[derive(Clone, Copy, Debug)]
pub struct ActorCommandContext<'a> {
    pub runtime: &'a ResidentRuntime,
    /// Actual first-player +A0 mask (PlayerSaveState::owner_a0), selected by
    /// 00488AA0's list lookup. None means no player, not an all-zero mask.
    pub first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
    /// Current H12 surface, consumed only by commands that sample terrain.
    pub terrain: Option<&'a crate::terrain_runtime::TerrainRuntime>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorCommandError {
    #[error("actor command requires the current terrain surface")]
    MissingTerrain,
    #[error(transparent)]
    Selector30(#[from] super::Selector30Error),
    #[error(transparent)]
    Derived(#[from] super::DerivedActorError),
    #[error(transparent)]
    Selector60(#[from] super::Selector60Error),
    #[error("actor command decoder target does not match its implemented dispatch")]
    WrongTarget,
    #[error("command {0} has an incompatible decoded payload")]
    Payload(i32),
    #[error("command {0} grants an ID and needs the executable grant table")]
    MissingGrantTable(i32),
    #[error("actor command {code} reached missing conditional WORD at {allocation_offset:#x}")]
    MissingConditionalWord { code: i32, allocation_offset: u32 },
    #[error("actor command {code} reached missing conditional DWORD at {allocation_offset:#x}")]
    MissingConditionalDword { code: i32, allocation_offset: u32 },
    #[error(transparent)]
    Grant(#[from] GrantClosureError),
    #[error(transparent)]
    Allocation(#[from] InstanceCommandAllocationError),
    #[error(transparent)]
    Object(#[from] ObjectError),
}

impl ObjectArena {
    /// Execute one shared command. False is native rejection; data/lifetime
    /// failures are explicit host errors. Disposal commands still return true.
    pub fn apply_actor_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        let actor = self.actor(object)?;
        let shared_dispatch = command.target == InstanceCommandTarget::SharedActor;
        let selector88_delegation = command.target == InstanceCommandTarget::Constructor(88)
            && matches!(actor.extension, ActorExtension::Selector88(_))
            && command.code.value <= 15;
        // 00454A30, the pickup spawner's +04, passes codes through 15 on.
        let selector53_delegation = command.target == InstanceCommandTarget::Constructor(53)
            && matches!(actor.extension, ActorExtension::Selector53(_))
            && command.code.value <= 15;
        // 00458C40, the Bomb's +04, passes codes through 15 on.
        let selector29_delegation = command.target == InstanceCommandTarget::Constructor(29)
            && matches!(&actor.extension, ActorExtension::BehaviorResource(state) if state.selector == 29)
            && command.code.value <= 15;
        // 00454880, the mining rig's +04, passes codes through 15 on.
        let selector52_delegation = command.target == InstanceCommandTarget::Constructor(52)
            && matches!(actor.extension, ActorExtension::Selector52(_))
            && command.code.value <= 15;
        // 00405D10, the Boulders' +04, passes codes through 15 on.
        let selector59_delegation = command.target == InstanceCommandTarget::Constructor(59)
            && matches!(actor.extension, ActorExtension::Selector59(_))
            && command.code.value <= 15;
        // 0042D6F0, the zone's +04, passes codes through 15 on.
        let selector74_delegation = command.target == InstanceCommandTarget::Constructor(74)
            && matches!(actor.extension, ActorExtension::Selector74(_))
            && command.code.value <= 15;
        // 0043C2A0, the capture node's +04, passes codes through 15 on.
        let selector84_delegation = command.target == InstanceCommandTarget::Constructor(84)
            && matches!(actor.extension, ActorExtension::Selector84(_))
            && command.code.value <= 15;
        // 00478860, the Force Field's +04, passes codes through 15 on.
        let selector18_delegation = command.target == InstanceCommandTarget::Constructor(18)
            && matches!(actor.extension, ActorExtension::Selector18(_))
            && command.code.value <= 15;
        // 0046E4F0, the Prison Unit's +04, passes codes through 15 on; its
        // code 16, the radius, is not ported.
        let selector17_delegation = command.target == InstanceCommandTarget::Constructor(17)
            && matches!(actor.extension, ActorExtension::Selector17(_))
            && command.code.value <= 15;
        // 00473F70, the Remote Beamer's +04, passes codes through 15 on;
        // its code 16, the range, is not ported.
        let selector55_delegation = command.target == InstanceCommandTarget::Constructor(55)
            && matches!(actor.extension, ActorExtension::Selector55(_))
            && command.code.value <= 15;
        // 00411100, the barrier's +04, passes codes through 15 on.
        let selector69_delegation = command.target == InstanceCommandTarget::Constructor(69)
            && matches!(actor.extension, ActorExtension::Selector69(_))
            && command.code.value <= 15;
        // 0048BBF0, 0040A1D0 and 0043B7D0 reach 0047ADA0 through 0040D9C0.
        let family40d800_delegation = matches!(
            (command.target, &actor.extension),
            (
                InstanceCommandTarget::Constructor(15),
                ActorExtension::Selector15(_)
            ) | (
                InstanceCommandTarget::Constructor(30),
                ActorExtension::Selector30(_)
            ) | (
                InstanceCommandTarget::Constructor(37),
                ActorExtension::Selector37(_)
            ) | (
                InstanceCommandTarget::Constructor(39),
                ActorExtension::Selector39(_)
            ) | (
                InstanceCommandTarget::Constructor(47),
                ActorExtension::Selector47(_)
            ) | (
                InstanceCommandTarget::Constructor(49),
                ActorExtension::Selector49(_)
            )
        ) && command.code.value <= 15;
        let derived_delegation = matches!(
            command.target,
            InstanceCommandTarget::DerivedActor
                | InstanceCommandTarget::Family428650
                | InstanceCommandTarget::Family4236d0
                | InstanceCommandTarget::Constructor(
                    6 | 7 | 9 | 10 | 11 | 12 | 21 | 33 | 34 | 35 | 36 | 60 | 77
                )
        ) && matches!(actor.extension, ActorExtension::Derived(_))
            && command.code.value <= 15;
        if !shared_dispatch
            && !selector88_delegation
            && !selector53_delegation
            && !selector29_delegation
            && !selector52_delegation
            && !selector59_delegation
            && !selector74_delegation
            && !selector84_delegation
            && !selector18_delegation
            && !selector69_delegation
            && !selector55_delegation
            && !selector17_delegation
            && !family40d800_delegation
            && !derived_delegation
        {
            return Err(ActorCommandError::WrongTarget);
        }
        let code = command.code.value;
        if !(0..=15).contains(&code) {
            return Ok(false);
        }
        // 00488AA0 returns before the operand read. Even missing/invalid
        // operand bytes are irrelevant to this no-player branch.
        if matches!(code, 13 | 14) && context.first_player_grants.is_none() {
            return Ok(true);
        }
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (0 | 1 | 3, NoPayloadRead) => {}
            (2, ModelSlot(model)) => {
                self.set_visual_model(object, model.raw.value, context.runtime)?
            }
            (4, ActorWords8e(value)) => self.actor_mut(object)?.words_8e_92 = value.value,
            (5, ActorWord8c(value)) => self.actor_mut(object)?.word_8c = value.value,
            (6, ActorSelectorB4(value)) => {
                self.actor_mut(object)?.word_b4 = map_actor_selector(value.value)
            }
            (7, ActorMaximumHealth(value)) => {
                self.actor_mut(object)?.maximum_health = Some(value.value)
            }
            (8, ActorCurrentHealth(value)) => self.actor_mut(object)?.health = value.value,
            (9, ActorWord7a(value)) => self.actor_mut(object)?.word_7a = value.value,
            (10, ActorShiftedByte7c(value)) => {
                self.actor_mut(object)?.byte_7c = (value.value >> 8) as u8
            }
            (11, ActorShiftedByte7d(value)) => {
                self.actor_mut(object)?.byte_7d = (value.value >> 8) as u8
            }
            (12, ActorStateC4(value)) => {
                self.actor_mut(object)?.word_c4 = value.value as u32;
                if value.value < 3 {
                    // Sticky 8000 is outside the low-nine-bit dirty mask.
                    self.owned_mut(object)?.state.base.flags |= 0x8000;
                }
            }
            (13, ActorDisableUnlessBit(value)) | (14, ActorDisableIfBit(value)) => {
                let index = match value {
                    ConditionalInstanceWord::Available(field) => field.value,
                    ConditionalInstanceWord::OutsideAllocation { allocation_offset } => {
                        return Err(ActorCommandError::MissingConditionalWord {
                            code,
                            allocation_offset: *allocation_offset,
                        });
                    }
                };
                let owned = grant_is_owned(context.first_player_grants.unwrap(), u32::from(index))?;
                if owned == (code == 14) {
                    self.dispose(object)?;
                }
            }
            (15, ActorWord76(value)) => self.actor_mut(object)?.word_76 = Some(value.value),
            _ => return Err(ActorCommandError::Payload(code)),
        }
        Ok(true)
    }

    /// 00479E10 for the exact shared vtable. Its +38 guard is 00401370
    /// (always true); derived guards remain separate. A rejection retains
    /// prior effects and the object: deletion belongs to enclosing completion
    /// owner 004602D0.
    pub fn run_actor_commands(
        &mut self,
        object: ObjectId,
        resource_argument: u32,
        suffix: Option<ObjectInstanceCommands<'_>>,
        context: ActorCommandContext<'_>,
    ) -> Result<Option<ObjectCommandRejection>, ObjectConstructionError> {
        self.actor(object)?;
        run_commands(
            self,
            context.runtime,
            object,
            CommandPolicy::SharedActor {
                first_player_grants: context.first_player_grants,
                allocation: None,
            },
            resource_argument,
            suffix,
        )
    }
}
