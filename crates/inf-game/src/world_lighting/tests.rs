use super::*;
use crate::menu::MenuBasis;
use serde::Deserialize;

fn policy(spacing: TerrainSpacing) -> TerrainLightPolicy {
    TerrainLightPolicy {
        enabled: true,
        spacing,
        color: TerrainLightColor::Unmodified,
    }
}

#[test]
fn clear_preserves_anchor_and_current_publication_changes_only_coordinates() {
    let mut grid = WorldLightGrid::default();
    grid.set_anchor([0x1234_6000, -256]);
    let anchors = grid.anchors();
    grid.cells.fill(117);
    grid.clear();
    assert_eq!(grid.anchors(), anchors);
    assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
    grid.cells[31] = 192;
    grid.set_anchor([5, 0x10004]);
    assert_eq!(
        grid.anchors(),
        [5u16.wrapping_sub(0x1680), 4u16.wrapping_sub(0x1680)]
    );
    assert_eq!(grid.cells[31], 192);
}

#[test]
fn world_anchor_uses_stored_forward_float_then_low_dword_truncation() {
    let mut basis = MenuBasis::INITIAL;
    basis.words[7] = (-0.125f32).to_bits();
    basis.words[9] = 0.3f32.to_bits();
    let view = SceneView {
        eye: [i32::MIN + 200, 777, i32::MAX - 200],
        basis,
    };
    assert_eq!(
        world_light_anchor_coordinates(view),
        [
            view.eye[0].wrapping_sub(480),
            view.eye[2].wrapping_add(1152)
        ]
    );
    basis.words[7] = f32::NAN.to_bits();
    basis.words[9] = f32::INFINITY.to_bits();
    assert_eq!(
        world_light_anchor_coordinates(SceneView { basis, ..view }),
        [view.eye[0], view.eye[2]]
    );
}

#[test]
fn signed_rgb_accumulates_and_native_negative_radius_is_not_an_abs_mode() {
    let mut grid = WorldLightGrid::default();
    let request = TerrainLightRequest {
        world_xz: [768, 768],
        radius: 512,
        rgb: [100, -100, 9],
    };
    let center = (6 * WORLD_LIGHT_SIDE + 6) * 3;
    grid.add(request, policy(TerrainSpacing::Units128)).unwrap();
    grid.add(request, policy(TerrainSpacing::Units128)).unwrap();
    assert_eq!(&grid.cells()[center..center + 3], &[127, 128, 18]);
    let before = grid.cells.clone();
    let outcome = grid
        .add(
            TerrainLightRequest {
                radius: -512,
                ..request
            },
            policy(TerrainSpacing::Units128),
        )
        .unwrap();
    assert_eq!(outcome.samples, 64);
    assert_eq!(outcome.cell_writes, 0);
    assert_eq!(grid.cells, before);
}

#[test]
fn native_division_fault_precedes_grid_index_rejection() {
    // Original bytecode checker verifies this exact #DE at0045B37B after the
    // second sample, before the otherwise outside-grid sample could be rejected.
    let mut grid = WorldLightGrid::default();
    let error = grid
        .add(
            TerrainLightRequest {
                world_xz: [768, 768],
                radius: 65536,
                rgb: [9, -19, 33],
            },
            policy(TerrainSpacing::Units128),
        )
        .unwrap_err();
    assert_eq!(
        error,
        TerrainLightError::Division {
            numerator: 134086656,
            denominator: 0
        }
    );
    assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
}

#[derive(Deserialize)]
struct Receipt {
    executable_sha256: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    anchors: [u16; 2],
    spacing: u16,
    enabled: bool,
    radius: i32,
    world_xz: [i32; 2],
    rgb: [i8; 3],
    tint: Option<[u8; 3]>,
    pattern: Option<u8>,
    #[serde(default)]
    clear: bool,
    anchor_input: Option<[i32; 2]>,
    expected_anchors: [u16; 2],
    samples: u64,
    cell_writes: u64,
    changes: Vec<(usize, u8)>,
}

