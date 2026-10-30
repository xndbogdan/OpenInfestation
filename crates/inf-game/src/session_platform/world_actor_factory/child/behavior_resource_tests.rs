//! 0048D820's registry resources over the loaded Level 1 world: the objects
//! 00460310 builds match the recorded flags and packets, and one cache
//! initialization builds, copies and disposes its temporary.

use super::*;
use crate::object_runtime::{
    BehaviorComponentQuery, BehaviorParticleAdapter, BehaviorParticleServices,
    BehaviorPredicateContext, BehaviorPredicateRequest, BehaviorRowSelection, ObjectArena,
    ObjectClass, ObjectList,
};
use crate::session_platform::world_actor_behavior::particles::WorldSelectionServices;
use crate::session_platform::world_actor_factory::{
    BehaviorResourceFactory, BehaviorResourceOwners,
};

fn loaded() -> PortSession<Recording> {
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session
}

fn with_owners<R>(
    session: &mut PortSession<Recording>,
    action: impl FnOnce(&mut ObjectArena, &mut BehaviorResourceOwners<'_>) -> R,
) -> R {
    let assets = session.catalog.assets();
    let world = session.world.as_mut().unwrap();
    let mut owners = BehaviorResourceOwners {
        residency: &session.residency,
        process: session.factory_process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: session.process.animation().value(),
        sine: &assets.sine_table().values,
        listeners: &mut world.listeners,
    };
    action(&mut world.arena, &mut owners)
}

/// The traces' objects at 00460310's return (Infest01 D7148E:511 for 116,
/// Infest03 2937FF:16EC for 130 and 2BE739:1E9B for 113): flags +2C and
/// resource +30. Their +32 position came from stack residue.
#[test]
fn registry_resources_match_the_recorded_objects() {
    let mut session = loaded();
    for (resource, flags) in [
        (116_i16, 0x4810_000b_u32),
        (113, 0x4810_0807),
        (130, 0x0810_0003),
    ] {
        let object = with_owners(&mut session, |arena, owners| {
            owners
                .create_behavior_resource(arena, resource)
                .unwrap()
                .unwrap()
        });
        let arena = &session.world.as_ref().unwrap().arena;
        let base = &arena.object(object).unwrap().base;
        assert_eq!(base.flags, flags, "resource {resource} flags");
        assert_eq!(base.resource_word, resource as u16);
        assert_eq!(base.placement_word, 0xffff);
        assert_eq!(base.position_words, None, "unwritten scratch position");
        match resource {
            116 => {
                let state = arena.behavior_resource(object).unwrap();
                assert_eq!(state.selector, 32);
                // b148=6 w146=c8 in the trace.
                assert_eq!(state.packet_channel_148(), Some(Some(6)));
                assert_eq!(state.packet_strength_146(), Some([Some(0xc8), Some(0)]));
            }
            113 => {
                let state = arena.behavior_resource(object).unwrap();
                assert_eq!(state.selector, 28);
                // d13e=40a8c in the trace.
                assert_eq!(
                    state.packet_13e(),
                    Some(0x0004_0a8c_u32.to_le_bytes().map(Some))
                );
            }
            _ => assert!(matches!(
                arena.object(object).unwrap().class,
                ObjectClass::Selector76(_)
            )),
        }
    }
    // Negative and above-count WORDs are the factory's NULL.
    with_owners(&mut session, |arena, owners| {
        assert_eq!(owners.create_behavior_resource(arena, -1).unwrap(), None);
        assert_eq!(
            owners.create_behavior_resource(arena, i16::MAX).unwrap(),
            None
        );
    });
}

/// Every resource the behavior tables name (the vehicle's rows and the
/// enemies' mode tables) builds through this path, answers its packet
/// slot with fully known bytes and is destroyed after its disposal. The
/// authored-placement path refuses each of them but the mine (38), whose
/// placements it builds and so asks for the actor inputs.
#[test]
fn every_registry_resource_builds_and_tears_down() {
    let mut session = loaded();
    for (resource, selector) in [
        (110_i16, 25_u8),
        (113, 28),
        (115, 31),
        (116, 32),
        (117, 38),
        (118, 38),
        (119, 26),
        (120, 26),
        (121, 27),
        (123, 26),
        (124, 26),
        (125, 26),
        (126, 26),
        (127, 25),
        (129, 68),
        (130, 76),
    ] {
        let world = session.world.as_mut().unwrap();
        let refused = crate::object_runtime::construct_world_object(
            &mut world.arena,
            &session.residency,
            crate::object_runtime::ObjectConstructionRequest {
                resource_argument: resource as u32,
                context: crate::object_runtime::BaseContext {
                    position_words: None,
                    placement_index: u32::MAX,
                },
                suffix: None,
                command_allocation: None,
                network_active: false,
                actor: None,
            },
        );
        if selector == 38 {
            assert!(
                matches!(
                    refused,
                    Err(
                        crate::object_runtime::ObjectConstructionError::MissingActorInputs {
                            selector: 38
                        }
                    )
                ),
                "{resource}: {refused:?}"
            );
        } else {
            assert!(
                matches!(
                    refused,
                    Err(crate::object_runtime::ObjectConstructionError::UnsupportedSelector { selector: s, .. })
                        if s == u16::from(selector)
                ),
                "{resource}"
            );
        }
        let object = with_owners(&mut session, |arena, owners| {
            owners
                .create_behavior_resource(arena, resource)
                .unwrap()
                .unwrap()
        });
        let arena = &mut session.world.as_mut().unwrap().arena;
        if selector == 76 {
            assert!(matches!(
                arena.object(object).unwrap().class,
                ObjectClass::Selector76(_)
            ));
        } else {
            let state = arena.behavior_resource(object).unwrap();
            assert_eq!(state.selector, selector, "{resource}");
            if state.component_6c() {
                assert!(state.packet_channel_148().unwrap().is_some());
                assert!(state
                    .packet_strength_146()
                    .unwrap()
                    .iter()
                    .all(Option::is_some));
            } else {
                assert!(state.component_68());
                assert!(state.packet_13e().unwrap().iter().all(Option::is_some));
            }
        }
        arena.dispose(object).unwrap();
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert!(arena.object(object).is_err(), "{resource} destroyed");
    }
}

/// One evaluation of a row through the session's services and factory.
fn evaluate(
    session: &mut PortSession<Recording>,
    row: BehaviorRowSelection,
    candidate: crate::object_runtime::ObjectId,
) {
    let state = SessionState::default();
    let assets = session.catalog.assets();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let world = session.world.as_mut().unwrap();
    let mut owners = BehaviorResourceOwners {
        residency: &session.residency,
        process: session.factory_process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: session.process.animation().value(),
        sine: &assets.sine_table().values,
        listeners: &mut world.listeners,
    };
    let mut process = session.factory_process;
    let mut context = BehaviorPredicateContext {
        tables: &mut session.actor_behavior_tables,
        process: &mut process,
        rng: &mut world.rng,
    };
    let mut services = WorldSelectionServices {
        tasks: &mut session.tasks,
        animation_clock: &session.process,
        session: &state,
        resources: Some(&mut owners),
    };
    let mut host = BehaviorParticleAdapter {
        particles: &mut session.particles,
        executable,
        services: &mut services,
    };
    world
        .arena
        .test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: row,
                candidate: Some(candidate),
            },
            &mut context,
            &mut host,
        )
        .unwrap();
}

