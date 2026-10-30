use serde::Deserialize;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
};

#[derive(Deserialize)]
struct Receipts {
    provenance: Provenance,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Provenance {
    process_null: u32,
    target_handle: u32,
}

/// One run of 00455F30, 00456010 or 00456180 (check_charge27.py).
#[derive(Deserialize)]
struct Case {
    section: String,
    mode: String,
    flags: u32,
    position: [u16; 3],
    dirty: [u8; 2],
    basis: [u32; 10],
    owner: u32,
    team: u32,
    fuse: u16,
    age: u16,
    detonate: u8,
    offset: [u16; 3],
    target_flags: u32,
    target_position: [u16; 3],
    target_basis: [u32; 10],
    target_team: u32,
    events: Vec<Event>,
    after: After,
    al: Option<u8>,
}

#[derive(Deserialize)]
struct Event {
    call: String,
    fuse: Option<u16>,
    value: Option<u8>,
}

#[derive(Deserialize)]
struct After {
    flags: u32,
    position: [u16; 3],
    dirty: [u8; 2],
    basis: [u32; 10],
    fuse: u16,
    detonate: u8,
    handle_154: u32,
    offset: [u16; 3],
}

/// The registry's null, and a handle whose owner (31) no registry has.
const NULL: ActorHandle = ActorHandle::from_raw(0);
const GONE: ActorHandle = ActorHandle::from_raw(0xf805_0001);

/// Records the update's callees, with the fuse as each finds it.
#[derive(Default)]
struct Steps {
    calls: Vec<(String, Option<u16>)>,
}

impl Charge27Steps for Steps {
    type Error = Infallible;
    fn smoke(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Infallible> {
        self.calls.push(("smoke".into(), None));
        Ok(())
    }
    fn flight(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Infallible> {
        self.calls.push(("flight".into(), None));
        Ok(())
    }
    fn fuse(&mut self, arena: &mut ObjectArena, charge: ObjectId) -> Result<(), Infallible> {
        let fuse = arena.thrown(charge).unwrap().word_13a;
        self.calls.push(("fuse".into(), Some(fuse)));
        Ok(())
    }
}

fn request(position: [u16; 3], sine: &[u16; 4096]) -> ActorConstructionRequest<'_> {
    ActorConstructionRequest {
        allocation_position: Some(position),
        owner: 1,
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: sine,
        },
    }
}

/// A charge and its target as the case supplies them.
struct Scene {
    arena: ObjectArena,
    charge: ObjectId,
    target: ObjectId,
}

fn scene(case: &Case, native: &Provenance) -> Scene {
    let sine = [0; 4096];
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 256,
        null_handle: NULL,
    })
    .unwrap();
    let target = arena
        .allocate_actor(request(case.target_position, &sine))
        .unwrap();
    let charge = arena
        .allocate_behavior_resource(request(case.position, &sine), 27)
        .unwrap();
    let target_handle = arena.actor(target).unwrap().handle;
    let handle = |raw: u32| match raw {
        raw if raw == native.target_handle => target_handle,
        raw if raw == native.process_null => NULL,
        raw => ActorHandle::from_raw(raw),
    };
    {
        let base = &mut arena.owned_mut(target).unwrap().state.base;
        base.flags = case.target_flags;
        base.position_words = Some(case.target_position);
        let actor = arena.actor_mut(target).unwrap();
        actor.rotating.basis.words = case.target_basis;
        actor.word_b4 = case.target_team;
    }
    {
        let base = &mut arena.owned_mut(charge).unwrap().state.base;
        base.flags = case.flags;
        base.position_words = Some(case.position);
        base.dirty_word = u16::from_le_bytes(case.dirty);
        let actor = arena.actor_mut(charge).unwrap();
        actor.rotating.basis.words = case.basis;
        actor.owner_handle = handle(case.owner);
        actor.word_b4 = case.team;
    }
    let stuck = match case.mode.as_str() {
        "null" => NULL,
        "live" => target_handle,
        _ => GONE,
    };
    let state = arena.thrown_mut(charge).unwrap();
    state.word_13a = case.fuse;
    state.word_13c = case.age;
    let Family455560Tail::Flags14c {
        byte_14c,
        byte_14d,
        byte_14e,
        handle_154,
        offset_158,
        ..
    } = &mut state.tail
    else {
        panic!("selector 27's tail");
    };
    *byte_14c = Some(8);
    *byte_14d = Some(0);
    *byte_14e = case.detonate;
    // The acceptance reads none of +154..+15C; the others start stuck or
    // not as the case's mode says.
    *handle_154 = Some(if case.section == "accept" {
        NULL
    } else {
        stuck
    });
    *offset_158 = Some(case.offset);
    Scene {
        arena,
        charge,
        target,
    }
}

fn tail(arena: &ObjectArena, charge: ObjectId) -> (u8, ActorHandle, Option<[u16; 3]>) {
    match &arena.thrown(charge).unwrap().tail {
        Family455560Tail::Flags14c {
            byte_14e,
            handle_154: Some(handle),
            offset_158,
            ..
        } => (*byte_14e, *handle, *offset_158),
        other => panic!("selector 27's tail, not {other:?}"),
    }
}

