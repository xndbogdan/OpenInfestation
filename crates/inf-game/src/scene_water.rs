//! The traced world's water (modern mode,
//! [the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#water)): each
//! frame, the water within sight as a mesh at the game's own wave heights
//! (`00410F60`, [`TerrainRuntime::water_surface_height`]), so reflections
//! move in step with what floats on it. A tile is water where retail draws a
//! water tile (`terrain_draw/water.rs`): any corner's ground below the
//! water plane. Where the ground stands above the plane, the height is the
//! plane's and the ground hides it, as in retail.

use crate::{
    scene_feed::{SceneFeed, SceneSurface, SceneTriangle, SceneVertex},
    terrain_runtime::{TerrainRuntime, TerrainWaterSurfaceRequest},
};

/// The water around one frame's eye.
pub struct WaterRequest<'a> {
    pub terrain: &'a TerrainRuntime,
    /// The eye, in world units (X and Z wrap at 16 bits).
    pub eye: [i32; 3],
    /// The wave clock and world flags the raster's water rows read
    /// (flag `0x40` moves the waves).
    pub clock: u32,
    pub world_flags: u32,
    pub sine: &'a [u16; 4096],
    /// How far from the eye to build, in world units.
    pub reach: i32,
    /// The water's texture, an index into the builder's sprites.
    pub texture: Option<u32>,
}

