//! OpcodeD8: 0049BE10/0049C020 Handler-7-material flat triangle.
//!
//! DC without its shade: either handler walks one material triangle
//! (mirror-indexed slots; the record's shade word is never read) through the
//! 0048FA60 area cull, the 00528BD0 clip gate, Far's all-fogged gate and the
//! inclusive material lookup, then enqueues it through 004A42B0 under its own
//! thunk (`0x4A38E0` near, two blocks; `0x4A3920` far, three): three corner
//! XY/depth pairs, mode and the material record's first DWORD; Far adds the
//! far color and three fog bytes. Sorting follows `flat_sort_depth`. A
//! triangle that crosses the near plane goes to the flat material wrapper
//! 004976A0 (near) / 00497B30 (far), shared with E0. A null material is
//! dereferenced natively, so the host error propagates.

use super::{
    clip_or_skip, flat_sort_depth,
    material_clipping::{prepare_material_clipped, MaterialClipRequest},
    quad::{all_fogged_triangle, positive_area},
    shared_shade_clipping::SharedShadeVertices,
    vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError,
    PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelTriangleLighting};

pub fn prepare_d8<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let record = request.instruction.span.start;
    let ModelInstructionKind::Triangle(triangle) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(record));
    };
    if request.instruction.raw_command as u8 != 0xd8 {
        return Err(PrimitiveError::Record(record));
    };
    let ModelTriangleLighting::Shared(_) = triangle.lighting else {
        return Err(PrimitiveError::Record(record));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop `dec esi; jns`: mirrored pass first when bit 10 is set.
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
                    PrimitiveCallback::SolidTriangleFar
                } else {
                    PrimitiveCallback::SolidTriangleNear
                },
                key,
                if far { 3 } else { 2 },
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x18, state.mode);
            output.write_u32(0x1c, material);
            if far {
                output.write_u32(0x20, state.far_color);
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x24 + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            // As DC's caller, the literal shade WORDs at record +08 load
            // before the calls at 0049BFFC / 0049C259, with count 3.
            let clipped = prepare_material_clipped(
                request,
                MaterialClipRequest {
                    mirror,
                    vertex_indices: SharedShadeVertices::Triangle(indices),
                    nominal_shade: None,
                    literal_shades_offset: record
                        .checked_add(10)
                        .ok_or(PrimitiveError::Record(record))?,
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
#[path = "flat_material_tests.rs"]
mod tests;
