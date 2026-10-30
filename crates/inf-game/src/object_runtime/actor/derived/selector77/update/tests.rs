//! Supplied complete typed arena states, not captured heap defaults. Native
//! construction is real;19C/1A0 are set only in explicitly named success cases.

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorConstructionRequest, ActorRegistryConfig, ActorServiceHeapError,
        ActorServiceResourceState, ActorTerrainInputs, DerivedActorConstructionRequest,
        FixtureActorServiceAllocation, NativeFamilyUpdateHost, NativeHeapRef,
    },
};
use std::path::Path;
mod member;
mod pair;
mod target_native;

const SINE_ZERO_FIXTURE: [u16; 4096] = [0; 4096];
const INVERSE_ZERO_FIXTURE: [i16; 1024] = [0; 1024];
const NULL: ActorHandle = ActorHandle::from_raw(0x1234);

fn construction(position: Option<[u16; 3]>) -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: position,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE_ZERO_FIXTURE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

fn subject() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap();
    let actor = arena
        .allocate_selector77_actor(construction(Some([100, 200, 300])), &mut 0)
        .unwrap();
    (arena, actor)
}

fn request(dt: u32) -> Selector77TickRequest<'static> {
    Selector77TickRequest {
        microseconds: dt,
        process_null_handle: Some(NULL),
        status_load_factor: None,
        sine_quarter: &SINE_ZERO_FIXTURE,
        inverse_sine: &INVERSE_ZERO_FIXTURE,
        settings_7c: 0,
        process_velocity: None,
    }
}

fn primary(arena: &mut ObjectArena, position: Option<[u16; 3]>) -> ObjectId {
    let actor = arena
        .allocate_derived_actor(construction(position), &mut 0)
        .unwrap();
    let player = arena.add_world_player([0; 6]);
    let handle = arena.actor(actor).unwrap().handle;
    // Explicit actual player storage input: this native lookup reads only+D4.
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    actor
}

#[derive(Default)]
struct Host {
    rng: u32,
    random_calls: usize,
    fail_random: Option<usize>,
    effects: Vec<FamilyUpdateEffect>,
    weapon_target: Option<NativeHeapRef>,
    fail_effect: bool,
}

impl FamilyUpdateHost for Host {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<crate::object_runtime::ActorHandle> {
        panic!("selector77 does not run the water consumer")
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.random_calls += 1;
        if self.fail_random == Some(self.random_calls) {
            return Err("supplied RNG error");
        }
        Ok(name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        self.effects.push(effect);
        if let Some(target) = self.weapon_target {
            arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
        }
        if self.fail_effect {
            Err("supplied required effect error")
        } else {
            Ok(())
        }
    }
}
impl Selector77UpdateHost for Host {}

#[test]
fn actual_process_null_is_required_before_damage_and_status() {
    let (mut arena, actor) = subject();
    arena.actor_mut(actor).unwrap().word_c8 = 999;
    let before = format!("{arena:?}");
    let mut host = Host::default();
    assert_eq!(
        arena.tick_selector77(
            actor,
            Selector77TickRequest {
                process_null_handle: None,
                ..request(1024)
            },
            &mut host
        ),
        Err(Selector77UpdateError::MissingProcessNullHandle)
    );
    assert_eq!(format!("{arena:?}"), before);
    let other = ActorHandle::from_raw(0x8765);
    arena.selector77_update_state_mut(actor).unwrap().handle_1b0 = other;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.tick_selector77(actor, request(1024), &mut host),
        Err(Selector77UpdateError::RequiredDamage {
            actor,
            callback: ActorDamageCallback {
                adjusted_damage: 10_000,
                packet: ActorDamagePacket {
                    strength: 10_000,
                    channel: 9,
                    byte_03: None
                },
                source: other,
            }
        })
    );
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(host.random_calls, 0);
    assert!(host.effects.is_empty());
}

