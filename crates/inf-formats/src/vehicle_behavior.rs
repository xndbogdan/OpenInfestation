//! Selector-2 vehicle behavior descriptors copied by its admission guard.
//!
//! 0041A620 copies 16 records of 44 bytes from 00530158..00530418 into the
//! vehicle at +26C before 0048CC70 retains that object copy as behavior +14.
//! Record fields are preserved as DWORDs without assigning meanings.

use crate::{FrontendExecutable, FrontendExecutableError};

pub const VEHICLE_BEHAVIOR_TABLE_ADDRESS: u32 = 0x0053_0158;
pub const VEHICLE_BEHAVIOR_RECORDS: usize = 16;
pub const VEHICLE_BEHAVIOR_RECORD_DWORDS: usize = 11;
pub const VEHICLE_BEHAVIOR_TABLE_DWORDS: usize =
    VEHICLE_BEHAVIOR_RECORDS * VEHICLE_BEHAVIOR_RECORD_DWORDS;

pub fn parse_vehicle_behavior_table(
    source: FrontendExecutable<'_>,
) -> Result<[u32; VEHICLE_BEHAVIOR_TABLE_DWORDS], FrontendExecutableError> {
    let range = source.mapped(
        VEHICLE_BEHAVIOR_TABLE_ADDRESS,
        VEHICLE_BEHAVIOR_TABLE_DWORDS * 4,
    )?;
    let mut table = [0; VEHICLE_BEHAVIOR_TABLE_DWORDS];
    for (word, chunk) in table.iter_mut().zip(range.bytes.chunks_exact(4)) {
        *word = u32::from_le_bytes(chunk.try_into().expect("four-byte DWORD"));
    }
    Ok(table)
}

/// 0046B110 clamps player +150..+18C against these sixteen signed limits.
pub const VEHICLE_UPGRADE_LIMITS_ADDRESS: u32 = 0x0053_a310;

pub fn parse_vehicle_upgrade_limits(
    source: FrontendExecutable<'_>,
) -> Result<[i32; 16], FrontendExecutableError> {
    let range = source.mapped(VEHICLE_UPGRADE_LIMITS_ADDRESS, 16 * 4)?;
    let mut limits = [0; 16];
    for (limit, chunk) in limits.iter_mut().zip(range.bytes.chunks_exact(4)) {
        *limit = i32::from_le_bytes(chunk.try_into().expect("four-byte DWORD"));
    }
    Ok(limits)
}
