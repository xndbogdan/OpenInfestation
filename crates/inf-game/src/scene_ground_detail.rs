//! Port option, Vulkan only: the traced ground's detail round the camera
//! ([the plan](../../../docs/rt-plans/ground-detail.md)), RDR2's approach.
//! The ground's mesh ([`crate::scene_terrain`]) is cut into tiles of 4,096
//! units, each its own bottom level; the 3 by 3 tiles round the eye's give
//! way to a patch whose cells are split 4 by 4, retail's triangles exactly,
//! less what the remaster carves into them: the rain's puddles, sunk under
//! their water (2026-10-26: the ground carves inwards slightly
//! and the puddles have volume). The carve follows `pt_trace.comp`'s own
//! puddles and lake shore, ported here, so the hollows lie where the tracer
//! shades their water. OpenGL and the software renderer never see it.

use crate::scene_terrain::{TerrainMesh, TerrainMeshVertex};

/// A tile's side in world units, and the tiles along the wrap's side.
pub const TILE_UNITS: f32 = 4096.0;
pub const TILES: usize = 16;
/// The patch's side in tiles: the eye's and one on every side, so the eye
/// is at least a tile from its edge.
pub const PATCH_TILES: usize = 3;
/// How many times the patch splits a cell each way: 32-unit points on
/// High's 128-unit cells. A tile corner's texture place is kept in quarters
/// (`scene_terrain::texture_place`), so 4 is the most a cell takes.
pub const SPLIT: usize = 4;
/// How deep a full puddle lies under its water, in world units: 6, a
/// third more than the 4 first carved (2026-10-29: puddles
/// should be a little deeper, so the ACAM's wheel visibly goes in deeper); 10 made pits, 7 read as
/// 4 did.
pub const PUDDLE_DEPTH: f32 = 6.0;
/// Where `pt_ground.glsl`'s puddles start (`puddle_at`'s share at their
/// shore) and where their water is whole (its `puddle_water`): the bed
/// sinks from nothing at the one to the full depth at the other, so it lies
/// at its depth under all the whole water, not only at the puddle's middle.
const PUDDLE_EDGE: f32 = 0.45;
const WATER_WHOLE: f32 = 0.7;
/// How many times the carve is smoothed over its neighbours, so a bank
/// slopes over a few points rather than dropping at one.
const SMOOTHING: usize = 2;
/// Over how much of the patch's border the carve fades to nothing, so the
/// patch meets the tiles round it.
const EDGE_FADE: f32 = 1024.0;
const WRAP: f64 = 65536.0;
/// `pt_trace.comp`'s band above the lake's edge in which a puddle takes the
/// lake's look (and here, its flat bed).
const PUDDLE_SHORE_BAND: f32 = 160.0;

/// The traced ground's triangles by tile: X-major tiles, each tile's
/// triangles together in the ground's order, and each tile's first triangle
/// and count.
#[derive(Debug)]
pub struct GroundTiles {
    pub generation: u64,
    pub indices: Vec<u32>,
    pub triangles: Vec<u32>,
    pub ranges: Vec<[u32; 2]>,
}

/// `mesh`'s triangles sorted into tiles by where their middle lies.
pub fn tile_ground(mesh: &TerrainMesh) -> GroundTiles {
    let count = mesh.triangles.len();
    let tile_of = |triangle: usize| {
        let middle = [0, 2].map(|axis| {
            (0..3)
                .map(|k| mesh.vertices[mesh.indices[triangle * 3 + k] as usize].position[axis])
                .sum::<f32>()
                / 3.0
        });
        let [x, z] = middle.map(|v| ((v / TILE_UNITS).max(0.0) as usize).min(TILES - 1));
        x * TILES + z
    };
    let tiles: Vec<usize> = (0..count).map(tile_of).collect();
    let mut ranges = vec![[0u32; 2]; TILES * TILES];
    for &tile in &tiles {
        ranges[tile][1] += 1;
    }
    let mut first = 0;
    for range in &mut ranges {
        range[0] = first;
        first += range[1];
    }
    let mut next: Vec<u32> = ranges.iter().map(|range| range[0]).collect();
    let mut indices = vec![0; count * 3];
    let mut triangles = vec![0; count];
    for (triangle, &tile) in tiles.iter().enumerate() {
        let at = next[tile] as usize;
        next[tile] += 1;
        triangles[at] = mesh.triangles[triangle];
        indices[at * 3..at * 3 + 3].copy_from_slice(&mesh.indices[triangle * 3..triangle * 3 + 3]);
    }
    GroundTiles {
        generation: mesh.generation,
        indices,
        triangles,
        ranges,
    }
}

