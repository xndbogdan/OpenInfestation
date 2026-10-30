use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling};
use inf_formats::{TerrainCell, TERRAIN_CELL_COUNT, TERRAIN_GRID_SIDE};
use sha2::{Digest, Sha256};

fn terrain(spacing: TerrainSpacing) -> TerrainRuntime {
    TerrainRuntime {
        sampling: TerrainSampling {
            spacing,
            height_scale: TerrainHeightScale::Units32,
        },
        cells: (0..TERRAIN_CELL_COUNT)
            .map(|index| {
                TerrainCell::from_raw(((index % 4) as u16) << 12 | (index as u16 & 255) | 0x800)
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        buckets: Vec::new(),
        plane_base: -128_000,
        slope_factors: [123, -456],
        light_direction_raw: [0; 3],
        ultra: None,
        ultra_footings: Vec::new(),
        puddle_carve: None,
    }
}

#[test]
fn terrain_policy_native_receipts_and_startup_census() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/terrain_policy_native.json"
    ))
    .unwrap();
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        fixture["exe_sha256"].as_str().unwrap()
    );
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let initial = ProcessTerrainPolicy::native_startup(executable).unwrap();
    assert_eq!(
        initial.spacing().units(),
        fixture["startup_pair"][0].as_u64().unwrap() as u16
    );
    for guard in fixture["guards"].as_array().unwrap() {
        let address = guard[0].as_u64().unwrap() as u32;
        let hex = guard[1].as_str().unwrap();
        let expected: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(
            &raw[(address - 0x0040_0000) as usize
                ..(address - 0x0040_0000) as usize + expected.len()],
            expected
        );
    }
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let spacing = if input["spacing"] == 128 {
            TerrainSpacing::Units128
        } else {
            TerrainSpacing::Units256
        };
        let mut policy = ProcessTerrainPolicy::supplied(spacing);
        let mut terrain = terrain(spacing);
        let detail = input["detail"].as_u64().unwrap() as u32;
        let flags = input["flags"].as_u64().map(|value| value as u32);
        match input["mode"].as_str().unwrap() {
            "configure" => TerrainDetailContext {
                process: &mut policy,
                world_flags: flags,
                terrain: Some(&mut terrain),
            }
            .configure(detail)
            .unwrap(),
            "temporary" => policy.temporary_fine(&mut terrain),
            "restore" => policy.restore_detail(detail, flags, &mut terrain),
            _ => unreachable!(),
        }
        assert_eq!(
            u64::from(policy.spacing().units()),
            case["pair"][0].as_u64().unwrap(),
            "case{index}"
        );
        assert_eq!(policy.spacing(), terrain.sampling().spacing, "case{index}");
        // Getter/prepare calls are supplied by the native receipt. They do not
        // claim a portable pointer identity or lazy-resource implementation.
        if input["mode"] == "configure" {
            let mut stopped = ProcessTerrainPolicy::supplied(spacing);
            assert!(matches!(
                TerrainDetailContext {
                    process: &mut stopped,
                    world_flags: flags,
                    terrain: None
                }
                .configure(detail),
                Err(TerrainPolicyError::MissingTerrain)
            ));
            assert_eq!(
                stopped.spacing(),
                policy.spacing(),
                "failed preparation case{index}"
            );
            assert_eq!(
                case["events"].as_array().unwrap().last().unwrap()["kind"],
                "prepare"
            );
        }
    }
}

#[test]
fn terrain_policy_reprepares_current_cells_and_temporary_scopes_do_not() {
    let mut policy = ProcessTerrainPolicy::supplied(TerrainSpacing::Units128);
    let mut terrain = terrain(policy.spacing());
    // Runtime writes are deliberately different from any authored H12 source.
    terrain.cells[0] = TerrainCell::from_raw(0x307f);
    // This odd-cell patch has uniform material zero, whose native identity orientation sets
    // the diagonal. Coarse preparation never visits its odd origin.
    terrain.cells[TERRAIN_GRID_SIDE + 1] = TerrainCell::from_raw(0x2091);
    let before = terrain.cells.clone();
    let slope = terrain.slope_factors;
    TerrainDetailContext {
        process: &mut policy,
        world_flags: Some(0x20),
        terrain: Some(&mut terrain),
    }
    .configure(0)
    .unwrap();
    assert_eq!(
        terrain.cells[TERRAIN_GRID_SIDE + 1],
        before[TERRAIN_GRID_SIDE + 1]
    );
    assert!(before
        .iter()
        .zip(terrain.cells.iter())
        .any(|(old, new)| old.raw() != new.raw()));
    assert!(before
        .iter()
        .zip(terrain.cells.iter())
        .all(|(old, new)| (old.raw() ^ new.raw()) & !0x800 == 0));
    assert_eq!(terrain.plane_base, -128_000);
    assert_eq!(terrain.slope_factors, slope);
    let coarse = terrain.cells.clone();
    policy.temporary_fine(&mut terrain);
    policy.restore_detail(2, Some(0x20), &mut terrain);
    assert_eq!(policy.spacing(), TerrainSpacing::Units128);
    assert_eq!(terrain.cells, coarse);
    //4502C0 still prepares when the selected pair is already fine.
    TerrainDetailContext {
        process: &mut policy,
        world_flags: Some(0x20),
        terrain: Some(&mut terrain),
    }
    .configure(1)
    .unwrap();
    // Native remap[Identity]=7 selects orientation 0x00112233; low nibble 3
    // sets bit 11. The initial clear bit survives coarse, then becomes set.
    assert_eq!(coarse[TERRAIN_GRID_SIDE + 1].raw(), 0x2091);
    assert_eq!(terrain.cells[TERRAIN_GRID_SIDE + 1].raw(), 0x2891);
    policy.restore_detail(1, Some(0x2020), &mut terrain);
    assert_eq!(policy.spacing(), TerrainSpacing::Units256);
}

#[test]
fn terrain_policy_load_retains_pair_except_header_override() {
    let mut policy = ProcessTerrainPolicy::supplied(TerrainSpacing::Units128);
    assert_eq!(policy.load_world(0x20), TerrainSpacing::Units128);
    assert_eq!(policy.load_world(0xa0), TerrainSpacing::Units128);
    assert_eq!(policy.load_world(0x2020), TerrainSpacing::Units256);
    assert_eq!(policy.load_world(0x20), TerrainSpacing::Units256);
}
