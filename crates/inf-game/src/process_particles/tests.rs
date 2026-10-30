use super::audio::ParticleSoundRequest;
use super::births::*;
use super::timing::*;
use super::*;

fn allocation(last_reclaim_category: i32) -> ParticleAllocationRequest {
    ParticleAllocationRequest {
        payload_bytes: 0x34,
        last_reclaim_category,
        network_active: false,
        phase: Some(4),
    }
}

fn construct(
    particles: &mut ProcessParticleRuntime,
    id: ParticleId,
    category: usize,
    velocity: Option<[u16; 3]>,
) {
    particles
        .construct(
            id,
            ParticleConstruction {
                category,
                position: [65535, 32767, 4].into(),
                velocity: velocity.map(Into::into),
                source_handle: 0x12345678,
                flags: 0x100,
            },
        )
        .unwrap();
}

#[test]
fn particle_pool_reclaims_only_eligible_active_tail_after_pending_transfer() {
    let mut particles = ProcessParticleRuntime::default();
    for expected in (0..400).rev() {
        let id = particles.allocate(allocation(1)).unwrap();
        assert_eq!(id, ParticleId::Pool(expected));
        assert_eq!(particles.occupant(usize::from(expected)), None);
        construct(&mut particles, id, 1, Some([0; 3]));
        assert_eq!(particles.occupant(usize::from(expected)), Some(id));
    }
    assert_eq!(particles.free_count(), 0);
    // All births still pending; allocator must not search them.
    assert_eq!(
        particles.allocate(allocation(1)).unwrap(),
        ParticleId::Scratch
    );
    particles.transfer_pending();
    assert_eq!(
        particles.active(1).unwrap().front(),
        Some(&ParticleId::Pool(0))
    );
    // All constructors set bit 0x80. The limiting bank skips a protected tail.
    assert_eq!(
        particles.allocate(allocation(1)).unwrap(),
        ParticleId::Scratch
    );
    particles
        .record_mut(ParticleId::Pool(399))
        .unwrap()
        .write_word(0x12, 0)
        .unwrap();
    assert_eq!(
        particles.allocate(allocation(1)).unwrap(),
        ParticleId::Pool(399)
    );
    assert_eq!(particles.allocation_marker(), Some(399));
    assert_eq!(particles.occupant(399), None);
    assert_eq!(
        particles.active(1).unwrap().back(),
        Some(&ParticleId::Pool(398))
    );
}

#[test]
fn particle_pool_earlier_category_reclaims_protected_tail_and_preserves_heap_bytes() {
    let mut particles = ProcessParticleRuntime::default();
    for _ in 0..400 {
        let id = particles.allocate(allocation(1)).unwrap();
        construct(&mut particles, id, 0, Some([1, 2, 3]));
    }
    particles.transfer_pending();
    particles
        .record_mut(ParticleId::Pool(399))
        .unwrap()
        .write_word(0x3e, 0xbeef)
        .unwrap();
    let id = particles.allocate(allocation(1)).unwrap();
    assert_eq!(id, ParticleId::Pool(399));
    construct(&mut particles, id, 3, None);
    let record = particles.record(id).unwrap();
    assert_eq!(record.read_word(0x3e).unwrap(), 0xbeef);
    assert_eq!(record.read_word(0x0a).unwrap(), 1);
    assert_eq!(record.read_word(0x0c).unwrap(), 2);
    assert_eq!(record.read_word(0x0e).unwrap(), 3);
    assert_eq!(record.read_word(0x12).unwrap(), 0x19d);
    assert_eq!(particles.pending(3).unwrap().front(), Some(&id));
}

#[test]
fn particle_phase_gate_keeps_nonnull_scratch_and_reads_no_phase_with_network() {
    let mut particles = ProcessParticleRuntime::default();
    let mut request = allocation(1);
    request.phase = Some(3);
    let id = particles.allocate(request).unwrap();
    assert_eq!(id, ParticleId::Scratch);
    construct(&mut particles, id, 100, None); // marker null: no category read.
    assert_eq!(particles.free_count(), 400);
    assert_eq!(
        particles.record(id).unwrap().position().unwrap(),
        [65535, 32767, 4]
    );
    assert_eq!(particles.record(id).unwrap().read_word(0x0a).unwrap(), 0);
    request.phase = None;
    assert_eq!(
        particles.allocate(request),
        Err(ParticleError::MissingPhase)
    );
    request.network_active = true;
    assert_eq!(particles.allocate(request).unwrap(), ParticleId::Pool(399));
}

