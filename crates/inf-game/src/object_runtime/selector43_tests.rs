//! The timed spawner against the original update 00454310 under Unicorn
//! ([selector43_update_native.json](../../tests/fixtures/selector43_update_native.json),
//! `check_selector43_update.py`), and its commands.

use inf_formats::{InstanceCommandPayload, InstanceField};
use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, ObjectClassKind,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};

const SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn sine_table() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values
}

/// The harness's raw cell at gx*512+gz.
fn pattern(index: usize) -> u16 {
    let (gx, gz) = (index >> 9, index & 511);
    (((gx * 29 + gz * 13 + (gx ^ gz)) & 0xff) | ((gx * 7 + gz) & 0xff) << 8) as u16
}

/// Any actor stands in for the rock: the factory is the host's.
fn rock(arena: &mut ObjectArena) -> ObjectId {
    arena
        .allocate_selector37(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
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
        .unwrap()
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_u64().unwrap() as u16)
}

#[derive(Debug, PartialEq)]
enum Call {
    Eruption(u32),
    /// The rock's start and the RNG word at the factory call.
    Launch([u16; 3], u32),
}

struct Host {
    terrain: TerrainRuntime,
    rock: ObjectId,
    calls: Vec<Call>,
}

impl Selector43Host for Host {
    type Error = String;
    fn terrain(&self) -> &TerrainRuntime {
        &self.terrain
    }
    fn eruption(&mut self, _: &mut ObjectArena, argument: u32, _: &mut u32) -> Result<(), String> {
        self.calls.push(Call::Eruption(argument));
        Ok(())
    }
    fn launch(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        self.calls.push(Call::Launch(position, *rng));
        let actor = arena.actor_mut(self.rock).unwrap();
        actor.words_8e_92 = [0; 3];
        actor.words_94_9c = [0; 3];
        Ok(Some(self.rock))
    }
}

