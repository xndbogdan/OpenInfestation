use crate::object_runtime::{ObjectArena, ObjectClass, ObjectId, ObjectModelPacket};
use crate::software_scene::SoftwareObjectModelDrawRequest;

#[derive(Default, Debug)]
struct FixtureCommonPacketSkips {
    expired: usize,
    not_actor: usize,
    no_model: usize,
    no_position: usize,
}

/// Historical common-packet fixtures, separate from production Flag0 traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntityDrawSkip {
    /// The object is gone from the arena.
    Expired,
    /// Only actors reach H1; lights, route nodes, camera and player objects
    /// have their own (unowned or no-op) draw paths.
    NotActor,
    /// The exact `0047AFC0` rule: visual model WORD `+0x42 == -1`.
    NoModel,
    /// `00479C60` leaves `+32..+37` unknown; None is unknown, not origin.
    NoPosition,
}

/// Geometry and captured source for the existing actor placement adapter.
/// This preserves signed WORD XYZ and basis custody, not the complete
/// 0047AFC0 lighting/flag prefix or 00479570 manager traversal. H8 resolution
/// remains a reached common-root backend operation.
fn entity_submission(
    arena: &ObjectArena,
    object: ObjectId,
) -> Result<ObjectModelPacket, EntityDrawSkip> {
    let state = arena.object(object).map_err(|_| EntityDrawSkip::Expired)?;
    let actor = match &state.class {
        ObjectClass::Actor(actor) => actor,
        _ => return Err(EntityDrawSkip::NotActor),
    };
    if actor.rotating.visual.model_word == u16::MAX {
        return Err(EntityDrawSkip::NoModel);
    }
    let words = state
        .base
        .position_words
        .ok_or(EntityDrawSkip::NoPosition)?;
    Ok(ObjectModelPacket {
        source: object,
        model_slot: actor.rotating.visual.model_word as i16,
        // 0047AFF5/FF9..0047B006 MOVSX all three actor position WORDs.
        position: words.map(|word| i32::from(word as i16)),
        basis: actor.rotating.basis,
    })
}

use std::path::Path;

use inf_formats::FrontendExecutable;

use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::frontend_display::frontend_startup;
use crate::frontend_scene::camera_basis;
use crate::menu::MenuBasis;
use crate::model_context::ModelProjection;
use crate::model_execution::ModelExecutionLimits;
use crate::model_lighting::ModelLighting;
use crate::model_primitives::PrimitiveTables;
use crate::render_resources::{NativeColorLayout, RenderResources};
use crate::session_platform::WorldCameraInputs;
use crate::software_frame::{D3dTextureSampling, SoftwareFrame};
use crate::software_scene::{SoftwareScene, SoftwareSceneConfig, SoftwareSceneRequest};
use crate::world_map::{MapLoadContext, MapLoadState, ProcessWorldMapRuntime};
use crate::{Installation, ResidencyRequest, ResidentRuntime, ResourceSelection};

// These isolated renderer fixtures supply a stable authored policy. They do not
// claim a process settings owner or native interleaved settings mutations.
struct FixtureObjectLightPolicy(crate::world_lighting::TerrainLightPolicy);

impl FixtureObjectLightPolicy {
    fn new(enabled: bool, spacing: crate::terrain_runtime::TerrainSpacing) -> Self {
        Self(crate::world_lighting::TerrainLightPolicy {
            enabled,
            spacing,
            color: crate::world_lighting::TerrainLightColor::Unmodified,
        })
    }
}

impl crate::object_runtime::ObjectLightPolicySource for FixtureObjectLightPolicy {
    fn dynamic_lighting(&mut self) -> bool {
        self.0.enabled
    }

    fn terrain_light_policy(&mut self) -> crate::world_lighting::TerrainLightPolicy {
        self.0
    }
}

#[path = "world_scene_tests/arena.rs"]
mod arena;
#[path = "world_scene_tests/shown_draw.rs"]
mod shown_draw;
#[path = "world_scene_tests/water_pixels.rs"]
mod water_pixels;

/// A campaign world loaded as the offline game loads it: `mission` selects
/// the OVL world, `spacing` the terrain detail (256 coarse, 128 fine).
/// Production's retained-manager `policy` lets placement actors that the
/// load disposes run their death; a clean import stops at them.
fn load_campaign_world(
    tier: u8,
    mission: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
    policy: crate::world_backend::OfflineWorldLoadPolicy<'_>,
) -> (
    FrontendAssets,
    ResidentRuntime,
    WorldSession,
    ProcessWorldMapRuntime,
) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: tier,
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let offline =
        crate::offline_prepare::offline_game_with_selection(&grants, Some(mission)).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
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
        &mut resident,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(spacing),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut load_owner,
            listener_effects: &mut load_listener_effects,
            policy,
            particles: None,
            phase: None,
            assets: &assets,
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    (assets, resident, world, maps)
}

#[path = "world_scene_tests/custom_vertex.rs"]
mod custom_vertex;
#[path = "world_scene_tests/focals.rs"]
mod focals;
#[path = "world_scene_tests/force_field.rs"]
mod force_field;
#[path = "world_scene_tests/vehicle.rs"]
mod vehicle;

// Supplied pixel-regression state lives in the real selector16 allocation.
// This fixture does not execute camera Tick or establish retail acceptance.
fn supply_native_camera(world: &mut WorldSession, position: [u16; 3], basis: MenuBasis) {
    let camera = world
        .arena
        .resolve_actor_handle(world.arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    world
        .arena
        .supply_camera_draw_fixture(camera, position, basis)
        .unwrap();
    world.native_camera = Some(camera);
}

struct WorldCameraFixture {
    sine: [u16; 4096],
    process: crate::process_particles::factories::ParticleFactoryProcessState,
    settings: crate::object_runtime::CameraSettings,
}

impl WorldCameraFixture {
    fn new(assets: &FrontendAssets) -> Self {
        Self {
            sine: assets.sine_table().values,
            process:
                crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
        }
    }

    fn inputs<'a>(
        &'a self,
        residency: &'a ResidentRuntime,
        animation_clock: u32,
    ) -> WorldCameraInputs<'a> {
        WorldCameraInputs {
            residency,
            sine_quarter: &self.sine,
            settings: self.settings,
            process: Some(&self.process),
            animation_clock,
        }
    }
}

struct WorldFurnitureFixture {
    state: crate::terrain_furniture::draw::FurnitureDrawState,
    pending: crate::world_furniture_effect::FurnitureEffectQueue,
    process: crate::process_particles::factories::ParticleFactoryProcessState,
}

impl WorldFurnitureFixture {
    fn new(assets: &FrontendAssets, world: &WorldSession) -> Self {
        Self {
            state: crate::terrain_furniture::draw::FurnitureDrawState::new(
                &inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values),
            ),
            pending: Default::default(),
            process: crate::process_particles::factories::ParticleFactoryProcessState {
                network_flag: 0,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: Some(world.arena.actor_handles().unwrap().null_handle()),
            },
        }
    }
    fn inputs(&mut self) -> WorldFurnitureInputs<'_> {
        WorldFurnitureInputs {
            state: &mut self.state,
            pending: &self.pending,
            birth_inputs: crate::process_particles::births::ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &self.process,
            },
        }
    }
}

/// Infest01 tick 1800: 0049FEA0 draws slot 0x116 at the eye's X/Z and the
/// intro's 00547668 = FFFFF1A0, not at the eye height 460.
#[test]
fn world_sky_keeps_the_eye_x_z_and_the_world_backdrop_height() {
    let submission = world_sky_submission([13837, 460, -20118], -3680);
    assert_eq!(submission.model_slot, 0x116);
    assert_eq!(submission.position, [13837, -3680, -20118]);
    assert_eq!(submission.basis, MenuBasis::INITIAL);
}

/// Infest01's intro tint 00608090: its pre-draw 004501A0 leaves the sky
/// under ambient 000C1012, and the terrain pass under directional 00608090.
#[test]
fn world_light_publishes_the_tint_directional_before_its_ambient() {
    let presentation = crate::world_backend::WorldPresentationState {
        tint_word: 0x0060_8090,
        base_tint_word: 0x0060_8090,
        terrain_light_direction: [0.5, -0.75, 0.25],
        backdrop_height: -3680,
    };
    let [first, second] = world_light_commands(&presentation);
    assert!(matches!(
        first,
        SceneCommand::DirectionalLight { direction, rgb }
            if direction == [0.5, -0.75, 0.25] && rgb == [0x90, 0x80, 0x60]
    ));
    assert!(matches!(second, SceneCommand::AmbientLight(rgb) if rgb == [0x12, 0x10, 0x0c]));
}

/// The remastered eye under the water: the world draw lays its own fog and
/// wobble only where neither the tracer, infrared vision's camera nor a
/// device shading the water lays its own; Vulkan's raster keeps the world
/// as above and lays the murk.
#[test]
fn the_view_under_the_water_is_the_world_draws_only_where_no_device_lays_it() {
    use UnderwaterLook::{Murk, Plain, Wet};
    // under, traced, infrared, device water.
    for (case, look) in [
        ((false, false, false, false), Plain),
        ((false, false, false, true), Plain),
        ((true, false, false, false), Wet),
        ((true, false, false, true), Murk),
        ((true, true, false, false), Plain),
        ((true, true, false, true), Plain),
        ((true, false, true, false), Plain),
        ((true, false, true, true), Plain),
    ] {
        assert_eq!(
            underwater_look(case.0, case.1, case.2, case.3),
            look,
            "{case:?}"
        );
    }
}

#[test]
fn view_focals_match_retail_first_frame_and_both_update_branches() {
    // TTD frame 1: zero currents initialize from 00546F20/24 = 0x200.
    assert_eq!(
        view_focals([0, 0], [0x200, 0x200], false, 0),
        [0x200, 0x200]
    );
    assert_eq!(
        view_focals([0, 0x200], [0x200, 0x200], true, 7),
        [0x200, 0x200]
    );
    // Steady state is fixed under both branches.
    assert_eq!(
        view_focals([0x200, 0x200], [0x200, 0x200], true, 0x100),
        [0x200, 0x200]
    );
    // Decay branch steps down by 0x50 while above source.
    assert_eq!(
        view_focals([0x250, 0x200], [0x200, 0x200], true, 0),
        [0x200, 0x200]
    );
    assert_eq!(
        view_focals([0x300, 0x300], [0x200, 0x200], true, 0),
        [0x2b0, 0x2b0]
    );
    // Ease branch with zero scale converges by quarters of the gap.
    assert_eq!(
        view_focals([0x300, 0x200], [0x200, 0x200], false, 0),
        [0x2c0, 0x200]
    );
    // Wrapping matches x86 DWORD behavior: just above a small source the
    // decay step underflows like the native DWORD subtract.
    assert_eq!(
        view_focals([0x40, 0x40], [0x30, 0x30], true, 0),
        [0x40i32.wrapping_sub(0x50), 0x40i32.wrapping_sub(0x50)]
    );
    // Below source snaps straight to it.
    assert_eq!(
        view_focals([0x10, 0x10], [0x200, 0x200], true, 0),
        [0x200, 0x200]
    );
}

#[test]
fn depth_policy_matches_the_plus10_call_shape() {
    assert_eq!(depth_policy(true), (1, 1));
    assert_eq!(depth_policy(false), (0, 0));
}

