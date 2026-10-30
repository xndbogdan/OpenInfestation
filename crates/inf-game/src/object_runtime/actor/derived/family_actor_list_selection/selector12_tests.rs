use super::*;
use crate::object_runtime::{ActorInteractionError, Selector12State, Selector35State};

const SLOTS: [FamilyTargetSlot; 4] = [
    FamilyTargetSlot::Target1ec,
    FamilyTargetSlot::Target274,
    FamilyTargetSlot::Target27c,
    FamilyTargetSlot::Target1f4,
];

fn vehicle(f: &mut Fixture, position: Option<[u16; 3]>, class: u32, radius: i32) -> ObjectId {
    // No first-player grant input is supplied; the constructor's real bit
    // owner starts empty. An empty table is unused on that constructor arm.
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let actor = f
        .arena
        .allocate_selector12_actor(
            request(position),
            &mut f.rng,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    classify(&mut f.arena, actor, class);
    behavior(&mut f.arena, actor);
    f.arena.derived_actor_mut(actor).unwrap().word_164 = radius;
    // Keep self out of this test's candidate pass without removing its node.
    f.arena.owned_mut(actor).unwrap().state.base.flags &= !0x20;
    actor
}

fn enemy(f: &mut Fixture, position: [u16; 3], class: u32, present: bool) -> ObjectId {
    let actor = f.derived(position, class);
    f.arena.actor_mut(actor).unwrap().word_c4 = 3;
    if present {
        behavior(&mut f.arena, actor);
    }
    actor
}

fn resource(f: &mut Fixture, position: [u16; 3], class: u32) -> ObjectId {
    let actor = f
        .arena
        .allocate_selector30_actor(request(Some(position)).actor)
        .unwrap();
    classify(&mut f.arena, actor, class);
    actor
}

fn selected(f: &Fixture, owner: ObjectId, slot: FamilyTargetSlot) -> Option<ActorHandle> {
    f.arena
        .family_weighted_target(owner, slot)
        .unwrap()
        .target
        .map(|reference| f.arena.native_target(reference).unwrap().handle_04())
}

fn old_target(f: &mut Fixture, owner: ObjectId, slot: FamilyTargetSlot) {
    f.arena
        .publish_family_target(
            owner,
            slot,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Point(Some([77, 88, 99])),
                score: 77,
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
            },
        )
        .unwrap();
}

fn selector12_mut(f: &mut Fixture, actor: ObjectId) -> &mut Selector12State {
    let Family429e20Class::Selector12(state) =
        &mut f.arena.family429e20_actor_mut(actor).unwrap().class
    else {
        unreachable!("actual selector12 constructor")
    };
    state
}

fn four_roles(f: &mut Fixture) -> (ObjectId, ObjectId, ObjectId, ObjectId) {
    let owner = vehicle(f, Some([0; 3]), 2, 1000);
    let primary = enemy(f, [100, 0, 0], 3, true);
    let interaction = resource(f, [200, 0, 0], 3);
    let grant = enemy(f, [100, 0, 0], 0, false);
    f.arena.actor_mut(grant).unwrap().word_7a = 3;
    selector12_mut(f, owner).grants_24c.mask_04 = [0; 6];
    (owner, primary, interaction, grant)
}

#[test]
fn empty_and_flags_bypass_keep_null_locals_without_common_inputs() {
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, None, u32::MAX, 1000);
    let rejected = f.family(None, u32::MAX);
    f.arena.owned_mut(rejected).unwrap().state.base.flags = 0;
    f.host.tasks.release().unwrap();
    f.process.process_null_handle = None;
    f.arena.derived_actor_mut(owner).unwrap().behavior_140 = None;
    f.arena.family428650_actor_mut(owner).unwrap().word_20c = 0xdead_beef;
    f.arena.family428650_actor_mut(owner).unwrap().words_1d8_200[4] = 901;
    selector12_mut(&mut f, owner).word_278 = 902;
    assert_eq!(f.run(owner), Ok(false));
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 901);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target274), 902);
    assert!(SLOTS
        .into_iter()
        .all(|slot| selected(&f, owner, slot).is_none()));
    assert_eq!(
        f.arena.family428650_actor(owner).unwrap().word_20c,
        0xdead_beef
    );
    assert_eq!(f.host.creates, 0);
    assert!(f.allocations.calls.is_empty());
}

