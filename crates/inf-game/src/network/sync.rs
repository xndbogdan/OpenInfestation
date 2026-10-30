//! The classes' network sync slots, virtual `+C0` (out) and `+C4` (in),
//! which the actor state record (message 1) carries after its shared
//! fields. Each works on the fields it moves; the arena's wrappers
//! (`object_runtime::actor::network_sync`) fetch and store them.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SyncError {
    #[error("a sync payload ends before offset {0}")]
    Short(usize),
    #[error("a received hold of {count} handles overruns its {slots} slots")]
    HoldOverrun { count: usize, slots: usize },
}

fn word(payload: &[u8], at: usize) -> Result<u16, SyncError> {
    payload
        .get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(SyncError::Short(at + 2))
}

fn dword(payload: &[u8], at: usize) -> Result<u32, SyncError> {
    payload
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(SyncError::Short(at + 4))
}

/// `0040DEF0`, the `+C0` of family `40D800` (selectors 15, 37 and 39):
/// `+140` and `+13C` as the high and low nibbles of a byte.
pub fn family40d800_out(out: &mut [u8], room: i32, [word_13c, word_140]: [i32; 2]) -> i32 {
    if room <= 0 {
        return -1;
    }
    out[0] = ((word_140 as u8) << 4) | word_13c as u8;
    1
}

/// `0040DF20`, its `+C4`. The low nibble is ORed with `0x0F` (not masked),
/// as in retail, before it replaces `+13C`; `+140` takes the high nibble,
/// each clamped to `0..=+13C`.
pub fn family40d800_in(payload: &[u8], fields: &mut [i32; 2]) -> Result<i32, SyncError> {
    let byte = i32::from(*payload.first().ok_or(SyncError::Short(1))?);
    let (low, high) = (byte | 0xf, byte >> 4);
    let [word_13c, word_140] = fields;
    if low != *word_13c {
        let previous = *word_140;
        *word_13c = low;
        if previous < 1 {
            *word_140 = 0;
        } else if previous > low {
            *word_140 = low;
        }
    }
    if *word_140 != high {
        *word_140 = high;
        if high < 1 {
            *word_140 = 0;
        } else if high > *word_13c {
            *word_140 = *word_13c;
        }
    }
    Ok(1)
}

/// The fields of selectors 31 and 32 (family `485F50`) their sync moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotSync {
    pub words_138: [u16; 3],
    pub word_144: u16,
    pub word_146: u16,
}

/// `00486310`: `+138..+13D`, the first WORD's low bit replaced by `+144`
/// (ORed in whole) and the second's by the sign of `+146`.
pub fn shot_out(out: &mut [u8], room: i32, fields: ShotSync) -> i32 {
    if room < 6 {
        return -1;
    }
    let [first, second, third] = fields.words_138;
    let first = (first & !1) | fields.word_144;
    let second = (second & !1) | u16::from((fields.word_146 as i16) < 0);
    for (index, value) in [first, second, third].iter().enumerate() {
        out[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    6
}

/// `00486370`: `+138..+13D` back, `+144` the first byte's low bit, and
/// `+146` set to -1 when the third byte's low bit is set.
pub fn shot_in(payload: &[u8], fields: &mut ShotSync) -> Result<i32, SyncError> {
    fields.words_138 = [word(payload, 0)?, word(payload, 2)?, word(payload, 4)?];
    fields.word_144 = u16::from(payload[0] & 1);
    if payload[2] & 1 != 0 {
        fields.word_146 = 0xffff;
    }
    Ok(6)
}

/// The vehicle's (selector 2) fields its sync moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleSync {
    pub mode_1e8: u32,
    pub dword_1ec: u32,
    pub dword_1fc: u32,
    pub byte_208: u8,
    pub word_20e: u16,
    pub dword_210: u32,
    pub dword_220: u32,
    pub dword_238: u32,
    pub dword_244: u32,
    pub word_58c: u16,
    /// The hold's handles (`+1A4`: `+8`'s slots, each handle at `+4`).
    pub hold: Vec<u32>,
}

/// `004127A0`: the hold's handles up to its first empty slot (the process
/// NULL); nothing when there is none or the first is empty.
pub fn hold_out(out: &mut [u8], room: i32, hold: &[u32], null: u32) -> i32 {
    let Some(count) = hold.iter().position(|&handle| handle == null) else {
        return 0;
    };
    if count == 0 {
        return 0;
    }
    if room < (count * 4) as i32 {
        return -1;
    }
    for (index, handle) in hold[..count].iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&handle.to_le_bytes());
    }
    (count * 4) as i32
}

