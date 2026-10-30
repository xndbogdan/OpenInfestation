use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, WorldPlayer,
};
use serde_json::Value;

static SINE: [u16; 4096] = [0; 4096];

/// A registry arena whose player drives a live unit at +D4, as both
/// recordings' players did.
fn driving_player() -> (ObjectArena, PlayerId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let player = arena.add_world_player([0; 6]);
    let vehicle = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    let handle = arena.actor(vehicle).unwrap().handle;
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    (arena, player)
}

/// Player +28..+57, the message byte, +F4 and +FC: the fixture's state.
fn snapshot(arena: &ObjectArena, player: PlayerId) -> Vec<u64> {
    let state = arena.world_player(player).unwrap();
    let controls = &state.controls;
    let bytes = |index: usize| {
        u64::from(u32::from_le_bytes(
            controls.bytes_38[index * 4..index * 4 + 4]
                .try_into()
                .unwrap(),
        ))
    };
    let known = |value: Option<u32>| value.map_or(u64::MAX, u64::from);
    vec![
        controls.dword_28.into(),
        controls.dword_2c.into(),
        controls.dword_30.into(),
        controls.dword_34.into(),
        bytes(0),
        bytes(1),
        bytes(2),
        bytes(3),
        known(state.dword_48),
        known(state.dword_4c),
        known(state.dword_50),
        known(state.dword_54.map(|value| value as u32)),
        state.message_byte_00().into(),
        u64::from(state.dword_f4 as u32),
        state.byte_fc.into(),
    ]
}

fn restore(arena: &mut ObjectArena, player: PlayerId, words: &[u64]) {
    let state = arena.world_player_mut(player).unwrap();
    let word = |index: usize| words[index] as u32;
    let controls = &mut state.controls;
    controls.dword_28 = word(0);
    controls.dword_2c = word(1);
    controls.dword_30 = word(2);
    controls.dword_34 = word(3);
    for index in 0..4 {
        controls.bytes_38[index * 4..index * 4 + 4].copy_from_slice(&word(4 + index).to_le_bytes());
    }
    state.dword_48 = Some(word(8));
    state.dword_4c = Some(word(9));
    state.dword_50 = Some(word(10));
    state.dword_54 = Some(word(11) as i32);
    state.set_message_byte_00(word(12) as u8);
    state.dword_f4 = word(13) as i32;
    state.byte_fc = word(14) as u8;
}

/// Every key callback both recordings made (752), and every repeat of the
/// player update's 0046D240 with a key held (740), each from its captured
/// entry state. A call whose port path is complete matches exactly, with or
/// without a shown message; only an omitted call may differ.
#[test]
fn key_callbacks_match_both_recordings() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/player_keys_native.json"
    ))
    .unwrap();
    let calls = fixture["calls"].as_array().unwrap();
    let (mut omitted, mut failures) = (Vec::new(), Vec::new());
    for (index, call) in calls.iter().enumerate() {
        let words: Vec<u64> = call
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap())
            .collect();
        let (address, value) = (words[0] as u32, words[3] as u32);
        let (before, after) = (&words[6..21], &words[21..36]);
        let (mut arena, player) = driving_player();
        restore(&mut arena, player, before);
        let mut omissions = BTreeSet::new();
        let result = if address == 0x0046_d240 {
            arena.player_message_keys(player, &mut omissions)
        } else {
            arena
                .player_key_callback(
                    player,
                    PlayerKeyEvent {
                        address,
                        value,
                        timestamp_micros: words[5] << 32 | words[4],
                    },
                    PlayerKeyInputs {
                        network: 0,
                        double_tap_window: 350,
                        cheat_word_84: 0,
                    },
                )
                .map(|found| omissions = found)
        };
        if let Err(error) = result {
            failures.push(format!("call {index} {address:08X}: {error}"));
            continue;
        }
        let port = snapshot(&arena, player);
        if port != after {
            let line = format!(
                "call {index} {address:08X} edx {value}: port {port:x?} native {after:x?} omitted {omissions:?}"
            );
            if omissions.is_empty() {
                failures.push(line);
            } else {
                omitted.push(line);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} key calls differ:\n{}",
        failures.len(),
        calls.len(),
        failures.join("\n")
    );
    // Infest01's one Tab press showed the message display (00413410) and
    // held Fire off; the port skips that callback whole.
    assert_eq!(omitted.len(), 1, "{}", omitted.join("\n"));
    assert!(omitted[0].contains("00469450 edx 1"), "{}", omitted[0]);
}

