//! Selector34 Attack entry +F8 = 00433FB0 and update +F4 = 00433CC0 under
//! the actual 00521EAC vtable. Neither runs the common 00429B00/004298A0
//! helper, size or sound policy; both share Selector35's target shape.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::super::{attack::select_attack_source, DerivedActorError, FamilyTargetSlot};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorServiceAllocationRequest, ActorServiceResourceKind, FixedPositionTarget,
        NativeHeapRef, NativeTarget, NativeTargetClass, NativeTargetError, ObjectArena,
        ObjectError, ObjectId, WeightedTargetConstructorNull,
    },
};

#[derive(Debug)]
pub struct Selector34AttackContext<'a> {
    pub rng: &'a mut u32,
    pub constructor_null: WeightedTargetConstructorNull<'a>,
    pub settings_7c: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector34AttackError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("selector34 {actor:?} requires field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("target {reference:?} requires raw focus +08 at {address:08X}")]
    RequiredFocus {
        reference: NativeHeapRef,
        address: u32,
    },
}

fn required(actor: ObjectId, offset: u16, address: u32) -> Selector34AttackError {
    Selector34AttackError::RequiredField {
        actor,
        offset,
        address,
    }
}

/// 00433DE7..E2B: wrapped signed-WORD deltas, DWORD squares and the bit31
/// `isqrt(sum >> 2) << 1` repair.
fn word_distance(a: [u16; 3], b: [u16; 3]) -> u32 {
    let squared = (0..3).fold(0u32, |sum, axis| {
        let delta = i32::from(a[axis].wrapping_sub(b[axis]) as i16);
        sum.wrapping_add(delta.wrapping_mul(delta) as u32)
    });
    if squared & 0x8000_0000 != 0 {
        isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
    } else {
        isqrt_48efc0(squared as i32)
    }
}

impl ObjectArena {
    /// 00433FB0. False returns before any read. True captures `+1F4`, writes
    /// Attack status, and with no source sets `+22E` bit 0. Otherwise it
    /// rearms `+18C` from one draw, clones the unsigned-score winner into
    /// `+148` and marks `+22C` when that clone resolves an actor.
    pub fn change_selector34_attack(
        &mut self,
        actor: ObjectId,
        entering: bool,
        context: &mut Selector34AttackContext<'_>,
    ) -> Result<(), Selector34AttackError> {
        if !entering {
            return Ok(());
        }
        self.selector34_actor(actor)?;
        let preferred_present = self.family428650_actor(actor)?.targets_1d4_1fc[4].is_some();
        self.family428650_actor_mut(actor)?.word_228 = 0x0053_1244;
        if !preferred_present && self.family428650_actor(actor)?.targets_1d4_1fc[0].is_none() {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
            return Ok(());
        }
        self.selector34_attack_rearm(actor, 2_000_000, context.rng)?;
        let selected = self.selector34_attack_source(actor, 0x0043_404b)?;
        let clone = self.clone_native_target(selected, context.constructor_null)?;
        if let Some(clone) = clone {
            self.selector34_entry_clone_carry(clone)?;
        }
        self.derived_actor_mut(actor)?.target_148 = clone;
        let clone = clone.ok_or(required(actor, 0x148, 0x0043_405e))?;
        if self
            .resolve_target_actor(&self.native_target(clone)?)?
            .is_some()
        {
            self.family428650_actor_mut(actor)?.byte_22c = 1;
        }
        Ok(())
    }

