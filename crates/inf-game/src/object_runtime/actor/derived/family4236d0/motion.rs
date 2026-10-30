//! Actual 00424CF0, the family's own +118 (selector 9's motion). It reads
//! `+C4` once: a live actor (above 1) takes its contact at `delta >> 5`,
//! answers its normal, then drives; a dying one takes contact at step zero,
//! answers it and damps its spin. Without contact a live actor silences its
//! engine scales `+16C/+170`. Shared physics closes every path. Selector
//! 10's 00425400 adds its mode `+268` and helper `+26C`, which this class
//! does not have.

use super::super::contact::{
    ActorContactEnvironment, ActorContactError, ActorContactHeaderHost, ActorContactOutput,
    ActorContactRequest, ActorContactStackPolicy,
};
use super::super::kinematics::{ftol, ActorMotionError};
use super::super::selector10_motion::{damp, inverse_floats};
use super::super::{
    ActorAngularAxis, ActorAngularMotionRequest, ActorAngularMotionTable,
    ActorForwardMotionRequest, ActorForwardMotionTable, ActorLateralMotionRequest,
    ActorLateralMotionTable, ActorPhysicsRequest, DerivedActorError,
};
use super::Family4236d0Class;
use crate::object_runtime::{
    ActorAngularResponseRequest, ActorTerrainHost, ObjectArena, ObjectError, ObjectId,
};
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

pub struct Family4236d0MotionRequest<'a> {
    pub microseconds: u32,
    /// The tail's `[yaw, forward]`.
    pub controls: [i32; 2],
    pub executable: FrontendExecutable<'a>,
    pub environment: ActorContactEnvironment<'a>,
}

#[derive(Debug, Error)]
pub enum Family4236d0MotionError<T, H> {
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

fn dwords(
    exe: FrontendExecutable<'_>,
    address: u32,
    count: usize,
) -> Result<Vec<i32>, FrontendExecutableError> {
    Ok(exe
        .mapped(address, count * 4)?
        .bytes
        .chunks_exact(4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
        .collect())
}

impl ObjectArena {
    pub fn tick_family4236d0_motion<T: ActorTerrainHost, H: ActorContactHeaderHost>(
        &mut self,
        actor: ObjectId,
        request: Family4236d0MotionRequest<'_>,
        terrain: &mut T,
        header: &mut H,
    ) -> Result<(), Family4236d0MotionError<T::Error, H::Error>> {
        if !matches!(
            self.family4236d0_actor(actor)?.class,
            Family4236d0Class::Base
        ) {
            return Err(DerivedActorError::NotFamilyUpdate424830(actor).into());
        }
        let step = request.microseconds >> 5;
        // 00424CFF: the one +C4 read.
        let alive = (self.actor(actor)?.word_c4 as i32) > 1;
        let table = self.family4236d0_actor(actor)?.table_25c;
        let mut contact = ActorContactOutput::default();
        self.apply_actor_contact(
            actor,
            ActorContactRequest {
                table,
                step: if alive { step } else { 0 },
                executable: request.executable,
                environment: &request.environment,
                stack_policy: ActorContactStackPolicy::RequiredNativeAddress,
            },
            terrain,
            header,
            &mut contact,
        )?;
        let normal = contact
            .normal
            .map(|v| v.expect("normal published on contact return"));
        let alignment = contact
            .alignment
            .expect("alignment published on contact return");
        if alignment != 0 {
            // 00424D41 / 00424E56: 00426730 on the Y basis.
            let basis = self.actor(actor)?.rotating.basis;
            self.apply_actor_angular_response(
                actor,
                ActorAngularResponseRequest {
                    current_axis: std::array::from_fn(|i| f32::from_bits(basis.words[4 + i])),
                    desired_axis: normal,
                    gain: (alignment.wrapping_mul(step) >> 14) as i32,
                },
            )?;
        }
        if alive {
            if alignment == 0 {
                // 00424E10..00424E16.
                let derived = self.derived_actor_mut(actor)?;
                derived.word_170 = Some(0);
                derived.word_16c = Some(0);
            } else {
                // 00424D52: 0047A6A0, the normal in the actor's frame.
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
                        let t = dwords(
                            request.executable,
                            self.family4236d0_actor(actor)?.table_254,
                            4,
                        )?;
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
                        let t = dwords(
                            request.executable,
                            self.family4236d0_actor(actor)?.table_258,
                            2,
                        )?;
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
                        let t = dwords(
                            request.executable,
                            self.family4236d0_actor(actor)?.table_250,
                            2,
                        )?;
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
        } else if self.object(actor)?.base.flags & 0x80000 != 0 {
            // 00424E5B..00424F2A.
            let velocity = self.actor(actor)?.words_8e_92;
            if velocity.iter().any(|v| *v != 0) {
                for axis in 0..3 {
                    self.actor_mut(actor)?.words_8e_92[axis] =
                        damp(velocity[axis], step, [64, 24, 64][axis]);
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
        Ok(())
    }
}

#[cfg(test)]
#[path = "motion_tests.rs"]
mod tests;
