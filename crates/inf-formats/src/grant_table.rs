//! Player grant table behind 00475320 and the 004750C0 ownership closure.
//!
//! IDs below 0x95 resolve into an 8-byte low table that this decoder does not
//! own; IDs 0x95..0xAE resolve into sixteen-byte records at 0053B088. Each
//! high record carries three unsigned dependency WORDs at +8/+A/+C. Granting an
//! ID sets its bit in a six-DWORD mask and rescans 0x95..=0xAE, granting every
//! still-unowned record with a nonzero already-owned dependency. ID 0 always
//! reads owned. The rescan (but not the bit itself) is skipped for already
//! owned IDs, matching the native early return.
//!
//! Granting also has notification and live-owner effects (004750C0).
//! The player message constructor 00413140 passes its actual receiver to
//! 00475020, which binds grant owner +1C at 00475027 before the initial grants.
//! The closure API owns the resulting bits; native constructor notification
//! and live-owner effects belong to the runtime.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, FrontendExecutable, FrontendExecutableError};

/// Told each `grant_into` call, the recursive ones included, as 004750C0's
/// own calls: the port's event log for the retail scenario lab.
pub static GRANT_OBSERVER: std::sync::OnceLock<fn(u32)> = std::sync::OnceLock::new();

/// Preferred virtual address of the sixteen-byte high records.
pub const GRANT_TABLE_ADDRESS: u32 = 0x0053_B088;
/// First ID served by the high table (00475320).
pub const GRANT_FIRST_ID: u32 = 0x95;
/// High IDs rescanned by 004750C0, 0x95..=0xAE inclusive.
pub const GRANT_RECORD_COUNT: usize = 0x1A;
pub const GRANT_RECORD_BYTES: usize = 16;
/// Six DWORDs at the nested owner (`+0xA0`), covering IDs 0..192.
pub const GRANT_MASK_WORDS: usize = 6;
pub const GRANT_MASK_BITS: u32 = (GRANT_MASK_WORDS as u32) * 32;

