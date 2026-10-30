//! Infest04's fence chain on Helniazine (world 6). The second of two mines
//! the player detonated beside the pad's fence queued the fence segments'
//! program 00532E30 at nine points, and their 004373B0 walks queued the rest
//! of the fence through 00436240: 21 records along the south, north and east
//! sides, after which the pad stood open (`fence-queue04.txt`).

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    object_runtime::{ActorHandle, TargetNullHandleSource},
    process_sound::ProcessSoundRuntime,
    terrain_runtime::{ProcessTerrainPolicy, TerrainSpacing},
    world_furniture_effect::{
        resident::ResidentFurnitureExecutor, FurnitureCameraCallbackHost,
        FurnitureCameraCallbackInputs, FurnitureParticleCallbackHost,
        FurnitureParticleCallbackInputs, FurnitureParticleEffect, FurniturePickupCallbackHost,
        FurniturePickupCallbackInputs, FurnitureQueueOutcome, FurnitureRadiusCallbackHost,
        FurnitureRadiusCallbackInputs, ProcessFurnitureQueryState,
    },
};

const FENCE_PROGRAM: u32 = 0x0053_2e30;

/// The blast's 00436A60 calls for 00532E30, in order.
const BLAST: [[u16; 2]; 9] = [
    [39278, 18496],
    [39278, 18752],
    [39278, 19008],
    [39278, 19264],
    [39278, 19520],
    [39278, 19776],
    [39278, 20032],
    [39534, 18496],
    [39534, 20032],
];

/// The chain's 00436240 calls that queued a record, in order; its other 43
/// calls found the record already queued.
const CHAIN: [[u16; 2]; 21] = [
    [39680, 18432],
    [39680, 19968],
    [39936, 18432],
    [39936, 19968],
    [40192, 18432],
    [40192, 19968],
    [40448, 18432],
    [40448, 19968],
    [40704, 18432],
    [40704, 19968],
    [40960, 18432],
    [40960, 19968],
    [41216, 18432],
    [41216, 19968],
    [41472, 18432],
    [41472, 18688],
    [41472, 19712],
    [41472, 19968],
    [41472, 18944],
    [41472, 19456],
    [41472, 19200],
];

/// The segments' program reaches only sounds, its debris burst (00436D20
/// selector 0), the chain and the clear.
#[derive(Default)]
struct FenceEffects {
    bursts: usize,
}

impl TargetNullHandleSource for FenceEffects {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        None
    }
}

impl FurnitureRadiusCallbackHost for FenceEffects {
    type Error = String;
    fn apply_radius(
        &mut self,
        _: FurnitureRadiusCallbackInputs<'_>,
        _: ActorHandle,
        _: bool,
    ) -> Result<(), String> {
        Err("the fence program reaches no 004375A0".into())
    }
}

impl FurnitureCameraCallbackHost for FenceEffects {
    type Error = String;
    fn camera_flash(&mut self, _: u32, _: FurnitureCameraCallbackInputs<'_>) -> Result<(), String> {
        Err("the fence program reaches no 004375D0".into())
    }
}

impl FurniturePickupCallbackHost for FenceEffects {
    type Error = String;
    fn first_player_owns(&mut self, _: u16) -> Result<Option<bool>, String> {
        Err("the fence program reaches no 00437880".into())
    }
    fn birth_pickup(
        &mut self,
        _: u16,
        _: u16,
        _: FurniturePickupCallbackInputs<'_>,
    ) -> Result<(), String> {
        Err("the fence program reaches no 00437880".into())
    }
}

impl FurnitureParticleCallbackHost for FenceEffects {
    type Error = String;
    fn emit(
        &mut self,
        effect: FurnitureParticleEffect,
        _: FurnitureParticleCallbackInputs<'_>,
    ) -> Result<(), String> {
        assert!(matches!(effect, FurnitureParticleEffect::Burst(_)));
        self.bursts += 1;
        Ok(())
    }
}

