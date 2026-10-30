//! Explicit typed constructor/heap fixtures; no complete production intro claim.
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
    ActorServiceResourceState, ActorTerrainInputs, DerivedActorConstructionRequest,
    FixedPositionTarget, FixtureActorServiceAllocation, NativeTarget, NativeTargetClass,
    ObjectClassKind, Selector12GrantContext, TargetNullHandleSource,
};
use std::cell::Cell;

const SINE: [u16; 4096] = [0; 4096];
#[derive(Clone, Copy)]
enum FixtureClass {
    Parent,
    Selector10,
    Selector36,
    Selector12,
    Selector35,
}
fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([1, 2, 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    }
}
fn fixture(class: FixtureClass) -> (ObjectArena, ObjectId, u32) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap();
    let mut rng = 0x1234;
    let actor = match class {
        FixtureClass::Parent => arena.allocate_family4236d0_actor(request(), &mut rng),
        FixtureClass::Selector10 => arena.allocate_selector10_actor(request(), &mut rng),
        FixtureClass::Selector36 => arena.allocate_selector36_actor(request(), &mut rng),
        FixtureClass::Selector12 => arena.allocate_selector12_actor(
            request(),
            &mut rng,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                    records: vec![],
                },
                first_player_grants: None,
            },
        ),
        FixtureClass::Selector35 => arena.allocate_selector35_actor(request(), &mut rng),
    }
    .unwrap();
    arena.derived_actor_mut(actor).unwrap().word_14c = Some(0x1122);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0x3344);
    (arena, actor, rng)
}
struct Process {
    reads: Cell<usize>,
    value: Option<ActorHandle>,
}
impl Process {
    fn new(value: Option<ActorHandle>) -> Self {
        Self {
            reads: Cell::new(0),
            value,
        }
    }
}
impl TargetNullHandleSource for Process {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.reads.set(self.reads.get() + 1);
        self.value
    }
}
fn enter() -> FamilyObjectTargetChangeRequest {
    FamilyObjectTargetChangeRequest { entering: true }
}
fn object_source(
    arena: &mut ObjectArena,
    actor: ObjectId,
) -> (ObjectId, crate::object_runtime::NativeHeapRef) {
    let source = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(source).unwrap().state.base.position_words = Some([4, 6, 3]);
    let target = arena
        .object_position_target_resource(source, WeightedTargetConstructorNull::ActorRegistry)
        .unwrap()
        .unwrap();
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[5] = Some(target);
    (source, target)
}
fn prior_current(arena: &mut ObjectArena, actor: ObjectId) -> crate::object_runtime::NativeHeapRef {
    let target = arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0x4567),
        focus_08: Some([8, 9, 10]),
        word_10: 8,
    }));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
    target
}

#[test]
fn false_entry_bypasses_receiver_heap_and_process_sources() {
    let mut arena = ObjectArena::new();
    let base = arena.allocate(ObjectClassKind::Base);
    let process = Process::new(None);
    let before = format!("{arena:?}");
    assert_eq!(
        arena.change_family_object_target(
            base,
            FamilyObjectTargetChangeRequest { entering: false },
            WeightedTargetConstructorNull::Process(&process)
        ),
        Ok(())
    );
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(process.reads.get(), 0);
}

