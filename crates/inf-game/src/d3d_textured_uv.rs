//! UV-only triangles 004C9A00/004CA7B0 and quads 004DFC80/004E1000, reached
//! through thunks004A3990/004A39C0 and004A3A20/004A3A60 (+6C/+70/+B4/+B8).
//! Near diffuse is white. Far flags10/20 fade that white by255-fog; ordinary
//! Far passes retain white with vertex fog. UV/depth retain signedness.

use crate::{
    d3d_flat::D3dFlatState,
    d3d_textured::{
        sprite_mode, D3dTextureBlend, D3dTextureFog, D3dTexturePolicy, D3dTexturedSubmission,
        TexturedPixelError,
    },
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

#[derive(Clone, Copy, Debug)]
pub enum TexturedUvGeometry {
    Triangle {
        vertices: [FlatVertex; 3],
        /// Signed DWORDs, in units of 1/65536 of allocation bounds.
        uv: [[i32; 2]; 3],
    },
    Quad {
        vertices: [FlatVertex; 4],
        /// Signed DWORDs, in units of 1/65536 of allocation bounds.
        uv: [[i32; 2]; 4],
    },
}

#[derive(Clone, Copy, Debug)]
pub struct TexturedUvPrimitive<'a> {
    pub geometry: TexturedUvGeometry,
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    pub shading: TexturedUvShading,
}

/// Far triangle consumes three fog bytes; the fourth array element is unused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedUvShading {
    Near,
    Far { color: u32, fog: [u8; 4] },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedUvContinuation {
    Complete,
    SecondaryPassRequired,
}

/// Converted primary geometry plus the required continuation after its fans.
#[derive(Clone, Debug, PartialEq)]
pub struct TexturedUvSubmission {
    pub primary: D3dTexturedUvSubmission,
    pub continuation: TexturedUvContinuation,
}

#[derive(Clone, Debug, PartialEq)]
pub enum D3dTexturedUvSubmission {
    Triangle(D3dTexturedSubmission<3, 1>),
    Quad(D3dTexturedSubmission<4, 2>),
}

/// FF selectors return before geometry/UV/upload/state reads. The triangle
/// consumes +00..37, the quad +00..47. Far also consumes a color DWORD and
/// three/four fog bytes; remaining padding is unread. Far color is reached
/// before geometry, matching004CA801/004E1051.
pub fn read_textured_uv<'a>(
    command: &PrimitiveCommand<'a>,
) -> Result<Option<TexturedUvPrimitive<'a>>, TexturedPixelError> {
    let (blocks, mode_offset, far) = match command.callback {
        PrimitiveCallback::TexturedUvTriangleNear => (4, 0x18, false),
        PrimitiveCallback::TexturedUvTriangleFar => (4, 0x18, true),
        PrimitiveCallback::TexturedUvQuadNear => (5, 0x20, false),
        PrimitiveCallback::TexturedUvQuadFar => (5, 0x20, true),
        _ => return Err(TexturedPixelError::UvPacket),
    };
    if command.payload_blocks != blocks || command.payload.len() != blocks * 16 {
        return Err(TexturedPixelError::UvPacket);
    }
    let mode = command
        .read_u32(mode_offset)
        .ok_or(TexturedPixelError::Unknown {
            field: "mode",
            offset: mode_offset,
        })?;
    let texture_offset = mode_offset + 4;
    let Some((offset, resource)) = command.texture else {
        return Err(TexturedPixelError::UvTexture(texture_offset));
    };
    if offset != texture_offset {
        return Err(TexturedPixelError::UvTexture(texture_offset));
    }
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(TexturedPixelError::UvTexture(texture_offset));
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let variant = entry
        .variants
        .get(selector)
        .ok_or(TexturedPixelError::Selector(selector))?;
    let shading = if far {
        let color_offset = if blocks == 4 { 0x38 } else { 0x48 };
        let color = command
            .read_u32(color_offset)
            .ok_or(TexturedPixelError::Unknown {
                field: "far color",
                offset: color_offset,
            })?;
        let count = if blocks == 4 { 3 } else { 4 };
        let mut fog = [0; 4];
        for (index, value) in fog.iter_mut().enumerate().take(count) {
            let offset = color_offset + 4 + index;
            *value = command.payload[offset].ok_or(TexturedPixelError::Unknown {
                field: "fog byte",
                offset,
            })?;
        }
        TexturedUvShading::Far { color, fog }
    } else {
        TexturedUvShading::Near
    };
    let geometry = match command.callback {
        PrimitiveCallback::TexturedUvTriangleNear | PrimitiveCallback::TexturedUvTriangleFar => {
            let (vertices, uv) = read_corners(command, 0x20)?;
            TexturedUvGeometry::Triangle { vertices, uv }
        }
        PrimitiveCallback::TexturedUvQuadNear | PrimitiveCallback::TexturedUvQuadFar => {
            let (vertices, uv) = read_corners(command, 0x28)?;
            TexturedUvGeometry::Quad { vertices, uv }
        }
        _ => unreachable!("callback checked above"),
    };
    Ok(Some(TexturedUvPrimitive {
        geometry,
        mode,
        resource,
        flags: variant.flags.raw(),
        shading,
    }))
}

