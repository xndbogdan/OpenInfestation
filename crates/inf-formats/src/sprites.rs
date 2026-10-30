//! Handler-3 sprite resources.
//!
//! `FUN_0050EE80` registers one logical sprite entry per PreLoad request. Each
//! entry starts with two render-mode selectors and owns zero or more physical
//! 0x1c-byte variants. Indexed variants point at a pixel-index-major set of 32
//! RGB555 shade colors followed by one base RGB555 color per index. Direct
//! variants store RGB555 words in the byte-addressed 2048-byte atlas.

use std::collections::HashSet;

use serde::Serialize;
use thiserror::Error;

use crate::{AtlasRecord, ByteSpan, HandlerFraming, HandlerRegion};

pub const SPRITE_HANDLER: u8 = 3;
pub const SPRITE_ENTRY_HEADER_BYTES: usize = 4;
pub const SPRITE_VARIANT_BYTES: usize = 0x1c;
pub const SPRITE_ATLAS_STRIDE_BYTES: u32 = 2048;
pub const SPRITE_SHADE_LEVELS: u8 = 32;
const SHADE_RAMP_BYTES_PER_COLOR: usize = SPRITE_SHADE_LEVELS as usize * 2;
const PALETTE_BYTES_PER_COLOR: usize = SHADE_RAMP_BYTES_PER_COLOR + 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpriteBank {
    pub main_payload: ByteSpan,
    pub descriptor_table: ByteSpan,
    pub entries: Vec<SpriteEntry>,
    pub atlas_records: Vec<AtlasRecord>,
    /// Atlas records not referenced by the entries registered by the retail
    /// loader. A conservatively discovered orphan may still point at one.
    pub unreferenced_record_indices: Vec<usize>,
    /// Structurally complete entries stored between the requested descriptor
    /// table and the first registered palette, but ignored by the retail count.
    pub orphan_entries: Vec<SpriteOrphanEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpriteEntry {
    pub local_index: u32,
    pub span: ByteSpan,
    pub selectors: SpriteModeSelectors,
    pub variants: Vec<SpriteVariant>,
}

impl SpriteEntry {
    pub fn variant(&self, mode: SpriteRenderMode) -> Option<&SpriteVariant> {
        self.selectors
            .variant_index(mode)
            .and_then(|index| self.variants.get(index))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpriteOrphanEntry {
    pub span: ByteSpan,
    pub selectors: SpriteModeSelectors,
    pub variants: Vec<SpriteVariant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpriteRenderMode {
    Mode0,
    Mode1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpriteModeSelectors {
    pub mode0: u8,
    pub mode1: u8,
    pub reserved: u16,
}

impl SpriteModeSelectors {
    pub const fn variant_index(self, mode: SpriteRenderMode) -> Option<usize> {
        let selector = match mode {
            SpriteRenderMode::Mode0 => self.mode0,
            SpriteRenderMode::Mode1 => self.mode1,
        };
        if selector == u8::MAX {
            None
        } else {
            Some(selector as usize)
        }
    }

    /// Reproduce `FUN_0050EE80` exactly: mode 0 establishes the initial count,
    /// then any present mode-1 selector replaces it.
    pub const fn physical_variant_count(self) -> usize {
        let mut count = if self.mode0 == u8::MAX {
            0
        } else {
            self.mode0 as usize + 1
        };
        if self.mode1 != u8::MAX {
            count = self.mode1 as usize + 1;
        }
        count
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpriteVariant {
    pub physical_index: u8,
    pub span: ByteSpan,
    pub sprite_id: u16,
    pub reserved: u16,
    pub flags: SpriteFlags,
    pub color_count: u16,
    pub atlas_offset_bytes: u32,
    /// Raw offset relative to the start of the handler-3 main payload. Direct
    /// RGB555 variants retain this field even though the loader ignores it.
    pub palette_offset: u32,
    pub width: u16,
    pub height: u16,
    pub runtime_slots: [u32; 2],
    pub atlas_origin: SpriteAtlasOrigin,
    pub atlas_record_index: usize,
    pub encoding: SpriteEncoding,
}

impl SpriteVariant {
    pub fn decode_rgba(
        &self,
        source: &[u8],
        shade: SpriteShade,
    ) -> Result<Vec<Rgba8>, SpriteError> {
        let expected_pixels = usize::from(self.width)
            .checked_mul(usize::from(self.height))
            .ok_or(SpriteError::IntegerOverflow {
                entry: None,
                context: "decoded sprite pixel count",
            })?;
        let mut decoded = Vec::with_capacity(expected_pixels);

        match &self.encoding {
            SpriteEncoding::Indexed { indices, palette } => {
                let indices = checked_span(source, *indices, "indexed sprite pixels")?;
                for &index in indices {
                    let alpha = if self.flags.is_masked() && index == 0 {
                        0
                    } else {
                        u8::MAX
                    };
                    let color = match shade {
                        SpriteShade::Base => palette.base_rgb555(source, index)?,
                        SpriteShade::Level(level) => palette.shade_rgb555(source, index, level)?,
                    };
                    decoded.push(Rgba8::from_rgb555(color, alpha));
                }
            }
            SpriteEncoding::DirectRgb555 { pixels } => {
                let pixels = checked_span(source, *pixels, "direct RGB555 sprite pixels")?;
                for pair in pixels.chunks_exact(2) {
                    let color = u16::from_le_bytes([pair[0], pair[1]]);
                    let alpha = if self.flags.is_masked() && color == 0 {
                        0
                    } else {
                        u8::MAX
                    };
                    decoded.push(Rgba8::from_rgb555(color, alpha));
                }
            }
        }

        debug_assert_eq!(decoded.len(), expected_pixels);
        Ok(decoded)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpriteFlags(u16);

impl SpriteFlags {
    pub const MASKED: u16 = 0x0001;
    pub const DIRECT_RGB555: u16 = 0x0002;
    pub const ALPHA_BLEND: u16 = 0x0008;
    pub const ADDITIVE_BLEND: u16 = 0x0010;
    pub const DARKEN_BLEND: u16 = 0x0020;
    pub const SHARED_PALETTE: u16 = 0x8000;

    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn is_masked(self) -> bool {
        self.0 & Self::MASKED != 0
    }

    pub const fn is_direct_rgb555(self) -> bool {
        self.0 & Self::DIRECT_RGB555 != 0
    }

    pub const fn uses_alpha_blend(self) -> bool {
        self.0 & Self::ALPHA_BLEND != 0
    }

    pub const fn palette_is_shared(self) -> bool {
        self.0 & Self::SHARED_PALETTE != 0
    }

    pub const fn blend_mode(self) -> SpriteBlendMode {
        if self.0 & Self::ADDITIVE_BLEND != 0 {
            SpriteBlendMode::Additive
        } else if self.0 & Self::DARKEN_BLEND != 0 {
            SpriteBlendMode::DarkenByInverseSourceColor
        } else if self.0 & Self::ALPHA_BLEND != 0 {
            SpriteBlendMode::SourceAlpha
        } else {
            SpriteBlendMode::OpaqueOrMasked
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpriteBlendMode {
    OpaqueOrMasked,
    SourceAlpha,
    Additive,
    DarkenByInverseSourceColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpriteAtlasOrigin {
    pub x_bytes: u16,
    pub y: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpriteEncoding {
    Indexed {
        indices: ByteSpan,
        palette: SpritePalette,
    },
    DirectRgb555 {
        pixels: ByteSpan,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpritePalette {
    pub color_count: u16,
    pub span: ByteSpan,
    pub shade_ramps: ByteSpan,
    pub base_colors: ByteSpan,
}

impl SpritePalette {
    pub fn shade_rgb555(
        self,
        source: &[u8],
        color_index: u8,
        shade_level: u8,
    ) -> Result<u16, SpriteError> {
        if u16::from(color_index) >= self.color_count {
            return Err(SpriteError::PaletteIndexOutOfRange {
                index: u16::from(color_index),
                color_count: self.color_count,
            });
        }
        if shade_level >= SPRITE_SHADE_LEVELS {
            return Err(SpriteError::ShadeLevelOutOfRange { shade_level });
        }
        let word_index = usize::from(color_index)
            .checked_mul(usize::from(SPRITE_SHADE_LEVELS))
            .and_then(|offset| offset.checked_add(usize::from(shade_level)))
            .ok_or(SpriteError::IntegerOverflow {
                entry: None,
                context: "shade-palette word index",
            })?;
        read_span_u16(source, self.shade_ramps, word_index, "sprite shade palette")
    }

    pub fn base_rgb555(self, source: &[u8], color_index: u8) -> Result<u16, SpriteError> {
        if u16::from(color_index) >= self.color_count {
            return Err(SpriteError::PaletteIndexOutOfRange {
                index: u16::from(color_index),
                color_count: self.color_count,
            });
        }
        read_span_u16(
            source,
            self.base_colors,
            usize::from(color_index),
            "sprite base palette",
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpriteShade {
    Base,
    Level(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Rgba8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Rgba8 {
    pub const fn from_rgb555(color: u16, alpha: u8) -> Self {
        let red = ((color >> 10) & 0x1f) as u8;
        let green = ((color >> 5) & 0x1f) as u8;
        let blue = (color & 0x1f) as u8;
        Self {
            red: (red << 3) | (red >> 2),
            green: (green << 3) | (green >> 2),
            blue: (blue << 3) | (blue >> 2),
            alpha,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpriteError {
    #[error("sprite parser requires handler 3, received handler {0}")]
    WrongHandler(u8),

    #[error("handler 3 requires sprite-atlas framing")]
    WrongFraming,

    #[error("handler 3 {context} span {span:?} is outside the {input_len}-byte OVL")]
    SourceSpanOutOfBounds {
        context: &'static str,
        span: ByteSpan,
        input_len: usize,
    },

    #[error(
        "handler 3 entry {entry} is truncated at main-payload byte {offset:#x}: needed {needed}, available {available}"
    )]
    DescriptorTableTruncated {
        entry: u32,
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error("handler 3 entry {entry} has nonzero reserved header value {value:#06x}")]
    NonZeroEntryReserved { entry: u32, value: u16 },

    #[error(
        "handler 3 entry {entry} mode {mode:?} selects variant {selector}, but loader count is {variant_count}"
    )]
    InvalidModeSelector {
        entry: u32,
        mode: SpriteRenderMode,
        selector: u8,
        variant_count: usize,
    },

    #[error("handler 3 entry {entry} variant {variant} has nonzero reserved value {value:#06x}")]
    NonZeroVariantReserved { entry: u32, variant: u8, value: u16 },

    #[error("handler 3 entry {entry} variant {variant} has nonzero runtime slots {slots:#010x?}")]
    NonZeroRuntimeSlots {
        entry: u32,
        variant: u8,
        slots: [u32; 2],
    },

    #[error("handler 3 entry {entry} variant {variant} has zero dimensions {width}x{height}")]
    ZeroDimensions {
        entry: u32,
        variant: u8,
        width: u16,
        height: u16,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} direct RGB555 atlas offset {offset:#x} is not word-aligned"
    )]
    MisalignedDirectAtlasOffset {
        entry: u32,
        variant: u8,
        offset: u32,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} direct RGB555 sprite has color count {color_count}, expected zero"
    )]
    DirectColorCount {
        entry: u32,
        variant: u8,
        color_count: u16,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} indexed sprite has unsupported color count {color_count}"
    )]
    IndexedColorCount {
        entry: u32,
        variant: u8,
        color_count: u16,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} palette span {span:?} is outside main payload {payload:?}"
    )]
    PaletteOutOfBounds {
        entry: u32,
        variant: u8,
        span: ByteSpan,
        payload: ByteSpan,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} atlas origin ({x_bytes},{y}) exceeds 16-bit record coordinates"
    )]
    AtlasOriginOutOfRange {
        entry: u32,
        variant: u8,
        x_bytes: u32,
        y: u32,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} has no atlas record at byte origin ({x_bytes},{y})"
    )]
    AtlasRecordMissing {
        entry: u32,
        variant: u8,
        x_bytes: u16,
        y: u16,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} expects atlas rectangle ({x_start},{y_start})..({x_end},{y_end}), found {actual:?}"
    )]
    AtlasDimensionsMismatch {
        entry: u32,
        variant: u8,
        x_start: u16,
        y_start: u16,
        x_end: u16,
        y_end: u16,
        actual: Vec<[u16; 4]>,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} matches multiple atlas records at ({x_bytes},{y})"
    )]
    AmbiguousAtlasRecord {
        entry: u32,
        variant: u8,
        x_bytes: u16,
        y: u16,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} uses pixel index {index} at pixel {pixel}, but color count is {color_count}"
    )]
    PixelIndexOutOfRange {
        entry: u32,
        variant: u8,
        pixel: usize,
        index: u8,
        color_count: u16,
    },

    #[error(
        "handler 3 entry {entry} variant {variant} marks palette offset {palette_offset:#x}/{color_count} colors as shared without a non-shared owner"
    )]
    SharedPaletteWithoutOwner {
        entry: u32,
        variant: u8,
        palette_offset: u32,
        color_count: u16,
    },

    #[error("sprite palette index {index} is outside color count {color_count}")]
    PaletteIndexOutOfRange { index: u16, color_count: u16 },

    #[error("sprite shade level {shade_level} is outside 0..31")]
    ShadeLevelOutOfRange { shade_level: u8 },

    #[error("integer overflow while computing {context} for handler 3 entry {entry:?}")]
    IntegerOverflow {
        entry: Option<u32>,
        context: &'static str,
    },
}

pub fn parse_sprite_bank(source: &[u8], region: &HandlerRegion) -> Result<SpriteBank, SpriteError> {
    if region.handler.get() != SPRITE_HANDLER {
        return Err(SpriteError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::SpriteAtlas {
        main_payload,
        records,
        ..
    } = &region.framing
    else {
        return Err(SpriteError::WrongFraming);
    };
    let main = checked_span(source, *main_payload, "main payload")?;
    for record in records {
        checked_span(source, record.pixels, "atlas pixels")?;
    }

    let entry_capacity =
        usize::try_from(region.requested_count).map_err(|_| SpriteError::IntegerOverflow {
            entry: Some(region.requested_count),
            context: "sprite entry capacity",
        })?;
    let mut entries = Vec::with_capacity(entry_capacity);
    let mut referenced_records = HashSet::new();
    let mut cursor = 0usize;

    for local_index in 0..region.requested_count {
        let (entry, next) = parse_entry(
            source,
            *main_payload,
            main,
            records,
            cursor,
            local_index,
            &mut referenced_records,
        )?;
        entries.push(entry);
        cursor = next;
    }

    validate_shared_palettes(&entries)?;

    let descriptor_table = ByteSpan::new(
        main_payload.start,
        main_payload
            .start
            .checked_add(cursor)
            .ok_or(SpriteError::IntegerOverflow {
                entry: None,
                context: "descriptor-table end",
            })?,
    );
    let unreferenced_record_indices = (0..records.len())
        .filter(|index| !referenced_records.contains(index))
        .collect::<Vec<_>>();
    let orphan_entries = discover_orphan_entries(
        source,
        *main_payload,
        main,
        records,
        cursor,
        &entries,
        &referenced_records,
    );

    Ok(SpriteBank {
        main_payload: *main_payload,
        descriptor_table,
        entries,
        atlas_records: records.clone(),
        unreferenced_record_indices,
        orphan_entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_entry(
    source: &[u8],
    main_payload: ByteSpan,
    main: &[u8],
    records: &[AtlasRecord],
    start: usize,
    local_index: u32,
    referenced_records: &mut HashSet<usize>,
) -> Result<(SpriteEntry, usize), SpriteError> {
    let header = main.get(start..start + SPRITE_ENTRY_HEADER_BYTES).ok_or(
        SpriteError::DescriptorTableTruncated {
            entry: local_index,
            offset: start,
            needed: SPRITE_ENTRY_HEADER_BYTES,
            available: main.len().saturating_sub(start),
        },
    )?;
    let selectors = SpriteModeSelectors {
        mode0: header[0],
        mode1: header[1],
        reserved: u16::from_le_bytes([header[2], header[3]]),
    };
    if selectors.reserved != 0 {
        return Err(SpriteError::NonZeroEntryReserved {
            entry: local_index,
            value: selectors.reserved,
        });
    }
    let variant_count = selectors.physical_variant_count();
    validate_selectors(local_index, selectors, variant_count)?;

    let variants_bytes =
        variant_count
            .checked_mul(SPRITE_VARIANT_BYTES)
            .ok_or(SpriteError::IntegerOverflow {
                entry: Some(local_index),
                context: "variant table length",
            })?;
    let end = start
        .checked_add(SPRITE_ENTRY_HEADER_BYTES)
        .and_then(|offset| offset.checked_add(variants_bytes))
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(local_index),
            context: "entry end",
        })?;
    if end > main.len() {
        return Err(SpriteError::DescriptorTableTruncated {
            entry: local_index,
            offset: start + SPRITE_ENTRY_HEADER_BYTES,
            needed: variants_bytes,
            available: main.len().saturating_sub(start + SPRITE_ENTRY_HEADER_BYTES),
        });
    }

    let mut variants = Vec::with_capacity(variant_count);
    for physical_index in 0..variant_count {
        let variant_start =
            start + SPRITE_ENTRY_HEADER_BYTES + physical_index * SPRITE_VARIANT_BYTES;
        variants.push(parse_variant(
            source,
            main_payload,
            main,
            records,
            variant_start,
            local_index,
            u8::try_from(physical_index).expect("selector-derived count fits u8 plus one"),
            referenced_records,
        )?);
    }

    let span = absolute_span(main_payload, start, end, local_index, "entry span")?;
    Ok((
        SpriteEntry {
            local_index,
            span,
            selectors,
            variants,
        },
        end,
    ))
}

fn validate_selectors(
    entry: u32,
    selectors: SpriteModeSelectors,
    variant_count: usize,
) -> Result<(), SpriteError> {
    for (mode, selector) in [
        (SpriteRenderMode::Mode0, selectors.mode0),
        (SpriteRenderMode::Mode1, selectors.mode1),
    ] {
        if selector != u8::MAX && usize::from(selector) >= variant_count {
            return Err(SpriteError::InvalidModeSelector {
                entry,
                mode,
                selector,
                variant_count,
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_variant(
    source: &[u8],
    main_payload: ByteSpan,
    main: &[u8],
    records: &[AtlasRecord],
    start: usize,
    entry: u32,
    physical_index: u8,
    referenced_records: &mut HashSet<usize>,
) -> Result<SpriteVariant, SpriteError> {
    let bytes = &main[start..start + SPRITE_VARIANT_BYTES];
    let sprite_id = u16_at(bytes, 0x00);
    let reserved = u16_at(bytes, 0x02);
    let flags = SpriteFlags::new(u16_at(bytes, 0x04));
    let color_count = u16_at(bytes, 0x06);
    let atlas_offset_bytes = u32_at(bytes, 0x08);
    let palette_offset = u32_at(bytes, 0x0c);
    let width = u16_at(bytes, 0x10);
    let height = u16_at(bytes, 0x12);
    let runtime_slots = [u32_at(bytes, 0x14), u32_at(bytes, 0x18)];

    if reserved != 0 {
        return Err(SpriteError::NonZeroVariantReserved {
            entry,
            variant: physical_index,
            value: reserved,
        });
    }
    if runtime_slots != [0, 0] {
        return Err(SpriteError::NonZeroRuntimeSlots {
            entry,
            variant: physical_index,
            slots: runtime_slots,
        });
    }
    if width == 0 || height == 0 {
        return Err(SpriteError::ZeroDimensions {
            entry,
            variant: physical_index,
            width,
            height,
        });
    }

    let x_bytes = atlas_offset_bytes % SPRITE_ATLAS_STRIDE_BYTES;
    let y = atlas_offset_bytes / SPRITE_ATLAS_STRIDE_BYTES;
    let x_bytes_u16 = u16::try_from(x_bytes).map_err(|_| SpriteError::AtlasOriginOutOfRange {
        entry,
        variant: physical_index,
        x_bytes,
        y,
    })?;
    let y_u16 = u16::try_from(y).map_err(|_| SpriteError::AtlasOriginOutOfRange {
        entry,
        variant: physical_index,
        x_bytes,
        y,
    })?;
    let bytes_per_pixel = if flags.is_direct_rgb555() { 2u16 } else { 1u16 };
    let record_width = width
        .checked_mul(bytes_per_pixel)
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(entry),
            context: "atlas record width",
        })?;
    let x_end = x_bytes_u16
        .checked_add(record_width)
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(entry),
            context: "atlas record x end",
        })?;
    let y_end = y_u16
        .checked_add(height)
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(entry),
            context: "atlas record y end",
        })?;
    let atlas_record_index = match_atlas_record(
        records,
        entry,
        physical_index,
        x_bytes_u16,
        y_u16,
        x_end,
        y_end,
    )?;
    let pixels = records[atlas_record_index].pixels;
    referenced_records.insert(atlas_record_index);

    let encoding = if flags.is_direct_rgb555() {
        if atlas_offset_bytes & 1 != 0 {
            return Err(SpriteError::MisalignedDirectAtlasOffset {
                entry,
                variant: physical_index,
                offset: atlas_offset_bytes,
            });
        }
        if color_count != 0 {
            return Err(SpriteError::DirectColorCount {
                entry,
                variant: physical_index,
                color_count,
            });
        }
        SpriteEncoding::DirectRgb555 { pixels }
    } else {
        if !(1..=256).contains(&color_count) {
            return Err(SpriteError::IndexedColorCount {
                entry,
                variant: physical_index,
                color_count,
            });
        }
        let palette = parse_palette(
            main_payload,
            entry,
            physical_index,
            palette_offset,
            color_count,
        )?;
        let indices = checked_span(source, pixels, "indexed sprite pixels")?;
        if let Some((pixel, &index)) = indices
            .iter()
            .enumerate()
            .find(|(_, index)| u16::from(**index) >= color_count)
        {
            return Err(SpriteError::PixelIndexOutOfRange {
                entry,
                variant: physical_index,
                pixel,
                index,
                color_count,
            });
        }
        SpriteEncoding::Indexed {
            indices: pixels,
            palette,
        }
    };

    Ok(SpriteVariant {
        physical_index,
        span: absolute_span(
            main_payload,
            start,
            start + SPRITE_VARIANT_BYTES,
            entry,
            "variant span",
        )?,
        sprite_id,
        reserved,
        flags,
        color_count,
        atlas_offset_bytes,
        palette_offset,
        width,
        height,
        runtime_slots,
        atlas_origin: SpriteAtlasOrigin {
            x_bytes: x_bytes_u16,
            y: y_u16,
        },
        atlas_record_index,
        encoding,
    })
}

fn parse_palette(
    main_payload: ByteSpan,
    entry: u32,
    variant: u8,
    palette_offset: u32,
    color_count: u16,
) -> Result<SpritePalette, SpriteError> {
    let relative_start =
        usize::try_from(palette_offset).map_err(|_| SpriteError::IntegerOverflow {
            entry: Some(entry),
            context: "palette relative offset",
        })?;
    let shade_len = usize::from(color_count)
        .checked_mul(SHADE_RAMP_BYTES_PER_COLOR)
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(entry),
            context: "shade-ramp byte length",
        })?;
    let palette_len = usize::from(color_count)
        .checked_mul(PALETTE_BYTES_PER_COLOR)
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(entry),
            context: "palette byte length",
        })?;
    let relative_end =
        relative_start
            .checked_add(palette_len)
            .ok_or(SpriteError::IntegerOverflow {
                entry: Some(entry),
                context: "palette end",
            })?;
    let span = absolute_span(
        main_payload,
        relative_start,
        relative_end,
        entry,
        "palette span",
    )?;
    if relative_end > main_payload.len() {
        return Err(SpriteError::PaletteOutOfBounds {
            entry,
            variant,
            span,
            payload: main_payload,
        });
    }
    let shade_end = span
        .start
        .checked_add(shade_len)
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(entry),
            context: "shade-ramp end",
        })?;
    Ok(SpritePalette {
        color_count,
        span,
        shade_ramps: ByteSpan::new(span.start, shade_end),
        base_colors: ByteSpan::new(shade_end, span.end),
    })
}

