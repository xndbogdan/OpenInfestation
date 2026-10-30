//! Free look's view `0041B230`, which the vehicle draws instead of its HUD
//! `0041C320` while V's free look (+1C0) is on.
//!
//! It lights grey (`00490030`), then masks the screen with four quarters of
//! the scope, sprite 0x29E, around the centre: each 3H/4 tall and that times
//! the focal ratio wide. A target +148 (refreshed through its virtual +18)
//! that is an actor other than the vehicle and not named 0x113 gets a
//! "Targetting" panel under the centre, one line every 4H/100: "%s:" of H0
//! 0x163, the actor's name +76 (none for 0 or a slot past the pool), "Allied"
//! (0x15D) for the vehicle's side, side 0 or 1 and "Alien Control" (0x15C)
//! otherwise, then its health bar (`0047F080`, 18H/100 wide and H/100 tall,
//! style 0 over style 0x21, blinking every 15 clock units below 1/32 left)
//! and for a gate (resource 0xF) "Warp to %s" (0x15F) of its destination's
//! name 0xE0 + +15C, or "Surface-bound warpgate" (0x160) without one.
//! Furniture with a grant (`00436BC0`) gets 0x163 alone, the grant's name
//! and its side from the H12 record's flag 4. Any other target, or none,
//! raises the reticle countdown +5C0 by 50000 up to 500000. Last come the
//! compass and the pitch ring (`0041BF60`, [`crate::vehicle_hud::hud_ring`])
//! and a narrow chevron on the centre (`0047F350`). Every text is
//! `004A3AB0` in style 0 with H4 font 0, centred by its width (`004A4240`).

use crate::scene_sprite::{DirectSpriteMode, DirectSpriteQuad};
use crate::vehicle_hud::{bracket, hud_bar, hud_ring, HudFrame, HudOp, VehicleHudError};

/// `0041B390..0041B3BB`: the scope's H3 sprite.
pub const SCOPE_SPRITE: u16 = 0x29e;
/// The vertex DWORD of every scope corner.
const SCOPE_DEPTH: u32 = 0x4000;
/// H0 texts the panel names.
const TARGETTING_TEXT: i32 = 0x163;
const ALIEN_TEXT: i32 = 0x15c;
const ALLIED_TEXT: i32 = 0x15d;
const WARP_TEXT: i32 = 0x15f;
const SURFACE_GATE_TEXT: i32 = 0x160;
/// `0041B5BF`: an actor named this shows no panel.
pub const HIDDEN_NAME: u16 = 0x113;
/// `0041B972`: a gate's destination names H0 0xE0 + destination.
const WORLD_NAMES: u32 = 0xe0;
/// `0041BE01..0041BE18`: the countdown's rise and ceiling.
const COUNTDOWN_RISE: i32 = 50_000;
const COUNTDOWN_CEILING: i32 = 500_000;

/// What the view reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreeLookInputs {
    /// Surface +0/+4 (`00495900`/`00495910`).
    pub surface: [i32; 2],
    /// `00546F20/24`.
    pub focal: [i32; 2],
    /// The animation clock `005471C0`.
    pub clock: i32,
    /// H6 styles 0 and 0x21.
    pub styles: [u32; 2],
    /// The vehicle's side +B4 and reticle countdown +5C0.
    pub side: u32,
    pub dword_5c0: u32,
    pub target: FreeLookTarget,
}

/// The vehicle's target +148 as the panel reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeLookTarget {
    /// No target, or one of a type other than 2 (an actor) and 4 (furniture).
    Absent,
    /// An actor that does not resolve, is the vehicle or is named 0x113, or
    /// furniture without a grant: no panel, and the countdown stays.
    Hidden,
    Actor(FreeLookActor),
    Furniture(FreeLookFurniture),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeLookActor {
    /// WORD +76, an H0 slot.
    pub name: u16,
    pub health: u16,
    /// +A2, read only with health left.
    pub maximum_health: Option<u16>,
    /// +B4.
    pub side: u32,
    /// A gate's (resource +30 0xF) destination +15C.
    pub gate: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeLookFurniture {
    /// The grant's record WORD +0 (`00475320`), an H0 slot.
    pub name: u16,
    /// The cell's H12 record flag 4: "Alien Control", else "Allied".
    pub alien: bool,
}

/// One lower call of `0041B230`, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FreeLookCall {
    /// `00490030`: grey 0x808080 along the terrain light `00548380`.
    Light,
    /// Renderer +9C: a quarter of the scope.
    Scope([[i16; 2]; 4]),
    /// Port, widescreen only: black over a side band the scope leaves.
    /// Retail's surfaces are 4:3, which the scope's width (`3H/2` times the
    /// focal ratio) always overruns, so nothing of the world shows outside
    /// it; a wider surface keeps that.
    Band([[i16; 2]; 4]),
    /// `004A3AB0` in `colour` (style 0) with H4 font 0.
    Text {
        text: Vec<u8>,
        position: [i32; 2],
        colour: u32,
    },
    /// `0047F080(x, y, filled, full, 0, thick, rest, bar)`.
    Bar {
        position: [i32; 2],
        filled: i32,
        full: i32,
        thick: i32,
        rest: u32,
        bar: u32,
    },
    /// `0041BF60(view, centre, radius, vertical, 0)`.
    Ring {
        centre: [i32; 2],
        radius: i32,
        vertical: bool,
    },
    /// `0047F350(point, colour, 0, 0)`.
    Chevron { point: [i32; 2], colour: u32 },
    /// The vehicle's new +5C0.
    Set5c0(u32),
}

