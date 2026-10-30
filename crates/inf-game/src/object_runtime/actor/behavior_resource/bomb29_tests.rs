//! The Bomb against its original bodies under Unicorn
//! ([selector29_native.json](../../../../tests/fixtures/selector29_native.json),
//! `check_selector29.py`).

use inf_formats::{ByteSpan, InstanceField};
use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
    ObjectList,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};

const SINE: [u16; 4096] = [0; 4096];

fn cases(mode: &str) -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector29_native.json"
    ))
    .unwrap();
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["input"]["mode"] == mode)
        .cloned()
        .collect()
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

/// A Bomb as the case's input left it.
fn bomb(input: &Value) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let bomb = arena
        .allocate_behavior_resource(
            ActorConstructionRequest {
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
            },
            BOMB_SELECTOR,
        )
        .unwrap();
    // 00457FE0 over 00455560(2000).
    let state = arena.bomb29(bomb).unwrap();
    assert_eq!(
        (
            state.byte_138,
            state.word_13a,
            state.word_13c,
            state.dword_148
        ),
        (0, 2000, 0, 0x36)
    );
    assert_eq!(arena.bomb29_fuse_length(bomb).unwrap(), 2000);
    let get = |key: &str| input[key].as_u64().unwrap();
    let base = &mut arena.owned_mut(bomb).unwrap().state.base;
    base.position_words = Some(words(&input["position"]));
    base.flags = get("flags") as u32;
    arena.actor_mut(bomb).unwrap().owner_handle = ActorHandle::from_raw(get("dword88") as u32);
    let state = arena.bomb29_mut(bomb).unwrap();
    state.byte_138 = get("byte138") as u8;
    state.word_13a = get("word13a") as u16;
    state.word_13c = get("word13c") as u16;
    state.dword_148 = get("dword148") as u32;
    state.bytes_13e = Some([0x11; 10]);
    state.tail = Family455560Tail::Selector29 {
        dword_14c: get("dword14c") as u32,
    };
    (arena, bomb)
}

fn state(arena: &ObjectArena, bomb: ObjectId) -> [u64; 4] {
    let state = arena.bomb29(bomb).unwrap();
    [
        u64::from(state.byte_138),
        u64::from(state.word_13a),
        u64::from(state.word_13c),
        u64::from(arena.bomb29_fuse_length(bomb).unwrap()),
    ]
}

fn expected(case: &Value) -> [u64; 4] {
    ["byte138", "word13a", "word13c", "dword14c"].map(|key| case["output"][key].as_u64().unwrap())
}

#[derive(Debug, PartialEq)]
enum Call {
    Detonate(ActorHandle),
    Blast(ActorHandle),
    Explosion([u16; 3]),
    Visual(ActorChildResource),
    Particles(ParticleBurst),
}

#[derive(Default)]
struct Host {
    calls: Vec<Call>,
}

impl MissileLifecycleHost for Host {
    type Error = String;
    fn detonate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        source: ActorHandle,
        _: &mut u32,
    ) -> Result<(), String> {
        self.calls.push(Call::Detonate(source));
        Ok(())
    }
    fn burst(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: [u8; 10],
        owner: ActorHandle,
        _: &mut u32,
    ) -> Result<(), String> {
        self.calls.push(Call::Blast(owner));
        Ok(())
    }
    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        _: &mut u32,
    ) -> Result<(), String> {
        self.calls.push(Call::Explosion(position));
        Ok(())
    }
    fn flash(
        &mut self,
        _: &mut ObjectArena,
        _: [u16; 3],
        _: u32,
        _: &mut u32,
    ) -> Result<(), String> {
        Err("the Bomb flashes nothing".into())
    }
}

impl BombLifecycleHost for Host {
    fn visual(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        resource: ActorChildResource,
        _: &mut u32,
    ) -> Result<(), String> {
        self.calls.push(Call::Visual(resource));
        Ok(())
    }
    fn particles(
        &mut self,
        _: &mut ObjectArena,
        burst: ParticleBurst,
        _: &mut u32,
    ) -> Result<(), String> {
        self.calls.push(Call::Particles(burst));
        Ok(())
    }
}

fn view(code: i32, payload: InstanceCommandPayload) -> InstanceCommandView {
    InstanceCommandView {
        target: InstanceCommandTarget::Constructor(29),
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: code,
        },
        proven_method: Some(0x0045_8c40),
        payload_consumer: Some(0x0045_8c40),
        payload,
    }
}

