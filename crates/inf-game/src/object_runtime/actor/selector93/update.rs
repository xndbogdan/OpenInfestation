//! Complete constructor93 virtual +24, 00440170..004404CC (RET4).
//! Actual FIFO/RNG/target owners precede two fresh H12 acquisitions. Far
//! steering, shared angular response and physics preserve reached prefixes.
//! Portable fallible target allocation retains the native NULL continuation; raw aliases and direct 004400D0 remain separate domains.

use inf_formats::native_basis::{
    inverse_sine_q31, q31_divide, InverseSineQ31Error, NativeBasisMath, RotationPlane,
};
use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorAngularResponseRequest, ActorMotionError, ActorPhysicsRequest, ActorTerrainHost,
        FollowTargetRequest, ObjectArena, ObjectError, ObjectId,
    },
    world_radius_effect::radius_word_distance,
};

pub struct Selector93TickRequest<'a> {
    pub microseconds: u32,
    /// Current settings +7C, consumed by the actual type2 refresh.
    pub settings_7c: u32,
    pub math: &'a NativeBasisMath,
    /// The same original0053FF00 WORDs as Q15 callers, with distinct Q31 policy.
    pub inverse_sine: &'a [i16; 1024],
    pub sine_quarter: &'a [u16; 4096],
    /// Current process0055F280, required only at00440429. No basis-derived up.
    pub up: Option<&'a [f32; 3]>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector93UpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error(transparent)]
    Steering(#[from] InverseSineQ31Error),
    #[error("constructor93 H12 acquisition failed: {0}")]
    Terrain(E),
    #[error("constructor93 actor {actor:?} requires position at {address:08X}")]
    MissingPosition { actor: ObjectId, address: u32 },
    #[error("constructor93 actor {0:?} requires its current target's cached +08")]
    MissingTargetFocus(ObjectId),
    #[error("constructor93 selection reaches an empty actor-list divisor at004401D1")]
    EmptyActorList,
    #[error("constructor93 selected incomplete constructor {object:?}")]
    StoppedConstructionTarget { object: ObjectId },
    #[error("constructor93 requires current process up0055F280 at00440429")]
    MissingUp,
}

