//! Strict class12 and explicit supplied heap/timer/point domains.
//! Native source constants below describe arithmetic fixtures, not retail captures.
use super::*;
use crate::object_runtime::*;
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
    let mut rng = 0;
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
            &mut rng,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                    records: vec![],
                },
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
fn fixed(arena: &mut ObjectArena, point: Option<[u16; 3]>) -> NativeHeapRef {
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
fn update(
    arena: &mut ObjectArena,
    actor: ObjectId,
    micros: u32,
    rng: &mut u32,
) -> Result<(), Selector12PointUpdateError> {
    arena.update_selector12_point_target(
        actor,
        Selector12PointUpdateRequest {
            microseconds: micros,
            settings_7c: 1,
            rng,
        },
    )
}
fn install(
    arena: &mut ObjectArena,
    actor: ObjectId,
    focus: [u16; 3],
) -> (NativeHeapRef, NativeHeapRef) {
    let selected = fixed(arena, Some(focus));
    let current = fixed(arena, Some([91, 92, 93]));
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.words_188_19c[1] = Some(0);
    family.targets_1d4_1fc[1] = Some(selected);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(current);
    (selected, current)
}

#[test]
fn false_new_changes_bypass_receiver_heap_and_current_process() {
    let mut arena = arena();
    let base = arena.allocate(ObjectClassKind::Base);
    let foreign = self::arena().allocate(ObjectClassKind::Base);
    let source = Source::new(None);
    for actor in [base, foreign] {
        for kind in [
            Selector12DescriptorChangeKind::Target26c,
            Selector12DescriptorChangeKind::PointAtCurrentPosition,
        ] {
            change(&mut arena, actor, kind, false, &source).unwrap();
        }
    }
    assert_eq!(source.reads.get(), 0);
    assert!(arena.actor_service_heap().attempts().is_empty());
}

#[test]
fn target26c_null_and_clone_keep_distinct_fresh29d_spacing_gate() {
    for (nonnull, byte) in [(false, 0), (false, 1), (true, 0), (true, 1)] {
        let (mut arena, actor) = fixture();
        let old = fixed(&mut arena, None);
        let source_target = fixed(&mut arena, Some([4, 5, 6]));
        arena.selector12_actor_mut(actor).unwrap().targets_26c_284[0] =
            nonnull.then_some(source_target);
        arena.selector12_actor_mut(actor).unwrap().byte_29d = byte;
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
        arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(77);
        let source = Source::new(Some(ActorHandle::from_raw(19)));
        source.value.set(Some(ActorHandle::from_raw(23)));
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::Target26c,
            true,
            &source,
        )
        .unwrap();
        let current = arena.derived_actor(actor).unwrap().target_148;
        if nonnull {
            assert_ne!(current, Some(source_target));
            let NativeTarget::FixedPosition(cloned) =
                arena.native_target(current.unwrap()).unwrap()
            else {
                panic!()
            };
            assert_eq!(cloned.handle_04, ActorHandle::from_raw(23));
            assert_eq!(cloned.focus_08, Some([4, 5, 6]));
        } else {
            assert_eq!(current, None);
        }
        let family = arena.family428650_actor(actor).unwrap();
        assert_eq!(family.word_228, 0x53a12c);
        assert_eq!(
            family.words_204_208[1],
            Some(if byte == 0 { 256 } else { 77 })
        );
        assert_eq!(family.words_188_19c[1..], [None; 5]);
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
        assert!(matches!(
            arena.actor_service_heap().state(old),
            Ok(ActorServiceResourceState::Target(_))
        ));
        assert_eq!(source.reads.get(), usize::from(nonnull));
    }
}

#[test]
fn target26c_failed_clone_retains_status_before_pointer_and_spacing() {
    let (mut arena, actor) = fixture();
    let old = fixed(&mut arena, None);
    let source_target = fixed(&mut arena, None);
    arena.selector12_actor_mut(actor).unwrap().targets_26c_284[0] = Some(source_target);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(77);
    let source = Source::new(None);
    assert!(matches!(
        change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::Target26c,
            true,
            &source
        ),
        Err(Selector12DescriptorChangeError::Target(
            NativeTargetError::MissingProcessNull
        ))
    ));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x53a12c);
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_204_208[1],
        Some(77)
    );
    assert_eq!(source.reads.get(), 1);
}

#[test]
fn point_change_null_is_lazy_and_allocator_error_retains_prior_suffix() {
    for allocation in [
        FixtureActorServiceAllocation::Null,
        FixtureActorServiceAllocation::Error(19),
    ] {
        let (mut arena, actor) = fixture();
        let old = fixed(&mut arena, None);
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
        arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(77);
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        arena.actor_service_heap_mut().script([allocation]);
        let source = Source::new(None);
        let result = change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::PointAtCurrentPosition,
            true,
            &source,
        );
        assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x53a148);
        assert_eq!(source.reads.get(), 0);
        match allocation {
            FixtureActorServiceAllocation::Null => {
                result.unwrap();
                assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
                assert_eq!(
                    arena.family428650_actor(actor).unwrap().words_188_19c[1],
                    Some(0)
                );
            }
            _ => {
                assert!(matches!(
                    result,
                    Err(Selector12DescriptorChangeError::Target(
                        NativeTargetError::Heap(ActorServiceHeapError::FixtureFailure(19))
                    ))
                ));
                assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
                assert_eq!(
                    arena.family428650_actor(actor).unwrap().words_188_19c[1],
                    Some(77)
                );
            }
        }
    }
}

