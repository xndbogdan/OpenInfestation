//! Composed original-byte receipts: real 0048D820, factories, packet copy and
//! normal deletion. Network/impact services are explicit mutable inputs;
//! actor/component bodies remain required. Scratch cleanup compares the
//! preserved kernel prefix and stops at its unowned physical node boundary.
//! Logical registration beneath the pool module keeps private list/free
//! custody available without adding production inspection APIs.

use std::sync::Arc;

use inf_formats::{
    parse_constructor_asset_bank, ActorConstructorPayload, ByteSpan, ConstructorPayloadView,
    HandlerFraming, HandlerId, HandlerRegion, RelocationTail,
};
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{
    ActorBehaviorState, ActorBehaviorTable, ActorConstructionRequest, ActorRegistryConfig,
    ActorTerrainInputs, BehaviorActorResourceRequest, BehaviorComponentPacketSourceRequest,
    BehaviorComponentQuery, BehaviorPacketCopy, BehaviorParticleAdapter,
    BehaviorParticleAdapterError, BehaviorParticleCreationSources, BehaviorParticleEffectRequest,
    BehaviorParticleServiceContext, BehaviorParticleServices, BehaviorPredicateContext,
    BehaviorPredicateError, BehaviorPredicateRequest, BehaviorRowSelection,
    DerivedActorConstructionRequest, ObjectArena, ObjectId, ProcessActorBehaviorTables,
};

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}
fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().map(|v| v.try_into().unwrap()))
        .collect()
}
fn dword(input: &[Option<u8>]) -> Option<u32> {
    Some(u32::from_le_bytes(
        input
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}
fn table(address: u32) -> ActorBehaviorTable {
    [
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
    .find(|t| *t as u32 == address)
    .expect("admitted shared table identity")
}
fn slots(value: &Value) -> Vec<u16> {
    let mut result = Vec::new();
    for run in value.as_array().unwrap() {
        let mut current = run[0].as_i64().unwrap();
        let last = run[1].as_i64().unwrap();
        let stride = run[2].as_i64().unwrap();
        loop {
            result.push(current.try_into().unwrap());
            if current == last {
                break;
            }
            assert_ne!(stride, 0);
            current += stride;
        }
    }
    result
}

fn payload(responses: &[Option<u8>]) -> Arc<ActorConstructorPayload> {
    // Canonical H13 shared payload supplies the candidate's actual ten pairs.
    // No constructor class or raw candidate pointer is inferred from this.
    let mut source = vec![0xa5; 9];
    source.extend_from_slice(&55_u16.to_le_bytes());
    source.extend_from_slice(&u16::MAX.to_le_bytes());
    let mut data = [0; 0x42];
    for (offset, value) in responses.iter().enumerate() {
        data[0x10 + offset] = value.expect("admitted known response backing");
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

fn candidate(null: ActorHandle) -> (ObjectArena, ObjectId) {
    const SINE: [u16; 4096] = [0; 4096];
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: null,
    })
    .unwrap();
    let id = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([10, 20, 30]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [0, 0],
            },
            &mut 0,
        )
        .unwrap();
    // Only +10/+14 are operands of this predicate. The supplied native
    // behavior backing has zeroed other fields; this is not construction RE.
    arena.set_behavior_particle_receipt_behavior(
        id,
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
        },
    );
    (arena, id)
}

struct Services {
    phase: crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    candidate: ObjectId,
    behavior: Vec<Option<u8>>,
    responses: Vec<Option<u8>>,
    invocation: Value,
    expected: Vec<Value>,
    event_index: usize,
    network_index: usize,
    impact_index: usize,
    scratch: Option<[Option<u8>; 8]>,
}

impl Services {
    fn apply(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        pool: &mut ProcessParticleRuntime,
        sources: Option<&mut BehaviorParticleCreationSources<'_>>,
        patch: &Value,
    ) {
        let region = patch["region"].as_str().unwrap();
        let offset = number(&patch["offset"]) as usize;
        let data = bytes(&patch["bytes"]);
        let end = offset + data.len();
        if let Some(slot) = region.strip_prefix("pool:") {
            assert!(end <= 0x18 || offset >= 0x24, "raw node pointers excluded");
            pool.record_mut(ParticleId::Pool(slot.parse().unwrap()))
                .unwrap()
                .copy_bytes(offset, &data)
                .unwrap();
            return;
        }
        match region {
            "shared" | "registry" | "default" => {
                let base = match region {
                    "shared" => 0x530e60,
                    "registry" => 0x53dba8,
                    _ => 0x546a94,
                };
                context
                    .tables
                    .write_process_bytes(base + offset as u32, &data)
                    .unwrap();
            }
            "behavior" => {
                self.behavior[offset..end].copy_from_slice(&data);
                let mut state = arena
                    .derived_actor(self.candidate)
                    .unwrap()
                    .behavior_140
                    .as_ref()
                    .unwrap()
                    .clone();
                if offset <= 0x10 && end > 0x10 {
                    state.byte_10 = self.behavior[0x10].unwrap();
                }
                if offset <= 0x14 && end >= 0x18 {
                    state.table_14 = table(dword(&self.behavior[0x14..0x18]).unwrap());
                }
                arena.set_behavior_particle_receipt_behavior(self.candidate, state);
            }
            "responses" => {
                self.responses[offset..end].copy_from_slice(&data);
                arena.set_behavior_particle_receipt_responses(
                    self.candidate,
                    payload(&self.responses),
                );
            }
            "cache_scratch" => {
                let sources = sources.expect("creation source exists only at network boundary");
                for (index, byte) in data.into_iter().enumerate() {
                    let index = offset + index;
                    if index < 6 {
                        sources.vectors.position.0[index] = byte;
                    } else {
                        sources.scratch_tail[index - 6] = byte;
                    }
                }
                self.scratch = Some(std::array::from_fn(|index| {
                    if index < 6 {
                        sources.vectors.position.0[index]
                    } else {
                        sources.scratch_tail[index - 6]
                    }
                }));
            }
            "process_null" => {
                assert_eq!((offset, end), (0, 4));
                context.process.process_null_handle = dword(&data).map(ActorHandle::from_raw);
            }
            "phase" => {
                assert_eq!((offset, end), (0, 4));
                self.phase.set(dword(&data));
            }
            "network" => {
                assert_eq!((offset, end), (0, 4));
                context.process.network_flag = dword(&data).unwrap();
            }
            "rng" => {
                assert_eq!((offset, end), (0, 4));
                *context.rng = dword(&data).unwrap();
            }
            _ => panic!("unadmitted supplied region {region}"),
        }
    }

    fn event(&mut self, operation: &str) -> Value {
        let expected = self.expected[self.event_index].clone();
        assert_eq!(expected["operation"], operation, "required service order");
        self.event_index += 1;
        expected
    }
}

impl BehaviorParticleServices for Services {
    type Error = String;
    type Component = u32;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase.current())
    }

    fn create_actor_resource(
        &mut self,
        _: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorActorResourceRequest,
    ) -> Result<Option<ObjectId>, String> {
        let expected = self.event("actor_resource");
        assert_eq!(expected["kind"], "required_boundary");
        assert_eq!(
            number(&expected["resource"]),
            u32::from(request.resource as u16)
        );
        assert_eq!(
            Some(request.null_handle),
            context.process.process_null_handle
        );
        assert_eq!(request.scratch_bytes, [None; 8]);
        Err("required_actor_resource".into())
    }
    fn query_component(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        _: BehaviorComponentQuery<u32>,
    ) -> Result<Option<u32>, String> {
        panic!("no successful actor/component body supplied")
    }
    fn component_packet_source(
        &mut self,
        _: &ObjectArena,
        _: &BehaviorPredicateContext<'_>,
        _: BehaviorComponentPacketSourceRequest<u32>,
    ) -> Result<Option<BehaviorPacketCopy>, String> {
        panic!("no component packet backing supplied")
    }
    fn particle_effect(
        &mut self,
        context: BehaviorParticleServiceContext<'_, '_, '_>,
        request: BehaviorParticleEffectRequest<'_>,
    ) -> Result<(), String> {
        let mut live = BehaviorPredicateContext {
            tables: context.tables,
            process: context.factory.process,
            rng: context.factory.rng,
        };
        let (expected, supplied, mut sources) = match request {
            BehaviorParticleEffectRequest::NetworkNotice { notice, sources } => {
                let expected = self.event("network_notice");
                let scratch: [Option<u8>; 8] = std::array::from_fn(|index| {
                    if index < 6 {
                        sources.vectors.position.0[index]
                    } else {
                        sources.scratch_tail[index - 6]
                    }
                });
                assert_eq!(bytes(&expected["request"]["scratch_bytes"]), scratch);
                assert_eq!(number(&expected["request"]["event"]), notice.event);
                assert_eq!(number(&expected["request"]["code"]), notice.code);
                assert_eq!(
                    expected["request"]["source_handle"]
                        .as_u64()
                        .map(|v| ActorHandle::from_raw(v as u32)),
                    notice.source_handle
                );
                assert_eq!(notice.velocity_source, ParticleVelocitySource::SamePosition);
                assert!(sources.vectors.separate_velocity.is_none());
                self.scratch = Some(scratch);
                if expected["kind"] == "required_boundary" {
                    return Err("required_network_notice".into());
                }
                let supplied = self.invocation["network_services"][self.network_index].clone();
                self.network_index += 1;
                (expected, supplied, Some(sources))
            }
            BehaviorParticleEffectRequest::Impact489600(request) => {
                let expected = self.event("impact_489600");
                assert_eq!(number(&expected["request"]["selector"]), request.selector);
                assert_eq!(number(&expected["request"]["flags"]), request.flags);
                let record = context.particles.record(request.particle).unwrap();
                assert_eq!(
                    bytes(&expected["request"]["position"]),
                    record.source_bytes::<6>(4).unwrap()
                );
                assert_eq!(
                    bytes(&expected["request"]["velocity"]),
                    record.source_bytes::<6>(0x0a).unwrap()
                );
                let supplied = self.invocation["impact_services"][self.impact_index].clone();
                self.impact_index += 1;
                (expected, supplied, None)
            }
        };
        assert_eq!(supplied["error"], expected["error"]);
        for patch in supplied["effects"].as_array().unwrap() {
            self.apply(
                context.arena,
                &mut live,
                context.particles,
                sources.as_mut(),
                patch,
            );
        }
        supplied["error"].as_str().map_or(Ok(()), |v| Err(v.into()))
    }
}

