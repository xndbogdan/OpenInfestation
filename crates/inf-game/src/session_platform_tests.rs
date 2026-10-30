//! Real-screen 2->9->2 through the production session host and the actual
//! SessionController. Factory effects and music/baseline services are
//! recorded; the screens, registry, residency, identity read and offline
//! preparation are the production owners.

use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::FrontendDevice;
use crate::input::{DeviceButtons, InputRuntime};
use crate::name_entry::LoadedIdentity;
use crate::session::{SessionController, SessionPhase};
use crate::text_entry_screen::text_entry_reuses_phase;
use crate::title_screen::{TitleHost, TitleVideoHost};
use crate::{Installation, ResidencyRequest, ResidentRuntime};
use std::path::Path;

#[path = "session_platform/particles/damage/tests.rs"]
mod projectile_damage;

#[path = "session_platform/cheats_tests.rs"]
mod cheats;

#[path = "session_platform/network_menus_tests.rs"]
mod network_menus;

#[path = "session_platform/particles/radius/tests.rs"]
mod projectile_radius;

#[path = "session_platform/particles/combat/tests.rs"]
mod projectile_retirement;

#[path = "session_platform/furniture/tests.rs"]
mod furniture_radius;

#[path = "session_platform/world_actor_factory/child/tests.rs"]
mod selector36_child;

#[path = "session_platform/world_actor_factory/child/behavior_resource_tests.rs"]
mod behavior_resource_cache;

#[path = "session_platform/world_contact_tests.rs"]
mod world_contact_pairs;

#[path = "session_platform/world_actor_update/selector60/tests.rs"]
mod selector60_update;

#[test]
fn map_publication_and_grid_survive_display_replacement_and_retire_locally() {
    let (mut session, _) = fixture();
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    let mut state = SessionState::default();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let first = session.world().unwrap().map;
    assert_eq!(session.map_load.current(), Some(first));
    assert_eq!(session.map_load.pending(), None);
    assert_eq!(session.maps.jitter(first).unwrap().jitter_radius(), 0);
    session
        .maps
        .jitter_mut(first)
        .unwrap()
        .apply(crate::world_map::WorldMapJitterWrite::MissionEvent48D980)
        .unwrap();
    let grid = session.maps.grid().unwrap().indices().to_vec();
    let plane = session.maps.plane_byte();
    let process = std::ptr::addr_of!(session.maps);

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut replacement = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    replacement.apply(ResidencyRequest::Boot).unwrap();
    let context = FrontendRenderContext {
        projection: crate::frontend_scene::SceneProjection {
            focal: [400, 300],
            viewport: [800, 600],
            fade: [0, 0],
        },
    };
    let _previous = session.install_display(replacement, context).unwrap();
    assert_eq!(std::ptr::addr_of!(session.maps), process);
    assert_eq!(session.map_load.current(), Some(first));
    assert_eq!(session.maps.grid().unwrap().indices(), grid);
    assert_eq!(session.maps.plane_byte(), plane);
    assert_eq!(session.maps.jitter(first).unwrap().jitter_radius(), 0x1fff);

    // Native48DC30 has no implicit48E890 call. A direct repeated load retains
    // the old allocation and global membership instead of losing its backing.
    session
        .world
        .as_mut()
        .unwrap()
        .mission
        .save
        .mission_selection = 18;
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let second = session.world().unwrap().map;
    assert_ne!(first, second);
    assert_eq!(session.map_load.current(), Some(second));
    assert_eq!(session.maps.jitter(second).unwrap().jitter_radius(), 0);
    assert_eq!(session.maps.jitter(first).unwrap().jitter_radius(), 0x1fff);
    let grid = session.maps.grid().unwrap().indices().to_vec();
    let plane = session.maps.plane_byte();
    session
        .transition_effect(TransitionEffect::LeaveWorld, &mut state)
        .unwrap();
    assert!(session.world().is_none());
    assert_eq!(session.map_load.current(), None);
    assert!(session.maps.jitter(second).is_err());
    assert_eq!(session.maps.jitter(first).unwrap().jitter_radius(), 0x1fff);
    assert_eq!(session.maps.grid().unwrap().indices(), grid);
    assert_eq!(session.maps.plane_byte(), plane);
}

#[test]
fn failed_retained_load_keeps_actual_owners_and_prevents_stale_scene_or_replay() {
    use crate::object_runtime::{
        ActorVehicleCleanupNotificationError, PlayerActorLookupError, WorldCleanupError,
    };
    use crate::world_backend::{WorldLoadPhase, WorldLoadStatus};
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let (old, first, message, carriers, input_before) = {
        let world = session.world().unwrap();
        let first = world.arena.first_player().unwrap();
        (
            world.map,
            first,
            world.arena.player_message(first).unwrap().identity(),
            format!("{:?}", world.constructions),
            (
                session.input.clock_micros(),
                session.input.queued_len(),
                session.input.selected_context(),
            ),
        )
    };
    let before = 0x1234_5678;
    session.world_mut().unwrap().rng = before;
    session.world_mut().unwrap().mission.save.mission_selection = 18;
    let null = session.factory_process.process_null_handle.take();
    // Native map publication precedes cleanup of this same old manager. The
    // old selector2 reaches its genuine event10 NULL read before placements.
    let result = session.transition_effect(TransitionEffect::LoadWorld, &mut state);
    assert!(
        matches!(
            result,
            Err(PortSessionError::World(WorldBackendError::Loader(
                crate::world_loader::WorldLoaderError::Cleanup(WorldCleanupError::Vehicle(
                    ActorVehicleCleanupNotificationError::PlayerLookup(
                        PlayerActorLookupError::MissingProcessNullHandle
                    )
                ))
            )))
        ),
        "{result:?}"
    );
    let reached = session.map_load.current().unwrap();
    assert_ne!(reached, old);
    assert!(
        session.world().is_none(),
        "old presentation cannot reference a mutated retained manager"
    );
    let owner = session.pending_world.as_ref().unwrap();
    assert_eq!(owner.arena.first_player(), Some(first));
    assert_eq!(
        owner.arena.player_message(first).unwrap().identity(),
        message
    );
    assert_eq!(format!("{:?}", owner.constructions), carriers);
    assert_eq!(owner.mission.current_world_a0, 18);
    assert_eq!(owner.mission.inhibit_byte_08, 1);
    assert!(owner.prepared.is_some());
    assert!(matches!(
        owner.load_status,
        WorldLoadStatus::Stopped {
            phase: WorldLoadPhase::WorldObjects,
            ..
        }
    ));
    assert_eq!(
        (
            session.input.clock_micros(),
            session.input.queued_len(),
            session.input.selected_context()
        ),
        input_before
    );
    assert_eq!(session.world_rng_custody, WorldRngCustody::Platform);
    let after = session.platform.services.rng;
    let mut expected = before;
    for _ in 0..65_536 {
        expected = expected.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
    }
    assert_eq!(after, expected, "reached map draws survive cleanup error");
    assert_eq!(session.map_load.pending(), None);
    assert!(session.maps.jitter(reached).is_ok());
    let arena_after = format!("{:?}", owner.arena);
    session.factory_process.process_null_handle = null;
    let retry = session.transition_effect(TransitionEffect::LoadWorld, &mut state);
    assert!(matches!(
        retry,
        Err(PortSessionError::World(
            WorldBackendError::LoadAlreadyAttempted(_)
        ))
    ));
    assert_eq!(
        format!("{:?}", session.pending_world.as_ref().unwrap().arena),
        arena_after
    );
    assert_eq!(session.platform.services.rng, after);
    assert_eq!(session.map_load.current(), Some(reached));
    // Leave is a distinct caller: its map prefix happens before the same lazy
    // unavailable retirement service, and the retained actual players stay.
    session.factory_process.process_null_handle = None;
    assert!(matches!(
        session.transition_effect(TransitionEffect::LeaveWorld, &mut state),
        Err(PortSessionError::WorldCleanup(_))
    ));
    assert_eq!(session.platform.services.rng, after);
    assert_eq!(session.map_load.current(), None);
    assert!(session.maps.jitter(reached).is_err());
    assert!(session.maps.jitter(old).is_ok());
    assert_eq!(
        session.pending_world.as_ref().unwrap().arena.first_player(),
        Some(first)
    );
    assert_eq!(
        format!(
            "{:?}",
            session.pending_world.as_ref().unwrap().constructions
        ),
        carriers
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrontendEffect {
    Pause,
    Resume,
    Enumerate,
    Persist,
}

#[derive(Default, Debug)]
struct RecordingFrontend {
    effects: Vec<FrontendEffect>,
    devices_created: usize,
    fail_enumeration: bool,
}

impl FrontendFactoryHost for RecordingFrontend {
    type Error = &'static str;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.effects.push(FrontendEffect::Pause);
        Ok(())
    }
    fn resume_music(&mut self) -> Result<(), Self::Error> {
        self.effects.push(FrontendEffect::Resume);
        Ok(())
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.effects.push(FrontendEffect::Enumerate);
        if self.fail_enumeration {
            return Err("device enumeration unavailable");
        }
        let source = input
            .create_device(DeviceButtons::default())
            .map_err(|_| "device failed")?;
        self.devices_created += 1;
        Ok(vec![FrontendDevice {
            native_type: 0,
            source,
        }])
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.effects.push(FrontendEffect::Persist);
        Ok(())
    }
}

#[derive(Default, Debug)]
struct RecordingText {
    reads: Vec<SessionPhase>,
    devices_created: usize,
}

impl TextEntryFactoryHost for RecordingText {
    type Error = &'static str;
    fn read_identity(&mut self, phase: SessionPhase, _: u8) -> Result<LoadedIdentity, Self::Error> {
        self.reads.push(phase);
        Ok(LoadedIdentity {
            name: b"Pilot".to_vec(),
            initials: b"AB".to_vec(),
        })
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        let source = input
            .create_device(DeviceButtons::default())
            .map_err(|_| "device failed")?;
        self.devices_created += 1;
        Ok(vec![FrontendDevice {
            native_type: 0,
            source,
        }])
    }
}

#[derive(Default, Debug)]
struct RecordingServices {
    terrain_detail: u32,
    terrain_detail_reads: usize,
    terrain_detail_missing: bool,
    rng: u32,
    loading_mode: std::cell::Cell<u8>,
    music_selections: usize,
    music_services: usize,
    baseline_resets: usize,
    exact_ticks: bool,
    /// Settings +84, the cheat word.
    cheats_84: u32,
    /// The port's Pelagia option.
    prototype_world: bool,
}

impl PortSessionServices for RecordingServices {
    type Error = &'static str;
    type IdentityError = std::io::Error;
    fn persist_identity(&mut self, _: IdentityWriteRequest<'_>) -> Result<(), Self::IdentityError> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "recording session has no identity persistence owner",
        ))
    }
    fn profile_store(&mut self) -> Option<&mut ProfileStore> {
        None
    }
    fn sound_voice_teardown(&mut self) -> crate::process_sound::ProcessSoundTeardownLoan<'_> {
        crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach
    }
    fn world_tick_inputs(&self) -> WorldTickPlatformInputs {
        WorldTickPlatformInputs {
            camera_settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            settings_20: 0,
            settings_70: self.terrain_detail,
            settings_78: 0,
            settings_7c: 0,
            input_mode_08: 0,
            settings_84: self.cheats_84,
            vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
        }
    }
    fn loading_platform(&mut self) -> crate::loading_integration::LoadingPlatformLoans<'_> {
        crate::loading_integration::LoadingPlatformLoans {
            voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
            mode: &self.loading_mode,
            rng: &mut self.rng,
        }
    }
    fn process_rng(&mut self) -> &mut u32 {
        &mut self.rng
    }
    fn world_tick_quantum(&self) -> WorldTickQuantum {
        if self.exact_ticks {
            WorldTickQuantum::Exact { micros: 16384 }
        } else {
            WorldTickQuantum::Retail
        }
    }
    fn session_music(&mut self, request: SessionMusicRequest) -> Result<(), Self::Error> {
        if request == (SessionMusicRequest::Select { native_argument: 7 }) {
            self.music_selections += 1;
        }
        if request == SessionMusicRequest::Service {
            self.music_services += 1;
        }
        Ok(())
    }
    fn reset_input_baselines(&mut self, input: &mut InputRuntime) -> Result<(), Self::Error> {
        input.reset_baselines_and_discard();
        self.baseline_resets += 1;
        Ok(())
    }
    fn startup_settings(&self) -> crate::session_platform::StartupSettings {
        crate::session_platform::StartupSettings {
            language: 0,
            loaded: true,
        }
    }
    fn key_bindings(&self) -> crate::key_bindings::KeyBindings {
        crate::key_bindings::KeyBindings::retail_initial()
    }
}

