use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectList};

// State setup isolates the post-placement finalizer. Canonical construction
// and native routine receipts are compared in separate suites.
fn node(
    arena: &mut ObjectArena,
    identity: u16,
    predecessor: u16,
    flags: u8,
    position: [u16; 3],
) -> ObjectId {
    let object = arena.allocate(ObjectClassKind::RouteNode);
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: Some(position),
                placement_index: u32::from(identity),
            },
        )
        .unwrap();
    let route = arena.route_mut(object).unwrap();
    route.identity_word = identity;
    route.predecessor_word = predecessor;
    route.flags = flags;
    object
}

fn groups(arena: &ObjectArena) -> Vec<Vec<ObjectId>> {
    arena
        .route_groups()
        .iter()
        .map(|group| group.members.clone())
        .collect()
}

#[test]
fn reverse_pending_order_requires_repeated_append_and_root_policy_wins() {
    let mut arena = ObjectArena::new();
    let end = node(&mut arena, 12, 11, 0xff, [10, 10, 10]);
    let middle = node(&mut arena, 11, 10, 0x01, [3, 4, 0]);
    let root = node(&mut arena, 10, 10, 0x12, [0, 0, 0]);
    let other = node(&mut arena, 20, 20, 0, [0, 0, 0]);
    assert_eq!(arena.route_pending(), [end, middle, root, other]);
    arena.finalize_routes().unwrap();
    assert!(arena.route_pending().is_empty());
    assert_eq!(groups(&arena), vec![vec![root, middle, end], vec![other]]);
    for object in [root, middle, end] {
        assert_eq!(arena.route_node(object).unwrap().flags, 0x12);
    }
    assert_eq!(arena.route_next(root).unwrap(), Some(middle));
    assert_eq!(arena.route_previous(root).unwrap(), None);
    assert_eq!(arena.route_next(end).unwrap(), None);
    assert_eq!(arena.route_previous(end).unwrap(), Some(middle));
    assert_eq!(arena.route_node(root).unwrap().distance_squared, 25);
    assert_eq!(arena.route_node(middle).unwrap().distance_squared, 185);
    assert_eq!(arena.route_node(end).unwrap().distance_squared, 0);
    assert_eq!(arena.find_route_node(11), Some(root));
    assert_eq!(arena.find_route_node(13), Some(end));
    assert_eq!(arena.find_route_node(10), None);
}

#[test]
fn endpoint_wrapping_and_word_wrapped_squared_segments_match_native_units() {
    let mut arena = ObjectArena::new();
    let first = node(&mut arena, 0x8000, 0x8000, 1, [0, 0, 0]);
    let second = node(&mut arena, 0x8001, 0x8000, 0, [0x8000, 0x8000, 0x8000]);
    let singleton = node(&mut arena, 0x8002, 0x8002, 1, [u16::MAX, 0, 0]);
    let short = node(&mut arena, 0x8003, 0x8002, 0, [0, 0, 0]);
    arena.finalize_routes().unwrap();
    assert_eq!(arena.route_next(second).unwrap(), Some(first));
    assert_eq!(arena.route_previous(first).unwrap(), Some(second));
    assert_eq!(
        arena.route_node(first).unwrap().distance_squared,
        0xc000_0000
    );
    assert_eq!(
        arena.route_node(second).unwrap().distance_squared,
        0xc000_0000
    );
    assert_eq!(arena.route_node(singleton).unwrap().distance_squared, 1);
    assert_eq!(arena.route_node(short).unwrap().distance_squared, 1);
    assert_eq!(arena.find_route_node(0xffff_8001), Some(first));
    assert_eq!(arena.find_route_node(0x8001), None);

    let mut one = ObjectArena::new();
    let closed = node(&mut one, 7, 7, 1, [2, 3, 4]);
    one.finalize_routes().unwrap();
    assert_eq!(one.route_next(closed).unwrap(), Some(closed));
    assert_eq!(one.route_previous(closed).unwrap(), Some(closed));
    assert_eq!(one.route_node(closed).unwrap().distance_squared, 1);
}

#[test]
fn duplicate_identity_uses_first_pending_match_and_branches_leave_only_first_append() {
    let mut arena = ObjectArena::new();
    let root = node(&mut arena, 10, 10, 2, [0; 3]);
    let duplicate_root = node(&mut arena, 10, 10, 1, [0; 3]);
    let branch_a = node(&mut arena, 11, 10, 0, [1; 3]);
    let branch_b = node(&mut arena, 12, 10, 0, [2; 3]);
    let discarded_child = node(&mut arena, 13, 12, 0, [3; 3]);
    let child_a = node(&mut arena, 14, 11, 0, [4; 3]);
    arena.finalize_routes().unwrap();
    assert_eq!(
        groups(&arena),
        vec![vec![root, branch_a, child_a], vec![duplicate_root]]
    );
    for object in [branch_b, discarded_child] {
        assert_eq!(
            arena.object(object),
            Err(ObjectError::UnknownObject(object))
        );
    }
    assert_eq!(
        arena.route_node(branch_a).unwrap().group,
        arena.route_node(root).unwrap().group
    );
    assert_eq!(
        arena.route_node(duplicate_root).unwrap().distance_squared,
        1
    );
    assert_eq!(arena.len(), 4);
    // Deleted route nodes are removed from every common list as well.
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag1Primary,
        ObjectList::Parking,
    ] {
        assert_eq!(arena.list(list).len(), 4);
    }
}

