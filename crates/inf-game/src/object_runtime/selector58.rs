//! Selector 58, 004753E0 over the 0047A490 rotating visual (final 00527838):
//! a pad the player can stop on, which saves the respawn point.
//! Construction runs +08 = 0047A610, then payload flags through 00479DB0.
//! Its +14 = 004755B0 returns the +6E timer for selector 1.
//!
//! Its +24, 00475420, first runs the chime timer +6E: while set it rises by
//! a quarter of the delta, and once it would reach FFFFh three one-shots of
//! sound 2 play at the pad (pitches 8000h, C000h, 10000h) and it clears.
//! Then, with a first player (00488AA0) whose vehicle (0046B030) is at
//! lifecycle 3 or more, standing on the pad (from 0 to 100h above it and
//! within 128 units across) saves the player's spawn (0046AF80) once per
//! visit (+6C) and chimes sound 2 at pitches 5555h, 8000h and 18000h,
//! starting +6E. Leaving the pad clears +6C; without a player it clears too.

use thiserror::Error;

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, PlayerId, RotatingVisual};

/// `00475459` and `0047553B`: the pad's one-shot.
pub const PAD_SOUND: u32 = 2;
/// `00475527`: the visit chime's pitches.
const VISIT_PITCHES: [u32; 3] = [0x5555, 0x8000, 0x18000];
/// `00475456`: the timer chime's pitches.
const TIMER_PITCHES: [u32; 3] = [0x8000, 0xc000, 0x10000];

/// The services the pad's update reaches.
pub trait Selector58Host {
    type Error;
    /// `00487EC0(2, &pad+32, 10000h, pitch, 0)` and its `004923C0`.
    fn chime(&mut self, position: [u16; 3], pitch: u32) -> Result<(), Self::Error>;
    /// `0046AF80` on the first player.
    fn save_spawn(&mut self, arena: &mut ObjectArena, player: PlayerId) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector58Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("pad {0:?} has no position for 00475420")]
    PadPosition(ObjectId),
    #[error("pad host: {0}")]
    Host(E),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector58State {
    pub rotating: RotatingVisual,
    /// Set while the player stands on the pad; cleared when it leaves.
    pub byte_6c: u8,
    /// Animation timer: 1 when triggered, then rising by delta/4 until it
    /// would reach 0xFFFF.
    pub word_6e: u16,
}

impl Selector58State {
    pub(super) fn new() -> Self {
        Self {
            rotating: RotatingVisual::new(),
            byte_6c: 0,
            word_6e: 0,
        }
    }

    /// 004755B0: +6E for selector 1, otherwise zero.
    pub(super) fn model_value(&self, selector: u32) -> u16 {
        if selector == 1 {
            self.word_6e
        } else {
            0
        }
    }
}

impl ObjectArena {
    pub fn selector58(&self, object: ObjectId) -> Result<&Selector58State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector58(state) => Ok(state),
            _ => Err(ObjectError::NotSelector58(object)),
        }
    }

    fn selector58_mut(&mut self, object: ObjectId) -> Result<&mut Selector58State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector58(state) => Ok(state),
            _ => Err(ObjectError::NotSelector58(object)),
        }
    }

    /// `00475420(delta)`, the pad's +24.
    pub fn tick_selector58<H: Selector58Host>(
        &mut self,
        pad: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector58Error<H::Error>> {
        let player = self.first_player();
        let position = self
            .object(pad)?
            .base
            .position_words
            .ok_or(Selector58Error::PadPosition(pad))?;
        let timer = self.selector58(pad)?.word_6e;
        if timer != 0 {
            let next = u32::from(timer) + (delta >> 2);
            if (next as i32) < 0xffff {
                self.selector58_mut(pad)?.word_6e = next as u16;
            } else {
                for pitch in TIMER_PITCHES {
                    host.chime(position, pitch).map_err(Selector58Error::Host)?;
                }
                self.selector58_mut(pad)?.word_6e = 0;
            }
        }
        let Some(player) = player else {
            self.selector58_mut(pad)?.byte_6c = 0;
            return Ok(());
        };
        let Some(vehicle) = self.first_player_primary_actor()? else {
            return Ok(());
        };
        if (self.actor(vehicle)?.word_c4 as i32) < 3 {
            return Ok(());
        }
        let at = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let rise = i32::from(at[1] as i16) - i32::from(position[1] as i16);
        if !(0..=0x100).contains(&rise) {
            return Ok(());
        }
        let dx = i32::from(at[0].wrapping_sub(position[0]) as i16);
        let dz = i32::from(at[2].wrapping_sub(position[2]) as i16);
        if (dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32) >= 0x4000 {
            self.selector58_mut(pad)?.byte_6c = 0;
            return Ok(());
        }
        if self.selector58(pad)?.byte_6c != 0 {
            return Ok(());
        }
        self.selector58_mut(pad)?.byte_6c = 1;
        host.save_spawn(self, player)
            .map_err(Selector58Error::Host)?;
        for pitch in VISIT_PITCHES {
            host.chime(position, pitch).map_err(Selector58Error::Host)?;
        }
        self.selector58_mut(pad)?.word_6e = 1;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector58_tests.rs"]
mod tests;
