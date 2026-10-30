use super::*;
use serde_json::json;

#[test]
fn combat_material_sprite_birth_matches_all_native_selector_and_shadow_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_ground_birth_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0x12345678)),
    };
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 17);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let expected = &case["output"];
        let mut particles = ProcessParticleRuntime::default();
        let mut rng = input["rng"].as_u64().unwrap() as u32;
        let id = particles
            .birth_material_sprite(
                ParticleBirthRequest {
                    particle_type: input["selector"].as_u64().unwrap() as u32,
                    position: words(&input["position"]),
                    velocity: Some(words(&input["velocity"])),
                    flags: input["flags"].as_u64().unwrap() as u32,
                    inputs: ParticleBirthInputs {
                        network_active: false,
                        phase: Some(input["phase"].as_u64().unwrap() as u32),
                        process: &process,
                    },
                },
                input["enabled"].as_u64().unwrap() as u8 != 0,
                executable,
                &mut rng,
            )
            .unwrap();
        assert_eq!(
            id.is_some(),
            expected["returned"].as_bool().unwrap(),
            "case{index}"
        );
        assert_eq!(
            id == Some(ParticleId::Scratch),
            expected["scratch"].as_bool().unwrap(),
            "case{index}"
        );
        if let Some(id) = id {
            let record = particles.record(id).unwrap();
            let fields = [
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
            .map(|(offset, width)| {
                if width == 4 {
                    record.read_dword(offset).unwrap()
                } else {
                    u32::from(record.read_word(offset).unwrap())
                }
            });
            assert_eq!(json!(fields), expected["fields"], "case{index}");
        }
        let pending: Vec<_> = particles
            .pending(1)
            .unwrap()
            .iter()
            .map(|id| match id {
                ParticleId::Pool(slot) => *slot,
                ParticleId::Scratch => panic!("scratch published"),
            })
            .collect();
        assert_eq!(json!(pending), expected["pending"], "case{index}");
        assert_eq!(json!(rng), expected["rng"], "case{index}");
    }
}

#[test]
fn combat_ground_burst_matches_native_material_vectors_cursor_and_count() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_ground_producer_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut covered = 0;
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        //A supplied third-factory interruption has no equivalent boundary in
        //this concrete factory. First-source failure proves cursor retention.
        if input["stop_at"] == 3 {
            continue;
        }
        covered += 1;
        let stop = input["stop_at"] == 1;
        let process = ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: (!stop).then_some(ActorHandle::from_raw(0x12345678)),
        };
        let mut particles = ProcessParticleRuntime::default();
        particles.vector_cursor = input["cursor"].as_u64().unwrap() as u32;
        let mut rng = 0x12345678;
        let result = particles.birth_ground_impact(
            input["selector"].as_u64().unwrap() as u32,
            words(&input["position"]),
            input["amount"].as_u64().unwrap() as i32,
            input["load_factor"].as_u64().unwrap() as i32,
            ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &process,
            },
            executable,
            &mut rng,
        );
        assert_eq!(result.is_err(), stop, "case{index}: {result:?}");
        assert_eq!(
            json!(particles.vector_cursor),
            case["cursor"],
            "case{index}"
        );
        if stop {
            assert_eq!(rng, 0x12345678, "failure precedes birth RNG");
            continue;
        }
        let admitted: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["selector"] != 5)
            .collect();
        let actual = particles.pending(1).unwrap();
        assert_eq!(actual.len(), admitted.len(), "case{index}");
        // Construction inserts each birth at the pending-list front; the
        // native producer receipt records chronological factory calls.
        for (id, event) in actual.iter().rev().zip(admitted) {
            let record = particles.record(*id).unwrap();
            let velocity: [u16; 3] = std::array::from_fn(|i| record.read_word(10 + 2 * i).unwrap());
            assert_eq!(
                json!(record.position().unwrap()),
                event["position"],
                "case{index}"
            );
            assert_eq!(json!(velocity), event["velocity"], "case{index}");
            assert_eq!(record.read_dword(0).unwrap(), 0x5287f4);
            assert_eq!(record.read_dword(0x30).unwrap(), 256);
        }
    }
    assert_eq!(covered, 13);
}
