//!Actual loaded classes through the production flag1 manager. Scheduling
//!isolates these callbacks, without replacing their field/list effects.
use super::*;
use crate::object_runtime::{ObjectClass, ObjectClassKind};

#[test]
fn simple_update_production_parks_all_42_terrain_lights_and_ticks_real_camera() {
    let (resident, assets, mut world, _maps) = loaded();
    let mut owners = TickOwners::new(&assets, &mut world, &resident);
    pause_controller(&mut world);
    let lights: Vec<_> = world
        .arena
        .list(ObjectList::Flag1Primary)
        .iter()
        .filter(|node| {
            matches!(
                world.arena.object(node.object).unwrap().class,
                ObjectClass::TerrainLight { .. }
            )
        })
        .map(|node| node.object)
        .collect();
    assert_eq!(lights.len(), 42);
    let camera = world
        .arena
        .list(ObjectList::Flag1Primary)
        .iter()
        .find(|node| world.arena.selector16(node.object).is_ok())
        .unwrap()
        .object;
    let mut selected = lights.clone();
    selected.push(camera);
    only_callbacks(&mut world, &selected);
    //The real camera constructor supplies zero. A supplied timer makes the
    //countdown visible without pretending camera pose produced this WORD.
    world
        .arena
        .with_supplied_family_dispatch_actor(camera, |actor| actor.word_c8 = 100)
        .unwrap();
    let states: Vec<_> = lights
        .iter()
        .map(|id| world.arena.object(*id).unwrap().clone())
        .collect();
    let before_rng = world.rng;
    owners.tick(&mut world, &resident, &assets, 64);
    assert_eq!(world.rng, before_rng);
    assert_eq!(world.arena.actor(camera).unwrap().word_c8, 98);
    assert_eq!(callback_order(&world), vec![camera]);
    for (light, before) in lights.iter().zip(&states) {
        let mut expected = before.clone();
        expected.base.flags &= !2;
        expected.base.dirty_word |= 2;
        assert_eq!(world.arena.object(*light).unwrap(), &expected);
        assert!(!world
            .actor_updates
            .skipped_callbacks
            .iter()
            .any(|(id, _)| id == light));
    }
    assert!(!world
        .actor_updates
        .skipped_callbacks
        .iter()
        .any(|(id, _)| *id == camera));
    owners.tick(&mut world, &resident, &assets, 32);
    assert_eq!(world.arena.actor(camera).unwrap().word_c8, 97);
    assert_eq!(callback_order(&world), vec![camera]);
}

#[test]
fn simple_update_alternate_forwarders_preserve_unowned_normal_effects() {
    for kind in [ObjectClassKind::Selector86, ObjectClassKind::TimedPickup] {
        let mut arena = ObjectArena::new();
        let object = arena.allocate(kind);
        let before = arena.object(object).unwrap().clone();
        assert!(
            !crate::session_platform::world_actor_update::tick_simple_object_callback(
                &mut arena, object, false, 100
            )
            .unwrap()
        );
        assert_eq!(arena.object(object).unwrap(), &before);
        assert!(
            crate::session_platform::world_actor_update::tick_simple_object_callback(
                &mut arena, object, true, 100
            )
            .unwrap()
        );
        assert_eq!(arena.object(object).unwrap().class, before.class);
        assert_eq!(
            arena.object(object).unwrap().base.flags,
            before.base.flags & !2
        );
        assert!(arena.list(ObjectList::Flag1Primary).is_empty());
    }
}
