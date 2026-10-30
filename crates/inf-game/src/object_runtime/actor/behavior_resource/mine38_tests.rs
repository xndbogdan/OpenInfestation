use serde::Deserialize;

use super::super::thrown_test_host::Host;
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorTerrainInputs,
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
    entry: Fields,
    exit: Fields,
    calls: Vec<NativeCall>,
}

/// The mine's bytes the update reads or writes, by native offset.
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
    team: u32,
    state: u32,
    byte_138: u8,
    fuse: u16,
    age: u16,
    block: [u8; 10],
    resource_148: u32,
    tail: [u8; 4],
    burn: u32,
    flat: u8,
    trigger: u32,
    /// +15C: zero, or the pointer to a target, which the port holds by
    /// reference; only zero is recorded.
    target: u32,
    tracked: u32,
}

#[derive(Deserialize)]
struct NativeCall {
    name: String,
}

fn mine(arena: &mut ObjectArena, entry: &Fields) -> ObjectId {
    let sine = [0; 4096];
    let mine = arena
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
            38,
        )
        .unwrap();
    seed(arena, mine, entry);
    mine
}

fn seed(arena: &mut ObjectArena, mine: ObjectId, entry: &Fields) {
    assert_eq!(entry.target, 0, "a recorded target needs its heap object");
    {
        let base = &mut arena.owned_mut(mine).unwrap().state.base;
        base.word_28 = entry.word_28;
        base.flags = entry.flags;
        base.position_words = Some(entry.position);
    }
    let actor = arena.actor_mut(mine).unwrap();
    actor.rotating.basis.words = entry.basis;
    actor.handle = ActorHandle::from_raw(entry.handle);
    actor.owner_handle = ActorHandle::from_raw(entry.owner);
    actor.words_8e_92 = entry.velocity;
    actor.words_94_9c = entry.angular;
    actor.health = entry.health;
    actor.word_b4 = entry.team;
    actor.word_c4 = entry.state;
    let state = arena.thrown_mut(mine).unwrap();
    state.byte_138 = entry.byte_138;
    state.word_13a = entry.fuse;
    state.word_13c = entry.age;
    state.bytes_13e = Some(entry.block);
    state.dword_148 = entry.resource_148;
    match &mut state.tail {
        Family455560Tail::Selector38 {
            byte_14c,
            byte_14d,
            byte_14e,
            byte_14f,
            dword_150,
            byte_154,
            dword_158,
            object_15c,
            dword_160,
        } => {
            *byte_14c = Some(entry.tail[0]);
            *byte_14d = Some(entry.tail[1]);
            *byte_14e = entry.tail[2];
            *byte_14f = entry.tail[3];
            *dword_150 = entry.burn;
            *byte_154 = Some(entry.flat);
            *dword_158 = Some(entry.trigger);
            *object_15c = None;
            *dword_160 = entry.tracked;
        }
        other => panic!("selector 38's tail, not {other:?}"),
    }
}

fn read(arena: &ObjectArena, mine: ObjectId) -> Fields {
    let object = arena.object(mine).unwrap();
    let actor = arena.actor(mine).unwrap();
    let state = arena.thrown(mine).unwrap();
    let Family455560Tail::Selector38 {
        byte_14c,
        byte_14d,
        byte_14e,
        byte_14f,
        dword_150,
        byte_154,
        dword_158,
        object_15c,
        dword_160,
    } = &state.tail
    else {
        panic!("selector 38's tail");
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
        team: actor.word_b4,
        state: actor.word_c4,
        byte_138: state.byte_138,
        fuse: state.word_13a,
        age: state.word_13c,
        block: state.bytes_13e.unwrap(),
        resource_148: state.dword_148,
        tail: [byte_14c.unwrap(), byte_14d.unwrap(), *byte_14e, *byte_14f],
        burn: *dword_150,
        flat: byte_154.unwrap(),
        trigger: dword_158.unwrap(),
        target: u32::from(object_15c.is_some()),
        tracked: *dword_160,
    }
}

