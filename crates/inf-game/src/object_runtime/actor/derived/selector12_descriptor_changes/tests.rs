//! Genuine strict class12 construction plus explicitly supplied target resources.
//! These tests do not select a loaded actor or claim descriptor/scene acceptance.
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorFollowTarget, ActorHandle, ActorRegistryConfig,
    ActorServiceHeapError, ActorServiceResourceState, ActorTerrainInputs,
    DerivedActorConstructionRequest, FixedPositionTarget, FixtureActorServiceAllocation,
    NativeTarget, ObjectClassKind, Selector12GrantContext, TargetNullHandleSource,
};
use std::cell::Cell;

const SINE: [u16; 4096] = [0; 4096];
fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}
fn subject(arena: &mut ObjectArena) -> ObjectId {
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([11, 22, 33]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap()
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = arena();
    let actor = subject(&mut arena);
    (arena, actor)
}
fn fixed(arena: &mut ObjectArena, point: Option<[u16; 3]>) -> crate::object_runtime::NativeHeapRef {
    arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(7),
        focus_08: point,
        word_10: 8,
    }))
}
struct Source {
    value: Cell<Option<ActorHandle>>,
    reads: Cell<usize>,
}
impl Source {
    fn new(value: Option<ActorHandle>) -> Self {
        Self {
            value: Cell::new(value),
            reads: Cell::new(0),
        }
    }
}
impl TargetNullHandleSource for Source {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.reads.set(self.reads.get() + 1);
        self.value.get()
    }
}
fn change(
    arena: &mut ObjectArena,
    actor: ObjectId,
    kind: Selector12DescriptorChangeKind,
    entering: bool,
    source: &Source,
) -> Result<(), Selector12DescriptorChangeError> {
    arena.change_selector12_descriptor_target(
        actor,
        Selector12DescriptorChangeRequest { kind, entering },
        WeightedTargetConstructorNull::Process(source),
    )
}

#[test]
fn actual_descriptor_profiles_map_the_four_class12_change_wrappers() {
    for present in [false, true] {
        let profile = FamilyBehaviorProfile::Selector12 {
            behavior_present: present,
        };
        for (index, kind) in [
            (2, Selector12DescriptorChangeKind::Target26c),
            (3, Selector12DescriptorChangeKind::PointAtCurrentPosition),
            (
                if present { 5 } else { 4 },
                Selector12DescriptorChangeKind::Target274,
            ),
            (
                if present { 8 } else { 7 },
                Selector12DescriptorChangeKind::PreferredTarget27c284,
            ),
        ] {
            let descriptor = profile.descriptor(index).unwrap();
            assert_eq!(
                Selector12DescriptorChangeRequest::from_descriptor(profile, descriptor, true),
                Some(Selector12DescriptorChangeRequest {
                    kind,
                    entering: true
                })
            );
            assert!(Selector12DescriptorChangeRequest::from_descriptor(
                FamilyBehaviorProfile::Family4236d0 { mode: 1 },
                descriptor,
                true
            )
            .is_none());
        }
        assert!(Selector12DescriptorChangeRequest::from_descriptor(
            profile,
            profile.descriptor(0).unwrap(),
            true
        )
        .is_none());
    }
}

#[test]
fn false_target274_bypasses_wrong_class_foreign_custody_and_process() {
    let mut arena = arena();
    let base = arena.allocate(ObjectClassKind::Base);
    let foreign = self::arena().allocate(ObjectClassKind::Base);
    let source = Source::new(None);
    for actor in [base, foreign] {
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::Target274,
            false,
            &source,
        )
        .unwrap();
    }
    assert_eq!(source.reads.get(), 0);
    assert!(arena.actor_service_heap().attempts().is_empty());
}

