//! The radar's actor walk, `004724CA..004728B3` inside `004723F0`.
//!
//! The ordinary radar `00473490` runs it while the player owns the Scanner
//! (grant 0x11); the terrain radar `00470170` always runs it, unless the
//! map's jam has reached 0x4000. It walks the actor list (`G+48`) in order:
//!
//! - An actor enters when its flags have 0x21 without 0x10000, or its `+70`
//!   answers (the pickup spawners, selector 53, whatever their flags). The
//!   `+B8` arm, which would draw four dots, is null on every class.
//! - The vehicle (`+64`) shows on even sevenths of the clock only.
//! - Its offset from the eye's X/Z WORDs is jammed while the map's jitter
//!   exceeds 7: each axis gains a process RNG draw modulo a quarter of the
//!   jitter. The offset turns by the view's heading as the annotations' do,
//!   and a distance r lands at the radar's half size times the quarter sine
//!   of 2r. From 2r = 0x4000 on, the blip is pinned to the rim as a single
//!   pixel, which shows only when the parity of a 16-bit word, rotated right
//!   for every pinned blip from the contents' first RNG draw, differs from
//!   the clock's sevenths.
//! - A spawner draws the Resource scanner's marker (grant 0x13) through
//!   `004736E0`: sprite 7 at half its count in style 0x22, 0x23 or 0x24 by
//!   its kind (blue, green, red). Without the upgrade it shows nothing.
//! - Anything else is a dot (`004719E0`), five pixels in a plus about
//!   (x + 1, y), written straight into the frame in the colour of its team
//!   (`004721A0`'s H7 palette), halved for a dying actor (flag 0x8000).

use inf_formats::native_basis::{isqrt_48efc0, sine_q15};
use thiserror::Error;

use crate::object_runtime::native_trunc_low32 as ftol;

/// What answers the walk's virtuals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadarActorKind {
    /// `+64` answers: the vehicle, selector 2.
    Vehicle,
    /// `+70` answers: a pickup spawner (selector 53) with its kind `+138`
    /// and count `+139`, both signed.
    Spawner {
        kind: i8,
        count: i8,
    },
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadarActor {
    /// `+2C`.
    pub flags: u32,
    /// X and Z, `+32` and `+36`, which the walk reads only for an actor it
    /// admits. A carried unit's capture clears flags 1 and 2, so the walk
    /// never reads the position a respawn's hold item has yet to take.
    pub position_words: Option<[u16; 2]>,
    /// `+B4`, signed, for `004721A0`'s palette.
    pub team: i32,
    pub kind: RadarActorKind,
}

