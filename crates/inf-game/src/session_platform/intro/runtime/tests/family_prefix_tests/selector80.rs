//! World 19's members (resource 0x86, selector 80) over the loaded world and
//! the production updates: raised through the factory where the spawner
//! raises them, deep under the liquid, a member rises to the surface, gives
//! up after five seconds without a target and sinks away.

use super::*;
use crate::object_runtime::{ActorChildResource, CameraSettings, MEMBER_RESOURCE};
use crate::session_platform::world_actor_factory::{
    RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
    WorldFactoryInitialization, WorldFactoryPosition,
};

#[test]
fn world_19s_member_rises_gives_up_and_sinks() {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_with_services(
        19,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    let level = (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16;
    // A point the spawner would take: ground more than 32 under the liquid.
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let [x, _, z] = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let point = (0..64u16)
        .flat_map(|i| (0..64u16).map(move |j| [x.wrapping_add(i * 256), z.wrapping_add(j * 256)]))
        .find(|&at| i32::from(world.terrain.height_at(at)) + 0x20 < i32::from(level))
        .expect("world 19 has deep liquid");
    let math = NativeBasisMath::new(assets.sine_table().values);
    let member = WorldActorChildHost {
        residency: &runtime,
        executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        process: &owners.factory_process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: owners.clock.value(),
        sine: &assets.sine_table().values,
        math: &math,
        camera_settings: Some(CameraSettings {
            first_person: 0,
            camera_style: 0,
        }),
        world_start: Some(&mut world.world_start),
        listeners: &mut world.listeners,
        listener_effects: &mut RequiredListenerEffects,
    }
    .create_object(
        &mut world.arena,
        WorldActorFactoryRequest {
            resource: ActorChildResource::Full(MEMBER_RESOURCE),
            position: WorldFactoryPosition::Local([point[0], 0, point[1]]),
            spawn_mode_1: false,
            network_handle: None,
            initialization: WorldFactoryInitialization::Bare,
        },
        &mut world.rng,
    )
    .unwrap()
    .expect("the member");
    assert!(world.arena.selector80(member).is_ok());
    only_callbacks(&mut world, &[member]);
    let height = |world: &WorldSession| {
        world
            .arena
            .object(member)
            .unwrap()
            .base
            .position_words
            .unwrap()[1] as i16
    };

    // Put a radius under the liquid, then 61 a tick up to it.
    owners.tick(&mut world, &runtime, &assets, 125_000);
    let radius = world.arena.object(member).unwrap().base.word_28;
    assert!(radius > 0);
    assert_eq!(
        i32::from(height(&world)),
        i32::from(level) - i32::from(radius)
    );
    let mut ticks = 1;
    while world.arena.selector80(member).unwrap().rise_1bc == 1 {
        owners.tick(&mut world, &runtime, &assets, 125_000);
        ticks += 1;
        assert!(ticks < 1000);
    }
    assert_eq!(height(&world), level);
    assert_eq!(ticks, 1 + (u32::from(radius) + 60) / 61);

    // Nothing to aim at: five seconds up (the surfacing update counts its
    // delta), then it sinks; not drawn near, it disposes of itself on the
    // first sinking update.
    let up = ticks;
    while world.arena.selector80(member).unwrap().rise_1bc == 0 {
        owners.tick(&mut world, &runtime, &assets, 125_000);
        ticks += 1;
        assert!(ticks < 1000);
    }
    assert_eq!(world.arena.selector80(member).unwrap().rise_1bc, -1);
    assert_eq!(ticks - up, 40);
    owners.tick(&mut world, &runtime, &assets, 125_000);
    assert!(
        world.arena.object(member).is_err()
            || world
                .arena
                .list(ObjectList::Disposal)
                .iter()
                .any(|node| node.object == member)
    );
    assert!(world.actor_updates.selector77_stops.is_empty());
}
