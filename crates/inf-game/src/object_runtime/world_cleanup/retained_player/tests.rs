use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    grant_text::{GrantTextData, GrantTextPairClassification},
    input::InputRuntime,
    object_runtime::{
        actor::vehicle_component_tests::{allocation, configured_arena, vehicle},
        ActorBehaviorState, ActorExtension, ActorHandle, ActorHandleCache,
        ActorVehicleCleanupNotificationError, DerivedActorClass, DerivedActorError,
        LiveGrantContext, ObjectClassKind, ObjectError, ObjectId, PlayerActorLookupError,
        PlayerAttachError, PlayerId, PlayerInputDescriptor, PlayerInputDescriptorKind,
        PlayerInputDescriptorRequest, PlayerInputDescriptorSource, WorldPlayer,
    },
    player_input_source::{PlayerAnalogDescriptors, PlayerInputSource},
    process_particles::factories::ParticleFactoryProcessState,
    profile_name_format::ProfileNameClassification,
    world_mission::{WorldMission, WorldMissionInputs},
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{
    parse_constructor_asset_bank, parse_vehicle_behavior_table, ActorConstructorPayload,
    ConstructorPayloadView, FrontendExecutable, MissionSaveState, PlayerSaveState,
    VEHICLE_BEHAVIOR_TABLE_DWORDS,
};

fn mission() -> WorldMission {
    let mut saved = MissionSaveState::new_game();
    saved.owner_240 = std::array::from_fn(|index| 0x8100_0000 | index as u32);
    let mut mission = WorldMission::new(
        saved,
        WorldMissionInputs {
            current_world_a0: u32::MAX,
            dword_1c: 0x1234_5678,
            byte_13: None,
            record_0c: None,
        },
    );
    mission.inhibit_byte_08 = 7;
    mission
}

fn context(process: Option<&ParticleFactoryProcessState>) -> LiveGrantContext<'_> {
    LiveGrantContext {
        process,
        executable: None,
        resident: None,
        text_data: None,
    }
}

fn run(
    arena: &mut ObjectArena,
    mission: &mut WorldMission,
    grants: &LiveGrantContext<'_>,
) -> Result<(), WorldCleanupError> {
    arena.retire_world_owners(WorldCleanupRequest {
        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        mission,
        grants,
        listeners: &mut crate::object_runtime::CreationListeners::new(),
        furniture: None,
    })
}

fn enable(arena: &mut ObjectArena, player: PlayerId) {
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
}

fn seed_player(arena: &mut ObjectArena, player: PlayerId) {
    arena
        .restore_player_save_prefix(
            player,
            &PlayerSaveState {
                owner_100: [0x2233_4455; 6],
                owner_118: 0x6677_8899,
                owner_11c: 0x8000,
                owner_11e: 0xffff,
                owner_120: [0xabc0_0001, 2, 3, 4],
                owner_144: 0xaaaa_0001,
                owner_148: 0xbbbb_0002,
                owner_14c: 0xcccc_0003,
                owner_150: [0xdddd_0004; 16],
                owner_13c: [0x0080_ff41, 0x9988_7766],
                owner_190: [0xaaaa, 0xbbbb, 0xcccc],
                owner_130: [5, 6, 7],
                owner_a0: [0x8000_0001; 6],
            },
        )
        .unwrap();
}

struct VehicleSource {
    payload: Arc<ActorConstructorPayload>,
    behavior: [u32; VEHICLE_BEHAVIOR_TABLE_DWORDS],
}

