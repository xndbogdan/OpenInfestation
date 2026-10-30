//! Additional reads and retained addresses of concrete actor payload initializers.

use serde::Serialize;

use super::{ConstructorAssetError, ConstructorField};
use crate::{ByteSpan, ConstructorResource};

/// 00468330's retained address, shared by the selector12/13 payload path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Family467070Payload {
    /// Object +23C receives payload+58 without reading the retained bytes.
    pub retained_58_offset: u32,
}

/// 00425290's extension, shared by selector10 and the selector21 subclass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Family425190Payload {
    /// Full DWORD copied to object +268. 004252B0 treats 0..4 as distinct
    /// helper choices and skips allocation for larger values with an unsigned comparison.
    pub dword_74: ConstructorField<u32>,
}

/// 0040D950's reads after 0047AC10, on the 0040D920 and 0042EDD0 wrappers
/// (selectors 15, 23, 37, 39, 47 and 49). Selector 30 has its own wrapper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Family40d950Payload {
    /// Maximum +13C; 0040D950 sign-compares it when clamping current +140.
    pub signed_dword_44: ConstructorField<i32>,
    /// Copied to +146 (DWORD) and +14A (WORD).
    pub words_48_4c: ConstructorField<[u16; 3]>,
}

/// 0042EE00 after 0040D950: the DWORD it copies to +15C.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector15ConstructorPayload {
    pub dword_50: ConstructorField<u32>,
}

/// 0042D780's reads past the shared 0047AC10, on selector 74's initializer
/// 0042D760: the WORD +44 goes to the zone's half height +150, and the
/// DWORDs +48 and +4C, halved toward zero, to its half width +152 (and +28)
/// and half depth +154.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector74ConstructorPayload {
    pub word_44: ConstructorField<u16>,
    pub dword_48: ConstructorField<u32>,
    pub dword_4c: ConstructorField<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector30ConstructorPayload {
    /// 0040D950 sign-compares this value when clamping current +140.
    pub signed_dword_44: ConstructorField<i32>,
    pub words_48_4c: ConstructorField<[u16; 3]>,
    /// 0048B3C0 copies both full DWORDs, unlike its BYTE command overrides.
    pub dword_50: ConstructorField<u32>,
    pub dword_54: ConstructorField<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector34ConstructorPayload {
    /// 00433C00 retains this address at +23C without reading any tail operands.
    /// Later consumers read the live allocation; it may be one past its end here.
    pub retained_58_offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector35ConstructorPayload {
    /// Address retained at +23C; the initializer does not read its WORDs.
    pub retained_58_offset: u32,
    pub dword_6c: ConstructorField<u32>,
    pub dword_70: ConstructorField<u32>,
}

/// Exact extra inputs of 00423850, also reached by 00425290 and 00482460.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Family4236d0Payload {
    /// Stored at object +23C without reading the retained data.
    pub retained_58_offset: u32,
    /// Full DWORD switch selecting the static table at object +25C.
    pub dword_68: ConstructorField<u32>,
    /// Copied to +264; only bit zero is additionally copied to byte +160.
    pub word_6c: ConstructorField<u16>,
    /// Copied unchanged to object +262.
    pub word_6e: ConstructorField<u16>,
    /// Full DWORD switch selecting the static table at object +254.
    pub dword_70: ConstructorField<u32>,
}

fn prefix<'a>(
    main: &'a [u8],
    entry: &ConstructorResource,
    length: usize,
) -> Result<&'a [u8], ConstructorAssetError> {
    let offset = entry.payload_offset as usize;
    offset
        .checked_add(length)
        .and_then(|end| main.get(offset..end))
        .ok_or(ConstructorAssetError::Truncated {
            local_index: entry.local_index,
            selector: entry.constructor_selector,
            offset: entry.payload_offset,
            length,
            allocation_bytes: main.len(),
        })
}

