use std::path::Path;

use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
    FamilyUpdateEffect,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};
use inf_formats::native_basis::length_48f240;
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

const SINE: [u16; 4096] = [0; 4096];

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/powerball_native.json"
    ))
    .unwrap()
}

fn n(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn inverse_sine() -> [i16; 1024] {
    let bytes =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
            .expect("required original retail executable");
    let executable = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    crate::session_platform::world_actor_update::inverse_sine_table(executable).unwrap()
}

/// A terrain whose cells are zero but for the receipt's.
fn terrain(case: &Value) -> TerrainRuntime {
    let (spacing, shift, cells) = match case.as_array() {
        Some(parts) => (
            n(&parts[0]),
            n(&parts[1]),
            parts[2].as_array().unwrap().clone(),
        ),
        None => (256, 4, Vec::new()),
    };
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for cell in &cells {
        let (x, z, height) = (n(&cell[0]) as usize, n(&cell[1]) as usize, n(&cell[2]));
        bytes[TERRAIN_PREFIX_BYTES + (x * 512 + z) * 2] = height as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let source = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &source.entries[0],
        TerrainSampling {
            spacing: if spacing == 128 {
                TerrainSpacing::Units128
            } else {
                TerrainSpacing::Units256
            },
            height_scale: if shift == 5 {
                TerrainHeightScale::Units32
            } else {
                TerrainHeightScale::Units16
            },
        },
    )
}

/// The receipt's H0 pool: its count and named strings.
type Strings = Option<(u16, Vec<(u16, Vec<u8>)>)>;

/// Records the ball's sounds and lines; the H0 pool is the receipt's.
struct Recorder {
    terrain: TerrainRuntime,
    null: ActorHandle,
    calls: Vec<Value>,
    rng: u32,
    /// None, no pool.
    strings: Strings,
}

impl FamilyUpdateHost for Recorder {
    type Error = String;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(self.null)
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), String> {
        Err(format!("unreached {effect:?}"))
    }
}

impl BallHost for Recorder {
    fn ball_sound(&mut self, sound: u32, position: [u16; 3]) -> Result<(), String> {
        self.calls
            .push(serde_json::json!(["sound", sound, position]));
        Ok(())
    }
    fn ball_terrain(&self) -> &TerrainRuntime {
        &self.terrain
    }
    fn ball_text(&self, index: u16) -> Result<Option<Vec<u8>>, String> {
        let Some((count, named)) = &self.strings else {
            return Ok(None);
        };
        Ok((index <= *count).then(|| {
            named.iter().find(|(slot, _)| *slot == index).map_or_else(
                || format!("S{index:03X}").into_bytes(),
                |(_, text)| text.clone(),
            )
        }))
    }
    fn ball_runtime(&self) -> &crate::ResidentRuntime {
        unreachable!("no model refresh here")
    }
    fn ball_line(&mut self, _: &mut ObjectArena, text: Vec<u8>) -> Result<(), String> {
        let hex: String = text.iter().map(|byte| format!("{byte:02x}")).collect();
        self.calls.push(serde_json::json!(["line", hex, 0]));
        Ok(())
    }
}

/// A ball with the receipt's fields, its own handle aside.
fn ball(case: &Value, null: ActorHandle) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: null,
    })
    .unwrap();
    let ball = arena
        .allocate_selector59(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
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
    arena.start_ball(ball, null).unwrap();
    let words = |key: &str| -> [u16; 3] {
        let v = case[key].as_array().unwrap();
        std::array::from_fn(|i| n(&v[i]) as u16)
    };
    let base = &mut arena.owned_mut(ball).unwrap().state.base;
    base.flags = case["flags"].as_i64().unwrap_or(0) as u32;
    base.position_words = Some(words("position"));
    arena.actor_mut(ball).unwrap().words_8e_92 = words("velocity");
    let state = arena.ball_mut(ball).unwrap();
    let handle = |key: &str| ActorHandle::from_raw(n(&case[key]) as u32);
    state.state_148 = n(&case["state"]) as u32;
    state.holder_14c = handle("holder");
    state.last_150 = handle("last");
    state.scorer_154 = handle("scorer");
    state.loose_158 = n(&case["loose"]) as i32;
    state.angles_15c = [0, 1].map(|i| Some(n(&case["angles"][i]) as i16));
    state.home_160 = Some(words("home"));
    state.carry_16c = n(&case["carry"]) as i32;
    state.threshold_170 = n(&case["threshold"]) as i32;
    state.sequence_174 = n(&case["sequence"]) as u16;
    state.throw_176 = [0, 1, 2].map(|i| n(&case["throw"][i]) as i16);
    state.throw_17c = n(&case["throw_seq"]) as u16;
    state.lock_17e = n(&case["lock"]) as i16;
    state.wait_180 = n(&case["wait"]) as i16;
    (arena, ball)
}