fn vehicle_source() -> &'static VehicleSource {
    static SOURCE: OnceLock<VehicleSource> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let behavior = parse_vehicle_behavior_table(executable).unwrap();
        let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        resident.apply(ResidencyRequest::Boot).unwrap();
        for level in [47, 48, 32] {
            resident
                .apply(ResidencyRequest::Activate { level })
                .unwrap();
        }
        for slot in 0..resident.resource_capacity(13).unwrap() {
            let Some(resource) = resident.resource(13, slot as u16) else {
                continue;
            };
            let region = &resource.overlay.layout().regions[resource.registration.region_index];
            let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
            let index = usize::from(resource.registration.local_index);
            if bank.prefixes.entries[index].constructor_selector == 2 {
                let ConstructorPayloadView::SharedActor(payload) = &bank.assets[index].payload
                else {
                    panic!("selector2 shared actor payload");
                };
                return VehicleSource {
                    payload: Arc::new(payload.clone()),
                    behavior,
                };
            }
        }
        panic!("actual current H13 lacks selector2");
    })
}

fn admitted_vehicle(arena: &mut ObjectArena) -> ObjectId {
    let vehicle = vehicle(arena, &mut 0x1234_5678);
    arena
        .initialize_actor_payload(vehicle, &vehicle_source().payload, 1)
        .unwrap();
    assert!(arena
        .admit_selector2_actor(vehicle, &vehicle_source().behavior, None)
        .unwrap());
    vehicle
}

fn behavior(arena: &mut ObjectArena, vehicle: ObjectId) -> &mut ActorBehaviorState {
    let ActorExtension::Derived(derived) = &mut arena.actor_mut(vehicle).unwrap().extension else {
        panic!("actual derived constructor");
    };
    derived.behavior_140.as_mut().unwrap()
}

fn vehicle_fields(
    arena: &mut ObjectArena,
    vehicle: ObjectId,
    mode: u32,
    handles: Vec<ActorHandle>,
) {
    let ActorExtension::Derived(derived) = &mut arena.actor_mut(vehicle).unwrap().extension else {
        panic!("actual derived constructor");
    };
    let DerivedActorClass::Selector2(state) = &mut derived.class else {
        panic!("actual selector2 constructor");
    };
    state.dwords_1e8_1f4[0] = mode;
    state.handles_1a4.handles = handles;
}

fn expect_copy(
    player: &mut WorldPlayer,
    mode: u32,
    bytes: [u8; 2],
    values: [u32; 16],
    items: [u16; 3],
) {
    player.dword_144 = mode;
    player.dword_148 = i32::from(bytes[0] as i8) as u32;
    player.dword_14c = i32::from(bytes[1] as i8) as u32;
    player.values_150 = values.map(|value| value as i32);
    player.items_190 = items;
}

// Borrow actual arena storage exhaustively. Normalize only a cloned actual
// selected player's independently asserted five snapshot groups, replacing
// exactly its unique Debug record in complete player storage. This retains
// next_id, camera and every other private field without widening visibility
// or mutating live storage. Registry cache goldens are compared separately.
fn custody(arena: &ObjectArena, player: PlayerId) -> [String; 3] {
    let ObjectArena {
        identity,
        slots,
        free,
        live_count,
        lists,
        list_owners,
        routes,
        actors,
        map_annotations,
        spawn_markers,
        territories,
        race,
        race_gates,
        race_clock,
        powerball,
        powerball_goals,
        powerball_seen,
        network_sends,
        network_game,
        actor_handles,
        actor_allocation,
        actor_services,
        actor_payloads,
        constructor_allocations,
        selector60_rows,
        players,
        world_load,
        pickup_kind7_5b4,
        force_field_bits,
        capture_nodes,
        alien_brain,
        cleanup_exit_pending,
    } = arena;
    let mut normalized = arena.world_player(player).unwrap().clone();
    let original = format!("{normalized:?}");
    normalized.dword_144 = 0;
    normalized.dword_148 = 0;
    normalized.dword_14c = 0;
    normalized.values_150 = [0; 16];
    normalized.items_190 = [0; 3];
    let player_storage = format!("{players:?}");
    assert_eq!(player_storage.matches(&original).count(), 1);
    let player_storage = player_storage.replace(&original, &format!("{normalized:?}"));
    [
        format!(
            "{:?}",
            (
                identity,
                slots,
                free,
                live_count,
                lists,
                list_owners,
                routes,
                actors,
                map_annotations
            )
        ),
        format!(
            "{:?}",
            (
                actor_allocation,
                actor_services,
                actor_payloads,
                constructor_allocations,
                selector60_rows,
                world_load,
                (
                    pickup_kind7_5b4,
                    force_field_bits,
                    capture_nodes,
                    alien_brain,
                    cleanup_exit_pending,
                    spawn_markers,
                    territories,
                    (race, race_gates, race_clock),
                    (powerball, powerball_goals, powerball_seen),
                    network_sends,
                    network_game
                ),
                actor_handles.as_ref().map(|registry| (
                    registry.owners(),
                    registry.initial_capacity(),
                    registry.null_handle()
                ))
            )
        ),
        player_storage,
    ]
}

