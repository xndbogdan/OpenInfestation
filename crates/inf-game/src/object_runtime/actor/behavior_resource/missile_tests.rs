use serde::Deserialize;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorFollowTarget, ActorTerrainInputs,
    DerivedActorClass, NativeTarget,
};

#[derive(Deserialize)]
struct Receipts {
    updates: Vec<Record>,
    lights: Vec<Record>,
    state_zero: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    at: String,
    stack: Vec<u32>,
    rng_in: u32,
    rng_out: u32,
    entry: Fields,
    exit: Fields,
    target_in: Option<Target>,
    target_out: Option<Target>,
    calls: Vec<NativeCall>,
}

/// The missile's bytes the update reads or writes, by native offset.
#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
struct Fields {
    word_28: u16,
    flags: u32,
    position: [u16; 3],
    basis: [u32; 10],
    handle: u32,
    owner: u32,
    velocity: [u16; 3],
    angular: [u32; 3],
    health: u16,
    team: u32,
    state: u32,
    byte_d0: u8,
    byte_138: u8,
    fuse: u16,
    age: u16,
    block: [u8; 10],
    resource_148: u32,
    /// +14C and +160 are pointers; the port holds references, so only
    /// their presence is compared.
    ribbon: u32,
    byte_150: u8,
    sound: u32,
    word_158: u16,
    speed: u16,
    bytes_15c: [u8; 3],
    target: u32,
}

/// A type-2 target's 0x20 bytes: +04 handle, +08 focus, +10 kind, +14
/// point and +1A velocity.
#[derive(Clone, Copy, Deserialize, Debug, PartialEq, Eq)]
struct Target {
    handle: u32,
    focus: [u16; 3],
    kind: u32,
    point: [u16; 3],
    velocity: [u16; 3],
}

#[derive(Deserialize)]
struct NativeCall {
    name: String,
    ecx: u32,
    edx: u32,
    stack: Vec<u32>,
    point: Option<[u16; 3]>,
    made: Option<Target>,
}

/// The services the update reaches, recorded.
struct Host<'a> {
    terrain: &'a TerrainRuntime,
    inverse_sine: [i16; 1024],
    calls: Vec<String>,
}

impl MissileHost for Host<'_> {
    fn terrain(&mut self, _: &mut ObjectArena) -> Result<&TerrainRuntime, Infallible> {
        Ok(self.terrain)
    }
    fn create_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<ObjectId>, Infallible> {
        self.calls.push("486600".into());
        Ok(Some(arena.allocate_selector67(ActorHandle::from_raw(0))))
    }
    fn extend_ribbon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), Infallible> {
        self.calls.push(format!("4869b0 {delta:#x} {point:x?}"));
        Ok(())
    }
    fn settings_7c(&self) -> u32 {
        1
    }
    fn inverse_sine(&self) -> &[i16; 1024] {
        &self.inverse_sine
    }
    fn process_null(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
    fn release_sound(
        &mut self,
        _: crate::process_sound::ProcessSoundRef,
    ) -> Result<(), Infallible> {
        self.calls.push("487fe0".into());
        Ok(())
    }
}

impl crate::object_runtime::StatusTrailHost for Host<'_> {
    type Error = Infallible;
    fn status_load_factor(&self) -> i32 {
        0x10000
    }
    fn status_trail(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::FamilyUpdateEffect,
        _: &mut u32,
    ) -> Result<(), Infallible> {
        self.calls.push("489d90 trail".into());
        Ok(())
    }
}

impl MissileLifecycleHost for Host<'_> {
    type Error = Infallible;
    fn detonate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Infallible> {
        self.calls.push("47d670".into());
        Ok(())
    }
    fn burst(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        block: [u8; 10],
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Infallible> {
        self.calls.push(format!("4887d0 {block:x?}"));
        Ok(())
    }
    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        _: &mut u32,
    ) -> Result<(), Infallible> {
        self.calls.push(format!("48a0f0 {position:x?}"));
        Ok(())
    }
    fn flash(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
        _: &mut u32,
    ) -> Result<(), Infallible> {
        self.calls.push(format!("487d70 {position:x?} {radius:#x}"));
        Ok(())
    }
}

