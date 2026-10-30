use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    BehaviorComponentQuery, BehaviorCreationRequest, BehaviorPacketCopy, BehaviorPacketSlot,
    BehaviorPacketSourceRequest, BehaviorTemporary, DerivedActorConstructionRequest,
    FamilyHelper482ff0, FamilyHelperHistoryRecord, ProcessActorBehaviorTables,
};
use serde_json::Value;

const SINE: [u16; 4096] = [0; 4096];

fn word(value: &Value) -> Option<u32> {
    value.as_u64().map(|value| value as u32)
}

fn position(value: &Value) -> Option<[u16; 3]> {
    value
        .as_array()
        .map(|values| std::array::from_fn(|axis| word(&values[axis]).unwrap() as u16))
}

fn helper(value: &Value) -> Option<Box<FamilyHelper482ff0>> {
    (!value.is_null()).then(|| {
        Box::new(FamilyHelper482ff0 {
            words_00_04: std::array::from_fn(|i| word(&value["header00_04"][i])),
            word_08: word(&value["count08"]).unwrap(),
            records: std::array::from_fn(|i| {
                let row = &value["records"][i];
                FamilyHelperHistoryRecord {
                    handle: word(&row["handle"]).map(ActorHandle::from_raw),
                    normal_bits: std::array::from_fn(|axis| word(&row["normal_bits"][axis])),
                    weight: word(&row["weight"]).map(|value| value as u16),
                    padding: word(&row["padding"]).map(|value| value as u16),
                }
            }),
        })
    })
}

fn behavior(present: bool) -> Option<super::super::ActorBehaviorState> {
    present.then(|| super::super::build_actor_behavior(1, 0, 3).unwrap())
}

struct Host<'a> {
    input: &'a Value,
    owner: ObjectId,
    candidate: ObjectId,
    creates: usize,
    sources: Vec<BehaviorPacketSlot>,
    deletes: usize,
}

impl BehaviorPredicateHost for Host<'_> {
    type Error = &'static str;
    type Particle = u32;
    type Component = u32;

    fn create(
        &mut self,
        arena: &mut ObjectArena,
        _context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorCreationRequest,
    ) -> Result<Option<BehaviorTemporary<u32>>, Self::Error> {
        assert!(matches!(
            request,
            BehaviorCreationRequest::ParticleFactory {
                callback: 0x466710,
                ..
            }
        ));
        self.creates += 1;
        let changes = self.input["effects"].as_object().unwrap();
        for (key, object) in [
            ("actor_position", self.owner),
            ("candidate_position", self.candidate),
        ] {
            if let Some(value) = changes.get(key) {
                arena.owned_mut(object).unwrap().state.base.position_words = position(value);
            }
        }
        if let Some(value) = changes.get("candidate_handle") {
            arena.actor_mut(self.candidate).unwrap().handle =
                ActorHandle::from_raw(word(value).unwrap());
        }
        if let Some(value) = changes.get("helper") {
            arena.family428650_actor_mut(self.owner).unwrap().helper_218 = helper(value);
        }
        if let Some(value) = changes.get("behavior") {
            arena.derived_actor_mut(self.owner).unwrap().behavior_140 =
                behavior(value.as_bool().unwrap());
        }
        if self.input["predicate_error"] == true {
            return Err("supplied predicate failure");
        }
        if self.input["actual_creation_null"] == true {
            assert!(matches!(
                request,
                BehaviorCreationRequest::ParticleFactory {
                    context_word: 0,
                    ..
                }
            ));
            return Ok(None);
        }
        Ok(Some(BehaviorTemporary::Particle(1)))
    }

    fn query_component(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        _: BehaviorComponentQuery<u32>,
    ) -> Result<Option<u32>, Self::Error> {
        panic!("particle cache arm does not query an actor component")
    }

    fn packet_source(
        &mut self,
        _: &ObjectArena,
        _: &BehaviorPredicateContext<'_>,
        request: BehaviorPacketSourceRequest<u32, u32>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error> {
        let BehaviorPacketSourceRequest::ParticlePointer { particle: 1, slot } = request else {
            panic!("unexpected source request: {request:?}");
        };
        self.sources.push(slot);
        Ok((slot == BehaviorPacketSlot::First).then(|| {
            let strength: u16 = if self.input["predicate_result"] == true {
                3000
            } else {
                0
            };
            let [low, high] = strength.to_le_bytes();
            BehaviorPacketCopy::Full([Some(low), Some(high), Some(1), None])
        }))
    }

    fn delete_particle(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        particle: u32,
        flags: u32,
    ) -> Result<(), Self::Error> {
        assert_eq!((particle, flags), (1, 1));
        self.deletes += 1;
        Ok(())
    }
}

