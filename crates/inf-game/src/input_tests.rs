use std::collections::VecDeque;

use super::*;

enum Action {
    Enqueue(ConsumerId, u32),
    Reset,
    Release(ConsumerId),
    Select(Option<InputContextId>),
    RecursivePoll,
    RejectBindingMutation(InputContextId, DeviceId),
    Fail,
    Many(Vec<Action>),
}

#[derive(Debug)]
struct Observation {
    event: InputCallback,
    queued: usize,
    clock: u64,
    states: Vec<Option<ConsumerState>>,
}

#[derive(Default)]
struct Host {
    now: u64,
    samples: Vec<u64>,
    watch: Vec<ConsumerId>,
    observations: Vec<Observation>,
    actions: VecDeque<Action>,
    fail_clock: bool,
}

impl Host {
    fn values(&self) -> Vec<u32> {
        self.observations
            .iter()
            .map(|seen| seen.event.value)
            .collect()
    }

    fn act(&mut self, action: Action, input: &mut InputRuntime) -> Result<(), &'static str> {
        match action {
            Action::Enqueue(id, value) => {
                input
                    .emit(id, value, &mut EventPacket::default(), self)
                    .unwrap();
            }
            Action::Reset => input.reset_baselines_and_discard(),
            Action::Release(id) => input.release_consumer(id).unwrap(),
            Action::Select(context) => input.select_context(context).unwrap(),
            Action::RecursivePoll => assert!(matches!(
                input.poll(self),
                Err(InputError::State(InputStateError::RecursivePoll))
            )),
            Action::RejectBindingMutation(context, device) => {
                assert_eq!(
                    input.select_context(None),
                    Err(InputStateError::DeviceNotificationInProgress)
                );
                assert_eq!(
                    input.destroy_context(context),
                    Err(InputStateError::DeviceNotificationInProgress)
                );
                assert!(matches!(
                    input.update_device(device, snapshot(DeviceButtons::default()), self),
                    Err(InputError::State(
                        InputStateError::DeviceNotificationInProgress
                    ))
                ));
            }
            Action::Fail => return Err("callback failure"),
            Action::Many(actions) => {
                for action in actions {
                    self.act(action, input)?;
                }
            }
        }
        Ok(())
    }
}

impl InputHost for Host {
    type Error = &'static str;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        if self.fail_clock {
            return Err("clock failure");
        }
        self.samples.push(self.now);
        Ok(self.now)
    }

    fn callback(
        &mut self,
        event: InputCallback,
        input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        self.observations.push(Observation {
            event,
            queued: input.queued_len(),
            clock: input.clock_micros(),
            states: self
                .watch
                .iter()
                .map(|id| input.consumer_state(*id).ok())
                .collect(),
        });
        if let Some(action) = self.actions.pop_front() {
            self.act(action, input)?;
        }
        Ok(())
    }
}

fn descriptor(identity: u32, flags: u32) -> ActionDescriptor {
    ActionDescriptor {
        identity,
        callback_address: Some(0x00400000 + identity),
        flags,
    }
}

fn consumer(input: &mut InputRuntime, identity: u32, flags: u32) -> ConsumerId {
    input
        .acquire_consumer(descriptor(identity, flags), 0x1200 + identity)
        .unwrap()
}

fn emit(input: &mut InputRuntime, host: &mut Host, id: ConsumerId, value: u32) -> EventDisposition {
    input
        .emit(id, value, &mut EventPacket::default(), host)
        .unwrap()
}

fn buttons(codes: &[u8]) -> DeviceButtons {
    let mut result = DeviceButtons::default();
    for &code in codes {
        result.set(code, true);
    }
    result
}

fn binding(consumer: ConsumerId, code: u8) -> DigitalBinding {
    DigitalBinding {
        consumer,
        required: [[code, 0, 0, 0], [0; 4]],
        forbidden: [[0; 4]; 2],
    }
}

fn attach(
    input: &mut InputRuntime,
    context: InputContextId,
    sources: [Option<DeviceId>; 2],
    records: &[DigitalBinding],
) -> Result<BindingSetId, InputStateError> {
    let set = input.create_binding_set(sources, records)?;
    input.attach_binding_set(context, set)?;
    Ok(set)
}

