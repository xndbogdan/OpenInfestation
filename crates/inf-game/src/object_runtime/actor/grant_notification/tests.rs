use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, CameraSettings, DerivedActorConstructionRequest, ObjectClass,
        ObjectClassKind, ObjectState, Selector12GrantContext, Selector12State,
    },
    process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, GrantTable, HandlerFraming, HandlerId, HandlerRegion,
    GRANT_TABLE_ADDRESS, TERRAIN_ENTRY_FIXED_BYTES,
};

// Unknown allocation XYZ bypasses this constructor's terrain/sine consumers.
const UNUSED_SINE: [u16; 4096] = [0; 4096];

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
                sine_quarter: &UNUSED_SINE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 32,
        // Deliberately independent of the live process owner in each test.
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

fn mask(ids: &[u16]) -> [u32; 6] {
    let mut result = [0; 6];
    for &id in ids {
        result[usize::from(id >> 5)] |= 1 << (id & 31);
    }
    result
}

fn selector12(arena: &mut ObjectArena, ids: &[u16], byte: u8) -> ObjectId {
    let first_player_grants = arena
        .first_player()
        .map(|player| arena.world_player(player).unwrap().grants_9c);
    // All constructor calls here have no player or an actual empty first
    // player's mask. That supplied arm requires no executable grant records.
    let table = GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let mut rng = 0;
    let actor = arena
        .allocate_selector12_actor(
            request(),
            &mut rng,
            Selector12GrantContext {
                table: &table,
                first_player_grants: first_player_grants.as_ref(),
            },
        )
        .unwrap();
    let state = selector12_mut(arena, actor);
    state.grants_24c.mask_04 = mask(ids);
    state.byte_29d = byte;
    actor
}

fn selector12_mut(arena: &mut ObjectArena, actor: ObjectId) -> &mut Selector12State {
    let Family428650Class::Family429e20(probe) =
        &mut arena.family428650_actor_mut(actor).unwrap().class
    else {
        panic!("actual probe family")
    };
    let Family429e20Class::Selector12(state) = &mut probe.class else {
        panic!("actual selector12 constructor")
    };
    state
}

fn vehicle(arena: &mut ObjectArena) -> ObjectId {
    // Actual selector2 construction requires a supplied parsed terrain.
    // This flat data is constructor input, not an event22 source or fallback.
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
    let mut rng = 0;
    arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0; 3]),
                    terrain: ActorTerrainInputs {
                        terrain: Some(ActorTerrainSurface {
                            terrain: &terrain,
                            plane_height_fixed: 0,
                        }),
                        world_flags: Some(0),
                        ..request().actor.terrain
                    },
                    ..request().actor
                },
                ..request()
            },
            &mut rng,
        )
        .unwrap()
}

