//! A saved start pair reaches the vehicle through a menu load.

use super::*;
use crate::object_runtime::ObjectId;
use crate::terrain_runtime::TerrainSpacing;
use crate::world_backend::WorldSession;

/// Terra Nova's main gate (placement 97) and its northern factory
/// (placement 98) both carry command 13 with this ID; gate 19 grants it.
const HIDDEN_BASE_GRANT: u32 = 0x84;

fn vehicle_position(world: &WorldSession) -> [u16; 3] {
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap()
}

/// The gate to Oolbryte (world 20) whose completion byte `+167` is `completion`.
fn oolbryte_gate(world: &WorldSession, completion: u8) -> Option<ObjectId> {
    world.arena.actor_list().iter().copied().find(|&actor| {
        world
            .arena
            .selector30_actor(actor)
            .is_ok_and(|gate| gate.word_15c == 20 && gate.byte_167 == completion)
    })
}

/// The factory standing at (31218, 50453).
fn northern_factory(world: &WorldSession) -> Option<ObjectId> {
    world.arena.actor_list().iter().copied().find(|&actor| {
        world.arena.selector15(actor).is_ok()
            && world
                .arena
                .object(actor)
                .unwrap()
                .base
                .position_words
                .is_some_and(|p| [p[0], p[2]] == [31218, 50453])
    })
}

#[test]
fn a_saved_second_start_places_the_vehicle_there() {
    let (runtime, _assets, world) =
        crate::test_worlds::offline_world_from_saved_start(2, 1, &[], TerrainSpacing::Units256);
    let position = vehicle_position(&world);
    let start = |pair| {
        let mut state = WorldStartState {
            pair_index: pair,
            override_position: None,
            vehicle_selection: 0,
            harness_spawn: None,
            port_spawn: None,
        };
        WorldStartContext {
            state: &mut state,
            runtime: &runtime,
            terrain: &world.terrain,
        }
        .resolve_position()
        .unwrap()
    };
    let distance = |to: [u16; 3]| {
        let dx = f64::from(position[0]) - f64::from(to[0]);
        let dz = f64::from(position[2]) - f64::from(to[2]);
        dx.hypot(dz)
    };
    // Terra Nova's first start is (8200, 12000), by the gate back to world
    // 18; its second (31200, 31200), by gate 19, where Oolbryte's arrival
    // gate (placement 75) leads back; its third (33536, 55040), by the main
    // gate 97, where Oolbryte's far gate (placement 4) leads. The respawn
    // sets the vehicle a few hundred units off the start.
    let (first, second, third) = (start(0), start(1), start(2));
    assert_eq!([first[0], first[2]], [8200, 12000]);
    assert_eq!([second[0], second[2]], [31200, 31200]);
    assert_eq!([third[0], third[2]], [33536, 55040]);
    assert_eq!([position[0], position[2]], [31544, 31407]);
    assert!(distance(second) < 500.0 && distance(first) > 20000.0);
    assert_eq!(world.world_start.pair_index, 1);
    // Gate 19 (to Oolbryte, completing the world) starts on the aliens'
    // side. Standing at the start, it passes to the player and sets the
    // vehicle 450 in front of it (0048C4F0).
    let gate = oolbryte_gate(&world, 1).unwrap();
    let gate_position = world
        .arena
        .object(gate)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!([gate_position[0], gate_position[2]], [31994, 31407]);
    assert_eq!(
        world.arena.selector30_actor(gate).unwrap().word_188,
        Some(18)
    );
    assert_eq!(world.arena.actor(gate).unwrap().word_b4, 2);
}

/// The hidden base: Terra Nova's main gate and northern factory stand only
/// for a player who knows 0x84, which gate 19 grants. A first visit has
/// neither; the third start, by the main gate, has both, and the factory's
/// shop lists the Dual fire cannons (0x96).
#[test]
fn terra_novas_hidden_base_stands_only_after_gate_19() {
    let (_, _, first_visit) =
        crate::test_worlds::offline_world_from_saved_start(2, 0, &[], TerrainSpacing::Units256);
    assert!(oolbryte_gate(&first_visit, 1).is_some());
    assert!(oolbryte_gate(&first_visit, 0).is_none());
    assert!(northern_factory(&first_visit).is_none());

    let (_, _, world) = crate::test_worlds::offline_world_from_saved_start(
        2,
        2,
        &[HIDDEN_BASE_GRANT],
        TerrainSpacing::Units256,
    );
    let gate = oolbryte_gate(&world, 0).unwrap();
    let gate_position = world
        .arena
        .object(gate)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!([gate_position[0], gate_position[2]], [33409, 54946]);
    assert_eq!(world.arena.actor(gate).unwrap().word_b4, 2);
    let position = vehicle_position(&world);
    let dx = f64::from(position[0]) - f64::from(gate_position[0]);
    let dz = f64::from(position[2]) - f64::from(gate_position[2]);
    assert!((dx.hypot(dz) - 450.0).abs() < 2.0, "{position:?}");
    let factory = northern_factory(&world).unwrap();
    let shop = world.arena.selector15(factory).unwrap().grants_198.mask_04;
    for id in [0x96, 0x9b, 0xa8] {
        assert!(inf_formats::grant_is_owned(&shop, id).unwrap(), "{id:#x}");
    }
}

