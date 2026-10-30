//! The Tractor Beam against `check_vehicle_tow.py`: Level 1's vehicle and a
//! scientist standing in for the ball (its resource word, flags, position,
//! velocity and mass are the receipt's), every other actor out of the scan.

use serde_json::{json, Value};

use super::*;
use crate::terrain_runtime::TerrainSpacing;
use std::convert::Infallible;

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

fn int(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

#[test]
fn the_tow_matches_00417e40() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_tow_native.json"
    ))
    .unwrap();
    let (_, _, world) = super::super::test_support::loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let ball = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector12_actor(actor).is_ok())
        .unwrap();
    for actor in arena.actor_list().to_vec() {
        arena.owned_mut(actor).unwrap().state.base.flags |= 0x80;
    }
    let player = arena.first_player().expect("Level 1's player");
    let null = arena.actor_handles().unwrap().null_handle();
    let handles = &fixture["handles"];
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 53);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let ball_handle = arena.actor(ball).unwrap().handle;
        let raw = |value: u32| {
            if value == int(&handles["null"]) {
                null.raw()
            } else if value == int(&handles["target"]) {
                if input["resolves"].as_u64() == Some(1) {
                    ball_handle.raw()
                } else {
                    // A handle the registry no longer resolves.
                    ball_handle.raw() ^ 0x0000_7fff
                }
            } else {
                value
            }
        };
        for (actor, fields) in [(vehicle, &input["vehicle"]), (ball, &input["target"])] {
            let base = &mut arena.owned_mut(actor).unwrap().state.base;
            base.flags = int(&fields["flags"]);
            base.resource_word = int(&fields["resource"]) as u16;
            base.position_words = Some(words(&fields["position"]));
            let state = arena.actor_mut(actor).unwrap();
            state.word_8c = int(&fields["mass"]) as u16;
            state.words_8e_92 = words(&fields["velocity"]);
        }
        arena
            .set_vehicle_tow(vehicle, raw(int(&input["tow"])), int(&input["length"]))
            .unwrap();
        let bits = &mut arena.world_player_mut(player).unwrap().bits_100;
        let bit = 1_u32 << (TRACTOR_UPGRADE & 31);
        if int(&input["owned"]) != 0 {
            bits[(TRACTOR_UPGRADE >> 5) as usize] |= bit;
        } else {
            bits[(TRACTOR_UPGRADE >> 5) as usize] &= !bit;
        }
        let mut controls = PlayerControlBlock::default();
        controls.set_byte(0x3b, int(&input["pickup"]) as u8);
        controls.set_byte(0x3c, int(&input["drop"]) as u8);
        let player = (int(&input["player"]) != 0).then_some(player);
        let mut smokes = Vec::new();
        arena
            .vehicle_tow(
                vehicle,
                &mut controls,
                player,
                null,
                int(&input["delta"]),
                &mut |kind, point: [u16; 3]| {
                    smokes.push(json!({"kind": "smoke", "type": kind, "point": point, "arg": 0}));
                    Ok::<_, Infallible>(())
                },
            )
            .unwrap_or_else(|error| panic!("case {index}: {error}"));
        let output = &case["output"];
        let tow = arena.vehicle_towed(vehicle).unwrap();
        let expected_tow = int(&output["tow"]);
        let expected_tow = if expected_tow == int(&handles["null"]) {
            null.raw()
        } else if expected_tow == int(&handles["target"]) {
            ball_handle.raw()
        } else {
            raw(expected_tow)
        };
        assert_eq!(tow, expected_tow, "case {index}");
        assert_eq!(
            arena.vehicle_tow_length(vehicle).unwrap(),
            int(&output["length"]),
            "case {index}"
        );
        for (actor, key) in [(vehicle, "vehicle"), (ball, "target")] {
            assert_eq!(
                arena.object(actor).unwrap().base.position_words,
                Some(words(&output[key]["position"])),
                "case {index} {key}"
            );
            assert_eq!(
                arena.actor(actor).unwrap().words_8e_92,
                words(&output[key]["velocity"]),
                "case {index} {key}"
            );
        }
        assert_eq!(
            u32::from(controls.byte(0x3b)),
            int(&output["pickup"]),
            "case {index}"
        );
        assert_eq!(
            u32::from(controls.byte(0x3c)),
            int(&output["drop"]),
            "case {index}"
        );
        let expected: Vec<Value> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call["kind"] == "smoke")
            .cloned()
            .collect();
        assert_eq!(smokes, expected, "case {index}");
    }
}
