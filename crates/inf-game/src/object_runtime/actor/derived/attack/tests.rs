use super::*;
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyHelper482ff0, FollowTargetRequest, NativeTarget,
    NativeTargetClass, ObjectClassKind,
};
use serde_json::{json, Value};
const UNUSED_SINE: [u16; 4096] = [0; 4096];

struct Host<'a> {
    spec: &'a Value,
    events: Vec<Value>,
    rng: u32,
}
impl crate::object_runtime::TargetNullHandleSource for Host<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
}
impl FamilyAttackHost for Host<'_> {
    type Error = &'static str;
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.events.push(json!(["random"]));
        if self.spec["failure"] == "random" {
            Err("random")
        } else {
            Ok(name_entry_rng_next(&mut self.rng))
        }
    }
    fn attack_sound(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        slot: u16,
    ) -> Result<(), Self::Error> {
        self.events.push(json!(["sound", slot]));
        if self.spec["failure"] == "sound" {
            return Err("sound");
        }
        if let Some(flags) = self.spec["sound_flags"].as_u64() {
            arena.family428650_actor_mut(actor).unwrap().word_22e = flags as u16;
        }
        Ok(())
    }
}

fn construction() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([1, 2, 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &UNUSED_SINE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

#[test]
fn exit_byte_requires_no_actor_or_service() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Base);
    let spec = json!({});
    let mut host = Host {
        spec: &spec,
        events: vec![],
        rng: 7,
    };
    assert_eq!(arena.change_family_attack(object, false, &mut host), Ok(()));
    assert!(host.events.is_empty());
}

