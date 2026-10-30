use std::{cell::RefCell, rc::Rc};

use serde::Deserialize;
use serde_json::{json, Value};

use super::*;
use crate::name_entry::name_entry_rng_next;

#[derive(Deserialize)]
struct NativeFixture {
    executable_sha256: String,
    cases: Vec<NativeCase>,
}

#[derive(Deserialize)]
struct NativeCase {
    name: String,
    policy: String,
    selector_word: u16,
    extras: Vec<u16>,
    source_index: u16,
    source: [i32; 3],
    destination: u16,
    initial_destination: [[i32; 3]; 2],
    mode: u16,
    load_factor: i32,
    rng: u32,
    #[serde(default)]
    missing_source: bool,
    missing_source_after: Option<usize>,
    #[serde(default)]
    missing_factor: bool,
    fail_random_after: Option<usize>,
    #[serde(default)]
    fail_trail: bool,
    fail_projection_after: Option<usize>,
    mutate_source: Option<[i32; 3]>,
    events: Vec<Value>,
    destinations: [[i32; 3]; 2],
    projections: [[Option<u8>; 12]; 2],
    final_rng: u32,
    error: Option<NativeError>,
    temporary_output: Option<[i32; 3]>,
}

#[derive(Deserialize)]
struct NativeError {
    kind: String,
}

type Events = Rc<RefCell<Vec<Value>>>;

struct Host<'a> {
    case: &'a NativeCase,
    rng: u32,
    draws: usize,
    trails: usize,
    events: Events,
}

impl FurnitureCustomVertexHost for Host<'_> {
    type Error = &'static str;

    fn load_factor(&mut self) -> Result<i32, Self::Error> {
        self.events
            .borrow_mut()
            .push(json!({"kind":"factor","value":self.case.load_factor}));
        if self.case.missing_factor {
            Err("factor")
        } else {
            Ok(self.case.load_factor)
        }
    }

    fn random(&mut self) -> Result<u32, Self::Error> {
        let value = name_entry_rng_next(&mut self.rng);
        self.draws += 1;
        self.events
            .borrow_mut()
            .push(json!({"kind":"random","value":value,"rng":self.rng}));
        if self.case.fail_random_after == Some(self.draws) {
            Err("random")
        } else {
            Ok(value)
        }
    }

    fn birth_trail(&mut self, request: FurnitureCustomTrail) -> Result<(), Self::Error> {
        self.trails += 1;
        self.events.borrow_mut().push(json!({"kind":"trail",
            "particle_type":request.particle_type,"position":request.position,
            "velocity":request.velocity,"flags":request.flags}));
        if self.case.fail_trail {
            Err("trail")
        } else {
            Ok(())
        }
    }
}

struct Projection {
    events: Events,
    calls: usize,
    fail_after: Option<usize>,
}

impl ModelVertexProjector for Projection {
    type Error = &'static str;

    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        let ModelVertexInput::World(world) = input else {
            panic!("custom2D uses the world projection callback")
        };
        self.events
            .borrow_mut()
            .push(json!({"kind":"projection","world":world}));
        self.calls += 1;
        output.bytes[8] = Some(0x31);
        if self.fail_after == Some(self.calls) {
            return Err("projection");
        }
        output.write_u32(0, 0x1234_5678);
        Ok(self.calls as u32)
    }
}

fn normalized(mut event: Value) -> Value {
    let object = event.as_object_mut().unwrap();
    if object["kind"] == "factor" || object["kind"] == "trail" {
        object.remove("output");
    }
    object.remove("destinations");
    event
}

