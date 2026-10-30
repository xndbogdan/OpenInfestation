use super::*;
use serde_json::Value;

fn u(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| u(&value[axis]) as u16)
}

fn slots(list: &std::collections::VecDeque<ParticleId>) -> Vec<u32> {
    list.iter()
        .map(|id| match id {
            ParticleId::Pool(slot) => u32::from(*slot),
            ParticleId::Scratch => panic!("scratch list publication"),
        })
        .collect()
}

/// Pool cells keep the supplied A5 heap fill where Rust reports unwritten
/// bytes; the native scratch record is zero there.
fn compare(
    particles: &ProcessParticleRuntime,
    id: ParticleId,
    layout: &[(usize, usize)],
    expected: &Value,
    context: &str,
) {
    let record = particles.record(id).unwrap();
    for ((offset, width), expected) in layout.iter().zip(expected.as_array().unwrap()) {
        let actual = match width {
            2 => record.read_word(*offset).map(u32::from),
            4 => record.read_dword(*offset),
            _ => unreachable!(),
        };
        let expected = u(expected);
        match actual {
            Ok(value) => assert_eq!(value, expected, "{context} {id:?} +{offset:02X}"),
            Err(ParticleError::Unwritten { .. }) => {
                let fill = match (id, width) {
                    (ParticleId::Scratch, _) => 0,
                    (_, 2) => 0xa5a5,
                    _ => 0xa5a5_a5a5,
                };
                assert_eq!(expected, fill, "{context} {id:?} unwritten +{offset:02X}");
            }
            Err(error) => panic!("{context}: {error}"),
        }
    }
}

fn compare_pool(
    particles: &ProcessParticleRuntime,
    layout: &[(usize, usize)],
    records: &Value,
    context: &str,
) {
    let records = records.as_array().unwrap();
    let occupied = (0..400)
        .filter(|slot| particles.occupant(*slot).is_some())
        .count();
    assert_eq!(occupied, records.len(), "{context} occupied cells");
    for record in records {
        let slot = u(&record["slot"]) as u16;
        let id = ParticleId::Pool(slot);
        assert_eq!(particles.occupant(usize::from(slot)), Some(id), "{context}");
        compare(particles, id, layout, &record["fields"], context);
    }
}

#[derive(Default)]
struct SparkAudio {
    requests: Vec<ParticleSoundRequest>,
}
impl ParticleBirthAudio for SparkAudio {
    type Error = &'static str;
    fn explosion_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.requests.push(request);
        Ok(())
    }
}

fn birth(
    case: &Value,
    process: &crate::process_particles::factories::ParticleFactoryProcessState,
    executable: FrontendExecutable<'_>,
    rng: &mut u32,
) -> (ProcessParticleRuntime, SparkAudio) {
    let mut particles = ProcessParticleRuntime::default();
    let mut audio = SparkAudio::default();
    particles
        .birth_impact_sparks(
            ParticleImpactSparksRequest {
                position: words(&case["position"]),
                strength: case["strength"].as_i64().unwrap() as i32,
                inputs: ParticleBirthInputs {
                    network_active: case["network_active"].as_bool().unwrap_or(false),
                    phase: Some(case["phase"].as_u64().map_or(4, |phase| phase as u32)),
                    process,
                },
            },
            executable,
            rng,
            &mut audio,
        )
        .unwrap();
    (particles, audio)
}

