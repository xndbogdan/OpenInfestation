//! Original common +104 with actual pure queries, task relations, candidate
//! gate/cache/particle owners and all three weighted publishers. Shared fixture
//! phases do not share +100's selection policy. Supplied owner effects compare
//! live arena/task/list custody, not native constructor/destructor or unused
//! registry free-chain bytes. Scratch and retired-task errors are typed stopped
//! prefixes; native raw faults and stop IPs are not compared.

use super::super::family_selection::native_support::*;
use super::*;
use crate::{
    object_runtime::{
        BehaviorParticleAdapter, BehaviorParticleAdapterError, BehaviorPredicateError,
        NativeTargetClass, ProcessActorBehaviorTables,
    },
    process_particles::{
        factories::{ParticleFactoryContext, ParticleFactoryError, ParticleFactoryProcessState},
        ParticleError,
    },
    process_tasks::ProcessTaskRuntime,
};
use inf_formats::FrontendExecutable;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::{cell::RefCell, rc::Rc};

#[test]
fn family_actor_list_selection_matches_composed_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_actor_list_selection_native.json"
    )))
    .unwrap();
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        fixture["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(fixture["text_address"], 0x401000);
    assert_eq!(fixture["fpcw"], 0x027f);
    assert_eq!(
        fixture["counts"],
        json!({"histories":25,"steps":31,"native_entries":688,"kernel_histories":20,"semantic_prefix_histories":3,"diagnostic_histories":2,"authentic_query_entries":78,"authentic_query_returns":78,"authentic_task_entries":48,"original_complete_steps":22,"supplied_service_stop_steps":4,"semantic_prefix_steps":3,"diagnostic_steps":2})
    );
    assert_eq!(
        fixture["actor_addresses"],
        json!({"self":0x740000,"reference":0x741000,"a":0x742000,"b":0x743000})
    );
    assert_eq!(
        fixture["query_bytes"],
        json!({"4199168":"33c0c3","4212240":"8bc1c3","4204448":"8b8140010000c3"})
    );
    let query_keys: BTreeSet<u32> = fixture["query_matrix"]
        .as_object()
        .unwrap()
        .keys()
        .map(|key| key.parse().unwrap())
        .collect();
    assert_eq!(
        query_keys,
        BTreeSet::from([
            0x520e58, 0x520f34, 0x521164, 0x520b84, 0x520cc8, 0x521eac, 0x52273c, 0x527ee0,
            0x52104c, 0x527058, 0x5269c4, 0x520860, 0x51f080, 0x527de8, 0x51fbb0, 0x527c00,
            0x5204d0, 0x5289c0, 0x524ac0, 0x526b50,
        ])
    );
    for (vtable, query) in fixture["query_matrix"].as_object().unwrap() {
        let vtable: u32 = vtable.parse().unwrap();
        let shared = matches!(vtable, 0x527c00 | 0x5204d0 | 0x5289c0 | 0x524ac0 | 0x526b50);
        let self5c = matches!(vtable, 0x527058 | 0x5269c4);
        assert_eq!(
            *query,
            json!({"component58":if shared{0x401300}else{0x404610},"component5c":if self5c{0x404610}else{0x401300},"behavior7c":if shared{0x401300}else{0x4027a0}})
        );
    }
    assert_eq!(fixture["query_matrix"].as_object().unwrap().len(), 20);
    let callback_keys: BTreeSet<u32> = fixture["callback_matrix"]
        .as_object()
        .unwrap()
        .keys()
        .map(|key| key.parse().unwrap())
        .collect();
    assert_eq!(
        callback_keys,
        BTreeSet::from([
            0x520f34, 0x521164, 0x520b84, 0x520cc8, 0x527ee0, 0x52273c, 0x521eac, 0x52104c,
            0x527058, 0x5269c4,
        ])
    );
    for (vtable, callback) in fixture["callback_matrix"].as_object().unwrap() {
        assert_eq!(
            *callback,
            if matches!(vtable.parse::<u32>().unwrap(), 0x527058 | 0x5269c4) {
                json!(0x468350)
            } else {
                json!(0x428f40)
            }
        );
    }
    assert_eq!(fixture["callback_matrix"].as_object().unwrap().len(), 10);
    assert_eq!(
        fixture["task_callback_matrix"].as_object().unwrap().len(),
        19
    );
    for class in 0..19 {
        let (vtable, entry) = match class {
            0 => (0x5280a8, 0x4027b0),
            1 => (0x5280a4, 0x4efbf0),
            18 => (0x5280a0, 0x482c90),
            _ => (0x52809c, 0x482ce0),
        };
        assert_eq!(
            fixture["task_callback_matrix"][class.to_string()],
            json!({"vtable":vtable,"entry":entry})
        );
    }
    assert_eq!(
        fixture["semantic_prefix_cases"],
        json!([
            "missing-task-source-preempts-traversal",
            "notice-replaces-task-owner-before-later-primary",
            "scratch-cleanup-stopped-prefix"
        ])
    );
    assert_eq!(
        fixture["diagnostic_cases"],
        json!([
            "diagnostic-partial-candidate-y",
            "diagnostic-unbacked-live-next-link"
        ])
    );
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let exe = FrontendExecutable::parse(&raw).unwrap();
    let (mut histories, mut calls, mut prefixes, mut diagnostics) = (0, 0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        if input["evidence"] == "diagnostic" {
            assert!(matches!(
                name,
                "diagnostic-partial-candidate-y" | "diagnostic-unbacked-live-next-link"
            ));
            diagnostics += 1;
            continue;
        }
        assert!(matches!(
            input["evidence"].as_str(),
            Some("kernel" | "semantic_boundary_prefix")
        ));
        let (mut arena, ids) = supplied_arena(input, FixtureLayout::ActorList104);
        let phase =
            crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(
                Some(number(&input["phase"])),
            );
        let mut process = ParticleFactoryProcessState {
            network_flag: u32::from(input["network_active"].as_bool().unwrap()),
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(number(&input["process_null"]))),
        };
        let mut rng = number(&input["rng"]);
        let mut tables = ProcessActorBehaviorTables::from_executable(exe).unwrap();
        for (region, base, size) in [
            ("shared", 0x530e60, 860),
            ("registry", 0x53dba8, 528),
            ("default", 0x546a94, 4),
        ] {
            assert_eq!(
                tables.read_process_bytes(base, size).unwrap(),
                bytes(&fixture["seeds"][region])
            );
        }
        let mut pool = prepare_particles(
            input,
            &mut ParticleFactoryContext {
                process: &mut process,
                rng: &mut rng,
                executable: exe,
            },
            &phase,
        );
        let mut tasks = ProcessTaskRuntime::default();
        for (class, source) in input["tasks"].as_array().unwrap().iter().enumerate() {
            if let Some(source) = source.as_u64() {
                let owner = tasks
                    .construct(class as u32, ActorHandle::from_raw(source as u32))
                    .unwrap();
                assert_eq!(tasks.record(owner).unwrap().counters, [None; 5]);
            }
        }
        assert!(input["task_counters"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| *v == json!([null, null, null, null, null])));
        assert!(input["task_padding"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| *v == json!([null, null])));
        let order = Rc::new(RefCell::new(ServiceOrder::default()));
        let mut host = Services {
            phase,
            animation_clock: None,
            ids,
            tasks,
            invocation: Value::Null,
            order: Rc::clone(&order),
            network_index: 0,
            impact_index: 0,
            scratch: None,
            layout: FixtureLayout::ActorList104,
            input: input.clone(),
            retired_tasks: vec![],
        };
        let mut live = BehaviorPredicateContext {
            tables: &mut tables,
            process: &mut process,
            rng: &mut rng,
        };
        live.tables
            .write_process_bytes(
                0x530e60,
                &(number(&input["kind"]) as u16).to_le_bytes().map(Some),
            )
            .unwrap();
        for patch in input["patches"].as_array().unwrap() {
            host.apply(&mut arena, &mut live, &mut pool, None, patch);
        }
        for step in case["steps"].as_array().unwrap() {
            let invocation = &step["input"];
            assert_eq!(invocation["operation"], "actor_list");
            assert_eq!(bytes(&invocation["scratch_bytes"]), [None; 8]);
            for patch in invocation["patches"].as_array().unwrap() {
                host.apply(&mut arena, &mut live, &mut pool, None, patch);
            }
            host.invocation = invocation.clone();
            host.network_index = 0;
            host.impact_index = 0;
            host.scratch = None;
            order.borrow_mut().events = step["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| {
                    matches!(
                        e["kind"].as_str(),
                        Some("supplied_service" | "required_boundary")
                    ) && e["operation"] != "target_free"
                })
                .cloned()
                .collect();
            let mut allocator = AllocationPlan {
                order: Rc::clone(&order),
                plans: invocation["allocations"].as_array().unwrap().clone(),
                before: arena.actor_service_heap().attempts().len(),
                bytes: Vec::new(),
                classes: &[
                    NativeTargetClass::ActorFollow,
                    NativeTargetClass::FixedPosition,
                ],
            };
            allocator.install(&mut arena);
            let mut adapter = BehaviorParticleAdapter {
                particles: &mut pool,
                executable: exe,
                services: &mut host,
            };
            let result = arena.select_family_actor_list_targets(
                FamilySelectionRequest {
                    actor: ids[0].unwrap(),
                },
                &mut live,
                &mut adapter,
            );
            match step["stopped"].as_str() {
                None => {
                    let actual = match &result {
                        Ok(value) => u8::from(*value),
                        Err(error) => panic!("{name}: {error:?}"),
                    };
                    assert_eq!(actual, number(&step["result"]) as u8, "{name}");
                    assert_eq!(step["outcome"], "original_body_complete");
                }
                Some("target_allocate_unavailable") => {
                    assert!(
                        matches!(
                            &result,
                            Err(FamilyActorListSelectionError::Target(
                                NativeTargetError::Heap(
                                    crate::object_runtime::ActorServiceHeapError::FixtureFailure(
                                        71
                                    )
                                )
                            ))
                        ),
                        "{name}: {result:?}"
                    );
                    assert_eq!(step["outcome"], "supplied_service_stop");
                }
                Some("impact_service_unavailable") => {
                    assert!(
                        matches!(&result,Err(FamilyActorListSelectionError::Gate(FamilyCandidateGateError::Predicate(BehaviorPredicateError::Host {address:0x48d891,error:BehaviorParticleAdapterError::Factory(ParticleFactoryError::Impact(e))}))) if e=="unavailable"),
                        "{name}: {result:?}"
                    );
                    assert_eq!(step["outcome"], "supplied_service_stop");
                }
                Some("outside_storage_read") => {
                    assert_eq!(name, "missing-task-source-preempts-traversal");
                    assert!(
                        matches!(
                            &result,
                            Err(FamilyActorListSelectionError::Task(
                                TaskStorageError::NullOwner(3)
                            ))
                        ),
                        "{result:?}"
                    );
                    prefixes += 1;
                    assert_eq!(step["outcome"], "semantic_prefix_stop");
                }
                Some("outside_storage_write") => {
                    assert_eq!(name, "scratch-cleanup-stopped-prefix");
                    assert!(
                        matches!(
                            &result,
                            Err(FamilyActorListSelectionError::Gate(
                                FamilyCandidateGateError::Predicate(BehaviorPredicateError::Host {
                                    address: 0x48d8b0,
                                    error: BehaviorParticleAdapterError::Factory(
                                        ParticleFactoryError::Particle(ParticleError::MissingNode(
                                            _
                                        ))
                                    )
                                })
                            ))
                        ),
                        "{result:?}"
                    );
                    prefixes += 1;
                    assert_eq!(step["outcome"], "semantic_prefix_stop");
                }
                Some("retired_captured_task") => {
                    assert!(
                        matches!(
                            &result,
                            Err(FamilyActorListSelectionError::Relation(
                                TaskRelationError::Storage(TaskStorageError::MissingOwner(_))
                            ))
                        ),
                        "{result:?}"
                    );
                    prefixes += 1;
                    assert_eq!(step["outcome"], "semantic_prefix_stop");
                }
                other => panic!("new stopped admission {other:?}: {result:?}"),
            }
            assert!(
                step["fault"].is_null(),
                "raw-fault diagnostics are excluded"
            );
            assert!(
                order.borrow().events.is_empty(),
                "{name} lower service cadence"
            );
            allocator.compare(&arena);
            assert_eq!(
                host.network_index,
                invocation["network_services"].as_array().unwrap().len()
            );
            assert_eq!(
                host.impact_index,
                invocation["impact_services"].as_array().unwrap().len()
            );
            compare_state(&pool, &arena, &live, &host, &step["state"], name);
            calls += 1;
        }
        histories += 1;
    }
    assert_eq!((histories, calls, prefixes, diagnostics), (23, 29, 3, 2));
}