#[test]
fn registration_identity_context_and_generation_are_independent_of_callback_address() {
    let mut input = InputRuntime::default();
    let original = consumer(&mut input, 1, 0);
    assert_eq!(consumer(&mut input, 1, 0), original);
    assert_eq!(input.consumer_state(original).unwrap().references, 2);
    let other_context = input.acquire_consumer(descriptor(1, 0), 99).unwrap();
    let other_descriptor = input
        .acquire_consumer(
            ActionDescriptor {
                identity: 2,
                ..descriptor(1, 0)
            },
            0x1201,
        )
        .unwrap();
    assert_ne!(original, other_context);
    assert_ne!(original, other_descriptor);
    assert_eq!(
        input.acquire_consumer(descriptor(1, 2), 99),
        Err(InputStateError::DescriptorConflict)
    );
    input.release_consumer(original).unwrap();
    assert_eq!(input.consumer_state(original).unwrap().references, 1);
    input.release_consumer(original).unwrap();
    let replacement = consumer(&mut input, 1, 0);
    assert_ne!(replacement, original);
    assert_eq!(input.held(original), Err(InputStateError::InvalidConsumer));
    assert_eq!(
        input.release_consumer(original),
        Err(InputStateError::InvalidConsumer)
    );

    let mut foreign = InputRuntime::default();
    let foreign_id = consumer(&mut foreign, 1, 0);
    assert_ne!(foreign_id, replacement);
    assert_eq!(
        input.held(foreign_id),
        Err(InputStateError::InvalidConsumer)
    );
    let foreign_context = foreign.create_context().unwrap();
    assert_eq!(
        input.select_context(Some(foreign_context)),
        Err(InputStateError::InvalidContext)
    );
    let foreign_device = foreign.create_device(DeviceButtons::default()).unwrap();
    assert!(matches!(
        input.update_device(
            foreign_device,
            snapshot(buttons(&[1])),
            &mut Host::default()
        ),
        Err(InputError::State(InputStateError::InvalidDevice))
    ));
    let local_context = input.create_context().unwrap();
    let foreign_set = foreign.create_binding_set([None, None], &[]).unwrap();
    assert_eq!(
        input.attach_binding_set(local_context, foreign_set),
        Err(InputStateError::InvalidBindingSet)
    );
    let stale_set = input.create_binding_set([None, None], &[]).unwrap();
    input.destroy_binding_set(stale_set).unwrap();
    let fresh_set = input.create_binding_set([None, None], &[]).unwrap();
    assert_ne!(stale_set, fresh_set);
    assert_eq!(
        input.attach_binding_set(local_context, stale_set),
        Err(InputStateError::InvalidBindingSet)
    );
}

#[test]
fn ring_wraps_fifo_drops_newest_and_reserves_current_callback_slot() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 0);
    let mut host = Host::default();
    for round in 0..3 {
        for value in 0..31 {
            assert_eq!(
                emit(&mut input, &mut host, id, round * 100 + value),
                EventDisposition::Queued
            );
        }
        assert_eq!(input.queued_len(), 31);
        assert_eq!(
            emit(&mut input, &mut host, id, 999),
            EventDisposition::DroppedFull
        );
        host.actions.push_back(Action::Enqueue(id, 999));
        input.poll(&mut host).unwrap();
        assert_eq!(input.queued_len(), 0);
    }
    assert_eq!(
        host.values(),
        (0..3)
            .flat_map(|round| (0..31).map(move |value| round * 100 + value))
            .collect::<Vec<_>>()
    );
    assert_eq!(host.observations[0].queued, 31);
    assert_eq!(host.observations[30].queued, 1);
}

