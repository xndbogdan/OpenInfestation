//! Direct3D D2 textured gouraud triangle: kinds 004A3930/004A3960,
//! renderer virtuals 005290E8+64/+68. The near record carries corners,
//! mode, material and three per-corner shade DWORDs; far adds far color
//! and fog. Unlike DE (one shared shade) each corner resolves its own
//! shade DWORD; unlike DA the UVs come from the upload allocation
//! bounds with the same per-corner assignment
//! ((b14,b18),(b1C,b18),(b1C,b20)), proven by the D2-bounds oracle.
//! Original consumers 004C6AB0/004C8160 own flags, normalization and Far fog;
//! observed frame-1 SourceAlpha/default-depth pins are superseded.

use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    d3d_textured::{
        sprite_mode, D3dTextureBlend, D3dTextureFog, D3dTexturePolicy, D3dTexturedSubmission,
    },
    d3d_textured_triangle::{normalized_shade, scaled_shade, TexturedTriangleShading},
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

#[derive(Clone, Copy, Debug)]
pub struct D2Triangle<'a> {
    pub vertices: [FlatVertex; 3],
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    /// Per-corner shade DWORDs in R/G/B order (low three bytes); the
    /// high byte is unconsumed, like DE's shared word.
    pub shades: [[u8; 3]; 3],
    pub shading: TexturedTriangleShading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D2Continuation {
    Complete,
    SecondaryPassRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct D2Submission {
    pub primary: D3dTexturedSubmission<3, 1>,
    pub continuation: D2Continuation,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum D2PixelError {
    #[error("D2 triangle requires callback 004A3930/three blocks or 004A3960/four blocks")]
    Packet,
    #[error("D2 triangle is missing {field} at payload offset {offset:#x}")]
    Unknown { field: &'static str, offset: usize },
    #[error("D2 triangle requires a Handler-3 relocation at payload+1C")]
    Texture,
    #[error("D2 sprite selector {0} does not identify a physical descriptor")]
    Selector(usize),
    #[error("D2 primary pass completed; descriptor0200 plus renderer+44 requires the secondary texture pass")]
    SecondaryPass,
}

/// Selector FF suppresses the draw before coordinates, shades, upload
/// or renderer states are consumed. The triangle consumes +00..2F,
/// the far tail color and fog follow; any further bytes are unread.
pub fn read_d2<'a>(command: &PrimitiveCommand<'a>) -> Result<Option<D2Triangle<'a>>, D2PixelError> {
    let far = match command.callback {
        PrimitiveCallback::D2TriangleNear => false,
        PrimitiveCallback::D2TriangleFar => true,
        _ => return Err(D2PixelError::Packet),
    };
    let blocks = if far { 4 } else { 3 };
    if command.payload_blocks != blocks || command.payload.len() != blocks * 16 {
        return Err(D2PixelError::Packet);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(D2PixelError::Unknown { field, offset })
    };
    let mode = read(0x18, "mode")?;
    let Some((0x1c, resource)) = command.texture else {
        return Err(D2PixelError::Texture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(D2PixelError::Texture);
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let flags = entry
        .variants
        .get(selector)
        .ok_or(D2PixelError::Selector(selector))?
        .flags
        .raw();
    let shading = if far {
        let color = read(0x2c, "far color")?;
        let mut fog = [0; 3];
        for (i, value) in fog.iter_mut().enumerate() {
            let offset = 0x30 + i;
            *value = command.payload[offset].ok_or(D2PixelError::Unknown {
                field: "fog byte",
                offset,
            })?;
        }
        TexturedTriangleShading::Far { color, fog }
    } else {
        TexturedTriangleShading::Near
    };
    let mut vertices = [FlatVertex {
        x: 0.,
        y: 0.,
        depth: 0,
    }; 3];
    for (i, vertex) in vertices.iter_mut().enumerate() {
        let xy = read(i * 8, "projected XY")?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
    }
    let mut shades = [[0; 3]; 3];
    for (corner, shade) in shades.iter_mut().enumerate() {
        for (channel, value) in shade.iter_mut().enumerate() {
            let offset = 0x20 + corner * 4 + channel;
            *value = command.payload[offset].ok_or(D2PixelError::Unknown {
                field: "per-corner RGB",
                offset,
            })?;
        }
    }
    Ok(Some(D2Triangle {
        vertices,
        mode,
        resource,
        flags,
        shades,
        shading,
    }))
}

/// D2 submission: corners with per-corner diffuse over the bound
/// texture's allocation rect. UV assignment mirrors the native fill
/// (`(b14,b18)`, `(b1C,b18)`, `(b1C,b20)`); `uv_bounds` is that
/// allocation's +1C/+20/+24/+28, not parser-inferred bounds.
/// Old matrix: both callbacks ignored flags/Far tail, copied raw RGB and
/// pinned SourceAlpha/default depth-write. Native normalization/state/Far fog
/// replaces those pins; allocation-corner UV remains the proven D2 policy.
/// Primary004C7513/004C8CF3 precedes secondary004C756D/004C8D4D.
pub fn prepare_d2(
    triangle: &D2Triangle<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> D2Submission {
    let flags = triangle.flags;
    let (blend, alpha) = if flags & 0x10 != 0 {
        (D3dTextureBlend::Additive, 255)
    } else if flags & 0x20 != 0 {
        (D3dTextureBlend::InverseSourceColor, 255)
    } else if flags & 8 != 0 {
        (D3dTextureBlend::SourceAlpha, 128)
    } else if global_policy & 8 != 0 {
        (D3dTextureBlend::SourceAlpha, 255)
    } else {
        (D3dTextureBlend::Copy, 255)
    };
    let fog = match triangle.shading {
        TexturedTriangleShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
            color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
        },
        _ => D3dTextureFog::Disabled,
    };
    let [u0, v0, u1, v1] = uv_bounds;
    let uv = [[u0, v0], [u1, v0], [u1, v1]];
    let primary = D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let vertex = triangle.vertices[i];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let (rgb, specular) = match triangle.shading {
                TexturedTriangleShading::Far { fog, .. } if flags & 0x30 != 0 => (
                    scaled_shade(
                        triangle.shades[i],
                        f64::from(256 - u16::from(fog[i]) - u16::from(fog[i] >> 7)) / 128.0,
                    ),
                    0xff00_0000,
                ),
                TexturedTriangleShading::Far { fog, .. } => (
                    normalized_shade(triangle.shades[i]),
                    u32::from(255 - fog[i]) << 24,
                ),
                TexturedTriangleShading::Near => {
                    (normalized_shade(triangle.shades[i]), 0xff00_0000)
                }
            };
            D3dTlVertex {
                x: vertex.x,
                y: vertex.y,
                depth: (1.0 - f64::from(renderer.near_depth) * f64::from(rhw)) as f32,
                rhw,
                diffuse: [rgb[0], rgb[1], rgb[2], alpha],
                specular,
                uv: uv[i],
            }
        }),
        triangles: [[0, 1, 2]],
        policy: D3dTexturePolicy {
            depth_enable: renderer.manages_depth.then_some(renderer.depth_enable),
            depth_write: renderer.manages_depth.then_some(if flags & 0x39 != 0 {
                0
            } else {
                renderer.depth_write
            }),
            blend,
            perspective: triangle.mode & 0x100 == 0,
            fog,
        },
        mode: triangle.mode,
        near_depth: renderer.near_depth,
    };
    D2Submission {
        primary,
        continuation: if flags & 0x200 != 0 && secondary_texture {
            D2Continuation::SecondaryPassRequired
        } else {
            D2Continuation::Complete
        },
    }
}

#[cfg(test)]
#[path = "d3d_d2_tests.rs"]
mod tests;