/// `00412810`: the room's DWORDs into the first slots, the rest emptied.
pub fn hold_in(payload: &[u8], room: i32, hold: &mut [u32], null: u32) -> Result<i32, SyncError> {
    let count = (room as u32 >> 2) as i32;
    let filled = count.max(0) as usize;
    if filled > hold.len() {
        return Err(SyncError::HoldOverrun {
            count: filled,
            slots: hold.len(),
        });
    }
    for (index, slot) in hold.iter_mut().enumerate() {
        *slot = if index < filled {
            u32::from_le_bytes(
                payload
                    .get(index * 4..index * 4 + 4)
                    .ok_or(SyncError::Short(index * 4 + 4))?
                    .try_into()
                    .unwrap(),
            )
        } else {
            null
        };
    }
    Ok(count.wrapping_mul(4))
}

/// `004120B0`: whether a slot holds the handle.
pub fn hold_has(hold: &[u32], handle: u32) -> bool {
    hold.contains(&handle)
}

/// `00420DA0`, the vehicle's `+C0`: a WORD of its mode (`+1E8`), `+208`,
/// `+238`, `+20E` and `+58C` (four bits each but two for `+238`), its
/// speed over 16 (`+244` in modes 1 and 2, else `+220`), the low WORDs of
/// `+1EC`, `+210` and `+1FC`, then the hold's handles.
pub fn vehicle_out(out: &mut [u8], room: i32, fields: &VehicleSync, null: u32) -> i32 {
    if room < 10 {
        return -1;
    }
    let packed = (fields.mode_1e8 & 0xf) as u16
        | if fields.byte_208 != 0 { 0x10 } else { 0 }
        | ((fields.dword_238 & 3) as u16) << 5
        | (fields.word_20e & 0xf) << 8
        | (fields.word_58c & 0xf) << 12;
    let speed = if matches!(fields.mode_1e8, 1 | 2) {
        fields.dword_244
    } else {
        fields.dword_220
    };
    let words = [
        packed,
        ((speed as i32) >> 4) as u16,
        fields.dword_1ec as u16,
        fields.dword_210 as u16,
        fields.dword_1fc as u16,
    ];
    for (index, value) in words.iter().enumerate() {
        out[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    match hold_out(&mut out[10..], room - 10, &fields.hold, null) {
        -1 => -1,
        length => length + 10,
    }
}

/// `00420E70`, its `+C4` (`0041F9B0` keeps bit 4 at `+208`). A payload under
/// ten bytes is left alone; either way the length is returned.
pub fn vehicle_in(
    payload: &[u8],
    length: i32,
    fields: &mut VehicleSync,
    null: u32,
) -> Result<i32, SyncError> {
    if length < 10 {
        return Ok(length);
    }
    let packed = word(payload, 0)?;
    let speed = i32::from(word(payload, 2)? as i16) << 4;
    fields.dword_1ec = i32::from(word(payload, 4)? as i16) as u32;
    fields.dword_210 = i32::from(word(payload, 6)? as i16) as u32;
    fields.mode_1e8 = u32::from(packed & 0xf);
    fields.dword_1fc = i32::from(word(payload, 8)? as i16) as u32;
    fields.byte_208 = ((packed >> 4) & 1) as u8;
    fields.word_58c = packed >> 12;
    fields.word_20e = (packed >> 8) & 0xf;
    fields.dword_238 = u32::from((packed >> 5) & 3);
    fields.dword_220 = speed as u32;
    fields.dword_244 = speed as u32;
    hold_in(&payload[10..], length - 10, &mut fields.hold, null)?;
    Ok(length)
}

/// `004025C0`, the turret's (selector 11) `+C0`: `+18C` as the high WORD
/// and `+188`'s low WORD as the low.
pub fn turret_out(out: &mut [u8], room: i32, [dword_188, dword_18c]: [u32; 2]) -> i32 {
    if room < 4 {
        return -1;
    }
    let packed = (dword_18c << 16).wrapping_add(dword_188 & 0xffff);
    out[..4].copy_from_slice(&packed.to_le_bytes());
    4
}

/// `00402600`, its `+C4`, whatever the length: `+188` the low WORD and
/// `+18C` the high, sign-extended.
pub fn turret_in(payload: &[u8], fields: &mut [u32; 2]) -> Result<i32, SyncError> {
    let packed = dword(payload, 0)?;
    *fields = [packed & 0xffff, ((packed as i32) >> 16) as u32];
    Ok(4)
}

/// The charge's (selector 27) fields its sync moves: the handle of the
/// actor it sticks to (`+154`) and its offset there (`+158..+15D`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChargeSync {
    pub handle_154: u32,
    pub offset_158: [u16; 3],
}

/// `004560D0`: nothing while `+154` is the process null, else the handle
/// and the offset, 10 bytes. Its room checks are unsigned, and the handle
/// goes in before the second.
pub fn charge_out(out: &mut [u8], room: i32, fields: ChargeSync, null: u32) -> i32 {
    if fields.handle_154 == null {
        return 0;
    }
    let room = room as u32;
    if room < 4 {
        return -1;
    }
    out[..4].copy_from_slice(&fields.handle_154.to_le_bytes());
    if room - 4 < 6 {
        return -1;
    }
    for (index, value) in fields.offset_158.iter().enumerate() {
        out[4 + index * 2..6 + index * 2].copy_from_slice(&value.to_le_bytes());
    }
    10
}

/// `00456130`: a payload gives the handle and offset; none puts the process
/// null back at `+154`.
pub fn charge_in(
    payload: &[u8],
    length: i32,
    fields: &mut ChargeSync,
    null: u32,
) -> Result<i32, SyncError> {
    if length == 0 {
        fields.handle_154 = null;
        return Ok(0);
    }
    fields.handle_154 = dword(payload, 0)?;
    fields.offset_158 = [word(payload, 4)?, word(payload, 6)?, word(payload, 8)?];
    Ok(10)
}

/// The factory's (selector 15) fields its sync moves: the family's `+13C`
/// and `+140`, the shop state `+160`, the production WORDs `+17E..+182`
/// and the docked vehicle `+188`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FactorySync {
    pub family: [i32; 2],
    pub state_160: u32,
    pub words_17e: [u16; 3],
    pub handle_188: u32,
}

