//! 0044E150 terrain near-plane clipping. This is not model497250 clipping:
//! signed WORD terrain positions are interpolated and reprojected, while
//! colors/UVs retain the initial Q31 edge factor during projection retries.

use super::{
    plan::ftol_low32, quad::texture_sort, TerrainDrawError, TerrainDrawFault, TerrainVertex,
};
use crate::{
    model_context::{ModelProjection, ModelTransform},
    model_primitives::{
        NativeProjectedVertex, PrimitiveCallback, PrimitiveCommand, PrimitiveTables,
        TextureSortIdentity,
    },
    render_resources::RenderResource,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainClipContext {
    /// Actual published0055F1F8..228, shared with the row projector.
    pub transform: ModelTransform,
    pub projection: ModelProjection,
    /// Live0055F244 from the reached fade publication, not inferred per edge.
    pub fog_factor: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct TerrainClipRequest<'a> {
    /// Source order A(old),B(new),C(new+1),D(old+1). Projection/shade bytes
    /// preserve the row buffer even when its final offset reaches stale data.
    pub corners: [TerrainVertex; 4],
    /// 548314/16/18 signed WORDs, independently recomputed from current row
    /// metadata and the selected native height callback. Never reconstructed
    /// from projected XY or taken from a stale vertex's world coordinates.
    pub eye_relative: [[i16; 3]; 4],
    pub context: TerrainClipContext,
    pub texture: Option<RenderResource<'a>>,
    pub orientation: u32,
    /// Original context+62; E150 adds64, without the direct-quad spacing bias.
    pub sort_bias: i16,
    pub material: u32,
}

/// Native per-accepted-triangle effects. Queue submission retains earlier
/// commands when a later sorting/resource/submission operation fails.
pub trait TerrainClippingHost<'a> {
    type Error;
    fn material_sorting(&mut self) -> Result<bool, Self::Error>;
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error>;
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug)]
struct ClipVertex {
    projected: NativeProjectedVertex,
    rgb: [u8; 3],
    color_high: Option<u8>,
    uv: [i32; 2],
    position: [i32; 3],
}

