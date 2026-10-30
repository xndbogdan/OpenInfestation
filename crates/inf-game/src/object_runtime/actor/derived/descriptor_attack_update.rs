//! Common Attack descriptor update 00435130 -> virtual+F4 ->004298A0.
//! This kernel consumes the preceding caller's +14C/+150 words. It does not
//! attach actor+24, recompute those words before+D8, or supply timer defaults.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::{
    attack::select_attack_source, Family428650Class, FamilyHelperRecordRequest, FamilyTargetSlot,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        NativeHeapRef, NativeTarget, NativeTargetError, ObjectArena, ObjectError, ObjectId,
        TargetNullHandleSource, WeightedTargetConstructorNull,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyAttackUpdateRequest {
    /// Native DWORD delta, interpreted as signed by timer comparisons.
    pub microseconds: u32,
    /// The real settings+7C input consumed only by reached type2 refresh.
    pub settings_7c: u32,
}

pub trait FamilyAttackUpdateHost: TargetNullHandleSource {
    type Error;
    /// Required 004830B0 on the actual +218 allocation captured at service
    /// entry. There is no intervening mutable boundary between capturing
    /// that helper and this call; actor identifies that existing owner.
    /// The host may mutate the arena and retains helper allocation custody
    /// during its implementation. Success is followed by a fresh +22E read.
    fn record_attack_helper(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyHelperRecordRequest,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyAttackUpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("actor {0:?} does not select the owned common004298A0 virtual+F4")]
    UnsupportedClass(ObjectId),
    #[error("actor {actor:?} requires its unwritten +{offset:03X} field")]
    Uninitialized { actor: ObjectId, offset: u16 },
    #[error("actor {actor:?} requires unwritten +218 helper field +{offset:02X}")]
    UninitializedHelper { actor: ObjectId, offset: u8 },
    #[error("actor {actor:?} requires its target +{offset:03X} raw+08 words")]
    UninitializedFocus { actor: ObjectId, offset: u16 },
    #[error("actor {0:?} dereferences a null current+148 target")]
    MissingCurrentTarget(ObjectId),
    #[error("Attack helper004830B0 failed: {0}")]
    Helper(E),
}

/// The WORD displacement/squared-DWORD/high-bit guard used at004299BF.
/// Its result is the original integer48EFC0 distance, not a float length.
fn attack_distance(source: [u16; 3], actor: [u16; 3]) -> u32 {
    let squared = source.into_iter().zip(actor).fold(0u32, |sum, (s, a)| {
        let delta = i32::from(s.wrapping_sub(a) as i16);
        sum.wrapping_add(delta.wrapping_mul(delta) as u32)
    });
    if squared & 0x8000_0000 != 0 {
        isqrt_48efc0((squared >> 2) as i32).wrapping_mul(2)
    } else {
        isqrt_48efc0(squared as i32)
    }
}

