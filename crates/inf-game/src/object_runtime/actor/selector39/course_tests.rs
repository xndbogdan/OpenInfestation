//! Level 1's course targets against Infest02 (`targetdeath`, `fuse` and
//! `bridge` queries). Targets 0x14, 0x13 and 0x12 (placements 98, 97, 96)
//! die in that order: each sends message 8 to the bridge (placement 106),
//! marks its death, stops the countdown and lights a 30 s fuse, and the
//! third opens the bridge (state 2). Target 0x14's fuse then runs 240
//! updates of 125 ms: it sounds on the 239th, as it passes 0.2 s, and on the
//! 240th the target comes back with message 7, which closes the bridge.

use super::*;
use crate::{
    object_runtime::{
        ActorDamageReactionEffect, ActorHandle, ActorStateChangeEffect, ActorStateChangeHost,
        ActorStateChangeQuery,
    },
    terrain_runtime::TerrainSpacing,
    test_worlds::offline_world,
};

const STEP: u32 = 0x1e848;

/// Applies the bridge's lifecycle change, skipping the session's
/// attribution, and records every other service.
struct Course {
    changes: Vec<(ObjectId, i32)>,
    sounds: Vec<[u16; 3]>,
    marks: Vec<i8>,
    commands: Vec<(u32, u8)>,
    grants: Vec<u16>,
    writes: Vec<Selector39MissionWrite>,
    status: u32,
    /// The generators' services, in order.
    calls: Vec<String>,
    radiation: Vec<(ObjectId, ActorTimedDamageRequest)>,
    math: NativeBasisMath,
    rng: u32,
    countdown: Selector39Countdown,
    /// Whether `retire` runs the state-1 prefix as 0047D670 would.
    retire_kills: bool,
}

impl Default for Course {
    fn default() -> Self {
        Self {
            changes: Vec::new(),
            sounds: Vec::new(),
            marks: Vec::new(),
            commands: Vec::new(),
            grants: Vec::new(),
            writes: Vec::new(),
            status: 0,
            calls: Vec::new(),
            radiation: Vec::new(),
            math: NativeBasisMath::new([0; 4096]),
            rng: 0x1234_5678,
            countdown: Selector39Countdown {
                held: 0,
                remaining: 0,
            },
            retire_kills: true,
        }
    }
}

struct Lifecycle;

impl ActorStateChangeHost for Lifecycle {
    type Error = String;
    type AttributionOwner = ();
    fn query(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        Ok(matches!(
            query,
            ActorStateChangeQuery::ExcludesAttribution68
        ))
    }
    fn attribute(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
    ) -> Result<(), String> {
        Err("attribution is excluded here".into())
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        Err(format!("unexpected lifecycle effect {effect:?}"))
    }
}

impl Selector39Host for Course {
    type Error = String;
    fn link_change(
        &mut self,
        arena: &mut ObjectArena,
        delivery: Selector39LinkDelivery,
    ) -> Result<(), String> {
        let request = match delivery.change {
            FamilyLinkChange::Lock(request) | FamilyLinkChange::Unlock(request) => request,
            FamilyLinkChange::None => unreachable!("only changes reach the host"),
            // The barrier's last link: no grant on Level 1, then 00479FA0.
            FamilyLinkChange::Release { grant } => {
                assert_eq!(grant, 0);
                self.changes.push((delivery.target, delivery.code));
                return arena
                    .dispose(delivery.target)
                    .map_err(|error| format!("{error:?}"));
            }
        };
        self.changes.push((delivery.target, delivery.code));
        arena
            .change_actor_state(delivery.target, request, &mut Lifecycle)
            .map_err(|error| format!("{error:?}"))
    }
    fn fuse_sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        self.sounds.push(position);
        Ok(())
    }
}