fn canonical_actors() -> (ObjectArena, Vec<(&'static str, ObjectId)>) {
    let mut arena = arena();
    let mut rng = 0;
    let mut actors = vec![
        ("shared", arena.allocate_actor(request().actor).unwrap()),
        (
            "derived",
            arena.allocate_derived_actor(request(), &mut rng).unwrap(),
        ),
        (
            "family",
            arena
                .allocate_family428650_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "probe",
            arena
                .allocate_family429e20_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "family4236",
            arena
                .allocate_family4236d0_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector10",
            arena
                .allocate_selector10_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector34",
            arena
                .allocate_selector34_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector35",
            arena
                .allocate_selector35_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector36",
            arena
                .allocate_selector36_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector11",
            arena
                .allocate_selector11_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector60",
            arena
                .allocate_selector60_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector77",
            arena
                .allocate_selector77_actor(request(), &mut rng)
                .unwrap(),
        ),
        (
            "selector16",
            arena
                .allocate_selector16(
                    request().actor,
                    CameraSettings {
                        first_person: 0,
                        camera_style: 0,
                    },
                )
                .unwrap(),
        ),
        (
            "selector30",
            arena.allocate_selector30_actor(request().actor).unwrap(),
        ),
        (
            "selector88",
            arena.allocate_selector88(request().actor).unwrap(),
        ),
        (
            "selector90",
            arena.allocate_selector90(request().actor).unwrap(),
        ),
    ];
    actors.push(("selector12", selector12(&mut arena, &[1, 174], 0xfe)));
    actors.push(("selector2", vehicle(&mut arena)));
    assert_eq!(actors.len(), 18);
    (arena, actors)
}

fn selector_in_state(state: &mut ObjectState) -> &mut Selector12State {
    let ObjectClass::Actor(actor) = &mut state.class else {
        panic!("actor")
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("derived")
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("family")
    };
    let Family428650Class::Family429e20(probe) = &mut family.class else {
        panic!("probe")
    };
    let Family429e20Class::Selector12(state) = &mut probe.class else {
        panic!("selector12")
    };
    state
}

/// Full typed actor states plus every other arena/player/list/registry owner.
/// The expected writes apply only to cloned post-call assertion data.
fn snapshot(arena: &ObjectArena, writes: &[(ObjectId, u8)]) -> (Vec<ObjectState>, String) {
    let mut states: Vec<_> = arena
        .actor_list()
        .iter()
        .map(|&id| arena.object(id).unwrap().clone())
        .collect();
    for &(actor, byte) in writes {
        let index = arena
            .actor_list()
            .iter()
            .position(|&id| id == actor)
            .unwrap();
        selector_in_state(&mut states[index]).byte_29d = byte;
    }
    let allocations: Vec<_> = arena
        .slots
        .iter()
        .map(|slot| {
            (
                slot.generation,
                slot.value.as_ref().map(|owned| owned.memberships),
            )
        })
        .collect();
    let custody = format!(
        "{:?}",
        (
            (
                arena.identity,
                &arena.free,
                arena.live_count,
                &arena.lists,
                &arena.list_owners,
                &arena.routes,
                &arena.actors,
                &arena.map_annotations,
                &arena.actor_handles
            ),
            (
                &arena.actor_payloads,
                &arena.constructor_allocations,
                &arena.selector60_rows,
                &arena.players,
                &arena.world_load,
                allocations
            )
        )
    );
    (states, custody)
}

fn notify(
    arena: &mut ObjectArena,
    delivery: ActorGrantDifferenceDelivery,
    context: PlayerActorLookupContext<'_>,
    writes: &[(ObjectId, u8)],
) {
    let expected = snapshot(arena, writes);
    assert_eq!(
        arena
            .notify_actor_grant_difference(
                ActorGrantDifferenceNotificationRequest { delivery },
                context
            )
            .unwrap(),
        0
    );
    assert_eq!(snapshot(arena, &[]), expected);
}

#[test]
fn all_18_actual_classes_dispatch_independently_of_b4_and_ignore_xyz_flags_lifecycle() {
    let (mut arena, actors) = canonical_actors();
    arena.add_world_player(mask(&[]));
    let process = ParticleFactoryProcessState {
        network_flag: 0x100,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0xdead_beef)),
    };
    let before_process = process;
    for (index, &(name, actor)) in actors.iter().enumerate() {
        arena.actor_mut(actor).unwrap().word_b4 = [2, 12, u32::MAX][index % 3];
        arena.actor_mut(actor).unwrap().word_c4 = 0;
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.position_words = None;
        base.flags |= 0x8000_0000;
        let writes = if name == "selector12" {
            vec![(actor, 1)]
        } else {
            vec![]
        };
        notify(
            &mut arena,
            ActorGrantDifferenceDelivery::Actor(actor),
            PlayerActorLookupContext {
                process: Some(&process),
            },
            &writes,
        );
        if name != "selector2" {
            notify(
                &mut arena,
                ActorGrantDifferenceDelivery::Actor(actor),
                PlayerActorLookupContext { process: None },
                &writes,
            );
        }
    }
    assert_eq!(process, before_process);
}