fn event(address: u32, value: u32) -> PlayerKeyEvent {
    PlayerKeyEvent {
        address,
        value,
        timestamp_micros: 0,
    }
}

const INPUTS: PlayerKeyInputs = PlayerKeyInputs {
    network: 0,
    double_tap_window: 350,
    cheat_word_84: 0,
};

/// The control-block stores the recordings never reached, by address.
#[test]
fn presses_store_their_requests_and_releases_do_nothing() {
    type Read = fn(&WorldPlayer) -> u32;
    let cases: [(u32, Read, u32); 24] = [
        (0x0046_9580, |p| p.controls.dword_34, u32::MAX),
        (0x0046_95e0, |p| p.controls.dword_34, 3),
        (0x0046_9600, |p| p.controls.dword_34, 4),
        (0x0046_9620, |p| p.controls.dword_34, 5),
        (0x0046_9640, |p| p.controls.dword_34, 6),
        (0x0046_9670, |p| p.controls.byte(0x3c).into(), 1),
        (0x0046_9680, |p| p.controls.dword_30, 1),
        (0x0046_96a0, |p| p.controls.dword_30, u32::MAX),
        (0x0046_96c0, |p| p.controls.dword_28, 1),
        (0x0046_96e0, |p| p.controls.dword_28, u32::MAX),
        (0x0046_9700, |p| p.controls.dword_2c, 1),
        (0x0046_9720, |p| p.controls.dword_2c, u32::MAX),
        (0x0046_9740, |p| p.controls.byte(0x3d).into(), 1),
        (0x0046_9750, |p| p.controls.byte(0x3f).into(), 1),
        (0x0046_9800, |p| p.controls.dword_28, 2),
        (0x0046_9820, |p| p.controls.dword_28, 3),
        (0x0046_9840, |p| p.controls.dword_28, 4),
        (0x0046_9860, |p| p.controls.dword_28, 5),
        (0x0046_9880, |p| p.controls.dword_28, 6),
        (0x0046_98a0, |p| p.controls.dword_28, 7),
        (0x0046_98c0, |p| p.controls.dword_28, 8),
        (0x0046_98e0, |p| p.controls.byte(0x42).into(), 1),
        (0x0046_9660, |p| p.controls.byte(0x3b).into(), 1),
        (0x0046_95a0, |p| p.controls.dword_34, 1),
    ];
    for (address, read, stored) in cases {
        let (mut arena, player) = driving_player();
        arena
            .player_key_callback(player, event(address, 0), INPUTS)
            .unwrap();
        assert_eq!(
            read(arena.world_player(player).unwrap()),
            0,
            "{address:08X} release"
        );
        let omissions = arena
            .player_key_callback(player, event(address, 1), INPUTS)
            .unwrap();
        assert!(omissions.is_empty());
        assert_eq!(
            read(arena.world_player(player).unwrap()),
            stored,
            "{address:08X}"
        );
    }
}

#[test]
fn level_keys_follow_the_key_both_ways() {
    let (mut arena, player) = driving_player();
    for (address, offset, down) in [
        (0x0046_94b0, 0x38, 2),
        (0x0046_9760, 0x40, 1),
        (0x0046_9770, 0x41, 1),
    ] {
        arena
            .player_key_callback(player, event(address, 7), INPUTS)
            .unwrap();
        assert_eq!(
            arena.world_player(player).unwrap().controls.byte(offset),
            down
        );
        arena
            .player_key_callback(player, event(address, 0), INPUTS)
            .unwrap();
        assert_eq!(arena.world_player(player).unwrap().controls.byte(offset), 0);
    }
}

/// 004694D0 with the constructor's -1000/0: press, release, and a second
/// press and release inside the 350-tick window set +39.
#[test]
fn a_grenade_double_tap_sets_39() {
    let (mut arena, player) = driving_player();
    {
        let state = arena.world_player_mut(player).unwrap();
        state.dword_48 = Some((-1000_i32) as u32);
        state.dword_4c = Some(0);
    }
    let tap = |arena: &mut ObjectArena, value, ticks: u64| {
        arena
            .player_key_callback(
                player,
                PlayerKeyEvent {
                    address: 0x0046_94d0,
                    value,
                    timestamp_micros: ticks << 10,
                },
                INPUTS,
            )
            .unwrap();
        let state = arena.world_player(player).unwrap();
        (state.dword_48.unwrap() as i32, state.dword_4c.unwrap())
    };
    assert_eq!(tap(&mut arena, 1, 5000), (5000, 1));
    assert_eq!(tap(&mut arena, 0, 5100), (5000, 0));
    assert_eq!(tap(&mut arena, 1, 5200), (5000, 2));
    assert_eq!(arena.world_player(player).unwrap().controls.byte(0x39), 0);
    assert_eq!(tap(&mut arena, 0, 5349), (5349 - 1_000_000, 0));
    assert_eq!(arena.world_player(player).unwrap().controls.byte(0x39), 1);

    // A release at the window's edge is too late.
    arena
        .world_player_mut(player)
        .unwrap()
        .controls
        .set_byte(0x39, 0);
    assert_eq!(tap(&mut arena, 1, 2_000_000), (2_000_000, 1));
    assert_eq!(tap(&mut arena, 0, 2_000_100), (2_000_000, 0));
    assert_eq!(tap(&mut arena, 1, 2_000_200), (2_000_000, 2));
    assert_eq!(tap(&mut arena, 0, 2_000_350), (2_000_350 - 1_000_000, 0));
    assert_eq!(arena.world_player(player).unwrap().controls.byte(0x39), 0);
}

