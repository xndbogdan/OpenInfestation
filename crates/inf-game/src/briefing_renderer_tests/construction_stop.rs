use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, ObjectArena,
};

#[test]
fn stopped_construction_map_marker_skips_unknown_position_and_keeps_healthy_pixels() {
    let (assets, tables, resident) = level1_fixture(2);
    let mut images = Vec::new();
    for include_stopped in [false, true] {
        images.push(level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            [640, 480],
            |owner, _, _| {
                *owner.world.arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
                    owner_count: 2,
                    initial_capacity: 8,
                    null_handle: ActorHandle::from_raw(0),
                })
                .unwrap();
                let arena = &mut *owner.world.arena;
                let allocation = |position| ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: position,
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &[0; 4096],
                    },
                };
                let stopped = include_stopped.then(|| {
                    let id = arena.allocate_actor(allocation(None)).unwrap();
                    arena.apply_payload_flags(id, 0x21).unwrap();
                    arena.stop_object_construction(id).unwrap();
                    (id, arena.object(id).unwrap().clone())
                });
                let healthy = arena
                    .allocate_actor(allocation(Some([0x8000, 0, 0x7f00])))
                    .unwrap();
                arena.apply_payload_flags(healthy, 0x21).unwrap();
                let nodes = arena.actor_list().to_vec();
                owner.scene.set_process_rng(Some(0x1234_5678));
                owner
                    .map_markers(
                        BriefingMarkerTile {
                            origin: [20, 20],
                            size: [256, 256],
                        },
                        0,
                        &FrontendProcessState::default(),
                    )
                    .unwrap();
                assert_eq!(owner.scene.stats.direct_sprites, 1);
                assert_eq!(owner.scene.process_rng(), Some(0x1234_5678));
                assert_eq!(owner.world.arena.actor_list(), nodes);
                if let Some((id, before)) = stopped {
                    assert_eq!(owner.world.arena.object(id).unwrap(), &before);
                }
            },
        ));
    }
    assert_eq!(images[0], images[1]);
    assert!(images[0]
        .chunks_exact(4)
        .any(|pixel| pixel != PIXEL_GATE_CLEAR));
}
