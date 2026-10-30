use super::*;
use crate::object_runtime::targets::{
    ActorFollowTarget, ActorServiceHeapError, ActorServiceResourceKind, ActorServiceResourceState,
    FixtureActorServiceAllocation, NativeTargetError, NativeWeightedTarget, WeightedTargetReferent,
    WeightedTargetRequest,
};
use crate::object_runtime::WeightedTargetConstructorNull;
use crate::object_runtime::{ActorRegistryConfig, ObjectClassKind};

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap()
}

fn allocation_bytes(arena: &ObjectArena, from: usize) -> Vec<u32> {
    arena.actor_service_heap().attempts()[from..]
        .iter()
        .map(|kind| match kind {
            ActorServiceResourceKind::WeaponFireNode => {
                panic!("unexpected firing allocation in target fixture")
            }
            ActorServiceResourceKind::Target(class) => class.allocation_bytes(),
            ActorServiceResourceKind::FamilyRadiusScoreInput => {
                panic!("unexpected radius input allocation")
            }
        })
        .collect()
}

fn fixed(point: Option<[u16; 3]>) -> NativeTarget {
    NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0x8765_4321),
        focus_08: point,
        word_10: 8,
    })
}

#[test]
fn fixed_clone_resets_handle_copies_unknown_point_and_preserves_allocation_boundaries() {
    let mut arena = arena();
    let mut clone_allocations = Vec::new();
    for point in [Some([65535, 32768, 7]), None] {
        let original = fixed(point);
        let reference = arena.import_fixture_target(original);
        let start = arena.actor_service_heap().attempts().len();
        let clone_ref = arena
            .clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry)
            .unwrap()
            .unwrap();
        assert_ne!(clone_ref, reference);
        let clone = arena.native_target(clone_ref).unwrap();
        assert_eq!(clone.handle_04().raw(), 0x1234_5678);
        assert_eq!(clone.point_words(), point);
        assert_eq!(clone.kind_word(), 8);
        assert_eq!(arena.native_target(reference).unwrap(), original);
        assert_eq!(original.handle_04().raw(), 0x8765_4321);
        clone_allocations.extend(allocation_bytes(&arena, start));
    }
    assert_eq!(clone_allocations, vec![0x14, 0x14]);
    let follow = NativeTarget::ActorFollow(ActorFollowTarget {
        handle_04: ActorHandle::from_raw(0x8765_4321),
        focus_08: Some([1, 2, 3]),
        word_10: 2,
        focus_14: Some([4, 5, 6]),
        velocity_1a: Some([7, 8, 9]),
    });
    let reference = arena.import_fixture_target(follow);
    let clone_ref = arena
        .clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry)
        .unwrap()
        .unwrap();
    assert_eq!(
        arena.native_target(clone_ref).unwrap().handle_04(),
        follow.handle_04()
    );
    arena.actor_handles = None;
    let unavailable_object = arena.allocate(ObjectClassKind::RouteNode);
    assert_eq!(
        arena.object_position_target(unavailable_object),
        Err(ObjectError::MissingActorRegistry)
    );
    let reference = arena.import_fixture_target(fixed(None));
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    assert_eq!(
        arena.clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry),
        Ok(None)
    );
    assert_eq!(
        arena.clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry),
        Err(NativeTargetError::Object(ObjectError::MissingActorRegistry))
    );
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    assert_eq!(
        arena.clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry),
        Err(NativeTargetError::Heap(
            ActorServiceHeapError::FixtureFailure(71)
        ))
    );
}

