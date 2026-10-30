//! Four-corner slope/plane probe 00450BD0. This is not the interpolated height
//! query 00450A30: it snaps X/Z to subcell centers and can reject before Y exists.

use super::{TerrainRuntime, TerrainSpacing};
use inf_formats::TERRAIN_GRID_SIDE;

/// Resolved native byte arguments four/five. A nonzero clamp byte dominates
/// the reject byte; the original caller 00429EE0 uses the latter two variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainProbePlanePolicy {
    Unmodified,
    RejectBelow,
    ClampToPlane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainSlopeProbeRequest {
    /// Both original stack coordinates are unsigned low WORDs.
    pub xz: [u16; 2],
    /// Signed DWORD compared to the final wrapping, shifted slope measure.
    pub maximum_step: i32,
    pub plane: TerrainProbePlanePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainProbePosition {
    /// Snapped unsigned low WORDs, written before any plane rejection.
    pub xz: [u16; 2],
    /// Early plane rejection leaves the caller's stack Y unwritten.
    pub y: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainSlopeProbeResult {
    pub blocked: bool,
    pub position: TerrainProbePosition,
}

impl TerrainRuntime {
    /// Complete 00450BD0 against prepared mutable H12 cells and its existing
    /// process spacing/height/plane policy. No collision or cache is implied.
    pub fn probe_slope(&self, request: TerrainSlopeProbeRequest) -> TerrainSlopeProbeResult {
        let [x, z] = request.xz;
        let mask = self.sampling.spacing.units() - 1;
        let step = usize::from(self.sampling.spacing.units() >> 7);
        let x0 = usize::from((x >> 7) & !(mask >> 7));
        let z0 = usize::from((z >> 7) & !(mask >> 7));
        let x1 = (x0 + step) & (TERRAIN_GRID_SIDE - 1);
        let z1 = (z0 + step) & (TERRAIN_GRID_SIDE - 1);
        let height =
            |x, z| i32::from(self.cell(x, z).height_i8()) << self.sampling.height_scale.shift();
        let [mut a, mut b, mut c, mut d] = [
            height(x0, z0),
            height(x1, z0),
            height(x1, z1),
            height(x0, z1),
        ];
        let alternate = self.cell(x0, z0).stored_alternate_diagonal();
        let average = |a: i32, b: i32| a.wrapping_add(b) / 2;
        let (xz, active_diagonal, mut center) = if self.sampling.spacing == TerrainSpacing::Units128
        {
            (
                [(x & !mask).wrapping_add(64), (z & !mask).wrapping_add(64)],
                true,
                0,
            )
        } else {
            let x_high = x & mask >= 128;
            let z_high = z & mask >= 128;
            let quadrant = if z_high {
                3 - u8::from(x_high)
            } else {
                u8::from(x_high)
            };
            let center = if alternate {
                average(b, d)
            } else {
                average(a, c)
            };
            let active = (quadrant & 1 != 0) == alternate;
            match quadrant {
                0 => {
                    b = average(b, a);
                    d = average(d, a);
                    c = center;
                }
                // 00450D51 uses SAR1 here, unlike the other truncating means.
                1 => {
                    a = a.wrapping_add(b) >> 1;
                    d = center;
                    c = average(b, c);
                }
                2 => {
                    a = center;
                    b = average(b, c);
                    d = average(d, c);
                }
                3 => {
                    a = average(d, a);
                    b = center;
                    c = average(d, c);
                }
                _ => unreachable!(),
            }
            (
                [
                    (x & !mask).wrapping_add(if x_high { 192 } else { 64 }),
                    (z & !mask).wrapping_add(if z_high { 192 } else { 64 }),
                ],
                active,
                center,
            )
        };
        if request.plane != TerrainProbePlanePolicy::Unmodified {
            let plane = (self.plane_base >> (self.plane_shift() & 31)) as i16 as i32;
            match request.plane {
                TerrainProbePlanePolicy::ClampToPlane => {
                    a = a.max(plane);
                    b = b.max(plane);
                    c = c.max(plane);
                    d = d.max(plane);
                }
                TerrainProbePlanePolicy::RejectBelow if [a, b, c, d].iter().any(|&h| h < plane) => {
                    return TerrainSlopeProbeResult {
                        blocked: true,
                        position: TerrainProbePosition { xz, y: None },
                    };
                }
                _ => {}
            }
        }
        let diagonal_slope = if active_diagonal {
            let (first, second) = if alternate {
                center = average(b, d);
                (a, c)
            } else {
                center = average(a, c);
                (d, b)
            };
            first
                .wrapping_sub(center)
                .wrapping_abs()
                .max(second.wrapping_sub(center).wrapping_abs())
                .wrapping_mul(363)
                >> 8
        } else {
            // The original coarse full-cell mean survives plane clamping in
            // this arm. Recomputing Y from clamped subcell heights is different.
            0
        };
        let maximum = [
            a.wrapping_sub(b).wrapping_abs(),
            b.wrapping_sub(c).wrapping_abs(),
            c.wrapping_sub(d).wrapping_abs(),
            d.wrapping_sub(a).wrapping_abs(),
            diagonal_slope,
        ]
        .into_iter()
        .max()
        .unwrap();
        TerrainSlopeProbeResult {
            blocked: request.maximum_step < (maximum.wrapping_shl(8) >> 7),
            position: TerrainProbePosition {
                xz,
                y: Some(center as u16),
            },
        }
    }
}

#[cfg(test)]
pub(super) mod tests;
