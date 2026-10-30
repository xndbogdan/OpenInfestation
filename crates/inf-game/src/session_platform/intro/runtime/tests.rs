use super::*;
use crate::object_runtime::CameraGlobalState;
use crate::session_platform::camera::{tick_world_camera, WorldCameraTickContext};
use crate::{
    process_particles::ParticleId,
    world_statistics::{ActorRetirementAttribution, ActorStatisticCategory, WorldStatistics},
};
use crate::{Installation, ResidencyRequest};
use serde_json::Value;

mod approach_tests;
mod burn_tests;
mod contact_tests;
mod fall_tests;
mod family_prefix_tests;
mod selector77_tests;
mod selector93_tests;
mod unheld_tests;

fn authored_surface_snap(assets: &FrontendAssets) -> u8 {
    inf_formats::FrontendExecutable::parse(assets.executable_bytes())
        .unwrap()
        .mapped(0x0053_a128, 1)
        .unwrap()
        .bytes[0]
}

fn loaded() -> (
    ResidentRuntime,
    FrontendAssets,
    WorldSession,
    crate::world_map::ProcessWorldMapRuntime,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let grants = inf_formats::parse_grant_table(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let offline = crate::offline_prepare::offline_game(&grants).unwrap();
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner = crate::world_backend::WorldLoadOwner::import_saved(
        crate::world_backend::OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        },
    )
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
fn production_intro_admission_moves_authored_helper_and_publishes_camera_pose() {
    let (runtime, assets, mut world, _maps) = loaded();
    let controller = world.intro_runtime.as_ref().unwrap().controller;
    assert_eq!(world.arena.selector88(controller).unwrap().word_140, None);
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes())
            .unwrap()
            .data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    let mut globals = CameraGlobalState::INITIAL;
    let mut state = SessionState::default();
    let mut clock = crate::frontend_camera::AnimationClock::default();
    let inputs = WorldTickPlatformInputs {
        settings_20: 0,
        settings_70: 0,
        camera_settings: crate::object_runtime::CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        settings_78: 0,
        settings_7c: 0,
        input_mode_08: 0,
        vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
        settings_84: 0,
    };
    let mut first_helper_position = None;
    let mut first_camera_position = None;
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut tasks = ProcessTaskRuntime::default();
    tasks
        .retain(world.arena.actor_handles().unwrap().null_handle())
        .unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let mut furniture = FurnitureEffectQueue::default();
    let mut furniture_query =
        crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
    let mut factory_process = ParticleFactoryProcessState::native_startup();
    let mut selector35_process = crate::object_runtime::Selector35RouteProcessState::default();
    let mut behavior_tables = crate::object_runtime::ProcessActorBehaviorTables::from_executable(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let furniture_draw = crate::terrain_furniture::draw::FurnitureDrawState::new(
        &NativeBasisMath::new(assets.sine_table().values),
    );
    let actor_motion = crate::object_runtime::ProcessActorMotionState::from_executable(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let actor_draw = crate::object_runtime::ObjectDrawProcessState::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let model_process = crate::model_process::ModelProcessOwners::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    for tick in 0..45 {
        clock.advance(40_000);
        tick_world_actor_callbacks(
            &mut world,
            WorldActorTickContext {
                attract: false,
                map: None,
                network_key_countdown: &mut 0,
                network_team_flag: &mut 0,
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                camera_globals: &globals,
                grant_text: None,
                pickup_text: None,
                input: &mut crate::input::InputRuntime::default(),
                voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
                residency: &runtime,
                model_fallback: None,
                assets: &assets,
                clock: clock.value(),
                animation_clock: &clock,
                inputs,
                microseconds: 40_000,
                paced_microseconds: Some(40_000),
                mission_completion: false,
                inverse_sine: &inverse.values,
                up: &[0.0, 1.0, 0.0],
                surface_snap_53a128: &authored_surface_snap(&assets),
                factory_process: &mut factory_process,
                behavior_tables: &mut behavior_tables,
                selector35_process: &mut selector35_process,
                actor_motion: &actor_motion,
                model_process: &model_process,
                actor_shared_basis: &actor_draw.shared_basis,
                state: &mut state,
                sounds: &mut sounds,
                tasks: &mut tasks,
                particles: &mut particles,
                furniture: &mut furniture,
                furniture_query: &mut furniture_query,
                furniture_radius: &furniture_radius,
                furniture_draw: &furniture_draw,
            },
        );
        tick_world_camera(
            &mut world,
            WorldCameraTickContext {
                residency: &runtime,
                assets: &assets,
                clock: clock.value(),
                inputs,
                microseconds: 40_000,
                process: &factory_process,
                globals: &mut globals,
                inverse_sine: &inverse.values,
                up: [0.0, 1.0, 0.0],
                shake_enabled: 0,
            },
        );
        let attachment = world.intro_runtime.as_ref().unwrap();
        assert_eq!(attachment.controller_failure, None);
        assert_eq!(world.camera_failure, None);
        let sequence = world.arena.selector88_intro_sequence(controller).unwrap();
        assert_eq!(sequence.stage_148, 1);
        assert_eq!(sequence.sequential_timer_150, None);
        let helper = sequence.primary_helper_154.unwrap();
        let position = world
            .arena
            .object(helper)
            .unwrap()
            .base
            .position_words
            .unwrap();
        let selected_camera = world.native_camera.unwrap();
        let camera = world
            .arena
            .resolve_actor_handle(world.arena.camera_handle().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(selected_camera, camera);
        let draw = world.arena.camera_draw_request(camera).unwrap();
        let position_words = draw.eye.map(|value| value as u16);
        assert_eq!(draw.coordinate_shift, 16);
        assert_eq!(globals.cached_position, Some(position_words));
        if tick == 0 {
            first_helper_position = Some(position);
            first_camera_position = Some(position_words);
            assert_eq!(sequence.elapsed_138, 0);
            assert_eq!(sequence.timer_144, 10_000);
            assert_eq!(sequence.deadline_140, 10_250);
        } else if tick == 44 {
            assert_ne!(Some(position), first_helper_position);
            assert_ne!(Some(position_words), first_camera_position);
        }
    }
    // This short native route window does not reach completion; the remaining
    // actor/task simulation is not replaced by a fabricated phase advance.
    assert_eq!(world.mission.save.mission_selection, 22);
    assert_ne!(
        state.requested_phase(),
        crate::session::SessionPhase::Phase8
    );
}

#[test]
fn production_completion_uses_live_network_and_null_actor_bypasses_missing_null() {
    let (runtime, assets, mut world, _maps) = loaded();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = TestRetirementOwners::new(&world, executable);
    owners.factory_process.process_null_handle = None;
    let sine = &assets.sine_table().values;
    let math = NativeBasisMath::new(*sine);
    let mut state = SessionState::default();
    let before_mission = world.mission.save.clone();
    let world_flags = world.world_flags;
    let mut listener_effects = RequiredListenerEffects;
    for network in [0x100, 0] {
        owners.factory_process.network_flag = network;
        let mut services = ProductionIntroServices {
            retirement: ProductionRetirementHost {
                owners: RetirementOwners {
                    grant_text: None,
                    mission: &mut world.mission,
                    terrain: &mut world.terrain,
                    tasks: &mut owners.tasks,
                    particles: &mut owners.particles,
                    sounds: &mut owners.sounds,
                    furniture: &mut owners.furniture,
                    residency: &runtime,
                    executable,
                    phase: Some(4),
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    factory_process: &mut owners.factory_process,
                    world: None,
                },
                rng: &mut world.rng,
            },
            state: &mut state,
            math: &math,
            sine,
            clock: 0,
            world_flags,
            camera_settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            listeners: &mut world.listeners,
            listener_effects: &mut listener_effects,
        };
        services
            .complete_mission(&mut world.arena, 18, 1, 0)
            .unwrap();
        assert_eq!(
            services
                .retirement
                .owners
                .factory_process
                .process_null_handle,
            None
        );
        assert_eq!(
            services.state.current_phase(),
            crate::session::SessionPhase::Title
        );
        assert_eq!(
            services.retirement.owners.factory_process.network_flag,
            network
        );
        if network != 0 {
            assert_eq!(services.retirement.owners.mission.save, before_mission);
            assert!(!services.state.transition_pending());
        } else {
            assert_eq!(
                services.retirement.owners.mission.save.mission_selection,
                18
            );
            assert_eq!(
                services.state.requested_phase(),
                crate::session::SessionPhase::Phase8
            );
        }
    }
}

#[test]
fn production_retirement_composes_both_authored_actors_particles_statistics_and_sound() {
    let (runtime, assets, mut world, _maps) = loaded();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = TestRetirementOwners::new(&world, executable);
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_births_native.json"
    ))
    .unwrap();
    let native = receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "retirement")
        .unwrap();
    // This supplied seed comes from the original-bytecode burst receipt;
    // position translation leaves its velocity, animation and RNG policy unchanged.
    world.rng = native["rng"].as_u64().unwrap() as u32;
    let mut completion = world.mission.current_world_completion().unwrap();
    let categories = ActorRetirementAttribution {
        target: ActorStatisticCategory::OtherB4,
        source: ActorStatisticCategory::B4ZeroOrUnresolved,
    };
    for (index, reference) in [30, 31].into_iter().enumerate() {
        let actor = world.arena.find_actor_by_placement(reference).unwrap();
        let before_rng = world.rng;
        let position = world
            .arena
            .object(actor)
            .unwrap()
            .base
            .position_words
            .unwrap();
        let placement = world.arena.actor(actor).unwrap().byte_78 as i8;
        let height_delta = i32::from(position[1] as i16)
            - i32::from(world.terrain.height_at([position[0], position[2]]));
        assert_eq!(world.arena.actor(actor).unwrap().word_c4, 3);
        assert!(owners
            .retire(&mut world, actor, &runtime, executable)
            .unwrap());
        assert_eq!(world.arena.actor(actor).unwrap().word_c4, 1);
        assert_eq!(world.arena.actor(actor).unwrap().health, 0);
        assert_ne!(world.arena.object(actor).unwrap().base.flags & 0x8000, 0);
        assert_eq!(world.arena.selector11_actor(actor).unwrap().word_1b2, 2000);
        mark_completion(&mut completion, placement);
        assert_eq!(
            world.mission.current_world_completion().unwrap(),
            completion
        );
        let statistics = WorldStatistics::from_record(world.mission.record_0c.as_mut().unwrap());
        assert_eq!(statistics.attribution_count(categories), index as u32 + 1);
        // H13/11 channel9: threshold1000, multiplier256. The first2000
        // effect adds1000; EXE531250 strength1000 adds zero at that threshold.
        assert_eq!(
            statistics.damage_total(categories),
            1000 * (index as u32 + 1)
        );
        assert_eq!(owners.particles.free_count(), 400 - 24 * (index + 1));
        assert_eq!(owners.particles.pending(1).unwrap().len(), 24 * (index + 1));
        assert!(owners.particles.active(1).unwrap().is_empty());
        assert_eq!(owners.particles.vector_cursor, 16 * (index as u32 + 1));
        assert!(owners.furniture.records().is_empty());
        assert_eq!(owners.sounds.pending_len(), 1);
        let burst_rng = if index == 0 {
            compare_translated_native_burst(
                &owners.particles,
                &receipt,
                native,
                position,
                world.arena.actor_handles().unwrap().null_handle().raw(),
            );
            native["final_rng"].as_u64().unwrap() as u32
        } else {
            // Original receipts establish16 debris*3 draws plus8 central*5.
            advance_rng(before_rng, 88)
        };
        assert_eq!(
            world.rng,
            advance_rng(burst_rng, if height_delta.abs() <= 128 { 3 } else { 0 })
        );
        let mut audio = RetirementAudio::default();
        let sound_rng = world.rng;
        assert_eq!(
            owners.sounds.service_world_draw(
                crate::process_sound::ProcessSoundListener {
                    view: crate::frontend_scene::SceneView {
                        eye: position.map(|word| i32::from(word as i16)),
                        basis: crate::menu::MenuBasis::INITIAL
                    },
                    phase: 4,
                    suppressed: false,
                    water: None,
                },
                &mut audio,
                15,
                &mut world.rng
            ),
            Ok(1)
        );
        assert_eq!(
            audio.calls,
            [(
                6,
                crate::process_sound::ProcessSoundParameters {
                    volume: 65536,
                    pitch: 65536,
                    pan: 0,
                    control_flag: false,
                    placement: Some(crate::process_sound::SoundPlacement {
                        rotated: [0, 0, 0],
                        under_water: false,
                    }),
                }
            )]
        );
        assert!(owners.sounds.is_empty());
        assert_eq!(world.rng, sound_rng); // Supplied voice service executes no alias RNG.
    }
}

