//! The +98 of the actor classes the port builds, for a message that
//! 0047DD80 sends one actor, 0047DAF0's radius arm and 0047DBF0's team walk
//! send every actor near the sender, or 0047DAF0's zero-radius arm sends
//! every actor. The arms that write the receiver's
//! own fields run here. The vehicle's effects and retirement (0047CE90) are
//! left to the world; an arm the port does not own stops.
//!
//! | `+98` | Classes | Arms |
//! | --- | --- | --- |
//! | `004270B0` | derived and family bases, 10, 34, 35, 36, 77 | 0xD: `+17A` at least 8000 |
//! | `00408A70` | 79 | 3: a follower; 2: a follower's strike (its update's) |
//! | `00402420` | 11 | 0xB: `+1B0` at least 8000; 0xC: `+1B0` 12000; 0xD: `+17A` at least 8000; 0x15: retire; 7, 8: lifecycle |
//! | `00420BE0` | 2 | the vehicle's own (`vehicle_message`) |
//! | `00468BE0` | 12, 13 | 0xD: retire unless flag `0x80`; 0x16: grants |
//! | `00480F40` | 60 | 7, 8: lifecycle |
//! | `004045F0` | shared, 16, 59, 88, 90, 93 | none |
//! | `00454AB0` | 53 | 0xF: an idle mine with pickups left arms its timer and updates; an updating mine answers its kind |
//! | `004546F0` | 52 | 7: a link; 8: one less |
//! | `004787E0` | 18 | 7: a link, and it switches on; 8: one less; 0xB, 0xC: below radius 1000, off for 5 s |
//! | `004864B0` | 57 | 5: retires it, then its sender when that is resource 0xE |
//! | `00454160` | 44 | 5: retires it |
//! | `00458010` | 29 | 9 arms it, as 10 from its live owner does; online, 10 from no sender or a dead one disposes of it |
//! | `0047FE10` | 47 | 0xB, 0xC: silence for 10 s; 0xE: the alarm; else `0040DE70` |
//! | `00411CE0` | 69 | 7: a link; 8: one less, and the last releases it; 0x15: releases it |
//! | `00405490` | 66 | 5: moves it by the argument's vector |
//! | `0048C470` | 30 | 0x14, 0x15: `+164`; 0xD: none; else `0040DE70` |
//! | `0042F380` | 15 | 0xD unless remote: the staff go, "Factory neutralised", team 0; 6 and 0x18 (the unit's broadcast); else `0040DE70` |
//! | `0040A180` | 37 | 2, 3; else `0040DE70` |
//! | `0040DE70` | 49 (the family's own) | 7, 8: lifecycle |
//! | `0043BDF0` | 39 | 0xB, 0xC: `+164` 0 or 1, then retire with `+160`; 0x14, 0x15: `+166`; else `0040DE70` |
//! | `0040DE70` | the 0040D800 family | 7, 8: lifecycle |
//! | `004557F0` | 26, 38 | 9 and 10: marks it to detonate |
//! | `004860D0` | 32 | 1 and 2: the repeat |

use thiserror::Error;

use super::{ActorExtension, DerivedActorClass, Family428650Class, Family429e20Class};
use crate::object_runtime::{
    ActorMessage, DerivedActorError, ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext,
    PlayerId, VehicleMessageEffect, VehicleMessageError,
};

/// What a receiver's +98 leaves to its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorMessageReply {
    /// The full EAX it returned.
    Done(u32),
    /// 00420BE0's effect on the world; it returns 0.
    Vehicle(VehicleMessageEffect),
    /// 0047CE90 on the receiver; it returns 0.
    Retire,
    /// 004864B0: 0047CE90 on the receiver, then on the sender when the
    /// sender's +30, read after the first, is 0xE; it returns 0.
    RetireWithSender { sender: ObjectId },
    /// 004135D0(H0 string, 0) on the player's message; it returns 0.
    PlayerText { player: PlayerId, string: u16 },
    /// `00411D0E`: a nonzero `+7A` to the first player's grants +9C
    /// (`00488AA0`, `004750C0`), then `00479FA0` on the receiver; it
    /// returns 0.
    Release { grant: u16 },
}