/// The first (south-west) tile of the patch round world X/Z `eye`, in tiles
/// counted without the wrap.
pub fn patch_first(eye: [f64; 2]) -> [i64; 2] {
    eye.map(|axis| (axis / f64::from(TILE_UNITS)).floor() as i64 - 1)
}

/// The tiles the patch whose first tile is `first` takes the place of, as
/// X-major tile numbers.
pub fn patch_tiles(first: [i64; 2]) -> [usize; PATCH_TILES * PATCH_TILES] {
    std::array::from_fn(|k| {
        let along =
            |first: i64, step: usize| (first + step as i64).rem_euclid(TILES as i64) as usize;
        along(first[0], k / PATCH_TILES) * TILES + along(first[1], k % PATCH_TILES)
    })
}

/// Where tile `tile` (X-major) lies from the eye at world `eye`: its copy
/// across the wrap nearest the eye, so the tiles cover the world round the
/// eye once.
pub fn tile_translation(tile: usize, eye: [f64; 3]) -> [f32; 3] {
    let middle = [tile / TILES, tile % TILES].map(|at| (at as f64 + 0.5) * f64::from(TILE_UNITS));
    let shift = |axis: usize, middle: f64| {
        let copies = ((eye[axis] - middle) / WRAP).round();
        (copies * WRAP - eye[axis]) as f32
    };
    [shift(0, middle[0]), -eye[1] as f32, shift(2, middle[1])]
}

/// Where the patch whose first tile is `first` lies from the eye at `eye`
/// (its points are counted from its south-west corner): its copy across the
/// wrap nearest the eye, as for the tiles it takes the place of. The patch
/// drawn was built for an earlier eye, which may lie across the wrap's seam
/// (Tabuchi's start is 140 units from it): counted without the wrap it lay a
/// world away while its tiles stayed out, and the ground round the eye went
/// missing until the next patch came (2026-10-29).
pub fn patch_translation(first: [i64; 2], eye: [f64; 3]) -> [f32; 3] {
    let shift = |axis: usize, first: i64| {
        let origin = first as f64 * f64::from(TILE_UNITS);
        let middle = origin + PATCH_TILES as f64 * f64::from(TILE_UNITS) / 2.0;
        let copies = ((eye[axis] - middle) / WRAP).round();
        (origin + copies * WRAP - eye[axis]) as f32
    };
    [shift(0, first[0]), -eye[1] as f32, shift(2, first[1])]
}

/// What the carve follows: how far the puddles have filled (the weather's
/// share, 0 to 1) and the world's lake, where it has one that shows
/// shores.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarveLook {
    pub fill: f32,
    pub shore: Option<ShoreLook>,
}

impl CarveLook {
    /// What the carve follows on a world whose water's plane is `plane`
    /// (-4096 or below: none, as retail tests) and whose water is tuned
    /// `water`, its puddles filled `fill`; none while none stand. The tracer
    /// and the wheels take the same.
    pub fn for_world(
        fill: f32,
        plane: f32,
        water: &crate::modern_tuning::WaterTuning,
    ) -> Option<Self> {
        (fill > 0.0).then(|| Self {
            fill: fill.min(1.0),
            shore: (plane > -4096.0 && water.glow <= 0.0).then(|| ShoreLook {
                level: plane,
                fade: water.shore.max(0.0),
                wobble: water.wobble.max(0.0),
            }),
        })
    }
}

/// The lake's plane and its shore's tuning (the water's `shore` and
/// `wobble`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShoreLook {
    pub level: f32,
    pub fade: f32,
    pub wobble: f32,
}

/// A ground's grid of heights, as the tracer's ground reads them.
pub trait GridHeights {
    /// Points a side and their spacing.
    fn points(&self) -> usize;
    fn spacing(&self) -> f32;
    fn height(&self, i: usize, j: usize) -> f32;
}

