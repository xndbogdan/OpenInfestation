//! OpcodeD0: 0049E790/0049E9F0 Handler-7-material per-corner triangle.
//!
//! D2 with a material instead of a texture. Either handler walks one
//! triangle (mirror-indexed slots and shade slots) through the 0048FA60 area
//! cull, the 00528BD0 clip gate, Far's all-fogged gate and the inclusive
//! material lookup, then enqueues it through 004A42B0 under the flat
//! triangle thunks (`0x4A39B0` near, three blocks; `0x4A39E0` far, four):
//! three corner XY/depth pairs, mode, the material record's first DWORD and
//! each corner's whole shade DWORD; Far adds the far color and three fog
//! bytes. Sorting follows `flat_sort_depth`. A triangle that crosses the near
//! plane goes to the per-corner material wrapper 004997E0 (near) / 00499CE0
//! (far), shared with D4. A null material is dereferenced natively, so the
//! host error propagates.

use super::{
    clip_or_skip, flat_sort_depth,
    material_per_corner_clipping::{
        prepare_material_per_corner_clipped, MaterialPerCornerClipPass,
    },
    per_corner_clipping::PerCornerVertices,
    quad::{all_fogged_triangle, positive_area},
    vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError,
    PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelTriangleLighting};

pub fn prepare_d0<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let record = request.instruction.span.start;
    let ModelInstructionKind::Triangle(triangle) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(record));
    };
    if request.instruction.raw_command as u8 != 0xd0 {
        return Err(PrimitiveError::Record(record));
    };
    let ModelTriangleLighting::PerVertex(shades) = &triangle.lighting else {
        return Err(PrimitiveError::Record(record));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop: one pass normally, two (1, then 0) when bit 10 is set.
    for mirror in (0..=((raw >> 10) & 1)).rev() {
        let mut indices = [0; 3];
        for (index, slot) in indices.iter_mut().zip(triangle.vertices) {
            *index = usize::try_from(i32::from(slot.raw() as i16) ^ i32::from(mirror))
                .map_err(|_| PrimitiveError::Record(record))?;
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
            let material = host
                .material(i32::from(triangle.material.global_id))
                .map_err(PrimitiveError::Host)?;
            let depth = vertex_depth(request, indices[0])?;
            let key = flat_sort_depth(depth, state.mode, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::FlatTriangleFar
                } else {
                    PrimitiveCallback::FlatTriangleNear
                },
                key,
                if far { 4 } else { 3 },
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x18, state.mode);
            output.write_u32(0x1c, material);
            // Each corner's sign-extended, mirror-XORed shade WORD copies its
            // whole DWORD; an unknown byte stays unknown.
            for (corner, shade) in shades.iter().enumerate() {
                let shade = request
                    .shades
                    .scratch_shade(i32::from(shade.raw() as i16) ^ i32::from(mirror))?;
                if let Some(rgb) = shade.rgb {
                    output.write_rgb(0x20 + corner * 4, rgb);
                }
                if let Some(high) = shade.high_byte {
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
            // The calls at 0049E9C8 / 0049EC66 pass the corners' shade
            // DWORDs, with count 3.
            let clipped = prepare_material_per_corner_clipped(
                request,
                MaterialPerCornerClipPass {
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
#[path = "material_per_corner_tests.rs"]
mod tests;
