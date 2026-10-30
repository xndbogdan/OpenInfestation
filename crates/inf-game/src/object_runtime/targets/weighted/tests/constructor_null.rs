use super::*;

fn request(referent: WeightedTargetReferent, score: u32) -> WeightedTargetRequest<'static> {
    WeightedTargetRequest {
        referent,
        score,
        constructor_null: WeightedTargetConstructorNull::Supplied(None),
    }
}

#[test]
fn process_null_is_independent_of_registry_and_preserved_by_accepted_replacement() {
    let mut arena = arena();
    let object = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(object).unwrap().state.base.position_words = Some([4, 5, 6]);
    let process_null = ActorHandle::from_raw(0xabcd_9876);
    for referent in [
        WeightedTargetReferent::Point(Some([1, 2, 3])),
        WeightedTargetReferent::Object(Some(object)),
    ] {
        let mut pair = NativeWeightedTarget::new(None, 0);
        let start = arena.actor_service_heap().attempts().len();
        assert_eq!(
            arena.publish_weighted_target(
                &mut pair,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::Supplied(Some(process_null)),
                    ..request(referent, 7)
                }
            ),
            Ok(true)
        );
        let reference = pair.target.unwrap();
        assert_eq!(
            arena.native_target(reference).unwrap().handle_04(),
            process_null
        );
        assert_eq!(arena.actor_handles().unwrap().null_handle().raw(), 0x1234);
        assert_eq!(
            arena.publish_weighted_target(&mut pair, request(referent, 8)),
            Ok(false)
        );
        assert_eq!(pair.target, Some(reference));
        assert_eq!(
            arena.native_target(reference).unwrap().handle_04(),
            process_null
        );
        assert_eq!(allocation_classes(&arena, start), [referent.class()]);
    }
}

#[test]
fn null_source_is_lazy_for_invalid_inputs_allocator_null_and_actor_follow() {
    let mut arena = arena();
    let actor = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    for (referent, score) in [
        (WeightedTargetReferent::Actor(None), 7),
        (WeightedTargetReferent::Object(None), 7),
        (WeightedTargetReferent::Point(None), 7),
        (WeightedTargetReferent::Point(Some([0xffff; 3])), 0),
    ] {
        let mut pair = NativeWeightedTarget::new(None, 99);
        let start = arena.actor_service_heap().attempts().len();
        assert_eq!(
            arena.publish_weighted_target(&mut pair, request(referent, score)),
            Ok(false)
        );
        assert_eq!(pair, NativeWeightedTarget::new(None, 99));
        assert!(allocation_classes(&arena, start).is_empty());
        let reference =
            arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
                handle_04: ActorHandle::from_raw(0x1234),
                focus_08: None,
                word_10: 8,
            }));
        pair.target = Some(reference);
        let start = arena.actor_service_heap().attempts().len();
        assert_eq!(
            arena.publish_weighted_target(&mut pair, request(referent, score)),
            Ok(false)
        );
        assert_eq!(pair, NativeWeightedTarget::new(None, 0));
        assert_eq!(
            arena.actor_service_heap().state(reference),
            Ok(ActorServiceResourceState::Released)
        );
        assert!(allocation_classes(&arena, start).is_empty());
    }
    for referent in [
        WeightedTargetReferent::Point(Some([1, 2, 3])),
        WeightedTargetReferent::Object(Some(actor)),
    ] {
        arena
            .actor_service_heap_mut()
            .script([FixtureActorServiceAllocation::Null]);
        let mut pair = NativeWeightedTarget::new(None, 99);
        assert_eq!(
            arena.publish_weighted_target(&mut pair, request(referent, 7)),
            Ok(true)
        );
        assert_eq!(pair, NativeWeightedTarget::new(None, 7));
    }
    let mut pair = NativeWeightedTarget::new(None, 0);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            request(WeightedTargetReferent::Actor(Some(actor)), 7)
        ),
        Ok(true)
    );
    assert_eq!(
        arena.native_target(pair.target.unwrap()).unwrap().class(),
        NativeTargetClass::ActorFollow
    );
}

