//! Exxodus's tanks (selector 36) under `PortFixes::paced_actors`. Retail steps
//! a tank's tread phases `+270/+272` (`00481180`, from its update `00481710`)
//! at every update, so at 60 fps at every world tick; its model alone reads
//! them (`00481E50`'s selectors 5 and 6, a frame of three each). A paced tank
//! updates every other tick, and its treads still step at every tick: held
//! at its paced updates alone they turned at half retail's rate, flipping
//! between two of their frames.

use super::*;
use crate::frontend_settings::PACED_ACTOR_MICROS;
use crate::session_platform::{ActorPacing, WorldTickCarry};

/// One exact world tick.
const TICK: u32 = 16_384;

#[test]
fn a_held_tanks_treads_step_at_every_tick() {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        4,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let tanks: Vec<ObjectId> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.selector36_actor(actor).is_ok())
        .collect();
    assert_eq!(tanks.len(), 6, "Exxodus places six tanks");
    only_callbacks(&mut world, &tanks);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    let pacing = ActorPacing::Paced {
        micros: PACED_ACTOR_MICROS,
    };
    let phases = |world: &WorldSession| -> Vec<u32> {
        tanks
            .iter()
            .map(|&tank| world.arena.selector36_actor(tank).unwrap().word_270)
            .collect()
    };
    let mut carry = WorldTickCarry::default();
    let (mut held, mut held_steps, mut paced_steps) = (0, 0, 0);
    // Four seconds of world ticks.
    for _ in 0..244 {
        let before = phases(&world);
        let paced = carry.paced_delta(pacing, TICK);
        owners.tick_paced(&mut world, &runtime, &assets, TICK, paced);
        let stepped = before
            .iter()
            .zip(phases(&world))
            .filter(|(was, now)| **was != *now)
            .count();
        if paced.is_some() {
            paced_steps += stepped;
        } else {
            held += 1;
            held_steps += stepped;
        }
    }
    let stops: Vec<_> = world
        .actor_updates
        .family_prefix_stops
        .iter()
        .filter(|stop| tanks.contains(&stop.actor))
        .collect();
    assert!(stops.is_empty(), "the tanks' updates stopped: {stops:?}");
    assert_eq!(held, 122, "every other tick holds the tanks' update");
    assert!(
        paced_steps > 200,
        "the tanks drive: {paced_steps} paced steps"
    );
    // A held tick moves no tank, so it steps the treads as the update before
    // did: about as often as the paced ticks do.
    assert!(
        held_steps * 10 >= paced_steps * 9,
        "held ticks step the treads: {held_steps} held, {paced_steps} paced"
    );
}
