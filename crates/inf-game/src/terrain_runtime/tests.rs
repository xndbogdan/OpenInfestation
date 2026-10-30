use super::*;
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde::Deserialize;

#[test]
fn normalized_light_preserves_native_float_spills_and_output_bits() {
    // Unmodified 44FE17..44FE97 with the supported CRT control word 027F.
    // Reproduce with check_terrain_light.py. The first three
    // vectors are synthetic; the final two are original H12 vectors.
    let cases = [
        ([1, 2, 3], [0x3e88_d677, 0x3f08_d677, 0x3f4d_41b2]),
        ([-3, 4, -5], [0xbed9_3924, 0x3f10_d0c3, 0xbf35_04f3]),
        (
            [-16_777_217, 123_456_789, i32::MIN],
            [0xbbff_91fc, 0x3d6b_1470, 0xbf7f_91fb],
        ),
        (
            [459_959_721, 2_062_150_567, 383_671_161],
            [0x3e5b_5632, 0x3f75_d6f1, 0x3e36_f52b],
        ),
        (
            [-1_186_767_279, 1_318_630_310, -1_186_767_279],
            [0xbf0e_5773, 0x3f1e_2846, 0xbf0e_5772],
        ),
    ];
    for (vector, bits) in cases {
        assert_eq!(normalize_terrain_light(vector).map(f32::to_bits), bits);
    }
    assert_eq!(normalize_terrain_light([0; 3]), [1.0, 0.0, 0.0]);
}

/// A valid canonical H12 entry, including sparse records for marked cells.
fn source_from_cells(cells: &[TerrainCell]) -> TerrainEntry {
    assert_eq!(cells.len(), TERRAIN_CELL_COUNT);
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for (index, cell) in cells.iter().enumerate() {
        let offset = TERRAIN_PREFIX_BYTES + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&cell.raw().to_le_bytes());
    }
    for z in 0..TERRAIN_GRID_SIDE {
        let mut count = 0_u16;
        for x in 0..TERRAIN_GRID_SIDE {
            if cells[x * TERRAIN_GRID_SIDE + z].has_sparse_record() {
                bytes.extend_from_slice(&(x as u16).to_le_bytes());
                bytes.extend_from_slice(&[0, 0]);
                count += 1;
            }
        }
        let offset = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2;
        bytes[offset..offset + 2].copy_from_slice(&(count * 4).to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    let mut bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    bank.entries.remove(0)
}

#[test]
fn raw_cell_height_keeps_signed_world_units_and_does_not_interpolate() {
    let mut cells = vec![TerrainCell::from_raw(0x00ff); TERRAIN_CELL_COUNT];
    cells[TERRAIN_GRID_SIDE] = TerrainCell::from_raw(1);
    let source = source_from_cells(&cells);
    for (height_scale, units) in [
        (TerrainHeightScale::Units16, 16_i16),
        (TerrainHeightScale::Units32, 32_i16),
    ] {
        let terrain = TerrainRuntime::new(
            &source,
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale,
            },
        );
        assert_eq!(terrain.raw_height_at([0, 0]), -units);
        assert_eq!(terrain.raw_height_at([127, 0]), -units);
        assert_eq!(terrain.raw_height_at([128, 0]), units);
        assert_eq!(terrain.height_at([64, 0]), 0);
        assert_eq!(terrain.raw_height_at([64, 0]), -units);
    }
}

#[test]
fn world_flags_keep_horizontal_and_vertical_policies_independent() {
    for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
        assert_eq!(spacing.for_world(0), spacing);
        assert_eq!(spacing.for_world(0x80), spacing);
        assert_eq!(spacing.for_world(0x2000), TerrainSpacing::Units256);
        assert_eq!(spacing.for_world(0x2080), TerrainSpacing::Units256);
    }
    assert_eq!(
        TerrainHeightScale::from_world_flags(0),
        TerrainHeightScale::Units32
    );
    assert_eq!(
        TerrainHeightScale::from_world_flags(0x2000),
        TerrainHeightScale::Units32
    );
    assert_eq!(
        TerrainHeightScale::from_world_flags(0x80),
        TerrainHeightScale::Units16
    );
    assert_eq!(
        TerrainHeightScale::from_world_flags(0x2080),
        TerrainHeightScale::Units16
    );
}