struct TestRetirementOwners {
    factory_process: ParticleFactoryProcessState,
    selector35_process: crate::object_runtime::Selector35RouteProcessState,
    animation_clock: crate::frontend_camera::AnimationClock,
    behavior_tables: crate::object_runtime::ProcessActorBehaviorTables,
    actor_motion: crate::object_runtime::ProcessActorMotionState,
    actor_draw: crate::object_runtime::ObjectDrawProcessState,
    model_process: crate::model_process::ModelProcessOwners,
    tasks: ProcessTaskRuntime,
    particles: ProcessParticleRuntime,
    sounds: crate::process_sound::ProcessSoundRuntime,
    furniture: FurnitureEffectQueue,
    furniture_query: crate::world_furniture_effect::ProcessFurnitureQueryState,
    furniture_radius: crate::terrain_runtime::ProcessFurnitureRadius,
    furniture_draw: crate::terrain_furniture::draw::FurnitureDrawState,
}
impl TestRetirementOwners {
    fn new(world: &WorldSession, executable: inf_formats::FrontendExecutable<'_>) -> Self {
        let mut tasks = ProcessTaskRuntime::default();
        tasks
            .retain(world.arena.actor_handles().unwrap().null_handle())
            .unwrap();
        Self {
            factory_process: ParticleFactoryProcessState::native_startup(),
            selector35_process: crate::object_runtime::Selector35RouteProcessState::default(),
            animation_clock: crate::frontend_camera::AnimationClock::default(),
            behavior_tables: crate::object_runtime::ProcessActorBehaviorTables::from_executable(
                executable,
            )
            .unwrap(),
            model_process: crate::model_process::ModelProcessOwners::native_startup(executable)
                .unwrap(),
            actor_draw: crate::object_runtime::ObjectDrawProcessState::native_startup(executable)
                .unwrap(),
            actor_motion: crate::object_runtime::ProcessActorMotionState::from_executable(
                executable,
            )
            .unwrap(),
            tasks,
            particles: ProcessParticleRuntime::default(),
            sounds: crate::process_sound::ProcessSoundRuntime::new(executable).unwrap(),
            furniture: FurnitureEffectQueue::default(),
            furniture_query:
                crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(
                    executable,
                )
                .unwrap(),
            //These retirement/eye fixtures admit no class12/36 collision tail.
            //The supplied retained process owner has its genuine startupWORD.
            furniture_radius: crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
                executable,
            )
            .unwrap(),
            furniture_draw: crate::terrain_furniture::draw::FurnitureDrawState::new(
                &NativeBasisMath::new(
                    inf_formats::parse_frontend_sine_table(
                        executable.data(),
                        inf_formats::FRONTEND_SINE_TABLE_ADDRESS,
                    )
                    .unwrap()
                    .values,
                ),
            ),
        }
    }
    fn retire(
        &mut self,
        world: &mut WorldSession,
        actor: ObjectId,
        residency: &ResidentRuntime,
        executable: inf_formats::FrontendExecutable<'_>,
    ) -> Result<bool, crate::object_runtime::ActorStateChangeError<String>> {
        world.arena.retire_actor(
            actor,
            self.factory_process.process_null_handle,
            &mut ProductionRetirementHost {
                owners: RetirementOwners {
                    grant_text: None,
                    mission: &mut world.mission,
                    terrain: &mut world.terrain,
                    tasks: &mut self.tasks,
                    particles: &mut self.particles,
                    sounds: &mut self.sounds,
                    furniture: &mut self.furniture,
                    residency,
                    executable,
                    phase: Some(4),
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    factory_process: &mut self.factory_process,
                    world: None,
                },
                rng: &mut world.rng,
            },
        )
    }
}

