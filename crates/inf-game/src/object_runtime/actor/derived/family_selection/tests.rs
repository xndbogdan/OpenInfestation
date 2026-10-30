use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorServiceHeapError, ActorServiceResourceKind,
    ActorServiceResourceState, ActorTerrainInputs, BehaviorComponentQuery, BehaviorCreationRequest,
    BehaviorPacketCopy, BehaviorPacketSourceRequest, BehaviorTemporary,
    DerivedActorConstructionRequest, FixtureActorServiceAllocation, NativeTargetClass,
    ObjectClassKind, ProcessActorBehaviorTables,
};

#[path = "selector12_tests.rs"]
mod selector12_tests;

const NULL: ActorHandle = ActorHandle::from_raw(0x1234_5678);
const SINE: [u16; 4096] = [0; 4096];

fn request(position: [u16; 3]) -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(position),
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

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap()
}

fn family(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    let actor = arena
        .allocate_family428650_actor(request(position), &mut 0)
        .unwrap();
    arena.actor_mut(actor).unwrap().word_b4 = 3;
    arena.derived_actor_mut(actor).unwrap().behavior_140 =
        crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
    actor
}

fn process() -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        process_null_handle: Some(NULL),
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
    }
}

fn tables() -> ProcessActorBehaviorTables {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    ProcessActorBehaviorTables::from_executable(
        inf_formats::FrontendExecutable::parse(&raw).unwrap(),
    )
    .unwrap()
}