#[test]
fn false_preferred_change_clears_only_actual29c() {
    let (mut arena, actor) = fixture();
    arena.selector12_actor_mut(actor).unwrap().byte_29c = 0xa5;
    let mut expected = arena.selector12_actor(actor).unwrap().clone();
    expected.byte_29c = 0;
    let family = arena.family428650_actor(actor).unwrap().clone();
    let source = Source::new(None);
    change(
        &mut arena,
        actor,
        Selector12DescriptorChangeKind::PreferredTarget27c284,
        false,
        &source,
    )
    .unwrap();
    assert_eq!(arena.selector12_actor(actor).unwrap(), &expected);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().word_228,
        family.word_228
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c,
        family.words_188_19c
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_204_208,
        family.words_204_208
    );
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn null_target274_replaces_pointer_without_retiring_old_resource() {
    let (mut arena, actor) = fixture();
    let old = fixed(&mut arena, None);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
    let attempts = arena.actor_service_heap().attempts().len();
    let source = Source::new(None);
    change(
        &mut arena,
        actor,
        Selector12DescriptorChangeKind::Target274,
        true,
        &source,
    )
    .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
    let family = arena.family428650_actor(actor).unwrap();
    assert_eq!(family.word_228, 0x53a138);
    assert_eq!(family.words_204_208[1], Some((-100_i32) as u32));
    assert_eq!(family.words_188_19c[1..], [None; 5]);
    assert!(matches!(
        arena.actor_service_heap().state(old),
        Ok(ActorServiceResourceState::Target(_))
    ));
    assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn target274_real_clone_reads_current_process_and_preserves_unknown_point() {
    let (mut arena, actor) = fixture();
    let target = fixed(&mut arena, None);
    let old = fixed(&mut arena, Some([1, 2, 3]));
    arena.selector12_actor_mut(actor).unwrap().targets_26c_284[1] = Some(target);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
    let source = Source::new(Some(ActorHandle::from_raw(19)));
    source.value.set(Some(ActorHandle::from_raw(23)));
    change(
        &mut arena,
        actor,
        Selector12DescriptorChangeKind::Target274,
        true,
        &source,
    )
    .unwrap();
    let clone = arena.derived_actor(actor).unwrap().target_148.unwrap();
    assert_ne!(clone, target);
    assert_ne!(clone, old);
    assert_eq!(
        arena.native_target(clone).unwrap(),
        NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: ActorHandle::from_raw(23),
            focus_08: None,
            word_10: 8,
        })
    );
    assert!(matches!(
        arena.actor_service_heap().state(old),
        Ok(ActorServiceResourceState::Target(_))
    ));
    assert_eq!(source.reads.get(), 1);
}

#[test]
fn target274_missing_process_retains_status_old_pointer_and_spacing() {
    let (mut arena, actor) = fixture();
    let target = fixed(&mut arena, None);
    let old = fixed(&mut arena, Some([1, 2, 3]));
    arena.selector12_actor_mut(actor).unwrap().targets_26c_284[1] = Some(target);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(77);
    let attempts = arena.actor_service_heap().attempts().len();
    let source = Source::new(None);
    assert!(matches!(
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::Target274,
            true,
            &source
        ),
        Err(Selector12DescriptorChangeError::Target(
            NativeTargetError::MissingProcessNull
        ))
    ));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x53a138);
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_204_208[1],
        Some(77)
    );
    assert_eq!(arena.actor_service_heap().attempts().len(), attempts + 1);
    assert_eq!(source.reads.get(), 1);
}

#[test]
fn null_preferred_target_keeps_unknown_carries_and_existing_pointer() {
    let (mut arena, actor) = fixture();
    let old = fixed(&mut arena, None);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
    arena.family428650_actor_mut(actor).unwrap().word_22e = 0x8000;
    let source = Source::new(None);
    change(
        &mut arena,
        actor,
        Selector12DescriptorChangeKind::PreferredTarget27c284,
        true,
        &source,
    )
    .unwrap();
    let family = arena.family428650_actor(actor).unwrap();
    assert_eq!(family.word_228, 0x53a154);
    assert_eq!(family.word_22e, 0x8001);
    assert_eq!(
        family.words_188_19c,
        [Some(0), None, None, Some(1), None, None]
    );
    assert_eq!(family.words_204_208[1], None);
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29c, 1);
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn preferred_selection_is_unsigned_and_ties_choose284_with_null_actor_threshold() {
    for (left, right, expected) in [
        (0x8000_0000, 7, [1, 2, 3]),
        (7, 7, [4, 5, 6]),
        (1, 9, [4, 5, 6]),
    ] {
        let (mut arena, actor) = fixture();
        let a = fixed(&mut arena, Some([1, 2, 3]));
        let b = fixed(&mut arena, Some([4, 5, 6]));
        let state = arena.selector12_actor_mut(actor).unwrap();
        state.word_280 = left;
        state.word_288 = right;
        state.targets_26c_284[2] = Some(a);
        state.targets_26c_284[3] = Some(b);
        let source = Source::new(Some(ActorHandle::from_raw(32)));
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::PreferredTarget27c284,
            true,
            &source,
        )
        .unwrap();
        let clone = arena.derived_actor(actor).unwrap().target_148.unwrap();
        let NativeTarget::FixedPosition(target) = arena.native_target(clone).unwrap() else {
            panic!()
        };
        assert_eq!(target.focus_08, Some(expected));
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c,
            [Some(0), Some(0), Some(4_000_000), Some(0), Some(288), None]
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_204_208[1],
            Some(128)
        );
        assert_eq!(source.reads.get(), 1);
    }
}