/// 00458C40: code 16 stores the DWORD and its WORD; codes above refuse;
/// codes through 15 (signed) reach the shared commands, which take 0 and 3
/// without a read, take 13 without a first player, and refuse a negative
/// code.
#[test]
fn the_commands_match_the_original() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    let context = || ActorCommandContext {
        runtime: &runtime,
        first_player_grants: None,
        terrain: None,
    };
    let all = cases("command");
    assert_eq!(all.len(), 13);
    for case in all {
        let input = &case["input"];
        let (mut arena, bomb) = bomb(input);
        let code = input["args"][0].as_u64().unwrap() as u32 as i32;
        let value = input["payload"][0].as_u64().unwrap() as u32;
        let events = case["events"].as_array().unwrap();
        let shared = events
            .first()
            .is_some_and(|event| event["kind"] == "shared");
        assert_eq!(shared, code <= 15, "{input}");
        let payload = if code == 16 {
            InstanceCommandPayload::Selector29Fuse(InstanceField {
                span: ByteSpan { start: 4, end: 8 },
                value,
            })
        } else {
            InstanceCommandPayload::NoPayloadRead
        };
        if code == 15 {
            continue; // 0047ADA0 reads a WORD operand the receipt records.
        }
        let accepted = arena
            .apply_bomb29_command(bomb, &view(code, payload), context())
            .unwrap();
        if !shared {
            assert_eq!(u64::from(accepted), case["output"]["al"].as_u64().unwrap());
        } else {
            assert_eq!(accepted, (0..=15).contains(&code), "{input}");
        }
        assert_eq!(state(&arena, bomb), expected(&case), "{input}");
    }
}

/// 00458CA0's 00479DD0: the height of the ground under it (00450A30) over a
/// terrain whose cells all hold the case's raw byte.
#[test]
fn the_initializer_drops_it_onto_the_ground() {
    let all = cases("init");
    assert_eq!(all.len(), 4);
    for case in all {
        let input = &case["input"];
        let terrain_input = &input["terrain"];
        let spacing = match terrain_input["spacing_mask"].as_u64().unwrap() {
            0x7f => TerrainSpacing::Units128,
            0xff => TerrainSpacing::Units256,
            other => panic!("{other}"),
        };
        let height_scale = match terrain_input["height_shift"].as_u64().unwrap() {
            5 => TerrainHeightScale::Units32,
            4 => TerrainHeightScale::Units16,
            other => panic!("{other}"),
        };
        let cell = terrain_input["cell"].as_u64().unwrap() as u16 & 0xff;
        let terrain = TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing,
                height_scale,
            },
            0,
            std::iter::repeat(cell).take(512 * 512),
        );
        let kinds: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["base_init", "copy"]);
        let (mut arena, bomb) = bomb(input);
        arena.drop_bomb29(bomb, &terrain).unwrap();
        assert_eq!(
            arena.object(bomb).unwrap().base.position_words,
            Some(words(&case["output"]["position"])),
            "{input}"
        );
    }
}

/// 00458C80 with the real fuse 004555B0: only an armed Bomb counts down,
/// and at zero it goes to state 0 with its own handle.
#[test]
fn the_update_runs_the_fuse_only_when_armed() {
    let all = cases("update");
    assert_eq!(all.len(), 10);
    for case in all {
        let input = &case["input"];
        let (mut arena, bomb) = bomb(input);
        let delta = input["args"][0].as_u64().unwrap() as u32;
        let mut host = Host::default();
        arena.tick_bomb29(bomb, delta, &mut 0, &mut host).unwrap();
        let own = arena.actor(bomb).unwrap().handle;
        let calls: Vec<Call> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| {
                assert_eq!(event["kind"], "state");
                assert_eq!(event["args"][0], 0);
                assert_eq!(event["args"][1], input["handle"]);
                Call::Detonate(own)
            })
            .collect();
        assert_eq!(host.calls, calls, "{input}");
        assert_eq!(state(&arena, bomb), expected(&case), "{input}");
    }
}