impl Selector39DeathHost for Course {
    fn mission_write(&mut self, write: Selector39MissionWrite) -> Result<(), String> {
        self.writes.push(write);
        Ok(())
    }
    fn mark_death(&mut self, _: &mut ObjectArena, index: i8) -> Result<(), String> {
        self.marks.push(index);
        Ok(())
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
    fn grant(&mut self, _: &mut ObjectArena, _: PlayerId, id: u16) -> Result<(), String> {
        self.grants.push(id);
        Ok(())
    }
    fn scorch(&mut self, _: &mut ObjectArena, _: ObjectId, argument: i32) -> Result<(), String> {
        self.calls.push(format!("scorch {argument}"));
        Ok(())
    }
}

impl Selector39UpdateHost for Course {
    fn status(&mut self, _: &mut ObjectArena, _: ObjectId, _: u32) -> Result<(), String> {
        self.status += 1;
        Ok(())
    }
    fn radiation(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), String> {
        self.radiation.push((actor, request));
        Ok(())
    }
    fn radiation_notice(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
        distance_squared: u32,
    ) -> Result<(), String> {
        self.calls.push(format!("notice {distance_squared}"));
        Ok(())
    }
    fn retire(&mut self, arena: &mut ObjectArena, target: ObjectId) -> Result<(), String> {
        self.calls.push("retire".into());
        if self.retire_kills {
            kill(arena, target);
        }
        Ok(())
    }
    fn status_visual(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        delta: u32,
    ) -> Result<(), String> {
        self.calls.push(format!("smoke {delta}"));
        Ok(())
    }
    fn child_visual(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        resource: u32,
    ) -> Result<(), String> {
        self.calls.push(format!("visual {resource:#x}"));
        Ok(())
    }
    fn burst(&mut self, _: &mut ObjectArena, burst: ParticleBurst) -> Result<(), String> {
        self.calls.push(format!("burst {:?}", burst.position));
        Ok(())
    }
    fn blast(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), String> {
        self.calls.push("blast".into());
        Ok(())
    }
    fn refresh_model(&mut self, arena: &mut ObjectArena, target: ObjectId) -> Result<(), String> {
        let model = arena.actor(target).unwrap().rotating.visual.model_word;
        self.calls.push(format!("model {model:#x}"));
        Ok(())
    }
    fn random(&mut self) -> u32 {
        crate::name_entry::name_entry_rng_next(&mut self.rng)
    }
    fn basis_math(&self) -> &NativeBasisMath {
        &self.math
    }
    fn countdown(&mut self) -> Result<Selector39Countdown, String> {
        Ok(self.countdown)
    }
}

/// 0047D670's state-1 prefix before the +AC: state 1, health 0, flag 8000.
fn kill(arena: &mut ObjectArena, target: ObjectId) {
    let actor = arena.actor_mut(target).unwrap();
    actor.word_c4 = 1;
    actor.health = 0;
    arena.owned_mut(target).unwrap().state.base.flags |= 0x8000;
}

#[test]
fn level1_course_targets_open_and_close_the_bridge_as_infest02s() {
    let (_resident, _assets, mut world) = offline_world(18, TerrainSpacing::Units256);
    let placement = |index: usize| world.report.placements[index].unwrap();
    let targets = [placement(98), placement(97), placement(96)];
    let bridge = placement(106);
    let arena = &mut world.arena;
    let bridge_state = |arena: &ObjectArena| {
        (
            arena.actor(bridge).unwrap().word_c4,
            arena.selector37(bridge).unwrap().intermediate.byte_14c,
        )
    };
    assert_eq!(bridge_state(arena), (3, 3));
    let mut course = Course::default();
    for (count, &target) in targets.iter().enumerate() {
        let state = arena.selector39(target).unwrap();
        assert_eq!(
            [
                state.byte_160,
                state.byte_161,
                state.byte_162,
                state.byte_163,
                state.byte_165
            ],
            [30, 0, 0, 1, 1]
        );
        kill(arena, target);
        arena.retire_selector39(target, &mut course).unwrap();
        assert_eq!(
            arena.selector39(target).unwrap().intermediate.word_150,
            0x1c9_c380
        );
        assert_eq!(bridge_state(arena).1, 2 - count as u8);
    }
    // The third death opens the bridge.
    assert_eq!(bridge_state(arena), (2, 0));
    assert_eq!(course.changes, [(bridge, 8)]);
    assert_eq!(course.marks, [0x14, 0x13, 0x12]);
    assert_eq!(course.commands, [(0x39, 1), (0x3a, 1), (0x3b, 1)].repeat(3));
    assert_eq!(course.writes, [Selector39MissionWrite::StopCountdown; 3]);
    // The other four targets of the same kind live, so no grant.
    assert!(course.grants.is_empty());
    let positions: Vec<_> = targets
        .iter()
        .map(|&target| arena.object(target).unwrap().base.position_words.unwrap())
        .collect();
    assert_eq!(course.sounds, positions);
    assert!(course.radiation.is_empty() && course.calls.is_empty());
    // Target 0x14's fuse: 240 updates.
    let target = targets[0];
    let maximum = arena.actor(target).unwrap().maximum_health.unwrap();
    assert_eq!(maximum, 0xc8);
    for update in 1..=240 {
        arena.tick_selector39(target, STEP, &mut course).unwrap();
        let fuse = arena.selector39(target).unwrap().intermediate.word_150;
        assert_eq!(fuse, 0x1c9_c380 - update * STEP, "update {update}");
        assert_eq!(course.sounds.len(), if update < 239 { 3 } else { 4 });
        let state = arena.actor(target).unwrap().word_c4;
        assert_eq!(state, if update < 240 { 1 } else { 3 }, "update {update}");
    }
    assert_eq!(course.status, 240);
    assert_eq!(course.sounds[3], positions[0]);
    let actor = arena.actor(target).unwrap();
    assert_eq!(actor.health, maximum);
    assert_eq!(arena.object(target).unwrap().base.flags & 0x8000, 0);
    // Its message 7 closes the bridge again.
    assert_eq!(bridge_state(arena), (3, 1));
    assert_eq!(course.changes, [(bridge, 8), (bridge, 7)]);
    // A fuse at zero only runs the status update.
    arena.tick_selector39(target, STEP, &mut course).unwrap();
    assert_eq!(arena.actor(target).unwrap().word_c4, 3);
    assert_eq!(course.status, 241);
}

/// Every reaction service refuses: reaching 0040E030 shows as its +CA/+C8
/// writes followed by the refusal.
struct Refuse;

impl ActorDamageReactionHost for Refuse {
    type Error = ();
    type RecentSource = ObjectId;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        None
    }
    fn network_active(&mut self) -> Result<bool, ()> {
        Err(())
    }
    fn difficulty_multiplier(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<i16, ()> {
        Err(())
    }
    fn source_bonuses(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<[i8; 6]>, ()> {
        Err(())
    }
    fn animation_tick(&mut self) -> Result<u32, ()> {
        Err(())
    }
    fn network_role(&mut self) -> Result<i32, ()> {
        Err(())
    }
    fn local_player_byte(&mut self) -> Result<u8, ()> {
        Err(())
    }
    fn excludes_status(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<bool, ()> {
        Err(())
    }
    fn recent_damage_source(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<ObjectId>, ()> {
        Err(())
    }
    fn recent_source_tick(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<u32, ()> {
        Err(())
    }
    fn recent_source_owner_word(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<u16, ()> {
        Err(())
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorDamageReactionEffect,
        _: &mut u32,
    ) -> Result<(), ()> {
        Err(())
    }
}

#[test]
fn course_target_damage_guard_follows_0043bd90() {
    let (_resident, _assets, mut world) = offline_world(18, TerrainSpacing::Units256);
    let target = world.report.placements[98].unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let null = arena.actor_handles().unwrap().null_handle();
    let callback = |channel, source| ActorDamageCallback {
        adjusted_damage: 50,
        packet: ActorDamagePacket {
            strength: 50,
            channel,
            byte_03: None,
        },
        source,
    };
    let reached = |arena: &ObjectArena| arena.actor(target).unwrap().word_ca == Some(55);
    let mut rng = 0;
    // The vehicle's collision (resource word 0) takes nothing.
    let source = arena.actor(vehicle).unwrap().handle;
    assert_ne!(arena.object(vehicle).unwrap().base.resource_word, 0x29);
    arena
        .react_selector39_damage(target, callback(1, source), &mut rng, &mut Refuse)
        .unwrap();
    assert!(!reached(arena));
    // A shot, or a collision with no source, reaches the family reaction.
    for (channel, source) in [(3, source), (1, null)] {
        arena.actor_mut(target).unwrap().word_ca = None;
        assert!(matches!(
            arena.react_selector39_damage(target, callback(channel, source), &mut rng, &mut Refuse),
            Err(ActorDamageReactionError::Host(()))
        ));
        assert!(reached(arena), "channel {channel}");
    }
    // A collision from resources 0x29..0x2C counts.
    arena.owned_mut(vehicle).unwrap().state.base.resource_word = 0x2b;
    arena.actor_mut(target).unwrap().word_ca = None;
    assert!(arena
        .react_selector39_damage(target, callback(1, source), &mut rng, &mut Refuse)
        .is_err());
    assert!(reached(arena));
    // +161 or +166 refuses everything.
    for byte in [0, 1] {
        arena.actor_mut(target).unwrap().word_ca = None;
        let state = arena.selector39_mut(target).unwrap();
        if byte == 0 {
            state.byte_161 = 1;
        } else {
            state.byte_161 = 0;
            state.byte_166 = 1;
        }
        arena
            .react_selector39_damage(target, callback(3, source), &mut rng, &mut Refuse)
            .unwrap();
        assert!(!reached(arena));
    }
}

/// World 11's generators, which Infest03 found idle (`generator-life.js`):
/// resources 0x12 (model 0x2E) and 0x13 (model 0x1A7), no countdown, the
/// radiation on. Within 1000 of the player one irradiates the player's
/// actor, `(1200 - distance) * 4` on channel 3 for 2 s, and sends notice
/// 0x17. Shot to death, it scorches six cells and burns 2 s (`0040DB40`
/// smokes each 125 ms update), then blows up: visuals 0x36 and 0x37, the
/// burst at its position, the blast, the wreck model 0x46 and a random
/// turn, and it leaves the update list. The generator its links hold up
/// (+14D) dies once their count +14C reaches 0.
#[test]
fn a_generator_burns_then_blows_up_into_its_wreck() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world_retained(11, TerrainSpacing::Units256);
    let arena = &mut world.arena;
    let generators: Vec<_> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector39(actor).is_ok())
        .collect();
    // The four 0x12 generators need grant 48 (command 13), which the new
    // player lacks, so the three 0x13 ones stand.
    let resources: Vec<_> = generators
        .iter()
        .map(|&actor| arena.object(actor).unwrap().base.resource_word)
        .collect();
    assert_eq!(resources, [0x13; 3]);
    let target = generators[1];
    assert_eq!(
        arena.object(target).unwrap().base.position_words,
        Some([62475, 0, 29942])
    );
    assert_eq!(
        arena.actor(target).unwrap().rotating.visual.model_word,
        0x1a7
    );
    let state = arena.selector39(target).unwrap();
    assert_eq!(
        [
            state.byte_160,
            state.byte_161,
            state.byte_163,
            state.byte_165
        ],
        [0, 0, 0, 0]
    );
    let position = arena.object(target).unwrap().base.position_words.unwrap();

    // The player's actor 600 away in X.
    let actor = arena.first_player_primary_actor().unwrap().unwrap();
    let saved = arena.object(actor).unwrap().base.position_words;
    arena.owned_mut(actor).unwrap().state.base.position_words =
        Some([position[0].wrapping_add(600), position[1], position[2]]);
    let mut course = Course::default();
    arena.tick_selector39(target, STEP, &mut course).unwrap();
    assert_eq!(course.radiation.len(), 1);
    let (hit, request) = course.radiation[0];
    assert_eq!(hit, actor);
    assert_eq!(
        (request.packet.strength, request.packet.channel),
        ((1200 - 600) * 4, 3)
    );
    assert_eq!(request.period_microseconds, 2_000_000);
    assert_eq!(course.calls, ["notice 360000"]);
    assert_eq!(course.status, 1);
    arena.owned_mut(actor).unwrap().state.base.position_words = saved;

    // Shot to death.
    let mut course = Course::default();
    kill(arena, target);
    arena.retire_selector39(target, &mut course).unwrap();
    assert_eq!(course.calls, ["scorch 5"]);
    assert_eq!(
        arena.selector39(target).unwrap().intermediate.word_150,
        2_000_000
    );
    assert!(course.writes.is_empty() && course.sounds.is_empty());
    assert_eq!(
        &course.commands[course.commands.len() - 3..],
        [(0x39, 1), (0x3a, 1), (0x3b, 1)]
    );
    course.calls.clear();
    for update in 1..=15 {
        arena.tick_selector39(target, STEP, &mut course).unwrap();
        assert_eq!(
            arena.selector39(target).unwrap().intermediate.word_150,
            2_000_000 - update * STEP
        );
    }
    assert_eq!(course.calls, vec![format!("smoke {STEP}"); 15]);
    course.calls.clear();
    let basis = arena.actor(target).unwrap().rotating.basis;
    assert!(arena.object(target).unwrap().base.flags & 2 != 0);
    arena.tick_selector39(target, STEP, &mut course).unwrap();
    assert_eq!(
        course.calls,
        [
            "visual 0x36".to_string(),
            "visual 0x37".to_string(),
            format!("burst {position:?}"),
            "blast".to_string(),
            "model 0x46".to_string(),
        ]
    );
    let state = arena.selector39(target).unwrap();
    assert_eq!(
        (state.intermediate.word_150, state.intermediate.word_144),
        (0, 0x46)
    );
    let actor = arena.actor(target).unwrap();
    assert_eq!((actor.word_74, actor.word_c8), (0, 0));
    assert_ne!(actor.rotating.basis, basis);
    assert_eq!(arena.object(target).unwrap().base.flags & 2, 0);
    assert_eq!(course.status, 16);

    // A generator its links hold up dies once the last is gone: Infest03's
    // first 0x13 had four (+14C 4, +14D 1), the 0x12s the port leaves out.
    let held = generators[0];
    assert_eq!(
        arena.object(held).unwrap().base.position_words,
        Some([45686, 640, 15587])
    );
    let mut course = Course::default();
    arena.selector39_mut(held).unwrap().intermediate.byte_14d = 1;
    arena.selector39_mut(held).unwrap().intermediate.byte_14c = 1;
    arena.tick_selector39(held, STEP, &mut course).unwrap();
    assert!(!course.calls.contains(&"retire".to_string()));
    arena.selector39_mut(held).unwrap().intermediate.byte_14c = 0;
    arena.tick_selector39(held, STEP, &mut course).unwrap();
    assert!(course.calls.contains(&"retire".to_string()));
}

/// World 12's timed generators (+160 30, +161 so no damage, no fuse):
/// message 0xC knocks one out through `0047CE90`, and its death adds its
/// 30 s, four times over, to the world's countdown instead of marking its
/// placement (`0043B987`), then runs the shared tail. While the countdown
/// runs, its update (`0043BCCD`) brings its health back by the elapsed
/// share; when the countdown ends it comes back (`0043BFE0`: state 3, full
/// health, message 7 to its links).
#[test]
fn a_timed_generator_waits_out_the_countdown() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world_retained(12, TerrainSpacing::Units256);
    let arena = &mut world.arena;
    let target = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            arena
                .selector39(actor)
                .is_ok_and(|state| state.byte_160 == 30 && state.byte_163 == 0)
        })
        .expect("a world-12 timed generator");
    let state = arena.selector39(target).unwrap();
    assert_eq!((state.byte_161, state.byte_165), (1, 0));
    let maximum = arena.actor(target).unwrap().maximum_health.unwrap();
    let mut course = Course::default();
    arena.selector39_mut(target).unwrap().byte_164 = 1;
    kill(arena, target);
    arena.retire_selector39(target, &mut course).unwrap();
    assert_eq!(
        course.writes,
        [Selector39MissionWrite::AddCountdown {
            seconds: 30,
            quadruple: true
        }]
    );
    assert!(course.marks.is_empty() && course.sounds.is_empty());
    assert_eq!(
        &course.commands[course.commands.len() - 3..],
        [(0x39, 1), (0x3a, 1), (0x3b, 1)]
    );
    // A quarter of the way through the 30 s: a quarter of the health.
    course.countdown = Selector39Countdown {
        held: 0,
        remaining: 22_500_000,
    };
    arena.tick_selector39(target, STEP, &mut course).unwrap();
    assert_eq!(
        arena.actor(target).unwrap().health,
        (7500 * i32::from(maximum) / 30_000) as u16
    );
    assert_eq!(arena.actor(target).unwrap().word_c4, 1);
    assert_eq!(course.status, 0);
    course.countdown.remaining = 0;
    arena.tick_selector39(target, STEP, &mut course).unwrap();
    let actor = arena.actor(target).unwrap();
    assert_eq!((actor.word_c4, actor.health), (3, maximum));
    // Back in state 3 it waits.
    arena.tick_selector39(target, STEP, &mut course).unwrap();
    assert_eq!(arena.actor(target).unwrap().word_c4, 3);
}

/// One course target from a `world_interactions_native.json` case.
fn receipt_target(arena: &mut ObjectArena, case: &serde_json::Value) -> ObjectId {
    use crate::object_runtime::ActorConstructorNull;
    let number = |key: &str| case[key].as_u64().unwrap();
    let words = |key: &str| -> Vec<u64> {
        case[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap())
            .collect()
    };
    let position = words("position");
    let target = arena
        .allocate_selector39(crate::object_runtime::ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([position[0] as u16, position[1] as u16, position[2] as u16]),
            owner: 1,
            terrain: crate::object_runtime::ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        })
        .unwrap();
    let owned = &mut arena.owned_mut(target).unwrap().state.base;
    owned.flags = number("flags") as u32;
    owned.resource_word = number("resource") as u16;
    owned.dirty_word = number("dirty") as u16;
    let basis = words("basis");
    let actor = arena.actor_mut(target).unwrap();
    actor.rotating.basis.words = std::array::from_fn(|i| basis[i] as u32);
    actor.rotating.visual.model_word = number("model") as u16;
    actor.word_74 = number("word_74") as u16;
    actor.word_7a = number("grant") as u16;
    actor.byte_78 = number("index") as u8;
    actor.health = number("health") as u16;
    actor.maximum_health = Some(number("maximum") as u16);
    actor.word_c4 = number("state") as u32;
    actor.word_c8 = number("word_c8") as u16;
    let state = arena.selector39_mut(target).unwrap();
    state.intermediate.word_13c = 0;
    state.intermediate.word_144 = number("wreck") as u16;
    state.intermediate.byte_14c = number("links") as u8;
    state.intermediate.byte_14d = number("held") as u8;
    state.intermediate.word_150 = number("fuse") as u32;
    state.word_15c = number("order") as u32;
    state.byte_160 = number("seconds") as u8;
    state.byte_161 = number("immune") as u8;
    state.byte_162 = number("byte_162") as u8;
    state.byte_163 = number("fused") as u8;
    state.byte_164 = number("pulse") as u8;
    state.byte_165 = number("radiation_off") as u8;
    target
}

