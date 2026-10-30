//! Four 20-byte records in the separate 0x5C helper allocation. Owners are
//! Record 004830B0, Fade 00483340, Clear 00483400 and query 00483000.

use inf_formats::native_basis::{normalize_words, NativeLengthPrecision};
use thiserror::Error;

use super::{FamilyAttackUpdateHost, FamilyHelper482ff0};
use crate::object_runtime::{ActorHandle, ObjectArena, ObjectError, ObjectId};

mod query;
pub use query::FamilyHelperQueryRequest;

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyHelperHistoryRecord {
    pub handle: Option<ActorHandle>,
    /// Stored f32 bits. Unknown copied spans remain unknown; no float is
    /// fabricated from allocation padding or unwritten constructor fields.
    pub normal_bits: [Option<u32>; 3],
    pub weight: Option<u16>,
    /// High WORD of record+0x10. Native compaction copies only 18 bytes.
    pub padding: Option<u16>,
}
impl FamilyHelperHistoryRecord {
    pub const UNKNOWN: Self = Self {
        handle: None,
        normal_bits: [None; 3],
        weight: None,
        padding: None,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyHelperHistoryInputs {
    /// Actual process scalar 0054E894, independently available from registry
    /// lookup. Reached helper clears and Selector35 +20C expiry read it lazily.
    pub process_null_handle: Option<ActorHandle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyHelperRecordRequest {
    pub handle: ActorHandle,
    pub displacement_words: [u16; 3],
    /// Original DWORD argument: low WORD << 11, saturated to 65535.
    pub native_weight: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyHelperHistoryError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires its current +218 helper allocation")]
    MissingHelper(ObjectId),
    #[error("actor {actor:?} requires unwritten helper+{offset:02X}")]
    Uninitialized { actor: ObjectId, offset: u16 },
    #[error("actor {actor:?} helper count {count} exceeds its four-record allocation")]
    CountOutsideAllocation { actor: ObjectId, count: u32 },
    #[error("helper trailing-handle clear requires process null 0054E894")]
    MissingProcessNullHandle,
}

/// Concrete Attack host: allocation remains a required supplied service;
/// the notification executes the real helper owner on current arena custody.
pub struct FamilyAttackUpdateServices<'a> {
    pub process: &'a dyn crate::object_runtime::TargetNullHandleSource,
    pub helper_inputs: FamilyHelperHistoryInputs,
}
impl crate::object_runtime::TargetNullHandleSource for FamilyAttackUpdateServices<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.process.target_null_handle()
    }
}
impl FamilyAttackUpdateHost for FamilyAttackUpdateServices<'_> {
    type Error = FamilyHelperHistoryError;
    fn record_attack_helper(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyHelperRecordRequest,
    ) -> Result<(), Self::Error> {
        arena.record_family_helper_history(actor, request, self.helper_inputs)
    }
}

impl ObjectArena {
    pub fn record_family_helper_history(
        &mut self,
        actor: ObjectId,
        request: FamilyHelperRecordRequest,
        inputs: FamilyHelperHistoryInputs,
    ) -> Result<(), FamilyHelperHistoryError> {
        let weight = ((request.native_weight & 0xffff) << 11).min(65535) as u16;
        let count = self.family_helper_history(actor)?.word_08;
        // Supplied counts outside a backed allocation are not native pointer
        // fault parity. Signed-negative counts have no scan and replace row zero.
        Self::admit_history_count(actor, count)?;
        if count >= 4 && (count as i32) > 0 {
            let mut dead = 0;
            for index in 0..count as usize {
                let handle = self.family_helper_history(actor)?.records[index]
                    .handle
                    .ok_or(FamilyHelperHistoryError::Uninitialized {
                        actor,
                        offset: 0x0c + index as u16 * 20,
                    })?;
                let resolved = self
                    .actor_handles
                    .as_mut()
                    .ok_or(ObjectError::MissingActorRegistry)?
                    .resolve(handle);
                if resolved.is_none() || (self.actor(resolved.unwrap())?.word_c4 as i32) < 3 {
                    self.family_helper_history_mut(actor)?.records[index].weight = Some(0);
                    dead += 1;
                }
            }
            if dead != 0 {
                self.compact_family_helper_history(actor, count, dead, inputs)?;
            }
        }
        let count = self.family_helper_history(actor)?.word_08;
        let index = if count < 4 {
            count as usize
        } else {
            let mut selected = 0;
            let mut minimum = 65535;
            for index in 0..(count as i32).max(0) as usize {
                let current = self.family_helper_weight(actor, index)?;
                if current < minimum {
                    selected = index;
                    minimum = current;
                }
            }
            selected
        };
        self.family_helper_history_mut(actor)?.records[index].handle = Some(request.handle);
        self.family_helper_history_mut(actor)?.records[index].weight = Some(weight);
        // 0048322B/004832DF spill the 53-bit signed-WORD sqrt to f32 before
        // the three divisions, unlike 00417DA0's retained divisor.
        let normal = normalize_words(
            request.displacement_words.map(|v| v as i16),
            NativeLengthPrecision::Spilled,
        )
        .map(f32::to_bits);
        for (axis, bits) in normal.into_iter().enumerate() {
            self.family_helper_history_mut(actor)?.records[index].normal_bits[axis] = Some(bits);
        }
        if count < 4 {
            self.family_helper_history_mut(actor)?.word_08 = count.wrapping_add(1);
        }
        Ok(())
    }

