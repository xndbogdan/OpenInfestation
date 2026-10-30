use super::*;
use serde_json::Value;
use std::collections::BTreeMap;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/net_game_native.json")).unwrap()
}

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn ints(value: &Value) -> Vec<i64> {
    value.as_array().unwrap().iter().map(int).collect()
}

/// `check_net_game.py`'s packs: 0045DC10 and 0045DD00.
#[test]
fn the_vector_packers_match_0045dc10_and_0045dd00() {
    for case in fixture()["packs"].as_array().unwrap() {
        let v = ints(&case["vector"]);
        let packed = match case["kind"].as_str().unwrap() {
            "word" => pack_word_vector([v[0] as i16, v[1] as i16, v[2] as i16]),
            _ => pack_dword_vector([v[0] as i32, v[1] as i32, v[2] as i32]),
        };
        assert_eq!(i64::from(packed), int(&case["packed"]), "{case}");
    }
}

/// `check_net_game.py`'s unpacks: 0045DCC0 and 0045DD90 over the packed
/// vectors and random DWORDs.
#[test]
fn the_vector_unpackers_match_0045dcc0_and_0045dd90() {
    let cases = fixture()["unpacks"].as_array().unwrap().clone();
    assert_eq!(cases.len(), 184);
    for case in cases {
        let packed = int(&case["packed"]) as u32;
        let words = unpack_word_vector(packed).map(i64::from);
        let dwords = unpack_dword_vector(packed).map(i64::from);
        assert_eq!(words.to_vec(), ints(&case["word"]), "{case}");
        assert_eq!(dwords.to_vec(), ints(&case["dword"]), "{case}");
    }
}

const SETTING_OFFSETS: [usize; 9] = [0x10, 0x1c, 0x20, 0x24, 0x28, 0x2c, 0x34, 0x4c, 0x70];

struct Host {
    settings: BTreeMap<usize, u32>,
    mission: (i32, u8),
    h12: bool,
    rebuilds: Vec<u32>,
}

impl GameHeaderHost for Host {
    type Error = ();
    fn settings_word(&mut self, offset: usize) -> u32 {
        self.settings[&offset]
    }
    fn set_settings_word(&mut self, offset: usize, value: u32) {
        self.settings.insert(offset, value);
    }
    fn mission(&self) -> (i32, u8) {
        self.mission
    }
    fn rebuild_terrain(&mut self, detail: u32) -> Result<bool, ()> {
        if self.h12 {
            self.rebuilds.push(detail);
        }
        Ok(self.h12)
    }
}

/// `check_net_game.py`'s headers: 0045C3D0 over supplied sessions, records,
/// missions and settings.
#[test]
fn the_header_reader_matches_0045c3d0() {
    for case in fixture()["headers"].as_array().unwrap() {
        let input = &case["input"];
        let mut state = GameNetState::default();
        for (slot, word) in ints(&input["session_words"]).into_iter().enumerate() {
            state.session_words[slot] = word as u32;
        }
        state.records = ints(&input["records"])
            .into_iter()
            .map(|sequence| SlotRecord {
                sequence: sequence as u32,
            })
            .collect();
        let mut host = Host {
            settings: SETTING_OFFSETS
                .iter()
                .zip(ints(&input["settings"]))
                .map(|(&offset, value)| (offset, value as u32))
                .collect(),
            mission: (int(&input["world"]) as i32, int(&input["flag_12"]) as u8),
            h12: input["h12"].as_str().unwrap() != "none",
            rebuilds: Vec::new(),
        };
        let header: Vec<u8> = ints(&input["header"])
            .into_iter()
            .flat_map(|word| (word as u32).to_le_bytes())
            .collect();
        let proceed = state.read_header(&header, &mut host).unwrap();
        assert_eq!(i64::from(proceed), int(&case["ok"]), "{case}");
        assert_eq!(
            state.header_in.map(i64::from).to_vec(),
            ints(&case["header_in"]),
            "{case}"
        );
        let slot = ((state.header_in[3] >> 16) & 0xff) as usize;
        assert_eq!(i64::from(state.states[slot]), int(&case["state"]), "{case}");
        assert_eq!(
            i64::from(state.heard[slot]),
            int(&case["present"]),
            "{case}"
        );
        let records: Vec<i64> = state
            .records
            .iter()
            .map(|r| i64::from(r.sequence))
            .collect();
        assert_eq!(records, ints(&case["records"]), "{case}");
        let settings: Vec<i64> = SETTING_OFFSETS
            .iter()
            .map(|offset| i64::from(host.settings[offset]))
            .collect();
        assert_eq!(settings, ints(&case["settings"]), "{case}");
        assert_eq!(
            i64::from(state.host_flag_23),
            int(&case["flag_23"]),
            "{case}"
        );
        assert_eq!(
            i64::from(state.host_world),
            int(&case["host_world"]),
            "{case}"
        );
        assert_eq!(
            i64::from(state.detail_refusals),
            int(&case["refusals"]),
            "{case}"
        );
        let rebuilds: Vec<i64> = host.rebuilds.iter().map(|&d| i64::from(d)).collect();
        assert_eq!(rebuilds, ints(&case["rebuilds"]), "{case}");
    }
}

