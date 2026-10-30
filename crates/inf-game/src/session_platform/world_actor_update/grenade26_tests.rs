use super::super::weapon_test_support::{disposed, emit, fire, Owners};
use super::*;
use crate::object_runtime::NativeWeaponFireNode;
use crate::session_platform::world_actor_factory::RequiredListenerEffects;
use inf_formats::FrontendExecutable;

/// Tick a grenade or mine once through the world's services.
fn tick(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    grenade: ObjectId,
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
        inverse_sine: [0; 1024],
        voices: None,
    };
    if world.arena.is_bomb29(grenade) {
        world
            .arena
            .tick_bomb29(grenade, 125_000, rng, &mut host)
            .map_err(|error| error.to_string())
    } else if world.arena.is_mine38(grenade) {
        world
            .arena
            .tick_mine38(grenade, 125_000, sine, rng, &mut host)
            .map_err(|error| error.to_string())
    } else if world.arena.is_charge27(grenade) {
        let null = host.owners.factory_process.process_null_handle.unwrap();
        world
            .arena
            .tick_charge27(grenade, 125_000, null, sine, rng, &mut host)
            .map_err(|error| error.to_string())
    } else {
        world
            .arena
            .tick_grenade26(grenade, 125_000, sine, rng, &mut host)
            .map_err(|error| error.to_string())
    }
}

/// The selector-22 visuals on the flag lists.
fn visuals(arena: &ObjectArena) -> usize {
    use crate::object_runtime::{ObjectClass, ObjectList};
    let mut seen: Vec<ObjectId> = Vec::new();
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
    ] {
        for node in arena.list(list) {
            let visual = matches!(
                arena.object(node.object).map(|object| &object.class),
                Ok(ObjectClass::Selector22(_))
            );
            if visual && !seen.contains(&node.object) {
                seen.push(node.object);
            }
        }
    }
    seen.len()
}

fn detonating(arena: &ObjectArena, grenade: ObjectId) -> bool {
    match &arena.thrown(grenade).unwrap().tail {
        crate::object_runtime::Family455560Tail::Flags14c { byte_14e, .. }
        | crate::object_runtime::Family455560Tail::Selector38 { byte_14e, .. } => *byte_14e != 0,
        other => panic!("a thrown tail, not {other:?}"),
    }
}

/// Player command 0x30's bit in +130 (word 1, bit 16).
fn hint_shown(arena: &ObjectArena, player: crate::object_runtime::PlayerId) -> bool {
    arena.world_player(player).unwrap().command_mask_130[1] & (1 << 16) != 0
}

/// The grenades and mines on the actor list.
fn grenades(arena: &ObjectArena) -> Vec<ObjectId> {
    arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.is_thrown(actor))
        .collect()
}

/// The vehicle's row 8 (registry kind 14, resource 119) throws a standard
/// grenade through the world's child factory: selector 26 at the emission
/// point with the emission velocity, its 2 s fuse and Level 1's block.
/// Run to its last 125 ms, it detonates through the lifecycle: state 1 and
/// 0, the default burst's selector-22 visual (resource 54) and debris, the
/// radius effect that kills the alien it was thrown at and reaches the
/// grenade's own +B4 (the shared 0047CBC0), the explosion, and its disposal.
#[test]
fn a_thrown_grenade_detonates_next_to_an_alien() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let alien = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            actor != vehicle
                && world.arena.family_update_class(actor).is_ok()
                && world.arena.actor(actor).unwrap().health > 0
        })
        .expect("an alien in Level 1");
    let target = world
        .arena
        .object(alien)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let health = world.arena.actor(alien).unwrap().health;
    let mut rng = 0x1234_5678;
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        8,
        target,
        &mut rng,
    );
    let thrown = grenades(&world.arena);
    assert_eq!(thrown.len(), 1);
    let grenade = thrown[0];
    {
        let state = world.arena.grenade26(grenade).unwrap();
        assert_eq!(state.word_13a, 2000);
        assert_eq!(state.dword_148, 54);
        assert_eq!(world.arena.object(grenade).unwrap().base.resource_word, 119);
        assert_eq!(
            world.arena.actor(grenade).unwrap().owner_handle,
            world.arena.actor(vehicle).unwrap().handle
        );
    }
    // Its last 125 ms of fuse, at the alien.
    world.arena.grenade26_set_fuse_for_tests(grenade, 100);
    let before = visuals(&world.arena);
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        grenade,
        &mut rng,
    )
    .unwrap();
    assert_eq!(world.arena.actor(grenade).unwrap().word_c4, 0);
    assert!(disposed(&world.arena, grenade));
    assert_eq!(visuals(&world.arena), before + 1);
    // 1500 on channel 9 at the alien's own position kills it (300 health);
    // the grenade's own hit (0047CBC0, state 0) changes nothing more.
    assert_eq!(health, 300);
    let alien_state = world.arena.actor(alien).unwrap();
    assert_eq!((alien_state.health, alien_state.word_c4), (0, 1));
}

