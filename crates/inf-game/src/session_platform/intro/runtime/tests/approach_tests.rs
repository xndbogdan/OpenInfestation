//! Retail's approaches to three of its hurting landings
//! (`vehicle-approach.js`), chained through the port: from the vehicle's,
//! its behavior's and its player's recorded state at an update `00416170`
//! some ticks before the landing, each tick runs the port's update with the
//! recorded delta, controls and clock, then the world contact pass
//! (`004791F0`), as the world tick does. At the recordings' 125 ms ticks
//! every update must start and return where retail's did and the landing
//! must cost what it cost; at the port's 16384 µs ticks the same approach
//! shows what the port's own rate makes of it.

use super::fall_tests::{magnitude, select_projection};
use super::family_prefix_tests::gameplay_state;
use super::*;
use crate::object_runtime::{
    ActorHandle, PlayerCommandContext, PlayerControlBlock, VehicleDriveRequest, VehicleGround,
    VehiclePositionStep, VehicleUpdateRequest, WorldContactPhases,
};
use crate::process_sound::{ProcessSoundRecordError, ProcessSoundRef, ProcessSoundVoiceRef};
use crate::session_platform::world_actor_update::WorldVehicleHost;
use crate::session_platform::world_contact::WorldContactPassHost;

#[derive(serde::Deserialize)]
struct Tick {
    at: String,
    delta: u32,
    clock: u32,
    rng: u32,
    /// The control block the update got, the player's `+04..+47`.
    controls: [u32; 17],
    /// The vehicle's bytes `+000..+5FF` at the update's start.
    vehicle: String,
    /// The same where the update returned, before the world's passes.
    returned: String,
    /// The driving player's bytes `+000..+19F` at the update's start.
    player: String,
    /// The vehicle's behavior `+140`: its DWORDs `+00..+3C` and its
    /// entries' (`+18`).
    behavior: Vec<u32>,
    entries: Vec<u32>,
}

#[derive(serde::Deserialize)]
struct Landing {
    position: [u16; 3],
    velocity: [u16; 3],
    health: u16,
    /// The health at the response, after the landing tick's update.
    before: u16,
}

#[derive(serde::Deserialize)]
struct Approach {
    recording: String,
    hit: String,
    world: u32,
    spacing: [u32; 2],
    shielding: u8,
    ticks: Vec<Tick>,
    landing: Landing,
}

#[derive(serde::Deserialize)]
struct Approaches {
    approaches: Vec<Approach>,
}

fn approaches() -> Vec<Approach> {
    let fixture: Approaches = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_approaches_infest.json"
    ))
    .unwrap();
    fixture.approaches
}

pub(super) fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

/// The block's `+04..+47` as the update reads them; the held keys' times
/// (`+14..+24`) scaled from the recording's 125 ms to `delta`.
fn controls(words: &[u32; 17], delta: u32) -> PlayerControlBlock {
    let held = |word: u32| (i64::from(word as i32) * i64::from(delta) / 125_000) as i32;
    let mut bytes_38 = [0u8; 16];
    for index in 0..4 {
        bytes_38[index * 4..index * 4 + 4].copy_from_slice(&words[13 + index].to_le_bytes());
    }
    let [analog_04, b5, b6, b7] = words[0].to_le_bytes();
    PlayerControlBlock {
        analog_04,
        bytes_05_07: [b5, b6, b7],
        axes_08: [words[1] as i32, words[2] as i32, words[3] as i32],
        turn_14: held(words[4]),
        pitch_18: held(words[5]),
        thrust_1c: held(words[6]),
        fire_20: held(words[7]),
        grenade_24: held(words[8]),
        dword_28: words[9],
        dword_2c: words[10],
        dword_30: words[11],
        dword_34: words[12],
        bytes_38,
    }
}

pub(super) struct NoVoices;
impl crate::process_sound::ProcessSoundVoiceTeardown for NoVoices {
    fn delete_voice(
        &mut self,
        _: ProcessSoundRef,
        _: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError> {
        Ok(())
    }
}

/// The vehicle's state a tick is compared on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Motion {
    position: [u16; 3],
    velocity: [u16; 3],
    health: u16,
    basis: [u32; 10],
    /// The form, its morph, the requested and the previous form.
    modes: [u32; 4],
}

