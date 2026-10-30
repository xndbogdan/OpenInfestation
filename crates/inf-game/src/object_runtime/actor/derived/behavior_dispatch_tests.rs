use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};

const SINE: [u16; 4096] = [0; 4096];
const PROFILE: FamilyBehaviorProfile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };

fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let actor = arena
        .allocate_family428650_actor(
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
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    arena.derived_actor_mut(actor).unwrap().sound_words_17c_182 = Some([0; 4]);
    (arena, actor)
}

fn request(time: u32) -> FamilyBehaviorDispatchRequest {
    FamilyBehaviorDispatchRequest {
        microseconds: time,
        profile: Some(PROFILE),
        periodic_scan: true,
    }
}

#[derive(Default)]
struct Host {
    events: Vec<String>,
    randoms: std::collections::VecDeque<u32>,
    scores: [i32; 8],
    fail: Option<&'static str>,
    bias: i32,
    expected_process_null: Option<ActorHandle>,
}

impl FamilyBehaviorHost for Host {
    type Error = &'static str;
    type PeriodicInput = ();
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.events.push("random".into());
        Ok(self.randoms.pop_front().expect("no unaccounted RNG draw"))
    }
    fn periodic_probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyPeriodicProbeRequest,
        input: Option<Self::PeriodicInput>,
    ) -> Result<u32, Self::Error> {
        assert!(input.is_none());
        let index = request.index;
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_220,
            Some(index)
        );
        self.events.push(format!("probe {index}"));
        Ok(0)
    }
    fn periodic_input_factory(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
    ) -> Result<Option<Self::PeriodicInput>, Self::Error> {
        Err("periodic input factory unowned")
    }
    fn release_periodic_input(
        &mut self,
        _: &mut ObjectArena,
        _: Self::PeriodicInput,
    ) -> Result<(), Self::Error> {
        Err("periodic input release unowned")
    }
    fn score(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
        _profile: FamilyBehaviorProfile,
        index: i32,
        current: bool,
        bias: i32,
    ) -> Result<i32, Self::Error> {
        self.events.push(format!("score {index} {current} {bias}"));
        Ok(self.scores[index as usize])
    }
    fn candidate_bias(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<i32, Self::Error> {
        self.events.push("bias".into());
        Ok(self.bias)
    }
    fn change(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _profile: FamilyBehaviorProfile,
        index: i32,
        entering: bool,
    ) -> Result<(), Self::Error> {
        self.events.push(format!("change {index} {entering}"));
        if entering {
            assert_eq!(arena.family428650_actor(actor).unwrap().word_184, -1);
            assert_eq!(
                arena.family428650_actor(actor).unwrap().words_204_208[0],
                Some(self.bias as u32)
            );
        }
        if self.fail == Some(if entering { "entry" } else { "exit" }) {
            return Err("required descriptor callback");
        }
        Ok(())
    }
    fn update(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _profile: FamilyBehaviorProfile,
        index: i32,
        microseconds: u32,
    ) -> Result<(), Self::Error> {
        assert_eq!(arena.family428650_actor(actor).unwrap().word_184, index);
        if let Some(expected) = self.expected_process_null {
            let state = arena.family428650_actor(actor).unwrap();
            assert_eq!(state.word_210, 0);
            assert_eq!(state.word_20c, expected.raw());
        }
        self.events.push(format!("update {index} {microseconds}"));
        Ok(())
    }
    fn occasional_sound(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
        slot: u16,
    ) -> Result<(), Self::Error> {
        self.events.push(format!("sound {slot}"));
        Ok(())
    }
}

