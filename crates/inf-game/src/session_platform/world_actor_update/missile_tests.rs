use super::super::weapon_test_support::{fire, Owners};
use super::super::WorldGrenadeHost;
use super::*;
use crate::object_runtime::{CameraSettings, ObjectClass, ObjectList};
use crate::session_platform::world_actor_factory::RequiredListenerEffects;
use inf_formats::native_basis::NativeBasisMath;
use inf_formats::FrontendExecutable;

/// Tick a missile once through the world's services.
fn tick(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    missile: ObjectId,
    rng: &mut u32,
) -> Result<(), String> {
    let math = NativeBasisMath::new(*sine);
    let mut effects = RequiredListenerEffects;
    let mut host = WorldGrenadeHost {
        owners: WorldDamageOwners {
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
            behavior_factory: Some(
                crate::session_platform::particles::damage::BehaviorFactoryLoan {
                    listeners: &mut world.listeners,
                    sine,
                    world_flags: world.world_flags,
                },
            ),
            map: Some(&mut owners.map),
        },
        world_flags: world.world_flags,
        clock: 0,
        sine,
        math: &math,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        world_start: &mut world.world_start,
        listener_effects: &mut effects,
        settings_7c: 0,
        inverse_sine: inverse_sine_table(executable).unwrap(),
        voices: None,
    };
    if world.arena.is_missile25(missile) {
        world
            .arena
            .tick_missile25(missile, 125_000, sine, rng, &mut host)
            .map_err(|error| error.to_string())
    } else {
        world
            .arena
            .tick_missile_base(missile, 125_000, sine, rng, &mut host)
            .map_err(|error| error.to_string())
    }
}

/// The missiles on the actor list.
fn missiles(arena: &ObjectArena) -> Vec<ObjectId> {
    arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.is_missile(actor))
        .collect()
}

/// The selector-67 ribbons on the update lists.
fn ribbons(arena: &ObjectArena) -> usize {
    let mut seen: Vec<ObjectId> = Vec::new();
    for list in [ObjectList::Flag1Primary, ObjectList::Flag1Secondary] {
        for node in arena.list(list) {
            let ribbon = matches!(
                arena.object(node.object).map(|object| &object.class),
                Ok(ObjectClass::Selector67(_))
            );
            if ribbon && !seen.contains(&node.object) {
                seen.push(node.object);
            }
        }
    }
    seen.len()
}

fn in_disposal(arena: &ObjectArena, object: ObjectId) -> bool {
    arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == object)
}

/// Fire `row` from the vehicle in Level 1 and run the missile until its
/// fuse runs out: the lifecycle's states 1 and 0, the radius effect of its
/// block, the explosion and its disposal. Returns the missile and its
/// heights above the ground, tick by tick.
fn fly(row: u32, selector: u8) -> (Vec<i32>, usize, bool) {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
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
    let mut rng = 0x2468_ace0;
    assert!(missiles(&world.arena).is_empty());
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        row,
        position,
        &mut rng,
    );
    let found = missiles(&world.arena);
    assert_eq!(found.len(), 1, "row {row} builds one missile");
    let missile = found[0];
    assert_eq!(
        world.arena.behavior_resource(missile).unwrap().selector,
        selector
    );
    let mut heights = Vec::new();
    let mut ticks = 0;
    while !in_disposal(&world.arena, missile) {
        tick(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            missile,
            &mut rng,
        )
        .unwrap();
        let [x, y, z] = world
            .arena
            .object(missile)
            .unwrap()
            .base
            .position_words
            .unwrap();
        heights.push(i32::from(y as i16) - i32::from(world.terrain.height_at([x, z])));
        ticks += 1;
        assert!(ticks < 200, "row {row}'s missile outlives its fuse");
    }
    let ribbons = ribbons(&world.arena);
    let dead = world.arena.actor(missile).unwrap().word_c4 == 0;
    (heights, ribbons, dead)
}

/// Row 4 (registry kind 11, resource 113, payload flags 0x20) fires a
/// selector-28 rocket through the world's child factory. Its update
/// (00457370) lays a ribbon behind it and holds it near 50 above the
/// ground, until the slope ahead of Level 1's start rises faster than a
/// 125 ms tick lets it climb: below the ground its fuse is spent (004574F3)
/// and the lifecycle detonates and disposes it.
#[test]
fn the_vehicles_row_four_rocket_hugs_the_ground_until_a_slope_takes_it() {
    let (heights, ribbons, dead) = fly(4, 28);
    assert_eq!(ribbons, 1);
    assert!(dead);
    let (last, riding) = heights.split_last().unwrap();
    assert!(
        riding.iter().all(|&height| (40..70).contains(&height)),
        "{heights:?}"
    );
    assert!(*last < 0, "{heights:?}");
}

/// Row 5 (registry kind 15, resource 110, registry flag 8) launches a
/// selector-25 missile along the vehicle's basis (0048C8E9). Without a
/// launcher target or an enemy ahead it flies straight, eases to its speed
/// limits and, after its 10 s fuse, detonates through the lifecycle.
#[test]
fn the_vehicles_row_five_missile_flies_out_its_fuse() {
    let (heights, ribbons, dead) = fly(5, 25);
    assert_eq!(ribbons, 1);
    assert!(dead);
    assert!(heights.len() >= 10, "{heights:?}");
}

