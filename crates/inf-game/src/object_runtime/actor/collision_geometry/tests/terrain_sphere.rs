use super::*;
use crate::terrain_runtime::TerrainSpacing;
use inf_formats::{TerrainCell, TERRAIN_GRID_SIDE};

fn spacing(value: &Value) -> TerrainSpacing {
    match value.as_u64().unwrap() {
        0x80 | 0x7f => TerrainSpacing::Units128,
        0x100 | 0xff => TerrainSpacing::Units256,
        other => panic!("unsupported native grid global {other:#x}"),
    }
}

fn patch_cells(patch: &Value) -> Vec<TerrainCell> {
    let mut cells = vec![TerrainCell::from_raw(0); TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE];
    let side = patch["side"].as_u64().unwrap() as usize;
    let origin = [&patch["origin"][0], &patch["origin"][1]].map(|v| v.as_u64().unwrap() as usize);
    let hex = patch["cells"].as_str().unwrap();
    assert_eq!(hex.len(), side * side * 4);
    for i in 0..side {
        for j in 0..side {
            let at = (i * side + j) * 4;
            let raw = u16::from_str_radix(&hex[at..at + 4], 16).unwrap();
            let x = (origin[0] + i) % TERRAIN_GRID_SIDE;
            let z = (origin[1] + j) % TERRAIN_GRID_SIDE;
            cells[x * TERRAIN_GRID_SIDE + z] = TerrainCell::from_raw(raw);
        }
    }
    cells
}

#[test]
fn terrain_sphere_contact_matches_native_0044ec90_cells_edges_and_retention() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/terrain_sphere_contact_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["provenance"]["fpcw"].as_u64(), Some(0x027f));
    let patches = fixture["patches"].as_array().unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!((patches.len(), cases.len()), (32, 971));
    let mut contacts = 0;
    for (number, patch) in patches.iter().enumerate() {
        let cells = patch_cells(patch);
        let grid = TerrainSphereGrid {
            spacing: spacing(&patch["grid"][0]),
            mask: spacing(&patch["grid"][1]),
            height_shift: patch["grid"][2].as_u64().unwrap() as u32,
        };
        for (index, case) in cases.iter().enumerate() {
            if case["patch"].as_u64() != Some(number as u64) {
                continue;
            }
            let initial: [u32; 5] = std::array::from_fn(|i| dword(&case["initial"][i]) as u32);
            let mut contact = CollisionContact::from_native_words(initial.map(Some));
            let origin = xyz(&case["origin"]);
            let offset = xyz(&case["center"]);
            let center = std::array::from_fn(|i| origin[i].wrapping_add(offset[i]));
            terrain_sphere_contact(&mut contact, &cells, grid, center, dword(&case["radius"]))
                .unwrap();
            let expected: [u32; 5] = std::array::from_fn(|i| dword(&case["final"][i]) as u32);
            assert_eq!(
                contact.native_words(),
                expected.map(Some),
                "native case {index}"
            );
            if case["writes"].as_array().unwrap().is_empty() {
                assert_eq!(expected, initial, "native case {index} rejection");
            } else {
                contacts += 1;
                assert_eq!(contact.flag(), Some(1));
            }
        }
    }
    assert_eq!(contacts, 724);
}

#[test]
fn terrain_sphere_contact_refuses_walks_native_would_not_finish() {
    let cells = vec![TerrainCell::from_raw(0); TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE];
    let mut contact = CollisionContact::unwritten();
    // A 0x100 step over 0x7F-masked bounds can miss its end forever.
    let grid = TerrainSphereGrid {
        spacing: TerrainSpacing::Units256,
        mask: TerrainSpacing::Units128,
        height_shift: 5,
    };
    assert!(matches!(
        terrain_sphere_contact(&mut contact, &cells, grid, [0x60, 0, 0x60], 0x20),
        Err(TerrainSphereError::UnboundedWalk { .. })
    ));
    // A strongly negative radius wraps the end below the start.
    let grid = TerrainSphereGrid {
        mask: TerrainSpacing::Units256,
        ..grid
    };
    assert!(matches!(
        terrain_sphere_contact(&mut contact, &cells, grid, [0, 0, 0], -0x1000),
        Err(TerrainSphereError::UnboundedWalk { .. })
    ));
    assert!(matches!(
        terrain_sphere_contact(&mut contact, &cells[1..], grid, [0, 0, 0], 1),
        Err(TerrainSphereError::GridShape { .. })
    ));
    assert_eq!(contact, CollisionContact::unwritten());
}
