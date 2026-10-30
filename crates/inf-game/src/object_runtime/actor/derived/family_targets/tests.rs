use super::*;
use crate::object_runtime::WeightedTargetConstructorNull;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
    ActorServiceResourceState, ActorTerrainInputs, DerivedActorConstructionRequest,
    FixtureActorServiceAllocation, NativeTarget, Selector12GrantContext, WeightedTargetReferent,
};

const SINE: [u16; 4096] = [0; 4096];
fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([1, 2, 3]),
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
    }
}
fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}
#[test]
fn selector12_four_slots_are_independent_and_destroy_before_lower_owners() {
    let mut arena = arena();
    let table = inf_formats::GrantTable {
        virtual_address: 0,
        records: vec![],
    };
    let actor = arena
        .allocate_selector12_actor(
            request(),
            &mut 0,
            Selector12GrantContext {
                table: &table,
                first_player_grants: None,
            },
        )
        .unwrap();
    let referent = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    let constructor = arena.selector12_actor(actor).unwrap();
    assert_eq!(constructor.targets_26c_284, [None; 4]);
    assert_eq!(
        [
            constructor.word_270,
            constructor.word_278,
            constructor.word_280,
            constructor.word_288
        ],
        [0; 4]
    );
    for (index, slot) in FamilyTargetSlot::SELECTOR12.into_iter().enumerate() {
        assert_eq!(slot.pointer_offset(), 0x26c + index as u32 * 8);
        assert!(arena
            .publish_family_target(
                actor,
                slot,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                    referent: WeightedTargetReferent::Actor(Some(referent)),
                    score: 100 + index as u32,
                },
            )
            .unwrap());
    }
    for slot in FamilyTargetSlot::COMMON {
        assert_eq!(
            arena.family_weighted_target(actor, slot).unwrap(),
            NativeWeightedTarget::new(None, 0)
        );
    }
    arena.clear_family_behavior_targets(actor).unwrap();
    for slot in FamilyTargetSlot::SELECTOR12 {
        assert!(arena
            .family_weighted_target(actor, slot)
            .unwrap()
            .target
            .is_some());
    }
    // A released middle slot stops at its original virtual dereference. Higher
    // target resources have already been released; lower owners are untouched.
    let references = arena.selector12_actor(actor).unwrap().targets_26c_284;
    let middle = references[1].unwrap();
    arena.release_actor_service(middle).unwrap();
    arena.family428650_actor_mut(actor).unwrap().helper_218 =
        Some(Box::new(super::super::FamilyHelper482ff0::new(None)));
    let probe_before = arena.family429e20_actor(actor).unwrap().probe_234.clone();
    let handle = arena.actor(actor).unwrap().handle;
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(middle)
        ))
    );
    let state = arena.selector12_actor(actor).unwrap();
    assert_eq!(state.targets_26c_284, references);
    assert!(matches!(
        arena.actor_service_heap().state(references[0].unwrap()),
        Ok(ActorServiceResourceState::Target(_))
    ));
    for reference in references[1..].iter().flatten() {
        assert_eq!(
            arena.actor_service_heap().state(*reference),
            Ok(ActorServiceResourceState::Released)
        );
    }
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().probe_234,
        probe_before
    );
    assert!(arena
        .family428650_actor(actor)
        .unwrap()
        .helper_218
        .is_some());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(actor));
    assert!(arena.object(referent).is_ok());
}

#[test]
fn selector12_completed_retirement_relinquishes_all_resource_custody() {
    let mut arena = arena();
    let table = inf_formats::GrantTable {
        virtual_address: 0,
        records: vec![],
    };
    let actor = arena
        .allocate_selector12_actor(
            request(),
            &mut 0,
            Selector12GrantContext {
                table: &table,
                first_player_grants: None,
            },
        )
        .unwrap();
    let referent = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    for slot in FamilyTargetSlot::SELECTOR12
        .into_iter()
        .chain(FamilyTargetSlot::COMMON)
    {
        arena
            .publish_family_target(
                actor,
                slot,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                    referent: WeightedTargetReferent::Actor(Some(referent)),
                    score: 31,
                },
            )
            .unwrap();
    }
    let common_references = arena.family428650_actor(actor).unwrap().targets_1d4_1fc;
    let selector_references = arena.selector12_actor(actor).unwrap().targets_26c_284;
    let retired = arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let crate::object_runtime::ObjectClass::Actor(actor_state) = retired.class else {
        panic!()
    };
    let crate::object_runtime::ActorExtension::Derived(derived) = actor_state.extension else {
        panic!()
    };
    let super::super::DerivedActorClass::Family428650(family) = derived.class else {
        panic!()
    };
    assert_eq!(family.targets_1d4_1fc, common_references);
    for reference in common_references
        .into_iter()
        .chain(selector_references)
        .flatten()
    {
        assert_eq!(
            arena.actor_service_heap().state(reference),
            Ok(ActorServiceResourceState::Released)
        );
    }
    let Family428650Class::Family429e20(probe) = family.class else {
        panic!()
    };
    assert!(probe.probe_234.is_none());
    let Family429e20Class::Selector12(state) = probe.class else {
        panic!()
    };
    assert_eq!(state.targets_26c_284, selector_references);
    assert_eq!(
        [
            state.word_270,
            state.word_278,
            state.word_280,
            state.word_288
        ],
        [31; 4]
    );
    assert!(arena.object(actor).is_err());
    assert!(arena.object(referent).is_ok());
}

