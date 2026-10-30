use std::path::Path;

use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TerrainCell,
    TerrainEntry, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_GRID_SIDE,
    TERRAIN_PREFIX_BYTES,
};
use inf_game::{
    terrain_map::{TerrainMap, TerrainMapPlane},
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    world_placement::WorldPlacements,
    HandlerResources, Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    synthetic: Vec<Case>,
    corpus: Vec<CorpusCase>,
    single_cells: Vec<CellCase>,
    captured: Vec<CorpusCase>,
}

#[derive(Deserialize)]
struct CellCase {
    raw_word: u16,
    height_shift: u32,
    spacing: u16,
    plane_mode: u8,
    plane_height_fixed: i32,
    seed: u32,
    map_index: u8,
}

#[derive(Deserialize)]
struct Case {
    spacing: u16,
    height_shift: u32,
    plane_mode: u8,
    plane_height_fixed: i32,
    seed: u32,
    final_rng: u32,
    input_cells_fnv1a64: String,
    map_fnv1a64: String,
    probes: Vec<[usize; 2]>,
    histogram: Vec<usize>,
}

#[derive(Deserialize)]
struct CorpusCase {
    leaf: u8,
    flags: u32,
    initial_spacing: u16,
    #[serde(flatten)]
    map: Case,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/terrain_map_native.json")).unwrap()
}

fn spacing(units: u16) -> TerrainSpacing {
    match units {
        128 => TerrainSpacing::Units128,
        256 => TerrainSpacing::Units256,
        _ => panic!("unknown fixture spacing"),
    }
}

fn hash(bytes: impl IntoIterator<Item = u8>) -> String {
    let hash = bytes
        .into_iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{hash:016x}")
}

fn check_map(terrain: &TerrainRuntime, plane: TerrainMapPlane, case: &Case) {
    assert_eq!(terrain.sampling().spacing.units(), case.spacing);
    assert_eq!(
        terrain.sampling().height_scale,
        match case.height_shift {
            4 => TerrainHeightScale::Units16,
            5 => TerrainHeightScale::Units32,
            _ => panic!("unknown fixture height shift"),
        }
    );
    let before = terrain.cells().to_vec();
    let mut state = case.seed;
    let map = TerrainMap::new(terrain, plane, &mut state);
    assert_eq!(state, case.final_rng, "complete native RNG endpoint");
    assert_eq!(hash(map.indices().iter().copied()), case.map_fnv1a64);
    let mut histogram = [0; 40];
    for &index in map.indices() {
        histogram[usize::from(index)] += 1;
    }
    assert_eq!(histogram.as_slice(), case.histogram);
    for &[index, value] in &case.probes {
        assert_eq!(map.indices()[index], value as u8);
        assert_eq!(
            map.index_at([(index % 256) as u8, (index / 256) as u8]),
            value as u8
        );
    }
    assert_eq!(terrain.cells(), before);
}

fn dense_source() -> TerrainEntry {
    // Arbitrary test input shared with the native harness, not expected map output.
    let cells: Vec<_> = (0..512_u32)
        .flat_map(|x| {
            (0..512_u32).map(move |z| {
                let height = (37 * x + 61 * z + 13 * (x ^ z) + 17) & 255;
                let metadata = (19 * x + 23 * z + ((x * z) >> 3) + 0xa5) & 255;
                TerrainCell::from_raw((height | metadata << 8) as u16)
            })
        })
        .collect();
    source_from_cells(&cells)
}