/// What a patch is built from, owned so a worker thread can build it: the
/// ground's points (grid order) and its triangles by tile.
#[derive(Debug)]
pub struct DetailGround {
    pub generation: u64,
    pub points: usize,
    pub spacing: f32,
    pub vertices: Vec<TerrainMeshVertex>,
    pub tiles: GroundTiles,
}

impl DetailGround {
    pub fn new(mesh: &TerrainMesh) -> Self {
        Self {
            generation: mesh.generation,
            points: mesh.points as usize,
            spacing: mesh.spacing,
            vertices: mesh.vertices.clone(),
            tiles: tile_ground(mesh),
        }
    }
}

impl GridHeights for DetailGround {
    fn points(&self) -> usize {
        self.points
    }
    fn spacing(&self) -> f32 {
        self.spacing
    }
    fn height(&self, i: usize, j: usize) -> f32 {
        self.vertices[i * self.points + j].position[1]
    }
}

impl GridHeights for TerrainMesh {
    fn points(&self) -> usize {
        self.points as usize
    }
    fn spacing(&self) -> f32 {
        self.spacing
    }
    fn height(&self, i: usize, j: usize) -> f32 {
        self.vertices[i * self.points as usize + j].position[1]
    }
}

fn smoothstep(low: f32, high: f32, x: f32) -> f32 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `pt_trace.comp`'s `hash`.
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// `pt_trace.comp`'s `wrap_noise`: value noise over world X/Z, 0 to 1,
/// `period` lattice cells across the wrap.
fn wrap_noise(world: [f32; 2], period: i32, layer: u32) -> f32 {
    let p = world.map(|axis| axis.rem_euclid(65536.0) * (period as f32 / 65536.0));
    let cell = p.map(f32::floor);
    let f = [0, 1].map(|axis| {
        let f = p[axis] - cell[axis];
        f * f * (3.0 - 2.0 * f)
    });
    let c = cell.map(|axis| axis as i32);
    let n: [f32; 4] = std::array::from_fn(|k| {
        let corner = [
            (c[0] + (k as i32 & 1)) % period,
            (c[1] + (k as i32 >> 1)) % period,
        ];
        let h = hash(
            (corner[0] as u32).wrapping_add(hash((corner[1] as u32).wrapping_add(hash(layer)))),
        );
        (h >> 8) as f32 / 16_777_216.0
    });
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    mix(mix(n[0], n[1], f[0]), mix(n[2], n[3], f[0]), f[1])
}

/// `pt_trace.comp`'s `ground_height`: bilinear between the grid's points.
pub fn ground_height(grid: &impl GridHeights, world: [f32; 2]) -> f32 {
    let points = grid.points();
    let at = world.map(|axis| axis.rem_euclid(65536.0) / grid.spacing());
    let corner = at.map(|axis| (axis.floor().max(0.0) as usize).min(points - 2));
    let t = [0, 1].map(|axis| (at[axis] - corner[axis] as f32).clamp(0.0, 1.0));
    let [i, j] = corner;
    let near = grid.height(i, j) + (grid.height(i + 1, j) - grid.height(i, j)) * t[0];
    let far = grid.height(i, j + 1) + (grid.height(i + 1, j + 1) - grid.height(i, j + 1)) * t[0];
    near + (far - near) * t[1]
}

/// `pt_trace.comp`'s `puddle_at`: 0 to 1 at world X/Z on ground whose
/// normal rises `normal_y`, the puddles filled `fill`.
pub fn puddle_at(grid: &impl GridHeights, world: [f32; 2], normal_y: f32, fill: f32) -> f32 {
    if fill <= 0.0 || normal_y < 0.94 || grid.points() < 2 {
        return 0.0;
    }
    let around = (0..6)
        .map(|k| {
            let a = k as f32 * 1.047_197_6;
            ground_height(
                grid,
                [world[0] + 320.0 * a.cos(), world[1] + 320.0 * a.sin()],
            )
        })
        .sum::<f32>()
        / 6.0;
    let hollow = around - ground_height(grid, world);
    let n = 0.65 * wrap_noise(world, 160, 0x50) + 0.35 * wrap_noise(world, 448, 0x51);
    let filled = hollow / 32.0 + (n - 0.5) * 0.6;
    let level = 1.1 - 0.9 * fill;
    smoothstep(level, level + 0.25, filled)
        * smoothstep(0.94, 0.985, normal_y)
        * smoothstep(0.0, 0.15, fill)
}

