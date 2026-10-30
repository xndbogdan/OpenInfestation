use super::super::super::selector10_tail::tests::{executable, fixture, resident, sound};
use super::*;
fn install(arena: &mut ObjectArena, actor: ObjectId, mode: u32) {
    arena.selector10_actor_mut(actor).unwrap().word_268 = Some(mode);
    arena.selector10_actor_mut(actor).unwrap().helper_26c = Selector10Helper::for_selector(mode);
}
fn visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    delta: u32,
    rng: &mut u32,
    sounds: &mut ProcessSoundRuntime,
) -> Result<u32, Selector10HelperTickError> {
    arena.tick_selector10_helper(
        actor,
        Selector10HelperTickRequest {
            microseconds: delta,
            executable: executable(),
            random_state: rng,
            residency: &resident(),
            sounds,
        },
    )
}
fn child(arena: &mut ObjectArena, actor: ObjectId, index: usize) -> &mut Selector10HelperChild {
    arena
        .selector10_actor_mut(actor)
        .unwrap()
        .helper_26c
        .as_mut()
        .unwrap()
        .children_14[index]
        .as_deref_mut()
        .unwrap()
}
fn xyz(arena: &mut ObjectArena, actor: ObjectId, index: usize, offset: usize, value: [u16; 3]) {
    for (i, b) in value.into_iter().flat_map(u16::to_le_bytes).enumerate() {
        child(arena, actor, index).unknown_18_35[offset - 0x18 + i] = Some(b);
    }
}
#[test]
fn genuine_four_child_constructor_path_advances_rotating_index_without_rng_audio_or_hole_reads() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    let before = arena.selector10_actor(actor).unwrap().helper_26c.clone();
    let mut rng = 0x12345678;
    let mut sounds = sound();
    for expected in [1, 2, 3, 0, 1] {
        assert_eq!(
            visit(&mut arena, actor, 125_000, &mut rng, &mut sounds).unwrap(),
            0
        );
        assert_eq!(
            arena
                .selector10_actor(actor)
                .unwrap()
                .helper_26c
                .as_ref()
                .unwrap()
                .word_08,
            expected
        );
    }
    assert_eq!(rng, 0x12345678);
    assert!(sounds.is_empty());
    let after = arena
        .selector10_actor(actor)
        .unwrap()
        .helper_26c
        .as_ref()
        .unwrap();
    let original = before.as_ref().unwrap();
    assert_eq!(after.children_14, original.children_14);
}
#[test]
fn disabled_helper_clears_only_request_bit_in_order_and_retains_index_without_pair_reads() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 3);
    let helper = arena
        .selector10_actor_mut(actor)
        .unwrap()
        .helper_26c
        .as_mut()
        .unwrap();
    helper.byte_0c = 0;
    helper.word_08 = 1;
    for index in 0..2 {
        child(&mut arena, actor, index).word_14 = 0x17;
    }
    let mut rng = 1;
    let mut sounds = sound();
    assert_eq!(
        visit(&mut arena, actor, 1, &mut rng, &mut sounds).unwrap(),
        0
    );
    assert_eq!(child(&mut arena, actor, 0).word_14, 0x15);
    assert_eq!(child(&mut arena, actor, 1).word_14, 0x15);
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        1
    );
    assert_eq!(rng, 1);
}
#[test]
fn count_two_and_extended_helper_preserve_their_first_reached_boundaries() {
    let (mut arena, actor) = fixture();
    let mut rng = 0;
    let mut sounds = sound();
    install(&mut arena, actor, 3);
    assert!(matches!(
        visit(&mut arena, actor, 1, &mut rng, &mut sounds),
        Err(Selector10HelperTickError::RequiredChildField {
            index: 1,
            offset: 0x28,
            address: 0x445d3d,
            ..
        })
    ));
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        0
    );
    // The walker's +28 divides its speed: zero is a native fault.
    install(&mut arena, actor, 4);
    arena.owned_mut(actor).unwrap().state.base.word_28 = 0;
    assert!(matches!(
        visit(&mut arena, actor, 1, &mut rng, &mut sounds),
        Err(Selector10HelperTickError::Division {
            address: 0x4477a4,
            ..
        })
    ));
    // Standing, the walker reaches gait 0 and reads its stride phase +34,
    // which neither 00446BC0 nor 00445A90 writes.
    arena.owned_mut(actor).unwrap().state.base.word_28 = 1024;
    assert!(matches!(
        visit(&mut arena, actor, 1, &mut rng, &mut sounds),
        Err(Selector10HelperTickError::RequiredHelperField {
            offset: 0x34,
            address: 0x4477d9,
            ..
        })
    ));
    let helper = arena
        .selector10_actor(actor)
        .unwrap()
        .helper_26c
        .as_ref()
        .unwrap();
    assert_eq!(helper.word_04, Some(0));
    assert_eq!(rng, 0);
    assert!(sounds.is_empty());
}
#[test]
fn partial_expiry_xyz_copy_retains_timer_flags_and_xy_before_unknown_z() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    let row = child(&mut arena, actor, 0);
    row.word_36 = 0x7fff;
    row.word_14 = 2;
    for (i, b) in [1, 2, 3, 4].into_iter().enumerate() {
        row.unknown_18_35[0x2a - 0x18 + i] = Some(b);
    }
    let mut rng = 0;
    let mut sounds = sound();
    assert!(matches!(
        visit(&mut arena, actor, 125_000, &mut rng, &mut sounds),
        Err(Selector10HelperTickError::RequiredChildField {
            index: 0,
            offset: 0x2e,
            ..
        })
    ));
    let row = child(&mut arena, actor, 0);
    assert_eq!(row.word_36, 0);
    assert_eq!(row.word_14, 10);
    assert_eq!(
        &row.unknown_18_35[..4],
        &[Some(1), Some(2), Some(3), Some(4)]
    );
    assert_eq!(rng, 0);
    assert!(sounds.is_empty());
}
#[test]
fn completed_child_audio_failure_retains_child_and_rng_prefix_without_index_publication() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    child(&mut arena, actor, 0).word_36 = 0x7fff;
    let mut rng = 0;
    let mut expected = rng;
    crate::name_entry::name_entry_rng_next(&mut expected);
    let mut sounds = sound();
    sounds.fail_next_record_allocation(71);
    assert!(matches!(
        visit(&mut arena, actor, 125_000, &mut rng, &mut sounds),
        Err(Selector10HelperTickError::Sound(
            ProcessSoundRecordError::FixtureAllocation(71)
        ))
    ));
    assert_eq!(child(&mut arena, actor, 0).word_36, 0);
    assert_eq!(child(&mut arena, actor, 0).word_14, 8);
    assert_eq!(rng, expected);
    assert!(sounds.is_empty());
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        0
    );
}
#[test]
fn actual_positive_child_return_creates_one_shot_and_uses_source_pitch() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    child(&mut arena, actor, 0).word_36 = 0x7fff;
    let mut rng = 0;
    let mut sounds = sound();
    assert_eq!(
        visit(&mut arena, actor, 125_000, &mut rng, &mut sounds).unwrap(),
        1
    );
    assert_eq!(sounds.pending_len(), 1);
    let mut expected = 0;
    crate::name_entry::name_entry_rng_next(&mut expected);
    assert_eq!(rng, expected);
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        1
    );
}
#[test]
fn null_child_is_not_success_and_disabled_prefix_clears_prior_children() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    child(&mut arena, actor, 0).word_14 = 2;
    let helper = arena
        .selector10_actor_mut(actor)
        .unwrap()
        .helper_26c
        .as_mut()
        .unwrap();
    helper.byte_0c = 0;
    helper.children_14[1] = None;
    let mut rng = 0;
    let mut sounds = sound();
    assert!(matches!(
        visit(&mut arena, actor, 1, &mut rng, &mut sounds),
        Err(Selector10HelperTickError::NullChild {
            index: 1,
            address: 0x445f3f,
            ..
        })
    ));
    assert_eq!(child(&mut arena, actor, 0).word_14, 0);
}
#[test]
fn armed_sibling_start_copies_real_xyz_and_clears_only_reached_bits() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    child(&mut arena, actor, 0).word_14 = 6;
    xyz(&mut arena, actor, 0, 0x18, [0x1111, 0x2222, 0x3333]);
    let mut rng = 0;
    let mut sounds = sound();
    assert_eq!(
        visit(&mut arena, actor, 125_000, &mut rng, &mut sounds).unwrap(),
        0
    );
    let row = child(&mut arena, actor, 0);
    assert_eq!(row.word_14, 0);
    assert!(row.word_36 > 0);
    assert_eq!(
        &row.unknown_18_35[0x30 - 0x18..],
        &[
            Some(0x11),
            Some(0x11),
            Some(0x22),
            Some(0x22),
            Some(0x33),
            Some(0x33)
        ]
    );
    assert_eq!(rng, 0);
    assert!(sounds.is_empty());
}