/// A row's first evaluation builds its temporary, copies the packet and
/// disposes it; a later evaluation reads the ready row and builds nothing.
/// The shared row 005310A0 names resource 110 (selector 25, +68 and the
/// DWORD +13E); the vehicle's row 0 names 116 (selector 32, +6C, +148 then
/// +146), the row the probe drive stopped on.
#[test]
fn cache_initialization_builds_copies_and_disposes_its_temporary() {
    let mut session = loaded();
    let world = session.world.as_ref().unwrap();
    let candidate = world.arena.actor_list()[0];
    let vehicle = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector2_actor(actor).is_ok());
    let mut rows = vec![(
        BehaviorRowSelection::SharedAddress {
            address: 0x0053_10a0,
        },
        110_u16,
    )];
    if let Some(vehicle) = vehicle {
        rows.push((
            BehaviorRowSelection::VehicleCopy {
                owner: vehicle,
                byte_offset: 0,
            },
            116,
        ));
    }
    for (row, resource) in rows {
        let mut temporary = None;
        for evaluation in 0..2 {
            let before = session
                .world
                .as_ref()
                .unwrap()
                .arena
                .list(ObjectList::Disposal)
                .len();
            evaluate(&mut session, row, candidate);
            let arena = &session.world.as_ref().unwrap().arena;
            let disposed = arena.list(ObjectList::Disposal);
            if evaluation == 0 {
                // One temporary, off both flag lists, awaiting the drain.
                assert_eq!(disposed.len(), before + 1, "{resource}");
                let object = disposed.last().unwrap().object;
                let base = &arena.object(object).unwrap().base;
                assert_eq!(base.resource_word, resource);
                assert_eq!(base.flags & 0x3, 0);
                assert_eq!(base.position_words, None);
                temporary = Some(object);
            } else {
                assert_eq!(disposed.len(), before, "a ready row builds nothing");
            }
        }
        let arena = &session.world.as_ref().unwrap().arena;
        let state = arena.behavior_resource(temporary.unwrap()).unwrap();
        let (ready, packet) = match row {
            BehaviorRowSelection::SharedAddress { address } => {
                let bytes = session
                    .actor_behavior_tables
                    .read_process_bytes(address + 0x22, 6)
                    .unwrap();
                (bytes[0], [bytes[2], bytes[3], bytes[4], bytes[5]])
            }
            BehaviorRowSelection::VehicleCopy { owner, .. } => {
                let vehicle = arena.selector2_actor(owner).unwrap();
                let table = vehicle.table_26c.as_deref().unwrap();
                let byte = |offset: usize| Some((table[offset / 4] >> ((offset % 4) * 8)) as u8);
                (byte(0x22), [byte(0x24), byte(0x25), byte(0x26), byte(0x27)])
            }
        };
        assert_eq!(ready, Some(1));
        match state.selector {
            // 00458F50: the whole DWORD +13E.
            25 => assert_eq!(Some(packet), state.packet_13e()),
            // 004863D0: channel +148 into byte 2, strength +146 into 0..1;
            // byte 3 keeps the default packet's.
            32 => {
                assert_eq!(packet[2], Some(6));
                assert_eq!([packet[0], packet[1]], [Some(0xc8), Some(0)]);
            }
            other => panic!("unexpected selector {other}"),
        }
    }
    assert!(vehicle.is_some(), "the loaded world has its vehicle");
}

