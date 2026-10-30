use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    world_furniture_effect::{
        run_furniture_replacement, FurnitureReplacement, ProcessFurnitureQueryState,
    },
};
use inf_formats::FrontendExecutable;
use serde_json::Value;

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

#[test]
fn furniture_replacement_matches_native_sparse_fallback_null_and_cell_writes() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/furniture_replacement_native.json"
    ))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for case in receipt["cases"].as_array().unwrap() {
        let input = &case["input"];
        let position = std::array::from_fn(|n| word(&input["xyz"][n]) as u16);
        let spacing = if word(&input["spacing"]) == 128 {
            TerrainSpacing::Units128
        } else {
            TerrainSpacing::Units256
        };
        let mut terrain = terrain(spacing, word(&input["cell"]) as u16);
        let grid = &case["initial_center"]["grid"];
        let [gx, gz] = [word(&grid[0]) as u16, word(&grid[1]) as u16];
        terrain.cells[usize::from(gx) * TERRAIN_GRID_SIDE + usize::from(gz)] =
            TerrainCell::from_raw(word(&case["initial_center"]["value"]) as u16);
        if input["record"] == "sparse" {
            terrain.buckets[usize::from(gz)]
                .records
                .push(TerrainRecord {
                    x: gx,
                    state: word(&input["state"]) as u8,
                    resource_lookup: TerrainResourceLookup::from_raw(word(&input["lookup"]) as u8),
                });
        }
        let mut expected_cells = terrain.cells.clone();
        for change in case["cells"].as_array().unwrap() {
            let grid = &change["grid"];
            let index = word(&grid[0]) as usize * TERRAIN_GRID_SIDE + word(&grid[1]) as usize;
            assert_eq!(expected_cells[index].raw(), word(&change["before"]) as u16);
            expected_cells[index] = TerrainCell::from_raw(word(&change["after"]) as u16);
        }
        let mut process = ProcessFurnitureQueryState::native_startup(executable).unwrap();
        process.write_fallback_state(word(&input["state"]) as u8);
        process.write_fallback_lookup(word(&input["lookup"]) as u8);
        let mut rng = word(&input["seed"]);
        let replacement = match word(&input["callback"]) {
            0x0043_7060 => FurnitureReplacement::ClearCellAndLookupFf,
            0x0043_70f0 => FurnitureReplacement::LookupFd,
            other => panic!("unsupported receipt callback {other:08X}"),
        };
        assert!(run_furniture_replacement(
            &mut terrain,
            &mut process,
            position,
            replacement,
            &mut rng,
        )
        .unwrap());
        assert_eq!(terrain.cells, expected_cells, "{input}");
        assert_eq!(rng, word(&case["rng"]), "{input}");
        if input["record"] == "sparse" {
            let sparse = terrain.buckets[usize::from(gz)].records[0];
            assert_eq!(sparse.x, gx, "sparse key survives");
            assert_eq!(sparse.state, word(&case["sparse"][1]) as u8, "{input}");
            assert_eq!(sparse.resource_lookup.raw(), word(&case["sparse"][2]) as u8);
        }
        let fallback = process.fallback_payload();
        assert_eq!(fallback.state, word(&case["fallback"][1]) as u8, "{input}");
        assert_eq!(
            fallback.resource_lookup.raw(),
            word(&case["fallback"][2]) as u8
        );
    }
}
