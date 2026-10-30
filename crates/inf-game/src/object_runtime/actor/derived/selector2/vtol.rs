//! Selector 2's VTOL, vehicle mode 4: the entry and exit 00418430 the mode
//! switch calls, and the drive 00418650 with its flight physics 00418AB0.
//! The VTOL steers itself rather than through the suspension: the keys and
//! the analog stick feed a pitch rate (+258) and a yaw rate (+25C), each
//! easing in through an accumulator (+250, +254); the nose levels itself
//! once the pitch keys rest (+24C) and the hull banks into its yaw. Thrust
//! lifts along the hull's up axis, the ground and water cushion it, the
//! lift fades between 1500 and 2500 units above them, and the air drags in
//! the hull's own frame. Infest03 reaches the drive on 107 updates, on
//! worlds 11 and 23.

use inf_formats::native_basis::{inverse_sine_q31, NativeBasisMath, RotationPlane};
use inf_formats::FrontendExecutable;

use super::super::kinematics::{divide, ftol};
use super::super::physics::ActorPhysicsRequest;
use super::drive::{VehicleDriveError, VehicleDriveHost, VehicleDriveRequest};
use super::update::VehicleUpdateHost;
use crate::object_runtime::actor_terrain::plane_height_at;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId, PlayerControlBlock};

/// `00418475`/`00418496`: the one-shots for taking off and landing.
const ENTER_SOUND: u32 = 0x11;
const EXIT_SOUND: u32 = 0x12;
/// 00520A24: 2^30, the forward row's Y as a Q30 sine.
const Q30: f64 = 1_073_741_824.0;
/// 00520A20: 65536.0, the X row's Y in WORD angle units.
const ROLL_SCALE: f64 = 65_536.0;
/// 0053FF00: the inverse sine table 0048F520 reads.
const INVERSE_SINE: u32 = 0x0053_ff00;

impl ObjectArena {
    /// 00418430(enter): entering clears the flight state (+24E at its
    /// 0x7FFF rest, +24C..+25C) and the traction, then plays the take-off;
    /// leaving only plays the landing.
    pub fn switch_vehicle_vtol<H: VehicleDriveHost>(
        &mut self,
        vehicle: ObjectId,
        enter: bool,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        if enter {
            let state = self.selector2_actor_mut(vehicle)?;
            state.words_24c_24e = [0, 0x7fff];
            state.dwords_250_25c = [0; 4];
            state.drive.dword_57c = Some(0);
        }
        host.play_sound(
            self,
            vehicle,
            if enter { ENTER_SOUND } else { EXIT_SOUND },
            0x4000,
            0x10000,
        )
        .map_err(VehicleDriveError::Sound)
    }

    /// 00418650 for an offline VTOL. `controls` is the driving player's
    /// block, which the drive only reads.
    pub fn tick_vehicle_vtol<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &PlayerControlBlock,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let delta = request.delta;
        let step = delta >> 5;
        let math = NativeBasisMath::new(*request.ground.sine_quarter);
        let turn_key = controls.turn_14;
        let pitch_key = controls.pitch_18;
        if turn_key != 0 || pitch_key != 0 {
            self.selector2_actor_mut(vehicle)?.words_24c_24e[1] = 0x7fff;
        }
        // 00418685..004186D8: the nose angle, the inverse sine 0048F520 of
        // the forward row's Y, mirrored while the up row's Y is below zero.
        let basis = self.actor(vehicle)?.rotating.basis;
        let m = |index: usize| f64::from(f32::from_bits(basis.words[index]));
        let sine = ftol(m(8) * Q30).wrapping_shl(1);
        let angle =
            inverse_sine_q31(sine, &inverse_sine_table(request.executable)?).map_err(|_| {
                VehicleDriveError::Unported {
                    vehicle,
                    address: 0x0048_f529,
                }
            })?;
        let upright = matches!(
            m(5).partial_cmp(&0.0),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        );
        let nose = if upright {
            angle
        } else if angle > 0 {
            0x8000 - angle
        } else {
            -0x8000 - angle
        };