#[test]
fn every_native_pair_uses_one_shared_publication_owner_and_real_constructor_nulls() {
    let mut arena = arena();
    let owner = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    let referent = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    assert_eq!(arena.derived_actor(owner).unwrap().target_148, None);
    assert_eq!(arena.derived_actor(owner).unwrap().target_154, None);
    for (index, slot) in FamilyTargetSlot::COMMON.into_iter().enumerate() {
        assert_eq!(slot.score_offset(), slot.pointer_offset() + 4);
        assert_eq!(
            arena.family_weighted_target(owner, slot).unwrap(),
            NativeWeightedTarget {
                target: None,
                score: 0,
            }
        );
        assert!(arena
            .publish_family_target(
                owner,
                slot,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                    referent: WeightedTargetReferent::Actor(Some(referent)),
                    score: 0x8000_0000 + index as u32
                },
            )
            .unwrap());
        assert!(matches!(
            arena
                .native_target(
                    arena
                        .family_weighted_target(owner, slot)
                        .unwrap()
                        .target
                        .unwrap()
                )
                .unwrap(),
            NativeTarget::ActorFollow(_)
        ));
    }
    assert_eq!(
        arena.family428650_actor(owner).unwrap().words_1d8_200,
        [
            0x8000_0000,
            0x8000_0001,
            0x8000_0002,
            0x8000_0003,
            0x8000_0004,
            0x8000_0005
        ]
    );
}

#[test]
fn real_clear_virtuals_release_current_targets_preserve_source_pairs_and_set_class_state() {
    let mut arena = arena();
    let mut actors = vec![];
    actors.push(
        arena
            .allocate_family4236d0_actor(request(), &mut 0)
            .unwrap(),
    );
    actors.push(arena.allocate_selector35_actor(request(), &mut 0).unwrap());
    let table = inf_formats::GrantTable {
        virtual_address: 0,
        records: vec![],
    };
    actors.push(
        arena
            .allocate_selector12_actor(
                request(),
                &mut 0,
                Selector12GrantContext {
                    table: &table,
                    first_player_grants: None,
                },
            )
            .unwrap(),
    );
    let referent = actors[0];
    for actor in actors {
        arena
            .publish_family_target(
                actor,
                FamilyTargetSlot::Target1d4,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                    referent: WeightedTargetReferent::Actor(Some(referent)),
                    score: 25,
                },
            )
            .unwrap();
        let source = arena
            .family_weighted_target(actor, FamilyTargetSlot::Target1d4)
            .unwrap();
        let current = arena
            .clone_native_target(
                source.target.unwrap(),
                WeightedTargetConstructorNull::ActorRegistry,
            )
            .unwrap()
            .unwrap();
        let secondary = arena
            .clone_native_target(
                source.target.unwrap(),
                WeightedTargetConstructorNull::ActorRegistry,
            )
            .unwrap()
            .unwrap();
        assert_ne!(current, secondary);
        assert_ne!(Some(current), source.target);
        assert_ne!(Some(secondary), source.target);
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(current);
        arena.derived_actor_mut(actor).unwrap().target_154 = Some(secondary);
        arena.family428650_actor_mut(actor).unwrap().byte_22c = 1;
        arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(100);
        arena.owned_mut(actor).unwrap().state.base.flags |= 0x0400_0000;
        let old_dirty = arena.object(actor).unwrap().base.dirty_word;
        arena.clear_family_behavior_targets(actor).unwrap();
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
        assert_eq!(arena.derived_actor(actor).unwrap().target_154, None);
        assert_eq!(
            arena.actor_service_heap().state(current),
            Ok(ActorServiceResourceState::Released)
        );
        assert_eq!(
            arena.actor_service_heap().state(secondary),
            Ok(ActorServiceResourceState::Released)
        );
        assert!(matches!(
            arena.actor_service_heap().state(source.target.unwrap()),
            Ok(ActorServiceResourceState::Target(_))
        ));
        assert_eq!(arena.family428650_actor(actor).unwrap().byte_22c, 0);
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_204_208[1],
            Some(0)
        );
        assert_eq!(
            arena
                .family_weighted_target(actor, FamilyTargetSlot::Target1d4)
                .unwrap(),
            source
        );
        if let Family428650Class::Selector35(state) =
            &arena.family428650_actor(actor).unwrap().class
        {
            assert_eq!(
                (state.word_230, state.word_234, state.byte_238),
                (150, 200, 1)
            );
            assert_eq!(arena.object(actor).unwrap().base.flags & 0x0400_0000, 0);
            assert_eq!(arena.object(actor).unwrap().base.dirty_word, old_dirty);
        } else if arena.family4236d0_actor(actor).is_ok() {
            assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
        }
    }
}

