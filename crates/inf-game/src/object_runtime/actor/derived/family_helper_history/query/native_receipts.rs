//! Compare every supplied query success and reached missing-span stop.
//! Native aliases, vector-pointer faults and overcapacity storage remain
//! diagnostics; all known f32 patterns execute the masked numeric policy.

use super::super::native_receipts::{record, snapshot, word};
use super::*;
use crate::object_runtime::{ActorRegistryConfig, FamilyHelper482ff0};
use serde_json::Value;

#[test]
fn unchanged_native_helper_query_matches_every_portable_receipt_without_writes() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_helper_query_native.json"
    )))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 525);
    assert_eq!(fixture["portable_cases"], 514);
    let mut compared = 0;
    for case in cases {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert_eq!(case["instruction_bytes_unchanged"], true, "{name}");
        assert_eq!(case["storage_unchanged"], true, "{name}");
        if input["portable_compare"] != true {
            assert!(input["source_only_reason"].as_str().is_some(), "{name}");
            continue;
        }
        compared += 1;
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let actor = arena
            .allocate_family428650_actor(super::super::tests::construction(), &mut 0)
            .unwrap();
        arena.actor_handles = None;
        let helper = FamilyHelper482ff0 {
            words_00_04: std::array::from_fn(|i| word(&input["header00_04"][i])),
            word_08: word(&input["count08"]).unwrap(),
            records: std::array::from_fn(|i| record(&input["records"][i])),
        };
        let before = helper.clone();
        arena.family428650_actor_mut(actor).unwrap().helper_218 = Some(Box::new(helper));
        let request = &input["request"];
        let result = arena.query_family_helper_history(
            actor,
            FamilyHelperQueryRequest {
                handle: ActorHandle::from_raw(word(&request["handle"]).unwrap()),
                displacement_words: std::array::from_fn(|i| {
                    word(&request["displacement_words"][i]).unwrap() as u16
                }),
            },
        );
        match case["stopped"].as_str() {
            None => assert_eq!(result, Ok(word(&case["result"]).unwrap() != 0), "{name}"),
            Some(stop) => match result.expect_err(name) {
                FamilyHelperHistoryError::Uninitialized { offset, .. } => {
                    assert_eq!(stop, format!("unknown_helper_{offset:02x}"), "{name}")
                }
                error => panic!("{name}: native {stop}, Rust {error:?}"),
            },
        }
        let after = arena.family_helper_history(actor).unwrap();
        assert_eq!(after, &before, "{name}: no writes on success or error");
        assert_eq!(
            snapshot(after),
            case["storage"],
            "{name}: native known spans"
        );
    }
    assert_eq!(compared, 514);
}