#[derive(Debug, Error)]
pub enum ActorMessageError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Vehicle(#[from] VehicleMessageError),
    #[error(transparent)]
    Selector47(#[from] super::Selector47Error<std::convert::Infallible>),
    #[error("{actor:?}'s +98 {function:08X} for message {code:#x} is not ported")]
    Unported {
        actor: ObjectId,
        function: u32,
        code: i32,
    },
    /// The native reads the null sender's +30 at `address` and faults; the
    /// port stops before the receiver's writes.
    #[error("{actor:?}'s +98 {function:08X} reads a null sender's +30 at {address:08X}")]
    NullSender {
        actor: ObjectId,
        function: u32,
        address: u32,
    },
}

/// The receiver's +98, by its final table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Receiver {
    Family4270b0,
    Selector79,
    Selector11,
    Selector2,
    Selector12,
    Selector60,
    Shared4045f0,
    Selector53,
    Selector52,
    Selector17,
    Selector74,
    Selector18,
    Selector44,
    Selector57,
    Bomb29,
    Selector47,
    Selector69,
    Selector49,
    Selector66,
    Selector30,
    Selector15,
    Selector37,
    Selector39,
    Thrown,
    Missile,
    Shot32,
}

/// 8000 (`00402462`, `004270BC`), the floor a signed WORD timer is raised
/// to, and 12000 (`0040247B`).
const TIMER_FLOOR: u16 = 0x1f40;
const TIMER_LONG: u16 = 0x2ee0;

fn raise(timer: &mut u16) {
    if (*timer as i16) < TIMER_FLOOR as i16 {
        *timer = TIMER_FLOOR;
    }
}

impl ObjectArena {
    fn message_receiver(&self, actor: ObjectId) -> Result<Option<Receiver>, ObjectError> {
        if self.is_thrown(actor) {
            return Ok(Some(Receiver::Thrown));
        }
        if self.is_missile(actor) {
            return Ok(Some(Receiver::Missile));
        }
        if self.is_bomb29(actor) {
            return Ok(Some(Receiver::Bomb29));
        }
        if self.is_shot32(actor) {
            return Ok(Some(Receiver::Shot32));
        }
        Ok(match &self.actor(actor)?.extension {
            ActorExtension::Derived(derived) => match &derived.class {
                DerivedActorClass::Selector2(_) => Some(Receiver::Selector2),
                DerivedActorClass::Selector11(_) => Some(Receiver::Selector11),
                DerivedActorClass::Selector60(_) => Some(Receiver::Selector60),
                DerivedActorClass::Selector77(eye) if eye.pair.is_some() => {
                    Some(Receiver::Selector79)
                }
                DerivedActorClass::Base | DerivedActorClass::Selector77(_) => {
                    Some(Receiver::Family4270b0)
                }
                DerivedActorClass::Family428650(family) => match &family.class {
                    Family428650Class::Family429e20(probe) => match &probe.class {
                        Family429e20Class::Selector12(_) => Some(Receiver::Selector12),
                        Family429e20Class::Base | Family429e20Class::Family4236d0(_) => {
                            Some(Receiver::Family4270b0)
                        }
                    },
                    Family428650Class::Base
                    | Family428650Class::Selector34(_)
                    | Family428650Class::Selector35(_) => Some(Receiver::Family4270b0),
                },
            },
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector93(_) => Some(Receiver::Shared4045f0),
            ActorExtension::Selector53(_) => Some(Receiver::Selector53),
            ActorExtension::Selector52(_) => Some(Receiver::Selector52),
            ActorExtension::Selector17(_) => Some(Receiver::Selector17),
            ActorExtension::Selector74(_) => Some(Receiver::Selector74),
            ActorExtension::Selector18(_) => Some(Receiver::Selector18),
            ActorExtension::Selector44(_) => Some(Receiver::Selector44),
            ActorExtension::Selector57 => Some(Receiver::Selector57),
            // The banner's and the base's tables keep the shared +98.
            ActorExtension::Selector40(_) | ActorExtension::Selector41 => {
                Some(Receiver::Shared4045f0)
            }
            ActorExtension::Selector47(_) => Some(Receiver::Selector47),
            ActorExtension::Selector69(_) => Some(Receiver::Selector69),
            ActorExtension::Selector49(_) => Some(Receiver::Selector49),
            ActorExtension::Selector66(_) => Some(Receiver::Selector66),
            ActorExtension::Selector30(_) => Some(Receiver::Selector30),
            ActorExtension::Selector15(_) => Some(Receiver::Selector15),
            ActorExtension::Selector37(_) => Some(Receiver::Selector37),
            ActorExtension::Selector39(_) => Some(Receiver::Selector39),
            ActorExtension::BehaviorResource(_) => None,
        })
    }

