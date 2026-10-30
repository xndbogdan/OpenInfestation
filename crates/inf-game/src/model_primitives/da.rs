//! OpcodeDA 0049C710/0049C960: unshaded textured triangle producer.
//! Signed mirror/cull/clip gates, one nullable H3 lookup, required sorting and
//! native2/3-block direct records. The nominal shared shade WORD is unread.
//! Eligible mixed near-plane clipping uses 00498B10/00499130 with three literal
//! scratch WORDs at instruction+0A/+0C/+0E, crossing the nominal record.
//! The complete-packet host cannot represent a current partial allocation on
//! a later payload-read fault; completed submissions remain published.

use super::{
    clip_or_skip,
    quad::{all_fogged_triangle, positive_area},
    textured_sort_depth,
    uv_only_clipping::{prepare_uv_only_clipped, UvOnlyClipRequest, UvOnlyVertices},
    vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError,
    PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelTriangleLighting};

pub fn prepare_da<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Triangle(triangle) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0xda {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    // The shade word exists in the decoded shape but the main path
    // never reads it; requiring Shared keeps per-vertex-lit decodes
    // out without consuming the value.
    if !matches!(triangle.lighting, ModelTriangleLighting::Shared(_)) {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    }
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop `dec ebp; jns`: one pass normally, two (1, then 0)
    // when bit 10 is set, matching the DE producer.
    for mirror in (0..=((raw >> 10) & 1)).rev() {
        let mut indices = [0; 3];
        for (index, slot) in indices.iter_mut().zip(triangle.vertices) {
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
        let mut clips = [0; 3];
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
            && (!far || !all_fogged_triangle(request, indices)?)
        {
            let texture = host
                .texture(i32::from(triangle.material.global_id))
                .map_err(PrimitiveError::Host)?;
            let depth = vertex_depth(request, indices[0])?;
            // The brightness kernel feeds only the dropped
            // queue-packet prefix, never the record.
            let key = textured_sort_depth(depth, state.mode, texture, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::DaTriangleFar
                } else {
                    PrimitiveCallback::DaTriangleNear
                },
                key,
                // Far fog bytes run to +0x26: the native far record
                // is 0x27 bytes, three blocks.
                if far { 3 } else { 2 },
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x18, state.mode);
            output.write_texture(0x1c, texture);
            if far {
                output.write_u32(0x20, state.far_color);
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x24 + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            let clipped = prepare_uv_only_clipped(
                request,
                UvOnlyClipRequest {
                    mirror,
                    vertex_indices: UvOnlyVertices::Triangle(indices),
                    literal_shades_offset: request
                        .instruction
                        .span
                        .start
                        .checked_add(10)
                        .ok_or(PrimitiveError::Record(request.instruction.span.start))?,
                    material: i32::from(triangle.material.global_id),
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

#[cfg(test)]
#[path = "da_tests.rs"]
mod da_tests;
