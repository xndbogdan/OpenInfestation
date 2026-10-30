//! A status burn that outlasts the vehicle's health: the controlled update
//! `00416170`'s status (`0047C990`) reaches `0047CE90` on the live vehicle,
//! as Altair Prime's turrets' burning shots (channel 2) do.

use super::approach_tests::NoVoices;
use super::family_prefix_tests::gameplay_state;
use super::*;
use crate::object_runtime::{
    PlayerCommandContext, PlayerControlBlock, VehicleDriveRequest, VehicleDyingRequest,
    VehicleGround, VehiclePositionStep, VehicleUpdateRequest,
};
use crate::session_platform::world_actor_update::{
    WorldFamilyUpdateHost, WorldVehicleHost, WorldWreckHost,
};

/// The retirement takes the lethal hit's lifecycle from the process NULL:
/// state 1 clears the health, the player counts the vehicle lost and the
/// wreck gets its 10 s (`00420B90`). The update then ends in the wreck's
/// branch (`0041625C`), which the caller runs with the dying owners: the
/// wreck's timer runs down by the delta.
#[test]
fn a_burn_that_outlasts_the_health_kills_the_vehicle() {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        5,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut owners = TestRetirementOwners::new(&world, executable);
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    let lost = world.arena.world_player(player).unwrap().word_11c;
    let null = owners.factory_process.process_null_handle.unwrap();
    let state = gameplay_state();
    let delta = 16_384;
    // A burn of strength 2000 takes 31 this tick, past the last health.
    world
        .arena
        .with_supplied_family_dispatch_actor(vehicle, |actor| {
            assert_eq!(actor.word_c4, 3);
            (actor.health, actor.word_cc, actor.word_ce) = (1, 2000, 1000);
        })
        .unwrap();
    let request = VehicleUpdateRequest {
        drive: VehicleDriveRequest {
            delta,
            executable,
            ground: VehicleGround {
                terrain: &world.terrain,
                world_flags: world.world_flags,
                clock: 0,
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
    let mut controls = PlayerControlBlock::default();
    let mut report = world
        .arena
        .tick_vehicle_update(vehicle, &mut controls, &request, &mut host)
        .unwrap();
    let pending = report.wreck.take().expect("the wreck's branch");
    let actor = world.arena.actor(vehicle).unwrap();
    assert_eq!((actor.word_c4, actor.health), (1, 0));
    assert_eq!(world.arena.world_player(player).unwrap().word_11c, lost + 1);
    let world_flags = world.world_flags;
    let water_plane = (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16;
    let mut host = WorldWreckHost {
        family: WorldFamilyUpdateHost {
            owners: WorldDamageOwners {
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
        },
        voices: &mut NoVoices,
    };
    world
        .arena
        .finish_vehicle_wreck(
            vehicle,
            pending,
            &VehicleDyingRequest {
                delta,
                attract: false,
                process_null: null,
                status_load_factor: Some(0x10000),
                sine_quarter: &sine,
                water_plane,
                world_flags,
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(
        world.arena.selector2_actor(vehicle).unwrap().dword_268,
        Some(10_000_000 - delta)
    );
}