#[test]
fn the_charge_matches_its_native_receipts() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/charge27_native.json"
    ))
    .unwrap();
    let native = &receipts.provenance;
    let mut failures = Vec::new();
    let mut counts = [0; 3];
    for (index, case) in receipts.cases.iter().enumerate() {
        let Scene {
            mut arena,
            charge,
            target,
        } = scene(case, native);
        let target_handle = arena.actor(target).unwrap().handle;
        let mut mismatch = |what: &str, port: String, native: String| {
            if port != native {
                failures.push(format!(
                    "{index} {} {}: {what} port {port} native {native}",
                    case.section, case.mode
                ));
            }
        };
        match case.section.as_str() {
            "update" => {
                counts[0] += 1;
                let mut steps = Steps::default();
                arena.update_charge27(charge, NULL, &mut steps).unwrap();
                let expected: Vec<(String, Option<u16>)> = case
                    .events
                    .iter()
                    .filter(|event| matches!(event.call.as_str(), "smoke" | "flight" | "fuse"))
                    .map(|event| (event.call.clone(), event.fuse))
                    .collect();
                mismatch(
                    "calls",
                    format!("{:?}", steps.calls),
                    format!("{expected:?}"),
                );
                // 00479E90 was recorded, not run: its flag is bit 0.
                let flags = match case.events.iter().find(|event| event.call == "flag0") {
                    Some(event) => (case.after.flags & !1) | u32::from(event.value.unwrap() & 1),
                    None => case.after.flags,
                };
                let object = arena.object(charge).unwrap();
                mismatch(
                    "flags",
                    format!("{:08x}", object.base.flags),
                    format!("{flags:08x}"),
                );
                mismatch(
                    "position",
                    format!("{:?}", object.base.position_words),
                    format!("{:?}", Some(case.after.position)),
                );
                let state = arena.thrown(charge).unwrap();
                mismatch(
                    "fuse",
                    state.word_13a.to_string(),
                    case.after.fuse.to_string(),
                );
                let (detonate, _, _) = tail(&arena, charge);
                mismatch(
                    "+14E",
                    detonate.to_string(),
                    case.after.detonate.to_string(),
                );
            }
            "accept" => {
                counts[1] += 1;
                let al = arena.charge27_accepts(charge, target).unwrap();
                mismatch("AL", u8::from(al).to_string(), case.al.unwrap().to_string());
                let (_, handle, offset) = tail(&arena, charge);
                let stuck = case.after.handle_154 == native.target_handle;
                let expected = if stuck { target_handle } else { NULL };
                mismatch("+154", format!("{handle:?}"), format!("{expected:?}"));
                let expected = if stuck {
                    case.after.offset
                } else {
                    case.offset
                };
                mismatch(
                    "+158",
                    format!("{offset:?}"),
                    format!("{:?}", Some(expected)),
                );
                let base = &arena.object(charge).unwrap().base;
                mismatch(
                    "flags",
                    format!("{:08x}", base.flags),
                    format!("{:08x}", case.after.flags),
                );
                mismatch(
                    "+38",
                    format!("{:04x}", base.dirty_word),
                    format!("{:04x}", u16::from_le_bytes(case.after.dirty)),
                );
            }
            _ => {
                counts[2] += 1;
                arena.pose_charge27(charge, NULL).unwrap();
                let object = arena.object(charge).unwrap();
                mismatch(
                    "position",
                    format!("{:?}", object.base.position_words),
                    format!("{:?}", Some(case.after.position)),
                );
                let basis = arena.actor(charge).unwrap().rotating.basis.words;
                mismatch(
                    "basis",
                    format!("{basis:08x?}"),
                    format!("{:08x?}", case.after.basis),
                );
                let (detonate, _, _) = tail(&arena, charge);
                mismatch(
                    "+14E",
                    detonate.to_string(),
                    case.after.detonate.to_string(),
                );
            }
        }
    }
    assert_eq!(counts, [144, 240, 240]);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_young_charge_ignores_its_thrower_and_a_teammate_bounces_it() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/charge27_native.json"
    ))
    .unwrap();
    let mut outcomes = std::collections::BTreeSet::new();
    for case in receipts
        .cases
        .iter()
        .filter(|case| case.section == "accept")
    {
        let owner = case.owner == receipts.provenance.target_handle && case.age < 500;
        let team = case.team == case.target_team;
        outcomes.insert((owner, team, case.al.unwrap()));
        // The thrower first, then the team; anyone else takes the charge.
        let expected = if owner {
            1
        } else if team {
            0
        } else {
            1
        };
        assert_eq!(case.al.unwrap(), expected);
    }
    // The receipts reach every arm.
    assert!(outcomes.contains(&(true, false, 1)));
    assert!(outcomes.contains(&(false, true, 0)));
    assert!(outcomes.contains(&(false, false, 1)));
}