#[test]
fn point_change_constructs_current_xyz_and_preserves_prior_resource_and14c() {
    let (mut arena, actor) = fixture();
    let old = fixed(&mut arena, None);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
    let source = Source::new(Some(ActorHandle::from_raw(17)));
    source.value.set(Some(ActorHandle::from_raw(23)));
    change(
        &mut arena,
        actor,
        Selector12DescriptorChangeKind::PointAtCurrentPosition,
        true,
        &source,
    )
    .unwrap();
    let current = arena.derived_actor(actor).unwrap().target_148.unwrap();
    assert_ne!(current, old);
    assert_eq!(
        arena.native_target(current).unwrap(),
        NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: ActorHandle::from_raw(23),
            focus_08: Some([11, 22, 33]),
            word_10: 8
        })
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1..],
        [Some(0), None, None, None, None]
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    assert!(matches!(
        arena.actor_service_heap().state(old),
        Ok(ActorServiceResourceState::Target(_))
    ));
    assert_eq!(source.reads.get(), 1);
}

#[test]
fn point_change_required_process_then_xyz_preserve_old_pointer_and_timer() {
    for process in [None, Some(ActorHandle::from_raw(23))] {
        let (mut arena, actor) = fixture();
        let old = fixed(&mut arena, None);
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(old);
        arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(77);
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        let before = arena.actor_service_heap().attempts().len();
        let source = Source::new(process);
        let result = change(
            &mut arena,
            actor,
            Selector12DescriptorChangeKind::PointAtCurrentPosition,
            true,
            &source,
        );
        if process.is_none() {
            assert!(matches!(
                result,
                Err(Selector12DescriptorChangeError::Target(
                    NativeTargetError::MissingProcessNull
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(Selector12DescriptorChangeError::RequiredField {
                    offset: 0x32,
                    address: 0x467619,
                    ..
                })
            ));
        }
        assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x53a148);
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[1],
            Some(77)
        );
        assert_eq!(arena.actor_service_heap().attempts().len(), before + 1);
        assert_eq!(source.reads.get(), 1);
    }
}

#[test]
fn update_requires18c_then_signed_timer_bypasses_all_target_backing_and_rng() {
    let (mut arena, actor) = fixture();
    let mut rng = 0;
    assert!(matches!(
        update(&mut arena, actor, 0, &mut rng),
        Err(Selector12PointUpdateError::RequiredField { offset: 0x18c, .. })
    ));
    assert_eq!(rng, 0);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    for (timer, micros, expected) in [(2, 1, 1), (0, u32::MAX, 1)] {
        arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(timer);
        update(&mut arena, actor, micros, &mut rng).unwrap();
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[1],
            Some(expected)
        );
        assert_eq!(rng, 0);
        assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
    }
}

#[test]
fn expired_null_pair_sets_only_low_flag_without_reset_or_randomness() {
    let (mut arena, actor) = fixture();
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.words_188_19c[1] = Some(u32::MAX);
    family.word_22e = 0x8000;
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut rng = 0;
    update(&mut arena, actor, 0, &mut rng).unwrap();
    let family = arena.family428650_actor(actor).unwrap();
    assert_eq!(family.word_22e, 0x8001);
    assert_eq!(
        family.words_188_19c[1..],
        [Some(u32::MAX), None, None, None, None]
    );
    assert_eq!(rng, 0);
}

#[test]
fn unsigned_score_choice_and_tie_reach_selected_cached_focus_after_one_rng() {
    for (left_score, right_score, choose_right) in
        [(7, 0x80000000, true), (7, 7, false), (0xffffffff, 7, false)]
    {
        let (mut arena, actor) = fixture();
        let left = fixed(&mut arena, None);
        let right = fixed(&mut arena, None);
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.words_188_19c[1] = Some(0);
        family.targets_1d4_1fc[1] = Some(left);
        family.targets_1d4_1fc[3] = Some(right);
        family.words_1d8_200[1] = left_score;
        family.words_1d8_200[3] = right_score;
        let mut rng = 0;
        assert!(matches!(update(&mut arena, actor, 0, &mut rng),
            Err(Selector12PointUpdateError::RequiredTargetFocus { reference, .. })
            if reference == if choose_right { right } else { left }));
        assert_eq!(rng, 0x0013d573);
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[1],
            Some(1000304)
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[2],
            None
        );
    }
}

