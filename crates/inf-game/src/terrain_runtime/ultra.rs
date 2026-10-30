//! Ultra ground detail ([the plan](../../../../docs/rt-plans/ultra-ground.md)), a port setting of
//! the traced world: retail's height grid as a smooth surface, sampled every
//! 64 units, which the game stands on as the tracer draws it.
//!
//! Each cell is a bicubic Hermite patch through its four corner heights,
//! with no twist. At each grid point and along each axis, the slope is the
//! mean of the two segments meeting there, unless they turn by more than the
//! crease angle: then each side keeps its own segment's slope. Hills round
//! off, smooth from cell to cell; ridges, cliff edges and the foot of a
//! cliff stay sharp. A shared edge takes the same corners and slopes from
//! both cells, so no crack opens.
//!
//! Buildings' footings ([`Footing`]) then raise the ground under them to
//! their base, easing back to its slope past them, so no gap opens under a
//! building on a slope. The ground only rises: where a building sinks into
//! a slope it stays sunk, as retail stands it.

use std::sync::atomic::{AtomicU64, Ordering};

use inf_formats::{TerrainCell, TERRAIN_GRID_SIDE};

/// Fine points along a cell's side.
pub const SPLIT: usize = 2;
/// Fine points along the world's side.
pub const FINE_SIDE: usize = TERRAIN_GRID_SIDE * SPLIT;
/// A cell's width in world units, on High.
const CELL: f32 = 128.0;
/// The fine points' spacing in world units.
const FINE: f32 = CELL / SPLIT as f32;
/// How far past a footing's outline the ground stands at its base: a fine
/// point's spacing, so the ground between the outline and the next point
/// out stays at the base too.
const MARGIN: f32 = FINE;

static GENERATION: AtomicU64 = AtomicU64::new(1);

/// The tuning's Ultra ground (`ultra`): the crease angle in degrees, the
/// most a footing raises the ground, and how far past it the ground eases
/// back to its slope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UltraShape {
    pub crease: f32,
    pub fill: f32,
    pub ease: f32,
}

/// A building's footing: the ground under `outline` (a convex polygon in
/// world X and Z, either way round) rises to `base`.
#[derive(Clone, Debug, PartialEq)]
pub struct Footing {
    pub outline: Vec<[f32; 2]>,
    pub base: f32,
}

/// The fine height grid (X-major, as the cells), and what it was built
/// with.
#[derive(Clone)]
pub struct UltraGround {
    shape: UltraShape,
    /// The smooth surface's points, before the footings.
    smooth: Box<[f32]>,
    heights: Box<[f32]>,
    footings: Vec<Footing>,
    /// New with every change, so the traced mesh knows to rebuild.
    generation: u64,
}

/// Equal heights from equal inputs, whenever they were made.
impl PartialEq for UltraGround {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape
            && self.smooth == other.smooth
            && self.heights == other.heights
            && self.footings == other.footings
    }
}

impl std::fmt::Debug for UltraGround {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UltraGround")
            .field("shape", &self.shape)
            .field("points", &self.heights.len())
            .field("footings", &self.footings.len())
            .finish()
    }
}

/// Retail's grid heights in world units (`00450A30`'s: the signed byte
/// shifted, kept to 16 bits), wrapping.
struct Grid<'a> {
    cells: &'a [TerrainCell],
    shift: u32,
    /// The crease angle, in radians.
    crease: f32,
}

