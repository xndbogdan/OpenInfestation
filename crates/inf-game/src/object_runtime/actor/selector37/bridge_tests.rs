//! The course bridge's update `00409A20` against Infest02
//! ([bridge_rise_native.json](../../../../../tests/fixtures/bridge_rise_native.json)):
//! from the third course death, its hold timer runs out over 8 updates
//! with sound 0x17, then 32 updates raise the strip's 140 cells toward the
//! ramp between its sampled ends, each with the commands 0x33 and 0x31.
//! Then water and dry bridges against the original update under Unicorn
//! ([selector37_update_native.json](../../../../../tests/fixtures/selector37_update_native.json)).

use inf_formats::GRANT_MASK_WORDS;
use serde_json::Value;

use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs,
    },
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
    test_worlds::offline_world,
};

struct Bridge<'a> {
    terrain: &'a mut TerrainRuntime,
    policy: ProcessTerrainPolicy,
    /// Settings +70 for 00450370.
    detail: u32,
    world_flags: u32,
    sounds: Vec<[u16; 3]>,
    commands: Vec<(u32, u8)>,
    status: u32,
    /// G+1C7 and G+1D4.
    doom: (u8, u32),
    /// Where 0040DCB0 asked the world factory for a scientist; none is made.
    evacuees: Vec<[u16; 3]>,
}

impl Selector37Host for Bridge<'_> {
    type Error = String;
    fn terrain(&mut self) -> BridgeTerrain<'_> {
        BridgeTerrain {
            terrain: self.terrain,
            policy: &mut self.policy,
            detail: self.detail,
            world_flags: Some(self.world_flags),
        }
    }
    fn sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        self.sounds.push(position);
        Ok(())
    }
    fn start_doom(&mut self, micros: u32) {
        self.doom = (1, micros);
    }
    fn player_command(
        &mut self,
        _: &mut ObjectArena,
        _: PlayerId,
        raw_id: u32,
        silent: u8,
    ) -> Result<(), String> {
        self.commands.push((raw_id, silent));
        Ok(())
    }
    fn status(&mut self, _: &mut ObjectArena, _: ObjectId, _: u32) -> Result<(), String> {
        self.status += 1;
        Ok(())
    }
    fn create_scientist(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, String> {
        self.evacuees.push(position);
        Ok(None)
    }
    fn flash(&mut self, _: &mut ObjectArena, _: [u16; 3]) -> Result<(), String> {
        Err("no scientist is made, so none flashes".into())
    }
}

