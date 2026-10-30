//! 0043E766..0043EE61 after normal virtual+D8. Live target refresh, native
//! clearance grid and fire queue precede the separate43EF60 motion owner.
use super::super::kinematics::{divide, ftol};
use super::Selector35MotionControls;
use crate::{
    object_runtime::{
        ActorTerrainHost, ActorWeaponFireContext, ActorWeaponFireError, ActorWeaponFireMode,
        ActorWeaponFireRequest, DerivedActorError, NativeHeapRef, NativeTargetQueryError,
        ObjectArena, ObjectError, ObjectId,
    },
    terrain_runtime::TerrainSpacing,
};
use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

pub trait Selector35ClearanceHost: ActorTerrainHost {
    /// Pure reads from the retained terrain sampling policy. The admitted H12
    /// getter does not replace these process values while a grid is scanned.
    fn spacing(&mut self) -> TerrainSpacing;
    fn fraction_mask(&mut self) -> TerrainSpacing;
    fn height_shift(&mut self) -> u32;
    fn fallback_placement(&self) -> u8;
    /// Reached nonzero BYTE record+3: H11 signed resource, then optional H8
    /// model. Missing/negative/out-of-range model is a genuine normal None.
    fn furniture_model_radius(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        placement: u8,
    ) -> Result<Option<u16>, Self::Error>;
}
pub struct Selector35TailRequest<'a, H> {
    pub microseconds: u32,
    pub settings_7c: u32,
    pub firing: ActorWeaponFireContext<'a>,
    pub clearance: &'a mut H,
}
#[derive(Debug, Error)]
pub enum Selector35TailError<E> {
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
    #[error("selector35 {actor:?} requires field +{offset:X} at {address:08X}")]
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
    #[error("selector35 clearance requires sparse record at {grid:?}")]
    RequiredSparseRecord { grid: [u16; 2] },
    #[error("selector35 clearance service failed: {0}")]
    Clearance(E),
    #[error("actor {actor:?} requires behavior entry {index}; count {entries}")]
    RequiredBehaviorEntry {
        actor: ObjectId,
        index: i8,
        entries: usize,
    },
}
impl ObjectArena {
    pub fn tick_selector35_tail<H: Selector35ClearanceHost>(
        &mut self,
        actor: ObjectId,
        mut request: Selector35TailRequest<'_, H>,
    ) -> Result<Selector35MotionControls, Selector35TailError<H::Error>> {
        self.selector35_actor(actor)?;
        let mut controls = Selector35MotionControls::default();
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
                .ok_or(field(actor, 0x148, 0x0043_e77d))?;
        let target = self.native_target(current)?;
        let mut focus = self.selector35_tail_focus(current, 0x0043_e7a4)?;
        if target.kind_word() == 2
            && self.family428650_actor(actor)?.byte_22c == 0
            && status as i32 > 0
        {
            let first = target
                .velocity_words(request.firing.process_velocity)?
                .ok_or(Selector35TailError::RequiredTarget {
                    reference: current,
                    address: 0x0043_e7b1,
                })?;
            focus[1] = focus[1].wrapping_add(self.object(actor)?.base.word_28.wrapping_mul(2));
            focus[0] = focus[0].wrapping_add(first[0].wrapping_mul(2));
            let fresh =
                self.derived_actor(actor)?
                    .target_148
                    .ok_or(field(actor, 0x148, 0x0043_e7cb))?;
            let velocity = self
                .native_target(fresh)?
                .velocity_words(request.firing.process_velocity)?
                .ok_or(Selector35TailError::RequiredTarget {
                    reference: fresh,
                    address: 0x0043_e7d9,
                })?;
            focus[2] = focus[2].wrapping_add(velocity[2].wrapping_mul(2));
        }
        let position = self.selector35_tail_position(actor)?;
        let mut world = std::array::from_fn(|n| focus[n].wrapping_sub(position[n]) as i16);
        let mut distance = vector_distance(world);
        self.derived_actor_mut(actor)?.word_14c = Some(distance);
        if let Some(secondary) = self.derived_actor(actor)?.target_154 {
            let mut target = self.native_target(secondary)?;
            let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
            self.write_native_target(secondary, target)?;
            refreshed?;
            let secondary =
                self.derived_actor(actor)?
                    .target_154
                    .ok_or(field(actor, 0x154, 0x0043_e898))?;
            let focus = self.selector35_tail_focus(secondary, 0x0043_e8b1)?;
            let position = self.selector35_tail_position(actor)?;
            world = std::array::from_fn(|n| focus[n].wrapping_sub(position[n]) as i16);
            distance = vector_distance(world);
            let timer =
                self.derived_actor(actor)?
                    .word_15c
                    .ok_or(field(actor, 0x15c, 0x0043_e907))?;
            self.derived_actor_mut(actor)?.word_158 = Some(distance);
            if (timer as i32) > request.microseconds as i32
                && (distance as i32) > i32::from(self.object(actor)?.base.word_28)
            {
                self.derived_actor_mut(actor)?.word_15c =
                    Some(timer.wrapping_sub(request.microseconds));
                selected = self.derived_actor(actor)?.target_154.ok_or(field(
                    actor,
                    0x154,
                    0x0043_e931,
                ))?;
            } else {
                if let Some(reference) = self.derived_actor(actor)?.target_154 {
                    self.release_actor_service(reference)?;
                }
                self.derived_actor_mut(actor)?.target_154 = None;
                //Native keeps secondary's computed world/distance on expiry,
                //while the captured altitude source remains the primary target.
            }
        }
        let offset = if self.selector35_actor(actor)?.byte_238 == 0 {
            self.selector35_actor(actor)?.word_230 as i32
        } else {
            self.selector35_clearance(actor, selected, request.clearance)?
        };
        if self.selector35_actor(actor)?.byte_250 != 0 {
            controls.vertical = 65535;
            return Ok(controls);
        }
        controls.vertical = i32::from(world[1]).wrapping_add(offset) / 2;
        let horizontal = isqrt_48efc0(square(world[0]).wrapping_add(square(world[2])) as i32) as u16
            as i16 as i32;
        let speed = self.family428650_actor(actor)?.words_204_208[1].ok_or(field(
            actor,
            0x208,
            0x0043_ec73,
        ))? as i32;
        self.selector35_actor_mut(actor)?.word_260 = Some(horizontal as u32);
        let wanted = if speed < 0 {
            speed.wrapping_neg()
        } else {
            horizontal.wrapping_sub(speed) / 2
        };
        controls.horizontal = if wanted < 0 {
            wanted.wrapping_neg()
        } else {
            wanted
        };
        controls.direction_x = if wanted < 0 {
            i32::from(world[0]).wrapping_neg()
        } else {
            i32::from(world[0])
        };
        controls.direction_z = if wanted < 0 {
            i32::from(world[2]).wrapping_neg()
        } else {
            i32::from(world[2])
        };
        let alternate =
            self.selector35_actor(actor)?
                .byte_251
                .ok_or(field(actor, 0x251, 0x0043_eccb))?
                != 0;
        if controls.horizontal < if alternate { 20 } else { 80 } {
            controls.horizontal = 0;
        }
        if self.derived_actor(actor)?.behavior_140.is_some()
            && (distance as i32) < self.derived_actor(actor)?.word_164
            && self.family428650_actor(actor)?.byte_22c != 0
            && self.derived_actor(actor)?.word_150 == Some(3)
        {
            let local = self.actor(actor)?.rotating.basis.inverse_words(world);
            controls.yaw = turn(local, distance, controls.yaw, 0x0043_ed4f)?;
            let behavior = self
                .derived_actor(actor)?
                .behavior_140
                .as_ref()
                .ok_or(field(actor, 0x140, 0x0043_ed74))?;
            let index = behavior.byte_10 as i8;
            let entry = behavior.entries_18.get(index as usize).ok_or(
                Selector35TailError::RequiredBehaviorEntry {
                    actor,
                    index,
                    entries: behavior.entries_18.len(),
                },
            )?;
            if entry[5] == 0
                && entry[0] != 0
                && entry[7] < 6_000_000
                && local[2] > 0
                && i32::from(local[0]).wrapping_mul(2).wrapping_abs() < i32::from(local[2])
                && i32::from(local[1]).wrapping_abs() < i32::from(local[2])
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
        if controls.yaw == 0
            && controls.horizontal > 100
            && self
                .selector35_actor(actor)?
                .byte_251
                .ok_or(field(actor, 0x251, 0x0043_edea))?
                != 0
        {
            let local = self.actor(actor)?.rotating.basis.inverse_words([
                controls.direction_x as u16 as i16,
                0,
                controls.direction_z as u16 as i16,
            ]);
            if local[2] <= 0 {
                controls.horizontal = 0;
            }
            controls.yaw = turn(local, distance, controls.yaw, 0x0043_ee34)?;
        }
        Ok(controls)
    }
    fn selector35_clearance<H: Selector35ClearanceHost>(
        &mut self,
        actor: ObjectId,
        selected: NativeHeapRef,
        host: &mut H,
    ) -> Result<i32, Selector35TailError<H::Error>> {
        let velocity = self.actor(actor)?.words_8e_92;
        let speed = isqrt_48efc0(
            square(velocity[0] as i16).wrapping_add(square(velocity[2] as i16)) as i32,
        ) as u16 as i16 as i32;
        let span = if speed <= 512 { 3 } else { (speed >> 8) + 1 }.wrapping_mul(128) as u16;
        let position = self.selector35_tail_position(actor)?;
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
                let terrain = host
                    .terrain(self, actor)
                    .map_err(Selector35TailError::Clearance)?;
                let raw = terrain.cell_at_grid(grid).height_i8();
                let mut height =
                    (i32::from(raw).wrapping_shl(host.height_shift() & 31) as u16 as i16) as i32;
                let mask = host.fraction_mask().units() - 1;
                let grid = [(x >> 7) & !(mask >> 7), (z >> 7) & !(mask >> 7)];
                let terrain = host
                    .terrain(self, actor)
                    .map_err(Selector35TailError::Clearance)?;
                let placement = if terrain.cell_at_grid(grid).has_sparse_record() {
                    terrain
                        .sparse_record_at_grid(grid)
                        .ok_or(Selector35TailError::RequiredSparseRecord { grid })?
                        .1
                        .resource_lookup
                        .raw()
                } else {
                    host.fallback_placement()
                };
                if placement != 0 {
                    if let Some(radius) = host
                        .furniture_model_radius(self, actor, placement)
                        .map_err(Selector35TailError::Clearance)?
                    {
                        height = height.wrapping_add(i32::from(radius));
                    }
                }
                highest = highest.max(height);
                z = z.wrapping_add(host.spacing().units());
            }
            x = x.wrapping_add(host.spacing().units());
        }
        highest = highest.wrapping_add(64);
        let vy = i32::from(self.actor(actor)?.words_8e_92[1] as i16);
        let y = i32::from(self.selector35_tail_position(actor)?[1] as i16).wrapping_add(vy.min(0));
        if y < highest {
            self.selector35_actor_mut(actor)?.byte_250 = 1;
            self.owned_mut(actor)?.state.base.flags &= !0x0400_0000;
            return Ok(0); //Offset is dead once250 is set; no invented motion input.
        }
        if y.wrapping_sub(highest) > 256 {
            self.owned_mut(actor)?.state.base.flags |= 0x0400_0000;
        }
        let basis = self.actor(actor)?.rotating.basis;
        let tilt = ftol(f64::from(f32::from_bits(basis.words[5])) * -1024.0);
        let selected_y = i32::from(self.selector35_tail_focus(selected, 0x0043_ec22)?[1] as i16);
        let offset = tilt
            .wrapping_sub(selected_y)
            .wrapping_add(self.selector35_actor(actor)?.word_234 as i32)
            .wrapping_add(1024)
            .wrapping_add(highest);
        Ok(offset.max(self.selector35_actor(actor)?.word_230 as i32))
    }
    fn selector35_tail_position<E>(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], Selector35TailError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(field(actor, 0x32, 0x0043_e824))
    }
    fn selector35_tail_focus<E>(
        &self,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<[u16; 3], Selector35TailError<E>> {
        self.native_target(reference)?
            .raw_focus_08_words()
            .ok_or(Selector35TailError::RequiredTarget { reference, address })
    }
}
fn field<E>(actor: ObjectId, offset: u16, address: u32) -> Selector35TailError<E> {
    Selector35TailError::RequiredField {
        actor,
        offset,
        address,
    }
}
fn square(v: i16) -> u32 {
    i32::from(v).wrapping_mul(i32::from(v)) as u32
}
fn vector_distance(v: [i16; 3]) -> u32 {
    let s = v.into_iter().fold(0u32, |s, v| s.wrapping_add(square(v)));
    if s & 0x8000_0000 != 0 {
        isqrt_48efc0((s >> 2) as i32).wrapping_shl(1)
    } else {
        isqrt_48efc0(s as i32)
    }
}
fn turn(
    local: [i16; 3],
    distance: u32,
    old: i32,
    address: u32,
) -> Result<i32, crate::object_runtime::ActorMotionError> {
    if local[2] <= 0 {
        Ok(if local[0] > 0 { 32768 } else { -32768 })
    } else if distance != 0 {
        divide(
            i32::from(local[0]).wrapping_shl(15),
            distance as i32,
            address,
        )
    } else {
        Ok(old)
    }
}

#[cfg(test)]
mod tests;
