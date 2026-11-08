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

/// `PortFixes::paced_doom`: over five wall seconds the paced dice keep
/// retail's 30 fps explosion rate at any tick step. Retail's
/// `0048E380..0048E4C5` rolls every world tick, so a game ticking 61
/// times a second (the port's default quantum) rolls twice the dice of a
/// 30 fps game, and faster displays roll faster still, overflowing the
/// 400-cell particle pool: the newest bursts are reclaimed first, and
/// births that find no cell go to undrawn scratch.
#[test]
fn the_paced_doom_dice_keep_their_rate_at_any_tick_step() {
    use crate::frontend_settings::PortFixes;
    use crate::session_platform::{ActorPacing, WorldTickCarry};

    fn dice_per_five_wall_seconds(delta: u32, pacing: ActorPacing) -> (usize, usize) {
        let mut carry = WorldTickCarry::default();
        let mut rng = 0x1234_5678u32;
        let mut left = 5_000_000u32;
        let mut evaluations = 0usize;
        let mut hits = 0usize;
        while left > 0 {
            if carry.doom_pace_delta(pacing, delta).is_some() {
                evaluations += 1;
                if countdown_chance(&mut rng, left) {
                    hits += 1;
                }
            }
            left = left.saturating_sub(delta);
        }
        (evaluations, hits)
    }

    let paced = PortFixes::default().doom_pacing();
    assert_eq!(paced, ActorPacing::Paced { micros: 32768 });
    for delta in [33_333u32, 16_384, 6_944] {
        let (evaluations, hits) = dice_per_five_wall_seconds(delta, paced);
        // About 30.5 evaluations a second at any tick step.
        assert!((140..=160).contains(&evaluations), "{delta}: {evaluations}");
        assert!((25..=85).contains(&hits), "{delta}: {hits}");
    }
    // Unpaced, the dice scale with the tick rate: retail's own behavior,
    // kept by switching the fix off.
    let (retail_evaluations, _) = dice_per_five_wall_seconds(33_333, ActorPacing::Retail);
    let (exact_evaluations, _) = dice_per_five_wall_seconds(16_384, ActorPacing::Retail);
    let (fast_evaluations, _) = dice_per_five_wall_seconds(6_944, ActorPacing::Retail);
    assert_eq!(retail_evaluations, 151);
    assert_eq!(exact_evaluations, 306);
    assert_eq!(fast_evaluations, 721);
}
