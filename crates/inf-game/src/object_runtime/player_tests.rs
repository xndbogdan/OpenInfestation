use super::*;
use crate::object_runtime::{ActorRegistryConfig, ActorTerrainInputs};

const SINE: [u16; 4096] = [0; 4096];
const PROCESS: ParticleFactoryProcessState = ParticleFactoryProcessState {
    network_flag: 0,
    local_slot: 1,
    host_flag: 0,
    settings: [0; 20],
    process_null_handle: Some(ActorHandle::from_raw(0)),
};

fn lookup(
    arena: &ObjectArena,
    handle: ActorHandle,
    process: Option<&ParticleFactoryProcessState>,
) -> Result<Option<PlayerId>, PlayerActorLookupError> {
    arena.find_player_for_actor_handle(
        PlayerActorLookupRequest { handle },
        PlayerActorLookupContext { process },
    )
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn request() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
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
    }
}

fn save_copy_other_custody(arena: &ObjectArena) -> [String; 2] {
    [
        format!(
            "{:?}",
            (
                arena.identity,
                &arena.slots,
                &arena.free,
                arena.live_count,
                &arena.lists,
                &arena.list_owners,
                &arena.routes,
                &arena.actors,
                &arena.map_annotations,
            )
        ),
        format!(
            "{:?}",
            (
                &arena.actor_handles,
                &arena.actor_payloads,
                &arena.constructor_allocations,
                &arena.selector60_rows,
                &arena.world_load,
                arena.players.next_id,
            )
        ),
    ]
}

#[test]
fn players_append_in_order_with_null_handles_and_first_is_the_list_head() {
    let mut arena = arena();
    assert_eq!(arena.first_player(), None);
    let first = arena.add_world_player([1, 0, 0, 0, 0, 0]);
    let second = arena.add_world_player([2, 0, 0, 0, 0, 0]);
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(
        arena
            .world_players()
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [first, second]
    );
    let player = arena.world_player(second).unwrap();
    assert_eq!(player.handle_d4, ActorHandle::from_raw(0));
    assert_eq!(player.handle_d8, ActorHandle::from_raw(0));
    assert_eq!(arena.camera_handle(), None);
}

#[test]
fn player_actor_prefers_d8_then_falls_back_to_d4() {
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    let actor = arena.allocate_actor(request()).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    assert_eq!(arena.player_actor(player).unwrap(), None);
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    assert_eq!(arena.player_actor(player).unwrap(), Some(actor));
    let other = arena.allocate_actor(request()).unwrap();
    arena.world_player_mut(player).unwrap().handle_d8 = arena.actor(other).unwrap().handle;
    assert_eq!(arena.player_actor(player).unwrap(), Some(other));
}

#[test]
fn completion_lookup_uses_nonnull_d8_even_when_it_does_not_resolve() {
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    let actor = arena.allocate_actor(request()).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    assert_eq!(lookup(&arena, handle, Some(&PROCESS)), Ok(Some(player)));
    let other = ActorHandle::from_raw(0x1234);
    arena.world_player_mut(player).unwrap().handle_d8 = other;
    assert_eq!(lookup(&arena, handle, Some(&PROCESS)), Ok(None));
    assert_eq!(lookup(&arena, other, Some(&PROCESS)), Ok(Some(player)));
    assert_eq!(
        lookup(&arena, ActorHandle::from_raw(0), Some(&PROCESS)),
        Ok(None)
    );
}

#[test]
fn lookup_requires_live_process_null_even_for_empty_list_and_raw_zero_source() {
    let arena = ObjectArena::new();
    let unavailable = ParticleFactoryProcessState {
        process_null_handle: None,
        ..PROCESS
    };
    for process in [None, Some(&unavailable)] {
        for handle in [ActorHandle::from_raw(0), ActorHandle::from_raw(u32::MAX)] {
            assert_eq!(
                lookup(&arena, handle, process),
                Err(PlayerActorLookupError::MissingProcessNullHandle)
            );
        }
    }
    // A typed arena owns its manager. Empty players is distinct from native
    // manager-null or an unavailable raw node backing.
    assert_eq!(
        lookup(&arena, ActorHandle::from_raw(0), Some(&PROCESS)),
        Ok(None)
    );
    assert_eq!(
        lookup(&arena, ActorHandle::from_raw(1), Some(&PROCESS)),
        Ok(None)
    );
}

