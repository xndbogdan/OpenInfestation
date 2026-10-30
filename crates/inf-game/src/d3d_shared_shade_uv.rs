//! E6 clipped shared-shade UV consumers in Direct3D vtable005290E8:
//! triangle 004CB6B0/004CCEF0 (+74/+78), quad 004E2550/004E4710 (+BC/+C0).
//! Their RGB and packet offsets differ from E2's white UV-only consumers.

use thiserror::Error;

use crate::{
    d3d_effect::{far_rgb, scaled_effect_rgb},
    d3d_flat::D3dFlatState,
    d3d_textured::{
        sprite_mode, D3dTextureBlend, D3dTextureFog, D3dTexturePolicy, D3dTexturedSubmission,
    },
    d3d_textured_uv::D3dTexturedUvSubmission,
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

#[derive(Clone, Copy, Debug)]
pub enum SharedShadeUvShading<const N: usize> {
    Near,
    Far { color: u32, fog: [u8; N] },
}

#[derive(Clone, Copy, Debug)]
pub enum SharedShadeUvGeometry {
    Triangle {
        vertices: [FlatVertex; 3],
        uv: [[i32; 2]; 3],
        shading: SharedShadeUvShading<3>,
    },
    Quad {
        vertices: [FlatVertex; 4],
        uv: [[i32; 2]; 4],
        shading: SharedShadeUvShading<4>,
    },
}

#[derive(Clone, Copy)]
enum PacketGeometry {
    Triangle,
    Quad,
}

#[derive(Clone, Copy, Debug)]
pub struct SharedShadeUvPrimitive<'a> {
    pub geometry: SharedShadeUvGeometry,
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    /// Shared shade low RGB bytes. The DWORD high byte is genuinely unread.
    pub rgb: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedShadeUvContinuation {
    Complete,
    /// The native primary draw precedes the still-unowned secondary setup.
    SecondaryPassRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SharedShadeUvSubmission {
    pub primary: D3dTexturedUvSubmission,
    pub continuation: SharedShadeUvContinuation,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SharedShadeUvPixelError {
    #[error("shared-shade UV packet requires its native callback and block layout")]
    Packet,
    #[error("shared-shade UV packet is missing {field} at payload offset {offset:#x}")]
    Unknown { field: &'static str, offset: usize },
    #[error("shared-shade UV packet requires a Handler-3 relocation at payload+{0:x}")]
    Texture(usize),
    #[error("shared-shade UV selector {0} does not identify a physical descriptor")]
    Selector(usize),
    #[error("shared-shade UV primary pass completed; descriptor0200 plus renderer+44 requires the secondary pass")]
    SecondaryPass,
}

/// SelectorFF rejects before RGB, UV, Far fog/color or pixel/device work.
/// Near tails3C/4C and Far triangle byte43 are not inputs to any primary pass.
pub fn read_shared_shade_uv<'a>(
    command: &PrimitiveCommand<'a>,
) -> Result<Option<SharedShadeUvPrimitive<'a>>, SharedShadeUvPixelError> {
    let (shape, blocks, mode_offset, far) = match command.callback {
        PrimitiveCallback::SharedShadeUvTriangleNear => (PacketGeometry::Triangle, 4, 0x18, false),
        PrimitiveCallback::SharedShadeUvTriangleFar => (PacketGeometry::Triangle, 5, 0x18, true),
        PrimitiveCallback::SharedShadeUvQuadNear => (PacketGeometry::Quad, 5, 0x20, false),
        PrimitiveCallback::SharedShadeUvQuadFar => (PacketGeometry::Quad, 6, 0x20, true),
        _ => return Err(SharedShadeUvPixelError::Packet),
    };
    if command.payload_blocks != blocks || command.payload.len() != blocks * 16 {
        return Err(SharedShadeUvPixelError::Packet);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(SharedShadeUvPixelError::Unknown { field, offset })
    };
    let mode = read(mode_offset, "mode")?;
    let texture_offset = mode_offset + 4;
    let Some((offset, resource)) = command.texture else {
        return Err(SharedShadeUvPixelError::Texture(texture_offset));
    };
    if offset != texture_offset {
        return Err(SharedShadeUvPixelError::Texture(texture_offset));
    }
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(SharedShadeUvPixelError::Texture(texture_offset));
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let flags = entry
        .variants
        .get(selector)
        .ok_or(SharedShadeUvPixelError::Selector(selector))?
        .flags
        .raw();
    let mut rgb = [0; 3];
    for (channel, value) in rgb.iter_mut().enumerate() {
        let offset = mode_offset + 8 + channel;
        *value = command.payload[offset].ok_or(SharedShadeUvPixelError::Unknown {
            field: "shared RGB",
            offset,
        })?;
    }
    let geometry = match shape {
        PacketGeometry::Triangle => {
            let (vertices, uv, shading) = read_geometry(command, 0x24, far)?;
            SharedShadeUvGeometry::Triangle {
                vertices,
                uv,
                shading,
            }
        }
        PacketGeometry::Quad => {
            let (vertices, uv, shading) = read_geometry(command, 0x2c, far)?;
            SharedShadeUvGeometry::Quad {
                vertices,
                uv,
                shading,
            }
        }
    };
    Ok(Some(SharedShadeUvPrimitive {
        geometry,
        mode,
        resource,
        flags,
        rgb,
    }))
}

fn read_geometry<const N: usize>(
    command: &PrimitiveCommand<'_>,
    uv_offset: usize,
    far: bool,
) -> Result<([FlatVertex; N], [[i32; 2]; N], SharedShadeUvShading<N>), SharedShadeUvPixelError> {
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(SharedShadeUvPixelError::Unknown { field, offset })
    };
    let shading = if far {
        let color_offset = uv_offset + N * 8;
        let color = read(color_offset, "far color")?;
        let mut fog = [0; N];
        for (i, value) in fog.iter_mut().enumerate() {
            let offset = color_offset + 4 + i;
            *value = command.payload[offset].ok_or(SharedShadeUvPixelError::Unknown {
                field: "fog byte",
                offset,
            })?;
        }
        SharedShadeUvShading::Far { color, fog }
    } else {
        SharedShadeUvShading::Near
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
            read(uv_offset + i * 8, "signed U")? as i32,
            read(uv_offset + i * 8 + 4, "signed V")? as i32,
        ];
    }
    Ok((vertices, uv, shading))
}

