//! Read-only 00483000, composed by the shared candidate gate 00429CB0.

use super::{ActorHandle, FamilyHelperHistoryError, ObjectArena, ObjectId};
use crate::object_runtime::actor::derived::kinematics::ftol;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyHelperQueryRequest {
    pub handle: ActorHandle,
    /// Complete supplied WORD displacement, interpreted as signed per axis.
    /// Native vector-pointer faults/aliases are outside this typed domain.
    pub displacement_words: [u16; 3],
}

impl ObjectArena {
    pub fn query_family_helper_history(
        &self,
        actor: ObjectId,
        request: FamilyHelperQueryRequest,
    ) -> Result<bool, FamilyHelperHistoryError> {
        let helper = self.family_helper_history(actor)?;
        let count = helper.word_08;
        if (count as i32) <= 0 {
            return Ok(false);
        }
        // Admission of supplied backed storage, not native fault parity.
        Self::admit_history_count(actor, count)?;
        for index in 0..count as usize {
            let record = helper.records[index];
            let handle = record
                .handle
                .ok_or(FamilyHelperHistoryError::Uninitialized {
                    actor,
                    offset: 0x0c + index as u16 * 20,
                })?;
            if handle != request.handle {
                continue;
            }
            let normal = |axis: usize| {
                record.normal_bits[axis]
                    .map(|bits| f64::from(f32::from_bits(bits)))
                    .ok_or(FamilyHelperHistoryError::Uninitialized {
                        actor,
                        offset: 0x10 + index as u16 * 20 + axis as u16 * 4,
                    })
            };
            let [x, y, z] = request.displacement_words.map(|v| i32::from(v as i16));
            // 00483043/4E/57: (X*nx + Z*nz) + Y*ny, retained at
            // supported 53-bit precision, with no intermediate f32 spill.
            let dot = ftol(
                (f64::from(x) * normal(0)? + f64::from(z) * normal(2)?) + f64::from(y) * normal(1)?,
            );
            if dot > 0 {
                // The source deliberately omits Y²: low-DWORD IMUL/ADD,
                // logical SHR2 and unsigned strict comparison at 0048307E.
                let threshold =
                    x.wrapping_mul(x)
                        .wrapping_add(y.wrapping_add(z).wrapping_mul(z)) as u32
                        >> 2;
                if (dot.wrapping_mul(dot) as u32) > threshold {
                    return Ok(true);
                }
            }
            // A matching handle with a failed dot/threshold continues to
            // later matching rows; only the first accepted row terminates.
        }
        Ok(false)
    }
}

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
