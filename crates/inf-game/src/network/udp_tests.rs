use super::*;

fn loopback(port: u16) -> SearchTarget {
    SearchTarget {
        broadcast: false,
        addresses: vec![SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))],
    }
}

/// Polls `step` until it answers, for loopback datagrams in flight.
fn settle<T>(mut step: impl FnMut() -> Option<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(value) = step() {
            return value;
        }
        assert!(start.elapsed() < Duration::from_secs(3), "nothing arrived");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn next(session: &mut NetSession) -> Received {
    settle(|| session.receive(Instant::now()))
}

/// `join` waits for the host's answer, so the host polls meanwhile.
fn join(
    host: &mut NetSession,
    info: &SessionInfo,
    name: &str,
) -> Result<NetSession, NetSessionError> {
    let (info, name) = (info.clone(), name.to_string());
    let joining = std::thread::spawn(move || NetSession::join(&info, &name));
    while !joining.is_finished() {
        host.poll(Instant::now());
        std::thread::sleep(Duration::from_millis(1));
    }
    host.poll(Instant::now());
    joining.join().unwrap()
}

fn hosted() -> (NetSession, SessionInfo) {
    let host = NetSession::host("HOSTBOX", "alice", 0, Instant::now()).unwrap();
    let mut search = SessionSearch::new(loopback(host.port().unwrap())).unwrap();
    let mut host = host;
    let info = settle(|| {
        host.poll(Instant::now());
        search.refresh(Instant::now()).into_iter().next()
    });
    (host, info)
}

#[test]
fn every_packet_round_trips() {
    let roster = Roster {
        host: 7,
        entries: vec![
            RosterEntry {
                id: 7,
                name: "host".into(),
                address: None,
            },
            RosterEntry {
                id: 9,
                name: "guest".into(),
                address: Some("10.0.0.2:5000".parse().unwrap()),
            },
        ],
    };
    for packet in [
        Packet::Query {
            application: APPLICATION,
            nonce: 3,
        },
        Packet::Session {
            nonce: 3,
            instance: 0x1122_3344_5566_7788,
            name: "HOSTBOX".into(),
            players: 2,
            max_players: 16,
        },
        Packet::Join {
            instance: 5,
            nonce: 6,
            name: "bob".into(),
        },
        Packet::Welcome {
            instance: 5,
            nonce: 6,
            local: 9,
            roster: roster.clone(),
        },
        Packet::Refused {
            nonce: 6,
            reason: 1,
        },
        Packet::Roster {
            instance: 5,
            roster,
        },
        Packet::Data {
            instance: 5,
            from: 9,
            data: vec![1, 2, 3, 4, 5],
        },
        Packet::Ping {
            instance: 5,
            from: 9,
        },
        Packet::Leave {
            instance: 5,
            from: 9,
        },
    ] {
        assert_eq!(Packet::decode(&packet.encode()), Some(packet));
    }
    assert_eq!(Packet::decode(b"XXXX\x01\x08"), None);
    let mut truncated = Packet::Ping {
        instance: 5,
        from: 9,
    }
    .encode();
    truncated.pop();
    assert_eq!(Packet::decode(&truncated), None);
}

#[test]
fn an_address_defaults_to_the_port_and_blank_searches_the_network() {
    assert_eq!(
        SearchTarget::parse("  ").unwrap(),
        SearchTarget {
            broadcast: true,
            addresses: vec![]
        }
    );
    assert_eq!(
        SearchTarget::parse("127.0.0.1").unwrap().addresses,
        vec!["127.0.0.1:47630".parse::<SocketAddr>().unwrap()]
    );
    assert_eq!(
        SearchTarget::parse("127.0.0.1:2350").unwrap().addresses,
        vec!["127.0.0.1:2350".parse::<SocketAddr>().unwrap()]
    );
    assert!(SearchTarget::parse("no such host.invalid").is_err());
}

/// A search finds the host; a join creates the player on the host, and both
/// directions carry game messages from the sender's id.
#[test]
fn a_searched_session_takes_a_player_and_carries_messages() {
    let (mut host, info) = hosted();
    assert_eq!(info.name, "HOSTBOX");
    assert_eq!((info.players, info.max_players), (1, 16));
    let mut guest = join(&mut host, &info, "bob").unwrap();
    assert!(host.is_host() && !guest.is_host());
    assert_eq!(
        next(&mut host),
        Received::System(SystemMessage::PlayerCreated {
            id: guest.local_id(),
            name: "bob".into()
        })
    );
    guest.send_all(b"from bob");
    assert_eq!(
        next(&mut host),
        Received::Game {
            from: guest.local_id(),
            data: b"from bob".to_vec()
        }
    );
    host.send_all(b"from alice");
    assert_eq!(
        next(&mut guest),
        Received::Game {
            from: host.local_id(),
            data: b"from alice".to_vec()
        }
    );
    assert_eq!(guest.players().len(), 2);
}

/// A second guest is created on the first through the host's list, the
/// guests reach each other directly, and a guest that leaves is destroyed.
#[test]
fn guests_learn_each_other_and_a_leaving_player_is_destroyed() {
    let (mut host, info) = hosted();
    let mut first = join(&mut host, &info, "bob").unwrap();
    let second = join(&mut host, &info, "carol").unwrap();
    let second_id = second.local_id();
    assert_eq!(
        next(&mut first),
        Received::System(SystemMessage::PlayerCreated {
            id: second_id,
            name: "carol".into()
        })
    );
    second.send_all(b"hi");
    assert_eq!(
        next(&mut first),
        Received::Game {
            from: second_id,
            data: b"hi".to_vec()
        }
    );
    second.close();
    let destroyed = Received::System(SystemMessage::PlayerDestroyed {
        id: second_id,
        name: "carol".into(),
    });
    assert_eq!(
        settle(|| host
            .receive(Instant::now())
            .filter(|message| *message == destroyed)),
        destroyed
    );
    assert_eq!(
        settle(|| first
            .receive(Instant::now())
            .filter(|message| *message == destroyed)),
        destroyed
    );
}

/// When the host leaves, the lowest remaining id becomes the host
/// (DPSYS_HOST) and answers searches.
#[test]
fn the_lowest_remaining_player_becomes_the_host() {
    let (mut host, info) = hosted();
    let mut first = join(&mut host, &info, "bob").unwrap();
    let mut second = join(&mut host, &info, "carol").unwrap();
    settle(|| {
        first.poll(Instant::now());
        (first.players().len() == 3).then_some(())
    });
    let lowest = first.local_id().min(second.local_id());
    host.close();
    let became = Received::System(SystemMessage::BecameHost);
    let (new_host, other) = if first.local_id() == lowest {
        (&mut first, &mut second)
    } else {
        (&mut second, &mut first)
    };
    assert_eq!(
        settle(|| new_host
            .receive(Instant::now())
            .filter(|message| *message == became)),
        became
    );
    assert!(new_host.is_host());
    settle(|| {
        other.poll(Instant::now());
        (!other.is_host() && other.players().len() == 2).then_some(())
    });
    let mut search = SessionSearch::new(loopback(new_host.port().unwrap())).unwrap();
    let found = settle(|| {
        new_host.poll(Instant::now());
        search.refresh(Instant::now()).into_iter().next()
    });
    assert_eq!(found.players, 2);
}

/// A guest silent for the keep-alive timeout leaves; a host silent that long
/// migrates away; a list that no longer names a player loses its session.
#[test]
fn silence_and_the_hosts_list_end_players() {
    let (mut host, info) = hosted();
    let mut guest = join(&mut host, &info, "bob").unwrap();
    let guest_id = guest.local_id();
    assert!(matches!(
        next(&mut host),
        Received::System(SystemMessage::PlayerCreated { .. })
    ));
    let later = Instant::now() + TIMEOUT + Duration::from_secs(1);
    host.poll(later);
    assert_eq!(
        host.receive(later),
        Some(Received::System(SystemMessage::PlayerDestroyed {
            id: guest_id,
            name: "bob".into()
        }))
    );
    // The host's next list leaves the guest out.
    host.poll(later + KEEPALIVE);
    assert_eq!(
        settle(|| guest
            .receive(Instant::now())
            .filter(|message| *message == Received::System(SystemMessage::SessionLost))),
        Received::System(SystemMessage::SessionLost)
    );
}

#[test]
fn a_full_session_is_not_listed_and_refuses_a_join() {
    let (mut host, info) = hosted();
    let mut guests = Vec::new();
    for index in 1..MAX_PLAYERS {
        guests.push(join(&mut host, &info, &format!("guest{index}")).unwrap());
    }
    let mut search = SessionSearch::new(loopback(host.port().unwrap())).unwrap();
    for _ in 0..20 {
        host.poll(Instant::now());
        assert!(search.refresh(Instant::now()).is_empty());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(
        join(&mut host, &info, "late"),
        Err(NetSessionError::Refused(_))
    ));
}