#[test]
fn destruction_releases_owned_resources_but_invalid_exit_boundary_retains_them() {
    let mut arena = arena();
    let owner = arena
        .allocate_family4236d0_actor(request(), &mut 0)
        .unwrap();
    let referent = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    arena
        .publish_family_target(
            owner,
            FamilyTargetSlot::Target1d4,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(Some(referent)),
                score: 17,
            },
        )
        .unwrap();
    let source = arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1d4)
        .unwrap();
    let current = arena
        .clone_native_target(
            source.target.unwrap(),
            WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap()
        .unwrap();
    let secondary = arena
        .clone_native_target(
            source.target.unwrap(),
            WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap()
        .unwrap();
    assert_ne!(current, secondary);
    assert_ne!(Some(current), source.target);
    assert_ne!(Some(secondary), source.target);
    arena.derived_actor_mut(owner).unwrap().target_148 = Some(current);
    arena.derived_actor_mut(owner).unwrap().target_154 = Some(secondary);
    arena.family428650_actor_mut(owner).unwrap().word_184 = 99;
    let handle = arena.actor(owner).unwrap().handle;
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::FamilyBehaviorDestructionBoundary(owner))
    );
    assert_eq!(
        arena
            .family_weighted_target(owner, FamilyTargetSlot::Target1d4)
            .unwrap(),
        source
    );
    assert_eq!(
        arena.derived_actor(owner).unwrap().target_148,
        Some(current)
    );
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(owner));
    arena.family428650_actor_mut(owner).unwrap().word_184 = -1;
    let retired = arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let crate::object_runtime::ObjectClass::Actor(actor) = retired.class else {
        panic!()
    };
    let crate::object_runtime::ActorExtension::Derived(state) = actor.extension else {
        panic!()
    };
    assert_eq!(state.target_148, None);
    assert_eq!(state.target_154, None);
    let crate::object_runtime::DerivedActorClass::Family428650(family) = state.class else {
        panic!()
    };
    assert_eq!(
        family.targets_1d4_1fc,
        [source.target, None, None, None, None, None]
    );
    for reference in [source.target.unwrap(), current, secondary] {
        assert_eq!(
            arena.actor_service_heap().state(reference),
            Ok(ActorServiceResourceState::Released)
        );
    }
    assert_eq!(family.words_1d8_200, [17, 0, 0, 0, 0, 0]);
    assert!(arena.object(referent).is_ok());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
}

