//! Direct3D <=9 pixel-center and top-left coverage. See Microsoft's
//! Direct3D rasterization rules, linked in PIXEL_RUNTIME.md. Integer source
//! coordinates use exact f64 edge products, without epsilon overdraw.

use super::{D3dRasterState, SoftwareFrame};
use crate::d3d_textured::D3dTextureBlend;
use crate::d3d_viewport::D3dTlVertex;

fn edge(a: D3dTlVertex, b: D3dTlVertex, x: f64, y: f64) -> f64 {
    (x - f64::from(a.x)) * (f64::from(b.y) - f64::from(a.y))
        - (y - f64::from(a.y)) * (f64::from(b.x) - f64::from(a.x))
}

fn top_left(a: D3dTlVertex, b: D3dTlVertex) -> bool {
    b.y > a.y || (b.y == a.y && b.x < a.x)
}

pub(super) fn rasterize(
    frame: &mut SoftwareFrame,
    vertices: &[D3dTlVertex; 3],
    pipeline: &D3dRasterState,
    additive: bool,
) -> usize {
    rasterize_shaded(
        frame,
        vertices,
        pipeline,
        if additive {
            D3dTextureBlend::Additive
        } else {
            D3dTextureBlend::Copy
        },
        |_, color| Ok::<_, std::convert::Infallible>(color),
    )
    .unwrap()
}

pub(super) fn rasterize_shaded<E>(
    frame: &mut SoftwareFrame,
    vertices: &[D3dTlVertex; 3],
    pipeline: &D3dRasterState,
    blend: D3dTextureBlend,
    mut shade: impl FnMut([f64; 3], [u8; 4]) -> Result<[u8; 4], E>,
) -> Result<usize, E> {
    if frame.width == 0 || frame.height == 0 {
        return Ok(0);
    }
    let mut vertices = *vertices;
    let mut area = edge(
        vertices[0],
        vertices[1],
        vertices[2].x.into(),
        vertices[2].y.into(),
    );
    if area == 0.0 {
        return Ok(0);
    }
    let reversed = area < 0.0;
    if reversed {
        vertices.swap(1, 2);
        area = -area;
    }
    let left = vertices
        .iter()
        .map(|v| v.x)
        .fold(f32::INFINITY, f32::min)
        .ceil()
        .max(0.0) as i64;
    let right = vertices
        .iter()
        .map(|v| v.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .floor()
        .min((frame.width - 1) as f32) as i64;
    let top = vertices
        .iter()
        .map(|v| v.y)
        .fold(f32::INFINITY, f32::min)
        .ceil()
        .max(0.0) as i64;
    let bottom = vertices
        .iter()
        .map(|v| v.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .floor()
        .min((frame.height - 1) as f32) as i64;
    let edges = [
        (vertices[1], vertices[2]),
        (vertices[2], vertices[0]),
        (vertices[0], vertices[1]),
    ];
    let inclusive = edges.map(|(a, b)| top_left(a, b));
    let mut written = 0;
    for y in top..=bottom {
        for x in left..=right {
            let weights = edges.map(|(a, b)| edge(a, b, x as f64, y as f64));
            if weights
                .iter()
                .zip(inclusive)
                .any(|(&weight, include)| weight < 0.0 || (weight == 0.0 && !include))
            {
                continue;
            }
            let weights = weights.map(|w| w / area);
            let depth = (0..3)
                .map(|i| f64::from(vertices[i].depth) * weights[i])
                .sum::<f64>() as f32;
            let color = std::array::from_fn(|channel| {
                (0..3)
                    .map(|i| f64::from(vertices[i].diffuse[channel]) * weights[i])
                    .sum::<f64>()
                    .round_ties_even()
                    .clamp(0.0, 255.0) as u8
            });
            // Shader operands refer to the caller's original order even
            // when coverage normalized the triangle winding above.
            let mut source_weights = weights;
            if reversed {
                source_weights.swap(1, 2);
            }
            let color = shade(source_weights, color)?;
            written += usize::from(
                frame.write_fragment(x as u32, y as u32, depth, color, pipeline, blend),
            );
        }
    }
    Ok(written)
}
