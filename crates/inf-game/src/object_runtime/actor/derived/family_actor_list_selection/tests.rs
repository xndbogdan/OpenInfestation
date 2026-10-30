use std::collections::VecDeque;

use crate::frontend_camera::{AnimationClock, AnimationClockSource};

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorServiceHeapError, ActorServiceResourceKind,
    ActorServiceResourceState, ActorTerrainInputs, BehaviorComponentQuery, BehaviorCreationRequest,
    BehaviorPacketCopy, BehaviorPacketSourceRequest, BehaviorPredicateHost, BehaviorTemporary,
    DerivedActorConstructionRequest, FixtureActorServiceAllocation, FollowTargetRequest,
    NativeTarget, NativeTargetClass, ProcessActorBehaviorTables, Selector12GrantContext,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_tasks::ProcessTaskRuntime;

const NULL: ActorHandle = ActorHandle::from_raw(0x1234_5678);
const SINE: [u16; 4096] = [0; 4096];

fn request(position: Option<[u16; 3]>) -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: position,
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

fn classify(arena: &mut ObjectArena, actor: ObjectId, classification: u32) {
    let state = arena.actor_mut(actor).unwrap();
    state.word_b4 = classification;
    state.word_c4 = 2;
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.flags = (base.flags & !0xa0) | 0x20;
}

fn behavior(arena: &mut ObjectArena, actor: ObjectId) {
    arena.derived_actor_mut(actor).unwrap().behavior_140 =
        crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
}

type Effect =
    Box<dyn FnMut(&mut ProcessTaskRuntime, &mut ObjectArena, &mut BehaviorPredicateContext<'_>)>;

struct Host {
    tasks: ProcessTaskRuntime,
    creates: usize,
    accept: bool,
    fail: bool,
    effect: Option<Effect>,
    clock: Option<AnimationClock>,
    advance_clock_on_create_micros: Option<u32>,
}

impl FamilySelectionHost for Host {
    fn process_tasks(&mut self) -> &mut ProcessTaskRuntime {
        &mut self.tasks
    }

    fn animation_clock(&self) -> Option<&dyn AnimationClockSource> {
        self.clock
            .as_ref()
            .map(|clock| clock as &dyn AnimationClockSource)
    }
}

impl BehaviorPredicateHost for Host {
    type Error = &'static str;
    type Particle = u32;
    type Component = u32;

    fn create(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        _: BehaviorCreationRequest,
    ) -> Result<Option<BehaviorTemporary<u32>>, Self::Error> {
        self.creates += 1;
        if let Some(micros) = self.advance_clock_on_create_micros {
            self.clock.as_mut().unwrap().advance(micros);
        }
        if let Some(effect) = &mut self.effect {
            effect(&mut self.tasks, arena, context);
        }
        if self.fail {
            return Err("supplied creation stopped");
        }
        // Explicit lower-service input. Factory composition is checked by
        // the separate original-code receipt suite, not by this test host.
        Ok(Some(BehaviorTemporary::Particle(1)))
    }

    fn query_component(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        _: BehaviorComponentQuery<u32>,
    ) -> Result<Option<u32>, Self::Error> {
        panic!("particle-only supplied boundary")
    }

    fn packet_source(
        &mut self,
        _: &ObjectArena,
        _: &BehaviorPredicateContext<'_>,
        _: BehaviorPacketSourceRequest<u32, u32>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error> {
        Ok(Some(BehaviorPacketCopy::Full(if self.accept {
            [Some(0xff), Some(0xff), Some(0), None]
        } else {
            [Some(0), Some(0), Some(0), None]
        })))
    }

    fn delete_particle(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        _: u32,
        flags: u32,
    ) -> Result<(), Self::Error> {
        assert_eq!(flags, 1);
        Ok(())
    }
}

#[derive(Default)]
struct AllocationPlan {
    calls: Vec<NativeTargetClass>,
    outcomes: VecDeque<Result<bool, &'static str>>,
}

struct Fixture {
    arena: ObjectArena,
    host: Host,
    process: ParticleFactoryProcessState,
    tables: ProcessActorBehaviorTables,
    rng: u32,
    allocations: AllocationPlan,
}

impl Fixture {
    fn new() -> Self {
        let raw = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
        )
        .unwrap();
        let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
        let mut tasks = ProcessTaskRuntime::default();
        tasks.retain(NULL).unwrap();
        Self {
            arena: ObjectArena::with_actor_registry(ActorRegistryConfig {
                owner_count: 3,
                initial_capacity: 8,
                null_handle: NULL,
            })
            .unwrap(),
            host: Host {
                tasks,
                creates: 0,
                accept: true,
                fail: false,
                effect: None,
                clock: None,
                advance_clock_on_create_micros: None,
            },
            process: ParticleFactoryProcessState {
                process_null_handle: Some(NULL),
                network_flag: 1,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
            },
            tables: ProcessActorBehaviorTables::from_executable(executable).unwrap(),
            rng: 0,
            allocations: AllocationPlan::default(),
        }
    }

    fn family(&mut self, position: Option<[u16; 3]>, classification: u32) -> ObjectId {
        let actor = self
            .arena
            .allocate_family428650_actor(request(position), &mut self.rng)
            .unwrap();
        classify(&mut self.arena, actor, classification);
        behavior(&mut self.arena, actor);
        actor
    }

    fn derived(&mut self, position: [u16; 3], classification: u32) -> ObjectId {
        let actor = self
            .arena
            .allocate_derived_actor(request(Some(position)), &mut self.rng)
            .unwrap();
        classify(&mut self.arena, actor, classification);
        actor
    }

    fn run(
        &mut self,
        actor: ObjectId,
    ) -> Result<bool, FamilyActorListSelectionError<&'static str>> {
        // Supplied allocation outcomes are applied by the genuine arena heap.
        self.arena
            .actor_service_heap_mut()
            .script(
                self.allocations
                    .outcomes
                    .drain(..)
                    .map(|outcome| match outcome {
                        Ok(true) => FixtureActorServiceAllocation::Allocate,
                        Ok(false) => FixtureActorServiceAllocation::Null,
                        Err(_) => FixtureActorServiceAllocation::Error(71),
                    }),
            );
        let before = self.arena.actor_service_heap().attempts().len();
        let result = self.arena.select_family_actor_list_targets(
            FamilySelectionRequest { actor },
            &mut BehaviorPredicateContext {
                tables: &mut self.tables,
                process: &mut self.process,
                rng: &mut self.rng,
            },
            &mut self.host,
        );
        self.allocations.calls.extend(
            self.arena.actor_service_heap().attempts()[before..]
                .iter()
                .map(|kind| {
                    let ActorServiceResourceKind::Target(class) = *kind else {
                        panic!("selection allocates targets only")
                    };
                    class
                }),
        );
        result
    }

    fn score(&self, actor: ObjectId, slot: FamilyTargetSlot) -> u32 {
        self.arena
            .family_weighted_target(actor, slot)
            .unwrap()
            .score
    }
}

