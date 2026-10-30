//! OpcodeD4: 0049F1E0/0049F4C0 Handler-7-material per-corner quad.
//!
//! D6 with a material instead of a texture. Either handler walks one quad
//! (mirror-indexed slots and shade slots) through the 0048FA60 area cull, the
//! 00528BD0 clip gate and Far's all-fogged gate. It then looks the material
//! up once without reading it (00405540 over context `+4C` near, the same
//! lookup inline far), takes the flat sort key, and enqueues the quad through
//! 004A42B0 under the colored quad thunks (`0x44EA80` near, `0x44EA90` far,
//! four blocks each) that terrain's 0044D030 also feeds: four corner
//! XY/depth pairs, mode, the material record's first DWORD from a second
//! lookup, and each corner's whole shade DWORD; Far adds the far color and
//! four fog bytes. A quad that crosses the near plane goes to the per-corner
//! material wrapper 004997E0 (near) / 00499CE0 (far), shared with D0.
//! Natively the first lookup leaves a null material alone and the second
//! dereferences it; the host's lookup fails at the first, before sorting.

use super::{
    clip_or_skip, flat_sort_depth,
    material_per_corner_clipping::{
        prepare_material_per_corner_clipped, MaterialPerCornerClipPass,
    },
    per_corner_clipping::PerCornerVertices,
    quad::{all_fogged, positive_area},
    vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError,
    PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelQuadLighting};

pub fn prepare_d4<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let record = request.instruction.span.start;
    let ModelInstructionKind::Quad(quad) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(record));
    };
    if request.instruction.raw_command as u8 != 0xd4 {
        return Err(PrimitiveError::Record(record));
    };
    let ModelQuadLighting::PerVertex(shades) = &quad.lighting else {
        return Err(PrimitiveError::Record(record));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop: one pass normally, two (1, then 0) when bit 10 is set.
    for mirror in (0..=((raw >> 10) & 1)).rev() {
        let mut indices = [0; 4];
        for (index, slot) in indices.iter_mut().zip(quad.vertices) {
            *index = usize::try_from(i32::from(slot.raw() as i16) ^ i32::from(mirror))
                .map_err(|_| PrimitiveError::Record(record))?;
        }
        // The area cull only tests the first three corners, like D6.
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
            let material_slot = i32::from(quad.material.global_id);
            // The first lookup only loads the entry.
            host.material(material_slot).map_err(PrimitiveError::Host)?;
            let depth = vertex_depth(request, indices[0])?;
            let key = flat_sort_depth(depth, state.mode, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::ColoredQuadFar
                } else {
                    PrimitiveCallback::ColoredQuadNear
                },
                key,
                4,
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x20, state.mode);
            let material = host.material(material_slot).map_err(PrimitiveError::Host)?;
            output.write_u32(0x24, material);
            // Each corner's sign-extended, mirror-XORed shade WORD copies its
            // whole DWORD; an unknown byte stays unknown.
            for (corner, shade) in shades.iter().enumerate() {
                let shade = request
                    .shades
                    .scratch_shade(i32::from(shade.raw() as i16) ^ i32::from(mirror))?;
                if let Some(rgb) = shade.rgb {
                    output.write_rgb(0x28 + corner * 4, rgb);
                }
                if let Some(high) = shade.high_byte {
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
            // The calls at 0049F49D / 0049F7D9 pass the corners' shade
            // DWORDs, with count 4.
            let clipped = prepare_material_per_corner_clipped(
                request,
                MaterialPerCornerClipPass {
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
