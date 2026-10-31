//! The traced world's ground (modern mode,
//! [the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md)): the whole
//! terrain grid as one mesh, with the heights, blend tiles and diagonals the
//! raster draws ([`crate::terrain_draw`]'s `quad.rs`), built again only when
//! the terrain changes (bridges, the ending's flood). The world wraps every
//! 65,536 units: the tracer cuts the mesh into tiles, each placed at its copy
//! nearest the eye ([`crate::scene_ground_detail`]).
//!
//! Retail lights the ground far more dimly than models: each cell carries a
//! baked shade (`shade4`, 0..15) and the raster draws the tint times
//! shade/15, about half of N.L plus a little (Terra Nova: 0.52 N.L + 0.07)
//! where models take N.L plus a quarter. The tracer lights everything alike,
//! so each ground vertex gets the albedo that turns the tracer's full light
//! (N.L plus [`AMBIENT_SHARE`]) into retail's shade: sunlit ground looks as
//! retail drew it, and shadow, bounce and lamps work from there.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use inf_formats::{terrain_blend_table, TerrainCell, TERRAIN_GRID_SIDE};

use crate::{
    scene_ground_detail::{TILES, TILE_UNITS},
    terrain_runtime::TerrainRuntime,
};

/// The tracer's ambient light as a share of the sun's: retail's object
/// ambient, the tint shifted right by two (`world_scene.rs`).
pub const AMBIENT_SHARE: f32 = 0.25;

/// How retail's baked ground shade rises with the sun's facing, in display
/// tones: the shade is about `a` N.L + `b` (Terra Nova: 0.52 N.L + 0.07),
/// and this is `a`, the least-squares slope of the dry ground's (facing,
/// shade) pairs. Traced light brings the N.L, so full path tracing's ground
/// keeps `a` as its reflectance (with the sun's tint, `pt_trace.comp`'s
/// `ground_reflectance`) and loses the shade's baked light.
pub fn ground_shade_slope(samples: impl Iterator<Item = (f32, f32)>) -> f32 {
    let [mut n, mut x, mut y, mut xx, mut xy] = [0.0_f64; 5];
    for (facing, shade) in samples {
        let [facing, shade] = [f64::from(facing), f64::from(shade)];
        n += 1.0;
        x += facing;
        y += shade;
        xx += facing * facing;
        xy += facing * shade;
    }
    let spread = n * xx - x * x;
    // A ground all at one facing (flat, or none dry) says nothing of the
    // slope: take its shade as it stands.
    let slope = if spread > 1.0e-9 * n * n {
        (n * xy - x * y) / spread
    } else if n > 0.0 {
        y / n
    } else {
        0.5
    };
    slope.clamp(0.02, 1.0) as f32
}

/// A grid point in world units, X and Z from 0 to 65,536 (the last row and
/// column repeat the first), the height field's normal there, and the
/// albedo that gives retail's baked shade under the tracer's light.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainMeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub albedo: f32,
}

#[derive(Debug)]
pub struct TerrainMesh {
    /// New for every mesh built, so the device knows to upload it.
    pub generation: u64,
    /// Grid points a side (the vertices are X-major) and their spacing.
    pub points: u32,
    pub spacing: f32,
    /// The water plane's height; worlds without water hold it at -4096 or
    /// below, as retail tests (`0044AA10`).
    pub plane: f32,
    /// The world's low ground: a fifth of the grid lies lower, the water
    /// counting at its surface.
    pub low: f32,
    /// Its high ground: four fifths of its land lie lower (the water's
    /// plane where it has none), so a world of sea keeps its hills in the
    /// fog. The distance fog thins above it.
    pub high: f32,
    /// How retail's baked shade rises with the sun's facing, in display
    /// tones ([`ground_shade_slope`]): full path tracing's ground takes its
    /// reflectance from it.
    pub shade_slope: f32,
    /// The mesh this one patched and the ground tiles
    /// ([`crate::scene_ground_detail::tile_ground`]'s, X-major) whose
    /// points it moved, when only cells' heights changed (a bridge rising):
    /// the device rebuilds those tiles alone.
    pub changed: Option<(u64, Vec<usize>)>,
    /// The dry ground's typical albedo, which the ground under the water
    /// takes.
    underwater: Option<f32>,
    pub vertices: Vec<TerrainMeshVertex>,
    pub indices: Rc<[u32]>,
    /// Per triangle: the blend texture's rank (H3 `0x4FC + rank`) in the low
    /// byte, then each corner's place in its tile's texture, six bits each
    /// from bit 8: U then V in quarters, 0 to 4 ([`texture_place`]).
    pub triangles: Rc<[u32]>,
    /// The spacing, height shift, light direction, water plane, Ultra's
    /// generation and cells it was built from.
    source: (u16, u32, [f32; 3], i32, Option<u64>, Box<[TerrainCell]>),
}

