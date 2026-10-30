//! Direct3D effect quads: near 0040EF30 / +A4 = 004D78A0,
//! far 00411D50 / +A8 = 004D9840 (vtable 005290E8).
//!
//! This consumer reads the shared effect RGB at packet+28, unlike ordinary
//! textured quads. Its source colors, state writes and TL arithmetic are
//! recovered independently; only the proven output submission shape is shared.

use inf_formats::SpriteRenderMode;
use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    d3d_texture::CpuTexture,
    d3d_textured::{
        sprite_mode, D3dTextureBlend, D3dTextureFog, D3dTexturePolicy, D3dTexturedSubmission,
    },
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

/// The word an unknown-mode quad draws under once [`settle_effect_mode`]
/// shows every word draws it alike: bit 20 skips the clipper, bit 100 takes
/// affine coordinates and bit 4000 the 0-2 diagonal.
const MODE_FREE: u32 = 0x4120;

#[derive(Clone, Copy, Debug)]
pub enum EffectQuadShading {
    Near,
    Far { color: u32, fog: [u8; 4] },
}

#[derive(Clone, Copy, Debug)]
pub struct EffectQuad<'a> {
    pub vertices: [FlatVertex; 4],
    /// Packet+20, or `None` when its producer left it to the allocation's
    /// carry (the world flash, 00488268..00488305) and that is unknown.
    pub mode: Option<u32>,
    /// The sprite mode that chose the descriptor; for an unknown `mode`,
    /// both modes select it.
    pub sprite_mode: SpriteRenderMode,
    pub resource: RenderResource<'a>,
    pub flags: u16,
    /// Packet+28/+29/+2A in R/G/B order. Byte+2B is not consumed.
    pub effect_rgb: [u8; 3],
    pub shading: EffectQuadShading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectQuadContinuation {
    Complete,
    /// Native commits the primary fans before starting the unowned secondary.
    SecondaryPassRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectQuadSubmission {
    pub primary: D3dTexturedSubmission<4, 2>,
    pub continuation: EffectQuadContinuation,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EffectPixelError {
    #[error("effect quad requires near 0040EF30/three blocks or far 00411D50/four blocks")]
    Packet,
    #[error("effect quad is missing {field} at payload offset {offset:#x}")]
    Unknown { field: &'static str, offset: usize },
    #[error("effect quad requires a Handler-3 relocation at payload+24")]
    Texture,
    #[error("effect sprite selector {0} does not identify a physical descriptor")]
    Selector(usize),
    #[error("effect primary pass completed; descriptor0200 plus renderer+44 requires the secondary texture pass")]
    SecondaryPass,
}

/// 004D78AD..004D78C3 / 004D984D..004D9866 suppress selector FF before
/// coordinates, effect RGB, far color, fog, upload or state writes.
pub fn read_effect_quad<'a>(
    command: &PrimitiveCommand<'a>,
) -> Result<Option<EffectQuad<'a>>, EffectPixelError> {
    let far = match command.callback {
        PrimitiveCallback::EffectQuadNear => false,
        PrimitiveCallback::EffectQuadFar => true,
        _ => return Err(EffectPixelError::Packet),
    };
    // The selector67 ribbon's 00486B30 allocates four blocks for its near
    // quads; 004D78A0/004B75F0 read only the first three.
    let blocks = command.payload_blocks;
    let admitted = if far {
        blocks == 4
    } else {
        blocks == 3 || blocks == 4
    };
    if !admitted || command.payload.len() != blocks * 16 {
        return Err(EffectPixelError::Packet);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(EffectPixelError::Unknown { field, offset })
    };
    let mode = command.read_u32(0x20);
    let Some((0x24, resource)) = command.texture else {
        return Err(EffectPixelError::Texture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(EffectPixelError::Texture);
    };
    // 004D78AD: bit 0 picks the selector byte. An unknown word picks the
    // same descriptor only when both selectors are one.
    let sprite_mode = match mode {
        Some(mode) => sprite_mode(mode),
        None if entry.selectors.mode0 == entry.selectors.mode1 => SpriteRenderMode::Mode0,
        None => {
            return Err(EffectPixelError::Unknown {
                field: "mode",
                offset: 0x20,
            })
        }
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode) else {
        return Ok(None);
    };
    let descriptor = entry
        .variants
        .get(selector)
        .ok_or(EffectPixelError::Selector(selector))?;
    // Far writes FOGCOLOR from +2C before the geometry conversion. All four
    // fog bytes are consumed by either branch; no missing shade is invented.
    let shading = if far {
        let color = read(0x2c, "far color")?;
        let mut fog = [0; 4];
        for (i, value) in fog.iter_mut().enumerate() {
            *value = command.payload[0x30 + i].ok_or(EffectPixelError::Unknown {
                field: "fog byte",
                offset: 0x30 + i,
            })?;
        }
        EffectQuadShading::Far { color, fog }
    } else {
        EffectQuadShading::Near
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
    let mut effect_rgb = [0; 3];
    for (i, channel) in effect_rgb.iter_mut().enumerate() {
        *channel = command.payload[0x28 + i].ok_or(EffectPixelError::Unknown {
            field: "effect RGB",
            offset: 0x28 + i,
        })?;
    }
    Ok(Some(EffectQuad {
        vertices,
        mode,
        sprite_mode,
        resource,
        flags: descriptor.flags.raw(),
        effect_rgb,
        shading,
    }))
}

/// The packet+20 a near quad draws under. A known word is itself. 004D78A0
/// reads an unknown one past the descriptor choice at 004D78FD (bit 100,
/// TEXTUREPERSPECTIVE), 004D8568 (bit 4000, the fans' diagonal; bit 20,
/// skipping 004BCCE0, which takes the whole word but reads it only once its
/// identity path fails) and 004D874E (the descriptor-0200 secondary pass,
/// unowned whatever the word). Every word draws the quad alike when its
/// texture is one color, its corners share one positive depth, it is convex
/// (the two choices then move only texture coordinates and the shared edge
/// of one even fill), and every corner lies in the inclusive clip bounds
/// (004BCCE0 returns such a fan unchanged): the world flash's screen quad.
/// Far quads (004D9840) keep the unknown word.
pub fn settle_effect_mode(
    quad: &EffectQuad<'_>,
    texture: &CpuTexture,
    bounds: [i16; 4],
) -> Result<u32, EffectPixelError> {
    if let Some(mode) = quad.mode {
        return Ok(mode);
    }
    let [left, top, right, bottom] = bounds.map(f32::from);
    let depth = quad.vertices[0].depth;
    let alike = matches!(quad.shading, EffectQuadShading::Near)
        && texture.single_color()
        && depth > 0
        && quad.vertices.iter().all(|vertex| {
            vertex.depth == depth
                && (left..=right).contains(&vertex.x)
                && (top..=bottom).contains(&vertex.y)
        })
        && convex(&quad.vertices);
    if alike {
        Ok(MODE_FREE)
    } else {
        Err(EffectPixelError::Unknown {
            field: "mode",
            offset: 0x20,
        })
    }
}

/// Every corner turns the same way, so either diagonal covers the same area.
fn convex(vertices: &[FlatVertex; 4]) -> bool {
    let turns: [f64; 4] = std::array::from_fn(|i| {
        let [a, b, c] = [0, 1, 2].map(|j| vertices[(i + j) % 4]);
        let [abx, aby, bcx, bcy] = [b.x - a.x, b.y - a.y, c.x - b.x, c.y - b.y].map(f64::from);
        abx * bcy - aby * bcx
    });
    turns.iter().all(|&turn| turn > 0.) || turns.iter().all(|&turn| turn < 0.)
}

/// Primary passes of 004D78A0 / 004D9840 under packet word `mode` (see
/// [`settle_effect_mode`]). Allocation UV belongs to the texture owner.
/// The native consumer uses mode 20 to bypass viewport clipping and mode 4000
/// to choose the diagonal; both are preserved for the shared pixel target.
/// Descriptor0200/+44 selects primary fans at004D8653/004D86EE and
/// 004DA783/004DA81E before secondary setup004D8748/004DA878. The caller
/// must commit the primary before reporting that unowned continuation.
pub fn prepare_effect_quad(
    quad: &EffectQuad<'_>,
    mode: u32,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> EffectQuadSubmission {
    let flags = quad.flags;
    // 004D7972..004D7A24 selects blend factors before deriving vertex color.
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
        EffectQuadShading::Far { color, .. } if flags & 0x30 == 0 => D3dTextureFog::Vertex {
            color: far_rgb(color, renderer.material_shifts),
        },
        _ => D3dTextureFog::Disabled,
    };
    let [u0, v0, u1, v1] = uv_bounds;
    let uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
    let k = ((!mode >> 14) & 1) as usize;
    let primary = D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let vertex = quad.vertices[i];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            // 004D9A65 selects the Far flags10/20 branch: RGB fades by
            // (256-fog-(fog>>7))/128, alpha stays FF, device fog is off.
            // 004DA0C4 uses normalized RGB and specular alpha 255-fog for
            // the ordinary/alpha branches, preserving descriptor alpha.
            let (rgb, alpha, specular) = match quad.shading {
                EffectQuadShading::Far { fog, .. } if flags & 0x30 != 0 => (
                    scaled_effect_rgb(
                        quad.effect_rgb,
                        f64::from(256 - u16::from(fog[i]) - u16::from(fog[i] >> 7)) / 128.0,
                    ),
                    255,
                    0xff00_0000,
                ),
                EffectQuadShading::Far { fog, .. } => (
                    effect_rgb(quad.effect_rgb),
                    alpha,
                    u32::from(255 - fog[i]) << 24,
                ),
                EffectQuadShading::Near => (effect_rgb(quad.effect_rgb), alpha, 0xff00_0000),
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
            perspective: mode & 0x100 == 0,
            fog,
        },
        mode,
        near_depth: renderer.near_depth,
    };
    EffectQuadSubmission {
        primary,
        continuation: if flags & 0x200 != 0 && secondary_texture {
            EffectQuadContinuation::SecondaryPassRequired
        } else {
            EffectQuadContinuation::Complete
        },
    }
}

fn effect_rgb(source: [u8; 3]) -> [u8; 3] {
    // 004D7A5D..004D7B03 and 004D7FE6..004D808C: 005291CC=255/128.
    // Each corner repeats the same conversion. Red remains in the x87
    // register; green and blue spill to f32 after the optional normalization.
    scaled_effect_rgb(source, 255.0 / 128.0)
}

pub(crate) fn scaled_effect_rgb(source: [u8; 3], scale: f64) -> [u8; 3] {
    // Both branches spill green/blue before normalization. The initial
    // byte * integer /128 products are exactly representable as f32.
    let mut rgb = source.map(|channel| f64::from(channel) * scale);
    let largest = rgb[0].max(rgb[1]).max(rgb[2]);
    if largest > 255.0 {
        let scale = 255.0 / largest;
        rgb[0] *= scale;
        rgb[1] = f64::from((rgb[1] * scale) as f32);
        rgb[2] = f64::from((rgb[2] * scale) as f32);
    }
    rgb.map(|channel| channel.round_ties_even() as u8)
}

pub(crate) fn far_rgb(color: u32, shifts: [u32; 3]) -> [u8; 3] {
    // 004D9891..004D98E3 and DE004C5261..004C52B3: same display-dependent
    // DWORD shifts (x86 masks CL),
    // then replicate each channel's high five bits into its low three.
    let packed = (color.wrapping_shl(15u32.wrapping_sub(shifts[0]) & 31) & 0x00f8_0000)
        | (color.wrapping_shl(7u32.wrapping_sub(shifts[1]) & 31) & 0x0000_f800)
        | (color.wrapping_shl(shifts[2].wrapping_sub(1) & 31) & 0x0000_00f8);
    let rgb = packed | ((packed >> 5) & 0x0007_0707);
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

#[cfg(test)]
#[path = "d3d_effect_tests.rs"]
mod tests;
