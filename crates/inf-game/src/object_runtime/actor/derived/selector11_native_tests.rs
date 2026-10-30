//! Complete original selector-11 constructor, initializer, specialized commands,
//! guard miss, postinitializer and destructor. Allocating guard cases have
//! separate coverage; these are not full world-factory or update receipts.

use std::{collections::BTreeMap, path::Path};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, ByteSpan, InstanceCommandRequest,
    InstanceCommandTarget,
};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::{
            derived::native_tests as common, ActorCommandContext, ActorConstructionRequest,
            ActorInitializationRequest,
        },
        native_model_fixture::{Fixture, Models},
        ActorHandle, ActorRegistryConfig, ActorTerrainInputs, BaseContext, RotatingVisualContext,
    },
};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn snapshot(arena: &ObjectArena, objects: &BTreeMap<u32, ObjectId>, rng: u32) -> Value {
    let mut state = common::snapshot(arena, objects, rng);
    for value in state["objects"].as_array_mut().unwrap() {
        let actor = arena
            .selector11_actor(objects[&word(&value["id"])])
            .unwrap();
        assert_eq!(
            (
                actor.word_184,
                actor.word_19c,
                actor.word_1aa,
                actor.word_1ac
            ),
            (None, None, None, None)
        );
        value["selector11"] = json!({
            "word_188":actor.word_188,
            "word_18c":actor.word_18c,
            "word_190":actor.word_190,
            "word_194":actor.word_194,
            "word_198":actor.word_198,
            "word_1a0":actor.word_1a0,
            "byte_1a2":actor.byte_1a2,
            "byte_1a3":actor.byte_1a3,
            "word_1a4":actor.word_1a4,
            "word_1a6":actor.word_1a6,
            "word_1a8":actor.word_1a8,
            "word_1ae":actor.word_1ae,
            "word_1b0":actor.word_1b0,
            "word_1b2":actor.word_1b2,
        });
    }
    state
}

#[test]
fn selector11_matches_complete_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector11_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 8);
    assert_eq!(fixture["coverage"]["snapshots"], 144);
    for address in ["004014F0", "00401590", "00402590", "00426BF0", "00426C80"] {
        assert_eq!(fixture["coverage"]["direct_native_entries"][address], 8);
    }
    assert_eq!(
        fixture["coverage"]["direct_native_entries"]["00402330"],
        104
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let math = NativeBasisMath::new(sine);
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let resident = Fixture::new(&serde_json::from_value::<Models>(case["h8"].clone()).unwrap());
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: word(&case["owner_count"]) as usize,
            initial_capacity: word(&case["initial_capacity"]) as usize,
            null_handle: ActorHandle::from_raw(word(&case["null_handle"])),
        })
        .unwrap();
        let terrain = ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &sine,
        };
        let mut objects = BTreeMap::new();
        let mut rng = 0;
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let id = word(&step["object"]);
            match step["op"].as_str().unwrap() {
                "construct" => {
                    rng = word(&step["rng_seed"]);
                    let object = arena
                        .allocate_selector11_actor(
                            DerivedActorConstructionRequest {
                                actor: ActorConstructionRequest {
                                    constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                                    allocation_position: serde_json::from_value(
                                        step["allocation_position"].clone(),
                                    )
                                    .unwrap(),
                                    owner: word(&step["owner"]) as u8,
                                    terrain,
                                },
                                world_mask_limits: serde_json::from_value(step["limits"].clone())
                                    .unwrap(),
                            },
                            &mut rng,
                        )
                        .unwrap();
                    assert!(objects.insert(id, object).is_none());
                }
                "initialize" => {
                    let payload = common::payload(&step["payload"], &step["extension"]);
                    arena
                        .initialize_derived_actor(
                            objects[&id],
                            ActorInitializationRequest {
                                context: RotatingVisualContext {
                                    base: BaseContext {
                                        position_words: serde_json::from_value(
                                            step["position_words"].clone(),
                                        )
                                        .unwrap(),
                                        placement_index: word(&step["placement_index"]),
                                    },
                                    model_argument: Some(word(&step["model_argument"])),
                                    rotation_words: serde_json::from_value(
                                        step["rotation_words"].clone(),
                                    )
                                    .unwrap(),
                                },
                                payload: &payload,
                                local_owner: 1,
                            },
                            &resident.runtime,
                            &math,
                        )
                        .unwrap();
                }
                "guard" => assert_eq!(
                    arena.admit_derived_actor(objects[&id]).unwrap(),
                    step["accepted"].as_bool().unwrap()
                ),
                "command" | "command_bytes" => {
                    let code = step["code"].as_i64().unwrap() as i32;
                    let mut bytes = code.to_le_bytes().to_vec();
                    if let Some(values) = step["payload_dwords"].as_array() {
                        for value in values {
                            bytes.extend_from_slice(&word(value).to_le_bytes());
                        }
                    } else {
                        for value in step["payload_bytes"].as_array().unwrap() {
                            bytes.push(word(value) as u8);
                        }
                    }
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Dispatch,
                        source: &bytes,
                        main_payload: ByteSpan::new(0, bytes.len()),
                        relocations: &[],
                        command_offset: 0,
                        target: InstanceCommandTarget::Constructor(11),
                    })
                    .unwrap();
                    assert_eq!(
                        arena
                            .apply_selector11_command(
                                objects[&id],
                                &view,
                                ActorCommandContext {
                                    terrain: None,
                                    runtime: &resident.runtime,
                                    first_player_grants: None,
                                }
                            )
                            .unwrap(),
                        step["accepted"].as_bool().unwrap()
                    );
                }
                "post_initialize" => arena
                    .finish_selector11_initialization(objects[&id])
                    .unwrap(),
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected selector11 operation {operation}"),
            }
            assert_eq!(
                snapshot(&arena, &objects, rng),
                step["state"],
                "{} step{index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 144);
}

