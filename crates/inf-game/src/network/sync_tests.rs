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

fn ints(value: &Value) -> Vec<i64> {
    value.as_array().unwrap().iter().map(int).collect()
}

fn vehicle(fields: &Value, hold: &Value) -> VehicleSync {
    let get = |key: &str| int(&fields[key]) as u32;
    VehicleSync {
        mode_1e8: get("mode_1e8"),
        dword_1ec: get("dword_1ec"),
        dword_1fc: get("dword_1fc"),
        byte_208: get("byte_208") as u8,
        word_20e: get("word_20e") as u16,
        dword_210: get("dword_210"),
        dword_220: get("dword_220"),
        dword_238: get("dword_238"),
        dword_244: get("dword_244"),
        word_58c: get("word_58c") as u16,
        hold: ints(hold).into_iter().map(|h| h as u32).collect(),
    }
}

fn words(bytes: &[u8]) -> [u16; 3] {
    [0, 2, 4].map(|at| u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn factory(fields: &Value) -> FactorySync {
    let get = |key: &str| int(&fields[key]);
    FactorySync {
        family: [get("word_13c") as i32, get("word_140") as i32],
        state_160: get("word_160") as u32,
        words_17e: [
            get("word_17e") as u16,
            get("word_180") as u16,
            get("word_182") as u16,
        ],
        handle_188: get("handle_188") as u32,
    }
}

/// `check_net_sync.py`: the built classes' +C0/+C4 slots.
#[test]
fn the_sync_slots_match_their_originals() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/net_sync_native.json")).unwrap();
    let null = int(&fixture["null"]) as u32;
    for case in fixture["cases"].as_array().unwrap() {
        match case["kind"].as_str().unwrap() {
            "family_out" => {
                let f = ints(&case["fields"]);
                let mut out = [0xccu8; 4];
                let result = family40d800_out(
                    &mut out,
                    int(&case["room"]) as i32,
                    [f[0] as i32, f[1] as i32],
                );
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(out.to_vec(), hex(case["out"].as_str().unwrap()), "{case}");
            }
            "family_in" => {
                let f = ints(&case["fields"]);
                let mut fields = [f[0] as i32, f[1] as i32];
                let result = family40d800_in(&[int(&case["byte"]) as u8], &mut fields).unwrap();
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(
                    fields.map(i64::from).to_vec(),
                    ints(&case["after"]),
                    "{case}"
                );
            }
            "shot_out" => {
                let w = ints(&case["words"]);
                let mut out = [0xccu8; 8];
                let result = shot_out(
                    &mut out,
                    int(&case["room"]) as i32,
                    ShotSync {
                        words_138: [w[0] as u16, w[1] as u16, w[2] as u16],
                        word_144: int(&case["word_144"]) as u16,
                        word_146: int(&case["word_146"]) as u16,
                    },
                );
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(out.to_vec(), hex(case["out"].as_str().unwrap()), "{case}");
            }
            "shot_in" => {
                let mut fields = ShotSync {
                    words_138: [0xa5a5; 3],
                    word_144: 0x5555,
                    word_146: int(&case["word_146_in"]) as u16,
                };
                let result = shot_in(&hex(case["payload"].as_str().unwrap()), &mut fields).unwrap();
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(
                    fields.words_138.map(i64::from).to_vec(),
                    ints(&case["words"]),
                    "{case}"
                );
                assert_eq!(i64::from(fields.word_144), int(&case["word_144"]), "{case}");
                assert_eq!(i64::from(fields.word_146), int(&case["word_146"]), "{case}");
            }
            "vehicle_out" => {
                let fields = vehicle(&case["fields"], &case["hold"]);
                let mut out = [0xccu8; 0x30];
                let result = vehicle_out(&mut out, int(&case["room"]) as i32, &fields, null);
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(out.to_vec(), hex(case["out"].as_str().unwrap()), "{case}");
            }
            "vehicle_in" => {
                let mut fields = vehicle(&case["fields"], &case["hold"]);
                let mut payload = hex(case["payload"].as_str().unwrap());
                payload.resize(payload.len() + 0x20, 0);
                let result =
                    vehicle_in(&payload, int(&case["length"]) as i32, &mut fields, null).unwrap();
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                let after = &case["after"];
                let get = |key: &str| int(&after[key]) as u32;
                assert_eq!(
                    [
                        fields.mode_1e8,
                        fields.dword_1ec,
                        fields.dword_1fc,
                        u32::from(fields.byte_208),
                        u32::from(fields.word_20e),
                        fields.dword_210,
                        fields.dword_220,
                        fields.dword_238,
                        fields.dword_244,
                        u32::from(fields.word_58c),
                    ],
                    [
                        "mode_1e8",
                        "dword_1ec",
                        "dword_1fc",
                        "byte_208",
                        "word_20e",
                        "dword_210",
                        "dword_220",
                        "dword_238",
                        "dword_244",
                        "word_58c"
                    ]
                    .map(get),
                    "{case}"
                );
                let hold: Vec<i64> = fields.hold.iter().map(|&h| i64::from(h)).collect();
                assert_eq!(hold, ints(&case["hold_after"]), "{case}");
            }
            "turret_out" => {
                let f = ints(&case["fields"]);
                let mut out = [0xccu8; 8];
                let result = turret_out(
                    &mut out,
                    int(&case["room"]) as i32,
                    [f[0] as u32, f[1] as u32],
                );
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(out.to_vec(), hex(case["out"].as_str().unwrap()), "{case}");
            }
            "turret_in" => {
                let mut fields = [0x5555_5555, 0x6666_6666];
                let packed = (int(&case["packed"]) as u32).to_le_bytes();
                let result = turret_in(&packed, &mut fields).unwrap();
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(
                    fields.map(i64::from).to_vec(),
                    ints(&case["after"]),
                    "{case}"
                );
            }
            "charge_out" => {
                let fields = ChargeSync {
                    handle_154: int(&case["handle"]) as u32,
                    offset_158: words(&hex(case["offset"].as_str().unwrap())),
                };
                let mut out = [0xccu8; 12];
                let result = charge_out(&mut out, int(&case["room"]) as i32, fields, null);
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(out.to_vec(), hex(case["out"].as_str().unwrap()), "{case}");
            }
            "charge_in" => {
                let mut fields = ChargeSync {
                    handle_154: 0x0102_0304,
                    offset_158: words(&hex(case["before"].as_str().unwrap())),
                };
                let mut payload = hex(case["payload"].as_str().unwrap());
                payload.resize(16, 0);
                let result =
                    charge_in(&payload, int(&case["length"]) as i32, &mut fields, null).unwrap();
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(i64::from(fields.handle_154), int(&case["handle"]), "{case}");
                assert_eq!(
                    fields.offset_158,
                    words(&hex(case["offset"].as_str().unwrap())),
                    "{case}"
                );
            }
            "factory_out" => {
                let mut out = [0xccu8; 16];
                let result = factory_out(
                    &mut out,
                    int(&case["room"]) as i32,
                    factory(&case["fields"]),
                    null,
                );
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(out.to_vec(), hex(case["out"].as_str().unwrap()), "{case}");
            }
            "factory_in" => {
                let mut fields = factory(&case["fields"]);
                let payload = hex(case["payload"].as_str().unwrap());
                let result = factory_in(&payload, &mut fields, null).unwrap();
                assert_eq!(i64::from(result), int(&case["result"]), "{case}");
                assert_eq!(fields, factory(&case["after"]), "{case}");
            }
            _ => {
                let hold: Vec<u32> = ints(&case["hold"]).into_iter().map(|h| h as u32).collect();
                assert_eq!(
                    i64::from(hold_has(&hold, int(&case["handle"]) as u32)),
                    int(&case["result"]),
                    "{case}"
                );
            }
        }
    }
}
