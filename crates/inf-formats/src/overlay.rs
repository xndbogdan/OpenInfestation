use std::fmt;

use serde::Serialize;

use crate::preload::{RequestKind, RequestTable, HANDLER_COUNT};
use crate::reader::{ByteSpan, Reader};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct HandlerId(u8);

impl HandlerId {
    pub const ATLAS: Self = Self(3);
    pub const MODELS: Self = Self(8);
    pub const RELOCATED_13: Self = Self(13);
    pub const RELOCATED_14: Self = Self(14);

    pub const fn new(value: u8) -> Option<Self> {
        if value < HANDLER_COUNT as u8 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for HandlerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct OverlayKey {
    pub level: u8,
    pub variant: OverlayVariant,
}

impl OverlayKey {
    pub fn parse_filename(name: &str) -> Result<Self> {
        let stem = name
            .strip_suffix(".ovl")
            .or_else(|| name.strip_suffix(".OVL"))
            .ok_or_else(|| Error::InvalidOverlayFilename {
                name: name.to_owned(),
            })?;

        let (level, variant) = if let Some(d_pos) = stem.find('d') {
            let level = parse_component(&stem[..d_pos], name)?;
            let rest = &stem[d_pos + 1..];
            if let Some(l_pos) = rest.find('l') {
                let difficulty = parse_component(&rest[..l_pos], name)?;
                let language = parse_component(&rest[l_pos + 1..], name)?;
                (
                    level,
                    OverlayVariant::DifficultyLanguage {
                        difficulty,
                        language,
                    },
                )
            } else {
                (
                    level,
                    OverlayVariant::Difficulty {
                        value: parse_component(rest, name)?,
                    },
                )
            }
        } else if let Some(l_pos) = stem.find('l') {
            (
                parse_component(&stem[..l_pos], name)?,
                OverlayVariant::Language {
                    value: parse_component(&stem[l_pos + 1..], name)?,
                },
            )
        } else {
            (parse_component(stem, name)?, OverlayVariant::Base)
        };

        Ok(Self { level, variant })
    }

    pub const fn request_kind(&self) -> RequestKind {
        self.variant.request_kind()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OverlayVariant {
    Base,
    Difficulty { value: u8 },
    Language { value: u8 },
    DifficultyLanguage { difficulty: u8, language: u8 },
}

impl OverlayVariant {
    pub const fn request_kind(&self) -> RequestKind {
        match self {
            Self::Base => RequestKind::Base,
            Self::Difficulty { .. } => RequestKind::Difficulty,
            Self::Language { .. } => RequestKind::Language,
            Self::DifficultyLanguage { .. } => RequestKind::DifficultyLanguage,
        }
    }
}

fn parse_component(component: &str, whole_name: &str) -> Result<u8> {
    if component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::InvalidOverlayFilename {
            name: whole_name.to_owned(),
        });
    }
    component
        .parse()
        .map_err(|_| Error::InvalidOverlayFilename {
            name: whole_name.to_owned(),
        })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OverlayLayout {
    pub file_size: usize,
    pub regions: Vec<HandlerRegion>,
}

impl OverlayLayout {
    pub fn covered_bytes(&self) -> usize {
        self.regions.last().map_or(0, |region| region.span.end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HandlerRegion {
    pub handler: HandlerId,
    pub requested_count: u32,
    pub span: ByteSpan,
    pub framing: HandlerFraming,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum HandlerFraming {
    Standard {
        payload: ByteSpan,
    },
    SpriteAtlas {
        main_payload: ByteSpan,
        records: Vec<AtlasRecord>,
        terminator: ByteSpan,
    },
    ModelSubBlocks {
        total_entries: u32,
        blocks: Vec<ModelSubBlock>,
    },
    RelocationTail(RelocationTail),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AtlasRecord {
    pub span: ByteSpan,
    pub x_start: u16,
    pub y_start: u16,
    pub x_end: u16,
    pub y_end: u16,
    pub pixels: ByteSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelSubBlock {
    pub span: ByteSpan,
    pub entry_count: u16,
    pub payload: ByteSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RelocationTail {
    pub main_payload: ByteSpan,
    pub relocation_count: u32,
    pub relocation_offsets: ByteSpan,
    pub entry_offsets: ByteSpan,
}

pub fn parse_overlay(data: &[u8], requests: &RequestTable) -> Result<OverlayLayout> {
    let mut reader = Reader::new(data);
    let mut regions = Vec::new();

    for (index, request) in requests.iter() {
        if request.count == 0 {
            continue;
        }
        if request.count < 0 {
            return Err(Error::NegativeHandlerCount {
                handler: index as u8,
                count: request.count,
            });
        }

        let handler = HandlerId::new(index as u8).expect("request table has 16 entries");
        let requested_count = request.count as u32;
        let start = reader.offset();
        let framing = match handler {
            HandlerId::ATLAS => parse_sprite_atlas(&mut reader)?,
            HandlerId::MODELS => parse_model_blocks(&mut reader, requested_count)?,
            HandlerId::RELOCATED_13 | HandlerId::RELOCATED_14 => {
                parse_relocation_tail(&mut reader, requested_count)?
            }
            _ => parse_standard_block(&mut reader)?,
        };
        regions.push(HandlerRegion {
            handler,
            requested_count,
            span: ByteSpan::new(start, reader.offset()),
            framing,
        });
    }

    if reader.remaining() != 0 {
        return Err(Error::TrailingOverlayBytes {
            offset: reader.offset(),
            remaining: reader.remaining(),
        });
    }

    Ok(OverlayLayout {
        file_size: data.len(),
        regions,
    })
}

fn parse_standard_block(reader: &mut Reader<'_>) -> Result<HandlerFraming> {
    let size = reader.u32("standard handler block size")? as usize;
    let payload = reader.span(size, "standard handler block payload")?;
    Ok(HandlerFraming::Standard { payload })
}

fn parse_sprite_atlas(reader: &mut Reader<'_>) -> Result<HandlerFraming> {
    // FUN_0050ee80: one standard main allocation followed by atlas rectangle
    // records and a standalone 0xFFFF terminator.
    let main_size = reader.u32("handler 3 main block size")? as usize;
    let main_payload = reader.span(main_size, "handler 3 main block payload")?;
    let mut records = Vec::new();

    loop {
        let start = reader.offset();
        let x_start = reader.u16("handler 3 atlas x start or terminator")?;
        if x_start == u16::MAX {
            return Ok(HandlerFraming::SpriteAtlas {
                main_payload,
                records,
                terminator: ByteSpan::new(start, reader.offset()),
            });
        }

        let y_start = reader.u16("handler 3 atlas y start")?;
        let x_end = reader.u16("handler 3 atlas x end")?;
        let y_end = reader.u16("handler 3 atlas y end")?;
        if x_end < x_start || y_end < y_start {
            return Err(Error::InvertedAtlasRectangle {
                offset: start,
                x_start,
                y_start,
                x_end,
                y_end,
            });
        }
        let width = usize::from(x_end - x_start);
        let height = usize::from(y_end - y_start);
        let pixel_count = width.checked_mul(height).ok_or(Error::IntegerOverflow {
            offset: start,
            context: "handler 3 atlas pixel count",
        })?;
        let pixels = reader.span(pixel_count, "handler 3 atlas pixels")?;
        records.push(AtlasRecord {
            span: ByteSpan::new(start, reader.offset()),
            x_start,
            y_start,
            x_end,
            y_end,
            pixels,
        });
    }
}

fn parse_model_blocks(reader: &mut Reader<'_>, requested: u32) -> Result<HandlerFraming> {
    // FUN_0050f5a0 accumulates repeated u16-size/u16-count allocations until
    // the PreLoad request count is satisfied.
    let mut total_entries = 0u32;
    let mut blocks = Vec::new();

    while total_entries < requested {
        let start = reader.offset();
        let allocation_size = reader.u16("handler 8 sub-block allocation size")? as usize;
        let entry_count = reader.u16("handler 8 sub-block entry count")?;
        if entry_count == 0 {
            return Err(Error::EmptyModelSubBlock {
                offset: start,
                requested,
            });
        }
        let payload = reader.span(allocation_size, "handler 8 sub-block payload")?;
        total_entries =
            total_entries
                .checked_add(u32::from(entry_count))
                .ok_or(Error::IntegerOverflow {
                    offset: start,
                    context: "handler 8 accumulated entry count",
                })?;
        blocks.push(ModelSubBlock {
            span: ByteSpan::new(start, reader.offset()),
            entry_count,
            payload,
        });
    }

    if total_entries != requested {
        return Err(Error::ModelEntryCountMismatch {
            requested,
            actual: total_entries,
        });
    }
    Ok(HandlerFraming::ModelSubBlocks {
        total_entries,
        blocks,
    })
}

fn parse_relocation_tail(reader: &mut Reader<'_>, requested: u32) -> Result<HandlerFraming> {
    // FUN_004eddb0: main allocation, relocation count/table, then one entry
    // pointer offset for every requested resource.
    let main_size = reader.u32("handler 13/14 main block size")? as usize;
    let main_payload = reader.span(main_size, "handler 13/14 main block payload")?;
    let relocation_count = reader.u32("handler 13/14 relocation count")?;
    let relocation_bytes =
        (relocation_count as usize)
            .checked_mul(4)
            .ok_or(Error::IntegerOverflow {
                offset: reader.offset(),
                context: "handler 13/14 relocation table size",
            })?;
    let relocation_offsets =
        reader.span(relocation_bytes, "handler 13/14 relocation offset table")?;
    let entry_bytes = (requested as usize)
        .checked_mul(4)
        .ok_or(Error::IntegerOverflow {
            offset: reader.offset(),
            context: "handler 13/14 entry offset table size",
        })?;
    let entry_offsets = reader.span(entry_bytes, "handler 13/14 entry offset table")?;

    Ok(HandlerFraming::RelocationTail(RelocationTail {
        main_payload,
        relocation_count,
        relocation_offsets,
        entry_offsets,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preload::{HandlerRequest, ResourceSegment};
    use std::array;

    fn requests(entries: &[(usize, i16)]) -> RequestTable {
        RequestTable(array::from_fn(|index| HandlerRequest {
            count: entries
                .iter()
                .find_map(|(handler, count)| (*handler == index).then_some(*count))
                .unwrap_or(0),
            segments: Vec::<ResourceSegment>::new(),
        }))
    }

    #[test]
    fn parses_all_special_framing_in_dispatch_order() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes()); // h0
        data.push(0xaa);
        data.extend_from_slice(&0u32.to_le_bytes()); // h3 main
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&3u16.to_le_bytes());
        data.extend_from_slice(&3u16.to_le_bytes());
        data.extend_from_slice(&[1, 2]);
        data.extend_from_slice(&u16::MAX.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes()); // h8 block
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&[3, 4]);
        data.extend_from_slice(&4u32.to_le_bytes()); // h13 main
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let layout = parse_overlay(&data, &requests(&[(0, 1), (3, 1), (8, 1), (13, 1)])).unwrap();
        assert_eq!(layout.covered_bytes(), data.len());
        assert_eq!(layout.regions.len(), 4);
        assert!(matches!(
            &layout.regions[1].framing,
            HandlerFraming::SpriteAtlas { records, .. } if records.len() == 1
        ));
        assert!(matches!(
            &layout.regions[2].framing,
            HandlerFraming::ModelSubBlocks { blocks, total_entries: 1 } if blocks.len() == 1
        ));
        assert!(matches!(
            &layout.regions[3].framing,
            HandlerFraming::RelocationTail(tail) if tail.relocation_count == 1
        ));
    }

    #[test]
    fn parses_overlay_filename_variants_without_losing_selection() {
        assert_eq!(
            OverlayKey::parse_filename("12d3l5.ovl").unwrap(),
            OverlayKey {
                level: 12,
                variant: OverlayVariant::DifficultyLanguage {
                    difficulty: 3,
                    language: 5,
                },
            }
        );
        assert_eq!(
            OverlayKey::parse_filename("47.ovl").unwrap().request_kind(),
            RequestKind::Base
        );
        assert!(OverlayKey::parse_filename("12foo.ovl").is_err());
    }

    #[test]
    fn rejects_unframed_trailing_data() {
        let error = parse_overlay(&[0xaa], &requests(&[])).unwrap_err();
        assert_eq!(
            error,
            Error::TrailingOverlayBytes {
                offset: 0,
                remaining: 1,
            }
        );
    }
}