/// The receipt's fields after, against the ball's; the angles only where
/// the port knows them.
fn check(arena: &ObjectArena, ball: ObjectId, after: &Value, label: &str) {
    let state = arena.ball(ball).unwrap();
    let base = &arena.object(ball).unwrap().base;
    let raw = |key: &str| n(&after[key]) as u32;
    assert_eq!(state.state_148, raw("state"), "{label} state");
    assert_eq!(state.holder_14c.raw(), raw("holder"), "{label} holder");
    assert_eq!(state.last_150.raw(), raw("last"), "{label} last");
    assert_eq!(state.scorer_154.raw(), raw("scorer"), "{label} scorer");
    assert_eq!(state.loose_158, n(&after["loose"]) as i32, "{label} loose");
    for i in 0..2 {
        if let Some(angle) = state.angles_15c[i] {
            assert_eq!(
                i64::from(angle),
                n(&after["angles"][i]),
                "{label} angle {i}"
            );
        }
    }
    assert_eq!(
        state.threshold_170,
        n(&after["threshold"]) as i32,
        "{label} threshold"
    );
    assert_eq!(
        state.sequence_174,
        raw("sequence") as u16,
        "{label} sequence"
    );
    let throw: [i16; 3] = std::array::from_fn(|i| n(&after["throw"][i]) as i16);
    assert_eq!(state.throw_176, throw, "{label} throw");
    assert_eq!(
        state.throw_17c,
        raw("throw_seq") as u16,
        "{label} throw sequence"
    );
    assert_eq!(state.lock_17e, n(&after["lock"]) as i16, "{label} lock");
    assert_eq!(state.wait_180, n(&after["wait"]) as i16, "{label} wait");
    let position: [u16; 3] = std::array::from_fn(|i| n(&after["position"][i]) as u16);
    assert_eq!(base.position_words, Some(position), "{label} position");
    let velocity: [u16; 3] = std::array::from_fn(|i| n(&after["velocity"][i]) as i16 as u16);
    assert_eq!(
        arena.actor(ball).unwrap().words_8e_92,
        velocity,
        "{label} velocity"
    );
}

fn sounds(host: &Recorder) -> Vec<Value> {
    host.calls.clone()
}

fn recorded_sounds(calls: &Value) -> Vec<Value> {
    calls
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| call[0] == "sound")
        .cloned()
        .collect()
}

fn recorded_sends(calls: &Value) -> Vec<u32> {
    calls
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| call[0] == "send")
        .map(|call| n(&call[2]) as u32)
        .collect()
}

/// `check_powerball.py`: 0048F240 and 0048F540 over edges and random pairs.
#[test]
fn lengths_and_angles_match_0048f240_and_0048f540() {
    let table = inverse_sine();
    let fixture = fixture();
    for case in fixture["cases"]["lengths"].as_array().unwrap() {
        let [x, y, length] = [0, 1, 2].map(|i| n(&case[i]));
        assert_eq!(
            i64::from(length_48f240(x as i32, y as i32)),
            length,
            "({x}, {y})"
        );
    }
    for case in fixture["cases"]["angles"].as_array().unwrap() {
        let [x, y, angle] = [0, 1, 2].map(|i| n(&case[i]));
        let ours = angle_48f540(x as i32, y as i32, &table).unwrap();
        assert_eq!(i64::from(ours), angle, "({x}, {y})");
    }
}