#[test]
fn missing_process_null_keeps_score_and_crossed_release_before_position_read() {
    let mut arena = arena();
    let actor = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let follow = NativeTarget::ActorFollow(
        arena
            .follow_target(FollowTargetRequest::resolve_actor(
                arena.actor(actor).unwrap().handle,
            ))
            .unwrap(),
    );
    let object = arena.allocate(ObjectClassKind::RouteNode);
    for referent in [
        WeightedTargetReferent::Point(Some([4, 5, 6])),
        WeightedTargetReferent::Object(Some(object)),
    ] {
        for old in [None, Some(follow)] {
            let old_ref = old.map(|data| arena.import_fixture_target(data));
            let mut pair = NativeWeightedTarget::new(old_ref, 99);
            let start = arena.actor_service_heap().attempts().len();
            assert_eq!(
                arena.publish_weighted_target(&mut pair, request(referent, 7)),
                Err(NativeTargetError::MissingProcessNull)
            );
            assert_eq!(pair.target, old_ref);
            assert_eq!(pair.score, 7);
            if let Some(reference) = old_ref {
                assert_eq!(
                    arena.actor_service_heap().state(reference),
                    Ok(ActorServiceResourceState::Released)
                );
            }
            assert_eq!(allocation_classes(&arena, start), [referent.class()]);
        }
    }
    // Object XYZ is also unavailable: process-null admission wins first.
    let mut pair = NativeWeightedTarget::new(None, 0);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Supplied(Some(
                    ActorHandle::from_raw(9)
                )),
                ..request(WeightedTargetReferent::Object(Some(object)), 8)
            }
        ),
        Err(NativeTargetError::Object(
            ObjectError::TargetObjectPositionUnknown(object)
        ))
    );
    assert_eq!(pair, NativeWeightedTarget::new(None, 8));
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            request(WeightedTargetReferent::Point(Some([1, 2, 3])), 9)
        ),
        Err(NativeTargetError::Heap(
            ActorServiceHeapError::FixtureFailure(71)
        ))
    );
}

#[test]
fn process_constructor_needs_no_registry_and_poisoned_kind8_reuses_old_handle() {
    let mut arena = arena();
    let actor = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let mut follow = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(actor).unwrap().handle,
        ))
        .unwrap();
    follow.word_10 = 8;
    let old_handle = follow.handle_04;
    let object = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(object).unwrap().state.base.position_words = Some([4, 5, 6]);
    arena.actor_handles = None;
    let reference = arena.import_fixture_target(NativeTarget::ActorFollow(follow));
    let mut pair = NativeWeightedTarget::new(Some(reference), 0);
    let start = arena.actor_service_heap().attempts().len();
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            request(WeightedTargetReferent::Point(Some([7, 8, 9])), 7)
        ),
        Ok(false)
    );
    assert_eq!(pair.target, Some(reference));
    assert_eq!(
        arena.native_target(reference).unwrap().handle_04(),
        old_handle
    );
    assert_eq!(
        arena.native_target(reference).unwrap().raw_focus_08_words(),
        Some([7, 8, 9])
    );
    assert!(allocation_classes(&arena, start).is_empty());
    for referent in [
        WeightedTargetReferent::Point(Some([1, 2, 3])),
        WeightedTargetReferent::Object(Some(object)),
    ] {
        let mut pair = NativeWeightedTarget::new(None, 0);
        assert_eq!(
            arena.publish_weighted_target(
                &mut pair,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::Supplied(Some(
                        ActorHandle::from_raw(9)
                    )),
                    ..request(referent, 8)
                }
            ),
            Ok(true)
        );
        assert_eq!(
            arena
                .native_target(pair.target.unwrap())
                .unwrap()
                .handle_04()
                .raw(),
            9
        );
    }
}
