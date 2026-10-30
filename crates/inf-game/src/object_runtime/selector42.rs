//! Selector 42: constructor 00401020 over the visual base 0047A030, final
//! vtable 0051F018, a placed wake trigger. Its update +24 (004011F0) waits
//! for the first player's primary actor (0046B030) or remote unit (0046B060)
//! within its radius; its draw +2C (004012D0) draws nothing, but with +4C
//! set it fires at once outside UI state 3. Firing (00401110) wakes every
//! dormant actor (flag 0x200000) within the activation radius, then
//! disposes the trigger. Actor command 18 put those actors to sleep with a
//! negated +144 delay and flags 0x04200000.

use super::{mark_flag_change, ObjectArena, ObjectClass, ObjectError, ObjectId, VisualObject};
use inf_formats::InstanceCommandPayload;

/// Flag 0x200000 marks a dormant actor; firing sets 0x400000 on it, then
/// clears 0x04600000 on its +58 component.
const DORMANT: u32 = 0x0020_0000;
const WOKEN: u32 = 0x0040_0000;
const SLEEP_FLAGS: u32 = 0x0460_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector42State {
    pub visual: VisualObject,
    /// +44, the squared trigger radius, zeroed by the constructor.
    pub radius_squared_44: u32,
    /// +48, the squared wake radius; the constructor stores 2500 squared.
    pub wake_squared_48: u32,
    /// +4C: the draw fires the trigger.
    pub on_draw_4c: u8,
    /// +4D: the update does nothing.
    pub disabled_4d: u8,
}

impl Selector42State {
    pub(super) fn new() -> Self {
        Self {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            radius_squared_44: 0,
            wake_squared_48: 0x005f_5e10,
            on_draw_4c: 0,
            disabled_4d: 0,
        }
    }
}

/// The XZ square both 004011F0 and 00401110 use: WORD-wrapped signed
/// differences, squared and summed in 32 bits.
fn ground_square(here: [u16; 3], there: [u16; 3]) -> u32 {
    let x = i32::from(here[0].wrapping_sub(there[0]) as i16);
    let z = i32::from(here[2].wrapping_sub(there[2]) as i16);
    x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z)) as u32
}

impl ObjectArena {
    pub fn selector42(&self, object: ObjectId) -> Result<&Selector42State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector42(state) => Ok(state),
            _ => Err(ObjectError::NotSelector42(object)),
        }
    }

    fn selector42_mut(&mut self, object: ObjectId) -> Result<&mut Selector42State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector42(state) => Ok(state),
            _ => Err(ObjectError::NotSelector42(object)),
        }
    }

    fn selector42_position(&self, object: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(object)?
            .base
            .position_words
            .ok_or(ObjectError::Selector42PositionUnknown(object))
    }

    /// 00401080 codes 3..6: WORD radii squared into +44/+48, BYTE != 0
    /// into +4C/+4D. Returns false for any other payload.
    pub(super) fn apply_selector42_command(
        &mut self,
        object: ObjectId,
        payload: &InstanceCommandPayload,
    ) -> Result<bool, ObjectError> {
        let state = self.selector42_mut(object)?;
        match payload {
            InstanceCommandPayload::Selector42Radius(radius) => {
                let radius = u32::from(radius.value);
                state.radius_squared_44 = radius * radius;
            }
            InstanceCommandPayload::Selector42WakeRadius(radius) => {
                let radius = u32::from(radius.value);
                state.wake_squared_48 = radius * radius;
            }
            InstanceCommandPayload::Selector42OnDraw(flag) => {
                state.on_draw_4c = u8::from(flag.value != 0);
            }
            InstanceCommandPayload::Selector42Disabled(flag) => {
                state.disabled_4d = u8::from(flag.value != 0);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 004011F0. Of the first player's primary actor and remote unit, the
    /// nearer on the ground (the primary on a tie loses to the remote) fires
    /// the trigger when its square is below +44 (unsigned).
    pub fn tick_selector42(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        let state = *self.selector42(object)?;
        if state.disabled_4d != 0 {
            return Ok(());
        }
        let here = self.selector42_position(object)?;
        let primary = self.first_player_primary_actor()?;
        let remote = self.first_player_remote_actor()?;
        let square = |arena: &Self, actor| -> Result<u32, ObjectError> {
            Ok(ground_square(here, arena.selector42_position(actor)?))
        };
        let nearest = match (primary, remote) {
            (None, None) => return Ok(()),
            (Some(actor), None) | (None, Some(actor)) => square(self, actor)?,
            (Some(primary), Some(remote)) => {
                let (primary, remote) = (square(self, primary)?, square(self, remote)?);
                if primary < remote {
                    primary
                } else {
                    remote
                }
            }
        };
        if nearest < state.radius_squared_44 {
            self.fire_selector42(object)?;
        }
        Ok(())
    }

    /// 004012D0, the trigger's draw: with +4C set it fires outside UI
    /// state 3 (the briefing). Nothing is drawn.
    pub fn draw_selector42(&mut self, object: ObjectId, ui_state: u32) -> Result<(), ObjectError> {
        if self.selector42(object)?.on_draw_4c != 0 && ui_state != 3 {
            self.fire_selector42(object)?;
        }
        Ok(())
    }

    /// 00401110 over the G+48 actor list, then 00479FA0.
    fn fire_selector42(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        let here = self.selector42_position(object)?;
        let reach = self.selector42(object)?.wake_squared_48;
        let actors = self.actor_list().to_vec();
        for actor in actors {
            let flags = self.object(actor)?.base.flags;
            if flags & DORMANT == 0 {
                continue;
            }
            if ground_square(here, self.selector42_position(actor)?) > reach {
                continue;
            }
            mark_flag_change(&mut self.owned_mut(actor)?.state.base, flags | WOKEN);
            let Some(component) = self.actor_derived_component(actor)? else {
                continue;
            };
            let derived = self.derived_actor_mut(component)?;
            if derived.word_144 < 0 {
                derived.word_144 = derived.word_144.wrapping_mul(-1000);
            }
            let base = &mut self.owned_mut(component)?.state.base;
            let flags = base.flags & !SLEEP_FLAGS;
            mark_flag_change(base, flags);
        }
        self.dispose(object)
    }
}

#[cfg(test)]
#[path = "selector42_tests.rs"]
mod tests;
