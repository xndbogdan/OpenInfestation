//! Declared native-input fixtures. No actor/model/mission policy or retail
//! schedule is selected here; actual arena, heap, tables and callbacks execute.
use super::tests::{
    assets, construction, executable, fixture, process, visit, RequiredChildFixture,
    RequiredMissFixture,
};
use super::*;
use crate::object_runtime::{
    ActorHandle, FixedPositionTarget, FollowTargetRequest, NativeTarget, ObjectDrawProcessState,
    RetainedActorAllocation, RuntimeAllocation,
};
use std::sync::Arc;

fn tables() -> &'static (NativeBasisMath, [i16; 1024]) {
    static TABLES: std::sync::OnceLock<(NativeBasisMath, [i16; 1024])> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        (
            NativeBasisMath::new(assets().sine_table().values),
            inf_formats::parse_frontend_inverse_sine_table(
                executable().data(),
                inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
            )
            .unwrap()
            .values,
        )
    })
}
fn fixed(arena: &mut ObjectArena, point: Option<[u16; 3]>) -> NativeHeapRef {
    arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: point,
        word_10: 8,
    }))
}
fn controls(
    arena: &mut ObjectArena,
    actor: ObjectId,
    draw: &ObjectDrawProcessState,
    delta: u32,
    rng: &mut u32,
) -> Result<Option<[i32; 2]>, Selector36TailPrefixError<&'static str>> {
    let process = process();
    let mut terrain = RequiredMissFixture;
    let (math, inverse) = tables();
    let captured = arena.derived_actor(actor).unwrap().target_148.unwrap();
    arena.selector36_target_controls(
        actor,
        captured,
        &mut Selector36TailPrefixRequest {
            behavior_tables: &mut crate::object_runtime::ProcessActorBehaviorTables::from_executable(executable()).unwrap(),
            process_velocity: None,
            microseconds: delta,
            process: &process,
            settings_7c: 1,
            math,
            inverse_sine: inverse,
            shared_basis: &draw.shared_basis,
            random_state: rng,
            terrain: &mut terrain,
            child: &mut RequiredChildFixture,
        },
    )
}
fn draw() -> ObjectDrawProcessState {
    ObjectDrawProcessState::native_startup(executable()).unwrap()
}
fn inactive(arena: &mut ObjectArena, actor: ObjectId) {
    arena.derived_actor_mut(actor).unwrap().word_164 = 0;
    arena.family428650_actor_mut(actor).unwrap().byte_22c = 0;
}
fn behavior(arena: &mut ObjectArena, actor: ObjectId, cached: bool) {
    // Actual 426C80 builder with explicitly supplied admitted guard mode.
    arena.derived_actor_mut(actor).unwrap().byte_13d = Some(0x81);
    arena.admit_derived_actor(actor).unwrap();
    let position = arena.object(actor).unwrap().base.position_words.unwrap();
    let state = arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    state.byte_10 = 0;
    state.entries_18[0] = [1, 0, 0, 0, 0, 0, 0, 0];
    let group = &mut state.groups_1c[0];
    group.word_0c = if cached { 3 } else { 1 };
    group.coordinates_00_0a = [
        None,
        None,
        None,
        Some(position[0]),
        Some(position[1]),
        Some(position[2]),
    ];
}
fn following(
    arena: &mut ObjectArena,
    actor: ObjectId,
    delta: [u16; 3],
    status: u32,
) -> (ObjectId, NativeHeapRef) {
    let mut rng = 7;
    let mut request = construction();
    let p = arena.object(actor).unwrap().base.position_words.unwrap();
    request.actor.allocation_position = Some(std::array::from_fn(|i| p[i].wrapping_add(delta[i])));
    let referent = arena.allocate_selector36_actor(request, &mut rng).unwrap();
    arena.actor_mut(referent).unwrap().word_74 = 0;
    arena.actor_mut(referent).unwrap().word_c4 = status;
    let handle = arena.actor(referent).unwrap().handle;
    let reference = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(handle))
        .unwrap()
        .unwrap();
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    (referent, reference)
}
fn payload(arena: &mut ObjectArena, actor: ObjectId, flags: u8) {
    // Supplied retained allocation, not an OVL edit or constructor claim.
    let mut bytes = vec![0_u8; 0x7a];
    bytes[0x74] = flags;
    let allocation = Arc::new(
        RuntimeAllocation::new(
            bytes.into(),
            inf_formats::ByteSpan {
                start: 0,
                end: 0x7a,
            },
        )
        .unwrap(),
    );
    arena
        .selector36_tail_state_mut::<&'static str>(actor)
        .unwrap()
        .payload_274 = Some(RetainedActorAllocation::new(&allocation, 0).unwrap());
}

