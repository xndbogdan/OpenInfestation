//! Deaths inside the world's lifecycle, on Artoria's (world 11) Powerball.

use super::super::weapon_test_support::Owners;
use crate::object_runtime::{ObjectClass, ObjectId, ObjectList};
use crate::session_platform::particles::damage::{
    world_retirement_host, BehaviorFactoryLoan, WorldDamageOwners,
};
use inf_formats::FrontendExecutable;

/// Retire `actor` (0047CE90) through the world's lifecycle host.
fn retire(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    actor: ObjectId,
    lend_map: bool,
) -> Result<(), String> {
    let mut damage = WorldDamageOwners {
        grant_text: None,
        camera_settings: None,
        flash: None,
        mission: &mut world.mission,
        tasks: &mut owners.tasks,
        terrain: &mut world.terrain,
        particles: &mut owners.particles,
        sounds: &mut owners.sounds,
        furniture: &mut owners.furniture,
        residency: resident,
        executable,
        factory_process: &mut owners.process,
        behavior_tables: &mut owners.tables,
        animation_clock: &owners.clock,
        session: &owners.state,
        particle_load_factor: 0x10000,
        behavior_factory: Some(BehaviorFactoryLoan {
            listeners: &mut world.listeners,
            sine,
            world_flags: world.world_flags,
        }),
        map: lend_map.then_some(&mut owners.map),
    };
    let null = damage.factory_process.process_null_handle;
    let mut rng = 0x1234_5678;
    let mut host = world_retirement_host(&mut damage, &mut rng);
    world
        .arena
        .retire_actor(actor, null, &mut host)
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
}

/// Objects built with `resource` that are not yet queued for disposal.
fn built(arena: &crate::object_runtime::ObjectArena, resource: u16) -> Vec<ObjectId> {
    let mut seen = Vec::new();
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
    ] {
        for node in arena.list(list) {
            let object = arena.object(node.object).unwrap();
            if object.base.resource_word == resource && !seen.contains(&node.object) {
                seen.push(node.object);
            }
        }
    }
    seen
}

/// 00486420 in the world: the visual 0x36 is built, the call reaches the
/// actors within 2000, and the ball is queued for disposal.
#[test]
fn artorias_powerball_dies_through_the_world() {
    // Grant 0x30 keeps the ball, which its command 13 otherwise disposes.
    let (resident, assets, mut world) = crate::test_worlds::offline_world_from_saved_start(
        11,
        0,
        &[0x30],
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = Owners::new(executable);
    let sine = assets.sine_table().values;
    let ball = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector57(actor).is_ok())
        .expect("Artoria places a Powerball");
    let position = world
        .arena
        .object(ball)
        .unwrap()
        .base
        .position_words
        .unwrap();
    // The vehicle's radar jam needs the map the lifecycle does not lend; keep
    // it beyond the call.
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let far = [position[0].wrapping_add(0x4000), position[1], position[2]];
    world.arena.copy_actor_position(vehicle, far).unwrap();
    // A generator beside the ball takes message 0xC (0043BDF0: +164 = 1).
    let generator = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector39(actor).is_ok())
        .unwrap();
    let near = [position[0].wrapping_add(100), position[1], position[2]];
    world.arena.copy_actor_position(generator, near).unwrap();
    assert_eq!(world.arena.selector39(generator).unwrap().byte_164, 0);
    let visuals = built(&world.arena, 0x36).len();
    // Outside gameplay the burst's births take the scratch record (00464ED0).
    retire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        ball,
        false,
    )
    .unwrap();
    assert_eq!(built(&world.arena, 0x36).len(), visuals + 1);
    assert_eq!(world.arena.selector39(generator).unwrap().byte_164, 1);
    assert!(world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == ball));
    assert!(matches!(
        world.arena.object(ball).unwrap().class,
        ObjectClass::Actor(_)
    ));
}

/// The player's vehicle within 2000 answers message 0xC with its radar jam
/// (00420BE0), which needs the world's map; the lifecycle lends none, so
/// the death stops there, after its visual and burst.
#[test]
fn a_vehicle_near_the_dying_powerball_needs_the_map() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_from_saved_start(
        11,
        0,
        &[0x30],
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = Owners::new(executable);
    let sine = assets.sine_table().values;
    let ball = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector57(actor).is_ok())
        .unwrap();
    let position = world
        .arena
        .object(ball)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let near = [position[0].wrapping_add(100), position[1], position[2]];
    world.arena.copy_actor_position(vehicle, near).unwrap();
    let visuals = built(&world.arena, 0x36).len();
    let error = retire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        ball,
        false,
    )
    .unwrap_err();
    assert!(error.contains("radar jam"), "{error}");
    assert_eq!(built(&world.arena, 0x36).len(), visuals + 1);
    assert!(!world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == ball));
}

/// With the world's map lent, as the contact pass and the actor updates lend
/// it, the vehicle 100 from the dying Powerball takes its radar jam
/// (`(2000 - 100) * 0xFFFF / 2000`) and the death runs to its disposal.
#[test]
fn the_dying_powerball_jams_a_near_vehicles_radar() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_from_saved_start(
        11,
        0,
        &[0x30],
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = Owners::new(executable);
    let sine = assets.sine_table().values;
    let ball = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector57(actor).is_ok())
        .unwrap();
    let position = world
        .arena
        .object(ball)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let near = [position[0].wrapping_add(100), position[1], position[2]];
    world.arena.copy_actor_position(vehicle, near).unwrap();
    retire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        ball,
        true,
    )
    .unwrap();
    assert_eq!(owners.map.jitter_radius(), (2000 - 100) * 0xffff / 2000);
    assert!(world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == ball));
}
