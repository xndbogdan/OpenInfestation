use super::*;
use crate::object_runtime::targets::{
    ActorFollowTarget, ActorServiceHeapError, ActorServiceResourceKind, ActorServiceResourceState,
    FixtureActorServiceAllocation, NativeTargetError, NativeWeightedTarget, ObjectPositionTarget,
    WeightedTargetReferent, WeightedTargetRequest,
};
use crate::object_runtime::WeightedTargetConstructorNull;
use crate::object_runtime::{
    ActorConstructionRequest, ActorExtension, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, ObjectClassKind, ObjectId,
};
use serde_json::Value;

const SINE: [u16; 4096] = [0; 4096];

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|i| value[i].as_u64().unwrap() as u16)
}

fn optional_words(value: &Value) -> Option<[u16; 3]> {
    (!value.is_null()).then(|| words(value))
}

fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([65535, 65530, 32768]),
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

fn old_target(case: &Value, first: ObjectId) -> Option<NativeTarget> {
    let handle = ActorHandle::from_raw(0x0801_0001);
    let kind = case["raw_old_kind"].as_u64().unwrap() as u32;
    let point = optional_words(&case["old_point"]);
    Some(match case["old_class"].as_str()? {
        "fixed" => NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: handle,
            focus_08: point,
            word_10: kind,
        }),
        "follow" => NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: handle,
            focus_08: point,
            word_10: kind,
            focus_14: Some([21, 22, 23]),
            velocity_1a: Some([31, 32, 33]),
        }),
        "object" => NativeTarget::ObjectPosition(ObjectPositionTarget {
            handle_04: ActorHandle::from_raw(case["process_null_handle"].as_u64().unwrap() as u32),
            focus_08: point.unwrap(),
            word_10: kind,
            object_14: first,
        }),
        _ => panic!("unknown receipt resource class"),
    })
}

fn compare(target: NativeTarget, native: &Value, first: ObjectId, case: &Value) {
    assert_eq!(
        target.handle_04().raw() as u64,
        native["handle_04"].as_u64().unwrap(),
        "{case}"
    );
    assert_eq!(
        target.kind_word() as u64,
        native["raw_kind_10"].as_u64().unwrap(),
        "{case}"
    );
    let (vtable, point) = match target {
        NativeTarget::FixedPosition(target) => (0x51f154, target.focus_08),
        NativeTarget::FurniturePosition(_) => panic!("unadmitted target class"),
        NativeTarget::ActorFollow(target) => {
            assert_eq!(
                target.focus_14,
                optional_words(&native["focus_14"]),
                "{case}"
            );
            assert_eq!(
                target.velocity_1a,
                optional_words(&native["velocity_1a"]),
                "{case}"
            );
            (0x528038, target.focus_08)
        }
        NativeTarget::ObjectPosition(target) => {
            assert_eq!(target.object_14, first, "{case}");
            assert_eq!(native["object_14"], "first", "{case}");
            (0x51fb6c, Some(target.focus_08))
        }
    };
    assert_eq!(vtable, native["vtable"].as_u64().unwrap(), "{case}");
    assert_eq!(point, optional_words(&native["point_08"]), "{case}");
}

#[derive(Clone, Copy)]
enum ConstructorPass {
    ActorRegistry,
    Process,
}

