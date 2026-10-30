//! The network game over the world: `0046BE00`'s online branch
//! (`0046C016..0046C5F1`), the message lines (`0045C1D0`, which
//! [`crate::network::lines`] keeps) and the race's status, each an H0
//! string in H6 style 0x28 centred on its point of the surface; and
//! `00442080`'s network section after the world draw (`004421E6..00442B2E`):
//! the game's end, the scoreboards `00441510` and their bars.

use crate::network::game::{GameNetState, SLOTS};
use crate::object_runtime::RaceState;
use crate::scene_sprite::{DirectSpriteMode, DirectSpriteQuad};

/// `0046C02F`: the status texts' H6 style.
pub const STATUS_STYLE: i32 = 0x28;

/// H0 "Wrong Way", twice ("Wrong       Way" spread out).
const WRONG_WAY: [u16; 2] = [0x83, 0x84];
/// H0 "Waiting", "Starting", "Check Point Missed" (twice) and "Press F10 to
/// start".
const WAITING: u16 = 0x85;
const STARTING: u16 = 0x86;
const MISSED: [u16; 2] = [0x87, 0x88];
const PRESS_F10: u16 = 0x89;
/// `0046C1DD` and `0046C24E`: the multipliers that divide by 11718, the
/// countdown's whole span, the first toward the top.
const UP: i32 = 0xa684_0aa9_u32 as i32;
const DOWN: i32 = 0x597b_f557;

/// What the race's status reads: the race's start `+2D4`, the player's
/// wrong-way time `+19C` and missed-gate time `+198`, and `0055F3A0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaceStatus {
    pub countdown: u16,
    pub wrong_way_19c: i32,
    pub missed_198: u32,
    pub host: bool,
}

/// `IMUL` by a magic multiplier, `SAR 12` of the high DWORD and the sign
/// bit added: a division rounded toward zero.
fn magic(value: i32, multiplier: i32) -> i32 {
    let high = ((i64::from(value) * i64::from(multiplier)) >> 32) as i32 >> 12;
    high + (high as u32 >> 31) as i32
}

/// `0046C052..0046C437`: the race's texts and the points each centres on,
/// by the animation clock `005471C0`'s low byte. While the race runs, the
/// player told it goes the wrong way sees it at 30/128 of the height and a
/// missed gate at 40/128, each alternating its two spellings; an idle race
/// blinks "Press F10 to start" for the host and "Waiting" for a guest at
/// the middle; a countdown blinks "Starting" at the middle and twice more,
/// sliding in from the top and the bottom as it runs out.
pub fn race_status_texts(
    status: RaceStatus,
    clock: u32,
    [width, height]: [i32; 2],
) -> Vec<(u16, [i32; 2])> {
    let x = width / 2;
    let blink = |mask: u32| clock & mask != 0;
    let mut texts = Vec::new();
    match status.countdown {
        0 => {
            if status.wrong_way_19c != 0 {
                let y = height.wrapping_mul(30) / 128;
                texts.push((WRONG_WAY[usize::from(!blink(0x30))], [x, y]));
            }
            if status.missed_198 != 0 {
                let y = height.wrapping_mul(40) / 128;
                texts.push((MISSED[usize::from(!blink(0x30))], [x, y]));
            }
        }
        0xffff => {
            if blink(0x30) {
                let text = if status.host { PRESS_F10 } else { WAITING };
                texts.push((text, [x, height / 2]));
            }
        }
        countdown => {
            if blink(0x0c) {
                let half = height / 2;
                let spread = i32::from(countdown).wrapping_mul(half);
                for multiplier in [None, Some(UP), Some(DOWN)] {
                    let y = half.wrapping_add(multiplier.map_or(0, |m| magic(spread, m)));
                    texts.push((STARTING, [x, y]));
                }
            }
        }
    }
    texts
}

/// What Powerball's status at the middle shows (`0046C43C..0046C5EC`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerballStatus {
    /// An H0 line as it stands.
    Line(u16),
    /// H0 0x82, "%s team wins", with the winning side's team name (H0).
    Wins(u16),
}

/// `0046C43C..0046C5EC`, Powerball's status at the middle once a goal byte
/// has bit 7: while one side has won, the host blinks "%s team wins" (the
/// clock over 50 odd) with "Press F10" (H0 0x89), which is all it shows
/// while both are idle; a guest sees "Waiting" (H0 0x85).
pub fn powerball_status(goals: [u8; 2], host: bool, clock: u32) -> Option<PowerballStatus> {
    let won = goals.map(|goal| goal & 0x80 != 0);
    if !won[0] && !won[1] {
        return None;
    }
    if !host {
        return Some(PowerballStatus::Line(WAITING));
    }
    if won[0] != won[1] && (clock as i32 / 50) % 2 != 0 {
        return Some(PowerballStatus::Wins(if won[0] { 0x3a } else { 0x3b }));
    }
    Some(PowerballStatus::Line(PRESS_F10))
}

