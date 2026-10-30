//! Focused `0044CD00` transcription tests: range selection, wrapping,
//! the odd-x flag rule, per-row z decrement, fail-closed bounds.
//! Coordinates below use scale 128 / mask 0x7F, where a coord word
//! maps to its own cell index (`(w*128 & ~0x7F) >> 7 == w`).

use super::*;
use inf_formats::{
    ByteSpan, MissionPlacementSlot, MissionPlacementTable, TerrainCell, TerrainResourceLookup,
};

fn record(x: u16, lookup: u8) -> TerrainRecord {
    TerrainRecord {
        x,
        state: 0,
        resource_lookup: TerrainResourceLookup::from_raw(lookup),
    }
}

fn bucket(y: u16, xs: &[(u16, u8)]) -> TerrainBucket {
    TerrainBucket {
        y,
        byte_size: (xs.len() * 4) as u16,
        records_span: ByteSpan { start: 0, end: 0 },
        records: xs.iter().map(|&(x, lookup)| record(x, lookup)).collect(),
    }
}

fn domain(coords: &[u16], rows: usize) -> FurnitureDomain {
    FurnitureDomain {
        origin_x: 0,
        origin_z: 0,
        scale: 128,
        mask: 0x7f,
        coords: coords.to_vec(),
        rows,
    }
}

fn domain_scaled(coords: &[u16], rows: usize, scale: u16, mask: u16) -> FurnitureDomain {
    FurnitureDomain {
        origin_x: 0,
        origin_z: 0,
        scale,
        mask,
        coords: coords.to_vec(),
        rows,
    }
}

#[test]
fn selects_in_range_records_in_order_with_lookups() {
    // Bucket 5 holds four records; the row window [12, 30] takes the
    // middle two only, preserving order and lookup bytes. The vec is
    // indexed by cell, so six buckets put cell 5 at index 5.
    let mut full = vec![bucket(0, &[]); 6];
    full[5] = bucket(5, &[(10, 3), (20, 7), (30, 9), (40, 11)]);
    let hits = walk_furniture_records(&full, &domain(&[5, 12, 30], 1)).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].bucket, 5);
    assert_eq!(hits[0].index, 1);
    assert_eq!(hits[0].record, record(20, 7));
    assert_eq!(hits[1].index, 2);
    assert_eq!(hits[1].record, record(30, 9));
}

#[test]
fn wrapping_range_crosses_zero() {
    // Window lo=0x1F0, hi=0x010 (width 0x20 wrapping): records at
    // 0x1F8 and 0x008 hit, 0x100 does not.
    let mut full = vec![bucket(0, &[]); 2];
    full[1] = bucket(1, &[(0x1f8, 1), (0x100, 2), (0x008, 3)]);
    let hits = walk_furniture_records(&full, &domain(&[1, 0x1f0, 0x010], 1)).unwrap();
    assert_eq!(
        hits.iter().map(|hit| hit.record.x).collect::<Vec<_>>(),
        [0x1f8, 0x008]
    );
}

#[test]
fn odd_x_records_skip_unless_mask_is_7f() {
    // Record x=21 is odd: visited when the mask is exactly 0x7F (the
    // native bypass), skipped otherwise. The strict case runs the
    // Units256 pair (scale 256 / mask 0xFF), where coord words double:
    // bucket 0 with the full even window [0, 0x1FE].
    let mut full = vec![bucket(0, &[]); 2];
    full[1] = bucket(1, &[(20, 1), (21, 2), (22, 3)]);
    let hits = walk_furniture_records(&full, &domain(&[1, 0, 0x1ff], 1)).unwrap();
    assert_eq!(hits.len(), 3);
    full[0] = bucket(0, &[(20, 1), (21, 2), (22, 3)]);
    let strict = domain_scaled(&[0, 0, 0xff], 1, 256, 0xff);
    let hits = walk_furniture_records(&full, &strict).unwrap();
    assert_eq!(
        hits.iter().map(|hit| hit.record.x).collect::<Vec<_>>(),
        [20, 22]
    );
}

#[test]
fn z_accumulator_decrements_one_scale_step_per_row() {
    // Row 0 seeds cell 5 from the z word; row 1 decrements to cell 4
    // (the native `uVar5 -= scale`, stale z words ignored).
    let mut full = vec![bucket(0, &[]); 6];
    full[5] = bucket(5, &[(8, 1)]);
    full[4] = bucket(4, &[(9, 2)]);
    // Layout [z0, lo0, hi0, stale, lo1, hi1]: full x-range both rows.
    let hits = walk_furniture_records(&full, &domain(&[5, 0, 0x1ff, 0, 0, 0x1ff], 2)).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].bucket, 5);
    assert_eq!(hits[1].bucket, 4);
    assert_eq!(hits[1].record, record(9, 2));
}