#[test]
fn deletion_during_resolution_changes_the_later_first_match() {
    let mut arena = ObjectArena::new();
    let invalid_identity = node(&mut arena, u16::MAX, u16::MAX, 0, [0; 3]);
    let broken_duplicate = node(&mut arena, 11, 99, 0, [0; 3]);
    let root = node(&mut arena, 10, 10, 0, [0; 3]);
    let good_duplicate = node(&mut arena, 11, 10, 0, [0; 3]);
    let follower = node(&mut arena, 12, 11, 0, [0; 3]);
    arena.finalize_routes().unwrap();
    assert_eq!(groups(&arena), vec![vec![root, good_duplicate, follower]]);
    for dead in [invalid_identity, broken_duplicate] {
        assert!(arena.object(dead).is_err());
    }

    let mut reordered = ObjectArena::new();
    let follower = node(&mut reordered, 12, 11, 0, [0; 3]);
    let bad = node(&mut reordered, 11, 99, 0, [0; 3]);
    let root = node(&mut reordered, 10, 10, 0, [0; 3]);
    let good = node(&mut reordered, 11, 10, 0, [0; 3]);
    reordered.finalize_routes().unwrap();
    assert!(reordered.object(follower).is_err());
    assert!(reordered.object(bad).is_err());
    assert_eq!(groups(&reordered), vec![vec![root, good]]);
}

#[test]
fn unrooted_cycle_is_an_explicit_error_with_prior_native_pass_effects_preserved() {
    let mut arena = ObjectArena::new();
    let invalid = node(&mut arena, u16::MAX, 2, 0, [0; 3]);
    let root = node(&mut arena, 10, 10, 0, [0; 3]);
    let resolved_follower = node(&mut arena, 14, 10, 0, [0; 3]);
    let a = node(&mut arena, 11, 12, 0, [0; 3]);
    let b = node(&mut arena, 12, 11, 0, [0; 3]);
    assert_eq!(arena.finalize_routes(), Err(ObjectError::RouteCycle(a)));
    assert!(arena.object(invalid).is_err());
    assert_eq!(arena.route_pending(), [root, resolved_follower, a, b]);
    assert_eq!(arena.route_groups().len(), 1);
    let group = &arena.route_groups()[0];
    assert_eq!(group.flags, None);
    assert!(group.members.is_empty());
    assert_eq!(arena.route_node(root).unwrap().group, Some(group.id));
    assert_eq!(
        arena.route_node(resolved_follower).unwrap().group,
        Some(group.id)
    );
    assert_eq!(arena.route_node(a).unwrap().group, None);
    assert_eq!(arena.route_node(b).unwrap().group, None);
    // Native recursion never returns here. The portable failure retains this
    // partial frontier for inspection; callers discard the whole arena.
    drop(arena);
}

#[test]
fn finalization_does_not_search_existing_groups_for_new_predecessors() {
    let mut arena = ObjectArena::new();
    let root = node(&mut arena, 10, 10, 0, [0; 3]);
    arena.finalize_routes().unwrap();
    let later = node(&mut arena, 11, 10, 0, [0; 3]);
    arena.finalize_routes().unwrap();
    assert!(arena.object(later).is_err());
    assert_eq!(groups(&arena), vec![vec![root]]);
}

#[test]
fn deletion_preserves_group_policy_distances_and_expires_only_the_empty_group() {
    let mut arena = ObjectArena::new();
    let first = node(&mut arena, 10, 10, 1, [0; 3]);
    let middle = node(&mut arena, 11, 10, 0, [1; 3]);
    let last = node(&mut arena, 12, 11, 0, [3; 3]);
    arena.finalize_routes().unwrap();
    let group = arena.route_node(first).unwrap().group.unwrap();
    assert_eq!(arena.route_node(first).unwrap().distance_squared, 3);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: middle,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.route_next(first).unwrap(), Some(last));
    assert_eq!(arena.route_node(first).unwrap().distance_squared, 3); // no recalculation on delete
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.route_group(group).unwrap().flags, Some(1));
    assert_eq!(arena.route_next(last).unwrap(), Some(last));
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: last,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(
        arena.route_group(group),
        Err(ObjectError::UnknownRouteGroup(group))
    );
    let new = node(&mut arena, 10, 10, 0, [0; 3]);
    arena.finalize_routes().unwrap();
    let new_group = arena.route_node(new).unwrap().group.unwrap();
    assert_ne!(group, new_group);
    let mut other = ObjectArena::new();
    node(&mut other, 10, 10, 0, [0; 3]);
    other.finalize_routes().unwrap();
    assert_eq!(
        other.route_group(new_group),
        Err(ObjectError::UnknownRouteGroup(new_group))
    );
}

#[test]
fn unknown_position_is_never_substituted_with_zero_and_open_tail_distance_is_retained() {
    let mut arena = ObjectArena::new();
    let first = node(&mut arena, 10, 10, 0, [0; 3]);
    let last = node(&mut arena, 11, 10, 0, [0; 3]);
    arena.owned_mut(first).unwrap().state.base.position_words = None;
    arena.route_mut(last).unwrap().distance_squared = 0x1234_5678;
    assert_eq!(
        arena.finalize_routes(),
        Err(ObjectError::RoutePositionUnknown(first))
    );
    assert_eq!(groups(&arena), vec![vec![first, last]]);
    arena.owned_mut(first).unwrap().state.base.position_words = Some([0; 3]);
    arena.calculate_route_segments().unwrap();
    assert_eq!(arena.route_node(first).unwrap().distance_squared, 1);
    assert_eq!(
        arena.route_node(last).unwrap().distance_squared,
        0x1234_5678
    );
}
