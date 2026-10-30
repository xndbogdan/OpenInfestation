//! The actual class guards select the native RET4 leaf. These supplied
//! callback/dispatcher fixtures do not claim a complete loaded actor+24.
use super::*;
use crate::object_runtime::{
    ActorExtension, DerivedActorClass, DerivedActorError, DerivedActorState,
    FamilyBehaviorDispatchRequest, FamilyBehaviorError, Selector12GrantContext,
};

fn construction<'a>(
    host: &'a WorldFamilyBehaviorHost<'_, '_>,
) -> DerivedActorConstructionRequest<'a> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([1200, 200, 1400]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: host.sine_quarter,
            },
        },
        world_mask_limits: [3, 3],
    }
}

fn selector12(arena: &mut ObjectArena, host: &WorldFamilyBehaviorHost<'_, '_>) -> ObjectId {
    let grants = inf_formats::parse_grant_table(host.executable).unwrap();
    let mut constructor_rng = 0x8765_4321;
    let mut request = construction(host);
    request.actor.allocation_position = None; //Explicit unknown XYZ fixture domain.
    arena
        .allocate_selector12_actor(
            request,
            &mut constructor_rng,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap()
}

fn process_snapshot(host: &WorldFamilyBehaviorHost<'_, '_>) -> String {
    format!(
        "{:?}",
        (
            &*host.factory_process,
            &*host.behavior_tables,
            &*host.tasks,
            &*host.particles,
            &*host.sounds,
            &*host.session,
            *host.process_rng,
        )
    )
}

fn supplied_derived(
    arena: &mut ObjectArena,
    actor: ObjectId,
    mutate: impl FnOnce(&mut DerivedActorState),
) {
    arena
        .with_supplied_family_dispatch_actor(actor, |state| {
            let ActorExtension::Derived(derived) = &mut state.extension else {
                panic!("derived")
            };
            mutate(derived);
        })
        .unwrap();
}

fn retained_current(arena: &mut ObjectArena, actor: ObjectId, index: i32, elapsed: Option<u32>) {
    //Explicit supplied scheduler state: signed C4>1 and native bit1 keep the
    //existing index. No score/periodic/duration phase is invented or retried.
    arena
        .with_supplied_family_dispatch_actor(actor, |actor| {
            actor.word_c4 = 3;
            let ActorExtension::Derived(derived) = &mut actor.extension else {
                panic!("derived")
            };
            let DerivedActorClass::Family428650(state) = &mut derived.class else {
                panic!("family")
            };
            state.word_184 = index;
            state.word_22e = 2;
            state.words_188_19c[0] = elapsed;
        })
        .unwrap();
}

#[test]
fn family_evade_selector35_production_entry_reaches_its_distinct_update() {
    with_host(|arena, host| {
        let actor = arena
            .allocate_selector35_actor(construction(host), &mut 7)
            .unwrap();
        let profile = FamilyBehaviorProfile::Selector35;
        assert_eq!(profile.descriptor(3).unwrap().change_address, 0x0043_5150);
        let null = crate::object_runtime::ActorHandle::from_raw(0x7654_3210);
        host.factory_process.process_null_handle = Some(null);
        assert_ne!(arena.actor_handles().unwrap().null_handle(), null);
        //Nondefault fixture values make the class-specific suffix observable.
        let state = arena.selector35_actor_mut(actor).unwrap();
        state.word_230 = 17;
        state.word_234 = 23;
        let mut expected_rng = *host.process_rng;
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
        FamilyBehaviorHost::change(host, arena, actor, profile, 3, true).unwrap();
        let current = arena.derived_actor(actor).unwrap().target_148.unwrap();
        assert_eq!(arena.native_target(current).unwrap().handle_04(), null);
        let state = arena.selector35_actor(actor).unwrap();
        assert_eq!((state.word_234, state.word_230), (500, 500));
        assert_eq!(*host.process_rng, expected_rng);
        supplied_derived(arena, actor, |state| state.word_150 = None);
        retained_current(arena, actor, 3, Some(21));
        let random = crate::name_entry::name_entry_rng_next(&mut expected_rng);
        arena
            .dispatch_family_behavior(
                actor,
                FamilyBehaviorDispatchRequest {
                    microseconds: 7,
                    profile: Some(profile),
                    periodic_scan: true,
                },
                host,
            )
            .unwrap();
        assert_eq!(*host.process_rng, expected_rng);
        let family = arena.family428650_actor(actor).unwrap();
        assert_eq!(family.words_188_19c[0], Some(28));
        assert_eq!(
            family.words_188_19c[1],
            Some(2_000_000 + ((random & 0xffff) << 4))
        );
        assert_ne!(family.word_22e & 1, 0);
        assert_eq!(
            arena.derived_actor(actor).unwrap().target_148,
            Some(current)
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_150, None);
    });
}

#[test]
fn family_evade_production_change_uses_live_null_and_continues_the_real_timed_update() {
    with_host(|arena, host| {
        let actor = constructor_actor(arena, host);
        let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
        assert_eq!(profile.descriptor(3).unwrap().change_address, 0x0043_5150);
        let null = crate::object_runtime::ActorHandle::from_raw(0x7654_3210);
        host.factory_process.process_null_handle = Some(null);
        assert_ne!(arena.actor_handles().unwrap().null_handle(), null);
        let mut expected_rng = *host.process_rng;
        let change_random = crate::name_entry::name_entry_rng_next(&mut expected_rng);
        FamilyBehaviorHost::change(host, arena, actor, profile, 3, true).unwrap();
        assert_eq!(*host.process_rng, expected_rng);
        let current = arena.derived_actor(actor).unwrap().target_148.unwrap();
        let target = arena.native_target(current).unwrap();
        assert_eq!(target.handle_04(), null);
        assert_eq!(
            target.raw_focus_08_words(),
            arena.object(actor).unwrap().base.position_words
        );
        let family = arena.family428650_actor(actor).unwrap();
        assert_eq!(family.word_228, 0x0053_0d30);
        assert_eq!(family.words_188_19c[1], Some(0));
        assert_eq!(family.words_188_19c[2], Some(0));
        assert_eq!(
            family.words_188_19c[3],
            Some(2000 + ((change_random & 0xffff) >> 5))
        );
        assert_eq!(family.byte_22c, 0);
        assert_eq!(family.words_204_208[1], Some(0));
        // The same native dispatcher now enters423BD0. Timer0 bypasses150;
        // no early movement-status writer or heap carry is supplied here.
        supplied_derived(arena, actor, |state| state.word_150 = None);
        retained_current(arena, actor, 3, Some(21));
        let update_random = crate::name_entry::name_entry_rng_next(&mut expected_rng);
        arena
            .dispatch_family_behavior(
                actor,
                FamilyBehaviorDispatchRequest {
                    microseconds: 7,
                    profile: Some(profile),
                    periodic_scan: true,
                },
                host,
            )
            .unwrap();
        assert_eq!(*host.process_rng, expected_rng);
        let family = arena.family428650_actor(actor).unwrap();
        assert_eq!(family.words_188_19c[0], Some(28));
        assert_eq!(
            family.words_188_19c[1],
            Some(2_000_000 + ((update_random & 0xffff) << 4))
        );
        assert_ne!(family.word_22e & 1, 0);
        assert_eq!(arena.derived_actor(actor).unwrap().word_150, None);
        assert_eq!(
            arena.derived_actor(actor).unwrap().target_148,
            Some(current)
        );
    });
}

#[test]
fn both_descriptor_tables_ret4_without_xyz_carries_or_process_reads() {
    with_host(|arena, host| {
        let actor = selector12(arena, host);
        host.factory_process.process_null_handle = None;
        for carry in [None, Some(0xa1b2_c3d4)] {
            //Explicit typed fixture value, never an allocation default.
            supplied_derived(arena, actor, |state| state.word_14c = carry);
            for behavior_present in [false, true] {
                //Both actual pointer tables share index2. Supplying the table
                //mode here does not fabricate a behavior+140 allocation.
                let profile = FamilyBehaviorProfile::Selector12 { behavior_present };
                let descriptor = profile.descriptor(2).unwrap();
                assert_eq!(descriptor.object_address, 0x0054_ecc0);
                assert_eq!(descriptor.update_address, 0x0043_5170);
                for delta in [0, 125_000, u32::MAX] {
                    let before = format!("{arena:?}");
                    let process = process_snapshot(host);
                    FamilyBehaviorHost::update(host, arena, actor, profile, 2, delta).unwrap();
                    assert_eq!(format!("{arena:?}"), before);
                    assert_eq!(process_snapshot(host), process);
                    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, carry);
                }
            }
        }
    });
}