pub(super) fn family425190(
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Family425190Payload, ConstructorAssetError> {
    let bytes = prefix(main, entry, 0x78)?;
    let start = span.start + entry.payload_offset as usize;
    Ok(Family425190Payload {
        dword_74: ConstructorField {
            span: ByteSpan::new(start + 0x74, start + 0x78),
            value: u32::from_le_bytes(bytes[0x74..0x78].try_into().unwrap()),
        },
    })
}

pub(super) fn family467070(
    entry: &ConstructorResource,
) -> Result<Family467070Payload, ConstructorAssetError> {
    Ok(Family467070Payload {
        retained_58_offset: entry.payload_offset.checked_add(0x58).ok_or(
            ConstructorAssetError::OffsetOverflow {
                local_index: entry.local_index,
                offset: entry.payload_offset,
            },
        )?,
    })
}

pub(super) fn selector30(
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Selector30ConstructorPayload, ConstructorAssetError> {
    let bytes = prefix(main, entry, 0x58)?;
    let start = span.start + entry.payload_offset as usize;
    let dword = |at| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 4),
        value: u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
    };
    let raw = dword(0x44);
    Ok(Selector30ConstructorPayload {
        signed_dword_44: ConstructorField {
            span: raw.span,
            value: raw.value as i32,
        },
        words_48_4c: ConstructorField {
            span: ByteSpan::new(start + 0x48, start + 0x4e),
            value: std::array::from_fn(|index| {
                u16::from_le_bytes(
                    bytes[0x48 + index * 2..0x4a + index * 2]
                        .try_into()
                        .unwrap(),
                )
            }),
        },
        dword_50: dword(0x50),
        dword_54: dword(0x54),
    })
}

pub(super) fn selector74(
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Selector74ConstructorPayload, ConstructorAssetError> {
    let bytes = prefix(main, entry, 0x50)?;
    let start = span.start + entry.payload_offset as usize;
    let dword = |at: usize| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 4),
        value: u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
    };
    Ok(Selector74ConstructorPayload {
        word_44: ConstructorField {
            span: ByteSpan::new(start + 0x44, start + 0x46),
            value: u16::from_le_bytes(bytes[0x44..0x46].try_into().unwrap()),
        },
        dword_48: dword(0x48),
        dword_4c: dword(0x4c),
    })
}

pub(super) fn selector15(
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Selector15ConstructorPayload, ConstructorAssetError> {
    let bytes = prefix(main, entry, 0x54)?;
    let start = span.start + entry.payload_offset as usize;
    Ok(Selector15ConstructorPayload {
        dword_50: ConstructorField {
            span: ByteSpan::new(start + 0x50, start + 0x54),
            value: u32::from_le_bytes(bytes[0x50..0x54].try_into().unwrap()),
        },
    })
}

pub(super) fn family40d950(
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Family40d950Payload, ConstructorAssetError> {
    let bytes = prefix(main, entry, 0x4e)?;
    let start = span.start + entry.payload_offset as usize;
    Ok(Family40d950Payload {
        signed_dword_44: ConstructorField {
            span: ByteSpan::new(start + 0x44, start + 0x48),
            value: i32::from_le_bytes(bytes[0x44..0x48].try_into().unwrap()),
        },
        words_48_4c: ConstructorField {
            span: ByteSpan::new(start + 0x48, start + 0x4e),
            value: std::array::from_fn(|index| {
                u16::from_le_bytes(
                    bytes[0x48 + index * 2..0x4a + index * 2]
                        .try_into()
                        .unwrap(),
                )
            }),
        },
    })
}

pub(super) fn selector34(
    entry: &ConstructorResource,
) -> Result<Selector34ConstructorPayload, ConstructorAssetError> {
    Ok(Selector34ConstructorPayload {
        retained_58_offset: entry.payload_offset.checked_add(0x58).ok_or(
            ConstructorAssetError::OffsetOverflow {
                local_index: entry.local_index,
                offset: entry.payload_offset,
            },
        )?,
    })
}

pub(super) fn selector35(
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Selector35ConstructorPayload, ConstructorAssetError> {
    let bytes = prefix(main, entry, 0x74)?;
    let start = span.start + entry.payload_offset as usize;
    let dword = |at| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 4),
        value: u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
    };
    Ok(Selector35ConstructorPayload {
        retained_58_offset: entry.payload_offset.checked_add(0x58).ok_or(
            ConstructorAssetError::OffsetOverflow {
                local_index: entry.local_index,
                offset: entry.payload_offset,
            },
        )?,
        dword_6c: dword(0x6c),
        dword_70: dword(0x70),
    })
}

pub(super) fn family4236d0(
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Family4236d0Payload, ConstructorAssetError> {
    let bytes = prefix(main, entry, 0x74)?;
    let start = span.start + entry.payload_offset as usize;
    let dword = |at| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 4),
        value: u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
    };
    let word = |at| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 2),
        value: u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()),
    };
    Ok(Family4236d0Payload {
        retained_58_offset: entry.payload_offset.checked_add(0x58).ok_or(
            ConstructorAssetError::OffsetOverflow {
                local_index: entry.local_index,
                offset: entry.payload_offset,
            },
        )?,
        dword_68: dword(0x68),
        word_6c: word(0x6c),
        word_6e: word(0x6e),
        dword_70: dword(0x70),
    })
}