#[test]
fn missing_birth_source_preserves_world_view_models_particle_transfer_and_audio() {
    use crate::process_particles::{
        audio::ParticleSoundRequest,
        births::{ParticleBirthAudio, ParticleBirthInputs, ParticleBirthRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    };
    use crate::process_sound::{ProcessSoundAudio, ProcessSoundParameters, ProcessSoundRuntime};

    struct NoBirthAudio;
    impl ParticleBirthAudio for NoBirthAudio {
        type Error = std::convert::Infallible;
        fn explosion_sound(
            &mut self,
            _: ParticleSoundRequest,
            _: &mut ProcessParticleRuntime,
            _: &mut u32,
        ) -> Result<(), Self::Error> {
            panic!("central type3 bypasses explosion enqueue");
        }
    }
    #[derive(Default)]
    struct Audio {
        calls: Vec<(i16, ProcessSoundParameters, u32)>,
    }
    impl crate::process_sound::ProcessSoundVoiceTeardown for Audio {
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
    impl ProcessSoundAudio for Audio {
        type Error = crate::platform_audio::PlatformAudioError;
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
                effects_volume,
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
                self.calls
                    .push((resource.requested_slot(), parameters, effects_volume));
                Ok(true)
            })()
            .map(Reply::OneShot)
            .map_err(Fault::Backend)
        }
    }

    let (residency, mut world, _maps, _map_load) = load_first_world();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let startup = frontend_startup(
        &residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let pose = crate::object_runtime::CameraPose {
        position_words: [0, 1000, 0],
        basis: MenuBasis::INITIAL,
        target_state: Some(1),
    };
    supply_native_camera(&mut world, pose.position_words, pose.basis);
    let mut particles = ProcessParticleRuntime::default();
    let birth_process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(7)),
    };
    let particle = particles
        .birth_central(
            ParticleBirthRequest {
                particle_type: 3,
                position: [0, 1000, 2560],
                velocity: Some([0; 3]),
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &birth_process,
                },
            },
            executable,
            &mut world.rng,
            &mut NoBirthAudio,
        )
        .unwrap()
        .unwrap();
    assert_eq!(particles.pending(1).unwrap().front(), Some(&particle));
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    sounds
        .enqueue(
            &residency,
            ParticleSoundRequest {
                sound: 6,
                position: pose.position_words,
                volume: 65536,
                pitch: 65536,
                flags: 0,
            },
        )
        .unwrap();
    let mut audio = Audio::default();
    let resources = RenderResources::new(&residency);
    let mut frame = SoftwareFrame::new(640, 480, [16, 24, 64, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    let mut host = ResidentSceneHost {
        resources,
        backend: &mut scene,
    };
    host.backend.set_terrain(Some(world.terrain.clone()));
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    furniture.process.process_null_handle = None;
    let mut lights = crate::world_lighting::WorldLightGrid::default();
    let mut object_light_policy =
        FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
    let outcome = draw_world_frame(
        WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
            weapon_tables: None,
            object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            ).unwrap(),
            attract_demo: &false,
            world: &mut world,
            animation: &AnimationClock::default(),
            particles: &mut particles,
            light_grid: &mut lights,
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            executable,
            camera: camera_inputs.inputs(&residency, 0),
            furniture: furniture.inputs(),
            sound: Some(WorldDrawSound {
                queue: &mut sounds,
                sink: Some(&mut audio),
                effects_volume: 15,
                phase: 4,
            }),
        },
        WorldFrameInputs {
            template_focal: startup.projection.focal,
            focal_src: startup.projection.focal,
            focal_kick: 1.0,
            scale_5c0: 0,
            special_view_active: false,
            surface: [640, 480],
            z_buffering: true,
            dynamic_lighting: false,
            arena_bytes: RETAIL_ARENA_BYTES,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            guided_mounts: false,
            camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
            settings_7c: 0,
            settings_70: 0,
            network_initials: [[0; 6]; crate::network::game::SLOTS],
            network_team_flag: 0,
            infrared: false,
            game: crate::presentation::GameView::Retail,
            game_far: None,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(outcome.focals, startup.projection.focal);
    assert_eq!(world.view_focals, startup.projection.focal);
    let receipt = world.last_draw.unwrap();
    assert!(receipt.native_camera);
    assert_eq!(receipt.eye, pose.position_words.map(i32::from));
    assert_eq!(receipt.basis_words, pose.basis.words);
    assert!(outcome.submitted_models > 0, "{outcome:?}");
    assert!(host.backend.stats.model_commands > 0, "{outcome:?}");
    assert!(particles.pending(1).unwrap().is_empty());
    assert_eq!(particles.active(1).unwrap().front(), Some(&particle));
    assert_eq!(
        particles
            .record(particle)
            .unwrap()
            .read_dword(0x14)
            .unwrap(),
        7
    );
    assert_eq!(outcome.sounds_started, 1);
    assert_eq!(outcome.sound_omissions, 0);
    assert_eq!(audio.calls.len(), 1);
    assert_eq!(audio.calls[0].0, 6);
    assert_eq!(audio.calls[0].1.volume, 65536);
    assert_eq!(audio.calls[0].2, 15);
    assert!(sounds.is_empty());
    assert_eq!(furniture.process.process_null_handle, None);
    // This checks the real earlier frame phases, independent of whether the
    // authored furniture walk reaches an emission in this supplied view.
}

fn load_first_world() -> (
    ResidentRuntime,
    WorldSession,
    ProcessWorldMapRuntime,
    MapLoadState,
) {
    load_first_world_at_tier(2)
}

fn load_first_world_at_tier(
    tier: u8,
) -> (
    ResidentRuntime,
    WorldSession,
    ProcessWorldMapRuntime,
    MapLoadState,
) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let installation = Installation::open(&root).unwrap();
    let mut residency = ResidentRuntime::new(installation).unwrap();
    residency
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: tier,
                language: 0,
            },
        })
        .unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    residency
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    let grants = inf_formats::parse_grant_table(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let offline = crate::offline_prepare::offline_game(&grants).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
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
        &mut residency,
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
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    (residency, world, maps, map_load)
}

#[test]
fn first_world_slope_factors_match_the_recorded_load() {
    // The ProjectStateProbe census captured the load-time pair as
    // f90=0x5BB4A123 f8c=0x3595710E, stable across its hits. The port
    // precompute must reproduce those exact words from retail H12 data.
    let (_, world, _maps, _map_load) = load_first_world();
    assert_eq!(
        world.terrain.slope_factors(),
        [0x5bb4_a123u32 as i32, 0x3595_710eu32 as i32]
    );
}

#[test]
fn frame1_entity_slots_pin_their_remaining_opcode_owners() {
    // Static root-program opcode inventories for the retail frame-1 entity
    // slots. Pins the remaining owner list per model: when an opcode is
    // owned, its entry moves out of the unowned set below.
    let (residency, _, _maps, _map_load) = load_first_world();
    let resources = RenderResources::new(&residency);
    let inventory = |slot: u16| {
        let resource = resources
            .resolve(crate::render_resources::RenderResourceRequest {
                kind: crate::render_resources::RenderResourceKind::Model,
                argument: i32::from(slot),
            })
            .unwrap();
        let crate::render_resources::RenderResourceData::Model(model) = resource.data else {
            panic!("slot {slot} is not a model")
        };
        let mut counts = std::collections::BTreeMap::new();
        for instruction in &model.program.instructions {
            *counts.entry(instruction.raw_command as u8).or_insert(0) += 1;
        }
        (slot, counts)
    };
    // 223 (0xDF): 0x2B owned; 0x2D/0xDA/0xE6/0x07/0x1C remain.
    let (slot, counts) = inventory(223);
    assert_eq!(slot, 223);
    for opcode in [0x2du8, 0xda, 0xe6, 0x07, 0x1c] {
        assert!(counts.contains_key(&opcode), "slot 223 lacks {opcode:#04x}");
    }
    // 474 (0x1DA): 0x2C owned; only 0x07 remains unowned (cheapest chain
    // to pixels). The inventory still pins both opcodes in the program.
    let (slot, counts) = inventory(474);
    assert_eq!(slot, 474);
    assert!(counts.contains_key(&0x2c));
    assert!(counts.contains_key(&0x07));
    // 552 (0x228): 0xE6/0xDE/0xEA remain.
    let (slot, counts) = inventory(552);
    assert_eq!(slot, 552);
    for opcode in [0xe6u8, 0xde, 0xea] {
        assert!(counts.contains_key(&opcode), "slot 552 lacks {opcode:#04x}");
    }
    // 62: root is fully owned (0x03/0x0E); its 0x22 lives in a child.
    let (slot, counts) = inventory(62);
    assert_eq!(slot, 62);
    assert!(!counts.contains_key(&0x22));
}

#[test]
fn level1_frame_slots_pin_their_remaining_opcode_owners() {
    // Retail mission-18 frame-1 census (bundle
    // `.tmp/infestation-ttd/20260928-135746-345-Level1FrameDraw/`, frame
    // between the `world_draw` ordinals 58 and 130): 19 H1 slots, resolved
    // to model names through the corpus global-slot map (H1 slots are
    // global H8 slots). Pins the remaining owner list per model root
    // program: when an opcode is owned, its entry moves out of the
    // unowned set below. Children behind 0x0E instances stay open.
    let residency = load_level1_world();
    let resources = RenderResources::new(&residency);
    let inventory = |slot: u16| {
        let resource = resources
            .resolve(crate::render_resources::RenderResourceRequest {
                kind: crate::render_resources::RenderResourceKind::Model,
                argument: i32::from(slot),
            })
            .unwrap();
        let crate::render_resources::RenderResourceData::Model(model) = resource.data else {
            panic!("slot {slot} is not a model")
        };
        let mut counts = std::collections::BTreeMap::new();
        for instruction in &model.program.instructions {
            *counts.entry(instruction.raw_command as u8).or_insert(0) += 1;
        }
        (slot, counts)
    };
    // Executor-unowned root opcodes across the 19-slot census. Faces
    // (0xD0..0xE6 even) have no executor arm except 0xE2; 0x1A has no
    // arm at all.
    const UNOWNED: [u8; 8] = [0x1a, 0xd2, 0xd6, 0xda, 0xde, 0xe6, 0xea, 0xeb];
    // Fully owned roots: token04 (59), gate (62; its 0x22 lives in a
    // child), arrow (77), acam (117; eight 0x0E children stay open),
    // backdrop (278), commship (474).
    for slot in [59u16, 62, 77, 117, 278, 474] {
        let (_, counts) = inventory(slot);
        for opcode in UNOWNED {
            assert!(
                !counts.contains_key(&opcode),
                "slot {slot} unexpectedly uses {opcode:#04x}"
            );
        }
    }
    // Blocked roots, each pinned to its unowned subset:
    // pipes (47/49/50/51) + gunpower (175) + banners (386) +
    // alliedpost (398) on 0xE6; bunkers (76) on faces; runway (79,
    // the x10 dominant slot) on 0x1A; frame (99) on 0xDA; hudgnade
    // (180) on 0xD6/0xE6/0xEB; palm02 (373) on 0xD2/0xD6;
    // imortalgun (552) on 0xDE/0xE6/0xEA.
    let blocked: &[(u16, &[u8])] = &[
        (47, &[0xe6]),
        (49, &[0xe6]),
        (50, &[0xe6]),
        (51, &[0xe6]),
        (76, &[0xd2, 0xd6, 0xda]),
        (79, &[0x1a]),
        (99, &[0xda]),
        (175, &[0xe6]),
        (180, &[0xd6, 0xe6, 0xeb]),
        (373, &[0xd2, 0xd6]),
        (386, &[0xe6]),
        (398, &[0xe6]),
        (552, &[0xde, 0xe6, 0xea]),
    ];
    for (slot, expected) in blocked {
        let (_, counts) = inventory(*slot);
        for opcode in *expected {
            assert!(
                counts.contains_key(opcode),
                "slot {slot} lacks {opcode:#04x}"
            );
        }
    }
}

fn load_level1_world() -> ResidentRuntime {
    // Level-1 residency only (mission selection 18 → OVL 28 via the
    // loader's `selection+10` rule) for render-resource resolution,
    // which needs the activated leaves only. Full construction lives
    // in `load_level1_session` (fail-closed skips for the 12 unowned
    // selectors); see `world_loader.rs`.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let installation = Installation::open(&root).unwrap();
    let mut residency = ResidentRuntime::new(installation).unwrap();
    residency
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    residency
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    residency
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    residency
        .apply(ResidencyRequest::Activate { level: 28 })
        .unwrap();
    residency
}

