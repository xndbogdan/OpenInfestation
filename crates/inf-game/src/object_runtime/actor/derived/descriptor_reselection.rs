//! Common final+128 (00423E50), +124 (00423BD0) and +130 (00423A20),
//! reached through435150/435170/425D90. Selector35's +128=43DE00 shares
//! the entry prefix and adds its own two writes; its update bodies differ.
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorServiceAllocationRequest, ActorServiceResourceKind, FixedPositionTarget, NativeTarget,
        NativeTargetClass, NativeTargetError, ObjectArena, ObjectError, ObjectId,
        WeightedTargetConstructorNull,
    },
};
use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyTimedDescriptorKind {
    Point124,
    Retry130,
}

#[derive(Clone, Copy, Debug)]
pub struct FamilyTimedDescriptorRequest {
    pub kind: FamilyTimedDescriptorKind,
    pub microseconds: u32,
    pub settings_7c: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyEvadeEntryKind {
    /// Parent/selector10/selector36 final+128=423E50.
    Common128,
    /// Selector35 final+128=43DE00, with the late +234/+230 suffix.
    Selector35128,
}

#[derive(Clone, Copy, Debug)]
pub struct FamilyEvadeEntryRequest<'a> {
    pub kind: FamilyEvadeEntryKind,
    pub entering: u32,
    pub constructor_null: WeightedTargetConstructorNull<'a>,
}

#[derive(Debug, Error)]
pub enum FamilyTimedDescriptorError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] super::DerivedActorError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("actor {actor:?} requires its current +{offset:03X} field")]
    MissingField { actor: ObjectId, offset: u16 },
    #[error("selected target has no current +08 position")]
    MissingFocus,
}

fn distance(words: [u16; 3]) -> u32 {
    let square = words.into_iter().fold(0u32, |sum, word| {
        let value = i32::from(word as i16);
        sum.wrapping_add(value.wrapping_mul(value) as u32)
    });
    if square & 0x8000_0000 != 0 {
        isqrt_48efc0((square >> 2) as i32).wrapping_mul(2)
    } else {
        isqrt_48efc0(square as i32)
    }
}

