use super::*;
use crate::input::{
    ActionDescriptor, ConsumerId, EventPacket, InputCallback, InputHost, InputRuntime,
};

struct Clock(u64);

impl InputHost for Clock {
    type Error = &'static str;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.0)
    }

    fn callback(&mut self, _: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// The player's references in 00469D60's order: the three analog ones
/// with their zero-fill metadata, then the eight digital ones with the
/// held-duration flag the retail descriptors carry.
fn consumers(input: &mut InputRuntime) -> PlayerInputConsumers {
    let analog_5c = crate::object_runtime::player_analog_fixture(input, 0x0054_ecf8);
    let analog_60 = crate::object_runtime::player_analog_fixture(input, 0x0054_ece8);
    let analog_64 = crate::object_runtime::player_analog_fixture(input, 0x0054_ed08);
    let mut acquire = |identity| {
        input
            .acquire_consumer(
                ActionDescriptor {
                    identity,
                    callback_address: None,
                    flags: 6,
                },
                0x77,
            )
            .unwrap()
    };
    PlayerInputConsumers {
        analog_5c,
        analog_60,
        analog_64,
        left_68: acquire(0x0053_a368),
        right_6c: acquire(0x0053_a370),
        dive_70: acquire(0x0053_a358),
        climb_74: acquire(0x0053_a360),
        thrust_78: acquire(0x0053_a378),
        reverse_7c: acquire(0x0053_a380),
        fire_80: acquire(0x0053_a388),
        grenade_84: acquire(0x0053_a390),
    }
}

fn press(input: &mut InputRuntime, clock: &mut Clock, id: ConsumerId, at: u64) {
    clock.0 = at;
    input
        .emit(id, 1, &mut EventPacket::default(), clock)
        .unwrap();
}

fn poll(input: &mut InputRuntime, clock: &mut Clock, at: u64) {
    clock.0 = at;
    input.poll(clock).unwrap();
}

/// A list player in the constructor's +50 domain.
fn player() -> (ObjectArena, PlayerId) {
    let mut arena = ObjectArena::new();
    let player = arena.add_world_player([0; 6]);
    arena.world_player_mut(player).unwrap().dword_50 = Some(0);
    (arena, player)
}

fn inputs(delta_micros: i32) -> PlayerUpdateInputs {
    PlayerUpdateInputs {
        delta_micros,
        slow_input_5c5: false,
        input_mode_08: 0,
        network: 0,
    }
}

const DRIVING: Option<(u32, i32)> = Some((2, 3));

#[test]
fn held_times_fill_the_block_capped_at_the_delta() {
    let (mut arena, player) = player();
    let mut input = InputRuntime::new(0);
    let mut clock = Clock(0);
    let ids = consumers(&mut input);
    press(&mut input, &mut clock, ids.thrust_78, 0);
    press(&mut input, &mut clock, ids.left_68, 40_000);
    press(&mut input, &mut clock, ids.climb_74, 90_000);
    poll(&mut input, &mut clock, 100_000);

    let update = arena
        .update_player_controls(
            player,
            inputs(80_000),
            &mut input,
            &ids,
            DRIVING,
            None,
            None,
        )
        .unwrap();
    assert_eq!(update.call, PlayerVehicleCall::Update44);
    assert!(update.omissions.is_empty());
    let controls = arena.world_player(player).unwrap().controls;
    // 100 000 µs of Thrust and 60 000 of Left; the delta caps Thrust.
    assert_eq!(controls.thrust_1c, 80_000);
    assert_eq!(controls.turn_14, 60_000);
    // Climb counts against Dive.
    assert_eq!(controls.pitch_18, -10_000);
    assert_eq!((controls.fire_20, controls.grenade_24), (0, 0));
    assert_eq!(controls.byte(0x3a), 1, "+3A follows a positive +1C");
    assert_eq!((controls.byte(0x3e), controls.byte(0x42)), (0, 0));
    assert_eq!(controls.analog_04, 0);
}

#[test]
fn g_5c5_quarters_the_cap_with_signed_truncation() {
    let (mut arena, player) = player();
    let mut input = InputRuntime::new(0);
    let mut clock = Clock(0);
    let ids = consumers(&mut input);
    press(&mut input, &mut clock, ids.thrust_78, 0);
    press(&mut input, &mut clock, ids.right_6c, 0);
    poll(&mut input, &mut clock, 100_000);
    let slow = PlayerUpdateInputs {
        slow_input_5c5: true,
        ..inputs(80_003)
    };
    arena
        .update_player_controls(player, slow, &mut input, &ids, DRIVING, None, None)
        .unwrap();
    let controls = arena.world_player(player).unwrap().controls;
    assert_eq!(controls.thrust_1c, 20_000);
    assert_eq!(controls.turn_14, -20_000);
}

#[test]
fn reverse_outweighing_thrust_clears_3a() {
    let (mut arena, player) = player();
    let mut input = InputRuntime::new(0);
    let mut clock = Clock(0);
    let ids = consumers(&mut input);
    press(&mut input, &mut clock, ids.thrust_78, 50_000);
    press(&mut input, &mut clock, ids.reverse_7c, 0);
    poll(&mut input, &mut clock, 100_000);
    arena
        .update_player_controls(
            player,
            inputs(125_000),
            &mut input,
            &ids,
            DRIVING,
            None,
            None,
        )
        .unwrap();
    let controls = arena.world_player(player).unwrap().controls;
    assert_eq!(controls.thrust_1c, -50_000);
    assert_eq!(controls.byte(0x3a), 0);
}

#[test]
fn f4_holds_fire_and_grenade_off_by_the_raw_delta() {
    let (mut arena, player) = player();
    let mut input = InputRuntime::new(0);
    let mut clock = Clock(0);
    let ids = consumers(&mut input);
    press(&mut input, &mut clock, ids.fire_80, 0);
    press(&mut input, &mut clock, ids.grenade_84, 0);
    press(&mut input, &mut clock, ids.thrust_78, 0);
    poll(&mut input, &mut clock, 30_000);
    arena.world_player_mut(player).unwrap().dword_f4 = 50_000;
    let slow = PlayerUpdateInputs {
        slow_input_5c5: true,
        ..inputs(30_000)
    };
    arena
        .update_player_controls(player, slow, &mut input, &ids, DRIVING, None, None)
        .unwrap();
    let state = arena.world_player(player).unwrap();
    // The lockout counts the raw delta, not the quartered cap.
    assert_eq!(state.dword_f4, 20_000);
    assert_eq!((state.controls.fire_20, state.controls.grenade_24), (0, 0));
    // 0046A4FC writes +3A again from +1C after the lockout's zero.
    assert_eq!(state.controls.byte(0x3a), 1);

    poll(&mut input, &mut clock, 60_000);
    arena
        .update_player_controls(
            player,
            inputs(30_000),
            &mut input,
            &ids,
            DRIVING,
            None,
            None,
        )
        .unwrap();
    let state = arena.world_player(player).unwrap();
    assert_eq!(state.dword_f4, 0);
    assert_eq!(
        (state.controls.fire_20, state.controls.grenade_24),
        (30_000, 30_000)
    );
}

#[test]
fn the_vehicle_call_follows_flag_2_and_the_lifecycle() {
    let (mut arena, player) = player();
    let mut input = InputRuntime::new(0);
    let ids = consumers(&mut input);
    let mut call = |actor| {
        arena
            .update_player_controls(player, inputs(1_000), &mut input, &ids, actor, None, None)
            .unwrap()
            .call
    };
    assert_eq!(call(None), PlayerVehicleCall::None);
    assert_eq!(call(Some((0x10, 3))), PlayerVehicleCall::None);
    assert_eq!(call(Some((2, 2))), PlayerVehicleCall::Update24);
    assert_eq!(call(Some((2, -1))), PlayerVehicleCall::Update24);
    assert_eq!(call(Some((2, 3))), PlayerVehicleCall::Update44);
    assert_eq!(call(Some((0x12, 7))), PlayerVehicleCall::Update44);
}

#[test]
fn unported_branches_stop_before_writing_the_block() {
    let (mut arena, player) = player();
    let mut input = InputRuntime::new(0);
    let mut clock = Clock(0);
    let ids = consumers(&mut input);
    press(&mut input, &mut clock, ids.thrust_78, 0);
    poll(&mut input, &mut clock, 10_000);
    let network = PlayerUpdateInputs {
        network: 1,
        ..inputs(10_000)
    };
    assert!(matches!(
        arena.update_player_controls(player, network, &mut input, &ids, DRIVING, None, None),
        Err(PlayerUpdateError::Network(1))
    ));
    assert_eq!(
        arena.world_player(player).unwrap().controls,
        PlayerControlBlock::default()
    );
}

/// The gameplay sub-object's sets over the keyboard and a pad of
/// `native_type` with `axes` axes, first read at `centre`, calibrated at
/// Sensitivity 10, with the player's references.
fn sub_object_chain(
    native_type: u8,
    axes: u8,
    centre: [u32; 6],
) -> (
    InputRuntime,
    Clock,
    PlayerInputConsumers,
    crate::platform_input::SdlInput,
    crate::frontend_bindings::FrontendBindings,
) {
    use crate::platform_input::{InputNotification, PadSample, SdlInput};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let mut input = InputRuntime::new(0);
    let mut clock = Clock(0);
    // The sub-object's tables share these references, so they take the
    // executable's own descriptors.
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut digital = |identity| {
        let descriptor =
            inf_formats::parse_digital_action_descriptor(executable, identity).unwrap();
        input
            .acquire_consumer(
                ActionDescriptor {
                    identity,
                    callback_address: (descriptor.callback_address != 0)
                        .then_some(descriptor.callback_address),
                    flags: descriptor.flags,
                },
                0x77,
            )
            .unwrap()
    };
    let [left_68, right_6c, dive_70, climb_74, thrust_78, reverse_7c, fire_80, grenade_84] = [
        0x0053_a368,
        0x0053_a370,
        0x0053_a358,
        0x0053_a360,
        0x0053_a378,
        0x0053_a380,
        0x0053_a388,
        0x0053_a390,
    ]
    .map(&mut digital);
    let ids = PlayerInputConsumers {
        analog_5c: crate::object_runtime::player_analog_fixture(&mut input, 0x0054_ecf8),
        analog_60: crate::object_runtime::player_analog_fixture(&mut input, 0x0054_ece8),
        analog_64: crate::object_runtime::player_analog_fixture(&mut input, 0x0054_ed08),
        left_68,
        right_6c,
        dive_70,
        climb_74,
        thrust_78,
        reverse_7c,
        fire_80,
        grenade_84,
    };
    let context = input.create_context().unwrap();
    let mut devices = SdlInput::attach(&mut input).unwrap();
    devices
        .update(
            InputNotification::Pad {
                native_type,
                axes,
                sample: PadSample::Read {
                    buttons: 0,
                    axes: centre,
                },
            },
            &mut input,
            &mut clock,
        )
        .unwrap();
    let list = devices.frontend_devices(&mut input, 10).unwrap();
    assert_eq!(list.len(), 2);
    let mut sub =
        crate::frontend_bindings::FrontendBindings::new(0x77, assets.screen_analog_descriptors());
    sub.install_sub_object(
        &mut input,
        context,
        assets.gameplay_sub_input_tables(),
        &list,
    )
    .unwrap();
    input.select_context(Some(context)).unwrap();
    (input, clock, ids, devices, sub)
}

/// The joystick from WinMM's reading to the control block: the pad's type-2
/// device calibrated at Sensitivity 10 (00439AD0), the sub-object's slot-2
/// table 005231D8 (X to +5C at 4, Y to +60 and Z to +64 at -4, Q31 one
/// half), then 0046A260's Analogue and Digital modes.
#[test]
fn the_joystick_reaches_the_control_block_in_both_modes() {
    use crate::platform_input::{InputNotification, PadSample};
    let (mut input, mut clock, ids, mut devices, _sub) = sub_object_chain(2, 5, [0x8000; 6]);
    let reading = |axes| InputNotification::Pad {
        native_type: 2,
        axes: 5,
        sample: PadSample::Read { buttons: 0, axes },
    };
    assert!(devices.joystick_live());
    // Full right, pushed forward (Y at 0), the left trigger in (Z full).
    devices
        .update(
            reading([0xffff, 0, 0xffff, 0x8000, 0x8000, 0]),
            &mut input,
            &mut clock,
        )
        .unwrap();
    poll(&mut input, &mut clock, 1_000);
    // 00438850's multiplier at Sensitivity 10, then the record's 4 x 1/2.
    let calibrated = |raw: i64| ((raw - 0x8000) * 2 * 0x57ff_ffff) >> 31;
    let (x, y, z) = (
        (calibrated(0xffff) * 4 / 2) as i32,
        (calibrated(0) * -4 / 2) as i32,
        (calibrated(0xffff) * -4 / 2) as i32,
    );
    assert_eq!(input.analog_value(ids.analog_5c).unwrap(), x);
    assert_eq!(input.analog_value(ids.analog_60).unwrap(), y);
    assert_eq!(input.analog_value(ids.analog_64).unwrap(), z);

    let (mut arena, player) = self::player();
    let mode = |input_mode_08| PlayerUpdateInputs {
        input_mode_08,
        ..inputs(10_000)
    };
    arena
        .update_player_controls(player, mode(1), &mut input, &ids, DRIVING, None, None)
        .unwrap();
    let controls = arena.world_player(player).unwrap().controls;
    assert_eq!(controls.analog_04, 1);
    // +8 and +C clamp to a unit; +10 does not.
    assert_eq!(controls.axes_08, [0x10000, 0x10000, z]);
    assert_eq!((controls.turn_14, controls.pitch_18), (0, 0));

    arena
        .update_player_controls(player, mode(2), &mut input, &ids, DRIVING, None, None)
        .unwrap();
    let controls = arena.world_player(player).unwrap().controls;
    assert_eq!(controls.analog_04, 0);
    // Past half deflection, against the sign, at the uncapped delta.
    assert_eq!((controls.turn_14, controls.pitch_18), (-10_000, -10_000));
    // The axes keep their last Analogue values.
    assert_eq!(controls.axes_08, [0x10000, 0x10000, z]);
}

/// The PlayStation pad (native type 9) in play: the sub-object's slots 8/9
/// table 00523238 sends its left stick's X to +5C at 1024 and Y to +60 and
/// the right stick's vertical (axis 3) to +64 at -1024, after 00533764's
/// -0x80 bias. A failed read holds its status input, so it stops counting.
#[test]
fn the_playstation_pad_reaches_the_control_block() {
    use crate::platform_input::{InputNotification, PadSample};
    let centre = [0x80, 0x80, 0x80, 0x80, 0, 0];
    let (mut input, mut clock, ids, mut devices, _sub) = sub_object_chain(9, 4, centre);
    assert!(devices.joystick_live());
    // Left stick full right and up, the right stick up.
    devices
        .update(
            InputNotification::Pad {
                native_type: 9,
                axes: 4,
                sample: PadSample::Read {
                    buttons: 0,
                    axes: [0xff, 0, 0x80, 0, 0, 0],
                },
            },
            &mut input,
            &mut clock,
        )
        .unwrap();
    poll(&mut input, &mut clock, 1_000);
    let calibrated = |raw: i64| ((raw - 0x80) * 2 * 0x57ff_ffff) >> 31;
    let (x, y, z) = (
        (calibrated(0xff) * 1024 / 2) as i32,
        (calibrated(0) * -1024 / 2) as i32,
        (calibrated(0) * -1024 / 2) as i32,
    );
    assert_eq!(input.analog_value(ids.analog_5c).unwrap(), x);
    assert_eq!(input.analog_value(ids.analog_60).unwrap(), y);
    assert_eq!(input.analog_value(ids.analog_64).unwrap(), z);
    let (mut arena, player) = self::player();
    let inputs = PlayerUpdateInputs {
        input_mode_08: 1,
        ..self::inputs(10_000)
    };
    arena
        .update_player_controls(player, inputs, &mut input, &ids, DRIVING, None, None)
        .unwrap();
    let controls = arena.world_player(player).unwrap().controls;
    assert_eq!(controls.axes_08, [x.clamp(-0x10000, 0x10000), 0x10000, z]);
    devices
        .update(
            InputNotification::Pad {
                native_type: 9,
                axes: 4,
                sample: PadSample::Failed,
            },
            &mut input,
            &mut clock,
        )
        .unwrap();
    assert!(!devices.joystick_live());
}

#[test]
fn the_message_timer_needs_its_fields_only_when_a_request_is_queued() {
    let mut input = InputRuntime::new(0);
    let ids = consumers(&mut input);

    // An unwritten +50 is never read as zero.
    let mut arena = ObjectArena::new();
    let unknown = arena.add_world_player([0; 6]);
    assert!(matches!(
        arena.update_player_controls(unknown, inputs(1_000), &mut input, &ids, None, None, None),
        Err(PlayerUpdateError::UnknownField(_, 0x50))
    ));

    // With no request the unwritten +54 only paces 0046D240's no-op.
    let (mut arena, player) = player();
    arena
        .update_player_controls(player, inputs(1_000), &mut input, &ids, None, None, None)
        .unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_54, None);

    // A running timer counts down whole milliseconds of the delta.
    arena.world_player_mut(player).unwrap().dword_54 = Some(10);
    arena
        .update_player_controls(player, inputs(4_999), &mut input, &ids, None, None, None)
        .unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_54, Some(6));

    // Expiry with a held key rearms +100 and runs 0046D240: the select
    // (bit 1) is the omitted call, and its bit clears.
    let state = arena.world_player_mut(player).unwrap();
    state.dword_50 = Some(1);
    let update = arena
        .update_player_controls(player, inputs(8_000), &mut input, &ids, None, None, None)
        .unwrap();
    assert_eq!(
        update.omissions.into_iter().collect::<Vec<_>>(),
        [crate::object_runtime::PlayerKeyOmission::MessageSelect]
    );
    let state = arena.world_player(player).unwrap();
    assert_eq!((state.dword_54, state.dword_50), (Some(98), Some(0)));

    // A request above bit 4 rearms without paging; no request zeroes.
    let state = arena.world_player_mut(player).unwrap();
    state.dword_50 = Some(0x20);
    state.dword_54 = Some(0);
    arena
        .update_player_controls(player, inputs(2_000), &mut input, &ids, None, None, None)
        .unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_54, Some(98));
    arena.world_player_mut(player).unwrap().dword_50 = Some(0);
    arena
        .update_player_controls(player, inputs(200_000), &mut input, &ids, None, None, None)
        .unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_54, Some(0));
}