fn mark_completion(completion: &mut [u32; 4], placement: i8) {
    if placement != -1 && placement >= -31 {
        completion[(i32::from(placement) / 32) as usize] |=
            1_u32.wrapping_shl(placement as u32 & 31);
    }
}
fn advance_rng(mut seed: u32, count: usize) -> u32 {
    for _ in 0..count {
        crate::name_entry::name_entry_rng_next(&mut seed);
    }
    seed
}
fn compare_translated_native_burst(
    particles: &ProcessParticleRuntime,
    receipt: &Value,
    native: &Value,
    position: [u16; 3],
    source: u32,
) {
    for record in native["records"].as_array().unwrap() {
        let id = ParticleId::Pool(record["slot"].as_u64().unwrap() as u16);
        let actual = particles.record(id).unwrap();
        assert_eq!(
            particles.occupant(record["slot"].as_u64().unwrap() as usize),
            Some(id)
        );
        let mut fields = receipt["common_fields"].as_array().unwrap().clone();
        if record["fields"][0].as_u64().unwrap() == 0x0052_8934 {
            fields.extend(
                receipt["debris_extra_fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned(),
            );
        }
        for (field, value) in fields.iter().zip(record["fields"].as_array().unwrap()) {
            let offset = field[0].as_u64().unwrap() as usize;
            let mut expected = value.as_u64().unwrap() as u32;
            if [4, 6, 8].contains(&offset) {
                let axis = (offset - 4) / 2;
                expected = u32::from(
                    (expected as u16)
                        .wrapping_add(position[axis])
                        .wrapping_sub(native["position"][axis].as_u64().unwrap() as u16),
                );
            } else if offset == 0x14 {
                expected = source;
            }
            let observed = if field[1] == 2 {
                u32::from(actual.read_word(offset).unwrap())
            } else {
                actual.read_dword(offset).unwrap()
            };
            assert_eq!(observed, expected, "{id:?}+{offset:02X}");
        }
    }
    let order: Vec<_> = particles
        .pending(1)
        .unwrap()
        .iter()
        .map(|id| match id {
            ParticleId::Pool(slot) => u32::from(*slot),
            ParticleId::Scratch => panic!("unexpected scratch"),
        })
        .collect();
    assert_eq!(serde_json::json!(order), native["pending"]);
}

#[derive(Default)]
struct RetirementAudio {
    calls: Vec<(i16, crate::process_sound::ProcessSoundParameters)>,
}
impl crate::process_sound::ProcessSoundVoiceTeardown for RetirementAudio {
    fn delete_voice(
        &mut self,
        record: crate::process_sound::ProcessSoundRef,
        voice: crate::process_sound::ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        crate::process_sound::ProcessSoundVoiceTeardown::delete_voice(
            &mut crate::process_sound::RequiredProcessSoundVoiceTeardown,
            record,
            voice,
        )
    }
}
impl crate::process_sound::ProcessSoundAudio for RetirementAudio {
    type Error = &'static str;
    fn operation(
        &mut self,
        request: crate::process_sound::ProcessSoundVoiceRequest<'_>,
    ) -> Result<
        crate::process_sound::ProcessSoundVoiceReply,
        crate::process_sound::ProcessSoundVoiceServiceFault<Self::Error>,
    > {
        use crate::process_sound::{
            ProcessSoundVoiceOperation as Operation, ProcessSoundVoiceReply as Reply,
            ProcessSoundVoiceRequest as Request, ProcessSoundVoiceServiceFault as Fault,
        };
        let Request::OneShot {
            resource,
            parameters,
            effects_volume: _,
            rng: _,
        } = request
        else {
            return Err(Fault::Required(match request {
                Request::Create { .. } => Operation::Create,
                Request::Start { .. } => Operation::Start,
                Request::Update { .. } => Operation::Update,
                Request::OneShot { .. } => unreachable!(),
            }));
        };
        (|| -> Result<bool, Self::Error> {
            self.calls.push((resource.requested_slot(), parameters));
            Ok(true)
        })()
        .map(Reply::OneShot)
        .map_err(Fault::Backend)
    }
}

#[test]
fn production_retirement_missing_task_keeps_state_and_statistics_before_burst() {
    let (runtime, assets, mut world, _maps) = loaded();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = TestRetirementOwners::new(&world, executable);
    owners.tasks = ProcessTaskRuntime::default(); // Actual process storage is absent.
    let actor = world.arena.find_actor_by_placement(30).unwrap();
    let placement = world.arena.actor(actor).unwrap().byte_78 as i8;
    let color = world.arena.actor(actor).unwrap().word_ca;
    let mut completion = world.mission.current_world_completion().unwrap();
    let initial_rng = world.rng;
    let error = owners
        .retire(&mut world, actor, &runtime, executable)
        .unwrap_err();
    assert!(format!("{error:?}").contains("NullTaskOwner"));
    assert_eq!(world.arena.actor(actor).unwrap().word_c4, 1);
    assert_eq!(world.arena.actor(actor).unwrap().health, 0);
    assert_eq!(world.arena.actor(actor).unwrap().word_ca, color); // State1 skips live-health reaction.
    assert_eq!(world.arena.selector11_actor(actor).unwrap().word_1b2, 0);
    assert_ne!(world.arena.object(actor).unwrap().base.flags & 0x8000, 0);
    mark_completion(&mut completion, placement);
    assert_eq!(
        world.mission.current_world_completion().unwrap(),
        completion
    );
    let categories = ActorRetirementAttribution {
        target: ActorStatisticCategory::OtherB4,
        source: ActorStatisticCategory::B4ZeroOrUnresolved,
    };
    let statistics = WorldStatistics::from_record(world.mission.record_0c.as_mut().unwrap());
    assert_eq!(statistics.attribution_count(categories), 1);
    assert_eq!(statistics.damage_total(categories), 0);
    assert_eq!(world.rng, initial_rng);
    assert_eq!(owners.particles.free_count(), 400);
    assert!(owners.sounds.is_empty());
    assert!(owners.furniture.records().is_empty());
}