/// `0041B230`'s calls. `string` answers an H0 slot, None past the pool;
/// `width` is `004A4240`'s signed width of a text.
pub fn plan_free_look(
    inputs: &FreeLookInputs,
    mut string: impl FnMut(i32) -> Result<Option<Vec<u8>>, String>,
    mut width: impl FnMut(&[u8]) -> Result<i32, String>,
) -> Result<Vec<FreeLookCall>, String> {
    let [w, h] = inputs.surface;
    let [style_0, style_21] = inputs.styles;
    let reach = h.wrapping_mul(3) / 4;
    let (cx, cy) = (w / 2, h / 2);
    let half = inputs.focal[0]
        .wrapping_mul(reach)
        .checked_div(inputs.focal[1])
        .ok_or_else(|| {
            format!(
                "native signed division by the focal {:?} faults",
                inputs.focal
            )
        })?;
    let [left, right, top, bottom] = [
        cx.wrapping_sub(half),
        cx.wrapping_add(half),
        cy.wrapping_sub(reach),
        cy.wrapping_add(reach),
    ];
    let scope = |corners: [[i32; 2]; 4]| {
        FreeLookCall::Scope(corners.map(|corner| corner.map(|value| value as i16)))
    };
    let mut calls = vec![
        FreeLookCall::Light,
        scope([[left, cy], [left, top], [cx, top], [cx, cy]]),
        scope([[cx, top], [right, top], [right, cy], [cx, cy]]),
        scope([[right, cy], [right, bottom], [cx, bottom], [cx, cy]]),
        scope([[cx, bottom], [left, bottom], [left, cy], [cx, cy]]),
    ];
    let band = |from: i32, to: i32| {
        FreeLookCall::Band(
            [[from, 0], [to, 0], [to, h], [from, h]].map(|corner| corner.map(|value| value as i16)),
        )
    };
    if left > 0 {
        calls.push(band(0, left));
    }
    if right < w {
        calls.push(band(right, w));
    }
    // The panel and the chevron read the centre's WORDs.
    let (cx, cy) = (i32::from(cx as i16), i32::from(cy as i16));
    let step = h.wrapping_mul(4) / 100;
    let mut y = cy;
    let mut line = |text: Vec<u8>| -> Result<FreeLookCall, String> {
        y = y.wrapping_add(step);
        let half = width(&text)? / 2;
        Ok(FreeLookCall::Text {
            text,
            position: [cx.wrapping_sub(half), y],
            colour: style_0,
        })
    };
    match inputs.target {
        FreeLookTarget::Absent => {
            let raised = (inputs.dword_5c0 as i32).wrapping_add(COUNTDOWN_RISE);
            calls.push(FreeLookCall::Set5c0(raised.min(COUNTDOWN_CEILING) as u32));
        }
        FreeLookTarget::Hidden => {}
        FreeLookTarget::Actor(actor) => {
            let title = string(TARGETTING_TEXT)?;
            calls.push(line(sprintf(b"%s:", &[title.as_deref()])?)?);
            // 0041B6DD: a zero name or one past the pool skips both lines.
            if actor.name != 0 {
                if let Some(name) = dynamic(&mut string, actor.name)? {
                    calls.push(line(sprintf(&name, &[])?)?);
                    let allied = actor.side == inputs.side || actor.side <= 1;
                    let side =
                        required(&mut string, if allied { ALLIED_TEXT } else { ALIEN_TEXT })?;
                    calls.push(line(sprintf(&side, &[])?)?);
                }
            }
            let full = h.wrapping_mul(18) / 100;
            let filled = if actor.health == 0 {
                0
            } else {
                let maximum =
                    i32::from(actor.maximum_health.ok_or(
                        "the health bar reads a maximum health +A2 the actor never wrote",
                    )?);
                let lost = (maximum - i32::from(actor.health)).wrapping_shl(16) / (maximum + 1);
                let mut level = 0x10000 - (lost & 0xffff);
                // 0041B85B: below 1/32 the bar blinks every 15 clock units.
                if level < 0x800 {
                    level = ((inputs.clock / 15) & 1) << 11;
                }
                full.wrapping_mul(level) >> 16
            };
            calls.push(FreeLookCall::Bar {
                position: [cx.wrapping_sub(full / 2), cy],
                filled,
                full,
                thick: h / 100,
                rest: style_21,
                bar: style_0,
            });
            if let Some(destination) = actor.gate {
                let text = if destination == 0 {
                    sprintf(&required(&mut string, SURFACE_GATE_TEXT)?, &[])?
                } else if let Some(name) = crate::pelagia::world_name(destination, true) {
                    // Only Pelagia's woken gate leads to world 1.
                    sprintf(&required(&mut string, WARP_TEXT)?, &[Some(name)])?
                } else {
                    let name = dynamic(&mut string, destination.wrapping_add(WORLD_NAMES) as u16)?;
                    sprintf(&required(&mut string, WARP_TEXT)?, &[name.as_deref()])?
                };
                calls.push(line(text)?);
            }
        }
        FreeLookTarget::Furniture(furniture) => {
            calls.push(line(sprintf(
                &required(&mut string, TARGETTING_TEXT)?,
                &[],
            )?)?);
            let name = dynamic(&mut string, furniture.name)?
                .ok_or_else(|| null_format(furniture.name as i16 as i32))?;
            calls.push(line(sprintf(&name, &[])?)?);
            let side = if furniture.alien {
                ALIEN_TEXT
            } else {
                ALLIED_TEXT
            };
            calls.push(line(sprintf(&required(&mut string, side)?, &[])?)?);
        }
    }
    calls.push(FreeLookCall::Ring {
        centre: [w.wrapping_mul(15) / 100, h.wrapping_mul(4) / 100],
        radius: w / 10,
        vertical: false,
    });
    calls.push(FreeLookCall::Ring {
        centre: [w.wrapping_mul(4) / 100, h.wrapping_mul(2) / 10],
        radius: h.wrapping_mul(15) / 100,
        vertical: true,
    });
    calls.push(FreeLookCall::Chevron {
        point: [cx, cy],
        colour: style_0,
    });
    Ok(calls)
}

