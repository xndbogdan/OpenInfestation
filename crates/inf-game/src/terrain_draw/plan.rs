//! 0044AA10 footprint construction. Context vectors are stored f32 values;
//! quantization uses CRT __ftol (truncate), not FISTP nearest-even.

use inf_formats::native_basis::{orient_from_forward, NativeBasis};

use super::{
    domain::RETAIL_TERRAIN_ROWS, terrain_row_domain, TerrainDomain, TerrainDrawFault, TerrainGrid,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainDrawPlan {
    pub furniture: TerrainDomain,
    pub near: TerrainDomain,
    pub far: TerrainDomain,
    pub near_fade: [i32; 2],
    pub far_fade: [i32; 2],
    pub sort_bias: i32,
}

/// How far the footprints reach to each side of the view's forward axis.
/// Retail's lateral extents, 11/8 of a corner's depth and the back corners'
/// `0x300` plus the tilt's share, cover a 4:3 view, whose focal is half its
/// width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainFootprintWidth {
    /// Retail's footprints, with their WORD wrapping.
    Retail,
    /// Port option: a view that sees `across / retail` times as far as
    /// retail's (a widescreen frame, the Remastered camera's narrower
    /// focal and its speed kick), so every lateral extent widens by that
    /// ratio. A widened corner stops where its distance from the eye
    /// reaches [`WIDE_REACH`], so the planar turn cannot wrap its WORDs (a
    /// far footprint at Tabuchi's fade already reaches 32427); one retail
    /// places beyond that keeps retail's value. Without it the ground at a
    /// 16:9 frame's sides ends short of the view, deep water shows the
    /// background through its additive surface, and the furniture at a
    /// wider view's edges is never drawn.
    Wide { across: u32, retail: u32 },
}

impl TerrainFootprintWidth {
    /// The footprints for a drawing surface at retail's focal: retail's on
    /// a 4:3 surface, and a wider one sees `width / (height * 4 / 3)` times
    /// as far to each side.
    pub fn for_surface(viewport: [i32; 2]) -> Self {
        let [width, height] = viewport.map(|v| v.max(0).unsigned_abs());
        if u64::from(width) * 3 > u64::from(height) * 4 {
            Self::Wide {
                across: width * 3,
                retail: height * 4,
            }
        } else {
            Self::Retail
        }
    }

    /// The footprints for a view `half` pixels from its centre to its
    /// farther edge on each axis (across, down) at `focal`, as
    /// [`crate::object_runtime::ObjectDrawCone::View`] keeps them. Retail's
    /// tiers see as far across as their focal and 3/4 of it down; a view
    /// reaching further on either axis widens by the larger of the two
    /// (the Remastered camera's 100 degrees across a 4:3 frame by its
    /// narrower focal: 512 / 430), and one reaching no further keeps
    /// retail's.
    pub fn for_view(half: [u32; 2], focal: [u32; 2]) -> Self {
        let [hx, hy] = half;
        let [fx, fy] = focal.map(|focal| focal.max(1));
        // half[0] / fx against half[1] / (fy * 3 / 4), over 3 * fx * fy.
        let (across, retail) =
            if u64::from(hx) * 3 * u64::from(fy) >= u64::from(hy) * 4 * u64::from(fx) {
                (hx, fx)
            } else {
                (hy * 4, fy * 3)
            };
        if across > retail {
            Self::Wide { across, retail }
        } else {
            Self::Retail
        }
    }

    /// The rows a footprint keeps: retail's buffer, or every row of a
    /// widescreen footprint. A widened footprint seen from the side spans
    /// more rows than retail's buffer holds (360 against 176 for the
    /// furniture's at the full draw distance on fine terrain), and retail's
    /// cap would drop every row beyond, with the furniture on them.
    pub(super) fn row_capacity(self) -> Option<usize> {
        match self {
            Self::Retail => Some(RETAIL_TERRAIN_ROWS),
            Self::Wide { .. } => None,
        }
    }

    /// A corner's lateral extent as retail computes it, widened for the
    /// surface; `z` is the corner's distance along the forward axis.
    pub(super) fn lateral(self, extent: i32, z: i32) -> i16 {
        match self {
            Self::Retail => extent as i16,
            Self::Wide { across, retail } => {
                let wide = i64::from(extent) * i64::from(across) / i64::from(retail.max(1));
                let z = i64::from(z.unsigned_abs()).min(WIDE_REACH);
                let room = (WIDE_REACH * WIDE_REACH - z * z).isqrt();
                wide.min(room).max(i64::from(extent)) as i16
            }
        }
    }
}

/// How far from the eye a widened footprint corner may reach: a WORD, less
/// a margin for the turn's rounding.
const WIDE_REACH: i64 = 0x7f00;

