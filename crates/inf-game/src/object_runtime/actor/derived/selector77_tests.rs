use super::*;
use crate::object_runtime::{
    actor::ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

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

#[test]
fn selector77_consumes_two_shared_rng_steps_and_preserves_unwritten_allocation_fields() {
    for (seed, final_rng, word_198) in [
        (0, 0x60a7_666a, 0x1829c),
        (1, 0xc0c3_f6d3, 0x3030c),
        (u32::MAX, 0x008a_d601, 0x228),
        (0x1234_5678, 0xbde2_5da2, 0x2f788),
    ] {
        let mut arena = arena();
        let mut rng = seed;
        let object = arena
            .allocate_selector77_actor(request(None), &mut rng)
            .unwrap();
        assert_eq!(rng, final_rng);
        let derived = arena.derived_actor(object).unwrap();
        assert_eq!(derived.word_178, 7);
        let state = arena.selector77_actor(object).unwrap();
        assert_eq!(state.words_184_190, [0; 7]);
        assert_eq!(state.word_198, word_198);
        assert_eq!(state.position_192, None);
        assert_eq!((state.word_19c, state.word_19e), (None, None));
        assert_eq!((state.word_1a0, state.word_1ac), (None, None));
        assert_eq!((state.word_1a4, state.word_1a8), (0, 0));
        assert_eq!(state.handle_1b0.raw(), 0x1234);
        // The selector has no additional destruction custody above central.
        let handle = arena.actor(object).unwrap().handle;
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert!(arena.actor_list().is_empty());
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
    }
    let mut missing_registry = ObjectArena::new();
    let mut rng = 0x1234;
    assert_eq!(
        missing_registry.allocate_selector77_actor(request(None), &mut rng),
        Err(ActorConstructionError::MissingRegistry)
    );
    assert_eq!(rng, 0x1234);
    assert!(missing_registry.is_empty());
}

#[test]
fn selector77_postinitialization_reselects_behavior_and_copies_current_raw_position() {
    let mut arena = arena();
    let object = arena
        .allocate_selector77_actor(request(Some([1, 2, 3])), &mut 0)
        .unwrap();
    arena.derived_actor_mut(object).unwrap().byte_13d = Some(1);
    arena.admit_derived_actor(object).unwrap();
    assert_eq!(
        arena
            .derived_actor(object)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .byte_10,
        1
    );
    arena.actor_mut(object).unwrap().word_b4 = 18;
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: Some([0xffff, 0x8000, 0x1234]),
                placement_index: 9,
            },
        )
        .unwrap();
    arena.owned_mut(object).unwrap().state.base.word_28 = 0xffff;
    let before = arena.object(object).unwrap().base.clone();
    arena.finish_selector77_initialization(object).unwrap();
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
    assert_eq!(arena.actor(object).unwrap().word_74, 0x1fff);
    assert_eq!(
        arena.selector77_actor(object).unwrap().position_192,
        Some([0xffff, 0x8000, 0x1234])
    );
    assert_eq!(arena.object(object).unwrap().base, before);
    // Repeated postinit observes current position, not a cached ctor snapshot.
    arena.owned_mut(object).unwrap().state.base.position_words = None;
    arena.finish_selector77_initialization(object).unwrap();
    assert_eq!(arena.selector77_actor(object).unwrap().position_192, None);
    let base = arena.allocate_derived_actor(request(None), &mut 0).unwrap();
    let before = arena.actor(base).unwrap().clone();
    assert_eq!(
        arena.finish_selector77_initialization(base),
        Err(DerivedActorError::NotSelector77(base))
    );
    assert_eq!(arena.actor(base).unwrap(), &before);
}