/// The low and high ground ([`TerrainMesh::low`], [`TerrainMesh::high`])
/// of `vertices` under water at `plane`.
fn levels(vertices: &[TerrainMeshVertex], plane: f32) -> [f32; 2] {
    let mut levels: Vec<f32> = vertices
        .iter()
        .map(|vertex| vertex.position[1].max(plane))
        .collect();
    let fifth = levels.len() / 5;
    let low = *levels.select_nth_unstable_by(fifth, f32::total_cmp).1;
    let mut land: Vec<f32> = levels.into_iter().filter(|&level| level > plane).collect();
    let high = if land.is_empty() {
        plane
    } else {
        let at = land.len() * 4 / 5;
        *land.select_nth_unstable_by(at, f32::total_cmp).1
    };
    [low, high]
}

/// What a ground point is made from: the terrain's cells as the mesh
/// samples them, Ultra's fine grid if it is on, and the light.
struct GroundGrid<'a> {
    cells: &'a [TerrainCell],
    /// Cells a side as sampled, and the cells between samples.
    side: usize,
    step: usize,
    shift: u32,
    ultra: Option<&'a crate::terrain_runtime::ultra::UltraGround>,
    /// Fine points along a cell's side, and their spacing.
    split: usize,
    units: f32,
    light: [f32; 3],
}

impl<'a> GroundGrid<'a> {
    fn new(terrain: &'a TerrainRuntime, light: [f32; 3]) -> Self {
        let spacing = terrain.sampling().spacing.units();
        let step = usize::from(spacing >> 7);
        // Ultra ground detail's fine grid splits each cell `split` by
        // `split` ([`crate::terrain_runtime::ultra`]).
        let ultra = terrain.ultra();
        let split = if ultra.is_some() {
            crate::terrain_runtime::ultra::SPLIT
        } else {
            1
        };
        Self {
            cells: terrain.cells(),
            side: TERRAIN_GRID_SIDE / step,
            step,
            shift: terrain.height_shift(),
            ultra,
            split,
            units: f32::from(spacing) / split as f32,
            light,
        }
    }

    fn cell(&self, i: usize, j: usize) -> TerrainCell {
        self.cells[(i % self.side) * self.step * TERRAIN_GRID_SIDE + (j % self.side) * self.step]
    }

    /// The height at a fine point: 00450A30's, the signed byte shifted and
    /// kept to 16 bits, or Ultra's.
    fn height(&self, fi: usize, fj: usize) -> f32 {
        match self.ultra {
            Some(ultra) => ultra.point(fi, fj),
            None => f32::from((i32::from(self.cell(fi, fj).height_i8()) << self.shift) as i16),
        }
    }

    /// The baked shade at a fine point, blended from its cell's corners.
    fn shade(&self, fi: usize, fj: usize) -> f32 {
        let split = self.split;
        let [i, j] = [fi / split, fj / split];
        let [s, t] = [fi % split, fj % split].map(|part| part as f32 / split as f32);
        let at = |i, j| f32::from(self.cell(i, j).shade4()) / 15.0;
        at(i, j) * (1.0 - s) * (1.0 - t)
            + at(i + 1, j) * s * (1.0 - t)
            + at(i + 1, j + 1) * s * t
            + at(i, j + 1) * (1.0 - s) * t
    }

    /// Fine point `i`, `j` (to the fine side, whose last repeats the
    /// first), before the water's albedo.
    fn vertex(&self, i: usize, j: usize) -> TerrainMeshVertex {
        let fine_side = self.side * self.split;
        let units = self.units;
        let [west, east] = [(i + fine_side - 1) % fine_side, (i + 1) % fine_side];
        let [south, north] = [(j + fine_side - 1) % fine_side, (j + 1) % fine_side];
        let slope = [
            (self.height(east, j) - self.height(west, j)) / (2.0 * units),
            (self.height(i, north) - self.height(i, south)) / (2.0 * units),
        ];
        let length = (slope[0] * slope[0] + slope[1] * slope[1] + 1.0).sqrt();
        let normal = [-slope[0] / length, 1.0 / length, -slope[1] / length];
        let facing = (0..3)
            .map(|axis| normal[axis] * self.light[axis])
            .sum::<f32>();
        TerrainMeshVertex {
            position: [i as f32 * units, self.height(i, j), j as f32 * units],
            normal,
            albedo: self.shade(i, j) / (facing.max(0.0) + AMBIENT_SHARE),
        }
    }
}

/// The raster's textured quad slots' UVs (0,0), (1,0), (1,1), (0,1), in
/// quarters.
const QUAD_QUARTERS: [[u32; 2]; 4] = [[0, 0], [4, 0], [4, 4], [0, 4]];

/// A corner's place in its tile's texture, U and V in quarters (0 to 4), as
/// a triangle word keeps it: U in the low three bits, V in the next.
pub fn texture_place([u, v]: [u32; 2]) -> u32 {
    u | v << 3
}

