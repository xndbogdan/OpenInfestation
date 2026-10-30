use super::*;
use serde_json::{json, Value};

fn snapshot(target: Option<NativeTarget>, first: ObjectId, other: ObjectId) -> Value {
    target.map_or(Value::Null, |target| match target {
        NativeTarget::ActorFollow(target) => json!({
            "handle_04":target.handle_04.raw(), "focus_08":target.focus_08,
            "word_10":target.word_10, "focus_14":target.focus_14, "velocity_1a":target.velocity_1a,
        }),
        NativeTarget::FixedPosition(target) => json!({
            "handle_04":target.handle_04.raw(), "focus_08":target.focus_08, "word_10":target.word_10,
        }),
        NativeTarget::FurniturePosition(_) => panic!("unadmitted target class"),
        NativeTarget::ObjectPosition(target) => json!({
            "handle_04":target.handle_04.raw(), "focus_08":target.focus_08, "word_10":target.word_10,
            "object_14":if target.object_14==first { "first" } else { assert_eq!(target.object_14,other); "other" },
        }),
    })
}

#[test]
fn complete_publishers_match_110_unchanged_native_instruction_receipts() {
    let receipts: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/weighted_targets_native.json"
    ))
    .unwrap();
    let cases = receipts["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 110);
    for case in cases {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234_5678),
        })
        .unwrap();
        let first = arena
            .allocate_actor(allocation(Some([65535, 65530, 32768])))
            .unwrap();
        let allocated = arena.actor(first).unwrap().handle;
        arena.actor_handles.as_mut().unwrap().release(allocated);
        let handle = ActorHandle::from_raw(0x0801_0001);
        arena
            .actor_handles
            .as_mut()
            .unwrap()
            .bind(handle, first)
            .unwrap();
        arena.actor_mut(first).unwrap().handle = handle;
        arena.actor_mut(first).unwrap().word_74 = 20;
        arena.owned_mut(first).unwrap().state.base.word_28 =
            case["size_word"].as_u64().unwrap() as u16;
        if case["resolves"] == false {
            arena.actor_handles.as_mut().unwrap().release(handle);
        }
        let other = arena.allocate(ObjectClassKind::RouteNode);
        arena.owned_mut(other).unwrap().state.base.position_words = Some([7, 8, 9]);
        if case["stop_stage"] == "constructor" {
            arena.owned_mut(first).unwrap().state.base.position_words = None;
        }
        let before_first = arena.object(first).unwrap().clone();
        let before_other = arena.object(other).unwrap().clone();
        let old = match case["old_kind"].as_str() {
            None => None,
            Some("follow") => Some(NativeTarget::ActorFollow(ActorFollowTarget {
                handle_04: handle,
                focus_08: Some([11, 12, 13]),
                word_10: 2,
                focus_14: Some([21, 22, 23]),
                velocity_1a: Some([31, 32, 33]),
            })),
            Some("object") => Some(NativeTarget::ObjectPosition(ObjectPositionTarget {
                handle_04: arena.actor_handles().unwrap().null_handle(),
                focus_08: [11, 12, 13],
                word_10: 16,
                object_14: first,
            })),
            _ => panic!("unsupported receipt target"),
        };
        let referent = match case["referent"].as_str() {
            None => None,
            Some("first") => Some(first),
            Some("other") => Some(other),
            _ => panic!(),
        };
        let old_ref = old.map(|data| arena.import_fixture_target(data));
        let mut pair =
            NativeWeightedTarget::new(old_ref, case["old_score"].as_u64().unwrap() as u32);
        let request = WeightedTargetRequest {
            constructor_null: WeightedTargetConstructorNull::ActorRegistry,
            referent: if case["publisher"] == "actor" {
                WeightedTargetReferent::Actor(referent)
            } else {
                WeightedTargetReferent::Object(referent)
            },
            score: case["score"].as_u64().unwrap() as u32,
        };
        arena
            .actor_service_heap_mut()
            .script([if case["stop_stage"] == "allocation" {
                FixtureActorServiceAllocation::Error(71)
            } else if case["allocation_fails"] == true {
                FixtureActorServiceAllocation::Null
            } else {
                FixtureActorServiceAllocation::Allocate
            }]);
        let allocation_start = arena.actor_service_heap().attempts().len();
        let result = arena.publish_weighted_target(&mut pair, request);
        if case["stop_stage"] == "allocation" {
            assert_eq!(
                result,
                Err(NativeTargetError::Heap(
                    ActorServiceHeapError::FixtureFailure(71)
                )),
                "{case}"
            );
        } else if case["stop_stage"] == "constructor" {
            let error = if case["publisher"] == "actor" {
                ObjectError::ActorPositionUnknown(first)
            } else {
                ObjectError::TargetObjectPositionUnknown(first)
            };
            assert_eq!(result, Err(NativeTargetError::Object(error)), "{case}");
        } else {
            assert_eq!(
                u64::from(result.unwrap()),
                case["returned"].as_u64().unwrap(),
                "{case}"
            );
        }
        let decoded = if case["custody"] == "released" {
            assert_eq!(pair.target, old_ref, "{case}");
            let reference = old_ref.expect("released receipt retains its old pointer");
            assert_eq!(
                arena.actor_service_heap().state(reference),
                Ok(ActorServiceResourceState::Released),
                "{case}"
            );
            assert!(pair.native_pointer_present());
            None
        } else {
            pair.target
                .map(|reference| arena.native_target(reference).unwrap())
        };
        assert_eq!(
            pair.score,
            case["final_score"].as_u64().unwrap() as u32,
            "{case}"
        );
        assert_eq!(snapshot(decoded, first, other), case["target"], "{case}");
        let native_allocations: Vec<u32> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "allocate")
            .map(|event| event["bytes"].as_u64().unwrap() as u32)
            .collect();
        assert_eq!(
            allocation_classes(&arena, allocation_start)
                .iter()
                .map(|class| class.allocation_bytes())
                .collect::<Vec<_>>(),
            native_allocations,
            "{case}"
        );
        assert_eq!(arena.object(first).unwrap(), &before_first);
        assert_eq!(arena.object(other).unwrap(), &before_other);
        assert_eq!(case["referents_unchanged"], true);
    }
}
