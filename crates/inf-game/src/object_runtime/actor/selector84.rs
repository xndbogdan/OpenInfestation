//! Selector 84, the Alien Brain's capture node: the shared 0047A7A0 with the
//! final table 00522488 (case 0x54, 0x140 bytes, through 0043C070, which
//! sets the link +138 to -1 and the capture clock +13C to 0). Its table is
//! the shared actor's 00527C00 but for +00 (0043C0A0 over the teardown
//! 0043C0C0), +04 (the commands 0043C2A0), +14 (the model registers
//! 0043C200), +1C (the finish 0043C110), +24 (the update 0043C0F0), +34 (the
//! after-load 0043C260) and +A4 (the contact 0043C150).
//!
//! The nodes count themselves by team in two process words: 00547620
//! holds the aliens' (team 0x12), 00547624 everyone else's. The finish adds
//! a node to its team's word, the teardown takes it away, and a capture
//! moves it. The brain (selector 85) wakes once the aliens hold none. A
//! node is captured by touching it with an actor whose +5C answers, which
//! among the classes the port builds is selector 12 alone (00404610,
//! itself): the node takes that actor's team, passes it to its linked
//! actor and starts its clock, which drives its three model registers.

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};

use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorExtension, ActorHandle,
};
use crate::object_runtime::{mark_flag_change, ObjectArena, ObjectError, ObjectId};

/// The aliens' team (`+B4`), whose nodes 00547620 counts.
pub const CAPTURE_ALIEN_TEAM: u32 = 0x12;
/// 0043C11F: the flag the finish sets on an alien node and a touch clears.
const ALIEN_NODE_FLAG: u32 = 0x1000_0000;

/// Case 0x54's fields after 0047A7A0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector84State {
    /// +138: command 16's one-based placement reference until the after-load
    /// resolves it to that placement's handle (0047F7B0), or the null handle;
    /// -1 when no command names one.
    pub link_138: u32,
    /// +13C, the capture clock: 0 until a capture starts it at 1; then each
    /// update adds `delta >> 4`.
    pub clock_13c: u32,
}

impl Selector84State {
    /// 0043C200, the model registers: 1 is half the clock, 2 the clock less
    /// 0x20000 and 3 the clock less 0x30000, each 0 before it starts and at
    /// most 0xFFFF; an idle node and other registers read 0.
    pub fn model_value(&self, register: u32) -> u16 {
        let clock = self.clock_13c as i32;
        if clock == 0 {
            return 0;
        }
        let value = match register {
            1 => (clock >> 1) as u32,
            2 if clock > 0x20000 => clock.wrapping_sub(0x20000) as u32,
            3 if clock > 0x30000 => clock.wrapping_sub(0x30000) as u32,
            _ => return 0,
        };
        if value < 0x10000 {
            value as u16
        } else {
            0xffff
        }
    }
}

/// 00547620 and 00547624: the capture nodes by team.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureNodeCounts {
    /// 00547620, the nodes on the aliens' team.
    pub aliens_547620: u32,
    /// 00547624, the nodes on any other team.
    pub others_547624: u32,
}

impl CaptureNodeCounts {
    fn word(&mut self, team: u32) -> &mut u32 {
        if team == CAPTURE_ALIEN_TEAM {
            &mut self.aliens_547620
        } else {
            &mut self.others_547624
        }
    }

    fn add(&mut self, team: u32) {
        let word = self.word(team);
        *word = word.wrapping_add(1);
    }

    fn take(&mut self, team: u32) {
        let word = self.word(team);
        *word = word.wrapping_sub(1);
    }
}

impl ObjectArena {
    /// Case 0x54: 0x140 bytes, the shared 0047A7A0, then 0043C070's link of
    /// -1 and idle clock.
    pub fn allocate_selector84(
        &mut self,
        request: super::ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension = ActorExtension::Selector84(Selector84State {
            link_138: u32::MAX,
            clock_13c: 0,
        });
        Ok(object)
    }

