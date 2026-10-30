//! OpcodeDC: 0049C280/0049C4B0 Handler-7-material shared-shade triangle.
//!
//! The triangle sibling of E4. Either handler walks one material triangle
//! (mirror-indexed slots, one shared shade word) through the 0048FA60 area
//! cull, the 00528BD0 clip gate, Far's all-fogged gate and the inclusive
//! material lookup, then enqueues three blocks through 004A42B0 under its own
//! thunk (`0x4A3950` near, `0x4A3980` far): three corner XY/depth pairs,
//! mode, the material record's first DWORD and the shade DWORD; Far adds the
//! far color and three fog bytes. The bytes after them stay unwritten.
//! Sorting follows `flat_sort_depth`. A triangle that crosses the near plane
//! goes to the material wrapper 00498060 (near) / 00498570 (far), shared with
//! E4. A null material is dereferenced natively, so the host error
//! propagates instead of a skip.

use super::{
    clip_or_skip, flat_sort_depth,
    material_clipping::{prepare_material_clipped, MaterialClipRequest},
    quad::{all_fogged_triangle, positive_area},
    shared_shade_clipping::SharedShadeVertices,
    vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError,
    PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelSlotRef, ModelTriangleLighting};

pub fn prepare_dc<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let record = request.instruction.span.start;
    let ModelInstructionKind::Triangle(triangle) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(record));
    };
    if request.instruction.raw_command as u8 != 0xdc {
        return Err(PrimitiveError::Record(record));
    };
    let ModelTriangleLighting::Shared(shade) = triangle.lighting else {
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
                    PrimitiveCallback::DcTriangleFar
                } else {
                    PrimitiveCallback::DcTriangleNear
                },
                key,
                3,
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x18, state.mode);
            output.write_u32(0x1c, material);
            // The whole shade DWORD is copied; an unknown high byte stays
            // unknown because the consumers read only RGB.
            let shade_index = u16::try_from(i32::from(shade.raw() as i16) ^ i32::from(mirror))
                .map_err(|_| PrimitiveError::Record(record))?;
            let shade = request.shades.shade(ModelSlotRef::from_raw(shade_index))?;
            let rgb = shade.rgb.ok_or(PrimitiveError::Unknown {
                field: "shared shade RGB",
                index: usize::from(shade_index),
            })?;
            output.write_rgb(0x20, rgb);
            if let Some(high) = shade.high_byte {
                output.write_byte(0x23, high);
            }
            if far {
                output.write_u32(0x24, state.far_color);
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x28 + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            // 0049C44B..470 / 0049C6B9..6DE read the literal shade WORDs at
            // record +08 before calling 00498060 / 00498570 with count 3.
            let clipped = prepare_material_clipped(
                request,
                MaterialClipRequest {
                    mirror,
                    vertex_indices: SharedShadeVertices::Triangle(indices),
                    nominal_shade: Some(shade.raw() as i16),
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
#[path = "dc_native_tests.rs"]
mod dc_native_tests;