/// A grenade thrown in weapon group 0 leaves its handle in behavior +24,
/// where the cannon (registry flag 2) looks for its group's earlier shot:
/// the grenade's 004557F0 answers event 2 with 0, so the cannon builds a
/// shot of its own and the grenade is not marked.
#[test]
fn a_grenade_in_the_group_leaves_the_cannon_its_own_shot() {
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
    let mut rng = 0x1234_5678;
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        8,
        position,
        &mut rng,
    );
    let grenade = grenades(&world.arena)[0];
    let group = |world: &crate::world_backend::WorldSession| {
        world
            .arena
            .derived_actor(vehicle)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .words_24[0]
    };
    assert_eq!(
        group(&world),
        Some(world.arena.actor(grenade).unwrap().handle.raw())
    );
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        0,
        position,
        &mut rng,
    );
    let shots: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.is_shot32(actor))
        .collect();
    assert_eq!(shots.len(), 1);
    assert_eq!(
        group(&world),
        Some(world.arena.actor(shots[0]).unwrap().handle.raw())
    );
    assert!(!detonating(&world.arena, grenade));
}

/// The detonator (row 6, registry kind 23, resource 0) sends event 10 to
/// the actors within 4000 of the vehicle: its own grenade marks itself to
/// detonate (+14E); a grenade of another owner does not. The player gets
/// hint 0x30.
#[test]
fn the_detonator_marks_only_the_vehicles_grenades() {
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
    let mut rng = 0x1234_5678;
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        8,
        position,
        &mut rng,
    );
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        8,
        position,
        &mut rng,
    );
    let thrown = grenades(&world.arena);
    assert_eq!(thrown.len(), 2);
    world
        .arena
        .grenade26_set_owner_for_tests(thrown[1], ActorHandle::from_raw(0x0bad_0001));
    let player = world.arena.first_player().unwrap();
    assert!(!hint_shown(&world.arena, player));
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        6,
        position,
        &mut rng,
    );
    assert!(detonating(&world.arena, thrown[0]));
    assert!(!detonating(&world.arena, thrown[1]));
    assert!(hint_shown(&world.arena, player));
}

/// With Sonic Shock (`0x26`) the detonator sends its pulse `0048D980`
/// instead: event 9 to every actor within 4000 of the vehicle sets off its
/// own grenade and another owner's alike, but not one 5000 away; the map's
/// jitter goes to 0x1FFF and the world flashes white at the vehicle, where
/// the camera looks on. The player gets no hint.
#[test]
fn sonic_shock_sets_off_every_grenade_in_reach() {
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
    let mut rng = 0x1234_5678;
    for _ in 0..3 {
        fire(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            8,
            position,
            &mut rng,
        );
    }
    let thrown = grenades(&world.arena);
    assert_eq!(thrown.len(), 3);
    world
        .arena
        .grenade26_set_owner_for_tests(thrown[1], ActorHandle::from_raw(0x0bad_0001));
    let far = [position[0].wrapping_add(5000), position[1], position[2]];
    world.arena.set_position_for_tests(thrown[2], far);
    let player = world.arena.first_player().unwrap();
    world.arena.grant_player_upgrade_for_tests(player, 0x26);
    // The camera the frame is drawn from, looking on from the vehicle.
    let mut camera = crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
        crate::session_platform::world_actor_factory::WorldCameraInputs {
            residency: &resident,
            sine_quarter: &sine,
            settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            process: Some(&owners.process),
            animation_clock: 0,
        },
    );
    let camera = world
        .arena
        .world_camera_with_rng(
            &mut camera.factory(&world.terrain, world.world_flags, &mut world.listeners),
            Some(&mut rng),
        )
        .unwrap()
        .unwrap();
    world.arena.set_position_for_tests(camera, position);
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        6,
        position,
        &mut rng,
    );
    assert!(detonating(&world.arena, thrown[0]));
    assert!(detonating(&world.arena, thrown[1]));
    assert!(!detonating(&world.arena, thrown[2]));
    assert_eq!(owners.map.jitter_radius(), 0x1fff);
    assert_eq!(world.flash.sequence_5b8, 1);
    assert!(world.flash.color_5bc.is_some());
    assert!(!hint_shown(&world.arena, player));
}

/// One grenade throw 0048C890 recorded in a retail run: the node, the
/// emission point, the vehicle's velocity (+8E) and basis (+44..+6B), the
/// RNG word, and what differs between the grenades when the emitter
/// returns.
struct Throw {
    at: &'static str,
    direction_bits: [u32; 3],
    group: u32,
    selection: u32,
    speed_scale: u32,
    position: [u16; 3],
    vehicle_velocity: [u16; 3],
    vehicle_basis: [u32; 10],
    rng: u32,
    word_28: u16,
    velocity: [u16; 3],
    block: [u8; 10],
    resource_148: u32,
    payload: u8,
    /// Health (+A0) and byte +138: 100 and 1 on a grenade, 1 and 0 on a
    /// mine, which its update arms later.
    health: u16,
    byte_138: u8,
}

