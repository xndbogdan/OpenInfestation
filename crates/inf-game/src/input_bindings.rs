//! Ordered digital binding sets at 004F0470, 004F06D0, and 004F08A0.

use super::{
    AnalogBindingSetId, AxisTransform, ConsumerId, EventPacket, Handle, InputError, InputHost,
    InputRuntime, InputStateError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputContextId(pub(super) Handle);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceId(pub(super) Handle);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingSetId(Handle);

/// Native 256-bit device button state. Codes are source-device codes; this type
/// deliberately assigns no Windows, SDL, keyboard, or mouse meanings to them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceButtons(pub [u32; 8]);

/// One physical notification: buttons followed by eight signed raw axes.
/// 004F0F80 shares a single timestamp packet across both binding families.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub buttons: DeviceButtons,
    pub axes: [i32; 8],
}

pub(super) struct Device {
    pub(super) buttons: DeviceButtons,
    pub(super) axes: [AxisTransform; 8],
}

impl DeviceButtons {
    pub fn pressed(self, code: u8) -> bool {
        self.0[usize::from(code >> 5)] & (1u32 << (code & 31)) != 0
    }

    pub fn set(&mut self, code: u8, pressed: bool) {
        let word = &mut self.0[usize::from(code >> 5)];
        let bit = 1u32 << (code & 31);
        if pressed {
            *word |= bit;
        } else {
            *word &= !bit;
        }
    }
}

/// One native 20-byte record, with its descriptor/context already resolved to
/// a logical consumer. Each four-code list terminates at its first zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigitalBinding {
    pub consumer: ConsumerId,
    pub required: [[u8; 4]; 2],
    pub forbidden: [[u8; 4]; 2],
}

struct Cell {
    binding: DigitalBinding,
    satisfied: [bool; 2],
}

pub(super) struct BindingSet {
    sources: [Option<DeviceId>; 2],
    cells: Vec<Cell>,
}

#[derive(Default)]
pub(super) struct Context {
    sets: Vec<BindingSetId>,
    pub(super) analog_sets: Vec<AnalogBindingSetId>,
}

fn satisfied(buttons: DeviceButtons, required: [u8; 4], forbidden: [u8; 4]) -> bool {
    required
        .into_iter()
        .take_while(|code| *code != 0)
        .all(|code| buttons.pressed(code))
        && forbidden
            .into_iter()
            .take_while(|code| *code != 0)
            .all(|code| !buttons.pressed(code))
}

impl InputRuntime {
    pub(super) fn binding_mutation_allowed(&self) -> Result<(), InputStateError> {
        self.ensure_running()?;
        if self.notifying_device {
            Err(InputStateError::DeviceNotificationInProgress)
        } else {
            Ok(())
        }
    }

    pub fn create_device(
        &mut self,
        initial_buttons: DeviceButtons,
    ) -> Result<DeviceId, InputStateError> {
        self.ensure_running()?;
        Ok(DeviceId(self.devices.insert(Device {
            buttons: initial_buttons,
            axes: [AxisTransform::NATIVE_DEFAULT; 8],
        })))
    }

    pub fn device_buttons(&self, device: DeviceId) -> Result<DeviceButtons, InputStateError> {
        self.devices
            .get(device.0)
            .map(|device| device.buttons)
            .ok_or(InputStateError::InvalidDevice)
    }

    /// 004F0D00: an input context with empty digital and analog lists.
    pub fn create_context(&mut self) -> Result<InputContextId, InputStateError> {
        self.binding_mutation_allowed()?;
        Ok(InputContextId(self.contexts.insert(Context::default())))
    }

    pub fn selected_context(&self) -> Option<InputContextId> {
        self.selected_context
    }

    /// 004F0470 owns one reference per record, independently of context links.
    pub fn create_binding_set(
        &mut self,
        sources: [Option<DeviceId>; 2],
        records: &[DigitalBinding],
    ) -> Result<BindingSetId, InputStateError> {
        self.binding_mutation_allowed()?;
        for source in sources.into_iter().flatten() {
            self.device_buttons(source)?;
        }
        for record in records {
            self.consumer(record.consumer)?;
        }
        let mut retained = Vec::new();
        for record in records {
            if let Err(error) = self.retain_consumer(record.consumer) {
                for id in retained {
                    self.release_reference(id)?;
                }
                return Err(error);
            }
            retained.push(record.consumer);
        }
        let id = BindingSetId(
            self.binding_sets.insert(BindingSet {
                sources,
                cells: records
                    .iter()
                    .map(|binding| Cell {
                        binding: *binding,
                        satisfied: [sources[0].is_none(), sources[1].is_none()],
                    })
                    .collect(),
            }),
        );
        Ok(id)
    }

    /// 004F0E60 appends a non-owning link. Duplicate links and sharing between
    /// contexts are intentional; cells belong to the set and are not cloned.
    /// Attaching does not reapply an already selected context.
    pub fn attach_binding_set(
        &mut self,
        context: InputContextId,
        set: BindingSetId,
    ) -> Result<(), InputStateError> {
        self.binding_mutation_allowed()?;
        if self.binding_sets.get(set.0).is_none() {
            return Err(InputStateError::InvalidBindingSet);
        }
        self.contexts
            .get_mut(context.0)
            .ok_or(InputStateError::InvalidContext)?
            .sets
            .push(set);
        Ok(())
    }