impl Grid<'_> {
    fn height(&self, x: usize, z: usize) -> f32 {
        let side = TERRAIN_GRID_SIDE;
        let cell = self.cells[(x % side) * side + z % side];
        f32::from((i32::from(cell.height_i8()) << self.shift) as i16)
    }

    /// The slopes (height a cell) at point `x`, `z` along an axis (0 X, 1
    /// Z) for the cell after it and the cell before it.
    fn slopes(&self, x: usize, z: usize, axis: usize) -> [f32; 2] {
        let side = TERRAIN_GRID_SIDE;
        let step = |by: usize| {
            if axis == 0 {
                self.height((x + by) % side, z)
            } else {
                self.height(x, (z + by) % side)
            }
        };
        let [before, here, after] = [step(side - 1), step(0), step(1)];
        let [into_before, into_after] = [here - before, after - here];
        let turn = (into_after / CELL).atan() - (into_before / CELL).atan();
        if turn.abs() > self.crease {
            [into_after, into_before]
        } else {
            let mean = (into_before + into_after) * 0.5;
            [mean, mean]
        }
    }

    /// The patch of cell `x`, `z` at `s` along X and `t` along Z, each 0
    /// to 1.
    fn patch(&self, x: usize, z: usize, s: f32, t: f32) -> f32 {
        // The cubic Hermite basis: the ends' heights and slopes.
        let basis = |u: f32| {
            let [u2, u3] = [u * u, u * u * u];
            [
                2.0 * u3 - 3.0 * u2 + 1.0,
                -2.0 * u3 + 3.0 * u2,
                u3 - 2.0 * u2 + u,
                u3 - u2,
            ]
        };
        let [s0, s1, ds0, ds1] = basis(s);
        let [t0, t1, dt0, dt1] = basis(t);
        let mut height = 0.0;
        for (a, along_s, slope_s) in [(0, s0, ds0), (1, s1, ds1)] {
            for (b, along_t, slope_t) in [(0, t0, dt0), (1, t1, dt1)] {
                let (cx, cz) = (x + a, z + b);
                // A corner's slope toward the cell: the slope after it at
                // the cell's near end, before it at its far end.
                let x_slope = self.slopes(cx, cz, 0)[a];
                let z_slope = self.slopes(cx, cz, 1)[b];
                height += self.height(cx, cz) * along_s * along_t
                    + x_slope * slope_s * along_t
                    + z_slope * along_s * slope_t;
            }
        }
        height
    }

    /// Fine point `fx`, `fz`: on its cell's patch.
    fn fine(&self, fx: usize, fz: usize) -> f32 {
        let [x, z] = [fx / SPLIT, fz / SPLIT];
        let [s, t] = [fx % SPLIT, fz % SPLIT].map(|part| part as f32 / SPLIT as f32);
        if s == 0.0 && t == 0.0 {
            return self.height(x, z);
        }
        self.patch(x, z, s, t)
    }
}

