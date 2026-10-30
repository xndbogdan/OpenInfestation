use super::*;
use crate::{parse_world_bank, HandlerFraming, HandlerId, HandlerRegion, RelocationTail};

fn fixture(kinds: &[(u32, &[u8])]) -> (Vec<u8>, WorldBank) {
    let mut main = vec![0; 0x3c + kinds.len() * BRIEFING_RECORD_BYTES];
    main[0x28..0x2c].copy_from_slice(&(kinds.len() as u32).to_le_bytes());
    main[0x2c..0x30].copy_from_slice(&0x3cu32.to_le_bytes());
    let mut relocations = vec![0x2c];
    for (index, (kind, payload)) in kinds.iter().enumerate() {
        let at = 0x3c + index * BRIEFING_RECORD_BYTES;
        main[at..at + 4].copy_from_slice(&(index as u32 + 7).to_le_bytes());
        main[at + 4..at + 8].copy_from_slice(&kind.to_le_bytes());
        main[at + 8..at + 12].copy_from_slice(&0xaabbccddu32.to_le_bytes());
        let payload_offset = main.len() as u32;
        main[at + 12..at + 16].copy_from_slice(&payload_offset.to_le_bytes());
        if !payload.is_empty() {
            relocations.push((at + 12) as u32);
        }
        main.extend_from_slice(payload);
    }
    let mut source = vec![0x55; 9];
    let main_payload = ByteSpan::new(9, 9 + main.len());
    source.extend_from_slice(&main);
    let relocation_offsets = ByteSpan::new(source.len(), source.len() + relocations.len() * 4);
    for offset in &relocations {
        source.extend_from_slice(&offset.to_le_bytes());
    }
    let entry_offsets = ByteSpan::new(source.len(), source.len() + 4);
    source.extend_from_slice(&0u32.to_le_bytes());
    let region = HandlerRegion {
        handler: HandlerId::RELOCATED_14,
        requested_count: 1,
        span: ByteSpan::new(9, source.len()),
        framing: HandlerFraming::RelocationTail(RelocationTail {
            main_payload,
            relocation_count: relocations.len() as u32,
            relocation_offsets,
            entry_offsets,
        }),
    };
    let bank = parse_world_bank(&source, &region).unwrap();
    (source, bank)
}

#[test]
fn dispatch_preserves_authored_order_raw_units_and_signedness() {
    let (source, bank) = fixture(&[
        (0, &[0x80, 0x03, 1, 2]),
        (1, &[0x0e, 0, 0, 0]),
        (
            2,
            &[
                0xff, 0xff, 0, 0x80, 0xff, 0x7f, 0xfe, 0xff, 5, 0, 0x80, 0xff,
            ],
        ),
        (3, &[2, 0, 0, 0]),
        (4, &[4, 0, 0, 0]),
        (5, &[8, 0, 0, 0]),
        (6, &[0x78, 0x56, 0x34, 0x12]),
        (
            8,
            &[
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0x80, 0xff, 0xff, 2, 3,
            ],
        ),
        (9, &[]),
        (10, &[0xff, 0xff]),
        (0xff, &[]),
    ]);
    let before = source.clone();
    let program = parse_briefing_program(&source, &bank, &bank.entries[0]).unwrap();
    assert_eq!(program.records.len(), 11);
    assert_eq!(program.records[0].duration_100ms, 7);
    assert_eq!(program.records[0].raw_word_08, 0xaabb_ccdd);
    assert_eq!(program.records[0].span.start, 9 + 0x3c);
    assert_eq!(
        program.records[0].payload,
        BriefingPayload::MenuText {
            string_id: 0x380,
            increment_bytes: [1, 2]
        }
    );
    assert_eq!(
        program.records[2].payload,
        BriefingPayload::Model(BriefingModel {
            model_slot: -1,
            position_xy: [i16::MIN, i16::MAX],
            depth_bias: -2,
            screen_shift_per_thousand: [5, -128],
        })
    );
    assert_eq!(
        program.records[7].payload,
        BriefingPayload::Camera(BriefingCamera {
            actor_id: u32::MAX,
            heading_degrees: -1,
            rotation_word: i16::MIN,
            speed_word: u16::MAX,
            track_byte: 2,
            terrain_byte: 3,
        })
    );
    assert_eq!(program.records[8].payload, BriefingPayload::Pulse);
    assert!(program.records[8].payload_span.is_empty());
    assert_eq!(
        program.records[9].payload,
        BriefingPayload::MapScale { word: u16::MAX }
    );
    assert_eq!(program.records[10].payload, BriefingPayload::Unknown);
    assert_eq!(
        source, before,
        "no native base addition mutates original bytes"
    );
}

#[test]
fn zero_count_does_not_consume_uninitialized_record_pointer() {
    let (mut source, mut bank) = fixture(&[]);
    source[9 + 0x2c..9 + 0x30].copy_from_slice(&u32::MAX.to_le_bytes());
    bank.relocations.clear();
    let program = parse_briefing_program(&source, &bank, &bank.entries[0]).unwrap();
    assert!(program.records.is_empty());
    assert!(program.record_span.is_empty());
}

#[test]
fn pulse_and_unknown_kind_do_not_dereference_payload_pointer() {
    let (mut source, bank) = fixture(&[(9, &[]), (7, &[])]);
    source[9 + 0x48..9 + 0x4c].copy_from_slice(&u32::MAX.to_le_bytes());
    source[9 + 0x58..9 + 0x5c].copy_from_slice(&0x8000_0000u32.to_le_bytes());
    let program = parse_briefing_program(&source, &bank, &bank.entries[0]).unwrap();
    assert_eq!(program.records[0].payload_offset, u32::MAX);
    assert_eq!(program.records[1].payload_offset, 0x8000_0000);
    assert!(program
        .records
        .iter()
        .all(|record| record.payload_span.is_empty()));
}

#[test]
fn rejects_missing_native_relocation_for_consumed_pointers() {
    let (source, mut bank) = fixture(&[(0, &[0; 4])]);
    bank.relocations.retain(|r| r.field_offset != 0x48);
    assert_eq!(
        parse_briefing_program(&source, &bank, &bank.entries[0]),
        Err(BriefingError::MissingRelocation(0x48))
    );
    bank.relocations.clear();
    assert_eq!(
        parse_briefing_program(&source, &bank, &bank.entries[0]),
        Err(BriefingError::MissingRelocation(0x2c))
    );
}

#[test]
fn rejects_record_and_payload_ranges_crossing_allocation() {
    let (mut source, bank) = fixture(&[(8, &[0; 12])]);
    source[9 + 0x28..9 + 0x2c].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        parse_briefing_program(&source, &bank, &bank.entries[0]),
        Err(BriefingError::AllocationSpan {
            context: "record table",
            ..
        }) | Err(BriefingError::IntegerOverflow(_))
    ));
    source[9 + 0x28..9 + 0x2c].copy_from_slice(&1u32.to_le_bytes());
    source[9 + 0x48..9 + 0x4c]
        .copy_from_slice(&(bank.main_payload.len() as u32 - 11).to_le_bytes());
    assert!(matches!(
        parse_briefing_program(&source, &bank, &bank.entries[0]),
        Err(BriefingError::AllocationSpan {
            context: "record payload",
            ..
        })
    ));
}