fn inverse_sine_table(executable: inf_formats::FrontendExecutable<'_>) -> [i16; 1024] {
    let bytes = executable.mapped(0x0053_ff00, 2048).unwrap().bytes;
    std::array::from_fn(|index| i16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]]))
}

fn missile(arena: &mut ObjectArena, entry: &Fields) -> ObjectId {
    let sine = [0; 4096];
    let missile = arena
        .allocate_behavior_resource(
            ActorConstructionRequest {
                allocation_position: Some(entry.position),
                owner: 1,
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: Some(0),
                    sine_quarter: &sine,
                },
            },
            25,
        )
        .unwrap();
    {
        let base = &mut arena.owned_mut(missile).unwrap().state.base;
        base.word_28 = entry.word_28;
        base.flags = entry.flags;
        base.position_words = Some(entry.position);
    }
    let actor = arena.actor_mut(missile).unwrap();
    actor.rotating.basis.words = entry.basis;
    actor.words_8e_92 = entry.velocity;
    actor.words_94_9c = entry.angular;
    actor.health = entry.health;
    actor.word_b4 = entry.team;
    actor.word_c4 = entry.state;
    actor.byte_d0 = entry.byte_d0;
    let state = arena.missile_mut(missile).unwrap();
    state.byte_138 = entry.byte_138;
    state.word_13a = entry.fuse;
    state.word_13c = entry.age;
    state.bytes_13e = Some(entry.block);
    state.dword_148 = entry.resource_148;
    missile
}

fn seed_tail(
    arena: &mut ObjectArena,
    missile: ObjectId,
    entry: &Fields,
    ribbon: Option<ObjectId>,
    target: Option<NativeHeapRef>,
) {
    match &mut arena.missile_mut(missile).unwrap().tail {
        Family455560Tail::Selector25 {
            ribbon_14c,
            byte_150,
            sound_154,
            word_158,
            word_15a,
            byte_15c,
            byte_15d,
            byte_15e,
            target_160,
        } => {
            *ribbon_14c = ribbon;
            *byte_150 = entry.byte_150;
            // No selector 25 missile is steered, so none has a sound.
            assert_eq!(entry.sound, 0);
            *sound_154 = None;
            *word_158 = Some(entry.word_158);
            *word_15a = entry.speed;
            *byte_15c = entry.bytes_15c[0];
            *byte_15d = entry.bytes_15c[1];
            *byte_15e = entry.bytes_15c[2];
            *target_160 = target;
        }
        other => panic!("selector 25's tail, not {other:?}"),
    }
}

/// The exit with each identity as its presence, and the handles replaced
/// by the recording's.
fn read(arena: &ObjectArena, missile: ObjectId, recorded: &Fields) -> Fields {
    let object = arena.object(missile).unwrap();
    let actor = arena.actor(missile).unwrap();
    let state = arena.missile(missile).unwrap();
    let Family455560Tail::Selector25 {
        ribbon_14c,
        byte_150,
        sound_154,
        word_158,
        word_15a,
        byte_15c,
        byte_15d,
        byte_15e,
        target_160,
    } = &state.tail
    else {
        panic!("selector 25's tail");
    };
    Fields {
        word_28: object.base.word_28,
        flags: object.base.flags,
        position: object.base.position_words.unwrap(),
        basis: actor.rotating.basis.words,
        handle: recorded.handle,
        owner: recorded.owner,
        velocity: actor.words_8e_92,
        angular: actor.words_94_9c,
        health: actor.health,
        team: actor.word_b4,
        state: actor.word_c4,
        byte_d0: actor.byte_d0,
        byte_138: state.byte_138,
        fuse: state.word_13a,
        age: state.word_13c,
        block: state.bytes_13e.unwrap(),
        resource_148: state.dword_148,
        ribbon: u32::from(ribbon_14c.is_some()),
        byte_150: *byte_150,
        sound: u32::from(sound_154.is_some()),
        word_158: word_158.unwrap(),
        speed: *word_15a,
        bytes_15c: [*byte_15c, *byte_15d, *byte_15e],
        target: u32::from(target_160.is_some()),
    }
}