/// A tile's blend and the texture place of each of its corners, X0Z0,
/// X1Z0, X1Z1 and X0Z1, or none where the raster stops at the tile (an
/// orientation fault); and whether its slots split it on the diagonal that
/// slot 0 starts.
fn tile_texture(
    blends: &[inf_formats::TerrainBlend; 256],
    cell: impl Fn(usize, usize) -> TerrainCell,
    i: usize,
    j: usize,
) -> Option<(u8, [[u32; 2]; 4], bool)> {
    // The raster's tile corners: its rows run towards -Z, so the old row's
    // tile is (x0, z1), the new row's (x0, z0).
    let corners = [(i, j + 1), (i, j), (i + 1, j), (i + 1, j + 1)];
    let flags = corners.map(|(x, z)| (cell(x, z).raw() >> 8) as u8);
    let combo = flags
        .iter()
        .enumerate()
        .fold(0, |combo, (k, flags)| combo | (flags & 3) << (k * 2));
    let blend = blends[usize::from(combo)];
    let orientation = blend.orientation_word;
    // Corner k lands in quad slot (orientation >> 8k) & 15.
    let slots = std::array::from_fn::<_, 4, _>(|k| ((orientation >> (k * 8)) & 15) as usize);
    let mut covered = [false; 4];
    for &slot in &slots {
        *covered.get_mut(slot)? = true;
    }
    if covered.contains(&false) {
        return None;
    }
    let place = |k: usize| QUAD_QUARTERS[slots[k]];
    let diagonal = (flags[1] & 8 != 0) == (!orientation & 1 != 0);
    Some((
        blend.texture_rank,
        [place(1), place(2), place(3), place(0)],
        diagonal,
    ))
}

static GENERATION: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static MESH: RefCell<Option<Rc<TerrainMesh>>> = const { RefCell::new(None) };
}

/// The mesh of `terrain` under the world's `light` (towards the sun), built
/// again only when its cells, sampling or light changed since the last call.
pub fn terrain_mesh(terrain: &TerrainRuntime, light: [f32; 3]) -> Rc<TerrainMesh> {
    MESH.with(|cached| {
        let mut cached = cached.borrow_mut();
        if let Some(mesh) = cached
            .as_ref()
            .filter(|mesh| mesh.built_from(terrain, light))
        {
            return mesh.clone();
        }
        let _scope = crate::cpu_profile::scope("terrain: mesh build");
        let mesh = Rc::new(
            cached
                .as_ref()
                .and_then(|mesh| mesh.patched(terrain, light))
                .unwrap_or_else(|| TerrainMesh::build(terrain, light)),
        );
        *cached = Some(mesh.clone());
        mesh
    })
}

impl TerrainMesh {
    pub fn build(terrain: &TerrainRuntime, light: [f32; 3]) -> Self {
        let spacing = terrain.sampling().spacing.units();
        let step = usize::from(spacing >> 7);
        let side = TERRAIN_GRID_SIDE / step;
        let shift = terrain.height_shift();
        let cells = terrain.cells();
        let cell =
            |i: usize, j: usize| cells[(i % side) * step * TERRAIN_GRID_SIDE + (j % side) * step];
        // 00450A30's heights: the signed byte shifted, kept to 16 bits.
        // Ultra ground detail's fine grid splits each cell `split` by
        // `split` ([`crate::terrain_runtime::ultra`]).
        let ultra = terrain.ultra();
        let split = if ultra.is_some() {
            crate::terrain_runtime::ultra::SPLIT
        } else {
            1
        };
        let fine_side = side * split;
        let units = f32::from(spacing) / split as f32;
        let points = fine_side + 1;
        let grid = GroundGrid::new(terrain, light);
        let mut vertices = Vec::with_capacity(points * points);
        for i in 0..points {
            for j in 0..points {
                vertices.push(grid.vertex(i, j));
            }
        }
        // Retail never lit the ground under its opaque water (a baked shade
        // of 0 there); the tracer shows it, so it takes the dry ground's
        // typical albedo.
        let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
        let mut dry: Vec<f32> = vertices
            .iter()
            .filter(|vertex| vertex.position[1] >= f32::from(plane) && vertex.albedo > 0.0)
            .map(|vertex| vertex.albedo)
            .collect();
        let mut underwater = None;
        if !dry.is_empty() {
            let middle = dry.len() / 2;
            let typical = *dry.select_nth_unstable_by(middle, f32::total_cmp).1;
            for vertex in &mut vertices {
                if vertex.position[1] < f32::from(plane) && vertex.albedo == 0.0 {
                    vertex.albedo = typical;
                }
            }
            underwater = Some(typical);
        }
        let shade_slope = ground_shade_slope(
            vertices
                .iter()
                .filter(|vertex| vertex.position[1] >= f32::from(plane) && vertex.albedo > 0.0)
                .map(|vertex| {
                    let facing = (0..3)
                        .map(|axis| vertex.normal[axis] * light[axis])
                        .sum::<f32>()
                        .max(0.0);
                    (facing, vertex.albedo * (facing + AMBIENT_SHARE))
                }),
        );
        let blends = terrain_blend_table();
        let mut indices = Vec::with_capacity(fine_side * fine_side * 6);
        let mut triangles = Vec::with_capacity(fine_side * fine_side * 2);
        for i in 0..side {
            for j in 0..side {
                // An orientation fault stops the raster at the tile.
                let Some((rank, places, diagonal)) = tile_texture(&blends, cell, i, j) else {
                    continue;
                };
                if ultra.is_some() {
                    fine_tile(
                        [i, j],
                        split,
                        points,
                        rank,
                        places,
                        cell(i, j).stored_alternate_diagonal(),
                        &mut indices,
                        &mut triangles,
                    );
                    continue;
                }
                // The tile's corners X0Z0, X1Z0, X1Z1, X0Z1 by quad slot,
                // which the places name; the raster's split is slots 0-1-2
                // and 0-2-3 from the slot its diagonal flag starts at.
                let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                let slot_of = places.map(|place| {
                    QUAD_QUARTERS
                        .iter()
                        .position(|&quad| quad == place)
                        .unwrap_or(0)
                });
                let mut by_slot = [0; 4];
                for (k, &slot) in slot_of.iter().enumerate() {
                    by_slot[slot] = k;
                }
                let first = usize::from(!diagonal);
                for slots in [[0, 1, 2], [0, 2, 3]] {
                    let mut word = u32::from(rank);
                    for (n, slot) in slots.into_iter().enumerate() {
                        let k = by_slot[(first + slot) & 3];
                        let (x, z) = corners[k];
                        indices.push((x * points + z) as u32);
                        word |= texture_place(places[k]) << (8 + n * 6);
                    }
                    triangles.push(word);
                }
            }
        }
        let [low, high] = levels(&vertices, f32::from(plane));
        Self {
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            changed: None,
            underwater,
            points: points as u32,
            spacing: units,
            plane: f32::from(plane),
            low,
            high,
            shade_slope,
            vertices,
            indices: indices.into(),
            triangles: triangles.into(),
            source: (
                spacing,
                shift,
                light,
                terrain.plane_base(),
                ultra.map(|ultra| ultra.generation()),
                cells.into(),
            ),
        }
    }