#[test]
fn selector2_executes_lazy_live_null_lookup_even_with_empty_players_and_null_source() {
    let mut arena = arena();
    let actor = vehicle(&mut arena);
    let unknown = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    for context in [
        PlayerActorLookupContext { process: None },
        PlayerActorLookupContext {
            process: Some(&unknown),
        },
    ] {
        let before = snapshot(&arena, &[]);
        assert!(matches!(
            arena.notify_actor_grant_difference(
                ActorGrantDifferenceNotificationRequest {
                    delivery: ActorGrantDifferenceDelivery::Actor(actor)
                },
                context
            ),
            Err(ActorGrantDifferenceNotificationError::PlayerLookup(
                PlayerActorLookupError::MissingProcessNullHandle
            ))
        ));
        assert_eq!(snapshot(&arena, &[]), before);
    }
    let handle = arena.actor(actor).unwrap().handle;
    let process = ParticleFactoryProcessState {
        network_flag: u32::MAX,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(handle),
    };
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::Actor(actor),
        PlayerActorLookupContext {
            process: Some(&process),
        },
        &[],
    );
}

#[test]
fn selector2_found_no_match_and_stale_authoritative_d8_never_resolve_registry() {
    let mut arena = arena();
    let actor = vehicle(&mut arena);
    let first = arena.add_world_player(mask(&[]));
    let source = ActorHandle::from_raw(0xdead_1234);
    let null = ActorHandle::from_raw(0xfedc_ba98);
    let stale = ActorHandle::from_raw(0xabcd_1234);
    arena.actor_mut(actor).unwrap().handle = source;
    let process = ParticleFactoryProcessState {
        network_flag: 0x100,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(null),
    };
    for (d4, d8, expected) in [
        (stale, source, Some(first)),
        (source, null, Some(first)),
        (source, stale, None),
        (null, null, None),
    ] {
        arena.players.list[0].handle_d4 = d4;
        arena.players.list[0].handle_d8 = d8;
        let context = PlayerActorLookupContext {
            process: Some(&process),
        };
        assert_eq!(
            arena
                .find_player_for_actor_handle(PlayerActorLookupRequest { handle: source }, context)
                .unwrap(),
            expected
        );
        notify(
            &mut arena,
            ActorGrantDifferenceDelivery::Actor(actor),
            context,
            &[],
        );
    }
}

#[test]
fn empty_list_and_nonvehicle_receivers_bypass_missing_process_and_registry() {
    let mut empty = ObjectArena::new();
    notify(
        &mut empty,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[],
    );
    let mut arena = arena();
    let shared = arena.allocate_actor(request().actor).unwrap();
    let selector = selector12(&mut arena, &[1], 0xa5);
    arena.actor_handles = None;
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[],
    );
    assert_eq!(selector12_mut(&mut arena, selector).byte_29d, 0xa5);
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::Actor(shared),
        PlayerActorLookupContext { process: None },
        &[],
    );
}

#[test]
fn mixed_fifo_failure_keeps_earlier_grant_write_and_skips_later_receiver() {
    let mut arena = arena();
    let first = selector12(&mut arena, &[1], 0xfe);
    let shared = arena.allocate_actor(request().actor).unwrap();
    let vehicle = vehicle(&mut arena);
    let later = selector12(&mut arena, &[174], 0xa5);
    arena.add_world_player(mask(&[]));
    assert_eq!(arena.actor_list(), &[first, shared, vehicle, later]);
    let expected = snapshot(&arena, &[(first, 1)]);
    assert!(matches!(
        arena.notify_actor_grant_difference(
            ActorGrantDifferenceNotificationRequest {
                delivery: ActorGrantDifferenceDelivery::ActorList
            },
            PlayerActorLookupContext { process: None }
        ),
        Err(ActorGrantDifferenceNotificationError::PlayerLookup(
            PlayerActorLookupError::MissingProcessNullHandle
        ))
    ));
    assert_eq!(snapshot(&arena, &[]), expected);
    let process = ParticleFactoryProcessState::native_startup();
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext {
            process: Some(&process),
        },
        &[(first, 1), (later, 1)],
    );
}