    /// One receiver's +98(sender, distance², code, argument).
    pub fn receive_actor_message(
        &mut self,
        actor: ObjectId,
        message: ActorMessage,
        context: PlayerActorLookupContext<'_>,
        network_active: bool,
    ) -> Result<ActorMessageReply, ActorMessageError> {
        let code = message.code;
        let unported = |function: u32| ActorMessageError::Unported {
            actor,
            function,
            code,
        };
        let receiver = self.message_receiver(actor)?.ok_or_else(|| unported(0))?;
        let family40de70 = |code: i32| match code {
            7 | 8 => Err(unported(0x0040_de70)),
            _ => Ok(ActorMessageReply::Done(0)),
        };
        match receiver {
            Receiver::Selector2 => Ok(ActorMessageReply::Vehicle(self.vehicle_message(
                actor,
                message,
                context,
                network_active,
            )?)),
            Receiver::Thrown => Ok(ActorMessageReply::Done(self.grenade26_message(
                actor,
                message.sender,
                code as u32,
            )?)),
            Receiver::Missile => Ok(ActorMessageReply::Done(
                self.missile_message(actor, code as u32)?,
            )),
            Receiver::Bomb29 => {
                if self.bomb29_message(actor, message.sender, code, network_active)?
                    == super::Bomb29Message::Dispose
                {
                    self.dispose(actor)?;
                }
                Ok(ActorMessageReply::Done(0))
            }
            Receiver::Shot32 => Ok(ActorMessageReply::Done(u32::from(
                self.retrigger_shot32(actor, code as u32)?,
            ))),
            // 00408A70: message 3 makes the eye a follower; message 2's
            // strike (00408DA0) runs in the eyes' own update.
            Receiver::Selector79 => match self.selector79_message(actor, message.sender, code)? {
                super::Selector79Message::Done => Ok(ActorMessageReply::Done(0)),
                super::Selector79Message::Strike => Err(unported(0x0040_8a70)),
            },
            Receiver::Family4270b0 => {
                if code == 0xd {
                    raise(&mut self.derived_actor_mut(actor)?.word_17a);
                }
                Ok(ActorMessageReply::Done(0))
            }
            Receiver::Selector11 => match code {
                0xb => {
                    raise(&mut self.selector11_mut(actor)?.word_1b0);
                    Ok(ActorMessageReply::Done(0))
                }
                0xc => {
                    self.selector11_mut(actor)?.word_1b0 = TIMER_LONG;
                    Ok(ActorMessageReply::Done(0))
                }
                0xd => {
                    raise(&mut self.derived_actor_mut(actor)?.word_17a);
                    Ok(ActorMessageReply::Done(0))
                }
                0x15 => Ok(ActorMessageReply::Retire),
                7 | 8 => Err(unported(0x0040_2420)),
                _ => Ok(ActorMessageReply::Done(0)),
            },
            Receiver::Selector12 => match code {
                0xd if self.object(actor)?.base.flags & 0x80 == 0 => Ok(ActorMessageReply::Retire),
                0x16 => Err(unported(0x0046_8be0)),
                _ => Ok(ActorMessageReply::Done(0)),
            },
            Receiver::Selector60 => match code {
                7 | 8 => Err(unported(0x0048_0f40)),
                _ => Ok(ActorMessageReply::Done(0)),
            },
            Receiver::Shared4045f0 => Ok(ActorMessageReply::Done(0)),
            Receiver::Selector53 => match code {
                0xf => Ok(ActorMessageReply::Done(self.call_selector53(actor)?)),
                _ => Ok(ActorMessageReply::Done(0)),
            },
            Receiver::Selector52 => {
                self.selector52_message(actor, code)?;
                Ok(ActorMessageReply::Done(0))
            }
            // 0046E4A0: message 1 resets the Prison Unit; every message
            // answers 0.
            Receiver::Selector17 => {
                self.selector17_message(actor, code)?;
                Ok(ActorMessageReply::Done(0))
            }
            // 0042D6B0: links 7 and 8; every message answers 0.
            Receiver::Selector74 => {
                self.selector74_message(actor, code)?;
                Ok(ActorMessageReply::Done(0))
            }
            Receiver::Selector18 => Ok(ActorMessageReply::Done(
                self.selector18_message(actor, code)?,
            )),
            Receiver::Selector44 => Ok(if code == super::ROCK_RETIRE_MESSAGE {
                ActorMessageReply::Retire
            } else {
                ActorMessageReply::Done(0)
            }),
            Receiver::Selector57 => match code {
                super::POWERBALL_RETIRE_MESSAGE => match message.sender {
                    Some(sender) => Ok(ActorMessageReply::RetireWithSender { sender }),
                    None => Err(ActorMessageError::NullSender {
                        actor,
                        function: 0x0048_64b0,
                        address: 0x0048_64c0,
                    }),
                },
                _ => Ok(ActorMessageReply::Done(0)),
            },
            // 0047FE10 passes links 7 and 8 to 0040DE70, whose lifecycle
            // arms the port does not own here; it answers 0.
            Receiver::Selector47 => match code {
                7 | 8 => Err(unported(0x0040_de70)),
                _ => {
                    self.selector47_message(actor, code, message.argument)?;
                    Ok(ActorMessageReply::Done(0))
                }
            },
            Receiver::Selector69 => Ok(match self.selector69_message(actor, code)? {
                super::Selector69Message::Done => ActorMessageReply::Done(0),
                super::Selector69Message::Release { grant } => ActorMessageReply::Release { grant },
            }),
            Receiver::Selector66 => match code {
                5 if message.argument != 0 => Err(unported(0x0040_5490)),
                _ => Ok(ActorMessageReply::Done(0)),
            },
            Receiver::Selector30 => match code {
                0xd => Ok(ActorMessageReply::Done(0)),
                0x14 | 0x15 => Err(unported(0x0048_c470)),
                _ => family40de70(code),
            },
            Receiver::Selector15 => match code {
                6 | 0x18 => Err(unported(0x0042_f380)),
                // 0042F3E7..0042F44B, then 0040DE70, which answers 0.
                0xd if self.object(actor)?.base.flags & 0x8000_0000 == 0 => {
                    Ok(match self.smoke_out_factory(actor)? {
                        Some(player) => ActorMessageReply::PlayerText {
                            player,
                            string: super::selector15::FACTORY_NEUTRALISED_TEXT,
                        },
                        None => ActorMessageReply::Done(0),
                    })
                }
                _ => family40de70(code),
            },
            Receiver::Selector49 => family40de70(code),
            Receiver::Selector37 => match code {
                2 | 3 => Err(unported(0x0040_a180)),
                _ => family40de70(code),
            },
            Receiver::Selector39 => match code {
                0xb | 0xc => {
                    let state = self.selector39_mut(actor)?;
                    state.byte_164 = u8::from(code == 0xc);
                    Ok(if state.byte_160 != 0 {
                        ActorMessageReply::Retire
                    } else {
                        ActorMessageReply::Done(0)
                    })
                }
                0x14 => {
                    let state = self.selector39_mut(actor)?;
                    state.byte_166 = state.byte_166.wrapping_add(message.argument as u8);
                    Ok(ActorMessageReply::Done(0))
                }
                0x15 => {
                    let state = self.selector39_mut(actor)?;
                    state.byte_166 = state.byte_166.wrapping_sub(message.argument as u8);
                    Ok(ActorMessageReply::Done(0))
                }
                _ => family40de70(code),
            },
        }
    }
}