    pub fn fade_family_helper_history(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        inputs: FamilyHelperHistoryInputs,
    ) -> Result<(), FamilyHelperHistoryError> {
        let count = self.family_helper_history(actor)?.word_08;
        if (count as i32) <= 0 {
            return Ok(());
        }
        Self::admit_history_count(actor, count)?;
        let step = microseconds >> 11;
        let mut removed = 0;
        for index in 0..count as usize {
            let weight = u32::from(self.family_helper_weight(actor, index)?);
            let next = if weight > step {
                (weight - step) as u16
            } else {
                removed += 1;
                0
            };
            self.family_helper_history_mut(actor)?.records[index].weight = Some(next);
        }
        if removed != 0 {
            self.compact_family_helper_history(actor, count, removed, inputs)?;
        }
        Ok(())
    }

    pub fn clear_family_helper_history(
        &mut self,
        actor: ObjectId,
        inputs: FamilyHelperHistoryInputs,
    ) -> Result<(), FamilyHelperHistoryError> {
        if self.family_helper_history(actor)?.word_08 == 0 {
            return Ok(());
        }
        // Any nonzero count reaches all four records, including negative.
        for index in 0..4 {
            self.family_helper_history_mut(actor)?.records[index].weight = Some(0);
            let handle = inputs
                .process_null_handle
                .ok_or(FamilyHelperHistoryError::MissingProcessNullHandle)?;
            self.family_helper_history_mut(actor)?.records[index].handle = Some(handle);
        }
        self.family_helper_history_mut(actor)?.word_08 = 0;
        Ok(())
    }

    fn compact_family_helper_history(
        &mut self,
        actor: ObjectId,
        count: u32,
        removed: u32,
        inputs: FamilyHelperHistoryInputs,
    ) -> Result<(), FamilyHelperHistoryError> {
        let mut destination = 0;
        for source in 0..count as usize {
            let weight = self.family_helper_weight(actor, source)?;
            if weight != 0 {
                if source != destination {
                    let source_record = self.family_helper_history(actor)?.records[source];
                    let target = &mut self.family_helper_history_mut(actor)?.records[destination];
                    target.handle = source_record.handle;
                    target.normal_bits = source_record.normal_bits;
                    target.weight = Some(weight);
                }
                destination += 1;
            }
        }
        for index in destination..count as usize {
            let handle = inputs
                .process_null_handle
                .ok_or(FamilyHelperHistoryError::MissingProcessNullHandle)?;
            self.family_helper_history_mut(actor)?.records[index].handle = Some(handle);
        }
        // Record subtracts dead referents, not total zero-weight records;
        // Fade subtracts all zeroed records. Preserve that real asymmetry.
        self.family_helper_history_mut(actor)?.word_08 = count.wrapping_sub(removed);
        Ok(())
    }

    fn admit_history_count(actor: ObjectId, count: u32) -> Result<(), FamilyHelperHistoryError> {
        if (count as i32) > 4 {
            Err(FamilyHelperHistoryError::CountOutsideAllocation { actor, count })
        } else {
            Ok(())
        }
    }
    fn family_helper_weight(
        &self,
        actor: ObjectId,
        index: usize,
    ) -> Result<u16, FamilyHelperHistoryError> {
        self.family_helper_history(actor)?.records[index]
            .weight
            .ok_or(FamilyHelperHistoryError::Uninitialized {
                actor,
                offset: 0x1c + index as u16 * 20,
            })
    }
    fn family_helper_history(
        &self,
        actor: ObjectId,
    ) -> Result<&FamilyHelper482ff0, FamilyHelperHistoryError> {
        self.family428650_actor(actor)?
            .helper_218
            .as_deref()
            .ok_or(FamilyHelperHistoryError::MissingHelper(actor))
    }
    fn family_helper_history_mut(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut FamilyHelper482ff0, FamilyHelperHistoryError> {
        self.family428650_actor_mut(actor)?
            .helper_218
            .as_deref_mut()
            .ok_or(FamilyHelperHistoryError::MissingHelper(actor))
    }
}
