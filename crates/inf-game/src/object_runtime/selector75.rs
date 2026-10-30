//! Selector 75, the furniture marker: 004352F0 over the base object
//! 00479C60 with the final table 00522008, which differs from the base's
//! 00527B24 only in +00 (00475400, the same 00479D00 teardown), the finish
//! +1C (00435310) and the world cleanup +30 (00435360). The constructor
//! sets the kind +40 to 0x0E, the fence segments', and the index +44 to
//! 0xFF.
//!
//! It keeps a destroyed fence down when its world loads again. Its finish
//! takes the world's next placement serial (0048EB90) for +44 and, when the
//! save has that completion bit (0048EA80), removes the chain of furniture
//! of its kind at its position (00437110,
//! [`remove_marked_furniture`](crate::world_furniture_effect::remove_marked_furniture)).
//! Leaving the world, its cleanup runs the same removal; when nothing of the
//! kind is left near it, it marks the bit (0048EAD0, forced past the
//! cleanup's inhibit).

use thiserror::Error;

use super::world_load::world_completion_bit;
use super::{ObjectArena, ObjectClass, ObjectError, ObjectId};
use crate::world_furniture_effect::{
    FurnitureChainResources, FurnitureMarkerError, MarkerFurniture,
};
use crate::world_mission::{PlacementDeathRequest, WorldMission, WorldMissionError};

/// 004352F8: the fence segments' kind.
const FENCE_KIND: u32 = 0x0e;

/// 004352F0's fields over the base object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector75State {
    /// DWORD +40, the furniture kind the marker removes (its low byte).
    pub kind_40: u32,
    /// BYTE +44, the world serial the finish takes; 0xFF before it.
    pub index_44: u8,
    /// The finish found its completion bit set. Its removal (00437110)
    /// writes the world's terrain, which the loader lends right after the
    /// marker's construction, before the next placement.
    pub chain_due: bool,
}

impl Selector75State {
    pub(super) fn new() -> Self {
        Self {
            kind_40: FENCE_KIND,
            index_44: 0xff,
            chain_due: false,
        }
    }
}

/// What the marker's cleanup (00435360) did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector75Cleanup {
    /// The removal took this many pieces: the furniture stood.
    Stood(u32),
    /// Nothing of its kind was left, and 0048EAD0 ran on +44 (which leaves
    /// the null index -1 and indices past the four words unmarked).
    Marked,
}

#[derive(Debug, Error)]
pub enum Selector75Error {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("furniture marker 00437110: {0}")]
    Removal(#[from] FurnitureMarkerError<String>),
    #[error(transparent)]
    Mission(#[from] WorldMissionError),
}

impl ObjectArena {
    pub fn selector75(&self, object: ObjectId) -> Result<&Selector75State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector75(state) => Ok(state),
            _ => Err(ObjectError::NotSelector75(object)),
        }
    }

    fn selector75_mut(&mut self, object: ObjectId) -> Result<&mut Selector75State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector75(state) => Ok(state),
            _ => Err(ObjectError::NotSelector75(object)),
        }
    }

    /// 00435310, the marker's +1C. A placement word of -1 disposes it
    /// (00479FA0). Otherwise +44 takes the low byte of the world serial
    /// (0048EB90 post-increments it, as each loaded actor's 0047F6C0 does),
    /// and a set completion bit at that signed index (0048EA80 over the
    /// loader's words) leaves the chain due.
    pub(super) fn finish_selector75(&mut self, marker: ObjectId) -> Result<(), ObjectError> {
        self.selector75(marker)?;
        if self.object(marker)?.base.placement_word == u16::MAX {
            return self.dispose(marker);
        }
        let completion = self
            .world_load
            .world_completion
            .ok_or(ObjectError::Selector75CompletionUnknown(marker))?;
        let index = self.world_load.actor_serial_3d0 as u8;
        self.world_load.actor_serial_3d0 = self.world_load.actor_serial_3d0.wrapping_add(1);
        let state = self.selector75_mut(marker)?;
        state.index_44 = index;
        state.chain_due = world_completion_bit(&completion, index as i8);
        Ok(())
    }

    /// The finish's due removal, for the loader: the marker's X/Z and kind,
    /// once.
    pub fn take_selector75_chain(
        &mut self,
        marker: ObjectId,
    ) -> Result<Option<([u16; 2], u8)>, ObjectError> {
        let state = *self.selector75(marker)?;
        if !state.chain_due {
            return Ok(None);
        }
        self.selector75_mut(marker)?.chain_due = false;
        let [x, _, z] = self
            .object(marker)?
            .base
            .position_words
            .ok_or(ObjectError::Selector75Position(marker))?;
        Ok(Some(([x, z], state.kind_40 as u8)))
    }

    /// 00435360, the marker's +30: the removal (00437110) at its position;
    /// none taken marks +44, sign-extended, in the mission with force 1
    /// (0048EAD0), which the cleanup's inhibit W+08 cannot stop.
    pub fn cleanup_selector75<R: FurnitureChainResources<Error = String>>(
        &mut self,
        marker: ObjectId,
        furniture: &mut MarkerFurniture<'_, R>,
        mission: &mut WorldMission,
    ) -> Result<Selector75Cleanup, Selector75Error> {
        let state = *self.selector75(marker)?;
        let [x, _, z] = self
            .object(marker)?
            .base
            .position_words
            .ok_or(ObjectError::Selector75Position(marker))?;
        let removed = furniture.remove([x, z], state.kind_40 as u8)?;
        if removed != 0 {
            return Ok(Selector75Cleanup::Stood(removed));
        }
        mission.record_placement_death(PlacementDeathRequest {
            index: i32::from(state.index_44 as i8),
            inhibit_byte_08: mission.inhibit_byte_08,
            force: 1,
        })?;
        Ok(Selector75Cleanup::Marked)
    }
}

#[cfg(test)]
#[path = "selector75_tests.rs"]
mod tests;