/// `pt_trace.comp`'s `water_near`.
fn water_near(grid: &impl GridHeights, world: [f32; 2], level: f32) -> f32 {
    let spacing = grid.spacing();
    let base = world.map(|axis| ((axis / spacing).floor() - 1.0) * spacing);
    let mut nearest = 4.0 * spacing;
    for k in 0..16 {
        let corner = [
            base[0] + (k & 3) as f32 * spacing,
            base[1] + (k >> 2) as f32 * spacing,
        ];
        if ground_height(grid, corner) < level {
            let d = [corner[0] - world[0], corner[1] - world[1]];
            nearest = nearest.min((d[0] * d[0] + d[1] * d[1]).sqrt());
        }
    }
    1.0 - smoothstep(spacing, 2.0 * spacing, nearest)
}

/// `pt_trace.comp`'s `puddle_shore`: how far a puddle at world X/Z and
/// height `y` (its uncarved ground) has become the lake's shore.
fn puddle_shore(grid: &impl GridHeights, world: [f32; 2], y: f32, shore: &ShoreLook) -> f32 {
    let wander = (wrap_noise(world, 512, 11) - 0.5) * 2.0 * shore.wobble;
    let above = y - shore.level - wander;
    if above >= PUDDLE_SHORE_BAND {
        return 0.0;
    }
    (1.0 - smoothstep(0.0, PUDDLE_SHORE_BAND, above)) * water_near(grid, world, shore.level)
}

/// How far the ground at world X/Z sinks under a puddle's water, its
/// uncarved height `y` and normal rising `normal_y`: none at the puddle's
/// shore, the full depth where its water is whole, none where the lake's
/// shore takes the puddle.
pub fn carve_at(
    grid: &impl GridHeights,
    world: [f32; 2],
    y: f32,
    normal_y: f32,
    look: &CarveLook,
) -> f32 {
    // `pt_trace.comp`'s puddle_at: no puddle on the lake's bed, under its
    // plane.
    if look.shore.as_ref().is_some_and(|shore| y < shore.level) {
        return 0.0;
    }
    let puddle = puddle_at(grid, world, normal_y, look.fill);
    if puddle <= PUDDLE_EDGE {
        return 0.0;
    }
    let depth = PUDDLE_DEPTH * smoothstep(PUDDLE_EDGE, WATER_WHOLE, puddle);
    match &look.shore {
        Some(shore) => depth * (1.0 - puddle_shore(grid, world, y, shore)),
        None => depth,
    }
}

/// The ground's normal at world X/Z as its grid points' normals give it
/// (central differences, as `scene_terrain`'s), blended between them.
fn grid_normal_y(grid: &impl GridHeights, world: [f32; 2]) -> f32 {
    let points = grid.points();
    let cells = points - 1;
    let spacing = grid.spacing();
    let normal = |i: usize, j: usize| -> [f32; 3] {
        let [west, east] = [(i + cells - 1) % cells, (i + 1) % cells];
        let [south, north] = [(j + cells - 1) % cells, (j + 1) % cells];
        let slope = [
            (grid.height(east, j) - grid.height(west, j)) / (2.0 * spacing),
            (grid.height(i, north) - grid.height(i, south)) / (2.0 * spacing),
        ];
        [-slope[0], 1.0, -slope[1]]
    };
    let at = world.map(|axis| axis.rem_euclid(65536.0) / spacing);
    let corner = at.map(|axis| (axis.floor().max(0.0) as usize).min(points - 2));
    let t = [0, 1].map(|axis| (at[axis] - corner[axis] as f32).clamp(0.0, 1.0));
    let [i, j] = corner;
    let blend: [f32; 3] = std::array::from_fn(|axis| {
        let near = normal(i, j)[axis] * (1.0 - t[0]) + normal(i + 1, j)[axis] * t[0];
        let far = normal(i, j + 1)[axis] * (1.0 - t[0]) + normal(i + 1, j + 1)[axis] * t[0];
        near * (1.0 - t[1]) + far * t[1]
    });
    unit(blend)[1]
}

