use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::vehicle_component_tests::{allocation, configured_arena, vehicle},
        ActorBehaviorState, ActorExtension, ActorHandle, ActorHandleCache, DerivedActorClass,
        DerivedActorConstructionRequest, ObjectClassKind, ObjectError,
    },
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{
    parse_constructor_asset_bank, parse_vehicle_behavior_table, ActorConstructorPayload,
    ConstructorPayloadView, FrontendExecutable, GrantTable, PlayerSaveState, GRANT_TABLE_ADDRESS,
    GRANT_TEXT_IDS, VEHICLE_BEHAVIOR_TABLE_DWORDS,
};

struct Inputs {
    payload: Arc<ActorConstructorPayload>,
    behavior: [u32; VEHICLE_BEHAVIOR_TABLE_DWORDS],
}

fn inputs() -> &'static Inputs {
    static INPUTS: OnceLock<Inputs> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let behavior = parse_vehicle_behavior_table(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
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
                    panic!("selector2 shared actor payload")
                };
                return Inputs {
                    payload: Arc::new(payload.clone()),
                    behavior,
                };
            }
        }
        panic!("actual current H13 lacks selector2")
    })
}

fn fixture() -> (ObjectArena, ObjectId, PlayerId, PlayerId) {
    let mut arena = configured_arena();
    let mut rng = 0x1234_5678;
    let vehicle = vehicle(&mut arena, &mut rng);
    arena
        .initialize_actor_payload(vehicle, &inputs().payload, 1)
        .unwrap();
    assert!(arena
        .admit_selector2_actor(vehicle, &inputs().behavior, None)
        .unwrap());
    let first = arena.add_world_player([0x1020_3040; 6]);
    let selected = arena.add_world_player([0x8000_0001; 6]);
    // Actual save-prefix restoration supplies unrelated fields and distinctive
    // output sentinels; no new runtime setter or class replacement is used.
    let saved = PlayerSaveState {
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
    };
    arena.restore_player_save_prefix(selected, &saved).unwrap();
    (arena, vehicle, first, selected)
}

fn derived_mut(
    arena: &mut ObjectArena,
    vehicle: ObjectId,
) -> &mut crate::object_runtime::DerivedActorState {
    let ActorExtension::Derived(state) = &mut arena.actor_mut(vehicle).unwrap().extension else {
        panic!("genuine derived constructor")
    };
    state
}

fn source_mut(arena: &mut ObjectArena, vehicle: ObjectId) -> &mut ActorBehaviorState {
    derived_mut(arena, vehicle).behavior_140.as_mut().unwrap()
}

fn set_handles(arena: &mut ObjectArena, vehicle: ObjectId, handles: Vec<ActorHandle>) {
    let DerivedActorClass::Selector2(state) = &mut derived_mut(arena, vehicle).class else {
        panic!("genuine selector2 constructor")
    };
    state.handles_1a4.handles = handles;
}

fn set_mode(arena: &mut ObjectArena, vehicle: ObjectId, mode: u32) {
    let DerivedActorClass::Selector2(state) = &mut derived_mut(arena, vehicle).class else {
        panic!("genuine selector2 constructor")
    };
    state.dwords_1e8_1f4[0] = mode;
}

fn fields(arena: &ObjectArena, player: PlayerId) -> (u32, u32, u32, [i32; 16], [u16; 3]) {
    let live = arena.world_player(player).unwrap();
    (
        live.dword_144,
        live.dword_148,
        live.dword_14c,
        live.values_150,
        live.items_190,
    )
}

// Borrow every actual arena owner; exhaustive patterns guard future additions.
// Clone only actual players and normalize the selected copy's allowed outputs.
// Registry cache effects have separate exact assertions; owner slots/freechains
// stay included. There is no cloned arena or expected setter/copy engine.
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
    let crate::object_runtime::player::WorldPlayers {
        next_id,
        list,
        camera_74,
        message_boxes,
        message_environment,
        message_effects,
        shown_menus,
        menu_commands,
        destroyed_menus,
        menu_actions,
        ui_cues,
        unit_broadcasts,
        full_notice_gate,
    } = players;
    let mut normalized = list.to_vec();
    assert_eq!(
        normalized.iter().filter(|entry| entry.id == player).count(),
        1
    );
    let live = normalized
        .iter_mut()
        .find(|entry| entry.id == player)
        .unwrap();
    live.dword_144 = 0;
    live.dword_148 = 0;
    live.dword_14c = 0;
    live.values_150 = [0; 16];
    live.items_190 = [0; 3];
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
                map_annotations,
                next_id,
                camera_74,
                (
                    message_boxes.len(),
                    message_environment.is_some(),
                    message_effects,
                    shown_menus.len(),
                    menu_commands,
                    destroyed_menus,
                    menu_actions,
                    ui_cues,
                    unit_broadcasts,
                    full_notice_gate
                )
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
        format!("{normalized:?}"),
    ]
}

