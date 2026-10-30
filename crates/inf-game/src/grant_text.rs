//! Bounded raw-byte grant formatting: 00511B56's fixed `%s` and `%s: %s`
//! callers at 0047517A/00475261. The complete payload must fit the caller's
//! 192-byte temporary with its NUL; the original CRT does not truncate there.
//! This phase owns neither grant mutation, H0 acquisition nor message delivery.

use thiserror::Error;

pub const GRANT_TEXT_MAX_PAYLOAD: usize = 191;

/// A captured H0 result. NULL differs from a nonNULL record whose backing is
/// unavailable. A record is scanned through its first NUL only when reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantTextSource<'a> {
    Null,
    Record(Option<&'a [u8]>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantTextRequest<'a> {
    Single {
        record: GrantTextSource<'a>,
    },
    Pair {
        category: GrantTextSource<'a>,
        record: GrantTextSource<'a>,
    },
}

/// Current semantic binding of 00545900. Neither this binding nor its backing
/// derives a native address from a Rust slice. Startup EXE bytes are one possible
/// supplied initialization, not a substitute for this current owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantTextNullBinding<'a> {
    pub record: Option<&'a [u8]>,
}

/// Current 00545C74 binding, narrowed to the two reached high bytes. Each byte
/// remains independently unavailable; bit80 requires the separate CRT branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantTextPairClassification {
    pub colon_high: Option<u8>,
    pub space_high: Option<u8>,
}

/// Borrowed afresh per call. The fixed native path has no mutable external
/// callback, so captured sources and these bindings stay stable within a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantTextData<'a> {
    pub null_substitute: Option<GrantTextNullBinding<'a>>,
    pub pair_classification: Option<GrantTextPairClassification>,
}

/// The pickup caller's additional literal and its actual 256-byte temporary.
#[derive(Clone, Copy, Debug)]
pub struct PickupTextData<'a> {
    pub common: GrantTextData<'a>,
    pub hyphen_high: Option<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct PickupTextRequest<'a> {
    pub prefix: GrantTextSource<'a>,
    pub name: GrantTextSource<'a>,
    /// Already selected by 0046B2B0's first-byte test, before formatting.
    pub detail: Option<GrantTextSource<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantTextArgument {
    Category,
    Record,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantTextBacking {
    Record,
    NullSubstitute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantTextLiteral {
    Colon,
    Space,
    Hyphen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum GrantTextError {
    #[error("grant text {0:?} record backing is unavailable")]
    MissingRecordBacking(GrantTextArgument),
    #[error("grant text {0:?} requires the current 00545900 binding")]
    MissingNullBinding(GrantTextArgument),
    #[error("grant text {0:?} requires the current NULL-substitute backing")]
    MissingNullBacking(GrantTextArgument),
    #[error("grant text {argument:?} {backing:?} has no reached NUL terminator")]
    Unterminated {
        argument: GrantTextArgument,
        backing: GrantTextBacking,
    },
    #[error("grant text literal {0:?} requires the current 00545C74 binding")]
    MissingClassificationBinding(GrantTextLiteral),
    #[error("grant text literal {0:?} classification high byte is unavailable")]
    MissingClassificationByte(GrantTextLiteral),
    #[error(
        "grant text literal {literal:?} high byte {high_byte:#04x} requires the bit80 CRT branch"
    )]
    ClassificationBoundary {
        literal: GrantTextLiteral,
        high_byte: u8,
    },
    #[error("complete grant text exceeds 191 payload bytes plus NUL")]
    OutputTooLong,
    #[error("complete pickup text exceeds 255 payload bytes plus NUL")]
    PickupOutputTooLong,
    /// A data-supplied format with a conversion other than `%s` or `%%`.
    #[error("warp text format conversion %{0} is not ported")]
    Conversion(char),
    /// A `%s` with no argument passed, or a second `%s` for the one passed.
    #[error("warp text format reads an argument it was not given")]
    MissingArgument,
}

/// Only a complete initialized prefix is published. No caller-buffer tail or
/// partially formatted result is exposed on an admission error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormattedGrantText {
    bytes: Vec<u8>,
}