/// A scope quarter's, a bar's, a ring's or the chevron's ops.
pub fn free_look_ops(call: &FreeLookCall, frame: &HudFrame) -> Result<Vec<HudOp>, VehicleHudError> {
    let mut ops = Vec::new();
    match *call {
        FreeLookCall::Scope(corners) => ops.push(HudOp::Quad(DirectSpriteQuad {
            sprite_slot: SCOPE_SPRITE,
            corners,
            vertex_depths: [SCOPE_DEPTH; 4],
            descriptor_word: 0,
            mode: DirectSpriteMode::Textured,
        })),
        FreeLookCall::Band(corners) => ops.push(HudOp::Flat(crate::scene_quad::DirectFlatQuad {
            corners,
            vertex_depths: [SCOPE_DEPTH; 4],
            descriptor_word: 0,
            color_word: 0,
        })),
        FreeLookCall::Bar {
            position,
            filled,
            full,
            thick,
            rest,
            bar,
        } => hud_bar(&mut ops, position, filled, full, 0, thick, rest, bar),
        FreeLookCall::Ring {
            centre,
            radius,
            vertical,
        } => hud_ring(&mut ops, frame, centre, radius, vertical, false)?,
        FreeLookCall::Chevron { point, colour } => {
            bracket(&mut ops, frame.focal, point, colour, 0, 0)?
        }
        FreeLookCall::Light | FreeLookCall::Text { .. } | FreeLookCall::Set5c0(_) => {}
    }
    Ok(ops)
}

pub(crate) fn null_format(slot: i32) -> String {
    format!("sprintf reads a NULL format: H0 {slot:#x} is past the pool, where retail crashes")
}

/// A constant slot the panel passes as `sprintf`'s format.
fn required(
    string: &mut impl FnMut(i32) -> Result<Option<Vec<u8>>, String>,
    slot: i32,
) -> Result<Vec<u8>, String> {
    string(slot)?.ok_or_else(|| null_format(slot))
}

/// A slot from a signed WORD: NULL when negative or past the pool.
fn dynamic(
    string: &mut impl FnMut(i32) -> Result<Option<Vec<u8>>, String>,
    word: u16,
) -> Result<Option<Vec<u8>>, String> {
    match word as i16 {
        slot if slot < 0 => Ok(None),
        slot => string(i32::from(slot)),
    }
}

/// `sprintf` over `%s` (NULL prints "(null)") and `%%`. Any other directive,
/// or a `%s` without its argument, is an error.
pub(crate) fn sprintf(format: &[u8], args: &[Option<&[u8]>]) -> Result<Vec<u8>, String> {
    let format = &format[..format
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(format.len())];
    let mut out = Vec::new();
    let mut args = args.iter();
    let mut bytes = format.iter();
    while let Some(&byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        match bytes.next() {
            Some(b'%') => out.push(b'%'),
            Some(b's') => match args.next() {
                Some(Some(text)) => out.extend_from_slice(text),
                Some(None) => out.extend_from_slice(b"(null)"),
                None => return Err(unformatted(format)),
            },
            _ => return Err(unformatted(format)),
        }
    }
    Ok(out)
}

fn unformatted(format: &[u8]) -> String {
    format!(
        "the panel's format {:?} needs a directive or argument the port does not format",
        String::from_utf8_lossy(format)
    )
}

#[cfg(test)]
#[path = "free_look_view_tests.rs"]
mod tests;