struct Preparation<'a> {
    phase: &'a crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
}
impl ParticleFactoryHost for Preparation<'_> {
    type Error = std::convert::Infallible;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase.current())
    }
    fn network_notice(
        &mut self,
        _: ParticleCreationNotice,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        _: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        panic!("preparation flags bypass notifications")
    }
    fn impact_489600(
        &mut self,
        _: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        panic!("preparation never reclaims")
    }
}

fn compare_payload(record: &super::super::ParticleRecord, wanted: &Value, name: &str) {
    let wanted = bytes(wanted);
    assert_eq!(wanted.len(), 64);
    for (offset, wanted) in wanted.into_iter().enumerate() {
        // These contain physical list/self addresses. All list, occupant,
        // marker and free custody is compared separately below.
        if (0x18..0x24).contains(&offset) {
            continue;
        }
        assert_eq!(
            record.initialized[offset].then_some(record.bytes[offset]),
            wanted,
            "{name} payload+{offset:02x}"
        );
    }
}

fn compare_state(
    pool: &ProcessParticleRuntime,
    arena: &ObjectArena,
    live: &BehaviorPredicateContext<'_>,
    host: &Services,
    wanted: &Value,
    name: &str,
) {
    for record in wanted["records"].as_array().unwrap() {
        compare_payload(
            pool.record(ParticleId::Pool(number(&record["slot"]) as u16))
                .unwrap(),
            &record["payload"],
            name,
        );
    }
    compare_payload(
        pool.record(ParticleId::Scratch).unwrap(),
        &wanted["scratch"],
        name,
    );
    assert_eq!(
        pool.free.iter().rev().copied().collect::<Vec<_>>(),
        slots(&wanted["free_order"]),
        "{name} free order"
    );
    assert!(
        wanted["free_boundary"].is_null(),
        "raw free boundary excluded"
    );
    assert_eq!(
        pool.allocation_marker.map(u32::from),
        wanted["allocation_marker"].as_u64().map(|v| v as u32),
        "{name} marker"
    );
    let actual: Vec<_> = pool
        .occupants
        .iter()
        .enumerate()
        .filter_map(|(slot, id)| {
            id.map(|id| match id {
                ParticleId::Pool(payload) => json!([slot, payload]),
                ParticleId::Scratch => panic!("Scratch has no physical cell"),
            })
        })
        .collect();
    assert_eq!(
        actual,
        *wanted["occupants"].as_array().unwrap(),
        "{name} occupants"
    );
    for category in 0..6 {
        for (key, actual) in [
            ("active", &pool.active[category]),
            ("pending", &pool.pending[category]),
        ] {
            let actual: Vec<_> = actual
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!("Scratch has no list node"),
                })
                .collect();
            assert_eq!(
                actual,
                slots(&wanted[key][category]),
                "{name} {key}/{category}"
            );
        }
    }
    for (region, base, size) in [
        ("shared", 0x530e60, 860),
        ("registry", 0x53dba8, 528),
        ("default", 0x546a94, 4),
    ] {
        assert_eq!(
            live.tables.read_process_bytes(base, size).unwrap(),
            bytes(&wanted[region]),
            "{name} {region} data/masks"
        );
    }
    assert_eq!(Some(*live.rng), dword(&bytes(&wanted["rng"])), "{name} RNG");
    assert_eq!(
        Some(live.process.network_flag),
        dword(&bytes(&wanted["network"])),
        "{name} network"
    );
    assert_eq!(
        host.phase.snapshot(),
        dword(&bytes(&wanted["phase"])),
        "{name} phase"
    );
    assert_eq!(
        live.process.process_null_handle,
        dword(&bytes(&wanted["process_null"])).map(ActorHandle::from_raw),
        "{name} process null"
    );
    let state = arena
        .derived_actor(host.candidate)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    assert_eq!(
        Some(state.byte_10),
        bytes(&wanted["behavior"])[0x10],
        "{name} behavior index"
    );
    assert_eq!(
        Some(state.table_14 as u32),
        dword(&bytes(&wanted["behavior"])[0x14..0x18]),
        "{name} live behavior table"
    );
    let responses: Vec<_> = arena
        .actor(host.candidate)
        .unwrap()
        .damage_responses()
        .iter()
        .flat_map(|&(threshold, multiplier)| {
            threshold
                .to_le_bytes()
                .into_iter()
                .chain(multiplier.to_le_bytes())
                .map(Some)
        })
        .collect();
    assert_eq!(
        responses,
        bytes(&wanted["responses"]),
        "{name} live candidate response bytes"
    );
}

