use super::super::Family455560Tail;
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
    ObjectList,
};

fn disposed(arena: &ObjectArena, object: ObjectId) -> bool {
    arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|entry| entry.object == object)
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 64,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

/// A selector `selector` behavior resource at `position` with `flags`.
fn actor(arena: &mut ObjectArena, selector: u16, position: [u16; 3], flags: u32) -> ObjectId {
    let sine = [0; 4096];
    let object = arena
        .allocate_behavior_resource(
            ActorConstructionRequest {
                allocation_position: Some(position),
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
            selector,
        )
        .unwrap();
    let base = &mut arena.owned_mut(object).unwrap().state.base;
    base.flags = flags;
    base.position_words = Some(position);
    object
}

/// A player whose +D8 is `unit`.
fn holder(arena: &mut ObjectArena, unit: ObjectId) -> PlayerId {
    let player = arena.add_world_player([0; 6]);
    let handle = arena.actor(unit).unwrap().handle;
    arena.world_player_mut(player).unwrap().handle_d8 = handle;
    player
}

/// 0046AE90 recorded: the handles that take, and every call.
struct Takes {
    accept: Vec<ActorHandle>,
    calls: Vec<(PlayerId, ActorHandle)>,
}

impl ImplantTakeHost for Takes {
    type Error = std::convert::Infallible;
    fn take(
        &mut self,
        _: &mut ObjectArena,
        player: PlayerId,
        handle: ActorHandle,
    ) -> Result<bool, Self::Error> {
        self.calls.push((player, handle));
        Ok(self.accept.contains(&handle))
    }
}

fn block(turn: i32, pitch: i32) -> PlayerControlBlock {
    PlayerControlBlock {
        turn_14: turn,
        pitch_18: pitch,
        ..PlayerControlBlock::default()
    }
}

/// The compiler reciprocals are exact divisions over the whole DWORD range:
/// 0x4EC4EC4F >> 14 by 0xD000, 0x10624DD3 >> 8 by 4000, and the negated
/// 0x10624DD3 >> 7 by -2000, each truncating toward zero.
#[test]
fn the_flights_reciprocals_divide_exactly() {
    for value in [
        0,
        1,
        -1,
        1999,
        2000,
        -2000,
        0xd000,
        -0xd001,
        i32::MAX,
        i32::MIN,
        123_456_789,
    ] {
        assert_eq!(
            magic_quotient(value, 0x4ec4_ec4f, 14),
            value / 0xd000,
            "{value}"
        );
        assert_eq!(
            magic_quotient(value, 0x1062_4dd3, 8),
            value / 4000,
            "{value}"
        );
        assert_eq!(
            magic_quotient(value, -0x1062_4dd3, 7),
            (value / 2000).wrapping_neg(),
            "{value}"
        );
    }
}

/// 004578F1..004579F5: a positive turn or pitch gives -step and a negative
/// one +step; without either, an analog block's axes count beyond their
/// 0x3000 dead zone, and a digital block without input gives nothing.
#[test]
fn steering_rates_follow_the_digital_and_analog_block() {
    let step = 7812;
    assert_eq!(steering_rates(&block(0, 0), step), (0, 0));
    assert_eq!(steering_rates(&block(125, 0), step), (-step, 0));
    assert_eq!(steering_rates(&block(-1, 0), step), (step, 0));
    assert_eq!(steering_rates(&block(0, 9), step), (0, -step));
    assert_eq!(steering_rates(&block(-3, -4), step), (step, step));
    let analog = |x: i32, y: i32| PlayerControlBlock {
        analog_04: 1,
        axes_08: [x, y, 0],
        ..PlayerControlBlock::default()
    };
    assert_eq!(steering_rates(&analog(0x3000, -0x3000), step), (0, 0));
    assert_eq!(
        steering_rates(&analog(0x10000, -0x9000), step),
        ((0xd000 * step) / 0xd000, (-0x6000 * step) / 0xd000)
    );
    // The digital block wins over the analog axes.
    let mut both = analog(0x10000, 0x10000);
    both.turn_14 = 1;
    assert_eq!(steering_rates(&both, step), (-step, 0));
}

/// The flight's limits at their edges: a pitch shrinks only past the nose's
/// 3000 either way, turns down only above 4000 and to at most a step; the
/// speed stops at 3000 while thrusting and at 1000 cruising; the rates
/// still only below 250, and lose 32 besides their share of the step.
#[test]
fn the_flights_limits_hold_at_their_edges() {
    assert_eq!(nose_limited_pitch(1024, -3000), 1024);
    assert_eq!(nose_limited_pitch(1024, -3001), 1023);
    assert_eq!(nose_limited_pitch(1024, 3001), 1024);
    assert_eq!(nose_limited_pitch(-1024, 3000), -1024);
    assert_eq!(nose_limited_pitch(-1024, 3001), -1023);
    assert_eq!(nose_limited_pitch(-1024, -3001), -1024);
    assert_eq!(nose_limited_pitch(4096, -3500), 2096);
    assert_eq!(nose_limited_pitch(-4096, 3500), -2096);
    assert_eq!(ceiling_pitch(0, 4000, 4000), 0);
    assert_eq!(ceiling_pitch(0, 4001, 4000), 1);
    assert_eq!(ceiling_pitch(-4000, 8000, 4000), 0);
    assert_eq!(ceiling_pitch(3990, 8000, 4000), 4000);
    // 125000 us gain ((125000 >> 5) * 1000) >> 15 = 119 a flight.
    assert_eq!(approached_speed(2900, 125_000, true), 3000);
    assert_eq!(approached_speed(3000, 125_000, true), 3000);
    assert_eq!(approached_speed(3100, 125_000, true), 3000);
    assert_eq!(approached_speed(2900, 125_000, false), 2900);
    assert_eq!(approached_speed(900, 125_000, false), 1000);
    assert_eq!(approached_speed(500, 125_000, false), 619);
    assert_eq!(damped_rates([249, 0, 0], 0), [0; 3]);
    assert_eq!(damped_rates([250, 0, 0], 0), [218, 0, 0]);
    assert_eq!(
        damped_rates([0, (-250i32) as u32, 0], 0),
        [0, (-218i32) as u32, 0]
    );
    // Held to 0x7FFF, which loses ((4096 * 32767) >> 15) + 32 = 4127.
    assert_eq!(damped_rates([0x1_0000, 0, 0], 4096), [28_640, 0, 0]);
}

/// 004575E0: a held missile takes any view mode but 0, a free one only 0,
/// and the mode it takes is kept at +D0.
#[test]
fn a_missiles_view_mode_follows_its_hold() {
    let mut arena = arena();
    let implant = actor(&mut arena, 68, [0, 0, 0], 0);
    assert!(!arena.missile_view_mode(implant, 2).unwrap());
    assert!(arena.missile_view_mode(implant, 0).unwrap());
    assert_eq!(arena.actor(implant).unwrap().byte_d0, 0);
    holder(&mut arena, implant);
    assert!(!arena.missile_view_mode(implant, 0).unwrap());
    assert!(arena.missile_view_mode(implant, 2).unwrap());
    assert_eq!(arena.actor(implant).unwrap().byte_d0, 2);
}

/// A held implant is its player's unit: the HUD pass draws the guided view
/// (00456CF0) for it, which reads its disruption +150 (messages 0xB/0xC),
/// its steering rates, height, velocity and fuse.
#[test]
fn a_held_implant_shows_its_player_the_guided_view() {
    let mut arena = arena();
    let implant = actor(&mut arena, 68, [0x4000, 0x800, 0x5000], 0x10);
    let mut process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    process.process_null_handle = Some(ActorHandle::from_raw(0));
    assert_eq!(
        arena.player_hud_units(Some(&process)).unwrap(),
        Vec::new(),
        "no player yet"
    );
    let player = holder(&mut arena, implant);
    assert_eq!(
        arena.player_hud_units(Some(&process)).unwrap(),
        vec![(
            player,
            crate::object_runtime::PlayerHudUnit::Missile {
                missile: implant,
                owner: Some(player),
            }
        )]
    );
    let source = arena.missile_view_source(implant).unwrap();
    assert!(!source.disrupted);
    assert_eq!(source.position, [0x4000, 0x800, 0x5000]);
    assert_eq!(arena.missile_message(implant, 0xb).unwrap(), 1);
    let actor = arena.actor_mut(implant).unwrap();
    actor.words_94_9c = [0x10000, (-0x10000_i32) as u32, 5];
    actor.words_8e_92 = [3000, 0, (-4_i16) as u16];
    let source = arena.missile_view_source(implant).unwrap();
    assert!(source.disrupted);
    assert_eq!(source.rates, [0x10000, -0x10000, 5]);
    assert_eq!(source.velocity, [3000, 0, -4]);
    assert_eq!(source.fuse, arena.missile(implant).unwrap().word_13a);
}

/// 00458DC0: the launcher is no contact; an actor the implant's player
/// takes is none either, and the implant keeps only flags 0xFFFFF68B and
/// is disposed; a refusal, a retired actor or an implant no player holds
/// leaves the contact.
#[test]
fn a_held_implant_hands_its_player_what_it_touches_and_goes() {
    let mut arena = arena();
    let launcher = actor(&mut arena, 28, [0, 0, 0], 0);
    let implant = actor(&mut arena, 68, [10, 0, 0], 0x0000_0975);
    let creature = actor(&mut arena, 28, [20, 0, 0], 0x24);
    let retired = actor(&mut arena, 28, [30, 0, 0], 0x8000_0024);
    let launcher_handle = arena.actor(launcher).unwrap().handle;
    arena.actor_mut(implant).unwrap().owner_handle = launcher_handle;
    let creature_handle = arena.actor(creature).unwrap().handle;
    let mut host = Takes {
        accept: vec![creature_handle],
        calls: Vec::new(),
    };
    // Free: only the launcher is no contact.
    assert!(arena.implant_accepts(implant, launcher, &mut host).unwrap());
    assert!(!arena.implant_accepts(implant, creature, &mut host).unwrap());
    assert!(host.calls.is_empty());
    let player = holder(&mut arena, implant);
    assert!(!arena.implant_accepts(implant, retired, &mut host).unwrap());
    host.accept.clear();
    assert!(!arena.implant_accepts(implant, creature, &mut host).unwrap());
    assert_eq!(host.calls, vec![(player, creature_handle)]);
    assert!(!disposed(&arena, implant));
    host.accept.push(creature_handle);
    assert!(arena.implant_accepts(implant, creature, &mut host).unwrap());
    assert_eq!(arena.object(implant).unwrap().base.flags & 0x974, 0);
    assert!(disposed(&arena, implant));
}

/// 00458E40: a held implant offers its player each actor of the list with
/// flags 0x24 of 0xB4, nearer than the reach (350, then each taker's
/// distance), not itself and not its launcher; then it is disposed.
#[test]
fn state_one_offers_the_player_each_near_candidate_narrowing_the_reach() {
    let mut arena = arena();
    let launcher = actor(&mut arena, 28, [1000, 0, 1000], 0x24);
    let implant = actor(&mut arena, 68, [1000, 0, 1000], 0x10);
    let launcher_handle = arena.actor(launcher).unwrap().handle;
    arena.actor_mut(implant).unwrap().owner_handle = launcher_handle;
    // In list order: exactly 350 (out of reach), taken at 300, skipped at
    // 320 (past the new reach), taken at 200, refused at 100, out of reach
    // at 400, wrong flags at 50.
    let edge = actor(&mut arena, 28, [1000, 0, 1350], 0x24);
    let a = actor(&mut arena, 28, [1300, 0, 1000], 0x24);
    let b = actor(&mut arena, 28, [1000, 320, 1000], 0x24);
    let c = actor(&mut arena, 28, [1000, 0, 800], 0x24);
    let d = actor(&mut arena, 28, [1100, 0, 1000], 0x24 | 0x08);
    let e = actor(&mut arena, 28, [1400, 0, 1000], 0x24);
    let f = actor(&mut arena, 28, [1050, 0, 1000], 0x34);
    let handle = |arena: &ObjectArena, object| arena.actor(object).unwrap().handle;
    let player = holder(&mut arena, implant);
    let mut host = Takes {
        accept: vec![handle(&arena, edge), handle(&arena, a), handle(&arena, c)],
        calls: Vec::new(),
    };
    let _ = (e, f);
    arena.implant_state_one(implant, &mut host).unwrap();
    assert_eq!(
        host.calls,
        vec![
            (player, handle(&arena, a)),
            (player, handle(&arena, c)),
            (player, handle(&arena, d)),
        ]
    );
    let _ = b;
    assert!(disposed(&arena, implant));
    // Not held (no flag 0x10), it only goes.
    let free = actor(&mut arena, 68, [1000, 0, 1000], 0);
    let mut host = Takes {
        accept: Vec::new(),
        calls: Vec::new(),
    };
    arena.implant_state_one(free, &mut host).unwrap();
    assert!(host.calls.is_empty());
    assert!(disposed(&arena, free));
}

#[derive(serde::Deserialize)]
struct FlightReceipts {
    flights: Vec<FlightRecord>,
}

#[derive(serde::Deserialize)]
struct FlightRecord {
    at: String,
    delta: u32,
    block: FlightBlock,
    entry: FlightFields,
    exit: FlightFields,
    rng_in: u32,
    rng_out: u32,
    calls: Vec<FlightCall>,
}

#[derive(serde::Deserialize)]
struct FlightBlock {
    analog_04: u8,
    axes_08: [i32; 3],
    turn_14: i32,
    pitch_18: i32,
    thrust_1c: i32,
    fire_20: i32,
    grenade_24: i32,
}

/// The implant's bytes the flight reads or writes, by native offset; the
/// ribbon and sound pointers compare by presence, the handles by role.
#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
struct FlightFields {
    word_28: u16,
    flags: u32,
    position: [u16; 3],
    basis: [u32; 10],
    handle: u32,
    owner: u32,
    velocity: [u16; 3],
    angular: [u32; 3],
    state: u32,
    byte_d0: u8,
    byte_138: u8,
    fuse: u16,
    age: u16,
    block: [u8; 10],
    resource_148: u32,
    ribbon: u32,
    byte_150: u8,
    sound: u32,
}

#[derive(serde::Deserialize)]
struct FlightCall {
    name: String,
    edx: u32,
    stack: Vec<u32>,
    point: Option<[u16; 3]>,
}

/// The flight's services, recorded in the recording's own words.
struct FlightHost<'a> {
    terrain: &'a crate::terrain_runtime::TerrainRuntime,
    world_flags: u32,
    resident: &'a crate::ResidentRuntime,
    sounds: crate::process_sound::ProcessSoundRuntime,
    calls: Vec<String>,
}

