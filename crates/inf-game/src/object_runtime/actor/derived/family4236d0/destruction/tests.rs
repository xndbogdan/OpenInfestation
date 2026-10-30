use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
    ActorServiceResourceState, ActorTerrainInputs, DerivedActorConstructionRequest,
    FamilyTargetSlot, ObjectDestructionRequest, Selector10Helper, SoundTeardownService,
    WeightedTargetConstructorNull, WeightedTargetReferent, WeightedTargetRequest,
};

const SINE: [u16; 4096] = [0; 4096];

fn fixture(class: &str, mode: u16, index: i32, helper: bool) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap();
    let request = DerivedActorConstructionRequest {
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
    };
    let actor = match class {
        "base" => arena.allocate_family4236d0_actor(request, &mut 0),
        "selector10" => arena.allocate_selector10_actor(request, &mut 0),
        "selector36" => arena.allocate_selector36_actor(request, &mut 0),
        _ => unreachable!("native fixture class"),
    }
    .unwrap();
    arena.family4236d0_actor_mut(actor).unwrap().word_266 = mode;
    arena.family428650_actor_mut(actor).unwrap().word_184 = index;
    if let Family4236d0Class::Selector10(state) =
        &mut arena.family4236d0_actor_mut(actor).unwrap().class
    {
        state.helper_26c = helper.then(|| Selector10Helper::for_selector(1).unwrap());
    }
    (arena, actor)
}

#[test]
fn shared_destruction_matches_original_modes_and_actual_subclass_wrappers() {
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/family4236d0_destruction_native.json"
    ))
    .unwrap();
    let cases = native["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 86);
    for case in cases {
        if case["stop_helper"] == true {
            // Native helper-delete failure is a supplied lower-service receipt;
            // the port's existing Box-owned helper deletion is infallible.
            assert_eq!(case["events"].as_array().unwrap().len(), 1);
            assert_eq!(case["events"][0]["kind"], "helper_delete");
            assert_eq!(case["index_184"], case["index"]);
            continue;
        }
        let class = case["class"].as_str().unwrap();
        let mode = case["mode"].as_u64().unwrap() as u16;
        let index = case["index"].as_i64().unwrap() as i32;
        let helper = case["helper"].as_bool().unwrap();
        let (mut arena, actor) = fixture(class, mode, index, helper);
        let before = arena.object(actor).unwrap().clone();
        let old_helper = arena
            .selector10_actor(actor)
            .ok()
            .and_then(|state| state.helper_26c.clone());
        arena.destroy_family4236d0_behavior_prefix(actor).unwrap();
        let expected_index = case["index_184"].as_u64().unwrap() as u32;
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_184 as u32,
            expected_index
        );
        if let Ok(state) = arena.selector10_actor(actor) {
            assert!(state.helper_26c.is_none());
            assert_eq!(case["helper_cleared"], true);
        }
        if let Some(callback) = case["callback"].as_u64() {
            assert_eq!(
                FamilyBehaviorProfile::Family4236d0 { mode }
                    .descriptor(index)
                    .unwrap()
                    .change_address,
                callback as u32
            );
        }
        // The exit leaves all other actor backing untouched. In particular it
        // must not execute the live class's entry or update virtuals.
        arena.family428650_actor_mut(actor).unwrap().word_184 = index;
        if let Family4236d0Class::Selector10(state) =
            &mut arena.family4236d0_actor_mut(actor).unwrap().class
        {
            state.helper_26c = old_helper;
        }
        assert_eq!(arena.object(actor).unwrap(), &before);
        let handle = arena.actor(actor).unwrap().handle;
        let retired = arena
            .destroy(ObjectDestructionRequest {
                object: actor,
                sound: SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
        let crate::object_runtime::ObjectClass::Actor(state) = retired.class else {
            panic!()
        };
        let crate::object_runtime::ActorExtension::Derived(state) = state.extension else {
            panic!()
        };
        let crate::object_runtime::DerivedActorClass::Family428650(family) = state.class else {
            panic!()
        };
        assert_eq!(family.word_184 as u32, expected_index);
    }
}

#[test]
fn invalid_descriptor_retains_actor_and_lower_owners_after_selector10_helper_prefix() {
    for class in ["base", "selector10", "selector36"] {
        for (mode, index) in [(3, 0), (u16::MAX, 0), (0, 3), (1, 8), (2, 9)] {
            let (mut arena, actor) = fixture(class, mode, index, true);
            let handle = arena.actor(actor).unwrap().handle;
            let probe = arena.family429e20_actor(actor).unwrap().probe_234.clone();
            let before = arena.object(actor).unwrap().clone();
            let old_helper = arena
                .selector10_actor(actor)
                .ok()
                .and_then(|state| state.helper_26c.clone());
            assert_eq!(
                arena.destroy(ObjectDestructionRequest {
                    object: actor,
                    sound: SoundTeardownService::RequiredAtReach,
                }),
                Err(ObjectError::FamilyBehaviorDestructionBoundary(actor))
            );
            assert_eq!(arena.family428650_actor(actor).unwrap().word_184, index);
            assert_eq!(arena.family429e20_actor(actor).unwrap().probe_234, probe);
            assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(actor));
            if let Family4236d0Class::Selector10(state) =
                &mut arena.family4236d0_actor_mut(actor).unwrap().class
            {
                assert!(state.helper_26c.is_none());
                state.helper_26c = old_helper;
            }
            assert_eq!(arena.object(actor).unwrap(), &before);
        }
    }
}

#[test]
fn lower_target_failure_retains_normal_exit_and_completed_probe_cleanup() {
    for class in ["base", "selector10", "selector36"] {
        let (mut arena, actor) = fixture(class, 2, 8, true);
        for slot in FamilyTargetSlot::COMMON {
            arena
                .publish_family_target(
                    actor,
                    slot,
                    WeightedTargetRequest {
                        constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                        referent: WeightedTargetReferent::Actor(Some(actor)),
                        score: 17,
                    },
                )
                .unwrap();
        }
        let targets = arena.family428650_actor(actor).unwrap().targets_1d4_1fc;
        let middle = targets[3].unwrap();
        arena.release_actor_service(middle).unwrap();
        let handle = arena.actor(actor).unwrap().handle;
        assert_eq!(
            arena.destroy(ObjectDestructionRequest {
                object: actor,
                sound: SoundTeardownService::RequiredAtReach,
            }),
            Err(ObjectError::ActorServiceHeap(
                ActorServiceHeapError::Released(middle)
            ))
        );
        assert_eq!(arena.family428650_actor(actor).unwrap().word_184, -1);
        assert!(arena.family429e20_actor(actor).unwrap().probe_234.is_none());
        if let Ok(state) = arena.selector10_actor(actor) {
            assert!(state.helper_26c.is_none());
        }
        for reference in targets[..3].iter().flatten() {
            assert!(matches!(
                arena.actor_service_heap().state(*reference),
                Ok(ActorServiceResourceState::Target(_))
            ));
        }
        for reference in targets[3..].iter().flatten() {
            assert_eq!(
                arena.actor_service_heap().state(*reference),
                Ok(ActorServiceResourceState::Released)
            );
        }
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(actor));
    }
}