/// Infest02's three standard grenades on Level 1 (row 8: kind 14,
/// resource 119).
const INFEST02_THROWS: [Throw; 3] = [
    Throw {
        at: "48D41A:2468",
        direction_bits: [0xbea4_c0d3, 0x3ed9_4b08, 0xbf58_ab99],
        group: 0,
        selection: 0x1_0008,
        speed_scale: 0,
        position: [13485, 9, 36613],
        vehicle_velocity: [0, 65533, 0],
        vehicle_basis: [
            0x60,
            0xbf6f_46f9,
            0x3c7f_f356,
            0x3eb5_ce26,
            0x3bf6_0284,
            0x3f7f_e43c,
            0xbcc6_3d72,
            0xbeb5_e07c,
            0xbca3_751f,
            0xbf6f_3991,
        ],
        rng: 0x9729_1d4b,
        word_28: 31,
        velocity: [64893, 845, 63844],
        block: [220, 5, 9, 0, 38, 2, 132, 3, 94, 1],
        resource_148: 54,
        payload: 0,
        health: 100,
        byte_138: 1,
    },
    Throw {
        at: "4A1572:1DB3",
        direction_bits: [0xbea1_bd7f, 0x3ee8_3025, 0xbf55_5996],
        group: 1,
        selection: 0x1_0008,
        speed_scale: 0,
        position: [13425, 7, 36453],
        vehicle_velocity: [65468, 65531, 65356],
        vehicle_basis: [
            0x39,
            0xbf6f_5c2f,
            0x3af5_cb50,
            0x3eb5_8f34,
            0x3c13_7588,
            0x3f7f_e694,
            0x3c97_0f54,
            0xbeb5_7d49,
            0x3ca7_648b,
            0xbf6f_4651,
        ],
        rng: 0xaf08_4924,
        word_28: 31,
        velocity: [64837, 901, 63690],
        block: [220, 5, 9, 0, 38, 2, 132, 3, 94, 1],
        resource_148: 54,
        payload: 0,
        health: 100,
        byte_138: 1,
    },
    Throw {
        at: "4C2747:DA0",
        direction_bits: [0xbea3_2e8e, 0x3edf_cc76, 0xbf57_4ece],
        group: 0,
        selection: 0x1_0008,
        speed_scale: 0,
        position: [13271, 7, 36024],
        vehicle_velocity: [0, 65531, 0],
        vehicle_basis: [
            0x45,
            0xbf6f_59e6,
            0xbbb4_1f93,
            0x3eb5_71fc,
            0xbbc5_e776,
            0x3f7f_f0ff,
            0xb9de_b081,
            0xbeb5_6316,
            0xbb26_524d,
            0xbf6f_5e3f,
        ],
        rng: 0x428c_cc8f,
        word_28: 31,
        velocity: [64899, 869, 63854],
        block: [220, 5, 9, 0, 38, 2, 132, 3, 94, 1],
        resource_148: 54,
        payload: 0,
        health: 100,
        byte_138: 1,
    },
];

/// Infest03's chain grenade in world 11 (row 14: kind 22, resource 124,
/// payload flag 2), thrown with speed scale A7C2.
const INFEST03_THROWS: [Throw; 1] = [Throw {
    at: "18F483:1ECB",
    direction_bits: [0x3f1a_a2b4, 0x3ee7_48ae, 0xbf28_1467],
    group: 0,
    selection: 0x1_000e,
    speed_scale: 0xa7c2,
    position: [15908, 61571, 3181],
    vehicle_velocity: [519, 106, 64492],
    vehicle_basis: [
        0x1b,
        0xbf3c_68dd,
        0x3cc5_d64a,
        0xbf2d_32ce,
        0x3bb7_7a55,
        0x3f7f_dd9b,
        0x3cf2_6197,
        0x3f2d_4b6a,
        0x3c93_5a66,
        0xbf3c_5a05,
    ],
    rng: 0x27b5_6164,
    word_28: 44,
    velocity: [1310, 697, 63632],
    block: [0; 10],
    resource_148: 57,
    payload: 2,
    health: 100,
    byte_138: 1,
}];