#[test]
fn services_answer_the_class_components() {
    let mut session = loaded();
    let object = with_owners(&mut session, |arena, owners| {
        owners
            .create_behavior_resource(arena, 116)
            .unwrap()
            .unwrap()
    });
    let state = SessionState::default();
    let world = session.world.as_mut().unwrap();
    let mut process = session.factory_process;
    let mut context = BehaviorPredicateContext {
        tables: &mut session.actor_behavior_tables,
        process: &mut process,
        rng: &mut world.rng,
    };
    let mut services = WorldSelectionServices {
        tasks: &mut session.tasks,
        animation_clock: &session.process,
        session: &state,
        resources: None,
    };
    for (query, expected) in [
        (BehaviorComponentQuery::Actor18(object), Some(object)),
        (BehaviorComponentQuery::Component6c(object), Some(object)),
        (BehaviorComponentQuery::Component68(object), None),
    ] {
        assert_eq!(
            services
                .query_component(&mut world.arena, &mut context, query)
                .unwrap(),
            expected
        );
    }
    // Without a lent factory the creation keeps its boundary.
    assert!(matches!(
        services.create_actor_resource(
            &mut world.arena,
            &mut context,
            crate::object_runtime::BehaviorActorResourceRequest {
                resource: 116,
                null_handle: crate::object_runtime::ActorHandle::from_raw(0),
                scratch_bytes: [None; 8],
            },
        ),
        Err(crate::session_platform::world_actor_behavior::WorldFamilyBehaviorError::RequiredActorResource460310 { .. })
    ));
}

/// The weapon-switch crash (crash logs 2026-10-17 19:02 and 19:37):
/// selecting the Cranial Implanter's row runs 0048D820 over the vehicle's
/// row 7, which builds resource 129 at the cache's unwritten stack bytes,
/// and each of Stendahl Alpha's Force Fields tests every new actor against
/// itself (00477350). The test of that residue leaves each field's bit
/// unknown in the temporary's +6C; 0048D820 disposes it at once, so no
/// field's walk reads it and the world goes on.
#[test]
fn a_force_field_lets_the_implanters_temporary_build() {
    let (mut session, _) = fixture();
    session.mission_override = Some(3);
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let world = session.world.as_ref().unwrap();
    let arena = &world.arena;
    let fields: Vec<_> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector18(actor).is_ok())
        .collect();
    assert!(!fields.is_empty(), "Stendahl Alpha has its Force Fields");
    let vehicle = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector2_actor(actor).is_ok())
        .expect("the loaded world has its vehicle");
    let candidate = arena.actor_list()[0];
    // Row 7 of the vehicle's copied table, 44 bytes a row: kind 28.
    let row = BehaviorRowSelection::VehicleCopy {
        owner: vehicle,
        byte_offset: 7 * 44,
    };
    evaluate(&mut session, row, candidate);
    let arena = &session.world.as_ref().unwrap().arena;
    let temporary = arena.list(ObjectList::Disposal).last().unwrap().object;
    assert_eq!(arena.object(temporary).unwrap().base.resource_word, 129);
    assert_eq!(arena.behavior_resource(temporary).unwrap().selector, 68);
    let bits = fields.iter().fold(0, |bits, &field| {
        bits | arena.selector18(field).unwrap().bit_16c
    });
    assert_eq!(arena.actor(temporary).unwrap().unknown_6c, bits);
    assert_eq!(
        arena.object(temporary).unwrap().base.flags & 4,
        0,
        "off the walks"
    );
    for _ in 0..4 {
        session
            .tick(
                crate::session::TickCallback::Gameplay,
                40_000,
                40_000,
                &mut state,
            )
            .unwrap();
    }
    let world = session.world.as_ref().unwrap();
    assert!(
        world.actor_updates.selector18_stops.is_empty(),
        "{:?}",
        world.actor_updates.selector18_stops
    );
    assert!(
        world.arena.object(temporary).is_err(),
        "the drain destroyed it"
    );
}
