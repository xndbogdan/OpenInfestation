//! All admitted history phases compare semantic known spans at every return
//! or reached stop. Native pointer/alias faults and copied physical unknown
//! bits remain source diagnostics, not invented portable allocation values.

use super::*;
use crate::object_runtime::ActorRegistryConfig;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) fn word(value: &Value) -> Option<u32> {
    value.as_u64().map(|v| v as u32)
}
pub(super) fn record(value: &Value) -> FamilyHelperHistoryRecord {
    FamilyHelperHistoryRecord {
        handle: word(&value["handle"]).map(ActorHandle::from_raw),
        normal_bits: std::array::from_fn(|i| word(&value["normal_bits"][i])),
        weight: word(&value["weight"]).map(|v| v as u16),
        padding: word(&value["padding"]).map(|v| v as u16),
    }
}
pub(super) fn snapshot(helper: &FamilyHelper482ff0) -> Value {
    json!({"header00_04":helper.words_00_04,"count08":helper.word_08,
        "records":helper.records.map(|r|json!({"handle":r.handle.map(ActorHandle::raw),
            "normal_bits":r.normal_bits,"weight":r.weight,"padding":r.padding}))})
}
fn compare_stop(result: Result<(), FamilyHelperHistoryError>, expected: &Value, name: &str) {
    let Some(stop) = expected["stopped"].as_str() else {
        assert_eq!(result, Ok(()), "{name}");
        return;
    };
    match (stop, result.expect_err(name)) {
        ("process_null", FamilyHelperHistoryError::MissingProcessNullHandle)
        | ("registry", FamilyHelperHistoryError::Object(ObjectError::MissingActorRegistry)) => {}
        (stop, FamilyHelperHistoryError::Uninitialized { offset, .. })
            if stop == format!("unknown_helper_{offset:02x}") => {}
        (stop, error) => panic!("{name}: native {stop}, Rust {error:?}"),
    }
}

#[test]
fn unchanged_native_constructor_record_fade_clear_match_every_portable_history() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_helper_history_native.json"
    )))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 590);
    assert_eq!(fixture["portable_cases"], 573);
    let mut histories = 0;
    let mut steps = 0;
    for case in cases {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert_eq!(case["instruction_bytes_unchanged"], true, "{name}");
        assert_eq!(case["referents_unchanged"], true, "{name}");
        if input["portable_compare"] != true {
            assert!(input["source_only_reason"].as_str().is_some(), "{name}");
            continue;
        }
        histories += 1;
        // Registry null and the separately unavailable clear scalar are
        // independent supplied sources. No preflight is introduced.
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(word(&input["registry_cache_handle"]).unwrap()),
        })
        .unwrap();
        let actor = arena
            .allocate_family428650_actor(tests::construction(), &mut 0)
            .unwrap();
        let mut referents = BTreeMap::new();
        for spec in input["referents"].as_array().unwrap() {
            let id = arena.allocate_actor(tests::construction().actor).unwrap();
            let old_handle = arena.actor(id).unwrap().handle;
            arena.actor_handles.as_mut().unwrap().release(old_handle);
            arena.actor_mut(id).unwrap().word_c4 = word(&spec["lifecycle"]).unwrap();
            referents.insert(spec["identity"].as_str().unwrap(), id);
        }
        for spec in input["referents"].as_array().unwrap() {
            if spec["resolves"] == true {
                let id = referents[spec["identity"].as_str().unwrap()];
                arena
                    .actor_handles
                    .as_mut()
                    .unwrap()
                    .bind(ActorHandle::from_raw(word(&spec["handle"]).unwrap()), id)
                    .unwrap();
            }
        }
        if let Some(identity) = input["registry_cache_referent"].as_str() {
            assert_eq!(
                arena
                    .actor_handles
                    .as_mut()
                    .unwrap()
                    .resolve(ActorHandle::from_raw(
                        word(&input["registry_cache_handle"]).unwrap()
                    )),
                Some(referents[identity])
            );
        }
        if input["registry_available"] == false {
            arena.actor_handles = None;
        }
        let mut helper = FamilyHelper482ff0 {
            words_00_04: std::array::from_fn(|i| word(&input["header00_04"][i])),
            word_08: word(&input["count08"]).unwrap(),
            records: std::array::from_fn(|i| record(&input["records"][i])),
        };
        if input["construct"] == true {
            helper.initialize();
            assert_eq!(
                snapshot(&helper),
                case["constructor"]["storage"],
                "{name}: constructor"
            );
        }
        arena.family428650_actor_mut(actor).unwrap().helper_218 = Some(Box::new(helper));
        let inputs = FamilyHelperHistoryInputs {
            process_null_handle: word(&input["process_null_handle"]).map(ActorHandle::from_raw),
        };
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            steps += 1;
            let request = &step["request"];
            let result = match request["phase"].as_str().unwrap() {
                "record" => arena.record_family_helper_history(
                    actor,
                    FamilyHelperRecordRequest {
                        handle: ActorHandle::from_raw(word(&request["handle"]).unwrap()),
                        displacement_words: std::array::from_fn(|i| {
                            word(&request["displacement_words"][i]).unwrap() as u16
                        }),
                        native_weight: word(&request["native_weight"]).unwrap(),
                    },
                    inputs,
                ),
                "fade" => arena.fade_family_helper_history(
                    actor,
                    word(&request["microseconds"]).unwrap(),
                    inputs,
                ),
                "clear" => arena.clear_family_helper_history(actor, inputs),
                _ => unreachable!(),
            };
            compare_stop(result, step, name);
            assert_eq!(
                snapshot(arena.family_helper_history(actor).unwrap()),
                step["storage"],
                "{name}: step {index}"
            );
        }
        assert_eq!(
            snapshot(arena.family_helper_history(actor).unwrap()),
            case["final"],
            "{name}: final"
        );
    }
    assert_eq!(histories, 573);
    assert_eq!(steps, 587);
}
