//! Keyboard key-name table 00535E54 consumed by 00445840, 00445680 and the
//! key-binding defaults.
//!
//! The table holds one pointer per DirectInput scan code. 00445680 searches
//! exactly 0xDE names; entry 0 and unassigned codes point at empty strings.
//! 00445840 indexes the table with the unbounded low byte of a scan code, so
//! codes 0xDE and above read past it: 0xDE is a NULL word, and later words
//! belong to the adjacent `{name, descriptor}` pair table (odd codes reach
//! action names, even codes descriptor bytes). This decoder owns the 0xDE
//! authored entries only; callers treat higher codes as an explicit boundary.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, FrontendExecutable, FrontendExecutableError};

/// Preferred virtual address of the pointer table.
pub const KEY_NAME_TABLE_ADDRESS: u32 = 0x0053_5E54;
/// Names searched by 00445680: scan codes 0x00..=0xDD.
pub const KEY_NAME_COUNT: usize = 0xDE;
/// Longest accepted name. The retail maximum is twelve bytes (`Keypad Enter`);
/// a missing terminator within this bound is malformed data.
pub const KEY_NAME_MAX_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KeyName {
    pub code: u8,
    /// Pointer word at `00535E54 + code * 4`.
    pub pointer: u32,
    /// Name bytes before the terminator; empty for unassigned codes.
    pub name: Vec<u8>,
    /// File span of the name and its terminator.
    pub file_span: ByteSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KeyNameTable {
    pub virtual_address: u32,
    pub entries: Vec<KeyName>,
}

impl KeyNameTable {
    /// The authored name for `code`, or `None` past the 0xDE-entry table.
    pub fn name(&self, code: u8) -> Option<&[u8]> {
        self.entries
            .get(usize::from(code))
            .map(|entry| entry.name.as_slice())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyNameError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("key name for scan code {code:#04x} has a NULL pointer")]
    Null { code: u8 },
    #[error("key name for scan code {code:#04x} at {pointer:#010x} has no terminator")]
    Unterminated { code: u8, pointer: u32 },
}

pub fn parse_key_name_table(source: FrontendExecutable<'_>) -> Result<KeyNameTable, KeyNameError> {
    let entries = (0..KEY_NAME_COUNT)
        .map(|index| {
            let code = index as u8;
            let pointer = source.word(KEY_NAME_TABLE_ADDRESS + index as u32 * 4)?;
            if pointer == 0 {
                return Err(KeyNameError::Null { code });
            }
            let mut name = Vec::new();
            for offset in 0..KEY_NAME_MAX_BYTES as u32 {
                let byte = source.mapped(pointer + offset, 1)?.bytes[0];
                if byte == 0 {
                    let span = source.mapped(pointer, name.len() + 1)?.file_span;
                    return Ok(KeyName {
                        code,
                        pointer,
                        name,
                        file_span: span,
                    });
                }
                name.push(byte);
            }
            Err(KeyNameError::Unterminated { code, pointer })
        })
        .collect::<Result<_, _>>()?;
    Ok(KeyNameTable {
        virtual_address: KEY_NAME_TABLE_ADDRESS,
        entries,
    })
}

#[cfg(test)]
#[path = "key_names_tests.rs"]
mod tests;