impl super::super::MissileLifecycleHost for FlightHost<'_> {
    type Error = std::convert::Infallible;
    fn detonate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("47d670".into());
        Ok(())
    }
    fn burst(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: [u8; 10],
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("4887d0".into());
        Ok(())
    }
    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        _: [u16; 3],
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("48a0f0".into());
        Ok(())
    }
    fn flash(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push(format!("487d70 {position:x?} {radius:#x}"));
        Ok(())
    }
}

impl MissileHost for FlightHost<'_> {
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
    ) -> Result<&crate::terrain_runtime::TerrainRuntime, Self::Error> {
        Ok(self.terrain)
    }
    fn create_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.calls.push("486600".into());
        Ok(Some(arena.allocate_selector67(ActorHandle::from_raw(0))))
    }
    fn extend_ribbon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), Self::Error> {
        self.calls.push(format!("4869b0 {delta:#x} {point:x?}"));
        Ok(())
    }
    fn settings_7c(&self) -> u32 {
        1
    }
    fn inverse_sine(&self) -> &[i16; 1024] {
        &[0; 1024]
    }
    fn process_null(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
    fn release_sound(&mut self, _: ProcessSoundRef) -> Result<(), Self::Error> {
        self.calls.push("487fe0".into());
        Ok(())
    }
}

