//! Original-instruction supplied-state receipts. Physical list/self pointers
//! are compared through semantic custody; immutable EXE backing cannot carry
//! the native checker's unknown parameter/packet byte masks.

use serde_json::Value;

use super::*;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}
fn byte_values(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().map(|v| v as u8))
        .collect()
}
fn known_dword(bytes: &[Option<u8>]) -> Option<u32> {
    let values = bytes.iter().copied().collect::<Option<Vec<_>>>()?;
    Some(u32::from_le_bytes(values.try_into().unwrap()))
}
fn vector(value: &Value) -> ParticleVectorBytes {
    ParticleVectorBytes(byte_values(value).try_into().unwrap())
}
fn kind(address: u32) -> ParticleFactoryKind {
    match address {
        0x466710 => ParticleFactoryKind::Factory466710,
        0x466850 => ParticleFactoryKind::Factory466850,
        0x4665d0 => ParticleFactoryKind::Factory4665D0,
        _ => panic!("unowned factory {address:x}"),
    }
}
fn identity(id: Option<ParticleId>) -> Value {
    match id {
        Some(ParticleId::Pool(slot)) => Value::from(slot),
        Some(ParticleId::Scratch) => Value::from("scratch"),
        None => Value::Null,
    }
}
fn slots(value: &Value) -> Vec<u16> {
    let mut result = Vec::new();
    for run in value.as_array().unwrap() {
        let a = run.as_array().unwrap();
        let mut current = a[0].as_i64().unwrap();
        let last = a[1].as_i64().unwrap();
        let stride = a[2].as_i64().unwrap();
        loop {
            result.push(current as u16);
            if current == last {
                break;
            }
            current += stride;
        }
    }
    result
}

fn patch(
    value: &Value,
    last: Option<ParticleId>,
    pool: &mut ProcessParticleRuntime,
    state: &mut ParticleFactoryProcessState,
    phase: &crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    mut sources: Option<&mut ParticleFactorySources>,
    rng: &mut u32,
    output: &mut [Option<u8>; 8],
) {
    let region = value["region"].as_str().unwrap();
    let offset = number(&value["offset"]) as usize;
    let data = byte_values(&value["bytes"]);
    if region == "last" || region.starts_with("pool:") {
        let id = if region == "last" {
            last.unwrap()
        } else {
            ParticleId::Pool(region[5..].parse().unwrap())
        };
        assert!(
            offset + data.len() <= 0x18 || offset >= 0x24,
            "physical pointer mutation excluded"
        );
        pool.record_mut(id)
            .unwrap()
            .copy_bytes(offset, &data)
            .unwrap();
        return;
    }
    match region {
        "position" => sources
            .as_mut()
            .expect("creation source unavailable in impact receipts")
            .position
            .0[offset..offset + data.len()]
            .copy_from_slice(&data),
        "velocity" => sources
            .as_mut()
            .expect("creation source unavailable in impact receipts")
            .separate_velocity
            .as_mut()
            .unwrap()
            .0[offset..offset + data.len()]
            .copy_from_slice(&data),
        "output" => output[offset..offset + data.len()].copy_from_slice(&data),
        "process_null" => {
            assert_eq!((offset, data.len()), (0, 4));
            state.process_null_handle = known_dword(&data).map(ActorHandle::from_raw);
        }
        "rng" => {
            assert_eq!((offset, data.len()), (0, 4));
            *rng = known_dword(&data).unwrap();
        }
        "network" => {
            assert_eq!((offset, data.len()), (0, 4));
            // Unknown unused inputs on invalid-selector receipts do not
            // become constructor defaults or a claimed native storage value.
            state.network_flag = known_dword(&data).unwrap_or(0);
        }
        "phase" => {
            assert_eq!((offset, data.len()), (0, 4));
            phase.set(known_dword(&data));
        }
        "manager_pointer" => {
            assert!(data.iter().all(Option::is_none));
        }
        _ => panic!("unadmitted mutable region {region}"),
    }
}

