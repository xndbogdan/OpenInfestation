//! Handler-4 fonts: loader 0050F250, text consumers 004A3B20/004A3E30/004A4240.
//! Original bytes remain in the owning overlay. Runtime pointer fixups at
//! header +38/+3C are represented by bounded spans, never host addresses.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const FONT_HANDLER: u8 = 4;
pub const FONT_HEADER_BYTES: usize = 0x40;
pub const FONT_METRIC_BYTES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FontMode {
    /// Header +04 == 1: one sprite resource word per byte-code.
    Direct,
    /// Header +04 == 2: a position-dependent table of glyph sprite resources.
    PositionGrid { columns: i32, rows: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GlyphMetrics {
    /// Signed native font units. Measurement omits spacing after the last byte;
    /// the drawing walker includes it after every consumed glyph.
    pub advance: i16,
    pub spacing: i16,
    /// Both signed offsets are multiplied by 100 by the text consumer.
    pub offset: [i16; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Font {
    pub local_index: u32,
    pub header_span: ByteSpan,
    /// All sixteen original words, including overwritten pointer placeholders
    /// and fields whose semantics have not been established.
    pub raw_header: [u32; 16],
    pub mode: FontMode,
    pub coordinate_divisor: i32,
    /// +14 and +18, added on each wrapped line. Individual names are not guessed.
    pub line_step_terms: [i32; 2],
    /// +28 is compared as signed i32 against the unsigned source byte.
    pub maximum_byte: i32,
    pub fallback_byte: u8,
    pub metric_span: ByteSpan,
    pub sprite_span: ByteSpan,
    pub metrics: Vec<GlyphMetrics>,
    /// Full callback arguments. 00439DE0 bounds/indexes their signed low WORD
    /// but forwards the complete DWORD to the Handler-3 lazy loader.
    pub sprite_words: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FontBank {
    pub payload: ByteSpan,
    pub header_table: ByteSpan,
    pub fonts: Vec<Font>,
    pub trailing: ByteSpan,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FontError {
    #[error("font parser requires handler 4, received handler {0}")]
    WrongHandler(u8),
    #[error("handler 4 fonts require standard framing")]
    WrongFraming,
    #[error("font payload {span:?} is outside the {input_len}-byte source")]
    PayloadOutOfBounds { span: ByteSpan, input_len: usize },
    #[error("font {font} {field} span {span:?} exceeds payload {payload:?}")]
    Truncated {
        font: u32,
        field: &'static str,
        span: ByteSpan,
        payload: ByteSpan,
    },
    #[error("font {font} has unsupported mode {mode}")]
    UnsupportedMode { font: u32, mode: u32 },
    #[error("font {font} has zero coordinate divisor")]
    ZeroDivisor { font: u32 },
    #[error("font {font} has negative {field}: {value}")]
    NegativeCount {
        font: u32,
        field: &'static str,
        value: i32,
    },
    #[error("integer overflow while computing font {font} {field}")]
    Overflow { font: u32, field: &'static str },
}

pub fn parse_font_bank(source: &[u8], region: &HandlerRegion) -> Result<FontBank, FontError> {
    if region.handler.get() != FONT_HANDLER {
        return Err(FontError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(FontError::WrongFraming);
    };
    payload.get(source).ok_or(FontError::PayloadOutOfBounds {
        span: payload,
        input_len: source.len(),
    })?;
    let header_bytes = (region.requested_count as usize)
        .checked_mul(FONT_HEADER_BYTES)
        .ok_or(FontError::Overflow {
            font: 0,
            field: "header table",
        })?;
    let header_table = bounded_span(payload, payload.start, header_bytes, 0, "header table")?;
    let mut cursor = header_table.end;
    let mut fonts = Vec::with_capacity(region.requested_count as usize);
    for local_index in 0..region.requested_count {
        let header_span = ByteSpan::new(
            payload.start + local_index as usize * FONT_HEADER_BYTES,
            payload.start + (local_index as usize + 1) * FONT_HEADER_BYTES,
        );
        let bytes = header_span.get(source).expect("bounded header table");
        let raw_header = std::array::from_fn(|i| {
            u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
        });
        let count = nonnegative(raw_header[2] as i32, local_index, "glyph count")?;
        let coordinate_divisor = raw_header[4] as i32;
        if coordinate_divisor == 0 {
            return Err(FontError::ZeroDivisor { font: local_index });
        }
        let (mode, planes) = match raw_header[1] {
            1 => (FontMode::Direct, 1usize),
            2 => {
                let columns = raw_header[12] as i32;
                let rows = raw_header[13] as i32;
                let planes = nonnegative(columns, local_index, "grid columns")?
                    .checked_mul(nonnegative(rows, local_index, "grid rows")?)
                    .ok_or(FontError::Overflow {
                        font: local_index,
                        field: "grid size",
                    })?;
                (FontMode::PositionGrid { columns, rows }, planes)
            }
            mode => {
                return Err(FontError::UnsupportedMode {
                    font: local_index,
                    mode,
                })
            }
        };
        let metric_bytes = count
            .checked_mul(FONT_METRIC_BYTES)
            .ok_or(FontError::Overflow {
                font: local_index,
                field: "metrics",
            })?;
        let metric_span = bounded_span(payload, cursor, metric_bytes, local_index, "metrics")?;
        cursor = metric_span.end;
        let sprite_bytes = count
            .checked_mul(planes)
            .and_then(|n| n.checked_mul(4))
            .ok_or(FontError::Overflow {
                font: local_index,
                field: "sprite table",
            })?;
        let sprite_span = bounded_span(payload, cursor, sprite_bytes, local_index, "sprite table")?;
        cursor = sprite_span.end;
        let metrics = source[metric_span.start..metric_span.end]
            .chunks_exact(8)
            .map(|b| {
                let words: [i16; 4] = std::array::from_fn(|i| {
                    i16::from_le_bytes(b[i * 2..i * 2 + 2].try_into().unwrap())
                });
                GlyphMetrics {
                    advance: words[0],
                    spacing: words[1],
                    offset: [words[2], words[3]],
                }
            })
            .collect();
        let sprite_words = source[sprite_span.start..sprite_span.end]
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        fonts.push(Font {
            local_index,
            header_span,
            raw_header,
            mode,
            coordinate_divisor,
            line_step_terms: [raw_header[5] as i32, raw_header[6] as i32],
            maximum_byte: raw_header[10] as i32,
            fallback_byte: raw_header[11] as u8,
            metric_span,
            sprite_span,
            metrics,
            sprite_words,
        });
    }
    Ok(FontBank {
        payload,
        header_table,
        fonts,
        trailing: ByteSpan::new(cursor, payload.end),
    })
}

fn nonnegative(value: i32, font: u32, field: &'static str) -> Result<usize, FontError> {
    usize::try_from(value).map_err(|_| FontError::NegativeCount { font, field, value })
}

fn bounded_span(
    payload: ByteSpan,
    start: usize,
    len: usize,
    font: u32,
    field: &'static str,
) -> Result<ByteSpan, FontError> {
    let end = start
        .checked_add(len)
        .ok_or(FontError::Overflow { font, field })?;
    let span = ByteSpan::new(start, end);
    if start < payload.start || end > payload.end {
        return Err(FontError::Truncated {
            font,
            field,
            span,
            payload,
        });
    }
    Ok(span)
}

#[cfg(test)]
#[path = "fonts_tests.rs"]
mod tests;
