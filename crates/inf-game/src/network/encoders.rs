//! The game protocol's encoders (rows `+4` of `00538050`) whose payload
//! comes from their argument or the game state. Each writes into the
//! caller's buffer and returns the payload's length, or -1 when the room it
//! is given runs out; `00492500` always gives `0x200`.

use super::game::{GameNetError, GameNetState};

fn put(out: &mut [u8], at: usize, bytes: &[u8]) {
    out[at..at + bytes.len()].copy_from_slice(bytes);
}

/// `0045C140`, message 0: nothing to send.
pub fn encode_nothing() -> i32 {
    0
}

/// `0045E0B0`: the argument, a handle, without a room check.
pub fn encode_handle(out: &mut [u8], value: u32) -> i32 {
    put(out, 0, &value.to_le_bytes());
    4
}

/// `0045ED70`: two handles, the argument's second first.
pub fn encode_pair(out: &mut [u8], room: u32, pair: [u32; 2]) -> i32 {
    if room < 4 {
        return -1;
    }
    put(out, 0, &pair[1].to_le_bytes());
    if room - 4 < 4 {
        return -1;
    }
    put(out, 4, &pair[0].to_le_bytes());
    8
}

/// `0045DF70`: damage, the record's first DWORD, the target and the amount.
pub fn encode_damage(out: &mut [u8], room: u32, damage: [u32; 3]) -> i32 {
    let mut room = room;
    for (index, value) in damage.iter().enumerate() {
        if room < 4 {
            return -1;
        }
        put(out, index * 4, &value.to_le_bytes());
        room -= 4;
    }
    12
}

/// `0045DFF0`: a world effect, a DWORD, a 6-byte point and a 10-byte record.
pub fn encode_effect(
    out: &mut [u8],
    room: u32,
    value: u32,
    point: [u8; 6],
    record: [u8; 10],
) -> i32 {
    if room < 4 {
        return -1;
    }
    put(out, 0, &value.to_le_bytes());
    if room - 4 < 6 {
        return -1;
    }
    put(out, 4, &point);
    if room - 10 < 10 {
        return -1;
    }
    put(out, 10, &record);
    20
}

/// `0045E960` and `0045EA60`: an effect between two 6-byte points, its kind
/// DWORD, and for `0045EA60` an extra DWORD.
pub fn encode_points(
    out: &mut [u8],
    room: u32,
    points: [[u8; 6]; 2],
    kind: u32,
    extra: Option<u32>,
) -> i32 {
    if room < 6 {
        return -1;
    }
    put(out, 0, &points[0]);
    if room - 6 < 6 {
        return -1;
    }
    put(out, 6, &points[1]);
    if room - 12 < 4 {
        return -1;
    }
    put(out, 12, &kind.to_le_bytes());
    let Some(extra) = extra else {
        return 16;
    };
    if room - 16 < 4 {
        return -1;
    }
    put(out, 16, &extra.to_le_bytes());
    20
}

/// `0045BC60`: a capture, the banner's handle `+84`, then its `+140` and
/// `+13C`.
pub fn encode_capture(out: &mut [u8], room: u32, banner: [u32; 3]) -> i32 {
    encode_damage(out, room, banner)
}

/// `0045BD70`: a banner event, its handle `+84` and its `+144`.
pub fn encode_banner(out: &mut [u8], room: u32, banner: [u32; 2]) -> i32 {
    if room < 4 {
        return -1;
    }
    put(out, 0, &banner[0].to_le_bytes());
    if room - 4 < 4 {
        return -1;
    }
    put(out, 4, &banner[1].to_le_bytes());
    8
}

/// `00403D60` and `00403DF0`: Powerball's ball let go, its holder `+14C`
/// and the throw's sequence `+17C`, and for 44 the throw `+176..+17A`;
/// a field the room cannot hold ends it at -1 after the ones before.
pub fn encode_ball(
    out: &mut [u8],
    room: u32,
    holder: u32,
    sequence: u16,
    throw: Option<[i16; 3]>,
) -> i32 {
    if room <= 3 {
        return -1;
    }
    put(out, 0, &holder.to_le_bytes());
    if room.wrapping_sub(4) <= 1 {
        return -1;
    }
    put(out, 4, &sequence.to_le_bytes());
    let Some(throw) = throw else {
        return 6;
    };
    if room.wrapping_sub(6) <= 5 {
        return -1;
    }
    for (index, value) in throw.iter().enumerate() {
        put(out, 6 + 2 * index, &value.to_le_bytes());
    }
    12
}

/// `0045D7A0`, `0045D780` and `0045ED10`: one DWORD (this machine's kill
/// word `0054E804`, a transfer, a value).
pub fn encode_dword(out: &mut [u8], room: u32, value: u32) -> i32 {
    if room < 4 {
        return -1;
    }
    put(out, 0, &value.to_le_bytes());
    4
}

