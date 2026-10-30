use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};

const SINE: [u16; 4096] = [0; 4096];

fn arena_with_bridge() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let object = arena
        .allocate_selector37(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 0, 0]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    (arena, object)
}

/// 0040D800 then 004099A0 with case 0x25's arguments 0x400, 0x100, 0.
#[test]
fn constructor_writes_the_family_prefix_and_the_strip_defaults() {
    let (arena, object) = arena_with_bridge();
    let state = arena.selector37(object).unwrap();
    assert_eq!(state.intermediate, Actor40d800State::new());
    assert_eq!(state.intermediate.word_144, 0xffff);
    assert_eq!(
        (state.word_158, state.word_15c, state.word_160),
        (0x400, 0x100, 0)
    );
    assert_eq!((state.byte_164, state.byte_165), (4, 0));
    assert_eq!(
        (state.word_170, state.word_17c, state.word_17e),
        (4_000_000, 0xffff, 0)
    );
    assert_eq!((state.word_174, state.word_178), (None, None));
}

/// 0040A180 restarts +170 from the raw +164 BYTE (not microseconds) for
/// message 2 with +167 clear or message 3 with +167 set, then always runs
/// 0040DE70, whose message 7 counts a link and sets +14D.
#[test]
fn messages_restart_the_raw_timer_and_count_links() {
    let (mut arena, object) = arena_with_bridge();
    arena.selector37_mut(object).unwrap().word_170 = 77;
    assert_eq!(
        arena.selector37_message(object, 3).unwrap(),
        FamilyLinkChange::None
    );
    assert_eq!(arena.selector37(object).unwrap().word_170, 77);
    arena.selector37_message(object, 2).unwrap();
    assert_eq!(arena.selector37(object).unwrap().word_170, 4);
    arena.selector37_mut(object).unwrap().byte_167 = 1;
    arena.selector37_mut(object).unwrap().word_170 = 77;
    arena.selector37_message(object, 2).unwrap();
    assert_eq!(arena.selector37(object).unwrap().word_170, 77);
    arena.selector37_message(object, 3).unwrap();
    assert_eq!(arena.selector37(object).unwrap().word_170, 4);
    // Fresh actors are lifecycle 3: links are counted without a lock.
    for _ in 0..3 {
        assert_eq!(
            arena.selector37_message(object, 7).unwrap(),
            FamilyLinkChange::None
        );
    }
    let family = &arena.selector37(object).unwrap().intermediate;
    assert_eq!((family.byte_14c, family.byte_14d), (3, 1));
}

/// 0040DE70: message 7 on a state-2 actor with a positive count locks it
/// (0047D670 to 3); the last message 8 on a linked state-3 actor unlocks it.
#[test]
fn link_messages_lock_from_state_two_and_unlock_after_the_last_release() {
    let (mut arena, object) = arena_with_bridge();
    let handle = arena.actor(object).unwrap().handle;
    arena.actor_mut(object).unwrap().word_c4 = 2;
    let lock = arena.selector37_message(object, 7).unwrap();
    assert_eq!(
        lock,
        FamilyLinkChange::Lock(super::super::ActorStateChangeRequest {
            target_state: 3,
            argument: 0,
            source: handle,
        })
    );
    arena.actor_mut(object).unwrap().word_c4 = 3;
    arena.selector37_message(object, 7).unwrap();
    assert_eq!(
        arena.selector37_message(object, 8).unwrap(),
        FamilyLinkChange::None
    );
    assert_eq!(
        arena.selector37_message(object, 8).unwrap(),
        FamilyLinkChange::Unlock(super::super::ActorStateChangeRequest {
            target_state: 2,
            argument: 0,
            source: handle,
        })
    );
    // Further releases keep testing the signed count.
    assert!(matches!(
        arena.selector37_message(object, 8).unwrap(),
        FamilyLinkChange::Unlock(_)
    ));
    assert_eq!(
        arena.selector37(object).unwrap().intermediate.byte_14c,
        0xff
    );
}
