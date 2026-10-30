//! Family target storage and lifetime policies. Slot names retain native
//! offsets until each score producer's gameplay meaning is independently owned.

use super::{Family428650Class, Family429e20Class};
use crate::object_runtime::{
    NativeTargetError, NativeWeightedTarget, ObjectArena, ObjectError, ObjectId,
    WeightedTargetRequest,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyTargetSlot {
    Target1d4,
    Target1dc,
    Target1e4,
    Target1ec,
    Target1f4,
    Target1fc,
    Target26c,
    Target274,
    Target27c,
    Target284,
}

impl FamilyTargetSlot {
    pub const COMMON: [Self; 6] = [
        Self::Target1d4,
        Self::Target1dc,
        Self::Target1e4,
        Self::Target1ec,
        Self::Target1f4,
        Self::Target1fc,
    ];

    pub const SELECTOR12: [Self; 4] = [
        Self::Target26c,
        Self::Target274,
        Self::Target27c,
        Self::Target284,
    ];

    fn storage(self) -> TargetStorage {
        match self {
            Self::Target1d4 => TargetStorage::Common(0),
            Self::Target1dc => TargetStorage::Common(1),
            Self::Target1e4 => TargetStorage::Common(2),
            Self::Target1ec => TargetStorage::Common(3),
            Self::Target1f4 => TargetStorage::Common(4),
            Self::Target1fc => TargetStorage::Common(5),
            Self::Target26c => TargetStorage::Selector12(0),
            Self::Target274 => TargetStorage::Selector12(1),
            Self::Target27c => TargetStorage::Selector12(2),
            Self::Target284 => TargetStorage::Selector12(3),
        }
    }

    pub fn pointer_offset(self) -> u32 {
        match self.storage() {
            TargetStorage::Common(index) => 0x1d4 + index as u32 * 8,
            TargetStorage::Selector12(index) => 0x26c + index as u32 * 8,
        }
    }

    pub fn score_offset(self) -> u32 {
        self.pointer_offset() + 4
    }
}

#[derive(Clone, Copy)]
enum TargetStorage {
    Common(usize),
    Selector12(usize),
}

#[derive(Clone, Copy)]
enum TargetClearPolicy {
    CurrentOnly,
    Family4236d0,
    Selector12,
    Scalar293d0,
}

impl ObjectArena {
    pub fn family_weighted_target(
        &self,
        actor: ObjectId,
        slot: FamilyTargetSlot,
    ) -> Result<NativeWeightedTarget, ObjectError> {
        Ok(match slot.storage() {
            TargetStorage::Common(index) => {
                let family = self.family428650_actor(actor)?;
                NativeWeightedTarget {
                    target: family.targets_1d4_1fc[index],
                    score: family.words_1d8_200[index],
                }
            }
            TargetStorage::Selector12(index) => {
                let Family429e20Class::Selector12(state) = &self.family429e20_actor(actor)?.class
                else {
                    return Err(ObjectError::NotSelector12(actor));
                };
                NativeWeightedTarget {
                    target: state.targets_26c_284[index],
                    score: [
                        state.word_270,
                        state.word_278,
                        state.word_280,
                        state.word_288,
                    ][index],
                }
            }
        })
    }

    /// One adapter for the common six and independent selector12 four pairs. Preserve the
    /// shared publisher's early score/identity writes even when a reached
    /// referent read or allocation service fails.
    pub fn publish_family_target(
        &mut self,
        actor: ObjectId,
        slot: FamilyTargetSlot,
        request: WeightedTargetRequest<'_>,
    ) -> Result<bool, NativeTargetError> {
        let mut pair = self.family_weighted_target(actor, slot)?;
        let result = self.publish_weighted_target(&mut pair, request);
        match slot.storage() {
            TargetStorage::Common(index) => {
                let family = self.family428650_actor_mut(actor)?;
                family.targets_1d4_1fc[index] = pair.target;
                family.words_1d8_200[index] = pair.score;
            }
            TargetStorage::Selector12(index) => {
                let Family429e20Class::Selector12(state) =
                    &mut self.family429e20_actor_mut(actor)?.class
                else {
                    return Err(ObjectError::NotSelector12(actor).into());
                };
                state.targets_26c_284[index] = pair.target;
                *[
                    &mut state.word_270,
                    &mut state.word_278,
                    &mut state.word_280,
                    &mut state.word_288,
                ][index] = pair.score;
            }
        }
        result
    }

    /// Shared 00429460 destroys +148 then +154, clearing each pointer after
    /// deletion, then clears +22C. Weighted source slots and scores survive.
    pub fn clear_family_current_targets(&mut self, actor: ObjectId) -> Result<(), ObjectError> {
        self.family428650_actor(actor)?;
        if let Some(reference) = self.derived_actor(actor)?.target_148 {
            self.release_actor_service(reference)?;
        }
        self.derived_actor_mut(actor)?.target_148 = None;
        if let Some(reference) = self.derived_actor(actor)?.target_154 {
            self.release_actor_service(reference)?;
        }
        self.derived_actor_mut(actor)?.target_154 = None;
        self.family428650_actor_mut(actor)?.byte_22c = 0;
        Ok(())
    }

    /// Actual virtual +10C policies. Base/probe-only classes invoke 00429460
    /// alone; family4236d0/selector12 and scalar293d0 classes have suffixes.
    pub fn clear_family_behavior_targets(&mut self, actor: ObjectId) -> Result<(), ObjectError> {
        let policy = match &self.family428650_actor(actor)?.class {
            Family428650Class::Base => TargetClearPolicy::CurrentOnly,
            Family428650Class::Selector34(_) | Family428650Class::Selector35(_) => {
                TargetClearPolicy::Scalar293d0
            }
            Family428650Class::Family429e20(state) => match state.class {
                Family429e20Class::Base => TargetClearPolicy::CurrentOnly,
                Family429e20Class::Family4236d0(_) => TargetClearPolicy::Family4236d0,
                Family429e20Class::Selector12(_) => TargetClearPolicy::Selector12,
            },
        };
        self.clear_family_current_targets(actor)?;
        if matches!(policy, TargetClearPolicy::CurrentOnly) {
            return Ok(());
        }
        if matches!(policy, TargetClearPolicy::Scalar293d0) {
            let base = &mut self.owned_mut(actor)?.state.base;
            let old_flags = base.flags;
            base.flags &= !0x0400_0000;
            if (old_flags ^ base.flags) & 0x1ff != 0 {
                base.dirty_word |= 2;
            }
        }
        self.family428650_actor_mut(actor)?.words_204_208[1] = Some(0);
        match &mut self.family428650_actor_mut(actor)?.class {
            Family428650Class::Selector35(state) => {
                state.word_234 = 200;
                state.word_230 = 150;
                state.byte_238 = 1;
            }
            Family428650Class::Selector34(state) => {
                state.word_234 = 200;
                state.word_230 = 150;
                state.byte_238 = 1;
            }
            Family428650Class::Family429e20(state) => {
                if let Family429e20Class::Family4236d0(state) = &mut state.class {
                    state.byte_260 = 1;
                }
            }
            _ => unreachable!("class checked before target deletion"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