        // 004186DC..00418812: the analog stick turns past a 6000 dead zone,
        // and while it is in use the nose eases toward its pitch.
        let mut turn = turn_key;
        if controls.analog_04 != 0 {
            let mut correction = 0;
            let mut rested = false;
            if turn == 0 {
                let x = controls.axes_08[0];
                let x = if x > 6000 {
                    x - 6000
                } else if x < -6000 {
                    x + 6000
                } else {
                    0
                };
                if x != 0 {
                    turn = divide_turn(x.wrapping_shl(15)).wrapping_mul(step as i32) >> 11;
                    rested = true;
                }
            }
            if pitch_key == 0 {
                let y = controls.axes_08[1];
                if y.wrapping_abs() > 6000 {
                    let y = if y > 0 { y - 6000 } else { y + 6000 };
                    correction = divide_pitch(y.wrapping_shl(15)).wrapping_mul(-13653) >> 16;
                    rested = true;
                }
            }
            let state = self.selector2_actor_mut(vehicle)?;
            if rested {
                state.words_24c_24e[1] = 0;
            }
            let timer = state.words_24c_24e[1];
            if (timer as i16) < 0x9c4 {
                state.words_24c_24e[1] = timer.wrapping_add((delta >> 10) as u16);
                let angle = nose.wrapping_sub(correction).wrapping_mul(step as i32) >> 13;
                self.rotate_visual(vehicle, RotationPlane::Yz, angle, &math)?;
            }
        }

        // 00418817..0041897C: the rotor phase, then the pitch rate +258.
        let state = self.selector2_actor_mut(vehicle)?;
        let phase = state
            .drive
            .dword_1fc
            .ok_or(VehicleDriveError::UnknownField {
                vehicle,
                offset: 0x1fc,
                address: 0x0041_8817,
            })?;
        state.drive.dword_1fc = Some(phase.wrapping_add(delta >> 3));
        let mut pitch = pitch_key;
        let mut held = true;
        if pitch_key == 0 {
            let rest = state.words_24c_24e[0];
            if (rest as i16) < 0xfa {
                state.words_24c_24e[0] = rest.wrapping_add((delta >> 10) as u16);
                held = false;
            } else {
                // Rested: a nose more than 9 degrees off level is pitched
                // back as if held.
                state.dwords_250_25c[0] = 0;
                if nose > 0x666 {
                    pitch = (delta as i32).wrapping_neg();
                } else if nose < -0x666 {
                    pitch = delta as i32;
                } else {
                    held = false;
                }
            }
        } else {
            state.words_24c_24e[0] = 0;
        }
        if held && pitch != 0 {
            let target: i32 = if pitch > 0 { -0x8000 } else { 0x8000 };
            let accumulated = state.dwords_250_25c[0] as i32;
            state.dwords_250_25c[2] = target as u32;
            if accumulated < 0x2ab9 {
                let accumulated = (pitch.wrapping_abs() >> 5).wrapping_add(accumulated);
                state.dwords_250_25c[0] = accumulated as u32;
                if accumulated < 0x2ab9 {
                    state.dwords_250_25c[2] = divide_2ab9(accumulated.wrapping_mul(target)) as u32;
                }
            }
        } else {
            let rate = state.dwords_250_25c[2] as i32;
            state.dwords_250_25c[0] = 0;
            state.dwords_250_25c[2] =
                rate.wrapping_sub(rate.wrapping_mul(step as i32) >> 13) as u32;
        }
        // 0041886F..00418976: past 30 degrees the rate yields to leveling.
        let rate = state.dwords_250_25c[2] as i32;
        if rate > 0 {
            if nose < -0x1555 {
                state.dwords_250_25c[2] = divide_8192(nose.wrapping_add(0x1555).wrapping_mul(rate))
                    .wrapping_add(rate) as u32;
            }
        } else if nose > 0x1555 {
            let next = rate.wrapping_sub(divide_8192(nose.wrapping_sub(0x1555).wrapping_mul(rate)));
            state.dwords_250_25c[2] = if next > 0 { next - 1 } else { next } as u32;
        }

