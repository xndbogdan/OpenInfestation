use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};

fn behavior() -> ActorBehaviorState {
    super::super::build_actor_behavior(0x81, 0, 1).unwrap()
}

fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let actor = arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([10, 20, 30]),
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
    (arena, actor)
}

#[test]
fn signed_counts_skip_missing_allocations_but_still_reach_timer() {
    let (_, actor) = fixture();
    let mut state = behavior();
    state.byte_0d = 0x80;
    state.byte_0e = 0x80;
    state.byte_0f = 0x7f;
    state.groups_1c.clear();
    state.entries_18.clear();
    state.word_38 = 20;
    state.word_2c = 0x8123_4567;
    let mut expected = state.clone();
    expected.word_38 = 15;
    tick_behavior(&mut state, actor, 5).unwrap();
    assert_eq!(state, expected);
}

#[test]
fn actual_group_flags_and_entry_dword_timers_change_in_place_only() {
    let (mut arena, actor) = fixture();
    let (mut expected_arena, expected_actor) = fixture();
    let mut state = behavior();
    state.groups_1c[0].word_0c = 7;
    state.groups_1c[1].word_0c = 0x8000_0002;
    state.entries_18[0] = [u32::MAX, 5, 10, 111, 222, 12, 333, 20];
    state.entries_18[1] = [u32::MAX, 0, u32::MAX, 111, 222, 9, 333, 10];
    state.word_38 = 50;
    state.word_2c = 0x9abc_def0;
    let mut expected = state.clone();
    expected.groups_1c[0].word_0c = 5;
    expected.groups_1c[1].word_0c = 0x8000_0000;
    expected.entries_18[0] = [u32::MAX, 0, 0, 111, 222, 2, 333, 10];
    expected.entries_18[1] = [u32::MAX, 0, u32::MAX - 10, 111, 222, 0, 333, 0];
    expected.word_38 = 40;
    arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(state);
    expected_arena
        .derived_actor_mut(expected_actor)
        .unwrap()
        .behavior_140 = Some(expected);
    arena.tick_actor_behavior(actor, 10).unwrap();
    assert_eq!(
        arena.object(actor).unwrap(),
        expected_arena.object(expected_actor).unwrap()
    );
    assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
}

#[test]
fn zero_delta_still_clears_group_flags_without_touching_known_timers() {
    let (_, actor) = fixture();
    let mut state = behavior();
    state.groups_1c[0].word_0c = 7;
    state.groups_1c[1].word_0c = 2;
    state.entries_18[0] = [99, 100, 101, 102, 103, 104, 105, 106];
    state.word_38 = 50;
    let mut expected = state.clone();
    expected.groups_1c[0].word_0c = 5;
    expected.groups_1c[1].word_0c = 0;
    tick_behavior(&mut state, actor, 0).unwrap();
    assert_eq!(state, expected);
}

#[test]
fn signed_top_timer_and_constructor_null_audio_preserve_2c() {
    let (_, actor) = fixture();
    for (timer, delta, next) in [
        (0, u32::MAX, 0),
        (u32::MAX, 1, 0),
        (5, u32::MAX, 6),
        (0x8000_0000, u32::MAX, 0),
        (10, 10, 0),
        (11, 10, 1),
    ] {
        let mut state = behavior();
        state.byte_0d = 0;
        state.byte_0e = 0;
        state.byte_0f = 0;
        state.word_38 = timer;
        state.word_2c = 0x1234_5678;
        let mut expected = state.clone();
        expected.word_38 = next;
        tick_behavior(&mut state, actor, delta).unwrap();
        assert_eq!(state, expected);
    }
}

#[test]
fn negative_group_count_skips_group_timers_but_keeps_entry_fixed_offsets() {
    let (_, actor) = fixture();
    let mut state = behavior();
    state.byte_0d = 0xff;
    state.groups_1c.clear();
    state.byte_0e = 0;
    state.byte_0f = 1;
    state.entries_18[0] = [99, 100, 101, 102, 103, 7, 105, 5];
    let mut expected = state.clone();
    expected.entries_18[0][5] = 2;
    expected.entries_18[0][7] = 0;
    tick_behavior(&mut state, actor, 5).unwrap();
    assert_eq!(state, expected);
}

#[test]
fn larger_supplied_group_count_walks_actual_adjacent_entry_backing() {
    let (_, actor) = fixture();
    let mut state = behavior();
    state.byte_0d = 8;
    let group = state.groups_1c[0].clone();
    state.groups_1c.resize(8, group);
    state.byte_0e = 1;
    state.byte_0f = 0;
    state.entries_18 = vec![
        [100, 101, 102, 103, 104, 105, 106, 107],
        [200, 201, 202, 203, 204, 205, 206, 207],
    ];
    let mut expected = state.clone();
    expected.entries_18[0] = [100, 98, 99, 100, 101, 99, 103, 101];
    expected.entries_18[1][0] = 197;
    tick_behavior(&mut state, actor, 3).unwrap();
    assert_eq!(state, expected);
}