fn receipt_course(case: &serde_json::Value) -> Course {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    Course {
        math: NativeBasisMath::new(assets.sine_table().values),
        rng: case["rng"].as_u64().unwrap_or(0) as u32,
        countdown: Selector39Countdown {
            held: case["mission_18"].as_u64().unwrap() as u8,
            remaining: case["countdown"].as_u64().unwrap() as u32,
        },
        retire_kills: false,
        ..Course::default()
    }
}

fn receipt_fixture() -> serde_json::Value {
    serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap()
}

/// `0043BB80` with the radiation off against the 60 updates of
/// `world_interactions_native.json`: the dispatch to `0040DB40`, `0043BCCD`
/// and the fuse, with the real `0043BFE0`, `0048F060` and `00491530`. The
/// services each reaches in order, the fuse, the wreck and model words,
/// +74, +C8, the health, the dirty word, the basis and the RNG.
#[test]
fn target_updates_match_the_native_receipt() {
    let fixture = receipt_fixture();
    let cases = fixture["target_update"].as_array().unwrap();
    assert_eq!(cases.len(), 60);
    let null = ActorHandle::from_raw(0);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let number = |key: &str| case[key].as_u64().unwrap();
        let mut arena =
            ObjectArena::with_actor_registry(crate::object_runtime::ActorRegistryConfig {
                owner_count: 3,
                initial_capacity: 8,
                null_handle: null,
            })
            .unwrap();
        let target = receipt_target(&mut arena, case);
        let state_before = arena.actor(target).unwrap().word_c4;
        let mut course = receipt_course(case);
        arena
            .tick_selector39(target, number("delta") as u32, &mut course)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let mut expected = Vec::new();
        let mut status = 0;
        let mut came_back = false;
        let mut left_list = false;
        let mut sounds = 0;
        for event in case["events"].as_array().unwrap() {
            let args: Vec<u64> = event["args"]
                .as_array()
                .map(|list| list.iter().map(|value| value.as_u64().unwrap()).collect())
                .unwrap_or_default();
            match event["call"].as_str().unwrap() {
                "retire" => expected.push("retire".to_string()),
                "status_visual" => expected.push(format!("smoke {}", args[0])),
                "status" => status += 1,
                "child" => expected.push(format!("visual {:#x}", args[1])),
                "burst" => {
                    let record: Vec<u64> = event["record"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_u64().unwrap())
                        .collect();
                    assert_eq!(
                        [
                            record[0], record[4], record[6], record[7], record[8], record[9],
                            record[10]
                        ],
                        [0x10, 0x14, 1, 1, 1, 8, 0],
                        "{name}"
                    );
                    expected.push(format!(
                        "burst {:?}",
                        [record[1] as u16, record[2] as u16, record[3] as u16]
                    ));
                }
                "blast" => {
                    assert_eq!(args[0], 0x0053_1250, "{name}");
                    assert_eq!(args[3], 1, "{name}");
                    expected.push("blast".to_string());
                }
                "refresh_model" => expected.push(format!("model {:#x}", number("model_after"))),
                "flag1" => {
                    assert_eq!(args[0], 0, "{name}");
                    left_list = true;
                }
                "state" => {
                    assert_eq!(args[2], 3, "{name}");
                    came_back = true;
                }
                "fuse_sound" => sounds += 1,
                "sound_release" => {}
                other => panic!("{name}: unexpected {other}"),
            }
        }
        assert_eq!(course.calls, expected, "{name}");
        assert_eq!(course.status, status, "{name}");
        assert_eq!(course.sounds.len(), sounds, "{name}");
        let actor = arena.actor(target).unwrap();
        let state = arena.selector39(target).unwrap();
        assert_eq!(
            u64::from(state.intermediate.word_150),
            number("fuse_after"),
            "{name}"
        );
        assert_eq!(
            u64::from(state.intermediate.word_144),
            number("wreck_after"),
            "{name}"
        );
        assert_eq!(
            u64::from(actor.rotating.visual.model_word),
            number("model_after"),
            "{name}"
        );
        assert_eq!(u64::from(actor.word_74), number("word_74_after"), "{name}");
        assert_eq!(u64::from(actor.word_c8), number("word_c8_after"), "{name}");
        assert_eq!(u64::from(actor.health), number("health_after"), "{name}");
        let basis_after: Vec<u32> = case["basis_after"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(&actor.rotating.basis.words[..], &basis_after[..], "{name}");
        assert_eq!(u64::from(course.rng), number("rng_after"), "{name}");
        assert_eq!(
            actor.word_c4,
            if came_back { 3 } else { state_before },
            "{name}"
        );
        // The receipt records 00479F20(0); the real one clears +2C bit 1
        // and, as that changes the low nine bits, sets +38 bit 1.
        let mut dirty_after = number("dirty_after");
        if left_list {
            assert_eq!(arena.object(target).unwrap().base.flags & 2, 0, "{name}");
            if number("flags") & 2 != 0 {
                dirty_after |= 2;
            }
        }
        let dirty = arena.object(target).unwrap().base.dirty_word;
        assert_eq!(u64::from(dirty), dirty_after, "{name}");
    }
}