#[test]
fn stopped_construction_notification_receivers_skip_lookup_and_writes_while_fifo_continues() {
    let mut arena = arena();
    let stopped_vehicle = vehicle(&mut arena);
    let stopped_grant = selector12(&mut arena, &[1], 0xa5);
    let healthy = selector12(&mut arena, &[174], 0xfe);
    arena.add_world_player(mask(&[]));
    for actor in [stopped_vehicle, stopped_grant] {
        arena.stop_object_construction(actor).unwrap();
    }
    let nodes = arena.actor_list().to_vec();
    // Missing process would fail selector2's lookup. Only the healthy
    // selector12 receives event22 and changes its grant-difference byte.
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[(healthy, 1)],
    );
    assert_eq!(arena.actor_list(), nodes);
}

#[test]
fn stopped_disposal_notifications_skip_lookup_and_writes_while_healthy_fifo_continues() {
    let mut arena = arena();
    let stopped_vehicle = vehicle(&mut arena);
    let stopped_grant = selector12(&mut arena, &[1], 0xa5);
    let healthy = selector12(&mut arena, &[174], 0xfe);
    arena.add_world_player(mask(&[]));
    for actor in [stopped_vehicle, stopped_grant] {
        arena.dispose(actor).unwrap();
        // Explicit supplied failed-destructor custody, separate from the
        // real disposal kernel receipts which establish marker publication.
        arena.owned_mut(actor).unwrap().disposal_stop =
            Some(crate::object_runtime::ObjectDisposalStop {
                object: actor,
                phase: crate::object_runtime::ObjectDisposalPhase::DeletingDestructor,
                reason: "supplied destructor stop".into(),
            });
    }
    let nodes = arena.actor_list().to_vec();
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[(healthy, 1)],
    );
    assert_eq!(arena.actor_list(), nodes);
    assert_eq!(
        arena.selector12_actor(stopped_grant).unwrap().byte_29d,
        0xa5
    );
}

#[test]
fn parked_and_disposed_actors_remain_in_fifo_until_actual_destroy() {
    let mut arena = arena();
    let parked = selector12(&mut arena, &[1], 0xfe);
    let disposed = selector12(&mut arena, &[174], 0xa5);
    let removed = arena.allocate_actor(request().actor).unwrap();
    arena.add_world_player(mask(&[]));
    arena.set_flag0(parked, false).unwrap();
    arena.set_flag1(parked, false).unwrap();
    arena.dispose(disposed).unwrap();
    assert_eq!(arena.actor_list(), &[parked, disposed, removed]);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: removed,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.actor_list(), &[parked, disposed]);
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[(parked, 1), (disposed, 1)],
    );
}

#[test]
fn each_broadcast_reloads_live_first_player_and_masks_after_real_appends() {
    let mut arena = arena();
    let first = selector12(&mut arena, &[1], 0xfe);
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[],
    );
    let player = arena.add_world_player(mask(&[]));
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[(first, 1)],
    );
    let later = selector12(&mut arena, &[174], 0xa5);
    arena.add_world_player(mask(&[1, 174]));
    assert_eq!(arena.first_player(), Some(player));
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[(first, 1), (later, 1)],
    );
    arena.players.list[0].grants_9c = mask(&[1]);
    selector12_mut(&mut arena, later).grants_24c.mask_04 = mask(&[1]);
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[(first, 0), (later, 0)],
    );
}

#[test]
fn direct_delivery_authenticates_actual_actor_before_lazy_process_lookup() {
    let mut arena = arena();
    let visual = arena.allocate(ObjectClassKind::VisualBase);
    let shared = arena.allocate_actor(request().actor).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: shared,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    for (id, missing) in [(visual, false), (shared, true)] {
        let result = arena.notify_actor_grant_difference(
            ActorGrantDifferenceNotificationRequest {
                delivery: ActorGrantDifferenceDelivery::Actor(id),
            },
            PlayerActorLookupContext { process: None },
        );
        assert!(
            matches!(result, Err(ActorGrantDifferenceNotificationError::Object(error)) if error == if missing { ObjectError::UnknownObject(id) } else { ObjectError::NotActor(id) })
        );
    }
}