#[test]
fn null_and_non_object_virtual14_keep_only_status_without_clone_or_registry_read() {
    for class in [
        FixtureClass::Parent,
        FixtureClass::Selector10,
        FixtureClass::Selector36,
        FixtureClass::Selector12,
    ] {
        for target_kind in [
            None,
            Some(NativeTargetClass::FixedPosition),
            Some(NativeTargetClass::ActorFollow),
        ] {
            let (mut arena, actor, _construction_rng) = fixture(class);
            let old = prior_current(&mut arena, actor);
            let pointer = match target_kind {
                None => None,
                Some(NativeTargetClass::FixedPosition) => Some(arena.import_fixture_target(
                    NativeTarget::FixedPosition(FixedPositionTarget {
                        handle_04: ActorHandle::from_raw(0x2222),
                        focus_08: None,
                        word_10: 8,
                    }),
                )),
                Some(NativeTargetClass::ActorFollow) => {
                    let data = arena
                        .follow_target(crate::object_runtime::FollowTargetRequest {
                            handle: ActorHandle::from_raw(0x3333),
                            point: crate::object_runtime::FollowTargetPoint::ExplicitPoint(None),
                        })
                        .unwrap();
                    Some(arena.import_fixture_target(NativeTarget::ActorFollow(data)))
                }
                _ => unreachable!(),
            };
            arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[5] = pointer;
            arena.owned_mut(actor).unwrap().state.base.position_words = None;
            arena.actor_handles = None;
            let attempts = arena.actor_service_heap().attempts().len();
            let process = Process::new(None);
            let mut expected = arena.family428650_actor(actor).unwrap().clone();
            expected.word_228 = 0x0053_11f0;
            assert_eq!(
                arena.change_family_object_target(
                    actor,
                    enter(),
                    WeightedTargetConstructorNull::Process(&process)
                ),
                Ok(())
            );
            assert_eq!(arena.family428650_actor(actor).unwrap(), &expected);
            assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
            assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(0x1122));
            assert!(matches!(
                arena.actor_service_heap().state(old),
                Ok(ActorServiceResourceState::Target(_))
            ));
            assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
            assert_eq!(process.reads.get(), 0);
        }
    }
}

#[test]
fn released_source_is_checked_after_status_and_distinct_selector35_stops_before_common_mutation() {
    let (mut arena, actor, _) = fixture(FixtureClass::Selector10);
    let (_, source_ref) = object_source(&mut arena, actor);
    let old = prior_current(&mut arena, actor);
    arena.release_actor_service(source_ref).unwrap();
    let attempts = arena.actor_service_heap().attempts().len();
    let process = Process::new(None);
    assert_eq!(
        arena.change_family_object_target(
            actor,
            enter(),
            WeightedTargetConstructorNull::Process(&process)
        ),
        Err(FamilyObjectTargetChangeError::Object(
            ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(source_ref))
        ))
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().word_228,
        0x0053_11f0
    );
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(0x1122));
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_204_208[1],
        Some(0x3344)
    );
    assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
    assert_eq!(process.reads.get(), 0);
    let (mut arena, actor, _) = fixture(FixtureClass::Selector35);
    let before = format!("{arena:?}");
    assert_eq!(
        arena.change_family_object_target(
            actor,
            enter(),
            WeightedTargetConstructorNull::Process(&process)
        ),
        Err(FamilyObjectTargetChangeError::RequiredActorVirtual(actor))
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn clone_failure_keeps_old_current_and_real_pending_allocation_without_process_fallback() {
    for failure in [
        FixtureActorServiceAllocation::Error(71),
        FixtureActorServiceAllocation::Allocate,
    ] {
        let (mut arena, actor, _) = fixture(FixtureClass::Parent);
        let (_, source_ref) = object_source(&mut arena, actor);
        let old = prior_current(&mut arena, actor);
        let attempts = arena.actor_service_heap().attempts().len();
        arena.actor_service_heap_mut().script([failure]);
        let process = Process::new(None);
        let result = arena.change_family_object_target(
            actor,
            enter(),
            WeightedTargetConstructorNull::Process(&process),
        );
        let expected = match failure {
            FixtureActorServiceAllocation::Error(71) => {
                NativeTargetError::Heap(ActorServiceHeapError::FixtureFailure(71))
            }
            FixtureActorServiceAllocation::Allocate => NativeTargetError::MissingProcessNull,
            _ => unreachable!(),
        };
        assert_eq!(result, Err(FamilyObjectTargetChangeError::Target(expected)));
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_228,
            0x0053_11f0
        );
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(0x1122));
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_204_208[1],
            Some(0x3344)
        );
        assert_eq!(
            &arena.actor_service_heap().attempts()[attempts..],
            &[crate::object_runtime::ActorServiceResourceKind::Target(
                NativeTargetClass::ObjectPosition
            )]
        );
        assert!(matches!(
            arena.actor_service_heap().state(source_ref),
            Ok(ActorServiceResourceState::Target(_))
        ));
        assert!(matches!(
            arena.actor_service_heap().state(old),
            Ok(ActorServiceResourceState::Target(_))
        ));
        assert_eq!(
            process.reads.get(),
            usize::from(matches!(failure, FixtureActorServiceAllocation::Allocate))
        );
        if matches!(failure, FixtureActorServiceAllocation::Allocate) {
            assert!(format!("{:?}", arena.actor_service_heap()).contains("state: Pending"));
        }
    }
}

