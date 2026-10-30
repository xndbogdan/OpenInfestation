//! Input ownership and event dispatch at 004EFD40..004F0F80.
//!
//! This accepts logical device snapshots; it does not map platform key codes or
//! implement platform events or timed analog callbacks. Retail callback addresses
//! are provenance for a required host dispatcher, never executable pointers.

use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

#[path = "input_bindings.rs"]
mod bindings;
pub use bindings::{
    BindingSetId, DeviceButtons, DeviceId, DeviceSnapshot, DigitalBinding, InputContextId,
};
#[path = "input_analog.rs"]
mod analog;
pub use analog::{
    AnalogActionDescriptor, AnalogBinding, AnalogBindingSetId, AnalogConsumerId,
    AnalogConsumerState, AnalogPolicy, AxisTransform,
};

pub const INPUT_RING_SLOTS: usize = 32;
pub const INPUT_QUEUE_CAPACITY: usize = INPUT_RING_SLOTS - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Handle {
    arena: u64,
    slot: usize,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumerId(Handle);

struct Slot<T> {
    generation: u64,
    value: Option<T>,
}

struct Arena<T> {
    identity: u64,
    slots: Vec<Slot<T>>,
}

static NEXT_ARENA_ID: AtomicU64 = AtomicU64::new(1);

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self {
            identity: NEXT_ARENA_ID
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("input arena identity exhausted"),
            slots: Vec::new(),
        }
    }
}

impl<T> Arena<T> {
    fn insert(&mut self, value: T) -> Handle {
        for (slot, entry) in self.slots.iter_mut().enumerate() {
            if entry.value.is_none() && entry.generation != u64::MAX {
                entry.generation += 1;
                entry.value = Some(value);
                return Handle {
                    arena: self.identity,
                    slot,
                    generation: entry.generation,
                };
            }
        }
        let slot = self.slots.len();
        self.slots.push(Slot {
            generation: 0,
            value: Some(value),
        });
        Handle {
            arena: self.identity,
            slot,
            generation: 0,
        }
    }

    fn get(&self, handle: Handle) -> Option<&T> {
        if handle.arena != self.identity {
            return None;
        }
        let slot = self.slots.get(handle.slot)?;
        (slot.generation == handle.generation)
            .then_some(slot.value.as_ref())
            .flatten()
    }

    fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        if handle.arena != self.identity {
            return None;
        }
        let slot = self.slots.get_mut(handle.slot)?;
        (slot.generation == handle.generation)
            .then_some(slot.value.as_mut())
            .flatten()
    }

    fn remove(&mut self, handle: Handle) -> Option<T> {
        if handle.arena != self.identity {
            return None;
        }
        let slot = self.slots.get_mut(handle.slot)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.value.take()
    }
}

/// Descriptor identity is independent of its callback address: descriptors
/// with the same callback remain distinct registrations in the retail lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionDescriptor {
    pub identity: u32,
    pub callback_address: Option<u32>,
    /// Native bits: 0x01 immediate, 0x02 sample timestamp, 0x04 held duration.
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputCallback {
    pub address: u32,
    /// Native ECX. This is an opaque logical context, not a dereferenced pointer.
    pub context: u32,
    /// Native EDX. Digital writers supply zero/one; analog writers preserve
    /// the signed axis value's raw word.
    pub value: u32,
    /// Native two stack words, low word first.
    pub timestamp_micros: u64,
}

pub trait InputHost {
    type Error;

    /// Supply the bit pattern of native 004A4890's microsecond clock.
    fn clock_micros(&mut self) -> Result<u64, Self::Error>;
    fn callback(
        &mut self,
        event: InputCallback,
        input: &mut InputRuntime,
    ) -> Result<(), Self::Error>;
}

/// One packet is shared by all events from one device notification. Flags 0x02
/// sample only once; an earlier event without that bit receives timestamp zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventPacket {
    sampled: bool,
    timestamp_micros: u64,
}

