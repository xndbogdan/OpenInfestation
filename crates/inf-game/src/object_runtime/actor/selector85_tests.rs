//! The brain against its original bodies under Unicorn
//! ([selector85_native.json](../../../tests/fixtures/selector85_native.json),
//! `check_selector85.py`).

use serde_json::{json, Value};

use super::*;
use crate::{
    object_runtime::{
        ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs, ActorTerrainSurface,
        DerivedActorConstructionRequest, FollowTargetRequest,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, GRANT_MASK_WORDS,
    TERRAIN_ENTRY_FIXED_BYTES,
};

/// The receipt's handle for the brain (`check_selector85.py`'s `HANDLE`).
const HANDLE: u32 = 0x00b8_a1e5;
const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn cases(mode: &str) -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector85_native.json"
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

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn numbers<const N: usize>(value: &Value) -> [u32; N] {
    std::array::from_fn(|index| number(&value[index]))
}

fn retail_sine() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    inf_formats::parse_frontend_sine_table(executable.data(), 0x53df00)
        .unwrap()
        .values
}

fn request(sine: &[u16; 4096]) -> ActorConstructionRequest<'_> {
    ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0, 0, 0]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: sine,
        },
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

/// A vehicle (selector 2), built over a flat terrain.
fn vehicle(arena: &mut ObjectArena) -> ObjectId {
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    let terrain = TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let mut actor = request(&UNUSED_SINE);
    actor.terrain.terrain = Some(ActorTerrainSurface {
        terrain: &terrain,
        plane_height_fixed: 0,
    });
    actor.terrain.world_flags = Some(0);
    arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor,
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap()
}

#[derive(Default)]
struct Host {
    rng: u32,
    plane: i32,
    shift: u32,
    shake: bool,
    events: Vec<Value>,
}

