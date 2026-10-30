use super::*;
use serde_json::{json, Value};

/// The receipt's H0 pool: its named texts, else "S%03X" of the slot.
fn receipt_string(slot: i32) -> Vec<u8> {
    match slot {
        0x163 => b"Targetting".to_vec(),
        0x15c => b"Alien Control".to_vec(),
        0x15d => b"Allied".to_vec(),
        0x15f => b"Warp to %s".to_vec(),
        0x160 => b"Surface-bound warpgate".to_vec(),
        _ => format!("S{slot:03X}").into_bytes(),
    }
}

fn int(value: &Value) -> i32 {
    value.as_i64().unwrap() as i32
}

/// The receipt's target as the arena and the furniture lookup answer it.
fn target(input: &Value) -> FreeLookTarget {
    let target = &input["target"];
    match target["type"].as_i64() {
        Some(2) => match target.get("actor") {
            Some(actor) if actor["handle"] != input["handle"] && int(&actor["name"]) != 0x113 => {
                FreeLookTarget::Actor(FreeLookActor {
                    name: int(&actor["name"]) as u16,
                    health: int(&actor["health"]) as u16,
                    maximum_health: Some(int(&actor["maximum"]) as u16),
                    side: int(&actor["side"]) as u32,
                    gate: (int(&actor["selector"]) == 0xf)
                        .then(|| int(&actor["destination"]) as u32),
                })
            }
            _ => FreeLookTarget::Hidden,
        },
        Some(4) => {
            let furniture = &target["furniture"];
            if int(&furniture["id"]) == 0 {
                return FreeLookTarget::Hidden;
            }
            // An unflagged cell reads the default record 005483C4, whose
            // flags the receipt leaves zero.
            let alien = int(&furniture["cell_flags"]) & 4 != 0
                && furniture["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| int(&entry[0]) == 0x30)
                    .is_some_and(|entry| int(&entry[1]) & 4 != 0);
            FreeLookTarget::Furniture(FreeLookFurniture {
                name: int(&furniture["name"]) as u16,
                alien,
            })
        }
        _ => FreeLookTarget::Absent,
    }
}

#[test]
fn free_look_matches_its_original_for_every_target() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/free_look_hud_native.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let pair = |key: &str| [int(&input[key][0]), int(&input[key][1])];
        let inputs = FreeLookInputs {
            surface: pair("surface"),
            focal: pair("focal"),
            clock: int(&input["clock"]),
            styles: [0x00c0_ffee, 0x00ba_ddad],
            side: int(&input["side"]) as u32,
            dword_5c0: int(&input["scope_5c0"]) as u32,
            target: target(input),
        };
        let count = int(&input["string_count"]);
        let [mul, add] = pair("width");
        let calls = plan_free_look(
            &inputs,
            |slot| Ok((slot <= count).then(|| receipt_string(slot))),
            |text| {
                Ok(i32::from(
                    (text.len() as i32).wrapping_mul(mul).wrapping_add(add) as i16,
                ))
            },
        )
        .unwrap();
        let mut countdown = inputs.dword_5c0 as i32;
        let mut recorded = Vec::new();
        for call in calls {
            recorded.push(match call {
                FreeLookCall::Light => json!(["light", "808080"]),
                FreeLookCall::Scope(corners) => {
                    let depths = [0x4000; 4];
                    json!([
                        "quad",
                        corners,
                        depths,
                        0,
                        int(&input["sprite_count"]) >= 0x29e
                    ])
                }
                FreeLookCall::Text {
                    text,
                    position: [x, y],
                    colour,
                } => json!(["text", String::from_utf8(text).unwrap(), x, y, colour]),
                FreeLookCall::Bar {
                    position: [x, y],
                    filled,
                    full,
                    thick,
                    rest,
                    bar,
                } => json!(["bar", x, y, filled, full, 0, thick, rest, bar]),
                FreeLookCall::Ring {
                    centre: [x, y],
                    radius,
                    vertical,
                } => json!(["ring", x, y, radius, u8::from(vertical), 0]),
                FreeLookCall::Chevron {
                    point: [x, y],
                    colour,
                } => json!(["chevron", x, y, colour, 0, 0]),
                FreeLookCall::Set5c0(value) => {
                    countdown = value as i32;
                    continue;
                }
                // Port-only: the original has no bands to draw.
                FreeLookCall::Band(_) => continue,
            });
        }
        // The furniture query's arguments are the caller's (00436BC0).
        let expected: Vec<Value> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call[0] != "furniture")
            .cloned()
            .collect();
        assert_eq!(recorded, expected, "{input}");
        assert_eq!(countdown, int(&case["scope_5c0"]), "{input}");
    }
}

#[test]
fn a_null_format_stops_where_retail_crashes() {
    let inputs = FreeLookInputs {
        surface: [640, 480],
        focal: [4, 3],
        clock: 0,
        styles: [1, 2],
        side: 1,
        dword_5c0: 0,
        target: FreeLookTarget::Furniture(FreeLookFurniture {
            name: 0x120,
            alien: false,
        }),
    };
    let error = plan_free_look(
        &inputs,
        |slot| Ok((slot != 0x120).then(|| receipt_string(slot))),
        |_| Ok(0),
    )
    .unwrap_err();
    assert!(error.contains("0x120"), "{error}");
}

/// Widescreen only: the scope spans `3H/2` times the focal ratio, which a
/// 4:3 surface never exceeds; on a 16:9 surface the bands either side of it
/// take black, so the world shows only through the scope, as at 4:3.
#[test]
fn a_wide_surface_blacks_the_bands_beside_the_scope() {
    let bands = |surface: [i32; 2], focal: [i32; 2]| -> Vec<[[i16; 2]; 4]> {
        let inputs = FreeLookInputs {
            surface,
            focal,
            clock: 0,
            styles: [0, 0],
            side: 1,
            dword_5c0: 0,
            target: FreeLookTarget::Absent,
        };
        plan_free_look(&inputs, |_| Ok(None), |_| Ok(0))
            .unwrap()
            .into_iter()
            .filter_map(|call| match call {
                FreeLookCall::Band(corners) => Some(corners),
                _ => None,
            })
            .collect()
    };
    assert!(bands([1024, 768], [512, 512]).is_empty());
    assert!(bands([640, 480], [320, 320]).is_empty());
    // 3 * 768 / 4 = 576 either side of 683.
    assert_eq!(
        bands([1366, 768], [683, 683]),
        [
            [[0, 0], [107, 0], [107, 768], [0, 768]],
            [[1259, 0], [1366, 0], [1366, 768], [1259, 768]],
        ]
    );
}
