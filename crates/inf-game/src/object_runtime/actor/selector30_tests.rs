use std::sync::Arc;

use super::*;
use crate::{
    object_runtime::{
        native_model_fixture::{Fixture, Models},
        ActorRegistryConfig, ActorTerrainInputs, BaseContext, RotatingVisualContext,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    world_start::WorldStartState,
};
use inf_formats::{
    decode_instance_command, parse_constructor_asset_bank, parse_terrain_bank,
    ActorConstructorPayload, ByteSpan, ConstructorPayloadView, HandlerFraming, HandlerId,
    HandlerRegion, InstanceCommandReadScope, InstanceCommandRequest, RelocationTail,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

pub(super) fn request(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

pub(super) fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn resident() -> Fixture {
    Fixture::new(
        &serde_json::from_value::<Models>(serde_json::json!({"capacity":0,"entries":[]})).unwrap(),
    )
}

pub(super) fn command(code: i32, bytes: &[u8]) -> InstanceCommandView {
    let mut source = code.to_le_bytes().to_vec();
    source.extend_from_slice(bytes);
    decode_instance_command(InstanceCommandRequest {
        source: &source,
        main_payload: ByteSpan::new(0, source.len()),
        relocations: &[],
        command_offset: 0,
        target: InstanceCommandTarget::Constructor(30),
        read_scope: InstanceCommandReadScope::Dispatch,
    })
    .unwrap()
}

pub(super) fn payload(bytes: &[u8]) -> Arc<ActorConstructorPayload> {
    let mut source = vec![0xa5; 9];
    source.extend_from_slice(&29_u16.to_le_bytes());
    source.extend_from_slice(&u16::MAX.to_le_bytes());
    source.extend_from_slice(bytes);
    let main = ByteSpan::new(9, source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let bank = parse_constructor_asset_bank(
        &source,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(9, source.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload: main,
                relocation_count: 0,
                relocation_offsets: ByteSpan::new(main.end, main.end),
                entry_offsets: ByteSpan::new(main.end, source.len()),
            }),
        },
    )
    .unwrap();
    let ConstructorPayloadView::SharedActor(value) = bank.assets[0].payload.clone() else {
        panic!("actor payload")
    };
    Arc::new(value)
}

pub(super) fn terrain(height: i8) -> TerrainRuntime {
    let mut source = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        source[TERRAIN_PREFIX_BYTES + index * 2] = height as u8;
    }
    let span = ByteSpan::new(0, source.len());
    let bank = parse_terrain_bank(
        &source,
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
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

#[test]
fn constructor_keeps_unwritten_fields_and_shared_registration_custody() {
    let mut arena = arena();
    let object = arena.allocate_selector30_actor(request(None)).unwrap();
    let actor = arena.selector30_actor(object).unwrap();
    assert_eq!(actor.intermediate.byte_138, 1);
    assert_eq!(actor.intermediate.word_144, u16::MAX);
    assert_eq!(actor.intermediate.words_146_14a, None);
    assert_eq!(actor.intermediate.word_158, None);
    assert_eq!(actor.word_188, None);
    assert_eq!(actor.position_16c, None);
    assert_eq!(actor.handle_184.raw(), 0);
    let handle = arena.actor(object).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
    assert!(arena.actor_list().is_empty());
}

#[test]
fn initialization_preserves_the_model_error_before_payload_mutation() {
    let resident = resident();
    let mut arena = arena();
    let object = arena.allocate_selector30_actor(request(None)).unwrap();
    let before = arena.actor(object).unwrap().clone();
    let payload = payload(&[0; 0x58]);
    assert_eq!(
        arena.initialize_selector30_actor(
            object,
            ActorInitializationRequest {
                context: RotatingVisualContext {
                    base: BaseContext {
                        position_words: Some([1, 2, 3]),
                        placement_index: 4,
                    },
                    rotation_words: Some([0; 3]),
                    model_argument: Some(0),
                },
                payload: &payload,
                local_owner: 1,
            },
            &resident.runtime,
            &NativeBasisMath::new(UNUSED_SINE),
        ),
        Err(ObjectError::ModelSlotBoundary {
            slot: 0,
            capacity: 0
        })
    );
    assert_eq!(arena.actor(object).unwrap(), &before);
}

#[test]
fn intermediate_signed_clamps_refill_dirty_bits_and_operand_widths_match_native() {
    let resident = resident();
    let context = ActorCommandContext {
        terrain: None,
        runtime: &resident.runtime,
        first_player_grants: None,
    };
    let mut arena = arena();
    let object = arena.allocate_selector30_actor(request(None)).unwrap();
    // A positive current with a newly negative maximum becomes that negative
    // maximum; the native lower test is not run again afterward.
    arena
        .selector30_actor_mut(object)
        .unwrap()
        .intermediate
        .word_140 = 10;
    assert!(arena
        .apply_selector30_command(object, &command(16, &[0x80]), context)
        .unwrap());
    assert_eq!(
        (
            arena
                .selector30_actor(object)
                .unwrap()
                .intermediate
                .word_13c,
            arena
                .selector30_actor(object)
                .unwrap()
                .intermediate
                .word_140
        ),
        (-128, -128)
    );
    arena.owned_mut(object).unwrap().state.base.dirty_word = 0;
    assert!(arena
        .apply_selector30_command(object, &command(18, &[1]), context)
        .unwrap());
    assert_eq!(
        arena
            .selector30_actor(object)
            .unwrap()
            .intermediate
            .word_140,
        0
    );
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 0x200);
    for (code, bytes) in [
        (17, vec![255]),
        (19, vec![0, 128]),
        (20, vec![0, 255, 1, 128, 2, 127]),
        (21, vec![0, 128]),
        (22, vec![255]),
        (23, vec![128]),
        (24, vec![255]),
        (25, vec![128]),
        (26, vec![255]),
        (27, vec![128]),
        (28, vec![128]),
        (29, vec![128]),
        (30, vec![128]),
        (31, vec![128]),
        (32, vec![0, 128]),
    ] {
        assert!(arena
            .apply_selector30_command(object, &command(code, &bytes), context)
            .unwrap());
    }
    let state = arena.selector30_actor(object).unwrap();
    assert_eq!(state.intermediate.byte_14e, 1);
    assert_eq!(state.intermediate.word_144, 0x8000);
    assert_eq!(
        state.intermediate.words_146_14a,
        Some([0xff00, 0x8001, 0x7f02])
    );
    assert_eq!(
        (
            state.word_180,
            state.word_15c,
            state.word_160,
            state.word_164
        ),
        (0x8000, 255, 128, 255)
    );
    assert_eq!(
        (state.byte_18f, state.byte_167, state.word_18c),
        (128, 255, 0x8000)
    );
    assert_eq!(
        [
            state.byte_18e,
            state.byte_166,
            state.byte_190,
            state.byte_191,
            state.byte_192
        ],
        [1; 5]
    );
    for code in [i32::MIN, -1, 33, i32::MAX] {
        assert!(!arena
            .apply_selector30_command(object, &command(code, &[]), context)
            .unwrap());
    }
}

#[test]
fn postinit_preserves_grant_order_distance_boundary_raw_selection_and_extent() {
    let resident = resident();
    let terrain = terrain(0);
    let mut grants = [0; inf_formats::GRANT_MASK_WORDS];
    grants[1] = 1 << 1; // Grant 33, including the actual nonzero-word path.
    let context = ActorCommandContext {
        terrain: None,
        runtime: &resident.runtime,
        first_player_grants: Some(&grants),
    };
    for (distance, near) in [(999_u16, true), (1000, false)] {
        let mut arena = arena();
        let object = arena
            .allocate_selector30_actor(request(Some([distance, 60, 0])))
            .unwrap();
        arena.actor_mut(object).unwrap().word_7a = 33;
        arena.actor_mut(object).unwrap().word_b4 = 18;
        arena.actor_mut(object).unwrap().word_74 = 123;
        let state = arena.selector30_actor_mut(object).unwrap();
        state.byte_166 = 1;
        state.word_164 = 123;
        state.byte_18f = 255;
        let mut world = WorldStartState {
            pair_index: u16::MAX,
            override_position: Some([0, 999, 0]),
            vehicle_selection: 7,
            harness_spawn: None,
            port_spawn: None,
        };
        arena
            .finish_selector30_initialization(
                object,
                context,
                &mut WorldStartContext {
                    state: &mut world,
                    runtime: &resident.runtime,
                    terrain: &terrain,
                },
            )
            .unwrap();
        let state = arena.selector30_actor(object).unwrap();
        assert_eq!(
            (state.byte_166, state.word_164, state.word_188),
            (0, 0, Some(18))
        );
        assert_eq!(arena.actor(object).unwrap().word_74, 123);
        assert_eq!(
            arena.actor(object).unwrap().word_b4,
            if near { 2 } else { 18 }
        );
        assert_eq!(
            world.override_position,
            if near {
                Some([999, 60, (-450_i16) as u16])
            } else {
                Some([0, 999, 0])
            }
        );
        assert_eq!(world.vehicle_selection, if near { 255 } else { 7 });
    }
    // A failed later world lookup keeps the earlier grant/selector writes.
    let mut arena = arena();
    let object = arena
        .allocate_selector30_actor(request(Some([0, 60, 0])))
        .unwrap();
    arena.actor_mut(object).unwrap().word_7a = 33;
    arena.selector30_actor_mut(object).unwrap().byte_166 = 1;
    let mut world = WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 7,
        harness_spawn: None,
        port_spawn: None,
    };
    assert!(matches!(
        arena.finish_selector30_initialization(
            object,
            context,
            &mut WorldStartContext {
                state: &mut world,
                runtime: &resident.runtime,
                terrain: &terrain,
            }
        ),
        Err(Selector30Error::WorldStart(_))
    ));
    assert_eq!(arena.selector30_actor(object).unwrap().byte_166, 0);
    assert!(arena.selector30_actor(object).unwrap().word_188.is_some());
}

#[test]
fn afterload_signed_index_uses_c_division_and_only_resolves_nonzero_placement() {
    for (index, initial, expected) in [(-1_i8, 2, 18), (-31, 18, 2), (-32, 2, 2), (127, 2, 18)] {
        let mut arena = arena();
        let object = arena.allocate_selector30_actor(request(None)).unwrap();
        arena.actor_mut(object).unwrap().byte_78 = index as u8;
        arena.actor_mut(object).unwrap().word_b4 = 2;
        arena.selector30_actor_mut(object).unwrap().word_188 = Some(initial);
        arena
            .initialize_selector30_after_load(object, &[u32::MAX; 4])
            .unwrap();
        assert_eq!(arena.actor(object).unwrap().word_b4, expected);
        assert_eq!(arena.selector30_actor(object).unwrap().handle_184.raw(), 0);
        assert_eq!(arena.object(object).unwrap().base.flags & 0x0080_0000, 0);
    }
    for bits in [
        0,
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
        (-1.5_f32).to_bits(),
    ] {
        assert_eq!(
            start_offset(bits),
            match f32::from_bits(bits) {
                x if x.is_finite() => (f64::from(x) * -450.0).trunc() as i64 as u16,
                _ => 0,
            }
        );
    }
}
