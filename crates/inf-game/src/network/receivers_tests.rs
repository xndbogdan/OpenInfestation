use super::*;
use crate::network::game::{TeamScore, SLOTS, TEAMS};
use serde_json::{json, Value};

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Objects are `k + 1`, components `0x100 + k`, as the receipt names them.
fn name(id: u32) -> Value {
    if id >= 0x100 {
        json!(format!("comp{}", id - 0x100))
    } else {
        json!(format!("obj{}", id - 1))
    }
}

struct Host<'a> {
    input: &'a Value,
    null: u32,
    calls: Vec<Value>,
}

impl Host<'_> {
    fn object(&self, id: u32) -> &Value {
        &self.input["objects"][(id - 1) as usize]
    }
    fn answer(&self, key: &str, base: u32) -> u32 {
        self.input
            .get("answers")
            .and_then(|a| a.get(key))
            .and_then(Value::as_i64)
            .map_or(0, |k| base + k as u32)
    }
}

impl GameMessageHost for Host<'_> {
    type Error = ();
    fn call(&mut self, call: GameCall, _: &mut GameNetState) -> Result<u32, ()> {
        let (record, answer) = match call {
            GameCall::Resolve { handle } => {
                let target = self.input["resolve"]
                    .get(handle.to_string())
                    .and_then(Value::as_i64)
                    .map_or(0, |k| k as u32 + 1);
                (json!(["resolve", handle]), target)
            }
            GameCall::Dispose { object } => (json!(["dispose", name(object)]), 0),
            GameCall::SetState {
                object,
                null: process_null,
                state,
            } => (
                json!(["set_state", name(object), 0, process_null, state]),
                0,
            ),
            GameCall::Send { id, argument } => {
                let argument = match argument {
                    SendArgument::Value(v) => json!(v),
                    SendArgument::Name(slot) => json!(format!("name{slot}")),
                };
                (json!(["send", 1, id, argument]), 0)
            }
            GameCall::SlotUnknown { handle } => (
                json!(["slot_unknown", handle]),
                self.input["answers"]["slot_unknown"].as_i64().unwrap_or(0) as u32,
            ),
            GameCall::SlotObject { handle } => (
                json!(["slot_object", handle]),
                self.answer("slot_object", 1),
            ),
            GameCall::SlotFree { handle } => (json!(["slot_free", handle]), 0),
            GameCall::Hold {
                object,
                take,
                other,
                out,
            } => {
                let out = match out {
                    HoldOut::Stack => json!("stack"),
                    HoldOut::FieldD4 => json!(format!("obj{}+d4", object - 1)),
                };
                (
                    json!(["hold", name(object), u32::from(take), other, out]),
                    0,
                )
            }
            GameCall::Component { object } => (
                json!(["component", name(object)]),
                self.answer("component", 0x100),
            ),
            GameCall::ComponentTake {
                component,
                handle,
                held,
            } => (
                json!(["component_take", name(component), handle, name(held)]),
                0,
            ),
            GameCall::ComponentDrop {
                component,
                holder,
                held,
            } => (
                json!(["component_drop", name(component), name(holder), name(held)]),
                0,
            ),
            GameCall::Ground { object } => (json!(["ground", name(object)]), 0),
            GameCall::BannerOf { object } => (
                json!(["banner_of", name(object)]),
                self.answer("banner_of", 1),
            ),
            GameCall::BannerTaken { banner, by } => (json!(["banner_taken", name(banner), by]), 0),
            GameCall::BannerDropped { banner, by } => {
                (json!(["banner_dropped", name(banner), by]), 0)
            }
            GameCall::BannerCaptured { banner, values } => (
                json!(["banner_captured", name(banner), values[0], values[1]]),
                0,
            ),
            GameCall::FirstPlayer => (
                json!(["first_player"]),
                self.input
                    .get("answers")
                    .and_then(|a| a.get("player"))
                    .and_then(Value::as_i64)
                    .unwrap_or(1) as u32,
            ),
            GameCall::BannerEvent {
                banner,
                event,
                value,
            } => (
                json!(["banner_event", "player", name(banner), event, value]),
                0,
            ),
            GameCall::Damage {
                object,
                record,
                amount,
            } => (json!(["damage", name(object), record, amount]), 0),
            GameCall::WorldEffect {
                record,
                point,
                value,
            } => (
                json!([
                    "world_effect",
                    "G",
                    to_hex(&record),
                    to_hex(&point),
                    value,
                    0
                ]),
                0,
            ),
            GameCall::SonicPulse { object } => (json!(["sonic_pulse", name(object)]), 0),
            GameCall::Effect431ef0 { object } => (json!(["effect_431ef0", name(object)]), 0),
            GameCall::PointEffect {
                kind,
                argument,
                points,
                extra,
            } => (
                json!([
                    format!("point_effect_{kind}"),
                    argument,
                    to_hex(&points[0]),
                    to_hex(&points[1]),
                    extra.unwrap_or(self.null),
                    1
                ]),
                0,
            ),
            GameCall::PlayerText { text } => {
                (json!(["player_text", "player+88", to_hex(&text), 0]), 0)
            }
            GameCall::Sound {
                id,
                position,
                values,
            } => (
                json!(["sound", id, position, values[0], values[1], 0]),
                0xe0e0,
            ),
            GameCall::ErrorDispose { error } => (json!(["error_dispose", error]), 0),
            GameCall::WorldWords { words } => (json!(["world_words", "mission", words, 16]), 0),
            GameCall::WorldBit { bit } => (json!(["world_bit", "mission", bit]), 0),
        };
        self.calls.push(record);
        Ok(answer)
    }
    fn flags(&mut self, object: u32) -> Result<u32, ()> {
        Ok(self
            .object(object)
            .get("flags")
            .map_or(0, |v| int(v) as u32))
    }
    fn word_70(&mut self, object: u32) -> Result<u32, ()> {
        Ok(self
            .object(object)
            .get("word_70")
            .map_or(0, |v| int(v) as u32))
    }
    fn handle_84(&mut self, object: u32) -> Result<u32, ()> {
        Ok(self
            .object(object)
            .get("handle_84")
            .map_or(0x9000 + object - 1, |v| int(v) as u32))
    }
}

