//! Complete481C10/482160 class36 motion. Contact still runs for nonpositive
//! status with its native zero step; the actual elapsed step drives alignment
//! and final physics. Persistent481B10 audio is a distinct reached continuation.

use super::contact::{
    ActorContactEnvironment, ActorContactError, ActorContactHeaderHost, ActorContactOutput,
    ActorContactRequest, ActorContactStackPolicy,
};
use super::kinematics::{
    divide, ftol, transform_local_dwords, transform_world_dwords, ActorMotionError,
};
use super::{
    ActorAngularAxis, ActorAngularMotionRequest, ActorAngularMotionTable,
    ActorForwardMotionRequest, ActorForwardMotionTable, ActorLateralMotionRequest,
    ActorLateralMotionTable, ActorPhysicsRequest, DerivedActorError,
};
use crate::object_runtime::{
    ActorAngularResponseRequest, ActorTerrainHost, ObjectArena, ObjectError, ObjectId,
};
use inf_formats::{
    native_basis::{inverse_sine_q15, NativeBasis, NativeBasisMath, RotationPlane},
    FrontendExecutable, FrontendExecutableError,
};
use thiserror::Error;

pub struct Selector36MotionRequest<'a> {
    pub microseconds: u32,
    /// Native full DWORD controls: yaw then forward. This is the returned
    /// current prefix output, not platform input or a fabricated zero command.
    pub controls: [i32; 2],
    pub executable: FrontendExecutable<'a>,
    pub environment: ActorContactEnvironment<'a>,
    pub stack_policy: ActorContactStackPolicy,
    pub math: &'a NativeBasisMath,
    pub inverse_sine: &'a [i16; 1024],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector36MotionNextPhase {
    PersistentAudio481b10 { controls: [i32; 2] },
}