impl ObjectArena {
    ///435150 forwards the original DWORD;423E50 admits only its low byte.
    ///The old target is not released here: the descriptor dispatcher's +10C
    ///already owns that phase. Genuine allocation NULL still reaches the RNG
    ///suffix; unavailable allocator/source services preserve the old owner.
    pub fn change_family_evade(
        &mut self,
        actor: ObjectId,
        request: FamilyEvadeEntryRequest<'_>,
        rng: &mut u32,
    ) -> Result<(), FamilyTimedDescriptorError> {
        if request.entering as u8 == 0 {
            return Ok(());
        }
        match request.kind {
            FamilyEvadeEntryKind::Common128 => {
                self.family4236d0_actor(actor)?;
            }
            FamilyEvadeEntryKind::Selector35128 => {
                self.selector35_actor(actor)?;
            }
        }
        self.family428650_actor_mut(actor)?.word_228 = 0x0053_0d30;
        let target = if let Some(reference) = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(NativeTargetError::from)?
        {
            //423E76 reads current0054E894 only after nonnull allocation.
            let handle = request.constructor_null.resolve(self)?;
            let point = self
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))?;
            self.construct_native_target(
                reference,
                NativeTarget::FixedPosition(FixedPositionTarget::from_point(handle, Some(point))),
            )?;
            Some(reference)
        } else {
            None
        };
        self.derived_actor_mut(actor)?.target_148 = target;
        let family = self.family428650_actor_mut(actor)?;
        family.words_188_19c[1] = Some(0);
        family.words_188_19c[2] = Some(0);
        let random = name_entry_rng_next(rng) & 0xffff;
        let family = self.family428650_actor_mut(actor)?;
        family.byte_22c = 0;
        family.words_204_208[1] = Some(0);
        family.words_188_19c[3] = Some((random >> 5).wrapping_add(2000));
        if request.kind == FamilyEvadeEntryKind::Selector35128 {
            //43DE93/43DE99: these stores follow the complete shared suffix.
            let state = self.selector35_actor_mut(actor)?;
            state.word_234 = 500;
            state.word_230 = 500;
        }
        Ok(())
    }

    pub fn update_family_timed_descriptor(
        &mut self,
        actor: ObjectId,
        request: FamilyTimedDescriptorRequest,
        rng: &mut u32,
    ) -> Result<(), FamilyTimedDescriptorError> {
        self.family4236d0_actor(actor)?;
        let missing = |offset| FamilyTimedDescriptorError::MissingField { actor, offset };
        let timer =
            self.family428650_actor(actor)?.words_188_19c[1].ok_or_else(|| missing(0x18c))?;
        if (timer as i32) > (request.microseconds as i32) {
            let state = self
                .derived_actor(actor)?
                .word_150
                .ok_or_else(|| missing(0x150))?;
            if (state as i32) >= 3 {
                let family = self.family428650_actor_mut(actor)?;
                if request.kind == FamilyTimedDescriptorKind::Point124 {
                    family.words_188_19c[2] = Some(20_000);
                }
                family.words_188_19c[1] = Some(timer.wrapping_sub(request.microseconds));
                return Ok(());
            }
        }
        let period = if request.kind == FamilyTimedDescriptorKind::Point124 {
            2_000_000u32
        } else {
            500_000u32
        };
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(period);
        let increment = (name_entry_rng_next(rng) & 0xffff) << 4;
        let family = self.family428650_actor_mut(actor)?;
        if request.kind == FamilyTimedDescriptorKind::Retry130 {
            //423A5B ORs the low byte, preserving its unrelated high byte.
            family.word_22e |= 0x10;
            family.words_188_19c[1] = Some(period.wrapping_add(increment));
            return Ok(());
        }
        family.words_188_19c[1] = Some(period.wrapping_add(increment));
        if family.targets_1d4_1fc[3].is_none() && family.targets_1d4_1fc[1].is_none() {
            family.word_22e |= 1;
            return Ok(());
        }
        family.words_188_19c[1] = Some(1_000_000);
        let increment = (name_entry_rng_next(rng) & 0xffff) << 4;
        let family = self.family428650_actor_mut(actor)?;
        family.words_188_19c[1] = Some(1_000_000 + increment);
        let selected = match (family.targets_1d4_1fc[3], family.targets_1d4_1fc[1]) {
            (Some(a), Some(b)) => {
                if family.words_1d8_200[3] > family.words_1d8_200[1] {
                    a
                } else {
                    b
                }
            }
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => unreachable!("no mutable callback follows presence check"),
        };
        let mut target = self.native_target(selected)?;
        let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
        self.write_native_target(selected, target)?;
        refreshed?; //Native ignores the numeric return but preserves cache writes.
        let focus = target
            .raw_focus_08_words()
            .ok_or(FamilyTimedDescriptorError::MissingFocus)?;
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let delta = std::array::from_fn(|axis| position[axis].wrapping_sub(focus[axis]));
        let mut length = distance(delta);
        let (mut x, mut z);
        if length > 256 {
            let shift = (14 / length) & 31;
            x = delta[0].wrapping_shl(shift);
            z = delta[2].wrapping_shl(shift);
            let first = name_entry_rng_next(rng) & 0xffff;
            x = x.wrapping_add(((first as i32).wrapping_mul(i32::from(x as i16)) >> 16) as u16);
            let second = name_entry_rng_next(rng) & 0xffff;
            z = z.wrapping_add(((second as i32).wrapping_mul(i32::from(z as i16)) >> 16) as u16);
        } else {
            let first = (name_entry_rng_next(rng) as u16) >> 3;
            let second = (name_entry_rng_next(rng) as u16) >> 3;
            (x, z) = if length & 1 != 0 {
                (first.wrapping_add(128), second.wrapping_add(128))
            } else {
                (
                    128u16.wrapping_neg().wrapping_sub(first),
                    128u16.wrapping_neg().wrapping_sub(second),
                )
            };
            length = distance([x, delta[1], z]);
            let shift = (14 / length) & 31;
            x = x.wrapping_shl(shift);
            z = z.wrapping_shl(shift);
        }
        self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(length);
        let current = self
            .derived_actor(actor)?
            .target_148
            .ok_or_else(|| missing(0x148))?;
        let mut target = self.native_target(current)?;
        if target.kind_word() == 8 {
            target.set_focus_08_words([
                position[0].wrapping_add(x),
                position[1],
                position[2].wrapping_add(z),
            ]);
            self.write_native_target(current, target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod evade_change_tests;
#[cfg(test)]
mod tests;
