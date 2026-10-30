//! No-growth registry receipts from complete native routines, plus explicitly
//! bounded initialization-tail and failed-reassignment mutation frontiers.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{ObjectArena, ObjectClassKind};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn handle(value: &Value) -> ActorHandle {
    ActorHandle::from_raw(word(value))
}

fn object_id(value: &Value, objects: &[ObjectId]) -> Option<ObjectId> {
    value.as_u64().map(|index| objects[index as usize])
}

fn normalized_object(object: Option<ObjectId>, objects: &[ObjectId]) -> Option<usize> {
    object.map(|object| objects.iter().position(|&id| id == object).unwrap())
}

fn snapshot(
    registry: &ActorHandles,
    objects: &[ObjectId],
    actor_words: &BTreeMap<usize, ActorHandle>,
) -> Value {
    let owners: Vec<_> = registry
        .owners()
        .iter()
        .enumerate()
        .map(|(owner, table)| {
            let slots: Vec<_> = table
                .slots()
                .iter()
                .map(|slot| {
                    json!({
                        "handle":slot.handle.raw(),
                        "object":normalized_object(slot.object, objects),
                        "next_free":slot.next_free,
                    })
                })
                .collect();
            json!({
                "owner":owner,
                "word_00":table.word_00,
                "word_04":table.word_04,
                "free_head":table.free_head,
                "free_tail":table.free_tail,
                "slots":slots,
            })
        })
        .collect();
    let cache = registry.cache();
    let object_handles: Vec<_> = actor_words
        .iter()
        .map(|(&object, handle)| json!({"object":object,"handle":handle.raw()}))
        .collect();
    json!({
        "owners":owners,
        "cache":{"handle":cache.handle.raw(),"object":normalized_object(cache.object,objects)},
        "object_handles":object_handles,
    })
}

#[test]
fn registry_operations_match_native_tables_cache_and_caller_words() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/actor_handles_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 17);
    assert_eq!(fixture["coverage"]["snapshots"], 127);
    assert_eq!(fixture["coverage"]["initialization_tail_executions"], 63);
    assert_eq!(fixture["coverage"]["completed_direct_entry_count"], 97);
    assert_eq!(
        fixture["coverage"]["diagnostic_frontier_entries"]["0047E2B0"],
        3
    );

    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["id"].as_str().unwrap();
        let mut registry = ActorHandles::new(
            word(&case["owner_count"]) as usize,
            word(&case["initial_capacity"]) as usize,
            handle(&case["null_handle"]),
        )
        .unwrap();
        let mut arena = ObjectArena::new();
        // These IDs stand for the oracle's opaque preallocated pointer payloads.
        // Common-object creation here supplies valid IDs, not native actor state.
        let objects: Vec<_> = (0..10)
            .map(|_| arena.allocate(ObjectClassKind::Base))
            .collect();
        let mut actor_words = BTreeMap::new();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let mut result = None;
            match step["op"].as_str().unwrap() {
                "initialize_tail" => {
                    // The original allocation call is outside this receipt.
                    // Compare logical constructor state to its executed tail.
                }
                "seed_slot" => {
                    let slot = &mut registry.owners[word(&step["owner"]) as usize].slots
                        [word(&step["slot"]) as usize];
                    if step.get("handle").is_some() {
                        slot.handle = handle(&step["handle"]);
                    }
                    if step.get("object").is_some() {
                        slot.object = object_id(&step["object"], &objects);
                    }
                    if let Some(next) = step.get("next_free") {
                        slot.next_free = next.as_i64().unwrap().try_into().unwrap();
                    }
                }
                "seed_owner" => {
                    let owner = &mut registry.owners[word(&step["owner"]) as usize];
                    for (name, field) in [
                        ("word_00", &mut owner.word_00),
                        ("word_04", &mut owner.word_04),
                        ("free_head", &mut owner.free_head),
                        ("free_tail", &mut owner.free_tail),
                    ] {
                        if let Some(value) = step.get(name) {
                            *field = value.as_i64().unwrap().try_into().unwrap();
                        }
                    }
                }
                "seed_cache" => {
                    registry.cache = ActorHandleCache {
                        handle: handle(&step["handle"]),
                        object: object_id(&step["object"], &objects),
                    };
                }
                "seed_actor_handle" => {
                    actor_words.insert(word(&step["object"]) as usize, handle(&step["handle"]));
                }
                "allocate" => {
                    let allocated = registry
                        .allocate(
                            word(&step["owner_dword"]) as u8,
                            object_id(&step["object"], &objects).unwrap(),
                        )
                        .unwrap();
                    result = Some(json!({"handle":allocated.raw()}));
                }
                "bind" => {
                    registry
                        .bind(
                            handle(&step["handle"]),
                            object_id(&step["object"], &objects).unwrap(),
                        )
                        .unwrap();
                }
                "resolve" => {
                    let resolved = registry.resolve(handle(&step["handle"]));
                    result = Some(json!({"object":normalized_object(resolved,&objects)}));
                }
                "release" => {
                    // Native is void; this compares effects, not a return ABI.
                    registry.release(handle(&step["handle"]));
                }
                "mark" => {
                    let mut marked = handle(&step["handle"]);
                    registry.mark_mode1(&mut marked);
                    result = Some(json!({"handle":marked.raw()}));
                }
                "mode2" => {
                    let admissible = registry.mode2_admissible(handle(&step["handle"])).unwrap();
                    result = Some(json!({"admissible":admissible}));
                }
                "reassign" => {
                    let object = word(&step["object"]) as usize;
                    let new = handle(&step["handle"]);
                    let outcome = registry.reassign(
                        objects[object],
                        actor_words.get_mut(&object).unwrap(),
                        new,
                    );
                    if step["frontier"].as_bool().unwrap_or(false) {
                        // Native stops before its first diagnostic call. The
                        // port's typed error retains all preceding mutations.
                        let expected = match name {
                            "reassign_owner_zero" => ActorHandleError::Owner(0),
                            "reassign_slot_zero" => ActorHandleError::ReservedSlot(new),
                            "reassign_already_bound" => ActorHandleError::AlreadyBound(new),
                            _ => panic!("unclassified diagnostic frontier {name}"),
                        };
                        assert_eq!(outcome, Err(expected));
                        result = Some(json!({"stopped_before":"00439230"}));
                    } else {
                        outcome.unwrap();
                    }
                }
                unknown => panic!("unknown native operation {unknown}"),
            }
            assert_eq!(
                result.as_ref(),
                step.get("result"),
                "{name} step{index} result"
            );
            assert_eq!(
                snapshot(&registry, &objects, &actor_words),
                step["state"],
                "{name} step{index} state"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 127);
}
