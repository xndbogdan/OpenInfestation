//! CPU texture sampling for the native Direct3D near ordinary quad.
//! Device subtexel/color precision remains separate from exact CPU submission.

use super::{native_bool, raster, D3dRasterState, SoftwareFrame, SoftwarePixelError};
use crate::{
    d3d_texture::{CpuTexture, NativeTextureUploadState, TEXTURE_SURFACE_SIZE},
    d3d_textured::{D3dTextureBlend, D3dTextureFog, D3dTexturedSubmission, TexturedPixelError},
    d3d_viewport::{clip_finite_into, ClippedPolygon, D3dViewportState},
    frame_device::{DeviceDraw, DeviceTexturing},
};

/// Persistent device state not set by 004D5410. 0050Cxxx sets matching
/// MAG/MIN filters 1 or 2, and texture blend 4 when global policy bit08 is set.
#[derive(Clone, Copy, Debug)]
pub struct D3dTextureSampling {
    pub min_filter: u32,
    pub mag_filter: u32,
    pub texture_blend: u32,
    pub address: u32,
    pub color_key_enabled: bool,
    pub alpha_test_enabled: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SoftwareTextureState {
    pub upload: NativeTextureUploadState,
    pub sampling: D3dTextureSampling,
    /// Whether renderer +44 supplies the extra texture pass for flag0200.
    pub secondary_texture: bool,
}

impl D3dTextureSampling {
    fn validate(self) -> Result<(), TexturedPixelError> {
        for (field, value, supported) in [
            (
                "TEXTUREMIN",
                self.min_filter,
                matches!(self.min_filter, 1 | 2),
            ),
            (
                "TEXTUREMAG",
                self.mag_filter,
                self.mag_filter == self.min_filter,
            ),
            (
                "TEXTUREMAPBLEND",
                self.texture_blend,
                self.texture_blend == 4,
            ),
            (
                "TEXTUREADDRESS",
                self.address,
                matches!(self.address, 1 | 3),
            ),
            (
                "COLORKEYENABLE",
                u32::from(self.color_key_enabled),
                !self.color_key_enabled,
            ),
            (
                "ALPHATESTENABLE",
                u32::from(self.alpha_test_enabled),
                !self.alpha_test_enabled,
            ),
        ] {
            if !supported {
                return Err(TexturedPixelError::State { field, value });
            }
        }
        Ok(())
    }
}

impl SoftwareFrame {
    pub fn fill_textured_submission<const V: usize, const T: usize>(
        &mut self,
        submission: &D3dTexturedSubmission<V, T>,
        texture: &CpuTexture,
        pipeline: &mut D3dRasterState,
        sampling: D3dTextureSampling,
    ) -> Result<usize, SoftwarePixelError> {
        sampling.validate()?;
        if let Some(value) = submission.policy.depth_enable {
            pipeline.depth_enabled = native_bool("ZENABLE", value)?;
        }
        if let Some(value) = submission.policy.depth_write {
            pipeline.depth_write = native_bool("ZWRITEENABLE", value)?;
        }
        for (field, value, supported) in [
            ("ZFUNC", pipeline.depth_function, 4),
            ("SHADEMODE", pipeline.shade_mode, 2),
            ("CULLMODE", pipeline.cull_mode, 1),
        ] {
            if value != supported && (field != "ZFUNC" || pipeline.depth_enabled) {
                return Err(SoftwarePixelError::State { field, value });
            }
        }
        // One pass for the finite and 1/z checks below, kept in their order.
        let (mut finite, mut zero, mut negative, mut positive) = (true, false, false, false);
        for v in &submission.vertices {
            finite &= v.finite();
            zero |= v.rhw == 0.0;
            negative |= v.rhw < 0.0;
            positive |= v.rhw > 0.0;
        }
        if !finite {
            return super::depth_pole(&submission.vertices, &submission.triangles, true);
        }
        if submission.policy.perspective && zero {
            return Err(TexturedPixelError::ReciprocalDepthSign.into());
        }
        // A far primitive across the world's 16-bit wrap, half a world away,
        // has corners wrapped behind the eye; retail hands D3D their mixed 1/z.
        if submission.policy.perspective && negative && positive {
            if !crate::d3d_viewport::zero_screen_area(&submission.vertices, &submission.triangles) {
                crate::d3d_viewport::log_depth_sign_approximation();
            }
            return Ok(0);
        }
        let blend = if pipeline.alpha_blend_enabled {
            submission.policy.blend
        } else {
            D3dTextureBlend::Copy
        };
        // Validate both clipped polygons before the first pixel submission.
        let mut polygons: [ClippedPolygon; T] = std::array::from_fn(|_| ClippedPolygon::new());
        for (polygon, indices) in polygons.iter_mut().zip(submission.triangles) {
            let vertices = indices.map(|i| self.wet_vertex(submission.vertices[i]));
            if submission.mode & 0x20 != 0 {
                polygon.extend_from_slice(&vertices);
            } else {
                // Finite: checked above, and the wet offsets keep it so.
                clip_finite_into(
                    &vertices,
                    D3dViewportState {
                        bounds: self.clip_bounds(pipeline.viewport),
                        near_depth: submission.near_depth,
                        mode: submission.mode,
                    },
                    polygon,
                )?;
            }
        }
        for polygon in &mut polygons {
            self.place(polygon);
        }
        let fog = self.wet_fog(submission.policy.fog);
        // A reciprocal-depth pole is not a supported CPU sampling policy.
        // Reject both polygons before writes, while retaining native signed
        // conversion in the submission adapter for inspection.
        if submission.policy.perspective
            && polygons.iter().any(|polygon| {
                polygon.iter().any(|v| v.rhw == 0.0)
                    || (polygon.iter().any(|v| v.rhw < 0.0) && polygon.iter().any(|v| v.rhw > 0.0))
            })
        {
            return Err(TexturedPixelError::ReciprocalDepthSign.into());
        }
        if let Some(device) = &mut self.device {
            // The device samples every texel the CPU raster would; texels
            // outside the uploaded border, which stop the CPU raster, read
            // the zeroed padding instead.
            let draw = DeviceDraw {
                blend,
                depth_test: pipeline.depth_enabled,
                depth_write: pipeline.depth_write,
                texture: Some(DeviceTexturing {
                    texture,
                    sampling,
                    perspective: submission.policy.perspective,
                    fog,
                }),
            };
            for polygon in &polygons {
                device.fan(polygon, &draw);
            }
            // The device does not count fragments.
            return Ok(0);
        }
        let mut fragments = 0;
        for polygon in &polygons {
            if polygon.len() < 3 {
                continue;
            }
            for edge in polygon[1..].windows(2) {
                let vertices = [polygon[0], edge[0], edge[1]];
                fragments += raster::rasterize_shaded(
                    self,
                    &vertices,
                    pipeline,
                    blend,
                    |weights, diffuse| {
                        let weighted = |get: &dyn Fn(usize) -> f64| {
                            (0..3).map(|i| weights[i] * get(i)).sum::<f64>()
                        };
                        let uv = if submission.policy.perspective {
                            let rhw = weighted(&|i| vertices[i].rhw.into());
                            std::array::from_fn(|axis| {
                                weighted(&|i| {
                                    f64::from(vertices[i].uv[axis]) * f64::from(vertices[i].rhw)
                                }) / rhw
                            })
                        } else {
                            std::array::from_fn(|axis| weighted(&|i| vertices[i].uv[axis].into()))
                        };
                        let texel = sample(texture, uv, sampling)?;
                        // D3DTBLEND_MODULATEALPHA (4): white RGB survives, while
                        // texture alpha multiplies descriptor-selected 255/128.
                        let mut color = std::array::from_fn(|c| {
                            ((u32::from(texel[c]) * u32::from(diffuse[c]) + 127) / 255) as u8
                        });
                        if let D3dTextureFog::Vertex { color: fog_color } = fog {
                            // Native TL vertices encode visibility in specular alpha.
                            // D3D interpolates this factor in screen space (not UV/RHW)
                            // and fogs RGB after texturing, before destination blending.
                            let visibility =
                                weighted(&|i| f64::from(vertices[i].specular >> 24)) / 255.0;
                            for c in 0..3 {
                                color[c] = (f64::from(color[c]) * visibility
                                    + f64::from(fog_color[c]) * (1.0 - visibility))
                                    .round_ties_even()
                                    .clamp(0.0, 255.0)
                                    as u8;
                            }
                        }
                        Ok::<_, TexturedPixelError>(color)
                    },
                )?;
            }
        }
        Ok(fragments)
    }
}

fn sample(
    texture: &CpuTexture,
    uv: [f64; 2],
    state: D3dTextureSampling,
) -> Result<[u8; 4], TexturedPixelError> {
    if uv
        .iter()
        .any(|x| !x.is_finite() || x.abs() > i32::MAX as f64 / 256.0)
    {
        return Err(TexturedPixelError::Sample);
    }
    let fetch = |x: i64, y: i64| {
        let address = |x: i64| match state.address {
            1 => x.rem_euclid(i64::from(TEXTURE_SURFACE_SIZE)) as u32,
            3 => x.clamp(0, i64::from(TEXTURE_SURFACE_SIZE) - 1) as u32,
            _ => unreachable!("validated native address policy"),
        };
        texture
            .texel(address(x), address(y))
            .ok_or(TexturedPixelError::Sample)
    };
    let [u, v] = uv.map(|x| x * f64::from(TEXTURE_SURFACE_SIZE));
    if state.min_filter == 1 {
        return fetch(u.floor() as i64, v.floor() as i64);
    }
    // Direct3D texel centers are (i+0.5)/surface-size. Duplicated upload
    // borders provide the neighboring samples at the authored UV bounds.
    let [x, y] = [u - 0.5, v - 0.5];
    let [ix, iy] = [x.floor() as i64, y.floor() as i64];
    let [fx, fy] = [x - x.floor(), y - y.floor()];
    let pixels = [
        fetch(ix, iy)?,
        fetch(ix + 1, iy)?,
        fetch(ix, iy + 1)?,
        fetch(ix + 1, iy + 1)?,
    ];
    Ok(std::array::from_fn(|c| {
        let top = f64::from(pixels[0][c]) * (1.0 - fx) + f64::from(pixels[1][c]) * fx;
        let bottom = f64::from(pixels[2][c]) * (1.0 - fx) + f64::from(pixels[3][c]) * fx;
        (top * (1.0 - fy) + bottom * fy)
            .round_ties_even()
            .clamp(0., 255.) as u8
    }))
}
