//! Synthetic ownership checks plus required local retail table integration.
//! The host records callback requests; it is not a platform or screen backend.

use super::*;
use crate::input::{DeviceButtons, DeviceSnapshot, EventPacket, InputCallback, InputHost};
use inf_formats::{AnalogInputRecord, DigitalInputRecord};

#[derive(Default)]
struct RecordingHost {
    callbacks: Vec<InputCallback>,
}

impl InputHost for RecordingHost {
    type Error = &'static str;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(1234)
    }
    fn callback(&mut self, event: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        self.callbacks.push(event);
        Ok(())
    }
}

fn analog(identity: u32) -> inf_formats::AnalogActionDescriptor {
    inf_formats::AnalogActionDescriptor {
        identity,
        file_span: Some(ByteSpan::new(0x900, 0x90c)),
        callback_address: 0,
        flags: 0,
        period_micros: 0,
    }
}

fn descriptors() -> ScreenAnalogDescriptors {
    ScreenAnalogDescriptors {
        consumer_cc: analog(SCREEN_ANALOG_CC_DESCRIPTOR),
        consumer_d0: analog(SCREEN_ANALOG_D0_DESCRIPTOR),
    }
}

fn digital(identity: u32) -> inf_formats::DigitalActionDescriptor {
    inf_formats::DigitalActionDescriptor {
        identity,
        file_span: ByteSpan::new(0x800, 0x808),
        callback_address: identity,
        flags: 2,
    }
}

fn digital_table(
    slot: u32,
    descriptors: &[inf_formats::DigitalActionDescriptor],
) -> DigitalInputTable {
    let address = 0x1000 + slot * 0x100;
    let start = 0x100 + slot as usize * 0x100;
    DigitalInputTable {
        virtual_address: address,
        records: descriptors
            .iter()
            .enumerate()
            .map(|(i, &descriptor)| DigitalInputRecord {
                virtual_address: address + (i * 20) as u32,
                file_span: ByteSpan::new(start + i * 20, start + (i + 1) * 20),
                descriptor,
                required: [[1, 0, 0, 0], [99, 0, 0, 0]],
                forbidden: [[0; 4]; 2],
            })
            .collect(),
        terminator: ByteSpan::new(
            start + descriptors.len() * 20,
            start + descriptors.len() * 20 + 4,
        ),
    }
}

fn analog_table(
    slot: u32,
    descriptors: &[inf_formats::AnalogActionDescriptor],
) -> AnalogInputTable {
    let address = 0x5000 + slot * 0x100;
    let start = 0x1000 + slot as usize * 0x100;
    AnalogInputTable {
        virtual_address: address,
        records: descriptors
            .iter()
            .enumerate()
            .map(|(i, &descriptor)| AnalogInputRecord {
                virtual_address: address + (i * 24) as u32,
                file_span: ByteSpan::new(start + i * 24, start + (i + 1) * 24),
                descriptor,
                axis: i as u32,
                flags: 0,
                offset: 0,
                scale_q31: 0x4000_0000,
                multiplier: 2,
            })
            .collect(),
        terminator: ByteSpan::new(
            start + descriptors.len() * 24,
            start + descriptors.len() * 24 + 4,
        ),
    }
}

fn tables() -> ScreenInputTables {
    ScreenInputTables {
        digital: std::array::from_fn(|_| None),
        analog: std::array::from_fn(|_| None),
    }
}

fn initialized(input: &mut InputRuntime) -> FrontendBindings {
    let mut owner = FrontendBindings::new(77, descriptors());
    owner.initialize(input).unwrap();
    owner
}

fn device(input: &mut InputRuntime, native_type: u32) -> FrontendDevice {
    FrontendDevice {
        native_type,
        source: input.create_device(DeviceButtons::default()).unwrap(),
    }
}

fn pressed() -> DeviceSnapshot {
    let mut snapshot = DeviceSnapshot::default();
    snapshot.buttons.set(1, true);
    snapshot.axes[0] = 10;
    snapshot
}