fn match_atlas_record(
    records: &[AtlasRecord],
    entry: u32,
    variant: u8,
    x_start: u16,
    y_start: u16,
    x_end: u16,
    y_end: u16,
) -> Result<usize, SpriteError> {
    let same_origin = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.x_start == x_start && record.y_start == y_start)
        .collect::<Vec<_>>();
    if same_origin.is_empty() {
        return Err(SpriteError::AtlasRecordMissing {
            entry,
            variant,
            x_bytes: x_start,
            y: y_start,
        });
    }
    let exact = same_origin
        .iter()
        .filter(|(_, record)| record.x_end == x_end && record.y_end == y_end)
        .map(|(index, _)| *index)
        .collect::<Vec<_>>();
    match exact.as_slice() {
        [index] => Ok(*index),
        [] => Err(SpriteError::AtlasDimensionsMismatch {
            entry,
            variant,
            x_start,
            y_start,
            x_end,
            y_end,
            actual: same_origin
                .iter()
                .map(|(_, record)| [record.x_start, record.y_start, record.x_end, record.y_end])
                .collect(),
        }),
        _ => Err(SpriteError::AmbiguousAtlasRecord {
            entry,
            variant,
            x_bytes: x_start,
            y: y_start,
        }),
    }
}

fn validate_shared_palettes(entries: &[SpriteEntry]) -> Result<(), SpriteError> {
    let mut owners = HashSet::new();
    for variant in entries.iter().flat_map(|entry| &entry.variants) {
        if matches!(variant.encoding, SpriteEncoding::Indexed { .. })
            && !variant.flags.palette_is_shared()
        {
            owners.insert((variant.palette_offset, variant.color_count));
        }
    }
    for entry in entries {
        for variant in &entry.variants {
            if matches!(variant.encoding, SpriteEncoding::Indexed { .. })
                && variant.flags.palette_is_shared()
                && !owners.contains(&(variant.palette_offset, variant.color_count))
            {
                return Err(SpriteError::SharedPaletteWithoutOwner {
                    entry: entry.local_index,
                    variant: variant.physical_index,
                    palette_offset: variant.palette_offset,
                    color_count: variant.color_count,
                });
            }
        }
    }
    Ok(())
}

