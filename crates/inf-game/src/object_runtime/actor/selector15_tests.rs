use super::*;
use crate::object_runtime::{ActorRegistryConfig, ActorTerrainInputs};

const SINE: [u16; 4096] = [0; 4096];

fn arena_with_factory() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x55),
    })
    .unwrap();
    let object = arena
        .allocate_selector15(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 0, 0]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    (arena, object)
}

/// 0040D800, then 0042E250's zero stores, the process NULL at +188, the
/// +198 grant owner (vtable 00527818, empty mask, null notification) and the
/// +1BD/+1BF ones.
#[test]
fn constructor_writes_the_family_prefix_and_factory_defaults() {
    let (arena, object) = arena_with_factory();
    let state = arena.selector15(object).unwrap();
    assert_eq!(state.intermediate, Actor40d800State::new());
    assert_eq!(
        (state.word_158, state.word_15c, state.word_160),
        (None, 0, 0)
    );
    assert_eq!(state.words_168_174, [0; 4]);
    assert_eq!((state.word_164, state.words_178_182), (None, None));
    assert_eq!(state.handle_188, ActorHandle::from_raw(0x55));
    assert_eq!(state.grants_198.vtable_00, 0x0052_7818);
    assert_eq!(state.grants_198.mask_04, [0; GRANT_MASK_WORDS]);
    assert_eq!(state.grants_198.notification_owner_1c, 0);
    assert_eq!(
        (
            state.byte_1bc,
            state.byte_1bd,
            state.byte_1be,
            state.byte_1bf
        ),
        (0, 1, 0, 1)
    );
    assert_eq!((state.word_1b8, state.word_1c0, state.word_1c4), (0, 0, 0));
}

/// 0042E550: selectors 3..8 read +178..+182, which exist only after the
/// postinit; every other selector returns itself.
#[test]
fn model_callback_reads_the_postinit_words_for_selectors_three_to_eight() {
    let (mut arena, object) = arena_with_factory();
    let state = arena.selector15(object).unwrap();
    assert_eq!(selector15_value(state, 3), None);
    assert_eq!(selector15_value(state, 2), Some(2));
    arena.selector15_mut(object).unwrap().words_178_182 = Some([10, 11, 12, 13, 14, 15]);
    let state = arena.selector15(object).unwrap();
    for selector in 3..9 {
        assert_eq!(selector15_value(state, selector), Some(7 + selector as u16));
    }
    assert_eq!(selector15_value(state, 9), Some(9));
    assert_eq!(selector15_value(state, 0x1_0002), Some(2));
}

/// 00431DC0 lights only production states 4..6.
#[test]
fn light_is_idle_outside_production_states_four_to_six() {
    let (mut arena, object) = arena_with_factory();
    for (production, idle) in [
        (0, true),
        (3, true),
        (4, false),
        (5, false),
        (6, false),
        (7, true),
    ] {
        arena.selector15_mut(object).unwrap().word_160 = production;
        assert_eq!(
            selector15_light_is_idle(arena.selector15(object).unwrap()),
            idle
        );
    }
}

/// 0042F380 sends link codes to 0040DE70; codes 6, 0xD and 0x18 have their
/// own arms.
#[test]
fn link_codes_reach_the_family_and_own_codes_stay_boundaries() {
    let (mut arena, object) = arena_with_factory();
    for code in [6, 0xd, 0x18] {
        assert_eq!(
            arena.selector15_message(object, code),
            Err(ObjectError::LinkMessageBoundary(object))
        );
    }
    assert_eq!(
        arena.selector15_message(object, 7).unwrap(),
        FamilyLinkChange::None
    );
    let family = &arena.selector15(object).unwrap().intermediate;
    assert_eq!((family.byte_14c, family.byte_14d), (1, 1));
}

