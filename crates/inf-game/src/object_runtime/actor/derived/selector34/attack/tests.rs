//! Native 00433FB0/00433CC0 receipts over supplied actor fields and targets.
//! Allocator outcomes are scripted; native pointers project to handle/focus/kind.
use serde_json::{json, Value};

use super::{Selector34AttackContext, Selector34AttackError};
use crate::object_runtime::{
    ActorAllocationPolicy, ActorConstructionRequest, ActorFollowTarget, ActorHandle,
    ActorRegistryConfig, ActorTerrainInputs, DerivedActorConstructionRequest, FixedPositionTarget,
    FixtureActorServiceAllocation, NativeHeapRef, NativeTarget, ObjectArena, ObjectClassKind,
    ObjectId, ObjectPositionTarget, WeightedTargetConstructorNull,
};

const SINE: [u16; 4096] = [0; 4096];

fn u(value: &Value) -> u32 {
    value.as_i64().unwrap() as u32
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| u(&value[axis]) as u16)
}

fn ship(null: u32) -> (ObjectArena, ObjectId) {
    ship_with(null, None)
}

fn ship_with(null: u32, policy: Option<ActorAllocationPolicy>) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(null),
    })
    .unwrap();
    if let Some(policy) = policy {
        arena.select_actor_allocation_policy(policy).unwrap();
    }
    let actor = arena
        .allocate_selector34_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0; 3]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    (arena, actor)
}

fn target(arena: &mut ObjectArena, spec: &Value, object: ObjectId) -> Option<NativeHeapRef> {
    if spec.is_null() {
        return None;
    }
    let handle = ActorHandle::from_raw(u(&spec["handle"]));
    let focus = words(&spec["focus"]);
    Some(arena.import_fixture_target(match u(&spec["kind"]) {
        8 => NativeTarget::FixedPosition(FixedPositionTarget::from_point(handle, Some(focus))),
        16 => {
            NativeTarget::ObjectPosition(ObjectPositionTarget::from_position(handle, focus, object))
        }
        kind => panic!("unexpected target kind {kind}"),
    }))
}

fn projected(arena: &ObjectArena, reference: Option<NativeHeapRef>) -> Value {
    reference.map_or(Value::Null, |reference| {
        let target = arena.native_target(reference).unwrap();
        json!({"handle":target.handle_04().raw(),"focus":target.raw_focus_08_words().unwrap(),"kind":target.kind_word()})
    })
}

#[test]
fn selector34_attack_matches_38_original_cases() {
    let receipt: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector34_attack_native.json"
    )))
    .unwrap();
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 38);
    for case in cases {
        let i = &case["input"];
        let name = format!("{}/{}", i["mode"], i["name"]);
        let (mut arena, actor) = ship(u(&i["null"]));
        let object = arena.allocate(ObjectClassKind::RouteNode);
        arena.owned_mut(object).unwrap().state.base.position_words = Some([5, 6, 7]);
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&i["position"]));
        arena.actor_mut(actor).unwrap().words_8e_92 = words(&i["velocity"]);
        let state = arena.selector34_actor_mut(actor).unwrap();
        state.byte_239 = u(&i["byte239"]) as u8;
        state.word_240 = u(&i["word240"]);
        state.local_direction_244 = Some([0xa5a5, 0xa5a5, u(&i["word248"]) as u16]);
        let derived = arena.derived_actor_mut(actor).unwrap();
        derived.word_164 = i["word164"].as_i64().unwrap() as i32;
        derived.word_14c = Some(u(&i["words"]["0x14c"]));
        derived.word_150 = Some(u(&i["words"]["0x150"]));
        derived.word_15c = Some(u(&i["words"]["0x15c"]));
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.words_188_19c[1] = Some(u(&i["words"]["0x18c"]));
        family.word_228 = u(&i["words"]["0x228"]);
        family.word_22e = u(&i["flags22e"]) as u16;
        family.byte_22c = u(&i["byte22c"]) as u8;
        let state = arena.selector34_actor_mut(actor).unwrap();
        state.word_230 = u(&i["words"]["0x230"]);
        state.word_234 = u(&i["words"]["0x234"]);
        let source = target(&mut arena, &i["source"], object);
        let preferred = target(&mut arena, &i["preferred"], object);
        let current = target(&mut arena, &i["current"], object);
        let secondary = target(&mut arena, &i["secondary"], object);
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.targets_1d4_1fc[0] = source;
        family.words_1d8_200[0] = u(&i["source_score"]);
        family.targets_1d4_1fc[4] = preferred;
        family.words_1d8_200[4] = u(&i["preferred_score"]);
        arena.derived_actor_mut(actor).unwrap().target_148 = current;
        arena.derived_actor_mut(actor).unwrap().target_154 = secondary;
        arena
            .actor_service_heap_mut()
            .script(i["allocations"].as_array().unwrap().iter().map(|outcome| {
                match outcome.as_str().unwrap() {
                    "allocate" => FixtureActorServiceAllocation::Allocate,
                    "null" => FixtureActorServiceAllocation::Null,
                    other => panic!("unexpected allocation {other}"),
                }
            }));
        let initial = arena.actor_service_heap().attempts().len();
        let mut rng = u(&i["rng"]);
        let mut context = Selector34AttackContext {
            rng: &mut rng,
            constructor_null: WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(
                u(&i["null"]),
            ))),
            settings_7c: 0,
        };
        let result = match i["mode"].as_str().unwrap() {
            "change" => arena.change_selector34_attack(actor, i["entering"] == true, &mut context),
            "update" => arena.update_selector34_attack(actor, u(&i["delta"]), &mut context),
            mode => panic!("unexpected mode {mode}"),
        };
        let events = case["events"].as_array().unwrap();
        match events.iter().find(|event| event["kind"] == "fault") {
            Some(fault) => assert!(
                matches!(result, Err(Selector34AttackError::RequiredField { offset: 0x148, address, .. })
                    if address == u(&fault["eip"])),
                "{name}: {result:?}"
            ),
            None => assert_eq!(result, Ok(()), "{name}"),
        }
        let derived = arena.derived_actor(actor).unwrap();
        let family = arena.family428650_actor(actor).unwrap();
        let state = arena.selector34_actor(actor).unwrap();
        let actual = json!({
            "words": {
                "0x14c": derived.word_14c.unwrap(),
                "0x150": derived.word_150.unwrap(),
                "0x15c": derived.word_15c.unwrap(),
                "0x18c": family.words_188_19c[1].unwrap(),
                "0x228": family.word_228,
                "0x230": state.word_230,
                "0x234": state.word_234,
            },
            "flags22e": family.word_22e,
            "byte22c": family.byte_22c,
            "rng": rng,
            "current": projected(&arena, derived.target_148),
            "secondary": projected(&arena, derived.target_154),
        });
        assert_eq!(actual, case["output"], "{name}");
        assert_eq!(
            arena.actor_service_heap().attempts().len() - initial,
            events
                .iter()
                .filter(|event| event["kind"] == "allocate")
                .count(),
            "{name} allocations"
        );
        assert_eq!(
            arena.actor_service_heap().fixture_release_attempts().len(),
            events
                .iter()
                .filter(|event| event["kind"] == "free")
                .count(),
            "{name} releases"
        );
    }
}

