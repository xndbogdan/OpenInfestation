//! The five row templates and two process-owned layout arrays copied/bound by
//! 00449410. These are not an authored 20-byte menu definition.

use std::{cell::Cell, rc::Rc};

use inf_formats::{
    ByteSpan, FrontendData, FrontendError, FrontendExecutable, FrontendExecutableError,
    FrontendMenuItem, FrontendMenuLayout,
};
use thiserror::Error;

use crate::menu::MenuLayouts;

#[derive(Debug, Error)]
pub enum ProfileTemplateError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Table(#[from] FrontendError),
    #[error("profile layout terminator {address:#010x} has nonzero flags")]
    Terminator { address: u32 },
}

/// Keep one instance in the frontend catalog. Clones share layout animation,
/// while every rebuild copies independent rows from these immutable templates.
#[derive(Clone, Debug)]
pub struct ProfileMenuTemplates {
    pub(crate) regular: MenuLayouts,
    pub(crate) exceptional: MenuLayouts,
    pub(crate) rows: [FrontendMenuItem; 5],
    // 00449410 writes +11/+12 of the exceptional array, inside its zero-flag
    // terminator. Keep these writes without creating a fictitious active row.
    exceptional_terminator: Rc<Cell<[u8; 16]>>,
}

impl ProfileMenuTemplates {
    pub fn parse(executable: &[u8]) -> Result<Self, ProfileTemplateError> {
        Self::decode(FrontendExecutable::parse(executable)?.data())
    }

    fn decode(source: FrontendData<'_>) -> Result<Self, ProfileTemplateError> {
        let regular = layouts(source, 0x0053_6d38, 2)?;
        let exceptional = layouts(source, 0x0053_6d68, 1)?;
        let mut rows = Vec::with_capacity(5);
        for address in [
            0x0053_6d88,
            0x0053_6da8,
            0x0053_6dc8,
            0x0053_6de8,
            0x0053_6e08,
        ] {
            let (span, b) = range(source, address, 32)?;
            let word = |i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            rows.push(FrontendMenuItem {
                virtual_address: address,
                span,
                flags: word(0),
                draw_callback: word(4),
                draw_arguments: [word(8), word(12)],
                action_callback: word(16),
                action_arguments: [word(20), word(24)],
                row_height: u16::from_le_bytes([b[28], b[29]]),
                age: u16::from_le_bytes([b[30], b[31]]),
            });
        }
        let (_, terminator) = range(source, 0x0053_6d78, 16)?;
        Ok(Self {
            regular,
            exceptional,
            rows: rows.try_into().unwrap(),
            exceptional_terminator: Rc::new(Cell::new(terminator.try_into().unwrap())),
        })
    }

    pub(crate) fn bind_exceptional(&self) -> MenuLayouts {
        let mut bytes = self.exceptional_terminator.get();
        bytes[1] = 1;
        bytes[2] = 1;
        self.exceptional_terminator.set(bytes);
        self.exceptional.clone()
    }
}

fn range(
    source: FrontendData<'_>,
    address: u32,
    length: usize,
) -> Result<(ByteSpan, &[u8]), FrontendError> {
    let invalid = || FrontendError::AddressOutOfBounds {
        address,
        bytes: length,
        source_address: source.virtual_address,
        source_len: source.bytes.len(),
    };
    let start = address
        .checked_sub(source.virtual_address)
        .ok_or_else(invalid)? as usize;
    let end = start.checked_add(length).ok_or_else(invalid)?;
    let bytes = source.bytes.get(start..end).ok_or_else(invalid)?;
    Ok((ByteSpan { start, end }, bytes))
}

fn layouts(
    source: FrontendData<'_>,
    address: u32,
    count: usize,
) -> Result<MenuLayouts, ProfileTemplateError> {
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let address = address + index as u32 * 16;
        let (span, b) = range(source, address, 16)?;
        let signed = |i| i16::from_le_bytes([b[i], b[i + 1]]);
        records.push(FrontendMenuLayout {
            virtual_address: address,
            span,
            flags: b[0],
            item_count: b[1],
            visible_rows: b[2],
            reserved: b[3],
            x: signed(4),
            y: signed(6),
            width: signed(8),
            height: signed(10),
            row_spacing: u16::from_le_bytes([b[12], b[13]]),
            scroll_animation: signed(14),
        });
    }
    let terminator_address = address + count as u32 * 16;
    let (terminator, bytes) = range(source, terminator_address, 16)?;
    if bytes[0] != 0 {
        return Err(ProfileTemplateError::Terminator {
            address: terminator_address,
        });
    }
    Ok(MenuLayouts::new(address, terminator, records))
}