#[test]
fn exact_zero_primary_distance_skips_aim_secondary_and_spacing_then_native_halves() {
    let (mut arena, actor) = fixture();
    let position = arena.object(actor).unwrap().base.position_words;
    let primary = fixed(&mut arena, position);
    let released = fixed(&mut arena, None);
    arena.release_actor_service(released).unwrap();
    let d = arena.derived_actor_mut(actor).unwrap();
    d.target_148 = Some(primary);
    d.target_154 = Some(released);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = None;
    arena
        .selector36_tail_state_mut::<&'static str>(actor)
        .unwrap()
        .word_268 = 0xeeee_8001;
    arena
        .selector36_tail_state_mut::<&'static str>(actor)
        .unwrap()
        .word_26c = 0x8001_dddd;
    assert_eq!(
        visit(&mut arena, actor, &process(), 1).unwrap(),
        Selector36TailNextPhase::Motion { controls: [0, 0] }
    );
    let d = arena.derived_actor(actor).unwrap();
    assert_eq!(
        (d.word_150, d.word_14c, d.word_158, d.word_15c, d.target_154),
        (Some(4), None, None, None, Some(released))
    );
    let s = arena.selector36_actor(actor).unwrap();
    assert_eq!((s.word_268, s.word_26c), (0xeeee_4000, 0x4000_dddd));
}

#[test]
fn captured_follow_refresh_writes_actual_heap_before_primary_distance_and_controls() {
    let (mut arena, actor) = fixture();
    inactive(&mut arena, actor);
    let (referent, reference) = following(&mut arena, actor, [300, 0, 400], 4);
    arena.actor_mut(referent).unwrap().words_8e_92 = [11, 22, 33];
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(100);
    let mut rng = 99;
    assert_eq!(
        controls(&mut arena, actor, &draw(), 0, &mut rng).unwrap(),
        Some([19660, 150])
    );
    let data = arena.actor_follow_target(reference).unwrap();
    let position = arena.object(referent).unwrap().base.position_words;
    assert_eq!(
        (data.focus_08, data.focus_14, data.velocity_1a),
        (position, position, Some([11, 22, 33]))
    );
    let d = arena.derived_actor(actor).unwrap();
    assert_eq!((d.word_150, d.word_14c), (Some(4), Some(500)));
    assert_eq!(rng, 99);
}

#[test]
fn bit31_repair_and_wrapped_signed_word_geometry_are_distinct_from_overflow_flag() {
    let (mut arena, actor) = fixture();
    inactive(&mut arena, actor);
    let p = arena.object(actor).unwrap().base.position_words.unwrap();
    let primary = fixed(&mut arena, Some(p.map(|v| v.wrapping_add(32767))));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(primary);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0);
    let mut rng = 0;
    let out = controls(&mut arena, actor, &draw(), 0, &mut rng)
        .unwrap()
        .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(56754));
    assert_eq!(out, [18918, 16383]);
}