type Effect =
    Box<dyn FnMut(&mut ProcessTaskRuntime, &mut ObjectArena, &mut BehaviorPredicateContext<'_>)>;

struct Host {
    tasks: ProcessTaskRuntime,
    creates: usize,
    accept: bool,
    fail: bool,
    effect: Option<Effect>,
}

impl Host {
    fn new() -> Self {
        let mut tasks = ProcessTaskRuntime::default();
        tasks.retain(NULL).unwrap();
        Self {
            tasks,
            creates: 0,
            accept: true,
            fail: false,
            effect: None,
        }
    }
}

impl FamilySelectionHost for Host {
    fn process_tasks(&mut self) -> &mut ProcessTaskRuntime {
        &mut self.tasks
    }

    fn animation_clock(&self) -> Option<&dyn crate::frontend_camera::AnimationClockSource> {
        None
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
        if let Some(effect) = &mut self.effect {
            effect(&mut self.tasks, arena, context);
        }
        if self.fail {
            return Err("required creation stopped");
        }
        // Explicit supplied lower creation, not a claimed factory execution.
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
    calls: usize,
    fail: bool,
    fail_at: usize,
    null_at: usize,
}
impl AllocationPlan {
    // Script the actual arena heap; count its real attempts after the call.
    fn select(
        &mut self,
        arena: &mut ObjectArena,
        request: FamilySelectionRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut Host,
    ) -> Result<bool, FamilySelectionError<&'static str>> {
        arena.actor_service_heap_mut().script((1..=2).map(|next| {
            if self.fail || self.calls + next == self.fail_at {
                FixtureActorServiceAllocation::Error(71)
            } else if self.calls + next == self.null_at {
                FixtureActorServiceAllocation::Null
            } else {
                FixtureActorServiceAllocation::Allocate
            }
        }));
        let before = arena.actor_service_heap().attempts().len();
        let result = arena.select_family_targets(request, context, host);
        let reached = &arena.actor_service_heap().attempts()[before..];
        assert!(reached
            .iter()
            .all(|kind| *kind == ActorServiceResourceKind::Target(NativeTargetClass::ActorFollow)));
        self.calls += reached.len();
        result
    }
}

fn row(
    arena: &mut ObjectArena,
    owner: ObjectId,
    index: usize,
    candidate: ObjectId,
    secondary: u32,
    primary: u32,
) {
    let handle = arena.actor(candidate).unwrap().handle;
    arena.family428650_actor_mut(owner).unwrap().entries_1a4[index] =
        super::super::FamilyHandleEntry {
            handle,
            word_04: secondary,
            word_08: primary,
        };
}

#[test]
fn reverse_prune_preserves_native_holes_weights_and_count_policy() {
    for middle in [false, true] {
        let mut arena = arena();
        let owner = family(&mut arena, [0; 3]);
        let table = FamilyTargetTable { owner };
        let family = arena.family428650_actor_mut(owner).unwrap();
        family.count_1a0 = 3;
        for (i, entry) in family.entries_1a4.iter_mut().enumerate() {
            entry.handle = ActorHandle::from_raw(0x101 + i as u32);
            entry.word_04 = 0x1100_0000 + i as u32;
            entry.word_08 = 0x2200_0000 + i as u32;
        }
        family.entries_1a4[usize::from(middle)].handle = NULL;
        let original = family.entries_1a4;
        arena.prune_family_target_table(table, &process()).unwrap();
        let after = arena.family428650_actor(owner).unwrap();
        assert_eq!(after.count_1a0, 2);
        if middle {
            assert_eq!(after.entries_1a4[0], original[0]);
            assert_eq!(after.entries_1a4[1], original[1]);
            assert_eq!(after.entries_1a4[2], original[1]);
        } else {
            assert_eq!(&after.entries_1a4[..3], &[original[0]; 3]);
        }
        assert_eq!(after.entries_1a4[3], original[3]);
    }
    let mut arena = arena();
    let owner = family(&mut arena, [0; 3]);
    let mut process = process();
    process.process_null_handle = None;
    for count in [0, u32::MAX, 0x8000_0000] {
        arena.family428650_actor_mut(owner).unwrap().count_1a0 = count;
        arena
            .prune_family_target_table(FamilyTargetTable { owner }, &process)
            .unwrap();
        assert_eq!(arena.family428650_actor(owner).unwrap().count_1a0, count);
    }
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = 5;
    assert!(matches!(
        arena.prune_family_target_table(FamilyTargetTable { owner }, &process),
        Err(FamilyTargetTableError::MissingProcessNull { address: 0x482d6e })
    ));
    process.process_null_handle = Some(NULL);
    assert!(matches!(
        arena.prune_family_target_table(FamilyTargetTable { owner }, &process),
        Err(FamilyTargetTableError::UnrepresentedEntry { index: 4, .. })
    ));
    assert_eq!(arena.family428650_actor(owner).unwrap().count_1a0, 5);
}

#[test]
fn resolver_runs_on_current_null_identity_and_clear_guard_is_unsigned_lazy() {
    let mut arena = arena();
    let owner = family(&mut arena, [0; 3]);
    let candidate = family(&mut arena, [0; 3]);
    row(&mut arena, owner, 0, candidate, 1, 1);
    let candidate_handle = arena.actor(candidate).unwrap().handle;
    let mut process = process();
    process.process_null_handle = Some(candidate_handle);
    assert_eq!(
        arena
            .resolve_family_table_entry(FamilyTargetTable { owner }, 0, &process)
            .unwrap(),
        Some(candidate)
    );
    assert_eq!(
        arena.actor_handles().unwrap().cache().object,
        Some(candidate)
    );
    assert_eq!(
        arena.actor_handles().unwrap().cache().handle,
        candidate_handle
    );
    let wrong = arena.allocate(ObjectClassKind::Base);
    process.process_null_handle = None;
    for index in [4, u32::MAX] {
        arena
            .clear_family_table_entry(FamilyTargetTable { owner: wrong }, index, &process)
            .unwrap();
    }
    arena.actor_mut(candidate).unwrap().word_c4 = 1;
    assert_eq!(
        arena
            .resolve_family_table_entry(FamilyTargetTable { owner }, 0, &process)
            .unwrap(),
        Some(candidate)
    );
    arena.actor_mut(candidate).unwrap().word_c4 = 0x8000_0000;
    assert!(matches!(
        arena.resolve_family_table_entry(FamilyTargetTable { owner }, 0, &process),
        Err(FamilyTargetTableError::MissingProcessNull { address: 0x482f73 })
    ));
    assert_eq!(
        arena.family428650_actor(owner).unwrap().entries_1a4[0].handle,
        candidate_handle
    );
}

#[test]
fn signed_count_still_requires_task_source_and_zero_best_keeps_unwritten_pointer() {
    let mut arena = arena();
    let owner = family(&mut arena, [0; 3]);
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = u32::MAX;
    let mut host = Host::new();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut primary = FamilyTargetSelection {
        score: u32::MAX,
        candidate: FamilySelectionCandidate::Null,
    };
    let scan = FamilyTargetScanRequest {
        actor: owner,
        table: FamilyTargetTable { owner },
        center_words: [0; 3],
    };
    let task = host.tasks.owner(3).unwrap().unwrap();
    host.tasks.destroy(task).unwrap();
    assert!(matches!(
        arena.scan_family_targets(
            scan,
            FamilyTargetScanOutputs::PrimaryOnly(&mut primary),
            &mut context,
            &mut host
        ),
        Err(FamilyTargetScanError::Task(TaskStorageError::NullOwner(3)))
    ));
    assert_eq!(primary.score, u32::MAX);
    host.tasks.construct(3, NULL).unwrap();
    context.process.process_null_handle = None;
    arena
        .scan_family_targets(
            scan,
            FamilyTargetScanOutputs::PrimaryOnly(&mut primary),
            &mut context,
            &mut host,
        )
        .unwrap();
    assert_eq!(primary, FamilyTargetSelection::EMPTY);
    assert_eq!(host.creates, 0);
}

#[test]
fn signed_extended_xz_and_unsigned_squared_root_gate_are_distinct_from_word_wrap() {
    let mut arena = arena();
    let owner = family(&mut arena, [0x8000, 0, 0x8000]);
    let candidate = family(&mut arena, [0x7fff, 0, 0x7fff]);
    arena.derived_actor_mut(owner).unwrap().word_164 = 65535;
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = 1;
    row(&mut arena, owner, 0, candidate, 0x0b40_0000, 0x0b40_0000);
    let mut host = Host::new();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut primary = FamilyTargetSelection::EMPTY;
    arena
        .scan_family_targets(
            FamilyTargetScanRequest {
                actor: owner,
                table: FamilyTargetTable { owner },
                center_words: [0x8000, 0, 0x8000],
            },
            FamilyTargetScanOutputs::PrimaryOnly(&mut primary),
            &mut context,
            &mut host,
        )
        .unwrap();
    assert_eq!(
        primary,
        FamilyTargetSelection {
            score: 65535,
            candidate: FamilySelectionCandidate::Actor(candidate)
        }
    );
    assert_eq!(host.creates, 1);
}

#[test]
fn mutable_gate_rereads_secondary_and_count_but_retains_base_and_task_source() {
    let mut arena = arena();
    let owner = family(&mut arena, [0; 3]);
    let candidate = family(&mut arena, [0; 3]);
    arena.derived_actor_mut(owner).unwrap().word_164 = 100;
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = 1;
    row(&mut arena, owner, 0, candidate, 0x0b40_0000, 0x0b40_0000);
    let handle = arena.actor(candidate).unwrap().handle;
    let mut host = Host::new();
    host.effect = Some(Box::new(move |tasks, arena, _| {
        arena.derived_actor_mut(owner).unwrap().word_164 = 4;
        arena.family428650_actor_mut(owner).unwrap().entries_1a4[0].word_04 = 0;
        arena.family428650_actor_mut(owner).unwrap().count_1a0 = 0;
        let task = tasks.owner(3).unwrap().unwrap();
        tasks.write_source(task, handle).unwrap();
    }));
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let (mut primary, mut secondary) = (FamilyTargetSelection::EMPTY, FamilyTargetSelection::EMPTY);
    arena
        .scan_family_targets(
            FamilyTargetScanRequest {
                actor: owner,
                table: FamilyTargetTable { owner },
                center_words: [0; 3],
            },
            FamilyTargetScanOutputs::Both {
                primary: &mut primary,
                secondary: &mut secondary,
            },
            &mut context,
            &mut host,
        )
        .unwrap();
    assert_eq!(primary.score, 100);
    assert_eq!(secondary.score, 25);
    assert_eq!(
        host.tasks
            .record(host.tasks.owner(3).unwrap().unwrap())
            .unwrap()
            .source,
        handle
    );
    assert_eq!(arena.family428650_actor(owner).unwrap().count_1a0, 0);
}

#[test]
fn rejected_primary_still_selects_secondary_and_gate_failure_skips_prune_outputs() {
    let mut arena = arena();
    let owner = family(&mut arena, [0; 3]);
    let candidate = family(&mut arena, [0; 3]);
    arena.derived_actor_mut(owner).unwrap().word_164 = 100;
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = 2;
    row(&mut arena, owner, 1, candidate, 0, 0x0b40_0000);
    let mut host = Host::new();
    host.fail = true;
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let (mut primary, mut secondary) = (FamilyTargetSelection::EMPTY, FamilyTargetSelection::EMPTY);
    let scan = FamilyTargetScanRequest {
        actor: owner,
        table: FamilyTargetTable { owner },
        center_words: [0; 3],
    };
    assert!(matches!(
        arena.scan_family_targets(
            scan,
            FamilyTargetScanOutputs::Both {
                primary: &mut primary,
                secondary: &mut secondary
            },
            &mut context,
            &mut host
        ),
        Err(FamilyTargetScanError::Gate(_))
    ));
    assert_eq!(arena.family428650_actor(owner).unwrap().count_1a0, 2);
    assert_eq!(
        arena.family428650_actor(owner).unwrap().entries_1a4[0].handle,
        NULL
    );
    assert_eq!(primary, FamilyTargetSelection::EMPTY);
    assert_eq!(secondary, FamilyTargetSelection::EMPTY);
    host.fail = false;
    host.accept = false;
    arena
        .scan_family_targets(
            scan,
            FamilyTargetScanOutputs::Both {
                primary: &mut primary,
                secondary: &mut secondary,
            },
            &mut context,
            &mut host,
        )
        .unwrap();
    assert_eq!(primary, FamilyTargetSelection::EMPTY);
    assert_eq!(secondary.score, 25);
    assert_eq!(arena.family428650_actor(owner).unwrap().count_1a0, 1);
    assert_eq!(
        arena.family428650_actor(owner).unwrap().entries_1a4[1].handle,
        NULL
    );
}

#[test]
fn outer_rereads_live_task_source_between_scans_and_always_reaches_second_publisher() {
    let mut arena = arena();
    let owner = family(&mut arena, [0; 3]);
    let reference = family(&mut arena, [3, 3, 3]);
    let candidate = family(&mut arena, [2, 2, 2]);
    arena.derived_actor_mut(owner).unwrap().word_164 = 8;
    let source = arena.actor(reference).unwrap().handle.raw();
    arena.family428650_actor_mut(owner).unwrap().word_20c = source;
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = 1;
    arena.family428650_actor_mut(reference).unwrap().count_1a0 = 1;
    row(&mut arena, reference, 0, candidate, 0, 0x0b40_0000);
    row(&mut arena, owner, 0, candidate, 0, 0x0b40_0000);
    let handle = arena.actor(candidate).unwrap().handle;
    let mut host = Host::new();
    host.effect = Some(Box::new(move |tasks, _, _| {
        let task = tasks.owner(3).unwrap().unwrap();
        tasks.write_source(task, handle).unwrap();
    }));
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut allocator = AllocationPlan::default();
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    assert!(allocator
        .select(
            &mut arena,
            FamilySelectionRequest { actor: owner },
            &mut context,
            &mut host
        )
        .unwrap());
    assert_eq!(
        arena
            .family_weighted_target(owner, FamilyTargetSlot::Target1d4)
            .unwrap()
            .score,
        10
    );
    assert!(arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1dc)
        .unwrap()
        .target
        .is_none());
    assert_eq!(allocator.calls, 1);
    assert_eq!(host.creates, 1);
    // Now both channels select and both allocation calls occur despite true
    // from the first publisher. Existing primary is cleared independently.
    let task = host.tasks.owner(3).unwrap().unwrap();
    host.tasks.write_source(task, NULL).unwrap();
    host.effect = None;
    arena.family428650_actor_mut(owner).unwrap().word_20c = NULL.raw();
    arena.family428650_actor_mut(owner).unwrap().targets_1d4_1fc[0] = None;
    allocator.calls = 0;
    assert!(allocator
        .select(
            &mut arena,
            FamilySelectionRequest { actor: owner },
            &mut context,
            &mut host
        )
        .unwrap());
    assert_eq!(allocator.calls, 2);
}

