use super::*;
use crate::object_runtime::ActorHandle;

#[derive(Default)]
struct Audio {
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
        self.requests.push(request);
        if self.fail {
            Err("required sound")
        } else {
            Ok(())
        }
    }
}

fn executable_bytes() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("source-boundary tests require the original executable")
}
fn process(source: Option<u32>) -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: source.map(ActorHandle::from_raw),
    }
}
fn request(process: &ParticleFactoryProcessState, particle_type: u32) -> ParticleBirthRequest<'_> {
    ParticleBirthRequest {
        particle_type,
        position: [65530, 1000, 9],
        velocity: Some([64, 0, 65530]),
        flags: 0,
        inputs: ParticleBirthInputs {
            network_active: false,
            phase: Some(4),
            process,
        },
    }
}
fn sparks(process: &ParticleFactoryProcessState) -> ParticleImpactSparksRequest<'_> {
    ParticleImpactSparksRequest {
        position: [65530, 1000, 9],
        strength: 1,
        inputs: ParticleBirthInputs {
            network_active: false,
            phase: Some(4),
            process,
        },
    }
}
fn source_stop<E: std::fmt::Debug>(
    result: Result<Option<ParticleId>, ParticleBirthError<E>>,
) -> ParticleBirthSourceError {
    match result {
        Err(ParticleBirthError::Source(error)) => error,
        other => panic!("expected reached source stop, got {other:?}"),
    }
}
fn payload(
    particles: &ProcessParticleRuntime,
    id: ParticleId,
) -> ([Option<u8>; 24], [Option<u8>; 28]) {
    let record = particles.record(id).unwrap();
    (
        record.source_bytes(0).unwrap(),
        record.source_bytes(0x24).unwrap(),
    )
}

#[test]
fn every_source_read_stops_after_allocation_before_constructor_publication() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    for (producer, address) in [
        (ParticleBirthProducer::Central, 0x48a215),
        (ParticleBirthProducer::Debris, 0x48a435),
        (ParticleBirthProducer::Trail, 0x489eb2),
        (ParticleBirthProducer::Trail12, 0x489f3a),
        (ParticleBirthProducer::Trail13, 0x489fbe),
        (ParticleBirthProducer::Splash, 0x489409),
        (ParticleBirthProducer::Impact, 0x489657),
        (ParticleBirthProducer::GroundImpact, 0x489521),
        (ParticleBirthProducer::FallingDrop, 0x4899cc),
        (ParticleBirthProducer::MaterialDust, 0x489cfd),
        (ParticleBirthProducer::PickupFlash, 0x489b0c),
        (ParticleBirthProducer::ShortSmoke, 0x489c29),
        (ParticleBirthProducer::TerrainFlash, 0x48add9),
        (ParticleBirthProducer::ImpactSpark, 0x48ac59),
    ] {
        let process = process(None);
        let mut particles = ProcessParticleRuntime::default();
        let before = payload(&particles, ParticleId::Pool(399));
        let mut rng = 0x1020_3040;
        let mut audio = Audio::default();
        let error = match producer {
            ParticleBirthProducer::Central => source_stop(particles.birth_central(
                request(&process, 0),
                executable,
                &mut rng,
                &mut audio,
            )),
            ParticleBirthProducer::Debris => {
                source_stop(particles.birth_debris(request(&process, 0), executable, &mut rng))
            }
            ParticleBirthProducer::Trail => {
                source_stop(particles.birth_trail(request(&process, 2), executable, &mut rng))
            }
            ParticleBirthProducer::Trail12 => {
                source_stop(particles.birth_trail(request(&process, 12), executable, &mut rng))
            }
            ParticleBirthProducer::Trail13 => {
                source_stop(particles.birth_trail(request(&process, 13), executable, &mut rng))
            }
            ParticleBirthProducer::Splash => {
                source_stop(particles.birth_splash(request(&process, 0), executable, &mut rng))
            }
            ParticleBirthProducer::Impact => {
                source_stop(particles.birth_impact(request(&process, 0), executable, &mut rng))
            }
            ParticleBirthProducer::GroundImpact => source_stop(particles.birth_material_sprite(
                request(&process, 0),
                true,
                executable,
                &mut rng,
            )),
            ParticleBirthProducer::FallingDrop => source_stop(particles.birth_falling_drop(
                request(&process, 0),
                executable,
                &mut rng,
            )),
            ParticleBirthProducer::MaterialDust => source_stop(particles.birth_material_dust(
                request(&process, 0),
                executable,
                &mut rng,
            )),
            ParticleBirthProducer::ShortSmoke => {
                source_stop(particles.birth_short_smoke(request(&process, 0), executable, &mut rng))
            }
            ParticleBirthProducer::PickupFlash => source_stop(particles.birth_pickup_flash(
                request(&process, 0),
                executable,
                &mut rng,
            )),
            ParticleBirthProducer::TerrainFlash => source_stop(particles.birth_terrain_flash(
                request(&process, 0),
                executable,
                &mut rng,
            )),
            ParticleBirthProducer::ImpactSpark => source_stop(
                particles
                    .birth_impact_sparks(sparks(&process), executable, &mut rng, &mut audio)
                    .map(|()| None),
            ),
        };
        assert_eq!(error, ParticleBirthSourceError { producer });
        assert_eq!(error.producer.source_read_address(), address);
        assert_eq!(particles.free_count(), 399);
        assert_eq!(particles.allocation_marker(), Some(399));
        assert_eq!(particles.occupant(399), None);
        assert_eq!(payload(&particles, ParticleId::Pool(399)), before);
        assert!(particles.pending(1).unwrap().is_empty());
        let mut expected_rng = 0x1020_3040;
        if producer == ParticleBirthProducer::Trail {
            name_entry_rng_next(&mut expected_rng);
        }
        if producer == ParticleBirthProducer::FallingDrop {
            for _ in 0..3 {
                name_entry_rng_next(&mut expected_rng);
            }
        }
        // Position, unread velocity and jitter draws, after the sound.
        let sparks = producer == ParticleBirthProducer::ImpactSpark;
        if sparks {
            for _ in 0..9 {
                name_entry_rng_next(&mut expected_rng);
            }
        }
        assert_eq!(rng, expected_rng);
        assert_eq!(audio.requests.len(), usize::from(sparks));
    }
}

