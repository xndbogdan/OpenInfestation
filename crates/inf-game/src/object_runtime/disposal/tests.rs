use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorServiceResourceState, ActorTerrainInputs, DerivedActorConstructionRequest,
    FamilyTargetSlot, ObjectClassKind, ObjectDestructionRequest, ObjectNodeKind,
    Selector12GrantContext, WeightedTargetConstructorNull, WeightedTargetReferent,
    WeightedTargetRequest,
};
use crate::process_particles::factories::ParticleFactoryProcessState;

fn descriptor(arena: &mut ObjectArena, actor: ObjectId, index: i32) {
    arena
        .with_supplied_family_dispatch_actor(actor, |state| {
            let crate::object_runtime::ActorExtension::Derived(derived) = &mut state.extension
            else {
                panic!("derived constructor")
            };
            let crate::object_runtime::DerivedActorClass::Family428650(family) = &mut derived.class
            else {
                panic!("family constructor")
            };
            family.word_184 = index;
        })
        .unwrap();
}

fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([1, 2, 3]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &[0; 4096],
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: 0,
                    records: vec![],
                },
                first_player_grants: None,
            },
        )
        .unwrap();
    descriptor(&mut arena, actor, 1);
    arena.actor_mut(actor).unwrap().byte_78 = 3;
    (arena, actor)
}

fn drain(
    arena: &mut ObjectArena,
    mission: &mut WorldMission,
    state: &mut ObjectDisposalState,
    policy: ObjectDisposalFailurePolicy,
) -> Result<ObjectDisposalReport, ObjectDisposalError<WorldCleanupError>> {
    let process = ParticleFactoryProcessState::native_startup();
    arena.drain_object_disposal(ObjectDisposalRequest {
        mission,
        grants: &LiveGrantContext {
            process: Some(&process),
            executable: None,
            resident: None,
            text_data: None,
        },
        sound: SoundTeardownService::RequiredAtReach,
        listeners: None,
        state,
        failure_policy: policy,
    })
}

#[test]
fn disposal_records_actual_placement_death_and_releases_actor_handle_and_nodes() {
    for inhibit in [0, 7] {
        let (mut arena, actor) = fixture();
        let handle = arena.actor(actor).unwrap().handle;
        arena.dispose(actor).unwrap();
        let stopped_child = arena.allocate(ObjectClassKind::Base);
        arena.stop_object_construction(stopped_child).unwrap();
        arena.dispose(stopped_child).unwrap();
        let mut mission = WorldMission::native_constructor();
        mission.inhibit_byte_08 = inhibit;
        let completion_before = mission.save.owner_240[4];
        let mut state = ObjectDisposalState::default();
        let report = drain(
            &mut arena,
            &mut mission,
            &mut state,
            ObjectDisposalFailurePolicy::ReturnFirstError,
        )
        .unwrap();
        assert_eq!(report.destroyed, [actor, stopped_child]);
        assert!(report.newly_stopped.is_empty());
        assert!(state.stopped().is_empty());
        assert_eq!(mission.inhibit_byte_08, inhibit);
        assert_eq!(
            mission.save.owner_240[4],
            completion_before | if inhibit == 0 { 1 << 3 } else { 0 }
        );
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
        assert!(arena.actor_list().is_empty());
        assert!(arena.lists.iter().all(Vec::is_empty));
        assert!(arena.is_empty());
        assert!(arena.object(actor).is_err());
        assert!(arena.object(stopped_child).is_err());
    }
}

