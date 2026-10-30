//! Selector 39, 0043B6D0 over 0040D800 over the shared actor (final
//! 00522398): a destructible course target. Construction, commands
//! 0043B7D0, postinitialization 0040E000, the 0043B720 after-load that links
//! it to other actors, and its 0043BDF0 messages are owned. Its damage,
//! death, update and fuse are in [`course`].

use std::sync::Arc;

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};

use super::{
    family40d800::{Actor40d800State, FamilyLinkChange},
    selector88::{
        InstanceCommandAllocation, InstanceCommandAllocationError, RetainedInstanceCommand,
    },
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension, FactoryError, FactoryStaffHost,
};
use crate::object_runtime::{
    world_load::world_completion_bit, GrantOwner, LiveGrantOperation, LiveGrantRequest,
    ObjectArena, ObjectError, ObjectId, WorldLoadError,
};

mod course;
pub use course::{
    Selector39Countdown, Selector39DeathHost, Selector39Error, Selector39Host,
    Selector39MissionWrite, Selector39UpdateHost, FUSE_SOUND,
};

/// 0043B6D0's writes after 0040D800.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector39State {
    pub intermediate: Actor40d800State,
    /// Code 21's retained operand: a count and a relocated array of
    /// one-based placement references, rewritten to handles at after-load.
    pub retained_158: Option<RetainedInstanceCommand>,
    pub word_15c: u32,
    /// Seconds for the destruction countdown; zero takes 0040DB40 instead.
    pub byte_160: u8,
    pub byte_161: u8,
    /// A completed target with this flag sets session byte G+1C3.
    pub byte_162: u8,
    pub byte_163: u8,
    /// Messages 0xB/0xC clear or set it.
    pub byte_164: u8,
    /// Disables the player-proximity effects of the update.
    pub byte_165: u8,
    /// Messages 0x14/0x15 add or subtract their argument.
    pub byte_166: u8,
}

impl Selector39State {
    fn new() -> Self {
        Self {
            intermediate: Actor40d800State::new(),
            retained_158: None,
            word_15c: 0,
            byte_160: 0,
            byte_161: 0,
            byte_162: 0,
            byte_163: 0,
            byte_164: 0,
            byte_165: 0,
            byte_166: 0,
        }
    }
}

/// One 0047DD40 delivery from the after-load, recorded for the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector39LinkDelivery {
    pub target: ObjectId,
    pub code: i32,
    pub change: FamilyLinkChange,
}

/// What 0043B720 reached for one target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selector39AfterLoad {
    pub deliveries: Vec<Selector39LinkDelivery>,
    /// Native writes session byte G+1C3 = 1 here.
    pub sets_session_1c3: bool,
    pub disposed: bool,
}

impl ObjectArena {
    pub fn allocate_selector39(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector39(Box::new(Selector39State::new()));
        Ok(object)
    }

