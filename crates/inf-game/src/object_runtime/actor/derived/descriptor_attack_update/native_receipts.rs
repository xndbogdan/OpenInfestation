//! Compare all admitted supplied-resource receipts. Raw instruction read IPs,
//! physical freed-pointer values and unknown heap padding remain diagnostics;
//! they are not invented Rust allocation identities or live resource words.

use super::tests::construction;
use super::*;
use crate::object_runtime::{
    ActorFollowTarget, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
    ActorServiceResourceState, FamilyHelper482ff0, FixedPositionTarget,
    FixtureActorServiceAllocation, NativeHeapRef, NativeTargetClass, ObjectPositionTarget,
    TargetNullHandleSource,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn word(value: &Value) -> Option<u32> {
    value.as_u64().map(|v| v as u32)
}
fn point(value: &Value) -> Option<[u16; 3]> {
    value
        .as_array()
        .map(|v| std::array::from_fn(|i| v[i].as_u64().unwrap() as u16))
}
fn target(value: &Value, referent: ObjectId) -> NativeTarget {
    let handle_04 = ActorHandle::from_raw(word(&value["handle_04"]).unwrap());
    let word_10 = word(&value["kind_10"]).unwrap();
    match value["class_name"].as_str().unwrap() {
        "fixed" => NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04,
            word_10,
            focus_08: point(&value["raw_focus_08_words"]),
        }),
        "follow" => NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04,
            word_10,
            focus_08: point(&value["raw_focus_08_words"]),
            focus_14: point(&value["virtual_point_words"]),
            velocity_1a: point(&value["velocity_1a_words"]),
        }),
        "object" => NativeTarget::ObjectPosition(ObjectPositionTarget {
            handle_04,
            word_10,
            focus_08: point(&value["raw_focus_08_words"])
                .expect("owned type16 constructor supplies complete XYZ"),
            object_14: referent,
        }),
        _ => panic!("unsupported admitted target class"),
    }
}
fn helper(value: &Value) -> Option<Box<FamilyHelper482ff0>> {
    (!value.is_null()).then(|| {
        Box::new(FamilyHelper482ff0 {
            words_00_04: std::array::from_fn(|i| word(&value["words00_04"][i])),
            word_08: word(&value["count08"]).unwrap(),
            records: [crate::object_runtime::FamilyHelperHistoryRecord::UNKNOWN; 4],
        })
    })
}
fn snapshot(target: NativeTarget, referent: ObjectId) -> Value {
    let mut value = json!({
        "class_name":match target.class() { NativeTargetClass::ActorFollow=>"follow", NativeTargetClass::FixedPosition=>"fixed",NativeTargetClass::ObjectPosition=>"object", NativeTargetClass::FurniturePosition=>"furniture" },
        "handle_04":target.handle_04().raw(), "kind_10":target.kind_word(),
        "raw_focus_08_words":target.raw_focus_08_words(), "virtual_point_words":target.point_words(),
    });
    if let NativeTarget::ActorFollow(follow) = target {
        value["velocity_1a_words"] = json!(follow.velocity_1a);
    }
    if let NativeTarget::ObjectPosition(object) = target {
        assert_eq!(object.object_14, referent);
        value["object_14"] = json!("referent");
    }
    value
}
fn compare_target(target: NativeTarget, expected: &Value, referent: ObjectId, name: &str) {
    assert_eq!(expected["custody"], "live", "{name}");
    let observed = snapshot(target, referent);
    for (key, value) in observed.as_object().unwrap() {
        assert_eq!(value, &expected[key], "{name}: {key}");
    }
}