#[test]
fn destructor_release_failure_is_latched_after_real_prefix_and_healthy_disposal_continues() {
    let (mut arena, actor) = fixture();
    descriptor(&mut arena, actor, 7);
    arena.selector12_actor_mut(actor).unwrap().byte_29c = 7;
    for slot in FamilyTargetSlot::SELECTOR12 {
        arena
            .publish_family_target(
                actor,
                slot,
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                    referent: WeightedTargetReferent::Actor(Some(actor)),
                    score: 17,
                },
            )
            .unwrap();
    }
    let targets = arena.selector12_actor(actor).unwrap().targets_26c_284;
    arena.release_actor_service(targets[1].unwrap()).unwrap();
    arena.dispose(actor).unwrap();
    let healthy = arena.allocate(ObjectClassKind::Base);
    arena.dispose(healthy).unwrap();
    let memberships = arena.owned(actor).unwrap().memberships;
    let mut mission = WorldMission::native_constructor();
    let mut state = ObjectDisposalState::default();
    let report = drain(
        &mut arena,
        &mut mission,
        &mut state,
        ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation,
    )
    .unwrap();
    assert_eq!(report.destroyed, [healthy]);
    assert_eq!(report.newly_stopped.len(), 1);
    assert_eq!(report.newly_stopped[0].object, actor);
    assert_eq!(
        report.newly_stopped[0].phase,
        ObjectDisposalPhase::DeletingDestructor
    );
    assert_eq!(arena.family428650_actor(actor).unwrap().word_184, -1);
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29c, 0);
    assert_eq!(arena.owned(actor).unwrap().memberships, memberships);
    assert_eq!(
        arena.selector12_actor(actor).unwrap().targets_26c_284,
        targets
    );
    for reference in targets[1..].iter().flatten() {
        assert_eq!(
            arena.actor_service_heap().state(*reference),
            Ok(ActorServiceResourceState::Released)
        );
    }
    assert!(matches!(
        arena.actor_service_heap().state(targets[0].unwrap()),
        Ok(ActorServiceResourceState::Target(_))
    ));
    let before = format!("{arena:?}");
    assert!(matches!(
        drain(&mut arena, &mut mission, &mut state, ObjectDisposalFailurePolicy::ReturnFirstError),
        Err(ObjectDisposalError::PreviouslyStopped(stop)) if stop.object == actor
    ));
    assert_eq!(format!("{arena:?}"), before);
    let report = drain(
        &mut arena,
        &mut mission,
        &mut state,
        ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation,
    )
    .unwrap();
    assert_eq!(report, ObjectDisposalReport::default());
    assert_eq!(format!("{arena:?}"), before);
    // Arena custody survives replacement of the frame's diagnostic ledger.
    assert!(matches!(
        drain(&mut arena, &mut mission, &mut ObjectDisposalState::default(), ObjectDisposalFailurePolicy::ReturnFirstError),
        Err(ObjectDisposalError::PreviouslyStopped(stop)) if stop.object == actor
    ));
    assert_eq!(
        arena.destroy(ObjectDestructionRequest {
            object: actor,
            sound: SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::StoppedDisposal {
            object: actor,
            phase: ObjectDisposalPhase::DeletingDestructor
        })
    );
    // Even absent process inputs cannot mask the earlier disposal stop: no
    // cleanup callback, descriptor exit or release is entered a second time.
    let error = arena
        .cleanup_world(crate::object_runtime::WorldCleanupRequest {
            mission: &mut mission,
            grants: &LiveGrantContext {
                process: None,
                executable: None,
                resident: None,
                text_data: None,
            },
            sound: SoundTeardownService::RequiredAtReach,
            listeners: &mut crate::object_runtime::CreationListeners::new(),
            furniture: None,
        })
        .unwrap_err();
    assert!(
        matches!(error, WorldCleanupError::Object(ObjectError::StoppedDisposal { object, phase: ObjectDisposalPhase::DeletingDestructor }) if object == actor)
    );
    assert_eq!(format!("{arena:?}"), before);
}

struct StopCallback {
    calls: Vec<(ObjectDisposalPhase, ObjectId)>,
    stopped: ObjectId,
}

impl DisposalDispatch for StopCallback {
    type Error = &'static str;

    fn callback(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        self.calls
            .push((ObjectDisposalPhase::CleanupCallback, object));
        if object == self.stopped {
            arena.owned_mut(object).unwrap().state.base.word_3c = 0x1234;
            return Err("supplied callback failed after write");
        }
        Ok(())
    }

