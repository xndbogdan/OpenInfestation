//! Actual manager and selector2 consumers with real queue/drain custody.
//! The recording emitter is an explicit 0048C890 service boundary. It does
//! not replace production particle/audio acceptance or the native fixtures.

use super::*;
use crate::object_runtime::{
    ActorServiceResourceState, ActorWeaponFireContext, NativeHeapRef, ProcessActorBehaviorTables,
    WeaponEmissionRequest, WeaponFiringContext, WeaponFiringEmitter,
};
use inf_formats::parse_vehicle_behavior_table;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum FiringEvent {
    Drain {
        actor: ObjectId,
        pending: bool,
    },
    Emit {
        node: NativeHeapRef,
        position: [u16; 3],
        pending: bool,
    },
}

struct RecordingEmitter {
    trace: Trace,
    calls: usize,
    fail_at: Option<usize>,
}

impl WeaponFiringEmitter for RecordingEmitter {
    type Error = &'static str;

    fn emit(
        &mut self,
        arena: &mut ObjectArena,
        _: &mut WeaponFiringContext<'_>,
        request: WeaponEmissionRequest,
    ) -> Result<(), Self::Error> {
        assert_eq!(head(arena, request.actor), Some(request.node));
        self.trace
            .borrow_mut()
            .push(Event::Firing(FiringEvent::Emit {
                node: request.node,
                position: request.position,
                pending: pending(arena, request.actor),
            }));
        self.calls += 1;
        if self.fail_at == Some(self.calls) {
            // A reached emitter can mutate state before a later service fails.
            // This write must survive containment and must never be replayed.
            arena
                .derived_actor_mut(request.actor)
                .unwrap()
                .behavior_140
                .as_mut()
                .unwrap()
                .word_38 = 500_000;
            return Err("supplied emitter failure after effect prefix");
        }
        Ok(())
    }
}

pub(super) struct FiringHost {
    tables: ProcessActorBehaviorTables,
    rng: u32,
    emitter: RecordingEmitter,
    capture_on_model_return: bool,
}

impl FiringHost {
    fn new(executable: FrontendExecutable<'_>, trace: &Trace) -> Self {
        Self {
            tables: ProcessActorBehaviorTables::from_executable(executable).unwrap(),
            rng: 0x1234_5678,
            emitter: RecordingEmitter {
                trace: Rc::clone(trace),
                calls: 0,
                fail_at: None,
            },
            capture_on_model_return: false,
        }
    }

    pub(super) fn model_return(&mut self, arena: &mut ObjectArena, actor: ObjectId) {
        if !self.capture_on_model_return {
            return;
        }
        let Some(behavior) = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
        else {
            return;
        };
        // Supplied current model-callback outputs, on the same retained owner
        // consumed by the real drain. Native 0048CF70 has separate tests.
        for (index, group) in behavior.groups_1c.iter_mut().enumerate() {
            for axis in 0..3 {
                group.coordinates_00_0a[axis + 3] = Some((1000 * (axis + 1) + index) as u16);
            }
            group.word_0c |= 2;
        }
    }

    pub(super) fn drain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<usize, ObjectDrawServiceError<&'static str>> {
        self.emitter
            .trace
            .borrow_mut()
            .push(Event::Firing(FiringEvent::Drain {
                actor,
                pending: pending(arena, actor),
            }));
        arena
            .drain_actor_firing(
                actor,
                &mut WeaponFiringContext {
                    tables: &mut self.tables,
                    rng: &mut self.rng,
                },
                &mut self.emitter,
            )
            .map(|summary| summary.emitted)
            .map_err(|error| {
                ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::WeaponFire {
                    reason: error.to_string(),
                })
            })
    }
}

fn head(arena: &ObjectArena, actor: ObjectId) -> Option<NativeHeapRef> {
    arena
        .derived_actor(actor)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .fire_head_00
}

fn pending(arena: &ObjectArena, actor: ObjectId) -> bool {
    arena.object(actor).unwrap().base.flags & 0x0100_0000 != 0
}

fn late_actor(arena: &mut ObjectArena) -> ObjectId {
    let object = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: allocation(),
                world_mask_limits: [3; 2],
            },
            &mut 17,
        )
        .unwrap();
    position(arena, object, [0, 0, 100], 0);
    model_word(arena, object, 23);
    arena.derived_actor_mut(object).unwrap().byte_13d = Some(129);
    arena.admit_derived_actor(object).unwrap();
    object
}