#[test]
fn furniture_custom_matches_105_original_instruction_receipts() {
    let fixture: NativeFixture = serde_json::from_str(include_str!(
        "../../tests/fixtures/model_furniture_callback_native.json"
    ))
    .unwrap();
    assert_eq!(
        fixture.executable_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(fixture.cases.len(), 105);
    for case in &fixture.cases {
        let events = Events::default();
        let mut host = Host {
            case,
            rng: case.rng,
            draws: 0,
            trails: 0,
            events: Rc::clone(&events),
        };
        let count =
            (usize::from(case.source_index) + 1).max(usize::from(case.destination) + 2) as u16;
        let mut vertices = ModelVertices::new_invocation(count);
        vertices.slots[usize::from(case.source_index)].world = Some(case.source);
        for parity in 0..2 {
            vertices.slots[usize::from(case.destination) + parity].world =
                Some(case.initial_destination[parity]);
        }
        if case.missing_source {
            vertices.slots[usize::from(case.source_index)].world = None;
        }
        let mut words = vec![
            0x2d | (case.mode << 8) | ((case.extras.len() as u16) << 12),
            case.destination,
            case.selector_word,
        ];
        words.extend_from_slice(&case.extras);
        words.push(0);
        let instruction = inf_formats::decode_model_program(&words)
            .unwrap()
            .instructions
            .remove(0);
        let mut projection = Projection {
            events: Rc::clone(&events),
            calls: 0,
            fail_after: case.fail_projection_after,
        };
        let furniture = case.policy == "furniture";
        let policy = if furniture {
            ModelCustomVertexPolicy::Furniture
        } else {
            ModelCustomVertexPolicy::UnsignedRowSource
        };
        let mut temporary = None;
        let mut row_reads = 0;
        let result = vertices.execute_custom_2d(
            &instruction,
            policy,
            &mut projection,
            &mut |request, vertices, output| {
                events
                    .borrow_mut()
                    .push(json!({"kind":"callback","selector":request.selector}));
                if request.selector == 7 {
                    row_reads += 1;
                    events
                        .borrow_mut()
                        .push(json!({"kind":"row","index":request.extras[0]}));
                    if case.missing_source_after == Some(row_reads) {
                        vertices.slots[usize::from(case.source_index)].world = None;
                    }
                }
                let trails_before = host.trails;
                let result = compute_furniture_custom_vertex(request, vertices, output, &mut host);
                // Explicit supplied-host mutation, performed even when that
                // host fails after its birth call. The next callback rereads.
                if host.trails != trails_before {
                    if let Some(world) = case.mutate_source {
                        vertices.slots[usize::from(case.source_index)].world = Some(world);
                    }
                }
                temporary = *output;
                if result.is_ok() {
                    events
                        .borrow_mut()
                        .push(json!({"kind":"returned","output":output}));
                }
                result
            },
        );
        let error = match &result {
            Ok(()) => None,
            Err(ModelVertexError::Custom(ModelCustomVertexError::Host(kind)))
            | Err(ModelVertexError::Projection(kind)) => Some(*kind),
            Err(ModelVertexError::Custom(ModelCustomVertexError::Fault(
                ModelVertexFault::UnknownWorld { .. },
            ))) => Some("source"),
            other => panic!("{}: unexpected result {other:?}", case.name),
        };
        assert_eq!(
            error,
            case.error.as_ref().map(|error| error.kind.as_str()),
            "{}",
            case.name
        );
        for parity in 0..2 {
            let slot = &vertices.slots[usize::from(case.destination) + parity];
            assert_eq!(slot.world, Some(case.destinations[parity]), "{}", case.name);
            assert_eq!(
                slot.projection.bytes, case.projections[parity],
                "{}",
                case.name
            );
        }
        assert_eq!(host.rng, case.final_rng, "{}", case.name);
        assert_eq!(vertices.stopped(), result.is_err(), "{}", case.name);
        let native_events: Vec<_> = case
            .events
            .iter()
            .filter(|event| furniture || event["kind"] == "projection")
            .cloned()
            .map(normalized)
            .collect();
        assert_eq!(*events.borrow(), native_events, "{}", case.name);
        if furniture {
            assert_eq!(temporary, case.temporary_output, "{}", case.name);
        }
    }
}

#[test]
fn birth_failure_retains_temporary_output_and_mutation_without_publishing_destination() {
    let mut vertices = ModelVertices::new_invocation(3);
    vertices.slots[0].world = Some([65535, 32768, 65537]);
    vertices.slots[1].world = Some([100, 200, 300]);
    let instruction = inf_formats::decode_model_program(&[0x232d, 1, 7, 0, 3, 0])
        .unwrap()
        .instructions
        .remove(0);
    let case: NativeFixture = serde_json::from_str(include_str!(
        "../../tests/fixtures/model_furniture_callback_native.json"
    ))
    .unwrap();
    let case = case.cases.iter().find(|case| case.fail_trail).unwrap();
    let mut host = Host {
        case,
        rng: 0,
        draws: 0,
        trails: 0,
        events: Events::default(),
    };
    let mut projection = Projection {
        events: Events::default(),
        calls: 0,
        fail_after: None,
    };
    let mut reached_output = None;
    let result = vertices.execute_custom_2d(
        &instruction,
        ModelCustomVertexPolicy::Furniture,
        &mut projection,
        &mut |request, vertices, output| {
            let result = compute_furniture_custom_vertex(request, vertices, output, &mut host);
            reached_output = *output;
            // The actual effect boundary may mutate other live rows before
            // failing; those writes must remain, independently of publication.
            vertices.slots[2].world = Some([7, 8, 9]);
            result
        },
    );
    assert_eq!(
        result,
        Err(ModelVertexError::Custom(ModelCustomVertexError::Host(
            "trail"
        )))
    );
    assert_eq!(reached_output, Some([-1, -32768, 1]));
    assert_eq!(vertices.slots[1].world, Some([100, 200, 300]));
    assert_eq!(vertices.slots[2].world, Some([7, 8, 9]));
    assert_eq!(projection.calls, 0);
    assert_eq!(host.draws, 1);
    assert!(vertices.stopped());
}

#[test]
fn custom_output_and_rng_survive_a_real_late_missing_source_stop() {
    use crate::process_particles::{
        births::{
            ParticleBirthError, ParticleBirthInputs, ParticleBirthProducer, ParticleBirthRequest,
            ParticleBirthSourceError,
        },
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    };
    struct Host<'a> {
        process: &'a ParticleFactoryProcessState,
        executable: inf_formats::FrontendExecutable<'a>,
        particles: ProcessParticleRuntime,
        rng: u32,
    }
    impl FurnitureCustomVertexHost for Host<'_> {
        type Error = ParticleBirthError<std::convert::Infallible>;
        fn load_factor(&mut self) -> Result<i32, Self::Error> {
            Ok(0xc000)
        }
        fn random(&mut self) -> Result<u32, Self::Error> {
            Ok(name_entry_rng_next(&mut self.rng))
        }
        fn birth_trail(&mut self, request: FurnitureCustomTrail) -> Result<(), Self::Error> {
            self.particles.birth_trail(
                ParticleBirthRequest {
                    particle_type: request.particle_type,
                    position: request.position,
                    velocity: Some(request.velocity),
                    flags: request.flags,
                    inputs: ParticleBirthInputs {
                        network_active: false,
                        phase: Some(4),
                        process: self.process,
                    },
                },
                self.executable,
                &mut self.rng,
            )?;
            Ok(())
        }
    }
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    let process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    let mut host = Host {
        process: &process,
        executable,
        particles: ProcessParticleRuntime::default(),
        rng: 0x1020_3040,
    };
    let mut vertices = ModelVertices::new_invocation(5);
    vertices.slots[4].world = Some([65530, 1000, 9]);
    let mut output = None;
    compute_furniture_custom_vertex(
        ModelCustomVertexRequest {
            selector: 0x10007,
            extras: &[],
        },
        &mut vertices,
        &mut output,
        &mut host,
    )
    .unwrap();
    assert_eq!(output, Some([0; 3]));
    assert_eq!(host.rng, 0x1020_3040);
    assert_eq!(host.particles.free_count(), 400);
    let result = compute_furniture_custom_vertex(
        ModelCustomVertexRequest {
            selector: 7,
            extras: &[4, 1],
        },
        &mut vertices,
        &mut output,
        &mut host,
    );
    assert!(matches!(
        result,
        Err(ModelCustomVertexError::Host(ParticleBirthError::Source(
            ParticleBirthSourceError {
                producer: ParticleBirthProducer::Trail
            }
        )))
    ));
    assert_eq!(output, Some([-6, 1000, 9]));
    let mut expected_rng = 0x1020_3040;
    name_entry_rng_next(&mut expected_rng);
    assert_eq!(host.rng, expected_rng);
    assert_eq!(host.particles.free_count(), 399);
    assert_eq!(host.particles.allocation_marker(), Some(399));
    assert!(host.particles.pending(1).unwrap().is_empty());
    assert_eq!(
        host.particles
            .record(crate::process_particles::ParticleId::Pool(399))
            .unwrap()
            .source_bytes::<24>(0)
            .unwrap(),
        [None; 24]
    );
}

