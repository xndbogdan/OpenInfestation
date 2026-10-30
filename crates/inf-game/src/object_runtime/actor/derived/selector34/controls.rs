//! 0043429C..004349D3: refreshed targets, terrain clearance, firing and
//! signed control arithmetic. The enclosing update owns prefix and suffix.

use super::super::kinematics::{divide, ftol};
use crate::object_runtime::{
    ActorWeaponFireContext, ActorWeaponFireError, ActorWeaponFireMode, ActorWeaponFireRequest,
    DerivedActorError, NativeHeapRef, NativeTargetQueryError, ObjectArena, ObjectError, ObjectId,
    Selector35ClearanceHost,
};
use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

/// Signed control words produced by 0043429C.. and consumed by 00434C10.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selector34MotionControls {
    pub pitch: i32,
    pub yaw: i32,
    pub thrust: i32,
}

pub struct Selector34ControlsRequest<'a, H> {
    pub microseconds: u32,
    pub settings_7c: u32,
    pub firing: ActorWeaponFireContext<'a>,
    pub clearance: &'a mut H,
}

#[derive(Debug, Error)]
pub enum Selector34ControlsError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Query(#[from] NativeTargetQueryError),
    #[error(transparent)]
    Motion(#[from] crate::object_runtime::ActorMotionError),
    #[error(transparent)]
    Firing(#[from] ActorWeaponFireError),
    #[error("selector34 {actor:?} requires field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("target {reference:?} requires cached point/velocity at {address:08X}")]
    RequiredTarget {
        reference: NativeHeapRef,
        address: u32,
    },
    #[error("selector34 clearance requires sparse record at {grid:?}")]
    RequiredSparseRecord { grid: [u16; 2] },
    #[error("selector34 clearance service failed: {0}")]
    Clearance(E),
    #[error("actor {actor:?} requires behavior entry {index}; count {entries}")]
    RequiredBehaviorEntry {
        actor: ObjectId,
        index: i8,
        entries: usize,
    },
}

impl ObjectArena {
    pub fn tick_selector34_controls<H: Selector35ClearanceHost>(
        &mut self,
        actor: ObjectId,
        mut request: Selector34ControlsRequest<'_, H>,
    ) -> Result<Selector34MotionControls, Selector34ControlsError<H::Error>> {
        self.selector34_actor(actor)?;
        let mut controls = Selector34MotionControls::default();
        let Some(captured) = self.derived_actor(actor)?.target_148 else {
            return Ok(controls);
        };
        let mut selected = captured;
        let mut target = self.native_target(captured)?;
        let status = self.refresh_native_target(&mut target, request.settings_7c);
        self.write_native_target(captured, target)?;
        let status = status?;
        self.derived_actor_mut(actor)?.word_150 = Some(status);
        let current =
            self.derived_actor(actor)?
                .target_148
                .ok_or(field(actor, 0x148, 0x0043_42b3))?;
        let mut focus = self.selector34_focus(current, 0x0043_42db)?;
        if self.native_target(current)?.kind_word() == 2
            && self.family428650_actor(actor)?.byte_22c == 0
            && status as i32 > 0
        {
            // Three actual virtual+08 reads, with current148 reread between
            // calls. The cached XYZ was captured before the first call.
            for axis in 0..3 {
                let current = self.derived_actor(actor)?.target_148.ok_or(field(
                    actor,
                    0x148,
                    0x0043_42ee,
                ))?;
                let velocity = self
                    .native_target(current)?
                    .velocity_words(request.firing.process_velocity)?
                    .ok_or(Selector34ControlsError::RequiredTarget {
                        reference: current,
                        address: 0x0043_42eb,
                    })?;
                focus[axis] = focus[axis].wrapping_add(velocity[axis]);
            }
        }
        let position = self.selector34_position(actor)?;
        let mut world = std::array::from_fn(|n| focus[n].wrapping_sub(position[n]) as i16);
        let mut distance = vector_distance(world);
        self.derived_actor_mut(actor)?.word_14c = Some(distance);
        if let Some(secondary) = self.derived_actor(actor)?.target_154 {
            let mut target = self.native_target(secondary)?;
            let status = self.refresh_native_target(&mut target, request.settings_7c);
            self.write_native_target(secondary, target)?;
            status?;
            let current =
                self.derived_actor(actor)?
                    .target_154
                    .ok_or(field(actor, 0x154, 0x0043_43ae))?;
            let focus = self.selector34_focus(current, 0x0043_43b4)?;
            let position = self.selector34_position(actor)?;
            world = std::array::from_fn(|n| focus[n].wrapping_sub(position[n]) as i16);
            distance = vector_distance(world);
            self.derived_actor_mut(actor)?.word_158 = Some(distance);
            // +244 is written even when the later allocator-carried timer
            // is unavailable, expired, or the secondary lies inside radius.
            self.selector34_inverse_direction(actor, world)?;
            let timer =
                self.derived_actor(actor)?
                    .word_15c
                    .ok_or(field(actor, 0x15c, 0x0043_443a))?;
            if timer as i32 > request.microseconds as i32
                && distance as i32 > i32::from(self.object(actor)?.base.word_28)
            {
                self.derived_actor_mut(actor)?.word_15c =
                    Some(timer.wrapping_sub(request.microseconds));
                selected = self.derived_actor(actor)?.target_154.ok_or(field(
                    actor,
                    0x154,
                    0x0043_4454,
                ))?;
            } else {
                if let Some(current) = self.derived_actor(actor)?.target_154 {
                    self.release_actor_service(current)?;
                }
                self.derived_actor_mut(actor)?.target_154 = None;
                // Computed secondary world/distance survive expiry, while
                // selected altitude still refers to captured primary148.
            }
        }
        let offset = if self.selector34_actor(actor)?.byte_238 == 0 {
            self.selector34_actor(actor)?.word_230 as i32
        } else {
            self.selector34_clearance(actor, selected, request.clearance)?
        };
        let divisor;
        if self.selector34_actor(actor)?.byte_239 != 0 {
            let basis = self.actor(actor)?.rotating.basis;
            let world = [
                ftol(f64::from(f32::from_bits(basis.words[7])) * 2048.0) as i16,
                4000,
                ftol(f64::from(f32::from_bits(basis.words[9])) * 2048.0) as i16,
            ];
            self.selector34_inverse_direction(actor, world)?;
            controls.thrust = if unordered_or_negative(f32::from_bits(basis.words[8])) {
                0
            } else {
                65535
            };
            divisor = 4000;
        } else {
            world[1] = (world[1] as u16).wrapping_add(offset as u16) as i16;
            let local = self.selector34_inverse_direction(actor, world)?;
            world[1] = (world[1] as u16).wrapping_sub(offset as u16) as i16;
            let z = i32::from(local[2]);
            controls.thrust = if z.wrapping_mul(2) > distance as i32 {
                65535
            } else if distance == 0 {
                0
            } else {
                divide(z.wrapping_mul(z) >> 1, distance as i32, 0x0043_4855)?
            };
            if self.derived_actor(actor)?.behavior_140.is_some()
                && (distance as i32) < self.derived_actor(actor)?.word_164
                && self.family428650_actor(actor)?.byte_22c != 0
                && self.derived_actor(actor)?.word_150 == Some(3)
            {
                let behavior = self.derived_actor(actor)?.behavior_140.as_ref().unwrap();
                let index = behavior.byte_10 as i8;
                let entry = behavior.entries_18.get(index as usize).ok_or(
                    Selector34ControlsError::RequiredBehaviorEntry {
                        actor,
                        index,
                        entries: behavior.entries_18.len(),
                    },
                )?;
                if entry[5] == 0
                    && entry[0] != 0
                    && entry[7] < 6_000_000
                    && z > 0
                    && divide(i32::from(local[0]).wrapping_mul(2), z, 0x0043_48b9)? == 0
                    && divide(i32::from(local[1]), z, 0x0043_48c7)? == 0
                {
                    self.fire_actor_weapon(
                        actor,
                        ActorWeaponFireRequest {
                            target: self.derived_actor(actor)?.target_148,
                            distance,
                            world,
                            microseconds: request.microseconds,
                            mode: ActorWeaponFireMode::Predictive { publish_lead: true },
                        },
                        &mut request.firing,
                    )?;
                }
            }
            if distance == 0 {
                return Ok(controls);
            }
            divisor = distance as i32;
        }
        let local = self
            .selector34_actor(actor)?
            .local_direction_244
            .ok_or(field(actor, 0x244, 0x0043_48f9))?
            .map(|v| v as i16);
        if local[2] <= 0 && self.selector34_actor(actor)?.byte_239 == 0 {
            controls.yaw = if local[0] > 0 { 32768 } else { -32768 };
            let divisor = divisor.min(1500);
            controls.pitch = pitch(local[1], divisor, 12, 4096, 0x0043_4968)?;
        } else {
            controls.yaw = divide(i32::from(local[0]).wrapping_shl(15), divisor, 0x0043_497a)?;
            controls.pitch = pitch(local[1], divisor.min(2000), 15, 32767, 0x0043_49c1)?;
        }
        Ok(controls)
    }

    fn selector34_inverse_direction<E>(
        &mut self,
        actor: ObjectId,
        world: [i16; 3],
    ) -> Result<[i16; 3], Selector34ControlsError<E>> {
        let local = self.actor(actor)?.rotating.basis.inverse_words(world);
        let words = local.map(|v| v as u16);
        let state = self.selector34_actor_mut(actor)?;
        state.local_direction_244 = Some(words);
        state.words_244_248[0] = Some(u32::from(words[0]) | (u32::from(words[1]) << 16));
        // Native writes only LOWORD+248. Its untouched upper WORD remains
        // unknown unless a prior full DWORD producer established it.
        if let Some(old) = state.words_244_248[1] {
            state.words_244_248[1] = Some((old & 0xffff_0000) | u32::from(words[2]));
        }
        Ok(local)
    }

    fn selector34_clearance<H: Selector35ClearanceHost>(
        &mut self,
        actor: ObjectId,
        selected: NativeHeapRef,
        host: &mut H,
    ) -> Result<i32, Selector34ControlsError<H::Error>> {
        let velocity = self.actor(actor)?.words_8e_92;
        let speed = isqrt_48efc0(
            square(velocity[0] as i16).wrapping_add(square(velocity[2] as i16)) as i32,
        ) as u16 as i16 as i32;
        let span = if speed <= 512 { 3 } else { (speed >> 8) + 1 }.wrapping_mul(128) as u16;
        let position = self.selector34_position(actor)?;
        let start = [
            position[0]
                .wrapping_add((i32::from(velocity[0] as i16) / 2) as u16)
                .wrapping_sub(span),
            position[2]
                .wrapping_add((i32::from(velocity[2] as i16) / 2) as u16)
                .wrapping_sub(span),
        ];
        let end = start.map(|v| v.wrapping_add(span.wrapping_mul(2)));
        let mut highest = -5000i32;
        let mut x = start[0];
        while x != end[0] {
            let mut z = start[1];
            while z != end[1] {
                let mask = host.fraction_mask().units() - 1;
                let grid = [(x >> 7) & !(mask >> 7), (z >> 7) & !(mask >> 7)];
                let raw = host
                    .terrain(self, actor)
                    .map_err(Selector34ControlsError::Clearance)?
                    .cell_at_grid(grid)
                    .height_i8();
                let mut height =
                    (i32::from(raw).wrapping_shl(host.height_shift() & 31) as u16 as i16) as i32;
                let mask = host.fraction_mask().units() - 1;
                let grid = [(x >> 7) & !(mask >> 7), (z >> 7) & !(mask >> 7)];
                let terrain = host
                    .terrain(self, actor)
                    .map_err(Selector34ControlsError::Clearance)?;
                let placement = if terrain.cell_at_grid(grid).has_sparse_record() {
                    terrain
                        .sparse_record_at_grid(grid)
                        .ok_or(Selector34ControlsError::RequiredSparseRecord { grid })?
                        .1
                        .resource_lookup
                        .raw()
                } else {
                    host.fallback_placement()
                };
                if placement != 0 {
                    if let Some(radius) = host
                        .furniture_model_radius(self, actor, placement)
                        .map_err(Selector34ControlsError::Clearance)?
                    {
                        height = height.wrapping_add(i32::from(radius));
                    }
                }
                highest = highest.max(height);
                z = z.wrapping_add(host.spacing().units());
            }
            x = x.wrapping_add(host.spacing().units());
        }
        let zy = f32::from_bits(self.actor(actor)?.rotating.basis.words[8]);
        let mut y = i32::from(self.selector34_position(actor)?[1] as i16);
        let vy = i32::from(self.actor(actor)?.words_8e_92[1] as i16);
        if unordered_or_negative(zy) {
            highest = highest.wrapping_add(128);
            y = y.wrapping_add(ftol(f64::from(zy) * 512.0));
            if vy < 0 {
                y = y.wrapping_add(vy.wrapping_mul(2));
            }
        } else {
            highest = highest.wrapping_add(64);
            if vy < 0 {
                y = y.wrapping_add(vy);
            }
        }
        if y < highest {
            self.selector34_actor_mut(actor)?.byte_239 = 1;
            self.owned_mut(actor)?.state.base.flags &= !0x0400_0000;
            return Ok(0); // Offset is dead on the following avoidance branch.
        }
        if y.wrapping_sub(highest) > 256 {
            self.owned_mut(actor)?.state.base.flags |= 0x0400_0000;
        }
        let selected_y = i32::from(self.selector34_focus(selected, 0x0043_47c4)?[1] as i16);
        Ok((self.selector34_actor(actor)?.word_234 as i32)
            .wrapping_sub(selected_y)
            .wrapping_add(highest)
            .max(self.selector34_actor(actor)?.word_230 as i32))
    }

    fn selector34_position<E>(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], Selector34ControlsError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(field(actor, 0x32, 0x0043_436b))
    }
    fn selector34_focus<E>(
        &self,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<[u16; 3], Selector34ControlsError<E>> {
        self.native_target(reference)?
            .raw_focus_08_words()
            .ok_or(Selector34ControlsError::RequiredTarget { reference, address })
    }
}
fn field<E>(actor: ObjectId, offset: u16, address: u32) -> Selector34ControlsError<E> {
    Selector34ControlsError::RequiredField {
        actor,
        offset,
        address,
    }
}
fn square(v: i16) -> u32 {
    i32::from(v).wrapping_mul(i32::from(v)) as u32
}
fn vector_distance(v: [i16; 3]) -> u32 {
    let sum = v
        .into_iter()
        .fold(0u32, |sum, v| sum.wrapping_add(square(v)));
    if sum & 0x8000_0000 != 0 {
        isqrt_48efc0((sum >> 2) as i32).wrapping_shl(1)
    } else {
        isqrt_48efc0(sum as i32)
    }
}
fn unordered_or_negative(value: f32) -> bool {
    value.is_nan() || value < 0.0
}
fn pitch(
    value: i16,
    divisor: i32,
    shift: u32,
    limit: i32,
    address: u32,
) -> Result<i32, crate::object_runtime::ActorMotionError> {
    if i32::from(value).abs() > divisor {
        Ok(if value > 0 { -limit } else { limit })
    } else {
        Ok(divide(i32::from(value).wrapping_shl(shift), divisor, address)?.wrapping_neg())
    }
}
