//! Native 0047F770/0047F7B0 read the actor list populated by real constructors.
use std::collections::BTreeMap;

use serde_json::Value;

use super::*;
use crate::object_runtime::BaseContext;

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

#[test]
fn placement_queries_match_original_actor_list_and_handle_results() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/actor_placement_lookup_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["snapshots"], 74);
    assert_eq!(fixture["coverage"]["direct_native_entries"]["0047F770"], 42);
    assert_eq!(fixture["coverage"]["direct_native_entries"]["0047F7B0"], 42);
    let sine = [0; 4096]; // Every case has an H12 miss, so no table read occurs.
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        assert_eq!(case["terrain"]["present"], false);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: word(&case["owner_count"]) as usize,
            initial_capacity: word(&case["initial_capacity"]) as usize,
            null_handle: ActorHandle::from_raw(word(&case["null_handle"])),
        })
        .unwrap();
        let mut objects = BTreeMap::new();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            match step["op"].as_str().unwrap() {
                "construct" => {
                    let object = arena
                        .allocate_actor(ActorConstructionRequest {
                            constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                            allocation_position: serde_json::from_value(
                                step["allocation_position"].clone(),
                            )
                            .unwrap(),
                            owner: word(&step["owner"]) as u8,
                            terrain: ActorTerrainInputs {
                                terrain: None,
                                coordinate_shift: word(&case["terrain"]["coordinate_shift"]),
                                clock: word(&case["terrain"]["clock"]),
                                world_flags: Some(word(&case["terrain"]["world_flags"])),
                                sine_quarter: &sine,
                            },
                        })
                        .unwrap();
                    assert!(objects.insert(word(&step["object"]), object).is_none());
                }
                "placement" => arena
                    .initialize_context(
                        objects[&word(&step["object"])],
                        BaseContext {
                            position_words: None,
                            placement_index: word(&step["value"]),
                        },
                    )
                    .unwrap(),
                "query" => {
                    let reference = step["reference"].as_i64().unwrap() as i32;
                    let expected = step["object"].as_u64().map(|id| objects[&(id as u32)]);
                    assert_eq!(arena.find_actor_by_placement(reference), expected);
                    assert_eq!(
                        arena.actor_handle_by_placement(reference).unwrap().raw(),
                        word(&step["handle"])
                    );
                }
                "dispose" => arena.dispose(objects[&word(&step["object"])]).unwrap(),
                "destroy" => {
                    let object = objects.remove(&word(&step["object"])).unwrap();
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: object,
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                op => panic!("unexpected operation {op}"),
            }
            assert_eq!(
                native_tests::snapshot(&arena, &objects),
                step["state"],
                "case {} step {index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 74);
}
