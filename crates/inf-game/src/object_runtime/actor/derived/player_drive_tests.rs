use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs,
};

fn block(turn: i32, pitch: i32, thrust: i32) -> PlayerControlBlock {
    PlayerControlBlock {
        turn_14: turn,
        pitch_18: pitch,
        thrust_1c: thrust,
        ..PlayerControlBlock::default()
    }
}

/// 00428490 digital: Thrust drives full ahead (0x7FFFF) or back (-0x7FFF)
/// before the pitch, which drives full ahead while negative; without
/// either, the forward intent is the speed along the nose. A positive turn
/// is -0x10000.
#[test]
fn the_players_digital_block_gives_full_intents_or_the_noses_speed() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let sine = [0; 4096];
    let actor = arena
        .allocate_behavior_resource(
            ActorConstructionRequest {
                allocation_position: Some([0; 3]),
                owner: 1,
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: Some(0),
                    sine_quarter: &sine,
                },
            },
            28,
        )
        .unwrap();
    {
        let state = arena.actor_mut(actor).unwrap();
        state.rotating.basis.words[7] = 0.5_f32.to_bits();
        state.rotating.basis.words[8] = 0.0_f32.to_bits();
        state.rotating.basis.words[9] = 0.75_f32.to_bits();
        state.words_8e_92 = [100, 50, (-200_i16) as u16];
    }
    let executable_bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&executable_bytes).unwrap();
    let intents = |arena: &ObjectArena, block: PlayerControlBlock| {
        arena
            .family_player_intents(actor, &block, executable)
            .unwrap()
    };
    assert_eq!(intents(&arena, block(0, 0, 9)), [0, FULL_AHEAD]);
    assert_eq!(intents(&arena, block(0, 0, -9)), [0, FULL_BACK]);
    assert_eq!(intents(&arena, block(0, -3, 0)), [0, FULL_AHEAD]);
    assert_eq!(intents(&arena, block(0, 3, -9)), [0, FULL_BACK]);
    assert_eq!(intents(&arena, block(5, 0, 1)), [-TURN, FULL_AHEAD]);
    assert_eq!(intents(&arena, block(-5, 0, 1)), [TURN, FULL_AHEAD]);
    // (-200 * 0.75 + 50 * 0) + 100 * 0.5, truncated.
    assert_eq!(intents(&arena, block(0, 0, 0)), [0, -100]);
}

#[derive(serde::Deserialize)]
struct DriveReceipt {
    cases: Vec<DriveCase>,
}

#[derive(serde::Deserialize)]
struct DriveCase {
    name: String,
    size: u16,
    basis: [f32; 10],
    velocity: [i16; 3],
    table: u32,
    behavior: Option<DriveBehavior>,
    block: DriveBlock,
    calls: Vec<DriveCall>,
}

#[derive(serde::Deserialize)]
struct DriveBehavior {
    selected: i32,
    entries: Vec<DriveEntry>,
}

#[derive(serde::Deserialize)]
struct DriveEntry {
    count: u32,
    timer: u32,
    pressure: u32,
}

#[derive(serde::Deserialize)]
struct DriveBlock {
    analog: u8,
    axes: [i32; 2],
    turn: i32,
    pitch: i32,
    thrust: i32,
    fire: i32,
}

#[derive(serde::Deserialize)]
struct DriveCall {
    name: String,
    intents: Option<[i32; 2]>,
    direction: Option<[u32; 3]>,
    args: Vec<serde_json::Value>,
}

/// 00481A50 against the unchanged executable (check_selector36_drive.py):
/// the intents the motion (+118) and the sound receive, whether the weapon
/// ticks, and what the queue 0048D0D0 receives, over digital and analog
/// blocks, the nose's speed, each +254 table and every fire gate, in the
/// native order: the water, the weapon, the motion, the counters, the
/// sound, the status.
#[test]
fn the_selector36_drive_matches_the_native_intents_and_weapon() {
    let receipt: DriveReceipt = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector36_drive_native.json"
    ))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    for case in &receipt.cases {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let mut constructor_rng = 0;
        let actor = arena
            .allocate_selector36_actor(
                crate::object_runtime::DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([384, 64, 640]),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &sine,
                        },
                    },
                    world_mask_limits: [3; 2],
                },
                &mut constructor_rng,
            )
            .unwrap();
        arena.owned_mut(actor).unwrap().state.base.word_28 = case.size;
        arena.family4236d0_actor_mut(actor).unwrap().table_254 = case.table;
        {
            let state = arena.actor_mut(actor).unwrap();
            state.rotating.basis.words = case.basis.map(f32::to_bits);
            state.words_8e_92 = case.velocity.map(|word| word as u16);
        }
        if let Some(spec) = &case.behavior {
            let mut behavior =
                crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 0).unwrap();
            behavior.byte_10 = spec.selected as u8;
            behavior.entries_18 = spec
                .entries
                .iter()
                .map(|entry| [entry.count, 0, 0, 0, 0, entry.timer, 0, entry.pressure])
                .collect();
            arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(behavior);
        }
        let block = PlayerControlBlock {
            analog_04: case.block.analog,
            axes_08: [case.block.axes[0], case.block.axes[1], 0],
            turn_14: case.block.turn,
            pitch_18: case.block.pitch,
            thrust_1c: case.block.thrust,
            fire_20: case.block.fire,
            ..PlayerControlBlock::default()
        };
        let intents = arena
            .family_player_intents(actor, &block, executable)
            .unwrap();
        let fire = if case.behavior.is_some() {
            arena.family_player_fire(actor, &block).unwrap()
        } else {
            None
        };
        let mut order = vec!["water"];
        if case.behavior.is_some() {
            order.push("weapon_tick");
        }
        if fire.is_some() {
            order.push("queue");
        }
        order.extend(["motion", "counters", "sound", "status"]);
        let native: Vec<&str> = case.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(native, order, "{} order", case.name);
        for call in &case.calls {
            match call.name.as_str() {
                "motion" | "sound" => {
                    assert_eq!(call.intents, Some(intents), "{} {}", case.name, call.name)
                }
                "queue" => {
                    let (held, direction) = fire.unwrap();
                    assert_eq!(call.direction, Some(direction), "{} direction", case.name);
                    assert_eq!(
                        call.args,
                        vec![
                            serde_json::Value::Bool(true),
                            serde_json::json!(held),
                            serde_json::json!(0)
                        ],
                        "{} queue",
                        case.name
                    );
                }
                _ => {}
            }
        }
    }
    assert_eq!(receipt.cases.len(), 42);
}

