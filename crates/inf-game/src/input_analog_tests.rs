use super::*;
use crate::input::{DeviceButtons, DeviceSnapshot, DigitalBinding, InputCallback};

#[derive(Default)]
struct Host {
    callbacks: Vec<InputCallback>,
    samples: usize,
    calibration: Option<(DeviceId, AxisTransform)>,
    fail: bool,
}

impl InputHost for Host {
    type Error = &'static str;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        self.samples += 1;
        Ok(1234)
    }
    fn callback(
        &mut self,
        event: InputCallback,
        input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        self.callbacks.push(event);
        if let Some((device, transform)) = self.calibration.take() {
            input.set_axis_transform(device, 0, transform).unwrap();
        }
        if self.fail {
            Err("callback failed")
        } else {
            Ok(())
        }
    }
}

fn descriptor(identity: u32, flags: u32) -> AnalogActionDescriptor {
    AnalogActionDescriptor {
        action: ActionDescriptor {
            identity,
            callback_address: Some(identity),
            flags,
        },
        period_micros: 0,
    }
}

fn snapshot(x: i32, y: i32) -> DeviceSnapshot {
    DeviceSnapshot {
        axes: [x, y, 0, 0, 0, 0, 0, 0],
        ..DeviceSnapshot::default()
    }
}

fn setup(
    flags: u32,
    policy: AnalogPolicy,
    axes: &[u32],
) -> (
    InputRuntime,
    InputContextId,
    DeviceId,
    AnalogConsumerId,
    AnalogBindingSetId,
) {
    let mut input = InputRuntime::default();
    let context = input.create_context().unwrap();
    let device = input.create_device(DeviceButtons::default()).unwrap();
    let consumer = input
        .acquire_analog_consumer(descriptor(10, flags), 55)
        .unwrap();
    let records: Vec<_> = axes
        .iter()
        .map(|&axis| AnalogBinding {
            consumer,
            axis,
            transform: AxisTransform::NATIVE_DEFAULT,
        })
        .collect();
    let set = input
        .create_analog_binding_set(device, policy, &records)
        .unwrap();
    input.attach_analog_binding_set(context, set).unwrap();
    input.select_context(Some(context)).unwrap();
    (input, context, device, consumer, set)
}

#[test]
fn signed_transform_wraps_before_wide_multiply_and_truncates_arithmetically() {
    assert_eq!(AxisTransform::NATIVE_DEFAULT.apply(123), 123);
    assert_eq!(AxisTransform::NATIVE_DEFAULT.apply(i32::MAX), -1);
    assert_eq!(AxisTransform::NATIVE_DEFAULT.apply(i32::MIN), 0);
    let transform = AxisTransform {
        offset: 1,
        q31_factor: -0x4000_0000,
        integer_factor: 3,
    };
    assert_eq!(transform.apply(2), -5);
    assert_eq!(transform.apply(-4), 4);
    assert_eq!(transform.apply(i32::MAX), 1_073_741_824);
}

#[test]
fn shared_consumer_accumulates_cell_changes_and_preserves_analog_on_selection() {
    let (mut input, context, device, consumer, set) =
        setup(0, AnalogPolicy::AccumulateChanges, &[0, 1]);
    let mut host = Host::default();
    input
        .update_device(device, snapshot(20, 30), &mut host)
        .unwrap();
    assert_eq!(
        input.analog_consumer_state(consumer).unwrap().pending_value,
        50
    );
    assert_eq!(input.analog_value(consumer).unwrap(), 0);
    input.select_context(None).unwrap();
    input.select_context(Some(context)).unwrap();
    input.attach_analog_binding_set(context, set).unwrap();
    input
        .update_device(device, snapshot(25, 30), &mut host)
        .unwrap();
    assert_eq!(input.queued_len(), 3); // duplicate link shares unchanged previous values
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.callbacks.iter().map(|c| c.value).collect::<Vec<_>>(),
        [20, 50, 55]
    );
    assert_eq!(input.analog_value(consumer).unwrap(), 55);
    input.select_context(None).unwrap();
    assert_eq!(input.analog_value(consumer).unwrap(), 55);
}

#[test]
fn replace_uses_last_changed_cell_even_when_another_cell_has_same_consumer() {
    let (mut input, _, device, consumer, _) = setup(1, AnalogPolicy::Replace, &[0, 1]);
    let mut host = Host::default();
    input
        .update_device(device, snapshot(20, -30), &mut host)
        .unwrap();
    assert_eq!(input.analog_value(consumer).unwrap(), -30);
    input
        .update_device(device, snapshot(21, -30), &mut host)
        .unwrap();
    assert_eq!(input.analog_value(consumer).unwrap(), 21);
    assert_eq!(host.callbacks.len(), 3);
}