/// `t` from 0 to 1, eased at both ends.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl UltraGround {
    /// The fine grid over `cells`, whose height bytes take `shift`, shaped
    /// as `shape` says, with `footings` raised.
    pub fn build(
        cells: &[TerrainCell],
        shift: u32,
        shape: UltraShape,
        footings: Vec<Footing>,
    ) -> Self {
        let grid = Grid {
            cells,
            shift,
            crease: shape.crease.to_radians(),
        };
        let smooth: Box<[f32]> = (0..FINE_SIDE * FINE_SIDE)
            .map(|index| grid.fine(index / FINE_SIDE, index % FINE_SIDE))
            .collect();
        let mut ground = Self {
            shape,
            heights: smooth.clone(),
            smooth,
            footings,
            generation: 0,
        };
        for index in 0..ground.footings.len() {
            ground.raise(index);
        }
        ground.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
        ground
    }

    /// The fine points a change to cell `x`, `z` reaches redone: its
    /// neighbours' slopes, and every patch with one of them for a corner;
    /// then the footings again.
    pub fn update(&mut self, cells: &[TerrainCell], shift: u32, [x, z]: [usize; 2]) {
        let grid = Grid {
            cells,
            shift,
            crease: self.shape.crease.to_radians(),
        };
        let side = TERRAIN_GRID_SIDE;
        for cx in 0..5 {
            for cz in 0..5 {
                let [px, pz] = [(x + side + cx - 2) % side, (z + side + cz - 2) % side];
                for part in 0..SPLIT * SPLIT {
                    let fx = px * SPLIT + part / SPLIT;
                    let fz = pz * SPLIT + part % SPLIT;
                    let index = fx * FINE_SIDE + fz;
                    self.smooth[index] = grid.fine(fx, fz);
                    self.heights[index] = self.smooth[index];
                }
            }
        }
        // A footing only raises, so laying them all again restores what
        // they raised round the change and leaves the rest as it was.
        for index in 0..self.footings.len() {
            self.raise(index);
        }
        self.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    }

    /// The footings to raise in place of the ones before.
    pub fn set_footings(&mut self, footings: Vec<Footing>) {
        self.heights.copy_from_slice(&self.smooth);
        self.footings = footings;
        for index in 0..self.footings.len() {
            self.raise(index);
        }
        self.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    }

    /// Raise the ground round footing `index`: to its base within
    /// [`MARGIN`] of its outline, easing back to the smooth surface over
    /// the shape's `ease` past that, never lower than it stands. Nothing
    /// when the ground under it lies more than the shape's `fill` below its
    /// base (a building over a drop); whether it raised.
    fn raise(&mut self, index: usize) -> bool {
        let Footing { outline, base } = &self.footings[index];
        let base = *base;
        if outline.len() < 3 {
            return false;
        }
        // Each edge's start and outward normal: the signed distance from a
        // convex polygon is the most of its edges'.
        let area: f32 = (0..outline.len())
            .map(|k| {
                let [a, b] = [outline[k], outline[(k + 1) % outline.len()]];
                a[0] * b[1] - b[0] * a[1]
            })
            .sum();
        let edges: Vec<([f32; 2], [f32; 2])> = (0..outline.len())
            .filter_map(|k| {
                let [a, b] = [outline[k], outline[(k + 1) % outline.len()]];
                let [dx, dz] = [b[0] - a[0], b[1] - a[1]];
                let length = dx.hypot(dz);
                (length > 1.0e-3).then(|| {
                    let normal = if area > 0.0 { [dz, -dx] } else { [-dz, dx] };
                    (a, normal.map(|c| c / length))
                })
            })
            .collect();
        let distance = |p: [f32; 2]| {
            edges
                .iter()
                .map(|(a, n)| (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1])
                .fold(f32::MIN, f32::max)
        };
        let reach = MARGIN + self.shape.ease;
        let low =
            [0, 1].map(|axis| outline.iter().map(|p| p[axis]).fold(f32::MAX, f32::min) - reach);
        let high =
            [0, 1].map(|axis| outline.iter().map(|p| p[axis]).fold(f32::MIN, f32::max) + reach);
        let [first, last] = [low, high].map(|at| at.map(|c| (c / FINE).floor() as i64));
        let points = || {
            (first[0]..=last[0] + 1)
                .flat_map(move |fx| (first[1]..=last[1] + 1).map(move |fz| (fx, fz)))
        };
        let at = |(fx, fz): (i64, i64)| {
            let side = FINE_SIDE as i64;
            (fx.rem_euclid(side) as usize) * FINE_SIDE + fz.rem_euclid(side) as usize
        };
        let place = |(fx, fz): (i64, i64)| [fx as f32 * FINE, fz as f32 * FINE];
        if points().any(|point| {
            distance(place(point)) <= MARGIN && base - self.smooth[at(point)] > self.shape.fill
        }) {
            return false;
        }
        for point in points() {
            let d = distance(place(point));
            if d >= reach {
                continue;
            }
            let index = at(point);
            let smooth = self.smooth[index];
            let target = base + (smooth - base) * smoothstep((d - MARGIN) / self.shape.ease);
            self.heights[index] = self.heights[index].max(target);
        }
        true
    }

    /// How it was shaped.
    pub fn shape(&self) -> UltraShape {
        self.shape
    }

    /// New with every change to its heights.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Fine point `fx`, `fz`'s height (wrapping).
    pub fn point(&self, fx: usize, fz: usize) -> f32 {
        self.heights[(fx % FINE_SIDE) * FINE_SIDE + fz % FINE_SIDE]
    }

    /// The ground's height at world `x`, `z`: on the fine cell's two
    /// triangles, split as `alternate` says its parent cell is.
    pub fn height_at(&self, [x, z]: [u16; 2], alternate: bool) -> f32 {
        let shift = FINE.log2() as u32;
        let [fx, fz] = [usize::from(x >> shift), usize::from(z >> shift)];
        let [dx, dz] = [x, z].map(|at| f32::from(at & (FINE as u16 - 1)) / FINE);
        let [h00, h10, h11, h01] = [
            self.point(fx, fz),
            self.point(fx + 1, fz),
            self.point(fx + 1, fz + 1),
            self.point(fx, fz + 1),
        ];
        // As 00450A30 splits a cell.
        if alternate {
            if dx < 1.0 - dz {
                h00 + (h01 - h00) * dz + (h10 - h00) * dx
            } else {
                h01 + (h10 - h11) * (1.0 - dz) + (h11 - h01) * dx
            }
        } else if dx < dz {
            h00 + (h11 - h01) * dx + (h01 - h00) * dz
        } else {
            h00 + (h11 - h10) * dz + (h10 - h00) * dx
        }
    }
}

#[cfg(test)]
#[path = "ultra_tests.rs"]
mod tests;