#[test]
fn short_coords_and_missing_buckets_fail_closed() {
    let full = vec![bucket(0, &[(4, 1)])];
    // Two rows need six words; five fail before any bucket read.
    assert!(matches!(
        walk_furniture_records(&full, &domain(&[0, 0, 0x1ff, 0, 0], 2)),
        Err(FurnitureWalkError::ShortCoords { .. })
    ));
    // Cell 7 is beyond this one-bucket grid (retail would read
    // out of bounds).
    assert!(matches!(
        walk_furniture_records(&full, &domain(&[7, 0, 0x1ff], 1)),
        Err(FurnitureWalkError::Bucket { .. })
    ));
    // An empty bucket selects nothing but still succeeds.
    let empty = vec![bucket(0, &[])];
    let hits = walk_furniture_records(&empty, &domain(&[0, 0, 0x1ff], 1)).unwrap();
    assert!(hits.is_empty());
}

fn setup_inputs<'a>(
    cells: &'a [TerrainCell],
    lights: &'a [u8],
    anchors: FurnitureAnchors,
) -> FurnitureSetupInputs<'a> {
    FurnitureSetupInputs {
        cells,
        height_shift: 5,
        mask: 0xff,
        anchors,
        light_cells: lights,
        outside_light_word: 0,
        tint_word: 0x0012_0000,
    }
}

fn flat_cells() -> Vec<TerrainCell> {
    vec![TerrainCell::from_raw(0); 512 * 512]
}

#[test]
fn lookup_zero_skips_before_any_read() {
    // The `+3` gate: a zero lookup never becomes an H1, even with
    // empty tables behind it.
    let inputs = setup_inputs(&[], &[], furniture_anchors(0, 0));
    let blank = TerrainRecord {
        x: 0xa4,
        state: 0,
        resource_lookup: TerrainResourceLookup::from_raw(0),
    };
    assert_eq!(setup_furniture_item(blank, 0x4100, &inputs).unwrap(), None);
}

#[test]
fn item_xyz_comes_from_record_grid_and_signed_cell_height() {
    // Record x 0xA4 at walk z 0x4100 over a height-3 cell:
    // X = 0xA4 << 7, Z verbatim, Y = 3 << 5. Anchors at the item
    // put it in-grid (cells (40, 50)); light comes from the table.
    let mut cells = flat_cells();
    cells[0xa4 * 512 + 130] = TerrainCell::from_raw(3);
    let mut lights = vec![0u8; 90 * 90 * 3];
    let at = (50 + 40 * 90) * 3;
    lights[at..at + 3].copy_from_slice(&[0xab, 0x7f, 0x80]);
    let anchors = FurnitureAnchors {
        x: 0xa4 * 128 - 40 * 128,
        z: 0x4100 - 50 * 128,
    };
    let inputs = setup_inputs(&cells, &lights, anchors);
    let item = setup_furniture_item(record(0xa4, 51), 0x4100, &inputs)
        .unwrap()
        .unwrap();
    assert_eq!(item.xyz, [0xa4 * 128, 96, 0x4100]);
    assert_eq!(item.anchor_cell, [40, 50]);
    assert!(item.in_light_grid);
    assert_eq!(item.light_rgb, [-85, 127, -128]);
    // Each unsigned tint/16 adds its own signed light channel.
    assert_eq!(item.ambient_rgb, [-85, 127, -127]);
    assert_eq!(clamp_ambient_rgb(item.ambient_rgb), [0, 127, 0]);
}

#[test]
fn far_items_read_all_signed_fallback_data_bytes() {
    // Anchors at zero leave cell (164, 130) off the 90-grid: the
    // light comes from the supplied data DWORD, with no grid read.
    let mut cells = flat_cells();
    cells[0xa4 * 512 + 130] = TerrainCell::from_raw(3);
    let mut inputs = setup_inputs(&cells, &[], furniture_anchors(0, 0));
    inputs.outside_light_word = 0x99ab_7f80;
    inputs.tint_word = 0x12ff_8010;
    let item = setup_furniture_item(record(0xa4, 51), 0x4100, &inputs)
        .unwrap()
        .unwrap();
    assert_eq!(item.xyz, [0xa4 * 128, 96, 0x4100]);
    assert!(!item.in_light_grid);
    assert_eq!(item.light_rgb, [-128, 127, -85]);
    assert_eq!(item.ambient_rgb, [-127, 135, -70]);
    assert_eq!(clamp_ambient_rgb(item.ambient_rgb), [0, 135, 0]);
}

