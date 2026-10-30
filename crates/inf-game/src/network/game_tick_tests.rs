use std::collections::BTreeMap;

use super::*;
use crate::network::game::GameHeaderHost;
use crate::network::protocol::GAME_CHANNEL;
use crate::network::transport::{NetProtocol, PlayerTableProtocol};

/// A world of objects by handle: each with its send timer and deadline.
#[derive(Default)]
struct World {
    settings: BTreeMap<usize, u32>,
    mission: (i32, u8),
    slots: BTreeMap<u8, Vec<u32>>,
    timers: BTreeMap<u32, ActorSendTimer>,
    deadlines: BTreeMap<u32, i32>,
    clock: u32,
    random: u32,
    calls: Vec<String>,
    received: Vec<Vec<u8>>,
}

impl GameMessageHost for World {
    type Error = String;
    fn call(&mut self, call: GameCall, _: &mut GameNetState) -> Result<u32, String> {
        self.calls.push(format!("{call:?}"));
        Ok(match call {
            GameCall::Resolve { handle } => u32::from(self.timers.contains_key(&handle)) * handle,
            _ => 0,
        })
    }
    fn flags(&mut self, _: u32) -> Result<u32, String> {
        Ok(0)
    }
    fn word_70(&mut self, _: u32) -> Result<u32, String> {
        Ok(0)
    }
    fn handle_84(&mut self, object: u32) -> Result<u32, String> {
        Ok(object)
    }
}

impl GameHeaderHost for World {
    type Error = String;
    fn settings_word(&mut self, offset: usize) -> u32 {
        self.settings.get(&offset).copied().unwrap_or(0)
    }
    fn set_settings_word(&mut self, offset: usize, value: u32) {
        self.settings.insert(offset, value);
    }
    fn mission(&self) -> (i32, u8) {
        self.mission
    }
    fn rebuild_terrain(&mut self, _: u32) -> Result<bool, String> {
        Ok(false)
    }
}

