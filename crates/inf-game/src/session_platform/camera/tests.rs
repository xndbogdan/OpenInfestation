use super::*;
use crate::{
    object_runtime::{BaseContext, CameraSettings},
    world_backend::{ArcadeDrivePolicy, OfflineWorldLoadRequest},
    Installation, ResidencyRequest,
};

struct CameraOwners {
    process: ParticleFactoryProcessState,
    globals: CameraGlobalState,
    inverse_sine: [i16; 1024],
    clock: crate::frontend_camera::AnimationClock,
}

impl CameraOwners {
    fn new(assets: &FrontendAssets) -> Self {
        let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        Self {
            process: ParticleFactoryProcessState::native_startup(),
            globals: CameraGlobalState::INITIAL,
            inverse_sine: inf_formats::parse_frontend_inverse_sine_table(
                executable.data(),
                inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
            )
            .unwrap()
            .values,
            clock: Default::default(),
        }
    }

    fn tick(
        &mut self,
        world: &mut WorldSession,
        residency: &ResidentRuntime,
        assets: &FrontendAssets,
    ) {
        // The production gameplay owner advances this process clock before
        // invoking the camera phase. This fixture supplies the same seam.
        self.clock.advance(40_000);
        tick_world_camera(
            world,
            WorldCameraTickContext {
                residency,
                assets,
                clock: self.clock.value(),
                inputs: WorldTickPlatformInputs {
                    settings_20: 0,
                    settings_70: 0,
                    camera_settings: CameraSettings {
                        first_person: 0,
                        camera_style: 0,
                    },
                    settings_78: 0,
                    settings_7c: 0,
                    input_mode_08: 0,
                    vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
                    settings_84: 0,
                },
                microseconds: 40_000,
                process: &self.process,
                globals: &mut self.globals,
                inverse_sine: &self.inverse_sine,
                up: [0.0, 1.0, 0.0],
                shake_enabled: 0,
            },
        );
    }
}

fn loaded(
    selection: u32,
) -> (
    ResidentRuntime,
    FrontendAssets,
    WorldSession,
    crate::world_map::ProcessWorldMapRuntime,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let offline =
        crate::offline_prepare::offline_game_with_selection(&grants, Some(selection)).unwrap();
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner =
        crate::world_backend::WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        })
        .unwrap();
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = crate::world_backend::load_offline_world(
        &mut runtime,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut load_owner,
            listener_effects: &mut load_listener_effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    (runtime, assets, world, maps)
}

#[test]
fn ordinary_world_ticks_the_real_spawn_camera_without_an_intro_controller() {
    let (runtime, assets, mut world, _maps) = loaded(18);
    assert!(world.intro_runtime.is_none());
    assert_eq!(world.drive_policy, ArcadeDrivePolicy::PlayerVehicle);
    let camera = world
        .arena
        .resolve_actor_handle(world.arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let vehicle_handle = world.arena.actor(vehicle).unwrap().handle;
    assert_eq!(
        world
            .arena
            .actor_follow_target(world.arena.selector16(camera).unwrap().target_18c.unwrap(),)
            .unwrap()
            .handle_04,
        vehicle_handle,
    );
    assert_eq!(world.native_camera, None);
    let mut owners = CameraOwners::new(&assets);
    let before_rng = world.rng;
    owners.tick(&mut world, &runtime, &assets);
    assert_eq!(world.native_camera, Some(camera));
    assert_eq!(world.camera_failure, None);
    let before_draw = world.arena.camera_draw_request(camera).unwrap();
    assert_eq!(before_draw.coordinate_shift, 16);
    assert_eq!(world.arena.selector16(camera).unwrap().mode_188, 3);
    assert_eq!(
        owners.globals.cached_position,
        Some(before_draw.eye.map(|value| value as u16)),
    );

    // Move the actual effective vehicle; the next shared camera ticks
    // follow that actor.
    let base = &world.arena.object(vehicle).unwrap().base;
    let mut moved = base.position_words.unwrap();
    moved[2] = moved[2].wrapping_add(64);
    let placement = u32::from(base.placement_word);
    world
        .arena
        .initialize_context(
            vehicle,
            BaseContext {
                position_words: Some(moved),
                placement_index: placement,
            },
        )
        .unwrap();
    for _ in 0..12 {
        owners.tick(&mut world, &runtime, &assets);
    }
    assert_eq!(world.camera_failure, None);
    assert_eq!(world.native_camera, Some(camera));
    assert_ne!(
        world.arena.camera_draw_request(camera).unwrap().eye,
        before_draw.eye
    );
    assert_eq!(
        world.rng, before_rng,
        "disabled shake does not invent RNG draws"
    );
}

#[test]
fn current_missing_process_value_keeps_acquisition_and_reached_target_refresh() {
    let (runtime, assets, mut world, _maps) = loaded(18);
    let mut owners = CameraOwners::new(&assets);
    owners.tick(&mut world, &runtime, &assets);
    assert_eq!(world.camera_failure, None);
    let camera = world.native_camera.unwrap();
    let before = world.arena.selector16(camera).unwrap().clone();
    let before_target_data = world
        .arena
        .actor_follow_target(before.target_18c.unwrap())
        .unwrap();
    let before_draw = world.arena.camera_draw_request(camera).unwrap();
    let before_cached_position = owners.globals.cached_position;
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let original = &world.arena.object(vehicle).unwrap().base;
    let mut position = original.position_words.unwrap();
    position[0] = position[0].wrapping_add(333);
    world
        .arena
        .initialize_context(
            vehicle,
            BaseContext {
                position_words: Some(position),
                placement_index: u32::from(original.placement_word),
            },
        )
        .unwrap();
    let registry_null = world.arena.actor_handles().unwrap().null_handle();
    owners.process.process_null_handle = None;
    owners.tick(&mut world, &runtime, &assets);
    assert_eq!(world.native_camera, Some(camera));
    assert_eq!(
        world.camera_failure,
        Some(CameraPoseError::MissingVehicleProcessNullHandle.to_string()),
    );
    let after = world.arena.selector16(camera).unwrap();
    assert_eq!(after.eye_138, before.eye_138);
    assert_eq!(after.look_15c, before.look_15c);
    let after_target_data = world
        .arena
        .actor_follow_target(after.target_18c.unwrap())
        .unwrap();
    assert_eq!(after_target_data.focus_08.unwrap()[0], position[0]);
    assert_eq!(
        after.target_18c, before.target_18c,
        "refresh keeps allocation identity"
    );
    assert_ne!(
        after_target_data, before_target_data,
        "reached refresh writes survive the later process-source error"
    );
    assert_eq!(
        world.arena.camera_draw_request(camera).unwrap(),
        before_draw
    );
    assert_eq!(owners.globals.cached_position, before_cached_position);
    assert_eq!(
        world.arena.actor_handles().unwrap().null_handle(),
        registry_null
    );

    owners.process.process_null_handle = Some(registry_null);
    owners.tick(&mut world, &runtime, &assets);
    assert_eq!(world.camera_failure, None);
    assert_ne!(
        world.arena.selector16(camera).unwrap().eye_138,
        before.eye_138
    );
}