#[test]
fn missing_group_keeps_earlier_flag_clears_before_entry_and_timer_phase() {
    let (_, actor) = fixture();
    let mut state = behavior();
    state.byte_0d = 3;
    state.groups_1c[0].word_0c = 7;
    state.groups_1c[1].word_0c = 2;
    state.word_38 = 50;
    let mut expected = state.clone();
    expected.groups_1c[0].word_0c = 5;
    expected.groups_1c[1].word_0c = 0;
    assert_eq!(
        tick_behavior(&mut state, actor, 10),
        Err(ActorBehaviorTickError::GroupBacking {
            actor,
            index: 2,
            groups: 2
        })
    );
    assert_eq!(state, expected);
}

#[test]
fn missing_second_entry_keeps_group_and_first_entry_prefix_but_not_top_timer() {
    let (_, actor) = fixture();
    let mut state = behavior();
    state.groups_1c[0].word_0c = 7;
    state.groups_1c[1].word_0c = 2;
    state.entries_18 = vec![[99, 20, 30, 40, 50, 60, 70, 80]];
    state.word_38 = 50;
    let mut expected = state.clone();
    expected.groups_1c[0].word_0c = 5;
    expected.groups_1c[1].word_0c = 0;
    expected.entries_18[0] = [99, 10, 20, 40, 50, 50, 70, 70];
    assert_eq!(
        tick_behavior(&mut state, actor, 10),
        Err(ActorBehaviorTickError::EntryBacking {
            actor,
            offset: 36,
            entries: 1
        })
    );
    assert_eq!(state, expected);
}

#[test]
fn group_timer_range_failure_preserves_its_stores_before_fixed_entry_offsets() {
    let (_, actor) = fixture();
    let mut state = behavior();
    state.byte_0d = 8;
    let group = state.groups_1c[0].clone();
    state.groups_1c.resize(8, group);
    state.byte_0e = 1;
    state.entries_18 = vec![[99, 20, 30, 40, 50, 60, 70, 80]];
    state.word_38 = 50;
    let mut expected = state.clone();
    expected.entries_18[0] = [99, 10, 20, 30, 40, 50, 60, 70];
    assert_eq!(
        tick_behavior(&mut state, actor, 10),
        Err(ActorBehaviorTickError::EntryBacking {
            actor,
            offset: 32,
            entries: 1
        })
    );
    assert_eq!(state, expected);
}

#[test]
fn missing_retained_behavior_is_required_and_leaves_actual_actor_unchanged() {
    let (mut arena, actor) = fixture();
    let before = arena.object(actor).unwrap().clone();
    assert_eq!(
        arena.tick_actor_behavior(actor, 10),
        Err(ActorBehaviorTickError::RequiredBehavior(actor))
    );
    assert_eq!(arena.object(actor).unwrap(), &before);
}

#[test]
fn original_timer_bodies_and_constructor_null_audio_are_guarded() {
    use sha2::{Digest, Sha256};
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    for (start, end, digest) in [
        (
            0x0048_cc70usize,
            0x0048_cd53usize,
            "6ac06000b2482525e1963898ef74cfa2e434ff6f5f24f003f2fd30529a65e205",
        ),
        (
            0x0048_d000,
            0x0048_d027,
            "06cbc3df012b6d2512e06fb16d7b0abfd5430a974c7776686b08e6dcbe26269b",
        ),
        (
            0x0048_d030,
            0x0048_d0c1,
            "822f819212228dceb52abf74a96aff80c18ef3d630dbb076fee0e5c43b22de82",
        ),
        (
            0x0048_d770,
            0x0048_d7cc,
            "b58a451a4ebfe6fb54a923747fbf799fe1386191a6452e65eb3cd2cdc2b568a5",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&raw[start - 0x0040_0000..end - 0x0040_0000])
            ),
            digest
        );
    }
}

/// The paused tick's release (00438950 -> virtual +7C -> 0048D000): a held
/// loop comes back for deletion and +28/+2C clear; with none held, nothing
/// changes, +2C included.
#[test]
fn the_paused_release_hands_back_a_held_loop_and_clears_28_and_2c() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    let resident = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let held = sounds
        .create_persistent(&resident, 56, 0x10000, 0x10000, 0, || {
            Ok::<_, std::convert::Infallible>([0; 3])
        })
        .unwrap();
    let (mut arena, actor) = fixture();
    let mut state = behavior();
    state.sound_28 = Some(held);
    state.word_2c = 7;
    arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(state);
    assert_eq!(arena.take_unit_behavior_sound(actor).unwrap(), Some(held));
    let state = arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    assert_eq!((state.sound_28, state.word_2c), (None, 0));
    state.word_2c = 9;
    assert_eq!(arena.take_unit_behavior_sound(actor).unwrap(), None);
    let state = arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_ref();
    assert_eq!(state.map(|state| state.word_2c), Some(9));
    arena.derived_actor_mut(actor).unwrap().behavior_140 = None;
    assert_eq!(arena.take_unit_behavior_sound(actor).unwrap(), None);
}