    /// 00433CC0. A set `+239` returns at once. Otherwise `+230/+234` are
    /// refreshed from `+14C`; the period either decays (signed `+18C` above
    /// delta with status `+150 >= 3`) or rearms and may switch `+148` to a
    /// refreshed nearer source. Without `+154` a close ship whose `+248` is
    /// below `+240` publishes a velocity-led secondary point; with one, a
    /// distance beyond half `+164` sets `+15C = 1`.
    pub fn update_selector34_attack(
        &mut self,
        actor: ObjectId,
        delta: u32,
        context: &mut Selector34AttackContext<'_>,
    ) -> Result<(), Selector34AttackError> {
        if self.selector34_actor(actor)?.byte_239 != 0 {
            return Ok(());
        }
        let distance = self.selector34_attack_distance(actor, 0x0043_3cd7)?;
        let state = self.selector34_actor_mut(actor)?;
        state.word_230 = 200;
        state.word_234 = if (distance as i32) <= 1000 { 350 } else { 600 };
        let timer = self.family428650_actor(actor)?.words_188_19c[1].ok_or(required(
            actor,
            0x18c,
            0x0043_3d0c,
        ))?;
        let decays = (timer as i32) > (delta as i32)
            && (self
                .derived_actor(actor)?
                .word_150
                .ok_or(required(actor, 0x150, 0x0043_3d16))? as i32)
                >= 3;
        if decays {
            self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_sub(delta));
        } else {
            self.selector34_attack_rearm(actor, 1_000_000, context.rng)?;
            let family = self.family428650_actor(actor)?;
            if family.targets_1d4_1fc[4].is_none() && family.targets_1d4_1fc[0].is_none() {
                self.family428650_actor_mut(actor)?.word_22e |= 1;
            } else {
                self.selector34_attack_rearm(actor, 2_000_000, context.rng)?;
                let selected = self.selector34_attack_source(actor, 0x0043_3dc4)?;
                let current = self.derived_actor(actor)?.target_148.ok_or(required(
                    actor,
                    0x148,
                    0x0043_3dd3,
                ))?;
                let selected_handle = self.native_target(selected)?.handle_04();
                if selected_handle != self.native_target(current)?.handle_04() {
                    let mut target = self.native_target(selected)?;
                    let status = self.refresh_native_target(&mut target, context.settings_7c);
                    self.write_native_target(selected, target)?;
                    if (status? as i32) > 1 {
                        let focus = self.selector34_attack_focus(selected, 0x0043_3de7)?;
                        let position = self.selector34_attack_position(actor)?;
                        let near = word_distance(focus, position).wrapping_mul(4) as i32;
                        let previous = self.selector34_attack_distance(actor, 0x0043_3e39)?;
                        if near < previous.wrapping_mul(3) as i32 {
                            if let Some(current) = self.derived_actor(actor)?.target_148 {
                                self.release_actor_service(current)?;
                            }
                            // A stopped clone retains the released raw pointer.
                            let clone =
                                self.clone_native_target(selected, context.constructor_null)?;
                            self.derived_actor_mut(actor)?.target_148 = clone;
                        }
                    }
                }
            }
        }
        if self.derived_actor(actor)?.target_154.is_some() {
            let half = self.derived_actor(actor)?.word_164 / 2;
            if (self.selector34_attack_distance(actor, 0x0043_3e72)? as i32) > half {
                self.derived_actor_mut(actor)?.word_15c = Some(1);
            }
            return Ok(());
        }
        let local = self
            .selector34_actor(actor)?
            .local_direction_244
            .ok_or(required(actor, 0x248, 0x0043_3e95))?;
        let limit = self.selector34_actor(actor)?.word_240 as i32;
        if i32::from(local[2] as i16) >= limit
            || (self.selector34_attack_distance(actor, 0x0043_3ead)? as i32)
                >= limit.wrapping_mul(2)
        {
            return Ok(());
        }
        let state = self.selector34_actor_mut(actor)?;
        state.word_234 = 750;
        state.word_230 = 500;
        let velocity = self.actor(actor)?.words_8e_92;
        // This call lacks the bit31 repair and sign-extends only AX.
        let squared = [0, 2].into_iter().fold(0u32, |sum, axis| {
            let value = i32::from(velocity[axis] as i16);
            sum.wrapping_add(value.wrapping_mul(value) as u32)
        });
        let divisor = isqrt_48efc0(squared as i32) as u16 as i16 as i32;
        let divisor = if divisor == 0 { 1 } else { divisor };
        let px = self.selector34_attack_position(actor)?[0];
        let rx = (name_entry_rng_next(context.rng) as u16) >> 4;
        let pz = self.selector34_attack_position(actor)?[2];
        let vx = i32::from(self.actor(actor)?.words_8e_92[0] as i16).wrapping_shl(13);
        let x = rx.wrapping_add(px.wrapping_add((vx / divisor) as u16));
        let rz = (name_entry_rng_next(context.rng) as u16) >> 4;
        let vz = i32::from(self.actor(actor)?.words_8e_92[2] as i16).wrapping_shl(13);
        let z = rz.wrapping_add(pz.wrapping_add((vz / divisor) as u16));
        let current =
            self.derived_actor(actor)?
                .target_148
                .ok_or(required(actor, 0x148, 0x0043_3f42))?;
        let y = self.selector34_attack_focus(current, 0x0043_3f42)?[1];
        let target = if let Some(reference) = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(NativeTargetError::from)?
        {
            let handle = context.constructor_null.resolve(self)?;
            self.construct_native_target(
                reference,
                NativeTarget::FixedPosition(FixedPositionTarget::from_point(
                    handle,
                    Some([x, y, z]),
                )),
            )?;
            Some(reference)
        } else {
            None
        };
        self.derived_actor_mut(actor)?.target_154 = target;
        let draw = u32::from(name_entry_rng_next(context.rng) as u16);
        self.derived_actor_mut(actor)?.word_15c = Some((draw + 0x2dc6c) << 4);
        Ok(())
    }

    /// A type-2 clone's unwritten `+08..+0D`, supplied only by the extended
    /// allocation approximation. Type-8 clones copy their source's `+08`.
    fn selector34_entry_clone_carry(
        &mut self,
        clone: NativeHeapRef,
    ) -> Result<(), Selector34AttackError> {
        let Some(point) = self.actor_allocation.selector34_entry_clone_focus() else {
            return Ok(());
        };
        if let NativeTarget::ActorFollow(mut target) = self.native_target(clone)? {
            if target.focus_08.is_none() {
                target.focus_08 = Some(point);
                self.write_native_target(clone, NativeTarget::ActorFollow(target))?;
            }
        }
        Ok(())
    }

    /// Period store, one process draw, then a fresh `+18C` reread.
    fn selector34_attack_rearm(
        &mut self,
        actor: ObjectId,
        period: u32,
        rng: &mut u32,
    ) -> Result<(), Selector34AttackError> {
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(period);
        let draw = (name_entry_rng_next(rng) & 0xffff) << 4;
        let timer = self.family428650_actor(actor)?.words_188_19c[1]
            .expect("period store precedes the pure process draw");
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_add(draw));
        Ok(())
    }

    /// `+1F4` wins only with a strictly larger unsigned `+1F8`.
    fn selector34_attack_source(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<NativeHeapRef, Selector34AttackError> {
        let first = self.family_weighted_target(actor, FamilyTargetSlot::Target1d4)?;
        let preferred = self.family_weighted_target(actor, FamilyTargetSlot::Target1f4)?;
        select_attack_source(first, preferred)
            .and_then(|(pair, _)| pair.target)
            .ok_or(required(actor, 0x1d4, address))
    }

    fn selector34_attack_distance(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<u32, Selector34AttackError> {
        self.derived_actor(actor)?
            .word_14c
            .ok_or(required(actor, 0x14c, address))
    }

    fn selector34_attack_position(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], Selector34AttackError> {
        Ok(self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?)
    }

    fn selector34_attack_focus(
        &self,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<[u16; 3], Selector34AttackError> {
        self.native_target(reference)?
            .raw_focus_08_words()
            .ok_or(Selector34AttackError::RequiredFocus { reference, address })
    }
}

#[cfg(test)]
#[path = "attack/tests.rs"]
mod tests;