/// One platform value, as production has, recording each trait separately.
#[derive(Default, Debug)]
struct Recording {
    frontend: RecordingFrontend,
    text: RecordingText,
    services: RecordingServices,
    video: RecordingVideo,
}

impl crate::gameplay_screen::GameplayTerrainSettings for Recording {
    fn terrain_detail_setting(
        &mut self,
    ) -> Result<u32, crate::terrain_runtime::TerrainPolicyError> {
        self.services.terrain_detail_reads += 1;
        if self.services.terrain_detail_missing {
            Err(crate::terrain_runtime::TerrainPolicyError::MissingDetailSetting)
        } else {
            Ok(self.services.terrain_detail)
        }
    }
}

impl FrontendFactoryHost for Recording {
    type Error = &'static str;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.frontend.pause_music()
    }
    fn resume_music(&mut self) -> Result<(), Self::Error> {
        self.frontend.resume_music()
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        FrontendFactoryHost::enumerate_calibrated_devices(&mut self.frontend, input)
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.frontend.persist_settings()
    }
}

impl TextEntryFactoryHost for Recording {
    type Error = &'static str;
    fn read_identity(
        &mut self,
        phase: SessionPhase,
        slot: u8,
    ) -> Result<LoadedIdentity, Self::Error> {
        self.text.read_identity(phase, slot)
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        TextEntryFactoryHost::enumerate_calibrated_devices(&mut self.text, input)
    }
}

impl PortSessionServices for Recording {
    type Error = &'static str;
    type IdentityError = <RecordingServices as PortSessionServices>::IdentityError;
    fn persist_identity(
        &mut self,
        request: IdentityWriteRequest<'_>,
    ) -> Result<(), Self::IdentityError> {
        PortSessionServices::persist_identity(&mut self.services, request)
    }
    fn profile_store(&mut self) -> Option<&mut ProfileStore> {
        self.services.profile_store()
    }
    fn sound_voice_teardown(&mut self) -> crate::process_sound::ProcessSoundTeardownLoan<'_> {
        crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach
    }
    fn world_tick_inputs(&self) -> WorldTickPlatformInputs {
        self.services.world_tick_inputs()
    }
    fn loading_platform(&mut self) -> crate::loading_integration::LoadingPlatformLoans<'_> {
        self.services.loading_platform()
    }
    fn process_rng(&mut self) -> &mut u32 {
        self.services.process_rng()
    }
    fn world_tick_quantum(&self) -> WorldTickQuantum {
        self.services.world_tick_quantum()
    }
    fn prototype_world(&self) -> bool {
        self.services.prototype_world
    }
    fn session_music(&mut self, request: SessionMusicRequest) -> Result<(), Self::Error> {
        self.services.session_music(request)
    }
    fn reset_input_baselines(&mut self, input: &mut InputRuntime) -> Result<(), Self::Error> {
        self.services.reset_input_baselines(input)
    }
    fn startup_settings(&self) -> crate::session_platform::StartupSettings {
        crate::session_platform::StartupSettings {
            language: 0,
            loaded: true,
        }
    }
    fn key_bindings(&self) -> crate::key_bindings::KeyBindings {
        crate::key_bindings::KeyBindings::retail_initial()
    }
}

#[derive(Default, Debug)]
struct RecordingVideo {
    opens: usize,
    closes: usize,
    restores: usize,
}

impl TitleVideoHost for RecordingVideo {
    type Error = &'static str;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        self.opens += 1;
        Ok(None)
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        Err("title video presentation is absent")
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        self.closes += 1;
        Ok(())
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        self.restores += 1;
        Ok(())
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl TitleVideoHost for Recording {
    type Error = &'static str;
    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        self.video.open_title()
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        self.video.playback_status()
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        self.video.present_frame()
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        self.video.close_title()
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        self.video.restore_display()
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        self.video.draw_preamble()
    }
}

impl TitleHost for Recording {
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        TextEntryFactoryHost::enumerate_calibrated_devices(&mut self.text, input)
    }
}

fn fixture() -> (PortSession<Recording>, SessionController) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infest.exe is required");
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    let session = PortSession::new(
        assets,
        residency,
        Recording::default(),
        crate::frontend_renderer::FrontendRenderContext {
            projection: crate::frontend_scene::SceneProjection {
                focal: [320, 240],
                viewport: [640, 480],
                fade: [0, 0],
            },
        },
        None,
    )
    .expect("port session boot");
    (session, SessionController::default())
}