/// How much of the ground at world X/Z standing water covers, 0 to 1, as
/// the puddles' shading lays it (`pt_ground.glsl`'s `puddle_water`): none
/// off a puddle, on steep ground or on the lake's bed under its plane.
pub fn water_at(grid: &impl GridHeights, world: [f32; 2], look: &CarveLook) -> f32 {
    if look.fill <= 0.0 || grid.points() < 2 {
        return 0.0;
    }
    if look
        .shore
        .as_ref()
        .is_some_and(|shore| ground_height(grid, world) < shore.level)
    {
        return 0.0;
    }
    let puddle = puddle_at(grid, world, grid_normal_y(grid, world), look.fill);
    smoothstep(PUDDLE_EDGE, WATER_WHOLE, puddle)
}

/// The carve at a point of the fine grid at world X/Z, from the ground's
/// grid alone (its height and normal there), before smoothing: what the
/// patch's points and the wheels' bed both start from.
pub fn point_carve(grid: &impl GridHeights, world: [f32; 2], look: &CarveLook) -> f32 {
    if look.fill <= 0.0 {
        return 0.0;
    }
    let normal_y = grid_normal_y(grid, world);
    if normal_y < 0.94 {
        return 0.0;
    }
    carve_at(grid, world, ground_height(grid, world), normal_y, look)
}

/// The smoothing's weights along an axis: 1-2-1 twice.
const SMOOTH_TAPS: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];

/// How far the puddles' bed lies below the ground at world X/Z, as the
/// patch draws it: the fine grid's points (`spacing / SPLIT` apart, on the
/// world's own lattice, so where the camera is changes nothing), smoothed as
/// the patch smooths them, blended between. The wheels stand on it (the
/// user, 2026-10-26: the wheels should "go in").
pub fn bed_carve(grid: &impl GridHeights, world: [f32; 2], look: &CarveLook) -> f32 {
    if look.fill <= 0.0 {
        return 0.0;
    }
    let fine = grid.spacing() / SPLIT as f32;
    let at = world.map(|axis| axis.rem_euclid(65536.0) / fine);
    let base = at.map(f32::floor);
    let t = [at[0] - base[0], at[1] - base[1]];
    // The raw carve over the 6 by 6 points the four corners' taps reach.
    let mut raw = [[0.0f32; 6]; 6];
    for (a, row) in raw.iter_mut().enumerate() {
        for (b, value) in row.iter_mut().enumerate() {
            let point = [
                (base[0] + a as f32 - 2.0) * fine,
                (base[1] + b as f32 - 2.0) * fine,
            ];
            *value = point_carve(grid, point, look);
        }
    }
    if raw.iter().flatten().all(|&value| value == 0.0) {
        return 0.0;
    }
    let smooth = |a: usize, b: usize| -> f32 {
        (0..5)
            .map(|i| {
                (0..5)
                    .map(|j| SMOOTH_TAPS[i] * SMOOTH_TAPS[j] * raw[a + i][b + j])
                    .sum::<f32>()
            })
            .sum()
    };
    let [s00, s10, s01, s11] = [smooth(0, 0), smooth(1, 0), smooth(0, 1), smooth(1, 1)];
    let near = s00 + (s10 - s00) * t[0];
    let far = s01 + (s11 - s01) * t[0];
    near + (far - near) * t[1]
}

/// A point of the patch: its place from the patch's south-west corner (the
/// height is the world's), its normal, the albedo as the ground's
/// ([`TerrainMeshVertex`]), how far it is carved below the ground, and the
/// uncarved ground's normal.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DetailVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub albedo: f32,
    pub carve: f32,
    pub flat: [f32; 3],
}

/// What a patch was built from: the ground's mesh, its first tile, and what
/// it carved (the fill kept in 64ths, so a soaking ground builds it again
/// only every step).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchKey {
    pub generation: u64,
    pub first: [i64; 2],
    pub look: CarveLook,
}

impl PatchKey {
    pub fn new(generation: u64, first: [i64; 2], mut look: CarveLook) -> Self {
        look.fill = (look.fill.clamp(0.0, 1.0) * 64.0).round() / 64.0;
        Self {
            generation,
            first,
            look,
        }
    }
}

