use std::{path::Path, sync::Arc};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, parse_constructor_asset_bank,
    ActorConstructorPayload, ByteSpan, ConstructorPayloadView, GrantClosureError, HandlerFraming,
    HandlerId, HandlerRegion, InstanceCommandRequest, InstanceCommandTarget, InstanceCommandView,
    RelocationTail, GRANT_MASK_WORDS,
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        native_model_fixture::{Fixture, Models},
        BaseContext, ObjectList, ObjectNode, RotatingVisualContext, INITIALIZED_FLAG,
    },
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn actor(arena: &mut ObjectArena) -> ObjectId {
    arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &UNUSED_SINE,
            },
        })
        .unwrap()
}

fn payload(flags: u32, health: u16, extent: u32, seed: u16) -> Arc<ActorConstructorPayload> {
    let mut source = vec![0; 4 + 0x42];
    source[..2].copy_from_slice(&2_u16.to_le_bytes()); // Authored selector 3.
    let bytes = &mut source[4..];
    bytes[..4].copy_from_slice(&flags.to_le_bytes());
    bytes[4..6].copy_from_slice(&health.to_le_bytes());
    bytes[8..10].copy_from_slice(&0x8123_u16.to_le_bytes());
    bytes[12..16].copy_from_slice(&3_i32.to_le_bytes());
    for channel in 0..10 {
        let at = 0x10 + channel * 4;
        bytes[at..at + 2].copy_from_slice(&(seed + channel as u16).to_le_bytes());
        bytes[at + 2..at + 4].copy_from_slice(&(256 + channel as u16).to_le_bytes());
    }
    bytes[0x38..0x3a].copy_from_slice(&15_u16.to_le_bytes());
    bytes[0x3c..0x40].copy_from_slice(&extent.to_le_bytes());
    bytes[0x40..0x42].copy_from_slice(&0xfedc_u16.to_le_bytes());
    let main_payload = ByteSpan::new(0, source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let region = HandlerRegion {
        handler: HandlerId::new(13).unwrap(),
        requested_count: 1,
        span: ByteSpan::new(0, source.len()),
        framing: HandlerFraming::RelocationTail(RelocationTail {
            main_payload,
            relocation_count: 0,
            relocation_offsets: ByteSpan::new(main_payload.end, main_payload.end),
            entry_offsets: ByteSpan::new(main_payload.end, source.len()),
        }),
    };
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[0].payload else {
        panic!("canonical shared actor view")
    };
    Arc::new(payload.clone())
}

fn retained(arena: &ObjectArena, object: ObjectId) -> &Arc<ActorConstructorPayload> {
    let ActorDamageTable::Payload(payload) = &arena.actor(object).unwrap().damage_table else {
        panic!("initialized damage table")
    };
    payload
}

fn models() -> Fixture {
    let models: Models = serde_json::from_str(
        r#"{"capacity":2,"entries":[
            {"slot":0,"dword_00":427,"dword_0e":0},
            {"slot":1,"dword_00":291,"dword_0e":1234}
        ]}"#,
    )
    .unwrap();
    Fixture::new(&models)
}

fn math() -> NativeBasisMath {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    NativeBasisMath::new(FrontendAssets::open(&root).unwrap().sine_table().values)
}

fn context(model: u32) -> RotatingVisualContext {
    RotatingVisualContext {
        base: BaseContext {
            position_words: Some([0xffff, 7, 0x8000]),
            placement_index: 0x1234_5678,
        },
        model_argument: Some(model),
        rotation_words: Some([0; 3]),
    }
}

fn lists(arena: &ObjectArena) -> Vec<Vec<ObjectNode>> {
    [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
        ObjectList::Parking,
        ObjectList::Disposal,
    ]
    .map(|list| arena.list(list).to_vec())
    .into()
}

fn command(code: i32, operands: &[u8]) -> InstanceCommandView {
    let mut source = code.to_le_bytes().to_vec();
    source.extend_from_slice(operands);
    let main_payload = ByteSpan::new(0, source.len());
    source.extend_from_slice(&[0xff; 8]); // Tail bytes cannot satisfy a guarded operand.
    decode_instance_command(InstanceCommandRequest {
        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
        source: &source,
        main_payload,
        relocations: &[],
        command_offset: 0,
        target: InstanceCommandTarget::SharedActor,
    })
    .unwrap()
}

#[test]
fn shared_response_backing_survives_caller_replacement_and_rebinds_only_one_actor() {
    let mut arena = arena();
    let first = actor(&mut arena);
    let second = actor(&mut arena);
    let mut current_payload = payload(7, 100, 0, 2000);
    let old = Arc::downgrade(&current_payload);
    for object in [first, second] {
        arena
            .initialize_actor_payload(object, &current_payload, 1)
            .unwrap();
        assert!(Arc::ptr_eq(retained(&arena, object), &current_payload));
    }
    assert!(Arc::ptr_eq(
        retained(&arena, first),
        retained(&arena, second)
    ));
    let second_before = arena.object(second).unwrap().clone();
    let old_responses = arena.actor(second).unwrap().damage_responses();
    current_payload = payload(3, 200, 24, 3000);
    assert!(old.upgrade().is_some());
    arena
        .initialize_actor_payload(first, &current_payload, 1)
        .unwrap();
    assert!(Arc::ptr_eq(retained(&arena, first), &current_payload));
    assert!(!Arc::ptr_eq(
        retained(&arena, first),
        retained(&arena, second)
    ));
    assert_eq!(arena.object(second).unwrap(), &second_before);
    assert_eq!(
        arena.actor(second).unwrap().damage_responses(),
        old_responses
    );
    assert_eq!(
        arena.actor(first).unwrap().damage_responses()[0],
        (3000, 256)
    );
    assert_eq!(arena.actor(first).unwrap().health, 200);
    let replacement = Arc::downgrade(&current_payload);
    drop(current_payload);
    assert!(replacement.upgrade().is_some());
    assert_eq!(
        arena.actor(first).unwrap().damage_responses()[9],
        (3009, 265)
    );
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(replacement.upgrade().is_none());
    drop(second_before);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(old.upgrade().is_none());
}