fn load_level1_session() -> (
    ResidentRuntime,
    crate::world_backend::WorldSession,
    ProcessWorldMapRuntime,
) {
    // Full session load for mission selection 18 (level 1 → OVL 28),
    // using the same offline loader as selection 22 → OVL 32.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let installation = Installation::open(&root).unwrap();
    let mut residency = ResidentRuntime::new(installation).unwrap();
    residency
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    residency
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    let grants = inf_formats::parse_grant_table(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let offline = crate::offline_prepare::offline_game_with_selection(&grants, Some(18)).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
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
        &mut residency,
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
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    (residency, world, maps)
}

#[test]
fn offline_reload_preserves_mission_player_and_process_rng_custody() {
    let (mut residency, mut world, mut maps, mut map_load) = load_first_world();
    let player = world.player.unwrap();
    let command_process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let command_context = crate::object_runtime::PlayerCommandContext {
        process: Some(&command_process),
        executable: None,
        resident: None,
    };
    for raw_id in [1, 33, 65] {
        world
            .arena
            .apply_player_command(
                crate::object_runtime::PlayerCommandRequest {
                    player,
                    operation: crate::object_runtime::PlayerCommandOperation::Set {
                        raw_id,
                        silent: 1,
                    },
                },
                &command_context,
            )
            .unwrap();
    }
    let mut restored = world.offline_reload_request().unwrap().player;
    restored.owner_118 = 0x8000_0118;
    restored.owner_11c = 0xffff;
    restored.owner_11e = 0xabcd;
    restored.owner_120[..3].copy_from_slice(&[0x8000_0120, 0x7654_0124, u32::MAX]);
    restored.owner_120[3] = 0x8765_412c;
    // These raw bytes include embedded NUL and high data, not an identity string.
    restored.owner_13c = [0x4180_ff00, 0x81fe_0042];
    // Restore precedes respawn. Bit3 clear must preserve this full DWORD.
    restored.owner_100[0] &= !(1 << 3);
    world
        .arena
        .restore_player_save_prefix(player, &restored)
        .unwrap();
    let live = world.arena.world_player(player).unwrap().clone();
    assert_eq!(live.dword_12c, 0x8765_412c);
    world.mission.save.mission_selection = 18;
    world.mission.save.owner_3c[22] = 2;
    world.mission.save.owner_a4 = 0;
    world.rng = 0x8765_4321;
    world.fade.performance.tick(30_000, 30_000).unwrap();
    world.mission.dword_1c = crate::world_fade::update_manager_far(
        world.mission.dword_1c,
        world.mission.dword_20.unwrap(),
        world.fade.performance.particle_load_factor as i32,
    );
    // Predraw slew is not reload custody: 004874D0 seeds it from the new
    // manager/header pair. The ring and manager F themselves survive.
    world.fade.predraw(
        world.mission.dword_1c,
        world.mission.dword_20.unwrap(),
        0x7fff,
    );
    let retained_far = world.mission.dword_1c;
    let retained_timing = world.fade.performance.clone();
    let request = world.offline_reload_request().unwrap();
    assert_eq!(request.previous_world, 22);
    let mut rng = world.rng;
    assert_eq!(rng, 0x8765_4321);
    assert_eq!(
        request.visibility.as_ref().unwrap().manager_far,
        retained_far
    );
    assert_eq!(
        request.visibility.as_ref().unwrap().performance,
        retained_timing
    );
    assert_eq!(request.player.owner_a0, live.grants_9c);
    assert_eq!(request.player.owner_100, live.bits_100);
    assert_eq!(
        request.player.owner_150,
        live.values_150.map(|word| word as u32)
    );
    assert_eq!(request.player, restored);
    assert_eq!(request.player.owner_118, live.dword_118);
    assert_eq!(request.player.owner_11e, live.word_11e);
    assert_eq!(&request.player.owner_120[..3], &live.dwords_120_128);
    assert_eq!(request.player.owner_120[3], live.dword_12c);
    assert_eq!(request.player.owner_13c, [0x4180_ff00, 0x81fe_0042]);
    assert_eq!(request.player.owner_130, live.command_mask_130);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut load_owner = crate::world_backend::WorldLoadOwner::import_saved(request).unwrap();
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = crate::world_backend::load_offline_world(
        &mut residency,
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
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let mut reloaded = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    assert_eq!(reloaded.rng, rng);
    assert_eq!(reloaded.mission.dword_1c, retained_far);
    assert_eq!(reloaded.fade.performance, retained_timing);
    let loaded_header = reloaded.mission.dword_20.unwrap();
    assert_eq!(
        reloaded.fade.predraw(retained_far, loaded_header, 0),
        [
            (retained_far as i32 - loaded_header as i32) / 4,
            retained_far as i32
        ],
    );
    assert_eq!(reloaded.mission.current_world_a0, 18);
    assert_eq!(reloaded.mission.save.owner_3c[22], 6);
    assert_eq!(
        reloaded.world_start.pair_index,
        reloaded.mission.save.owner_a4
    );
    assert!(!reloaded.briefing.records.is_empty());
    let reloaded_player = reloaded
        .arena
        .world_player(reloaded.player.unwrap())
        .unwrap();
    assert_eq!(reloaded_player.bits_100[0] & (1 << 3), 0);
    assert_eq!(reloaded_player.dword_118, live.dword_118);
    // Actual loader 0046A880 clears +11C after the complete saved prefix restore.
    assert_eq!(reloaded_player.word_11c, 0);
    assert_eq!(reloaded_player.word_11e, live.word_11e);
    assert_eq!(reloaded_player.dwords_120_128, live.dwords_120_128);
    assert_eq!(reloaded_player.dword_12c, live.dword_12c);
    assert_eq!(reloaded_player.bytes_13c_143, live.bytes_13c_143);
    assert_eq!(
        reloaded
            .arena
            .world_player(reloaded.player.unwrap())
            .unwrap()
            .command_mask_130,
        live.command_mask_130
    );
}

#[test]
fn offline_reload_exports_the_retained_player_instead_of_the_first_list_player() {
    let (_, mut world, _maps, _map_load) = load_first_world();
    let first = world.player.unwrap();
    let second = world.arena.add_world_player([0x1357_2468; 6]);
    let mut saved = world.arena.export_player_save_state(second).unwrap();
    saved.owner_118 = 0x8000_0118;
    saved.owner_11e = 0xffff;
    saved.owner_120 = [1, 2, 3, 0x8000_012c];
    saved.owner_13c = [0x4180_ff00, 0x81fe_0042];
    world
        .arena
        .restore_player_save_prefix(second, &saved)
        .unwrap();
    world.player = Some(second);
    assert_eq!(world.arena.first_player(), Some(first));
    let before = format!("{world:?}");
    let request = world.offline_reload_request().unwrap();
    assert_eq!(request.player, saved);
    assert_eq!(format!("{world:?}"), before);

    // This closed reload policy needs its retained player even with a live
    // first player. It does not adopt 00449C10's distinct fresh optional lookup.
    world.player = None;
    let before = format!("{world:?}");
    assert!(matches!(
        world.offline_reload_request(),
        Err(crate::world_backend::WorldBackendError::MissingPlayer)
    ));
    assert_eq!(format!("{world:?}"), before);
}

#[test]
fn offline_reload_restores_12c_before_respawn_bit3_override() {
    let (mut residency, mut world, mut maps, mut map_load) = load_first_world();
    let player = world.player.unwrap();
    let mut restored = world.offline_reload_request().unwrap().player;
    restored.owner_120 = [0x1111_1111, 0x2222_2222, 0x3333_3333, 0x8000_012c];
    restored.owner_100[0] |= 1 << 3;
    world
        .arena
        .restore_player_save_prefix(player, &restored)
        .unwrap();
    assert_eq!(
        world.arena.world_player(player).unwrap().dword_12c,
        0x8000_012c
    );
    let request = world.offline_reload_request().unwrap();
    assert_eq!(request.player.owner_120, restored.owner_120);
    assert_ne!(request.player.owner_100[0] & (1 << 3), 0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut rng = world.rng;
    let mut load_owner = crate::world_backend::WorldLoadOwner::import_saved(request).unwrap();
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = crate::world_backend::load_offline_world(
        &mut residency,
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
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let reloaded = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    assert_eq!(reloaded.rng, rng);
    let live = reloaded
        .arena
        .world_player(reloaded.player.unwrap())
        .unwrap();
    assert_ne!(live.bits_100[0] & (1 << 3), 0);
    assert_eq!(live.dwords_120_128, [0x1111_1111, 0x2222_2222, 0x3333_3333]);
    // The actual spawn owner performs 0046A945 after the saved value is restored.
    assert!(reloaded.respawn.is_some());
    assert_eq!(live.dword_12c, 3);
}

#[test]
fn drive_policy_follows_the_retained_intro_controller() {
    let (_, mut intro, _intro_maps, _intro_map_load) = load_first_world();
    let player = intro.player.unwrap();
    let crate::world_backend::ArcadeDrivePolicy::FrozenIntroControlApproximation { controller } =
        intro.drive_policy
    else {
        panic!("retained intro controller must own the unfinished cinematic");
    };
    assert!(matches!(
        intro.arena.actor(controller).unwrap().extension,
        crate::object_runtime::ActorExtension::Selector88(_)
    ));
    let vehicle = intro.respawn.as_ref().unwrap().vehicle.unwrap();
    // The loader respawns after selector 88's after-load attachment; its
    // detach clears +D8, so the explicit cinematic policy is necessary.
    assert_eq!(intro.arena.player_actor(player).unwrap(), Some(vehicle));

    let (_, mut level1, _level1_maps) = load_level1_session();
    assert_eq!(
        level1.drive_policy,
        crate::world_backend::ArcadeDrivePolicy::PlayerVehicle
    );
    let player = level1.player.unwrap();
    let vehicle = level1.respawn.as_ref().unwrap().vehicle.unwrap();
    assert_eq!(level1.arena.player_actor(player).unwrap(), Some(vehicle));
}

#[test]
fn level1_frame_entities_bind_expected_models() {
    // Placement → +0x42 model linkage for mission 18 through the real
    // construction + code-2 override path (static cross-check:
    // H13 global → selector → initial H8 binding, code-2 operand at
    // the H14 allocation payload). The two retail commships (frame-1
    // slot 0x1DA) are placements 166/213 via selector-6 code-2
    // overrides to 474; selector 6 is unowned, so both skip here and
    // the commship stays open alongside it.
    let (_, world, _maps) = load_level1_session();
    let model_of = |index: usize| {
        let object = world.report.placements[index].unwrap();
        match entity_submission(&world.arena, object) {
            Ok(submission) => Some(submission.model_slot),
            Err(skip) => panic!("placement {index} unexpectedly skips: {skip:?}"),
        }
    };
    // imortalgun (0x228) x7 through code-2 overrides.
    for index in [4, 5, 6, 7, 92, 104, 105] {
        assert_eq!(model_of(index), Some(552), "placement {index}");
    }
    // gate (0x3E) x2, saber (0x299) x2, scientst (0xDF) x4.
    for index in [2, 81] {
        assert_eq!(model_of(index), Some(62), "placement {index}");
    }
    for index in [102, 103] {
        assert_eq!(model_of(index), Some(665), "placement {index}");
    }
    for index in [32, 33, 34, 35] {
        assert_eq!(model_of(index), Some(223), "placement {index}");
    }
    // hoverbot (188) and tomspawn (191) singletons.
    assert_eq!(model_of(188), Some(294));
    assert_eq!(model_of(191), Some(390));
    // Respawned vehicle is the acam (0x75) craft.
    let vehicle = world.respawn.unwrap().vehicle.unwrap();
    let submission = entity_submission(&world.arena, vehicle).unwrap();
    assert_eq!(submission.model_slot, 117);
    // Commships construct through selector 6 with code-2 overrides.
    for index in [166, 213] {
        assert_eq!(model_of(index), Some(474), "placement {index}");
    }
}

#[test]
fn camera_focus_resolves_to_the_owned_follow_target() {
    let (_, mut world, _maps, _map_load) = load_first_world();
    // Retail after-load publishes camera handle 08E10001 with a follow
    // target; the port owns the same handle and target.
    let handle = world.arena.camera_handle().unwrap();
    assert_eq!(
        handle,
        crate::object_runtime::ActorHandle::from_raw(0x08E1_0001)
    );
    let camera = world.arena.resolve_actor_handle(handle).unwrap().unwrap();
    let target = world
        .arena
        .actor_follow_target(world.arena.selector16(camera).unwrap().target_18c.unwrap())
        .unwrap();
    let focus = target.focus_08.unwrap();
    // Focus is the followed actor position raised by word_74: X/Z match
    // the respawned vehicle exactly.
    let vehicle = world.respawn.as_ref().and_then(|r| r.vehicle).unwrap();
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!(focus[0], position[0]);
    assert_eq!(focus[2], position[2]);
}

#[test]
fn entity_walk_classifies_every_placement_without_panicking() {
    let (_, world, _maps, _map_load) = load_first_world();
    let mut submitted = 0;
    let mut skips = FixtureCommonPacketSkips::default();
    let mut walked = 0;
    let mut planned: Vec<ObjectId> = world.report.placements.iter().flatten().copied().collect();
    if let Some(vehicle) = world.respawn.as_ref().and_then(|respawn| respawn.vehicle) {
        planned.push(vehicle);
    }
    for object in &planned {
        walked += 1;
        match entity_submission(&world.arena, *object) {
            Ok(submission) => {
                assert_eq!(submission.source, *object);
                assert_ne!(submission.model_slot, -1);
                submitted += 1;
            }
            Err(EntityDrawSkip::Expired) => skips.expired += 1,
            Err(EntityDrawSkip::NotActor) => skips.not_actor += 1,
            Err(EntityDrawSkip::NoModel) => skips.no_model += 1,
            Err(EntityDrawSkip::NoPosition) => skips.no_position += 1,
        }
    }
    assert_eq!(walked, planned.len());
    assert_eq!(
        submitted + skips.expired + skips.not_actor + skips.no_model + skips.no_position,
        walked
    );
    // Actors with bound models exist in the first world; lights, route
    // nodes, camera and player objects never reach H1 here.
    assert!(submitted > 0, "skips={skips:?}");
    assert!(skips.not_actor > 0, "skips={skips:?}");
}

#[test]
fn entity_submission_sign_extends_each_position_word() {
    let (_, mut world, _maps, _map_load) = load_first_world();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let placement_index = u32::from(world.arena.object(vehicle).unwrap().base.placement_word);
    world
        .arena
        .initialize_context(
            vehicle,
            crate::object_runtime::BaseContext {
                position_words: Some([0x8000, 0xffff, 0x8001]),
                placement_index,
            },
        )
        .unwrap();
    assert_eq!(
        entity_submission(&world.arena, vehicle).unwrap().position,
        [-32768, -1, -32767]
    );
    let camera = world
        .arena
        .resolve_actor_handle(world.arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    let target = world
        .arena
        .follow_target_resource(crate::object_runtime::FollowTargetRequest::resolve_actor(
            world.arena.actor(vehicle).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    world
        .arena
        .replace_camera_target(
            camera,
            Some(target),
            crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
        )
        .unwrap();
    let focus = world
        .arena
        .actor_follow_target(world.arena.selector16(camera).unwrap().target_18c.unwrap())
        .unwrap()
        .focus_08
        .unwrap()
        .map(|word| i32::from(word as i16));
    let submission = entity_submission(&world.arena, vehicle).unwrap();
    assert_eq!([focus[0], focus[2]], [-32768, -32767]);
    assert_eq!(submission.position[0].wrapping_sub(focus[0]), 0);
    assert_eq!(submission.position[2].wrapping_sub(focus[2]), 0);
}

/// Loaded Draw-acquired camera fixture, without a preceding camera Tick.
/// These are native admission/prefix checks, not a retail frame census.
fn assert_native_loaded_camera_object_draw(world: &WorldSession, outcome: &WorldDrawOutcome) {
    // Inspect actual typed sources after the real manager; no placement, slot,
    // model number or captured coordinate selects the callback policy.
    let caches: Vec<_> = world
        .arena
        .list(crate::object_runtime::ObjectList::Flag0Primary)
        .iter()
        .filter_map(|node| {
            world
                .arena
                .selector60_actor(node.object)
                .ok()
                .map(|state| (node.object, state.byte_184, state.position_198))
        })
        .collect();
    assert_eq!(caches.len(), 1, "typed class60 sources: {caches:?}");
    assert_eq!(caches[0].1, 1, "actual captured flag: {caches:?}");
    assert!(caches[0].2.is_some(), "actual retained row: {caches:?}");
    assert_eq!(
        outcome.submitted_models, 2,
        "separate sky and the cached-row source complete: {outcome:?}"
    );
    assert_eq!(
        outcome.skips.object_draw, 0,
        "this loaded-camera draw has no remaining object omission: {outcome:?}"
    );
    assert!(
        world
            .arena
            .list(crate::object_runtime::ObjectList::Flag0Secondary)
            .is_empty(),
        "normal physical list drain completed"
    );
    let near_actors: Vec<_> = world
        .arena
        .list(crate::object_runtime::ObjectList::Flag0Primary)
        .iter()
        .filter_map(|node| {
            let state = world.arena.object(node.object).unwrap();
            let ObjectClass::Actor(actor) = &state.class else {
                return None;
            };
            (state.base.flags & 0x0200_0000 != 0 && actor.rotating.visual.model_word != u16::MAX)
                .then_some(node.object)
        })
        .collect();
    assert!(
        near_actors.is_empty(),
        "the first Draw-acquired camera admits no actor through the near cone: {near_actors:?}"
    );
    assert!(
        world
            .arena
            .list(crate::object_runtime::ObjectList::Flag0Primary)
            .iter()
            .all(|node| world.arena.object(node.object).unwrap().base.flags & 1 != 0),
        "every surviving physical Flag0 node retains active membership"
    );
}

#[test]
fn native_draw_submits_first_world_entities_to_real_pixels() {
    let (residency, mut world, _maps, _map_load) = load_first_world();
    let startup = frontend_startup(
        &residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let resources = RenderResources::new(&residency);
    let mut frame = SoftwareFrame::new(640, 480, [16, 24, 64, 255]);
    let supplied_scene_config = SoftwareSceneConfig {
        environment: crate::model_execution::ModelEnvironment {
            view: crate::frontend_scene::SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            projection: ModelProjection {
                focal: [0x200, 0x200],
                viewport: [640, 480],
                center: [320, 240],
                fade: [0x10_0000; 2],
            },
            coordinate_shift: 0,
            fog_factor: 0x100_0000,
            lighting: ModelLighting {
                direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
                directional_word: 0x0080_8080,
                ambient: [32; 3],
                ambient_word: 0xab20_2020,
            },
        },
        primitive_mode: 0,
        far_color: 0,
        effect_color: 0xcd00_0000,
        color_layout: NativeColorLayout {
            red_shift: 10,
            green_shift: 5,
            blue_shift: 0,
        },
        renderer: crate::d3d_flat::D3dFlatState {
            material_shifts: [6, 1, 4],
            near_depth: 64,
            manages_depth: true,
            depth_enable: 1,
            depth_write: 1,
        },
        pipeline: crate::software_frame::D3dRasterState {
            viewport: [0, 0, 639, 479],
            depth_enabled: false,
            depth_write: true,
            depth_function: 4,
            alpha_blend_enabled: true,
            shade_mode: 2,
            cull_mode: 1,
        },
        texture_state: crate::software_frame::SoftwareTextureState {
            upload: crate::d3d_texture::NativeTextureUploadState {
                source_rgb_shifts: [10, 5, 0],
                global_policy: 8,
            },
            sampling: D3dTextureSampling {
                min_filter: 1,
                mag_filter: 1,
                texture_blend: 4,
                address: 1,
                color_key_enabled: false,
                alpha_test_enabled: false,
            },
            secondary_texture: false,
        },
        queue_capacity: 0xc8000,
        execution_limits: ModelExecutionLimits {
            instructions: 100_000,
            invocation_depth: 16,
        },
    };
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (supplied_scene_config).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: supplied_scene_config,
    })
    .unwrap();
    let mut host = ResidentSceneHost {
        resources,
        backend: &mut scene,
    };
    // Production refreshes the scene terrain every epoch; the test mirrors
    // that wiring so grounding controls see resident terrain.
    host.backend.set_terrain(Some(world.terrain.clone()));
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    let mut object_light_policy =
        FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
    let outcome = draw_world_frame(
        WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
            weapon_tables: None,
            object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            ).unwrap(),
            attract_demo: &false,
            world: &mut world,
            animation: &AnimationClock::default(),
            particles: &mut Default::default(),
            light_grid: &mut Default::default(),
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            camera: camera_inputs.inputs(&residency, 0),
            furniture: furniture.inputs(),
            sound: None,
        },
        WorldFrameInputs {
            template_focal: startup.projection.focal,
            focal_src: startup.projection.focal,
            focal_kick: 1.0,
            scale_5c0: 0,
            special_view_active: false,
            surface: [640, 480],
            z_buffering: true,
            dynamic_lighting: false,
            arena_bytes: RETAIL_ARENA_BYTES,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            guided_mounts: false,
            camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
            settings_7c: 0,
            settings_70: 0,
            network_initials: [[0; 6]; crate::network::game::SLOTS],
            network_team_flag: 0,
            infrared: false,
            game: crate::presentation::GameView::Retail,
            game_far: None,
        },
        &mut host,
    )
    .unwrap();
    // First-frame view setup publishes the template focals from zero.
    assert_eq!(outcome.focals, startup.projection.focal);
    assert_eq!(outcome.depth, (1, 1));
    let RenderResourceData::Material(material) = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument: 0x74,
        })
        .unwrap()
        .data
    else {
        unreachable!()
    };
    assert_eq!(
        host.backend.primitive_state.far_color,
        host.backend.color_layout().convert_material(material)
    );
    assert!(!outcome.skipped_terrain, "outcome={outcome:?}");
    assert!(outcome.terrain_commands > 0, "outcome={outcome:?}");
    assert!(!outcome.skipped_sky);
    // Current loaded-camera submission is distinct from retail camera
    // timeline/visibility acceptance; supplied pose ground coverage follows.
    assert_native_loaded_camera_object_draw(&world, &outcome);

    assert!(
        host.backend.pixels.stats.fragments > 0,
        "outcome={outcome:?} pixels={:?}",
        host.backend.pixels.stats
    );
    assert!(host.backend.stats.model_commands > 0, "outcome={outcome:?}");
    // Focal state persists; the first Draw independently acquires the camera.
    assert_eq!(world.view_focals, startup.projection.focal);
    assert!(world.native_camera.is_some());

    // Supplied live camera fields keep all
    // ten basis DWORDs and publishes signed WORD coordinates plus shift16.
    // It also renders without a respawned vehicle, as intro helpers can own
    // the view independently of that separately constructed actor.
    world.respawn = None;
    let mut basis = MenuBasis::INITIAL;
    basis.words[0] = 17;
    let pose = crate::object_runtime::CameraPose {
        position_words: [0xfff0, 0x8000, 0x8001],
        basis,
        target_state: Some(1),
    };
    supply_native_camera(&mut world, pose.position_words, pose.basis);
    for position_words in [pose.position_words, [0xffef, 0x8001, 0x8002]] {
        // Draw must see a fresh native field write after the prior draw.
        supply_native_camera(&mut world, position_words, basis);
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
        draw_world_frame(
            WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
                weapon_tables: None,
                object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                    inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                ).unwrap(),
                attract_demo: &false,
                world: &mut world,
                animation: &AnimationClock::default(),
                particles: &mut Default::default(),
                light_grid: &mut Default::default(),
                object_light_policy: &mut object_light_policy,
                object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
                executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes())
                    .unwrap(),
                camera: camera_inputs.inputs(&residency, 0),
                furniture: furniture.inputs(),
                sound: None,
            },
            WorldFrameInputs {
                template_focal: startup.projection.focal,
                focal_src: startup.projection.focal,
                focal_kick: 1.0,
                scale_5c0: 0,
                special_view_active: false,
                surface: [640, 480],
                z_buffering: true,
                dynamic_lighting: false,
                arena_bytes: RETAIL_ARENA_BYTES,
                mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
                hovercraft_fans: false,
                guided_mounts: false,
                camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
                settings_7c: 0,
                settings_70: 0,
                network_initials: [[0; 6]; crate::network::game::SLOTS],
                network_team_flag: 0,
                infrared: false,
                game: crate::presentation::GameView::Retail,
                game_far: None,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(
            host.backend.environment.view,
            SceneView {
                eye: position_words.map(|word| i32::from(word as i16)),
                basis,
            }
        );
        assert_eq!(host.backend.environment.coordinate_shift, 16);
        let receipt = world.last_draw.unwrap();
        assert!(receipt.native_camera);
        assert_eq!(
            receipt.eye,
            position_words.map(|word| i32::from(word as i16))
        );
        assert_eq!(receipt.basis_words, basis.words);
        assert_eq!(receipt.coordinate_shift, 16);
        assert_eq!(receipt.projection.viewport, [640, 480]);
        assert_eq!(receipt.projection.center, [320, 240]);
        assert_eq!(
            receipt.terrain_height_at_eye,
            world
                .terrain
                .height_at([position_words[0], position_words[2]])
        );
    }

    // Passive PORT-side frame 380 pose that exposed the infinite frontend
    // fade wrapping AA10's footprint WORDs. It is a regression fixture,
    // never native-camera acceptance or a source of runtime behavior.
    let eye = [14073, 1669, -22585];
    let basis = MenuBasis {
        words: [
            0, 1065131800, 0, 1042665598, 3181783145, 1063095451, 1056777481, 3188683171,
            3204465630, 1062903982,
        ],
    };
    supply_native_camera(&mut world, eye.map(|v| v as u16), basis);
    world.mission.dword_1c = crate::world_fade::INITIAL_WORLD_FAR;
    world.fade = crate::world_fade::WorldFadeState::loaded(
        world.mission.dword_1c,
        world.mission.dword_20.unwrap(),
        Default::default(),
    );
    world.fade.performance.tick(125_000, 125_000).unwrap();
    world.mission.dword_1c = crate::world_fade::update_manager_far(
        world.mission.dword_1c,
        world.mission.dword_20.unwrap(),
        world.fade.performance.particle_load_factor as i32,
    );
    struct NoParticleAudio;
    impl crate::process_particles::births::ParticleBirthAudio for NoParticleAudio {
        type Error = std::convert::Infallible;
        fn explosion_sound(
            &mut self,
            _: crate::process_particles::audio::ParticleSoundRequest,
            _: &mut crate::process_particles::ProcessParticleRuntime,
            _: &mut u32,
        ) -> Result<(), Self::Error> {
            panic!("central type3 has no native audio enqueue");
        }
    }
    let mut particles = crate::process_particles::ProcessParticleRuntime::default();
    let mut lights = crate::world_lighting::WorldLightGrid::default();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut particle_position: [u16; 3] = std::array::from_fn(|axis| {
        (eye[axis] + (f64::from(f32::from_bits(basis.words[7 + axis])) * 2560.0).trunc() as i32)
            as u16
    });
    particle_position[1] = world
        .terrain
        .height_at([particle_position[0], particle_position[2]]) as u16;
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(
            world.arena.actor_handles().unwrap().null_handle().raw(),
        )),
    };
    let particle = particles
        .birth_central(
            crate::process_particles::births::ParticleBirthRequest {
                particle_type: 3,
                position: particle_position,
                velocity: None,
                flags: 0,
                inputs: crate::process_particles::births::ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &birth_process,
                },
            },
            executable,
            &mut world.rng,
            &mut NoParticleAudio,
        )
        .unwrap()
        .unwrap();
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    let mut object_light_policy =
        FixtureObjectLightPolicy::new(true, world.terrain.sampling().spacing);
    let outcome = draw_world_frame(
        WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
            weapon_tables: None,
            object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            ).unwrap(),
            attract_demo: &false,
            world: &mut world,
            animation: &AnimationClock::default(),
            particles: &mut particles,
            light_grid: &mut lights,
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            camera: camera_inputs.inputs(&residency, 0),
            furniture: furniture.inputs(),
            sound: None,
        },
        WorldFrameInputs {
            template_focal: startup.projection.focal,
            focal_src: startup.projection.focal,
            focal_kick: 1.0,
            scale_5c0: 0,
            special_view_active: false,
            surface: [640, 480],
            z_buffering: true,
            dynamic_lighting: true,
            arena_bytes: RETAIL_ARENA_BYTES,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            guided_mounts: false,
            camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
            settings_7c: 0,
            settings_70: 0,
            network_initials: [[0; 6]; crate::network::game::SLOTS],
            network_team_flag: 0,
            infrared: false,
            game: crate::presentation::GameView::Retail,
            game_far: None,
        },
        &mut host,
    )
    .unwrap();
    assert!(outcome.particle_commands > 0, "{outcome:?}");
    assert_eq!(outcome.particle_omissions, 0);
    assert!(lights.cells().iter().any(|&byte| byte != 0));
    assert!(particles.pending(1).unwrap().is_empty());
    assert_eq!(particles.active(1).unwrap().front(), Some(&particle));
    assert!(outcome.terrain_commands > 0, "outcome={outcome:?}");
    assert!(!outcome.skipped_terrain);
    let receipt = world.last_draw.unwrap();
    assert_eq!(receipt.outcome, outcome);
    assert!(receipt.native_camera);
    assert_eq!(receipt.eye, eye);
    assert_eq!(receipt.basis_words, basis.words);
    assert_eq!(receipt.coordinate_shift, 16);
    assert_eq!(receipt.projection.focal, startup.projection.focal);
    assert_eq!(receipt.projection.viewport, [640, 480]);
    assert_eq!(receipt.projection.center, [320, 240]);
    assert_eq!(receipt.projection.fade, [2224, 10176]);
    assert_eq!(
        receipt.terrain_height_at_eye,
        world.terrain.height_at([eye[0] as u16, eye[2] as u16])
    );
    // Genuine release enters the real H13 factory on the next Draw. Draw
    // acquires a new camera without a pose Tick or clearing the prior error.
    let camera = world.native_camera.unwrap();
    world
        .arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: camera,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    // Explicit empty-listener admission for this factory/pixel regression.
    // Unowned listener callbacks remain real factory errors in production.
    world.listeners.clear();
    world.camera_failure = Some("retained preceding pose failure".into());
    let mut object_light_policy =
        FixtureObjectLightPolicy::new(true, world.terrain.sampling().spacing);
    draw_world_frame(
        WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
            weapon_tables: None,
            object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            ).unwrap(),
            attract_demo: &false,
            world: &mut world,
            animation: &AnimationClock::default(),
            particles: &mut particles,
            light_grid: &mut lights,
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            executable,
            camera: camera_inputs.inputs(&residency, 0),
            furniture: furniture.inputs(),
            sound: None,
        },
        WorldFrameInputs {
            template_focal: startup.projection.focal,
            focal_src: startup.projection.focal,
            focal_kick: 1.0,
            scale_5c0: 0,
            special_view_active: false,
            surface: [640, 480],
            z_buffering: true,
            dynamic_lighting: true,
            arena_bytes: RETAIL_ARENA_BYTES,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            guided_mounts: false,
            camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
            settings_7c: 0,
            settings_70: 0,
            network_initials: [[0; 6]; crate::network::game::SLOTS],
            network_team_flag: 0,
            infrared: false,
            game: crate::presentation::GameView::Retail,
            game_far: None,
        },
        &mut host,
    )
    .unwrap();
    let fresh = world.native_camera.unwrap();
    assert_ne!(fresh, camera);
    assert_eq!(
        world.camera_failure.as_deref(),
        Some("retained preceding pose failure")
    );
    assert_eq!(
        host.backend.environment.view.eye,
        world.arena.camera_draw_request(fresh).unwrap().eye
    );
}

