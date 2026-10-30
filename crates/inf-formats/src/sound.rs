//! Handler-9 sound resources.
//!
//! `FUN_0050F760` copies one standard handler allocation, registers its
//! 0x14-byte records, then fixes type-1 offsets relative to the allocation and
//! type-5 references through the global resource table. The PCM format is the
//! executable's fixed 22,050 Hz, mono, signed-16-bit little-endian format.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const SOUND_HANDLER: u8 = 9;
pub const SOUND_ENTRY_BYTES: usize = 0x14;
pub const SOUND_SAMPLE_RATE: u32 = 22_050;
pub const SOUND_CHANNELS: u16 = 1;
pub const SOUND_BITS_PER_SAMPLE: u16 = 16;

/// Wrap Handler-9 PCM bytes in the fixed WAVEFORMATEX used by Infest.exe.
pub fn encode_sound_wav(pcm: &[u8]) -> Result<Vec<u8>, WavError> {
    let data_size = u32::try_from(pcm.len()).map_err(|_| WavError::PcmTooLarge(pcm.len()))?;
    let bytes_per_sample = u32::from(SOUND_BITS_PER_SAMPLE / 8);
    let block_align = SOUND_CHANNELS * (SOUND_BITS_PER_SAMPLE / 8);
    let byte_rate = SOUND_SAMPLE_RATE * u32::from(SOUND_CHANNELS) * bytes_per_sample;
    let riff_size = 36u32
        .checked_add(data_size)
        .ok_or(WavError::PcmTooLarge(pcm.len()))?;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_size.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SOUND_CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SOUND_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&SOUND_BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm);
    Ok(wav)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SoundBank {
    pub payload: ByteSpan,
    pub entries: Vec<SoundEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SoundEntry {
    pub local_index: u32,
    pub record: ByteSpan,
    pub raw_type: u32,
    pub fields_04_to_10: [u32; 4],
    pub kind: SoundEntryKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SoundEntryKind {
    Pcm {
        data: ByteSpan,
        byte_len: u32,
        sample_count: u32,
    },
    Alias {
        raw_target: u32,
        target_global_slot: i16,
        /// 004F4230 adds/subtracts a random fraction of the multiplied pitch.
        pitch_spread_raw_16_16: u32,
        pitch_multiplier_raw_16_16: u32,
        gain_multiplier_raw_16_16: u32,
    },
    Unknown {
        fields_04_to_10: [u32; 4],
    },
}

impl SoundEntryKind {
    pub const fn pcm_span(&self) -> Option<ByteSpan> {
        match self {
            Self::Pcm { data, .. } => Some(*data),
            _ => None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SoundError {
    #[error("sound parser requires handler 9, received handler {0}")]
    WrongHandler(u8),

    #[error("handler 9 requires standard framing")]
    WrongFraming,

    #[error("handler 9 payload span {span:?} is outside the {input_len}-byte OVL")]
    PayloadOutOfBounds { span: ByteSpan, input_len: usize },

    #[error("handler 9 entry table needs {needed} bytes, payload contains {available}")]
    EntryTableTruncated { needed: usize, available: usize },

    #[error("handler 9 entry {entry} PCM span {span:?} is outside payload {payload:?}")]
    PcmOutOfBounds {
        entry: u32,
        span: ByteSpan,
        payload: ByteSpan,
    },

    #[error("handler 9 entry {entry} has odd 16-bit PCM byte length {byte_len}")]
    OddPcmLength { entry: u32, byte_len: u32 },

    #[error("integer overflow while parsing handler 9 entry {entry}")]
    IntegerOverflow { entry: u32 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WavError {
    #[error("PCM payload is too large for a RIFF/WAVE file: {0} bytes")]
    PcmTooLarge(usize),
}

pub fn parse_sound_bank(source: &[u8], region: &HandlerRegion) -> Result<SoundBank, SoundError> {
    if region.handler.get() != SOUND_HANDLER {
        return Err(SoundError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(SoundError::WrongFraming);
    };
    let bytes = payload.get(source).ok_or(SoundError::PayloadOutOfBounds {
        span: payload,
        input_len: source.len(),
    })?;
    let entry_count =
        usize::try_from(region.requested_count).map_err(|_| SoundError::IntegerOverflow {
            entry: region.requested_count,
        })?;
    let table_bytes =
        entry_count
            .checked_mul(SOUND_ENTRY_BYTES)
            .ok_or(SoundError::IntegerOverflow {
                entry: region.requested_count,
            })?;
    if table_bytes > bytes.len() {
        return Err(SoundError::EntryTableTruncated {
            needed: table_bytes,
            available: bytes.len(),
        });
    }

    let mut entries = Vec::with_capacity(entry_count);
    for local_index in 0..region.requested_count {
        let start = usize::try_from(local_index)
            .ok()
            .and_then(|index| index.checked_mul(SOUND_ENTRY_BYTES))
            .ok_or(SoundError::IntegerOverflow { entry: local_index })?;
        let record = &bytes[start..start + SOUND_ENTRY_BYTES];
        let raw_type = u32_at(record, 0x00);
        let field_04 = u32_at(record, 0x04);
        let field_08 = u32_at(record, 0x08);
        let field_0c = u32_at(record, 0x0c);
        let field_10 = u32_at(record, 0x10);
        let fields_04_to_10 = [field_04, field_08, field_0c, field_10];
        let kind = match raw_type {
            1 => {
                if field_08 & 1 != 0 {
                    return Err(SoundError::OddPcmLength {
                        entry: local_index,
                        byte_len: field_08,
                    });
                }
                let data_start = payload
                    .start
                    .checked_add(field_04 as usize)
                    .ok_or(SoundError::IntegerOverflow { entry: local_index })?;
                let data_end = data_start
                    .checked_add(field_08 as usize)
                    .ok_or(SoundError::IntegerOverflow { entry: local_index })?;
                let data = ByteSpan::new(data_start, data_end);
                if data.start < payload.start || data.end > payload.end {
                    return Err(SoundError::PcmOutOfBounds {
                        entry: local_index,
                        span: data,
                        payload,
                    });
                }
                SoundEntryKind::Pcm {
                    data,
                    byte_len: field_08,
                    sample_count: field_08 / 2,
                }
            }
            5 => SoundEntryKind::Alias {
                raw_target: field_04,
                target_global_slot: field_04 as u16 as i16,
                pitch_spread_raw_16_16: field_08,
                pitch_multiplier_raw_16_16: field_0c,
                gain_multiplier_raw_16_16: field_10,
            },
            _ => SoundEntryKind::Unknown { fields_04_to_10 },
        };
        entries.push(SoundEntry {
            local_index,
            record: ByteSpan::new(
                payload.start + start,
                payload.start + start + SOUND_ENTRY_BYTES,
            ),
            raw_type,
            fields_04_to_10,
            kind,
        });
    }

    Ok(SoundBank { payload, entries })
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed record"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HandlerId, HandlerRegion};

    #[test]
    fn parses_pcm_alias_and_unknown_without_pointer_fixups() {
        let mut source = vec![0u8; 60 + 4];
        source[0x00..0x04].copy_from_slice(&1u32.to_le_bytes());
        source[0x04..0x08].copy_from_slice(&60u32.to_le_bytes());
        source[0x08..0x0c].copy_from_slice(&4u32.to_le_bytes());
        source[0x0c..0x10].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        source[0x10..0x14].copy_from_slice(&0x9abc_def0u32.to_le_bytes());
        source[0x14..0x18].copy_from_slice(&5u32.to_le_bytes());
        source[0x18..0x1c].copy_from_slice(&0xFFFF_FFEDu32.to_le_bytes());
        source[0x1c..0x20].copy_from_slice(&0x0001_8000u32.to_le_bytes());
        source[0x28..0x2c].copy_from_slice(&99u32.to_le_bytes());
        source[60..64].copy_from_slice(&[1, 2, 3, 4]);
        let region = HandlerRegion {
            handler: HandlerId::new(9).unwrap(),
            requested_count: 3,
            span: ByteSpan::new(0, source.len()),
            framing: HandlerFraming::Standard {
                payload: ByteSpan::new(0, source.len()),
            },
        };

        let bank = parse_sound_bank(&source, &region).unwrap();
        assert_eq!(bank.entries.len(), 3);
        assert!(matches!(
            bank.entries[0].kind,
            SoundEntryKind::Pcm { data, sample_count: 2, .. }
                if data == ByteSpan::new(60, 64)
        ));
        assert_eq!(bank.entries[0].record, ByteSpan::new(0, SOUND_ENTRY_BYTES));
        assert_eq!(
            bank.entries[0].fields_04_to_10,
            [60, 4, 0x1234_5678, 0x9abc_def0]
        );
        assert!(matches!(
            bank.entries[1].kind,
            SoundEntryKind::Alias {
                raw_target: 0xFFFF_FFED,
                target_global_slot: -19,
                pitch_spread_raw_16_16: 0x0001_8000,
                ..
            }
        ));
        assert!(matches!(
            bank.entries[2].kind,
            SoundEntryKind::Unknown { .. }
        ));
    }

    #[test]
    fn wraps_pcm_in_the_executables_fixed_wave_format() {
        let wav = encode_sound_wav(&[1, 2, 3, 4]).unwrap();
        assert_eq!(wav.len(), 48);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32_at(&wav, 4), 40);
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(u32_at(&wav, 24), SOUND_SAMPLE_RATE);
        assert_eq!(u32_at(&wav, 28), 44_100);
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32_at(&wav, 40), 4);
        assert_eq!(&wav[44..], &[1, 2, 3, 4]);
    }
}