/// `004424BB..00442737` and `00441851`: the full board's backdrop and the
/// bars, each quad's vertex DWORDs 0x40 and packet word 2.
const BACKDROP_SPRITE: u16 = 0x2a6;
const BAR_SPRITE: u16 = 0x230;
const QUAD_DEPTH: u32 = 0x40;
const QUAD_WORD: u32 = 2;
/// H0 "%s team wins", "Still choosing team", and the team names from 0x38.
const TEAM_WINS: i32 = 0x82;
const STILL_CHOOSING: i32 = 0x8a;
const TEAM_NAMES: u16 = 0x38;
/// H6: the teams' styles from 0x25 (by state), the players' from 0x26 (by
/// slot).
const TEAM_STYLES: u16 = 0x25;
const PLAYER_STYLES: u16 = 0x26;

/// What `00442080`'s network section and `00441510` read.
#[derive(Clone, Copy, Debug)]
pub struct OverlayInputs<'a> {
    pub surface: [i32; 2],
    /// The animation clock `005471C0`.
    pub clock: u32,
    /// Settings `+10`, the game mode.
    pub mode: u32,
    /// `0055F3A0` and `0055F2F4`.
    pub host: bool,
    pub local_slot: u8,
    /// Screen `+F8`, Backspace held (`00440940`).
    pub scores_held: bool,
    pub game: &'a GameNetState,
    /// `0054ED70`.
    pub race: Option<&'a RaceState>,
    /// Each carried banner's carrier slot and team, in actor-list order
    /// ([`crate::object_runtime::ObjectArena::banner_carriers`]), which
    /// Capture the Banner's boards flash.
    pub carriers: &'a [(u8, u32)],
    /// Powerball's goal bytes `0052E528`/`0052E529` (bit 7 a won side),
    /// which its boards count, and its ball's holder `+14C` (`005468D0`),
    /// whose slot's row flashes.
    pub goals: [u8; 2],
    pub ball_holder: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverlayOp {
    Quad(DirectSpriteQuad),
    /// One `004A3AB0` line in H4 font 0 from its left end.
    Text {
        text: Vec<u8>,
        position: [i32; 2],
        color_word: u32,
    },
}

/// The H0 pool (None past its count), `004A4240` in font 0 and the H6
/// style words.
pub struct OverlayResources<S, W, C> {
    pub string: S,
    pub width: W,
    pub style: C,
}

fn quad(sprite_slot: u16, corners: [[i32; 2]; 4]) -> OverlayOp {
    OverlayOp::Quad(DirectSpriteQuad {
        sprite_slot,
        corners: corners.map(|corner| corner.map(|value| value as i16)),
        vertex_depths: [QUAD_DEPTH; 4],
        descriptor_word: QUAD_WORD,
        mode: DirectSpriteMode::Textured,
    })
}

/// `00441D27..00441D7B`: each colour byte 0x40 brighter, to at most 0xFF.
fn brighten(word: u32) -> u32 {
    let mut bytes = word.to_le_bytes();
    for byte in &mut bytes[..3] {
        *byte = byte.saturating_add(0x40);
    }
    u32::from_le_bytes(bytes)
}

/// `%s` of a slot's name or details: to the first NUL, which sprintf seeks
/// on through the next slots' fields.
fn field<const N: usize>(fields: &[[u8; N]; SLOTS], slot: usize) -> Result<Vec<u8>, String> {
    let rest = fields.get(slot..).unwrap_or_default();
    let text: Vec<u8> = rest
        .iter()
        .flatten()
        .copied()
        .take_while(|&byte| byte != 0)
        .collect();
    if text.len() == rest.len() * N {
        return Err(format!(
            "slot {slot}'s text runs to the end of its table with no NUL"
        ));
    }
    Ok(text)
}

/// An H0 team name: `state + 0x38` as a WORD, NULL when negative or past
/// the pool.
fn team_name<S, W, C>(
    resources: &OverlayResources<S, W, C>,
    state: u16,
) -> Result<Option<Vec<u8>>, String>
where
    S: Fn(i32) -> Result<Option<Vec<u8>>, String>,
{
    match state.wrapping_add(TEAM_NAMES) as i16 {
        index if index < 0 => Ok(None),
        index => (resources.string)(i32::from(index)),
    }
}

