use std::collections::VecDeque;

use serde::Deserialize;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorTerrainInputs, FixedPositionTarget,
    NativeTarget, ObjectList,
};

#[derive(Deserialize)]
struct Receipts {
    updates: Vec<Update>,
}

#[derive(Deserialize)]
struct Update {
    at: String,
    delta: u32,
    rng_in: u32,
    rng_out: u32,
    entry: Shot,
    exit: Shot,
    calls: Vec<Call>,
}

#[derive(Deserialize)]
struct Shot {
    position: [u16; 3],
    aim: [u16; 3],
    life: u32,
    slot: u16,
    strength: u16,
    channel: u8,
    pending: u32,
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Call {
    Roll {
        value: u32,
    },
    Spark {
        particle_type: u32,
        position: [u16; 3],
        velocity: [u16; 3],
        draws: u32,
    },
    Owner {
        found: bool,
    },
    Deliver {
        packet: [u8; 4],
        target_type: u32,
        target: [u32; 8],
        draws: u32,
    },
    Dispose,
}

/// Records the update's effects and advances the RNG by the draws retail's
/// own spark and delivery made, so the update's own draws line up.
struct Host {
    draws: VecDeque<u32>,
    sparks: Vec<([u16; 3], [u16; 3])>,
    packets: Vec<ActorDamagePacket>,
}

impl Host {
    fn advance(&mut self, rng: &mut u32) {
        for _ in 0..self.draws.pop_front().expect("a recorded effect") {
            name_entry_rng_next(rng);
        }
    }
}

impl Shot32Host for Host {
    type Error = Infallible;
    fn spark(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), Infallible> {
        self.sparks.push((position, velocity));
        self.advance(rng);
        Ok(())
    }
    fn deliver(
        &mut self,
        _: &mut ObjectArena,
        packet: ActorDamagePacket,
        _: NativeHeapRef,
        _: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Infallible> {
        self.packets.push(packet);
        self.advance(rng);
        Ok(())
    }
}

fn shot(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    let sine = [0; 4096];
    arena
        .allocate_behavior_resource(
            ActorConstructionRequest {
                allocation_position: Some(position),
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
            32,
        )
        .unwrap()
}

/// Every 00486250 call of the cannon's shots in Infest01 (one shot) and
/// Infest02, each run from its captured entry state: the life, the spark
/// roll and its velocity, the owner, the packets and the disposal.
#[test]
fn shot32_update_matches_every_recorded_tick() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/shot32_updates.json"
    ))
    .unwrap();
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let vehicle_handle = world.arena.actor(vehicle).unwrap().handle;
    let target = world
        .arena
        .import_fixture_target(NativeTarget::FixedPosition(
            FixedPositionTarget::from_point(vehicle_handle, Some([0; 3])),
        ));
    let mut failures = Vec::new();
    for update in &receipts.updates {
        let arena = &mut world.arena;
        let shot = shot(arena, update.entry.position);
        arena.owned_mut(shot).unwrap().state.base.position_words = Some(update.entry.position);
        let found = !update.calls.contains(&Call::Owner { found: false });
        arena.actor_mut(shot).unwrap().owner_handle = if found {
            vehicle_handle
        } else {
            ActorHandle::from_raw(0)
        };
        {
            let state = arena.shot32_mut(shot).unwrap();
            state.words_138 = Some(update.entry.aim);
            state.dword_140 = update.entry.life;
            state.word_144 = update.entry.slot;
            state.word_146 = Some(update.entry.strength);
            state.byte_148 = update.entry.channel;
            state.dword_14c = update.entry.pending;
        }
        let delivers = update
            .calls
            .iter()
            .any(|call| matches!(call, Call::Deliver { .. }));
        arena.derived_actor_mut(vehicle).unwrap().target_148 = delivers.then_some(target);
        let mut host = Host {
            draws: update
                .calls
                .iter()
                .filter_map(|call| match call {
                    Call::Spark { draws, .. } | Call::Deliver { draws, .. } => Some(*draws),
                    _ => None,
                })
                .collect(),
            sparks: Vec::new(),
            packets: Vec::new(),
        };
        let mut rng = update.rng_in;
        arena
            .tick_shot32(shot, update.delta, &mut rng, &mut host)
            .unwrap();
        let sparks: Vec<_> = update
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::Spark {
                    particle_type: 5,
                    position,
                    velocity,
                    ..
                } => Some((*position, *velocity)),
                _ => None,
            })
            .collect();
        let packets: Vec<_> = update
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::Deliver { packet, .. } => Some(ActorDamagePacket {
                    strength: i16::from_le_bytes([packet[0], packet[1]]),
                    channel: packet[2],
                    byte_03: None,
                }),
                _ => None,
            })
            .collect();
        let disposed = arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|entry| entry.object == shot);
        let state = arena.shot32(shot).unwrap();
        let actual = (
            sparks == host.sparks,
            packets == host.packets,
            disposed == update.calls.contains(&Call::Dispose),
            state.dword_140 == update.exit.life,
            state.dword_14c == update.exit.pending,
            state.words_138 == Some(update.exit.aim),
            rng == update.rng_out,
        );
        if actual != (true, true, true, true, true, true, true) {
            failures.push(format!(
                "{}: sparks {:?} vs {:?}, packets {:?}, disposed {disposed}, life {}, pending {}, rng {rng:08x} vs {:08x}",
                update.at, host.sparks, sparks, host.packets, state.dword_140, state.dword_14c, update.rng_out
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(receipts.updates.len(), 101);
}

/// 004860D0: mode 2 adds a pending packet; 1 and 2 restore the life; any
/// other mode leaves the shot and answers 0.
#[test]
fn shot32_retrigger_counts_and_restores_life() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let shot = shot(&mut world.arena, [0; 3]);
    world.arena.shot32_mut(shot).unwrap().dword_140 = 1000;
    assert!(!world.arena.retrigger_shot32(shot, 0).unwrap());
    assert!(!world.arena.retrigger_shot32(shot, 3).unwrap());
    assert_eq!(world.arena.shot32(shot).unwrap().dword_140, 1000);
    assert!(world.arena.retrigger_shot32(shot, 1).unwrap());
    assert_eq!(
        (
            world.arena.shot32(shot).unwrap().dword_140,
            world.arena.shot32(shot).unwrap().dword_14c
        ),
        (SHOT32_LIFE_MICROS, 1)
    );
    assert!(world.arena.retrigger_shot32(shot, 2).unwrap());
    assert_eq!(world.arena.shot32(shot).unwrap().dword_14c, 2);
}

/// 00485F90 for each recorded ground shot: the shot takes its weapon group's
/// muzzle (cached at the recorded position) and aims at the fixed target's
/// point less that position, as the following update found it. Without a
/// target it aims 2000 along the owner's forward row; a remote shot in a
/// network game stays as it is.
#[test]
fn shot32_pose_takes_the_muzzle_and_aims_at_the_target() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/shot32_updates.json"
    ))
    .unwrap();
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let arena = &mut world.arena;
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let vehicle_handle = arena.actor(vehicle).unwrap().handle;
    let mut checked = 0;
    for update in &receipts.updates {
        let Some(target) = update.calls.iter().find_map(|call| match call {
            Call::Deliver {
                target_type: 8,
                target,
                ..
            } => Some(target),
            _ => None,
        }) else {
            continue;
        };
        let point = [target[2] as u16, (target[2] >> 16) as u16, target[3] as u16];
        let reference = arena.import_fixture_target(NativeTarget::FixedPosition(
            FixedPositionTarget::from_point(ActorHandle::from_raw(target[1]), Some(point)),
        ));
        arena.derived_actor_mut(vehicle).unwrap().target_148 = Some(reference);
        let group = &mut arena
            .derived_actor_mut(vehicle)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap()
            .groups_1c[usize::from(update.entry.slot)];
        group.word_0c |= 2;
        for axis in 0..3 {
            group.coordinates_00_0a[3 + axis] = Some(update.entry.position[axis]);
        }
        let shot = shot(arena, [0; 3]);
        arena.actor_mut(shot).unwrap().owner_handle = vehicle_handle;
        arena.shot32_mut(shot).unwrap().word_144 = update.entry.slot;
        arena.pose_shot32(shot, false, 0).unwrap();
        assert_eq!(
            arena.object(shot).unwrap().base.position_words,
            Some(update.entry.position),
            "{}",
            update.at
        );
        assert_eq!(
            arena.shot32(shot).unwrap().words_138,
            Some(update.entry.aim),
            "{}",
            update.at
        );
        checked += 1;
    }
    assert_eq!(checked, 4);

    arena.derived_actor_mut(vehicle).unwrap().target_148 = None;
    arena.actor_mut(vehicle).unwrap().rotating.basis.words[7..10].copy_from_slice(&[
        (-0.6_f32).to_bits(),
        0.0_f32.to_bits(),
        0.8_f32.to_bits(),
    ]);
    let shot = shot(arena, [0; 3]);
    arena.actor_mut(shot).unwrap().owner_handle = vehicle_handle;
    arena.pose_shot32(shot, false, 0).unwrap();
    // 0.8f32 * 2000 truncates to 1600; -0.6f32 * 2000 to -1200.
    assert_eq!(
        arena.shot32(shot).unwrap().words_138,
        Some([-1200_i16 as u16, 0, 1600])
    );

    let remote = self::shot(arena, [0; 3]);
    arena.actor_mut(remote).unwrap().owner_handle = vehicle_handle;
    arena.owned_mut(remote).unwrap().state.base.flags |= 0x8000_0000;
    arena.pose_shot32(remote, true, 0).unwrap();
    assert_eq!(arena.shot32(remote).unwrap().words_138, None);
    assert_eq!(
        arena.object(remote).unwrap().base.position_words,
        Some([0; 3])
    );
}

