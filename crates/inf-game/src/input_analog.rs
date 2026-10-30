//! Untimed analog ownership at 004EFE90, 004F0A40, and 004F0C00.
//! Frontend descriptors have period zero. A nonzero period requires the native
//! timer lifecycle and is rejected before acquiring a reference.

use super::{
    ActionDescriptor, DeviceId, EventPacket, EventSubmission, Handle, InputContextId, InputError,
    InputHost, InputRuntime, InputStateError, StateTarget,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalogConsumerId(Handle);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalogBindingSetId(Handle);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalogActionDescriptor {
    pub action: ActionDescriptor,
    pub period_micros: i32,
}

/// The add and integer multiply wrap BEFORE the signed wide Q31 multiply.
/// Both device calibration and each binding use this exact operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisTransform {
    pub offset: i32,
    pub q31_factor: i32,
    pub integer_factor: i32,
}

impl AxisTransform {
    pub const NATIVE_DEFAULT: Self = Self {
        offset: 0,
        q31_factor: 0x4000_0000,
        integer_factor: 2,
    };

    pub fn apply(self, value: i32) -> i32 {
        let intermediate = value
            .wrapping_add(self.offset)
            .wrapping_mul(self.integer_factor);
        ((i64::from(intermediate) * i64::from(self.q31_factor)) >> 31) as i32
    }
}

