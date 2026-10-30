use serde::Deserialize;

use super::*;
use crate::object_runtime::{
    ActorServiceAllocationRequest, ActorServiceResourceKind, CameraSettings, FixedPositionTarget,
    NativeWeaponFireNode, ObjectId, WeaponEmissionRequest, WeaponFiringContext,
    WeaponFiringEmitter, WeaponFiringRuntime, SHOT32_LIFE_MICROS,
};
use crate::session_platform::world_actor_factory::{RequiredListenerEffects, WorldActorChildHost};

#[derive(Deserialize)]
struct Receipts {
    updates: Vec<Update>,
}

#[derive(Deserialize)]
struct Update {
    at: String,
    rng_in: u32,
    calls: Vec<serde_json::Value>,
}

fn draws(call: &serde_json::Value) -> u32 {
    call["draws"].as_u64().unwrap() as u32
}

/// Infest02's cannon hit the ground four times, in world 18 at coarse
/// spacing. Each delivery takes its burst from the material of the target
/// point's cell (the selector retail passed 00489390) and, run through the
/// world host from the RNG word it started with, draws exactly as recorded.
#[test]
fn shot32_ground_delivery_picks_the_cells_burst_and_draws_as_recorded() {
    let receipts: Receipts =
        serde_json::from_str(include_str!("../../../tests/fixtures/shot32_updates.json")).unwrap();
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let impact_selectors = world.particle_impact_selectors.unwrap();
    let vehicle: ObjectId = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let owner = world.arena.actor(vehicle).unwrap().handle;
    let mut tasks = ProcessTaskRuntime::default();
    let mut particles = ProcessParticleRuntime::default();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let mut furniture = FurnitureEffectQueue::default();
    let mut process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0x1234)),
    };
    let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
    let clock = crate::frontend_camera::AnimationClock::default();
    let state = SessionState::default();
    let sine = assets.sine_table().values;
    let mut checked = 0;
    for update in &receipts.updates {
        let mut rng = update.rng_in;
        for call in &update.calls {
            match call["kind"].as_str().unwrap() {
                "roll" => {
                    name_entry_rng_next(&mut rng);
                }
                "spark" => {
                    for _ in 0..draws(call) {
                        name_entry_rng_next(&mut rng);
                    }
                }
                "deliver" => {
                    if call["target_type"].as_u64() != Some(8) {
                        break;
                    }
                    let words: Vec<u32> = call["target"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|word| word.as_u64().unwrap() as u32)
                        .collect();
                    let point = [words[2] as u16, (words[2] >> 16) as u16, words[3] as u16];
                    let material = (world.terrain.cell_at([point[0], point[2]]).raw() >> 8) & 3;
                    assert_eq!(
                        u64::from(impact_selectors[usize::from(material)]),
                        call["ground_selector"].as_u64().unwrap(),
                        "{}",
                        update.at
                    );
                    let target = world
                        .arena
                        .import_fixture_target(NativeTarget::FixedPosition(
                            FixedPositionTarget::from_point(
                                ActorHandle::from_raw(words[1]),
                                Some(point),
                            ),
                        ));
                    let mut expected = rng;
                    for _ in 0..draws(call) {
                        name_entry_rng_next(&mut expected);
                    }
                    let bytes: Vec<u8> = call["packet"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|byte| byte.as_u64().unwrap() as u8)
                        .collect();
                    let packet = ActorDamagePacket {
                        strength: i16::from_le_bytes([bytes[0], bytes[1]]),
                        channel: bytes[2],
                        byte_03: Some(bytes[3]),
                    };
                    let mut host = WorldShotHost {
                        grant_text: None,
                        mission: &mut world.mission,
                        tasks: &mut tasks,
                        terrain: &mut world.terrain,
                        particles: &mut particles,
                        sounds: &mut sounds,
                        furniture: &mut furniture,
                        residency: &resident,
                        executable,
                        factory_process: &mut process,
                        behavior_tables: &mut tables,
                        animation_clock: &clock,
                        session: &state,
                        particle_load_factor: 0,
                        impact_selectors,
                        listeners: &mut world.listeners,
                        sine: &sine,
                        world_flags: world.world_flags,
                    };
                    host.deliver(&mut world.arena, packet, target, owner, &mut rng)
                        .unwrap();
                    assert_eq!(rng, expected, "{}", update.at);
                    checked += 1;
                    break;
                }
                _ => {}
            }
        }
    }
    assert_eq!(checked, 4);
}