/// Emits the original5-block 0044EA50 packets (renderer+80). The clip table
/// admits each generated fan triangle; there is no extra geometric cull.
pub fn clip_terrain_tile<'a, H: TerrainClippingHost<'a>>(
    request: TerrainClipRequest<'a>,
    tables: &PrimitiveTables,
    host: &mut H,
) -> Result<usize, TerrainDrawError<H::Error>> {
    let clips = request.corners.map(|v| v.projection.bytes[8]);
    let clips: [u8; 4] =
        super::all_written(clips).ok_or(TerrainDrawFault::Unwritten { field: "clip byte" })?;
    if clips.iter().all(|v| v & 0x40 == 0) || clips.iter().all(|v| v & 0x40 != 0) {
        // E150's gate precedes its unconditional texture dimension reads.
        return Ok(0);
    }
    let texture = request.texture.ok_or(TerrainDrawFault::Texture)?;
    let dimensions = texture
        .sprite_dimensions()
        .map_err(|_| TerrainDrawFault::Texture)?;
    let indices: [usize; 4] =
        std::array::from_fn(|i| ((request.orientation >> (i * 8)) & 15) as usize);
    if indices.iter().any(|i| *i > 3) || (0..4).any(|i| indices[..i].contains(&indices[i])) {
        return Err(TerrainDrawFault::Orientation(request.orientation).into());
    }
    let uv = [
        [0, 0],
        [i32::from(dimensions[0]), 0],
        dimensions.map(i32::from),
        [0, i32::from(dimensions[1])],
    ];
    let mut sources = [None; 4];
    for (source, target) in indices.into_iter().enumerate() {
        let corner = request.corners[source];
        sources[target] = Some(ClipVertex {
            projected: NativeProjectedVertex {
                // These words are read at actual copy/crossing boundaries.
                xy: 0,
                depth: 0,
                clip: clips[source],
                fog: corner.projection.bytes[9],
            },
            rgb: corner.rgb,
            color_high: Some(corner.cell_flags),
            uv: uv[target],
            position: request.eye_relative[source].map(i32::from),
        });
    }
    let sources = sources.map(Option::unwrap);
    let diagonal = ((request.corners[1].cell_flags & 8) >> 3) ^ ((!request.orientation & 1) as u8);
    let triangles = if diagonal == 0 {
        [[0, 1, 2], [0, 2, 3]]
    } else {
        [[1, 2, 3], [1, 3, 0]]
    };
    let read = |index: usize| -> Result<ClipVertex, TerrainDrawFault> {
        let source = indices.iter().position(|target| *target == index).unwrap();
        let projection = request.corners[source].projection;
        Ok(ClipVertex {
            projected: NativeProjectedVertex {
                // A crossing reads depth/position of its behind endpoint,
                // never that endpoint's projected XY. Visible copies require
                // XY separately at their reached copy boundary below.
                xy: projection.read_u32(0).unwrap_or(0),
                depth: projection.read_u32(4).ok_or(TerrainDrawFault::Unwritten {
                    field: "clipped source depth",
                })?,
                ..sources[index].projected
            },
            ..sources[index]
        })
    };
    let mut submitted = 0;
    for triangle in triangles {
        let mut polygon = Vec::with_capacity(4);
        let mut previous = triangle[2];
        for current in triangle {
            if (sources[current].projected.clip ^ sources[previous].projected.clip) & 0x40 != 0 {
                let (front, behind) = if sources[current].projected.clip & 0x40 != 0 {
                    (previous, current)
                } else {
                    (current, previous)
                };
                if let Some(vertex) = intersection(read(front)?, read(behind)?, request.context) {
                    polygon.push(vertex);
                }
            }
            if sources[current].projected.clip & 0x40 == 0 {
                let source = indices
                    .iter()
                    .position(|target| *target == current)
                    .unwrap();
                request.corners[source].projection.read_u32(0).ok_or(
                    TerrainDrawFault::Unwritten {
                        field: "clipped source XY",
                    },
                )?;
                polygon.push(read(current)?);
            }
            previous = current;
        }
        for end in 2..polygon.len() {
            let triangle = [polygon[0], polygon[end - 1], polygon[end]];
            let combined = triangle.iter().fold(0, |bits, v| bits | v.projected.clip);
            if tables.clip_acceptance[usize::from(combined)] == 0 {
                continue;
            }
            let mut depth = i32::from(request.sort_bias).wrapping_add(64) as u32;
            if host.material_sorting().map_err(TerrainDrawError::Host)? {
                depth = texture_sort(depth, texture, |texture| {
                    host.texture_sort_identity(texture)
                })?;
            }
            let mut command =
                PrimitiveCommand::new(PrimitiveCallback::TexturedTriangleFar, depth, 5);
            command.write_u32(0x18, 0);
            command.write_texture(0x1c, Some(texture));
            command.write_u32(0x44, request.material);
            for (index, vertex) in triangle.into_iter().enumerate() {
                command.write_u32(index * 8, vertex.projected.xy);
                command.write_u32(index * 8 + 4, vertex.projected.depth);
                command.write_rgb(0x20 + index * 4, vertex.rgb);
                if let Some(high) = vertex.color_high {
                    command.write_byte(0x23 + index * 4, high);
                }
                for axis in 0..2 {
                    let normalized = vertex.uv[axis]
                        .wrapping_shl(16)
                        .checked_div(i32::from(dimensions[axis]))
                        .ok_or(TerrainDrawFault::Division)?;
                    command.write_u32(0x2c + index * 8 + axis * 4, normalized as u32);
                }
                command.write_byte(
                    0x48 + index,
                    vertex.projected.fog.ok_or(TerrainDrawFault::Unwritten {
                        field: "clipped triangle fog",
                    })?,
                );
            }
            host.submit(command).map_err(TerrainDrawError::Host)?;
            submitted += 1;
        }
    }
    Ok(submitted)
}

