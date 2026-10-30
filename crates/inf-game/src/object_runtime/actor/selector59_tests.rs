//! The Boulders against their disassembly (00405780, 00405D10, 00405DC0,
//! 004057D0, 00405A00 and 00405E30) and Infest03's and Infest04's boulders
//! (`boulders.js`).

use inf_formats::{ByteSpan, InstanceField};

use super::*;
use crate::object_runtime::{
    ActorHandle, ActorRegistryConfig, ActorTerrainInputs, FamilyUpdateEffect,
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

fn boulder(arena: &mut ObjectArena) -> ObjectId {
    let boulder = arena
        .allocate_selector59(ActorConstructionRequest {
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
    arena.owned_mut(boulder).unwrap().state.base.position_words = Some([0x100, 0x200, 0x100]);
    boulder
}

fn view(code: i32, payload: InstanceCommandPayload) -> InstanceCommandView {
    InstanceCommandView {
        target: InstanceCommandTarget::Constructor(59),
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: code,
        },
        proven_method: Some(0x0040_5d10),
        payload_consumer: Some(0x0040_5d10),
        payload,
    }
}

fn byte(value: u8) -> InstanceField<u8> {
    InstanceField {
        span: ByteSpan { start: 4, end: 5 },
        value,
    }
}

/// The status reaches no effect while +CC and +CE are zero and the boulder
/// does not smoke.
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

/// 00405780 and 00450400: rest 0, crushing on, the other two off, the
/// position at +138 still unwritten.
#[test]
fn construction_crushes_and_leaves_home_unwritten() {
    let mut arena = arena();
    let boulder = boulder(&mut arena);
    assert_eq!(
        arena.selector59(boulder).unwrap(),
        &Selector59State {
            home_138: None,
            rest_140: 0,
            crushes_144: 1,
            crushes_any_145: 0,
            smokes_146: 0,
            timer_148: None,
            ball_148: None,
        }
    );
}

fn context() -> crate::object_runtime::ActorCommandContext<'static> {
    static RUNTIME: std::sync::OnceLock<crate::ResidentRuntime> = std::sync::OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap()
    });
    crate::object_runtime::ActorCommandContext {
        runtime,
        first_player_grants: None,
        terrain: None,
    }
}

/// 00405D10: code 16 wakes it only for a nonzero BYTE, 17 stores BYTE == 0
/// and 18/19 BYTE != 0; codes above 19 refuse without a read.
#[test]
fn commands_wake_it_and_set_its_bytes() {
    let mut arena = arena();
    let boulder = boulder(&mut arena);
    arena.selector59_mut(boulder).unwrap().rest_140 = 500_001;
    let base = &mut arena.owned_mut(boulder).unwrap().state.base;
    base.flags |= 0x0420_0000;
    base.dirty_word = 0;
    let flags = base.flags;
    let wake = |value| view(16, InstanceCommandPayload::Selector59Wake(byte(value)));
    assert!(arena
        .apply_selector59_command(boulder, &wake(0), context())
        .unwrap());
    assert_eq!(arena.selector59(boulder).unwrap().rest_140, 500_001);
    assert_eq!(arena.object(boulder).unwrap().base.flags, flags);
    assert!(arena
        .apply_selector59_command(boulder, &wake(2), context())
        .unwrap());
    assert_eq!(arena.selector59(boulder).unwrap().rest_140, 0);
    assert_eq!(
        arena.object(boulder).unwrap().base.flags,
        flags & !0x0420_0000
    );
    // The low nine bits do not change, so the dirty word stays clear.
    assert_eq!(arena.object(boulder).unwrap().base.dirty_word, 0);
    let bytes = |arena: &ObjectArena| {
        let state = arena.selector59(boulder).unwrap();
        [state.crushes_144, state.crushes_any_145, state.smokes_146]
    };
    for (code, value, expected) in [
        (17, 5, [0, 0, 0]),
        (17, 0, [1, 0, 0]),
        (18, 7, [1, 1, 0]),
        (19, 1, [1, 1, 1]),
        (18, 0, [1, 0, 1]),
        (19, 0, [1, 0, 0]),
    ] {
        let payload = match code {
            17 => InstanceCommandPayload::Selector59Crushes144(byte(value)),
            18 => InstanceCommandPayload::Selector59CrushesAny145(byte(value)),
            _ => InstanceCommandPayload::Selector59Smokes146(byte(value)),
        };
        assert!(arena
            .apply_selector59_command(boulder, &view(code, payload), context())
            .unwrap());
        assert_eq!(bytes(&arena), expected, "{code} {value}");
    }
    assert!(!arena
        .apply_selector59_command(
            boulder,
            &view(20, InstanceCommandPayload::Opaque),
            context()
        )
        .unwrap());
}

