use super::*;
use crate::object_runtime::targets::{
    ActorFollowTarget, FixedPositionTarget, ObjectPositionTarget,
};
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyBehaviorProfile,
};
use serde_json::{json, Value};

const SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap()
}

fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([65535, 32768, 7]),
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

fn words(case: &Value, name: &str) -> [u16; 3] {
    std::array::from_fn(|i| case[name][i].as_u64().unwrap() as u16)
}

fn old_target(case: &Value, actor: ObjectId) -> Option<NativeTarget> {
    let handle = ActorHandle::from_raw(case["old_handle"].as_u64().unwrap() as u32);
    let kind = case["old_kind_word"].as_u64().unwrap() as u32;
    Some(match case["old_class"].as_str()? {
        "fixed" => NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: handle,
            focus_08: Some([11, 12, 13]),
            word_10: kind,
        }),
        "follow" => NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: handle,
            focus_08: Some([11, 12, 13]),
            word_10: kind,
            focus_14: Some([21, 22, 23]),
            velocity_1a: Some([31, 32, 33]),
        }),
        "object" => NativeTarget::ObjectPosition(ObjectPositionTarget {
            handle_04: handle,
            focus_08: [11, 12, 13],
            word_10: kind,
            object_14: actor,
        }),
        _ => panic!("unsupported native target class"),
    })
}

fn snapshot(target: Option<NativeTarget>) -> Value {
    target.map_or(Value::Null, |target| match target {
        NativeTarget::FixedPosition(target) => json!({"class_name":"fixed",
            "handle_04":target.handle_04.raw(),"focus_08":target.focus_08,"word_10":target.word_10}),
        NativeTarget::ActorFollow(target) => json!({"class_name":"follow",
            "handle_04":target.handle_04.raw(),"focus_08":target.focus_08,"word_10":target.word_10,
            "focus_14":target.focus_14,"velocity_1a":target.velocity_1a}),
        NativeTarget::ObjectPosition(target) => json!({"class_name":"object",
            "handle_04":target.handle_04.raw(),"focus_08":target.focus_08,"word_10":target.word_10,
            "object_14":"actor"}),
        NativeTarget::FurniturePosition(_) => panic!("unadmitted target class"),
    })
}

fn profile(name: &str) -> FamilyBehaviorProfile {
    match name {
        "family-mode0" => FamilyBehaviorProfile::Family4236d0 { mode: 0 },
        "family-mode1" => FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        "family-mode2" => FamilyBehaviorProfile::Family4236d0 { mode: 2 },
        "selector12-absent" => FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        "selector12-present" => FamilyBehaviorProfile::Selector12 {
            behavior_present: true,
        },
        "selector35" => FamilyBehaviorProfile::Selector35,
        _ => panic!("unknown native profile"),
    }
}