#[test]
fn original_writer_receipt_matches_every_cell_and_sample_store_count() {
    // Regenerate with tracked check_world_lighting.py --output, which executes
    // authenticated original clear/anchor/writer/memset without byte patches.
    let receipt: Receipt = serde_json::from_str(include_str!("native_receipt.json")).unwrap();
    assert_eq!(
        receipt.executable_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert!(receipt.cases.len() >= 25);
    for case in receipt.cases {
        let mut grid = WorldLightGrid {
            anchors: case.anchors,
            ..WorldLightGrid::default()
        };
        if let Some(seed) = case.pattern {
            for (index, value) in grid.cells.iter_mut().enumerate() {
                *value = (index as u8).wrapping_mul(17).wrapping_add(seed);
            }
        }
        let mut expected = grid.cells.clone();
        if case.clear {
            expected.fill(0);
        }
        for (index, value) in case.changes {
            expected[index] = value;
        }
        if case.clear {
            grid.clear();
        }
        if let Some(coordinates) = case.anchor_input {
            grid.set_anchor(coordinates);
        }
        let spacing = match case.spacing {
            128 => TerrainSpacing::Units128,
            256 => TerrainSpacing::Units256,
            _ => panic!("invalid native fixture spacing"),
        };
        let outcome = grid
            .add(
                TerrainLightRequest {
                    world_xz: case.world_xz,
                    radius: case.radius,
                    rgb: case.rgb,
                },
                TerrainLightPolicy {
                    enabled: case.enabled,
                    spacing,
                    color: case
                        .tint
                        .map_or(TerrainLightColor::Unmodified, TerrainLightColor::Infrared),
                },
            )
            .unwrap();
        assert_eq!(
            grid.anchors(),
            case.expected_anchors,
            "{} anchors",
            case.name
        );
        assert_eq!(grid.cells(), expected.as_slice(), "{} full grid", case.name);
        assert_eq!(
            outcome,
            TerrainLightOutcome {
                samples: case.samples,
                cell_writes: case.cell_writes
            },
            "{} traversal",
            case.name
        );
    }
}

/// The grid keeps each deposit that reached its cells, for the tracer to
/// shadow: a particle's marked as such, with where it shines from, a light
/// too small or off the grid not at all, and a clear forgets them with the
/// cells. The cells stay as the deposits alone make them.
#[test]
fn deposits_that_reach_the_cells_are_kept_as_sources() {
    let mut grid = WorldLightGrid::default();
    let lamp = TerrainLightRequest {
        world_xz: [768, 768],
        radius: 512,
        rgb: [40, 20, -10],
    };
    grid.add(lamp, policy(TerrainSpacing::Units128)).unwrap();
    let cells = grid.cells().to_vec();
    grid.add(
        TerrainLightRequest {
            radius: 200,
            ..lamp
        },
        policy(TerrainSpacing::Units128),
    )
    .unwrap();
    grid.add(
        TerrainLightRequest {
            world_xz: [40_000, 40_000],
            ..lamp
        },
        policy(TerrainSpacing::Units128),
    )
    .unwrap();
    assert_eq!(grid.cells(), &cells[..]);
    grid.add_from(
        TerrainLightRequest {
            world_xz: [1024, 512],
            ..lamp
        },
        policy(TerrainSpacing::Units128),
        [1000, 300, 520],
        true,
    )
    .unwrap();
    assert_eq!(
        grid.sources(),
        [
            TerrainLightSource {
                world_xz: [768, 768],
                radius: 512,
                rgb: [40, 20, -10],
                particle: false,
                emitter: None,
                own: [40, 20, -10],
                warms: true,
            },
            TerrainLightSource {
                world_xz: [1024, 512],
                radius: 512,
                rgb: [40, 20, -10],
                particle: true,
                emitter: Some([1000, 300, 520]),
                own: [40, 20, -10],
                warms: true,
            },
        ]
    );
    grid.clear();
    assert!(grid.sources().is_empty());
}