#[test]
fn task_source_precedes_null_position_and_failed_reference_keeps_reset_prefix() {
    let mut f = Fixture::new();
    let owner = f.family(None, 3);
    f.host.tasks.release().unwrap();
    f.process.process_null_handle = None;
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::Task(
            TaskStorageError::NullOwner(3)
        ))
    );
    f.host.tasks.retain(NULL).unwrap();
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::MissingProcessNull {
            address: 0x0042_8fc6
        })
    );
    f.process.process_null_handle = Some(NULL);
    let retired = f.derived([0; 3], 4);
    let handle = f.arena.actor(retired).unwrap().handle;
    f.arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: retired,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let state = f.arena.family428650_actor_mut(owner).unwrap();
    state.word_20c = handle.raw();
    state.word_210 = 77;
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::MissingPosition(owner))
    );
    let state = f.arena.family428650_actor(owner).unwrap();
    assert_eq!(state.word_20c, NULL.raw());
    assert_eq!(state.word_210, 0);
    assert!(f.allocations.calls.is_empty());
}

#[test]
fn stopped_construction_common_candidate_is_not_read_or_selected() {
    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 3);
    f.arena.derived_actor_mut(owner).unwrap().word_164 = 1000;
    let stopped = f.family(None, u32::MAX);
    f.arena.stop_object_construction(stopped).unwrap();
    let healthy = f.derived([0; 3], 4);
    let before = f.arena.object(stopped).unwrap().clone();
    let nodes = f.arena.actor_list().to_vec();

    assert!(f.run(owner).unwrap());
    assert_eq!(f.host.creates, 1);
    for slot in [FamilyTargetSlot::Target1f4, FamilyTargetSlot::Target1ec] {
        let reference = f
            .arena
            .family_weighted_target(owner, slot)
            .unwrap()
            .target
            .unwrap();
        assert_eq!(
            f.arena.native_target(reference).unwrap().handle_04(),
            f.arena.actor(healthy).unwrap().handle
        );
    }
    assert_eq!(f.arena.object(stopped).unwrap(), &before);
    assert_eq!(f.arena.actor_list(), nodes);
}

