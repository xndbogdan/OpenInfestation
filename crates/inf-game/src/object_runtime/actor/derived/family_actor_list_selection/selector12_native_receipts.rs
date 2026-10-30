//! Actual 00425D00 -> selector12 +104, composed with canonical interaction
//! queries, live task relations, gate/cache/factory bodies and four publishers.
//! Shared preparation and snapshots retain independent fixture layouts. Native
//! hidden EBX/stack observations are not observable Rust parity. Supplied
//! task/list/clock effects compare actual owners, not native constructors,
//! destructors, registry free-chain bytes or private pool free-stack order.

use super::super::family_selection::native_support::*;
use super::*;
use crate::{
    object_runtime::{
        ActorInteractionError, BehaviorParticleAdapter, BehaviorParticleAdapterError,
        BehaviorPredicateError, NativeTargetClass, ProcessActorBehaviorTables,
    },
    process_particles::{
        factories::{ParticleFactoryContext, ParticleFactoryError, ParticleFactoryProcessState},
        ParticleError,
    },
    process_tasks::ProcessTaskRuntime,
};
use inf_formats::FrontendExecutable;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

type SelectionResult =
    Result<bool, FamilyActorListSelectionError<BehaviorParticleAdapterError<String>>>;

fn assert_fixture_source(fixture: &Value) {
    assert_eq!(
        fixture["executable_sha256"],
        json!("bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956")
    );
    assert_eq!(fixture["text_address"], json!(4198400));
    assert_eq!(
        fixture["text_sha256"],
        json!("403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2")
    );
    assert_eq!(fixture["fpcw"], json!(639));
    assert_eq!(
        fixture["callback"],
        json!({"start":4621136,"end":4622038,"sha256":"012fa136bbec65a00e5e2ff79dc18586a553435277d8d9c19f9ea3bd1537e468","wrapper":4349184,"wrapper_bytes":"8b01ffa004010000"})
    );
    assert_eq!(
        fixture["counts"],
        json!({"histories":33,"steps":43,"kernel_histories":24,"semantic_prefix_histories":6,"diagnostic_histories":3,"native_entries":936})
    );
    assert_eq!(
        fixture["admission_counts"],
        json!({"kernel_steps":34,"semantic_prefix_steps":6,"diagnostic_steps":3,"admitted_histories":30,"admitted_steps":40})
    );
    assert_eq!(
        fixture["outcome_counts"],
        json!({"original_body_complete":29,"semantic_prefix_stop":6,"source_diagnostic":3,"supplied_service_stop":5})
    );
    assert_eq!(
        fixture["event_counts"],
        json!({"call":936,"canonical_query_enter":184,"canonical_query_return":181,"captured_scratch":15,"factory_scratch_return":14,"native_score_phase":42,"notification":15,"outside_storage_write":1,"publisher_enter":130,"read":3385,"resolve":37,"resolved":37,"selector12_list_callback_enter":43,"semantic_custody_boundary":1,"semantic_storage_boundary":1,"semantic_task_boundary":1,"supplied_effect":22,"supplied_owner_effect":5,"supplied_service":61,"task_relation_enter":30,"task_relation_return":30,"write":944})
    );
    assert_eq!(
        fixture["call_entry_counts"],
        serde_json::from_str::<Value>(r#"{"4199168":66,"4204240":12,"4204448":44,"4212240":44,"4349184":43,"4354320":130,"4355616":51,"4365488":18,"4368624":15,"4454496":6,"4596112":37,"4607536":13,"4607568":14,"4607648":14,"4607680":13,"4607696":15,"4607920":14,"4608080":14,"4612112":15,"4612352":15,"4612368":13,"4612384":14,"4612736":13,"4613584":2,"4613904":12,"4614224":1,"4615456":13,"4615504":2,"4621136":43,"4674272":11,"4697488":37,"4728272":30,"4728672":7,"4728896":10,"4730000":1,"4730080":19,"4756992":3,"4763856":2,"4767280":12,"4773920":18,"4779968":51,"4780128":1,"4793600":15,"5176304":10,"5315496":3}"#).unwrap()
    );
    assert_eq!(
        fixture["service_counts"],
        json!({"impact_489600":3,"network_notice":10,"target_allocate":38,"target_free":10})
    );
    assert_eq!(
        fixture["query_operation_counts"],
        json!({"candidate58":40,"interaction54":59,"notification9c":18,"numericCC":12,"self50":10,"self7c":43,"task5c":1,"task7c":1})
    );
    assert_eq!(
        fixture["query_body_entry_counts"],
        json!({"4199168":66,"4204240":12,"4204448":44,"4212240":44,"4454496":6,"4767280":12})
    );
    assert_eq!(
        fixture["query_bodies"],
        json!({"4199168":"33c0c3","4212240":"8bc1c3","4204240":"c20800","4767280":"8a818e01000084c07408b801000000c204008b44240485c074188b80b40000008b91b40000003bc27408b801000000c2040033c0c20400","4454496":"8a814202000084c07410a1c071540005c8000000898144020000c20800","4204448":"8b8140010000c3"})
    );
    assert_eq!(
        fixture["task_callbacks"],
        json!({"0":[5406888,4204464],"1":[5406884,5176304],"18":[5406880,4730000]})
    );
    assert_eq!(
        fixture["slots"],
        json!({"secondary":492,"resource":628,"grant":636,"primary":500})
    );
    assert_eq!(
        fixture["clock_backing"],
        json!({"value":5534144,"accumulated":5533488})
    );
    assert_eq!(fixture["layout"], json!("Selector12ActorList104"));
    assert_eq!(
        fixture["diagnostic_cases"],
        json!([
            "diagnostic-partial-xyz",
            "diagnostic-selector13-static-dispatch",
            "diagnostic-unknown-selector30-byte18e"
        ])
    );
    assert_eq!(
        fixture["semantic_prefix_cases"],
        json!([
            "missing-candidate-task-prefix",
            "missing-selector35-clock-prefix",
            "notice-retires-current-owner-before-cache-consumer",
            "scratch-cleanup-stopped-prefix",
            "unknown-selector35-byte242-prefix",
            "unrepresented-grant192-prefix"
        ])
    );

    let tables = BTreeSet::from([
        0x520e58, 0x520f34, 0x521164, 0x520b84, 0x520cc8, 0x521eac, 0x52273c, 0x527ee0, 0x52104c,
        0x527058, 0x5269c4, 0x520860, 0x51f080, 0x527de8, 0x51fbb0, 0x527c00, 0x5204d0, 0x5289c0,
        0x524ac0, 0x526b50,
    ]);
    for matrix in ["query_matrix", "interaction_matrix"] {
        let actual: BTreeSet<u32> = fixture[matrix]
            .as_object()
            .unwrap()
            .keys()
            .map(|key| key.parse().unwrap())
            .collect();
        assert_eq!(actual, tables, "exact canonical table keys");
    }
    for &table in &tables {
        let shared = matches!(table, 0x527c00 | 0x5204d0 | 0x5289c0 | 0x524ac0 | 0x526b50);
        let self5c = matches!(table, 0x527058 | 0x5269c4);
        assert_eq!(
            fixture["query_matrix"][table.to_string()],
            json!([
                if shared { 0x401300 } else { 0x404610 },
                if self5c { 0x404610 } else { 0x401300 },
                if shared { 0x401300 } else { 0x4027a0 }
            ])
        );
        assert_eq!(
            fixture["interaction_matrix"][table.to_string()],
            json!({
                "self50": 0x401300,
                "interaction54": if table == 0x5289c0 { 0x404610 } else { 0x401300 },
                "notification9c": if table == 0x52273c { 0x43f860 } else { 0x4026d0 }
            })
        );
    }
}

/// Typed errors compare the reached semantic prefix. Supplied lower stops,
/// unknown spans and retired owners do not claim native raw fault/IP parity.
fn assert_outcome(
    step: &Value,
    result: &SelectionResult,
    ids: &[Option<ObjectId>; 4],
    name: &str,
) -> bool {
    let prefix = match step["stopped"].as_str() {
        None => {
            let actual = match result {
                Ok(value) => u8::from(*value),
                Err(error) => panic!("{name}: {error:?}"),
            };
            assert_eq!(actual, number(&step["result"]) as u8, "{name}");
            assert_eq!(step["outcome"], "original_body_complete");
            false
        }
        Some("target_allocate_unavailable") => {
            assert!(
                matches!(
                    result,
                    Err(FamilyActorListSelectionError::Target(
                        NativeTargetError::Heap(
                            crate::object_runtime::ActorServiceHeapError::FixtureFailure(71)
                        )
                    ))
                ),
                "{name}: {result:?}"
            );
            assert_eq!(step["outcome"], "supplied_service_stop");
            false
        }
        Some("impact_service_unavailable") => {
            assert_eq!(name, "reclaim-third-impact-error-before-publishers");
            assert!(
                matches!(result, Err(FamilyActorListSelectionError::Gate(FamilyCandidateGateError::Predicate(BehaviorPredicateError::Host {
                    address: 0x48d891,
                    error: BehaviorParticleAdapterError::Factory(ParticleFactoryError::Impact(error))
                }))) if error == "unavailable"),
                "{name}: {result:?}"
            );
            assert_eq!(step["outcome"], "supplied_service_stop");
            false
        }
        Some("outside_storage_write") => {
            assert_eq!(name, "scratch-cleanup-stopped-prefix");
            assert!(
                matches!(
                    result,
                    Err(FamilyActorListSelectionError::Gate(
                        FamilyCandidateGateError::Predicate(BehaviorPredicateError::Host {
                            address: 0x48d8b0,
                            error: BehaviorParticleAdapterError::Factory(
                                ParticleFactoryError::Particle(ParticleError::MissingNode(_))
                            )
                        })
                    ))
                ),
                "{name}: {result:?}"
            );
            true
        }
        Some("unrepresented_grant_mask") => {
            assert_eq!(name, "unrepresented-grant192-prefix");
            assert!(
                matches!(
                    result,
                    Err(FamilyActorListSelectionError::Grant(
                        inf_formats::GrantClosureError::Id(192)
                    ))
                ),
                "{name}: {result:?}"
            );
            true
        }
        Some("missing_candidate_task") => {
            assert_eq!(name, "missing-candidate-task-prefix");
            assert!(
                matches!(
                    result,
                    Err(FamilyActorListSelectionError::Task(
                        TaskStorageError::NullOwner(3)
                    ))
                ),
                "{name}: {result:?}"
            );
            true
        }
        Some("unknown_animation_clock_0") => {
            assert_eq!(name, "missing-selector35-clock-prefix");
            assert!(
                matches!(
                    result,
                    Err(FamilyActorListSelectionError::Interaction(
                        ActorInteractionError::MissingAnimationClock
                    ))
                ),
                "{name}: {result:?}"
            );
            true
        }
        Some("unknown_a_242") => {
            assert_eq!(name, "unknown-selector35-byte242-prefix");
            assert!(
                matches!(result, Err(FamilyActorListSelectionError::Interaction(ActorInteractionError::Uninitialized { actor, offset: 0x242 })) if Some(*actor) == ids[2]),
                "{name}: {result:?}"
            );
            true
        }
        Some("retired_actor_read") => {
            assert_eq!(name, "notice-retires-current-owner-before-cache-consumer");
            assert!(
                matches!(result, Err(FamilyActorListSelectionError::Gate(FamilyCandidateGateError::Predicate(BehaviorPredicateError::Object(ObjectError::UnknownObject(actor))))) if Some(*actor) == ids[2]),
                "{name}: {result:?}"
            );
            true
        }
        other => panic!("new stopped admission {other:?}: {result:?}"),
    };
    if prefix {
        assert_eq!(step["outcome"], "semantic_prefix_stop");
    }
    assert!(
        step["fault"].is_null(),
        "raw faults stay outside typed admission"
    );
    prefix
}

fn compare_history(case: &Value, executable: FrontendExecutable<'_>) -> (usize, usize) {
    let input = &case["input"];
    let name = input["name"].as_str().unwrap();
    assert_eq!(input["composition"], "actual_gate_cache_factory");
    assert_eq!(input["actors"]["self"]["vtable"], 0x527058);
    // Constructor classes and the native table matrix are separate claims.
    // No selector13 constructor or arbitrary virtual override is admitted.
    for actor in ["reference", "a", "b"] {
        assert!(matches!(
            number(&input["actors"][actor]["vtable"]),
            0x520f34 | 0x52273c | 0x5289c0
        ));
    }
    let (mut arena, ids) = supplied_arena(input, FixtureLayout::Selector12ActorList104);
    // Registry construction backing stays explicit even when the independently
    // live process-null source is unavailable. No fallback is supplied here.
    let phase = crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(
        Some(number(&input["phase"])),
    );
    let mut process = ParticleFactoryProcessState {
        network_flag: u32::from(input["network_active"].as_bool().unwrap()),
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: input["process_null"]
            .as_u64()
            .map(|value| ActorHandle::from_raw(value.try_into().unwrap())),
    };
    let mut rng = number(&input["rng"]);
    let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
    let mut pool = prepare_particles(
        input,
        &mut ParticleFactoryContext {
            process: &mut process,
            rng: &mut rng,
            executable,
        },
        &phase,
    );
    let mut tasks = ProcessTaskRuntime::default();
    assert_eq!(input["tasks"].as_array().unwrap().len(), 19);
    for (class, source) in input["tasks"].as_array().unwrap().iter().enumerate() {
        if let Some(source) = source.as_u64() {
            let owner = tasks
                .construct(
                    class as u32,
                    ActorHandle::from_raw(source.try_into().unwrap()),
                )
                .unwrap();
            assert_eq!(tasks.record(owner).unwrap().counters, [None; 5]);
        }
    }
    for (key, value) in [
        ("task_counters", json!([null, null, null, null, null])),
        ("task_padding", json!([null, null])),
    ] {
        let supplied = input[key].as_array().unwrap();
        assert_eq!(supplied.len(), 19);
        assert!(supplied.iter().all(|item| *item == value));
    }
    let order = Rc::new(RefCell::new(ServiceOrder::default()));
    let mut host = Services {
        phase,
        ids,
        tasks,
        animation_clock: supplied_animation_clock(&input["animation_clock"]),
        invocation: Value::Null,
        order: Rc::clone(&order),
        network_index: 0,
        impact_index: 0,
        scratch: None,
        layout: FixtureLayout::Selector12ActorList104,
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
    let (mut calls, mut prefixes) = (0, 0);
    for step in case["steps"].as_array().unwrap() {
        let invocation = &step["input"];
        assert_eq!(invocation["operation"], "selector12_actor_list");
        assert_eq!(
            invocation["ignored_arguments"],
            json!([0xdead0000_u32, 0xffffffff_u32])
        );
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
        let publishers: Vec<_> = step["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "publisher_enter")
            .collect();
        assert!(publishers.len() <= 4);
        for (event, slot) in publishers
            .iter()
            .zip(["secondary", "resource", "grant", "primary"])
        {
            assert_eq!(event["slot"], slot, "{name} native publication order");
        }
        let mut allocator = AllocationPlan {
            order: Rc::clone(&order),
            plans: invocation["allocations"].as_array().unwrap().clone(),
            before: arena.actor_service_heap().attempts().len(),
            bytes: Vec::new(),
            classes: &[NativeTargetClass::ActorFollow],
        };
        allocator.install(&mut arena);
        let mut adapter = BehaviorParticleAdapter {
            particles: &mut pool,
            executable,
            services: &mut host,
        };
        let result = arena.select_family_actor_list_targets(
            FamilySelectionRequest {
                actor: ids[0].unwrap(),
            },
            &mut live,
            &mut adapter,
        );
        prefixes += usize::from(assert_outcome(step, &result, &host.ids, name));
        assert!(
            order.borrow().events.is_empty(),
            "{name} lower service cadence"
        );
        allocator.compare(&arena);
        assert_eq!(
            host.network_index,
            invocation["network_services"].as_array().unwrap().len(),
            "{name} notice cadence"
        );
        assert_eq!(
            host.impact_index,
            invocation["impact_services"].as_array().unwrap().len(),
            "{name} impact cadence"
        );
        if let Some(source) = step["events"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|event| event["kind"] == "factory_scratch_return")
        {
            // Compare the same eight-byte carrier before its native stack
            // address is reused. The factories consume only the first six.
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
    (calls, prefixes)
}

#[test]
fn selector12_actor_list_selection_matches_composed_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector12_actor_list_selection_native.json"
    )))
    .unwrap();
    assert_fixture_source(&fixture);
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let (mut histories, mut calls, mut prefixes, mut diagnostics) = (0, 0, 0, 0);
    let mut seen = BTreeSet::new();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert!(seen.insert(name), "unique named histories");
        match input["evidence"].as_str().unwrap() {
            "diagnostic" => {
                assert!(matches!(
                    name,
                    "diagnostic-partial-xyz"
                        | "diagnostic-selector13-static-dispatch"
                        | "diagnostic-unknown-selector30-byte18e"
                ));
                assert_eq!(case["steps"].as_array().unwrap().len(), 1);
                assert_eq!(case["steps"][0]["outcome"], "source_diagnostic");
                diagnostics += 1;
                continue;
            }
            "semantic_boundary_prefix" => assert!(fixture["semantic_prefix_cases"]
                .as_array()
                .unwrap()
                .contains(&input["name"])),
            "kernel" => assert!(!fixture["semantic_prefix_cases"]
                .as_array()
                .unwrap()
                .contains(&input["name"])),
            other => panic!("new evidence admission {other}"),
        }
        let (steps, stopped_prefixes) = compare_history(case, executable);
        assert_eq!(
            stopped_prefixes,
            usize::from(input["evidence"] == "semantic_boundary_prefix")
        );
        histories += 1;
        calls += steps;
        prefixes += stopped_prefixes;
    }
    assert_eq!((histories, calls, prefixes, diagnostics), (30, 40, 6, 3));
    assert_eq!(seen.len(), 33);
}