/// A follow target's recorded bytes on `handle`.
fn follow(target: &Target, handle: ActorHandle) -> ActorFollowTarget {
    ActorFollowTarget {
        handle_04: handle,
        focus_08: Some(target.focus),
        word_10: target.kind,
        focus_14: Some(target.point),
        velocity_1a: Some(target.velocity),
    }
}

/// Every update 00458510 of Infest03's three homing missiles, fired by
/// selector 11's launchers in world 23, each run from its captured entry on
/// the loaded terrain. The world's vehicle stands in for their target and
/// one of its selector 11 actors for the launcher, so handles compare by
/// role. Two missiles inherit the launcher's type-2 target on their first
/// update (004583A0) and home on it until a close pass spends the fuse;
/// the third finds nobody in any scan (every other actor's scan flags are
/// cleared, the recording's world not being the loaded one) and flies
/// straight. The fields, the target's refresh, the ribbon's points and the
/// detonations match.
#[test]
fn missile25_update_matches_every_recorded_tick() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/missile25_updates.json"
    ))
    .unwrap();
    let (_resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        23,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let inverse_sine = inverse_sine_table(executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let launcher = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            world
                .arena
                .derived_actor(actor)
                .is_ok_and(|derived| matches!(derived.class, DerivedActorClass::Selector11(_)))
        })
        .expect("a selector 11 in world 23");
    let vehicle_handle = world.arena.actor(vehicle).unwrap().handle;
    let launcher_handle = world.arena.actor(launcher).unwrap().handle;
    let mut failures = Vec::new();
    for update in &receipts.updates {
        let arena = &mut world.arena;
        let missile = missile(arena, &update.entry);
        arena.actor_mut(missile).unwrap().owner_handle = launcher_handle;
        let ribbon =
            (update.entry.ribbon != 0).then(|| arena.allocate_selector67(ActorHandle::from_raw(0)));
        let target = update.target_in.map(|target| {
            arena.import_fixture_target(NativeTarget::ActorFollow(follow(&target, vehicle_handle)))
        });
        seed_tail(arena, missile, &update.entry, ribbon, target);
        // The launcher's own target, which an acquisition clones.
        let made = update.calls.iter().find_map(|call| call.made);
        let held = made.map(|made| {
            arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
                focus_08: None,
                velocity_1a: None,
                ..follow(&made, vehicle_handle)
            }))
        });
        arena.derived_actor_mut(launcher).unwrap().target_148 = held;
        // The refresh finds the vehicle where the recording's found it.
        if let Some(out) = update.target_out {
            let lift = arena.actor(vehicle).unwrap().word_74;
            arena.place_for_tests(
                vehicle,
                [out.focus[0], out.focus[1].wrapping_sub(lift), out.focus[2]],
            );
            let negate = arena.object(vehicle).unwrap().base.flags & 0x0004_0000 != 0;
            arena.actor_mut(vehicle).unwrap().words_8e_92 = if negate {
                out.velocity.map(u16::wrapping_neg)
            } else {
                out.velocity
            };
        }
        // Without an inherited target the scan finds nobody.
        let saved_flags: Vec<(ObjectId, u32)> = if made.is_none() {
            let others: Vec<ObjectId> = arena
                .actor_list()
                .iter()
                .copied()
                .filter(|&actor| actor != missile)
                .collect();
            others
                .into_iter()
                .map(|actor| {
                    let base = &mut arena.owned_mut(actor).unwrap().state.base;
                    let flags = base.flags;
                    base.flags &= !0x24;
                    (actor, flags)
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut host = Host {
            terrain: &world.terrain,
            inverse_sine,
            calls: Vec::new(),
        };
        let mut rng = update.rng_in;
        let delta = update.stack[0];
        let result = arena.tick_missile25(missile, delta, &sine, &mut rng, &mut host);
        for (actor, flags) in saved_flags {
            arena.owned_mut(actor).unwrap().state.base.flags = flags;
        }
        if let Err(error) = result {
            failures.push(format!("{}: {error}", update.at));
            continue;
        }
        let detonated = update.calls.iter().any(|call| call.name == "455680");
        let mut exit = update.exit.clone();
        exit.ribbon = u32::from(exit.ribbon != 0);
        exit.target = u32::from(exit.target != 0);
        let mut actual = read(arena, missile, &update.exit);
        if detonated {
            // States 1 and 0 (the recording host's 0047D670) belong to the
            // lifecycle receipts.
            for fields in [&mut exit, &mut actual] {
                fields.flags = 0;
                fields.state = 0;
                fields.health = 0;
            }
        }
        if actual != exit {
            failures.push(format!("{}: {actual:?}\n   != {exit:?}", update.at));
        }
        // The target after the update: the clone or the seeded one,
        // refreshed.
        let target_after = match &arena.missile(missile).unwrap().tail {
            Family455560Tail::Selector25 { target_160, .. } => *target_160,
            _ => unreachable!(),
        };
        let port_target = target_after.map(|target| {
            let data = arena.actor_follow_target(target).unwrap();
            (data.focus_08, data.word_10, data.focus_14, data.velocity_1a)
        });
        let native_target = update.target_out.map(|target| {
            (
                Some(target.focus),
                target.kind,
                Some(target.point),
                Some(target.velocity),
            )
        });
        if port_target != native_target {
            failures.push(format!(
                "{}: target {port_target:x?} != {native_target:x?}",
                update.at
            ));
        }
        // The services reached: the missile's own ribbon (00486600 with its
        // handle), each point (004869B0) and the detonation.
        let missile_handle = update.entry.handle;
        let mut expected: Vec<String> = update
            .calls
            .iter()
            .filter_map(|call| match call.name.as_str() {
                "486600" if call.stack[0] == missile_handle => Some("486600".to_string()),
                "4869b0" => Some(format!(
                    "4869b0 {:#x} {:x?}",
                    call.stack[0],
                    call.point.unwrap()
                )),
                "455680" => Some("47d670".to_string()),
                _ => None,
            })
            .collect();
        let mut reached = host.calls.clone();
        if detonated {
            // The recording host's detonation runs no state change, so the
            // native order inside the lifecycle is not compared.
            reached.sort();
            expected.sort();
        }
        if reached != expected {
            failures.push(format!("{}: calls {reached:?} != {expected:?}", update.at));
        }
        if rng != update.rng_out && !detonated {
            failures.push(format!(
                "{}: rng {rng:#x} != {:#x}",
                update.at, update.rng_out
            ));
        }
        if let Some(target) = target_after {
            arena.release_actor_service(target).unwrap();
        }
        if let Some(target) = arena.derived_actor_mut(launcher).unwrap().target_148.take() {
            arena.release_actor_service(target).unwrap();
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} updates differ:\n{}",
        failures.len(),
        receipts.updates.len(),
        failures.join("\n")
    );
    assert_eq!(receipts.updates.len(), 28);
}

/// Every light 00457EE0 of those missiles: within 1500 of the ground cell,
/// red falling with the height and green and blue wrapping below zero.
#[test]
fn missile_light_matches_every_recorded_call() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/missile25_updates.json"
    ))
    .unwrap();
    let (_resident, _assets, mut world) = crate::test_worlds::offline_world_retained(
        23,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let mut failures = Vec::new();
    for light in &receipts.lights {
        let missile = missile(&mut world.arena, &light.entry);
        let port = world
            .arena
            .missile_light(missile, &world.terrain)
            .unwrap()
            .map(|light| {
                (
                    light.position_xz.map(|value| value as u32),
                    light.radius as u32,
                    light.rgb,
                )
            });
        let native = light
            .calls
            .iter()
            .find(|call| call.name == "45b1a0")
            .map(|call| {
                let rgb = call.stack[1].to_le_bytes();
                (
                    [call.ecx, call.edx],
                    call.stack[0],
                    [rgb[0], rgb[1], rgb[2]],
                )
            });
        if port != native {
            failures.push(format!("{}: {port:x?} != {native:x?}", light.at));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(receipts.lights.len(), 26);
}

/// Both detonations 00455680: the missile's own block from its launcher,
/// the explosion where it is, and its disposal.
#[test]
fn missile_detonation_matches_both_recorded_calls() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/missile25_updates.json"
    ))
    .unwrap();
    let (_resident, _assets, mut world) = crate::test_worlds::offline_world_retained(
        23,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    for record in &receipts.state_zero {
        let arena = &mut world.arena;
        let missile = missile(arena, &record.entry);
        let mut host = Host {
            terrain: &world.terrain,
            inverse_sine: [0; 1024],
            calls: Vec::new(),
        };
        let mut rng = record.rng_in;
        arena
            .detonate_missile(missile, &mut rng, &mut host)
            .unwrap();
        assert_eq!(
            host.calls,
            [
                format!("4887d0 {:x?}", record.entry.block),
                format!("48a0f0 {:x?}", record.entry.position),
            ],
            "{}",
            record.at
        );
        let names: Vec<&str> = record.calls.iter().map(|call| call.name.as_str()).collect();
        assert!(
            names.contains(&"4887d0") && names.contains(&"479fa0"),
            "{names:?}"
        );
        assert_eq!(rng, record.rng_in);
        // 00479FA0's flag writes.
        assert_eq!(
            arena.object(missile).unwrap().base.flags,
            record.exit.flags,
            "{}",
            record.at
        );
        let _ = record.rng_out;
    }
    assert_eq!(receipts.state_zero.len(), 2);
}

