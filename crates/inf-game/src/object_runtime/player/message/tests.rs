use super::*;
use crate::{
    object_runtime::{ActorHandle, ActorRegistryConfig, ObjectClassKind, WorldPlayer},
    process_particles::factories::ParticleFactoryProcessState,
};

fn arena() -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let base = arena.allocate(ObjectClassKind::Base);
    arena.set_flag0(base, true).unwrap();
    arena.allocate(ObjectClassKind::VisualBase);
    arena
}

fn message(arena: &ObjectArena, player: PlayerId) -> PlayerMessage88 {
    arena.player_message(player).unwrap().identity()
}

fn apply(
    arena: &mut ObjectArena,
    recipient: PlayerMessage88,
    operation: PlayerMessageOperation<'_>,
) -> Result<(), PlayerMessageError> {
    arena.apply_player_message(PlayerMessageRequest {
        recipient,
        operation,
    })
}

// Every actual arena owner except the player records, which are compared
// separately with only the declared post-call assertion byte changed.
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
            &arena.map_annotations,
        ),
        (
            &arena.actor_handles,
            &arena.actor_payloads,
            &arena.constructor_allocations,
            &arena.selector60_rows,
            arena.world_load,
        ),
        (
            arena.identity,
            arena.players.next_id,
            arena.players.camera_74,
        ),
    )
}

fn expected_byte(players: &[WorldPlayer], player: PlayerId, byte: u8) -> Vec<WorldPlayer> {
    let mut expected = players.to_vec();
    expected
        .iter_mut()
        .find(|record| record.id == player)
        .unwrap()
        .message_88
        .suppression_01 = byte;
    expected
}

