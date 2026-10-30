use super::*;
use crate::object_runtime::actor::selector30::tests::{arena, request};
use crate::process_sound::{ProcessSoundRuntime, ProcessSoundVoiceRef};
use crate::ResidentRuntime;
use inf_formats::FrontendExecutable;
use serde_json::{json, Value};

fn u(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| u(&value[axis]) as u16)
}

fn retail() -> (Vec<u8>, ResidentRuntime) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe"))
        .expect("selector30 receipts require the supported local retail executable");
    let resident = ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    (bytes, resident)
}

struct NoVoices;
impl crate::process_sound::ProcessSoundVoiceTeardown for NoVoices {
    fn delete_voice(
        &mut self,
        _: ProcessSoundRef,
        _: ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        Ok(())
    }
}

struct Host<'a> {
    sounds: &'a mut ProcessSoundRuntime,
    resident: &'a ResidentRuntime,
    events: Vec<Value>,
    /// 0048B411's G+1D8 WORD; G+1DA is then 0x119.
    published: Option<u16>,
}

impl Selector30UpdateHost for Host<'_> {
    type Error = String;
    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
        volume: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, String> {
        let position = arena.object(actor).unwrap().base.position_words.unwrap();
        self.events.push(
            json!({"call": "create", "sound": 0x1a, "position": position,
            "volume": volume, "pitch": pitch, "flag": 0}),
        );
        self.sounds
            .create_persistent(self.resident, 0x1a, volume, pitch, 0, || {
                Ok::<_, std::convert::Infallible>(position)
            })
            .map_err(|error| format!("{error:?}"))
    }
    fn update_sound(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
        _: ProcessSoundRef,
        volume: u32,
        pitch: u32,
    ) -> Result<(), String> {
        let position = arena.object(actor).unwrap().base.position_words.unwrap();
        self.events
            .push(json!({"call": "update", "position": position,
            "volume": volume, "pitch": pitch}));
        Ok(())
    }
    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.events.push(json!({"call": "release"}));
        self.sounds
            .delete_record(reference, &mut NoVoices)
            .map_err(|error| format!("{error:?}"))
    }
    fn find_player(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<crate::object_runtime::PlayerId>, String> {
        Ok(None)
    }
    fn aliens_remain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<(), String> {
        Err("unreached".into())
    }
    fn aliens_cleared(&mut self) -> Result<(), String> {
        Err("unreached".into())
    }
    fn publish_gate_word(&mut self, word_164: u16) -> Result<(), String> {
        self.published = Some(word_164);
        Ok(())
    }
    fn missing_grant(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        _: u16,
    ) -> Result<(), String> {
        Err("unreached".into())
    }
    fn destination(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        _: u32,
    ) -> Result<(), String> {
        Err("unreached".into())
    }
    fn pass(&mut self, _: &mut ObjectArena, _: ObjectId, _: ObjectId) -> Result<(), String> {
        Err("unreached".into())
    }
}

