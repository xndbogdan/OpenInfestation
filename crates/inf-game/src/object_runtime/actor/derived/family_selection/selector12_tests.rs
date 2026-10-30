use super::*;
use crate::object_runtime::{FollowTargetRequest, NativeTarget, Selector12GrantContext};

struct Fixture {
    arena: ObjectArena,
    owner: ObjectId,
    tables: ProcessActorBehaviorTables,
    process: ParticleFactoryProcessState,
    rng: u32,
    host: Host,
}

impl Fixture {
    fn new() -> Self {
        let raw = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
        )
        .unwrap();
        let grants =
            inf_formats::parse_grant_table(inf_formats::FrontendExecutable::parse(&raw).unwrap())
                .unwrap();
        let mut arena = arena();
        let owner = arena
            .allocate_selector12_actor(
                request([0; 3]),
                &mut 0,
                Selector12GrantContext {
                    table: &grants,
                    first_player_grants: None,
                },
            )
            .unwrap();
        arena.derived_actor_mut(owner).unwrap().word_164 = 100;
        arena.derived_actor_mut(owner).unwrap().behavior_140 =
            crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
        // Neither B4 nor task owner nor source20C selects this virtual policy.
        arena.actor_mut(owner).unwrap().word_b4 = u32::MAX;
        arena.family428650_actor_mut(owner).unwrap().word_20c = 0xdead_beef;
        let mut host = Host::new();
        host.tasks = ProcessTaskRuntime::default();
        Self {
            arena,
            owner,
            tables: tables(),
            process: process(),
            rng: 7,
            host,
        }
    }

    fn candidate(&mut self, position: [u16; 3]) -> ObjectId {
        let actor = family(&mut self.arena, position);
        self.arena.actor_mut(actor).unwrap().word_c4 = 1;
        actor
    }

    fn row(&mut self, index: usize, actor: ObjectId, secondary: u32, primary: u32) {
        row(
            &mut self.arena,
            self.owner,
            index,
            actor,
            secondary,
            primary,
        );
    }

    fn count(&mut self, count: u32) {
        self.arena
            .family428650_actor_mut(self.owner)
            .unwrap()
            .count_1a0 = count;
    }

    fn run(
        &mut self,
        allocations: &mut AllocationPlan,
    ) -> Result<bool, FamilySelectionError<&'static str>> {
        allocations.select(
            &mut self.arena,
            FamilySelectionRequest { actor: self.owner },
            &mut BehaviorPredicateContext {
                tables: &mut self.tables,
                process: &mut self.process,
                rng: &mut self.rng,
            },
            &mut self.host,
        )
    }

    fn target(&self, slot: FamilyTargetSlot) -> crate::object_runtime::NativeWeightedTarget {
        self.arena.family_weighted_target(self.owner, slot).unwrap()
    }

    fn decoded_target(&self, slot: FamilyTargetSlot) -> Option<NativeTarget> {
        self.target(slot)
            .target
            .map(|reference| self.arena.native_target(reference).unwrap())
    }

    fn old_targets(&mut self, actor: ObjectId) {
        let target = NativeTarget::ActorFollow(
            self.arena
                .follow_target(FollowTargetRequest::resolve_actor(
                    self.arena.actor(actor).unwrap().handle,
                ))
                .unwrap(),
        );
        // Two native pointer slots own separate allocations, despite equal data.
        let first = self.arena.import_fixture_target(target);
        let second = self.arena.import_fixture_target(target);
        let state = self.arena.family428650_actor_mut(self.owner).unwrap();
        state.targets_1d4_1fc[0] = Some(first);
        state.targets_1d4_1fc[1] = Some(second);
        state.words_1d8_200[0] = 701;
        state.words_1d8_200[1] = 702;
    }
}