#[test]
fn live_factory_process_survives_screens_worlds_display_and_task_source_capture() {
    use crate::object_runtime::ActorHandle;
    use crate::process_particles::factories::ParticleFactoryProcessState;
    let (mut session, _) = fixture();
    assert_eq!(
        session.factory_process,
        ParticleFactoryProcessState::native_startup()
    );
    assert_eq!(session.process_up.map(f32::to_bits), [0, 0x3f80_0000, 0]);
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    // Supplied current state exercises lifetime custody, not a native writer.
    session.process_up = [0.25, -0.5, 0.75];
    let retained_up = session.process_up.map(f32::to_bits);
    let up_owner_address = std::ptr::addr_of!(session.process_up);
    let captured_tasks: Vec<_> = (0..crate::process_tasks::TASK_CLASS_COUNT as u32)
        .map(|class| {
            let owner = session.tasks.owner(class).unwrap().unwrap();
            (owner, session.tasks.record(owner).unwrap().source)
        })
        .collect();
    assert!(captured_tasks.iter().all(|(_, source)| source.raw() == 0));
    session.factory_process.network_flag = 0x100;
    session.factory_process.process_null_handle = Some(ActorHandle::from_raw(0x1234));
    let retained = session.factory_process;
    let owner_address = std::ptr::addr_of!(session.factory_process);
    assert!(SessionHost::network_session_active(&session));

    let mut state = SessionState::default();
    session.boot_frontend().unwrap();
    session
        .destroy_screen(PortScreen::Frontend, &mut state)
        .unwrap();
    session.boot_frontend().unwrap();
    // The loader's network branches need a network world: the campaign
    // world loads offline, and the flag returns for the reload's cleanup.
    session.factory_process.network_flag = 0;
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session.factory_process.network_flag = 0x100;
    session
        .world
        .as_mut()
        .unwrap()
        .mission
        .save
        .mission_selection = 18;
    // The reload's cleanup tells the network of each local actor's end
    // (0047EC50) and goes on, as does the leave's.
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session
        .transition_effect(TransitionEffect::LeaveWorld, &mut state)
        .unwrap();
    assert!(session.world.is_none());
    assert!(session.map_load.current().is_none());
    // The leave's 00493650 cleared the network flag.
    let retained = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        ..retained
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut replacement = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    replacement.apply(ResidencyRequest::Boot).unwrap();
    let generation = session.residency_generation();
    let _previous = session
        .install_display(
            replacement,
            crate::frontend_renderer::FrontendRenderContext {
                projection: crate::frontend_scene::SceneProjection {
                    focal: [400, 300],
                    viewport: [800, 600],
                    fade: [0, 0],
                },
            },
        )
        .unwrap();
    assert_eq!(session.residency_generation(), generation + 1);
    assert_eq!(std::ptr::addr_of!(session.factory_process), owner_address);
    assert_eq!(session.factory_process, retained);
    assert_eq!(std::ptr::addr_of!(session.process_up), up_owner_address);
    assert_eq!(session.process_up.map(f32::to_bits), retained_up);
    assert_eq!(state.current_phase(), SessionPhase::Title);
    for (class, (owner, source)) in captured_tasks.into_iter().enumerate() {
        assert_eq!(session.tasks.owner(class as u32).unwrap(), Some(owner));
        assert_eq!(session.tasks.record(owner).unwrap().source, source);
    }
}

#[test]
fn production_helper_constructor_requires_process_null_before_publication() {
    use crate::process_particles::{
        ParticleAllocationRequest, ParticleConstruction, ProcessParticleRuntime,
    };
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let registry_null = session
        .world
        .as_ref()
        .unwrap()
        .arena
        .actor_handles()
        .unwrap()
        .null_handle();
    let world = session.world.as_ref().unwrap();
    let actors_before = world.arena.actor_list().to_vec();
    let object_count_before = world.arena.len();
    let queued_before: Vec<_> = world
        .arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .map(|node| node.object)
        .collect();
    session.factory_process.process_null_handle = None;
    session.tick_world_actor_runtime(40_000, &mut state);
    session.tick_world_camera(40_000);
    let attachment = session
        .world
        .as_ref()
        .unwrap()
        .intro_runtime
        .as_ref()
        .unwrap();
    // The helper's shared actor constructor requires the actual process NULL
    // before publication. The registry sentinel cannot replace this source;
    // route/target construction is never reached.
    let expected_controller_failure = format!(
        "{:?}",
        crate::object_runtime::ArenaIntroTickError::<String>::Kernel(
            crate::intro_sequence::IntroError::Host(crate::object_runtime::ArenaIntroError::Host(
                crate::object_runtime::ActorConstructionError::MissingProcessNull.to_string(),
            )),
        )
    );
    assert_eq!(
        attachment.controller_failure.as_deref(),
        Some(expected_controller_failure.as_str())
    );
    let world = session.world.as_ref().unwrap();
    let sequence = world
        .arena
        .selector88_intro_sequence(attachment.controller)
        .unwrap();
    assert_eq!((sequence.stage_148, sequence.elapsed_138), (0, 0));
    assert_eq!(sequence.secondary_helper_158, None);
    assert_eq!(sequence.primary_helper_154, None);
    // The manager drains prior disposal before reaching the helper factory.
    // No new actor or helper is published by the failed constructor.
    assert_eq!(world.arena.len(), object_count_before - queued_before.len());
    assert_eq!(
        world.arena.actor_list(),
        actors_before
            .into_iter()
            .filter(|actor| !queued_before.contains(actor))
            .collect::<Vec<_>>()
    );
    assert!(queued_before
        .iter()
        .all(|&object| world.arena.object(object).is_err()));
    assert!(world.actor_updates.disposal.stopped().is_empty());
    assert_eq!(
        world.camera_failure,
        Some(crate::object_runtime::CameraPoseError::MissingVehicleProcessNullHandle.to_string())
    );
    assert_eq!(session.factory_process.process_null_handle, None);
    assert_eq!(
        session
            .world
            .as_ref()
            .unwrap()
            .arena
            .actor_handles()
            .unwrap()
            .null_handle(),
        registry_null
    );

    // Direct constructor source is an actual argument, independent of birth
    // globals. Pending transfer and expiration do not reach any new birth.
    session.particles = ProcessParticleRuntime::default();
    let id = session
        .particles
        .allocate(ParticleAllocationRequest {
            payload_bytes: 0x34,
            last_reclaim_category: 1,
            network_active: false,
            phase: Some(4),
        })
        .unwrap();
    session
        .particles
        .construct(
            id,
            ParticleConstruction {
                category: 1,
                position: [1, 1000, 3].into(),
                velocity: Some([0; 3].into()),
                source_handle: 7,
                flags: 0,
            },
        )
        .unwrap();
    session
        .particles
        .record_mut(id)
        .unwrap()
        .write_word(0x10, 1)
        .unwrap();
    session.tick_process_particles(20_000, &state);
    assert!((0..crate::process_particles::PARTICLE_POOL_SIZE)
        .filter_map(|slot| session.particles.occupant(slot))
        .all(|id| session.particles.stopped_phase(id).is_none()));
    assert_eq!(session.particles.free_count(), 400);
    assert!(session.particles.active(1).unwrap().is_empty());
    assert!(session.particles.pending(1).unwrap().is_empty());
    assert_ne!(session.particles.update_globals.gravity, 0);
    assert_eq!(session.factory_process.process_null_handle, None);
    assert_eq!(state.current_phase(), SessionPhase::Title);
}

