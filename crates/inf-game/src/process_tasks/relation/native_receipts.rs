//! Unchanged task relation bodies and authentic nested-query receipts.
//!
//! This file is logically registered under object_runtime for fixture-only
//! field mutation. Native vtable and +140 pointer identities are supplied
//! lower-query backing; the host evaluates 401300/404610/4027A0 from those
//! inputs, rather than reading the oracle's expected result. Four explicitly
//! supplied mutable query histories retain their distinct service evidence.
//! Missing network/query services compare semantic prefixes, not raw fault IPs.

use std::{cell::Cell, collections::VecDeque};

use serde_json::{json, Value};

use super::*;
use crate::process_tasks::{
    relation::{TaskRelationContext, TaskRelationError, TaskRelationHost, TaskRelationRequest},
    ProcessTaskRuntime, TaskCallback, TaskOwner, TaskStorageError, TASK_CLASS_COUNT,
};

const ACTOR: u32 = 0x0072_5000;
const COMPONENT: u32 = 0x0072_6000;
const TASK: u32 = 0x0072_4000;
const FAMILY: u32 = 0x0052_0f34;
const SELECTOR12: u32 = 0x0052_7058;
const SELECTOR13: u32 = 0x0052_69c4;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_u64().map(|value| value.try_into().unwrap()))
        .collect()
}

fn dword(bytes: &[Option<u8>]) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}

fn write_scalar(bytes: &mut [Option<u8>], offset: usize, value: Option<u32>) {
    let value = value.map(u32::to_le_bytes);
    for byte in 0..4 {
        bytes[offset + byte] = value.map(|value| value[byte]);
    }
}

fn supplied_behavior() -> ActorBehaviorState {
    // Opaque owned presence for the supplied raw +140 identity. No constructor
    // or behavior entry is executed or inferred by this relation receipt.
    ActorBehaviorState {
        fire_head_00: None,
        firing_stop_00: None,
        sound_28: None,
        word_04: 0,
        word_08: 0,
        byte_0c: 0,
        byte_0d: 0,
        byte_0e: 0,
        byte_0f: 0,
        byte_10: 0,
        byte_11: 0,
        table_14: ActorBehaviorTable::Mode01,
        entries_18: vec![],
        groups_1c: vec![],
        word_20: 0,
        words_24: vec![],
        word_2c: 0,
        word_30: None,
        word_34: None,
        word_38: 0,
    }
}

fn set_presence(actor: &mut ActorState, present: bool) {
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("supplied derived actor")
    };
    derived.behavior_140 = present.then(supplied_behavior);
}

struct Backing {
    task: Vec<Option<u8>>,
    actors: [Vec<Option<u8>>; 2],
    network: Vec<Option<u8>>,
    registry: Vec<Option<u8>>,
}

impl Backing {
    fn new(input: &Value, callback: &Value) -> Self {
        let mut task = vec![None; 24];
        write_scalar(&mut task, 0, Some(number(&callback["vtable"])));
        write_scalar(&mut task, 4, Some(number(&input["task"]["classification"])));
        write_scalar(&mut task, 8, Some(number(&input["task"]["source"])));
        for (index, counter) in input["task"]["counters"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let counter = counter.as_u64().map(|value| (value as u16).to_le_bytes());
            task[12 + index * 2] = counter.map(|value| value[0]);
            task[13 + index * 2] = counter.map(|value| value[1]);
        }
        task[22..].copy_from_slice(&bytes(&input["task"]["padding"]));
        let actors = ["actor", "component"].map(|name| {
            let mut result = vec![None; 0x144];
            for (offset, field) in [(0, "vtable"), (0xb4, "classification"), (0x140, "behavior")] {
                write_scalar(
                    &mut result,
                    offset,
                    input[name][field].as_u64().map(|value| value as u32),
                );
            }
            result
        });
        let mut network = vec![None; 4];
        write_scalar(
            &mut network,
            0,
            input["network"].as_u64().map(|value| value as u32),
        );
        let mut registry = vec![Some(0); 4 * TASK_CLASS_COUNT];
        write_scalar(
            &mut registry,
            number(&input["task"]["classification"]) as usize * 4,
            Some(TASK),
        );
        Self {
            task,
            actors,
            network,
            registry,
        }
    }

