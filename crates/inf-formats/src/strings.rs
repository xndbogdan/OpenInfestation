//! Handler-0 string resources.
//!
//! `FUN_0050ECD0` registers `count` consecutive NUL-terminated byte strings
//! from one standard allocation. Decoding stays byte-preserving because the
//! executable's active Windows code page is external to the OVL format.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const STRING_HANDLER: u8 = 0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StringBank {
    pub payload: ByteSpan,
    pub entries: Vec<StringEntry>,
    /// Bytes retained by the allocation after the final registered string.
    pub trailing: ByteSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StringEntry {
    pub local_index: u32,
    /// String bytes excluding the terminating NUL.
    pub bytes: ByteSpan,
    /// Complete record including the terminating NUL.
    pub record: ByteSpan,
}

impl StringEntry {
    pub fn raw<'a>(&self, source: &'a [u8]) -> Option<&'a [u8]> {
        self.bytes.get(source)
    }

    /// One Unicode scalar per source byte. This is lossless and deliberately
    /// does not guess which Windows code page was active for a language OVL.
    pub fn byte_preserving_text(&self, source: &[u8]) -> Option<String> {
        self.raw(source)
            .map(|bytes| bytes.iter().map(|byte| char::from(*byte)).collect())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StringError {
    #[error("string parser requires handler 0, received handler {0}")]
    WrongHandler(u8),

    #[error("handler 0 requires standard framing")]
    WrongFraming,

    #[error("handler 0 payload span {span:?} is outside the {input_len}-byte OVL")]
    PayloadOutOfBounds { span: ByteSpan, input_len: usize },

    #[error("handler 0 string {entry} has no NUL terminator before payload end {payload_end:#x}")]
    UnterminatedString { entry: u32, payload_end: usize },
}

pub fn parse_string_bank(source: &[u8], region: &HandlerRegion) -> Result<StringBank, StringError> {
    if region.handler.get() != STRING_HANDLER {
        return Err(StringError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(StringError::WrongFraming);
    };
    let payload_bytes = payload.get(source).ok_or(StringError::PayloadOutOfBounds {
        span: payload,
        input_len: source.len(),
    })?;
    let mut entries = Vec::with_capacity(region.requested_count as usize);
    let mut relative_start = 0usize;
    for local_index in 0..region.requested_count {
        let tail = &payload_bytes[relative_start..];
        let relative_nul =
            tail.iter()
                .position(|byte| *byte == 0)
                .ok_or(StringError::UnterminatedString {
                    entry: local_index,
                    payload_end: payload.end,
                })?;
        let start = payload.start + relative_start;
        let end = start + relative_nul;
        entries.push(StringEntry {
            local_index,
            bytes: ByteSpan::new(start, end),
            record: ByteSpan::new(start, end + 1),
        });
        relative_start += relative_nul + 1;
    }
    Ok(StringBank {
        payload,
        entries,
        trailing: ByteSpan::new(payload.start + relative_start, payload.end),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HandlerId, HandlerRegion};

    #[test]
    fn parses_empty_and_non_ascii_strings_without_guessing_a_code_page() {
        let source = b"hello\0\0caf\xe9\0tail";
        let region = HandlerRegion {
            handler: HandlerId::new(0).unwrap(),
            requested_count: 3,
            span: ByteSpan::new(0, source.len()),
            framing: HandlerFraming::Standard {
                payload: ByteSpan::new(0, source.len()),
            },
        };
        let bank = parse_string_bank(source, &region).unwrap();
        assert_eq!(bank.entries[0].raw(source), Some(&b"hello"[..]));
        assert_eq!(bank.entries[1].raw(source), Some(&b""[..]));
        assert_eq!(
            bank.entries[2].byte_preserving_text(source).unwrap(),
            "caf\u{e9}"
        );
        assert_eq!(bank.trailing, ByteSpan::new(12, 16));
    }

    #[test]
    fn rejects_an_unterminated_registered_string() {
        let source = b"one\0two";
        let region = HandlerRegion {
            handler: HandlerId::new(0).unwrap(),
            requested_count: 2,
            span: ByteSpan::new(0, source.len()),
            framing: HandlerFraming::Standard {
                payload: ByteSpan::new(0, source.len()),
            },
        };
        assert_eq!(
            parse_string_bank(source, &region).unwrap_err(),
            StringError::UnterminatedString {
                entry: 1,
                payload_end: source.len(),
            }
        );
    }
}