/// `00431AD0`: the family's byte (`0040DEF0`), then a flags byte: `+160`'s
/// low byte, `+17E`'s code at bit 3 (0 for 0, 2 for 0xFFFF, else 1 and the
/// WORD follows), 0x20 when `+180` follows, 0x40 when `+182` does and 0x80
/// when `+188` does (any handle but the process null).
pub fn factory_out(out: &mut [u8], room: i32, fields: FactorySync, null: u32) -> i32 {
    if room == 0 {
        return -1;
    }
    let first = family40d800_out(out, room, fields.family);
    if first <= 0 {
        return -1;
    }
    let (mut at, mut left) = (first as usize, (room - first) as u32);
    let [word_17e, word_180, word_182] = fields.words_17e;
    let code = match word_17e {
        0 => 0,
        0xffff => 2,
        _ => 1,
    };
    let docked = fields.handle_188 != null;
    if left < 1 {
        return -1;
    }
    out[at] = fields.state_160 as u8
        | code << 3
        | if word_180 != 0 { 0x20 } else { 0 }
        | if word_182 != 0 { 0x40 } else { 0 }
        | if docked { 0x80 } else { 0 };
    at += 1;
    left -= 1;
    for (value, follows) in [
        (word_17e, code == 1),
        (word_180, word_180 != 0),
        (word_182, word_182 != 0),
    ] {
        if follows {
            if left < 2 {
                return -1;
            }
            out[at..at + 2].copy_from_slice(&value.to_le_bytes());
            at += 2;
            left -= 2;
        }
    }
    if docked {
        if left < 4 {
            return -1;
        }
        out[at..at + 4].copy_from_slice(&fields.handle_188.to_le_bytes());
        at += 4;
    }
    at as i32
}

/// `00431C00`: the family's byte (`0040DF20`), then the flags byte. `+160`
/// takes its low three bits; `+17E` the WORD that follows for code 1, else
/// 0xFFFF for any other nonzero code and 0 for none; `+180` the WORD for bit
/// 0x20, else 0; `+182` the WORD for bit 0x40, else it stays; `+188` the
/// DWORD for bit 0x80, else the process null. Returns the bytes it used.
pub fn factory_in(payload: &[u8], fields: &mut FactorySync, null: u32) -> Result<i32, SyncError> {
    let mut at = family40d800_in(payload, &mut fields.family)? as usize;
    let flags = *payload.get(at).ok_or(SyncError::Short(at + 1))?;
    at += 1;
    let mut next = |size: usize| {
        let value = if size == 2 {
            word(payload, at).map(u32::from)
        } else {
            dword(payload, at)
        };
        at += size;
        value
    };
    fields.state_160 = u32::from(flags & 7);
    fields.words_17e[0] = match (flags >> 3) & 3 {
        1 => next(2)? as u16,
        0 => 0,
        _ => 0xffff,
    };
    fields.words_17e[1] = if flags & 0x20 != 0 {
        next(2)? as u16
    } else {
        0
    };
    if flags & 0x40 != 0 {
        fields.words_17e[2] = next(2)? as u16;
    }
    fields.handle_188 = if flags & 0x80 != 0 { next(4)? } else { null };
    Ok(at as i32)
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
