//! Selector 50: constructor 0043FE60 over the visual base 0047A030, final
//! vtable 00522888, a placed hint trigger. It differs from the visual base
//! 00527B68 only in its destructor, commands +04 (0043FF40) and update +24
//! (0043FEB0). While the first player's primary actor stays within the
//! radius past the delay, the update sets the trigger's player command
//! (0046C760), which sends that command's hint. Level 1's tutorial hints
//! come from nine of these.

use inf_formats::InstanceCommandPayload;
use thiserror::Error;

use super::{
    ObjectArena, ObjectClass, ObjectError, ObjectId, PlayerCommandContext, PlayerCommandError,
    PlayerCommandOperation, PlayerCommandOutcome, PlayerCommandRequest, VisualObject,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector50State {
    pub visual: VisualObject,
    /// +44, the squared radius; the constructor leaves it unwritten.
    pub radius_squared_44: Option<u32>,
    /// +48, the delay in microseconds, zeroed by the constructor.
    pub delay_micros_48: u32,
    /// +4C, the player command; the constructor leaves it unwritten.
    pub command_4c: Option<u32>,
    /// +50, the command's silent byte.
    pub silent_50: u8,
    /// +54, the time the actor has spent in range.
    pub timer_54: u32,
}

impl Selector50State {
    pub(super) fn new() -> Self {
        Self {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            radius_squared_44: None,
            delay_micros_48: 0,
            command_4c: None,
            silent_50: 0,
            timer_54: 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum Selector50Error {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Command(#[from] PlayerCommandError),
    #[error("selector-50 trigger {object:?} reads unwritten +{offset:02X}")]
    RequiredField { object: ObjectId, offset: u8 },
    #[error("object {0:?} has no position for the selector-50 range test")]
    PositionUnknown(ObjectId),
    /// 0043FFBC queries the first player through a NULL this when none exists.
    #[error("selector-50 trigger {0:?} command 5 needs a first player")]
    MissingFirstPlayer(ObjectId),
}

impl ObjectArena {
    pub fn selector50(&self, object: ObjectId) -> Result<&Selector50State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector50(state) => Ok(state),
            _ => Err(ObjectError::NotSelector50(object)),
        }
    }

    fn selector50_mut(&mut self, object: ObjectId) -> Result<&mut Selector50State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector50(state) => Ok(state),
            _ => Err(ObjectError::NotSelector50(object)),
        }
    }

    /// 0043FF40 codes 3..6. Returns false for any other payload, which the
    /// caller reports as a rejection. Code 5 disposes the trigger (00479FA0)
    /// when the first player already owns the command, and still accepts.
    pub(super) fn apply_selector50_command(
        &mut self,
        object: ObjectId,
        payload: &InstanceCommandPayload,
    ) -> Result<bool, Selector50Error> {
        match payload {
            InstanceCommandPayload::Selector50Radius(radius) => {
                let radius = u32::from(radius.value);
                self.selector50_mut(object)?.radius_squared_44 = Some(radius * radius);
            }
            InstanceCommandPayload::Selector50Seconds(seconds) => {
                self.selector50_mut(object)?.delay_micros_48 =
                    u32::from(seconds.value).wrapping_mul(1_000_000);
            }
            InstanceCommandPayload::Selector50Command(command) => {
                self.selector50_mut(object)?.command_4c = Some(command.value);
                let player = self
                    .first_player()
                    .ok_or(Selector50Error::MissingFirstPlayer(object))?;
                let owned = self.apply_player_command(
                    PlayerCommandRequest {
                        player,
                        operation: PlayerCommandOperation::Query {
                            raw_id: command.value,
                        },
                    },
                    &PlayerCommandContext {
                        process: None,
                        executable: None,
                        resident: None,
                    },
                )?;
                if owned == PlayerCommandOutcome::Owned(true) {
                    self.dispose(object)?;
                }
            }
            InstanceCommandPayload::Selector50Silent(flag) => {
                self.selector50_mut(object)?.silent_50 = u8::from(flag.value != 0);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 0043FEB0. Nothing happens without a first player (00488AA0) or its
    /// primary actor (0046B030); otherwise [`selector50_step`] runs, and a
    /// fire sets the command, which the setter ignores once owned.
    pub fn tick_selector50(
        &mut self,
        object: ObjectId,
        microseconds: u32,
        context: &PlayerCommandContext<'_>,
    ) -> Result<(), Selector50Error> {
        let Some(player) = self.first_player() else {
            return Ok(());
        };
        let Some(actor) = self.first_player_primary_actor()? else {
            return Ok(());
        };
        let state = *self.selector50(object)?;
        let here = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector50Error::PositionUnknown(object))?;
        let there = self
            .object(actor)?
            .base
            .position_words
            .ok_or(Selector50Error::PositionUnknown(actor))?;
        let step = selector50_step(&state, here, there, microseconds).ok_or(
            Selector50Error::RequiredField {
                object,
                offset: 0x44,
            },
        )?;
        // The timer is stored before +4C is read.
        self.selector50_mut(object)?.timer_54 = step.timer;
        if !step.fire {
            return Ok(());
        }
        let command = state.command_4c.ok_or(Selector50Error::RequiredField {
            object,
            offset: 0x4c,
        })?;
        self.apply_player_command(
            PlayerCommandRequest {
                player,
                operation: PlayerCommandOperation::Set {
                    raw_id: command,
                    silent: u32::from(state.silent_50),
                },
            },
            context,
        )?;
        Ok(())
    }
}

/// One update's result: the new +54 and whether the command is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector50Step {
    pub timer: u32,
    pub fire: bool,
}

/// 0043FEC8..0043FF24. The squared distance sums WORD-wrapped signed
/// differences; above +44 (unsigned) the timer restarts. In range the
/// timer adds the delta, and passing +48 (unsigned) fires. None when the
/// radius +44 is unwritten.
pub fn selector50_step(
    state: &Selector50State,
    here: [u16; 3],
    there: [u16; 3],
    microseconds: u32,
) -> Option<Selector50Step> {
    let squared = (0..3).fold(0u32, |sum, axis| {
        let delta = i32::from(here[axis].wrapping_sub(there[axis]) as i16);
        sum.wrapping_add(delta.wrapping_mul(delta) as u32)
    });
    if squared > state.radius_squared_44? {
        return Some(Selector50Step {
            timer: 0,
            fire: false,
        });
    }
    let timer = state.timer_54.wrapping_add(microseconds);
    Some(Selector50Step {
        timer,
        fire: timer > state.delay_micros_48,
    })
}

#[cfg(test)]
#[path = "selector50_tests.rs"]
mod tests;