/// 0048C890 for the cannon, from Infest01's emission at E6C3C0: the
/// vehicle's row 0 is registry kind 1, resource 116. The world's child
/// factory builds the shot at the emission point without a draw, the group
/// keeps its handle, and the shot takes the emission velocity and the
/// group. The tail roll (BF02, even) adds the flash's two draws, which ends
/// at retail's RNG word. The group's next emission repeats that shot.
#[test]
fn shot32_emission_builds_then_repeats_the_groups_shot() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
    let process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0x1234)),
    };
    let mut particles = ProcessParticleRuntime::default();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let position = [0xb9df, 0x60, 0x6cc5];
    world
        .arena
        .with_supplied_family_dispatch_actor(vehicle, |state| {
            state.words_8e_92 = [-53_i16 as u16, -1_i16 as u16, 4];
        })
        .unwrap();
    assert_eq!(
        crate::object_runtime::resident_constructor_selector(&mut world.arena, &resident, 116)
            .unwrap(),
        32
    );
    assert!(!world
        .arena
        .derived_actor(vehicle)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .words_24
        .is_empty());
    let mut rng = 0xb866_c653;
    for pass in 0..2 {
        let node = world
            .arena
            .actor_service_heap_mut()
            .allocate(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::WeaponFireNode,
            })
            .unwrap()
            .unwrap();
        world
            .arena
            .actor_service_heap_mut()
            .construct_weapon_fire_node(
                node,
                NativeWeaponFireNode {
                    direction_bits: [0xbf6f_4385, 0x3c8e_6dd8, 0xbcd9_57da],
                    compensation: 0,
                    group: 0,
                    selection: 0,
                    speed_scale: 0,
                    next: None,
                },
            )
            .unwrap();
        let mut expected = rng;
        if pass == 1 && name_entry_rng_next(&mut expected) & 1 == 0 {
            name_entry_rng_next(&mut expected);
            name_entry_rng_next(&mut expected);
        }
        let mut effects = RequiredListenerEffects;
        let mut children = WorldActorChildHost {
            residency: &resident,
            executable,
            process: &process,
            terrain: &world.terrain,
            world_flags: world.world_flags,
            clock: 0,
            sine: &sine,
            math: &math,
            camera_settings: Some(CameraSettings {
                first_person: 0,
                camera_style: 0,
            }),
            world_start: Some(&mut world.world_start),
            listeners: &mut world.listeners,
            listener_effects: &mut effects,
        };
        WeaponFiringRuntime {
            executable,
            factory_process: &process,
            particles: &mut particles,
            statistics: None,
            sounds: &mut sounds,
            residency: &resident,
            phase: Some(4),
            children: Some(&mut children),
            pulse: None,
        }
        .emit(
            &mut world.arena,
            &mut WeaponFiringContext {
                tables: &mut tables,
                rng: &mut rng,
            },
            WeaponEmissionRequest {
                actor: vehicle,
                node,
                position,
            },
        )
        .unwrap();
        let handle = world
            .arena
            .derived_actor(vehicle)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .words_24[0]
            .unwrap();
        let shot = world
            .arena
            .resolve_actor_handle(ActorHandle::from_raw(handle))
            .unwrap()
            .unwrap();
        let state = world.arena.shot32(shot).unwrap();
        assert_eq!(state.word_144, 0);
        if pass == 0 {
            assert_eq!(rng, 0x2af6_3644);
            assert_eq!(state.dword_14c, 1);
            assert_eq!(
                world.arena.object(shot).unwrap().base.position_words,
                Some(position)
            );
            // 0047B510 added (F4D8, 33, FFB5): row 0's speed, 3000, along
            // the direction plus the vehicle's +8E. The record lacks the
            // +8E; that speed leaves (-53, -1, 4) for it.
            assert_eq!(
                world.arena.actor(shot).unwrap().words_8e_92,
                [0xf4d8, 0x33, 0xffb5]
            );
        } else {
            assert_eq!(rng, expected);
            assert_eq!(state.dword_14c, 2);
            assert_eq!(state.dword_140, SHOT32_LIFE_MICROS);
        }
    }
}