fn read_corners<const N: usize>(
    command: &PrimitiveCommand<'_>,
    uv_offset: usize,
) -> Result<([FlatVertex; N], [[i32; 2]; N]), TexturedPixelError> {
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(TexturedPixelError::Unknown { field, offset })
    };
    let mut vertices = [FlatVertex {
        x: 0.,
        y: 0.,
        depth: 0,
    }; N];
    let mut uv = [[0; 2]; N];
    for i in 0..N {
        let xy = read(i * 8, "projected XY")?;
        vertices[i] = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
        uv[i] = [
            read(uv_offset + i * 8, "signed U coordinate")? as i32,
            read(uv_offset + i * 8 + 4, "signed V coordinate")? as i32,
        ];
    }
    Ok((vertices, uv))
}

/// Native conversion ends at004C9F84/004E031A/004CAE0F/004E17B4.
/// Ordinary triangle is one fan; quad is two separately clipped fans whose
/// diagonal depends on mode4000 (004E0E36). Caller owns upload/allocation;
/// uv_bounds is that allocation's +1C/+20/+24/+28, not parser-inferred bounds.
pub fn prepare_textured_uv(
    primitive: &TexturedUvPrimitive<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> TexturedUvSubmission {
    let flags = primitive.flags;
    // Native primary fans precede secondary descriptor reads at004CA04B,
    // 004E0521,004CAED2,004E19B7, even when viewport clipping removes them.
    let continuation = if flags & 0x200 != 0 && secondary_texture {
        TexturedUvContinuation::SecondaryPassRequired
    } else {
        TexturedUvContinuation::Complete
    };
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
    let policy = D3dTexturePolicy {
        depth_enable: renderer.manages_depth.then_some(renderer.depth_enable),
        depth_write: renderer.manages_depth.then_some(if flags & 0x39 != 0 {
            0
        } else {
            renderer.depth_write
        }),
        blend,
        perspective: primitive.mode & 0x100 == 0,
        fog: match primitive.shading {
            TexturedUvShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
                color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
            },
            _ => D3dTextureFog::Disabled,
        },
    };
    let convert = |index: usize, vertex: FlatVertex, uv: [i32; 2]| {
        let (rgb, specular) = match primitive.shading {
            TexturedUvShading::Far { fog, .. } if flags & 0x30 != 0 => {
                ([255 - fog[index]; 3], 0xff00_0000)
            }
            TexturedUvShading::Far { fog, .. } => ([255; 3], u32::from(255 - fog[index]) << 24),
            TexturedUvShading::Near => ([255; 3], 0xff00_0000),
        };
        let rhw = (1.0 / f64::from(vertex.depth)) as f32;
        D3dTlVertex {
            x: vertex.x,
            y: vertex.y,
            depth: (1.0 - f64::from(renderer.near_depth) * f64::from(rhw)) as f32,
            rhw,
            diffuse: [rgb[0], rgb[1], rgb[2], alpha],
            specular,
            uv: std::array::from_fn(|axis| {
                let start = f64::from(uv_bounds[axis]);
                let end = f64::from(uv_bounds[axis + 2]);
                ((f64::from(uv[axis]) / 65536.0) * (end - start) + start) as f32
            }),
        }
    };
    let primary = match primitive.geometry {
        TexturedUvGeometry::Triangle { vertices, uv } => {
            D3dTexturedUvSubmission::Triangle(D3dTexturedSubmission {
                vertices: std::array::from_fn(|i| convert(i, vertices[i], uv[i])),
                triangles: [[0, 1, 2]],
                policy,
                mode: primitive.mode,
                near_depth: renderer.near_depth,
            })
        }
        TexturedUvGeometry::Quad { vertices, uv } => {
            let k = ((!primitive.mode >> 14) & 1) as usize;
            D3dTexturedUvSubmission::Quad(D3dTexturedSubmission {
                vertices: std::array::from_fn(|i| convert(i, vertices[i], uv[i])),
                triangles: [[k, (k + 1) & 3, (k + 2) & 3], [k, (k + 2) & 3, (k + 3) & 3]],
                policy,
                mode: primitive.mode,
                near_depth: renderer.near_depth,
            })
        }
    };
    TexturedUvSubmission {
        primary,
        continuation,
    }
}

#[cfg(test)]
#[path = "d3d_textured_uv_tests.rs"]
mod tests;
