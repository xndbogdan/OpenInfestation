use super::*;
use crate::input::{
    AnalogBinding, AnalogPolicy, AxisTransform, DeviceButtons, DeviceSnapshot, EventPacket,
    InputCallback, InputHost,
};

const CONTEXT: u32 = 0x71a2;

fn expected_requests() -> Vec<PlayerInputDescriptorRequest> {
    let analog = [0x0054_ecf8, 0x0054_ece8, 0x0054_ed08];
    let digital = [
        0x0053_a368,
        0x0053_a370,
        0x0053_a358,
        0x0053_a360,
        0x0053_a378,
        0x0053_a380,
        0x0053_a388,
        0x0053_a390,
    ];
    analog
        .into_iter()
        .enumerate()
        .map(|(index, identity)| PlayerInputDescriptorRequest {
            kind: PlayerInputDescriptorKind::Analog,
            identity,
            player_offset: 0x5c + index as u32 * 4,
        })
        .chain(digital.into_iter().enumerate().map(|(index, identity)| {
            PlayerInputDescriptorRequest {
                kind: PlayerInputDescriptorKind::Digital,
                identity,
                player_offset: 0x68 + index as u32 * 4,
            }
        }))
        .collect()
}

fn supplied_descriptor(request: PlayerInputDescriptorRequest) -> PlayerInputDescriptor {
    match request.kind {
        PlayerInputDescriptorKind::Analog => {
            // Explicit known startup-zero metadata; no file-parser BSS fallback.
            PlayerInputDescriptor::Analog(AnalogActionDescriptor {
                action: ActionDescriptor {
                    identity: request.identity,
                    callback_address: None,
                    flags: 0,
                },
                period_micros: 0,
            })
        }
        PlayerInputDescriptorKind::Digital => PlayerInputDescriptor::Digital(ActionDescriptor {
            identity: request.identity,
            callback_address: (request.identity == 0x0053_a390).then_some(0x0046_94d0),
            flags: 6,
        }),
    }
}

#[derive(Clone, Copy)]
enum Fault {
    Source,
    Family,
    Identity,
    Period(i32),
}

#[derive(Default)]
struct Source {
    calls: Vec<PlayerInputDescriptorRequest>,
    fault: Option<(usize, Fault)>,
    analog_callback: bool,
}

impl PlayerInputDescriptorSource for Source {
    type Error = &'static str;

    fn descriptor(
        &mut self,
        request: PlayerInputDescriptorRequest,
    ) -> Result<PlayerInputDescriptor, Self::Error> {
        let index = self.calls.len();
        self.calls.push(request);
        let mut descriptor = supplied_descriptor(request);
        if self.analog_callback {
            if let PlayerInputDescriptor::Analog(value) = &mut descriptor {
                value.action.callback_address = Some(0x1111);
                value.action.flags = 6;
            }
        }
        if let Some((failed, fault)) = self.fault {
            if failed == index {
                match fault {
                    Fault::Source => return Err("current descriptor unavailable"),
                    Fault::Family => {
                        descriptor = match descriptor {
                            PlayerInputDescriptor::Analog(value) => {
                                PlayerInputDescriptor::Digital(value.action)
                            }
                            PlayerInputDescriptor::Digital(value) => {
                                PlayerInputDescriptor::Analog(AnalogActionDescriptor {
                                    action: value,
                                    period_micros: 0,
                                })
                            }
                        };
                    }
                    Fault::Identity => match &mut descriptor {
                        PlayerInputDescriptor::Analog(value) => value.action.identity ^= 4,
                        PlayerInputDescriptor::Digital(value) => value.identity ^= 4,
                    },
                    Fault::Period(period) => {
                        let PlayerInputDescriptor::Analog(value) = &mut descriptor else {
                            panic!("period fixture must name an analog slot");
                        };
                        value.period_micros = period;
                    }
                }
            }
        }
        Ok(descriptor)
    }
}

#[derive(Default)]
struct Host {
    callbacks: Vec<InputCallback>,
    samples: usize,
}

impl InputHost for Host {
    type Error = &'static str;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        self.samples += 1;
        Ok(0x1234)
    }

    fn callback(
        &mut self,
        event: InputCallback,
        _input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        self.callbacks.push(event);
        Ok(())
    }
}