#[test]
fn negative_height_carry_leaves_anchor_cells_unchanged() {
    // Y = -32 carries 0xFFFF into the anchor high word, but the
    // `& 0x1FF` after `>> 7` kills the +0x10000 exactly: anchor
    // cells match the positive-height twin. The carry is
    // transcribed verbatim anyway (dead by arithmetic, not by audit).
    let mut neg = flat_cells();
    neg[0xa4 * 512 + 130] = TerrainCell::from_raw(0xff);
    let mut pos = flat_cells();
    pos[0xa4 * 512 + 130] = TerrainCell::from_raw(1);
    let anchors = FurnitureAnchors {
        x: 0xa4 * 128 - 40 * 128,
        z: 0x4100 - 50 * 128,
    };
    let lights = vec![0u8; 90 * 90 * 3];
    let item_neg = setup_furniture_item(
        record(0xa4, 51),
        0x4100,
        &setup_inputs(&neg, &lights, anchors),
    )
    .unwrap()
    .unwrap();
    let item_pos = setup_furniture_item(
        record(0xa4, 51),
        0x4100,
        &setup_inputs(&pos, &lights, anchors),
    )
    .unwrap()
    .unwrap();
    assert_eq!(item_neg.xyz[1], -32);
    assert_eq!(item_pos.xyz[1], 32);
    assert_eq!(item_neg.anchor_cell, item_pos.anchor_cell);
}

#[test]
fn anchors_subtract_0x1680_and_packed_ambient_clamps() {
    assert_eq!(
        furniture_anchors(100, -200),
        FurnitureAnchors {
            x: 100 - 0x1680,
            z: -200 - 0x1680
        }
    );
    assert_eq!(clamp_ambient_rgb([300, -5, 100]), [0xff, 0, 100]);
    assert_eq!(clamp_ambient_rgb([1, 2, -1]), [1, 2, 0]);
}

#[test]
fn setup_out_of_range_tables_fail_closed() {
    let inputs = setup_inputs(&[], &[], furniture_anchors(0, 0));
    assert!(matches!(
        setup_furniture_item(record(0xa4, 51), 0x4100, &inputs),
        Err(FurnitureSetupError::HeightCell { .. })
    ));
    // In-grid item with a short light table (retail would read
    // out of bounds).
    let cells = flat_cells();
    let anchors = FurnitureAnchors {
        x: 0xa4 * 128 - 40 * 128,
        z: 0x4100 - 50 * 128,
    };
    let inputs = setup_inputs(&cells, &[0xab; 10], anchors);
    assert!(matches!(
        setup_furniture_item(record(0xa4, 51), 0x4100, &inputs),
        Err(FurnitureSetupError::LightTable { .. })
    ));
}

#[test]
fn in_grid_setup_requires_the_complete_reached_rgb_triple() {
    let cells = flat_cells();
    let anchors = FurnitureAnchors { x: 0, z: 0 };
    for count in 0..3 {
        let lights = vec![0x80; count];
        let inputs = setup_inputs(&cells, &lights, anchors);
        assert_eq!(
            setup_furniture_item(record(0, 51), 0, &inputs),
            Err(FurnitureSetupError::LightTable {
                needed: 3,
                have: count,
            })
        );
    }
}

#[derive(serde::Deserialize)]
struct FurnitureSetupReceipt {
    executable_sha256: String,
    setup: u32,
    ambient: u32,
    cases: Vec<FurnitureSetupCase>,
    clamp_cases: Vec<FurnitureAmbientCase>,
}

#[derive(serde::Deserialize)]
struct FurnitureSetupCase {
    name: String,
    x: u16,
    z: u16,
    lookup: u8,
    height: i8,
    height_shift: u32,
    mask: u16,
    anchors: [i32; 2],
    light_rgb: [i8; 3],
    outside_light_word: u32,
    tint_word: u32,
    ambient_word_before: u32,
    expected: Option<FurnitureSetupExpected>,
}

#[derive(serde::Deserialize)]
struct FurnitureSetupExpected {
    xyz: [i32; 3],
    anchor_cell: [u32; 2],
    in_light_grid: bool,
    light_rgb: [i8; 3],
    ambient_rgb: [i32; 3],
    ambient_word: u32,
}

#[derive(serde::Deserialize)]
struct FurnitureAmbientCase {
    values: [i32; 3],
    ambient_word: u32,
}

