use std::path::Path;

use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_GRID_SIDE, TERRAIN_PREFIX_BYTES,
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    terrain_runtime::{TerrainSampling, TerrainSpacing},
};

fn make_terrain(
    spacing: TerrainSpacing,
    height_scale: TerrainHeightScale,
    height: impl Fn(usize, usize) -> i8,
) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for x in 0..TERRAIN_GRID_SIDE {
        for z in 0..TERRAIN_GRID_SIDE {
            let offset = TERRAIN_PREFIX_BYTES + (x * TERRAIN_GRID_SIDE + z) * 2;
            let word = u16::from(height(x, z) as u8) | 0xa300;
            bytes[offset..offset + 2].copy_from_slice(&word.to_le_bytes());
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
    TerrainRuntime::new(
        &source.entries[0],
        TerrainSampling {
            spacing,
            height_scale,
        },
    )
}

fn sine() -> [u16; 4096] {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    FrontendAssets::open(&root).unwrap().sine_table().values
}

fn inputs<'a>(terrain: &'a TerrainRuntime, sine: &'a [u16; 4096]) -> ActorTerrainInputs<'a> {
    ActorTerrainInputs {
        terrain: Some(ActorTerrainSurface {
            terrain,
            plane_height_fixed: 0,
        }),
        coordinate_shift: 8,
        clock: 0,
        world_flags: Some(0),
        sine_quarter: sine,
    }
}

#[test]
fn missing_terrain_precedes_position_but_present_terrain_requires_allocation_words() {
    let terrain = make_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units32,
        |_, _| -1,
    );
    let sine = sine();
    let mut request = inputs(&terrain, &sine);
    request.world_flags = None;
    assert_eq!(
        classify_actor_terrain(None, request),
        Err(ActorTerrainError::UnknownAllocationPosition)
    );
    request.terrain = None;
    assert_eq!(
        classify_actor_terrain(None, request),
        Ok(ActorTerrainClassification::Set40000000)
    );
    assert_eq!(
        ActorTerrainClassification::Set40000000.flags_to_or(),
        0x4000_0000
    );
    assert_eq!(
        ActorTerrainClassification::Set20000000.flags_to_or(),
        0x2000_0000
    );
    assert_eq!(ActorTerrainClassification::Unchanged.flags_to_or(), 0);
}

#[test]
fn missing_world_is_read_only_after_strict_plane_margin_and_raw_height_tests() {
    let terrain = make_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units32,
        |_, _| -1,
    );
    let sine = sine();
    let mut request = inputs(&terrain, &sine);
    request.world_flags = None;
    for (shift, margin) in [(8, 512_i32), (9, 256_i32)] {
        request.coordinate_shift = shift;
        for (y, expected) in [
            (-margin - 33, Ok(ActorTerrainClassification::Set20000000)),
            (-margin - 32, Err(ActorTerrainError::MissingWorldResource)),
            (margin + 32, Err(ActorTerrainError::MissingWorldResource)),
            (margin + 33, Ok(ActorTerrainClassification::Set40000000)),
        ] {
            assert_eq!(
                classify_actor_terrain(Some([0, y as u16, 0]), request),
                expected
            );
        }
    }

    let equal = make_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units32,
        |_, _| 0,
    );
    request.terrain.as_mut().unwrap().terrain = &equal;
    assert_eq!(
        classify_actor_terrain(Some([0; 3]), request),
        Ok(ActorTerrainClassification::Set40000000)
    );
}

#[test]
fn signed_allocation_height_and_inclusive_32_unit_plane_band_are_preserved() {
    let terrain = make_terrain(
        TerrainSpacing::Units128,
        TerrainHeightScale::Units16,
        |_, _| -1,
    );
    let sine = sine();
    for (y, expected) in [
        (-33_i16, ActorTerrainClassification::Set20000000),
        (-32, ActorTerrainClassification::Unchanged),
        (-1, ActorTerrainClassification::Unchanged),
        (0, ActorTerrainClassification::Unchanged),
        (32, ActorTerrainClassification::Unchanged),
        (33, ActorTerrainClassification::Set40000000),
    ] {
        assert_eq!(
            classify_actor_terrain(Some([0xffff, y as u16, 0x8000]), inputs(&terrain, &sine)),
            Ok(expected)
        );
    }
}

#[test]
fn constructor_samples_raw_aligned_cell_instead_of_interpolated_placement_height() {
    let sine = sine();
    for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
        for scale in [TerrainHeightScale::Units16, TerrainHeightScale::Units32] {
            let terrain = make_terrain(spacing, scale, |x, _| if x == 0 { -128 } else { 127 });
            let x = spacing.units() - 1;
            assert!(terrain.height_at([x, 0]) > 0);
            assert_eq!(terrain.cell_at([x, 0]).height_i8(), -128);
            let mut request = inputs(&terrain, &sine);
            request.world_flags = None;
            assert_eq!(
                classify_actor_terrain(Some([x, 0, 0]), request),
                Err(ActorTerrainError::MissingWorldResource)
            );
        }
    }
}

