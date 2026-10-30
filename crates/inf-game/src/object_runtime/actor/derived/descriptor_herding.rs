//! Herding descriptor 0043FC00/0043FC20 -> direct family bodies 00427A70 and
//! 00427AA0, with group scan 00427E70 publishing through 00427390 into +1E4.
//! The calls are direct, so every profile that lists them shares one owner.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::FamilyTargetSlot;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        NativeTargetError, ObjectArena, ObjectError, ObjectId, WeightedTargetConstructorNull,
        WeightedTargetReferent, WeightedTargetRequest,
    },
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyHerdingError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("actor {actor:?} requires unwritten +{offset:03X} at {address:08X}")]
    Uninitialized {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("actor {0:?} has null +148 at 00427B0A")]
    MissingCurrentTarget(ObjectId),
    #[error("herd target {0:?} has no point at 00427B10")]
    MissingHerdPoint(crate::object_runtime::NativeHeapRef),
}

impl ObjectArena {
    /// 00427A70: entry publishes `Herding` and clones a nonnull `+1E4` into
    /// `+148`; false returns before any read. The dispatcher's earlier `+10C`
    /// already deleted the previous `+148`.
    pub fn change_family_herding(
        &mut self,
        actor: ObjectId,
        entering: bool,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<(), FamilyHerdingError> {
        if !entering {
            return Ok(());
        }
        // 00427A7B captures +1E4 before 00427A81 publishes the status.
        let source = self.family428650_actor(actor)?.targets_1d4_1fc[2];
        self.family428650_actor_mut(actor)?.word_228 = 0x0053_11dc;
        let Some(reference) = source else {
            return Ok(());
        };
        let clone = self.clone_native_target(reference, constructor_null)?;
        self.derived_actor_mut(actor)?.target_148 = clone;
        Ok(())
    }

    /// 00427AA0. A signed `+18C` above delta with status `+150 >= 3` only
    /// decrements. Otherwise the period and one process draw rearm `+18C`,
    /// the group scan republishes `+1E4`, and a kind-8 `+148` takes that
    /// point; an empty slot sets `+22E` bit 0 instead.
    pub fn update_family_herding(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        process_rng: &mut u32,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<(), FamilyHerdingError> {
        let timer = self.herding_word(actor, 1, 0x18c, 0x0042_7aa8)?;
        if (timer as i32) > (microseconds as i32) {
            let status =
                self.derived_actor(actor)?
                    .word_150
                    .ok_or(FamilyHerdingError::Uninitialized {
                        actor,
                        offset: 0x150,
                        address: 0x0042_7ab2,
                    })?;
            if (status as i32) >= 3 {
                self.family428650_actor_mut(actor)?.words_188_19c[1] =
                    Some(timer.wrapping_sub(microseconds));
                return Ok(());
            }
        }
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_add(250_000));
        let draw = name_entry_rng_next(process_rng) & 0xffff;
        let timer = self.herding_word(actor, 1, 0x18c, 0x0042_7ad8)?;
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_add(draw << 2));
        self.scan_family_herd(actor, constructor_null)?;
        let Some(herd) = self.family428650_actor(actor)?.targets_1d4_1fc[2] else {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
            return Ok(());
        };
        let current = self
            .derived_actor(actor)?
            .target_148
            .ok_or(FamilyHerdingError::MissingCurrentTarget(actor))?;
        let mut target = self.native_target(current)?;
        if target.kind_word() == 8 {
            let point = self
                .native_target(herd)?
                .raw_focus_08_words()
                .ok_or(FamilyHerdingError::MissingHerdPoint(herd))?;
            target.set_focus_08_words(point);
            self.write_native_target(current, target)?;
        }
        Ok(())
    }

    /// 00427E70 walks the live actor list for the same `+B4`, resource WORD
    /// `+30` and `(flags & A0) == 20`. Members within unsigned `+164²` add
    /// their signed offset plus velocity. More than one member yields the
    /// signed mean; `20000h < |mean|² < +164²` publishes self plus mean with
    /// score `isqrt(|mean|²)`, anything else publishes the NULL point.
    fn scan_family_herd(
        &mut self,
        actor: ObjectId,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<bool, FamilyHerdingError> {
        let radius = self.derived_actor(actor)?.word_164;
        let radius_squared = radius.wrapping_mul(radius) as u32;
        let classification = self.actor(actor)?.word_b4;
        let resource = self.object(actor)?.base.resource_word;
        let mut sums = [0i32; 3];
        let mut count = 0i32;
        // No callback runs inside the walk, so the list cannot change.
        for candidate in self.actor_list().to_vec() {
            // A stopped constructor prefix is not yet a live list member.
            if self.construction_is_stopped(candidate)? {
                continue;
            }
            if self.actor(candidate)?.word_b4 != classification
                || self.object(candidate)?.base.resource_word != resource
                || self.object(candidate)?.base.flags & 0xa0 != 0x20
            {
                continue;
            }
            let position = self.herding_position(candidate)?;
            let own = self.herding_position(actor)?;
            let delta: [i32; 3] =
                std::array::from_fn(
                    |axis| i32::from(position[axis].wrapping_sub(own[axis]) as i16),
                );
            let squared = delta.into_iter().fold(0u32, |sum, value| {
                sum.wrapping_add(value.wrapping_mul(value) as u32)
            });
            if squared >= radius_squared {
                continue;
            }
            let velocity = self.actor(candidate)?.words_8e_92;
            for axis in 0..3 {
                sums[axis] = sums[axis]
                    .wrapping_add(delta[axis])
                    .wrapping_add(i32::from(velocity[axis] as i16));
            }
            count = count.wrapping_add(1);
        }
        let mut point = None;
        let mut score = 0;
        if count > 1 {
            let means = sums.map(|sum| sum.wrapping_div(count));
            let squared = [1, 2, 0].into_iter().fold(0u32, |sum, axis| {
                sum.wrapping_add(means[axis].wrapping_mul(means[axis]) as u32)
            });
            if squared < radius_squared && squared > 0x20000 {
                let own = self.herding_position(actor)?;
                point = Some(std::array::from_fn(|axis| {
                    own[axis].wrapping_add(means[axis] as u16)
                }));
                score = isqrt_48efc0(squared as i32);
            }
        }
        Ok(self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1e4,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Point(point),
                score,
                constructor_null,
            },
        )?)
    }

    fn herding_word(
        &self,
        actor: ObjectId,
        index: usize,
        offset: u16,
        address: u32,
    ) -> Result<u32, FamilyHerdingError> {
        self.family428650_actor(actor)?.words_188_19c[index].ok_or(
            FamilyHerdingError::Uninitialized {
                actor,
                offset,
                address,
            },
        )
    }

    fn herding_position(&self, actor: ObjectId) -> Result<[u16; 3], FamilyHerdingError> {
        Ok(self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?)
    }
}

#[cfg(test)]
#[path = "descriptor_herding/tests.rs"]
mod tests;
