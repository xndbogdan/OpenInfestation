//! Solid quads 00466170/180 -> Direct3D+84/+88 ->004D1BB0/004D2170.
//! Frontend stars use this exact packet with signed depth0x7FFFFFFF.
//! D8's solid triangles (`004A38E0`/`004A3920` -> `+3C`/`+40` ->
//! `004BFAB0`/`004BFE80`) are the same callbacks over three corners: the
//! packet's mode, material and far tail start eight bytes earlier.

use crate::{
    d3d_flat::{D3dFlatBlend, D3dFlatClip, D3dFlatPolicy, D3dFlatState, D3dFlatSubmission},
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    software_frame::{FlatVertex, SoftwarePixelError},
};

/// A solid packet's corners and color, quad or triangle.
#[derive(Clone, Copy, Debug)]
pub struct SolidPolygon<const N: usize> {
    pub vertices: [FlatVertex; N],
    pub mode: u32,
    /// Already converted H7 DWORD, distinct from an intrinsic RGB555 value.
    pub color: SolidColor<N>,
}

#[derive(Clone, Copy, Debug)]
pub enum SolidColor<const N: usize> {
    Material(u32),
    Fog {
        material: u32,
        far_color: u32,
        amount: [u8; N],
    },
}

pub type SolidQuad = SolidPolygon<4>;
pub type SolidQuadColor = SolidColor<4>;
pub type SolidTriangle = SolidPolygon<3>;

pub fn read_solid_quad(command: &PrimitiveCommand<'_>) -> Result<SolidQuad, SoftwarePixelError> {
    read_solid(
        command,
        [
            PrimitiveCallback::FlatQuadNear,
            PrimitiveCallback::FlatQuadFar,
        ],
        [3, 3],
    )
}

/// D8's packet: two blocks near, three far.
pub fn read_solid_triangle(
    command: &PrimitiveCommand<'_>,
) -> Result<SolidTriangle, SoftwarePixelError> {
    read_solid(
        command,
        [
            PrimitiveCallback::SolidTriangleNear,
            PrimitiveCallback::SolidTriangleFar,
        ],
        [2, 3],
    )
}

fn read_solid<const N: usize>(
    command: &PrimitiveCommand<'_>,
    [near, far]: [PrimitiveCallback; 2],
    blocks: [usize; 2],
) -> Result<SolidPolygon<N>, SoftwarePixelError> {
    let is_far = match command.callback {
        callback if callback == near => false,
        callback if callback == far => true,
        callback => {
            return Err(SoftwarePixelError::Unsupported {
                callback,
                expected: near,
            })
        }
    };
    let blocks = blocks[usize::from(is_far)];
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
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(SoftwarePixelError::Unknown { field, offset })
    };
    let mut vertices = [FlatVertex {
        x: 0.,
        y: 0.,
        depth: 0,
    }; N];
    for (i, v) in vertices.iter_mut().enumerate() {
        let xy = read(i * 8, "projected XY")?;
        *v = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
    }
    let mode = N * 8;
    Ok(SolidPolygon {
        vertices,
        mode: read(mode, "mode")?,
        color: if is_far {
            let mut amount = [0; N];
            for (corner, fog) in amount.iter_mut().enumerate() {
                let offset = mode + 12 + corner;
                *fog = command.payload[offset].ok_or(SoftwarePixelError::Unknown {
                    field: "fog bytes",
                    offset,
                })?;
            }
            SolidColor::Fog {
                material: read(mode + 4, "converted color")?,
                far_color: read(mode + 8, "far color")?,
                amount,
            }
        } else {
            SolidColor::Material(read(mode + 4, "converted color")?)
        },
    })
}

pub fn prepare_solid_quad(quad: &SolidQuad, state: D3dFlatState) -> D3dFlatSubmission<4, 2> {
    let k = ((!quad.mode >> 14) & 1) as usize;
    D3dFlatSubmission {
        vertices: solid_vertices(quad, state),
        triangles: [[k, (k + 1) & 3, (k + 2) & 3], [k, (k + 2) & 3, (k + 3) & 3]],
        policy: solid_policy(quad.mode, state),
        clip: solid_clip(quad.mode),
        mode: quad.mode,
    }
}

