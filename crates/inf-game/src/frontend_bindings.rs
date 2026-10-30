//! Frontend input ownership at 00437F90 and 00439B30/00439D00/00439D50.
//!
//! Device enumeration, native eligibility filtering (+10 == 0 && +14 == 0),
//! and 00439AD0 calibration are required external inputs. Table presence does
//! not establish device presence. Device adapters outlive this screen owner.
//! Failures retain completed acquisitions for explicit teardown; releasing a
//! final consumer can invalidate queued targets, so cleanup is not rollback.

use inf_formats::{
    parse_analog_action_descriptor, AnalogInputTable, ByteSpan, DigitalInputTable,
    FrontendExecutable, InputTableError, ScreenInputTables, INPUT_DEVICE_SLOTS,
};
use thiserror::Error;

use crate::input::{
    ActionDescriptor, AnalogActionDescriptor, AnalogBinding, AnalogBindingSetId, AnalogConsumerId,
    AnalogPolicy, AxisTransform, BindingSetId, ConsumerId, DeviceId, DigitalBinding,
    InputContextId, InputRuntime, InputStateError,
};
use crate::screen_input::ScreenAnalogValues;

pub const SCREEN_ANALOG_CC_DESCRIPTOR: u32 = 0x0054_6f10;
pub const SCREEN_ANALOG_D0_DESCRIPTOR: u32 = 0x0054_6f00;

/// Base-screen references exist even when enumeration finds no analog device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenAnalogDescriptors {
    pub consumer_cc: inf_formats::AnalogActionDescriptor,
    pub consumer_d0: inf_formats::AnalogActionDescriptor,
}

impl ScreenAnalogDescriptors {
    pub fn from_executable(source: FrontendExecutable<'_>) -> Result<Self, InputTableError> {
        Ok(Self {
            consumer_cc: parse_analog_action_descriptor(source, SCREEN_ANALOG_CC_DESCRIPTOR)?,
            consumer_d0: parse_analog_action_descriptor(source, SCREEN_ANALOG_D0_DESCRIPTOR)?,
        })
    }
}

/// One caller-enumerated ELIGIBLE device, in native enumeration order.
/// `source` must already have the caller's 00439AD0-equivalent calibration.
/// Its native type is not an OS key/device mapping. Repeated types are rejected
/// rather than reproducing the retail helper's overwritten owning pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrontendDevice {
    pub native_type: u32,
    pub source: DeviceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendBindingPhase {
    Metadata,
    Initialized,
    Installed,
    Stopped,
    SetsReleased,
    Released,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrontendBindingError {
    #[error("frontend input requires phase {expected:?}, currently {actual:?}")]
    Phase {
        expected: FrontendBindingPhase,
        actual: FrontendBindingPhase,
    },
    #[error("native input device type {0} is outside the twelve frontend slots")]
    DeviceType(u32),
    #[error("native input device type {0} appeared twice in the eligible enumeration")]
    DuplicateDeviceType(u32),
    #[error(
        "native input device type {native_type} has no valid supplied logical source: {source}"
    )]
    Device {
        native_type: u32,
        source: InputStateError,
    },
    #[error("screen descriptor must have native identity {expected:#010x}, got {actual:#010x}")]
    ScreenDescriptor { expected: u32, actual: u32 },
    #[error("input descriptor {0:#010x} has invalid source provenance")]
    DescriptorProvenance(u32),
    #[error(
        "input table {address:#010x} for native type {native_type} has invalid source provenance"
    )]
    TableProvenance { native_type: u32, address: u32 },
    #[error("base screen analog consumer +{0:X} is not owned")]
    MissingScreenConsumer(u16),
    #[error(transparent)]
    Input(#[from] InputStateError),
}

#[derive(Debug, Default)]
struct DigitalOwner {
    set: Option<BindingSetId>,
    consumers: Vec<ConsumerId>,
    /// Acquisitions not yet released after transfer to the set's own refs.
    released_external: usize,
}

