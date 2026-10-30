//! Per-corner RGB/UV wrappers 0049B070 (Near) /0049B6F0 (Far).
//!
//! Old matrix: eligible D2/D6 mixed clips stopped at these required wrappers;
//! direct/rejected faces used their existing paths. The original wrappers
//! call 00497250, optionally cull the first three results, then accept3/4/5.
//! Five emits quad0/1/2/3 followed by triangle0/3/4, each with independent
//! clip-table, H3, sorting and allocation gates. There is no all-FF fog gate.
//! Retained shade DWORDs include their owned high byte; new intersection
//! high bytes remain unwritten. Unknown source/interpolated bytes do not
//! prevent emission; reached pixel consumers own their required inputs.
//!
//! Native allocation precedes packet copying. The current complete-packet
//! submission API preserves earlier packets, not a partially written current
//! allocation when a later source read fails.

use super::{
    clipping::{clip_polygon, ClipSource, ClippedVertex},
    quad::positive_area,
    textured_sort_depth, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost,
    PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};
use inf_formats::{ModelInstructionKind, ModelQuadLighting, ModelTriangleLighting};

#[derive(Clone, Copy, Debug)]
pub(super) enum PerCornerVertices {
    Triangle([usize; 3]),
    Quad([usize; 4]),
}

impl PerCornerVertices {
    pub(super) fn as_slice(&self) -> &[usize] {
        match self {
            Self::Triangle(indices) => indices,
            Self::Quad(indices) => indices,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PerCornerClipPass {
    pub mirror: u16,
    pub vertex_indices: PerCornerVertices,
}

pub(super) fn prepare_per_corner_clipped<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    pass: PerCornerClipPass,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let opcode = request.instruction.raw_command as u8;
    let shades: &[_] = match (&pass.vertex_indices, &request.instruction.kind, opcode) {
        (PerCornerVertices::Triangle(_), ModelInstructionKind::Triangle(triangle), 0xd2) => {
            let ModelTriangleLighting::PerVertex(shades) = &triangle.lighting else {
                return Err(PrimitiveError::Record(request.instruction.span.start));
            };
            shades
        }
        (PerCornerVertices::Quad(_), ModelInstructionKind::Quad(quad), 0xd6) => {
            let ModelQuadLighting::PerVertex(shades) = &quad.lighting else {
                return Err(PrimitiveError::Record(request.instruction.span.start));
            };
            shades
        }
        _ => return Err(PrimitiveError::Record(request.instruction.span.start)),
    };
    // D2:49EEB0..49EEE6 /49F17E..49F1B4; D6:49FAAB..49FAEE /
    //49FE2A..49FE6F. Each caller copies every signed/mirror shade DWORD
    // before the wrapper/clipper, including a later geometry abort.
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
            .ok_or(PrimitiveError::Record(request.instruction.span.start))?;
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
    let clipped = clip_polygon(source, state.projection, tables, opcode)?;
    let raw = request.instruction.raw_command;
    let mut outcome = PrimitiveOutcome::default();
    // Native tests facing before count and may read unwritten stack for<3.
    // Both native count branches emit nothing; retain no fabricated point.
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
    let emission = PerCornerEmission { request, state };
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

/// Shared invocation inputs for the independent packets of a five-result clip.
struct PerCornerEmission<'request, 'invocation> {
    request: &'request ModelPrimitiveRequest<'invocation>,
    state: PrimitiveState,
}

impl PerCornerEmission<'_, '_> {
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
        let material = match &self.request.instruction.kind {
            ModelInstructionKind::Triangle(triangle) => triangle.material.global_id,
            ModelInstructionKind::Quad(quad) => quad.material.global_id,
            _ => unreachable!(),
        };
        let texture = host
            .texture(i32::from(material))
            .map_err(PrimitiveError::Host)?;
        let key = textured_sort_depth(vertices[0].projected.depth, self.state.mode, texture, host)?;
        let far = self.request.dispatch == ModelDispatch::Far;
        let triangle = vertices.len() == 3;
        let callback = match (triangle, far) {
            (true, false) => PrimitiveCallback::TexturedTriangleNear,
            (true, true) => PrimitiveCallback::TexturedTriangleFar,
            (false, false) => PrimitiveCallback::TexturedShadeUvQuadNear,
            (false, true) => PrimitiveCallback::TexturedShadeUvQuadFar,
        };
        let mut output = PrimitiveCommand::new(callback, key, if triangle { 5 } else { 6 });
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(corner * 8, vertex.projected.xy);
            output.write_u32(corner * 8 + 4, vertex.projected.depth);
        }
        let mode_offset = vertices.len() * 8;
        output.write_u32(mode_offset, self.state.mode);
        output.write_texture(mode_offset + 4, texture);
        for (corner, vertex) in vertices.iter().enumerate() {
            let color_offset = mode_offset + 8 + corner * 4;
            if let Some(rgb) = vertex.rgb {
                output.write_rgb(color_offset, rgb);
            }
            if let Some(high) = vertex.high_byte {
                output.write_byte(color_offset + 3, high);
            }
        }
        let uv_offset = mode_offset + 8 + vertices.len() * 4;
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(uv_offset + corner * 8, vertex.uv[0] as u32);
            output.write_u32(uv_offset + corner * 8 + 4, vertex.uv[1] as u32);
        }
        if far {
            let color_offset = uv_offset + vertices.len() * 8;
            output.write_u32(color_offset, self.state.far_color);
            for (corner, vertex) in vertices.iter().enumerate() {
                output.write_byte(
                    color_offset + 4 + corner,
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
#[path = "per_corner_clipping_tests.rs"]
mod tests;