/// Offline, Tab without the message environment is skipped whole and Escape
/// queues its 0x18 broadcast from the unit; online both do nothing.
#[test]
fn tab_and_escape_act_offline_only() {
    let (mut arena, player) = driving_player();
    let omissions = arena
        .player_key_callback(player, event(0x0046_9450, 1), INPUTS)
        .unwrap();
    assert_eq!(
        omissions.into_iter().collect::<Vec<_>>(),
        [PlayerKeyOmission::MessageToggle]
    );
    assert!(arena.take_unit_broadcasts().is_empty());
    let omissions = arena
        .player_key_callback(player, event(0x0046_9480, 1), INPUTS)
        .unwrap();
    assert!(omissions.is_empty());
    let unit = arena.player_actor(player).unwrap().unwrap();
    assert_eq!(
        arena.take_unit_broadcasts(),
        [crate::object_runtime::UnitBroadcast {
            player,
            unit,
            code: crate::object_runtime::UnitBroadcastCode::Escape,
        }]
    );
    let online = PlayerKeyInputs {
        network: 1,
        ..INPUTS
    };
    for address in [0x0046_9450, 0x0046_9480] {
        assert!(arena
            .player_key_callback(player, event(address, 1), online)
            .unwrap()
            .is_empty());
        assert!(arena
            .player_key_callback(player, event(address, 0), INPUTS)
            .unwrap()
            .is_empty());
    }
    assert!(arena.take_unit_broadcasts().is_empty());
    // Without a unit to broadcast from, Escape does nothing.
    let mut bare = driving_player();
    bare.0.world_player_mut(bare.1).unwrap().handle_d4 = ActorHandle::from_raw(0);
    assert!(bare
        .0
        .player_key_callback(bare.1, event(0x0046_9480, 1), INPUTS)
        .unwrap()
        .is_empty());
    assert!(bare.0.take_unit_broadcasts().is_empty());
}

/// A held cursor key does nothing without a message beyond its +50 bit and
/// the 250 ms repeat; Enter's select is the omitted call.
#[test]
fn message_keys_hold_bits_and_select_is_omitted() {
    let (mut arena, player) = driving_player();
    arena.world_player_mut(player).unwrap().dword_50 = Some(0);
    let omissions = arena
        .player_key_callback(player, event(0x0046_9780, 1), INPUTS)
        .unwrap();
    assert!(omissions.is_empty());
    let state = arena.world_player(player).unwrap();
    assert_eq!(
        (state.dword_50, state.dword_54, state.dword_f4),
        (Some(2), Some(250), 0)
    );
    // A second press of a held key leaves the repeat alone.
    arena.world_player_mut(player).unwrap().dword_54 = Some(40);
    arena
        .player_key_callback(player, event(0x0046_9780, 1), INPUTS)
        .unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_54, Some(40));
    let omissions = arena
        .player_key_callback(player, event(0x0046_97f0, 1), INPUTS)
        .unwrap();
    assert_eq!(
        omissions.into_iter().collect::<Vec<_>>(),
        [PlayerKeyOmission::MessageSelect]
    );
    assert_eq!(arena.world_player(player).unwrap().dword_50, Some(2));
    arena
        .player_key_callback(player, event(0x0046_9780, 0), INPUTS)
        .unwrap();
    assert_eq!(arena.world_player(player).unwrap().dword_50, Some(0));
    assert!(matches!(
        arena.player_key_callback(player, event(0x0046_9790, 1), INPUTS),
        Err(PlayerKeyError::NotPlayerKey(0x0046_9790))
    ));
}
