use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        actor::ActorConstructionRequest,
        native_model_fixture::{Fixture, Models},
        ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    },
};
use inf_formats::{
    decode_instance_command, ByteSpan, InstanceCommandReadScope, InstanceCommandRequest,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
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
        },
        world_mask_limits: [3, 3],
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

fn resident() -> Fixture {
    Fixture::new(
        &serde_json::from_value::<Models>(serde_json::json!({"capacity":0,"entries":[]})).unwrap(),
    )
}

fn command(code: i32, operand: &[u8]) -> InstanceCommandView {
    let mut bytes = code.to_le_bytes().to_vec();
    bytes.extend_from_slice(operand);
    decode_instance_command(InstanceCommandRequest {
        source: &bytes,
        main_payload: ByteSpan::new(0, bytes.len()),
        relocations: &[],
        command_offset: 0,
        target: InstanceCommandTarget::Constructor(11),
        read_scope: InstanceCommandReadScope::Dispatch,
    })
    .unwrap()
}

#[test]
fn constructor_adds_no_rng_draw_and_preserves_unknown_fields_and_shared_custody() {
    let mut arena = arena();
    let mut rng = 0x1234_5678;
    let mut expected_rng = rng;
    name_entry_rng_next(&mut expected_rng);
    let object = arena
        .allocate_selector11_actor(request(), &mut rng)
        .unwrap();
    assert_eq!(rng, expected_rng);
    let state = arena.selector11_actor(object).unwrap();
    assert_eq!(
        (
            state.word_184,
            state.word_19c,
            state.word_1aa,
            state.word_1ac
        ),
        (None, None, None, None)
    );
    assert_eq!(state.byte_1a3, None);
    assert_eq!(
        [
            state.word_188,
            state.word_18c,
            state.word_190,
            state.word_194,
            state.word_198
        ],
        [0; 5]
    );
    assert_eq!((state.word_1a0, state.byte_1a2), (2000, 9));
    assert_eq!(
        (state.word_1a4, state.word_1a6, state.word_1a8),
        (50, 512, 256)
    );
    assert_eq!((state.word_1ae, state.word_1b0, state.word_1b2), (0, 0, 0));
    let handle = arena.actor(object).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.actor_list().is_empty());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);

    let mut unconfigured = ObjectArena::new();
    let before_rng = rng;
    assert_eq!(
        unconfigured.allocate_selector11_actor(request(), &mut rng),
        Err(ActorConstructionError::MissingRegistry)
    );
    assert_eq!(rng, before_rng);
    assert!(unconfigured.is_empty());
}

#[test]
fn flag_commands_keep_full_operand_width_and_preserve_neighbor_bits() {
    let resident = resident();
    let context = ActorCommandContext {
        terrain: None,
        runtime: &resident.runtime,
        first_player_grants: None,
    };
    let mut arena = arena();
    let object = arena.allocate_selector11_actor(request(), &mut 0).unwrap();
    let DerivedActorClass::Selector11(state) = &mut arena.derived_actor_mut(object).unwrap().class
    else {
        unreachable!()
    };
    state.word_1ae = 0xa5e9;
    let before_base = arena.object(object).unwrap().base.clone();
    for (code, operand, expected) in [
        (19, 0x8000_0000_u32.to_le_bytes().to_vec(), 0xa5eb),
        (22, 0x0100_u16.to_le_bytes().to_vec(), 0xa5ef),
        (23, 0x8000_u16.to_le_bytes().to_vec(), 0xa5ff),
        (19, 0_u32.to_le_bytes().to_vec(), 0xa5fd),
        (22, 0_u16.to_le_bytes().to_vec(), 0xa5f9),
        (23, 0_u16.to_le_bytes().to_vec(), 0xa5e9),
    ] {
        assert!(arena
            .apply_selector11_command(object, &command(code, &operand), context)
            .unwrap());
        assert_eq!(arena.selector11_actor(object).unwrap().word_1ae, expected);
        assert_eq!(arena.object(object).unwrap().base, before_base);
    }
    // The copy writes exactly 1A0..1A9, including the constructor's hole at 1A3.
    let before = arena.selector11_actor(object).unwrap().clone();
    assert!(arena
        .apply_selector11_command(
            object,
            &command(20, &[0, 255, 128, 129, 1, 2, 3, 4, 5, 6]),
            context
        )
        .unwrap());
    let mut expected = before;
    expected.word_1a0 = 0xff00;
    expected.byte_1a2 = 0x80;
    expected.byte_1a3 = Some(0x81);
    expected.word_1a4 = 0x0201;
    expected.word_1a6 = 0x0403;
    expected.word_1a8 = 0x0605;
    assert_eq!(arena.selector11_actor(object).unwrap(), &expected);
    assert_eq!(arena.object(object).unwrap().base, before_base);
}