#[test]
fn matching_descriptor_does_not_admit_wrong_class_or_invalid_index() {
    with_host(|arena, host| {
        let actor = constructor_actor(arena, host);
        let profile = FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        };
        let before = format!("{arena:?}");
        let process = process_snapshot(host);
        let error = FamilyBehaviorHost::update(host, arena, actor, profile, 2, 7).unwrap_err();
        let WorldFamilyBehaviorError::Owner { phase, source } = error else {
            panic!("actual class custody must fail");
        };
        assert_eq!(phase, "actual Selector12+124 RET4 update");
        assert!(source.downcast_ref::<DerivedActorError>().is_some());
        assert!(matches!(
            FamilyBehaviorHost::update(host, arena, actor, profile, -1, 7),
            Err(WorldFamilyBehaviorError::DescriptorIndex { index: -1, .. })
        ));
        assert_eq!(format!("{arena:?}"), before);
        assert_eq!(process_snapshot(host), process);
    });
}

#[test]
fn common_parent10_36_and_selector35_wrappers_reach_their_distinct_timer_owners() {
    #[derive(Clone, Copy)]
    enum Class {
        Parent,
        Selector10,
        Selector36,
        Selector35,
    }
    with_host(|arena, host| {
        for class in [
            Class::Parent,
            Class::Selector10,
            Class::Selector36,
            Class::Selector35,
        ] {
            let mut constructor_rng = 0x8765_4321;
            let actor = match class {
                Class::Parent => {
                    arena.allocate_family4236d0_actor(construction(host), &mut constructor_rng)
                }
                Class::Selector10 => {
                    arena.allocate_selector10_actor(construction(host), &mut constructor_rng)
                }
                Class::Selector36 => {
                    arena.allocate_selector36_actor(construction(host), &mut constructor_rng)
                }
                Class::Selector35 => {
                    arena.allocate_selector35_actor(construction(host), &mut constructor_rng)
                }
            }
            .unwrap();
            // Drop the extended +18C carry so each owner's first required
            // read identifies it.
            arena.forget_family_period_18c(actor).unwrap();
            let profile = if matches!(class, Class::Selector35) {
                FamilyBehaviorProfile::Selector35
            } else {
                FamilyBehaviorProfile::Family4236d0 { mode: 1 }
            };
            assert_eq!(profile.descriptor(3).unwrap().update_address, 0x0043_5170);
            let before = format!("{arena:?}");
            let process = process_snapshot(host);
            let error =
                FamilyBehaviorHost::update(host, arena, actor, profile, 3, u32::MAX).unwrap_err();
            let WorldFamilyBehaviorError::Owner { phase, source } = error else {
                panic!("wrapper must reach its owned virtual");
            };
            if matches!(class, Class::Selector35) {
                assert_eq!(phase, "Selector35 descriptor");
                assert!(
                    matches!(source.downcast_ref::<crate::object_runtime::Selector35DescriptorError<WorldActorTerrainError>>(),
                    Some(crate::object_runtime::Selector35DescriptorError::RequiredField {
                        actor: failed, offset: 0x18c, address: 0x0043_db9a,
                    }) if *failed == actor)
                );
            } else {
                assert_eq!(phase, "common timed descriptor update");
                assert!(
                    matches!(source.downcast_ref::<crate::object_runtime::FamilyTimedDescriptorError>(),
                    Some(crate::object_runtime::FamilyTimedDescriptorError::MissingField {
                        actor: failed, offset: 0x18c,
                    }) if *failed == actor)
                );
            }
            assert_eq!(format!("{arena:?}"), before);
            assert_eq!(process_snapshot(host), process);
        }
    });
}

