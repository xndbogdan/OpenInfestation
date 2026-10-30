use super::*;
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
};

const SINE: [u16; 4096] = [0; 4096];
fn request(position: Option<[u16; 3]>) -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: position,
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

#[test]
fn constructor_initializes_embedded_handles_and_preserves_holes_with_one_rng_step() {
    let mut arena = arena();
    let mut rng = 0x12345678;
    let mut expected = rng;
    name_entry_rng_next(&mut expected);
    let object = arena
        .allocate_family428650_actor(request(Some([1, 2, 3])), &mut rng)
        .unwrap();
    assert_eq!(rng, expected);
    let state = arena.family428650_actor(object).unwrap();
    assert_eq!(
        (state.word_184, state.count_1a0, state.word_214),
        (-1, 0, 256)
    );
    assert_eq!(state.position_222, None);
    assert_eq!(state.words_188_19c, [None; 6]);
    assert_eq!(state.words_204_208, [None; 2]);
    assert_eq!((state.word_220, state.byte_22d), (None, None));
    assert_eq!(state.words_1d8_200, [0; 6]);
    assert_eq!(state.word_20c, 0x1234);
    assert_eq!(
        state.entries_1a4,
        [FamilyHandleEntry {
            handle: ActorHandle::from_raw(0x1234),
            word_04: 0,
            word_08: 0
        }; 4]
    );
    let handle = arena.actor(object).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
}

#[test]
fn guard_copies_current_position_after_central_behavior_and_errors_only_at_missing_read() {
    let mut arena = arena();
    let object = arena
        .allocate_family428650_actor(request(Some([0xffff, 0x8000, 42])), &mut 0)
        .unwrap();
    arena.derived_actor_mut(object).unwrap().byte_13d = Some(1);
    arena.family428650_actor_mut(object).unwrap().position_222 = Some([10, 20, 30]);
    assert!(arena.admit_family428650_actor(object).unwrap());
    assert_eq!(
        arena.family428650_actor(object).unwrap().position_222,
        Some([0xffff, 0x8000, 42])
    );
    assert!(arena.derived_actor(object).unwrap().behavior_140.is_some());
    arena.owned_mut(object).unwrap().state.base.position_words = None;
    arena.derived_actor_mut(object).unwrap().behavior_140 = None;
    assert_eq!(
        arena.admit_family428650_actor(object),
        Err(DerivedActorError::MissingPosition(object))
    );
    assert!(arena.derived_actor(object).unwrap().behavior_140.is_some());
    assert_eq!(
        arena.family428650_actor(object).unwrap().position_222,
        Some([0xffff, 0x8000, 42])
    );
}

#[test]
fn after_load_skips_exact_zero_and_reinterprets_nonzero_handles_on_every_call() {
    let mut arena = arena();
    let target = arena.allocate_actor(request(None).actor).unwrap();
    arena
        .initialize_context(
            target,
            BaseContext {
                position_words: None,
                placement_index: 4,
            },
        )
        .unwrap();
    let handle = arena.actor(target).unwrap().handle;
    let object = arena
        .allocate_family428650_actor(request(None), &mut 0)
        .unwrap();
    arena.family428650_actor_mut(object).unwrap().word_20c = 5;
    arena.initialize_family428650_after_load(object).unwrap();
    assert_eq!(
        arena.family428650_actor(object).unwrap().word_20c,
        handle.raw()
    );
    arena.initialize_family428650_after_load(object).unwrap();
    assert_eq!(arena.family428650_actor(object).unwrap().word_20c, 0x1234);
    arena.family428650_actor_mut(object).unwrap().word_20c = 0;
    arena.initialize_family428650_after_load(object).unwrap();
    assert_eq!(arena.family428650_actor(object).unwrap().word_20c, 0);
    arena.family428650_actor_mut(object).unwrap().word_20c = u32::MAX;
    arena.initialize_family428650_after_load(object).unwrap();
    assert_eq!(arena.family428650_actor(object).unwrap().word_20c, 0x1234);
}
