use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};
use inf_formats::TERRAIN_GRID_SIDE;

/// A world of High ground at height byte `height(x, z)`, a baked shade of 6.
fn terrain(height: impl Fn(usize, usize) -> i8) -> TerrainRuntime {
    let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
        let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
        6 << 12 | u16::from(height(x, z) as u8)
    });
    TerrainRuntime::from_raw_cells(
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
        0,
        cells,
    )
}

/// A pit two bytes (64 units) deep, 3 by 3 points round grid point
/// (100, 200), in flat ground.
fn pit(x: usize, z: usize) -> i8 {
    if x.abs_diff(100) <= 1 && z.abs_diff(200) <= 1 {
        -2
    } else {
        0
    }
}

fn mesh(height: impl Fn(usize, usize) -> i8) -> TerrainMesh {
    TerrainMesh::build(&terrain(height), [0.0, 1.0, 0.0])
}

fn pitted() -> TerrainMesh {
    mesh(pit)
}

#[test]
fn the_tiles_hold_every_triangle_once_each_inside_its_tile() {
    let mesh = mesh(|x, _| (x % 64) as i8);
    let tiles = tile_ground(&mesh);
    assert_eq!(tiles.triangles.len(), mesh.triangles.len());
    let total: u32 = tiles.ranges.iter().map(|range| range[1]).sum();
    assert_eq!(total as usize, mesh.triangles.len());
    // High's 32 by 32 cells a tile, two triangles a cell.
    assert!(tiles.ranges.iter().all(|range| range[1] == 2 * 32 * 32));
    for (tile, range) in tiles.ranges.iter().enumerate() {
        let low = [tile / TILES, tile % TILES].map(|at| at as f32 * TILE_UNITS);
        for triangle in range[0]..range[0] + range[1] {
            for k in 0..3 {
                let point = mesh.vertices[tiles.indices[triangle as usize * 3 + k] as usize];
                assert!((low[0]..=low[0] + TILE_UNITS).contains(&point.position[0]));
                assert!((low[1]..=low[1] + TILE_UNITS).contains(&point.position[2]));
            }
        }
    }
}

#[test]
fn the_tiles_cover_the_world_once_round_the_eye() {
    // An eye near the wrap's corner: the tiles there and across the wrap
    // all lie within half the wrap of it.
    let eye = [65000.0, 300.0, 200.0];
    for tile in 0..TILES * TILES {
        let [x, _, z] = tile_translation(tile, eye);
        let middle = [
            x + 2048.0 + (tile / TILES) as f32 * TILE_UNITS,
            z + 2048.0 + (tile % TILES) as f32 * TILE_UNITS,
        ];
        assert!(
            middle.iter().all(|axis| axis.abs() <= 32768.0),
            "{tile} {middle:?}"
        );
    }
    // The patch round it takes the tiles about the wrap's corner.
    let first = patch_first([eye[0], eye[2]]);
    assert_eq!(first, [14, -1]);
    let taken = patch_tiles(first);
    assert!(taken.contains(&(15 * TILES)) && taken.contains(&(14 * TILES + 15)));
    assert!(taken.contains(&(15 * TILES + 1)));
}