/// 00405DC0: up by its +28, which becomes +138; the shared 0047AAD0 (and
/// its +74) is not run.
#[test]
fn postinitialization_raises_it_and_keeps_its_home() {
    let mut arena = arena();
    let boulder = boulder(&mut arena);
    arena.owned_mut(boulder).unwrap().state.base.word_28 = 135;
    arena.actor_mut(boulder).unwrap().word_74 = 7;
    arena.finish_selector59_initialization(boulder).unwrap();
    let raised = Some([0x100, 0x200 + 135, 0x100]);
    assert_eq!(arena.object(boulder).unwrap().base.position_words, raised);
    assert_eq!(arena.selector59(boulder).unwrap().home_138, raised);
    assert_eq!(arena.actor(boulder).unwrap().word_74, 7);
}

/// The update before the postinitialization reads an unwritten +138.
#[test]
fn an_update_before_the_postinitialization_stops() {
    let mut arena = arena();
    let boulder = boulder(&mut arena);
    assert!(matches!(
        arena.tick_selector59(boulder, 125_000, None, &SINE, &mut Unreached),
        Err(Selector59Error::UnwrittenHome(_))
    ));
}

fn placed(arena: &mut ObjectArena) -> ObjectId {
    let boulder = boulder(arena);
    arena.owned_mut(boulder).unwrap().state.base.word_28 = 135;
    arena.finish_selector59_initialization(boulder).unwrap();
    boulder
}

/// 004057D0's still branch: asleep it does nothing; resting, it counts the
/// delta and moves, and on reaching half a second it falls asleep without
/// moving; woken, it clears the three flags and moves.
#[test]
fn a_still_boulder_rests_then_sleeps_and_a_woken_one_moves() {
    let mut arena = arena();
    let boulder = placed(&mut arena);
    let home = arena.selector59(boulder).unwrap().home_138.unwrap();
    // Asleep: the rest timer past the half second, nothing moves.
    arena.selector59_mut(boulder).unwrap().rest_140 = 500_001;
    arena
        .tick_selector59(boulder, 125_000, None, &SINE, &mut Unreached)
        .unwrap();
    assert_eq!(
        arena.object(boulder).unwrap().base.position_words,
        Some(home)
    );
    assert_eq!(arena.actor(boulder).unwrap().words_8e_92, [0, 0, 0]);
    // Resting: 400000 + 99999 stays below and it falls (gravity 97).
    arena.selector59_mut(boulder).unwrap().rest_140 = 400_000;
    arena
        .tick_selector59(boulder, 99_999, None, &SINE, &mut Unreached)
        .unwrap();
    assert_eq!(arena.selector59(boulder).unwrap().rest_140, 499_999);
    assert_eq!(
        arena.actor(boulder).unwrap().words_8e_92[1],
        (-96_i16) as u16
    );
    // Back where it was and still (Y 32), the next microsecond reaches the
    // half second: asleep, without moving.
    arena.owned_mut(boulder).unwrap().state.base.position_words = Some(home);
    arena.actor_mut(boulder).unwrap().words_8e_92 = [0, 0x20, 0];
    let flags = arena.object(boulder).unwrap().base.flags;
    arena
        .tick_selector59(boulder, 1, None, &SINE, &mut Unreached)
        .unwrap();
    assert_eq!(arena.selector59(boulder).unwrap().rest_140, 500_000);
    assert_eq!(
        arena.object(boulder).unwrap().base.flags,
        flags | 0x0400_0000
    );
    assert_eq!(
        arena.object(boulder).unwrap().base.position_words,
        Some(home)
    );
    assert_eq!(arena.actor(boulder).unwrap().words_8e_92, [0, 0x20, 0]);
    // Woken: the three flags go and it moves.
    arena.owned_mut(boulder).unwrap().state.base.flags |= 0x0060_0000;
    arena
        .tick_selector59(boulder, 125_000, None, &SINE, &mut Unreached)
        .unwrap();
    assert_eq!(arena.selector59(boulder).unwrap().rest_140, 0);
    assert_eq!(arena.object(boulder).unwrap().base.flags & 0x0460_0000, 0);
    assert_ne!(
        arena.object(boulder).unwrap().base.position_words,
        Some(home)
    );
}