impl EventPacket {
    pub fn timestamp_micros(&self) -> u64 {
        self.timestamp_micros
    }
    pub fn sampled(&self) -> bool {
        self.sampled
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventDisposition {
    Queued,
    Immediate,
    DroppedFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum InputStateError {
    #[error("invalid or released input consumer")]
    InvalidConsumer,
    #[error("descriptor identity was reused with different callback or flags")]
    DescriptorConflict,
    #[error("input consumer reference count overflow")]
    ReferenceOverflow,
    #[error("no externally owned reference remains for this consumer")]
    NoConsumerReference,
    #[error("invalid or released input context")]
    InvalidContext,
    #[error("invalid input device")]
    InvalidDevice,
    #[error("invalid or released binding set")]
    InvalidBindingSet,
    #[error("invalid or released analog input consumer")]
    InvalidAnalogConsumer,
    #[error("invalid or released analog binding set")]
    InvalidAnalogBindingSet,
    #[error("analog axis {0} is outside the eight native device axes")]
    InvalidAxis(u32),
    #[error("analog timer period {0} microseconds requires an unimplemented timer owner")]
    UnsupportedAnalogTimer(i32),
    #[error("recursive input polling is unsupported")]
    RecursivePoll,
    #[error(
        "recursive device notification or binding mutation during notification is unsupported"
    )]
    DeviceNotificationInProgress,
    #[error("input stopped after a host failure; completed callbacks cannot be replayed")]
    Stopped,
}

#[derive(Debug, Error)]
pub enum InputError<E> {
    #[error(transparent)]
    State(#[from] InputStateError),
    #[error("input host failed: {0}")]
    Host(E),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumerState {
    pub references: u32,
    pub active_bindings: u32,
    pub published_value: u32,
    pub baseline_micros: u64,
    pub held_micros: u64,
}

struct Consumer {
    descriptor: ActionDescriptor,
    context: u32,
    external_references: u32,
    state: ConsumerState,
}

#[derive(Clone, Copy)]
struct QueuedEvent {
    target: Option<StateTarget>,
    duration: Option<ConsumerId>,
    callback: Option<InputCallback>,
    value: u32,
    timestamp_micros: u64,
}

#[derive(Clone, Copy)]
enum StateTarget {
    Digital(ConsumerId),
    Analog(AnalogConsumerId),
}

struct EventSubmission {
    target: StateTarget,
    descriptor: ActionDescriptor,
    context: u32,
    value: u32,
    duration: Option<ConsumerId>,
}

/// Owns native logical consumers, ordered digital/analog bindings, and the 31-event
/// FIFO. Callback failure stops dispatch; it does not undo published state or
/// callbacks. Unused ring slots retain their payload after consumption, matching
/// the cached-cursor behavior when a callback calls reset/discard.
pub struct InputRuntime {
    consumers: Arena<Consumer>,
    consumer_order: Vec<ConsumerId>,
    contexts: Arena<bindings::Context>,
    binding_sets: Arena<bindings::BindingSet>,
    analog_consumers: Arena<analog::Consumer>,
    analog_order: Vec<AnalogConsumerId>,
    analog_sets: Arena<analog::BindingSet>,
    devices: Arena<bindings::Device>,
    selected_context: Option<InputContextId>,
    events: [Option<QueuedEvent>; INPUT_RING_SLOTS],
    read: usize,
    write: usize,
    clock_micros: u64,
    polling: bool,
    notifying_device: bool,
    stopped: bool,
    /// Port fix (exact world ticks): the world skipped this frame, so the
    /// next poll keeps the held durations for the tick that consumes them.
    carry_durations: bool,
    /// Port fix (exact world ticks): a world tick's delta over the real time
    /// its held durations span, which scales them until the next poll.
    duration_scale: Option<(u32, u32)>,
}

impl Default for InputRuntime {
    fn default() -> Self {
        Self::new(0)
    }
}

impl InputRuntime {
    pub fn new(previous_poll_clock_micros: u64) -> Self {
        Self {
            consumers: Arena::default(),
            // The consumer lists 0065EC68 and 0065EC80 start empty (the CRT's
            // 004EFD10 and 004EFDD0).
            consumer_order: Vec::new(),
            // The context list 0065E750 starts empty (004EFCA0).
            contexts: Arena::default(),
            binding_sets: Arena::default(),
            analog_consumers: Arena::default(),
            analog_order: Vec::new(),
            analog_sets: Arena::default(),
            // The device list 0065EDF8 starts empty (004F5050).
            devices: Arena::default(),
            // 004EFCD0 starts the ring's two positions and the selected
            // context at zero.
            selected_context: None,
            events: [None; INPUT_RING_SLOTS],
            read: 0,
            write: 0,
            clock_micros: previous_poll_clock_micros,
            polling: false,
            notifying_device: false,
            stopped: false,
            carry_durations: false,
            duration_scale: None,
        }
    }

    /// Port fix for the exact world ticks, which run the world only on
    /// frames that complete a whole quantum. Retail's poll `004F0200`
    /// resets every held duration, and the vehicle reads them as the time a
    /// key was held during its tick (`0046A260`, capped at the delta).
    /// Without this, a frame whose world update was skipped would drop its
    /// held time: at 120 frames a second Thrust would read half the delta.
    /// The next poll keeps the durations instead of resetting them.
    pub fn carry_durations(&mut self) {
        self.carry_durations = true;
    }

    /// Port fix for the exact world ticks: until the next poll, held
    /// durations read as `held * world_delta / elapsed`, where `elapsed` is
    /// the real time since the previous world tick. A key held throughout
    /// then reads the tick's whole delta, as in retail where both are the
    /// frame's time.
    pub fn scale_durations(&mut self, world_delta: u32, elapsed: u32) {
        self.duration_scale =
            (elapsed != 0 && world_delta != elapsed).then_some((world_delta, elapsed));
    }

    pub fn queued_len(&self) -> usize {
        (self.write + INPUT_RING_SLOTS - self.read) % INPUT_RING_SLOTS
    }
    pub fn clock_micros(&self) -> u64 {
        self.clock_micros
    }
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    fn ensure_running(&self) -> Result<(), InputStateError> {
        if self.stopped {
            Err(InputStateError::Stopped)
        } else {
            Ok(())
        }
    }

    fn consumer(&self, id: ConsumerId) -> Result<&Consumer, InputStateError> {
        self.consumers
            .get(id.0)
            .ok_or(InputStateError::InvalidConsumer)
    }

    fn consumer_mut(&mut self, id: ConsumerId) -> Result<&mut Consumer, InputStateError> {
        self.consumers
            .get_mut(id.0)
            .ok_or(InputStateError::InvalidConsumer)
    }

    /// 004EFD40 (behind the handle wrapper 004EFFA0) shares an existing
    /// registration by descriptor identity AND callback context. New
    /// registrations append to the consumer list.
    pub fn acquire_consumer(
        &mut self,
        descriptor: ActionDescriptor,
        context: u32,
    ) -> Result<ConsumerId, InputStateError> {
        self.ensure_running()?;
        for &id in &self.consumer_order {
            let consumer = self.consumer(id)?;
            if consumer.descriptor.identity == descriptor.identity {
                if consumer.descriptor != descriptor {
                    return Err(InputStateError::DescriptorConflict);
                }
                if consumer.context == context {
                    self.retain_consumer(id)?;
                    self.consumer_mut(id)?.external_references += 1;
                    return Ok(id);
                }
            }
        }
        let id = ConsumerId(self.consumers.insert(Consumer {
            descriptor,
            context,
            external_references: 1,
            state: ConsumerState {
                references: 1,
                active_bindings: 0,
                published_value: 0,
                // Retail leaves these uninitialized until poll/reset. The
                // portable owner initializes them to its existing time base.
                baseline_micros: self.clock_micros,
                held_micros: 0,
            },
        }));
        self.consumer_order.push(id);
        Ok(id)
    }

    fn retain_consumer(&mut self, id: ConsumerId) -> Result<(), InputStateError> {
        let consumer = self.consumer_mut(id)?;
        consumer.state.references = consumer
            .state
            .references
            .checked_add(1)
            .ok_or(InputStateError::ReferenceOverflow)?;
        Ok(())
    }

    /// Final release 004EFFC0 preserves retail's unusual bulk invalidation: ALL queued
    /// state targets are cleared, while their callback snapshots remain queued.
    /// See literal MOV instructions at 004EFFE7 and 004EFFF0, not a filtered erase.
    pub fn release_consumer(&mut self, id: ConsumerId) -> Result<(), InputStateError> {
        let consumer = self.consumer_mut(id)?;
        if consumer.external_references == 0 {
            return Err(InputStateError::NoConsumerReference);
        }
        consumer.external_references -= 1;
        self.release_reference(id)
    }

    fn release_reference(&mut self, id: ConsumerId) -> Result<(), InputStateError> {
        let consumer = self.consumer_mut(id)?;
        consumer.state.references -= 1;
        if consumer.state.references != 0 {
            return Ok(());
        }
        self.invalidate_queued_targets();
        self.consumers.remove(id.0);
        self.consumer_order.retain(|candidate| *candidate != id);
        Ok(())
    }

    fn invalidate_queued_targets(&mut self) {
        let mut cursor = self.read;
        while cursor != self.write {
            if let Some(event) = &mut self.events[cursor] {
                event.target = None;
            }
            cursor = (cursor + 1) % INPUT_RING_SLOTS;
        }
    }

    pub fn consumer_state(&self, id: ConsumerId) -> Result<ConsumerState, InputStateError> {
        Ok(self.consumer(id)?.state)
    }

    /// 004F0030 observes published state, which can lag a queued physical edge.
    pub fn held(&self, id: ConsumerId) -> Result<bool, InputStateError> {
        Ok(self.consumer(id)?.state.published_value != 0)
    }

    /// 004F0040 accumulates through the last completed poll and returns only
    /// the low native word. Repeated reads in one poll do not double-count.
    pub fn held_micros(&mut self, id: ConsumerId) -> Result<u32, InputStateError> {
        let clock = self.clock_micros;
        let scale = self.duration_scale;
        let state = &mut self.consumer_mut(id)?.state;
        if state.published_value != 0 {
            state.held_micros = state
                .held_micros
                .wrapping_add(clock.wrapping_sub(state.baseline_micros));
            state.baseline_micros = clock;
        }
        Ok(match scale {
            Some((numerator, denominator)) => {
                (state.held_micros * u64::from(numerator) / u64::from(denominator)) as u32
            }
            None => state.held_micros as u32,
        })
    }

    /// 004F0150. Value is kept as a native word; the binding writer emits bools.
    pub fn emit<H: InputHost>(
        &mut self,
        id: ConsumerId,
        value: u32,
        packet: &mut EventPacket,
        host: &mut H,
    ) -> Result<EventDisposition, InputError<H::Error>> {
        self.ensure_running()?;
        let consumer = self.consumer(id)?;
        let descriptor = consumer.descriptor;
        let context = consumer.context;
        self.submit_event(
            EventSubmission {
                target: StateTarget::Digital(id),
                descriptor,
                context,
                value,
                duration: (descriptor.flags & 4 != 0).then_some(id),
            },
            packet,
            host,
        )
    }

    /// Both native binding families call 004F0150. Preserve the old digital
    /// decision matrix: lazy packet clock, immediate publication before callback,
    /// drop-new FIFO, optional digital duration, and cached post-callback cursor.
    /// Analog callers always pass a null duration pointer, even with flag 4.
    fn submit_event<H: InputHost>(
        &mut self,
        submission: EventSubmission,
        packet: &mut EventPacket,
        host: &mut H,
    ) -> Result<EventDisposition, InputError<H::Error>> {
        self.ensure_running()?;
        let EventSubmission {
            target,
            descriptor,
            context,
            value,
            duration,
        } = submission;
        if descriptor.flags & 2 != 0 && !packet.sampled {
            let sampled = host.clock_micros();
            packet.timestamp_micros = self.host_result(sampled)?;
            packet.sampled = true;
        }
        let callback = descriptor.callback_address.map(|address| InputCallback {
            address,
            context,
            value,
            timestamp_micros: packet.timestamp_micros,
        });
        if descriptor.flags & 1 != 0 {
            self.publish_value(target, value)?;
            if let Some(callback) = callback {
                let result = host.callback(callback, self);
                self.host_result(result)?;
            }
            return Ok(EventDisposition::Immediate);
        }
        let next = (self.write + 1) % INPUT_RING_SLOTS;
        if next == self.read {
            return Ok(EventDisposition::DroppedFull);
        }
        self.events[self.write] = Some(QueuedEvent {
            target: Some(target),
            duration,
            callback,
            value,
            timestamp_micros: packet.timestamp_micros,
        });
        self.write = next;
        Ok(EventDisposition::Queued)
    }

    fn published_value(&self, target: StateTarget) -> Result<u32, InputStateError> {
        match target {
            StateTarget::Digital(id) => Ok(self.consumer(id)?.state.published_value),
            StateTarget::Analog(id) => Ok(self.analog_consumer(id)?.state.published_value as u32),
        }
    }

    fn publish_value(&mut self, target: StateTarget, value: u32) -> Result<(), InputStateError> {
        match target {
            StateTarget::Digital(id) => self.consumer_mut(id)?.state.published_value = value,
            StateTarget::Analog(id) => {
                self.analog_consumer_mut(id)?.state.published_value = value as i32
            }
        }
        Ok(())
    }

    fn reset_baselines(&mut self) {
        for &id in &self.consumer_order {
            let consumer = self.consumers.get_mut(id.0).expect("registered consumer");
            consumer.state.baseline_micros = self.clock_micros;
            consumer.state.held_micros = 0;
        }
    }

    /// 004F0340 does not change held state, bindings, or the last poll clock.
    /// During a callback its cursor write is overwritten by poll's cached next
    /// cursor, exactly as at 004F0304; it is not a guaranteed mid-poll cancellation.
    pub fn reset_baselines_and_discard(&mut self) {
        self.reset_baselines();
        self.read = self.write;
    }

    /// 004F0200 resets durations, dispatches the live FIFO, then samples time.
    /// The current slot occupies capacity until its callback has completed.
    pub fn poll<H: InputHost>(&mut self, host: &mut H) -> Result<(), InputError<H::Error>> {
        self.ensure_running()?;
        if self.polling {
            return Err(InputStateError::RecursivePoll.into());
        }
        if self.notifying_device {
            return Err(InputStateError::DeviceNotificationInProgress.into());
        }
        self.polling = true;
        let result = self.poll_events(host);
        self.polling = false;
        result
    }

    fn poll_events<H: InputHost>(&mut self, host: &mut H) -> Result<(), InputError<H::Error>> {
        self.duration_scale = None;
        if !std::mem::take(&mut self.carry_durations) {
            self.reset_baselines();
        }
        while self.read != self.write {
            let next = (self.read + 1) % INPUT_RING_SLOTS;
            let event = self.events[self.read].expect("published ring slot");
            if let Some(id) = event.target {
                let old_value = self.published_value(id)?;
                if let Some(duration) = event.duration {
                    if (old_value != 0) != (event.value != 0) {
                        let state = &mut self.consumer_mut(duration)?.state;
                        if event.value != 0 {
                            state.baseline_micros = event.timestamp_micros;
                        } else {
                            state.held_micros = state.held_micros.wrapping_add(
                                event.timestamp_micros.wrapping_sub(state.baseline_micros),
                            );
                        }
                    }
                }
                self.publish_value(id, event.value)?;
            }
            if let Some(callback) = event.callback {
                let result = host.callback(callback, self);
                self.host_result(result)?;
            }
            self.read = next;
        }
        let sampled = host.clock_micros();
        self.clock_micros = self.host_result(sampled)?;
        Ok(())
    }

    fn host_result<T, E>(&mut self, result: Result<T, E>) -> Result<T, InputError<E>> {
        match result {
            Err(error) => {
                self.stopped = true;
                Err(InputError::Host(error))
            }
            Ok(value) => {
                // A callback can swallow a nested host failure. The portable
                // owner remains stopped; do not resume its outer dispatch.
                self.ensure_running()?;
                Ok(value)
            }
        }
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