#[test]
fn original_selection_publication_ranges_helper_and_failure_prefixes_match() {
    let native: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_attack_native.json"
    )))
    .unwrap();
    let cases = native["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 54);
    for (ordinal, expected) in cases.iter().enumerate() {
        let spec = &expected["input"];
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let actor = arena
            .allocate_family429e20_actor(construction(), &mut 0)
            .unwrap();
        let a = arena.allocate_actor(construction().actor).unwrap();
        let b = arena.allocate_actor(construction().actor).unwrap();
        arena.owned_mut(a).unwrap().state.base.word_28 = 100;
        arena.owned_mut(b).unwrap().state.base.word_28 = 65535;
        let a_handle = arena.actor(a).unwrap().handle;
        let b_handle = arena.actor(b).unwrap().handle;
        let mut targets = [None; 2];
        for (i, name, referent, handle) in [(0, "A", a, a_handle), (1, "B", b, b_handle)] {
            targets[i] = match spec[name].as_str() {
                Some("actor") => Some(NativeTarget::ActorFollow(
                    arena
                        .follow_target(FollowTargetRequest::resolve_actor(handle))
                        .unwrap(),
                )),
                Some("object") => Some(NativeTarget::ObjectPosition(
                    arena.object_position_target(referent).unwrap(),
                )),
                _ => None,
            };
        }
        let targets = targets.map(|target| target.map(|data| arena.import_fixture_target(data)));
        if let Some(released) = spec["released"].as_str() {
            if let Some(reference) = targets[if released == "A" { 0 } else { 1 }] {
                arena.release_actor_service(reference).unwrap();
            }
        }
        if spec["stale"] == true {
            arena.actor_handles.as_mut().unwrap().release(b_handle);
        }
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.targets_1d4_1fc[0] = targets[0];
        state.targets_1d4_1fc[4] = targets[1];
        state.words_1d8_200[0] = spec["score_a"].as_u64().unwrap_or(1) as u32;
        state.words_1d8_200[4] = spec["score_b"].as_u64().unwrap_or(2) as u32;
        state.word_228 = 99;
        state.words_188_19c[1] = Some(77);
        state.words_204_208[1] = Some(88);
        state.byte_22c = 7;
        state.word_22e = 12;
        if let Some(helper) = spec["helper"].as_array() {
            let handle = match helper[0].as_str() {
                Some("A") => a_handle.raw(),
                Some("B") => b_handle.raw(),
                _ => helper[0].as_u64().unwrap() as u32,
            };
            let mut owner = FamilyHelper482ff0::new(None);
            owner.words_00_04 = [Some(handle), Some(helper[1].as_u64().unwrap() as u32)];
            if let Some(unknown) = spec["helper_unknown"].as_u64() {
                owner.words_00_04[(unknown / 4) as usize] = None;
            }
            state.helper_218 = Some(Box::new(owner));
        }
        let old_target = if spec["old_target"] == true {
            Some(NativeTarget::ActorFollow(
                arena
                    .follow_target(FollowTargetRequest::resolve_actor(a_handle))
                    .unwrap(),
            ))
        } else {
            None
        };
        let old_target = old_target.map(|data| arena.import_fixture_target(data));
        let derived = arena.derived_actor_mut(actor).unwrap();
        derived.sound_words_17c_182 = if spec["sound_unknown"] == true {
            None
        } else {
            Some([0, 0, spec["sound"].as_u64().unwrap_or(0) as u16, 0])
        };
        derived.target_148 = old_target;
        arena.owned_mut(actor).unwrap().state.base.word_28 =
            spec["size"].as_u64().unwrap_or(400) as u16;
        let mut host = Host {
            spec,
            events: vec![],
            rng: spec["rng"].as_u64().unwrap_or(0x12345678) as u32,
        };
        let attempts_before = arena.actor_service_heap().attempts().len();
        arena
            .actor_service_heap_mut()
            .script([if spec["failure"] == "allocate" {
                crate::object_runtime::FixtureActorServiceAllocation::Error(71)
            } else if spec["allocation_null"] == true {
                crate::object_runtime::FixtureActorServiceAllocation::Null
            } else {
                crate::object_runtime::FixtureActorServiceAllocation::Allocate
            }]);
        let result = arena.change_family_attack(
            actor,
            spec["entering"].as_bool().unwrap_or(true),
            &mut host,
        );
        let error = match result {
            Ok(()) => None,
            Err(FamilyAttackError::Host { error, .. }) => Some(error),
            Err(FamilyAttackError::Target(NativeTargetError::Heap(
                crate::object_runtime::ActorServiceHeapError::FixtureFailure(71),
            ))) => Some("allocate"),
            Err(FamilyAttackError::MissingClone(_)) => Some("clone"),
            Err(FamilyAttackError::Uninitialized { offset: 0x180, .. }) => Some("sound_field"),
            Err(FamilyAttackError::UninitializedHelper { offset: 0, .. }) => Some("helper0"),
            Err(FamilyAttackError::UninitializedHelper { offset: 4, .. }) => Some("helper4"),
            Err(FamilyAttackError::Object(ObjectError::ActorServiceHeap(
                crate::object_runtime::ActorServiceHeapError::Released(_),
            )))
            | Err(FamilyAttackError::Target(NativeTargetError::Object(
                ObjectError::ActorServiceHeap(
                    crate::object_runtime::ActorServiceHeapError::Released(_),
                ),
            ))) => Some("released"),
            Err(other) => panic!("unexpected case{ordinal}: {other:?}"),
        };
        let state = arena.family428650_actor(actor).unwrap();
        let label = |word: u32| {
            if word == a_handle.raw() {
                json!("A")
            } else if word == b_handle.raw() {
                json!("B")
            } else {
                json!(word)
            }
        };
        let helper = state
            .helper_218
            .as_ref()
            .map(|h| json!([h.words_00_04[0].map(label), h.words_00_04[1]]));
        let target = arena
            .derived_actor(actor)
            .unwrap()
            .target_148
            .map(|reference| {
                let target = arena.native_target(reference).unwrap();
                let referent = target.object_referent();
                if referent == Some(a) || target.handle_04() == a_handle {
                    "A"
                } else {
                    "B"
                }
            });
        let actual = json!({"error":error,"events":host.events,"target":target,"timer":state.words_188_19c[1],"policy":state.word_228,
            "range":state.words_204_208[1],"enabled":state.byte_22c,"flags":state.word_22e,"helper":helper,"rng":host.rng});
        let mut expected = expected.clone();
        expected.as_object_mut().unwrap().remove("input");
        let expected_events = expected["events"].as_array().unwrap().clone();
        let expected_allocations: Vec<_> = expected_events
            .iter()
            .filter(|event| event[0] == "allocate")
            .map(|event| match event[1].as_str().unwrap() {
                "actor" => crate::object_runtime::ActorServiceResourceKind::Target(
                    NativeTargetClass::ActorFollow,
                ),
                "point" => crate::object_runtime::ActorServiceResourceKind::Target(
                    NativeTargetClass::FixedPosition,
                ),
                "object" => crate::object_runtime::ActorServiceResourceKind::Target(
                    NativeTargetClass::ObjectPosition,
                ),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            &arena.actor_service_heap().attempts()[attempts_before..],
            expected_allocations.as_slice()
        );
        expected["events"] = json!(expected_events
            .into_iter()
            .filter(|event| event[0] != "allocate")
            .collect::<Vec<_>>());
        assert_eq!(actual, expected, "native attack{ordinal}");
    }
}
