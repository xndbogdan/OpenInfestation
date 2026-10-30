//! Captured-object packed values, 004789B0 and the admitted virtual +14 bodies.
//!
//! Selector zero reads the current animation clock before touching the packet
//! source. Nonzero selectors retain the actual source identity through model
//! children and dispatch its live typed class. Arena and executable loans admit
//! the nonmutating getters, while the Cell-backed process clock is read fresh
//! at each zero selector. Arbitrary native alias writes and unknown backing do
//! not acquire fabricated values. model_custom_vertex owns the separate
//! packet +4/source+10 phase; object_draw owns common and 004ED610 traversal.
//! Admitted getters ignore the forwarded model-invocation context argument;
//! the query therefore borrows only the owners they actually consume.

use inf_formats::{native_basis::isqrt_48efc0, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::{
    ActorExtension, ActorState, DerivedActorClass, DerivedActorState, Family4236d0Class,
    Family428650Class, Family429e20Class, ObjectArena, ObjectClass, ObjectError, ObjectId,
    Selector2State, Selector30State, Selector39State,
};
use crate::frontend_camera::AnimationClockSource;

/// Actual packet +3C source, current clock, and authenticated executable data.
/// H8 child model identity never replaces `source`.
#[derive(Clone, Copy)]
pub struct ObjectModelCallbacks<'a> {
    pub arena: &'a ObjectArena,
    pub source: ObjectId,
    pub animation: &'a dyn AnimationClockSource,
    pub executable: FrontendExecutable<'a>,
}

impl std::fmt::Debug for ObjectModelCallbacks<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Debug inspection must not trigger an extra live clock read.
        formatter
            .debug_struct("ObjectModelCallbacks")
            .field("source", &self.source)
            .field("animation", &"current clock source")
            .field("executable", &self.executable)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ObjectModelCallbackError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("object {object:?} model callback requires current field +{offset:X}")]
    RequiredField { object: ObjectId, offset: u32 },
    #[error("object {object:?} model callback requires its current behavior +140")]
    RequiredVehicleBehavior { object: ObjectId },
    #[error("object {object:?} model callback behavior row {row} is outside {rows} rows")]
    VehicleBehaviorRow {
        object: ObjectId,
        row: i8,
        rows: usize,
    },
    #[error("object {object:?} selector36 model callback {selector} requires preceding backing")]
    Selector36Backing { object: ObjectId, selector: i32 },
}

/// Weapon model 3, the guided missiles' (primary weapon 5 in `00530590`).
const GUIDED_MISSILE_MODEL: u16 = 3;

/// Port fix `guided_mounts`: the vehicle model's value 7, its weapon's
/// deployment `+210`, through a mode change between the ground forms (1, 2)
/// and the others (3 to 5) with the guided missiles shown. The ground forms
/// carry them in a roof launcher (buggy H8 118's child 154), the others in
/// side launchers (children 145/146), each drawn as far out as value 7
/// says. Retail swaps one for the other at the switch, fully out. Here the
/// value never exceeds the morph's progress `+1EC`: the old launcher folds
/// away as the old form morphs out (`FFFF` down to 0), and the new ones come
/// out as the new form morphs in (0 up to `FFFF`). `+210` itself, which
/// gates the firing, is untouched.
pub fn guided_mount_deploy(state: &Selector2State) -> u16 {
    let deploy = state.dwords_210_218[0] as u16;
    if state.word_20e != GUIDED_MISSILE_MODEL {
        return deploy;
    }
    let [current, progress, requested, previous] = state.dwords_1e8_1f4;
    let ground = |mode: u32| matches!(mode, 1 | 2);
    let crossing = if requested != current {
        // Morphing out toward the requested form.
        current != 0 && ground(current) != ground(requested)
    } else {
        // Morphing in from the previous form.
        progress != 0xffff && previous != 0 && ground(previous) != ground(current)
    };
    if crossing {
        deploy.min(progress as u16)
    } else {
        deploy
    }
}

