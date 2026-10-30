//! The Powerball against its disassembly (004863F0 and 004864B0) and
//! Infest03's and Infest04's world-11 ball (`powerball.js`), which settles
//! and bounces on the ground at every update.

use super::*;
use crate::object_runtime::{
    ActorHandle, ActorMessage, ActorMessageError, ActorMessageReply, ActorRegistryConfig,
    ActorTerrainInputs, FamilyUpdateEffect, PlayerActorLookupContext,
};

const SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn powerball(arena: &mut ObjectArena) -> ObjectId {
    let ball = arena
        .allocate_selector57(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
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
    arena.owned_mut(ball).unwrap().state.base.position_words = Some([0x100, 0x200, 0x100]);
    arena.actor_mut(ball).unwrap().maximum_health = Some(10);
    ball
}

/// The status reaches no effect while +CC and +CE are zero.
struct Unreached;

impl FamilyUpdateHost for Unreached {
    type Error = String;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        None
    }
    fn random(&mut self) -> Result<u32, String> {
        Err("unreached RNG".into())
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

/// What 004863F0 reads and writes: flags +2C, position +32, dirty +38,
/// velocity +8E, spin +94, health +A0/+A2 and the status +C8/+CC/+CE.
#[derive(Debug, PartialEq, Eq)]
struct Fields {
    flags: u32,
    position: [u16; 3],
    dirty: u16,
    velocity: [u16; 3],
    spin: [u32; 3],
    health: [u16; 2],
    status: [u16; 3],
}

fn capture(hex: &str) -> Fields {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    assert_eq!(bytes.len(), 0x138);
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    Fields {
        flags: dword(0x2c),
        position: [word(0x32), word(0x34), word(0x36)],
        dirty: word(0x38),
        velocity: [word(0x8e), word(0x90), word(0x92)],
        spin: [dword(0x94), dword(0x98), dword(0x9c)],
        health: [word(0xa0), word(0xa2)],
        status: [word(0xc8), word(0xcc), word(0xce)],
    }
}

fn state(arena: &ObjectArena, ball: ObjectId) -> Fields {
    let base = &arena.object(ball).unwrap().base;
    let actor = arena.actor(ball).unwrap();
    Fields {
        flags: base.flags,
        position: base.position_words.unwrap(),
        dirty: base.dirty_word,
        velocity: actor.words_8e_92,
        spin: actor.words_94_9c,
        health: [actor.health, actor.maximum_health.unwrap()],
        status: [actor.word_c8, actor.word_cc, actor.word_ce],
    }
}

fn set(arena: &mut ObjectArena, ball: ObjectId, fields: &Fields) {
    let base = &mut arena.owned_mut(ball).unwrap().state.base;
    base.flags = fields.flags;
    base.position_words = Some(fields.position);
    base.dirty_word = fields.dirty;
    let actor = arena.actor_mut(ball).unwrap();
    actor.words_8e_92 = fields.velocity;
    actor.words_94_9c = fields.spin;
    actor.health = fields.health[0];
    actor.maximum_health = Some(fields.health[1]);
    [actor.word_c8, actor.word_cc, actor.word_ce] = fields.status;
}

#[derive(serde::Deserialize)]
struct Receipts {
    updates: Vec<Update>,
}

#[derive(serde::Deserialize)]
struct Update {
    recording: String,
    at: String,
    delta: u32,
    load: i32,
    before: String,
    after: String,
}

/// Two updates from each recording: the first falls from rest (gravity 122,
/// drag to 121, down 15) and the settled one bounces from 40 to -81 and down
/// 10. Each writes only the position, its dirty bits and the velocity.
#[test]
fn updates_match_infest03_and_infest04() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../tests/fixtures/powerball_infest.json"
    ))
    .unwrap();
    assert_eq!(receipts.updates.len(), 4);
    for update in &receipts.updates {
        let mut arena = arena();
        let ball = powerball(&mut arena);
        set(&mut arena, ball, &capture(&update.before));
        arena
            .tick_selector57(ball, update.delta, Some(update.load), &SINE, &mut Unreached)
            .unwrap();
        assert_eq!(
            state(&arena, ball),
            capture(&update.after),
            "{} {}",
            update.recording,
            update.at
        );
    }
}

/// 004863F0's +A0 = +A2 refills a hurt ball, and its mask 0xD leaves the
/// spin alone (no angular drag or world-space turn).
#[test]
fn the_update_refills_its_health_and_keeps_its_spin() {
    let mut arena = arena();
    let ball = powerball(&mut arena);
    let actor = arena.actor_mut(ball).unwrap();
    actor.health = 3;
    actor.words_94_9c = [0x4000, 0, 0x100];
    arena
        .tick_selector57(ball, 125_000, None, &SINE, &mut Unreached)
        .unwrap();
    let actor = arena.actor(ball).unwrap();
    assert_eq!(actor.health, 10);
    assert_eq!(actor.words_94_9c, [0x4000, 0, 0x100]);
    assert_eq!(actor.words_8e_92, [0, (-121_i16) as u16, 0]);
    assert_eq!(
        arena.object(ball).unwrap().base.position_words,
        Some([0x100, 0x200 - 15, 0x100])
    );
}