#[test]
fn first_scan_preserves_unknown_cursor_after_reached_rng_timer_and_target_decay() {
    let (mut arena, actor) = fixture();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_22e = 0x0c;
    state.count_1a0 = 2;
    state.entries_1a4[0].word_04 = 0;
    state.entries_1a4[0].word_08 = 100;
    state.entries_1a4[1].word_04 = 101;
    state.entries_1a4[1].word_08 = 99;
    let mut host = Host {
        randoms: [0xffff_0007].into(),
        ..Default::default()
    };
    assert_eq!(
        arena.dispatch_family_behavior(actor, request(100), &mut host),
        Err(FamilyBehaviorError::Uninitialized {
            actor,
            offset: 0x220
        })
    );
    let state = arena.family428650_actor(actor).unwrap();
    assert_eq!(state.word_21c, 1_000_028);
    assert_eq!(state.word_22e, 0x0d);
    assert_eq!(state.word_220, None);
    assert_eq!(state.words_188_19c, [None; 6]);
    assert_eq!(
        (state.entries_1a4[0].word_04, state.entries_1a4[0].word_08),
        (1, 1)
    );
    assert_eq!(
        (state.entries_1a4[1].word_04, state.entries_1a4[1].word_08),
        (1, 1)
    );
    assert_eq!(host.events, ["random"]);
}

#[test]
fn full_scan_wraps_word_before_modulo_and_selects_first_strict_positive_tie() {
    let (mut arena, actor) = fixture();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_22e = 8;
    state.word_220 = Some(u16::MAX);
    let mut host = Host {
        randoms: [9, 1, 7].into(),
        scores: [0, 0, 10, 10, 0, -1, 0, 0],
        bias: 37,
        ..Default::default()
    };
    arena
        .dispatch_family_behavior(actor, request(40_000), &mut host)
        .unwrap();
    assert_eq!(
        &host.events[..5],
        ["random", "probe 0", "probe 1", "probe 2", "random"]
    );
    assert_eq!(
        &host.events[host.events.len() - 3..],
        ["change 2 true", "random", "update 2 40000"]
    );
    let state = arena.family428650_actor(actor).unwrap();
    assert_eq!(state.word_184, 2);
    assert_eq!(state.word_220, Some(2));
    assert_eq!(state.word_21c, 1_000_036);
    assert_eq!(state.word_22e, 0);
    assert_eq!(state.words_188_19c[0], Some(40_000));
    assert_eq!(state.words_188_19c[5], Some((46_875 + 7) << 6));
    assert_eq!(state.words_204_208[0], Some(37));
}

#[test]
fn failing_entry_keeps_selected_bias_but_does_not_publish_index_or_elapsed() {
    let (mut arena, actor) = fixture();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_22e = 1;
    state.word_21c = 1_000_000;
    // Bit 1 resets +21C before scanning, so this fixture must supply a
    // cursor and both actual draws to reach its intended entry callback.
    state.word_220 = Some(0);
    let mut host = Host {
        randoms: [1, 1].into(),
        scores: [0, 0, 10, 0, 0, 0, 0, 0],
        bias: 51,
        fail: Some("entry"),
        ..Default::default()
    };
    assert!(matches!(
        arena.dispatch_family_behavior(actor, request(100), &mut host),
        Err(FamilyBehaviorError::Host {
            effect: "behavior entry",
            ..
        })
    ));
    let state = arena.family428650_actor(actor).unwrap();
    assert_eq!(state.word_184, -1);
    assert_eq!(state.words_188_19c[0], None);
    assert_eq!(state.words_188_19c[5], None);
    assert_eq!(state.words_204_208[0], Some(51));
    assert_eq!(state.word_228, 0x0053_1230);
    assert_eq!(state.word_22e, 0);
}

#[test]
fn lock_skips_scans_and_selection_but_runs_update_and_wrapping_elapsed_tail() {
    let (mut arena, actor) = fixture();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_184 = 3;
    state.word_22e = 0x87;
    state.words_188_19c[0] = Some(u32::MAX - 5);
    state.word_21c = 1000;
    let mut host = Host::default();
    arena
        .dispatch_family_behavior(actor, request(10), &mut host)
        .unwrap();
    let state = arena.family428650_actor(actor).unwrap();
    assert_eq!(state.word_22e, 0x80);
    assert_eq!(state.word_21c, 1000);
    assert_eq!(state.words_188_19c[0], Some(4));
    assert_eq!(host.events, ["update 3 10"]);
}

