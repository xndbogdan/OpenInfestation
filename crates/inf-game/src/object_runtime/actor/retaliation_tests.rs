use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};

fn actor_request() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0; 3]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &[0; 4096],
        },
    }
}

fn setup() -> (ObjectArena, ObjectId, ObjectId, ActorHandle) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 32,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut construction_rng = 1;
    let mut actors = Vec::new();
    for _ in 0..3 {
        let id = arena
            .allocate_family428650_actor(
                DerivedActorConstructionRequest {
                    actor: actor_request(),
                    world_mask_limits: [0, 0],
                },
                &mut construction_rng,
            )
            .unwrap();
        let base = &mut arena.owned_mut(id).unwrap().state.base;
        base.flags = 0x20;
        base.dirty_word = 0;
        arena.actor_mut(id).unwrap().word_b4 = 7;
        actors.push(id);
    }
    let source = actors[2];
    arena.owned_mut(source).unwrap().state.base.flags = 0;
    let source = arena.actor(source).unwrap().handle;
    (arena, actors[0], actors[1], source)
}

fn position(arena: &mut ObjectArena, actor: ObjectId, words: [u16; 3]) {
    arena.owned_mut(actor).unwrap().state.base.position_words = Some(words);
}

/// A synthetic seed whose next native LCG result is exactly the desired
/// high WORD. Five Newton steps invert the odd multiplier modulo2^32.
fn seed_for_next(draw: u16) -> u32 {
    let multiplier = 0x0013_d56du32;
    let mut inverse = 1u32;
    for _ in 0..5 {
        inverse = inverse.wrapping_mul(2u32.wrapping_sub(multiplier.wrapping_mul(inverse)));
    }
    (u32::from(draw) << 16)
        .wrapping_sub(0x0013_d573)
        .wrapping_mul(inverse)
}

#[test]
fn nearby_target_uses_byte_radius_times_256_and_strict_word_distance() {
    for (distance, admitted) in [(255, true), (256, false)] {
        let (mut arena, victim, candidate, source) = setup();
        arena.actor_mut(victim).unwrap().byte_7c = 1;
        position(&mut arena, candidate, [distance, 0, 0]);
        let mut rng = 1;
        arena
            .propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut rng)
            .unwrap();
        let family = arena.family428650_actor(candidate).unwrap();
        assert_eq!(family.count_1a0, u32::from(admitted));
        if admitted {
            assert_eq!(family.entries_1a4[0].handle, source);
            assert_eq!(family.entries_1a4[0].word_08, 0x0780_0000);
            assert_eq!(family.entries_1a4[0].word_04, 0);
        }
        assert_eq!(rng, 1); // +D4 does not draw RNG; reaction radius is zero.
        assert_eq!(arena.family428650_actor(victim).unwrap().count_1a0, 0);
    }
}

#[test]
fn stopped_construction_retaliation_receiver_is_unchanged_and_healthy_receiver_continues() {
    let (mut arena, victim, stopped, source) = setup();
    arena.actor_mut(victim).unwrap().byte_7c = 1;
    arena.owned_mut(stopped).unwrap().state.base.position_words = None;
    arena.stop_object_construction(stopped).unwrap();
    let healthy = arena.resolve_actor_handle(source).unwrap().unwrap();
    arena.owned_mut(healthy).unwrap().state.base.flags = 0x20;
    let before = arena.object(stopped).unwrap().clone();
    let nodes = arena.actor_list().to_vec();
    let mut rng = 1;

    arena
        .propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut rng)
        .unwrap();

    assert_eq!(arena.family428650_actor(healthy).unwrap().count_1a0, 1);
    assert_eq!(arena.object(stopped).unwrap(), &before);
    assert_eq!(arena.actor_list(), nodes);
    assert_eq!(rng, 1);
}

#[test]
fn word_subtraction_and_unsigned_three_axis_square_match_native_comparisons() {
    let (mut arena, victim, candidate, source) = setup();
    arena.actor_mut(victim).unwrap().byte_7c = 1;
    position(&mut arena, victim, [65530, 0, 0]);
    position(&mut arena, candidate, [4, 0, 0]);
    arena
        .propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut 1)
        .unwrap();
    assert_eq!(arena.family428650_actor(candidate).unwrap().count_1a0, 1);

    let (mut arena, victim, candidate, source) = setup();
    arena.actor_mut(victim).unwrap().byte_7c = 255;
    position(&mut arena, candidate, [0x8000; 3]);
    arena
        .propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut 1)
        .unwrap();
    // SquareC0000000 is unsigned-below FE010000 despite its signed high bit.
    assert_eq!(arena.family428650_actor(candidate).unwrap().count_1a0, 1);
}

