use serde::Deserialize;

use super::super::thrown_test_host::Host;
use super::*;
use crate::object_runtime::{ActorConstructionRequest, ActorConstructorNull, ActorTerrainInputs};

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
    entry: Fields,
    exit: Fields,
    calls: Vec<NativeCall>,
    world: u32,
}

/// The grenade's bytes the update reads or writes, by native offset.
#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
struct Fields {
    word_28: u16,
    flags: u32,
    position: [u16; 3],
    basis: [u32; 10],
    handle: u32,
    owner: u32,
    velocity: [u16; 3],
    angular: [u32; 3],
    health: u16,
    state: u32,
    byte_138: u8,
    fuse: u16,
    age: u16,
    block: [u8; 10],
    resource_148: u32,
    tail: [u8; 4],
    burn: u32,
}

#[derive(Deserialize)]
struct NativeCall {
    name: String,
}

fn grenade(arena: &mut ObjectArena, entry: &Fields) -> ObjectId {
    let sine = [0; 4096];
    let grenade = arena
        .allocate_behavior_resource(
            ActorConstructionRequest {
                allocation_position: Some(entry.position),
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
            26,
        )
        .unwrap();
    seed(arena, grenade, entry);
    grenade
}

fn seed(arena: &mut ObjectArena, grenade: ObjectId, entry: &Fields) {
    {
        let base = &mut arena.owned_mut(grenade).unwrap().state.base;
        base.word_28 = entry.word_28;
        base.flags = entry.flags;
        base.position_words = Some(entry.position);
    }
    let actor = arena.actor_mut(grenade).unwrap();
    actor.rotating.basis.words = entry.basis;
    actor.handle = ActorHandle::from_raw(entry.handle);
    actor.owner_handle = ActorHandle::from_raw(entry.owner);
    actor.words_8e_92 = entry.velocity;
    actor.words_94_9c = entry.angular;
    actor.health = entry.health;
    actor.word_c4 = entry.state;
    let state = arena.thrown_mut(grenade).unwrap();
    state.byte_138 = entry.byte_138;
    state.word_13a = entry.fuse;
    state.word_13c = entry.age;
    state.bytes_13e = Some(entry.block);
    state.dword_148 = entry.resource_148;
    match &mut state.tail {
        Family455560Tail::Flags14c {
            byte_14c,
            byte_14d,
            byte_14e,
            byte_14f,
            dword_150,
            ..
        } => {
            *byte_14c = Some(entry.tail[0]);
            *byte_14d = Some(entry.tail[1]);
            *byte_14e = entry.tail[2];
            *byte_14f = entry.tail[3];
            *dword_150 = entry.burn;
        }
        other => panic!("selector 26's tail, not {other:?}"),
    }
}

fn read(arena: &ObjectArena, grenade: ObjectId) -> Fields {
    let object = arena.object(grenade).unwrap();
    let actor = arena.actor(grenade).unwrap();
    let state = arena.grenade26(grenade).unwrap();
    let Family455560Tail::Flags14c {
        byte_14c,
        byte_14d,
        byte_14e,
        byte_14f,
        dword_150,
        ..
    } = &state.tail
    else {
        panic!("selector 26's tail");
    };
    Fields {
        word_28: object.base.word_28,
        flags: object.base.flags,
        position: object.base.position_words.unwrap(),
        basis: actor.rotating.basis.words,
        handle: actor.handle.raw(),
        owner: actor.owner_handle.raw(),
        velocity: actor.words_8e_92,
        angular: actor.words_94_9c,
        health: actor.health,
        state: actor.word_c4,
        byte_138: state.byte_138,
        fuse: state.word_13a,
        age: state.word_13c,
        block: state.bytes_13e.unwrap(),
        resource_148: state.dword_148,
        tail: [byte_14c.unwrap(), byte_14d.unwrap(), *byte_14e, *byte_14f],
        burn: *dword_150,
    }
}

/// Every update 00455E10 of the grenades thrown in the recordings, each run
/// from its captured entry on its world's loaded terrain: Infest02's three
/// on Level 1 (resource 119, no payload flags) and Infest03's one in world
/// 11 (resource 124, flag 2). The flight with its bounces, the fuse and
/// age, and each detonation through the lifecycle (state 1 arms it, state
/// 0 bursts and disposes it, flag 2 broadcasting first) match: every field
/// the update writes, and the services reached against the native calls.
#[test]
fn grenade26_update_matches_every_recorded_tick() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/grenade26_updates.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    for selection in [18, 11] {
        replay(
            selection,
            receipts
                .updates
                .iter()
                .filter(|update| update.world == selection),
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn replay<'a>(
    selection: u32,
    updates: impl Iterator<Item = &'a Update>,
    failures: &mut Vec<String>,
) {
    // World 11's load disposes a placement whose death runs (a clean
    // import stops there).
    let (_resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        selection,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let water = (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16;
    for update in updates {
        let arena = &mut world.arena;
        let grenade = grenade(arena, &update.entry);
        let mut host = Host {
            calls: Vec::new(),
            water,
            terrain: &world.terrain,
        };
        let mut rng = update.rng_in;
        arena
            .tick_grenade26(grenade, update.delta, &sine, &mut rng, &mut host)
            .unwrap();
        let disposed = update.calls.iter().any(|call| call.name == "479fa0");
        let mut exit = update.exit.clone();
        let mut actual = read(arena, grenade);
        if disposed {
            // 00479FA0's own flag writes belong to the disposal.
            exit.flags = 0;
            actual.flags = 0;
        }
        if actual != exit {
            failures.push(format!("{}: {actual:?}\n   != {exit:?}", update.at));
        }
        // The update's own helpers and the disposal are not services, and the
        // default burst's debris births its central particle (0048A0F0).
        let mut expected = Vec::new();
        for call in &update.calls {
            let name = call.name.as_str();
            if matches!(name, "4559e0" | "4558b0" | "4555b0" | "479fa0")
                || (name == "48a0f0" && expected.last() == Some(&"458d20"))
            {
                continue;
            }
            expected.push(name);
        }
        let reached = host.calls.clone();
        if reached != expected {
            failures.push(format!("{}: calls {reached:?} != {expected:?}", update.at));
        }
        if host.calls.is_empty() && rng != update.rng_out {
            failures.push(format!(
                "{}: rng {rng:08x} != {:08x}",
                update.at, update.rng_out
            ));
        }
    }
}

/// `004559B0`, another machine's grenade: the smoke `004558B0` (flag 1, past
/// a second), `+138` set, then `0047EBF0`, which moves it on its velocity
/// and spin (`0047B1E0` with mask 0x18) and counts `+C8` down. No fuse runs.
#[test]
fn a_remote_grenade_smokes_and_drifts_as_004559b0() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/grenade26_updates.json"
    ))
    .unwrap();
    let (_resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        18,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let mut entry = receipts.updates[0].entry.clone();
    entry.flags &= !8;
    entry.tail[0] = 1;
    entry.age = 1200;
    entry.byte_138 = 0;
    let arena = &mut world.arena;
    let thrown = grenade(arena, &entry);
    let twin = grenade(arena, &entry);
    arena.actor_mut(thrown).unwrap().word_c8 = 5000;
    let mut host = Host {
        calls: Vec::new(),
        water: 0,
        terrain: &world.terrain,
    };
    let mut rng = 1;
    arena
        .tick_remote_thrown(thrown, 1000, &sine, &mut rng, &mut host)
        .unwrap();
    assert_eq!(host.calls, ["489d90"]);
    arena
        .tick_actor_physics(
            twin,
            crate::object_runtime::ActorPhysicsRequest {
                microseconds: 1000,
                mask: 0x18,
                sine_quarter: &sine,
            },
        )
        .unwrap();
    let (moved, expected) = (read(arena, thrown), read(arena, twin));
    assert_eq!(
        (moved.position, moved.basis),
        (expected.position, expected.basis)
    );
    assert_eq!(
        (moved.byte_138, moved.fuse, moved.age),
        (1, entry.fuse, entry.age)
    );
    assert_eq!(arena.actor(thrown).unwrap().word_c8, 4000);
}
