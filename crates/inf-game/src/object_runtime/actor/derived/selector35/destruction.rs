//! 0043D0F0 descriptor-exit prefix under its actual 0052273C class vtable.
//! The shared dispatcher table 00533D38 selects the callback; the destructor
//! never calls a descriptor update or enters a behavior.

use crate::object_runtime::{
    FamilyBehaviorProfile, ObjectArena, ObjectError, ObjectId, Selector35LifecycleError,
    Selector35LifecycleHost, Selector35LifecycleRequest,
};

impl ObjectArena {
    /// The selected descriptor is Dormant (`00469380`, whose `+E0 0043E120`
    /// runs its burst and radius even on false): the exit needs the world's
    /// effect services, so a caller with them runs it before destruction.
    pub(in crate::object_runtime) fn selector35_exit_needs_world_effects(
        &self,
        actor: ObjectId,
    ) -> bool {
        self.selector35_actor(actor).is_ok()
            && self.family428650_actor(actor).is_ok_and(|family| {
                FamilyBehaviorProfile::Selector35
                    .descriptor(family.word_184)
                    .is_some_and(|descriptor| descriptor.change_address == 0x0046_9380)
            })
    }

    /// 0043D0F0's exit from a Dormant descriptor with the world's services:
    /// `0043E120(false)` (burst, then the radius unless bit31), then the
    /// destructor's `+184 = -1`. The cleanup walk pauses for it.
    pub fn exit_selector35_dormant_for_destruction<H: Selector35LifecycleHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector35LifecycleError<H::Error>> {
        if !self.selector35_exit_needs_world_effects(actor) {
            return Err(ObjectError::FamilyBehaviorDestructionBoundary(actor).into());
        }
        self.run_selector35_lifecycle(
            actor,
            Selector35LifecycleRequest::DormantChange { entering: false },
            host,
            rng,
        )?;
        // 0043D12A stores -1 only after the selected callback returns.
        self.family428650_actor_mut(actor)?.word_184 = -1;
        Ok(())
    }

    pub(in crate::object_runtime) fn exit_selector35_behavior_for_destruction(
        &mut self,
        actor: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector35_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        let index = self
            .family428650_actor(actor)
            .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
            .word_184;
        if index < 0 {
            // 0043D110 skips the table and retains every negative value.
            return Ok(());
        }
        let descriptor = FamilyBehaviorProfile::Selector35
            .descriptor(index)
            .ok_or(ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
        match descriptor.change_address {
            // Each forwards argument false to an entry-only body under this
            // vtable: +120 0043DEB0 (Dying), +128 0043DE00 (Run Away),
            // +E8 0043F700 (Following a trail), +E4 00428AB0 (Flying
            // Escort), 0043E660, 0043D780 (Rescue), 0043D9A0 (Go to space)
            // and shared 00427990/00427A00/00427A70. None writes, allocates
            // or draws process RNG on exit.
            0x0043_fe40 | 0x0043_5150 | 0x0043_fa80 | 0x0043_5220 | 0x0043_fca0 | 0x0043_fd80
            | 0x0043_fe00 | 0x0043_fba0 | 0x0043_5190 | 0x0043_fc00 => {}
            // +F8 0043D620 (Attack): the false arm only clears +264.
            0x0043_fb00 => {
                self.selector35_actor_mut(actor)
                    .map_err(|_| ObjectError::FamilyBehaviorDestructionBoundary(actor))?
                    .word_264 = 0;
            }
            // +E0 0043E120 runs its 0048A8B0 burst and the unflagged
            // 004887D0 radius before 00428630 whatever the argument is.
            // Destruction has no particle or radius services.
            _ => return Err(ObjectError::FamilyBehaviorDestructionBoundary(actor)),
        }
        // 0043D12A stores -1 only after the selected callback returns.
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
            .allocate_selector35_actor(
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
    fn selector35_descriptor_exits_clear_the_index_and_only_attack_writes() {
        for index in 1..12 {
            let (mut arena, actor) = ship();
            arena.family428650_actor_mut(actor).unwrap().word_184 = index;
            arena.selector35_actor_mut(actor).unwrap().word_264 = 0x55;
            let before_family = arena.family428650_actor(actor).unwrap().clone();
            arena
                .exit_selector35_behavior_for_destruction(actor)
                .unwrap();
            let mut expected = before_family;
            expected.word_184 = -1;
            let crate::object_runtime::Family428650Class::Selector35(state) = &mut expected.class
            else {
                unreachable!("selector35 fixture");
            };
            if index == 2 {
                state.word_264 = 0;
            }
            assert_eq!(
                arena.family428650_actor(actor).unwrap(),
                &expected,
                "descriptor {index}"
            );
        }
        assert_eq!(FamilyBehaviorProfile::Selector35.descriptor(12), None);
    }

    #[test]
    fn dormant_exit_and_out_of_table_index_are_boundaries_negative_is_retained() {
        let (mut arena, actor) = ship();
        arena.family428650_actor_mut(actor).unwrap().word_184 = -3;
        arena
            .exit_selector35_behavior_for_destruction(actor)
            .unwrap();
        assert_eq!(arena.family428650_actor(actor).unwrap().word_184, -3);
        for index in [0, 12] {
            arena.family428650_actor_mut(actor).unwrap().word_184 = index;
            assert_eq!(
                arena.exit_selector35_behavior_for_destruction(actor),
                Err(ObjectError::FamilyBehaviorDestructionBoundary(actor))
            );
            assert_eq!(arena.family428650_actor(actor).unwrap().word_184, index);
        }
    }
}
