//! Selector 48: constructor 0043C7F0 over the visual base 0047A030, final
//! vtable 005225D0, an altitude ceiling. It differs from the visual base
//! 00527B68 only in its destructor, commands +04 (0043C840) and update +24
//! (0043C8F0). While the first player's unit is inside its radius (when one
//! is set) and above its ceiling, every update fires shots at the unit
//! through the particle factory 00466710, more of them the higher it is, and
//! sends hint 0x2E. Artoria and the Defence Satellite place one each; a
//! command disposes it once the first player owns a given grant.

use inf_formats::InstanceCommandPayload;
use thiserror::Error;

use super::{
    native_trunc_low32, GrantOwner, LiveGrantError, ObjectArena, ObjectClass, ObjectError,
    ObjectId, PlayerAttachError, PlayerId, VisualObject,
};
use crate::name_entry::name_entry_rng_next;

/// `0052261C`: the shots fly at 4000.
const SHOT_SPEED: f32 = 4000.0;
/// `0043CC5E`: the hint each in-range update sends (`0046C760(0x2E, 0)`).
pub const CEILING_HINT: u32 = 0x2e;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector48State {
    pub visual: VisualObject,
    /// +44, the ceiling's height, a signed WORD.
    pub ceiling_44: i16,
    /// +46, the radius around the ceiling object; 0 is none.
    pub radius_46: u16,
    /// +48, nonzero: the unit's height counts from the ceiling object's.
    pub relative_48: u8,
}

impl Selector48State {
    pub(super) fn new() -> Self {
        Self {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            ceiling_44: 0,
            radius_46: 0,
            relative_48: 0,
        }
    }
}

/// The services the update reaches.
pub trait Selector48Host {
    type Error: std::fmt::Display;
    /// `00466710(kind, position, velocity, 0054E894, 0)`: one shot, owned
    /// by the process NULL, with the process RNG the factory may draw.
    fn shot(
        &mut self,
        arena: &mut ObjectArena,
        kind: u32,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// `0046C760(0x2E, 0)` on the first player.
    fn hint(&mut self, arena: &mut ObjectArena, player: PlayerId) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector48Error<E: std::fmt::Display> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Grant(#[from] LiveGrantError),
    #[error("object {0:?} has no position for the ceiling's test")]
    PositionUnknown(ObjectId),
    #[error("ceiling {object:?} service at {address:08X}: {error}")]
    Host {
        object: ObjectId,
        address: u32,
        error: E,
    },
}

impl ObjectArena {
    pub fn selector48(&self, object: ObjectId) -> Result<&Selector48State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector48(state) => Ok(state),
            _ => Err(ObjectError::NotSelector48(object)),
        }
    }

