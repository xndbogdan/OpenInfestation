use super::*;
use crate::object_runtime::{
    actor::derived::build_actor_behavior, ActorConstructionRequest, ActorHandle,
    ActorRegistryConfig, ActorServiceHeapError, ActorServiceResourceState, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyTargetSlot, ObjectDestructionRequest,
    Selector12GrantContext, SoundTeardownService, WeightedTargetConstructorNull,
    WeightedTargetReferent, WeightedTargetRequest,
};

const SINE: [u16; 4096] = [0; 4096];

fn fixture(behavior_present: bool, index: i32) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap();
    let actor = arena
        .allocate_selector12_actor(
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
            },
            &mut 0,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: 0,
                    records: vec![],
                },
                first_player_grants: None,
            },
        )
        .unwrap();
    if behavior_present {
        arena.derived_actor_mut(actor).unwrap().behavior_140 = build_actor_behavior(1, 0, 0);
    }
    arena.family428650_actor_mut(actor).unwrap().word_184 = index;
    arena.selector12_actor_mut(actor).unwrap().byte_29c = 7;
    (arena, actor)
}

#[test]
fn actual_destructor_exits_every_native_table_entry_without_update_or_entry_services() {
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector12_destruction_native.json"
    ))
    .unwrap();
    for case in native["cases"].as_array().unwrap() {
        let present = case["behavior_present"].as_bool().unwrap();
        let index = case["index"].as_i64().unwrap() as i32;
        let (mut arena, actor) = fixture(present, index);
        let before = arena.object(actor).unwrap().clone();
        arena
            .exit_selector12_behavior_for_destruction(actor)
            .unwrap();
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_184 as u32,
            case["index_184"].as_u64().unwrap() as u32
        );
        assert_eq!(
            arena.selector12_actor(actor).unwrap().byte_29c,
            case["byte_29c"].as_u64().unwrap() as u8
        );
        // Only the two proven fields differ from the live object snapshot.
        arena.family428650_actor_mut(actor).unwrap().word_184 = index;
        arena.selector12_actor_mut(actor).unwrap().byte_29c = 7;
        assert_eq!(arena.object(actor).unwrap(), &before);
        if let Some(callback) = case["callback"].as_u64() {
            assert_eq!(
                FamilyBehaviorProfile::Selector12 {
                    behavior_present: present
                }
                .descriptor(index)
                .unwrap()
                .change_address,
                callback as u32
            );
        }
        // Production destruction consumes the same prefix and real lower owners.
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
        assert_eq!(
            family.word_184 as u32,
            case["index_184"].as_u64().unwrap() as u32
        );
    }
}

#[test]
fn invalid_descriptor_stops_before_index_clear_or_lower_target_release() {
    for present in [false, true] {
        let (mut arena, actor) = fixture(present, if present { 9 } else { 8 });
        let before = format!("{arena:?}");
        assert_eq!(
            arena.destroy(ObjectDestructionRequest {
                object: actor,
                sound: SoundTeardownService::RequiredAtReach,
            }),
            Err(ObjectError::FamilyBehaviorDestructionBoundary(actor))
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn reached_target_failure_retains_exit_and_reverse_release_prefix() {
    let (mut arena, actor) = fixture(false, 7); // Actual4692F0 preferred-target exit.
    for slot in FamilyTargetSlot::SELECTOR12 {
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
    let targets = arena.selector12_actor(actor).unwrap().targets_26c_284;
    let middle = targets[1].unwrap();
    arena.release_actor_service(middle).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    let probe = arena.family429e20_actor(actor).unwrap().probe_234.clone();
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
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29c, 0);
    assert_eq!(
        arena.selector12_actor(actor).unwrap().targets_26c_284,
        targets
    );
    assert!(matches!(
        arena.actor_service_heap().state(targets[0].unwrap()),
        Ok(ActorServiceResourceState::Target(_))
    ));
    for reference in targets[1..].iter().flatten() {
        assert_eq!(
            arena.actor_service_heap().state(*reference),
            Ok(ActorServiceResourceState::Released)
        );
    }
    assert_eq!(arena.family429e20_actor(actor).unwrap().probe_234, probe);
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(actor));
}