impl MissileSteerHost for FlightHost<'_> {
    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        missile: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
        flag: u32,
    ) -> Result<ProcessSoundRef, Self::Error> {
        self.calls
            .push(format!("487f60 {sound:#x} {gain:#x} {pitch:#x} {flag}"));
        let position = arena.object(missile).unwrap().base.position_words.unwrap();
        Ok(self
            .sounds
            .create_persistent(self.resident, sound as i16, gain, pitch, flag, || {
                Ok::<_, std::convert::Infallible>(position)
            })
            .unwrap())
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        gain: u32,
        pitch: u32,
    ) -> Result<(), Self::Error> {
        self.calls.push(format!("487fc0 {gain:#x} {pitch:#x}"));
        Ok(())
    }
    fn retire(&mut self, _: &mut ObjectArena, _: ObjectId, _: &mut u32) -> Result<(), Self::Error> {
        self.calls.push("47ce90".into());
        Ok(())
    }
    fn world_flags(&self) -> u32 {
        self.world_flags
    }
}

/// The recorded calls in the host's words.
fn expected_calls(record: &FlightRecord) -> Vec<String> {
    record
        .calls
        .iter()
        .map(|call| match call.name.as_str() {
            "487f60" => format!(
                "487f60 {:#x} {:#x} {:#x} {}",
                call.edx, call.stack[1], call.stack[2], call.stack[3]
            ),
            "487fc0" => format!("487fc0 {:#x} {:#x}", call.stack[0], call.stack[1]),
            "4869b0" => format!("4869b0 {:#x} {:x?}", call.stack[0], call.point.unwrap()),
            "487d70" => format!("487d70 {:x?} {:#x}", call.point.unwrap(), call.stack[1]),
            other => other.to_string(),
        })
        .collect()
}