/// 004F0C00 tests the FIRST record's flags bit 0 for the entire set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalogPolicy {
    Replace,
    AccumulateChanges,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalogBinding {
    pub consumer: AnalogConsumerId,
    pub axis: u32,
    pub transform: AxisTransform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalogConsumerState {
    pub references: u32,
    pub pending_value: i32,
    pub published_value: i32,
}

pub(super) struct Consumer {
    descriptor: AnalogActionDescriptor,
    context: u32,
    external_references: u32,
    pub(super) state: AnalogConsumerState,
}

struct Cell {
    binding: AnalogBinding,
    previous: i32,
}

pub(super) struct BindingSet {
    device: DeviceId,
    policy: AnalogPolicy,
    cells: Vec<Cell>,
}

impl InputRuntime {
    pub(super) fn analog_consumer(
        &self,
        id: AnalogConsumerId,
    ) -> Result<&Consumer, InputStateError> {
        self.analog_consumers
            .get(id.0)
            .ok_or(InputStateError::InvalidAnalogConsumer)
    }

    pub(super) fn analog_consumer_mut(
        &mut self,
        id: AnalogConsumerId,
    ) -> Result<&mut Consumer, InputStateError> {
        self.analog_consumers
            .get_mut(id.0)
            .ok_or(InputStateError::InvalidAnalogConsumer)
    }

    /// 004EFE90, behind the handle wrapper 004F00A0.
    pub fn acquire_analog_consumer(
        &mut self,
        descriptor: AnalogActionDescriptor,
        context: u32,
    ) -> Result<AnalogConsumerId, InputStateError> {
        self.ensure_running()?;
        if descriptor.period_micros != 0 {
            return Err(InputStateError::UnsupportedAnalogTimer(
                descriptor.period_micros,
            ));
        }
        for &id in &self.analog_order {
            let consumer = self.analog_consumer(id)?;
            if consumer.descriptor.action.identity == descriptor.action.identity {
                if consumer.descriptor != descriptor {
                    return Err(InputStateError::DescriptorConflict);
                }
                if consumer.context == context {
                    self.retain_analog_consumer(id)?;
                    self.analog_consumer_mut(id)?.external_references += 1;
                    return Ok(id);
                }
            }
        }
        let id = AnalogConsumerId(self.analog_consumers.insert(Consumer {
            descriptor,
            context,
            external_references: 1,
            state: AnalogConsumerState {
                references: 1,
                pending_value: 0,
                published_value: 0,
            },
        }));
        self.analog_order.push(id);
        Ok(id)
    }

    fn retain_analog_consumer(&mut self, id: AnalogConsumerId) -> Result<(), InputStateError> {
        let state = &mut self.analog_consumer_mut(id)?.state;
        state.references = state
            .references
            .checked_add(1)
            .ok_or(InputStateError::ReferenceOverflow)?;
        Ok(())
    }

    pub fn release_analog_consumer(&mut self, id: AnalogConsumerId) -> Result<(), InputStateError> {
        let consumer = self.analog_consumer_mut(id)?;
        if consumer.external_references == 0 {
            return Err(InputStateError::NoConsumerReference);
        }
        consumer.external_references -= 1;
        self.release_analog_reference(id)
    }

    fn release_analog_reference(&mut self, id: AnalogConsumerId) -> Result<(), InputStateError> {
        let consumer = self.analog_consumer_mut(id)?;
        consumer.state.references -= 1;
        if consumer.state.references == 0 {
            // 004F00C0 clears every queued target, including digital targets.
            self.invalidate_queued_targets();
            self.analog_consumers.remove(id.0);
            self.analog_order.retain(|candidate| *candidate != id);
        }
        Ok(())
    }

    pub fn analog_consumer_state(
        &self,
        id: AnalogConsumerId,
    ) -> Result<AnalogConsumerState, InputStateError> {
        Ok(self.analog_consumer(id)?.state)
    }

    /// 004F0140 reads the published word, not a value still waiting in the FIFO.
    pub fn analog_value(&self, id: AnalogConsumerId) -> Result<i32, InputStateError> {
        Ok(self.analog_consumer(id)?.state.published_value)
    }

    pub fn create_analog_binding_set(
        &mut self,
        device: DeviceId,
        policy: AnalogPolicy,
        records: &[AnalogBinding],
    ) -> Result<AnalogBindingSetId, InputStateError> {
        self.binding_mutation_allowed()?;
        self.device_buttons(device)?;
        for record in records {
            if record.axis >= 8 {
                return Err(InputStateError::InvalidAxis(record.axis));
            }
            self.analog_consumer(record.consumer)?;
        }
        let mut retained = Vec::new();
        for record in records {
            if let Err(error) = self.retain_analog_consumer(record.consumer) {
                for id in retained {
                    self.release_analog_reference(id)?;
                }
                return Err(error);
            }
            retained.push(record.consumer);
        }
        Ok(AnalogBindingSetId(
            self.analog_sets.insert(BindingSet {
                device,
                policy,
                cells: records
                    .iter()
                    .map(|binding| Cell {
                        binding: *binding,
                        previous: 0,
                    })
                    .collect(),
            }),
        ))
    }

    /// 004F0EA0 appends a non-owning link to the context's analog list
    /// (+3C). Context links do not acquire additional consumer references.
    pub fn attach_analog_binding_set(
        &mut self,
        context: InputContextId,
        set: AnalogBindingSetId,
    ) -> Result<(), InputStateError> {
        self.binding_mutation_allowed()?;
        if self.analog_sets.get(set.0).is_none() {
            return Err(InputStateError::InvalidAnalogBindingSet);
        }
        self.contexts
            .get_mut(context.0)
            .ok_or(InputStateError::InvalidContext)?
            .analog_sets
            .push(set);
        Ok(())
    }

    /// 004F0AD0 removes every context's link to the set, then releases each
    /// cell's consumer reference; a final release voids the queued events
    /// that target that consumer.
    pub fn destroy_analog_binding_set(
        &mut self,
        id: AnalogBindingSetId,
    ) -> Result<(), InputStateError> {
        self.binding_mutation_allowed()?;
        if self.analog_sets.get(id.0).is_none() {
            return Err(InputStateError::InvalidAnalogBindingSet);
        }
        for slot in &mut self.contexts.slots {
            if let Some(context) = &mut slot.value {
                context.analog_sets.retain(|set| *set != id);
            }
        }
        let set = self.analog_sets.remove(id.0).expect("validated analog set");
        for cell in set.cells {
            self.release_analog_reference(cell.binding.consumer)?;
        }
        Ok(())
    }

    /// 004F0F50 permits calibration changes independently of binding ownership.
    pub fn set_axis_transform(
        &mut self,
        device: DeviceId,
        axis: u32,
        transform: AxisTransform,
    ) -> Result<(), InputStateError> {
        self.ensure_running()?;
        if axis >= 8 {
            return Err(InputStateError::InvalidAxis(axis));
        }
        self.devices
            .get_mut(device.0)
            .ok_or(InputStateError::InvalidDevice)?
            .axes[axis as usize] = transform;
        Ok(())
    }

    pub(super) fn update_analog_bindings<H: InputHost>(
        &mut self,
        context: InputContextId,
        device: DeviceId,
        raw_axes: [i32; 8],
        packet: &mut EventPacket,
        host: &mut H,
    ) -> Result<(), InputError<H::Error>> {
        // 004F0F80 calibrates all axes AFTER completing digital callbacks, and
        // BEFORE walking any analog set. Changes during an analog callback only
        // affect the next physical notification.
        let transforms = self
            .devices
            .get(device.0)
            .ok_or(InputStateError::InvalidDevice)?
            .axes;
        let axes: [i32; 8] = std::array::from_fn(|i| transforms[i].apply(raw_axes[i]));
        let sets = self
            .contexts
            .get(context.0)
            .ok_or(InputStateError::InvalidContext)?
            .analog_sets
            .clone();
        for id in sets {
            let set = self.analog_sets.get(id.0).expect("linked analog set");
            if set.device != device {
                continue;
            }
            let count = set.cells.len();
            for index in 0..count {
                let set = self.analog_sets.get_mut(id.0).expect("linked analog set");
                let cell = &mut set.cells[index];
                let value = cell
                    .binding
                    .transform
                    .apply(axes[cell.binding.axis as usize]);
                if value == cell.previous {
                    continue;
                }
                let change = value.wrapping_sub(cell.previous);
                cell.previous = value;
                let consumer_id = cell.binding.consumer;
                let policy = set.policy;
                let consumer = self.analog_consumer_mut(consumer_id)?;
                consumer.state.pending_value = match policy {
                    AnalogPolicy::Replace => value,
                    AnalogPolicy::AccumulateChanges => {
                        consumer.state.pending_value.wrapping_add(change)
                    }
                };
                let submission = EventSubmission {
                    target: StateTarget::Analog(consumer_id),
                    descriptor: consumer.descriptor.action,
                    context: consumer.context,
                    value: consumer.state.pending_value as u32,
                    duration: None,
                };
                self.submit_event(submission, packet, host)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "input_analog_tests.rs"]
mod tests;
