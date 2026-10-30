use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::BehaviorRowSelection;
use crate::object_runtime::{DerivedActorError, ObjectArena, ObjectId};

const DESCRIPTOR_START: u32 = 0x0053_0e60;
const REGISTRY_START: u32 = 0x0053_dba8;
const DEFAULT_PACKET: u32 = 0x0054_6a94;
/// The Pursuit Ship's one 44-byte row (0048CC70's table 0052F748).
const PURSUIT_SHIP_TABLE: u32 = 0x0052_f748;
const PURSUIT_SHIP_ROW: usize = 44;

/// Mutable process data mapped by the supported executable. The 860-byte
/// descriptor region includes its authored padding, preserving address aliases
/// between nominal tables. None is unavailable storage; unknown byte values
/// within available storage are a different custody state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessActorBehaviorTables {
    pub descriptor_bytes: Option<Box<[Option<u8>; 860]>>,
    pub registry_bytes: Option<Box<[Option<u8>; 528]>>,
    pub default_packet_bytes: Option<[Option<u8>; 4]>,
    pub pursuit_ship_bytes: Option<[Option<u8>; PURSUIT_SHIP_ROW]>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BehaviorCacheStorageError {
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("unavailable process storage at {address:08X}")]
    Unavailable { address: u32 },
    #[error("unknown process byte at {address:08X}")]
    Unknown { address: u32 },
    #[error("process span {address:08X}+{size} lies outside owned behavior storage")]
    Outside { address: u32, size: usize },
    #[error("vehicle {owner:?} requires its +26C data and known-byte mask")]
    MissingVehicleCopy { owner: ObjectId },
    #[error("vehicle {owner:?} span +26C+{offset}+{size} lies outside its inline copy")]
    OutsideVehicle {
        owner: ObjectId,
        offset: i32,
        size: usize,
    },
    #[error("vehicle {owner:?} has an unknown +26C byte at offset {offset}")]
    UnknownVehicle { owner: ObjectId, offset: i32 },
}

impl ProcessActorBehaviorTables {
    /// Structural mapping, independently of any native checker's EXE hash.
    pub fn from_executable(
        source: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        let descriptors = source.mapped(DESCRIPTOR_START, 860)?;
        let registry = source.mapped(REGISTRY_START, 528)?;
        let default = source.mapped(DEFAULT_PACKET, 4)?;
        let ship = source.mapped(PURSUIT_SHIP_TABLE, PURSUIT_SHIP_ROW)?;
        Ok(Self {
            descriptor_bytes: Some(Box::new(std::array::from_fn(|i| {
                Some(descriptors.bytes[i])
            }))),
            registry_bytes: Some(Box::new(std::array::from_fn(|i| Some(registry.bytes[i])))),
            default_packet_bytes: Some(std::array::from_fn(|i| Some(default.bytes[i]))),
            pursuit_ship_bytes: Some(std::array::from_fn(|i| Some(ship.bytes[i]))),
        })
    }

    fn slice(&self, address: u32, size: usize) -> Result<&[Option<u8>], BehaviorCacheStorageError> {
        for (base, length, source) in [
            (
                DESCRIPTOR_START,
                860,
                self.descriptor_bytes.as_deref().map(|v| v.as_slice()),
            ),
            (
                REGISTRY_START,
                528,
                self.registry_bytes.as_deref().map(|v| v.as_slice()),
            ),
            (
                DEFAULT_PACKET,
                4,
                self.default_packet_bytes.as_ref().map(|v| v.as_slice()),
            ),
            (
                PURSUIT_SHIP_TABLE,
                PURSUIT_SHIP_ROW,
                self.pursuit_ship_bytes.as_ref().map(|v| v.as_slice()),
            ),
        ] {
            if let Some(offset) = address.checked_sub(base) {
                let offset = offset as usize;
                if offset.checked_add(size).is_some_and(|end| end <= length) {
                    return source
                        .ok_or(BehaviorCacheStorageError::Unavailable { address })
                        .map(|v| &v[offset..offset + size]);
                }
            }
        }
        Err(BehaviorCacheStorageError::Outside { address, size })
    }