#[test]
fn primary_spacing_failure_retains_status_angles_and_distance_before_counter() {
    let (mut arena, actor) = fixture();
    inactive(&mut arena, actor);
    following(&mut arena, actor, [300, 0, 400], 4);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = None;
    let mut rng = 0;
    assert!(matches!(
        controls(&mut arena, actor, &draw(), 0, &mut rng),
        Err(Selector36TailPrefixError::RequiredField {
            offset: 0x208,
            address: 0x4819c3,
            ..
        })
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(500));
    assert_ne!(arena.selector36_actor(actor).unwrap().word_268 as u16, 0);
    assert_eq!(arena.selector36_actor(actor).unwrap().word_270, 0);
}

#[test]
fn secondary_unknown_timer_stops_after158_and_before_primary_distance_publication() {
    let (mut arena, actor) = fixture();
    inactive(&mut arena, actor);
    following(&mut arena, actor, [0, 0, 100], 4);
    let p = arena.object(actor).unwrap().base.position_words.unwrap();
    let secondary = fixed(&mut arena, Some([p[0] + 300, p[1], p[2] + 400]));
    arena.derived_actor_mut(actor).unwrap().target_154 = Some(secondary);
    let mut rng = 0;
    assert!(matches!(
        controls(&mut arena, actor, &draw(), 0, &mut rng),
        Err(Selector36TailPrefixError::RequiredField {
            offset: 0x15c,
            address: 0x4818f5,
            ..
        })
    ));
    let d = arena.derived_actor(actor).unwrap();
    assert_eq!(
        (d.word_158, d.word_14c, d.word_15c, d.target_154),
        (Some(500), None, None, Some(secondary))
    );
    assert!(arena.native_target(secondary).is_ok());
}

#[test]
fn secondary_signed_timer_branch_or_actual_release_keeps_captured_control_pair() {
    for (timer, delta, released, expected_timer) in [
        (0, 0, true, 0),
        (0x7fff_ffff, 0x8000_0000, false, 0xffff_ffff),
    ] {
        let (mut arena, actor) = fixture();
        inactive(&mut arena, actor);
        following(&mut arena, actor, [0, 0, 100], 4);
        let p = arena.object(actor).unwrap().base.position_words.unwrap();
        let secondary = fixed(&mut arena, Some([p[0] + 300, p[1], p[2] + 400]));
        let d = arena.derived_actor_mut(actor).unwrap();
        d.target_154 = Some(secondary);
        d.word_15c = Some(timer);
        arena.owned_mut(actor).unwrap().state.base.word_28 = 100;
        let mut rng = 0;
        assert_eq!(
            controls(&mut arena, actor, &draw(), delta, &mut rng).unwrap(),
            Some([19660, 200])
        );
        let d = arena.derived_actor(actor).unwrap();
        assert_eq!(
            (d.word_158, d.word_14c, d.word_15c),
            (Some(500), None, Some(expected_timer))
        );
        assert_eq!(d.target_154, if released { None } else { Some(secondary) });
        assert_eq!(arena.native_target(secondary).is_err(), released);
    }
}

#[test]
fn actual_shared_draw_basis_loan_changes_pitch_after_same_yaw_and_keeps_other_halves() {
    let mut owner = draw();
    let mut values = Vec::new();
    for flip in [false, true] {
        if flip {
            owner.shared_basis.words[5] = (-1_f32).to_bits();
        }
        let (mut arena, actor) = fixture();
        inactive(&mut arena, actor);
        following(&mut arena, actor, [300, 100, 400], 4);
        let state = arena
            .selector36_tail_state_mut::<&'static str>(actor)
            .unwrap();
        state.word_268 = 0xbeef_0000;
        state.word_26c = 0x0000_cafe;
        arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0);
        let mut rng = 0;
        controls(&mut arena, actor, &owner, 0, &mut rng).unwrap();
        let state = arena.selector36_actor(actor).unwrap();
        assert_eq!(
            (state.word_268 >> 16, state.word_26c as u16),
            (0xbeef, 0xcafe)
        );
        values.push((state.word_268 as u16, (state.word_26c >> 16) as u16 as i16));
    }
    assert_eq!(values[0].0, values[1].0);
    assert_ne!(values[0].1, 0);
    assert_eq!(values[0].1, -values[1].1);
}

#[test]
fn active_weapon_reads_actual_unknown_relative_or_cached_world_before_angle_stores() {
    for (cached, offset) in [(false, 0), (true, 6)] {
        let (mut arena, actor) = fixture();
        following(&mut arena, actor, [0, 0, 100], 3);
        arena.derived_actor_mut(actor).unwrap().word_164 = 1000;
        arena.family428650_actor_mut(actor).unwrap().byte_22c = 1;
        behavior(&mut arena, actor, cached);
        if cached {
            arena
                .derived_actor_mut(actor)
                .unwrap()
                .behavior_140
                .as_mut()
                .unwrap()
                .groups_1c[0]
                .coordinates_00_0a[3] = None;
        }
        let mut rng = 99;
        assert!(matches!(controls(&mut arena,actor,&draw(),0,&mut rng),
            Err(Selector36TailPrefixError::BehaviorPosition(ActorBehaviorPositionError::Coordinate{offset:actual,..})) if actual==offset));
        assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(3));
        assert_eq!(
            (
                arena.selector36_actor(actor).unwrap().word_268,
                arena.selector36_actor(actor).unwrap().word_26c,
                arena.derived_actor(actor).unwrap().word_14c
            ),
            (0, 0, None)
        );
        assert_eq!(rng, 99);
    }
}

#[test]
fn reached_payload_failure_does_not_turn_real_group_world_into_a_default_or_run_rng() {
    let (mut arena, actor) = fixture();
    following(&mut arena, actor, [0, 0, 100], 3);
    arena.derived_actor_mut(actor).unwrap().word_164 = 1000;
    arena.family428650_actor_mut(actor).unwrap().byte_22c = 1;
    behavior(&mut arena, actor, true);
    let mut rng = 99;
    assert!(matches!(
        controls(&mut arena, actor, &draw(), 0, &mut rng),
        Err(Selector36TailPrefixError::RequiredField {
            offset: 0x274,
            address: 0x4815da,
            ..
        })
    ));
    assert_eq!(rng, 99);
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    assert_eq!(
        arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .groups_1c[0]
            .word_0c
            & 2,
        2
    );
}

