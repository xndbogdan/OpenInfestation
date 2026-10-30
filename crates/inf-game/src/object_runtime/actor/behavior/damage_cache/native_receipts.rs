//! Supplied-input original-byte oracle: 299 admitted cases, including three
//! standalone packet copies. Arbitrary intercepted disposal mutations/errors
//! remain native diagnostics: this owner runs actual ObjectArena::dispose.

use std::{collections::BTreeMap, sync::Arc};

use inf_formats::{
    parse_constructor_asset_bank, ActorConstructorPayload, ByteSpan, ConstructorPayloadView,
    HandlerFraming, HandlerId, HandlerRegion, RelocationTail,
};
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{ActorDamageTable, ObjectClassKind, ObjectList};

type Bytes = BTreeMap<String, Vec<Option<u8>>>;

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}
fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().map(|v| v as u8))
        .collect()
}
fn put(regions: &mut Bytes, name: &str, offset: usize, input: &[Option<u8>]) {
    regions.get_mut(name).unwrap()[offset..offset + input.len()].copy_from_slice(input);
}
fn dword(regions: &Bytes, name: &str, offset: usize) -> u32 {
    u32::from_le_bytes(
        regions[name][offset..offset + 4]
            .iter()
            .map(|v| v.unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
    )
}
fn base(name: &str) -> u32 {
    match name {
        "shared" => 0x530e60,
        "registry" => 0x53dba8,
        "default" => 0x546a94,
        "process_null" => 0x54e894,
        "vehicle0" => 0x72b26c,
        "vehicle1" => 0x72c26c,
        _ => panic!("non-process base {name}"),
    }
}
fn table(name: &str) -> ActorBehaviorTable {
    match name {
        "mode01" => ActorBehaviorTable::Mode01,
        "mode02" => ActorBehaviorTable::Mode02,
        "mode03" => ActorBehaviorTable::Mode03,
        "mode04" => ActorBehaviorTable::Mode04,
        "mode05" => ActorBehaviorTable::Mode05,
        "mode06" => ActorBehaviorTable::Mode06,
        "mode07" => ActorBehaviorTable::Mode07,
        "mode08" => ActorBehaviorTable::Mode08,
        "mode09" => ActorBehaviorTable::Mode09,
        "mode10" => ActorBehaviorTable::Mode10,
        "mode11" => ActorBehaviorTable::Mode11,
        "mode12" => ActorBehaviorTable::Mode12,
        "mode13" => ActorBehaviorTable::Mode13,
        "vehicle0" | "vehicle1" => ActorBehaviorTable::Selector2Copy,
        _ => panic!("unknown table {name}"),
    }
}
fn payload(responses: &[Option<u8>]) -> Arc<ActorConstructorPayload> {
    // Selector56 exposes the proven shared 0047AD70 prefix. Its class is not
    // constructed here; the candidate retains the canonical ten-pair payload.
    let mut source = vec![0xa5; 9];
    source.extend_from_slice(&55_u16.to_le_bytes());
    source.extend_from_slice(&u16::MAX.to_le_bytes());
    let mut data = [0; 0x42];
    for (offset, value) in responses.iter().enumerate() {
        data[0x10 + offset] = value.unwrap();
    }
    source.extend_from_slice(&data);
    let main = ByteSpan::new(9, source.len());
    let empty = ByteSpan::new(source.len(), source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let bank = parse_constructor_asset_bank(
        &source,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(9, source.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload: main,
                relocation_count: 0,
                relocation_offsets: empty,
                entry_offsets: ByteSpan::new(main.end, source.len()),
            }),
        },
    )
    .unwrap();
    let ConstructorPayloadView::SharedActor(value) = &bank.assets[0].payload else {
        panic!("shared actor payload")
    };
    Arc::new(value.clone())
}

