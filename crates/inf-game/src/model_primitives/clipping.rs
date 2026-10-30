//! 00497250's near-plane clipping, called with four sources by E2/E6/D6
//! and three by DE. Both counts use the first N original quad UV table rows.

use super::{
    effect::{axis_clip, multiply_q31, pack_xy},
    NativeProjectedVertex, PrimitiveError, PrimitiveTables,
};
use crate::model_context::{fistp_low32, ModelProjection};
use crate::model_lighting::ModelShade;
use crate::model_vertices::ModelVertexProjection;

#[derive(Clone, Copy, Debug)]
pub(super) enum ClipSource<'a> {
    Triangle([(&'a ModelVertexProjection, ModelShade); 3]),
    Quad([(&'a ModelVertexProjection, ModelShade); 4]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ClippedVertex {
    pub projected: NativeProjectedVertex,
    pub uv: [i32; 2],
    /// Shared-shade/UV-only emissions ignore scratch RGB; D6 serializes it.
    pub rgb: Option<[u8; 3]>,
    /// Retained vertices copy the complete source DWORD. Intersections write
    /// only RGB, leaving the scratch high byte unwritten.
    pub high_byte: Option<u8>,
}

pub(super) fn clip_polygon<E>(
    source: ClipSource<'_>,
    projection: ModelProjection,
    tables: &PrimitiveTables,
    opcode: u8,
) -> Result<Vec<ClippedVertex>, PrimitiveError<E>> {
    let sources: &[(&ModelVertexProjection, ModelShade)] = match &source {
        ClipSource::Triangle(vertices) => vertices,
        ClipSource::Quad(vertices) => vertices,
    };
    let clip = |index: usize| {
        sources[index].0.bytes[8].ok_or(PrimitiveError::Unknown {
            field: "clip byte",
            index,
        })
    };
    let read = |index: usize| -> Result<ClippedVertex, PrimitiveError<E>> {
        let (vertex, shade) = sources[index];
        Ok(ClippedVertex {
            projected: NativeProjectedVertex {
                xy: vertex.read_u32(0).ok_or(PrimitiveError::Unknown {
                    field: "clipped source XY",
                    index,
                })?,
                depth: vertex.read_u32(4).ok_or(PrimitiveError::Unknown {
                    field: "clipped source depth",
                    index,
                })?,
                clip: clip(index)?,
                fog: vertex.bytes[9],
            },
            uv: [tables.quad_u[index], tables.quad_v[index]],
            rgb: shade.rgb,
            high_byte: shade.high_byte,
        })
    };
    let mut output = Vec::with_capacity(6);
    for index in 0..sources.len() {
        let current_clip = clip(index)?;
        let previous_index = (index + sources.len() - 1) % sources.len();
        if (current_clip ^ clip(previous_index)?) & 0x40 != 0 {
            let (behind, front) = if current_clip & 0x40 != 0 {
                (read(index)?, read(previous_index)?)
            } else {
                (read(previous_index)?, read(index)?)
            };
            output.push(intersection(behind, front, projection, tables, opcode)?);
        }
        if current_clip & 0x40 == 0 {
            output.push(read(index)?);
        } else if current_clip & 0x80 == 0 {
            // 497617..621 abandons the complete polygon for this clip class.
            return Ok(Vec::new());
        }
    }
    Ok(output)
}

fn intersection<E>(
    behind: ClippedVertex,
    front: ClippedVertex,
    projection: ModelProjection,
    tables: &PrimitiveTables,
    opcode: u8,
) -> Result<ClippedVertex, PrimitiveError<E>> {
    let [sx, sy] = [
        i32::from(front.projected.xy as i16),
        i32::from((front.projected.xy >> 16) as i16),
    ];
    let front_depth = front.projected.depth as i32;
    let front_view = [
        divide(
            sx.wrapping_sub(projection.center[0])
                .wrapping_mul(front_depth),
            projection.focal[0],
            opcode,
        )?,
        divide(
            projection.center[1]
                .wrapping_sub(sy)
                .wrapping_mul(front_depth),
            projection.focal[1],
            opcode,
        )?,
    ];
    let behind_view = [
        i32::from(behind.projected.xy as i16),
        i32::from((behind.projected.xy >> 16) as i16),
    ];
    let behind_depth = behind.projected.depth as i32;
    // Native targets depth65 for the edge factor, but stores depth64 and
    // uses the fixed +/-1/64 projection constants for the resulting vertex.
    let factor = divide_q31(
        behind_depth.wrapping_sub(65),
        behind_depth.wrapping_sub(front_depth),
    )
    .max(0);
    let interpolate = |a: i32, b: i32| a.wrapping_add(multiply_q31(b.wrapping_sub(a), factor));
    let view: [i32; 2] =
        std::array::from_fn(|axis| interpolate(behind_view[axis], front_view[axis]));
    let mut x = projection.center[0].wrapping_sub(fistp_low32(
        (f64::from(view[0]) * f64::from(projection.focal[0]))
            * f64::from(f32::from_bits(tables.clip_projection_bits[1])),
    ));
    let mut y = projection.center[1].wrapping_sub(fistp_low32(
        (f64::from(view[1]) * f64::from(projection.focal[1]))
            * f64::from(f32::from_bits(tables.clip_projection_bits[0])),
    ));
    let mut magnitude = x.wrapping_abs() | y.wrapping_abs();
    while magnitude >= 32767 {
        magnitude >>= 1;
        x >>= 1;
        y >>= 1;
    }
    Ok(ClippedVertex {
        projected: NativeProjectedVertex {
            xy: pack_xy(x, y),
            depth: 64,
            clip: axis_clip(x, projection.viewport[0], [1, 2, 4])
                | axis_clip(y, projection.viewport[1], [8, 16, 32]),
            fog: Some(0),
        },
        uv: std::array::from_fn(|axis| interpolate(behind.uv[axis], front.uv[axis])),
        rgb: behind.rgb.zip(front.rgb).map(|(behind, front)| {
            std::array::from_fn(|channel| {
                interpolate(i32::from(behind[channel]), i32::from(front[channel])) as u8
            })
        }),
        high_byte: None,
    })
}

fn divide<E>(numerator: i32, denominator: i32, opcode: u8) -> Result<i32, PrimitiveError<E>> {
    numerator
        .checked_div(denominator)
        .ok_or(PrimitiveError::Unsupported {
            opcode,
            reason: "valid native signed division while reconstructing the clipped vertex",
        })
}

/// 0048EF30: unsigned magnitude division with saturation and signed result.
/// The native 0/0 case saturates rather than raising a hardware divide fault.
fn divide_q31(numerator: i32, denominator: i32) -> i32 {
    let negative = (numerator ^ denominator) < 0;
    let numerator = numerator.wrapping_abs() as u32;
    let denominator = denominator.wrapping_abs() as u32;
    if numerator >= denominator {
        return if negative { -i32::MAX } else { i32::MAX };
    }
    let quotient = ((u64::from(numerator) << 31) / u64::from(denominator)) as i32;
    if negative {
        quotient.wrapping_neg()
    } else {
        quotient
    }
}

#[cfg(test)]
#[path = "clipping_tests.rs"]
mod tests;