    fn region(&mut self, name: &str) -> &mut Vec<Option<u8>> {
        match name {
            "task" => &mut self.task,
            "actor" => &mut self.actors[0],
            "component" => &mut self.actors[1],
            "network" => &mut self.network,
            "task_registry" => &mut self.registry,
            other => panic!("unadmitted region {other}"),
        }
    }
}

struct Host {
    backing: Backing,
    ids: [ObjectId; 2],
    expected_actors: [ActorState; 2],
    owner: TaskOwner,
    services: VecDeque<Value>,
    entries: Vec<Value>,
    returns: Vec<Value>,
    network_reads: Cell<usize>,
}

impl Host {
    fn actor_index(&self, id: ObjectId) -> usize {
        self.ids
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    }

    fn patch(&mut self, change: &Value, context: TaskRelationContext<'_>) {
        let region = change["region"].as_str().unwrap();
        let offset = number(&change["offset"]) as usize;
        let values = bytes(&change["bytes"]);
        self.backing.region(region)[offset..offset + values.len()].copy_from_slice(&values);
        match region {
            "actor" | "component" => {
                let index = usize::from(region == "component");
                let id = self.ids[index];
                for field_offset in [0, 0xb4, 0x140] {
                    if offset < field_offset + 4 && field_offset < offset + values.len() {
                        let value =
                            dword(&self.backing.actors[index][field_offset..field_offset + 4]);
                        match field_offset {
                            0 => {
                                // A supplied callback can replace native query-table
                                // identity. It is a lower-service input, not a claim
                                // that this creates a different full actor class.
                                assert!(matches!(value, Some(FAMILY | SELECTOR12 | SELECTOR13)));
                            }
                            0xb4 => {
                                let value = value.expect("admitted actor classification");
                                context.arena.actor_mut(id).unwrap().word_b4 = value;
                                self.expected_actors[index].word_b4 = value;
                            }
                            0x140 => {
                                let present = value.expect("admitted pointer identity") != 0;
                                set_presence(context.arena.actor_mut(id).unwrap(), present);
                                set_presence(&mut self.expected_actors[index], present);
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
            "network" => {}
            "task" => {
                assert_eq!(offset, 8, "only task-source mutation is admitted");
                assert_eq!(values.len(), 4);
                context
                    .tasks
                    .write_source(self.owner, ActorHandle::from_raw(dword(&values).unwrap()))
                    .unwrap();
            }
            other => panic!("no arbitrary native owner alias admitted: {other}"),
        }
    }

    fn effects(&mut self, effects: &Value, context: TaskRelationContext<'_>) {
        for effect in effects.as_array().unwrap() {
            match effect["operation"].as_str().unwrap() {
                "bytes" => self.patch(
                    &effect["patch"],
                    TaskRelationContext {
                        tasks: context.tasks,
                        arena: context.arena,
                    },
                ),
                "destroy_task" => {
                    let classification = context.tasks.record(self.owner).unwrap().classification;
                    context.tasks.destroy(self.owner).unwrap();
                    write_scalar(
                        &mut self.backing.registry,
                        classification as usize * 4,
                        Some(0),
                    );
                }
                other => panic!("unadmitted effect {other}"),
            }
        }
    }

    fn service(
        &mut self,
        operation: &str,
        address: u32,
        receiver: usize,
        context: TaskRelationContext<'_>,
    ) -> Option<Result<Value, String>> {
        let supplied = self
            .services
            .front()
            .is_some_and(|service| service["operation"] == operation);
        self.entries.push(json!({
            "operation": operation,
            "address": address,
            "receiver": (["actor", "component"][receiver]),
            "execution": if supplied {"supplied_service"} else {"native_body"},
        }));
        if !supplied {
            return None;
        }
        let service = self.services.pop_front().unwrap();
        self.effects(&service["effects"], context);
        Some(match service["error"].as_str() {
            Some(error) => Err(error.to_owned()),
            None => Ok(service["result"].clone()),
        })
    }
}

impl TaskRelationHost for Host {
    type Error = String;
    type Component = ObjectId;

    fn network_flag(&self) -> Option<u32> {
        self.network_reads.set(self.network_reads.get() + 1);
        dword(&self.backing.network)
    }

    fn actor_component_5c(
        &mut self,
        context: TaskRelationContext<'_>,
        actor: ObjectId,
    ) -> Result<Option<Self::Component>, Self::Error> {
        let index = self.actor_index(actor);
        let vtable = dword(&self.backing.actors[index][..4]).unwrap();
        let address = match vtable {
            FAMILY => 0x0040_1300,
            SELECTOR12 | SELECTOR13 => 0x0040_4610,
            other => panic!("unadmitted +5C table {other:08X}"),
        };
        if let Some(result) = self.service("component5c", address, index, context) {
            return result.map(|value| match value.as_str() {
                None if value.is_null() => None,
                Some("actor") => Some(self.ids[0]),
                Some("component") => Some(self.ids[1]),
                other => panic!("unadmitted component token {other:?}"),
            });
        }
        // Authentic 401300 is constant NULL; 404610 returns its receiver.
        let result = (vtable != FAMILY).then_some(actor);
        self.returns.push(json!({
            "operation":"component5c",
            "raw_result": result.map_or(0, |_| [ACTOR, COMPONENT][index]),
        }));
        Ok(result)
    }

    fn component_presence_7c(
        &mut self,
        context: TaskRelationContext<'_>,
        component: Self::Component,
    ) -> Result<u32, Self::Error> {
        let index = self.actor_index(component);
        assert!(matches!(
            dword(&self.backing.actors[index][..4]),
            Some(FAMILY | SELECTOR12 | SELECTOR13)
        ));
        if let Some(result) = self.service("behavior7c", 0x0040_27a0, index, context) {
            return result.map(|value| number(&value));
        }
        // Authentic 4027A0 returns exactly the supplied live DWORD +140.
        let result = dword(&self.backing.actors[index][0x140..0x144]).unwrap();
        self.returns
            .push(json!({"operation":"behavior7c", "raw_result":result}));
        Ok(result)
    }
}

fn arena(input: &Value) -> (ObjectArena, [ObjectId; 2], [ActorState; 2]) {
    const SINE: [u16; 4096] = [0; 4096];
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let ids = std::array::from_fn(|index| {
        let id = arena
            .allocate_derived_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: None,
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &SINE,
                        },
                    },
                    world_mask_limits: [0; 2],
                },
                &mut 0,
            )
            .unwrap();
        let supplied = &input[["actor", "component"][index]];
        let actor = arena.actor_mut(id).unwrap();
        if let Some(classification) = supplied["classification"].as_u64() {
            actor.word_b4 = classification as u32;
        }
        set_presence(
            actor,
            supplied["behavior"]
                .as_u64()
                .is_some_and(|value| value != 0),
        );
        id
    });
    let before = ids.map(|id| arena.actor(id).unwrap().clone());
    if input["actor"]["pointer"] != "actor" {
        // A stale typed ID admits only the constant body's actor bypass. Raw
        // null/unmapped dereference prefixes remain named native diagnostics.
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: ids[0],
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
    }
    (arena, ids, before)
}

fn assert_state(
    host: &Host,
    arena: &ObjectArena,
    tasks: &ProcessTaskRuntime,
    step: &Value,
    input: &Value,
) {
    let state = &step["state"];
    let storage = &state["storage"];
    assert_eq!(host.backing.task, bytes(&storage["task"]));
    assert_eq!(host.backing.network, bytes(&storage["network"]));
    assert_eq!(host.backing.registry, bytes(&storage["task_registry"]));
    for (index, name) in ["actor", "component"].into_iter().enumerate() {
        assert_eq!(host.backing.actors[index], bytes(&storage[name]));
        if index == 0 && input["actor"]["pointer"] != "actor" {
            assert!(arena.actor(host.ids[index]).is_err());
        } else {
            assert_eq!(
                arena.actor(host.ids[index]).unwrap(),
                &host.expected_actors[index]
            );
        }
    }
    let classification = number(&input["task"]["classification"]);
    let live = state["task_live"].as_bool().unwrap();
    for index in 0..TASK_CLASS_COUNT {
        assert_eq!(
            tasks.owner(index as u32).unwrap(),
            (live && index as u32 == classification).then_some(host.owner)
        );
    }
    if live {
        let record = tasks.record(host.owner).unwrap();
        assert_eq!(
            record.classification,
            dword(&host.backing.task[4..8]).unwrap()
        );
        assert_eq!(
            record.source.raw(),
            dword(&host.backing.task[8..12]).unwrap()
        );
        let counters: [Option<u16>; 5] = std::array::from_fn(|index| {
            let bytes = &host.backing.task[12 + index * 2..14 + index * 2];
            Some(u16::from_le_bytes([bytes[0]?, bytes[1]?]))
        });
        assert_eq!(record.counters, counters);
    } else {
        assert_eq!(
            tasks.record(host.owner),
            Err(TaskStorageError::MissingOwner(host.owner))
        );
        // Stale native task bytes above are supplied backing only. No freed
        // Rust record is retained or interpreted after owner retirement.
    }
}

#[test]
fn task_relations_match_original_bodies_and_supplied_mutable_prefixes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/process_task_relation_native.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(fixture["text_address"], 0x0040_1000);
    assert_eq!(
        fixture["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(fixture["fpcw"], 0x027f);
    assert_eq!(
        fixture["counts"],
        json!({
            "histories":42, "steps":42, "authentic_routine_entries":60,
            "supplied_query_entries":5, "authentic_query_returns":17,
            "kernel_histories":27, "supplied_service_kernel_histories":2,
            "semantic_prefix_histories":5, "diagnostic_histories":8,
            "completed_steps":31, "stopped_steps":9, "fault_steps":2,
        })
    );
    assert_eq!(
        fixture["task_callbacks"],
        json!({
            "zero4027b0":{"classification":0,"vtable":0x0052_80a8,"entry":0x0040_27b0},
            "zero4efbf0":{"classification":1,"vtable":0x0052_80a4,"entry":0x004e_fbf0},
            "component":{"classification":18,"vtable":0x0052_80a0,"entry":0x0048_2c90},
            "team":{"classification":3,"vtable":0x0052_809c,"entry":0x0048_2ce0},
        })
    );
    assert_eq!(
        fixture["actor_query_matrix"],
        json!({
            "5377844":{"component5c":0x0040_1300,"behavior7c":0x0040_27a0},
            "5402712":{"component5c":0x0040_4610,"behavior7c":0x0040_27a0},
            "5401028":{"component5c":0x0040_4610,"behavior7c":0x0040_27a0},
        })
    );
    let mut admitted = 0;
    let mut steps = 0;
    let mut diagnostics = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        if input["evidence"] == "diagnostic" {
            diagnostics.push(name);
            continue;
        }
        assert!(matches!(
            input["evidence"].as_str().unwrap(),
            "kernel" | "supplied_service_kernel" | "semantic_boundary_prefix"
        ));
        admitted += 1;
        let callback = &fixture["task_callbacks"][input["task"]["callback"].as_str().unwrap()];
        let (mut arena, ids, expected_actors) = arena(input);
        let mut tasks = ProcessTaskRuntime::default();
        let owner = tasks
            .construct(
                number(&input["task"]["classification"]),
                ActorHandle::from_raw(0),
            )
            .unwrap();
        tasks
            .write_source(
                owner,
                ActorHandle::from_raw(number(&input["task"]["source"])),
            )
            .unwrap();
        let actual_callback = match input["task"]["callback"].as_str().unwrap() {
            "zero4027b0" => TaskCallback::Zero4027b0,
            "zero4efbf0" => TaskCallback::Zero4efbf0,
            "team" => TaskCallback::TeamClassification,
            "component" => TaskCallback::ComponentClassification,
            other => panic!("unadmitted callback {other}"),
        };
        assert_eq!(tasks.record(owner).unwrap().callback, actual_callback);
        let mut host = Host {
            backing: Backing::new(input, callback),
            ids,
            expected_actors,
            owner,
            services: VecDeque::new(),
            entries: vec![],
            returns: vec![],
            network_reads: Cell::new(0),
        };
        for change in input["patches"].as_array().unwrap() {
            host.patch(
                change,
                TaskRelationContext {
                    tasks: &mut tasks,
                    arena: &mut arena,
                },
            );
        }
        for step in case["steps"].as_array().unwrap() {
            steps += 1;
            host.services = step["input"]["services"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .collect();
            host.entries.clear();
            host.returns.clear();
            host.network_reads.set(0);
            for change in step["input"]["patches"].as_array().unwrap() {
                host.patch(
                    change,
                    TaskRelationContext {
                        tasks: &mut tasks,
                        arena: &mut arena,
                    },
                );
            }
            let result = tasks.query_relation(
                TaskRelationRequest {
                    owner,
                    actor: ids[0],
                },
                &mut arena,
                &mut host,
            );
            match step["stopped"].as_str() {
                None => assert_eq!(result, Ok(number(&step["result"])), "{name}"),
                Some("unknown_network_0") => assert_eq!(
                    result,
                    Err(TaskRelationError::MissingNetworkFlag {
                        address: 0x0048_2cf7
                    }),
                    "{name}"
                ),
                Some(reason) => {
                    let (operation, address) = if reason.starts_with("component5c_") {
                        ("component5c", 0x0048_2caa)
                    } else if reason.starts_with("behavior7c_") {
                        ("behavior7c", 0x0048_2cb5)
                    } else {
                        panic!("unadmitted stopped prefix {name}: {reason}")
                    };
                    assert_eq!(
                        result,
                        Err(TaskRelationError::Host {
                            address,
                            error: reason
                                .strip_prefix(operation)
                                .unwrap()
                                .strip_prefix('_')
                                .unwrap()
                                .to_owned(),
                        }),
                        "{name}"
                    );
                }
            }
            assert!(host.services.is_empty(), "{name}");
            let events = step["events"].as_array().unwrap();
            let entries: Vec<_> = events.iter().filter(|event| event["kind"] == "routine_entry" && !event["operation"].is_null())
                .map(|event| json!({"operation":event["operation"],"address":event["address"],"receiver":event["receiver"],"execution":event["execution"]})).collect();
            assert_eq!(host.entries, entries, "{name}");
            let returns: Vec<_> = events.iter().filter(|event| event["kind"] == "native_query_return")
                .map(|event| json!({"operation":event["operation"],"raw_result":event["raw_result"]})).collect();
            assert_eq!(host.returns, returns, "{name}");
            assert_eq!(
                host.network_reads.get(),
                events
                    .iter()
                    .filter(|event| event["kind"] == "read" && event["region"] == "network")
                    .count(),
                "{name}"
            );
            assert!(
                step["fault"].is_null(),
                "semantic prefixes do not compare native faults: {name}"
            );
            assert_state(&host, &arena, &tasks, step, input);
        }
    }
    assert_eq!(admitted, 34);
    assert_eq!(steps, 34);
    assert_eq!(
        diagnostics,
        [
            "component-family-null5c-bypasses-unknown140",
            "team-unknown-actor-class-before-network",
            "component-unknown-actor-vtable",
            "component-actual-self-unknown140",
            "team-unmapped-actor-class",
            "component-unmapped-actor-class",
            "team-unknown-task-class-network0",
            "team-unknown-task-class-network1",
        ]
    );
}