#[test]
fn no_player_reaches_real_manager_without_registry_or_source_admission() {
    let mut arena = ObjectArena::new();
    let base = arena.allocate(ObjectClassKind::Base);
    let visual = arena.allocate(ObjectClassKind::VisualBase);
    assert!(arena.actor_handles().is_none());
    let mut mission = mission();
    let mut expected = mission.clone();
    expected.inhibit_byte_08 = 0;
    run(&mut arena, &mut mission, &context(None)).unwrap();
    assert!(arena.first_player().is_none());
    assert!(arena.actor_handles().is_none());
    assert!(arena.is_empty());
    assert!(arena.object(base).is_err());
    assert!(arena.object(visual).is_err());
    assert_eq!(arena.free, [base.slot, visual.slot]);
    assert_eq!(mission, expected);
}

#[test]
fn first_actual_null_keeps_enabled_message_and_both_players_without_process() {
    let mut arena = configured_arena();
    let first = arena.add_world_player([0x1234_5678; 6]);
    seed_player(&mut arena, first);
    let second = arena.add_world_player([0x8765_4321; 6]);
    enable(&mut arena, first);
    let before = format!("{arena:?}");
    let mut mission = mission();
    let mut expected = mission.clone();
    expected.inhibit_byte_08 = 0;
    run(&mut arena, &mut mission, &context(None)).unwrap();
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(arena.world_players()[1].id, second);
    assert_eq!(arena.player_message(first).unwrap().suppression_byte(), 0);
    assert_eq!(mission, expected);
}