    pub fn read_process_bytes(
        &self,
        address: u32,
        size: usize,
    ) -> Result<&[Option<u8>], BehaviorCacheStorageError> {
        self.slice(address, size)
    }

    pub fn write_process_bytes(
        &mut self,
        address: u32,
        bytes: &[Option<u8>],
    ) -> Result<(), BehaviorCacheStorageError> {
        // Verify the reached span before touching any bytes of this store.
        self.slice(address, bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }
        if address >= REGISTRY_START && address < REGISTRY_START + 528 {
            let offset = (address - REGISTRY_START) as usize;
            self.registry_bytes.as_deref_mut().expect("checked storage")
                [offset..offset + bytes.len()]
                .copy_from_slice(bytes);
        } else if address >= DEFAULT_PACKET && address < DEFAULT_PACKET + 4 {
            let offset = (address - DEFAULT_PACKET) as usize;
            self.default_packet_bytes.as_mut().expect("checked storage")
                [offset..offset + bytes.len()]
                .copy_from_slice(bytes);
        } else if address >= PURSUIT_SHIP_TABLE
            && address < PURSUIT_SHIP_TABLE + PURSUIT_SHIP_ROW as u32
        {
            let offset = (address - PURSUIT_SHIP_TABLE) as usize;
            self.pursuit_ship_bytes.as_mut().expect("checked storage")
                [offset..offset + bytes.len()]
                .copy_from_slice(bytes);
        } else {
            let offset = (address - DESCRIPTOR_START) as usize;
            self.descriptor_bytes
                .as_deref_mut()
                .expect("checked storage")[offset..offset + bytes.len()]
                .copy_from_slice(bytes);
        }
        Ok(())
    }

    pub(super) fn default_bytes(&self) -> Result<[Option<u8>; 4], BehaviorCacheStorageError> {
        Ok(self
            .slice(DEFAULT_PACKET, 4)?
            .try_into()
            .expect("four-byte packet"))
    }

    pub(crate) fn read_dword(&self, address: u32) -> Result<u32, BehaviorCacheStorageError> {
        let bytes = known(self.slice(address, 4)?, address)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("four-byte DWORD"),
        ))
    }
    pub(crate) fn read_word(&self, address: u32) -> Result<u16, BehaviorCacheStorageError> {
        let bytes = known(self.slice(address, 2)?, address)?;
        Ok(u16::from_le_bytes(bytes.try_into().expect("two-byte WORD")))
    }
}

fn known(bytes: &[Option<u8>], address: u32) -> Result<Vec<u8>, BehaviorCacheStorageError> {
    bytes
        .iter()
        .enumerate()
        .map(|(i, byte)| {
            byte.ok_or(BehaviorCacheStorageError::Unknown {
                address: address.wrapping_add(i as u32),
            })
        })
        .collect()
}

fn vehicle_offset(
    owner: ObjectId,
    offset: i32,
    size: usize,
) -> Result<usize, BehaviorCacheStorageError> {
    if offset < 0
        || (offset as usize)
            .checked_add(size)
            .is_none_or(|end| end > 704)
    {
        Err(BehaviorCacheStorageError::OutsideVehicle {
            owner,
            offset,
            size,
        })
    } else {
        Ok(offset as usize)
    }
}

pub(crate) fn read_row_byte(
    arena: &ObjectArena,
    tables: &ProcessActorBehaviorTables,
    row: BehaviorRowSelection,
    offset: u32,
) -> Result<u8, BehaviorCacheStorageError> {
    match row {
        BehaviorRowSelection::SharedAddress { address } => {
            let address = address.wrapping_add(offset);
            tables.slice(address, 1)?[0].ok_or(BehaviorCacheStorageError::Unknown { address })
        }
        BehaviorRowSelection::VehicleCopy { owner, byte_offset } => {
            read_vehicle_row_byte(arena, owner, byte_offset, offset)
        }
    }
}