fn baseline(metadata: &Value, input: &Value) -> Bytes {
    let mut regions: Bytes = [
        ("behavior", 0x18),
        ("candidate", 0xa8),
        ("temporary", 0x40),
        ("component", 0x14c),
        ("source0", 4),
        ("source1", 4),
        ("responses", 40),
        ("default", 4),
        ("process_null", 4),
    ]
    .into_iter()
    .map(|(name, size)| (name.into(), vec![None; size]))
    .collect();
    for (name, seed) in [
        ("shared", "initial_shared_bytes"),
        ("registry", "initial_registry_bytes"),
        ("vehicle0", "initial_vehicle_bytes"),
        ("vehicle1", "initial_vehicle_bytes"),
    ] {
        regions.insert(name.into(), bytes(&metadata[seed]));
    }
    for (name, offset, value) in [
        ("temporary", 0, 0x526efc_u32),
        ("temporary", 0x34, 0x728000),
        ("temporary", 0x38, 0x729000),
        ("component", 0, 0x528524),
        ("process_null", 0, 0x08010000),
        ("candidate", 0xa4, 0x72a000),
    ] {
        put(&mut regions, name, offset, &value.to_le_bytes().map(Some));
    }
    for (name, offset, value) in [
        (
            "component",
            0x13e,
            vec![Some(64), Some(0), Some(3), Some(0xa7)],
        ),
        ("component", 0x146, vec![Some(64), Some(0)]),
        ("component", 0x148, vec![Some(3)]),
        ("source0", 0, vec![Some(64), Some(0), Some(3), Some(0xa7)]),
        ("source1", 0, vec![Some(0), Some(0), Some(4), Some(0xb8)]),
        ("default", 0, vec![Some(0); 4]),
    ] {
        put(&mut regions, name, offset, &value);
    }
    let response_bytes: Vec<_> = input["responses"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|pair| {
            pair.as_array()
                .unwrap()
                .iter()
                .flat_map(|v| (word(v) as u16).to_le_bytes().map(Some))
        })
        .collect();
    put(&mut regions, "responses", 0, &response_bytes);
    regions
}

struct Host {
    regions: Bytes,
    vehicles: [ObjectId; 2],
    candidate: ObjectId,
    owner: ObjectId,
    temporary: ObjectId,
    invocation: Value,
    expected_services: Vec<Value>,
    service_index: usize,
    operations: Vec<String>,
    source_operations: Vec<String>,
}

impl Host {
    fn apply(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        patch: &Value,
    ) {
        let tables = &mut *context.tables;
        let name = patch["region"].as_str().unwrap();
        let offset = word(&patch["offset"]) as usize;
        let data = bytes(&patch["bytes"]);
        put(&mut self.regions, name, offset, &data);
        match name {
            "shared" | "registry" | "default" => {
                tables
                    .write_process_bytes(base(name) + offset as u32, &data)
                    .unwrap();
            }
            "vehicle0" | "vehicle1" => {
                let owner = self.vehicles[usize::from(name == "vehicle1")];
                storage::write_row(
                    arena,
                    tables,
                    BehaviorRowSelection::VehicleCopy {
                        owner,
                        byte_offset: 0,
                    },
                    offset as u32,
                    &data,
                )
                .unwrap();
            }
            "process_null" => {
                context.process.process_null_handle = self.regions[name]
                    .iter()
                    .copied()
                    .collect::<Option<Vec<_>>>()
                    .map(|v| ActorHandle::from_raw(u32::from_le_bytes(v.try_into().unwrap())));
            }
            "responses" => {
                arena.actor_mut(self.candidate).unwrap().damage_table =
                    ActorDamageTable::Payload(payload(&self.regions[name]));
            }
            "behavior" => {
                let state = arena
                    .derived_actor_mut(self.owner)
                    .unwrap()
                    .behavior_140
                    .as_mut()
                    .unwrap();
                if offset <= 0x10 && offset + data.len() > 0x10 {
                    state.byte_10 = self.regions[name][0x10].unwrap();
                }
                if offset <= 0x14 && offset + data.len() >= 0x18 {
                    let native = dword(&self.regions, name, 0x14);
                    state.table_14 = [
                        ActorBehaviorTable::Mode01,
                        ActorBehaviorTable::Mode02,
                        ActorBehaviorTable::Mode03,
                        ActorBehaviorTable::Mode04,
                        ActorBehaviorTable::Mode05,
                        ActorBehaviorTable::Mode06,
                        ActorBehaviorTable::Mode07,
                        ActorBehaviorTable::Mode08,
                        ActorBehaviorTable::Mode09,
                        ActorBehaviorTable::Mode10,
                        ActorBehaviorTable::Mode11,
                        ActorBehaviorTable::Mode12,
                        ActorBehaviorTable::Mode13,
                    ]
                    .into_iter()
                    .find(|t| *t as u32 == native)
                    .unwrap_or_else(|| {
                        assert!([0x72b26c, 0x72c26c].contains(&native));
                        ActorBehaviorTable::Selector2Copy
                    });
                }
            }
            "temporary" | "component" | "source0" | "source1" | "candidate" => {}
            _ => panic!("unsupported admitted effect {name}"),
        }
    }

