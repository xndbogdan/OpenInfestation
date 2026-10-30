use super::*;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector55_beamer_native.json"
    ))
    .unwrap()
}

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

/// 00473D70 against `check_selector55.py`: below a full charge it only adds
/// the step; full, it writes +138 = 0x6000, +140 = 0 and +13C = the process
/// NULL, and with an empty actor list scans nothing.
#[test]
fn the_charge_matches_00473d70() {
    let fixture = fixture();
    let null = word(&fixture["null_handle"]);
    let updates = fixture["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 121);
    let mut full = 0;
    for case in updates {
        let charge = word(&case["input"]["charge"]) as u16;
        let delta = word(&case["input"]["delta"]);
        let output = &case["output"];
        match beamer_charged(charge, delta) {
            Some(charged) => {
                assert_eq!(u32::from(charged), word(&output["charge"]), "{case}");
                assert_ne!(word(&output["receiver_13c"]), null, "{case}");
            }
            None => {
                assert_eq!(word(&output["charge"]), u32::from(BEAMER_CHARGE), "{case}");
                assert_eq!(word(&output["receiver_13c"]), null, "{case}");
                assert_eq!(word(&output["distance_140"]), 0, "{case}");
                full += 1;
            }
        }
    }
    assert_eq!(full, 102);
}

/// 00473EE0 against `check_selector55.py`, including a receiver that is not
/// nearer falling through to the person arm.
#[test]
fn the_filter_matches_00473ee0() {
    let fixture = fixture();
    let scores = fixture["scores"].as_array().unwrap();
    assert_eq!(scores.len(), 60);
    for case in scores {
        let input = &case["input"];
        let output = &case["output"];
        let mut state = Selector55State {
            charge_138: 0,
            receiver_13c: Some(ActorHandle::from_raw(word(&input["receiver"]))),
            distance_140: Some(word(&input["distance"])),
            range_144: word(&input["range"]),
        };
        let candidate = BeamerCandidate {
            resource_30: word(&input["resource"]) as u16,
            handle: ActorHandle::from_raw(word(&input["handle"])),
            view: input["view"]
                .as_bool()
                .unwrap()
                .then(|| (word(&input["candidate_6c"]), word(&input["team"]))),
        };
        let (score, receiver) = beamer_score(
            &mut state,
            word(&input["beamer_6c"]),
            candidate,
            word(&input["squared"]),
        );
        assert_eq!(score, word(&output["score"]), "{case}");
        assert_eq!(
            state.receiver_13c.unwrap().raw(),
            word(&output["receiver_13c"]),
            "{case}"
        );
        assert_eq!(
            state.distance_140,
            Some(word(&output["distance_140"])),
            "{case}"
        );
        let flags = word(&input["flags"]) | if receiver { RECEIVER_FLAG } else { 0 };
        assert_eq!(flags, word(&output["flags_2c"]), "{case}");
        assert_eq!(word(&output["dirty_38"]), word(&input["dirty"]), "{case}");
    }
}

/// [`ObjectArena::person_view`]'s premise: of the actor tables the port
/// builds, only selector 12's +5C answers (00404610); every other binds the
/// shared NULL 00401300.
#[test]
fn only_selector_12_answers_the_person_view() {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let word = |address: u32| {
        let at = (address - 0x0040_0000) as usize;
        u32::from_le_bytes(exe[at..at + 4].try_into().unwrap())
    };
    for selector in [
        2_u16, 6, 7, 9, 10, 11, 12, 15, 18, 21, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37,
        38, 39, 44, 47, 49, 52, 53, 55, 56, 57, 59, 60, 61, 62, 66, 68, 69, 73, 74, 77, 78, 79, 82,
        84, 85, 88, 90, 93,
    ] {
        let table = inf_formats::constructor_dispatch(selector).unwrap().vtable;
        let expected = if selector == 12 {
            0x0040_4610
        } else {
            0x0040_1300
        };
        assert_eq!(word(table + 0x5c), expected, "selector {selector}");
    }
}