/// Who a broadcast reaches and from where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorMessageWalk {
    /// 0047DAF0's radius arm from 0047DD20: every actor, measured from the
    /// sender's live position (the formed `+32` pointer is reread). Radius 0
    /// is its other arm, every actor at distance 0 with no position read.
    Radius { radius: u32 },
    /// 0047DBF0: the actors of one side (`+B4`), measured from the
    /// position the sender had when the walk began. Radius 0 reaches the
    /// whole side at distance 0.
    Team { radius: u32, team: u32 },
}

impl ObjectArena {
    /// The walk over G+48 shared by 0047DAF0's radius arm and 0047DBF0.
    /// Each receiver within the radius (signed WORD deltas, wrapping sums,
    /// unsigned compare with the radius squared) gets `deliver(arena,
    /// receiver, distance²)`, and the full EAX results add up. The next node
    /// is read after each delivery; stopped construction or disposal
    /// withholds the callback, as for the other notifications.
    pub fn walk_message_receivers<E: From<ObjectError>>(
        &mut self,
        source: ObjectId,
        walk: ActorMessageWalk,
        mut deliver: impl FnMut(&mut ObjectArena, ObjectId, u32) -> Result<u32, E>,
    ) -> Result<u32, E> {
        if walk == (ActorMessageWalk::Radius { radius: 0 }) {
            return self.walk_every_message_receiver(|arena, actor| deliver(arena, actor, 0));
        }
        let position = |arena: &ObjectArena, actor: ObjectId| {
            arena
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))
        };
        let (radius, team) = match walk {
            ActorMessageWalk::Radius { radius } => (radius, None),
            ActorMessageWalk::Team { radius, team } => (radius, Some(team)),
        };
        let captured = position(self, source)?;
        let limit = radius.wrapping_mul(radius);
        let (mut index, mut sum) = (0, 0_u32);
        while let Some(&actor) = self.actor_list().get(index) {
            index += 1;
            if self.construction_is_stopped(actor)? || self.disposal_is_stopped(actor)? {
                continue;
            }
            if team.is_some_and(|team| self.actor(actor).map(|state| state.word_b4) != Ok(team)) {
                continue;
            }
            let squared = if radius == 0 && team.is_some() {
                0
            } else {
                let centre = if team.is_some() {
                    captured
                } else {
                    position(self, source)?
                };
                let other = position(self, actor)?;
                let squared = (0..3).fold(0_i32, |squares, axis| {
                    let delta = i32::from(centre[axis].wrapping_sub(other[axis]) as i16);
                    squares.wrapping_add(delta.wrapping_mul(delta))
                }) as u32;
                if squared > limit {
                    continue;
                }
                squared
            };
            sum = sum.wrapping_add(deliver(self, actor, squared)?);
            index = self
                .actor_list()
                .iter()
                .position(|&current| current == actor)
                .ok_or(ObjectError::LeftActorList(actor))?
                + 1;
        }
        Ok(sum)
    }

    /// 0047DAF0's zero-radius arm (0047DBA4): every actor on G+48 gets
    /// `deliver(arena, receiver)` for its +98(sender, 0, code, argument),
    /// whatever the sender (the timed spawner's event has none), and the
    /// full EAX results add up. The next node is read after each delivery;
    /// stopped construction or disposal withholds the callback.
    pub fn walk_every_message_receiver<E: From<ObjectError>>(
        &mut self,
        mut deliver: impl FnMut(&mut ObjectArena, ObjectId) -> Result<u32, E>,
    ) -> Result<u32, E> {
        let (mut index, mut sum) = (0, 0_u32);
        while let Some(&actor) = self.actor_list().get(index) {
            index += 1;
            if self.construction_is_stopped(actor)? || self.disposal_is_stopped(actor)? {
                continue;
            }
            sum = sum.wrapping_add(deliver(self, actor)?);
            index = self
                .actor_list()
                .iter()
                .position(|&current| current == actor)
                .ok_or(ObjectError::LeftActorList(actor))?
                + 1;
        }
        Ok(sum)
    }
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