fn source_from_cells(cells: &[TerrainCell]) -> TerrainEntry {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for (index, cell) in cells.iter().enumerate() {
        let at = TERRAIN_PREFIX_BYTES + index * 2;
        bytes[at..at + 2].copy_from_slice(&cell.raw().to_le_bytes());
    }
    // Keep the synthetic source valid for the canonical parser's sparse custody.
    for z in 0..TERRAIN_GRID_SIDE {
        let mut size = 0_u16;
        for x in 0..TERRAIN_GRID_SIDE {
            if cells[x * TERRAIN_GRID_SIDE + z].has_sparse_record() {
                bytes.extend_from_slice(&(x as u16).to_le_bytes());
                bytes.extend_from_slice(&[0, 0]);
                size += 4;
            }
        }
        let at = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2;
        bytes[at..at + 2].copy_from_slice(&size.to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap()
    .entries
    .remove(0)
}

#[test]
fn native_single_cell_receipts_pin_signed_plane_edges_and_shade_clamping() {
    let cases = fixture().single_cells;
    assert_eq!(cases.len(), 96);
    for case in cases {
        let source = source_from_cells(&vec![
            TerrainCell::from_raw(case.raw_word);
            TERRAIN_CELL_COUNT
        ]);
        let terrain = TerrainRuntime::new(
            &source,
            TerrainSampling {
                spacing: spacing(case.spacing),
                height_scale: match case.height_shift {
                    4 => TerrainHeightScale::Units16,
                    5 => TerrainHeightScale::Units32,
                    _ => panic!("unknown height shift"),
                },
            },
        );
        let mut rng = case.seed;
        let map = TerrainMap::new(
            &terrain,
            TerrainMapPlane::from_world_flags(
                u32::from(case.plane_mode) << 5,
                case.plane_height_fixed,
            ),
            &mut rng,
        );
        // Native receipt executes 00471C40 once at 0,0; the production map's
        // first cell sees exactly that initial RNG state. Whole-loop receipts
        // above independently pin the final RNG after all 65536 advances.
        assert_eq!(map.index_at([0, 0]), case.map_index);
    }
}

#[test]
fn complete_synthetic_maps_match_the_unmodified_native_loop() {
    let source = dense_source();
    let input_hash = hash(
        source
            .cells()
            .iter()
            .flat_map(|cell| cell.raw().to_le_bytes()),
    );
    let cases = fixture().synthetic;
    assert_eq!(cases.len(), 24);
    for case in cases {
        assert_eq!(input_hash, case.input_cells_fnv1a64);
        let terrain = TerrainRuntime::new(
            &source,
            TerrainSampling {
                spacing: spacing(case.spacing),
                height_scale: if case.height_shift == 4 {
                    TerrainHeightScale::Units16
                } else {
                    TerrainHeightScale::Units32
                },
            },
        );
        check_map(
            &terrain,
            TerrainMapPlane::from_world_flags(
                u32::from(case.plane_mode) << 5,
                case.plane_height_fixed,
            ),
            &case,
        );
    }
}

#[test]
fn resident_world_maps_match_native_and_the_recorded_first_load() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let fixture = fixture();
    let mut cases = fixture.corpus;
    assert_eq!(cases.len(), 66);
    assert_eq!(fixture.captured.len(), 1);
    assert_eq!(fixture.captured[0].leaf, 32);
    assert_eq!(fixture.captured[0].map.seed, 0x3e23_e340);
    assert_eq!(fixture.captured[0].map.final_rng, 0x9ad4_e340);
    cases.extend(fixture.captured);
    for case in cases {
        runtime
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        runtime
            .apply(ResidencyRequest::Activate { level: case.leaf })
            .unwrap();
        let revision = runtime.revision();
        let placements = WorldPlacements::new(&runtime, spacing(case.initial_spacing)).unwrap();
        assert_eq!(placements.world().flags, case.flags);
        assert_eq!((case.flags >> 5) & 1, u32::from(case.map.plane_mode));
        let source = placements.terrain_source();
        let original_bytes = source.overlay.bytes().to_vec();
        let HandlerResources::Terrain(bank) =
            &source.overlay.handlers()[source.registration.region_index]
        else {
            panic!("registered terrain");
        };
        let raw = &bank.entries[usize::from(source.registration.local_index)];
        assert_eq!(raw.prefix.plane_height_fixed, case.map.plane_height_fixed);
        assert_eq!(
            hash(raw.cells().iter().flat_map(|cell| cell.raw().to_le_bytes())),
            case.map.input_cells_fnv1a64
        );
        check_map(
            placements.terrain(),
            TerrainMapPlane::from_world_flags(case.flags, raw.prefix.plane_height_fixed),
            &case.map,
        );
        assert_eq!(source.overlay.bytes(), original_bytes);
        assert_eq!(runtime.revision(), revision);
    }
}