#[test]
fn water_surface_uses_the_h12_plane_shift_and_h14_wave_gate() {
    let mut source = source_from_cells(&vec![TerrainCell::from_raw(0); TERRAIN_CELL_COUNT]);
    source.prefix.plane_height_fixed = -256_000;
    let request = TerrainWaterSurfaceRequest {
        xz: [0xffff, 0x8000],
        wave_clock: u32::MAX,
        world_flags: 0x80,
        terrain_height: -3000,
    };
    let table = [32767; 4096];
    for (height_scale, expected) in [
        (TerrainHeightScale::Units32, -1000),
        (TerrainHeightScale::Units16, -500),
    ] {
        let terrain = TerrainRuntime::new(
            &source,
            TerrainSampling {
                spacing: TerrainSpacing::Units256,
                height_scale,
            },
        );
        // World flag80 changes neither the already-selected plane shift nor
        // the wave gate, which consumes flag40 specifically.
        assert_eq!(terrain.water_surface_height(request, &table), expected);
    }
}

#[test]
fn water_surface_preserves_wave_clock_spatial_signs_and_terrain_clamp() {
    let mut source = source_from_cells(&vec![TerrainCell::from_raw(0); TERRAIN_CELL_COUNT]);
    source.prefix.plane_height_fixed = 1000 << 8;
    let mut terrain = TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    );
    let table = [32767; 4096];
    let request = TerrainWaterSurfaceRequest {
        xz: [0, 0],
        wave_clock: 0,
        world_flags: 0x40,
        terrain_height: 0,
    };
    // 410F60: ((32767+32767)>>5)+(32767>>6)=2558;
    // signed 2558*(1000-0+512)>>15=118, then add plane1000.
    assert_eq!(terrain.water_surface_height(request, &table), 1118);
    assert_eq!(
        terrain.water_surface_height(
            TerrainWaterSurfaceRequest {
                wave_clock: 256,
                ..request
            },
            &table,
        ),
        1023
    );
    assert_eq!(
        terrain.water_surface_height(
            TerrainWaterSurfaceRequest {
                wave_clock: 256 + 1024,
                ..request
            },
            &table,
        ),
        1023
    );
    // At/above the plane, the sine block is skipped before any table read.
    for terrain_height in [1000, 1001] {
        assert_eq!(
            terrain.water_surface_height(
                TerrainWaterSurfaceRequest {
                    terrain_height,
                    ..request
                },
                &table,
            ),
            1000
        );
    }
    terrain.plane_base = 100 << 8;
    // x2000/z0 puts all three phases in the negative sine half-turn.
    // The resulting59 is below terrain99 and native returns the terrain.
    assert_eq!(
        terrain.water_surface_height(
            TerrainWaterSurfaceRequest {
                xz: [2000, 0],
                terrain_height: 99,
                ..request
            },
            &table,
        ),
        99
    );
}

#[test]
fn preparation_owns_cells_and_changes_only_sampled_diagonal_bits() {
    let cells: Vec<_> = (0..TERRAIN_CELL_COUNT)
        .map(|index| TerrainCell::from_raw((index as u16).wrapping_mul(1237)))
        .collect();
    let source = source_from_cells(&cells);
    for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
        let terrain = TerrainRuntime::new(
            &source,
            TerrainSampling {
                spacing,
                height_scale: TerrainHeightScale::Units32,
            },
        );
        assert_eq!(source.cells(), cells);
        assert_ne!(source.cells().as_ptr(), terrain.cells().as_ptr());
        let mut changed = 0;
        for (index, (&original, &prepared)) in cells.iter().zip(terrain.cells()).enumerate() {
            assert_eq!(original.raw() & !0x800, prepared.raw() & !0x800);
            if spacing == TerrainSpacing::Units256
                && (index / TERRAIN_GRID_SIDE % 2 != 0 || index % TERRAIN_GRID_SIDE % 2 != 0)
            {
                assert_eq!(prepared, original, "unvisited cell {index}");
            }
            changed += usize::from(original != prepared);
        }
        assert!(changed > 0);
    }
}