/// The patch: its points (`points` a side, X-major, `fine` apart), and its
/// triangles' indices and words (the ground's: blend rank, then each
/// corner's texture place).
#[derive(Debug)]
pub struct DetailPatch {
    pub key: PatchKey,
    pub fine: f32,
    pub points: usize,
    pub vertices: Vec<DetailVertex>,
    pub indices: Vec<u32>,
    pub triangles: Vec<u32>,
}

/// The patch `key` names over `ground`: each of the patch's tiles'
/// triangles split `SPLIT` by `SPLIT` on its own plane, its texture places
/// and shade blended across it, then carved.
pub fn build_patch(ground: &DetailGround, key: PatchKey) -> DetailPatch {
    let tiles = &ground.tiles;
    let n = SPLIT;
    let fine = ground.spacing / n as f32;
    let cells = (PATCH_TILES as f32 * TILE_UNITS / fine).round() as usize;
    let points = cells + 1;
    let mut vertices = vec![DetailVertex::default(); points * points];
    let mut used = vec![false; points * points];
    let mut indices = Vec::new();
    let mut triangles = Vec::new();
    for (k, tile) in patch_tiles(key.first).into_iter().enumerate() {
        let [a, b] = [k / PATCH_TILES, k % PATCH_TILES];
        let shift = [
            (a as f32 - (tile / TILES) as f32) * TILE_UNITS,
            (b as f32 - (tile % TILES) as f32) * TILE_UNITS,
        ];
        let [first, count] = tiles.ranges[tile];
        for triangle in first as usize..(first + count) as usize {
            let corner = |k: usize| &ground.vertices[tiles.indices[triangle * 3 + k] as usize];
            let corners = [corner(0), corner(1), corner(2)];
            let grid = corners.map(|v| {
                [
                    ((v.position[0] + shift[0]) / fine).round() as i64,
                    ((v.position[2] + shift[1]) / fine).round() as i64,
                ]
            });
            let word = tiles.triangles[triangle];
            let places = [0, 1, 2].map(|k| {
                let place = (word >> (8 + 6 * k)) & 63;
                [place & 7, place >> 3]
            });
            let at = |i: usize, j: usize| -> Option<usize> {
                let [gx, gz] = [0, 1].map(|axis| {
                    grid[0][axis]
                        + (grid[1][axis] - grid[0][axis]) * i as i64 / n as i64
                        + (grid[2][axis] - grid[0][axis]) * j as i64 / n as i64
                });
                (0..points as i64)
                    .contains(&gx)
                    .then_some(())
                    .filter(|_| (0..points as i64).contains(&gz))
                    .map(|_| gx as usize * points + gz as usize)
            };
            // Every point of the split: on the triangle's plane, its normal
            // and shade blended as the coarse ground's are.
            let mut inside = true;
            for i in 0..=n {
                for j in 0..=n - i {
                    let Some(index) = at(i, j) else {
                        inside = false;
                        continue;
                    };
                    if used[index] {
                        continue;
                    }
                    used[index] = true;
                    let w = [(n - i - j) as f32, i as f32, j as f32].map(|w| w / n as f32);
                    let blend = |value: &dyn Fn(&TerrainMeshVertex) -> f32| {
                        (0..3).map(|k| w[k] * value(corners[k])).sum::<f32>()
                    };
                    let normal = unit([0, 1, 2].map(|axis| blend(&|v| v.normal[axis])));
                    vertices[index] = DetailVertex {
                        position: [
                            (index / points) as f32 * fine,
                            blend(&|v| v.position[1]),
                            (index % points) as f32 * fine,
                        ],
                        normal,
                        albedo: blend(&|v| v.albedo),
                        carve: 0.0,
                        flat: normal,
                    };
                }
            }
            if !inside {
                continue;
            }
            let place = |i: usize, j: usize| {
                let [u, v] = [0, 1].map(|axis| {
                    (places[0][axis] * (n - i - j) as u32
                        + places[1][axis] * i as u32
                        + places[2][axis] * j as u32)
                        / n as u32
                });
                u | v << 3
            };
            let mut push = |corners: [(usize, usize); 3]| {
                let mut word = word & 0xff;
                for (k, &(i, j)) in corners.iter().enumerate() {
                    indices.push(at(i, j).expect("inside") as u32);
                    word |= place(i, j) << (8 + 6 * k);
                }
                triangles.push(word);
            };
            for i in 0..n {
                for j in 0..n - i {
                    push([(i, j), (i + 1, j), (i, j + 1)]);
                    if i + j + 1 < n {
                        push([(i + 1, j), (i + 1, j + 1), (i, j + 1)]);
                    }
                }
            }
        }
    }
    carve(ground, &mut vertices, &used, points, fine, &key);
    DetailPatch {
        key,
        fine,
        points,
        vertices,
        indices,
        triangles,
    }
}