impl GameWorldHost for World {
    fn null_handle(&self) -> u32 {
        0
    }
    fn encode_actor(&mut self, handle: u32, out: &mut [u8]) -> Result<i32, String> {
        out[..4].copy_from_slice(&handle.to_le_bytes());
        out[4..8].copy_from_slice(&[0; 4]);
        Ok(8)
    }
    fn receive_actor(
        &mut self,
        payload: &[u8],
        _: &mut Vec<(u32, GameSend)>,
    ) -> Result<i32, String> {
        self.received.push(payload[..8].to_vec());
        Ok(8)
    }
    fn world_values(&mut self) -> Result<[u32; 4], String> {
        Ok([1, 2, 3, 4])
    }
    fn territory(&mut self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
    fn encode_race(&mut self, _: &mut [u8]) -> Result<i32, String> {
        Ok(0)
    }
    fn receive_race(&mut self, _: &[u8]) -> Result<i32, String> {
        Err("no race".into())
    }
    fn receive_ball(&mut self, _: u32, _: &[u8]) -> Result<(), String> {
        Err("no ball".into())
    }
    fn receive_territory(
        &mut self,
        _: u32,
        _: &[u8],
        _: &mut crate::network::game::GameNetState,
        _: &mut Vec<(u32, GameSend)>,
    ) -> Result<(), String> {
        Ok(())
    }
}

impl GameFrameHost for World {
    fn slot_handles(&mut self, slot: u8) -> Result<Option<Vec<u32>>, String> {
        Ok((slot < 17).then(|| self.slots.get(&slot).cloned().unwrap_or_default()))
    }
    fn send_timer(&mut self, handle: u32) -> Result<ActorSendTimer, String> {
        Ok(self.timers[&handle])
    }
    fn set_send_timer(&mut self, handle: u32, countdown: u32, count: u32) -> Result<(), String> {
        let timer = self.timers.get_mut(&handle).unwrap();
        timer.countdown_dc = countdown;
        timer.count_e0 = count;
        Ok(())
    }
    fn deadline(&mut self, handle: u32) -> Result<i32, String> {
        Ok(self.deadlines.get(&handle).copied().unwrap_or(i32::MAX))
    }
    fn expire(&mut self, handle: u32) -> Result<bool, String> {
        self.calls.push(format!("expire {handle:#x}"));
        Ok(true)
    }
    fn race_actor(&mut self) -> bool {
        false
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(self.random)
    }
    fn clock(&self) -> u32 {
        self.clock
    }
    fn identity(&mut self) -> Result<([u8; 16], [u8; 6]), String> {
        let mut name = [0; 16];
        name[..4].copy_from_slice(b"Anna");
        Ok((name, *b"AN\0\0\0\0"))
    }
}

fn transport(slot: u8, host: bool) -> NetTransport {
    let mut transport = NetTransport::default();
    let mut table = PlayerTableProtocol::default();
    let mut game = GameNetState::default();
    let mut world = World::default();
    let mut protocol = GameProtocol::new(&mut game, &mut world);
    transport
        .start_session(0x100 + u32::from(slot), host, [&mut table, &mut protocol])
        .unwrap();
    transport.state.local_slot = slot;
    transport
}

/// The frames a transport committed on the game channel.
fn sent(transport: &mut NetTransport) -> Vec<Vec<u8>> {
    transport
        .take_outgoing()
        .into_iter()
        .filter(|frame| usize::from(frame[2]) == GAME_CHANNEL)
        .collect()
}

/// The message ids of a game frame, after its 4-byte prefix and header.
fn ids(frame: &[u8]) -> Vec<u8> {
    let mut at = 20;
    let mut ids = Vec::new();
    while at < frame.len() {
        let id = frame[at] & 0x7f;
        let size = usize::from(u16::from_le_bytes([frame[at + 1], frame[at + 2]]));
        ids.push(id);
        at += 3 + size;
    }
    ids
}

#[test]
fn the_header_takes_the_sequence_id_and_slot() {
    let mut transport = transport(3, true);
    let mut game = GameNetState::with_slot_records();
    game.header_out = [0x05, 0, 7, 0x0019];
    game.publish_header(&transport.state);
    assert_eq!(game.header_out, [0x05, 0x103, 8, 0x0303_0019]);
    assert_eq!(game.states[3], 7);
    transport.state.host = false;
    game.header_out[0] = 0x1f;
    game.publish_header(&transport.state);
    assert_eq!(game.header_out[3], 0x0003_0019);
    assert_eq!(game.states[3], 1);
    // States from 0x12 read as 1.
    for (status, state) in [(0x11, 0x13), (0x12, 1)] {
        game.header_out[0] = status;
        game.publish_header(&transport.state);
        assert_eq!(game.states[3], state);
    }
    assert_eq!(game.records[0].sequence, 0);
    assert_eq!(game.records[5].sequence, 1);
}

#[test]
fn a_reset_names_every_slot_and_loads_the_lines_once() {
    let mut game = GameNetState::with_slot_records();
    game.kill_word = 9;
    game.ended = 1;
    game.present[4] = 1;
    game.teams[3] = Some(TeamScore {
        index: 3,
        score: 5,
        ..TeamScore::default()
    });
    let mut frame = GameFrameState::default();
    frame.lines.lines[2].timer = 77;
    let loads = std::cell::Cell::new(0);
    let load = |index: usize| {
        loads.set(loads.get() + 1);
        vec![format!("line {index}").into_bytes()]
    };
    game.reset_game(4, &mut frame);
    frame.load_line_groups(load);
    assert_eq!(&game.names[16][..10], b"Player 16\0");
    assert_eq!((game.kill_word, game.ended, game.present[4]), (0, 0, 0));
    assert_eq!(game.team_game, 1);
    assert_eq!(game.teams[3].unwrap().score, 0);
    assert_eq!(game.teams[3].unwrap().index, 3);
    assert_eq!(frame.host_lines, vec![b"line 4".to_vec()]);
    assert_eq!(frame.lines.lines[2].timer, 0);
    game.reset_game(2, &mut frame);
    frame.load_line_groups(load);
    assert_eq!(loads.get(), 4);
    assert_eq!(game.team_game, 0);
}

#[test]
fn a_changed_player_is_cleared_and_its_objects_disposed() {
    let transport = {
        let mut transport = transport(1, true);
        transport.state.players[1] = 0x101;
        transport.state.players[2] = 0x202;
        transport.state.players[3] = 0x303;
        transport
    };
    let mut game = GameNetState::with_slot_records();
    game.session_words[2] = 0x999;
    game.scores[2] = [1, 2, 3];
    game.kill_words[2] = 0x1234_5678;
    game.records[2].sequence = 40;
    let mut world = World::default();
    world.settings.insert(0x30, 5000);
    world.slots.insert(2, vec![0x1001_0005, 0x1002_0006]);
    let mut protocol = GameProtocol::new(&mut game, &mut world);
    assert!(protocol.receive_slots(&transport.state).unwrap());
    assert!(!protocol.receive_slots(&transport.state).unwrap());
    assert_eq!(&game.names[1][..5], b"Anna\0");
    assert_eq!(&game.details[1][..3], b"AN\0");
    assert_eq!(game.score_limit, 5000);
    assert_eq!(game.scores[2], [0; 3]);
    assert_eq!(game.kill_words[2], 0x1234_0000);
    assert_eq!(game.records[2].sequence, 0);
    assert_eq!((game.present[2], game.heard[2]), (0, 1));
    // Slot 3 had no player before: cleared, but nothing disposed.
    assert_eq!(game.heard[3], 1);
    assert_eq!(game.session_words[2..4], [0x202, 0x303]);
    assert_eq!(world.calls.len(), 2);
    assert!(world.calls[0].contains("Dispose { object: 268500997 }"));
}

#[test]
fn sends_wait_for_a_tenth_of_a_second() {
    let mut transport = transport(1, true);
    let mut game = GameNetState::with_slot_records();
    let mut world = World::default();
    let mut frame = GameFrameState::default();
    let mut protocol = GameProtocol::new(&mut game, &mut world);
    protocol
        .send_frame(&mut transport, &mut frame, 60_000)
        .unwrap();
    protocol
        .send_frame(&mut transport, &mut frame, 40_000)
        .unwrap();
    assert!(sent(&mut transport).is_empty());
    assert_eq!(frame.gathered, 100_000);
    protocol.send_frame(&mut transport, &mut frame, 1).unwrap();
    assert_eq!(frame.gathered, 0);
    let frames = sent(&mut transport);
    assert_eq!(frames.len(), 1);
    assert_eq!(ids(&frames[0]), [28, 36, 38]);
}

#[test]
fn the_host_sends_a_game_over_line_each_150_ticks() {
    let mut transport = transport(1, true);
    let mut game = GameNetState::with_slot_records();
    game.ended = 1;
    game.winner = 2;
    game.present[2] = 1;
    game.names[2][..3].copy_from_slice(b"Bob");
    let mut world = World {
        clock: 300,
        random: 0x0001_0003,
        ..World::default()
    };
    let mut frame = GameFrameState {
        host_lines: vec![
            b"unused".to_vec(),
            b"@1 wins".to_vec(),
            b"@1 rules".to_vec(),
        ],
        ..GameFrameState::default()
    };
    let mut protocol = GameProtocol::new(&mut game, &mut world);
    protocol
        .send_frame(&mut transport, &mut frame, 200_000)
        .unwrap();
    // 3 % 2 picks the second line after the first.
    assert_eq!(&frame.lines.lines[0].text[..8], b"Bob rule");
    assert_eq!(frame.line_step, 2);
    protocol
        .send_frame(&mut transport, &mut frame, 200_000)
        .unwrap();
    let frames = sent(&mut transport);
    assert_eq!(ids(&frames[0]), [31, 28, 36, 38]);
    assert_eq!(ids(&frames[1]), [28, 36, 38]);
}

#[test]
fn a_guest_sends_its_kill_word_names_and_due_records() {
    let mut transport = transport(2, false);
    let mut game = GameNetState::with_slot_records();
    game.kill_word = 3;
    game.heard[5] = 1;
    game.heard[6] = 1;
    game.present[6] = 1;
    let mut world = World {
        random: 0x8000,
        ..World::default()
    };
    world
        .slots
        .insert(2, vec![0x1001_0007, 0x1002_0008, 0x1003_0009]);
    world.timers.insert(
        0x1001_0007,
        ActorSendTimer {
            period_d8: 1000,
            countdown_dc: 50,
            count_e0: 4,
            held: false,
        },
    );
    world.timers.insert(
        0x1002_0008,
        ActorSendTimer {
            countdown_dc: 500_000,
            ..ActorSendTimer::default()
        },
    );
    world.timers.insert(
        0x1003_0009,
        ActorSendTimer {
            held: true,
            ..ActorSendTimer::default()
        },
    );
    world.slots.insert(1, vec![0x0801_0001]);
    world.deadlines.insert(0x0801_0001, 5);
    world.clock = 10;
    let mut frame = GameFrameState::default();
    let mut protocol = GameProtocol::new(&mut game, &mut world);
    protocol
        .send_frame(&mut transport, &mut frame, 120_000)
        .unwrap();
    let frames = sent(&mut transport);
    assert_eq!(ids(&frames[0]), [26, 29, 1, 7]);
    // The countdown restarts at the period plus a jitter of
    // (0x8000 * (120000 >> 2)) >> 13.
    let timer = world.timers[&0x1001_0007];
    assert_eq!((timer.countdown_dc, timer.count_e0), (1000 + 120_000, 5));
    assert_eq!(world.timers[&0x1002_0008].countdown_dc, 500_000 - 120_000);
    assert_eq!(world.timers[&0x1003_0009].count_e0, 0);
    assert_eq!(frame.check_slot, 1);
    assert_eq!(world.calls, ["expire 0x8010001"]);
    assert_eq!((game.heard[5], game.heard[6]), (0, 0));
}

#[test]
fn a_frame_reaches_the_other_machine_in_the_same_world() {
    let mut host = transport(1, true);
    let mut guest = transport(2, false);
    let mut host_game = GameNetState::with_slot_records();
    host_game.header_out[3] = 25;
    host_game.score_limit = 7000;
    let mut host_world = World::default();
    host_world.slots.insert(1, vec![0x0801_0003]);
    host_world
        .timers
        .insert(0x0801_0003, ActorSendTimer::default());
    let mut frame = GameFrameState::default();
    {
        let mut protocol = GameProtocol::new(&mut host_game, &mut host_world);
        protocol.state.publish_header(&host.state);
        protocol.send_frame(&mut host, &mut frame, 200_000).unwrap();
    }
    let mut guest_game = GameNetState::with_slot_records();
    guest_game.session_words[1] = 0x101;
    let mut guest_world = World {
        mission: (25, 1),
        ..World::default()
    };
    let mut table = PlayerTableProtocol::default();
    for frame in sent(&mut host) {
        guest
            .receive_message(
                [
                    &mut table,
                    &mut GameProtocol::new(&mut guest_game, &mut guest_world),
                ],
                &frame,
            )
            .unwrap();
    }
    let mut protocol = GameProtocol::new(&mut guest_game, &mut guest_world);
    protocol.drain(&mut guest, &mut table).unwrap();
    assert_eq!(guest_game.score_limit, 7000);
    assert_eq!(guest_game.host_world, 25);
    assert_eq!(guest_game.records[1].sequence, 1);
    assert_eq!(guest_world.received, [vec![3, 0, 1, 8, 0, 0, 0, 0]]);
    assert!(guest_world
        .calls
        .iter()
        .any(|call| call.contains("WorldWords")));
}

#[test]
fn a_protocol_receives_nothing_on_the_receive_thread() {
    let mut game = GameNetState::default();
    let mut world = World::default();
    let protocol = GameProtocol::new(&mut game, &mut world);
    assert!(!protocol.receives());
    assert_eq!(protocol.message_count(), 45);
}

/// `0045CE60`'s reading: past leading white space, cut at CR or LF, blank
/// lines and `#` comments skipped, `fgets` chunks of 255 bytes, CR LF read as
/// LF, `0x1A` the end, up to the limit. `0045CCE0` falls back on one default:
/// the H0 text for kinds 0 to 2 ("null" without it) and the empty string for
/// 4; a file in the game's folder wins.
#[test]
fn the_line_files_read_as_0045ce60_and_fall_back_as_0045cce0() {
    use super::{parse_line_file, LineGroupSource};
    let text = b"# comment\r\n\r\n   @2 got @1\r\n\t@1 fell\n#x\n";
    assert_eq!(
        parse_line_file(text, 10),
        vec![b"@2 got @1".to_vec(), b"@1 fell".to_vec()]
    );
    let long = [vec![b'a'; 300], b"\n".to_vec()].concat();
    let lines = parse_line_file(&long, 10);
    assert_eq!((lines.len(), lines[0].len(), lines[1].len()), (2, 255, 45));
    assert_eq!(parse_line_file(b"one\x1atwo\n", 10), vec![b"one".to_vec()]);
    assert_eq!(parse_line_file(b"a\nb\nc\n", 2).len(), 2);

    let folder = std::env::temp_dir().join(format!("inf-lines-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let source = LineGroupSource {
        folder: folder.clone(),
        defaults: [
            Some(b"@2 Killed @1".to_vec()),
            None,
            Some(b"@1 Died stupidly".to_vec()),
        ],
    };
    assert_eq!(source.load(0), vec![b"@2 Killed @1".to_vec()]);
    assert_eq!(source.load(1), vec![b"null".to_vec()]);
    assert_eq!(source.load(4), vec![Vec::<u8>::new()]);
    std::fs::write(folder.join("kill.txt"), b"@2 got @1\r\n# not this\r\n").unwrap();
    assert_eq!(source.load(0), vec![b"@2 got @1".to_vec()]);
    std::fs::remove_dir_all(&folder).unwrap();
}
