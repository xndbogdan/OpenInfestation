//! Original-byte +100 and inline-table comparisons. Shared fixture phases
//! preserve actual gate/cache/factory composition, while this driver retains
//! its own operation, result, row and stopped-boundary admission.

use super::native_support::*;
use super::*;
use crate::object_runtime::{
    BehaviorParticleAdapterError, BehaviorPredicateError, NativeTargetClass,
    ProcessActorBehaviorTables,
};
use crate::process_particles::factories::{ParticleFactoryContext, ParticleFactoryError};
use inf_formats::FrontendExecutable;
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

fn supplied_selection(value: &Value, ids: &[ObjectId; 4]) -> FamilyTargetSelection {
    FamilyTargetSelection {
        score: number(&value["score"]),
        candidate: match value["candidate"].as_str() {
            Some("unwritten") => FamilySelectionCandidate::Unwritten,
            Some(name) => {
                FamilySelectionCandidate::Actor(ids[NAMES.iter().position(|v| *v == name).unwrap()])
            }
            None if value["candidate"].is_null() => FamilySelectionCandidate::Null,
            other => panic!("unadmitted selection {other:?}"),
        },
    }
}

fn compare_selection(
    actual: FamilyTargetSelection,
    raw: &[Option<u8>],
    ids: &[ObjectId; 4],
    name: &str,
) {
    assert_eq!(Some(actual.score), dword(&raw[4..8]), "{name} caller score");
    let expected = match dword(&raw[..4]) {
        None => FamilySelectionCandidate::Unwritten,
        Some(0) => FamilySelectionCandidate::Null,
        Some(pointer) => FamilySelectionCandidate::Actor(
            ids[ADDRESSES.iter().position(|v| *v == pointer).unwrap()],
        ),
    };
    assert_eq!(actual.candidate, expected, "{name} caller pointer custody");
}

