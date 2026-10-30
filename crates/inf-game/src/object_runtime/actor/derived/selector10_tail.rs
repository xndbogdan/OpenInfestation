//! Native4248DD..424BBF of the family's 00424830, entered only after normal
//! +D8: selector 10's and the family's own class's (selector 9).
//! The existing family prefix already ran17A and WeaponTick; this tail does
//! not repeat either. Geometry/target/error writes are retained in native order.
use super::kinematics::{divide, ActorMotionError};
use super::selector12_tail::TailAvoidance;
use super::{
    DerivedActorError, FamilyAvoidanceError, FamilyObstacleScanError, FamilyTerrainProbeMissHost,
    Selector12TailProbeError,
};
use crate::object_runtime::{NativeHeapRef, ObjectArena, ObjectError, ObjectId};
use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

pub struct Selector10TailPrefixRequest<'a, H> {
    pub microseconds: u32,
    pub settings_7c: u32,
    pub terrain: &'a mut H,
    pub firing: crate::object_runtime::ActorWeaponFireContext<'a>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector10TailNextPhase {
    Motion { controls: [i32; 2] },
}
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector10TailPrefixError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Division(#[from] ActorMotionError),
    #[error("actor {actor:?} requires field+{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("target {reference:?} requires focus08 at {address:08X}")]
    RequiredTargetFocus {
        reference: NativeHeapRef,
        address: u32,
    },
    #[error("actor {actor:?} requires behavior entry {index}; current {entries}")]
    BehaviorEntry {
        actor: ObjectId,
        index: i8,
        entries: usize,
    },
    #[error(transparent)]
    Firing(#[from] crate::object_runtime::ActorWeaponFireError),
    #[error("native class10 avoidance failed: {0}")]
    Avoidance(FamilyAvoidanceError<FamilyObstacleScanError<Selector12TailProbeError<E>>>),
}
impl ObjectArena {
    pub fn tick_selector10_tail_prefix<H: FamilyTerrainProbeMissHost>(
        &mut self,
        actor: ObjectId,
        mut request: Selector10TailPrefixRequest<'_, H>,
    ) -> Result<Selector10TailNextPhase, Selector10TailPrefixError<H::Error>> {
        self.require_family_update_424830(actor)?;
        let mut controls = [0; 2];
        if let Some(captured) = self.derived_actor(actor)?.target_148 {
            let mut target = self.native_target(captured)?;
            let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
            self.write_native_target(captured, target)?;
            let status = refreshed?;
            //4248F1 reads fresh154 before4248F7 publishes150.
            let secondary = self.derived_actor(actor)?.target_154;
            self.derived_actor_mut(actor)?.word_150 = Some(status);
            let (local, distance, forward) = if let Some(captured) = secondary {
                let mut target = self.native_target(captured)?;
                let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
                self.write_native_target(captured, target)?;
                refreshed?;
                let fresh = self.derived_actor(actor)?.target_154.ok_or(
                    Selector10TailPrefixError::RequiredField {
                        actor,
                        offset: 0x154,
                        address: 0x42490a,
                    },
                )?;
                let (world, distance) =
                    self.selector10_geometry::<H::Error>(actor, fresh, 0x424923)?;
                self.derived_actor_mut(actor)?.word_158 = Some(distance);
                let local = self.actor(actor)?.rotating.basis.inverse_words(world);
                let forward = i32::from(local[2]) / 2;
                let timer = self.derived_actor(actor)?.word_15c.ok_or(
                    Selector10TailPrefixError::RequiredField {
                        actor,
                        offset: 0x15c,
                        address: 0x4249a2,
                    },
                )?;
                if (timer as i32) > request.microseconds as i32
                    && (self.derived_actor(actor)?.word_158.unwrap() as i32)
                        > i32::from(self.object(actor)?.base.word_28)
                {
                    self.family4236d0_actor_mut(actor)?.byte_260 = 1;
                    self.derived_actor_mut(actor)?.word_15c =
                        Some(timer.wrapping_sub(request.microseconds));
                } else {
                    if let Some(fresh) = self.derived_actor(actor)?.target_154 {
                        self.release_actor_service(fresh)?;
                    }
                    self.derived_actor_mut(actor)?.target_154 = None;
                }
                (local, distance, forward)
            } else {
                let fresh = self.derived_actor(actor)?.target_148.ok_or(
                    Selector10TailPrefixError::RequiredField {
                        actor,
                        offset: 0x148,
                        address: 0x4249eb,
                    },
                )?;
                let (world, distance) =
                    self.selector10_geometry::<H::Error>(actor, fresh, 0x424a04)?;
                self.derived_actor_mut(actor)?.word_14c = Some(distance);
                let local = self.actor(actor)?.rotating.basis.inverse_words(world);
                let spacing = self.family428650_actor(actor)?.words_204_208[1].ok_or(
                    Selector10TailPrefixError::RequiredField {
                        actor,
                        offset: 0x208,
                        address: 0x424a79,
                    },
                )? as i32;
                let forward = if spacing < 0 {
                    let mut value = divide(
                        spacing.wrapping_mul(i32::from(local[2])),
                        distance as i32,
                        0x424a8e,
                    )?
                    .wrapping_neg();
                    if local[2] < 0 {
                        value = value.wrapping_neg();
                    }
                    value
                } else {
                    i32::from(local[2]).wrapping_sub(spacing) / 2
                };
                self.selector10_fire_gate::<H::Error>(
                    actor,
                    distance,
                    world,
                    local,
                    request.microseconds,
                    &mut request.firing,
                )?;
                (local, distance, forward)
            };
            let turn = if distance == 0 {
                0
            } else {
                let value = divide(
                    i32::from(local[0]).wrapping_shl(14),
                    distance as i32,
                    0x424b63,
                )?;
                if local[2] >= 0 {
                    value
                } else if value > 0 {
                    0x8000_i32.wrapping_sub(value)
                } else {
                    (-0x8000_i32).wrapping_sub(value)
                }
            };
            controls = [turn, forward];
        }
        //424B96 fresh260 precedes fresh148. NULL target bypasses avoidance,
        //not physics/contact or the installed child callback.
        if self.family4236d0_actor(actor)?.byte_260 != 0
            && self.derived_actor(actor)?.target_148.is_some()
        {
            self.adjust_family_avoidance(
                actor,
                &mut controls,
                request.microseconds,
                &mut TailAvoidance {
                    miss: request.terrain,
                },
            )
            .map_err(Selector10TailPrefixError::Avoidance)?;
        }
        Ok(Selector10TailNextPhase::Motion { controls })
    }
    fn selector10_geometry<E>(
        &self,
        actor: ObjectId,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<([i16; 3], u32), Selector10TailPrefixError<E>> {
        let focus = self
            .native_target(reference)?
            .raw_focus_08_words()
            .ok_or(Selector10TailPrefixError::RequiredTargetFocus { reference, address })?;
        let position = self.object(actor)?.base.position_words.ok_or(
            Selector10TailPrefixError::RequiredField {
                actor,
                offset: 0x32,
                address,
            },
        )?;
        let world = std::array::from_fn(|axis| focus[axis].wrapping_sub(position[axis]) as i16);
        let squared = world.iter().fold(0_u32, |sum, &v| {
            sum.wrapping_add(i32::from(v).wrapping_mul(i32::from(v)) as u32)
        });
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        };
        Ok((world, distance))
    }
    fn selector10_fire_gate<E>(
        &mut self,
        actor: ObjectId,
        distance: u32,
        world: [i16; 3],
        local: [i16; 3],
        microseconds: u32,
        firing: &mut crate::object_runtime::ActorWeaponFireContext<'_>,
    ) -> Result<(), Selector10TailPrefixError<E>> {
        if let Some(request) =
            self.selector10_fire_request::<E>(actor, distance, world, local, microseconds)?
        {
            self.fire_actor_weapon(actor, request, firing)?;
        }
        Ok(())
    }
    /// `00424AAF..00424B4A`, the shot at the current target +148, shared by
    /// selectors 9, 10 and 21. None without the +140 behavior, from +164
    /// away (signed), with +22C clear, a refreshed +150 other than 3, the
    /// selected entry still cooling (+14), empty (+0) or at its pressure
    /// limit (+1C, unsigned 6,000,000), or the target outside the cone ahead
    /// (local z > 0, |x| < 2z, |y| < z + 50). `0048D5A0` while the WORD +17A
    /// counts down, else `0048D340`, which publishes its lead.
    fn selector10_fire_request<E>(
        &self,
        actor: ObjectId,
        distance: u32,
        world: [i16; 3],
        local: [i16; 3],
        microseconds: u32,
    ) -> Result<Option<crate::object_runtime::ActorWeaponFireRequest>, Selector10TailPrefixError<E>>
    {
        let Some(behavior) = self.derived_actor(actor)?.behavior_140.as_ref() else {
            return Ok(None);
        };
        if distance as i32 >= self.derived_actor(actor)?.word_164
            || self.family428650_actor(actor)?.byte_22c == 0
            || self.derived_actor(actor)?.word_150 != Some(3)
        {
            return Ok(None);
        }
        let index = behavior.byte_10 as i8;
        let entry = behavior.entries_18.get(index as usize).ok_or(
            Selector10TailPrefixError::BehaviorEntry {
                actor,
                index,
                entries: behavior.entries_18.len(),
            },
        )?;
        if entry[5] != 0
            || entry[0] == 0
            || entry[7] >= 6_000_000
            || local[2] <= 0
            || i32::from(local[0]).wrapping_abs() >= i32::from(local[2]).wrapping_mul(2)
            || i32::from(local[1]).wrapping_abs() >= i32::from(local[2]).wrapping_add(50)
        {
            return Ok(None);
        }
        use crate::object_runtime::{ActorWeaponFireMode, ActorWeaponFireRequest};
        let mode = if self.derived_actor(actor)?.word_17a == 0 {
            ActorWeaponFireMode::Predictive { publish_lead: true }
        } else {
            ActorWeaponFireMode::Scattered
        };
        Ok(Some(ActorWeaponFireRequest {
            target: self.derived_actor(actor)?.target_148,
            distance,
            world,
            microseconds,
            mode,
        }))
    }
}
#[cfg(test)]
mod native_guards;
#[cfg(test)]
pub(super) mod tests;