#[test]
fn null_clone_is_published_and_continues_suffix_before_reached_xyz_failure() {
    for missing in [None, Some(false), Some(true)] {
        let (mut arena, actor, _) = fixture(FixtureClass::Selector36);
        let (source, source_ref) = object_source(&mut arena, actor);
        let old = prior_current(&mut arena, actor);
        arena.owned_mut(actor).unwrap().state.base.word_28 = 65535;
        if let Some(source_missing) = missing {
            let object = if source_missing { source } else { actor };
            arena.owned_mut(object).unwrap().state.base.position_words = None;
        }
        arena
            .actor_service_heap_mut()
            .script([FixtureActorServiceAllocation::Null]);
        let process = Process::new(None);
        let expected = match missing {
            None => Ok(()),
            Some(false) => Err(FamilyObjectTargetChangeError::Object(
                ObjectError::ActorPositionUnknown(actor),
            )),
            Some(true) => Err(FamilyObjectTargetChangeError::Object(
                ObjectError::TargetObjectPositionUnknown(source),
            )),
        };
        assert_eq!(
            arena.change_family_object_target(
                actor,
                enter(),
                WeightedTargetConstructorNull::Process(&process)
            ),
            expected
        );
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
        assert_eq!(process.reads.get(), 0);
        assert_eq!(
            arena.derived_actor(actor).unwrap().word_14c,
            Some(if missing.is_none() { 5 } else { 0x1122 })
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_204_208[1],
            Some(if missing.is_none() {
                0xfffc_0004
            } else {
                0x3344
            })
        );
        assert!(matches!(
            arena.actor_service_heap().state(old),
            Ok(ActorServiceResourceState::Target(_))
        ));
        assert!(matches!(
            arena.actor_service_heap().state(source_ref),
            Ok(ActorServiceResourceState::Target(_))
        ));
    }
}

#[test]
fn clone_publication_precedes_actor_xyz_fault_and_keeps_actual_object_identity() {
    let (mut arena, actor, _) = fixture(FixtureClass::Parent);
    let (source, source_ref) = object_source(&mut arena, actor);
    let old = prior_current(&mut arena, actor);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let process = Process::new(Some(ActorHandle::from_raw(0x8765_4321)));
    assert_eq!(
        arena.change_family_object_target(
            actor,
            enter(),
            WeightedTargetConstructorNull::Process(&process)
        ),
        Err(FamilyObjectTargetChangeError::Object(
            ObjectError::ActorPositionUnknown(actor)
        ))
    );
    let clone = arena.derived_actor(actor).unwrap().target_148.unwrap();
    assert_ne!(clone, source_ref);
    assert_ne!(clone, old);
    let data = arena.native_target(clone).unwrap();
    assert_eq!(data.object_referent(), Some(source));
    assert_eq!(data.handle_04().raw(), 0x8765_4321);
    assert_eq!(process.reads.get(), 1);
    assert_eq!(arena.resolve_target_actor(&data).unwrap(), None);
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(0x1122));
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_204_208[1],
        Some(0x3344)
    );
    assert!(matches!(
        arena.actor_service_heap().state(old),
        Ok(ActorServiceResourceState::Target(_))
    ));
}