/// Every update 00456860 of the mine Infest03 laid in world 6 (resource
/// 118, payload flag 8, flat, trigger radius 600), each run from its
/// captured entry on the loaded terrain. Its scan finds nobody inside the
/// trigger radius, the fuse rearms it a second after it lands and every
/// second after, the grenade's flight carries it, and the detonator's mark
/// sets it off: state 1, the flat burst (visual 0x3A, the block 1500 above
/// it), message 0x13 to its thrower, the block where it is, the explosion
/// and its disposal. Every field the update writes matches (the bursts'
/// impulses on the mine are the world's), and the services reached match
/// the native calls.
#[test]
fn mine38_update_matches_every_recorded_tick() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/mine38_updates.json"
    ))
    .unwrap();
    let (_resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let water = (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16;
    let mut failures = Vec::new();
    for update in &receipts.updates {
        let arena = &mut world.arena;
        let mine = mine(arena, &update.entry);
        let mut host = Host {
            calls: Vec::new(),
            water,
            terrain: &world.terrain,
        };
        let mut rng = update.rng_in;
        arena
            .tick_mine38(mine, update.delta, &sine, &mut rng, &mut host)
            .unwrap();
        let disposed = update.calls.iter().any(|call| call.name == "479fa0");
        let mut exit = update.exit.clone();
        let mut actual = read(arena, mine);
        if disposed {
            // 00479FA0's own flag writes belong to the disposal.
            exit.flags = 0;
            actual.flags = 0;
        }
        if update.calls.iter().any(|call| call.name == "4887d0") {
            // The bursts' impulses on the mine (004887D0, its raised block
            // and the placements it set off) are the world's, which this
            // recording host does not run.
            actual.velocity = exit.velocity;
            actual.angular = exit.angular;
        }
        if actual != exit {
            failures.push(format!("{}: {actual:?}\n   != {exit:?}", update.at));
        }
        // The update's own helpers, the scan, the disposal and 0047DD40's
        // send are not services, and the default burst's debris births its
        // central particle (0048A0F0).
        let mut expected = Vec::new();
        for call in &update.calls {
            let name = call.name.as_str();
            if matches!(
                name,
                "475ec0"
                    | "4567a0"
                    | "455e10"
                    | "4559e0"
                    | "4558b0"
                    | "4555b0"
                    | "4564b0"
                    | "47dd80"
                    | "479fa0"
            ) || (name == "48a0f0" && expected.last() == Some(&"458d20"))
            {
                continue;
            }
            expected.push(name);
        }
        let reached: Vec<_> = host
            .calls
            .iter()
            .copied()
            .filter(|&name| name != "4566a0")
            .collect();
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
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Kankutan (world 15) places seven mines (resource 118). 004603B0's case
/// 0x26 builds them as the resource factory does (0x164 bytes,
/// 00455560(2000), table 00525914), and their after-load +34 is the
/// shared 00401310, so the load sets their flag and goes on.
#[test]
fn kankutans_placed_mines_are_built() {
    use crate::terrain_runtime::TerrainSpacing;
    let (_resident, _assets, session) =
        crate::test_worlds::offline_world_retained(15, TerrainSpacing::Units128);
    let arena = &session.arena;
    let mines: Vec<(usize, ObjectId)> = session
        .report
        .placements
        .iter()
        .enumerate()
        .filter_map(|(index, placement)| placement.map(|object| (index, object)))
        .filter(|&(_, object)| arena.is_mine38(object))
        .collect();
    assert_eq!(
        mines.iter().map(|&(index, _)| index).collect::<Vec<_>>(),
        [19, 20, 21, 22, 23, 24, 27]
    );
    for (index, mine) in mines {
        assert_eq!(
            arena.thrown(mine).unwrap().word_13a,
            2000,
            "placement {index}"
        );
        assert_ne!(
            arena.object(mine).unwrap().base.flags & crate::object_runtime::AFTER_LOAD_FLAG,
            0,
            "placement {index}"
        );
    }
}
