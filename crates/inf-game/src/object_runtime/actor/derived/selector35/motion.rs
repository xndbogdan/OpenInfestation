//! Selector35 virtual+118=0043EF60. Direction requests are DWORD controls,
//! but thrust geometry consumes signed low WORDs. The final acceleration is
//! along the captured current basis Y; the desired axis only drives47D440.
use super::super::kinematics::{divide, ftol};
use crate::object_runtime::{
    ActorAngularAxis, ActorAngularMotionRequest, ActorAngularMotionTable,
    ActorAngularResponseRequest, ActorMotionError, ActorPhysicsRequest, ContactAxisNormals,
    DerivedActorError, ObjectArena, ObjectError, ObjectId,
};
use inf_formats::{native_basis::isqrt_48efc0, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selector35MotionControls {
    pub yaw: i32,
    pub vertical: i32,
    pub horizontal: i32,
    pub direction_x: i32,
    pub direction_z: i32,
}
pub struct Selector35MotionRequest<'a> {
    pub microseconds: u32,
    pub controls: &'a mut Selector35MotionControls,
    pub executable: FrontendExecutable<'a>,
    pub sine_quarter: &'a [u16; 4096],
}
#[derive(Debug, Error)]
pub enum Selector35MotionError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error(transparent)]
    Table(#[from] FrontendExecutableError),
    #[error("selector35 {actor:?} requires field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
}
impl ObjectArena {
    pub fn tick_selector35_motion(
        &mut self,
        actor: ObjectId,
        request: Selector35MotionRequest<'_>,
    ) -> Result<(), Selector35MotionError> {
        self.selector35_actor(actor)?;
        self.selector35_motion_forces(
            actor,
            request.microseconds,
            request.controls,
            request.executable,
        )?;
        self.tick_actor_physics(
            actor,
            ActorPhysicsRequest {
                microseconds: request.microseconds,
                mask: 0x1f,
                sine_quarter: request.sine_quarter,
            },
        )?;
        self.selector35_actor_mut(actor)?.byte_250 = 0;
        Ok(())
    }
    fn selector35_motion_forces(
        &mut self,
        actor: ObjectId,
        delta: u32,
        controls: &mut Selector35MotionControls,
        exe: FrontendExecutable<'_>,
    ) -> Result<(), Selector35MotionError> {
        let step = delta >> 5;
        if (self.actor(actor)?.word_c4 as i32) >= 3 {
            if controls.vertical < -976 {
                controls.vertical = -976;
            }
            let needed = controls
                .vertical
                .wrapping_sub(i32::from(self.actor(actor)?.words_8e_92[1] as i16))
                .wrapping_add(976) as u32;
            let (budget, y) = if needed as i32 <= 0 {
                (
                    self.selector35_actor(actor)?.word_25c.ok_or(missing(
                        actor,
                        0x25c,
                        0x0043_efd9,
                    ))?,
                    1i16,
                )
            } else {
                let limit = self.selector35_actor(actor)?.word_258.ok_or(missing(
                    actor,
                    0x258,
                    0x0043_efab,
                ))?;
                if needed < limit {
                    (
                        limit
                            .wrapping_mul(limit)
                            .wrapping_sub(needed.wrapping_mul(needed)),
                        needed as u16 as i16,
                    )
                } else {
                    (0, limit as u16 as i16)
                }
            };
            let (mut x, mut z) = (0i16, 0i16);
            if budget != 0 {
                if controls.horizontal > 1250 {
                    controls.horizontal = 1250;
                }
                let dx = controls.direction_x as u16 as i16;
                let dz = controls.direction_z as u16 as i16;
                let length =
                    isqrt_48efc0(square(dx).wrapping_add(square(dz)) as i32) as u16 as i16 as i32;
                if length != 0 {
                    x = (divide(
                        i32::from(dx).wrapping_mul(controls.horizontal),
                        length,
                        0x0043_f033,
                    )? as u16)
                        .wrapping_sub(self.actor(actor)?.words_8e_92[0])
                        as i16;
                    z = (divide(
                        i32::from(dz).wrapping_mul(controls.horizontal),
                        length,
                        0x0043_f03e,
                    )? as u16)
                        .wrapping_sub(self.actor(actor)?.words_8e_92[2])
                        as i16;
                    let drift = self.selector35_actor(actor)?.word_264 as i32;
                    if drift != 0 {
                        let basis = self.actor(actor)?.rotating.basis;
                        x = (x as u16).wrapping_sub(ftol(
                            f64::from(drift) * f64::from(f32::from_bits(basis.words[1])),
                        ) as u16) as i16;
                        z = (z as u16).wrapping_sub(ftol(
                            f64::from(drift) * f64::from(f32::from_bits(basis.words[3])),
                        ) as u16) as i16;
                    }
                    let squared = square(x).wrapping_add(square(z));
                    if (squared as i32) > budget as i32 {
                        let divisor = isqrt_48efc0(squared as i32) as u16 as i16 as i32;
                        let limit = isqrt_48efc0(budget as i32) as u16 as i16 as i32;
                        x = divide(i32::from(x).wrapping_mul(limit), divisor, 0x0043_f0b3)? as u16
                            as i16;
                        z = divide(i32::from(z).wrapping_mul(limit), divisor, 0x0043_f0be)? as u16
                            as i16;
                    }
                }
            }
            let alternate =
                self.selector35_actor(actor)?
                    .byte_251
                    .ok_or(missing(actor, 0x251, 0x0043_f0c7))?
                    != 0;
            if alternate {
                x >>= 1;
                z >>= 1;
            }
            let squared = square(x).wrapping_add(square(z)).wrapping_add(square(y));
            let mut magnitude = if squared & 0x8000_0000 != 0 {
                isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
            } else {
                isqrt_48efc0(squared as i32)
            };
            let current = basis_y(self, actor)?;
            let vector = [f64::from(x), f64::from(y), f64::from(z)];
            //FPCW027F accumulates Z²+Y², then X²; length spills to float
            //before the component divisions, which each spill once more.
            let length = ((vector[2] * vector[2] + vector[1] * vector[1]) + vector[0] * vector[0])
                .sqrt() as f32;
            let desired = if length == 0.0 {
                [1.0, 0.0, 0.0]
            } else {
                vector.map(|v| (v / f64::from(length)) as f32)
            };
            self.apply_actor_angular_response(
                actor,
                ActorAngularResponseRequest {
                    current_axis: current,
                    desired_axis: desired,
                    gain: if alternate {
                        step as i32
                    } else {
                        (delta >> 2) as i32
                    },
                },
            )?;
            if alternate {
                let dot = (f64::from(desired[2]) * f64::from(current[2])
                    + f64::from(desired[1]) * f64::from(current[1]))
                    + f64::from(desired[0]) * f64::from(current[0]);
                magnitude =
                    ftol(dot * f64::from(magnitude as u16 as i16)) as u16 as i16 as i32 as u32;
            }
            let force = ((magnitude.wrapping_mul(step) as i32) >> 15) as u16 as i16;
            let increment = current.map(|axis| ftol(f64::from(force) * f64::from(axis)) as u16);
            //All three ftol calls precede the native Z,X,Y writes.
            self.actor_mut(actor)?.words_8e_92[2] =
                self.actor(actor)?.words_8e_92[2].wrapping_add(increment[2]);
            self.actor_mut(actor)?.words_8e_92[0] =
                self.actor(actor)?.words_8e_92[0].wrapping_add(increment[0]);
            self.actor_mut(actor)?.words_8e_92[1] =
                self.actor(actor)?.words_8e_92[1].wrapping_add(increment[1]);
            let table = self.selector35_actor(actor)?.table_270.ok_or(missing(
                actor,
                0x270,
                0x0043_f278,
            ))?;
            self.apply_actor_angular_motion(
                actor,
                ActorAngularMotionRequest {
                    power: 65535,
                    desired_turn: controls.yaw,
                    step_units: step,
                    table: ActorAngularMotionTable {
                        maximum: exe.word(table)? as i32,
                        response: exe.word(table.wrapping_add(4))? as i32,
                    },
                    axis: ActorAngularAxis::LocalY,
                },
            )?;
        } else if self.selector35_actor(actor)?.byte_251.ok_or(missing(
            actor,
            0x251,
            0x0043_f298,
        ))? != 0
        {
            self.apply_actor_angular_response(
                actor,
                ActorAngularResponseRequest {
                    current_axis: basis_y(self, actor)?,
                    desired_axis: ContactAxisNormals::NATIVE_STARTUP.positive[1]
                        .map(f32::from_bits),
                    gain: step as i32,
                },
            )?;
            if self.object(actor)?.base.flags & 0x80000 != 0 {
                let velocity = self.actor(actor)?.words_8e_92;
                for axis in 0..3 {
                    self.actor_mut(actor)?.words_8e_92[axis] =
                        damp(velocity[axis], step, [64, 24, 64][axis]);
                }
            }
        }
        Ok(())
    }
}
fn missing(actor: ObjectId, offset: u16, address: u32) -> Selector35MotionError {
    Selector35MotionError::RequiredField {
        actor,
        offset,
        address,
    }
}
fn square(value: i16) -> u32 {
    i32::from(value).wrapping_mul(i32::from(value)) as u32
}
fn basis_y(arena: &ObjectArena, actor: ObjectId) -> Result<[f32; 3], ObjectError> {
    let basis = arena.actor(actor)?.rotating.basis;
    Ok([basis.words[4], basis.words[5], basis.words[6]].map(f32::from_bits))
}
fn damp(word: u16, step: u32, threshold: i32) -> u16 {
    let velocity = i32::from(word as i16);
    if velocity == 0 {
        return word;
    }
    //43F30B uses SAR; similar class36 ground damping uses a logical shift.
    let decrement = velocity.wrapping_mul(step as i32) >> 14;
    if if velocity > 0 {
        velocity <= threshold || velocity <= decrement
    } else {
        velocity >= -threshold || velocity >= decrement
    } {
        0
    } else {
        word.wrapping_sub(decrement as u16)
    }
}

#[cfg(test)]
mod tests;