const HELD_SETTINGS: CameraSettings = CameraSettings {
    first_person: 0,
    camera_style: 0,
};

/// 0046A0E0 on the world's first player: its vehicle takes flag 0x10, the
/// player's unit.
fn mark_the_players_unit(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    owners: &Owners,
    sine: &[u16; 4096],
) -> crate::object_runtime::PlayerId {
    let player = world.arena.first_player().unwrap();
    let mut camera = crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
        crate::session_platform::world_actor_factory::WorldCameraInputs {
            residency: resident,
            sine_quarter: sine,
            settings: HELD_SETTINGS,
            process: Some(&owners.process),
            animation_clock: 0,
        },
    );
    let mut factory = camera.factory(&world.terrain, world.world_flags, &mut world.listeners);
    let null = owners.process.process_null_handle.unwrap();
    world
        .arena
        .player_pre_update(player, null, &mut factory)
        .unwrap();
    player
}

/// One steered flight (00457800) through the world's services, with the
/// camera the hand-over may reach.
fn steer(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    missile: ObjectId,
    block: &crate::object_runtime::PlayerControlBlock,
    rng: &mut u32,
) -> Result<(), String> {
    let math = NativeBasisMath::new(*sine);
    let mut effects = RequiredListenerEffects;
    let mut host = WorldGrenadeHost {
        owners: WorldDamageOwners {
            grant_text: None,
            camera_settings: Some(HELD_SETTINGS),
            flash: Some(&mut world.flash),
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
            behavior_factory: Some(
                crate::session_platform::particles::damage::BehaviorFactoryLoan {
                    listeners: &mut world.listeners,
                    sine,
                    world_flags: world.world_flags,
                },
            ),
            map: Some(&mut owners.map),
        },
        world_flags: world.world_flags,
        clock: 0,
        sine,
        math: &math,
        camera_settings: HELD_SETTINGS,
        world_start: &mut world.world_start,
        listener_effects: &mut effects,
        settings_7c: 0,
        inverse_sine: inverse_sine_table(executable).unwrap(),
        voices: None,
    };
    world
        .arena
        .tick_missile_steered(missile, 125_000, block, sine, rng, &mut host)
        .map_err(|error| error.to_string())
}

/// The camera's target's handle.
fn camera_follows(
    world: &crate::world_backend::WorldSession,
) -> crate::object_runtime::ActorHandle {
    let camera = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector16(actor).is_ok())
        .unwrap();
    let target = world.arena.selector16(camera).unwrap().target_18c.unwrap();
    world.arena.actor_follow_target(target).unwrap().handle_04
}

/// Row 7 (registry kind 28, resource 129) from Artoria's vehicle builds the
/// implant, and the firing arm (0048CAF9) hands it to the vehicle's player:
/// its +D8 is the implant, whose +88 sets the fuse to 30 s and points the
/// camera at it in view mode 2. Steered through the world's services, it
/// thrusts and turns with its engine sound and ribbon; touching a creature
/// (00458DC0) hands the player the creature, the camera follows it and the
/// implant goes.
#[test]
fn the_vehicles_row_seven_implant_takes_the_player_and_then_a_creature() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        11,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let player = mark_the_players_unit(&mut world, &resident, &owners, &sine);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x2468_ace0;
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        7,
        position,
        &mut rng,
    );
    let implant = missiles(&world.arena)
        .into_iter()
        .find(|&missile| world.arena.is_implant(missile))
        .expect("row 7 builds the implant");
    let handle = world.arena.actor(implant).unwrap().handle;
    assert_eq!(world.arena.world_player(player).unwrap().handle_d8, handle);
    assert_eq!(world.arena.missile(implant).unwrap().word_13a, 30_000);
    assert_eq!(world.arena.actor(implant).unwrap().byte_d0, 2);
    assert_eq!(camera_follows(&world), handle);
    let start = world
        .arena
        .object(implant)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut block = crate::object_runtime::PlayerControlBlock {
        thrust_1c: 125_000,
        ..crate::object_runtime::PlayerControlBlock::default()
    };
    for flight in 0..12 {
        if flight == 6 {
            block.turn_14 = 125_000;
        }
        steer(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            implant,
            &block,
            &mut rng,
        )
        .unwrap();
    }
    let end = world
        .arena
        .object(implant)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_ne!(start, end);
    assert_eq!(ribbons(&world.arena), 1);
    assert!(!in_disposal(&world.arena, implant));
    // A creature whose +88 is 00405750: the family 004236D0's classes or
    // selector 12 (Infest04's selector 36 spawned later).
    let creature = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            world.arena.family4236d0_actor(actor).is_ok()
                || world.arena.selector12_actor(actor).is_ok()
        })
        .expect("a creature on Artoria");
    let creature_handle = world.arena.actor(creature).unwrap().handle;
    let taken = {
        let mut damage = WorldDamageOwners {
            grant_text: None,
            camera_settings: Some(HELD_SETTINGS),
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
        };
        world
            .arena
            .implant_accepts(
                implant,
                creature,
                &mut WorldImplantTake {
                    owners: &mut damage,
                },
            )
            .unwrap()
    };
    assert!(taken);
    assert_eq!(
        world.arena.world_player(player).unwrap().handle_d8,
        creature_handle
    );
    assert_eq!(camera_follows(&world), creature_handle);
    assert!(in_disposal(&world.arena, implant));
}

