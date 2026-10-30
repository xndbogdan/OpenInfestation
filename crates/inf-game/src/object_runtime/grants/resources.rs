//! Reached record fields for 004750C0.
//!
//! 00475320 computes an address without touching its backing. The caller reads
//! text, flags and each dependency separately. Canonical H0 acquisition is
//! owned by the shared object_runtime::h0_text phase.

use inf_formats::{
    GRANT_FIRST_ID, GRANT_LOW_TABLE_ADDRESS, GRANT_RECORD_BYTES, GRANT_TABLE_ADDRESS,
    GRANT_TEXT_IDS,
};

use super::{LiveGrantContext, LiveGrantError};

/// 00475320: only address arithmetic, including undocumented adjacent IDs.
pub(super) fn record_address(id: u16) -> u32 {
    let id = u32::from(id);
    if id < GRANT_FIRST_ID {
        GRANT_LOW_TABLE_ADDRESS + id * 8
    } else {
        GRANT_TABLE_ADDRESS + (id - GRANT_FIRST_ID) * GRANT_RECORD_BYTES as u32
    }
}

fn read_span<'a>(
    context: &LiveGrantContext<'a>,
    id: u16,
    address: u32,
    bytes: usize,
) -> Result<&'a [u8], LiveGrantError> {
    let executable = context
        .executable
        .ok_or(LiveGrantError::MissingExecutable { id, address, bytes })?;
    // The original address calculation has already happened. Admission belongs
    // to this reached read, after requiring its actual executable owner.
    if usize::from(id) >= GRANT_TEXT_IDS {
        return Err(LiveGrantError::AdjacentRecord { id, address, bytes });
    }
    executable
        .mapped(address, bytes)
        .map(|span| span.bytes)
        .map_err(|source| LiveGrantError::Executable {
            id,
            address,
            bytes,
            source,
        })
}

/// WORD +0 at 0047512C; the zero branch precedes the flags read.
pub(super) fn read_text(context: &LiveGrantContext<'_>, id: u16) -> Result<u16, LiveGrantError> {
    let bytes = read_span(context, id, record_address(id), 2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Full DWORD +4 at 00475138; its AL bit0 wins over bit3.
pub(super) fn read_flags(context: &LiveGrantContext<'_>, id: u16) -> Result<u32, LiveGrantError> {
    let bytes = read_span(context, id, record_address(id) + 4, 4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// The closure's individual WORD read at 00475295, slots +8/+A/+C.
pub(super) fn read_dependency(
    context: &LiveGrantContext<'_>,
    id: u16,
    slot: usize,
) -> Result<u16, LiveGrantError> {
    // Only the private three-slot closure calls this phase.
    debug_assert!(slot < 3);
    let address = record_address(id) + 8 + (slot as u32) * 2;
    let bytes = read_span(context, id, address, 2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}
