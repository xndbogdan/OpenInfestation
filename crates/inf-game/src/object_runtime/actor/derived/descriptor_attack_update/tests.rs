use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
    ActorServiceResourceState, ActorTerrainInputs, DerivedActorConstructionRequest,
    FamilyAttackHost, FamilyHelper482ff0, FamilyPointTargetChangeKind,
    FamilyPointTargetChangeRequest, FixedPositionTarget, FixtureActorServiceAllocation,
    FollowTargetPoint, FollowTargetRequest, NativeTargetClass, TargetNullHandleSource,
    WeightedTargetConstructorNull,
};

const SINE: [u16; 4096] = [0; 4096];

pub(super) fn construction() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([100, 200, 300]),
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

fn arena() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_family428650_actor(construction(), &mut 0)
        .unwrap();
    (arena, actor)
}

fn fixed(handle: u32, point: [u16; 3]) -> NativeTarget {
    NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(handle),
        focus_08: Some(point),
        word_10: 8,
    })
}

fn initialize(arena: &mut ObjectArena, actor: ObjectId, state150: u32, distance: u32) {
    let target = arena.import_fixture_target(fixed(1, [100, 200, 300]));
    let state = arena.derived_actor_mut(actor).unwrap();
    state.target_148 = Some(target);
    state.word_150 = Some(state150);
    state.word_14c = Some(distance);
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(0);
}

fn update_request(delta: u32) -> FamilyAttackUpdateRequest {
    FamilyAttackUpdateRequest {
        microseconds: delta,
        settings_7c: 0,
    }
}

#[derive(Default)]
struct Host {
    missing_process_null: bool,
    records: Vec<FamilyHelperRecordRequest>,
    notify_error: bool,
    flags_after_notify: Option<u16>,
}
impl TargetNullHandleSource for Host {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        (!self.missing_process_null).then_some(ActorHandle::from_raw(0))
    }
}
impl FamilyAttackUpdateHost for Host {
    type Error = &'static str;
    fn record_attack_helper(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyHelperRecordRequest,
    ) -> Result<(), Self::Error> {
        self.records.push(request);
        if let Some(flags) = self.flags_after_notify {
            let family = arena.family428650_actor_mut(actor).unwrap();
            family.word_22e = flags;
            family.helper_218 = None;
        }
        if self.notify_error {
            Err("notify")
        } else {
            Ok(())
        }
    }
}
impl FamilyAttackHost for Host {
    type Error = &'static str;
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn attack_sound(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u16,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn first_attack_entry_keeps_constructor_unknown_until_the_later_movement_writer() {
    let (mut arena, actor) = arena();
    let selected = arena.import_fixture_target(fixed(7, [1, 2, 3]));
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(selected);
    arena.derived_actor_mut(actor).unwrap().sound_words_17c_182 = Some([0; 4]);
    let mut host = Host::default();
    arena.change_family_attack(actor, true, &mut host).unwrap();
    let current = arena.derived_actor(actor).unwrap().target_148.unwrap();
    assert_ne!(current, selected);
    // Fixed-position clone 00402730 constructs a fresh process-null handle;
    // its copied focus survives, not the source allocation's handle field.
    assert_eq!(arena.native_target(current).unwrap().handle_04().raw(), 0);
    assert_eq!(
        arena.native_target(current).unwrap().raw_focus_08_words(),
        Some([1, 2, 3])
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, None);
    let timer = arena.family428650_actor(actor).unwrap().words_188_19c[1];
    assert_eq!(timer, Some(2_000_000));
    let mut rng = 0x1234_5678;
    // 004297FF entry is followed by 00429876 update within the same +D8.
    // The outer selector12 target refresh/store at 00467A88/00467A91 has
    // not run yet; entry must not synthesize its eventual target status.
    assert_eq!(
        arena.update_family_attack(actor, update_request(33_333), &mut rng, &mut host),
        Err(FamilyAttackUpdateError::Uninitialized {
            actor,
            offset: 0x150
        })
    );
    assert_eq!(rng, 0x1234_5678);
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_148,
        Some(current)
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1],
        timer
    );
    assert!(host.records.is_empty());
}