/// `check_powerball.py`: 00402A40 from every state into every state, local
/// and remote, with the kick-off's sounds, the strip's scan and the RNG.
#[test]
fn the_ball_changes_state_as_00402a40() {
    let fixture = fixture();
    let null = ActorHandle::from_raw(n(&fixture["null"]) as u32);
    for (index, case) in fixture["cases"]["states"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let (mut arena, ball) = ball(&case["ball"], null);
        arena.set_powerball_goals([0, 1].map(|i| n(&case["goals"][i]) as u8));
        let mut host = Recorder {
            terrain: terrain(&case["terrain"]),
            null,
            calls: Vec::new(),
            strings: None,
            rng: n(&case["seed"]) as u32,
        };
        arena
            .set_ball_state(
                ball,
                n(&case["target"]) as u32,
                ActorHandle::from_raw(n(&case["handle"]) as u32),
                &mut host,
            )
            .unwrap();
        check(&arena, ball, &case["after"], &format!("state case {index}"));
        assert_eq!(host.rng, n(&case["rng"]) as u32, "state case {index} rng");
        assert_eq!(
            sounds(&host),
            recorded_sounds(&case["calls"]),
            "state case {index} sounds"
        );
    }
}

/// `check_powerball.py`: 00403940 drops and throws, in every mode, from
/// each held state, with the jitter's RNG and a remote ball's message 44.
#[test]
fn the_ball_is_thrown_as_00403940() {
    let fixture = fixture();
    let null = ActorHandle::from_raw(n(&fixture["null"]) as u32);
    for (index, case) in fixture["cases"]["throws"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let (mut arena, ball) = ball(&case["ball"], null);
        arena.set_powerball_goals([1, 2]);
        let mut host = Recorder {
            terrain: terrain(&Value::Null),
            null,
            calls: Vec::new(),
            strings: None,
            rng: n(&case["seed"]) as u32,
        };
        let velocity = case["velocity"]
            .as_array()
            .map(|v| std::array::from_fn(|i| n(&v[i]) as i16));
        arena
            .throw_ball(
                ball,
                ActorHandle::from_raw(n(&case["holder"]) as u32),
                velocity,
                n(&case["mode"]) as u32,
                &mut host,
            )
            .unwrap();
        check(&arena, ball, &case["after"], &format!("throw case {index}"));
        assert_eq!(host.rng, n(&case["rng"]) as u32, "throw case {index} rng");
        let sends: Vec<u32> = arena
            .take_network_sends()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            sends,
            recorded_sends(&case["calls"]),
            "throw case {index} sends"
        );
    }
}

/// `check_powerball.py`: 00403AE0's record for every state, with the ball
/// pointer naming the ball, another or none, in every room.
#[test]
fn the_ball_sync_encodes_as_00403ae0() {
    let fixture = fixture();
    let null = ActorHandle::from_raw(n(&fixture["null"]) as u32);
    for (index, case) in fixture["cases"]["encodes"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let (arena, ball) = ball(&case["ball"], null);
        let pointer = case["pointer"].as_str().unwrap();
        let mut out = [0u8; 64];
        let length = encode_ball(
            arena.ball(ball).unwrap(),
            [0, 1].map(|i| n(&case["goals"][i]) as u8),
            pointer == "self",
            pointer != "none",
            &mut out,
            n(&case["room"]) as usize,
        );
        assert_eq!(i64::from(length), n(&case["length"]), "encode case {index}");
        let expected = hex(case["out"].as_str().unwrap());
        assert_eq!(
            &out[..expected.len()],
            &expected[..],
            "encode case {index} bytes"
        );
    }
}

