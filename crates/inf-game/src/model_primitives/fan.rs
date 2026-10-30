//! Opcode01, near 004966F0 and far 00496C90. These handlers perform fan
//! rejection and queue original projected triangles; they do not geometrically
//! clip them. Clipping belongs to the selected renderer callback.

use inf_formats::{ModelFanVertex, ModelFanVertexAttributes, ModelInstructionKind};

use super::{
    clip_or_skip, flat_sort_depth, projected_vertex, shade_rgb, textured_sort_depth, vertex_depth,
    vertex_xy, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost,
    PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{
    model_context::ModelDispatch, model_execution::ModelPrimitiveRequest,
    render_resources::RenderResource,
};

/// Material acquisition happens once, before either mirror pass or any culling.
/// A rejected first triangle ends its instance only when original flag2 is set;
/// flag4 continues testing later triangles. Mirror1 precedes ordinary mirror0.
pub fn prepare_fan<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::TriangleFan(fan) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 1 || fan.vertices.len() < 3 {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    }
    let textured = fan.flags.has_textured_records();
    let material = if textured {
        FanMaterial::Texture(
            host.texture(i32::from(fan.material.global_id))
                .map_err(PrimitiveError::Host)?,
        )
    } else {
        FanMaterial::Flat(
            host.material(i32::from(fan.material.global_id))
                .map_err(PrimitiveError::Host)?,
        )
    };
    let flags = fan.flags.raw();
    let mut outcome = PrimitiveOutcome::default();
    for mirror in (0..=((flags >> 4) & 1)).rev() {
        let first = corner(request, &fan.vertices[0], mirror, textured)?;
        let mut previous = corner(request, &fan.vertices[1], mirror, textured)?;
        let mut remaining_flags = flags;
        for record in &fan.vertices[2..] {
            // 004EBDE0 executes for this corner even if culling then rejects it.
            let current = corner(request, record, mirror, textured)?;
            let triangle = [first, previous, current];
            if remaining_flags & 6 != 0 {
                remaining_flags &= !2;
                let xy = [
                    vertex_xy(request, first.index)?,
                    vertex_xy(request, previous.index)?,
                    vertex_xy(request, current.index)?,
                ];
                if (u16::from(positive_area(xy)) ^ (remaining_flags & 1)) == mirror {
                    outcome.rejected += 1;
                    if flags & 2 != 0 {
                        break;
                    }
                    previous = current;
                    continue;
                }
            }
            let (Some(first_clip), Some(previous_clip), Some(current_clip)) = (
                clip_or_skip(request, first.index)?,
                clip_or_skip(request, previous.index)?,
                clip_or_skip(request, current.index)?,
            ) else {
                // Unprojected dynamic slot: skip the face, keep the walk.
                outcome.rejected += 1;
                previous = current;
                continue;
            };
            let clip = first_clip | previous_clip | current_clip;
            if tables.clip_acceptance[usize::from(clip)] == 0 {
                outcome.rejected += 1;
                previous = current;
                continue;
            }

            let depth = vertex_depth(request, first.index)?;
            // Native captures mode and first depth before renderer virtual+4.
            // The common helpers query sorting on every accepted triangle,
            // including a texture whose lookup returned the native null value.
            let key = match material {
                FanMaterial::Texture(texture) => {
                    textured_sort_depth(depth, state.mode, texture, host)?
                }
                FanMaterial::Flat(_) => flat_sort_depth(depth, state.mode, host)?,
            };
            let command = pack(request, state, material, triangle, key)?;
            host.submit(command).map_err(PrimitiveError::Host)?;
            outcome.commands += 1;
            previous = current;
        }
    }
    Ok(outcome)
}

#[derive(Clone, Copy)]
enum FanMaterial<'a> {
    Texture(Option<RenderResource<'a>>),
    Flat(u32),
}

#[derive(Clone, Copy)]
struct FanCorner {
    index: usize,
    rgb: [u8; 3],
    uv: Option<[u16; 2]>,
}

