//! The network game's message lines (`0054E460`): four lines of `0x54`
//! bytes, a microsecond timer and up to `0x50` bytes of text, with the head
//! at `0054E848`. `0045C8D0` takes a line for 7.5 seconds, `0045D7C0`
//! writes its text, and `0045C1D0` (from the player's HUD `0046BE00`) counts
//! them down and draws the live ones.

use super::game::GameNetState;

pub const LINES: usize = 4;
pub const LINE_TEXT: usize = 0x50;
/// `0045C934`: 7.5 seconds.
const LINE_MICROS: i32 = 7_500_000;
/// `0045C1E1`: a tick takes 0.1 s off each line.
const LINE_STEP: i32 = 100_000;
/// `0045C2F5..0045C30E`: the lines' X, the first's Y and the step.
const LINE_X: i32 = 0x14;
const LINE_Y: i32 = 0x3c;
const LINE_Y_STEP: i32 = 0x12;
const LINE_Y_END: i32 = 0x84;
/// `0045C27C`: H6 style `0x2A`, in H4 font 0.
pub const LINE_STYLE: i32 = 0x2a;
/// `00538398`, an absent slot's name.
const STRANGER: &[u8] = b"a stranger";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageLine {
    pub timer: i32,
    pub text: [u8; LINE_TEXT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageLines {
    pub lines: [MessageLine; LINES],
    pub head: u32,
}

impl Default for MessageLines {
    fn default() -> Self {
        Self {
            lines: [MessageLine {
                timer: 0,
                text: [0; LINE_TEXT],
            }; LINES],
            head: 0,
        }
    }
}

impl MessageLines {
    /// `0045C8D0`: the first idle line from the head, else the head's (the
    /// head moving on), for 7.5 seconds.
    pub fn allocate(&mut self) -> usize {
        let index = (0..LINES as u32)
            .map(|k| ((self.head + k) & 3) as usize)
            .find(|&index| self.lines[index].timer == 0)
            .unwrap_or_else(|| {
                let index = self.head as usize;
                self.head = (self.head + 1) & 3;
                index
            });
        self.lines[index].timer = LINE_MICROS;
        index
    }

    /// `0045C1D0`'s state: each line loses 0.1 s (stopping at 0), then the
    /// head steps past idle lines (four looks). Returns the lines to draw
    /// and where: from the head, while a line keeps more than 0.1 s, at
    /// (20, 60) and 18 lower each, at most four.
    pub fn tick(&mut self) -> Vec<(usize, [i32; 2])> {
        for line in &mut self.lines {
            line.timer = if line.timer > LINE_STEP {
                line.timer - LINE_STEP
            } else {
                0
            };
        }
        let mut head = self.head;
        for _ in 0..LINES {
            if self.lines[head as usize].timer == 0 {
                head = (head + 1) & 3;
            }
        }
        self.head = head;
        if self.lines[head as usize].timer == 0 {
            return Vec::new();
        }
        let mut drawn = Vec::new();
        let (mut y, mut k) = (LINE_Y, 0);
        while y < LINE_Y_END {
            let index = ((head + k) & 3) as usize;
            if self.lines[index].timer <= LINE_STEP {
                break;
            }
            drawn.push((index, [LINE_X, y]));
            y += LINE_Y_STEP;
            k += 1;
        }
        drawn
    }
}

/// `0045D7C0`: `format` into at most `room - 1` bytes, `@1` and `@2` being
/// slot `a`'s and `b`'s names (or "a stranger" for a slot whose details
/// never arrived), or with `strings` the H0 strings `0x38 + a` and `0x38 + b`
/// (`h0` answers None past the pool's count). Another byte after `@`, or
/// the digit of a NULL string, is kept and the `@` dropped.
pub fn format_line(
    format: &[u8],
    room: usize,
    [a, b]: [i32; 2],
    strings: bool,
    state: &GameNetState,
    h0: impl Fn(i32) -> Option<Vec<u8>>,
) -> Vec<u8> {
    let terminated = |text: &[u8]| text.split(|&c| c == 0).next().unwrap_or(&[]).to_vec();
    let name = |slot: i32| -> Option<Vec<u8>> {
        if strings {
            let id = i32::from((slot.wrapping_add(0x38)) as i16);
            if id < 0 {
                return None;
            }
            return h0(id).map(|text| terminated(&text));
        }
        let slot = slot as usize;
        if !state.present.get(slot).is_some_and(|&p| p != 0) {
            return Some(STRANGER.to_vec());
        }
        // A name without a NUL in its 16 bytes runs on into the next slot's.
        let names: Vec<u8> = state.names.iter().flatten().copied().collect();
        Some(terminated(&names[slot * 16..]))
    };
    let names = [name(a), name(b)];
    let limit = room.saturating_sub(1);
    let mut out = Vec::new();
    let format = terminated(format);
    let mut at = 0;
    while at < format.len() && out.len() < limit {
        let byte = format[at];
        at += 1;
        if byte != b'@' {
            out.push(byte);
            continue;
        }
        let which = match format.get(at) {
            Some(b'1') => 0,
            Some(b'2') => 1,
            _ => continue,
        };
        // A NULL string leaves the digit to be copied as it stands.
        let Some(text) = &names[which] else {
            continue;
        };
        at += 1;
        for &c in text {
            if out.len() >= limit {
                break;
            }
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