#[test]
fn invalid_selectors_bypass_both_source_and_allocation_inputs() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = process(None);
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 123;
    let mut audio = Audio::default();
    for selector in [14, 0x8000_0000, u32::MAX] {
        let mut request = request(&process, selector);
        request.inputs.phase = None;
        assert_eq!(
            particles
                .birth_debris(request, executable, &mut rng)
                .unwrap(),
            None
        );
        assert_eq!(
            particles
                .birth_central(request, executable, &mut rng, &mut audio)
                .unwrap(),
            None
        );
        assert_eq!(
            particles
                .birth_trail(request, executable, &mut rng)
                .unwrap(),
            None
        );
        assert_eq!(
            particles
                .birth_splash(request, executable, &mut rng)
                .unwrap(),
            None
        );
    }
    assert_eq!(particles.free_count(), 400);
    assert_eq!(particles.allocation_marker(), None);
    assert_eq!(rng, 123);
    assert!(audio.requests.is_empty());
}

#[test]
fn allocation_phase_precedes_source_and_scratch_does_not_construct_on_missing_source() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = process(None);
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 123;
    let mut birth = request(&process, 6);
    birth.inputs.phase = None;
    assert!(matches!(
        particles.birth_trail(birth, executable, &mut rng),
        Err(ParticleBirthError::Particle(ParticleError::MissingPhase))
    ));
    assert_eq!(particles.free_count(), 400);
    birth.inputs.phase = Some(3);
    let before = payload(&particles, ParticleId::Scratch);
    assert_eq!(
        source_stop(particles.birth_trail(birth, executable, &mut rng)).producer,
        ParticleBirthProducer::Trail
    );
    assert_eq!(payload(&particles, ParticleId::Scratch), before);
    assert_eq!(particles.free_count(), 400);
    assert_eq!(particles.allocation_marker(), None);
    birth.inputs.phase = None;
    birth.inputs.network_active = true;
    source_stop(particles.birth_trail(birth, executable, &mut rng));
    assert_eq!(particles.free_count(), 399);
    assert_eq!(particles.allocation_marker(), Some(399));
    assert_eq!(rng, 123);
}