#[test]
fn event_packet_samples_lazily_once_and_even_a_dropped_event_can_sample() {
    let mut input = InputRuntime::default();
    let untimed = consumer(&mut input, 1, 0);
    let timed = consumer(&mut input, 2, 2);
    let mut host = Host {
        now: 123,
        ..Host::default()
    };
    let mut packet = EventPacket::default();
    input.emit(untimed, 1, &mut packet, &mut host).unwrap();
    assert!(!packet.sampled());
    input.emit(timed, 2, &mut packet, &mut host).unwrap();
    host.now = 456;
    input.emit(untimed, 3, &mut packet, &mut host).unwrap();
    input.emit(timed, 4, &mut packet, &mut host).unwrap();
    assert_eq!(packet.timestamp_micros(), 123);
    assert_eq!(host.samples, [123]);
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.observations
            .iter()
            .map(|seen| seen.event.timestamp_micros)
            .collect::<Vec<_>>(),
        [0, 123, 123, 123]
    );
    assert_eq!(host.samples, [123, 456]);
    for _ in 0..31 {
        emit(&mut input, &mut host, untimed, 1);
    }
    let mut dropped_packet = EventPacket::default();
    host.now = 789;
    assert_eq!(
        input
            .emit(timed, 0, &mut dropped_packet, &mut host)
            .unwrap(),
        EventDisposition::DroppedFull
    );
    assert!(dropped_packet.sampled());
    assert_eq!(dropped_packet.timestamp_micros(), 789);
}

#[test]
fn immediate_dispatch_publishes_before_callback_and_does_not_account_duration() {
    let mut input = InputRuntime::new(10);
    let id = consumer(&mut input, 1, 7);
    let mut host = Host {
        now: 20,
        watch: vec![id],
        ..Host::default()
    };
    assert_eq!(
        emit(&mut input, &mut host, id, 1),
        EventDisposition::Immediate
    );
    assert_eq!(input.queued_len(), 0);
    assert_eq!(host.observations[0].event.context, 0x1201);
    assert_eq!(host.observations[0].event.timestamp_micros, 20);
    let state = host.observations[0].states[0].unwrap();
    assert_eq!(state.published_value, 1);
    assert_eq!(state.baseline_micros, 10);
    assert_eq!(state.held_micros, 0);
    host.now = 30;
    emit(&mut input, &mut host, id, 0);
    assert_eq!(input.consumer_state(id).unwrap().held_micros, 0);
}

#[test]
fn held_duration_tracks_edges_between_polls_and_repeated_reads_do_not_double_count() {
    let mut input = InputRuntime::new(100);
    let id = consumer(&mut input, 1, 6);
    let mut host = Host {
        now: 120,
        watch: vec![id],
        ..Host::default()
    };
    emit(&mut input, &mut host, id, 1);
    assert!(!input.held(id).unwrap());
    host.now = 180;
    emit(&mut input, &mut host, id, 0);
    host.now = 200;
    input.poll(&mut host).unwrap();
    assert_eq!(host.observations[0].clock, 100);
    assert_eq!(host.observations[1].states[0].unwrap().held_micros, 60);
    assert_eq!(input.held_micros(id).unwrap(), 60);
    assert_eq!(input.held_micros(id).unwrap(), 60);

    host.now = 220;
    emit(&mut input, &mut host, id, 1);
    host.now = 250;
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 30);
    assert_eq!(input.held_micros(id).unwrap(), 30);
    host.now = 300;
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 50);
    host.now = 320;
    emit(&mut input, &mut host, id, 0);
    host.now = 330;
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 20);
}

/// Port fix for the exact world ticks: a poll after a skipped world update
/// keeps the held time, and a tick's scale maps the real span onto its
/// delta, until the next poll.
#[test]
fn skipped_ticks_carry_held_time_and_ticks_scale_it_to_their_delta() {
    let mut input = InputRuntime::new(0);
    let id = consumer(&mut input, 1, 6);
    let mut host = Host::default();
    emit(&mut input, &mut host, id, 1);
    host.now = 8333;
    input.poll(&mut host).unwrap();
    // The world skipped this frame without reading the key.
    input.carry_durations();
    host.now = 16_666;
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 16_666);
    // The tick's 16384 µs delta over the 16666 µs both frames spanned.
    input.scale_durations(16_384, 16_666);
    assert_eq!(input.held_micros(id).unwrap(), 16_384);
    assert_eq!(input.held_micros(id).unwrap(), 16_384);
    // The next poll restarts the durations and drops the scale.
    host.now = 25_000;
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 8_334);
    // An equal delta and span, or no span, leave the durations alone.
    input.scale_durations(8_334, 8_334);
    assert_eq!(input.held_micros(id).unwrap(), 8_334);
    input.scale_durations(8_334, 0);
    assert_eq!(input.held_micros(id).unwrap(), 8_334);
}

