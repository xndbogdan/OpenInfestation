//! Opcode1B: 004A21F0/004A33C0, authored sprite dimensions and integer rotation.

use inf_formats::ModelInstructionKind;

use super::{
    clip_or_skip, textured_sort_depth, vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback,
    PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState,
    PrimitiveTables,
};
use crate::{
    menu_basis::sine_q31,
    model_context::{fistp_low32, ModelDispatch},
    model_execution::ModelPrimitiveRequest,
    model_values::ModelValues,
    render_resources::{RenderResourceData, RenderResourceError},
};

pub fn prepare_effect<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    values: &mut ModelValues,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Opaque(command) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let [slot, material, size, angle] = command.arguments.as_slice() else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0x1b {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    }
    // Native lookup precedes even the vertex reference and visibility check.
    let texture = host
        .texture(i32::from(*material as i16))
        .map_err(PrimitiveError::Host)?;
    let index = usize::try_from(i32::from(*slot as i16))
        .map_err(|_| PrimitiveError::Record(request.instruction.span.start))?;
    let far = request.dispatch == ModelDispatch::Far;
    let Some(clip) = clip_or_skip(request, index)? else {
        // Unprojected dynamic slot: skip the effect, keep the walk.
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    };
    if clip & 0x40 != 0 || (far && vertex_fog(request, index)? == 255) {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    }
    let size = values
        .evaluate_packed(*size, host)
        .map_err(PrimitiveError::Value)?;
    let angle = values
        .evaluate_packed(*angle, host)
        .map_err(PrimitiveError::Value)?;
    let depth = vertex_depth(request, index)?;
    if (depth as i32) < 64 {
        // The native branch reads uninitialized stack extent words. Preserve
        // preceding lookup/callbacks and reject this inconsistent slot state.
        return Err(PrimitiveError::Unsupported {
            opcode: 0x1b,
            reason: "uninitialized extent after a depth below64 without clip40",
        });
    }
    let texture = texture.ok_or(PrimitiveError::Unsupported {
        opcode: 0x1b,
        reason: "a non-null sprite at the dimensions dereference",
    })?;
    let RenderResourceData::Sprite(sprite) = texture.data else {
        return Err(RenderResourceError::Registration {
            request: texture.request,
        }
        .into());
    };
    let variant = sprite
        .variants
        .first()
        .ok_or(RenderResourceError::SpriteDimensions {
            slot: texture.request.slot(),
        })?;
    if variant.width.max(variant.height) == 0 {
        return Err(PrimitiveError::Unsupported {
            opcode: 0x1b,
            reason: "nonzero sprite dimensions at native integer division",
        });
    }
    let corners = expanded_sprite_corners(
        vertex_xy(request, index)?,
        depth as i32,
        [size, angle],
        [variant.width, variant.height],
        state.projection.focal,
        &tables.sine_table,
    );
    let clip = corners.iter().fold(0, |combined, xy| {
        combined | screen_clip(*xy, state.projection.viewport)
    });
    if tables.clip_acceptance[usize::from(clip)] == 0 {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    }
    let key = textured_sort_depth(depth, state.mode, Some(texture), host)?;
    let mut output = PrimitiveCommand::new(
        if far {
            PrimitiveCallback::EffectQuadFar
        } else {
            PrimitiveCallback::EffectQuadNear
        },
        key,
        if far { 4 } else { 3 },
    );
    for (corner, xy) in corners.into_iter().enumerate() {
        output.write_u32(corner * 8, xy);
        output.write_u32(corner * 8 + 4, depth);
    }
    output.write_texture(0x24, Some(texture));
    output.write_u32(0x28, state.effect_color);
    output.write_u32(0x20, state.mode);
    if far {
        output.write_u32(0x2c, state.far_color);
        let fog = vertex_fog(request, index)?;
        for offset in 0x30..0x34 {
            output.write_byte(offset, fog);
        }
    }
    host.submit(output).map_err(PrimitiveError::Host)?;
    Ok(PrimitiveOutcome {
        commands: 1,
        rejected: 0,
    })
}

/// Shared native1A/1B corner expansion. Admission, lookup, packed callbacks,
/// clipping and packet layouts stay in each producer.
/// 1A Near4A1DC7..1F2A and Far4A2F81..30E4 match1B's retained scale/Q31 phase.
pub(super) fn expanded_sprite_corners(
    center: u32,
    depth: i32,
    operands: [u16; 2],
    dimensions: [u16; 2],
    focal: [i32; 2],
    sine: &[u16; 4096],
) -> [u32; 4] {
    let [size, angle] = operands;
    // Division precedes either focal multiply; the ratio remains at53-bit
    // x87 precision. No f32 spill is present at4A2267..228C.
    let ratio = f64::from(u32::from(size) * 2) / f64::from(depth);
    let [mut x, mut y] = focal.map(|focal| fistp_low32(f64::from(focal) * ratio));
    // Native CMP/JL and SAR operate on signed(x|y), not absolute magnitudes.
    // Keep negative and low-DWORD-overflow extents before sign-extending WORDs.
    let mut magnitude = x | y;
    while magnitude >= 32767 {
        magnitude >>= 1;
        x >>= 1;
        y >>= 1;
    }
    let x = i32::from(x as i16);
    let y = i32::from(y as i16);
    let [width, height] = dimensions.map(i32::from);
    let divisor = width.max(height);
    let sin = multiply_q31(sine_q31(i32::from(angle), sine) / divisor, 0x5a82_7999);
    let cos = multiply_q31(
        sine_q31(i32::from(angle) + 0x4000, sine) / divisor,
        0x5a82_7999,
    );
    let sw = multiply_q31(sin, y.wrapping_mul(width));
    let cw = multiply_q31(cos, x.wrapping_mul(width));
    let sh = multiply_q31(sin, x.wrapping_mul(height));
    let ch = multiply_q31(cos, y.wrapping_mul(height));
    let cx = i32::from(center as i16);
    let cy = i32::from((center >> 16) as i16);
    [
        pack_xy(
            cx.wrapping_sub(cw).wrapping_add(sh),
            cy.wrapping_sub(ch).wrapping_sub(sw),
        ),
        pack_xy(
            cx.wrapping_add(sh).wrapping_add(cw),
            cy.wrapping_sub(ch).wrapping_add(sw),
        ),
        pack_xy(
            cx.wrapping_sub(sh).wrapping_add(cw),
            cy.wrapping_add(ch).wrapping_add(sw),
        ),
        pack_xy(
            cx.wrapping_sub(sh).wrapping_sub(cw),
            cy.wrapping_sub(sw).wrapping_add(ch),
        ),
    ]
}

pub(super) fn multiply_q31(left: i32, right: i32) -> i32 {
    ((i64::from(left) * i64::from(right)) >> 31) as i32
}

pub(super) fn pack_xy(x: i32, y: i32) -> u32 {
    u32::from(x as u16) | (u32::from(y as u16) << 16)
}

pub(super) fn screen_clip(xy: u32, viewport: [i32; 2]) -> u8 {
    let [x, y] = [i32::from(xy as i16), i32::from((xy >> 16) as i16)];
    axis_clip(x, viewport[0], [1, 2, 4]) | axis_clip(y, viewport[1], [8, 16, 32])
}

pub(super) fn axis_clip(value: i32, limit: i32, codes: [u8; 3]) -> u8 {
    if (value as u32) < (limit as u32) {
        codes[1]
    } else if value < 0 {
        codes[0]
    } else {
        codes[2]
    }
}

#[cfg(test)]
#[path = "effect_tests.rs"]
mod tests;
