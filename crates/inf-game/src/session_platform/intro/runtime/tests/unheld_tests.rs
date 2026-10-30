//! Retail's parked VTOL (`vehicle-unheld.js`): while Infest04's player
//! steered the implant and then held the creature it took on Artoria, the
//! vehicle's own `+24` (`00416090`) ran 157 times, the VTOL sinking from
//! its launch height and then standing. Each runs in the port from its
//! recorded start: the update `00416170` on the braked block must return
//! where retail's did, and the deep-water tail must leave the vehicle
//! where `00416090` did.

use super::approach_tests::{bytes, NoVoices};
use super::fall_tests::select_projection;
use super::family_prefix_tests::gameplay_state;
use super::*;
use crate::object_runtime::{
    ActorHandle, PlayerCommandContext, VehicleDriveRequest, VehicleDyingRequest, VehicleGround,
    VehiclePositionStep, VehicleUpdateRequest,
};
use crate::session_platform::world_actor_update::{WorldFamilyUpdateHost, WorldVehicleHost};

#[derive(serde::Deserialize)]
struct Tick {
    /// What the player held: "implant" or "creature".
    hold: String,
    at: String,
    delta: u32,
    clock: u32,
    rng: u32,
    /// The 17 DWORDs `00416090` passed to `00416170`.
    block: [u32; 17],
    /// The first player's unit handles `+D4` and `+D8`.
    handles_d4_d8: [u32; 2],
    vehicle: String,
    behavior: Vec<u32>,
    entries: Vec<u32>,
    /// The `[offset, DWORD]` pairs where the vehicle differed from its
    /// start when `00416090` returned.
    after: Vec<[u32; 2]>,
    /// Those where it differed from that when `00416170` returned.
    updated: Vec<[u32; 2]>,
}

fn overlay(mut image: Vec<u8>, pairs: &[[u32; 2]]) -> Vec<u8> {
    for &[offset, value] in pairs {
        let offset = offset as usize;
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    image
}

/// The vehicle where `00416090` returned.
fn after(tick: &Tick) -> Vec<u8> {
    overlay(bytes(&tick.vehicle), &tick.after)
}

/// The vehicle where `00416170` returned.
fn updated(tick: &Tick) -> Vec<u8> {
    overlay(after(tick), &tick.updated)
}

#[derive(serde::Deserialize)]
struct Unheld {
    world: u32,
    vehicle_handle: u32,
    ticks: Vec<Tick>,
}

fn differences(arena: &ObjectArena, vehicle: ObjectId, recorded: &[u8]) -> Option<String> {
    let differences = arena.vehicle_snapshot_differences_for_tests(vehicle, recorded);
    (!differences.is_empty()).then(|| {
        differences
            .iter()
            .map(|(offset, port, native)| {
                format!("+{offset:03x} port {port:08x} recorded {native:08x}")
            })
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// The block retail built is zero but for its `+3A` (1) and the three
/// bytes after `+00`, which `00416090` leaves as the stack held them; the
/// port's update reads neither those nor the player, whose `+D8` holds the
/// implant or the creature, so the vehicle has no player. In every one of
/// the 157 updates the port's `+24` returns the VTOL where retail's did,
/// after the update and after the tail.
#[test]
fn a_parked_vtol_runs_its_own_update_as_retail_did() {
    let fixture: Unheld = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_unheld_infest04.json"
    ))
    .unwrap();
    let holds = |hold: &str| {
        fixture
            .ticks
            .iter()
            .filter(|tick| tick.hold == hold)
            .count()
    };
    assert_eq!((holds("implant"), holds("creature")), (114, 43));
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        fixture.world,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut owners = TestRetirementOwners::new(&world, executable);
    select_projection(&owners.model_process);
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    let null = owners
        .factory_process
        .process_null_handle
        .unwrap_or(ActorHandle::from_raw(0));
    let state = gameplay_state();
    let water_plane = vehicle_water_plane(&world.terrain);
    let mut failures = Vec::new();
    for tick in &fixture.ticks {
        let mut block = [0u32; 17];
        block[0] = tick.block[0] & 0xffff_ff00;
        block[14] = 0x0001_0000;
        assert_eq!(tick.block, block, "{}", tick.at);
        assert_eq!(tick.handles_d4_d8[0], fixture.vehicle_handle);
        world
            .arena
            .set_player_held_unit_for_tests(player, ActorHandle::from_raw(tick.handles_d4_d8[1]));
        world
            .arena
            .inject_vehicle_snapshot_for_tests(vehicle, &bytes(&tick.vehicle));
        world
            .arena
            .inject_vehicle_behavior_for_tests(vehicle, &tick.behavior, &tick.entries);
        world.rng = tick.rng;
        {
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
                .tick_vehicle_unheld(vehicle, &request, &mut host)
            {
                Ok(_) => {}
                Err(error) => {
                    failures.push(format!("{}: {error}", tick.at));
                    continue;
                }
            }
        }
        if let Some(text) = differences(&world.arena, vehicle, &updated(tick)) {
            failures.push(format!("{} update: {text}", tick.at));
        }
        let world_flags = world.world_flags;
        let mut host = WorldFamilyUpdateHost {
            owners: WorldDamageOwners {
                grant_text: None,
                camera_settings: None,
                flash: None,
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
                session: &state,
                particle_load_factor: 0x10000,
                behavior_factory: Some(BehaviorFactoryLoan {
                    listeners: &mut world.listeners,
                    sine: &sine,
                    world_flags,
                }),
                map: None,
            },
            rng: &mut world.rng,
            sine_quarter: &sine,
        };
        if let Err(error) = world.arena.tick_vehicle_water_tail(
            vehicle,
            &VehicleDyingRequest {
                delta: tick.delta,
                attract: false,
                process_null: null,
                status_load_factor: Some(0x10000),
                sine_quarter: &sine,
                water_plane,
                world_flags,
            },
            &mut host,
        ) {
            failures.push(format!("{} tail: {error}", tick.at));
            continue;
        }
        if let Some(text) = differences(&world.arena, vehicle, &after(tick)) {
            failures.push(format!("{} after: {text}", tick.at));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
