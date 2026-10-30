//! The world captions of the gameplay draw `00440F00` (`0044121E..00441456`),
//! between the world-22 overlay and the HUD pass. Each is one `004A3AB0`
//! line in H4 font 0 and H6 style 0:
//!
//! - "Bonus Time: N" at (10, 40) while `0048E290`'s count is positive (H14
//!   header +0 less 20 per animation tick; +0 is zero in every authored
//!   world, so it never shows offline);
//! - unless the first player's message box is up (`00488AA0`, byte +88):
//!   "Countdown: S.T" while the mission's doom countdown W+24 runs (not in
//!   its second stage W+18, nor in the attract demo `0055F14C`), centred a
//!   tenth of the way down (`00440E20`), then W+2A's H0 text and the count
//!   W+28 ("Warp power hoops: N", from a gate's `0048B411`) at 14/100.
//!
//! A text past the H0 pool's count prints as sprintf's "(null)".

/// H0 0x113 "Bonus Time" and 0x114 "Countdown".
const BONUS_TEXT: i32 = 0x113;
const COUNTDOWN_TEXT: i32 = 0x114;
/// `00441264..00441278`: the bonus line's position.
const BONUS_POSITION: [i32; 2] = [10, 0x28];

/// What the segment reads.
#[derive(Clone, Copy, Debug)]
pub struct WorldCaptionInputs {
    pub surface: [i32; 2],
    /// H14 header +0.
    pub bonus_base: u32,
    /// The animation clock `005471C0`.
    pub clock: u32,
    /// The first player's message byte +88.
    pub message_shown: bool,
    /// `0055F14C`.
    pub attract: bool,
    /// Mission W+18, W+24 (microseconds), W+28 and W+2A.
    pub doom_stage: u8,
    pub countdown: u32,
    pub count: u16,
    pub count_text: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldCaption {
    pub text: Vec<u8>,
    pub position: [i32; 2],
}

/// `0048E290`: the bonus left, nothing once negative or without a base.
pub fn bonus_time(base: u32, clock: u32) -> i32 {
    if base == 0 {
        return 0;
    }
    (base.wrapping_sub(clock.wrapping_mul(20)) as i32).max(0)
}

/// The captions in draw order. `string` is `00405540` on the H0 pool (None
/// past its count); `width` is `004A4240` in font 0.
pub fn plan_world_captions<E>(
    inputs: &WorldCaptionInputs,
    string: impl Fn(i32) -> Result<Option<Vec<u8>>, E>,
    width: impl Fn(&[u8]) -> Result<i32, E>,
) -> Result<Vec<WorldCaption>, E> {
    let line = |name: Option<Vec<u8>>, value: String| {
        let mut text = name.unwrap_or_else(|| b"(null)".to_vec());
        text.extend_from_slice(format!(": {value}").as_bytes());
        text
    };
    // 00440E20: centred, a tenth (row 0) or 14/100 (row 1) of the way down.
    let [w, h] = inputs.surface;
    let centred = |text: Vec<u8>, row: bool| -> Result<WorldCaption, E> {
        let measured = i32::from(width(&text)? as i16);
        let y = if row {
            h.wrapping_mul(14) / 100
        } else {
            h / 10
        };
        Ok(WorldCaption {
            position: [w.wrapping_sub(measured) / 2, y],
            text,
        })
    };
    let mut captions = Vec::new();
    let bonus = bonus_time(inputs.bonus_base, inputs.clock);
    if bonus != 0 {
        captions.push(WorldCaption {
            text: line(string(BONUS_TEXT)?, bonus.to_string()),
            position: BONUS_POSITION,
        });
    }
    if inputs.message_shown {
        return Ok(captions);
    }
    let countdown = if inputs.doom_stage != 0 {
        0
    } else {
        inputs.countdown
    };
    if countdown != 0 && !inputs.attract {
        let millis = countdown as i32 / 1000;
        let value = format!("{}.{}", millis / 1000, millis / 100 % 10);
        captions.push(centred(line(string(COUNTDOWN_TEXT)?, value), false)?);
    }
    let count = inputs.count as i16;
    if count != 0 {
        let id = inputs.count_text as i16;
        let name = if id < 0 { None } else { string(i32::from(id))? };
        captions.push(centred(line(name, count.to_string()), true)?);
    }
    Ok(captions)
}

#[cfg(test)]
mod tests;