#[test]
fn level1_draw_uses_native_admission_and_keeps_common_resource_fixture() {
    // Mission-18 native-draw pin, mirroring
    // `native_draw_submits_first_world_entities_to_real_pixels`.
    // Values below are machine-measured on first green: each newly
    // owned opcode moves submissions out of `unresolvable`, so this
    // pin is updated by every primitive/control owner commit. The
    // retail frame-1 submission set (19 H1 slots, `0x4F` dominant)
    // lives in GAMEPLAY_RUNTIME.md; pixel comparison against retail
    // stays open until terrain/furniture paths submit.
    let (residency, mut world, _maps) = load_level1_session();
    let startup = frontend_startup(
        &residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let resources = RenderResources::new(&residency);
    let mut frame = SoftwareFrame::new(640, 480, [16, 24, 64, 255]);
    let supplied_scene_config = SoftwareSceneConfig {
        environment: crate::model_execution::ModelEnvironment {
            view: crate::frontend_scene::SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            projection: ModelProjection {
                focal: [0x200, 0x200],
                viewport: [640, 480],
                center: [320, 240],
                fade: [0x10_0000; 2],
            },
            coordinate_shift: 0,
            fog_factor: 0x100_0000,
            lighting: ModelLighting {
                direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
                directional_word: 0x0080_8080,
                ambient: [32; 3],
                ambient_word: 0xab20_2020,
            },
        },
        primitive_mode: 0,
        far_color: 0,
        effect_color: 0xcd00_0000,
        color_layout: NativeColorLayout {
            red_shift: 10,
            green_shift: 5,
            blue_shift: 0,
        },
        renderer: crate::d3d_flat::D3dFlatState {
            material_shifts: [6, 1, 4],
            near_depth: 64,
            manages_depth: true,
            depth_enable: 1,
            depth_write: 1,
        },
        pipeline: crate::software_frame::D3dRasterState {
            viewport: [0, 0, 639, 479],
            depth_enabled: false,
            depth_write: true,
            depth_function: 4,
            alpha_blend_enabled: true,
            shade_mode: 2,
            cull_mode: 1,
        },
        texture_state: crate::software_frame::SoftwareTextureState {
            upload: crate::d3d_texture::NativeTextureUploadState {
                source_rgb_shifts: [10, 5, 0],
                global_policy: 8,
            },
            sampling: D3dTextureSampling {
                min_filter: 1,
                mag_filter: 1,
                texture_blend: 4,
                address: 1,
                color_key_enabled: false,
                alpha_test_enabled: false,
            },
            secondary_texture: false,
        },
        queue_capacity: 0xc8000,
        execution_limits: ModelExecutionLimits {
            instructions: 100_000,
            invocation_depth: 16,
        },
    };
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (supplied_scene_config).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: supplied_scene_config,
    })
    .unwrap();
    let mut host = ResidentSceneHost {
        resources,
        backend: &mut scene,
    };
    host.backend.set_terrain(Some(world.terrain.clone()));
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    let mut object_light_policy =
        FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
    let outcome = draw_world_frame(
        WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
            weapon_tables: None,
            object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            ).unwrap(),
            attract_demo: &false,
            world: &mut world,
            animation: &AnimationClock::default(),
            particles: &mut Default::default(),
            light_grid: &mut Default::default(),
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            camera: camera_inputs.inputs(&residency, 0),
            furniture: furniture.inputs(),
            sound: None,
        },
        WorldFrameInputs {
            template_focal: startup.projection.focal,
            focal_src: startup.projection.focal,
            focal_kick: 1.0,
            scale_5c0: 0,
            special_view_active: false,
            surface: [640, 480],
            z_buffering: true,
            dynamic_lighting: false,
            arena_bytes: RETAIL_ARENA_BYTES,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            guided_mounts: false,
            camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
            settings_7c: 0,
            settings_70: 0,
            network_initials: [[0; 6]; crate::network::game::SLOTS],
            network_team_flag: 0,
            infrared: false,
            game: crate::presentation::GameView::Retail,
            game_far: None,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(outcome.focals, startup.projection.focal);
    assert_eq!(outcome.depth, (1, 1));
    assert!(!outcome.skipped_terrain, "outcome={outcome:?}");
    assert!(outcome.terrain_commands > 0, "outcome={outcome:?}");
    // Sky draws through the new worker path now.
    assert!(!outcome.skipped_sky);
    // Native visibility now precedes captured-source model execution. The
    // old21-placement completion count did not apply these manager gates.
    assert_native_loaded_camera_object_draw(&world, &outcome);
    // Five authored selector-87 annotations now construct successfully.
    // Their independent map draw does not introduce world model submissions.
    assert_eq!(world.arena.map_annotation_list().len(), 5);
    //The actual Flag0 manager admits class-specific nonactor callbacks.
    //Historical common-packet fixture classification is a separate test.
    let pixels = &host.backend.pixels.stats;
    // The loaded camera's current fields now supply this view. Obsolete
    // fit-at-spawn primitive/pixel totals do not apply to this new input.
    assert_eq!(pixels.unsupported, 0, "pixels={pixels:?}");
    assert!(pixels.fragments > 0, "pixels={pixels:?}");
    // Historical independent common-root resource fixture. It bypasses the
    // production Flag0 manager and does not assert native admission, lighting
    // or the complete world frame: which bound model slots resolve and finish. Each newly owned
    // opcode moves its slots from `failed` to `ok`, so this pin is
    // updated by every primitive/control owner commit.
    let mut planned: Vec<(Option<usize>, crate::object_runtime::ObjectId)> = world
        .report
        .placements
        .iter()
        .enumerate()
        .filter_map(|(index, object)| object.map(|object| (Some(index), object)))
        .collect();
    if let Some(vehicle) = world.respawn.as_ref().and_then(|respawn| respawn.vehicle) {
        planned.push((None, vehicle));
    }
    let mut roster: std::collections::BTreeMap<i16, Vec<Option<usize>>> = Default::default();
    let mut failed: std::collections::BTreeMap<i16, Vec<Option<usize>>> = Default::default();
    for (index, object) in planned {
        let submission = match entity_submission(&world.arena, object) {
            Ok(submission) => submission,
            Err(_) => continue,
        };
        let result = host
            .submit(crate::frontend_scene::SceneCommand::BeginBatch)
            .and_then(|()| {
                host.backend
                    .draw_object_model(SoftwareObjectModelDrawRequest {
                        terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
                        root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                        context: &crate::object_runtime::ObjectDrawContext {
                            view: host.backend.environment.view,
                            near: host.backend.environment.projection.fade[0],
                            far: host.backend.environment.projection.fade[1],
                            far_color: host.backend.primitive_state.far_color,
                        },
                        packet: submission,
                        arena: &mut world.arena,
                        animation: &AnimationClock::default(),
                        executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                    })
                    .map_err(crate::scene_resources::SceneResourceError::Backend)
                    .and_then(|()| host.submit(crate::frontend_scene::SceneCommand::FlushBatch))
            });
        match result {
            Ok(()) => roster.entry(submission.model_slot).or_default().push(index),
            Err(_) => failed.entry(submission.model_slot).or_default().push(index),
        }
    }
    // Resource bindings remain intact despite distance rejection in this
    // current view. Completion does not claim a retail primitive census.
    // The barrier (placement 190) binds model 480; in production its own +2C
    // 00411570 draws it, but this fixture draws every bound model.
    assert_eq!(roster.len(), 13, "roster={roster:?}");
    assert_eq!(roster.get(&480).unwrap(), &vec![Some(190)]);
    assert_eq!(roster.get(&62).unwrap(), &vec![Some(2), Some(81)]);
    // The four pickup spawners bind model 1. Their flag 0 is clear, so the
    // native manager never draws them; only this fixture does.
    assert_eq!(
        roster.get(&1).unwrap(),
        &vec![Some(47), Some(50), Some(51), Some(52)]
    );
    // The three selector-15 factories draw model 0x9D.
    assert_eq!(roster.get(&157).unwrap(), &vec![Some(0), Some(1), Some(3)]);
    assert_eq!(roster.get(&117).unwrap(), &vec![None]);
    assert_eq!(roster.get(&474).unwrap().len(), 2);
    assert_eq!(
        roster.get(&552).unwrap(),
        &vec![
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(92),
            Some(104),
            Some(105)
        ]
    );
    assert_eq!(roster.get(&390).unwrap(), &vec![Some(191)]);
    assert_eq!(
        roster.get(&223).unwrap(),
        &vec![Some(32), Some(33), Some(34), Some(35)]
    );
    assert_eq!(roster.get(&294).unwrap(), &vec![Some(188)]);
    assert_eq!(roster.get(&665).unwrap(), &vec![Some(102), Some(103)]);
    // The selector-37 bridge and the seven selector-39 course targets.
    assert_eq!(roster.get(&392).unwrap(), &vec![Some(106)]);
    assert_eq!(
        roster.get(&686).unwrap(),
        &vec![
            Some(96),
            Some(97),
            Some(98),
            Some(189),
            Some(201),
            Some(202),
            Some(203)
        ]
    );
    assert!(failed.is_empty(), "failed={failed:?}");
}