impl FormattedGrantText {
    pub fn bytes_with_nul(&self) -> &[u8] {
        &self.bytes
    }

    /// The full emitted DWORD count returned by 00511BA7, excluding final NUL.
    pub fn length(&self) -> u32 {
        (self.bytes.len() - 1) as u32
    }
}

/// Execute only the two fixed byte policies. Genuine NULL stays unresolved
/// until that argument's phase. Single nonNULL text requires no CRT data owner.
pub fn format_grant_text<'a>(
    request: GrantTextRequest<'a>,
    data: Option<&GrantTextData<'a>>,
) -> Result<FormattedGrantText, GrantTextError> {
    let mut bytes = Vec::new();
    match request {
        GrantTextRequest::Single { record } => {
            append_argument(&mut bytes, record, GrantTextArgument::Record, data)?;
        }
        GrantTextRequest::Pair { category, record } => {
            append_argument(&mut bytes, category, GrantTextArgument::Category, data)?;
            append_literal(&mut bytes, GrantTextLiteral::Colon, data)?;
            append_literal(&mut bytes, GrantTextLiteral::Space, data)?;
            append_argument(&mut bytes, record, GrantTextArgument::Record, data)?;
        }
    }
    bytes.push(0);
    Ok(FormattedGrantText { bytes })
}

fn append_argument(
    output: &mut Vec<u8>,
    source: GrantTextSource<'_>,
    argument: GrantTextArgument,
    data: Option<&GrantTextData<'_>>,
) -> Result<(), GrantTextError> {
    append_payload(output, argument_bytes(source, argument, data)?)
}

fn argument_bytes<'a>(
    source: GrantTextSource<'a>,
    argument: GrantTextArgument,
    data: Option<&GrantTextData<'a>>,
) -> Result<&'a [u8], GrantTextError> {
    let (bytes, backing) = match source {
        GrantTextSource::Record(bytes) => (
            bytes.ok_or(GrantTextError::MissingRecordBacking(argument))?,
            GrantTextBacking::Record,
        ),
        GrantTextSource::Null => {
            // 005149EF, separately reached for each NULL argument.
            let binding = data
                .and_then(|data| data.null_substitute)
                .ok_or(GrantTextError::MissingNullBinding(argument))?;
            (
                binding
                    .record
                    .ok_or(GrantTextError::MissingNullBacking(argument))?,
                GrantTextBacking::NullSubstitute,
            )
        }
    };
    // 00514A01 scans before the corresponding 00514D3F copy. Bytes after this
    // first NUL are not consumed; no UTF-8 or format interpretation is applied.
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(GrantTextError::Unterminated { argument, backing })?;
    Ok(&bytes[..end])
}

fn append_literal(
    output: &mut Vec<u8>,
    literal: GrantTextLiteral,
    data: Option<&GrantTextData<'_>>,
) -> Result<(), GrantTextError> {
    // 00514737/00514743 are reached separately for ':' and space.
    let binding = data
        .and_then(|data| data.pair_classification)
        .ok_or(GrantTextError::MissingClassificationBinding(literal))?;
    let (byte, high_byte) = match literal {
        GrantTextLiteral::Colon => (b':', binding.colon_high),
        GrantTextLiteral::Space => (b' ', binding.space_high),
        GrantTextLiteral::Hyphen => unreachable!("pickup owns hyphen classification"),
    };
    let high_byte = high_byte.ok_or(GrantTextError::MissingClassificationByte(literal))?;
    if high_byte & 0x80 != 0 {
        return Err(GrantTextError::ClassificationBoundary { literal, high_byte });
    }
    append_payload(output, &[byte])
}

