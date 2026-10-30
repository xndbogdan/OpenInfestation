//! 00445840 translation, latch and bounds, plus receipts from the unmodified
//! translator (`tests/fixtures/key_text_native.json`).

use super::*;
use crate::frontend_assets::FrontendAssets;
use inf_formats::{ByteSpan, KeyName, KEY_NAME_COUNT};
use std::path::Path;

fn synthetic_names() -> KeyNameTable {
    KeyNameTable {
        virtual_address: 0x0053_5E54,
        entries: (0..KEY_NAME_COUNT)
            .map(|index| KeyName {
                code: index as u8,
                pointer: 0,
                name: match index {
                    0x0E => b"Backspace".to_vec(),
                    0x1E => b"A".to_vec(),
                    0x30 => b"B".to_vec(),
                    0x9C => b"Keypad Enter Extended".to_vec(),
                    _ => Vec::new(),
                },
                file_span: ByteSpan::new(0, 1),
            })
            .collect(),
    }
}

fn keys(codes: &[u8]) -> DeviceButtons {
    let mut buttons = DeviceButtons::default();
    for code in codes {
        buttons.set(*code, true);
    }
    buttons
}

#[test]
fn reports_the_lowest_code_once_per_full_release() {
    let names = synthetic_names();
    let mut latch = KeyTextLatch::default();
    let idle = KeyRead::Idle { stale_byte: 7 };
    assert_eq!(latch.read(keys(&[]), &names, 7), Ok(idle.clone()));
    assert!(!latch.held());
    // 004F5230 lists ascending codes; only the first is named.
    assert_eq!(
        latch.read(keys(&[0x30, 0x1E]), &names, 7),
        Ok(KeyRead::Named(b"A".to_vec()))
    );
    assert!(latch.held());
    // Held or overlapping keys stay idle until every bit clears.
    assert_eq!(latch.read(keys(&[0x30]), &names, 7), Ok(idle.clone()));
    assert!(latch.held());
    assert_eq!(latch.read(keys(&[]), &names, 7), Ok(idle.clone()));
    assert!(!latch.held());
    assert_eq!(
        latch.read(keys(&[0x30]), &names, 7),
        Ok(KeyRead::Named(b"B".to_vec()))
    );
    // Unnamed codes still latch with an empty name.
    let mut blank = KeyTextLatch::default();
    assert_eq!(
        blank.read(keys(&[0x54]), &names, 7),
        Ok(KeyRead::Named(Vec::new()))
    );
    assert!(blank.held());
}

#[test]
fn copies_at_most_fourteen_name_bytes() {
    let mut latch = KeyTextLatch::default();
    assert_eq!(
        latch.read(keys(&[0x9C]), &synthetic_names(), 0),
        Ok(KeyRead::Named(b"Keypad Enter E".to_vec()))
    );
}

#[test]
fn native_faults_are_explicit_and_leave_the_latch_clear() {
    let names = synthetic_names();
    let mut latch = KeyTextLatch::default();
    // Four held keys fill 004F5230's buffer exactly; five overrun it.
    assert!(latch.read(keys(&[1, 2, 3, 0x1E]), &names, 0).is_ok());
    let mut latch = KeyTextLatch::default();
    assert_eq!(
        latch.read(keys(&[1, 2, 3, 4, 0x1E]), &names, 0),
        Err(KeyTextError::Overrun { pressed: 5 })
    );
    assert!(!latch.held());
    assert_eq!(
        latch.read(keys(&[0xDE]), &names, 0),
        Err(KeyTextError::NullName)
    );
    assert!(!latch.held());
    assert_eq!(
        latch.read(keys(&[0xDF]), &names, 0),
        Err(KeyTextError::PastTable { code: 0xDF })
    );
    assert!(!latch.held());
    // A held latch never reaches either fault.
    let mut held = KeyTextLatch { held: true };
    assert_eq!(
        held.read(keys(&[1, 2, 3, 4, 5, 0xDE]), &names, 9),
        Ok(KeyRead::Idle { stale_byte: 9 })
    );
}

#[test]
fn poll_stale_byte_takes_bits_8_to_15_of_the_millisecond_clock() {
    // The retail sample 0x09E900E0 ms left 0x00 in buf[1] and 0xE9 in buf[2].
    assert_eq!(poll_stale_byte(0x09E9_00E0 * 1000), 0x00);
    assert_eq!(poll_stale_byte(0x09E9_01FF * 1000 + 999), 0x01);
    assert_eq!(poll_stale_byte(255_999), 0x00);
    assert_eq!(poll_stale_byte(256_000), 0x01);
    // 004A4890 sign-extends timeGetTime before multiplying by 1000, so
    // millisecond words at or above 2^31 arrive as negative patterns.
    let negative = |ms: u32| (i64::from(ms as i32) * 1000) as u64;
    assert_eq!(poll_stale_byte(negative(0x8000_1234)), 0x12);
    assert_eq!(poll_stale_byte(negative(0xFFFF_F0FF)), 0xF0);
}

#[test]
fn matches_the_original_translator_across_key_sequences() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infest.exe is required");
    let sequences: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/key_text_native.json")).unwrap();
    let sequences = sequences.as_array().unwrap();
    assert_eq!(sequences.len(), 64);
    let mut calls = 0;
    for sequence in sequences {
        let mut latch = KeyTextLatch::default();
        for step in sequence.as_array().unwrap() {
            let codes: Vec<u8> = step["codes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|code| code.as_u64().unwrap() as u8)
                .collect();
            let buffer: Vec<u8> = step["buffer"]
                .as_array()
                .unwrap()
                .iter()
                .map(|byte| byte.as_u64().unwrap() as u8)
                .collect();
            let read = latch.read(keys(&codes), assets.key_names(), 0xCC).unwrap();
            match read {
                KeyRead::Named(name) => {
                    // strncpy pads with NUL; buf[14] is forced to zero.
                    let mut expected = name.clone();
                    expected.resize(15, 0);
                    assert_eq!(buffer, expected, "{step}");
                }
                KeyRead::Idle { stale_byte } => {
                    // Only buf[0] is written; the rest keeps its old bytes.
                    assert_eq!(buffer[0], 0, "{step}");
                    assert!(buffer[1..].iter().all(|byte| *byte == 0xCC), "{step}");
                    assert_eq!(stale_byte, buffer[1]);
                }
            }
            assert_eq!(latch.held(), step["latch"].as_u64().unwrap() != 0, "{step}");
            calls += 1;
        }
    }
    assert_eq!(calls, 310);
}