#[test]
fn zero_count_only_clears_secondary_while_negative_counts_clear_both_without_inputs() {
    for count in [0, u32::MAX, 0x8000_0000] {
        let mut f = Fixture::new();
        let candidate = f.candidate([0; 3]);
        f.old_targets(candidate);
        f.count(count);
        f.arena
            .owned_mut(f.owner)
            .unwrap()
            .state
            .base
            .position_words = None;
        f.arena.derived_actor_mut(f.owner).unwrap().behavior_140 = None;
        f.process.process_null_handle = None;
        let primary = f.target(FamilyTargetSlot::Target1d4);
        let mut allocator = AllocationPlan::default();
        assert_eq!(f.run(&mut allocator), Ok(false));
        assert!(f.target(FamilyTargetSlot::Target1dc).target.is_none());
        if count == 0 {
            assert_eq!(f.target(FamilyTargetSlot::Target1d4), primary);
        } else {
            assert!(f.target(FamilyTargetSlot::Target1d4).target.is_none());
        }
        assert_eq!(allocator.calls, 0);
        assert_eq!(f.host.creates, 0);
        assert_eq!(
            f.arena.family428650_actor(f.owner).unwrap().count_1a0,
            count
        );
        assert_eq!(f.rng, 7);
    }
}

#[test]
fn selector12_enum_dispatch_admits_lifecycle_one_and_secondary_zero_without_primary_presence() {
    let mut f = Fixture::new();
    let candidate = f.candidate([0, 0x8000, 0]);
    f.count(1);
    f.row(0, candidate, 0, 0x0b40_0000);
    f.arena.derived_actor_mut(f.owner).unwrap().behavior_140 = None;
    f.process.process_null_handle = None;
    let mut allocator = AllocationPlan::default();
    assert_eq!(f.run(&mut allocator), Ok(true));
    assert_eq!(f.target(FamilyTargetSlot::Target1dc).score, 25);
    assert_eq!(
        f.decoded_target(FamilyTargetSlot::Target1dc)
            .unwrap()
            .handle_04(),
        f.arena.actor(candidate).unwrap().handle
    );
    assert!(f.target(FamilyTargetSlot::Target1d4).target.is_none());
    assert_eq!(allocator.calls, 1);
    assert_eq!(f.host.creates, 0);
    assert_eq!(f.arena.family428650_actor(f.owner).unwrap().count_1a0, 1);
}

#[test]
fn signed_extended_xz_strict_radius_and_ties_preserve_valid_rows() {
    for position in [[0x7fff, 0, 0], [0x8001, 0x7fff, 0], [0x8064, 0, 0]] {
        let mut f = Fixture::new();
        f.arena
            .owned_mut(f.owner)
            .unwrap()
            .state
            .base
            .position_words = Some([0x8000, 0, 0]);
        let candidate = f.candidate(position);
        let handle = f.arena.actor(candidate).unwrap().handle;
        f.count(1);
        f.row(0, candidate, 0, 0);
        let mut allocator = AllocationPlan::default();
        f.run(&mut allocator).unwrap();
        assert_eq!(
            f.target(FamilyTargetSlot::Target1dc).score,
            if position[0] == 0x8001 { 24 } else { 0 }
        );
        let state = f.arena.family428650_actor(f.owner).unwrap();
        assert_eq!(state.entries_1a4[0].handle, handle);
        assert_eq!(state.count_1a0, 1);
    }
    let mut f = Fixture::new();
    let first = f.candidate([0; 3]);
    let second = f.candidate([0, 200, 0]);
    f.count(2);
    f.row(0, first, 0, 0x0b40_0000);
    f.row(1, second, 0, 0x0b40_0000);
    f.run(&mut AllocationPlan::default()).unwrap();
    for slot in [FamilyTargetSlot::Target1dc, FamilyTargetSlot::Target1d4] {
        assert_eq!(
            f.decoded_target(slot).unwrap().handle_04(),
            f.arena.actor(first).unwrap().handle
        );
    }
    assert_eq!(f.host.creates, 1); // equal primary does not reach a second gate.
}