#[test]
fn wrapper_restores_payload_flag_four_and_extent_before_separate_completion() {
    let fixture = models();
    let math = math();
    let mut arena = arena();
    let object = actor(&mut arena);
    let payload = payload(7, 100, 0x1_0008, 2000);
    arena
        .initialize_actor(
            object,
            ActorInitializationRequest {
                context: context(0),
                payload: &payload,
                local_owner: 1,
            },
            &fixture.runtime,
            &math,
        )
        .unwrap();
    let state = arena.object(object).unwrap();
    assert_eq!(state.base.position_words, Some([0xffff, 7, 0x8000]));
    assert_eq!(state.base.placement_word, 0x5678);
    assert_eq!(
        state.base.flags & 4,
        4,
        "payload follows the zero-radius model clear"
    );
    assert_eq!(state.base.word_28, 8, "payload follows the H8 extent 427");
    assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
    assert_eq!(state.base.dirty_word, 2);
    assert_eq!(arena.actor(object).unwrap().rotating.basis.words[0], 3);
    assert_eq!(arena.actor(object).unwrap().word_74, 0);
    arena.finish_actor_initialization(object).unwrap();
    assert_eq!(arena.actor(object).unwrap().word_74, 1);
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 2);
    assert_eq!(
        arena.object(object).unwrap().base.flags & INITIALIZED_FLAG,
        0
    );
}

#[test]
fn wrong_class_and_context_resource_errors_precede_payload_or_membership_mutation() {
    let fixture = models();
    let math = math();
    let payload = payload(0, 500, 99, 3000);
    let mut arena = arena();
    let visual = arena.allocate(ObjectClassKind::RotatingVisualBase);
    let object = actor(&mut arena);
    for (target, model, expected) in [
        (visual, 1, ObjectError::NotActor(visual)),
        (
            object,
            2,
            ObjectError::ModelSlotBoundary {
                slot: 2,
                capacity: 2,
            },
        ),
    ] {
        let before = arena.object(target).unwrap().clone();
        let nodes = lists(&arena);
        assert_eq!(
            arena.initialize_actor(
                target,
                ActorInitializationRequest {
                    context: context(model),
                    payload: &payload,
                    local_owner: 2,
                },
                &fixture.runtime,
                &math
            ),
            Err(expected)
        );
        assert_eq!(arena.object(target).unwrap(), &before);
        assert_eq!(lists(&arena), nodes);
    }
    let before = arena.object(visual).unwrap().clone();
    let nodes = lists(&arena);
    assert_eq!(
        arena.initialize_actor_payload(visual, &payload, 1),
        Err(ObjectError::NotActor(visual))
    );
    assert_eq!(
        arena.finish_actor_initialization(visual),
        Err(ObjectError::NotActor(visual))
    );
    assert_eq!(arena.object(visual).unwrap(), &before);
    assert_eq!(lists(&arena), nodes);
}

#[test]
fn conditional_operand_and_grant_bounds_are_checked_only_with_a_first_player() {
    let fixture = models();
    let mut arena = arena();
    let object = actor(&mut arena);
    let before = arena.object(object).unwrap().clone();
    let nodes = lists(&arena);
    let mask = [0; GRANT_MASK_WORDS];
    for code in [13, 14] {
        for (operands, expected) in [
            (
                vec![],
                ActorCommandError::MissingConditionalWord {
                    code,
                    allocation_offset: 4,
                },
            ),
            (
                0xffff_u16.to_le_bytes().to_vec(),
                ActorCommandError::Grant(GrantClosureError::Id(65535)),
            ),
        ] {
            let view = command(code, &operands);
            assert_eq!(
                arena.apply_actor_command(
                    object,
                    &view,
                    ActorCommandContext {
                        terrain: None,
                        runtime: &fixture.runtime,
                        first_player_grants: None,
                    }
                ),
                Ok(true)
            );
            assert_eq!(
                arena.apply_actor_command(
                    object,
                    &view,
                    ActorCommandContext {
                        terrain: None,
                        runtime: &fixture.runtime,
                        first_player_grants: Some(&mask),
                    }
                ),
                Err(expected)
            );
            assert_eq!(arena.object(object).unwrap(), &before);
            assert_eq!(lists(&arena), nodes);
        }
    }
}

#[test]
fn conditional_disposal_uses_current_first_player_mask_and_keeps_actor_custody() {
    let fixture = models();
    for (code, index, granted, disposed) in [
        (13, 0_u16, false, false),
        (14, 0, false, true),
        (13, 191, false, true),
        (13, 191, true, false),
        (14, 191, false, false),
        (14, 191, true, true),
    ] {
        let mut arena = arena();
        let object = actor(&mut arena);
        let handle = arena.actor(object).unwrap().handle;
        let mut mask = [0; GRANT_MASK_WORDS];
        if granted {
            mask[5] = 0x8000_0000;
        }
        assert_eq!(
            arena.apply_actor_command(
                object,
                &command(code, &index.to_le_bytes()),
                ActorCommandContext {
                    terrain: None,
                    runtime: &fixture.runtime,
                    first_player_grants: Some(&mask),
                }
            ),
            Ok(true)
        );
        assert_eq!(!arena.list(ObjectList::Disposal).is_empty(), disposed);
        assert_eq!(arena.actor_list(), &[object]);
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(object));
    }
}
