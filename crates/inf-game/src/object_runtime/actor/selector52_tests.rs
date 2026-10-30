//! The mining rig against its disassembly and Infest04's Helniazine rigs
//! (`mining-rig.js`): three linked rigs call the mines within 2000 at every
//! update and change nothing else.

use inf_formats::{ByteSpan, InstanceField};

use super::*;
use crate::object_runtime::{ActorHandle, ActorRegistryConfig, ActorTerrainInputs};

const SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn rig(arena: &mut ObjectArena) -> ObjectId {
    let rig = arena
        .allocate_selector52(ActorConstructionRequest {
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
    arena.owned_mut(rig).unwrap().state.base.position_words = Some([0x100, 0, 0x100]);
    rig
}

fn view(code: i32, payload: InstanceCommandPayload) -> InstanceCommandView {
    InstanceCommandView {
        target: InstanceCommandTarget::Constructor(52),
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: code,
        },
        proven_method: Some(0x0045_4880),
        payload_consumer: Some(0x0045_4880),
        payload,
    }
}

/// Answers each call with the next reply and records its radius.
#[derive(Default)]
struct Calls {
    replies: Vec<u32>,
    radii: Vec<u32>,
}

impl Selector52Host for Calls {
    type Error = ();
    fn call_mines(&mut self, _: &mut ObjectArena, _: ObjectId, radius: u32) -> Result<u32, ()> {
        self.radii.push(radius);
        Ok(self.replies.get(self.radii.len() - 1).copied().unwrap_or(0))
    }
}

fn set(arena: &mut ObjectArena, rig: ObjectId, state: Selector52State) {
    *arena.selector52_mut(rig).unwrap() = state;
}

#[test]
fn case_34_builds_a_running_unlinked_empty_rig() {
    let mut arena = arena();
    let rig = rig(&mut arena);
    assert_eq!(
        arena.selector52(rig).unwrap(),
        &Selector52State {
            running_138: 1,
            links_13c: 0,
            level_140: 0,
            filling_142: 0,
        }
    );
}

/// 00454880: code 16 stores BYTE == 0 and clears flag 0x40, marking the
/// change; codes above 16 refuse without a write.
#[test]
fn command_16_stops_or_runs_the_rig_and_clears_flag_40() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    let context = || ActorCommandContext {
        runtime: &runtime,
        first_player_grants: None,
        terrain: None,
    };
    let byte = |value| {
        InstanceCommandPayload::Selector52Stopped(InstanceField {
            span: ByteSpan { start: 4, end: 5 },
            value,
        })
    };
    let mut arena = arena();
    let rig = rig(&mut arena);
    arena.owned_mut(rig).unwrap().state.base.flags |= 0x40;
    for (value, running) in [(1, 0), (0, 1), (0x80, 0)] {
        assert!(arena
            .apply_selector52_command(rig, &view(16, byte(value)), context())
            .unwrap());
        assert_eq!(arena.selector52(rig).unwrap().running_138, running);
    }
    let base = &arena.object(rig).unwrap().base;
    assert_eq!(base.flags & 0x40, 0);
    assert_eq!(base.dirty_word & 2, 2);
    // A clear flag changes nothing to mark.
    arena.owned_mut(rig).unwrap().state.base.dirty_word = 0;
    arena
        .apply_selector52_command(rig, &view(16, byte(0)), context())
        .unwrap();
    assert_eq!(arena.object(rig).unwrap().base.dirty_word, 0);
    let before = arena.object(rig).unwrap().clone();
    assert!(!arena
        .apply_selector52_command(rig, &view(17, InstanceCommandPayload::Opaque), context())
        .unwrap());
    assert_eq!(arena.object(rig).unwrap(), &before);
}

/// 004546F0: 7 adds a link and 8 takes one; nothing else changes.
#[test]
fn messages_7_and_8_count_the_links() {
    let mut arena = arena();
    let rig = rig(&mut arena);
    for (code, links) in [(7, 1), (7, 2), (0xf, 2), (8, 1), (8, 0), (8, u32::MAX)] {
        arena.selector52_message(rig, code).unwrap();
        assert_eq!(arena.selector52(rig).unwrap().links_13c, links);
    }
}

/// 0045463D: any link holds the rig; it calls the mines within 2000 and
/// keeps its level and filling BYTE, running or not.
#[test]
fn a_held_rig_only_calls_the_mines_within_2000() {
    let mut arena = arena();
    let rig = rig(&mut arena);
    let held = Selector52State {
        running_138: 1,
        links_13c: 1,
        level_140: 0x1234,
        filling_142: 1,
    };
    set(&mut arena, rig, held.clone());
    let mut calls = Calls {
        replies: vec![5],
        ..Calls::default()
    };
    arena.tick_selector52(rig, 125_000, &mut calls).unwrap();
    assert_eq!(calls.radii, [2000]);
    assert_eq!(arena.selector52(rig).unwrap(), &held);
}

