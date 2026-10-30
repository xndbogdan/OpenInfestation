//! Handler-7 four-byte resources and their neutral eight-entry palette shape.
//!
//! `Handler7_Load` (`FUN_0050F490`) registers every requested resource at a
//! four-byte stride. Terrain levels request exactly eight of these values.
//! Runtime terrain sampling selects entries 0 through 3 from each cell's
//! two-bit material and uses entry 4 for the plane/water path. Entries 5
//! through 7 remain deliberately unnamed.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const MATERIAL_HANDLER: u8 = 7;
pub const MATERIAL_ENTRY_BYTES: usize = 4;
pub const TERRAIN_MATERIAL_PALETTE_ENTRIES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MaterialBank {
    pub payload: ByteSpan,
    pub entries: Vec<MaterialValue>,
}

impl MaterialBank {
    /// Expose an exact eight-entry shape without assigning consumer semantics.
    /// Callers may apply terrain roles only after proving same-context H12 use.
    pub fn palette8(&self) -> Option<MaterialPalette8> {
        let entries = self.entries.as_slice().try_into().ok()?;
        Some(MaterialPalette8 { entries })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct MaterialValue(u32);

impl MaterialValue {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Diagnostic RGB555 interpretation of the low word. Runtime consumers
    /// may instead use the full DWORD with live display-format shifts.
    pub const fn rgb555(self) -> Rgb555 {
        Rgb555::from_raw(self.0 as u16)
    }

    /// Preserved because Handler 7 itself copies the complete u32 resource.
    pub const fn high_word(self) -> u16 {
        (self.0 >> 16) as u16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Rgb555(u16);

impl Rgb555 {
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn red5(self) -> u8 {
        ((self.0 >> 10) & 0x1f) as u8
    }

    pub const fn green5(self) -> u8 {
        ((self.0 >> 5) & 0x1f) as u8
    }

    pub const fn blue5(self) -> u8 {
        (self.0 & 0x1f) as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct MaterialPalette8 {
    entries: [MaterialValue; TERRAIN_MATERIAL_PALETTE_ENTRIES],
}

impl MaterialPalette8 {
    pub const fn entries(&self) -> &[MaterialValue; TERRAIN_MATERIAL_PALETTE_ENTRIES] {
        &self.entries
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MaterialError {
    #[error("material parser requires handler 7, received handler {0}")]
    WrongHandler(u8),

    #[error("handler 7 requires standard framing")]
    WrongFraming,

    #[error("handler 7 payload span {span:?} is outside the {input_len}-byte OVL")]
    PayloadOutOfBounds { span: ByteSpan, input_len: usize },

    #[error(
        "handler 7 requested {requested} four-byte entries ({expected_bytes} bytes), payload contains {actual_bytes} bytes"
    )]
    PayloadLengthMismatch {
        requested: u32,
        expected_bytes: usize,
        actual_bytes: usize,
    },

    #[error("integer overflow while sizing {requested} handler 7 entries")]
    IntegerOverflow { requested: u32 },
}

pub fn parse_material_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<MaterialBank, MaterialError> {
    if region.handler.get() != MATERIAL_HANDLER {
        return Err(MaterialError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(MaterialError::WrongFraming);
    };
    let bytes = payload
        .get(source)
        .ok_or(MaterialError::PayloadOutOfBounds {
            span: payload,
            input_len: source.len(),
        })?;
    let entry_count =
        usize::try_from(region.requested_count).map_err(|_| MaterialError::IntegerOverflow {
            requested: region.requested_count,
        })?;
    let expected_bytes =
        entry_count
            .checked_mul(MATERIAL_ENTRY_BYTES)
            .ok_or(MaterialError::IntegerOverflow {
                requested: region.requested_count,
            })?;
    if bytes.len() != expected_bytes {
        return Err(MaterialError::PayloadLengthMismatch {
            requested: region.requested_count,
            expected_bytes,
            actual_bytes: bytes.len(),
        });
    }

    let entries = bytes
        .chunks_exact(MATERIAL_ENTRY_BYTES)
        .map(|entry| {
            MaterialValue::from_raw(u32::from_le_bytes(
                entry.try_into().expect("four-byte material entry"),
            ))
        })
        .collect();
    Ok(MaterialBank { payload, entries })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HandlerId;

    fn region(payload: ByteSpan, requested_count: u32) -> HandlerRegion {
        HandlerRegion {
            handler: HandlerId::new(MATERIAL_HANDLER).unwrap(),
            requested_count,
            span: payload,
            framing: HandlerFraming::Standard { payload },
        }
    }

    #[test]
    fn preserves_u32_entries_and_exposes_a_neutral_eight_entry_shape() {
        let values: [u32; TERRAIN_MATERIAL_PALETTE_ENTRIES] = [
            0xabcd_7c1f,
            0x0000_03e0,
            0x0000_001f,
            0x0000_7fff,
            0x0000_3de7,
            0x1111_0001,
            0x2222_0002,
            0x3333_0003,
        ];
        let mut source = vec![0xaa; 5];
        for value in values {
            source.extend_from_slice(&value.to_le_bytes());
        }
        let payload = ByteSpan::new(5, source.len());

        let bank = parse_material_bank(&source, &region(payload, 8)).unwrap();
        assert_eq!(bank.payload, payload);
        assert_eq!(bank.entries.len(), 8);
        assert_eq!(bank.entries[0].raw(), 0xabcd_7c1f);
        assert_eq!(bank.entries[0].high_word(), 0xabcd);
        assert_eq!(bank.entries[0].rgb555().raw(), 0x7c1f);
        assert_eq!(bank.entries[0].rgb555().red5(), 31);
        assert_eq!(bank.entries[0].rgb555().green5(), 0);
        assert_eq!(bank.entries[0].rgb555().blue5(), 31);

        let palette = bank.palette8().unwrap();
        assert_eq!(
            palette
                .entries()
                .iter()
                .map(|entry| entry.raw())
                .collect::<Vec<_>>(),
            values
        );
    }

    #[test]
    fn parses_non_terrain_banks_without_assigning_terrain_meaning() {
        let source = [1u32.to_le_bytes(), 2u32.to_le_bytes()].concat();
        let bank =
            parse_material_bank(&source, &region(ByteSpan::new(0, source.len()), 2)).unwrap();
        assert_eq!(bank.entries.len(), 2);
        assert!(bank.palette8().is_none());
    }

    #[test]
    fn rejects_payload_lengths_that_do_not_match_the_loader_stride() {
        for source in [vec![0; 31], vec![0; 33]] {
            assert!(matches!(
                parse_material_bank(&source, &region(ByteSpan::new(0, source.len()), 8)),
                Err(MaterialError::PayloadLengthMismatch {
                    expected_bytes: 32,
                    ..
                })
            ));
        }
    }
}