/// `004421E6..00442B2E` in draw order, online: the game's end blinking (the
/// winning team, then who restarts it), Backspace's backdrop, the top bar,
/// then the boards. A stop leaves the ops it reached in `ops`.
pub fn plan_network_overlay<S, W, C>(
    inputs: &OverlayInputs<'_>,
    resources: &OverlayResources<S, W, C>,
    ops: &mut Vec<OverlayOp>,
) -> Result<(), String>
where
    S: Fn(i32) -> Result<Option<Vec<u8>>, String>,
    W: Fn(&[u8]) -> Result<i32, String>,
    C: Fn(i32) -> Result<u32, String>,
{
    let [w, h] = inputs.surface;
    let game = inputs.game;
    let centred = |text: Vec<u8>, y: i32| -> Result<OverlayOp, String> {
        let measured = i32::from((resources.width)(&text)? as i16);
        Ok(OverlayOp::Text {
            position: [w.wrapping_sub(measured) / 2, y],
            color_word: (resources.style)(0)?,
            text,
        })
    };
    if game.ended != 0 && (inputs.clock as i32 / 50) % 2 != 0 {
        if game.team_game != 0 {
            let name = team_name(resources, game.winning_team as u16)?;
            let format = (resources.string)(TEAM_WINS)?
                .ok_or_else(|| crate::free_look_view::null_format(TEAM_WINS))?;
            let text = crate::free_look_view::sprintf(&format, &[name.as_deref()])?;
            ops.push(centred(text, h / 3)?);
        }
        let id = if inputs.host { PRESS_F10 } else { WAITING };
        let text = (resources.string)(i32::from(id))?
            .ok_or_else(|| format!("H0 {id:#x} is past the pool, which retail draws through"))?;
        ops.push(centred(text, (h / 3).wrapping_add(h / 10))?);
    }
    if inputs.scores_held {
        let [x, y] = [w / 2, h / 2];
        let r = w.wrapping_mul(45) / 100;
        ops.extend(
            [
                [[x - r, y], [x - r, y - r], [x, y - r], [x, y]],
                [[x, y - r], [x + r, y - r], [x + r, y], [x, y]],
                [[x + r, y], [x + r, y + r], [x, y + r], [x, y]],
                [[x, y + r], [x - r, y + r], [x - r, y], [x, y]],
            ]
            .map(|corners| quad(BACKDROP_SPRITE, corners)),
        );
    }
    let h4 = h.wrapping_mul(4) / 100;
    ops.push(quad(BAR_SPRITE, [[w, 0], [0, 0], [0, h4], [w, h4]]));
    let board = Board { inputs, resources };
    if inputs.mode == 4 {
        // 0044274C..004428CC: each side's compact board from a fifth of the
        // width, team 2's, a bar, then team 3's; while Backspace is held,
        // every team's full board from a fifth of the height, then the
        // slots still choosing.
        let (x, column) = (w.wrapping_mul(20) / 100, w.wrapping_mul(30) / 256);
        let below = board
            .draw(true, 2, [x, h.wrapping_mul(3) / 100], column, h4, ops)?
            .wrapping_add(h4);
        let [top, bottom] = [
            below.wrapping_add(h / 100),
            below.wrapping_sub(h.wrapping_mul(3) / 100),
        ];
        ops.push(quad(
            BAR_SPRITE,
            [[w, top], [0, top], [0, bottom], [w, bottom]],
        ));
        board.draw(true, 3, [x, below], column, h4, ops)?;
        if inputs.scores_held {
            let (x, mut y) = (w.wrapping_mul(50) / 100, h.wrapping_mul(20) / 100);
            for team in (2..=0x13).chain([1]) {
                y = board.draw(false, team, [x, y], 0, h4, ops)?;
            }
        }
        return Ok(());
    }
    let compact = |row, ops: &mut Vec<OverlayOp>| {
        let at = [w.wrapping_mul(8) / 100, h.wrapping_mul(3) / 100];
        board.draw(true, -1, at, w.wrapping_mul(30) / 256, row, ops)
    };
    if game.team_game == 0 {
        let (mut x, mut row) = (w.wrapping_mul(60) / 100, h4);
        if inputs.mode == 3 {
            let h8 = h.wrapping_mul(8) / 100;
            ops.push(quad(BAR_SPRITE, [[w, h8], [0, h8], [0, h4], [w, h4]]));
            (x, row) = (w.wrapping_mul(50) / 100, h8);
        }
        compact(row, ops)?;
        if inputs.scores_held {
            board.draw(false, 0, [x, h.wrapping_mul(20) / 100], 0, h4, ops)?;
        }
    } else {
        if inputs.scores_held {
            let x = w.wrapping_mul(50) / 100;
            let mut y = h.wrapping_mul(14) / 100;
            for team in (2..=0x13).chain([1]) {
                y = board.draw(false, team, [x, y], 0, h4, ops)?;
            }
        }
        compact(h4, ops)?;
    }
    Ok(())
}