    fn service(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        operation: &str,
        request: Value,
    ) -> Result<Option<String>, String> {
        let expected = &self.expected_services[self.service_index];
        assert_eq!(expected["operation"], operation);
        assert_eq!(
            expected["request"], request,
            "{operation} reached arguments"
        );
        self.service_index += 1;
        let supplied = &self.invocation["services"][self.operations.len()];
        assert_eq!(supplied["operation"], operation);
        let supplied = supplied.clone();
        self.operations.push(operation.into());
        for patch in supplied["effects"].as_array().unwrap() {
            self.apply(arena, context, patch);
        }
        if !supplied["error"].is_null() {
            return Err(format!("host_{operation}"));
        }
        Ok(supplied["result"].as_str().map(str::to_owned))
    }

    fn source(&mut self, operation: &str) -> Result<(), String> {
        self.source_operations.push(operation.into());
        if self.invocation["source_error"]["operation"] == operation {
            Err(format!("source_{operation}"))
        } else {
            Ok(())
        }
    }

    fn snapshot(&self, arena: &ObjectArena, context: &BehaviorPredicateContext<'_>) -> Bytes {
        let tables = &*context.tables;
        let mut regions = self.regions.clone();
        for (name, data) in [
            (
                "shared",
                tables.descriptor_bytes.as_deref().map(|v| v.as_slice()),
            ),
            (
                "registry",
                tables.registry_bytes.as_deref().map(|v| v.as_slice()),
            ),
            (
                "default",
                tables.default_packet_bytes.as_ref().map(|v| v.as_slice()),
            ),
        ] {
            if let Some(data) = data {
                regions.insert(name.into(), data.to_vec());
            }
        }
        for (index, name) in ["vehicle0", "vehicle1"].into_iter().enumerate() {
            let state = arena.selector2_actor(self.vehicles[index]).unwrap();
            if let (Some(data), Some(mask)) =
                (state.table_26c.as_deref(), state.table_26c_known.as_deref())
            {
                regions.insert(
                    name.into(),
                    (0..704)
                        .map(|i| mask[i].then(|| (data[i / 4] >> ((i % 4) * 8)) as u8))
                        .collect(),
                );
            }
        }
        // The response pointer owns the complete retained canonical table.
        let expected: Vec<_> = arena
            .actor(self.candidate)
            .unwrap()
            .damage_responses()
            .into_iter()
            .flat_map(|(a, b)| a.to_le_bytes().into_iter().chain(b.to_le_bytes()).map(Some))
            .collect();
        assert_eq!(regions["responses"], expected);
        let behavior = arena
            .derived_actor(self.owner)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        assert_eq!(regions["behavior"][0x10], Some(behavior.byte_10));
        if behavior.table_14 != ActorBehaviorTable::Selector2Copy {
            assert_eq!(dword(&regions, "behavior", 0x14), behavior.table_14 as u32);
        }
        regions
    }
}