#[test]
fn production_camera_borrows_the_live_process_owner_and_keeps_loss_prefix() {
    use crate::object_runtime::{ActorHandle, PlayerActorLookupError};
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session.tick_world_actor_runtime(40_000, &mut state);
    session.tick_world_camera(40_000);
    let world = session.world.as_mut().unwrap();
    assert_eq!(world.camera_failure, None);
    let camera = world
        .arena
        .resolve_actor_handle(world.arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    let target = world.arena.selector16(camera).unwrap().target_18c.unwrap();
    let handle = world.arena.actor_follow_target(target).unwrap().handle_04;
    let followed = world.arena.resolve_actor_handle(handle).unwrap().unwrap();
    let basis = world.arena.actor(camera).unwrap().rotating.basis;
    let mut expected_velocity: [u16; 3] = std::array::from_fn(|axis| {
        crate::object_runtime::native_trunc_low32(
            f64::from(f32::from_bits(basis.words[7 + axis])) * -200.0,
        ) as u16
    });
    expected_velocity[1] = expected_velocity[1].wrapping_add(50);
    // Disposal queues the helper while its handle remains live. Actual
    // destruction releases the handle before the public view-mode setter;
    // the missing actor then permits mode 1, published as camera mode 2.
    world.arena.dispose(followed).unwrap();
    world
        .arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: followed,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(world.arena.resolve_actor_handle(handle).unwrap(), None);
    world.arena.set_camera_view_mode(camera, 1).unwrap();
    assert_eq!(world.arena.selector16(camera).unwrap().mode_188, 2);
    world.intro_runtime.as_mut().unwrap().controller_failure =
        Some("supplied schedule stops the unrelated controller".into());
    assert_eq!(
        world.arena.actor_handles().unwrap().null_handle(),
        ActorHandle::from_raw(0)
    );
    // Selector-11 turrets draw process RNG in their own +24 phase. Detach
    // them so the camera failure keeps pinning an untouched stream.
    let turrets: Vec<_> = world
        .arena
        .list(crate::object_runtime::ObjectList::Flag1Primary)
        .iter()
        .map(|node| node.object)
        .filter(|&actor| world.arena.selector11_actor(actor).is_ok())
        .collect();
    for actor in turrets {
        world.arena.set_flag1(actor, false).unwrap();
    }
    let before_globals = session.camera_globals.clone();
    let before_rng = world.rng;
    session.factory_process.process_null_handle = None;
    session.tick_world_actor_runtime(32_000, &mut state);
    session.tick_world_camera(32_000);
    let world = session.world.as_ref().unwrap();
    assert_eq!(
        world.camera_failure,
        Some(PlayerActorLookupError::MissingProcessNullHandle.to_string())
    );
    assert_eq!(world.arena.selector16(camera).unwrap().mode_188, 2);
    assert_eq!(world.arena.selector16(camera).unwrap().byte_187, 1);
    assert_eq!(
        world.arena.actor(camera).unwrap().words_8e_92,
        expected_velocity
    );
    assert_eq!(session.camera_globals, before_globals);
    assert_eq!(world.rng, before_rng);
    assert_eq!(session.factory_process.process_null_handle, None);
}

#[test]
fn shared_gameplay_camera_precedes_particle_rng_and_failure_keeps_the_tail() {
    use crate::process_particles::births::{ParticleBirthInputs, ParticleBirthRequest};
    use crate::session::TickCallback;

    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.mission_override = Some(18);
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    state.restore_mission_elapsed_seconds(13);
    assert!(session.world.as_ref().unwrap().intro_runtime.is_none());
    // Family behavior actors (mission 18's selector34 ships) and its
    // selector-11 turrets draw process RNG in their own +24/+D8 phase. Detach
    // only those callbacks so this test keeps pinning the camera-before-particle
    // RNG order.
    let world = session.world.as_mut().unwrap();
    let behavior: Vec<_> = world
        .arena
        .list(crate::object_runtime::ObjectList::Flag1Primary)
        .iter()
        .map(|node| node.object)
        .filter(|&actor| {
            world.arena.family_update_class(actor).is_ok()
                || world.arena.selector11_actor(actor).is_ok()
        })
        .collect();
    assert!(
        !behavior.is_empty(),
        "mission 18 retains its behavior actors"
    );
    for actor in behavior {
        world.arena.set_flag1(actor, false).unwrap();
    }

    // Admit the real random-acceleration trail, without collision or births
    // during its update. Its three draws distinguish the phase ordering.
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let id = session
        .particles
        .birth_trail(
            ParticleBirthRequest {
                particle_type: 1,
                position: [100, 1000, 200],
                velocity: Some([0, 0, 0]),
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &session.factory_process,
                },
            },
            executable,
            &mut session.world.as_mut().unwrap().rng,
        )
        .unwrap()
        .unwrap();
    let before_velocity: [u16; 3] = std::array::from_fn(|axis| {
        session
            .particles
            .record(id)
            .unwrap()
            .read_word(0x0a + axis * 2)
            .unwrap()
    });
    let mut expected_rng = session.world.as_ref().unwrap().rng;
    session.camera_shake_enabled = 1;
    session.camera_globals.shake_remaining_micros = 0;
    session.camera_globals.shake_pause_micros = 0;
    session.camera_globals.shake_step_micros = 0;
    // Pause restart, then the native X/Z/Y jitter draws.
    for _ in 0..4 {
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
    }
    let mut expected_velocity = before_velocity;
    for (axis, divisor, bias) in [(1, 10, 0), (0, 18, -10), (2, 18, -10)] {
        let increment = (crate::name_entry::name_entry_rng_next(&mut expected_rng) & 0xffff) as i32
            % divisor
            + bias;
        expected_velocity[axis] = expected_velocity[axis].wrapping_add(increment as u16);
    }
    session
        .tick(TickCallback::Gameplay, 40_000, 40_000, &mut state)
        .unwrap();
    // 00487B3C: the update ends in the music service.
    assert_eq!(session.platform.services.music_services, 1);
    let world = session.world.as_ref().unwrap();
    assert!(world.native_camera.is_some());
    assert_eq!(world.camera_failure, None);
    assert_eq!(world.rng, expected_rng);
    assert_eq!(world.mission.save.elapsed_seconds, 13);
    assert!((0..crate::process_particles::PARTICLE_POOL_SIZE)
        .filter_map(|slot| session.particles.occupant(slot))
        .all(|id| session.particles.stopped_phase(id).is_none()));
    assert!(session.particles.pending(1).unwrap().is_empty());
    assert_eq!(session.particles.active(1).unwrap().front(), Some(&id));
    for (axis, expected) in expected_velocity.iter().enumerate() {
        assert_eq!(
            session
                .particles
                .record(id)
                .unwrap()
                .read_word(0x0a + axis * 2)
                .unwrap(),
            *expected
        );
    }

    // The next camera call reaches its missing current process read. It
    // consumes no shake RNG; the ordinary particle callback still executes.
    let camera = world.native_camera;
    session.factory_process.process_null_handle = None;
    for (axis, divisor, bias) in [(1, 10, 0), (0, 18, -10), (2, 18, -10)] {
        let increment = (crate::name_entry::name_entry_rng_next(&mut expected_rng) & 0xffff) as i32
            % divisor
            + bias;
        expected_velocity[axis] = expected_velocity[axis].wrapping_add(increment as u16);
    }
    session
        .tick(TickCallback::Gameplay, 40_000, 40_000, &mut state)
        .unwrap();
    assert_eq!(session.platform.services.music_services, 2);
    let world = session.world.as_ref().unwrap();
    assert_eq!(world.native_camera, camera);
    assert_eq!(
        world.camera_failure,
        Some(crate::object_runtime::CameraPoseError::MissingVehicleProcessNullHandle.to_string())
    );
    assert_eq!(world.rng, expected_rng);
    assert_eq!(world.mission.save.elapsed_seconds, 13);
    assert!((0..crate::process_particles::PARTICLE_POOL_SIZE)
        .filter_map(|slot| session.particles.occupant(slot))
        .all(|id| session.particles.stopped_phase(id).is_none()));
    for (axis, expected) in expected_velocity.iter().enumerate() {
        assert_eq!(
            session
                .particles
                .record(id)
                .unwrap()
                .read_word(0x0a + axis * 2)
                .unwrap(),
            *expected
        );
    }
    // 00438950: paused, the world stands still and the tick still ends in
    // the music service (0043899D).
    session
        .tick(TickCallback::Player, 40_000, 40_000, &mut state)
        .unwrap();
    assert_eq!(session.platform.services.music_services, 3);
    assert_eq!(session.world.as_ref().unwrap().rng, expected_rng);
}

#[test]
fn actual_results_factory_uses_live_process_and_exports_command_through_reload() {
    use crate::object_runtime::{PlayerMessageOperation, PlayerMessageRequest};
    use crate::screen_registry::ScreenLifecycle;

    for (network, word) in [(0, 0x8000), (0x100, 0xffff)] {
        let (mut session, mut controller) = fixture();
        session.boot_frontend().unwrap();
        assert!(controller.request_phase(SessionPhase::Frontend));
        controller.transition(&mut session).unwrap();
        session
            .prepare_offline_player(&mut SessionState::default())
            .unwrap();
        let mut load_state = controller.state().clone();
        session
            .transition_effect(TransitionEffect::LoadWorld, &mut load_state)
            .unwrap();
        // Isolate the results factory after the actual world-retirement
        // prelude. Its own process gate must not replay cleanup; the real
        // linked transition separately verifies vehicle copy/manager order.
        session
            .prepare_intro_results(&SessionState::default())
            .unwrap();
        assert!(matches!(
            session.world.as_ref().unwrap().owner_retirement,
            crate::world_backend::WorldOwnerRetirement::Completed
        ));
        let mut saved = session.offline_game.as_ref().unwrap().player.clone();
        saved.owner_11c = word;
        saved.owner_130 = [0x1020_3040, 4, 0x8877_6655];
        let world = session.world.as_mut().unwrap();
        let first = world.arena.first_player().unwrap();
        world
            .arena
            .restore_player_save_prefix(first, &saved)
            .unwrap();
        world.arena.add_world_player([0x2345_6789; 6]);
        let recipient = world.arena.player_message(first).unwrap().identity();
        world
            .arena
            .apply_player_message(PlayerMessageRequest {
                recipient,
                operation: PlayerMessageOperation::Enable,
            })
            .unwrap();
        // G+1E0 is this actual mission selection, distinct from current22.
        // Selection18 bypasses identity/format before the player prefix;
        // the real Phase8 transition retains this world through PrepareResults.
        world.mission.save.mission_selection = 18;
        assert_eq!(world.mission.current_world_a0, 22);
        let mut expected = world.arena.world_players().to_vec();
        if network == 0 {
            expected[0].command_mask_130[1] |= 0x0080_0000;
        }
        let registry = format!("{:?}", world.arena.actor_handles().unwrap());
        let process_null_handle = session.factory_process.process_null_handle.unwrap();
        session.factory_process.network_flag = network;
        session.factory_process.process_null_handle = None;
        let process = session.factory_process;
        let process_address = std::ptr::addr_of!(session.factory_process);

        assert!(controller.request_phase(SessionPhase::Phase8));
        controller.transition(&mut session).unwrap();
        assert_eq!(controller.state().current_phase(), SessionPhase::Phase8);
        let screen = session.intro_results_screen().unwrap();
        assert_eq!(screen.lifecycle(), ScreenLifecycle::Ready);
        assert_eq!(screen.player_word_144(), word);
        let id = screen.id().unwrap();
        assert_eq!(session.registry.selected_screen(), Some(id));
        assert_eq!(
            session.input.selected_context(),
            session.registry.screen(id).unwrap().input_context
        );
        let world = session.world.as_ref().unwrap();
        assert_eq!(world.mission.save.mission_selection, 18);
        assert_eq!(world.arena.world_players(), expected);
        assert_eq!(
            format!("{:?}", world.arena.actor_handles().unwrap()),
            registry
        );
        assert_eq!(session.factory_process, process);
        assert_eq!(std::ptr::addr_of!(session.factory_process), process_address);
        assert_eq!(controller.state().current_phase(), SessionPhase::Phase8);

        // Export uses the completed real Phase8 state and the retained live
        // player, without replacing the actual owner with a save mirror.
        session
            .retain_offline_world_load(controller.state())
            .unwrap();
        let retained = session.world.as_ref().unwrap();
        assert_eq!(retained.mission.current_world_a0, 22);
        assert_eq!(retained.mission.save.mission_selection, 18);
        let first = retained.arena.first_player().unwrap();
        let saved = retained.arena.export_player_save_state(first).unwrap();
        assert_eq!(saved.owner_11c, word);
        assert_eq!(saved.owner_130, expected[0].command_mask_130);
        let message = retained.arena.player_message(first).unwrap().identity();
        let carriers = retained
            .constructions
            .iter()
            .map(|carrier| (carrier.player(), carrier.callback_context()))
            .collect::<Vec<_>>();
        // Results admission above bypasses the missing current NULL. A fresh
        // world reload reaches camera creation and requires that actual owner.
        session.factory_process.process_null_handle = Some(process_null_handle);
        // The loader's network branches need a network world; the campaign
        // world reloads offline.
        session.factory_process.network_flag = 0;
        let reload_process = session.factory_process;
        let mut reload_state = controller.state().clone();
        session
            .transition_effect(TransitionEffect::LoadWorld, &mut reload_state)
            .unwrap();
        let reloaded = session.world.as_ref().unwrap();
        let player = reloaded
            .arena
            .world_player(reloaded.player.unwrap())
            .unwrap();
        assert_eq!(reloaded.mission.current_world_a0, 18);
        // Actual loader player setup 0046A880 resets the restored counter;
        // the export above preserves its raw value, while command words
        // survive that later setup phase.
        assert_eq!(player.word_11c, 0);
        assert_eq!(player.command_mask_130, expected[0].command_mask_130);
        assert_eq!(reloaded.arena.first_player(), Some(first));
        assert_eq!(
            reloaded.arena.player_message(first).unwrap().identity(),
            message
        );
        assert_eq!(
            reloaded
                .constructions
                .iter()
                .map(|carrier| (carrier.player(), carrier.callback_context()))
                .collect::<Vec<_>>(),
            carriers
        );
        assert_eq!(session.factory_process, reload_process);
        assert_eq!(std::ptr::addr_of!(session.factory_process), process_address);
    }
}