/// Y beyond 32 or any X or Z velocity is not still: a set rest timer clears
/// with the three flags, and it moves.
#[test]
fn a_moving_boulder_wakes() {
    let mut arena = arena();
    let boulder = placed(&mut arena);
    for velocity in [
        [0, 0x21, 0],
        [0, (-0x21_i16) as u16, 0],
        [1, 0, 0],
        [0, 0, 1],
    ] {
        let home = arena.selector59(boulder).unwrap().home_138.unwrap();
        arena.owned_mut(boulder).unwrap().state.base.position_words = Some(home);
        arena.actor_mut(boulder).unwrap().words_8e_92 = velocity;
        arena.selector59_mut(boulder).unwrap().rest_140 = 500_001;
        arena.owned_mut(boulder).unwrap().state.base.flags |= 0x0460_0000;
        arena
            .tick_selector59(boulder, 125_000, None, &SINE, &mut Unreached)
            .unwrap();
        assert_eq!(
            arena.selector59(boulder).unwrap().rest_140,
            0,
            "{velocity:?}"
        );
        assert_eq!(arena.object(boulder).unwrap().base.flags & 0x0460_0000, 0);
    }
}

/// 00405E30: off without +144 or +145; above 320 (any change, with +145)
/// the excess damages by `isqrt(excess) * excess >> 4` times the mass
/// `>> 6`, at most 0xFFFF, on channel 3.
#[test]
fn crushing_damages_by_the_change_of_speed() {
    let mut arena = arena();
    let boulder = boulder(&mut arena);
    let set = |arena: &mut ObjectArena, mass: u16, velocity: [i16; 3]| {
        let actor = arena.actor_mut(boulder).unwrap();
        actor.word_8c = mass;
        actor.words_8e_92 = velocity.map(|word| word as u16);
    };
    let packet = |strength: i16| ActorDamagePacket {
        strength,
        channel: 3,
        byte_03: None,
    };
    // Infest04's crush of the vehicle: a change of 700, 1804.
    set(&mut arena, 256, [-699, 10, -34]);
    assert_eq!(
        arena.selector59_crush(boulder, [0, 40, 0]).unwrap(),
        Some(packet(1804))
    );
    // 320 itself does not crush; 321 crushes by 0.
    set(&mut arena, 256, [-320, 0, 0]);
    assert_eq!(arena.selector59_crush(boulder, [0, 0, 0]).unwrap(), None);
    set(&mut arena, 256, [-321, 0, 0]);
    assert_eq!(
        arena.selector59_crush(boulder, [0, 0, 0]).unwrap(),
        Some(packet(0))
    );
    // +145 crushes from zero: 100 → isqrt(100) * 100 >> 4 = 62, times 4.
    arena.selector59_mut(boulder).unwrap().crushes_any_145 = 1;
    set(&mut arena, 256, [-100, 0, 0]);
    assert_eq!(
        arena.selector59_crush(boulder, [0, 0, 0]).unwrap(),
        Some(packet(248))
    );
    // A heavy fast change stops at 0xFFFF, a negative WORD: 5000 gives
    // 70 * 5000 >> 4 = 21875, times 4096 >> 6.
    set(&mut arena, 0x1000, [-5000, 0, 0]);
    assert_eq!(
        arena.selector59_crush(boulder, [0, 0, 0]).unwrap(),
        Some(packet(-1))
    );
    // The mass product wraps as 00405ED3's IMUL does: 141 * 20000 >> 4 =
    // 176250, times 0xFFFF, is negative and passes the clamp.
    set(&mut arena, 0xffff, [-20000, 0, 0]);
    assert_eq!(
        arena.selector59_crush(boulder, [0, 0, 0]).unwrap(),
        Some(packet(
            (0xffff_i32.wrapping_mul(176_250) >> 6) as u16 as i16
        ))
    );
    // Neither byte: no crush.
    let state = arena.selector59_mut(boulder).unwrap();
    state.crushes_144 = 0;
    state.crushes_any_145 = 0;
    assert_eq!(arena.selector59_crush(boulder, [0, 0, 0]).unwrap(), None);
}