impl BehaviorPredicateHost for Host {
    type Error = String;
    type Particle = u32;
    type Component = u32;
    fn create(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorCreationRequest,
    ) -> Result<Option<BehaviorTemporary<u32>>, String> {
        let (operation, args, particle) = match request {
            BehaviorCreationRequest::ParticleFactory {
                callback,
                context_word,
                null_handle,
                scratch_bytes,
            } => (
                "factory",
                json!({"factory_address":callback,"argument":context_word,"scratch_bytes":scratch_bytes,
                    "same_position_velocity_pointer":true,"process_null_handle":null_handle.raw(),"flags":0}),
                true,
            ),
            BehaviorCreationRequest::ActorResource {
                resource,
                null_handle,
                scratch_bytes,
            } => (
                "actor_create",
                json!({"slot_word":resource as u16,"slot_signed":resource,"scratch_bytes":scratch_bytes,
                    "edx":0,"arguments_after_position":[0,u32::MAX,0,null_handle.raw()]}),
                false,
            ),
        };
        Ok(self
            .service(arena, context, operation, args)?
            .map(|identity| {
                assert_eq!(identity, "created");
                if particle {
                    BehaviorTemporary::Particle(0x726000)
                } else {
                    BehaviorTemporary::Actor(self.temporary)
                }
            }))
    }
    fn query_component(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorComponentQuery<u32>,
    ) -> Result<Option<u32>, String> {
        let (operation, receiver) = match request {
            BehaviorComponentQuery::Actor18(actor) => {
                assert_eq!(actor, self.temporary);
                ("actor18", "created")
            }
            BehaviorComponentQuery::Component6c(value) => {
                assert_eq!(value, 0x727000);
                ("component6c", "component")
            }
            BehaviorComponentQuery::Component68(value) => {
                assert_eq!(value, 0x727000);
                ("component68", "component")
            }
        };
        Ok(self
            .service(
                arena,
                context,
                operation,
                json!({"temporary_identity":"created","receiver_identity":receiver}),
            )?
            .map(|value| {
                assert_eq!(value, "component");
                0x727000
            }))
    }
    fn packet_source(
        &mut self,
        _: &ObjectArena,
        _: &BehaviorPredicateContext<'_>,
        request: BehaviorPacketSourceRequest<u32, u32>,
    ) -> Result<Option<BehaviorPacketCopy>, String> {
        Ok(match request {
            BehaviorPacketSourceRequest::ParticlePointer { particle, slot } => {
                assert_eq!(particle, 0x726000);
                let (operation, offset) = match slot {
                    BehaviorPacketSlot::First => ("particle_first", 0x34),
                    BehaviorPacketSlot::Second => ("particle_second", 0x38),
                };
                self.source(operation)?;
                let pointer = dword(&self.regions, "temporary", offset);
                match pointer {
                    0 => None,
                    0x728000 => Some(BehaviorPacketCopy::Full(
                        self.regions["source0"].as_slice().try_into().unwrap(),
                    )),
                    0x729000 => Some(BehaviorPacketCopy::Full(
                        self.regions["source1"].as_slice().try_into().unwrap(),
                    )),
                    _ => panic!("unadmitted supplied source pointer {pointer:08x}"),
                }
            }
            BehaviorPacketSourceRequest::Channel148(value) => {
                assert_eq!(value, 0x727000);
                self.source("channel148")?;
                Some(BehaviorPacketCopy::Byte(self.regions["component"][0x148]))
            }
            BehaviorPacketSourceRequest::Strength146(value) => {
                assert_eq!(value, 0x727000);
                self.source("strength146")?;
                Some(BehaviorPacketCopy::Word(
                    self.regions["component"][0x146..0x148].try_into().unwrap(),
                ))
            }
            BehaviorPacketSourceRequest::Complete13e(value) => {
                assert_eq!(value, 0x727000);
                self.source("complete13e")?;
                Some(BehaviorPacketCopy::Full(
                    self.regions["component"][0x13e..0x142].try_into().unwrap(),
                ))
            }
        })
    }
    fn delete_particle(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        particle: u32,
        flags: u32,
    ) -> Result<(), String> {
        assert_eq!(particle, 0x726000);
        self.service(
            arena,
            context,
            "particle_delete",
            json!({"temporary_identity":"created","deleting_flags":flags}),
        )?;
        Ok(())
    }
}

