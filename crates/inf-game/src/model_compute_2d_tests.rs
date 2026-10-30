//! Focused 0x2D compute-core tests: row stride/select, sign
//! extension, tail arithmetic (wrap, truncation), dispatch.
//! All rows and bases are synthetic — no captured words.

use super::*;
use crate::model_vertices::ModelVertexFault;

fn base() -> Compute2DBase {
    Compute2DBase {
        base: [100, 200, 300],
        base16: [10, 20, 30],
    }
}

// Two 24-byte rows (12 u16s each). Row 0 carries markers at the
// consumed stride-4 slots (u16 0/2/4) and distinct decoys at the
// contiguous slots (u16 1/3/5) the old +2i draft misread.
fn rows() -> Vec<u16> {
    vec![
        0x1111, 0xAAAA, 0x2222, 0xBBBB, 0x3333, 0xCCCC, 0, 0, 0, 0, 0, 0, 0x4444, 0xDDDD, 0x5555,
        0xEEEE, 0x6666, 0xFFFF, 0, 0, 0, 0, 0, 0,
    ]
}

#[test]
fn fetch_reads_stride4_words_of_the_selected_row() {
    assert_eq!(
        fetch_frame_words(&rows(), 0).unwrap(),
        [0x1111, 0x2222, 0x3333]
    );
    assert_eq!(
        fetch_frame_words(&rows(), 1).unwrap(),
        [0x4444, 0x5555, 0x6666]
    );
}

#[test]
fn fetch_fails_closed_out_of_range() {
    assert!(matches!(
        fetch_frame_words(&rows(), 2),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidInstruction { opcode: 0x2d }
        ))
    ));
    assert!(matches!(
        fetch_frame_words(&rows(), 0xFFFF),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidInstruction { opcode: 0x2d }
        ))
    ));
    assert!(matches!(
        fetch_frame_words(&[], 0),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidInstruction { opcode: 0x2d }
        ))
    ));
}

#[test]
fn inner_sign_extends_frame_words() {
    assert_eq!(
        inner_row_select([0x7FFF, 0x8000, 0xFFFF]),
        [32767, -32768, -1]
    );
    assert_eq!(inner_row_select([0, 1, 0x1234]), [0, 1, 0x1234]);
}

#[test]
fn tail_adds_base_over_wrapped_differences() {
    // 15-10, 25-20, 35-30.
    assert_eq!(normalize_tail(base(), [15, 25, 35]), [105, 205, 305]);
}

#[test]
fn tail_wraps_below_and_truncates_to_low_words() {
    // 5-10 wraps to -5; high DWORD bits never participate.
    assert_eq!(
        normalize_tail(base(), [5, 0x1_0005, -1]),
        [95, 185, 300 + ((0xFFFFu16.wrapping_sub(30)) as i16 as i32)]
    );
}

#[test]
fn dispatch_runs_row_select_and_rejects_other_paths() {
    // Full pipeline on synthetic rows: frame [0x1111, 0x2222,
    // 0x3333], each minus its base16, plus base.
    let out = compute_2d(0x2d, 2, &rows(), 0, base()).unwrap();
    assert_eq!(
        out,
        [
            100 + (0x1111i32 - 10),
            200 + (0x2222i32 - 20),
            300 + (0x3333i32 - 30),
        ]
    );
    for arg1 in [0u32, 1, 3, 10, 11, 0xFF] {
        assert!(matches!(
            compute_2d(0x2d, arg1, &rows(), 0, base()),
            Err(ModelVertexError::Fault(
                ModelVertexFault::UnsupportedOpcode { opcode: 0x2d }
            ))
        ));
    }
    assert!(matches!(
        compute_2d(0x22, 2, &rows(), 0, base()),
        Err(ModelVertexError::Fault(
            ModelVertexFault::UnsupportedOpcode { opcode: 0x22 }
        ))
    ));
}