fn cleanup(arena: &mut ObjectArena, source: ObjectId, context: PlayerActorLookupContext<'_>) {
    let expected = snapshot(arena, &[]);
    assert_eq!(
        arena
            .notify_actor_vehicle_cleanup(
                ActorVehicleCleanupNotificationRequest { source },
                context,
            )
            .unwrap(),
        0
    );
    assert_eq!(snapshot(arena, &[]), expected);
}

#[test]
fn vehicle_cleanup_all_19_actual_classes_preserve_full_state_without_grant_suffix() {
    let (mut arena, mut actors) = canonical_actors();
    actors.push((
        "selector93",
        arena.allocate_selector93(request().actor).unwrap(),
    ));
    let first = arena.add_world_player(mask(&[]));
    let second = arena.add_world_player(mask(&[1, 174]));
    let process = ParticleFactoryProcessState {
        network_flag: u32::MAX,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0xfefe_1234)),
    };
    let before_process = process;
    for (index, &(name, actor)) in actors.iter().enumerate() {
        arena.actor_mut(actor).unwrap().word_b4 = [2, 12, u32::MAX][index % 3];
        arena.actor_mut(actor).unwrap().word_c4 = 0;
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.position_words = None;
        base.flags |= 0x8000_0000;
        if name != "selector2" {
            // Actual class suffix isolation, without manufacturing a callback
            // observer or replacing the public fanout's current-node policy.
            let before = snapshot(&arena, &[]);
            assert_eq!(
                VehicleCleanupMode { source: actor }
                    .receive(
                        &mut arena,
                        actor,
                        PlayerActorLookupContext { process: None }
                    )
                    .unwrap(),
                0
            );
            assert_eq!(snapshot(&arena, &[]), before);
        }
    }
    let source = actors[0].1;
    cleanup(
        &mut arena,
        source,
        PlayerActorLookupContext {
            process: Some(&process),
        },
    );
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(
        arena.world_player(second).unwrap().grants_9c,
        mask(&[1, 174])
    );
    assert_eq!(process, before_process);
}

#[test]
fn vehicle_cleanup_selector12_bypasses_first_player_masks_and_29d_but_event22_does_not() {
    let mut arena = arena();
    let receiver = selector12(&mut arena, &[1, 174], 0xa5);
    arena.add_world_player(mask(&[]));
    arena.add_world_player(mask(&[1, 174]));
    arena.actor_handles = None;
    cleanup(
        &mut arena,
        receiver,
        PlayerActorLookupContext { process: None },
    );
    assert_eq!(selector12_mut(&mut arena, receiver).byte_29d, 0xa5);
    // Same real receiver and backing, distinct authentic event policy.
    notify(
        &mut arena,
        ActorGrantDifferenceDelivery::ActorList,
        PlayerActorLookupContext { process: None },
        &[(receiver, 1)],
    );
    assert_eq!(selector12_mut(&mut arena, receiver).byte_29d, 1);
    cleanup(
        &mut arena,
        receiver,
        PlayerActorLookupContext { process: None },
    );
}

#[test]
fn vehicle_cleanup_requires_live_null_only_at_reached_selector2_even_with_no_players() {
    let mut arena = arena();
    let source = arena.allocate_actor(request().actor).unwrap();
    let receiver = vehicle(&mut arena);
    let missing = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    for context in [
        PlayerActorLookupContext { process: None },
        PlayerActorLookupContext {
            process: Some(&missing),
        },
    ] {
        let before = snapshot(&arena, &[]);
        assert!(matches!(
            arena.notify_actor_vehicle_cleanup(
                ActorVehicleCleanupNotificationRequest { source },
                context,
            ),
            Err(ActorVehicleCleanupNotificationError::PlayerLookup(
                PlayerActorLookupError::MissingProcessNullHandle
            ))
        ));
        assert_eq!(snapshot(&arena, &[]), before);
    }
    let process = ParticleFactoryProcessState {
        network_flag: 0x100,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(arena.actor(receiver).unwrap().handle),
    };
    cleanup(
        &mut arena,
        source,
        PlayerActorLookupContext {
            process: Some(&process),
        },
    );
}

