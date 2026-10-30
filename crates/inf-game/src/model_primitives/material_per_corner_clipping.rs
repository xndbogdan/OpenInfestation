//! Per-corner material wrappers 004997E0 (Near) / 00499CE0 (Far), reached by
//! D0 triangles and D4 quads that cross the near plane.
//!
//! Each caller passes one shade DWORD per corner: its record's shade WORDs,
//! sign-extended and XORed with the mirror. The wrapper clips through
//! 00497250 and, for command bit 8, culls the first three results with
//! 0048FA60 (bit 9 and the mirror flip it). Three results queue D0's flat
//! triangle, four D4's colored quad, five the quad 0/1/2/3 and then the
//! triangle 0/3/4; other counts queue nothing. Each packet passes the
//! 00528BD0 gate on its own corners, looks up the Handler-7 material
//! inclusively, takes the flat sort key and stores the direct handlers'
//! layout with each clipped corner's color DWORD: a retained corner copies
//! its whole source DWORD, an intersection only its RGB (the fourth byte is
//! the stack's, unknown here). Far adds the far color and the clipped
//! corners' fog bytes. Neither wrapper has Far's all-fogged gate. A null
//! material is dereferenced natively, so the host error propagates.

use super::{
    clipping::{clip_polygon, ClipSource, ClippedVertex},
    flat_sort_depth,
    per_corner_clipping::PerCornerVertices,
    quad::positive_area,
    PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveOutcome,
    PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelQuadLighting, ModelTriangleLighting};

#[derive(Clone, Copy, Debug)]
pub(super) struct MaterialPerCornerClipPass {
    pub mirror: u16,
    pub vertex_indices: PerCornerVertices,
}

pub(super) fn prepare_material_per_corner_clipped<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    pass: MaterialPerCornerClipPass,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let record = request.instruction.span.start;
    let (shades, material): (&[_], _) = match (&pass.vertex_indices, &request.instruction.kind) {
        (PerCornerVertices::Triangle(_), ModelInstructionKind::Triangle(triangle)) => {
            let ModelTriangleLighting::PerVertex(shades) = &triangle.lighting else {
                return Err(PrimitiveError::Record(record));
            };
            (shades, triangle.material.global_id)
        }
        (PerCornerVertices::Quad(_), ModelInstructionKind::Quad(quad)) => {
            let ModelQuadLighting::PerVertex(shades) = &quad.lighting else {
                return Err(PrimitiveError::Record(record));
            };
            (shades, quad.material.global_id)
        }
        _ => return Err(PrimitiveError::Record(record)),
    };
    // The callers copy every corner's shade DWORD before the call.
    let mut colors = Vec::with_capacity(shades.len());
    for shade in shades {
        colors.push(
            request
                .shades
                .scratch_shade(i32::from(shade.raw() as i16) ^ i32::from(pass.mirror))?,
        );
    }
    let mut sources = Vec::with_capacity(shades.len());
    for (index, color) in pass.vertex_indices.as_slice().iter().zip(colors) {
        let slot = request
            .vertices
            .slots()
            .get(*index)
            .ok_or(PrimitiveError::Record(record))?;
        sources.push((&slot.projection, color));
    }
    let source = match pass.vertex_indices {
        PerCornerVertices::Triangle(_) => {
            ClipSource::Triangle([sources[0], sources[1], sources[2]])
        }
        PerCornerVertices::Quad(_) => {
            ClipSource::Quad([sources[0], sources[1], sources[2], sources[3]])
        }
    };
    let clipped = clip_polygon(
        source,
        state.projection,
        tables,
        request.instruction.raw_command as u8,
    )?;
    let raw = request.instruction.raw_command;
    let mut outcome = PrimitiveOutcome::default();
    // 0049980B/00499D0B cull before switching on the count, reading stale
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
    let emission = Emission {
        request,
        material: i32::from(material),
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
struct Emission<'request, 'invocation> {
    request: &'request ModelPrimitiveRequest<'invocation>,
    material: i32,
    state: PrimitiveState,
}

impl Emission<'_, '_> {
    fn emit<'a, H: PrimitiveHost<'a>>(
        &self,
        vertices: &[ClippedVertex],
        tables: &PrimitiveTables,
        host: &mut H,
        outcome: &mut PrimitiveOutcome,
    ) -> Result<(), PrimitiveError<H::Error>> {
        let clip = vertices
            .iter()
            .fold(0, |value, vertex| value | vertex.projected.clip);
        if tables.clip_acceptance[usize::from(clip)] == 0 {
            outcome.rejected += 1;
            return Ok(());
        }
        let material = host.material(self.material).map_err(PrimitiveError::Host)?;
        let depth = flat_sort_depth(vertices[0].projected.depth, self.state.mode, host)?;
        let far = self.request.dispatch == ModelDispatch::Far;
        let (callback, blocks) = match (vertices.len() == 3, far) {
            (true, false) => (PrimitiveCallback::FlatTriangleNear, 3),
            (true, true) => (PrimitiveCallback::FlatTriangleFar, 4),
            (false, false) => (PrimitiveCallback::ColoredQuadNear, 4),
            (false, true) => (PrimitiveCallback::ColoredQuadFar, 4),
        };
        let mut output = PrimitiveCommand::new(callback, depth, blocks);
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(corner * 8, vertex.projected.xy);
            output.write_u32(corner * 8 + 4, vertex.projected.depth);
        }
        let mode_offset = vertices.len() * 8;
        output.write_u32(mode_offset, self.state.mode);
        output.write_u32(mode_offset + 4, material);
        for (corner, vertex) in vertices.iter().enumerate() {
            let color_offset = mode_offset + 8 + corner * 4;
            if let Some(rgb) = vertex.rgb {
                output.write_rgb(color_offset, rgb);
            }
            if let Some(high) = vertex.high_byte {
                output.write_byte(color_offset + 3, high);
            }
        }
        if far {
            let far_offset = mode_offset + 8 + vertices.len() * 4;
            output.write_u32(far_offset, self.state.far_color);
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
#[path = "material_per_corner_clipping_tests.rs"]
mod tests;
