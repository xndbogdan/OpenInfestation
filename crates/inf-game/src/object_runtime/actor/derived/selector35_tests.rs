use super::*;
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::{
    native_model_fixture::{Fixture, Models},
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs, RuntimeAllocationError,
};
use inf_formats::{
    decode_instance_command, ByteSpan, InstanceCommandReadScope, InstanceCommandRequest,
};

const SINE: [u16; 4096] = [0; 4096];
fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([1, 2, 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
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
fn allocation(bytes: Vec<u8>) -> Arc<RuntimeAllocation> {
    let main = ByteSpan::new(0, bytes.len());
    Arc::new(RuntimeAllocation::new(bytes.into(), main).unwrap())
}

#[test]
fn constructor_adds_no_rng_and_preserves_partly_initialized_packed_word() {
    let mut arena = arena();
    let mut rng = 1;
    let mut expected = rng;
    name_entry_rng_next(&mut expected);
    let object = arena
        .allocate_selector35_actor(request(), &mut rng)
        .unwrap();
    assert_eq!(rng, expected);
    let state = arena.selector35_actor(object).unwrap();
    assert_eq!(
        (
            state.word_230,
            state.word_234,
            state.byte_238,
            state.byte_239
        ),
        (500, 512, 1, 0)
    );
    assert_eq!(state.bytes_240, [None, None, Some(0), Some(0)]);
    assert_eq!(state.byte_251, None);
    assert_eq!(state.table_270, None);
    assert_eq!(
        (state.word_258, state.word_25c, state.word_260),
        (None, None, None)
    );
    assert!(state.retained_23c.is_none());
    assert_eq!(state.handle_268.raw(), 0x1234);
    assert_eq!(arena.family428650_actor(object).unwrap().word_184, -1);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.actor_list().is_empty());
}

#[test]
fn retained_pointer_has_allocation_identity_one_past_and_lazy_unsigned_ratio_reads() {
    let mut arena = arena();
    let object = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
    let short = allocation(vec![0, 0]);
    arena.set_selector35_retained(object, &short, 0).unwrap();
    arena.finish_selector35_initialization(object).unwrap();
    assert_eq!(arena.family428650_actor(object).unwrap().word_214, 256);
    short.write_word(0, 1).unwrap();
    assert!(matches!(
        arena.finish_selector35_initialization(object),
        Err(DerivedActorError::Allocation(
            RuntimeAllocationError::Range { offset: 16, .. }
        ))
    ));
    let owner = allocation(vec![0; 18]);
    owner.write_word(0, 0xffff).unwrap();
    owner.write_word(16, 0x8000).unwrap();
    arena.set_selector35_retained(object, &owner, 0).unwrap();
    assert!(Arc::ptr_eq(
        arena
            .selector35_actor(object)
            .unwrap()
            .retained_23c
            .as_ref()
            .unwrap()
            .allocation(),
        &owner
    ));
    arena.finish_selector35_initialization(object).unwrap();
    assert_eq!(
        arena.family428650_actor(object).unwrap().word_214,
        (0x8000_u32 << 10) / 0xffff
    );
    owner.write_word(0, 2).unwrap();
    arena.finish_selector35_initialization(object).unwrap();
    assert_eq!(
        arena.family428650_actor(object).unwrap().word_214,
        0x1000000
    );
    arena.set_selector35_retained(object, &owner, 18).unwrap();
    assert!(matches!(
        arena.finish_selector35_initialization(object),
        Err(DerivedActorError::Allocation(
            RuntimeAllocationError::Range { offset: 18, .. }
        ))
    ));
    let before = arena.selector35_actor(object).unwrap().retained_23c.clone();
    assert!(arena.set_selector35_retained(object, &owner, 19).is_err());
    assert_eq!(arena.selector35_actor(object).unwrap().retained_23c, before);
}

#[test]
fn packed_command_preserves_input_byte242_zeros243_and_does_not_mark_low_flags_dirty() {
    let fixture = Fixture::new(
        &serde_json::from_value::<Models>(serde_json::json!({"capacity":0,"entries":[]})).unwrap(),
    );
    let mut arena = arena();
    let object = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
    arena.clear_dirty(object).unwrap();
    let before = arena.object(object).unwrap().base.flags;
    let mut bytes = 24_i32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&0xabcd1234_u32.to_le_bytes());
    let command = decode_instance_command(InstanceCommandRequest {
        source: &bytes,
        main_payload: ByteSpan::new(0, 8),
        relocations: &[],
        command_offset: 0,
        target: InstanceCommandTarget::Constructor(35),
        read_scope: InstanceCommandReadScope::Dispatch,
    })
    .unwrap();
    assert!(arena
        .apply_selector35_command(
            object,
            &command,
            ActorCommandContext {
                runtime: &fixture.runtime,
                first_player_grants: None,
                terrain: None
            }
        )
        .unwrap());
    assert_eq!(
        arena.selector35_actor(object).unwrap().bytes_240,
        [Some(0x34), Some(0x12), Some(0xcd), Some(0)]
    );
    assert_eq!(
        arena.object(object).unwrap().base.flags,
        before | 0x10000000
    );
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
}