        // 0041897C..00418A1B: turn about world Y at +25C, then the yaw rate.
        let yaw_rate = state.dwords_250_25c[3] as i32;
        let angle = (yaw_rate.wrapping_mul(step as i32) >> 15) as i16 as i32;
        self.rotate_visual(vehicle, RotationPlane::WorldY, angle, &math)?;
        let state = self.selector2_actor_mut(vehicle)?;
        if turn != 0 {
            let rate = divide(turn.wrapping_mul(682), step as i32, 0x0041_89b2)?;
            let accumulated = state.dwords_250_25c[1] as i32;
            state.dwords_250_25c[3] = rate as u32;
            if accumulated < 0x186a {
                let accumulated = (turn.wrapping_abs() >> 5).wrapping_add(accumulated);
                state.dwords_250_25c[1] = accumulated as u32;
                if accumulated < 0x186a {
                    state.dwords_250_25c[3] = divide_186a(accumulated.wrapping_mul(rate)) as u32;
                }
            }
        } else {
            let rate = state.dwords_250_25c[3] as i32;
            state.dwords_250_25c[1] = 0;
            state.dwords_250_25c[3] =
                rate.wrapping_sub(rate.wrapping_mul(step as i32) >> 12) as u32;
        }

        // 00418A1B..00418A7E: pitch at +258, then bank into the yaw while
        // leveling the roll (the X row's Y).
        let pitch_rate = state.dwords_250_25c[2] as i32;
        let yaw_rate = state.dwords_250_25c[3] as i32;
        let angle = (pitch_rate.wrapping_mul(step as i32) >> 15) as i16 as i32;
        self.rotate_visual(vehicle, RotationPlane::Yz, angle, &math)?;
        let tilt = f64::from(f32::from_bits(self.actor(vehicle)?.rotating.basis.words[2]));
        let bank = yaw_rate.wrapping_mul(6) >> 2;
        let lean = divide_16384(nose.wrapping_abs().wrapping_mul(bank));
        let roll = lean
            .wrapping_neg()
            .wrapping_sub(ftol(tilt * ROLL_SCALE))
            .wrapping_add(bank);
        let angle = (roll.wrapping_mul(step as i32) >> 15) as i16 as i32;
        self.rotate_visual(vehicle, RotationPlane::Xy, angle, &math)?;

