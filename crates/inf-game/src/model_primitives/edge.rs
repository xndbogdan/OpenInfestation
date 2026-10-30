//! Opcode07: 00495CA0/004961A0 edge/line expander (both tables owned).
//!
//! Either handler widens one projected edge into a textured quad over
//! two computed corners per endpoint and enqueues it through 004A42B0
//! under the table's own thunk (`0040EF30` near, `00411D50` far), so
//! near packets are EffectQuadNear and far packets EffectQuadFar in
//! full: corner XY/depth pairs, mode at +0x20, the sprite relocation
//! at +0x24, effect color at +0x28, plus context+0x54 (the far color)
//! at +0x2C and per-vertex fog byte pairs at +0x30..0x33 on far
//! packets only. The near raster backend is already owned, so near
//! edges reach real pixels; far packets await the far pixel path.

use inf_formats::{native_basis::isqrt_48efc0, ModelInstructionKind, ModelSlotRef};

use super::{
    clip_or_skip, textured_sort_depth, vertex_depth, vertex_xy, PrimitiveCallback,
    PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState,
    PrimitiveTables,
};
use crate::{
    model_context::ModelDispatch,
    model_execution::ModelPrimitiveRequest,
    model_vertices::ModelVertexFault,
    render_resources::{RenderResourceData, RenderResourceError},
};