#[test]
fn primary_and_secondary_weight_zero_high_and_wrapped_middle_policies_remain_distinct() {
    for (weight, primary, secondary) in [
        (0, 0, 25),
        (0x02cf_ffff, 25, 25),
        (0x02d0_0000, 25, 25),
        (0x0b3f_ffff, 99, 99),
        (0x0b40_0000, 100, 0),
        (u32::MAX, 100, 0),
    ] {
        let mut f = Fixture::new();
        let candidate = f.candidate([0; 3]);
        f.count(1);
        f.row(0, candidate, weight, weight);
        f.run(&mut AllocationPlan::default()).unwrap();
        assert_eq!(f.target(FamilyTargetSlot::Target1d4).score, primary);
        assert_eq!(f.target(FamilyTargetSlot::Target1dc).score, secondary);
        assert_eq!(f.host.creates, usize::from(primary > 0));
    }
    let mut f = Fixture::new();
    let candidate = f.candidate([0; 3]);
    f.arena.derived_actor_mut(f.owner).unwrap().word_164 = -1;
    f.count(1);
    f.row(0, candidate, 0x02d0_0000, 0x02d0_0000);
    f.run(&mut AllocationPlan::default()).unwrap();
    // Wrapped product FFFF E980 / 5A00, not widened/signed multiplication.
    assert_eq!(f.target(FamilyTargetSlot::Target1dc).score, 186413);
    assert_eq!(f.target(FamilyTargetSlot::Target1d4).score, 186413);
}

#[test]
fn mutable_gate_retains_current_secondary_and_square_but_rereads_later_position_radius_presence_count(
) {
    for (new_radius, self_x, next_x, expected) in
        [(200, 0, 50, 149), (200, 0, 150, 25), (100, 100, 120, 79)]
    {
        let mut f = Fixture::new();
        let first = f.candidate([0; 3]);
        let second = f.candidate([next_x, 0, 0]);
        f.count(1); // the gate appends the already supplied second row.
        f.row(0, first, 0, 0x0b40_0000);
        f.row(1, second, 0x0b3f_ffff, 0x0b40_0000);
        let owner = f.owner;
        f.host.effect = Some(Box::new(move |_, arena, context| {
            let derived = arena.derived_actor_mut(owner).unwrap();
            derived.word_164 = new_radius;
            derived.behavior_140 = None;
            arena.owned_mut(owner).unwrap().state.base.position_words = Some([self_x, 0, 0]);
            let family = arena.family428650_actor_mut(owner).unwrap();
            family.count_1a0 = 2;
            family.entries_1a4[0].word_04 = 0x0b40_0000;
            context.process.process_null_handle = None;
        }));
        let mut allocator = AllocationPlan::default();
        assert_eq!(f.run(&mut allocator), Ok(true));
        assert_eq!(f.host.creates, 1);
        assert_eq!(f.target(FamilyTargetSlot::Target1d4).score, 100);
        assert_eq!(f.target(FamilyTargetSlot::Target1dc).score, expected);
        let expected_actor = if expected == 25 { first } else { second };
        assert_eq!(
            f.decoded_target(FamilyTargetSlot::Target1dc)
                .unwrap()
                .handle_04(),
            f.arena.actor(expected_actor).unwrap().handle
        );
        assert_eq!(allocator.calls, 2); // first true still reaches D4.
    }
}

#[test]
fn stopped_gate_keeps_resolver_prefix_and_skips_prune_and_both_publishers() {
    let mut f = Fixture::new();
    let candidate = f.candidate([0; 3]);
    f.old_targets(candidate);
    let before = [
        f.target(FamilyTargetSlot::Target1d4),
        f.target(FamilyTargetSlot::Target1dc),
    ];
    f.count(2);
    f.row(1, candidate, 0, 0x0b40_0000);
    f.arena.family428650_actor_mut(f.owner).unwrap().entries_1a4[0].handle =
        ActorHandle::from_raw(0xdead_beef);
    f.host.fail = true;
    let mut allocator = AllocationPlan::default();
    assert!(matches!(
        f.run(&mut allocator),
        Err(FamilySelectionError::Gate(_))
    ));
    let state = f.arena.family428650_actor(f.owner).unwrap();
    assert_eq!(state.count_1a0, 2);
    assert_eq!(state.entries_1a4[0].handle, NULL);
    assert_eq!(f.target(FamilyTargetSlot::Target1d4), before[0]);
    assert_eq!(f.target(FamilyTargetSlot::Target1dc), before[1]);
    assert_eq!(allocator.calls, 0);
}