/// A selector 68 seeded with a recorded entry, `owner` its launcher, with a
/// ribbon and a live sound where the recording had them.
fn seed_flight(
    arena: &mut ObjectArena,
    entry: &FlightFields,
    owner: ActorHandle,
    host: &mut FlightHost<'_>,
) -> ObjectId {
    let implant = actor(arena, 68, entry.position, entry.flags);
    arena.owned_mut(implant).unwrap().state.base.word_28 = entry.word_28;
    {
        let state = arena.actor_mut(implant).unwrap();
        state.owner_handle = owner;
        state.rotating.basis.words = entry.basis;
        state.words_8e_92 = entry.velocity;
        state.words_94_9c = entry.angular;
        state.word_c4 = entry.state;
        state.byte_d0 = entry.byte_d0;
    }
    let ribbon = (entry.ribbon != 0).then(|| arena.allocate_selector67(ActorHandle::from_raw(0)));
    let sound = (entry.sound != 0).then(|| {
        host.sounds
            .create_persistent(host.resident, 0x15, 0x1000, 0x800, 1, || {
                Ok::<_, std::convert::Infallible>(entry.position)
            })
            .unwrap()
    });
    let state = arena.missile_mut(implant).unwrap();
    state.byte_138 = entry.byte_138;
    state.word_13a = entry.fuse;
    state.word_13c = entry.age;
    state.bytes_13e = Some(entry.block);
    state.dword_148 = entry.resource_148;
    match &mut state.tail {
        Family455560Tail::Zeroed14c {
            ribbon_14c,
            byte_150,
            sound_154,
        } => {
            *ribbon_14c = ribbon;
            *byte_150 = entry.byte_150;
            *sound_154 = sound;
        }
        other => panic!("the implant's tail, not {other:?}"),
    }
    implant
}