/// Writes the 07 record's packet with the shared quad backend matching
/// the dispatch table (near or far); only the geometry expansion is
/// edge-specific. All arithmetic wraps like the original; the float
/// scales round half-even in f64, which is exact for these magnitudes
/// (verified against the x87 shape).
pub fn prepare_edge<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Edge(edge) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0x07 {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    }
    let record = request.instruction.span.start;
    let index = |slot: ModelSlotRef| {
        usize::try_from(i32::from(slot.raw() as i16)).map_err(|_| PrimitiveError::Record(record))
    };
    let (v0, v1) = (index(edge.vertices[0])?, index(edge.vertices[1])?);
    // Clip-40 (and, on far, double-FF fog) skips precede every host
    // lookup. Unprojected dynamic slots skip the edge the same way.
    let (Some(clip0), Some(clip1)) = (clip_or_skip(request, v0)?, clip_or_skip(request, v1)?)
    else {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    };
    if clip0 & 0x40 != 0 || clip1 & 0x40 != 0 {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    }
    // Far 004961F3..004961FD alone skips an edge whose two fog bytes
    // (+0x15) are both 0xFF. Near 00495CA0 reads no fog byte, so the
    // stale bytes the near projector leaves in its slots (004EB5A0
    // writes only XY/depth/clip) never drop a near edge. Unknown far
    // bytes pass the skip as non-FF and fail closed at the packet fill.
    let fog = |index: usize| -> Result<Option<u8>, PrimitiveError<H::Error>> {
        let slot = request
            .vertices
            .slots()
            .get(index)
            .ok_or(ModelVertexFault::InvalidSlot {
                index: index as i32,
                count: request.vertices.slots().len(),
            })?;
        Ok(slot.projection.bytes[9])
    };
    let (fog0, fog1) = (fog(v0)?, fog(v1)?);
    if request.dispatch == ModelDispatch::Far
        && fog0.unwrap_or(0) == 255
        && fog1.unwrap_or(0) == 255
    {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    }
    let (xy0, xy1) = (vertex_xy(request, v0)?, vertex_xy(request, v1)?);
    let (x0, y0) = (i32::from(xy0 as i16), i32::from((xy0 >> 16) as i16));
    let (x1, y1) = (i32::from(xy1 as i16), i32::from((xy1 >> 16) as i16));
    let (d0, d1) = (vertex_depth(request, v0)?, vertex_depth(request, v1)?);
    // Squared components wrap mod 2^32 exactly like the native imuls, so
    // screen-extent edges degenerate to length zero there and here.
    let dx = x1.wrapping_sub(x0);
    let dy = y1.wrapping_sub(y0);
    let squared = (dx as u32)
        .wrapping_mul(dx as u32)
        .wrapping_add((dy as u32).wrapping_mul(dy as u32)) as i32;
    let length = isqrt_48efc0(squared);
    // The length-plus-one truncates to a signed word exactly like the
    // native movsx unit (negative past 32767px; zero is unreachable
    // since isqrt tops out at 46340), so all downstream divisions use
    // the truncated value.
    let unit = (length.wrapping_add(1) as u16 as i16) as i32;
    let unit_dy = (dy.wrapping_shl(15).wrapping_div(unit) as i16) as i32;
    let unit_dx = dx.wrapping_shl(15).wrapping_div(unit);
    // A negative or over-count material id faults retail (null deref);
    // a null entry would fault or take an unowned lazy callback.
    let material_id = i32::from(edge.edge_type as i16);
    if material_id < 0 {
        return Err(PrimitiveError::Unsupported {
            opcode: 0x07,
            reason: "negative edge material faults retail",
        });
    }
    let texture = host.texture(material_id).map_err(PrimitiveError::Host)?;
    let Some(texture) = texture else {
        return Err(PrimitiveError::Unsupported {
            opcode: 0x07,
            reason: "null edge material faults retail",
        });
    };
    let RenderResourceData::Sprite(entry) = texture.data else {
        return Err(RenderResourceError::Registration {
            request: texture.request,
        }
        .into());
    };
    let variant = entry
        .variants
        .first()
        .ok_or(RenderResourceError::SpriteDimensions {
            slot: texture.request.slot(),
        })?;
    // The width pair is variant 0's dimensions; a zero divisor faults
    // retail, and MIN/-1 only differs inside C undefined behavior.
    let width_scale = unit
        .wrapping_mul(i32::from(variant.height))
        .checked_div(
            i32::from(variant.width)
                .wrapping_sub(i32::from(variant.height))
                .wrapping_mul(2),
        )
        .ok_or(PrimitiveError::Unsupported {
            opcode: 0x07,
            reason: "zero edge width divisor faults retail",
        })?;
    // Unsigned depth sum, then signed halving; at or below 0x3F retail
    // scales by the H8 context pointer halves, which has no port
    // equivalent (address-derived, nondeterministic across runs).
    let mean = (d0.wrapping_add(d1) as i32) / 2;
    if mean <= 0x3f {
        return Err(PrimitiveError::Unsupported {
            opcode: 0x07,
            reason: "near-camera pointer-scale edge",
        });
    }
    let parameter = f64::from(i32::from(edge.parameter as i16));
    let (u, v) = (
        round_edge_scale(
            f64::from(state.projection.focal[0]),
            parameter,
            f64::from(mean),
        ),
        round_edge_scale(
            f64::from(state.projection.focal[1]),
            parameter,
            f64::from(mean),
        ),
    );
    let (mut u, mut v) = (u, v);
    while (u | v) as i32 > 0x7ffe {
        u >>= 1;
        v >>= 1;
    }
    let mut neg_half = (((u as i16 as i32).wrapping_mul(unit_dy) >> 15) as i16).wrapping_neg();
    let mut half_v = ((v as i16 as i32).wrapping_mul(unit_dx) >> 15) as i16;
    if neg_half == 0 {
        neg_half = 1;
    }
    if half_v == 0 {
        half_v = 1;
    }
    let perp = (unit_dx.wrapping_mul(width_scale) >> 15) as i16;
    let par = ((unit_dy.wrapping_mul(width_scale) >> 15) as i16) as i32;
    let (neg_half, half_v, perp) = (neg_half as i32, half_v as i32, perp as i32);
    let xa = x0.wrapping_sub(neg_half).wrapping_sub(perp);
    let xd = x0.wrapping_sub(perp).wrapping_add(neg_half);
    let ya = y0.wrapping_sub(par).wrapping_add(half_v);
    let xb = x1.wrapping_add(neg_half).wrapping_add(perp);
    let yb = y1.wrapping_add(half_v).wrapping_add(par);
    let xc = x1.wrapping_sub(neg_half).wrapping_add(perp);
    let yc = y1.wrapping_sub(half_v).wrapping_add(par);
    let yd = y0.wrapping_sub(half_v).wrapping_sub(par);
    // Viewport extents classify each corner into clip-bit buckets;
    // all four X corners must also clear the +-0x3FF window, whose right
    // bound a widescreen frame moves out by its extra width.
    let right = edge_right_bound(state.projection.viewport);
    let class_x = |s: i32| {
        if (s as u32) < state.projection.viewport[0] as u32 {
            2
        } else if s < 0 {
            1
        } else {
            4
        }
    };
    let class_y = |s: i32| {
        if (s as u32) < state.projection.viewport[1] as u32 {
            0x10
        } else if s < 0 {
            8
        } else {
            0x20
        }
    };
    let combined = class_x(xa)
        | class_y(ya)
        | class_x(xb)
        | class_y(yb)
        | class_x(xc)
        | class_y(yc)
        | class_x(xd)
        | class_y(yd);
    if tables.clip_acceptance[combined as usize] == 0
        || ![xa, xb, xc, xd].iter().all(|&s| -0x3ff < s && s < right)
    {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    }
    // One sorting call like the native path; the band logic matches the
    // owned textured shape exactly (FF selector and 0x39 flag bits
    // force the 0xC0000000 band, else the opaque address band).
    let key = textured_sort_depth(d0, state.mode, Some(texture), host)?;
    // Packet depths keep only the low word, sign-extended exactly like
    // the native movsx stores; the sort key and mean above stay full.
    let short_depth = |depth: u32| (depth & 0xffff) as u16 as i16 as i32 as u32;
    let (d0s, d1s) = (short_depth(d0), short_depth(d1));
    // Zero-extending halves like the CONCAT22 stores; readers unpack
    // each half as a signed word. Sign-extending X would clobber Y.
    let pack = |x: i32, y: i32| ((y as u16 as u32) << 16) | (x as u16 as u32);
    // Retail pairs Ya with the Xd-formula corner and Yd with the
    // Xa-formula one at +0x00/+0x18 (disassembly WORD stores plus the
    // Edge07PacketProbe oracle); +0x08/+0x10 pair straight.
    let (callback, blocks) = match request.dispatch {
        ModelDispatch::Far => (PrimitiveCallback::EffectQuadFar, 4),
        ModelDispatch::Near => (PrimitiveCallback::EffectQuadNear, 3),
    };
    let mut output = PrimitiveCommand::new(callback, key, blocks);
    output.write_u32(0x00, pack(xd, ya));
    output.write_u32(0x04, d0s);
    output.write_u32(0x08, pack(xb, yb));
    output.write_u32(0x0c, d1s);
    output.write_u32(0x10, pack(xc, yc));
    output.write_u32(0x14, d1s);
    output.write_u32(0x18, pack(xa, yd));
    output.write_u32(0x1c, d0s);
    output.write_u32(0x20, state.mode | 0x100);
    output.write_texture(0x24, Some(texture));
    output.write_u32(0x28, state.effect_color);
    if request.dispatch == ModelDispatch::Far {
        output.write_u32(0x2c, state.far_color);
        // Far fog bytes reach the packet, so missing bytes fail closed
        // here; the gate above already passed on the stand-in.
        let fog0 = fog0.ok_or(PrimitiveError::Unknown {
            field: "fog byte",
            index: v0,
        })?;
        let fog1 = fog1.ok_or(PrimitiveError::Unknown {
            field: "fog byte",
            index: v1,
        })?;
        output.write_byte(0x30, fog0);
        output.write_byte(0x31, fog1);
        output.write_byte(0x32, fog1);
        output.write_byte(0x33, fog0);
    }
    host.submit(output).map_err(PrimitiveError::Host)?;
    Ok(PrimitiveOutcome {
        commands: 1,
        rejected: 0,
    })
}

