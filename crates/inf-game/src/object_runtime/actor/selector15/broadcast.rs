//! `0047DD20(0x300, code, 0)` from a player's unit (`0046AE60`): the tail of
//! `004137D0`, which shows a menu on the player's message (Tab's computer
//! and the factory's shop, code 6), and Escape's `00469480` (code 0x18).
//! Every actor within 0x300 of the unit gets its `+98`, and the answers add
//! up; a nonzero sum after a shown menu sets the message's `+34`.
//!
//! The factory's `0042F380` is the receiver that acts on them. Code 6 re-arms
//! its dock (`+1BD = 1`), so a slow second Tab on the pad opens the computer
//! and the factory docks the vehicle again at its next update, its shop over
//! the computer. Code 0x18 sends Facilities and Energy Distribution back to
//! the Options (`00431CE0(0, 0)`); in any other state it hides the first
//! player's message (`004133C0`), whatever that message shows. Both answer
//! 1. Every other receiver the port builds answers 0 and changes nothing:
//! the vehicle's `00420BE0` looks its player up first, the grenades' and
//! mines' `004557F0` and the missiles' `00456AD0` act on other codes.

use crate::object_runtime::{
    ActorMessage, ActorMessageError, ActorMessageReply, ActorMessageWalk, ObjectArena, ObjectId,
    PlayerActorLookupContext, PlayerId, VehicleMessageEffect,
};

use super::factory::{FactoryError, FactoryStaffHost};

/// `004137D0`'s and `00469480`'s radius.
const UNIT_BROADCAST_RADIUS: u32 = 0x300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitBroadcastCode {
    /// 6, after `004137D0` shows a menu. A nonzero sum sets the message's
    /// `+34`, which nothing reads.
    MenuShown,
    /// 0x18, Escape's `00469480`.
    Escape,
}

impl UnitBroadcastCode {
    fn code(self) -> i32 {
        match self {
            Self::MenuShown => 6,
            Self::Escape => 0x18,
        }
    }
}

/// One broadcast, with the unit `0046AE60` found when it was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitBroadcast {
    pub player: PlayerId,
    pub unit: ObjectId,
    pub code: UnitBroadcastCode,
}

/// What a broadcast reached.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UnitBroadcastOutcome {
    /// The answers' wrapping sum.
    pub sum: u32,
    /// Receivers whose `+98` the port does not build; each is skipped.
    pub skipped: Vec<String>,
}

impl ObjectArena {
    /// The broadcast's walk (`0047DAF0`'s radius arm from the unit's
    /// position), each receiver's `+98(unit, distance², code, 0)`, then
    /// `004137D0`'s `+34` store. A receiver without a ported `+98` is
    /// skipped and reported; the walk goes on.
    pub fn run_unit_broadcast<H: FactoryStaffHost>(
        &mut self,
        broadcast: UnitBroadcast,
        host: &mut H,
    ) -> Result<UnitBroadcastOutcome, FactoryError<H::Error>> {
        let UnitBroadcast { player, unit, code } = broadcast;
        let mut skipped = Vec::new();
        let sum = self.walk_message_receivers(
            unit,
            ActorMessageWalk::Radius {
                radius: UNIT_BROADCAST_RADIUS,
            },
            |arena, actor, distance_squared| {
                if arena.selector15(actor).is_ok() {
                    return arena.factory_unit_message(actor, code, host);
                }
                let network_active = host.process().network_flag != 0;
                let reply = arena.receive_actor_message(
                    actor,
                    ActorMessage {
                        sender: Some(unit),
                        distance_squared,
                        code: code.code(),
                        argument: 0,
                    },
                    PlayerActorLookupContext {
                        process: Some(host.process()),
                    },
                    network_active,
                );
                match reply {
                    Ok(ActorMessageReply::Done(answer)) => Ok(answer),
                    Ok(ActorMessageReply::Vehicle(VehicleMessageEffect::None)) => Ok(0),
                    Ok(reply) => Err(FactoryError::UnitBroadcastEffect { actor, reply }),
                    Err(error @ ActorMessageError::Unported { .. }) => {
                        skipped.push(error.to_string());
                        Ok(0)
                    }
                    Err(error) => Err(FactoryError::UnitBroadcast(error)),
                }
            },
        )?;
        if code == UnitBroadcastCode::MenuShown && sum != 0 {
            self.set_player_message_byte_34(player)?;
        }
        Ok(UnitBroadcastOutcome { sum, skipped })
    }

    /// `0042F380`'s arms for the unit's codes. It looks the first player up
    /// (`00488AA0`) before any arm.
    fn factory_unit_message<H: FactoryStaffHost>(
        &mut self,
        factory: ObjectId,
        code: UnitBroadcastCode,
        host: &mut H,
    ) -> Result<u32, FactoryError<H::Error>> {
        let first = self.first_player();
        match code {
            // 0042F472: re-armed, answering 1.
            UnitBroadcastCode::MenuShown => self.selector15_mut(factory)?.byte_1bd = 1,
            // 0042F3A7..0042F3D7.
            UnitBroadcastCode::Escape => {
                let state = self.selector15(factory)?.word_160;
                match first {
                    Some(player) if !matches!(state, 1 | 2) => self.hide_player_message(player)?,
                    _ => self.set_factory_state(factory, 0, false, host)?,
                }
            }
        }
        Ok(1)
    }
}
