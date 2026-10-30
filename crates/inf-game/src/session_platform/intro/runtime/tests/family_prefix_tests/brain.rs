//! The Alien Brain (selector 85) over world 19 and the production updates:
//! it saves the liquid plane while the aliens hold capture nodes and wakes
//! when they hold none; collapsed, it sheds debris; from 5 s into state 7
//! the liquid rises, and state 8 puts it back and restarts the brain.

use super::*;

fn brain(world: &WorldSession) -> ObjectId {
    let brains: Vec<ObjectId> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.selector85(actor).is_ok())
        .collect();
    assert_eq!(brains.len(), 1, "the Alien Brain places one brain");
    brains[0]
}

fn state(world: &WorldSession, brain: ObjectId) -> u32 {
    world.arena.selector85(brain).unwrap().state_184
}

#[test]
fn the_alien_brain_wakes_collapses_and_raises_the_liquid() {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_with_services(
        19,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let brain = brain(&world);
    assert_eq!(world.arena.alien_brain_words().phase_546910, 0);
    only_callbacks(&mut world, &[brain]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    let shift = world.terrain.plane_shift();
    let plane = world.terrain.plane_base();
    let level = plane.wrapping_shr(shift) as u16;
    for _ in 0..3 {
        owners.tick(&mut world, &runtime, &assets, 40_000);
    }
    assert_eq!(state(&world, brain), 0, "the aliens hold their nodes");
    assert_eq!(
        world.arena.selector85(brain).unwrap().plane_19a,
        Some(level)
    );

    // Every node taken from the aliens wakes it, and it stops targeting.
    // Any actor off the aliens' team takes a node it touches.
    let capturer = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            world.arena.selector84(actor).is_err()
                && world.arena.actor(actor).unwrap().word_b4 != 0x12
        })
        .expect("a non-alien actor");
    let nodes: Vec<ObjectId> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.selector84(actor).is_ok())
        .collect();
    for node in nodes {
        world.arena.touch_selector84(node, Some(capturer)).unwrap();
    }
    assert_eq!(world.arena.capture_node_counts().aliens_547620, 0);
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(state(&world, brain), 1);
    assert_eq!(world.arena.selector62(brain).unwrap().may_target_160, 0);
    // With no player's unit holding it as its target, it waits.
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(state(&world, brain), 1);

    // Collapsed: register 2 closes, and debris goes off round it.
    world.arena.selector85_mut(brain).unwrap().state_184 = 4;
    let free = owners.particles.free_count();
    for _ in 0..20 {
        owners.tick(&mut world, &runtime, &assets, 40_000);
    }
    let collapsed = world.arena.selector85(brain).unwrap();
    assert_eq!(collapsed.state_184, 4);
    assert!((1..0xffff).contains(&collapsed.registers_190[1]), "closing");
    assert!(owners.particles.free_count() < free, "debris sparks");

    // State 7, 10 s in: the liquid has risen a unit for every 45 ms past 5 s.
    let brain_state = world.arena.selector85_mut(brain).unwrap();
    brain_state.state_184 = 7;
    brain_state.clock_17c = 10_000_000;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    let risen = (5_040_000 / 45_000 + i32::from(level as i16)) << shift;
    assert_eq!(world.terrain.plane_base(), risen);
    // Past 50 s state 8 puts it back and sets the restart, which the next
    // update takes: state 1 again.
    world.arena.selector85_mut(brain).unwrap().clock_17c = 50_000_000;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(state(&world, brain), 8);
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(world.terrain.plane_base(), i32::from(level as i16) << shift);
    assert_eq!(world.arena.alien_brain_words().restart_546914, 1);
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(state(&world, brain), 1);
    assert_eq!(world.arena.alien_brain_words().restart_546914, 0);
    assert_eq!(world.actor_updates.selector62_stops, Vec::new());
}