impl ObjectModelCallbacks<'_> {
    /// Raw DWORD selector supplied by 004ED4B0's packed decoder. Do not mask or
    /// truncate it again: native comparisons precede the eventual AX return.
    pub fn packed_value(&self, selector: u32) -> Result<u16, ObjectModelCallbackError> {
        if selector == 0 {
            return Ok(self.animation.animation_value() as u16);
        }
        match &self.arena.object(self.source)?.class {
            ObjectClass::Actor(actor) => self.actor_value(actor, selector),
            ObjectClass::TimedPickup(pickup) => {
                if selector == 1 {
                    pickup
                        .kind_44
                        .ok_or(ObjectModelCallbackError::RequiredField {
                            object: self.source,
                            offset: 0x44,
                        })
                } else {
                    Ok(selector as u16)
                }
            }
            // 0046E050, shared with the timed pickup, over a known DWORD.
            ObjectClass::Selector22(state) => Ok(if selector == 1 {
                state.fade_44 as u16
            } else {
                selector as u16
            }),
            ObjectClass::Selector65(state) => Ok(if selector == 1 {
                state.fade_44 as u16
            } else {
                selector as u16
            }),
            ObjectClass::Selector58(state) => Ok(state.model_value(selector)),
            ObjectClass::Selector64(gate) => Ok(gate.model_value()),
            // 00475EB0, the territory's +14: its charge +88.
            ObjectClass::Selector46(territory) => Ok(territory.charge_88 as u16),
            // 004324E0, the flame's +14: query 1 is how much of the flame is
            // left to grow, 0x10000 less +70; any other DWORD comes back as
            // asked.
            ObjectClass::Selector76(state) => Ok(if selector == 1 {
                0x1_0000_u32.wrapping_sub(state.dword_70) as u16
            } else {
                selector as u16
            }),
            // Every other represented nonactor final table inherits 0047A020.
            _ => Ok(0),
        }
    }

    fn actor_value(
        &self,
        actor: &ActorState,
        selector: u32,
    ) -> Result<u16, ObjectModelCallbackError> {
        match &actor.extension {
            ActorExtension::Derived(derived) => self.derived_value(actor, derived, selector),
            ActorExtension::Selector15(state) => super::actor::selector15_value(state, selector)
                .ok_or(ObjectModelCallbackError::RequiredField {
                    object: self.source,
                    offset: 0x172_u32.wrapping_add(selector.wrapping_mul(2)),
                }),
            ActorExtension::Selector30(state) => Ok(selector30_value(state, selector)),
            ActorExtension::Selector39(state) => Ok(selector39_value(actor, state, selector)),
            ActorExtension::BehaviorResource(state) => Ok(state.model_value_14(selector)),
            // 0047FAA0: the Siren's WORD at +158 plus twice the selector.
            ActorExtension::Selector47(state) => {
                state
                    .model_value(selector)
                    .ok_or(ObjectModelCallbackError::RequiredField {
                        object: self.source,
                        offset: 0x158_u32.wrapping_add(selector.wrapping_mul(2)),
                    })
            }
            // 00454720 over the mining rig's level.
            ActorExtension::Selector52(state) => {
                Ok(super::actor::selector52_value(state, selector))
            }
            // 0043C200 over the capture clock.
            ActorExtension::Selector84(state) => Ok(state.model_value(selector)),
            // The brain's 00406630; the attack keeps the shared 0047A020.
            ActorExtension::Selector62(state) => match &state.brain {
                Some(brain) => {
                    brain
                        .model_value(selector)
                        .ok_or(ObjectModelCallbackError::RequiredField {
                            object: self.source,
                            offset: 0x18e_u32.wrapping_add(selector.wrapping_mul(2)),
                        })
                }
                None => Ok(0),
            },
            ActorExtension::Selector17(state) => {
                Ok(super::actor::selector17_value(state, selector))
            }
            // 00473D50, the Remote Beamer's: query 1 is its charge; any
            // other DWORD comes back as asked.
            ActorExtension::Selector55(state) => Ok(if selector == 1 {
                state.charge_138
            } else {
                selector as u16
            }),
            // 004045C0, the ball's: query 1 is 0xFFFF but while the ball is
            // held ahead (state 3); any other is 0.
            ActorExtension::Selector59(state) if state.ball_148.is_some() => {
                Ok(match state.ball_148.as_deref() {
                    Some(ball) if selector == 1 && ball.state_148 != 3 => 0xffff,
                    _ => 0,
                })
            }
            // 0051FF54, 005262CC and the receiver's 00526530 keep the shared
            // +14 = 0047A020.
            ActorExtension::Selector56
            | ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_) => Ok(0),
        }
    }

    fn derived_value(
        &self,
        actor: &ActorState,
        derived: &DerivedActorState,
        selector: u32,
    ) -> Result<u16, ObjectModelCallbackError> {
        match &derived.class {
            DerivedActorClass::Base => Ok(derived_value(derived, selector)),
            DerivedActorClass::Selector11(state) => Ok(match selector {
                2 => {
                    let maximum =
                        actor
                            .maximum_health
                            .ok_or(ObjectModelCallbackError::RequiredField {
                                object: self.source,
                                offset: 0xa2,
                            })?;
                    let numerator = u32::from(maximum)
                        .wrapping_sub(u32::from(actor.health))
                        .wrapping_shl(16) as i32;
                    (numerator / (i32::from(maximum) + 1)) as u16
                }
                3 => state.word_188 as u16,
                _ => state.word_18c as u16,
            }),
            DerivedActorClass::Selector2(state) => self.vehicle_value(derived, state, selector),
            DerivedActorClass::Selector60(state) => Ok(match selector {
                6 => state.word_1a0,
                7 => state.word_19e,
                _ => 0,
            }),
            DerivedActorClass::Selector77(state) => Ok(if selector < 7 {
                let value = i32::from(state.words_184_190[selector as usize] as i16);
                if matches!(selector, 1 | 5) {
                    value.wrapping_neg() as u16
                } else {
                    value as u16
                }
            } else {
                0
            }),
            DerivedActorClass::Family428650(family) => match &family.class {
                Family428650Class::Base
                | Family428650Class::Selector34(_)
                | Family428650Class::Selector35(_) => Ok(derived_value(derived, selector)),
                Family428650Class::Family429e20(family) => match &family.class {
                    Family429e20Class::Base => Ok(derived_value(derived, selector)),
                    Family429e20Class::Selector12(state) => {
                        // 00468920 reads WORD90,92,8E and computes the wrapped
                        // speed even for selectors whose result does not use it.
                        let [x, y, z] = actor.words_8e_92.map(|word| i32::from(word as i16));
                        let sum =
                            y.wrapping_mul(y)
                                .wrapping_add(z.wrapping_mul(z))
                                .wrapping_add(x.wrapping_mul(x)) as u32;
                        let speed = if sum & 0x8000_0000 != 0 {
                            isqrt_48efc0((sum >> 2) as i32).wrapping_shl(1)
                        } else {
                            isqrt_48efc0(sum as i32)
                        };
                        Ok(match selector {
                            1 => speed as u16,
                            2 if speed as i32 > 100 => 0xc000,
                            2 if speed as i32 > 5 => 0x8000,
                            2 if state.byte_29c != 0 => 0x4000,
                            9 if state.byte_29d != 0 => {
                                u16::from(self.arena.world_load_state().flag_54ece4)
                            }
                            _ => 0,
                        })
                    }
                    Family429e20Class::Family4236d0(family) => match &family.class {
                        // 00405700: selector 21's swings, then 004260A0.
                        Family4236d0Class::Selector10(state) => {
                            Ok(super::actor::selector21_value(state, selector)
                                .unwrap_or_else(|| derived_value(derived, selector)))
                        }
                        Family4236d0Class::Base => Ok(derived_value(derived, selector)),
                        Family4236d0Class::Selector36(state) => {
                            if selector as i32 <= 6 {
                                Ok(match selector {
                                    1 => state.word_268 as u16,
                                    2 => (state.word_268 >> 16) as u16,
                                    3 => state.word_26c as u16,
                                    4 => (state.word_26c >> 16) as u16,
                                    5 => state.word_270 as u16,
                                    6 => (state.word_270 >> 16) as u16,
                                    _ => {
                                        return Err(ObjectModelCallbackError::Selector36Backing {
                                            object: self.source,
                                            selector: selector as i32,
                                        })
                                    }
                                })
                            } else {
                                Ok(derived_value(derived, selector))
                            }
                        }
                    },
                },
            },
        }
    }

    fn vehicle_value(
        &self,
        derived: &DerivedActorState,
        state: &Selector2State,
        selector: u32,
    ) -> Result<u16, ObjectModelCallbackError> {
        Ok(match selector {
            1 => {
                let address = 0x0053_0578_u32.wrapping_add(state.dwords_1e8_1f4[0].wrapping_shl(1));
                let bytes = self.executable.mapped(address, 2)?.bytes;
                u16::from_le_bytes([bytes[0], bytes[1]])
            }
            2 => state.dwords_1e8_1f4[1] as u16,
            // 00415BF3: the track phase's low WORD.
            3 => state
                .drive
                .dword_1fc
                .ok_or(ObjectModelCallbackError::RequiredField {
                    object: self.source,
                    offset: 0x1fc,
                })? as u16,
            4 => {
                let mode = state.dwords_1e8_1f4[0];
                if matches!(mode, 1 | 2) {
                    let value = state.dword_244 as i32;
                    let factor = if mode == 1 { 47 } else { 28 };
                    if value > 0 {
                        isqrt_48efc0(value).wrapping_mul(factor) as u16
                    } else {
                        isqrt_48efc0(value.wrapping_neg())
                            .wrapping_mul(factor)
                            .wrapping_neg() as u16
                    }
                } else {
                    ((state.dwords_220_224[0] as i32) / 10) as u16
                }
            }
            6 => state.word_20e,
            7 => state.dwords_210_218[0] as u16,
            8 => match state.dword_238 {
                1 => 0x1000,
                0 => 0x8001,
                _ => 0,
            },
            10 if state.dwords_220_224[1] != 0 && state.bytes_20a_20c[2] == 0 => 0xffff,
            11 | 12 => {
                let behavior = derived.behavior_140.as_ref().ok_or(
                    ObjectModelCallbackError::RequiredVehicleBehavior {
                        object: self.source,
                    },
                )?;
                let row = behavior.byte_10 as i8;
                let record = usize::try_from(row)
                    .ok()
                    .and_then(|row| behavior.entries_18.get(row))
                    .ok_or(ObjectModelCallbackError::VehicleBehaviorRow {
                        object: self.source,
                        row,
                        rows: behavior.entries_18.len(),
                    })?;
                record[if selector == 11 { 1 } else { 2 }] as u16
            }
            _ => 0,
        })
    }
}

