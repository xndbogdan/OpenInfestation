use super::super::mode_receipts::{assert_no_failures, mode_receipt_failures};

/// Infest03's 16 calls of 00422400, on world 11 (coarse terrain), each run
/// from its captured entry state on the port's own terrain.
#[test]
fn skimmer_drive_matches_every_infest03_tick() {
    let (count, failures) = mode_receipt_failures(
        include_str!("../../../../../tests/fixtures/vehicle_skimmer_infest03.json"),
        |arena, vehicle, controls, request, host| {
            arena.tick_vehicle_skimmer(vehicle, controls, request, host)
        },
    );
    assert_eq!(count, 16);
    assert_no_failures((count, failures));
}

/// 00422380(1) sets the skimmer's table and byte +160, clears its state and
/// the traction and puts the camera in mode 1; 00422380(0) has no wake
/// ribbon to release offline, and stops if one were held.
#[test]
fn skimmer_entry_sets_its_table_and_camera_and_the_exit_releases_no_ribbon() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let camera = world
        .arena
        .resolve_actor_handle(world.arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    {
        let state = world.arena.selector2_actor_mut(vehicle).unwrap();
        state.words_24c_24e = [5, 6];
        state.dwords_250_25c = [1, 2, 3, 4];
        state.drive.dword_57c = Some(9);
    }
    world
        .arena
        .switch_vehicle_skimmer::<String>(
            vehicle,
            true,
            Some(camera),
            crate::object_runtime::ActorHandle::from_raw(0),
        )
        .unwrap();
    let state = world.arena.selector2_actor(vehicle).unwrap();
    assert_eq!(state.words_188_194[3], 0x0053_05d8);
    assert_eq!(state.words_24c_24e, [0, 6]);
    assert_eq!(state.dwords_250_25c, [0; 4]);
    assert_eq!(state.dword_260, Some(0));
    assert_eq!(state.drive.dword_57c, Some(0));
    assert_eq!(world.arena.derived_actor(vehicle).unwrap().byte_160, 1);
    world
        .arena
        .switch_vehicle_skimmer::<String>(
            vehicle,
            false,
            Some(camera),
            crate::object_runtime::ActorHandle::from_raw(0),
        )
        .unwrap();
    // 004223EF: leaving lets a wake ribbon go, its owner the null.
    let null = crate::object_runtime::ActorHandle::from_raw(0x55);
    let ribbon = world.arena.allocate_selector67(null);
    let handle = world.arena.actor(vehicle).unwrap().handle;
    world.arena.set_selector67_owner(ribbon, handle).unwrap();
    world.arena.selector2_actor_mut(vehicle).unwrap().ribbon_264 = Some(ribbon);
    world
        .arena
        .switch_vehicle_skimmer::<String>(vehicle, false, Some(camera), null)
        .unwrap();
    assert_eq!(
        world.arena.selector2_actor(vehicle).unwrap().ribbon_264,
        None
    );
    assert_eq!(world.arena.selector67(ribbon).unwrap().owner_50, null);
}
