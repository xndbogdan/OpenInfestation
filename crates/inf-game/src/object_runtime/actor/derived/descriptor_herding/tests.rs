//! Native Herding receipts. Supplied actors are allocated in list order, self
//! first; allocator outcomes are scripted and native pointers project to
//! handle/focus/kind.
use serde_json::{json, Value};

use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyHerdingError, FixedPositionTarget,
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

fn allocate(arena: &mut ObjectArena) -> ObjectId {
    arena
        .allocate_selector35_actor(
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
        .unwrap()
}

fn configure(arena: &mut ObjectArena, actor: ObjectId, spec: &Value) {
    arena.actor_mut(actor).unwrap().word_b4 = u(&spec["classification"]);
    arena.actor_mut(actor).unwrap().words_8e_92 = words(&spec["velocity"]);
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.resource_word = u(&spec["resource"]) as u16;
    base.flags = u(&spec["flags"]);
    base.position_words = Some(words(&spec["position"]));
}

fn target(arena: &mut ObjectArena, spec: &Value, object: ObjectId) -> NativeHeapRef {
    let handle = ActorHandle::from_raw(u(&spec["handle"]));
    let focus = words(&spec["focus"]);
    arena.import_fixture_target(match u(&spec["kind"]) {
        8 => NativeTarget::FixedPosition(FixedPositionTarget::from_point(handle, Some(focus))),
        16 => {
            NativeTarget::ObjectPosition(ObjectPositionTarget::from_position(handle, focus, object))
        }
        kind => panic!("unexpected target kind {kind}"),
    })
}

fn projected(arena: &ObjectArena, reference: Option<NativeHeapRef>) -> Value {
    reference.map_or(Value::Null, |reference| {
        let target = arena.native_target(reference).unwrap();
        json!({"handle":target.handle_04().raw(),"focus":target.raw_focus_08_words().unwrap(),"kind":target.kind_word()})
    })
}

#[test]
fn herding_matches_26_original_change_update_and_scan_cases() {
    let receipt: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_herding_native.json"
    )))
    .unwrap();
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 26);
    for case in cases {
        let i = &case["input"];
        let name = format!("{}/{}", i["mode"], i["name"]);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(u(&i["null"])),
        })
        .unwrap();
        let actor = allocate(&mut arena);
        configure(&mut arena, actor, &i["actor"]);
        for member in i["members"].as_array().unwrap() {
            let other = allocate(&mut arena);
            configure(&mut arena, other, member);
        }
        let object = arena.allocate(ObjectClassKind::RouteNode);
        arena.owned_mut(object).unwrap().state.base.position_words = Some([5, 6, 7]);
        arena.derived_actor_mut(actor).unwrap().word_164 = i["radius"].as_i64().unwrap() as i32;
        arena.derived_actor_mut(actor).unwrap().word_150 = Some(u(&i["status"]));
        let herd = (!i["herd"].is_null()).then(|| target(&mut arena, &i["herd"], object));
        let current = (!i["current"].is_null()).then(|| target(&mut arena, &i["current"], object));
        arena.derived_actor_mut(actor).unwrap().target_148 = current;
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.words_188_19c[1] = Some(u(&i["timer"]));
        family.word_228 = u(&i["word228"]);
        family.word_22e = u(&i["flags22e"]) as u16;
        family.targets_1d4_1fc[2] = herd;
        family.words_1d8_200[2] = u(&i["herd_score"]);
        arena
            .actor_service_heap_mut()
            .script(i["allocations"].as_array().unwrap().iter().map(|outcome| {
                match outcome.as_str().unwrap() {
                    "allocate" => FixtureActorServiceAllocation::Allocate,
                    "null" => FixtureActorServiceAllocation::Null,
                    other => panic!("unexpected allocation {other}"),
                }
            }));
        let initial_attempts = arena.actor_service_heap().attempts().len();
        let null =
            WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(u(&i["null"]))));
        let mut rng = u(&i["rng"]);
        let result = match i["mode"].as_str().unwrap() {
            "change" => arena.change_family_herding(actor, i["entering"] == true, null),
            "update" => arena.update_family_herding(actor, u(&i["delta"]), &mut rng, null),
            mode => panic!("unexpected mode {mode}"),
        };
        let events = case["events"].as_array().unwrap();
        if events.iter().any(|event| event["kind"] == "fault") {
            assert_eq!(
                result,
                Err(FamilyHerdingError::MissingCurrentTarget(actor)),
                "{name}"
            );
        } else {
            assert_eq!(result, Ok(()), "{name}");
        }
        let family = arena.family428650_actor(actor).unwrap();
        let actual = json!({
            "timer": family.words_188_19c[1].unwrap(),
            "word228": family.word_228,
            "flags22e": family.word_22e,
            "rng": rng,
            "current": projected(&arena, arena.derived_actor(actor).unwrap().target_148),
            "herd": projected(&arena, family.targets_1d4_1fc[2]),
            "herd_score": family.words_1d8_200[2],
        });
        assert_eq!(actual, case["output"], "{name}");
        let allocations = arena.actor_service_heap().attempts().len() - initial_attempts;
        let expected = events
            .iter()
            .filter(|event| event["kind"] == "allocate")
            .count();
        assert_eq!(allocations, expected, "{name} allocations");
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

#[test]
fn herding_update_requires_its_timer_and_reached_status() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(1),
    })
    .unwrap();
    let actor = allocate(&mut arena);
    let null = WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(1)));
    let mut rng = 5;
    assert_eq!(
        arena.update_family_herding(actor, 100, &mut rng, null),
        Err(FamilyHerdingError::Uninitialized {
            actor,
            offset: 0x18c,
            address: 0x0042_7aa8,
        })
    );
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(1000);
    assert_eq!(
        arena.update_family_herding(actor, 100, &mut rng, null),
        Err(FamilyHerdingError::Uninitialized {
            actor,
            offset: 0x150,
            address: 0x0042_7ab2,
        })
    );
    assert_eq!(rng, 5, "no draw precedes the reached status read");
}