    /// 004F05C0 removes every matching link before releasing the set's record
    /// references. Surviving consumer counts/held values are NOT recomputed.
    pub fn destroy_binding_set(&mut self, id: BindingSetId) -> Result<(), InputStateError> {
        self.binding_mutation_allowed()?;
        if self.binding_sets.get(id.0).is_none() {
            return Err(InputStateError::InvalidBindingSet);
        }
        for slot in &mut self.contexts.slots {
            if let Some(context) = &mut slot.value {
                context.sets.retain(|set| *set != id);
            }
        }
        let set = self
            .binding_sets
            .remove(id.0)
            .expect("validated binding set");
        for cell in set.cells {
            self.release_reference(cell.binding.consumer)?;
        }
        Ok(())
    }

    /// 004F03B0 silently publishes current held states, including on null
    /// selection. It neither emits release events nor discards queued callbacks.
    pub fn select_context(
        &mut self,
        context: Option<InputContextId>,
    ) -> Result<(), InputStateError> {
        self.binding_mutation_allowed()?;
        if let Some(id) = context {
            if self.contexts.get(id.0).is_none() {
                return Err(InputStateError::InvalidContext);
            }
        }
        self.selected_context = None;
        for &id in &self.consumer_order {
            self.consumers
                .get_mut(id.0)
                .expect("registered consumer")
                .state
                .active_bindings = 0;
        }
        if let Some(id) = context {
            let context = self.contexts.get(id.0).expect("validated context");
            for id in &context.sets {
                let set = self
                    .binding_sets
                    .get_mut(id.0)
                    .expect("registered binding set");
                for cell in &mut set.cells {
                    for (group, source) in set.sources.iter().enumerate() {
                        if let Some(device) = source {
                            let buttons = self
                                .devices
                                .get(device.0)
                                .expect("binding retains device")
                                .buttons;
                            cell.satisfied[group] = satisfied(
                                buttons,
                                cell.binding.required[group],
                                cell.binding.forbidden[group],
                            );
                        }
                    }
                    if cell.satisfied.into_iter().all(|value| value) {
                        let state = &mut self
                            .consumers
                            .get_mut(cell.binding.consumer.0)
                            .expect("binding retains consumer")
                            .state;
                        state.active_bindings = state.active_bindings.wrapping_add(1);
                    }
                }
            }
        }
        for &id in &self.consumer_order {
            let state = &mut self
                .consumers
                .get_mut(id.0)
                .expect("registered consumer")
                .state;
            state.published_value = u32::from(state.active_bindings != 0);
        }
        self.selected_context = context;
        Ok(())
    }

    /// 004F0D60 destroys registration links, not binding sets or their consumer
    /// references. Callers must remove their screen registry binding as well.
    pub fn destroy_context(&mut self, id: InputContextId) -> Result<(), InputStateError> {
        self.binding_mutation_allowed()?;
        if self.contexts.get(id.0).is_none() {
            return Err(InputStateError::InvalidContext);
        }
        if self.selected_context == Some(id) {
            self.select_context(None)?;
        }
        self.contexts.remove(id.0);
        Ok(())
    }

    /// Submit an already decoded physical device snapshot. Native order is each
    /// binding set, source A's records, then source B's records. Matching both
    /// sources can produce intermediate edges; no whole-packet coalescing occurs.
    /// Recursive notifications and structural callback mutations are rejected,
    /// since the original live-pointer traversal provides no safe such contract.
    pub fn update_device<H: InputHost>(
        &mut self,
        device: DeviceId,
        snapshot: DeviceSnapshot,
        host: &mut H,
    ) -> Result<(), InputError<H::Error>> {
        self.binding_mutation_allowed()?;
        self.devices
            .get_mut(device.0)
            .ok_or(InputStateError::InvalidDevice)?
            .buttons = snapshot.buttons;
        let Some(context) = self.selected_context else {
            return Ok(());
        };
        self.notifying_device = true;
        let result = (|| {
            let mut packet = EventPacket::default();
            self.update_bindings(context, device, snapshot.buttons, &mut packet, host)?;
            // Calibration occurs after digital callbacks, as in 004F0F80.
            self.update_analog_bindings(context, device, snapshot.axes, &mut packet, host)
        })();
        self.notifying_device = false;
        result
    }

    fn update_bindings<H: InputHost>(
        &mut self,
        context: InputContextId,
        device: DeviceId,
        buttons: DeviceButtons,
        packet: &mut EventPacket,
        host: &mut H,
    ) -> Result<(), InputError<H::Error>> {
        let set_count = self
            .contexts
            .get(context.0)
            .expect("selected context")
            .sets
            .len();
        for set_index in 0..set_count {
            let set_id = self.contexts.get(context.0).expect("selected context").sets[set_index];
            for group in 0..2 {
                let set = self
                    .binding_sets
                    .get(set_id.0)
                    .expect("registered binding set");
                if set.sources[group] != Some(device) {
                    continue;
                }
                let cell_count = set.cells.len();
                for cell_index in 0..cell_count {
                    let cell = &mut self
                        .binding_sets
                        .get_mut(set_id.0)
                        .expect("registered binding set")
                        .cells[cell_index];
                    let id = cell.binding.consumer;
                    let old_active = cell.satisfied.into_iter().all(|value| value);
                    cell.satisfied[group] = satisfied(
                        buttons,
                        cell.binding.required[group],
                        cell.binding.forbidden[group],
                    );
                    let new_active = cell.satisfied.into_iter().all(|value| value);
                    let state = &mut self.consumer_mut(id)?.state;
                    let was_held = state.active_bindings != 0;
                    state.active_bindings = state
                        .active_bindings
                        .wrapping_sub(u32::from(old_active))
                        .wrapping_add(u32::from(new_active));
                    let held = state.active_bindings != 0;
                    if was_held != held {
                        self.emit(id, u32::from(held), packet, host)?;
                    }
                }
            }
        }
        Ok(())
    }
}
