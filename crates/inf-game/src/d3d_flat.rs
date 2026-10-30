//! Near flat-triangle preparation for the retail Direct3D renderer.
//!
//! Renderer vtable `005290E8` slot `+4C` is `004C12F0`. This is deliberately
//! separate from the software renderer (`00528FF0` / `004B0190`), whose
//! fixed-point scanline attributes and depth ownership differ. The output is
//! the three TL vertices before optional viewport clipping at `004BCCE0`.

use crate::d3d_viewport::D3dTlVertex;
use crate::model_primitives::{PrimitiveCallback, PrimitiveCommand};
use crate::software_frame::{FlatFog, FlatTriangle, FlatVertex, SoftwarePixelError};
use inf_formats::MaterialValue;

/// Live inputs read by `004C12F0`; none are inferred from a material ID or
/// replaced by a default framebuffer format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D3dFlatState {
    /// Display `+24/+28/+2C`. The first two are incremented before a DWORD
    /// right shift; the third is decremented before a low-byte left shift.
    pub material_shifts: [u32; 3],
    /// Unsigned renderer `+38`, multiplied by the rounded reciprocal depth.
    pub near_depth: u32,
    /// Display byte `+0E`: whether this callback updates depth state.
    pub manages_depth: bool,
    /// Renderer `+3C`, forwarded to D3D ZENABLE on the non-additive path.
    pub depth_enable: u32,
    /// Renderer `+40`, forwarded to D3D ZWRITEENABLE on that path.
    pub depth_write: u32,
}

/// The callback always sets source factor ONE. It does not change whether
/// alpha blending is enabled, so the caller must retain that live state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D3dFlatBlend {
    /// Destination factor ZERO.
    Copy,
    /// Destination factor ONE (`mode & 2`).
    Additive,
}

