//! Material shared-shade wrappers 00498060 (Near) / 00498570 (Far), reached
//! by DC triangles and E4 quads that cross the near plane, and the flat
//! material wrappers 004976A0 / 00497B30 that D8 and E0 reach: the same code
//! without the shade DWORD, queueing D8's triangle and E0's solid quad.
//!
//! Each caller first loads one shade DWORD per corner through the signed
//! WORDs after its record (DC record +08, E4 +0A), XORed with the mirror.
//! Only 00497250's scratch RGB uses them, and neither wrapper reads it back.
//! The wrapper clips through 00497250 and, for command bit 8, culls the first
//! three results with 0048FA60 (bit 9 and the mirror flip it). Three results
//! queue a DC triangle, four an E4 quad, five the quad 0/1/2/3 and then the
//! triangle 0/3/4; other counts queue nothing. Each packet passes the
//! 00528BD0 gate on its own corners, looks up the Handler-7 material
//! inclusively, takes the flat sort key and stores the direct handlers'
//! layout: corners, mode, material, the nominal shade DWORD and, Far only,
//! the far color and the clipped corners' fog bytes. Neither wrapper has
//! Far's all-fogged gate. A null material is dereferenced natively, so the
//! host error propagates.

use super::{
    clipping::{clip_polygon, ClipSource, ClippedVertex},
    flat_sort_depth,
    quad::positive_area,
    shared_shade_clipping::SharedShadeVertices,
    PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveOutcome,
    PrimitiveState, PrimitiveTables,
};
use crate::model_lighting::ModelShade;
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};

#[derive(Clone, Copy, Debug)]
pub(super) struct MaterialClipRequest {
    pub mirror: u16,
    pub vertex_indices: SharedShadeVertices,
    /// DC's and E4's shade slot; D8 and E0 have none.
    pub nominal_shade: Option<i16>,
    /// Absolute original source byte offset of the first literal shade WORD.
    pub literal_shades_offset: usize,
    pub material: i32,
}

pub(super) fn prepare_material_clipped<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    pass: MaterialClipRequest,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let indices: &[usize] = match &pass.vertex_indices {
        SharedShadeVertices::Triangle(indices) => indices,
        SharedShadeVertices::Quad(indices) => indices,
    };
    // The callers' literal loads (E4 0049D875..8A7) precede the call, even
    // when the clipper then abandons the polygon.
    let mut colors = [ModelShade::default(); 4];
    for (corner, color) in colors.iter_mut().take(indices.len()).enumerate() {
        let offset = pass
            .literal_shades_offset
            .checked_add(corner * 2)
            .ok_or(PrimitiveError::Record(request.instruction.span.start))?;
        let bytes = request
            .source
            .bytes
            .get(offset..offset + 2)
            .ok_or(PrimitiveError::Record(offset))?;
        let index = i32::from(i16::from_le_bytes([bytes[0], bytes[1]])) ^ i32::from(pass.mirror);
        *color = request.shades.scratch_shade(index)?;
    }
    let vertex = |index: usize| {
        request
            .vertices
            .slots()
            .get(index)
            .map(|slot| &slot.projection)
            .ok_or(PrimitiveError::Record(request.instruction.span.start))
    };
    let source = match pass.vertex_indices {
        SharedShadeVertices::Triangle([a, b, c]) => ClipSource::Triangle([
            (vertex(a)?, colors[0]),
            (vertex(b)?, colors[1]),
            (vertex(c)?, colors[2]),
        ]),
        SharedShadeVertices::Quad([a, b, c, d]) => ClipSource::Quad([
            (vertex(a)?, colors[0]),
            (vertex(b)?, colors[1]),
            (vertex(c)?, colors[2]),
            (vertex(d)?, colors[3]),
        ]),
    };
    let clipped = clip_polygon(
        source,
        state.projection,
        tables,
        request.instruction.raw_command as u8,
    )?;
    let raw = request.instruction.raw_command;
    let mut outcome = PrimitiveOutcome::default();
    // 004980A9/004985B9 cull before switching on the count, reading stale
    // stack for counts below three; such counts queue nothing either way.
    if !(3..=5).contains(&clipped.len())
        || (raw & 0x100 != 0
            && (u16::from(positive_area([
                clipped[0].projected.xy,
                clipped[1].projected.xy,
                clipped[2].projected.xy,
            ])) ^ ((raw >> 9) & 1))
                == pass.mirror)
    {
        outcome.rejected += 1;
        return Ok(outcome);
    }
    let emission = MaterialEmission {
        request,
        pass,
        state,
    };
    match clipped.len() {
        3 | 4 => emission.emit(&clipped, tables, host, &mut outcome)?,
        5 => {
            emission.emit(&clipped[..4], tables, host, &mut outcome)?;
            emission.emit(
                &[clipped[0], clipped[3], clipped[4]],
                tables,
                host,
                &mut outcome,
            )?;
        }
        _ => unreachable!(),
    }
    Ok(outcome)
}

