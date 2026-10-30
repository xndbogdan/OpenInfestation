use super::*;

/// Cells from a height byte per grid point (X-major), at shift 5.
fn cells(height: impl Fn(usize, usize) -> i8) -> Vec<TerrainCell> {
    (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE)
        .map(|index| {
            let [x, z] = [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE];
            TerrainCell::from_raw(u16::from(height(x, z) as u8))
        })
        .collect()
}

/// The shipped tuning's shape.
const SHAPE: UltraShape = UltraShape {
    crease: 40.0,
    fill: 256.0,
    ease: 192.0,
};

/// A square footing of half-width `half` round world `x`, `z`.
fn square(x: f32, z: f32, half: f32, base: f32) -> Footing {
    Footing {
        outline: vec![
            [x - half, z - half],
            [x + half, z - half],
            [x + half, z + half],
            [x - half, z + half],
        ],
        base,
    }
}

/// A rough field: heights from a small hash.
fn rough(x: usize, z: usize) -> i8 {
    ((x * 7919 + z * 104_729) % 61) as i8 - 30
}

#[test]
fn the_grid_points_keep_retails_heights() {
    let cells = cells(rough);
    let ground = UltraGround::build(&cells, 5, SHAPE, Vec::new());
    for (x, z) in [(0, 0), (3, 7), (511, 511), (200, 13)] {
        let retail = f32::from(rough(x, z)) * 32.0;
        assert_eq!(ground.point(x * SPLIT, z * SPLIT), retail);
        let at = [(x * 128) as u16, (z * 128) as u16];
        assert_eq!(ground.height_at(at, false), retail);
        assert_eq!(ground.height_at(at, true), retail);
    }
}

/// A shared edge takes the same height from the patches on both sides.
#[test]
fn neighbouring_patches_meet_on_their_edge() {
    let cells = cells(rough);
    let grid = Grid {
        cells: &cells,
        shift: 5,
        crease: 40.0_f32.to_radians(),
    };
    for (x, z) in [(10, 10), (100, 37), (0, 511)] {
        for u in [0.25, 0.5, 0.75] {
            // Across Z: cell (x, z)'s far edge is cell (x, z + 1)'s near one.
            assert!((grid.patch(x, z, u, 1.0) - grid.patch(x, z + 1, u, 0.0)).abs() < 1.0e-3);
            assert!((grid.patch(x, z, 1.0, u) - grid.patch(x + 1, z, 0.0, u)).abs() < 1.0e-3);
        }
    }
}

/// A slope keeps its plane, and gentle ground has no creases.
#[test]
fn a_plane_stays_a_plane() {
    // Rising 2 bytes a cell along X and 1 along Z, well inside the world.
    let cells = cells(|x, z| ((x % 32) as i8 * 2 + (z % 32) as i8) - 40);
    let ground = UltraGround::build(&cells, 5, SHAPE, Vec::new());
    for (fx, fz) in [(21, 23), (40, 41), (51, 33)] {
        let plane = (fx as f32 / 2.0 * 2.0 + fz as f32 / 2.0) * 32.0 - 40.0 * 32.0;
        assert!((ground.point(fx, fz) - plane).abs() < 1.0e-2, "{fx} {fz}");
    }
}

/// A gentle hill rounds off; a steep ridge keeps its edge.
#[test]
fn hills_round_off_and_ridges_stay_sharp() {
    // Along X: a hill of 1 byte a cell (14 degrees each side), a ridge of 4
    // (51 degrees each side), each peaking at X 100.
    let peak = |rise: i8| {
        move |x: usize, _: usize| (30 - i32::from(rise) * (x as i32 - 100).abs().min(7)) as i8
    };
    let midpoint = |rise: i8| {
        let cells = cells(peak(rise));
        let ground = UltraGround::build(&cells, 5, SHAPE, Vec::new());
        // Halfway down the cell after the peak, against the straight line.
        let line = (f32::from(peak(rise)(100, 0)) + f32::from(peak(rise)(101, 0))) * 16.0;
        ground.point(201, 40) - line
    };
    assert!(midpoint(1) > 1.0, "{}", midpoint(1));
    assert_eq!(midpoint(4), 0.0);
}

