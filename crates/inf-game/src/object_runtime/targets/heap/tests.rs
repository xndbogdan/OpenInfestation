use super::*;
use crate::object_runtime::{
    ActorHandle, FixedPositionTarget, NativeTargetError, NativeWeightedTarget, ObjectArena,
    TargetNullHandleSource, WeightedTargetConstructorNull, WeightedTargetReferent,
    WeightedTargetRequest,
};
use std::cell::Cell;
fn fixed(value: u16) -> NativeTarget {
    NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(7),
        word_10: 8,
        focus_08: Some([value, 2, 3]),
    })
}
fn request(class: NativeTargetClass) -> ActorServiceAllocationRequest {
    ActorServiceAllocationRequest {
        kind: ActorServiceResourceKind::Target(class),
    }
}
struct Source {
    value: Cell<Option<ActorHandle>>,
    reads: Cell<u32>,
}
impl TargetNullHandleSource for Source {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.reads.set(self.reads.get() + 1);
        self.value.get()
    }
}
#[test]
fn pending_backing_remains_until_explicit_release_and_identity_is_never_reused() {
    let mut heap = ActorServiceHeap::new(
        17,
        ActorServiceAllocationPolicy::PortableFallibleActorHeapApproximation,
    );
    let first = heap
        .allocate(request(NativeTargetClass::FixedPosition))
        .unwrap()
        .unwrap();
    assert_eq!(heap.state(first), Ok(ActorServiceResourceState::Pending));
    assert_eq!(heap.retained_bytes(first), Ok(0x14));
    assert_eq!(
        heap.target(first),
        Err(ActorServiceHeapError::Pending(first))
    );
    heap.construct_target(first, fixed(1)).unwrap();
    let alias = first;
    heap.release(first).unwrap();
    assert_eq!(heap.state(alias), Ok(ActorServiceResourceState::Released));
    assert_eq!(heap.retained_bytes(alias), Ok(0));
    assert_eq!(
        heap.target(alias),
        Err(ActorServiceHeapError::Released(alias))
    );
    assert_eq!(
        heap.release(alias),
        Err(ActorServiceHeapError::Released(alias))
    );
    let next = heap
        .allocate(request(NativeTargetClass::FixedPosition))
        .unwrap()
        .unwrap();
    assert_ne!(next, first);
    assert_eq!(heap.retained_bytes(next), Ok(0x14));
}
#[test]
fn foreign_kind_and_pending_faults_do_not_mutate_existing_reservations() {
    let mut a = ActorServiceHeap::new(
        1,
        ActorServiceAllocationPolicy::PortableFallibleActorHeapApproximation,
    );
    let mut b = ActorServiceHeap::new(
        2,
        ActorServiceAllocationPolicy::PortableFallibleActorHeapApproximation,
    );
    let target = a
        .allocate(request(NativeTargetClass::ActorFollow))
        .unwrap()
        .unwrap();
    assert_eq!(
        b.release(target),
        Err(ActorServiceHeapError::Foreign(target))
    );
    assert_eq!(
        a.construct_target(target, fixed(1)),
        Err(ActorServiceHeapError::Kind(target))
    );
    assert_eq!(a.retained_bytes(target), Ok(0x20));
    assert_eq!(a.state(target), Ok(ActorServiceResourceState::Pending));
    let input = a
        .allocate(ActorServiceAllocationRequest {
            kind: ActorServiceResourceKind::FamilyRadiusScoreInput,
        })
        .unwrap()
        .unwrap();
    assert_eq!(a.target(input), Err(ActorServiceHeapError::Pending(input)));
    let actor = ObjectArena::new().allocate(crate::object_runtime::ObjectClassKind::Base);
    a.construct_radius_score_input(input, actor).unwrap();
    assert_eq!(a.target(input), Err(ActorServiceHeapError::Kind(input)));
    assert_eq!(a.radius_score_actor(input), Ok(actor));
    assert_eq!(a.retained_bytes(input), Ok(8));
}
#[test]
fn scripted_null_and_error_are_distinct_and_neither_creates_an_entry() {
    let mut heap = ActorServiceHeap::new(
        4,
        ActorServiceAllocationPolicy::PortableFallibleActorHeapApproximation,
    );
    heap.script([
        FixtureActorServiceAllocation::Null,
        FixtureActorServiceAllocation::Error(71),
        FixtureActorServiceAllocation::Allocate,
    ]);
    assert_eq!(
        heap.allocate(request(NativeTargetClass::FixedPosition)),
        Ok(None)
    );
    assert!(heap.entries.is_empty());
    assert_eq!(
        heap.allocate(request(NativeTargetClass::FixedPosition)),
        Err(ActorServiceHeapError::FixtureFailure(71))
    );
    assert!(heap.entries.is_empty());
    let reference = heap
        .allocate(request(NativeTargetClass::FixedPosition))
        .unwrap()
        .unwrap();
    assert_eq!(reference.serial, 1);
    assert_eq!(heap.retained_bytes(reference), Ok(0x14));
    assert_eq!(heap.attempts().len(), 3);
}
#[test]
fn process_getter_is_lazy_after_allocation_and_each_new_constructor_reads_current_owner() {
    let mut arena = ObjectArena::new();
    let source = Source {
        value: Cell::new(None),
        reads: Cell::new(0),
    };
    let mut pair = NativeWeightedTarget::new(None, 99);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Point(Some([1, 2, 3])),
                score: 7,
                constructor_null: WeightedTargetConstructorNull::Process(&source),
            }
        ),
        Ok(true)
    );
    assert_eq!(source.reads.get(), 0);
    assert_eq!(pair, NativeWeightedTarget::new(None, 7));
    source.value.set(Some(ActorHandle::from_raw(0x1234)));
    arena
        .publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Point(Some([4, 5, 6])),
                score: 8,
                constructor_null: WeightedTargetConstructorNull::Process(&source),
            },
        )
        .unwrap();
    let first = pair.target.unwrap();
    assert_eq!(source.reads.get(), 1);
    assert_eq!(
        arena.native_target(first).unwrap().handle_04(),
        ActorHandle::from_raw(0x1234)
    );
    source.value.set(Some(ActorHandle::from_raw(0x5678)));
    let second = arena
        .clone_native_target(first, WeightedTargetConstructorNull::Process(&source))
        .unwrap()
        .unwrap();
    assert_ne!(second, first);
    assert_eq!(source.reads.get(), 2);
    assert_eq!(
        arena.native_target(second).unwrap().handle_04(),
        ActorHandle::from_raw(0x5678)
    );
    assert_eq!(
        arena.native_target(first).unwrap().handle_04(),
        ActorHandle::from_raw(0x1234)
    );
}
#[test]
fn missing_process_after_cross_class_release_retains_raw_ref_score_and_pending_backing() {
    let mut arena = ObjectArena::new();
    let old = arena.import_fixture_target(NativeTarget::ActorFollow(
        crate::object_runtime::ActorFollowTarget {
            handle_04: ActorHandle::from_raw(1),
            focus_08: None,
            word_10: 2,
            focus_14: None,
            velocity_1a: None,
        },
    ));
    let source = Source {
        value: Cell::new(None),
        reads: Cell::new(0),
    };
    let mut pair = NativeWeightedTarget::new(Some(old), 3);
    assert_eq!(
        arena.publish_weighted_target(
            &mut pair,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Point(Some([1, 2, 3])),
                score: 91,
                constructor_null: WeightedTargetConstructorNull::Process(&source),
            }
        ),
        Err(NativeTargetError::MissingProcessNull)
    );
    assert_eq!(pair, NativeWeightedTarget::new(Some(old), 91));
    assert_eq!(
        arena.actor_service_heap().state(old),
        Ok(ActorServiceResourceState::Released)
    );
    assert_eq!(
        arena.actor_service_heap().entries.last().unwrap().state,
        ActorServiceResourceState::Pending
    );
    assert_eq!(
        arena
            .actor_service_heap()
            .entries
            .last()
            .unwrap()
            .backing
            .as_ref()
            .unwrap()
            .len(),
        0x14
    );
    assert_eq!(source.reads.get(), 1);
}
#[test]
fn pointer_copy_aliases_while_virtual_clone_allocates_and_release_is_explicit() {
    let mut arena = ObjectArena::new();
    let first = arena.import_fixture_target(fixed(9));
    let copied = first;
    let source = Source {
        value: Cell::new(Some(ActorHandle::from_raw(7))),
        reads: Cell::new(0),
    };
    let cloned = arena
        .clone_native_target(first, WeightedTargetConstructorNull::Process(&source))
        .unwrap()
        .unwrap();
    assert_ne!(cloned, first);
    assert_eq!(arena.native_target(cloned), arena.native_target(first));
    arena.release_actor_service(copied).unwrap();
    assert!(
        matches!(arena.native_target(first), Err(crate::object_runtime::ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(reference))) if reference==first)
    );
    assert_eq!(arena.native_target(cloned), Ok(fixed(9)));
}

mod native_guards;