#[test]
fn signed_word_wrap_bit31_recovery_and_unsigned_size_preserve_all_other_actor_fields() {
    for (origin, position, size, distance, extension) in [
        ([0, 0, 0], [0, 0, 0], 0, 0, 0),
        ([1, 2, 3], [4, 6, 3], 7, 5, 0xffff_ffe4),
        ([65535, 0, 0], [0, 65535, 32768], 65535, 32768, 0xfffc_0004),
        ([0, 0, 0], [30000, 30000, 0], 32768, 42426, 0xfffe_0000),
        ([0, 0, 0], [32768, 32768, 32768], 1, 56754, 0xffff_fffc),
    ] {
        let (mut arena, actor, _) = fixture(FixtureClass::Selector10);
        let (source, source_ref) = object_source(&mut arena, actor);
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(origin);
        arena.owned_mut(actor).unwrap().state.base.word_28 = size;
        arena.owned_mut(source).unwrap().state.base.position_words = Some(position);
        let actor_before = arena.object(actor).unwrap().clone();
        let source_before = arena.object(source).unwrap().clone();
        let mut expected = arena.family428650_actor(actor).unwrap().clone();
        expected.word_228 = 0x0053_11f0;
        expected.words_204_208[1] = Some(extension);
        let process = Process::new(Some(ActorHandle::from_raw(0x2345_6789)));
        assert_eq!(
            arena.change_family_object_target(
                actor,
                enter(),
                WeightedTargetConstructorNull::Process(&process)
            ),
            Ok(())
        );
        assert_eq!(arena.family428650_actor(actor).unwrap(), &expected);
        assert_eq!(arena.object(source).unwrap(), &source_before);
        let mut actor_after = arena.object(actor).unwrap().clone();
        let clone = arena.derived_actor(actor).unwrap().target_148.unwrap();
        assert_ne!(clone, source_ref);
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(distance));
        let crate::object_runtime::ObjectClass::Actor(state) = &mut actor_after.class else {
            panic!()
        };
        let crate::object_runtime::ActorExtension::Derived(state) = &mut state.extension else {
            panic!()
        };
        state.target_148 = None;
        state.word_14c = Some(0x1122);
        let crate::object_runtime::DerivedActorClass::Family428650(state) = &mut state.class else {
            panic!()
        };
        state.word_228 = 0;
        state.words_204_208[1] = Some(0x3344);
        assert_eq!(actor_after, actor_before);
    }
}