#[test]
fn actual_results_device_failure_publishes_partial_screen_before_player_and_binding() {
    use crate::screen_registry::{ScreenLifecycle, ScreenMembership};

    let (mut session, _) = fixture();
    session.boot_frontend().unwrap();
    let selected_screen = session.registry.selected_screen();
    let selected_context = session.input.selected_context();
    assert!(selected_screen.is_some());
    assert!(selected_context.is_some());
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    let mut state = SessionState::default();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let mut saved = session.offline_game.as_ref().unwrap().player.clone();
    saved.owner_11c = 0xffff;
    saved.owner_130 = [0x1020_3040, 4, 0x8877_6655];
    let world = session.world.as_mut().unwrap();
    let first = world.arena.first_player().unwrap();
    world
        .arena
        .restore_player_save_prefix(first, &saved)
        .unwrap();
    let players = world.arena.world_players().to_vec();
    let arena = format!("{:?}", world.arena);
    session.factory_process.process_null_handle = None;
    let process = session.factory_process;
    let devices_created = session.platform.frontend.devices_created;
    session.platform.frontend.fail_enumeration = true;
    state.request_phase(SessionPhase::Phase8);

    assert!(matches!(
        session.construct_intro_results(&mut state),
        Err(PortSessionError::IntroResults(error))
            if error == "required intro results effect failed"
    ));
    let screen = session.intro_results_screen().unwrap();
    assert_eq!(screen.lifecycle(), ScreenLifecycle::Stopped);
    assert_eq!(screen.player_word_144(), 0);
    let id = screen.id().unwrap();
    assert_eq!(
        session.registry.screen(id).unwrap().membership,
        ScreenMembership::Linked
    );
    assert_eq!(session.registry.screen(id).unwrap().input_context, None);
    assert_eq!(session.registry.selected_screen(), selected_screen);
    assert_eq!(session.input.selected_context(), selected_context);
    assert_eq!(session.platform.frontend.devices_created, devices_created);
    assert_eq!(
        session.platform.frontend.effects.last(),
        Some(&FrontendEffect::Enumerate)
    );
    assert_eq!(
        session.world.as_ref().unwrap().arena.world_players(),
        players
    );
    assert_eq!(
        format!("{:?}", session.world.as_ref().unwrap().arena),
        arena
    );
    assert_eq!(session.factory_process, process);

    session
        .destroy_screen(PortScreen::IntroResults, &mut state)
        .unwrap();
    assert!(session.intro_results_screen().is_none());
    assert_eq!(session.registry.selected_screen(), selected_screen);
    assert_eq!(session.input.selected_context(), selected_context);
}

/// A crash, 2026-10-16: a world gate's Results retired a world
/// with a ship left Dormant (descriptor 0). The walk stopped at that ship's
/// destructor exit 0043E120, the next world's load refused the stopped
/// retirement, and the game stopped on the next briefing. The exit now runs
/// over the world's owners (its burst draws from the world's RNG word), the
/// ship is destroyed and the retirement completes.
#[test]
fn results_retirement_runs_a_dormant_ships_exit_and_completes() {
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let world = session.world.as_mut().unwrap();
    let ship = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector35_actor(actor).is_ok())
        .expect("the loaded world places selector35 ships");
    world.arena.set_family_descriptor_for_tests(ship, 0);
    let rng = world.rng;
    session.prepare_intro_results(&state).unwrap();
    let world = session.world.as_ref().unwrap();
    assert!(matches!(
        world.owner_retirement,
        crate::world_backend::WorldOwnerRetirement::Completed
    ));
    assert!(world.arena.object(ship).is_err());
    assert_ne!(world.rng, rng);
}

#[test]
fn mission_clear_disposes_furniture_before_results_reload_and_world_teardown() {
    use crate::world_furniture_effect::{FurnitureEffectProgram, FurnitureQueueRequest};
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let program = FurnitureEffectProgram::new(0x0053_2c88).unwrap();
    let request = FurnitureQueueRequest {
        xz: [100, 200],
        program,
    };
    session
        .furniture
        .enqueue(&session.world.as_ref().unwrap().terrain, request)
        .unwrap();
    session.furniture_failure = Some("previous reached callback failure".into());
    session.camera_shake_enabled = 1;
    let rng = session.world.as_ref().unwrap().rng;
    session
        .prepare_intro_results(&SessionState::default())
        .unwrap();
    assert_eq!(session.world.as_ref().unwrap().mission.byte_12, Some(0));
    assert!(session.furniture.records().is_empty());
    assert_eq!(session.furniture_failure, None);
    assert_eq!(session.camera_shake_enabled, 0);
    assert_eq!(session.world.as_ref().unwrap().rng, rng);
    // Results keeps the world for the existing reload owner. A second
    // admission must also be cleared by the earlier mission-clear subphase
    // of 004871C0, before particle/actor teardown.
    session
        .furniture
        .enqueue(&session.world.as_ref().unwrap().terrain, request)
        .unwrap();
    session.camera_shake_enabled = 1;
    session
        .transition_effect(TransitionEffect::LeaveWorld, &mut state)
        .unwrap();
    assert!(session.furniture.records().is_empty());
    assert_eq!(session.camera_shake_enabled, 0);
    assert!(session.world.is_none());
}

#[test]
fn session_particle_pool_survives_world_replacement_and_clear_retains_payloads() {
    use crate::process_particles::births::{ParticleBirthInputs, ParticleBirthRequest};
    struct NoAudio;
    impl crate::process_particles::births::ParticleBirthAudio for NoAudio {
        type Error = std::convert::Infallible;
        fn explosion_sound(
            &mut self,
            _: crate::process_particles::audio::ParticleSoundRequest,
            _: &mut crate::process_particles::ProcessParticleRuntime,
            _: &mut u32,
        ) -> Result<(), Self::Error> {
            panic!("native central type3 must not enqueue sound")
        }
    }
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let executable =
        FrontendExecutable::parse(session.catalog.assets().executable_bytes()).unwrap();
    let source_handle = session
        .world
        .as_ref()
        .unwrap()
        .arena
        .actor_handles()
        .unwrap()
        .null_handle()
        .raw();
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(source_handle)),
    };
    let id = session
        .particles
        .birth_central(
            ParticleBirthRequest {
                particle_type: 3,
                position: [100, 1000, 200],
                velocity: Some([64, 0, 0]),
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &birth_process,
                },
            },
            executable,
            &mut session.world.as_mut().unwrap().rng,
            &mut NoAudio,
        )
        .unwrap()
        .unwrap();
    let before = session
        .particles
        .record(id)
        .unwrap()
        .read_word(0x10)
        .unwrap();
    session.tick_process_particles(32_768, &state);
    assert!((0..crate::process_particles::PARTICLE_POOL_SIZE)
        .filter_map(|slot| session.particles.occupant(slot))
        .all(|id| session.particles.stopped_phase(id).is_none()));
    assert_eq!(session.particles.pending(1).unwrap().len(), 0);
    assert_eq!(session.particles.active(1).unwrap().front(), Some(&id));
    assert_eq!(
        session
            .particles
            .record(id)
            .unwrap()
            .read_word(0x10)
            .unwrap(),
        before - 32
    );
    assert_eq!(
        session.particles.record(id).unwrap().position().unwrap(),
        [102, 1000, 200]
    );

    session.particles.vector_cursor = u32::MAX;
    session
        .world
        .as_mut()
        .unwrap()
        .mission
        .save
        .mission_selection = 18;
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    assert_eq!(session.particles.active(1).unwrap().front(), Some(&id));
    assert_eq!(session.particles.vector_cursor, u32::MAX);
    let retained = session.particles.record(id).unwrap().read_dword(0).unwrap();
    session
        .transition_effect(TransitionEffect::LeaveWorld, &mut state)
        .unwrap();
    assert!(session.particles.active(1).unwrap().is_empty());
    assert_eq!(
        session.particles.record(id).unwrap().read_dword(0).unwrap(),
        retained
    );
    assert_eq!(session.particles.vector_cursor, u32::MAX);
    assert!(session.world.is_none());
}