#[test]
fn constructor_unknowns_null_flag_and_lazy_timer_reads() {
    let (mut arena, actor) = arena();
    let state = arena.derived_actor(actor).unwrap();
    assert_eq!((state.word_14c, state.word_150), (None, None));
    assert_eq!(state.target_148, None);
    let mut rng = 7;
    let mut host = Host::default();
    arena
        .update_family_attack(actor, update_request(1), &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x10);
    assert_eq!(rng, 7);
    assert_eq!(arena.actor_service_heap().attempts().len(), 0);
    initialize(&mut arena, actor, 4, 3000);
    arena.derived_actor_mut(actor).unwrap().word_150 = None;
    assert_eq!(
        arena.update_family_attack(actor, update_request(1), &mut rng, &mut host),
        Err(FamilyAttackUpdateError::Uninitialized {
            actor,
            offset: 0x150
        })
    );
    let mut expected_rng = 7;
    name_entry_rng_next(&mut expected_rng);
    assert_eq!(rng, expected_rng); // expired timer bypassed unknown150 until after draw
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(10);
    let old_rng = rng;
    assert_eq!(
        arena.update_family_attack(actor, update_request(1), &mut rng, &mut host),
        Err(FamilyAttackUpdateError::Uninitialized {
            actor,
            offset: 0x150
        })
    );
    assert_eq!(rng, old_rng);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1],
        Some(10)
    );
}

#[test]
fn equal_handles_bypass_refresh_and_focus_but_reach_later_unknown_words() {
    for missing in [0x150, 0x14c] {
        let (mut arena, actor) = arena();
        initialize(&mut arena, actor, 3, 3000);
        let mut current = fixed(1, [0; 3]);
        if let NativeTarget::FixedPosition(target) = &mut current {
            target.focus_08 = None;
        }
        let current_ref = arena.import_fixture_target(current);
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(current_ref);
        let mut selected = current;
        if let NativeTarget::FixedPosition(target) = &mut selected {
            target.word_10 = u32::MAX;
        }
        let selected_ref = arena.import_fixture_target(selected);
        arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(selected_ref);
        let state = arena.derived_actor_mut(actor).unwrap();
        if missing == 0x150 {
            state.word_150 = None;
        } else {
            state.word_14c = None;
        }
        arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa510;
        let mut rng = 7;
        let mut expected_rng = 7;
        name_entry_rng_next(&mut expected_rng);
        name_entry_rng_next(&mut expected_rng);
        let mut host = Host::default();
        assert_eq!(
            arena.update_family_attack(actor, update_request(1), &mut rng, &mut host),
            Err(FamilyAttackUpdateError::Uninitialized {
                actor,
                offset: missing
            })
        );
        assert_eq!(rng, expected_rng);
        assert_eq!(
            arena.derived_actor(actor).unwrap().target_148,
            Some(current_ref)
        );
        assert_eq!(arena.actor_service_heap().attempts().len(), 3);
        assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0xa510);
    }
}

#[test]
fn released_presence_bypasses_dereference_until_reached_and_unselected_source_survives() {
    let (mut arena, actor) = arena();
    initialize(&mut arena, actor, 4, 3000);
    let released = arena.derived_actor(actor).unwrap().target_148.unwrap();
    arena.release_actor_service(released).unwrap();
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(10);
    let mut rng = 7;
    let mut host = Host::default();
    arena
        .update_family_attack(actor, update_request(1), &mut rng, &mut host)
        .unwrap();
    assert_eq!(rng, 7);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0);
    // An expired timer with no sources also does not dereference current148.
    arena
        .update_family_attack(actor, update_request(10), &mut rng, &mut host)
        .unwrap();
    let selected = arena.import_fixture_target(fixed(1, [1, 2, 3]));
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(selected);
    assert_eq!(
        arena.update_family_attack(actor, update_request(u32::MAX >> 1), &mut rng, &mut host),
        Err(FamilyAttackUpdateError::Object(
            ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(released))
        ))
    );
    let current = arena.import_fixture_target(fixed(1, [100, 200, 300]));
    let unused = arena.import_fixture_target(fixed(9, [0; 3]));
    arena.release_actor_service(unused).unwrap();
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(current);
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.targets_1d4_1fc[4] = Some(unused);
    family.words_1d8_200[0] = u32::MAX;
    family.words_1d8_200[4] = 1;
    arena
        .update_family_attack(actor, update_request(i32::MAX as u32), &mut rng, &mut host)
        .unwrap();
    assert_eq!(
        arena.actor_service_heap().state(unused).unwrap(),
        ActorServiceResourceState::Released
    );
}