impl Selector62Host for Host {
    type Error = String;
    fn process_null(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
    fn settings_7c(&self) -> u32 {
        0
    }
    fn create_ribbon(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<ObjectId>, String> {
        Err("no ribbons here".into())
    }
    fn extend_ribbon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: [u16; 3],
    ) -> Result<(), String> {
        Ok(())
    }
    fn spawn(&mut self, _: &mut ObjectArena, _: u32, _: [u16; 3]) -> Result<(), String> {
        Ok(())
    }
    fn blast(
        &mut self,
        _: &mut ObjectArena,
        _: u32,
        _: [u16; 3],
        _: ActorHandle,
    ) -> Result<(), String> {
        Ok(())
    }
    fn flash(&mut self, _: &mut ObjectArena, _: ObjectId, _: u32) -> Result<(), String> {
        Ok(())
    }
}

impl Selector85Host for Host {
    fn rng(&mut self) -> &mut u32 {
        &mut self.rng
    }
    fn spark(
        &mut self,
        _: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
    ) -> Result<(), String> {
        self.events.push(
            json!({"kind": "spark", "point": point, "velocity": velocity,
            "owner": owner.raw(), "seed": self.rng}),
        );
        Ok(())
    }
    fn plane(&mut self) -> (i32, u32) {
        (self.plane, self.shift)
    }
    fn set_plane(&mut self, plane: i32) {
        self.plane = plane;
    }
    fn pickup(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
        amount: i16,
    ) -> Result<(), String> {
        let quiet = arena.player_message(player).unwrap().suppression_byte();
        self.events
            .push(json!({"kind": "pickup", "args": [id, amount], "quiet": quiet}));
        Ok(())
    }
    fn change_state(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: ActorStateChangeRequest,
    ) -> Result<(), String> {
        self.events.push(json!({"kind": "lifecycle",
            "args": [request.argument, request.source.raw(), request.target_state]}));
        Ok(())
    }
    fn shake(&mut self) {
        self.shake = true;
    }
}

fn brain(arena: &mut ObjectArena) -> ObjectId {
    arena.allocate_selector85(request(&UNUSED_SINE)).unwrap()
}

/// The receipt's events the port's host sees, with the brain's handle for
/// the receipt's.
fn native_events(case: &Value, handle: u32) -> Vec<Value> {
    let own = |value: &Value| {
        let raw = number(value);
        json!(if raw == HANDLE { handle } else { raw })
    };
    case["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| match event["kind"].as_str().unwrap() {
            "spark" => {
                assert_eq!(
                    (number(&event["selector"]), number(&event["flags"])),
                    (1, 0)
                );
                Some(
                    json!({"kind": "spark", "point": event["point"], "velocity": event["velocity"],
                    "owner": own(&event["owner"]), "seed": event["seed"]}),
                )
            }
            "pickup" => {
                assert_eq!(event["player"], true);
                Some(json!({"kind": "pickup", "args": event["args"], "quiet": event["quiet"]}))
            }
            "lifecycle" => {
                assert_eq!(event["self"], true);
                let args = &event["args"];
                Some(json!({"kind": "lifecycle",
                    "args": [number(&args[0]), own(&args[1]), number(&args[2])]}))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_constructor_matches_00405f10() {
    let cases = cases("ctor");
    assert_eq!(cases.len(), 2);
    for case in cases {
        let (input, output) = (&case["input"], &case["output"]);
        let mut arena = arena();
        *arena.alien_brain_words_mut() = AlienBrainWords {
            phase_546910: number(&input["phase"]),
            restart_546914: number(&input["restart"]),
        };
        let brain = brain(&mut arena);
        let state = arena.selector85(brain).unwrap();
        let words: [u32; 6] = numbers(&output["words"]);
        assert_eq!(
            (
                state.swing_178,
                state.clock_17c,
                state.dword_180,
                state.state_184,
                state.speeds_188,
                state.registers_190.map(u32::from),
            ),
            (
                number(&output["swing"]),
                number(&output["clock"]),
                number(&output["dword_180"]),
                number(&output["state"]),
                numbers(&output["speeds"]),
                [words[0], words[1], words[2], words[3], words[4]],
            ),
            "{input}"
        );
        // +19A keeps what the heap held.
        assert_eq!(words[5], number(&input["words"][5]));
        assert_eq!(state.plane_19a, None);
        let words = arena.alien_brain_words();
        assert_eq!(
            (words.phase_546910, words.restart_546914),
            (number(&output["phase"]), number(&output["restart"]))
        );
        assert_eq!(arena.selector62(brain).unwrap().may_target_160, 1);
    }
}

#[test]
fn the_registers_match_00406630() {
    let cases = cases("registers");
    assert_eq!(cases.len(), 7);
    for case in cases {
        let input = &case["input"];
        let words: [u32; 6] = numbers(&input["words"]);
        let state = Selector85State {
            swing_178: 0,
            clock_17c: 0,
            dword_180: 0,
            state_184: 0,
            speeds_188: numbers(&input["speeds"]),
            registers_190: std::array::from_fn(|index| words[index] as u16),
            plane_19a: Some(words[5] as u16),
        };
        assert_eq!(
            state.model_value(number(&input["args"][1])).map(u32::from),
            Some(number(&case["output"]["ax"])),
            "{input}"
        );
    }
    assert_eq!(
        Selector85State {
            plane_19a: None,
            ..arena_brain_state()
        }
        .model_value(6),
        None
    );
    assert_eq!(arena_brain_state().model_value(7), None);
}

fn arena_brain_state() -> Selector85State {
    let mut arena = arena();
    let brain = brain(&mut arena);
    arena.selector85(brain).unwrap().clone()
}

#[test]
fn the_update_matches_00405fc0() {
    let sine = retail_sine();
    let cases = cases("update");
    assert_eq!(cases.len(), 64);
    let mut kinds = std::collections::BTreeMap::new();
    for case in cases {
        let (input, output) = (&case["input"], &case["output"]);
        let mut arena = arena();
        let brain = brain(&mut arena);
        let handle = arena.actor(brain).unwrap().handle.raw();
        let words: [u32; 6] = numbers(&input["words"]);
        *arena.selector85_mut(brain).unwrap() = Selector85State {
            swing_178: number(&input["swing"]),
            clock_17c: number(&input["clock"]),
            dword_180: 0,
            state_184: number(&input["state"]),
            speeds_188: numbers(&input["speeds"]),
            registers_190: std::array::from_fn(|index| words[index] as u16),
            plane_19a: Some(words[5] as u16),
        };
        let base = &mut arena.owned_mut(brain).unwrap().state.base;
        base.word_28 = number(&input["radius"]) as u16;
        base.position_words = Some(numbers::<3>(&input["position"]).map(|word| word as u16));
        // The receipt supplies the attack's +138 after 00464050; the port
        // runs the attack, from a state that leaves the same +138.
        let attack = arena.selector62_mut(brain).unwrap();
        attack.may_target_160 = number(&input["may_target"]) as u8;
        attack.state_138 = number(&input["attack"]);
        if case["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "attack")
        {
            attack.state_138 = 2;
            attack.point_140 = Some([0x1000, 0x200, 0x3000]);
            attack.blasts_158 = 4;
            if number(&input["attack_after"]) == 3 {
                attack.clock_13c = 0;
            } else {
                attack.clock_13c = 1_000_000;
                attack.blast_154 = Some(0);
            }
        }
        *arena.alien_brain_words_mut() = AlienBrainWords {
            phase_546910: number(&input["phase"]),
            restart_546914: number(&input["restart"]),
        };
        arena.capture_nodes.aliens_547620 = number(&input["aliens"]);
        // The first player, their unit and its target.
        let player = (input["player"] == true).then(|| {
            let player = arena.add_world_player([0; GRANT_MASK_WORDS]);
            let recipient = arena.player_message(player).unwrap().identity();
            arena
                .apply_player_message(PlayerMessageRequest {
                    recipient,
                    operation: PlayerMessageOperation::Enable,
                })
                .unwrap();
            player
        });
        let unit = match input["unit"].as_str() {
            Some("vehicle") => Some(vehicle(&mut arena)),
            Some(_) => Some(arena.allocate_actor(request(&UNUSED_SINE)).unwrap()),
            None => None,
        };
        if let (Some(player), Some(unit)) = (player, unit) {
            let handle = arena.actor(unit).unwrap().handle;
            arena.world_player_mut(player).unwrap().handle_d4 = handle;
        }
        if let (Some(unit), Some(lock)) = (unit, input["lock"].as_u64()) {
            let target = if lock as u32 == HANDLE {
                brain
            } else {
                arena.allocate_actor(request(&UNUSED_SINE)).unwrap()
            };
            let target = arena.actor(target).unwrap().handle;
            let target = arena
                .follow_target_resource(FollowTargetRequest::resolve_actor(target))
                .unwrap();
            if let Ok(derived) = arena.derived_actor_mut(unit) {
                derived.target_148 = target;
            }
        }
        let mut host = Host {
            rng: number(&input["seed"]),
            plane: number(&input["plane"]) as i32,
            shift: number(&input["shift"]),
            ..Host::default()
        };
        arena
            .tick_selector85(brain, number(&input["args"][0]), &sine, &mut host)
            .unwrap();
        let state = arena.selector85(brain).unwrap();
        let attack = arena.selector62(brain).unwrap();
        let words = arena.alien_brain_words();
        let mut registers = state.registers_190.map(u32::from).to_vec();
        registers.push(u32::from(state.plane_19a.unwrap()));
        assert_eq!(
            (
                (state.state_184, state.clock_17c, state.swing_178),
                (state.speeds_188, registers),
                (attack.state_138, u32::from(attack.may_target_160)),
                (words.phase_546910, words.restart_546914),
                (u32::from(host.shake), host.plane as u32, host.rng),
            ),
            (
                (
                    number(&output["state"]),
                    number(&output["clock"]),
                    number(&output["swing"])
                ),
                (
                    numbers(&output["speeds"]),
                    numbers::<6>(&output["words"]).to_vec()
                ),
                (number(&output["attack"]), number(&output["may_target"])),
                (number(&output["phase"]), number(&output["restart"])),
                (
                    number(&output["shake"]),
                    number(&output["plane"]),
                    number(&output["seed"])
                ),
            ),
            "{input}"
        );
        assert_eq!(host.events, native_events(&case, handle), "{input}");
        if let Some(player) = player {
            let player_state = arena.world_player(player).unwrap();
            assert_eq!(
                (
                    u32::from(player_state.byte_58.unwrap()),
                    u32::from(arena.player_message(player).unwrap().suppression_byte())
                ),
                (number(&output["player_58"]), number(&output["player_89"])),
                "{input}"
            );
        }
        for event in &host.events {
            *kinds
                .entry(event["kind"].as_str().unwrap().to_owned())
                .or_insert(0) += 1;
        }
    }
    // Every recorded service is reached: the sparks (both throws), the
    // grant (from state 1 and after restarts) and the lifecycle.
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        [
            ("lifecycle".to_owned(), 1),
            ("pickup".to_owned(), 5),
            ("spark".to_owned(), 14)
        ]
    );
}
