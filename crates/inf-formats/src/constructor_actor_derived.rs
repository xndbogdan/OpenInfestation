//! The common 00426C10 extension. Later class-specific data at +58 remains open.

use serde::Serialize;

use super::{ConstructorAssetError, ConstructorField};
use crate::{ByteSpan, ConstructorResource};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DerivedActorConstructorPayload {
    /// Payload +42..+58, including explicit raw gaps; not a complete class record.
    pub span: ByteSpan,
    pub consumer: u32,
    /// Unread +42..+44 and +45..+48 remain original source bytes.
    pub raw_gaps: [ByteSpan; 2],
    /// Copied to object +13D; later 00426C80 switches on its low seven bits.
    pub byte_44: ConstructorField<u8>,
    /// Nonzero sets bit 7 in object +13D. Preserve the complete input WORD.
    pub word_48: ConstructorField<u16>,
    /// MOVSX WORD into object +164; unlike the unsigned H14 code-17 override.
    pub signed_word_4a: ConstructorField<i16>,
    /// Copied to object +174; later consumers compare against 1 and 2.
    pub word_4c: ConstructorField<u32>,
    /// Authored +50,+52,+54,+56 WORDs, in that order. No lookup is executed.
    pub sound_bindings: [ConstructorSoundBinding; 4],
}

/// A sound argument written by 00426C10 and read by the named object consumer.
/// The handler table at 0053AA14 identifies 0055EE98 as the H9 sound pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorSoundBinding {
    pub raw: ConstructorField<u16>,
    pub signed_slot: i16,
    /// Strictly positive argument eligible for lookup. These callers skip zero;
    /// signed negatives skip the inline or 00405540 lookup. Capacity, residency,
    /// lazy loading and runtime branch execution are not determined here.
    pub lookup_slot: Option<u16>,
    pub object_offset: u16,
    pub consumer: u32,
}

pub(super) fn has_derived_actor(initializer: u32) -> bool {
    matches!(
        initializer,
        0x0042_39c0
            | 0x0042_5270
            | 0x0042_6bf0
            | 0x0043_3c70
            | 0x0043_d250
            | 0x0046_8310
            | 0x0048_2480
    )
}

pub(super) fn parse_derived_actor(
    main: &[u8],
    main_span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<DerivedActorConstructorPayload, ConstructorAssetError> {
    let offset = entry.payload_offset as usize;
    let bytes = offset
        .checked_add(0x58)
        .and_then(|end| main.get(offset..end))
        .ok_or(ConstructorAssetError::Truncated {
            local_index: entry.local_index,
            selector: entry.constructor_selector,
            offset: entry.payload_offset,
            length: 0x58,
            allocation_bytes: main.len(),
        })?;
    let start = main_span.start + offset;
    let word = |at| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 2),
        value: u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()),
    };
    let signed_word = word(0x4a);
    Ok(DerivedActorConstructorPayload {
        span: ByteSpan::new(start + 0x42, start + 0x58),
        consumer: 0x0042_6c10,
        raw_gaps: [
            ByteSpan::new(start + 0x42, start + 0x44),
            ByteSpan::new(start + 0x45, start + 0x48),
        ],
        byte_44: ConstructorField {
            span: ByteSpan::new(start + 0x44, start + 0x45),
            value: bytes[0x44],
        },
        word_48: word(0x48),
        signed_word_4a: ConstructorField {
            span: signed_word.span,
            value: signed_word.value as i16,
        },
        word_4c: ConstructorField {
            span: ByteSpan::new(start + 0x4c, start + 0x50),
            value: u32::from_le_bytes(bytes[0x4c..0x50].try_into().unwrap()),
        },
        sound_bindings: std::array::from_fn(|index| {
            let raw = word(0x50 + index * 2);
            let signed_slot = raw.value as i16;
            ConstructorSoundBinding {
                raw,
                signed_slot,
                lookup_slot: (signed_slot > 0).then_some(raw.value),
                object_offset: 0x17c + index as u16 * 2,
                consumer: if index == 3 { 0x0042_4280 } else { 0x0042_94a0 },
            }
        }),
    })
}
