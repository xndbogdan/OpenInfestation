use super::*;
use serde_json::{json, Value};

#[test]
fn process_particles_short_smoke_matches_native_birth_counter_and_normal_release() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/short_smoke_native.json"
    ))
    .unwrap();
    let bytes = executable_bytes();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let u = |v: &Value| v.as_u64().unwrap() as u32;
    let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
    };
    let mapped = executable.mapped(0x528874, 32).unwrap();
    let vtable: Vec<_> = mapped
        .bytes
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(json!(vtable), fixture["vtable"]);
    assert_eq!(
        vtable,
        vec![0x466d50, 0x48b0f0, 0x466cd0, 0x402720, 0x466190, 0x4661a0, 0x4661a0, 0x401310]
    );
    for row in fixture["births"].as_array().unwrap() {
        let mut particles = ProcessParticleRuntime::default();
        particles.vector_cursor = u(&row["cursor"]);
        let mut rng = u(&row["rng"]);
        let request = ParticleBirthRequest {
            particle_type: u(&row["selector"]),
            position: std::array::from_fn(|i| u(&row["position"][i]) as u16),
            velocity: Some([7, 8, 9]), // Native ignores this pointer argument.
            flags: u(&row["flags"]),
            inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(u(&row["phase"])),
                process: &process,
            },
        };
        let id = particles
            .birth_short_smoke(request, executable, &mut rng)
            .unwrap();
        let out = &row["output"];
        assert_eq!(id.is_some(), out["returned"].as_bool().unwrap(), "{row}");
        assert_eq!(
            id == Some(ParticleId::Scratch),
            out["scratch"].as_bool().unwrap()
        );
        if let Some(id) = id {
            let record = particles.record(id).unwrap();
            for (index, (offset, width)) in [
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
            ]
            .into_iter()
            .enumerate()
            {
                if (10..=14).contains(&offset) {
                    if id != ParticleId::Scratch {
                        assert!(record.read_word(offset).is_err());
                    }
                    continue;
                }
                let actual = if width == 4 {
                    record.read_dword(offset).unwrap()
                } else {
                    u32::from(record.read_word(offset).unwrap())
                };
                assert_eq!(actual, u(&out["fields"][index]), "offset {offset:X}: {row}");
            }
            assert_eq!(record.read_dword(0x34).unwrap(), u(&out["counter"]));
        }
        for category in 0..6 {
            let list: Vec<_> = particles
                .pending(category)
                .unwrap()
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!("scratch list publication"),
                })
                .collect();
            assert_eq!(json!(list), out["pending"][category]);
        }
        assert_eq!(rng, u(&out["rng"]));
        assert_eq!(particles.vector_cursor, u(&out["cursor"]));
    }
    for row in fixture["updates"].as_array().unwrap() {
        let mut particles = ProcessParticleRuntime::default();
        let id = ParticleId::Scratch;
        particles
            .record_mut(id)
            .unwrap()
            .write_dword(0, 0x528874)
            .unwrap();
        particles
            .record_mut(id)
            .unwrap()
            .write_dword(0x34, u(&row["counter"]))
            .unwrap();
        let mut rng = 123;
        let result = particles
            .update_particle(
                id,
                &mut no_terrain_request(&mut process, u(&row["delta"]) as i32),
                executable,
                &mut rng,
                &mut Effects::default(),
            )
            .unwrap();
        assert_eq!(result, u(&row["result"]) != 0);
        assert_eq!(
            particles.record(id).unwrap().read_dword(0x34).unwrap(),
            u(&row["after"])
        );
        assert_eq!(rng, 123);
    }
    // Actual manager, pending occupancy and normal scalar deletion. Neither
    // frame needs terrain or constructor-unwritten velocity bytes.
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 7;
    let id = particles
        .birth_short_smoke(
            ParticleBirthRequest {
                particle_type: 0,
                position: [0; 3],
                velocity: None,
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &process,
                },
            },
            executable,
            &mut rng,
        )
        .unwrap()
        .unwrap();
    let first = particles.tick_particles(
        no_terrain_request(&mut process, 10000),
        executable,
        &mut rng,
        &mut Effects::default(),
    );
    assert!(first.omissions.is_empty());
    assert_eq!(first.destroyed, 0);
    assert_eq!(particles.record(id).unwrap().read_dword(0x34).unwrap(), 0);
    let second = particles.tick_particles(
        no_terrain_request(&mut process, 10000),
        executable,
        &mut rng,
        &mut Effects::default(),
    );
    assert!(second.omissions.is_empty());
    assert_eq!(second.destroyed, 1);
    assert_eq!(particles.free_count(), 400);
    assert_eq!(fixture["births"].as_array().unwrap().len(), 9);
    assert_eq!(fixture["updates"].as_array().unwrap().len(), 7);
}