#[test]
fn signed_c4_gate_runs_real_status_before_skipping_unwritten_eye_state() {
    for lifecycle in [0, 2, u32::MAX] {
        let (mut arena, actor) = subject();
        arena.actor_mut(actor).unwrap().word_c4 = lifecycle;
        arena.actor_mut(actor).unwrap().word_c8 = 50;
        let before = arena.selector77_actor(actor).unwrap().clone();
        let mut host = Host::default();
        assert_eq!(
            arena.tick_selector77(actor, request(1024), &mut host),
            Ok(())
        );
        assert_eq!(arena.actor(actor).unwrap().word_c8, 18);
        assert_eq!(arena.selector77_actor(actor).unwrap(), &before);
        assert_eq!(host.random_calls, 0);
        assert!(host.effects.is_empty());
    }
}

#[test]
fn status_error_retains_c8_prefix_before_lifecycle_or_timer_reads() {
    let (mut arena, actor) = subject();
    arena.actor_mut(actor).unwrap().word_c8 = 50;
    arena.actor_mut(actor).unwrap().word_cc = 1;
    arena.actor_mut(actor).unwrap().word_c4 = 0;
    let before = arena.selector77_actor(actor).unwrap().clone();
    assert_eq!(
        arena.tick_selector77(actor, request(1024), &mut Host::default()),
        Err(Selector77UpdateError::Status(
            FamilyUpdateError::MissingStatusLoadFactor(actor)
        ))
    );
    assert_eq!(arena.actor(actor).unwrap().word_c8, 18);
    assert_eq!(arena.selector77_actor(actor).unwrap(), &before);
}

#[test]
fn equal_timer_subtracts_without_reacquisition_and_next_expiry_uses_actual_empty_player_list() {
    let (mut arena, actor) = subject();
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 64;
    let mut host = Host::default();
    assert_eq!(
        arena.tick_selector77(actor, request(64), &mut host),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x1a0
        })
    );
    assert_eq!(arena.selector77_actor(actor).unwrap().word_198, 0);
    assert_eq!(arena.selector77_actor(actor).unwrap().word_1ac, Some(1));
    assert_eq!(host.random_calls, 0);
    assert_eq!(
        arena.tick_selector77(actor, request(1), &mut host),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x1a0
        })
    );
    // Real48F060 from seed0 returns0x13; no copied fixture endpoint is fed back.
    assert_eq!(host.rng, 0x0013_d573);
    assert_eq!(arena.selector77_actor(actor).unwrap().word_198, 0x13 << 4);
    assert_eq!(arena.selector77_actor(actor).unwrap().word_1ac, Some(0));
    assert_eq!(host.random_calls, 1);
    assert_eq!(arena.selector77_actor(actor).unwrap().word_19c, None);
}

#[test]
fn far_primary_wraps_word_differences_and_publishes_unsigned_clamp_before_rng_error() {
    for (position, range, expected_timer) in [
        ([0, 0, 0], 0, 512_000),
        ([0, 0, 0], 2500, 3_000_000),
        ([0, 0, 0], -1, 512_512),
        // Three0x8000 signed deltas sum toC0000000: SHR2/isqrt/SHL1, not
        // signed sqrt(negative)=0. Literal distance56754 gives unsigned clamp.
        ([0x8000, 0x8000, 0x8000], 0, 3_000_000),
    ] {
        let (mut arena, actor) = subject();
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(if position == [0, 0, 0] {
                [if range == 2500 { 10_000 } else { 1000 }, 0, 0]
            } else {
                [0, 0, 0]
            });
        let primary = primary(&mut arena, Some(position));
        arena.derived_actor_mut(actor).unwrap().word_164 = range;
        arena.selector77_update_state_mut(actor).unwrap().word_198 = 0;
        let eye_before = arena.selector77_actor(actor).unwrap().clone();
        let primary_before = arena.object(primary).unwrap().clone();
        let mut host = Host {
            fail_random: Some(1),
            ..Host::default()
        };
        assert_eq!(
            arena.tick_selector77(actor, request(1), &mut host),
            Err(Selector77UpdateError::Random {
                address: 0x0040_76be,
                error: "supplied RNG error"
            })
        );
        let mut expected = eye_before;
        expected.word_198 = expected_timer;
        assert_eq!(arena.selector77_actor(actor).unwrap(), &expected);
        assert_eq!(arena.object(primary).unwrap(), &primary_before);
        assert_eq!(host.rng, 0);
        assert_eq!(host.random_calls, 1);
    }
}