fn read_vehicle_row_byte(
    arena: &ObjectArena,
    owner: ObjectId,
    byte_offset: i32,
    offset: u32,
) -> Result<u8, BehaviorCacheStorageError> {
    let offset = byte_offset.wrapping_add(offset as i32);
    let index = vehicle_offset(owner, offset, 1)?;
    let vehicle = arena.selector2_actor(owner)?;
    let values = vehicle
        .table_26c
        .as_deref()
        .ok_or(BehaviorCacheStorageError::MissingVehicleCopy { owner })?;
    let mask = vehicle
        .table_26c_known
        .as_deref()
        .ok_or(BehaviorCacheStorageError::MissingVehicleCopy { owner })?;
    if !mask[index] {
        return Err(BehaviorCacheStorageError::UnknownVehicle { owner, offset });
    }
    Ok((values[index / 4] >> ((index % 4) * 8)) as u8)
}

/// A DWORD of a row in the vehicle's own `+26C` copy, which needs none of
/// the process tables.
pub(crate) fn read_vehicle_row_dword(
    arena: &ObjectArena,
    owner: ObjectId,
    byte_offset: i32,
    offset: u32,
) -> Result<u32, BehaviorCacheStorageError> {
    (0..4).try_fold(0u32, |value, index| {
        let byte = read_vehicle_row_byte(arena, owner, byte_offset, offset.wrapping_add(index))?;
        Ok(value | (u32::from(byte) << (index * 8)))
    })
}

pub(crate) fn read_row_word(
    arena: &ObjectArena,
    tables: &ProcessActorBehaviorTables,
    row: BehaviorRowSelection,
    offset: u32,
) -> Result<u16, BehaviorCacheStorageError> {
    // The typed backed span retains lazy unknown-byte custody. Raw unbacked
    // per-component memory faults remain outside this portable owner.
    let low = read_row_byte(arena, tables, row, offset)?;
    let high = read_row_byte(arena, tables, row, offset.wrapping_add(1))?;
    Ok(u16::from_le_bytes([low, high]))
}

pub(crate) fn read_row_dword(
    arena: &ObjectArena,
    tables: &ProcessActorBehaviorTables,
    row: BehaviorRowSelection,
    offset: u32,
) -> Result<u32, BehaviorCacheStorageError> {
    let low = read_row_word(arena, tables, row, offset)?;
    let high = read_row_word(arena, tables, row, offset.wrapping_add(2))?;
    Ok(u32::from(low) | (u32::from(high) << 16))
}

pub(super) fn write_row(
    arena: &mut ObjectArena,
    tables: &mut ProcessActorBehaviorTables,
    row: BehaviorRowSelection,
    offset: u32,
    bytes: &[Option<u8>],
) -> Result<(), BehaviorCacheStorageError> {
    match row {
        BehaviorRowSelection::SharedAddress { address } => {
            tables.write_process_bytes(address.wrapping_add(offset), bytes)
        }
        BehaviorRowSelection::VehicleCopy { owner, byte_offset } => {
            let offset = byte_offset.wrapping_add(offset as i32);
            let index = vehicle_offset(owner, offset, bytes.len())?;
            let vehicle = arena.selector2_actor_mut(owner)?;
            let values = vehicle
                .table_26c
                .as_deref_mut()
                .ok_or(BehaviorCacheStorageError::MissingVehicleCopy { owner })?;
            let mask = vehicle
                .table_26c_known
                .as_deref_mut()
                .ok_or(BehaviorCacheStorageError::MissingVehicleCopy { owner })?;
            for (i, byte) in bytes.iter().copied().enumerate() {
                let index = index + i;
                mask[index] = byte.is_some();
                if let Some(byte) = byte {
                    let shift = (index % 4) * 8;
                    values[index / 4] =
                        (values[index / 4] & !(0xff << shift)) | (u32::from(byte) << shift);
                }
            }
            Ok(())
        }
    }
}