#[test]
fn captured_follow_refresh_writes_real_focus_velocity_and_settings_point() {
    let (mut arena, actor) = fixture();
    let referent = subject(&mut arena);
    arena.owned_mut(referent).unwrap().state.base.position_words = Some([0, 0, 0]);
    let state = arena.actor_mut(referent).unwrap();
    state.word_74 = 3;
    state.words_8e_92 = [1, 2, 3];
    let selected = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
        handle_04: arena.actor(referent).unwrap().handle,
        focus_08: None,
        word_10: 2,
        focus_14: None,
        velocity_1a: None,
    }));
    let current = fixed(&mut arena, None);
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.words_188_19c[1] = Some(0);
    family.targets_1d4_1fc[3] = Some(selected);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(current);
    let mut rng = 0;
    update(&mut arena, actor, 0, &mut rng).unwrap();
    let NativeTarget::ActorFollow(target) = arena.native_target(selected).unwrap() else {
        panic!()
    };
    assert_eq!(target.focus_08, Some([0, 3, 0]));
    assert_eq!(target.focus_14, target.focus_08);
    assert_eq!(target.velocity_1a, Some([1, 2, 3]));
    assert!(arena
        .native_target(current)
        .unwrap()
        .raw_focus_08_words()
        .is_some());
    assert_eq!(rng, 0x096ea295);
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, None);
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
}

#[test]
fn refresh_error_retains_timer_rng_and_captured_resource_without_distance_store() {
    let (mut arena, actor) = fixture();
    let referent = subject(&mut arena);
    arena.owned_mut(referent).unwrap().state.base.position_words = None;
    let original = NativeTarget::ActorFollow(ActorFollowTarget {
        handle_04: arena.actor(referent).unwrap().handle,
        focus_08: Some([4, 5, 6]),
        word_10: 2,
        focus_14: None,
        velocity_1a: None,
    });
    let selected = arena.import_fixture_target(original);
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.words_188_19c[1] = Some(0);
    family.targets_1d4_1fc[1] = Some(selected);
    let mut rng = 0;
    assert!(matches!(update(&mut arena, actor, 0, &mut rng),
        Err(Selector12PointUpdateError::Object(ObjectError::ActorPositionUnknown(id))) if id == referent));
    assert_eq!(arena.native_target(selected).unwrap(), original);
    assert_eq!(rng, 0x0013d573);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1],
        Some(1000304)
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[2],
        None
    );
}

#[test]
fn actor_xyz_is_required_before_selected_cached_focus() {
    let (mut arena, actor) = fixture();
    let selected = fixed(&mut arena, None);
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.words_188_19c[1] = Some(0);
    family.targets_1d4_1fc[1] = Some(selected);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut rng = 0;
    assert!(matches!(
        update(&mut arena, actor, 0, &mut rng),
        Err(Selector12PointUpdateError::RequiredField { offset: 0x34, .. })
    ));
    assert_eq!(rng, 0x0013d573);
}

#[test]
fn far_close_parity_wrapping_and_bit31_use_exact_rng_and_point_units() {
    //Expected values are from native WORD/DWORD operations with supplied seed0.
    for (focus, distance, point) in [
        ([64547, 22, 65369], 1019, [1388, 22, 240]),
        ([10, 22, 33], 3248, [3231, 22, 462]),
        ([11, 22, 33], 3248, [62327, 22, 65140]),
        ([32779, 32790, 33], 46340, [20407, 22, 33]),
    ] {
        let (mut arena, actor) = fixture();
        let (_, current) = install(&mut arena, actor, focus);
        let mut rng = 0;
        update(&mut arena, actor, 0, &mut rng).unwrap();
        assert_eq!(
            arena.native_target(current).unwrap().raw_focus_08_words(),
            Some(point)
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[1],
            Some(1000304)
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[2],
            Some(distance)
        );
        assert_eq!(rng, 0x096ea295);
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    }
}

#[test]
fn fresh_current148_fault_and_full_kind_gate_are_after_rng_and190() {
    for kind in [None, Some(8), Some(0x108), Some(0x10008)] {
        let (mut arena, actor) = fixture();
        let (_, current) = install(&mut arena, actor, [64547, 22, 65369]);
        if let Some(kind) = kind {
            let NativeTarget::FixedPosition(mut target) = arena.native_target(current).unwrap()
            else {
                panic!()
            };
            target.word_10 = kind;
            arena
                .write_native_target(current, NativeTarget::FixedPosition(target))
                .unwrap();
        } else {
            arena.derived_actor_mut(actor).unwrap().target_148 = None;
        }
        let mut rng = 0;
        let result = update(&mut arena, actor, 0, &mut rng);
        if kind.is_none() {
            assert!(matches!(
                result,
                Err(Selector12PointUpdateError::RequiredField {
                    offset: 0x148,
                    address: 0x4675a3,
                    ..
                })
            ));
        } else {
            result.unwrap();
            let expected = if kind == Some(8) {
                [1388, 22, 240]
            } else {
                [91, 92, 93]
            };
            assert_eq!(
                arena.native_target(current).unwrap().raw_focus_08_words(),
                Some(expected)
            );
        }
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[2],
            Some(1019)
        );
        assert_eq!(rng, 0x096ea295);
    }
}
