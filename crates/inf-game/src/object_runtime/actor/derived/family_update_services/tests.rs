use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    ActorWaterDamage, DerivedActorConstructionRequest, FamilyUpdateError, FamilyUpdateProgress,
    FamilyUpdateRequest,
};

const SINE: [u16; 4096] = [0; 4096];

fn fixture(position: Option<[u16; 3]>) -> (ObjectArena, ObjectId) {
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
                    allocation_position: position,
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
    arena.actor_mut(actor).unwrap().words_8e_92 = [1000, 500, 100];
    arena.actor_mut(actor).unwrap().words_94_9c = [1000, 2000, 3000];
    (arena, actor)
}

#[test]
fn random_advances_only_the_supplied_process_stream() {
    let mut process_rng = 0x1234_5678;
    let mut expected = process_rng;
    let mut host = NativeFamilyUpdateHost {
        process_rng: &mut process_rng,
        sine_quarter: &SINE,
    };
    for _ in 0..4 {
        assert_eq!(host.random().unwrap(), name_entry_rng_next(&mut expected));
    }
    assert_eq!(process_rng, expected);
}

#[test]
fn delayed_return_forwards_complete_native_physics_and_never_reaches_other_effects() {
    for (delay, microseconds) in [(50_000, 20_000), (100, 1024), (1, u32::MAX)] {
        let (mut arena, actor) = fixture(Some([65535, 0, 123]));
        let (mut expected, other) = fixture(Some([65535, 0, 123]));
        arena.derived_actor_mut(actor).unwrap().word_144 = delay;
        expected.derived_actor_mut(other).unwrap().word_144 = if delay > microseconds as i32 {
            delay.wrapping_sub(microseconds as i32)
        } else {
            0
        };
        expected
            .tick_actor_physics(
                other,
                ActorPhysicsRequest {
                    microseconds,
                    mask: 0x1f,
                    sine_quarter: &SINE,
                },
            )
            .unwrap();
        let mut process_rng = 0x8765_4321;
        let mut host = NativeFamilyUpdateHost {
            process_rng: &mut process_rng,
            sine_quarter: &SINE,
        };
        assert_eq!(
            arena.update_family_prefix(
                actor,
                FamilyUpdateRequest {
                    microseconds,
                    water_plane: None,
                    status_load_factor: None
                },
                &mut host,
            ),
            Ok(FamilyUpdateProgress::Returned)
        );
        assert_eq!(
            arena.object(actor).unwrap(),
            expected.object(other).unwrap()
        );
        assert_eq!(process_rng, 0x8765_4321);
        assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
    }
}

#[test]
fn delayed_physics_error_keeps_delay_and_prior_physics_writes() {
    let (mut arena, actor) = fixture(None);
    let (mut expected, other) = fixture(None);
    arena.derived_actor_mut(actor).unwrap().word_144 = 100;
    expected.derived_actor_mut(other).unwrap().word_144 = 0;
    assert_eq!(
        expected.tick_actor_physics(
            other,
            ActorPhysicsRequest {
                microseconds: 40_000,
                mask: 0x1f,
                sine_quarter: &SINE
            },
        ),
        Err(ActorMotionError::MissingPosition(other))
    );
    let mut process_rng = 0;
    let effect = FamilyUpdateEffect::DelayedPhysics {
        microseconds: 40_000,
        mask: 0x1f,
    };
    assert_eq!(
        arena.update_family_prefix(
            actor,
            FamilyUpdateRequest {
                microseconds: 40_000,
                water_plane: None,
                status_load_factor: None
            },
            &mut NativeFamilyUpdateHost {
                process_rng: &mut process_rng,
                sine_quarter: &SINE
            },
        ),
        Err(FamilyUpdateError::Host {
            effect,
            error: NativeFamilyUpdateError::Physics(ActorMotionError::MissingPosition(actor)),
        })
    );
    assert_eq!(
        arena.object(actor).unwrap(),
        expected.object(other).unwrap()
    );
    assert_ne!(arena.actor(actor).unwrap().words_8e_92, [1000, 500, 100]);
    assert_eq!(process_rng, 0);
}