/// 0048B3F0 over 38 supplied worlds: the scan's flag, state, team and +18E
/// filters, nearest selection, range edges, both colour halves across clock
/// wraps, the sound life cycle and the four typed-boundary branches.
#[test]
fn selector30_update_matches_native_gate_scan_light_and_sound() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/selector30_update_native.json"
    ))
    .unwrap();
    let (bytes, resident) = retail();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut arena = arena();
        let gate = arena
            .allocate_selector30_actor(request(Some(words(&case["gate_position"]))))
            .unwrap();
        arena.owned_mut(gate).unwrap().state.base.word_28 = u(&case["size"]) as u16;
        arena.owned_mut(gate).unwrap().state.base.flags = case["gate_flags"]
            .as_u64()
            .map_or(0x24, |flags| flags as u32);
        let basis: Vec<f64> = case["basis"].as_array().map_or_else(
            || vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            |values| values.iter().map(|value| value.as_f64().unwrap()).collect(),
        );
        let actor = arena.actor_mut(gate).unwrap();
        actor.rotating.basis.words[0] = 0;
        for (index, value) in basis.iter().enumerate() {
            actor.rotating.basis.words[1 + index] = (*value as f32).to_bits();
        }
        actor.word_b4 = case["team"].as_u64().map_or(1, |team| team as u32);
        let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
        let existing = case["sound"].as_bool().unwrap_or(false).then(|| {
            sounds
                .create_persistent(&resident, 0x1a, 1, 1, 0, || {
                    Ok::<_, std::convert::Infallible>([0; 3])
                })
                .unwrap()
        });
        let state = arena.selector30_mut(gate).unwrap();
        state.byte_166 = case["byte_166"].as_u64().unwrap_or(0) as u8;
        state.word_164 = case["word_164"].as_u64().unwrap_or(0) as u16;
        state.word_168 = case["word_168"].as_u64().unwrap_or(0) as u32;
        state.word_174 = case["word_174"].as_u64().unwrap_or(0x1122_3344) as u32;
        state.word_178 = 0x5555;
        state.word_17c = 0x1234;
        state.byte_18e = case["byte_18e"].as_u64().unwrap_or(0) as u8;
        state.intermediate.word_158 = Some(case["handle_158"].as_u64().unwrap_or(0) as u32);
        state.intermediate.sound_154 = existing;
        for spec in case["candidates"].as_array().unwrap() {
            let candidate = arena
                .allocate_actor(request(Some(words(&spec["position"]))))
                .unwrap();
            arena.owned_mut(candidate).unwrap().state.base.flags = u(&spec["flags"]);
            let actor = arena.actor_mut(candidate).unwrap();
            actor.word_b4 = spec["team"].as_u64().map_or(1, |team| team as u32);
            actor.word_c4 = spec["state"].as_u64().map_or(3, |state| state as u32);
        }
        let mut host = Host {
            sounds: &mut sounds,
            resident: &resident,
            events: Vec::new(),
            published: None,
        };
        let result = arena.tick_selector30(
            gate,
            Selector30TickRequest {
                animation_clock: case["clock"].as_u64().unwrap_or(0) as u32,
                process_null_handle: Some(ActorHandle::from_raw(
                    case["process_null"].as_u64().unwrap_or(0x77) as u32,
                )),
                network_active: case["network"].as_u64().unwrap_or(0) != 0,
                mission_elapsed_ms: Some(case["elapsed"].as_u64().unwrap_or(1000) as u32),
            },
            &mut host,
        );
        // The harness stopped at the near-gate tracking and the destination
        // notice, which the port now runs (the TTD receipts of the pass
        // cover them); nothing past the stop was recorded to compare.
        if matches!(case["stopped"].as_str(), Some("near_gate" | "message")) {
            assert!(result.is_ok(), "{name}: {result:?}");
            continue;
        }
        assert!(case["stopped"].is_null(), "{name}");
        result.unwrap_or_else(|error| panic!("{name}: {error:?}"));
        // G+1D8/G+1DA start as markers; 0048B411 writes the gate's +164 and
        // 0x119.
        let world: Vec<u64> = case["world"]
            .as_array()
            .unwrap()
            .iter()
            .map(|word| word.as_u64().unwrap())
            .collect();
        if world == [0xa5a5, 0x5a5a] {
            assert_eq!(host.published, None, "{name}");
        } else {
            assert_eq!(world[1], 0x119, "{name}");
            assert_eq!(host.published.map(u64::from), Some(world[0]), "{name}");
        }
        let native: Vec<&Value> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["call"] != "resolve")
            .collect();
        assert_eq!(host.events.len(), native.len(), "{name} events");
        for (port, native) in host.events.iter().zip(native) {
            for key in ["call", "sound", "position", "volume", "pitch", "flag"] {
                if !native[key].is_null() {
                    assert_eq!(port[key], native[key], "{name} {key}");
                }
            }
        }
        let after = &case["after"];
        let state = arena.selector30_actor(gate).unwrap();
        assert_eq!(
            state.intermediate.word_158,
            Some(u(&after["handle_158"])),
            "{name}"
        );
        assert_eq!(state.word_168, u(&after["word_168"]), "{name}");
        assert_eq!(state.word_174, u(&after["word_174"]), "{name}");
        assert_eq!(state.word_178, u(&after["word_178"]), "{name}");
        assert_eq!(u32::from(state.word_17c), u(&after["word_17c"]), "{name}");
        assert_eq!(
            state.intermediate.sound_154.is_some(),
            u(&after["sound"]) != 0,
            "{name}"
        );
    }
    assert_eq!(cases.len(), 42);
}

