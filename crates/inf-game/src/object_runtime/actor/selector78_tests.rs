use super::*;
use crate::object_runtime::{
    ActorRegistryConfig, ActorTerrainInputs, Selector90MotionMode, TargetNullHandleSource,
};

const SINE: [u16; 4096] = [0; 4096];

fn request(position: [u16; 3]) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some(position),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

/// What the receipt records: the sparks, retirements and world ends.
#[derive(Default)]
struct Recorder {
    rng: u32,
    events: Vec<serde_json::Value>,
    bonus: bool,
}

impl TargetNullHandleSource for Recorder {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        unreachable!("the receipt's states build no route target")
    }
}

impl ArenaIntroServices for Recorder {
    type Error = std::convert::Infallible;

    fn create_helper(&mut self, _: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        unreachable!("state 1 is not left")
    }

    fn camera(&mut self, _: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        unreachable!("state 4 is not entered")
    }

    fn retire_actor(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        self.events.push(serde_json::json!({"kind": "retire"}));
        Ok(())
    }

    fn world_present(&mut self) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn complete_mission(
        &mut self,
        _: &mut ObjectArena,
        selection: u32,
        start_ordinal: u16,
        argument: u32,
    ) -> Result<(), Self::Error> {
        self.events.push(serde_json::json!({
            "kind": "world_end", "args": [selection, start_ordinal, argument]
        }));
        Ok(())
    }
}

impl Selector78Host for Recorder {
    fn rng(&mut self) -> &mut u32 {
        &mut self.rng
    }

    fn take_player(
        &mut self,
        _: &mut ObjectArena,
        _: PlayerId,
        _: ActorHandle,
    ) -> Result<(), Self::Error> {
        unreachable!("state 1 is not left")
    }

    fn spark(
        &mut self,
        _: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
    ) -> Result<(), Self::Error> {
        self.events.push(serde_json::json!({
            "kind": "spark", "point": point, "velocity": velocity,
            "owner": owner.raw(), "seed": self.rng
        }));
        Ok(())
    }

    fn completion_bonus(&mut self) {
        self.bonus = true;
    }
}

/// `004072A0` and `00406830` against the 88 cases of
/// `selector78_native.json`, run on the original instructions with the real
/// RNG and selector 90's own `00462BA0`: the sparks round a victim and its
/// retirement, and runs of updates through states 2, 3 and 5 onward, each
/// update's clock, state, phase, helpers' speeds and arrivals, the world's
/// end and the settings bit.
#[test]
fn the_ending_matches_the_native_receipt() {
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/selector78_native.json"
    ))
    .unwrap();
    let number = |value: &serde_json::Value| value.as_i64().unwrap();
    let words = |value: &serde_json::Value| -> [u16; 3] {
        let list: Vec<u16> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|word| number(word) as u16)
            .collect();
        [list[0], list[1], list[2]]
    };
    let mut checked = 0;
    for section in ["spark", "update"] {
        for (index, case) in fixture[section].as_array().unwrap().iter().enumerate() {
            let input = &case["input"];
            let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
                owner_count: 8,
                initial_capacity: 8,
                null_handle: ActorHandle::from_raw(0),
            })
            .unwrap();
            let mut listeners = CreationListeners::new();
            let director = arena
                .allocate_selector78(request([0; 3]), &mut listeners)
                .unwrap();
            let victim = arena
                .allocate_actor(request(words(&input["victim_position"])))
                .unwrap();
            arena.actor_mut(victim).unwrap().word_c4 = number(&input["victim_state"]) as u32;
            let victim_handle = arena.actor(victim).unwrap().handle;
            let helpers = input["helpers"].as_bool().unwrap();
            let helper = |arena: &mut ObjectArena, spec: &serde_json::Value| {
                let helper = arena.allocate_selector90(request([0; 3])).unwrap();
                for (field, key) in [
                    (Selector90Word::Approach138, "approach"),
                    (Selector90Word::Speed13c, "speed"),
                ] {
                    arena
                        .write_selector90_word(helper, field, number(&spec[key]) as i32)
                        .unwrap();
                }
                for (field, key) in [
                    (Selector90Byte::Arrival140, "arrived"),
                    (Selector90Byte::Mode142, "mode"),
                ] {
                    arena
                        .write_selector90_byte(helper, field, number(&spec[key]) as u8)
                        .unwrap();
                }
                helper
            };
            let (guide, bug) = if helpers {
                (
                    Some(helper(&mut arena, &input["guide"])),
                    Some(helper(&mut arena, &input["bug"])),
                )
            } else {
                (None, None)
            };
            let state = arena.selector78_mut(director).unwrap();
            state.clock_158 = number(&input["clock"]) as i32;
            state.victim_clock_15c = Some(number(&input["victim_clock"]) as i32);
            state.state_160 = number(&input["state"]) as u32;
            state.helper_164 = guide;
            state.helper_168 = bug;
            state.victim_16c = victim_handle;
            // The receipt's routes are never read: neither state 1's exit
            // nor state 5 is reached.
            arena.alien_brain.phase_546910 = number(&input["phase"]) as u32;
            let mut host = Recorder {
                rng: number(&input["seed"]) as u32,
                ..Recorder::default()
            };
            let inputs = ArenaIntroInputs {
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                settings_7c: 0,
                motion_mode: Selector90MotionMode::FractionalThousandths,
                up: [0.0, 1.0, 0.0],
            };
            for delta in input["deltas"].as_array().unwrap() {
                arena
                    .tick_selector78(director, number(delta) as u32, inputs, &mut host)
                    .unwrap();
            }
            let name = format!("{section} {index}");
            let expected: Vec<serde_json::Value> = case["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["kind"] != "ending")
                .map(|event| match event["kind"].as_str().unwrap() {
                    "spark" => {
                        assert_eq!(number(&event["selector"]), 1, "{name}");
                        assert_eq!(number(&event["flags"]), 0, "{name}");
                        serde_json::json!({
                            "kind": "spark", "point": event["point"], "velocity": event["velocity"],
                            "owner": victim_handle.raw(), "seed": event["seed"]
                        })
                    }
                    "retire" => {
                        assert!(event["victim"].as_bool().unwrap(), "{name}");
                        serde_json::json!({"kind": "retire"})
                    }
                    _ => event.clone(),
                })
                .collect();
            assert_eq!(host.events, expected, "{name}");
            let output = &case["output"];
            let state = arena.selector78(director).unwrap();
            assert_eq!(
                state.clock_158 as u32 as i64,
                number(&output["clock"]),
                "{name}"
            );
            assert_eq!(
                state.victim_clock_15c.unwrap() as u32 as i64,
                number(&output["victim_clock"]),
                "{name}"
            );
            assert_eq!(
                i64::from(state.state_160),
                number(&output["state"]),
                "{name}"
            );
            assert_eq!(
                i64::from(arena.alien_brain.phase_546910),
                number(&output["phase"]),
                "{name}"
            );
            let settings = number(&input["settings_84"]) as u32
                | if host.bonus { COMPLETION_BONUS } else { 0 };
            assert_eq!(
                i64::from(settings),
                number(&output["settings_84"]),
                "{name}"
            );
            assert_eq!(i64::from(host.rng), number(&output["seed"]), "{name}");
            for (key, helper) in [("guide", guide), ("bug", bug)] {
                let Some(helper) = helper else { continue };
                let state = arena.selector90(helper).unwrap();
                assert_eq!(
                    (state.word_13c as u32 as i64, i64::from(state.byte_140)),
                    (
                        number(&output[key]["speed"]),
                        number(&output[key]["arrived"])
                    ),
                    "{name} {key}"
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 88);
}
