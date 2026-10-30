use std::{path::Path, sync::OnceLock};

use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{BaseContext, ObjectClassKind},
    terrain_runtime::{TerrainSampling, TerrainSpacing},
};

fn math() -> &'static MenuBasisMath {
    static MATH: OnceLock<MenuBasisMath> = OnceLock::new();
    MATH.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        MenuBasisMath::new(FrontendAssets::open(&root).unwrap().sine_table().values)
    })
}

fn synthetic_terrain(
    spacing: TerrainSpacing,
    scale: TerrainHeightScale,
    plane: i32,
    height: impl Fn(usize, usize) -> i8,
) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[..4].copy_from_slice(&plane.to_le_bytes());
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2] = height(index / 512, index % 512) as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
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
        &bank.entries[0],
        TerrainSampling {
            spacing,
            height_scale: scale,
        },
    )
}

fn record() -> BriefingCamera {
    BriefingCamera {
        actor_id: 0,
        heading_degrees: 0,
        rotation_word: 0,
        speed_word: 25,
        track_byte: 1,
        terrain_byte: 0,
    }
}

fn request() -> BriefingCameraTick {
    BriefingCameraTick {
        record: record(),
        heading: 0,
        delta_micros: 65_536,
        map_scale_word: 0,
        terrain_camera_disabled: false,
    }
}

#[test]
fn kind8_searches_primary_flag0_order_with_signed_placement_and_preserves_target() {
    let mut arena = ObjectArena::new();
    let parked = arena.allocate(ObjectClassKind::Base);
    let first = arena.allocate(ObjectClassKind::Base);
    let second = arena.allocate(ObjectClassKind::Base);
    for (object, position) in [
        (parked, [99, 0, 99]),
        (first, [10, 0, 20]),
        (second, [30, 0, 40]),
    ] {
        arena
            .initialize_context(
                object,
                BaseContext {
                    position_words: Some(position),
                    placement_index: 0xffff,
                },
            )
            .unwrap();
    }
    arena.set_flag0(first, true).unwrap();
    arena.set_flag0(second, true).unwrap();
    let mut state = BriefingCameraState::default();
    assert_eq!(state.current_words(), [0; 3]);
    // Zero bypasses lookup even though native +2A=-1 matches DWORD-1.
    state.activate(0, &arena).unwrap();
    assert_eq!(state.target_xz_words(), [0; 2]);
    arena
        .initialize_context(
            first,
            BaseContext {
                position_words: Some([10, 0, 20]),
                placement_index: 0xfffe,
            },
        )
        .unwrap();
    arena
        .initialize_context(
            second,
            BaseContext {
                position_words: Some([30, 0, 40]),
                placement_index: 0xfffe,
            },
        )
        .unwrap();
    state.activate(0xffff_ffff, &arena).unwrap();
    assert_eq!(state.target_xz_words(), [10, 20]);
    state.activate(123, &arena).unwrap();
    state.activate(0, &arena).unwrap();
    assert_eq!(state.target_xz_words(), [10, 20]);
    arena.set_flag0(first, false).unwrap();
    state.activate(0xffff_ffff, &arena).unwrap();
    assert_eq!(state.target_xz_words(), [30, 40]);
}

