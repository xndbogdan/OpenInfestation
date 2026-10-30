use super::*;
use serde_json::{json, Value};

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|i| value[i].as_i64().unwrap() as u16)
}
fn allocate(particles: &mut ProcessParticleRuntime, input: &Value) -> ParticleId {
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
                category: 1,
                position: words(&input["position"]).into(),
                velocity: Some(words(&input["velocity"]).into()),
                source_handle: 12,
                flags: input["flags"].as_u64().unwrap() as u32,
            },
        )
        .unwrap();
    let record = particles.record_mut(id).unwrap();
    record.write_dword(0, 0x5287f4).unwrap();
    record
        .write_word(0x10, input["lifetime"].as_u64().unwrap() as u16)
        .unwrap();
    record
        .write_word(0x12, input["flags"].as_u64().unwrap() as u16)
        .unwrap();
    id
}
fn snapshot(particles: &ProcessParticleRuntime, id: ParticleId, rng: u32) -> Value {
    let record = particles.record(id).unwrap();
    let velocity: [u16; 3] = std::array::from_fn(|i| record.read_word(10 + 2 * i).unwrap());
    json!({"position":record.position().unwrap(),"velocity":velocity,"lifetime":record.read_word(0x10).unwrap(),"flags":record.read_word(0x12).unwrap(),"rng":rng})
}

#[test]
fn combat_material_sprite_update_matches_native_gravity_and_wrapping_movement() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_ground_update_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 11);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let mut particles = ProcessParticleRuntime::default();
        let id = allocate(&mut particles, input);
        particles.update_globals.gravity = input["gravity"].as_u64().unwrap() as i32;
        let mut process =
            crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
        let mut request = no_terrain_request(&mut process, input["delta"].as_u64().unwrap() as i32);
        let mut rng = input["rng"].as_u64().unwrap() as u32;
        let result = particles
            .update_particle(
                id,
                &mut request,
                executable,
                &mut rng,
                &mut Effects::default(),
            )
            .unwrap();
        assert_eq!(result, case["return_eax"] == 1, "case{index}");
        assert_eq!(snapshot(&particles, id, rng), case["output"], "case{index}");
    }
}

#[test]
fn combat_material_sprite_bounce_and_water_effect_order_match_native_prefixes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_ground_contact_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 14);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let mut particles = ProcessParticleRuntime::default();
        let id = allocate(&mut particles, input);
        let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: (input["stop_at"] != "splash")
                .then_some(crate::object_runtime::ActorHandle::from_raw(12)),
        };
        let mut request = no_terrain_request(&mut process, 0);
        let mut rng = input["rng"].as_u64().unwrap() as u32;
        let mut effects = Effects {
            fail: input["stop_at"] == "audio",
            ..Effects::default()
        };
        let result = particles.material_sprite_contact(
            id,
            super::super::collision::ParticleTerrainContact {
                selector: input["selector"].as_u64().unwrap() as u32,
                height: input["height"].as_u64().unwrap() as i32,
            },
            input["radius"].as_u64().unwrap() as i32,
            &mut request,
            executable,
            &mut rng,
            &mut effects,
        );
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case{index}: {result:?}"
        );
        if let Ok(dead) = result {
            assert_eq!(dead, case["return_al"] == 1, "case{index}");
        }
        //Native prefixes supply a splash normal-return without its RNG.
        //The actual splash birth consumes one draw; compare its caller state
        //with the boundary seed and then independently check the real draw.
        let expected_rng = case["output"]["rng"].as_u64().unwrap() as u32;
        let splash = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "splash");
        let normal = case["normal_return"].as_bool().unwrap();
        if splash.is_some() && normal {
            let mut after = expected_rng;
            crate::name_entry::name_entry_rng_next(&mut after);
            assert_eq!(rng, after, "case{index}");
            let born = ParticleId::Pool(398);
            assert_eq!(
                particles.record(born).unwrap().read_dword(0).unwrap(),
                0x5287b4
            );
            assert_eq!(
                json!(particles.record(born).unwrap().position().unwrap()),
                splash.unwrap()["position"]
            );
        } else {
            assert_eq!(rng, expected_rng, "case{index}");
        }
        assert_eq!(
            snapshot(&particles, id, expected_rng),
            case["output"],
            "case{index}"
        );
        let expected:Vec<_>=case["events"].as_array().unwrap().iter().filter(|event|event["kind"]=="audio").map(|event|json!({"sound":event["sound"],"position":event["position"],"volume":event["args"][0],"pitch":event["args"][1],"flags":event["args"][2]})).collect();
        let actual:Vec<_>=effects.impacts.iter().map(|sound|json!({"sound":sound.sound,"position":sound.position,"volume":sound.volume,"pitch":sound.pitch,"flags":sound.flags})).collect();
        assert_eq!(actual, expected, "case{index}");
    }
}