impl ObjectArena {
    /// Complete 00440170 in the constructor-backed actor/target domain.
    /// Existing148 bypasses FIFO/RNG; near retirement bypasses terrain/up/math.
    /// Each service observes this same arena. Errors retain earlier writes;
    /// success does not attach native attract admission or context-free update.
    pub fn tick_selector93<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        request: Selector93TickRequest<'_>,
        process_rng: &mut u32,
        terrain: &mut H,
    ) -> Result<(), Selector93UpdateError<H::Error>> {
        let state = self.selector93(actor)?;
        let mut target = state.target_148;
        // 00440196 captures EBP before allocation; NULL retains this speed.
        let allocation_null_speed = state.dword_144;
        if target.is_none() {
            let count = self.actor_list().len() as i32;
            let random = name_entry_rng_next(process_rng) & 0xffff;
            if count == 0 {
                return Err(Selector93UpdateError::EmptyActorList);
            }
            let index = (random as i32 % count) as usize;
            //The original rereads G48 after the pure shared RNG draw. No
            //self/lifecycle/flag/class filter or preselection belongs here.
            let selected = self.actor_list()[index];
            // Portable containment preserves the native divisor, draw and
            // index. Do not resample or allocate a target for this prefix.
            if self.construction_is_stopped(selected)? {
                return Err(Selector93UpdateError::StoppedConstructionTarget { object: selected });
            }
            // Keep the same draw and selected index, but do not allocate a
            // target referring to a retained partial destructor.
            self.ensure_disposal_active(selected)?;
            let allocation = self
                .allocate_actor_service(crate::object_runtime::ActorServiceAllocationRequest {
                    kind: crate::object_runtime::ActorServiceResourceKind::Target(
                        crate::object_runtime::NativeTargetClass::ActorFollow,
                    ),
                })
                .map_err(ObjectError::from)?;
            if let Some(reference) = allocation {
                let handle = self.actor(selected)?.handle;
                let data = self.follow_target(FollowTargetRequest::resolve_actor(handle))?;
                self.construct_native_target(
                    reference,
                    crate::object_runtime::NativeTarget::ActorFollow(data),
                )?;
            }
            self.selector93_mut(actor)?.target_148 = allocation;
            target = allocation;
        }
        let desired_speed = if let Some(reference) = target {
            self.refresh_follow_target_resource(reference, request.settings_7c)?;
            let target = self.actor_follow_target(reference)?;
            let focus = target
                .focus_08
                .ok_or(Selector93UpdateError::MissingTargetFocus(actor))?;
            let position = self.selector93_position(actor, 0x0044_025f)?;
            let delta = std::array::from_fn(|axis| focus[axis].wrapping_sub(position[axis]) as i16);
            let distance = radius_word_distance(delta) as i32;
            if distance < 2000 {
                //Type2 resource release does not destroy its referent. Unlike
                //the class destructor,0044029E then explicitly stores148=NULL.
                self.release_actor_service(reference)?;
                self.selector93_mut(actor)?.target_148 = None;
                return Ok(());
            }

            let clearance = self.selector93_clearance(actor, terrain)?;
            if clearance < 300 {
                self.selector93_add_y(actor, 300i32.wrapping_sub(clearance) as u16, 0x0044_0348)?;
            }
            //Both branches use the saved clearance, even after the first Y write.
            if clearance < 1200 {
                let correction = (request.microseconds >> 3)
                    .wrapping_mul(1200i32.wrapping_sub(clearance) as u32)
                    as i32
                    >> 15;
                self.selector93_add_y(actor, correction as u16, 0x0044_0367)?;
            } else if clearance > 1500 {
                let correction = (request.microseconds >> 4)
                    .wrapping_mul(clearance.wrapping_sub(1500) as u32)
                    as i32
                    >> 15;
                self.selector93_add_y(actor, (correction as u16).wrapping_neg(), 0x0044_0386)?;
            }

            let turn_limit = (request.microseconds / 160) as i32;
            let local = self.actor(actor)?.rotating.basis.inverse_words(delta);
            let ratio = q31_divide(i32::from(local[0]) as u32, distance as u32);
            let mut angle = inverse_sine_q31(ratio as i32, request.inverse_sine)?;
            if local[2] < 0 {
                angle = if angle > 0 {
                    angle.wrapping_add(0x4000)
                } else {
                    -0x4000
                };
            }
            angle = angle.clamp(-turn_limit, turn_limit);
            self.rotate_visual(actor, RotationPlane::Xz, angle, request.math)?;
            (distance / 4).wrapping_add(500).min(3000)
        } else {
            allocation_null_speed.min(3000)
        };
        let basis = self.actor(actor)?.rotating.basis;
        let current_axis = std::array::from_fn(|axis| f32::from_bits(basis.words[4 + axis]));
        let desired_axis = *request.up.ok_or(Selector93UpdateError::MissingUp)?;
        self.apply_actor_angular_response(
            actor,
            ActorAngularResponseRequest {
                current_axis,
                desired_axis,
                gain: (request.microseconds >> 3) as i32,
            },
        )?;

        let speed = self.selector93(actor)?.dword_144;
        let difference = desired_speed.wrapping_sub(speed);
        if difference != 0 {
            let correction =
                ((request.microseconds >> 4).wrapping_mul(difference as u32) as i32 >> 15) as i16;
            self.selector93_mut(actor)?.dword_144 = speed.wrapping_add(i32::from(correction));
        }
        //00440474 reuses the consumed native delta argument for this signed
        //LOWWORD; EBX retained original delta for physics. No Rust stack claim.
        let speed = f64::from(self.selector93(actor)?.dword_144 as i16);
        let basis = self.actor(actor)?.rotating.basis;
        let forward: [f32; 3] = std::array::from_fn(|axis| f32::from_bits(basis.words[7 + axis]));
        for (axis, component) in forward.into_iter().enumerate() {
            self.actor_mut(actor)?.words_8e_92[axis] =
                super::super::derived::native_trunc_low32(speed * f64::from(component)) as u16;
        }
        self.tick_actor_physics(
            actor,
            ActorPhysicsRequest {
                microseconds: request.microseconds,
                mask: 0x18,
                sine_quarter: request.sine_quarter,
            },
        )?;
        Ok(())
    }

    /// The current pool is acquired twice. Capture each XYZ before its own
    /// service; capture velocity only after the second one has returned.
    fn selector93_clearance<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<i32, Selector93UpdateError<H::Error>> {
        let first = self.selector93_position(actor, 0x0044_02ae)?;
        let terrain = host
            .terrain(self, actor)
            .map_err(Selector93UpdateError::Terrain)?;
        let first_height = terrain.height_at([first[0], first[2]]);
        let first_clearance = i32::from(first[1] as i16).wrapping_sub(i32::from(first_height));
        let second = self.selector93_position(actor, 0x0044_02df)?;
        let terrain = host
            .terrain(self, actor)
            .map_err(Selector93UpdateError::Terrain)?;
        let velocity = self.actor(actor)?.words_8e_92;
        let second_height = terrain.height_at([
            second[0].wrapping_add(velocity[0]),
            second[2].wrapping_add(velocity[2]),
        ]);
        let second_clearance = i32::from(second[1] as i16).wrapping_sub(i32::from(second_height));
        Ok(first_clearance.min(second_clearance))
    }

    fn selector93_position<E>(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<[u16; 3], Selector93UpdateError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(Selector93UpdateError::MissingPosition { actor, address })
    }

    fn selector93_add_y<E>(
        &mut self,
        actor: ObjectId,
        increment: u16,
        address: u32,
    ) -> Result<(), Selector93UpdateError<E>> {
        let position = self
            .owned_mut(actor)?
            .state
            .base
            .position_words
            .as_mut()
            .ok_or(Selector93UpdateError::MissingPosition { actor, address })?;
        position[1] = position[1].wrapping_add(increment);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
