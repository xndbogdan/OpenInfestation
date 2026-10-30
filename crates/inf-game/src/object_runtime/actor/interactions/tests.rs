use super::*;
use crate::frontend_camera::AnimationClock;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, CameraSettings, DerivedActorConstructionRequest, ObjectClassKind,
        Selector12GrantContext, Selector30State, Selector35State,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, GrantTable, HandlerFraming, HandlerId, HandlerRegion,
    GRANT_TABLE_ADDRESS, TERRAIN_ENTRY_FIXED_BYTES,
};

const SINE: [u16; 4096] = [0; 4096];

fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
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
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 32,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

fn canonical_actors() -> (ObjectArena, Vec<ObjectId>) {
    let mut arena = arena();
    let mut rng = 0;
    let mut actors = vec![
        arena.allocate_actor(request().actor).unwrap(),
        arena.allocate_derived_actor(request(), &mut rng).unwrap(),
        arena
            .allocate_family428650_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_family429e20_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_family4236d0_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector10_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector34_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector35_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector36_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector11_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector60_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector77_actor(request(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector16(
                request().actor,
                CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
            )
            .unwrap(),
        arena.allocate_selector30_actor(request().actor).unwrap(),
        arena.allocate_selector88(request().actor).unwrap(),
        arena.allocate_selector90(request().actor).unwrap(),
    ];
    let grants = GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    actors.push(
        arena
            .allocate_selector12_actor(
                request(),
                &mut rng,
                Selector12GrantContext {
                    table: &grants,
                    first_player_grants: None,
                },
            )
            .unwrap(),
    );
    // The vehicle has a genuine constructor requiring a supplied terrain.
    // This flat parsed grid is only construction input, never query policy.
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    let terrain = TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    actors.push(
        arena
            .allocate_selector2_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([0; 3]),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: Some(ActorTerrainSurface {
                                terrain: &terrain,
                                plane_height_fixed: 0,
                            }),
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: Some(0),
                            sine_quarter: &SINE,
                        },
                    },
                    world_mask_limits: [3, 3],
                },
                &mut rng,
            )
            .unwrap(),
    );
    assert_eq!(actors.len(), 18);
    (arena, actors)
}

fn selector30_mut(arena: &mut ObjectArena, actor: ObjectId) -> &mut Selector30State {
    let ActorExtension::Selector30(state) = &mut arena.actor_mut(actor).unwrap().extension else {
        panic!("actual selector30 constructor");
    };
    state
}

fn selector35_mut(arena: &mut ObjectArena, actor: ObjectId) -> &mut Selector35State {
    let Family428650Class::Selector35(state) =
        &mut arena.family428650_actor_mut(actor).unwrap().class
    else {
        panic!("actual selector35 constructor");
    };
    state
}

fn clock_at(ticks: u32) -> AnimationClock {
    let mut clock = AnimationClock::default();
    let mut micros = u64::from(ticks) * 20_000;
    while micros != 0 {
        let step = micros.min(u64::from(u32::MAX)) as u32;
        clock.advance(step);
        micros -= u64::from(step);
    }
    assert_eq!(clock.value(), ticks);
    clock
}

fn notice(candidate: ObjectId, source: Option<ObjectId>) -> ActorInteractionNotificationRequest {
    ActorInteractionNotificationRequest {
        candidate,
        source,
        distance_squared: u32::MAX,
    }
}

#[test]
fn component_query_modes_preserve_canonical_identity_and_payload() {
    let (mut arena, actors) = canonical_actors();
    let list = arena.actor_list().to_vec();
    let cache = arena.actor_handles().unwrap().cache();
    for actor in actors {
        arena.actor_mut(actor).unwrap().word_b4 = u32::MAX;
        arena.actor_mut(actor).unwrap().word_c4 = i32::MIN as u32;
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        let is_selector30 = matches!(
            &arena.actor(actor).unwrap().extension,
            ActorExtension::Selector30(_)
        );
        let before = arena.object(actor).unwrap().clone();
        for (mode, expected) in [
            (ActorComponentQueryMode::Self50, None),
            (
                ActorComponentQueryMode::Interaction54,
                is_selector30.then_some(actor),
            ),
        ] {
            assert_eq!(
                arena
                    .query_actor_component(ActorComponentQueryRequest { actor, mode })
                    .unwrap(),
                expected
            );
            assert_eq!(arena.object(actor).unwrap(), &before);
        }
    }
    assert_eq!(arena.actor_list(), list);
    assert_eq!(arena.actor_handles().unwrap().cache(), cache);
}