#[test]
fn held_time_wraps_full_width_but_native_accessor_returns_low_word() {
    let mut input = InputRuntime::new(u64::MAX - 10);
    let id = consumer(&mut input, 1, 0);
    let mut host = Host {
        now: 5,
        ..Host::default()
    };
    emit(&mut input, &mut host, id, 1);
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 16);
    host.now = (1u64 << 32) + 12;
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 7);
    assert_eq!(
        input.consumer_state(id).unwrap().held_micros,
        (1u64 << 32) + 7
    );
}

#[test]
fn reset_discards_outside_poll_without_releasing_held_or_sampling_clock() {
    let mut input = InputRuntime::new(10);
    let id = consumer(&mut input, 1, 0);
    let mut host = Host {
        now: 20,
        ..Host::default()
    };
    emit(&mut input, &mut host, id, 1);
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 10);
    emit(&mut input, &mut host, id, 0);
    input.reset_baselines_and_discard();
    assert_eq!(input.queued_len(), 0);
    assert!(input.held(id).unwrap());
    assert_eq!(input.consumer_state(id).unwrap().held_micros, 0);
    assert_eq!(input.clock_micros(), 20);
    assert_eq!(host.samples, [20]);
}

#[test]
fn live_poll_observes_callback_enqueues_and_overwrites_mid_callback_discard_cursor() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 0);
    let mut host = Host::default();
    emit(&mut input, &mut host, id, 1);
    emit(&mut input, &mut host, id, 2);
    host.actions.push_back(Action::Many(vec![
        Action::RecursivePoll,
        Action::Reset,
        Action::Enqueue(id, 3),
    ]));
    input.poll(&mut host).unwrap();
    assert_eq!(host.values(), [1, 2, 3]);
    assert_eq!(input.consumer_state(id).unwrap().published_value, 3);
    assert!(!input.stopped());
}

#[test]
fn final_release_clears_all_pending_targets_but_retains_callback_snapshots() {
    let mut input = InputRuntime::default();
    let a = consumer(&mut input, 1, 6);
    let b = consumer(&mut input, 2, 6);
    let mut host = Host {
        now: 25,
        watch: vec![a, b],
        ..Host::default()
    };
    emit(&mut input, &mut host, a, 1);
    emit(&mut input, &mut host, b, 1);
    input.release_consumer(a).unwrap();
    let replacement = consumer(&mut input, 1, 6);
    assert_ne!(a, replacement);
    input.poll(&mut host).unwrap();
    assert_eq!(host.values(), [1, 1]);
    assert_eq!(host.observations[0].event.context, 0x1201);
    assert_eq!(host.observations[0].states[0], None);
    assert!(!input.held(b).unwrap());
    assert!(!input.held(replacement).unwrap());
    assert_eq!(input.consumer_state(b).unwrap().held_micros, 0);
}

#[test]
fn callback_release_invalidates_later_targets_without_undoing_current_published_value() {
    let mut input = InputRuntime::default();
    let a = consumer(&mut input, 1, 0);
    let b = consumer(&mut input, 2, 0);
    let mut host = Host {
        watch: vec![a, b],
        ..Host::default()
    };
    emit(&mut input, &mut host, a, 1);
    emit(&mut input, &mut host, b, 1);
    host.actions.push_back(Action::Release(a));
    input.poll(&mut host).unwrap();
    assert_eq!(host.observations[0].states[0].unwrap().published_value, 1);
    assert_eq!(host.values(), [1, 1]);
    assert!(!input.held(b).unwrap());
}