#[test]
fn preferred_actor_query_precedes_even_scripted_null_clone_allocation() {
    let (mut arena, actor) = fixture();
    let handle = arena.actor(actor).unwrap().handle;
    let target = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
        handle_04: handle,
        focus_08: None,
        word_10: 2,
        focus_14: Some([7, 8, 9]),
        velocity_1a: None,
    }));
    arena.selector12_actor_mut(actor).unwrap().targets_26c_284[3] = Some(target);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    let attempts = arena.actor_service_heap().attempts().len();
    //Explicit unavailable registry fixture, no claim of a native writer/removal.
    arena.actor_handles = None;
    let source = Source::new(None);
    assert!(matches!(
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::PreferredTarget27c284,
            true,
            &source
        ),
        Err(Selector12DescriptorChangeError::Object(
            ObjectError::MissingActorRegistry
        ))
    ));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x53a154);
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29c, 1);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1..],
        [None; 5]
    );
    assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn preferred_follow_capture_uses_unsigned_original_size_even_for_null_clone() {
    for null_clone in [false, true] {
        let (mut arena, actor) = fixture();
        let referent = subject(&mut arena);
        arena.owned_mut(referent).unwrap().state.base.word_28 = 65535;
        let handle = arena.actor(referent).unwrap().handle;
        let target = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: handle,
            focus_08: Some([1, 2, 3]),
            word_10: 2,
            focus_14: Some([7, 8, 9]),
            velocity_1a: Some([11, 12, 13]),
        }));
        arena.selector12_actor_mut(actor).unwrap().targets_26c_284[3] = Some(target);
        if null_clone {
            arena
                .actor_service_heap_mut()
                .script([FixtureActorServiceAllocation::Null]);
        }
        let source = Source::new(None);
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::PreferredTarget27c284,
            true,
            &source,
        )
        .unwrap();
        let current = arena.derived_actor(actor).unwrap().target_148;
        if null_clone {
            assert_eq!(current, None);
        } else {
            let NativeTarget::ActorFollow(cloned) = arena.native_target(current.unwrap()).unwrap()
            else {
                panic!()
            };
            assert_eq!(cloned.handle_04, handle);
            assert_eq!(cloned.focus_14, Some([7, 8, 9]));
            assert_eq!(cloned.focus_08, None);
            assert_eq!(cloned.velocity_1a, None);
        }
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c,
            [
                Some(0),
                Some(0),
                Some(4_000_000),
                Some(0),
                Some(65823),
                None
            ]
        );
        assert_eq!(source.reads.get(), 0); //Real follow clone has no process NULL read.
    }
}

#[test]
fn preferred_released_resource_preserves_only_reached_prefix_and_blocks_allocation() {
    let (mut arena, actor) = fixture();
    let target = fixed(&mut arena, None);
    arena.release_actor_service(target).unwrap();
    arena.selector12_actor_mut(actor).unwrap().targets_26c_284[3] = Some(target);
    let attempts = arena.actor_service_heap().attempts().len();
    let source = Source::new(None);
    assert!(
        matches!(change(&mut arena,actor,Selector12DescriptorChangeKind::PreferredTarget27c284,true,&source),
        Err(Selector12DescriptorChangeError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(actual)))) if actual == target)
    );
    assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x53a154);
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29c, 1);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1..],
        [None; 5]
    );
    assert_eq!(arena.actor_service_heap().attempts().len(), attempts);
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn preferred_clone_error_retains_status_byte_and_old_current_before_carry_suffix() {
    let (mut arena, actor) = fixture();
    let target = fixed(&mut arena, None);
    let old = fixed(&mut arena, Some([1, 2, 3]));
    arena.selector12_actor_mut(actor).unwrap().targets_26c_284[3] = Some(target);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(77);
    let attempts = arena.actor_service_heap().attempts().len();
    let source = Source::new(None);
    assert!(matches!(
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::PreferredTarget27c284,
            true,
            &source
        ),
        Err(Selector12DescriptorChangeError::Target(
            NativeTargetError::MissingProcessNull
        ))
    ));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x53a154);
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29c, 1);
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_204_208[1],
        Some(77)
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1..],
        [None; 5]
    );
    assert_eq!(arena.actor_service_heap().attempts().len(), attempts + 1);
    assert_eq!(source.reads.get(), 1);
}