#[test]
fn release_before_clone_distinguishes_null_allocation_and_missing_constructor() {
    for allocation in [
        FixtureActorServiceAllocation::Error(71),
        FixtureActorServiceAllocation::Null,
        FixtureActorServiceAllocation::Allocate,
    ] {
        let (mut arena, actor) = arena();
        initialize(&mut arena, actor, 2, 3000);
        let old = arena.derived_actor(actor).unwrap().target_148.unwrap();
        let selected = arena.import_fixture_target(fixed(2, [9, 8, 7]));
        arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(selected);
        let before = arena.actor_service_heap().attempts().len();
        arena.actor_service_heap_mut().script([allocation]);
        let mut host = Host {
            missing_process_null: allocation == FixtureActorServiceAllocation::Allocate,
            ..Default::default()
        };
        let result = arena.update_family_attack(actor, update_request(1), &mut 7, &mut host);
        assert_eq!(
            arena.actor_service_heap().state(old).unwrap(),
            ActorServiceResourceState::Released
        );
        if allocation == FixtureActorServiceAllocation::Null {
            assert_eq!(result, Ok(()));
            assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
        } else {
            assert!(result.is_err());
            assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(old));
        }
        assert_eq!(
            &arena.actor_service_heap().attempts()[before..],
            &[crate::object_runtime::ActorServiceResourceKind::Target(
                NativeTargetClass::FixedPosition
            )]
        );
    }
}

#[test]
fn live_follow_refresh_commits_cache_even_when_lifecycle_bypasses_retarget() {
    let (mut arena, actor) = arena();
    initialize(&mut arena, actor, 3, 3000);
    let referent = arena.allocate_actor(construction().actor).unwrap();
    let handle = arena.actor(referent).unwrap().handle;
    let target = NativeTarget::ActorFollow(
        arena
            .follow_target(FollowTargetRequest {
                handle,
                point: FollowTargetPoint::ExplicitPoint(Some([7, 8, 9])),
            })
            .unwrap(),
    );
    let live = arena.actor_mut(referent).unwrap();
    live.word_74 = 31;
    live.words_8e_92 = [1, 32768, 65535];
    live.word_c4 = 1;
    arena.owned_mut(referent).unwrap().state.base.flags |= 0x40000;
    let target = arena.import_fixture_target(target);
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(target);
    let mut host = Host::default();
    arena
        .update_family_attack(
            actor,
            FamilyAttackUpdateRequest {
                microseconds: 1,
                settings_7c: 0,
            },
            &mut 7,
            &mut host,
        )
        .unwrap();
    let after = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target1d4)
        .unwrap()
        .target
        .unwrap();
    let after = arena.native_target(after).unwrap();
    assert_eq!(after.raw_focus_08_words(), Some([100, 231, 300]));
    assert_eq!(after.point_words(), Some([7, 8, 9]));
    assert_eq!(after.velocity_words(None).unwrap(), Some([65535, 32768, 1]));
    assert_eq!(arena.actor_service_heap().attempts().len(), 2);
}