    /// This mesh with the points moved by cells whose heights alone
    /// changed since it was built, and the ground under a water that rose
    /// or fell, or none where more changed (the sampling or light; a cell's
    /// material, shade or diagonal; Ultra ground) or too many cells for a
    /// patch to pay. Rebuilding the whole grid each time a bridge's strip
    /// or a water bridge's plane rose a step slowed the game down
    /// (2026-10-31).
    fn patched(&self, terrain: &TerrainRuntime, light: [f32; 3]) -> Option<Self> {
        // shortcut: the shade slope and the water's albedo stay the whole
        // build's, and so do the low and high ground while the water stays;
        // a strip of cells barely moves them.
        const MOST_CELLS: usize = 4096;
        let (spacing, shift, built_light, plane, ultra, cells) = &self.source;
        if *spacing != terrain.sampling().spacing.units()
            || *shift != terrain.height_shift()
            || *built_light != light
            || ultra.is_some()
            || terrain.ultra().is_some()
        {
            return None;
        }
        let now = terrain.cells();
        let mut changed = Vec::new();
        for (index, (was, is)) in cells.iter().zip(now).enumerate() {
            if was != is {
                if (was.raw() ^ is.raw()) & 0xff00 != 0 || changed.len() == MOST_CELLS {
                    return None;
                }
                changed.push(index);
            }
        }
        let grid = GroundGrid::new(terrain, light);
        let (side, step) = (grid.side, grid.step);
        let points = self.points as usize;
        let water = f32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
        let mut vertices = self.vertices.clone();
        // A new water: the ground below it or the old one lit again, unlit
        // ground under it taking the dry ground's albedo. No point moves.
        if *plane != terrain.plane_base() {
            let below = water.max(self.plane);
            for (index, vertex) in vertices.iter_mut().enumerate() {
                if vertex.position[1] >= below {
                    continue;
                }
                let facing = (0..3)
                    .map(|axis| vertex.normal[axis] * light[axis])
                    .sum::<f32>();
                let shade = grid.shade(index / points, index % points);
                vertex.albedo = shade / (facing.max(0.0) + AMBIENT_SHARE);
                if vertex.position[1] < water && vertex.albedo == 0.0 {
                    vertex.albedo = self.underwater.unwrap_or(0.0);
                }
            }
        }
        let mut tiles = vec![false; TILES * TILES];
        let tile = |at: f32| ((at / TILE_UNITS).max(0.0) as usize).min(TILES - 1);
        for index in changed {
            let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
            if x % step != 0 || z % step != 0 {
                continue;
            }
            let [i, j] = [x / step, z / step];
            // The point and its neighbours, whose normals it slopes, each
            // with its copy on the far edge.
            for [di, dj] in [[0, 0], [1, 0], [side - 1, 0], [0, 1], [0, side - 1]] {
                let [a, b] = [(i + di) % side, (j + dj) % side];
                for a in std::iter::once(a).chain((a == 0).then_some(side)) {
                    for b in std::iter::once(b).chain((b == 0).then_some(side)) {
                        let mut vertex = grid.vertex(a, b);
                        if vertex.position[1] < water && vertex.albedo == 0.0 {
                            vertex.albedo = self.underwater.unwrap_or(0.0);
                        }
                        let [px, pz] = [vertex.position[0], vertex.position[2]];
                        vertices[a * points + b] = vertex;
                        for x in [px - self.spacing, px, px + self.spacing] {
                            for z in [pz - self.spacing, pz, pz + self.spacing] {
                                tiles[tile(x) * TILES + tile(z)] = true;
                            }
                        }
                    }
                }
            }
        }
        let [low, high] = if *plane == terrain.plane_base() {
            [self.low, self.high]
        } else {
            levels(&vertices, water)
        };
        Some(Self {
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            changed: Some((
                self.generation,
                (0..tiles.len()).filter(|&tile| tiles[tile]).collect(),
            )),
            underwater: self.underwater,
            points: self.points,
            spacing: self.spacing,
            plane: water,
            low,
            high,
            shade_slope: self.shade_slope,
            vertices,
            indices: self.indices.clone(),
            triangles: self.triangles.clone(),
            source: (
                *spacing,
                *shift,
                light,
                terrain.plane_base(),
                None,
                now.into(),
            ),
        })
    }