#[test]
fn self_and_inactive_cc_null_friend_contribute_signed_truncated_group_velocity() {
    let mut f = Fixture::new();
    let owner = f.family(Some([1000, 20, 30]), 3);
    let friend = f.derived([1000, 20, 30], 3);
    f.arena.actor_mut(owner).unwrap().word_c4 = i32::MIN as u32;
    f.arena.actor_mut(friend).unwrap().word_c4 = 0;
    f.arena.actor_mut(owner).unwrap().words_8e_92 = [(-601i16) as u16, 0, 0];
    f.arena.actor_mut(friend).unwrap().words_8e_92 = [(-600i16) as u16, 0, 0];
    assert!(!f.arena.intro_active_component(owner).unwrap());
    assert!(!f.arena.intro_active_component(friend).unwrap());
    let live_null = ActorHandle::from_raw(0x7654_0000);
    f.process.process_null_handle = Some(live_null);
    f.arena.family428650_actor_mut(owner).unwrap().word_20c = live_null.raw();
    assert!(f.run(owner).unwrap());
    assert_eq!(f.allocations.calls, [NativeTargetClass::FixedPosition]);
    assert_eq!(f.host.creates, 0);
    let group = f
        .arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1e4)
        .unwrap();
    assert_eq!(group.score, 600);
    let NativeTarget::FixedPosition(point) = f.arena.native_target(group.target.unwrap()).unwrap()
    else {
        panic!("group publisher must use kind8")
    };
    assert_eq!(point.focus_08, Some([400, 20, 30]));
    assert_eq!(point.handle_04, live_null);
    assert_eq!(f.arena.actor_handles().unwrap().null_handle(), NULL);
    // Excluding self leaves just one friend: the existing group is cleared
    // by the third publisher, while its zero-input bytes remain unused.
    f.arena.owned_mut(owner).unwrap().state.base.flags &= !0x20;
    assert!(!f.run(owner).unwrap());
    assert!(f
        .arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1e4)
        .unwrap()
        .target
        .is_none());
}

#[test]
fn wrapped_three_axis_range_and_signed_product_division_reach_real_gate() {
    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 3);
    let enemy = f.derived([0; 3], 4);
    f.arena.derived_actor_mut(owner).unwrap().word_164 = -1;
    assert!(f.run(owner).unwrap());
    // (-1*2)/4 truncates to zero, then OR C000 reaches the primary gate.
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 0xc000);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 0);
    assert_eq!(f.host.creates, 1);
    assert_eq!(f.allocations.calls, [NativeTargetClass::ActorFollow]);
    let first = f
        .arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1f4)
        .unwrap()
        .target
        .unwrap();
    assert_eq!(
        f.arena.native_target(first).unwrap().handle_04(),
        f.arena.actor(enemy).unwrap().handle
    );

    let mut f = Fixture::new();
    let owner = f.family(Some([0x8000, 0, 0]), 3);
    let enemy = f.derived([0x7fff, 0, 0], 4);
    f.arena.derived_actor_mut(owner).unwrap().word_164 = 2;
    assert!(f.run(owner).unwrap());
    assert_eq!(f.host.creates, 1); // signed WORD delta is -1, not 65535.
    let base = &mut f.arena.owned_mut(enemy).unwrap().state.base;
    base.position_words = Some([0x8000, 2, 0]);
    f.arena.actor_mut(enemy).unwrap().word_b4 = u32::MAX;
    assert!(!f.run(owner).unwrap()); // Y-only squared distance4 is strict out.
    assert_eq!(f.host.creates, 1);
    f.arena.owned_mut(enemy).unwrap().state.base.flags &= !0x20;
    f.arena.owned_mut(enemy).unwrap().state.base.position_words = None;
    assert!(!f.run(owner).unwrap()); // flags skip even unknown XYZ/class task.
}