#[test]

fn level1_furniture_walk_matches_direct_enumeration() {
    // Mission-18 H12 census through the owned `0044CD00` walk.
    // Pair provenance: world 28 leaves H14 `0x2000` clear (pinned
    // below), retaining the boot pair `(0x80, 0x7F)` — and the
    // furniture bind oracle proves it live (odd-bucket lookups
    // bind, unreachable under any even-only pair). Under the fine
    // pair every cell is reachable (decrement 128), the odd-x flag
    // test bypasses, and `[0, 0x1FF]` is the full x window — so one
    // 512-row walk must return all 293 sparse records exactly once.
    // Lookup bytes stay raw: the lookup-to-H1 map lives in the
    // runtime H8 tables (`0055EEE4` family); the bind oracle banks
    // the frame-1 pairs separately. The domain owner (terrain
    // parent frustum rows) replays this shape per draw.
    let (_, world, _maps) = load_level1_session();
    let buckets: Vec<inf_formats::TerrainBucket> = world.terrain.buckets().to_vec();
    assert_eq!(buckets.len(), 512);
    let mut coords = vec![0u16];
    for _ in 0..512 {
        coords.extend_from_slice(&[0, 0x1ff, 0]);
    }
    let domain = crate::terrain_furniture::FurnitureDomain {
        origin_x: 0,
        origin_z: 0,
        scale: 128,
        mask: 0x7f,
        coords,
        rows: 512,
    };
    let hits = crate::terrain_furniture::walk_furniture_records(&buckets, &domain).unwrap();
    let direct: usize = buckets.iter().map(|bucket| bucket.records.len()).sum();
    assert_eq!(direct, 293);
    assert_eq!(hits.len(), direct, "fine walk returns every record once");
    let mut lookups = std::collections::BTreeMap::new();
    for hit in &hits {
        *lookups.entry(hit.record.resource_lookup.raw()).or_insert(0) += 1;
    }
    // Full OVL-28 lookup population banked 2026-09-29 (frame-1
    // binds are a subset: every bind lookup fits these counts,
    // with lookup 95 exactly exhausted at 2/2).
    assert_eq!(
        lookups.into_iter().collect::<Vec<_>>(),
        [
            (3, 1),
            (13, 7),
            (14, 16),
            (15, 14),
            (16, 15),
            (17, 24),
            (18, 4),
            (19, 8),
            (21, 3),
            (25, 4),
            (26, 29),
            (33, 3),
            (34, 6),
            (36, 1),
            (38, 1),
            (39, 2),
            (43, 2),
            (50, 15),
            (51, 40),
            (65, 2),
            (66, 70),
            (67, 2),
            (68, 1),
            (69, 4),
            (72, 1),
            (73, 1),
            (74, 1),
            (95, 2),
            (98, 1),
            (100, 3),
            (101, 1),
            (102, 1),
            (109, 6),
            (116, 2),
        ]
    );
    // Item positions through the owned `0044CEF0` setup: real OVL-28
    // records/cells with synthetic anchors (world-draw register
    // leftovers, open), a flat light table, and the height shift
    // from the world sampling. Anchors only move anchor cells and
    // light — xyz below is anchor-independent retail geometry.
    let shift = match world.terrain.sampling().height_scale {
        crate::terrain_runtime::TerrainHeightScale::Units16 => 4,
        crate::terrain_runtime::TerrainHeightScale::Units32 => 5,
    };
    let setup_inputs = crate::terrain_furniture::FurnitureSetupInputs {
        cells: world.terrain.cells(),
        height_shift: shift,
        mask: 0x7f,
        anchors: crate::terrain_furniture::furniture_anchors(0, 0),
        light_cells: &vec![0x40u8; 90 * 90 * 3],
        outside_light_word: 0,
        tint_word: 0,
    };
    let mut positions = Vec::new();
    for hit in &hits {
        let item = crate::terrain_furniture::setup_furniture_item(hit.record, hit.z, &setup_inputs)
            .unwrap()
            .unwrap();
        positions.push(item.xyz);
    }
    assert_eq!(positions.len(), direct);
    assert_eq!(positions.len(), direct);
    assert_eq!(
        &positions[..3],
        &[
            [32000, 288, 41216],
            [32512, 288, 41216],
            [30720, 288, 40960]
        ]
    );
}