#[test]
fn custom_packets_reach_real_trail_tables_pending_list_and_shared_rng() {
    use crate::process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        ParticleId, ProcessParticleRuntime,
    };
    use inf_formats::FrontendExecutable;

    struct PoolHost<'a> {
        executable: FrontendExecutable<'a>,
        particles: ProcessParticleRuntime,
        rng: u32,
        inputs: ParticleBirthInputs<'a>,
        births: Vec<ParticleId>,
    }

    impl FurnitureCustomVertexHost for PoolHost<'_> {
        type Error = String;

        fn load_factor(&mut self) -> Result<i32, Self::Error> {
            Ok(49152)
        }

        fn random(&mut self) -> Result<u32, Self::Error> {
            Ok(name_entry_rng_next(&mut self.rng))
        }

        fn birth_trail(&mut self, request: FurnitureCustomTrail) -> Result<(), Self::Error> {
            let id = self
                .particles
                .birth_trail(
                    ParticleBirthRequest {
                        particle_type: request.particle_type,
                        position: request.position,
                        velocity: Some(request.velocity),
                        flags: request.flags,
                        inputs: self.inputs,
                    },
                    self.executable,
                    &mut self.rng,
                )
                .map_err(|error| error.to_string())?
                .ok_or("supplied supported trail selector returned null")?;
            self.births.push(id);
            Ok(())
        }
    }

    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .expect("this corpus test requires the original retail executable");
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let fixture: NativeFixture = serde_json::from_str(include_str!(
        "../../tests/fixtures/model_furniture_callback_native.json"
    ))
    .unwrap();
    for (kind, selector, table) in [
        (0, 5, 0x53d1e0),
        (1, 6, 0x53d1f0),
        (2, 8, 0x53d200),
        (3, 11, 0x53d210),
    ] {
        let case = fixture
            .cases
            .iter()
            .find(|case| case.name == format!("kind-{kind}"))
            .unwrap();
        let mut vertices = ModelVertices::new_invocation(5);
        vertices.slots[4].world = Some(case.source);
        let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
        };
        let mut host = PoolHost {
            executable,
            particles: ProcessParticleRuntime::default(),
            rng: case.rng,
            inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &birth_process,
            },
            births: Vec::new(),
        };
        let mut output = None;
        compute_furniture_custom_vertex(
            ModelCustomVertexRequest {
                selector: 7,
                extras: &[4, kind],
            },
            &mut vertices,
            &mut output,
            &mut host,
        )
        .unwrap();
        assert_eq!(output, case.temporary_output);
        assert_eq!(host.births.len(), 1);
        let id = host.births[0];
        let record = host.particles.record(id).unwrap();
        let trail = case
            .events
            .iter()
            .find(|event| event["kind"] == "trail")
            .unwrap();
        assert_eq!(trail["particle_type"], selector);
        assert_eq!(
            serde_json::to_value(record.position().unwrap()).unwrap(),
            trail["position"]
        );
        assert_eq!(
            serde_json::to_value(std::array::from_fn::<_, 3, _>(|axis| record
                .read_word(10 + axis * 2)
                .unwrap()))
            .unwrap(),
            trail["velocity"]
        );
        assert_eq!(record.read_dword(0).unwrap(), 0x5288f4);
        assert_eq!(record.read_dword(0x28).unwrap(), table);
        assert_eq!(
            record.read_dword(0x14).unwrap(),
            host.inputs.process.process_null_handle.unwrap().raw()
        );
        assert_eq!(record.read_word(0x12).unwrap(), 0x089c);
        let mut expected_rng = case.final_rng;
        let draw = name_entry_rng_next(&mut expected_rng) as u16;
        assert_eq!(host.rng, expected_rng);
        assert_eq!(
            record.read_dword(0x2c).unwrap(),
            0x200 + u32::from(draw >> 9) - 64
        );
        assert_eq!(
            host.particles
                .pending(1)
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [id]
        );
        for category in 0..crate::process_particles::PARTICLE_LIST_COUNT {
            assert!(host.particles.active(category).unwrap().is_empty());
            if category != 1 {
                assert!(host.particles.pending(category).unwrap().is_empty());
            }
        }
    }
}
