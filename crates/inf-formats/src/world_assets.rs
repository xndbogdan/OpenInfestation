//! Bounded Handler-13 constructor prefixes and Handler-14 world placement graphs.
//!
//! `00460310`/`00460120` establish the common constructor prefix;
//! `0048DC30`/`0048E170` establish world placement lookup and its fixed prefix.
//! Class-specific suffixes remain allocation-relative byte addresses, never
//! guessed record lengths. All spans refer to the original OVL byte source.

use std::collections::BTreeSet;

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion, HANDLER_13, HANDLER_14};

pub const CONSTRUCTOR_PREFIX_BYTES: usize = 4;
pub const WORLD_HEADER_BYTES: usize = 0x3c;
pub const WORLD_PLACEMENT_PREFIX_BYTES: usize = 0x10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AssetRelocation {
    /// Word in the relocation table identifying the pointer field.
    pub table_span: ByteSpan,
    pub field_offset: u32,
    pub field_span: ByteSpan,
    /// Original allocation-relative word, before the native base addition.
    pub target_offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorBank {
    pub main_payload: ByteSpan,
    pub relocations: Vec<AssetRelocation>,
    pub entries: Vec<ConstructorResource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorResource {
    pub local_index: u32,
    pub entry_pointer_span: ByteSpan,
    pub allocation_offset: u32,
    pub prefix_span: ByteSpan,
    /// Stored WORD +0; not a global resource slot or campaign selection.
    pub constructor_tag: u16,
    /// WORD +0 incremented with 16-bit wrapping, passed to `004603B0`.
    pub constructor_selector: u16,
    /// Unsigned WORD +2 supplied through the `00460120` initialization context.
    pub argument_word: u16,
    /// Start of class-specific data passed to virtual +0C. Extent is unproven.
    pub payload_offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorldBank {
    pub main_payload: ByteSpan,
    pub relocations: Vec<AssetRelocation>,
    pub entries: Vec<WorldResource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorldResource {
    pub local_index: u32,
    pub entry_pointer_span: ByteSpan,
    pub allocation_offset: u32,
    pub header_span: ByteSpan,
    /// Preserve undecoded header bytes without assigning neighboring meanings.
    pub header_words_00_to_34: [u32; 14],
    /// Header +4; queried by `0048EDE0` and loading policy in `0048DC30`.
    pub flags: u32,
    pub placement_pointer_span: ByteSpan,
    pub placements: Vec<WorldPlacement>,
}

impl WorldResource {
    /// Signed H14 bytes +0E/+0F read by 0048E120 for actor constructor
    /// 00425E60. They are endpoints, not a sorted or clamped range.
    pub const fn actor_mask_limits(&self) -> [i8; 2] {
        let bytes = self.header_words_00_to_34[3].to_le_bytes();
        [bytes[2] as i8, bytes[3] as i8]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorldPlacement {
    /// Authored array order, also passed to `00460310` by `0048DC30`.
    pub index: u32,
    pub pointer_span: ByteSpan,
    pub allocation_offset: u32,
    pub prefix_span: ByteSpan,
    /// Full +0 DWORD passed in ECX; the constructor lookup uses its signed low WORD.
    pub resource_argument: u32,
    /// Raw X/Y/Z words at +4/+6/+8. Loading adds terrain height to Y with WORD wrapping.
    /// No signedness, world-unit conversion, or terrain sampling is invented here.
    pub position_words: [u16; 3],
    /// Stored words at +A/+C/+E, before the constructor's ordering adapter.
    pub rotation_words: [u16; 3],
    /// Start of instance-specific data passed in EDX; extent is unproven.
    pub payload_offset: u32,
}

impl WorldPlacement {
    pub const fn constructor_slot(&self) -> i16 {
        self.resource_argument as u16 as i16
    }

    /// Native stack order is source +E, +A, +C, not source order.
    pub const fn constructor_rotation_words(&self) -> [u16; 3] {
        [
            self.rotation_words[2],
            self.rotation_words[0],
            self.rotation_words[1],
        ]
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorldAssetError {
    #[error("world asset parser expected handler {expected}, received {actual}")]
    WrongHandler { expected: u8, actual: u8 },
    #[error("handler {0} requires relocation-tail framing")]
    WrongFraming(u8),
    #[error("{context} span {span:?} is outside the {input_len}-byte source")]
    InvalidSpan {
        context: &'static str,
        span: ByteSpan,
        input_len: usize,
    },
    #[error("{context} has {actual} bytes, expected {expected}")]
    TableSize {
        context: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("{context} at {offset:#x} needs {length} bytes in a {allocation_len}-byte allocation")]
    OutOfBounds {
        context: &'static str,
        offset: u32,
        length: usize,
        allocation_len: usize,
    },
    #[error("relocation field {0:#x} occurs more than once")]
    DuplicateRelocation(u32),
    #[error("placement pointer field {0:#x} is absent from the native relocation table")]
    MissingRelocation(u32),
    #[error("integer overflow while reading {0}")]
    IntegerOverflow(&'static str),
}

pub fn parse_constructor_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<ConstructorBank, WorldAssetError> {
    let allocation = Allocation::parse(source, region, HANDLER_13)?;
    let mut entries = Vec::with_capacity(allocation.entries.len());
    for (local_index, &(allocation_offset, entry_pointer_span)) in
        allocation.entries.iter().enumerate()
    {
        let prefix_span = allocation.range(
            allocation_offset,
            CONSTRUCTOR_PREFIX_BYTES,
            "constructor prefix",
        )?;
        let prefix = prefix_span.get(source).expect("validated source span");
        let constructor_tag = word(prefix, 0);
        entries.push(ConstructorResource {
            local_index: local_index as u32,
            entry_pointer_span,
            allocation_offset,
            prefix_span,
            constructor_tag,
            constructor_selector: constructor_tag.wrapping_add(1),
            argument_word: word(prefix, 2),
            payload_offset: add(
                allocation_offset,
                CONSTRUCTOR_PREFIX_BYTES,
                "constructor payload",
            )?,
        });
    }
    Ok(ConstructorBank {
        main_payload: allocation.main_payload,
        relocations: allocation.relocations,
        entries,
    })
}

pub fn parse_world_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<WorldBank, WorldAssetError> {
    let allocation = Allocation::parse(source, region, HANDLER_14)?;
    let relocation_fields: BTreeSet<_> = allocation
        .relocations
        .iter()
        .map(|r| r.field_offset)
        .collect();
    let mut entries = Vec::with_capacity(allocation.entries.len());
    for (local_index, &(allocation_offset, entry_pointer_span)) in
        allocation.entries.iter().enumerate()
    {
        let header_span =
            allocation.range(allocation_offset, WORLD_HEADER_BYTES, "world header")?;
        let header = header_span.get(source).expect("validated source span");
        let count = dword(header, 0x38);
        let pointer_bytes = table_bytes(count, "placement pointer table")?;
        let pointer_offset = add(
            allocation_offset,
            WORLD_HEADER_BYTES,
            "placement pointer table",
        )?;
        let placement_pointer_span =
            allocation.range(pointer_offset, pointer_bytes, "placement pointer table")?;
        let pointer_words = placement_pointer_span
            .get(source)
            .expect("validated source span");
        let mut placements = Vec::with_capacity(count as usize);
        for index in 0..count {
            let relative = index as usize * 4;
            let field_offset = add(pointer_offset, relative, "placement pointer field")?;
            if !relocation_fields.contains(&field_offset) {
                return Err(WorldAssetError::MissingRelocation(field_offset));
            }
            let target = dword(pointer_words, relative);
            let prefix_span =
                allocation.range(target, WORLD_PLACEMENT_PREFIX_BYTES, "placement prefix")?;
            let prefix = prefix_span.get(source).expect("validated source span");
            placements.push(WorldPlacement {
                index,
                pointer_span: ByteSpan::new(
                    placement_pointer_span.start + relative,
                    placement_pointer_span.start + relative + 4,
                ),
                allocation_offset: target,
                prefix_span,
                resource_argument: dword(prefix, 0),
                position_words: std::array::from_fn(|i| word(prefix, 4 + i * 2)),
                rotation_words: std::array::from_fn(|i| word(prefix, 10 + i * 2)),
                payload_offset: add(target, WORLD_PLACEMENT_PREFIX_BYTES, "placement payload")?,
            });
        }
        entries.push(WorldResource {
            local_index: local_index as u32,
            entry_pointer_span,
            allocation_offset,
            header_span,
            header_words_00_to_34: std::array::from_fn(|i| dword(header, i * 4)),
            flags: dword(header, 4),
            placement_pointer_span,
            placements,
        });
    }
    Ok(WorldBank {
        main_payload: allocation.main_payload,
        relocations: allocation.relocations,
        entries,
    })
}

struct Allocation {
    main_payload: ByteSpan,
    entries: Vec<(u32, ByteSpan)>,
    relocations: Vec<AssetRelocation>,
}

impl Allocation {
    fn parse(source: &[u8], region: &HandlerRegion, expected: u8) -> Result<Self, WorldAssetError> {
        if region.handler.get() != expected {
            return Err(WorldAssetError::WrongHandler {
                expected,
                actual: region.handler.get(),
            });
        }
        let HandlerFraming::RelocationTail(tail) = &region.framing else {
            return Err(WorldAssetError::WrongFraming(expected));
        };
        let main = source_span(source, tail.main_payload, "main payload")?;
        let relocation_bytes = source_span(source, tail.relocation_offsets, "relocation table")?;
        let entry_bytes = source_span(source, tail.entry_offsets, "entry table")?;
        for (bytes, count, context) in [
            (relocation_bytes, tail.relocation_count, "relocation table"),
            (entry_bytes, region.requested_count, "entry table"),
        ] {
            let expected = table_bytes(count, context)?;
            if bytes.len() != expected {
                return Err(WorldAssetError::TableSize {
                    context,
                    expected,
                    actual: bytes.len(),
                });
            }
        }
        let mut allocation = Self {
            main_payload: tail.main_payload,
            entries: Vec::new(),
            relocations: Vec::new(),
        };
        let mut fields = BTreeSet::new();
        for (index, raw) in relocation_bytes.chunks_exact(4).enumerate() {
            let field_offset = dword(raw, 0);
            if !fields.insert(field_offset) {
                return Err(WorldAssetError::DuplicateRelocation(field_offset));
            }
            let field_span = allocation.range(field_offset, 4, "relocation field")?;
            let target_offset = dword(main, field_offset as usize);
            allocation.range(target_offset, 0, "relocation target")?;
            let at = tail.relocation_offsets.start + index * 4;
            allocation.relocations.push(AssetRelocation {
                table_span: ByteSpan::new(at, at + 4),
                field_offset,
                field_span,
                target_offset,
            });
        }
        for (index, raw) in entry_bytes.chunks_exact(4).enumerate() {
            let offset = dword(raw, 0);
            allocation.range(offset, 0, "entry target")?;
            let at = tail.entry_offsets.start + index * 4;
            allocation.entries.push((offset, ByteSpan::new(at, at + 4)));
        }
        Ok(allocation)
    }

    fn range(
        &self,
        offset: u32,
        length: usize,
        context: &'static str,
    ) -> Result<ByteSpan, WorldAssetError> {
        let start = offset as usize;
        let end = start
            .checked_add(length)
            .ok_or(WorldAssetError::IntegerOverflow(context))?;
        if end > self.main_payload.len() {
            return Err(WorldAssetError::OutOfBounds {
                context,
                offset,
                length,
                allocation_len: self.main_payload.len(),
            });
        }
        Ok(ByteSpan::new(
            self.main_payload.start + start,
            self.main_payload.start + end,
        ))
    }
}

fn source_span<'a>(
    source: &'a [u8],
    span: ByteSpan,
    context: &'static str,
) -> Result<&'a [u8], WorldAssetError> {
    span.get(source).ok_or(WorldAssetError::InvalidSpan {
        context,
        span,
        input_len: source.len(),
    })
}

fn table_bytes(count: u32, context: &'static str) -> Result<usize, WorldAssetError> {
    (count as usize)
        .checked_mul(4)
        .ok_or(WorldAssetError::IntegerOverflow(context))
}

fn add(offset: u32, length: usize, context: &'static str) -> Result<u32, WorldAssetError> {
    u32::try_from(length)
        .ok()
        .and_then(|length| offset.checked_add(length))
        .ok_or(WorldAssetError::IntegerOverflow(context))
}

fn word(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated field"),
    )
}

fn dword(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated field"),
    )
}

#[cfg(test)]
#[path = "world_assets_tests.rs"]
mod tests;
