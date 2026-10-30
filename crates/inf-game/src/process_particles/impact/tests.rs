use super::*;
use crate::{
    object_runtime::ActorHandle,
    process_particles::{births::ParticleBirthInputs, factories::ParticleFactoryProcessState},
};
use serde_json::Value;

mod ground;

#[test]
fn material_sprite_uses_native_shared_deletion_and_releases_its_pool_cell() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    assert_eq!(
        executable.mapped(0x0052_87f4, 4).unwrap().bytes,
        &0x0046_6d50_u32.to_le_bytes()
    );
    let process = ParticleFactoryProcessState::native_startup();
    let mut particles = ProcessParticleRuntime::default();
    let id = particles
        .birth_material_sprite(
            ParticleBirthRequest {
                particle_type: 0,
                position: [100, 200, 300],
                velocity: Some([1, 2, 3]),
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &process,
                },
            },
            true,
            executable,
            &mut 1,
        )
        .unwrap()
        .unwrap();
    particles.transfer_pending();
    particles.destroy(id).unwrap();
    assert_eq!(particles.free_count(), 400);
    assert!(particles.active(1).unwrap().is_empty());
    assert_eq!(
        particles.record(id).unwrap().read_dword(0).unwrap(),
        0x0052_6ed8
    );
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|i| value[i].as_i64().unwrap() as u16)
}

pub(super) fn executable_bytes() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("impact receipts require the original executable")
}

#[test]
fn impact_birth_matches_original_instructions_and_pending_custody() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/particle_impact_birth_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
    };
    let mut checked = 0;
    for case in receipt["cases"].as_array().unwrap() {
        let input = &case["input"];
        let output = &case["output"];
        let mut particles = ProcessParticleRuntime::default();
        particles.vector_cursor = input["cursor"].as_u64().unwrap() as u32;
        let mut rng = input["rng"].as_u64().unwrap() as u32;
        let request = ParticleBirthRequest {
            particle_type: input["selector"].as_u64().unwrap() as u32,
            position: words(&input["position"]),
            velocity: Some(words(&input["velocity"])),
            flags: input["flags"].as_u64().unwrap() as u32,
            inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(input["phase"].as_u64().unwrap() as u32),
                process: &process,
            },
        };
        if input["prior_birth"] == true {
            particles
                .birth_impact(request, executable, &mut rng)
                .unwrap();
            particles.transfer_pending();
        }
        let id = particles
            .birth_impact(request, executable, &mut rng)
            .unwrap();
        assert_eq!(
            id.is_some(),
            output["returned"].as_bool().unwrap(),
            "{input}"
        );
        assert_eq!(
            id == Some(ParticleId::Scratch),
            output["scratch"].as_bool().unwrap()
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
            assert_eq!(serde_json::json!(fields), output["fields"], "{input}");
        }
        for (actual, key) in [
            (particles.pending(1).unwrap(), "pending"),
            (particles.active(1).unwrap(), "active"),
        ] {
            let actual: Vec<_> = actual
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!("scratch is not published"),
                })
                .collect();
            assert_eq!(serde_json::json!(actual), output[key], "{input}");
        }
        assert_eq!(u64::from(rng), output["rng"].as_u64().unwrap());
        assert_eq!(
            u64::from(particles.vector_cursor),
            output["cursor"].as_u64().unwrap()
        );
        checked += 1;
    }
    assert_eq!(checked, 13);
}

#[test]
fn impact_source_stop_preserves_allocator_prefix_and_unsupported_selector_skips_source() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 123;
    let mut request = ParticleBirthRequest {
        particle_type: 1,
        position: [1, 2, 3],
        velocity: Some([4, 5, 6]),
        flags: 0,
        inputs: ParticleBirthInputs {
            network_active: false,
            phase: Some(4),
            process: &process,
        },
    };
    assert_eq!(
        particles
            .birth_impact(request, executable, &mut rng)
            .unwrap(),
        None
    );
    assert_eq!(particles.free_count(), 400);
    request.particle_type = 0;
    assert!(
        matches!(particles.birth_impact(request,executable,&mut rng),Err(ParticleBirthError::Source(error)) if error.producer==ParticleBirthProducer::Impact)
    );
    assert_eq!(particles.free_count(), 399);
    assert_eq!(particles.allocation_marker(), Some(399));
    assert!(particles.pending(1).unwrap().is_empty());
    assert_eq!(rng, 123);
}