    fn destroy(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        self.calls
            .push((ObjectDisposalPhase::DeletingDestructor, object));
        arena
            .destroy(ObjectDestructionRequest {
                object,
                sound: SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        Ok(())
    }
}

#[test]
fn strict_failure_latches_callback_prefix_without_mutating_membership_or_replaying() {
    let mut arena = ObjectArena::new();
    let stopped = arena.allocate(ObjectClassKind::Base);
    let healthy = arena.allocate(ObjectClassKind::Base);
    for object in [stopped, healthy] {
        arena.dispose(object).unwrap();
    }
    let membership = arena.owned(stopped).unwrap().memberships;
    let mut dispatch = StopCallback {
        calls: vec![],
        stopped,
    };
    let mut state = ObjectDisposalState::default();
    assert!(
        matches!(arena.drain_disposal_with(&mut dispatch, &mut state, ObjectDisposalFailurePolicy::ReturnFirstError),
        Err(ObjectDisposalError::Failed { object, phase: ObjectDisposalPhase::CleanupCallback, error: "supplied callback failed after write" }) if object == stopped)
    );
    assert_eq!(arena.object(stopped).unwrap().base.word_3c, 0x1234);
    assert_eq!(arena.owned(stopped).unwrap().memberships, membership);
    assert_eq!(
        dispatch.calls,
        [(ObjectDisposalPhase::CleanupCallback, stopped)]
    );
    let report = arena
        .drain_disposal_with(
            &mut dispatch,
            &mut state,
            ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation,
        )
        .unwrap();
    assert_eq!(report.destroyed, [healthy]);
    assert_eq!(
        dispatch.calls,
        [
            (ObjectDisposalPhase::CleanupCallback, stopped),
            (ObjectDisposalPhase::CleanupCallback, healthy),
            (ObjectDisposalPhase::DeletingDestructor, healthy),
        ]
    );
    assert_eq!(arena.list(ObjectList::Disposal).len(), 1);
    assert_eq!(
        arena.destroy(ObjectDestructionRequest {
            object: stopped,
            sound: SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::StoppedDisposal {
            object: stopped,
            phase: ObjectDisposalPhase::CleanupCallback
        })
    );
}

struct MovingCallback {
    first: ObjectId,
    new_head: ObjectId,
    events: Vec<(ObjectDisposalPhase, ObjectId)>,
}

impl DisposalDispatch for MovingCallback {
    type Error = &'static str;

    fn callback(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        self.events
            .push((ObjectDisposalPhase::CleanupCallback, object));
        if object == self.first {
            arena.move_node(object, ObjectNodeKind::Lifecycle, ObjectList::Parking);
            arena.dispose(self.new_head).unwrap();
            let physical = arena.list_owners[ObjectList::Disposal as usize];
            arena.lists[physical].rotate_right(1); // Supplied native lower-call head mutation.
        }
        Ok(())
    }

    fn destroy(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        self.events
            .push((ObjectDisposalPhase::DeletingDestructor, object));
        arena
            .destroy(ObjectDestructionRequest {
                object,
                sound: SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        Ok(())
    }
}

#[test]
fn disposal_keeps_captured_object_after_callback_moves_it_and_then_reloads_head() {
    let mut arena = ObjectArena::new();
    let first = arena.allocate(ObjectClassKind::Base);
    let last = arena.allocate(ObjectClassKind::Base);
    let new_head = arena.allocate(ObjectClassKind::Base);
    for object in [first, last] {
        arena.dispose(object).unwrap();
    }
    let mut dispatch = MovingCallback {
        first,
        new_head,
        events: vec![],
    };
    let report = arena
        .drain_disposal_with(
            &mut dispatch,
            &mut ObjectDisposalState::default(),
            ObjectDisposalFailurePolicy::ReturnFirstError,
        )
        .unwrap();
    assert_eq!(report.destroyed, [first, new_head, last]);
    assert_eq!(
        dispatch.events,
        [first, new_head, last]
            .into_iter()
            .flat_map(|id| [
                (ObjectDisposalPhase::CleanupCallback, id),
                (ObjectDisposalPhase::DeletingDestructor, id)
            ])
            .collect::<Vec<_>>()
    );
    assert!(arena.is_empty());
}

#[test]
fn load_actor_services_remain_lazy_after_nonactor_disposal() {
    let (mut arena, actor) = fixture();
    let base = arena.allocate(ObjectClassKind::Base);
    arena.dispose(base).unwrap();
    arena.dispose(actor).unwrap();
    let before = arena.object(actor).unwrap().clone();
    let error = arena
        .finish_world_placements(
            crate::object_runtime::ObjectAfterLoadInputs::default(),
            None,
            None,
        )
        .unwrap_err();
    assert!(
        matches!(error, crate::object_runtime::WorldLoadError::Object(ObjectError::ActorDisposalBoundary(id)) if id == actor)
    );
    assert!(arena.object(base).is_err());
    assert_eq!(arena.object(actor).unwrap(), &before);
    assert!(
        matches!(arena.finish_world_placements(crate::object_runtime::ObjectAfterLoadInputs::default(), None, None), Err(crate::object_runtime::WorldLoadError::Object(ObjectError::StoppedDisposal { object, phase: ObjectDisposalPhase::CleanupCallback })) if object == actor)
    );
}

struct DestroyThenFail {
    fail: ObjectId,
    replacement: Option<ObjectId>,
}

impl DisposalDispatch for DestroyThenFail {
    type Error = &'static str;

    fn callback(&mut self, _arena: &mut ObjectArena, _object: ObjectId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn destroy(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        arena
            .destroy(ObjectDestructionRequest {
                object,
                sound: SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        if object == self.fail {
            let replacement = arena.allocate(ObjectClassKind::Base);
            assert_eq!(replacement.slot, object.slot);
            arena.dispose(replacement).unwrap();
            self.replacement = Some(replacement);
            return Err("supplied destructor failed after allocation release and reuse");
        }
        Ok(())
    }
}

#[test]
fn post_release_failure_never_marks_the_new_generation_in_the_reused_slot() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Base);
    arena.dispose(object).unwrap();
    let mut dispatch = DestroyThenFail {
        fail: object,
        replacement: None,
    };
    let mut state = ObjectDisposalState::default();
    let report = arena
        .drain_disposal_with(
            &mut dispatch,
            &mut state,
            ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation,
        )
        .unwrap();
    assert_eq!(report.newly_stopped.len(), 1);
    assert_eq!(report.newly_stopped[0].object, object);
    assert_eq!(report.destroyed, [dispatch.replacement.unwrap()]);
    assert!(arena.is_empty());
}

/// The burst and radius a Dormant ship's exit reaches, in order.
#[derive(Default)]
struct ExitEffects(Vec<crate::object_runtime::Selector35LifecycleEffect>);

impl crate::object_runtime::TargetNullHandleSource for ExitEffects {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        None
    }
}

impl crate::object_runtime::Selector35LifecycleHost for ExitEffects {
    type Error = std::convert::Infallible;
    fn particle_load_factor(&mut self) -> Result<u32, Self::Error> {
        unreachable!("0043E120 reads no load factor")
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: crate::object_runtime::Selector35LifecycleEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.0.push(effect);
        Ok(())
    }
}

fn dormant_ship() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let ship = arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([1, 2, 3]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &[0; 4096],
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    arena.place_for_tests(ship, [100, 200, 300]);
    descriptor(&mut arena, ship, 0);
    arena.dispose(ship).unwrap();
    (arena, ship)
}

/// A ship disposed while Dormant: the drain pauses after its +30 for the
/// exit 0043E120, which needs the world's burst and radius, and the next
/// drain runs its destructor without a second +30.
#[test]
fn a_dormant_ship_pauses_the_drain_for_its_exit() {
    let (mut arena, ship) = dormant_ship();
    let mut mission = WorldMission::native_constructor();
    let mut state = ObjectDisposalState::default();
    let policy = ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation;
    let report = drain(&mut arena, &mut mission, &mut state, policy).unwrap();
    assert_eq!(report.exit_pending, Some(ship));
    assert!(report.destroyed.is_empty() && report.newly_stopped.is_empty());
    let mut effects = ExitEffects::default();
    arena
        .exit_selector35_dormant_for_destruction(ship, &mut effects, &mut 0)
        .unwrap();
    assert_eq!(effects.0.len(), 2);
    let report = drain(&mut arena, &mut mission, &mut state, policy).unwrap();
    assert_eq!(report.destroyed, [ship]);
    assert_eq!(report.exit_pending, None);
    assert!(arena.is_empty());
}

/// Without its exit the resumed destructor keeps its typed boundary: the
/// ship stops there, retained, and the drain does not pause again.
#[test]
fn a_dormant_ship_without_its_exit_stops_at_its_destructor() {
    let (mut arena, ship) = dormant_ship();
    let mut mission = WorldMission::native_constructor();
    let mut state = ObjectDisposalState::default();
    let policy = ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation;
    assert_eq!(
        drain(&mut arena, &mut mission, &mut state, policy)
            .unwrap()
            .exit_pending,
        Some(ship)
    );
    let report = drain(&mut arena, &mut mission, &mut state, policy).unwrap();
    assert_eq!(report.exit_pending, None);
    let [stop] = &report.newly_stopped[..] else {
        panic!("one stop: {report:?}");
    };
    assert_eq!(
        (stop.object, stop.phase),
        (ship, ObjectDisposalPhase::DeletingDestructor)
    );
    assert!(arena.object(ship).is_ok());
}