#[test]
fn successful_far_timer_adds_one_real_rng_before_reaching_strict_idle_history() {
    let (mut arena, actor) = subject();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([1000, 0, 0]);
    primary(&mut arena, Some([0, 0, 0]));
    arena.derived_actor_mut(actor).unwrap().word_164 = 0;
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 0;
    let mut host = Host::default();
    assert_eq!(
        arena.tick_selector77(actor, request(1), &mut host),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x1a0
        })
    );
    assert_eq!(
        arena.selector77_actor(actor).unwrap().word_198,
        512_000 + (0x13 << 3)
    );
    assert_eq!(arena.selector77_actor(actor).unwrap().word_1ac, Some(0));
    assert_eq!(host.random_calls, 1);
}

#[test]
fn reacquisition_uses_only_first_primary_d4_without_secondary_or_next_player_fallback() {
    let (mut arena, actor) = subject();
    let other = primary(&mut arena, None);
    let first = arena.first_player().unwrap();
    let handle = arena.actor(other).unwrap().handle;
    let player = arena.world_player_mut(first).unwrap();
    player.handle_d4 = NULL;
    player.handle_d8 = handle;
    let second = arena.add_world_player([0; 6]);
    arena.world_player_mut(second).unwrap().handle_d4 = handle;
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 0;
    let before = arena.object(other).unwrap().clone();
    let mut host = Host::default();
    assert_eq!(
        arena.tick_selector77(actor, request(1), &mut host),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x1a0
        })
    );
    assert_eq!(host.random_calls, 1);
    assert_eq!(arena.selector77_actor(actor).unwrap().word_198, 0x13 << 4);
    assert_eq!(arena.object(other).unwrap(), &before);
}

#[test]
fn real_near_primary_requires_allocation_before_handle_rng_or_idle_publication() {
    let (mut arena, actor) = subject();
    let primary = primary(&mut arena, Some([100, 200, 300]));
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 0;
    let before = arena.selector77_actor(actor).unwrap().clone();
    let primary_before = arena.object(primary).unwrap().clone();
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    let mut host = Host::default();
    assert_eq!(
        arena.tick_selector77(actor, request(64), &mut host),
        Err(Selector77UpdateError::Object(
            ObjectError::ActorServiceHeap(ActorServiceHeapError::FixtureFailure(71))
        ))
    );
    assert_eq!(arena.selector77_actor(actor).unwrap(), &before);
    assert_eq!(arena.object(primary).unwrap(), &primary_before);
    assert_eq!(host.random_calls, 0);
    assert!(host.effects.is_empty());
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
}

#[test]
fn expiry_releases_old_target_before_player_lookup_and_retains_failed_retry() {
    let (mut arena, actor) = subject();
    let data = NativeTarget::FixedPosition(arena.fixed_position_target(None).unwrap());
    let target = arena.import_fixture_target(data);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 0;
    let mut host = Host::default();
    assert_eq!(
        arena.tick_selector77(actor, request(1), &mut host),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x1a0
        })
    );
    assert_eq!(
        arena.actor_service_heap().state(target).unwrap(),
        ActorServiceResourceState::Released
    );
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, None);
    assert_eq!(host.random_calls, 1);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 0;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.tick_selector77(actor, request(1), &mut host),
        Err(Selector77UpdateError::Object(
            ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(target))
        ))
    );
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(host.random_calls, 1);
}

