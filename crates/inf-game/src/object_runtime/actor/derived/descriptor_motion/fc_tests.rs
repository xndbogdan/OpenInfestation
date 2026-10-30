//! Supplied strict constructor/heap/route domains, not a loaded actor observation.
use super::*;
use crate::object_runtime::{
    ActorServiceAllocationRequest, ActorServiceResourceKind, FixedPositionTarget, NativeHeapRef,
    NativeTarget,
};

fn current(bias: i32) -> FamilyDescriptorScoreRequest {
    FamilyDescriptorScoreRequest {
        kind: FamilyDescriptorScoreKind::VirtualFc,
        current: true,
        bias,
    }
}
fn common(selector36: bool) -> (ObjectArena, ObjectId, Arc<RuntimeAllocation>) {
    if !selector36 {
        return retained_fixture(FamilyBehaviorProfile::Family4236d0 { mode: 1 });
    }
    let mut arena = self::arena();
    let mut rng = 0;
    let actor = arena
        .allocate_selector36_actor(request(), &mut rng)
        .unwrap();
    let owner = allocation(vec![0; 32]);
    arena.set_family4236d0_retained(actor, &owner, 4).unwrap();
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    (arena, actor, owner)
}
fn target(arena: &mut ObjectArena, data: NativeTarget) -> NativeHeapRef {
    let reference = arena
        .allocate_actor_service(ActorServiceAllocationRequest {
            kind: ActorServiceResourceKind::Target(data.class()),
        })
        .unwrap()
        .unwrap();
    arena.construct_native_target(reference, data).unwrap();
    reference
}
fn fixed(arena: &mut ObjectArena) -> NativeHeapRef {
    target(
        arena,
        NativeTarget::FixedPosition(FixedPositionTarget::from_point(
            ActorHandle::from_raw(0x1234_5678),
            Some([5, 6, 7]),
        )),
    )
}
fn object_target(arena: &mut ObjectArena, object: ObjectId) -> NativeHeapRef {
    let data = arena.object_position_target(object).unwrap();
    target(arena, NativeTarget::ObjectPosition(data))
}
fn node(arena: &mut ObjectArena, point: [u16; 3]) -> ObjectId {
    let object = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(object).unwrap().state.base.position_words = Some(point);
    object
}
fn install(arena: &mut ObjectArena, actor: ObjectId, reference: NativeHeapRef, distance: u32) {
    let state = arena.derived_actor_mut(actor).unwrap();
    state.target_148 = Some(reference);
    state.word_14c = Some(distance);
}
fn forward(arena: &mut ObjectArena, actor: ObjectId, words: [f32; 3]) {
    for (index, value) in [7, 8, 9].into_iter().zip(words) {
        arena.actor_mut(actor).unwrap().rotating.basis.words[index] = value.to_bits();
    }
}
fn route_case(
    prior: [u16; 3],
    point: [u16; 3],
    actor_point: [u16; 3],
    basis: [f32; 3],
) -> (ObjectArena, ObjectId, Arc<RuntimeAllocation>) {
    let (mut arena, actor, owner) = common(true);
    coefficient(&owner, current(0).kind, 1);
    node(&mut arena, prior);
    let object = node(&mut arena, point);
    let reference = object_target(&mut arena, object);
    install(&mut arena, actor, reference, 0);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some(actor_point);
    forward(&mut arena, actor, basis);
    (arena, actor, owner)
}

#[test]
fn direct_fc_null_and_signed_distance_gates_bypass_unreached_heap_rng_and_positions() {
    for profile in [
        FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        FamilyBehaviorProfile::Selector35,
    ] {
        let (mut arena, actor, owner) = retained_fixture(profile);
        coefficient(&owner, current(0).kind, 1);
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        let before = format!("{arena:?}");
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(0),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(1)
        );
        assert_eq!(format!("{arena:?}"), before);
        let reference = fixed(&mut arena);
        arena.release_actor_service(reference).unwrap();
        install(&mut arena, actor, reference, 257);
        arena.derived_actor_mut(actor).unwrap().word_164 = -1;
        let before = format!("{arena:?}");
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(0),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(-257)
        );
        assert_eq!(format!("{arena:?}"), before);
        //Signed14C, not unsigned: bit31-set reaches the actual released target.
        arena.derived_actor_mut(actor).unwrap().word_14c = Some(0x8000_0000);
        assert!(matches!(
            arena.score_family_descriptor(
                actor,
                current(0),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Err(FamilyDescriptorScoreError::Object(_))
        ));
    }
}

#[test]
fn unknown_distance_precedes_target_custody_and_common_rng() {
    let (mut arena, actor, owner) = common(true);
    coefficient(&owner, current(0).kind, 1);
    let reference = fixed(&mut arena);
    arena.release_actor_service(reference).unwrap();
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 0;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            current(-1),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::Uninitialized {
            actor,
            offset: 0x14c
        })
    );
    assert_eq!(format!("{arena:?}"), before);
    coefficient(&owner, current(0).kind, 0);
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(0)
    );
}

#[test]
fn direct_fc_queries_actual_route44_and_keeps_other_layouts_required() {
    let (mut arena, actor, owner) = retained_fixture(FamilyBehaviorProfile::Selector12 {
        behavior_present: false,
    });
    coefficient(&owner, current(0).kind, 1);
    let route = node(&mut arena, [1, 2, 3]);
    let reference = object_target(&mut arena, route);
    install(&mut arena, actor, reference, 256);
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(1)
    );
    arena.route_mut(route).unwrap().distance_squared = 0x8000_0000;
    arena.derived_actor_mut(actor).unwrap().word_164 = 300;
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(44)
    );
    let ordinary = arena.allocate(ObjectClassKind::Base);
    arena.owned_mut(ordinary).unwrap().state.base.position_words = Some([7, 8, 9]);
    let reference = object_target(&mut arena, ordinary);
    install(&mut arena, actor, reference, 0);
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Err(FamilyDescriptorScoreError::RequiredReferent44 {
            object: ordinary,
            address: 0x0042_735a
        })
    );
}

