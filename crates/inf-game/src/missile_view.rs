//! The guided missile's view, the missiles' virtual +84 (`00456CF0` for
//! selectors 25 and 28; selector 68's `00458F30` only calls it): what a
//! player steering a missile or the Cranial Implanter sees over the world.
//!
//! A missile a chain grenade or selector 57 has disrupted (+150) first fills
//! the screen with static (`00456B50`). With the "HUD" setting it then draws
//! a narrow chevron on the screen's centre, five upright bars (`0047F080`,
//! W/100 wide and 18H/100 tall, style 1 for the rest and 0 for the fill): at
//! 88, 91 and 94/100 W the steering rates +94, +98 and +9C from half full;
//! at 60/100 W the height above the ground or water out of 2000; at 40/100
//! W the speed out of 3000. Then the shared actor view `0047F690`, which is
//! the player's HUD `0046BE00`, and the player's HUD again, so its menu and
//! messages draw twice; last the fuse +13A as "%d" (above 1000 in thousands,
//! rounded up) centred at 6/10 H. Without the setting only the static draws,
//! and the player sees no menu or message.

use inf_formats::native_basis::isqrt_48efc0;

use crate::scene_sprite::{DirectSpriteMode, DirectSpriteQuad};
use crate::vehicle_hud::{bracket, hud_bar, HudOp, VehicleHudError};

/// `00456BFB..00456C00`: the static's sprites, H3 0xBF to 0xC2.
const STATIC_SPRITE: u16 = 0xbf;
/// The vertex DWORD of every static quad.
const STATIC_DEPTH: u32 = 0x40;
/// `00456E1A`'s bars' heights: 18/100 of the surface.
const BAR_HEIGHT_PERCENT: i32 = 18;
/// The height bar is full 2000 above the ground, the speed bar at 3000.
const FULL_HEIGHT: i32 = 2000;
const FULL_SPEED: i32 = 3000;

/// What the view reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissileViewInputs {
    /// Surface +0/+4 (`00495900`/`00495910`).
    pub surface: [i32; 2],
    /// Settings +60, "HUD".
    pub hud_setting: u32,
    /// H6 styles 0 and 1.
    pub styles: [u32; 2],
    /// BYTE +150, set by messages 0xB and 0xC.
    pub disrupted: bool,
    /// Whether a player holds the missile (`0046B090` of +84).
    pub held: bool,
    /// +94, +98 and +9C, the steering rates.
    pub rates: [i32; 3],
    /// WORD +34, the missile's height.
    pub height: i16,
    /// `00450A30` and `00410F60` under the missile (the water at clock 0).
    pub ground: i16,
    pub water: i16,
    /// +8E..+92.
    pub velocity: [i16; 3],
    /// WORD +13A, the fuse.
    pub fuse: u16,
}

/// One lower call of `00456CF0`, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissileViewCall {
    /// `00456B50` with EDX -1 from (0, 0): every row of the surface.
    Static { end: [i32; 2] },
    /// `0047F350(point, colour, 0, 0)`.
    Chevron { point: [i32; 2], colour: u32 },
    /// `0047F080(x, y, 0, width, fill, height, rest, bar)`.
    Bar {
        point: [i32; 2],
        width: i32,
        fill: i32,
        height: i32,
        rest: u32,
        bar: u32,
    },
    /// `0047F690`, which draws the player's HUD when a player holds it.
    SharedView,
    /// `0046BE00` on the player that holds it.
    PlayerHud,
    /// `004A3AB0` in style 0 with H4 font 0, centred on `centre` by its
    /// width (`004A4240`).
    Counter { text: Vec<u8>, centre: [i32; 2] },
}

/// `00456CF0`'s calls.
pub fn plan_missile_view(inputs: &MissileViewInputs) -> Vec<MissileViewCall> {
    let [w, h] = inputs.surface;
    let mut calls = Vec::new();
    if inputs.disrupted {
        calls.push(MissileViewCall::Static { end: [w, h] });
    }
    if inputs.hud_setting == 0 {
        return calls;
    }
    let [fill_style, rest_style] = inputs.styles;
    calls.push(MissileViewCall::Chevron {
        point: [w / 2, h / 2],
        colour: fill_style,
    });
    let full = h.wrapping_mul(BAR_HEIGHT_PERCENT) / 100;
    let width = w / 100;
    let bar = |point, fill| MissileViewCall::Bar {
        point,
        width,
        fill,
        height: full,
        rest: rest_style,
        bar: fill_style,
    };
    let low = h.wrapping_mul(9) / 10;
    for (rate, column) in inputs.rates.into_iter().zip([88, 91, 94]) {
        let fill = (rate.wrapping_mul(full) >> 17).wrapping_add(full / 2);
        calls.push(bar([w.wrapping_mul(column) / 100, low], fill));
    }
    let middle = h.wrapping_mul(6) / 10;
    let surface = inputs.ground.max(inputs.water);
    let above =
        full.wrapping_mul(i32::from(inputs.height).wrapping_sub(i32::from(surface))) / FULL_HEIGHT;
    calls.push(bar(
        [w.wrapping_mul(60) / 100, middle],
        above.max(0).min(full),
    ));
    let [x, y, z] = inputs.velocity.map(i32::from);
    let squared = x
        .wrapping_mul(x)
        .wrapping_add(y.wrapping_mul(y))
        .wrapping_add(z.wrapping_mul(z));
    // 00457179: a sum past i32 takes the root of a quarter, doubled.
    let speed = if squared < 0 {
        (isqrt_48efc0(((squared as u32) >> 2) as i32) << 1) as i32
    } else {
        isqrt_48efc0(squared) as i32
    };
    let fill = if speed < FULL_SPEED {
        full.wrapping_mul(speed) / FULL_SPEED
    } else {
        full
    };
    calls.push(bar([w.wrapping_mul(4) / 10, middle], fill));
    calls.push(MissileViewCall::SharedView);
    if inputs.held {
        calls.push(MissileViewCall::PlayerHud);
    }
    let fuse = u32::from(inputs.fuse);
    let shown = if fuse > 1000 {
        (fuse + 999) / 1000
    } else {
        fuse
    };
    calls.push(MissileViewCall::Counter {
        text: shown.to_string().into_bytes(),
        centre: [w.wrapping_mul(5) / 10, middle],
    });
    calls
}