#[test]
fn weapon_callback_is_real_and_target_is_captured_after_it_before_limit_publication() {
    let (mut arena, actor) = subject();
    arena.derived_actor_mut(actor).unwrap().byte_13d = Some(1);
    arena.admit_derived_actor(actor).unwrap();
    {
        let behavior = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        behavior.groups_1c[0].word_0c = 7;
        behavior.entries_18[0][1] = 128;
        behavior.word_38 = 32;
        behavior.word_2c = 0x1234;
    }
    let mut process_rng = 0;
    assert_eq!(
        arena.tick_selector77(
            actor,
            request(64),
            &mut NativeFamilyUpdateHost {
                process_rng: &mut process_rng,
                sine_quarter: &SINE_ZERO_FIXTURE,
            }
        ),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x1a0
        })
    );
    let behavior = arena
        .derived_actor(actor)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    assert_eq!(behavior.groups_1c[0].word_0c, 5);
    assert_eq!(behavior.entries_18[0][1], 64);
    assert_eq!(behavior.word_38, 0);
    assert_eq!(behavior.word_2c, 0x1234);
    assert_eq!(process_rng, 0);
    let data = NativeTarget::FixedPosition(arena.fixed_position_target(None).unwrap());
    let target = arena.import_fixture_target(data);
    let mut host = Host {
        weapon_target: Some(target),
        ..Host::default()
    };
    assert_eq!(
        arena.tick_selector77(actor, request(64), &mut host),
        Err(Selector77UpdateError::RequiredTargetWords {
            reference: target,
            field: "focus08"
        })
    );
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(target));
    assert_eq!(arena.selector77_actor(actor).unwrap().word_1ac, Some(1));
    assert_eq!(
        host.effects,
        [FamilyUpdateEffect::WeaponTick { microseconds: 64 }]
    );
    assert_eq!(arena.selector77_actor(actor).unwrap().word_1a0, None);
}

#[test]
fn weapon_error_retains_timer_and_synchronous_mutation_without_later_limit() {
    let (mut arena, actor) = subject();
    arena.derived_actor_mut(actor).unwrap().byte_13d = Some(1);
    arena.admit_derived_actor(actor).unwrap();
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 100;
    let data = NativeTarget::FixedPosition(arena.fixed_position_target(None).unwrap());
    let target = arena.import_fixture_target(data);
    let effect = FamilyUpdateEffect::WeaponTick { microseconds: 64 };
    let mut host = Host {
        weapon_target: Some(target),
        fail_effect: true,
        ..Host::default()
    };
    assert_eq!(
        arena.tick_selector77(actor, request(64), &mut host),
        Err(Selector77UpdateError::Host {
            effect,
            error: "supplied required effect error"
        })
    );
    assert_eq!(arena.selector77_actor(actor).unwrap().word_198, 36);
    assert_eq!(arena.selector77_actor(actor).unwrap().word_1ac, None);
    assert_eq!(arena.derived_actor(actor).unwrap().target_148, Some(target));
    assert_eq!(host.random_calls, 0);
}

#[test]
fn supplied_idle_history_reaches_strict19c_fault_after_real_phase_and_controls() {
    let (mut arena, actor) = subject();
    arena.selector77_update_state_mut(actor).unwrap().word_1a0 = Some(u32::MAX);
    arena
        .selector77_update_state_mut(actor)
        .unwrap()
        .words_184_190[1] = 1234;
    let mut host = Host::default();
    assert_eq!(
        arena.tick_selector77(actor, request(64), &mut host),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x19c
        })
    );
    let state = arena.selector77_actor(actor).unwrap();
    assert_eq!(state.word_1a0, Some(63));
    assert_eq!(state.word_1ac, Some(1));
    assert_eq!(state.words_184_190, [0, 1234, 0, 0, 0, 0, 0]);
    assert_eq!((state.word_1a4, state.word_1a8), (0, 0));
    assert_eq!(state.word_19c, None);
    assert_eq!(host.random_calls, 0);
}