/// Invocation inputs shared by both packets of a five-vertex result.
struct MaterialEmission<'request, 'invocation> {
    request: &'request ModelPrimitiveRequest<'invocation>,
    pass: MaterialClipRequest,
    state: PrimitiveState,
}

impl MaterialEmission<'_, '_> {
    fn emit<'a, H: PrimitiveHost<'a>>(
        &self,
        vertices: &[ClippedVertex],
        tables: &PrimitiveTables,
        host: &mut H,
        outcome: &mut PrimitiveOutcome,
    ) -> Result<(), PrimitiveError<H::Error>> {
        let (request, pass, state) = (self.request, self.pass, self.state);
        let clip = vertices
            .iter()
            .fold(0, |value, vertex| value | vertex.projected.clip);
        if tables.clip_acceptance[usize::from(clip)] == 0 {
            outcome.rejected += 1;
            return Ok(());
        }
        let material = host.material(pass.material).map_err(PrimitiveError::Host)?;
        let depth = flat_sort_depth(vertices[0].projected.depth, state.mode, host)?;
        let far = request.dispatch == ModelDispatch::Far;
        let (callback, blocks) = match (vertices.len() == 3, far, pass.nominal_shade.is_some()) {
            (true, false, true) => (PrimitiveCallback::DcTriangleNear, 3),
            (true, true, true) => (PrimitiveCallback::DcTriangleFar, 3),
            (false, false, true) => (PrimitiveCallback::SharedShadeQuadNear, 3),
            (false, true, true) => (PrimitiveCallback::SharedShadeQuadFar, 4),
            (true, false, false) => (PrimitiveCallback::SolidTriangleNear, 2),
            (true, true, false) => (PrimitiveCallback::SolidTriangleFar, 3),
            (false, false, false) => (PrimitiveCallback::FlatQuadNear, 3),
            (false, true, false) => (PrimitiveCallback::FlatQuadFar, 3),
        };
        let mut output = PrimitiveCommand::new(callback, depth, blocks);
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(corner * 8, vertex.projected.xy);
            output.write_u32(corner * 8 + 4, vertex.projected.depth);
        }
        let mode_offset = vertices.len() * 8;
        output.write_u32(mode_offset, state.mode);
        output.write_u32(mode_offset + 4, material);
        let mut far_offset = mode_offset + 8;
        if let Some(nominal) = pass.nominal_shade {
            // The whole nominal shade DWORD is copied after allocation; the
            // consumers read only its RGB, as for the direct handlers.
            let index = i32::from(nominal) ^ i32::from(pass.mirror);
            let shade = request.shades.scratch_shade(index)?;
            let rgb = shade.rgb.ok_or(PrimitiveError::Unknown {
                field: "shared shade RGB",
                index: usize::from(index as u16),
            })?;
            output.write_rgb(far_offset, rgb);
            if let Some(high) = shade.high_byte {
                output.write_byte(far_offset + 3, high);
            }
            far_offset += 4;
        }
        if far {
            output.write_u32(far_offset, state.far_color);
            for (corner, vertex) in vertices.iter().enumerate() {
                output.write_byte(
                    far_offset + 4 + corner,
                    vertex.projected.fog.ok_or(PrimitiveError::Unknown {
                        field: "clipped fog byte",
                        index: corner,
                    })?,
                );
            }
        }
        host.submit(output).map_err(PrimitiveError::Host)?;
        outcome.commands += 1;
        Ok(())
    }
}

#[cfg(test)]
#[path = "material_clipping_tests.rs"]
mod tests;
