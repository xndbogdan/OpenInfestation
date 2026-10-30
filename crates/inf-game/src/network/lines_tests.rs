use super::*;
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn lines(value: &Value, head: i64) -> MessageLines {
    let mut lines = MessageLines {
        head: head as u32,
        ..MessageLines::default()
    };
    for (line, input) in lines.lines.iter_mut().zip(value.as_array().unwrap()) {
        line.timer = int(&input[0]) as i32;
        let text = hex(input[1].as_str().unwrap());
        line.text[..text.len()].copy_from_slice(&text);
    }
    lines
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/net_lines_native.json")).unwrap()
}

/// `check_net_lines.py`: 0045C8D0 and 0045C1D0 over line states.
#[test]
fn the_lines_match_0045c8d0_and_0045c1d0() {
    let fixture = fixture();
    for case in fixture["allocations"].as_array().unwrap() {
        let mut state = lines(&case["lines"], int(&case["head_in"]));
        let picks: Vec<i64> = (0..int(&case["count"]))
            .map(|_| state.allocate() as i64)
            .collect();
        let expected: Vec<i64> = case["picks"].as_array().unwrap().iter().map(int).collect();
        assert_eq!(picks, expected, "{case}");
        let timers: Vec<i64> = state.lines.iter().map(|l| i64::from(l.timer)).collect();
        let expected: Vec<i64> = case["timers"].as_array().unwrap().iter().map(int).collect();
        assert_eq!(timers, expected, "{case}");
        assert_eq!(i64::from(state.head), int(&case["head"]), "{case}");
    }
    for case in fixture["ticks"].as_array().unwrap() {
        let mut state = lines(&case["lines"], int(&case["head_in"]));
        for step in case["steps"].as_array().unwrap() {
            let draws: Vec<Vec<i64>> = state
                .tick()
                .into_iter()
                .map(|(line, [x, y])| vec![line as i64, i64::from(x), i64::from(y)])
                .collect();
            let expected: Vec<Vec<i64>> = step["draws"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d.as_array().unwrap().iter().map(int).collect())
                .collect();
            assert_eq!(draws, expected, "{step}");
            let timers: Vec<i64> = state.lines.iter().map(|l| i64::from(l.timer)).collect();
            let expected: Vec<i64> = step["timers"].as_array().unwrap().iter().map(int).collect();
            assert_eq!(timers, expected, "{step}");
            assert_eq!(i64::from(state.head), int(&step["head"]), "{step}");
        }
    }
}

/// `check_net_lines.py`: 0045D7C0 over names, presence and H0 strings.
#[test]
fn the_formatter_matches_0045d7c0() {
    for case in fixture()["formats"].as_array().unwrap() {
        let mut state = GameNetState::default();
        for (slot, value) in case["present"].as_array().unwrap().iter().enumerate() {
            state.present[slot] = int(value) as u8;
        }
        for (slot, name) in case["names"].as_array().unwrap().iter().enumerate() {
            let name = hex(name.as_str().unwrap());
            let length = name.len().min(16);
            state.names[slot][..length].copy_from_slice(&name[..length]);
        }
        let count = int(&case["count"]) as i32;
        let out = format_line(
            &hex(case["format"].as_str().unwrap()),
            int(&case["room"]) as usize,
            [int(&case["a"]) as i32, int(&case["b"]) as i32],
            int(&case["strings"]) != 0,
            &state,
            |id| (id <= count).then(|| format!("H0_{id:02X}").into_bytes()),
        );
        let expected = hex(case["out"].as_str().unwrap());
        let end = expected.iter().position(|&b| b == 0).unwrap();
        assert_eq!(out, expected[..end].to_vec(), "{case}");
    }
}
