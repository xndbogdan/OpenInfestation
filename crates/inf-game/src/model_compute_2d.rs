//! Opcode 2D inner compute core: `00478950` plus the `004260C0`
//! arg1==2 path and the `00439E10` row fetch.
//!
//! Retail chain, verified instruction-by-instruction against the
//! executable (see `GAMEPLAY_RUNTIME.md`): `00478950`
//! (thiscall, `ret 0x10`) forwards `(outbuf=a0, ctx=a1, arg1=a2,
//! extras=a3)` to the inner virtual `[this+0x3C]+0x10`, then runs
//! the delta-normalize tail. `004260C0` dispatches on the low byte
//! of arg1 (`-2` → `0x426213`, `-1` → `0x42617E`, `-7` →
//! `0x426135`, else the default path). The arg1==2 path reads
//! three frame words from the row table and sign-extends them to
//! the out buffer; the refine call `0x48CF70` runs only when
//! `[actor+0x140]` is nonzero and is side-effect-only for the
//! outputs (scratch read-only; it writes sim tables), so the
//! output behavior transcribed here needs no refine input.
//!
//! Two transcription corrections over the earlier doc draft,
//! both read off the disassembly:
//!
//! - `00439E10` copies source words at byte offsets 0, 4, 8
//!   (not contiguous `+2i`): `dst.w = (src[0], src[2], src[4])`
//!   in u16 units, from row base `[[ctx]] + extra0 * 24`.
//! - The `0x426213` outs read the stack buffer across two pops
//!   (`pop edi` / `pop esi`), so `out = (buf[0], buf[1],
//!   buf[2])` in order — no reversal.
//!
//! The row table (`[[ctx]]`) and the normalize base
//! (`[[ctx+0x58]+4+4i]`) are simulation-owned H8 state with no
//! port source yet, so this core takes them as explicit inputs
//! and nothing live calls it: the 0x2D executor arm keeps
//! skipping until the table-source mapping (item (b) in the
//! gameplay runtime doc) is owned. The extra words need no
//! such mapping: 0x2D args carry them as `args[2]` (row)
//! and `args[3]` (refine arg), proven 6/6 against the banked
//! call words. No captured words appear
//! here; tests use synthetic rows.

use crate::model_vertices::{ModelVertexError, ModelVertexFault};

/// Which `004260C0` inner path arg1 selects. Only the frame-1
/// 554 path is transcribed; the x87 kernel (3), the
/// terrain-sample path (10) and the default path stay explicitly
/// open and fail closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compute2DPath {
    /// arg1 == 2: row fetch plus sign extension.
    RowSelect,
}

impl Compute2DPath {
    fn select(opcode: u8, arg1: u32) -> Result<Self, ModelVertexError> {
        match arg1 & 0xff {
            2 => Ok(Self::RowSelect),
            _ => Err(ModelVertexFault::UnsupportedOpcode { opcode }.into()),
        }
    }
}

/// Normalize base from `[[ctx+0x58]]`: full DWORD addends plus
/// the low-word subtrahends, both at `+4+4i`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Compute2DBase {
    pub base: [i32; 3],
    pub base16: [u16; 3],
}

/// `00439E10(dst, ctx, extra0)`: three frame words from row
/// `extra0` of the table at `[ctx]`. Rows are 24 bytes (12
/// u16s); the consumed words sit at u16 offsets 0, 2, 4 of the
/// row (byte offsets 0, 4, 8). Out-of-range selectors fail
/// closed — retail would fault on the unmapped read.
pub fn fetch_frame_words(rows: &[u16], extra0: u16) -> Result<[u16; 3], ModelVertexError> {
    let start = usize::from(extra0)
        .checked_mul(12)
        .ok_or(ModelVertexFault::InvalidInstruction { opcode: 0x2d })?;
    let word = |offset: usize| {
        rows.get(
            start
                .checked_add(offset)
                .ok_or(ModelVertexFault::InvalidInstruction { opcode: 0x2d })?,
        )
        .copied()
        .ok_or(ModelVertexFault::InvalidInstruction { opcode: 0x2d })
    };
    Ok([word(0)?, word(2)?, word(4)?])
}

/// The `0x426213` outputs: sign extension of the three frame
/// words to the out buffer (the pops only shift the stack
/// reads; the word order is unchanged).
pub fn inner_row_select(frame: [u16; 3]) -> [i32; 3] {
    frame.map(|word| i32::from(word as i16))
}

/// The `00478950` tail: `out[i] = base[i] +
/// sext16(in_low[i] - base16[i])`. The subtraction wraps in 16
/// bits (`sub cx, [...]`), the extension is signed (`movsx`),
/// and the accumulation wraps in 32 bits (`add`).
pub fn normalize_tail(base: Compute2DBase, inner: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| {
        let diff = (inner[i] as u16).wrapping_sub(base.base16[i]);
        base.base[i].wrapping_add(i32::from(diff as i16))
    })
}

/// Full arg1==2 compute: dispatch, row fetch, sign extension,
/// tail normalize. `opcode` must be `0x2d`; any other arg1 low
/// byte fails closed with its native dispatch left open.
pub fn compute_2d(
    opcode: u8,
    arg1: u32,
    rows: &[u16],
    extra0: u16,
    base: Compute2DBase,
) -> Result<[i32; 3], ModelVertexError> {
    if opcode != 0x2d {
        return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into());
    }
    match Compute2DPath::select(opcode, arg1)? {
        Compute2DPath::RowSelect => {
            let frame = fetch_frame_words(rows, extra0)?;
            Ok(normalize_tail(base, inner_row_select(frame)))
        }
    }
}

#[cfg(test)]
#[path = "model_compute_2d_tests.rs"]
mod tests;
