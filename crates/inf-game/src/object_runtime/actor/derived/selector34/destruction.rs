//! 00433B90 descriptor-exit prefix under its actual 00521EAC class vtable.
//! The shared dispatcher table 005329F8 selects the callback; the destructor
//! never calls a descriptor update or enters a behavior.

use crate::object_runtime::{FamilyBehaviorProfile, ObjectArena, ObjectError, ObjectId};

impl ObjectArena {
    pub(in crate::object_runtime) fn exit_selector34_behavior_for_destruction(
        &mut self,
        actor: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector34_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        let index = self
            .family428650_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
            .word_184;
        if index < 0 {
            // 00433BB0 skips the table and retains every negative value.
            return Ok(());
        }
        let descriptor = FamilyBehaviorProfile::Selector34
            .descriptor(index)
            .ok_or(ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        match descriptor.change_address {
            // Each forwards argument false to an entry-only body under this
            // vtable: +E0 00428630, +120 00434130 (Dying), +F8 00433FB0
            // (Attack), +128 00434080 (Run Away), +E4 00428AB0 (Flying
            // Escort), and shared 00427990/00427A00/00427A70. None writes,
            // allocates or draws process RNG on exit.
            0x0046_9380 | 0x0043_fe40 | 0x0043_fb00 | 0x0043_5150 | 0x0043_fba0 | 0x0043_5190
            | 0x0043_fc00 | 0x0043_5220 => {}
            _ => return Err(ObjectError::FamilyBehaviorDestructionBoundary(actor)),
        }
        // 00433BD0 stores -1 only after the selected callback returns.
        self.family428650_actor_mut(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
            .word_184 = -1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::object_runtime::ActorHandle;
    use crate::object_runtime::{
        ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest, FamilyBehaviorProfile, ObjectArena, ObjectError,
    };

    const SINE: [u16; 4096] = [0; 4096];

    fn ship() -> (ObjectArena, crate::object_runtime::ObjectId) {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234),
        })
        .unwrap();
        let actor = arena
            .allocate_selector34_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null:
                            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([10, 20, 30]),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &SINE,
                        },
                    },
                    world_mask_limits: [3, 3],
                },
                &mut 0,
            )
            .unwrap();
        (arena, actor)
    }

    #[test]
    fn every_selector34_descriptor_exit_is_a_no_op_that_clears_the_index() {
        for index in 0..8 {
            let (mut arena, actor) = ship();
            arena.family428650_actor_mut(actor).unwrap().word_184 = index;
            let before = arena.family428650_actor(actor).unwrap().clone();
            arena
                .exit_selector34_behavior_for_destruction(actor)
                .unwrap();
            let after = arena.family428650_actor(actor).unwrap();
            assert_eq!(after.word_184, -1, "descriptor {index}");
            let mut expected = before;
            expected.word_184 = -1;
            assert_eq!(
                after, &expected,
                "descriptor {index} exit wrote no other field"
            );
        }
        assert_eq!(FamilyBehaviorProfile::Selector34.descriptor(8), None);
    }

    #[test]
    fn negative_index_is_retained_and_out_of_table_index_is_a_boundary() {
        let (mut arena, actor) = ship();
        arena.family428650_actor_mut(actor).unwrap().word_184 = -7;
        arena
            .exit_selector34_behavior_for_destruction(actor)
            .unwrap();
        assert_eq!(arena.family428650_actor(actor).unwrap().word_184, -7);
        arena.family428650_actor_mut(actor).unwrap().word_184 = 8;
        assert_eq!(
            arena.exit_selector34_behavior_for_destruction(actor),
            Err(ObjectError::FamilyBehaviorDestructionBoundary(actor))
        );
        assert_eq!(arena.family428650_actor(actor).unwrap().word_184, 8);
    }
}