#[test]
fn stopped_construction_selector12_candidate_preserves_carried_score_and_continuation() {
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0; 3]), 18, 1000);
    let interaction = resource(&mut f, [200, 0, 0], 3);
    let stopped = f.family(None, u32::MAX);
    f.arena.stop_object_construction(stopped).unwrap();
    let far = enemy(&mut f, [800, 0, 0], 1, true);
    let before = f.arena.object(stopped).unwrap().clone();
    let nodes = f.arena.actor_list().to_vec();

    assert!(f.run(owner).unwrap());
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 4000);
    assert_eq!(
        selected(&f, owner, FamilyTargetSlot::Target1ec),
        Some(f.arena.actor(far).unwrap().handle)
    );
    assert_eq!(
        selected(&f, owner, FamilyTargetSlot::Target274),
        Some(f.arena.actor(interaction).unwrap().handle)
    );
    assert_eq!(f.host.creates, 0);
    assert_eq!(f.arena.object(stopped).unwrap(), &before);
    assert_eq!(f.arena.actor_list(), nodes);
}

#[test]
fn far_zero_relation_retains_initial_quarter_and_prior_interaction_score() {
    for prior_resource in [false, true] {
        let mut f = Fixture::new();
        let owner = vehicle(&mut f, Some([0; 3]), 18, 1000);
        let resource_actor = prior_resource.then(|| resource(&mut f, [200, 0, 0], 3));
        let far = enemy(&mut f, [800, 0, 0], 1, true);
        assert_eq!(f.run(owner), Ok(true));
        assert_eq!(
            f.score(owner, FamilyTargetSlot::Target1ec),
            if prior_resource { 4000 } else { 250_000 }
        );
        assert_eq!(
            selected(&f, owner, FamilyTargetSlot::Target1ec),
            Some(f.arena.actor(far).unwrap().handle)
        );
        assert_eq!(
            selected(&f, owner, FamilyTargetSlot::Target274),
            resource_actor.map(|id| f.arena.actor(id).unwrap().handle)
        );
        assert_eq!(f.host.creates, 0);
    }
}

#[test]
fn grant_signed_negative_y_carries_and_owned_or_zero_ids_bypass() {
    for (y, id, owned, expected) in [
        (-600i16, 3u16, false, 1400u32),
        (600, 3, false, 1_000_000),
        (-600, 0, false, 1_000_000),
        (-600, 3, true, 1_000_000),
    ] {
        let mut f = Fixture::new();
        let owner = vehicle(&mut f, Some([0; 3]), 18, 2000);
        let grant = enemy(&mut f, [0, y as u16, 0], 0, false);
        f.arena.actor_mut(grant).unwrap().word_7a = id;
        selector12_mut(&mut f, owner).grants_24c.mask_04 = [0; 6];
        if owned {
            selector12_mut(&mut f, owner).grants_24c.mask_04[0] = 1 << 3;
        }
        let far = enemy(&mut f, [1000, 0, 0], 1, true);
        assert_eq!(f.run(owner), Ok(true));
        assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), expected);
        assert_eq!(
            selected(&f, owner, FamilyTargetSlot::Target1ec),
            Some(f.arena.actor(far).unwrap().handle)
        );
        assert_eq!(
            selected(&f, owner, FamilyTargetSlot::Target27c).is_some(),
            y < 512 && id != 0 && !owned
        );
    }
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0; 3]), 18, 1000);
    let grant = enemy(&mut f, [0; 3], 0, false);
    f.arena.actor_mut(grant).unwrap().word_7a = 192;
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::Grant(
            inf_formats::GrantClosureError::Id(192)
        ))
    );
    assert!(f.allocations.calls.is_empty());
}