/// `0043B910` against the 48 deaths of `world_interactions_native.json`:
/// generators, fused, timed and held targets without links. The placement
/// mark, the scorch, the fuse sound, the commands and the grants in order,
/// the fuse, and the mission's +13, +17, +18 and +24 after the writes.
#[test]
fn target_deaths_match_the_native_receipt() {
    let fixture = receipt_fixture();
    let cases = fixture["target_death"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    let null = ActorHandle::from_raw(0);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let number = |key: &str| case[key].as_u64().unwrap();
        let mut arena =
            ObjectArena::with_actor_registry(crate::object_runtime::ActorRegistryConfig {
                owner_count: 3,
                initial_capacity: 8,
                null_handle: null,
            })
            .unwrap();
        arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]);
        let target = receipt_target(&mut arena, case);
        let other = receipt_target(&mut arena, case);
        arena.owned_mut(other).unwrap().state.base.resource_word = number("other_resource") as u16;
        let actor = arena.actor_mut(other).unwrap();
        actor.word_c4 = number("other_state") as u32;
        actor.word_7a = number("other_grant") as u16;
        let mut course = receipt_course(case);
        arena
            .retire_selector39(target, &mut course)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let mut marks = Vec::new();
        let mut scorches = Vec::new();
        let mut sounds = 0;
        let mut commands = Vec::new();
        let mut grants = Vec::new();
        for event in case["events"].as_array().unwrap() {
            let args: Vec<u64> = event["args"]
                .as_array()
                .map(|list| list.iter().map(|value| value.as_u64().unwrap()).collect())
                .unwrap_or_default();
            match event["call"].as_str().unwrap() {
                "mark" => {
                    assert_eq!(args[1], 0, "{name}");
                    marks.push(args[0] as u32 as i32 as i8);
                }
                "scorch" => scorches.push(format!("scorch {}", args[0])),
                "fuse_sound" => sounds += 1,
                "sound_release" => {}
                "command" => {
                    assert!(event["player"].as_bool().unwrap(), "{name}");
                    commands.push((args[0] as u32, args[1] as u8));
                }
                "grant" => {
                    assert!(event["player"].as_bool().unwrap(), "{name}");
                    grants.push(args[0] as u16);
                }
                other => panic!("{name}: unexpected {other}"),
            }
        }
        assert_eq!(course.marks, marks, "{name}");
        assert_eq!(course.calls, scorches, "{name}");
        assert_eq!(course.sounds.len(), sounds, "{name}");
        assert_eq!(course.commands, commands, "{name}");
        assert_eq!(course.grants, grants, "{name}");
        assert_eq!(
            u64::from(arena.selector39(target).unwrap().intermediate.word_150),
            number("fuse_after"),
            "{name}"
        );
        let mut mission = crate::world_mission::WorldMission::native_constructor();
        mission.byte_13 = Some(number("mission_13") as u8);
        mission.byte_17 = Some(number("mission_17") as u8);
        mission.byte_18 = Some(number("mission_18") as u8);
        mission.dword_24 = Some(number("countdown") as u32);
        for write in &course.writes {
            match *write {
                Selector39MissionWrite::StopCountdown => {
                    mission.dword_24 = Some(0);
                    mission.byte_17 = Some(0);
                }
                Selector39MissionWrite::AddCountdown { seconds, quadruple } => {
                    mission.add_target_countdown(seconds, quadruple).unwrap()
                }
                Selector39MissionWrite::SetByte13 => mission.byte_13 = Some(1),
            }
        }
        let after: Vec<u64> = case["mission_after"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap())
            .collect();
        assert_eq!(
            [
                u64::from(mission.byte_13.unwrap()),
                u64::from(mission.byte_17.unwrap()),
                u64::from(mission.byte_18.unwrap()),
                u64::from(mission.dword_24.unwrap()),
            ],
            [after[0], after[1], after[2], after[3]],
            "{name}"
        );
    }
}