fn packed_ambient_with_retained_high_byte(before: u32, components: [i32; 3]) -> u32 {
    let [r, g, b] = clamp_ambient_rgb(components);
    (before & 0xff00_0000) | u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

#[test]
fn original_setup_rgb_arguments_and_ambient_state_match_native_receipt() {
    // The tracked checker executes unchanged CEF0/490130; only its terminal
    // 4359A0 model/effect composition is an explicit external host boundary.
    let receipt: FurnitureSetupReceipt = serde_json::from_str(include_str!(
        "../tests/fixtures/furniture_setup_native.json"
    ))
    .unwrap();
    assert_eq!(
        receipt.executable_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!((receipt.setup, receipt.ambient), (0x44cef0, 0x490130));
    assert_eq!(receipt.cases.len(), 48);
    for case in receipt.cases {
        let align = ((!(i32::from(case.mask))) >> 7) as u32;
        let cell =
            ((u32::from(case.x) & 511 & align) * 512 + ((u32::from(case.z) >> 7) & align)) as usize;
        let mut cells = flat_cells();
        cells[cell] = TerrainCell::from_raw(0xe700 | u16::from(case.height as u8));
        let mut lights = vec![0; 90 * 90 * 3];
        if let Some(expected) = case.expected.as_ref().filter(|item| item.in_light_grid) {
            let [x, z] = expected.anchor_cell;
            let at = ((x * 90 + z) * 3) as usize;
            lights[at..at + 3].copy_from_slice(&case.light_rgb.map(|value| value as u8));
        }
        let inputs = FurnitureSetupInputs {
            cells: &cells,
            height_shift: case.height_shift,
            mask: case.mask,
            anchors: FurnitureAnchors {
                x: case.anchors[0],
                z: case.anchors[1],
            },
            light_cells: &lights,
            outside_light_word: case.outside_light_word,
            tint_word: case.tint_word,
        };
        let actual = setup_furniture_item(record(case.x, case.lookup), case.z, &inputs).unwrap();
        match (actual, case.expected) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                assert_eq!(
                    actual,
                    FurnitureItem {
                        xyz: expected.xyz,
                        anchor_cell: expected.anchor_cell,
                        in_light_grid: expected.in_light_grid,
                        light_rgb: expected.light_rgb,
                        ambient_rgb: expected.ambient_rgb,
                    },
                    "{}",
                    case.name
                );
                assert_eq!(
                    packed_ambient_with_retained_high_byte(
                        case.ambient_word_before,
                        actual.ambient_rgb
                    ),
                    expected.ambient_word,
                    "{}",
                    case.name
                );
            }
            _ => panic!("{}: original lookup admission differs", case.name),
        }
    }
    assert_eq!(receipt.clamp_cases.len(), 6);
    for case in receipt.clamp_cases {
        assert_eq!(
            packed_ambient_with_retained_high_byte(0xa5ab_cdef, case.values),
            case.ambient_word,
            "{:?}",
            case.values
        );
    }
}

fn mission_table(pairs: &[(u8, u16)]) -> MissionPlacementTable {
    let mut slots = vec![
        MissionPlacementSlot {
            slot: 0,
            resource_word: 0,
            parameter_word: 0,
        };
        256
    ];
    for &(lookup, slot) in pairs {
        slots[usize::from(lookup)] = MissionPlacementSlot {
            slot: 0,
            resource_word: slot,
            parameter_word: 0,
        };
    }
    MissionPlacementTable {
        local_index: 0,
        span: ByteSpan { start: 0, end: 0 },
        slots,
    }
}

#[test]
fn bind_reads_first_word_at_lookup_index() {
    // Banked frame-1 pairs (bind oracle): the runtime table carries
    // these slots, and 6.ovl region 0 carries the same DWORDs.
    let table = mission_table(&[
        (50, 79),
        (13, 76),
        (100, 51),
        (51, 398),
        (95, 386),
        (101, 50),
        (102, 47),
        (98, 49),
        (26, 77),
        (15, 373),
    ]);
    for (lookup, slot) in [
        (50u8, 79u16),
        (13, 76),
        (100, 51),
        (51, 398),
        (95, 386),
        (101, 50),
        (102, 47),
        (98, 49),
        (26, 77),
        (15, 373),
    ] {
        assert_eq!(
            bind_furniture_lookup(&table, TerrainResourceLookup::from_raw(lookup)),
            Some(slot),
            "lookup {lookup}"
        );
    }
}

#[test]
fn bind_skips_only_zero_and_reads_fd_through_ff_as_unsigned_indices() {
    let table = mission_table(&[(50, 79), (0xfd, 0x8123), (0xfe, 0x4567), (0xff, 0xfedc)]);
    assert_eq!(
        bind_furniture_lookup(&table, TerrainResourceLookup::from_raw(0)),
        None
    );
    for (raw, slot) in [(0xfdu8, 0x8123), (0xfe, 0x4567), (0xff, 0xfedc)] {
        assert_eq!(
            bind_furniture_lookup(&table, TerrainResourceLookup::from_raw(raw)),
            Some(slot),
            "unsigned lookup {raw:#x}"
        );
    }
    let short = MissionPlacementTable {
        local_index: 0,
        span: ByteSpan { start: 0, end: 0 },
        slots: vec![],
    };
    assert_eq!(
        bind_furniture_lookup(&short, TerrainResourceLookup::from_raw(50)),
        None
    );
}