#[test]
fn slopes_round_separately_and_wrap_to_the_first_grid_row() {
    let mut terrain = TerrainRuntime {
        sampling: TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
        cells: vec![TerrainCell::from_raw(0); TERRAIN_CELL_COUNT].into_boxed_slice(),
        buckets: Vec::new(),
        plane_base: 0,
        slope_factors: compute_slope_factors([0, 0, 0]),
        light_direction_raw: [0; 3],
        ultra: None,
        ultra_footings: Vec::new(),
        puddle_carve: None,
    };
    // Ordinary diagonal, origin X=511*128, Z=0. X+128 wraps to grid X=0.
    for ([x, z], height) in [([511, 0], 0_i8), ([0, 0], -1), ([0, 1], 2), ([511, 1], 3)] {
        terrain.cells[x * TERRAIN_GRID_SIDE + z] = TerrainCell::from_raw(height as u8 as u16);
    }
    // floor(-32*1/128) + floor(96*127/128) = -1 + 95.
    // A combined product would incorrectly return 95.
    assert_eq!(terrain.height_at([65409, 127]), 94);
    assert_eq!(terrain.height_at([65408, 0]), 0);
    assert_eq!(terrain.height_at([0, 0]), -32);
    assert_eq!(terrain.height_at([0, 128]), 64);
}

#[derive(Deserialize)]
struct NativeFixture {
    dense_pattern: DensePattern,
    preparation: Vec<PreparedReceipt>,
    sampling: Vec<SampleReceipt>,
    isolated_sampling: Vec<IsolatedReceipt>,
}

#[derive(Deserialize)]
struct DensePattern {
    input_cells_fnv1a64: String,
}

#[derive(Deserialize)]
struct PreparedReceipt {
    spacing: u16,
    step_argument: u16,
    cells_fnv1a64: String,
    diagonal_bytes_fnv1a64: String,
    alternate_count: usize,
    changed_cells: usize,
    cell_probes: Vec<[u16; 3]>,
}

#[derive(Deserialize)]
struct SampleReceipt {
    spacing: u16,
    height_shift: u8,
    samples: Vec<(u32, u32, u32, i16)>,
}

impl SampleReceipt {
    fn policy(&self) -> TerrainSampling {
        TerrainSampling {
            spacing: receipt_spacing(self.spacing),
            height_scale: match self.height_shift {
                4 => TerrainHeightScale::Units16,
                5 => TerrainHeightScale::Units32,
                _ => panic!("unsupported fixture height scale"),
            },
        }
    }

    fn check(&self, terrain: &TerrainRuntime) {
        for &(x, z, _unused, native) in &self.samples {
            assert_eq!(
                terrain.height_at([x as u16, z as u16]),
                native,
                "native height at {x:#x}/{z:#x}, {:?}",
                terrain.sampling()
            );
        }
    }
}

#[derive(Deserialize)]
struct IsolatedReceipt {
    #[serde(flatten)]
    sampling: SampleReceipt,
    alternate: bool,
    base_world: [u16; 2],
    /// Native corner order: origin, +X, +X+Z, +Z.
    corner_heights_abcd: [i8; 4],
}

fn fixture() -> NativeFixture {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/terrain_runtime_native.json"
    ))
    .unwrap()
}

fn receipt_spacing(units: u16) -> TerrainSpacing {
    match units {
        128 => TerrainSpacing::Units128,
        256 => TerrainSpacing::Units256,
        _ => panic!("unsupported fixture spacing"),
    }
}

fn fnv(bytes: impl IntoIterator<Item = u8>) -> String {
    let value = bytes
        .into_iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("{value:016x}")
}