fn flight_block(block: &FlightBlock) -> PlayerControlBlock {
    PlayerControlBlock {
        analog_04: block.analog_04,
        axes_08: block.axes_08,
        turn_14: block.turn_14,
        pitch_18: block.pitch_18,
        thrust_1c: block.thrust_1c,
        fire_20: block.fire_20,
        grenade_24: block.grenade_24,
        ..PlayerControlBlock::default()
    }
}

/// The implant's fields after a flight, with the recording's handles, and
/// the recorded exit with its pointers as presence.
fn flight_fields(
    arena: &ObjectArena,
    implant: ObjectId,
    exit: &FlightFields,
) -> (FlightFields, FlightFields) {
    let object = arena.object(implant).unwrap();
    let state = arena.actor(implant).unwrap();
    let missile = arena.missile(implant).unwrap();
    let Family455560Tail::Zeroed14c {
        ribbon_14c,
        byte_150,
        sound_154,
    } = &missile.tail
    else {
        unreachable!()
    };
    let actual = FlightFields {
        word_28: object.base.word_28,
        flags: object.base.flags,
        position: object.base.position_words.unwrap(),
        basis: state.rotating.basis.words,
        handle: exit.handle,
        owner: exit.owner,
        velocity: state.words_8e_92,
        angular: state.words_94_9c,
        state: state.word_c4,
        byte_d0: state.byte_d0,
        byte_138: missile.byte_138,
        fuse: missile.word_13a,
        age: missile.word_13c,
        block: missile.bytes_13e.unwrap(),
        resource_148: missile.dword_148,
        ribbon: u32::from(ribbon_14c.is_some()),
        byte_150: *byte_150,
        sound: u32::from(sound_154.is_some()),
    };
    let mut recorded = exit.clone();
    recorded.ribbon = u32::from(recorded.ribbon != 0);
    recorded.sound = u32::from(recorded.sound != 0);
    (actual, recorded)
}