/// Every update's event and rock (start and the RNG at the factory call),
/// the rock's spin through its identity basis and its velocity, and the
/// timer, elapsed time, phase and RNG after it. The original raises the
/// event with no sender at radius 0 (every actor), and creates the rock
/// with the process NULL and no owner, which the production host does.
#[test]
fn spawner_update_matches_the_original() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/selector43_update_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    let sine = sine_table();
    let (mut updates, mut eruptions, mut launches) = (0, 0, 0);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let get = |key: &str| case[key].as_u64().unwrap() as u32;
        let mut arena = arena();
        let object = arena.allocate(ObjectClassKind::Selector43);
        let position = words(&case["position"]);
        arena.owned_mut(object).unwrap().state.base.position_words = Some(position);
        let state = arena.selector43_mut(object).unwrap();
        (state.on_44, state.off_48, state.spread_4c) = (get("on"), get("off"), get("spread"));
        (state.ring_50, state.byte_51) = (get("ring") as u8, get("byte51") as u8);
        (state.timer_54, state.elapsed_5c) = (get("timer"), get("elapsed"));
        state.phase_60 = get("phase") as u8;
        let spacing = match get("spacing_mask") {
            0x7f => TerrainSpacing::Units128,
            0xff => TerrainSpacing::Units256,
            other => panic!("{name}: {other:#x}"),
        };
        let height_scale = match get("height_shift") {
            5 => TerrainHeightScale::Units32,
            4 => TerrainHeightScale::Units16,
            other => panic!("{name}: {other}"),
        };
        let terrain = TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing,
                height_scale,
            },
            0,
            (0..512 * 512).map(pattern),
        );
        let rock = rock(&mut arena);
        let mut host = Host {
            terrain,
            rock,
            calls: Vec::new(),
        };
        let mut rng = get("rng");
        for (index, update) in case["updates"].as_array().unwrap().iter().enumerate() {
            let at = format!("{name} update {index}");
            let (mut expected, mut spin, mut velocity) = (Vec::new(), None, None);
            for event in update["events"].as_array().unwrap() {
                match event["kind"].as_str().unwrap() {
                    "event" => {
                        assert_eq!(
                            (&event["source"], &event["radius"], &event["event"]),
                            (&0.into(), &0.into(), &ERUPTION_EVENT.into()),
                            "{at}"
                        );
                        assert_eq!(words(&event["position"]), position, "{at}");
                        expected.push(Call::Eruption(event["argument"].as_u64().unwrap() as u32));
                        eruptions += 1;
                    }
                    "create" => {
                        assert_eq!(
                            (&event["resource"], &event["edx"]),
                            (&ROCK_RESOURCE.into(), &0.into()),
                            "{at}"
                        );
                        let args = serde_json::json!([0, u32::MAX, 0, case["null"]]);
                        assert_eq!(event["args"], args, "{at}");
                        expected.push(Call::Launch(
                            words(&event["position"]),
                            event["rng"].as_u64().unwrap() as u32,
                        ));
                        launches += 1;
                    }
                    "spin" => {
                        assert_eq!(event["child"], true, "{at}");
                        let vector = &event["vector"];
                        spin = Some(std::array::from_fn::<u32, 3, _>(|axis| {
                            vector[axis].as_u64().unwrap() as u32
                        }));
                    }
                    "motion" => {
                        assert_eq!(event["child"], true, "{at}");
                        assert_eq!(
                            (&event["first"], &event["position"]),
                            (&0.into(), &0.into())
                        );
                        velocity = Some(words(&event["velocity"]));
                    }
                    other => panic!("{at}: {other}"),
                }
            }
            arena
                .tick_selector43(object, get_delta(update), &sine, &mut rng, &mut host)
                .unwrap_or_else(|error| panic!("{at}: {error}"));
            assert_eq!(std::mem::take(&mut host.calls), expected, "{at}");
            if let Some(spin) = spin {
                assert_eq!(arena.actor(rock).unwrap().words_94_9c, spin, "{at}");
            }
            if let Some(velocity) = velocity {
                assert_eq!(arena.actor(rock).unwrap().words_8e_92, velocity, "{at}");
            }
            let state = arena.selector43(object).unwrap();
            assert_eq!(
                (state.timer_54, state.elapsed_5c, state.phase_60, rng),
                (
                    update["timer"].as_u64().unwrap() as u32,
                    update["elapsed"].as_u64().unwrap() as u32,
                    update["phase"].as_u64().unwrap() as u8,
                    update["rng"].as_u64().unwrap() as u32
                ),
                "{at}"
            );
            assert_eq!(state.dword_58, 0, "{at}");
            updates += 1;
        }
    }
    assert_eq!(updates, 713);
    assert!(eruptions > 0 && launches > 0, "{eruptions} {launches}");
}

fn get_delta(update: &Value) -> u32 {
    update["delta"].as_u64().unwrap() as u32
}

fn field<T>(value: T) -> InstanceField<T> {
    InstanceField {
        span: inf_formats::ByteSpan::new(0, 0),
        value,
    }
}

/// 00454220: the seconds become microseconds, the spread and ring are
/// stored and +51 is the DWORD's truth; the base's codes are not these.
#[test]
fn commands_set_the_phases_and_the_spread() {
    let mut arena = arena();
    let object = arena.allocate(ObjectClassKind::Selector43);
    assert_eq!(*arena.selector43(object).unwrap(), Selector43State::new());
    for payload in [
        InstanceCommandPayload::Selector43OnSeconds(field(4)),
        InstanceCommandPayload::Selector43OffSeconds(field(0xffff)),
        InstanceCommandPayload::Selector43Spread(field(0x1234)),
        InstanceCommandPayload::Selector43Ring(field(7)),
        InstanceCommandPayload::Selector43Byte51(field(0x100)),
    ] {
        assert!(arena.apply_selector43_command(object, &payload).unwrap());
    }
    let state = arena.selector43(object).unwrap();
    assert_eq!(
        (
            state.on_44,
            state.off_48,
            state.spread_4c,
            state.ring_50,
            state.byte_51
        ),
        (4_000_000, 0xffffu32.wrapping_mul(1_000_000), 0x1234, 7, 1)
    );
}
