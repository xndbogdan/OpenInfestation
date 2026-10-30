//! Fixed 32-bit resource banks shared by Handlers 1, 2, and 6.
//!
//! `FUN_0050ED90` and `FUN_0050F3F0` register one four-byte record per local
//! resource. Semantics belong to their consumers; this layer preserves the
//! exact word and runtime global identity mapping separately.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const WORD_RESOURCE_BYTES: usize = 4;
pub const WORD_RESOURCE_HANDLERS: [u8; 3] = [1, 2, 6];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WordResourceBank {
    pub handler: u8,
    pub payload: ByteSpan,
    pub entries: Vec<WordResource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct WordResource {
    pub local_index: u32,
    pub span: ByteSpan,
    pub raw: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WordResourceError {
    #[error("32-bit resource parser supports handlers 1, 2, and 6, received handler {0}")]
    WrongHandler(u8),

    #[error("32-bit resource handlers require standard framing")]
    WrongFraming,

    #[error("handler {handler} payload span {span:?} is outside the {input_len}-byte OVL")]
    PayloadOutOfBounds {
        handler: u8,
        span: ByteSpan,
        input_len: usize,
    },

    #[error(
        "handler {handler} payload has {actual} bytes; {count} words require exactly {expected}"
    )]
    PayloadSizeMismatch {
        handler: u8,
        count: u32,
        expected: usize,
        actual: usize,
    },

    #[error("integer overflow while sizing handler {handler} with {count} words")]
    IntegerOverflow { handler: u8, count: u32 },
}

pub fn parse_word_resource_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<WordResourceBank, WordResourceError> {
    let handler = region.handler.get();
    if !WORD_RESOURCE_HANDLERS.contains(&handler) {
        return Err(WordResourceError::WrongHandler(handler));
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(WordResourceError::WrongFraming);
    };
    let bytes = payload
        .get(source)
        .ok_or(WordResourceError::PayloadOutOfBounds {
            handler,
            span: payload,
            input_len: source.len(),
        })?;
    let expected = usize::try_from(region.requested_count)
        .ok()
        .and_then(|count| count.checked_mul(WORD_RESOURCE_BYTES))
        .ok_or(WordResourceError::IntegerOverflow {
            handler,
            count: region.requested_count,
        })?;
    if bytes.len() != expected {
        return Err(WordResourceError::PayloadSizeMismatch {
            handler,
            count: region.requested_count,
            expected,
            actual: bytes.len(),
        });
    }
    let entries = bytes
        .chunks_exact(WORD_RESOURCE_BYTES)
        .enumerate()
        .map(|(index, record)| WordResource {
            local_index: index as u32,
            span: ByteSpan::new(
                payload.start + index * WORD_RESOURCE_BYTES,
                payload.start + (index + 1) * WORD_RESOURCE_BYTES,
            ),
            raw: u32::from_le_bytes(record.try_into().expect("four-byte chunk")),
        })
        .collect();
    Ok(WordResourceBank {
        handler,
        payload,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HandlerId;

    #[test]
    fn parses_each_supported_handler_without_assigning_semantics() {
        for handler in WORD_RESOURCE_HANDLERS {
            let source = [1, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];
            let region = HandlerRegion {
                handler: HandlerId::new(handler).unwrap(),
                requested_count: 2,
                span: ByteSpan::new(0, source.len()),
                framing: HandlerFraming::Standard {
                    payload: ByteSpan::new(0, source.len()),
                },
            };
            let bank = parse_word_resource_bank(&source, &region).unwrap();
            assert_eq!(bank.entries[0].raw, 1);
            assert_eq!(bank.entries[1].raw, u32::MAX);
        }
    }
}