/// `004BFAB0`/`004BFE80`: one three-vertex fan, no quad diagonal choice.
pub fn prepare_solid_triangle(
    triangle: &SolidTriangle,
    state: D3dFlatState,
) -> D3dFlatSubmission<3, 1> {
    D3dFlatSubmission {
        vertices: solid_vertices(triangle, state),
        triangles: [[0, 1, 2]],
        policy: solid_policy(triangle.mode, state),
        clip: solid_clip(triangle.mode),
        mode: triangle.mode,
    }
}

fn solid_vertices<const N: usize>(
    polygon: &SolidPolygon<N>,
    state: D3dFlatState,
) -> [D3dTlVertex; N] {
    // 004D1C79..004D1CE3: unlike the shaded flat triangle this callback
    // restores the low three component bits from each five-bit channel.
    // Every shift is a DWORD x86 shift with its count masked by31.
    let [red, green, blue] = state.material_shifts;
    let additive = polygon.mode & 2 != 0;
    std::array::from_fn(|index| {
        let v = polygon.vertices[index];
        let (packed, specular) = match polygon.color {
            SolidColor::Material(word) => {
                let packed = (word.wrapping_shl(15u32.wrapping_sub(red)) & 0xf80000)
                    | (word.wrapping_shl(7u32.wrapping_sub(green)) & 0xf800)
                    | (word.wrapping_shl(blue.wrapping_sub(1)) & 0xf8);
                (packed | ((packed >> 5) & 0x070707), 0xff000000)
            }
            SolidColor::Fog {
                material,
                far_color,
                amount,
            } => {
                //004D223D..004D238A: fog expands0..255 to0..256 before
                //mixing five-bit channels. Unlike the near callback, no
                //low-three-bit replication follows this CPU fog blend.
                let fog = u32::from(amount[index]);
                let weight = fog + (fog >> 7);
                let blend = |shift: u32| {
                    (material.wrapping_shl(shift) & 0xf800)
                        .wrapping_mul(256 - weight)
                        .wrapping_add((far_color.wrapping_shl(shift) & 0xf800).wrapping_mul(weight))
                };
                let rgb = (blend(7u32.wrapping_sub(red)) & 0xff0000)
                    | ((blend(7u32.wrapping_sub(green)) >> 8) & 0xff00)
                    | (blend(blue.wrapping_add(7)) >> 16);
                (rgb, (255 - fog) << 24)
            }
        };
        let rhw = (1.0 / f64::from(v.depth)) as f32;
        D3dTlVertex {
            x: v.x,
            y: v.y,
            depth: (1.0 - f64::from(state.near_depth) * f64::from(rhw)) as f32,
            rhw,
            diffuse: [
                (packed >> 16) as u8,
                (packed >> 8) as u8,
                packed as u8,
                if additive { 0 } else { 255 },
            ],
            specular,
            uv: [0.; 2],
        }
    })
}

fn solid_policy(mode: u32, state: D3dFlatState) -> D3dFlatPolicy {
    let additive = mode & 2 != 0;
    D3dFlatPolicy {
        depth_enable: (state.manages_depth && !additive).then_some(state.depth_enable),
        depth_write: state
            .manages_depth
            .then_some(if additive { 0 } else { state.depth_write }),
        blend: if additive {
            D3dFlatBlend::Additive
        } else {
            D3dFlatBlend::Copy
        },
        fog_enabled: false,
    }
}

fn solid_clip(mode: u32) -> D3dFlatClip {
    if mode & 0x20 != 0 {
        D3dFlatClip::Bypass
    } else {
        D3dFlatClip::NativeViewport
    }
}

#[cfg(test)]
#[path = "d3d_solid_quad_tests.rs"]
mod tests;