#[derive(Debug, Default)]
struct AnalogOwner {
    set: Option<AnalogBindingSetId>,
    consumers: Vec<AnalogConsumerId>,
    released_external: usize,
}

/// Explicitly staged because native audio/residency/menu operations occur
/// between base initialization, binding installation, and the two teardown
/// phases. Dropping metadata cannot release resources in a separate runtime.
#[derive(Debug)]
pub struct FrontendBindings {
    callback_context: u32,
    descriptors: ScreenAnalogDescriptors,
    phase: FrontendBindingPhase,
    context: Option<InputContextId>,
    consumer_cc: Option<AnalogConsumerId>,
    consumer_d0: Option<AnalogConsumerId>,
    digital: [DigitalOwner; INPUT_DEVICE_SLOTS],
    analog: [AnalogOwner; INPUT_DEVICE_SLOTS],
}

impl FrontendBindings {
    /// Metadata only. `initialize` performs the actual base-screen acquisitions.
    pub fn new(callback_context: u32, descriptors: ScreenAnalogDescriptors) -> Self {
        Self {
            callback_context,
            descriptors,
            phase: FrontendBindingPhase::Metadata,
            context: None,
            consumer_cc: None,
            consumer_d0: None,
            digital: std::array::from_fn(|_| DigitalOwner::default()),
            analog: std::array::from_fn(|_| AnalogOwner::default()),
        }
    }

    pub fn phase(&self) -> FrontendBindingPhase {
        self.phase
    }
    pub fn context(&self) -> Option<InputContextId> {
        self.context
    }
    pub fn consumer_cc(&self) -> Option<AnalogConsumerId> {
        self.consumer_cc
    }
    pub fn consumer_d0(&self) -> Option<AnalogConsumerId> {
        self.consumer_d0
    }

    pub fn analog_values(
        &self,
        input: &InputRuntime,
    ) -> Result<ScreenAnalogValues, FrontendBindingError> {
        Ok(ScreenAnalogValues {
            consumer_cc: input.analog_value(
                self.consumer_cc
                    .ok_or(FrontendBindingError::MissingScreenConsumer(0xcc))?,
            )?,
            consumer_d0: input.analog_value(
                self.consumer_d0
                    .ok_or(FrontendBindingError::MissingScreenConsumer(0xd0))?,
            )?,
        })
    }

    /// 00437F90: create context, acquire +CC, then +D0. No context selection.
    pub fn initialize(&mut self, input: &mut InputRuntime) -> Result<(), FrontendBindingError> {
        self.require_phase(FrontendBindingPhase::Metadata)?;
        self.phase = FrontendBindingPhase::Stopped;
        for (descriptor, expected) in [
            (self.descriptors.consumer_cc, SCREEN_ANALOG_CC_DESCRIPTOR),
            (self.descriptors.consumer_d0, SCREEN_ANALOG_D0_DESCRIPTOR),
        ] {
            if descriptor.identity != expected {
                return Err(FrontendBindingError::ScreenDescriptor {
                    expected,
                    actual: descriptor.identity,
                });
            }
            validate_analog_descriptor(descriptor)?;
        }
        self.context = Some(input.create_context()?);
        self.consumer_cc = Some(input.acquire_analog_consumer(
            analog_descriptor(self.descriptors.consumer_cc),
            self.callback_context,
        )?);
        self.consumer_d0 = Some(input.acquire_analog_consumer(
            analog_descriptor(self.descriptors.consumer_d0),
            self.callback_context,
        )?);
        self.phase = FrontendBindingPhase::Initialized;
        Ok(())
    }