#[test]
fn level1_world_flags_keep_the_fine_spacing_pair() {
    // H14 world flags select the process spacing policy through
    // `004502C0`: `0x2000` set forces coarse `(0x100, 0xFF)`,
    // clear retains the explicitly supplied fine pair `(0x80, 0x7F)`.
    // Neither world sets it — and the furniture bind oracle proves
    // the fine pair live for mission 18 (odd-bucket lookups bind,
    // unreachable under any even-only pair). `0x80` set selects
    // the Units16 height scale in the same flags word.
    let runtime = load_level1_world();
    let placements = crate::world_placement::WorldPlacements::new(
        &runtime,
        crate::terrain_runtime::TerrainSpacing::Units256,
    )
    .unwrap();
    assert_eq!(placements.world().flags, 0xa0);
    let (residency22, _, _maps, _map_load) = load_first_world();
    let placements22 = crate::world_placement::WorldPlacements::new(
        &residency22,
        crate::terrain_runtime::TerrainSpacing::Units256,
    )
    .unwrap();
    assert_eq!(placements22.world().flags, 0x20);
}

// This phase matrix checks real acquisition and the retained typed Setup/material
// prefix. It does not claim native per-component context faults or Rust cadence.
#[test]
fn predraw_acquisition_and_late_boundaries_preserve_reached_owners() {
    use crate::object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        CameraFactoryInputs, CreationListener, CreationListenerEffects, CreationListenerError,
        CreationListeners, ObjectConstructionError, ObjectError, PlayerAttachError,
    };
    #[derive(Clone, Copy, Debug)]
    enum Arm {
        ResolvedNoOwner,
        ResolvedNoValue,
        MissingOwner,
        MissingValue,
        MissingResidency,
        MaterialAfterFactory,
        UnknownPosition,
    }
    struct NoListeners;
    impl CreationListenerEffects for NoListeners {
        fn invoke(
            &mut self,
            _: CreationListener,
            _: ObjectId,
            _: &mut CreationListeners,
            _: &mut ObjectArena,
        ) -> Result<(), CreationListenerError> {
            panic!("no supplied listener")
        }
    }
    for arm in [
        Arm::ResolvedNoOwner,
        Arm::ResolvedNoValue,
        Arm::MissingOwner,
        Arm::MissingValue,
        Arm::MissingResidency,
        Arm::MaterialAfterFactory,
        Arm::UnknownPosition,
    ] {
        let (residency, mut world, _maps, _map_load) = load_first_world();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let inactive = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        let mut camera_inputs = WorldCameraFixture::new(&assets);
        let original_camera = world
            .arena
            .resolve_actor_handle(world.arena.camera_handle().unwrap())
            .unwrap()
            .unwrap();
        if matches!(
            arm,
            Arm::MissingOwner
                | Arm::MissingValue
                | Arm::MissingResidency
                | Arm::MaterialAfterFactory
        ) {
            world
                .arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: original_camera,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap();
        }
        if matches!(arm, Arm::ResolvedNoValue | Arm::MissingValue) {
            camera_inputs.process.process_null_handle = None;
        }
        if matches!(arm, Arm::MaterialAfterFactory) {
            // Complete factory domain supplies no unowned creation callback.
            world.listeners.clear();
        }
        if matches!(arm, Arm::UnknownPosition) {
            // An independently constructed actual camera with declared unknown
            // allocation WORDs. Old world-worker IDs are never reached here.
            world.listeners = CreationListeners::new();
            world.arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
                owner_count: 17,
                initial_capacity: 16,
                null_handle: ActorHandle::from_raw(0),
            })
            .unwrap();
            let math = inf_formats::native_basis::NativeBasisMath::new(camera_inputs.sine);
            let mut effects = NoListeners;
            let allocation = ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: None,
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 17,
                    world_flags: None,
                    sine_quarter: &camera_inputs.sine,
                },
            };
            world
                .arena
                .world_camera(&mut CameraFactoryInputs {
                    process: Some(&camera_inputs.process),
                    runtime: &residency,
                    math: &math,
                    allocation,
                    local_owner: 1,
                    settings: camera_inputs.settings,
                    listeners: &mut world.listeners,
                    listener_effects: &mut effects,
                    rng: Some(&mut world.rng),
                })
                .unwrap()
                .unwrap();
        }
        let startup = frontend_startup(
            &residency,
            crate::presentation::NativeFrame::original([640, 480]),
        )
        .unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let tables = PrimitiveTables::from_executable(executable).unwrap();
        let renderer_residency = if matches!(arm, Arm::MaterialAfterFactory) {
            &inactive
        } else {
            &residency
        };
        let resources = RenderResources::new(renderer_residency);
        let mut pixels = SoftwareFrame::new(640, 480, [13, 27, 41, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        let retained_view = SceneView {
            eye: [91, 82, 73],
            basis: MenuBasis::INITIAL,
        };
        let mut host = ResidentSceneHost {
            resources,
            backend: &mut scene,
        };
        host.submit(SceneCommand::PublishView(retained_view))
            .unwrap();
        host.submit(SceneCommand::CoordinateShift(7)).unwrap();
        let retained_far_color = host.backend.primitive_state.far_color;
        let retained_projection = host.backend.environment.projection;
        world.camera_failure = Some("preceding pose failure".into());
        // Prime only the genuine resolver cache in this fixture; lookup may
        // otherwise change cache custody before its required factory read.
        if let Some(handle) = world.arena.camera_handle() {
            world.arena.resolve_actor_handle(handle).unwrap();
        }
        let before_arena = format!("{:?}", world.arena);
        let before_listeners = format!("{:?}", world.listeners);
        let before_rng = world.rng;
        let before_fade = world.fade.clone();
        let before_native_camera = world.native_camera;
        let initial_far = world.mission.dword_1c;
        world.mission.dword_1c = initial_far + 1024;
        let mut furniture = WorldFurnitureFixture::new(&assets, &world);
        let factory_residency = if matches!(
            arm,
            Arm::MissingOwner | Arm::MissingValue | Arm::MissingResidency
        ) {
            &inactive
        } else {
            &residency
        };
        let mut animation = AnimationClock::default();
        animation.advance(0x1234 * 20_000);
        let mut camera = camera_inputs.inputs(factory_residency, animation.value());
        if matches!(arm, Arm::ResolvedNoOwner | Arm::MissingOwner) {
            camera.process = None;
        }
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
        let result = draw_world_frame(
            WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
                weapon_tables: None,
                object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                    inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                ).unwrap(),
                attract_demo: &false,
                world: &mut world,
                animation: &animation,
                particles: &mut Default::default(),
                light_grid: &mut Default::default(),
                object_light_policy: &mut object_light_policy,
                object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
                executable,
                camera,
                furniture: furniture.inputs(),
                sound: None,
            },
            WorldFrameInputs {
                template_focal: startup.projection.focal,
                focal_src: startup.projection.focal,
                focal_kick: 1.0,
                scale_5c0: 0,
                special_view_active: false,
                surface: [640, 480],
                z_buffering: true,
                dynamic_lighting: false,
                arena_bytes: RETAIL_ARENA_BYTES,
                mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
                hovercraft_fans: false,
                guided_mounts: false,
                camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
                settings_7c: 0,
                settings_70: 0,
                network_initials: [[0; 6]; crate::network::game::SLOTS],
                network_team_flag: 0,
                infrared: false,
                game: crate::presentation::GameView::Retail,
                game_far: None,
            },
            &mut host,
        );
        assert_eq!(
            world.camera_failure.as_deref(),
            Some("preceding pose failure"),
            "{arm:?}"
        );
        match arm {
            Arm::ResolvedNoOwner | Arm::ResolvedNoValue => {
                result.unwrap();
                assert_eq!(world.native_camera, Some(original_camera));
                // Downstream model/terrain RNG is independent of acquisition.
                // Resolve cache may update, but camera/target/players stay actual.
                assert_eq!(
                    world
                        .arena
                        .camera_draw_request(original_camera)
                        .unwrap()
                        .eye,
                    host.backend.environment.view.eye
                );
            }
            Arm::MissingOwner | Arm::MissingValue => {
                assert!(
                    matches!(
                        result,
                        Err(WorldDrawError::CameraAcquisition(
                            PlayerAttachError::MissingCameraFactoryProcessNullHandle
                        ))
                    ),
                    "{arm:?}: {result:?}"
                );
                assert_eq!(world.fade, before_fade);
                assert_eq!(host.backend.environment.projection, retained_projection);
                assert_eq!(format!("{:?}", world.arena), before_arena);
                assert_eq!(format!("{:?}", world.listeners), before_listeners);
            }
            Arm::MissingResidency => {
                // Known current NULL passes the mandatory process read before H13.
                // The fixed camera request has no rejecting command suffix.
                assert_eq!(
                    camera_inputs.process.process_null_handle,
                    Some(ActorHandle::from_raw(0))
                );
                assert!(
                    matches!(
                        result,
                        Err(WorldDrawError::CameraAcquisition(
                            PlayerAttachError::Construction(ObjectConstructionError::NotResident {
                                resource_argument: 1
                            })
                        ))
                    ),
                    "{result:?}"
                );
                assert_eq!(world.native_camera, before_native_camera);
                assert_eq!(world.fade, before_fade);
                assert_eq!(host.backend.environment.projection, retained_projection);
                assert_eq!(host.backend.primitive_state.far_color, retained_far_color);
                assert_eq!(format!("{:?}", world.arena), before_arena);
            }
            Arm::MaterialAfterFactory => {
                assert!(
                    matches!(result, Err(WorldDrawError::Resource(_))),
                    "{result:?}"
                );
                let camera = world.native_camera.unwrap();
                assert_ne!(camera, original_camera);
                assert_eq!(
                    world.arena.camera_handle(),
                    Some(world.arena.actor(camera).unwrap().handle)
                );
                assert_eq!(host.backend.primitive_state.far_color, retained_far_color);
                assert_ne!(format!("{:?}", world.arena), before_arena);
            }
            Arm::UnknownPosition => {
                assert!(
                    matches!(
                        result,
                        Err(WorldDrawError::Camera(
                            crate::object_runtime::CameraPoseError::Object(
                                ObjectError::ActorPositionUnknown(_)
                            )
                        ))
                    ),
                    "{result:?}"
                );
                assert_eq!(
                    world
                        .arena
                        .object(world.native_camera.unwrap())
                        .unwrap()
                        .base
                        .position_words,
                    None
                );
            }
        }
        if !matches!(arm, Arm::ResolvedNoOwner | Arm::ResolvedNoValue) {
            assert_eq!(host.backend.environment.view, retained_view, "{arm:?}");
            assert_eq!(host.backend.environment.coordinate_shift, 7, "{arm:?}");
            assert_eq!(world.rng, before_rng);
            if !matches!(arm, Arm::MaterialAfterFactory) {
                assert_eq!(format!("{:?}", world.listeners), before_listeners);
            } else {
                assert!(world.listeners.is_empty());
                // Empty notification still owns its cursor allocation/free.
            }
        }
        if matches!(arm, Arm::MaterialAfterFactory | Arm::UnknownPosition) {
            assert_ne!(world.fade, before_fade, "{arm:?}");
            assert_eq!(
                host.backend.environment.projection.fade[1],
                initial_far as i32 + 64,
                "{arm:?}"
            );
            if !matches!(arm, Arm::MaterialAfterFactory) {
                let RenderResourceData::Material(material) = resources
                    .resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Material,
                        argument: 0x74,
                    })
                    .unwrap()
                    .data
                else {
                    unreachable!()
                };
                assert_eq!(
                    host.backend.primitive_state.far_color,
                    host.backend.color_layout().convert_material(material)
                );
            }
        }
    }
}