fn flight_receipts() -> FlightReceipts {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/implant_flights_infest04.json"
    ))
    .unwrap()
}

/// Every steered flight 00457800 of Infest04's implant over Artoria (world
/// 11): each runs from its recorded entry, with its recorded control block,
/// on the loaded terrain, the world's vehicle standing in for the launcher.
/// Turns, pitches and thrust are among them. The fields, the engine sound's
/// start and its gains, the ribbon and its points match; no flight flashes,
/// retires or spends the fuse.
#[test]
fn the_implants_flights_match_every_recorded_flight() {
    let receipts = flight_receipts();
    assert_eq!(receipts.flights.len(), 114);
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        11,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let vehicle_handle = world.arena.actor(vehicle).unwrap().handle;
    let launcher = receipts.flights[0].entry.owner;
    let mut failures = Vec::new();
    for flight in &receipts.flights {
        assert_eq!(flight.entry.owner, launcher);
        let mut host = FlightHost {
            terrain: &world.terrain,
            world_flags: world.world_flags,
            resident: &resident,
            sounds: crate::process_sound::ProcessSoundRuntime::new(executable).unwrap(),
            calls: Vec::new(),
        };
        let arena = &mut world.arena;
        let implant = seed_flight(arena, &flight.entry, vehicle_handle, &mut host);
        let mut rng = flight.rng_in;
        arena
            .tick_missile_steered(
                implant,
                flight.delta,
                &flight_block(&flight.block),
                &sine,
                &mut rng,
                &mut host,
            )
            .unwrap();
        let (actual, exit) = flight_fields(arena, implant, &flight.exit);
        if actual != exit {
            failures.push(format!(
                "{}: fields\n  port {actual:x?}\n  native {exit:x?}",
                flight.at
            ));
        }
        let expected = expected_calls(flight);
        if host.calls != expected {
            failures.push(format!(
                "{}: calls\n  port {:?}\n  native {:?}",
                flight.at, host.calls, expected
            ));
        }
        if rng != flight.rng_out {
            failures.push(format!(
                "{}: rng {rng:#x} != {:#x}",
                flight.at, flight.rng_out
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 114 differ:\n{}",
        failures.len(),
        failures[..failures.len().min(6)].join("\n")
    );
}

/// 00457ABE..00457AF0: a launcher that no longer resolves flashes the world
/// at the implant (0xF00) and retires it (0047CE90) after the engine sound;
/// the flight goes on and ends as the recorded one did.
#[test]
fn a_lost_launcher_flashes_the_world_and_retires_the_implant_which_flies_on() {
    let receipts = flight_receipts();
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        11,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let flight = &receipts.flights[20];
    assert!(flight.entry.sound != 0 && flight.entry.ribbon != 0);
    let mut host = FlightHost {
        terrain: &world.terrain,
        world_flags: world.world_flags,
        resident: &resident,
        sounds: crate::process_sound::ProcessSoundRuntime::new(executable).unwrap(),
        calls: Vec::new(),
    };
    let arena = &mut world.arena;
    let gone = ActorHandle::from_raw(0x0800_0007);
    assert_eq!(arena.resolve_actor_handle(gone).ok().flatten(), None);
    let implant = seed_flight(arena, &flight.entry, gone, &mut host);
    let mut rng = flight.rng_in;
    arena
        .tick_missile_steered(
            implant,
            flight.delta,
            &flight_block(&flight.block),
            &sine,
            &mut rng,
            &mut host,
        )
        .unwrap();
    let (actual, exit) = flight_fields(arena, implant, &flight.exit);
    assert_eq!(actual, exit);
    let mut expected = expected_calls(flight);
    expected.insert(1, format!("487d70 {:x?} 0xf00", flight.entry.position));
    expected.insert(2, "47ce90".into());
    assert_eq!(host.calls, expected);
}

/// 00457B1A..00457B2C: Fire spends the fuse once it is below 28,000 (2 s
/// flown), and the fuse step then detonates the implant (0047D670); at
/// 28,000 Fire does nothing.
#[test]
fn fire_spends_the_implants_fuse_once_two_seconds_are_flown() {
    let receipts = flight_receipts();
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        11,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let vehicle_handle = world.arena.actor(vehicle).unwrap().handle;
    let flight = &receipts.flights[20];
    for (fuse, spent) in [(28_000_u16, false), (27_999, true)] {
        let mut host = FlightHost {
            terrain: &world.terrain,
            world_flags: world.world_flags,
            resident: &resident,
            sounds: crate::process_sound::ProcessSoundRuntime::new(executable).unwrap(),
            calls: Vec::new(),
        };
        let arena = &mut world.arena;
        let mut entry = flight.entry.clone();
        entry.fuse = fuse;
        let implant = seed_flight(arena, &entry, vehicle_handle, &mut host);
        let mut block = flight_block(&flight.block);
        block.fire_20 = 1;
        let mut rng = flight.rng_in;
        arena
            .tick_missile_steered(implant, flight.delta, &block, &sine, &mut rng, &mut host)
            .unwrap();
        assert_eq!(
            host.calls.iter().any(|call| call == "47d670"),
            spent,
            "fuse {fuse}"
        );
        assert_eq!(
            arena.missile(implant).unwrap().word_13a == 0,
            spent,
            "fuse {fuse}"
        );
    }
}