/// Infest03's two mines in world 6 (row 13: kind 21, resource 118, payload
/// flag 8, flat, trigger radius 600), the first thrown with speed scale
/// C647.
const INFEST03_MINE_THROWS: [Throw; 2] = [
    Throw {
        at: "2B2943:1CBC",
        direction_bits: [0xbf66_0aa1, 0x3ee0_a662, 0],
        group: 0,
        selection: 0x1_000d,
        speed_scale: 0xc647,
        position: [40316, 64393, 19314],
        vehicle_velocity: [0, 0, 0],
        vehicle_basis: [3, 0, 0, 0x3f80_0000, 0, 0x3f80_0000, 0, 0xbf80_0000, 0, 0],
        rng: 0x426f_a83d,
        word_28: 36,
        velocity: [64145, 679, 0],
        block: [184, 11, 9, 0, 200, 0, 32, 3, 200, 0],
        resource_148: 54,
        payload: 8,
        health: 1,
        byte_138: 0,
    },
    Throw {
        at: "32C051:153",
        direction_bits: [0xbf3f_cb8d, 0x3ee1_8770, 0x3efd_41d3],
        group: 1,
        selection: 0x1_000d,
        speed_scale: 0,
        position: [38806, 64374, 20251],
        vehicle_velocity: [64732, 0, 531],
        vehicle_basis: [
            0x45,
            0x3f0c_ff1b,
            0x3c26_d403,
            0x3f55_97f5,
            0xbb6e_8175,
            0x3f7f_f9ec,
            0xbc20_930d,
            0xbf55_9def,
            0x3b1b_eecc,
            0x3f0d_0016,
        ],
        rng: 0xb63b_b60a,
        word_28: 36,
        velocity: [63234, 880, 1520],
        block: [184, 11, 9, 0, 200, 0, 32, 3, 200, 0],
        resource_148: 54,
        payload: 8,
        health: 1,
        byte_138: 0,
    },
];

/// Every grenade and mine throw recorded: the emitter builds it at the
/// emission point through the vehicle's child factory without a draw, and
/// it holds what retail's did when 0048C890 returned. Its radius
/// (resource 119's 31, 124's 44, 118's 36), flags 48100807, the vehicle's
/// basis and handle as owner, the speed (2000, scaled) along the node plus
/// the vehicle's velocity, still angular words, its health in state 3,
/// byte +138, the 2 s fuse at age 0, the resource's block and visual, the
/// payload flags and the burn's 5 s; a mine is flat with trigger radius
/// 600, no target and no tracked distance.
#[test]
fn grenade_throws_match_the_recorded_emissions() {
    for (selection, throws) in [
        (18, &INFEST02_THROWS[..]),
        (11, &INFEST03_THROWS[..]),
        (6, &INFEST03_MINE_THROWS[..]),
    ] {
        let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
            selection,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let sine = assets.sine_table().values;
        let mut owners = Owners::new(executable);
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        for throw in throws {
            world
                .arena
                .with_supplied_family_dispatch_actor(vehicle, |state| {
                    state.words_8e_92 = throw.vehicle_velocity;
                    state.rotating.basis.words = throw.vehicle_basis;
                })
                .unwrap();
            let before = grenades(&world.arena);
            let mut rng = throw.rng;
            let node = NativeWeaponFireNode {
                direction_bits: throw.direction_bits,
                compensation: 0,
                group: throw.group,
                selection: throw.selection,
                speed_scale: throw.speed_scale,
                next: None,
            };
            emit(
                &mut world,
                &resident,
                executable,
                &mut owners,
                &sine,
                node,
                throw.position,
                &mut rng,
            );
            assert_eq!(rng, throw.rng, "{}: a throw draws nothing", throw.at);
            let thrown: Vec<_> = grenades(&world.arena)
                .into_iter()
                .filter(|grenade| !before.contains(grenade))
                .collect();
            assert_eq!(thrown.len(), 1, "{}", throw.at);
            let grenade = thrown[0];
            let object = world.arena.object(grenade).unwrap();
            let actor = world.arena.actor(grenade).unwrap();
            let state = world.arena.thrown(grenade).unwrap();
            let (byte_14c, byte_14d, byte_14e, byte_14f, dword_150) = match &state.tail {
                crate::object_runtime::Family455560Tail::Flags14c {
                    byte_14c,
                    byte_14d,
                    byte_14e,
                    byte_14f,
                    dword_150,
                    ..
                } => (byte_14c, byte_14d, byte_14e, byte_14f, dword_150),
                crate::object_runtime::Family455560Tail::Selector38 {
                    byte_14c,
                    byte_14d,
                    byte_14e,
                    byte_14f,
                    dword_150,
                    byte_154,
                    dword_158,
                    object_15c,
                    dword_160,
                } => {
                    assert_eq!(
                        (*byte_154, *dword_158, *object_15c, *dword_160),
                        (Some(1), Some(600), None, 0),
                        "{}",
                        throw.at
                    );
                    (byte_14c, byte_14d, byte_14e, byte_14f, dword_150)
                }
                other => panic!("{}: a thrown tail, not {other:?}", throw.at),
            };
            let vehicle_handle = world.arena.actor(vehicle).unwrap().handle;
            assert_eq!(
                (
                    object.base.word_28,
                    object.base.flags,
                    object.base.position_words,
                    actor.rotating.basis.words,
                    actor.owner_handle,
                    actor.words_8e_92,
                    actor.words_94_9c,
                    actor.health,
                    actor.word_c4,
                ),
                (
                    throw.word_28,
                    0x4810_0807,
                    Some(throw.position),
                    throw.vehicle_basis,
                    vehicle_handle,
                    throw.velocity,
                    [0; 3],
                    throw.health,
                    3,
                ),
                "{}",
                throw.at
            );
            assert_eq!(
                (
                    state.byte_138,
                    state.word_13a,
                    state.word_13c,
                    state.bytes_13e,
                    state.dword_148,
                    [*byte_14c, *byte_14d],
                    [*byte_14e, *byte_14f],
                    *dword_150,
                ),
                (
                    throw.byte_138,
                    2000,
                    0,
                    Some(throw.block),
                    throw.resource_148,
                    [Some(throw.payload), Some(0)],
                    [0, 0],
                    5_000_000,
                ),
                "{}",
                throw.at
            );
        }
    }
}