#[test]
fn wrapped_boost_zero_candidate_and_crossed_allocator_failure_preserve_publisher_custody() {
    let mut arena = arena();
    let owner = family(&mut arena, [0; 3]);
    let reference = family(&mut arena, [0; 3]);
    let candidate = family(&mut arena, [0; 3]);
    let source = arena.actor(reference).unwrap().handle.raw();
    arena.derived_actor_mut(owner).unwrap().word_164 = 4;
    arena.family428650_actor_mut(owner).unwrap().word_20c = source;
    arena.family428650_actor_mut(owner).unwrap().word_214 = 0x8000_0000;
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = u32::MAX;
    arena.family428650_actor_mut(owner).unwrap().words_1d8_200[0] = 777;
    arena.family428650_actor_mut(reference).unwrap().count_1a0 = 1;
    row(&mut arena, reference, 0, candidate, 0, 1);
    let mut host = Host::new();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut allocator = AllocationPlan::default();
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    assert!(!allocator
        .select(
            &mut arena,
            FamilySelectionRequest { actor: owner },
            &mut context,
            &mut host
        )
        .unwrap());
    assert_eq!(allocator.calls, 0);
    assert_eq!(
        arena
            .family_weighted_target(owner, FamilyTargetSlot::Target1d4)
            .unwrap()
            .score,
        777
    );
    // Crossed-class failure publishes score before releasing old ownership,
    // and prevents the second publisher from running.
    let old = arena.object_position_target(candidate).unwrap();
    let second = arena
        .follow_target(crate::object_runtime::FollowTargetRequest::resolve_actor(
            arena.actor(candidate).unwrap().handle,
        ))
        .unwrap();
    let old = arena.import_fixture_target(crate::object_runtime::NativeTarget::ObjectPosition(old));
    let second =
        arena.import_fixture_target(crate::object_runtime::NativeTarget::ActorFollow(second));
    arena.family428650_actor_mut(owner).unwrap().targets_1d4_1fc[0] = Some(old);
    arena.family428650_actor_mut(owner).unwrap().targets_1d4_1fc[1] = Some(second);
    arena.family428650_actor_mut(owner).unwrap().words_1d8_200[1] = 900;
    arena.family428650_actor_mut(owner).unwrap().word_20c = NULL.raw();
    arena.family428650_actor_mut(owner).unwrap().count_1a0 = 1;
    row(&mut arena, owner, 0, candidate, 0, 0x0b40_0000);
    allocator.fail = true;
    assert!(matches!(
        allocator.select(
            &mut arena,
            FamilySelectionRequest { actor: owner },
            &mut context,
            &mut host
        ),
        Err(FamilySelectionError::Target(NativeTargetError::Heap(
            ActorServiceHeapError::FixtureFailure(71)
        )))
    ));
    let first = arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1d4)
        .unwrap();
    assert_eq!(first.score, 4);
    assert_eq!(first.target, Some(old));
    assert_eq!(
        arena.actor_service_heap().state(old).unwrap(),
        ActorServiceResourceState::Released
    );
    assert!(arena.native_target(old).is_err());
    assert_eq!(allocator.calls, 1);
    let second = arena
        .family_weighted_target(owner, FamilyTargetSlot::Target1dc)
        .unwrap();
    assert_eq!(second.score, 900);
    assert!(second.target.is_some());
}
