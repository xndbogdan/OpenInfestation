use std::convert::Infallible;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorDamagePacket, ActorHandle,
    ActorTerrainInputs, NativeHeapRef, Shot32Host,
};

struct NoEffects;

impl Shot32Host for NoEffects {
    type Error = Infallible;
    fn spark(
        &mut self,
        _: &mut ObjectArena,
        _: [u16; 3],
        _: [u16; 3],
        _: &mut u32,
    ) -> Result<(), Infallible> {
        panic!("a laser throws no sparks")
    }
    fn deliver(
        &mut self,
        _: &mut ObjectArena,
        _: ActorDamagePacket,
        _: NativeHeapRef,
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Infallible> {
        Ok(())
    }
}

fn laser(arena: &mut ObjectArena, group: u16) -> ObjectId {
    let sine = [0; 4096];
    let laser = arena
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
            31,
        )
        .unwrap();
    arena.shot32_mut(laser).unwrap().word_144 = group;
    laser
}

/// `laser-fire-16ms.json`'s first laser updates (tick 95): each adds the
/// 16,000 µs to its age, loses them from its life and turns a tenth of them,
/// backwards in group 1. Neither draws the RNG.
#[test]
fn laser31_update_ages_and_turns_by_group() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    world.arena.derived_actor_mut(vehicle).unwrap().target_148 = None;
    let owner = world.arena.actor(vehicle).unwrap().handle;
    for (group, angle) in [(1, 0xf9c0), (0, 0x0640)] {
        let laser = laser(&mut world.arena, group);
        world.arena.actor_mut(laser).unwrap().owner_handle = owner;
        assert!(world.arena.is_laser31(laser) && world.arena.is_shot32(laser));
        let mut rng = 0x1234_5678;
        world
            .arena
            .tick_shot32(laser, 16_000, &mut rng, &mut NoEffects)
            .unwrap();
        let state = world.arena.shot32(laser).unwrap();
        assert_eq!(
            (state.selector31, state.dword_140, state.byte_148, rng),
            (Some((angle, 16_000)), 584_000, 8, 0x1234_5678)
        );
    }
}

/// 00485E68..00485F1C: tick 95's pose aimed (-1834, -302, -1264), which the
/// age 16,000 cut to the (-180, -30, -124) tick 96 found. Fully reached, the
/// aim keeps its length until the life falls below 0xFFFE, then shrinks
/// with half the life.
#[test]
fn laser31_reach_grows_with_age_and_shrinks_with_life() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let laser = laser(&mut world.arena, 0);
    let aim = |words: [i16; 3]| Some(words.map(|word| word as u16));
    let mut reach = |age: u32, life: u32, words: [i16; 3]| {
        let state = world.arena.shot32_mut(laser).unwrap();
        state.selector31 = Some((0, age));
        state.dword_140 = life;
        state.words_138 = aim(words);
        world.arena.reach_laser31(laser, false).unwrap();
        world.arena.shot32(laser).unwrap().words_138
    };
    assert_eq!(
        reach(16_000, 584_000, [-1834, -302, -1264]),
        aim([-180, -30, -124])
    );
    assert_eq!(
        reach(200_000, 584_000, [-1834, -302, -1264]),
        aim([-1834, -302, -1264])
    );
    assert_eq!(reach(800_000, 0x8000, [-1834, 302, 0]), aim([-917, 151, 0]));
}

/// 00485520 over a flat projection that keeps every point on screen: two
/// strips of sixteen quads, the first drawing three RNG steps a segment.
/// Each quad starts at the previous point and widens by six pixels across a
/// horizontal step; the wound strip's points stay within its radius of the
/// straight one.
#[test]
fn laser31_draw_lays_two_strips_of_sixteen_quads() {
    let mut sine = [0_u16; 4096];
    for (index, word) in sine.iter_mut().enumerate() {
        *word = ((index as f64 / 4096.0 * std::f64::consts::FRAC_PI_2).sin() * 32767.0) as u16;
    }
    let mut clip = [0_u8; 256];
    clip[0x12] = 1;
    let beam = Laser31Beam {
        aim: [3200, 0, 0],
        angle: 0,
        group: 0,
        negative: false,
    };
    let project = |[x, y, _]: [i32; 3]| -> Result<Selector69Projected, Infallible> {
        Ok(Selector69Projected {
            xy: u32::from((x + 100) as u16) | (u32::from((y + 200) as u16) << 16),
            depth: 1000,
            clip: 0x12,
            fog: Some(0),
        })
    };
    let mut quads = Vec::new();
    let mut rng = 1;
    let mut expected = 1;
    for _ in 0..48 {
        name_entry_rng_next(&mut expected);
    }
    laser31_draw(beam, &sine, &clip, &mut rng, project, |quad| {
        quads.push(quad);
        Ok(())
    })
    .unwrap();
    assert_eq!((quads.len(), rng), (32, expected));
    let first = &quads[0];
    assert!(!first.far);
    // The origin, widened by the strip's starting six pixels.
    assert_eq!(first.corners[0].xy, (106 << 0) | (206 << 16));
    assert_eq!(first.corners[1].xy, 100 | (200 << 16));
    let y = |xy: u32| (xy >> 16) as u16 as i16 as i32;
    let x = |xy: u32| xy as u16 as i16 as i32;
    for quad in &quads[16..] {
        let point = quad.corners[2].xy;
        assert!((y(point) - 200).abs() <= 3200 * 7 / 128 + 1);
        assert!(x(point) >= 100);
    }
    // The last strip ends on the aim: its radius is back to zero.
    assert_eq!(quads[31].corners[2].xy, (3300 & 0xffff) | (200 << 16));
}