#[test]
fn device_and_binding_calibrations_remain_separate() {
    let (mut input, _, device, consumer, set) = setup(1, AnalogPolicy::Replace, &[0]);
    input
        .set_axis_transform(
            device,
            0,
            AxisTransform {
                offset: 4,
                q31_factor: 0x4000_0000,
                integer_factor: 3,
            },
        )
        .unwrap();
    input.analog_sets.get_mut(set.0).unwrap().cells[0]
        .binding
        .transform = AxisTransform {
        offset: -2,
        q31_factor: -0x4000_0000,
        integer_factor: 1,
    };
    input
        .update_device(device, snapshot(3, 0), &mut Host::default())
        .unwrap();
    assert_eq!(input.analog_value(consumer).unwrap(), -4); // (7*3)>>1 =10; -(10-2)/2
}

#[test]
fn digital_callback_precedes_calibration_and_both_families_share_timestamp() {
    let (mut input, context, device, consumer, _) = setup(7, AnalogPolicy::Replace, &[0]);
    let digital = input
        .acquire_consumer(
            ActionDescriptor {
                identity: 11,
                callback_address: Some(11),
                flags: 3,
            },
            55,
        )
        .unwrap();
    let set = input
        .create_binding_set(
            [Some(device), None],
            &[DigitalBinding {
                consumer: digital,
                required: [[1, 0, 0, 0], [0; 4]],
                forbidden: [[0; 4]; 2],
            }],
        )
        .unwrap();
    input.attach_binding_set(context, set).unwrap();
    let mut values = snapshot(10, 0);
    values.buttons.set(1, true);
    let mut host = Host {
        calibration: Some((
            device,
            AxisTransform {
                offset: 7,
                ..AxisTransform::NATIVE_DEFAULT
            },
        )),
        ..Host::default()
    };
    input.update_device(device, values, &mut host).unwrap();
    assert_eq!(host.samples, 1);
    assert_eq!(
        host.callbacks
            .iter()
            .map(|c| (c.address, c.value, c.timestamp_micros))
            .collect::<Vec<_>>(),
        [(11, 1, 1234), (10, 17, 1234)]
    );
    assert_eq!(input.analog_value(consumer).unwrap(), 17);
}