    pub fn selector84(&self, object: ObjectId) -> Result<&Selector84State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector84(state) => Ok(state),
            _ => Err(ObjectError::NotSelector84(object)),
        }
    }

    fn selector84_mut(&mut self, object: ObjectId) -> Result<&mut Selector84State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector84(state) => Ok(state),
            _ => Err(ObjectError::NotSelector84(object)),
        }
    }

    /// 00547620 and 00547624.
    pub fn capture_node_counts(&self) -> CaptureNodeCounts {
        self.capture_nodes
    }

    /// 0043C2A0, the node's +04: codes through 15 are the shared actor's
    /// (0047ADA0), 16 sets the link reference, and any other refuses unread.
    pub fn apply_selector84_command(
        &mut self,
        node: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(84) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector84(node)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(node, command, context);
        }
        match (code, &command.payload) {
            (16, InstanceCommandPayload::Selector84Link(value)) => {
                self.set_selector84_link(node, value.value)?;
                Ok(true)
            }
            (16, _) => Err(ActorCommandError::Payload(code)),
            _ => Ok(false),
        }
    }

    /// 0043C2A0's code 16: `+138` takes the operand's DWORD.
    pub(in crate::object_runtime) fn set_selector84_link(
        &mut self,
        object: ObjectId,
        reference: u32,
    ) -> Result<(), ObjectError> {
        self.selector84_mut(object)?.link_138 = reference;
        Ok(())
    }

    /// 0043C110 in place of the shared +1C: the node joins its team's count,
    /// and an alien node takes flag 0x10000000; then the shared 0047AAD0.
    pub fn finish_selector84_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        let team = self.actor(object)?.word_b4;
        self.capture_nodes.add(team);
        if team == CAPTURE_ALIEN_TEAM {
            let state = &mut self.owned_mut(object)?.state.base;
            let flags = state.flags | ALIEN_NODE_FLAG;
            mark_flag_change(state, flags);
        }
        self.finish_actor_initialization(object)
    }

    /// 0043C260, the after-load: `+138` becomes the handle of the actor at
    /// its placement reference (0047F7B0), and that actor joins the node's
    /// team (dirty); with none resolved, `+138` is the null handle.
    pub fn initialize_selector84_after_load(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ActorConstructionError> {
        let reference = self.selector84(object)?.link_138 as i32;
        let handle = self.actor_handle_by_placement(reference)?;
        self.selector84_mut(object)?.link_138 = handle.raw();
        match self.resolve_actor_handle(handle)? {
            Some(linked) => {
                let team = self.actor(object)?.word_b4;
                self.owned_mut(linked)?.state.base.dirty_word |= 2;
                self.actor_mut(linked)?.word_b4 = team;
            }
            None => {
                let null = self
                    .actor_handles()
                    .ok_or(ActorConstructionError::MissingRegistry)?
                    .null_handle();
                self.selector84_mut(object)?.link_138 = null.raw();
            }
        }
        Ok(())
    }

    /// 0043C0F0: a running clock gains `delta >> 4`.
    pub fn tick_selector84(&mut self, object: ObjectId, delta: u32) -> Result<(), ObjectError> {
        let state = self.selector84_mut(object)?;
        if state.clock_13c != 0 {
            state.clock_13c = state.clock_13c.wrapping_add(delta >> 4);
        }
        Ok(())
    }

    /// 0043C150, the contact: `capturer` is what the partner's +5C answers
    /// (selector 12 answers itself; every other class the port builds,
    /// nothing). An idle node moves from its team's count to the capturer's
    /// team, passes the team to its linked actor and starts its clock; any
    /// answer clears the alien flag.
    pub fn touch_selector84(
        &mut self,
        object: ObjectId,
        capturer: Option<ObjectId>,
    ) -> Result<(), ActorConstructionError> {
        let Some(capturer) = capturer else {
            return Ok(());
        };
        if self.selector84(object)?.clock_13c == 0 {
            let old = self.actor(object)?.word_b4;
            self.capture_nodes.take(old);
            let team = self.actor(capturer)?.word_b4;
            self.owned_mut(object)?.state.base.dirty_word |= 2;
            self.actor_mut(object)?.word_b4 = team;
            self.capture_nodes.add(team);
            let link = ActorHandle::from_raw(self.selector84(object)?.link_138);
            if let Some(linked) = self.resolve_actor_handle(link)? {
                self.owned_mut(linked)?.state.base.dirty_word |= 2;
                self.actor_mut(linked)?.word_b4 = team;
            }
            let state = self.selector84_mut(object)?;
            state.clock_13c = state.clock_13c.wrapping_add(1);
        }
        let state = &mut self.owned_mut(object)?.state.base;
        let flags = state.flags & !ALIEN_NODE_FLAG;
        mark_flag_change(state, flags);
        Ok(())
    }

    /// 0043C0C0, before the shared 0047AA60: the node leaves its team's
    /// count.
    pub(in crate::object_runtime) fn release_selector84(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        let team = self.actor(object)?.word_b4;
        self.capture_nodes.take(team);
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector84_tests.rs"]
mod tests;
