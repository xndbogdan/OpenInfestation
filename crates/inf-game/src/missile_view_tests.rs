//! The view, its static and the bar against `check_missile_view.py`.

use serde_json::{json, Value};

use super::*;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/missile_view_native.json")).unwrap()
}

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn quad_json(quad: &DirectSpriteQuad) -> Value {
    let mut out = json!({
        "corners": quad.corners,
        "dwords": quad.vertex_depths,
        "descriptor": quad.descriptor_word,
        "sprite": quad.sprite_slot,
    });
    if let DirectSpriteMode::Effect { color_word } = quad.mode {
        out["colour"] = json!(color_word);
    }
    out
}

fn strip_renderer(quads: &Value) -> Vec<Value> {
    quads
        .as_array()
        .unwrap()
        .iter()
        .map(|quad| {
            let mut quad = quad.clone();
            quad.as_object_mut().unwrap().remove("renderer");
            quad
        })
        .collect()
}

/// `0047F080` in both modes, quad for quad.
#[test]
fn the_bar_matches_0047f080() {
    let fixture = fixture();
    for case in fixture["bars"].as_array().unwrap() {
        let args: Vec<i64> = case["input"].as_array().unwrap().iter().map(int).collect();
        let mut ops = Vec::new();
        hud_bar(
            &mut ops,
            [args[0] as i32, args[1] as i32],
            args[2] as i32,
            args[3] as i32,
            args[4] as i32,
            args[5] as i32,
            args[6] as u32,
            args[7] as u32,
        );
        let quads: Vec<Value> = ops
            .iter()
            .map(|op| match op {
                HudOp::Quad(quad) => quad_json(quad),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(quads, strip_renderer(&case["quads"]), "{case}");
    }
}

/// `00456B50`: its quads and the RNG it leaves.
#[test]
fn the_static_matches_00456b50() {
    let fixture = fixture();
    let widths = &fixture["widths"];
    for case in fixture["statics"].as_array().unwrap() {
        let input = &case["input"];
        let rect: Vec<i16> = input["rect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| int(value) as i16)
            .collect();
        let mut rng = int(&input["rng"]) as u32;
        let quads = static_quads(
            int(&input["count"]) as i32,
            [rect[0], rect[1]],
            [rect[2], rect[3]],
            &mut rng,
            |slot| Ok::<_, ()>(int(&widths[format!("{slot:#x}")]) as u16),
        )
        .unwrap();
        let quads: Vec<Value> = quads.iter().map(quad_json).collect();
        assert_eq!(quads, strip_renderer(&case["quads"]), "{input}");
        assert_eq!(rng, int(&case["rng"]) as u32, "{input}");
    }
}

/// A widescreen frame's static leaves the RNG as retail's 4:3 surface does,
/// and still covers every row out to its width.
#[test]
fn the_wide_static_draws_retails_numbers() {
    let fixture = fixture();
    let widths = &fixture["widths"];
    let width = |slot: u16| Ok::<_, ()>(int(&widths[format!("{slot:#x}")]) as u16);
    let [mut retail, mut wide] = [0x1234_5678u32; 2];
    let four_three = surface_static_quads([1024, 768], &mut retail, width).unwrap();
    let widescreen = surface_static_quads([1365, 768], &mut wide, width).unwrap();
    assert_eq!(wide, retail);
    assert_eq!(widescreen[..four_three.len()], four_three[..]);
    for row in 0..=768i16 {
        let right = widescreen
            .iter()
            .filter(|quad| quad.corners[0][1] == row)
            .map(|quad| quad.corners[0][0].max(quad.corners[1][0]))
            .max();
        assert!(right >= Some(1365), "row {row} ends at {right:?}");
    }
}

/// `00456CF0`'s calls, the services' answers supplied as the receipt did.
#[test]
fn the_view_matches_00456cf0() {
    let fixture = fixture();
    let styles = [
        int(&fixture["styles"][0]) as u32,
        int(&fixture["styles"][1]) as u32,
    ];
    let views = fixture["views"].as_array().unwrap();
    assert_eq!(views.len(), 40);
    for case in views {
        let input = &case["input"];
        let words =
            |key: &str| -> Vec<i64> { input[key].as_array().unwrap().iter().map(int).collect() };
        let (surface, rates, position, velocity) = (
            words("surface"),
            words("rates"),
            words("position"),
            words("velocity"),
        );
        let inputs = MissileViewInputs {
            surface: [surface[0] as i32, surface[1] as i32],
            hud_setting: int(&input["hud"]) as u32,
            styles,
            disrupted: int(&input["static"]) != 0,
            held: int(&input["player"]) != 0,
            rates: std::array::from_fn(|i| rates[i] as i32),
            height: position[1] as u16 as i16,
            ground: int(&input["ground"]) as u16 as i16,
            water: int(&input["water"]) as u16 as i16,
            velocity: std::array::from_fn(|i| velocity[i] as i16),
            fuse: int(&input["fuse"]) as u16,
        };
        let mut expected = Vec::new();
        for call in case["calls"].as_array().unwrap() {
            match call["kind"].as_str().unwrap() {
                "static" => {
                    assert_eq!(call["start"], json!([0, 0]));
                    assert_eq!(call["count"], json!(-1));
                    expected.push(MissileViewCall::Static {
                        end: [int(&call["end"][0]) as i32, int(&call["end"][1]) as i32],
                    });
                }
                "chevron" => {
                    assert_eq!((call["w"].clone(), call["h"].clone()), (json!(0), json!(0)));
                    expected.push(MissileViewCall::Chevron {
                        point: [int(&call["point"][0]) as i32, int(&call["point"][1]) as i32],
                        colour: int(&call["colour"]) as u32,
                    });
                }
                "bar" => {
                    let args: Vec<i64> = call["args"].as_array().unwrap().iter().map(int).collect();
                    assert_eq!(args[2], 0);
                    expected.push(MissileViewCall::Bar {
                        point: [args[0] as i32, args[1] as i32],
                        width: args[3] as i32,
                        fill: args[4] as i32,
                        height: args[5] as i32,
                        rest: args[6] as u32,
                        bar: args[7] as u32,
                    });
                }
                "shared" => expected.push(MissileViewCall::SharedView),
                "player" => expected.push(MissileViewCall::PlayerHud),
                "ground" | "water" => {
                    assert_eq!(int(&call["x"]) as u16 as i64, position[0], "{input}");
                    assert_eq!(int(&call["z"]) as u16 as i64, position[2], "{input}");
                }
                "width" => {}
                "text" => {
                    assert_eq!(call["colour"], fixture["styles"][0]);
                    assert_eq!(call["callback"], "0x439de0");
                    let text = call["text"].as_str().unwrap();
                    // The receipt's 004A4240 answered 7 per glyph plus 1.
                    let width = 7 * text.len() as i32 + 1;
                    let Some(MissileViewCall::Counter { centre, .. }) =
                        plan_missile_view(&inputs).last().cloned()
                    else {
                        panic!("{input}: no counter");
                    };
                    assert_eq!(int(&call["x"]) as i32, centre[0] - width / 2, "{input}");
                    assert_eq!(int(&call["y"]) as i32, centre[1], "{input}");
                    expected.push(MissileViewCall::Counter {
                        text: text.as_bytes().to_vec(),
                        centre,
                    });
                }
                other => panic!("{other}"),
            }
        }
        assert_eq!(plan_missile_view(&inputs), expected, "{input}");
    }
}
