use super::*;
use crate::object_runtime::{ActorRegistryConfig, FamilyHelper482ff0, FamilyHelperHistoryRecord};

fn arena() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_family428650_actor(super::super::tests::construction(), &mut 0)
        .unwrap();
    arena.family428650_actor_mut(actor).unwrap().helper_218 =
        Some(Box::new(FamilyHelper482ff0::new(None)));
    arena.actor_handles = None; // Query never resolves handles.
    (arena, actor)
}

fn request(words: [u16; 3]) -> FamilyHelperQueryRequest {
    FamilyHelperQueryRequest {
        handle: ActorHandle::from_raw(7),
        displacement_words: words,
    }
}

fn matched(normal: [u32; 3]) -> FamilyHelperHistoryRecord {
    FamilyHelperHistoryRecord {
        handle: Some(ActorHandle::from_raw(7)),
        normal_bits: normal.map(Some),
        weight: None,
        padding: None,
    }
}

#[test]
fn signed_empty_count_bypasses_spans_and_positive_overcapacity_is_preflight_only() {
    let (mut arena, actor) = arena();
    for count in [0, 0x8000_0000, u32::MAX] {
        arena.family_helper_history_mut(actor).unwrap().word_08 = count;
        let before = arena.family_helper_history(actor).unwrap().clone();
        assert_eq!(
            arena.query_family_helper_history(actor, request([1, 2, 3])),
            Ok(false)
        );
        assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
    }
    arena.family_helper_history_mut(actor).unwrap().word_08 = 5;
    let before = arena.family_helper_history(actor).unwrap().clone();
    assert_eq!(
        arena.query_family_helper_history(actor, request([1, 2, 3])),
        Err(FamilyHelperHistoryError::CountOutsideAllocation { actor, count: 5 })
    );
    assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
}

#[test]
fn unmatched_unknown_normals_and_later_rows_after_first_acceptance_are_lazy() {
    let (mut arena, actor) = arena();
    let helper = arena.family_helper_history_mut(actor).unwrap();
    helper.word_08 = 4;
    helper.records[0] = matched([0; 3]); // Same handle fails dot and continues.
    helper.records[1].handle = Some(ActorHandle::from_raw(8));
    helper.records[2] = matched([1.0f32.to_bits(), 0, 0]);
    helper.records[2].weight = Some(0); // Weight does not gate this query.
    let before = helper.clone();
    assert_eq!(
        arena.query_family_helper_history(actor, request([1, 0, 0])),
        Ok(true)
    );
    assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
    arena.family_helper_history_mut(actor).unwrap().records[2].handle =
        Some(ActorHandle::from_raw(8));
    let before = arena.family_helper_history(actor).unwrap().clone();
    assert_eq!(
        arena.query_family_helper_history(actor, request([1, 0, 0])),
        Err(FamilyHelperHistoryError::Uninitialized {
            actor,
            offset: 0x48
        })
    );
    assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
}

#[test]
fn matched_normals_are_required_in_native_x_z_y_order_without_writes() {
    let (mut arena, actor) = arena();
    let helper = arena.family_helper_history_mut(actor).unwrap();
    helper.word_08 = 1;
    helper.records[0].handle = Some(ActorHandle::from_raw(7));
    for (offset, next_axis) in [(0x10, 0), (0x18, 2), (0x14, 1)] {
        let before = arena.family_helper_history(actor).unwrap().clone();
        assert_eq!(
            arena.query_family_helper_history(actor, request([1, 0, 0])),
            Err(FamilyHelperHistoryError::Uninitialized { actor, offset })
        );
        assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
        arena.family_helper_history_mut(actor).unwrap().records[0].normal_bits[next_axis] =
            Some(if next_axis == 0 { 1.0f32.to_bits() } else { 0 });
    }
    assert_eq!(
        arena.query_family_helper_history(actor, request([1, 0, 0])),
        Ok(true)
    );
}

#[test]
fn unusual_threshold_wrapping_and_masked_conversion_keep_native_numeric_domains() {
    let (mut arena, actor) = arena();
    arena.family_helper_history_mut(actor).unwrap().word_08 = 1;
    for (words, normal, expected) in [
        (
            [1, 1, 1],
            [0x5d80_0000, 1.0f32.to_bits(), 0xdd80_0000],
            true,
        ), // (X+Z)+Y retains1.
        ([0, 8, 0], [0, 0.125f32.to_bits(), 0], true), // No Y² term.
        ([0, 0x8000, 0x8000], [0, (-1.0f32).to_bits(), 0], true), // Logical SHR.
        ([1, 0, 0], [65536.0f32.to_bits(), 0, 0], false), // Wrapping dot²=0.
        (
            [4, 1, 0],
            [1_073_741_824.0f32.to_bits(), 3.0f32.to_bits(), 0],
            true,
        ), // Low DWORD3.
        ([1, 0, 0], [f32::MAX.to_bits(), 0, 0], false),
        ([1, 0, 0], [f32::INFINITY.to_bits(), 0, 0], false),
        ([1, 0, 0], [f32::NAN.to_bits(), 0, 0], false),
    ] {
        arena.family_helper_history_mut(actor).unwrap().records[0] = matched(normal);
        let before = arena.family_helper_history(actor).unwrap().clone();
        assert_eq!(
            arena.query_family_helper_history(actor, request(words)),
            Ok(expected),
            "{words:?}/{normal:?}"
        );
        assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
    }
}