#[test]
fn vehicle_cleanup_player_lookup_uses_current_authoritative_d8_without_registry_resolution() {
    let mut arena = arena();
    let receiver = vehicle(&mut arena);
    let player = arena.add_world_player(mask(&[]));
    let handle = ActorHandle::from_raw(0xaaaa_1234);
    let null = ActorHandle::from_raw(0xbbbb_5678);
    let stale = ActorHandle::from_raw(0xcccc_9876);
    arena.actor_mut(receiver).unwrap().handle = handle;
    // Receiver actual84 remains authoritative even when registry is absent.
    arena.actor_handles = None;
    let process = ParticleFactoryProcessState {
        network_flag: 0x100,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(null),
    };
    let before_process = process;
    for (d4, d8, found) in [
        (stale, handle, Some(player)),
        (handle, null, Some(player)),
        (handle, stale, None),
        (null, null, None),
    ] {
        arena.players.list[0].handle_d4 = d4;
        arena.players.list[0].handle_d8 = d8;
        let context = PlayerActorLookupContext {
            process: Some(&process),
        };
        assert_eq!(
            arena
                .find_player_for_actor_handle(PlayerActorLookupRequest { handle }, context)
                .unwrap(),
            found,
        );
        cleanup(&mut arena, receiver, context);
    }
    assert_eq!(process, before_process);
}

#[test]
fn vehicle_cleanup_captured_source_is_unread_and_empty_or_nonvehicle_lists_need_no_process() {
    let mut other = ObjectArena::new();
    let foreign_source = other.allocate(ObjectClassKind::Base);
    let mut empty = ObjectArena::new();
    cleanup(
        &mut empty,
        foreign_source,
        PlayerActorLookupContext { process: None },
    );
    let mut arena = arena();
    let source = arena.allocate_actor(request().actor).unwrap();
    let receiver = selector12(&mut arena, &[1], 0xa5);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: source,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    // Native zero-radius event10 forms source+32 but never reads it on these
    // admitted bodies. A stale or foreign captured token is not a receiver.
    for source in [source, foreign_source] {
        cleanup(
            &mut arena,
            source,
            PlayerActorLookupContext { process: None },
        );
    }
    assert_eq!(selector12_mut(&mut arena, receiver).byte_29d, 0xa5);
}

#[test]
fn vehicle_cleanup_uses_current_fifo_including_parked_disposed_and_new_receivers() {
    let mut arena = arena();
    let source = selector12(&mut arena, &[1], 0xfe);
    let removed = arena.allocate_actor(request().actor).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: removed,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    cleanup(
        &mut arena,
        source,
        PlayerActorLookupContext { process: None },
    );
    let receiver = vehicle(&mut arena);
    let later = selector12(&mut arena, &[174], 0xa5);
    arena.add_world_player(mask(&[]));
    arena.set_flag0(receiver, false).unwrap();
    arena.set_flag1(receiver, false).unwrap();
    arena.dispose(later).unwrap();
    assert_eq!(arena.actor_list(), &[source, receiver, later]);
    let before = snapshot(&arena, &[]);
    assert!(matches!(
        arena.notify_actor_vehicle_cleanup(
            ActorVehicleCleanupNotificationRequest { source },
            PlayerActorLookupContext { process: None },
        ),
        Err(ActorVehicleCleanupNotificationError::PlayerLookup(
            PlayerActorLookupError::MissingProcessNullHandle
        ))
    ));
    // Event10 has no earlier selector12 write to roll back or replay. The
    // parked vehicle still requires its actual lookup; bytes stay FE/A5.
    assert_eq!(snapshot(&arena, &[]), before);
    let process = ParticleFactoryProcessState::native_startup();
    cleanup(
        &mut arena,
        source,
        PlayerActorLookupContext {
            process: Some(&process),
        },
    );
}