/// The walk's inputs from `004723F0`'s frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadarBlipPass {
    /// The actor list `G+48` in order.
    pub actors: Vec<RadarActor>,
    /// The eye's X and Z WORDs.
    pub eye_words: [u16; 2],
    /// The radar's top left (its centre less its half size), signed WORDs.
    pub origin: [i32; 2],
    /// H10 global slot 0's dimensions, the radar's half width and height.
    pub size: [i32; 2],
    /// `ftol(cos * 32768)` and `ftol(sin * -32768)` of the view's heading.
    pub rotation: [i32; 2],
    /// The map's jitter, `+24`.
    pub jitter: i32,
    /// The animation clock `005471C0`.
    pub clock: i32,
    /// The final argument's byte: the Resource scanner (grant 0x13).
    pub resource_scanner: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadarBlip {
    /// `004719E0(x, y, colour, pinned)`: the team's palette colour, halved
    /// when `dim`; five pixels about (x + 1, y), or that one when pinned.
    Dot {
        position: [i32; 2],
        team: i32,
        dim: bool,
        pinned: bool,
    },
    /// `004736E0(x, y, half_size, style)`.
    Marker {
        position: [i32; 2],
        half_size: i32,
        style: i32,
    },
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum RadarBlipError {
    /// `[ebp-14]` keeps the previous spawner's style; the first one's is
    /// unwritten stack.
    #[error("a kind-{kind} spawner reads the style its predecessor left, and none did")]
    SpawnerStyle { kind: i8 },
    #[error("the walk's actor {index} has no position")]
    Position { index: usize },
}

/// `0047242C..004724BC`: the view's heading as `ftol(cos * 32768)` and
/// `ftol(sin * -32768)`, from its X axis in XZ (scaled to 4096 and
/// narrowed to signed WORDs). The cosine keeps the 53-bit quotient; the sine
/// is stored as a float first.
pub fn heading_rotation(axis_x: [f32; 3]) -> [i32; 2] {
    let a = f64::from(ftol(f64::from(axis_x[0]) * 4096.0) as i16);
    let b = f64::from(ftol(f64::from(axis_x[2]) * 4096.0) as i16);
    let length = (b * b + a * a).sqrt();
    let (cosine, sine) = if length == 0.0 {
        (1.0, 0.0f32)
    } else {
        let stored = f64::from(length as f32);
        (a / stored, (b / stored) as f32)
    };
    [ftol(cosine * 32768.0), ftol(f64::from(sine) * -32768.0)]
}

/// The pixels `004719E0` writes for a dot: (x + 1, y) always, then unless
/// pinned (x, y), (x + 2, y), (x + 1, y + 1) and (x + 1, y - 1).
pub fn dot_pixels([x, y]: [i32; 2], pinned: bool) -> Vec<[i32; 2]> {
    let mut pixels = vec![[x + 1, y]];
    if !pinned {
        pixels.extend([[x, y], [x + 2, y], [x + 1, y + 1], [x + 1, y - 1]]);
    }
    pixels
}

/// The signed remainder's test `0047252B..00472548`: the clock's sevenths are
/// odd (or, for a negative clock, -1).
fn odd_sevenths(clock: i32) -> i32 {
    (clock / 7) % 2
}

/// The walk. `blink` is the contents' first draw (`00472409`); `next` is the
/// process RNG `0048F060`, drawn twice per entering actor while jammed.
pub fn radar_blips(
    inputs: &RadarBlipPass,
    sine_quarter: &[u16; 4096],
    mut blink: u32,
    rng: &mut u32,
    mut next: impl FnMut(&mut u32) -> u32,
) -> Result<Vec<RadarBlip>, RadarBlipError> {
    let [c, s] = inputs.rotation;
    let [width, height] = inputs.size;
    let [origin_x, origin_y] = inputs.origin;
    let mut style = None;
    let mut blips = Vec::new();
    for (index, actor) in inputs.actors.iter().enumerate() {
        let spawner = matches!(actor.kind, RadarActorKind::Spawner { .. });
        if actor.flags & 0x10021 != 0x21 && !spawner {
            continue;
        }
        if actor.kind == RadarActorKind::Vehicle && odd_sevenths(inputs.clock) != 0 {
            continue;
        }
        let [x, z] = actor
            .position_words
            .ok_or(RadarBlipError::Position { index })?;
        let mut dx = i32::from(x.wrapping_sub(inputs.eye_words[0]) as i16);
        let mut dz = i32::from(z.wrapping_sub(inputs.eye_words[1]) as i16);
        if inputs.jitter > 7 {
            let quarter = inputs.jitter / 4;
            dx = i32::from((next(rng) as i32 % quarter + dx) as i16);
            dz = i32::from((next(rng) as i32 % quarter + dz) as i16);
        }
        // 004725F7..00472643: the quartered squares' root, doubled.
        let span = (isqrt_48efc0((dz.wrapping_mul(dz) >> 2).wrapping_add(dx.wrapping_mul(dx) >> 2))
            as u32)
            << 1;
        let turned_x = i32::from((dx.wrapping_mul(c) >> 15) as i16)
            - i32::from((dz.wrapping_mul(s) >> 15) as i16);
        let turned_z = i32::from((dx.wrapping_mul(s) >> 15) as i16)
            + i32::from((dz.wrapping_mul(c) >> 15) as i16);
        let (reach, divisor, pinned) = if span >= 0x4000 {
            // 00472654..004726BC: the rim's blink.
            blink = ((blink & 1) << 15) + ((blink & 0xffff) >> 1) + (blink & 0xffff_0000);
            if odd_sevenths(inputs.clock) == (blink & 0xffff) as i32 % 2 {
                continue;
            }
            (0x4000, span as i32 / 2, true)
        } else {
            (sine_q15(span as i32, sine_quarter), span as i32, false)
        };
        let position = if divisor == 0 {
            [origin_x + width, origin_y + height]
        } else {
            let across = i32::from((reach.wrapping_mul(turned_x) >> 15) as i16);
            let up = i32::from((reach.wrapping_mul(turned_z) >> 15) as i16);
            [
                origin_x + width + across.wrapping_mul(width) / divisor,
                origin_y - up.wrapping_mul(height) / divisor + height,
            ]
        };
        match actor.kind {
            RadarActorKind::Spawner { kind, count } => {
                if !inputs.resource_scanner {
                    continue;
                }
                if let 1..=3 = kind {
                    style = Some(0x21 + i32::from(kind));
                }
                blips.push(RadarBlip::Marker {
                    position,
                    half_size: i32::from(count),
                    style: style.ok_or(RadarBlipError::SpawnerStyle { kind })?,
                });
            }
            _ => blips.push(RadarBlip::Dot {
                position,
                team: actor.team,
                dim: actor.flags & 0x8000 != 0,
                pinned,
            }),
        }
    }
    Ok(blips)
}

#[cfg(test)]
#[path = "radar_blips_tests.rs"]
mod tests;