fn discover_orphan_entries(
    source: &[u8],
    main_payload: ByteSpan,
    main: &[u8],
    records: &[AtlasRecord],
    table_end: usize,
    entries: &[SpriteEntry],
    registered_records: &HashSet<usize>,
) -> Vec<SpriteOrphanEntry> {
    let Some(first_palette) = entries
        .iter()
        .flat_map(|entry| &entry.variants)
        .filter_map(|variant| match variant.encoding {
            SpriteEncoding::Indexed { .. } => usize::try_from(variant.palette_offset).ok(),
            SpriteEncoding::DirectRgb555 { .. } => None,
        })
        .min()
    else {
        return Vec::new();
    };
    if first_palette <= table_end || first_palette > main.len() {
        return Vec::new();
    }

    let mut cursor = table_end;
    let mut working_records = registered_records.clone();
    let mut discovered = Vec::new();
    while cursor < first_palette {
        let before_records = working_records.clone();
        let Ok((entry, next)) = parse_entry(
            source,
            main_payload,
            main,
            records,
            cursor,
            u32::MAX,
            &mut working_records,
        ) else {
            return Vec::new();
        };
        if entry.variants.is_empty() || next > first_palette {
            return Vec::new();
        }
        if entry
            .variants
            .iter()
            .any(|variant| registered_records.contains(&variant.atlas_record_index))
        {
            return Vec::new();
        }
        if entry.variants.iter().any(|variant| {
            matches!(variant.encoding, SpriteEncoding::Indexed { .. })
                && usize::try_from(variant.palette_offset)
                    .map_or(true, |offset| offset < first_palette)
        }) {
            return Vec::new();
        }
        if working_records == before_records {
            return Vec::new();
        }
        discovered.push(SpriteOrphanEntry {
            span: entry.span,
            selectors: entry.selectors,
            variants: entry.variants,
        });
        cursor = next;
    }
    if cursor == first_palette {
        discovered
    } else {
        Vec::new()
    }
}