fn corner<E>(
    request: &ModelPrimitiveRequest<'_>,
    record: &ModelFanVertex,
    mirror: u16,
    textured: bool,
) -> Result<FanCorner, PrimitiveError<E>> {
    let (uv, factors) = match &record.attributes {
        ModelFanVertexAttributes::Textured {
            uv_words,
            lighting_and_end,
        } if textured => (Some(*uv_words), *lighting_and_end),
        ModelFanVertexAttributes::Untextured { lighting_and_end } if !textured => {
            (None, *lighting_and_end)
        }
        _ => return Err(PrimitiveError::Record(request.instruction.span.start)),
    };
    // MOVZX references, including indices above i16::MAX, then XOR parity.
    let index = usize::from(record.vertex.raw() ^ mirror);
    let shade = shade_rgb(request, usize::from(record.lighting_ref.raw() ^ mirror))?;
    let factors = [factors[0] as u8, (factors[0] >> 8) as u8, factors[1] as u8];
    let rgb = std::array::from_fn(|channel| {
        ((u32::from(shade[channel]) * u32::from(factors[channel])) >> 7).min(255) as u8
    });
    Ok(FanCorner { index, rgb, uv })
}

/// 0048FA60: signed16 screen components, wrapping32 products/subtraction, and
/// SETG. A zero or overflowed-negative determinant is not a positive face.
fn positive_area(points: [u32; 3]) -> bool {
    let coordinates = points.map(|word| [i32::from(word as i16), i32::from((word >> 16) as i16)]);
    let [[x0, y0], [x1, y1], [x2, y2]] = coordinates;
    (x1 - x0)
        .wrapping_mul(y2 - y0)
        .wrapping_sub((y1 - y0).wrapping_mul(x2 - x0))
        > 0
}

fn pack<'a, E>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    material: FanMaterial<'a>,
    corners: [FanCorner; 3],
    depth: u32,
) -> Result<PrimitiveCommand<'a>, PrimitiveError<E>> {
    let textured = matches!(material, FanMaterial::Texture(_));
    let far = request.dispatch == ModelDispatch::Far;
    let (callback, blocks) = match (textured, far) {
        (true, false) => (PrimitiveCallback::TexturedTriangleNear, 5),
        (true, true) => (PrimitiveCallback::TexturedTriangleFar, 5),
        (false, false) => (PrimitiveCallback::FlatTriangleNear, 3),
        (false, true) => (PrimitiveCallback::FlatTriangleFar, 4),
    };
    let mut command = PrimitiveCommand::new(callback, depth, blocks);
    let vertices = [
        projected_vertex(request, corners[0].index)?,
        projected_vertex(request, corners[1].index)?,
        projected_vertex(request, corners[2].index)?,
    ];
    for (index, vertex) in vertices.iter().enumerate() {
        command.write_u32(index * 8, vertex.xy);
        command.write_u32(index * 8 + 4, vertex.depth);
    }
    command.write_u32(0x18, state.mode);
    match material {
        FanMaterial::Texture(texture) => command.write_texture(0x1c, texture),
        FanMaterial::Flat(word) => command.write_u32(0x1c, word),
    }
    for (index, corner) in corners.iter().enumerate() {
        // Only three bytes were initialized in each temporary color DWORD.
        command.write_rgb(0x20 + index * 4, corner.rgb);
        if let Some([u, v]) = corner.uv {
            command.write_u32(0x2c + index * 8, u32::from(u));
            command.write_u32(0x30 + index * 8, u32::from(v));
        }
    }
    if far {
        let color_offset = if textured { 0x44 } else { 0x2c };
        command.write_u32(color_offset, state.far_color);
        for (index, vertex) in vertices.iter().enumerate() {
            let fog = vertex.fog.ok_or(PrimitiveError::Unknown {
                field: "vertex fog byte",
                index: corners[index].index,
            })?;
            command.write_byte(color_offset + 4 + index, fog);
        }
    }
    Ok(command)
}

#[cfg(test)]
#[path = "fan_tests.rs"]
mod tests;