#[test]
fn lookup_uses_full_live_null_identity_and_authoritative_unresolved_d8() {
    let mut arena = arena();
    let first = arena.add_world_player([0; 6]);
    let second = arena.add_world_player([0; 6]);
    let fallback = ActorHandle::from_raw(0x8000_0100);
    let mut process = ParticleFactoryProcessState {
        process_null_handle: Some(ActorHandle::from_raw(0x8000_0000)),
        ..PROCESS
    };
    arena.world_player_mut(first).unwrap().handle_d4 = fallback;
    // Registry NULL/raw zero is an authoritative D8 when it differs from the
    // captured process NULL, even though no actor can resolve from it.
    arena.world_player_mut(first).unwrap().handle_d8 = ActorHandle::from_raw(0);
    arena.world_player_mut(second).unwrap().handle_d4 = fallback;
    arena.world_player_mut(second).unwrap().handle_d8 = process.process_null_handle.unwrap();
    let players = arena.world_players().to_vec();
    let cache = arena.actor_handles.as_ref().unwrap().cache();
    let owners = arena.actor_handles.as_ref().unwrap().owners().to_vec();
    assert_eq!(
        lookup(&arena, ActorHandle::from_raw(0), Some(&process)),
        Ok(Some(first))
    );
    assert_eq!(lookup(&arena, fallback, Some(&process)), Ok(Some(second)));
    assert_eq!(
        lookup(&arena, process.process_null_handle.unwrap(), Some(&process)),
        Ok(None)
    );
    assert_eq!(arena.world_players(), players);
    assert_eq!(arena.actor_handles.as_ref().unwrap().cache(), cache);
    assert_eq!(arena.actor_handles.as_ref().unwrap().owners(), owners);

    // Each invocation reads this same live owner anew, rather than retaining
    // either the registry constructor NULL or the previous process value.
    process.process_null_handle = Some(ActorHandle::from_raw(0));
    assert_eq!(lookup(&arena, fallback, Some(&process)), Ok(Some(first)));
    assert_eq!(
        lookup(&arena, ActorHandle::from_raw(0), Some(&process)),
        Ok(None)
    );
}

#[test]
fn lookup_needs_no_actor_registry_and_returns_first_matching_raw_player() {
    let mut arena = arena();
    let first = arena.add_world_player([0; 6]);
    let second = arena.add_world_player([0; 6]);
    let raw = ActorHandle::from_raw(0xffff_ffff);
    arena.world_player_mut(first).unwrap().handle_d8 = raw;
    arena.world_player_mut(second).unwrap().handle_d8 = raw;
    arena.actor_handles = None;
    let players = arena.world_players().to_vec();
    assert_eq!(lookup(&arena, raw, Some(&PROCESS)), Ok(Some(first)));
    assert_eq!(
        lookup(&arena, ActorHandle::from_raw(0), Some(&PROCESS)),
        Ok(None)
    );
    assert_eq!(arena.world_players(), players);
    assert!(arena.actor_handles.is_none());
}

#[test]
fn completion_checkpoint_reset_clears_only_ed_and_f0() {
    let mut arena = arena();
    let player = arena.add_world_player([1; 6]);
    let state = arena.world_player_mut(player).unwrap();
    state.byte_ed = 1;
    state.byte_ee = 2;
    state.dword_f0 = 0x1234;
    state.saved_e0 = Some([3, 4, 5]);
    state.byte_ec = Some(6);
    let mut expected = state.clone();
    expected.byte_ed = 0;
    expected.dword_f0 = 0;
    arena.clear_player_completion_checkpoint(player).unwrap();
    assert_eq!(arena.world_player(player).unwrap(), &expected);
}