#[test]
fn null_callback_still_publishes_value_and_duration() {
    let mut input = InputRuntime::new(10);
    let id = input
        .acquire_consumer(
            ActionDescriptor {
                callback_address: None,
                ..descriptor(1, 6)
            },
            42,
        )
        .unwrap();
    let mut host = Host {
        now: 20,
        ..Host::default()
    };
    emit(&mut input, &mut host, id, 1);
    host.now = 70;
    emit(&mut input, &mut host, id, 0);
    input.poll(&mut host).unwrap();
    assert_eq!(input.held_micros(id).unwrap(), 50);
    assert!(host.observations.is_empty());
}

#[test]
fn host_failure_preserves_partial_publication_and_stops_retry() {
    let mut input = InputRuntime::new(10);
    let id = consumer(&mut input, 1, 0);
    let mut host = Host::default();
    emit(&mut input, &mut host, id, 1);
    emit(&mut input, &mut host, id, 2);
    host.actions.push_back(Action::Fail);
    assert!(matches!(
        input.poll(&mut host),
        Err(InputError::Host("callback failure"))
    ));
    assert_eq!(input.consumer_state(id).unwrap().published_value, 1);
    assert_eq!(input.queued_len(), 2);
    assert_eq!(input.clock_micros(), 10);
    assert!(input.stopped());
    assert!(matches!(
        input.poll(&mut host),
        Err(InputError::State(InputStateError::Stopped))
    ));
    assert_eq!(host.values(), [1]);

    let mut input = InputRuntime::new(10);
    let mut host = Host {
        fail_clock: true,
        ..Host::default()
    };
    assert!(matches!(
        input.poll(&mut host),
        Err(InputError::Host("clock failure"))
    ));
    assert_eq!(input.clock_micros(), 10);
    assert!(input.stopped());
}

#[test]
fn context_selection_silently_publishes_current_masks_and_null_does_not_flush_events() {
    let mut input = InputRuntime::default();
    let a = consumer(&mut input, 1, 0);
    let b = consumer(&mut input, 2, 0);
    let device = input.create_device(buttons(&[7])).unwrap();
    let context = input.create_context().unwrap();
    attach(
        &mut input,
        context,
        [Some(device), None],
        &[binding(a, 7), binding(b, 8)],
    )
    .unwrap();
    input.select_context(Some(context)).unwrap();
    assert!(input.held(a).unwrap());
    assert!(!input.held(b).unwrap());
    assert_eq!(input.queued_len(), 0);
    let mut host = Host::default();
    input
        .update_device(device, snapshot(buttons(&[7, 8])), &mut host)
        .unwrap();
    assert_eq!(input.queued_len(), 1);
    input.select_context(None).unwrap();
    assert!(!input.held(a).unwrap());
    assert!(!input.held(b).unwrap());
    assert_eq!(input.queued_len(), 1);
    input.poll(&mut host).unwrap();
    assert!(
        input.held(b).unwrap(),
        "queued snapshot remains live after null selection"
    );
    assert_eq!(host.values(), [1]);
}

#[test]
fn aliased_bindings_emit_only_aggregate_edges_and_repeated_snapshots_are_silent() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 2);
    let device = input.create_device(DeviceButtons::default()).unwrap();
    let context = input.create_context().unwrap();
    attach(
        &mut input,
        context,
        [Some(device), None],
        &[binding(id, 7), binding(id, 8)],
    )
    .unwrap();
    input.select_context(Some(context)).unwrap();
    let mut host = Host {
        now: 100,
        ..Host::default()
    };
    for codes in [&[7][..], &[7, 8], &[8], &[8], &[]] {
        input
            .update_device(device, snapshot(buttons(codes)), &mut host)
            .unwrap();
    }
    assert_eq!(input.queued_len(), 2);
    assert_eq!(host.samples, [100, 100]);
    input.poll(&mut host).unwrap();
    assert_eq!(host.values(), [1, 0]);
    assert_eq!(input.consumer_state(id).unwrap().active_bindings, 0);
}

