//! Supplied prepared cells and complete nonaliased DWORD vectors admit the
//! unchanged native44FFA0 receipts. Native raw-pointer/alias stops and process
//! sampling pairs outside the owned loader domain remain explicit diagnostics.

use super::*;
use serde_json::Value;

fn dword(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn xyz(value: &Value) -> [i32; 3] {
    std::array::from_fn(|index| dword(&value[index]) as i32)
}

fn supplied_terrain(input: &Value) -> TerrainRuntime {
    let sampling = TerrainSampling {
        spacing: match input["spacing"].as_u64().unwrap() {
            128 => TerrainSpacing::Units128,
            256 => TerrainSpacing::Units256,
            value => panic!("unsupported spacing {value}"),
        },
        height_scale: match input["height_shift"].as_u64().unwrap() {
            4 => TerrainHeightScale::Units16,
            5 => TerrainHeightScale::Units32,
            value => panic!("unsupported height shift {value}"),
        },
    };
    // The native checker supplies already selected triangle bits. It does not
    // execute H12 resource preparation, so neither does this fixture adapter.
    let grid = &input["grid"];
    let mut cells: Vec<_> = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE)
        .map(|index| {
            let height = match grid["kind"].as_str().unwrap() {
                "flat" => grid["background"].as_i64().unwrap(),
                "affine" => {
                    (index / TERRAIN_GRID_SIDE * 13 + index % TERRAIN_GRID_SIDE * 7) as i64
                        + grid["bias"].as_i64().unwrap()
                }
                value => panic!("unknown grid {value}"),
            };
            TerrainCell::from_raw(u16::from(height as u8))
        })
        .collect();
    let [x, _, z] = xyz(&input["source_xyz"]);
    let units = sampling.spacing.units();
    let step = usize::from(units >> 7);
    let mask = units - 1;
    let x0 = usize::from(((x as u16) >> 7) & !(mask >> 7));
    let z0 = usize::from(((z as u16) >> 7) & !(mask >> 7));
    let corners = [
        (x0, z0),
        ((x0 + step) & 511, z0),
        ((x0 + step) & 511, (z0 + step) & 511),
        (x0, (z0 + step) & 511),
    ];
    for (corner, (gx, gz)) in corners.into_iter().enumerate() {
        let index = gx * TERRAIN_GRID_SIDE + gz;
        let height = grid["patch_heights"]
            .as_array()
            .map(|patch| patch[corner].as_i64().unwrap() as u8)
            .unwrap_or(cells[index].height_i8() as u8);
        let triangle = if corner == 0 && grid["alternate"] == true {
            0x800
        } else {
            0
        };
        cells[index] = TerrainCell::from_raw(u16::from(height) | triangle);
    }
    let terrain = TerrainRuntime {
        sampling,
        cells: cells.into_boxed_slice(),
        buckets: Vec::new(),
        plane_base: dword(&input["plane_base"]) as i32,
        slope_factors: [
            dword(&input["slope_x"]) as i32,
            dword(&input["slope_z"]) as i32,
        ],
        light_direction_raw: [0; 3],
        ultra: None,
        ultra_footings: Vec::new(),
        puddle_carve: None,
    };
    assert_eq!(terrain.plane_shift(), dword(&input["plane_shift"]));
    terrain
}

fn retail_sine() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    inf_formats::parse_frontend_sine_table(
        executable.data(),
        inf_formats::FRONTEND_SINE_TABLE_ADDRESS,
    )
    .unwrap()
    .values
}

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/terrain_project_44ffa0_native.json"
    ))
    .unwrap()
}

#[test]
fn project_44ffa0_matches_every_owned_native_receipt_and_real_helper_argument() {
    let fixture = fixture();
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(fixture["entry"], "0044FFA0");
    assert_eq!(fixture["height_entry"], "00450A30");
    assert_eq!(fixture["water_entry"], "00410F60");
    let sine = retail_sine();
    let mut compared = 0;
    let mut diagnostics = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        if input["portable_compare"] != true {
            assert!(input["source_only_reason"].as_str().is_some());
            diagnostics += 1;
            continue;
        }
        let terrain = supplied_terrain(input);
        let inputs = Project44FFA0Inputs {
            world_flags: dword(&input["world_flags"]),
            wave_clock: dword(&input["wave_clock"]),
            renderer_xyz: xyz(&input["renderer_xyz"]),
            table: Some(&sine),
        };
        assert_eq!(
            terrain.project_44ffa0(xyz(&input["source_xyz"]), &inputs),
            Ok(xyz(&case["output_xyz"])),
            "{}",
            input["name"]
        );
        for call in case["height_calls"].as_array().unwrap() {
            let xz = std::array::from_fn(|index| dword(&call["xz"][index]) as u16);
            assert_eq!(
                terrain.height_at(xz),
                dword(&call["result"]) as i16,
                "{} height arguments {:?}",
                input["name"],
                call["args"]
            );
        }
        for call in case["water_calls"].as_array().unwrap() {
            let args = call["args"].as_array().unwrap();
            assert_eq!(dword(&args[2]), inputs.wave_clock);
            assert_eq!(dword(&args[3]), inputs.world_flags & 0x40);
            let height = dword(&args[4]) as i32;
            assert_eq!(height, i32::from(height as i16));
            let request = TerrainWaterSurfaceRequest {
                xz: [dword(&args[0]) as u16, dword(&args[1]) as u16],
                wave_clock: dword(&args[2]),
                world_flags: dword(&args[3]),
                terrain_height: height as i16,
            };
            // Native helper EAX has retained upper bits; signed low AX is its
            // actual return contract consumed by44FFA0's signed comparisons.
            assert_eq!(
                terrain.water_surface_height(request, &sine),
                dword(&call["result"]) as i16,
                "{} water arguments {:?}",
                input["name"],
                args
            );
        }
        compared += 1;
    }
    assert_eq!((compared, diagnostics), (543, 26));
}

#[test]
fn project_44ffa0_requires_sine_only_at_a_reached_enabled_wave_gate() {
    let fixture = fixture();
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        if !input["name"].as_str().unwrap().starts_with("missing-sine-") {
            continue;
        }
        let terrain = supplied_terrain(input);
        let inputs = Project44FFA0Inputs {
            world_flags: dword(&input["world_flags"]),
            wave_clock: dword(&input["wave_clock"]),
            renderer_xyz: xyz(&input["renderer_xyz"]),
            table: None,
        };
        let actual = terrain.project_44ffa0(xyz(&input["source_xyz"]), &inputs);
        if case["stopped"].is_null() {
            assert_eq!(actual, Ok(xyz(&case["output_xyz"])), "{}", input["name"]);
        } else {
            assert_eq!(actual, Err(Project44FFA0Error::MissingTable));
            // The typed Result has no partially written output array. Native
            // second-call X/Z publication is retained only as a diagnostic.
            assert_eq!(case["water_calls"].as_array().unwrap().len(), 2);
            assert!(case["output_xyz"][0].is_number());
            assert!(case["output_xyz"][1].is_null());
            assert!(case["output_xyz"][2].is_number());
        }
        compared += 1;
    }
    assert_eq!(compared, 5);
}