#[test]
fn fixed_query_callbacks_require_process_sources_and_skip_unrelated_actor_reads() {
    let mut arena = ObjectArena::new();
    let invalid = arena.allocate(ObjectClassKind::RouteNode);
    let mut target = fixed(None);
    let before = target;
    assert_eq!(arena.refresh_native_target(&mut target, 0xffff_ffff), Ok(4));
    assert_eq!(arena.resolve_target_actor(&target), Ok(None));
    assert_eq!(target.object_referent(), None);
    assert_eq!(target.point_words(), None);
    assert_eq!(
        target.velocity_words(None),
        Err(NativeTargetQueryError::MissingProcessVelocity)
    );
    assert_eq!(
        target.velocity_words(Some([0xffff, 0x8000, 17])),
        Ok(Some([0xffff, 0x8000, 17]))
    );
    assert_eq!(
        arena.native_target_basis(&target, None),
        Err(NativeTargetQueryError::MissingProcessBasis)
    );
    let mut basis = NativeBasis::INITIAL;
    basis.words[0] = 7;
    assert_eq!(arena.native_target_basis(&target, Some(basis)), Ok(basis));
    for referent in [
        WeightedTargetReferent::Actor(None),
        WeightedTargetReferent::Actor(Some(invalid)),
        WeightedTargetReferent::Object(Some(invalid)),
    ] {
        assert_eq!(
            arena.try_replace_weighted_target(&mut target, referent),
            Ok(false)
        );
        assert_eq!(target, before);
    }
}

#[test]
fn point_publisher_kind_gate_and_released_custody_survive_failures() {
    let mut arena = arena();
    let point = [65535, 32768, 7];
    let request = WeightedTargetRequest {
        constructor_null: WeightedTargetConstructorNull::ActorRegistry,
        referent: WeightedTargetReferent::Point(Some(point)),
        score: 0x8000_0000,
    };
    let mut malformed = match fixed(Some([1, 2, 3])) {
        NativeTarget::FixedPosition(target) => target,
        _ => unreachable!(),
    };
    malformed.word_10 = 16;
    let old = NativeTarget::FixedPosition(malformed);
    let reference = arena.import_fixture_target(old);
    let mut pair = NativeWeightedTarget::new(Some(reference), 10);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    assert_eq!(
        arena.publish_weighted_target(&mut pair, request),
        Err(NativeTargetError::Heap(
            ActorServiceHeapError::FixtureFailure(71)
        ))
    );
    assert_eq!(pair.target, Some(reference));
    assert_eq!(
        arena.actor_service_heap().state(reference),
        Ok(ActorServiceResourceState::Released)
    );
    assert_eq!(pair.score, 0x8000_0000);
    assert!(pair.native_pointer_present());
    let changed = WeightedTargetRequest {
        score: 99,
        ..request
    };
    assert_eq!(
        arena.publish_weighted_target(&mut pair, changed),
        Err(NativeTargetError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(reference)
        )))
    );
    assert_eq!(pair.score, 99);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Point(None),
                score: 0,
            }
        ),
        Err(NativeTargetError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(reference)
        )))
    );
    assert_eq!(pair.score, 99);
    let fresh_old = arena.import_fixture_target(old);
    let mut pair = NativeWeightedTarget::new(Some(fresh_old), 10);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    assert_eq!(arena.publish_weighted_target(&mut pair, request), Ok(false));
    assert_eq!(pair, NativeWeightedTarget::new(None, 0x8000_0000));
    assert_eq!(
        arena.actor_service_heap().state(fresh_old),
        Ok(ActorServiceResourceState::Released)
    );
    let mut pair = NativeWeightedTarget::new(None, 10);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    assert_eq!(arena.publish_weighted_target(&mut pair, request), Ok(true));
    assert_eq!(pair, NativeWeightedTarget::new(None, 0x8000_0000));
    let mut follow = ActorFollowTarget {
        handle_04: ActorHandle::from_raw(0x8765_4321),
        focus_08: Some([1, 2, 3]),
        word_10: 8,
        focus_14: Some([4, 5, 6]),
        velocity_1a: Some([7, 8, 9]),
    };
    let reference = arena.import_fixture_target(NativeTarget::ActorFollow(follow));
    let mut pair = NativeWeightedTarget::new(Some(reference), 10);
    let start = arena.actor_service_heap().attempts().len();
    assert_eq!(arena.publish_weighted_target(&mut pair, request), Ok(false));
    follow.focus_08 = Some(point);
    assert_eq!(
        pair,
        NativeWeightedTarget::new(Some(reference), 0x8000_0000)
    );
    assert_eq!(
        arena.native_target(reference).unwrap(),
        NativeTarget::ActorFollow(follow)
    );
    assert!(allocation_bytes(&arena, start).is_empty());
}