fn empty_pickup_tables<'a>(
    grants: &'a GrantTable,
    texts: &'a [u16; GRANT_TEXT_IDS],
    limits: &'a [i32; 16],
) -> crate::object_runtime::PickupTables<'a> {
    crate::object_runtime::PickupTables {
        grants,
        texts,
        upgrade_limits: limits,
        network_active: false,
    }
}

#[test]
fn admitted_constructor_copy_selects_actual_second_player_and_keeps_all_other_custody() {
    let (mut arena, vehicle, first, selected) = fixture();
    let source = arena
        .derived_actor(vehicle)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    assert_eq!(
        (source.byte_10, source.byte_11, source.entries_18.len()),
        (0, 8, 16)
    );
    let null = arena.actor_handles().unwrap().null_handle();
    set_handles(&mut arena, vehicle, vec![null; 3]);
    let before = custody(&arena, selected);
    let first_before = format!("{:?}", arena.world_player(first).unwrap());
    let cache = arena.actor_handles().unwrap().cache();
    arena.save_vehicle_into_player(selected, vehicle).unwrap();
    assert_eq!(fields(&arena, selected), (0, 0, 8, [-1; 16], [0; 3]));
    assert_eq!(custody(&arena, selected), before);
    assert_eq!(
        format!("{:?}", arena.world_player(first).unwrap()),
        first_before
    );
    assert_eq!(arena.actor_handles().unwrap().cache(), cache);
}

#[test]
fn raw_signed_behavior_and_reached_item_words_preserve_bits_and_real_null_policy() {
    let (mut arena, vehicle, _, selected) = fixture();
    let item = arena.allocate_actor(allocation()).unwrap();
    arena.owned_mut(item).unwrap().state.base.resource_word = 0x80ff;
    let stale_object = arena.allocate_actor(allocation()).unwrap();
    let stale = arena.actor(stale_object).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: stale_object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let item_handle = arena.actor(item).unwrap().handle;
    let null = arena.actor_handles().unwrap().null_handle();
    set_mode(&mut arena, vehicle, 0x8000_0002);
    let raw = [
        0x8000_0000,
        0xffff_ffff,
        0x7fff_ffff,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14,
        0x1234_5678,
    ];
    let source = source_mut(&mut arena, vehicle);
    source.byte_10 = 0x80;
    source.byte_11 = 0xff;
    for (entry, word) in source.entries_18.iter_mut().zip(raw) {
        entry[0] = word;
    }
    // Fourth handle is deliberately unread. The released matching identity
    // really resolves NULL and caches that empty slot; missing typed storage
    // is tested separately and never becomes zero.
    set_handles(
        &mut arena,
        vehicle,
        vec![item_handle, stale, null, ActorHandle::from_raw(u32::MAX)],
    );
    let before = custody(&arena, selected);
    arena.save_vehicle_into_player(selected, vehicle).unwrap();
    assert_eq!(
        fields(&arena, selected),
        (
            0x8000_0002,
            0xffff_ff80,
            u32::MAX,
            raw.map(|word| word as i32),
            [0x80ff, 0, 0]
        )
    );
    assert_eq!(custody(&arena, selected), before);
    assert_eq!(
        arena.actor_handles().unwrap().cache(),
        ActorHandleCache {
            handle: stale,
            object: None
        }
    );
}