#[test]
fn every_other_required_effect_is_an_explicit_failure_without_mutation() {
    let (mut arena, actor) = fixture(Some([1, 2, 3]));
    let before = arena.object(actor).unwrap().clone();
    let mut process_rng = 123;
    let mut host = NativeFamilyUpdateHost {
        process_rng: &mut process_rng,
        sine_quarter: &SINE,
    };
    for effect in [
        FamilyUpdateEffect::Drowning(ActorWaterDamage {
            amount: 100,
            channel: 5,
            source: ActorHandle::from_raw(0x1234),
            microseconds: 40_000,
            period_microseconds: 4_000_000,
        }),
        FamilyUpdateEffect::StatusTrail {
            particle_type: 23,
            position: [1, 2, 3],
            velocity: [4, 5, 6],
            flags: 0,
        },
        FamilyUpdateEffect::Retire,
    ] {
        assert_eq!(
            host.effect(&mut arena, actor, effect),
            Err(NativeFamilyUpdateError::UnownedEffect(effect))
        );
        assert_eq!(arena.object(actor).unwrap(), &before);
    }
    assert_eq!(process_rng, 123);
}

#[test]
fn actual_weapon_tick_prefix_reaches_d8_without_allocating_targets_or_seeding_220() {
    let (mut arena, actor) = fixture(Some([1, 2, 3]));
    let mut state = crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 1).unwrap();
    state.groups_1c[0].word_0c = 7;
    state.entries_18[0] = [99, 50_000, 123, 456, 789, 60_000, 987, 40_000];
    state.word_38 = 100;
    state.word_2c = 0x1234_5678;
    let mut expected = state.clone();
    expected.groups_1c[0].word_0c = 5;
    expected.entries_18[0][1] = 10_000;
    expected.entries_18[0][5] = 20_000;
    expected.entries_18[0][7] = 0;
    expected.word_38 = 0;
    arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(state);
    let mut process_rng = 0x1234_5678;
    assert_eq!(
        arena.update_family_prefix(
            actor,
            FamilyUpdateRequest {
                microseconds: 40_000,
                water_plane: None,
                status_load_factor: None
            },
            &mut NativeFamilyUpdateHost {
                process_rng: &mut process_rng,
                sine_quarter: &SINE
            },
        ),
        Ok(FamilyUpdateProgress::BeforeBehaviorDispatch {
            class: crate::object_runtime::FamilyUpdateClass::Selector35
        })
    );
    assert_eq!(
        arena.derived_actor(actor).unwrap().behavior_140.as_ref(),
        Some(&expected)
    );
    assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
    assert_eq!(process_rng, 0x1234_5678);
    // This is the actual +24 prefix result, not a successful +D8 or class tail.
}

#[test]
fn weapon_backing_error_is_typed_after_earlier_live_stores_with_rng_held() {
    let (mut arena, actor) = fixture(Some([1, 2, 3]));
    let mut state = crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 1).unwrap();
    state.byte_0d = 2;
    state.groups_1c[0].word_0c = 7;
    state.word_38 = 50;
    let mut expected = state.clone();
    expected.groups_1c[0].word_0c = 5;
    arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(state);
    let mut process_rng = 0x1234_5678;
    let effect = FamilyUpdateEffect::WeaponTick {
        microseconds: 40_000,
    };
    assert_eq!(
        arena.update_family_prefix(
            actor,
            FamilyUpdateRequest {
                microseconds: 40_000,
                water_plane: None,
                status_load_factor: None
            },
            &mut NativeFamilyUpdateHost {
                process_rng: &mut process_rng,
                sine_quarter: &SINE
            },
        ),
        Err(FamilyUpdateError::Host {
            effect,
            error: NativeFamilyUpdateError::Behavior(ActorBehaviorTickError::GroupBacking {
                actor,
                index: 1,
                groups: 1,
            }),
        })
    );
    assert_eq!(
        arena.derived_actor(actor).unwrap().behavior_140.as_ref(),
        Some(&expected)
    );
    assert_eq!(process_rng, 0x1234_5678);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
}