struct ReceiptHost<'a> {
    spec: &'a Value,
    targets: BTreeMap<String, NativeHeapRef>,
    detached_helpers: BTreeMap<String, Box<FamilyHelper482ff0>>,
    helper_id: Option<String>,
    records: Vec<Value>,
}
impl TargetNullHandleSource for ReceiptHost<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        word(&self.spec["process_null_handle"]).map(ActorHandle::from_raw)
    }
}
impl FamilyAttackUpdateHost for ReceiptHost<'_> {
    type Error = &'static str;
    fn record_attack_helper(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyHelperRecordRequest,
    ) -> Result<(), Self::Error> {
        self.records.push(
            json!({"helper_id":self.helper_id,"handle_04":request.handle.raw(),
            "vector_words":request.displacement_words,"strength":request.native_weight}),
        );
        let mutation = self.spec["notify"]["mutation"].as_object().unwrap();
        for (key, value) in mutation {
            match key.as_str() {
                "flags22E" => {
                    arena.family428650_actor_mut(actor).unwrap().word_22e =
                        word(value).unwrap() as u16
                }
                "distance14C" => arena.derived_actor_mut(actor).unwrap().word_14c = word(value),
                "state150" => arena.derived_actor_mut(actor).unwrap().word_150 = word(value),
                "position_words" => {
                    arena.owned_mut(actor).unwrap().state.base.position_words = point(value)
                }
                "helper_words00_04" => {
                    let helper = arena
                        .family428650_actor_mut(actor)
                        .unwrap()
                        .helper_218
                        .as_mut()
                        .unwrap();
                    helper.words_00_04 = std::array::from_fn(|i| word(&value[i]));
                }
                "helper_id" => {
                    if let Some(old) = arena
                        .family428650_actor_mut(actor)
                        .unwrap()
                        .helper_218
                        .take()
                    {
                        self.detached_helpers
                            .insert(self.helper_id.take().unwrap(), old);
                    }
                    self.helper_id = value.as_str().map(str::to_owned);
                    if let Some(id) = &self.helper_id {
                        arena.family428650_actor_mut(actor).unwrap().helper_218 =
                            self.detached_helpers.remove(id);
                    }
                }
                "current_id" => {
                    let state = arena.derived_actor_mut(actor).unwrap();
                    if let Some(clone) = state.target_148 {
                        self.targets.insert("clone".into(), clone);
                    }
                    state.target_148 = value.as_str().map(|id| self.targets[id]);
                }
                _ => panic!("unexpected portable notify mutation {key}"),
            }
        }
        if self.spec["notify"]["outcome"] == "stop" {
            Err("notify")
        } else {
            Ok(())
        }
    }
}

fn compare_stop(
    result: &Result<(), FamilyAttackUpdateError<&'static str>>,
    expected: &Value,
    name: &str,
) {
    let Some(stop) = expected["stopped"].as_str() else {
        assert_eq!(result, &Ok(()), "{name}");
        return;
    };
    let error = result.as_ref().expect_err(name);
    match (stop, error) {
        (
            "allocation",
            FamilyAttackUpdateError::Target(NativeTargetError::Heap(
                ActorServiceHeapError::FixtureFailure(71),
            )),
        )
        | ("notify", FamilyAttackUpdateError::Helper("notify"))
        | (
            "process_null",
            FamilyAttackUpdateError::Target(NativeTargetError::MissingProcessNull),
        )
        | (
            "registry",
            FamilyAttackUpdateError::Target(NativeTargetError::Object(
                ObjectError::MissingActorRegistry,
            )),
        )
        | ("registry", FamilyAttackUpdateError::Object(ObjectError::MissingActorRegistry))
        | (
            "referent_position",
            FamilyAttackUpdateError::Target(NativeTargetError::Object(
                ObjectError::TargetObjectPositionUnknown(_),
            )),
        )
        | (
            "released_target",
            FamilyAttackUpdateError::Object(ObjectError::ActorServiceHeap(
                ActorServiceHeapError::Released(_),
            )),
        )
        | ("null_current", FamilyAttackUpdateError::MissingCurrentTarget(_)) => {}
        (s, FamilyAttackUpdateError::Uninitialized { offset, .. })
            if s == format!("unknown_actor_{offset:03x}") => {}
        (s, FamilyAttackUpdateError::Object(ObjectError::ActorPositionUnknown(_)))
            if s.starts_with("unknown_actor_03") => {}
        (s, FamilyAttackUpdateError::UninitializedHelper { offset, .. })
            if s == format!("unknown_helper_helper_{offset:02x}") => {}
        (s, FamilyAttackUpdateError::UninitializedFocus { .. })
            if s.starts_with("unknown_target_") => {}
        _ => panic!("{name}: native {stop}, Rust {error:?}"),
    }
}