#[test]
fn released_source_preserves_publication_order_current_clear_and_reverse_destruction_prefix() {
    let mut arena = arena();
    let owner = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    let other = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    for slot in [
        FamilyTargetSlot::Target1dc,
        FamilyTargetSlot::Target1f4,
        FamilyTargetSlot::Target1fc,
    ] {
        arena
            .publish_family_target(
                owner,
                slot,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                    referent: WeightedTargetReferent::Object(Some(other)),
                    score: 17,
                },
            )
            .unwrap();
    }
    let references = arena.family428650_actor(owner).unwrap().targets_1d4_1fc;
    let crossed = references[1].unwrap();
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    assert_eq!(
        arena.publish_family_target(
            owner,
            FamilyTargetSlot::Target1dc,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(Some(other)),
                score: 23,
            }
        ),
        Err(NativeTargetError::Heap(
            ActorServiceHeapError::FixtureFailure(71)
        ))
    );
    let released = arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1dc)
        .unwrap();
    assert_eq!(released.target, Some(crossed));
    assert_eq!(released.score, 23);
    assert_eq!(
        arena.actor_service_heap().state(crossed),
        Ok(ActorServiceResourceState::Released)
    );
    assert!(released.native_pointer_present());
    assert_eq!(
        arena.publish_family_target(
            owner,
            FamilyTargetSlot::Target1dc,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(Some(other)),
                score: 29,
            }
        ),
        Err(NativeTargetError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(crossed)
        )))
    );
    assert_eq!(
        arena
            .family_weighted_target(owner, FamilyTargetSlot::Target1dc)
            .unwrap()
            .score,
        29
    );
    assert_eq!(
        arena.publish_family_target(
            owner,
            FamilyTargetSlot::Target1dc,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(None),
                score: 0,
            }
        ),
        Err(NativeTargetError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(crossed)
        )))
    );
    assert_eq!(
        arena
            .family_weighted_target(owner, FamilyTargetSlot::Target1dc)
            .unwrap()
            .score,
        29
    );
    arena.clear_family_behavior_targets(owner).unwrap();
    assert_eq!(
        arena
            .family_weighted_target(owner, FamilyTargetSlot::Target1dc)
            .unwrap()
            .target,
        Some(crossed)
    );
    assert_eq!(
        arena.actor_service_heap().state(crossed),
        Ok(ActorServiceResourceState::Released)
    );
    arena.family428650_actor_mut(owner).unwrap().helper_218 = Some(Box::new(
        crate::object_runtime::actor::derived::FamilyHelper482ff0::new(None),
    ));
    let handle = arena.actor(owner).unwrap().handle;
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(crossed)
        ))
    );
    let family = arena.family428650_actor(owner).unwrap();
    assert!(family.helper_218.is_none());
    assert_eq!(family.targets_1d4_1fc, references);
    for index in [1, 4, 5] {
        assert_eq!(
            arena.actor_service_heap().state(references[index].unwrap()),
            Ok(ActorServiceResourceState::Released)
        );
    }
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(owner));
    assert!(arena.object(other).is_ok());
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(references[5].unwrap())
        ))
    );
}

#[test]
fn clear_wrappers_match_eight_original_native_instruction_receipts() {
    let receipts: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/weighted_targets_native.json"
    ))
    .unwrap();
    let clears = receipts["clears"].as_array().unwrap();
    assert_eq!(clears.len(), 8);
    for case in clears {
        let mut arena = arena();
        let table = inf_formats::GrantTable {
            virtual_address: 0,
            records: vec![],
        };
        let method = case["method"].as_u64().unwrap();
        let actor = match method {
            0x429460 => arena
                .allocate_family428650_actor(request(), &mut 0)
                .unwrap(),
            0x425b70 => arena
                .allocate_family4236d0_actor(request(), &mut 0)
                .unwrap(),
            0x468de0 => arena
                .allocate_selector12_actor(
                    request(),
                    &mut 0,
                    Selector12GrantContext {
                        table: &table,
                        first_player_grants: None,
                    },
                )
                .unwrap(),
            0x43f910 => arena.allocate_selector35_actor(request(), &mut 0).unwrap(),
            _ => panic!(),
        };
        arena
            .publish_family_target(
                actor,
                FamilyTargetSlot::Target1d4,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                    referent: WeightedTargetReferent::Actor(Some(actor)),
                    score: 0x8765_4321,
                },
            )
            .unwrap();
        let source = arena
            .family_weighted_target(actor, FamilyTargetSlot::Target1d4)
            .unwrap();
        let current = arena
            .clone_native_target(
                source.target.unwrap(),
                WeightedTargetConstructorNull::ActorRegistry,
            )
            .unwrap()
            .unwrap();
        assert_ne!(Some(current), source.target);
        let secondary = if case["secondary"] == true {
            let reference = arena
                .clone_native_target(
                    source.target.unwrap(),
                    WeightedTargetConstructorNull::ActorRegistry,
                )
                .unwrap()
                .unwrap();
            assert_ne!(reference, current);
            assert_ne!(Some(reference), source.target);
            Some(reference)
        } else {
            None
        };
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(current);
        arena.derived_actor_mut(actor).unwrap().target_154 = secondary;
        arena.family428650_actor_mut(actor).unwrap().byte_22c = 1;
        arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0x1234_5678);
        arena.owned_mut(actor).unwrap().state.base.flags = 0x0400_0100;
        arena.owned_mut(actor).unwrap().state.base.dirty_word = 0x40;
        if method == 0x429460 {
            arena.clear_family_current_targets(actor).unwrap();
        } else {
            arena.clear_family_behavior_targets(actor).unwrap();
        }
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
        assert_eq!(arena.derived_actor(actor).unwrap().target_154, None);
        for reference in Some(current).into_iter().chain(secondary) {
            assert_eq!(
                arena.actor_service_heap().state(reference),
                Ok(ActorServiceResourceState::Released)
            );
        }
        assert!(matches!(
            arena.actor_service_heap().state(source.target.unwrap()),
            Ok(ActorServiceResourceState::Target(_))
        ));
        assert_eq!(
            arena.family428650_actor(actor).unwrap().byte_22c,
            u8::try_from(case["byte_22c"].as_u64().unwrap()).unwrap()
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_204_208[1],
            Some(case["word_208"].as_u64().unwrap() as u32)
        );
        assert_eq!(
            arena
                .family_weighted_target(actor, FamilyTargetSlot::Target1d4)
                .unwrap(),
            source
        );
        assert_eq!(
            arena.object(actor).unwrap().base.flags,
            case["flags"].as_u64().unwrap() as u32
        );
        assert_eq!(
            arena.object(actor).unwrap().base.dirty_word,
            case["dirty_word"].as_u64().unwrap() as u16
        );
        if method == 0x43f910 {
            let state = arena.selector35_actor(actor).unwrap();
            assert_eq!(
                (state.word_230, state.word_234, state.byte_238),
                (
                    case["word_230"].as_u64().unwrap() as u32,
                    case["word_234"].as_u64().unwrap() as u32,
                    case["byte_238"].as_u64().unwrap() as u8
                )
            );
        }
        if method == 0x425b70 {
            assert_eq!(
                arena.family4236d0_actor(actor).unwrap().byte_260,
                case["byte_260"].as_u64().unwrap() as u8
            );
        }
        let native_frees = case["frees"].as_array().unwrap();
        assert_eq!(native_frees[0], "old");
        assert_eq!(
            native_frees.len(),
            1 + usize::from(case["secondary"] == true)
        );
        if native_frees.len() == 2 {
            assert_eq!(native_frees[1], "secondary");
        }
    }
}