/// Synthetic input specified in the fixture's provenance. Material overrides
/// put every packed four-corner combination in both fine and coarse quads.
fn native_pattern() -> Vec<TerrainCell> {
    (0..TERRAIN_CELL_COUNT)
        .map(|index| {
            let x = index / TERRAIN_GRID_SIDE;
            let z = index % TERRAIN_GRID_SIDE;
            let height = (37 * x + 61 * z + 13 * (x ^ z) + 17) as u8;
            let mut metadata = (19 * x + 23 * z + ((x * z) >> 3) + 0xa5) as u8;
            if x < 128 && z < 128 {
                let [lx, lz] = [x % 8, z % 8];
                let digit = if lx < 2 && lz < 2 {
                    Some(2 * lx + lz)
                } else if matches!(lx, 4 | 6) && matches!(lz, 4 | 6) {
                    Some(lx - 4 + (lz - 4) / 2)
                } else {
                    None
                };
                if let Some(digit) = digit {
                    let combo = x / 8 + 16 * (z / 8);
                    metadata = (metadata & !3) | ((combo >> (2 * digit)) & 3) as u8;
                }
            }
            TerrainCell::from_raw(u16::from_le_bytes([height, metadata]))
        })
        .collect()
}

#[test]
fn native_preparation_matches_every_cell_and_all_material_combinations() {
    let fixture = fixture();
    let source_cells = native_pattern();
    assert_eq!(
        fnv(source_cells
            .iter()
            .flat_map(|cell| cell.raw().to_le_bytes())),
        fixture.dense_pattern.input_cells_fnv1a64
    );
    let source = source_from_cells(&source_cells);
    assert_eq!(fixture.preparation.len(), 4);
    for receipt in fixture.preparation {
        // The native default and an explicit equal step must agree. The port
        // implements only the default-step preparation used by the loader.
        assert!(receipt.step_argument == 0 || receipt.step_argument == receipt.spacing);
        let terrain = TerrainRuntime::new(
            &source,
            TerrainSampling {
                spacing: receipt_spacing(receipt.spacing),
                height_scale: TerrainHeightScale::Units32,
            },
        );
        assert_eq!(
            fnv(terrain
                .cells()
                .iter()
                .flat_map(|cell| cell.raw().to_le_bytes())),
            receipt.cells_fnv1a64,
            "spacing {}",
            receipt.spacing
        );
        assert_eq!(
            fnv(terrain
                .cells()
                .iter()
                .map(|cell| ((cell.raw() >> 11) & 1) as u8)),
            receipt.diagonal_bytes_fnv1a64
        );
        assert_eq!(
            terrain
                .cells()
                .iter()
                .filter(|cell| cell.raw() & 0x800 != 0)
                .count(),
            receipt.alternate_count
        );
        assert_eq!(
            terrain
                .cells()
                .iter()
                .zip(&source_cells)
                .filter(|(a, b)| a != b)
                .count(),
            receipt.changed_cells
        );
        for [x, z, raw] in receipt.cell_probes {
            assert_eq!(terrain.cell(usize::from(x), usize::from(z)).raw(), raw);
        }
        assert_eq!(source.cells(), source_cells);
    }
}