/// 00424C40, the +44 of selectors 9, 10 and 21, against the unchanged
/// executable (check_family_drive.py): the same intents and fire decisions
/// as selector 36's drive over every +254 table, the assault craft's and
/// the trucks' 0052FF08 among them, in its own shorter order: the water,
/// the weapon, the motion (+118), the status. No counters and no engine
/// sound.
#[test]
fn the_family_drive_matches_the_native_intents_and_weapon() {
    let receipt: DriveReceipt = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/family_drive_native.json"
    ))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    for case in &receipt.cases {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let actor = arena
            .allocate_family4236d0_actor(
                crate::object_runtime::DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([384, 64, 640]),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &sine,
                        },
                    },
                    world_mask_limits: [3; 2],
                },
                &mut 0,
            )
            .unwrap();
        arena.owned_mut(actor).unwrap().state.base.word_28 = case.size;
        arena.family4236d0_actor_mut(actor).unwrap().table_254 = case.table;
        {
            let state = arena.actor_mut(actor).unwrap();
            state.rotating.basis.words = case.basis.map(f32::to_bits);
            state.words_8e_92 = case.velocity.map(|word| word as u16);
        }
        if let Some(spec) = &case.behavior {
            let mut behavior =
                crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 0).unwrap();
            behavior.byte_10 = spec.selected as u8;
            behavior.entries_18 = spec
                .entries
                .iter()
                .map(|entry| [entry.count, 0, 0, 0, 0, entry.timer, 0, entry.pressure])
                .collect();
            arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(behavior);
        }
        let block = PlayerControlBlock {
            analog_04: case.block.analog,
            axes_08: [case.block.axes[0], case.block.axes[1], 0],
            turn_14: case.block.turn,
            pitch_18: case.block.pitch,
            thrust_1c: case.block.thrust,
            fire_20: case.block.fire,
            ..PlayerControlBlock::default()
        };
        let intents = arena
            .family_player_intents(actor, &block, executable)
            .unwrap();
        let fire = if case.behavior.is_some() {
            arena.family_player_fire(actor, &block).unwrap()
        } else {
            None
        };
        let mut order = vec!["water"];
        if case.behavior.is_some() {
            order.push("weapon_tick");
        }
        if fire.is_some() {
            order.push("queue");
        }
        order.extend(["motion", "status"]);
        let native: Vec<&str> = case.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(native, order, "{} order", case.name);
        for call in &case.calls {
            match call.name.as_str() {
                "motion" => assert_eq!(call.intents, Some(intents), "{} motion", case.name),
                "queue" => {
                    let (held, direction) = fire.unwrap();
                    assert_eq!(call.direction, Some(direction), "{} direction", case.name);
                    assert_eq!(
                        call.args,
                        vec![
                            serde_json::Value::Bool(true),
                            serde_json::json!(held),
                            serde_json::json!(0)
                        ],
                        "{} queue",
                        case.name
                    );
                }
                _ => {}
            }
        }
    }
    assert_eq!(receipt.cases.len(), 44);
}

/// 00467DA0 hands 00428490 a scientist's own +290 (0053A080) where the
/// family's +44 hands it +254: a held scientist's analog forward axis
/// scales by that table's maximum ahead and minimum back.
#[test]
fn a_held_scientists_analog_intents_read_its_290_table() {
    let (_resident, assets, world) = crate::test_worlds::offline_world_retained(
        18,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let arena = &world.arena;
    let scientist = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector12_actor(actor).is_ok())
        .expect("Level 1's scientists");
    assert_eq!(
        arena.selector12_actor(scientist).unwrap().table_290,
        0x0053_a080
    );
    let bytes = executable.mapped(0x0053_a080, 8).unwrap().bytes;
    let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let size = i32::from(arena.object(scientist).unwrap().base.word_28);
    for (axes, limit, beyond) in [
        ([3000, 0x6000], word(0), 0x4000),
        ([-3000, -0x6000], word(4), 0x4000),
    ] {
        let block = PlayerControlBlock {
            analog_04: 1,
            axes_08: [axes[0], axes[1], 0],
            ..PlayerControlBlock::default()
        };
        assert_eq!(
            arena
                .family_player_intents(scientist, &block, executable)
                .unwrap(),
            [
                axes[0],
                (size.wrapping_mul(limit) >> 12).wrapping_mul(beyond) / 0xe000
            ]
        );
    }
}