/// An unwritten +A2 stops after the physics' writes.
#[test]
fn an_unwritten_maximum_stops_after_the_physics() {
    let mut arena = arena();
    let ball = powerball(&mut arena);
    arena.actor_mut(ball).unwrap().maximum_health = None;
    let error = arena
        .tick_selector57(ball, 125_000, None, &SINE, &mut Unreached)
        .unwrap_err();
    assert!(matches!(error, Selector57Error::UnwrittenMaximum(stopped) if stopped == ball));
    assert_eq!(
        arena.object(ball).unwrap().base.position_words,
        Some([0x100, 0x200 - 15, 0x100])
    );
}

fn message(sender: Option<ObjectId>, code: i32) -> ActorMessage {
    ActorMessage {
        sender,
        distance_squared: 0,
        code,
        argument: 2000,
    }
}

/// 004864B0: message 5 retires the ball and then its sender (when that is
/// resource 0xE, which the world reads after the first); every other
/// message answers 0, and a null sender's +30 faults.
#[test]
fn message_five_retires_it_with_its_sender() {
    let mut arena = arena();
    let ball = powerball(&mut arena);
    let sender = powerball(&mut arena);
    let context = PlayerActorLookupContext { process: None };
    assert_eq!(
        arena
            .receive_actor_message(ball, message(Some(sender), 5), context, false)
            .unwrap(),
        ActorMessageReply::RetireWithSender { sender }
    );
    for code in [0, 4, 6, 7, 8, 0xc, 0x16] {
        assert_eq!(
            arena
                .receive_actor_message(ball, message(Some(sender), code), context, false)
                .unwrap(),
            ActorMessageReply::Done(0),
            "{code:#x}"
        );
    }
    assert!(matches!(
        arena.receive_actor_message(ball, message(None, 5), context, false),
        Err(ActorMessageError::NullSender {
            address: 0x0048_64c0,
            ..
        })
    ));
    assert_eq!(arena.actor(ball).unwrap().word_c4, 3);
}

#[derive(Debug, PartialEq)]
enum Death {
    Visual(u32),
    Burst(crate::process_particles::births::ParticleBurst),
    Call {
        radius: u32,
        code: i32,
        argument: u32,
    },
}

#[derive(Default)]
struct Dying {
    calls: Vec<Death>,
}

impl Selector57DeathHost for Dying {
    type Error = String;
    fn visual(&mut self, _: &mut ObjectArena, _: ObjectId, resource: u32) -> Result<(), String> {
        self.calls.push(Death::Visual(resource));
        Ok(())
    }
    fn burst(
        &mut self,
        _: &mut ObjectArena,
        burst: crate::process_particles::births::ParticleBurst,
    ) -> Result<(), String> {
        self.calls.push(Death::Burst(burst));
        Ok(())
    }
    fn call(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        radius: u32,
        code: i32,
        argument: u32,
    ) -> Result<(), String> {
        self.calls.push(Death::Call {
            radius,
            code,
            argument,
        });
        Ok(())
    }
}

/// 00486420 against the original under Unicorn
/// ([selector57_death_native.json](../../../tests/fixtures/selector57_death_native.json),
/// `check_selector57_death.py`): the visual through
/// +4C(0, 0x36, 0), the burst at its position, 0047DD20(2000, 0xC, 2000)
/// from the ball and its disposal, whatever its flags.
#[test]
fn the_death_matches_the_original() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector57_death_native.json"
    ))
    .unwrap();
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let input = &case["input"];
        let mut arena = arena();
        let ball = powerball(&mut arena);
        let base = &mut arena.owned_mut(ball).unwrap().state.base;
        base.flags = input["flags"].as_u64().unwrap() as u32;
        base.position_words = Some(std::array::from_fn(|axis| {
            input["position"][axis].as_u64().unwrap() as u16
        }));
        let mut dying = Dying::default();
        arena.retire_selector57(ball, &mut dying).unwrap();
        let events = case["events"].as_array().unwrap();
        let mut expected = Vec::new();
        for event in &events[..events.len() - 1] {
            // The burst block is the caller's; the others are on the ball.
            if event["kind"] != "burst" {
                assert_eq!(event["self"], true, "{input}");
            }
            expected.push(match event["kind"].as_str().unwrap() {
                "child" => {
                    let args = &event["args"];
                    assert_eq!((&args[0], &args[2]), (&0.into(), &0.into()));
                    Death::Visual(args[1].as_u64().unwrap() as u32)
                }
                "burst" => {
                    let w = |n: usize| event["words"][n].as_u64().unwrap() as u16;
                    let d = |n: usize| event["dwords"][n].as_u64().unwrap() as u32;
                    Death::Burst(crate::process_particles::births::ParticleBurst {
                        debris_count: w(0),
                        position: [w(1), w(2), w(3)],
                        placement_radius: w(4),
                        central_type: d(0),
                        debris_types: [d(1), d(2)],
                        speed: d(3),
                        flags: d(4),
                    })
                }
                "walk" => Death::Call {
                    radius: event["radius"].as_u64().unwrap() as u32,
                    code: event["code"].as_u64().unwrap() as i32,
                    argument: event["argument"].as_u64().unwrap() as u32,
                },
                other => panic!("{other}"),
            });
        }
        assert_eq!(events.last().unwrap()["kind"], "dispose");
        assert_eq!(dying.calls, expected, "{input}");
        assert!(arena
            .list(crate::object_runtime::ObjectList::Disposal)
            .iter()
            .any(|node| node.object == ball));
    }
}