/// `check_powerball.py`: 00403BC0's long records for every state.
#[test]
fn the_ball_sync_applies_as_00403bc0() {
    let fixture = fixture();
    let null = ActorHandle::from_raw(n(&fixture["null"]) as u32);
    for (index, case) in fixture["cases"]["receives"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let (mut arena, ball) = ball(&case["ball"], null);
        let mut host = Recorder {
            terrain: terrain(&Value::Null),
            null,
            calls: Vec::new(),
            strings: None,
            rng: 0,
        };
        let record = hex(case["record"].as_str().unwrap());
        let sync = read_ball_sync(&record, null).unwrap();
        if let Some(position) = arena.apply_ball_sync(ball, sync).unwrap() {
            host.ball_sound(SOUND_KICK_OFF, position).unwrap();
        }
        check(
            &arena,
            ball,
            &case["after"],
            &format!("receive case {index}"),
        );
        let goals: [u8; 2] = [0, 1].map(|i| n(&case["goals"][i]) as u8);
        assert_eq!(arena.powerball_goals(), goals, "receive case {index} goals");
        assert_eq!(
            sounds(&host),
            recorded_sounds(&case["calls"]),
            "receive case {index} sounds"
        );
    }
}

/// `check_powerball.py`: messages 43 and 44 written in every room
/// (00403D60, 00403DF0) and received for the ball held and not (00403DA0,
/// 00403E50).
#[test]
fn ball_messages_match_43_and_44() {
    let fixture = fixture();
    let null = ActorHandle::from_raw(n(&fixture["null"]) as u32);
    for (index, case) in fixture["cases"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let (mut arena, ball) = ball(&case["ball"], null);
        arena.powerball = Some(ball);
        let kind = n(&case["kind"]);
        let state = arena.ball(ball).unwrap().clone();
        let mut out = [0u8; 16];
        let length = crate::network::encoders::encode_ball(
            &mut out,
            n(&case["room"]) as u32,
            state.holder_14c.raw(),
            state.throw_17c,
            (kind == 44).then_some(state.throw_176),
        );
        assert_eq!(
            i64::from(length),
            n(&case["length"]),
            "message case {index}"
        );
        let encoded = hex(case["encoded"].as_str().unwrap());
        let room = n(&case["room"]);
        let written = match length {
            -1 if room <= 3 => 0,
            -1 if room <= 5 => 4,
            -1 => 6,
            length => length as usize,
        };
        assert_eq!(
            &out[..written],
            &encoded[..written],
            "message case {index} bytes"
        );
        let payload = hex(case["payload"].as_str().unwrap());
        let holder = ActorHandle::from_raw(u32::from_le_bytes(payload[0..4].try_into().unwrap()));
        let sequence = u16::from_le_bytes([payload[4], payload[5]]);
        let throw = (kind == 44).then(|| {
            std::array::from_fn(|i| i16::from_le_bytes([payload[6 + 2 * i], payload[7 + 2 * i]]))
        });
        let mut host = Recorder {
            terrain: terrain(&Value::Null),
            null,
            calls: Vec::new(),
            strings: None,
            rng: 0,
        };
        arena
            .receive_ball_message(holder, sequence, throw, &mut host)
            .unwrap();
        check(
            &arena,
            ball,
            &case["after"],
            &format!("message case {index}"),
        );
        let sends: Vec<u32> = arena
            .take_network_sends()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            sends,
            recorded_sends(&case["calls"]),
            "message case {index} sends"
        );
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// An actor holding `handle` (the registry slot it names), as the
/// receipts' scorers, vehicles and sources.
fn actor_at(arena: &mut ObjectArena, handle: u32) -> ObjectId {
    let actor = arena
        .allocate_selector59(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
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
    arena
        .reassign_actor_handle(actor, ActorHandle::from_raw(handle))
        .unwrap();
    actor
}

fn basis(value: &Value) -> inf_formats::native_basis::NativeBasis {
    inf_formats::native_basis::NativeBasis {
        words: std::array::from_fn(|i| n(&value[i]) as u32),
    }
}

fn receipt_strings(fixture: &Value) -> Strings {
    let named = fixture["strings"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(slot, text)| {
            (
                slot.parse().unwrap(),
                text.as_str().unwrap().as_bytes().to_vec(),
            )
        })
        .collect();
    Some((n(&fixture["string_count"]) as u16, named))
}

/// `check_powerball.py`: 00403F40 over a ball near and far, in front of the
/// goal, behind it and crossing, each side and limit, with a scorer that
/// resolves or not: the marks, the goal bytes, the ball scored, the
/// scorer's credit and the first player's lines.
#[test]
fn the_goal_scores_as_00403f40() {
    use crate::network::notices::NetworkGameHold;
    let fixture = fixture();
    let null = ActorHandle::from_raw(n(&fixture["null"]) as u32);
    for (index, case) in fixture["cases"]["goals"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let label = format!("goal case {index}");
        let (mut arena, ball) = ball(&case["ball"], null);
        arena.powerball = Some(ball);
        let bytes = |key: &str| [0, 1].map(|i| n(&case[key][i]) as u8);
        arena.powerball_goals = bytes("goals");
        arena.powerball_seen = bytes("seen");
        let input = &case["goal"];
        let goal = arena.allocate(crate::object_runtime::ObjectClassKind::Selector92);
        let base = &mut arena.owned_mut(goal).unwrap().state.base;
        base.word_28 = n(&input["size"]) as u16;
        base.position_words = Some(std::array::from_fn(|i| n(&input["position"][i]) as u16));
        arena.race_object_mut(goal).unwrap().rotating.basis = basis(&input["basis"]);
        let state = arena.goal_state_mut(goal).unwrap();
        state.side_6c = n(&input["side"]) as i32;
        state.points_70 = n(&input["points"]) as u32;
        [state.crossed_74, state.crossed_75] = [0, 1].map(|i| n(&input["marks"][i]) as u8);
        if case["resolves"].as_bool().unwrap() {
            let scorer = actor_at(&mut arena, n(&case["scorer_handle"]) as u32);
            arena.actor_mut(scorer).unwrap().word_b4 = n(&case["team"]) as u32;
        }
        let mut hold = NetworkGameHold::default();
        for slot in 0..17 {
            hold.game.states[slot] = n(&case["states"][slot]) as u32;
            let name = hex(case["names"][slot].as_str().unwrap());
            hold.game.names[slot][..name.len()].copy_from_slice(&name);
            hold.game.scores[slot] = [0, 1, 2].map(|k| n(&case["scores"][slot][k]) as u16);
        }
        arena.lend_network_game(Box::new(hold));
        if n(&case["player"]) != 0 {
            arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]);
        }
        let mut host = Recorder {
            terrain: terrain(&Value::Null),
            null,
            calls: Vec::new(),
            strings: receipt_strings(&fixture),
            rng: n(&case["seed"]) as u32,
        };
        let on_host = n(&case["host"]) != 0;
        match arena.tick_goal(goal, on_host, n(&case["limit"]) as u32, &mut host) {
            // Retail adds a sideless goal's points to the bytes before
            // 0052E528; no world places one.
            Err(BallError::GoalSide(_, side)) => {
                assert!(!(1..=2).contains(&side), "{label}");
                continue;
            }
            result => result.unwrap(),
        }
        let after = &case["after"];
        let calls: Vec<Value> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call[0] != "flag1")
            .cloned()
            .collect();
        assert_eq!(host.calls, calls, "{label} calls");
        if !on_host {
            // 00479F20(0): it leaves the update list and changes nothing.
            assert_eq!(case["calls"][0][0], "flag1", "{label}");
            continue;
        }
        let state = arena.goal_state(goal).unwrap();
        let marks = [0, 1].map(|i| n(&after["marks"][i]) as u8);
        assert_eq!([state.crossed_74, state.crossed_75], marks, "{label} marks");
        let bytes = |key: &str| [0, 1].map(|i| n(&after[key][i]) as u8);
        assert_eq!(arena.powerball_goals, bytes("goals"), "{label} goals");
        assert_eq!(arena.powerball_seen, bytes("seen"), "{label} seen");
        check(&arena, ball, &after["ball"], &label);
        assert_eq!(host.rng, n(&after["rng"]) as u32, "{label} rng");
        let game = &arena.network_game_mut().unwrap().game;
        for slot in 0..17 {
            let scores = [0, 1, 2].map(|k| n(&after["scores"][slot][k]) as u16);
            assert_eq!(game.scores[slot], scores, "{label} slot {slot} scores");
        }
    }
}