struct Host {
    phase: crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    step: Value,
    expected: Vec<Value>,
    index: usize,
    output: [Option<u8>; 8],
}

impl ParticleFactoryHost for Host {
    type Error = String;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase.current())
    }
    fn network_notice(
        &mut self,
        _: ParticleCreationNotice,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        _: &mut ParticleFactorySources,
    ) -> Result<(), String> {
        panic!("network encoder receipts remain diagnostics")
    }
    fn impact_489600(
        &mut self,
        request: ParticleFactoryImpactRequest,
        pool: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), String> {
        let event = &self.expected[self.index];
        assert_eq!(event["operation"], "impact_489600");
        assert_eq!(number(&event["request"]["selector"]), request.selector);
        assert_eq!(number(&event["request"]["flags"]), request.flags);
        assert_eq!(
            byte_values(&event["request"]["position"]),
            pool.record(request.particle)
                .unwrap()
                .source_bytes::<6>(4)
                .unwrap()
        );
        assert_eq!(
            byte_values(&event["request"]["velocity"]),
            pool.record(request.particle)
                .unwrap()
                .source_bytes::<6>(0x0a)
                .unwrap()
        );
        let service = self.step["impact_services"][self.index].clone();
        assert_eq!(service["error"], event["error"]);
        self.index += 1;
        for effect in service["effects"].as_array().unwrap() {
            patch(
                effect,
                Some(request.particle),
                pool,
                context.process,
                &self.phase,
                None,
                context.rng,
                &mut self.output,
            );
        }
        match service["error"].as_str() {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

fn compare_state(
    pool: &ProcessParticleRuntime,
    process: &ParticleFactoryProcessState,
    rng: u32,
    output: &[Option<u8>; 8],
    expected: &Value,
    name: &str,
) {
    for item in expected["records"].as_array().unwrap() {
        let id = ParticleId::Pool(number(&item["slot"]) as u16);
        compare_payload(pool.record(id).unwrap(), &item["payload"], name);
    }
    compare_payload(
        pool.record(ParticleId::Scratch).unwrap(),
        &expected["scratch"],
        name,
    );
    assert_eq!(
        output.as_slice(),
        byte_values(&expected["output"]),
        "{name} reached packet source results"
    );
    assert_eq!(
        pool.free.iter().rev().copied().collect::<Vec<_>>(),
        slots(&expected["free_order"]),
        "{name} free order"
    );
    assert!(
        expected["free_boundary"].is_null(),
        "raw free-list boundaries excluded"
    );
    assert_eq!(
        pool.allocation_marker.map(u32::from),
        expected["allocation_marker"].as_u64().map(|v| v as u32),
        "{name} publication marker"
    );
    let wanted: Vec<_> = expected["occupants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| (number(&pair[0]) as usize, pair[1].clone()))
        .collect();
    let actual: Vec<_> = pool
        .occupants
        .iter()
        .enumerate()
        .filter_map(|(slot, id)| id.map(|id| (slot, identity(Some(id)))))
        .collect();
    assert_eq!(actual, wanted, "{name} physical occupants");
    for category in 0..6 {
        for (key, actual) in [
            ("pending", &pool.pending[category]),
            ("active", &pool.active[category]),
        ] {
            let actual: Vec<_> = actual
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    _ => panic!("scratch has no node"),
                })
                .collect();
            assert_eq!(
                actual,
                slots(&expected[key][category]),
                "{name} {key}/{category}"
            );
        }
    }
    assert_eq!(
        known_dword(&byte_values(&expected["rng"])),
        Some(rng),
        "{name} process RNG"
    );
    let null = known_dword(&byte_values(&expected["process_null"])).map(ActorHandle::from_raw);
    assert_eq!(
        process.process_null_handle, null,
        "{name} independent process null"
    );
}

fn compare_payload(record: &super::super::ParticleRecord, expected: &Value, name: &str) {
    let expected = byte_values(expected);
    for (offset, wanted) in expected.into_iter().enumerate() {
        if (0x18..0x24).contains(&offset) {
            continue;
        }
        let actual = record.initialized[offset].then_some(record.bytes[offset]);
        assert_eq!(actual, wanted, "{name} payload+{offset:02x}");
    }
}

fn admitted(input: &Value) -> bool {
    if input["evidence"] == "diagnostic" {
        return false;
    }
    // The native harness admits unknown EXE bytes as supplied copy data.
    // Immutable FrontendExecutable has no matching byte-mask storage.
    !input["patches"].as_array().unwrap().iter().any(|p| {
        p["region"] == "factory_data" && byte_values(&p["bytes"]).iter().any(Option::is_none)
    })
}

#[test]
fn shared_factories_and_deletion_match_admitted_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/particle_factories_native.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(fixture["fpcw"], 0x027f);
    assert_eq!(fixture["text_address"], 0x401000);
    assert_eq!(
        fixture["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 154);
    let raw = super::tests::raw_executable();
    let mut compared = 0;
    let mut steps = 0;
    let mut excluded_unknown_backing = 0;
    let mut diagnostics = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        if !admitted(input) {
            if input["evidence"] != "diagnostic" {
                excluded_unknown_backing += 1;
            } else {
                diagnostics += 1;
            }
            continue;
        }
        let name = input["name"].as_str().unwrap();
        let mut supplied_raw = raw.clone();
        for patch in input["patches"].as_array().unwrap() {
            if patch["region"] == "factory_data" {
                let bytes = byte_values(&patch["bytes"])
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .expect("admitted immutable EXE input is completely known");
                let offset = FrontendExecutable::parse(&supplied_raw)
                    .unwrap()
                    .mapped(0x539d20 + number(&patch["offset"]), bytes.len())
                    .unwrap()
                    .file_span
                    .start;
                supplied_raw[offset..offset + bytes.len()].copy_from_slice(&bytes);
            }
        }
        let mut pool = ProcessParticleRuntime::default();
        let mut process = ParticleFactoryProcessState {
            network_flag: u32::from(input["network_active"].as_bool().unwrap()),
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(number(&input["process_null"]))),
        };
        let mut sources = ParticleFactorySources {
            position: vector(&input["position"]),
            separate_velocity: Some(vector(&input["velocity"])),
        };
        let mut rng = number(&input["rng"]);
        let mut host = Host {
            phase:
                crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(
                    Some(number(&input["phase"])),
                ),
            step: Value::Null,
            expected: vec![],
            index: 0,
            output: byte_values(&fixture["initial_native_support"]["output"])
                .try_into()
                .unwrap(),
        };
        let mut context = ParticleFactoryContext {
            process: &mut process,
            rng: &mut rng,
            executable: FrontendExecutable::parse(&supplied_raw).unwrap(),
        };
        for index in 0..number(&input["prepare_count"]) {
            if index == 0 && !input["prepare_first_base"].is_null() {
                let base = &input["prepare_first_base"];
                let id = pool
                    .allocate(ParticleAllocationRequest {
                        payload_bytes: number(&base["payload_bytes"]),
                        last_reclaim_category: number(&base["last_reclaim_category"]) as i32,
                        network_active: context.process.network_flag != 0,
                        phase: host.phase.snapshot(),
                    })
                    .unwrap();
                pool.construct(
                    id,
                    ParticleConstruction {
                        category: number(&base["category"]) as usize,
                        position: sources.position,
                        velocity: sources.separate_velocity,
                        source_handle: number(&base["source_handle"]),
                        flags: number(&base["flags"]),
                    },
                )
                .unwrap();
            } else {
                let prepare = &input["prepare_factory"];
                pool.create_factory_particle(
                    ParticleFactoryRequest {
                        kind: kind(number(&prepare["address"])),
                        selector: number(&prepare["selector"]),
                        flags: number(&prepare["flags"]),
                        source_handle: ActorHandle::from_raw(number(&prepare["source_handle"])),
                        velocity_source: ParticleVelocitySource::Separate,
                    },
                    &mut context,
                    &mut sources,
                    &mut host,
                )
                .unwrap();
            }
        }
        if input["prepare_transfer"].as_bool().unwrap() {
            pool.transfer_pending();
        }
        for change in input["patches"].as_array().unwrap() {
            if change["region"] == "factory_data" {
                // Initial complete bytes are already in the supplied immutable
                // image. Generic step/host patching rejects this region.
                continue;
            }
            patch(
                change,
                None,
                &mut pool,
                context.process,
                &host.phase,
                Some(&mut sources),
                context.rng,
                &mut host.output,
            );
        }
        let mut last = None;
        for step in case["steps"].as_array().unwrap() {
            let invocation = &step["input"];
            for change in invocation["patches"].as_array().unwrap() {
                patch(
                    change,
                    last,
                    &mut pool,
                    context.process,
                    &host.phase,
                    Some(&mut sources),
                    context.rng,
                    &mut host.output,
                );
            }
            host.step = invocation.clone();
            host.index = 0;
            host.expected = step["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "supplied_service")
                .cloned()
                .collect();
            let (result, error) = match invocation["operation"].as_str().unwrap() {
                "factory" => {
                    let mode = if invocation["null_velocity"].as_bool().unwrap() {
                        ParticleVelocitySource::Null
                    } else if invocation["same_pointer"].as_bool().unwrap() {
                        ParticleVelocitySource::SamePosition
                    } else {
                        ParticleVelocitySource::Separate
                    };
                    let result = pool.create_factory_particle(
                        ParticleFactoryRequest {
                            kind: kind(number(&invocation["address"])),
                            selector: number(&invocation["selector"]),
                            flags: number(&invocation["flags"]),
                            source_handle: ActorHandle::from_raw(number(
                                &invocation["source_handle"],
                            )),
                            velocity_source: mode,
                        },
                        &mut context,
                        &mut sources,
                        &mut host,
                    );
                    match result {
                        Ok(id) => {
                            last = id;
                            (identity(id), None)
                        }
                        Err(error) => (Value::Null, Some(error)),
                    }
                }
                "delete" => {
                    let id = last.unwrap();
                    let result = pool.delete_factory_particle(
                        ParticleDeleteRequest {
                            particle: id,
                            flags: number(&invocation["flags"]),
                        },
                        &mut context,
                        &mut host,
                    );
                    (identity(Some(id)), result.err())
                }
                "copy" => {
                    // Compare the narrow source resolver. Production packet
                    // transfers remain owned by behavior::damage_cache.
                    for (index, slot) in [BehaviorPacketSlot::First, BehaviorPacketSlot::Second]
                        .into_iter()
                        .enumerate()
                    {
                        if let Some(bytes) = pool
                            .factory_packet_source(last.unwrap(), slot, context.executable)
                            .unwrap()
                        {
                            host.output[index * 4..index * 4 + 4].copy_from_slice(&bytes);
                        }
                    }
                    (Value::Null, None)
                }
                "transfer" => {
                    pool.transfer_pending();
                    (Value::Null, None)
                }
                operation => panic!("unadmitted operation {operation}"),
            };
            assert_eq!(result, step["result"], "{name} result");
            match step["stopped"].as_str() {
                None => assert!(error.is_none(), "{name}: {error:?}"),
                Some("impact_service_unavailable") => assert!(
                    matches!(error,Some(ParticleFactoryError::Impact(ref v)) if v=="unavailable"),
                    "{name}: {error:?}"
                ),
                stopped => panic!("unadmitted stopped state {stopped:?}"),
            }
            assert_eq!(
                host.index,
                host.expected.len(),
                "{name} required service order/cadence"
            );
            compare_state(
                &pool,
                context.process,
                *context.rng,
                &host.output,
                &step["state"],
                name,
            );
            steps += 1;
        }
        compared += 1;
    }
    assert_eq!(excluded_unknown_backing, 2);
    assert_eq!(diagnostics, 39);
    assert_eq!((compared, steps), (113, 202));
}