#[test]
fn common_rng_is_lazy_and_retains_exact_one_draw_on_both_threshold_arms() {
    let (mut arena, actor, owner) = common(true);
    coefficient(&owner, current(0).kind, 1);
    arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 0;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Err(FamilyDescriptorScoreError::RequiredRandom {
            actor,
            address: 0x0042_4f77
        })
    );
    assert_eq!(format!("{arena:?}"), before);
    let mut high = 0xffff;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            current(0),
            FamilyDescriptorScoreRng::Available(&mut high)
        ),
        Ok(1)
    );
    assert_eq!(high, 0xd56d_0006);
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 0);
    let mut low = 0;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            current(0),
            FamilyDescriptorScoreRng::Available(&mut low)
        ),
        Err(FamilyDescriptorScoreError::MissingCurrentTarget {
            actor,
            address: 0x0042_4f86
        })
    );
    assert_eq!(low, 0x0013_d573);
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 0);
}

#[test]
fn successful_fresh_null_referent_query_writes260_and_nonzero260_skips_rng() {
    for selector36 in [false, true] {
        let (mut arena, actor, owner) = common(selector36);
        coefficient(&owner, current(0).kind, 0xffff);
        let reference = fixed(&mut arena);
        install(&mut arena, actor, reference, 0);
        arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 7;
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(i32::MAX),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(0xffffi32.wrapping_add(i32::MAX))
        );
        assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
        //The next reached retained read is live, not the earlier coefficient.
        coefficient(&owner, current(0).kind, 3);
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(-3),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(0)
        );
    }
}

#[test]
fn common_fresh_heap_failure_precedes260_but_route_and_position_failures_keep260() {
    let (mut arena, actor, owner) = common(true);
    coefficient(&owner, current(0).kind, 1);
    let reference = fixed(&mut arena);
    arena.release_actor_service(reference).unwrap();
    install(&mut arena, actor, reference, 257);
    arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 9;
    assert!(matches!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Err(FamilyDescriptorScoreError::Object(_))
    ));
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 9);
    let route = node(&mut arena, [1, 2, 3]);
    let reference = object_target(&mut arena, route);
    install(&mut arena, actor, reference, 0);
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Err(FamilyDescriptorScoreError::Object(
            ObjectError::RouteUnfinalized(route)
        ))
    );
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
    let second = node(&mut arena, [4, 5, 6]);
    let reference = object_target(&mut arena, second);
    install(&mut arena, actor, reference, 0);
    arena.owned_mut(second).unwrap().state.base.position_words = None;
    arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 9;
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Err(FamilyDescriptorScoreError::Object(
            ObjectError::RoutePositionUnknown(second)
        ))
    );
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
}

#[test]
fn ordinary_previous_null_returns_after260_without_actor_position_or_basis_work() {
    let (mut arena, actor, owner) = common(true);
    coefficient(&owner, current(0).kind, 1);
    let route = node(&mut arena, [1, 2, 3]);
    let row = arena.route_mut(route).unwrap();
    row.identity_word = 0;
    row.predecessor_word = 0;
    arena.finalize_routes().unwrap();
    assert_eq!(arena.route_previous(route).unwrap(), None);
    let reference = object_target(&mut arena, route);
    install(&mut arena, actor, reference, 0);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 5;
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(1)
    );
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
}

#[test]
fn literal_current_x_geometry_and_signed_word_cross_threshold_are_preserved() {
    //Replacing CURRENT targetX with previousX would spuriously clear this byte.
    let (mut arena, actor, _) = route_case([0, 0, 0], [512, 512, 0], [0, 0, 0], [1.0, 1.0, 0.0]);
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(1)
    );
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
    //Two signedLOWWORD -32768 cross components sum to0x80000000. A signed
    //squared-sum comparison would wrongly reach the aligned forward and clear260.
    let (mut arena, actor, _) =
        route_case([0, 0, 0], [1, 0, 0], [1, 0x8000, 0x8000], [1.0, 0.0, 0.0]);
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(1)
    );
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 1);
    for (x, expected) in [(255u16, 0u8), (256, 1), (0x8000, 1), (0xffff, 0)] {
        let (mut arena, actor, _) = route_case([0, 0, 0], [0, 1, 0], [x, 0, 0], [0.0, 1.0, 0.0]);
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(0),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(1)
        );
        assert_eq!(
            arena.family4236d0_actor(actor).unwrap().byte_260,
            expected,
            "x={x:04x}"
        );
    }
}

#[test]
fn strict_dot_equality_nan_and_word_wrapped_direction_keep_native_outcomes() {
    for (bits, expected) in [
        (0x3f66_6666u32, 1u8),
        (0x3f66_6667, 0),
        (f32::NAN.to_bits(), 1),
    ] {
        let (mut arena, actor, _) = route_case(
            [0, 0, 0],
            [1, 0, 0],
            [1, 0, 0],
            [f32::from_bits(bits), 0.0, 0.0],
        );
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(0),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(1)
        );
        assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, expected);
    }
    let (mut arena, actor, _) =
        route_case([0, 0, 0], [0x8000, 0, 0], [0x8000, 0, 0], [-1.0, 0.0, 0.0]);
    assert_eq!(
        arena.score_family_descriptor(actor, current(0), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(1)
    );
    assert_eq!(arena.family4236d0_actor(actor).unwrap().byte_260, 0);
}
