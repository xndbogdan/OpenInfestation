//! Selector12's actual +100 policy, through425CF0/4686E0 and the real
//! resolver, radius score, canonical self7C, gate/cache/particle and target
//! owners. Fixture preparation and lower services share backing, not common
//! +100's traversal or expected winners. Native transient stack locals remain
//! source evidence; published scores/targets and typed mutable prefixes are
//! compared without claiming native raw-pointer fault or stop-IP equality.

use super::native_support::*;
use super::*;
use crate::object_runtime::{
    BehaviorParticleAdapterError, BehaviorPredicateError, NativeTargetClass,
    ProcessActorBehaviorTables,
};
use crate::process_particles::{
    factories::{ParticleFactoryContext, ParticleFactoryError},
    ParticleError,
};
use inf_formats::FrontendExecutable;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[test]
fn selector12_family_selection_matches_composed_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector12_target_selection_native.json"
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
        json!({"histories":20,"steps":26,"native_entries":575,"kernel_histories":16,"semantic_prefix_histories":2,"diagnostic_histories":2})
    );
    assert_eq!(
        fixture["admission_counts"],
        json!({"kernel_steps":22,"semantic_prefix_steps":2,"diagnostic_steps":2,"admitted_histories":18,"admitted_steps":24})
    );
    assert_eq!(
        fixture["outcome_counts"],
        json!({"original_body_complete":19,"semantic_prefix_stop":2,"source_diagnostic":2,"supplied_service_stop":3})
    );
    assert_eq!(
        fixture["supplied_service_counts"],
        json!({"impact_489600":3,"network_notice":2,"target_allocate":21,"target_free":5})
    );
    assert_eq!(
        fixture["candidate_copy_pairs"],
        json!({"4622416":0x468855,"4622433":0x46886c})
    );
    assert_eq!(
        fixture["actor_addresses"],
        json!({"self":0x740000,"reference":0x741000,"a":0x742000,"b":0x743000})
    );
    assert_eq!(fixture["wrapper_bytes"], "8b01ffa000010000");
    assert_eq!(fixture["query_bytes"], "8b8140010000c3");
    assert_eq!(
        fixture["dispatch_matrix"],
        json!({
            "5402712":{"component5c":0x404610,"behavior7c":0x4027a0,"selection100":0x4686e0,"selection104":0x468350},
            "5401028":{"component5c":0x404610,"behavior7c":0x4027a0,"selection100":0x4686e0,"selection104":0x468350}
        })
    );
    assert_eq!(
        fixture["diagnostic_cases"],
        json!([
            "diagnostic-bypassed-unknown-primary-weight",
            "diagnostic-empty-count-unknown-radius"
        ])
    );
    assert_eq!(
        fixture["semantic_prefix_cases"],
        json!([
            "scratch-cleanup-stopped-prefix",
            "unrepresented-fifth-inline-entry-prefix"
        ])
    );
    let mut entries: BTreeMap<String, u32> = BTreeMap::new();
    let (mut query_entries, mut query_returns) = (0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        for step in case["steps"].as_array().unwrap() {
            for event in step["events"].as_array().unwrap() {
                match event["kind"].as_str() {
                    Some("call") => {
                        *entries
                            .entry(number(&event["address"]).to_string())
                            .or_default() += 1;
                    }
                    Some("canonical_query_enter") => query_entries += 1,
                    Some("canonical_query_return") => query_returns += 1,
                    _ => {}
                }
            }
        }
    }
    assert_eq!(json!(&entries), fixture["native_entry_counts"]);
    assert_eq!((query_entries, query_returns), (27, 27));
    assert_eq!(entries.values().sum::<u32>(), 575);
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
                "diagnostic-bypassed-unknown-primary-weight"
                    | "diagnostic-empty-count-unknown-radius"
            ));
            diagnostics += 1;
            continue;
        }
        assert!(matches!(
            input["evidence"].as_str(),
            Some("kernel" | "semantic_boundary_prefix")
        ));
        let (mut arena, ids) = supplied_arena(input, FixtureLayout::Selector12Inline100);
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
        // Task backing is retained for unchanged-state comparison. This
        // distinct +100 policy performs no task query or source read.
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
            layout: FixtureLayout::Selector12Inline100,
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
            assert_eq!(invocation["operation"], "selector12_selection");
            assert_eq!(
                invocation["ignored_arguments"],
                json!([0xdead0000u32, 0xffffffffu32])
            );
            assert_eq!(bytes(&invocation["scratch_bytes"]), [None; 8]);
            let events = step["events"].as_array().unwrap();
            for address in [0x425cf0, 0x4686e0] {
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event["kind"] == "call" && event["address"] == address)
                        .count(),
                    1,
                    "{name} authentic wrapper/callback entry"
                );
            }
            for event in events {
                if event["kind"] == "read" {
                    assert!(!matches!(
                        event["region"].as_str(),
                        Some("task" | "task_registry")
                    ));
                    if event["region"] == "self" {
                        assert!(!matches!(
                            event["offset"].as_u64(),
                            Some(0x20c | 0x210 | 0x214)
                        ));
                    }
                }
                if matches!(
                    event["kind"].as_str(),
                    Some("canonical_query_enter" | "canonical_query_return")
                ) {
                    assert_eq!(event["receiver"], "self");
                    assert_eq!(event["execution"], "native_body");
                    assert_eq!(
                        event["address"],
                        if event["kind"] == "canonical_query_enter" {
                            0x4027a0
                        } else {
                            0x4027a6
                        }
                    );
                }
            }
            let publisher_slots: Vec<_> = events
                .iter()
                .filter(|event| event["kind"] == "publisher_enter")
                .map(|event| {
                    let slot = event["slot"].as_str().unwrap();
                    assert_eq!(
                        event["slot_address"],
                        if slot == "secondary" {
                            0x7401dc
                        } else {
                            0x7401d4
                        }
                    );
                    slot
                })
                .collect();
            assert!(matches!(
                publisher_slots.as_slice(),
                [] | ["secondary"] | ["secondary", "primary"]
            ));
            // These masks authenticate native stack custody only. Completed
            // winners/scores are compared through the published arena slots;
            // stopped unpublished locals have no exposed portable counterpart.
            for channel in ["primary", "secondary"] {
                let locals = &step["state"]["locals"];
                let score = dword(&bytes(&locals[format!("{channel}_score")])).unwrap();
                let candidate = bytes(&locals[format!("{channel}_candidate")]);
                if score == 0 {
                    assert_eq!(candidate, [None; 4]);
                } else {
                    assert!(ADDRESSES.contains(&dword(&candidate).unwrap()));
                }
            }
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
            allocator.install(&mut arena);
            let mut adapter = BehaviorParticleAdapter {
                particles: &mut pool,
                executable: exe,
                services: &mut host,
            };
            let result = arena.select_family_targets(
                FamilySelectionRequest { actor: ids[0] },
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
                            Err(FamilySelectionError::Target(NativeTargetError::Heap(
                                crate::object_runtime::ActorServiceHeapError::FixtureFailure(71)
                            )))
                        ),
                        "{name}: {result:?}"
                    );
                    assert_eq!(step["outcome"], "supplied_service_stop");
                }
                Some("impact_service_unavailable") => {
                    assert!(
                        matches!(&result, Err(FamilySelectionError::Gate(FamilyCandidateGateError::Predicate(BehaviorPredicateError::Host {address:0x48d891,error:BehaviorParticleAdapterError::Factory(ParticleFactoryError::Impact(error))}))) if error == "unavailable"),
                        "{name}: {result:?}"
                    );
                    assert_eq!(step["outcome"], "supplied_service_stop");
                }
                Some("outside_storage_write") => {
                    assert_eq!(name, "scratch-cleanup-stopped-prefix");
                    assert!(
                        matches!(
                            &result,
                            Err(FamilySelectionError::Gate(
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
                        "{name}: {result:?}"
                    );
                    assert_eq!(step["outcome"], "semantic_prefix_stop");
                    prefixes += 1;
                }
                Some("unrepresented_inline_entry") => {
                    assert_eq!(name, "unrepresented-fifth-inline-entry-prefix");
                    assert!(
                        matches!(
                            &result,
                            Err(FamilySelectionError::Table(
                                FamilyTargetTableError::UnrepresentedEntry { owner, index: 4 }
                            )) if *owner == ids[0]
                        ),
                        "{name}: {result:?}"
                    );
                    assert_eq!(step["outcome"], "semantic_prefix_stop");
                    prefixes += 1;
                }
                other => panic!("new stopped admission {other:?}: {result:?}"),
            }
            assert!(step["fault"].is_null(), "raw fault diagnostics excluded");
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
            if let Some(source) = events
                .iter()
                .rev()
                .find(|event| event["kind"] == "factory_scratch_return")
            {
                // The notice and its supplied writes retain this source
                // carrier. Compare it before native stack reuse, rather than
                // treating later bytes at that address as the old scratch.
                assert_eq!(source["execution"], "native_body");
                assert_eq!(
                    bytes(&source["bytes"]),
                    host.scratch.unwrap_or([None; 8]),
                    "{name} live creation-source custody"
                );
            }
            compare_state(&pool, &arena, &live, &host, &step["state"], name);
            calls += 1;
        }
        histories += 1;
    }
    assert_eq!((histories, calls, prefixes, diagnostics), (18, 24, 2, 2));
}