/// The reproduction (Tabuchi's start in the rain, 2026-10-29): the patch
/// built round an eye at Z 32,608 stays drawn while the eye crosses the
/// wrap's seam to -32,484 (the camera's signed words) until the next patch
/// comes. It still lies under the eye, and the tiles it leaves out are the
/// ones round the eye.
#[test]
fn a_patch_built_across_the_seam_stays_under_the_eye() {
    let built = patch_first([-15856.0, 32608.0]);
    assert_eq!(built, [-5, 6]);
    for eye in [
        [-15856.0, -849.0, 32608.0],
        [-15856.0, -849.0, -32484.0],
        [49680.0, -849.0, 33052.0],
    ] {
        let [x, _, z] = patch_translation(built, eye);
        let side = PATCH_TILES as f32 * TILE_UNITS;
        assert!(
            (-side..=0.0).contains(&x) && (-side..=0.0).contains(&z),
            "{eye:?}: {x} {z}"
        );
        // The eye's own tile, by the tiles' placement, is one it covers.
        let under = (0..TILES * TILES)
            .find(|&tile| {
                let [tx, _, tz] = tile_translation(tile, eye);
                let x = tx + (tile / TILES) as f32 * TILE_UNITS;
                let z = tz + (tile % TILES) as f32 * TILE_UNITS;
                (-TILE_UNITS..0.0).contains(&x) && (-TILE_UNITS..0.0).contains(&z)
            })
            .unwrap();
        assert!(patch_tiles(built).contains(&under), "{eye:?}: {under}");
    }
}

#[test]
fn without_puddles_the_patch_is_retails_ground() {
    let mesh = mesh(|x, z| ((x * 7 + z * 3) % 11) as i8 - 5);
    let ground = DetailGround::new(&mesh);
    let key = PatchKey::new(
        mesh.generation,
        [3, 5],
        CarveLook {
            fill: 0.0,
            shore: None,
        },
    );
    let patch = build_patch(&ground, key);
    assert_eq!(patch.points, 3 * 128 + 1);
    // Every split triangle: 9 tiles' triangles, 16 each.
    assert_eq!(patch.triangles.len(), 9 * 2 * 32 * 32 * 16);
    assert_eq!(patch.indices.len(), patch.triangles.len() * 3);
    // Every coarse point keeps retail's height; points between lie on the
    // segment between two coarse points along a cell's edge.
    for i in (0..patch.points).step_by(SPLIT) {
        for j in (0..patch.points).step_by(SPLIT) {
            let point = patch.vertices[i * patch.points + j];
            let [ci, cj] = [3 * 32 + i / SPLIT, 5 * 32 + j / SPLIT];
            assert_eq!(point.position[1], mesh.vertices[ci * 513 + cj].position[1]);
            assert_eq!(point.carve, 0.0);
        }
    }
    let between = patch.vertices[2 * patch.points].position[1];
    let ends = [0, 4].map(|i| patch.vertices[i * patch.points].position[1]);
    assert!((between - (ends[0] + ends[1]) / 2.0).abs() < 1e-4);
    // No split triangle is degenerate.
    for corners in patch.indices.chunks(3) {
        let p: [[f32; 3]; 3] =
            std::array::from_fn(|k| patch.vertices[corners[k] as usize].position);
        let e1 = [p[1][0] - p[0][0], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][2] - p[0][2]];
        assert_ne!(e1[0] * e2[1] - e1[1] * e2[0], 0.0);
    }
}

