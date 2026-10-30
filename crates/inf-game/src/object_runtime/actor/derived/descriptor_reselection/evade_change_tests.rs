use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorServiceHeapError, ActorServiceResourceState, ActorTerrainInputs,
    DerivedActorConstructionRequest, FixtureActorServiceAllocation,
};
use serde_json::{json, Value};

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}
fn receipt() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_evade_change_native.json"
    ))
    .unwrap()
}

#[test]
fn family_evade_entry_matches_56_original_thunk_and_final_virtual_receipts() {
    let fixture = receipt();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 56);
    for case in cases {
        let input = &case["input"];
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let request = DerivedActorConstructionRequest {
            actor: ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some(std::array::from_fn(|n| {
                    number(&input["position"][n]) as u16
                })),
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &[0; 4096],
                },
            },
            world_mask_limits: [3; 2],
        };
        let actor = match input["profile"].as_str().unwrap() {
            "parent" => arena.allocate_family4236d0_actor(request, &mut 0),
            "selector10" => arena.allocate_selector10_actor(request, &mut 0),
            "selector36" => arena.allocate_selector36_actor(request, &mut 0),
            "selector35" => arena.allocate_selector35_actor(request, &mut 0),
            _ => unreachable!(),
        }
        .unwrap();
        let old = input["old"].as_bool().unwrap().then(|| {
            arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
                handle_04: ActorHandle::from_raw(9),
                focus_08: Some([11, 12, 13]),
                word_10: 8,
            }))
        });
        arena.derived_actor_mut(actor).unwrap().target_148 = old;
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.word_228 = number(&input["status"]);
        state.words_188_19c[1] = Some(number(&input["timer"]));
        state.words_188_19c[2] = Some(number(&input["distance"]));
        state.words_188_19c[3] = Some(number(&input["duration"]));
        state.words_204_208[1] = Some(number(&input["speed"]));
        state.byte_22c = number(&input["fire"]) as u8;
        let selector35 = input["profile"] == "selector35";
        if selector35 {
            let state = arena.selector35_actor_mut(actor).unwrap();
            state.word_230 = number(&input["word230"]);
            state.word_234 = number(&input["word234"]);
        }
        let stop = input["stop"].as_str();
        if stop == Some("position") {
            arena.owned_mut(actor).unwrap().state.base.position_words = None;
        }
        let source = WeightedTargetConstructorNull::Supplied(
            (stop != Some("process_null"))
                .then(|| ActorHandle::from_raw(number(&input["process_null"]))),
        );
        arena
            .actor_service_heap_mut()
            .script([if stop == Some("allocation") {
                FixtureActorServiceAllocation::Error(7)
            } else if input["allocation_null"] == true {
                FixtureActorServiceAllocation::Null
            } else {
                FixtureActorServiceAllocation::Allocate
            }]);
        let before_attempts = arena.actor_service_heap().attempts().len();
        let base_before = arena.object(actor).unwrap().base.clone();
        let mut rng = number(&input["rng"]);
        let result = arena.change_family_evade(
            actor,
            FamilyEvadeEntryRequest {
                kind: if selector35 {
                    FamilyEvadeEntryKind::Selector35128
                } else {
                    FamilyEvadeEntryKind::Common128
                },
                entering: number(&input["entering"]),
                constructor_null: source,
            },
            &mut rng,
        );
        match case["stopped"].as_str() {
            None => assert!(result.is_ok(), "{case}: {result:?}"),
            Some("allocation") => assert!(
                matches!(
                    result,
                    Err(FamilyTimedDescriptorError::Target(NativeTargetError::Heap(
                        ActorServiceHeapError::FixtureFailure(7)
                    )))
                ),
                "{case}: {result:?}"
            ),
            Some("process_null") => assert!(
                matches!(
                    result,
                    Err(FamilyTimedDescriptorError::Target(
                        NativeTargetError::MissingProcessNull
                    ))
                ),
                "{case}: {result:?}"
            ),
            Some("position") => assert!(
                matches!(result, Err(FamilyTimedDescriptorError::Object(
                ObjectError::ActorPositionUnknown(id))) if id == actor),
                "{case}: {result:?}"
            ),
            other => panic!("unexpected stop {other:?}"),
        }
        assert_eq!(
            arena.actor_service_heap().attempts().len() - before_attempts,
            number(&case["allocation_attempts"]) as usize,
            "{case}"
        );
        let current = arena.derived_actor(actor).unwrap().target_148;
        let name = if current.is_none() {
            None
        } else if current == old {
            Some("old")
        } else {
            Some("new")
        };
        let state = arena.family428650_actor(actor).unwrap();
        let mut actual = json!({"status":state.word_228,"timer":state.words_188_19c[1],
            "distance":state.words_188_19c[2],"duration":state.words_188_19c[3],
            "speed":state.words_204_208[1],"fire":state.byte_22c,"target":name,"rng":rng});
        if selector35 {
            let state = arena.selector35_actor(actor).unwrap();
            actual["word230"] = json!(state.word_230);
            actual["word234"] = json!(state.word_234);
        }
        assert_eq!(actual, case["output"], "{case}");
        if name == Some("new") {
            let target = arena.native_target(current.unwrap()).unwrap();
            assert_eq!(
                json!({"vtable":0x0051_f154,"handle":target.handle_04().raw(),
                "focus":target.raw_focus_08_words(),"kind":target.kind_word()}),
                case["new_target"],
                "{case}"
            );
        }
        if let Some(old) = old {
            assert!(matches!(
                arena.actor_service_heap().state(old).unwrap(),
                ActorServiceResourceState::Target(_)
            ));
        }
        assert!(arena
            .actor_service_heap()
            .fixture_release_attempts()
            .is_empty());
        assert_eq!(arena.object(actor).unwrap().base, base_before);
    }
}

#[test]
fn family_evade_false_low_byte_and_null_allocation_do_not_read_unavailable_sources() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector10_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: None,
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &[0; 4096],
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    let before = arena.object(actor).unwrap().clone();
    let mut rng = 123;
    arena
        .change_family_evade(
            actor,
            FamilyEvadeEntryRequest {
                kind: FamilyEvadeEntryKind::Common128,
                entering: 0x100,
                constructor_null: WeightedTargetConstructorNull::Supplied(None),
            },
            &mut rng,
        )
        .unwrap();
    assert_eq!(arena.object(actor).unwrap(), &before);
    assert_eq!(rng, 123);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    let mut expected_rng = rng;
    name_entry_rng_next(&mut expected_rng);
    arena
        .change_family_evade(
            actor,
            FamilyEvadeEntryRequest {
                kind: FamilyEvadeEntryKind::Common128,
                entering: 0x101,
                constructor_null: WeightedTargetConstructorNull::Supplied(None),
            },
            &mut rng,
        )
        .unwrap();
    assert_eq!(rng, expected_rng);
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1],
        Some(0)
    );
    assert_eq!(arena.object(actor).unwrap().base.position_words, None);
}

#[test]
fn family_evade_entry_full_native_bodies_and_actual_class_virtuals_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    let fixture = receipt();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixture["retail_sha256"].as_str().unwrap()
    );
    for guard in fixture["source_guards"].as_array().unwrap() {
        let start = number(&guard["address"]) as usize - 0x0040_0000;
        let end = start + number(&guard["length"]) as usize;
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes[start..end])),
            guard["sha256"].as_str().unwrap()
        );
    }
    let exe = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    for value in fixture["virtual_128"].as_object().unwrap().values() {
        assert_eq!(
            exe.word(number(&value["table"]) + 0x128).unwrap(),
            number(&value["target"])
        );
    }
}
