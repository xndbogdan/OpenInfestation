use super::*;
use crate::object_runtime::ActorHandle;
use crate::process_particles::{factories::ParticleFactoryProcessState, ParticleId};
use serde_json::{json, Value};

#[test]
fn pickup_particle_births_match_native_records_rng_and_pool_custody() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/pickup_particle_birth_native.json"
    ))
    .unwrap();
    let bytes = super::tests::executable_bytes();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0x12345678)),
    };
    for row in fixture["cases"].as_array().unwrap() {
        let u = |key: &str| row[key].as_u64().unwrap() as u32;
        let words = |key: &str| std::array::from_fn(|i| row[key][i].as_u64().unwrap() as u16);
        let mut particles = ProcessParticleRuntime::default();
        particles.vector_cursor = u("cursor");
        let mut rng = u("rng");
        let request = ParticleBirthRequest {
            particle_type: u("selector"),
            position: words("position"),
            velocity: Some(words("velocity")),
            flags: u("flags"),
            inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(u("phase")),
                process: &process,
            },
        };
        let kind = row["kind"].as_str().unwrap();
        let result = match kind {
            "drop" => particles.birth_falling_drop(request, exe, &mut rng),
            "dust" => particles.birth_material_dust(request, exe, &mut rng),
            "flash" => particles.birth_pickup_flash(request, exe, &mut rng),
            _ => unreachable!(),
        }
        .unwrap();
        let out = &row["output"];
        assert_eq!(
            result.is_some(),
            out["returned"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(
            result == Some(ParticleId::Scratch),
            out["scratch"].as_bool().unwrap(),
            "{row}"
        );
        if let Some(id) = result {
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
                if kind == "flash" && (10..=14).contains(&offset) {
                    // NULL velocity copies no bytes: retain unknown heap or zero BSS.
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
                assert_eq!(
                    actual,
                    out["fields"][index].as_u64().unwrap() as u32,
                    "offset {offset:X}: {row}"
                );
            }
        }
        for category in 0..6 {
            let slots: Vec<_> = particles
                .pending(category)
                .unwrap()
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!("scratch must not be published"),
                })
                .collect();
            assert_eq!(json!(slots), out["pending"][category], "{row}");
        }
        assert_eq!(rng, out["rng"].as_u64().unwrap() as u32, "{row}");
        assert_eq!(
            particles.vector_cursor,
            out["cursor"].as_u64().unwrap() as u32,
            "{row}"
        );
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 31);
}