#[test]
fn particle_constructor_unknown_velocity_and_clear_order_have_explicit_custody() {
    let mut particles = ProcessParticleRuntime::default();
    let id = particles.allocate(allocation(-1)).unwrap();
    construct(&mut particles, id, 1, None);
    assert_eq!(
        particles.record(id).unwrap().read_word(0x0a),
        Err(ParticleError::Unwritten {
            offset: 0x0a,
            bytes: 2
        })
    );
    assert_eq!(
        particles.record(id).unwrap().read_dword(0x20),
        Err(ParticleError::PortableListPointer {
            offset: 0x20,
            bytes: 4
        })
    );
    let second = particles.allocate(allocation(-1)).unwrap();
    construct(&mut particles, second, 1, None);
    particles
        .record_mut(id)
        .unwrap()
        .write_dword(0, 0xdeadbeef)
        .unwrap();
    particles.clear().unwrap();
    // Direct clear does not dispatch the unknown destructor or touch vtable.
    assert_eq!(particles.occupant(398), None);
    assert_eq!(particles.occupant(399), None);
    assert_eq!(particles.free_count(), 400);
    assert_eq!(
        particles.record(id).unwrap().read_dword(0).unwrap(),
        0xdeadbeef
    );
    // Physical ascending release leaves 399 at the free-stack head.
    assert_eq!(particles.allocate(allocation(-1)).unwrap(), id);
}

#[test]
fn particle_timing_owns_native_constructor_ring_threshold_and_reached_failure() {
    let mut timing = WorldTimingRing::default();
    assert_eq!(timing.samples, [125000; 8]);
    for _ in 0..8 {
        timing.tick(34464, 0).unwrap();
    }
    assert_eq!(timing.average, 34464);
    assert_eq!(timing.particle_load_factor, 65536); // 100000 - 34464, same endpoint.
    timing.tick(34472, 0).unwrap();
    assert_eq!(timing.average, 34465);
    assert_eq!(timing.particle_load_factor, 65535);
    timing.tick(10000, 20000).unwrap();
    assert_eq!(timing.samples, [20000; 8]);
    assert_eq!(timing.particle_load_factor, 0);
    timing.samples = [0; 8];
    timing.sum = 0;
    let previous_fps = timing.frames_per_second;
    let previous_cursor = timing.cursor;
    assert_eq!(timing.tick(0, 0), Err(TimingDivisionByZero));
    assert_eq!(timing.cursor, previous_cursor + 1);
    assert_eq!(timing.particle_load_factor, 65536);
    assert_eq!(timing.frames_per_second, previous_fps);
    // The native input comparison is signed even though the average is not.
    timing.tick(u32::MAX, 1).unwrap();
    assert_eq!(timing.samples, [1; 8]);
    assert_eq!(timing.elapsed_milliseconds, 34 * 8 + 34 + 10);
}

struct Audio {
    calls: u32,
    requests: Vec<ParticleSoundRequest>,
    fail: bool,
}
impl ParticleBirthAudio for Audio {
    type Error = &'static str;
    fn explosion_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls += 1;
        self.requests.push(request);
        if self.fail {
            Err("missing native audio")
        } else {
            Ok(())
        }
    }
}

#[derive(serde::Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}
#[derive(serde::Deserialize)]
struct Record {
    slot: u16,
    fields: Vec<u32>,
}
#[derive(serde::Deserialize)]
struct NativeSoundRequest {
    sound: u32,
    position: [u16; 3],
    volume: u32,
    pitch: u32,
    flags: u32,
}