/// Fire `row` from the vehicle at `position` and still the new grenade or
/// mine, so a test decides where it is when it goes off.
fn place(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    row: u32,
    position: [u16; 3],
    rng: &mut u32,
) -> ObjectId {
    let before = grenades(&world.arena);
    fire(
        world, resident, executable, owners, sine, row, position, rng,
    );
    let thrown: Vec<_> = grenades(&world.arena)
        .into_iter()
        .filter(|grenade| !before.contains(grenade))
        .collect();
    assert_eq!(thrown.len(), 1);
    world
        .arena
        .with_supplied_family_dispatch_actor(thrown[0], |state| state.words_8e_92 = [0; 3])
        .unwrap();
    thrown[0]
}

fn squared_distance(arena: &ObjectArena, a: ObjectId, b: ObjectId) -> i32 {
    let a = arena.object(a).unwrap().base.position_words.unwrap();
    let b = arena.object(b).unwrap().base.position_words.unwrap();
    (0..3).fold(0_i32, |sum, axis| {
        let delta = i32::from(a[axis].wrapping_sub(b[axis]) as i16);
        sum.wrapping_add(delta * delta)
    })
}

/// A chain grenade (row 14: kind 22, resource 124, payload flag 2) going
/// off 500 from the vehicle in world 11 broadcasts event 0xB within 2000
/// (0047DD20): the vehicle's 00420BE0 jams its player's radar, the map's
/// jitter (2000 - distance) * 0xFFFF / 2000, as Infest03's did. The other
/// receivers ignore it, and the grenade bursts and is disposed.
#[test]
fn a_chain_grenade_jams_the_vehicles_radar() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        11,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let [x, y, z] = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    let grenade = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        14,
        [
            x.wrapping_add(300),
            y.wrapping_add(200),
            z.wrapping_add(400),
        ],
        &mut rng,
    );
    assert_eq!(world.arena.grenade26(grenade).unwrap().dword_148, 57);
    world.arena.grenade26_set_fuse_for_tests(grenade, 100);
    assert_eq!(owners.map.jitter_radius(), 0);
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        grenade,
        &mut rng,
    )
    .unwrap();
    assert!(disposed(&world.arena, grenade));
    let distance =
        inf_formats::native_basis::isqrt_48efc0(squared_distance(&world.arena, grenade, vehicle))
            as i32;
    assert!(distance < 2000, "{distance}");
    assert_eq!(
        owners.map.jitter_radius(),
        (2000 - distance) * 0xffff / 2000
    );
}

/// A mine (row 13: kind 21, resource 118) laid near the vehicle in world 6
/// goes off when the detonator marks it, as Infest03's did: the flat burst
/// builds visual 0x3A, its block applies 1500 above it, the vehicle hears
/// 0x13 (nothing offline), and the mine bursts and is disposed.
#[test]
fn the_detonator_sets_off_the_vehicles_mine() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let [x, y, z] = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    let mine = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        13,
        [x.wrapping_add(300), y, z],
        &mut rng,
    );
    assert!(world.arena.is_mine38(mine));
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        6,
        position,
        &mut rng,
    );
    assert!(detonating(&world.arena, mine));
    let before = visuals(&world.arena);
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        mine,
        &mut rng,
    )
    .unwrap();
    assert!(disposed(&world.arena, mine));
    assert_eq!(visuals(&world.arena), before + 1);
    assert_eq!(world.arena.thrown(mine).unwrap().dword_148, 0x38);
}