/// The exclusive right bound of `00495CA0`/`004961A0`'s X window. Retail's
/// `0x3FF` is the right edge of its widest frame, 1024x768, so a widescreen
/// frame, whose 4:3 centre keeps retail's tier, moves the bound out by the
/// width beyond that centre: 0x554 at 1365x768. Without it every edge with a
/// corner in a 1365-wide frame's right quarter vanished. A frame no wider
/// than 4:3 keeps retail's window.
pub(crate) fn edge_right_bound(viewport: [i32; 2]) -> i32 {
    let four_three = viewport[1].saturating_mul(4) / 3;
    0x3ff + viewport[0].saturating_sub(four_three).max(0)
}

/// x87 `ROUND` to an unsigned DWORD: nearest-even with the indefinite
/// low DWORD (zero) outside the i64 range, matching the converter the
/// 07 float scales use.
fn round_edge_scale(focal: f64, parameter: f64, mean: f64) -> u32 {
    const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    let scaled = focal * (parameter / mean);
    if !scaled.is_finite() || !(-I64_UPPER_EXCLUSIVE..I64_UPPER_EXCLUSIVE).contains(&scaled) {
        return 0;
    }
    scaled.round_ties_even() as i64 as u32
}

#[cfg(test)]
#[path = "edge_native_tests.rs"]
mod edge_native_tests;
#[cfg(test)]
#[path = "edge_tests.rs"]
mod edge_tests;
