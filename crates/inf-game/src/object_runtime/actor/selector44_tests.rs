//! The falling rock against its original bodies under Unicorn
//! ([selector44_native.json](../../../tests/fixtures/selector44_native.json),
//! `check_selector44.py`): the update, the message, the
//! death and the glow. The draw's receipt is matched in the draw pass's
//! tests; the three contacts only retire, which the world's contact pass
//! owns.

use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorHandle, ActorMessage, ActorMessageReply, ActorRegistryConfig, ActorTerrainInputs,
    ObjectList, PlayerActorLookupContext,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};

const SINE: [u16; 4096] = [0; 4096];

fn fixture() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector44_native.json"
    ))
    .unwrap();
    fixture["cases"].as_array().unwrap().clone()
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

/// A rock as the case's input left it.
fn rock(input: &Value) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let rock = arena
        .allocate_selector44(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 0, 0]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    assert_eq!(*arena.selector44(rock).unwrap(), Selector44State::new());
    let get = |key: &str| input[key].as_u64().unwrap();
    let base = &mut arena.owned_mut(rock).unwrap().state.base;
    base.position_words = Some(words(&input["position"]));
    base.flags = get("flags") as u32;
    arena.actor_mut(rock).unwrap().words_8e_92 = words(&input["velocity"]);
    let state = arena.selector44_mut(rock).unwrap();
    state.started_138 = get("byte138") as u8;
    state.rising_139 = get("byte139") as u8;
    state.mask_13c = get("dword13c") as u32;
    (arena, rock)
}

fn output(arena: &ObjectArena, rock: ObjectId) -> [u64; 3] {
    let state = arena.selector44(rock).unwrap();
    [
        u64::from(state.started_138),
        u64::from(state.rising_139),
        u64::from(state.mask_13c),
    ]
}

fn expected_output(case: &Value) -> [u64; 3] {
    let output = &case["output"];
    ["byte138", "byte139", "dword13c"].map(|key| output[key].as_u64().unwrap())
}

#[derive(Default)]
struct Smoke {
    deltas: Vec<u32>,
}

impl Selector44Host for Smoke {
    type Error = String;
    fn status_visual(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        delta: u32,
    ) -> Result<(), String> {
        self.deltas.push(delta);
        Ok(())
    }
}

/// 00453ED0: the mask the first update picks (the physics records it as
/// its argument, and the original rereads +13C for it), then the physics
/// and the status visual with the update's delta.
#[test]
fn the_update_picks_the_original_mask() {
    let mut updates = 0;
    for case in fixture()
        .iter()
        .filter(|case| case["input"]["mode"] == "update")
    {
        let input = &case["input"];
        let (mut arena, rock) = rock(input);
        let delta = input["delta"].as_u64().unwrap() as u32;
        let mut smoke = Smoke::default();
        arena
            .tick_selector44(rock, delta, &SINE, &mut smoke)
            .unwrap();
        let output = output(&arena, rock);
        assert_eq!(output, expected_output(case), "{input}");
        let events = case["events"].as_array().unwrap();
        assert_eq!(events.len(), 2, "{input}");
        assert_eq!(events[0]["kind"], "physics");
        assert_eq!(events[0]["delta"].as_u64(), Some(u64::from(delta)));
        assert_eq!(events[0]["mask"].as_u64(), Some(output[2]), "{input}");
        assert_eq!(events[1]["kind"], "status");
        assert_eq!(smoke.deltas, [events[1]["delta"].as_u64().unwrap() as u32]);
        updates += 1;
    }
    assert_eq!(updates, 11);
}

/// 00454160: message 5 retires the rock; every message answers 0.
#[test]
fn message_5_retires_the_rock() {
    let mut messages = 0;
    for case in fixture()
        .iter()
        .filter(|case| case["input"]["mode"] == "message")
    {
        let input = &case["input"];
        let (mut arena, rock) = rock(input);
        let code = input["code"].as_u64().unwrap() as u32 as i32;
        let reply = arena
            .receive_actor_message(
                rock,
                ActorMessage {
                    sender: None,
                    distance_squared: 0,
                    code,
                    argument: 0x1234,
                },
                PlayerActorLookupContext { process: None },
                false,
            )
            .unwrap();
        let retires = case["events"].as_array().unwrap().len() == 1;
        assert_eq!(
            reply,
            if retires {
                ActorMessageReply::Retire
            } else {
                ActorMessageReply::Done(0)
            },
            "{input}"
        );
        assert_eq!(case["output"]["eax"], 0);
        messages += 1;
    }
    assert_eq!(messages, 5);
}

#[derive(Debug, PartialEq)]
enum Death {
    Blast {
        block: u32,
        position: [u16; 3],
        own_handle: bool,
    },
    Burst(ParticleBurst),
    Visual(u32),
}

#[derive(Default)]
struct Dying {
    calls: Vec<Death>,
}

