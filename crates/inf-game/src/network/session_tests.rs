use super::super::transport::{PlayerTableProtocol, SequenceProtocol};
use super::*;

const HOST: u32 = 0x1111_0001;
const GUEST: u32 = 0x2222_0002;

fn started(id: u32, host: bool) -> NetTransport {
    let mut transport = NetTransport::default();
    transport
        .start_session(
            id,
            host,
            [
                &mut PlayerTableProtocol::default(),
                &mut SequenceProtocol::default(),
            ],
        )
        .unwrap();
    transport
}

/// Every frame `from` sends, received by `to` as the receive thread would.
fn deliver(from: &mut NetTransport, to: &mut NetTransport) -> usize {
    let frames = from.take_outgoing();
    for frame in &frames {
        to.receive_message(
            [
                &mut PlayerTableProtocol::default(),
                &mut SequenceProtocol::default(),
            ],
            frame,
        )
        .unwrap();
    }
    frames.len()
}

#[test]
fn a_started_session_heads_its_table_with_its_id() {
    let transport = started(HOST, true);
    assert!(transport.state.network_active);
    assert_eq!(transport.state.players[0], HOST);
    assert_eq!(transport.state.players[1..], [0; 16]);
    assert_eq!(transport.state.buffer_size, 0x400);
    assert_eq!(transport.state.local_slot, 0);
}

/// 004934C0 on the host: a slot from the first free entry, holding its own
/// id; then the table only every twelfth quiet heartbeat.
#[test]
fn the_hosts_heartbeat_takes_a_slot_and_resends_the_table_every_twelfth_beat() {
    let mut host = started(HOST, true);
    let mut table = PlayerTableProtocol::default();
    host.heartbeat(&mut table).unwrap();
    assert_eq!(host.state.local_slot, 1);
    assert_eq!(host.state.players[1], HOST);
    let mut sent = Vec::new();
    for beat in 1..=24 {
        if beat > 1 {
            host.heartbeat(&mut table).unwrap();
        }
        sent.push(!host.take_outgoing().is_empty());
    }
    let sends: Vec<usize> = sent
        .iter()
        .enumerate()
        .filter_map(|(index, &sent)| sent.then_some(index + 1))
        .collect();
    assert_eq!(sends, [12, 24]);
    assert_eq!(host.state.heartbeats, 24);
    assert_eq!(host.state.tables_sent, 2);
}

/// A guest sends its id until it has a slot; the host adds it, sends the
/// table at its next heartbeat, and the guest finds its slot there.
#[test]
fn a_guest_gets_its_slot_through_the_player_table() {
    let mut host = started(HOST, true);
    let mut guest = started(GUEST, false);
    let mut table = PlayerTableProtocol::default();
    host.heartbeat(&mut table).unwrap();
    assert_eq!(host.take_outgoing().len(), 0);
    guest.heartbeat(&mut table).unwrap();
    assert_eq!(deliver(&mut guest, &mut host), 1);
    assert_eq!(host.state.players[2], GUEST);
    assert_eq!(host.state.players_added[0], 1);
    host.heartbeat(&mut table).unwrap();
    assert_eq!(deliver(&mut host, &mut guest), 1);
    assert_eq!(guest.state.local_slot, 2);
    assert_eq!(guest.state.players[..3], [GUEST, HOST, GUEST]);
    // With its slot, the guest goes quiet.
    guest.heartbeat(&mut table).unwrap();
    assert!(guest.take_outgoing().is_empty());
}

#[test]
fn system_messages_clear_a_player_and_set_the_flags() {
    let mut host = started(HOST, true);
    host.state.players[3] = GUEST;
    host.system_message(&SystemMessage::PlayerDestroyed {
        id: GUEST,
        name: "bob".into(),
    });
    assert_eq!(host.state.players[3], 0);
    let mut guest = started(GUEST, false);
    guest.system_message(&SystemMessage::BecameHost);
    assert!(guest.state.host && guest.state.host_changed);
    guest.system_message(&SystemMessage::SessionLost);
    assert!(guest.state.session_lost);
    guest.close_session();
    assert!(!guest.state.network_active);
    assert_eq!((guest.state.local_slot, guest.state.players), (1, [0; 17]));
}

/// Short messages are dropped; channel 1's default protocol does not
/// receive, so its frames wait on the receive ring for 00492830.
#[test]
fn received_frames_route_or_wait_on_the_ring() {
    let mut host = started(HOST, true);
    host.receive_message(
        [
            &mut PlayerTableProtocol::default(),
            &mut SequenceProtocol::default(),
        ],
        &[1, 2, 3],
    )
    .unwrap();
    assert!(host.receive_ring.read_slot().is_none());
    let frame = [8, 0, 1, 2, 0, 0, 0, 0];
    host.receive_message(
        [
            &mut PlayerTableProtocol::default(),
            &mut SequenceProtocol::default(),
        ],
        &frame,
    )
    .unwrap();
    assert!(host.receive_ring.read_slot().is_some());
    host.drain_received([
        &mut PlayerTableProtocol::default(),
        &mut SequenceProtocol::default(),
    ])
    .unwrap();
    assert!(host.receive_ring.read_slot().is_none());
}