#[test]
fn forced_exit_failure_keeps_native_preceding_index_and_flags() {
    let (mut arena, actor) = fixture();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_184 = 3;
    state.word_22e = 4;
    let mut host = Host {
        fail: Some("exit"),
        ..Default::default()
    };
    assert!(matches!(
        arena.dispatch_family_behavior(actor, request(10), &mut host),
        Err(FamilyBehaviorError::Host {
            effect: "forced behavior exit",
            ..
        })
    ));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_184, 3);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 4);
    assert_eq!(host.events, ["change 3 false"]);
}

#[test]
fn signed_decision_timer_and_current_bias_keep_native_asymmetries() {
    let (mut arena, actor) = fixture();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_184 = 2;
    state.word_21c = 1_000_000;
    state.words_188_19c[0] = Some(468_750);
    state.words_188_19c[5] = Some(u32::MAX); // signed -1 must reselect.
    state.words_204_208[0] = Some(32);
    let mut host = Host {
        randoms: [1, 7].into(),
        scores: [0, 0, 1, 0, 0, 0, 0, 0],
        ..Default::default()
    };
    arena
        .dispatch_family_behavior(actor, request(10), &mut host)
        .unwrap();
    assert!(host.events.iter().any(|event| event == "score 2 true 60"));
    assert_eq!(host.events[1], "random");
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[0],
        Some(468_760)
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_204_208[1],
        None
    );
}

#[test]
fn null_descriptor_wrapper_returns_before_any_allocation_read_on_live_actor() {
    let (mut arena, actor) = fixture();
    let before = arena.family428650_actor(actor).unwrap().clone();
    let mut host = Host::default();
    arena
        .dispatch_family_behavior(
            actor,
            FamilyBehaviorDispatchRequest {
                microseconds: 40_000,
                profile: None,
                periodic_scan: true,
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(arena.family428650_actor(actor).unwrap(), &before);
    assert!(host.events.is_empty());
}

#[test]
fn negative_embedded_count_is_native_empty_but_positive_overflow_fails_at_feature() {
    let (mut arena, actor) = fixture();
    arena.family428650_actor_mut(actor).unwrap().count_1a0 = u32::MAX;
    arena.decay_family_target_entries::<()>(actor, 100).unwrap();
    arena.family428650_actor_mut(actor).unwrap().count_1a0 = 5;
    assert_eq!(
        arena.decay_family_target_entries::<()>(actor, 100),
        Err(FamilyBehaviorError::TargetCount { actor, count: 5 })
    );
}

#[test]
fn executable_profile_tables_and_update_callbacks_are_pinned_to_retail_data() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for mode in 0..3 {
        let profile = FamilyBehaviorProfile::Family4236d0 { mode };
        let (table, count) = profile.descriptor_table().unwrap();
        assert_eq!(
            executable.word(0x0053_0790 + u32::from(mode) * 8).unwrap(),
            table
        );
        assert_eq!(
            executable.word(0x0053_0794 + u32::from(mode) * 8).unwrap(),
            count as u32
        );
    }
    for (vtable, update, behavior) in [
        (0x0052_0cc8, 0x0042_4830, 0x0042_5af0),
        (0x0052_7058, 0x0046_79f0, 0x0046_8d10),
        (0x0052_7ee0, 0x0048_1710, 0x0042_5af0),
        (0x0052_273c, 0x0043_e6d0, 0x0043_f880),
    ] {
        assert_eq!(executable.word(vtable + 0x24).unwrap(), update);
        assert_eq!(executable.word(vtable + 0xd8).unwrap(), behavior);
    }
    assert_eq!(executable.word(0x0052_0e04).unwrap(), 0x0042_5c30);
    assert_eq!(executable.word(0x0052_0e08).unwrap(), 0x0043_f990);
    assert_eq!(executable.word(0x0052_0e0c).unwrap(), 0x0043_f9b0);
    assert_eq!(executable.word(0x0052_0e10).unwrap(), 0x0043_f9d0);
    assert_eq!(executable.word(0x0052_0e14).unwrap(), 0x0042_5c60);
}

/// Explicit wrapper fixture: actual selector35 construction, locked descriptor
/// index3 and known elapsed storage avoid unrelated periodic/score owners.
fn selector35_expiry_fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
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
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_184 = 3;
    state.word_22e = 2;
    state.words_188_19c[0] = Some(17);
    state.word_20c = 0x7654_3210;
    (arena, actor)
}

