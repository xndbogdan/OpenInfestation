//! Complete constructor-backed type2 child phases of 0040F6A0.
//!
//! The camera deletes +18C then +190 before 0047AA60. The authentic type2
//! deleting body 00482840 -> 00407400 frees only its own allocation, with no
//! referent lookup, callback or vector read. Logical release is explicit here;
//! portable backing and raw alias identity are retained; native heap/SEH parity is not claimed.

use super::{CameraTargetSlot, ObjectArena, ObjectError, ObjectId};

impl ObjectArena {
    /// Consumed by destroy before shared actor handle/FIFO/common-node release.
    /// Empty constructor NULLs stay Published; a freed present target becomes
    /// Released because native 0040F6A0 does not NULL-store either pointer.
    pub(in crate::object_runtime) fn destroy_selector16_targets(
        &mut self,
        camera: ObjectId,
    ) -> Result<(), ObjectError> {
        for slot in [CameraTargetSlot::Primary, CameraTargetSlot::Secondary] {
            let reference = match slot {
                CameraTargetSlot::Primary => self.selector16(camera)?.target_18c,
                CameraTargetSlot::Secondary => self.selector16(camera)?.target_190,
            };
            if let Some(reference) = reference {
                self.release_actor_service(reference)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