fn recorded(bytes: &[u8]) -> Motion {
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    Motion {
        position: [word(0x32), word(0x34), word(0x36)],
        velocity: [word(0x8e), word(0x90), word(0x92)],
        health: word(0xa0),
        basis: std::array::from_fn(|index| dword(0x44 + index * 4)),
        modes: std::array::from_fn(|index| dword(0x1e8 + index * 4)),
    }
}

fn motion(arena: &ObjectArena, vehicle: ObjectId) -> Motion {
    let actor = arena.actor(vehicle).unwrap();
    let state = arena.selector2_actor(vehicle).unwrap();
    Motion {
        position: arena.object(vehicle).unwrap().base.position_words.unwrap(),
        velocity: actor.words_8e_92,
        health: actor.health,
        basis: actor.rotating.basis.words,
        modes: state.dwords_1e8_1f4,
    }
}

/// What a run did after each tick, and each contact's strength.
struct Run {
    /// The vehicle after each tick's update.
    updated: Vec<Motion>,
    /// The vehicle after each tick's update and contact pass.
    motions: Vec<Motion>,
    /// `0047B590`'s strength of every contact that changed the velocity:
    /// the change's length times the mass, over 192.
    strengths: Vec<i32>,
}

/// The approach from its update `first`, at `delta` per tick, for the
/// recorded game time from that update through the landing's tick and
/// `extra` microseconds more, holding the last tick's controls.
fn run(
    approach: &Approach,
    first: usize,
    delta: u32,
    step: VehiclePositionStep,
    extra: u64,
) -> Run {
    let spacing = match approach.spacing[0] {
        0x7f => crate::terrain_runtime::TerrainSpacing::Units128,
        _ => crate::terrain_runtime::TerrainSpacing::Units256,
    };
    let (runtime, assets, mut world) =
        crate::test_worlds::offline_world_retained(approach.world, spacing);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut owners = TestRetirementOwners::new(&world, executable);
    select_projection(&owners.model_process);
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    world
        .arena
        .inject_vehicle_snapshot_for_tests(vehicle, &bytes(&approach.ticks[first].vehicle));
    world.arena.inject_vehicle_behavior_for_tests(
        vehicle,
        &approach.ticks[first].behavior,
        &approach.ticks[first].entries,
    );
    world
        .arena
        .set_player_shielding_for_tests(player, approach.shielding);
    let null = owners
        .factory_process
        .process_null_handle
        .unwrap_or(ActorHandle::from_raw(0));
    let ticks = &approach.ticks[first..];
    let total = 125_000 * ticks.len() as u64 + extra;
    let mut out = Run {
        updated: Vec::new(),
        motions: Vec::new(),
        strengths: Vec::new(),
    };
    let mut state = gameplay_state();
    let mut elapsed = 0u64;
    let mut last = usize::MAX;
    while elapsed < total {
        let index = ((elapsed / 125_000) as usize).min(ticks.len() - 1);
        let tick = &ticks[index];
        assert_eq!(tick.delta, 125_000, "{} {}", approach.hit, tick.at);
        // The clock steps 48 a second (6 or 7 a recorded tick).
        let clock = tick.clock + ((elapsed - index as u64 * 125_000) * 48 / 1_000_000) as u32;
        if delta == 125_000 {
            world.rng = tick.rng;
        }
        let fresh = index != last;
        last = index;
        if fresh {
            world
                .arena
                .inject_player_snapshot_for_tests(player, &bytes(&tick.player));
        }
        let mut block = controls(&tick.controls, delta);
        if !fresh {
            // A recorded tick's later ticks hold its keys; its requests and
            // presses, which the update consumes, go to the first alone.
            block.dword_28 = 0;
            block.dword_2c = 0;
            block.dword_34 = 0;
            block.bytes_38[2] = 0;
            block.bytes_38[3] = 0;
        }
        {
            let request = VehicleUpdateRequest {
                drive: VehicleDriveRequest {
                    delta,
                    executable,
                    ground: VehicleGround {
                        terrain: &world.terrain,
                        world_flags: world.world_flags,
                        clock,
                        sine_quarter: &sine,
                    },
                    process_null: null,
                    position_step: step,
                    view_zoom_5c0: &std::cell::Cell::new(0),
                    network: None,
                },
                attract: false,
                current_world: world.mission.current_world_a0,
                status_load_factor: Some(0x10000),
                command: PlayerCommandContext {
                    process: Some(&owners.factory_process),
                    executable: Some(executable),
                    resident: Some(&runtime),
                },
                settings_7c: 1,
                special_view: false,
                carry: None,
            };
            let mut host = WorldVehicleHost {
                sounds: &mut sounds,
                residency: &runtime,
                model_fallback: None,
                voices: &mut NoVoices,
                particles: &mut owners.particles,
                executable,
                factory_process: &owners.factory_process,
                session: &state,
                rng: &mut world.rng,
                difficulty_10: world.mission.word_10,
                mission: &mut world.mission,
                animation: &owners.animation_clock,
                grant_text: None,
                behavior_tables: &mut owners.behavior_tables,
                tasks: &mut owners.tasks,
                terrain: &world.terrain,
                terrain_source: &world.terrain_source,
                world_start: &mut world.world_start,
                flash: &mut world.flash,
                listeners: &mut world.listeners,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                sine: &sine,
                math: &math,
                world_flags: world.world_flags,
                model_process: &owners.model_process,
                furniture_query: &mut owners.furniture_query,
                furniture_draw: &owners.furniture_draw,
                load_factor: 0x10000,
            };
            world
                .arena
                .tick_vehicle_update(vehicle, &mut block, &request, &mut host)
                .unwrap_or_else(|error| panic!("{} {}: {error}", approach.hit, tick.at));
        }
        out.updated.push(motion(&world.arena, vehicle));
        let before = world.arena.actor(vehicle).unwrap().words_8e_92;
        {
            let mut host = WorldContactPassHost {
                paced_skip: false,
                mission_completion: false,
                damage: WorldDamageOwners {
                    grant_text: None,
                    camera_settings: None,
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut owners.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut owners.particles,
                    sounds: &mut owners.sounds,
                    furniture: &mut owners.furniture,
                    residency: &runtime,
                    executable,
                    factory_process: &mut owners.factory_process,
                    behavior_tables: &mut owners.behavior_tables,
                    animation_clock: &owners.animation_clock,
                    session: &mut state,
                    particle_load_factor: 0x10000,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut world.listeners,
                        sine: &sine,
                        world_flags: world.world_flags,
                    }),
                    map: None,
                },
                rng: &mut world.rng,
                model_fallback: None,
                model_process: &owners.model_process,
                furniture_query: &mut owners.furniture_query,
                furniture_radius: &owners.furniture_radius,
                furniture_draw: &owners.furniture_draw,
                shared_basis: &owners.actor_draw.shared_basis,
                terrain_source: &world.terrain_source,
                world_flags: world.world_flags,
                sine_quarter: &sine,
                network_active: false,
                world_start: &mut world.world_start,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                grant_text: None,
                factory_tables: std::cell::OnceCell::new(),
                math: &math,
            };
            world
                .arena
                .run_world_contact(
                    vehicle,
                    WorldContactPhases {
                        terrain: true,
                        furniture: true,
                    },
                    &mut host,
                )
                .unwrap_or_else(|error| panic!("{} {}: {error}", approach.hit, tick.at));
        }
        let after = world.arena.actor(vehicle).unwrap().words_8e_92;
        if after != before {
            let change: [i32; 3] =
                std::array::from_fn(
                    |axis| i32::from(after[axis].wrapping_sub(before[axis]) as i16),
                );
            let mass = i32::from(world.arena.actor(vehicle).unwrap().word_8c);
            out.strengths.push(magnitude(change) as i32 * mass / 192);
        }
        out.motions.push(motion(&world.arena, vehicle));
        elapsed += u64::from(delta);
    }
    out
}

