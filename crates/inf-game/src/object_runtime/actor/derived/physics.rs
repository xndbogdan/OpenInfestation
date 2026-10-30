//! 0047B1E0 and gravity 0047AD10. All arithmetic retains native DWORD/WORD
//! wrapping and the order gravity → angular drag → linear drag → position
//! → world-vector rotation. This is shared physics, not an enclosing +24.

use inf_formats::native_basis::isqrt_48efc0;

use super::kinematics::{divide, ActorMotionError};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

#[derive(Clone, Copy, Debug)]
pub struct ActorPhysicsRequest<'a> {
    pub microseconds: u32,
    /// Native low byte: 01 gravity, 02 angular drag, 04 linear drag,
    /// 08 position integration, 10 world-space angular integration.
    pub mask: u8,
    pub sine_quarter: &'a [u16; 4096],
}

impl ObjectArena {
    /// 0047B510 adds signed impulse bit patterns with WORD wrapping. It
    /// does not mark position/basis dirty; integration owns those writes.
    pub fn add_actor_linear_velocity(
        &mut self,
        actor: ObjectId,
        increment: [u16; 3],
    ) -> Result<(), ObjectError> {
        for (word, increment) in self.actor_mut(actor)?.words_8e_92.iter_mut().zip(increment) {
            *word = word.wrapping_add(increment);
        }
        Ok(())
    }

    /// 0047B470's direct WORLD angular DWORD addition, without basis
    /// transformation or dirty writes. The local-vector helper is 0047B4B0.
    pub fn add_actor_angular_velocity(
        &mut self,
        actor: ObjectId,
        increment: [u32; 3],
    ) -> Result<(), ObjectError> {
        for (word, increment) in self.actor_mut(actor)?.words_94_9c.iter_mut().zip(increment) {
            *word = word.wrapping_add(increment);
        }
        Ok(())
    }

    /// 0047B4B0: a local DWORD vector through the actor's basis at +44
    /// (0047A6E0, then 004907E0), added to the world angular DWORDs as
    /// 0047B470 does.
    pub fn add_actor_local_angular(
        &mut self,
        actor: ObjectId,
        local: [i32; 3],
    ) -> Result<(), ObjectError> {
        let basis = self.actor(actor)?.rotating.basis;
        let world = super::kinematics::transform_local_dwords(basis, local);
        self.add_actor_angular_velocity(actor, world.map(|word| word as u32))
    }

    pub fn tick_actor_physics(
        &mut self,
        actor: ObjectId,
        request: ActorPhysicsRequest<'_>,
    ) -> Result<(), ActorMotionError> {
        let step = request.microseconds >> 5;
        if request.mask & 1 != 0 {
            let velocity = &mut self.actor_mut(actor)?.words_8e_92[1];
            *velocity = velocity.wrapping_sub((request.microseconds >> 10) as u16);
        }
        if request.mask & 2 != 0 {
            let angular = self.actor(actor)?.words_94_9c.map(|word| word as i32);
            let squares = angular.map(|value| value.wrapping_mul(value));
            let square = squares[0].wrapping_add(squares[1]).wrapping_add(squares[2]);
            let factor = request.microseconds.wrapping_shl(10) as i32;
            if squares.iter().all(|&value| value >= 0) && (0..=0x3fff_0001).contains(&square) {
                let magnitude = isqrt_48efc0(square) as i16 as i32;
                let decrement = multiply_high31(magnitude, factor).wrapping_add(2);
                if magnitude > decrement {
                    let remaining = magnitude.wrapping_sub(decrement);
                    // Each division/write is reached separately. A trap on
                    // a later axis must retain the preceding axis writes.
                    for (axis, value) in angular.into_iter().enumerate() {
                        let next = divide(
                            value.wrapping_mul(remaining),
                            magnitude,
                            [0x0047_b288, 0x0047_b29a, 0x0047_b2ac][axis],
                        )?;
                        self.actor_mut(actor)?.words_94_9c[axis] = next as u32;
                    }
                } else {
                    self.actor_mut(actor)?.words_94_9c = [0; 3];
                }
            } else {
                // The large/overflowed-square branch uses the high DWORD
                // of a signed 64-bit product shifted left through carry.
                // It does not use the short-vector square root or +2.
                for (axis, value) in angular.into_iter().enumerate() {
                    self.actor_mut(actor)?.words_94_9c[axis] =
                        value.wrapping_sub(multiply_high31(value, factor)) as u32;
                }
            }
        }
        if request.mask & 4 != 0 {
            let velocity = self
                .actor(actor)?
                .words_8e_92
                .map(|word| word as i16 as i32);
            let square = velocity[0]
                .wrapping_mul(velocity[0])
                .wrapping_add(velocity[1].wrapping_mul(velocity[1]))
                .wrapping_add(velocity[2].wrapping_mul(velocity[2]));
            let magnitude = isqrt_48efc0(square) as i32;
            let decrement =
                (((square as u32 >> 12) as i32).wrapping_mul(step as i32) >> 15) as i16 as i32;
            let remaining = magnitude.wrapping_sub(decrement).wrapping_sub(1);
            if magnitude > 0 {
                // Native tests magnitude, not remaining: large deltas may
                // reverse velocity instead of flooring the remaining value.
                for (axis, value) in velocity.into_iter().enumerate() {
                    let next = divide(
                        value.wrapping_mul(remaining),
                        magnitude,
                        [0x0047_b38f, 0x0047_b3a3, 0x0047_b3b7][axis],
                    )?;
                    self.actor_mut(actor)?.words_8e_92[axis] = next as u16;
                }
            } else {
                self.actor_mut(actor)?.words_8e_92 = [0; 3];
            }
        }
        if request.mask & 8 != 0 {
            let velocity = self.actor(actor)?.words_8e_92;
            let base = &mut self.owned_mut(actor)?.state.base;
            let mut position = base
                .position_words
                .ok_or(ActorMotionError::MissingPosition(actor))?;
            for (word, velocity) in position.iter_mut().zip(velocity) {
                let increment = i32::from(velocity as i16).wrapping_mul(step as i32) >> 15;
                *word = word.wrapping_add(increment as u16);
            }
            base.position_words = Some(position);
            base.dirty_word |= 0xc0;
        }
        if request.mask & 0x10 != 0 {
            let angular = self.actor(actor)?.words_94_9c;
            if angular.iter().any(|&word| word != 0) {
                let world =
                    angular.map(|word| ((word as i32).wrapping_mul(step as i32) >> 15) as u16);
                super::super::selector16::rotate_world_vector(
                    &mut self.actor_mut(actor)?.rotating.basis,
                    world,
                    request.sine_quarter,
                );
                self.owned_mut(actor)?.state.base.dirty_word |= 0x10;
            }
        }
        Ok(())
    }
}

/// 0047B26C / 0047B2DE: signed IMUL EDX:EAX, SHL EAX then RCL EDX.
/// Taking the low DWORD after arithmetic >>31 preserves that exact bit train.
fn multiply_high31(left: i32, right: i32) -> i32 {
    (i64::from(left) * i64::from(right) >> 31) as i32
}

#[cfg(test)]
#[path = "physics_tests.rs"]
mod tests;