    fn built_from(&self, terrain: &TerrainRuntime, light: [f32; 3]) -> bool {
        let (spacing, shift, built_light, plane, generation, cells) = &self.source;
        *spacing == terrain.sampling().spacing.units()
            && *shift == terrain.height_shift()
            && *built_light == light
            && *plane == terrain.plane_base()
            && *generation == terrain.ultra().map(|ultra| ultra.generation())
            && **cells == *terrain.cells()
    }

    /// Which diagonal each cell's triangles share, a bit a cell (X-major,
    /// 32 to a word): set where it runs from the cell's first corner to its
    /// last, clear where across. Vulkan's raster blends the ground's normals
    /// across the triangle a point lies in, as the tracer does.
    pub fn rising_diagonals(&self) -> Vec<u32> {
        rising_diagonals(self.points as usize, &self.indices)
    }
}

/// [`TerrainMesh::rising_diagonals`] of a grid `points` a side whose
/// triangles' corners are `indices` (X-major points).
fn rising_diagonals(points: usize, indices: &[u32]) -> Vec<u32> {
    let cells = points.saturating_sub(1);
    let mut bits = vec![0_u32; (cells * cells).div_ceil(32)];
    for triangle in indices.chunks_exact(3) {
        let corners = [0, 1, 2].map(|k| {
            let point = triangle[k] as usize;
            (point / points, point % points)
        });
        let x = corners.iter().map(|corner| corner.0).min().unwrap_or(0);
        let z = corners.iter().map(|corner| corner.1).min().unwrap_or(0);
        if corners.contains(&(x, z)) && corners.contains(&(x + 1, z + 1)) {
            let cell = x * cells + z;
            bits[cell / 32] |= 1 << (cell % 32);
        }
    }
    bits
}