#[test]
fn selector35_d8_expiry_reads_process_null_before_the_first_descriptor_callback() {
    let process_null = ActorHandle::from_raw(0xabcd_9876);
    for (timer, supplied_null, expected_timer, expected_handle) in [
        (37, Some(process_null), 0, process_null.raw()),
        (0x8000_0000, Some(process_null), 0, process_null.raw()),
        (38, None, 1, 0x7654_3210),
        (0, None, 0, 0x7654_3210),
    ] {
        let (mut arena, actor) = selector35_expiry_fixture();
        arena.family428650_actor_mut(actor).unwrap().word_210 = timer;
        let registry_before = format!("{:?}", arena.actor_handles());
        assert_ne!(arena.actor_handles().unwrap().null_handle(), process_null);
        let mut host = Host {
            expected_process_null: supplied_null,
            ..Default::default()
        };
        arena
            .dispatch_actor_family_behavior(
                actor,
                37,
                FamilyHelperHistoryInputs {
                    process_null_handle: supplied_null,
                },
                &mut host,
            )
            .unwrap();
        let state = arena.family428650_actor(actor).unwrap();
        assert_eq!(
            (state.word_210, state.word_20c),
            (expected_timer, expected_handle)
        );
        assert_eq!(state.words_188_19c[0], Some(54));
        assert_eq!(state.word_184, 3);
        assert_eq!(state.word_22e, 0);
        assert_eq!(host.events, ["update 3 37"]);
        assert_eq!(format!("{:?}", arena.actor_handles()), registry_before);
    }
}

#[test]
fn selector35_d8_missing_process_null_keeps_timer_clear_and_old_handle_before_host() {
    for timer in [37, 0x8000_0000] {
        let (mut arena, actor) = selector35_expiry_fixture();
        arena.family428650_actor_mut(actor).unwrap().word_210 = timer;
        let before = format!("{arena:?}");
        let marker = format!("word_210: {timer},");
        assert_eq!(before.matches(&marker).count(), 1);
        let expected = before.replacen(&marker, "word_210: 0,", 1);
        let mut host = Host::default();
        assert_eq!(
            arena.dispatch_actor_family_behavior(
                actor,
                37,
                FamilyHelperHistoryInputs {
                    process_null_handle: None
                },
                &mut host
            ),
            Err(FamilyBehaviorError::MissingProcessNullHandle { actor })
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_20c,
            0x7654_3210
        );
        assert_eq!(format!("{arena:?}"), expected);
        assert!(host.events.is_empty());
    }
}

#[test]
fn complete_selector35_d8_and_process_null_writer_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    assert_eq!(executable.word(0x0052_273c + 0xd8).unwrap(), 0x0043_f880);
    // Complete body through RET4: +210=0 precedes the genuine54E894 read,
    // then +20C publication and the first4294A0 callback.
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(&bytes[0x43f880 - 0x400000..0x43f902 - 0x400000])
        ),
        "369f2c2bde2cd219cce1cee611160dcc98cb1a099ba41d56a45fa677abcfd781"
    );
    assert_eq!(
        &bytes[0x43f8d7 - 0x400000..0x43f8ec - 0x400000],
        &[
            0xc7, 0x81, 0x10, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa1, 0x94, 0xe8, 0x54,
            0x00, 0x89, 0x81, 0x0c, 0x02, 0x00, 0x00,
        ]
    );
}