/// Sink the patch's points under the puddles (`point_carve`, smoothed as
/// `bed_carve` smooths it, so the wheels meet the bed drawn), fading toward
/// its border, and turn their normals by the carve's slope.
fn carve(
    ground: &DetailGround,
    vertices: &mut [DetailVertex],
    used: &[bool],
    points: usize,
    fine: f32,
    key: &PatchKey,
) {
    if key.look.fill <= 0.0 {
        return;
    }
    let origin = key.first.map(|tiles| tiles as f64 * f64::from(TILE_UNITS));
    let side = (points - 1) as f32 * fine;
    // Each point's depth, the points shared among the CPU's cores.
    let depth_at = |vertex: &DetailVertex, used: bool| -> f32 {
        if !used {
            return 0.0;
        }
        let [x, z] = [vertex.position[0], vertex.position[2]];
        let border = x.min(z).min(side - x).min(side - z);
        let edge = smoothstep(0.0, EDGE_FADE, border);
        if edge <= 0.0 {
            return 0.0;
        }
        let world = [
            (origin[0] + f64::from(x)).rem_euclid(WRAP) as f32,
            (origin[1] + f64::from(z)).rem_euclid(WRAP) as f32,
        ];
        edge * point_carve(ground, world, &key.look)
    };
    let mut depth = vec![0.0; vertices.len()];
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, 8);
    let chunk = vertices.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for ((out, points), used) in depth
            .chunks_mut(chunk)
            .zip(vertices.chunks(chunk))
            .zip(used.chunks(chunk))
        {
            let depth_at = &depth_at;
            scope.spawn(move || {
                for ((out, vertex), &used) in out.iter_mut().zip(points).zip(used) {
                    *out = depth_at(vertex, used);
                }
            });
        }
    });
    for _ in 0..SMOOTHING {
        depth = smoothed(&depth, used, points);
    }
    let at = |i: usize, j: usize| depth[i * points + j];
    for i in 0..points {
        for j in 0..points {
            let index = i * points + j;
            let [west, east] = [i.saturating_sub(1), (i + 1).min(points - 1)];
            let [south, north] = [j.saturating_sub(1), (j + 1).min(points - 1)];
            let slope = [
                (at(east, j) - at(west, j)) / ((east - west).max(1) as f32 * fine),
                (at(i, north) - at(i, south)) / ((north - south).max(1) as f32 * fine),
            ];
            if !used[index] || (depth[index] == 0.0 && slope == [0.0, 0.0]) {
                continue;
            }
            let vertex = &mut vertices[index];
            let flat = vertex.flat;
            let rise = flat[1].max(1.0e-3);
            vertex.normal = unit([flat[0] / rise + slope[0], 1.0, flat[2] / rise + slope[1]]);
            vertex.position[1] -= depth[index];
            vertex.carve = depth[index];
        }
    }
}

/// depth over the patch's points blurred 1-2-1 each way, unused points
/// and the patch's border kept at nothing.
fn smoothed(depth: &[f32], used: &[bool], points: usize) -> Vec<f32> {
    let pass = |source: &[f32], step: usize| -> Vec<f32> {
        (0..source.len())
            .map(|index| {
                let along = if step == 1 {
                    index % points
                } else {
                    index / points
                };
                if !used[index] || along == 0 || along + 1 == points {
                    return 0.0;
                }
                let side = |at: usize| if used[at] { source[at] } else { source[index] };
                (side(index - step) + 2.0 * source[index] + side(index + step)) / 4.0
            })
            .collect()
    };
    pass(&pass(depth, points), 1)
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1.0e-6);
    v.map(|c| c / length)
}

#[cfg(test)]
#[path = "scene_ground_detail_tests.rs"]
mod tests;