/// Helniazine's two Bombs (selector 29) lie on the ground under them,
/// disarmed. One the vehicle owns (as if it had carried it) arms when the
/// detonator (row 6) calls the actors within 4000 with event 10; the other,
/// owned by no one there, stays disarmed. The armed one's next update spends
/// its fuse: state 1 and 0, its visual (resource 54) and burst, the blast of
/// its block from its owner, the explosion and its disposal. The blast kills
/// the other, which goes off too.
#[test]
fn the_detonator_sets_off_the_vehicles_bomb_and_its_neighbour() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let bombs: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.is_bomb29(actor))
        .collect();
    assert_eq!(bombs.len(), 2);
    for &bomb in &bombs {
        let [x, y, z] = world
            .arena
            .object(bomb)
            .unwrap()
            .base
            .position_words
            .unwrap();
        assert_eq!(y, world.terrain.height_at([x, z]) as u16);
        assert_eq!(world.arena.bomb29(bomb).unwrap().byte_138, 0);
    }
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let handle = world.arena.actor(vehicle).unwrap().handle;
    world
        .arena
        .with_supplied_family_dispatch_actor(bombs[0], |state| state.owner_handle = handle)
        .unwrap();
    let [x, y, z] = world
        .arena
        .object(bombs[0])
        .unwrap()
        .base
        .position_words
        .unwrap();
    let near = [x.wrapping_add(500), y, z];
    world.arena.copy_actor_position(vehicle, near).unwrap();
    let mut rng = 0x1234_5678;
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        6,
        near,
        &mut rng,
    );
    let armed = |world: &crate::world_backend::WorldSession, bomb| {
        let state = world.arena.bomb29(bomb).unwrap();
        (state.byte_138, state.word_13a)
    };
    assert_eq!(armed(&world, bombs[0]), (1, 0));
    assert_eq!(armed(&world, bombs[1]).0, 0);
    let before = visuals(&world.arena);
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        bombs[0],
        &mut rng,
    )
    .unwrap();
    assert!(disposed(&world.arena, bombs[0]));
    // The other lies 921 away, inside the block's outer 1200: its 20000 on
    // channel 9 kills it (the shared 0047CBC0), and it goes off in turn.
    assert!(disposed(&world.arena, bombs[1]));
    assert_eq!(visuals(&world.arena), before + 2);
}

/// Row 12 (registry kind 20, resource 121) throws the charge, selector 27,
/// through the world's child factory. Beside Tabuchi's 0x13 generator
/// (world 9, model 0x1A7, 2000 health) it sticks to the generator when they
/// meet (00456010), and its update holds it there with the fuse at a
/// second. The detonator (row 6) marks it, and its next update bursts the
/// block's 5000 on channel 9 at the generator's own position. The
/// generator answers channel 9 above 2000 twice over: 6000, which this
/// world's difficulty scale (mission +10, 81) brings to 1898.
#[test]
fn a_charge_stuck_to_a_generator_hits_it_with_its_whole_block() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        9,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let generator = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            let state = world.arena.actor(actor).unwrap();
            world.arena.selector39(actor).is_ok()
                && state.rotating.visual.model_word == 0x1a7
                && state.health == 2000
        })
        .expect("Tabuchi's 0x13 generator");
    let [x, y, z] = world
        .arena
        .object(generator)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    let charge = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        12,
        [x.wrapping_add(250), y, z],
        &mut rng,
    );
    assert!(world.arena.is_charge27(charge));
    assert_eq!(world.arena.object(charge).unwrap().base.resource_word, 121);
    assert_eq!(
        world.arena.thrown(charge).unwrap().bytes_13e,
        Some([0x88, 0x13, 9, 0, 0xc8, 0, 0x2c, 1, 0x20, 3])
    );
    // The contact pass's +A0: a generator of another team takes it.
    assert!(world.arena.charge27_accepts(charge, generator).unwrap());
    assert_eq!(world.arena.object(charge).unwrap().base.flags & 0x104, 0);
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        charge,
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        world.arena.object(charge).unwrap().base.position_words,
        Some([x, y, z])
    );
    assert!(!detonating(&world.arena, charge));
    // The detonator, from 1500 away.
    let position = [x.wrapping_add(1500), y, z];
    world.arena.set_position_for_tests(vehicle, position);
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        6,
        position,
        &mut rng,
    );
    assert!(detonating(&world.arena, charge));
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        charge,
        &mut rng,
    )
    .unwrap();
    assert!(disposed(&world.arena, charge));
    // (5000 - 2000) * 2 at distance 0, then 0047CBC0's offline difficulty
    // scale, mission +10 (81 in this world: 1898 of the generator's 2000).
    let scale = i32::from(world.mission.word_10.unwrap());
    let damage = (6000 * scale) >> 8;
    assert_eq!(damage, 1898);
    let state = world.arena.actor(generator).unwrap();
    assert_eq!((state.health, state.word_c4), (2000 - damage as u16, 3));
}

