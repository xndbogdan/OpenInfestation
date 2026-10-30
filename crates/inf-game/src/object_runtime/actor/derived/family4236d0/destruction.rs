//! 004251D0 helper release and 004237D0 shared descriptor-exit prefix.
//! Selector36's 00481170 directly jumps to the shared destructor. The latter
//! installs00520B84 before dispatch, independently of the live final subclass.

use crate::object_runtime::{
    Family4236d0Class, FamilyBehaviorProfile, ObjectArena, ObjectError, ObjectId,
};

/// Descriptor change bodies whose false (exit) arm writes nothing: under the
/// destructor's 00520B84, E0=424790, 120=424280, F8=429B00, 128=423E50,
/// E8=4288E0 and 134=423A80; the other four wrappers call their shared
/// bodies directly (427990/427A00/427A70/427B40). Every false arm returns
/// without actor writes, RNG, allocations or entry/update services.
pub(in crate::object_runtime) fn exit_writes_nothing(change_address: u32) -> bool {
    matches!(
        change_address,
        0x0046_9380
            | 0x0043_fe40
            | 0x0043_fb00
            | 0x0043_5150
            | 0x0043_fba0
            | 0x0043_5190
            | 0x0043_fc00
            | 0x0043_fa80
            | 0x0042_5d70
            | 0x0042_5e00
    )
}

impl ObjectArena {
    pub(in crate::object_runtime) fn destroy_family4236d0_behavior_prefix(
        &mut self,
        actor: ObjectId,
    ) -> Result<(), ObjectError> {
        let state = self
            .family4236d0_actor_mut(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        if let Family4236d0Class::Selector10(state) = &mut state.class {
            // Existing owned4251D0 storage release: children/header and normal
            // null store precede4237D0, including a later descriptor failure.
            state.helper_26c = None;
        }
        let index = self
            .family428650_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
            .word_184;
        if index < 0 {
            //423803 bypasses even the mode read/table selection.
            return Ok(());
        }
        let mode = self
            .family4236d0_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
            .word_266;
        let descriptor = FamilyBehaviorProfile::Family4236d0 { mode }
            .descriptor(index)
            .ok_or(ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        if !exit_writes_nothing(descriptor.change_address) {
            return Err(ObjectError::FamilyBehaviorDestructionBoundary(actor));
        }
        //423820 follows the descriptor's normal return, before429E70.
        self.family428650_actor_mut(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
            .word_184 = -1;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