fn direct_vehicle(
    arena: &mut ObjectArena,
    terrain: &TerrainRuntime,
    executable: FrontendExecutable<'_>,
) -> ObjectId {
    let object = vehicle(arena, terrain);
    position(arena, object, [0, 0, 10], 0);
    model_word(arena, object, 23);
    arena.actor_mut(object).unwrap().maximum_health = Some(700);
    assert!(arena
        .admit_selector2_actor(
            object,
            &parse_vehicle_behavior_table(executable).unwrap(),
            None
        )
        .unwrap());
    assert!(arena.apply_actor_view_mode(object, 1).unwrap());
    object
}

fn queue(arena: &mut ObjectArena, actor: ObjectId, firing: &mut FiringHost) -> Vec<NativeHeapRef> {
    // Genuine retained 0048D0D0 writes publish the pending flag and chain.
    arena
        .queue_actor_weapon_direction(
            actor,
            1,
            [1.0_f32.to_bits(), 0, 0],
            0,
            &mut ActorWeaponFireContext {
                tables: &mut firing.tables,
                rng: &mut firing.rng,
                process_velocity: None,
            },
        )
        .unwrap();
    let mut at = head(arena, actor);
    let mut chain = Vec::new();
    while let Some(reference) = at {
        chain.push(reference);
        at = arena
            .actor_service_heap()
            .weapon_fire_node(reference)
            .unwrap()
            .next;
    }
    assert!(
        !chain.is_empty(),
        "native queue must supply a real nonempty consumer"
    );
    chain
}

fn firing_events(trace: &Trace) -> Vec<FiringEvent> {
    trace
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::Firing(event) => Some(event.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn firing_manager_late_draw_precedes_flag_clear_and_real_chain_drain() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.far = 1;
    let value = terrain();
    let mut arena = arena();
    let object = late_actor(&mut arena);
    let (mut terrain, mut host, trace) = services(&value);
    let mut firing = FiringHost::new(fixture.executable, &trace);
    firing.capture_on_model_return = true;
    let chain = queue(&mut arena, object, &mut firing);
    assert_eq!(chain.len(), 2);
    host.firing = Some(firing);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!(
        (
            outcome.near_callbacks,
            outcome.late_models,
            outcome.firing_drains,
            outcome.emitted_shots
        ),
        (0, 1, 1, 2)
    );
    assert!(matches!(
        trace.borrow()[1],
        Event::Model(_, ModelRootMode::ManagerLate, _)
    ));
    assert_eq!(
        firing_events(&trace),
        vec![
            FiringEvent::Drain {
                actor: object,
                pending: false
            },
            FiringEvent::Emit {
                node: chain[0],
                position: [1001, 2001, 3001],
                pending: false
            },
            FiringEvent::Emit {
                node: chain[1],
                position: [1000, 2000, 3000],
                pending: false
            },
        ]
    );
    assert_eq!(head(&arena, object), None);
    for node in chain {
        assert_eq!(
            arena.actor_service_heap().state(node),
            Ok(ActorServiceResourceState::Released)
        );
    }
    trace.borrow_mut().clear();
    let next = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!(
        (next.firing_drains, next.emitted_shots, next.late_models),
        (0, 0, 0)
    );
    assert!(firing_events(&trace).is_empty());
}

#[test]
fn firing_manager_failed_second_emission_retains_head_and_completed_prefix_without_replay() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.far = 1;
    let value = terrain();
    let mut arena = arena();
    let object = late_actor(&mut arena);
    let (mut terrain, mut host, trace) = services(&value);
    let mut firing = FiringHost::new(fixture.executable, &trace);
    firing.emitter.fail_at = Some(2);
    let chain = queue(&mut arena, object, &mut firing);
    host.firing = Some(firing);
    let policy = Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation;
    let outcome = fixture
        .run(&mut arena, &mut terrain, &mut host, policy)
        .unwrap();
    assert_eq!(outcome.omissions.len(), 1);
    assert_eq!(outcome.omissions[0].object, object);
    assert!(!pending(&arena, object));
    assert_eq!(head(&arena, object), Some(chain[1]));
    assert_eq!(
        arena.actor_service_heap().state(chain[0]),
        Ok(ActorServiceResourceState::Released)
    );
    let retained = arena
        .actor_service_heap()
        .weapon_fire_node(chain[1])
        .unwrap();
    assert_eq!(
        arena
            .derived_actor(object)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .word_38,
        500_000
    );
    fixture
        .run(&mut arena, &mut terrain, &mut host, policy)
        .unwrap();
    assert_eq!(host.firing.as_ref().unwrap().emitter.calls, 2);
    // A newly queued request can set this flag again. The stopped node still
    // cannot replay its partially completed emitter on a later draw visit.
    arena.owned_mut(object).unwrap().state.base.flags |= 0x0100_0000;
    let retry = fixture
        .run(&mut arena, &mut terrain, &mut host, policy)
        .unwrap();
    assert_eq!(retry.omissions.len(), 1);
    assert_eq!(host.firing.as_ref().unwrap().emitter.calls, 2);
    assert_eq!(head(&arena, object), Some(chain[1]));
    assert_eq!(
        arena
            .actor_service_heap()
            .weapon_fire_node(chain[1])
            .unwrap(),
        retained
    );
}