/// Infest04's laser mine on Helniazine (world 6) rested outside the pad's
/// fence at (38521, -1198, 19200), and the detonator set it off. Its blast
/// (3000 on channel 9, 200 to 800) queues the program 00532D00 of the two
/// pylons beside the fence and kills the Bomb 421 away (health 1), whose
/// blast (20000 on channel 9, 200 to 1200) queues a post, nine fence
/// segments and a corner and kills the other Bomb, whose own finds them all
/// queued. From Infest04's RNG at the mine's last update, that tick queues
/// every record Infest04's did, in its order (`fence-queue04.txt`), and both
/// Bombs go as they did there (`bombs04.txt`).
#[test]
fn a_mine_beside_helniazines_fence_sets_off_its_bombs_as_infest04() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let mut rng = 0x1234_5678;
    let mine = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        13,
        [38521, (-1198_i16) as u16, 19200],
        &mut rng,
    );
    assert_eq!(
        world.arena.thrown(mine).unwrap().bytes_13e,
        Some([0xb8, 0x0b, 9, 0, 0xc8, 0, 0x20, 3, 0xc8, 0])
    );
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let pad = [40500, (-1156_i16) as u16, 19247];
    world.arena.set_position_for_tests(vehicle, pad);
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        6,
        pad,
        &mut rng,
    );
    assert!(detonating(&world.arena, mine));
    // Infest04's process RNG at the mine's last update (6B4F45:E97).
    let mut rng = 0x01ca_146a;
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        mine,
        &mut rng,
    )
    .unwrap();
    assert!(disposed(&world.arena, mine));
    let queued: Vec<_> = owners
        .furniture
        .records()
        .iter()
        .map(|record| {
            (
                record.program.address(),
                [record.position_words[0], record.position_words[2]],
            )
        })
        .collect();
    let segment = |x, z| (0x0053_2e30, [x, z]);
    assert_eq!(
        queued,
        [
            (0x0053_2d00, [38489, 19680]),
            (0x0053_2d00, [39001, 19680]),
            (0x0053_2da8, [38766, 18496]),
            segment(39278, 18496),
            segment(39278, 18752),
            segment(39278, 19008),
            segment(39278, 19264),
            segment(39278, 19520),
            segment(39278, 19776),
            segment(39278, 20032),
            segment(39534, 18496),
            segment(39534, 20032),
            (0x0053_2e78, [39790, 18752]),
        ]
    );
    let bombs: Vec<_> = world
        .arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .map(|node| node.object)
        .filter(|&object| world.arena.is_bomb29(object))
        .collect();
    assert_eq!(bombs.len(), 2);
}

/// A mine laid 300 from an enemy in Level 1 picks it through the scan
/// (00475EC0 with 004567A0) and tracks it with a type-2 target at +15C.
/// Once armed, it holds while the enemy stands, and goes off the tick the
/// enemy moves away.
#[test]
fn a_mine_tracks_an_enemy_and_goes_off_as_it_leaves() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    // 004567A0 passes over sides 0 and 1 and the thrower's own (2): Level
    // 1's side-0x12 actors are the mine's.
    let enemy = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            let state = world.arena.actor(actor).unwrap();
            state.word_b4 == 0x12
                && state.word_c4 > 1
                && world.arena.object(actor).unwrap().base.flags & 0x24 == 0x24
        })
        .expect("a side-0x12 actor in Level 1");
    let [x, y, z] = world
        .arena
        .object(enemy)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    let mine = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        13,
        [x.wrapping_add(300), y, z],
        &mut rng,
    );
    let target = |world: &crate::world_backend::WorldSession| match &world
        .arena
        .thrown(mine)
        .unwrap()
        .tail
    {
        crate::object_runtime::Family455560Tail::Selector38 {
            object_15c,
            dword_160,
            ..
        } => (*object_15c, *dword_160),
        other => panic!("a mine's tail, not {other:?}"),
    };
    // Unarmed, it tracks the enemy; a second later it arms and picks it
    // again.
    for _ in 0..12 {
        tick(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            mine,
            &mut rng,
        )
        .unwrap();
        world
            .arena
            .with_supplied_family_dispatch_actor(mine, |state| state.words_8e_92 = [0; 3])
            .unwrap();
        assert!(target(&world).0.is_some());
        assert!(!detonating(&world.arena, mine));
    }
    assert_eq!(world.arena.thrown(mine).unwrap().byte_138, 1);
    let tracked = target(&world).1;
    assert!(tracked > 0 && tracked < 600 * 600, "{tracked}");
    // It steps 100 farther away.
    world
        .arena
        .place_for_tests(enemy, [x.wrapping_sub(100), y, z]);
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        mine,
        &mut rng,
    )
    .unwrap();
    assert!(disposed(&world.arena, mine));
}

/// A napalm grenade (row 15: kind 24, resource 126, payload flag 4) burns
/// as it flies: each tick counts its burn timer +150 down and runs the
/// burning actors' status visual 0047DFD0, which marks the dirty word
/// (0x400) and, as the load factor allows, births a flame (00489D90 type
/// 9). It detonates on its 2 s fuse without the default burst.
#[test]
fn a_napalm_grenade_burns_as_it_flies() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let [x, y, z] = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    let grenade = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        15,
        [x.wrapping_add(300), y.wrapping_add(300), z],
        &mut rng,
    );
    let burn = |world: &crate::world_backend::WorldSession| match &world
        .arena
        .grenade26(grenade)
        .unwrap()
        .tail
    {
        crate::object_runtime::Family455560Tail::Flags14c {
            byte_14c,
            dword_150,
            ..
        } => (*byte_14c, *dword_150),
        other => panic!("a grenade's tail, not {other:?}"),
    };
    assert_eq!(burn(&world), (Some(4), 5_000_000));
    let before = visuals(&world.arena);
    let mut flames = 0;
    for tick_index in 1..=17_u32 {
        let draws_before = rng;
        tick(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            grenade,
            &mut rng,
        )
        .unwrap();
        assert_ne!(rng, draws_before, "tick {tick_index} draws for its flames");
        if disposed(&world.arena, grenade) {
            // 17 steps of 122 ms run out the 2 s fuse.
            assert_eq!(tick_index, 17);
            break;
        }
        assert_eq!(burn(&world).1, 5_000_000 - tick_index * 125_000);
        assert_ne!(
            world.arena.object(grenade).unwrap().base.dirty_word & 0x400,
            0
        );
        flames += 1;
    }
    assert!(disposed(&world.arena, grenade));
    assert_eq!(flames, 16);
    // Payload flags skip the default burst's visual.
    assert_eq!(visuals(&world.arena), before);
}