fn intersection(
    front: ClipVertex,
    behind: ClipVertex,
    context: TerrainClipContext,
) -> Option<ClipVertex> {
    let front_depth = front.projected.depth as i32;
    let mut factor = divide_q31(
        front_depth.wrapping_sub(64),
        front_depth.wrapping_sub(behind.projected.depth as i32),
    );
    let interpolate =
        |a: i32, b: i32, factor| a.wrapping_sub(multiply_q31(a.wrapping_sub(b), factor));
    let rgb = std::array::from_fn(|i| {
        interpolate(i32::from(front.rgb[i]), i32::from(behind.rgb[i]), factor) as u8
    });
    let uv = std::array::from_fn(|i| interpolate(front.uv[i], behind.uv[i], factor));
    loop {
        let position =
            std::array::from_fn(|i| interpolate(front.position[i], behind.position[i], factor));
        // Native subtracts AFTER position interpolation but BEFORE admission.
        factor = factor.wrapping_sub(0x0100_0000);
        let projected = project(position, context);
        if projected.depth as i32 >= 64 {
            return (factor > 0).then_some(ClipVertex {
                projected,
                rgb,
                color_high: None,
                uv,
                position,
            });
        }
        if factor <= 0 {
            return None;
        }
    }
}

/// Bespoke E150 x87 addition order and CRT truncation; ordinary model
/// projection uses different ordering and nearest-even FISTP.
fn project(position: [i32; 3], context: TerrainClipContext) -> NativeProjectedVertex {
    let [x, y, z] = position.map(f64::from);
    let basis = |i| f64::from(f32::from_bits(context.transform.view_basis.words[i]));
    let view_x = ftol_low32((x * basis(1) + y * basis(4)) + z * basis(7))
        .wrapping_add(context.transform.view_position[0]);
    let view_y = ftol_low32((x * basis(2) + y * basis(5)) + z * basis(8))
        .wrapping_add(context.transform.view_position[1]);
    let depth = ftol_low32((y * basis(6) + z * basis(9)) + x * basis(3))
        .wrapping_add(context.transform.view_position[2]);
    let pack_xy = |x: i32, y: i32| u32::from(x as u16) | (u32::from(y as u16) << 16);
    if depth < 64 {
        return NativeProjectedVertex {
            xy: pack_xy(view_x, view_y),
            depth: depth as u32,
            clip: 0xc0,
            fog: None,
        };
    }
    let p = context.projection;
    let mut sx = ftol_low32(f64::from(p.focal[0]) * f64::from(view_x) / f64::from(depth))
        .wrapping_add(p.center[0]);
    let mut sy = p.center[1].wrapping_sub(ftol_low32(
        f64::from(p.focal[1]) * f64::from(view_y) / f64::from(depth),
    ));
    let mut magnitude = sx.wrapping_abs() | sy.wrapping_abs();
    while magnitude >= 32767 {
        magnitude >>= 1;
        sx >>= 1;
        sy >>= 1;
    }
    let axis = |value: i32, limit: i32, codes: [u8; 3]| {
        if (value as u32) < limit as u32 {
            codes[1]
        } else if value < 0 {
            codes[0]
        } else {
            codes[2]
        }
    };
    let fog = if depth < p.fade[0] {
        0
    } else if depth >= p.fade[1] {
        255
    } else {
        (depth
            .wrapping_sub(p.fade[0])
            .wrapping_mul(context.fog_factor)
            >> 16) as u8
    };
    NativeProjectedVertex {
        xy: pack_xy(sx, sy),
        depth: depth as u32,
        clip: axis(sx, p.viewport[0], [1, 2, 4]) | axis(sy, p.viewport[1], [8, 16, 32]),
        fog: Some(fog),
    }
}

fn multiply_q31(value: i32, factor: i32) -> i32 {
    ((i64::from(value) * i64::from(factor)) >> 31) as i32
}

/// 0048EF30 saturates unsigned magnitudes, including its native0/0 case.
fn divide_q31(numerator: i32, denominator: i32) -> i32 {
    let negative = (numerator ^ denominator) < 0;
    let numerator = numerator.wrapping_abs() as u32;
    let denominator = denominator.wrapping_abs() as u32;
    let quotient = if numerator >= denominator {
        i32::MAX
    } else {
        ((u64::from(numerator) << 31) / u64::from(denominator)) as i32
    };
    if negative {
        quotient.wrapping_neg()
    } else {
        quotient
    }
}

#[cfg(test)]
#[path = "clipping_tests.rs"]
mod tests;
