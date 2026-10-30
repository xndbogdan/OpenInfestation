use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling};
use inf_formats::{TerrainCell, TERRAIN_CELL_COUNT};
use serde_json::Value;

/// Supplied prepared native cells, not a second invocation of diagonal setup.
pub(crate) fn fixture_terrain(spec: &Value) -> TerrainRuntime {
    let spacing = match spec["spacing"].as_u64().unwrap() {
        128 => TerrainSpacing::Units128,
        256 => TerrainSpacing::Units256,
        _ => panic!("unsupported native fixture spacing"),
    };
    let height_scale = match spec["height_shift"].as_u64().unwrap() {
        4 => TerrainHeightScale::Units16,
        5 => TerrainHeightScale::Units32,
        _ => panic!("unsupported native fixture height scale"),
    };
    let mut terrain = TerrainRuntime {
        sampling: TerrainSampling {
            spacing,
            height_scale,
        },
        cells: vec![TerrainCell::from_raw(0); TERRAIN_CELL_COUNT].into_boxed_slice(),
        buckets: Vec::new(),
        plane_base: spec["plane_base"].as_i64().unwrap() as i32,
        slope_factors: [0; 2],
        light_direction_raw: [0; 3],
        ultra: None,
        ultra_footings: Vec::new(),
        puddle_carve: None,
    };
    assert_eq!(
        terrain.plane_shift(),
        spec["plane_shift"].as_u64().unwrap() as u32
    );
    let x = spec["xz"][0].as_u64().unwrap() as u16;
    let z = spec["xz"][1].as_u64().unwrap() as u16;
    let mask = spacing.units() - 1;
    let step = usize::from(spacing.units() >> 7);
    let gx = usize::from((x >> 7) & !(mask >> 7));
    let gz = usize::from((z >> 7) & !(mask >> 7));
    let cells = [
        (gx, gz),
        ((gx + step) & 511, gz),
        ((gx + step) & 511, (gz + step) & 511),
        (gx, (gz + step) & 511),
    ];
    for (index, (x, z)) in cells.into_iter().enumerate() {
        let height = spec["heights"][index].as_i64().unwrap() as i8 as u8;
        let diagonal = if index == 0 && spec["alternate"].as_bool().unwrap() {
            0x800
        } else {
            0
        };
        terrain.cells[x * TERRAIN_GRID_SIDE + z] =
            TerrainCell::from_raw(u16::from(height) | diagonal);
    }
    terrain
}

#[test]
fn native_four_height_probe_matches_all_supported_policies() {
    // Reproduce with check_terrain_slope_probe.py. The native
    // shift domain is the owned loader pair (height4/plane9, height5/plane8).
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/terrain_slope_probe_native.json"
    ))
    .unwrap();
    let cases = fixture["terrain_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 934);
    for (index, spec) in cases.iter().enumerate() {
        let terrain = fixture_terrain(spec);
        let plane = if spec["clamp"].as_u64().unwrap() as u8 != 0 {
            TerrainProbePlanePolicy::ClampToPlane
        } else if spec["reject"].as_u64().unwrap() as u8 != 0 {
            TerrainProbePlanePolicy::RejectBelow
        } else {
            TerrainProbePlanePolicy::Unmodified
        };
        let result = terrain.probe_slope(TerrainSlopeProbeRequest {
            xz: std::array::from_fn(|i| spec["xz"][i].as_u64().unwrap() as u16),
            maximum_step: spec["maximum_step"].as_i64().unwrap() as i32,
            plane,
        });
        assert_eq!(
            result.blocked,
            spec["result"].as_u64().unwrap() != 0,
            "case {index}: {spec}"
        );
        assert_eq!(
            result.position.xz,
            std::array::from_fn(|i| spec["temporary"]["xz"][i].as_u64().unwrap() as u16),
            "XZ case {index}"
        );
        assert_eq!(
            result.position.y,
            spec["temporary"]["y"].as_u64().map(|y| y as u16),
            "Y case {index}"
        );
    }
}