#[test]
fn actual_player_constructor_retains_distinct_message_identities_and_live_byte() {
    let mut arena = arena();
    let first = arena.add_world_player([1, 2, 3, 4, 5, 6]);
    let second = arena.add_world_player([6, 5, 4, 3, 2, 1]);
    let before = format!("{arena:?}");
    let first_message = arena.player_message(first).unwrap();
    let second_message = arena.player_message(second).unwrap();
    assert_eq!(first_message.suppression_byte(), 1);
    assert_eq!(second_message.suppression_byte(), 1);
    assert_ne!(first_message.identity(), second_message.identity());
    assert_eq!(first_message.identity().arena, arena.identity);
    assert_eq!(first_message.identity().player, first);
    assert_eq!(second_message.identity().player, second);
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(
        arena.world_player(first).unwrap().grants_9c,
        [1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        arena.world_player(second).unwrap().handle_d8,
        ActorHandle::from_raw(0x1234)
    );
    assert_eq!(format!("{arena:?}"), before, "state readers are pure");
}

#[test]
fn foreign_coincident_player_and_unknown_message_fail_before_suppression() {
    let mut local = arena();
    let mut foreign = arena();
    let local_player = local.add_world_player([0; 6]);
    let foreign_player = foreign.add_world_player([0; 6]);
    assert_eq!(local_player, foreign_player, "bare PlayerId is still local");
    let foreign_message = message(&foreign, foreign_player);
    let missing = PlayerMessage88 {
        arena: local.identity,
        // Declared private test identity; no public numeric constructor exists.
        player: PlayerId(u64::MAX),
    };
    let local_before = format!("{local:?}");
    let foreign_before = format!("{foreign:?}");
    for operation in [
        PlayerMessageOperation::CleanupWorld,
        PlayerMessageOperation::Suppress,
        PlayerMessageOperation::Enable,
        PlayerMessageOperation::Send {
            text: None,
            flag: u32::MAX,
        },
    ] {
        assert_eq!(
            apply(&mut local, foreign_message, operation),
            Err(PlayerMessageError::ForeignArena(foreign_message))
        );
        assert_eq!(
            apply(&mut local, missing, operation),
            Err(PlayerMessageError::UnknownMessage(missing))
        );
    }
    assert_eq!(format!("{local:?}"), local_before);
    assert_eq!(format!("{foreign:?}"), foreign_before);
}

#[test]
fn world_cleanup_retains_actual_message_and_player_fifo() {
    let mut arena = arena();
    let first = arena.add_world_player([0x1234; 6]);
    let second = arena.add_world_player([0x5678; 6]);
    let first_message = message(&arena, first);
    let second_message = message(&arena, second);
    apply(&mut arena, first_message, PlayerMessageOperation::Enable).unwrap();
    let before = format!("{arena:?}");
    // This same live allocation can be cleaned repeatedly: its NULL branches
    // never delete storage or clear BYTE+0/+1. Neither a new message owner
    // nor the player's destructor is used to obtain the result.
    for recipient in [first_message, second_message, first_message] {
        apply(&mut arena, recipient, PlayerMessageOperation::CleanupWorld).unwrap();
        assert_eq!(format!("{arena:?}"), before);
        assert_eq!(arena.first_player(), Some(first));
    }
    assert_eq!(
        apply(
            &mut arena,
            first_message,
            PlayerMessageOperation::Send {
                text: None,
                flag: 7
            }
        ),
        Err(PlayerMessageError::EnabledMessageBoundary(first_message))
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn explicit_writer_sequence_uses_live_byte_and_failure_does_not_restore_it() {
    let mut arena = arena();
    let first = arena.add_world_player([1; 6]);
    let second = arena.add_world_player([2; 6]);
    let recipient = message(&arena, second);
    let initial = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    assert_eq!(
        apply(&mut arena, recipient, PlayerMessageOperation::Suppress),
        Ok(())
    );
    assert_eq!(
        apply(
            &mut arena,
            recipient,
            PlayerMessageOperation::Send {
                text: None,
                flag: 0
            },
        ),
        Ok(())
    );
    assert_eq!(
        apply(&mut arena, recipient, PlayerMessageOperation::Enable),
        Ok(())
    );
    assert_eq!(
        arena.world_players(),
        expected_byte(&initial, second, 0),
        "enable changes only the chosen message byte"
    );
    let enabled = format!("{arena:?}");
    assert_eq!(
        apply(
            &mut arena,
            recipient,
            PlayerMessageOperation::Send {
                text: None,
                flag: 0
            },
        ),
        Err(PlayerMessageError::EnabledMessageBoundary(recipient))
    );
    assert_eq!(format!("{arena:?}"), enabled);
    assert_eq!(arena.player_message(first).unwrap().suppression_byte(), 1);
    assert_eq!(
        apply(&mut arena, recipient, PlayerMessageOperation::Suppress),
        Ok(())
    );
    assert_eq!(arena.world_players(), initial);
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn captured_second_message_stays_second_across_first_player_and_append_changes() {
    let mut arena = arena();
    let first = arena.add_world_player([1; 6]);
    let second = arena.add_world_player([2; 6]);
    let first_message = message(&arena, first);
    let captured = message(&arena, second);
    apply(&mut arena, first_message, PlayerMessageOperation::Enable).unwrap();
    let third = arena.add_world_player([3; 6]);
    let before = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    apply(&mut arena, captured, PlayerMessageOperation::Enable).unwrap();
    assert_eq!(arena.world_players(), expected_byte(&before, second, 0));
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(arena.player_message(third).unwrap().suppression_byte(), 1);
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn every_nonzero_byte_bypasses_missing_text_raw_flags_and_all_other_state() {
    let mut arena = arena();
    let player = arena.add_world_player([0x55aa_55aa; 6]);
    let recipient = message(&arena, player);
    // Input need not be terminated or fit the future 192-byte caller buffer:
    // the actual suppressed branch does not consume it at all.
    let text = vec![0xfe; 257];
    let text_before = text.clone();
    for byte in [1, 0x80, 0xff] {
        // Declared private raw-state fixture, not a public numeric write API.
        arena
            .world_player_mut(player)
            .unwrap()
            .message_88
            .suppression_01 = byte;
        let before = format!("{arena:?}");
        for text in [None, Some(text.as_slice())] {
            for flag in [0, 0x100, u32::MAX] {
                assert_eq!(
                    apply(
                        &mut arena,
                        recipient,
                        PlayerMessageOperation::Send { text, flag },
                    ),
                    Ok(())
                );
                assert_eq!(format!("{arena:?}"), before);
            }
        }
        assert_eq!(
            arena.player_message(player).unwrap().suppression_byte(),
            byte
        );
    }
    assert_eq!(text, text_before);
}

#[test]
fn enabled_byte_stops_before_missing_text_flag_and_any_enabled_storage() {
    let mut arena = arena();
    let player = arena.add_world_player([u32::MAX; 6]);
    let recipient = message(&arena, player);
    apply(&mut arena, recipient, PlayerMessageOperation::Enable).unwrap();
    let before = format!("{arena:?}");
    for flag in [0, 0x100, u32::MAX] {
        assert_eq!(
            apply(
                &mut arena,
                recipient,
                PlayerMessageOperation::Send { text: None, flag },
            ),
            Err(PlayerMessageError::EnabledMessageBoundary(recipient))
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn real_save_restore_and_completion_reset_preserve_enabled_message_owner() {
    let mut arena = arena();
    let first = arena.add_world_player([0; 6]);
    let second = arena.add_world_player([0; 6]);
    let recipient = message(&arena, second);
    apply(&mut arena, recipient, PlayerMessageOperation::Enable).unwrap();
    {
        let player = arena.world_player_mut(second).unwrap();
        player.byte_ed = 1;
        player.dword_f0 = 123;
    }
    let saved = inf_formats::PlayerSaveState {
        owner_100: [1, 2, 3, 4, 5, 6],
        owner_11c: 7,
        owner_144: 8,
        owner_148: 9,
        owner_14c: 10,
        owner_150: [u32::MAX; 16],
        owner_190: [11, 12, 13],
        owner_a0: [14; 6],
        ..Default::default()
    };
    let owners = other_owners(&arena);
    let first_before = arena.world_player(first).unwrap().clone();
    arena.restore_player_save_prefix(second, &saved).unwrap();
    assert_eq!(message(&arena, second), recipient);
    assert_eq!(arena.player_message(second).unwrap().suppression_byte(), 0);
    assert_eq!(
        arena.world_player(second).unwrap().grants_9c,
        saved.owner_a0
    );
    // Genuine owned 0046AF70 reset; the whole world cleanup is separate.
    arena.clear_player_completion_checkpoint(second).unwrap();
    assert_eq!(arena.world_player(second).unwrap().byte_ed, 0);
    assert_eq!(arena.world_player(second).unwrap().dword_f0, 0);
    assert_eq!(arena.world_player(first).unwrap(), &first_before);
    assert_eq!(message(&arena, second), recipient);
    assert_eq!(arena.player_message(second).unwrap().suppression_byte(), 0);
    assert_eq!(other_owners(&arena), owners);
    let before = format!("{arena:?}");
    assert_eq!(
        apply(
            &mut arena,
            recipient,
            PlayerMessageOperation::Send {
                text: None,
                flag: 0
            },
        ),
        Err(PlayerMessageError::EnabledMessageBoundary(recipient))
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn message_gate_needs_no_registry_or_process_owner() {
    let mut arena = ObjectArena::new();
    let first = arena.add_world_player([1; 6]);
    let second = arena.add_world_player([2; 6]);
    let recipient = message(&arena, second);
    let process = ParticleFactoryProcessState {
        network_flag: 0x100,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    let process_before = process;
    assert!(arena.actor_handles().is_none());
    let before = format!("{arena:?}");
    assert_eq!(
        apply(
            &mut arena,
            recipient,
            PlayerMessageOperation::Send {
                text: None,
                flag: u32::MAX
            },
        ),
        Ok(())
    );
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(process, process_before);
    assert_eq!(arena.first_player(), Some(first));
}