#[test]
fn complete_save_prefix_restore_preserves_handles_and_checkpoint_fields() {
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    arena.add_world_player([0x1357_9bdf; 6]);
    let actor = arena.allocate_actor(request()).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    let live = arena.world_player_mut(player).unwrap();
    live.handle_d4 = handle;
    live.handle_d8 = ActorHandle::from_raw(0x7654_3210);
    live.byte_ed = 1;
    live.byte_ee = 2;
    live.dword_f0 = 123;
    live.saved_e0 = Some([3, 4, 5]);
    live.byte_ec = Some(6);
    live.dword_12c = 0x1234_5678;
    let saved = inf_formats::PlayerSaveState {
        owner_100: [1, 2, 3, 4, 5, 6],
        owner_118: 0x1357_2468,
        owner_11c: 7,
        owner_11e: 0xabcd,
        owner_120: [0x1111_1111, 0x2222_2222, 0x3333_3333, 0x8000_012c],
        owner_13c: [0xaaaa_aaaa, 0xbbbb_bbbb],
        owner_130: [0x1234_5678, 0x8000_0001, 0xffff_ffff],
        owner_144: 8,
        owner_148: 9,
        owner_14c: 10,
        owner_150: [u32::MAX; 16],
        owner_190: [11, 12, 13],
        owner_a0: [14; 6],
        ..Default::default()
    };
    let saved_before = saved.clone();
    let objects_before = format!("{:?}", arena.slots);
    let registry_before = format!("{:?}", arena.actor_handles);
    let mut expected = arena.world_players().to_vec();
    let restored = expected.iter_mut().find(|live| live.id == player).unwrap();
    restored.bits_100 = saved.owner_100;
    restored.dword_118 = saved.owner_118;
    restored.word_11c = saved.owner_11c;
    restored.word_11e = saved.owner_11e;
    restored.dwords_120_128 = saved.owner_120[..3].try_into().unwrap();
    restored.dword_12c = saved.owner_120[3];
    restored.bytes_13c_143 = [0xaa, 0xaa, 0xaa, 0xaa, 0xbb, 0xbb, 0xbb, 0xbb];
    restored.command_mask_130 = saved.owner_130;
    restored.dword_144 = saved.owner_144;
    restored.dword_148 = saved.owner_148;
    restored.dword_14c = saved.owner_14c;
    restored.values_150 = saved.owner_150.map(|word| word as i32);
    restored.items_190 = saved.owner_190;
    restored.grants_9c = saved.owner_a0;
    arena.restore_player_save_prefix(player, &saved).unwrap();
    assert_eq!(arena.world_players(), expected);
    assert_eq!(saved, saved_before);
    assert_eq!(format!("{:?}", arena.slots), objects_before);
    assert_eq!(format!("{:?}", arena.actor_handles), registry_before);
    let live = arena.world_player(player).unwrap();
    assert_eq!(live.handle_d4, handle);
    assert_eq!(live.byte_ed, 1);
    assert_eq!(live.dword_f0, 123);
    assert_eq!(live.bits_100, saved.owner_100);
    assert_eq!(live.dword_118, saved.owner_118);
    assert_eq!(live.word_11c, 7);
    assert_eq!(live.word_11e, saved.owner_11e);
    assert_eq!(live.dwords_120_128, [0x1111_1111, 0x2222_2222, 0x3333_3333]);
    assert_eq!(live.dword_12c, 0x8000_012c);
    assert_eq!(
        live.bytes_13c_143,
        [0xaa, 0xaa, 0xaa, 0xaa, 0xbb, 0xbb, 0xbb, 0xbb]
    );
    assert_eq!(live.command_mask_130, saved.owner_130);
    assert_eq!([live.dword_144, live.dword_148, live.dword_14c], [8, 9, 10]);
    assert_eq!(live.values_150, [-1; 16]);
    assert_eq!(live.items_190, saved.owner_190);
    assert_eq!(live.grants_9c, saved.owner_a0);
    assert_eq!(arena.export_player_save_state(player).unwrap(), saved);
    let missing = PlayerId(u64::MAX);
    assert!(matches!(
        arena.restore_player_save_prefix(missing, &saved),
        Err(PlayerAttachError::UnknownPlayer(actual)) if actual == missing
    ));
    assert!(matches!(
        arena.export_player_save_state(missing),
        Err(PlayerAttachError::UnknownPlayer(actual)) if actual == missing
    ));
    assert_eq!(arena.world_players(), expected);
    assert_eq!(saved, saved_before);
    assert_eq!(format!("{:?}", arena.slots), objects_before);
    assert_eq!(format!("{:?}", arena.actor_handles), registry_before);
}

