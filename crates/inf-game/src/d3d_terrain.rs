//! Colored terrain quads 0044EA80/90 -> renderer+94/+98 ->004D3EB0/004D47C0.
//! Per-corner RGB modulates the converted material; Far blends that result
//! with its own material before rounding. Shade high bytes are not colors.

use crate::{
    d3d_flat::{D3dFlatBlend, D3dFlatClip, D3dFlatPolicy, D3dFlatState, D3dFlatSubmission},
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    software_frame::{FlatVertex, SoftwarePixelError},
};

#[derive(Clone, Copy, Debug)]
pub struct ColoredTerrainQuad {
    pub vertices: [FlatVertex; 4],
    pub mode: u32,
    pub material: u32,
    pub shades: [[u8; 3]; 4],
    pub far: Option<(u32, [u8; 4])>,
}

pub fn read_colored_terrain(
    command: &PrimitiveCommand<'_>,
) -> Result<ColoredTerrainQuad, SoftwarePixelError> {
    let far = match command.callback {
        PrimitiveCallback::ColoredQuadNear => false,
        PrimitiveCallback::ColoredQuadFar => true,
        callback => {
            return Err(SoftwarePixelError::Unsupported {
                callback,
                expected: PrimitiveCallback::ColoredQuadNear,
            })
        }
    };
    if command.payload_blocks != 4 {
        return Err(SoftwarePixelError::Blocks {
            expected: 4,
            actual: command.payload_blocks,
        });
    }
    if command.payload.len() != 64 {
        return Err(SoftwarePixelError::PayloadLength {
            expected: 64,
            actual: command.payload.len(),
        });
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(SoftwarePixelError::Unknown { field, offset })
    };
    let byte = |offset: usize, field| {
        command.payload[offset].ok_or(SoftwarePixelError::Unknown { field, offset })
    };
    let mut vertices = [FlatVertex {
        x: 0.,
        y: 0.,
        depth: 0,
    }; 4];
    let mut shades = [[0; 3]; 4];
    for index in 0..4 {
        let xy = read(index * 8, "projected XY")?;
        vertices[index] = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(index * 8 + 4, "signed depth")? as i32,
        };
        for channel in 0..3 {
            shades[index][channel] = byte(0x28 + index * 4 + channel, "shade RGB")?;
        }
    }
    Ok(ColoredTerrainQuad {
        vertices,
        shades,
        mode: read(0x20, "mode")?,
        material: read(0x24, "material")?,
        far: if far {
            Some((
                read(0x38, "far material")?,
                [
                    byte(0x3c, "fog")?,
                    byte(0x3d, "fog")?,
                    byte(0x3e, "fog")?,
                    byte(0x3f, "fog")?,
                ],
            ))
        } else {
            None
        },
    })
}

fn material_rgb(word: u32, shifts: [u32; 3]) -> [u8; 3] {
    [
        ((word >> (shifts[0].wrapping_add(1) & 31)) & 0xf8) as u8,
        ((word >> (shifts[1].wrapping_add(1) & 31)) & 0xf8) as u8,
        ((u32::from(word as u8) << (shifts[2].wrapping_sub(1) & 31)) & 0xf8) as u8,
    ]
}

/// The colour a far ring's fogged quad mixes into its corners' colours
/// (as [`prepare_colored_terrain`] does), none for an unfogged one: Vulkan's
/// raster takes it back out to wet the ground under it.
pub fn colored_terrain_fog(quad: &ColoredTerrainQuad, state: D3dFlatState) -> Option<[u8; 3]> {
    quad.far
        .map(|(word, _)| material_rgb(word, state.material_shifts))
}

pub fn prepare_colored_terrain(
    quad: &ColoredTerrainQuad,
    state: D3dFlatState,
) -> D3dFlatSubmission<4, 2> {
    let additive = quad.mode & 2 != 0;
    let base = material_rgb(quad.material, state.material_shifts);
    let k = ((!quad.mode >> 14) & 1) as usize;
    D3dFlatSubmission {
        vertices: std::array::from_fn(|index| {
            let mut rgb = std::array::from_fn::<_, 3, _>(|channel| {
                f64::from(base[channel]) * f64::from(quad.shades[index][channel]) / 128.0
            });
            let largest = rgb[0].max(rgb[1]).max(rgb[2]);
            if largest > 255.0 {
                let scale = 255.0 / largest;
                rgb[0] *= scale;
                rgb[1] = f64::from((rgb[1] * scale) as f32);
                rgb[2] = f64::from((rgb[2] * scale) as f32);
            }
            let specular = if let Some((word, fog)) = quad.far {
                // 004D4996..004D4AD4: unlike textured Far, FOGENABLE stays0.
                // It blends normalized but unrounded RGB with unreplicated
                // material channels using exactly fog/256 and(256-fog)/256.
                let far = material_rgb(word, state.material_shifts);
                for channel in 0..3 {
                    rgb[channel] = (f64::from(far[channel]) * f64::from(fog[index])
                        + rgb[channel] * f64::from(256 - u16::from(fog[index])))
                        / 256.0;
                }
                u32::from(255 - fog[index]) << 24
            } else {
                0xff00_0000
            };
            let [r, g, b] = rgb.map(|channel| channel.round_ties_even() as u8);
            let vertex = quad.vertices[index];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            D3dTlVertex {
                x: vertex.x,
                y: vertex.y,
                depth: (1.0 - f64::from(state.near_depth) * f64::from(rhw)) as f32,
                rhw,
                diffuse: [r, g, b, if additive { 0 } else { 255 }],
                specular,
                uv: [0.; 2],
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

#[cfg(test)]
#[path = "d3d_terrain_tests.rs"]
mod tests;