fn checked_span<'a>(
    source: &'a [u8],
    span: ByteSpan,
    context: &'static str,
) -> Result<&'a [u8], SpriteError> {
    span.get(source).ok_or(SpriteError::SourceSpanOutOfBounds {
        context,
        span,
        input_len: source.len(),
    })
}

fn absolute_span(
    main_payload: ByteSpan,
    relative_start: usize,
    relative_end: usize,
    entry: u32,
    context: &'static str,
) -> Result<ByteSpan, SpriteError> {
    let start =
        main_payload
            .start
            .checked_add(relative_start)
            .ok_or(SpriteError::IntegerOverflow {
                entry: Some(entry),
                context,
            })?;
    let end = main_payload
        .start
        .checked_add(relative_end)
        .ok_or(SpriteError::IntegerOverflow {
            entry: Some(entry),
            context,
        })?;
    Ok(ByteSpan::new(start, end))
}

fn read_span_u16(
    source: &[u8],
    span: ByteSpan,
    word_index: usize,
    context: &'static str,
) -> Result<u16, SpriteError> {
    let bytes = checked_span(source, span, context)?;
    let offset = word_index
        .checked_mul(2)
        .ok_or(SpriteError::IntegerOverflow {
            entry: None,
            context: "palette byte offset",
        })?;
    let pair = bytes
        .get(offset..offset + 2)
        .ok_or(SpriteError::SourceSpanOutOfBounds {
            context,
            span,
            input_len: source.len(),
        })?;
    Ok(u16::from_le_bytes([pair[0], pair[1]]))
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
