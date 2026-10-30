//! Handler-14 briefing programs consumed by `0040CE40`.
//!
//! World +28/+2C names a contiguous array of sixteen-byte records. Payload
//! extents below are exactly the fields read by the recovered consumers,
//! rather than allocation lengths inferred from adjacent pointer targets.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, WorldBank, WorldResource};

pub const BRIEFING_RECORD_BYTES: usize = 0x10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BriefingProgram {
    pub record_span: ByteSpan,
    pub records: Vec<BriefingRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BriefingRecord {
    pub index: u32,
    pub span: ByteSpan,
    /// Native +0, multiplied by 100 with DWORD wrapping at `0040CCA8`.
    pub duration_100ms: u32,
    pub raw_kind: u32,
    /// +8 has no established consumer in the recovered briefing owner.
    pub raw_word_08: u32,
    pub payload_offset: u32,
    pub payload_span: ByteSpan,
    pub payload: BriefingPayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum BriefingPayload {
    MenuText {
        string_id: u16,
        /// Either nonzero byte increments the previous ID instead of using
        /// the stored WORD. Keep both bytes; they are separate authored fields.
        increment_bytes: [u8; 2],
    },
    /// Kinds 1/3/4/5 feed the same bit-priority rule to different panels.
    Visibility {
        flags: u32,
    },
    Model(BriefingModel),
    Terrain {
        /// Kind6 writes screen+1D4: nonzero selects "Camera disabled".
        camera_disabled: u32,
    },
    Camera(BriefingCamera),
    /// Kind 9 sets the native lightning age without dereferencing its payload.
    Pulse,
    MapScale {
        word: u16,
    },
    /// No guessed payload extent for an unowned kind.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct BriefingModel {
    /// Signed H8 resource WORD; negative values do not select slot zero.
    pub model_slot: i16,
    /// `0040B840` copies signed WORDs +2/+4 into the model position.
    pub position_xy: [i16; 2],
    /// Added to 600 and the H8 first DWORD to produce model Z.
    pub depth_bias: i16,
    /// Signed WORDs +8/+A shift the panel origin by viewport * word / 1000.
    pub screen_shift_per_thousand: [i16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct BriefingCamera {
    /// Zero disables actor lookup; otherwise matches actor +2A to value - 1.
    pub actor_id: u32,
    /// +4 feeds the degrees-to-native-angle expression if nonzero.
    pub heading_degrees: i16,
    /// +6 feeds the signed native-angle accumulation in `0040CDAC`.
    pub rotation_word: i16,
    /// +8 is read unsigned and multiplied by four in camera movement.
    pub speed_word: u16,
    pub track_byte: u8,
    pub terrain_byte: u8,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BriefingError {
    #[error("briefing {context} span {span:?} is outside the {input_len}-byte source")]
    SourceSpan {
        context: &'static str,
        span: ByteSpan,
        input_len: usize,
    },
    #[error("briefing {context} at {offset:#x} needs {length} bytes in a {allocation_len}-byte allocation")]
    AllocationSpan {
        context: &'static str,
        offset: u32,
        length: usize,
        allocation_len: usize,
    },
    #[error("briefing pointer field {0:#x} is absent from the native relocation table")]
    MissingRelocation(u32),
    #[error("briefing world header has no +28/+2C fields")]
    WorldHeader,
    #[error("integer overflow while reading briefing {0}")]
    IntegerOverflow(&'static str),
}

pub fn parse_briefing_program(
    source: &[u8],
    bank: &WorldBank,
    world: &WorldResource,
) -> Result<BriefingProgram, BriefingError> {
    let allocation = bank
        .main_payload
        .get(source)
        .ok_or(BriefingError::SourceSpan {
            context: "allocation",
            span: bank.main_payload,
            input_len: source.len(),
        })?;
    let header = world
        .header_span
        .get(source)
        .ok_or(BriefingError::SourceSpan {
            context: "world header",
            span: world.header_span,
            input_len: source.len(),
        })?;
    if header.len() < 0x30 {
        return Err(BriefingError::WorldHeader);
    }
    let count = dword(header, 0x28);
    let offset = dword(header, 0x2c);
    // A zero-count intro does not consume the pointer, which need not name
    // any array. Do not create a requirement absent from 0040C6A0's gate.
    if count == 0 {
        return Ok(BriefingProgram {
            record_span: ByteSpan::new(bank.main_payload.start, bank.main_payload.start),
            records: Vec::new(),
        });
    }
    require_relocation(bank, add(world.allocation_offset, 0x2c, "world pointer")?)?;
    let length = (count as usize)
        .checked_mul(BRIEFING_RECORD_BYTES)
        .ok_or(BriefingError::IntegerOverflow("record table"))?;
    let record_span = range(bank, offset, length, "record table")?;
    let records_bytes = record_span.get(source).expect("validated allocation span");
    let mut records = Vec::with_capacity(count as usize);
    for (index, bytes) in records_bytes
        .chunks_exact(BRIEFING_RECORD_BYTES)
        .enumerate()
    {
        let relative = index * BRIEFING_RECORD_BYTES;
        let raw_kind = dword(bytes, 4);
        let payload_offset = dword(bytes, 0xc);
        let length = match raw_kind {
            0 | 1 | 3 | 4 | 5 | 6 => 4,
            2 | 8 => 12,
            10 => 2,
            _ => 0,
        };
        if length != 0 {
            require_relocation(bank, add(offset, relative + 0xc, "payload pointer")?)?;
        }
        let payload_span = if length == 0 {
            ByteSpan::new(bank.main_payload.start, bank.main_payload.start)
        } else {
            range(bank, payload_offset, length, "record payload")?
        };
        let payload_bytes = payload_span.get(source).expect("validated allocation span");
        let payload = match raw_kind {
            0 => BriefingPayload::MenuText {
                string_id: word(payload_bytes, 0),
                increment_bytes: [payload_bytes[2], payload_bytes[3]],
            },
            1 | 3 | 4 | 5 => BriefingPayload::Visibility {
                flags: dword(payload_bytes, 0),
            },
            2 => BriefingPayload::Model(BriefingModel {
                model_slot: word(payload_bytes, 0) as i16,
                position_xy: [word(payload_bytes, 2) as i16, word(payload_bytes, 4) as i16],
                depth_bias: word(payload_bytes, 6) as i16,
                screen_shift_per_thousand: [
                    word(payload_bytes, 8) as i16,
                    word(payload_bytes, 10) as i16,
                ],
            }),
            6 => BriefingPayload::Terrain {
                camera_disabled: dword(payload_bytes, 0),
            },
            8 => BriefingPayload::Camera(BriefingCamera {
                actor_id: dword(payload_bytes, 0),
                heading_degrees: word(payload_bytes, 4) as i16,
                rotation_word: word(payload_bytes, 6) as i16,
                speed_word: word(payload_bytes, 8),
                track_byte: payload_bytes[10],
                terrain_byte: payload_bytes[11],
            }),
            9 => BriefingPayload::Pulse,
            10 => BriefingPayload::MapScale {
                word: word(payload_bytes, 0),
            },
            _ => BriefingPayload::Unknown,
        };
        records.push(BriefingRecord {
            index: index as u32,
            span: ByteSpan::new(
                record_span.start + relative,
                record_span.start + relative + BRIEFING_RECORD_BYTES,
            ),
            duration_100ms: dword(bytes, 0),
            raw_kind,
            raw_word_08: dword(bytes, 8),
            payload_offset,
            payload_span,
            payload,
        });
    }
    debug_assert!(record_span.end <= bank.main_payload.start + allocation.len());
    Ok(BriefingProgram {
        record_span,
        records,
    })
}

fn require_relocation(bank: &WorldBank, field: u32) -> Result<(), BriefingError> {
    if bank
        .relocations
        .iter()
        .any(|item| item.field_offset == field)
    {
        Ok(())
    } else {
        Err(BriefingError::MissingRelocation(field))
    }
}

fn range(
    bank: &WorldBank,
    offset: u32,
    length: usize,
    context: &'static str,
) -> Result<ByteSpan, BriefingError> {
    let end = (offset as usize)
        .checked_add(length)
        .ok_or(BriefingError::IntegerOverflow(context))?;
    if end > bank.main_payload.len() {
        return Err(BriefingError::AllocationSpan {
            context,
            offset,
            length,
            allocation_len: bank.main_payload.len(),
        });
    }
    Ok(ByteSpan::new(
        bank.main_payload.start + offset as usize,
        bank.main_payload.start + end,
    ))
}

fn add(offset: u32, bytes: usize, context: &'static str) -> Result<u32, BriefingError> {
    let bytes = u32::try_from(bytes).map_err(|_| BriefingError::IntegerOverflow(context))?;
    offset
        .checked_add(bytes)
        .ok_or(BriefingError::IntegerOverflow(context))
}

fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("validated field"))
}
fn dword(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("validated field"))
}

#[cfg(test)]
#[path = "briefing_tests.rs"]
mod tests;
