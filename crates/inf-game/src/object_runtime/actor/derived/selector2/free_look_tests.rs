use super::*;
use crate::object_runtime::DerivedActorClass;

fn vehicle_state(arena: &ObjectArena, vehicle: ObjectId) -> super::super::Selector2State {
    match &arena.derived_actor(vehicle).unwrap().class {
        DerivedActorClass::Selector2(state) => (**state).clone(),
        _ => panic!("not a vehicle"),
    }
}

/// 00421DC0 over Level 1's buggy at 40 ms ticks: V turns the look on with
/// the zoom at 0x3FFF, takes the eye and both rows, and spends the controls;
/// Left swings the yaw rate (pushed twice as hard from rest), the zoom key
/// with Climb steps the zoom, a weapon pick sets its end, and Thrust ends
/// the look.
#[test]
fn free_look_toggles_swings_zooms_and_ends() {
    let (_runtime, _assets, mut world) = crate::test_worlds::offline_world_retained(
        18,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let arena = &mut world.arena;
    let player = arena.first_player().unwrap();
    let vehicle = arena.player_actor(player).unwrap().unwrap();
    let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
    let basis = arena.actor(vehicle).unwrap().rotating.basis;
    let zoom = Cell::new(0);
    let look = |arena: &mut ObjectArena, controls: &mut PlayerControlBlock| {
        arena
            .vehicle_free_look::<()>(vehicle, controls, 40_000, &zoom)
            .unwrap();
    };

    let mut controls = PlayerControlBlock::default();
    controls.set_byte(0x3f, 1);
    controls.pitch_18 = 7;
    look(arena, &mut controls);
    let state = vehicle_state(arena, vehicle);
    assert_eq!(state.byte_1c0, 1);
    assert_eq!(zoom.get(), 0x3fff);
    assert_eq!(state.special_position_1ca, position.map(Some));
    assert_eq!(
        state.special_forward_1d0,
        [7, 8, 9].map(|i| Some(basis.words[i]))
    );
    assert_eq!(
        state.special_up_1dc,
        [4, 5, 6].map(|i| Some(basis.words[i]))
    );
    let mut spent = PlayerControlBlock::default();
    spent.set_byte(0x3e, 1);
    assert_eq!(controls, spent);

    // Left held a whole tick: -(40000 >> 5) doubled from rest, then the
    // yaw turns by (rate * 1250) >> 15.
    let mut controls = PlayerControlBlock {
        turn_14: 40_000,
        ..Default::default()
    };
    look(arena, &mut controls);
    let state = vehicle_state(arena, vehicle);
    assert_eq!(state.byte_1c0, 1);
    assert_eq!(
        state.words_1c2_1c8,
        [(-2500i16) as u16, 0, (-96i16) as u16, 0]
    );

    // The zoom key with Climb: +0x1000, and the pitch rate rests.
    let mut controls = PlayerControlBlock {
        pitch_18: -40_000,
        ..Default::default()
    };
    controls.set_byte(0x40, 1);
    look(arena, &mut controls);
    assert_eq!(zoom.get(), 0x4fff);
    assert_eq!(vehicle_state(arena, vehicle).words_1c2_1c8[1], 0);
    assert_eq!(
        controls.byte(0x40),
        1,
        "the zoom key survives the spent block"
    );

    // A secondary weapon pick puts the zoom at its near end.
    let mut controls = PlayerControlBlock {
        dword_2c: 1,
        ..Default::default()
    };
    look(arena, &mut controls);
    assert_eq!(zoom.get(), 0);
    assert_eq!(vehicle_state(arena, vehicle).byte_1c0, 1);

    // Thrust ends the look; the zoom stays.
    let mut controls = PlayerControlBlock {
        thrust_1c: 40_000,
        ..Default::default()
    };
    look(arena, &mut controls);
    let state = vehicle_state(arena, vehicle);
    assert_eq!(state.byte_1c0, 0);
    assert_eq!(state.words_1c2_1c8, [0; 4]);
    assert_eq!(zoom.get(), 0);
    assert_eq!(controls, spent);
}

/// The decay never passes zero and falls at least by its floor.
#[test]
fn free_look_rates_fall_to_rest() {
    assert_eq!(decay(0, 5000, 1250), 0);
    assert_eq!(decay(1000, 5000, 1250), 0);
    assert_eq!(decay((-1000i16) as u16, 5000, 1250), 0);
    // 0x4000 * 5000 >> 15 = 2500 a tick.
    assert_eq!(decay(0x4000, 5000, 1250), 0x4000 - 2500);
    assert_eq!(analog_turn(0x3000, 1250), None);
    assert_eq!(
        analog_turn(-0x6000, 1250),
        Some(((0x3000 / 3 * 1250) >> 15) as u16)
    );
}
