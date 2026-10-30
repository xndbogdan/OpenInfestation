use super::*;
use crate::frontend_bindings::{FrontendBindings, ScreenAnalogDescriptors};
use crate::input::InputCallback;
use crate::scheduler::SchedulerTick;
use crate::screen_input::{ScreenButton, ScreenDispatchPolicy, ScreenInput, ScreenInputAction};
use inf_formats::{parse_frontend_input_tables, FrontendExecutable, ScreenInputTables};

const CONTEXT: u32 = 71;

fn executable() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("retail Infest.exe is required; copy the game into retail/")
}

struct Host {
    screen: ScreenInput,
    callbacks: Vec<InputCallback>,
    actions: Vec<ScreenInputAction>,
    fail_clock: bool,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            screen: ScreenInput::new(ScreenDispatchPolicy::Frontend, CONTEXT),
            callbacks: Vec::new(),
            actions: Vec::new(),
            fail_clock: false,
        }
    }
}

impl InputHost for Host {
    type Error = &'static str;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        if self.fail_clock {
            Err("clock unavailable")
        } else {
            Ok(1234)
        }
    }

    fn callback(&mut self, event: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        self.callbacks.push(event);
        if ScreenButton::from_callback(event.address).is_some() {
            self.screen
                .digital_callback(event, &mut |_, action| {
                    self.actions.push(action);
                    Ok::<_, ()>(())
                })
                .map_err(|_| "screen dispatch failed")?;
        }
        // Non-screen callbacks are only recorded by this observation fixture;
        // no native platform side effect is claimed or used by the adapter.
        Ok(())
    }
}

fn fixture() -> (
    InputRuntime,
    SdlInput,
    FrontendBindings,
    Host,
    ScreenInputTables,
) {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let tables = parse_frontend_input_tables(source).unwrap();
    let mut input = InputRuntime::default();
    let keyboard = SdlInput::attach(&mut input).unwrap();
    assert_eq!(keyboard.frontend_device().native_type, 0);
    let mut bindings = FrontendBindings::new(
        CONTEXT,
        ScreenAnalogDescriptors::from_executable(source).unwrap(),
    );
    bindings.initialize(&mut input).unwrap();
    bindings
        .install(&mut input, &tables, &[keyboard.frontend_device()])
        .unwrap();
    input.select_context(bindings.context()).unwrap();
    (input, keyboard, bindings, Host::default(), tables)
}

fn key(
    keyboard: &mut SdlInput,
    input: &mut InputRuntime,
    host: &mut Host,
    scancode: Scancode,
    pressed: bool,
) -> KeyboardUpdate {
    keyboard
        .update(InputNotification::Key { scancode, pressed }, input, host)
        .unwrap()
}

#[test]
fn every_authored_keyboard_code_is_supported_without_keypad_or_modifier_aliases() {
    let bytes = executable();
    let tables = parse_frontend_input_tables(FrontendExecutable::parse(&bytes).unwrap()).unwrap();
    let keyboard = tables.digital[0].as_ref().unwrap();
    assert_eq!(keyboard.records.len(), 17);
    assert!(tables.analog[0].is_none());
    let supported: Vec<_> = (0..512)
        .filter_map(Scancode::from_i32)
        .filter_map(native_keyboard_code)
        .collect();
    let mut distinct = supported.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        supported.len(),
        "physical keys must not alias"
    );
    for record in &keyboard.records {
        for group in record.required.into_iter().chain(record.forbidden) {
            for code in group.into_iter().take_while(|code| *code != 0) {
                assert!(supported.contains(&code), "missing native code {code:#x}");
            }
        }
    }
    for (scancode, native) in [
        (Scancode::Return, 0x1c),
        (Scancode::KpEnter, 0x9c),
        (Scancode::LAlt, 0x38),
        (Scancode::RAlt, 0xb8),
        (Scancode::Up, 0xc8),
        (Scancode::Kp8, 0x48),
    ] {
        assert_eq!(native_keyboard_code(scancode), Some(native));
    }
    assert_eq!(native_keyboard_code(Scancode::International1), None);
}

