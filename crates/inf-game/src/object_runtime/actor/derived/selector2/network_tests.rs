//! The vehicle in a network game: 004156E0's form and 0041A620's scaling.

use crate::terrain_runtime::TerrainSpacing;

/// From the buggy's 5000: in a team game (mode 0) the truck has half again,
/// the VTOL half and the skimmer a quarter; modes 2 and 4 give sixteen
/// times, kept in a WORD; a race (mode 3) keeps it. The form is the
/// player's vehicle +18 in mode 0, else the arena's.
#[test]
fn the_guard_scales_a_network_vehicles_health_by_mode_and_form() {
    let (_, assets, world) = super::test_support::loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let table = inf_formats::parse_vehicle_behavior_table(executable).unwrap();
    for (mode, vehicle_18, arena_1c, form, maximum) in [
        (0, 0, 0, 1, 5000),
        (0, 1, 0, 2, 7500),
        (0, 2, 0, 3, 5000),
        (0, 3, 0, 4, 2500),
        (0, 4, 0, 5, 1250),
        (2, 0, 5, 1, 14464),
        (3, 0, 8, 5, 5000),
        (4, 0, 1, 1, 14464),
    ] {
        let mut settings = [0; 20];
        settings[0x10 / 4] = mode;
        settings[0x18 / 4] = vehicle_18;
        settings[0x1c / 4] = arena_1c;
        arena.actor_mut(vehicle).unwrap().maximum_health = Some(5000);
        arena
            .set_selector2_network_form(vehicle, &settings)
            .unwrap();
        assert_eq!(
            arena.selector2_actor(vehicle).unwrap().dwords_1e8_1f4[2],
            form
        );
        arena
            .admit_selector2_actor(vehicle, &table, Some(mode))
            .unwrap();
        let actor = arena.actor(vehicle).unwrap();
        assert_eq!(
            (actor.maximum_health, actor.health),
            (Some(maximum), maximum)
        );
    }
}

/// 004211B4: a new wake ribbon starts a quarter of the velocity behind (the
/// doubled WORD's arithmetic eighth), then steps an eighth twice.
#[test]
fn a_new_wake_ribbon_starts_behind_the_skimmer() {
    let points = super::remote::wake_ribbon_seed([1000, 200, 3000], [80, (-81_i16) as u16, 7]);
    assert_eq!(
        points,
        [[980, 221, 2999], [990, 210, 2999], [1000, 199, 2999],]
    );
}