impl From<NativeSoundRequest> for ParticleSoundRequest {
    fn from(request: NativeSoundRequest) -> Self {
        Self {
            sound: request.sound,
            position: request.position,
            volume: request.volume,
            pitch: request.pitch,
            flags: request.flags,
        }
    }
}
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    kind: String,
    rng: u32,
    flags: Option<u32>,
    particle_type: Option<u32>,
    position: Option<[u16; 3]>,
    velocity: Option<[u16; 3]>,
    has_velocity: Option<bool>,
    repeat_after_transfer: Option<bool>,
    phase: Option<u32>,
    network_active: Option<bool>,
    cursor: Option<u32>,
    load_factor: Option<u32>,
    debris_count: Option<u16>,
    radius: Option<u16>,
    central_type: Option<u32>,
    debris_types: Option<[u32; 2]>,
    speed: Option<u32>,
    records: Vec<Record>,
    scratch: Vec<u32>,
    pending: Vec<u16>,
    active: Vec<u16>,
    final_rng: u32,
    final_cursor: u32,
    audio: Vec<NativeSoundRequest>,
    audio_error_cleanup_count: usize,
}

fn executable_bytes() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
    std::fs::read(path)
        .expect("particle native receipts require the local supported retail executable")
}

fn compare_fields(particles: &ProcessParticleRuntime, id: ParticleId, fields: &[u32], name: &str) {
    let common_fields = [
        (0, 4),
        (4, 2),
        (6, 2),
        (8, 2),
        (10, 2),
        (12, 2),
        (14, 2),
        (16, 2),
        (18, 2),
        (20, 4),
        (40, 4),
        (44, 4),
        (48, 4),
    ];
    let extra_fields = [(52, 4), (56, 2), (58, 2)];
    let layout = common_fields.into_iter().chain(
        if fields.len() == 16 {
            &extra_fields[..]
        } else {
            &[]
        }
        .iter()
        .copied(),
    );
    for ((offset, bytes), value) in layout.zip(fields) {
        let actual = match bytes {
            2 => u32::from(particles.record(id).unwrap().read_word(offset).unwrap()),
            4 => particles.record(id).unwrap().read_dword(offset).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(actual, *value, "{name} {id:?} +{offset:02X}");
    }
}

#[test]
fn particle_births_match_original_native_receipts_for_all_types_and_retirement_bursts() {
    let raw = executable_bytes();
    let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../tests/fixtures/particle_births_native.json"
    ))
    .unwrap();
    for case in receipt.cases {
        let mut particles = ProcessParticleRuntime::default();
        particles.vector_cursor = case.cursor.unwrap_or(0);
        let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
        };
        let inputs = ParticleBirthInputs {
            network_active: case.network_active.unwrap_or(false),
            phase: Some(case.phase.unwrap_or(4)),
            process: &birth_process,
        };
        let mut rng = case.rng;
        let mut audio = Audio {
            calls: 0,
            requests: Vec::new(),
            fail: false,
        };
        if case.kind == "burst" {
            particles
                .birth_burst(
                    ParticleBurstRequest {
                        burst: ParticleBurst {
                            debris_count: case.debris_count.unwrap(),
                            position: case.position.unwrap(),
                            placement_radius: case.radius.unwrap(),
                            central_type: case.central_type.unwrap(),
                            debris_types: case.debris_types.unwrap(),
                            speed: case.speed.unwrap(),
                            flags: case.flags.unwrap(),
                        },
                        load_factor: case.load_factor.unwrap_or(65536),
                        inputs,
                    },
                    executable,
                    &mut rng,
                    &mut audio,
                )
                .unwrap();
        } else {
            let request = ParticleBirthRequest {
                particle_type: case.particle_type.unwrap(),
                position: case.position.unwrap_or([32767, 65535, 300]),
                velocity: if case.kind == "debris" || case.has_velocity.unwrap_or(false) {
                    Some(case.velocity.unwrap_or([65535, 32, 0]))
                } else {
                    None
                },
                flags: case.flags.unwrap_or(0),
                inputs,
            };
            if case.kind == "debris" {
                particles
                    .birth_debris(request, executable, &mut rng)
                    .unwrap();
            } else {
                particles
                    .birth_central(request, executable, &mut rng, &mut audio)
                    .unwrap();
                if case.repeat_after_transfer.unwrap_or(false) {
                    particles.transfer_pending();
                    particles
                        .birth_central(request, executable, &mut rng, &mut audio)
                        .unwrap();
                }
            }
        }
        assert_eq!(rng, case.final_rng, "{} RNG", case.name);
        assert_eq!(
            particles.vector_cursor, case.final_cursor,
            "{} cursor",
            case.name
        );
        assert_eq!(
            audio.calls as usize, case.audio_error_cleanup_count,
            "{} nullable audio error cleanup",
            case.name
        );
        assert_eq!(
            audio.requests,
            case.audio
                .into_iter()
                .map(ParticleSoundRequest::from)
                .collect::<Vec<_>>(),
            "{} positional enqueue arguments",
            case.name
        );
        assert_eq!(
            particles
                .pending(1)
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            case.pending
                .iter()
                .map(|slot| ParticleId::Pool(*slot))
                .collect::<Vec<_>>(),
            "{} pending",
            case.name
        );
        for record in &case.records {
            compare_fields(
                &particles,
                ParticleId::Pool(record.slot),
                &record.fields,
                &case.name,
            );
        }
        if !case.scratch.is_empty() {
            compare_fields(&particles, ParticleId::Scratch, &case.scratch, &case.name);
        }
        particles.transfer_pending();
        assert_eq!(
            particles
                .active(1)
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            case.active
                .iter()
                .map(|slot| ParticleId::Pool(*slot))
                .collect::<Vec<_>>(),
            "{} active",
            case.name
        );
        assert_eq!(
            particles.free_count(),
            400 - case.records.len(),
            "{} pool",
            case.name
        );
    }
}