#[test]
fn captured26c_change_then_retained_dispatch_only_adds_elapsed_after_ret4() {
    with_host(|arena, host| {
        let actor = selector12(arena, host);
        let profile = FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        };
        assert_eq!(
            arena.selector12_actor(actor).unwrap().targets_26c_284[0],
            None
        );
        supplied_derived(arena, actor, |state| state.word_14c = None);
        FamilyBehaviorHost::change(host, arena, actor, profile, 2, true).unwrap();
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_228,
            0x0053_a12c
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_204_208[1],
            Some(256)
        );
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
        retained_current(arena, actor, 2, Some(u32::MAX - 2));
        let base = arena.object(actor).unwrap().base.clone();
        let mut expected = arena.derived_actor(actor).unwrap().clone();
        let DerivedActorClass::Family428650(state) = &mut expected.class else {
            panic!("family")
        };
        state.word_22e = 0;
        state.words_188_19c[0] = Some(4);
        let process = process_snapshot(host);
        arena
            .dispatch_family_behavior(
                actor,
                FamilyBehaviorDispatchRequest {
                    microseconds: 7,
                    profile: Some(profile),
                    periodic_scan: true,
                },
                host,
            )
            .unwrap();
        assert_eq!(arena.derived_actor(actor).unwrap(), &expected);
        assert_eq!(arena.object(actor).unwrap().base, base);
        assert_eq!(process_snapshot(host), process);
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    });
}