#[test]
fn missing_behavior_keeps_144_before_later_registry_or_behavior_reads() {
    let (mut arena, vehicle, _, selected) = fixture();
    derived_mut(&mut arena, vehicle).behavior_140 = None;
    set_mode(&mut arena, vehicle, 0xfeed_beef);
    arena.actor_handles = None;
    let before = custody(&arena, selected);
    assert!(matches!(arena.save_vehicle_into_player(selected, vehicle),
        Err(PlayerAttachError::Derived(DerivedActorError::ShortVehicleBehavior(id))) if id == vehicle));
    assert_eq!(
        fields(&arena, selected),
        (
            0xfeed_beef,
            0xbbbb_0002,
            0xcccc_0003,
            [0xdddd_0004_u32 as i32; 16],
            [0xaaaa, 0xbbbb, 0xcccc]
        )
    );
    assert_eq!(custody(&arena, selected), before);
}

#[test]
fn every_short_behavior_prefix_keeps_each_already_reached_head() {
    for available in 0..16 {
        let (mut arena, vehicle, _, selected) = fixture();
        let source = source_mut(&mut arena, vehicle);
        source.byte_10 = 0xff;
        source.byte_11 = 0x80;
        for entry in &mut source.entries_18 {
            entry[0] = 0x8765_4321;
        }
        source.entries_18.truncate(available);
        // No resolver access is reached even when it is unavailable.
        arena.actor_handles = None;
        let before = custody(&arena, selected);
        assert!(matches!(arena.save_vehicle_into_player(selected, vehicle),
            Err(PlayerAttachError::Derived(DerivedActorError::ShortVehicleBehavior(id))) if id == vehicle));
        let live = arena.world_player(selected).unwrap();
        assert_eq!(
            (live.dword_144, live.dword_148, live.dword_14c),
            (0, u32::MAX, 0xffff_ff80)
        );
        assert!(live.values_150[..available]
            .iter()
            .all(|&word| word == 0x8765_4321_u32 as i32));
        assert!(live.values_150[available..]
            .iter()
            .all(|&word| word == 0xdddd_0004_u32 as i32));
        assert_eq!(live.items_190, [0xaaaa, 0xbbbb, 0xcccc]);
        assert_eq!(custody(&arena, selected), before);
    }
}

#[test]
fn every_short_item_prefix_stops_at_storage_after_actual_null_word_stores() {
    for available in 0..3 {
        let (mut arena, vehicle, _, selected) = fixture();
        let null = arena.actor_handles().unwrap().null_handle();
        set_handles(&mut arena, vehicle, vec![null; available]);
        let before = custody(&arena, selected);
        let cache = arena.actor_handles().unwrap().cache();
        assert!(matches!(arena.save_vehicle_into_player(selected, vehicle),
            Err(PlayerAttachError::VehicleItemStorageBoundary { vehicle: id, index })
                if id == vehicle && index == available));
        let live = arena.world_player(selected).unwrap();
        assert_eq!(
            (
                live.dword_144,
                live.dword_148,
                live.dword_14c,
                live.values_150
            ),
            (0, 0, 8, [-1; 16])
        );
        assert_eq!(&live.items_190[..available], &vec![0; available]);
        assert_eq!(
            &live.items_190[available..],
            &[0xaaaa, 0xbbbb, 0xcccc][available..]
        );
        assert_eq!(custody(&arena, selected), before);
        assert_eq!(arena.actor_handles().unwrap().cache(), cache);
    }
}

#[test]
fn missing_registry_is_reached_after_all_scalar_heads_before_first_item_store() {
    let (mut arena, vehicle, _, selected) = fixture();
    set_handles(&mut arena, vehicle, vec![ActorHandle::from_raw(0); 3]);
    arena.actor_handles = None;
    let before = custody(&arena, selected);
    assert!(matches!(
        arena.save_vehicle_into_player(selected, vehicle),
        Err(PlayerAttachError::Object(ObjectError::MissingActorRegistry))
    ));
    assert_eq!(
        fields(&arena, selected),
        (0, 0, 8, [-1; 16], [0xaaaa, 0xbbbb, 0xcccc])
    );
    assert_eq!(custody(&arena, selected), before);
}