#[test]
fn null_self_presence_blocks_primary_but_secondary_and_word_xyz_range_remain() {
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0x7fff, 0, 0]), 2, 2);
    f.arena.derived_actor_mut(owner).unwrap().behavior_140 = None;
    let candidate = enemy(&mut f, [0x8000, 0, 0], 3, true);
    assert_eq!(f.run(owner), Ok(true));
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 1);
    assert_eq!(f.host.creates, 0);
    assert_eq!(selected(&f, owner, FamilyTargetSlot::Target1f4), None);
    assert_eq!(
        selected(&f, owner, FamilyTargetSlot::Target1ec),
        Some(f.arena.actor(candidate).unwrap().handle)
    );
    f.arena
        .owned_mut(candidate)
        .unwrap()
        .state
        .base
        .position_words = Some([0x7fff, 2, 0]);
    f.arena.actor_mut(candidate).unwrap().word_b4 = u32::MAX;
    assert_eq!(f.run(owner), Ok(false)); // strict squared equality4, before task.
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 0);
}

#[test]
fn interaction_self54_uses_zero_distance_and_strict_close_boundary() {
    for (x, byte, expected) in [(255, 1, 4000), (256, 1, 2372), (0, 0, 0)] {
        let mut f = Fixture::new();
        let owner = vehicle(&mut f, Some([0; 3]), 3, 1000);
        let candidate = resource(&mut f, [x, 0, 0], 3);
        let ActorExtension::Selector30(state) =
            &mut f.arena.actor_mut(candidate).unwrap().extension
        else {
            unreachable!()
        };
        state.byte_18e = byte;
        // Ordinary flags20 lack the null54 notification flag. Actual30 self54
        // bypasses that filter, and numericCC1 consumes actual self50 NULL.
        assert_eq!(f.run(owner), Ok(expected != 0));
        assert_eq!(f.score(owner, FamilyTargetSlot::Target274), expected);
        assert_eq!(f.host.creates, 0);
    }
}

fn selector35_mut(f: &mut Fixture, actor: ObjectId) -> &mut Selector35State {
    let Family428650Class::Selector35(state) =
        &mut f.arena.family428650_actor_mut(actor).unwrap().class
    else {
        unreachable!()
    };
    state
}

#[test]
fn notification_clock_is_lazy_and_fresh_after_a_mutable_gate() {
    for byte in [None, Some(0), Some(1)] {
        let mut f = Fixture::new();
        let owner = vehicle(&mut f, Some([0; 3]), 3, 1000);
        let candidate = f
            .arena
            .allocate_selector35_actor(request(Some([0; 3])), &mut f.rng)
            .unwrap();
        classify(&mut f.arena, candidate, 3);
        f.arena.owned_mut(candidate).unwrap().state.base.flags |= 0x1000_0000;
        selector35_mut(&mut f, candidate).bytes_240[2] = byte;
        selector35_mut(&mut f, candidate).word_244 = 999;
        let expected = match byte {
            None => Err(FamilyActorListSelectionError::Interaction(
                ActorInteractionError::Uninitialized {
                    actor: candidate,
                    offset: 0x242,
                },
            )),
            Some(0) => Ok(true),
            Some(_) => Err(FamilyActorListSelectionError::Interaction(
                ActorInteractionError::MissingAnimationClock,
            )),
        };
        assert_eq!(f.run(owner), expected);
        assert_eq!(selector35_mut(&mut f, candidate).word_244, 999);
    }
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0; 3]), 2, 1000);
    let first = enemy(&mut f, [100, 0, 0], 3, true);
    let later = f
        .arena
        .allocate_selector35_actor(request(Some([0; 3])), &mut f.rng)
        .unwrap();
    classify(&mut f.arena, later, 2);
    f.arena.owned_mut(later).unwrap().state.base.flags |= 0x1000_0000;
    selector35_mut(&mut f, later).bytes_240[2] = Some(1);
    let mut clock = AnimationClock::default();
    clock.advance(7 * 20_000);
    f.host.clock = Some(clock);
    f.host.advance_clock_on_create_micros = Some(3 * 20_000);
    f.host.effect = Some(Box::new(move |_, arena, _| {
        // Current-row base/presence stay captured; later XYZ/radius are fresh.
        arena.derived_actor_mut(first).unwrap().behavior_140 = None;
        arena.derived_actor_mut(owner).unwrap().word_164 = 100;
        arena.owned_mut(owner).unwrap().state.base.position_words = Some([50, 0, 0]);
    }));
    assert_eq!(f.run(owner), Ok(true));
    assert_eq!(f.host.clock.as_ref().unwrap().value(), 10);
    assert_eq!(selector35_mut(&mut f, later).word_244, 210);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 900);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 650);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target274), 225);
}

