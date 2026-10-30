//! World owner's start position 0048DAF0, heading 0048DBC0 and selector-30 override writes.
//! H1 stores authored X/Z pairs; Y always comes from the current raw H12 cell,
//! including when the stored override carries a different Y value.

use thiserror::Error;

use crate::{
    terrain_runtime::{TerrainHeightScale, TerrainRuntime},
    word_resources::{resolve_h1_word, H1WordError},
    ResidentRuntime,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldStartState {
    /// Raw owner +A4. The position resource uses the signed, WORD-wrapped double.
    pub pair_index: u16,
    /// Owner +A6 nonzero with the three stored WORDs at +A8..+AC.
    pub override_position: Option<[u16; 3]>,
    /// World +1C4 (owner +14): this is a BYTE, independent of adjacent fields.
    pub vehicle_selection: u8,
    /// The start harness's position and heading for the first spawn
    /// ([`crate::harness_start`]); a port debug field with no native slot,
    /// which the load's clear of the override leaves alone.
    pub harness_spawn: Option<crate::harness_start::HarnessSpawn>,
    /// Port option, not retail: a start of the port's own (Pelagia's
    /// rework, behind its moved return gate). The load's clear of the
    /// override leaves it too, so a lost ACAM comes back there.
    pub port_spawn: Option<crate::harness_start::HarnessSpawn>,
}

pub struct WorldStartContext<'a> {
    pub state: &'a mut WorldStartState,
    pub runtime: &'a ResidentRuntime,
    pub terrain: &'a TerrainRuntime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum WorldStartError {
    #[error("world start pair {pair_index:#06x} selects negative H1 position slot {slot}")]
    NegativePositionSlot { pair_index: u16, slot: i16 },
    #[error("world start H1 slot {slot} is outside its {capacity}-slot table")]
    PositionSlotBoundary { slot: u16, capacity: usize },
    #[error("world start H1 slot {slot} is not resident; lazy loading is not implemented")]
    PositionNotResident { slot: u16 },
    #[error("world start H1 slot {slot} does not resolve to its registered word resource")]
    PositionRegistration { slot: u16 },
}

impl WorldStartContext<'_> {
    /// 0048DAF0. An override skips H1 entirely. Neither branch uses the
    /// interpolated height sampler 00450A30 or preserves the stored override Y.
    pub fn resolve_position(&self) -> Result<[u16; 3], WorldStartError> {
        let harness = self
            .state
            .harness_spawn
            .and_then(|spawn| spawn.position)
            .or(self.state.port_spawn.and_then(|spawn| spawn.position));
        let [x, z] = if let Some(position) = harness {
            position
        } else if let Some([x, _, z]) = self.state.override_position {
            [x, z]
        } else {
            let word = self.h1_word(self.state.pair_index.wrapping_shl(1))?;
            [word as u16, (word >> 16) as u16]
        };
        let shift = match self.terrain.sampling().height_scale {
            TerrainHeightScale::Units16 => 4,
            TerrainHeightScale::Units32 => 5,
        };
        let height = i32::from(self.terrain.cell_at([x, z]).height_i8()) << shift;
        Ok([x, (height + 60) as u16, z])
    }

    /// 0048DBC0: rotation [0, heading, 0] from the first WORD of H1 slot
    /// `(pair << 1) + 1`, converted as `(signed WORD << 16) / 360` with C
    /// truncation. The override position never affects the heading.
    pub fn resolve_heading(&self) -> Result<[u16; 3], WorldStartError> {
        let slot = self.state.pair_index.wrapping_shl(1).wrapping_add(1);
        let heading = self
            .state
            .harness_spawn
            .and_then(|spawn| spawn.heading)
            .or(self.state.port_spawn.and_then(|spawn| spawn.heading));
        let degrees = match heading {
            Some(degrees) => i32::from(degrees),
            None => i32::from(self.h1_word(slot)? as u16 as i16),
        };
        Ok([0, ((degrees << 16) / 360) as u16, 0])
    }

    /// One H1 entry DWORD by the loader's signed, WORD-wrapped slot.
    fn h1_word(&self, raw_slot: u16) -> Result<u32, WorldStartError> {
        let signed = raw_slot as i16;
        let slot = u16::try_from(signed).map_err(|_| WorldStartError::NegativePositionSlot {
            pair_index: self.state.pair_index,
            slot: signed,
        })?;
        resolve_h1_word(self.runtime, slot).map_err(|error| match error {
            H1WordError::CapacityBoundary { slot, capacity } => {
                WorldStartError::PositionSlotBoundary { slot, capacity }
            }
            H1WordError::NotResident { slot } => WorldStartError::PositionNotResident { slot },
            H1WordError::Registration { slot } => WorldStartError::PositionRegistration { slot },
        })
    }

    /// 0048C5AC..0048C613. The caller supplies the reached optional BYTE
    /// store separately from the unconditional six-byte override publication.
    pub fn publish_override(&mut self, position: [u16; 3], vehicle: Option<u8>) {
        if let Some(vehicle) = vehicle {
            self.state.vehicle_selection = vehicle;
        }
        self.state.override_position = Some(position);
    }
}

#[cfg(test)]
#[path = "world_start_tests.rs"]
mod tests;