#[test]
fn command_3_charges_12c_every_eight_seconds() {
    let (mut arena, player) = player();
    // Without command 3 nothing counts.
    arena.finish_player_update(player, 9_000_000).unwrap();
    let state = arena.world_player(player).unwrap();
    assert_eq!((state.dword_f8, state.dword_12c), (0, 0));

    arena.world_player_mut(player).unwrap().bits_100[0] |= 1 << 3;
    arena.finish_player_update(player, 8_000_000).unwrap();
    let state = arena.world_player(player).unwrap();
    assert_eq!((state.dword_f8, state.dword_12c), (8_000_000, 0));
    arena.finish_player_update(player, 1).unwrap();
    let state = arena.world_player(player).unwrap();
    assert_eq!((state.dword_f8, state.dword_12c), (0, 1));

    // 0046A81B..0046A83E: a count of 3 still rises to 4; above that, 3.
    arena.world_player_mut(player).unwrap().dword_12c = 3;
    arena.finish_player_update(player, 8_000_001).unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_12c, 4);
    arena.finish_player_update(player, 8_000_001).unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_12c, 3);
}

/// 0046A27D..0046A2BE: under a shown message, held Fire presses message
/// key bit 1 once (latched by +FC): without the message environment the
/// select is the omitted call and its bit clears at once. Releasing Fire
/// clears the latch. The block brakes without a unit: +3E, zero thrust.
#[test]
fn a_shown_message_makes_fire_select_once() {
    let (mut arena, player) = player();
    arena
        .world_player_mut(player)
        .unwrap()
        .set_message_byte_00(1);
    let mut input = InputRuntime::new(0);
    let mut clock = Clock(0);
    let ids = consumers(&mut input);
    press(&mut input, &mut clock, ids.fire_80, 0);
    press(&mut input, &mut clock, ids.thrust_78, 0);
    poll(&mut input, &mut clock, 100_000);
    let update = arena
        .update_player_controls(
            player,
            inputs(80_000),
            &mut input,
            &ids,
            DRIVING,
            None,
            None,
        )
        .unwrap();
    assert_eq!(update.call, PlayerVehicleCall::Update44);
    assert_eq!(
        update.omissions.into_iter().collect::<Vec<_>>(),
        [PlayerKeyOmission::MessageSelect]
    );
    let state = arena.world_player(player).unwrap();
    // The press restarts the +54 repeat at 250 ms; the same update's timer
    // then counts the 80 ms delta off it.
    assert_eq!(
        (state.byte_fc, state.dword_50, state.dword_54),
        (1, Some(0), Some(170))
    );
    let controls = state.controls;
    assert_eq!(controls.byte(0x3e), 1);
    assert_eq!(
        (controls.thrust_1c, controls.fire_20, controls.turn_14),
        (0, 0, 0)
    );
    // Still held: no second press.
    poll(&mut input, &mut clock, 200_000);
    let update = arena
        .update_player_controls(
            player,
            inputs(80_000),
            &mut input,
            &ids,
            DRIVING,
            None,
            None,
        )
        .unwrap();
    assert!(update.omissions.is_empty());
    // Released: the latch clears.
    clock.0 = 210_000;
    input
        .emit(ids.fire_80, 0, &mut EventPacket::default(), &mut clock)
        .unwrap();
    // The poll after the release still saw it held; the next sees none.
    for at in [400_000, 500_000] {
        poll(&mut input, &mut clock, at);
        arena
            .update_player_controls(
                player,
                inputs(80_000),
                &mut input,
                &ids,
                DRIVING,
                None,
                None,
            )
            .unwrap();
    }
    assert_eq!(arena.world_player(player).unwrap().byte_fc, 0);
}
