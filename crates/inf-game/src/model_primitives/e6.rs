//! OpcodeE6 direct producers 0049E1A0/0049E470 and shared clipped fallback.
//! Mirror1 then0, optional area cull, clip admission and Far all-FF fog rejection
//! precede H3 lookup. A native nullable texture still queries sorting and queues
//! its packet. Plain mode, shared shade DWORD and unwritten bytes retain custody;
//! the reached pixel consumer validates the bytes/resources it actually reads.

use super::{
    clip_or_skip,
    quad::{all_fogged, positive_area},
    shared_shade_clipping::{
        prepare_shared_shade_clipped, SharedShadeClipRequest, SharedShadeVertices,
    },
    textured_sort_depth, vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand,
    PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelQuadLighting};

pub fn prepare_e6<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Quad(quad) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0xe6 {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    }
    let ModelQuadLighting::Shared(shade) = quad.lighting else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop `dec esi; jns`: one pass normally, two (1, then 0)
    // when bit 10 is set, matching the E2 producer.
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
                    PrimitiveCallback::EffectQuadFar
                } else {
                    PrimitiveCallback::EffectQuadNear
                },
                key,
                // Far fog bytes run to +0x33, one block past E2's
                // layout: the native far record is 0x34 bytes.
                if far { 4 } else { 3 },
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x20, state.mode);
            output.write_texture(0x24, texture);
            //49E39E/49E696 reads a signed WORD XOR mirror into the shared
            //native arena, without a local shade-count check. Preserve mapped
            //unknown bytes; isolated fixtures still require a declared slot.
            let shade_value = request
                .shades
                .scratch_shade(i32::from(shade.raw() as i16) ^ i32::from(mirror))?;
            if let Some(rgb) = shade_value.rgb {
                output.write_rgb(0x28, rgb);
            }
            if let Some(high) = shade_value.high_byte {
                output.write_byte(0x2b, high);
            }
            if far {
                output.write_u32(0x2c, state.far_color);
                // Fog follows the far color (not the shade DWORD):
                // the E6 record runs four bytes longer than E2's.
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x30 + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            let clipped = prepare_shared_shade_clipped(
                request,
                SharedShadeClipRequest {
                    mirror,
                    vertex_indices: SharedShadeVertices::Quad(indices),
                    nominal_shade: shade.raw() as i16,
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

#[cfg(test)]
#[path = "e6_tests.rs"]
mod e6_tests;