/// 0045465D..004546E3 with `delta / 100` units: draining loses them or
/// empties; filling gains them below 0xFFFF less them, else fills and calls
/// the mines within 0x400, stopping when none answers.
#[test]
fn a_free_running_rig_drains_or_fills_its_level() {
    let free = |level, filling| Selector52State {
        running_138: 1,
        links_13c: 0,
        level_140: level,
        filling_142: filling,
    };
    // (level, filling, delta, reply) -> (level, filling, calls)
    let cases = [
        ((1000, 0, 99_999, 0), (1, 0, 0)),
        ((1000, 0, 100_000, 0), (0, 0, 0)),
        ((1000, 0, 199_999, 0), (0, 0, 0)),
        ((0, 0, 125_000, 0), (0, 0, 0)),
        ((0, 1, 125_000, 0), (1250, 1, 0)),
        ((0xfffe - 1250, 1, 125_000, 0), (0xfffe, 1, 0)),
        ((0xffff - 1250, 1, 125_000, 0), (0xffff, 0, 1)),
        ((0xffff - 1250, 1, 125_000, 3), (0xffff, 1, 1)),
        ((0xffff, 1, 99, 0), (0xffff, 0, 1)),
        // Units beyond 0xFFFF make the bound negative: full at once.
        ((0, 1, u32::MAX, 2), (0xffff, 1, 1)),
    ];
    for ((level, filling, delta, reply), (after, still, calls)) in cases {
        let mut arena = arena();
        let rig = rig(&mut arena);
        set(&mut arena, rig, free(level, filling));
        let mut host = Calls {
            replies: vec![reply],
            ..Calls::default()
        };
        arena.tick_selector52(rig, delta, &mut host).unwrap();
        let state = arena.selector52(rig).unwrap();
        assert_eq!(
            (state.level_140, state.filling_142, host.radii.len()),
            (after, still, calls),
            "{level} {filling} {delta}"
        );
        assert!(host.radii.iter().all(|&radius| radius == 0x400));
    }
    // A stopped rig does nothing.
    let mut arena = arena();
    let rig = rig(&mut arena);
    let stopped = Selector52State {
        running_138: 0,
        ..free(500, 1)
    };
    set(&mut arena, rig, stopped.clone());
    let mut host = Calls::default();
    arena.tick_selector52(rig, 125_000, &mut host).unwrap();
    assert!(host.radii.is_empty());
    assert_eq!(arena.selector52(rig).unwrap(), &stopped);
}

/// 00454720's ramps (LEA displacements 0xFFFF8002, 0xFFFF0004, 0xFFFE8006,
/// 0xFFFE4007 and 0xFFFE0008), the floor 0x1800 of selector 3 and the full
/// test of selector 9.
#[test]
fn model_values_ramp_with_the_level() {
    let at = |level, selector| {
        selector52_value(
            &Selector52State {
                running_138: 1,
                links_13c: 0,
                level_140: level,
                filling_142: 0,
            },
            selector,
        )
    };
    let table: [(u16, [u16; 10]); 7] = [
        (0, [0, 0, 0, 0x1800, 0, 0, 0, 0, 0, 0]),
        (0x2aaa, [0, 0, 0, 0xfffc, 0, 0, 0, 0, 0, 0]),
        (0x4000, [0, 0x8004, 0, 0xffff, 0, 0, 0, 0, 0, 0]),
        (0x6000, [0, 0xffff, 0x4008, 0xffff, 0, 0, 0, 0, 0, 0]),
        (0x9000, [0, 0xffff, 0xffff, 0xffff, 0x600c, 0, 0, 0, 0, 0]),
        (
            0xb000,
            [0, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0x2010, 0],
        ),
        (
            0xffff,
            [0, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0xffff, 1],
        ),
    ];
    for (level, values) in table {
        for (selector, expected) in values.into_iter().enumerate() {
            assert_eq!(
                at(level, selector as u32),
                expected,
                "{level:#x} {selector}"
            );
        }
        assert_eq!(at(level, 10), 0);
    }
    assert_eq!(at(0xa000, 5), 0x801c);
}

#[derive(serde::Deserialize)]
struct Receipts {
    updates: Vec<Update>,
}

#[derive(serde::Deserialize)]
struct Update {
    at: String,
    delta: u32,
    /// Each 0047DD20 inside: radius, code, data and the answers' sum.
    calls: Vec<[u32; 4]>,
    before: String,
    after: String,
}

fn rig_fields(hex: &str) -> (u32, Selector52State) {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        dword(0x2c),
        Selector52State {
            running_138: bytes[0x138],
            links_13c: dword(0x13c),
            level_140: u16::from_le_bytes([bytes[0x140], bytes[0x141]]),
            filling_142: bytes[0x142],
        },
    )
}

/// Three of Infest04's updates of the linked Helniazine rig: each calls the
/// mines within 2000 once (answered 2 while the nearest mine's flag 2 is set,
/// 0 after) and leaves the rig and its flags as they were.
#[test]
fn held_rig_updates_match_infest04() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../tests/fixtures/mining_rig_infest04.json"
    ))
    .unwrap();
    assert_eq!(receipts.updates.len(), 3);
    for update in &receipts.updates {
        let (flags, before) = rig_fields(&update.before);
        let mut arena = arena();
        let rig = rig(&mut arena);
        set(&mut arena, rig, before);
        let mut host = Calls {
            replies: update.calls.iter().map(|call| call[3]).collect(),
            ..Calls::default()
        };
        arena.tick_selector52(rig, update.delta, &mut host).unwrap();
        let calls: Vec<_> = host
            .radii
            .iter()
            .map(|&radius| [radius, SELECTOR52_MINE_CALL as u32, 0])
            .collect();
        let recorded: Vec<_> = update
            .calls
            .iter()
            .map(|call| [call[0], call[1], call[2]])
            .collect();
        assert_eq!(calls, recorded, "{}", update.at);
        let (after_flags, after) = rig_fields(&update.after);
        assert_eq!(after_flags, flags, "{}", update.at);
        assert_eq!(arena.selector52(rig).unwrap(), &after, "{}", update.at);
    }
}