#[test]
fn metadata_then_base_acquisitions_exist_without_enumerated_devices_or_selection() {
    let mut input = InputRuntime::default();
    let prior = input.create_context().unwrap();
    input.select_context(Some(prior)).unwrap();
    let mut owner = FrontendBindings::new(77, descriptors());
    assert_eq!(owner.phase(), FrontendBindingPhase::Metadata);
    assert_eq!(owner.context(), None);
    assert!(matches!(
        owner.analog_values(&input),
        Err(FrontendBindingError::MissingScreenConsumer(0xcc))
    ));
    owner.initialize(&mut input).unwrap();
    let cc = owner.consumer_cc().unwrap();
    let d0 = owner.consumer_d0().unwrap();
    assert_eq!(input.analog_consumer_state(cc).unwrap().references, 1);
    assert_eq!(input.analog_consumer_state(d0).unwrap().references, 1);
    let mut tables = tables();
    tables.digital[0] = Some(digital_table(0, &[digital(10)]));
    // Unsupported data for an ABSENT device is never instantiated.
    let mut timed = analog(20);
    timed.period_micros = 500;
    tables.analog[2] = Some(analog_table(2, &[timed]));
    owner.install(&mut input, &tables, &[]).unwrap();
    assert!(owner.digital.iter().all(|owner| owner.set.is_none()));
    assert!(owner.analog.iter().all(|owner| owner.set.is_none()));
    assert_eq!(input.selected_context(), Some(prior));
    assert!(matches!(
        owner.release_base(&mut input),
        Err(FrontendBindingError::Phase { .. })
    ));
    owner.release_sets(&mut input).unwrap();
    assert_eq!(input.analog_consumer_state(cc).unwrap().references, 1);
    owner.release_base(&mut input).unwrap();
    assert_eq!(owner.phase(), FrontendBindingPhase::Released);
    assert_eq!(input.selected_context(), Some(prior));
    assert_eq!(
        input.analog_value(cc),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(
        input.analog_value(d0),
        Err(InputStateError::InvalidAnalogConsumer)
    );
}

#[test]
fn attachment_is_digital_then_analog_in_slot_order_despite_reverse_enumeration() {
    let mut input = InputRuntime::default();
    let mut owner = initialized(&mut input);
    let source = device(&mut input, 8).source;
    let mut tables = tables();
    for slot in [2, 8] {
        tables.digital[slot] = Some(digital_table(slot as u32, &[digital(10 + slot as u32)]));
        let mut descriptor = analog(20 + slot as u32);
        descriptor.callback_address = descriptor.identity;
        descriptor.flags = 2;
        tables.analog[slot] = Some(analog_table(slot as u32, &[descriptor]));
    }
    owner
        .install(
            &mut input,
            &tables,
            &[
                FrontendDevice {
                    native_type: 8,
                    source,
                },
                FrontendDevice {
                    native_type: 2,
                    source,
                },
            ],
        )
        .unwrap();
    assert_eq!(input.selected_context(), None);
    input.select_context(owner.context()).unwrap();
    let mut host = RecordingHost::default();
    input.update_device(source, pressed(), &mut host).unwrap();
    assert!(host.callbacks.is_empty());
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.callbacks
            .iter()
            .map(|event| (event.address, event.value))
            .collect::<Vec<_>>(),
        [(12, 1), (18, 1), (22, 10), (28, 10)]
    );
    assert!(host
        .callbacks
        .iter()
        .all(|event| event.context == 77 && event.timestamp_micros == 1234));
    // Source B required code 99 was preserved in the table, but native frontend
    // installation supplies no B source and never requires that device bit.
    assert!(!input.device_buttons(source).unwrap().pressed(99));
}