#[test]
fn coordinate_shift_is_independent_and_uses_x86_masked_counts() {
    let terrain = make_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units32,
        |_, _| -1,
    );
    let sine = sine();
    let mut request = inputs(&terrain, &sine);
    request.terrain.as_mut().unwrap().plane_height_fixed = 100 << 8;
    for (shift, expected) in [
        (8, ActorTerrainClassification::Unchanged),
        (40, ActorTerrainClassification::Unchanged),
        (9, ActorTerrainClassification::Set40000000),
        (41, ActorTerrainClassification::Set40000000),
    ] {
        request.coordinate_shift = shift;
        assert_eq!(
            classify_actor_terrain(Some([0, 100, 0]), request),
            Ok(expected)
        );
    }

    request.terrain.as_mut().unwrap().plane_height_fixed = -1;
    for shift in [0, 1, 8, 9, 31, 32, 63, u32::MAX] {
        request.coordinate_shift = shift;
        assert_eq!(
            classify_actor_terrain(Some([0, (-1_i16) as u16, 0]), request),
            Ok(ActorTerrainClassification::Unchanged)
        );
    }
}

#[test]
fn only_world_bit_40_enables_clock_dependent_plane_waves() {
    let terrain = make_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units32,
        |_, _| -128,
    );
    let sine = sine();
    let mut request = inputs(&terrain, &sine);
    request.clock = 128;
    for flags in [0, 0x20, 0x80, 0x4000, !0x40] {
        request.world_flags = Some(flags);
        assert_eq!(
            classify_actor_terrain(Some([0; 3]), request),
            Ok(ActorTerrainClassification::Unchanged)
        );
        request.world_flags = Some(flags | 0x40);
        assert_eq!(
            classify_actor_terrain(Some([0; 3]), request),
            Ok(ActorTerrainClassification::Set20000000)
        );
    }
    request.clock = 0;
    assert_eq!(
        classify_actor_terrain(Some([0; 3]), request),
        Ok(ActorTerrainClassification::Unchanged)
    );
}

#[test]
fn negative_wave_clamps_to_ground_before_the_final_position_band_test() {
    let terrain = make_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units16,
        |_, _| -1,
    );
    let sine = sine();
    let mut request = inputs(&terrain, &sine);
    request.world_flags = Some(0x40);
    request.clock = 384;
    // The original sine table gives a wave offset of -29 for a plane of 0
    // and ground -16. 0041106E clamps it to -16 before the Y +/-32 tests.
    assert_eq!(plane_height_at([0; 2], 0, -16, 384, true, &sine), -16);
    for (y, expected) in [
        (-49_i16, ActorTerrainClassification::Set20000000),
        (-48, ActorTerrainClassification::Unchanged),
        (16, ActorTerrainClassification::Unchanged),
        (17, ActorTerrainClassification::Set40000000),
    ] {
        assert_eq!(
            classify_actor_terrain(Some([0, y as u16, 0]), request),
            Ok(expected)
        );
    }
}

#[test]
fn positive_wave_wraps_the_returned_word_after_the_ground_comparison() {
    let terrain = make_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units32,
        |_, _| -128,
    );
    let sine = sine();
    let mut request = inputs(&terrain, &sine);
    request.terrain.as_mut().unwrap().plane_height_fixed = 32767 << 8;
    request.world_flags = Some(0x40);
    request.clock = 128;
    // Candidate height is above ground in signed DWORD space, then its low
    // WORD crosses 0x8000. The constructor sign-extends that returned WORD.
    assert!(plane_height_at([0; 2], 32767, -4096, 128, true, &sine) < -4096);
    assert_eq!(
        classify_actor_terrain(Some([0, 32767, 0]), request),
        Ok(ActorTerrainClassification::Set40000000)
    );
    request.world_flags = Some(0);
    assert_eq!(
        classify_actor_terrain(Some([0, 32767, 0]), request),
        Ok(ActorTerrainClassification::Unchanged)
    );
}

#[test]
fn raw_cell_accessor_retains_word_coordinates_metadata_and_spacing_alignment() {
    for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
        let terrain = make_terrain(spacing, TerrainHeightScale::Units32, |x, z| (x + z) as i8);
        for coordinates in [
            [0, 0],
            [127, 128],
            [255, 256],
            [0xffff, 0x8000],
            [0xffff; 2],
        ] {
            let mask = !((spacing.units() - 1) >> 7);
            let x = usize::from((coordinates[0] >> 7) & mask);
            let z = usize::from((coordinates[1] >> 7) & mask);
            assert_eq!(
                terrain.cell_at(coordinates),
                terrain.cells()[x * TERRAIN_GRID_SIDE + z]
            );
            assert_eq!(terrain.cell_at(coordinates).raw() & 0xf300, 0xa300);
        }
    }
}