#[test]
fn primary_and_reverse_keep_distinct_presence_shifts_and_lifecycle_policy() {
    for (present, lifecycle, source_excluded) in [
        (false, 2, false),
        (true, 2, false),
        (false, 1, false),
        (false, 2, true),
    ] {
        let mut f = Fixture::new();
        let owner = f.family(Some([0; 3]), 3);
        let enemy = f.derived([0; 3], 4);
        f.arena.derived_actor_mut(owner).unwrap().word_164 = 1000;
        if present {
            behavior(&mut f.arena, enemy);
        }
        f.arena.actor_mut(enemy).unwrap().word_c4 = lifecycle;
        if source_excluded {
            let task = f.host.tasks.owner(3).unwrap().unwrap();
            f.host
                .tasks
                .write_source(task, f.arena.actor(enemy).unwrap().handle)
                .unwrap();
        }
        assert!(f.run(owner).unwrap());
        assert_eq!(
            f.score(owner, FamilyTargetSlot::Target1f4),
            if lifecycle <= 1 || source_excluded {
                0
            } else if present {
                0xc000 | 500
            } else {
                0xc000 | 250
            }
        );
        assert_eq!(
            f.score(owner, FamilyTargetSlot::Target1ec),
            if present { 500 } else { 125 }
        );
        assert_eq!(
            f.host.creates,
            usize::from(lifecycle > 1 && !source_excluded)
        );
    }
}

#[test]
fn mutable_gate_keeps_captures_but_fresh_network_group_position_null_and_boost() {
    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 3);
    let enemy = f.derived([0; 3], 4);
    let friend = f.derived([0; 3], 3);
    f.arena.actor_mut(friend).unwrap().words_8e_92 = [1000, 0, 0];
    f.arena.derived_actor_mut(owner).unwrap().word_164 = 1000;
    let changed_null = ActorHandle::from_raw(0x4321_0000);
    f.host.effect = Some(Box::new(move |_, arena, context| {
        arena.actor_mut(owner).unwrap().word_b4 = 5;
        arena.actor_mut(enemy).unwrap().word_b4 = 6;
        behavior(arena, enemy); // captured candidate absence remains in effect.
        arena.derived_actor_mut(owner).unwrap().behavior_140 = None;
        arena.derived_actor_mut(owner).unwrap().word_164 = 8;
        arena.family428650_actor_mut(owner).unwrap().word_214 = 1024;
        arena.owned_mut(owner).unwrap().state.base.position_words = Some([50, 0, 0]);
        context.process.network_flag = 0;
        context.process.process_null_handle = Some(changed_null);
    }));
    assert!(f.run(owner).unwrap());
    assert_eq!(
        f.score(owner, FamilyTargetSlot::Target1f4),
        (0xc000 | 250) * 2
    );
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 0);
    assert_eq!(f.host.creates, 1);
    let group = f
        .arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1e4)
        .unwrap();
    assert_eq!(group.score, 500); // old class3 still groups self and friend.
    let NativeTarget::FixedPosition(point) = f.arena.native_target(group.target.unwrap()).unwrap()
    else {
        panic!("expected copied group point")
    };
    assert_eq!(point.focus_08, Some([550, 0, 0]));
    assert_eq!(point.handle_04, changed_null);
}

#[test]
fn captured_task_retirement_and_live_current_node_removal_stop_at_reached_reads() {
    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 3);
    let _first = f.derived([0; 3], 4);
    let _second = f.derived([0; 3], 5);
    let captured = f.host.tasks.owner(3).unwrap().unwrap();
    f.host.effect = Some(Box::new(move |tasks, _, _| {
        tasks.destroy(captured).unwrap();
        tasks.construct(3, NULL).unwrap();
    }));
    assert!(matches!(
        f.run(owner),
        Err(FamilyActorListSelectionError::Relation(TaskRelationError::Storage(
            TaskStorageError::MissingOwner(value)
        ))) if value == captured
    ));
    assert_ne!(f.host.tasks.owner(3).unwrap(), Some(captured));
    assert!(f.allocations.calls.is_empty());

    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 3);
    let enemy = f.derived([0; 3], 4);
    f.host.effect = Some(Box::new(move |_, arena, _| {
        // Retain the live actor while removing only its current typed node.
        // This is a boundary fixture, not a faithful raw list-rewrite service.
        arena.actors.retain(|&value| value != enemy);
    }));
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::RemovedCurrentNode(enemy))
    );
    assert!(f.arena.actor(enemy).is_ok());
    assert!(f.allocations.calls.is_empty());
}