fn compare_receipts(pass: ConstructorPass) -> usize {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/fixed_targets_native.json"
    ))
    .unwrap();
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 290);
    assert_eq!(
        cases.iter().filter(|case| case["stopped"] == true).count(),
        30
    );
    let mut compared = 0;
    for case in cases {
        if case["portable_compare"] != true {
            assert!(case["source_only_reason"].as_str().is_some());
            continue;
        }
        if matches!(pass, ConstructorPass::Process) && case["operation"] != "publisher" {
            // Direct clones retain their separate registry-backed contract.
            continue;
        }
        compared += 1;
        let process_null =
            ActorHandle::from_raw(case["process_null_handle"].as_u64().unwrap() as u32);
        let registry_null = match pass {
            ConstructorPass::ActorRegistry => process_null,
            ConstructorPass::Process => ActorHandle::from_raw(process_null.raw() ^ 0xffff_ffff),
        };
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: registry_null,
        })
        .unwrap();
        let first = arena
            .allocate_family428650_actor(request(), &mut 0)
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
        let stop = case["stop_stage"].as_str();
        if stop == Some("process_null") {
            if matches!(pass, ConstructorPass::ActorRegistry) {
                arena.actor_handles = None;
            }
        } else if stop == Some("point") {
            // Supplied raw point/clone-copy stops are explicitly source-only;
            // portable point stops here are actual arena XYZ failures.
            assert!(case["publisher"] == "actor" || case["publisher"] == "object");
            arena.owned_mut(first).unwrap().state.base.position_words = None;
        }
        let before_first = arena.object(first).unwrap().clone();
        let before_other = arena.object(other).unwrap().clone();
        let old = old_target(case, first);
        let old_ref = old.map(|data| arena.import_fixture_target(data));
        arena
            .actor_service_heap_mut()
            .script([if stop == Some("allocation") {
                FixtureActorServiceAllocation::Error(71)
            } else if case["allocation_fails"] == true {
                FixtureActorServiceAllocation::Null
            } else {
                FixtureActorServiceAllocation::Allocate
            }]);
        let allocation_start = arena.actor_service_heap().attempts().len();
        if case["operation"] == "method" {
            let mut target = old.unwrap();
            let original = target;
            match case["method_index"].as_u64().unwrap() {
                0 => {
                    // Canonical deleting callback is consumed by real owner
                    // clear 00429460; raw returned self is not an arena address.
                    assert_eq!(case["delete_flags"], 1);
                    let ActorExtension::Derived(owner) =
                        &mut arena.actor_mut(first).unwrap().extension
                    else {
                        panic!("fixture owner is a derived actor");
                    };
                    owner.target_148 = old_ref;
                    arena.clear_family_current_targets(first).unwrap();
                    assert_eq!(arena.derived_actor(first).unwrap().target_148, None);
                    assert_eq!(
                        arena.actor_service_heap().state(old_ref.unwrap()),
                        Ok(ActorServiceResourceState::Released)
                    );
                    assert_eq!(case["custody"], "deleted");
                    assert_eq!(case["old_target"]["vtable"], 0x51f19c);
                    assert_eq!(
                        case["events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|event| event["kind"] == "free")
                            .count(),
                        1
                    );
                }
                1 => {
                    let result = arena.clone_native_target(
                        old_ref.unwrap(),
                        WeightedTargetConstructorNull::ActorRegistry,
                    );
                    if stop == Some("allocation") {
                        assert_eq!(
                            result,
                            Err(NativeTargetError::Heap(
                                ActorServiceHeapError::FixtureFailure(71)
                            )),
                            "{case}"
                        );
                    } else if stop == Some("process_null") {
                        assert_eq!(
                            result,
                            Err(NativeTargetError::Object(ObjectError::MissingActorRegistry)),
                            "{case}"
                        );
                    } else if case["returned"].is_null() {
                        assert_eq!(result, Ok(None), "{case}");
                    } else {
                        assert_eq!(case["returned"], "new");
                        compare(
                            arena.native_target(result.unwrap().unwrap()).unwrap(),
                            &case["new_target"],
                            first,
                            case,
                        );
                    }
                }
                2 => assert_eq!(
                    target
                        .velocity_words(Some(words(&case["process_velocity"])))
                        .unwrap(),
                    optional_words(&case["query"]["values"]),
                    "{case}"
                ),
                3 => {
                    let basis = NativeBasis {
                        words: std::array::from_fn(|i| {
                            case["process_basis_words"][i].as_u64().unwrap() as u32
                        }),
                    };
                    let actual = arena.native_target_basis(&target, Some(basis)).unwrap();
                    assert_eq!(
                        actual.words.to_vec(),
                        case["query"]["values"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|value| value.as_u64().unwrap() as u32)
                            .collect::<Vec<_>>(),
                        "{case}"
                    );
                }
                4 => assert_eq!(
                    arena.resolve_target_actor(&target).unwrap().is_some() as u64,
                    case["returned"].as_u64().unwrap()
                ),
                5 => assert_eq!(
                    target.object_referent().is_some() as u64,
                    case["returned"].as_u64().unwrap()
                ),
                6 => assert_eq!(
                    arena
                        .refresh_native_target(&mut target, 0xffff_ffff)
                        .unwrap() as u64,
                    case["returned"].as_u64().unwrap()
                ),
                7 => assert_eq!(
                    target.point_words(),
                    optional_words(&case["query"]["values"]),
                    "{case}"
                ),
                8 | 9 => {
                    let referent = if case["method_index"] == 8 {
                        WeightedTargetReferent::Actor(Some(first))
                    } else {
                        WeightedTargetReferent::Object(Some(first))
                    };
                    assert_eq!(
                        arena
                            .try_replace_weighted_target(&mut target, referent)
                            .unwrap() as u64,
                        case["returned"].as_u64().unwrap(),
                        "{case}"
                    );
                }
                _ => panic!("unexpected callback index"),
            }
            assert_eq!(
                target, original,
                "resource method unexpectedly mutated: {case}"
            );
        } else {
            let input = (case["input_present"] == true).then_some(first);
            let referent = match case["publisher"].as_str().unwrap() {
                "actor" => WeightedTargetReferent::Actor(input),
                "object" => WeightedTargetReferent::Object(input),
                "point" => WeightedTargetReferent::Point(
                    (case["input_present"] == true).then(|| words(&case["input_point"])),
                ),
                _ => panic!("unexpected publisher"),
            };
            let mut pair =
                NativeWeightedTarget::new(old_ref, case["old_score"].as_u64().unwrap() as u32);
            let result = arena.publish_weighted_target(
                &mut pair,
                WeightedTargetRequest {
                    constructor_null: match pass {
                        ConstructorPass::ActorRegistry => {
                            WeightedTargetConstructorNull::ActorRegistry
                        }
                        ConstructorPass::Process => WeightedTargetConstructorNull::Supplied(
                            (stop != Some("process_null")).then_some(process_null),
                        ),
                    },
                    referent,
                    score: case["score"].as_u64().unwrap() as u32,
                },
            );
            if stop == Some("allocation") {
                assert_eq!(
                    result,
                    Err(NativeTargetError::Heap(
                        ActorServiceHeapError::FixtureFailure(71)
                    )),
                    "{case}"
                );
            } else if stop == Some("process_null") {
                assert_eq!(
                    result,
                    Err(match pass {
                        ConstructorPass::ActorRegistry => {
                            NativeTargetError::Object(ObjectError::MissingActorRegistry)
                        }
                        ConstructorPass::Process => NativeTargetError::MissingProcessNull,
                    }),
                    "{case}"
                );
            } else if stop == Some("point") {
                let error = if case["publisher"] == "actor" {
                    ObjectError::ActorPositionUnknown(first)
                } else {
                    ObjectError::TargetObjectPositionUnknown(first)
                };
                assert_eq!(result, Err(NativeTargetError::Object(error)), "{case}");
            } else {
                assert_eq!(
                    result.unwrap() as u64,
                    case["returned"].as_u64().unwrap(),
                    "{case}"
                );
            }
            assert_eq!(
                pair.score as u64,
                case["final_score"].as_u64().unwrap(),
                "{case}"
            );
            if case["custody"] == "released" {
                assert_eq!(pair.target, old_ref, "{case}");
                assert_eq!(
                    arena.actor_service_heap().state(old_ref.unwrap()),
                    Ok(ActorServiceResourceState::Released),
                    "{case}"
                );
                assert_eq!(case["final_pointer"], "old");
                assert!(pair.native_pointer_present());
            } else {
                match case["final_pointer"].as_str() {
                    Some("old") => {
                        assert_eq!(pair.target, old_ref);
                        compare(
                            arena.native_target(pair.target.unwrap()).unwrap(),
                            &case["old_target"],
                            first,
                            case,
                        );
                    }
                    Some("new") => {
                        assert_ne!(pair.target, old_ref);
                        compare(
                            arena.native_target(pair.target.unwrap()).unwrap(),
                            &case["new_target"],
                            first,
                            case,
                        );
                    }
                    None => assert_eq!(pair.target, None, "{case}"),
                    _ => panic!("unexpected owner pointer"),
                }
            }
        }
        let native_allocations: Vec<u32> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "allocate")
            .map(|event| event["bytes"].as_u64().unwrap() as u32)
            .collect();
        let actual_allocations: Vec<u32> = arena.actor_service_heap().attempts()
            [allocation_start..]
            .iter()
            .map(|kind| match kind {
                ActorServiceResourceKind::WeaponFireNode => {
                    panic!("unexpected firing allocation in target fixture")
                }
                ActorServiceResourceKind::Target(class) => class.allocation_bytes(),
                ActorServiceResourceKind::FamilyRadiusScoreInput => {
                    panic!("unexpected radius input allocation")
                }
            })
            .collect();
        assert_eq!(actual_allocations, native_allocations, "{case}");
        assert_eq!(arena.object(first).unwrap(), &before_first);
        assert_eq!(arena.object(other).unwrap(), &before_other);
        assert_eq!(case["referents_unchanged"], true);
        if matches!(pass, ConstructorPass::Process) {
            assert_eq!(arena.actor_handles().unwrap().null_handle(), registry_null);
        }
    }
    compared
}

#[test]
fn fixed_resource_and_three_publishers_match_every_283_portable_native_receipt() {
    assert_eq!(compare_receipts(ConstructorPass::ActorRegistry), 283);
}

#[test]
fn explicit_process_null_matches_266_native_publishers_with_different_registry_backing() {
    assert_eq!(compare_receipts(ConstructorPass::Process), 266);
}