#[test]
fn boot_title_builds_the_real_screen_and_yields_to_frontend() {
    let (mut session, mut controller) = fixture();
    session.boot_title().unwrap();
    let title = session.title_screen().expect("title screen");
    assert!(!title.finished());
    assert!(!title.video_present());
    assert!(!session.title_process().title_seen);
    assert!(session.frontend_screen().is_none());
    // The absent video makes no phase request from the factory itself.
    assert!(!controller.state().transition_pending());

    // The linked title tick's finish requests phase 2 (proven through the
    // screen and linked owners); the kernel then destroys the title and
    // builds the frontend through its factory.
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut session).unwrap();
    assert_eq!(controller.state().current_phase(), SessionPhase::Frontend);
    assert!(session.title_screen().is_none());
    assert!(session.frontend_screen().is_some());
    assert_eq!(session.platform().services.music_selections, 1);
    assert_eq!(session.platform().services.baseline_resets, 1);
    assert_eq!(session.platform().video.opens, 1);
    assert_eq!(session.platform().video.restores, 0);
}

#[test]
fn boot_frontend_then_2_9_2_runs_the_real_screens() {
    let (mut session, mut controller) = fixture();
    session.boot_frontend().unwrap();
    assert!(session.frontend_screen().is_some());
    assert_eq!(session.platform().frontend.effects.len(), 4);

    // Title -> 2 through the kernel: the booted frontend screen reuses phase
    // 2, frontend music is selected and baselines reset.
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut session).unwrap();
    assert_eq!(controller.state().current_phase(), SessionPhase::Frontend);
    assert_eq!(session.platform().services.music_selections, 1);
    assert_eq!(session.platform().services.baseline_resets, 1);
    assert!(session.offline_game().is_none());

    // 2 -> 9: the frontend screen is destroyed, the identity read supplies
    // the editable buffers, and PreparePhase9 publishes the offline state.
    assert!(controller.request_phase(SessionPhase::Phase9));
    controller.transition(&mut session).unwrap();
    assert_eq!(controller.state().current_phase(), SessionPhase::Phase9);
    assert!(session.frontend_screen().is_none());
    let text = session.text_screen().expect("text-entry screen");
    assert_eq!(text.entry().expect("entry state").name(), b"Pilot");
    assert_eq!(text.entry().expect("entry state").limit(), 18);
    assert_eq!(session.platform().text.reads, [SessionPhase::Phase9]);
    assert!(session.owner_byte());
    let offline = session.offline_game().expect("published offline game");
    assert_eq!(offline.mission.mission_selection, 22);
    assert_eq!(offline.player.owner_144, 1);
    assert_eq!(session.platform().services.baseline_resets, 2);
    // The frontend destroy published the returning flag for the next entry.
    assert_eq!(
        session.process.entry(),
        crate::frontend_camera::FrontendEntry::Returning
    );

    // 9 -> 2: LeaveWorld clears the prepared state, frontend music runs and a
    // fresh frontend screen is constructed (second full factory pass).
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut session).unwrap();
    assert_eq!(controller.state().current_phase(), SessionPhase::Frontend);
    assert!(session.text_screen().is_none());
    assert!(session.frontend_screen().is_some());
    assert!(!session.owner_byte());
    assert!(session.offline_game().is_none());
    assert_eq!(session.platform().services.music_selections, 2);
    assert_eq!(session.platform().services.baseline_resets, 3);
    assert_eq!(session.platform().frontend.effects.len(), 8);
    assert_eq!(session.platform().frontend.devices_created, 2);
    assert_eq!(session.platform().text.devices_created, 1);
}

#[test]
fn unsupported_factories_and_ticks_are_explicit() {
    let (mut session, _) = fixture();
    let mut state = crate::session::SessionState::default();
    assert!(matches!(
        session.create_screen(ScreenFactory::Phase5, session.attachment, &mut state),
        Err(PortSessionError::UnsupportedFactory(ScreenFactory::Phase5))
    ));
    // The map factory serves only a requested phase 7.
    assert!(matches!(
        session.create_screen(ScreenFactory::Phase7, session.attachment, &mut state),
        Err(PortSessionError::Map(_))
    ));
    // The intro-only phase-8 factory is admitted, but requires the retained
    // intro world before any construction effects. Other factories above
    // remain explicit unsupported boundaries.
    assert!(matches!(
        session.create_screen(ScreenFactory::Phase8, session.attachment, &mut state),
        Err(PortSessionError::World(WorldBackendError::MissingWorld))
    ));
}

#[test]
fn startup_factory_constructs_and_requests_the_title_outside_a_transition() {
    let (mut session, _) = fixture();
    // As 004389C0's boot construction: no transition is pending, so with
    // settings loaded the constructor's phase-1 request is accepted.
    let mut state = crate::session::SessionState::default();
    session
        .create_screen(ScreenFactory::Startup, session.attachment, &mut state)
        .unwrap();
    assert_eq!(session.current_screen(), Some(PortScreen::Startup));
    assert!(state.transition_pending());
    assert_eq!(state.requested_phase(), SessionPhase::Title);
    assert_eq!(session.residency().summary().active_leaves, [0, 2]);
    assert!(matches!(
        session.create_screen(ScreenFactory::Startup, session.attachment, &mut state),
        Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Startup))
    ));
    session
        .destroy_screen(PortScreen::Startup, &mut state)
        .unwrap();
    assert_eq!(session.current_screen(), None);
}

#[test]
fn key_config_factory_remembers_the_phase_it_was_built_from() {
    let (mut session, _) = fixture();
    // 00444BD0 stores +64C, still the previous phase mid-transition.
    let mut state = crate::session::SessionState::pending_for_tests(
        SessionPhase::Frontend,
        SessionPhase::Phase12,
    );
    session
        .create_screen(ScreenFactory::Phase12, session.attachment, &mut state)
        .unwrap();
    assert_eq!(session.current_screen(), Some(PortScreen::KeyConfig));
    let screen = session
        .key_config_screen()
        .expect("key configuration screen");
    assert_eq!(screen.return_phase(), SessionPhase::Frontend);
    assert_eq!(screen.menus().expect("menu controller").len(), 1);
    assert!(matches!(
        session.create_screen(ScreenFactory::Phase12, session.attachment, &mut state),
        Err(PortSessionError::ScreenAlreadyPresent(
            PortScreen::KeyConfig
        ))
    ));
    session
        .destroy_screen(PortScreen::KeyConfig, &mut state)
        .unwrap();
    assert_eq!(session.current_screen(), None);
}

#[test]
fn attract_prepare_keeps_its_explicit_application_admission() {
    let (mut session, mut controller) = fixture();
    session.boot_frontend().unwrap();
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut session).unwrap();
    session.attract = true;
    assert!(controller.request_phase(SessionPhase::Phase9));
    assert_eq!(
        session.admit_transition(controller.state()),
        Err(SessionBoundary::AttractDemo)
    );
    assert!(!session.owner_byte());
    assert!(session.pending_world.is_none());
}

/// A network session's new game is admitted; its world comes from the
/// platform's link at the preparation (00487485).
#[test]
fn network_prepare_is_admitted() {
    let (mut session, mut controller) = fixture();
    session.boot_frontend().unwrap();
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut session).unwrap();
    session.factory_process.network_flag = 0x100;
    assert!(controller.request_phase(SessionPhase::Phase9));
    assert_eq!(session.admit_transition(controller.state()), Ok(()));
}

/// 00414480 and 004147D0, the computer's database and hint list, request
/// phase 4 when they close, though the game may still be in it; the
/// session admits the request and reuses the gameplay screen.
#[test]
fn a_closed_computer_menu_may_ask_for_the_running_gameplay() {
    let (session, _) = fixture();
    let state = crate::session::SessionState::pending_for_tests(
        SessionPhase::Gameplay,
        SessionPhase::Gameplay,
    );
    assert_eq!(session.admit_transition(&state), Ok(()));
    assert!(crate::gameplay_screen::gameplay_reuses_phase(
        SessionPhase::Gameplay
    ));
}

/// 00443070, Escape over the gameplay screen, asks for the in-world menu
/// while a hint pauses the game (phase 6) as in phase 4; the menu's
/// destructor 00443400 then requests 4 for the 6 it stored. The port stopped
/// the session there (a crash of 2026-10-21 23:40).
#[test]
fn escape_during_a_hint_pause_opens_the_in_world_menu() {
    let (session, _) = fixture();
    let state =
        crate::session::SessionState::pending_for_tests(SessionPhase::Phase6, SessionPhase::Phase5);
    assert_eq!(session.admit_transition(&state), Ok(()));
}

#[test]
fn text_screen_reuse_covers_only_phases_9_through_11() {
    assert!(text_entry_reuses_phase(SessionPhase::Phase9));
    assert!(text_entry_reuses_phase(SessionPhase::Phase10));
    assert!(text_entry_reuses_phase(SessionPhase::Phase11));
    assert!(!text_entry_reuses_phase(SessionPhase::Frontend));
    assert!(!text_entry_reuses_phase(SessionPhase::Phase3));
}

