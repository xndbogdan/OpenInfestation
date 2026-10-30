//! Consumer-specific views of Handler-10 buffers used by `00470170`.
//! The fixed-record parser remains the lossless authority. This module names
//! only the three global-slot roles that the native consumer actually reads;
//! it does not infer a general multidimensional array format from metadata.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, Handler10Record};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleGridRole {
    EightWordSamples,
    FourSampleMask,
    MaterialIndices,
}

impl SampleGridRole {
    /// `00470170` uses global registrations 0, 1, and 2, not local ordinals.
    pub const fn from_global_slot(slot: u16) -> Option<Self> {
        match slot {
            0 => Some(Self::EightWordSamples),
            1 => Some(Self::FourSampleMask),
            2 => Some(Self::MaterialIndices),
            _ => None,
        }
    }

    const fn component_shape(self) -> (usize, u32) {
        match self {
            Self::EightWordSamples => (8, 2),
            Self::FourSampleMask | Self::MaterialIndices => (1, 1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "encoding", content = "values", rename_all = "snake_case")]
pub enum SampleGridValues {
    SignedWords8(Vec<[i16; 8]>),
    SignedBytes(Vec<i8>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SampleGrid {
    pub role: SampleGridRole,
    /// Stored dimensions, not the native reflected output dimensions.
    pub width: u32,
    pub height: u32,
    pub dimensions_span: ByteSpan,
    pub descriptor_span: ByteSpan,
    /// Raw metadata at the +14 allocation-relative pointer.
    pub descriptor_words: Vec<u32>,
    pub samples_span: ByteSpan,
    pub samples: SampleGridValues,
}

impl SampleGrid {
    pub fn sample_count(&self) -> usize {
        match &self.samples {
            SampleGridValues::SignedWords8(values) => values.len(),
            SampleGridValues::SignedBytes(values) => values.len(),
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SampleGridError {
    #[error("sample-grid payload {0:?} is outside the source")]
    InvalidPayload(ByteSpan),
    #[error("sample-grid {field} at allocation offset {offset:#x} needs {length} bytes in a {payload_len}-byte payload")]
    OutOfBounds {
        field: &'static str,
        offset: u32,
        length: usize,
        payload_len: usize,
    },
    #[error("unsupported sample-grid dimensions {width} by {height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("sample-grid dimensions overflow a bounded array")]
    SizeOverflow,
    #[error(
        "sample-grid metadata {actual:?} differs from the supported {role:?} layout {expected:?}"
    )]
    UnsupportedMetadata {
        role: SampleGridRole,
        expected: [u32; 4],
        actual: [u32; 4],
    },
    #[error("sample-grid component descriptor {actual:?} differs from supported {expected:?}")]
    UnsupportedDescriptor {
        expected: Vec<u32>,
        actual: Vec<u32>,
    },
}

/// Decode a proven consumer role while retaining exact OVL spans. Unknown
/// roles remain available through `parse_handler10_bank`, never guessed here.
/// Header/descriptor checks delimit the supported retail layout; they are not
/// claims that the original loader validates or interprets those words.
pub fn decode_sample_grid(
    source: &[u8],
    payload: ByteSpan,
    record: &Handler10Record,
    role: SampleGridRole,
) -> Result<SampleGrid, SampleGridError> {
    let allocation = payload
        .get(source)
        .ok_or(SampleGridError::InvalidPayload(payload))?;
    let range = |offset: u32, length: usize, field| {
        let start = offset as usize;
        let end = start
            .checked_add(length)
            .ok_or(SampleGridError::SizeOverflow)?;
        if end > allocation.len() {
            return Err(SampleGridError::OutOfBounds {
                field,
                offset,
                length,
                payload_len: allocation.len(),
            });
        }
        Ok(ByteSpan::new(payload.start + start, payload.start + end))
    };
    let dimensions_span = range(record.relative_offsets[0], 8, "dimensions")?;
    let dimensions = dimensions_span.get(source).expect("bounded span");
    let width = u32::from_le_bytes(dimensions[..4].try_into().unwrap());
    let height = u32::from_le_bytes(dimensions[4..].try_into().unwrap());
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(SampleGridError::InvalidDimensions { width, height });
    }
    let count = width
        .checked_mul(height)
        .ok_or(SampleGridError::SizeOverflow)?;
    let (components, component_bytes) = role.component_shape();
    let expected = [
        2,
        components as u32,
        count,
        components as u32 * component_bytes,
    ];
    if record.words_00_to_0c != expected {
        return Err(SampleGridError::UnsupportedMetadata {
            role,
            expected,
            actual: record.words_00_to_0c,
        });
    }
    let descriptor_span = range(
        record.relative_offsets[1],
        components * 4,
        "component descriptors",
    )?;
    let descriptor_words: Vec<_> = descriptor_span
        .get(source)
        .expect("bounded span")
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    if descriptor_words.iter().any(|&word| word != component_bytes) {
        return Err(SampleGridError::UnsupportedDescriptor {
            expected: vec![component_bytes; components],
            actual: descriptor_words,
        });
    }
    let sample_bytes = usize::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(expected[3] as usize))
        .ok_or(SampleGridError::SizeOverflow)?;
    let samples_span = range(record.relative_offsets[2], sample_bytes, "samples")?;
    let bytes = samples_span.get(source).expect("bounded span");
    let samples = match role {
        SampleGridRole::EightWordSamples => SampleGridValues::SignedWords8(
            bytes
                .chunks_exact(16)
                .map(|cell| {
                    std::array::from_fn(|i| i16::from_le_bytes([cell[i * 2], cell[i * 2 + 1]]))
                })
                .collect(),
        ),
        SampleGridRole::FourSampleMask | SampleGridRole::MaterialIndices => {
            SampleGridValues::SignedBytes(bytes.iter().map(|&b| b as i8).collect())
        }
    };
    Ok(SampleGrid {
        role,
        width,
        height,
        dimensions_span,
        descriptor_span,
        descriptor_words,
        samples_span,
        samples,
    })
}

#[cfg(test)]
#[path = "sample_grids_tests.rs"]
mod tests;