#[test]
fn native_heights_match_prepared_maps_and_isolated_triangle_edges() {
    let fixture = fixture();
    let source = source_from_cells(&native_pattern());
    let mut samples = 0;
    for receipt in fixture.sampling {
        let terrain = TerrainRuntime::new(&source, receipt.policy());
        receipt.check(&terrain);
        samples += receipt.samples.len();
    }
    for receipt in fixture.isolated_sampling {
        let sampling = receipt.sampling.policy();
        let [x0, z0] = receipt.base_world.map(|word| usize::from(word >> 7));
        let step = usize::from(sampling.spacing.units() >> 7);
        let x1 = (x0 + step) & 511;
        let z1 = (z0 + step) & 511;
        let mut terrain = TerrainRuntime {
            sampling,
            cells: vec![TerrainCell::from_raw(0); TERRAIN_CELL_COUNT].into_boxed_slice(),
            buckets: Vec::new(),
            plane_base: 0,
            slope_factors: compute_slope_factors([0, 0, 0]),
            light_direction_raw: [0; 3],
            ultra: None,
            ultra_footings: Vec::new(),
            puddle_carve: None,
        };
        for ([x, z], height) in [[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
            .into_iter()
            .zip(receipt.corner_heights_abcd)
        {
            terrain.cells[x * TERRAIN_GRID_SIDE + z] = TerrainCell::from_raw(height as u8 as u16);
        }
        let origin = &mut terrain.cells[x0 * TERRAIN_GRID_SIDE + z0];
        *origin = TerrainCell::from_raw(origin.raw() | (u16::from(receipt.alternate) << 11));
        receipt.sampling.check(&terrain);
        samples += receipt.sampling.samples.len();
    }
    assert_eq!(samples, 1024);
}

/// Flat synthetic terrain with an explicit plane datum for projector
/// tests. Uniform heights make every sample exact by construction.
fn flat_projector_terrain(height: i8, plane: i32) -> TerrainRuntime {
    let cells = vec![TerrainCell::from_raw(height as u8 as u16); TERRAIN_CELL_COUNT];
    let mut source = source_from_cells(&cells);
    source.prefix.plane_height_fixed = plane;
    TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

#[test]
fn projector_retains_plane_datum_and_shift_policy() {
    let terrain = flat_projector_terrain(7, 0x10000);
    assert_eq!(terrain.plane_base(), 0x10000);
    // Loader pairs 005372F8 5/4 with 005483BC 8/9 on the same flag.
    assert_eq!(terrain.plane_shift(), 8);
    let coarse = TerrainRuntime::new(
        &{
            let mut source = source_from_cells(&vec![TerrainCell::from_raw(0); TERRAIN_CELL_COUNT]);
            source.prefix.plane_height_fixed = 0;
            source
        },
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    assert_eq!(coarse.plane_shift(), 9);
}

#[test]
fn projector_copy_path_returns_renderer_origin_for_near_datum_vertices() {
    // Plane 0x10000 >> 8 = 256; flat height 7 is 224. Y=260 sits
    // within ±10 of the datum with terrain below it: copy path.
    let terrain = flat_projector_terrain(7, 0x10000);
    let inputs = Project44FFA0Inputs {
        world_flags: 0,
        wave_clock: 0,
        renderer_xyz: [1, 2, 3],
        table: None,
    };
    // No factors or table needed: the copy path touches neither.
    assert_eq!(
        terrain.project_44ffa0([1000, 260, 2000], &inputs),
        Ok([1, 2, 3])
    );
}

#[test]
fn projector_slope_path_scales_adjustment_by_factors() {
    // Y=200 is more than 10 below the datum with terrain below: slope.
    // Flat-terrain light is zero, so factors are the degenerate
    // [MIN+1, MIN+1]; delta = -24 gives adjustment +23.
    let terrain = flat_projector_terrain(7, 0x10000);
    assert_eq!(terrain.slope_factors(), [0x8000_0001u32 as i32; 2]);
    let inputs = Project44FFA0Inputs {
        world_flags: 0,
        wave_clock: 0,
        renderer_xyz: [1, 2, 3],
        table: None,
    };
    assert_eq!(
        terrain.project_44ffa0([1000, 200, 2000], &inputs),
        Ok([1023, 224, 2023])
    );
}

#[test]
fn slope_adjust_pins_high_dword_and_sign_bits() {
    // Hand-verified against the 64-bit multiply/shift/or shape.
    assert_eq!(super::slope_adjust(44, -2147483647), -44);
    assert_eq!(super::slope_adjust(-24, -2147483647), 23);
    assert_eq!(super::slope_adjust(0, 0x4000_0000), 0);
    assert_eq!(super::slope_adjust(1, 1), 0);
}

#[test]
fn projector_tail_path_resamples_around_factor_slopes() {
    // Y=300 clears the datum: tail height, early-return water sample (H14
    // clear, no table), factor slopes, resample, clamped spacing max.
    // delta = 44 gives adjustment -44, verified against slope_adjust.
    let terrain = flat_projector_terrain(7, 0x10000);
    let inputs = Project44FFA0Inputs {
        world_flags: 0,
        wave_clock: 0,
        renderer_xyz: [1, 2, 3],
        table: None,
    };
    assert_eq!(
        terrain.project_44ffa0([1000, 300, 2000], &inputs),
        Ok([956, 256, 1956])
    );
}

#[test]
fn projector_sine_paths_require_a_table_when_gated() {
    let terrain = flat_projector_terrain(7, 0x10000);
    // H14 flag40 set below the plane needs a table before any sine read.
    let inputs = Project44FFA0Inputs {
        world_flags: 0x40,
        wave_clock: 0,
        renderer_xyz: [1, 2, 3],
        table: None,
    };
    assert_eq!(
        terrain.project_44ffa0([1000, 300, 2000], &inputs),
        Err(Project44FFA0Error::MissingTable)
    );
    // With a table the sine paths run deterministically.
    let table = [0x1000u16; 4096];
    let inputs = Project44FFA0Inputs {
        table: Some(&table),
        ..inputs
    };
    let first = terrain.project_44ffa0([1000, 300, 2000], &inputs);
    assert!(first.is_ok());
    assert_eq!(first, terrain.project_44ffa0([1000, 300, 2000], &inputs));
}

#[test]
fn detail_selects_spacing_and_pair() {
    // Stateful raw0/1/retain and constructor Booleanization are covered by
    // policy's native receipts; no unsupported boot-fine inference remains.
    // `004502C0` static table behind both branches.
    assert_eq!(spacing_pair(TerrainSpacing::Units128), (0x80, 0x7f));
    assert_eq!(spacing_pair(TerrainSpacing::Units256), (0x100, 0xff));
}

/// Ultra ground detail (a port setting): on High the ground's height is
/// the fine grid's, retail's own at its grid points; on Low it is retail's
/// again; a written cell reaches the fine grid.
#[test]
fn ultra_ground_takes_the_height_on_high() {
    let cells = (0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE).map(|index| {
        // A bowl round grid point (12, 12).
        let [x, z] =
            [index / TERRAIN_GRID_SIDE, index % TERRAIN_GRID_SIDE].map(|at| at as i32 - 12);
        ((x * x + z * z) / 4).min(30) as u16
    });
    let sampling = TerrainSampling {
        spacing: TerrainSpacing::Units128,
        height_scale: TerrainHeightScale::Units32,
    };
    let mut terrain = TerrainRuntime::from_raw_cells(sampling, 0, cells);
    let between = [1280 + 64, 1280 + 32];
    let retail = terrain.height_at(between);
    terrain.set_ultra(Some(ultra::UltraShape {
        crease: 40.0,
        fill: 256.0,
        ease: 192.0,
    }));
    // Grid point (10, 10): byte (4 + 4) / 4 = 2, shifted by 5.
    assert_eq!(terrain.height_at([1280, 1280]), 2 * 32);
    let ultra = terrain.ultra().expect("High takes Ultra's ground");
    let fine = ultra.height_at(
        between,
        terrain.cell_at(between).stored_alternate_diagonal(),
    );
    assert_eq!(terrain.height_at(between), fine.round() as i16);
    assert_ne!(terrain.height_at(between), retail);
    // Low: retail's ground, Ultra's grid kept for High.
    let mut policy = ProcessTerrainPolicy::supplied(TerrainSpacing::Units128);
    policy.restore_detail(0, None, &mut terrain);
    assert!(terrain.ultra().is_none());
    policy.restore_detail(1, None, &mut terrain);
    assert!(terrain.ultra().is_some());
    // A written cell moves the fine points round it.
    terrain.write_raw_height_128([1280, 1280], 20 * 32);
    assert_eq!(terrain.ultra().unwrap().point(20, 20), 20.0 * 32.0);
    assert_eq!(terrain.height_at([1280, 1280]), 20 * 32);
}