/// Records the gate's notices and passes on the real world.
struct GateHost<'a> {
    sounds: &'a mut ProcessSoundRuntime,
    resident: &'a ResidentRuntime,
    player: Option<crate::object_runtime::PlayerId>,
    calls: Vec<String>,
}

impl Selector30UpdateHost for GateHost<'_> {
    type Error = String;
    fn create_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        volume: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, String> {
        self.sounds
            .create_persistent(self.resident, 0x1a, volume, pitch, 0, || {
                Ok::<_, std::convert::Infallible>([0; 3])
            })
            .map_err(|error| format!("{error:?}"))
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Ok(())
    }
    fn release_sound(&mut self, _: ProcessSoundRef) -> Result<(), String> {
        Ok(())
    }
    fn find_player(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<crate::object_runtime::PlayerId>, String> {
        Ok(self.player)
    }
    fn aliens_remain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<(), String> {
        self.calls.push("aliens_remain".into());
        Ok(())
    }
    fn aliens_cleared(&mut self) -> Result<(), String> {
        self.calls.push("aliens_cleared".into());
        Ok(())
    }
    fn publish_gate_word(&mut self, word_164: u16) -> Result<(), String> {
        self.calls.push(format!("publish_gate_word {word_164}"));
        Ok(())
    }
    fn missing_grant(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        grant: u16,
    ) -> Result<(), String> {
        self.calls.push(format!("missing_grant {grant}"));
        Ok(())
    }
    fn destination(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        world: u32,
    ) -> Result<(), String> {
        self.calls.push(format!("destination {world}"));
        Ok(())
    }
    fn pass(
        &mut self,
        arena: &mut ObjectArena,
        gate: ObjectId,
        actor: ObjectId,
    ) -> Result<(), String> {
        let position = arena.object(actor).unwrap().base.position_words.unwrap();
        self.calls.push(format!("pass {gate:?} {position:?}"));
        Ok(())
    }
}

/// Level 1's gate to Terra Nova in Infest02 (world 18). At 4E8C3A the
/// vehicle at (11791, 26, 35889) is inside the ring, 58 in front of it: it
/// is tracked at +158 and, as the first find after two seconds, hears the
/// destination. At 4E9955, at (11678, 28, 35883), it is 55 behind the face:
/// it passes, and +158 returns to the process NULL; the scan does not track
/// it again from behind.
#[test]
fn level1_exit_gate_tracks_the_vehicle_then_passes_it() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let arena = &mut world.arena;
    let gate = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            arena
                .selector30_actor(actor)
                .is_ok_and(|state| state.word_15c == 2)
        })
        .expect("Level 1's gate to world 2");
    assert_eq!(
        arena.object(gate).unwrap().base.position_words,
        Some([11733, 65488, 35852])
    );
    assert_eq!(arena.object(gate).unwrap().base.word_28, 660);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let handle = arena.actor(vehicle).unwrap().handle.raw();
    let null = ActorHandle::from_raw(0);
    arena.selector30_mut(gate).unwrap().intermediate.word_158 = Some(null.raw());
    arena.selector30_mut(gate).unwrap().word_168 = 0;
    let request = Selector30TickRequest {
        animation_clock: 0,
        process_null_handle: Some(null),
        network_active: false,
        mission_elapsed_ms: Some(3000),
    };
    let player = crate::object_runtime::PlayerId::unissued(1);
    let mut host = GateHost {
        sounds: &mut sounds,
        resident: &resident,
        player: Some(player),
        calls: Vec::new(),
    };
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some([11791, 26, 35889]);
    arena.tick_selector30(gate, request, &mut host).unwrap();
    assert_eq!(
        arena.selector30_actor(gate).unwrap().intermediate.word_158,
        Some(handle)
    );
    assert_eq!(host.calls, ["destination 2"]);
    assert_eq!(arena.selector30_actor(gate).unwrap().word_168, 1);

    host.calls.clear();
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some([11678, 28, 35883]);
    arena.tick_selector30(gate, request, &mut host).unwrap();
    assert_eq!(host.calls, [format!("pass {gate:?} [11678, 28, 35883]")]);
    assert_eq!(
        arena.selector30_actor(gate).unwrap().intermediate.word_158,
        Some(null.raw())
    );
}