/// The +3C of a shot another machine fired: the cannon shot keeps its life
/// and throws a spark one draw in three (00486100), from the fresh heap's
/// zero aim before a record's sync writes it; nothing is delivered.
#[test]
fn a_remote_shot_only_throws_its_sparks() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let shot = shot(&mut world.arena, [100, 200, 300]);
    world.arena.shot32_mut(shot).unwrap().dword_140 = 1000;
    world.arena.shot32_mut(shot).unwrap().dword_14c = 1;
    let mut host = Host {
        draws: VecDeque::from([0; 8]),
        sparks: Vec::new(),
        packets: Vec::new(),
    };
    // Seeds whose first draw is (not) a multiple of three.
    let throws = |seed: u32| name_entry_rng_next(&mut seed.clone()) % 3 == 0;
    let thrown = (0..).find(|&seed| throws(seed)).unwrap();
    let held = (0..).find(|&seed| !throws(seed)).unwrap();
    let mut rng = held;
    world
        .arena
        .tick_remote_shot32(shot, 40_000, &mut rng, &mut host)
        .unwrap();
    assert!(host.sparks.is_empty());
    let mut rng = thrown;
    world
        .arena
        .tick_remote_shot32(shot, 40_000, &mut rng, &mut host)
        .unwrap();
    assert_eq!(host.sparks, [([100, 200, 300], spark_velocity([0; 3]))]);
    let state = world.arena.shot32(shot).unwrap();
    assert_eq!((state.dword_140, state.dword_14c), (1000, 1));
    assert!(host.packets.is_empty());
}

/// A remote laser's +3C (004854C0) turns its angle by a tenth of the
/// delta, backwards in weapon group 1, and leaves its age.
#[test]
fn a_remote_laser_turns() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let shot = shot(&mut world.arena, [0; 3]);
    world.arena.shot32_mut(shot).unwrap().selector31 = Some((5, 77));
    let mut host = Host {
        draws: VecDeque::new(),
        sparks: Vec::new(),
        packets: Vec::new(),
    };
    world
        .arena
        .tick_remote_shot32(shot, 1000, &mut 0, &mut host)
        .unwrap();
    assert_eq!(
        world.arena.shot32(shot).unwrap().selector31,
        Some((105, 77))
    );
    world.arena.shot32_mut(shot).unwrap().word_144 = 1;
    world
        .arena
        .tick_remote_shot32(shot, 1000, &mut 0, &mut host)
        .unwrap();
    assert_eq!(world.arena.shot32(shot).unwrap().selector31, Some((5, 77)));
    assert!(host.sparks.is_empty());
}
