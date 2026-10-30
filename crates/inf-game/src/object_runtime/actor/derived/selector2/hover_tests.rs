use super::super::mode_receipts::{assert_no_failures, mode_receipt_failures};

/// Infest03's 212 calls of 004191C0, on worlds 9 and 11 (coarse terrain),
/// each run from its captured entry state on the port's own terrain.
#[test]
fn hover_drive_matches_every_infest03_tick() {
    let (count, failures) = mode_receipt_failures(
        include_str!("../../../../../tests/fixtures/vehicle_hover_infest03.json"),
        |arena, vehicle, controls, request, host| {
            arena.tick_vehicle_hover(vehicle, controls, request, host)
        },
    );
    assert_eq!(count, 212);
    assert_no_failures((count, failures));
}

/// A world started as the hovercraft enters mode 3 without the ground reset
/// that stores the engine load, so the drive's load step reads what
/// construction left: VehicleEngineLoadApproximation's zero, not a stop (the
/// user's 2026-10-19 crash log, after Tabuchi's gate).
#[test]
fn a_hovercraft_start_reads_the_engine_load_construction_left() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let drive = &world.arena.selector2_actor(vehicle).unwrap().drive;
    assert_eq!(
        [drive.dword_55c, drive.dword_560],
        [Some(super::super::drive::VEHICLE_ENGINE_LOAD_APPROXIMATION); 2]
    );
    world.arena.reset_vehicle_hover(vehicle).unwrap();
    let drive = &world.arena.selector2_actor(vehicle).unwrap().drive;
    assert_eq!([drive.dword_55c, drive.dword_560], [Some(0); 2]);
}

/// A vehicle that has only been the hovercraft since it was built turns on
/// the hold +240 construction left (00419050 reads it at 004190CF, adding
/// the step): VehicleTurnHoldApproximation's zero, as the lab read in retail
/// (`level1-hover-turn-hold.json`), not a stop (a 2026-10-21 crash
/// log, turning just after switching to the hovercraft on Artoria).
#[test]
fn a_hovercraft_start_turns_on_the_hold_construction_left() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let hold = |world: &crate::world_backend::WorldSession| {
        world
            .arena
            .selector2_actor(vehicle)
            .unwrap()
            .drive
            .dword_240
    };
    assert_eq!(
        hold(&world),
        Some(super::super::drive::VEHICLE_TURN_HOLD_APPROXIMATION)
    );
    world.arena.reset_vehicle_hover(vehicle).unwrap();
    let controls = crate::object_runtime::PlayerControlBlock {
        turn_14: 1,
        ..Default::default()
    };
    world
        .arena
        .vehicle_turn_rate::<String>(vehicle, 1953, &controls, 0)
        .unwrap();
    assert_eq!(hold(&world), Some(1953));
}

/// 004185D0 sets the hover tables and clears the turn rate, the traction
/// and the boost, as Infest03's three entries do.
#[test]
fn hover_entry_sets_its_tables_and_clears_the_drive() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    {
        let state = world.arena.selector2_actor_mut(vehicle).unwrap();
        state.drive.dword_21c = Some(0x1234);
        state.drive.dword_57c = Some(0x4321);
        state.dwords_220_224 = [7, 9];
    }
    world.arena.reset_vehicle_hover(vehicle).unwrap();
    let state = world.arena.selector2_actor(vehicle).unwrap();
    assert_eq!(
        state.words_188_194,
        [0x0053_0510, 0x0053_0518, 0x0053_0508, 0x0053_0560]
    );
    assert_eq!(state.drive.dword_21c, Some(0));
    assert_eq!(state.drive.dword_57c, Some(0));
    assert_eq!(state.dwords_220_224, [7, 0]);
    assert_eq!(world.arena.derived_actor(vehicle).unwrap().byte_160, 1);
}
