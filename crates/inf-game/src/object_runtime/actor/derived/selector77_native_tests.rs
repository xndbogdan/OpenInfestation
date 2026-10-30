//! Complete original selector-77 constructor, inherited initializer/command,
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
        ActorRegistryConfig, ActorTerrainInputs, BaseContext, RotatingVisualContext,
    },
};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn snapshot(arena: &ObjectArena, objects: &BTreeMap<u32, ObjectId>, rng: u32) -> Value {
    let mut state = common::snapshot(arena, objects, rng);
    for value in state["objects"].as_array_mut().unwrap() {
        let actor = arena
            .selector77_actor(objects[&word(&value["id"])])
            .unwrap();
        assert_eq!(
            (actor.word_19e, actor.word_1a0, actor.word_1ac),
            (None, None, None)
        );
        value["selector77"] = json!({
            "words_184_190":actor.words_184_190,
            "position_192":actor.position_192,
            "word_198":actor.word_198,
            "word_19c":actor.word_19c,
            "word_1a4":actor.word_1a4,
            "word_1a8":actor.word_1a8,
            "handle_1b0":actor.handle_1b0.raw(),
        });
    }
    state
}

#[test]
fn selector77_matches_complete_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector77_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 8);
    assert_eq!(fixture["coverage"]["snapshots"], 56);
    for address in ["00407410", "00407490", "00408090", "00426BF0", "00426C80"] {
        assert_eq!(fixture["coverage"]["direct_native_entries"][address], 8);
    }
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
                        .allocate_selector77_actor(
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
                "command" => {
                    let code = step["code"].as_i64().unwrap() as i32;
                    let mut bytes = code.to_le_bytes().to_vec();
                    for value in step["payload_dwords"].as_array().unwrap() {
                        bytes.extend_from_slice(&word(value).to_le_bytes());
                    }
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                        source: &bytes,
                        main_payload: ByteSpan::new(0, bytes.len()),
                        relocations: &[],
                        command_offset: 0,
                        target: InstanceCommandTarget::Constructor(77),
                    })
                    .unwrap();
                    assert_eq!(
                        arena
                            .apply_derived_actor_command(
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
                    .finish_selector77_initialization(objects[&id])
                    .unwrap(),
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected selector77 operation {operation}"),
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
    assert_eq!(compared, 56);
}
