//! `00421DC0`, the free look, which only the ground drive `004195F0` calls.
//! V (key callback `00469750`) and the computer's Image enhancement mode
//! set control `+3F`, which toggles it on; any other control toggles it off
//! again. While it is on, the turn and pitch controls swing the view round
//! the standing vehicle (`+1C2..+1C8`, read by the special view `00421D50`)
//! and the controls are spent. The zoom `G+5C0` starts at `0x3FFF`; the zoom
//! key (`+40`, `00469760`) held with pitch steps it, and a weapon selection
//! sets it to either end.

use std::cell::Cell;

use super::drive::VehicleDriveError;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId, PlayerControlBlock};

/// `00546A08`: a saved row whose dot product with the vehicle's falls below
/// this is taken again.
const ROW_KEPT: f64 = 0.984375;

impl ObjectArena {
    pub(super) fn vehicle_free_look<E>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        delta: u32,
        zoom: &Cell<u16>,
    ) -> Result<(), VehicleDriveError<E>> {
        // Virtual +48 is the rotating basis: up words 4..6, forward 7..9.
        let basis = self.actor(vehicle)?.rotating.basis;
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let unknown = |offset| VehicleDriveError::UnknownField {
            vehicle,
            offset,
            address: 0x0042_1dc0,
        };
        let state = self.selector2_actor_mut(vehicle)?;
        if controls.thrust_1c != 0
            || controls.byte(0x3a) != 0
            || controls.fire_20 != 0
            || controls.grenade_24 != 0
            || controls.dword_30 != 0
            || controls.dword_34 != 0
            || (0x3b..=0x3f).any(|offset| controls.byte(offset) != 0)
            || controls.byte(0x38) != 0
        {
            state.words_1c2_1c8 = [0; 4];
            state.special_forward_1d0 = [7, 8, 9].map(|word| Some(basis.words[word]));
            state.special_up_1dc = [4, 5, 6].map(|word| Some(basis.words[word]));
            state.byte_1c0 = u8::from(state.byte_1c0 == 0);
            if state.byte_1c0 != 0 {
                zoom.set(0x3fff);
            }
        }
        if controls.byte(0x40) != 0 {
            let current = i32::from(zoom.get() as i16);
            if controls.pitch_18 < 0 {
                zoom.set((current + 0x1000).min(0x7fff) as u16);
            } else if controls.pitch_18 > 0 {
                zoom.set((current - 0x1000).max(0) as u16);
            }
        }
        if controls.dword_28 != 0 {
            zoom.set(0x7fff);
        } else if controls.dword_2c != 0 {
            zoom.set(0);
        }
        if state.byte_1c0 != 0 {
            let eighth = delta >> 3;
            let step = (delta >> 5) as i32;
            let least = ((eighth << 13) as i32 >> 15) as i16;
            // 00421F45..00421FCC: the eye follows a vehicle that moved.
            let mut saved = [0u16; 3];
            for (axis, word) in saved.iter_mut().enumerate() {
                *word = state.special_position_1ca[axis].ok_or(unknown(0x1ca + axis as u16 * 2))?;
            }
            let distance = (0..3)
                .map(|axis| i32::from(position[axis].wrapping_sub(saved[axis]) as i16))
                .fold(0i32, |sum, d| sum.wrapping_add(d.wrapping_mul(d)));
            if distance as u32 > 0x18 {
                state.special_position_1ca = position.map(Some);
            }
            // 00421FD3..00422081: the saved rows follow a vehicle that turned.
            let row =
                |saved: &[Option<u32>; 3], offset: u16| -> Result<[f64; 3], VehicleDriveError<E>> {
                    let mut values = [0.0; 3];
                    for (axis, value) in values.iter_mut().enumerate() {
                        let word = saved[axis].ok_or(unknown(offset + axis as u16 * 4))?;
                        *value = f64::from(f32::from_bits(word));
                    }
                    Ok(values)
                };
            let current = |first: usize| {
                [0, 1, 2].map(|axis| f64::from(f32::from_bits(basis.words[first + axis])))
            };
            let kept = |saved: [f64; 3], current: [f64; 3]| {
                (saved[2] * current[2] + saved[1] * current[1]) + saved[0] * current[0] >= ROW_KEPT
            };
            if !kept(row(&state.special_forward_1d0, 0x1d0)?, current(7)) {
                state.special_forward_1d0 = [7, 8, 9].map(|word| Some(basis.words[word]));
            }
            if !kept(row(&state.special_up_1dc, 0x1dc)?, current(4)) {
                state.special_up_1dc = [4, 5, 6].map(|word| Some(basis.words[word]));
            }
            let [yaw_rate, pitch_rate, yaw, pitch] = &mut state.words_1c2_1c8;
            // 00422087..00422141: turn swings the yaw rate +1C2, else it decays.
            if controls.turn_14 != 0 {
                let push = (controls.turn_14 >> 5).wrapping_neg();
                let push = if (*yaw_rate as i16 ^ push as i16) < 0 {
                    push << 1
                } else {
                    push
                };
                *yaw_rate = clamp(yaw_rate.wrapping_add(push as u16), 0x4000);
            } else {
                *yaw_rate = decay(*yaw_rate, eighth, least);
            }
            if *yaw_rate != 0 {
                *yaw = yaw
                    .wrapping_add(((i32::from(*yaw_rate as i16).wrapping_mul(step)) >> 15) as u16);
            }
            // 00422147..00422200: pitch swings +1C4 unless the zoom key holds.
            if controls.pitch_18 != 0 && controls.byte(0x40) == 0 {
                let pull = controls.pitch_18.wrapping_neg() >> 5;
                let pull = if (pull as i16 ^ *pitch_rate as i16) < 0 {
                    pull << 1
                } else {
                    pull
                };
                *pitch_rate = clamp(pitch_rate.wrapping_sub(pull as u16), 0x3000);
            } else {
                *pitch_rate = decay(*pitch_rate, eighth, least);
            }
            if *pitch_rate != 0 {
                *pitch = pitch.wrapping_add(
                    ((i32::from(*pitch_rate as i16).wrapping_mul(step)) >> 15) as u16,
                );
            }
            // 00422200..00422293: the analog axes beyond 0x3000 turn the view.
            if controls.analog_04 != 0 {
                if let Some(turn) = analog_turn(controls.axes_08[0], step) {
                    *yaw = if controls.axes_08[0] > 0 {
                        yaw.wrapping_add(turn)
                    } else {
                        yaw.wrapping_sub(turn)
                    };
                }
                if let Some(turn) = analog_turn(controls.axes_08[1], step) {
                    *pitch = if controls.axes_08[1] > 0 {
                        pitch.wrapping_sub(turn)
                    } else {
                        pitch.wrapping_add(turn)
                    };
                }
            }
            // 00422293..004222BA: the pitch stops at 0x1000 up and 0x4000 down.
            if *pitch as i16 > 0x1000 {
                *pitch = 0x1000;
                *pitch_rate = 0;
            } else if (*pitch as i16) < -0x4000 {
                *pitch = 0xc000;
                *pitch_rate = 0;
            }
        }
        // 004222C7..004222D9: 004959E0 zeroes the block; +40 survives and
        // +3E is set.
        let zoom_key = controls.byte(0x40);
        *controls = PlayerControlBlock::default();
        controls.set_byte(0x40, zoom_key);
        controls.set_byte(0x3e, 1);
        Ok(())
    }
}

/// A rate past `limit` either way stops there.
fn clamp(rate: u16, limit: i16) -> u16 {
    (rate as i16).clamp(-limit, limit) as u16
}

/// A rate with no control falls toward zero by its share of the tick, at
/// least `least` and never past zero.
fn decay(rate: u16, eighth: u32, least: i16) -> u16 {
    let rate = rate as i16;
    let mut fall = ((i32::from(rate).wrapping_mul(eighth as i32)) >> 15) as i16;
    if fall < 0 {
        if i32::from(fall) > -i32::from(least) {
            fall = least.wrapping_neg();
        }
        if fall < rate {
            fall = rate;
        }
    } else {
        if fall < least {
            fall = least;
        }
        if fall > rate {
            fall = rate;
        }
    }
    rate.wrapping_sub(fall) as u16
}

/// An analog axis past its 0x3000 dead zone turns the view by a third of
/// the rest, over the tick.
fn analog_turn(axis: i32, step: i32) -> Option<u16> {
    let magnitude = axis.wrapping_abs();
    (magnitude > 0x3000).then(|| ((((magnitude - 0x3000) / 3).wrapping_mul(step)) >> 15) as u16)
}

#[cfg(test)]
#[path = "free_look_tests.rs"]
mod tests;