#[test]
fn family_selection_matches_composed_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_selection_native.json"
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
        json!({"histories":64,"steps":68,"native_entries":999,"kernel_histories":57,"semantic_prefix_histories":2,"diagnostic_histories":5,"kernel_steps":61,"semantic_prefix_steps":2,"diagnostic_steps":5})
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
            assert!(
                matches!(
                    name,
                    "prune-oversized"
                        | "scanner-source-equality-bypasses-unknown-weights"
                        | "scanner-primary-only-lazy04"
                        | "scanner-known-xz-unknown-y"
                        | "prune-unknown-weight-copy-custody"
                ),
                "new admission exclusion requires an explicit comparison decision"
            );
            diagnostics += 1;
            continue;
        }
        assert!(matches!(
            input["evidence"].as_str(),
            Some("kernel" | "semantic_boundary_prefix")
        ));
        let (mut arena, ids) = supplied_arena(input, FixtureLayout::InlineTable100);
        let ids = ids.map(Option::unwrap);
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
                tasks
                    .construct(class as u32, ActorHandle::from_raw(source as u32))
                    .unwrap();
            }
        }
        let order = Rc::new(RefCell::new(ServiceOrder::default()));
        let mut host = Services {
            phase,
            animation_clock: None,
            ids: ids.map(Some),
            tasks,
            layout: FixtureLayout::InlineTable100,
            input: input.clone(),
            retired_tasks: vec![],
            invocation: Value::Null,
            order: Rc::clone(&order),
            network_index: 0,
            impact_index: 0,
            scratch: None,
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
                .filter(|event| {
                    matches!(
                        event["kind"].as_str(),
                        Some("supplied_service" | "required_boundary")
                    ) && event["operation"] != "target_free"
                })
                .cloned()
                .collect();
            let mut allocator = AllocationPlan {
                order: Rc::clone(&order),
                classes: &[NativeTargetClass::ActorFollow],
                plans: invocation["allocations"].as_array().unwrap().clone(),
                before: arena.actor_service_heap().attempts().len(),
                bytes: Vec::new(),
            };
            let table = FamilyTargetTable {
                owner: ids[NAMES
                    .iter()
                    .position(|v| *v == invocation["table"].as_str().unwrap())
                    .expect("diagnostic probe excluded")],
            };
            allocator.install(&mut arena);
            let mut primary = supplied_selection(&invocation["primary"], &ids);
            let mut secondary = supplied_selection(&invocation["secondary"], &ids);
            let operation = invocation["operation"].as_str().unwrap();
            let mut adapter = BehaviorParticleAdapter {
                particles: &mut pool,
                executable: exe,
                services: &mut host,
            };
            let result: Result<Value, String> = match operation {
                "resolve" => arena
                    .resolve_family_table_entry(table, number(&invocation["index"]), live.process)
                    .map(|id| {
                        id.map(|id| json!(NAMES[ids.iter().position(|v| *v == id).unwrap()]))
                            .unwrap_or(Value::Null)
                    })
                    .map_err(|e| format!("{e:?}")),
                "invalidate" => arena
                    .clear_family_table_entry(table, number(&invocation["index"]), live.process)
                    .map(|()| Value::Null)
                    .map_err(|e| format!("{e:?}")),
                "prune" => arena
                    .prune_family_target_table(table, live.process)
                    .map(|()| Value::Null)
                    .map_err(|e| format!("{e:?}")),
                "scan" => {
                    let outputs = if invocation["mode"] == "both" {
                        FamilyTargetScanOutputs::Both {
                            primary: &mut primary,
                            secondary: &mut secondary,
                        }
                    } else {
                        FamilyTargetScanOutputs::PrimaryOnly(&mut primary)
                    };
                    let result = arena.scan_family_targets(
                        FamilyTargetScanRequest {
                            actor: ids[0],
                            table,
                            center_words: xyz(&invocation["center_words"]),
                        },
                        outputs,
                        &mut live,
                        &mut adapter,
                    );
                    if input["evidence"] == "semantic_boundary_prefix" {
                        let expected = match name {
                            "scanner-empty-class-outside" => TaskStorageError::Classification(19),
                            "scanner-empty-task-missing" => TaskStorageError::NullOwner(3),
                            other => panic!("unknown semantic prefix {other}"),
                        };
                        assert!(
                            matches!(&result, Err(FamilyTargetScanError::Task(actual)) if actual == &expected),
                            "{name}: {result:?}"
                        );
                    }
                    if step["stopped"] == "impact_service_unavailable" {
                        assert!(
                            matches!(&result, Err(FamilyTargetScanError::Gate(FamilyCandidateGateError::Predicate(BehaviorPredicateError::Host {address:0x48d891,error:BehaviorParticleAdapterError::Factory(ParticleFactoryError::Impact(error))}))) if error == "unavailable"),
                            "{name}: {result:?}"
                        );
                    }
                    result.map(|()| Value::Null).map_err(|e| format!("{e:?}"))
                }
                "common" => {
                    let result = arena.select_family_targets(
                        FamilySelectionRequest { actor: ids[0] },
                        &mut live,
                        &mut adapter,
                    );
                    if step["stopped"] == "target_allocate_unavailable" {
                        assert!(
                            matches!(
                                &result,
                                Err(FamilySelectionError::Target(NativeTargetError::Heap(
                                    crate::object_runtime::ActorServiceHeapError::FixtureFailure(
                                        71
                                    )
                                )))
                            ),
                            "{name}: {result:?}"
                        );
                    }
                    result
                        .map(|value| json!(u8::from(value)))
                        .map_err(|e| format!("{e:?}"))
                }
                other => panic!("unadmitted operation {other}"),
            };
            if step["stopped"].is_null() {
                assert_eq!(
                    result.as_ref().ok(),
                    Some(&step["result"]),
                    "{name}: {result:?}"
                );
            } else {
                assert!(result.is_err(), "{name} expected stopped kernel");
                if input["evidence"] == "semantic_boundary_prefix" {
                    prefixes += 1;
                    assert_eq!(step["stopped"], "outside_storage_read");
                }
            }
            assert!(
                step["fault"].is_null(),
                "raw pointer fault diagnostics excluded"
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
            if operation == "scan" {
                let raw = bytes(&step["state"]["selection_storage"]["selection_output"]);
                compare_selection(primary, &raw[..8], &ids, name);
                compare_selection(secondary, &raw[8..], &ids, name);
            }
            compare_state(&pool, &arena, &live, &host, &step["state"], name);
            calls += 1;
        }
        histories += 1;
    }
    assert_eq!((histories, calls, prefixes, diagnostics), (59, 63, 2, 5));
}