#[test]
fn actual_pickup_and_leaf_keep_first_item_and_invalid_referent_cache_on_late_error() {
    for through_pickup in [false, true] {
        let (mut arena, vehicle, _, selected) = fixture();
        let item = arena.allocate_actor(allocation()).unwrap();
        arena.owned_mut(item).unwrap().state.base.resource_word = 0xff81;
        let item_handle = arena.actor(item).unwrap().handle;
        // An actual registry binding to a nonactor is typed containment, not a
        // native memory-fault claim. Resolution caches it before authentication.
        let nonactor = arena.allocate(ObjectClassKind::VisualBase);
        let invalid = arena
            .actor_handles
            .as_mut()
            .unwrap()
            .allocate(1, nonactor)
            .unwrap();
        let vehicle_handle = arena.actor(vehicle).unwrap().handle;
        arena.world_player_mut(selected).unwrap().handle_d4 = vehicle_handle;
        set_handles(
            &mut arena,
            vehicle,
            vec![item_handle, invalid, ActorHandle::from_raw(0)],
        );
        let before = custody(&arena, selected);
        if through_pickup {
            let grants = GrantTable {
                virtual_address: GRANT_TABLE_ADDRESS,
                records: Vec::new(),
            };
            assert!(matches!(
                arena.grant_player_pickup(
                    selected,
                    0,
                    1,
                    empty_pickup_tables(&grants, &[0; GRANT_TEXT_IDS], &[0; 16])
                ),
                Err(PlayerAttachError::Object(ObjectError::MissingActorRegistry))
            ));
        } else {
            assert!(matches!(
                arena.save_vehicle_into_player(selected, vehicle),
                Err(PlayerAttachError::Object(ObjectError::MissingActorRegistry))
            ));
        }
        assert_eq!(
            fields(&arena, selected),
            (0, 0, 8, [-1; 16], [0xff81, 0xbbbb, 0xcccc])
        );
        assert_eq!(custody(&arena, selected), before);
        assert_eq!(
            arena.actor_handles().unwrap().cache(),
            ActorHandleCache {
                handle: invalid,
                object: Some(nonactor)
            }
        );
    }
}

#[test]
fn pickup_keeps_derived_rejection_shared_false_and_reached_copy_prefix_policies() {
    let (mut arena, vehicle, _, selected) = fixture();
    let shared = arena.allocate_actor(allocation()).unwrap();
    let mut rng = 17;
    let derived = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: allocation(),
                world_mask_limits: [3, 3],
            },
            &mut rng,
        )
        .unwrap();
    let grants = GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let texts = [0; GRANT_TEXT_IDS];
    let limits = [0; 16];
    for object in [shared, derived] {
        let handle = arena.actor(object).unwrap().handle;
        arena.world_player_mut(selected).unwrap().handle_d4 = handle;
        let before = custody(&arena, selected);
        let before_fields = fields(&arena, selected);
        assert_eq!(arena.actor_vehicle_component(object).unwrap(), None);
        let result = arena.grant_player_pickup(
            selected,
            2,
            1,
            empty_pickup_tables(&grants, &texts, &limits),
        );
        if object == shared {
            assert_eq!(
                result.unwrap(),
                crate::object_runtime::PickupOutcome {
                    accepted: false,
                    message: None,
                    notification: None,
                    nested: Vec::new(),
                    full: false,
                    command: None,
                    sound: None,
                }
            );
        } else {
            assert!(
                matches!(result, Err(PlayerAttachError::VehicleViewBoundary(id)) if id == derived)
            );
        }
        assert_eq!(fields(&arena, selected), before_fields);
        assert_eq!(custody(&arena, selected), before);
        assert_eq!(
            arena.actor_handles().unwrap().cache(),
            ActorHandleCache {
                handle,
                object: Some(object),
            }
        );
    }
    let handle = arena.actor(vehicle).unwrap().handle;
    arena.world_player_mut(selected).unwrap().handle_d4 = handle;
    derived_mut(&mut arena, vehicle).behavior_140 = None;
    set_mode(&mut arena, vehicle, 0xffff_8000);
    let before = custody(&arena, selected);
    assert!(
        matches!(arena.grant_player_pickup(selected, 0, 1, empty_pickup_tables(&grants, &texts, &limits)),
        Err(PlayerAttachError::Derived(DerivedActorError::ShortVehicleBehavior(id))) if id == vehicle)
    );
    assert_eq!(
        fields(&arena, selected),
        (
            0xffff_8000,
            0xbbbb_0002,
            0xcccc_0003,
            [0xdddd_0004_u32 as i32; 16],
            [0xaaaa, 0xbbbb, 0xcccc]
        )
    );
    assert_eq!(custody(&arena, selected), before);
    assert_eq!(
        arena.actor_handles().unwrap().cache(),
        ActorHandleCache {
            handle,
            object: Some(vehicle),
        }
    );
}
