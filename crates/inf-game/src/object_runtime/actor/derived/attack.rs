//! Shared descriptor change callback 00429B00. Target clone custody and
//! process RNG are explicit; no family or mission selects fabricated targets.

use thiserror::Error;

use super::FamilyTargetSlot;
use crate::object_runtime::{
    NativeTargetError, NativeWeightedTarget, ObjectArena, ObjectError, ObjectId,
    TargetNullHandleSource, WeightedTargetConstructorNull,
};

pub trait FamilyAttackHost: TargetNullHandleSource {
    type Error;
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// The reached positive +180 branch: native registration/lazy acquisition,
    /// positional 00487F10(65536,65536), then nullable 004923C0 cleanup.
    fn attack_sound(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        slot: u16,
    ) -> Result<(), Self::Error>;
}

/// 00429B00 and 004298A0 choose the same unsigned weighted source. Released
/// pointers participate in presence/score selection; the caller owns the
/// reached live-resource check before its actual dereference.
pub(super) fn select_attack_source(
    first: NativeWeightedTarget,
    preferred: NativeWeightedTarget,
) -> Option<(NativeWeightedTarget, FamilyTargetSlot)> {
    match (
        first.native_pointer_present(),
        preferred.native_pointer_present(),
    ) {
        (true, true) if preferred.score > first.score => {
            Some((preferred, FamilyTargetSlot::Target1f4))
        }
        (true, _) => Some((first, FamilyTargetSlot::Target1d4)),
        (false, true) => Some((preferred, FamilyTargetSlot::Target1f4)),
        (false, false) => None,
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyAttackError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("actor {actor:?} requires its unwritten +{offset:03X} field")]
    Uninitialized { actor: ObjectId, offset: u16 },
    #[error("actor {actor:?} requires unwritten +218 helper field +{offset:02X}")]
    UninitializedHelper { actor: ObjectId, offset: u8 },
    #[error("actor {0:?} received a null target clone before virtual +10")]
    MissingClone(ObjectId),
    #[error("family attack host failed at {effect}: {error}")]
    Host { effect: &'static str, error: E },
}

impl ObjectArena {
    /// 00429B00's entering byte is a no-op when zero. Allocation null and
    /// unavailable services fail at their reached boundary, retaining writes.
    pub fn change_family_attack<H: FamilyAttackHost>(
        &mut self,
        actor: ObjectId,
        entering: bool,
        host: &mut H,
    ) -> Result<(), FamilyAttackError<H::Error>> {
        if !entering {
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.word_228 = 0x0053_1244;
        let first = self.family_weighted_target(actor, FamilyTargetSlot::Target1d4)?;
        let preferred = self.family_weighted_target(actor, FamilyTargetSlot::Target1f4)?;
        if !first.native_pointer_present() && !preferred.native_pointer_present() {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
        } else {
            self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(2_000_000);
            let draw = host.random().map_err(|error| FamilyAttackError::Host {
                effect: "0048F060",
                error,
            })?;
            self.family428650_actor_mut(actor)?.words_188_19c[1] =
                Some(2_000_000 + ((draw & 0xffff) << 4));
            // Unsigned score comparison; equal scores choose +1D4. Native
            // publishes the clone even when its allocation returned null.
            let (selected, _slot) = select_attack_source(first, preferred)
                .expect("at least one native pointer is present");
            // A released raw pointer still participates in native presence and
            // score selection. Only the selected dereference requires a live
            // resource; an unselected released slot does not stop this feature.
            let clone = self.clone_native_target(
                selected.target.expect("selected native pointer"),
                WeightedTargetConstructorNull::Process(host),
            )?;
            self.derived_actor_mut(actor)?.target_148 = clone;
            let clone = clone.ok_or(FamilyAttackError::MissingClone(actor))?;
            let clone_data = self.native_target(clone)?;
            if let Some(target_actor) = self.resolve_target_actor(&clone_data)? {
                let target_size = self.object(target_actor)?.base.word_28;
                let actor_size = self.object(actor)?.base.word_28;
                self.family428650_actor_mut(actor)?.byte_22c = 1;
                self.family428650_actor_mut(actor)?.words_204_208[1] =
                    Some(u32::from(target_size) + u32::from(actor_size) + 350);
                let handle = self.actor(target_actor)?.handle.raw();
                if let Some(helper) = self.family428650_actor_mut(actor)?.helper_218.as_mut() {
                    let old_handle = helper.words_00_04[0]
                        .ok_or(FamilyAttackError::UninitializedHelper { actor, offset: 0 })?;
                    if old_handle != handle {
                        helper.words_00_04 = [Some(handle), Some(0)];
                    } else {
                        let old = helper.words_00_04[1]
                            .ok_or(FamilyAttackError::UninitializedHelper { actor, offset: 4 })?
                            as i32;
                        helper.words_00_04[1] = Some((old / 2) as u32);
                    }
                }
            }
        }
        let bindings = self.derived_actor(actor)?.sound_words_17c_182.ok_or(
            FamilyAttackError::Uninitialized {
                actor,
                offset: 0x180,
            },
        )?;
        let slot = bindings[2];
        if (slot as i16) > 0 {
            host.attack_sound(self, actor, slot)
                .map_err(|error| FamilyAttackError::Host {
                    effect: "00429C09 attack sound",
                    error,
                })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