#[derive(Deserialize)]
struct ScoreCases {
    cases: Vec<ScoreCase>,
}

#[derive(Deserialize)]
struct ScoreCase {
    process_null: u32,
    forward: [f32; 3],
    missile_position: [u16; 3],
    missile_owner: u32,
    missile_side: u32,
    flags_15c: [u8; 2],
    candidate_position: [u16; 3],
    candidate_handle: u32,
    candidate_side: u32,
    candidate_lifecycle: u32,
    vehicle_mode: u32,
    vehicle: bool,
    ground: i16,
    score: u32,
}

/// 004580B0 against the evaluator itself (Unicorn, 257 cases from
/// `check_missile_score.py`): every early out (lifecycle, the launcher, the
/// side, the altitude with and without +15C/+15D, a vehicle in modes 4 and
/// 5), behind the missile, the 60-degree cone, the lateral square and the
/// process null's quarter. The world's vehicle stands in for the vehicle
/// candidates and a grenade (whose +64 is null) for the rest; handles map
/// by role.
#[test]
fn missile25_score_matches_the_native_evaluator() {
    let fixture: ScoreCases = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/missile_scores_oracle.json"
    ))
    .unwrap();
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let blank = Fields {
        word_28: 0,
        flags: 0,
        position: [0; 3],
        basis: inf_formats::native_basis::NativeBasis::INITIAL.words,
        handle: 0,
        owner: 0,
        velocity: [0; 3],
        angular: [0; 3],
        health: 1,
        team: 0,
        state: 3,
        byte_d0: 0,
        byte_138: 1,
        fuse: 10_000,
        age: 0,
        block: [0; 10],
        resource_148: 0,
        ribbon: 0,
        byte_150: 0,
        sound: 0,
        word_158: 0,
        speed: 3000,
        bytes_15c: [1, 1, 0],
        target: 0,
    };
    let missile = missile(arena, &blank);
    let sine = [0; 4096];
    let grenade = arena
        .allocate_behavior_resource(
            ActorConstructionRequest {
                allocation_position: Some([0; 3]),
                owner: 1,
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: Some(0),
                    sine_quarter: &sine,
                },
            },
            26,
        )
        .unwrap();
    let other = ActorHandle::from_raw(0x7e57_0001);
    let mut failures = Vec::new();
    for (index, case) in fixture.cases.iter().enumerate() {
        let candidate = if case.vehicle { vehicle } else { grenade };
        let handle = arena.actor(candidate).unwrap().handle;
        let by_role = |recorded: u32| {
            if recorded == case.candidate_handle {
                handle
            } else {
                other
            }
        };
        arena.place_for_tests(missile, case.missile_position);
        {
            let actor = arena.actor_mut(missile).unwrap();
            actor.owner_handle = by_role(case.missile_owner);
            actor.word_b4 = case.missile_side;
            for (axis, value) in case.forward.iter().enumerate() {
                actor.rotating.basis.words[7 + axis] = value.to_bits();
            }
        }
        match &mut arena.missile_mut(missile).unwrap().tail {
            Family455560Tail::Selector25 {
                byte_15c, byte_15d, ..
            } => {
                *byte_15c = case.flags_15c[0];
                *byte_15d = case.flags_15c[1];
            }
            _ => unreachable!(),
        }
        arena.place_for_tests(candidate, case.candidate_position);
        {
            let actor = arena.actor_mut(candidate).unwrap();
            actor.word_b4 = case.candidate_side;
            actor.word_c4 = case.candidate_lifecycle;
        }
        if case.vehicle {
            arena.selector2_actor_mut(candidate).unwrap().dwords_1e8_1f4[0] = case.vehicle_mode;
        }
        let process_null = if case.process_null == case.candidate_handle {
            handle
        } else {
            ActorHandle::from_raw(0x0bad_0001)
        };
        let port = arena
            .missile25_candidate_score(missile, candidate, case.ground, process_null)
            .unwrap();
        if port != case.score {
            failures.push(format!("case {index}: {port:#x} != {:#x}", case.score));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(fixture.cases.len(), 257);
}

/// `00457660`, another machine's missile: `0047EBF0` moves it (`0047B1E0`
/// with mask 0x18), then its ribbon: a new one (`00486600`) takes three
/// points, from a quarter of the velocity behind in eighths, and one it has
/// takes the position.
#[test]
fn a_remote_missile_drifts_and_lays_its_ribbon_as_00457660() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/missile25_updates.json"
    ))
    .unwrap();
    let (_resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        23,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let mut entry = receipts.updates[0].entry.clone();
    entry.flags &= !8;
    entry.velocity = [0x40, 0xffc0, 0x1b];
    let arena = &mut world.arena;
    let remote = missile(arena, &entry);
    seed_tail(arena, remote, &entry, None, None);
    let twin = missile(arena, &entry);
    seed_tail(arena, twin, &entry, None, None);
    let mut host = Host {
        terrain: &world.terrain,
        inverse_sine: [0; 1024],
        calls: Vec::new(),
    };
    let mut rng = 1;
    arena
        .tick_remote_missile(remote, 0x1000, &sine, &mut rng, &mut host)
        .unwrap();
    arena
        .tick_actor_physics(
            twin,
            crate::object_runtime::ActorPhysicsRequest {
                microseconds: 0x1000,
                mask: 0x18,
                sine_quarter: &sine,
            },
        )
        .unwrap();
    let position = arena.object(twin).unwrap().base.position_words.unwrap();
    assert_eq!(
        arena.object(remote).unwrap().base.position_words,
        Some(position)
    );
    let velocity = arena.actor(remote).unwrap().words_8e_92;
    let behind: [u16; 3] = std::array::from_fn(|axis| {
        position[axis].wrapping_sub(((i32::from(velocity[axis] as i16) * 2) >> 3) as u16)
    });
    let step = |point: [u16; 3]| -> [u16; 3] {
        std::array::from_fn(|axis| point[axis].wrapping_add(((velocity[axis] as i16) >> 3) as u16))
    };
    let points = [behind, step(behind), step(step(behind))];
    let mut expected = vec!["486600".to_string()];
    expected.extend(points.map(|point| format!("4869b0 0x1000 {point:x?}")));
    assert_eq!(host.calls, expected);
    host.calls.clear();
    arena
        .tick_remote_missile(remote, 0x1000, &sine, &mut rng, &mut host)
        .unwrap();
    let position = arena.object(remote).unwrap().base.position_words.unwrap();
    assert_eq!(host.calls, [format!("4869b0 0x1000 {position:x?}")]);
}
