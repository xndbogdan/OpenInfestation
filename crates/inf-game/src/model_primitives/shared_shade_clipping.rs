//! Shared-shade UV wrappers 0049A270 (Near) /0049A930 (Far), reached by DE/E6.
//!
//! Old decision matrix: E6 supplies four sources to the owned wrapper; DE
//! supplies three but previously rejected that branch. E2 uses a UV-only
//! emitter and D6 consumes interpolated corner RGB; neither uses this emitter.
//! Other rejected faces enqueue nothing. Near and Far
//! wrappers both call the same 00497250, cull its first three results when100
//! is set, and accept only counts3/4/5. Five emits0/1/2/3 then0/3/4, with
//! independent clip/material/sort/submit boundaries. Neither wrapper has the
//! direct Far all-fog-FF gate. Null H3 still queries sorting and queues a null
//! relocation; the eventual pixel callback owns its required texture error.
//!
//! Geometry, UV and scratch RGB are the existing 00497250 policy. The emitted
//! packet instead carries the nominal shared shade, signed allocation UVs,
//! plain context mode and (Far only) color/fog. It cannot use E2's UV-only
//! callback or the direct effect quad's implicit UV corners.

use super::{
    clipping::{clip_polygon, ClipSource, ClippedVertex},
    quad::positive_area,
    textured_sort_depth, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost,
    PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::model_lighting::ModelShade;
use crate::{model_context::ModelDispatch, model_execution::ModelPrimitiveRequest};

#[derive(Clone, Copy, Debug)]
pub(super) enum SharedShadeVertices {
    Triangle([usize; 3]),
    Quad([usize; 4]),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SharedShadeClipRequest {
    pub mirror: u16,
    pub vertex_indices: SharedShadeVertices,
    pub nominal_shade: i16,
    /// Absolute original source byte offset of the first literal shade WORD.
    pub literal_shades_offset: usize,
    pub material: i32,
}

pub(super) fn prepare_shared_shade_clipped<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    pass: SharedShadeClipRequest,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let indices: &[usize] = match &pass.vertex_indices {
        SharedShadeVertices::Triangle(indices) => indices,
        SharedShadeVertices::Quad(indices) => indices,
    };
    // DE49CE12..37/49D0AF..D4 reads three signed WORDs at arguments+08;
    // E649E405..43B/49E721..753 reads four at+0A. The caller retains that
    // literal source offset; reads precede clipping even on a40/no80 abort.
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
    let emission = SharedShadeEmission {
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
struct SharedShadeEmission<'request, 'invocation> {
    request: &'request ModelPrimitiveRequest<'invocation>,
    pass: SharedShadeClipRequest,
    state: PrimitiveState,
}

impl SharedShadeEmission<'_, '_> {
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
            (true, false) => PrimitiveCallback::SharedShadeUvTriangleNear,
            (true, true) => PrimitiveCallback::SharedShadeUvTriangleFar,
            (false, false) => PrimitiveCallback::SharedShadeUvQuadNear,
            (false, true) => PrimitiveCallback::SharedShadeUvQuadFar,
        };
        let mut output =
            PrimitiveCommand::new(callback, depth, vertices.len() + usize::from(far) + 1);
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(corner * 8, vertex.projected.xy);
            output.write_u32(corner * 8 + 4, vertex.projected.depth);
        }
        let mode_offset = vertices.len() * 8;
        output.write_u32(mode_offset, state.mode);
        output.write_texture(mode_offset + 4, texture);
        // Native allocates before copying this DWORD. Mapped stale arena bytes
        // remain unknown in the queued packet; a pixel callback requires only
        // its reached three RGB bytes and does not read the high byte.
        let shared = request
            .shades
            .scratch_shade(i32::from(pass.nominal_shade) ^ i32::from(pass.mirror))?;
        if let Some(rgb) = shared.rgb {
            output.write_rgb(mode_offset + 8, rgb);
        }
        if let Some(high) = shared.high_byte {
            output.write_byte(mode_offset + 11, high);
        }
        for (corner, vertex) in vertices.iter().enumerate() {
            output.write_u32(mode_offset + 12 + corner * 8, vertex.uv[0] as u32);
            output.write_u32(mode_offset + 16 + corner * 8, vertex.uv[1] as u32);
        }
        if far {
            let color_offset = mode_offset + 12 + vertices.len() * 8;
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
#[path = "shared_shade_clipping_tests.rs"]
mod tests;
