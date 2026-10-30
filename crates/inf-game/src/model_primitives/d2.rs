//! Opcode D2: unchanged 0049EC80/0049EF10 per-corner textured triangle policy.
//!
//! One nullable H3 lookup precedes sorting and allocation. Direct packets copy
//! three signed mirror-indexed shade DWORDs without filling unknown bytes;
//! mixed-C0 triangles reach shared 0049B070/0049B6F0 and 00497250 with count3.
//! Direct implicit-UV pixel policy remains separately bounded in d3d_d2.

use super::{
    clip_or_skip,
    per_corner_clipping::{prepare_per_corner_clipped, PerCornerClipPass, PerCornerVertices},
    quad::{all_fogged_triangle, positive_area},
    textured_sort_depth, vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand,
    PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelTriangleLighting};

pub fn prepare_d2<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Triangle(triangle) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0xd2 {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    // Per-corner shades; shared-shade decodes belong to DE.
    let ModelTriangleLighting::PerVertex(shades) = &triangle.lighting else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop `dec esi; jns`: one pass normally, two (1, then 0)
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
            //49ED63..49EE05 /49F00D..49F0AF retain null H3 through
            // the required sorting query and allocation. D2 performs one
            // lookup, unlike direct D6's fresh second packet lookup.
            let depth = vertex_depth(request, indices[0])?;
            let key = textured_sort_depth(depth, state.mode, texture, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::D2TriangleFar
                } else {
                    PrimitiveCallback::D2TriangleNear
                },
                key,
                // Eleven near DWORDs own +00..+2B; +2C..+2F stays
                // unwritten. Far color/fog require a fourth block.
                if far { 4 } else { 3 },
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x18, state.mode);
            output.write_texture(0x1c, texture);
            //49EE58..49EE7F /49F102..49F129 sign-extend each WORD,
            // XOR mirror, then copy its complete shade DWORD after allocation.
            for (corner, shade) in shades.iter().enumerate() {
                let shade_value = request
                    .shades
                    .scratch_shade(i32::from(shade.raw() as i16) ^ i32::from(mirror))?;
                if let Some(rgb) = shade_value.rgb {
                    output.write_rgb(0x20 + corner * 4, rgb);
                }
                if let Some(high) = shade_value.high_byte {
                    output.write_byte(0x23 + corner * 4, high);
                }
            }
            if far {
                output.write_u32(0x2c, state.far_color);
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x30 + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            let clipped = prepare_per_corner_clipped(
                request,
                PerCornerClipPass {
                    mirror,
                    vertex_indices: PerCornerVertices::Triangle(indices),
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
#[path = "d2_tests.rs"]
mod d2_tests;