#[test]
fn classification_flags_and_signed_lifecycle_skip_before_position_reads() {
    for (classification, flags, lifecycle) in [
        (8, 0x20, 3),
        (7, 0xa0, 3),
        (7, 0x00, 3),
        (7, 0x20, 1),
        (7, 0x20, u32::MAX),
    ] {
        let (mut arena, victim, candidate, source) = setup();
        arena.actor_mut(victim).unwrap().byte_7c = 1;
        arena.actor_mut(candidate).unwrap().word_b4 = classification;
        arena.actor_mut(candidate).unwrap().word_c4 = lifecycle;
        let base = &mut arena.owned_mut(candidate).unwrap().state.base;
        base.flags = flags;
        base.position_words = None;
        arena
            .propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut 1)
            .unwrap();
        assert_eq!(arena.family428650_actor(candidate).unwrap().count_1a0, 0);
    }
}

#[test]
fn matching_candidate_position_is_read_before_the_self_handle_gate_even_with_zero_radius() {
    let (mut arena, victim, candidate, source) = setup();
    arena.owned_mut(victim).unwrap().state.base.position_words = None;
    assert!(matches!(
        arena.propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut 1),
        Err(ActorRetaliationError::MissingPosition(actor)) if actor == victim
    ));
    // Skip the first node, then an eligible zero-radius candidate still
    // reaches both position reads before the impossible strict distance gate.
    arena.owned_mut(victim).unwrap().state.base.flags = 0;
    arena
        .owned_mut(candidate)
        .unwrap()
        .state
        .base
        .position_words = None;
    assert!(matches!(
        arena.propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut 1),
        Err(ActorRetaliationError::MissingPosition(actor)) if actor == candidate
    ));
}

#[test]
fn null_component_has_no_family_target_or_reaction_and_reads_no_rng() {
    let (mut arena, victim, candidate, source) = setup();
    arena.owned_mut(candidate).unwrap().state.base.flags = 0;
    let shared = arena.allocate_actor(actor_request()).unwrap();
    arena.owned_mut(shared).unwrap().state.base.flags = 0x1020;
    arena.actor_mut(shared).unwrap().word_b4 = 7;
    let victim_state = arena.actor_mut(victim).unwrap();
    victim_state.byte_7c = 1;
    victim_state.byte_7d = 1;
    let mut rng = 1;
    arena
        .propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut rng)
        .unwrap();
    assert_eq!(rng, 1);
}

#[test]
fn reaction_radius_and_flag_are_independent_of_target_radius_and_active_component_byte() {
    for (target_radius, reaction_radius, flags, targets, draws) in [
        (1, 0, 0x1020, 1, 0),
        (0, 1, 0x1020, 0, 1),
        (1, 1, 0x0020, 1, 0),
        (1, 1, 0x1020, 1, 1),
    ] {
        let (mut arena, victim, candidate, source) = setup();
        let victim_state = arena.actor_mut(victim).unwrap();
        victim_state.byte_7c = target_radius;
        victim_state.byte_7d = reaction_radius;
        arena.owned_mut(candidate).unwrap().state.base.flags = flags;
        assert_eq!(arena.family428650_actor(candidate).unwrap().byte_22c, 0);
        let mut rng = 1;
        let mut expected_rng = rng;
        for _ in 0..draws {
            name_entry_rng_next(&mut expected_rng);
        }
        arena
            .propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut rng)
            .unwrap();
        assert_eq!(rng, expected_rng);
        let family = arena.family428650_actor(candidate).unwrap();
        assert_eq!(family.count_1a0, targets);
        if draws != 0 {
            assert_eq!(family.word_20c, arena.actor(victim).unwrap().handle.raw());
        }
    }
}

#[test]
fn target_admission_error_keeps_activation_prefix_and_skips_later_reaction_draw() {
    let (mut arena, victim, candidate, source) = setup();
    let victim_state = arena.actor_mut(victim).unwrap();
    victim_state.byte_7c = 1;
    victim_state.byte_7d = 1;
    arena.owned_mut(candidate).unwrap().state.base.flags = 0x0460_1020;
    arena.derived_actor_mut(candidate).unwrap().word_144 = -3;
    arena.family428650_actor_mut(candidate).unwrap().count_1a0 = 5;
    let mut rng = 1;
    assert!(matches!(
        arena.propagate_actor_retaliation(victim, source, Some(ActorHandle::from_raw(0)), &mut rng),
        Err(ActorRetaliationError::Target(ActorFamilyTargetError::TargetCount { actor, count: 5 })) if actor == candidate
    ));
    assert_eq!(arena.derived_actor(candidate).unwrap().word_144, 3000);
    assert_eq!(arena.object(candidate).unwrap().base.flags, 0x1020);
    assert_eq!(rng, 1);
}

