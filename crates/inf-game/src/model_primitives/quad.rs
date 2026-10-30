//! OpcodeE2: 0049DBE0/0049DE90 and clipped textured callbacks498B10/499130.

use super::{
    clip_or_skip, textured_sort_depth,
    uv_only_clipping::{prepare_uv_only_clipped, UvOnlyClipRequest, UvOnlyVertices},
    vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError,
    PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::ModelInstructionKind;

pub fn prepare_quad<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Quad(quad) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0xe2 {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    }
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    for mirror in (0..=((raw >> 10) & 1)).rev() {
        let mut indices = [0; 4];
        for (index, slot) in indices.iter_mut().zip(quad.vertices) {
            *index = usize::try_from(i32::from(slot.raw() as i16) ^ i32::from(mirror))
                .map_err(|_| PrimitiveError::Record(request.instruction.span.start))?;
        }
        let passes_cull = if raw & 0x100 == 0 {
            true
        } else {
            (u16::from(positive_area([
                vertex_xy(request, indices[0])?,
                vertex_xy(request, indices[1])?,
                vertex_xy(request, indices[2])?,
            ])) ^ ((raw >> 9) & 1))
                != mirror
        };
        let mut clips = [0; 4];
        let mut unprojected = false;
        for (clip, index) in clips.iter_mut().zip(indices) {
            let Some(byte) = clip_or_skip(request, index)? else {
                unprojected = true;
                break;
            };
            *clip = byte;
        }
        if unprojected {
            outcome.rejected += 1;
            continue;
        }
        let combined = clips.into_iter().fold(0, |a, b| a | b);
        if passes_cull
            && tables.clip_acceptance[usize::from(combined)] != 0
            && (!far || !all_fogged(request, indices)?)
        {
            let texture = host
                .texture(i32::from(quad.material.global_id))
                .map_err(PrimitiveError::Host)?;
            let depth = vertex_depth(request, indices[0])?;
            let key = textured_sort_depth(depth, state.mode, texture, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::TexturedQuadFar
                } else {
                    PrimitiveCallback::TexturedQuadNear
                },
                key,
                3,
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x20, state.mode);
            output.write_texture(0x24, texture);
            if far {
                output.write_u32(0x28, state.far_color);
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x2c + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            let clipped = prepare_uv_only_clipped(
                request,
                UvOnlyClipRequest {
                    mirror,
                    vertex_indices: UvOnlyVertices::Quad(indices),
                    literal_shades_offset: request
                        .instruction
                        .span
                        .start
                        .checked_add(12)
                        .ok_or(PrimitiveError::Record(request.instruction.span.start))?,
                    material: i32::from(quad.material.global_id),
                },
                state,
                tables,
                host,
            )?;
            outcome.commands += clipped.commands;
            outcome.rejected += clipped.rejected;
        } else {
            outcome.rejected += 1;
        }
    }
    Ok(outcome)
}

pub(crate) fn positive_area(points: [u32; 3]) -> bool {
    let [[x0, y0], [x1, y1], [x2, y2]] =
        points.map(|xy| [i32::from(xy as i16), i32::from((xy >> 16) as i16)]);
    (x1 - x0)
        .wrapping_mul(y2 - y0)
        .wrapping_sub((y1 - y0).wrapping_mul(x2 - x0))
        > 0
}

pub(crate) fn all_fogged<E>(
    request: &ModelPrimitiveRequest<'_>,
    indices: [usize; 4],
) -> Result<bool, PrimitiveError<E>> {
    for index in indices {
        if vertex_fog(request, index)? != 255 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Far gate over three corners (not four): skip only when every corner
/// fog byte is set, mirroring the quad helper's 255 semantics. Shared
/// by the DE/DA triangle producers.
pub(crate) fn all_fogged_triangle<E>(
    request: &ModelPrimitiveRequest<'_>,
    indices: [usize; 3],
) -> Result<bool, PrimitiveError<E>> {
    for index in indices {
        if vertex_fog(request, index)? != 255 {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "quad_tests.rs"]
mod tests;