#[test]
fn per_record_references_share_consumers_but_sets_and_base_refs_have_separate_lifetimes() {
    let mut input = InputRuntime::default();
    let mut owner = initialized(&mut input);
    let devices = [device(&mut input, 2), device(&mut input, 8)];
    let mut tables = tables();
    for slot in [2, 8] {
        tables.digital[slot] = Some(digital_table(slot as u32, &[digital(10), digital(10)]));
        tables.analog[slot] = Some(analog_table(
            slot as u32,
            &[descriptors().consumer_cc, descriptors().consumer_d0],
        ));
    }
    owner.install(&mut input, &tables, &devices).unwrap();
    let digital = owner.digital[2].consumers[0];
    assert_eq!(owner.digital[2].consumers, owner.digital[8].consumers);
    assert_ne!(owner.digital[2].set, owner.digital[8].set);
    assert_eq!(input.consumer_state(digital).unwrap().references, 4);
    let cc = owner.consumer_cc().unwrap();
    let d0 = owner.consumer_d0().unwrap();
    assert_eq!(input.analog_consumer_state(cc).unwrap().references, 3);
    assert_eq!(input.analog_consumer_state(d0).unwrap().references, 3);
    input
        .acquire_consumer(action_descriptor(10, 10, 2), 77)
        .unwrap();
    input
        .acquire_analog_consumer(analog_descriptor(descriptors().consumer_cc), 77)
        .unwrap();
    input.select_context(owner.context()).unwrap();
    owner.release_sets(&mut input).unwrap();
    assert_eq!(input.consumer_state(digital).unwrap().references, 1);
    assert_eq!(input.analog_consumer_state(cc).unwrap().references, 2);
    assert_eq!(input.analog_consumer_state(d0).unwrap().references, 1);
    assert_eq!(input.selected_context(), owner.context());
    owner.release_base(&mut input).unwrap();
    assert_eq!(input.selected_context(), None);
    assert_eq!(input.analog_consumer_state(cc).unwrap().references, 1);
    assert_eq!(
        input.analog_value(d0),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    for device in devices {
        assert!(input.device_buttons(device.source).is_ok());
    }
}

#[test]
fn analog_first_record_policy_applies_to_all_cells_without_overwriting_device_calibration() {
    let mut input = InputRuntime::default();
    let mut owner = initialized(&mut input);
    let device = device(&mut input, 2);
    input
        .set_axis_transform(
            device.source,
            0,
            AxisTransform {
                offset: 4,
                q31_factor: 0x4000_0000,
                integer_factor: 3,
            },
        )
        .unwrap();
    let mut tables = tables();
    let mut table = analog_table(2, &[descriptors().consumer_cc; 2]);
    table.records[0].flags = 1;
    table.records[1].flags = 0;
    tables.analog[2] = Some(table);
    owner.install(&mut input, &tables, &[device]).unwrap();
    input.select_context(owner.context()).unwrap();
    let mut values = pressed();
    values.axes[1] = 20;
    let mut host = RecordingHost::default();
    input
        .update_device(device.source, values, &mut host)
        .unwrap();
    assert_eq!(owner.analog_values(&input).unwrap().consumer_cc, 0);
    input.poll(&mut host).unwrap();
    assert_eq!(owner.analog_values(&input).unwrap().consumer_cc, 41);
    assert!(
        host.callbacks.is_empty(),
        "null descriptors still publish analog state"
    );
}

#[test]
fn failed_creation_retains_enumeration_order_acquisitions_and_never_attaches_or_retries() {
    let mut input = InputRuntime::default();
    let mut owner = initialized(&mut input);
    let devices = [device(&mut input, 8), device(&mut input, 2)];
    let conflicting = input
        .acquire_consumer(action_descriptor(30, 999, 2), 77)
        .unwrap();
    let mut tables = tables();
    tables.digital[8] = Some(digital_table(8, &[digital(10)]));
    tables.analog[8] = Some(analog_table(8, &[descriptors().consumer_cc]));
    tables.digital[2] = Some(digital_table(2, &[digital(20), digital(30)]));
    let error = owner.install(&mut input, &tables, &devices).unwrap_err();
    assert_eq!(
        error,
        FrontendBindingError::Input(InputStateError::DescriptorConflict)
    );
    assert_eq!(owner.phase(), FrontendBindingPhase::Stopped);
    assert!(owner.digital[8].set.is_some());
    assert!(owner.analog[8].set.is_some());
    assert!(owner.digital[2].set.is_none());
    let partial = owner.digital[2].consumers[0];
    let completed = owner.digital[8].consumers[0];
    assert_eq!(input.consumer_state(partial).unwrap().references, 1);
    assert_eq!(
        input
            .analog_consumer_state(owner.consumer_cc().unwrap())
            .unwrap()
            .references,
        2
    );
    input.select_context(owner.context()).unwrap();
    input
        .update_device(devices[0].source, pressed(), &mut RecordingHost::default())
        .unwrap();
    assert_eq!(
        input.queued_len(),
        0,
        "attachment begins only after all creation succeeds"
    );
    assert!(matches!(
        owner.install(&mut input, &tables, &devices),
        Err(FrontendBindingError::Phase { .. })
    ));
    owner.release_sets(&mut input).unwrap();
    assert_eq!(
        input.consumer_state(partial),
        Err(InputStateError::InvalidConsumer)
    );
    assert_eq!(
        input.consumer_state(completed),
        Err(InputStateError::InvalidConsumer)
    );
    assert_eq!(input.consumer_state(conflicting).unwrap().references, 1);
    owner.release_base(&mut input).unwrap();
}

#[test]
fn base_failure_retains_context_and_first_acquisition_for_explicit_teardown() {
    let mut input = InputRuntime::default();
    let mut conflicting = analog_descriptor(descriptors().consumer_d0);
    conflicting.action.flags = 4;
    let existing = input.acquire_analog_consumer(conflicting, 77).unwrap();
    let mut owner = FrontendBindings::new(77, descriptors());
    assert_eq!(
        owner.initialize(&mut input),
        Err(FrontendBindingError::Input(
            InputStateError::DescriptorConflict
        ))
    );
    let context = owner.context().unwrap();
    let cc = owner.consumer_cc().unwrap();
    assert_eq!(owner.consumer_d0(), None);
    assert_eq!(input.analog_consumer_state(cc).unwrap().references, 1);
    assert!(matches!(
        owner.initialize(&mut input),
        Err(FrontendBindingError::Phase { .. })
    ));
    owner.release_sets(&mut input).unwrap();
    owner.release_base(&mut input).unwrap();
    assert_eq!(
        input.select_context(Some(context)),
        Err(InputStateError::InvalidContext)
    );
    assert_eq!(
        input.analog_value(cc),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(input.analog_consumer_state(existing).unwrap().references, 1);
}

#[test]
fn invalid_device_enumeration_is_explicit_and_acquires_no_sets() {
    let mut input = InputRuntime::default();
    let source = device(&mut input, 0).source;
    let foreign = device(&mut InputRuntime::default(), 0).source;
    for (devices, expected) in [
        (
            vec![FrontendDevice {
                native_type: 12,
                source,
            }],
            FrontendBindingError::DeviceType(12),
        ),
        (
            vec![
                FrontendDevice {
                    native_type: 0,
                    source
                };
                2
            ],
            FrontendBindingError::DuplicateDeviceType(0),
        ),
        (
            vec![FrontendDevice {
                native_type: 0,
                source: foreign,
            }],
            FrontendBindingError::Device {
                native_type: 0,
                source: InputStateError::InvalidDevice,
            },
        ),
    ] {
        let mut owner = initialized(&mut input);
        assert_eq!(
            owner.install(&mut input, &tables(), &devices),
            Err(expected)
        );
        assert!(owner.digital.iter().all(|owner| owner.consumers.is_empty()));
        owner.release_sets(&mut input).unwrap();
        owner.release_base(&mut input).unwrap();
    }
}

#[test]
fn required_descriptor_provenance_period_and_axis_errors_are_not_success_defaults() {
    let mut input = InputRuntime::default();
    let device = device(&mut input, 2);
    let mut bad = descriptors();
    bad.consumer_cc.identity = 0;
    let mut owner = FrontendBindings::new(77, bad);
    assert!(matches!(
        owner.initialize(&mut input),
        Err(FrontendBindingError::ScreenDescriptor { .. })
    ));
    assert_eq!(owner.context(), None);
    for (table, expected) in [
        (
            {
                let mut table = analog_table(2, &[analog(20)]);
                table.records[0].descriptor.period_micros = 1;
                table
            },
            FrontendBindingError::Input(InputStateError::UnsupportedAnalogTimer(1)),
        ),
        (
            {
                let mut table = analog_table(2, &[analog(20)]);
                table.records[0].axis = 8;
                table
            },
            FrontendBindingError::Input(InputStateError::InvalidAxis(8)),
        ),
        (
            {
                let mut table = analog_table(2, &[analog(20)]);
                table.records[0].descriptor.file_span.as_mut().unwrap().end -= 1;
                table
            },
            FrontendBindingError::DescriptorProvenance(20),
        ),
        (
            {
                let mut table = analog_table(2, &[analog(20)]);
                table.records[0].virtual_address += 4;
                table
            },
            FrontendBindingError::TableProvenance {
                native_type: 2,
                address: 0x5200,
            },
        ),
    ] {
        let mut owner = initialized(&mut input);
        let mut tables = tables();
        tables.analog[2] = Some(table);
        assert_eq!(owner.install(&mut input, &tables, &[device]), Err(expected));
        assert!(owner.analog[2].consumers.is_empty());
        owner.release_sets(&mut input).unwrap();
        owner.release_base(&mut input).unwrap();
    }
}

#[test]
fn teardown_progress_is_interleaved_by_slot_and_base_releases_d0_before_cc() {
    let mut input = InputRuntime::default();
    let mut owner = initialized(&mut input);
    let devices = [device(&mut input, 0), device(&mut input, 1)];
    let mut tables = tables();
    tables.digital[0] = Some(digital_table(0, &[digital(10)]));
    tables.analog[0] = Some(analog_table(0, &[analog(20)]));
    tables.digital[1] = Some(digital_table(1, &[digital(11)]));
    owner.install(&mut input, &tables, &devices).unwrap();
    let first = owner.digital[0].consumers[0];
    let later = owner.digital[1].consumers[0];
    // A caller violating ownership is reported after earlier successful native
    // releases, without advancing to the next slot or retrying those releases.
    input
        .destroy_analog_binding_set(owner.analog[0].set.unwrap())
        .unwrap();
    assert_eq!(
        owner.release_sets(&mut input),
        Err(FrontendBindingError::Input(
            InputStateError::InvalidAnalogBindingSet
        ))
    );
    assert_eq!(
        input.consumer_state(first),
        Err(InputStateError::InvalidConsumer)
    );
    assert_eq!(input.consumer_state(later).unwrap().references, 1);
    assert_eq!(owner.digital[0].set, None);

    let mut base = initialized(&mut input);
    base.release_sets(&mut input).unwrap();
    // Use a distinct callback context to avoid sharing with the earlier owner.
    let mut isolated = FrontendBindings::new(88, descriptors());
    isolated.initialize(&mut input).unwrap();
    isolated.release_sets(&mut input).unwrap();
    let d0 = isolated.consumer_d0().unwrap();
    input
        .release_analog_consumer(isolated.consumer_cc().unwrap())
        .unwrap();
    assert_eq!(
        isolated.release_base(&mut input),
        Err(FrontendBindingError::Input(
            InputStateError::InvalidAnalogConsumer
        ))
    );
    assert_eq!(
        input.analog_value(d0),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(isolated.consumer_d0(), None);
    assert!(input.select_context(isolated.context()).is_ok());
}

#[test]
fn final_release_invalidates_unrelated_queued_targets_but_preserves_callback_snapshots() {
    let mut input = InputRuntime::default();
    let mut owner = initialized(&mut input);
    let device = device(&mut input, 0);
    let mut tables = tables();
    tables.digital[0] = Some(digital_table(0, &[digital(10)]));
    owner.install(&mut input, &tables, &[device]).unwrap();
    let external = input
        .acquire_consumer(action_descriptor(99, 99, 2), 99)
        .unwrap();
    let mut host = RecordingHost::default();
    input
        .emit(external, 1, &mut EventPacket::default(), &mut host)
        .unwrap();
    owner.release_sets(&mut input).unwrap();
    input.poll(&mut host).unwrap();
    assert_eq!(host.callbacks[0].address, 99);
    assert_eq!(host.callbacks[0].value, 1);
    assert!(!input.held(external).unwrap());
}

#[test]
fn retail_tables_drive_explicit_keyboard_and_analog_sources_with_exact_consumer_references() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
    let bytes = std::fs::read(path).expect("retail Infest.exe is required");
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let descriptors = ScreenAnalogDescriptors::from_executable(source).unwrap();
    let tables = inf_formats::parse_frontend_input_tables(source).unwrap();
    let mut input = InputRuntime::default();
    let mut owner = FrontendBindings::new(77, descriptors);
    owner.initialize(&mut input).unwrap();
    // These are explicitly supplied logical devices for the fixture, not a
    // claim that native type/table presence performed platform enumeration.
    let keyboard = device(&mut input, 0);
    let pointer = device(&mut input, 2);
    owner
        .install(&mut input, &tables, &[pointer, keyboard])
        .unwrap();
    assert_eq!(owner.digital[0].consumers.len(), 17);
    assert_eq!(owner.digital[2].consumers.len(), 16);
    assert_eq!(
        input
            .analog_consumer_state(owner.consumer_cc().unwrap())
            .unwrap()
            .references,
        2
    );
    assert_eq!(
        input
            .analog_consumer_state(owner.consumer_d0().unwrap())
            .unwrap()
            .references,
        2
    );
    input.select_context(owner.context()).unwrap();
    let mut host = RecordingHost::default();
    let mut key = DeviceSnapshot::default();
    key.buttons.set(0xd0, true);
    input
        .update_device(keyboard.source, key, &mut host)
        .unwrap();
    let mut movement = DeviceSnapshot::default();
    movement.axes[0] = 80;
    movement.axes[1] = 16;
    input
        .update_device(pointer.source, movement, &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.callbacks,
        [InputCallback {
            address: 0x0044_5240,
            context: 77,
            value: 1,
            timestamp_micros: 1234
        }]
    );
    assert_eq!(
        owner.analog_values(&input).unwrap(),
        ScreenAnalogValues {
            consumer_cc: 10,
            consumer_d0: -2
        }
    );
    let context = owner.context().unwrap();
    owner.release_sets(&mut input).unwrap();
    owner.release_base(&mut input).unwrap();
    assert_eq!(
        input.select_context(Some(context)),
        Err(InputStateError::InvalidContext)
    );
    assert!(input.device_buttons(keyboard.source).is_ok());
    assert!(input.device_buttons(pointer.source).is_ok());
}
