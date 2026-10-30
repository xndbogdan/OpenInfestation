use super::*;
use crate::{Installation, ResidencyRequest};

#[test]
fn projectile_radius_origin_rereads_cell_but_keeps_original_executable_packet() {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let arena = ObjectArena::default();
    let origin = WorldRadiusOrigin::Particle {
        particle: ParticleId::Scratch,
        block_address: 0x539ea8,
    };
    particles
        .record_mut(ParticleId::Scratch)
        .unwrap()
        .write_position([100, 200, 300])
        .unwrap();
    assert_eq!(
        origin.position(&particles, &arena).unwrap(),
        [100, 200, 300]
    );
    assert_eq!(origin.block(executable).unwrap().outer_radius, 600);
    // A nested callback can overwrite the actual cell; captured position
    // pointers observe that write while the original block pointer survives.
    let record = particles.record_mut(ParticleId::Scratch).unwrap();
    record.write_position([400, 0xfffe, 600]).unwrap();
    record.write_dword(0x38, 0x539eb8).unwrap();
    assert_eq!(
        origin.position(&particles, &arena).unwrap(),
        [400, 0xfffe, 600]
    );
    assert_eq!(origin.block(executable).unwrap().outer_radius, 600);
}

#[test]
fn projectile_furniture_damage_rejections_do_not_resolve_absent_h11() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    assert!(resident_placement_table(&residency).is_err());
    let terrain = crate::terrain_runtime::native_probe_fixture_terrain(&serde_json::json!({
        "spacing":128,"height_shift":5,"plane_base":-256000,"plane_shift":8,
        "xz":[0,0],"heights":[0,0,0,0],"alternate":false,
    }));
    let mut resources = ProjectileFurnitureResources::new(&residency, executable);
    let mut queue = FurnitureEffectQueue::default();
    let request = FurnitureDamageRequest {
        xz: [0, 0],
        effect: crate::world_radius_effect::ScaledRadiusEffect {
            strength: 100,
            channel: 9,
            byte_03: Some(0),
            impulse_magnitude: 0,
            impulse_vector: None,
        },
    };
    let mut rng = 7;
    for (network_active, expected) in [
        (
            true,
            crate::world_furniture_effect::FurnitureDamageRejection::NetworkSession,
        ),
        (
            false,
            crate::world_furniture_effect::FurnitureDamageRejection::NoFurniture,
        ),
    ] {
        let outcome = FurnitureDamageRuntime {
            network_active,
            resources: &mut resources,
            queue: &mut queue,
        }
        .apply(&terrain, request, &mut rng)
        .unwrap();
        assert_eq!(
            outcome,
            crate::world_furniture_effect::FurnitureDamageOutcome::Rejected(expected)
        );
        assert!(resources.resolved_table.is_none());
        assert_eq!(rng, 7);
    }
}