#[derive(serde::Deserialize)]
struct Receipts {
    updates: Vec<Update>,
    pairs: Vec<Pair>,
}

#[derive(serde::Deserialize)]
struct Update {
    recording: String,
    at: String,
    delta: u32,
    load: i32,
    /// The 0x148-byte boulder.
    before: String,
    after: String,
}

#[derive(serde::Deserialize)]
struct Pair {
    recording: String,
    at: String,
    partner_vtable: u32,
    /// The boulder's velocity before the exchange, +A4's second argument.
    saved: [u16; 3],
    before: String,
    damage: Vec<Damage>,
}

#[derive(serde::Deserialize)]
struct Damage {
    /// 0047C6E0's four packet bytes.
    packet: String,
    source: u32,
}

fn receipts() -> Receipts {
    serde_json::from_str(include_str!("../../../tests/fixtures/boulders_infest.json")).unwrap()
}

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

/// What 004057D0 reads and writes: flags +2C, position +32, dirty +38, the
/// basis +44, velocity +8E, +138, +140 and the BYTEs +144..+146.
#[derive(Debug, PartialEq, Eq)]
struct Fields {
    flags: u32,
    position: [u16; 3],
    dirty: u16,
    basis: [u32; 10],
    velocity: [u16; 3],
    home: [u16; 3],
    rest: u32,
    bytes: [u8; 3],
}

fn capture(hex: &str) -> (Fields, u16) {
    let bytes = bytes(hex);
    assert_eq!(bytes.len(), 0x148);
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        Fields {
            flags: dword(0x2c),
            position: [word(0x32), word(0x34), word(0x36)],
            dirty: word(0x38),
            basis: std::array::from_fn(|index| dword(0x44 + 4 * index)),
            velocity: [word(0x8e), word(0x90), word(0x92)],
            home: [word(0x138), word(0x13a), word(0x13c)],
            rest: dword(0x140),
            bytes: [bytes[0x144], bytes[0x145], bytes[0x146]],
        },
        word(0x28),
    )
}

fn state(arena: &ObjectArena, boulder: ObjectId) -> Fields {
    let base = &arena.object(boulder).unwrap().base;
    let actor = arena.actor(boulder).unwrap();
    let state = arena.selector59(boulder).unwrap();
    Fields {
        flags: base.flags,
        position: base.position_words.unwrap(),
        dirty: base.dirty_word,
        basis: actor.rotating.basis.words,
        velocity: actor.words_8e_92,
        home: state.home_138.unwrap(),
        rest: state.rest_140,
        bytes: [state.crushes_144, state.crushes_any_145, state.smokes_146],
    }
}

fn set(arena: &mut ObjectArena, boulder: ObjectId, fields: &Fields, radius: u16) {
    let base = &mut arena.owned_mut(boulder).unwrap().state.base;
    base.flags = fields.flags;
    base.position_words = Some(fields.position);
    base.dirty_word = fields.dirty;
    base.word_28 = radius;
    let actor = arena.actor_mut(boulder).unwrap();
    actor.rotating.basis.words = fields.basis;
    actor.words_8e_92 = fields.velocity;
    let state = arena.selector59_mut(boulder).unwrap();
    state.home_138 = Some(fields.home);
    state.rest_140 = fields.rest;
    [state.crushes_144, state.crushes_any_145, state.smokes_146] = fields.bytes;
}