#[test]
fn behavior_command_skips_absent_operand_until_behavior_exists() {
    let resident = resident();
    let context = ActorCommandContext {
        terrain: None,
        runtime: &resident.runtime,
        first_player_grants: None,
    };
    let mut arena = arena();
    let object = arena.allocate_selector11_actor(request(), &mut 0).unwrap();
    let missing = command(21, &[]);
    let before = arena.object(object).unwrap().clone();
    assert!(arena
        .apply_selector11_command(object, &missing, context)
        .unwrap());
    assert_eq!(arena.object(object).unwrap(), &before);
    arena.derived_actor_mut(object).unwrap().byte_13d = Some(1);
    arena.admit_derived_actor(object).unwrap();
    let behavior = arena
        .derived_actor_mut(object)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    behavior.word_04 = 0x7654_3210;
    behavior.word_08 = 0xabcd_ef01;
    behavior.byte_0c = 0;
    let before = arena.object(object).unwrap().clone();
    assert_eq!(
        arena.apply_selector11_command(object, &missing, context),
        Err(ActorCommandError::MissingConditionalDword {
            code: 21,
            allocation_offset: 4
        })
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    let mut expected = arena
        .derived_actor(object)
        .unwrap()
        .behavior_140
        .clone()
        .unwrap();
    expected.word_04 = 0xffff_0123;
    expected.byte_0c = 1;
    assert!(arena
        .apply_selector11_command(
            object,
            &command(21, &0xffff_0123_u32.to_le_bytes()),
            context
        )
        .unwrap());
    assert_eq!(
        arena.derived_actor(object).unwrap().behavior_140.as_ref(),
        Some(&expected)
    );
}

#[test]
fn delegation_rejection_and_postinit_keep_their_original_owners() {
    let resident = resident();
    let context = ActorCommandContext {
        terrain: None,
        runtime: &resident.runtime,
        first_player_grants: None,
    };
    let mut arena = arena();
    let object = arena.allocate_selector11_actor(request(), &mut 0).unwrap();
    for code in [i32::MIN, -1, 24, i32::MAX] {
        let before = arena.object(object).unwrap().clone();
        assert!(!arena
            .apply_selector11_command(object, &command(code, &[]), context)
            .unwrap());
        assert_eq!(arena.object(object).unwrap(), &before);
    }
    assert!(arena
        .apply_selector11_command(object, &command(17, &0xffff_u16.to_le_bytes()), context)
        .unwrap());
    assert_eq!(arena.derived_actor(object).unwrap().word_164, 65535);
    arena.derived_actor_mut(object).unwrap().byte_13d = Some(1);
    arena.admit_derived_actor(object).unwrap();
    // Shared code 6 maps input 2 to selector 18; postinit must reselect behavior.
    assert!(arena
        .apply_selector11_command(object, &command(6, &2_i32.to_le_bytes()), context)
        .unwrap());
    assert_eq!(arena.actor(object).unwrap().word_b4, 18);
    arena.owned_mut(object).unwrap().state.base.word_28 = 0xffff;
    let before_base = arena.object(object).unwrap().base.clone();
    let before_extension = arena.selector11_actor(object).unwrap().clone();
    arena.finish_selector11_initialization(object).unwrap();
    assert_eq!(arena.actor(object).unwrap().word_74, 0x1fff);
    assert_eq!(
        arena
            .derived_actor(object)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .byte_10,
        0
    );
    assert_eq!(arena.object(object).unwrap().base, before_base);
    assert_eq!(arena.selector11_actor(object).unwrap(), &before_extension);

    let base = arena.allocate_derived_actor(request(), &mut 0).unwrap();
    let before = arena.object(base).unwrap().clone();
    assert_eq!(
        arena.apply_selector11_command(base, &command(21, &[]), context),
        Err(ActorCommandError::WrongTarget)
    );
    assert_eq!(
        arena.finish_selector11_initialization(base),
        Err(DerivedActorError::NotSelector11(base))
    );
    assert_eq!(arena.object(base).unwrap(), &before);
}

/// 00402420: message 7 counts DWORD +198 and locks from state 2; message 8
/// clears +190 and uncounts, and the last link unlocks from state 3.
#[test]
fn link_messages_count_in_198_and_lock_or_unlock_by_state() {
    let mut arena = arena();
    let mut rng = 1;
    let object = arena
        .allocate_selector11_actor(request(), &mut rng)
        .unwrap();
    let source = arena.actor(object).unwrap().handle;
    let change = |target_state| ActorStateChangeRequest {
        target_state,
        argument: 0,
        source,
    };
    // A fresh actor's state 3 counts the link without locking.
    arena.actor_mut(object).unwrap().word_c4 = 3;
    assert_eq!(
        arena.selector11_link_message(object, 7).unwrap(),
        FamilyLinkChange::None
    );
    assert_eq!(arena.selector11_actor(object).unwrap().word_198, 1);
    if let DerivedActorClass::Selector11(state) =
        &mut arena.derived_actor_mut(object).unwrap().class
    {
        state.word_190 = 5;
    }
    assert_eq!(
        arena.selector11_link_message(object, 8).unwrap(),
        FamilyLinkChange::Unlock(change(2))
    );
    let state = arena.selector11_actor(object).unwrap();
    assert_eq!((state.word_190, state.word_198), (0, 0));
    arena.actor_mut(object).unwrap().word_c4 = 2;
    assert_eq!(
        arena.selector11_link_message(object, 7).unwrap(),
        FamilyLinkChange::Lock(change(3))
    );
    assert_eq!(
        arena.selector11_link_message(object, 0xb),
        Err(ObjectError::LinkMessageBoundary(object))
    );
}
