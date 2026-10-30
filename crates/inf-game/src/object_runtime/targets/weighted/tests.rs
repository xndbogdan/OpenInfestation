use super::*;
use crate::object_runtime::targets::tests::allocation;
use crate::object_runtime::targets::{
    ActorServiceHeapError, ActorServiceResourceKind, ActorServiceResourceState,
    FixtureActorServiceAllocation,
};
use crate::object_runtime::{ActorRegistryConfig, ObjectClassKind};

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

fn allocation_classes(arena: &ObjectArena, from: usize) -> Vec<NativeTargetClass> {
    arena.actor_service_heap().attempts()[from..]
        .iter()
        .map(|kind| match kind {
            ActorServiceResourceKind::WeaponFireNode => {
                panic!("unexpected firing allocation in target fixture")
            }
            ActorServiceResourceKind::Target(class) => *class,
            ActorServiceResourceKind::FamilyRadiusScoreInput => {
                panic!("unexpected radius input allocation")
            }
        })
        .collect()
}

#[test]
fn native_actor_replacement_preserves_other_caches_and_publishes_handle_before_failure() {
    let mut arena = arena();
    let first = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let second = arena
        .allocate_actor(allocation(Some([4, 0xfffe, 6])))
        .unwrap();
    arena.owned_mut(second).unwrap().state.base.word_28 = 0xfffc;
    let first_handle = arena.actor(first).unwrap().handle;
    let second_handle = arena.actor(second).unwrap().handle;
    let mut target = arena
        .follow_target(FollowTargetRequest::resolve_actor(first_handle))
        .unwrap();
    target.velocity_1a = Some([7, 8, 9]);
    assert!(arena
        .replace_follow_target(&mut target, Some(second))
        .unwrap());
    assert_eq!(target.handle_04, second_handle);
    assert_eq!(target.focus_14, Some([4, 0x3ffd, 6]));
    assert_eq!(target.focus_08, Some([1, 2, 3]));
    assert_eq!(target.velocity_1a, Some([7, 8, 9]));
    arena.owned_mut(first).unwrap().state.base.position_words = None;
    assert_eq!(
        arena.replace_follow_target(&mut target, Some(first)),
        Err(ObjectError::ActorPositionUnknown(first))
    );
    assert_eq!(target.handle_04, first_handle);
    assert_eq!(target.focus_14, Some([4, 0x3ffd, 6]));
    arena.actor_handles.as_mut().unwrap().release(first_handle);
    assert!(arena
        .replace_follow_target(&mut target, Some(first))
        .unwrap());
    assert_eq!(target.focus_14, Some([4, 0x3ffd, 6]));
    assert!(arena.replace_follow_target(&mut target, None).unwrap());
    assert_eq!(target.handle_04.raw(), 0x1234);
}

#[test]
fn score_order_allocation_null_and_crossed_callbacks_preserve_native_results() {
    let mut arena = arena();
    let actor = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let object = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(object).unwrap().state.base.position_words = Some([4, 5, 6]);
    let mut pair = NativeWeightedTarget::new(None, 0x8765_4321);
    arena.actor_service_heap_mut().script([
        FixtureActorServiceAllocation::Null,
        FixtureActorServiceAllocation::Allocate,
        FixtureActorServiceAllocation::Null,
    ]);
    assert!(!arena
        .publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(None),
                score: 5,
            }
        )
        .unwrap());
    assert_eq!(pair.score, 0x8765_4321);
    assert!(arena.actor_service_heap().attempts().is_empty());
    assert!(arena
        .publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(Some(actor)),
                score: 0x8000_0000,
            }
        )
        .unwrap());
    assert_eq!(pair, NativeWeightedTarget::new(None, 0x8000_0000));
    assert!(arena
        .publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(Some(actor)),
                score: u32::MAX,
            }
        )
        .unwrap());
    let old = pair.target.unwrap();
    let before_actor = arena.object(actor).unwrap().clone();
    let before_object = arena.object(object).unwrap().clone();
    assert!(!arena
        .publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Object(Some(object)),
                score: 2,
            }
        )
        .unwrap());
    assert_eq!(pair, NativeWeightedTarget::new(None, 2));
    assert_eq!(
        arena.actor_service_heap().state(old),
        Ok(ActorServiceResourceState::Released)
    );
    assert_eq!(arena.object(actor).unwrap(), &before_actor);
    assert_eq!(arena.object(object).unwrap(), &before_object);
    assert_eq!(
        allocation_classes(&arena, 0),
        [
            NativeTargetClass::ActorFollow,
            NativeTargetClass::ActorFollow,
            NativeTargetClass::ObjectPosition
        ]
    );
}

