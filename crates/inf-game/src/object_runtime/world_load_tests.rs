use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectNodeKind};

#[test]
fn completion_bit_matches_native_truncating_division_and_masked_shift() {
    let mut completion = [0; 4];
    completion[0] = 1 << 31;
    // -1 / 32 truncates to word 0 and (-1 & 31) selects bit 31.
    assert!(world_completion_bit(&completion, -1));
    assert!(!world_completion_bit(&completion, 31 - 64));
    assert!(!world_completion_bit(&completion, i8::MIN));
    completion = [0, 0, 0, 1 << 31];
    assert!(world_completion_bit(&completion, 127));
    assert!(!world_completion_bit(&completion, 95));
}

#[test]
fn traversal_visits_flag0_then_parked_nodes_once_per_object_and_drains_disposal() {
    let mut arena = ObjectArena::new();
    let enabled = arena.allocate(ObjectClassKind::Base);
    let parked = arena.allocate(ObjectClassKind::VisualBase);
    let disposed = arena.allocate(ObjectClassKind::Base);
    arena.set_flag0(parked, false).unwrap();
    arena.dispose(disposed).unwrap();
    // The parked object now has two nodes in G+30; only one callback runs.
    assert_eq!(
        arena
            .list(ObjectList::Parking)
            .iter()
            .filter(|node| node.object == parked)
            .count(),
        2
    );
    let before = arena.object(enabled).unwrap().base.dirty_word;
    arena
        .finish_world_placements(ObjectAfterLoadInputs::default(), None, None)
        .unwrap();
    assert!(arena.object(disposed).is_err());
    assert!(arena.list(ObjectList::Disposal).is_empty());
    for object in [enabled, parked] {
        assert_ne!(
            arena.object(object).unwrap().base.flags & AFTER_LOAD_FLAG,
            0
        );
    }
    // Flag 0x00800000 lies outside the low nine bits and marks nothing dirty.
    assert_eq!(arena.object(enabled).unwrap().base.dirty_word, before);
    assert_eq!(
        arena.list(ObjectList::Flag0Primary),
        [ObjectNode {
            object: enabled,
            kind: ObjectNodeKind::Flag0
        }]
    );
}

#[test]
fn already_flagged_objects_are_skipped_and_a_second_pass_changes_nothing() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Base);
    arena
        .finish_world_placements(ObjectAfterLoadInputs::default(), None, None)
        .unwrap();
    let state = arena.object(object).unwrap().clone();
    arena
        .finish_world_placements(ObjectAfterLoadInputs::default(), None, None)
        .unwrap();
    assert_eq!(*arena.object(object).unwrap(), state);
}

#[test]
fn loaded_actor_registration_requires_an_actor() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Base);
    arena.begin_world_placements();
    assert_eq!(
        arena.register_loaded_actor(object, Some(&[0; 4])),
        Err(ObjectError::NotActor(object))
    );
    assert_eq!(
        arena.world_load_state(),
        WorldLoadState {
            actor_serial_3d0: 0,
            flag_54ece4: true,
            pickup_serial_54ed48: 0,
            pickup_words: None,
            session_1c3: false,
            world_completion: None,
        }
    );
}

fn actor_request() -> crate::object_runtime::ActorConstructionRequest<'static> {
    crate::object_runtime::ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0; 3]),
        owner: 1,
        terrain: crate::object_runtime::ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &[0; 4096],
        },
    }
}

/// 0047F6C0 asks a completed placement's virtual +54. The shared actor's
/// 00401300 returns zero, so 00479FA0 queues it for the post-placement
/// drain; selector 30's 00404610 returns the gate, which stays. A placement
/// whose serial is not completed is never asked.
#[test]
fn completed_placements_dispose_unless_their_plus54_keeps_them() {
    let mut arena = ObjectArena::with_actor_registry(crate::object_runtime::ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: crate::object_runtime::ActorHandle::from_raw(0),
    })
    .unwrap();
    let shared = arena.allocate_actor(actor_request()).unwrap();
    let gate = arena.allocate_selector30_actor(actor_request()).unwrap();
    let pending = arena.allocate_actor(actor_request()).unwrap();
    for object in [shared, gate, pending] {
        arena.owned_mut(object).unwrap().state.base.placement_word = 0;
    }
    arena.begin_world_placements();
    // Serials 0 and 1 are completed; serial 2 is not.
    let completion = [0b011, 0, 0, 0];
    for object in [shared, gate, pending] {
        arena
            .register_loaded_actor(object, Some(&completion))
            .unwrap();
    }
    let disposed: Vec<_> = arena
        .list(ObjectList::Disposal)
        .iter()
        .map(|node| node.object)
        .collect();
    assert_eq!(disposed, [shared]);
    assert!(arena.actor_completion_kept(gate).unwrap());
    assert!(!arena.actor_completion_kept(pending).unwrap());
    assert_eq!(arena.actor(gate).unwrap().byte_78, 1);
}