#[test]
fn supplied_two_child_backing_consumes_real_pair_math_and_keeps_word_velocity_units() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 3);
    xyz(&mut arena, actor, 0, 0x24, [10, 0, 0]);
    xyz(&mut arena, actor, 1, 0x24, [20, 0, 0]);
    for byte in &mut child(&mut arena, actor, 0).unknown_38_4f[0x40 - 0x38..0x44 - 0x38] {
        *byte = Some(0);
    }
    arena.actor_mut(actor).unwrap().words_8e_92 = [80, 0, 0];
    let mut rng = 0;
    let mut sounds = sound();
    assert_eq!(
        visit(&mut arena, actor, 0, &mut rng, &mut sounds).unwrap(),
        0
    );
    //dot0=800 < dot1=1600 chooses child0; 80 SAR3 - child1.X = -10.
    assert_eq!(
        &child(&mut arena, actor, 0).unknown_18_35[0x24 - 0x18..0x2a - 0x18],
        &[Some(0xf6), Some(0xff), Some(0), Some(0), Some(0), Some(0)]
    );
    assert_eq!(
        &child(&mut arena, actor, 1).unknown_18_35[0x24 - 0x18..0x2a - 0x18],
        &[Some(20), Some(0), Some(0), Some(0), Some(0), Some(0)]
    );
    assert_eq!(rng, 0);
    assert!(sounds.is_empty());
}
/// `00425CD0`, another machine's selector-10 or 21 actor: only its helper's
/// `+4` runs (the rotating index advances, nothing moves), and one without
/// a helper is left alone.
#[test]
fn a_remote_actor_runs_only_its_helper_as_00425cd0() {
    let (mut arena, actor) = fixture();
    arena.selector10_actor_mut(actor).unwrap().helper_26c = None;
    let position = arena.object(actor).unwrap().base.position_words;
    let mut rng = 0x12345678;
    let mut sounds = sound();
    for helper in [false, true] {
        if helper {
            install(&mut arena, actor, 1);
        }
        arena
            .tick_remote_selector10(
                actor,
                Selector10HelperTickRequest {
                    microseconds: 125_000,
                    executable: executable(),
                    random_state: &mut rng,
                    residency: &resident(),
                    sounds: &mut sounds,
                },
            )
            .unwrap();
    }
    let helper = arena.selector10_actor(actor).unwrap().helper_26c.as_ref();
    assert_eq!(helper.unwrap().word_08, 1);
    assert_eq!(arena.object(actor).unwrap().base.position_words, position);
    assert_eq!(rng, 0x12345678);
}