/// `check_net_game.py`'s scores: 0045D350, 0045D4C0, 0045C150 and 0045C190.
#[test]
fn the_scores_match_0045d350_and_0045d4c0() {
    for case in fixture()["scores"].as_array().unwrap() {
        let input = &case["input"];
        let mut state = GameNetState {
            host: int(&input["host"]) != 0,
            team_game: int(&input["team_game"]) as u8,
            ended: int(&input["ended"]) as u8,
            score_limit: int(&input["limit"]) as u16,
            ..GameNetState::default()
        };
        for (slot, record) in input["scores"].as_array().unwrap().iter().enumerate() {
            let r = ints(record);
            state.scores[slot] = [r[0] as u16, r[1] as u16, r[2] as u16];
        }
        for (slot, value) in ints(&input["states"]).into_iter().enumerate() {
            state.states[slot] = value as u32;
        }
        for (team, record) in input["teams"].as_array().unwrap().iter().enumerate() {
            let r = ints(record);
            state.teams[team] = Some(TeamScore {
                index: team as u32,
                words_0c: [0; 2],
                score: r[0] as u16,
                kills: r[1] as u16,
                deaths: r[2] as u16,
            });
        }
        let op = input["op"].as_array().unwrap();
        let mode = int(&input["mode"]) as u32;
        let result = match op[0].as_str().unwrap() {
            "kill" => {
                state
                    .record_kill(int(&op[1]) as i32, int(&op[2]) as i32, mode)
                    .unwrap();
                None
            }
            "transfer" => {
                let bytes = ints(&op[1]);
                state.transfer_score([0, 1, 2, 3].map(|i| bytes[i] as u8));
                None
            }
            "player_limit" => Some(i64::from(state.check_player_limit(int(&op[1]) as u8))),
            _ => {
                let team = state.teams[int(&op[1]) as usize].unwrap();
                Some(i64::from(state.check_team_limit(team)))
            }
        };
        assert_eq!(result, case["result"].as_i64(), "{case}");
        let scores: Vec<Vec<i64>> = state
            .scores
            .iter()
            .map(|r| r.iter().map(|&v| i64::from(v)).collect())
            .collect();
        let expected: Vec<Vec<i64>> = case["scores"]
            .as_array()
            .unwrap()
            .iter()
            .map(ints)
            .collect();
        assert_eq!(scores, expected, "{case}");
        let teams: Vec<Vec<i64>> = state
            .teams
            .iter()
            .map(|t| {
                let t = t.unwrap();
                vec![i64::from(t.score), i64::from(t.kills), i64::from(t.deaths)]
            })
            .collect();
        let expected: Vec<Vec<i64>> = case["teams"].as_array().unwrap().iter().map(ints).collect();
        assert_eq!(teams, expected, "{case}");
        assert_eq!(i64::from(state.ended), int(&case["ended"]), "{case}");
        assert_eq!(i64::from(state.winner), int(&case["winner"]), "{case}");
        assert_eq!(
            i64::from(state.winning_team),
            int(&case["winning_team"]),
            "{case}"
        );
    }
}

#[test]
fn a_slot_without_a_team_stops_rather_than_dereferencing_null() {
    let mut state = GameNetState {
        host: true,
        ..GameNetState::default()
    };
    state.states[3] = 0x13;
    assert_eq!(
        state.record_kill(3, 4, 0),
        Err(GameNetError::MissingTeam {
            slot: 3,
            state: 0x13
        })
    );
}
