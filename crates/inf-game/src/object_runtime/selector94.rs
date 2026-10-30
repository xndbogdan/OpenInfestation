//! Selector 94, Capture the Banner's missile launcher: constructor 004458E0
//! over the base object 00479C60, final table 00524E20, case 0x5D. The table
//! is the base object's but for its commands +04 (00445900: 2 keeps the
//! operand's address at +40, its zones), its update +24 (00445930), its draw
//! +2C (00409990, which only parks it: it is never seen) and its remote +3C
//! (00405140).
//!
//! Every 1.5 seconds (+44 counts down) it fires a missile at the first
//! player's vehicle, unless the last of its zones that holds the vehicle is
//! of another kind than 1: selector 25 (H13 0x6E, 00458A50), 1500 past the
//! vehicle along the launcher's bearing on it and 500 above, homing on it.

use inf_formats::native_basis::{normalize_words, NativeLengthPrecision};
use thiserror::Error;

use super::{
    actor::native_trunc_low32, ActorHandle, ObjectArena, ObjectClass, ObjectError, ObjectId,
    RetainedInstanceCommand,
};

/// `004459D5`: the time between missiles.
const PERIOD: i32 = 1_500_000;
/// `00524E64`: how far past the vehicle a missile starts.
const LEAD: f64 = 1500.0;
/// `00445A37`: how far above it.
const HEIGHT: u16 = 500;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Selector94State {
    /// +40, command 2's operand: a zone count and the zones' (relocated)
    /// address, each zone six WORDs: its kind, X, a WORD the update skips, Z,
    /// and its half width and depth.
    pub zones_40: Option<RetainedInstanceCommand>,
    /// +44, the time to the next missile (signed).
    pub countdown_44: i32,
}

/// What the launcher reaches beyond the arena.
pub trait Selector94Host {
    type Error: std::fmt::Display;
    /// `00458A50`: a missile at `position` homing on `target`.
    fn fire(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        target: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector94Error<E: std::fmt::Display> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("launcher {object:?}'s zones read past their allocation at {offset:#x}")]
    Zones { object: ObjectId, offset: u32 },
    #[error("launcher {object:?}'s zone pointer {field:#x} has no relocation, where retail reads an address")]
    Relocation { object: ObjectId, field: u32 },
    #[error("launcher {0:?} reads the vehicle's or its own unwritten position")]
    Position(ObjectId),
    #[error("launcher {object:?}'s missile (00458A50): {error}")]
    Host { object: ObjectId, error: E },
}

impl ObjectArena {
    pub fn selector94(&self, object: ObjectId) -> Result<&Selector94State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector94(state) => Ok(state),
            _ => Err(ObjectError::NotSelector94(object)),
        }
    }

    fn selector94_mut(&mut self, object: ObjectId) -> Result<&mut Selector94State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector94(state) => Ok(state),
            _ => Err(ObjectError::NotSelector94(object)),
        }
    }

    /// `00445900`'s code 2.
    pub(super) fn set_selector94_zones(
        &mut self,
        object: ObjectId,
        zones: RetainedInstanceCommand,
    ) -> Result<(), ObjectError> {
        self.selector94_mut(object)?.zones_40 = Some(zones);
        Ok(())
    }

    /// The zones' rectangles, `[kind, x, _, z, half width, half depth]`.
    fn selector94_zones<E: std::fmt::Display>(
        &self,
        object: ObjectId,
        zones: &RetainedInstanceCommand,
    ) -> Result<Vec<[i16; 6]>, Selector94Error<E>> {
        let allocation = zones.allocation();
        let memory = allocation.runtime_allocation();
        let dword = |offset: u32| {
            memory
                .dword(offset)
                .map_err(|_| Selector94Error::Zones { object, offset })
        };
        let count = dword(zones.allocation_offset)? as i32;
        let field = zones.allocation_offset.wrapping_add(4);
        if !allocation
            .relocations()
            .iter()
            .any(|entry| entry.field_offset == field)
        {
            return Err(Selector94Error::Relocation { object, field });
        }
        let start = dword(field)?;
        (0..count.max(0) as u32)
            .map(|index| {
                let at = start.wrapping_add(12 * index);
                let words: Result<Vec<i16>, _> = (0..6)
                    .map(|word| {
                        memory
                            .word(at + 2 * word)
                            .map(|value| value as i16)
                            .map_err(|_| Selector94Error::Zones { object, offset: at })
                    })
                    .collect();
                words.map(|words| std::array::from_fn(|i| words[i]))
            })
            .collect()
    }

    /// `00445930`: the countdown runs (signed); once spent, with zones and a
    /// first player's vehicle, the last zone holding the vehicle (by signed
    /// WORD distances under its half sizes) must be of kind 1, or none hold
    /// it. Then the launcher fires and waits 1.5 s again.
    pub fn tick_selector94<H: Selector94Host>(
        &mut self,
        object: ObjectId,
        delta: u32,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), Selector94Error<H::Error>> {
        let delta = delta as i32;
        let state = self.selector94_mut(object)?;
        if state.countdown_44 > delta {
            state.countdown_44 -= delta;
            return Ok(());
        }
        let Some(zones) = state.zones_40.clone() else {
            return Ok(());
        };
        let Some(vehicle) = self.first_player_primary_actor()? else {
            return Ok(());
        };
        let at = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(Selector94Error::Position(object))?;
        let mut fires = true;
        for [kind, x, _, z, half_x, half_z] in self.selector94_zones(object, &zones)? {
            let dx = i32::from((at[0] as i16).wrapping_sub(x)).abs();
            let dz = i32::from((at[2] as i16).wrapping_sub(z)).abs();
            if dx < i32::from(half_x) && dz < i32::from(half_z) {
                fires = kind == 1;
            }
        }
        if !fires {
            return Ok(());
        }
        self.selector94_mut(object)?.countdown_44 = PERIOD;
        let own = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector94Error::Position(object))?;
        // 004459DC..00445A58: 00417DA0 of the bearing in the plane, times
        // 1500 and truncated, beyond the vehicle; 500 above it.
        let bearing = normalize_words(
            [
                at[0].wrapping_sub(own[0]) as i16,
                0,
                at[2].wrapping_sub(own[2]) as i16,
            ],
            NativeLengthPrecision::Retained,
        );
        let lead = |axis: f32| native_trunc_low32(f64::from(axis) * LEAD) as u16;
        let position = [
            at[0].wrapping_add(lead(bearing[0])),
            at[1].wrapping_add(HEIGHT),
            at[2].wrapping_add(lead(bearing[2])),
        ];
        let target = self.actor(vehicle)?.handle;
        host.fire(self, position, target, rng)
            .map_err(|error| Selector94Error::Host { object, error })
    }
}

#[cfg(test)]
#[path = "selector94_tests.rs"]
mod tests;