/// Add the water within reach of the eye to `feed`: vertices shared at the
/// grid points, two triangles a water tile, eye-relative like the rest of
/// the feed. True when there was any.
pub fn add_water(feed: &mut SceneFeed, request: WaterRequest<'_>) -> bool {
    let terrain = request.terrain;
    let spacing = i32::from(terrain.sampling().spacing.units());
    let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
    let steps = (request.reach / spacing).clamp(1, 255);
    // Tiles across, and grid points with one more each side for normals.
    let tiles = (2 * steps + 1) as usize;
    let points = tiles + 3;
    let origin = [0, 2].map(|axis| (request.eye[axis].div_euclid(spacing) - steps - 1) * spacing);
    let grid = |i: usize, j: usize| -> [u16; 2] {
        [
            (origin[0] + i as i32 * spacing) as u16,
            (origin[1] + j as i32 * spacing) as u16,
        ]
    };
    // Each point's ground, and whether it lies under the plane: a tile is
    // water where any corner does.
    let mut ground = vec![0_i16; points * points];
    let mut below = vec![false; points * points];
    for i in 0..points {
        for j in 0..points {
            let height = terrain.raw_height_at(grid(i, j));
            ground[i * points + j] = height;
            below[i * points + j] = height < plane;
        }
    }
    let water_tile = |i: usize, j: usize| {
        below[i * points + j]
            || below[(i + 1) * points + j]
            || below[(i + 1) * points + j + 1]
            || below[i * points + j + 1]
    };
    // The waves only where the mesh reads them: the water tiles' corners
    // and the neighbours their normals take (the land's heights went
    // unread, about half the work on a world of islands).
    let mut wanted = vec![false; points * points];
    let mut corners = 0_usize;
    for i in 1..=tiles {
        for j in 1..=tiles {
            if !water_tile(i, j) {
                continue;
            }
            for (ci, cj) in [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)] {
                for (ni, nj) in [
                    (ci, cj),
                    (ci - 1, cj),
                    (ci + 1, cj),
                    (ci, cj - 1),
                    (ci, cj + 1),
                ] {
                    wanted[ni * points + nj] = true;
                }
            }
            corners += 1;
        }
    }
    let mut heights = vec![0_i16; points * points];
    for (index, _) in wanted.iter().enumerate().filter(|(_, wanted)| **wanted) {
        let [i, j] = [index / points, index % points];
        heights[index] = terrain.water_surface_height(
            TerrainWaterSurfaceRequest {
                xz: grid(i, j),
                wave_clock: request.clock,
                world_flags: request.world_flags,
                terrain_height: ground[index],
            },
            request.sine,
        );
    }
    let height = |i: usize, j: usize| f32::from(heights[i * points + j]);
    let units = spacing as f32;
    let first = feed.vertices.len();
    feed.vertices.reserve(corners + 2 * tiles + 1);
    feed.triangles.reserve(2 * corners);
    let mut ids = vec![u32::MAX; points * points];
    // The 16-bit wrap, relative to the eye.
    let relative = |value: u16, eye: i32| i32::from(value).wrapping_sub(eye) as i16;
    for i in 1..=tiles {
        for j in 1..=tiles {
            if !water_tile(i, j) {
                continue;
            }
            let mut place = [0_u32; 4];
            for (k, (ci, cj)) in [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
                .into_iter()
                .enumerate()
            {
                let index = ci * points + cj;
                if ids[index] == u32::MAX {
                    let [x, z] = grid(ci, cj);
                    let slope = [
                        (height(ci + 1, cj) - height(ci - 1, cj)) / (2.0 * units),
                        (height(ci, cj + 1) - height(ci, cj - 1)) / (2.0 * units),
                    ];
                    let length = (slope[0] * slope[0] + slope[1] * slope[1] + 1.0).sqrt();
                    // Water lying on the ground (ground at the plane, or a
                    // trough the game clamps to it) would fight it pixel by
                    // pixel: it is drawn just under. The normals keep the
                    // game's heights.
                    let surface = heights[index];
                    let drawn = if surface <= ground[index] {
                        surface.min(ground[index].saturating_sub(2))
                    } else {
                        surface
                    };
                    ids[index] = feed.vertices.len() as u32;
                    feed.vertices.push(SceneVertex {
                        position: [
                            f32::from(relative(x, request.eye[0])),
                            f32::from(relative(drawn as u16, request.eye[1])),
                            f32::from(relative(z, request.eye[2])),
                        ],
                        normal: [-slope[0] / length, 1.0 / length, -slope[1] / length],
                        uv: [0.0; 2],
                        color: [255; 4],
                        shade: [0; 4],
                        fog: [0; 4],
                        motion: [0.0; 3],
                    });
                }
                place[k] = ids[index];
            }
            let [a, b, c, d] = place;
            for vertices in [[a, b, c], [a, c, d]] {
                feed.triangles.push(SceneTriangle {
                    vertices,
                    texture: request.texture,
                    surface: SceneSurface::Water,
                    model: None,
                    light: None,
                    cutout: false,
                    own_fog: false,
                    object: None,
                    culled: false,
                });
            }
        }
    }
    feed.vertices.len() > first
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_feed::SceneCamera;
    use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
    use inf_formats::TERRAIN_GRID_SIDE;

    #[test]
    fn water_covers_the_tiles_below_the_plane_around_the_eye() {
        // Ground at byte 4 (128 units) except a basin at byte -4 (-128)
        // for cells 10..=11 on X and 20..=21 on Z; the plane at 0.
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
            let basin = (10..=11).contains(&x) && (20..=21).contains(&z);
            u16::from(if basin { (-4_i8) as u8 } else { 4 })
        });
        let terrain = TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale: TerrainHeightScale::Units32,
            },
            0,
            cells,
        );
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera::default());
        let sine = [0_u16; 4096];
        let eye = [10 * 128 + 64, 300, 20 * 128 + 64];
        assert!(add_water(
            &mut feed,
            WaterRequest {
                terrain: &terrain,
                eye,
                clock: 0,
                world_flags: 0,
                sine: &sine,
                reach: 1024,
                texture: Some(7),
            },
        ));
        // Every tile with a basin corner: tiles 9..=11 on X and 19..=21 on
        // Z, two triangles each, over corners X 9..=12 and Z 19..=22.
        assert_eq!(feed.triangles.len(), 3 * 3 * 2);
        assert_eq!(feed.vertices.len(), 4 * 4);
        assert!(
            feed.triangles
                .iter()
                .all(|triangle| triangle.surface == SceneSurface::Water
                    && triangle.texture == Some(7))
        );
        // Flat water at the plane: 300 below the eye, normals straight up.
        assert!(feed
            .vertices
            .iter()
            .all(|vertex| vertex.position[1] == -300.0 && vertex.normal == [0.0, 1.0, 0.0]));
        // The block spans X 9*128..12*128 relative to the eye.
        let xs: Vec<f32> = feed
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .collect();
        let lowest = xs.iter().copied().fold(f32::MAX, f32::min);
        let highest = xs.iter().copied().fold(f32::MIN, f32::max);
        assert_eq!(
            (lowest, highest),
            (9.0 * 128.0 - eye[0] as f32, 12.0 * 128.0 - eye[0] as f32)
        );
    }

    /// The mesh as it was built before the waves were taken only where it
    /// reads them: every grid point's height, then the tiles.
    fn whole_grid_water(feed: &mut SceneFeed, request: WaterRequest<'_>) {
        let terrain = request.terrain;
        let spacing = i32::from(terrain.sampling().spacing.units());
        let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
        let steps = (request.reach / spacing).clamp(1, 255);
        let tiles = (2 * steps + 1) as usize;
        let points = tiles + 3;
        let origin =
            [0, 2].map(|axis| (request.eye[axis].div_euclid(spacing) - steps - 1) * spacing);
        let grid = |i: usize, j: usize| -> [u16; 2] {
            [
                (origin[0] + i as i32 * spacing) as u16,
                (origin[1] + j as i32 * spacing) as u16,
            ]
        };
        let mut heights = vec![0_i16; points * points];
        let mut drawn = vec![0_i16; points * points];
        let mut below = vec![false; points * points];
        for i in 0..points {
            for j in 0..points {
                let xz = grid(i, j);
                let ground = terrain.raw_height_at(xz);
                below[i * points + j] = ground < plane;
                let surface = terrain.water_surface_height(
                    TerrainWaterSurfaceRequest {
                        xz,
                        wave_clock: request.clock,
                        world_flags: request.world_flags,
                        terrain_height: ground,
                    },
                    request.sine,
                );
                heights[i * points + j] = surface;
                drawn[i * points + j] = if surface <= ground {
                    surface.min(ground.saturating_sub(2))
                } else {
                    surface
                };
            }
        }
        let height = |i: usize, j: usize| f32::from(heights[i * points + j]);
        let units = spacing as f32;
        let mut ids = vec![u32::MAX; points * points];
        let relative = |value: u16, eye: i32| i32::from(value).wrapping_sub(eye) as i16;
        for i in 1..=tiles {
            for j in 1..=tiles {
                let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                if !corners.iter().any(|&(ci, cj)| below[ci * points + cj]) {
                    continue;
                }
                let place = corners.map(|(ci, cj)| {
                    let slot = &mut ids[ci * points + cj];
                    if *slot == u32::MAX {
                        let [x, z] = grid(ci, cj);
                        let slope = [
                            (height(ci + 1, cj) - height(ci - 1, cj)) / (2.0 * units),
                            (height(ci, cj + 1) - height(ci, cj - 1)) / (2.0 * units),
                        ];
                        let length = (slope[0] * slope[0] + slope[1] * slope[1] + 1.0).sqrt();
                        *slot = feed.vertices.len() as u32;
                        feed.vertices.push(SceneVertex {
                            position: [
                                f32::from(relative(x, request.eye[0])),
                                f32::from(relative(drawn[ci * points + cj] as u16, request.eye[1])),
                                f32::from(relative(z, request.eye[2])),
                            ],
                            normal: [-slope[0] / length, 1.0 / length, -slope[1] / length],
                            uv: [0.0; 2],
                            color: [255; 4],
                            shade: [0; 4],
                            fog: [0; 4],
                            motion: [0.0; 3],
                        });
                    }
                    *slot
                });
                let [a, b, c, d] = place;
                for vertices in [[a, b, c], [a, c, d]] {
                    feed.triangles.push(SceneTriangle {
                        vertices,
                        texture: request.texture,
                        surface: SceneSurface::Water,
                        model: None,
                        light: None,
                        cutout: false,
                        own_fog: false,
                        object: None,
                        culled: false,
                    });
                }
            }
        }
    }

    /// Taking the waves only where the mesh reads them builds the same mesh
    /// as taking them everywhere: rolling islands and bays, moving waves.
    #[test]
    fn the_water_mesh_is_the_whole_grids() {
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
            let wave = ((x * 37 + z * 11) % 23) as i32 - 11 + ((x / 7 + z / 5) % 3) as i32;
            u16::from((wave as i8) as u8)
        });
        let terrain = TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale: TerrainHeightScale::Units32,
            },
            0,
            cells,
        );
        let sine: [u16; 4096] = std::array::from_fn(|k| {
            ((k as f64 / 4096.0 * std::f64::consts::FRAC_PI_2).sin() * 16384.0) as u16
        });
        for (eye, clock) in [
            ([40_000, 500, 900], 1234_u32),
            ([65_400, 200, 31_000], 99_999),
        ] {
            let request = || WaterRequest {
                terrain: &terrain,
                eye,
                clock,
                world_flags: 0x40,
                sine: &sine,
                reach: 3000,
                texture: Some(3),
            };
            let mut new = SceneFeed::default();
            new.begin(SceneCamera::default());
            add_water(&mut new, request());
            let mut old = SceneFeed::default();
            old.begin(SceneCamera::default());
            whole_grid_water(&mut old, request());
            assert!(!old.triangles.is_empty());
            assert_eq!(new.vertices, old.vertices);
            assert_eq!(new.triangles, old.triangles);
        }
    }

    #[test]
    fn water_on_ground_at_its_level_is_drawn_just_under_it() {
        // Ground exactly at the plane (0) except the basin at byte -4.
        let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
            let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
            let basin = (10..=11).contains(&x) && (20..=21).contains(&z);
            u16::from(if basin { (-4_i8) as u8 } else { 0 })
        });
        let terrain = TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale: TerrainHeightScale::Units32,
            },
            0,
            cells,
        );
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera::default());
        let sine = [0_u16; 4096];
        let eye = [10 * 128 + 64, 300, 20 * 128 + 64];
        assert!(add_water(
            &mut feed,
            WaterRequest {
                terrain: &terrain,
                eye,
                clock: 0,
                world_flags: 0,
                sine: &sine,
                reach: 1024,
                texture: None,
            },
        ));
        // The basin's four corners stay at the plane; the twelve round them,
        // whose ground is at the plane, sit 2 units under it. The normals
        // keep the game's flat water.
        let at = |height: f32| {
            feed.vertices
                .iter()
                .filter(|vertex| vertex.position[1] == height)
                .count()
        };
        assert_eq!((at(-300.0), at(-302.0)), (4, 12));
        assert!(feed
            .vertices
            .iter()
            .all(|vertex| vertex.normal == [0.0, 1.0, 0.0]));
    }
}