struct Board<'i, 'a, 'r, S, W, C> {
    inputs: &'i OverlayInputs<'a>,
    resources: &'r OverlayResources<S, W, C>,
}

impl<S, W, C> Board<'_, '_, '_, S, W, C>
where
    S: Fn(i32) -> Result<Option<Vec<u8>>, String>,
    W: Fn(&[u8]) -> Result<i32, String>,
    C: Fn(i32) -> Result<u32, String>,
{
    fn width(&self, text: &[u8]) -> Result<i32, String> {
        Ok(i32::from((self.resources.width)(text)? as i16))
    }

    /// `0052E526 + team`'s count: Powerball's goals for team 2 or 3. Other
    /// teams read the bytes past them.
    fn goals(&self, team: i32) -> Result<u8, String> {
        match team {
            2 | 3 => Ok(self.inputs.goals[team as usize - 2] & 0x7f),
            _ => Err(format!(
                "Powerball's board for team {team} reads {:#x}, past the goal bytes 0052E528",
                0x52e526 + team
            )),
        }
    }

    /// An H6 style word from a WORD index; retail reads through NULL past
    /// the pool.
    fn style(&self, index: u16) -> Result<u32, String> {
        (self.resources.style)(i32::from(index as i16))
    }

    /// `00441510`: the present slots by score, highest first (ties in slot
    /// order), of one team (`team` above 0) or all. A compact board puts
    /// each slot's details `column` right of the last, under a new bar when
    /// "MMMM" would pass the edge, and a race's total time under them. A
    /// full one gives each slot "name (details)" right-aligned on `x` and
    /// its stats from `x`, a row each, under its team's line in a team
    /// game. Returns the next row's `y`.
    fn draw(
        &self,
        compact: bool,
        team: i32,
        [x, y]: [i32; 2],
        column: i32,
        row: i32,
        ops: &mut Vec<OverlayOp>,
    ) -> Result<i32, String> {
        let inputs = self.inputs;
        let game = inputs.game;
        let [w, h] = inputs.surface;
        let mut order: Vec<usize> = Vec::new();
        for slot in (1..SLOTS).filter(|&slot| game.present[slot] != 0) {
            let score = game.scores[slot][2];
            let at = order
                .iter()
                .position(|&other| game.scores[other][2] < score)
                .unwrap_or(order.len());
            order.insert(at, slot);
        }
        let mut carried = [0u32; SLOTS];
        if inputs.mode == 0 {
            for &(slot, team) in inputs.carriers {
                *carried.get_mut(usize::from(slot)).ok_or_else(|| {
                    format!(
                        "a banner's carrier slot {slot} is past the board's, where retail \
                         writes past its array"
                    )
                })? = team;
            }
        }
        let tick = inputs.clock as i32 / 5;
        let (mut x, mut y, mut header) = (x, y, compact);
        // 004416BD..004417AB: Powerball's compact board opens with its
        // side's "%s : %02d :" right-aligned on `x`, then moves on by "MMM".
        if compact && inputs.mode == 4 {
            let mut text =
                team_name(self.resources, team as u16)?.unwrap_or_else(|| b"(null)".to_vec());
            text.extend(format!(" : {:02} :", self.goals(team)?).bytes());
            ops.push(OverlayOp::Text {
                position: [x.wrapping_sub(self.width(&text)?), y],
                color_word: self.style((team as u16).wrapping_add(TEAM_STYLES))?,
                text,
            });
            x = x.wrapping_add(self.width(b"MMM")?);
        }
        let start = x;
        for slot in order {
            if compact && x > w.wrapping_sub(self.width(b"MMMM")?) {
                let mut top = h.wrapping_add(y.wrapping_mul(100)).wrapping_add(20) / 100;
                if inputs.race.is_some() {
                    top = top.wrapping_add(h.wrapping_mul(4) / 100);
                }
                let bottom = top.wrapping_add(row);
                ops.push(quad(
                    BAR_SPRITE,
                    [[w, top], [0, top], [0, bottom], [w, bottom]],
                ));
                x = start;
                y = y.wrapping_add(row);
            }
            let state = game.states[slot];
            if team > 0 && state != team as u32 {
                continue;
            }
            let playing = (2..=0x11).contains(&(state as i32));
            let mut color;
            if game.team_game != 0 {
                color = self.style((state as u16).wrapping_add(TEAM_STYLES))?;
                if !playing {
                    color |= 0x00ff_ffff;
                }
                if !header {
                    let text = if playing && inputs.mode == 4 {
                        // 00441A1A: Powerball's count is the board's team's.
                        let mut text = team_name(self.resources, state as u16)?
                            .unwrap_or_else(|| b"(null)".to_vec());
                        text.extend(format!(" : {:02}", self.goals(team)?).bytes());
                        text
                    } else if playing {
                        let scores = game
                            .teams
                            .get(state as usize)
                            .copied()
                            .flatten()
                            .ok_or_else(|| {
                                format!(
                                    "slot {slot}'s team {state} has no task (0055F100), which \
                                     retail reads through"
                                )
                            })?;
                        let mut text = team_name(self.resources, state as u16)?
                            .unwrap_or_else(|| b"(null)".to_vec());
                        let [won, lost] = scores.words_0c;
                        text.extend(
                            format!(" : {won:02}-{lost:02} : Score {:5}", scores.score).bytes(),
                        );
                        text
                    } else {
                        let mut text = (self.resources.string)(STILL_CHOOSING)?
                            .unwrap_or_else(|| b"(null)".to_vec());
                        text.extend(b" :");
                        text
                    };
                    let position = [
                        x.wrapping_sub(self.width(&text)? / 2),
                        y.wrapping_add(row / 2),
                    ];
                    ops.push(OverlayOp::Text {
                        text,
                        position,
                        color_word: color,
                    });
                    y = y.wrapping_add(row.wrapping_mul(2));
                    header = true;
                }
                // 00441BA2: Powerball's ball's holder brightens.
                let holder = inputs.ball_holder.map(|handle| (handle >> 27) as usize);
                if tick % 2 == 0 && inputs.mode == 4 && holder == Some(slot) {
                    color = brighten(color);
                }
                // 00441C1B: a banner's carrier flashes the banner's colour.
                else if tick % 8 == 0 && inputs.mode == 0 && carried[slot] != 0 {
                    color = self.style((carried[slot] as u16).wrapping_add(TEAM_STYLES))?;
                }
            } else {
                color = self.style((slot as u16).wrapping_add(PLAYER_STYLES))?;
                if tick % 2 == 0 && inputs.mode == 3 && slot == usize::from(inputs.local_slot) {
                    color = brighten(color);
                }
            }
            let details = field(&game.details, slot)?;
            let (text, left) = if compact {
                let left = x.wrapping_sub(self.width(&details)? / 2);
                (details, left)
            } else {
                let mut text = field(&game.names, slot)?;
                text.extend(b" (");
                text.extend(details);
                text.push(b')');
                let left = x.wrapping_sub(self.width(&text)?);
                (text, left)
            };
            ops.push(OverlayOp::Text {
                text,
                position: [left, y],
                color_word: color,
            });
            let stats = match (inputs.race, compact) {
                (Some(race), true) => {
                    let total = i32::from(race.records[slot].total_06);
                    let text = format!("{:02}:{:02}", total / 1200, total / 20 % 60).into_bytes();
                    let left = x.wrapping_sub(self.width(&text)? / 2);
                    ops.push(OverlayOp::Text {
                        text,
                        position: [left, y.wrapping_add(h.wrapping_mul(4) / 100)],
                        color_word: color,
                    });
                    None
                }
                (Some(race), false) => {
                    let record = &race.records[slot];
                    let laps = if record.flags_20 & 1 != 0 {
                        record.laps_18
                    } else {
                        race.laps_338
                    };
                    let best = i32::from(record.best_02);
                    Some(format!(
                        "- {:02}:{:02}:{:02}   {}/{}",
                        best / 1200,
                        best / 20 % 60,
                        best % 20 * 5,
                        laps as i32,
                        race.laps_338 as i32
                    ))
                }
                (None, true) => None,
                (None, false) => {
                    let [kills, deaths, score] = game.scores[slot];
                    Some(match inputs.mode {
                        2 => format!(" : {score:5}"),
                        // 00441F96: goals and own goals.
                        4 => format!(" +{kills:2}  -{deaths:2}"),
                        _ => format!(" : {score:5} : {kills:02}-{deaths:02}"),
                    })
                }
            };
            if let Some(text) = stats {
                ops.push(OverlayOp::Text {
                    text: text.into_bytes(),
                    position: [x, y],
                    color_word: color,
                });
                y = y.wrapping_add(row);
            }
            x = x.wrapping_add(column);
        }
        Ok(y)
    }
}

#[cfg(test)]
#[path = "network_hud_tests.rs"]
mod tests;