/// Entry clones a type-2 source whose 004828D0 copy leaves +08 unwritten; the
/// same tick's update reaches 00433F42 before any refresh, as Infest01's entry
/// ticks order them. Strict arenas keep failing there; the extended carry
/// supplies zero, so the velocity-led secondary point takes Y = 0.
#[test]
fn entry_clone_focus_is_unknown_unless_the_extended_carry_supplies_zero() {
    let extended = ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation;
    for policy in [None, Some(extended)] {
        let (mut arena, actor) = ship_with(7, policy);
        arena.owned_mut(actor).unwrap().state.base.position_words = Some([1000, 1000, 1000]);
        arena.actor_mut(actor).unwrap().words_8e_92 = [30, 40, 50];
        let state = arena.selector34_actor_mut(actor).unwrap();
        state.word_240 = 750;
        state.local_direction_244 = Some([0, 0, 100]);
        let derived = arena.derived_actor_mut(actor).unwrap();
        derived.word_14c = Some(800);
        derived.word_150 = Some(3);
        let point = [0x3c28, 0x01b2, 0xb55c];
        let source = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: ActorHandle::from_raw(7),
            focus_08: Some(point),
            word_10: 2,
            focus_14: Some(point),
            velocity_1a: Some([0; 3]),
        }));
        arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(source);
        let mut rng = 0x1234_5678;
        let mut context = Selector34AttackContext {
            rng: &mut rng,
            constructor_null: WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(
                7,
            ))),
            settings_7c: 0,
        };
        arena
            .change_selector34_attack(actor, true, &mut context)
            .unwrap();
        let clone = arena.derived_actor(actor).unwrap().target_148.unwrap();
        assert_ne!(clone, source);
        let result = arena.update_selector34_attack(actor, 100, &mut context);
        if policy.is_none() {
            assert_eq!(
                result,
                Err(Selector34AttackError::RequiredFocus {
                    reference: clone,
                    address: 0x0043_3f42,
                })
            );
            assert_eq!(arena.derived_actor(actor).unwrap().target_154, None);
            continue;
        }
        result.unwrap();
        let clone = arena.native_target(clone).unwrap();
        assert_eq!(clone.raw_focus_08_words(), Some([0; 3]));
        assert_eq!(clone.point_words(), Some(point));
        let secondary = arena.derived_actor(actor).unwrap().target_154.unwrap();
        let secondary = arena.native_target(secondary).unwrap();
        assert_eq!(secondary.kind_word(), 8);
        assert_eq!(secondary.raw_focus_08_words().unwrap()[1], 0);
    }
}