/// `0043BDF0` against the 48 messages of `world_interactions_native.json`,
/// run on the original instructions with the real `0040DE70`: 0xB/0xC set
/// +164 and retire a target with seconds, 0x14/0x15 move +166 by the
/// argument, and the rest answer 0.
#[test]
fn target_messages_match_the_native_receipt() {
    use crate::object_runtime::{ActorMessage, ActorMessageReply, PlayerActorLookupContext};
    let fixture = receipt_fixture();
    let cases = fixture["target_message"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let number = |key: &str| case[key].as_u64().unwrap();
        let mut arena =
            ObjectArena::with_actor_registry(crate::object_runtime::ActorRegistryConfig {
                owner_count: 3,
                initial_capacity: 8,
                null_handle: ActorHandle::from_raw(0),
            })
            .unwrap();
        let target = arena
            .allocate_selector39(crate::object_runtime::ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some([0; 3]),
                owner: 1,
                terrain: crate::object_runtime::ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &[0; 4096],
                },
            })
            .unwrap();
        let state = arena.selector39_mut(target).unwrap();
        state.byte_160 = number("seconds") as u8;
        state.byte_164 = number("pulse") as u8;
        state.byte_166 = number("shield") as u8;
        let reply = arena
            .receive_actor_message(
                target,
                ActorMessage {
                    sender: None,
                    distance_squared: 1000,
                    code: number("code") as i32,
                    argument: number("argument") as u32,
                },
                PlayerActorLookupContext {
                    process: Some(&process),
                },
                false,
            )
            .unwrap();
        let retired = case["retired"].as_bool().unwrap();
        assert_eq!(
            reply,
            if retired {
                ActorMessageReply::Retire
            } else {
                ActorMessageReply::Done(number("answer") as u32)
            },
            "{name}"
        );
        let state = arena.selector39(target).unwrap();
        assert_eq!(u64::from(state.byte_164), number("pulse_after"), "{name}");
        assert_eq!(u64::from(state.byte_166), number("shield_after"), "{name}");
    }
}