#[test]
fn behavior_particle_adapter_matches_composed_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/behavior_particle_adapter_native.json"
    ))
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
        json!({"cases":26,"predicate_calls":27,"native_entries":370,"kernel_cases":23,"scratch_prefix_cases":3})
    );
    let raw = super::tests::raw_executable();
    let exe = FrontendExecutable::parse(&raw).unwrap();
    let (mut cases, mut steps, mut scratch_prefixes) = (0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert!(matches!(
            input["comparison_scope"].as_str(),
            Some("kernel" | "scratch_prefix")
        ));
        let mut pool = ProcessParticleRuntime::default();
        let null = ActorHandle::from_raw(number(&input["process_null"]));
        let mut process = ParticleFactoryProcessState {
            network_flag: u32::from(input["network_active"].as_bool().unwrap()),
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(null),
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
        let (mut arena, candidate) = candidate(null);
        let responses: Vec<_> = input["responses"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|pair| {
                pair.as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|v| (number(v) as u16).to_le_bytes().map(Some))
            })
            .collect();
        arena.set_behavior_particle_receipt_responses(candidate, payload(&responses));
        let mut host = Services {
            phase:
                crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(
                    Some(number(&input["phase"])),
                ),
            candidate,
            behavior: vec![Some(0); 0x18],
            responses,
            invocation: Value::Null,
            expected: vec![],
            event_index: 0,
            network_index: 0,
            impact_index: 0,
            scratch: None,
        };
        assert!(
            input["prepare_first_base"].is_null(),
            "no extra preparation modes admitted"
        );
        let prepare = &input["prepare_factory"];
        assert_eq!(prepare["address"], 0x466710);
        let mut sources = ParticleFactorySources {
            position: ParticleVectorBytes(bytes(&input["position"]).try_into().unwrap()),
            separate_velocity: Some(ParticleVectorBytes(
                bytes(&input["velocity"]).try_into().unwrap(),
            )),
        };
        for _ in 0..number(&input["prepare_count"]) {
            pool.create_factory_particle(
                ParticleFactoryRequest {
                    kind: ParticleFactoryKind::Factory466710,
                    selector: number(&prepare["selector"]),
                    source_handle: ActorHandle::from_raw(number(&prepare["source_handle"])),
                    flags: number(&prepare["flags"]),
                    velocity_source: ParticleVelocitySource::Separate,
                },
                &mut ParticleFactoryContext {
                    process: &mut process,
                    rng: &mut rng,
                    executable: exe,
                },
                &mut sources,
                &mut Preparation { phase: &host.phase },
            )
            .unwrap();
        }
        if input["prepare_transfer"].as_bool().unwrap() {
            pool.transfer_pending();
        }
        let mut live = BehaviorPredicateContext {
            tables: &mut tables,
            process: &mut process,
            rng: &mut rng,
        };
        host.apply(&mut arena, &mut live, &mut pool, None, &json!({"region":"shared","offset":0,"bytes":(number(&input["kind"]) as u16).to_le_bytes()}));
        host.apply(&mut arena, &mut live, &mut pool, None, &json!({"region":"shared","offset":0x22,"bytes":[input["ready"].as_u64().unwrap_or(0)]}));
        for patch in input["patches"].as_array().unwrap() {
            host.apply(&mut arena, &mut live, &mut pool, None, patch);
        }
        for step in case["steps"].as_array().unwrap() {
            let invocation = &step["input"];
            assert_eq!(invocation["operation"], "predicate");
            assert_eq!(
                bytes(&invocation["scratch_bytes"]),
                [None; 8],
                "no new assumptions about call-local scratch"
            );
            for patch in invocation["patches"].as_array().unwrap() {
                host.apply(&mut arena, &mut live, &mut pool, None, patch);
            }
            if !invocation["selection"].is_null() {
                let selection = &invocation["selection"];
                let selected = number(&fixture["tables"][selection["table"].as_str().unwrap()]);
                host.apply(&mut arena, &mut live, &mut pool, None, &json!({"region":"behavior","offset":0x10,"bytes":[number(&selection["index_byte"])]}));
                host.apply(
                    &mut arena,
                    &mut live,
                    &mut pool,
                    None,
                    &json!({"region":"behavior","offset":0x14,"bytes":selected.to_le_bytes()}),
                );
            }
            let selection = arena.actor_behavior_row_selection(candidate).unwrap();
            assert_eq!(
                selection,
                BehaviorRowSelection::SharedAddress {
                    address: number(&step["selected_address"])
                }
            );
            host.invocation = invocation.clone();
            host.expected = step["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| {
                    matches!(
                        event["kind"].as_str(),
                        Some("supplied_service" | "required_boundary")
                    )
                })
                .cloned()
                .collect();
            host.event_index = 0;
            host.network_index = 0;
            host.impact_index = 0;
            host.scratch = None;
            let result = arena.test_behavior_damage_candidate(
                BehaviorPredicateRequest {
                    selection,
                    candidate: match invocation["candidate"].as_str().unwrap() {
                        "live" => Some(candidate),
                        "null" => None,
                        _ => panic!("unadmitted candidate"),
                    },
                },
                &mut live,
                &mut BehaviorParticleAdapter {
                    particles: &mut pool,
                    executable: exe,
                    services: &mut host,
                },
            );
            assert!(step["fault"].is_null());
            match step["stopped"].as_str() {
                None => assert_eq!(
                    result.as_ref().ok().copied().map(u8::from),
                    step["result"].as_u64().map(|v| v as u8),
                    "{name}: {result:?}"
                ),
                Some("unknown_process_null_0") => assert!(
                    matches!(
                        result,
                        Err(BehaviorPredicateError::MissingProcessNull { address: 0x48d87c })
                    ),
                    "{name}: {result:?}"
                ),
                Some("required_actor_resource") => assert!(
                    matches!(result, Err(BehaviorPredicateError::Host { address:0x48d8d3, error:BehaviorParticleAdapterError::Lower(ref v) }) if v=="required_actor_resource"),
                    "{name}: {result:?}"
                ),
                Some("required_network_notice" | "network_service_unavailable") => {
                    let expected = if step["stopped"] == "required_network_notice" {
                        "required_network_notice"
                    } else {
                        "unavailable"
                    };
                    assert!(
                        matches!(result, Err(BehaviorPredicateError::Host { address:0x48d891, error:BehaviorParticleAdapterError::Factory(ParticleFactoryError::Network(ref v)) }) if v==expected),
                        "{name}: {result:?}"
                    );
                }
                Some("impact_service_unavailable") => assert!(
                    matches!(result, Err(BehaviorPredicateError::Host { address:0x48d891, error:BehaviorParticleAdapterError::Factory(ParticleFactoryError::Impact(ref v)) }) if v=="unavailable"),
                    "{name}: {result:?}"
                ),
                Some("outside_storage_write") => {
                    assert_eq!(input["comparison_scope"], "scratch_prefix");
                    assert!(
                        matches!(
                            result,
                            Err(BehaviorPredicateError::Host {
                                address: 0x48d8b0,
                                error: BehaviorParticleAdapterError::Factory(
                                    ParticleFactoryError::Particle(ParticleError::MissingNode(
                                        ParticleId::Scratch
                                    ))
                                )
                            })
                        ),
                        "{name}: {result:?}"
                    );
                    scratch_prefixes += 1;
                }
                other => panic!("unadmitted stopped state {other:?}"),
            }
            assert_eq!(
                host.event_index,
                host.expected.len(),
                "{name} service cadence"
            );
            assert_eq!(
                host.network_index,
                invocation["network_services"].as_array().unwrap().len()
            );
            assert_eq!(
                host.impact_index,
                invocation["impact_services"].as_array().unwrap().len()
            );
            if let Some(scratch) = host.scratch {
                assert_eq!(
                    scratch.as_slice(),
                    bytes(&step["scratch_bytes"]),
                    "{name} retained eight-byte source"
                );
            }
            // The snapshot keeps cumulative touched freed cells, so a ready
            // skip still detects mutations of earlier retained data/masks.
            compare_state(&pool, &arena, &live, &host, &step["state"], name);
            steps += 1;
        }
        cases += 1;
    }
    assert_eq!((cases, steps, scratch_prefixes), (26, 27, 3));
}