#[test]
fn helper_delta_wrap_and_mutable_notification_preserve_reached_writes() {
    for fails in [false, true] {
        let (mut arena, actor) = arena();
        initialize(&mut arena, actor, 4, 10);
        let target = arena.import_fixture_target(fixed(77, [65535, 32768, 0]));
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
        arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(100);
        let mut helper = FamilyHelper482ff0::new(None);
        helper.words_00_04 = [None, Some(8_000_000)];
        arena.family428650_actor_mut(actor).unwrap().helper_218 = Some(Box::new(helper));
        let mut host = Host {
            flags_after_notify: Some(0xab01),
            notify_error: fails,
            ..Default::default()
        };
        let result = arena.update_family_attack(actor, update_request(1), &mut 7, &mut host);
        assert_eq!(
            host.records,
            vec![FamilyHelperRecordRequest {
                handle: ActorHandle::from_raw(77),
                displacement_words: [101, 32968, 300],
                native_weight: 90,
            }]
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_22e,
            if fails { 0xab01 } else { 0xab0d }
        );
        assert_eq!(
            result,
            if fails {
                Err(FamilyAttackUpdateError::Helper("notify"))
            } else {
                Ok(())
            }
        );
    }
    let (mut arena, actor) = arena();
    initialize(&mut arena, actor, 4, 10);
    let mut helper = FamilyHelper482ff0::new(None);
    helper.words_00_04[1] = Some(0x7fff_ffff);
    arena.family428650_actor_mut(actor).unwrap().helper_218 = Some(Box::new(helper));
    arena
        .update_family_attack(actor, update_request(1), &mut 7, &mut Host::default())
        .unwrap();
    assert_eq!(
        arena
            .family428650_actor(actor)
            .unwrap()
            .helper_218
            .as_ref()
            .unwrap()
            .words_00_04[1],
        Some(0x8000_0000)
    );
}

#[test]
fn current_clear_destroy_and_entry_publication_observe_released_custody() {
    let (mut arena, actor) = arena();
    initialize(&mut arena, actor, 4, 10);
    let source = arena.import_fixture_target(fixed(8, [1, 2, 3]));
    let secondary = arena.import_fixture_target(fixed(7, [1, 2, 3]));
    let current = arena.derived_actor(actor).unwrap().target_148.unwrap();
    arena.release_actor_service(current).unwrap();
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.targets_1d4_1fc[5] = Some(source);
    family.helper_218 = Some(Box::new(FamilyHelper482ff0::new(None)));
    arena.derived_actor_mut(actor).unwrap().target_154 = Some(secondary);
    let released_error = ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(current));
    assert_eq!(
        arena.clear_family_current_targets(actor),
        Err(released_error.clone())
    );
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_154,
        Some(secondary)
    );
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(released_error.clone())
    );
    assert_eq!(arena.family428650_actor(actor).unwrap().helper_218, None);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().targets_1d4_1fc[5],
        Some(source)
    );
    assert_eq!(
        arena.actor_service_heap().state(source).unwrap(),
        ActorServiceResourceState::Released
    );
    assert_eq!(
        arena.update_family_stay_still_target(actor),
        Err(released_error)
    );
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    assert!(arena
        .change_family_point_target(
            actor,
            FamilyPointTargetChangeRequest {
                kind: FamilyPointTargetChangeKind::StayStill,
                entering: true
            },
            WeightedTargetConstructorNull::ActorRegistry
        )
        .is_err());
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_148,
        Some(current)
    );
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    arena
        .change_family_point_target(
            actor,
            FamilyPointTargetChangeRequest {
                kind: FamilyPointTargetChangeKind::StayStill,
                entering: true,
            },
            WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(source)
        ))
    );
}

#[test]
fn attack_entry_only_resets_current_custody_at_completed_clone_publication() {
    let (mut arena, actor) = arena();
    let current = arena.import_fixture_target(fixed(1, [0; 3]));
    arena.release_actor_service(current).unwrap();
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(current);
    let source = arena.import_fixture_target(fixed(7, [1, 2, 3]));
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(source);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    let mut host = Host::default();
    assert!(arena.change_family_attack(actor, true, &mut host).is_err());
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_148,
        Some(current)
    );
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    assert!(
        matches!(arena.change_family_attack(actor, true, &mut host), Err(crate::object_runtime::FamilyAttackError::MissingClone(id)) if id == actor)
    );
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
}
