use super::*;

fn owner() -> ScreenInput {
    ScreenInput::new(ScreenDispatchPolicy::Frontend, 71)
}
fn frame(delta: i32, elapsed: i32) -> SchedulerTick {
    SchedulerTick {
        delta_micros: delta,
        elapsed_micros: elapsed,
    }
}
fn event(address: u32, value: u32) -> InputCallback {
    InputCallback {
        address,
        context: 71,
        value,
        timestamp_micros: u64::MAX,
    }
}

#[test]
fn profile_reset_preserves_high_held_word_countdown_and_latch() {
    let mut input = owner();
    input.held_mask = 0xa53c_fff7;
    input.repeat_millis = 135;
    input.analog_latched = true;
    input.clear_held_low_word();
    assert_eq!(input.held_mask(), 0xa53c_0000);
    assert_eq!(input.repeat_millis(), 135);
    assert!(input.analog_latched());
}

#[test]
fn direct_callback_mapping_handles_nonzero_values_and_release_ignores_timestamp() {
    for (address, button, action) in [
        (
            0x445200,
            ScreenButton::Confirm,
            Some(ScreenInputAction::Confirm),
        ),
        (
            0x445220,
            ScreenButton::Previous,
            Some(ScreenInputAction::Previous),
        ),
        (0x445240, ScreenButton::Next, Some(ScreenInputAction::Next)),
        (
            0x445260,
            ScreenButton::AdjustPrevious,
            Some(ScreenInputAction::AdjustPrevious),
        ),
        (
            0x445280,
            ScreenButton::AdjustNext,
            Some(ScreenInputAction::AdjustNext),
        ),
        (0x4451e0, ScreenButton::Back, Some(ScreenInputAction::Back)),
        (
            0x485300,
            ScreenButton::Auxiliary6,
            Some(ScreenInputAction::Auxiliary6),
        ),
        (
            0x42aa00,
            ScreenButton::Auxiliary7,
            Some(ScreenInputAction::Auxiliary7),
        ),
        (
            0x42a9d0,
            ScreenButton::RequestAttract,
            Some(ScreenInputAction::RequestAttract),
        ),
        (0x42aa30, ScreenButton::Bit200, None),
    ] {
        let mut input = owner();
        let mut log = Vec::new();
        input
            .digital_callback(event(address, u32::MAX), &mut |_, action| {
                log.push(action);
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(log, action.into_iter().collect::<Vec<_>>());
        assert_eq!(input.held_mask(), button.mask());
        assert_eq!(input.repeat_millis(), 250);
        input
            .digital_callback(event(address, 0), &mut |_, _| {
                Err::<(), _>("release dispatched")
            })
            .unwrap();
        assert_eq!(input.held_mask(), 0);
        assert_eq!(input.repeat_millis(), 250);
    }
}

#[test]
fn bad_callback_or_context_does_not_mutate_or_stop_owner() {
    let mut input = owner();
    let error = input
        .digital_callback(event(0x443490, 1), &mut |_, _| Ok::<_, ()>(()))
        .unwrap_err();
    assert_eq!(error, ScreenInputError::UnknownCallback(0x443490));
    let error = input
        .digital_callback(
            InputCallback {
                context: 72,
                ..event(0x445200, 1)
            },
            &mut |_, _| Ok::<_, ()>(()),
        )
        .unwrap_err();
    assert_eq!(
        error,
        ScreenInputError::Context {
            expected: 71,
            actual: 72
        }
    );
    assert_eq!(input.held_mask(), 0);
    assert_eq!(input.repeat_millis(), 0);
    assert!(!input.stopped());
}

#[test]
fn new_press_dispatches_existing_bits_before_post_callback_repeat_reset() {
    let mut input = owner();
    input.held_mask = ScreenButton::Previous.mask();
    input.repeat_millis = 17;
    let mut log = Vec::new();
    input
        .press(ScreenButton::Back, &mut |input, action| {
            log.push((action, input.held_mask(), input.repeat_millis()));
            if action == ScreenInputAction::Back {
                input.clear_held();
            }
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(
        log,
        [
            (ScreenInputAction::Previous, 0x22, 17),
            (ScreenInputAction::Back, 0x22, 17),
        ]
    );
    assert_eq!(input.held_mask(), 0);
    assert_eq!(input.repeat_millis(), 250);
}

#[test]
fn repeated_down_does_not_dispatch_or_reset_countdown() {
    let mut input = owner();
    input.held_mask = ScreenButton::Confirm.mask();
    input.repeat_millis = -5;
    input
        .press(ScreenButton::Confirm, &mut |_, _| {
            Err::<(), _>("duplicate press")
        })
        .unwrap();
    assert_eq!(input.repeat_millis(), -5);
}

#[test]
fn live_bit_checks_observe_callback_clear_before_later_base_and_frontend_slots() {
    let mut input = owner();
    input.held_mask = 0x3ff;
    let mut log = Vec::new();
    input
        .tick(
            frame(0, 0),
            ScreenAnalogValues::default(),
            &mut |input, action| {
                log.push(action);
                if action == ScreenInputAction::Previous {
                    input.clear_held();
                }
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(
        log,
        [ScreenInputAction::Confirm, ScreenInputAction::Previous]
    );
    assert_eq!(input.held_mask(), 0);
    // Expiry sets the next countdown before dispatch; callback clear preserves it.
    assert_eq!(input.repeat_millis(), 100);
}

#[test]
fn recursive_press_preserves_nested_then_remaining_outer_slot_order() {
    let mut input = owner();
    let mut log = Vec::new();
    input
        .press(ScreenButton::Confirm, &mut |input, action| {
            log.push(("outer", action));
            if action == ScreenInputAction::Confirm {
                input
                    .press(ScreenButton::Next, &mut |_, nested| {
                        log.push(("nested", nested));
                        Ok::<_, ()>(())
                    })
                    .unwrap();
            }
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(
        log,
        [
            ("outer", ScreenInputAction::Confirm),
            ("nested", ScreenInputAction::Confirm),
            ("nested", ScreenInputAction::Next),
            ("outer", ScreenInputAction::Next),
        ]
    );
    assert_eq!(input.repeat_millis(), 250);
}

#[test]
fn base_and_frontend_dispatch_policies_are_explicit() {
    let expected = [
        ScreenInputAction::Confirm,
        ScreenInputAction::Previous,
        ScreenInputAction::Next,
        ScreenInputAction::AdjustPrevious,
        ScreenInputAction::AdjustNext,
        ScreenInputAction::Back,
        ScreenInputAction::Auxiliary6,
        ScreenInputAction::Auxiliary7,
        ScreenInputAction::RequestAttract,
    ];
    for (policy, count) in [
        (ScreenDispatchPolicy::Base, 6),
        (ScreenDispatchPolicy::Frontend, 9),
    ] {
        let mut input = ScreenInput::new(policy, 71);
        input.held_mask = 0x3ff;
        let mut log = Vec::new();
        input
            .tick(
                frame(0, 0),
                ScreenAnalogValues::default(),
                &mut |_, action| {
                    log.push(action);
                    Ok::<_, ()>(())
                },
            )
            .unwrap();
        assert_eq!(log, expected[..count]);
        assert_eq!(input.held_mask(), 0x3ff);
    }
    assert_eq!(ScreenInputAction::Back.virtual_slot(), Some(0x38));
    assert_eq!(ScreenInputAction::Auxiliary6.virtual_slot(), None);
}

#[test]
fn analog_reversal_dispatches_both_directions_before_clearing_old_bit() {
    let mut input = owner();
    input.held_mask = ScreenButton::Previous.mask();
    input.repeat_millis = 200;
    let mut log = Vec::new();
    input
        .tick(
            frame(20_000, 20_000),
            ScreenAnalogValues {
                consumer_cc: 0,
                consumer_d0: -2501,
            },
            &mut |input, action| {
                log.push((action, input.held_mask(), input.analog_latched()));
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(
        log,
        [
            (ScreenInputAction::Previous, 6, false),
            (ScreenInputAction::Next, 6, false),
        ]
    );
    assert_eq!(input.held_mask(), ScreenButton::Next.mask());
    assert!(input.analog_latched());
    assert_eq!(input.repeat_millis(), 230);
}

#[test]
fn analog_priority_and_strict_thresholds_match_native_branch_order() {
    for (cc, d0, expected) in [
        (-5000, 2501, Some(ScreenInputAction::Previous)),
        (5000, -2501, Some(ScreenInputAction::Next)),
        (-2501, 2500, Some(ScreenInputAction::AdjustPrevious)),
        (2501, -2500, Some(ScreenInputAction::AdjustNext)),
        (2500, -2500, None),
        (-2500, 2500, None),
    ] {
        let mut input = owner();
        let mut log = Vec::new();
        input
            .tick(
                frame(1000, 1000),
                ScreenAnalogValues {
                    consumer_cc: cc,
                    consumer_d0: d0,
                },
                &mut |_, action| {
                    log.push(action);
                    Ok::<_, ()>(())
                },
            )
            .unwrap();
        assert_eq!(log, expected.into_iter().collect::<Vec<_>>());
    }
}

#[test]
fn neutral_analog_clears_all_direction_bits_only_after_analog_latch() {
    let mut input = owner();
    input.held_mask = 0x3ff;
    input.repeat_millis = 200;
    input
        .tick(
            frame(1000, 1000),
            ScreenAnalogValues::default(),
            &mut |_, _| Err::<(), _>("unexpected dispatch"),
        )
        .unwrap();
    assert_eq!(input.held_mask(), 0x3ff);
    input.analog_latched = true;
    input
        .tick(
            frame(1000, 1000),
            ScreenAnalogValues::default(),
            &mut |_, _| Err::<(), _>("unexpected dispatch"),
        )
        .unwrap();
    assert_eq!(input.held_mask(), 0x3e1);
    assert!(!input.analog_latched());
    assert_eq!(input.repeat_millis(), 198);
}

#[test]
fn analog_dispatch_can_clear_held_but_latch_and_repeat_writes_still_follow() {
    let mut input = owner();
    input
        .tick(
            frame(125_000, 1000),
            ScreenAnalogValues {
                consumer_cc: 0,
                consumer_d0: 2501,
            },
            &mut |input, _| {
                input.clear_held();
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(input.held_mask(), 0);
    assert!(input.analog_latched());
    assert_eq!(input.repeat_millis(), 125);
}

#[test]
fn repeat_is_once_per_tick_with_strict_deadline_and_negative_remainder() {
    let mut input = owner();
    input.held_mask = ScreenButton::Confirm.mask();
    input.repeat_millis = 250;
    let mut log = Vec::new();
    input
        .tick(
            frame(249_999, 249_999),
            ScreenAnalogValues::default(),
            &mut |_, action| {
                log.push(action);
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(input.repeat_millis(), 1);
    assert!(log.is_empty());
    input
        .tick(
            frame(1000, 1000),
            ScreenAnalogValues::default(),
            &mut |_, action| {
                log.push(action);
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(input.repeat_millis(), 100);
    input
        .tick(
            frame(350_000, 350_000),
            ScreenAnalogValues::default(),
            &mut |_, action| {
                log.push(action);
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(input.repeat_millis(), -150);
    assert_eq!(log.len(), 2);
    input
        .tick(
            frame(0, 0),
            ScreenAnalogValues::default(),
            &mut |_, action| {
                log.push(action);
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(input.repeat_millis(), -50);
    assert_eq!(log.len(), 3);
}

#[test]
fn age_uses_raw_time_and_per_call_truncation_while_repeat_uses_capped_time() {
    let mut input = owner();
    input.held_mask = ScreenButton::Confirm.mask();
    input.repeat_millis = 250;
    input
        .tick(
            frame(125_999, 900_999),
            ScreenAnalogValues::default(),
            &mut |_, _| Err::<(), _>("unexpected dispatch"),
        )
        .unwrap();
    assert_eq!(input.age_millis(), 900);
    assert_eq!(input.repeat_millis(), 125);
    for _ in 0..4 {
        input
            .tick(
                frame(999, 999),
                ScreenAnalogValues::default(),
                &mut |_, _| Err::<(), _>("unexpected dispatch"),
            )
            .unwrap();
    }
    assert_eq!(input.age_millis(), 900);
    assert_eq!(input.repeat_millis(), 125);
    input
        .tick(
            frame(-1999, -1999),
            ScreenAnalogValues::default(),
            &mut |_, _| Err::<(), _>("unexpected dispatch"),
        )
        .unwrap();
    assert_eq!(input.age_millis(), 899);
    assert_eq!(input.repeat_millis(), 126);
}

#[test]
fn native_age_and_repeat_word_arithmetic_wraps() {
    let mut input = owner();
    input.age_millis = i32::MAX;
    input.repeat_millis = i32::MAX;
    input
        .tick(
            frame(-1000, 1000),
            ScreenAnalogValues::default(),
            &mut |_, _| Ok::<_, ()>(()),
        )
        .unwrap();
    assert_eq!(input.age_millis(), i32::MIN);
    assert_eq!(input.repeat_millis(), i32::MIN);
}

#[test]
fn callback_failure_keeps_completed_state_and_stops_before_later_actions() {
    let mut input = owner();
    input.held_mask = ScreenButton::Confirm.mask();
    input.repeat_millis = 17;
    let mut log = Vec::new();
    let error = input
        .press(ScreenButton::Back, &mut |_, action| {
            log.push(action);
            Err("unavailable control")
        })
        .unwrap_err();
    assert_eq!(error, ScreenInputError::Dispatch("unavailable control"));
    assert_eq!(log, [ScreenInputAction::Confirm]);
    assert_eq!(input.held_mask(), 0x21);
    assert_eq!(input.repeat_millis(), 17);
    assert!(input.stopped());
    let error = input
        .tick(
            frame(1000, 1000),
            ScreenAnalogValues::default(),
            &mut |_, _| Ok::<_, ()>(()),
        )
        .unwrap_err();
    assert_eq!(error, ScreenInputError::Stopped);
    assert_eq!(input.age_millis(), 0);
}

#[test]
fn swallowed_nested_failure_stops_outer_dispatch_and_post_callback_repeat_write() {
    let mut input = owner();
    input.repeat_millis = 17;
    let mut log = Vec::new();
    let error = input
        .press(ScreenButton::Confirm, &mut |input, action| {
            log.push(action);
            let nested = input.press(ScreenButton::Next, &mut |_, _| Err("nested failure"));
            assert_eq!(nested, Err(ScreenInputError::Dispatch("nested failure")));
            Ok::<_, ()>(())
        })
        .unwrap_err();
    assert_eq!(error, ScreenInputError::Stopped);
    assert_eq!(log, [ScreenInputAction::Confirm]);
    assert_eq!(input.repeat_millis(), 17);
    assert_eq!(input.held_mask(), 5);
}

#[test]
fn new_unconsumed_bit_retriggers_other_held_actions_without_inventing_an_action() {
    let mut input = owner();
    input.held_mask = ScreenButton::Previous.mask();
    let mut log = Vec::new();
    input
        .press(ScreenButton::Bit200, &mut |_, action| {
            log.push(action);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(log, [ScreenInputAction::Previous]);
    assert_eq!(input.held_mask(), 0x202);
    assert_eq!(input.repeat_millis(), 250);
}

#[test]
fn shared_post_age_phase_preserves_analog_dispatch_repeat_and_outer_age_custody() {
    let mut input = ScreenInput::new(ScreenDispatchPolicy::Base, 71);
    let mut actions = Vec::new();
    input.add_age_millis(250);
    input
        .tick_after_age(
            frame(125_000, 250_000),
            ScreenAnalogDirection::Previous,
            &mut |_, action| {
                actions.push(action);
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(
        input.age_millis(),
        250,
        "post-age phase does not add raw age twice"
    );
    assert_eq!(actions, [ScreenInputAction::Previous]);
    assert_eq!(input.held_mask(), 2);
    assert!(input.analog_latched());
    assert_eq!(
        input.repeat_millis(),
        125,
        "post-dispatch250 consumes capped125"
    );
    input.add_age_millis(125);
    assert_eq!(input.age_millis(), 375);
    assert_eq!(input.repeat_millis(), 125);
    input.add_age_millis(i32::MAX);
    assert_eq!(input.age_millis(), i32::MIN + 374);
}