#[test]
fn authored_navigation_callbacks_receive_both_edges_in_order_before_one_poll() {
    let (mut input, mut keyboard, bindings, mut host, _) = fixture();
    for (scancode, address, action) in [
        (Scancode::Up, 0x0044_5220, ScreenInputAction::Previous),
        (Scancode::Down, 0x0044_5240, ScreenInputAction::Next),
        (
            Scancode::Left,
            0x0044_5260,
            ScreenInputAction::AdjustPrevious,
        ),
        (Scancode::Right, 0x0044_5280, ScreenInputAction::AdjustNext),
        (Scancode::Return, 0x0044_5200, ScreenInputAction::Confirm),
        (Scancode::Escape, 0x0044_51e0, ScreenInputAction::Back),
    ] {
        host.callbacks.clear();
        host.actions.clear();
        assert_eq!(
            key(&mut keyboard, &mut input, &mut host, scancode, true),
            KeyboardUpdate::Changed
        );
        assert_eq!(
            key(&mut keyboard, &mut input, &mut host, scancode, false),
            KeyboardUpdate::Changed
        );
        assert!(host.callbacks.is_empty());
        assert_eq!(input.queued_len(), 2);
        input.poll(&mut host).unwrap();
        assert_eq!(host.actions, [action]);
        assert_eq!(host.screen.held_mask(), 0);
        assert_eq!(
            host.callbacks,
            [1, 0].map(|value| InputCallback {
                address,
                context: CONTEXT,
                value,
                timestamp_micros: 1234,
            })
        );
    }
    assert_eq!(bindings.analog_values(&input).unwrap(), Default::default());
}

#[test]
fn os_repeat_preserves_native_screen_timer_and_focus_loss_emits_releases() {
    let (mut input, mut keyboard, bindings, mut host, _) = fixture();
    key(&mut keyboard, &mut input, &mut host, Scancode::Down, true);
    input.poll(&mut host).unwrap();
    assert_eq!(host.actions, [ScreenInputAction::Next]);
    assert_eq!(host.screen.repeat_millis(), 250);
    for _ in 0..4 {
        assert_eq!(
            key(&mut keyboard, &mut input, &mut host, Scancode::Down, true),
            KeyboardUpdate::Unchanged
        );
    }
    assert_eq!(input.queued_len(), 0);
    for delta in [125_000, 124_000, 1_000] {
        host.screen
            .tick(
                SchedulerTick {
                    delta_micros: delta,
                    elapsed_micros: delta,
                },
                bindings.analog_values(&input).unwrap(),
                &mut |_, action| {
                    host.actions.push(action);
                    Ok::<_, ()>(())
                },
            )
            .unwrap();
        assert_eq!(host.actions.len(), if delta == 1_000 { 2 } else { 1 });
    }
    assert_eq!(host.actions, [ScreenInputAction::Next; 2]);
    assert_eq!(host.screen.repeat_millis(), 100);
    assert_eq!(
        keyboard
            .update(InputNotification::FocusLost, &mut input, &mut host)
            .unwrap(),
        KeyboardUpdate::Changed
    );
    input.poll(&mut host).unwrap();
    assert_eq!(host.callbacks.last().unwrap().value, 0);
    assert_eq!(host.screen.held_mask(), 0);
    assert_eq!(host.screen.repeat_millis(), 100);
    assert_eq!(
        keyboard
            .update(InputNotification::FocusLost, &mut input, &mut host)
            .unwrap(),
        KeyboardUpdate::Unchanged
    );
    assert_eq!(input.queued_len(), 0);
}