#[test]
fn high_bit_group_norm_consumes_fresh_position_before_zero_root_and_group_gate_is_strict() {
    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 3);
    let friend = f.derived([0; 3], 3);
    let _enemy = f.derived([0; 3], 4);
    for actor in [owner, friend] {
        f.arena.actor_mut(actor).unwrap().words_8e_92 = [0x7fff; 3];
    }
    f.host.effect = Some(Box::new(move |_, arena, _| {
        arena.owned_mut(owner).unwrap().state.base.position_words = None;
    }));
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::MissingPosition(owner))
    );
    assert!(f.allocations.calls.is_empty());
    for means in [[256, 256, 0], [257, 256, 0]] {
        let mut f = Fixture::new();
        let owner = f.family(Some([0; 3]), 3);
        let friend = f.derived([0; 3], 3);
        for actor in [owner, friend] {
            f.arena.actor_mut(actor).unwrap().words_8e_92 = means;
        }
        let expected = means != [256, 256, 0];
        assert_eq!(f.run(owner).unwrap(), expected);
        assert_eq!(f.allocations.calls.len(), usize::from(expected));
    }
}

#[test]
fn three_publishers_continue_after_true_and_keep_crossed_third_failure_prefix() {
    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 3);
    let friend = f.derived([0; 3], 3);
    let enemy = f.derived([0; 3], 4);
    for actor in [owner, friend] {
        f.arena.actor_mut(actor).unwrap().words_8e_92 = [600, 0, 0];
    }
    let old = f
        .arena
        .follow_target(FollowTargetRequest::resolve_actor(
            f.arena.actor(enemy).unwrap().handle,
        ))
        .unwrap();
    let old = f
        .arena
        .import_fixture_target(NativeTarget::ActorFollow(old));
    f.arena
        .family428650_actor_mut(owner)
        .unwrap()
        .targets_1d4_1fc[2] = Some(old);
    f.allocations.outcomes = VecDeque::from([
        Ok(false), // real allocation NULL still returns true to the publisher.
        Ok(true),
        Err("third allocation stopped"),
    ]);
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::Target(
            NativeTargetError::Heap(ActorServiceHeapError::FixtureFailure(71))
        ))
    );
    assert_eq!(
        f.allocations.calls,
        [
            NativeTargetClass::ActorFollow,
            NativeTargetClass::ActorFollow,
            NativeTargetClass::FixedPosition
        ]
    );
    let primary = f
        .arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1f4)
        .unwrap();
    assert_eq!(primary.score, 0xc000 | 625);
    assert_eq!(primary.target, None);
    // A normal allocation-NULL return is represented by the actual NULL slot.
    assert!(f
        .arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1ec)
        .unwrap()
        .target
        .is_some());
    let group = f
        .arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1e4)
        .unwrap();
    assert_eq!(group.score, 600);
    assert_eq!(group.target, Some(old));
    assert_eq!(
        f.arena.actor_service_heap().state(old).unwrap(),
        ActorServiceResourceState::Released
    );
    assert!(f.arena.native_target(old).is_err());
}

#[test]
fn canonical_component_task_uses_actual_selector12_presence_and_distinct_owner_callback() {
    let mut f = Fixture::new();
    let owner = f.family(Some([0; 3]), 18);
    let ordinary = f.derived([0; 3], 1);
    f.arena.derived_actor_mut(owner).unwrap().word_164 = 1000;
    assert!(f.run(owner).unwrap()); // class1 ordinary +5C NULL returns relation1.
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 0xc000 | 125);
    f.arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: ordinary,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let grants =
        inf_formats::parse_grant_table(inf_formats::FrontendExecutable::parse(&raw).unwrap())
            .unwrap();
    let vehicle = f
        .arena
        .allocate_selector12_actor(
            request(Some([0; 3])),
            &mut f.rng,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    classify(&mut f.arena, vehicle, 1);
    assert!(!f.run(owner).unwrap()); // actual +5C Self then +7C NULL ->0.
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 0);
    behavior(&mut f.arena, vehicle);
    assert!(f.run(owner).unwrap());
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 0xc000 | 250);
    assert_eq!(f.run(vehicle), Ok(false)); // actual distinct body, class1 skips interaction.
}

#[path = "selector12_tests.rs"]
mod selector12_tests;
