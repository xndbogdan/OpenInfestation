//! Direct3D D6 textured gouraud quad: kinds 0044EA60/0044EA70
//! (renderer thunks +AC/+B0 ->004DBA90/004DDA30). The near
//! record carries corners, mode, material and four per-corner
//! shade DWORDs; far adds far color and fog. UVs come
//! from the upload allocation bounds with the quad assignment
//! (`(u0,v0)`, `(u1,v0)`, `(u1,v1)`, `(u0,v1)`), the quad form of
//! the upload's +1C/+20/+24/+28 (004DBCF3/004DBE4F and successors).

use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    d3d_textured::{sprite_mode, D3dTextureBlend, D3dTexturePolicy, D3dTexturedSubmission},
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

#[derive(Clone, Copy, Debug)]
pub struct D6Quad<'a> {
    pub vertices: [FlatVertex; 4],
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    pub shades: [[u8; 3]; 4],
    pub shading: crate::d3d_textured::TexturedQuadShading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D6QuadContinuation {
    Complete,
    SecondaryPassRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct D6QuadSubmission {
    pub primary: D3dTexturedSubmission<4, 2>,
    pub continuation: D6QuadContinuation,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum D6PixelError {
    #[error("D6 quad requires callback 0044EA60/four blocks or 0044EA70/four blocks")]
    Packet,
    #[error("D6 quad is missing {field} at payload offset {offset:#x}")]
    Unknown { field: &'static str, offset: usize },
    #[error("D6 quad requires a Handler-3 relocation at payload+24")]
    Texture,
    #[error("D6 sprite selector {0} does not identify a physical descriptor")]
    Selector(usize),
    #[error("D6 primary pass completed; descriptor0200 plus renderer+44 requires the secondary texture pass")]
    SecondaryPass,
}

/// Selector FF suppresses the draw before coordinates, shades, upload
/// or renderer states are consumed. The quad consumes +00..37, the
/// far tail color and fog follow; any further bytes are unread.
pub fn read_d6<'a>(command: &PrimitiveCommand<'a>) -> Result<Option<D6Quad<'a>>, D6PixelError> {
    match command.callback {
        PrimitiveCallback::D6QuadNear | PrimitiveCallback::D6QuadFar => {}
        _ => return Err(D6PixelError::Packet),
    };
    let blocks = 4;
    if command.payload_blocks != blocks || command.payload.len() != blocks * 16 {
        return Err(D6PixelError::Packet);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(D6PixelError::Unknown { field, offset })
    };
    let mode = read(0x20, "mode")?;
    let Some((0x24, resource)) = command.texture else {
        return Err(D6PixelError::Texture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(D6PixelError::Texture);
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let variant = entry
        .variants
        .get(selector)
        .ok_or(D6PixelError::Selector(selector))?;
    let mut vertices = [FlatVertex {
        x: 0.,
        y: 0.,
        depth: 0,
    }; 4];
    for (i, vertex) in vertices.iter_mut().enumerate() {
        let xy = read(i * 8, "projected XY")?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
    }
    let mut shades = [[0; 3]; 4];
    for (corner, shade) in shades.iter_mut().enumerate() {
        for (channel, value) in shade.iter_mut().enumerate() {
            let offset = 0x28 + corner * 4 + channel;
            *value = command.payload[offset].ok_or(D6PixelError::Unknown {
                field: "per-corner shade",
                offset,
            })?;
        }
    }
    let shading = if command.callback == PrimitiveCallback::D6QuadFar {
        let color = read(0x38, "far color")?;
        let mut fog = [0; 4];
        for (index, value) in fog.iter_mut().enumerate() {
            let offset = 0x3c + index;
            *value = command.payload[offset].ok_or(D6PixelError::Unknown {
                field: "fog byte",
                offset,
            })?;
        }
        crate::d3d_textured::TexturedQuadShading::Far { color, fog }
    } else {
        crate::d3d_textured::TexturedQuadShading::Near
    };
    Ok(Some(D6Quad {
        vertices,
        mode,
        resource,
        flags: variant.flags.raw(),
        shades,
        shading,
    }))
}

/// Primary004DBAF2..004DBC19/004DDAB0..004DDC14 policies. Transparent
/// special blends fade shade directly; ordinary/alpha Far uses vertex fog.
/// Native fans precede secondary setup004DC938/004DEA68. Old matrix:
/// selectorFF skips all work; admitted packets prepare the same primary for
/// either renderer+44 value. Descriptor0200/+44 formerly failed here before
/// primary. Callers now commit the primary before reporting continuation.
pub fn prepare_d6(
    quad: &D6Quad<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> D6QuadSubmission {
    let flags = quad.flags;
    // Literal SRCBLEND/DESTBLEND writes and diffuse alpha, not a captured pin.
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
    let [u0, v0, u1, v1] = uv_bounds;
    let uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
    let k = ((!quad.mode >> 14) & 1) as usize;
    use crate::d3d_textured::{D3dTextureFog, TexturedQuadShading};
    let fog = match quad.shading {
        TexturedQuadShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
            color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
        },
        _ => D3dTextureFog::Disabled,
    };
    let primary = D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let vertex = quad.vertices[i];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let (rgb, specular) = match quad.shading {
                TexturedQuadShading::Far { fog, .. } if flags & 0x30 != 0 => (
                    crate::d3d_textured_triangle::scaled_shade(
                        quad.shades[i],
                        f64::from(256 - u16::from(fog[i]) - u16::from(fog[i] >> 7)) / 128.0,
                    ),
                    0xff00_0000,
                ),
                TexturedQuadShading::Far { fog, .. } => (
                    crate::d3d_textured_triangle::normalized_shade(quad.shades[i]),
                    u32::from(255 - fog[i]) << 24,
                ),
                TexturedQuadShading::Near => (
                    crate::d3d_textured_triangle::normalized_shade(quad.shades[i]),
                    0xff00_0000,
                ),
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
        triangles: [[k, (k + 1) & 3, (k + 2) & 3], [k, (k + 2) & 3, (k + 3) & 3]],
        policy: D3dTexturePolicy {
            depth_enable: renderer.manages_depth.then_some(renderer.depth_enable),
            depth_write: renderer.manages_depth.then_some(if flags & 0x39 != 0 {
                0
            } else {
                renderer.depth_write
            }),
            blend,
            perspective: quad.mode & 0x100 == 0,
            fog,
        },
        mode: quad.mode,
        near_depth: renderer.near_depth,
    };
    D6QuadSubmission {
        primary,
        continuation: if flags & 0x200 != 0 && secondary_texture {
            D6QuadContinuation::SecondaryPassRequired
        } else {
            D6QuadContinuation::Complete
        },
    }
}

#[cfg(test)]
#[path = "d3d_d6_tests.rs"]
mod tests;
