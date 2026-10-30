use super::super::mode_receipts::{assert_no_failures, mode_receipt_failures};

/// Infest03's 107 calls of 00418650, on worlds 11 and 23 (coarse terrain),
/// each run from its captured entry state on the port's own terrain.
#[test]
fn vtol_drive_matches_every_infest03_tick() {
    let (count, failures) = mode_receipt_failures(
        include_str!("../../../../../tests/fixtures/vehicle_vtol_infest03.json"),
        |arena, vehicle, controls, request, host| {
            arena.tick_vehicle_vtol(vehicle, controls, request, host)
        },
    );
    assert_eq!(count, 107);
    assert_no_failures((count, failures));
}

/// Records the one-shots; the VTOL's entry and exit make no other calls.
#[derive(Default)]
struct Sounds(Vec<(u32, u32, u32)>);

impl super::super::drive::VehicleDriveHost for Sounds {
    type Error = String;
    fn play_sound(
        &mut self,
        _: &crate::object_runtime::ObjectArena,
        _: crate::object_runtime::ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.0.push((sound, gain, pitch));
        Ok(())
    }
    fn create_sound(
        &mut self,
        _: &crate::object_runtime::ObjectArena,
        _: crate::object_runtime::ObjectId,
        _: u32,
        _: u32,
        _: u32,
    ) -> Result<crate::process_sound::ProcessSoundRef, String> {
        Err("create".into())
    }
    fn update_sound(
        &mut self,
        _: &crate::object_runtime::ObjectArena,
        _: crate::object_runtime::ObjectId,
        _: crate::process_sound::ProcessSoundRef,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Err("update".into())
    }
    fn release_sound(&mut self, _: crate::process_sound::ProcessSoundRef) -> Result<(), String> {
        Err("release".into())
    }
}

/// 00418430(1) clears the flight state with +24E at rest and plays the
/// take-off; 00418430(0) only plays the landing.
#[test]
fn vtol_entry_clears_the_flight_state_and_the_exit_only_sounds() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    {
        let state = world.arena.selector2_actor_mut(vehicle).unwrap();
        state.words_24c_24e = [5, 6];
        state.dwords_250_25c = [1, 2, 3, 4];
        state.drive.dword_57c = Some(9);
    }
    let mut sounds = Sounds::default();
    world
        .arena
        .switch_vehicle_vtol(vehicle, true, &mut sounds)
        .unwrap();
    let state = world.arena.selector2_actor(vehicle).unwrap();
    assert_eq!(state.words_24c_24e, [0, 0x7fff]);
    assert_eq!(state.dwords_250_25c, [0; 4]);
    assert_eq!(state.drive.dword_57c, Some(0));
    world
        .arena
        .selector2_actor_mut(vehicle)
        .unwrap()
        .dwords_250_25c = [1, 2, 3, 4];
    world
        .arena
        .switch_vehicle_vtol(vehicle, false, &mut sounds)
        .unwrap();
    assert_eq!(
        world.arena.selector2_actor(vehicle).unwrap().dwords_250_25c,
        [1, 2, 3, 4]
    );
    assert_eq!(sounds.0, [(0x11, 0x4000, 0x10000), (0x12, 0x4000, 0x10000)]);
}
