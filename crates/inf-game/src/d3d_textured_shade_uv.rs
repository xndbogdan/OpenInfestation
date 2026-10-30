//! Explicit-UV Gouraud quads emitted by 0049B070/0049B6F0.
//! Thunks 004A3940/004A3970 forward +C4/+C8 to 004E6B70/004E8D30.
//! Packet and UV policies remain distinct from direct D6's implicit UVs.

use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    d3d_textured::{
        sprite_mode, D3dTextureBlend, D3dTextureFog, D3dTexturePolicy, D3dTexturedSubmission,
        TexturedQuadShading,
    },
    d3d_textured_triangle::{normalized_shade, scaled_shade},
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

#[derive(Clone, Copy, Debug)]
pub struct TexturedShadeUvQuad<'a> {
    pub vertices: [FlatVertex; 4],
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    pub shades: [[u8; 3]; 4],
    /// Native signed 16.16 allocation coordinates, not intrinsic sprite pixels.
    pub uv: [[i32; 2]; 4],
    pub shading: TexturedQuadShading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedShadeUvContinuation {
    Complete,
    SecondaryPassRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexturedShadeUvSubmission {
    pub primary: D3dTexturedSubmission<4, 2>,
    pub continuation: TexturedShadeUvContinuation,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TexturedShadeUvPixelError {
    #[error("explicit-UV Gouraud quad requires callback 004A3940/004A3970 and six blocks")]
    Packet,
    #[error("explicit-UV Gouraud quad is missing {field} at payload offset {offset:#x}")]
    Unknown { field: &'static str, offset: usize },
    #[error("explicit-UV Gouraud quad requires a Handler-3 relocation at payload+24")]
    Texture,
    #[error("explicit-UV Gouraud quad selector {0} does not identify a physical descriptor")]
    Selector(usize),
    #[error("explicit-UV Gouraud primary pass completed; descriptor0200 plus renderer+44 requires the secondary pass")]
    SecondaryPass,
}

/// SelectorFF skips before shade, UV, geometry and Far fields. Near +58..5F
/// and each shade DWORD's high byte are genuinely unread by the primary pass.
pub fn read_shade_uv_quad<'a>(
    command: &PrimitiveCommand<'a>,
) -> Result<Option<TexturedShadeUvQuad<'a>>, TexturedShadeUvPixelError> {
    let far = match command.callback {
        PrimitiveCallback::TexturedShadeUvQuadNear => false,
        PrimitiveCallback::TexturedShadeUvQuadFar => true,
        _ => return Err(TexturedShadeUvPixelError::Packet),
    };
    if command.payload_blocks != 6 || command.payload.len() != 96 {
        return Err(TexturedShadeUvPixelError::Packet);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(TexturedShadeUvPixelError::Unknown { field, offset })
    };
    let mode = read(0x20, "mode")?;
    let Some((0x24, resource)) = command.texture else {
        return Err(TexturedShadeUvPixelError::Texture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(TexturedShadeUvPixelError::Texture);
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let flags = entry
        .variants
        .get(selector)
        .ok_or(TexturedShadeUvPixelError::Selector(selector))?
        .flags
        .raw();
    let shading = if far {
        let color = read(0x58, "far color")?;
        let mut fog = [0; 4];
        for (i, value) in fog.iter_mut().enumerate() {
            let offset = 0x5c + i;
            *value = command.payload[offset].ok_or(TexturedShadeUvPixelError::Unknown {
                field: "fog byte",
                offset,
            })?;
        }
        TexturedQuadShading::Far { color, fog }
    } else {
        TexturedQuadShading::Near
    };
    let mut vertices = [FlatVertex {
        x: 0.,
        y: 0.,
        depth: 0,
    }; 4];
    let mut shades = [[0; 3]; 4];
    let mut uv = [[0; 2]; 4];
    for i in 0..4 {
        let xy = read(i * 8, "projected XY")?;
        vertices[i] = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
        for (channel, value) in shades[i].iter_mut().enumerate() {
            let offset = 0x28 + i * 4 + channel;
            *value = command.payload[offset].ok_or(TexturedShadeUvPixelError::Unknown {
                field: "per-corner RGB",
                offset,
            })?;
        }
        uv[i] = [
            read(0x38 + i * 8, "signed U")? as i32,
            read(0x3c + i * 8, "signed V")? as i32,
        ];
    }
    Ok(Some(TexturedShadeUvQuad {
        vertices,
        mode,
        resource,
        flags,
        shades,
        uv,
        shading,
    }))
}

/// Native primary fans precede secondary setup 004E7B27/004E9E70. Callers
/// must commit the primary submission before reporting that continuation.
pub fn prepare_shade_uv_quad(
    quad: &TexturedShadeUvQuad<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    bounds: [f32; 4],
) -> TexturedShadeUvSubmission {
    let flags = quad.flags;
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
    let fog = match quad.shading {
        TexturedQuadShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
            color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
        },
        _ => D3dTextureFog::Disabled,
    };
    let k = ((!quad.mode >> 14) & 1) as usize;
    let primary = D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let vertex = quad.vertices[i];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let (rgb, specular) = match quad.shading {
                TexturedQuadShading::Far { fog, .. } if flags & 0x30 != 0 => (
                    scaled_shade(
                        quad.shades[i],
                        f64::from(256 - u16::from(fog[i]) - u16::from(fog[i] >> 7)) / 128.0,
                    ),
                    0xff00_0000,
                ),
                TexturedQuadShading::Far { fog, .. } => (
                    normalized_shade(quad.shades[i]),
                    u32::from(255 - fog[i]) << 24,
                ),
                TexturedQuadShading::Near => (normalized_shade(quad.shades[i]), 0xff00_0000),
            };
            D3dTlVertex {
                x: vertex.x,
                y: vertex.y,
                depth: (1.0 - f64::from(renderer.near_depth) * f64::from(rhw)) as f32,
                rhw,
                diffuse: [rgb[0], rgb[1], rgb[2], alpha],
                specular,
                uv: std::array::from_fn(|axis| {
                    (f64::from(quad.uv[i][axis]) / 65536.0
                        * (f64::from(bounds[axis + 2]) - f64::from(bounds[axis]))
                        + f64::from(bounds[axis])) as f32
                }),
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
    TexturedShadeUvSubmission {
        primary,
        continuation: if flags & 0x200 != 0 && secondary_texture {
            TexturedShadeUvContinuation::SecondaryPassRequired
        } else {
            TexturedShadeUvContinuation::Complete
        },
    }
}

#[cfg(test)]
#[path = "d3d_textured_shade_uv_tests.rs"]
mod tests;
