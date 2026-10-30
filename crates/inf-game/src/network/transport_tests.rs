use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/net_transport_native.json"
    ))
    .unwrap()
}

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

/// `check_net_transport.py`'s rings: 004927B0..00492800 over each sequence.
#[test]
fn the_message_ring_matches_004927b0() {
    for case in fixture()["rings"].as_array().unwrap() {
        let (count, size) = (int(&case["count"]) as i32, int(&case["size"]) as i32);
        let mut ring = match MessageRing::build(count, size) {
            Ok(ring) => {
                assert_eq!(int(&case["built"]), 0, "{case}");
                ring
            }
            Err(_) => {
                assert_eq!(int(&case["built"]), 0x528d08, "{case}");
                continue;
            }
        };
        for step in case["steps"].as_array().unwrap() {
            let slot = |index: usize| if size == 0 { 0 } else { index as i64 };
            let result = match step["op"].as_str().unwrap() {
                "write" => {
                    let write = ring.indices()[1];
                    ring.write_slot().map(|_| slot(write))
                }
                "commit" => {
                    ring.commit();
                    None
                }
                "read" => {
                    let read = ring.indices()[0];
                    ring.read_slot().map(|_| slot(read))
                }
                "pop" => {
                    ring.pop();
                    None
                }
                "free" => {
                    ring.free();
                    None
                }
                other => panic!("{other}"),
            };
            assert_eq!(result, step["result"].as_i64(), "{case}\n{step}");
            let [read, write] = ring.indices();
            assert_eq!(
                [read as i64, write as i64],
                [0, 1].map(|i| int(&step["indices"][i]))
            );
            assert_eq!(ring.count() as i64, int(&step["count"]));
            assert_eq!(ring.slot_size() as i64, int(&step["size"]));
        }
    }
}

fn protocol<'a>(
    ch: i64,
    players: &'a mut PlayerTableProtocol,
    sequence: &'a mut SequenceProtocol,
) -> &'a mut dyn NetProtocol {
    if ch == 1 {
        sequence
    } else {
        players
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// `check_net_transport.py`'s cases: the framing, the buffers and the route
/// over the two built-in protocols, the state compared after every step.
#[test]
fn the_transport_matches_00492500_and_004926e0() {
    for case in fixture()["cases"].as_array().unwrap() {
        let init = &case["init"];
        let get = |key: &str, default: i64| init.get(key).map_or(default, int);
        let mut transport = NetTransport::default();
        transport.state.network_active = get("network", 1) != 0;
        transport.state.host = get("host", 0) != 0;
        transport.state.session_id = get("session", 0) as u32;
        transport.state.local_slot = get("local_slot", 0) as u8;
        transport.state.next_sequence = get("sequence", 0) as u32;
        if let Some(players) = init.get("players") {
            for (entry, value) in transport
                .state
                .players
                .iter_mut()
                .zip(players.as_array().unwrap())
            {
                *entry = int(value) as u32;
            }
        }
        let mut players = PlayerTableProtocol::default();
        let mut sequence = SequenceProtocol::default();
        for step in case["steps"].as_array().unwrap() {
            let op = step["op"].as_array().unwrap();
            let arg = |i: usize| int(&op[i]);
            let result = match op[0].as_str().unwrap() {
                "setup" => {
                    transport
                        .setup_buffers(arg(1) as i32, [&mut players, &mut sequence])
                        .unwrap();
                    None
                }
                "start" => {
                    transport.start_channels().unwrap();
                    Some(0)
                }
                "send" => {
                    let ch = arg(1);
                    transport
                        .send(
                            ch as usize,
                            protocol(ch, &mut players, &mut sequence),
                            arg(2) as u32,
                            arg(3) as u32,
                        )
                        .unwrap();
                    None
                }
                "finish" => {
                    let ch = arg(1);
                    transport.finish_frame(ch as usize, protocol(ch, &mut players, &mut sequence));
                    None
                }
                "flush" => {
                    let ch = arg(1);
                    transport.flush_frame(ch as usize, protocol(ch, &mut players, &mut sequence));
                    None
                }
                "route" => Some(
                    transport
                        .route([&mut players, &mut sequence], &hex(op[1].as_str().unwrap()))
                        .unwrap() as i64,
                ),
                other => panic!("{other}"),
            };
            assert_eq!(result, step["result"].as_i64(), "{step}");
            let state = &step["state"];
            for (ch, expected) in state["channels"].as_array().unwrap().iter().enumerate() {
                let channel = &transport.channels[ch];
                let ring = &expected["ring"];
                let [read, write] = channel.ring.indices();
                assert_eq!(
                    [read, write, channel.ring.count(), channel.ring.slot_size()].map(|v| v as i64),
                    ["read", "write", "count", "size"].map(|k| int(&ring[k])),
                    "channel {ch}: {op:?}"
                );
                assert_eq!(
                    format!("{:x}", Sha256::digest(channel.ring.storage())),
                    ring["storage"].as_str().unwrap(),
                    "channel {ch} bytes: {op:?}"
                );
                let open = channel.open.map(|open| {
                    [
                        open.slot_size as i64,
                        open.used as i64,
                        i64::from(open.room),
                        open.cursor as i64,
                        i64::from(open.entries),
                    ]
                });
                let expected_open = expected["open"].as_object().map(|open| {
                    ["slot_size", "used", "room", "cursor", "entries"].map(|k| int(&open[k]))
                });
                assert_eq!(open, expected_open, "channel {ch} frame: {op:?}");
            }
            let s = &transport.state;
            assert_eq!(
                i64::from(transport.send_signals),
                int(&state["signals"]),
                "{op:?}"
            );
            let expected_players: Vec<u32> = state["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| int(v) as u32)
                .collect();
            assert_eq!(s.players.to_vec(), expected_players, "{op:?}");
            assert_eq!(i64::from(s.local_slot), int(&state["local_slot"]), "{op:?}");
            assert_eq!(
                i64::from(s.slot_assigned),
                int(&state["assigned"]),
                "{op:?}"
            );
            assert_eq!(
                i64::from(s.tables_received),
                int(&state["received"]),
                "{op:?}"
            );
            assert_eq!(
                s.players_added.map(i64::from),
                [0, 1].map(|i| int(&state["added"][i])),
                "{op:?}"
            );
            assert_eq!(
                i64::from(s.last_sequence),
                int(&state["last_sequence"]),
                "{op:?}"
            );
            assert_eq!(
                i64::from(s.next_sequence),
                int(&state["next_sequence"]),
                "{op:?}"
            );
            assert_eq!(
                i64::from(s.buffer_size),
                int(&state["buffer_size"]),
                "{op:?}"
            );
        }
    }
}
