//! Selector34 +118=00434C10: local force response, shared physics, then
//! the independently scheduled 005286D8 ribbon owner. No RNG is consumed.
use super::super::kinematics::{divide, ftol, transform_local_dwords, transform_world_dwords};
use super::Selector34MotionControls;
use crate::object_runtime::{
    ActorHandle, ActorMotionError, ActorPhysicsRequest, DerivedActorError, ObjectArena,
    ObjectError, ObjectId,
};
use inf_formats::{native_basis::inverse_sine_q15, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

pub trait Selector34MotionHost {
    type Error;
    fn create_trail(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<ObjectId>, Self::Error>;
    fn update_trail(
        &mut self,
        arena: &mut ObjectArena,
        trail: ObjectId,
        microseconds: u32,
        position_owner: ObjectId,
    ) -> Result<(), Self::Error>;
}
pub struct Selector34MotionRequest<'a> {
    pub microseconds: u32,
    pub controls: &'a mut Selector34MotionControls,
    pub executable: FrontendExecutable<'a>,
    pub sine_quarter: &'a [u16; 4096],
    pub inverse_sine: &'a [i16; 1024],
}
#[derive(Debug, Error)]
pub enum Selector34MotionError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error(transparent)]
    Table(#[from] FrontendExecutableError),
    #[error("selector34 {actor:?} requires field+{offset:X} at{address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("selector34 {actor:?} trail service at{address:08X}: {error}")]
    Trail {
        actor: ObjectId,
        address: u32,
        error: E,
    },
}

impl ObjectArena {
    pub fn tick_selector34_motion<H: Selector34MotionHost>(
        &mut self,
        actor: ObjectId,
        request: Selector34MotionRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector34MotionError<H::Error>> {
        self.selector34_actor(actor)?;
        self.selector34_motion_forces(
            actor,
            request.microseconds,
            request.controls,
            request.executable,
            request.inverse_sine,
        )?;
        self.tick_actor_physics(
            actor,
            ActorPhysicsRequest {
                microseconds: request.microseconds,
                mask: 0x19,
                sine_quarter: request.sine_quarter,
            },
        )?;
        if let Some(trail) = self.selector34_actor(actor)?.trail_25c {
            host.update_trail(self, trail, request.microseconds, actor)
                .map_err(|error| Selector34MotionError::Trail {
                    actor,
                    address: 0x434ebc,
                    error,
                })?;
        } else {
            let source = self.actor(actor)?.handle;
            let trail =
                host.create_trail(self, source)
                    .map_err(|error| Selector34MotionError::Trail {
                        actor,
                        address: 0x434ea3,
                        error,
                    })?;
            self.selector34_actor_mut(actor)?.trail_25c = trail;
        }
        Ok(())
    }

    fn selector34_motion_forces<E>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        controls: &Selector34MotionControls,
        exe: FrontendExecutable<'_>,
        inverse: &[i16; 1024],
    ) -> Result<(), Selector34MotionError<E>> {
        let step = delta >> 5;
        let basis = self.actor(actor)?.rotating.basis;
        let mut velocity = basis.inverse_words(self.actor(actor)?.words_8e_92.map(|v| v as i16));
        let mut angular =
            transform_world_dwords(basis, self.actor(actor)?.words_94_9c.map(|v| v as i32));
        let damping = delta >> 4;
        velocity[0] = (velocity[0] as u16)
            .wrapping_sub((i32::from(velocity[0]).wrapping_mul(damping as i32) >> 15) as u16)
            as i16;
        let loss = (i32::from(velocity[1]).wrapping_mul(damping as i32) >> 15) as i16;
        velocity[1] = (velocity[1] as u16).wrapping_sub(loss as u16) as i16;
        velocity[2] =
            (velocity[2] as u16).wrapping_add(i32::from(loss).wrapping_abs() as u16) as i16;
        let table = self.selector34_actor(actor)?.table_258;
        let thrust = forward_response(
            actor,
            i32::from(self.object(actor)?.base.word_28),
            i32::from(velocity[2]),
            controls.thrust,
            table,
            step,
            exe,
        )?;
        velocity[2] = (velocity[2] as u16).wrapping_add(thrust as u16) as i16;
        let threshold = f32::from_bits(exe.word(0x51f198)?);
        if f32::from_bits(basis.words[5]) > threshold {
            let lift = (i32::from(velocity[2]).wrapping_mul(step as i32) >> 15) as i16 as i32;
            let cap = (delta.wrapping_mul(3) >> 12) as i32;
            velocity[1] = (velocity[1] as u16).wrapping_add(lift.min(cap) as u16) as i16;
        }
        angular[1] = angular[1].wrapping_add(
            (i32::from(velocity[0])
                .wrapping_shl(7)
                .wrapping_mul(step as i32)
                >> 15) as i16 as i32,
        );
        angular[0] = angular[0].wrapping_sub(
            (i32::from(velocity[1])
                .wrapping_shl(7)
                .wrapping_mul(step as i32)
                >> 15) as i16 as i32,
        );
        let loss = ((i64::from(angular[1]) * i64::from(step.wrapping_shl(14) as i32)) >> 31) as i32;
        angular[2] = angular[2].wrapping_sub(loss);
        angular[1] = angular[1].wrapping_sub(loss);
        let factor = f64::from(f32::from_bits(exe.word(0x5205a4)?));
        let mut angle = inverse_sine_q15(
            ftol(f64::from(f32::from_bits(basis.words[8])) * factor),
            inverse,
        );
        //FCOMP's carry flag includes unordered, unlike a Rust `<` test.
        let inverted = !(f32::from_bits(basis.words[5]) >= threshold);
        if inverted {
            angle = if angle > 0 {
                0x8000i32.wrapping_sub(angle)
            } else {
                (-0x8000i32).wrapping_sub(angle)
            };
        }
        let correction = (angle.wrapping_mul(step as i32) >> 15) as i16 as i32;
        angular[0] = opposing_add(angular[0], correction);
        let bank = ftol(f64::from(f32::from_bits(basis.words[2])) * factor);
        let correction = (((controls
            .yaw
            .wrapping_abs()
            .wrapping_sub(0xffff)
            .wrapping_mul(bank)
            >> 14)
            .wrapping_mul(step as i32)
            >> 15) as i16) as i32;
        let correction = if inverted {
            correction.wrapping_shl(1)
        } else {
            correction
        };
        angular[2] = opposing_add(angular[2], correction);
        let table = self.selector34_actor(actor)?.table_254;
        angular[1] = angular[1].wrapping_add(angular_response(
            actor,
            angular[1],
            controls.yaw,
            table,
            step,
            exe,
            0x254,
        )?);
        angular[2] = angular[2].wrapping_add(angular_response(
            actor,
            angular[2],
            controls.yaw.wrapping_neg().wrapping_shl(1),
            table,
            step,
            exe,
            0x254,
        )?);
        if self.selector34_actor(actor)?.byte_239 != 0 {
            let table = self.selector34_actor(actor)?.table_24c;
            let response =
                angular_response(actor, angular[0], controls.pitch, table, step, exe, 0x24c)?;
            self.selector34_actor_mut(actor)?.byte_239 = 0;
            angular[0] = angular[0].wrapping_add(response);
        } else {
            angular[0] = angular[0].wrapping_add(angular_response(
                actor,
                angular[0],
                controls.pitch,
                self.selector34_actor(actor)?.table_250,
                step,
                exe,
                0x250,
            )?);
        }
        self.actor_mut(actor)?.words_8e_92 = basis.transform_words(velocity);
        self.actor_mut(actor)?.words_94_9c =
            transform_local_dwords(self.actor(actor)?.rotating.basis, angular).map(|v| v as u32);
        Ok(())
    }
}