#[test]
fn actual_prepare_retains_each_reached_error_and_refuses_to_replay() {
    use crate::object_runtime::{
        PlayerConstructionError, PlayerConstructionPhase, PlayerInputError,
    };
    use crate::player_input_source::{PlayerAnalogDescriptors, PlayerInputSourceError};
    use crate::player_prepare::{PlayerPrepareError, PlayerPreparePhase};
    #[derive(Clone, Copy)]
    enum Failure {
        Analog,
        ProcessNull,
        Network,
    }
    for failure in [Failure::Analog, Failure::ProcessNull, Failure::Network] {
        let (mut session, _) = fixture();
        session.mission_override = Some(18);
        session.player_cycle.armed_5d8 = 9;
        session.player_cycle.selection_index_04 = 7;
        match failure {
            Failure::Analog => {
                session.player_analog = PlayerAnalogDescriptors::from_current([None; 3])
            }
            Failure::ProcessNull => session.factory_process.process_null_handle = None,
            Failure::Network => session.factory_process.network_flag = 0x100,
        }
        let mut state = SessionState::default();
        state.restore_mission_elapsed_seconds(137);
        let result = session.transition_effect(TransitionEffect::PreparePhase9, &mut state);
        match failure {
            Failure::Analog => assert!(matches!(
                result,
                Err(PortSessionError::Prepare(PlayerPrepareError::Construction(
                    PlayerConstructionError::Input(PlayerInputError::Source {
                        error: PlayerInputSourceError::UnavailableAnalog {
                            identity: 0x0054_ecf8
                        },
                        ..
                    })
                )))
            )),
            Failure::ProcessNull => assert!(matches!(
                result,
                Err(PortSessionError::Prepare(PlayerPrepareError::Construction(
                    PlayerConstructionError::MissingProcessNull
                )))
            )),
            Failure::Network => assert!(matches!(
                result,
                Err(PortSessionError::Prepare(
                    PlayerPrepareError::NetworkMissionSelection { network: 0x100 }
                ))
            )),
        }
        assert!(session.owner_byte());
        assert!(session.offline_game().is_none());
        assert!(session.player_prepare_stopped.is_some());
        assert_eq!(
            state.gameplay_time().seconds,
            137,
            "mission reset was not reached"
        );
        let owner = session.pending_world.as_ref().unwrap();
        assert_eq!(
            owner.mission.save.mission_selection, 22,
            "debug override is after reached reset"
        );
        assert_eq!(
            owner.mission.current_world_a0, 1,
            "native48DA88 constructor store"
        );
        assert_eq!(owner.constructions.len(), 1);
        assert_eq!(
            owner.constructions[0].callback_context(),
            world_owners::PLAYER_CALLBACK_CONTEXT_START
        );
        assert_eq!(session.player_cycle.selection_index_04, 7);
        if matches!(failure, Failure::Network) {
            assert_eq!(owner.prepare_phase, PlayerPreparePhase::SelectingMission);
            assert_eq!(
                owner.constructions[0].phase(),
                PlayerConstructionPhase::Published
            );
            assert_eq!(
                owner.arena.first_player(),
                Some(owner.constructions[0].player())
            );
            assert_eq!(session.player_cycle.armed_5d8, 0);
        } else {
            assert_eq!(owner.prepare_phase, PlayerPreparePhase::Constructing);
            assert_eq!(
                owner.constructions[0].phase(),
                PlayerConstructionPhase::Stopped
            );
            assert!(owner.arena.first_player().is_none());
            assert_eq!(session.player_cycle.armed_5d8, 9);
        }
        let retained = format!("{owner:?}");
        let callback_next = session.player_callback_next;
        let input = (
            session.input.clock_micros(),
            session.input.queued_len(),
            session.input.selected_context(),
        );
        assert!(matches!(
            session.transition_effect(TransitionEffect::PreparePhase9, &mut state),
            Err(PortSessionError::WorldOwnerBoundary(_))
        ));
        assert_eq!(
            format!("{:?}", session.pending_world.as_ref().unwrap()),
            retained
        );
        assert_eq!(session.player_callback_next, callback_next);
        assert_eq!(
            (
                session.input.clock_micros(),
                session.input.queued_len(),
                session.input.selected_context()
            ),
            input
        );
    }
}

#[test]
fn real_name_entry_back_and_new_game_retain_the_same_manager_and_carriers() {
    let (mut session, mut controller) = fixture();
    session.boot_frontend().unwrap();
    controller.request_phase(SessionPhase::Frontend);
    controller.transition(&mut session).unwrap();
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    let owner = session.pending_world.as_ref().unwrap();
    assert_eq!(owner.constructions.len(), 1);
    let first = owner.arena.first_player().unwrap();
    let callback = owner.constructions[0].callback_context();
    let registry = format!("{:?}", owner.arena.actor_handles().unwrap());
    assert_eq!(owner.constructions[0].player(), first);
    controller.request_phase(SessionPhase::Frontend);
    controller.transition(&mut session).unwrap();
    let owner = session.pending_world.as_ref().unwrap();
    assert!(owner.arena.first_player().is_none());
    assert_eq!(owner.constructions.len(), 1);
    assert!(owner.constructions[0].retiring_player().is_some());
    let retired = format!("{:?}", owner.constructions[0]);
    assert!(session.offline_game().is_none());
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    let owner = session.pending_world.as_ref().unwrap();
    assert_eq!(owner.constructions.len(), 2);
    assert_eq!(format!("{:?}", owner.constructions[0]), retired);
    assert_eq!(
        format!("{:?}", owner.arena.actor_handles().unwrap()),
        registry
    );
    assert_ne!(owner.constructions[1].player(), first);
    assert_eq!(
        owner.arena.first_player(),
        Some(owner.constructions[1].player())
    );
    assert_eq!(owner.constructions[1].callback_context(), callback + 1);
    assert!(session.offline_game().is_some());
}

/// A ship left Dormant (descriptor 0) when its world ends: the destructor
/// 0043D0F0 runs 0043E120(false), its burst and radius, over the world's
/// owners, and the leave completes. Before, the cleanup walk stopped there
/// and the next world's load refused (a crash, 2026-10-16). The
/// leave clears the particles after the walk (004871FB), so the exit shows
/// in the RNG word its burst drew from.
#[test]
fn leaving_a_world_with_a_dormant_ship_runs_its_exit_and_completes() {
    let leave = |dormant: bool| {
        let (mut session, _) = fixture();
        let mut state = SessionState::default();
        session.prepare_offline_player(&mut state).unwrap();
        session
            .transition_effect(TransitionEffect::LoadWorld, &mut state)
            .unwrap();
        let world = session.world.as_mut().unwrap();
        let ship = world
            .arena
            .actor_list()
            .iter()
            .copied()
            .find(|&actor| world.arena.selector35_actor(actor).is_ok())
            .expect("the loaded world places selector35 ships");
        if dormant {
            world.arena.set_family_descriptor_for_tests(ship, 0);
        }
        session.clear_constructed_world(&state).unwrap();
        assert!(session.world.is_none());
        *session.platform.process_rng()
    };
    assert_ne!(leave(true), leave(false));
}