fn execute_copy(
    arena: &mut ObjectArena,
    context: &mut BehaviorPredicateContext<'_>,
    row: BehaviorRowSelection,
    host: &mut Host,
    phase: &str,
) -> Result<(), BehaviorPredicateError<String>> {
    let phases = match phase {
        "particle_copy" => vec![
            (
                BehaviorPacketSourceRequest::ParticlePointer {
                    particle: 0x726000,
                    slot: BehaviorPacketSlot::First,
                },
                packets::PacketCopyPhase::Particle466120(BehaviorPacketSlot::First),
                0x466120,
            ),
            (
                BehaviorPacketSourceRequest::ParticlePointer {
                    particle: 0x726000,
                    slot: BehaviorPacketSlot::Second,
                },
                packets::PacketCopyPhase::Particle466120(BehaviorPacketSlot::Second),
                0x466120,
            ),
        ],
        "component6c_copy" => vec![
            (
                BehaviorPacketSourceRequest::Channel148(0x727000),
                packets::PacketCopyPhase::Channel4863da,
                0x4863d4,
            ),
            (
                BehaviorPacketSourceRequest::Strength146(0x727000),
                packets::PacketCopyPhase::Strength4863e4,
                0x4863dd,
            ),
        ],
        "component68_copy" => vec![(
            BehaviorPacketSourceRequest::Complete13e(0x727000),
            packets::PacketCopyPhase::Complete458f50,
            0x458f50,
        )],
        _ => panic!("not a standalone copy"),
    };
    for (request, phase, address) in phases {
        if let Some(source) = host
            .packet_source(arena, context, request)
            .map_err(|error| BehaviorPredicateError::Host { address, error })?
        {
            packets::copy(arena, context.tables, row, phase, source)?;
        }
    }
    Ok(())
}

fn compare_result(
    result: &Result<Option<bool>, BehaviorPredicateError<String>>,
    expected: &Value,
    name: &str,
) {
    let Some(stopped) = expected["stopped"].as_str() else {
        let actual = result.as_ref().unwrap();
        let value = actual.map(|v| u32::from(v));
        assert_eq!(json!(value), expected["result"], "{name} return");
        return;
    };
    let error = result.as_ref().unwrap_err();
    if stopped.starts_with("host_") || stopped.starts_with("source_") {
        let BehaviorPredicateError::Host {
            error: actual,
            address,
        } = error
        else {
            panic!("{name}: {error:?}")
        };
        assert_eq!(actual, stopped, "{name}");
        let expected_address = match stopped {
            "host_factory" => 0x48d891,
            "host_actor_create" => 0x48d8d3,
            "host_actor18" => 0x48d8e6,
            "host_component6c" => 0x48d8f3,
            "host_component68" => 0x48d90f,
            "host_particle_delete" => 0x48d8b0,
            "source_particle_first" | "source_particle_second" => 0x466120,
            "source_channel148" => 0x4863d4,
            "source_strength146" => 0x4863dd,
            "source_complete13e" => 0x458f50,
            _ => panic!("unadmitted service stop {stopped}"),
        };
        assert_eq!(*address, expected_address, "{name}");
        return;
    }
    if stopped == "unknown_process_null_0" {
        assert!(
            matches!(
                error,
                BehaviorPredicateError::MissingProcessNull {
                    address: 0x48d87c | 0x48d8bb
                }
            ),
            "{name}: {error:?}"
        );
        return;
    }
    let read = expected["events"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|event| event["kind"] == "read")
        .unwrap();
    let region = read["region"].as_str().unwrap();
    let offset = word(&read["offset"]);
    let length = word(&read["bytes"]);
    match error {
        BehaviorPredicateError::Storage(BehaviorCacheStorageError::Unknown { address }) => {
            assert!(stopped.starts_with("unknown_"));
            assert!(
                *address >= base(region) + offset && *address < base(region) + offset + length,
                "{name}: {error:?} {read}"
            );
        }
        BehaviorPredicateError::Storage(BehaviorCacheStorageError::Unavailable { address }) => {
            assert_eq!(stopped, format!("storage_{region}"));
            assert_eq!(*address, base(region) + offset);
        }
        BehaviorPredicateError::Storage(BehaviorCacheStorageError::MissingVehicleCopy {
            ..
        }) => {
            assert_eq!(stopped, "storage_vehicle0");
        }
        BehaviorPredicateError::Storage(BehaviorCacheStorageError::UnknownVehicle {
            offset: actual,
            ..
        }) => {
            assert_eq!(stopped, format!("unknown_{region}_{offset:x}"));
            assert!(*actual >= offset as i32 && *actual < (offset + length) as i32);
        }
        _ => panic!("{name}: unexpected {error:?} for {stopped}"),
    }
}

