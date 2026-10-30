use std::path::{Path, PathBuf};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{ActorHandle, ActorRegistryConfig, ObjectClassKind, WorldPlayer},
    HandlerResources, Installation, ResidencyEvent, ResidencyRequest,
};
use inf_formats::RequestKind;

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn executable_bytes() -> Vec<u8> {
    FrontendAssets::open(&data_root())
        .unwrap()
        .executable_bytes()
        .to_vec()
}

fn arena() -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x0801_0000),
    })
    .unwrap();
    let base = arena.allocate(ObjectClassKind::Base);
    arena.set_flag0(base, true).unwrap();
    arena.allocate(ObjectClassKind::VisualBase);
    arena
}

fn absent() -> PlayerCommandContext<'static> {
    PlayerCommandContext {
        process: None,
        executable: None,
        resident: None,
    }
}

fn offline(process: &ParticleFactoryProcessState) -> PlayerCommandContext<'_> {
    PlayerCommandContext {
        process: Some(process),
        executable: None,
        resident: None,
    }
}

fn set(
    arena: &mut ObjectArena,
    player: PlayerId,
    raw_id: u32,
    silent: u32,
    context: &PlayerCommandContext<'_>,
) -> Result<PlayerCommandOutcome, PlayerCommandError> {
    arena.apply_player_command(
        PlayerCommandRequest {
            player,
            operation: PlayerCommandOperation::Set { raw_id, silent },
        },
        context,
    )
}

fn query(
    arena: &mut ObjectArena,
    player: PlayerId,
    raw_id: u32,
) -> Result<PlayerCommandOutcome, PlayerCommandError> {
    arena.apply_player_command(
        PlayerCommandRequest {
            player,
            operation: PlayerCommandOperation::Query { raw_id },
        },
        &absent(),
    )
}

// An actual ID whose local numeric value is absent from our two-player arena.
// This does not broaden bare PlayerId into an arena-qualified identity.
fn missing_player() -> PlayerId {
    let mut foreign = ObjectArena::new();
    foreign.add_world_player([0; 6]);
    foreign.add_world_player([0; 6]);
    foreign.add_world_player([0; 6])
}

fn expected_commands(
    players: &[WorldPlayer],
    player: PlayerId,
    words: [u32; 3],
) -> Vec<WorldPlayer> {
    let mut expected = players.to_vec();
    expected
        .iter_mut()
        .find(|p| p.id == player)
        .unwrap()
        .command_mask_130 = words;
    expected
}

fn other_owners(arena: &ObjectArena) -> String {
    format!(
        "{:?}{:?}{:?}",
        (
            &arena.slots,
            &arena.free,
            arena.live_count,
            &arena.lists,
            &arena.list_owners,
            &arena.routes,
            &arena.actors,
            &arena.map_annotations
        ),
        (
            &arena.actor_handles,
            &arena.actor_payloads,
            &arena.constructor_allocations,
            &arena.selector60_rows,
            arena.world_load
        ),
        (arena.identity, arena.players.camera_74)
    )
}

fn patch_text(bytes: &mut [u8], raw_id: u32, index: u16) {
    let span = FrontendExecutable::parse(bytes)
        .unwrap()
        .mapped(COMMAND_TEXT_ADDRESS + raw_id * 2, 2)
        .unwrap()
        .file_span;
    bytes[span.start..span.end].copy_from_slice(&index.to_le_bytes());
}