/// State writes made by this callback. `None` preserves the previous value;
/// in particular, additive drawing never writes ZENABLE. ZFUNC, alpha-blend
/// enable, shade mode, and other device state remain owned by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D3dFlatPolicy {
    pub depth_enable: Option<u32>,
    pub depth_write: Option<u32>,
    pub blend: D3dFlatBlend,
    pub fog_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D3dFlatClip {
    /// Invoke `004BCCE0` with the complete mode word before submission.
    NativeViewport,
    /// `mode & 0x20` skips that helper.
    Bypass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct D3dFlatSubmission<const V: usize = 3, const T: usize = 1> {
    pub vertices: [D3dTlVertex; V],
    pub triangles: [[usize; 3]; T],
    pub policy: D3dFlatPolicy,
    pub clip: D3dFlatClip,
    /// Preserved for the optional clipping consumer.
    pub mode: u32,
}

/// Execute the arithmetic and state selection before clipping/submission in
/// `004C12F0`. Signed packet depth is converted directly by native FILD;
/// zero depth therefore preserves IEEE infinity/NaN rather than becoming an
/// invented near-plane vertex. A pixel target may reject non-finite output.
pub fn prepare_flat_triangle(triangle: &FlatTriangle, state: D3dFlatState) -> D3dFlatSubmission {
    let additive = triangle.mode & 2 != 0;
    let material = triangle.material.raw();
    // 004C13B9..004C144C: these are display-dependent shifts, not a generic
    // RGB555 expansion. The blue instruction shifts AL, not the full DWORD.
    let base = [
        ((material >> (state.material_shifts[0].wrapping_add(1) & 31)) & 0xf8) as u8,
        ((material >> (state.material_shifts[1].wrapping_add(1) & 31)) & 0xf8) as u8,
        (u32::from(material as u8) << (state.material_shifts[2].wrapping_sub(1) & 31) & 0xf8) as u8,
    ];
    D3dFlatSubmission {
        triangles: [[0, 1, 2]],
        vertices: std::array::from_fn(|index| {
            let vertex = triangle.vertices[index];
            // 004C14D6..004C151A: reciprocal is stored as float first; the
            // following unsigned near-depth multiply consumes that value.
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let depth = (1.0 - f64::from(state.near_depth) * f64::from(rhw)) as f32;
            let rgb = modulated_rgb(base, triangle.rgb[index]);
            D3dTlVertex {
                x: vertex.x,
                y: vertex.y,
                depth,
                rhw,
                diffuse: [rgb[0], rgb[1], rgb[2], if additive { 0 } else { 255 }],
                specular: 0xff00_0000,
                uv: [0.0; 2],
            }
        }),
        policy: D3dFlatPolicy {
            depth_enable: (state.manages_depth && !additive).then_some(state.depth_enable),
            depth_write: state.manages_depth.then_some(if additive {
                0
            } else {
                state.depth_write
            }),
            blend: if additive {
                D3dFlatBlend::Additive
            } else {
                D3dFlatBlend::Copy
            },
            fog_enabled: false,
        },
        clip: if triangle.mode & 0x20 != 0 {
            D3dFlatClip::Bypass
        } else {
            D3dFlatClip::NativeViewport
        },
        mode: triangle.mode,
    }
}

/// Far flat-triangle `004C1940`: the near arithmetic, then each corner's
/// unrounded channels blend toward the display-shifted far color by its fog
/// byte, `((256 - fog) * channel + fog * far) / 256`, before nearest-even
/// FISTP. Device fog stays disabled; specular alpha carries `255 - fog`.
pub fn prepare_flat_triangle_far(
    triangle: &FlatTriangle,
    far: FlatFog,
    state: D3dFlatState,
) -> D3dFlatSubmission {
    let mut submission = prepare_flat_triangle(triangle, state);
    let base = display_channels(triangle.material.raw(), state.material_shifts);
    let far_color = display_channels(far.color, state.material_shifts);
    for (index, vertex) in submission.vertices.iter_mut().enumerate() {
        let channels = modulated_channels(base, triangle.rgb[index]);
        let rgb = fogged_rgb(channels, far.fog[index], far_color);
        vertex.diffuse = [rgb[0], rgb[1], rgb[2], vertex.diffuse[3]];
        vertex.specular = (255 - u32::from(far.fog[index])) << 24;
    }
    submission
}

/// One far corner: its unrounded channels blend toward the display-shifted
/// far color by its fog byte before nearest-even FISTP.
fn fogged_rgb(channels: [f64; 3], fog: u8, far_color: [u8; 3]) -> [u8; 3] {
    let keep = f64::from(256 - u32::from(fog));
    let fog = f64::from(fog);
    std::array::from_fn(|channel| {
        ((keep * channels[channel] + fog * f64::from(far_color[channel])) / 256.0).round_ties_even()
            as u8
    })
}

/// 004C13B9..004C144C shifts, shared by material and far color DWORDs: red
/// and green shift the DWORD right, blue shifts the low byte left.
fn display_channels(word: u32, shifts: [u32; 3]) -> [u8; 3] {
    [
        ((word >> (shifts[0].wrapping_add(1) & 31)) & 0xf8) as u8,
        ((word >> (shifts[1].wrapping_add(1) & 31)) & 0xf8) as u8,
        (u32::from(word as u8) << (shifts[2].wrapping_sub(1) & 31) & 0xf8) as u8,
    ]
}

fn modulated_rgb(base: [u8; 3], shade: [u8; 3]) -> [u8; 3] {
    modulated_channels(base, shade).map(|channel| channel.round_ties_even() as u8)
}

fn modulated_channels(base: [u8; 3], shade: [u8; 3]) -> [f64; 3] {
    // Products /128 are exact binary fractions. Red remains in the x87
    // register while green/blue are spilled to f32. If the brightest channel
    // exceeds 255, all three scale together before nearest-even FISTP.
    let mut rgb: [f64; 3] =
        std::array::from_fn(|i| f64::from(base[i]) * f64::from(shade[i]) / 128.0);
    let largest = rgb[0].max(rgb[1]).max(rgb[2]);
    if largest > 255.0 {
        let scale = 255.0 / largest;
        rgb[0] *= scale;
        rgb[1] = f64::from((rgb[1] * scale) as f32);
        rgb[2] = f64::from((rgb[2] * scale) as f32);
    }
    rgb
}

#[cfg(test)]
#[path = "d3d_flat_tests.rs"]
mod tests;

/// One decoded shared-shade flat quad (`0x4A39A0` near, `0x4A39D0` far)
/// as the E4 producer emits it: four corner XY/depth pairs, mode, the
/// Handler-7 material DWORD, one shared shade DWORD. Far appends the
/// far color and four fog bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatQuad {
    pub vertices: [FlatVertex; 4],
    pub mode: u32,
    pub material: MaterialValue,
    pub rgb: [[u8; 3]; 4],
    /// Far color `+2C` and the corner fog bytes `+30..+33`.
    pub fog: Option<FlatFog<4>>,
}

