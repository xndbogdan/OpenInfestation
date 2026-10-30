use super::*;
use crate::network::game::{TeamScore, SLOTS};
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn bytes<const N: usize>(value: &Value) -> [u8; N] {
    let list: Vec<u8> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| int(v) as u8)
        .collect();
    list.try_into().unwrap()
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn details_state() -> GameNetState {
    let mut state = GameNetState::default();
    for slot in 0..SLOTS {
        state.details[slot] = std::array::from_fn(|k| ((slot * 6 + k) & 0xff) as u8);
        state.names[slot] = std::array::from_fn(|k| ((0x40 + slot + k) & 0xff) as u8);
    }
    state
}

fn game_state(input: &Value) -> GameNetState {
    let mut state = GameNetState::default();
    let [limit, team, flags] = ["limit", "team", "flags"].map(|k| int(&input[k]) as u32);
    state.score_limit = limit as u16;
    state.winner = (limit >> 16) as u8;
    state.byte_823 = (limit >> 24) as u8;
    state.winning_team = team;
    [
        state.team_game,
        state.ended,
        state.bytes_82a[0],
        state.bytes_82a[1],
    ] = flags.to_le_bytes();
    for (slot, value) in input["present"].as_array().unwrap().iter().enumerate() {
        state.present[slot] = int(value) as u8;
    }
    for (slot, value) in input["states"].as_array().unwrap().iter().enumerate() {
        state.states[slot] = int(value) as u32;
    }
    for (slot, record) in input["scores"].as_array().unwrap().iter().enumerate() {
        let r: Vec<u16> = record
            .as_array()
            .unwrap()
            .iter()
            .map(|v| int(v) as u16)
            .collect();
        state.scores[slot] = [r[0], r[1], r[2]];
    }
    for (team, record) in input["teams"].as_array().unwrap().iter().enumerate() {
        let r: Vec<u16> = record
            .as_array()
            .unwrap()
            .iter()
            .map(|v| int(v) as u16)
            .collect();
        state.teams[team] = Some(TeamScore {
            index: team as u32,
            words_0c: [r[0], r[1]],
            score: r[2],
            kills: r[3],
            deaths: r[4],
        });
    }
    state
}

/// `check_net_encoders.py`: every argument- and state-fed encoder of
/// 00538050, over rooms that cut each one short.
#[test]
fn the_encoders_match_the_message_table() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/net_encoders_native.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let room = int(&case["room"]) as u32;
        let mut out = [0xccu8; 0x200];
        let value = |key: &str| int(&input[key]) as u32;
        let length = match case["kind"].as_str().unwrap() {
            "nothing" => encode_nothing(),
            "handle" => encode_handle(&mut out, value("value")),
            "pair" => encode_pair(&mut out, room, [value("first"), value("second")]),
            "damage" => encode_damage(
                &mut out,
                room,
                [value("record"), value("handle"), value("amount")],
            ),
            "effect" => encode_effect(
                &mut out,
                room,
                value("value"),
                bytes(&input["a"]),
                bytes(&input["b"]),
            ),
            "points" => encode_points(
                &mut out,
                room,
                [bytes(&input["a"]), bytes(&input["b"])],
                value("kind"),
                None,
            ),
            "points_extra" => encode_points(
                &mut out,
                room,
                [bytes(&input["a"]), bytes(&input["b"])],
                value("kind"),
                Some(value("extra")),
            ),
            "capture" => encode_capture(
                &mut out,
                room,
                [value("handle_84"), value("word_140"), value("word_13c")],
            ),
            "banner" => encode_banner(&mut out, room, [value("handle_84"), value("word_144")]),
            "kill_word" => encode_dword(&mut out, room, value("kill_word")),
            "transfer" | "value" => encode_dword(&mut out, room, value("value")),
            "details" => encode_details(&mut out, room, &details_state(), value("slot") as u8),
            "text" => encode_text(&mut out, room, &hex(input["text"].as_str().unwrap())),
            "sound" => {
                let position: Vec<u16> = input["position"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| int(v) as u16)
                    .collect();
                encode_sound(
                    &mut out,
                    room,
                    value("id"),
                    [position[0], position[1], position[2]],
                    [value("a"), value("b")],
                    value("flags") as u8,
                )
            }
            "mission" => encode_world_values(&mut out, room, [0xa1, 0xa2, 0xa3, 0xa4]),
            "game_state" => encode_game_state(&mut out, room, &game_state(input)).unwrap(),
            other => panic!("{other}"),
        };
        assert_eq!(i64::from(length), int(&case["length"]), "{case}");
        assert_eq!(
            out[..0x60].to_vec(),
            hex(case["out"].as_str().unwrap()),
            "{case}"
        );
    }
}