#[test]
fn camera_motion_caps_speed_then_extracts_signed_steps_and_forces_nonzero_xz() {
    let arena = ObjectArena::new();
    let terrain = synthetic_terrain(
        TerrainSpacing::Units128,
        TerrainHeightScale::Units32,
        0,
        |_, _| 0,
    );
    let inputs = BriefingCameraInputs {
        world: BriefingCameraWorld {
            arena: &arena,
            terrain: &terrain,
        },
        math: math(),
    };
    for (target, expected) in [(1000, 12), ((-1000_i16) as u16, (-13_i16) as u16)] {
        let mut state = BriefingCameraState {
            current: [0, 400, 0],
            target_xz: [target, 0],
        };
        let result = state.tick(request(), inputs);
        assert_eq!(result.distance, 1000);
        assert_eq!(state.current_words(), [expected, 400, 0]);
    }
    let mut state = BriefingCameraState {
        current: [0xffff, 450, 0],
        target_xz: [0, 0xffff],
    };
    let mut tiny = request();
    tiny.delta_micros = 1;
    tiny.record.speed_word = 0;
    state.tick(tiny, inputs);
    assert_eq!(
        state.current_words(),
        [0, 450, 0xffff],
        "zero speed still reaches native forced ±1, Y has no forced step"
    );
    // DWORD square-sum overflow is signed nonpositive; 48EFC0 returns0.
    let mut overflow = BriefingCameraState {
        current: [0, 400, 0],
        target_xz: [0x8000, 0x8000],
    };
    assert_eq!(overflow.tick(request(), inputs).distance, 0);
    assert_eq!(overflow.current_words(), [0xf000, 400, 0xf000]);
}

#[test]
fn unforced_far_approach_freezes_xz_for_zoom_and_terrain_crossing_restarts_pulse() {
    let arena = ObjectArena::new();
    let terrain = synthetic_terrain(
        TerrainSpacing::Units128,
        TerrainHeightScale::Units32,
        0,
        |_, _| 0,
    );
    let inputs = BriefingCameraInputs {
        world: BriefingCameraWorld {
            arena: &arena,
            terrain: &terrain,
        },
        math: math(),
    };
    let mut state = BriefingCameraState {
        current: [0, 400, 0],
        target_xz: [3000, 0],
    };
    let mut tick = request();
    tick.record.track_byte = 0;
    tick.map_scale_word = 0x2000;
    let far = state.tick(tick, inputs);
    assert!(far.retreat_map_scale);
    assert!(far.terrain_camera_disabled);
    assert_eq!(state.current_words(), [0, 400, 0]);
    tick.map_scale_word = 0x1fff;
    state.tick(tick, inputs);
    assert_eq!(state.current_words()[0], 12);
    state.target_xz = [1012, 0];
    tick.terrain_camera_disabled = true;
    let near = state.tick(tick, inputs);
    assert_eq!(near.distance, 1000);
    assert!(!near.retreat_map_scale);
    assert!(!near.terrain_camera_disabled);
    assert!(near.restart_pulse);
    tick.record.terrain_byte = 1;
    let suppressed = state.tick(tick, inputs);
    assert!(!suppressed.terrain_camera_disabled);
    assert!(!suppressed.restart_pulse);
}

#[test]
fn camera_height_uses_stored_aligned_forward_cell_and_signed_plane() {
    let arena = ObjectArena::new();
    // Original heading0 forward1200 samples coarse stored z=8, bypassing
    // interpolation. The plane is fixed-point DWORD500<<8.
    let terrain = synthetic_terrain(
        TerrainSpacing::Units256,
        TerrainHeightScale::Units32,
        500 << 8,
        |x, z| if x == 0 && z == 8 { 20 } else { -10 },
    );
    assert_eq!(stored_height(&terrain, [1, 1199]), 640);
    assert_eq!(stored_height(&terrain, [1, 0]), 500);
    let inputs = BriefingCameraInputs {
        world: BriefingCameraWorld {
            arena: &arena,
            terrain: &terrain,
        },
        math: math(),
    };
    let mut state = BriefingCameraState::default();
    let mut tick = request();
    tick.delta_micros = 10_000;
    let result = state.tick(tick, inputs);
    assert_eq!(result.distance, 1040);
    assert_eq!(state.current_words(), [0, 20, 0]);
    let short_plane = synthetic_terrain(
        TerrainSpacing::Units128,
        TerrainHeightScale::Units16,
        (-600_i32) << 9,
        |_, _| -20,
    );
    assert_eq!(stored_height(&short_plane, [0, 0]), -320);
}
