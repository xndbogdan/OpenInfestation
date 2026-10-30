//! Selector 43: constructor 004541C0 over the visual base 0047A030, final
//! vtable 00525730, 100 bytes: a timed spawner. It differs from the visual
//! base 00527B68 only in its destructor, commands +04 (00454220), update +24
//! (00454310) and network slot +40 (004542E0), which the offline game never
//! calls. It alternates an on phase (+44) and an off phase (+48); each on
//! phase raises event 0xE to every actor (`0047DAF0`, which sets off the
//! Sirens), and while on it launches rocks (resource 0x18, selector 44) at
//! a rate that grows with the time since the last. Luna Azine, Shak'tahh
//! and Caradion place them.

use inf_formats::{native_basis::sine_q31, InstanceCommandPayload};
use thiserror::Error;

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, VisualObject};
use crate::{name_entry::name_entry_rng_next, terrain_runtime::TerrainRuntime};

/// `00454353`: the event each on phase raises.
pub const ERUPTION_EVENT: i32 = 0xe;
/// `004545F3`: the rocks' resource.
pub const ROCK_RESOURCE: u32 = 0x18;
/// `0045437E..0045438E`: a spawn needs the low-byte draw times the phase's
/// time since the last spawn, its low byte cleared, above this.
const SPAWN_THRESHOLD: u32 = 0x0493_e000;
/// `005377B8`/`005377C0`: the off-ring spawns' divisor and their velocity,
/// constants of the static at 005377B0.
const FALL_DIVISOR: i32 = 0x3000;
const FALL_VELOCITY: [u16; 3] = [0xfc00, 0xfe00, 0];
/// `005377C8`: the spin 0047B4B0 gives every rock, a local DWORD vector.
const ROCK_SPIN: [i32; 3] = [0x400; 3];
/// `SHL EAX,1; RCL EDX,1`: the high DWORD of the doubled product.
fn scale(sine: i32, length: i32) -> u16 {
    ((i64::from(sine) * i64::from(length)) << 1 >> 32) as u16
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector43State {
    pub visual: VisualObject,
    /// +44 and +48, the on and off phases in microseconds.
    pub on_44: u32,
    pub off_48: u32,
    /// +4C, the off-ring spawns' spread.
    pub spread_4c: u32,
    /// +50: spawns leave a ring of 256 around the spawner, thrown outward.
    pub ring_50: u8,
    pub byte_51: u8,
    /// +54, the time left in the phase; +58 stays zero.
    pub timer_54: u32,
    pub dword_58: u32,
    /// +5C, the phase's time since the last spawn.
    pub elapsed_5c: u32,
    /// +60, set in the on phase.
    pub phase_60: u8,
}

impl Selector43State {
    pub(super) fn new() -> Self {
        Self {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            on_44: 0,
            off_48: 0,
            spread_4c: 0,
            ring_50: 0,
            byte_51: 0,
            timer_54: 0,
            dword_58: 0,
            elapsed_5c: 0,
            phase_60: 0,
        }
    }
}

/// The services the update reaches.
pub trait Selector43Host {
    type Error: std::fmt::Display;
    /// The H12 terrain the spawns stand on (`00405540`, then the cell).
    fn terrain(&self) -> &TerrainRuntime;
    /// `0047DAF0(NULL, 0, 0xE, +44, position)`: every actor's +98.
    fn eruption(
        &mut self,
        arena: &mut ObjectArena,
        argument: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// `00460310(0x18, position, NULL, -1, NULL, 0054E894)`: one rock.
    fn launch(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector43Error<E: std::fmt::Display> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("spawner {0:?} has no position")]
    PositionUnknown(ObjectId),
    /// 00454604 uses the factory's answer without a NULL test.
    #[error("spawner {0:?} launched no rock (00460310 answered NULL)")]
    MissingRock(ObjectId),
    #[error("spawner {object:?} service at {address:08X}: {error}")]
    Host {
        object: ObjectId,
        address: u32,
        error: E,
    },
}

impl ObjectArena {
    pub fn selector43(&self, object: ObjectId) -> Result<&Selector43State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector43(state) => Ok(state),
            _ => Err(ObjectError::NotSelector43(object)),
        }
    }

    fn selector43_mut(&mut self, object: ObjectId) -> Result<&mut Selector43State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector43(state) => Ok(state),
            _ => Err(ObjectError::NotSelector43(object)),
        }
    }

    /// 00454220 codes 3..7: the on and off seconds (WORD times 1,000,000),
    /// the spread WORD, the ring byte and DWORD != 0 at +51; codes up to 2
    /// are the visual base's (0047A1A0) and higher ones refused.
    pub(super) fn apply_selector43_command(
        &mut self,
        object: ObjectId,
        payload: &InstanceCommandPayload,
    ) -> Result<bool, ObjectError> {
        let state = self.selector43_mut(object)?;
        match payload {
            InstanceCommandPayload::Selector43OnSeconds(seconds) => {
                state.on_44 = u32::from(seconds.value).wrapping_mul(1_000_000)
            }
            InstanceCommandPayload::Selector43OffSeconds(seconds) => {
                state.off_48 = u32::from(seconds.value).wrapping_mul(1_000_000)
            }
            InstanceCommandPayload::Selector43Spread(spread) => {
                state.spread_4c = u32::from(spread.value)
            }
            InstanceCommandPayload::Selector43Ring(ring) => state.ring_50 = ring.value,
            InstanceCommandPayload::Selector43Byte51(value) => {
                state.byte_51 = u8::from(value.value != 0)
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 00454310. When +54 runs out (unsigned, below the delta) the phase
    /// turns, +54 gains the next phase's length and +5C clears; a phase
    /// turning on raises event 0xE. Then +54 counts the delta down and +5C
    /// up. In the on phase a spawn happens when the low byte of one draw
    /// times +5C, its own low byte cleared, passes 0x493E000: the next draw
    /// is the angle, then either a ring of 256 thrown outward at 0x400 and
    /// up to 0x7FF high (+50), or a draw within twice the spread, at the
    /// static's offset, 0x3000 above the ground and its velocity.
    /// The rock gets the static's spin (0047B4B0) and the velocity
    /// (0047DDF0), and +5C clears.
    pub fn tick_selector43<H: Selector43Host>(
        &mut self,
        object: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), Selector43Error<H::Error>> {
        let fail = |address| {
            move |error| Selector43Error::Host {
                object,
                address,
                error,
            }
        };
        let state = *self.selector43(object)?;
        if state.timer_54 < delta {
            let next = if state.phase_60 != 0 {
                state.off_48
            } else {
                state.on_44
            };
            let state = self.selector43_mut(object)?;
            state.elapsed_5c = 0;
            state.timer_54 = state.timer_54.wrapping_add(next);
            state.phase_60 = u8::from(state.phase_60 == 0);
            if state.phase_60 != 0 {
                let on = state.on_44;
                host.eruption(self, on, rng).map_err(fail(0x0045_4359))?;
            }
        }
        let state = self.selector43_mut(object)?;
        state.timer_54 = state.timer_54.wrapping_sub(delta);
        state.elapsed_5c = state.elapsed_5c.wrapping_add(delta);
        if state.phase_60 == 0 {
            return Ok(());
        }
        let elapsed = state.elapsed_5c;
        let draw = name_entry_rng_next(rng) & 0xff;
        if draw.wrapping_mul(elapsed) & !0xff <= SPAWN_THRESHOLD {
            return Ok(());
        }
        let state = *self.selector43(object)?;
        let mut position = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector43Error::PositionUnknown(object))?;
        let angle = i32::from(name_entry_rng_next(rng) as u16 as i16);
        let length = if state.ring_50 != 0 {
            0x100
        } else {
            ((name_entry_rng_next(rng) & 0xffff) as i32).wrapping_mul(state.spread_4c as i32) >> 15
        };
        let quarter = angle.wrapping_add(0x4000);
        position[0] =
            position[0].wrapping_add(scale(sine_q31(angle, sine_quarter), length as i16 as i32));
        position[2] =
            position[2].wrapping_add(scale(sine_q31(quarter, sine_quarter), length as i16 as i32));
        let ground = host.terrain().raw_height_at([position[0], position[2]]) as u16;
        let velocity = if state.ring_50 != 0 {
            position[1] = ground.wrapping_add(0x100);
            let speed = (length << 2) as i16 as i32;
            let up = (name_entry_rng_next(rng) & 0x7ff) as u16;
            [
                scale(sine_q31(angle, sine_quarter), speed),
                up,
                scale(sine_q31(quarter, sine_quarter), speed),
            ]
        } else {
            let fall = |word: u16| i32::from(word as i16);
            let divisor = (fall(FALL_VELOCITY[1]) << 16) / FALL_DIVISOR;
            position[0] =
                position[0].wrapping_add((((fall(FALL_VELOCITY[0]) << 8) / divisor) << 8) as u16);
            position[2] =
                position[2].wrapping_add((((fall(FALL_VELOCITY[2]) << 8) / divisor) << 8) as u16);
            // 004545DA adds BX, which 00454590 reloaded with the divisor:
            // the rock starts 0x3000 above the ground.
            position[1] = ground.wrapping_add(FALL_DIVISOR as u16);
            FALL_VELOCITY
        };
        let rock = host
            .launch(self, position, rng)
            .map_err(fail(0x0045_45f8))?
            .ok_or(Selector43Error::MissingRock(object))?;
        // 0047B4B0, then 0047DDF0 with only the velocity.
        self.add_actor_local_angular(rock, ROCK_SPIN)?;
        self.actor_mut(rock)?.words_8e_92 = velocity;
        self.selector43_mut(object)?.elapsed_5c = 0;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector43_tests.rs"]
mod tests;