/// A chevron's or a bar's quads.
pub fn missile_view_ops(
    call: &MissileViewCall,
    focal: [i32; 2],
) -> Result<Vec<HudOp>, VehicleHudError> {
    let mut ops = Vec::new();
    match *call {
        MissileViewCall::Chevron { point, colour } => {
            bracket(&mut ops, focal, point, colour, 0, 0)?;
        }
        MissileViewCall::Bar {
            point,
            width,
            fill,
            height,
            rest,
            bar,
        } => hud_bar(&mut ops, point, 0, width, fill, height, rest, bar),
        _ => {}
    }
    Ok(ops)
}

/// `00456B50(renderer, rows, start, end)`: rows of one-pixel quads from
/// `start` to `end`, each a random sprite of H3 0xBF..0xC2 (`width` gives
/// its +14), mirrored at random, side by side from the left until one
/// reaches `end`'s X. `rows` -1 draws every row from `start`'s Y to `end`'s
/// inclusive; otherwise `rows + 1` rows at random heights between them.
/// Each draw is the process RNG's (`0048F060`).
pub fn static_quads<E>(
    rows: i32,
    start: [i16; 2],
    end: [i16; 2],
    rng: &mut u32,
    mut width: impl FnMut(u16) -> Result<u16, E>,
) -> Result<Vec<DirectSpriteQuad>, E> {
    let span = i32::from(end[1]).wrapping_sub(i32::from(start[1]));
    let every = rows == -1;
    let rows = if every { span } else { rows };
    let mut quads = Vec::new();
    let mut row = 0;
    while row <= rows {
        let y = if every {
            i32::from(start[1]).wrapping_add(row)
        } else {
            let draw = crate::name_entry::name_entry_rng_next(rng) & 0xffff;
            ((draw as i32).wrapping_mul(span) >> 16).wrapping_add(i32::from(start[1]))
        } as i16;
        let mut x = i32::from(start[0]);
        while x < i32::from(end[0]) {
            let sprite = STATIC_SPRITE
                + (((crate::name_entry::name_entry_rng_next(rng) & 0xffff) >> 4) & 3) as u16;
            let step = i32::from(width(sprite)?);
            let right = x.wrapping_add(step);
            let (first, second) = if crate::name_entry::name_entry_rng_next(rng) & 0x10 != 0 {
                (x, right)
            } else {
                (right, x)
            };
            let below = y.wrapping_add(1);
            quads.push(DirectSpriteQuad {
                sprite_slot: sprite,
                corners: [
                    [first as i16, y],
                    [second as i16, y],
                    [second as i16, below],
                    [first as i16, below],
                ],
                vertex_depths: [STATIC_DEPTH; 4],
                descriptor_word: 0,
                mode: DirectSpriteMode::Textured,
            });
            x = right;
        }
        row += 1;
    }
    Ok(quads)
}

/// [`MissileViewCall::Static`]: `00456B50` over the whole surface. Retail's
/// surface is 4:3, so the process RNG draws across the 4:3 width at the
/// surface's height only; a wider port frame's sides draw from a copy of it,
/// and a widescreen frame plays the same game.
pub fn surface_static_quads<E>(
    end: [i32; 2],
    rng: &mut u32,
    mut width: impl FnMut(u16) -> Result<u16, E>,
) -> Result<Vec<DirectSpriteQuad>, E> {
    let [w, h] = end;
    let retail = w.min(h.wrapping_mul(4) / 3);
    let mut quads = static_quads(-1, [0, 0], [retail as i16, h as i16], rng, &mut width)?;
    if w > retail {
        let mut sides = *rng;
        quads.extend(static_quads(
            -1,
            [retail as i16, 0],
            [w as i16, h as i16],
            &mut sides,
            width,
        )?);
    }
    Ok(quads)
}

#[cfg(test)]
#[path = "missile_view_tests.rs"]
mod tests;