/// Tile `i`, `j`'s fine cells on Ultra ground: each split as the game's
/// ground splits the tile (`alternate`, 00450A30's), each corner taking its
/// place in the tile's texture between the tile's corners' `places` (X0Z0,
/// X1Z0, X1Z1, X0Z1), wound up as the raster's are.
#[allow(clippy::too_many_arguments)]
fn fine_tile(
    [i, j]: [usize; 2],
    split: usize,
    points: usize,
    rank: u8,
    places: [[u32; 2]; 4],
    alternate: bool,
    indices: &mut Vec<u32>,
    triangles: &mut Vec<u32>,
) {
    let n = split as u32;
    // The tiles' places are the square's turns and flips, so their blend
    // is the affine map the raster's triangles draw.
    let place = |a: usize, b: usize| {
        let [s, t] = [a as u32, b as u32];
        let blend = |axis: usize| {
            (places[0][axis] * (n - s) * (n - t)
                + places[1][axis] * s * (n - t)
                + places[2][axis] * s * t
                + places[3][axis] * (n - s) * t)
                / (n * n)
        };
        texture_place([blend(0), blend(1)])
    };
    for a in 0..split {
        for b in 0..split {
            let [x, z] = [i * split + a, j * split + b];
            let corner =
                |da: usize, db: usize| (((x + da) * points + z + db) as u32, place(a + da, b + db));
            let [c00, c10, c11, c01] = [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)];
            let pair = if alternate {
                [[c00, c01, c10], [c01, c11, c10]]
            } else {
                [[c11, c10, c00], [c11, c00, c01]]
            };
            for triangle in pair {
                let mut word = u32::from(rank);
                for (k, (index, place)) in triangle.into_iter().enumerate() {
                    indices.push(index);
                    word |= place << (8 + k * 6);
                }
                triangles.push(word);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};

    fn sampling(spacing: TerrainSpacing) -> TerrainSampling {
        TerrainSampling {
            spacing,
            height_scale: TerrainHeightScale::Units32,
        }
    }

    #[test]
    fn the_ground_keeps_its_shades_slope() {
        // Terra Nova's ground: 0.52 N.L + 0.07.
        let terra_nova = (0..=10).map(|k| {
            let facing = k as f32 / 10.0;
            (facing, 0.52 * facing + 0.07)
        });
        assert!((ground_shade_slope(terra_nova) - 0.52).abs() < 1.0e-4);
        // Flat ground has no slope to fit: its shade as it stands.
        let flat = std::iter::repeat_n((1.0, 0.4), 8);
        assert!((ground_shade_slope(flat) - 0.4).abs() < 1.0e-4);
    }

    #[test]
    fn the_ground_mesh_covers_the_wrapping_grid() {
        // A ramp along X (height byte x % 64) with one material-1 cell at
        // (3, 5), the alternate diagonal bit on the tile at (20, 30), and a
        // baked shade of 6.
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
            let material = u16::from((x, z) == (3, 5)) << 8;
            let diagonal = u16::from((x, z) == (20, 30)) << 11;
            6 << 12 | material | diagonal | (x % 64) as u16
        });
        let terrain = TerrainRuntime::from_raw_cells(sampling(TerrainSpacing::Units128), 0, cells);
        // The sun straight overhead.
        let mesh = TerrainMesh::build(&terrain, [0.0, 1.0, 0.0]);
        assert_eq!(mesh.vertices.len(), 513 * 513);
        assert_eq!(mesh.triangles.len(), 2 * 512 * 512);
        assert_eq!(mesh.indices.len(), mesh.triangles.len() * 3);
        // Point (8, 1): X 1024, Z 128, height byte 8 shifted by 5.
        let point = mesh.vertices[8 * 513 + 1];
        assert_eq!(point.position, [1024.0, 256.0, 128.0]);
        // The ramp rises 32 a cell of 128: the normal leans to -X.
        let length = (0.25_f32 * 0.25 + 1.0).sqrt();
        assert!((point.normal[0] + 0.25 / length).abs() < 1e-6);
        assert!((point.normal[1] - 1.0 / length).abs() < 1e-6 && point.normal[2] == 0.0);
        // Lit fully, the albedo gives back retail's shade of 6/15.
        let lit = point.albedo * (point.normal[1] + AMBIENT_SHARE);
        assert!((lit - 0.4).abs() < 1e-6);
        // The last row repeats the first, 65,536 on.
        assert_eq!(mesh.vertices[512 * 513 + 7].position, [65536.0, 0.0, 896.0]);

        let tile = |i: usize, j: usize| {
            let index = (i * 512 + j) * 2;
            let words = [mesh.triangles[index], mesh.triangles[index + 1]];
            let points: Vec<_> = mesh.indices[index * 3..index * 3 + 6]
                .iter()
                .map(|point| (*point as usize / 513, *point as usize % 513))
                .collect();
            (words, points)
        };
        let slots = |word: u32| {
            [0, 1, 2].map(|n| {
                let place = (word >> (8 + 6 * n)) & 63;
                QUAD_QUARTERS
                    .iter()
                    .position(|&quad| texture_place(quad) == place)
                    .unwrap() as u32
            })
        };
        // A plain tile: rank 0, whose word sends corners (x0,z1), (x0,z0),
        // (x1,z0), (x1,z1) to slots 3, 2, 1, 0. Without the diagonal bit and
        // with the word's bit 0 set, the split is slots 0-1-2 and 0-2-3.
        let plain = terrain_blend_table()[0];
        assert_eq!(
            (plain.texture_rank, plain.orientation_word),
            (0, 0x0011_2233)
        );
        let (words, points) = tile(10, 10);
        assert_eq!(words.map(|word| word & 0xff), [0, 0]);
        assert_eq!(words.map(slots), [[0, 1, 2], [0, 2, 3]]);
        assert_eq!(
            points,
            [(11, 11), (11, 10), (10, 10), (11, 11), (10, 10), (10, 11)]
        );
        // The diagonal bit on the tile's (x0, z0) cell flips the split.
        let (words, _) = tile(20, 30);
        assert_eq!(words.map(slots), [[1, 2, 3], [1, 3, 0]]);
        // The plain tile's triangles share its (x0, z0)-(x1, z1) diagonal,
        // the flipped one's the other.
        let rising = mesh.rising_diagonals();
        let bit = |i: usize, j: usize| rising[(i * 512 + j) / 32] >> ((i * 512 + j) % 32) & 1;
        assert_eq!((bit(10, 10), bit(20, 30)), (1, 0));
        // The four tiles around the material-1 cell take the blends of their
        // corner combos, in the raster's corner order.
        for (i, j, corner) in [(3, 4, 0), (3, 5, 1), (2, 5, 2), (2, 4, 3)] {
            let blend = terrain_blend_table()[1 << (corner * 2)];
            let (words, _) = tile(i, j);
            assert_eq!(
                words.map(|word| word & 0xff),
                [u32::from(blend.texture_rank); 2]
            );
        }
    }

    /// Ultra ground detail: the fine grid's points, each tile's fine cells
    /// split as the game's ground splits the tile, their corners at the
    /// tile's texture places between its corners'.
    #[test]
    fn ultra_ground_draws_its_fine_grid() {
        // The first test's ramp, with the alternate diagonal on tile (20, 30).
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
            let diagonal = u16::from((x, z) == (20, 30)) << 11;
            6 << 12 | diagonal | (x % 64) as u16
        });
        let mut terrain =
            TerrainRuntime::from_raw_cells(sampling(TerrainSpacing::Units128), 0, cells);
        terrain.set_ultra(Some(crate::terrain_runtime::ultra::UltraShape {
            crease: 40.0,
            fill: 256.0,
            ease: 192.0,
        }));
        let mesh = TerrainMesh::build(&terrain, [0.0, 1.0, 0.0]);
        assert_eq!((mesh.points, mesh.spacing), (1025, 64.0));
        assert_eq!(mesh.triangles.len(), 2 * 1024 * 1024);
        // Retail's point (8, 1) is fine point (16, 2), at retail's height.
        assert_eq!(
            mesh.vertices[16 * 1025 + 2].position,
            [1024.0, 256.0, 128.0]
        );
        let ultra = terrain.ultra().unwrap();
        assert_eq!(mesh.vertices[17 * 1025 + 3].position[1], ultra.point(17, 3));
        // A tile's eight triangles in order; each corner's fine point and
        // texture place.
        let corners = |tile: usize, n: usize| -> Vec<((usize, usize), [u32; 2])> {
            let index = tile * 8 + n;
            (0..3)
                .map(|k| {
                    let point = mesh.indices[index * 3 + k] as usize;
                    let place = (mesh.triangles[index] >> (8 + 6 * k)) & 63;
                    ((point / 1025, point % 1025), [place & 7, place >> 3])
                })
                .collect()
        };
        // Plain tile (10, 10): its corners X0Z0, X1Z0, X1Z1, X0Z1 take slots
        // 2, 1, 0, 3, so places (4,4), (4,0), (0,0), (0,4). Its first fine
        // cell, on the main diagonal, wound as the raster's tiles are.
        assert_eq!(
            corners(10 * 512 + 10, 0),
            [((21, 21), [2, 2]), ((21, 20), [4, 2]), ((20, 20), [4, 4])]
        );
        assert_eq!(
            corners(10 * 512 + 10, 1),
            [((21, 21), [2, 2]), ((20, 20), [4, 4]), ((20, 21), [2, 4])]
        );
        // Tile (20, 30)'s fine cells split on the other diagonal.
        let split = corners(20 * 512 + 30, 0);
        assert_eq!(
            split.iter().map(|(point, _)| *point).collect::<Vec<_>>(),
            [(40, 60), (40, 61), (41, 60)]
        );
        // The fine cells' diagonals, a bit each.
        let rising = mesh.rising_diagonals();
        let bit = |x: usize, z: usize| rising[(x * 1024 + z) / 32] >> ((x * 1024 + z) % 32) & 1;
        assert_eq!((bit(20, 20), bit(40, 60)), (1, 0));
    }

    /// Every blend's corner slots, where the raster draws the tile, are a
    /// turn or a flip of the square: neighbouring corners take neighbouring
    /// slots, all one way round. So a place blended between the corners is
    /// the affine map the raster's two triangles draw, as Ultra's fine
    /// cells take it.
    #[test]
    fn the_tiles_orientations_turn_or_flip_the_square() {
        let mut drawn = 0;
        for blend in terrain_blend_table() {
            let slots: [u32; 4] = std::array::from_fn(|k| (blend.orientation_word >> (k * 8)) & 15);
            let mut sorted = slots;
            sorted.sort_unstable();
            if sorted != [0, 1, 2, 3] {
                continue;
            }
            drawn += 1;
            let steps: Vec<u32> = (0..4)
                .map(|k| (slots[(k + 1) % 4] + 4 - slots[k]) % 4)
                .collect();
            assert!(
                steps == [1; 4] || steps == [3; 4],
                "{:#010x}",
                blend.orientation_word
            );
        }
        assert!(drawn > 0);
    }

    #[test]
    fn unlit_ground_under_the_water_takes_the_dry_grounds_albedo() {
        // Dry cells at byte 2 with a baked shade of 6; below X 8 a basin at
        // byte -2 with no shade, under the plane at 0.
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            if index / TERRAIN_GRID_SIDE < 8 {
                u16::from((-2_i8) as u8)
            } else {
                6 << 12 | 2
            }
        });
        let terrain = TerrainRuntime::from_raw_cells(sampling(TerrainSpacing::Units128), 0, cells);
        let mesh = TerrainMesh::build(&terrain, [0.0, 1.0, 0.0]);
        // Flat dry ground under an overhead sun: 6/15 over 1 + 1/4.
        let dry = mesh.vertices[100 * 513 + 5].albedo;
        assert!((dry - 0.4 / 1.25).abs() < 1e-6);
        assert_eq!(mesh.vertices[3 * 513 + 5].albedo, dry);
    }

    /// The mist's base: the height a fifth of the grid lies below, the water
    /// counting at its surface.
    #[test]
    fn the_low_ground_counts_the_water_at_its_surface() {
        // A basin at byte -4 under the plane at 0 for the first 128 rows
        // (a quarter of the grid), then ground rising a byte every 64 rows.
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            let x = index / TERRAIN_GRID_SIDE;
            if x < 128 {
                u16::from((-4_i8) as u8)
            } else {
                (x / 64) as u16
            }
        });
        let terrain = TerrainRuntime::from_raw_cells(sampling(TerrainSpacing::Units128), 0, cells);
        let mesh = TerrainMesh::build(&terrain, [0.0, 1.0, 0.0]);
        assert_eq!((mesh.points, mesh.spacing, mesh.plane), (513, 128.0, 0.0));
        // The basin's floor lies at -128, but its water stands at 0.
        assert_eq!(mesh.vertices[5 * 513].position[1], -128.0);
        assert_eq!(mesh.low, 0.0);
        // Four fifths of the land (rows 128 to 512) lie below row 436's
        // ground, byte 6.
        assert_eq!(mesh.high, 6.0 * 32.0);
        // Three quarters of sea: the high ground is still the land's, row
        // 487's byte 7, where counting the sea's surface gave row 410's 6.
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            let x = index / TERRAIN_GRID_SIDE;
            if x < 384 {
                u16::from((-4_i8) as u8)
            } else {
                (x / 64) as u16
            }
        });
        let terrain = TerrainRuntime::from_raw_cells(sampling(TerrainSpacing::Units128), 0, cells);
        assert_eq!(
            TerrainMesh::build(&terrain, [0.0, 1.0, 0.0]).high,
            7.0 * 32.0
        );
    }

    #[test]
    fn coarse_ground_uses_every_other_cell_and_the_cache_follows_changes() {
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| (index % 7) as u16);
        let mut terrain =
            TerrainRuntime::from_raw_cells(sampling(TerrainSpacing::Units256), 0, cells);
        let sun = [0.6, 0.8, 0.0];
        let mesh = terrain_mesh(&terrain, sun);
        assert_eq!(mesh.vertices.len(), 257 * 257);
        assert_eq!(mesh.triangles.len(), 2 * 256 * 256);
        // Point (1, 3) reads cell (2, 6): index 2*512+6, byte 1030 % 7 = 1.
        assert_eq!(mesh.vertices[257 + 3].position, [256.0, 32.0, 768.0]);
        assert!(Rc::ptr_eq(&mesh, &terrain_mesh(&terrain, sun)));
        terrain.write_raw_height_128([256, 768], 64);
        let rebuilt = terrain_mesh(&terrain, sun);
        assert!(rebuilt.generation > mesh.generation);
        assert_eq!(rebuilt.vertices[257 + 3].position[1], 64.0);
        // A new light is a new mesh too.
        assert!(!Rc::ptr_eq(
            &rebuilt,
            &terrain_mesh(&terrain, [0.0, 1.0, 0.0])
        ));
    }

    /// A strip of cells raised a step (a bridge rising) moves the mesh's
    /// points as a whole build would, across the wrap and under the water,
    /// and names every tile whose triangles moved; a changed material
    /// builds it whole.
    #[test]
    fn heights_alone_patch_the_mesh() {
        for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
            // A basin under the plane below X 8, unshaded; dry ground shaded
            // 6 and rolling past it.
            let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
                let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
                if x < 8 {
                    u16::from((-2_i8) as u8)
                } else {
                    6 << 12 | ((x + z) % 5) as u16
                }
            });
            let mut terrain = TerrainRuntime::from_raw_cells(sampling(spacing), 0, cells);
            let sun = [0.6, 0.8, 0.0];
            let before = TerrainMesh::build(&terrain, sun);
            for x in (0..12).chain(508..512) {
                for z in 300..304 {
                    terrain.write_raw_height_128([x << 7, z << 7], 64 + 32 * (x as i32 % 3));
                }
            }
            let patched = before.patched(&terrain, sun).expect("heights alone");
            let whole = TerrainMesh::build(&terrain, sun);
            assert_eq!(patched.vertices, whole.vertices, "{spacing:?}");
            let (previous, moved) = patched.changed.clone().expect("a patch");
            assert_eq!(previous, before.generation);
            let tiles = crate::scene_ground_detail::tile_ground(&whole);
            let mut first = 0;
            for (tile, &[_, count]) in tiles.ranges.iter().enumerate() {
                let triangles = &tiles.indices[first as usize * 3..(first + count) as usize * 3];
                first += count;
                if triangles
                    .iter()
                    .any(|&point| before.vertices[point as usize] != whole.vertices[point as usize])
                {
                    assert!(moved.contains(&tile), "{spacing:?} tile {tile}");
                }
            }
            assert!(Rc::ptr_eq(&patched.indices, &before.indices));
            // A water bridge's plane: its basin covered less, then bare.
            for plane in [-32, -100] {
                terrain.set_plane_base(plane << terrain.plane_shift());
                let patched = whole.patched(&terrain, sun).expect("the water alone");
                let rebuilt = TerrainMesh::build(&terrain, sun);
                assert_eq!(patched.vertices, rebuilt.vertices, "{spacing:?} {plane}");
                assert_eq!(
                    [patched.plane, patched.low, patched.high],
                    [rebuilt.plane, rebuilt.low, rebuilt.high]
                );
                assert_eq!(
                    patched.changed.as_ref().map(|(_, moved)| moved.len()),
                    Some(0)
                );
            }
            let whole = TerrainMesh::build(&terrain, sun);
            // A cell's material is more than its height.
            let raw = terrain.cells()[5 * TERRAIN_GRID_SIDE + 5].raw();
            let mut cells: Vec<u16> = terrain.cells().iter().map(|cell| cell.raw()).collect();
            cells[5 * TERRAIN_GRID_SIDE + 5] = raw ^ 0x100;
            let recut = TerrainRuntime::from_raw_cells(sampling(spacing), 0, cells);
            assert!(whole.patched(&recut, sun).is_none());
        }
    }
}
