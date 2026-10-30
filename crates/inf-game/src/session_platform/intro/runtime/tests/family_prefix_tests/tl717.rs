//! Exxodus's TL717 (base resource 88, selector 21 in mode 4, placement 12)
//! over the loaded world and the production updates: its walker starts from
//! rest (the stride phase +34 and its legs' bytes are fresh-heap carries),
//! and a killing hit carries it through the family's death.

use super::*;
use crate::object_runtime::{ActorDamagePacket, ActorHandle};
use crate::session_platform::particles::damage::{apply_world_actor_damage, WorldDamageOwners};

fn tl717(world: &WorldSession) -> ObjectId {
    world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.object(actor).unwrap().base.resource_word == 88)
        .expect("Exxodus places the TL717")
}

fn stops(world: &WorldSession, actor: ObjectId) -> Vec<String> {
    world
        .actor_updates
        .family_prefix_stops
        .iter()
        .filter(|stop| stop.actor == actor)
        .map(|stop| format!("{stop:?}"))
        .collect()
}

#[test]
fn exxodus_tl717_walks_and_dies() {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        4,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let actor = tl717(&world);
    assert!(
        world
            .arena
            .selector10_actor(actor)
            .is_ok_and(|state| state.swing.is_some()),
        "selector 21"
    );
    only_callbacks(&mut world, &[actor]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    for _ in 0..50 {
        owners.tick(&mut world, &runtime, &assets, 40_000);
    }
    assert_eq!(stops(&world, actor), Vec::<String>::new(), "walking");

    // 0047C6E0 with a cannon's packets until its health is gone.
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let source = ActorHandle::from_raw(0);
    while world.arena.actor(actor).unwrap().health > 0 {
        let mut damage = WorldDamageOwners {
            grant_text: None,
            camera_settings: None,
            flash: Some(&mut world.flash),
            mission: &mut world.mission,
            tasks: &mut owners.tasks,
            terrain: &mut world.terrain,
            particles: &mut owners.particles,
            sounds: &mut owners.sounds,
            furniture: &mut owners.furniture,
            residency: &runtime,
            executable,
            factory_process: &mut owners.factory_process,
            behavior_tables: &mut owners.behavior_tables,
            animation_clock: &owners.clock,
            session: &mut owners.state,
            particle_load_factor: 0x10000,
            behavior_factory: Some(BehaviorFactoryLoan {
                listeners: &mut world.listeners,
                sine: &sine,
                world_flags: world.world_flags,
            }),
            map: None,
        };
        apply_world_actor_damage(
            &mut world.arena,
            actor,
            ActorDamagePacket {
                strength: 20_000,
                channel: 7,
                byte_03: None,
            },
            source,
            &mut damage,
            &mut world.rng,
        )
        .unwrap();
    }
    for _ in 0..250 {
        owners.tick(&mut world, &runtime, &assets, 40_000);
    }
    assert_eq!(stops(&world, actor), Vec::<String>::new(), "dying");
    // Lifecycle 1 and its wreck's model 311 (rhinodead), which stays.
    let actor = world.arena.actor(actor).unwrap();
    assert_eq!(
        (
            actor.word_c4,
            actor.health,
            actor.rotating.visual.model_word
        ),
        (1, 0, 311)
    );
}