#[test]
fn unchanged_native_attack_and_closed_target_owners_match_every_portable_receipt() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/actor_attack_update_native.json"
    )))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 406);
    assert_eq!(fixture["portable_cases"], 390);
    let mut compared = 0;
    for case in cases {
        let spec = &case["input"];
        let name = spec["name"].as_str().unwrap();
        assert_eq!(case["instruction_bytes_unchanged"], true, "{name}");
        assert_eq!(case["referent_unchanged"], true, "{name}");
        assert_eq!(case["unrelated_actor_bytes_unchanged"], true, "{name}");
        if spec["portable_compare"] != true {
            assert!(spec["source_only_reason"].as_str().is_some());
            continue;
        }
        compared += 1;
        let null_handle = word(&spec["process_null_handle"]).unwrap_or(0);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(null_handle),
        })
        .unwrap();
        let actor = arena
            .allocate_family428650_actor(construction(), &mut 0)
            .unwrap();
        let referent = arena.allocate_actor(construction().actor).unwrap();
        let original_handle = arena.actor(referent).unwrap().handle;
        arena
            .actor_handles
            .as_mut()
            .unwrap()
            .release(original_handle);
        let fixture_handle = ActorHandle::from_raw(0x0801_0001);
        arena.actor_mut(referent).unwrap().handle = fixture_handle;
        if spec["referent"]["resolves"] == true {
            arena
                .actor_handles
                .as_mut()
                .unwrap()
                .bind(fixture_handle, referent)
                .unwrap();
        }
        let live = arena.actor_mut(referent).unwrap();
        live.word_74 = word(&spec["referent"]["raise_word"]).unwrap() as u16;
        live.words_8e_92 = point(&spec["referent"]["velocity_words"]).unwrap();
        live.word_c4 = word(&spec["referent"]["lifecycle"]).unwrap();
        let base = &mut arena.owned_mut(referent).unwrap().state.base;
        base.position_words = point(&spec["referent"]["position_words"]);
        base.flags = word(&spec["referent"]["flags"]).unwrap();
        arena.owned_mut(actor).unwrap().state.base.position_words = point(&spec["position_words"]);
        let targets: BTreeMap<String, NativeHeapRef> = spec["targets"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, data)| {
                (
                    id.clone(),
                    arena.import_fixture_target(target(data, referent)),
                )
            })
            .collect();
        let current_id = spec["current_id"].as_str();
        let released: Vec<&str> = spec["released_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for id in &released {
            arena.release_actor_service(targets[*id]).unwrap();
        }
        let state = arena.derived_actor_mut(actor).unwrap();
        state.target_148 = current_id.map(|id| targets[id]);
        state.word_14c = word(&spec["distance14C"]);
        state.word_150 = word(&spec["state150"]);
        state.word_164 = word(&spec["radius164"]).unwrap() as i32;
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.words_188_19c[1] = word(&spec["timer18C"]);
        family.word_22e = word(&spec["flags22E"]).unwrap() as u16;
        for (ordinal, index) in [(0, 0), (1, 4)] {
            let id = spec["source_ids"][ordinal].as_str();
            family.targets_1d4_1fc[index] = id.map(|id| targets[id]);
            family.words_1d8_200[index] = word(&spec["scores"][ordinal]).unwrap();
        }
        family.helper_218 = helper(&spec["helper"]);
        let helper_id = family.helper_218.as_ref().map(|_| "helper".to_owned());
        // These are independent native sources. Only reached follow resolve
        // needs unavailable548888; fixed/object clone needs548894 instead.
        // The fresh process getter and actor resolver remain distinct owners.
        let refresh_missing_registry =
            spec["registry_available"] == false && case["stopped"] == "registry";
        if refresh_missing_registry {
            arena.actor_handles = None;
        }
        let initial_rng = word(&spec["rng"]).unwrap_or(0);
        let mut rng = initial_rng;
        let mut host = ReceiptHost {
            spec,
            targets,
            detached_helpers: BTreeMap::new(),
            helper_id,
            records: vec![],
        };
        if let Some(replacement) = helper(&spec["replacement_helper"]) {
            host.detached_helpers
                .insert("replacement".into(), replacement);
        }
        let allocation_start = arena.actor_service_heap().attempts().len();
        let allocation = match spec["allocation"].as_str().unwrap() {
            "ok" => FixtureActorServiceAllocation::Allocate,
            "null" => FixtureActorServiceAllocation::Null,
            "stop" => FixtureActorServiceAllocation::Error(71),
            _ => unreachable!(),
        };
        arena.actor_service_heap_mut().script([allocation]);
        let result = arena.update_family_attack(
            actor,
            FamilyAttackUpdateRequest {
                microseconds: word(&spec["delta"]).unwrap(),
                settings_7c: word(&spec["settings_7c"]).unwrap(),
            },
            &mut rng,
            &mut host,
        );
        compare_stop(&result, case, name);
        if let Some(expected_rng) = word(&case["rng"]) {
            assert_eq!(rng, expected_rng, "{name}: RNG");
        } else {
            assert_eq!(rng, initial_rng, "{name}: bypass unavailable RNG storage");
        }
        let state = arena.derived_actor(actor).unwrap();
        let family = arena.family428650_actor(actor).unwrap();
        let expected_actor = &case["actor"];
        assert_eq!(
            json!(family.words_188_19c[1]),
            expected_actor["timer18C"],
            "{name}: timer"
        );
        assert_eq!(
            json!(state.word_14c),
            expected_actor["distance14C"],
            "{name}: previous distance"
        );
        assert_eq!(
            json!(state.word_150),
            expected_actor["state150"],
            "{name}: previous lifecycle"
        );
        assert_eq!(
            json!(state.word_164 as u32),
            expected_actor["radius164"],
            "{name}: radius"
        );
        assert_eq!(
            json!(arena.object(actor).unwrap().base.position_words),
            expected_actor["position_words"],
            "{name}: position"
        );
        assert_eq!(
            json!(family.word_22e),
            expected_actor["flags22E"],
            "{name}: flags"
        );
        assert_eq!(
            state.target_148.is_some_and(|reference| arena
                .actor_service_heap()
                .state(reference)
                .unwrap()
                == ActorServiceResourceState::Released),
            expected_actor["current_custody"] == "released",
            "{name}: current custody"
        );
        if expected_actor["current_custody"] == "released" {
            assert!(
                state.target_148.is_some(),
                "{name}: native raw freed pointer retained"
            );
        } else if let Some(id) = expected_actor["current_id"].as_str() {
            compare_target(
                arena.native_target(state.target_148.expect(name)).unwrap(),
                &case["targets"][id],
                referent,
                name,
            );
        } else {
            assert_eq!(state.target_148, None, "{name}: genuine null publication");
        }
        for (ordinal, index) in [(0, 0), (1, 4)] {
            assert_eq!(
                json!(family.words_1d8_200[index]),
                expected_actor["scores"][ordinal],
                "{name}: score"
            );
            if let Some(id) = expected_actor["source_ids"][ordinal].as_str() {
                if arena
                    .actor_service_heap()
                    .state(family.targets_1d4_1fc[index].unwrap())
                    .unwrap()
                    == ActorServiceResourceState::Released
                {
                    assert_eq!(case["targets"][id]["custody"], "released");
                    assert!(family.targets_1d4_1fc[index].is_some());
                } else {
                    compare_target(
                        arena
                            .native_target(family.targets_1d4_1fc[index].unwrap())
                            .unwrap(),
                        &case["targets"][id],
                        referent,
                        name,
                    );
                }
            } else {
                assert_eq!(family.targets_1d4_1fc[index], None);
            }
        }
        assert_eq!(
            json!(host.helper_id),
            expected_actor["helper_id"],
            "{name}: helper identity"
        );
        if let Some(id) = &host.helper_id {
            let live = family.helper_218.as_ref().unwrap();
            assert_eq!(
                json!(live.words_00_04),
                case["helpers"][id]["words00_04"],
                "{name}: helper words"
            );
            assert_eq!(
                json!(live.word_08),
                case["helpers"][id]["count08"],
                "{name}: helper count"
            );
            assert_eq!(
                live.records,
                [crate::object_runtime::FamilyHelperHistoryRecord::UNKNOWN; 4],
                "{name}: notification service owns record words"
            );
        } else {
            assert_eq!(family.helper_218, None);
        }
        for (id, detached) in &host.detached_helpers {
            assert_eq!(
                json!(detached.words_00_04),
                case["helpers"][id]["words00_04"],
                "{name}: detached helper words"
            );
            assert_eq!(
                json!(detached.word_08),
                case["helpers"][id]["count08"],
                "{name}: detached helper count"
            );
        }
        let events = case["events"].as_array().unwrap();
        let allocations: Vec<u32> = events
            .iter()
            .filter(|e| e["kind"] == "allocate")
            .map(|e| word(&e["bytes"]).unwrap())
            .collect();
        assert_eq!(
            arena.actor_service_heap().attempts()[allocation_start..]
                .iter()
                .map(|kind| kind.allocation_bytes() as u32)
                .collect::<Vec<_>>(),
            allocations,
            "{name}: reached allocations"
        );
        let notifications:Vec<Value> = events.iter().filter(|e|e["kind"]=="notify").map(|e|json!({
            "helper_id":e["helper_id"],"handle_04":e["handle_04"],"vector_words":e["vector_words"],"strength":e["strength"],
        })).collect();
        assert_eq!(
            host.records, notifications,
            "{name}: captured notification inputs"
        );
        // Actor identities and referents survive resource-only release.
        assert!(arena.object(actor).is_ok());
        assert!(arena.object(referent).is_ok());
    }
    assert_eq!(compared, 390);
}