#[test]
fn shared_confirm_bindings_hold_until_both_source_keys_release() {
    let (mut input, mut keyboard, _, mut host, _) = fixture();
    for (scancode, pressed) in [
        (Scancode::Return, true),
        (Scancode::Space, true),
        (Scancode::Return, false),
    ] {
        key(&mut keyboard, &mut input, &mut host, scancode, pressed);
    }
    input.poll(&mut host).unwrap();
    assert_eq!(host.callbacks.len(), 1);
    assert_eq!(host.screen.held_mask(), ScreenButton::Confirm.mask());
    key(&mut keyboard, &mut input, &mut host, Scancode::Space, false);
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.callbacks
            .iter()
            .map(|event| event.value)
            .collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(host.actions, [ScreenInputAction::Confirm]);
}

#[test]
fn left_alt_chords_use_authored_callbacks_and_do_not_alias_right_alt_or_keypad_enter() {
    let (mut input, mut keyboard, _, mut host, tables) = fixture();
    let alt_enter = tables.digital[0]
        .as_ref()
        .unwrap()
        .records
        .iter()
        .find(|record| record.required[0] == [0x38, 0x1c, 0, 0])
        .unwrap();
    assert_eq!(alt_enter.descriptor.flags, 0);
    key(&mut keyboard, &mut input, &mut host, Scancode::LAlt, true);
    key(&mut keyboard, &mut input, &mut host, Scancode::Return, true);
    assert!(host.callbacks.is_empty());
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.callbacks,
        [InputCallback {
            address: alt_enter.descriptor.callback_address,
            context: CONTEXT,
            value: 1,
            timestamp_micros: 0,
        }]
    );
    assert!(host.actions.is_empty());
    keyboard
        .update(InputNotification::FocusLost, &mut input, &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    host.callbacks.clear();
    key(&mut keyboard, &mut input, &mut host, Scancode::RAlt, true);
    key(&mut keyboard, &mut input, &mut host, Scancode::Return, true);
    input.poll(&mut host).unwrap();
    assert_eq!(host.actions, [ScreenInputAction::Confirm]);
    keyboard
        .update(InputNotification::FocusLost, &mut input, &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    host.callbacks.clear();
    key(
        &mut keyboard,
        &mut input,
        &mut host,
        Scancode::KpEnter,
        true,
    );
    input.poll(&mut host).unwrap();
    assert!(host.callbacks.is_empty());
    assert!(input
        .device_buttons(keyboard.frontend_device().source)
        .unwrap()
        .pressed(0x9c));
}

#[test]
fn unsupported_scancodes_leave_state_unchanged_and_real_host_failures_propagate() {
    let (mut input, mut keyboard, _, mut host, _) = fixture();
    let before = input
        .device_buttons(keyboard.frontend_device().source)
        .unwrap();
    assert_eq!(
        key(
            &mut keyboard,
            &mut input,
            &mut host,
            Scancode::International1,
            true
        ),
        KeyboardUpdate::UnsupportedScancode
    );
    assert_eq!(
        input
            .device_buttons(keyboard.frontend_device().source)
            .unwrap(),
        before
    );
    host.fail_clock = true;
    assert!(matches!(
        keyboard.update(
            InputNotification::Key {
                scancode: Scancode::Down,
                pressed: true
            },
            &mut input,
            &mut host
        ),
        Err(InputError::Host("clock unavailable"))
    ));
    assert!(input.stopped());
    assert!(matches!(
        keyboard.update(InputNotification::FocusLost, &mut input, &mut host),
        Err(InputError::State(InputStateError::Stopped))
    ));
}

#[test]
fn teardown_releases_binding_owners_but_keeps_the_physical_source_for_later_contexts() {
    let (mut input, mut keyboard, mut bindings, mut host, _) = fixture();
    bindings.release_sets(&mut input).unwrap();
    bindings.release_base(&mut input).unwrap();
    key(&mut keyboard, &mut input, &mut host, Scancode::Down, true);
    assert!(input
        .device_buttons(keyboard.frontend_device().source)
        .unwrap()
        .pressed(0xd0));
    assert_eq!(input.queued_len(), 0);
}

/// The pad in the frontend: a button confirms (slot 2's sixteen records),
/// its X reaches the screen's +CC at an eighth (00533350), and a failed
/// read holds only bit 31 and keeps the axes (004F548C).
#[test]
fn the_pad_confirms_and_moves_the_frontend_menus() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let tables = parse_frontend_input_tables(source).unwrap();
    let mut input = InputRuntime::default();
    let mut host = Host::default();
    let mut devices = SdlInput::attach(&mut input).unwrap();
    let reading = |buttons, x| InputNotification::Pad {
        native_type: 2,
        axes: 5,
        sample: PadSample::Read {
            buttons,
            axes: [x, 0x8000, 0x8000, 0x8000, 0x8000, 0],
        },
    };
    devices
        .update(reading(0, 0x8000), &mut input, &mut host)
        .unwrap();
    let list = devices.frontend_devices(&mut input, 10).unwrap();
    assert_eq!(list[1].native_type, 2);
    let pad = list[1].source;
    let mut bindings = FrontendBindings::new(
        CONTEXT,
        ScreenAnalogDescriptors::from_executable(source).unwrap(),
    );
    bindings.initialize(&mut input).unwrap();
    bindings.install(&mut input, &tables, &list).unwrap();
    input.select_context(bindings.context()).unwrap();

    for buttons in [1, 0] {
        assert_eq!(
            devices
                .update(reading(buttons, 0x8000), &mut input, &mut host)
                .unwrap(),
            KeyboardUpdate::Changed
        );
    }
    input.poll(&mut host).unwrap();
    assert_eq!(host.actions, [ScreenInputAction::Confirm]);

    devices
        .update(reading(0, 0xffff), &mut input, &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    let calibrated = ((0xffff_i64 - 0x8000) * 2 * 0x57ff_ffff) >> 31;
    assert_eq!(
        bindings.analog_values(&input).unwrap().consumer_cc,
        ((calibrated * 0x1000_0000) >> 31) as i32
    );

    devices
        .update(
            InputNotification::Pad {
                native_type: 2,
                axes: 5,
                sample: PadSample::Failed,
            },
            &mut input,
            &mut host,
        )
        .unwrap();
    let mut failed = DeviceButtons::default();
    failed.0[0] = 0x8000_0000;
    assert_eq!(input.device_buttons(pad).unwrap(), failed);
    input.poll(&mut host).unwrap();
    assert_eq!(
        bindings.analog_values(&input).unwrap().consumer_cc,
        ((calibrated * 0x1000_0000) >> 31) as i32
    );
}

/// The PlayStation pad (type 9) in the frontend: Cross (code 15, its bit
/// 14) confirms through slot 9's table, as on the console.
#[test]
fn the_playstation_pad_confirms_with_cross() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let tables = parse_frontend_input_tables(source).unwrap();
    let mut input = InputRuntime::default();
    let mut host = Host::default();
    let mut devices = SdlInput::attach(&mut input).unwrap();
    let reading = |buttons| InputNotification::Pad {
        native_type: 9,
        axes: 4,
        sample: PadSample::Read {
            buttons,
            axes: [0x80, 0x80, 0x80, 0x80, 0, 0],
        },
    };
    devices.update(reading(0), &mut input, &mut host).unwrap();
    let list = devices.frontend_devices(&mut input, 10).unwrap();
    assert_eq!(list[1].native_type, 9);
    let mut bindings = FrontendBindings::new(
        CONTEXT,
        ScreenAnalogDescriptors::from_executable(source).unwrap(),
    );
    bindings.initialize(&mut input).unwrap();
    bindings.install(&mut input, &tables, &list).unwrap();
    input.select_context(bindings.context()).unwrap();
    for buttons in [1 << 14, 0] {
        devices
            .update(reading(buttons), &mut input, &mut host)
            .unwrap();
    }
    input.poll(&mut host).unwrap();
    assert_eq!(host.actions, [ScreenInputAction::Confirm]);
}