#[test]
fn genuine_timer_rng_and_expiry_stores_precede_firing_or_required_effect() {
    for expiry in [false, true] {
        let (mut arena, actor) = fixture();
        following(&mut arena, actor, [0, 0, 100], 3);
        arena.derived_actor_mut(actor).unwrap().word_164 = 1000;
        arena.family428650_actor_mut(actor).unwrap().byte_22c = 1;
        behavior(&mut arena, actor, true);
        payload(&mut arena, actor, 1);
        arena.owned_mut(actor).unwrap().state.base.flags |= 0x0200_0000;
        if expiry {
            arena.actor_mut(actor).unwrap().word_b4 = 1;
            arena
                .selector36_tail_state_mut::<&'static str>(actor)
                .unwrap()
                .word_27e = 1;
        }
        let mut rng = 99;
        let mut expected_rng = rng;
        let timer =
            ((crate::name_entry::name_entry_rng_next(&mut expected_rng) as u16) >> 6) + 1000;
        let result = controls(
            &mut arena,
            actor,
            &draw(),
            if expiry { 1024 } else { 0 },
            &mut rng,
        );
        if expiry {
            assert!(matches!(result, Err(Selector36TailPrefixError::Child(_))));
            assert_eq!(
                (arena.selector36_actor(actor).unwrap().word_27e, rng),
                (0, 99)
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(Selector36TailPrefixError::RequiredField {
                        offset: 0x208,
                        address: 0x4819c3,
                        ..
                    })
                ),
                "{result:?}"
            );
            // The zero interval skips queue admission, after all three
            // direction-spread RNG calls in the actual firing producer.
            for _ in 0..3 {
                crate::name_entry::name_entry_rng_next(&mut expected_rng);
            }
            assert_eq!(
                (arena.selector36_actor(actor).unwrap().word_27e, rng),
                (timer, expected_rng)
            );
        }
        assert_eq!(
            (
                arena.derived_actor(actor).unwrap().word_14c,
                arena.selector36_actor(actor).unwrap().word_270
            ),
            (if expiry { None } else { Some(100) }, 0)
        );
    }
}

#[test]
fn actual_weapon_failure_follows_counter_writes_and_precedes_fresh148_avoidance() {
    let (mut arena, actor) = fixture();
    behavior(&mut arena, actor, true);
    let state = arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    state.byte_0d = 2;
    state.groups_1c.truncate(1);
    state.groups_1c[0].word_0c = 3;
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, 300];
    let before = arena.family428650_actor(actor).unwrap().word_22e;
    assert!(matches!(
        visit(&mut arena, actor, &process(), 0),
        Err(Selector36TailPrefixError::BehaviorTick(
            ActorBehaviorTickError::GroupBacking {
                index: 1,
                groups: 1,
                ..
            }
        ))
    ));
    assert_eq!(arena.selector36_actor(actor).unwrap().word_270, 0x4000_4000);
    assert_eq!(
        arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .groups_1c[0]
            .word_0c,
        1
    );
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, before);
}

#[test]
fn real_primary_counter_then_shared_avoidance_retains_installed_cache_on_first_required_miss() {
    let (mut arena, actor) = fixture();
    inactive(&mut arena, actor);
    following(&mut arena, actor, [300, 0, 400], 4);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0);
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, 300];
    assert!(
        matches!(visit(&mut arena,actor,&process(),0),Err(Selector36TailPrefixError::Avoidance(
        FamilyAvoidanceError::Host{error:FamilyObstacleScanError::Host{ordinal:0,error:
            Selector12TailProbeError::Miss{actor:actual,error:crate::object_runtime::FamilyTerrainProbeMissError::Host{effect:"H12 terrain acquisition",error:"fixture miss service unavailable"},..}},..})) if actual==actor)
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(500));
    assert_eq!(arena.selector36_actor(actor).unwrap().word_270, 0x4000_4000);
    assert!(arena.family429e20_actor(actor).unwrap().probe_234.is_some());
    // Constructor-zero cache words plus genuine preparation, never a
    // fabricated successful collision result. The failed miss encoded no cell.
    assert!(arena
        .family429e20_actor(actor)
        .unwrap()
        .probe_234
        .as_ref()
        .unwrap()
        .decoded_cache()
        .words()
        .iter()
        .all(|word| *word == 0));
}