#[test]
fn missing_registry_stops_first_lookup_before_message_or_manager_inhibit() {
    let mut arena = ObjectArena::new();
    let first = arena.add_world_player([0x8000_0000; 6]);
    let base = arena.allocate(ObjectClassKind::Base);
    enable(&mut arena, first);
    let before = format!("{arena:?}");
    let mut mission = mission();
    let expected = mission.clone();
    assert!(matches!(
        run(&mut arena, &mut mission, &context(None)),
        Err(WorldCleanupError::Player(PlayerAttachError::Object(
            ObjectError::MissingActorRegistry
        )))
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert!(arena.object(base).is_ok());
    assert_eq!(mission, expected);
}

#[test]
fn live_nonvehicle_d8_bypasses_short_d4_copy_before_late_common_failure() {
    let mut arena = configured_arena();
    let shared = arena.allocate_actor(allocation()).unwrap();
    let vehicle = admitted_vehicle(&mut arena);
    let ActorExtension::Derived(derived) = &mut arena.actor_mut(vehicle).unwrap().extension else {
        unreachable!();
    };
    derived.behavior_140 = None;
    let first = arena.add_world_player([1; 6]);
    seed_player(&mut arena, first);
    let d8 = arena.actor(shared).unwrap().handle;
    let d4 = arena.actor(vehicle).unwrap().handle;
    let player = arena.world_player_mut(first).unwrap();
    player.handle_d8 = d8;
    player.handle_d4 = d4;
    enable(&mut arena, first);
    let before = custody(&arena, first);
    let original = arena.world_player(first).unwrap().clone();
    let mut mission = mission();
    let mut expected = mission.clone();
    expected.inhibit_byte_08 = 1;
    assert!(matches!(run(&mut arena, &mut mission, &context(None)),
        Err(WorldCleanupError::MissingProcess(id)) if id == shared));
    assert_eq!(arena.world_player(first).unwrap(), &original);
    assert_eq!(
        arena.actor_handles().unwrap().cache(),
        ActorHandleCache {
            handle: d8,
            object: Some(shared)
        }
    );
    assert_eq!(mission, expected);
    assert_eq!(custody(&arena, first), before);
}

#[test]
fn stale_d8_falls_back_and_copies_only_first_player_before_real_vehicle_destruction() {
    let mut arena = configured_arena();
    let stale_actor = arena.allocate_actor(allocation()).unwrap();
    let stale = arena.actor(stale_actor).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: stale_actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let vehicle = admitted_vehicle(&mut arena);
    let handle = arena.actor(vehicle).unwrap().handle;
    let null = arena.actor_handles().unwrap().null_handle();
    vehicle_fields(&mut arena, vehicle, 0x8000_0002, vec![null; 3]);
    let source = behavior(&mut arena, vehicle);
    source.byte_10 = 0x80;
    source.byte_11 = 0xff;
    let raw = std::array::from_fn::<_, 16, _>(|index| 0x8000_0000 | index as u32);
    for (entry, value) in source.entries_18.iter_mut().zip(raw) {
        entry[0] = value;
    }
    let first = arena.add_world_player([1; 6]);
    seed_player(&mut arena, first);
    let second = arena.add_world_player([2; 6]);
    seed_player(&mut arena, second);
    let player = arena.world_player_mut(first).unwrap();
    player.handle_d8 = stale;
    player.handle_d4 = handle;
    enable(&mut arena, first);
    let mut expected_players = arena.world_players().to_vec();
    expect_copy(
        &mut expected_players[0],
        0x8000_0002,
        [0x80, 0xff],
        raw,
        [0; 3],
    );
    let mut expected_registry = arena.actor_handles().unwrap().clone();
    assert_eq!(expected_registry.resolve(handle), Some(vehicle));
    assert!(expected_registry.release(handle));
    let expected_free = vec![vehicle.slot];
    let process = ParticleFactoryProcessState::native_startup();
    assert_eq!(process.process_null_handle, Some(null));
    let mut mission = mission();
    let mut expected_mission = mission.clone();
    expected_mission.inhibit_byte_08 = 0;
    run(&mut arena, &mut mission, &context(Some(&process))).unwrap();
    assert_eq!(arena.world_players(), expected_players);
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(arena.player_message(first).unwrap().suppression_byte(), 0);
    assert_eq!(arena.player_message(second).unwrap().suppression_byte(), 1);
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{expected_registry:?}")
    );
    assert_eq!(arena.free, expected_free);
    assert!(arena.is_empty());
    assert_eq!(mission, expected_mission);
}

#[test]
fn complete_copy_survives_reached_event10_error_before_common_or_deletion() {
    let mut arena = configured_arena();
    let vehicle = admitted_vehicle(&mut arena);
    let handle = arena.actor(vehicle).unwrap().handle;
    let null = arena.actor_handles().unwrap().null_handle();
    vehicle_fields(&mut arena, vehicle, 0xfeed_beef, vec![null; 3]);
    let first = arena.add_world_player([1; 6]);
    seed_player(&mut arena, first);
    arena.world_player_mut(first).unwrap().handle_d4 = handle;
    enable(&mut arena, first);
    let original = arena.world_player(first).unwrap().clone();
    let mut expected = original.clone();
    expect_copy(&mut expected, 0xfeed_beef, [0, 8], [u32::MAX; 16], [0; 3]);
    let before = custody(&arena, first);
    let mut mission = mission();
    let mut expected_mission = mission.clone();
    expected_mission.inhibit_byte_08 = 1;
    assert!(matches!(
        run(&mut arena, &mut mission, &context(None)),
        Err(WorldCleanupError::Vehicle(
            ActorVehicleCleanupNotificationError::PlayerLookup(
                PlayerActorLookupError::MissingProcessNullHandle
            )
        ))
    ));
    assert_eq!(arena.world_player(first).unwrap(), &expected);
    assert_eq!(
        arena.actor_handles().unwrap().cache(),
        ActorHandleCache {
            handle,
            object: Some(vehicle)
        }
    );
    assert_eq!(mission, expected_mission);
    assert_eq!(custody(&arena, first), before);
}

#[test]
fn short_behavior_and_item_backing_preserve_reached_copy_before_manager() {
    #[derive(Clone, Copy)]
    enum MissingSource {
        Behavior,
        FourthRecord,
        SecondItem,
    }
    for phase in [
        MissingSource::Behavior,
        MissingSource::FourthRecord,
        MissingSource::SecondItem,
    ] {
        let mut arena = configured_arena();
        let vehicle = admitted_vehicle(&mut arena);
        let handle = arena.actor(vehicle).unwrap().handle;
        let null = arena.actor_handles().unwrap().null_handle();
        vehicle_fields(
            &mut arena,
            vehicle,
            0x1234_5678,
            vec![
                null;
                if matches!(phase, MissingSource::SecondItem) {
                    1
                } else {
                    3
                }
            ],
        );
        if matches!(phase, MissingSource::Behavior) {
            let ActorExtension::Derived(derived) = &mut arena.actor_mut(vehicle).unwrap().extension
            else {
                unreachable!();
            };
            derived.behavior_140 = None;
        } else if matches!(phase, MissingSource::FourthRecord) {
            let source = behavior(&mut arena, vehicle);
            source.byte_10 = 0xff;
            source.byte_11 = 0x80;
            for entry in &mut source.entries_18 {
                entry[0] = 0x8765_4321;
            }
            source.entries_18.truncate(3);
        }
        let first = arena.add_world_player([1; 6]);
        seed_player(&mut arena, first);
        arena.world_player_mut(first).unwrap().handle_d4 = handle;
        enable(&mut arena, first);
        let original = arena.world_player(first).unwrap().clone();
        let mut expected = original.clone();
        expected.dword_144 = 0x1234_5678;
        if matches!(phase, MissingSource::FourthRecord) {
            expected.dword_148 = u32::MAX;
            expected.dword_14c = 0xffff_ff80;
            expected.values_150[..3].fill(0x8765_4321_u32 as i32);
        } else if matches!(phase, MissingSource::SecondItem) {
            expect_copy(
                &mut expected,
                0x1234_5678,
                [0, 8],
                [u32::MAX; 16],
                [0, 0xbbbb, 0xcccc],
            );
        }
        let before = custody(&arena, first);
        let mut mission = mission();
        let expected_mission = mission.clone();
        let error = run(&mut arena, &mut mission, &context(None)).unwrap_err();
        if matches!(phase, MissingSource::SecondItem) {
            assert!(
                matches!(error, WorldCleanupError::Player(PlayerAttachError::VehicleItemStorageBoundary { vehicle: id, index: 1 }) if id == vehicle)
            );
        } else {
            assert!(
                matches!(error, WorldCleanupError::Player(PlayerAttachError::Derived(DerivedActorError::ShortVehicleBehavior(id))) if id == vehicle)
            );
        }
        assert_eq!(arena.world_player(first).unwrap(), &expected);
        assert_eq!(
            arena.actor_handles().unwrap().cache(),
            ActorHandleCache {
                handle,
                object: Some(vehicle)
            }
        );
        assert_eq!(
            mission, expected_mission,
            "the manager's inhibit store is not reached"
        );
        assert_eq!(custody(&arena, first), before);
    }
}

#[test]
fn constructed_player_input_and_message_survive_real_nonvehicle_manager_drain() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let classification = ProfileNameClassification::native_startup(executable).unwrap();
    let text = GrantTextData {
        null_substitute: None,
        pair_classification: classification.high_bytes.as_ref().map(|high| {
            GrantTextPairClassification {
                colon_high: high[b':' as usize],
                space_high: high[b' ' as usize],
            }
        }),
    };
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    let analog = PlayerAnalogDescriptors::native_startup(assets.executable_bytes()).unwrap();
    let mut descriptors = PlayerInputSource {
        analog: &analog,
        executable,
    };
    let process = ParticleFactoryProcessState::native_startup();
    let grants = LiveGrantContext {
        process: Some(&process),
        executable: Some(executable),
        resident: Some(&resident),
        text_data: Some(&text),
    };
    let mut arena = configured_arena();
    let mut input = InputRuntime::new(0x1234_5678);
    let mut carrier = arena.reserve_world_player_construction(0x7700);
    let player = arena
        .construct_world_player(&mut carrier, &mut input, &mut descriptors, &grants)
        .unwrap();
    let shared = arena.allocate_actor(allocation()).unwrap();
    let handle = arena.actor(shared).unwrap().handle;
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    enable(&mut arena, player);
    let players = arena.world_players().to_vec();
    let message = arena.player_message(player).unwrap().clone();
    let PlayerInputDescriptor::Analog(value) = descriptors
        .descriptor(PlayerInputDescriptorRequest {
            kind: PlayerInputDescriptorKind::Analog,
            identity: 0x0054_ecf8,
            player_offset: 0x5c,
        })
        .unwrap()
    else {
        panic!("actual retained analog source");
    };
    let reference = input
        .acquire_analog_consumer(value, carrier.callback_context())
        .unwrap();
    let reference_before = input.analog_consumer_state(reference).unwrap();
    assert_eq!(reference_before.references, 2);
    let input_before = (
        input.clock_micros(),
        input.queued_len(),
        input.stopped(),
        input.selected_context(),
    );
    let carrier_before = (
        carrier.player(),
        carrier.phase(),
        carrier.callback_context(),
        carrier.destruction_phase(),
    );
    let resident_before = format!("{resident:?}");
    let mut registry = arena.actor_handles().unwrap().clone();
    assert_eq!(registry.resolve(handle), Some(shared));
    assert!(registry.release(handle));
    let mut mission = mission();
    let mut expected_mission = mission.clone();
    expected_mission.inhibit_byte_08 = 0;
    run(&mut arena, &mut mission, &grants).unwrap();
    assert_eq!(arena.world_players(), players);
    assert_eq!(arena.first_player(), Some(player));
    assert_eq!(arena.player_message(player).unwrap(), &message);
    assert_eq!(
        input.analog_consumer_state(reference).unwrap(),
        reference_before
    );
    assert_eq!(
        (
            input.clock_micros(),
            input.queued_len(),
            input.stopped(),
            input.selected_context()
        ),
        input_before
    );
    assert_eq!(
        (
            carrier.player(),
            carrier.phase(),
            carrier.callback_context(),
            carrier.destruction_phase()
        ),
        carrier_before
    );
    assert_eq!(format!("{resident:?}"), resident_before);
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{registry:?}")
    );
    assert_eq!(mission, expected_mission);
    assert!(arena.is_empty());
    // Explicit component release uses the same acquiring runtime only after
    // retirement assertions; manager retirement does not perform it.
    carrier.release_input_references(&mut input).unwrap();
    assert_eq!(
        input.analog_consumer_state(reference).unwrap().references,
        1
    );
    input.release_analog_consumer(reference).unwrap();
    assert!(input.analog_consumer_state(reference).is_err());
}