#[test]
fn point_changes_and_stay_updates_match_138_original_literal_wrapper_receipts() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_descriptor_points_native.json"
    ))
    .unwrap();
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 138);
    for case in cases {
        let mut arena = arena();
        let mut construction_rng = 0x8765_4321;
        let actor = arena
            .allocate_family428650_actor(request(), &mut construction_rng)
            .unwrap();
        let rng_before = construction_rng;
        let descriptor = profile(case["profile"].as_str().unwrap())
            .descriptor(case["index"].as_i64().unwrap() as i32)
            .unwrap();
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.position_222 = Some(words(case, "home"));
        state.words_188_19c[1] = Some(case["timer"].as_u64().unwrap() as u32);
        state.word_228 = case["status"].as_u64().unwrap() as u32;
        let target = old_target(case, actor).map(|data| arena.import_fixture_target(data));
        arena.derived_actor_mut(actor).unwrap().target_148 = target;
        let stop = case["stop"].as_str();
        if stop == Some("process_null") {
            arena.actor_handles = None;
        } else if stop == Some("point") {
            if descriptor.change_address == 0x435190 {
                arena.family428650_actor_mut(actor).unwrap().position_222 = None;
            } else {
                arena.owned_mut(actor).unwrap().state.base.position_words = None;
            }
        }
        let base_before = arena.object(actor).unwrap().base.clone();
        let mut family_before = arena.family428650_actor(actor).unwrap().clone();
        arena
            .actor_service_heap_mut()
            .script([if stop == Some("allocate") {
                crate::object_runtime::FixtureActorServiceAllocation::Error(71)
            } else if case["allocation_null"] == true {
                crate::object_runtime::FixtureActorServiceAllocation::Null
            } else {
                crate::object_runtime::FixtureActorServiceAllocation::Allocate
            }]);
        let attempts_before = arena.actor_service_heap().attempts().len();
        if case["operation"] == "change" {
            assert_eq!(
                descriptor.change_address as u64,
                case["callback"].as_u64().unwrap()
            );
            let request = FamilyPointTargetChangeRequest::from_descriptor(
                descriptor,
                case["entering"].as_u64().unwrap() as u8 != 0,
            )
            .unwrap();
            let result = arena.change_family_point_target(
                actor,
                request,
                crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry,
            );
            let expected = match stop {
                Some("allocate") => Err(FamilyPointTargetChangeError::Target(
                    NativeTargetError::Heap(
                        crate::object_runtime::ActorServiceHeapError::FixtureFailure(71),
                    ),
                )),
                Some("process_null") => Err(FamilyPointTargetChangeError::Target(
                    NativeTargetError::Object(ObjectError::MissingActorRegistry),
                )),
                Some("point") if request.kind == FamilyPointTargetChangeKind::Home => {
                    Err(FamilyPointTargetChangeError::UninitializedHome(actor))
                }
                Some("point") => Err(FamilyPointTargetChangeError::Object(
                    ObjectError::ActorPositionUnknown(actor),
                )),
                _ => Ok(()),
            };
            assert_eq!(result, expected, "{case}");
        } else {
            assert_eq!(descriptor.update_address, 0x425e20);
            assert_eq!(
                arena.update_family_stay_still_target(actor),
                if stop == Some("point") {
                    Err(ObjectError::ActorPositionUnknown(actor))
                } else {
                    Ok(())
                },
                "{case}"
            );
        }
        assert_eq!(
            snapshot(
                arena
                    .derived_actor(actor)
                    .unwrap()
                    .target_148
                    .map(|reference| arena.native_target(reference).unwrap())
            ),
            case["target"],
            "{case}"
        );
        let family_after = arena.family428650_actor(actor).unwrap();
        assert_eq!(
            family_after.word_228 as u64,
            case["final_status"].as_u64().unwrap(),
            "{case}"
        );
        assert_eq!(
            family_after.words_188_19c[1].unwrap() as u64,
            case["final_timer"].as_u64().unwrap(),
            "{case}"
        );
        family_before.word_228 = family_after.word_228;
        family_before.words_188_19c[1] = family_after.words_188_19c[1];
        assert_eq!(
            &family_before, family_after,
            "other family fields changed: {case}"
        );
        assert_eq!(arena.object(actor).unwrap().base, base_before);
        assert_eq!(construction_rng, rng_before);
        let allocations: Vec<u32> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["bytes"].as_u64().unwrap() as u32)
            .collect();
        let actual: Vec<_> = arena.actor_service_heap().attempts()[attempts_before..]
            .iter()
            .map(|kind| kind.allocation_bytes() as u32)
            .collect();
        assert_eq!(actual, allocations, "{case}");
        assert_eq!(case["padding_untouched"], true);
        assert_eq!(case["rng_unchanged"], true);
    }
}

#[test]
fn zero_entry_and_actual_allocator_null_bypass_unavailable_sources() {
    let mut arena = ObjectArena::new();
    let invalid = arena.allocate(crate::object_runtime::ObjectClassKind::RouteNode);
    arena.actor_service_heap_mut().script([
        crate::object_runtime::FixtureActorServiceAllocation::Error(71),
    ]);
    for kind in [
        FamilyPointTargetChangeKind::Roam,
        FamilyPointTargetChangeKind::Home,
        FamilyPointTargetChangeKind::StayStill,
    ] {
        assert_eq!(
            arena.change_family_point_target(
                invalid,
                FamilyPointTargetChangeRequest {
                    kind,
                    entering: false
                },
                crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry
            ),
            Ok(())
        );
    }
    assert!(arena.actor_service_heap().attempts().is_empty());
    let mut arena = self::arena();
    let actor = arena
        .allocate_family428650_actor(request(), &mut 0)
        .unwrap();
    arena.actor_handles = None;
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    arena.family428650_actor_mut(actor).unwrap().position_222 = None;
    for kind in [
        FamilyPointTargetChangeKind::Roam,
        FamilyPointTargetChangeKind::Home,
        FamilyPointTargetChangeKind::StayStill,
    ] {
        arena
            .actor_service_heap_mut()
            .script([crate::object_runtime::FixtureActorServiceAllocation::Null]);
        let before = arena.actor_service_heap().attempts().len();
        assert_eq!(
            arena.change_family_point_target(
                actor,
                FamilyPointTargetChangeRequest {
                    kind,
                    entering: true
                },
                crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry
            ),
            Ok(())
        );
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
        assert_eq!(
            arena.actor_service_heap().attempts()[before..],
            [crate::object_runtime::ActorServiceResourceKind::Target(
                NativeTargetClass::FixedPosition
            )]
        );
    }
}