#[test]
fn unavailable_effect_and_changed_identity_read_preserve_early_publication() {
    let mut arena = arena();
    let first = arena.allocate(ObjectClassKind::RouteNode);
    let other = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(first).unwrap().state.base.position_words = Some([1, 2, 3]);
    let seed = NativeTarget::ObjectPosition(arena.object_position_target(first).unwrap());
    let reference = arena.import_fixture_target(seed);
    let start = arena.actor_service_heap().attempts().len();
    let mut pair = NativeWeightedTarget::new(Some(reference), 1);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Object(Some(other)),
                score: 2,
            }
        ),
        Err(NativeTargetError::Object(
            ObjectError::TargetObjectPositionUnknown(other)
        ))
    );
    assert_eq!(pair.score, 2);
    assert_eq!(pair.target, Some(reference));
    let NativeTarget::ObjectPosition(target) = arena.native_target(reference).unwrap() else {
        panic!()
    };
    assert_eq!(target.object_14, other);
    assert_eq!(target.focus_08, [1, 2, 3]);
    assert!(allocation_classes(&arena, start).is_empty());
    let actor = arena.allocate_actor(allocation(Some([7, 8, 9]))).unwrap();
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(Some(actor)),
                score: 3,
            }
        ),
        Err(NativeTargetError::Heap(
            ActorServiceHeapError::FixtureFailure(71)
        ))
    );
    assert_eq!(pair, NativeWeightedTarget::new(Some(reference), 3));
    assert_eq!(
        arena.actor_service_heap().state(reference),
        Ok(ActorServiceResourceState::Released)
    );
    assert!(pair.native_pointer_present());
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(Some(actor)),
                score: 4,
            }
        ),
        Err(NativeTargetError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(reference)
        )))
    );
    assert_eq!(pair.score, 4);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                referent: WeightedTargetReferent::Actor(None),
                score: 5,
            }
        ),
        Err(NativeTargetError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(reference)
        )))
    );
    assert_eq!(pair.score, 4);
}

#[test]
fn clone_and_future_identity_queries_keep_actor_and_object_domains_distinct() {
    let mut arena = arena();
    let actor = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    let target = NativeTarget::ActorFollow(
        arena
            .follow_target(FollowTargetRequest::resolve_actor(handle))
            .unwrap(),
    );
    let reference = arena.import_fixture_target(target);
    let clone_ref = arena
        .clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry)
        .unwrap()
        .unwrap();
    assert_ne!(clone_ref, reference);
    let clone = arena.native_target(clone_ref).unwrap();
    let NativeTarget::ActorFollow(follow) = clone else {
        panic!()
    };
    assert_eq!(follow.focus_08, None);
    assert_eq!(follow.focus_14, Some([1, 2, 3]));
    assert_eq!(arena.resolve_target_actor(&clone).unwrap(), Some(actor));
    let route = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(route).unwrap().state.base.position_words = Some([4, 5, 6]);
    let target = NativeTarget::ObjectPosition(arena.object_position_target(route).unwrap());
    let reference = arena.import_fixture_target(target);
    arena.owned_mut(route).unwrap().state.base.position_words = Some([7, 8, 9]);
    let clone_ref = arena
        .clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry)
        .unwrap()
        .unwrap();
    assert_ne!(clone_ref, reference);
    let clone = arena.native_target(clone_ref).unwrap();
    assert_eq!(clone.object_referent(), Some(route));
    assert_eq!(clone.handle_04().raw(), 0x1234);
    assert_eq!(arena.resolve_target_actor(&clone).unwrap(), None);
    let NativeTarget::ObjectPosition(object) = clone else {
        panic!()
    };
    assert_eq!(object.focus_08, [7, 8, 9]);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    assert_eq!(
        arena
            .clone_native_target(clone_ref, WeightedTargetConstructorNull::ActorRegistry)
            .unwrap(),
        None
    );
}

mod constructor_null;
mod native_receipts;