fn state(input: &Value) -> GameNetState {
    let get = |key: &str, default: i64| input.get(key).map_or(default, int);
    let mut state = GameNetState {
        host: get("host", 1) != 0,
        game_mode: get("mode", 0) as u32,
        ..GameNetState::default()
    };
    for slot in 0..SLOTS {
        state.states[slot] = 2;
        state.scores[slot] = [slot as u16, slot as u16, 100 * slot as u16];
        state.names[slot] = std::array::from_fn(|k| ((0x41 + slot + k) & 0xff) as u8);
        state.present[slot] = input.get("present").map_or(1, |p| int(&p[slot]) as u8);
        state.kill_words[slot] = input.get("kill_words").map_or(0, |p| int(&p[slot]) as u32);
    }
    for team in 0..TEAMS {
        state.teams[team] = Some(TeamScore {
            index: team as u32,
            ..TeamScore::default()
        });
    }
    state
}

/// `check_net_receivers.py`: every receiver of 00538050 but 1, 22 and 38 to
/// 44, its calls and the protocol state it keeps.
#[test]
fn the_receivers_match_the_message_table() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/net_receivers_native.json"
    ))
    .unwrap();
    let null = int(&fixture["null"]) as u32;
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut net = state(input);
        let mut host = Host {
            input,
            null,
            calls: Vec::new(),
        };
        let used = receive_message(
            int(&input["id"]) as u32,
            &hex(input["payload"].as_str().unwrap()),
            input.get("sender").map_or(2, int) as u8,
            input.get("local_slot").map_or(1, int) as u8,
            null,
            &mut net,
            &mut host,
        )
        .unwrap();
        assert_eq!(i64::from(used), int(&case["consumed"]), "{case}");
        assert_eq!(Value::Array(host.calls), case["calls"], "{case}");
        if let Some(words) = case.get("kill_words") {
            assert_eq!(json!(net.kill_words), *words, "{case}");
            assert_eq!(json!(net.scores), case["scores"], "{case}");
        }
        if let Some(words) = case.get("state_820") {
            let expected: Vec<u32> = words
                .as_array()
                .unwrap()
                .iter()
                .map(|v| int(v) as u32)
                .collect();
            let actual = [
                u32::from(net.score_limit)
                    | u32::from(net.winner) << 16
                    | u32::from(net.byte_823) << 24,
                net.winning_team,
                u32::from_le_bytes([net.team_game, net.ended, net.bytes_82a[0], net.bytes_82a[1]]),
            ];
            assert_eq!(actual.to_vec(), expected, "{case}");
            let teams: Vec<Value> = net
                .teams
                .iter()
                .map(|t| {
                    let t = t.unwrap();
                    json!([t.words_0c[0], t.words_0c[1], t.score, t.kills, t.deaths])
                })
                .collect();
            assert_eq!(Value::Array(teams), case["teams"], "{case}");
        }
        if let Some(present) = case.get("present") {
            assert_eq!(json!(net.present), *present, "{case}");
            let names: Vec<String> = net.names.iter().map(|n| to_hex(n)).collect();
            assert_eq!(json!(names), case["names"], "{case}");
            let details: Vec<String> = net.details.iter().map(|d| to_hex(d)).collect();
            assert_eq!(json!(details), case["details"], "{case}");
        }
    }
}
