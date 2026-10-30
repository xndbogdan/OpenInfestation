//! Textured triangles 004CE930/004D0170: callbacks 004A38F0/0044EA50
//! dispatch Direct3D vtable 005290E8+7C/+80. Shade and UV are per corner;
//! texture upload/allocation remains owned by the borrowed H3 resource.
//! Shared-shade DE triangles have separate consumers:004C3B60 Near/+5C
//! and 004C5210 Far/+60. Their allocation-corner UV and fog policies are explicit.

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
pub struct TexturedTriangle<'a> {
    pub vertices: [FlatVertex; 3],
    /// Three significant bytes in each packet color DWORD, in RGB order.
    pub shades: [[u8; 3]; 3],
    /// Signed DWORDs at +2C..40, in units of 1/65536 of allocation bounds.
    /// The model producer zero-extends authored WORDs; this consumer uses FILD32.
    pub uv: [[i32; 2]; 3],
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    pub shading: TexturedTriangleShading,
}

/// 004CE93A..004CE953 selects the H3 mode before reading geometry or shade.
/// Selector FF suppresses even an otherwise incomplete packet. Color high
/// bytes and near-packet padding +44..4F do not contribute to this consumer.
pub fn read_textured_triangle<'a>(
    command: &PrimitiveCommand<'a>,
) -> Result<Option<TexturedTriangle<'a>>, TexturedPixelError> {
    let far = match command.callback {
        PrimitiveCallback::TexturedTriangleNear => false,
        PrimitiveCallback::TexturedTriangleFar => true,
        _ => return Err(TexturedPixelError::TrianglePacket),
    };
    if command.payload_blocks != 5 || command.payload.len() != 80 {
        return Err(TexturedPixelError::TrianglePacket);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(TexturedPixelError::Unknown { field, offset })
    };
    let mode = read(0x18, "mode")?;
    let Some((0x1c, resource)) = command.texture else {
        return Err(TexturedPixelError::TriangleTexture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(TexturedPixelError::TriangleTexture);
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let variant = entry
        .variants
        .get(selector)
        .ok_or(TexturedPixelError::Selector(selector))?;
    // 004D01C1 reads the Far tail only after the selector gate. Its three
    // fog bytes are consumed independently; padding +4B..4F is not read.
    let shading = if far {
        let color = read(0x44, "far color")?;
        let mut fog = [0; 3];
        for (index, value) in fog.iter_mut().enumerate() {
            let offset = 0x48 + index;
            *value = command.payload[offset].ok_or(TexturedPixelError::Unknown {
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
    let mut shades = [[0; 3]; 3];
    let mut uv = [[0; 2]; 3];
    for i in 0..3 {
        let xy = read(i * 8, "projected XY")?;
        vertices[i] = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
        for channel in 0..3 {
            let offset = 0x20 + i * 4 + channel;
            shades[i][channel] = command.payload[offset].ok_or(TexturedPixelError::Unknown {
                field: "corner shade byte",
                offset,
            })?;
        }
        uv[i] = [
            read(0x2c + i * 8, "signed U coordinate")? as i32,
            read(0x30 + i * 8, "signed V coordinate")? as i32,
        ];
    }
    Ok(Some(TexturedTriangle {
        vertices,
        shades,
        uv,
        mode,
        resource,
        flags: variant.flags.raw(),
        shading,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedTriangleShading {
    Near,
    Far { color: u32, fog: [u8; 3] },
}

#[derive(Clone, Copy, Debug)]
pub struct SharedShadeTriangle<'a> {
    pub vertices: [FlatVertex; 3],
    pub shared_rgb: [u8; 3],
    pub mode: u32,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    pub shading: TexturedTriangleShading,
}

/// DE's 004A38C0/004A3900 records both occupy three blocks. Far adds the
/// converted color at+24 and three fog bytes at+28..2A.004C3B60/004C5210
/// suppress selectorFF before reading any geometry, RGB or Far tail.
pub fn read_de_triangle<'a>(
    command: &PrimitiveCommand<'a>,
) -> Result<Option<SharedShadeTriangle<'a>>, TexturedPixelError> {
    let far = match command.callback {
        PrimitiveCallback::SharedShadeTriangleNear => false,
        PrimitiveCallback::SharedShadeTriangleFar => true,
        _ => return Err(TexturedPixelError::DePacket),
    };
    if command.payload_blocks != 3 || command.payload.len() != 48 {
        return Err(TexturedPixelError::DePacket);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(TexturedPixelError::Unknown { field, offset })
    };
    let mode = read(0x18, "mode")?;
    let Some((0x1c, resource)) = command.texture else {
        return Err(TexturedPixelError::DeTexture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(TexturedPixelError::DeTexture);
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let variant = entry
        .variants
        .get(selector)
        .ok_or(TexturedPixelError::Selector(selector))?;
    let shading = if far {
        let color = read(0x24, "far color")?;
        let mut fog = [0; 3];
        for (index, value) in fog.iter_mut().enumerate() {
            let offset = 0x28 + index;
            *value = command.payload[offset].ok_or(TexturedPixelError::Unknown {
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
    for (index, vertex) in vertices.iter_mut().enumerate() {
        let xy = read(index * 8, "projected XY")?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(index * 8 + 4, "signed depth")? as i32,
        };
    }
    let mut shared_rgb = [0; 3];
    for (channel, value) in shared_rgb.iter_mut().enumerate() {
        let offset = 0x20 + channel;
        *value = command.payload[offset].ok_or(TexturedPixelError::Unknown {
            field: "shared shade RGB",
            offset,
        })?;
    }
    Ok(Some(SharedShadeTriangle {
        vertices,
        shared_rgb,
        mode,
        resource,
        flags: variant.flags.raw(),
        shading,
    }))
}

/// Primary DE passes before native viewport clipping. Allocation corners are
/// TL/TR/BR in both consumers (004C3DC7/3F1F/4074 and004C59B6/5B25/5C79).
/// Far flags10/20 fade diffuse RGB directly; ordinary/alpha paths instead
/// enable vertex fog and place255-fog in specular alpha. Secondary0200 is
/// separately required and is never replaced by the primary texture.
pub fn prepare_de_triangle(
    triangle: &SharedShadeTriangle<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> Result<D3dTexturedSubmission<3, 1>, TexturedPixelError> {
    let flags = triangle.flags;
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
    let fog = match triangle.shading {
        TexturedTriangleShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
            color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
        },
        _ => D3dTextureFog::Disabled,
    };
    let [u0, v0, u1, v1] = uv_bounds;
    let uv = [[u0, v0], [u1, v0], [u1, v1]];
    Ok(D3dTexturedSubmission {
        vertices: std::array::from_fn(|index| {
            let vertex = triangle.vertices[index];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let (rgb, specular) = match triangle.shading {
                TexturedTriangleShading::Far { fog, .. } if flags & 0x30 != 0 => (
                    scaled_shade(
                        triangle.shared_rgb,
                        f64::from(256 - u16::from(fog[index]) - u16::from(fog[index] >> 7)) / 128.0,
                    ),
                    0xff00_0000,
                ),
                TexturedTriangleShading::Far { fog, .. } => (
                    normalized_shade(triangle.shared_rgb),
                    u32::from(255 - fog[index]) << 24,
                ),
                TexturedTriangleShading::Near => {
                    (normalized_shade(triangle.shared_rgb), 0xff00_0000)
                }
            };
            D3dTlVertex {
                x: vertex.x,
                y: vertex.y,
                depth: (1.0 - f64::from(renderer.near_depth) * f64::from(rhw)) as f32,
                rhw,
                diffuse: [rgb[0], rgb[1], rgb[2], alpha],
                specular,
                uv: uv[index],
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
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedTriangleContinuation {
    Complete,
    SecondaryPassRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexturedTriangleSubmission {
    pub primary: D3dTexturedSubmission<3, 1>,
    pub continuation: TexturedTriangleContinuation,
}

/// 004CE98E..004CF3F3 / 004D01C1..004D0D61 state and TL conversion under
/// startup's x87 027F. Far flags10/20 fade each diffuse RGB directly;
/// other Far paths use normalized RGB and 255-fog in specular alpha.
/// The ordinary path clips the complete triangle using 004BCCE0 unless mode20
/// is set, then submits one triangle fan (004D00D3..004D010C). The submission
/// retains full mode and near depth for that explicit viewport stage.
/// `uv_bounds` is the selected native allocation's +1C/+20/+24/+28, not an
/// intrinsic H3 extent. No UV clamping occurs during conversion.
pub fn prepare_textured_triangle(
    triangle: &TexturedTriangle<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> TexturedTriangleSubmission {
    let flags = triangle.flags;
    // Native primary fan 004CF460/004D0DD0 (with HRESULT checks/retry)
    // precedes secondary descriptor read/setup 004CF4BA/004D0E2A.
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
                uv: std::array::from_fn(|axis| {
                    let start = f64::from(uv_bounds[axis]);
                    let range = f64::from(uv_bounds[axis + 2]) - start;
                    (f64::from(triangle.uv[i][axis]) / 65536.0 * range + start) as f32
                }),
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
            fog: match triangle.shading {
                TexturedTriangleShading::Far { color, .. } if flags & 0x30 == 0 => {
                    D3dTextureFog::Vertex {
                        color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
                    }
                }
                _ => D3dTextureFog::Disabled,
            },
        },
        mode: triangle.mode,
        near_depth: renderer.near_depth,
    };
    TexturedTriangleSubmission {
        primary,
        continuation: if flags & 0x200 != 0 && secondary_texture {
            TexturedTriangleContinuation::SecondaryPassRequired
        } else {
            TexturedTriangleContinuation::Complete
        },
    }
}

pub(crate) fn normalized_shade(shade: [u8; 3]) -> [u8; 3] {
    // 005291CC is exactly 255/128. Red remains in an x87 register while G/B
    // spill to float. Above 255, scale the complete RGB by 255/max before
    // FISTP nearest-even. Independent clamping changes the native hue.
    scaled_shade(shade, 255.0 / 128.0)
}

pub(crate) fn scaled_shade(shade: [u8; 3], scale: f64) -> [u8; 3] {
    let mut rgb = shade.map(|channel| f64::from(channel) * scale);
    let largest = rgb[0].max(rgb[1]).max(rgb[2]);
    if largest > 255.0 {
        let scale = 255.0 / largest;
        rgb[0] *= scale;
        rgb[1] = f64::from((rgb[1] * scale) as f32);
        rgb[2] = f64::from((rgb[2] * scale) as f32);
    }
    rgb.map(|channel| channel.round_ties_even() as u8)
}

#[cfg(test)]
#[path = "d3d_textured_triangle_tests.rs"]
mod tests;