#[test]
fn a_puddle_sinks_its_hollow_and_leaves_the_flat_alone() {
    let mesh = pitted();
    let ground = DetailGround::new(&mesh);
    let look = CarveLook {
        fill: 1.0,
        shore: None,
    };
    // The pit's middle at world (12800, 25600): tile (3, 6), in the middle
    // of a patch from (2, 5).
    let middle = [12800.0, 25600.0];
    let deep = carve_at(&mesh, middle, -64.0, 1.0, &look);
    assert!(deep > 0.5 * PUDDLE_DEPTH && deep <= PUDDLE_DEPTH, "{deep}");
    assert_eq!(carve_at(&mesh, [12800.0, 27600.0], 0.0, 1.0, &look), 0.0);
    // The water stands whole over the pit's middle, none on the flat.
    assert_eq!(water_at(&mesh, middle, &look), 1.0);
    assert_eq!(water_at(&mesh, [12800.0, 27600.0], &look), 0.0);
    // Dry, nothing sinks.
    let dry = CarveLook {
        fill: 0.0,
        shore: None,
    };
    assert_eq!(carve_at(&mesh, middle, -64.0, 1.0, &dry), 0.0);
    let patch = build_patch(&ground, PatchKey::new(mesh.generation, [2, 5], look));
    let at = |world: [f32; 2]| {
        let [i, j] = [
            (world[0] - 2.0 * TILE_UNITS) / patch.fine,
            (world[1] - 5.0 * TILE_UNITS) / patch.fine,
        ]
        .map(|g| g.round() as usize);
        patch.vertices[i * patch.points + j]
    };
    let sunk = at(middle);
    // Smoothed over its neighbours, which lie as deep in the pit's middle.
    assert!(
        (sunk.carve - deep).abs() < 0.1 * deep,
        "{} {deep}",
        sunk.carve
    );
    assert!((sunk.position[1] - (-64.0 - sunk.carve)).abs() < 1e-3);
    // Beside the middle the bed slopes up toward the shore: its normal
    // leans toward the middle.
    let bank = at([12800.0 + 96.0, 25600.0]);
    if bank.carve > 0.0 && bank.carve < sunk.carve {
        assert!(bank.normal[0] < 0.0, "{:?}", bank.normal);
    }
    // The patch's border never sinks.
    assert!(patch.vertices[..patch.points]
        .iter()
        .all(|vertex| vertex.carve == 0.0));
}

/// No puddle stands on a lake's bed (2026-10-28: puddles only form
/// above water level): the ground under the lake's plane sinks
/// nowhere, in the drawn patch or under the wheels; the cut is the plane,
/// not the shore's band.
#[test]
fn no_puddle_sinks_under_the_lakes_plane() {
    let mesh = pitted();
    let shore = |level| CarveLook {
        fill: 1.0,
        shore: Some(ShoreLook {
            level,
            fade: 24.0,
            wobble: 0.0,
        }),
    };
    let open = CarveLook {
        fill: 1.0,
        shore: None,
    };
    let middle = [12800.0, 25600.0];
    // The pit's floor (-64) lies under a lake at -60: no puddle there.
    assert_eq!(carve_at(&mesh, middle, -64.0, 1.0, &shore(-60.0)), 0.0);
    // A lake under the floor, at -70, takes nothing from the pit's puddle:
    // no ground lies under it, so no lake is near.
    assert_eq!(
        carve_at(&mesh, middle, -64.0, 1.0, &shore(-70.0)),
        carve_at(&mesh, middle, -64.0, 1.0, &open)
    );
    assert!(carve_at(&mesh, middle, -64.0, 1.0, &open) > 0.0);
    // The drawn patch over the drowned pit floor sinks only by what its
    // smoothing takes from the wall above the plane, a fraction of a unit
    // (under 0.08 at the depth of 6).
    let ground = DetailGround::new(&mesh);
    let patch = build_patch(
        &ground,
        PatchKey::new(mesh.generation, [2, 5], shore(-60.0)),
    );
    let drowned: Vec<f32> = patch
        .vertices
        .iter()
        .filter(|vertex| vertex.position[1] + vertex.carve < -60.0)
        .map(|vertex| vertex.carve)
        .collect();
    assert!(!drowned.is_empty());
    assert!(drowned.iter().all(|carve| *carve < 0.08), "{drowned:?}");
    // Nor do the wheels.
    let mut terrain = terrain(pit);
    let wheel = [12800, 25600];
    terrain.set_puddle_carve(Some(shore(-60.0)));
    assert_eq!(terrain.wheel_height_at(wheel), terrain.height_at(wheel));
}

#[test]
fn the_patch_key_steps_the_fill() {
    let key = |fill| PatchKey::new(1, [0, 0], CarveLook { fill, shore: None });
    assert_eq!(key(0.5001), key(0.4999));
    assert_ne!(key(0.5), key(0.53));
}