#[test]
fn groups_use_required_forbidden_zero_termination_and_absent_device_is_satisfied() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 0);
    let a = input.create_device(buttons(&[1, 2, 200])).unwrap();
    let b = input.create_device(buttons(&[3])).unwrap();
    let context = input.create_context().unwrap();
    let record = DigitalBinding {
        consumer: id,
        required: [[1, 2, 0, 99], [3, 0, 0, 0]],
        forbidden: [[5, 0, 200, 0], [6, 0, 0, 0]],
    };
    attach(&mut input, context, [Some(a), Some(b)], &[record]).unwrap();
    input.select_context(Some(context)).unwrap();
    assert!(input.held(id).unwrap(), "bytes after zero are not examined");
    let mut host = Host::default();
    input
        .update_device(b, snapshot(buttons(&[3, 6])), &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    assert!(!input.held(id).unwrap());
    let absent = input.create_context().unwrap();
    attach(&mut input, absent, [None, None], &[record]).unwrap();
    input.select_context(Some(absent)).unwrap();
    assert!(input.held(id).unwrap());
}

#[test]
fn source_groups_are_processed_in_order_without_coalescing_alias_handover() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 0);
    let device = input.create_device(buttons(&[1])).unwrap();
    let context = input.create_context().unwrap();
    let a = binding(id, 1);
    let b = DigitalBinding {
        required: [[0; 4], [2, 0, 0, 0]],
        ..binding(id, 0)
    };
    attach(&mut input, context, [Some(device), Some(device)], &[a, b]).unwrap();
    input.select_context(Some(context)).unwrap();
    assert_eq!(input.consumer_state(id).unwrap().active_bindings, 1);
    let mut host = Host::default();
    input
        .update_device(device, snapshot(buttons(&[2])), &mut host)
        .unwrap();
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.values(),
        [0, 1],
        "source A releases before source B acquires"
    );
    assert!(input.held(id).unwrap());
}

#[test]
fn device_packet_and_binding_registration_order_are_preserved() {
    let mut input = InputRuntime::default();
    let a = consumer(&mut input, 1, 0);
    let b = consumer(&mut input, 2, 2);
    let c = consumer(&mut input, 3, 0);
    let device = input.create_device(DeviceButtons::default()).unwrap();
    let context = input.create_context().unwrap();
    attach(
        &mut input,
        context,
        [Some(device), None],
        &[binding(a, 1), binding(b, 1)],
    )
    .unwrap();
    attach(&mut input, context, [Some(device), None], &[binding(c, 1)]).unwrap();
    input.select_context(Some(context)).unwrap();
    let mut host = Host {
        now: 25,
        ..Host::default()
    };
    input
        .update_device(device, snapshot(buttons(&[1])), &mut host)
        .unwrap();
    assert_eq!(host.samples, [25]);
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.observations
            .iter()
            .map(|seen| (seen.event.context, seen.event.timestamp_micros))
            .collect::<Vec<_>>(),
        [(0x1201, 0), (0x1202, 25), (0x1203, 25)]
    );
}

#[test]
fn binding_set_owns_references_independently_of_context_links_and_external_refs() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 0);
    let other = consumer(&mut input, 2, 0);
    let device = input.create_device(DeviceButtons::default()).unwrap();
    let context = input.create_context().unwrap();
    let set = attach(&mut input, context, [Some(device), None], &[binding(id, 1)]).unwrap();
    input.select_context(Some(context)).unwrap();
    let mut host = Host::default();
    input
        .update_device(device, snapshot(buttons(&[1])), &mut host)
        .unwrap();
    emit(&mut input, &mut host, other, 1);
    input.destroy_context(context).unwrap();
    assert_eq!(input.selected_context(), None);
    assert_eq!(input.consumer_state(id).unwrap().references, 2);
    input.poll(&mut host).unwrap();
    assert!(input.held(id).unwrap());
    assert!(
        input.held(other).unwrap(),
        "external ref prevented final-release bulk invalidation"
    );
    let replacement = input.create_context().unwrap();
    assert_ne!(context, replacement);
    assert_eq!(
        input.select_context(Some(context)),
        Err(InputStateError::InvalidContext)
    );
    input.attach_binding_set(replacement, set).unwrap();
    input.release_consumer(id).unwrap();
    assert_eq!(
        input.release_consumer(id),
        Err(InputStateError::NoConsumerReference)
    );
    emit(&mut input, &mut host, other, 0);
    input.destroy_binding_set(set).unwrap();
    input.poll(&mut host).unwrap();
    assert!(
        input.held(other).unwrap(),
        "final binding-owned release invalidates every pending target"
    );
    assert_eq!(
        input.consumer_state(id),
        Err(InputStateError::InvalidConsumer)
    );
    input.destroy_context(replacement).unwrap();
}