/// Reads past the shared 0x42 bytes on the initializers of the classes the
/// behavior registry 0053DBA8 names at +08 (selectors 25..28, 31, 32, 38
/// and 68), and of the Bomb (selector 29) over the same family. Each variant
/// is one initializer's exact read set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum BehaviorResourcePayload {
    /// 00485440, on 00485460 (selectors 31 and 32): WORD +44 to object +146.
    Strength44 { word_44: ConstructorField<u16> },
    /// 004556F0 copies +44..+4D to object +13E..+147 (DWORD, DWORD, WORD);
    /// the tail holds what its caller reads next.
    Packet44 {
        bytes_44: ConstructorField<[u8; 10]>,
        tail: BehaviorResourcePayloadTail,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum BehaviorResourcePayloadTail {
    /// 00455720 (selectors 28 and 68, and the Bomb's 00458CA0, selector 29)
    /// reads nothing more.
    None,
    /// 00455770, on 004557D0 (selectors 26 and 27): BYTE +50 to +14D and
    /// BYTE +54 to +14C, whose bits then adjust +13A and +148.
    Bytes50And54 {
        byte_50: ConstructorField<u8>,
        byte_54: ConstructorField<u8>,
    },
    /// 00456740, on 00456840 (selector 38): 00455770's reads, then DWORD
    /// +58 (only its zero test reaches +154) and DWORD +5C to +158.
    Bytes50And54Dwords58And5c {
        byte_50: ConstructorField<u8>,
        byte_54: ConstructorField<u8>,
        dword_58: ConstructorField<u32>,
        dword_5c: ConstructorField<u32>,
    },
    /// 00458330, on 00458380 (selector 25): WORD +50 to +15A (sign-compared
    /// with 600), then BYTEs +52 and +53, each tested for zero.
    Word50Bytes52And53 {
        word_50: ConstructorField<u16>,
        byte_52: ConstructorField<u8>,
        byte_53: ConstructorField<u8>,
    },
}

/// The read set of a behavior-resource initializer, or None for others.
pub(super) fn behavior_resource(
    initializer: u32,
    main: &[u8],
    span: ByteSpan,
    entry: &ConstructorResource,
) -> Result<Option<BehaviorResourcePayload>, ConstructorAssetError> {
    let length = match initializer {
        0x0048_5460 => 0x46,
        0x0045_5720 | 0x0045_8ca0 => 0x4e,
        0x0045_57d0 => 0x55,
        0x0045_6840 => 0x60,
        0x0045_8380 => 0x54,
        _ => return Ok(None),
    };
    let bytes = prefix(main, entry, length)?;
    let start = span.start + entry.payload_offset as usize;
    let byte = |at: usize| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 1),
        value: bytes[at],
    };
    let word = |at: usize| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 2),
        value: u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()),
    };
    let dword = |at: usize| ConstructorField {
        span: ByteSpan::new(start + at, start + at + 4),
        value: u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
    };
    if initializer == 0x0048_5460 {
        return Ok(Some(BehaviorResourcePayload::Strength44 {
            word_44: word(0x44),
        }));
    }
    let tail = match initializer {
        0x0045_57d0 => BehaviorResourcePayloadTail::Bytes50And54 {
            byte_50: byte(0x50),
            byte_54: byte(0x54),
        },
        0x0045_6840 => BehaviorResourcePayloadTail::Bytes50And54Dwords58And5c {
            byte_50: byte(0x50),
            byte_54: byte(0x54),
            dword_58: dword(0x58),
            dword_5c: dword(0x5c),
        },
        0x0045_8380 => BehaviorResourcePayloadTail::Word50Bytes52And53 {
            word_50: word(0x50),
            byte_52: byte(0x52),
            byte_53: byte(0x53),
        },
        _ => BehaviorResourcePayloadTail::None,
    };
    Ok(Some(BehaviorResourcePayload::Packet44 {
        bytes_44: ConstructorField {
            span: ByteSpan::new(start + 0x44, start + 0x4e),
            value: bytes[0x44..0x4e].try_into().unwrap(),
        },
        tail,
    }))
}
