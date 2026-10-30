//! OpcodeDE: 0049CC10/0049CE70 textured shared-shade face triangle.
//!
//! Either handler walks one Handler-3-textured triangle (mirror-indexed
//! slots, one shared shade word) through the 0048FA60 area cull, the
//! 00528BD0 clip gate and material resolution, then enqueues it through
//! 004A42B0 under its own thunk (`0x4A38C0` near, `0x4A3900` far). The
//! record is nine DWORDs (three corner XY/depth pairs, mode, material,
//! shared shade DWORD); far adds the far color and three fog bytes.
//! Plain context mode and nullable H3 retain their native ordering. Mixed
//! near-plane rejection calls the shared 0049A270/0049A930 wrapper with count3.
//! Old matrix: direct null previously skipped, computed high defaulted to zero,
//! and clipped faces failed closed. Direct Far all-FF rejection remains; the
//! clipped wrapper has no extra all-FF gate. Unknown arena bytes stay unknown.

use super::{
    clip_or_skip,
    quad::{all_fogged_triangle, positive_area},
    shared_shade_clipping::{
        prepare_shared_shade_clipped, SharedShadeClipRequest, SharedShadeVertices,
    },
    textured_sort_depth, vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand,
    PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelTriangleLighting};

pub fn prepare_de<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Triangle(triangle) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0xde {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let ModelTriangleLighting::Shared(shade) = triangle.lighting else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop `dec esi; jns`: one pass normally, two (1, then 0)
    // when bit 10 is set, matching the E6 producer.
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
            let key = textured_sort_depth(depth, state.mode, texture, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::SharedShadeTriangleFar
                } else {
                    PrimitiveCallback::SharedShadeTriangleNear
                },
                key,
                3,
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x18, state.mode);
            output.write_texture(0x1c, texture);
            // Shared shade DWORD: color in the low three bytes (R/G/B
            // order) plus the shade high byte, mirroring E6's +0x28.
            let shade_value = request
                .shades
                .scratch_shade(i32::from(shade.raw() as i16) ^ i32::from(mirror))?;
            if let Some(rgb) = shade_value.rgb {
                output.write_rgb(0x20, rgb);
            }
            if let Some(high) = shade_value.high_byte {
                output.write_byte(0x23, high);
            }
            if far {
                output.write_u32(0x24, state.far_color);
                // Far fog occupies +28..2A after color+24; the shared
                // shade remains at+20. E6's separate tail ends at+33.
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x28 + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            let clipped = prepare_shared_shade_clipped(
                request,
                SharedShadeClipRequest {
                    mirror,
                    vertex_indices: SharedShadeVertices::Triangle(indices),
                    nominal_shade: shade.raw() as i16,
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
#[path = "de_tests.rs"]
mod de_tests;