    pub fn selector39(&self, object: ObjectId) -> Result<&Selector39State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector39(state) => Ok(state),
            _ => Err(ObjectError::NotSelector39(object)),
        }
    }

    pub(in crate::object_runtime) fn selector39_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector39State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector39(state) => Ok(state),
            _ => Err(ObjectError::NotSelector39(object)),
        }
    }

    /// `00468160`'s target arm against a course target, whose `+D0` is
    /// `0043BEA0`: the family's staff take (`0040DAB0`), and a take that
    /// fills a target that had room grants its nonzero `+7A` to the first
    /// player (`00488AA0`'s `+9C`, `004750C0`). The Defence Satellite's
    /// hangar grants "Defence satellite captured" this way. A refusal sets
    /// the scientist's `+22E` bit 0.
    pub fn scientist_reaches_course_target<H: FactoryStaffHost>(
        &mut self,
        scientist: ObjectId,
        target: ObjectId,
        host: &mut H,
    ) -> Result<bool, FactoryError<H::Error>> {
        self.selector39(target)?;
        // 0043BEB8: SETG over +CC(0), the free staff as a signed count.
        let had_room = (self.free_staff(target, None)? as i32) > 0;
        if !self.take_scientist_staff(target, scientist, host)? {
            self.scientist_refused(scientist)?;
            return Ok(false);
        }
        let grant = self.actor(target)?.word_7a;
        if had_room && self.free_staff(target, None)? == 0 && grant != 0 {
            let player = self
                .first_player()
                .ok_or(FactoryError::CourseTargetFirstPlayer)?;
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw {
                        id: u32::from(grant),
                    },
                },
                &host.grants(),
            )?;
        }
        Ok(true)
    }

    /// 0043B7D0: shared commands through 15, 0040D9C0 below 21, then the
    /// class stores. It has no case 26 and returns zero for it.
    pub fn apply_selector39_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
        allocation: Option<&Arc<InstanceCommandAllocation>>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(39) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector39(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let state = self.selector39_mut(object)?;
        if let Some(result) = state.intermediate.apply_command(code, &command.payload) {
            let dirty_0200 = result.map_err(ActorCommandError::Payload)?;
            if dirty_0200 {
                self.owned_mut(object)?.state.base.dirty_word |= 0x0200;
            }
            return Ok(true);
        }
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (21, RetainedPayloadPointer { allocation_offset }) => {
                let allocation = allocation.ok_or(InstanceCommandAllocationError::MissingOwner)?;
                if allocation_offset.checked_sub(4) != Some(command.command_offset)
                    || *allocation_offset as usize
                        > allocation.main_payload().end - allocation.main_payload().start
                {
                    return Err(InstanceCommandAllocationError::RetainedOffset {
                        allocation_offset: *allocation_offset,
                    }
                    .into());
                }
                state.retained_158 = Some(RetainedInstanceCommand::new(
                    Arc::clone(allocation),
                    *allocation_offset,
                ));
            }
            (22, Selector39Dword15c(value)) => state.word_15c = value.value,
            (23, Selector39Flag(value)) => state.byte_162 = u8::from(value.value != 0),
            (24, Selector39Byte160(value)) => state.byte_160 = value.value,
            (25, Selector39Flag(value)) => state.byte_163 = u8::from(value.value != 0),
            (27, Selector39Flag(value)) => state.byte_161 = u8::from(value.value != 0),
            (28, Selector39Flag(value)) => state.byte_165 = u8::from(value.value != 0),
            (21..=25 | 27 | 28, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 0043B720, selector 39's +34. It tests the placement's completion
    /// (0048EA80 over signed +78) and clears +138. Each retained reference is
    /// resolved through 0047F7B0, stored back as a handle, and sent message 7
    /// through 0047DD40 (no range); a completed target also sends message 8.
    /// The count and array pointer are reread per row. A completed target
    /// then sets G+1C3 when +162 is set and disposes itself. The receiver's
    /// own 0047D670 runs inside its +98, so `apply` takes each delivery's
    /// state change before the next message.
    pub(crate) fn initialize_selector39_after_load(
        &mut self,
        object: ObjectId,
        world_completion: &[u32; 4],
        apply: &mut dyn FnMut(
            &mut ObjectArena,
            &Selector39LinkDelivery,
        ) -> Result<(), WorldLoadError>,
    ) -> Result<Selector39AfterLoad, WorldLoadError> {
        let index = self.actor(object)?.byte_78 as i8;
        let completed = world_completion_bit(world_completion, index);
        let state = self.selector39_mut(object)?;
        state.intermediate.byte_138 = 0;
        let retained = state.retained_158.clone();
        let mut reached = Selector39AfterLoad::default();
        if let Some(retained) = retained {
            let owner = retained.allocation();
            let image = owner.runtime_allocation();
            let base = retained.allocation_offset;
            let mut row = 0_u32;
            while (row as i32) < image.dword(base)? as i32 {
                let field = base.checked_add(4).ok_or(WorldLoadError::OffsetOverflow)?;
                let at = super::selector88::selector88_row(owner, field, row)?;
                let reference = image.dword(at)? as i32;
                let handle = self.actor_handle_by_placement(reference)?;
                image.write_dword(at, handle.raw())?;
                for code in [7, 8].into_iter().take(if completed { 2 } else { 1 }) {
                    if let Some(delivery) = self.deliver_link_message(handle, code)? {
                        apply(self, &delivery)?;
                        reached.deliveries.push(delivery);
                    }
                }
                row += 1;
            }
        }
        if completed {
            reached.sets_session_1c3 = self.selector39(object)?.byte_162 != 0;
            self.dispose(object)?;
            reached.disposed = true;
        }
        Ok(reached)
    }

    /// 0047DD40 with zero range from an after-load: resolve the handle
    /// (0047AD90), then the target's +98. Selectors 2 and 66 stay explicit
    /// boundaries: selector 2's 00420BE0 looks up the player first.
    fn deliver_link_message(
        &mut self,
        handle: crate::object_runtime::ActorHandle,
        code: i32,
    ) -> Result<Option<Selector39LinkDelivery>, WorldLoadError> {
        let Some(target) = self.resolve_actor_handle(handle)? else {
            return Ok(None);
        };
        use super::DerivedActorClass;
        let change = match &self.actor(target)?.extension {
            ActorExtension::Selector15(_) => self.selector15_message(target, code)?,
            ActorExtension::Selector30(_) => self.selector30_message(target, code)?,
            ActorExtension::Selector37(_) => self.selector37_message(target, code)?,
            // 0047FE10 passes links 7 and 8 to the family's 0040DE70.
            ActorExtension::Selector47(_) => self.family40d800_message(target, code)?,
            ActorExtension::Selector39(_) => self.selector39_message(target, code)?,
            // 004546F0: the mining rig's links.
            ActorExtension::Selector52(_) => {
                self.selector52_message(target, code)?;
                FamilyLinkChange::None
            }
            // 0042D6B0: the zone's links.
            ActorExtension::Selector74(_) => {
                self.selector74_message(target, code)?;
                FamilyLinkChange::None
            }
            // 004787E0: the Force Field's links.
            ActorExtension::Selector18(_) => {
                self.selector18_message(target, code)?;
                FamilyLinkChange::None
            }
            // 004864B0 retires the Powerball on 5 alone.
            ActorExtension::Selector57 if code == super::POWERBALL_RETIRE_MESSAGE => {
                return Err(ObjectError::LinkMessageBoundary(target).into())
            }
            ActorExtension::Selector57 => FamilyLinkChange::None,
            // 00454160 retires the rock on 5 alone.
            ActorExtension::Selector44(_) if code == super::ROCK_RETIRE_MESSAGE => {
                return Err(ObjectError::LinkMessageBoundary(target).into())
            }
            ActorExtension::Selector44(_) => FamilyLinkChange::None,
            // 00527908 keeps the family's own +98, 0040DE70.
            ActorExtension::Selector49(_) => self.family40d800_message(target, code)?,
            ActorExtension::Selector69(_) => match self.selector69_message(target, code)? {
                super::Selector69Message::Done => FamilyLinkChange::None,
                super::Selector69Message::Release { grant } => FamilyLinkChange::Release { grant },
            },
            // 004045F0, the shared and selector-16/59/73/84/88/90/93 +98,
            // returns zero; the spawner's 00454AB0 acts on 0xF only.
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector93(_) => FamilyLinkChange::None,
            ActorExtension::Derived(derived) => match &derived.class {
                DerivedActorClass::Selector11(_) => self.selector11_link_message(target, code)?,
                DerivedActorClass::Selector60(_) => self.selector60_link_message(target, code)?,
                // 004270B0 (central derived, the family/probe/4236 bases and
                // selectors 10/34/35/36/77) and selector 12's 00468BE0 act
                // only on codes 0xD and 0x16.
                DerivedActorClass::Base
                | DerivedActorClass::Selector77(_)
                | DerivedActorClass::Family428650(_) => FamilyLinkChange::None,
                DerivedActorClass::Selector2(_) => {
                    return Err(ObjectError::LinkMessageBoundary(target).into())
                }
            },
            ActorExtension::Selector66(_) | ActorExtension::BehaviorResource(_) => {
                return Err(ObjectError::LinkMessageBoundary(target).into())
            }
        };
        Ok(Some(Selector39LinkDelivery {
            target,
            code,
            change,
        }))
    }

    /// 0043BDF0, selector 39's +98, for the link codes. Messages 0xB/0xC
    /// set +164 and, with +160 set, retire the actor through 0047CE90;
    /// 0x14/0x15 adjust +166 by the argument. Those need the world's
    /// services and stay boundaries here; every other code is 0040DE70.
    pub(super) fn selector39_message(
        &mut self,
        object: ObjectId,
        code: i32,
    ) -> Result<FamilyLinkChange, ObjectError> {
        if matches!(code, 0xb | 0xc | 0x14 | 0x15) {
            return Err(ObjectError::LinkMessageBoundary(object));
        }
        self.family40d800_message(object, code)
    }
}

#[cfg(test)]
#[path = "selector39_tests.rs"]
mod tests;