#[test]
fn captured_class_one_skips_interaction_even_after_gate_changes_live_class() {
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0; 3]), 1, 1000);
    let candidate = enemy(&mut f, [0; 3], 3, true);
    f.arena.owned_mut(candidate).unwrap().state.base.flags |= 0x1000_0000;
    f.host.effect = Some(Box::new(move |_, arena, _| {
        arena.actor_mut(owner).unwrap().word_b4 = 2;
    }));
    // Team relation for class1 self is0, so select a component-task class18
    // candidate: actual self5C and nonnull7C yield relation1 and reach the gate.
    f.arena.actor_mut(candidate).unwrap().word_b4 = 18;
    assert_eq!(f.run(owner), Ok(true));
    assert_eq!(f.host.creates, 1);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target274), 0);
    assert_eq!(f.arena.actor(owner).unwrap().word_b4, 2);
}

#[test]
fn all_four_publishers_continue_after_true_and_preserve_crossed_failure_prefixes() {
    for stop in [None, Some(0usize), Some(1), Some(2), Some(3)] {
        let mut f = Fixture::new();
        let (owner, primary, interaction, grant) = four_roles(&mut f);
        // Type8 old resources require the real crossed-class deletion before
        // allocating type2. Different type2 handles merely replace in place.
        for slot in SLOTS {
            old_target(&mut f, owner, slot);
        }
        f.allocations.calls.clear();
        f.allocations.outcomes = (0..4)
            .map(|i| {
                if Some(i) == stop {
                    Err("allocation stopped")
                } else {
                    Ok(true)
                }
            })
            .collect();
        let result = f.run(owner);
        if let Some(index) = stop {
            assert_eq!(
                result,
                Err(FamilyActorListSelectionError::Target(
                    NativeTargetError::Heap(ActorServiceHeapError::FixtureFailure(71))
                ))
            );
            assert_eq!(f.allocations.calls.len(), index + 1);
            for (prior, slot) in SLOTS.into_iter().enumerate() {
                let pair = f.arena.family_weighted_target(owner, slot).unwrap();
                if prior < index {
                    let reference = pair.target.unwrap();
                    assert!(matches!(
                        f.arena.actor_service_heap().state(reference).unwrap(),
                        ActorServiceResourceState::Target(_)
                    ));
                } else if prior == index {
                    let reference = pair.target.unwrap();
                    assert_eq!(
                        f.arena.actor_service_heap().state(reference).unwrap(),
                        ActorServiceResourceState::Released
                    );
                    assert!(f.arena.native_target(reference).is_err());
                    assert_eq!(pair.score, [900, 4000, 2450, 2450][index]);
                } else {
                    assert_eq!(pair.score, 77);
                    assert!(
                        matches!(f.arena.native_target(pair.target.unwrap()).unwrap(), NativeTarget::FixedPosition(target) if target.focus_08 == Some([77, 88, 99]))
                    );
                }
            }
        } else {
            assert_eq!(result, Ok(false)); // all crossed old slots, never initially empty.
            assert_eq!(
                SLOTS.map(|slot| f.score(owner, slot)),
                [900, 4000, 2450, 2450]
            );
            assert_eq!(
                SLOTS.map(|slot| selected(&f, owner, slot)),
                [
                    Some(f.arena.actor(primary).unwrap().handle),
                    Some(f.arena.actor(interaction).unwrap().handle),
                    Some(f.arena.actor(grant).unwrap().handle),
                    Some(f.arena.actor(primary).unwrap().handle)
                ]
            );
            assert_eq!(f.allocations.calls, [NativeTargetClass::ActorFollow; 4]);
        }
    }
    let mut f = Fixture::new();
    let (owner, _, _, _) = four_roles(&mut f);
    f.allocations.outcomes = VecDeque::from([Ok(false); 4]);
    assert_eq!(f.run(owner), Ok(true)); // real allocation NULL still returns initially-empty true.
    assert_eq!(f.allocations.calls, [NativeTargetClass::ActorFollow; 4]);
    assert_eq!(
        SLOTS.map(|slot| f.score(owner, slot)),
        [900, 4000, 2450, 2450]
    );
    assert!(SLOTS
        .into_iter()
        .all(|slot| selected(&f, owner, slot).is_none()));
}

