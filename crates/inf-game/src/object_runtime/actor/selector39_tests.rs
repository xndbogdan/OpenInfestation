use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};

const SINE: [u16; 4096] = [0; 4096];

pub(crate) fn arena_with_target() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let object = arena
        .allocate_selector39(ActorConstructionRequest {
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

/// 0040D800 then 0043B6D0's zero stores.
#[test]
fn constructor_writes_the_family_prefix_and_zero_target_fields() {
    let (arena, object) = arena_with_target();
    let state = arena.selector39(object).unwrap();
    assert_eq!(state.intermediate, Actor40d800State::new());
    assert_eq!(state.retained_158, None);
    assert_eq!(state.word_15c, 0);
    assert_eq!(
        [
            state.byte_160,
            state.byte_161,
            state.byte_162,
            state.byte_163,
            state.byte_164,
            state.byte_165,
            state.byte_166,
        ],
        [0; 7]
    );
}

/// 0043BDF0 takes link codes through 0040DE70; its own 0xB/0xC (which can
/// retire the actor) and 0x14/0x15 stay explicit boundaries.
#[test]
fn messages_reach_the_family_handler_except_the_service_codes() {
    let (mut arena, object) = arena_with_target();
    for code in [0xb, 0xc, 0x14, 0x15] {
        assert_eq!(
            arena.selector39_message(object, code),
            Err(ObjectError::LinkMessageBoundary(object))
        );
    }
    assert_eq!(
        arena.selector39_message(object, 7).unwrap(),
        FamilyLinkChange::None
    );
    assert_eq!(arena.selector39(object).unwrap().intermediate.byte_14c, 1);
    assert_eq!(
        arena.selector39_message(object, 0x16).unwrap(),
        FamilyLinkChange::None
    );
}

/// The after-load of a target without a retained list or completion only
/// clears +138, and leaves the target alive.
#[test]
fn after_load_without_links_or_completion_only_clears_byte_138() {
    let (mut arena, object) = arena_with_target();
    let reached = arena
        .initialize_selector39_after_load(object, &[0; 4], &mut |_, _| unreachable!("no links"))
        .unwrap();
    assert_eq!(reached, Selector39AfterLoad::default());
    assert_eq!(arena.selector39(object).unwrap().intermediate.byte_138, 0);
    assert!(arena.object(object).is_ok());
}

/// A completed target (0048EA80 over signed +78) disposes itself and, with
/// +162, sets session byte G+1C3.
#[test]
fn completed_target_disposes_and_reports_g1c3() {
    let (mut arena, object) = arena_with_target();
    arena.actor_mut(object).unwrap().byte_78 = 33;
    arena.selector39_mut(object).unwrap().byte_162 = 1;
    let mut completion = [0; 4];
    completion[1] = 1 << 1;
    let reached = arena
        .initialize_selector39_after_load(object, &completion, &mut |_, _| unreachable!("no links"))
        .unwrap();
    assert!(reached.disposed && reached.sets_session_1c3);
    let listed: Vec<_> = arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .map(|node| node.object)
        .collect();
    assert_eq!(listed, [object]);
}