#[test]
fn normal_ret4_does_not_supply_unknown_following_elapsed_word() {
    with_host(|arena, host| {
        let actor = selector12(arena, host);
        let profile = FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        };
        retained_current(arena, actor, 2, None);
        supplied_derived(arena, actor, |state| state.word_14c = None);
        let mut expected = arena.derived_actor(actor).unwrap().clone();
        let DerivedActorClass::Family428650(state) = &mut expected.class else {
            panic!("family")
        };
        state.word_22e = 0;
        let process = process_snapshot(host);
        assert!(matches!(
            arena.dispatch_family_behavior(actor, FamilyBehaviorDispatchRequest {
                microseconds: 7, profile: Some(profile), periodic_scan: true,
            }, host),
            Err(FamilyBehaviorError::Uninitialized { actor: failed, offset: 0x188 }) if failed == actor
        ));
        assert_eq!(arena.derived_actor(actor).unwrap(), &expected);
        assert_eq!(process_snapshot(host), process);
    });
}

#[test]
fn other_class_update_error_retains_prefix_and_blocks_elapsed_suffix() {
    with_host(|arena, host| {
        let actor = constructor_actor(arena, host);
        // Without the extended +18C carry the owned timer read fails first.
        arena.forget_family_period_18c(actor).unwrap();
        let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
        retained_current(arena, actor, 3, Some(21));
        let mut expected = arena.derived_actor(actor).unwrap().clone();
        let DerivedActorClass::Family428650(state) = &mut expected.class else {
            panic!("family")
        };
        state.word_22e = 0;
        let process = process_snapshot(host);
        let error = arena
            .dispatch_family_behavior(
                actor,
                FamilyBehaviorDispatchRequest {
                    microseconds: 7,
                    profile: Some(profile),
                    periodic_scan: true,
                },
                host,
            )
            .unwrap_err();
        let FamilyBehaviorError::Host {
            effect: "behavior update",
            error: WorldFamilyBehaviorError::Owner { phase, source },
        } = error
        else {
            panic!("owned timer read must fail before the elapsed suffix");
        };
        assert_eq!(phase, "common timed descriptor update");
        assert!(
            matches!(source.downcast_ref::<crate::object_runtime::FamilyTimedDescriptorError>(),
            Some(crate::object_runtime::FamilyTimedDescriptorError::MissingField {
                actor: failed, offset: 0x18c,
            }) if *failed == actor)
        );
        assert_eq!(arena.derived_actor(actor).unwrap(), &expected);
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[0],
            Some(21)
        );
        assert_eq!(process_snapshot(host), process);
    });
}
