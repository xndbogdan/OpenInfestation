use super::*;

#[test]
fn the_hull_runs_round_the_outermost_points_anticlockwise() {
    let points = vec![
        [0.0, 0.0],
        [10.0, 0.0],
        [5.0, 5.0],
        [10.0, 10.0],
        [0.0, 10.0],
        [10.0, 0.0],
    ];
    assert_eq!(
        hull(points),
        [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
    );
}

/// Level 1: its factories stand on footings round their origins, at their
/// height; furniture held above its cell (the arrows) stands on none.
#[test]
fn level_1s_buildings_stand_on_footings() {
    let (residency, assets, world) = crate::test_worlds::offline_world_retained(
        18,
        crate::terrain_runtime::TerrainSpacing::Units128,
    );
    let furniture = FurnitureDrawState::new(&inf_formats::native_basis::NativeBasisMath::new(
        assets.sine_table().values,
    ));
    let process = ObjectDrawProcessState::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let footings = world_footings(
        &residency,
        &world.arena,
        &world.terrain,
        &furniture,
        &process,
        crate::modern_tuning::current().ultra(),
    );
    // The factory south of the start (census, 2026-10-24), 542 across.
    let [x, z] = [37085.0_f32, 34363.0_f32];
    let factory = footings
        .iter()
        .find(|footing| {
            let low = [0, 1].map(|axis| {
                footing
                    .outline
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::MAX, f32::min)
            });
            let high = [0, 1].map(|axis| {
                footing
                    .outline
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::MIN, f32::max)
            });
            let [x, z] = [x, z].map(|c| if c >= 32768.0 { c - 65536.0 } else { c });
            (low[0]..=high[0]).contains(&x)
                && (low[1]..=high[1]).contains(&z)
                && high[0] - low[0] > 500.0
        })
        .expect("the factory's footing");
    let ground = world.terrain.height_at([x as u16, z as u16]);
    assert_eq!(factory.base, f32::from(ground));
    // Every footing is a building's: no arrow's.
    assert!(
        footings.len() > 2 && footings.len() < 200,
        "{}",
        footings.len()
    );
}