fn derived_value(state: &DerivedActorState, selector: u32) -> u16 {
    if selector == 9 {
        state.word_178
    } else {
        selector as u16
    }
}

fn selector30_value(state: &Selector30State, selector: u32) -> u16 {
    if state.word_164 != 0 {
        return 0;
    }
    match selector {
        1 if state.word_168 == 1 => {
            let numerator = 1200_i32
                .wrapping_sub(i32::from(state.word_17c))
                .wrapping_mul(65535);
            (numerator / 1200) as u16
        }
        2 => 1,
        _ => 0,
    }
}

/// 0043BF10, selector 39's +14. Selector 1 reports a lifecycle other than
/// 3. Selector 2 animates the +150 destruction timer against +160 seconds:
/// unsigned millisecond tests, then the signed magic divisions by 250 and
/// 200. A timer of 200 ms or more inside the window wraps to 0xC000.
fn selector39_value(actor: &ActorState, state: &Selector39State, selector: u32) -> u16 {
    match selector {
        1 => u16::from(actor.word_c4 != 3),
        2 => {
            let timer = state.intermediate.word_150;
            let window = u32::from(state.byte_160) * 1000;
            let elapsed = timer / 1000;
            if elapsed > window.wrapping_sub(250) {
                let rise = elapsed.wrapping_sub(window).wrapping_add(250);
                ((rise.wrapping_mul(16383) as i32 / 250) as u32).wrapping_add(0xc000) as u16
            } else if timer < 200_000 {
                0xffff_u32.wrapping_sub((elapsed.wrapping_mul(16383) as i32 / 200) as u32) as u16
            } else {
                0xc000
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests;
