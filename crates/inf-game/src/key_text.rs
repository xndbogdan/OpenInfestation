//! Direct-keyboard text translation 00445840 and its process latch 0054766C.
//!
//! The text-entry tick asks this translator for one key per full-release
//! cycle. It reads the 256-bit DirectInput state of device-adapter slot 0
//! (`00439610(0)`, `[adapter+4]+0x2C`), not the binding queue. With the latch
//! clear and any bit set (00445090), 004F5230 lists the set scan codes in
//! ascending order and the lowest one's name from table 00535E54 is copied
//! into the tick's 15-byte buffer. The latch then suppresses every later key
//! until all bits are clear again, so overlapping presses report only the
//! first key. The latch is process-global and never reset per screen.

use inf_formats::KeyNameTable;
use thiserror::Error;

use crate::input::DeviceButtons;
use crate::name_entry::KeyRead;

/// 004F5230's output buffer in 00445840 (`[esp+0x10]`, capacity 4).
pub const KEY_LIST_CAPACITY: u32 = 4;
/// 00445840 copies at most fourteen name bytes (`strncpy(15)`, `buf[14] = 0`).
pub const KEY_TEXT_BYTES: usize = 14;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyTextError {
    /// 004F5230 writes every set code without a capacity check, then clears
    /// the remaining capacity with a negative count: five or more held keys
    /// overrun the 4-byte buffer and fault natively.
    #[error("{pressed} keys held; 004F5230 overruns its 4-byte buffer natively")]
    Overrun { pressed: u32 },
    /// Code 0xDE reads the NULL word after the table; strncpy faults
    /// natively before the latch store.
    #[error("scan code 0xDE reads 00535E54's NULL terminator word; strncpy faults natively")]
    NullName,
    /// Codes 0xDF..0xFF natively read the adjacent `{name, descriptor}` pair
    /// table, copy an action name (odd codes) or descriptor bytes (even codes)
    /// and set the latch. The port rejects them as its own boundary; the SDL
    /// adapter never produces them.
    #[error("scan code {code:#04x} is past the 00535E54 key-name table")]
    PastTable { code: u8 },
}

/// Process latch 0054766C, initially clear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyTextLatch {
    held: bool,
}

impl KeyTextLatch {
    pub fn held(self) -> bool {
        self.held
    }

    /// One 00445840 call over the keyboard's current state. `stale_byte` is
    /// what the tick's buffer already holds at `buf[1]` when only `buf[0]` is
    /// written; see [`poll_stale_byte`]. Failures leave the latch unchanged:
    /// the overrun and code 0xDE fault natively before its store, while the
    /// past-table rejection is a port boundary.
    pub fn read(
        &mut self,
        buttons: DeviceButtons,
        names: &KeyNameTable,
        stale_byte: u8,
    ) -> Result<KeyRead, KeyTextError> {
        let idle = KeyRead::Idle { stale_byte };
        let any = buttons.0.iter().any(|word| *word != 0);
        if self.held {
            if !any {
                self.held = false;
            }
            return Ok(idle);
        }
        if !any {
            return Ok(idle);
        }
        let pressed: u32 = buttons.0.iter().map(|word| word.count_ones()).sum();
        if pressed > KEY_LIST_CAPACITY {
            return Err(KeyTextError::Overrun { pressed });
        }
        let code = lowest_code(buttons);
        let name = names.name(code).ok_or(if code == 0xDE {
            KeyTextError::NullName
        } else {
            KeyTextError::PastTable { code }
        })?;
        self.held = true;
        Ok(KeyRead::Named(
            name[..name.len().min(KEY_TEXT_BYTES)].to_vec(),
        ))
    }
}

/// The stale `buf[1]` of an idle 00445840 call in steady-state text-entry
/// ticks: bits 8..15 of the millisecond clock sampled by the selected-input
/// poll 004F0200 (`push eax` at 004A48A2). The argument is the 004A4890 bit
/// pattern the input poll samples: `timeGetTime()` sign-extended (`cdq`) and
/// multiplied by 1000, so signed division recovers the pushed millisecond
/// word. The first tick after screen construction had another, unidentified
/// writer in retail.
pub fn poll_stale_byte(poll_micros: u64) -> u8 {
    ((poll_micros as i64 / 1000) as u32 >> 8) as u8
}

fn lowest_code(buttons: DeviceButtons) -> u8 {
    let (word, bits) = buttons
        .0
        .iter()
        .enumerate()
        .find(|(_, bits)| **bits != 0)
        .expect("caller checked a set bit");
    (word as u32 * 32 + bits.trailing_zeros()) as u8
}

#[cfg(test)]
#[path = "key_text_tests.rs"]
mod tests;