#[test]
fn canonical_void_notifications_bypass_ignored_operands_and_clock() {
    let (mut arena, actors) = canonical_actors();
    let retired_source = arena.allocate_actor(request().actor).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: retired_source,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let list = arena.actor_list().to_vec();
    let cache = arena.actor_handles().unwrap().cache();
    for actor in actors {
        arena.actor_mut(actor).unwrap().word_b4 = u32::MAX;
        let before = arena.object(actor).unwrap().clone();
        arena
            .notify_actor_interaction(
                notice(actor, Some(retired_source)),
                ActorInteractionNotificationContext {
                    animation_clock: None,
                },
            )
            .unwrap();
        assert_eq!(arena.object(actor).unwrap(), &before);
    }
    assert_eq!(arena.actor_list(), list);
    assert_eq!(arena.actor_handles().unwrap().cache(), cache);
}

#[test]
fn actor_bearing_queries_reject_nonactors_and_retired_receivers() {
    let mut arena = arena();
    let nonactor = arena.allocate(ObjectClassKind::Base);
    let retired = arena.allocate_actor(request().actor).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: retired,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    for (actor, expected) in [
        (nonactor, ObjectError::NotActor(nonactor)),
        (retired, ObjectError::UnknownObject(retired)),
    ] {
        for mode in [
            ActorComponentQueryMode::Self50,
            ActorComponentQueryMode::Interaction54,
        ] {
            assert_eq!(
                arena.query_actor_component(ActorComponentQueryRequest { actor, mode }),
                Err(expected)
            );
        }
        assert_eq!(
            arena.notify_actor_interaction(
                notice(actor, None),
                ActorInteractionNotificationContext {
                    animation_clock: None,
                },
            ),
            Err(ActorInteractionError::Object(expected))
        );
        assert_eq!(
            arena.query_actor_interaction_relation(ActorInteractionRelationRequest {
                component: actor,
                source: None,
            }),
            Err(ActorInteractionError::Object(expected))
        );
    }
    let shared = arena.allocate_actor(request().actor).unwrap();
    assert_eq!(
        arena.query_actor_interaction_relation(ActorInteractionRelationRequest {
            component: shared,
            source: None,
        }),
        Err(ActorInteractionError::NotSelector30(shared))
    );
}

#[test]
fn numeric_relation_skips_source_on_nonzero_18e_and_null_source() {
    let mut arena = arena();
    let component = arena.allocate_selector30_actor(request().actor).unwrap();
    let nonactor = arena.allocate(ObjectClassKind::Base);
    let retired = arena.allocate_actor(request().actor).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: retired,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    for byte in [1, 0x80, 0xff] {
        selector30_mut(&mut arena, component).byte_18e = byte;
        let before = arena.object(component).unwrap().clone();
        for source in [None, Some(nonactor), Some(retired)] {
            assert_eq!(
                arena
                    .query_actor_interaction_relation(ActorInteractionRelationRequest {
                        component,
                        source,
                    })
                    .unwrap(),
                1
            );
            assert_eq!(arena.object(component).unwrap(), &before);
        }
    }
    selector30_mut(&mut arena, component).byte_18e = 0;
    assert_eq!(
        arena
            .query_actor_interaction_relation(ActorInteractionRelationRequest {
                component,
                source: None,
            })
            .unwrap(),
        0
    );
    for (source, error) in [
        (nonactor, ObjectError::NotActor(nonactor)),
        (retired, ObjectError::UnknownObject(retired)),
    ] {
        assert_eq!(
            arena.query_actor_interaction_relation(ActorInteractionRelationRequest {
                component,
                source: Some(source),
            }),
            Err(ActorInteractionError::Object(error))
        );
    }
}

#[test]
fn numeric_relation_compares_fresh_full_dword_classes_without_mutation() {
    let mut arena = arena();
    let component = arena.allocate_selector30_actor(request().actor).unwrap();
    let source = arena.allocate_actor(request().actor).unwrap();
    let list = arena.actor_list().to_vec();
    let cache = arena.actor_handles().unwrap().cache();
    for (component_class, source_class, expected) in [
        (0, 0, 0),
        (1, 0, 1),
        (0x8000_0000, 0x8000_0000, 0),
        (0, 0x1_0000, 1),
        (u32::MAX, u32::MAX, 0),
        (u32::MAX, 0, 1),
    ] {
        arena.actor_mut(component).unwrap().word_b4 = component_class;
        arena.actor_mut(source).unwrap().word_b4 = source_class;
        let before_component = arena.object(component).unwrap().clone();
        let before_source = arena.object(source).unwrap().clone();
        assert_eq!(
            arena
                .query_actor_interaction_relation(ActorInteractionRelationRequest {
                    component,
                    source: Some(source),
                })
                .unwrap(),
            expected
        );
        assert_eq!(arena.object(component).unwrap(), &before_component);
        assert_eq!(arena.object(source).unwrap(), &before_source);
    }
    assert_eq!(arena.actor_list(), list);
    assert_eq!(arena.actor_handles().unwrap().cache(), cache);
}