/// The start harness's spawn replaces the start's X/Z (Y still the ground's
/// plus 60) and its heading, ahead of a stored override; without either
/// field the start's own values stand.
#[test]
fn the_harness_spawn_replaces_the_starts_position_and_heading() {
    let (runtime, _assets, world) =
        crate::test_worlds::offline_world_from_saved_start(2, 1, &[], TerrainSpacing::Units256);
    let resolve = |harness| {
        let mut state = WorldStartState {
            pair_index: 0,
            override_position: Some([1000, 0, 2000]),
            vehicle_selection: 0,
            harness_spawn: harness,
            port_spawn: None,
        };
        let context = WorldStartContext {
            state: &mut state,
            runtime: &runtime,
            terrain: &world.terrain,
        };
        (
            context.resolve_position().unwrap(),
            context.resolve_heading().unwrap(),
        )
    };
    let (stored, start_heading) = resolve(None);
    assert_eq!([stored[0], stored[2]], [1000, 2000]);
    let spawn = crate::harness_start::HarnessSpawn {
        position: Some([31200, 31200]),
        heading: Some(90),
    };
    let (position, heading) = resolve(Some(spawn));
    assert_eq!([position[0], position[2]], [31200, 31200]);
    let cell = world.terrain.cell_at([31200, 31200]).height_i8();
    let shift = match world.terrain.sampling().height_scale {
        crate::terrain_runtime::TerrainHeightScale::Units16 => 4,
        crate::terrain_runtime::TerrainHeightScale::Units32 => 5,
    };
    assert_eq!(position[1], ((i32::from(cell) << shift) + 60) as u16);
    // 0048DBC0's conversion: (degrees << 16) / 360.
    assert_eq!(heading, [0, 0x4000, 0]);
    let (_, kept) = resolve(Some(crate::harness_start::HarnessSpawn {
        position: Some([31200, 31200]),
        heading: None,
    }));
    assert_eq!(kept, start_heading);
    let (kept, _) = resolve(Some(crate::harness_start::HarnessSpawn {
        position: None,
        heading: Some(-90),
    }));
    assert_eq!([kept[0], kept[2]], [1000, 2000]);
}

/// Exxodus's big turrets (H13 resource 11, selector 11) come in two sets on
/// the same three spots: H14 placements 0, 4 and 17 fight and carry command
/// 14 on 0x17, the grant Altair Prime's hangars give, so they leave once
/// the player has it; placements 170 to 172 carry command 12 (state 2) and
/// command 13 on 0x17, so they stand only then, disabled. A load disposes
/// whichever set the grant rules out, and its drain records no placement
/// deaths for them (0048DFDB..0048DFE4 hold the mission's +08). So the
/// first visit leaves world 4's finished bits clear, as retail's does
/// (tampered-run: runtime_re/lab/scenarios/exxodus-fresh.json
/// (a constructed save at Exxodus's first start)), and the return after the
/// hangars finds the disabled set standing.
#[test]
fn exxodus_keeps_its_disabled_turrets_for_the_return_from_altair() {
    let turrets = |world: &WorldSession| -> Vec<([u16; 2], u32)> {
        world
            .arena
            .actor_list()
            .iter()
            .copied()
            .filter(|&actor| {
                world.arena.selector11_actor(actor).is_ok()
                    && world.arena.object(actor).unwrap().base.resource_word == 11
            })
            .map(|actor| {
                let p = world
                    .arena
                    .object(actor)
                    .unwrap()
                    .base
                    .position_words
                    .unwrap();
                ([p[0], p[2]], world.arena.actor(actor).unwrap().word_c4)
            })
            .collect()
    };
    let (_, _, first) =
        crate::test_worlds::offline_world_from_saved_start(4, 0, &[], TerrainSpacing::Units256);
    assert_eq!(
        turrets(&first),
        [
            ([40276, 24064], 3),
            ([41350, 22921], 3),
            ([46624, 25699], 3)
        ]
    );
    assert_eq!(first.mission.current_world_completion().unwrap(), [0; 4]);
    let (_, _, back) =
        crate::test_worlds::offline_world_from_saved_start(4, 3, &[0x17], TerrainSpacing::Units256);
    assert_eq!(
        turrets(&back),
        [
            ([40230, 24051], 2),
            ([46627, 25594], 2),
            ([41380, 22901], 2)
        ]
    );
    assert_eq!(back.mission.current_world_completion().unwrap(), [0; 4]);
}
