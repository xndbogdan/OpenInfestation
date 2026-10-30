//! Direct3D ordinary Near/Far quads 004D5410/004D6570, fed by
//! callbacks 00466150/00466160. The Far packet/color policy remains explicit.
//! Texture upload/allocation is separate from the packet's borrowed relocation.

use inf_formats::SpriteRenderMode;
use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

#[derive(Clone, Copy, Debug)]
pub struct TexturedQuad<'a> {
    pub vertices: [FlatVertex; 4],
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    pub shading: TexturedQuadShading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedQuadShading {
    Near,
    Far { color: u32, fog: [u8; 4] },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum D3dTextureBlend {
    Copy,
    Additive,
    InverseSourceColor,
    SourceAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D3dTextureFog {
    Disabled,
    /// Native pretransformed vertices carry visibility in specular alpha.
    Vertex {
        color: [u8; 3],
    },
}

impl D3dTextureFog {
    pub fn enabled(self) -> bool {
        matches!(self, Self::Vertex { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D3dTexturePolicy {
    pub depth_enable: Option<u32>,
    pub depth_write: Option<u32>,
    pub blend: D3dTextureBlend,
    pub perspective: bool,
    pub fog: D3dTextureFog,
}

#[derive(Clone, Debug, PartialEq)]
pub struct D3dTexturedSubmission<const V: usize = 4, const T: usize = 2> {
    pub vertices: [D3dTlVertex; V],
    pub triangles: [[usize; 3]; T],
    pub policy: D3dTexturePolicy,
    pub mode: u32,
    pub near_depth: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TexturedPixelError {
    #[error("ordinary quad requires callback 00466150/00466160 and three payload blocks")]
    Packet,
    #[error("textured triangle requires callback 004A38F0/0044EA50 and five payload blocks")]
    TrianglePacket,
    #[error("near textured triangle requires a Handler-3 relocation at payload +1C")]
    TriangleTexture,
    #[error("shared-shade triangle requires callback 004A38C0/004A3900 and three payload blocks")]
    DePacket,
    #[error("shared-shade triangle requires a Handler-3 relocation at payload +1C")]
    DeTexture,
    #[error("near UV primitive requires callback 004A3990/four blocks or 004A3A20/five blocks")]
    UvPacket,
    #[error("near UV primitive requires a Handler-3 relocation at payload offset {0:#x}")]
    UvTexture(usize),
    #[error("textured packet is missing {field} at payload offset {offset:#x}")]
    Unknown { field: &'static str, offset: usize },
    #[error("ordinary quad requires a Handler-3 relocation at payload +24")]
    Texture,
    #[error("sprite selector {0} does not identify a physical descriptor")]
    Selector(usize),
    #[error("descriptor 0200 with renderer +44 requires the secondary texture pass")]
    SecondaryPass,
    #[error("texture sampling requires supported native {field} state, received {value}")]
    State { field: &'static str, value: u32 },
    #[error("texture sampling reached an unuploaded texel or nonfinite attribute")]
    Sample,
    #[error("CPU perspective sampling requires nonzero reciprocal depths of one sign")]
    ReciprocalDepthSign,
}

/// Selector FF suppresses the draw before coordinates, depth, upload or
/// renderer states are consumed. Near ignores +28..2F; Far requires its
/// converted color DWORD at +28 and four fog bytes at +2C..2F.
pub fn read_textured_quad<'a>(
    command: &PrimitiveCommand<'a>,
) -> Result<Option<TexturedQuad<'a>>, TexturedPixelError> {
    let far = match command.callback {
        PrimitiveCallback::TexturedQuadNear => false,
        PrimitiveCallback::TexturedQuadFar => true,
        _ => return Err(TexturedPixelError::Packet),
    };
    if command.payload_blocks != 3 || command.payload.len() != 48 {
        return Err(TexturedPixelError::Packet);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(TexturedPixelError::Unknown { field, offset })
    };
    let mode = read(0x20, "mode")?;
    let Some((0x24, resource)) = command.texture else {
        return Err(TexturedPixelError::Texture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(TexturedPixelError::Texture);
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let variant = entry
        .variants
        .get(selector)
        .ok_or(TexturedPixelError::Selector(selector))?;
    let shading = if far {
        let color = read(0x28, "far color")?;
        let mut fog = [0; 4];
        for (index, value) in fog.iter_mut().enumerate() {
            let offset = 0x2c + index;
            *value = command.payload[offset].ok_or(TexturedPixelError::Unknown {
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
    for (i, vertex) in vertices.iter_mut().enumerate() {
        let xy = read(i * 8, "projected XY")?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
    }
    Ok(Some(TexturedQuad {
        vertices,
        mode,
        resource,
        flags: variant.flags.raw(),
        shading,
    }))
}

pub const fn sprite_mode(mode: u32) -> SpriteRenderMode {
    if mode & 1 == 0 {
        SpriteRenderMode::Mode0
    } else {
        SpriteRenderMode::Mode1
    }
}

/// Primary Near 004D5472..004D5993 / Far 004D65C1..004D6C14 conversion,
/// then the mode-selected diagonal. Far flags 10/20 use literal 255-fog RGB
/// with device fog disabled; other Far paths use white with vertex fog.
/// `uv_bounds` belongs to the actual texture allocation, never the H3 parser.
pub fn prepare_textured_quad(
    quad: &TexturedQuad<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> Result<D3dTexturedSubmission, TexturedPixelError> {
    let flags = quad.flags;
    // Near 004D599D / Far 004D6C14 require a second pass only when both
    // descriptor 0200 and renderer+44 are present. Do not substitute primary.
    if flags & 0x200 != 0 && secondary_texture {
        return Err(TexturedPixelError::SecondaryPass);
    }
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
    // 004D65C1..004D6613 uses the same display-shift/replication conversion
    // as the independently recovered Far effect consumer.
    let fog = match quad.shading {
        TexturedQuadShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
            color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
        },
        _ => D3dTextureFog::Disabled,
    };
    Ok(D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let vertex = quad.vertices[i];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let (rgb, specular) = match quad.shading {
                TexturedQuadShading::Far { fog, .. } if flags & 0x30 != 0 => {
                    ([255 - fog[i]; 3], 0xff00_0000)
                }
                TexturedQuadShading::Far { fog, .. } => ([255; 3], u32::from(255 - fog[i]) << 24),
                TexturedQuadShading::Near => ([255; 3], 0xff00_0000),
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
    })
}

#[cfg(test)]
#[path = "d3d_textured_tests.rs"]
mod tests;