/// `check_powerball.py`: 004213C8's Powerball arm over the ball held by
/// the vehicle, by its source or by neither, soft and hard blows, and
/// collisions with an actor that resolves (the ball itself among them).
#[test]
fn a_hard_blow_knocks_the_ball_loose_as_004213c8() {
    let fixture = fixture();
    let null = ActorHandle::from_raw(n(&fixture["null"]) as u32);
    for (index, case) in fixture["cases"]["knocks"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let label = format!("knock case {index}");
        let (mut arena, ball) = ball(&case["ball"], null);
        arena.powerball = Some(ball);
        arena.powerball_goals = [1, 2];
        arena
            .reassign_actor_handle(
                ball,
                ActorHandle::from_raw(n(&case["ball"]["handle"]) as u32),
            )
            .unwrap();
        arena.actor_mut(ball).unwrap().rotating.basis = basis(&case["ball_basis"]);
        let words =
            |key: &str| -> [u16; 3] { std::array::from_fn(|i| n(&case[key][i]) as i16 as u16) };
        let vehicle = actor_at(&mut arena, n(&case["vehicle_handle"]) as u32);
        arena.actor_mut(vehicle).unwrap().rotating.basis = basis(&case["own"]);
        arena.actor_mut(vehicle).unwrap().words_8e_92 = words("own_velocity");
        let other = n(&case["other_handle"]) as u32;
        if other != 0x0eee_eeee {
            let other = actor_at(&mut arena, other);
            arena.actor_mut(other).unwrap().rotating.basis = basis(&case["theirs"]);
            arena.actor_mut(other).unwrap().words_8e_92 = words("their_velocity");
        }
        let packet = crate::object_runtime::ActorDamagePacket {
            strength: n(&case["strength"]) as i16,
            channel: n(&case["channel"]) as u8,
            byte_03: None,
        };
        let mut rng = n(&case["seed"]) as u32;
        arena
            .powerball_hit(
                vehicle,
                packet,
                ActorHandle::from_raw(n(&case["source_handle"]) as u32),
                Some(null),
                &mut rng,
            )
            .unwrap();
        check(&arena, ball, &case["after"], &label);
        assert_eq!(rng, n(&case["rng"]) as u32, "{label} rng");
        let sends: Vec<u32> = arena
            .take_network_sends()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(sends, recorded_sends(&case["calls"]), "{label} sends");
    }
}

