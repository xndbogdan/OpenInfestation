//! Retained current player analog metadata and lazy authored digital reads.
//!
//! 00469D8E/DA7/DB9 name 0054ECF8/ECE8/ED08 in the supported PE's zero-filled
//! .data tail. Startup seeds those records once; it does not make writable
//! native metadata immutable. This owner admits the same known, stable, untimed
//! metadata lifetime as InputRuntime's copied consumers. A later native writer
//! needs shared current-metadata ownership, not a descriptor-read fallback.
//! Keep this owner with the acquiring InputRuntime until explicit component
//! release. This does not initialize bindings, devices or an input context.

use crate::input::{ActionDescriptor, AnalogActionDescriptor};
use crate::object_runtime::{
    PlayerInputDescriptor, PlayerInputDescriptorKind, PlayerInputDescriptorRequest,
    PlayerInputDescriptorSource,
};
use inf_formats::{
    parse_digital_action_descriptor, ByteSpan, FrontendExecutable, FrontendExecutableError,
    InputTableError,
};
use thiserror::Error;

/// The player's three analog descriptors, in `.data`'s zero fill.
pub const ANALOG_IDENTITIES: [u32; 3] = [0x0054_ecf8, 0x0054_ece8, 0x0054_ed08];
const DATA_ADDRESS: u32 = 0x0052_e000;
const DATA_VIRTUAL_BYTES: u32 = 0x0013_2c38;
const DATA_RAW_BYTES: usize = 0x0001_9000;
const DATA_FILE_OFFSET: usize = 0x0012_e000;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum PlayerInputSourceError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("player analog startup requires the supported PE .data zero-tail profile")]
    UnsupportedLoaderProfile,
    #[error("player analog identity {identity:#010x} is outside the three retained records")]
    UnknownAnalog { identity: u32 },
    #[error("current player analog metadata {identity:#010x} is unavailable")]
    UnavailableAnalog { identity: u32 },
    #[error(transparent)]
    Digital(#[from] InputTableError),
}

/// Slots are the constructor's +5C, +60 and +64 source records, in that order.
/// None remains unavailable; a source request never reconstructs startup data.
/// There is no mutable setter: supplied current records describe a stable
/// domain, rather than implementing an unproved native metadata writer.
#[derive(Debug, PartialEq, Eq)]
pub struct PlayerAnalogDescriptors {
    current: [Option<AnalogActionDescriptor>; 3],
}

impl PlayerAnalogDescriptors {
    /// Verify the existing structural executable profile and original .data
    /// layout, then seed loader-initial metadata once. No file span is invented
    /// for virtual storage. This does not authenticate the file hash, execute
    /// CRT initialization, relocate the image or prove later writer absence.
    pub fn native_startup(executable: &[u8]) -> Result<Self, PlayerInputSourceError> {
        validate_zero_tail(executable)?;
        Ok(Self {
            current: ANALOG_IDENTITIES.map(|identity| {
                Some(AnalogActionDescriptor {
                    action: ActionDescriptor {
                        identity,
                        callback_address: None,
                        flags: 0,
                    },
                    period_micros: 0,
                })
            }),
        })
    }

    /// Explicit current backing, including unavailable records. Values are
    /// returned unchanged when reached: the existing player/input owners keep
    /// their identity, period and copied-metadata conflict checks and ordering.
    pub const fn from_current(current: [Option<AnalogActionDescriptor>; 3]) -> Self {
        Self { current }
    }

    fn descriptor(&self, identity: u32) -> Result<AnalogActionDescriptor, PlayerInputSourceError> {
        let index = ANALOG_IDENTITIES
            .iter()
            .position(|&candidate| candidate == identity)
            .ok_or(PlayerInputSourceError::UnknownAnalog { identity })?;
        self.current[index].ok_or(PlayerInputSourceError::UnavailableAnalog { identity })
    }
}

/// Borrow actual retained analog state and the existing file-only executable
/// view. No records are scanned at construction; each digital parser call and
/// analog availability read occurs at the reached source request. This loan
/// creates no consumer reference and does not reset either owner.
pub struct PlayerInputSource<'state, 'exe> {
    pub analog: &'state PlayerAnalogDescriptors,
    pub executable: FrontendExecutable<'exe>,
}

impl PlayerInputDescriptorSource for PlayerInputSource<'_, '_> {
    type Error = PlayerInputSourceError;

    fn descriptor(
        &mut self,
        request: PlayerInputDescriptorRequest,
    ) -> Result<PlayerInputDescriptor, Self::Error> {
        match request.kind {
            PlayerInputDescriptorKind::Analog => Ok(PlayerInputDescriptor::Analog(
                self.analog.descriptor(request.identity)?,
            )),
            PlayerInputDescriptorKind::Digital => {
                let record = parse_digital_action_descriptor(self.executable, request.identity)?;
                Ok(PlayerInputDescriptor::Digital(ActionDescriptor {
                    identity: record.identity,
                    callback_address: (record.callback_address != 0)
                        .then_some(record.callback_address),
                    flags: record.flags,
                }))
            }
        }
    }
}

pub(crate) fn validate_zero_tail(bytes: &[u8]) -> Result<(), PlayerInputSourceError> {
    let mapped = FrontendExecutable::parse(bytes)?;
    if mapped.data().virtual_address != DATA_ADDRESS
        || mapped.data().bytes.len() != DATA_RAW_BYTES
        || mapped.file_span() != ByteSpan::new(DATA_FILE_OFFSET, DATA_FILE_OFFSET + DATA_RAW_BYTES)
    {
        return Err(PlayerInputSourceError::UnsupportedLoaderProfile);
    }
    // FrontendExecutable deliberately retains only raw mapped ranges. Inspect
    // the PE section header here solely for the loader-initial domain; mapped()
    // and the authored descriptor parser keep their file-backed contract.
    let pe = read_u32(bytes, 0x3c)? as usize;
    let count = read_u16(bytes, pe + 6)? as usize;
    let optional_bytes = read_u16(bytes, pe + 20)? as usize;
    let sections = pe + 24 + optional_bytes;
    for index in 0..count {
        let offset = sections + index * 40;
        if bytes.get(offset..offset + 8) != Some(b".data\0\0\0".as_slice()) {
            continue;
        }
        if read_u32(bytes, offset + 8)? != DATA_VIRTUAL_BYTES
            || read_u32(bytes, offset + 36)? != 0xc000_0040
        {
            return Err(PlayerInputSourceError::UnsupportedLoaderProfile);
        }
        let raw_end = u64::from(DATA_ADDRESS) + DATA_RAW_BYTES as u64;
        let virtual_end = u64::from(DATA_ADDRESS) + u64::from(DATA_VIRTUAL_BYTES);
        if ANALOG_IDENTITIES.iter().any(|&identity| {
            u64::from(identity) < raw_end || u64::from(identity) + 12 > virtual_end
        }) {
            return Err(PlayerInputSourceError::UnsupportedLoaderProfile);
        }
        return Ok(());
    }
    Err(PlayerInputSourceError::UnsupportedLoaderProfile)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, FrontendExecutableError> {
    let source = bytes
        .get(offset..offset + 4)
        .ok_or(FrontendExecutableError::Truncated { offset, bytes: 4 })?;
    Ok(u32::from_le_bytes(source.try_into().expect("four bytes")))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, FrontendExecutableError> {
    let source = bytes
        .get(offset..offset + 2)
        .ok_or(FrontendExecutableError::Truncated { offset, bytes: 2 })?;
    Ok(u16::from_le_bytes(source.try_into().expect("two bytes")))
}

#[cfg(test)]
mod tests;
