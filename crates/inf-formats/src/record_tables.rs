//! Fixed record tables used by Handlers 5 and 10.
//!
//! These structural parsers preserve raw words and the pointer-relative facts
//! proved by `FUN_0050F340` and `FUN_0050F890`. Consumer-specific Handler-10
//! views live separately in `sample_grids`.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const HANDLER_5: u8 = 5;
pub const HANDLER_5_RECORD_BYTES: usize = 0x14;
pub const HANDLER_10: u8 = 10;
pub const HANDLER_10_RECORD_BYTES: usize = 0x1c;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Handler5Bank {
    pub payload: ByteSpan,
    pub records: Vec<Handler5Record>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Handler5Record {
    pub local_index: u32,
    pub span: ByteSpan,
    pub words: [u32; 5],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Handler10Bank {
    pub payload: ByteSpan,
    pub table: ByteSpan,
    pub trailing_data: ByteSpan,
    pub records: Vec<Handler10Record>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Handler10Record {
    pub local_index: u32,
    pub span: ByteSpan,
    pub words_00_to_0c: [u32; 4],
    /// Allocation-relative offsets fixed up at +10, +14, and +18.
    pub relative_offsets: [u32; 3],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecordTableError {
    #[error("record-table parser expected handler {expected}, received handler {actual}")]
    WrongHandler { expected: u8, actual: u8 },

    #[error("handler {0} requires standard framing")]
    WrongFraming(u8),

    #[error("handler {handler} payload span {span:?} is outside the {input_len}-byte OVL")]
    PayloadOutOfBounds {
        handler: u8,
        span: ByteSpan,
        input_len: usize,
    },

    #[error("handler {handler} payload has {actual} bytes; fixed records require {minimum}")]
    TableTruncated {
        handler: u8,
        minimum: usize,
        actual: usize,
    },

    #[error("handler 5 payload has {actual} bytes; {count} records require exactly {expected}")]
    Handler5SizeMismatch {
        count: u32,
        expected: usize,
        actual: usize,
    },

    #[error("handler 10 record {record} offset field {field} points to {offset:#x}, beyond payload size {payload_len:#x}")]
    Handler10OffsetOutOfBounds {
        record: u32,
        field: usize,
        offset: u32,
        payload_len: usize,
    },

    #[error("integer overflow while sizing handler {handler} with {count} records")]
    IntegerOverflow { handler: u8, count: u32 },
}

pub fn parse_handler5_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<Handler5Bank, RecordTableError> {
    let (payload, bytes) = standard_payload(source, region, HANDLER_5)?;
    let expected = table_bytes(region, HANDLER_5_RECORD_BYTES)?;
    if bytes.len() != expected {
        return Err(RecordTableError::Handler5SizeMismatch {
            count: region.requested_count,
            expected,
            actual: bytes.len(),
        });
    }
    let records = bytes
        .chunks_exact(HANDLER_5_RECORD_BYTES)
        .enumerate()
        .map(|(index, record)| Handler5Record {
            local_index: index as u32,
            span: record_span(payload, index, HANDLER_5_RECORD_BYTES),
            words: std::array::from_fn(|word| u32_at(record, word * 4)),
        })
        .collect();
    Ok(Handler5Bank { payload, records })
}

pub fn parse_handler10_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<Handler10Bank, RecordTableError> {
    let (payload, bytes) = standard_payload(source, region, HANDLER_10)?;
    let table_len = table_bytes(region, HANDLER_10_RECORD_BYTES)?;
    if bytes.len() < table_len {
        return Err(RecordTableError::TableTruncated {
            handler: HANDLER_10,
            minimum: table_len,
            actual: bytes.len(),
        });
    }
    let mut records = Vec::with_capacity(region.requested_count as usize);
    for (index, record) in bytes[..table_len]
        .chunks_exact(HANDLER_10_RECORD_BYTES)
        .enumerate()
    {
        let relative_offsets = std::array::from_fn(|field| u32_at(record, 0x10 + field * 4));
        for (field, &offset) in relative_offsets.iter().enumerate() {
            if offset as usize > bytes.len() {
                return Err(RecordTableError::Handler10OffsetOutOfBounds {
                    record: index as u32,
                    field,
                    offset,
                    payload_len: bytes.len(),
                });
            }
        }
        records.push(Handler10Record {
            local_index: index as u32,
            span: record_span(payload, index, HANDLER_10_RECORD_BYTES),
            words_00_to_0c: std::array::from_fn(|word| u32_at(record, word * 4)),
            relative_offsets,
        });
    }
    let table = ByteSpan::new(payload.start, payload.start + table_len);
    Ok(Handler10Bank {
        payload,
        table,
        trailing_data: ByteSpan::new(table.end, payload.end),
        records,
    })
}

fn standard_payload<'a>(
    source: &'a [u8],
    region: &HandlerRegion,
    expected: u8,
) -> Result<(ByteSpan, &'a [u8]), RecordTableError> {
    if region.handler.get() != expected {
        return Err(RecordTableError::WrongHandler {
            expected,
            actual: region.handler.get(),
        });
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(RecordTableError::WrongFraming(expected));
    };
    let bytes = payload
        .get(source)
        .ok_or(RecordTableError::PayloadOutOfBounds {
            handler: expected,
            span: payload,
            input_len: source.len(),
        })?;
    Ok((payload, bytes))
}

fn table_bytes(region: &HandlerRegion, stride: usize) -> Result<usize, RecordTableError> {
    usize::try_from(region.requested_count)
        .ok()
        .and_then(|count| count.checked_mul(stride))
        .ok_or(RecordTableError::IntegerOverflow {
            handler: region.handler.get(),
            count: region.requested_count,
        })
}

fn record_span(payload: ByteSpan, index: usize, stride: usize) -> ByteSpan {
    ByteSpan::new(
        payload.start + index * stride,
        payload.start + (index + 1) * stride,
    )
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed record"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HandlerId;

    fn region(handler: u8, count: u32, len: usize) -> HandlerRegion {
        HandlerRegion {
            handler: HandlerId::new(handler).unwrap(),
            requested_count: count,
            span: ByteSpan::new(0, len),
            framing: HandlerFraming::Standard {
                payload: ByteSpan::new(0, len),
            },
        }
    }

    #[test]
    fn handler5_preserves_all_five_words() {
        let mut source = vec![0u8; HANDLER_5_RECORD_BYTES];
        for word in 0..5 {
            source[word * 4..word * 4 + 4].copy_from_slice(&(word as u32 + 10).to_le_bytes());
        }
        let bank = parse_handler5_bank(&source, &region(5, 1, source.len())).unwrap();
        assert_eq!(bank.records[0].words, [10, 11, 12, 13, 14]);
    }

    #[test]
    fn handler10_preserves_table_and_relative_payload_offsets() {
        let mut source = vec![0u8; HANDLER_10_RECORD_BYTES + 12];
        for (field, offset) in [28u32, 32, 36].into_iter().enumerate() {
            source[0x10 + field * 4..0x14 + field * 4].copy_from_slice(&offset.to_le_bytes());
        }
        let bank = parse_handler10_bank(&source, &region(10, 1, source.len())).unwrap();
        assert_eq!(bank.records[0].relative_offsets, [28, 32, 36]);
        assert_eq!(bank.trailing_data.len(), 12);
    }
}