#[test]
fn candidate_gate_matches_native_branches_mutable_prefixes_and_every_wrapped_tail() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_candidate_gate_native.json"
    )))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 225);
    assert_eq!(fixture["portable_cases"], 224);
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert_eq!(case["instruction_bytes_unchanged"], true, "{name}");
        if input["portable_compare"] != true {
            assert!(input["source_only_reason"].as_str().is_some());
            continue;
        }
        compared += 1;
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let construction = || ActorConstructionRequest {
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
        };
        let owner = arena
            .allocate_family428650_actor(
                DerivedActorConstructionRequest {
                    actor: construction(),
                    world_mask_limits: [0; 2],
                },
                &mut 0,
            )
            .unwrap();
        let candidate = arena.allocate_actor(construction()).unwrap();
        arena.owned_mut(owner).unwrap().state.base.position_words =
            position(&input["actor_position"]);
        arena
            .owned_mut(candidate)
            .unwrap()
            .state
            .base
            .position_words = position(&input["candidate_position"]);
        arena.actor_mut(candidate).unwrap().handle =
            ActorHandle::from_raw(word(&input["candidate_handle"]).unwrap());
        arena.family428650_actor_mut(owner).unwrap().helper_218 = helper(&input["helper"]);
        arena.derived_actor_mut(owner).unwrap().behavior_140 = behavior(input["behavior"] == true);
        // The gate compares raw handle values, without registry resolution.
        arena.actor_handles = None;
        let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
        tables
            .write_process_bytes(0x530e60, &[Some(2), Some(0)])
            .unwrap();
        tables.write_process_bytes(0x530e82, &[Some(0)]).unwrap();
        let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(0)),
        };
        let mut rng = 0;
        let mut host = Host {
            input,
            owner,
            candidate,
            creates: 0,
            sources: vec![],
            deletes: 0,
        };
        let result = arena.test_family_candidate_gate(
            FamilyCandidateGateRequest {
                actor: owner,
                candidate: (input["candidate"] == true).then_some(candidate),
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng,
            },
            &mut host,
        );
        let predicate_called = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "predicate")
            .count();
        assert_eq!(host.creates, predicate_called, "{name}: reached predicate");
        if predicate_called != 0 && input["predicate_error"] != true {
            assert_eq!(
                host.sources,
                [BehaviorPacketSlot::First, BehaviorPacketSlot::Second],
                "{name}"
            );
            assert_eq!(host.deletes, 1, "{name}");
        } else {
            assert!(host.sources.is_empty() && host.deletes == 0, "{name}");
        }
        match case["stopped"].as_str() {
            None => assert_eq!(result, Ok(word(&case["result"]).unwrap() != 0), "{name}"),
            Some("predicate_service") => assert!(
                matches!(
                    result,
                    Err(FamilyCandidateGateError::Predicate(
                        BehaviorPredicateError::Host {
                            address: 0x48d891,
                            ..
                        }
                    ))
                ),
                "{name}: {result:?}"
            ),
            Some("unknown_actor_32") => assert_eq!(
                result,
                Err(FamilyCandidateGateError::MissingPosition(owner)),
                "{name}"
            ),
            Some("unknown_candidate_32") => assert_eq!(
                result,
                Err(FamilyCandidateGateError::MissingPosition(candidate)),
                "{name}"
            ),
            Some("missing_current_helper") => assert_eq!(
                result,
                Err(FamilyCandidateGateError::Helper(
                    FamilyHelperHistoryError::MissingHelper(owner)
                )),
                "{name}"
            ),
            Some(stop) if stop.starts_with("unknown_helper0_") => {
                let offset = u16::from_str_radix(&stop[16..], 16).unwrap();
                assert_eq!(
                    result,
                    Err(FamilyCandidateGateError::Helper(
                        FamilyHelperHistoryError::Uninitialized {
                            actor: owner,
                            offset
                        }
                    )),
                    "{name}"
                );
            }
            Some(stop) => panic!("{name}: unhandled stop {stop}: {result:?}"),
        }
        let expected = |key: &str| {
            if predicate_called != 0 {
                input["effects"].get(key).unwrap_or(&input[key])
            } else {
                &input[key]
            }
        };
        assert_eq!(
            arena.object(owner).unwrap().base.position_words,
            position(expected("actor_position")),
            "{name}"
        );
        assert_eq!(
            arena.object(candidate).unwrap().base.position_words,
            position(expected("candidate_position")),
            "{name}"
        );
        assert_eq!(
            arena.actor(candidate).unwrap().handle.raw(),
            word(expected("candidate_handle")).unwrap(),
            "{name}"
        );
        assert_eq!(
            &arena.family428650_actor(owner).unwrap().helper_218,
            &helper(expected("helper")),
            "{name}: no invented helper writes"
        );
        assert_eq!(
            arena.derived_actor(owner).unwrap().behavior_140.is_some(),
            expected("behavior") == &Value::Bool(true),
            "{name}"
        );
    }
    assert_eq!(compared, 224);
}

#[test]
fn candidate_gate_absent_helper_failed_creation_does_not_dereference_a_stale_candidate() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let construction = || ActorConstructionRequest {
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
    };
    let owner = arena
        .allocate_family428650_actor(
            DerivedActorConstructionRequest {
                actor: construction(),
                world_mask_limits: [0; 2],
            },
            &mut 0,
        )
        .unwrap();
    let candidate = arena.allocate_actor(construction()).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: candidate,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    arena.derived_actor_mut(owner).unwrap().behavior_140 = behavior(true);
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let mut tables = ProcessActorBehaviorTables::from_executable(
        inf_formats::FrontendExecutable::parse(&raw).unwrap(),
    )
    .unwrap();
    tables
        .write_process_bytes(0x530e60, &[Some(2), Some(0)])
        .unwrap();
    tables.write_process_bytes(0x530e82, &[Some(0)]).unwrap();
    // The registry's factory selector zero is unsupported by 00466710,
    // so actual creation can return null before its allocator is reached.
    tables.write_process_bytes(0x53dbcc, &[Some(0); 4]).unwrap();
    let input = serde_json::json!({ "effects": {}, "actual_creation_null": true });
    let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0)),
    };
    let mut rng = 0;
    let mut host = Host {
        input: &input,
        owner,
        candidate,
        creates: 0,
        sources: vec![],
        deletes: 0,
    };
    assert_eq!(
        arena.test_family_candidate_gate(
            FamilyCandidateGateRequest {
                actor: owner,
                candidate: Some(candidate)
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host,
        ),
        Ok(true)
    );
    assert_eq!(host.creates, 1);
    assert!(host.sources.is_empty() && host.deletes == 0);
    assert_eq!(tables.read_process_bytes(0x530e82, 1).unwrap(), &[Some(0)]);
}