/// Primary color arithmetic is independently identical to effect/shared-shade
/// consumers. E6 UV is signed DWORD/65536 in allocation coordinates, not the
/// automatic three/four corners of those other packets. Native emits primary
/// fans before secondary setup; callers must preserve that committed prefix.
pub fn prepare_shared_shade_uv(
    primitive: &SharedShadeUvPrimitive<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> SharedShadeUvSubmission {
    let primary = match primitive.geometry {
        SharedShadeUvGeometry::Triangle {
            vertices,
            uv,
            shading,
        } => D3dTexturedUvSubmission::Triangle(prepare_geometry(
            primitive,
            vertices,
            uv,
            shading,
            [[0, 1, 2]],
            renderer,
            global_policy,
            uv_bounds,
        )),
        SharedShadeUvGeometry::Quad {
            vertices,
            uv,
            shading,
        } => {
            let k = ((!primitive.mode >> 14) & 1) as usize;
            D3dTexturedUvSubmission::Quad(prepare_geometry(
                primitive,
                vertices,
                uv,
                shading,
                [[k, (k + 1) & 3, (k + 2) & 3], [k, (k + 2) & 3, (k + 3) & 3]],
                renderer,
                global_policy,
                uv_bounds,
            ))
        }
    };
    SharedShadeUvSubmission {
        primary,
        continuation: if primitive.flags & 0x200 != 0 && secondary_texture {
            SharedShadeUvContinuation::SecondaryPassRequired
        } else {
            SharedShadeUvContinuation::Complete
        },
    }
}

fn prepare_geometry<const N: usize, const T: usize>(
    primitive: &SharedShadeUvPrimitive<'_>,
    vertices: [FlatVertex; N],
    uv: [[i32; 2]; N],
    shading: SharedShadeUvShading<N>,
    triangles: [[usize; 3]; T],
    renderer: D3dFlatState,
    global_policy: u32,
    bounds: [f32; 4],
) -> D3dTexturedSubmission<N, T> {
    let flags = primitive.flags;
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
    let fog = match shading {
        SharedShadeUvShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
            color: far_rgb(color, renderer.material_shifts),
        },
        _ => D3dTextureFog::Disabled,
    };
    D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let rhw = (1.0 / f64::from(vertices[i].depth)) as f32;
            let (rgb, specular) = match shading {
                SharedShadeUvShading::Far { fog, .. } if flags & 0x30 != 0 => (
                    scaled_effect_rgb(
                        primitive.rgb,
                        f64::from(256 - u16::from(fog[i]) - u16::from(fog[i] >> 7)) / 128.0,
                    ),
                    0xff00_0000,
                ),
                SharedShadeUvShading::Far { fog, .. } => (
                    scaled_effect_rgb(primitive.rgb, 255.0 / 128.0),
                    u32::from(255 - fog[i]) << 24,
                ),
                SharedShadeUvShading::Near => {
                    (scaled_effect_rgb(primitive.rgb, 255.0 / 128.0), 0xff00_0000)
                }
            };
            D3dTlVertex {
                x: vertices[i].x,
                y: vertices[i].y,
                depth: (1.0 - f64::from(renderer.near_depth) * f64::from(rhw)) as f32,
                rhw,
                diffuse: [rgb[0], rgb[1], rgb[2], alpha],
                specular,
                uv: std::array::from_fn(|axis| {
                    ((f64::from(uv[i][axis]) / 65536.0)
                        * (f64::from(bounds[axis + 2]) - f64::from(bounds[axis]))
                        + f64::from(bounds[axis])) as f32
                }),
            }
        }),
        triangles,
        policy: D3dTexturePolicy {
            depth_enable: renderer.manages_depth.then_some(renderer.depth_enable),
            depth_write: renderer.manages_depth.then_some(if flags & 0x39 != 0 {
                0
            } else {
                renderer.depth_write
            }),
            blend,
            perspective: primitive.mode & 0x100 == 0,
            fog,
        },
        mode: primitive.mode,
        near_depth: renderer.near_depth,
    }
}

#[cfg(test)]
#[path = "d3d_shared_shade_uv_tests.rs"]
mod tests;