fn number(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

#[test]
fn level1_bridge_rises_as_infest02s() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/bridge_rise_native.json"
    ))
    .unwrap();
    let (_resident, _assets, mut world) = offline_world(18, TerrainSpacing::Units128);
    let bridge = world.report.placements[106].unwrap();
    let world_flags = world.world_flags;
    let arena = &mut world.arena;
    let [x, _, z] = arena.object(bridge).unwrap().base.position_words.unwrap();
    let state = arena.selector37(bridge).unwrap();
    assert_eq!(
        (
            state.word_158,
            state.word_15c,
            state.byte_164,
            state.byte_165
        ),
        (3500, 300, 4, 3)
    );
    let mut host = Bridge {
        terrain: &mut world.terrain,
        policy: ProcessTerrainPolicy::supplied(TerrainSpacing::Units128),
        detail: 1,
        world_flags,
        sounds: Vec::new(),
        commands: Vec::new(),
        status: 0,
        doom: (0, 0),
        evacuees: Vec::new(),
    };
    let tick = |arena: &mut ObjectArena, host: &mut Bridge, delta| {
        arena.tick_selector37(bridge, delta, host).unwrap();
    };
    // The first update samples both ends; three links hold the bridge.
    tick(arena, &mut host, 0x8ca0);
    let state = arena.selector37(bridge).unwrap();
    let sampled: Vec<_> = fixture["sampled"]
        .as_array()
        .unwrap()
        .iter()
        .map(number)
        .collect();
    assert_eq!(
        [state.word_174.unwrap(), state.word_178.unwrap()].map(i64::from),
        [sampled[0], sampled[1]]
    );
    assert_eq!((state.word_16c, state.word_170), (1_000_000, 4_000_000));
    assert_eq!(host.terrain.sampling().spacing, TerrainSpacing::Units128);
    // The strip in the update's write order: 28 cells along, 5 across.
    let cells: Vec<[u16; 2]> = (0..3500)
        .step_by(0x80)
        .flat_map(|along: i32| {
            (-300..=300).step_by(0x80).map(move |across: i32| {
                [
                    x.wrapping_sub(along as u16).wrapping_sub(0x100),
                    z.wrapping_add(across as u16),
                ]
            })
        })
        .collect();
    assert_eq!(cells.len(), 140);
    let bytes = |terrain: &TerrainRuntime| -> Vec<i64> {
        cells
            .iter()
            .map(|&cell| i64::from(terrain.cell_at(cell).raw() as u8))
            .collect()
    };
    let before: Vec<_> = fixture["cells_before"]
        .as_array()
        .unwrap()
        .iter()
        .map(number)
        .collect();
    assert_eq!(bytes(host.terrain), before);
    // The third course death took the links to zero.
    arena.selector37_mut(bridge).unwrap().intermediate.byte_14c = 0;
    let heights = fixture["heights"].as_array().unwrap();
    let mut raised = 0;
    for (index, update) in fixture["updates"].as_array().unwrap().iter().enumerate() {
        let timers = |key: &str| -> (u32, u32) {
            let pair = update[key].as_array().unwrap();
            (number(&pair[0]) as u32, number(&pair[1]) as u32)
        };
        let state = arena.selector37(bridge).unwrap();
        assert_eq!(
            (state.word_16c, state.word_170),
            timers("before"),
            "update {index}"
        );
        let (sounds, commands) = (host.sounds.len(), host.commands.len());
        tick(arena, &mut host, number(&update["delta"]) as u32);
        let state = arena.selector37(bridge).unwrap();
        assert_eq!(
            (state.word_16c, state.word_170),
            timers("after"),
            "update {index}"
        );
        assert_eq!(
            host.sounds.len() - sounds,
            usize::from(update["sound"].as_bool().unwrap()),
            "update {index}"
        );
        let expected: Vec<(u32, u8)> = update["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (number(&pair[0]) as u32, number(&pair[1]) as u8))
            .collect();
        assert_eq!(
            &host.commands[commands..],
            expected.as_slice(),
            "update {index}"
        );
        if number(&update["writes"]) != 0 {
            let written: Vec<_> = heights[raised]
                .as_array()
                .unwrap()
                .iter()
                .map(|height| (number(height) >> 4) & 0xff)
                .collect();
            assert_eq!(bytes(host.terrain), written, "update {index}");
            raised += 1;
        }
    }
    assert_eq!(raised, 32);
    assert_eq!(
        host.sounds,
        [[
            x,
            arena.object(bridge).unwrap().base.position_words.unwrap()[1],
            z
        ]]
    );
    assert_eq!(host.status, 61);
}

const SINE: [u16; 4096] = [0; 4096];
/// The harness's markers: +174/+178 before any sample, G+1C7/G+1D4.
const END_MARKERS: [u32; 2] = [0x1357_2468, 0x2468_1357];
const WORLD_MARKERS: (u8, u32) = (0xa5, 0x5a5a_5a5a);

/// The harness's raw cell at `gx*512+gz`.
fn pattern(index: usize) -> u16 {
    let (gx, gz) = (index >> 9, index & 511);
    let low = (gx * 37 + gz * 11 + ((gx ^ gz) >> 2)) & 0xff;
    let high = (gx * 5 + gz * 3) & 0xff;
    (low | high << 8) as u16
}

fn grant_mask(grants: &Value) -> [u32; GRANT_MASK_WORDS] {
    let mut mask = [0; GRANT_MASK_WORDS];
    for grant in grants.as_array().unwrap() {
        let id = grant.as_u64().unwrap() as usize;
        mask[id >> 5] |= 1 << (id & 31);
    }
    mask
}

fn set_field(arena: &mut ObjectArena, bridge: ObjectId, key: &str, value: i64) {
    let state = arena.selector37_mut(bridge).unwrap();
    match key {
        "byte_14c" => state.intermediate.byte_14c = value as u8,
        "byte_14d" => state.intermediate.byte_14d = value as u8,
        "word_13c" => state.intermediate.word_13c = value as i32,
        "word_140" => state.intermediate.word_140 = value as i32,
        "word_170" => state.word_170 = value as u32,
        "word_17e" => state.word_17e = value as u16,
        other => panic!("{other}"),
    }
}