#[test]
fn explicitly_supplied_history_first_selects_extension_then_next_tick_moves190() {
    let (mut arena, actor) = subject();
    {
        let state = arena.selector77_update_state_mut(actor).unwrap();
        state.word_1a0 = Some(0);
        state.word_19c = Some(0xfffe);
    }
    let base = arena.object(actor).unwrap().base.clone();
    let mut host = Host::default();
    assert_eq!(arena.tick_selector77(actor, request(64), &mut host), Ok(()));
    let state = arena.selector77_actor(actor).unwrap();
    assert_eq!(state.word_19c, Some(0));
    assert_eq!(state.word_1a0, Some(64));
    assert_eq!(state.words_184_190, [0, 0, 0xc00, 0, 0, 0, 0]);
    assert_eq!((state.word_1a4, state.word_1a8), (0x806, 0x3029));
    assert_eq!(host.rng, 0x60a7_666a);
    assert_eq!(host.random_calls, 2);
    // A larger actual second delta moves the selected target without RNG.
    arena.selector77_update_state_mut(actor).unwrap().word_198 = 2_000_000;
    assert_eq!(
        arena.tick_selector77(actor, request(32768), &mut host),
        Ok(())
    );
    let state = arena.selector77_actor(actor).unwrap();
    assert_eq!(state.words_184_190[6], 385);
    assert_eq!(state.word_1a4, 0x806 - 385);
    assert_eq!(host.random_calls, 2);
    assert_eq!(state.word_19e, None);
    assert_eq!(arena.object(actor).unwrap().base, base);
}

#[test]
fn extension_second_rng_failure_preserves_first_target_and_pitch_zero_delta_is_not_bypassed() {
    let (mut arena, actor) = subject();
    arena.selector77_update_state_mut(actor).unwrap().word_19c = Some(0);
    arena.selector77_update_state_mut(actor).unwrap().word_1a0 = Some(0);
    let mut host = Host {
        fail_random: Some(2),
        ..Host::default()
    };
    assert_eq!(
        arena.tick_selector77(actor, request(0), &mut host),
        Err(Selector77UpdateError::Random {
            address: 0x0040_8186,
            error: "supplied RNG error"
        })
    );
    let state = arena.selector77_actor(actor).unwrap();
    assert_eq!(
        (state.word_1a0, state.word_19c, state.word_1ac),
        (Some(0), Some(0), Some(0))
    );
    assert_eq!(state.words_184_190[2], 0xc00);
    assert_eq!(state.words_184_190[6], 0);
    assert_eq!((state.word_1a4, state.word_1a8), (0x806, 0));
    assert_eq!(host.rng, 0x0013_d573);
}

#[test]
fn negative_extension_clamps_to_remaining_and_keeps_untouched_pose_words() {
    let (mut arena, actor) = subject();
    {
        let state = arena.selector77_update_state_mut(actor).unwrap();
        state.word_19c = Some(0);
        state.word_1a4 = (-9i32) as u32;
        state.word_1a8 = 32768;
        state.words_184_190 = [11, 22, 33, 44, 55, 66, 100];
    }
    let mut host = Host::default();
    arena
        .selector77_extension(actor, 320, &SINE_ZERO_FIXTURE, &mut host)
        .unwrap();
    let state = arena.selector77_actor(actor).unwrap();
    assert_eq!(state.words_184_190, [11, 22, 0xc00, 44, 55, 66, 91]);
    assert_eq!(state.word_1a4, 0);
    assert_eq!(state.word_1a8, 32768);
    assert_eq!(host.random_calls, 0);
}