#[test]
fn selector11_command21_matches_separately_seeded_nonnull_behavior_receipt() {
    use crate::object_runtime::{
        actor::{behavior::build_actor_behavior, native_tests as actor_native, ActorExtension},
        ObjectClass,
    };

    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector11_command21_seeded_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 5);
    let resident = Fixture::new(
        &serde_json::from_value::<Models>(json!({"capacity":0,"entries":[]})).unwrap(),
    );
    let sine = [0; 4096]; // H12 is absent; the command reads no terrain or model.
    for case in fixture["cases"].as_array().unwrap() {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        // This is host setup only. The native fixture starts with explicitly
        // supplied actor/behavior storage and claims no constructor or guard.
        let object = arena
            .allocate_selector11_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: None,
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &sine,
                        },
                    },
                    world_mask_limits: [0, 0],
                },
                &mut 0,
            )
            .unwrap();
        let mut behavior = build_actor_behavior(1, 0, 3).unwrap();
        behavior.word_04 = word(&case["before"]["word_04"]);
        behavior.word_08 = word(&case["before"]["word_08"]);
        behavior.byte_0c = word(&case["before"]["byte_0c"]) as u8;
        arena.derived_actor_mut(object).unwrap().behavior_140 = Some(behavior);
        let objects = BTreeMap::from([(1, object)]);
        let unchanged_shared = actor_native::snapshot(&arena, &objects);
        let mut expected = arena.object(object).unwrap().clone();
        let ObjectClass::Actor(actor) = &mut expected.class else {
            unreachable!()
        };
        let ActorExtension::Derived(derived) = &mut actor.extension else {
            unreachable!()
        };
        let behavior = derived.behavior_140.as_mut().unwrap();
        behavior.word_04 = word(&case["after"]["word_04"]);
        behavior.word_08 = word(&case["after"]["word_08"]);
        behavior.byte_0c = word(&case["after"]["byte_0c"]) as u8;

        let mut bytes = 21_i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&word(&case["operand"]).to_le_bytes());
        let command = decode_instance_command(InstanceCommandRequest {
            source: &bytes,
            main_payload: ByteSpan::new(0, bytes.len()),
            relocations: &[],
            command_offset: 0,
            target: InstanceCommandTarget::Constructor(11),
            read_scope: inf_formats::InstanceCommandReadScope::Dispatch,
        })
        .unwrap();
        assert_eq!(
            arena
                .apply_selector11_command(
                    object,
                    &command,
                    ActorCommandContext {
                        terrain: None,
                        runtime: &resident.runtime,
                        first_player_grants: None,
                    },
                )
                .unwrap(),
            case["accepted"].as_bool().unwrap()
        );
        assert_eq!(arena.object(object).unwrap(), &expected);
        assert_eq!(actor_native::snapshot(&arena, &objects), unchanged_shared);
    }
}