/// A changed cell's update gives what a rebuild gives, footings and all.
#[test]
fn an_update_matches_a_rebuild() {
    let mut cells = cells(rough);
    let footings = vec![square(50.0 * 128.0, 60.0 * 128.0, 150.0, 1200.0)];
    let mut ground = UltraGround::build(&cells, 5, SHAPE, footings.clone());
    for (x, z) in [(50, 60), (0, 511)] {
        cells[x * TERRAIN_GRID_SIDE + z] = TerrainCell::from_raw(u16::from(90_u8));
        ground.update(&cells, 5, [x, z]);
    }
    assert!(ground == UltraGround::build(&cells, 5, SHAPE, footings));
}

/// A building on a slope: the ground under it and just past it rises to
/// its base, the uphill side stays where it was, and past the ease the
/// slope runs on untouched.
#[test]
fn a_footing_raises_the_ground_to_its_base() {
    // Rising 2 bytes (64 units) a cell along X.
    let slope = |x: usize, _: usize| (x as i32 % 40 * 2 - 40) as i8;
    let cells = cells(slope);
    let plain = UltraGround::build(&cells, 5, SHAPE, Vec::new());
    // A 300-wide footing round X 20 cells (height 0), base 0.
    let footing = square(20.0 * 128.0, 100.0 * 128.0, 150.0, 0.0);
    let ground = UltraGround::build(&cells, 5, SHAPE, vec![footing]);
    let z = 200;
    // Downhill, under the footing: up to the base.
    assert_eq!(ground.point(38, z), 0.0);
    assert_eq!(plain.point(38, z), -64.0);
    // Within a fine point past the outline too.
    assert_eq!(ground.point(37, z), 0.0);
    // Uphill, above the base: as it was.
    assert_eq!(ground.point(42, z), plain.point(42, z));
    // In the ease: between the base and the slope.
    let eased = ground.point(35, z);
    assert!(eased > plain.point(35, z) && eased < 0.0, "{eased}");
    // Past it: untouched.
    assert_eq!(ground.point(30, z), plain.point(30, z));
}

/// A building over a drop deeper than the fill keeps its gap.
#[test]
fn a_footing_over_a_drop_raises_nothing() {
    let cells = cells(|_, _| -40);
    let plain = UltraGround::build(&cells, 5, SHAPE, Vec::new());
    let footing = square(64.0 * 128.0, 64.0 * 128.0, 100.0, 300.0 - 1280.0);
    assert_eq!(
        UltraGround::build(&cells, 5, SHAPE, vec![footing]).heights,
        plain.heights
    );
}

/// A footing across the world's edge raises both sides of it.
#[test]
fn a_footing_wraps_round_the_world() {
    let cells = cells(|_, _| -4);
    let footing = square(0.0, 3000.0, 100.0, 0.0);
    let ground = UltraGround::build(&cells, 5, SHAPE, vec![footing]);
    let z = (3000.0 / FINE) as usize;
    assert_eq!(ground.point(0, z), 0.0);
    assert_eq!(ground.point(FINE_SIDE - 1, z), 0.0);
    assert_eq!(ground.point(FINE_SIDE / 2, z), -128.0);
}

/// Between fine points the ground is split as retail splits its cells.
#[test]
fn the_ground_between_fine_points_follows_the_cells_diagonal() {
    let cells = cells(|x, z| if (x, z) == (10, 10) { 40 } else { 0 });
    let ground = UltraGround::build(&cells, 5, SHAPE, Vec::new());
    // The fine cell east and north of the raised point, at its middle.
    let at = [1280 + 32, 1280 + 32];
    let [h00, h10, h11, h01] =
        [(20, 20), (21, 20), (21, 21), (20, 21)].map(|(fx, fz)| ground.point(fx, fz));
    // Default diagonal (00 to 11): the middle on it, between h00 and h11.
    assert!((ground.height_at(at, false) - (h00 + h11) / 2.0).abs() < 1.0e-3);
    // The other diagonal (10 to 01).
    assert!((ground.height_at(at, true) - (h10 + h01) / 2.0).abs() < 1.0e-3);
}