/// Pelagia's gate (the port's), while its lock holds, answers a scientist
/// of the player's side whatever the teams and an alien never; woken, it
/// answers as retail's. A gate with a destination or a link is never
/// Pelagia's.
#[test]
fn pelagias_locked_gate_answers_the_players_scientists() {
    let mut arena = arena();
    let gate = arena.allocate_selector30_actor(request().actor).unwrap();
    let scientist = arena.allocate_actor(request().actor).unwrap();
    selector30_mut(&mut arena, gate).word_164 = 1;
    arena.mark_pelagia_gate(gate);
    assert!(arena.pelagia_gate_locked(gate));
    let relation = |arena: &ObjectArena| {
        arena
            .query_actor_interaction_relation(ActorInteractionRelationRequest {
                component: gate,
                source: Some(scientist),
            })
            .unwrap()
    };
    for (gate_team, scientist_team, expected) in [(2, 2, 1), (1, 2, 1), (2, 0x12, 0)] {
        arena.actor_mut(gate).unwrap().word_b4 = gate_team;
        arena.actor_mut(scientist).unwrap().word_b4 = scientist_team;
        assert_eq!(relation(&arena), expected, "{gate_team} {scientist_team}");
    }
    selector30_mut(&mut arena, gate).word_164 = 0;
    arena.actor_mut(gate).unwrap().word_b4 = 2;
    arena.actor_mut(scientist).unwrap().word_b4 = 2;
    assert!(!arena.pelagia_gate_locked(gate));
    assert_eq!(relation(&arena), 0, "woken, retail's answer");
    for (world, link) in [(2, 0), (0, 0x4b)] {
        let other = arena.allocate_selector30_actor(request().actor).unwrap();
        let state = selector30_mut(&mut arena, other);
        (state.word_164, state.word_15c, state.word_180) = (1, world, link);
        arena.mark_pelagia_gate(other);
        assert!(!arena.pelagia_gate_locked(other));
    }
}

#[test]
fn selector35_notification_preserves_lazy_unknown_and_clock_error_prefixes() {
    let mut arena = arena();
    let candidate = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
    let state = selector35_mut(&mut arena, candidate);
    state.bytes_240 = [None, None, Some(0), None];
    state.word_244 = 0x8765_4321;
    let before = arena.object(candidate).unwrap().clone();
    arena
        .notify_actor_interaction(
            notice(candidate, None),
            ActorInteractionNotificationContext {
                animation_clock: None,
            },
        )
        .unwrap();
    assert_eq!(arena.object(candidate).unwrap(), &before);

    selector35_mut(&mut arena, candidate).bytes_240[2] = None;
    let before = arena.object(candidate).unwrap().clone();
    assert_eq!(
        arena.notify_actor_interaction(
            notice(candidate, None),
            ActorInteractionNotificationContext {
                animation_clock: None,
            },
        ),
        Err(ActorInteractionError::Uninitialized {
            actor: candidate,
            offset: 0x242,
        })
    );
    assert_eq!(arena.object(candidate).unwrap(), &before);

    selector35_mut(&mut arena, candidate).bytes_240[2] = Some(0x80);
    let before = arena.object(candidate).unwrap().clone();
    assert_eq!(
        arena.notify_actor_interaction(
            notice(candidate, None),
            ActorInteractionNotificationContext {
                animation_clock: None,
            },
        ),
        Err(ActorInteractionError::MissingAnimationClock)
    );
    assert_eq!(arena.object(candidate).unwrap(), &before);
}