/// 00458CC0's own part: a nonzero low byte disarms it, refills the fuse and
/// names the holder its owner, with hint 0x1A; zero hints 0x19.
#[test]
fn the_capture_disarms_and_hints() {
    let all = cases("capture");
    assert_eq!(all.len(), 4);
    for case in all {
        let input = &case["input"];
        let (mut arena, bomb) = bomb(input);
        let args = &input["args"];
        let operation = args[0].as_u64().unwrap() as u32;
        let holder = ActorHandle::from_raw(args[1].as_u64().unwrap() as u32);
        let hint = arena.bomb29_capture(bomb, operation, holder).unwrap();
        let events = case["events"].as_array().unwrap();
        assert_eq!(events[0]["kind"], "first_player");
        assert_eq!(events[1]["player"], true);
        assert_eq!(events[1]["args"], serde_json::json!([hint, 0]), "{input}");
        assert_eq!(events[2]["kind"], "capture");
        assert_eq!(events[2]["args"], *args);
        assert_eq!(state(&arena, bomb), expected(&case), "{input}");
        assert_eq!(
            u64::from(arena.actor(bomb).unwrap().owner_handle.raw()),
            case["output"]["dword88"].as_u64().unwrap()
        );
    }
}

/// 00458010: 9 arms it; 10 arms it from its live owner; flag 0x80 keeps it
/// unarmed; a 10 from no sender or a dead one disposes of it online.
#[test]
fn the_messages_arm_it() {
    let all = cases("message");
    assert_eq!(all.len(), 13);
    for case in all {
        let input = &case["input"];
        let (mut arena, bomb) = bomb(input);
        let sender = if input["sender"].is_null() {
            None
        } else {
            let sender = arena
                .allocate_behavior_resource(
                    ActorConstructionRequest {
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
                    },
                    BOMB_SELECTOR,
                )
                .unwrap();
            arena.actor_mut(sender).unwrap().word_c4 =
                input["sender"]["state"].as_u64().unwrap() as u32;
            // The sender's handle is its owner's when the case names the
            // Bomb's owner +88.
            let handle = arena.actor(sender).unwrap().handle;
            let owner = if input["sender"]["handle"] == input["dword88"] {
                handle
            } else {
                ActorHandle::from_raw(handle.raw() ^ 1)
            };
            arena.actor_mut(bomb).unwrap().owner_handle = owner;
            Some(sender)
        };
        let code = input["args"][2].as_u64().unwrap() as u32 as i32;
        let network = input["network"].as_u64().unwrap() != 0;
        let reply = arena.bomb29_message(bomb, sender, code, network).unwrap();
        let disposed = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "dispose");
        assert_eq!(
            reply,
            if disposed {
                Bomb29Message::Dispose
            } else {
                Bomb29Message::None
            },
            "{input}"
        );
        assert_eq!(case["output"]["eax"], 0);
        let state = state(&arena, bomb);
        assert_eq!(state[..2], expected(&case)[..2], "{input}");
    }
}

/// 00458F60: state 0 with its own handle.
#[test]
fn the_death_goes_to_state_zero() {
    let all = cases("death");
    assert_eq!(all.len(), 2);
    for case in all {
        let input = &case["input"];
        let (mut arena, bomb) = bomb(input);
        let mut host = Host::default();
        arena.bomb29_state_one(bomb, &mut 0, &mut host).unwrap();
        let own = arena.actor(bomb).unwrap().handle;
        assert_eq!(host.calls, [Call::Detonate(own)]);
        assert_eq!(case["events"][0]["args"][1], input["handle"]);
    }
}

/// 00458DA0: the visual +148 through +4C (its WORD, the register's high
/// half residue), the burst at its position, then 00455680's blast from the
/// owner, explosion and disposal.
#[test]
fn state_zero_builds_its_visual_and_bursts() {
    let all = cases("b0");
    assert_eq!(all.len(), 3);
    for case in all {
        let input = &case["input"];
        let (mut arena, bomb) = bomb(input);
        let mut host = Host::default();
        arena.bomb29_state_zero(bomb, &mut 0, &mut host).unwrap();
        let events = case["events"].as_array().unwrap();
        let resource = events[0]["args"][1].as_u64().unwrap() as u32;
        let burst = &events[1];
        let w = |n: usize| burst["words"][n].as_u64().unwrap() as u16;
        let d = |n: usize| burst["dwords"][n].as_u64().unwrap() as u32;
        let position = words(&input["position"]);
        let owner = arena.actor(bomb).unwrap().owner_handle;
        assert_eq!(events[2]["kind"], "detonate");
        assert_eq!(
            host.calls,
            [
                Call::Visual(ActorChildResource::RegisterLowWord(resource as u16)),
                Call::Particles(ParticleBurst {
                    debris_count: w(0),
                    position: [w(1), w(2), w(3)],
                    placement_radius: w(4),
                    central_type: d(0),
                    debris_types: [d(1), d(2)],
                    speed: d(3),
                    flags: d(4),
                }),
                Call::Blast(owner),
                Call::Explosion(position),
            ],
            "{input}"
        );
        assert!(arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == bomb));
    }
}