/// Every object on the world's lists that `keep` admits.
fn world_objects(arena: &ObjectArena, keep: impl Fn(ObjectId) -> bool) -> Vec<ObjectId> {
    use crate::object_runtime::ObjectList;
    let mut found = Vec::new();
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag1Primary,
        ObjectList::Parking,
    ] {
        for node in arena.list(list) {
            if keep(node.object) && !found.contains(&node.object) {
                found.push(node.object);
            }
        }
    }
    found
}

/// Powerball's world (25), hosted: one ball, the ball 005468D0, scored
/// by its network start (00403CF0) at home and 500 up with every goal
/// byte idle; and four goals, two a side.
#[test]
fn powerball_builds_its_ball_and_its_goals() {
    let mut settings = [0; 20];
    settings[0x10 / 4] = 4;
    let process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings,
        ..crate::process_particles::factories::ParticleFactoryProcessState::native_startup()
    };
    let (_resident, _assets, world) = crate::test_worlds::network_world(25, process);
    let arena = &world.arena;
    let balls = world_objects(arena, |object| arena.is_ball(object));
    assert_eq!(balls, [arena.powerball().expect("005468D0")]);
    let state = arena.ball(balls[0]).unwrap();
    assert_eq!(
        (state.state_148, state.sequence_174, state.wait_180),
        (4, 1, 500)
    );
    let home = state.home_160.unwrap();
    assert_eq!(
        arena.object(balls[0]).unwrap().base.position_words,
        Some([home[0], home[1].wrapping_add(500), home[2]])
    );
    assert_eq!(arena.powerball_goals(), [GOAL_WON; 2]);
    let goals = world_objects(arena, |object| arena.goal_state(object).is_ok());
    let mut sides: Vec<i32> = goals
        .iter()
        .map(|&goal| arena.goal_state(goal).unwrap().side_6c)
        .collect();
    sides.sort();
    assert_eq!(sides, [1, 1, 2, 2]);
}