#[test]
fn selector35_notification_uses_real_clock_wrap_and_repeated_ticks() {
    let mut arena = arena();
    let candidate = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
    let retired_source = arena.allocate_actor(request().actor).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: retired_source,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    selector35_mut(&mut arena, candidate).bytes_240 = [None, None, Some(0xff), None];
    let mut clock = clock_at(u32::MAX - 100);
    let before = arena.object(candidate).unwrap().clone();
    let before_clock = clock;
    arena
        .notify_actor_interaction(
            notice(candidate, Some(retired_source)),
            ActorInteractionNotificationContext {
                animation_clock: Some(&clock),
            },
        )
        .unwrap();
    assert_eq!(arena.selector35_actor(candidate).unwrap().word_244, 99);
    assert_eq!(clock, before_clock);
    // Only the deadline changed; every other owned and unknown span survived.
    let mut expected = before.clone();
    let crate::object_runtime::ObjectClass::Actor(expected_actor) = &mut expected.class else {
        unreachable!();
    };
    let ActorExtension::Derived(expected_derived) = &mut expected_actor.extension else {
        unreachable!();
    };
    let DerivedActorClass::Family428650(expected_family) = &mut expected_derived.class else {
        unreachable!();
    };
    let Family428650Class::Selector35(expected_state) = &mut expected_family.class else {
        unreachable!();
    };
    expected_state.word_244 = 99;
    assert_eq!(arena.object(candidate).unwrap(), &expected);

    clock.advance(20_000);
    arena
        .notify_actor_interaction(
            notice(candidate, None),
            ActorInteractionNotificationContext {
                animation_clock: Some(&clock),
            },
        )
        .unwrap();
    assert_eq!(arena.selector35_actor(candidate).unwrap().word_244, 100);
    selector35_mut(&mut arena, candidate).bytes_240[2] = Some(0);
    clock.advance(20_000);
    arena
        .notify_actor_interaction(
            notice(candidate, None),
            ActorInteractionNotificationContext {
                animation_clock: Some(&clock),
            },
        )
        .unwrap();
    assert_eq!(arena.selector35_actor(candidate).unwrap().word_244, 100);
}

#[test]
fn selector35_notification_borrows_the_actual_cell_clock_without_eager_reads() {
    use crate::frontend_screen::FrontendProcessState;
    use std::cell::Cell;

    struct ObservedProcess<'a> {
        process: &'a FrontendProcessState,
        reads: Cell<usize>,
    }
    impl AnimationClockSource for ObservedProcess<'_> {
        fn animation_value(&self) -> u32 {
            self.reads.set(self.reads.get() + 1);
            self.process.animation_value()
        }
    }

    let mut arena = arena();
    let candidate = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
    let process = FrontendProcessState::default();
    let observed = ObservedProcess {
        process: &process,
        reads: Cell::new(0),
    };
    let context = ActorInteractionNotificationContext {
        animation_clock: Some(&observed),
    };
    let _ = format!("{context:?}");
    assert_eq!(observed.reads.get(), 0);

    for byte in [Some(0), None] {
        selector35_mut(&mut arena, candidate).bytes_240[2] = byte;
        let before = arena.object(candidate).unwrap().clone();
        let result = arena.notify_actor_interaction(notice(candidate, None), context);
        assert_eq!(
            result,
            if byte.is_none() {
                Err(ActorInteractionError::Uninitialized {
                    actor: candidate,
                    offset: 0x242,
                })
            } else {
                Ok(())
            }
        );
        assert_eq!(arena.object(candidate).unwrap(), &before);
        assert_eq!(observed.reads.get(), 0);
    }

    selector35_mut(&mut arena, candidate).bytes_240[2] = Some(0x80);
    let mut expected = arena.object(candidate).unwrap().clone();
    let actor_list = arena.actor_list().to_vec();
    let cache = arena.actor_handles().unwrap().cache();
    // This same context still borrows the live Cell owner. Advancing or
    // resetting that owner must be observed at each later reached callback.
    for (index, (delta, deadline)) in [(1_400_000, 270), (40_000, 272), (0, 200)]
        .into_iter()
        .enumerate()
    {
        if delta == 0 {
            process.reset_animation();
        } else {
            process.advance_animation(delta);
        }
        arena
            .notify_actor_interaction(notice(candidate, None), context)
            .unwrap();
        let crate::object_runtime::ObjectClass::Actor(actor) = &mut expected.class else {
            unreachable!();
        };
        let ActorExtension::Derived(derived) = &mut actor.extension else {
            unreachable!();
        };
        let DerivedActorClass::Family428650(family) = &mut derived.class else {
            unreachable!();
        };
        let Family428650Class::Selector35(state) = &mut family.class else {
            unreachable!();
        };
        state.word_244 = deadline;
        assert_eq!(arena.object(candidate).unwrap(), &expected);
        assert_eq!(observed.reads.get(), index + 1);
        assert_eq!(arena.actor_list(), actor_list);
        assert_eq!(arena.actor_handles().unwrap().cache(), cache);
    }
}