/// With Pilot guided rockets (0xA4), 0046B110 sets the vehicle's row 4 to
/// registry kind 12, resource 112: a selector 28 rocket with flag 0x200, so
/// the firing arm hands it to the player as it does the implant. Held, its
/// own update never runs; steered into the ground it spends its fuse, and
/// the lifecycle flashes the world for the viewed rocket (00456B2E) and
/// detonates it: the radius effect, the explosion and its disposal.
#[test]
fn a_pilot_guided_rocket_flies_under_the_players_hand_and_explodes() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        11,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let player = mark_the_players_unit(&mut world, &resident, &owners, &sine);
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let texts = inf_formats::parse_grant_texts(executable).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable).unwrap();
    world
        .arena
        .grant_player_pickup(
            player,
            0xa4,
            0,
            crate::object_runtime::PickupTables {
                grants: &grants,
                texts: &texts,
                upgrade_limits: &limits,
                network_active: false,
            },
        )
        .unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1357_9bdf;
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        4,
        position,
        &mut rng,
    );
    let rocket = missiles(&world.arena)[0];
    assert_eq!(world.arena.behavior_resource(rocket).unwrap().selector, 28);
    assert_ne!(world.arena.object(rocket).unwrap().base.flags & 0x200, 0);
    let handle = world.arena.actor(rocket).unwrap().handle;
    assert_eq!(world.arena.world_player(player).unwrap().handle_d8, handle);
    assert_eq!(world.arena.actor(rocket).unwrap().byte_d0, 2);
    // Dive it into the ground.
    let block = crate::object_runtime::PlayerControlBlock {
        thrust_1c: 125_000,
        pitch_18: 125_000,
        ..crate::object_runtime::PlayerControlBlock::default()
    };
    let mut flights = 0;
    while !in_disposal(&world.arena, rocket) {
        steer(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            rocket,
            &block,
            &mut rng,
        )
        .unwrap();
        flights += 1;
        assert!(flights < 400, "the rocket outlives its fuse");
    }
    assert_eq!(world.arena.actor(rocket).unwrap().word_c4, 0);
}

/// 00458A50 for Capture the Banner's launcher (selector 94): H13 0x6E
/// builds selector 25 at the point, following the target and facing it,
/// and its first update runs with 0.
#[test]
fn the_launchers_missile_faces_and_follows_its_target() {
    use crate::object_runtime::Selector94Host;
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let at = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let target = world.arena.actor(vehicle).unwrap().handle;
    let start = [at[0].wrapping_add(1500), at[1].wrapping_add(500), at[2]];
    let math = NativeBasisMath::new(sine);
    let mut effects = RequiredListenerEffects;
    let mut rng = 0x1357_9bdf;
    let mut host = WorldGrenadeHost {
        owners: WorldDamageOwners {
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
            map: Some(&mut owners.map),
        },
        world_flags: world.world_flags,
        clock: 0,
        sine: &sine,
        math: &math,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        world_start: &mut world.world_start,
        listener_effects: &mut effects,
        settings_7c: 0,
        inverse_sine: inverse_sine_table(executable).unwrap(),
        voices: None,
    };
    host.fire(&mut world.arena, start, target, &mut rng)
        .unwrap();
    let found = missiles(&world.arena);
    assert_eq!(found.len(), 1);
    let missile = found[0];
    assert!(world.arena.is_missile25(missile));
    let base = &world.arena.object(missile).unwrap().base;
    assert_eq!(base.position_words, Some(start));
    // Its follow target names the vehicle; it faces that target's refreshed
    // point, the offset over its spilled length.
    let crate::object_runtime::Family455560Tail::Selector25 {
        target_160: Some(follow),
        word_158,
        ..
    } = world.arena.missile(missile).unwrap().tail
    else {
        panic!("a selector-25 missile with a target");
    };
    let data = world.arena.actor_follow_target(follow).unwrap();
    assert_eq!(data.handle_04, target);
    let point = data.focus_08.unwrap();
    let offset = std::array::from_fn(|axis| point[axis].wrapping_sub(start[axis]) as i16);
    let forward = inf_formats::native_basis::normalize_words(
        offset,
        inf_formats::native_basis::NativeLengthPrecision::Spilled,
    );
    let basis = world.arena.actor(missile).unwrap().rotating.basis;
    let row: [f32; 3] = std::array::from_fn(|axis| f32::from_bits(basis.words[7 + axis]));
    assert_eq!(row, forward);
    // The first update (with 0) wrote the distance it read only far off.
    assert!(word_158.is_some_and(|distance| distance > 1500));
}
