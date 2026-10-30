//! Actual425400: genuine contact, alignment (modes 3/4 to their function
//! statics), forces, physics and installed helper. This caller never borrows
//! class36's native stack-domain token.
use super::contact::{
    ActorContactEnvironment, ActorContactError, ActorContactHeaderHost, ActorContactOutput,
    ActorContactRequest, ActorContactStackPolicy,
};
use super::kinematics::{ftol, ActorMotionError};
use super::selector10_helper::{Selector10HelperTickError, Selector10HelperTickRequest};
use super::{
    ActorAngularAxis, ActorAngularMotionRequest, ActorAngularMotionTable,
    ActorForwardMotionRequest, ActorForwardMotionTable, ActorLateralMotionRequest,
    ActorLateralMotionTable, ActorPhysicsRequest, DerivedActorError,
};
use crate::{
    object_runtime::{
        ActorAngularResponseRequest, ActorTerrainHost, ObjectArena, ObjectError, ObjectId,
    },
    process_sound::ProcessSoundRuntime,
    ResidentRuntime,
};
use inf_formats::{native_basis::NativeBasis, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

pub struct Selector10MotionRequest<'a> {
    pub microseconds: u32,
    pub controls: [i32; 2],
    pub executable: FrontendExecutable<'a>,
    pub environment: ActorContactEnvironment<'a>,
    pub random_state: &'a mut u32,
    pub residency: &'a ResidentRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector10MotionNextPhase {
    PersistentAudio,
}
#[derive(Debug, Error)]
pub enum Selector10MotionError<T, H> {
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
    #[error(transparent)]
    Helper(#[from] Selector10HelperTickError),
    #[error("actor {actor:?} requires full268 at {address:08X}")]
    RequiredMode { actor: ObjectId, address: u32 },
}
/// 0055F280: the CRT-initialized world up vector.
const WORLD_UP: [f32; 3] = [0.0, 1.0, 0.0];

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
    pub fn tick_selector10_motion<T: ActorTerrainHost, H: ActorContactHeaderHost>(
        &mut self,
        actor: ObjectId,
        request: Selector10MotionRequest<'_>,
        terrain: &mut T,
        header: &mut H,
    ) -> Result<Selector10MotionNextPhase, Selector10MotionError<T::Error, H::Error>> {
        self.selector10_actor(actor)?;
        let step = request.microseconds >> 5;
        let contact_step = if (self.actor(actor)?.word_c4 as i32) > 1 {
            step
        } else {
            0
        };
        let table = self.family4236d0_actor(actor)?.table_25c;
        let mut contact = ActorContactOutput::default();
        self.apply_actor_contact(
            actor,
            ActorContactRequest {
                table,
                step: contact_step,
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
            let mode = self.selector10_actor(actor)?.word_268.ok_or(
                Selector10MotionError::RequiredMode {
                    actor,
                    address: 0x42544c,
                },
            )? as i32;
            let basis = self.actor(actor)?.rotating.basis;
            let desired_axis =
                alignment_axis(mode, self.actor(actor)?.word_c4 as i32, normal, basis);
            self.apply_actor_angular_response(
                actor,
                ActorAngularResponseRequest {
                    current_axis: std::array::from_fn(|i| f32::from_bits(basis.words[4 + i])),
                    desired_axis,
                    gain: (alignment.wrapping_mul(step) >> 14) as i32,
                },
            )?;
        }
        //FreshC4 after contact/alignment; the contact H12/H14 hosts may have
        //changed the live actor, so the earlier contact gate is not reused.
        if (self.actor(actor)?.word_c4 as i32) > 1 {
            if alignment != 0 {
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
        //42571E NULL helper bypasses even268. Actual callback always precedes
        //the fresh mode/impulse gates; no mode1/count4 success shortcut.
        if self.selector10_actor(actor)?.helper_26c.is_some() {
            let completed = self.tick_selector10_helper(
                actor,
                Selector10HelperTickRequest {
                    microseconds: request.microseconds,
                    executable: request.executable,
                    random_state: request.random_state,
                    residency: request.residency,
                    sounds: request.sounds,
                },
            )?;
            let mode = self.selector10_actor(actor)?.word_268.ok_or(
                Selector10MotionError::RequiredMode {
                    actor,
                    address: 0x42572f,
                },
            )? as i32;
            if (3..=4).contains(&mode)
                && completed != 0
                && alignment != 0
                && (self.actor(actor)?.word_c4 as i32) > 1
            {
                let state = self.actor(actor)?;
                let [x, y, z] = state.words_8e_92.map(|v| f64::from(v as i16));
                let c = |i| f64::from(f32::from_bits(state.rotating.basis.words[i]));
                let forward = ftol((z * c(9) + y * c(8)) + x * c(7));
                if forward > 50 {
                    let impulse =
                        u32::from(contact.impulse.unwrap()).wrapping_mul(completed) as i32 >> 3;
                    let power = f64::from(impulse as i16);
                    let increment = normal.map(|v| ftol(power * f64::from(v)) as u16);
                    self.add_actor_linear_velocity(actor, increment)?;
                    self.owned_mut(actor)?.state.base.flags |= 0x400;
                }
            }
        }
        Ok(Selector10MotionNextPhase::PersistentAudio)
    }
}
/// The axis 00426730 turns the Y row toward (`0042544C..00425562`). Modes
/// 3 and 4 align to a function static instead of the ground's normal:
/// alive (C4 at least 3), the world up 0055F280, copied once into
/// 00546A50; dying, the normal crossed with basis row 2 (+60..+68),
/// rewritten into 00546A88 on each call (exact products, one rounding to
/// f32 each). Their first-use guards (00546A68) and atexit registrations
/// change nothing else.
pub(super) fn alignment_axis(mode: i32, c4: i32, normal: [f32; 3], basis: NativeBasis) -> [f32; 3] {
    if !(3..=4).contains(&mode) {
        return normal;
    }
    if c4 >= 3 {
        return WORLD_UP;
    }
    let row = |i: usize| f64::from(f32::from_bits(basis.words[7 + i]));
    let [x, y, z] = normal.map(f64::from);
    [
        (y * row(2) - z * row(1)) as f32,
        (z * row(0) - row(2) * x) as f32,
        (row(1) * x - y * row(0)) as f32,
    ]
}

/// 0047A6A0: the normal in the actor's frame.
pub(super) fn inverse_floats(basis: NativeBasis, normal: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = normal.map(f64::from);
    std::array::from_fn(|axis| {
        let c = |i| f64::from(f32::from_bits(basis.words[1 + axis * 3 + i]));
        ((z * c(2) + y * c(1)) + x * c(0)) as f32
    })
}
pub(super) fn damp(word: u16, step: u32, threshold: i32) -> u16 {
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