/// `0042F380` with message 0xD against the 48 factory cases of
/// `world_interactions_native.json`, run on the original instructions with
/// the real `00488AA0`, `0040D8D0` and `0040DE70`: the staff, the team, the
/// dirty bytes +38/+39 and whether the first player gets the text.
#[test]
fn smoke_matches_the_native_receipt() {
    use crate::object_runtime::{ActorMessage, ActorMessageReply, PlayerActorLookupContext};
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap();
    let cases = fixture["factory_smoke"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    let number = |value: &serde_json::Value| value.as_u64().unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let (mut arena, factory) = arena_with_factory();
        let player = case["player"]
            .as_bool()
            .unwrap()
            .then(|| arena.add_world_player([0; GRANT_MASK_WORDS]));
        let flags = number(&case["flags"]) as u32;
        arena.owned_mut(factory).unwrap().state.base.flags = flags;
        let state = &mut arena.selector15_mut(factory).unwrap().intermediate;
        state.word_140 = number(&case["staff"]) as i32;
        state.word_13c = number(&case["capacity"]) as i32;
        arena.actor_mut(factory).unwrap().word_b4 = number(&case["team"]) as u32;
        arena.owned_mut(factory).unwrap().state.base.dirty_word =
            number(&case["dirty_38"]) as u16 | (number(&case["dirty_39"]) as u16) << 8;
        let reply = arena
            .receive_actor_message(
                factory,
                ActorMessage {
                    sender: None,
                    distance_squared: 360_000,
                    code: 0xd,
                    argument: 0,
                },
                PlayerActorLookupContext {
                    process: Some(&process),
                },
                false,
            )
            .unwrap();
        let texts = case["texts"].as_array().unwrap();
        match reply {
            ActorMessageReply::PlayerText {
                player: told,
                string,
            } => {
                assert_eq!(
                    (Some(told), string, texts.len()),
                    (player, 0x30c, 1),
                    "{name}"
                );
                assert_eq!(number(&texts[0]["flag"]), 0, "{name}");
            }
            ActorMessageReply::Done(answer) => {
                assert!(texts.is_empty(), "{name}");
                assert_eq!(u64::from(answer), number(&case["answer"]), "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
        let state = &arena.selector15(factory).unwrap().intermediate;
        assert_eq!(
            state.word_140 as u64,
            number(&case["staff_after"]),
            "{name}"
        );
        assert_eq!(
            u64::from(arena.actor(factory).unwrap().word_b4),
            number(&case["team_after"]),
            "{name}"
        );
        let dirty = arena.object(factory).unwrap().base.dirty_word;
        assert_eq!(
            (u64::from(dirty & 0xff), u64::from(dirty >> 8)),
            (
                number(&case["dirty_38_after"]),
                number(&case["dirty_39_after"])
            ),
            "{name}"
        );
    }
}

/// 0040DE20 on a factory: the placement's death at its signed +78, the
/// ground scorched with argument 5, then the fuse +150: 2 s, or 1,000
/// microseconds with +14E set.
#[test]
fn a_factory_dies_through_the_family_death() {
    struct Host(Vec<(&'static str, i32)>);
    impl crate::object_runtime::FamilyDeathHost for Host {
        type Error = ();
        fn mark_death(&mut self, _: &mut ObjectArena, index: i8) -> Result<(), ()> {
            self.0.push(("mark", index.into()));
            Ok(())
        }
        fn scorch(&mut self, _: &mut ObjectArena, _: ObjectId, argument: i32) -> Result<(), ()> {
            self.0.push(("scorch", argument));
            Ok(())
        }
    }
    for (flag_14e, fuse) in [(0, 2_000_000), (1, 1_000)] {
        let (mut arena, object) = arena_with_factory();
        arena.actor_mut(object).unwrap().byte_78 = 0xfe;
        arena.selector15_mut(object).unwrap().intermediate.byte_14e = flag_14e;
        let mut host = Host(Vec::new());
        arena.retire_family40d800(object, &mut host).unwrap();
        assert_eq!(host.0, [("mark", -2), ("scorch", 5)]);
        assert_eq!(arena.family40d800(object).unwrap().word_150, fuse);
    }
}