/// Every kind-0x0E record the pad's fence holds. They sit on the coarse
/// grid's cells, 0x100 apart.
fn fence_records(
    terrain: &crate::terrain_runtime::TerrainRuntime,
    resources: &mut ResidentFurnitureChainResources<'_>,
) -> Vec<[u16; 2]> {
    let mut found = Vec::new();
    for x in (39168..=41472).step_by(256) {
        for z in (18432..=19968).step_by(256) {
            let xz = [x as u16, z as u16];
            if !terrain.cell_at(xz).has_sparse_record() {
                continue;
            }
            if let Some((_, record)) = terrain.sparse_record_at(xz) {
                if record.resource_lookup.raw() != 0
                    && resources.kind(record.resource_lookup).unwrap() == FENCE_KIND
                {
                    found.push(xz);
                }
            }
        }
    }
    found
}

#[test]
fn a_fence_segment_takes_the_whole_fence_down_as_in_infest04() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut resources = ResidentFurnitureChainResources {
        resident: &resident,
        executable,
    };
    // The blast's nine and the chain's 21: the whole fence.
    assert_eq!(fence_records(&world.terrain, &mut resources).len(), 30);
    let program = FurnitureEffectProgram::new(FENCE_PROGRAM).unwrap();
    let mut queue = FurnitureEffectQueue::default();
    for xz in BLAST {
        let outcome = queue
            .enqueue(&world.terrain, FurnitureQueueRequest { xz, program })
            .unwrap();
        assert!(matches!(outcome, FurnitureQueueOutcome::Enqueued { .. }));
    }
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let mut process = ProcessFurnitureQueryState::native_startup(executable).unwrap();
    let mut policy = ProcessTerrainPolicy::supplied(TerrainSpacing::Units256);
    let mut effects = FenceEffects::default();
    let mut rng = 0x1234_5678;
    let mut queued = Vec::new();
    for _ in 0..100 {
        if queue.records().is_empty() {
            break;
        }
        let before = queue.records().to_vec();
        queue
            .tick(
                &mut world.terrain,
                125_000,
                &mut ResidentFurnitureExecutor {
                    resident: &resident,
                    executable,
                    sounds: &mut sounds,
                    process: &mut process,
                    effects: &mut effects,
                    chain: FurnitureChainLoan {
                        policy: &mut policy,
                        detail: 0,
                        world_flags: Some(world.world_flags),
                        network_active: false,
                    },
                },
                &mut rng,
            )
            .unwrap();
        // 0043758E restored the coarse pair after every walk.
        assert_eq!(world.terrain.sampling().spacing, TerrainSpacing::Units256);
        queued.extend(
            queue
                .records()
                .iter()
                .filter(|record| {
                    !before
                        .iter()
                        .any(|old| old.aligned_key == record.aligned_key)
                })
                .map(|record| {
                    // A record queued ahead of the executor may already have
                    // run the program's undelayed first steps this tick.
                    assert!(
                        (FENCE_PROGRAM..=FENCE_PROGRAM + 0x3c).contains(&record.program.address())
                    );
                    [record.position_words[0], record.position_words[2]]
                }),
        );
    }
    assert!(queue.records().is_empty());
    assert_eq!(queued, CHAIN);
    assert_eq!(effects.bursts, BLAST.len() + CHAIN.len());
    assert_eq!(
        fence_records(&world.terrain, &mut resources),
        Vec::<[u16; 2]>::new()
    );
}

#[test]
fn online_00436240_queues_nothing() {
    let (resident, assets, world) =
        crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut resources = ResidentFurnitureChainResources {
        resident: &resident,
        executable,
    };
    let fence = fence_records(&world.terrain, &mut resources);
    let mut queue = FurnitureEffectQueue::default();
    assert!(
        !queue_furniture_program(&mut queue, &world.terrain, fence[0], true, &mut resources)
            .unwrap()
    );
    assert!(queue.records().is_empty());
    assert!(
        queue_furniture_program(&mut queue, &world.terrain, fence[0], false, &mut resources)
            .unwrap()
    );
    assert_eq!(queue.records()[0].program.address(), FENCE_PROGRAM);
}