        self.vtol_flight(vehicle, controls, request, host)?;
        // 00418A99: 0047B1E0 with angular drag, the position and the
        // world-space spin.
        self.tick_actor_physics(
            vehicle,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: 0x1a,
                sine_quarter: request.ground.sine_quarter,
            },
        )?;
        Ok(())
    }

    /// 00418AB0(delta, controls): thrust lifts along the up row (a tow adds
    /// a third), the ground or water below cushions the hull, the lift
    /// fades with altitude, the spray, drag in the hull's frame, the spin
    /// damping and the engine load.
    fn vtol_flight<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &PlayerControlBlock,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let delta = request.delta;
        let step = (delta >> 5) as i32;
        let mut lift = divide_600(controls.thrust_1c);
        let mut fine = step >> 5;
        let towed = {
            let bytes = self.selector2_actor(vehicle)?.bytes_1b4_1bf;
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        };
        if towed != request.process_null.raw() {
            fine = divide_3(fine.wrapping_shl(2));
            lift = divide_3(lift.wrapping_shl(2));
        }
        // 00418B19..00418BDF: within 384 units of the ground or the water,
        // the hull is pushed up, twice as hard while falling.
        let surface = self.vtol_surface(vehicle, request)?;
        let position = self.vtol_position(vehicle)?;
        let gap = i32::from(position[1] as i16) - i32::from(surface) + 0x80;
        if gap < 0x200 {
            let velocity_y = self.actor(vehicle)?.words_8e_92[1] as i16;
            let push = (0x200 - gap).wrapping_mul(fine);
            let push = if velocity_y < 0 {
                push.wrapping_shl(2)
            } else {
                push.wrapping_shl(1)
            };
            self.actor_mut(vehicle)?.words_8e_92[1] =
                (velocity_y as u16).wrapping_add(divide_512(push) as u16);
        }
        // 00418BE6: 0047AD10(delta).
        {
            let velocity_y = &mut self.actor_mut(vehicle)?.words_8e_92[1];
            *velocity_y = velocity_y.wrapping_sub((delta >> 10) as u16);
        }
        if lift >= 0 {
            // 00418BF2..00418C18: a fall faster than 384 is braked.
            let velocity_y = self.actor(vehicle)?.words_8e_92[1] as i16;
            if velocity_y < -0x180 {
                let next = (fine as u16).wrapping_add(velocity_y as u16) as i16;
                self.actor_mut(vehicle)?.words_8e_92[1] =
                    if next > -0x180 { -0x180 } else { next } as u16;
            }
        }
        if lift != 0 {
            // 00418C20..00418CFB: lift along the up row, fading out between
            // 1500 and 2500 units above the surface.
            let basis = self.actor(vehicle)?.rotating.basis;
            let up = |index: usize| f64::from(f32::from_bits(basis.words[index]));
            let along = |lift: i32, index: usize| ftol(f64::from(lift) * up(index)) as u16;
            {
                let velocity = &mut self.actor_mut(vehicle)?.words_8e_92;
                velocity[0] = velocity[0].wrapping_add(along(lift, 4));
                velocity[2] = velocity[2].wrapping_add(along(lift, 6));
            }
            let surface = self.vtol_surface(vehicle, request)?;
            let height = i32::from(position[1] as i16) - i32::from(surface);
            if height > 0x5dc {
                lift = if height >= 0x9c4 {
                    0
                } else {
                    lift.wrapping_add(divide_neg_1000(
                        height.wrapping_sub(0x5dc).wrapping_mul(lift),
                    ))
                };
            }
            let velocity = &mut self.actor_mut(vehicle)?.words_8e_92;
            velocity[1] = velocity[1].wrapping_add(along(lift, 5));
        }
        // 00418D05..00418D27: the spray, at least 1000 strong.
        let strength = lift.wrapping_mul(40);
        host.hull_spray(self, vehicle, if strength > 1000 { strength } else { 1000 })
            .map_err(VehicleDriveError::Host)?;
        // 00418D2C..00418E89: drag in the hull's frame (0047A660), sideways
        // and backward at 1/8192 of the step, up at 1/65536 and forward at
        // 1/32768, with small speeds dropped whole; back to world (0047A6C0)
        // and taken off each axis without reversing it.
        let basis = self.actor(vehicle)?.rotating.basis;
        let velocity = self.actor(vehicle)?.words_8e_92;
        let local = basis.inverse_words(velocity.map(|word| word as i16));
        let [x, y, z] = local.map(i32::from);
        let drag = [
            limit_drag(x, x.wrapping_mul(step) >> 13, 0x10),
            limit_drag(y, y.wrapping_mul(step) >> 16, 4),
            if z > 0 {
                limit_drag(z, z.wrapping_mul(step) >> 15, 8)
            } else {
                limit_drag(z, z.wrapping_mul(step) >> 13, 0x10)
            },
        ];
        let world = basis.transform_words(drag.map(|value| value as i16));
        {
            let velocity = &mut self.actor_mut(vehicle)?.words_8e_92;
            for axis in 0..3 {
                let speed = velocity[axis] as i16;
                let drag = world[axis] as i16;
                velocity[axis] = if i32::from(speed).abs() > i32::from(drag).abs() {
                    (speed as u16).wrapping_sub(drag as u16)
                } else {
                    0
                };
            }
        }
        // 00418E89..00418EF9: each angular DWORD loses (w * delta << 11) >> 31.
        let factor = delta.wrapping_shl(11) as i32;
        let angular = self.actor(vehicle)?.words_94_9c;
        let loss = angular.map(|word| multiply_high31((word as i32).wrapping_neg(), factor) as u32);
        self.add_actor_angular_velocity(vehicle, loss)?;
        // 00418EFE..00418F47: the engine load rises while lifting, else falls
        // to a quarter.
        let state = self.selector2_actor_mut(vehicle)?;
        let unknown = |offset, address| VehicleDriveError::UnknownField {
            vehicle,
            offset,
            address,
        };
        let low = state.drive.dword_55c.ok_or(unknown(0x55c, 0x0041_8f0b))?;
        let high = state.drive.dword_560.ok_or(unknown(0x560, 0x0041_8f05))?;
        if lift != 0 {
            state.drive.dword_560 = Some(high.wrapping_add(100));
            state.drive.dword_55c = Some(low.wrapping_add(100));
        } else {
            state.drive.dword_55c = Some(((low as i32) >> 2) as u32);
            state.drive.dword_560 = Some(((high as i32) >> 2) as u32);
        }
        Ok(())
    }

    fn vtol_position(&self, vehicle: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))
    }

    /// The higher of the ground 00450A30 and the water 00410F60 under the
    /// hull. The water's waves follow H14 flag 40 but at clock 0: the call
    /// passes a zero clock.
    fn vtol_surface(
        &self,
        vehicle: ObjectId,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<i16, ObjectError> {
        let [x, _, z] = self.vtol_position(vehicle)?;
        let terrain = request.ground.terrain;
        let ground = terrain.height_at([x, z]);
        let water = plane_height_at(
            [x, z],
            terrain.plane_base() >> terrain.plane_shift(),
            ground,
            0,
            request.ground.world_flags & 0x40 != 0,
            request.ground.sine_quarter,
        );
        Ok(if ground > water { ground } else { water })
    }
}

