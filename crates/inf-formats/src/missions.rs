//! Fixed-layout mission resources from Handlers 11 and 15.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const MISSION_PLACEMENT_HANDLER: u8 = 11;
pub const MISSION_PARAMETER_HANDLER: u8 = 15;
pub const MISSION_PLACEMENT_BYTES: usize = 0x400;
pub const MISSION_PLACEMENT_SLOTS: usize = 256;
pub const MISSION_PARAMETER_BYTES: usize = 0x14;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MissionPlacementBank {
    pub payload: ByteSpan,
    pub entries: Vec<MissionPlacementTable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MissionPlacementTable {
    pub local_index: u32,
    pub span: ByteSpan,
    pub slots: Vec<MissionPlacementSlot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct MissionPlacementSlot {
    pub slot: u16,
    /// Raw first word. Runtime meaning is deliberately not inferred here.
    pub resource_word: u16,
    /// Raw second word. High/low-byte interpretations remain presentation data.
    pub parameter_word: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MissionParameterBank {
    pub payload: ByteSpan,
    pub entries: Vec<MissionParameters>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct MissionParameters {
    pub local_index: u32,
    pub span: ByteSpan,
    pub fields: [u32; 5],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MissionError {
    #[error("mission parser requires handler 11 or 15, received handler {0}")]
    WrongHandler(u8),

    #[error("mission handlers require standard framing")]
    WrongFraming,

    #[error("handler {handler} payload span {span:?} is outside the {input_len}-byte OVL")]
    PayloadOutOfBounds {
        handler: u8,
        span: ByteSpan,
        input_len: usize,
    },

    #[error(
        "handler {handler} payload has {actual} bytes; {count} entries require exactly {expected}"
    )]
    PayloadSizeMismatch {
        handler: u8,
        count: u32,
        expected: usize,
        actual: usize,
    },

    #[error("integer overflow while sizing handler {handler} with {count} entries")]
    IntegerOverflow { handler: u8, count: u32 },
}

pub fn parse_mission_placements(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<MissionPlacementBank, MissionError> {
    let (payload, bytes) = standard_payload(source, region, MISSION_PLACEMENT_HANDLER)?;
    validate_size(region, bytes.len(), MISSION_PLACEMENT_BYTES)?;
    let mut entries = Vec::with_capacity(region.requested_count as usize);
    for local_index in 0..region.requested_count {
        let relative = local_index as usize * MISSION_PLACEMENT_BYTES;
        let span = ByteSpan::new(
            payload.start + relative,
            payload.start + relative + MISSION_PLACEMENT_BYTES,
        );
        let entry = &bytes[relative..relative + MISSION_PLACEMENT_BYTES];
        let mut slots = Vec::with_capacity(MISSION_PLACEMENT_SLOTS);
        for slot in 0..MISSION_PLACEMENT_SLOTS {
            let offset = slot * 4;
            slots.push(MissionPlacementSlot {
                slot: slot as u16,
                resource_word: u16_at(entry, offset),
                parameter_word: u16_at(entry, offset + 2),
            });
        }
        entries.push(MissionPlacementTable {
            local_index,
            span,
            slots,
        });
    }
    Ok(MissionPlacementBank { payload, entries })
}

pub fn parse_mission_parameters(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<MissionParameterBank, MissionError> {
    let (payload, bytes) = standard_payload(source, region, MISSION_PARAMETER_HANDLER)?;
    validate_size(region, bytes.len(), MISSION_PARAMETER_BYTES)?;
    let mut entries = Vec::with_capacity(region.requested_count as usize);
    for local_index in 0..region.requested_count {
        let relative = local_index as usize * MISSION_PARAMETER_BYTES;
        let span = ByteSpan::new(
            payload.start + relative,
            payload.start + relative + MISSION_PARAMETER_BYTES,
        );
        let entry = &bytes[relative..relative + MISSION_PARAMETER_BYTES];
        entries.push(MissionParameters {
            local_index,
            span,
            fields: std::array::from_fn(|field| u32_at(entry, field * 4)),
        });
    }
    Ok(MissionParameterBank { payload, entries })
}

fn standard_payload<'a>(
    source: &'a [u8],
    region: &HandlerRegion,
    expected_handler: u8,
) -> Result<(ByteSpan, &'a [u8]), MissionError> {
    if region.handler.get() != expected_handler {
        return Err(MissionError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(MissionError::WrongFraming);
    };
    let bytes = payload
        .get(source)
        .ok_or(MissionError::PayloadOutOfBounds {
            handler: expected_handler,
            span: payload,
            input_len: source.len(),
        })?;
    Ok((payload, bytes))
}

fn validate_size(region: &HandlerRegion, actual: usize, stride: usize) -> Result<(), MissionError> {
    let expected = usize::try_from(region.requested_count)
        .ok()
        .and_then(|count| count.checked_mul(stride))
        .ok_or(MissionError::IntegerOverflow {
            handler: region.handler.get(),
            count: region.requested_count,
        })?;
    if actual != expected {
        return Err(MissionError::PayloadSizeMismatch {
            handler: region.handler.get(),
            count: region.requested_count,
            expected,
            actual,
        });
    }
    Ok(())
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("fixed record"))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed record"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HandlerId;

    fn standard_region(handler: u8, count: u32, payload_len: usize) -> HandlerRegion {
        HandlerRegion {
            handler: HandlerId::new(handler).unwrap(),
            requested_count: count,
            span: ByteSpan::new(0, payload_len),
            framing: HandlerFraming::Standard {
                payload: ByteSpan::new(0, payload_len),
            },
        }
    }

    #[test]
    fn parses_all_placement_slots_without_filtering_zero_records() {
        let mut source = vec![0u8; MISSION_PLACEMENT_BYTES];
        source[4..6].copy_from_slice(&69u16.to_le_bytes());
        source[6..8].copy_from_slice(&26u16.to_le_bytes());
        let bank =
            parse_mission_placements(&source, &standard_region(11, 1, MISSION_PLACEMENT_BYTES))
                .unwrap();
        assert_eq!(bank.entries[0].slots.len(), 256);
        assert_eq!(bank.entries[0].slots[0].resource_word, 0);
        assert_eq!(bank.entries[0].slots[1].resource_word, 69);
        assert_eq!(bank.entries[0].slots[1].parameter_word, 26);
    }

    #[test]
    fn parses_all_five_parameter_words_and_rejects_trailing_data() {
        let mut source = vec![0u8; MISSION_PARAMETER_BYTES];
        for field in 0..5 {
            source[field * 4..field * 4 + 4].copy_from_slice(&(field as u32 + 10).to_le_bytes());
        }
        let bank =
            parse_mission_parameters(&source, &standard_region(15, 1, MISSION_PARAMETER_BYTES))
                .unwrap();
        assert_eq!(bank.entries[0].fields, [10, 11, 12, 13, 14]);

        source.push(0);
        assert!(matches!(
            parse_mission_parameters(
                &source,
                &standard_region(15, 1, MISSION_PARAMETER_BYTES + 1)
            ),
            Err(MissionError::PayloadSizeMismatch { actual: 21, .. })
        ));
    }
}
