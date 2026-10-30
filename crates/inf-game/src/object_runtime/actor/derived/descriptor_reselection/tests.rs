//! Native caller receipts. Selected virtual+18 is explicitly supplied in the
//! original run; Rust uses actual fixed or stale/live follow-target services.
//! A missing referent position compares only the stopped caller prefix.
use super::*;
use crate::object_runtime::{
    targets::{ActorFollowTarget, FixedPositionTarget, NativeTarget},
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorServiceAllocationRequest, ActorServiceResourceKind, ActorTerrainInputs,
    DerivedActorConstructionRequest, NativeHeapRef,
};
use serde_json::{json, Value};

fn n(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}
fn xyz(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

fn fixture(input: &Value, stop_refresh: bool) -> (ObjectArena, ObjectId, [NativeHeapRef; 3]) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let request = || DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        },
        world_mask_limits: [3, 3],
    };
    let actor = arena.allocate_selector10_actor(request(), &mut 0).unwrap();
    let referent = arena.allocate_selector10_actor(request(), &mut 0).unwrap();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some(xyz(&input["position"]));
    let refresh = if stop_refresh {
        "error"
    } else {
        input["refresh"].as_str().unwrap()
    };
    arena.owned_mut(referent).unwrap().state.base.position_words =
        (refresh != "error").then(|| xyz(&input["refresh_point"]));
    arena.actor_mut(referent).unwrap().word_74 = 0;
    arena.actor_mut(referent).unwrap().word_c4 = 4;
    let handle = if refresh == "stale" {
        ActorHandle::from_raw(0)
    } else {
        arena.actor(referent).unwrap().handle
    };
    let current_index = input["current"].as_u64().map(|v| v as usize);
    let targets = std::array::from_fn(|index| {
        let kind = if current_index == Some(index) {
            n(&input["current_kind"])
        } else {
            8
        };
        let target = if index < 2 && refresh != "fixed" {
            NativeTarget::ActorFollow(ActorFollowTarget {
                handle_04: handle,
                focus_08: Some(xyz(&input["points"][index])),
                word_10: kind,
                focus_14: Some([1, 2, 3]),
                velocity_1a: Some([4, 5, 6]),
            })
        } else {
            NativeTarget::FixedPosition(FixedPositionTarget {
                handle_04: ActorHandle::from_raw(0),
                focus_08: Some(xyz(&input["points"][index])),
                word_10: kind,
            })
        };
        let reference = arena
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(target.class()),
            })
            .unwrap()
            .unwrap();
        arena.construct_native_target(reference, target).unwrap();
        reference
    });
    let derived = arena.derived_actor_mut(actor).unwrap();
    derived.word_150 = Some(n(&input["state"]));
    derived.target_148 = current_index.map(|index| targets[index]);
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.words_188_19c[1] = Some(n(&input["timer"]));
    family.words_188_19c[2] = Some(n(&input["distance"]));
    family.word_22e = n(&input["flags"]) as u16;
    for (index, slot) in [(0, 3), (1, 1)] {
        family.targets_1d4_1fc[slot] = input["present"][index]
            .as_bool()
            .unwrap()
            .then_some(targets[index]);
        family.words_1d8_200[slot] = n(&input["scores"][index]);
    }
    (arena, actor, targets)
}

fn snapshot(arena: &ObjectArena, actor: ObjectId, targets: [NativeHeapRef; 3], rng: u32) -> Value {
    let family = arena.family428650_actor(actor).unwrap();
    json!({
        "timer":family.words_188_19c[1].unwrap(),
        "state":arena.derived_actor(actor).unwrap().word_150.unwrap(),
        "distance":family.words_188_19c[2].unwrap(),
        "flags":family.word_22e,
        "position":arena.object(actor).unwrap().base.position_words.unwrap(),
        "points":targets.map(|reference| arena.native_target(reference).unwrap().raw_focus_08_words().unwrap()),
        "rng":rng
    })
}

fn request(input: &Value) -> FamilyTimedDescriptorRequest {
    FamilyTimedDescriptorRequest {
        kind: match input["kind"].as_str().unwrap() {
            "point124" => FamilyTimedDescriptorKind::Point124,
            "retry130" => FamilyTimedDescriptorKind::Retry130,
            value => panic!("unexpected native operation {value}"),
        },
        microseconds: n(&input["microseconds"]),
        settings_7c: n(&input["settings"]),
    }
}

fn receipt() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/descriptor_reselection_native.json"
    )))
    .unwrap()
}

#[test]
fn descriptor_reselection_matches_61_native_rng_target_and_distance_receipts() {
    let fixture = receipt();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 61);
    assert_eq!(fixture["fpcw"], "0x027F");
    for case in cases {
        let input = &case["input"];
        let name = case["name"].as_str().unwrap();
        let (mut arena, actor, targets) = self::fixture(input, false);
        let mut rng = n(&input["rng"]);
        let result = arena.update_family_timed_descriptor(actor, request(input), &mut rng);
        match case["stop"].as_str() {
            None => assert!(result.is_ok(), "{name}: {result:?}"),
            Some("current_null_native_fault") => assert!(
                matches!(
                    result,
                    Err(FamilyTimedDescriptorError::MissingField { offset: 0x148, .. })
                ),
                "{name}: {result:?}"
            ),
            Some("supplied_refresh_failure") => assert!(
                matches!(
                    result,
                    Err(FamilyTimedDescriptorError::Object(
                        ObjectError::ActorPositionUnknown(_)
                    ))
                ),
                "{name}: {result:?}"
            ),
            value => panic!("unexpected stop {value:?}"),
        }
        assert_eq!(
            snapshot(&arena, actor, targets, rng),
            case["output"],
            "{name}"
        );

        // A real unavailable follow-target referent stops at the same caller
        // boundary as the oracle's explicitly supplied virtual+18 failure.
        // It authenticates the caller's reached prefix, not that callee body.
        for event in case["events"].as_array().unwrap() {
            let (mut arena, actor, targets) = self::fixture(input, true);
            let mut rng = n(&input["rng"]);
            let result = arena.update_family_timed_descriptor(actor, request(input), &mut rng);
            assert!(
                matches!(
                    result,
                    Err(FamilyTimedDescriptorError::Object(
                        ObjectError::ActorPositionUnknown(_)
                    ))
                ),
                "{name} boundary: {result:?}"
            );
            assert_eq!(
                snapshot(&arena, actor, targets, rng),
                event["before"],
                "{name} boundary"
            );
        }
    }
}

#[test]
fn descriptor_reselection_complete_native_bodies_and_final_vtables_are_guarded() {
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
        let start = n(&guard["address"]) as usize - 0x0040_0000;
        let length = n(&guard["length"]) as usize;
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes[start..start + length])),
            guard["sha256"].as_str().unwrap()
        );
    }
    let executable = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    for vtable in [0x0052_0cc8, 0x0052_7ee0] {
        assert_eq!(executable.word(vtable + 0x124).unwrap(), 0x0042_3bd0);
        assert_eq!(executable.word(vtable + 0x130).unwrap(), 0x0042_3a20);
    }
}