#[test]
fn source_is_borrowed_again_after_live_process_changes_and_every_arm_copies_it() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut process = process(Some(0x1234_5678));
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 123;
    let mut audio = Audio::default();
    for (index, producer) in [
        ParticleBirthProducer::Central,
        ParticleBirthProducer::Debris,
        ParticleBirthProducer::Trail,
        ParticleBirthProducer::Trail12,
        ParticleBirthProducer::Trail13,
        ParticleBirthProducer::Splash,
        ParticleBirthProducer::Impact,
        ParticleBirthProducer::ImpactSpark,
    ]
    .into_iter()
    .enumerate()
    {
        let source = 0x1234_5678_u32.wrapping_add(index as u32);
        process.process_null_handle = Some(ActorHandle::from_raw(source));
        let id = match producer {
            ParticleBirthProducer::Central => particles
                .birth_central(request(&process, 0), executable, &mut rng, &mut audio)
                .unwrap(),
            ParticleBirthProducer::Debris => particles
                .birth_debris(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::Trail => particles
                .birth_trail(request(&process, 6), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::Trail12 => particles
                .birth_trail(request(&process, 12), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::Trail13 => particles
                .birth_trail(request(&process, 13), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::Splash => particles
                .birth_splash(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::Impact => particles
                .birth_impact(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::GroundImpact => particles
                .birth_material_sprite(request(&process, 0), true, executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::FallingDrop => particles
                .birth_falling_drop(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::MaterialDust => particles
                .birth_material_dust(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::ShortSmoke => particles
                .birth_short_smoke(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::PickupFlash => particles
                .birth_pickup_flash(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::TerrainFlash => particles
                .birth_terrain_flash(request(&process, 0), executable, &mut rng)
                .unwrap(),
            ParticleBirthProducer::ImpactSpark => {
                particles
                    .birth_impact_sparks(sparks(&process), executable, &mut rng, &mut audio)
                    .unwrap();
                particles.pending(1).unwrap().front().copied()
            }
        }
        .unwrap();
        assert_eq!(
            particles.record(id).unwrap().read_dword(0x14).unwrap(),
            source
        );
    }
}

#[test]
fn central_sound_runs_before_source_and_audio_failure_bypasses_allocation() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = process(None);
    for fail in [false, true] {
        let mut particles = ProcessParticleRuntime::default();
        let mut rng = 123;
        let mut audio = Audio {
            fail,
            ..Audio::default()
        };
        let result =
            particles.birth_central(request(&process, 1), executable, &mut rng, &mut audio);
        if fail {
            assert!(matches!(
                result,
                Err(ParticleBirthError::Audio("required sound"))
            ));
            assert_eq!(particles.free_count(), 400);
        } else {
            assert_eq!(source_stop(result).producer, ParticleBirthProducer::Central);
            assert_eq!(particles.free_count(), 399);
        }
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].sound, 6);
        assert_eq!(audio.requests[0].position, [65530, 1000, 9]);
        assert_eq!(rng, 123);
    }
}

#[test]
fn burst_source_stop_keeps_vector_cursor_before_debris_and_skips_central() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = process(None);
    let mut particles = ProcessParticleRuntime::default();
    particles.vector_cursor = 99;
    let mut rng = 123;
    let mut audio = Audio::default();
    let result = particles.birth_burst(
        ParticleBurstRequest {
            burst: ParticleBurst {
                debris_count: 2,
                position: [100, 1000, 200],
                placement_radius: 20,
                central_type: 1,
                debris_types: [0, 1],
                speed: 8,
                flags: 0,
            },
            load_factor: 65536,
            inputs: request(&process, 0).inputs,
        },
        executable,
        &mut rng,
        &mut audio,
    );
    assert!(matches!(
        result,
        Err(ParticleBirthError::Source(ParticleBirthSourceError {
            producer: ParticleBirthProducer::Debris
        }))
    ));
    assert_eq!(particles.vector_cursor, 100);
    assert_eq!(particles.free_count(), 399);
    assert!(particles.pending(1).unwrap().is_empty());
    assert!(audio.requests.is_empty());
    assert_eq!(rng, 123);
}

#[test]
fn reached_reclaim_survives_source_stop_with_reused_payload_unconstructed() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = process(None);
    let mut particles = ProcessParticleRuntime::default();
    for _ in 0..400 {
        let id = particles
            .allocate(ParticleAllocationRequest {
                payload_bytes: 0x34,
                last_reclaim_category: 1,
                network_active: false,
                phase: Some(4),
            })
            .unwrap();
        particles
            .construct(
                id,
                ParticleConstruction {
                    category: 0,
                    position: [1, 2, 3].into(),
                    velocity: Some([4, 5, 6].into()),
                    source_handle: 7,
                    flags: 0x80,
                },
            )
            .unwrap();
    }
    particles.transfer_pending();
    let tail = *particles.active(0).unwrap().back().unwrap();
    let before = payload(&particles, tail);
    let mut rng = 123;
    source_stop(particles.birth_trail(request(&process, 6), executable, &mut rng));
    let ParticleId::Pool(slot) = tail else {
        panic!("actual pool tail")
    };
    assert_eq!(particles.free_count(), 0);
    assert_eq!(particles.allocation_marker(), Some(slot));
    assert_eq!(particles.occupant(usize::from(slot)), None);
    assert_eq!(particles.active(0).unwrap().len(), 399);
    assert!(!particles.active(0).unwrap().contains(&tail));
    assert_eq!(payload(&particles, tail), before);
    assert!(particles.pending(1).unwrap().is_empty());
    assert_eq!(rng, 123);
}