/// 0048AB10's sound and sparks, the real 00464B30 walk over them, and the
/// whole 512-angle space of 0048ACE0 run on the unchanged executable.
#[test]
fn impact_sparks_match_native_births_walk_and_update_sweep() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_impact_sparks_native.json"
    ))
    .unwrap();
    let bytes = executable_bytes();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let layout: Vec<(usize, usize)> = fixture["provenance"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| (u(&pair[0]) as usize, u(&pair[1]) as usize))
        .collect();
    let vtable = |address| -> Vec<u32> {
        executable
            .mapped(address, 32)
            .unwrap()
            .bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect()
    };
    let (spark, debris) = (vtable(0x0052_8954), vtable(0x0052_8914));
    assert_eq!(serde_json::json!(spark), fixture["vtable"]);
    // Only the update slot differs from the central/debris class.
    assert_eq!(spark[1], 0x0048_ace0);
    assert_eq!(debris[1], 0x0048_a390);
    assert_eq!((spark[0], &spark[2..]), (debris[0], &debris[2..]));

    let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
    };
    for case in fixture["births"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut rng = u(&case["rng"]);
        let (particles, audio) = birth(case, &process, executable, &mut rng);
        assert_eq!(rng, u(&case["final_rng"]), "{name} RNG");
        let sounds: Vec<_> = case["sounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sound| ParticleSoundRequest {
                sound: u(&sound["sound"]),
                position: words(&sound["position"]),
                volume: u(&sound["volume"]),
                pitch: u(&sound["pitch"]),
                flags: u(&sound["flags"]),
            })
            .collect();
        assert_eq!(audio.requests, sounds, "{name} enqueue");
        assert_eq!(
            audio.requests.len(),
            case["audio_error_cleanup_count"].as_u64().unwrap() as usize,
            "{name} nullable error cleanup"
        );
        assert_eq!(
            serde_json::json!(slots(particles.pending(1).unwrap())),
            case["pending"],
            "{name} pending"
        );
        compare_pool(&particles, &layout, &case["records"], name);
        if !case["scratch"].as_array().unwrap().is_empty() {
            compare(
                &particles,
                ParticleId::Scratch,
                &layout,
                &case["scratch"],
                name,
            );
        }
    }

    for case in fixture["ticks"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut rng = u(&case["rng"]);
        let (mut particles, _) = birth(case, &process, executable, &mut rng);
        compare_pool(&particles, &layout, &case["born"], name);
        particles.transfer_pending();
        let deltas = case["deltas"].as_array().unwrap();
        let frames = case["frames"].as_array().unwrap();
        for (index, (delta, frame)) in deltas.iter().zip(frames).enumerate() {
            let context = format!("{name} frame {index}");
            let report = particles.tick_particles(
                no_terrain_request(&mut process, delta.as_i64().unwrap() as i32),
                executable,
                &mut rng,
                &mut Effects::default(),
            );
            assert!(
                report.omissions.is_empty(),
                "{context}: {:?}",
                report.omissions
            );
            assert_eq!(rng, u(&frame["rng"]), "{context} RNG");
            assert_eq!(
                particles.free_count(),
                u(&frame["free_count"]) as usize,
                "{context} free cells"
            );
            assert_eq!(
                serde_json::json!(slots(particles.active(1).unwrap())),
                frame["active"],
                "{context} active"
            );
            compare_pool(&particles, &layout, &frame["records"], &context);
        }
    }

    let sweep = fixture["sweep"].as_array().unwrap();
    for row in sweep {
        let mut particles = ProcessParticleRuntime::default();
        let id = ParticleId::Scratch;
        let record = particles.record_mut(id).unwrap();
        record.write_dword(0, 0x0052_8954).unwrap();
        for (axis, (position, velocity)) in words(&row["position"])
            .into_iter()
            .zip(words(&row["velocity"]))
            .enumerate()
        {
            record.write_word(4 + axis * 2, position).unwrap();
            record.write_word(0x0a + axis * 2, velocity).unwrap();
        }
        let lifetime = u(&row["lifetime"]) as u16;
        record.write_word(0x10, lifetime).unwrap();
        let mut rng = 0x1234_5678;
        let result = particles
            .update_particle(
                id,
                &mut no_terrain_request(&mut process, u(&row["delta"]) as i32),
                executable,
                &mut rng,
                &mut Effects::default(),
            )
            .unwrap();
        assert_eq!(u32::from(result), u(&row["result"]), "{row}");
        let record = particles.record(id).unwrap();
        assert_eq!(
            record.position().unwrap(),
            words(&row["after_position"]),
            "{row}"
        );
        let velocity: [u16; 3] =
            std::array::from_fn(|axis| record.read_word(0x0a + axis * 2).unwrap());
        assert_eq!(velocity, words(&row["after_velocity"]), "{row}");
        assert_eq!(record.read_word(0x10).unwrap(), lifetime, "{row}");
        assert_eq!(rng, 0x1234_5678, "{row}");
    }
    assert_eq!(fixture["births"].as_array().unwrap().len(), 11);
    assert_eq!(fixture["ticks"].as_array().unwrap().len(), 5);
    assert_eq!(sweep.len(), 517);
}