#[test]
fn firing_selector2_direct_consumer_drains_with_or_without_manager_pending() {
    let raw = executable_bytes();
    for keep_pending in [false, true] {
        let mut fixture = Fixture::new(&raw);
        let value = terrain();
        let mut arena = arena();
        let object = direct_vehicle(&mut arena, &value, fixture.executable);
        let (mut terrain, mut host, trace) = services(&value);
        let mut firing = FiringHost::new(fixture.executable, &trace);
        let chain = queue(&mut arena, object, &mut firing);
        if !keep_pending {
            arena.owned_mut(object).unwrap().state.base.flags &= !0x0100_0000;
        }
        host.firing = Some(firing);
        let outcome = fixture
            .run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::Stop,
            )
            .unwrap();
        assert_eq!(
            outcome.firing_drains, 1,
            "direct42086D must run even when outer manager pending is clear"
        );
        assert_eq!(outcome.emitted_shots, chain.len());
        assert_eq!(outcome.late_models, usize::from(keep_pending));
        assert_eq!(outcome.empty_behaviors, usize::from(keep_pending));
        assert_eq!(
            firing_events(&trace)[0],
            FiringEvent::Drain {
                actor: object,
                pending: keep_pending
            }
        );
        assert!(!pending(&arena, object));
        assert_eq!(head(&arena, object), None);
        assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), Some(0));
        for node in chain {
            assert_eq!(
                arena.actor_service_heap().state(node),
                Ok(ActorServiceResourceState::Released)
            );
        }
        let calls = host.firing.as_ref().unwrap().emitter.calls;
        let next = fixture
            .run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::Stop,
            )
            .unwrap();
        assert_eq!(next.firing_drains, 0);
        assert_eq!(host.firing.as_ref().unwrap().emitter.calls, calls);
    }
}

#[test]
fn firing_selector2_failed_emission_precedes_outer_flag_clear_and_normal_tail() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    let value = terrain();
    let mut arena = arena();
    let object = direct_vehicle(&mut arena, &value, fixture.executable);
    let other = actor(&mut arena, [0, 0, 20], 24);
    let (mut terrain, mut host, trace) = services(&value);
    let mut firing = FiringHost::new(fixture.executable, &trace);
    firing.emitter.fail_at = Some(1);
    let chain = queue(&mut arena, object, &mut firing);
    host.firing = Some(firing);
    let policy = Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation;
    let outcome = fixture
        .run(&mut arena, &mut terrain, &mut host, policy)
        .unwrap();
    assert_eq!(
        (
            outcome.visited,
            outcome.firing_drains,
            outcome.omissions.len()
        ),
        (2, 1, 1)
    );
    assert_eq!(
        firing_events(&trace)[0],
        FiringEvent::Drain {
            actor: object,
            pending: true
        }
    );
    assert!(
        pending(&arena, object),
        "failure cannot fabricate outer4797EC flag clear"
    );
    assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), None);
    assert_eq!(head(&arena, object), Some(chain[0]));
    assert!(trace
        .borrow()
        .iter()
        .any(|event| matches!(event, Event::Model(packet, _, _) if packet.source == other)));
    let retry = fixture
        .run(&mut arena, &mut terrain, &mut host, policy)
        .unwrap();
    assert_eq!(retry.omissions.len(), 1);
    assert_eq!(host.firing.as_ref().unwrap().emitter.calls, 1);
    assert_eq!(head(&arena, object), Some(chain[0]));
    assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), None);
}

#[test]
fn firing_queued_model_failure_never_reaches_drain_or_pending_clear() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.far = 1;
    let value = terrain();
    let mut arena = arena();
    let object = late_actor(&mut arena);
    let (mut terrain, mut host, trace) = services(&value);
    let mut firing = FiringHost::new(fixture.executable, &trace);
    let chain = queue(&mut arena, object, &mut firing);
    host.firing = Some(firing);
    host.fail = Some((
        object,
        ObjectDrawServiceError::Backend("late model stopped"),
    ));
    assert!(matches!(
        fixture.run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop
        ),
        Err(Flag0ObjectDrawError::Host("late model stopped"))
    ));
    assert!(pending(&arena, object));
    assert_eq!(head(&arena, object), Some(chain[0]));
    assert_eq!(host.firing.as_ref().unwrap().emitter.calls, 0);
    assert!(firing_events(&trace).is_empty());
}