#[test]
fn resolver_dirty_prune_uses_new_live_null_instead_of_earlier_null_write() {
    let mut f = Fixture::new();
    let candidate = f.candidate([0; 3]);
    f.count(3);
    f.row(1, candidate, 0, 0x0b40_0000);
    f.arena.family428650_actor_mut(f.owner).unwrap().entries_1a4[0].handle =
        ActorHandle::from_raw(0xdead_beef);
    f.arena.family428650_actor_mut(f.owner).unwrap().entries_1a4[2].handle =
        ActorHandle::from_raw(0xdead_dead);
    let new_null = ActorHandle::from_raw(0x8765_4321);
    f.host.effect = Some(Box::new(move |_, _, context| {
        context.process.process_null_handle = Some(new_null);
    }));
    f.run(&mut AllocationPlan::default()).unwrap();
    let state = f.arena.family428650_actor(f.owner).unwrap();
    assert_eq!(state.count_1a0, 2);
    assert_eq!(state.entries_1a4[0].handle, NULL); // old NULL now counts as a survivor.
    assert_eq!(state.entries_1a4[2].handle, new_null);
}

#[test]
fn secondary_publisher_precedes_primary_with_null_success_and_crossed_failure_prefixes() {
    for fail_at in [1, 2, 0] {
        let mut f = Fixture::new();
        let candidate = f.candidate([0; 3]);
        f.count(1);
        f.row(0, candidate, 0, 0x0b40_0000);
        let old = NativeTarget::ObjectPosition(f.arena.object_position_target(candidate).unwrap());
        let first = f.arena.import_fixture_target(old);
        let second = f.arena.import_fixture_target(old);
        let family = f.arena.family428650_actor_mut(f.owner).unwrap();
        family.targets_1d4_1fc[0] = Some(first);
        family.targets_1d4_1fc[1] = Some(second);
        family.words_1d8_200[0] = 701;
        family.words_1d8_200[1] = 702;
        let mut allocator = AllocationPlan {
            calls: 0,
            fail: false,
            fail_at,
            null_at: if fail_at == 0 { 1 } else { 0 },
        };
        let result = f.run(&mut allocator);
        if fail_at != 0 {
            assert!(matches!(
                result,
                Err(FamilySelectionError::Target(NativeTargetError::Heap(
                    ActorServiceHeapError::FixtureFailure(71)
                )))
            ));
        } else {
            assert_eq!(result, Ok(false)); // both prior slots were occupied.
        }
        let secondary = f.target(FamilyTargetSlot::Target1dc);
        assert_eq!(secondary.score, 25);
        if fail_at == 1 {
            assert_eq!(secondary.target, Some(second));
            assert_eq!(
                f.arena.actor_service_heap().state(second).unwrap(),
                ActorServiceResourceState::Released
            );
            assert_eq!(f.target(FamilyTargetSlot::Target1d4).score, 701);
            assert_eq!(allocator.calls, 1);
        } else {
            assert_eq!(f.target(FamilyTargetSlot::Target1d4).score, 100);
            assert_eq!(allocator.calls, 2);
            let primary = f.target(FamilyTargetSlot::Target1d4).target.unwrap();
            if fail_at == 2 {
                assert_eq!(primary, first);
                assert_eq!(
                    f.arena.actor_service_heap().state(primary).unwrap(),
                    ActorServiceResourceState::Released
                );
            } else {
                assert!(matches!(
                    f.arena.actor_service_heap().state(primary).unwrap(),
                    ActorServiceResourceState::Target(_)
                ));
            }
            if fail_at == 0 {
                assert!(secondary.target.is_none());
            }
        }
    }
}