#[test]
fn fresh_candidate_task_and_live_appends_do_not_reuse_a_retired_task_capture() {
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0; 3]), 2, 1000);
    let first = enemy(&mut f, [100, 0, 0], 3, true);
    let old_task = f.host.tasks.owner(3).unwrap().unwrap();
    f.host.effect = Some(Box::new(move |tasks, arena, _| {
        tasks.destroy(old_task).unwrap();
        tasks.construct(3, NULL).unwrap();
        let appended = arena
            .allocate_derived_actor(request(Some([0; 3])), &mut 0)
            .unwrap();
        classify(arena, appended, 3);
        arena.actor_mut(appended).unwrap().word_c4 = 3;
        behavior(arena, appended);
        // A ready cache suppresses another create, but the later row still
        // selects the newly registered task owner and improves both scores.
    }));
    assert_eq!(f.run(owner), Ok(true));
    assert_ne!(f.host.tasks.owner(3).unwrap(), Some(old_task));
    assert_eq!(f.host.creates, 1);
    let chosen = selected(&f, owner, FamilyTargetSlot::Target1ec).unwrap();
    assert_ne!(chosen, f.arena.actor(first).unwrap().handle);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 1000);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 2500);
}

#[test]
fn unsigned_high_bit_xyz_range_and_reached_current_node_boundary_remain_distinct() {
    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0; 3]), 2, 65_535);
    let candidate = enemy(&mut f, [0x8000; 3], 3, true);
    assert_eq!(f.run(owner), Ok(true));
    // DWORD C0000000 is inside captured FFFE0001, while the actual signed
    // square-root primitive returns0, making base equal to the full radius.
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1ec), 65_535);
    assert_eq!(f.score(owner, FamilyTargetSlot::Target1f4), 163_837);
    assert_eq!(
        selected(&f, owner, FamilyTargetSlot::Target1ec),
        Some(f.arena.actor(candidate).unwrap().handle)
    );

    let mut f = Fixture::new();
    let owner = vehicle(&mut f, Some([0; 3]), 1, 1000);
    let candidate = enemy(&mut f, [0; 3], 18, true);
    f.host.effect = Some(Box::new(move |_, arena, _| {
        // Still-live actor, but its captured typed node is absent. Captured
        // class1 bypasses later interaction reads, reaching continuation first.
        arena.actors.retain(|&id| id != candidate);
    }));
    assert_eq!(
        f.run(owner),
        Err(FamilyActorListSelectionError::RemovedCurrentNode(candidate))
    );
    assert!(f.arena.actor(candidate).is_ok());
    assert!(f.allocations.calls.is_empty());
}
