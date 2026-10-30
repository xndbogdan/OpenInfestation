use crate::process_particles::factories::ParticleFactoryProcessState;

/// The host of a Capture the Banner game (world 33, mode 0): the arena's
/// spawn markers load into the spawn list with their teams and offsets.
#[test]
fn capture_the_banner_lists_its_spawn_markers() {
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings: [0; 20],
        ..ParticleFactoryProcessState::native_startup()
    };
    let (_resident, _assets, world) = crate::test_worlds::network_world(33, process);
    let markers = world.arena.spawn_markers();
    let teams: Vec<_> = markers
        .iter()
        .map(|&marker| world.arena.selector45(marker).unwrap().team_84)
        .collect();
    // Two markers for each banner's team (states 2 and 3).
    assert_eq!(markers.len(), 4);
    assert_eq!(teams.iter().filter(|&&team| team == 2).count(), 2);
    assert_eq!(teams.iter().filter(|&&team| team == 3).count(), 2);
}

/// The host of Buggy Madness (arena 5: world 27, mode 2) spawns at a marker:
/// its position plus offset, the marker's basis turned a quarter, the
/// arena's buggy owned by slot 1 + 1 with sixteen times the health, and the
/// mode's kit granted in silence.
#[test]
fn buggy_madness_spawns_the_host_at_a_marker_with_the_modes_kit() {
    let mut settings = [0; 20];
    settings[0x10 / 4] = 2;
    settings[0x1c / 4] = 5;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (_resident, assets, world) = crate::test_worlds::network_world(27, process);
    let respawn = world.respawn.as_ref().unwrap();
    assert_eq!(respawn.messages, []);
    let vehicle = respawn.vehicle.unwrap();
    let arena = &world.arena;
    let actor = arena.actor(vehicle).unwrap();
    assert_eq!(arena.selector2_actor(vehicle).unwrap().dwords_1e8_1f4[2], 1);
    assert_eq!(actor.word_b4, 2);
    assert_eq!((actor.maximum_health, actor.health), (Some(14464), 14464));
    let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
    let marker = arena
        .spawn_markers()
        .iter()
        .copied()
        .find(|&marker| {
            let at = arena.object(marker).unwrap().base.position_words.unwrap();
            let offset = arena.selector45(marker).unwrap().offset_78;
            [0, 2].map(|axis| at[axis].wrapping_add(offset[axis])) == [position[0], position[2]]
        })
        .expect("the vehicle stands at a marker");
    let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
    let mut basis = arena.selector45(marker).unwrap().rotating.basis;
    math.rotate_in_plane(
        &mut basis,
        0x4000,
        inf_formats::native_basis::RotationPlane::Xz,
    );
    assert_eq!(actor.rotating.basis, basis);
    let player = arena.world_player(arena.first_player().unwrap()).unwrap();
    // 004698F0's mode 2: 0xA0, 0x25, 0xAB, 0xA9, 0xAC, then Turbo (3, 5).
    assert_eq!(
        player.bits_100,
        [1 << 3, 1 << 5, 0, 0, 0, 1 | 1 << 9 | 1 << 11 | 1 << 12]
    );
    assert_eq!(player.dword_12c, 5);
}

/// Four Play Heaven (arena 7: world 26, mode 0) loads: its placements'
/// after-load reads its completion words, the timing ring's samples 1 to 4
/// past the mission (0048EA80), and a network placement's index -1 tests
/// bit 31 of the first, which a frame's time never sets.
#[test]
fn four_play_heaven_reads_the_timing_ring_as_its_completion_words() {
    let mut settings = [0; 20];
    settings[0x1c / 4] = 7;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (_resident, _assets, world) = crate::test_worlds::network_world(26, process);
    let read: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .filter(|&&actor| world.arena.selector30_actor(actor).is_ok())
        .map(|&actor| world.arena.actor(actor).unwrap().byte_78 as i8)
        .collect();
    assert!(!read.is_empty());
    assert!(read.iter().all(|&index| index == -1));
}