#[test]
fn player_save_export_uses_genuine_constructor_fields_and_selected_player() {
    let mut arena = arena();
    let first_grants = [1, 2, 3, 4, 5, 6];
    let second_grants = [u32::MAX, 0x8000_0000, 7, 8, 9, 10];
    let first = arena.add_world_player(first_grants);
    let second = arena.add_world_player(second_grants);
    let actor = arena.allocate_actor(request()).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    arena.world_player_mut(second).unwrap().handle_d4 = handle;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.export_player_save_state(first).unwrap(),
        inf_formats::PlayerSaveState::new_player(first_grants)
    );
    assert_eq!(
        arena.export_player_save_state(second).unwrap(),
        inf_formats::PlayerSaveState::new_player(second_grants)
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn player_save_copy_retains_raw_high_bytes_and_nuls_for_the_actual_second_player() {
    let mut arena = arena();
    let first = arena.add_world_player([0x1357_2468; 6]);
    let second = arena.add_world_player([0; 6]);
    let actor = arena.allocate_actor(request()).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    arena.world_player_mut(second).unwrap().handle_d8 = handle;
    let recipient = arena.player_message(second).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    let first_before = arena.world_player(first).unwrap().clone();
    let other_owners = save_copy_other_custody(&arena);
    let saved = inf_formats::PlayerSaveState {
        owner_100: [0x8000_0000, 1, 2, 3, 4, u32::MAX],
        owner_118: 0x8000_0118,
        owner_11c: 0xffff,
        owner_11e: 0x8001,
        owner_120: [0x8000_0120, u32::MAX, 0x7654_0128, 0x8000_012c],
        owner_144: 0x8000_0144,
        owner_148: u32::MAX,
        owner_14c: 0x7654_014c,
        owner_150: std::array::from_fn(|index| 0x8000_0000 | index as u32),
        // Bytes 00 FF 80 41 | 42 00 FE 81 contain both NUL and high-bit data.
        owner_13c: [0x4180_ff00, 0x81fe_0042],
        owner_190: [0x8000, 0xffff, 0x7654],
        owner_130: [0x8000_0130, u32::MAX, 0x7654_0138],
        owner_a0: [u32::MAX; 6],
    };
    let saved_before = saved.clone();
    arena.restore_player_save_prefix(second, &saved).unwrap();
    let live = arena.world_player(second).unwrap();
    assert_eq!(
        live.bytes_13c_143,
        [0, 0xff, 0x80, b'A', b'B', 0, 0xfe, 0x81]
    );
    assert_eq!(live.handle_d8, handle);
    assert_eq!(arena.player_message(second).unwrap().identity(), recipient);
    assert_eq!(arena.player_message(second).unwrap().suppression_byte(), 0);
    let after_restore = format!("{arena:?}");
    assert_eq!(arena.export_player_save_state(second).unwrap(), saved);
    assert_eq!(format!("{arena:?}"), after_restore);
    assert_eq!(saved, saved_before);
    assert_eq!(arena.world_player(first).unwrap(), &first_before);
    assert_eq!(save_copy_other_custody(&arena), other_owners);
}

#[test]
fn player_save_export_is_pure_without_actor_registry_and_errors_for_unknown_player() {
    let mut arena = arena();
    let player = arena.add_world_player([0x1234_5678; 6]);
    // The save-copy consumer reads raw player values and never resolves D4/D8.
    arena.world_player_mut(player).unwrap().handle_d8 = ActorHandle::from_raw(u32::MAX);
    arena.actor_handles = None;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.export_player_save_state(player).unwrap(),
        inf_formats::PlayerSaveState::new_player([0x1234_5678; 6])
    );
    let missing = PlayerId(u64::MAX);
    assert!(matches!(
        arena.export_player_save_state(missing),
        Err(PlayerAttachError::UnknownPlayer(actual)) if actual == missing
    ));
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn player_saved_adjacent_bytes_do_not_widen_command_mask_admission() {
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    let saved = inf_formats::PlayerSaveState {
        owner_120: [u32::MAX; 4],
        owner_13c: [u32::MAX; 2],
        owner_130: [2, 0, 0x8000_0000],
        ..inf_formats::PlayerSaveState::new_player([0; 6])
    };
    arena.restore_player_save_prefix(player, &saved).unwrap();
    let process = ParticleFactoryProcessState::native_startup();
    let context = PlayerCommandContext {
        process: Some(&process),
        executable: None,
        resident: None,
    };
    let before = format!("{arena:?}");
    let process_before = process;
    // Native group3 aliases13C; group-1 aliases12C. Both remain explicit
    // typed boundaries although these adjacent save values are now owned.
    for (raw_id, word_index) in [(96, 3), ((-32_i32) as u32, -1)] {
        for operation in [
            PlayerCommandOperation::Query { raw_id },
            PlayerCommandOperation::Set { raw_id, silent: 1 },
        ] {
            assert!(matches!(
                arena.apply_player_command(PlayerCommandRequest { player, operation }, &context),
                Err(PlayerCommandError::MaskBoundary { player: actual, raw_id: raw, word_index: index })
                    if actual == player && raw == raw_id && index == word_index
            ));
        }
    }
    for (raw_id, expected) in [(95, true), (94, false), ((-31_i32) as u32, true)] {
        assert_eq!(
            arena
                .apply_player_command(
                    PlayerCommandRequest {
                        player,
                        operation: PlayerCommandOperation::Query { raw_id },
                    },
                    &context,
                )
                .unwrap(),
            PlayerCommandOutcome::Owned(expected)
        );
    }
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(process, process_before);
}

/// 0046B9C8 and 004231C0: in a network game, pickup 0x77 gives a vehicle
/// with a clear +5AC two seconds of full power (+5AC and +224) and sound
/// 0x19 at the vehicle; while the boost runs, or offline, nothing is needed.
#[test]
fn a_network_boost_pickup_drives_two_seconds_flat_out() {
    let mut settings = [0; 20];
    settings[0x10 / 4] = 2;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (_resident, assets, mut world) = crate::test_worlds::network_world(33, process);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let texts = inf_formats::parse_grant_texts(executable).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable).unwrap();
    let arena = &mut world.arena;
    let player = arena.first_player().unwrap();
    let vehicle = arena.first_player_primary_actor().unwrap().unwrap();
    let tables = |network_active| PickupTables {
        grants: &grants,
        texts: &texts,
        upgrade_limits: &limits,
        network_active,
    };
    let offline = arena
        .grant_player_pickup(player, 0x77, 1, tables(false))
        .unwrap();
    assert!(!offline.accepted && offline.sound.is_none());
    let boost = arena
        .grant_player_pickup(player, 0x77, 1, tables(true))
        .unwrap();
    let at = arena.object(vehicle).unwrap().base.position_words.unwrap();
    assert_eq!(
        boost.sound,
        Some(PickupSound {
            sound: 0x19,
            position: at
        })
    );
    assert!(boost.accepted && boost.message.is_none());
    let state = arena.selector2_actor(vehicle).unwrap();
    assert_eq!(
        (state.dwords_5ac_5b0[0], state.dwords_220_224[1]),
        (2_000_000, 2_000_000)
    );
    let again = arena
        .grant_player_pickup(player, 0x77, 1, tables(true))
        .unwrap();
    assert!(!again.accepted && again.sound.is_none());
}