/// Read the exact E4 record layout: corners, mode, material word, then
/// one shared RGB triple replicated to all corners; Far adds the far color
/// and four fog bytes. Anything else fails closed.
pub fn read_shared_shade_quad(
    command: &PrimitiveCommand<'_>,
) -> Result<FlatQuad, SoftwarePixelError> {
    use PrimitiveCallback::{SharedShadeQuadFar, SharedShadeQuadNear};
    let far = match command.callback {
        SharedShadeQuadNear => false,
        SharedShadeQuadFar => true,
        callback => {
            return Err(SoftwarePixelError::Unsupported {
                callback,
                expected: SharedShadeQuadNear,
            });
        }
    };
    let blocks = if far { 4 } else { 3 };
    if command.payload_blocks != blocks {
        return Err(SoftwarePixelError::Blocks {
            expected: blocks,
            actual: command.payload_blocks,
        });
    }
    if command.payload.len() != blocks * 16 {
        return Err(SoftwarePixelError::PayloadLength {
            expected: blocks * 16,
            actual: command.payload.len(),
        });
    }
    let unknown =
        |field: &'static str, offset: usize| SoftwarePixelError::Unknown { field, offset };
    let mut vertices = [FlatVertex {
        x: 0.0,
        y: 0.0,
        depth: 0,
    }; 4];
    for (index, vertex) in vertices.iter_mut().enumerate() {
        let xy = command
            .read_u32(index * 8)
            .ok_or_else(|| unknown("projected XY", index * 8))?;
        let depth = command
            .read_u32(index * 8 + 4)
            .ok_or_else(|| unknown("projected depth", index * 8 + 4))?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: depth as i32,
        };
    }
    let mode = command
        .read_u32(0x20)
        .ok_or_else(|| unknown("mode word", 0x20))?;
    let material = command
        .read_u32(0x24)
        .ok_or_else(|| unknown("material word", 0x24))?;
    let mut rgb = [0u8; 3];
    for (channel, slot) in rgb.iter_mut().enumerate() {
        *slot = command.payload[0x28 + channel]
            .ok_or_else(|| unknown("shared shade RGB", 0x28 + channel))?;
    }
    let fog = if far {
        let color = command
            .read_u32(0x2c)
            .ok_or_else(|| unknown("far color", 0x2c))?;
        let mut fog = [0u8; 4];
        for (index, slot) in fog.iter_mut().enumerate() {
            *slot =
                command.payload[0x30 + index].ok_or_else(|| unknown("fog byte", 0x30 + index))?;
        }
        Some(FlatFog { color, fog })
    } else {
        None
    };
    Ok(FlatQuad {
        vertices,
        mode,
        material: MaterialValue::from_raw(material),
        rgb: [rgb; 4],
        fog,
    })
}

/// `004D2950` (near) / `004D3260` (far): the quad form of
/// `prepare_flat_triangle`, with the same display-shift base color and
/// `modulated_rgb` kernel, uniform across all four corners because the E4
/// shade is shared. Far blends each corner toward the far color like
/// `004C1940`. Both draw two three-vertex fans from corner `k`, where mode
/// bit 14 clear selects `k = 1`.
pub fn prepare_flat_quad(quad: &FlatQuad, state: D3dFlatState) -> D3dFlatSubmission<4, 2> {
    let additive = quad.mode & 2 != 0;
    let material = quad.material.raw();
    let base = [
        ((material >> (state.material_shifts[0].wrapping_add(1) & 31)) & 0xf8) as u8,
        ((material >> (state.material_shifts[1].wrapping_add(1) & 31)) & 0xf8) as u8,
        (u32::from(material as u8) << (state.material_shifts[2].wrapping_sub(1) & 31) & 0xf8) as u8,
    ];
    let k = ((!quad.mode >> 14) & 1) as usize;
    D3dFlatSubmission {
        vertices: std::array::from_fn(|corner| {
            let vertex = quad.vertices[corner];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let depth = (1.0 - f64::from(state.near_depth) * f64::from(rhw)) as f32;
            let (rgb, specular) = match quad.fog {
                Some(far) => (
                    fogged_rgb(
                        modulated_channels(base, quad.rgb[corner]),
                        far.fog[corner],
                        display_channels(far.color, state.material_shifts),
                    ),
                    (255 - u32::from(far.fog[corner])) << 24,
                ),
                None => (modulated_rgb(base, quad.rgb[corner]), 0xff00_0000),
            };
            D3dTlVertex {
                x: vertex.x,
                y: vertex.y,
                depth,
                rhw,
                diffuse: [rgb[0], rgb[1], rgb[2], if additive { 0 } else { 255 }],
                specular,
                uv: [0.0; 2],
            }
        }),
        triangles: [[k, (k + 1) & 3, (k + 2) & 3], [k, (k + 2) & 3, (k + 3) & 3]],
        policy: D3dFlatPolicy {
            depth_enable: (state.manages_depth && !additive).then_some(state.depth_enable),
            depth_write: state.manages_depth.then_some(if additive {
                0
            } else {
                state.depth_write
            }),
            blend: if additive {
                D3dFlatBlend::Additive
            } else {
                D3dFlatBlend::Copy
            },
            fog_enabled: false,
        },
        clip: if quad.mode & 0x20 != 0 {
            D3dFlatClip::Bypass
        } else {
            D3dFlatClip::NativeViewport
        },
        mode: quad.mode,
    }
}