    fn selector48_mut(&mut self, object: ObjectId) -> Result<&mut Selector48State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector48(state) => Ok(state),
            _ => Err(ObjectError::NotSelector48(object)),
        }
    }

    /// 0043C840 codes 3..6; codes up to 2 are the visual base's (0047A1A0)
    /// and higher ones refused, as for any other payload. Code 6 disposes
    /// the ceiling (00479FA0) when the first player owns the grant
    /// (004752E0), and accepts either way, with no player too.
    pub(super) fn apply_selector48_command(
        &mut self,
        object: ObjectId,
        payload: &InstanceCommandPayload,
    ) -> Result<bool, Selector48Error<std::convert::Infallible>> {
        match payload {
            InstanceCommandPayload::Selector48Ceiling(ceiling) => {
                self.selector48_mut(object)?.ceiling_44 = ceiling.value as i16;
            }
            InstanceCommandPayload::Selector48Radius(radius) => {
                self.selector48_mut(object)?.radius_46 = radius.value;
            }
            InstanceCommandPayload::Selector48Relative(relative) => {
                self.selector48_mut(object)?.relative_48 = relative.value;
            }
            InstanceCommandPayload::Selector48Grant(grant) => {
                self.selector48(object)?;
                if let Some(player) = self.first_player() {
                    if self.live_grant_has(GrantOwner::Player9c(player), grant.value)? {
                        self.dispose(object)?;
                    }
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 0043C8F0. Nothing happens without a first player (00488AA0) or its
    /// unit (0046AE60), outside a set radius (the wrapped squared distance,
    /// signed, against the radius squared), or at or below the ceiling (the
    /// unit's height, less the ceiling object's when relative, as a signed
    /// WORD). Otherwise the margin is the ceiling less that height plus
    /// 2000, at least 1, and one shot leaves for every `(margin * 5 +
    /// 20000) * 2` microseconds of the delta, rounded up (none for no time).
    /// Each draws the process RNG seven times (0048F060): it starts within
    /// half of `max(4 * margin, 2000)` of the unit across, and `margin / 8`
    /// plus up to 1000 above it, and flies at 4000 toward the unit led by a
    /// fortieth of its velocity, within half the margin, as kind 2 or 5.
    /// The hint follows.
    pub fn tick_selector48<H: Selector48Host>(
        &mut self,
        object: ObjectId,
        delta: u32,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), Selector48Error<H::Error>> {
        let Some(player) = self.first_player() else {
            return Ok(());
        };
        let Some(unit) = self.player_actor(player)? else {
            return Ok(());
        };
        let state = *self.selector48(object)?;
        let here = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector48Error::PositionUnknown(object))?;
        let there = self
            .object(unit)?
            .base
            .position_words
            .ok_or(Selector48Error::PositionUnknown(unit))?;
        if state.radius_46 != 0 {
            let squared = (0..3).fold(0_i32, |sum, axis| {
                let delta = i32::from(here[axis].wrapping_sub(there[axis]) as i16);
                sum.wrapping_add(delta.wrapping_mul(delta))
            });
            let radius = i32::from(state.radius_46);
            if squared >= radius.wrapping_mul(radius) {
                return Ok(());
            }
        }
        let height = if state.relative_48 != 0 {
            there[1].wrapping_sub(here[1])
        } else {
            there[1]
        } as i16;
        if height <= state.ceiling_44 {
            return Ok(());
        }
        let margin = (i32::from(state.ceiling_44) - i32::from(height) + 2000).max(1);
        let spread = (margin * 4).max(2000);
        if delta != 0 {
            let (above, half_spread, half_margin) = (margin / 8, spread / 2, margin / 2);
            let step = ((margin * 5 + 20000) * 2) as u32;
            let velocity = self.actor(unit)?.words_8e_92;
            let mut elapsed = 0_u32;
            loop {
                let mut draw = || (name_entry_rng_next(rng) & 0xffff) as i32;
                let mut start = there;
                start[0] = start[0].wrapping_add((draw() % spread - half_spread) as u16);
                start[1] = start[1].wrapping_add((draw() % 1000 + above) as u16);
                start[2] = start[2].wrapping_add((draw() % spread - half_spread) as u16);
                // A fortieth of the velocity, toward zero (0x66666667 >> 4).
                let mut aim: [u16; 3] = std::array::from_fn(|axis| {
                    ((i32::from(velocity[axis] as i16) / 40) as u16).wrapping_add(there[axis])
                });
                for word in &mut aim {
                    *word = word.wrapping_add((draw() % margin - half_margin) as u16);
                }
                let kind = if draw() & 1 != 0 { 5 } else { 2 };
                let velocity = shot_velocity(start, aim);
                host.shot(self, kind, start, velocity, rng)
                    .map_err(|error| Selector48Error::Host {
                        object,
                        address: 0x0043_cc39,
                        error,
                    })?;
                elapsed = elapsed.wrapping_add(step);
                if elapsed >= delta {
                    break;
                }
            }
        }
        host.hint(self, player)
            .map_err(|error| Selector48Error::Host {
                object,
                address: 0x0043_cc62,
                error,
            })
    }
}

/// `0043CB23..0043CC09`: the aim less the start, as signed WORDs, through
/// x87. Y and Z are stored as f32 first, the length's square sums Z, Y,
/// then X, and the length is stored as f32 for the divisions. X keeps its
/// quotient in the register while Y's and Z's are stored as f32. A zero
/// length aims along X. Each is scaled by 4000 and truncated (`__ftol`).
fn shot_velocity(start: [u16; 3], aim: [u16; 3]) -> [u16; 3] {
    let [x, y, z] = std::array::from_fn::<f64, 3, _>(|axis| {
        f64::from(aim[axis].wrapping_sub(start[axis]) as i16)
    });
    let (y32, z32) = (y as f32, z as f32);
    let squared = (z * f64::from(z32) + f64::from(y32) * f64::from(y32)) + x * x;
    let length = squared.sqrt();
    let (along, up, across) = if length == 0.0 {
        (1.0, 0.0_f32, 0.0_f32)
    } else {
        let length32 = f64::from(length as f32);
        (
            x / length32,
            (f64::from(y32) / length32) as f32,
            (f64::from(z32) / length32) as f32,
        )
    };
    let scale = f64::from(SHOT_SPEED);
    [
        native_trunc_low32(along * scale) as u16,
        native_trunc_low32(f64::from(up) * scale) as u16,
        native_trunc_low32(f64::from(across) * scale) as u16,
    ]
}

#[cfg(test)]
#[path = "selector48_tests.rs"]
mod tests;