/// `0045D100`: this slot's details and name, 22 bytes; it reports 32, the
/// rest left as the buffer held it.
pub fn encode_details(out: &mut [u8], room: u32, state: &GameNetState, slot: u8) -> i32 {
    if (room as i32) < 0x20 {
        return -1;
    }
    let slot = usize::from(slot);
    put(out, 0, &state.details[slot]);
    put(out, 6, &state.names[slot]);
    0x20
}

/// `0045D0B0`: a text of up to 200 bytes with its NUL, after its length; a
/// NUL follows the copied bytes, past the reported length.
pub fn encode_text(out: &mut [u8], room: u32, text: &[u8]) -> i32 {
    let length = (text.len() + 1).min(200);
    if (length + 1) as i32 > room as i32 {
        return -1;
    }
    out[0] = length as u8;
    let copied = length.min(text.len());
    put(out, 1, &text[..copied]);
    out[1 + copied..=1 + length].fill(0);
    (length + 1) as i32
}

/// `0045DA90`: a sound, its id's low byte and the high bytes of its
/// position's WORDs, then the DWORDs its flags (`+10`) name.
pub fn encode_sound(
    out: &mut [u8],
    room: u32,
    id: u32,
    position: [u16; 3],
    values: [u32; 2],
    flags: u8,
) -> i32 {
    if room < 4 {
        return -1;
    }
    let high = position.map(|word| u32::from(word >> 8));
    put(
        out,
        0,
        &(id & 0xff | high[0] << 8 | high[1] << 16 | high[2] << 24).to_le_bytes(),
    );
    let (mut length, mut room) = (4, room - 4);
    for (bit, value) in [(1, values[0]), (2, values[1])] {
        if flags & bit == 0 {
            continue;
        }
        if room < 4 {
            return -1;
        }
        put(out, length, &value.to_le_bytes());
        length += 4;
        room -= 4;
    }
    length as i32
}

/// `0045ECA0`: the current world's four DWORDs of the mission's table at
/// `+B0` (`0048EA10`).
pub fn encode_world_values(out: &mut [u8], room: u32, values: [u32; 4]) -> i32 {
    let mut room = room;
    for (index, value) in values.iter().enumerate() {
        if room < 4 {
            return -1;
        }
        put(out, index * 4, &value.to_le_bytes());
        room -= 4;
    }
    16
}

/// Bytes copied as they are (`00475BB0`, `00475C40`, `00475DB0`), which
/// retail writes without checking the room.
pub fn encode_bytes(out: &mut [u8], bytes: &[u8]) -> i32 {
    match out.get_mut(..bytes.len()) {
        Some(room) => {
            room.copy_from_slice(bytes);
            bytes.len() as i32
        }
        None => -1,
    }
}

/// `0045D5D0`, the host's game state: `0054E820..0054E82B` as three DWORDs,
/// a mask of the present slots 1 to 16 and one of the teams they play for
/// (states 2 to 17), then each present slot's kills, deaths and score, and
/// each named team's five words from `+0C`.
pub fn encode_game_state(
    out: &mut [u8],
    room: u32,
    state: &GameNetState,
) -> Result<i32, GameNetError> {
    if room < 12 {
        return Ok(-1);
    }
    let dword_820 = u32::from(state.score_limit)
        | u32::from(state.winner) << 16
        | u32::from(state.byte_823) << 24;
    let dword_828 = u32::from(state.team_game)
        | u32::from(state.ended) << 8
        | u32::from(state.bytes_82a[0]) << 16
        | u32::from(state.bytes_82a[1]) << 24;
    put(out, 0, &dword_820.to_le_bytes());
    put(out, 4, &state.winning_team.to_le_bytes());
    put(out, 8, &dword_828.to_le_bytes());
    let mut room = room - 12;
    let (mut present, mut teams) = (0u16, 0u16);
    for slot in 1..=16 {
        if state.present[slot] == 0 {
            continue;
        }
        present |= 1 << (slot - 1);
        let team = state.states[slot].wrapping_sub(2);
        if team < 16 {
            teams |= 1 << team;
        }
    }
    let mut length = 12;
    for mask in [present, teams] {
        if room < 2 {
            return Ok(-1);
        }
        put(out, length, &mask.to_le_bytes());
        length += 2;
        room -= 2;
    }
    for slot in 1..=16 {
        if state.present[slot] == 0 {
            continue;
        }
        if room < 6 {
            return Ok(-1);
        }
        for (index, value) in state.scores[slot].iter().enumerate() {
            put(out, length + index * 2, &value.to_le_bytes());
        }
        length += 6;
        room -= 6;
    }
    for team in 0..16 {
        if teams & 1 << team == 0 {
            continue;
        }
        let index = team + 2;
        let object = state.teams[index].ok_or(GameNetError::MissingTeam {
            slot: 0,
            state: index as u32,
        })?;
        let words = [
            object.words_0c[0],
            object.words_0c[1],
            object.score,
            object.kills,
            object.deaths,
        ];
        for word in words {
            if room < 2 {
                return Ok(-1);
            }
            put(out, length, &word.to_le_bytes());
            length += 2;
            room -= 2;
        }
    }
    Ok(length as i32)
}

#[cfg(test)]
#[path = "encoders_tests.rs"]
mod tests;