/// 00413140's twelve ordered grants, shared by every 00469D60 player
/// construction. Offline save projection consumes the same sequence.
pub const PLAYER_INITIAL_GRANTS: [u32; 12] =
    [1, 0x48, 4, 0x83, 5, 6, 0x76, 0xD, 10, 0x7A, 0x7C, 0x4E];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GrantRecord {
    pub id: u32,
    pub file_span: ByteSpan,
    /// Raw sixteen bytes. Only the dependency WORDs below are interpreted;
    /// the remaining header bytes stay visible for the notification owner.
    pub raw: [u8; GRANT_RECORD_BYTES],
    /// Unsigned dependency WORDs at +8/+A/+C. Zero means no dependency; a
    /// nonzero dependency grants this record once that ID is owned.
    /// 004750C0 loads AX; 004752E0 masks to unsigned WORD before indexing.
    pub dependencies: [u16; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GrantTable {
    pub virtual_address: u32,
    pub records: Vec<GrantRecord>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GrantTableError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum GrantClosureError {
    #[error("grant ID {0} is outside the six-DWORD ownership mask")]
    Id(u32),
}

/// Sixteen-byte high records at 0053B088 in the supported executable.
pub fn parse_grant_table(source: FrontendExecutable<'_>) -> Result<GrantTable, GrantTableError> {
    let range = source.mapped(GRANT_TABLE_ADDRESS, GRANT_RECORD_COUNT * GRANT_RECORD_BYTES)?;
    let mut records = Vec::with_capacity(GRANT_RECORD_COUNT);
    for (index, chunk) in range.bytes.chunks_exact(GRANT_RECORD_BYTES).enumerate() {
        let raw: [u8; GRANT_RECORD_BYTES] =
            chunk.try_into().expect("exact sixteen-byte grant record");
        let word = |at: usize| u16::from_le_bytes([raw[at], raw[at + 1]]);
        records.push(GrantRecord {
            id: GRANT_FIRST_ID + index as u32,
            file_span: ByteSpan::new(
                range.file_span.start + index * GRANT_RECORD_BYTES,
                range.file_span.start + (index + 1) * GRANT_RECORD_BYTES,
            ),
            raw,
            dependencies: [word(8), word(10), word(12)],
        });
    }
    Ok(GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records,
    })
}

/// 004752E0 ownership read over a six-DWORD mask. ID 0 always reads owned;
/// IDs outside the mask are an explicit error instead of an out-of-bounds
/// native access.
pub fn grant_is_owned(mask: &[u32; GRANT_MASK_WORDS], id: u32) -> Result<bool, GrantClosureError> {
    if id == 0 {
        return Ok(true);
    }
    if id >= GRANT_MASK_BITS {
        return Err(GrantClosureError::Id(id));
    }
    Ok(mask[(id >> 5) as usize] & (1 << (id & 31)) != 0)
}

/// 004750C0 over a parsed high table: grant each initial ID in order,
/// rescanning 0x95..=0xAE after every newly set bit.
pub fn grant_closure(
    table: &GrantTable,
    initial: &[u32],
) -> Result<[u32; GRANT_MASK_WORDS], GrantClosureError> {
    let mut mask = [0u32; GRANT_MASK_WORDS];
    for &id in initial {
        grant_into(&mut mask, table, id)?;
    }
    Ok(mask)
}

/// Ownership-bit portion of 004750C0 applied to an existing mask. IDs are
/// normalized unsigned values in 0..192; this API does not narrow arbitrary
/// DWORDs to the native WORD argument. Newly owned IDs immediately recurse
/// through the ascending high-record scan; existing IDs, including special
/// ID0, return before reading the table. Invalid dependencies stop at their
/// native read frontier, preserving earlier bit writes. Notification and
/// live-owner effects are outside this pure bit operation.
pub fn grant_into(
    mask: &mut [u32; GRANT_MASK_WORDS],
    table: &GrantTable,
    id: u32,
) -> Result<(), GrantClosureError> {
    if let Some(observe) = GRANT_OBSERVER.get() {
        observe(id);
    }
    if grant_is_owned(mask, id)? {
        return Ok(());
    }
    mask[(id >> 5) as usize] |= 1 << (id & 31);
    for candidate in GRANT_FIRST_ID..GRANT_FIRST_ID + GRANT_RECORD_COUNT as u32 {
        if grant_is_owned(mask, candidate)? {
            continue;
        }
        let record = &table.records[(candidate - GRANT_FIRST_ID) as usize];
        for &dependency in &record.dependencies {
            if dependency == 0 {
                continue;
            }
            if grant_is_owned(mask, u32::from(dependency))? {
                grant_into(mask, table, candidate)?;
                break;
            }
        }
    }
    Ok(())
}

/// 00475350 queries source IDs 0..=0xAE in ascending order, granting each owned
/// ID into the existing target through 004750C0. This executes dependency
/// closure after each newly granted bit. Source bits above 0xAE are not copied;
/// all pre-existing target bits remain intact, including those higher bits.
pub fn merge_grants(
    target: &mut [u32; GRANT_MASK_WORDS],
    table: &GrantTable,
    source: &[u32; GRANT_MASK_WORDS],
) -> Result<(), GrantClosureError> {
    for id in 0..GRANT_FIRST_ID + GRANT_RECORD_COUNT as u32 {
        if grant_is_owned(source, id)? {
            grant_into(target, table, id)?;
        }
    }
    Ok(())
}

/// 00475320's eight-byte records for IDs below 0x95.
pub const GRANT_LOW_TABLE_ADDRESS: u32 = 0x0053_abd8;
/// IDs 0..=0xAE resolve through 00475320 without leaving either table.
pub const GRANT_TEXT_IDS: usize = GRANT_FIRST_ID as usize + GRANT_RECORD_COUNT;

/// Each ID's record WORD +0: the text-resource index that 004750C0 and
/// 0046B2B0 use for pickup messages (zero means no message).
pub fn parse_grant_texts(
    source: FrontendExecutable<'_>,
) -> Result<[u16; GRANT_TEXT_IDS], GrantTableError> {
    parse_grant_words(source, 0)
}

/// Each ID's record WORD +2: the model the factory shop's preview
/// (`0042FD70`) turns, signed there (a negative one draws nothing).
pub fn parse_grant_models(
    source: FrontendExecutable<'_>,
) -> Result<[u16; GRANT_TEXT_IDS], GrantTableError> {
    parse_grant_words(source, 2)
}

fn parse_grant_words(
    source: FrontendExecutable<'_>,
    offset: usize,
) -> Result<[u16; GRANT_TEXT_IDS], GrantTableError> {
    let low = source.mapped(GRANT_LOW_TABLE_ADDRESS, GRANT_FIRST_ID as usize * 8)?;
    let high = source.mapped(GRANT_TABLE_ADDRESS, GRANT_RECORD_COUNT * GRANT_RECORD_BYTES)?;
    let mut words = [0; GRANT_TEXT_IDS];
    for (id, word) in words.iter_mut().enumerate() {
        let (bytes, at) = if id < GRANT_FIRST_ID as usize {
            (low.bytes, id * 8 + offset)
        } else {
            (
                high.bytes,
                (id - GRANT_FIRST_ID as usize) * GRANT_RECORD_BYTES + offset,
            )
        };
        *word = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    }
    Ok(words)
}

/// Each ID's record DWORD +4, which 004750C0 tests for its notice: bit 0
/// sends the text alone, bit 3 selects category text 0x13B, else 0x13C.
pub fn parse_grant_flags(
    source: FrontendExecutable<'_>,
) -> Result<[u32; GRANT_TEXT_IDS], GrantTableError> {
    let low = source.mapped(GRANT_LOW_TABLE_ADDRESS, GRANT_FIRST_ID as usize * 8)?;
    let high = source.mapped(GRANT_TABLE_ADDRESS, GRANT_RECORD_COUNT * GRANT_RECORD_BYTES)?;
    let mut flags = [0; GRANT_TEXT_IDS];
    for (id, value) in flags.iter_mut().enumerate() {
        let (bytes, at) = if id < GRANT_FIRST_ID as usize {
            (low.bytes, id * 8 + 4)
        } else {
            (
                high.bytes,
                (id - GRANT_FIRST_ID as usize) * GRANT_RECORD_BYTES + 4,
            )
        };
        *value = u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
    }
    Ok(flags)
}

#[cfg(test)]
#[path = "grant_table_tests.rs"]
mod tests;