#[test]
fn shared_sets_and_duplicate_context_links_revisit_shared_cells_without_recomputing_on_detach() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 0);
    let device = input.create_device(buttons(&[1])).unwrap();
    let first = input.create_context().unwrap();
    let second = input.create_context().unwrap();
    let set = attach(&mut input, first, [Some(device), None], &[binding(id, 1)]).unwrap();
    input.attach_binding_set(first, set).unwrap();
    input.attach_binding_set(second, set).unwrap();
    assert_eq!(
        input.consumer_state(id).unwrap().references,
        2,
        "links own no extra consumer references"
    );
    input.select_context(Some(first)).unwrap();
    assert_eq!(input.consumer_state(id).unwrap().active_bindings, 2);
    let mut host = Host::default();
    input
        .update_device(device, snapshot(DeviceButtons::default()), &mut host)
        .unwrap();
    assert_eq!(
        input.consumer_state(id).unwrap().active_bindings,
        1,
        "duplicate link revisits already changed shared cell"
    );
    assert_eq!(input.queued_len(), 0);
    input.select_context(Some(first)).unwrap();
    assert!(!input.held(id).unwrap());
    input.destroy_context(first).unwrap();
    input
        .update_device(device, snapshot(buttons(&[1])), &mut host)
        .unwrap();
    input.select_context(Some(second)).unwrap();
    assert!(input.held(id).unwrap());
    input.destroy_binding_set(set).unwrap();
    assert_eq!(input.consumer_state(id).unwrap().references, 1);
    assert_eq!(
        input.consumer_state(id).unwrap().active_bindings,
        1,
        "set detach does not subtract stale contribution"
    );
    assert!(input.held(id).unwrap());
    input
        .update_device(device, snapshot(DeviceButtons::default()), &mut host)
        .unwrap();
    assert!(input.held(id).unwrap(), "all matching links were detached");
    input.select_context(Some(second)).unwrap();
    assert!(!input.held(id).unwrap());
    assert_eq!(
        input.attach_binding_set(second, set),
        Err(InputStateError::InvalidBindingSet)
    );
}

#[test]
fn queue_callbacks_can_select_bindings_but_immediate_device_callbacks_reject_structural_mutation() {
    let mut input = InputRuntime::default();
    let id = consumer(&mut input, 1, 0);
    let device = input.create_device(buttons(&[1])).unwrap();
    let context = input.create_context().unwrap();
    attach(&mut input, context, [Some(device), None], &[binding(id, 1)]).unwrap();
    let mut host = Host::default();
    emit(&mut input, &mut host, id, 0);
    host.actions.push_back(Action::Select(Some(context)));
    input.poll(&mut host).unwrap();
    assert!(input.held(id).unwrap());
    assert_eq!(input.selected_context(), Some(context));

    let immediate = consumer(&mut input, 2, 1);
    let immediate_context = input.create_context().unwrap();
    attach(
        &mut input,
        immediate_context,
        [Some(device), None],
        &[binding(immediate, 2)],
    )
    .unwrap();
    input.select_context(Some(immediate_context)).unwrap();
    host.actions
        .push_back(Action::RejectBindingMutation(immediate_context, device));
    input
        .update_device(device, snapshot(buttons(&[2])), &mut host)
        .unwrap();
    assert!(input.held(immediate).unwrap());
    assert_eq!(input.selected_context(), Some(immediate_context));
    input.select_context(None).unwrap();
}

fn snapshot(buttons: DeviceButtons) -> DeviceSnapshot {
    DeviceSnapshot {
        buttons,
        axes: [0; 8],
    }
}