///0046BAxx's fixed `%s %s` / `%s %s - %s` forms. H0 acquisition and
///the detail first-byte branch occur in the enclosing pickup owner.
pub fn format_pickup_text<'a>(
    request: PickupTextRequest<'a>,
    data: Option<&PickupTextData<'a>>,
) -> Result<FormattedGrantText, GrantTextError> {
    let common = data.map(|data| &data.common);
    let mut bytes = Vec::new();
    let append = |bytes: &mut Vec<u8>, part: &[u8]| {
        if bytes
            .len()
            .checked_add(part.len())
            .is_none_or(|len| len > 255)
        {
            return Err(GrantTextError::PickupOutputTooLong);
        }
        bytes.extend_from_slice(part);
        Ok(())
    };
    let literal = |bytes: &mut Vec<u8>, kind: GrantTextLiteral| {
        let (byte, high) = match kind {
            GrantTextLiteral::Space => (
                b' ',
                common
                    .and_then(|data| data.pair_classification)
                    .ok_or(GrantTextError::MissingClassificationBinding(kind))?
                    .space_high,
            ),
            GrantTextLiteral::Hyphen => (
                b'-',
                data.ok_or(GrantTextError::MissingClassificationBinding(kind))?
                    .hyphen_high,
            ),
            GrantTextLiteral::Colon => unreachable!("pickup has no colon literal"),
        };
        let high_byte = high.ok_or(GrantTextError::MissingClassificationByte(kind))?;
        if high_byte & 0x80 != 0 {
            return Err(GrantTextError::ClassificationBoundary {
                literal: kind,
                high_byte,
            });
        }
        append(bytes, &[byte])
    };
    append(
        &mut bytes,
        argument_bytes(request.prefix, GrantTextArgument::Category, common)?,
    )?;
    literal(&mut bytes, GrantTextLiteral::Space)?;
    append(
        &mut bytes,
        argument_bytes(request.name, GrantTextArgument::Record, common)?,
    )?;
    if let Some(detail) = request.detail {
        literal(&mut bytes, GrantTextLiteral::Space)?;
        literal(&mut bytes, GrantTextLiteral::Hyphen)?;
        literal(&mut bytes, GrantTextLiteral::Space)?;
        append(
            &mut bytes,
            argument_bytes(detail, GrantTextArgument::Detail, common)?,
        )?;
    }
    bytes.push(0);
    Ok(FormattedGrantText { bytes })
}

/// 00511B56 with an H0 string as its format: the gate's "Warp to %s"
/// (0048B9B4) with one argument, or "Surface-bound warpgate" (0048B9F6)
/// with none. `%%` prints a percent sign; a NULL argument prints the CRT's
/// NULL substitute. Any other conversion stops.
pub fn format_warp_text<'a>(
    format: GrantTextSource<'a>,
    argument: Option<GrantTextSource<'a>>,
    data: Option<&GrantTextData<'a>>,
) -> Result<FormattedGrantText, GrantTextError> {
    let format = argument_bytes(format, GrantTextArgument::Category, data)?;
    let mut argument = argument;
    let mut bytes = Vec::new();
    let mut rest = format;
    while let Some(index) = rest.iter().position(|byte| *byte == b'%') {
        append_payload(&mut bytes, &rest[..index])?;
        match rest.get(index + 1) {
            Some(b's') => {
                let source = argument.take().ok_or(GrantTextError::MissingArgument)?;
                append_payload(
                    &mut bytes,
                    argument_bytes(source, GrantTextArgument::Record, data)?,
                )?;
            }
            Some(b'%') => append_payload(&mut bytes, b"%")?,
            other => {
                return Err(GrantTextError::Conversion(
                    other.map_or('\0', |byte| char::from(*byte)),
                ))
            }
        }
        rest = &rest[index + 2..];
    }
    append_payload(&mut bytes, rest)?;
    bytes.push(0);
    Ok(FormattedGrantText { bytes })
}

fn append_payload(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), GrantTextError> {
    let length = output
        .len()
        .checked_add(bytes.len())
        .ok_or(GrantTextError::OutputTooLong)?;
    if length > GRANT_TEXT_MAX_PAYLOAD {
        return Err(GrantTextError::OutputTooLong);
    }
    output.extend_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod native_evidence;
#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
