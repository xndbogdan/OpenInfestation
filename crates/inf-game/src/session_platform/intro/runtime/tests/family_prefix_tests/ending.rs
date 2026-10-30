//! The Alien Brain's whole ending over world 19 and the production updates,
//! with the brain (selector 85) and the director (selector 78) both live:
//! every capture node taken, the vehicle's lock on the brain, the director's
//! flight, the brain's attack, the aliens' units dying, the brain opening,
//! the bug's flight and the world's end. Offline worlds hold no player input
//! references, so the update list keeps running the director after the take
//! (as `director.rs` explains).

use super::*;

fn only(world: &WorldSession, test: impl Fn(ObjectId) -> bool) -> ObjectId {
    let found: Vec<ObjectId> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| test(actor))
        .collect();
    assert_eq!(found.len(), 1);
    found[0]
}

/// The phases come in order: 1 at the lock, 2 when helper `+164` has flown
/// in (the brain's attack, whose ribbons aim as the lab found:
/// `BrainRibbonAimApproximation`), 3 when the attack ends, 4 when the
/// aliens' units are gone (the brain opens), 5 when it has opened, and the
/// world ends with the completion bonus.
#[test]
fn the_brain_and_the_director_play_the_ending_through() {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_with_services(
        19,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let brain = only(&world, |actor| world.arena.selector85(actor).is_ok());
    let director = only(&world, |actor| world.arena.selector78(actor).is_ok());
    only_callbacks(&mut world, &[brain, director]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
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
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(world.arena.selector85(brain).unwrap().state_184, 1);
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    let handle = world.arena.actor(brain).unwrap().handle;
    world.arena.lock_vehicle_for_tests(vehicle, handle);

    let mut phases = vec![world.arena.alien_brain_words().phase_546910];
    let mut attack_states = vec![world.arena.selector62(brain).unwrap().state_138];
    for _ in 0..2500 {
        owners.tick(&mut world, &runtime, &assets, 40_000);
        assert_eq!(world.actor_updates.selector62_stops, Vec::new());
        assert_eq!(world.actor_updates.selector78_stops, Vec::new());
        let phase = world.arena.alien_brain_words().phase_546910;
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
        let attack = world.arena.selector62(brain).unwrap().state_138;
        if attack_states.last() != Some(&attack) {
            attack_states.push(attack);
        }
        if owners.state.transition_pending() {
            break;
        }
    }
    assert_eq!(phases, [0, 1, 2, 3, 4, 5]);
    assert_eq!(attack_states, [0, 1, 2, 3]);
    assert!(owners.state.transition_pending(), "the world ends");
    assert_eq!(world.actor_updates.cheat_flags_84, 0x40);
}
