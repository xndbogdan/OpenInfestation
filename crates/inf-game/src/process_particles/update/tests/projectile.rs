use super::*;
use serde_json::{json, Value};

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

fn allocate(particles: &mut ProcessParticleRuntime, input: &Value, vtable: u32) -> ParticleId {
    let id = particles
        .allocate(ParticleAllocationRequest {
            payload_bytes: 60,
            last_reclaim_category: 1,
            network_active: false,
            phase: Some(4),
        })
        .unwrap();
    particles
        .construct(
            id,
            ParticleConstruction {
                category: 1,
                position: words(&input["position"]).into(),
                velocity: Some(words(&input["velocity"]).into()),
                source_handle: 12,
                flags: input["flags"].as_u64().unwrap() as u32,
            },
        )
        .unwrap();
    let record = particles.record_mut(id).unwrap();
    record.write_dword(0, vtable).unwrap();
    record
        .write_word(0x12, input["flags"].as_u64().unwrap() as u16)
        .unwrap();
    record
        .write_word(0x10, input["lifetime"].as_u64().unwrap() as u16)
        .unwrap();
    record.write_dword(0x28, 5480120).unwrap();
    record.write_dword(0x2c, 128).unwrap();
    id
}

fn output(particles: &ProcessParticleRuntime, id: ParticleId) -> Value {
    let record = particles.record(id).unwrap();
    let velocity: [u16; 3] = std::array::from_fn(|axis| record.read_word(0x0a + axis * 2).unwrap());
    json!({"position": record.position().unwrap(), "velocity": velocity,
        "lifetime": record.read_word(0x10).unwrap(), "flags": record.read_word(0x12).unwrap(),
        "animation": record.read_dword(0x28).unwrap(), "scale": record.read_dword(0x2c).unwrap()})
}

#[test]
fn projectile_update_matches_native_signed_intervals_lifetime_and_word_wraps() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_projectile_update_native.json"
    ))
    .unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let mut particles = ProcessParticleRuntime::default();
        let id = allocate(&mut particles, input, 0x0052_6efc);
        particles
            .update_projectile(id, input["delta"].as_u64().unwrap() as i32)
            .unwrap();
        assert_eq!(output(&particles, id), case["output"], "case{index}");
    }
}

#[test]
fn impact_bubble_update_matches_native_water_motion_and_splash_prefixes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_impact_update_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let sine_bytes = executable.mapped(0x0053_df00, 8192).unwrap().bytes;
    let sine: [u16; 4096] = std::array::from_fn(|index| {
        u16::from_le_bytes(sine_bytes[index * 2..index * 2 + 2].try_into().unwrap())
    });
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let mut terrain_case = receipt().cases.remove(0);
        terrain_case.plane = Some(input["plane"].as_i64().unwrap() as i32);
        terrain_case.height = Some(input["height"].as_i64().unwrap() as i8);
        let mut terrain = TerrainRuntime::new(
            &source(&terrain_case),
            TerrainSampling {
                spacing: if input["spacing"] == 256 {
                    TerrainSpacing::Units256
                } else {
                    TerrainSpacing::Units128
                },
                height_scale: if input["height_shift"] == 4 {
                    TerrainHeightScale::Units16
                } else {
                    TerrainHeightScale::Units32
                },
            },
        );
        assert_eq!(
            terrain.plane_shift(),
            input["coordinate_shift"].as_u64().unwrap() as u32
        );
        let normal = case["normal_return"].as_bool().unwrap();
        let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: normal.then_some(crate::object_runtime::ActorHandle::from_raw(12)),
        };
        let mut particles = ProcessParticleRuntime::default();
        let id = allocate(&mut particles, input, 0x0052_8814);
        let mut request = ParticleUpdateRequest {
            delta_microseconds: input["delta"].as_u64().unwrap() as i32,
            load_factor: 65536,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &mut process,
            },
            terrain: Some(ParticleTerrainContext {
                terrain: &mut terrain,
                sine_quarter: &sine,
                wave_clock: input["clock"].as_u64().unwrap() as u32,
                world_flags: input["world_flags"].as_u64().unwrap() as u32,
                impact_selectors: [1, 2, 3, 4, 6],
            }),
        };
        let result: Result<bool, ParticleUpdateError<&str>> =
            particles.update_impact_bubble(id, &mut request, executable, &mut 1);
        assert_eq!(result.is_ok(), normal, "case{index}: {result:?}");
        if normal {
            assert_eq!(result.unwrap(), case["return_eax"] == 1, "case{index}");
        }
        assert_eq!(output(&particles, id), case["output"], "case{index}");
        if normal && !case["splash_calls"].as_array().unwrap().is_empty() {
            let born = ParticleId::Pool(398);
            assert_eq!(
                particles.record(born).unwrap().read_dword(0).unwrap(),
                0x0052_87b4
            );
            assert_eq!(
                particles.record(born).unwrap().position().unwrap(),
                words(&case["splash_calls"][0]["position"])
            );
        }
    }
}

#[test]
fn projectile_manager_continues_after_collision_stop_without_replaying_particle_prefix() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let input =
        json!({"position": [10,2000,30], "velocity": [1000,0,0], "flags": 0x88, "lifetime": 1000});
    let healthy = allocate(&mut particles, &input, 0x0052_8914);
    let stopped = allocate(&mut particles, &input, 0x0052_6efc);
    //Unlike the isolated update receipts, the manager also reads animation
    //radius. Use factory466710 selector2's actual animation, not its packet.
    particles
        .record_mut(stopped)
        .unwrap()
        .write_dword(0x28, 0x0053_9d28)
        .unwrap();
    let mut process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut effects = Effects::default();
    let mut rng = 1;
    let report = particles.tick_particles(
        no_terrain_request(&mut process, 20_000),
        executable,
        &mut rng,
        &mut effects,
    );
    assert_eq!(report.updated, 1);
    assert_eq!(report.omissions.len(), 1);
    assert_eq!(report.omissions[0].id, stopped);
    assert!(matches!(
        report.omissions[0].error,
        ParticleUpdateError::Collision { .. }
    ));
    assert_eq!(
        particles.stopped_phase(stopped),
        Some(ParticleStopPhase::Collision)
    );
    let prefix = output(&particles, stopped);
    assert_eq!(
        particles.record(stopped).unwrap().read_word(0x10).unwrap(),
        981
    );
    let report = particles.tick_particles(
        no_terrain_request(&mut process, 20_000),
        executable,
        &mut rng,
        &mut effects,
    );
    assert_eq!(report.stopped, 1);
    assert_eq!(report.updated, 1);
    assert!(report.omissions.is_empty());
    assert_eq!(output(&particles, stopped), prefix);
    assert_eq!(
        particles.record(healthy).unwrap().read_word(0x10).unwrap(),
        962
    );
}
