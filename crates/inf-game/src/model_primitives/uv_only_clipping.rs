//! UV-only wrappers 00498B10/00499130, shared by DA(count3) and E2(count4).
//!
//! Each caller supplies its exact literal source offset, material and mirrored
//! indices. Literal scratch DWORD reads precede clipping even on an abort;
//! scratch RGB/high bytes do not enter the output packet. Null H3 still queries
//! sorting and queues. Three/four results emit once; five emits0123 then034
//! with independent gates. Far wrappers have no direct all-fog-FF gate.
//!
//! Native allocation precedes later packet reads. This complete-packet submit
//! API retains completed packet prefixes but cannot expose the current native
//! partial allocation on a later read fault.

use super::{
    clipping::{clip_polygon, ClipSource, ClippedVertex},
    quad::positive_area,
    textured_sort_depth, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost,
    PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::model_lighting::ModelShade;
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};

#[derive(Clone, Copy, Debug)]
pub(super) enum UvOnlyVertices {
    Triangle([usize; 3]),
    Quad([usize; 4]),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct UvOnlyClipRequest {
    pub mirror: u16,
    pub vertex_indices: UvOnlyVertices,
    /// Absolute original source byte offset of the first literal shade WORD.
    pub literal_shades_offset: usize,
    pub material: i32,
}

pub(super) fn prepare_uv_only_clipped<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    pass: UvOnlyClipRequest,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let indices: &[usize] = match &pass.vertex_indices {
        UvOnlyVertices::Triangle(indices) => indices,
        UvOnlyVertices::Quad(indices) => indices,
    };
    // DA49C8FD..926/49CBA0..BC0 reads three signed WORDs at
    // arguments+08. E249DE2C..53 reads four at+0A.
    // These mandatory DWORD reads use the bounded original source arena.
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
        UvOnlyVertices::Triangle([a, b, c]) => ClipSource::Triangle([
            (vertex(a)?, colors[0]),
            (vertex(b)?, colors[1]),
            (vertex(c)?, colors[2]),
        ]),
        UvOnlyVertices::Quad([a, b, c, d]) => ClipSource::Quad([
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
    // Native culls before switching count, potentially reading unwritten
    // stack for counts<3. Such counts emit nothing in either branch; contain
    // that read without assigning a fabricated projected point.
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
    let emission = UvOnlyEmission {
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

/// Retained invocation inputs shared by both packets of a five-vertex result.
struct UvOnlyEmission<'request, 'invocation> {
    request: &'request ModelPrimitiveRequest<'invocation>,
    pass: UvOnlyClipRequest,
    state: PrimitiveState,
}

impl UvOnlyEmission<'_, '_> {
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
        let texture = host.texture(pass.material).map_err(PrimitiveError::Host)?;
        let depth = textured_sort_depth(vertices[0].projected.depth, state.mode, texture, host)?;
        let far = request.dispatch == ModelDispatch::Far;
        let triangle = vertices.len() == 3;
        let callback = match (triangle, far) {
            (true, false) => PrimitiveCallback::TexturedUvTriangleNear,
            (true, true) => PrimitiveCallback::TexturedUvTriangleFar,
            (false, false) => PrimitiveCallback::TexturedUvQuadNear,
            (false, true) => PrimitiveCallback::TexturedUvQuadFar,
        };
        let mut output = PrimitiveCommand::new(callback, depth, if triangle { 4 } else { 5 });
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(corner * 8, vertex.projected.xy);
            output.write_u32(corner * 8 + 4, vertex.projected.depth);
        }
        let mode_offset = vertices.len() * 8;
        output.write_u32(mode_offset, state.mode);
        output.write_texture(mode_offset + 4, texture);
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(mode_offset + 8 + corner * 8, vertex.uv[0] as u32);
            output.write_u32(mode_offset + 12 + corner * 8, vertex.uv[1] as u32);
        }
        if far {
            let color_offset = mode_offset + 8 + vertices.len() * 8;
            output.write_u32(color_offset, state.far_color);
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
#[path = "uv_only_clipping_tests.rs"]
mod tests;