impl ObjectArena {
    /// Retained type2/8/16 target allocations preserve pointer identity,
    /// fresh process constructor input and reached refresh/clone writes.
    /// Pure process RNG preserves cadence. Errors retain raw released refs
    /// and pending allocation custody, never a live copy of freed storage.
    pub fn update_family_attack<H: FamilyAttackUpdateHost>(
        &mut self,
        actor: ObjectId,
        request: FamilyAttackUpdateRequest,
        process_rng: &mut u32,
        host: &mut H,
    ) -> Result<(), FamilyAttackUpdateError<H::Error>> {
        // Portable class admission, separate from native pointer fault order.
        if matches!(
            self.family428650_actor(actor)?.class,
            Family428650Class::Selector34(_) | Family428650Class::Selector35(_)
        ) {
            return Err(FamilyAttackUpdateError::UnsupportedClass(actor));
        }
        if !self
            .derived_actor(actor)?
            .target_148_native_pointer_present()
        {
            self.family428650_actor_mut(actor)?.word_22e |= 0x10;
            return Ok(());
        }
        let timer = self.family428650_actor(actor)?.words_188_19c[1].ok_or(
            FamilyAttackUpdateError::Uninitialized {
                actor,
                offset: 0x18c,
            },
        )?;
        if (timer as i32) > (request.microseconds as i32)
            && (self.attack_state_150(actor)? as i32) >= 3
        {
            self.family428650_actor_mut(actor)?.words_188_19c[1] =
                Some(timer.wrapping_sub(request.microseconds));
        } else {
            self.attack_reset_period(actor, process_rng)?;
            let first = self.family_weighted_target(actor, FamilyTargetSlot::Target1d4)?;
            let preferred = self.family_weighted_target(actor, FamilyTargetSlot::Target1f4)?;
            if first.native_pointer_present() || preferred.native_pointer_present() {
                self.attack_reset_period(actor, process_rng)?;
                let (pair, slot) = select_attack_source(first, preferred)
                    .expect("source presence established before second pure draw");
                let selected_ref = pair.target.expect("selected native pointer");
                let mut selected = self.native_target(selected_ref)?;
                //0042997E reads the selected handle before current+04.
                let selected_handle = selected.handle_04();
                let current = self.attack_current_target(actor)?;
                if selected_handle != current.handle_04() {
                    if (self.attack_state_150(actor)? as i32) < 3 {
                        self.attack_replace_current(actor, selected_ref, host)?;
                    } else {
                        let refreshed =
                            self.refresh_native_target(&mut selected, request.settings_7c);
                        // Commit real target cache writes before propagating a
                        // reached refresh failure or taking its signed<=1 exit.
                        self.write_native_target(selected_ref, selected)?;
                        if (refreshed? as i32) > 1 {
                            let source = selected.raw_focus_08_words().ok_or(
                                FamilyAttackUpdateError::UninitializedFocus {
                                    actor,
                                    offset: slot.pointer_offset() as u16,
                                },
                            )?;
                            let position = self
                                .object(actor)?
                                .base
                                .position_words
                                .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                            let distance = attack_distance(source, position);
                            let previous = self.attack_distance_14c(actor)?;
                            if (distance.wrapping_mul(4) as i32) < (previous.wrapping_mul(3) as i32)
                            {
                                self.attack_replace_current(actor, selected_ref, host)?;
                                if let Some(helper) =
                                    self.family428650_actor_mut(actor)?.helper_218.as_mut()
                                {
                                    helper.words_00_04[0] = Some(selected.handle_04().raw());
                                }
                                // Native reloads +218 between the two stores.
                                if let Some(helper) =
                                    self.family428650_actor_mut(actor)?.helper_218.as_mut()
                                {
                                    helper.words_00_04[1] = Some(0);
                                }
                            }
                        }
                    }
                }
            }
            if (self.attack_state_150(actor)? as i32) < 3
                && (name_entry_rng_next(process_rng) & 0xffff) < 0x2000
            {
                self.family428650_actor_mut(actor)?.word_22e |= 0x10;
            }
        }
        let previous = self.attack_distance_14c(actor)?;
        if (previous as i32) >= self.derived_actor(actor)?.word_164 {
            return Ok(());
        }
        let Some(helper) = self.family428650_actor_mut(actor)?.helper_218.as_mut() else {
            return Ok(());
        };
        let time = helper.words_00_04[1]
            .ok_or(FamilyAttackUpdateError::UninitializedHelper { actor, offset: 4 })?
            .wrapping_add(request.microseconds);
        helper.words_00_04[1] = Some(time);
        if (time as i32) <= 8_000_000 {
            return Ok(());
        }
        //00429AAE reads actor X before the current target's raw+08 X.
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let current = self.attack_current_target(actor)?;
        let focus =
            current
                .raw_focus_08_words()
                .ok_or(FamilyAttackUpdateError::UninitializedFocus {
                    actor,
                    offset: 0x148,
                })?;
        let record = FamilyHelperRecordRequest {
            handle: current.handle_04(),
            displacement_words: std::array::from_fn(|i| position[i].wrapping_sub(focus[i])),
            native_weight: 90,
        };
        host.record_attack_helper(self, actor, record)
            .map_err(FamilyAttackUpdateError::Helper)?;
        self.family428650_actor_mut(actor)?.word_22e |= 0x0c;
        Ok(())
    }

    fn attack_state_150<E>(&self, actor: ObjectId) -> Result<u32, FamilyAttackUpdateError<E>> {
        self.derived_actor(actor)?
            .word_150
            .ok_or(FamilyAttackUpdateError::Uninitialized {
                actor,
                offset: 0x150,
            })
    }

    fn attack_distance_14c<E>(&self, actor: ObjectId) -> Result<u32, FamilyAttackUpdateError<E>> {
        self.derived_actor(actor)?
            .word_14c
            .ok_or(FamilyAttackUpdateError::Uninitialized {
                actor,
                offset: 0x14c,
            })
    }

    fn attack_current_target<E>(
        &self,
        actor: ObjectId,
    ) -> Result<NativeTarget, FamilyAttackUpdateError<E>> {
        let reference = self
            .derived_actor(actor)?
            .target_148
            .ok_or(FamilyAttackUpdateError::MissingCurrentTarget(actor))?;
        Ok(self.native_target(reference)?)
    }

    fn attack_reset_period(
        &mut self,
        actor: ObjectId,
        process_rng: &mut u32,
    ) -> Result<(), ObjectError> {
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(2_000_000);
        let draw = name_entry_rng_next(process_rng) & 0xffff;
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(2_000_000 + (draw << 4));
        Ok(())
    }

    fn attack_replace_current<H: FamilyAttackUpdateHost>(
        &mut self,
        actor: ObjectId,
        selected: NativeHeapRef,
        host: &mut H,
    ) -> Result<(), FamilyAttackUpdateError<H::Error>> {
        // Closed resource destructors release only their own allocation.
        // The raw owner slot remains nonnull until clone publication.
        if let Some(reference) = self.derived_actor(actor)?.target_148 {
            self.release_actor_service(reference)?;
        }
        let clone =
            self.clone_native_target(selected, WeightedTargetConstructorNull::Process(host))?;
        self.derived_actor_mut(actor)?.target_148 = clone;
        Ok(())
    }
}

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