#[test]
fn shared_cache_and_packet_phases_match_every_admitted_native_history() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/behavior_predicate_cache_native.json"
    ))
    .unwrap();
    assert_eq!(
        data["metadata"]["exe_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(data["metadata"]["cases"], 324);
    assert_eq!(data["metadata"]["portable_cases"], 299);
    assert_eq!(data["metadata"]["diagnostic_cases"], 25);
    let mut compared = 0;
    let mut calls = 0;
    let mut standalone = 0;
    for case in data["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        if !input["portable_compare"].as_bool().unwrap() {
            assert!(!input["source_only_reason"].is_null());
            continue;
        }
        assert_eq!(case["instruction_bytes_unchanged"], true);
        let baseline = baseline(&data["metadata"], input);
        let (mut arena, candidate, vehicles) = super::tests::vehicle_fixture();
        let stale = arena.allocate(ObjectClassKind::Base);
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: stale,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        let temporary = arena.allocate(ObjectClassKind::Base);
        arena.set_flag0(temporary, true).unwrap();
        arena.set_flag1(temporary, true).unwrap();
        let mut tables = super::tests::tables();
        let mut process = super::tests::process();
        process.process_null_handle = Some(ActorHandle::from_raw(0x08010000));
        let mut rng = 0;
        let mut context = BehaviorPredicateContext {
            tables: &mut tables,
            process: &mut process,
            rng: &mut rng,
        };
        let mut host = Host {
            regions: baseline.clone(),
            vehicles,
            candidate,
            owner: candidate,
            temporary,
            invocation: Value::Null,
            expected_services: vec![],
            service_index: 0,
            operations: vec![],
            source_operations: vec![],
        };
        arena.actor_mut(candidate).unwrap().damage_table =
            ActorDamageTable::Payload(payload(&baseline["responses"]));
        for patch in input["patches"].as_array().unwrap() {
            host.apply(&mut arena, &mut context, patch);
        }
        for unavailable in input["unavailable_regions"].as_array().unwrap() {
            match unavailable.as_str().unwrap() {
                "shared" => context.tables.descriptor_bytes = None,
                "registry" => context.tables.registry_bytes = None,
                "default" => context.tables.default_packet_bytes = None,
                "vehicle0" | "vehicle1" => {
                    let index = usize::from(unavailable == "vehicle1");
                    let state = arena.selector2_actor_mut(vehicles[index]).unwrap();
                    state.table_26c = None;
                    state.table_26c_known = None;
                }
                _ => panic!("unadmitted unavailable region"),
            }
        }
        for (invocation, expected) in input["calls"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["calls"].as_array().unwrap())
        {
            let table_name = invocation["selection"]["table"].as_str().unwrap();
            let index = word(&invocation["selection"]["index_byte"]) as u8;
            let native_table = if table_name.starts_with("vehicle") {
                base(table_name)
            } else {
                table(table_name) as u32
            };
            let row = BehaviorRowSelection::from_table(
                table(table_name),
                index,
                vehicles[usize::from(table_name == "vehicle1")],
            );
            assert_eq!(
                word(&expected["selected_address"]),
                native_table.wrapping_add(i32::from(index as i8).wrapping_mul(44) as u32),
                "{name} captured row"
            );
            host.apply(
                &mut arena,
                &mut context,
                &json!({"region":"behavior","offset":0x10,"bytes":[index]}),
            );
            host.apply(
                &mut arena,
                &mut context,
                &json!({"region":"behavior","offset":0x14,"bytes":native_table.to_le_bytes()}),
            );
            host.invocation = invocation.clone();
            host.service_index = 0;
            host.operations.clear();
            host.source_operations.clear();
            host.expected_services = expected["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "supplied_service")
                .cloned()
                .collect();
            let result = if invocation["phase"] == "predicate" {
                let candidate = match invocation["candidate"].as_str().unwrap() {
                    "candidate" => Some(candidate),
                    "unavailable" => Some(stale),
                    "null" => None,
                    _ => panic!("candidate"),
                };
                arena
                    .test_behavior_damage_candidate(
                        BehaviorPredicateRequest {
                            selection: row,
                            candidate,
                        },
                        &mut context,
                        &mut host,
                    )
                    .map(Some)
            } else {
                standalone += 1;
                execute_copy(
                    &mut arena,
                    &mut context,
                    row,
                    &mut host,
                    invocation["phase"].as_str().unwrap(),
                )
                .map(|()| None)
            };
            compare_result(&result, expected, name);
            // 479FA0 is the actual arena owner. The native oracle intercepts
            // it only to report this valid callback-free cleanup edge.
            if host
                .expected_services
                .get(host.service_index)
                .is_some_and(|e| e["operation"] == "actor_dispose")
            {
                let event = &host.expected_services[host.service_index];
                assert!(event["error"].is_null(), "disposal host errors excluded");
                let plan = &invocation["services"][host.operations.len()];
                assert!(
                    plan["effects"].as_array().unwrap().is_empty(),
                    "arbitrary disposal mutations excluded"
                );
                assert!(
                    arena.object(temporary).is_ok(),
                    "disposal retains allocation"
                );
                assert!(arena
                    .list(ObjectList::Disposal)
                    .iter()
                    .any(|v| v.object == temporary));
                assert_eq!(arena.object(temporary).unwrap().base.flags & 0x164, 0);
                host.service_index += 1;
                host.operations.push("actor_dispose".into());
            }
            assert_eq!(
                host.service_index,
                host.expected_services.len(),
                "{name} reached service cadence"
            );
            let expected_operations: Vec<_> = host
                .expected_services
                .iter()
                .map(|e| e["operation"].as_str().unwrap())
                .collect();
            assert_eq!(host.operations, expected_operations, "{name} service order");
            let mut wanted = baseline.clone();
            for patch in expected["storage_patches"].as_array().unwrap() {
                put(
                    &mut wanted,
                    patch["region"].as_str().unwrap(),
                    word(&patch["offset"]) as usize,
                    &bytes(&patch["bytes"]),
                );
            }
            assert_eq!(
                host.snapshot(&arena, &context),
                wanted,
                "{name} all available storage bytes/masks"
            );
            if !expected["scratch_bytes"].is_null() {
                assert_eq!(
                    expected["scratch_bytes"],
                    serde_json::to_value([None::<u8>; 8]).unwrap()
                );
            }
            calls += 1;
        }
        compared += 1;
    }
    assert_eq!((compared, calls, standalone), (299, 306, 3));
}