#[test]
fn initial_and_same_target_reactions_use_one_draw_and_exact_microsecond_timer() {
    for retained_same in [false, true] {
        let (mut arena, _, candidate, source) = setup();
        let family = arena.family428650_actor_mut(candidate).unwrap();
        if retained_same {
            family.word_20c = source.raw();
            family.word_210 = 1;
        }
        arena.derived_actor_mut(candidate).unwrap().word_144 = i32::MIN;
        arena.owned_mut(candidate).unwrap().state.base.flags = 0x0460_1020;
        let mut rng = seed_for_next(0xffff);
        let mut expected_rng = rng;
        let draw = name_entry_rng_next(&mut expected_rng);
        assert_eq!(draw, 0xffff);
        arena
            .react_family_retaliation(candidate, source, Some(ActorHandle::from_raw(0)), &mut rng)
            .unwrap();
        assert_eq!(rng, expected_rng);
        let family = arena.family428650_actor(candidate).unwrap();
        assert_eq!(family.word_20c, source.raw());
        assert_eq!(family.word_210, (156250 + 65535) << 7);
        assert_eq!(arena.derived_actor(candidate).unwrap().word_144, 0);
        assert_eq!(arena.object(candidate).unwrap().base.flags, 0x1020);
        assert_eq!(arena.object(candidate).unwrap().base.dirty_word, 0);
    }
}

#[test]
fn nonnull_target_with_zero_timer_rejects_even_the_same_source_without_rng_or_writes() {
    let (mut arena, _, candidate, source) = setup();
    let family = arena.family428650_actor_mut(candidate).unwrap();
    family.word_20c = source.raw();
    family.word_210 = 0;
    arena.derived_actor_mut(candidate).unwrap().word_144 = -7;
    arena.owned_mut(candidate).unwrap().state.base.flags = 0x0460_1020;
    let mut rng = 1;
    arena
        .react_family_retaliation(candidate, source, Some(ActorHandle::from_raw(0)), &mut rng)
        .unwrap();
    assert_eq!(rng, 1);
    assert_eq!(arena.derived_actor(candidate).unwrap().word_144, -7);
    assert_eq!(arena.object(candidate).unwrap().base.flags, 0x0460_1020);
}

#[test]
fn changed_target_gate_uses_strict_unsigned_low_word_then_a_separate_timer_draw() {
    for (gate, accepted) in [(0x7ff, true), (0x800, false), (0xffff, false)] {
        let (mut arena, victim, candidate, source) = setup();
        let retained = arena.actor(victim).unwrap().handle;
        let family = arena.family428650_actor_mut(candidate).unwrap();
        family.word_20c = retained.raw();
        family.word_210 = 17;
        let mut rng = seed_for_next(gate);
        let mut expected_rng = rng;
        assert_eq!(name_entry_rng_next(&mut expected_rng), u32::from(gate));
        let timer_draw = accepted.then(|| name_entry_rng_next(&mut expected_rng));
        arena
            .react_family_retaliation(candidate, source, Some(ActorHandle::from_raw(0)), &mut rng)
            .unwrap();
        assert_eq!(rng, expected_rng);
        let family = arena.family428650_actor(candidate).unwrap();
        if let Some(draw) = timer_draw {
            assert_eq!(family.word_20c, source.raw());
            assert_eq!(family.word_210, (156250 + (draw & 0xffff)) << 7);
        } else {
            assert_eq!(family.word_20c, retained.raw());
            assert_eq!(family.word_210, 17);
        }
    }
}

#[test]
fn family_retaliation_compares_retained_target_to_current_process_null() {
    let current_null = ActorHandle::from_raw(0x8123_4567);
    for (retained, admitted) in [(current_null.raw(), true), (0, false)] {
        let (mut arena, _, candidate, source) = setup();
        assert_eq!(arena.actor_handles().unwrap().null_handle().raw(), 0);
        let family = arena.family428650_actor_mut(candidate).unwrap();
        family.word_20c = retained;
        family.word_210 = 0;
        let mut rng = 1;
        let mut expected_rng = rng;
        if admitted {
            name_entry_rng_next(&mut expected_rng);
        }
        arena
            .react_family_retaliation(candidate, source, Some(current_null), &mut rng)
            .unwrap();
        assert_eq!(rng, expected_rng);
        assert_eq!(
            arena.family428650_actor(candidate).unwrap().word_20c,
            if admitted { source.raw() } else { retained }
        );
    }
}

#[test]
fn missing_process_null_is_required_only_at_the_reached_family_reaction() {
    let (mut arena, victim, candidate, source) = setup();
    arena.actor_mut(victim).unwrap().byte_7c = 1;
    arena.actor_mut(victim).unwrap().byte_7d = 0;
    arena.owned_mut(candidate).unwrap().state.base.flags = 0x1020;
    let mut rng = 1;
    arena
        .propagate_actor_retaliation(victim, source, None, &mut rng)
        .unwrap();
    assert_eq!(arena.family428650_actor(candidate).unwrap().count_1a0, 1);
    assert_eq!(rng, 1);

    arena.actor_mut(victim).unwrap().byte_7d = 1;
    assert_eq!(
        arena.propagate_actor_retaliation(victim, source, None, &mut rng),
        Err(ActorRetaliationError::MissingProcessNullHandle(candidate))
    );
    assert_eq!(rng, 1);
    assert_eq!(arena.family428650_actor(candidate).unwrap().word_20c, 0);
}