/// The wheels' bed is the patch's: the same points on the world's lattice,
/// smoothed alike, wherever the patch lies.
#[test]
fn the_wheels_bed_is_the_drawn_bed() {
    let mesh = pitted();
    let ground = DetailGround::new(&mesh);
    let look = CarveLook {
        fill: 1.0,
        shore: None,
    };
    for first in [[2, 5], [1, 4]] {
        let patch = build_patch(&ground, PatchKey::new(mesh.generation, first, look));
        let origin = first.map(|tiles| tiles as f32 * TILE_UNITS);
        let mut compared = 0;
        for (index, vertex) in patch.vertices.iter().enumerate() {
            let [x, z] = [vertex.position[0], vertex.position[2]];
            let border = x.min(z).min(3.0 * TILE_UNITS - x).min(3.0 * TILE_UNITS - z);
            if border < 1100.0 || vertex.carve == 0.0 {
                continue;
            }
            let world = [origin[0] + x, origin[1] + z];
            let bed = bed_carve(&mesh, world, &look);
            assert!(
                (bed - vertex.carve).abs() < 1.0e-3,
                "{index}: {bed} {}",
                vertex.carve
            );
            compared += 1;
        }
        assert!(compared > 4, "{compared}");
    }
}

/// The wheeled craft's ground sinks into the carve only while the session
/// sets one; retail's height otherwise.
#[test]
fn the_wheels_sink_only_where_the_carve_is_set() {
    let mut terrain = terrain(pit);
    let middle = [12800, 25600];
    let retail = terrain.height_at(middle);
    assert_eq!(terrain.wheel_height_at(middle), retail);
    terrain.set_puddle_carve(Some(CarveLook {
        fill: 1.0,
        shore: None,
    }));
    let sunk = terrain.wheel_height_at(middle);
    assert!(
        sunk < retail && sunk >= retail - PUDDLE_DEPTH as i16,
        "{sunk} {retail}"
    );
    // Far from the pit, the flat stays retail's.
    assert_eq!(
        terrain.wheel_height_at([20000, 40000]),
        terrain.height_at([20000, 40000])
    );
    // The game's grid of heights is the tracer's mesh's.
    let mesh = pitted();
    for (i, j) in [(100, 200), (5, 7), (512, 0)] {
        assert_eq!(
            GridHeights::height(&terrain, i, j),
            GridHeights::height(&mesh, i, j)
        );
    }
    assert_eq!(GridHeights::points(&terrain), 513);
}

/// In the worlds the rain wets (Pelagia, Tabuchi), with the puddles full, no
/// bed sinks under the lake's plane: the carve takes only ground above it,
/// and its smoothing a tenth of a unit past at most (the bed at the
/// waterline, which the lake's water covers).
#[test]
fn no_puddle_bed_sinks_under_the_plane_in_the_rain_worlds() {
    let mut deepest = 0.0f32;
    for world in [1, 9] {
        let (_, _, session) =
            crate::test_worlds::offline_world_retained(world, TerrainSpacing::Units128);
        let terrain = &session.terrain;
        let level = f32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
        let water = crate::modern_tuning::ModernTuning::defaults().water(world);
        let look = CarveLook::for_world(1.0, level, &water).expect("a lake");
        let mesh = TerrainMesh::build(terrain, [0.0, 1.0, 0.0]);
        let fine = mesh.spacing / SPLIT as f32;
        let side = (65536.0 / fine) as usize;
        let mut carved = 0;
        for i in 0..side {
            for j in 0..side {
                let at = [i as f32 * fine, j as f32 * fine];
                let y = ground_height(&mesh, at);
                if (y - level).abs() > 3.0 * PUDDLE_DEPTH {
                    continue;
                }
                let bed = bed_carve(&mesh, at, &look);
                carved += usize::from(bed > 0.0);
                deepest = deepest.max(y.min(level) - (y - bed));
            }
        }
        assert!(carved > 0, "world {world}: no puddle by the plane");
    }
    assert!(deepest < 0.1, "{deepest}");
}