/// One harness sequence: the bridge, its first player and a patterned
/// terrain, then each update's fields, world words, plane, spacing, cells
/// and calls against the original's.
fn run_native_case(case: &Value) {
    let name = case["name"].as_str().unwrap();
    let get = |key: &str, default: i64| case.get(key).and_then(Value::as_i64).unwrap_or(default);
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let bridge = arena
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
        .unwrap();
    let position: Vec<u16> = case["position"]
        .as_array()
        .unwrap()
        .iter()
        .map(|word| word.as_u64().unwrap() as u16)
        .collect();
    arena.owned_mut(bridge).unwrap().state.base.position_words =
        Some([position[0], position[1], position[2]]);
    let player = arena.add_world_player(grant_mask(
        case.get("grants").unwrap_or(&Value::Array(Vec::new())),
    ));
    let state = arena.selector37_mut(bridge).unwrap();
    state.word_158 = get("word_158", 0x400) as u32;
    state.word_15c = get("word_15c", 0x100) as u32;
    state.word_160 = get("word_160", 0) as i32;
    state.byte_164 = get("byte_164", 4) as u8;
    state.byte_165 = get("byte_165", 0) as u8;
    state.byte_166 = get("byte_166", 0) as u8;
    state.byte_167 = get("byte_167", 0) as u8;
    state.word_16c = get("word_16c", 0) as u32;
    state.word_170 = get("word_170", i64::from(state.byte_164) * 1_000_000) as u32;
    state.word_17c = get("word_17c", 0xffff) as u16;
    state.word_17e = get("word_17e", 0) as u16;
    state.intermediate.word_13c = get("word_13c", 0) as i32;
    state.intermediate.word_140 = get("word_140", 0) as i32;
    state.intermediate.byte_14c = get("byte_14c", 0) as u8;
    state.intermediate.byte_14d = get("byte_14d", 0) as u8;
    // The harness zeroes the object: the spawn offset +146..+14A reads 0.
    state.intermediate.words_146_14a = Some([0, 0, 0]);
    let height_scale = match get("height_shift", 5) {
        5 => TerrainHeightScale::Units32,
        4 => TerrainHeightScale::Units16,
        other => panic!("{other}"),
    };
    let mut terrain = TerrainRuntime::from_raw_cells(
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale,
        },
        get("plane", 0) as i32,
        (0..512 * 512).map(pattern),
    );
    assert_eq!(i64::from(terrain.plane_shift()), get("plane_shift", 8));
    // Settings +70 = 0: 00450370 leaves the coarse pair, as the harness's.
    let mut host = Bridge {
        terrain: &mut terrain,
        policy: ProcessTerrainPolicy::supplied(TerrainSpacing::Units256),
        detail: 0,
        world_flags: 0,
        sounds: Vec::new(),
        commands: Vec::new(),
        status: 0,
        doom: WORLD_MARKERS,
        evacuees: Vec::new(),
    };
    let number = |value: &Value| value.as_u64().unwrap();
    for (index, update) in case["updates"].as_array().unwrap().iter().enumerate() {
        let label = format!("{name} update {index}");
        if let Some(set) = update.get("set") {
            for (key, value) in set.as_object().unwrap() {
                set_field(&mut arena, bridge, key, value.as_i64().unwrap());
            }
        }
        if let Some(grants) = update.get("grants") {
            arena.world_player_mut(player).unwrap().grants_9c = grant_mask(grants);
        }
        let (sounds, commands, statuses, evacuees) = (
            host.sounds.len(),
            host.commands.len(),
            host.status,
            host.evacuees.len(),
        );
        let result = arena.tick_selector37(bridge, number(&update["delta"]) as u32, &mut host);
        match (&update["stopped"], result) {
            (Value::Null, Ok(())) => {}
            // A divide error at the IDIV.
            (Value::Object(stop), Err(Selector37Error::Boundary { address, .. })) => {
                assert_eq!(stop["interrupt"], 0, "{label}");
                assert_eq!(u64::from(address), number(&stop["address"]), "{label}")
            }
            (stop, result) => panic!("{label}: {stop} against {result:?}"),
        }
        let state = arena.selector37(bridge).unwrap();
        let after = &update["after"];
        let ends = [state.word_174, state.word_178];
        assert_eq!(
            [
                u64::from(state.byte_168),
                u64::from(state.word_16c),
                u64::from(state.word_170),
                u64::from(ends[0].map_or(END_MARKERS[0], |end| end as u32)),
                u64::from(ends[1].map_or(END_MARKERS[1], |end| end as u32)),
                u64::from(state.intermediate.word_13c as u32),
                u64::from(state.intermediate.word_140 as u32),
            ],
            ["byte_168", "word_16c", "word_170", "word_174", "word_178", "word_13c", "word_140"]
                .map(|key| number(&after[key])),
            "{label}"
        );
        assert_eq!(
            [u64::from(host.doom.0), u64::from(host.doom.1)],
            [number(&update["world"][0]), number(&update["world"][1])],
            "{label}"
        );
        assert_eq!(
            u64::from(host.terrain.plane_base() as u32),
            number(&update["plane"]),
            "{label}"
        );
        assert_eq!(
            u64::from(host.terrain.sampling().spacing.units()),
            number(&update["spacing"]),
            "{label}"
        );
        let cells: Vec<[u64; 2]> = host
            .terrain
            .cells()
            .iter()
            .enumerate()
            .filter(|&(index, cell)| cell.raw() != pattern(index))
            .map(|(index, cell)| [index as u64, u64::from(cell.raw())])
            .collect();
        let expected: Vec<[u64; 2]> = update["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| [number(&pair[0]), number(&pair[1])])
            .collect();
        assert_eq!(cells, expected, "{label}");
        let events = update["events"].as_array().unwrap();
        let calls = |call: &'static str| events.iter().filter(move |event| event["call"] == call);
        for sound in calls("sound") {
            // WorldBridgeHost passes 0x17 at volume 0x10000, pitch 0x4000.
            assert_eq!(
                [&sound["sound"], &sound["volume"], &sound["pitch"]].map(number),
                [u64::from(BRIDGE_SOUND), 0x10000, 0x4000],
                "{label}"
            );
        }
        let expected: Vec<[u16; 3]> = calls("sound")
            .map(|sound| {
                let words: Vec<u16> = sound["position"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|word| number(word) as u16)
                    .collect();
                [words[0], words[1], words[2]]
            })
            .collect();
        assert_eq!(&host.sounds[sounds..], expected.as_slice(), "{label}");
        let expected: Vec<(u32, u8)> = calls("command")
            .map(|command| {
                (
                    number(&command["raw_id"]) as u32,
                    number(&command["silent"]) as u8,
                )
            })
            .collect();
        assert_eq!(&host.commands[commands..], expected.as_slice(), "{label}");
        for status in calls("status") {
            assert_eq!(status["delta"], update["delta"], "{label}");
        }
        assert_eq!(
            (host.status - statuses) as usize,
            calls("status").count(),
            "{label}"
        );
        // 0040DCB0 (00409ED9) with the staff and no vehicle: the port's
        // release asks for one scientist per staff member at the bridge's
        // +146 offset; the receipt records the call and returns.
        let released: u64 = calls("staff_release")
            .map(|call| {
                assert_eq!(number(&call["vehicle"]), 0, "{label}");
                number(&call["count"])
            })
            .sum();
        assert_eq!((host.evacuees.len() - evacuees) as u64, released, "{label}");
    }
}

/// The original update 00409A20 under Unicorn
/// (check_selector37_update.py): water bridges sample the
/// plane, restart the doom countdown while their released links run the
/// timer, and raise the plane by the elapsed share of +160 (shifted by 10,
/// over +164 thousands); dry strips in all four directions, wrapped, empty
/// and held; the grant gate, the staff and the restart; the staff release
/// 0040DCB0 at the rise's end; and the stops at the two divisions by zero
/// seconds.
#[test]
fn bridge_updates_match_the_original() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector37_update_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27);
    let updates: usize = cases
        .iter()
        .map(|case| case["updates"].as_array().unwrap().len())
        .sum();
    assert_eq!(updates, 287);
    for case in cases {
        run_native_case(case);
    }
}