#[test]
fn world_object_light_deposits_feed_the_retained_terrain_grid() {
    let (residency, mut world, _maps, _map_load) = load_first_world();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let resources = RenderResources::new(&residency);
    let startup = frontend_startup(
        &residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let eye = [
        position[0],
        position[1].wrapping_add(1024),
        position[2].wrapping_sub(1024),
    ];
    supply_native_camera(&mut world, eye, MenuBasis::INITIAL);

    // This is a supplied complete TerrainLight constructor/context domain,
    // appended to a genuinely loaded arena. It does not alter class dispatch
    // or fabricate a successful callback for existing world actors.
    let light = world
        .arena
        .allocate(crate::object_runtime::ObjectClassKind::TerrainLight);
    world
        .arena
        .initialize_terrain_light_payload(
            light,
            3,
            crate::object_runtime::TerrainLight {
                channels: [96, 80, 64],
                radius: 2048,
                draw_gate: 0,
            },
        )
        .unwrap();
    world
        .arena
        .initialize_context(
            light,
            crate::object_runtime::BaseContext {
                position_words: Some(position),
                placement_index: u32::MAX,
            },
        )
        .unwrap();
    let light_before = world.arena.object(light).unwrap().clone();
    let mut frame = SoftwareFrame::new(640, 480, [16, 24, 64, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    scene.set_terrain(Some(world.terrain.clone()));
    let mut host = ResidentSceneHost {
        resources,
        backend: &mut scene,
    };
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    let mut particles = crate::process_particles::ProcessParticleRuntime::default();
    let mut lights = crate::world_lighting::WorldLightGrid::default();
    let mut object_light_policy =
        FixtureObjectLightPolicy::new(true, world.terrain.sampling().spacing);
    let outcome = draw_world_frame(
        WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
            weapon_tables: None,
            object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            ).unwrap(),
            attract_demo: &false,
            world: &mut world,
            animation: &AnimationClock::default(),
            particles: &mut particles,
            light_grid: &mut lights,
            object_light_policy: &mut object_light_policy,
            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            executable,
            camera: camera_inputs.inputs(&residency, 0),
            furniture: furniture.inputs(),
            sound: None,
        },
        WorldFrameInputs {
            template_focal: startup.projection.focal,
            focal_src: startup.projection.focal,
            focal_kick: 1.0,
            scale_5c0: 0,
            special_view_active: false,
            surface: [640, 480],
            z_buffering: true,
            dynamic_lighting: true,
            arena_bytes: RETAIL_ARENA_BYTES,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            guided_mounts: false,
            camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
            settings_7c: 0,
            settings_70: 0,
            network_initials: [[0; 6]; crate::network::game::SLOTS],
            network_team_flag: 0,
            infrared: false,
            game: crate::presentation::GameView::Retail,
            game_far: None,
        },
        &mut host,
    ).unwrap();
    assert!(outcome.object_light_callbacks.unwrap() >= 1, "{outcome:?}");
    assert!(outcome.object_light_deposits.unwrap() >= 1, "{outcome:?}");
    assert!(lights.cells().iter().any(|&cell| cell != 0));
    assert_eq!(world.arena.object(light).unwrap(), &light_before);
    assert!(!outcome.skipped_terrain, "{outcome:?}");
    assert!(outcome.terrain_commands > 0, "{outcome:?}");
    // The frame's terrain phase receives this retained grid after the reached
    // object pass, rather than a cloned or cleared replacement. Check its
    // actual borrowed cell owner as well as the completed terrain submission.
    let crate::terrain_draw::TerrainLighting::Dynamic { cells, anchors, .. } =
        crate::world_lighting::terrain_lighting_inputs(&lights, true, executable).unwrap()
    else {
        panic!("dynamic terrain light input")
    };
    assert_eq!(cells.as_ptr(), lights.cells().as_ptr());
    assert_eq!(cells.len(), lights.cells().len());
    assert_eq!(anchors, lights.anchors());
}

/// Infrared vision (the world manager's +5C3) in the world draw: no sky
/// (`00440DA8`), and before the grid's anchor moves the first player's unit
/// lights it green, radius 0xC18 (`00488072..004880C2`), scaled by the
/// infrared bytes (`0045B29E`). Nothing else deposits here, so the second
/// frame's grid is exactly that light under the first frame's anchor.
#[test]
fn infrared_vision_skips_the_sky_and_lights_the_vehicle_green() {
    let (residency, mut world, _maps, _map_load) = load_first_world();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let resources = RenderResources::new(&residency);
    let startup = frontend_startup(
        &residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let eye = [
        position[0],
        position[1].wrapping_add(1024),
        position[2].wrapping_sub(1024),
    ];
    supply_native_camera(&mut world, eye, MenuBasis::INITIAL);
    let mut frame = SoftwareFrame::new(640, 480, [16, 24, 64, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    scene.set_terrain(Some(world.terrain.clone()));
    let mut host = ResidentSceneHost {
        resources,
        backend: &mut scene,
    };
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    let mut particles = crate::process_particles::ProcessParticleRuntime::default();
    let mut lights = crate::world_lighting::WorldLightGrid::default();
    let spacing = world.terrain.sampling().spacing;
    let mut object_light_policy = FixtureObjectLightPolicy::new(false, spacing);
    let mut object_draw_process =
        crate::object_runtime::ObjectDrawProcessState::native_startup(executable).unwrap();
    let mut expected = None;
    for _ in 0..2 {
        let outcome = draw_world_frame(
            WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
                weapon_tables: None,
                object_draw_process: &mut object_draw_process,
                attract_demo: &false,
                world: &mut world,
                animation: &AnimationClock::default(),
                particles: &mut particles,
                light_grid: &mut lights,
                object_light_policy: &mut object_light_policy,
                object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
                executable,
                camera: camera_inputs.inputs(&residency, 0),
                furniture: furniture.inputs(),
                sound: None,
            },
            WorldFrameInputs {
                template_focal: startup.projection.focal,
                focal_src: startup.projection.focal,
                focal_kick: 1.0,
                scale_5c0: 0,
                special_view_active: false,
                surface: [640, 480],
                z_buffering: true,
                dynamic_lighting: true,
                arena_bytes: RETAIL_ARENA_BYTES,
                mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
                hovercraft_fans: false,
                guided_mounts: false,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                settings_7c: 0,
                settings_70: 0,
                network_initials: [[0; 6]; crate::network::game::SLOTS],
                network_team_flag: 0,
                infrared: true,
                game: crate::presentation::GameView::Retail,
                game_far: None,
            },
            &mut host,
        )
        .unwrap();
        assert!(outcome.skipped_sky, "{outcome:?}");
        // The first frame's anchor, cleared, takes the expected light.
        if expected.is_none() {
            let mut grid = lights.clone();
            grid.clear();
            grid.add(
                crate::world_lighting::TerrainLightRequest {
                    world_xz: [position[0], position[2]].map(|word| i32::from(word as i16)),
                    radius: 0xc18,
                    rgb: [0, 127, 0],
                },
                crate::world_lighting::TerrainLightPolicy {
                    enabled: true,
                    spacing,
                    color: crate::world_lighting::TerrainLightColor::Infrared([0, 127, 0]),
                },
            )
            .unwrap();
            assert!(grid.cells().iter().any(|&cell| cell != 0));
            expected = Some(grid);
        }
    }
    let expected = expected.unwrap();
    assert_eq!(lights.anchors(), expected.anchors());
    assert!(
        lights.cells() == expected.cells(),
        "the player light's cells"
    );
}

#[test]
fn native_object_manager_hard_queue_stop_commits_rng_and_retains_list_prefix() {
    let (residency, mut world, _maps, _map_load) = load_first_world();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let resources = RenderResources::new(&residency);
    // Explicit supplied manager state: park the loaded domain through its real
    // setter, then construct two canonical visual-prefix objects. H8/161 and
    // terrain are original current resources; no OVL/placement bytes change.
    let parked: Vec<_> = world
        .arena
        .list(crate::object_runtime::ObjectList::Flag0Primary)
        .iter()
        .map(|node| node.object)
        .collect();
    for object in parked {
        world.arena.set_flag0(object, false).unwrap();
    }
    let sources: Vec<_> = (0..2)
        .map(|_| {
            let object = world
                .arena
                .allocate(crate::object_runtime::ObjectClassKind::VisualBase);
            world
                .arena
                .initialize_visual_context(
                    object,
                    crate::object_runtime::BaseContext {
                        position_words: Some([0, 0, 30_000]),
                        placement_index: u32::MAX,
                    },
                    Some(161),
                    &residency,
                )
                .unwrap();
            object
        })
        .collect();
    let mut config = frontend_startup(
        &residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap()
    .scene;
    let eye = [0, 0, 60_000];
    config.environment.view = SceneView {
        eye,
        basis: camera_basis(eye, [0, 0, 30_000]),
    };
    config.environment.coordinate_shift = 0;
    config.environment.projection.fade = [1_000_000, 2_000_000];
    let mut frame = SoftwareFrame::new(640, 480, [16, 24, 64, 255]);
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            scratch.clone(),
            (config).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config,
    })
    .unwrap();
    let mut process =
        crate::object_runtime::ObjectDrawProcessState::native_startup(executable).unwrap();
    // Supplied state after the native update-byte writer; production does not
    // claim the complete478F20 update prefix merely by resetting this byte.
    process.completion_53b890 = 0;
    let mut context = crate::object_runtime::ObjectDrawContext {
        view: config.environment.view,
        near: config.environment.projection.fade[0],
        far: config.environment.projection.fade[1],
        far_color: config.far_color,
    };
    let grid = crate::world_lighting::WorldLightGrid::default();
    let clock = AnimationClock::default();
    let seed = 0x1020_3040;
    world.rng = seed;
    let mut expected = seed;
    for completed_prefix in 1..=2 {
        scene.begin_frame().unwrap();
        scene.replace_command_arena(16).unwrap();
        if completed_prefix == 2 {
            // Supplied inter-attempt fixture: queue reset does not recover the
            // actual stopped process pool. Invoke the named approximation
            // explicitly before this independent second queue-capacity attempt.
            // No production/native hard-error unwind acceptance is claimed.
            let crate::model_scratch::ModelScratchDrawState::Stopped(scope) =
                scratch.state().unwrap()
            else {
                panic!("the first hard queue error must retain stopped scratch");
            };
            assert!(!scene.queue.state().stopped);
            let cursor_before = scratch.cursor().unwrap();
            assert!(cursor_before > 0);
            let bytes_before: Vec<_> = (0..cursor_before)
                .map(|offset| scratch.read_pool::<1>(offset).unwrap()[0])
                .collect();
            scratch
                .recover_stopped_draw(
                    &scope,
                    crate::model_scratch::RegisteredMarkUnwindApproximation,
                )
                .unwrap();
            assert_eq!(
                scratch.state().unwrap(),
                crate::model_scratch::ModelScratchDrawState::Ready
            );
            assert_eq!(scratch.cursor().unwrap(), 0);
            let bytes_after: Vec<_> = (0..cursor_before)
                .map(|offset| scratch.read_pool::<1>(offset).unwrap()[0])
                .collect();
            assert_eq!(bytes_after, bytes_before);
        }
        for _ in 0..3 {
            crate::name_entry::name_entry_rng_next(&mut expected);
        }
        world.presentation.tint_word = 0x00ff_ffff;
        world.presentation.terrain_light_direction = [0.0, 1.0, 0.0];
        assert_eq!(
            world.terrain.sampling().spacing,
            crate::terrain_runtime::TerrainSpacing::Units256
        );
        let result = submit_world_object_models(
            &mut world,
            &mut scene,
            &mut WorldObjectDrawRequest {
                weapon_tables: None,
                pickup_text_data: None,
                particles: &mut Default::default(),
                sounds: None,
                birth_inputs: crate::process_particles::births::ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
                },
                context: &mut context,
                process: &mut process,
                residency: &residency,
                light_grid: &grid,
                executable,
                network_flag: &0,
                animation: &clock,
                attract_demo: &false,
                camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
                settings_7c: 0,
                settings_70: 0,
                network_initials: &[[0; 6]; crate::network::game::SLOTS],
                network_team_flag: 0,
                map: None,
            },
            ObjectPass::Manager(crate::object_runtime::ObjectDrawCone::Retail),
        );
        assert!(
            matches!(
                result,
                Err(crate::object_runtime::Flag0ObjectDrawError::Host(_))
            ),
            "{result:?}"
        );
        assert!(scene.queue.state().stopped);
        assert_eq!(
            world.rng, expected,
            "reached manager prefix{completed_prefix}"
        );
        assert_eq!(scene.process_rng(), Some(expected));
        assert_eq!(scene.stats.models, 0);
        assert_eq!(
            process.completion_53b890, 0,
            "hard return does not complete manager"
        );
        let ids = |role| {
            world
                .arena
                .list(role)
                .iter()
                .map(|node| node.object)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(crate::object_runtime::ObjectList::Flag0Primary),
            sources[completed_prefix..]
        );
        assert_eq!(
            ids(crate::object_runtime::ObjectList::Flag0Secondary),
            sources[..completed_prefix]
        );
        assert_ne!(
            world
                .arena
                .object(sources[completed_prefix - 1])
                .unwrap()
                .base
                .flags
                & 0x0200_0000,
            0
        );
    }
}

#[path = "world_scene_tests/eye.rs"]
mod eye;

/// A vehicle's two lamps stand at the tuning's place and its mirror across
/// the nose, eye-relative, aimed down the nose turned down by the pitch;
/// the nearest vehicles come first.
#[test]
fn lamps_stand_on_the_nose_and_aim_down_it() {
    let look = crate::modern_tuning::ModernTuning::defaults()
        .headlight()
        .clone();
    let identity = inf_formats::native_basis::NativeBasis::INITIAL.words;
    let near = crate::object_runtime::VehicleLamp {
        hull: [1100, 50, 2000],
        basis: identity,
        head: true,
        rear: None,
    };
    let far = crate::object_runtime::VehicleLamp {
        hull: [9000, 50, 2000],
        ..near
    };
    let lamps = super::place_lamps(&[far, near], [1000, 0, 2000], &look);
    assert_eq!(lamps.len(), 4);
    let [x, y, z] = look.position;
    assert_eq!(lamps[0].position, [100.0 + x, 50.0 + y, z]);
    assert_eq!(lamps[1].position, [100.0 - x, 50.0 + y, z]);
    let pitch = look.pitch.to_radians();
    let axis = lamps[0].axis;
    assert!((axis[1] + pitch.sin()).abs() < 1.0e-5 && (axis[2] - pitch.cos()).abs() < 1.0e-5);
    assert_eq!((lamps[0].color, lamps[0].rear), (look.color, false));
    assert_eq!(lamps[2].position[0], 8000.0 + x);
}

/// The rear lamps lit stand at their billboards' pair, 2 behind them, aimed
/// straight back turned down by their pitch, in their pair's colour; the
/// headlamps come first.
#[test]
fn rear_lamps_stand_on_the_back_and_aim_back() {
    let look = crate::modern_tuning::ModernTuning::defaults()
        .headlight()
        .clone();
    let vehicle = crate::object_runtime::VehicleLamp {
        hull: [1100, 50, 2000],
        basis: inf_formats::native_basis::NativeBasis::INITIAL.words,
        head: true,
        rear: Some(crate::object_runtime::RearLamps::Red),
    };
    let lamps = super::place_lamps(&[vehicle], [1000, 0, 2000], &look);
    assert_eq!(
        lamps.iter().map(|lamp| lamp.rear).collect::<Vec<_>>(),
        [false, false, true, true]
    );
    assert_eq!(lamps[2].position, [117.0, 44.0, -83.0]);
    assert_eq!(lamps[3].position, [83.0, 44.0, -83.0]);
    assert_eq!(lamps[2].color, look.rear.red);
    let pitch = look.rear.pitch.to_radians();
    let axis = lamps[2].axis;
    assert!((axis[1] + pitch.sin()).abs() < 1.0e-5 && (axis[2] + pitch.cos()).abs() < 1.0e-5);
    let reversing = crate::object_runtime::VehicleLamp {
        head: false,
        rear: Some(crate::object_runtime::RearLamps::White),
        ..vehicle
    };
    let lamps = super::place_lamps(&[reversing], [1000, 0, 2000], &look);
    assert_eq!(lamps.len(), 2);
    assert_eq!(lamps[0].position, [110.0, 44.0, -83.0]);
    assert_eq!(lamps[0].color, look.rear.white);
}

/// A lamp facing the eye glares; one facing away does not, nor any when
/// the tuning turns the glare off.
#[test]
fn a_lamp_glares_only_toward_the_eye() {
    let look = crate::modern_tuning::ModernTuning::defaults()
        .headlight()
        .clone();
    let camera = crate::scene_feed::SceneCamera {
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        focal: [960.0, 960.0],
        centre: [960.0, 540.0],
        native: [1920, 1080],
        near: 16.0,
        far: 30000.0,
        ..Default::default()
    };
    let facing = crate::scene_feed::SceneLamp {
        position: [0.0, 0.0, 500.0],
        axis: [0.0, 0.0, -1.0],
        across: [1.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        color: look.color,
        rear: false,
    };
    let away = crate::scene_feed::SceneLamp {
        axis: [0.0, 0.0, 1.0],
        ..facing
    };
    let rear = crate::scene_feed::SceneLamp {
        rear: true,
        ..facing
    };
    assert!(!super::lamp_glare(&camera, &[facing], &look).is_empty());
    assert!(super::lamp_glare(&camera, &[away], &look).is_empty());
    // A rear lamp shows retail's own billboard.
    assert!(super::lamp_glare(&camera, &[rear], &look).is_empty());
    let dark = crate::modern_tuning::HeadlightTuning {
        glare: 0.0,
        ..look.clone()
    };
    assert!(super::lamp_glare(&camera, &[facing], &dark).is_empty());
}

/// The CPU's beam, which the raster's crystals take, is the shader's: on
/// its axis it gives the light 1,000 units out, nothing behind the lamp or
/// above its top edge.
#[test]
fn the_cpu_beam_lights_along_its_axis_only() {
    let tuning = crate::modern_tuning::ModernTuning::defaults();
    let look = tuning.headlight().look(false);
    let lamp = crate::scene_feed::SceneLamp {
        position: [0.0; 3],
        axis: [0.0, 0.0, 1.0],
        across: [1.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        ..Default::default()
    };
    let ahead = super::lamp_beam(&lamp, &look, [0.0, 0.0, 1000.0]);
    let falloff = 1.0e6 / (1.0e6 + look.near * look.near);
    let window = 1.0 - (1000.0 / look.reach).powi(4);
    assert!((ahead - look.intensity * falloff * window * window).abs() < 1.0e-3);
    assert_eq!(super::lamp_beam(&lamp, &look, [0.0, 0.0, -1000.0]), 0.0);
    // 30 degrees up: past the top edge and its soft fade (8 and 12).
    assert_eq!(super::lamp_beam(&lamp, &look, [0.0, 577.0, 1000.0]), 0.0);
    // 14 degrees up a wall still takes some (2026-10-24: real
    // light goes up the wall a bit).
    assert!(super::lamp_beam(&lamp, &look, [0.0, 250.0, 1000.0]) > 0.0);
    // A headlamp's spill still lights 60 degrees aside (2026-10-29:
    // the area was a little too small).
    assert!(super::lamp_beam(&lamp, &look, [520.0, 0.0, 300.0]) > 0.0);
    // A rear lamp's beam is short and wide: nothing at 1,000, light 67
    // degrees aside, past where a headlamp's ends.
    let rear = tuning.headlight().look(true);
    assert_eq!(super::lamp_beam(&lamp, &rear, [0.0, 0.0, 1000.0]), 0.0);
    assert!(super::lamp_beam(&lamp, &rear, [700.0, 0.0, 300.0]) > 0.0);
    assert_eq!(super::lamp_beam(&lamp, &look, [700.0, 0.0, 300.0]), 0.0);
}

/// Artoria's second fog rolls in over its seconds once mission `+13` is set
/// (its generators down), from the timer `+2C`, and not before.
#[test]
fn a_darkened_world_rolls_its_fog_in() {
    let mut mission = crate::world_mission::WorldMission::native_constructor();
    mission.byte_13 = Some(0);
    mission.dword_2c = Some(0);
    assert_eq!(super::darkened(&mission), 0.0);
    mission.byte_13 = Some(1);
    mission.dword_2c = Some(3000);
    assert!((super::darkened(&mission) - 3.0 / super::FOG_ROLLS_IN).abs() < 1.0e-6);
    mission.dword_2c = Some(60_000);
    assert_eq!(super::darkened(&mission), 1.0);
    let tuning = crate::modern_tuning::ModernTuning::defaults();
    let lit = tuning.fog_at(11, 0.0);
    let dark = tuning.fog_at(11, 1.0);
    assert_eq!(lit, tuning.fog(11));
    assert!(dark.start < lit.start && dark.density > lit.density);
    let half = tuning.fog_at(11, 0.5);
    assert!((half.start - 0.5 * (lit.start + dark.start)).abs() < 1.0e-5);
    // A world without a dark profile keeps its fog.
    assert_eq!(tuning.fog_at(2, 1.0), tuning.fog(2));
}

/// What falls comes only under the modern sky, except Pelagia's rain,
/// which waits for the night over the painted sky too, and only with the
/// rework; every world with a drawn drift has a wind to carry it.
#[test]
fn the_fall_follows_the_sky_and_the_night() {
    let tuning = crate::modern_tuning::ModernTuning::defaults();
    let pelagia = tuning.sky(crate::pelagia::WORLD).precip;
    assert!(pelagia.with_night);
    assert_eq!(super::precip_share(&pelagia, false, 0.0, true), 0.0);
    assert_eq!(super::precip_share(&pelagia, false, 0.6, true), 0.6);
    assert_eq!(super::precip_share(&pelagia, false, 1.0, false), 0.0);
    let mut plain = pelagia;
    plain.with_night = false;
    assert_eq!(super::precip_share(&plain, false, 1.0, true), 0.0);
    assert_eq!(super::precip_share(&plain, true, 0.0, true), 1.0);
    let weather = tuning.atmosphere().weather.as_ref().unwrap();
    for world in weather.drift.keys() {
        assert!(weather.winds.contains_key(world), "world {world}");
    }
}

/// A wheeled craft stirs puddles from each wheel that touches the ground,
/// at the ground under its hub; a wheel in the air, or another object's,
/// stirs nothing. A craft that drew no wheels stirs from its middle.
#[test]
fn a_wheeled_craft_stirs_from_its_grounded_wheels() {
    let (_residency, _assets, world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units128);
    let nodes = world
        .arena
        .list(crate::object_runtime::ObjectList::Flag0Primary);
    let (craft, other) = (nodes[0].object, nodes[1].object);
    let eye = [37_000, 0, 34_000];
    let ground = |dx: f32, dz: f32| {
        f32::from(world.terrain.height_at([
            (eye[0] as f32 + dx) as i32 as u16,
            (eye[2] as f32 + dz) as i32 as u16,
        ]))
    };
    let mut feed = crate::scene_feed::SceneFeed::default();
    for id in [craft, other] {
        feed.objects.push(crate::scene_feed::SceneObject {
            id,
            position: [0; 3],
            root: None,
        });
    }
    let wheel = |object: u32, dx: f32, dz: f32, lift: f32| crate::scene_feed::SceneWheel {
        object,
        hub: [dx, ground(dx, dz) + 32.0 + lift, dz],
        radius: 32.0,
    };
    feed.wheels = vec![
        wheel(0, -60.0, 80.0, 0.0),
        wheel(0, 60.0, 80.0, 10.0),
        wheel(0, -60.0, -80.0, 120.0),
        wheel(1, 500.0, 500.0, 0.0),
    ];
    let wake = Some(super::PlayerWake {
        unit: craft,
        centre: [0.0, 50.0, 0.0],
        strength: 0.7,
    });
    let wakes = super::craft_wakes(&feed, wake, &world.terrain, eye);
    assert_eq!(wakes.len(), 2, "{wakes:?}");
    assert_eq!(wakes[0].position, [-60.0, ground(-60.0, 80.0), 80.0]);
    assert_eq!(wakes[1].position, [60.0, ground(60.0, 80.0), 80.0]);
    assert!(wakes.iter().all(|wake| wake.strength == 0.7));
    feed.wheels.clear();
    let wakes = super::craft_wakes(&feed, wake, &world.terrain, eye);
    assert_eq!(
        wakes,
        [crate::scene_feed::SceneWake {
            position: [0.0, 50.0, 0.0],
            strength: 0.7
        }]
    );
    assert!(super::craft_wakes(&feed, None, &world.terrain, eye).is_empty());
}

/// Through the water, a wheel across its surface and the hull in or just
/// over it stir it from the plane, their strength negated (the puddles
/// skip them); a wheel standing on dry ground still stirs a puddle.
#[test]
fn a_craft_stirs_the_water_from_its_wheels_and_hull() {
    let (_residency, _assets, world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units128);
    let terrain = &world.terrain;
    let plane = terrain.plane_base() >> terrain.plane_shift();
    assert!(plane > -4096, "world 18 has water");
    let below = |x: u16, z: u16| i32::from(terrain.height_at([x, z])) < plane - 64;
    let [x, z] = (0..512u32)
        .flat_map(|i| (0..512u32).map(move |j| [(i * 128) as u16, (j * 128) as u16]))
        .find(|&[x, z]| below(x, z) && below(x.wrapping_add(200), z))
        .expect("deep water on world 18");
    let dry = (0..512u32)
        .flat_map(|i| (0..512u32).map(move |j| [(i * 128) as u16, (j * 128) as u16]))
        .find(|&[x, z]| i32::from(terrain.height_at([x, z])) > plane + 64)
        .expect("dry ground on world 18");
    let eye = [i32::from(x), 0, i32::from(z)];
    let plane = plane as f32;
    let craft = world
        .arena
        .list(crate::object_runtime::ObjectList::Flag0Primary)[0]
        .object;
    let mut feed = crate::scene_feed::SceneFeed::default();
    feed.objects.push(crate::scene_feed::SceneObject {
        id: craft,
        position: [0; 3],
        root: None,
    });
    let dry_offset = [
        f32::from(dry[0].wrapping_sub(x) as i16),
        f32::from(dry[1].wrapping_sub(z) as i16),
    ];
    let dry_ground = f32::from(terrain.height_at(dry));
    feed.wheels = vec![
        // Across the surface, and wholly over it.
        crate::scene_feed::SceneWheel {
            object: 0,
            hub: [0.0, plane + 10.0, 0.0],
            radius: 32.0,
        },
        crate::scene_feed::SceneWheel {
            object: 0,
            hub: [200.0, plane + 80.0, 0.0],
            radius: 32.0,
        },
        // Standing on dry ground.
        crate::scene_feed::SceneWheel {
            object: 0,
            hub: [dry_offset[0], dry_ground + 32.0, dry_offset[1]],
            radius: 32.0,
        },
    ];
    let wake = |height: f32| {
        Some(super::PlayerWake {
            unit: craft,
            centre: [0.0, plane + height, 0.0],
            strength: 0.5,
        })
    };
    let wakes = super::craft_wakes(&feed, wake(40.0), terrain, eye);
    let water = |at: [f32; 2]| crate::scene_feed::SceneWake {
        position: [at[0], plane, at[1]],
        strength: -0.5,
    };
    let puddle = crate::scene_feed::SceneWake {
        position: [dry_offset[0], dry_ground, dry_offset[1]],
        strength: 0.5,
    };
    assert_eq!(wakes, [water([0.0, 0.0]), puddle, water([0.0, 0.0])]);
    // The hull high over the water stirs nothing there.
    let wakes = super::craft_wakes(&feed, wake(400.0), terrain, eye);
    assert_eq!(wakes, [water([0.0, 0.0]), puddle]);
}