impl Selector44DeathHost for Dying {
    type Error = String;
    fn blast(
        &mut self,
        arena: &mut ObjectArena,
        rock: ObjectId,
        block: u32,
        source: ActorHandle,
    ) -> Result<(), String> {
        self.calls.push(Death::Blast {
            block,
            position: arena.object(rock).unwrap().base.position_words.unwrap(),
            own_handle: source == arena.actor(rock).unwrap().handle,
        });
        Ok(())
    }
    fn burst(&mut self, _: &mut ObjectArena, burst: ParticleBurst) -> Result<(), String> {
        self.calls.push(Death::Burst(burst));
        Ok(())
    }
    fn visual(&mut self, _: &mut ObjectArena, _: ObjectId, resource: u32) -> Result<(), String> {
        self.calls.push(Death::Visual(resource));
        Ok(())
    }
}

/// 00453F30: both blasts (the world, the live position, its own handle and
/// flag 1) unless flag bit 31, the burst block, the two visuals through
/// +4C(0, resource, 0) and the disposal.
#[test]
fn the_death_matches_the_original() {
    let mut deaths = 0;
    for case in fixture()
        .iter()
        .filter(|case| case["input"]["mode"] == "death")
    {
        let input = &case["input"];
        let (mut arena, rock) = rock(input);
        let mut dying = Dying::default();
        arena.retire_selector44(rock, &mut dying).unwrap();
        let mut expected = Vec::new();
        let events = case["events"].as_array().unwrap();
        for event in &events[..events.len() - 1] {
            expected.push(match event["kind"].as_str().unwrap() {
                "radius" => {
                    assert_eq!(event["handle"], input["handle"]);
                    assert_eq!((&event["flag"], &event["world"]), (&1.into(), &true.into()));
                    Death::Blast {
                        block: event["table"].as_u64().unwrap() as u32,
                        position: words(&event["position"]),
                        own_handle: true,
                    }
                }
                "burst" => {
                    let w = |n: usize| event["words"][n].as_u64().unwrap() as u16;
                    let d = |n: usize| event["dwords"][n].as_u64().unwrap() as u32;
                    // Word 5 is the unwritten padding 0048A8B0 skips.
                    Death::Burst(ParticleBurst {
                        debris_count: w(0),
                        position: [w(1), w(2), w(3)],
                        placement_radius: w(4),
                        central_type: d(0),
                        debris_types: [d(1), d(2)],
                        speed: d(3),
                        flags: d(4),
                    })
                }
                "child" => {
                    assert_eq!(event["self"], true);
                    let args = &event["args"];
                    assert_eq!((&args[0], &args[2]), (&0.into(), &0.into()));
                    Death::Visual(args[1].as_u64().unwrap() as u32)
                }
                other => panic!("{other}"),
            });
        }
        assert_eq!(events.last().unwrap()["kind"], "dispose");
        assert_eq!(dying.calls, expected, "{input}");
        assert!(arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == rock));
        deaths += 1;
    }
    assert_eq!(deaths, 4);
}

/// 00454040 over a terrain whose cells all hold the case's raw byte: the
/// light's position, radius and colour, or none.
#[test]
fn the_glow_matches_the_original() {
    let (mut lights, mut lit) = (0, 0);
    for case in fixture()
        .iter()
        .filter(|case| case["input"]["mode"] == "light")
    {
        let input = &case["input"];
        let get = |key: &str| input[key].as_i64().unwrap();
        let height_scale = match get("height_shift") {
            5 => TerrainHeightScale::Units32,
            4 => TerrainHeightScale::Units16,
            other => panic!("{other}"),
        };
        assert_eq!(get("spacing_mask"), 0x7f);
        let terrain = TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale,
            },
            get("plane") as i32,
            std::iter::repeat((get("cell") & 0xff) as u16).take(512 * 512),
        );
        assert_eq!(i64::from(terrain.plane_shift()), get("plane_shift"));
        let (arena, rock) = rock(input);
        let glow = arena
            .selector44_glow::<()>(rock, &terrain, get("light") as u32)
            .unwrap();
        let expected = case["events"].as_array().unwrap().first().map(|event| {
            let color = event["color"].as_u64().unwrap() as u32;
            assert_eq!(color >> 24, get("light") as u32 >> 24, "{input}");
            let rgb = color.to_le_bytes();
            TerrainLightRequest {
                world_xz: [
                    event["x"].as_i64().unwrap() as i32,
                    event["z"].as_i64().unwrap() as i32,
                ],
                radius: event["distance"].as_i64().unwrap() as i32,
                rgb: [rgb[0] as i8, rgb[1] as i8, rgb[2] as i8],
            }
        });
        lit += usize::from(expected.is_some());
        assert_eq!(glow, expected, "{input}");
        lights += 1;
    }
    assert_eq!(lights, 84);
    assert!(lit > 0);
}
