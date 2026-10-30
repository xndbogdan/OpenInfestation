//! OpcodeD6: 0049F800/0049FB10 textured per-vertex-shaded face quad.
//!
//! Mirror1 then0, optional first-three facing, clip-table admission and
//! Far all-FF fog rejection precede sorting. The native H3 lookup occurs
//! twice: once for sorting and again after allocation/geometry for packet+24.
//! Null H3 still queues; copied signed shade DWORDs retain unknown bytes.
//! Eligible mixed near-plane faces use 0049B070/0049B6F0's per-corner RGB/UV
//! wrappers. The complete-packet host cannot reproduce partial allocation
//! writes before a later resource/read fault; receipts state that boundary.

use super::{
    clip_or_skip,
    per_corner_clipping::{prepare_per_corner_clipped, PerCornerClipPass, PerCornerVertices},
    quad::{all_fogged, positive_area},
    textured_sort_depth, vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand,
    PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelQuadLighting};

pub fn prepare_d6<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Quad(quad) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0xd6 {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    // Per-corner shades; shared-shade decodes belong to E4.
    let ModelQuadLighting::PerVertex(shades) = &quad.lighting else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop `dec ebx; jns`: one pass normally, two (1, then 0)
    // when bit 10 is set, matching the E4 producer.
    for mirror in (0..=((raw >> 10) & 1)).rev() {
        let mut indices = [0; 4];
        for (index, slot) in indices.iter_mut().zip(quad.vertices) {
            *index = usize::try_from(i32::from(slot.raw() as i16) ^ i32::from(mirror))
                .map_err(|_| PrimitiveError::Record(request.instruction.span.start))?;
        }
        // The area cull only tests the first three corners, like E4.
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
            //49F8D0/49FC0D owns the first lookup used only for sorting.
            let texture = host
                .texture(i32::from(quad.material.global_id))
                .map_err(PrimitiveError::Host)?;
            let depth = vertex_depth(request, indices[0])?;
            let key = textured_sort_depth(depth, state.mode, texture, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::D6QuadFar
                } else {
                    PrimitiveCallback::D6QuadNear
                },
                key,
                // Corners (8 DWORDs) + mode + four shade DWORDs run
                // to +0x34; the far tail (far color + four fog bytes)
                // ends exactly at +0x40.
                4,
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x20, state.mode);
            //49F9EC/49FD37 performs a fresh lookup after native allocation.
            // It may differ from the earlier sort resource. The shared host
            // queues complete packets, so a fault here cannot expose native
            // allocation/geometry prefix writes for the current packet.
            let packet_texture = host
                .texture(i32::from(quad.material.global_id))
                .map_err(PrimitiveError::Host)?;
            output.write_texture(0x24, packet_texture);
            for (corner, shade) in shades.iter().enumerate() {
                let shade_value = request
                    .shades
                    .scratch_shade(i32::from(shade.raw() as i16) ^ i32::from(mirror))?;
                if let Some(rgb) = shade_value.rgb {
                    output.write_rgb(0x28 + corner * 4, rgb);
                }
                if let Some(high) = shade_value.high_byte {
                    output.write_byte(0x2b + corner * 4, high);
                }
            }
            if far {
                output.write_u32(0x38, state.far_color);
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x3c + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            let clipped = prepare_per_corner_clipped(
                request,
                PerCornerClipPass {
                    mirror,
                    vertex_indices: PerCornerVertices::Quad(indices),
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
#[path = "d6_tests.rs"]
mod d6_tests;