    /// 00439B30 creates digital then analog for each eligible enumerated device.
    /// 00439D00 then attaches every digital slot ascending, followed by every
    /// analog slot ascending. Selection remains the screen registry's operation.
    pub fn install(
        &mut self,
        input: &mut InputRuntime,
        tables: &ScreenInputTables,
        devices: &[FrontendDevice],
    ) -> Result<(), FrontendBindingError> {
        self.require_phase(FrontendBindingPhase::Initialized)?;
        self.phase = FrontendBindingPhase::Stopped;
        // Validate ownership against this runtime without changing selection.
        self.analog_values(input)?;
        let context = self.context.expect("initialized context");
        self.create_and_attach(input, context, tables, devices)?;
        self.phase = FrontendBindingPhase::Installed;
        Ok(())
    }

    /// 00439B30/00439D00 on the gameplay screen's +0xFC sub-object: the same
    /// set creation over this owner's callback context (the first player's),
    /// attached to the screen's existing context. The sub-object owns no
    /// context and no screen consumers, so its `release_base` frees nothing.
    pub fn install_sub_object(
        &mut self,
        input: &mut InputRuntime,
        context: InputContextId,
        tables: &ScreenInputTables,
        devices: &[FrontendDevice],
    ) -> Result<(), FrontendBindingError> {
        self.require_phase(FrontendBindingPhase::Metadata)?;
        self.phase = FrontendBindingPhase::Stopped;
        self.create_and_attach(input, context, tables, devices)?;
        self.phase = FrontendBindingPhase::Installed;
        Ok(())
    }

    fn create_and_attach(
        &mut self,
        input: &mut InputRuntime,
        context: InputContextId,
        tables: &ScreenInputTables,
        devices: &[FrontendDevice],
    ) -> Result<(), FrontendBindingError> {
        let mut seen = [false; INPUT_DEVICE_SLOTS];
        for device in devices {
            let slot = usize::try_from(device.native_type)
                .ok()
                .filter(|slot| *slot < INPUT_DEVICE_SLOTS)
                .ok_or(FrontendBindingError::DeviceType(device.native_type))?;
            if std::mem::replace(&mut seen[slot], true) {
                return Err(FrontendBindingError::DuplicateDeviceType(
                    device.native_type,
                ));
            }
            input
                .device_buttons(device.source)
                .map_err(|source| FrontendBindingError::Device {
                    native_type: device.native_type,
                    source,
                })?;
            if let Some(table) = &tables.digital[slot] {
                validate_digital_table(device.native_type, table)?;
            }
            if let Some(table) = &tables.analog[slot] {
                validate_analog_table(device.native_type, table)?;
            }
        }
        for device in devices {
            let slot = device.native_type as usize;
            if let Some(table) = &tables.digital[slot] {
                self.create_digital(input, slot, device.source, table)?;
            }
            if let Some(table) = &tables.analog[slot] {
                self.create_analog(input, slot, device.source, table)?;
            }
        }
        for owner in &self.digital {
            if let Some(set) = owner.set {
                input.attach_binding_set(context, set)?;
            }
        }
        for owner in &self.analog {
            if let Some(set) = owner.set {
                input.attach_analog_binding_set(context, set)?;
            }
        }
        Ok(())
    }

    fn create_digital(
        &mut self,
        input: &mut InputRuntime,
        slot: usize,
        source: DeviceId,
        table: &DigitalInputTable,
    ) -> Result<(), FrontendBindingError> {
        let owner = &mut self.digital[slot];
        let mut records = Vec::with_capacity(table.records.len());
        for record in &table.records {
            let descriptor = record.descriptor;
            let consumer = input.acquire_consumer(
                action_descriptor(
                    descriptor.identity,
                    descriptor.callback_address,
                    descriptor.flags,
                ),
                self.callback_context,
            )?;
            owner.consumers.push(consumer);
            records.push(DigitalBinding {
                consumer,
                required: record.required,
                forbidden: record.forbidden,
            });
        }
        // 00439BFF supplies adapter as A and literal zero as B. A record's B
        // lists are still preserved; the absent source is satisfied natively.
        owner.set = Some(input.create_binding_set([Some(source), None], &records)?);
        release_digital_externals(input, owner)?;
        Ok(())
    }