fn assert_prefix(input: &InputRuntime, references: &PlayerInputReferences, count: usize) {
    for (index, id) in references.analog.iter().copied().enumerate() {
        assert_eq!(id.is_some(), index < count);
        if let Some(id) = id {
            let state = input.analog_consumer_state(id).unwrap();
            assert_eq!(state.references, 1);
            assert_eq!((state.pending_value, state.published_value), (0, 0));
        }
    }
    for (index, id) in references.digital.iter().copied().enumerate() {
        assert_eq!(id.is_some(), index + 3 < count);
        if let Some(id) = id {
            let state = input.consumer_state(id).unwrap();
            assert_eq!(state.references, 1);
            assert_eq!((state.active_bindings, state.published_value), (0, 0));
        }
    }
}

#[test]
fn current_digital_file_records_and_explicit_startup_analog_source_acquire_in_native_order() {
    struct RetailSource {
        bytes: Vec<u8>,
        calls: Vec<PlayerInputDescriptorRequest>,
    }
    impl PlayerInputDescriptorSource for RetailSource {
        type Error = inf_formats::InputTableError;

        fn descriptor(
            &mut self,
            request: PlayerInputDescriptorRequest,
        ) -> Result<PlayerInputDescriptor, Self::Error> {
            self.calls.push(request);
            let executable = inf_formats::FrontendExecutable::parse(&self.bytes)?;
            match request.kind {
                PlayerInputDescriptorKind::Analog => {
                    assert!(inf_formats::parse_analog_action_descriptor(
                        executable,
                        request.identity
                    )
                    .is_err());
                    Ok(supplied_descriptor(request))
                }
                PlayerInputDescriptorKind::Digital => {
                    let record =
                        inf_formats::parse_digital_action_descriptor(executable, request.identity)?;
                    let value = ActionDescriptor {
                        identity: record.identity,
                        callback_address: (record.callback_address != 0)
                            .then_some(record.callback_address),
                        flags: record.flags,
                    };
                    assert_eq!(
                        PlayerInputDescriptor::Digital(value),
                        supplied_descriptor(request)
                    );
                    Ok(PlayerInputDescriptor::Digital(value))
                }
            }
        }
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
    let mut source = RetailSource {
        bytes: std::fs::read(path).expect("retail Infest.exe is required"),
        calls: Vec::new(),
    };
    let mut input = InputRuntime::new(7654);
    let context = input.create_context().unwrap();
    let device = input.create_device(DeviceButtons::default()).unwrap();
    input.select_context(Some(context)).unwrap();
    let mut references = PlayerInputReferences::new(CONTEXT);
    references.acquire(&mut input, &mut source).unwrap();
    assert_eq!(source.calls, expected_requests());
    assert_eq!(references.callback_context(), CONTEXT);
    assert_prefix(&input, &references, 11);
    assert_eq!(input.selected_context(), Some(context));
    assert_eq!(
        input.device_buttons(device).unwrap(),
        DeviceButtons::default()
    );
    assert_eq!(input.clock_micros(), 7654);
    assert_eq!(input.queued_len(), 0);
    references.release(&mut input).unwrap();
    assert_prefix(&input, &references, 0);
    assert_eq!(input.selected_context(), Some(context));
    input.destroy_context(context).unwrap();
}

#[test]
fn source_family_and_identity_errors_are_lazy_at_every_slot_and_keep_each_actual_prefix() {
    let requests = expected_requests();
    for failed in 0..11 {
        for fault in [Fault::Source, Fault::Family, Fault::Identity] {
            let mut input = InputRuntime::new(123);
            let mut source = Source {
                fault: Some((failed, fault)),
                ..Source::default()
            };
            let mut references = PlayerInputReferences::new(CONTEXT);
            let error = references.acquire(&mut input, &mut source).unwrap_err();
            match fault {
                Fault::Source => assert_eq!(
                    error,
                    PlayerInputError::Source {
                        request: requests[failed],
                        error: "current descriptor unavailable",
                    }
                ),
                Fault::Family => assert!(matches!(
                    error,
                    PlayerInputError::Family { request, actual }
                        if request == requests[failed] && actual != request.kind
                )),
                Fault::Identity => assert_eq!(
                    error,
                    PlayerInputError::Identity {
                        request: requests[failed],
                        actual: requests[failed].identity ^ 4,
                    }
                ),
                Fault::Period(_) => unreachable!(),
            }
            assert_eq!(source.calls, requests[..=failed]);
            assert_prefix(&input, &references, failed);
            assert_eq!(input.clock_micros(), 123);
            assert_eq!(input.queued_len(), 0);
            assert_eq!(
                references.acquire(&mut input, &mut source),
                Err(PlayerInputError::AlreadyStarted)
            );
            assert_eq!(source.calls, requests[..=failed]);
            references.release(&mut input).unwrap();
            assert_prefix(&input, &references, 0);
            references.release(&mut input).unwrap();
        }
    }
}

#[test]
fn timed_current_analog_metadata_stops_only_at_its_reached_slot() {
    for failed in 0..3 {
        for period in [1, -1, i32::MIN] {
            let mut input = InputRuntime::default();
            let mut source = Source {
                fault: Some((failed, Fault::Period(period))),
                ..Source::default()
            };
            let mut references = PlayerInputReferences::new(CONTEXT);
            assert_eq!(
                references.acquire(&mut input, &mut source),
                Err(PlayerInputError::Input {
                    request: expected_requests()[failed],
                    error: InputStateError::UnsupportedAnalogTimer(period),
                })
            );
            assert_eq!(source.calls.len(), failed + 1);
            assert_prefix(&input, &references, failed);
            references.release(&mut input).unwrap();
        }
    }
}

#[test]
fn existing_conflicting_metadata_retains_its_owner_and_every_earlier_acquisition() {
    for (failed, request) in expected_requests().into_iter().enumerate() {
        let mut input = InputRuntime::default();
        let mut references = PlayerInputReferences::new(CONTEXT);
        let mut source = Source::default();
        let mut conflict = supplied_descriptor(request);
        match &mut conflict {
            PlayerInputDescriptor::Analog(value) => value.action.flags ^= 0x80,
            PlayerInputDescriptor::Digital(value) => value.flags ^= 0x80,
        }
        let analog = match conflict {
            PlayerInputDescriptor::Analog(value) => {
                Some(input.acquire_analog_consumer(value, CONTEXT + 1).unwrap())
            }
            _ => None,
        };
        let digital = match conflict {
            PlayerInputDescriptor::Digital(value) => {
                Some(input.acquire_consumer(value, CONTEXT + 1).unwrap())
            }
            _ => None,
        };
        assert_eq!(
            references.acquire(&mut input, &mut source),
            Err(PlayerInputError::Input {
                request,
                error: InputStateError::DescriptorConflict,
            })
        );
        assert_prefix(&input, &references, failed);
        references.release(&mut input).unwrap();
        if let Some(id) = analog {
            assert_eq!(input.analog_consumer_state(id).unwrap().references, 1);
            input.release_analog_consumer(id).unwrap();
        }
        if let Some(id) = digital {
            assert_eq!(input.consumer_state(id).unwrap().references, 1);
            input.release_consumer(id).unwrap();
        }
    }
}

#[test]
fn same_context_shares_real_references_without_resetting_state_and_other_context_is_distinct() {
    let mut input = InputRuntime::new(123);
    let mut first = PlayerInputReferences::new(CONTEXT);
    first.acquire(&mut input, &mut Source::default()).unwrap();
    let id = first.digital[7].unwrap();
    let mut host = Host::default();
    input
        .emit(id, 1, &mut EventPacket::default(), &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    let mut expected = input.consumer_state(id).unwrap();
    expected.references += 1;
    let mut shared = PlayerInputReferences::new(CONTEXT);
    shared.acquire(&mut input, &mut Source::default()).unwrap();
    assert_eq!(first.analog, shared.analog);
    assert_eq!(first.digital, shared.digital);
    assert_eq!(input.consumer_state(id).unwrap(), expected);
    for id in first.analog.into_iter().flatten() {
        assert_eq!(input.analog_consumer_state(id).unwrap().references, 2);
    }
    let mut distinct = PlayerInputReferences::new(CONTEXT + 1);
    distinct
        .acquire(&mut input, &mut Source::default())
        .unwrap();
    for (left, right) in first.analog.into_iter().zip(distinct.analog) {
        assert_ne!(left, right);
    }
    for (left, right) in first.digital.into_iter().zip(distinct.digital) {
        assert_ne!(left, right);
    }
    first.release(&mut input).unwrap();
    expected.references -= 1;
    assert_eq!(input.consumer_state(id).unwrap(), expected);
    shared.release(&mut input).unwrap();
    assert_eq!(
        input.consumer_state(id),
        Err(InputStateError::InvalidConsumer)
    );
    assert_prefix(&input, &distinct, 11);
    distinct.release(&mut input).unwrap();
}

#[test]
fn cleanup_is_reverse_and_clears_only_successfully_released_slots() {
    for failed in 0..11 {
        let mut input = InputRuntime::default();
        let mut references = PlayerInputReferences::new(CONTEXT);
        references
            .acquire(&mut input, &mut Source::default())
            .unwrap();
        let analog = references.analog;
        let digital = references.digital;
        let expected_error = if failed < 3 {
            input
                .release_analog_consumer(analog[failed].unwrap())
                .unwrap();
            InputStateError::InvalidAnalogConsumer
        } else {
            input
                .release_consumer(digital[failed - 3].unwrap())
                .unwrap();
            InputStateError::InvalidConsumer
        };
        let error = PlayerInputReleaseError {
            request: expected_requests()[failed],
            error: expected_error,
        };
        assert_eq!(references.release(&mut input), Err(error));
        for index in 0..3 {
            assert_eq!(
                references.analog[index],
                (index <= failed).then_some(analog[index]).flatten()
            );
            if index < failed {
                assert_eq!(
                    input
                        .analog_consumer_state(analog[index].unwrap())
                        .unwrap()
                        .references,
                    1
                );
            }
        }
        for index in 0..8 {
            assert_eq!(
                references.digital[index],
                (index + 3 <= failed).then_some(digital[index]).flatten()
            );
            if index + 3 < failed {
                assert_eq!(
                    input
                        .consumer_state(digital[index].unwrap())
                        .unwrap()
                        .references,
                    1
                );
            }
        }
        assert_eq!(references.release(&mut input), Err(error));
    }
}

#[test]
fn final_digital_or_analog_release_clears_real_queued_targets_but_keeps_callback_snapshots() {
    for retain_digitals in [false, true] {
        let mut input = InputRuntime::default();
        let mut source = Source {
            analog_callback: true,
            ..Source::default()
        };
        let mut references = PlayerInputReferences::new(CONTEXT);
        references.acquire(&mut input, &mut source).unwrap();
        let analog = references.analog[0].unwrap();
        let digital = references.digital[7].unwrap();
        let retained: Vec<_> = if retain_digitals {
            expected_requests()
                .into_iter()
                .skip(3)
                .map(|request| {
                    let PlayerInputDescriptor::Digital(value) = supplied_descriptor(request) else {
                        unreachable!()
                    };
                    input.acquire_consumer(value, CONTEXT).unwrap()
                })
                .collect()
        } else {
            Vec::new()
        };
        let other = input
            .acquire_consumer(
                ActionDescriptor {
                    identity: 0x2222,
                    callback_address: Some(0x2222),
                    flags: 6,
                },
                99,
            )
            .unwrap();
        let context = input.create_context().unwrap();
        let device = input.create_device(DeviceButtons::default()).unwrap();
        let set = input
            .create_analog_binding_set(
                device,
                AnalogPolicy::Replace,
                &[AnalogBinding {
                    consumer: analog,
                    axis: 0,
                    transform: AxisTransform::NATIVE_DEFAULT,
                }],
            )
            .unwrap();
        input.attach_analog_binding_set(context, set).unwrap();
        input.select_context(Some(context)).unwrap();
        let mut host = Host::default();
        input
            .update_device(
                device,
                DeviceSnapshot {
                    axes: [25, 0, 0, 0, 0, 0, 0, 0],
                    ..DeviceSnapshot::default()
                },
                &mut host,
            )
            .unwrap();
        input
            .emit(digital, 1, &mut EventPacket::default(), &mut host)
            .unwrap();
        input
            .emit(other, 1, &mut EventPacket::default(), &mut host)
            .unwrap();
        assert_eq!(input.queued_len(), 3);
        references.release(&mut input).unwrap();
        assert_eq!(input.queued_len(), 3);
        input.poll(&mut host).unwrap();
        assert_eq!(
            host.callbacks
                .iter()
                .map(|event| event.address)
                .collect::<Vec<_>>(),
            [0x1111, 0x0046_94d0, 0x2222]
        );
        assert_eq!(
            host.callbacks
                .iter()
                .map(|event| (event.context, event.value))
                .collect::<Vec<_>>(),
            [(CONTEXT, 25), (CONTEXT, 1), (99, 1)]
        );
        assert_eq!(input.analog_value(analog).unwrap(), 0);
        assert!(!input.held(other).unwrap());
        if retain_digitals {
            assert!(!input.held(digital).unwrap());
        }
        for id in retained {
            input.release_consumer(id).unwrap();
        }
        input.release_consumer(other).unwrap();
        input.destroy_analog_binding_set(set).unwrap();
        input.destroy_context(context).unwrap();
    }
}