#[test]
fn actual_leave_keeps_audio_stop_before_owner_and_input_release_and_refuses_replay() {
    use crate::input::InputStateError;
    use crate::object_runtime::{
        PlayerDestructionPhase, PlayerInputDescriptor, PlayerInputDescriptorKind,
        PlayerInputDescriptorRequest, PlayerInputDescriptorSource,
    };
    use crate::player_input_source::PlayerInputSource;
    use crate::session_platform::world_owners::{
        GameAudioCustody, PlayerLeavePhase, PlayerLeaveState,
    };
    use crate::world_backend::WorldOwnerRetirement;

    for missing_audio in [false, true] {
        let (mut session, _) = fixture();
        let mut state = SessionState::default();
        session.prepare_offline_player(&mut state).unwrap();
        session
            .transition_effect(TransitionEffect::LoadWorld, &mut state)
            .unwrap();
        let world = session.world.as_ref().unwrap();
        let player = world.arena.first_player().unwrap();
        let callback = world.constructions[0].callback_context();
        let carrier = format!("{:?}", world.constructions[0]);
        let loaded_map = world.map;
        let executable =
            FrontendExecutable::parse(session.catalog.assets().executable_bytes()).unwrap();
        let mut source = PlayerInputSource {
            analog: &session.player_analog,
            executable,
        };
        let PlayerInputDescriptor::Digital(descriptor) = source
            .descriptor(PlayerInputDescriptorRequest {
                kind: PlayerInputDescriptorKind::Digital,
                identity: 0x0053_a390,
                player_offset: 0,
            })
            .unwrap()
        else {
            unreachable!()
        };
        let consumer = session
            .input
            .acquire_consumer(descriptor, callback)
            .unwrap();
        assert_eq!(
            session.input.consumer_state(consumer).unwrap().references,
            2
        );
        session.input.release_consumer(consumer).unwrap();
        let input_before = session.input.consumer_state(consumer).unwrap();
        assert_eq!(input_before.references, 1);
        assert_eq!(
            session.player_audio_5d0,
            GameAudioCustody::ConstructorAbsent
        );
        if missing_audio {
            // Supplied unavailable custody; this invents no present native pointer
            // or audio writer. The authentic constructor initializes this NULL.
            session.player_audio_5d0 = GameAudioCustody::Unavailable;
        }
        let result = session.clear_constructed_world(&SessionState::default());
        assert!(session.world.is_none());
        assert!(session.map_load.current().is_none());
        assert!(session.maps.jitter(loaded_map).is_err());
        let owner = session.pending_world.as_ref().unwrap();
        assert!(owner.arena.is_empty());
        assert_eq!(owner.mission.byte_12, Some(0));
        assert!(matches!(
            owner.owner_retirement,
            WorldOwnerRetirement::Completed
        ));
        if missing_audio {
            let error = result.unwrap_err();
            assert!(matches!(
                error,
                PortSessionError::WorldOwnerBoundary("current game audio owner unavailable")
            ));
            let PlayerLeaveState::Stopped { phase, reason } = &session.player_leave_state else {
                panic!("missing retained audio stop")
            };
            assert_eq!(*phase, PlayerLeavePhase::Audio);
            assert_eq!(reason, &error.to_string());
            assert!(session.owner_byte);
            assert!(session.offline_game.is_some());
            assert_eq!(owner.arena.first_player(), Some(player));
            assert_eq!(format!("{:?}", owner.constructions[0]), carrier);
            assert_eq!(
                session.input.consumer_state(consumer).unwrap(),
                input_before
            );
            let retained = format!("{:?}", owner);
            let stop = format!("{:?}", session.player_leave_state);
            let rng = *session.platform.process_rng();
            // Resolving a later source does not authorize replay of the earlier
            // native map/message/manager phases or release the retained inputs.
            session.player_audio_5d0 = GameAudioCustody::ConstructorAbsent;
            assert!(matches!(
                session.clear_constructed_world(&SessionState::default()),
                Err(PortSessionError::WorldOwnerBoundary(
                    "LeaveWorld already stopped"
                ))
            ));
            assert_eq!(
                format!("{:?}", session.pending_world.as_ref().unwrap()),
                retained
            );
            assert_eq!(format!("{:?}", session.player_leave_state), stop);
            assert_eq!(*session.platform.process_rng(), rng);
            assert_eq!(
                session.input.consumer_state(consumer).unwrap(),
                input_before
            );
            assert!(session.owner_byte);
        } else {
            result.unwrap();
            assert!(matches!(
                session.player_leave_state,
                PlayerLeaveState::Completed
            ));
            assert!(!session.owner_byte);
            assert!(session.offline_game.is_none());
            assert!(owner.arena.first_player().is_none());
            assert_eq!(owner.constructions[0].player(), player);
            assert_eq!(owner.constructions[0].callback_context(), callback);
            assert_eq!(
                owner.constructions[0].destruction_phase(),
                Some(PlayerDestructionPhase::Complete)
            );
            assert!(owner.constructions[0].retiring_player().is_some());
            assert_eq!(
                session.input.consumer_state(consumer),
                Err(InputStateError::InvalidConsumer)
            );
        }
    }
}

#[path = "session_platform/terrain_policy_tests.rs"]
mod terrain_policy;

/// At 144 Hz (6 and 7 ms frames) the exact quantum gives the world whole
/// 16384 µs multiples, skipping frames that reach none, and loses nothing:
/// the world's deltas and the mission's milliseconds sum to the frames'.
#[test]
fn exact_world_ticks_carry_every_microsecond() {
    let quantum = WorldTickQuantum::Exact { micros: 16384 };
    let mut carry = WorldTickCarry::default();
    let frames: Vec<i32> = (0..144)
        .map(|i| if i % 3 == 0 { 6000 } else { 7000 })
        .collect();
    let mut world = 0i64;
    let mut millis = 0u32;
    let mut ticks = 0;
    let mut spans = 0u32;
    for &frame in &frames {
        let delta = carry.world_delta(quantum, frame);
        assert_eq!(delta % 16384, 0);
        if delta != 0 {
            ticks += 1;
            millis += carry.mission_millis(quantum, delta);
            // The input's held durations span every frame since the last
            // tick, skipped ones included.
            spans += carry.take_input_micros();
        }
        world += i64::from(delta);
        // Gravity's step is exact: delta >> 10 loses no microsecond.
        assert_eq!((delta >> 10) << 10, delta);
    }
    let total: i64 = frames.iter().map(|&f| i64::from(f)).sum();
    assert_eq!(world + i64::from(carry.delta_micros), total);
    assert_eq!(i64::from(spans + carry.input_micros), total);
    assert_eq!(
        i64::from(millis) * 1000 + i64::from(carry.mission_micros),
        world
    );
    // 0.96 s of frames reach 58 whole quanta: about 61 ticks a second.
    assert_eq!(ticks, 58);
    // Retail passes each frame and truncates the milliseconds.
    let mut retail = WorldTickCarry::default();
    assert_eq!(retail.world_delta(WorldTickQuantum::Retail, 6999), 6999);
    assert_eq!(retail.mission_millis(WorldTickQuantum::Retail, 6999), 6);
    assert_eq!(retail.take_input_micros(), 0);
}

/// Paced, the scientists and the alien ships take every other exact world
/// tick with both ticks' time, as retail's 30 fps gave them; a long tick
/// that reaches the pace alone runs them at once. Unpaced, every tick's
/// own delta.
#[test]
fn paced_actors_update_every_other_exact_tick_with_both_ticks_time() {
    use crate::session_platform::ActorPacing;

    let paced = crate::frontend_settings::PortFixes::default().actor_pacing();
    assert_eq!(paced, ActorPacing::Paced { micros: 32768 });
    let mut carry = WorldTickCarry::default();
    let deltas: Vec<Option<u32>> = (0..6).map(|_| carry.paced_delta(paced, 16384)).collect();
    assert_eq!(
        deltas,
        [None, Some(32768), None, Some(32768), None, Some(32768)]
    );
    assert_eq!(carry.paced_delta(paced, 49152), Some(49152));
    assert_eq!(carry.paced_delta(paced, 16384), None);
    assert_eq!(carry.paced_delta(paced, 32768), Some(49152));
    assert_eq!(carry.paced_delta(ActorPacing::Retail, 16384), Some(16384));
}

/// A gameplay tick that reaches no whole quantum skips the world and holds
/// the frame; the next that reaches one runs it. Outside the gameplay
/// callback the hold never applies.
#[test]
fn a_skipped_world_tick_holds_the_gameplay_frame() {
    use crate::session::TickCallback;

    let (mut session, _) = fixture();
    session.platform.services.exact_ticks = true;
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session
        .tick(TickCallback::Gameplay, 8_000, 8_000, &mut state)
        .unwrap();
    assert!(session.world_tick_skipped);
    assert_eq!(session.platform.services.music_services, 0);
    // The default state's callback is Idle: a held flag draws anyway.
    assert!(!session.holds_gameplay_frame(&state));
    session
        .tick(TickCallback::Gameplay, 9_000, 9_000, &mut state)
        .unwrap();
    assert!(!session.world_tick_skipped);
    assert_eq!(session.platform.services.music_services, 1);
}

/// 0048E2E0's doom countdown (G+1C7 set) with the player in the vehicle: in
/// the last 5 s the world factory builds resource 0x37 by the unit on a
/// chance (0048E380..0048E4C5), and the end's first stage retires the
/// vehicle (0047CE90, 0048E505..0048E515) between the +18 and the 3 s.
#[test]
fn the_doom_countdown_sets_off_explosions_and_retires_the_vehicle() {
    use crate::object_runtime::ObjectList;
    use crate::session::TickCallback;

    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let explosions = |session: &PortSession<Recording>| {
        let arena = &session.world.as_ref().unwrap().arena;
        let mut objects = std::collections::HashSet::new();
        for list in [
            ObjectList::Flag0Primary,
            ObjectList::Flag0Secondary,
            ObjectList::Flag1Primary,
            ObjectList::Flag1Secondary,
            ObjectList::Parking,
            ObjectList::Disposal,
        ] {
            objects.extend(arena.list(list).iter().map(|node| node.object));
        }
        objects
            .into_iter()
            .filter(|&object| {
                arena.object(object).unwrap().base.resource_word
                    == super::countdown::COUNTDOWN_EXPLOSION_RESOURCE as u16
            })
            .count()
    };
    let world = session.world.as_mut().unwrap();
    let player = world.arena.first_player().unwrap();
    let vehicle = world.arena.player_actor(player).unwrap().unwrap();
    assert!(world.arena.actor_is_vehicle(vehicle).unwrap());
    assert_eq!(explosions(&session), 0);
    session
        .world
        .as_mut()
        .unwrap()
        .mission
        .start_doom_countdown(4_000_000);
    let mut ticks = 0;
    while session.world.as_ref().unwrap().mission.byte_18 != Some(1) {
        session
            .tick(TickCallback::Gameplay, 40_000, 40_000, &mut state)
            .unwrap();
        ticks += 1;
        assert!(ticks <= 101, "the countdown never ended");
    }
    assert!(explosions(&session) > 0, "no explosion in the last 4 s");
    let world = session.world.as_ref().unwrap();
    assert_eq!(world.mission.dword_24, Some(3_000_000));
    assert_ne!(
        world.arena.actor(vehicle).map(|actor| actor.word_c4).ok(),
        Some(3),
        "the vehicle was retired"
    );
}