#[derive(Debug, Error)]
pub enum Selector36MotionError<T, H> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error(transparent)]
    Table(#[from] FrontendExecutableError),
    #[error(transparent)]
    Contact(#[from] ActorContactError<T, H>),
}

pub(super) fn table_dwords(
    executable: FrontendExecutable<'_>,
    address: u32,
    count: usize,
) -> Result<Vec<i32>, FrontendExecutableError> {
    let bytes = executable.mapped(address, count * 4)?.bytes;
    Ok(bytes
        .chunks_exact(4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
        .collect())
}

impl ObjectArena {
    pub fn tick_selector36_motion<T: ActorTerrainHost, H: ActorContactHeaderHost>(
        &mut self,
        actor: ObjectId,
        request: Selector36MotionRequest<'_>,
        terrain: &mut T,
        header: &mut H,
    ) -> Result<Selector36MotionNextPhase, Selector36MotionError<T::Error, H::Error>> {
        self.selector36_actor(actor)?;
        let step = request.microseconds >> 5;
        let positive = (self.actor(actor)?.word_c4 as i32) > 0;
        let contact_table = self.family4236d0_actor(actor)?.table_25c;
        let mut contact = ActorContactOutput::default();
        self.apply_actor_contact(
            actor,
            ActorContactRequest {
                table: contact_table,
                step: if positive { step } else { 0 },
                executable: request.executable,
                environment: &request.environment,
                stack_policy: request.stack_policy,
            },
            terrain,
            header,
            &mut contact,
        )?;
        let normal = contact
            .normal
            .map(|x| x.expect("normal published on contact return"));
        let alignment = contact
            .alignment
            .expect("alignment published on contact return");
        if alignment != 0 {
            self.align_selector36::<T, H::Error>(
                actor,
                normal,
                alignment,
                step,
                request.math,
                request.inverse_sine,
                terrain,
            )?;
        }
        if positive && alignment != 0 {
            let local = inverse_floats(self.actor(actor)?.rotating.basis, normal);
            if local[1] > 0.0 {
                let mut power =
                    ftol(f64::from(contact.motor.unwrap() as i32) * f64::from(local[1]));
                if power != 0 {
                    let mut motor_step = step;
                    if self.family428650_actor(actor)?.word_22e & 0x20 != 0 {
                        power = power.wrapping_add((power as u32 >> 1) as i32);
                        if power >= 0x10000 {
                            power = 0xffff;
                        }
                        motor_step = motor_step.wrapping_add(motor_step >> 1);
                    }
                    let pointer = self.family4236d0_actor(actor)?.table_254;
                    let t = table_dwords(request.executable, pointer, 4)?;
                    self.apply_actor_forward_motion(
                        actor,
                        ActorForwardMotionRequest {
                            power,
                            desired_velocity: request.controls[1],
                            step_units: motor_step,
                            table: ActorForwardMotionTable {
                                maximum: t[0],
                                minimum: t[1],
                                acceleration: t[2],
                                deceleration: t[3],
                            },
                        },
                    )?;
                    let pointer = self.family4236d0_actor(actor)?.table_258;
                    let t = table_dwords(request.executable, pointer, 2)?;
                    self.apply_actor_angular_motion(
                        actor,
                        ActorAngularMotionRequest {
                            power,
                            desired_turn: request.controls[0],
                            step_units: motor_step,
                            table: ActorAngularMotionTable {
                                maximum: t[0],
                                response: t[1],
                            },
                            axis: ActorAngularAxis::LocalY,
                        },
                    )?;
                    let pointer = self.family4236d0_actor(actor)?.table_250;
                    let t = table_dwords(request.executable, pointer, 2)?;
                    self.apply_actor_lateral_motion(
                        actor,
                        ActorLateralMotionRequest {
                            power,
                            step_units: motor_step,
                            table: ActorLateralMotionTable {
                                threshold: t[0] as u32,
                                rate: t[1],
                            },
                        },
                    )?;
                }
            }
        }
        if (self.actor(actor)?.word_c4 as i32) < 3 && self.object(actor)?.base.flags & 0x80000 != 0
        {
            let velocity = self.actor(actor)?.words_8e_92;
            if velocity.iter().any(|v| *v != 0) {
                for axis in 0..3 {
                    self.actor_mut(actor)?.words_8e_92[axis] =
                        ground_velocity(velocity[axis], step, [64, 24, 64][axis]);
                }
            }
        }
        self.tick_actor_physics(
            actor,
            ActorPhysicsRequest {
                microseconds: request.microseconds,
                mask: 0x1f,
                sine_quarter: request.environment.sine_quarter,
            },
        )?;
        Ok(Selector36MotionNextPhase::PersistentAudio481b10 {
            controls: request.controls,
        })
    }

    /// 482160 captures Y/Z/X before its possible fresh H12 acquisition, then
    /// uses unsigned size*2. Both local rotation corrections precede47D440.
    fn align_selector36<T: ActorTerrainHost, H>(
        &mut self,
        actor: ObjectId,
        normal: [f32; 3],
        alignment: u32,
        step: u32,
        math: &NativeBasisMath,
        inverse_sine: &[i16; 1024],
        terrain: &mut T,
    ) -> Result<(), ActorContactError<T::Error, H>> {
        let xyz = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ActorContactError::MissingPosition(actor))?;
        let y = i32::from(xyz[1] as i16);
        let z = xyz[2];
        let x = xyz[0];
        let current = terrain
            .terrain(self, actor)
            .map_err(ActorContactError::Terrain)?;
        let separation = y.wrapping_sub(i32::from(current.height_at([x, z])));
        let size2 = i32::from(self.object(actor)?.base.word_28).wrapping_shl(1);
        if separation < size2 {
            let bound = divide(separation.wrapping_mul(0x7fff), size2, 0x0048_21b9)?.max(0x38e);
            let mut local = inverse_normal(self.actor(actor)?.rotating.basis, normal);
            if local[1] < 0x7eb7 {
                let angle = quadrant_angle(local[0], local[1], inverse_sine);
                if angle.wrapping_abs() > bound {
                    let correction = if angle > 0 {
                        bound.wrapping_sub(angle)
                    } else {
                        angle.wrapping_add(bound).wrapping_neg()
                    };
                    self.rotate_visual(actor, RotationPlane::Xy, correction, math)?;
                    self.alignment_angular_recovery(actor, 2, correction)?;
                    local = inverse_normal(self.actor(actor)?.rotating.basis, normal);
                }
                let angle = quadrant_angle(local[2], local[1], inverse_sine);
                if angle.wrapping_abs() > bound {
                    let correction = if angle > 0 {
                        angle.wrapping_sub(bound)
                    } else {
                        angle.wrapping_add(bound)
                    };
                    self.rotate_visual(actor, RotationPlane::Yz, correction, math)?;
                    self.alignment_angular_recovery(actor, 0, correction)?;
                }
            }
        }
        if alignment != 0 {
            let basis = self.actor(actor)?.rotating.basis;
            let current_axis = std::array::from_fn(|i| f32::from_bits(basis.words[4 + i]));
            self.apply_actor_angular_response(
                actor,
                ActorAngularResponseRequest {
                    current_axis,
                    desired_axis: normal,
                    gain: (alignment.wrapping_mul(step) >> 14) as i32,
                },
            )?;
        }
        Ok(())
    }

    fn alignment_angular_recovery(
        &mut self,
        actor: ObjectId,
        axis: usize,
        correction: i32,
    ) -> Result<(), ObjectError> {
        let state = self.actor(actor)?;
        let basis = state.rotating.basis;
        let local = transform_world_dwords(basis, state.words_94_9c.map(|v| v as i32));
        if (local[axis] ^ correction) < 0 {
            let mut recovery = [0; 3];
            recovery[axis] = (local[axis] / 2).wrapping_neg();
            let world = transform_local_dwords(basis, recovery);
            self.add_actor_angular_velocity(actor, world.map(|v| v as u32))?;
        }
        Ok(())
    }
}

/// 47A6A0->490710->4921B0/E0/2210: source floats captured before any
/// output, (Z product+Y product)+X and one F32 spill on each output axis.
fn inverse_floats(basis: NativeBasis, normal: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = normal.map(f64::from);
    std::array::from_fn(|axis| {
        let c = |i| f64::from(f32::from_bits(basis.words[1 + axis * 3 + i]));
        ((z * c(2) + y * c(1)) + x * c(0)) as f32
    })
}
fn inverse_normal(basis: NativeBasis, normal: [f32; 3]) -> [i32; 3] {
    transform_world_dwords(basis, normal.map(|v| ftol(f64::from(v) * 32767.0)))
}
fn quadrant_angle(component: i32, local_y: i32, table: &[i16; 1024]) -> i32 {
    let mut angle = inverse_sine_q15(component.clamp(-0x7fff, 0x7fff), table);
    if local_y < 0 {
        angle = if angle > 0 {
            0x8000i32.wrapping_sub(angle)
        } else {
            (-0x8000i32).wrapping_sub(angle)
        };
    }
    angle
}
fn ground_velocity(word: u16, step: u32, threshold: i32) -> u16 {
    let velocity = i32::from(word as i16);
    if velocity == 0 {
        return word;
    }
    let decrement = (velocity.wrapping_mul(step as i32) as u32 >> 14) as i32;
    let clear = if velocity > 0 {
        velocity <= threshold || velocity <= decrement
    } else {
        velocity >= -threshold || velocity >= decrement
    };
    if clear {
        0
    } else {
        word.wrapping_sub(decrement as u16)
    }
}

#[cfg(test)]
mod tests;
