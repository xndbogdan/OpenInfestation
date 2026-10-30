use super::*;
use crate::terrain_runtime::{TerrainSampling, TerrainSpacing};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TerrainCell,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

fn terrain_with_cells(
    sampling: TerrainSampling,
    cell: impl Fn(usize, usize) -> TerrainCell,
) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for x in 0..TERRAIN_GRID_SIDE {
        for z in 0..TERRAIN_GRID_SIDE {
            let cell = cell(x, z);
            assert!(!cell.has_sparse_record());
            let offset = TERRAIN_PREFIX_BYTES + (x * TERRAIN_GRID_SIDE + z) * 2;
            bytes[offset..offset + 2].copy_from_slice(&cell.raw().to_le_bytes());
        }
    }
    let span = ByteSpan::new(0, bytes.len());
    let source = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(&source.entries[0], sampling)
}

#[test]
fn plane_policy_uses_only_its_world_flag() {
    for flags in [0, 0x80, 0x2000, 0xffff_ffdf] {
        assert_eq!(
            TerrainMapPlane::from_world_flags(flags, -1234),
            TerrainMapPlane::Ignore
        );
        assert_eq!(
            TerrainMapPlane::from_world_flags(flags | 0x20, -1234),
            TerrainMapPlane::Classify {
                height_fixed: -1234
            }
        );
    }
}

#[test]
fn rng_visits_columns_while_map_bytes_store_rows() {
    for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
        let terrain = terrain_with_cells(
            TerrainSampling {
                spacing,
                height_scale: TerrainHeightScale::Units32,
            },
            |_, _| TerrainCell::from_raw(0x1300),
        );
        let original = terrain.cells().to_vec();
        let mut rng = 0;
        let map = TerrainMap::new(&terrain, TerrainMapPlane::Ignore, &mut rng);
        assert_eq!(map.indices().len(), TERRAIN_MAP_CELLS);
        assert_eq!(rng, 0x47b1_0000);
        // Fixed LCG prefix, including distinct positions that a row-major
        // traversal would transpose. The native fixture pins the whole map.
        for (coordinate, value) in [
            ([0, 0], 9),
            ([0, 1], 9),
            ([0, 2], 4),
            ([0, 3], 4),
            ([0, 255], 9),
            ([1, 0], 9),
            ([1, 1], 4),
            ([1, 255], 9),
            ([255, 255], 9),
        ] {
            assert_eq!(map.index_at(coordinate), value, "{coordinate:?}");
        }
        assert_eq!(map.indices()[256], map.index_at([0, 1]));
        assert_eq!(terrain.cells(), original);
    }
}

#[test]
fn signed_plane_threshold_rounds_down_and_keeps_equality_as_material() {
    for (height_scale, plane_shift, height_units) in [
        (TerrainHeightScale::Units16, 9, 16),
        (TerrainHeightScale::Units32, 8, 32),
    ] {
        let terrain = terrain_with_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale,
            },
            |x, z| {
                let height = match (x / 2, z / 2) {
                    (0, 0) => -1_i8,
                    (0, 1) => -128,
                    (0, 2) => 127,
                    _ => 0,
                };
                TerrainCell::from_raw(0x0200 | u16::from(height as u8))
            },
        );
        for (height_fixed, expected) in [
            (-height_units << plane_shift, 3),
            ((-height_units << plane_shift) + 1, 3),
            ((-height_units + 1) << plane_shift, 0),
        ] {
            let mut rng = 0;
            let map = TerrainMap::new(
                &terrain,
                TerrainMapPlane::Classify { height_fixed },
                &mut rng,
            );
            assert_eq!(map.index_at([0, 0]), expected);
            assert_eq!(map.index_at([0, 1]), 0); // signed -128, not +128
            assert_eq!(map.index_at([0, 2]), 3);
            // Zero shade makes every RNG result visually irrelevant, but
            // classification and shade still consume the complete sequence.
            assert_eq!(rng, 0x47b1_0000);
        }
        for (height_fixed, expected) in [(i32::MIN, 3), (i32::MAX, 0)] {
            let map = TerrainMap::new(&terrain, TerrainMapPlane::Classify { height_fixed }, &mut 0);
            assert!(map.indices().iter().all(|&index| index == expected));
        }
    }
}

#[test]
fn shade_and_material_choose_five_entry_groups() {
    let terrain = terrain_with_cells(
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
        |x, z| TerrainCell::from_raw(((x / 2 % 16) as u16) << 12 | ((z / 2 % 4) as u16) << 8),
    );
    let map = TerrainMap::new(&terrain, TerrainMapPlane::Ignore, &mut 0);
    for x in 0..TERRAIN_MAP_SIDE {
        for z in 0..TERRAIN_MAP_SIDE {
            let value = map.index_at([x as u8, z as u8]);
            assert!((1..40).contains(&value));
            let material = (z % 4 + 1) as u8;
            assert_eq!(value % 5, material);
            let shade = x % 16;
            let group = usize::from(value / 5);
            if shade % 2 == 0 || shade == 15 {
                assert_eq!(group, shade / 2);
            } else {
                assert!([shade / 2, shade / 2 + 1].contains(&group));
            }
        }
    }
}