#[test]
fn particle_burst_missing_audio_retains_debris_but_does_not_invent_central_births() {
    let raw = executable_bytes();
    let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 0;
    let mut audio = Audio {
        calls: 0,
        requests: Vec::new(),
        fail: true,
    };
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0)),
    };
    let error = particles
        .birth_burst(
            ParticleBurstRequest {
                burst: ParticleBurst {
                    debris_count: 16,
                    position: [0; 3],
                    placement_radius: 20,
                    central_type: 1,
                    debris_types: [1; 2],
                    speed: 8,
                    flags: 0,
                },
                load_factor: 65536,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &birth_process,
                },
            },
            executable,
            &mut rng,
            &mut audio,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ParticleBirthError::Audio("missing native audio")
    ));
    assert_eq!(particles.pending(1).unwrap().len(), 16);
    assert_eq!(particles.free_count(), 384);
    assert_eq!(particles.vector_cursor, 16);
    let mut expected_rng = 0;
    for _ in 0..48 {
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
    }
    assert_eq!(rng, expected_rng);
    assert_eq!(
        audio.requests,
        [ParticleSoundRequest {
            sound: 6,
            position: [0; 3],
            volume: 65536,
            pitch: 65536,
            flags: 0,
        }]
    );
}

/// Production startup follows Infest01's fresh pool block: zero payload
/// bytes, except the leftover heap words in cell 32's payload +00..+17.
#[test]
fn fresh_pool_zero_approximation_keeps_only_cell_32_heap_words_unknown() {
    let runtime = ProcessParticleRuntime::fresh_pool_zero_approximation();
    for (cell, record) in runtime.records.iter().enumerate() {
        // +18/+1C/+20 are list pointers, which numeric reads always reject.
        for offset in (0..PARTICLE_BYTES)
            .step_by(4)
            .filter(|offset| !(0x18..0x24).contains(offset))
        {
            let read = record.read_dword(offset);
            if cell == 32 && offset < 0x18 {
                assert!(
                    matches!(read, Err(ParticleError::Unwritten { .. })),
                    "cell {cell} +{offset:02X}"
                );
            } else {
                assert_eq!(read, Ok(0), "cell {cell} +{offset:02X}");
            }
        }
    }
    assert!(runtime.occupants.iter().all(Option::is_none));
    assert_eq!(runtime.free.len(), PARTICLE_POOL_SIZE);
}