// Explicit post-placement route fixtures. Real canonical route finalization owns
// group/next/segment fields; later supplied unknowns isolate the reached read gates.
fn update_route_node(
    arena: &mut ObjectArena,
    identity: u16,
    predecessor: u16,
    flags: u8,
    xyz: [u16; 3],
) -> ObjectId {
    let object = arena.allocate(ObjectClassKind::RouteNode);
    arena
        .initialize_context(
            object,
            crate::object_runtime::BaseContext {
                position_words: Some(xyz),
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
fn update_current_route(
    arena: &mut ObjectArena,
    actor: ObjectId,
    source: ObjectId,
) -> crate::object_runtime::NativeHeapRef {
    let reference = arena
        .object_position_target_resource(source, WeightedTargetConstructorNull::ActorRegistry)
        .unwrap()
        .unwrap();
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    reference
}
fn update_route_pair(
    arena: &mut ObjectArena,
    actor: ObjectId,
    next_xyz: [u16; 3],
) -> (ObjectId, ObjectId, crate::object_runtime::NativeHeapRef) {
    let source = update_route_node(arena, 70, 70, 0, [10, 10, 10]);
    let next = update_route_node(arena, 71, 70, 0, next_xyz);
    arena.finalize_routes().unwrap();
    assert_eq!(arena.route_next(source), Ok(Some(next)));
    let reference = update_current_route(arena, actor, source);
    (source, next, reference)
}

#[test]
fn update_null_current_pending_and_released_resources_stop_without_flag_or_heap_recovery() {
    for state in 0..3 {
        let (mut arena, actor, _) = fixture(FixtureClass::Selector10);
        arena.derived_actor_mut(actor).unwrap().word_14c = None;
        arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa500;
        let reference = match state {
            0 => None,
            1 => Some(
                arena
                    .allocate_actor_service(crate::object_runtime::ActorServiceAllocationRequest {
                        kind: crate::object_runtime::ActorServiceResourceKind::Target(
                            NativeTargetClass::ObjectPosition,
                        ),
                    })
                    .unwrap()
                    .unwrap(),
            ),
            _ => {
                let r = prior_current(&mut arena, actor);
                arena.release_actor_service(r).unwrap();
                Some(r)
            }
        };
        arena.derived_actor_mut(actor).unwrap().target_148 = reference;
        let before = format!("{arena:?}");
        let result = arena.update_family_object_target(actor);
        match (state, reference) {
            (0, _) => assert_eq!(
                result,
                Err(FamilyObjectTargetUpdateError::MissingCurrentTarget(actor))
            ),
            (1, Some(r)) => assert_eq!(
                result,
                Err(FamilyObjectTargetUpdateError::Object(
                    ObjectError::ActorServiceHeap(ActorServiceHeapError::Pending(r))
                ))
            ),
            (_, Some(r)) => assert_eq!(
                result,
                Err(FamilyObjectTargetUpdateError::Object(
                    ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(r))
                ))
            ),
            _ => unreachable!(),
        }
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn update_virtual14_null_sets_only_low_flag_bit_before_distance_position_or_registry_reads() {
    for class in [
        FixtureClass::Parent,
        FixtureClass::Selector10,
        FixtureClass::Selector36,
        FixtureClass::Selector12,
    ] {
        for kind in [
            NativeTargetClass::FixedPosition,
            NativeTargetClass::ActorFollow,
        ] {
            let (mut arena, actor, _) = fixture(class);
            let data = match kind {
                NativeTargetClass::FixedPosition => {
                    NativeTarget::FixedPosition(FixedPositionTarget {
                        handle_04: ActorHandle::from_raw(0x7788),
                        focus_08: None,
                        word_10: 8,
                    })
                }
                NativeTargetClass::ActorFollow => NativeTarget::ActorFollow(
                    arena
                        .follow_target(crate::object_runtime::FollowTargetRequest {
                            handle: ActorHandle::from_raw(0x7788),
                            point: crate::object_runtime::FollowTargetPoint::ExplicitPoint(None),
                        })
                        .unwrap(),
                ),
                _ => unreachable!(),
            };
            let reference = arena.import_fixture_target(data);
            arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
            arena.derived_actor_mut(actor).unwrap().word_14c = None;
            arena.owned_mut(actor).unwrap().state.base.position_words = None;
            arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa5c0;
            arena.actor_handles = None;
            let before = format!("{arena:?}");
            assert_eq!(arena.update_family_object_target(actor), Ok(()));
            assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0xa5c4);
            arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa5c0;
            assert_eq!(format!("{arena:?}"), before);
        }
    }
}

#[test]
fn update_signed_near_distance_advances_without_segment_or_actor_position_reads_and_without_allocation(
) {
    for distance in [0, 255, u32::MAX, 0x8000_0000] {
        let (mut arena, actor, _) = fixture(FixtureClass::Selector10);
        let (source, next, reference) = update_route_pair(&mut arena, actor, [50, 60, 70]);
        arena.derived_actor_mut(actor).unwrap().word_14c = Some(distance);
        arena.route_mut(source).unwrap().distance_squared = 0;
        arena.owned_mut(source).unwrap().state.base.position_words = None;
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        let actor_before = arena.object(actor).unwrap().clone();
        let attempts = arena.actor_service_heap().attempts().len();
        let pending = arena.route_pending().to_vec();
        let groups = arena.route_groups().to_vec();
        assert_eq!(arena.update_family_object_target(actor), Ok(()));
        let target = arena.object_position_target_data(reference).unwrap();
        assert_eq!(target.object_14, next);
        assert_eq!(target.focus_08, [50, 60, 70]);
        assert_eq!(arena.object(actor).unwrap(), &actor_before);
        assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
        assert_eq!(arena.route_pending(), pending);
        assert_eq!(arena.route_groups(), groups);
    }
}

#[test]
fn update_endpoint_sets_mask_04_only_in_near_branch_and_preserves_far_null_and_zero_paths() {
    for (distance, segment, expected_flags) in
        [(255, 0, 0xa5c4), (256, 0, 0xa5c0), (256, 1, 0xa5c0)]
    {
        let (mut arena, actor, _) = fixture(FixtureClass::Selector36);
        let source = update_route_node(&mut arena, 80, 80, 0, [1, 2, 3]);
        arena.finalize_routes().unwrap();
        assert_eq!(arena.route_next(source), Ok(None));
        let reference = update_current_route(&mut arena, actor, source);
        arena.route_mut(source).unwrap().distance_squared = segment;
        arena.derived_actor_mut(actor).unwrap().word_14c = Some(distance);
        arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa5c0;
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        let target = arena.native_target(reference).unwrap();
        let before = format!("{arena:?}");
        assert_eq!(arena.update_family_object_target(actor), Ok(()));
        assert_eq!(arena.native_target(reference).unwrap(), target);
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_22e,
            expected_flags
        );
        arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa5c0;
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn update_zero_segment_skips_unfinalized_route_group_but_nonzero_reaches_required_group() {
    let (mut arena, actor, _) = fixture(FixtureClass::Parent);
    let source = update_route_node(&mut arena, 90, 90, 0, [1, 2, 3]);
    update_current_route(&mut arena, actor, source);
    arena.derived_actor_mut(actor).unwrap().word_14c = Some(256);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let before = format!("{arena:?}");
    assert_eq!(arena.update_family_object_target(actor), Ok(()));
    assert_eq!(format!("{arena:?}"), before);
    arena.route_mut(source).unwrap().distance_squared = 1;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.update_family_object_target(actor),
        Err(FamilyObjectTargetUpdateError::Object(
            ObjectError::RouteUnfinalized(source)
        ))
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn update_far_uses_unsigned_wrapped_squares_with_exact_threshold_equality_and_no_isqrt() {
    //Literal retail arithmetic outcomes discriminate signed comparison, WORD
    //subtraction, DWORD accumulation, equality and a spurious square root.
    for (actor_xyz, next_xyz, segment, advances) in [
        ([0, 0, 0], [3, 4, 0], 26, true),
        ([0, 0, 0], [3, 4, 0], 25, false),
        ([0, 0, 0], [3, 4, 0], 24, false),
        ([0xffff, 0, 0], [0, 0, 0], 2, true),
        ([0, 0, 0], [0x8000, 0x8000, 0x8000], 0xc000_0001, true),
        ([0, 0, 0], [0x8000, 0x8000, 0x8000], 0xc000_0000, false),
        ([0, 0, 0], [0x8000, 0x8000, 0], 0x7fff_ffff, false),
    ] {
        let (mut arena, actor, _) = fixture(FixtureClass::Selector10);
        let (source, next, reference) = update_route_pair(&mut arena, actor, next_xyz);
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(actor_xyz);
        arena.derived_actor_mut(actor).unwrap().word_14c = Some(256);
        arena.route_mut(source).unwrap().distance_squared = segment;
        let actor_before = arena.object(actor).unwrap().clone();
        let attempts = arena.actor_service_heap().attempts().len();
        assert_eq!(arena.update_family_object_target(actor), Ok(()));
        let target = arena.object_position_target_data(reference).unwrap();
        assert_eq!(target.object_14, if advances { next } else { source });
        assert_eq!(
            target.focus_08,
            if advances { next_xyz } else { [10, 10, 10] }
        );
        assert_eq!(arena.object(actor).unwrap(), &actor_before);
        assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
    }
}

#[test]
fn update_wrapping_singleton_same_referent_skips_target_xyz_and_preserves_whole_arena() {
    let (mut arena, actor, _) = fixture(FixtureClass::Parent);
    let source = update_route_node(&mut arena, 100, 100, 1, [7, 8, 9]);
    arena.finalize_routes().unwrap();
    assert_eq!(arena.route_next(source), Ok(Some(source)));
    update_current_route(&mut arena, actor, source);
    arena.derived_actor_mut(actor).unwrap().word_14c = Some(0);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    arena.owned_mut(source).unwrap().state.base.position_words = None;
    let before = format!("{arena:?}");
    assert_eq!(arena.update_family_object_target(actor), Ok(()));
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn update_changed_referent_is_retained_before_unknown_position_error_without_reallocation() {
    let (mut arena, actor, _) = fixture(FixtureClass::Selector36);
    let (_source, next, reference) = update_route_pair(&mut arena, actor, [50, 60, 70]);
    arena.derived_actor_mut(actor).unwrap().word_14c = Some(0);
    arena.owned_mut(next).unwrap().state.base.position_words = None;
    let actor_before = arena.object(actor).unwrap().clone();
    let attempts = arena.actor_service_heap().attempts().len();
    let old_focus = arena
        .object_position_target_data(reference)
        .unwrap()
        .focus_08;
    assert_eq!(
        arena.update_family_object_target(actor),
        Err(FamilyObjectTargetUpdateError::Object(
            ObjectError::TargetObjectPositionUnknown(next)
        ))
    );
    let target = arena.object_position_target_data(reference).unwrap();
    assert_eq!(target.object_14, next);
    assert_eq!(target.focus_08, old_focus);
    assert_eq!(arena.object(actor).unwrap(), &actor_before);
    assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
    assert!(matches!(
        arena.actor_service_heap().state(reference),
        Ok(ActorServiceResourceState::Target(_))
    ));
}

#[test]
fn update_unknown_distance_precedes_route_and_position_reads_and_nonroute_keeps_typed_boundary() {
    let (mut arena, actor, _) = fixture(FixtureClass::Selector10);
    let source = update_route_node(&mut arena, 110, 110, 0, [1, 2, 3]);
    update_current_route(&mut arena, actor, source);
    arena.derived_actor_mut(actor).unwrap().word_14c = None;
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.update_family_object_target(actor),
        Err(FamilyObjectTargetUpdateError::UninitializedDistance(actor))
    );
    assert_eq!(format!("{arena:?}"), before);
    let base = arena.allocate(ObjectClassKind::Base);
    arena.owned_mut(base).unwrap().state.base.position_words = Some([1, 2, 3]);
    update_current_route(&mut arena, actor, base);
    arena.derived_actor_mut(actor).unwrap().word_14c = Some(0);
    let before = format!("{arena:?}");
    assert_eq!(
        arena.update_family_object_target(actor),
        Err(FamilyObjectTargetUpdateError::Object(
            ObjectError::NotRouteNode(base)
        ))
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn update_keeps_null_clone_change_prefix_and_selector35_distinct_policy_without_callbacks() {
    let (mut arena, actor, _) = fixture(FixtureClass::Selector10);
    object_source(&mut arena, actor);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    arena
        .change_family_object_target(actor, enter(), WeightedTargetConstructorNull::ActorRegistry)
        .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
    assert!(arena.derived_actor(actor).unwrap().word_14c.is_some());
    let before = format!("{arena:?}");
    assert_eq!(
        arena.update_family_object_target(actor),
        Err(FamilyObjectTargetUpdateError::MissingCurrentTarget(actor))
    );
    assert_eq!(format!("{arena:?}"), before);

    let (mut other, actor, _) = fixture(FixtureClass::Selector35);
    let before = format!("{other:?}");
    assert_eq!(
        other.update_family_object_target(actor),
        Err(FamilyObjectTargetUpdateError::RequiredActorVirtual(actor))
    );
    assert_eq!(format!("{other:?}"), before);
    for present in [false, true] {
        let profile = crate::object_runtime::FamilyBehaviorProfile::Selector12 {
            behavior_present: present,
        };
        for index in 0..profile.descriptor_table().unwrap().1 {
            assert_ne!(
                profile.descriptor(index).unwrap().update_address,
                0x0043_faa0
            );
        }
    }
}