/// A smoke grenade (row 10: kind 18, resource 123, payload flag 1, fuse
/// 10 s) fizzes once it is older than 1.5 s: its first alert (0047DBF0)
/// sends event 0xD to side 0x12 within 800, and Level 1's side-0x12 family
/// actor there raises its alert timer +17A to 8000 (004270B0).
#[test]
fn a_smoke_grenade_alerts_the_alien_near_it() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let turret = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            world.arena.actor(actor).unwrap().word_b4 == 0x12
                && world.arena.family_update_class(actor).is_ok()
        })
        .expect("a side-0x12 family actor in Level 1");
    let [x, y, z] = world
        .arena
        .object(turret)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    let grenade = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        10,
        [x.wrapping_add(400), y, z],
        &mut rng,
    );
    assert_eq!(world.arena.grenade26(grenade).unwrap().word_13a, 10_000);
    assert_eq!(world.arena.derived_actor(turret).unwrap().word_17a, 0);
    let mut alerted_at = None;
    for tick_index in 1..=14 {
        tick(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            grenade,
            &mut rng,
        )
        .unwrap();
        world
            .arena
            .with_supplied_family_dispatch_actor(grenade, |state| state.words_8e_92 = [0; 3])
            .unwrap();
        world
            .arena
            .place_for_tests(grenade, [x.wrapping_add(400), y, z]);
        if alerted_at.is_none() && world.arena.derived_actor(turret).unwrap().word_17a != 0 {
            alerted_at = Some(tick_index);
        }
    }
    // The smoke reads the age before the fuse adds the tick's step: 13
    // steps of 122 ms make the first age past 1500 (1586), on the 14th.
    assert_eq!(alerted_at, Some(14));
    assert_eq!(world.arena.derived_actor(turret).unwrap().word_17a, 8000);
}

/// A smoke grenade fizzing 400 from world 5's mutant factory (an enemy
/// factory, command 22, filled by command 18, so team 0x12) sends it event
/// 0xD (0047DBF0 within 800 to side 0x12): the factory's 0042F380 takes its
/// whole staff, tells the first player "Factory neutralised" and turns it
/// team 0. A mutant scientist 300 from the grenade dies (00468BE0's
/// 0047CE90); one of another team there lives.
#[test]
fn a_smoke_grenade_neutralises_a_mutant_factory() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        5,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let factory = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            world.arena.selector15(actor).is_ok()
                && world.arena.actor(actor).unwrap().word_b4 == 0x12
        })
        .expect("world 5's mutant factory");
    let staff = &world.arena.selector15(factory).unwrap().intermediate;
    assert!(staff.word_140 > 0 && staff.word_140 == staff.word_13c);
    let [x, y, z] = world
        .arena
        .object(factory)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let mut rng = 0x1234_5678;
    let grenade = place(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        10,
        [x.wrapping_add(400), y, z],
        &mut rng,
    );
    let scientists: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            world.arena.selector12_actor(actor).is_ok()
                && world.arena.actor(actor).unwrap().word_c4 == 3
        })
        .take(2)
        .collect();
    assert_eq!(scientists.len(), 2);
    for (&scientist, (team, dz)) in scientists.iter().zip([(0x12, 300), (2, 200)]) {
        world
            .arena
            .with_supplied_family_dispatch_actor(scientist, |state| state.word_b4 = team)
            .unwrap();
        world
            .arena
            .place_for_tests(scientist, [x.wrapping_add(400), y, z.wrapping_add(dz)]);
    }
    let mut neutralised_at = None;
    for tick_index in 1..=14 {
        tick(
            &mut world,
            &resident,
            executable,
            &mut owners,
            &sine,
            grenade,
            &mut rng,
        )
        .unwrap();
        world
            .arena
            .with_supplied_family_dispatch_actor(grenade, |state| state.words_8e_92 = [0; 3])
            .unwrap();
        world
            .arena
            .place_for_tests(grenade, [x.wrapping_add(400), y, z]);
        if neutralised_at.is_none() && world.arena.actor(factory).unwrap().word_b4 == 0 {
            neutralised_at = Some(tick_index);
        }
    }
    assert_eq!(neutralised_at, Some(14));
    assert_eq!(
        world
            .arena
            .selector15(factory)
            .unwrap()
            .intermediate
            .word_140,
        0
    );
    let states: Vec<_> = scientists
        .iter()
        .map(|&scientist| world.arena.actor(scientist).map(|state| state.word_c4).ok())
        .collect();
    assert_ne!(states[0], Some(3), "the mutant lives");
    assert_eq!(states[1], Some(3));
}