// An explicitly supplied structural-EXE variant narrows file backing at one
// reached WORD. Original files and virtual offsets are never overwritten.
fn limit_mapping(bytes: &mut [u8], end_address: u32) {
    let word = |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let pe = word(bytes, 0x3c) as usize;
    let count = u16::from_le_bytes(bytes[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let optional = u16::from_le_bytes(bytes[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let sections = pe + 24 + optional;
    for at in (sections..sections + count * 40).step_by(40) {
        let start = 0x0040_0000 + word(bytes, at + 12);
        let size = word(bytes, at + 16);
        if start < end_address && end_address <= start + size {
            bytes[at + 16..at + 20].copy_from_slice(&(end_address - start).to_le_bytes());
            FrontendExecutable::parse(bytes).unwrap();
            return;
        }
    }
    panic!("declared table boundary must be inside actual mapped file backing");
}

fn boot() -> ResidentRuntime {
    let mut runtime = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
}

fn enable(arena: &mut ObjectArena, player: PlayerId) -> super::super::PlayerMessage88 {
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    recipient
}

#[test]
fn actual_constructor_and_command_zero_are_independent_of_grants() {
    let mut arena = arena();
    let first = arena.add_world_player([0x55aa_55aa; 6]);
    arena.add_world_player([0x1234_5678; 6]);
    let live = arena.world_player(first).unwrap();
    assert_eq!(live.command_mask_130, [0; 3]);
    assert_eq!(live.byte_58, Some(1));
    assert_eq!(
        [
            live.dword_12c,
            live.dword_144,
            live.dword_148,
            live.dword_14c
        ],
        [0, 1, 0, 8]
    );
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    let process = ParticleFactoryProcessState::native_startup();
    let before_process = process;
    assert_eq!(
        query(&mut arena, first, 0).unwrap(),
        PlayerCommandOutcome::Owned(false)
    );
    assert_eq!(
        set(&mut arena, first, 0, 1, &offline(&process)).unwrap(),
        PlayerCommandOutcome::Applied
    );
    assert_eq!(
        query(&mut arena, first, 0).unwrap(),
        PlayerCommandOutcome::Owned(true)
    );
    assert_eq!(
        arena.world_players(),
        expected_commands(&players, first, [1, 0, 0])
    );
    assert_eq!(other_owners(&arena), owners);
    assert_eq!(process, before_process);
}

#[test]
fn signed_raw_mask_uses_truncating_quotient_and_full_dword_identity() {
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    arena.add_world_player([0; 6]);
    let process = ParticleFactoryProcessState::native_startup();
    for raw_id in [(-1i32) as u32, (-31i32) as u32, 31, 32, 63, 64, 95] {
        set(&mut arena, player, raw_id, 1, &offline(&process)).unwrap();
        assert_eq!(
            query(&mut arena, player, raw_id).unwrap(),
            PlayerCommandOutcome::Owned(true)
        );
    }
    assert_eq!(
        arena.world_player(player).unwrap().command_mask_130,
        [0x8000_0002, 0x8000_0001, 0x8000_0001]
    );
    let before = format!("{arena:?}");
    for (raw_id, expected_index) in [((-32i32) as u32, -1), (96, 3), (0xdead_0037, -17_471_486)] {
        assert!(matches!(query(&mut arena, player, raw_id),
            Err(PlayerCommandError::MaskBoundary { word_index, .. }) if word_index == expected_index));
        assert_eq!(format!("{arena:?}"), before);
    }
    assert!(matches!(
        query(&mut arena, missing_player(), 96),
        Err(PlayerCommandError::Player(
            PlayerAttachError::UnknownPlayer(_)
        ))
    ));
}

#[test]
fn network_full_dword_precedes_missing_player_and_mask_but_query_is_pure() {
    let mut arena = arena();
    arena.add_world_player([0; 6]);
    arena.add_world_player([0; 6]);
    let player = missing_player();
    let mut process = ParticleFactoryProcessState::native_startup();
    process.network_flag = 0x100;
    let before = format!("{arena:?}");
    let before_process = process;
    assert_eq!(
        set(&mut arena, player, 0xdead_0037, 0, &offline(&process)).unwrap(),
        PlayerCommandOutcome::Applied
    );
    assert!(matches!(
        query(&mut arena, player, 0xdead_0037),
        Err(PlayerCommandError::Player(
            PlayerAttachError::UnknownPlayer(_)
        ))
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(process, before_process);
}

#[test]
fn process_is_mandatory_even_for_owned_bits_but_owned_skips_all_lower_phases() {
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    arena.add_world_player([0; 6]);
    let process = ParticleFactoryProcessState::native_startup();
    set(&mut arena, player, 1, 1, &offline(&process)).unwrap();
    enable(&mut arena, player);
    let before = format!("{arena:?}");
    assert!(matches!(
        set(&mut arena, player, 1, 0, &absent()),
        Err(PlayerCommandError::MissingProcess { .. })
    ));
    assert!(matches!(
        set(&mut arena, missing_player(), 0xdead_0037, 0, &absent()),
        Err(PlayerCommandError::MissingProcess { .. })
    ));
    assert_eq!(
        set(&mut arena, player, 1, 0, &offline(&process)).unwrap(),
        PlayerCommandOutcome::Applied
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn low_byte_silent_is_consumed_after_commit_and_retry_is_owned() {
    let mut arena = arena();
    let first = arena.add_world_player([3; 6]);
    let second = arena.add_world_player([4; 6]);
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    let process = ParticleFactoryProcessState::native_startup();
    assert!(matches!(
        set(&mut arena, first, 1, 0x100, &offline(&process)),
        Err(PlayerCommandError::MissingExecutable {
            raw_id: 1,
            address: 0x0053_a27e,
            bytes: 2
        })
    ));
    assert_eq!(
        arena.world_players(),
        expected_commands(&players, first, [2, 0, 0])
    );
    assert_eq!(
        set(&mut arena, first, 1, 0, &offline(&process)).unwrap(),
        PlayerCommandOutcome::Applied
    );
    assert_eq!(
        set(&mut arena, second, 1, 0x101, &offline(&process)).unwrap(),
        PlayerCommandOutcome::Applied
    );
    let expected = expected_commands(
        &expected_commands(&players, first, [2, 0, 0]),
        second,
        [2, 0, 0],
    );
    assert_eq!(arena.world_players(), expected);
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn silent_mask_domain_exceeds_text_domain_and_executable_precedes_adjacent_error() {
    let bytes = executable_bytes();
    let process = ParticleFactoryProcessState::native_startup();
    let context = PlayerCommandContext {
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..offline(&process)
    };
    for raw_id in [73, 95, (-1i32) as u32] {
        let mut arena = arena();
        let first = arena.add_world_player([0; 6]);
        let second = arena.add_world_player([0; 6]);
        set(&mut arena, first, raw_id, 1, &offline(&process)).unwrap();
        assert!(matches!(
            set(&mut arena, second, raw_id, 0, &offline(&process)),
            Err(PlayerCommandError::MissingExecutable { .. })
        ));
        let committed = arena.world_player(second).unwrap().command_mask_130;
        let third = arena.add_world_player([0; 6]);
        assert!(matches!(set(&mut arena, third, raw_id, 0, &context),
            Err(PlayerCommandError::AdjacentTextTable { raw_id: actual, .. }) if actual == raw_id));
        assert_eq!(
            arena.world_player(third).unwrap().command_mask_130,
            committed
        );
    }
}

#[test]
fn supplied_negative_text_bypasses_residency_but_reaches_same_player_message() {
    let mut bytes = executable_bytes();
    patch_text(&mut bytes, 1, 0xffff);
    let before_bytes = bytes.clone();
    let process = ParticleFactoryProcessState::native_startup();
    let context = PlayerCommandContext {
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..offline(&process)
    };
    let mut arena = arena();
    let first = arena.add_world_player([0; 6]);
    let second = arena.add_world_player([0; 6]);
    set(&mut arena, first, 1, 0, &context).unwrap();
    let recipient = enable(&mut arena, second);
    assert!(matches!(set(&mut arena, second, 1, 0, &context),
        Err(PlayerCommandError::Message(PlayerMessageError::EnabledMessageBoundary(actual))) if actual == recipient));
    assert_eq!(
        arena.world_player(second).unwrap().command_mask_130,
        [2, 0, 0]
    );
    assert_eq!(bytes, before_bytes);
}

#[test]
fn reached_executable_mapping_failure_retains_commit_before_any_h0_or_message() {
    let mut bytes = executable_bytes();
    let address = COMMAND_TEXT_ADDRESS + 0x48 * 2;
    limit_mapping(&mut bytes, address);
    let process = ParticleFactoryProcessState::native_startup();
    let context = PlayerCommandContext {
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..offline(&process)
    };
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    arena.add_world_player([0; 6]);
    enable(&mut arena, player);
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    assert!(matches!(set(&mut arena, player, 0x48, 0, &context),
        Err(PlayerCommandError::Executable { raw_id: 0x48, address: actual, bytes: 2, .. }) if actual == address));
    assert_eq!(
        arena.world_players(),
        expected_commands(&players, player, [0, 0, 0x100])
    );
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn h0_missing_equal_greater_and_hole_keep_the_committed_bit() {
    let resident = boot();
    assert_eq!(resident.resource_capacity(0), Some(950));
    let before_resident = resident.summary();
    let revision = resident.revision();
    let process = ParticleFactoryProcessState::native_startup();
    for index in [950, 951, 0x2b5] {
        let mut bytes = executable_bytes();
        patch_text(&mut bytes, 1, index);
        let context = PlayerCommandContext {
            executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
            ..offline(&process)
        };
        let mut arena = arena();
        let missing = arena.add_world_player([0; 6]);
        let present = arena.add_world_player([0; 6]);
        assert!(matches!(set(&mut arena, missing, 1, 0, &context),
            Err(PlayerCommandError::MissingResidency { index: actual }) if actual == index));
        let context = PlayerCommandContext {
            resident: Some(&resident),
            ..context
        };
        if index == 950 {
            assert!(matches!(
                set(&mut arena, present, 1, 0, &context),
                Err(PlayerCommandError::H0OnePast {
                    index: 950,
                    count: 950
                })
            ));
        } else {
            if index == 0x2b5 {
                assert!(resident.resource(0, index).is_none());
            }
            set(&mut arena, present, 1, 0, &context).unwrap();
        }
        assert_eq!(
            arena.world_player(missing).unwrap().command_mask_130,
            [2, 0, 0]
        );
        assert_eq!(
            arena.world_player(present).unwrap().command_mask_130,
            [2, 0, 0]
        );
    }
    assert_eq!(resident.summary(), before_resident);
    assert_eq!(resident.revision(), revision);
}

#[test]
fn original_command_table_and_actual_leaf_one_language_allocations_are_used() {
    let bytes = executable_bytes();
    let bytes_before = bytes.clone();
    let process = ParticleFactoryProcessState::native_startup();
    let mut resident = boot();
    assert_eq!(resident.summary().active_leaves, [0, 2]);
    let change = resident
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let leaves = change
        .events
        .iter()
        .filter_map(|event| match event {
            ResidencyEvent::LeafReleased { level } => Some((false, *level)),
            ResidencyEvent::LeafActivated { level } => Some((true, *level)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(leaves, [(false, 2), (true, 1), (true, 48)]);
    assert_eq!(resident.summary().active_leaves, [0, 1, 48]);
    assert!(resident.resource(0, 872).is_none());
    assert!(resident.resource(0, 873).is_none());
    let current = resident.summary();
    let revision = resident.revision();
    let context = PlayerCommandContext {
        process: Some(&process),
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        resident: Some(&resident),
    };
    let mut arena = arena();
    let player = arena.add_world_player([0xa5a5_a5a5; 6]);
    arena.add_world_player([0x5a5a_5a5a; 6]);
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    for (raw_id, index) in [
        (0, 0x2b4),
        (1, 0x2b5),
        (0x37, 0x2eb),
        (0x41, 0x170),
        (0x42, 0x2f5),
        (0x48, 0x366),
    ] {
        assert_eq!(command_text_word(&context, raw_id).unwrap(), index);
        let resource = resident.resource(0, index).unwrap();
        if raw_id == 0x41 {
            assert_eq!(resource.registration.level, 0);
        } else {
            assert_eq!(resource.registration.level, 1);
            assert_eq!(resource.registration.phase, RequestKind::Language);
        }
        assert_eq!(resource.registration.global_index, index);
        let HandlerResources::Strings(bank) =
            &resource.overlay.handlers()[resource.registration.region_index]
        else {
            panic!("actual string bank")
        };
        let expected = bank.entries[usize::from(resource.registration.local_index)]
            .record
            .get(resource.overlay.bytes())
            .unwrap();
        assert_eq!(expected.last(), Some(&0));
        assert_eq!(
            capture_h0_text(H0TextCaptureRequest {
                index_word: index,
                resident: Some(&resident)
            })
            .unwrap(),
            GrantTextSource::Record(Some(expected))
        );
        set(&mut arena, player, raw_id, 0, &context).unwrap();
    }
    assert_eq!(
        arena.world_players(),
        expected_commands(&players, player, [3, 0x0080_0000, 0x106])
    );
    assert_eq!(other_owners(&arena), owners);
    assert_eq!(resident.summary(), current);
    assert_eq!(resident.revision(), revision);
    assert_eq!(bytes, bytes_before);
}

#[test]
fn original_command_41_consumes_reordered_root_text_index_after_fresh_commit() {
    let bytes = executable_bytes();
    let process = ParticleFactoryProcessState::native_startup();
    let context = PlayerCommandContext {
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..offline(&process)
    };
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    arena.add_world_player([0; 6]);
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    assert!(matches!(
        set(&mut arena, player, 0x41, 0, &context),
        Err(PlayerCommandError::MissingResidency { index: 0x170 })
    ));
    assert_eq!(
        arena.world_players(),
        expected_commands(&players, player, [0, 0, 2])
    );
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn missing_h0_precedes_enabled_message_and_recipient_is_requested_second_player() {
    let bytes = executable_bytes();
    let resident = boot();
    let process = ParticleFactoryProcessState::native_startup();
    let mut arena = arena();
    let first = arena.add_world_player([1; 6]);
    let second = arena.add_world_player([2; 6]);
    enable(&mut arena, first);
    let first_before = arena.world_player(first).unwrap().clone();
    let owners = other_owners(&arena);
    let context = PlayerCommandContext {
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..offline(&process)
    };
    let recipient = enable(&mut arena, second);
    assert!(matches!(
        set(&mut arena, second, 0, 0, &context),
        Err(PlayerCommandError::MissingResidency { index: 0x2b4 })
    ));
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Suppress,
        })
        .unwrap();
    let context = PlayerCommandContext {
        resident: Some(&resident),
        ..context
    };
    set(&mut arena, second, 1, 0, &context).unwrap();
    enable(&mut arena, second);
    assert!(matches!(set(&mut arena, second, 2, 0, &context),
        Err(PlayerCommandError::Message(PlayerMessageError::EnabledMessageBoundary(actual))) if actual == recipient));
    assert_eq!(
        arena.world_player(second).unwrap().command_mask_130,
        [7, 0, 0]
    );
    assert_eq!(arena.world_player(first).unwrap(), &first_before);
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn reborrowed_live_process_changes_do_not_rewrite_captured_commands() {
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    arena.add_world_player([0; 6]);
    let mut process = ParticleFactoryProcessState::native_startup();
    let session_phase =
        crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(Some(
            0xdead_beef,
        ));
    process.process_null_handle = None;
    set(&mut arena, player, 32, 1, &offline(&process)).unwrap();
    process.network_flag = 0x100;
    set(&mut arena, player, 64, 1, &offline(&process)).unwrap();
    assert_eq!(
        query(&mut arena, player, 64).unwrap(),
        PlayerCommandOutcome::Owned(false)
    );
    process.network_flag = 0;
    set(&mut arena, player, 64, 1, &offline(&process)).unwrap();
    assert_eq!(
        arena.world_player(player).unwrap().command_mask_130,
        [0, 1, 1]
    );
    assert_eq!(session_phase.snapshot(), Some(0xdead_beef));
    assert_eq!(process.process_null_handle, None);
}

#[test]
fn real_save_restore_message_and_checkpoint_operations_preserve_command_owner() {
    let mut arena = arena();
    let first = arena.add_world_player([0; 6]);
    let second = arena.add_world_player([0; 6]);
    let first_before = arena.world_player(first).unwrap().clone();
    let recipient = enable(&mut arena, second);
    {
        let live = arena.world_player_mut(second).unwrap();
        live.byte_ed = 1;
        live.dword_f0 = 0x1234_5678;
    }
    let before = arena.world_player(second).unwrap().clone();
    let owners = other_owners(&arena);
    let saved = inf_formats::PlayerSaveState {
        owner_130: [0x1234_5678, 0x8000_0001, 0xffff_ffff],
        ..inf_formats::PlayerSaveState::new_player([0; 6])
    };
    arena.restore_player_save_prefix(second, &saved).unwrap();
    let live = arena.world_player(second).unwrap();
    assert_eq!(live.command_mask_130, saved.owner_130);
    assert_eq!(live.handle_d4, before.handle_d4);
    assert_eq!(live.handle_d8, before.handle_d8);
    assert_eq!(live.byte_58, Some(1));
    let mut expected = before;
    expected.bits_100 = saved.owner_100;
    expected.word_11c = saved.owner_11c;
    expected.command_mask_130 = saved.owner_130;
    expected.dword_144 = saved.owner_144;
    expected.dword_148 = saved.owner_148;
    expected.dword_14c = saved.owner_14c;
    expected.values_150 = saved.owner_150.map(|word| word as i32);
    expected.items_190 = saved.owner_190;
    expected.grants_9c = saved.owner_a0;
    assert_eq!(live, &expected);
    assert_eq!(arena.player_message(second).unwrap().suppression_byte(), 0);
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Suppress,
        })
        .unwrap();
    let mut expected = arena.world_player(second).unwrap().clone();
    expected.byte_ed = 0;
    expected.dword_f0 = 0;
    arena.clear_player_completion_checkpoint(second).unwrap();
    assert_eq!(
        arena.world_player(second).unwrap().command_mask_130,
        saved.owner_130
    );
    assert_eq!(arena.world_player(second).unwrap(), &expected);
    assert_eq!(arena.world_player(first).unwrap(), &first_before);
    assert_eq!(other_owners(&arena), owners);
}