    fn create_analog(
        &mut self,
        input: &mut InputRuntime,
        slot: usize,
        source: DeviceId,
        table: &AnalogInputTable,
    ) -> Result<(), FrontendBindingError> {
        let owner = &mut self.analog[slot];
        let mut records = Vec::with_capacity(table.records.len());
        for record in &table.records {
            let consumer = input.acquire_analog_consumer(
                analog_descriptor(record.descriptor),
                self.callback_context,
            )?;
            owner.consumers.push(consumer);
            records.push(AnalogBinding {
                consumer,
                axis: record.axis,
                transform: AxisTransform {
                    offset: record.offset,
                    q31_factor: record.scale_q31,
                    integer_factor: record.multiplier,
                },
            });
        }
        let policy = if table
            .records
            .first()
            .is_some_and(|record| record.flags & 1 != 0)
        {
            AnalogPolicy::AccumulateChanges
        } else {
            AnalogPolicy::Replace
        };
        owner.set = Some(input.create_analog_binding_set(source, policy, &records)?);
        release_analog_externals(input, owner)?;
        Ok(())
    }

    /// 00439D50: digital[i], analog[i], then advance i. Works after partial
    /// initialization/installation too. Successful releases are not retried.
    pub fn release_sets(&mut self, input: &mut InputRuntime) -> Result<(), FrontendBindingError> {
        if matches!(
            self.phase,
            FrontendBindingPhase::SetsReleased | FrontendBindingPhase::Released
        ) {
            return Ok(());
        }
        self.phase = FrontendBindingPhase::Stopped;
        for slot in 0..INPUT_DEVICE_SLOTS {
            let digital = &mut self.digital[slot];
            if let Some(set) = digital.set {
                input.destroy_binding_set(set)?;
                digital.set = None;
            }
            release_digital_externals(input, digital)?;
            let analog = &mut self.analog[slot];
            if let Some(set) = analog.set {
                input.destroy_analog_binding_set(set)?;
                analog.set = None;
            }
            release_analog_externals(input, analog)?;
        }
        self.phase = FrontendBindingPhase::SetsReleased;
        Ok(())
    }

    /// After the caller's returning-entry write and menu destruction: 00438080
    /// releases +D0, then +CC, then the context. Never destroys device adapters.
    pub fn release_base(&mut self, input: &mut InputRuntime) -> Result<(), FrontendBindingError> {
        if self.phase == FrontendBindingPhase::Released {
            return Ok(());
        }
        self.require_phase(FrontendBindingPhase::SetsReleased)?;
        if let Some(id) = self.consumer_d0 {
            input.release_analog_consumer(id)?;
            self.consumer_d0 = None;
        }
        if let Some(id) = self.consumer_cc {
            input.release_analog_consumer(id)?;
            self.consumer_cc = None;
        }
        if let Some(context) = self.context {
            input.destroy_context(context)?;
            self.context = None;
        }
        self.phase = FrontendBindingPhase::Released;
        Ok(())
    }

    fn require_phase(&self, expected: FrontendBindingPhase) -> Result<(), FrontendBindingError> {
        if self.phase == expected {
            Ok(())
        } else {
            Err(FrontendBindingError::Phase {
                expected,
                actual: self.phase,
            })
        }
    }
}

fn release_digital_externals(
    input: &mut InputRuntime,
    owner: &mut DigitalOwner,
) -> Result<(), InputStateError> {
    while owner.released_external < owner.consumers.len() {
        input.release_consumer(owner.consumers[owner.released_external])?;
        owner.released_external += 1;
    }
    Ok(())
}