fn sine() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values
}

/// Six of Infest04's updates on world 20: an asleep boulder returns as it
/// was (its first update, 11 ms); one that moved keeps its new position
/// and falls; a settled one bounces from the contact's 40 to -81; a still
/// one drawn near does not roll; one drawn near that moved turns its basis
/// by the arc over its radius (0047A660, 0047A760).
#[test]
fn updates_match_infest04() {
    let receipts = receipts();
    assert_eq!(receipts.updates.len(), 6);
    let sine = sine();
    let mut rolled = 0;
    for update in &receipts.updates {
        let mut arena = arena();
        let boulder = boulder(&mut arena);
        let (before, radius) = capture(&update.before);
        set(&mut arena, boulder, &before, radius);
        arena
            .tick_selector59(
                boulder,
                update.delta,
                Some(update.load),
                &sine,
                &mut Unreached,
            )
            .unwrap();
        let (after, _) = capture(&update.after);
        if after.basis != before.basis {
            rolled += 1;
        }
        assert_eq!(
            state(&arena, boulder),
            after,
            "{} {}",
            update.recording,
            update.at
        );
    }
    assert_eq!(rolled, 1);
}

/// Every Infest04 +A4 that damaged, five of 688: the vehicle (00520860)
/// crushed by 1804, two boulders by 384 and two by 0, each from the
/// boulder's handle on channel 3; and two that did not (a change of 98,
/// and a mutant scientist's touch).
#[test]
fn crushes_match_infest04() {
    let receipts = receipts();
    assert_eq!(receipts.pairs.len(), 7);
    for pair in &receipts.pairs {
        let mut arena = arena();
        let boulder = boulder(&mut arena);
        let before = bytes(&pair.before);
        let (fields, radius) = capture(&pair.before);
        set(&mut arena, boulder, &fields, radius);
        arena.actor_mut(boulder).unwrap().word_8c =
            u16::from_le_bytes([before[0x8c], before[0x8d]]);
        let crush = arena.selector59_crush(boulder, pair.saved).unwrap();
        let recorded = pair.damage.first().map(|damage| {
            let packet = bytes(&damage.packet);
            assert_eq!(
                damage.source,
                u32::from_le_bytes(before[0x84..0x88].try_into().unwrap()),
                "{} {}",
                pair.recording,
                pair.at
            );
            (i16::from_le_bytes([packet[0], packet[1]]), packet[2])
        });
        assert!(pair.damage.len() <= 1);
        assert_eq!(
            crush.map(|packet| (packet.strength, packet.channel)),
            recorded,
            "{} {} partner {:08X}",
            pair.recording,
            pair.at,
            pair.partner_vtable
        );
    }
}

/// 00405840..0040591B: a move of one radius along X turns a radian (10430)
/// about -Z, one along Z about X, and one along Y not at all; the arc
/// truncates toward zero, and a zero radius faults.
#[test]
fn the_roll_turns_a_radian_per_radius() {
    let home = [0x100, 0x200, 0x100];
    assert_eq!(
        roll_turn([0x100 + 135, 0x200, 0x100], home, 135),
        Some([0, 0, -10430])
    );
    assert_eq!(
        roll_turn([0x100, 0x200, 0x100 + 135], home, 135),
        Some([10430, 0, 0])
    );
    assert_eq!(
        roll_turn([0x100, 0x200 + 135, 0x100], home, 135),
        Some([0, 0, 0])
    );
    // -10430 / 135 is -77.3, truncated.
    assert_eq!(
        roll_turn([0x100 - 1, 0x200, 0x100], home, 135),
        Some([0, 0, 77])
    );
    assert_eq!(roll_turn(home, home, 0), None);
}
