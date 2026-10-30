//! OpcodeE4: 0049D650/0049D8E0 Handler-7-material shared-shade face quad.
//!
//! Either handler walks one Handler-7-material quad (mirror-indexed
//! slots, one shared shade word) through the 0048FA60 area cull, the
//! 00528BD0 clip gate and material resolution, then enqueues it through
//! 004A42B0 under its own thunk (`0x4A39A0` near, `0x4A39D0` far).
//! The record is eleven DWORDs (four corner XY/depth pairs, mode, the
//! material record's first DWORD, shared shade DWORD); far adds the far
//! color and four fog bytes. Unlike E6 there is no brightness kernel:
//! the virtual gate feeds `flat_sort_depth` directly (`mode & 0x16`
//! selects the `(depth>>1)+0xC0000000` key, else `0x40000000`).
//! A quad that crosses the near plane goes to the material wrapper
//! 00498060 (near) / 00498570 (far), shared with DC. The epilogue
//! `lea eax,[edi+0xC]` proves the six-word instruction shape the
//! format decoder already consumes, so no decoder change is needed.
//! A null/unresolvable material has no retail skip path (the handler
//! dereferences the pointer unconditionally), so the port propagates
//! the host error fail-closed instead of skipping.

use super::{
    clip_or_skip, flat_sort_depth,
    material_clipping::{prepare_material_clipped, MaterialClipRequest},
    quad::{all_fogged, positive_area},
    shared_shade_clipping::SharedShadeVertices,
    vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError,
    PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelQuadLighting, ModelSlotRef};

pub fn prepare_e4<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Quad(quad) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0xe4 {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let ModelQuadLighting::Shared(shade) = quad.lighting else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let raw = request.instruction.raw_command;
    let far = request.dispatch == ModelDispatch::Far;
    let mut outcome = PrimitiveOutcome::default();
    // Mirror loop `dec esi; jns`: one pass normally, two (1, then 0)
    // when bit 10 is set, matching the E6 producer.
    for mirror in (0..=((raw >> 10) & 1)).rev() {
        let mut indices = [0; 4];
        for (index, slot) in indices.iter_mut().zip(quad.vertices) {
            *index = usize::try_from(i32::from(slot.raw() as i16) ^ i32::from(mirror))
                .map_err(|_| PrimitiveError::Record(request.instruction.span.start))?;
        }
        // The area cull only tests the first three corners, like E6.
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
            // Handler-7 material: the record carries the material
            // record's first DWORD, not a texture relocation.
            let material = host
                .material(i32::from(quad.material.global_id))
                .map_err(PrimitiveError::Host)?;
            let depth = vertex_depth(request, indices[0])?;
            let key = flat_sort_depth(depth, state.mode, host)?;
            let mut output = PrimitiveCommand::new(
                if far {
                    PrimitiveCallback::SharedShadeQuadFar
                } else {
                    PrimitiveCallback::SharedShadeQuadNear
                },
                key,
                if far { 4 } else { 3 },
            );
            for (corner, index) in indices.into_iter().enumerate() {
                output.write_u32(corner * 8, vertex_xy(request, index)?);
                output.write_u32(corner * 8 + 4, vertex_depth(request, index)?);
            }
            output.write_u32(0x20, state.mode);
            output.write_u32(0x24, material);
            // Shared shade DWORD: color in the low three bytes (R/G/B
            // order), mirroring E6's +0x28. The fourth byte stays zero;
            // the flat backend does not consume it.
            let shade_index = usize::try_from(i32::from(shade.raw() as i16) ^ i32::from(mirror))
                .map_err(|_| PrimitiveError::Record(request.instruction.span.start))?;
            let shade_value = request
                .shades
                .shade(ModelSlotRef::from_raw(u16::try_from(shade_index).map_err(
                    |_| PrimitiveError::Record(request.instruction.span.start),
                )?))?;
            let rgb = shade_value.rgb.ok_or(PrimitiveError::Unknown {
                field: "shared shade RGB",
                index: shade_index,
            })?;
            output.write_rgb(0x28, rgb);
            // High byte unproven for computed shades (the pool DWORD
            // always exists natively): E4 still stands in as zero. The
            // flat backend does not consume it.
            output.write_byte(0x2b, shade_value.high_byte.unwrap_or(0));
            if far {
                output.write_u32(0x2c, state.far_color);
                for (corner, index) in indices.into_iter().enumerate() {
                    output.write_byte(0x30 + corner, vertex_fog(request, index)?);
                }
            }
            host.submit(output).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
        } else if combined & 0xc0 == 0xc0 && clips.into_iter().fold(255, |a, b| a & b) & 0x40 == 0 {
            // 0049D875..8A7 / 0049DB6D..B9F read the literal shade WORDs at
            // record +0A before calling 00498060 / 00498570 with count 4.
            let start = request.instruction.span.start;
            let clipped = prepare_material_clipped(
                request,
                MaterialClipRequest {
                    mirror,
                    vertex_indices: SharedShadeVertices::Quad(indices),
                    nominal_shade: Some(shade.raw() as i16),
                    literal_shades_offset: start
                        .checked_add(12)
                        .ok_or(PrimitiveError::Record(start))?,
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
#[path = "e4_tests.rs"]
mod e4_tests;
