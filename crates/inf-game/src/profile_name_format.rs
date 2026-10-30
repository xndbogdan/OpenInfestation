//! Completed zero-vararg profile-name formatting from 00511B56/00514598.
//!
//! This owns only literal raw bytes and `%%` within the caller's 32-byte
//! buffer. Directive, bit80 and missing-data branches are admission errors;
//! they do not describe native faults or expose native partial output. The
//! current raw FORMAT and intrinsic save-codec cursor remain distinct.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

const CLASSIFICATION_POINTER: u32 = 0x0054_5c74;
const STARTUP_CLASSIFICATION_TABLE: u32 = 0x0054_5c7e;
const MAX_PAYLOAD: usize = 31;

/// Current semantic 00545C74 binding. None is unavailable binding custody;
/// each WORD's high byte remains independently unavailable. No native address
/// is derived from this owner, and its startup seed is not global immutability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProfileNameClassification {
    pub(crate) high_bytes: Option<[Option<u8>; 256]>,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub(crate) enum ProfileNameStartupError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("profile-name startup 00545C74 binding {pointer:#010x} is outside 00545C7E")]
    UnsupportedStartupBinding { pointer: u32 },
}

impl ProfileNameClassification {
    /// Capture original file-backed high bytes once. FrontendExecutable checks
    /// a structural supported profile; it does not authenticate the file hash,
    /// relocate pointers or establish later CRT writer absence.
    pub(crate) fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, ProfileNameStartupError> {
        let pointer = executable.word(CLASSIFICATION_POINTER)?;
        if pointer != STARTUP_CLASSIFICATION_TABLE {
            return Err(ProfileNameStartupError::UnsupportedStartupBinding { pointer });
        }
        let table = executable.mapped(pointer, 512)?.bytes;
        Ok(Self {
            high_bytes: Some(std::array::from_fn(|byte| Some(table[2 * byte + 1]))),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub(crate) enum ProfileNameFormatError {
    #[error("current profile-name FORMAT backing is unavailable")]
    CurrentNameUnavailable,
    #[error("profile-name FORMAT has no reached NUL at offset {offset}")]
    Unterminated { offset: usize },
    #[error("trailing percent at offset {offset} is outside the literal/%% admission domain")]
    TrailingPercentBoundary { offset: usize },
    #[error(
        "profile-name FORMAT byte {byte:#04x} at offset {offset} requires an unowned directive"
    )]
    FormatBoundary { offset: usize, byte: u8 },
    #[error("profile-name byte {byte:#04x} at offset {offset} requires current 00545C74 binding")]
    MissingClassificationBinding { offset: usize, byte: u8 },
    #[error("profile-name byte {byte:#04x} at offset {offset} has unavailable classification")]
    MissingClassificationByte { offset: usize, byte: u8 },
    #[error("profile-name byte {byte:#04x} at offset {offset} high byte {high_byte:#04x} requires the bit80 branch")]
    ClassificationBoundary {
        offset: usize,
        byte: u8,
        high_byte: u8,
    },
    #[error("completed profile-name output exceeds 31 payload bytes plus NUL")]
    OutputTooLong,
}

/// Only a complete initialized result is published. Its count excludes the
/// final NUL, matching the full DWORD return of 00511BA7.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FormattedProfileName {
    bytes: Vec<u8>,
}

impl FormattedProfileName {
    pub(crate) fn bytes_with_nul(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn length(&self) -> u32 {
        (self.bytes.len() - 1) as u32
    }
}

/// Consume FORMAT incrementally through its first NUL, without interpreting
/// UTF-8 or escaping it automatically. Empty output succeeds independently of
/// the outer save record's nonempty-name admission. A lone trailing percent is
/// excluded by this narrower domain; native NUL-before-state dispatch completes
/// that input rather than raising this error.
pub(crate) fn format_profile_name(
    format: Option<&[u8]>,
    classification: &ProfileNameClassification,
) -> Result<FormattedProfileName, ProfileNameFormatError> {
    let format = format.ok_or(ProfileNameFormatError::CurrentNameUnavailable)?;
    let mut output = Vec::new();
    let mut offset = 0;
    loop {
        let mut byte = *format
            .get(offset)
            .ok_or(ProfileNameFormatError::Unterminated { offset })?;
        if byte == 0 {
            output.push(0);
            return Ok(FormattedProfileName { bytes: output });
        }
        if byte == b'%' {
            let percent_offset = offset;
            offset += 1;
            byte = *format
                .get(offset)
                .ok_or(ProfileNameFormatError::Unterminated { offset })?;
            if byte == 0 {
                return Err(ProfileNameFormatError::TrailingPercentBoundary {
                    offset: percent_offset,
                });
            }
            if byte != b'%' {
                return Err(ProfileNameFormatError::FormatBoundary { offset, byte });
            }
        }
        // 00514737/00514743 use unsigned BYTE indexing for each emitted
        // literal. The first '%' changes parser state and does not read ctype.
        let high_bytes = classification
            .high_bytes
            .as_ref()
            .ok_or(ProfileNameFormatError::MissingClassificationBinding { offset, byte })?;
        let high_byte = high_bytes[usize::from(byte)]
            .ok_or(ProfileNameFormatError::MissingClassificationByte { offset, byte })?;
        if high_byte & 0x80 != 0 {
            return Err(ProfileNameFormatError::ClassificationBoundary {
                offset,
                byte,
                high_byte,
            });
        }
        if output.len() == MAX_PAYLOAD {
            return Err(ProfileNameFormatError::OutputTooLong);
        }
        output.push(byte);
        offset += 1;
    }
}

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