#[test]
fn references_belong_to_sets_and_contexts_only_link_them() {
    let (mut input, context, _, consumer, set) = setup(0, AnalogPolicy::Replace, &[0, 1]);
    assert_eq!(
        input
            .acquire_analog_consumer(descriptor(10, 0), 55)
            .unwrap(),
        consumer
    );
    assert_eq!(input.analog_consumer_state(consumer).unwrap().references, 4);
    input.destroy_context(context).unwrap();
    assert_eq!(input.analog_consumer_state(consumer).unwrap().references, 4);
    input.release_analog_consumer(consumer).unwrap();
    input.release_analog_consumer(consumer).unwrap();
    assert_eq!(
        input.release_analog_consumer(consumer),
        Err(InputStateError::NoConsumerReference)
    );
    input.destroy_analog_binding_set(set).unwrap();
    assert_eq!(
        input.analog_value(consumer),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(
        input.destroy_analog_binding_set(set),
        Err(InputStateError::InvalidAnalogBindingSet)
    );
}

#[test]
fn final_analog_release_invalidates_both_target_families_but_keeps_callbacks() {
    let (mut input, _, device, consumer, _) = setup(6, AnalogPolicy::Replace, &[0]);
    let other = input
        .acquire_analog_consumer(descriptor(12, 0), 55)
        .unwrap();
    let digital = input
        .acquire_consumer(
            ActionDescriptor {
                identity: 13,
                callback_address: Some(13),
                flags: 6,
            },
            55,
        )
        .unwrap();
    let mut host = Host::default();
    input
        .update_device(device, snapshot(10, 0), &mut host)
        .unwrap();
    input
        .emit(digital, 1, &mut EventPacket::default(), &mut host)
        .unwrap();
    input.release_analog_consumer(other).unwrap();
    input.poll(&mut host).unwrap();
    assert_eq!(input.analog_value(consumer).unwrap(), 0);
    assert!(!input.held(digital).unwrap());
    assert_eq!(
        host.callbacks.iter().map(|c| c.value).collect::<Vec<_>>(),
        [10, 1]
    );
}

#[test]
fn invalid_axis_foreign_consumer_and_timed_descriptor_fail_without_retaining() {
    let (mut input, _, device, consumer, _) = setup(0, AnalogPolicy::Replace, &[0]);
    let before = input.analog_consumer_state(consumer).unwrap();
    assert_eq!(
        input.create_analog_binding_set(
            device,
            AnalogPolicy::Replace,
            &[AnalogBinding {
                consumer,
                axis: 8,
                transform: AxisTransform::NATIVE_DEFAULT,
            }]
        ),
        Err(InputStateError::InvalidAxis(8))
    );
    let mut foreign = InputRuntime::default();
    let other = foreign
        .acquire_analog_consumer(descriptor(10, 0), 55)
        .unwrap();
    assert_eq!(
        input.create_analog_binding_set(
            device,
            AnalogPolicy::Replace,
            &[AnalogBinding {
                consumer: other,
                axis: 0,
                transform: AxisTransform::NATIVE_DEFAULT,
            }]
        ),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(
        input.acquire_analog_consumer(
            AnalogActionDescriptor {
                period_micros: 1000,
                ..descriptor(99, 0)
            },
            55
        ),
        Err(InputStateError::UnsupportedAnalogTimer(1000))
    );
    assert_eq!(input.analog_consumer_state(consumer).unwrap(), before);
}

#[test]
fn callback_failure_stops_before_later_cells_and_preserves_completed_publication() {
    let (mut input, _, device, consumer, _) = setup(1, AnalogPolicy::AccumulateChanges, &[0, 1]);
    let mut host = Host {
        fail: true,
        ..Host::default()
    };
    assert!(matches!(
        input.update_device(device, snapshot(10, 20), &mut host),
        Err(InputError::Host("callback failed"))
    ));
    assert_eq!(input.analog_value(consumer).unwrap(), 10);
    assert_eq!(
        input.analog_consumer_state(consumer).unwrap().pending_value,
        10
    );
    assert!(input.stopped());
}

#[test]
fn swallowed_nested_host_failure_stops_before_later_analog_cells() {
    struct SwallowNestedFailure {
        digital: crate::input::ConsumerId,
        callbacks: Vec<InputCallback>,
        nested_callbacks: usize,
    }

    impl InputHost for SwallowNestedFailure {
        type Error = &'static str;

        fn clock_micros(&mut self) -> Result<u64, Self::Error> {
            Ok(1234)
        }

        fn callback(
            &mut self,
            event: InputCallback,
            input: &mut InputRuntime,
        ) -> Result<(), Self::Error> {
            self.callbacks.push(event);
            let mut nested = Host {
                fail: true,
                ..Host::default()
            };
            assert!(matches!(
                input.emit(self.digital, 1, &mut EventPacket::default(), &mut nested),
                Err(InputError::Host("callback failed"))
            ));
            self.nested_callbacks += nested.callbacks.len();
            Ok(()) // Deliberately swallow the nested failure.
        }
    }

    let (mut input, _, device, consumer, set) = setup(1, AnalogPolicy::AccumulateChanges, &[0, 1]);
    let digital = input
        .acquire_consumer(
            ActionDescriptor {
                identity: 11,
                callback_address: Some(11),
                flags: 1,
            },
            55,
        )
        .unwrap();
    let mut host = SwallowNestedFailure {
        digital,
        callbacks: Vec::new(),
        nested_callbacks: 0,
    };

    assert!(matches!(
        input.update_device(device, snapshot(10, 20), &mut host),
        Err(InputError::State(InputStateError::Stopped))
    ));
    assert!(input.stopped());
    assert_eq!(input.analog_value(consumer).unwrap(), 10);
    assert_eq!(
        input.analog_consumer_state(consumer).unwrap().pending_value,
        10
    );
    let cells = &input.analog_sets.get(set.0).unwrap().cells;
    assert_eq!(cells[0].previous, 10);
    assert_eq!(cells[1].previous, 0);
    assert_eq!(host.callbacks.len(), 1);
    assert_eq!(host.callbacks[0].value, 10);
    assert_eq!(host.nested_callbacks, 1);
    assert!(input.held(digital).unwrap());
    assert!(!input.notifying_device);
}

#[test]
fn queue_overflow_drops_publication_but_keeps_cell_change() {
    let (mut input, _, device, consumer, _) = setup(0, AnalogPolicy::Replace, &[0]);
    let mut host = Host::default();
    for value in 1..=32 {
        input
            .update_device(device, snapshot(value, 0), &mut host)
            .unwrap();
    }
    assert_eq!(input.queued_len(), 31);
    assert_eq!(
        input.analog_consumer_state(consumer).unwrap().pending_value,
        32
    );
    input.poll(&mut host).unwrap();
    assert_eq!(input.analog_value(consumer).unwrap(), 31);
    input
        .update_device(device, snapshot(32, 0), &mut host)
        .unwrap();
    assert_eq!(input.queued_len(), 0);
    input
        .update_device(device, snapshot(33, 0), &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    assert_eq!(input.analog_value(consumer).unwrap(), 33);
}
