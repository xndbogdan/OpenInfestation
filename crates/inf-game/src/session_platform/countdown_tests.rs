use super::*;
use crate::world_mission::{MissionCountdownTick, WorldMission};
use serde_json::{json, Value};

/// The doom's segment `0048E32A..0048E521` with a first player's unit, run
/// natively over 64 cases (`check_world_interactions.py`, its
/// `mission_countdown_unit` section): the RNG draws, each explosion's
/// factory point and burst record, the flash, the retirement of a unit
/// whose +64 answers, and the countdown's fields. The factory and the burst
/// are recorded hosts there, so they draw nothing between the point and the
/// flash's chance.
#[test]
fn the_doom_with_a_unit_matches_the_native_receipt() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap();
    let cases = fixture["mission_countdown_unit"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    let mut kinds = std::collections::BTreeMap::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let number = |key: &str| case[key].as_u64().unwrap();
        let position: [u16; 3] =
            std::array::from_fn(|axis| case["position"][axis].as_u64().unwrap() as u16);
        let mut mission = WorldMission::native_constructor();
        mission.current_world_a0 = 7;
        mission.byte_17 = Some(1);
        mission.byte_18 = Some(number("second") as u8);
        mission.dword_24 = Some(number("countdown") as u32);
        let mut rng = number("rng") as u32;
        let mut events = Vec::new();
        let tick = mission
            .tick_countdown(number("delta") as u32, true)
            .unwrap();
        match tick {
            MissionCountdownTick::Explosions => {
                let left = mission.dword_24.unwrap();
                if let Some(point) = countdown_explosion_point(&mut rng, left, position) {
                    events.push(json!({
                        "call": "create",
                        "resource": COUNTDOWN_EXPLOSION_RESOURCE,
                        "source": 0,
                        "point": point,
                        "rest": [0, 0xFFFF_FFFF_u32, 0, 0x12345],
                    }));
                    let burst = COUNTDOWN_BURST;
                    events.push(json!({
                        "call": "burst",
                        "record": [
                            burst.debris_count, point[0], point[1], point[2],
                            burst.placement_radius, 0, burst.central_type,
                            burst.debris_types[0], burst.debris_types[1],
                            burst.speed, burst.flags,
                        ],
                    }));
                    if countdown_chance(&mut rng, left) {
                        events.push(json!({
                            "call": "flash",
                            "args": [0, 0, COUNTDOWN_FLASH_STYLE],
                        }));
                    }
                }
            }
            MissionCountdownTick::UnitEnd => {
                if number("vehicle") != 0 {
                    events.push(json!({"call": "retire", "on_unit": true}));
                }
                mission.arm_doom_last_seconds();
            }
            MissionCountdownTick::MissionFailed { world } => {
                events.push(json!({"call": "world_end", "args": [world, 1, 0]}));
            }
            _ => {}
        }
        *kinds.entry(format!("{tick:?}")).or_insert(0) += 1;
        assert_eq!(Value::from(events), case["events"], "{name}");
        assert_eq!(u64::from(rng), number("rng_after"), "{name}");
        assert_eq!(
            u64::from(mission.dword_24.unwrap()),
            number("countdown_after"),
            "{name}"
        );
        assert_eq!(
            u64::from(mission.byte_18.unwrap()),
            number("second_after"),
            "{name}"
        );
        assert_eq!(
            u64::from(mission.bytes_14_16.unwrap()[1]),
            number("byte_15_after"),
            "{name}"
        );
    }
    // Every arm the doom has with a unit is reached.
    for kind in ["Explosions", "UnitEnd", "Counted"] {
        assert!(kinds.contains_key(kind), "{kinds:?}");
    }
    assert!(
        kinds.keys().any(|kind| kind.starts_with("MissionFailed")),
        "{kinds:?}"
    );
}