/// The inverse sine table at 0053FF00.
fn inverse_sine_table<E>(
    executable: FrontendExecutable<'_>,
) -> Result<[i16; 1024], VehicleDriveError<E>> {
    let bytes = executable
        .mapped(INVERSE_SINE, 2048)
        .map_err(|error| VehicleDriveError::Executable {
            address: INVERSE_SINE,
            reason: error.to_string(),
        })?
        .bytes;
    Ok(std::array::from_fn(|index| {
        i16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])
    }))
}

/// A drag that removes small speeds whole and otherwise at most `scaled`.
fn limit_drag(speed: i32, scaled: i32, small: i32) -> i32 {
    if speed > small {
        speed.min(scaled)
    } else if speed < -small {
        speed.max(scaled)
    } else {
        speed
    }
}

/// Signed IMUL EDX:EAX, SHL EAX then RCL EDX.
fn multiply_high31(left: i32, right: i32) -> i32 {
    (i64::from(left) * i64::from(right) >> 31) as i32
}

/// The compiler's reciprocal divisions, each with its truncating sign
/// correction: the high DWORD of `value * magic`, an optional correction by
/// the value, an arithmetic shift, then plus one when negative.
fn reciprocal(value: i32, magic: u32, correction: i32, shift: u32) -> i32 {
    let high = ((i64::from(value) * i64::from(magic as i32)) >> 32) as i32;
    let quotient = high.wrapping_add(correction.wrapping_mul(value)) >> shift;
    quotient.wrapping_add((quotient as u32 >> 31) as i32)
}

/// 0041871D: by -29768.
fn divide_turn(value: i32) -> i32 {
    reciprocal(value, 0x7319_a973, -1, 14)
}

/// 0041876A/004187A5: by 29768.
fn divide_pitch(value: i32) -> i32 {
    reciprocal(value, 0x8ce6_568d, 1, 14)
}

/// 00418935: by 0x2AB9.
fn divide_2ab9(value: i32) -> i32 {
    reciprocal(value, 0x2fef_e569, 0, 11)
}

/// 004189E7: by 6250.
fn divide_186a(value: i32) -> i32 {
    reciprocal(value, 0x14f8_b589, 0, 9)
}

/// 00418AC5: by 600.
fn divide_600(value: i32) -> i32 {
    reciprocal(value, 0x1b4e_81b5, 0, 6)
}

/// 00418AFA: by 3.
fn divide_3(value: i32) -> i32 {
    reciprocal(value, 0x5555_5556, 0, 0)
}

/// 00418CD6: by -1000.
fn divide_neg_1000(value: i32) -> i32 {
    reciprocal(value, 0xef9d_b22d, 0, 6)
}

/// CDQ, AND EDX with 2^n-1, ADD, SAR n: division by 2^n toward zero.
fn divide_power(value: i32, bits: u32) -> i32 {
    let bias = ((value >> 31) as u32 & ((1 << bits) - 1)) as i32;
    value.wrapping_add(bias) >> bits
}

fn divide_512(value: i32) -> i32 {
    divide_power(value, 9)
}

fn divide_8192(value: i32) -> i32 {
    divide_power(value, 13)
}

fn divide_16384(value: i32) -> i32 {
    divide_power(value, 14)
}

#[cfg(test)]
#[path = "vtol_tests.rs"]
mod tests;