#[test]
fn base_clear_preserves_source_pairs_and_family_adapter_keeps_partial_writes() {
    let mut arena = arena();
    let actor = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    arena
        .publish_family_target(
            actor,
            FamilyTargetSlot::Target1f4,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Object(Some(actor)),
                score: 5,
            },
        )
        .unwrap();
    let source = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target1f4)
        .unwrap();
    let current = arena
        .clone_native_target(
            source.target.unwrap(),
            WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap()
        .unwrap();
    assert_ne!(Some(current), source.target);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(current);
    arena.clear_family_behavior_targets(actor).unwrap();
    assert_eq!(
        arena.actor_service_heap().state(current),
        Ok(ActorServiceResourceState::Released)
    );
    assert!(matches!(
        arena.actor_service_heap().state(source.target.unwrap()),
        Ok(ActorServiceResourceState::Target(_))
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
    assert_eq!(
        arena
            .family_weighted_target(actor, FamilyTargetSlot::Target1f4)
            .unwrap(),
        source
    );
    let other = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    arena.owned_mut(other).unwrap().state.base.position_words = None;
    assert_eq!(
        arena.publish_family_target(
            actor,
            FamilyTargetSlot::Target1f4,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Object(Some(other)),
                score: 7
            },
        ),
        Err(NativeTargetError::Object(
            ObjectError::TargetObjectPositionUnknown(other)
        ))
    );
    let changed = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target1f4)
        .unwrap();
    assert_eq!(changed.score, 7);
    assert_eq!(
        arena
            .native_target(changed.target.unwrap())
            .unwrap()
            .object_referent(),
        Some(other)
    );
}

#[test]
fn family_publication_keeps_released_custody_when_reached_process_null_is_missing() {
    let mut arena = arena();
    let actor = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    let slot = FamilyTargetSlot::Target1e4;
    arena
        .publish_family_target(
            actor,
            slot,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Actor(Some(actor)),
                score: 5,
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
            },
        )
        .unwrap();
    let old = arena
        .family_weighted_target(actor, slot)
        .unwrap()
        .target
        .unwrap();
    assert_eq!(
        arena.publish_family_target(
            actor,
            slot,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Point(Some([1, 2, 3])),
                score: 7,
                constructor_null: WeightedTargetConstructorNull::Supplied(None),
            },
        ),
        Err(NativeTargetError::MissingProcessNull)
    );
    let reached = arena.family_weighted_target(actor, slot).unwrap();
    assert_eq!(reached.target, Some(old));
    assert_eq!(reached.score, 7);
    assert_eq!(
        arena.actor_service_heap().state(old),
        Ok(ActorServiceResourceState::Released)
    );
    assert!(reached.native_pointer_present());
    assert_eq!(arena.actor_handles().unwrap().null_handle().raw(), 0x1234);
    assert!(arena.object(actor).is_ok());
}