pub fn terrain_draw_plan(
    basis: NativeBasis,
    context_fade: [i32; 2],
    grid: TerrainGrid,
    footprint: TerrainFootprintWidth,
) -> Result<TerrainDrawPlan, TerrainDrawFault> {
    let f = |word: usize| f64::from(f32::from_bits(basis.words[word]));
    let quantize = |value: f64| ftol_low32(value) as i16;
    // context+30 is the forward vector's Y component.
    let forward = if ftol_low32(f(8) * 4096.0).wrapping_abs() <= 2048 {
        [quantize(f(7) * 4096.0), quantize(f(9) * 4096.0)]
    } else {
        [quantize(f(3) * -4096.0), quantize(f(1) * 4096.0)]
    };
    let x = f64::from(forward[0]);
    let z = f64::from(forward[1]);
    // FST m32 spills length before the two FDIV operations.
    let length = (z * z + x * x).sqrt() as f32;
    let forward = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [
            (x / f64::from(length)) as f32,
            0.0,
            (z / f64::from(length)) as f32,
        ]
    };
    let mut planar = basis;
    orient_from_forward(&mut planar, forward, [0.0, 1.0, 0.0], false);
    let transform = |corners: [[i16; 2]; 4]| {
        corners.map(|[x, z]| {
            let p = planar.transform_point([i32::from(x), 0, i32::from(z)]);
            [p[0] as i16, p[2] as i16]
        })
    };
    // Retail's manager never draws further than 0x3100 (12,544). A world the
    // tracer lets see further (its fog tuning's `far`) keeps the raster's
    // ground plan there, whose corners are WORDs; the tracer draws its own
    // ground from the whole grid.
    let far = context_fade[1].min(RETAIL_FAR_LIMIT);
    // 11/8 of `depth` to each side, at `z` along the forward axis.
    let width = |depth: i32, z: i32| footprint.lateral(depth.wrapping_mul(11) / 8, z);
    let tilt = ftol_low32(f(8) * -110.0);
    let excess = if tilt > 10 { tilt.wrapping_sub(10) } else { 0 };
    let back_z = -0x200 - excess.wrapping_mul(0x500) / 100;
    let back_width = footprint.lateral(excess.wrapping_mul(0xb00) / 100 + 0x300, back_z);
    let back_z = back_z as i16;
    let first = [
        [width(far, far + 0x200).wrapping_neg(), (far + 0x200) as i16],
        [width(far, far + 0x200), (far + 0x200) as i16],
        [back_width, back_z],
        [back_width.wrapping_neg(), back_z],
    ];
    // The bias is the saved caller forward dotted with one of the two
    // read-only integer vectors at536FA8/536FB8: [128,0,+/-128].
    let direction = if f(9) > 0.0 { 128.0 } else { -128.0 };
    let bias_word = ftol_low32(128.0 * f(7) + 0.0 * f(8) + direction * f(9));
    let sort_bias = i32::from(grid.spacing).wrapping_mul(bias_word) / 256;
    let near_fade = [far.wrapping_mul(30) / 100, far.wrapping_mul(55) / 100];
    let mut near_corners = first;
    let near_edge = near_fade[1] + 0x200;
    near_corners[0] = [
        width(near_fade[1], near_edge).wrapping_neg(),
        near_edge as i16,
    ];
    near_corners[1] = [width(near_fade[1], near_edge), near_edge as i16];
    let far_fade = [far.wrapping_mul(5) / 4, far.wrapping_mul(3) / 2];
    let back_z = near_fade[1]
        - if ftol_low32(f(9) * 64.0) < 0 && ftol_low32(f(7) * 64.0).wrapping_abs() < 48 {
            0x600
        } else {
            0x200
        };
    let far_edge = far_fade[1] + 1000;
    let far_corners = [
        [width(far_fade[1], far_edge).wrapping_neg(), far_edge as i16],
        [width(far_fade[1], far_edge), far_edge as i16],
        [width(near_fade[1], back_z), back_z as i16],
        [width(near_fade[1], back_z).wrapping_neg(), back_z as i16],
    ];
    Ok(TerrainDrawPlan {
        furniture: terrain_row_domain(transform(first), grid, footprint.row_capacity())?,
        near: terrain_row_domain(transform(near_corners), grid, footprint.row_capacity())?,
        far: terrain_row_domain(
            transform(far_corners),
            TerrainGrid::FAR,
            footprint.row_capacity(),
        )?,
        near_fade,
        far_fade,
        sort_bias,
    })
}

/// `0048E6BF..738`'s ceiling on the manager's far distance.
const RETAIL_FAR_LIMIT: i32 = 0x3100;

/// CRT 00511BA8 switches to truncation for FISTP QWORD, then returns the
/// low DWORD. Rust's saturating float-to-i32 cast is not that conversion.
pub(super) fn ftol_low32(value: f64) -> i32 {
    let integral = value.trunc();
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
        integral as i64 as i32
    } else {
        0
    }
}
