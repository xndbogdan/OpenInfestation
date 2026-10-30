//! Selector 45, an arena's spawn marker: `0045BF00` over the rotating visual
//! `0047A490` (final `0052616C`). The constructor clears the offsets, makes
//! the team 2 and links the marker into the spawn list (`0054E2F8`, its node
//! at `+6C`); the destructor `0045BF80` unlinks it. Its payload (`0045BFC0`,
//! after `+08`'s `0047A610`) takes the flags, the team (`+4`: 0 is 1, else
//! one more) and two offsets turned by its basis (`0047A6C0`), and its
//! commands (`0045C010`) set the same. Its `+24` and `+3C` are `00405140` and
//! its draw the visual's `0047AFC0`. The respawn's network search
//! (`0046A890`) picks among them.

use inf_formats::Selector45ConstructorPayload;

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, RotatingVisual};
use inf_formats::InstanceCommandPayload;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector45State {
    pub rotating: RotatingVisual,
    /// +78..+7D: the spawn's offset from the marker, in the world's frame.
    pub offset_78: [u16; 3],
    /// +7E..+83: a second offset, which the search does not read.
    pub offset_7e: [u16; 3],
    /// +84: the team whose players spawn here (1 is anyone's).
    pub team_84: u32,
}

impl Selector45State {
    pub(super) fn new() -> Self {
        Self {
            rotating: RotatingVisual::new(),
            offset_78: [0; 3],
            offset_7e: [0; 3],
            team_84: 2,
        }
    }
}

/// `0045BFC0`/`0045C010`'s team: a zero DWORD is team 1.
fn team(value: u32) -> u32 {
    if value == 0 {
        1
    } else {
        value.wrapping_add(1)
    }
}

impl ObjectArena {
    pub fn selector45(&self, object: ObjectId) -> Result<&Selector45State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector45(state) => Ok(state),
            _ => Err(ObjectError::NotSelector45(object)),
        }
    }

    fn selector45_mut(&mut self, object: ObjectId) -> Result<&mut Selector45State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector45(state) => Ok(state),
            _ => Err(ObjectError::NotSelector45(object)),
        }
    }

    /// The spawn list `0054E2F8`, in link order. Its static initializer
    /// `0045B470` sets the list up empty, as each world's starts here.
    pub fn spawn_markers(&self) -> &[ObjectId] {
        &self.spawn_markers
    }

    /// `00492500(1, id, ...)` from world code: the game protocol sends it on
    /// the next network tick, ahead of that tick's own.
    pub fn queue_network_send(&mut self, id: u32, send: crate::network::protocol::GameSend) {
        self.network_sends
            .push((crate::network::next_send_order(), id, send));
    }

    pub fn take_network_sends(&mut self) -> Vec<(u32, crate::network::protocol::GameSend)> {
        crate::network::merge_sends(self.take_ordered_network_sends(), Vec::new())
    }

    pub fn take_ordered_network_sends(&mut self) -> Vec<crate::network::OrderedSend> {
        std::mem::take(&mut self.network_sends)
    }

    /// The game protocol's state, lent while the world updates.
    pub fn lend_network_game(&mut self, hold: Box<crate::network::notices::NetworkGameHold>) {
        self.network_game = Some(hold);
    }

    pub fn take_network_game(&mut self) -> Option<Box<crate::network::notices::NetworkGameHold>> {
        self.network_game.take()
    }

    pub fn network_game_mut(&mut self) -> Option<&mut crate::network::notices::NetworkGameHold> {
        self.network_game.as_deref_mut()
    }

    /// `0045BFC0` after the flags (`00479D70`): the team and the two offsets
    /// turned by the basis `0047A610` just set.
    pub(super) fn initialize_selector45_payload(
        &mut self,
        object: ObjectId,
        payload: &Selector45ConstructorPayload,
    ) -> Result<(), ObjectError> {
        let state = self.selector45_mut(object)?;
        state.team_84 = team(payload.team.value as u32);
        let basis = state.rotating.basis;
        state.offset_78 = basis.transform_words(payload.offsets[0].value);
        state.offset_7e = basis.transform_words(payload.offsets[1].value);
        Ok(())
    }

    /// `0045C010`'s codes 3 to 5: the team, or an offset as it stands.
    pub(super) fn apply_selector45_command(
        &mut self,
        object: ObjectId,
        payload: &InstanceCommandPayload,
    ) -> Result<bool, ObjectError> {
        let state = self.selector45_mut(object)?;
        match payload {
            InstanceCommandPayload::Selector45Team(value) => state.team_84 = team(value.value),
            InstanceCommandPayload::Selector45Offset { code: 4, words } => {
                state.offset_78 = words.value
            }
            InstanceCommandPayload::Selector45Offset { code: 5, words } => {
                state.offset_7e = words.value
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "selector45_tests.rs"]
mod tests;
