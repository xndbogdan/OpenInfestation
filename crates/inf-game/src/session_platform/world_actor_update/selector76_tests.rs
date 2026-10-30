use super::super::weapon_test_support::{disposed, fire, Owners};
use super::*;
use crate::object_runtime::{ObjectClass, ObjectId};
use inf_formats::FrontendExecutable;

fn flamethrowers(arena: &ObjectArena) -> Vec<ObjectId> {
    let mut found = Vec::new();
    for list in [
        crate::object_runtime::ObjectList::Flag1Primary,
        crate::object_runtime::ObjectList::Flag1Secondary,
    ] {
        for node in arena.list(list) {
            if matches!(
                arena.object(node.object).map(|object| &object.class),
                Ok(ObjectClass::Selector76(_))
            ) && !found.contains(&node.object)
            {
                found.push(node.object);
            }
        }
    }
    found
}

/// The vehicle's row 3 (registry kind 10, resource 0x82) in world 6, where
/// Infest03 fired it: 00431EF0 builds the flamethrower through the world's
/// registry construction, keeps the vehicle's handle and roots the flame
/// one model radius along the vehicle's forward row. Through the world's
/// flames and burns it grows to 240, 540 and 600, and goes.
#[test]
fn the_vehicles_flamethrower_burns_out_in_three_ticks() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    assert!(flamethrowers(&world.arena).is_empty());
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        3,
        position,
        &mut rng,
    );
    let found = flamethrowers(&world.arena);
    assert_eq!(found.len(), 1);
    let flame = found[0];
    {
        let state = world.arena.selector76(flame).unwrap();
        let vehicle_state = world.arena.actor(vehicle).unwrap();
        assert_eq!(state.owner_6c, Some(vehicle_state.handle));
        assert_eq!(state.owner_position_78, Some(position));
        assert_eq!((state.dword_70, state.dword_74), (1, 0));
        let reach = f64::from(vehicle_state.rotating.visual.radius_word as i16);
        let forward = [7, 8, 9]
            .map(|word| f64::from(f32::from_bits(vehicle_state.rotating.basis.words[word])));
        let root: [u16; 3] = std::array::from_fn(|axis| {
            ((reach * forward[axis]).trunc() as i32 as u16).wrapping_add(position[axis])
        });
        assert_eq!(state.root_7e, Some(root));
    }
    for (tick, length) in [(1, 240), (2, 540), (3, 600)] {
        let mut host = WorldFlameHost {
            owners: crate::session_platform::particles::damage::WorldDamageOwners {
                grant_text: None,
                camera_settings: None,
                flash: None,
                mission: &mut world.mission,
                tasks: &mut owners.tasks,
                terrain: &mut world.terrain,
                particles: &mut owners.particles,
                sounds: &mut owners.sounds,
                furniture: &mut owners.furniture,
                residency: &resident,
                executable,
                factory_process: &mut owners.process,
                behavior_tables: &mut owners.tables,
                animation_clock: &owners.clock,
                session: &owners.state,
                particle_load_factor: 0x10000,
                behavior_factory: Some(
                    crate::session_platform::particles::damage::BehaviorFactoryLoan {
                        listeners: &mut world.listeners,
                        sine: &sine,
                        world_flags: world.world_flags,
                    },
                ),
                map: None,
            },
            rng: &mut rng,
        };
        world
            .arena
            .tick_selector76(flame, 125_000, &mut host)
            .unwrap();
        assert_eq!(
            world.arena.selector76(flame).unwrap().dword_74,
            length,
            "tick {tick}"
        );
    }
    assert_eq!(world.arena.selector76(flame).unwrap().dword_70, 0xffff);
    assert!(disposed(&world.arena, flame));
}