fn release_analog_externals(
    input: &mut InputRuntime,
    owner: &mut AnalogOwner,
) -> Result<(), InputStateError> {
    while owner.released_external < owner.consumers.len() {
        input.release_analog_consumer(owner.consumers[owner.released_external])?;
        owner.released_external += 1;
    }
    Ok(())
}

fn action_descriptor(identity: u32, callback: u32, flags: u32) -> ActionDescriptor {
    ActionDescriptor {
        identity,
        callback_address: (callback != 0).then_some(callback),
        flags,
    }
}

fn analog_descriptor(descriptor: inf_formats::AnalogActionDescriptor) -> AnalogActionDescriptor {
    AnalogActionDescriptor {
        action: action_descriptor(
            descriptor.identity,
            descriptor.callback_address,
            descriptor.flags,
        ),
        period_micros: descriptor.period_micros as i32,
    }
}

fn valid_span(span: ByteSpan, size: usize) -> bool {
    span.start.checked_add(size) == Some(span.end)
}

fn validate_descriptor(
    identity: u32,
    span: ByteSpan,
    size: usize,
) -> Result<(), FrontendBindingError> {
    if identity == 0 || !valid_span(span, size) {
        Err(FrontendBindingError::DescriptorProvenance(identity))
    } else {
        Ok(())
    }
}

fn validate_analog_descriptor(
    descriptor: inf_formats::AnalogActionDescriptor,
) -> Result<(), FrontendBindingError> {
    match descriptor.file_span {
        Some(span) => validate_descriptor(descriptor.identity, span, 12)?,
        // Only the player's three lie in the loader's zero fill.
        None if crate::player_input_source::ANALOG_IDENTITIES.contains(&descriptor.identity) => {}
        None => {
            return Err(FrontendBindingError::DescriptorProvenance(
                descriptor.identity,
            ))
        }
    }
    if descriptor.period_micros != 0 {
        return Err(
            InputStateError::UnsupportedAnalogTimer(descriptor.period_micros as i32).into(),
        );
    }
    Ok(())
}

fn validate_digital_table(
    native_type: u32,
    table: &DigitalInputTable,
) -> Result<(), FrontendBindingError> {
    validate_table(
        native_type,
        table.virtual_address,
        table.terminator,
        20,
        table
            .records
            .iter()
            .map(|record| (record.virtual_address, record.file_span)),
    )?;
    for record in &table.records {
        validate_descriptor(record.descriptor.identity, record.descriptor.file_span, 8)?;
    }
    Ok(())
}

fn validate_analog_table(
    native_type: u32,
    table: &AnalogInputTable,
) -> Result<(), FrontendBindingError> {
    validate_table(
        native_type,
        table.virtual_address,
        table.terminator,
        24,
        table
            .records
            .iter()
            .map(|record| (record.virtual_address, record.file_span)),
    )?;
    for record in &table.records {
        validate_analog_descriptor(record.descriptor)?;
        if record.axis >= 8 {
            return Err(InputStateError::InvalidAxis(record.axis).into());
        }
    }
    Ok(())
}

fn validate_table(
    native_type: u32,
    address: u32,
    terminator: ByteSpan,
    stride: usize,
    rows: impl Iterator<Item = (u32, ByteSpan)>,
) -> Result<(), FrontendBindingError> {
    let error = || FrontendBindingError::TableProvenance {
        native_type,
        address,
    };
    if address == 0 || !valid_span(terminator, 4) {
        return Err(error());
    }
    let mut next_address = address;
    let mut next_file = None;
    for (row_address, span) in rows {
        if row_address != next_address
            || !valid_span(span, stride)
            || next_file.is_some_and(|file| file != span.start)
        {
            return Err(error());
        }
        next_address = next_address.checked_add(stride as u32).ok_or_else(error)?;
        next_file = Some(span.end);
    }
    if next_file.is_some_and(|file| file != terminator.start) {
        return Err(error());
    }
    Ok(())
}

#[cfg(test)]
#[path = "frontend_bindings_tests.rs"]
mod tests;
