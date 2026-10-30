use super::*;

/// The receipt's H0 pool: 0x400 slots, three named.
fn receipt_string(slot: i32) -> Vec<u8> {
    match slot {
        0x113 => b"Bonus Time".to_vec(),
        0x114 => b"Countdown".to_vec(),
        0x119 => b"Warp power hoops".to_vec(),
        _ => format!("S{slot:03X}").into_bytes(),
    }
}

/// `check_world_captions.py`: the original segment over 55 cases.
#[test]
fn the_world_captions_match_0044121e() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/world_captions_native.json"
    ))
    .unwrap();
    let int = |value: &serde_json::Value| value.as_i64().unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let inputs = WorldCaptionInputs {
            surface: [0, 1].map(|i| int(&input["surface"][i]) as i32),
            bonus_base: int(&input["bonus"]) as u32,
            clock: int(&input["clock"]) as u32,
            message_shown: int(&input["shown"]) != 0,
            attract: int(&input["attract"]) != 0,
            doom_stage: int(&input["stage"]) as u8,
            countdown: int(&input["countdown"]) as u32,
            count: int(&input["hoops"]) as u16,
            count_text: int(&input["hoops_text"]) as u16,
        };
        let capacity = int(&input["string_count"]) as i32;
        let [mul, add] = [0, 1].map(|i| int(&input["width"][i]) as i32);
        let captions = plan_world_captions::<()>(
            &inputs,
            |slot| Ok((slot <= capacity).then(|| receipt_string(slot))),
            |text| Ok((text.len() as i32).wrapping_mul(mul).wrapping_add(add)),
        )
        .unwrap();
        let expected: Vec<WorldCaption> = case["texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|text| WorldCaption {
                text: text["text"].as_str().unwrap().as_bytes().to_vec(),
                position: [int(&text["x"]) as i32, int(&text["y"]) as i32],
            })
            .collect();
        assert_eq!(captions, expected, "{input}");
    }
}
