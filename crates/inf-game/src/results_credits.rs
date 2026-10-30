//! The final world's results page rolls the credits (`0042AED0`): the
//! lines of `005313F0..00531AA4` scroll up the surface, one row (5/100 of
//! its height) apart, faded in over four rows above the middle and out over
//! the last four, each centred on the surface's width.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

/// `005313F0`: twelve bytes a line, up to `00531AA4`: an H0 slot WORD, the
/// text's address when the slot is 0, and the bold byte `+8`.
const TABLE: u32 = 0x0053_13f0;
const TABLE_END: u32 = 0x0053_1aa4;
const ENTRY_BYTES: u32 = 12;
/// `00546A98`: the line naming the player, which the screen's constructor
/// (`0042AB00`) formats.
pub const PLAYER_LINE: u32 = 0x0054_6a98;
/// `0042AF34`: the roll moves `height / 14400` pixels a millisecond.
const SCROLL_DIVISOR: i32 = 14400;
/// `0042AF44`: the period's 143 rows, less the four of the top fade.
const ROWS: i32 = 143;
/// `0042AF7A`: the text's flags.
pub const TEXT_FLAGS: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditText {
    /// An H0 slot; out of the pool's range it draws nothing.
    H0(i16),
    /// A string in the executable.
    Literal(u32),
    /// `00546A98`.
    PlayerLine,
    /// Slot 0 and no address: a blank row.
    Blank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditLine {
    pub text: CreditText,
    /// `+8`: H6 style 4 when set, else 5.
    pub bold: bool,
}

/// One line `0042AED0` draws (`004A3AB0`), in H4 font 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditDraw {
    /// Its index in the table.
    pub line: usize,
    pub x: i32,
    pub y: i32,
    /// The style's color, its three low bytes faded.
    pub color: u32,
}

#[derive(Debug, Error)]
pub enum CreditsError {
    #[error("credits table: {0}")]
    Table(#[from] FrontendExecutableError),
    /// `0042AF61` divides by the period and `0042B03F`/`0042B05E` by the
    /// fade's four rows, which a surface under 20 pixels high makes zero.
    #[error("a {0}-pixel-high surface makes the credits divide by zero (0042AF61, 0042B03F)")]
    ZeroDivisor(i32),
}

/// What the final world's page rolls: the table's lines, the executable's
/// strings they name, and the player's name the screen's constructor read
/// (`0042AB00`'s `00463550`) for the line H0 `0x330` formats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalCredits {
    pub lines: Vec<CreditLine>,
    literals: Vec<(u32, Vec<u8>)>,
    pub player_name: Vec<u8>,
}

impl FinalCredits {
    pub fn load(
        executable: FrontendExecutable<'_>,
        player_name: Vec<u8>,
    ) -> Result<Self, CreditsError> {
        let lines = credit_lines(executable)?;
        let mut literals = Vec::new();
        for line in &lines {
            if let CreditText::Literal(address) = line.text {
                if !literals.iter().any(|(known, _)| *known == address) {
                    literals.push((address, c_string(executable, address)?));
                }
            }
        }
        Ok(Self {
            lines,
            literals,
            player_name,
        })
    }

    /// The executable's string at `address`, as the table names it.
    pub fn literal(&self, address: u32) -> Option<&[u8]> {
        self.literals
            .iter()
            .find(|(known, _)| *known == address)
            .map(|(_, text)| text.as_slice())
    }
}

/// H0 `0x330`, which `0042AB00` formats with the player's name.
pub const PLAYER_LINE_TEXT: i32 = 0x330;

fn c_string(executable: FrontendExecutable<'_>, address: u32) -> Result<Vec<u8>, CreditsError> {
    let mut text = Vec::new();
    loop {
        let byte = executable.mapped(address + text.len() as u32, 1)?.bytes[0];
        if byte == 0 {
            return Ok(text);
        }
        text.push(byte);
    }
}

/// The 143 lines of `005313F0..00531AA4`.
pub fn credit_lines(executable: FrontendExecutable<'_>) -> Result<Vec<CreditLine>, CreditsError> {
    let count = (TABLE_END - TABLE) / ENTRY_BYTES;
    (0..count)
        .map(|index| {
            let entry = executable.mapped(TABLE + index * ENTRY_BYTES, ENTRY_BYTES as usize)?;
            let bytes = entry.bytes;
            let slot = i16::from_le_bytes([bytes[0], bytes[1]]);
            let address = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let text = match (slot, address) {
                (0, 0) => CreditText::Blank,
                (0, PLAYER_LINE) => CreditText::PlayerLine,
                (0, address) => CreditText::Literal(address),
                (slot, _) => CreditText::H0(slot),
            };
            Ok(CreditLine {
                text,
                bold: bytes[8] != 0,
            })
        })
        .collect()
}

/// `0042AED0` over a `width` by `height` surface, `age` milliseconds into
/// the page. `text` resolves a line (None draws nothing, as a NULL does) and
/// `measure` is `004A4240`'s width; `styles` are H6 styles 4 and 5. Each line
/// still above the top fade's start is skipped; the first past the bottom
/// ends the roll.
pub fn roll_credits<'t, E: From<CreditsError>>(
    lines: &[CreditLine],
    mut text: impl FnMut(CreditText) -> Option<&'t [u8]>,
    mut measure: impl FnMut(&[u8]) -> Result<i16, E>,
    [width, height]: [i32; 2],
    age: u32,
    [bold, plain]: [u32; 2],
) -> Result<Vec<CreditDraw>, E> {
    let row = height.wrapping_mul(5) / 100;
    let fade = row.wrapping_mul(4);
    let bottom = height.wrapping_sub(fade);
    let top = height / 2 + fade / 2;
    let scroll = height.wrapping_mul(age as i32) / SCROLL_DIVISOR;
    let period = row.wrapping_mul(ROWS).wrapping_sub(fade).wrapping_add(top) as u32;
    let offset = (scroll as u32)
        .checked_rem(period)
        .ok_or(CreditsError::ZeroDivisor(height))?;
    let mut draws = Vec::new();
    let mut at = 0i32;
    for (index, line) in lines.iter().enumerate() {
        let advance = at;
        at = at.wrapping_add(row);
        let Some(bytes) = text(line.text) else {
            continue;
        };
        let y = height.wrapping_add(advance.wrapping_sub(offset as i32));
        if y <= top.wrapping_sub(fade) {
            continue;
        }
        if y > bottom.wrapping_add(fade) {
            break;
        }
        let faded = |depth: i32| -> Result<i32, E> {
            let level = (depth << 8)
                .checked_div(fade)
                .ok_or(CreditsError::ZeroDivisor(height))?
                + 0x100;
            Ok(level.max(0))
        };
        let level = if y > bottom {
            faded(bottom.wrapping_sub(y))?
        } else if y < top {
            faded(y.wrapping_sub(top))?
        } else {
            0x100
        };
        let style = if line.bold { bold } else { plain };
        let mut color = style.to_le_bytes();
        for byte in &mut color[..3] {
            *byte = ((i32::from(*byte) * level) >> 8) as u8;
        }
        let x = width.wrapping_sub(i32::from(measure(bytes)?)) / 2;
        draws.push(CreditDraw {
            line: index,
            x,
            y,
            color: u32::from_le_bytes(color),
        });
    }
    Ok(draws)
}

#[cfg(test)]
#[path = "results_credits_tests.rs"]
mod tests;
