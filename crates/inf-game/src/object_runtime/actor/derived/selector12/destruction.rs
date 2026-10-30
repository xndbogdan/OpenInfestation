//! 004671E0 descriptor-exit prefix under its actual 00527058 class vtable.
//! The shared dispatcher tables select the callback. A destructor never calls
//! the descriptor update or enters its behavior, even if those are unowned.

use crate::object_runtime::{FamilyBehaviorProfile, ObjectArena, ObjectError, ObjectId};

impl ObjectArena {
    pub(in crate::object_runtime) fn exit_selector12_behavior_for_destruction(
        &mut self,
        actor: ObjectId,
    ) -> Result<(), ObjectError> {
        // ObjectArena::destroy dispatches this only for the concrete class;
        // 004671FD also installs that class's vtable before virtual dispatch.
        self.selector12_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        let family = self
            .family428650_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        let index = family.word_184;
        if index < 0 {
            // All negative values bypass the table and retain their value.
            return Ok(());
        }
        let profile = FamilyBehaviorProfile::Selector12 {
            behavior_present: self
                .derived_actor(actor)
                .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
                .behavior_140
                .is_some(),
        };
        let descriptor = profile
            .descriptor(index)
            .ok_or(ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        match descriptor.change_address {
            // These exact callbacks return on false after the wrapper's
            // actual 00527058 virtual dispatch. In particular 0043FE40 calls
            // +120=004678A0; its false branch skips all particle/RNG work.
            // 0043FB00 calls +F8=00429B00, whose false branch skips attack.
            0x0046_9380 | 0x0043_fe40 | 0x0046_9170 | 0x0046_91f0 | 0x0046_91d0 | 0x0043_fba0
            | 0x0043_5190 | 0x0043_fb00 => {}
            // 004692F0 -> actual +144=00467760 -> false arm00467826.
            // Keep the side effect in the existing shared change owner.
            0x0046_92f0 => {
                self.change_selector12_descriptor_target(
                    actor,
                    super::super::Selector12DescriptorChangeRequest {
                        kind: super::super::Selector12DescriptorChangeKind::PreferredTarget27c284,
                        entering: false,
                    },
                    // The exit only clears29C; it reads no constructor/null
                    // handle, allocates nothing and consumes no process RNG.
                    crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry,
                )
                .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
            }
            _ => return Err(ObjectError::FamilyBehaviorDestructionBoundary(actor)),
        }
        // 00467239 is reached only after the selected callback returns.
        self.family428650_actor_mut(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
            .word_184 = -1;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