#[test]
fn steering_lazy_limit_and_signed_word_clamps_follow_true_native_thresholds() {
    let (mut arena, actor) = subject();
    assert_eq!(
        arena.selector77_steering::<&str>(actor, 119, -119),
        Ok(true)
    );
    assert_eq!(
        arena.selector77_steering::<&str>(actor, 120, 0),
        Err(Selector77UpdateError::RequiredField {
            actor,
            offset: 0x1ac
        })
    );
    {
        let state = arena.selector77_update_state_mut(actor).unwrap();
        state.word_1ac = Some(1024);
        state.words_184_190[1] = (-0x4000i16) as u16;
        state.words_184_190[5] = 0x3000;
    }
    assert_eq!(
        arena.selector77_steering::<&str>(actor, 2000, 1000),
        Ok(false)
    );
    assert_eq!(
        arena.selector77_actor(actor).unwrap().words_184_190,
        [0, (-0x4000i16) as u16, 0, 512, 0, 0x3000, 0]
    );
    assert_eq!(wrapping_abs(i32::MIN), i32::MIN);
}

#[test]
fn supplied_quarter_table_preserves_reverse_quadrants_low_two_bits_and_unsigned_words() {
    let sine = std::array::from_fn(|i| (i * 8) as u16);
    for (phase, expected) in [
        (0, 0),
        (3, 0),
        (4, 8),
        (0x3ffc, 32760),
        (0x4000, 32760),
        (0x4004, 32752),
        (0x7ffc, 0),
        (0x8004, -8),
        (0xc000, -32760),
        (0x1c000, -32760),
    ] {
        assert_eq!(selector77_sine(phase, &sine), expected);
    }
    let mut high = [0; 4096];
    high[1] = 0xffff;
    assert_eq!(selector77_sine(4, &high), 65535);
    assert_eq!(selector77_sine(0x8004, &high), -65535);
}

#[test]
fn original_complete_eye_update_bodies_and_actual_vtable_slots_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(exe).unwrap();
    // This supported PE's .text/.rdata raw offset equals RVA; whole EXE
    // guard prevents treating arbitrary images as an addressable source.
    for (start, end, expected) in [
        (
            0x00407410,
            0x00407489,
            "de7cb9f1b1d315c9b03dbd4c9ebe61dca2f3fc2379d5f2261099479b707342b2",
        ),
        (
            0x00407ca0,
            0x00407e62,
            "5c880bc026337661ea757693b0419216334ab5a24c6e6d91c50181e9446767fb",
        ),
        (
            0x00407600,
            0x0040775b,
            "76054b8d2ad84296b96ea8d0270b1fc95f1b0f5db224fbf839410414bf5b0df5",
        ),
        (
            0x00407760,
            0x00407aff,
            "ece9583fbf633e92080a8ad680b35d13165e2fe9d7b9c4d9b7a29f8ccac84a9c",
        ),
        (
            0x00407b00,
            0x00407bac,
            "5bf4baced5f76c0438c3497dffd6354a833ac9078fddcbe92541f7f628b36c92",
        ),
        (
            0x004080c0,
            0x004081a3,
            "2c4fa2b41cf4960474794b966e67339faf5364a7910b38d8530af2e0bf40f417",
        ),
        (
            0x004081b0,
            0x0040830c,
            "b33a3857753b962e48fe3fc09da6af369f2980fa3b815bc3b5209f181595ea85",
        ),
        (
            0x0046b030,
            0x0046b054,
            "d0c47f1b6483e919225dd5c28fc47dfbf4175f4e7e8855e95422a6294da14cfa",
        ),
        (
            0x0047c990,
            0x0047cad1,
            "0187a363d4b4417778f67f42889ba61a59a7fceab4eeeaf8433f5c7ed1d2daf7",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x0040_0000..end - 0x0040_0000])
            ),
            expected
        );
    }
    for (offset, target) in [(0x24, 0x0040_7ca0u32), (0xd4, 0x0040_80c0)] {
        let raw = 0x0051_fbb0usize - 0x0040_0000 + offset;
        assert_eq!(
            u32::from_le_bytes(exe[raw..raw + 4].try_into().unwrap()),
            target
        );
    }
}