/// The first update each approach is chained from.
fn first_update(approach: &Approach) -> usize {
    match approach.hit.as_str() {
        // The VTOL takes two blasts and two bumps before update 15.
        "4D07AA:286" => 15,
        _ => 0,
    }
}

/// Whether retail's update raised the vehicle's health in a tick: the
/// scientists it carries repair it, and the injected vehicle carries none,
/// so only the landing's own damage is compared.
fn repairs(approach: &Approach, first: usize) -> bool {
    approach.ticks[first..].iter().any(|tick| {
        recorded(&bytes(&tick.returned)).health > recorded(&bytes(&tick.vehicle)).health
    })
}

/// At the recordings' 125 ms ticks the port's update and contact pass
/// carry each approach from its recorded update to every later one and to
/// the landing, which costs what it cost retail. Each tick is compared
/// where the update returns and again where the next update starts.
#[test]
fn retail_approaches_reach_their_landings_in_the_port() {
    let approaches = approaches();
    assert_eq!(approaches.len(), 3);
    let mut failures = Vec::new();
    for approach in &approaches {
        let first = first_update(approach);
        let ticks = &approach.ticks[first..];
        let repairs = repairs(approach, first);
        let run = run(approach, first, 125_000, VehiclePositionStep::Retail, 0);
        let mut differ = |stage: &str, at: &str, port: Motion, mut native: Motion| {
            if repairs {
                native.health = port.health;
            }
            if port != native {
                failures.push(format!(
                    "{} {} {stage} at {at}: port {port:?}\n  recorded {native:?}",
                    approach.recording, approach.hit
                ));
                return true;
            }
            false
        };
        for (index, tick) in ticks.iter().enumerate() {
            if differ(
                "update",
                &tick.at,
                run.updated[index],
                recorded(&bytes(&tick.returned)),
            ) {
                break;
            }
            let port = run.motions[index];
            let (at, native) = match ticks.get(index + 1) {
                Some(next) => (next.at.as_str(), recorded(&bytes(&next.vehicle))),
                None => {
                    let landing = &approach.landing;
                    let mut native = port;
                    native.position = landing.position;
                    native.velocity = landing.velocity;
                    native.health = landing.health;
                    (approach.hit.as_str(), native)
                }
            };
            if differ("passes", at, port, native) {
                break;
            }
        }
        // The landing's damage, from the health its tick's update left.
        let damage = run.updated.last().unwrap().health - run.motions.last().unwrap().health;
        let recorded = approach.landing.before - approach.landing.health;
        if damage != recorded {
            failures.push(format!(
                "{} {}: landing damage {damage}, recorded {recorded}",
                approach.recording, approach.hit
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The same approaches at the port's 16384 µs ticks, each run until 3 s
/// past the recorded landing. Under TTD retail updated 8 times a second,
/// and the recorded landings come from those 125 ms updates. At 61 a
/// second, under the port's carried position step and under retail's, the
/// same keys take the vehicle elsewhere: the buggy's drop off Level 1's
/// plateau lands at 1375 (and under retail's step, which loses each
/// update's fraction of its turn, the buggy takes the slope instead, at
/// 584), the hovercraft's fall at 1162, both below the 1500 that hurts
/// them, while the turbo jump lands harder in the port's step (1882) and
/// softer in retail's (1340).
#[test]
fn retail_approaches_land_differently_at_the_ports_tick_rate() {
    // (hit, delta, step, hardest landing, health lost)
    let expected = [
        (
            "2111C5:17BC",
            125_000,
            VehiclePositionStep::Retail,
            1545,
            180,
        ),
        (
            "2111C5:17BC",
            16_384,
            VehiclePositionStep::CarryRemainder,
            1375,
            0,
        ),
        ("2111C5:17BC", 16_384, VehiclePositionStep::Retail, 584, 0),
        ("4D07AA:286", 125_000, VehiclePositionStep::Retail, 1531, 62),
        (
            "4D07AA:286",
            16_384,
            VehiclePositionStep::CarryRemainder,
            1162,
            0,
        ),
        ("4D07AA:286", 16_384, VehiclePositionStep::Retail, 1162, 0),
        (
            "821B84:2059",
            125_000,
            VehiclePositionStep::Retail,
            1675,
            350,
        ),
        (
            "821B84:2059",
            16_384,
            VehiclePositionStep::CarryRemainder,
            1882,
            764,
        ),
        ("821B84:2059", 16_384, VehiclePositionStep::Retail, 1340, 0),
    ];
    let approaches = approaches();
    let mut found = Vec::new();
    for (hit, delta, step, _, _) in expected {
        let approach = approaches.iter().find(|a| a.hit == hit).unwrap();
        let first = first_update(approach);
        let run = run(approach, first, delta, step, 3_000_000);
        let hardest = run.strengths.iter().copied().max().unwrap_or(0);
        let start = run.updated[0].health;
        let lowest = run.motions.iter().map(|m| m.health).min().unwrap();
        found.push((hit, delta, step, hardest, start - lowest));
    }
    assert_eq!(found, expected);
}

/// Each recorded update on its own: the port's `00416170` from the
/// update's recorded start, against where retail's returned, field by
/// field.
#[test]
fn every_approach_update_returns_as_retail_did() {
    let mut failures = Vec::new();
    let mut count = 0;
    for approach in approaches() {
        let spacing = match approach.spacing[0] {
            0x7f => crate::terrain_runtime::TerrainSpacing::Units128,
            _ => crate::terrain_runtime::TerrainSpacing::Units256,
        };
        let (runtime, assets, mut world) =
            crate::test_worlds::offline_world_retained(approach.world, spacing);
        let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let sine = assets.sine_table().values;
        let math = inf_formats::native_basis::NativeBasisMath::new(sine);
        let mut owners = TestRetirementOwners::new(&world, executable);
        select_projection(&owners.model_process);
        let mut sounds = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        let player = world.arena.first_player().unwrap();
        world
            .arena
            .set_player_shielding_for_tests(player, approach.shielding);
        let null = owners
            .factory_process
            .process_null_handle
            .unwrap_or(ActorHandle::from_raw(0));
        let state = gameplay_state();
        for tick in &approach.ticks {
            count += 1;
            world
                .arena
                .inject_vehicle_snapshot_for_tests(vehicle, &bytes(&tick.vehicle));
            world
                .arena
                .inject_vehicle_behavior_for_tests(vehicle, &tick.behavior, &tick.entries);
            world
                .arena
                .inject_player_snapshot_for_tests(player, &bytes(&tick.player));
            world.rng = tick.rng;
            let mut block = controls(&tick.controls, tick.delta);
            let request = VehicleUpdateRequest {
                drive: VehicleDriveRequest {
                    delta: tick.delta,
                    executable,
                    ground: VehicleGround {
                        terrain: &world.terrain,
                        world_flags: world.world_flags,
                        clock: tick.clock,
                        sine_quarter: &sine,
                    },
                    process_null: null,
                    position_step: VehiclePositionStep::Retail,
                    view_zoom_5c0: &std::cell::Cell::new(0),
                    network: None,
                },
                attract: false,
                current_world: world.mission.current_world_a0,
                status_load_factor: Some(0x10000),
                command: PlayerCommandContext {
                    process: Some(&owners.factory_process),
                    executable: Some(executable),
                    resident: Some(&runtime),
                },
                settings_7c: 1,
                special_view: false,
                carry: None,
            };
            let mut host = WorldVehicleHost {
                sounds: &mut sounds,
                residency: &runtime,
                model_fallback: None,
                voices: &mut NoVoices,
                particles: &mut owners.particles,
                executable,
                factory_process: &owners.factory_process,
                session: &state,
                rng: &mut world.rng,
                difficulty_10: world.mission.word_10,
                mission: &mut world.mission,
                animation: &owners.animation_clock,
                grant_text: None,
                behavior_tables: &mut owners.behavior_tables,
                tasks: &mut owners.tasks,
                terrain: &world.terrain,
                terrain_source: &world.terrain_source,
                world_start: &mut world.world_start,
                flash: &mut world.flash,
                listeners: &mut world.listeners,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                sine: &sine,
                math: &math,
                world_flags: world.world_flags,
                model_process: &owners.model_process,
                furniture_query: &mut owners.furniture_query,
                furniture_draw: &owners.furniture_draw,
                load_factor: 0x10000,
            };
            match world
                .arena
                .tick_vehicle_update(vehicle, &mut block, &request, &mut host)
            {
                Ok(_) => {}
                Err(error) => {
                    failures.push(format!("{} {}: {error}", approach.recording, tick.at));
                    continue;
                }
            }
            let returned = bytes(&tick.returned);
            // Retail's carried scientists repair the vehicle in its update
            // (`carried_scientists_repair_the_vehicle`); the injected
            // vehicle carries none.
            let repaired = recorded(&returned).health > recorded(&bytes(&tick.vehicle)).health;
            let differences: Vec<_> = world
                .arena
                .vehicle_snapshot_differences_for_tests(vehicle, &returned)
                .into_iter()
                .filter(|&(offset, _, _)| !(repaired && offset == 0xa0))
                .collect();
            if !differences.is_empty() {
                failures.push(format!(
                    "{} {} (approach {}): {}",
                    approach.recording,
                    tick.at,
                    approach.hit,
                    differences
                        .iter()
                        .map(|(offset, port, native)| format!(
                            "+{offset:03x} port {port:08x} recorded {native:08x}"
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{count} updates\n{}",
        failures.join("\n")
    );
}
