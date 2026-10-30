use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use super::super::transport::SequenceProtocol;
use super::*;

/// Two machines over loopback: a search finds the host, the guest joins
/// while the host's frame runs, and the heartbeats and player tables give
/// each a slot, the host 1 and the guest 2.
#[test]
fn a_hosted_session_is_found_joined_and_slots_both_players() {
    let mut host = NetworkLink::default();
    let mut host_game = SequenceProtocol::default();
    host.host(Instant::now()).unwrap();
    let port = host.session().unwrap().port().unwrap();
    let mut guest = NetworkLink::default();
    guest.initialize_connection(SearchTarget {
        broadcast: false,
        addresses: vec![SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))],
    });
    let mut clock = 0u64;
    let mut found = Vec::new();
    for _ in 0..500 {
        host.pump(clock, Instant::now(), &mut host_game).unwrap();
        found = guest.enumerate_sessions(Instant::now()).unwrap();
        if !found.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, computer_name());
    let info = found.remove(0);
    let joining = std::thread::spawn(move || guest.join(&info).map(|()| guest));
    while !joining.is_finished() {
        host.pump(clock, Instant::now(), &mut host_game).unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut guest = joining.join().unwrap().unwrap();
    let mut guest_game = SequenceProtocol::default();
    for _ in 0..40 {
        clock += HEARTBEAT_MICROS;
        for _ in 0..5 {
            host.pump(clock, Instant::now(), &mut host_game).unwrap();
            guest.pump(clock, Instant::now(), &mut guest_game).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        if guest.transport.state.local_slot != 0 && host.transport.state.local_slot != 0 {
            break;
        }
    }
    assert_eq!(host.transport.state.local_slot, 1);
    assert_eq!(guest.transport.state.local_slot, 2);
    let guest_id = guest.session().unwrap().local_id();
    assert_eq!(host.transport.state.players[2], guest_id);
    guest.close();
    assert!(!guest.transport.state.network_active);
    host.close();
}