fn opposing_add(current: i32, correction: i32) -> i32 {
    current.wrapping_add(if current ^ correction < 0 {
        correction.wrapping_mul(2)
    } else {
        correction
    })
}
fn missing<E>(actor: ObjectId, offset: u16, address: u32) -> Selector34MotionError<E> {
    Selector34MotionError::RequiredField {
        actor,
        offset,
        address,
    }
}

///004269D0 returns the size-normalized increment without modifying the actor.
fn forward_response<E>(
    actor: ObjectId,
    size: i32,
    current: i32,
    desired: i32,
    table: Option<u32>,
    step: u32,
    exe: FrontendExecutable<'_>,
) -> Result<i32, Selector34MotionError<E>> {
    let desired = divide(desired.wrapping_shl(12), size, 0x4269e4)?;
    let table = table.ok_or(missing(actor, 0x258, 0x4269e8))?;
    let maximum = exe.word(table)? as i32;
    let desired = if desired > maximum {
        maximum
    } else {
        desired.max(exe.word(table.wrapping_add(4))? as i32)
    };
    let current = divide(current.wrapping_shl(12), size, 0x4269ff)?;
    let difference = desired.wrapping_sub(current);
    if difference == 0 {
        return Ok(0);
    }
    let difference = if difference > 0 {
        difference
            .min((exe.word(table.wrapping_add(8))?.wrapping_mul(step) as i32 >> 15) as i16 as i32)
    } else {
        difference.max(
            ((exe.word(table.wrapping_add(12))?.wrapping_mul(step) as i32 >> 15) as i16 as i32)
                .wrapping_neg(),
        )
    };
    Ok(difference.wrapping_mul(size) >> 12)
}

///00426A50. Zero control returns before dereferencing its table.
fn angular_response<E>(
    actor: ObjectId,
    current: i32,
    control: i32,
    table: Option<u32>,
    step: u32,
    exe: FrontendExecutable<'_>,
    offset: u16,
) -> Result<i32, Selector34MotionError<E>> {
    if control == 0 {
        return Ok(0);
    }
    let table = table.ok_or(missing(actor, offset, 0x426a63))?;
    let maximum = exe.word(table)? as i32;
    let level = divide(control.wrapping_shl(10), maximum, 0x426a6a)?.wrapping_abs();
    let response = exe.word(table.wrapping_add(4))? as i32;
    let limit = divide(maximum.wrapping_shl(10), response, 0x426a7d)?;
    let mut desired = if level > limit {
        maximum
    } else {
        divide(maximum.wrapping_mul(level), limit, 0x426a8b)?
    };
    if control < 0 {
        desired = desired.wrapping_neg();
    }
    let difference = desired.wrapping_sub(current);
    let mut change = (response.wrapping_mul(step as i32) >> 15) as i16 as i32;
    if difference ^ current < 0 {
        change = change.wrapping_mul(6) >> 2;
    }
    Ok(if difference < 0 {
        difference.max(change.wrapping_neg())
    } else {
        difference.min(change)
    })
}

#[cfg(test)]
mod tests;
