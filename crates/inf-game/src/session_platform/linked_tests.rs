//! Headless retail-data sessions through the production routing, session
//! effects and CPU draw/present glue. Only the frontend's audio/cue effects
//! and factory music are recorded; screens, registry, input, residency,
//! profile store, settings sidecar and renderer are the production owners,
//! and one renderer serves the whole 2->9->2 run.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use inf_formats::FrontendExecutable;
use sdl2::keyboard::Scancode;

use super::*;

#[path = "linked_tests/required_miss_observer.rs"]
mod required_miss_observer_tests;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_bindings::FrontendDevice,
    frontend_camera::FrontendEntry,
    frontend_display::{frontend_startup, presentation_clear_rgba},
    frontend_renderer::FrontendRendererState,
    frontend_runtime::{read_identity, FrontendRuntimeError, SettingsTextEntryPlatform},
    frontend_scene::{FrontendScene, SceneProjection},
    frontend_screen::FrontendFactoryHost,
    frontend_settings::FrontendSettings,
    frontend_video::{FrontendVideoState, VideoDevice},
    menu::MenuOwner,
    menu_basis::MenuBasisMath,
    menu_text::MenuTextResources,
    model_primitives::PrimitiveTables,
    name_entry::{IdentityInitialsWrite, IdentityTarget, IdentityWriteRequest, LoadedIdentity},
    name_entry_renderer::NameEntryRendererState,
    player_identity::PlayerIdentity,
    render_resources::RenderResources,
    scheduler::SchedulerActivity,
    session::{
        ScreenFactory, ScreenKind, SessionDriver, SessionError, SessionFrameHost, SessionHost,
        SessionPhase, SessionTransitionAdmission, TickCallback, TransitionEffect,
    },
    session_platform::{
        software::{
            present_linked, present_linked_clear, FrontendDrawParts, LinkedDrawOutcome,
            PortDrawPlatform, SoftwareLinkedError, SoftwareLinkedFrame,
        },
        LinkedActivityError, PortSession, PortSessionError, PortSessionErrorOf,
        PortSessionServices, PortSplit, SessionMusicRequest,
    },
    software_frame::{SoftwareFrame, SoftwareSurface},
    software_scene::{SoftwareScene, SoftwareSceneRequest},
    text_entry_screen::TextEntryFactoryHost,
    title_screen::{TitleHost, TitleVideoHost},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Effect {
    SelectFrontendMusic,
    PauseMusic,
    ResumeMusic,
    PersistSettings,
    Sound(i16),
    TransitionFlag,
    ProfileFailed(String),
    SessionMusic(SessionMusicRequest),
}

/// CPU fixture for the briefing's canonical resident cues. The real
/// platform_audio resolver and its native_alias_pitch_gain_and_random_consumption
/// test establish PCM slot 2 consumes none and alias slot 5 consumes one
/// step per play. Confirm emits slot 5 twice, consuming two steps in total.
fn record_briefing_cue(effects: &mut Vec<Effect>, slot: u32, rng: &mut u32) -> Result<(), String> {
    match slot {
        2 => {}
        5 => {
            crate::name_entry::name_entry_rng_next(rng);
        }
        _ => {
            return Err(format!(
                "CPU briefing cue fixture has no owner for slot {slot}"
            ))
        }
    }
    effects.push(Effect::Sound(slot as i16));
    Ok(())
}

struct TestPlatform {
    loading_mode: std::cell::Cell<u8>,
    settings: FrontendSettings,
    video: FrontendVideoState,
    profiles: ProfileStore,
    keyboard: Option<SdlInput>,
    keyboard_attaches: usize,
    enumerated: Vec<DeviceId>,
    random_state: u32,
    identity: PlayerIdentity,
    exit_requested: bool,
    effects: Vec<Effect>,
}

impl TestPlatform {
    fn keyboard(&mut self, input: &mut InputRuntime) -> Result<Vec<FrontendDevice>, String> {
        if self.keyboard.is_none() {
            self.keyboard = Some(SdlInput::attach(input).map_err(|error| error.to_string())?);
            self.keyboard_attaches += 1;
        }
        let device = self.keyboard.as_ref().expect("attached").frontend_device();
        self.enumerated.push(device.source);
        Ok(vec![device])
    }
}

impl crate::gameplay_screen::GameplayTerrainSettings for TestPlatform {
    fn terrain_detail_setting(
        &mut self,
    ) -> Result<u32, crate::terrain_runtime::TerrainPolicyError> {
        Ok(self.settings.words()[0x70 / 4])
    }
}

impl FrontendFactoryHost for TestPlatform {
    type Error = String;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.effects.push(Effect::PauseMusic);
        Ok(())
    }
    fn resume_music(&mut self) -> Result<(), Self::Error> {
        self.effects.push(Effect::ResumeMusic);
        Ok(())
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.keyboard(input)
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.settings.persist().map_err(|error| error.to_string())?;
        self.effects.push(Effect::PersistSettings);
        Ok(())
    }
}

impl TextEntryFactoryHost for TestPlatform {
    type Error = String;
    fn read_identity(
        &mut self,
        phase: SessionPhase,
        slot: u8,
    ) -> Result<LoadedIdentity, Self::Error> {
        read_identity(&mut self.identity, &self.settings, phase, slot)
            .map_err(|error| error.to_string())
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.keyboard(input)
    }
}

impl PortSessionServices for TestPlatform {
    type Error = String;
    type IdentityError = FrontendRuntimeError;
    fn persist_identity(
        &mut self,
        request: IdentityWriteRequest<'_>,
    ) -> Result<(), Self::IdentityError> {
        TextEntryPlatform::persist_identity(
            &mut SettingsTextEntryPlatform {
                settings: &mut self.settings,
                identity: &mut self.identity,
                exit_requested: &mut self.exit_requested,
            },
            request,
        )
    }
    fn profile_store(&mut self) -> Option<&mut ProfileStore> {
        Some(&mut self.profiles)
    }
    fn sound_voice_teardown(&mut self) -> crate::process_sound::ProcessSoundTeardownLoan<'_> {
        crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach
    }
    fn world_tick_inputs(&self) -> crate::session_platform::WorldTickPlatformInputs {
        crate::session_platform::WorldTickPlatformInputs {
            camera_settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            settings_20: 0,
            settings_70: self.settings.words()[0x70 / 4],
            settings_78: 0,
            settings_7c: 0,
            input_mode_08: 0,
            settings_84: 0,
            vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
        }
    }
    fn loading_platform(&mut self) -> crate::loading_integration::LoadingPlatformLoans<'_> {
        crate::loading_integration::LoadingPlatformLoans {
            voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
            mode: &self.loading_mode,
            rng: &mut self.random_state,
        }
    }
    fn process_rng(&mut self) -> &mut u32 {
        &mut self.random_state
    }
    fn session_music(&mut self, request: SessionMusicRequest) -> Result<(), Self::Error> {
        if request == (SessionMusicRequest::Select { native_argument: 7 }) {
            self.effects.push(Effect::SelectFrontendMusic);
        } else {
            self.effects.push(Effect::SessionMusic(request));
        }
        Ok(())
    }
    fn reset_input_baselines(&mut self, input: &mut InputRuntime) -> Result<(), Self::Error> {
        input.reset_baselines_and_discard();
        Ok(())
    }
    fn startup_settings(&self) -> crate::session_platform::StartupSettings {
        crate::session_platform::StartupSettings::from_settings(&self.settings)
    }
    fn key_bindings(&self) -> crate::key_bindings::KeyBindings {
        self.settings.key_bindings().clone()
    }
}

/// Stateless absent title-video backend for the linked title callbacks.
/// Production uses its own unit backend for the same reason: the poll owns
/// the keyboard adapter beside the exit flag, so the linked path cannot also
/// borrow the whole platform for video.
struct AbsentVideo {
    opens: usize,
}

impl TitleVideoHost for AbsentVideo {
    type Error = String;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        self.opens += 1;
        Ok(None)
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        Err("title video presentation is absent".into())
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        Err("title video close is absent".into())
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl TitleVideoHost for TestPlatform {
    type Error = String;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(None)
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        Err("title video presentation is absent".into())
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        Err("title video close is absent".into())
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl TitleHost for TestPlatform {
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.keyboard(input)
    }
}

impl PortDrawPlatform for TestPlatform {
    fn world_draw_parts(&mut self) -> crate::session_platform::software::WorldDrawParts<'_> {
        crate::session_platform::software::WorldDrawParts {
            settings: &self.settings,
            audio: None,
            video: None,
            network: None,
        }
    }

    fn frontend_draw_parts(&mut self) -> FrontendDrawParts<'_> {
        FrontendDrawParts {
            profiles: &mut self.profiles,
            settings: &self.settings,
            identity: &mut self.identity,
            video: &mut self.video,
        }
    }
}

/// Recorded frontend effects. The session must intercept its own requests.
struct TestEffects<'a> {
    loading_mode: &'a std::cell::Cell<u8>,
    effects: &'a mut Vec<Effect>,
    exit_requested: &'a mut bool,
    settings: &'a mut FrontendSettings,
}

impl FrontendEffects for TestEffects<'_> {
    type Error = String;
    fn service_music(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error> {
        self.effects.push(Effect::Sound(resource));
        Ok(())
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.settings.persist().map_err(|error| error.to_string())?;
        self.effects.push(Effect::PersistSettings);
        Ok(())
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        self.loading_mode.set(1);
        self.effects.push(Effect::TransitionFlag);
        Ok(())
    }
    fn profile_operation_failed(
        &mut self,
        operation: ProfileOperation,
        error: &ProfileStoreError,
    ) -> Result<(), Self::Error> {
        self.effects
            .push(Effect::ProfileFailed(format!("{operation:?}: {error}")));
        Ok(())
    }
    fn open_network_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        Err("network menu is outside this scenario".into())
    }
    fn adjust_setting(&mut self, _: u32, _: MenuCommand, _: u32) -> Result<(), Self::Error> {
        Err("settings rows are outside this scenario".into())
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        Err("settings rows are outside this scenario".into())
    }
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        Err("settings rows are outside this scenario".into())
    }
    fn adjust_display_setting(
        &mut self,
        _: u32,
        _: MenuCommand,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("display rows are outside this scenario".into())
    }
    fn apply_volume(&mut self, _: VolumeChannel) -> Result<(), Self::Error> {
        Err("volume rows are outside this scenario".into())
    }
    fn request_phase(&mut self, _: u32) -> Result<(), Self::Error> {
        Err("session-owned phase request reached the platform".into())
    }
    fn set_attract_flag(&mut self, _: bool) -> Result<(), Self::Error> {
        Err("session-owned attract flag reached the platform".into())
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        *self.exit_requested = true;
        Ok(())
    }
    fn external_action(
        &mut self,
        _: MenuActionRequest,
        _: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        Err("external actions are outside this scenario".into())
    }
}

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("session: {0}")]
    Session(PortSessionErrorOf<TestPlatform>),
    #[error("linked callback: {0}")]
    Linked(PortLinkedError<String, FrontendRuntimeError>),
    #[error("linked title callback: {0}")]
    TitleLinked(TitleLinkedError<String>),
    #[error("linked startup callback: {0}")]
    StartupLinked(crate::session_platform::StartupLinkedError),
    #[error("linked key configuration callback: {0}")]
    KeyConfigLinked(crate::session_platform::KeyConfigLinkedError),
    #[error("briefing tick: {0}")]
    Briefing(String),
    #[error("gameplay tick: {0}")]
    Gameplay(String),
    #[error("intro results callback: {0}")]
    IntroResults(String),
    #[error("linked screen: {0}")]
    Screen(LinkedScreenError),
    #[error("draw: {0}")]
    Draw(SoftwareLinkedError<String>),
    #[error("activity: {0}")]
    Activity(LinkedActivityError),
    #[error("test frame has no renderer")]
    MissingRenderer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Presented {
    screen: PortScreen,
    hash: u64,
    /// The surface after 0050D960, i.e. the clear colour 00438120 chose.
    cleared: [u8; 4],
}

struct CpuRender<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    frontend_scene: &'a FrontendScene,
    sine: &'a [u16; 4096],
    template: SceneProjection,
    frontend_state: &'a mut FrontendRendererState,
    name_state: &'a mut NameEntryRendererState,
    presented: &'a mut Vec<Presented>,
}

struct CpuFrame<'a, 'f, 'r> {
    session: &'a mut PortSession<TestPlatform>,
    basis: &'a MenuBasisMath,
    notifications: &'a mut Vec<InputNotification>,
    now_micros: u64,
    completions: &'a mut Vec<NameCompletion>,
    render: Option<CpuRender<'a, 'f, 'r>>,
}

#[derive(Clone, Copy)]
enum Call {
    Poll,
    Tick(SchedulerTick),
}

impl CpuFrame<'_, '_, '_> {
    fn run(&mut self, state: &mut SessionState, call: Call) -> Result<(), TestError> {
        let screen = self.session.linked_screen().map_err(TestError::Screen)?;
        let PortSplit {
            mut linked,
            residency,
            platform,
            world,
            sounds,
        } = self.session.split();
        let TestPlatform {
            loading_mode,
            settings,
            profiles,
            keyboard,
            random_state,
            identity,
            exit_requested,
            effects,
            ..
        } = platform;
        let adapter = keyboard
            .as_ref()
            .map(|keyboard| keyboard.frontend_device().source);
        let pass = PortInputPass {
            keyboard: keyboard.as_mut(),
            notifications: &mut *self.notifications,
            now_micros: self.now_micros,
        };
        let completions = &mut *self.completions;
        let result = match screen {
            PortScreen::KeyConfig => {
                let process = linked.process;
                let owner_byte = linked.owner_byte;
                let mut key_config = crate::session_platform::KeyConfigSessionEffects {
                    settings,
                    state: &mut *state,
                    process,
                    owner_byte,
                    basis: self.basis,
                };
                match call {
                    Call::Poll => linked.poll_key_config(pass, &mut key_config, exit_requested),
                    Call::Tick(tick) => linked.tick_key_config(tick, adapter, &mut key_config),
                }
                .map_err(TestError::KeyConfigLinked)
            }
            PortScreen::Startup => {
                let mut startup = crate::session_platform::StartupSessionEffects {
                    settings,
                    residency,
                    state: &mut *state,
                };
                match call {
                    Call::Poll => linked.poll_startup(pass, &mut startup),
                    Call::Tick(tick) => linked.tick_startup(tick, &mut startup),
                }
                .map_err(TestError::StartupLinked)
            }
            PortScreen::Title => {
                // The linked title borrows the keyboard adapter for
                // notifications, the exit flag beside it, and the stateless
                // absent video backend, mirroring production.
                let mut video = AbsentVideo { opens: 0 };
                match call {
                    Call::Poll => linked
                        .poll_title(state, pass, &mut video, exit_requested)
                        .map_err(TestError::TitleLinked),
                    Call::Tick(tick) => linked
                        .tick_title(state, tick, &mut video)
                        .map_err(TestError::TitleLinked)
                        .map(|_| ()),
                }
            }
            PortScreen::Frontend => {
                let mut effects = TestEffects {
                    loading_mode,
                    effects,
                    exit_requested,
                    settings,
                };
                let services: LinkedServices<'_, _, SettingsTextEntryPlatform<'_>> =
                    LinkedServices::Frontend {
                        effects: &mut effects,
                        profiles,
                        basis: self.basis,
                    };
                match call {
                    Call::Poll => linked.poll(state, pass, services, completions),
                    Call::Tick(tick) => linked.tick(state, tick, services, completions),
                }
                .map_err(TestError::Linked)
            }
            PortScreen::TextEntry => {
                let mut text = SettingsTextEntryPlatform {
                    settings,
                    identity,
                    exit_requested,
                };
                let services: LinkedServices<'_, TestEffects<'_>, _> = LinkedServices::TextEntry {
                    platform: &mut text,
                    profiles,
                    rng: random_state,
                    keyboard: adapter,
                };
                match call {
                    Call::Poll => linked.poll(state, pass, services, completions),
                    Call::Tick(tick) => linked.tick(state, tick, services, completions),
                }
                .map_err(TestError::Linked)
            }
            PortScreen::Briefing => match call {
                Call::Poll => {
                    let cues = linked
                        .poll_briefing(pass, exit_requested)
                        .map_err(|error| TestError::Briefing(error.to_string()))?;
                    let rng = &mut world.as_mut().expect("briefing world retained").rng;
                    for cue in cues {
                        record_briefing_cue(effects, cue, rng).map_err(TestError::Briefing)?;
                    }
                    Ok(())
                }
                Call::Tick(tick) => {
                    let world = world.as_mut().expect("briefing world retained");
                    let outcome = linked
                        .tick_briefing(state, tick, world, &mut |request, rng| match request {
                            crate::briefing_screen::BriefingAudioRequest::ResumeMusic => {
                                effects.push(Effect::ResumeMusic);
                                Ok(())
                            }
                            crate::briefing_screen::BriefingAudioRequest::Cue { slot } => {
                                record_briefing_cue(effects, slot, rng)
                            }
                        })
                        .map_err(|error| TestError::Briefing(error.to_string()))?;
                    for effect in outcome.effects {
                        if let crate::briefing_runtime::BriefingTickEffect::PlayCue { cue } = effect
                        {
                            record_briefing_cue(effects, cue, &mut world.rng)
                                .map_err(TestError::Briefing)?;
                        }
                    }
                    Ok(())
                }
            },
            PortScreen::Gameplay => match call {
                Call::Poll => linked
                    .poll_gameplay(
                        state,
                        pass,
                        world.as_mut().expect("gameplay world retained"),
                        sounds,
                        exit_requested,
                        &mut [0; crate::frontend_settings::SETTINGS_WORDS],
                        0,
                    )
                    .map(|_| ())
                    .map_err(|error| TestError::Gameplay(error.to_string())),
                Call::Tick(tick) => linked
                    .tick_gameplay(state, tick, world.as_mut())
                    .map_err(|error| TestError::Gameplay(error.to_string())),
            },
            PortScreen::IntroResults => match call {
                Call::Poll => linked
                    .poll_intro_results(pass, exit_requested, None)
                    .map(|_| ())
                    .map_err(|error| TestError::IntroResults(error.to_string())),
                Call::Tick(tick) => linked
                    .tick_intro_results(
                        state,
                        tick,
                        world.as_mut().expect("results world retained"),
                        None,
                    )
                    .map(|_| ())
                    .map_err(|error| TestError::IntroResults(error.to_string())),
            },
            PortScreen::Map => match call {
                Call::Poll => linked
                    .poll_map(state, pass, exit_requested)
                    .map_err(|error| TestError::Gameplay(error.to_string())),
                Call::Tick(tick) => linked
                    .tick_map(tick)
                    .map_err(|error| TestError::Gameplay(error.to_string())),
            },
        };
        result
    }
}

impl SessionHost for CpuFrame<'_, '_, '_> {
    type Screen = PortScreen;
    type ScreenContext = crate::screen_registry::ScreenAttachment;
    type Error = TestError;
    fn current_screen(&self) -> Option<PortScreen> {
        self.session.current_screen()
    }
    fn screen_kind(
        &mut self,
        screen: PortScreen,
        state: &mut SessionState,
    ) -> Result<ScreenKind, TestError> {
        self.session
            .screen_kind(screen, state)
            .map_err(TestError::Session)
    }
    fn screen_context(&self, screen: PortScreen) -> Self::ScreenContext {
        self.session.screen_context(screen)
    }
    fn reuse_screen(
        &mut self,
        screen: PortScreen,
        requested: SessionPhase,
        state: &mut SessionState,
    ) -> Result<bool, TestError> {
        self.session
            .reuse_screen(screen, requested, state)
            .map_err(TestError::Session)
    }
    fn destroy_screen(
        &mut self,
        screen: PortScreen,
        state: &mut SessionState,
    ) -> Result<(), TestError> {
        self.session
            .destroy_screen(screen, state)
            .map_err(TestError::Session)
    }
    fn network_session_active(&self) -> bool {
        SessionHost::network_session_active(&*self.session)
    }
    fn transition_effect(
        &mut self,
        effect: TransitionEffect,
        state: &mut SessionState,
    ) -> Result<(), TestError> {
        self.session
            .transition_effect(effect, state)
            .map_err(TestError::Session)
    }
    fn create_screen(
        &mut self,
        factory: ScreenFactory,
        context: Self::ScreenContext,
        state: &mut SessionState,
    ) -> Result<(), TestError> {
        self.session
            .create_screen(factory, context, state)
            .map_err(TestError::Session)
    }
    fn tick(
        &mut self,
        callback: TickCallback,
        delta: i32,
        elapsed: i32,
        state: &mut SessionState,
    ) -> Result<(), TestError> {
        SessionHost::tick(&mut *self.session, callback, delta, elapsed, state)
            .map_err(TestError::Session)
    }
}

impl SessionFrameHost for CpuFrame<'_, '_, '_> {
    fn input_is_selected(&self) -> bool {
        self.session.input_is_selected()
    }
    fn poll_input(&mut self, state: &mut SessionState) -> Result<(), TestError> {
        self.run(state, Call::Poll)
    }
    fn admit_transition(
        &mut self,
        state: &SessionState,
    ) -> Result<SessionTransitionAdmission, TestError> {
        self.session
            .admit_frame_transition(state)
            .map_err(|boundary| TestError::Session(PortSessionError::Boundary(boundary)))
    }
    fn linked_tick(
        &mut self,
        tick: SchedulerTick,
        state: &mut SessionState,
    ) -> Result<(), TestError> {
        self.run(state, Call::Tick(tick))
    }
    fn linked_callback_08(&mut self, state: &mut SessionState) -> Result<(), TestError> {
        let render = self.render.as_mut().ok_or(TestError::MissingRenderer)?;
        let outcome = SoftwareLinkedFrame {
            scene: &mut *render.scene,
            resources: render.resources,
            database_resources: None,
            frontend_scene: render.frontend_scene,
            sine_quarter: render.sine,
            template_projection: render.template,
            frontend_state: &mut *render.frontend_state,
            name_entry_state: &mut *render.name_state,
            surface: SoftwareSurface::Pixels,
        }
        .draw(self.session, state)
        .map_err(TestError::Draw)?;
        assert!(!matches!(
            outcome,
            LinkedDrawOutcome::Frontend(
                crate::frontend_frame::FrontendFrameOutcome::RecoveredNative301
            )
        ));
        Ok(())
    }
    fn linked_callback_0c(&mut self, _: &mut SessionState) -> Result<(), TestError> {
        // Title presentation 00404840 is empty: it never presents or clears,
        // mirroring production.
        let screen = self.session.linked_screen().map_err(TestError::Screen)?;
        if screen == PortScreen::Title {
            return Ok(());
        }
        // 00433620: the startup clears with its own +1A8 word.
        let startup_clear = match self.session.startup_screen() {
            Some(startup) if screen == PortScreen::Startup => Some(
                crate::frontend_display::presentation_word_rgba(startup.state().clear_color),
            ),
            _ => None,
        };
        let forced_black = self.session.gameplay_forces_black();
        let render = self.render.as_mut().ok_or(TestError::MissingRenderer)?;
        let mut hash = None;
        let present = |frame: &mut SoftwareFrame| {
            let mut hasher = DefaultHasher::new();
            frame.rgba().hash(&mut hasher);
            hash = Some(hasher.finish());
            Ok::<(), String>(())
        };
        match startup_clear {
            Some(clear) => present_linked_clear(&mut *render.scene, clear, present),
            None => present_linked(
                &mut *render.scene,
                render.resources,
                if screen == PortScreen::Gameplay {
                    crate::frontend_display::PresentationClear::Gameplay
                } else {
                    crate::frontend_display::PresentationClear::Standard
                },
                forced_black,
                present,
            ),
        }
        .map_err(TestError::Draw)?;
        let cleared: [u8; 4] = render.scene.pixels.frame.rgba()[..4].try_into().unwrap();
        render.presented.push(Presented {
            screen,
            hash: hash.expect("presented"),
            cleared,
        });
        Ok(())
    }
    fn linked_activity(
        &mut self,
        activity: SchedulerActivity,
        _: &mut SessionState,
    ) -> Result<(), TestError> {
        self.session
            .linked_activity(activity)
            .map_err(TestError::Activity)
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Ctx {
    session: PortSession<TestPlatform>,
    driver: SessionDriver,
    now: u64,
    frames: u64,
    notifications: Vec<InputNotification>,
    completions: Vec<NameCompletion>,
    presented: Vec<Presented>,
    frontend_state: FrontendRendererState,
    name_state: NameEntryRendererState,
    frontend_scene: FrontendScene,
    basis: MenuBasisMath,
    sine: [u16; 4096],
    template: SceneProjection,
    clear: [u8; 4],
    epoch: (u64, u64),
    directory: PathBuf,
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

/// The app's startup order: settings, residency Select/Boot, the display's
/// startup values, the session, then direct entry into phase 2.
fn setup(
    label: &str,
) -> (
    Ctx,
    PrimitiveTables,
    crate::frontend_app::FrontendAppStartup,
) {
    setup_inner(label, false, None)
}

/// As [`setup`], but booting through the real title factory instead of
/// direct frontend entry.
fn setup_title(
    label: &str,
) -> (
    Ctx,
    PrimitiveTables,
    crate::frontend_app::FrontendAppStartup,
) {
    setup_inner(label, true, None)
}

#[derive(Clone, Copy)]
enum InitialProfileMode {
    ImportRetail,
    EmptyPortable,
}

fn setup_inner(
    label: &str,
    title: bool,
    mission_override: Option<u32>,
) -> (
    Ctx,
    PrimitiveTables,
    crate::frontend_app::FrontendAppStartup,
) {
    setup_inner_with_profiles(
        label,
        title,
        mission_override,
        InitialProfileMode::ImportRetail,
    )
}

fn setup_inner_with_profiles(
    label: &str,
    title: bool,
    mission_override: Option<u32>,
    profiles: InitialProfileMode,
) -> (
    Ctx,
    PrimitiveTables,
    crate::frontend_app::FrontendAppStartup,
) {
    let root = root();
    let directory = root.join(format!(
        "../.tmp/inf-session-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let installation = Installation::open(&root).unwrap();
    let language = installation.preload().header.language_flag;
    let mut settings =
        FrontendSettings::load_or_defaults(directory.join("settings.json"), &assets).unwrap();
    crate::frontend_video::prepare_video_startup(settings.words_mut());
    settings.select_presentation(2, language).unwrap();
    let video = FrontendVideoState::new(
        vec![VideoDevice {
            id: 0,
            name: b"Test Display".to_vec(),
        }],
        settings.current_display(),
    )
    .unwrap();
    if matches!(profiles, InitialProfileMode::EmptyPortable) {
        // An explicit test input, never an empty-slice behavioral sentinel.
        // ProfileStore treats this portable sidecar as authoritative; retail
        // Infest.sav and all overlays remain immutable.
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("profiles.sav"), []).unwrap();
    }
    let profiles =
        ProfileStore::open_copy(root.join("Infest.sav"), directory.join("profiles.sav")).unwrap();
    let mut runtime = ResidentRuntime::new(installation).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let mut startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    crate::frontend_video::apply_video_pipeline(settings.words(), &mut startup.scene).unwrap();
    let frontend_scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let basis = MenuBasisMath::new(assets.sine_table().values);
    let sine = assets.sine_table().values;
    let mut session = PortSession::new(
        assets,
        runtime,
        TestPlatform {
            loading_mode: std::cell::Cell::new(0),
            settings,
            video,
            profiles,
            keyboard: None,
            keyboard_attaches: 0,
            enumerated: Vec::new(),
            random_state: 0,
            identity: PlayerIdentity::default(),
            exit_requested: false,
            effects: Vec::new(),
        },
        FrontendRenderContext {
            projection: startup.projection,
        },
        mission_override,
    )
    .unwrap();
    let mut driver = SessionDriver::new(0);
    let attachment = session.attachment();
    if title {
        session.boot_title().unwrap();
    } else {
        driver
            .enter_frontend_directly(&mut session, attachment)
            .unwrap();
    }
    let epoch = (
        session.residency_generation(),
        session.residency().revision(),
    );
    let ctx = Ctx {
        session,
        driver,
        now: 0,
        frames: 0,
        notifications: Vec::new(),
        completions: Vec::new(),
        presented: Vec::new(),
        frontend_state: FrontendRendererState::default(),
        name_state: NameEntryRendererState::default(),
        frontend_scene,
        basis,
        sine,
        template: startup.projection,
        clear: startup.clear_rgba,
        epoch,
        directory,
    };
    (ctx, tables, startup)
}

/// One admitted 125 ms scheduler iteration with the given notifications.
fn step<'f, 'r>(
    ctx: &mut Ctx,
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    keys: &[(&str, bool)],
) -> Result<(), SessionError<TestError>> {
    for &(key, pressed) in keys {
        ctx.notifications.push(InputNotification::Key {
            scancode: Scancode::from_name(key).expect("scancode"),
            pressed,
        });
    }
    ctx.now += 125_000;
    macro_rules! frame {
        ($render:expr) => {
            CpuFrame {
                session: &mut ctx.session,
                basis: &ctx.basis,
                notifications: &mut ctx.notifications,
                now_micros: ctx.now,
                completions: &mut ctx.completions,
                render: $render,
            }
        };
    }
    assert!(ctx.driver.first_pass(ctx.now, &mut frame!(None))?.is_some());
    ctx.driver.linked_update(&mut frame!(None))?;
    // World loads (9→3) change residency (`48`, then `selection+10`); other
    // transitions leave it intact. Update the epoch so later draws use the
    // current lineage; the 2→9→2 round trip still never rebounds (covered by
    // its own presentation-clear checks).
    ctx.epoch = (
        ctx.session.residency_generation(),
        ctx.session.residency().revision(),
    );
    // Mirror the interactive renderer epoch handoff after update: world loads
    // supply current backing; frontend-only phases clear it. Table selection
    // remains owned by the native installer, independently of availability.
    scene.set_terrain(ctx.session.world().map(|world| world.terrain.clone()));
    let render = CpuRender {
        scene,
        resources,
        frontend_scene: &ctx.frontend_scene,
        sine: &ctx.sine,
        template: ctx.template,
        frontend_state: &mut ctx.frontend_state,
        name_state: &mut ctx.name_state,
        presented: &mut ctx.presented,
    };
    ctx.driver.linked_present(&mut frame!(Some(render)))?;
    ctx.frames += 1;
    Ok(())
}

/// One iteration as the production loop runs it across residency changes:
/// the update first, then a renderer bound to the residency current after
/// it. Each call starts a fresh CPU scene from the startup configuration.
fn step_rebinding(
    ctx: &mut Ctx,
    tables: &PrimitiveTables,
    startup: &crate::frontend_app::FrontendAppStartup,
    keys: &[(&str, bool)],
) -> Result<(), SessionError<TestError>> {
    for &(key, pressed) in keys {
        ctx.notifications.push(InputNotification::Key {
            scancode: Scancode::from_name(key).expect("scancode"),
            pressed,
        });
    }
    ctx.now += 125_000;
    {
        let mut frame = CpuFrame {
            session: &mut ctx.session,
            basis: &ctx.basis,
            notifications: &mut ctx.notifications,
            now_micros: ctx.now,
            completions: &mut ctx.completions,
            render: None,
        };
        assert!(ctx.driver.first_pass(ctx.now, &mut frame)?.is_some());
        ctx.driver.linked_update(&mut frame)?;
    }
    ctx.epoch = (
        ctx.session.residency_generation(),
        ctx.session.residency().revision(),
    );
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut pixels = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    {
        let mut frame = CpuFrame {
            session: &mut ctx.session,
            basis: &ctx.basis,
            notifications: &mut ctx.notifications,
            now_micros: ctx.now,
            completions: &mut ctx.completions,
            render: Some(CpuRender {
                scene: &mut scene,
                resources,
                frontend_scene: &ctx.frontend_scene,
                sine: &ctx.sine,
                template: ctx.template,
                frontend_state: &mut ctx.frontend_state,
                name_state: &mut ctx.name_state,
                presented: &mut ctx.presented,
            }),
        };
        ctx.driver.linked_present(&mut frame)?;
    }
    ctx.frames += 1;
    Ok(())
}

fn press<'f, 'r>(
    ctx: &mut Ctx,
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    key: &str,
) {
    step(ctx, scene, resources, &[(key, true)]).unwrap();
    step(ctx, scene, resources, &[(key, false)]).unwrap();
}

fn top_profile(ctx: &Ctx) -> Option<&crate::profile_menu::ProfileMenu> {
    match ctx
        .session
        .frontend_screen()?
        .control()?
        .menus()
        .current_owner()?
    {
        MenuOwner::Profile(profile) => Some(profile),
        MenuOwner::Authored(_) | MenuOwner::Network(_) => None,
    }
}

/// Start Game, then the first empty profile slot. Returns the scheduler
/// iterations between that activation and the deferred phase-9 request.
fn enter_name_entry<'f, 'r>(
    ctx: &mut Ctx,
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
) -> usize {
    for _ in 0..100 {
        step(ctx, scene, resources, &[]).unwrap();
    }
    enter_name_entry_from_root(ctx, scene, resources)
}

/// As [`enter_name_entry`], once the root menu is settled on Start Game.
fn enter_name_entry_from_root<'f, 'r>(
    ctx: &mut Ctx,
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
) -> usize {
    press(ctx, scene, resources, "Return");
    for _ in 0..80 {
        if top_profile(ctx).is_some() {
            break;
        }
        step(ctx, scene, resources, &[]).unwrap();
    }
    assert!(
        top_profile(ctx).is_some(),
        "Start Game opens the derived profile menu"
    );
    // The deferred Start Game path pushes the menu; no phase was requested.
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Frontend);
    assert!(!ctx.driver.state().transition_pending());
    for _ in 0..40 {
        step(ctx, scene, resources, &[]).unwrap();
    }
    let count = ctx.session.platform().profiles.cached_count() as i32;
    for _ in 0..16 {
        if top_profile(ctx).unwrap().base().selection() == count + 1 {
            break;
        }
        press(ctx, scene, resources, "Down");
        step(ctx, scene, resources, &[]).unwrap();
    }
    assert_eq!(top_profile(ctx).unwrap().base().selection(), count + 1);
    let transition_flags = |ctx: &Ctx| {
        ctx.session
            .platform()
            .effects
            .iter()
            .filter(|effect| **effect == Effect::TransitionFlag)
            .count()
    };
    let flags = transition_flags(ctx);
    press(ctx, scene, resources, "Return");
    assert_eq!(
        transition_flags(ctx),
        flags + 1,
        "0054694C at the empty-slot action"
    );
    let mut waited = 0;
    while !ctx.driver.state().transition_pending() {
        assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Frontend);
        step(ctx, scene, resources, &[]).unwrap();
        waited += 1;
        assert!(waited < 20, "operation 2 must request phase 9");
    }
    assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Phase9);
    step(ctx, scene, resources, &[]).unwrap();
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase9);
    waited
}

#[test]
fn boot_title_reaches_frontend_through_tick_and_transition() {
    let (mut ctx, tables, startup) = setup_title("title");
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Title);
    assert!(ctx.session.title_screen().is_some());
    assert!(ctx.session.frontend_screen().is_none());
    // The absent video makes no phase request from the factory itself.
    assert!(!ctx.driver.state().transition_pending());
    assert!(ctx.session.input_is_selected());

    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();

    // First iteration: the title tick finishes (video absent) and requests
    // phase 2. Its draw runs only the arena preamble and its empty
    // presentation presents nothing.
    step(&mut ctx, &mut scene, resources, &[]).unwrap();
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Title);
    assert!(ctx.driver.state().transition_pending());
    assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Frontend);
    assert!(ctx.session.title_screen().unwrap().finished());
    assert!(ctx.presented.is_empty());

    // Second iteration, driven manually: the Title->Frontend transition is
    // the first activation of leaf 47, so the residency revision advances
    // past the boot epoch. Production rebinds its renderer here; the test
    // re-snapshots resources and the scene the same way before presenting.
    ctx.now += 125_000;
    {
        let mut frame = CpuFrame {
            session: &mut ctx.session,
            basis: &ctx.basis,
            notifications: &mut ctx.notifications,
            now_micros: ctx.now,
            completions: &mut ctx.completions,
            render: None,
        };
        assert!(ctx
            .driver
            .first_pass(ctx.now, &mut frame)
            .expect("title-to-frontend first pass")
            .is_some());
        ctx.driver
            .linked_update(&mut frame)
            .expect("title-to-frontend linked update");
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Frontend);
    assert!(ctx.session.title_screen().is_none());
    assert!(ctx.session.frontend_screen().is_some());
    ctx.epoch = (
        ctx.session.residency_generation(),
        ctx.session.residency().revision(),
    );
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    {
        let mut frame = CpuFrame {
            session: &mut ctx.session,
            basis: &ctx.basis,
            notifications: &mut ctx.notifications,
            now_micros: ctx.now,
            completions: &mut ctx.completions,
            render: Some(CpuRender {
                scene: &mut scene,
                resources,
                frontend_scene: &ctx.frontend_scene,
                sine: &ctx.sine,
                template: ctx.template,
                frontend_state: &mut ctx.frontend_state,
                name_state: &mut ctx.name_state,
                presented: &mut ctx.presented,
            }),
        };
        ctx.driver
            .linked_present(&mut frame)
            .expect("title-to-frontend linked present");
    }
    ctx.frames += 1;
    assert_eq!(ctx.presented.len(), 1);
    assert_eq!(ctx.presented[0].screen, PortScreen::Frontend);
    let transition = ctx.driver.transitions().last().expect("transition");
    assert_eq!(transition.previous, SessionPhase::Title);
    assert_eq!(transition.requested, SessionPhase::Frontend);
    assert_eq!(transition.created, Some(ScreenFactory::Frontend));
    assert_eq!(
        ctx.session.platform().effects[..3],
        [
            Effect::SelectFrontendMusic,
            Effect::PauseMusic,
            Effect::ResumeMusic
        ],
        "00487360 precedes the frontend factory"
    );
    // One keyboard adapter serves both screens; the title enumerated first.
    assert_eq!(ctx.session.platform().keyboard_attaches, 1);
    assert!(ctx.session.input_is_selected());
}

#[test]
fn options_language_turns_accepts_and_returns_through_the_title() {
    let (mut ctx, tables, startup) = setup_title("language");
    let press_key = |ctx: &mut Ctx, key| {
        step_rebinding(ctx, &tables, &startup, &[(key, true)]).unwrap();
        step_rebinding(ctx, &tables, &startup, &[(key, false)]).unwrap();
    };
    let idle = |ctx: &mut Ctx, steps| {
        for _ in 0..steps {
            step_rebinding(ctx, &tables, &startup, &[]).unwrap();
        }
    };
    idle(&mut ctx, 100);
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Frontend);
    // Root Options, then its fourth row: Language (00453C70 requests 0).
    press_key(&mut ctx, "Down");
    idle(&mut ctx, 20);
    press_key(&mut ctx, "Return");
    idle(&mut ctx, 40);
    for _ in 0..3 {
        press_key(&mut ctx, "Down");
        idle(&mut ctx, 20);
    }
    press_key(&mut ctx, "Return");
    for _ in 0..20 {
        if ctx.session.startup_screen().is_some() {
            break;
        }
        idle(&mut ctx, 1);
    }
    let transition = *ctx.driver.transitions().last().unwrap();
    assert_eq!(
        (
            transition.previous,
            transition.requested,
            transition.created
        ),
        (
            SessionPhase::Frontend,
            SessionPhase::Startup,
            Some(ScreenFactory::Startup)
        )
    );
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Startup);
    // The constructor's phase-1 request was dropped by the pending
    // transition, the music stopped, and leaf 2 replaced leaf 47.
    assert!(!ctx.driver.state().transition_pending());
    assert!(ctx.session.frontend_screen().is_none());
    assert_eq!(ctx.session.residency().summary().active_leaves, [0, 2]);
    assert_eq!(
        ctx.session.platform().effects.last(),
        Some(&Effect::PauseMusic)
    );
    // The boot stood in for the native startup construction, so the Options
    // visit starts fully faded in. The press iteration's poll requested
    // phase 0 and its transition built the screen before that iteration's
    // tick; the release iteration ticked again.
    let state = *ctx.session.startup_screen().unwrap().state();
    assert_eq!((state.selection, state.age), (0, 2_250_000));
    let presented = *ctx.presented.last().unwrap();
    assert_eq!(presented.screen, PortScreen::Startup);
    assert_eq!(presented.cleared, [66, 66, 255, 255]);

    press_key(&mut ctx, "Right");
    press_key(&mut ctx, "Right");
    assert_eq!(ctx.session.startup_screen().unwrap().state().selection, 2);
    assert!(!ctx.driver.state().transition_pending());
    press_key(&mut ctx, "Return");
    // The accept wrote and reselected language 2, persisted the sidecar
    // (no settings existed at startup) and the title rebuilt the frontend.
    for _ in 0..4 {
        if ctx.session.frontend_screen().is_some() {
            break;
        }
        idle(&mut ctx, 1);
    }
    let phases: Vec<_> = ctx
        .driver
        .transitions()
        .iter()
        .rev()
        .take(2)
        .map(|transition| (transition.previous, transition.requested))
        .collect();
    assert_eq!(
        phases,
        [
            (SessionPhase::Title, SessionPhase::Frontend),
            (SessionPhase::Startup, SessionPhase::Title),
        ]
    );
    assert!(ctx.session.startup_screen().is_none());
    assert!(ctx.session.frontend_screen().is_some());
    assert_eq!(ctx.session.platform().settings.language(), 2);
    let summary = ctx.session.residency().summary();
    assert_eq!(summary.selection.language, 2);
    assert_eq!(summary.active_leaves, [0, 47]);
    let saved = FrontendSettings::load_or_defaults(
        ctx.directory.join("settings.json"),
        ctx.session.assets(),
    )
    .unwrap();
    assert_eq!(saved.language(), 2);
    assert!(saved.loaded_at_startup());
}

#[test]
fn options_controls_rebinds_a_key_and_returns_to_a_frontend_that_saves_it() {
    let (mut ctx, tables, startup) = setup_title("key-config");
    let step = |ctx: &mut Ctx, keys: &[(&str, bool)]| {
        step_rebinding(ctx, &tables, &startup, keys).unwrap();
    };
    let press_key = |ctx: &mut Ctx, key| {
        step(ctx, &[(key, true)]);
        step(ctx, &[(key, false)]);
    };
    let idle = |ctx: &mut Ctx, steps| {
        for _ in 0..steps {
            step(ctx, &[]);
        }
    };
    idle(&mut ctx, 100);
    // Root Options, its Controls row, then Controls row 3 (00453C70 with C).
    press_key(&mut ctx, "Down");
    idle(&mut ctx, 20);
    press_key(&mut ctx, "Return");
    idle(&mut ctx, 40);
    for _ in 0..2 {
        press_key(&mut ctx, "Down");
        idle(&mut ctx, 20);
    }
    press_key(&mut ctx, "Return");
    idle(&mut ctx, 40);
    for _ in 0..2 {
        press_key(&mut ctx, "Down");
        idle(&mut ctx, 20);
    }
    press_key(&mut ctx, "Return");
    for _ in 0..20 {
        if ctx.session.key_config_screen().is_some() {
            break;
        }
        idle(&mut ctx, 1);
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase12);
    let screen = ctx.session.key_config_screen().unwrap();
    assert_eq!(screen.return_phase(), SessionPhase::Frontend);
    assert_eq!(ctx.presented.last().unwrap().screen, PortScreen::KeyConfig);
    // Down to the second binding row (Down, action 1), then capture K.
    press_key(&mut ctx, "Down");
    step(&mut ctx, &[("Return", true)]);
    assert!(ctx.session.key_config_screen().unwrap().capture().is_some());
    step(&mut ctx, &[("Return", false)]);
    assert!(
        ctx.session
            .key_config_screen()
            .unwrap()
            .capture()
            .unwrap()
            .armed
    );
    step(&mut ctx, &[("K", true)]);
    assert_eq!(ctx.session.key_config_screen().unwrap().capture(), None);
    step(&mut ctx, &[("K", false)]);
    assert_eq!(
        ctx.session
            .platform()
            .settings
            .key_bindings()
            .record(1)
            .unwrap()
            .codes,
        [0x25, 0, 0, 0]
    );
    press_key(&mut ctx, "Escape");
    for _ in 0..4 {
        if ctx.session.frontend_screen().is_some() {
            break;
        }
        idle(&mut ctx, 1);
    }
    let phases: Vec<_> = ctx
        .driver
        .transitions()
        .iter()
        .rev()
        .take(2)
        .map(|transition| (transition.previous, transition.requested))
        .collect();
    assert_eq!(
        phases,
        [
            (SessionPhase::Phase12, SessionPhase::Frontend),
            (SessionPhase::Frontend, SessionPhase::Phase12),
        ]
    );
    assert!(ctx.session.key_config_screen().is_none());
    // The rebuilt frontend's 00463100 persisted the binding by key name.
    let saved = FrontendSettings::load_or_defaults(
        ctx.directory.join("settings.json"),
        ctx.session.assets(),
    )
    .unwrap();
    assert_eq!(saved.key_bindings().record(1).unwrap().codes[0], 0x25);
}

#[test]
fn repeated_idle_demo_deferral_retains_the_menu_and_allows_new_game() {
    let (mut ctx, tables, startup) = setup("deferred-attract");
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    for _ in 0..100 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    let screen = ctx.session.frontend_screen().unwrap().id().unwrap();
    let registration = ctx.session.registry.screen(screen).unwrap();
    let screens = ctx
        .session
        .registry
        .screens(ctx.session.attachment.session)
        .unwrap()
        .to_vec();
    let scheduler_members = ctx
        .session
        .registry
        .scheduler_members(ctx.session.attachment.session)
        .unwrap()
        .to_vec();
    let bindings = format!("{:?}", ctx.session.frontend_screen().unwrap().bindings());
    let epoch = ctx.epoch;
    let transitions = ctx.driver.transitions().to_vec();
    let effects = ctx.session.platform().effects.clone();
    let settings = *ctx.session.platform().settings.words();
    let working = ctx.session.platform().profiles.working_record().cloned();
    let current_name = ctx
        .session
        .platform()
        .profiles
        .current_name()
        .map(<[u8]>::to_vec);
    let cached_count = ctx.session.platform().profiles.cached_count();
    let settings_bytes = std::fs::read(ctx.directory.join("settings.json")).unwrap();
    let profile_bytes = std::fs::read(ctx.directory.join("profiles.sav")).ok();
    let renderer = scene.id();
    let process = ctx.session.factory_process;

    for _ in 0..3 {
        // A real scheduler gap supplies raw elapsed to the unchanged native
        // root-menu timeout, while animation still receives the capped delta.
        ctx.now += 60_000_000;
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
        assert!(ctx.driver.state().transition_pending());
        assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Phase9);
        assert!(ctx.session.attract);
        assert!(
            ctx.session
                .frontend_screen()
                .unwrap()
                .control()
                .unwrap()
                .idle_millis()
                > 60000
        );
        // Pure native capability still reports the actual unowned demo.
        assert_eq!(
            ctx.session.admit_transition(ctx.driver.state()),
            Err(crate::session_platform::SessionBoundary::AttractDemo)
        );
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
        assert!(!ctx.driver.state().transition_pending());
        assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Frontend);
        assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Frontend);
        assert!(!ctx.session.attract);
        assert_eq!(
            ctx.session
                .frontend_screen()
                .unwrap()
                .control()
                .unwrap()
                .idle_millis(),
            125
        );
        assert_eq!(ctx.session.frontend_screen().unwrap().id(), Some(screen));
        assert_eq!(ctx.session.registry.screen(screen).unwrap(), registration);
        assert_eq!(
            ctx.session
                .registry
                .screens(ctx.session.attachment.session)
                .unwrap(),
            screens
        );
        assert_eq!(
            ctx.session
                .registry
                .scheduler_members(ctx.session.attachment.session)
                .unwrap(),
            scheduler_members
        );
        assert_eq!(ctx.session.registry.selected_screen(), Some(screen));
        assert!(ctx.session.input_is_selected());
        assert_eq!(
            format!("{:?}", ctx.session.frontend_screen().unwrap().bindings()),
            bindings
        );
        assert_eq!(ctx.epoch, epoch);
        assert_eq!(ctx.driver.transitions(), transitions);
        assert_eq!(ctx.session.platform().keyboard_attaches, 1);
        assert_eq!(ctx.session.platform().enumerated.len(), 1);
        assert_eq!(ctx.session.platform().effects, effects);
        assert_eq!(ctx.session.platform().settings.words(), &settings);
        assert_eq!(
            ctx.session.platform().profiles.working_record(),
            working.as_ref()
        );
        assert_eq!(
            ctx.session.platform().profiles.current_name(),
            current_name.as_deref()
        );
        assert_eq!(ctx.session.platform().profiles.cached_count(), cached_count);
        assert!(!ctx.session.platform().profiles.write_attempted());
        assert_eq!(
            std::fs::read(ctx.directory.join("settings.json")).unwrap(),
            settings_bytes
        );
        assert_eq!(
            std::fs::read(ctx.directory.join("profiles.sav")).ok(),
            profile_bytes
        );
        assert_eq!(ctx.session.factory_process, process);
        assert!(!ctx.session.owner_byte());
        assert!(ctx.session.offline_game().is_none());
        assert!(ctx.session.world().is_none());
        assert_eq!(scene.id(), renderer);
        assert_eq!(ctx.presented.last().unwrap().screen, PortScreen::Frontend);
    }
    let selection = ctx
        .session
        .frontend_screen()
        .unwrap()
        .control()
        .unwrap()
        .menus()
        .current()
        .unwrap()
        .selection();
    press(&mut ctx, &mut scene, resources, "Down");
    assert_ne!(
        ctx.session
            .frontend_screen()
            .unwrap()
            .control()
            .unwrap()
            .menus()
            .current()
            .unwrap()
            .selection(),
        selection
    );
    press(&mut ctx, &mut scene, resources, "Up");
    assert_eq!(
        ctx.session
            .frontend_screen()
            .unwrap()
            .control()
            .unwrap()
            .menus()
            .current()
            .unwrap()
            .selection(),
        selection
    );
    enter_name_entry_from_root(&mut ctx, &mut scene, resources);
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase9);
    assert!(ctx.session.text_screen().is_some());
    assert!(ctx.session.offline_game().is_some());
    assert!(ctx.session.owner_byte());
    assert_eq!(ctx.session.platform().keyboard_attaches, 1);
}

#[test]
fn attract_deferral_requires_the_actual_ready_frontend_owner() {
    let (mut ctx, _, _) = setup("attract-without-frontend");
    assert!(ctx.driver.request_phase(SessionPhase::Phase9));
    ctx.session.attract = true;
    // Real owned teardown leaves no screen to retain. Admission must not
    // turn this missing-owner boundary into a deferred successful frame.
    ctx.session.shutdown().unwrap();
    assert!(ctx.session.frontend_screen().is_none());
    let effects = ctx.session.platform().effects.clone();
    assert_eq!(
        ctx.session.admit_frame_transition(ctx.driver.state()),
        Err(crate::session_platform::SessionBoundary::AttractDemo)
    );
    assert!(ctx.driver.state().transition_pending());
    assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Phase9);
    assert!(ctx.session.attract);
    assert_eq!(ctx.session.platform().effects, effects);
}

/// A network session's new game passes the admission; without the
/// platform's link, 00487485 has no network world to take.
#[test]
fn network_new_game_needs_the_platforms_link() {
    let (mut ctx, tables, startup) = setup("network-admission");
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    ctx.session.factory_process.network_flag = 0x100;
    assert!(ctx.driver.request_phase(SessionPhase::Phase9));
    let result = step(&mut ctx, &mut scene, resources, &[]);
    assert!(
        matches!(
            result,
            Err(SessionError::Host(TestError::Session(
                PortSessionError::Prepare(
                    crate::player_prepare::PlayerPrepareError::NetworkMissionSelection {
                        network: 0x100
                    }
                )
            )))
        ),
        "{result:?}"
    );
    assert!(!ctx.session.attract);
}

#[test]
fn start_game_reaches_name_entry_and_back_returns_to_a_fresh_frontend() {
    let (mut ctx, tables, startup) = setup("back");
    let initial_behavior = crate::object_runtime::ProcessActorBehaviorTables::from_executable(
        FrontendExecutable::parse(ctx.session.assets().executable_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(ctx.session.actor_behavior_tables, initial_behavior);
    // A reached cache prefix can include unknown bytes. Screen and display
    // replacement must retain that state instead of remapping executable defaults.
    for address in [0x0053_0e60, 0x0053_dba8, 0x0054_6a94] {
        ctx.session
            .actor_behavior_tables
            .write_process_bytes(address, &[Some(0xa5), None, Some(0x5a)])
            .unwrap();
    }
    let retained_behavior = ctx.session.actor_behavior_tables.clone();
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    let renderer = scene.id();
    assert_eq!(
        ctx.session.platform().effects[..3],
        [
            Effect::SelectFrontendMusic,
            Effect::PauseMusic,
            Effect::ResumeMusic
        ],
        "00487360 precedes the frontend factory"
    );
    // Two 125 ms iterations occur for the press and release; operation 2
    // then waits until its remaining delay is strictly below the delta.
    let waited = enter_name_entry(&mut ctx, &mut scene, resources);
    assert!(
        waited == 7,
        "the 1000 ms operation delay is honoured ({waited})"
    );

    // 2->9 destroyed the frontend and built the text-entry screen over the
    // same keyboard adapter; PreparePhase9 published the offline state.
    assert!(ctx.session.frontend_screen().is_none());
    let entry = ctx.session.text_screen().unwrap().entry().unwrap();
    assert_eq!(entry.name(), b"Jameson", "unset sidecar name reads Jameson");
    // sprintf("%dup", 0055F2F4), which is 1 from boot.
    assert_eq!(entry.loaded().initials, b"1up");
    assert!(ctx.session.owner_byte());
    assert!(ctx.session.offline_game().is_some());
    assert_eq!(ctx.session.platform().keyboard_attaches, 1);
    let enumerated = &ctx.session.platform().enumerated;
    assert_eq!(enumerated.len(), 2);
    assert_eq!(enumerated[0], enumerated[1]);
    assert!(ctx.session.input_is_selected());
    assert_eq!(
        ctx.session.frontend_process().entry(),
        FrontendEntry::Returning
    );
    // The transition iteration already ticked, drew and presented the new
    // screen with the same renderer.
    assert_eq!(ctx.presented.last().unwrap().screen, PortScreen::TextEntry);
    assert!(ctx.name_state.finished_draws >= 1);
    assert_eq!(scene.id(), renderer);

    assert_eq!(ctx.session.actor_behavior_tables, retained_behavior);
    // Typing goes through the 00445840 key block on the following ticks.
    press(&mut ctx, &mut scene, resources, "Backspace");
    press(&mut ctx, &mut scene, resources, "A");
    step(&mut ctx, &mut scene, resources, &[]).unwrap();
    assert_eq!(
        ctx.session.text_screen().unwrap().entry().unwrap().name(),
        b"JamesoA"
    );
    let name_frames: Vec<_> = ctx
        .presented
        .iter()
        .filter(|presented| presented.screen == PortScreen::TextEntry)
        .map(|presented| presented.hash)
        .collect();
    let frontend_frames: Vec<_> = ctx
        .presented
        .iter()
        .filter(|presented| presented.screen == PortScreen::Frontend)
        .map(|presented| presented.hash)
        .collect();
    assert!(name_frames
        .iter()
        .all(|hash| !frontend_frames.contains(hash)));

    // Back (0045AC20) requests phase 2; 9->2 clears the prepared state and
    // constructs a returning frontend with frontend music.
    let selections = |ctx: &Ctx| {
        ctx.session
            .platform()
            .effects
            .iter()
            .filter(|effect| **effect == Effect::SelectFrontendMusic)
            .count()
    };
    assert_eq!(selections(&ctx), 1);
    press(&mut ctx, &mut scene, resources, "Escape");
    for _ in 0..4 {
        if ctx.driver.state().current_phase() == SessionPhase::Frontend {
            break;
        }
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Frontend);
    assert!(ctx.session.text_screen().is_none());
    let control = ctx.session.frontend_screen().unwrap().control().unwrap();
    assert_eq!(control.entry_state(), FrontendEntry::Returning);
    assert!(!ctx.session.owner_byte());
    assert!(ctx.session.offline_game().is_none());
    assert_eq!(selections(&ctx), 2);
    assert_eq!(ctx.session.platform().keyboard_attaches, 1);
    for _ in 0..8 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    assert_eq!(ctx.presented.last().unwrap().screen, PortScreen::Frontend);
    let phases: Vec<_> = ctx
        .driver
        .transitions()
        .iter()
        .map(|transition| (transition.previous, transition.effective))
        .collect();
    assert_eq!(
        phases,
        [
            (SessionPhase::Title, SessionPhase::Frontend),
            (SessionPhase::Frontend, SessionPhase::Phase9),
            (SessionPhase::Phase9, SessionPhase::Frontend),
        ]
    );
    // Every iteration presented once, and each 0040D1D0 cleared with the
    // H7/32 material of the renderer's residency.
    assert_eq!(ctx.presented.len() as u64, ctx.frames);
    let clear = presentation_clear_rgba(
        resources,
        crate::frontend_display::PresentationClear::Standard,
        false,
    )
    .unwrap();
    assert_eq!(clear, ctx.clear);
    assert!(ctx
        .presented
        .iter()
        .all(|presented| presented.cleared == clear));
    assert_eq!(scene.stats.presentation_clears as u64, ctx.frames);
    assert_eq!(scene.pixels.stats.unsupported, 0);
    assert!(ctx.completions.is_empty());
    assert!(!ctx.session.platform().profiles.write_attempted());
    assert_eq!(ctx.session.actor_behavior_tables, retained_behavior);
    let replacement_residency = ctx.session.residency().clone();
    let generation = ctx.session.residency_generation();
    ctx.session
        .install_display(
            replacement_residency,
            FrontendRenderContext {
                projection: startup.projection,
            },
        )
        .unwrap();
    assert_eq!(ctx.session.residency_generation(), generation + 1);
    assert_eq!(ctx.session.actor_behavior_tables, retained_behavior);
}

#[test]
fn completion_serializes_in_memory_then_new_game_reaches_level1_via_intro_results() {
    let (mut ctx, tables, startup) = setup("complete");
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    enter_name_entry(&mut ctx, &mut scene, resources);
    let profiles_path = ctx.directory.join("profiles.sav");
    let sidecar = std::fs::read(&profiles_path).unwrap();
    let before = ctx.session.platform().profiles.working_record().cloned();
    // Name completion serializes the actual Prepare projection before LoadWorld
    // applies its player/respawn writes. It is not the later loaded projection.
    let prepared_save = ctx.session.offline_game().unwrap().player.clone();
    let prepared_owner = ctx.session.pending_world.as_ref().unwrap();
    let prepared_player = prepared_owner.arena.first_player().unwrap();
    let prepared_carrier = format!("{:?}", prepared_owner.constructions[0]);
    // Tab runs 0045AEB0: the 1000 ms completion delay, then 0045AD30.
    press(&mut ctx, &mut scene, resources, "Tab");
    let epoch_before = ctx.epoch;
    let mut steps = 0;
    for _ in 0..16 {
        steps += 1;
        match step(&mut ctx, &mut scene, resources, &[]) {
            Ok(()) => {
                // The 9→3 transition loads the world; stop after briefing appears.
                if ctx.session.briefing_screen().is_some() {
                    break;
                }
            }
            Err(error) => panic!("unexpected session error: {error:?}"),
        }
    }
    // The Tab iteration arms 1000 ms and already spends one 125 ms delta
    // (875), the release iteration another (750). 0045A8F0 fires once the
    // remaining delay reaches the delta (equality fires): the sixth later
    // iteration completes, and the seventh first pass runs the 9→3 world load.
    assert_eq!(steps, 7);
    assert!(matches!(ctx.completions[..], [NameCompletion::Phase3]));
    // 9→3 ran `LoadWorld` with the fresh-heap policy: the text-entry screen
    // is destroyed, briefing is constructed, the world is retained, and the
    // profile save was not written to disk.
    assert!(ctx.session.text_screen().is_none());
    assert!(ctx.session.briefing_screen().is_some());
    let briefing = ctx.session.briefing_screen().unwrap();
    assert_eq!(briefing.ticks(), 1);
    assert!(briefing.runtime().unwrap().program().records.is_empty());
    assert!(!briefing.runtime().unwrap().draw_enabled());
    assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Gameplay);
    let world = ctx.session.world().expect("fresh-heap world loaded");
    assert_eq!(world.report.placements.len(), 118);
    assert_eq!(world.arena.len(), 120);
    assert_ne!(ctx.epoch, epoch_before, "world load changed residency");
    assert!(ctx.session.offline_game().is_some());
    let store = &ctx.session.platform().profiles;
    assert_eq!(store.current_name(), Some(b"Jameson".as_slice()));
    // 0044A0E0 selects the append index; 00449C10 serializes the published
    // offline mission/player into the working record in memory.
    assert_eq!(store.selected(), store.cached_count());
    let offline = ctx.session.offline_game().unwrap();
    let working = store.working_record().unwrap();
    assert_ne!(Some(working), before.as_ref());
    assert_eq!(working.name(), b"Jameson");
    assert_eq!(working.mission(), offline.mission);
    assert_eq!(working.player(), prepared_save);
    assert_ne!(working.player(), offline.player);
    assert_eq!(world.arena.first_player(), Some(prepared_player));
    assert_eq!(format!("{:?}", world.constructions[0]), prepared_carrier);
    assert!(!store.write_attempted());
    assert_eq!(std::fs::read(&profiles_path).unwrap(), sidecar);
    // 004633D0 reached the portable sidecar instead of the registry.
    let assets = FrontendAssets::open(&root()).unwrap();
    let saved =
        FrontendSettings::load_or_defaults(ctx.directory.join("settings.json"), &assets).unwrap();
    assert_eq!(saved.stored_player_name(), Some(&b"Jameson"[..]));
    assert_eq!(saved.stored_player_initials(), Some(&b"1up"[..]));
    // The app rebinds its renderer to the loaded world's residency after the
    // 9→3 load, before drawing; gameplay's clear reads that world's H7 0x72.
    drop(scene);
    let world_snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&world_snapshot);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    // Briefing ticks once and requests gameplay (3→4), matching the observed
    // single-tick completion. Step until gameplay appears.
    let mut gameplay_steps = 0;
    for _ in 0..8 {
        gameplay_steps += 1;
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
        if ctx.session.gameplay_screen().is_some() {
            break;
        }
    }
    assert!(
        ctx.session.gameplay_screen().is_some(),
        "briefing ticked to gameplay in {gameplay_steps} steps"
    );
    assert_eq!(
        gameplay_steps, 1,
        "the empty intro briefing finishes in its first pass"
    );
    assert!(ctx.session.briefing_screen().is_none());
    assert!(ctx.session.world().is_some());
    assert_eq!(
        ctx.session
            .maps
            .jitter(ctx.session.world().unwrap().map)
            .unwrap()
            .jitter_radius(),
        0
    );
    let phases: Vec<_> = ctx
        .driver
        .transitions()
        .iter()
        .map(|transition| (transition.previous, transition.effective))
        .collect();
    assert!(phases.contains(&(SessionPhase::Phase9, SessionPhase::Phase3)));
    assert!(phases.contains(&(SessionPhase::Phase3, SessionPhase::Gameplay)));
    assert_eq!(
        ctx.session
            .offline_game()
            .unwrap()
            .mission
            .mission_selection,
        22
    );
    let intro_epoch = ctx.epoch;
    drop(scene);

    // The intro is phase 4 on mission 22, and its authored Escape skip runs
    // the real 4→8 admission. The bounded results owner must be constructed
    // and tick before its head submits 8→3; a redirect would miss this state.
    let intro_snapshot = ctx.session.residency().clone();
    let intro_resources = RenderResources::new(&intro_snapshot);
    let elapsed_before_results;
    {
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: ctx.session.model_process().clone(),
            resources: intro_resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        for _ in 0..9 {
            step(&mut ctx, &mut scene, intro_resources, &[]).unwrap();
        }
        elapsed_before_results = ctx.driver.state().gameplay_time();
        assert!(elapsed_before_results.seconds > 0);
        assert!(elapsed_before_results.remainder_micros > 0);
        assert_eq!(
            ctx.session.world().unwrap().mission.save.elapsed_seconds,
            elapsed_before_results.seconds
        );
        // Inject an explicit saved-import second player to discriminate the
        // native fresh first-player rule from the old stored-player projection.
        // Reload must keep both actual arena records and the genuine first
        // player's constructor/input carrier; it cannot reimport either one.
        {
            let world = ctx.session.world_mut().unwrap();
            let first = world.arena.first_player().unwrap();
            let mut first_state = world.arena.export_player_save_state(first).unwrap();
            first_state.owner_118 = 0xf123_4567;
            first_state.owner_13c = [0xff00_0080, 0x807f_0100];
            world
                .arena
                .restore_player_save_prefix(first, &first_state)
                .unwrap();
            let retained = world.arena.add_world_player(first_state.owner_a0);
            let mut retained_state = world.arena.export_player_save_state(retained).unwrap();
            retained_state.owner_118 = 0x1122_3344;
            world
                .arena
                .restore_player_save_prefix(retained, &retained_state)
                .unwrap();
            world.player = Some(retained);
            // W+34 is a projection of the session counter. Saving must read
            // its sole authority without turning this stale copy into a writer.
            world.mission.save.elapsed_seconds = 0x1234_5678;
            assert_eq!(world.mission.bytes_14_16.unwrap()[1], 0);
        }
        ctx.session
            .profile_name_classification
            .high_bytes
            .as_mut()
            .unwrap()[b'#' as usize] = Some(0x7f);
        let intro_map = ctx.session.world().unwrap().map;
        ctx.session
            .maps
            .jitter_mut(intro_map)
            .unwrap()
            .apply(crate::world_map::WorldMapJitterWrite::MissionEvent48D980)
            .unwrap();
        let animation_before_results = ctx.session.frontend_process().animation();
        let render_before_results = scene.stats;
        let context_before_results = ctx.frontend_state.caller_context;
        let name_draws_before_results = ctx.name_state.finished_draws;
        // Actual scheduler subtraction now supplies raw250000 while retaining
        // its authored cap125000; phase8 does not advance gameplay elapsed.
        ctx.now += 125_000;
        step(&mut ctx, &mut scene, intro_resources, &[("Escape", true)]).unwrap();
        assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase8);
        assert_eq!(
            ctx.session.linked_screen().unwrap(),
            PortScreen::IntroResults
        );
        assert_eq!(ctx.session.intro_results_screen().unwrap().ticks(), 1);
        // The pending head request3 does not suppress +08/+0C this pass.
        // 0042C2D1 completes439270, then fresh current-world22 exits before
        // caller-context capture, content/model work or depth reset.
        assert_eq!(
            scene.stats.frame_begins,
            render_before_results.frame_begins + 1
        );
        assert_eq!(scene.stats.begins, render_before_results.begins + 1);
        assert_eq!(scene.stats.depth_resets, render_before_results.depth_resets);
        assert_eq!(scene.stats.models, render_before_results.models);
        assert_eq!(scene.stats.flushes, render_before_results.flushes);
        assert_eq!(
            scene.stats.presentation_clears,
            render_before_results.presentation_clears + 1
        );
        assert_eq!(ctx.frontend_state.published_renderer, Some(scene.id()));
        assert_eq!(ctx.frontend_state.caller_context, context_before_results);
        assert_eq!(ctx.name_state.finished_draws, name_draws_before_results);
        assert_eq!(scene.queue.state().command_count, 0);
        let results = ctx.session.intro_results_screen().unwrap();
        assert_eq!(results.input().age_millis(), 375);
        assert_eq!(results.elapsed_millis_14c(), 125);
        assert!(!results.intro_delay_elapsed());
        assert_eq!(results.input().held_mask(), 0);
        let animation_after_results = ctx.session.frontend_process().animation();
        assert_eq!(
            animation_after_results.accumulated_micros(),
            animation_before_results
                .accumulated_micros()
                .wrapping_add(125_000)
        );
        assert_eq!(
            animation_after_results.value(),
            ((animation_after_results.accumulated_micros() as i64) / 20_000) as u32
        );
        assert!(ctx.session.gameplay_screen().is_none());
        assert!(ctx.driver.state().transition_pending());
        assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Phase3);
        let transition = ctx.driver.transitions().last().unwrap();
        assert_eq!(transition.previous, SessionPhase::Gameplay);
        assert_eq!(transition.effective, SessionPhase::Phase8);
        assert_eq!(transition.created, Some(ScreenFactory::Phase8));
        assert_eq!(ctx.epoch, intro_epoch, "results retains the intro world");
        assert_eq!(
            ctx.session.map_load.current(),
            None,
            "results retires the native map before retaining the player save carrier"
        );
        assert!(
            ctx.session.maps.jitter(intro_map).is_err(),
            "the retired allocation cannot supply a stale jitter copy"
        );
        assert_eq!(ctx.driver.state().gameplay_time(), elapsed_before_results);
        let world = ctx.session.world().unwrap();
        let first = world.arena.first_player().unwrap();
        let expected_player = world.arena.export_player_save_state(first).unwrap();
        assert_ne!(Some(first), world.player);
        assert_eq!(world.mission.save.elapsed_seconds, 0x1234_5678);
        let mut expected_mission = world.mission.save.clone();
        expected_mission.elapsed_seconds = elapsed_before_results.seconds;
        let store = &ctx.session.platform().profiles;
        let working = store.working_record().unwrap();
        assert_eq!(working.name(), b"Jameson");
        assert_eq!(working.mission(), expected_mission);
        assert_eq!(working.player(), expected_player);
        assert_eq!(working.player().owner_118, 0xf123_4567);
        assert_eq!(store.current_name(), Some(b"Jameson".as_slice()));
        assert!(!store.write_attempted());
        assert_eq!(std::fs::read(&profiles_path).unwrap(), sidecar);
        assert_eq!(
            ctx.session
                .profile_name_classification
                .high_bytes
                .as_ref()
                .unwrap()[b'#' as usize],
            Some(0x7f)
        );
        assert_eq!(
            ctx.presented.last().unwrap().screen,
            PortScreen::IntroResults
        );
        step(&mut ctx, &mut scene, intro_resources, &[("Escape", false)]).unwrap();
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase3);
    assert!(ctx.session.intro_results_screen().is_none());
    assert!(ctx.session.briefing_screen().is_some());
    // 0040A98E resets this same process clock. Constructor +F0=1 then makes
    // 0040C735..0040C746 supply trunc(125000*7/8) to the first item Tick;
    // 0040CDF4 advances439560 with that scaled argument, not raw125000.
    assert_eq!(
        ctx.session
            .frontend_process()
            .animation()
            .accumulated_micros(),
        109_375
    );
    assert_eq!(
        ctx.session
            .maps
            .jitter(ctx.session.world().unwrap().map)
            .unwrap()
            .jitter_radius(),
        0,
        "the next world load constructs a fresh native map owner"
    );
    assert_eq!(ctx.driver.state().gameplay_time(), elapsed_before_results);
    assert_eq!(
        ctx.session
            .profile_name_classification
            .high_bytes
            .as_ref()
            .unwrap()[b'#' as usize],
        Some(0x7f)
    );
    let world = ctx.session.world().unwrap();
    let reloaded = world
        .arena
        .export_player_save_state(world.player.unwrap())
        .unwrap();
    assert_eq!(world.player, Some(prepared_player));
    assert_eq!(world.arena.first_player(), Some(prepared_player));
    assert_eq!(world.arena.world_players().len(), 2);
    assert_eq!(format!("{:?}", world.constructions[0]), prepared_carrier);
    assert_eq!(
        reloaded.owner_118, 0xf123_4567,
        "reload uses the actual fresh first player"
    );
    assert_eq!(reloaded.owner_13c, [0xff00_0080, 0x807f_0100]);
    let second = world.arena.world_players()[1].id;
    assert_ne!(second, prepared_player);
    assert_eq!(
        world
            .arena
            .export_player_save_state(second)
            .unwrap()
            .owner_118,
        0x1122_3344
    );
    assert_eq!(
        ctx.session.world().unwrap().mission.save.elapsed_seconds,
        elapsed_before_results.seconds
    );
    assert_eq!(
        ctx.session.offline_game().unwrap().mission.elapsed_seconds,
        elapsed_before_results.seconds
    );
    assert_eq!(
        ctx.session
            .offline_game()
            .unwrap()
            .mission
            .mission_selection,
        18
    );
    assert_ne!(
        ctx.epoch, intro_epoch,
        "8→3 loads the authored next mission"
    );
    let level1_epoch = ctx.epoch;
    let level1_snapshot = ctx.session.residency().clone();
    let level1_resources = RenderResources::new(&level1_snapshot);
    {
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: ctx.session.model_process().clone(),
            resources: level1_resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        for _ in 0..40 {
            step(&mut ctx, &mut scene, level1_resources, &[]).unwrap();
        }
        assert_eq!(ctx.driver.state().gameplay_time(), elapsed_before_results);
        let briefing = ctx.session.briefing_screen().unwrap();
        let runtime = briefing.runtime().unwrap();
        assert_eq!(runtime.program().records.len(), 50);
        assert!(runtime.menu_text_id().is_some());
        assert!(runtime.draw_enabled());
        assert!(!ctx.driver.state().transition_pending());
        assert_eq!(ctx.presented.last().unwrap().screen, PortScreen::Briefing);
        press(&mut ctx, &mut scene, level1_resources, "Escape");
        for _ in 0..18 {
            step(&mut ctx, &mut scene, level1_resources, &[]).unwrap();
            if ctx.session.gameplay_screen().is_some() {
                break;
            }
        }
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Gameplay);
    assert!(ctx.session.briefing_screen().is_none());
    assert_eq!(
        ctx.session
            .offline_game()
            .unwrap()
            .mission
            .mission_selection,
        18
    );
    assert_eq!(ctx.epoch, level1_epoch, "3→4 retains the level-1 world");
    let phases: Vec<_> = ctx
        .driver
        .transitions()
        .iter()
        .map(|transition| (transition.previous, transition.effective))
        .collect();
    assert_eq!(
        phases,
        [
            (SessionPhase::Title, SessionPhase::Frontend),
            (SessionPhase::Frontend, SessionPhase::Phase9),
            (SessionPhase::Phase9, SessionPhase::Phase3),
            (SessionPhase::Phase3, SessionPhase::Gameplay),
            (SessionPhase::Gameplay, SessionPhase::Phase8),
            (SessionPhase::Phase8, SessionPhase::Phase3),
            (SessionPhase::Phase3, SessionPhase::Gameplay),
        ]
    );

    // 00478F20's player block: 0046A0E0 keeps the vehicle out of the update
    // loop and 0046A260 fills the first player's control block from its own
    // references. Space reaches Thrust through the gameplay sub-object's row.
    {
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: ctx.session.model_process().clone(),
            resources: level1_resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        step(&mut ctx, &mut scene, level1_resources, &[("Space", true)]).unwrap();
        step(&mut ctx, &mut scene, level1_resources, &[]).unwrap();
    }
    let world = ctx.session.world_mut().unwrap();
    assert_eq!(world.actor_updates.player_update_stop, None);
    let player = world.arena.first_player().unwrap();
    let controls = world.arena.world_player(player).unwrap().controls;
    assert_eq!(controls.thrust_1c, 125_000);
    assert_eq!((controls.turn_14, controls.pitch_18), (0, 0));
    // 0046A260 set +3A (Thrust held); the drive consumed it as a jump
    // request, which needs turbo (upgrade 3), so Space only drove forward.
    assert_eq!(controls.byte(0x3a), 0);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    assert_eq!(world.arena.player_actor(player).unwrap(), Some(vehicle));
    assert_ne!(world.arena.object(vehicle).unwrap().base.flags & 0x10, 0);
    assert!(!world
        .actor_updates
        .skipped_callbacks
        .iter()
        .any(|(actor, _)| *actor == vehicle));
    // The vehicle's own update (00416170) ran with that block: it did not
    // stop, and the buggy is moving.
    assert_eq!(world.actor_updates.player_vehicle_stop, None);
    assert_ne!(world.arena.actor(vehicle).unwrap().words_8e_92, [0; 3]);

    // The sub-object's rows call the player's key callbacks with the player
    // as context: Up is also the message cursor key, held in +50 with the
    // repeat 0046D2E0 starts, and the player block keeps running.
    {
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: ctx.session.model_process().clone(),
            resources: level1_resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        step(&mut ctx, &mut scene, level1_resources, &[("Up", true)]).unwrap();
    }
    let world = ctx.session.world_mut().unwrap();
    assert_eq!(world.actor_updates.player_keys_stop, None);
    assert_eq!(world.actor_updates.player_update_stop, None);
    assert!(world.actor_updates.key_omissions_logged.is_empty());
    let state = world.arena.world_player(player).unwrap();
    assert_eq!(state.dword_50, Some(2));
    assert!(state.dword_54.is_some_and(|repeat| repeat <= 250));
    ctx.session.shutdown().unwrap();
}

#[test]
fn level1_briefing_keeps_authored_text_until_native_start_delay_finishes() {
    let (mut ctx, tables, startup) = setup_inner("level1-briefing", false, Some(18));
    // The diagnostic mission override uses the same profile, completion,
    // LoadWorld and screen factories as New Game, selecting the actual
    // level-1 mission directly rather than waiting through the intro.
    let frontend_snapshot = ctx.session.residency().clone();
    let frontend_resources = RenderResources::new(&frontend_snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    {
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: ctx.session.model_process().clone(),
            resources: frontend_resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        enter_name_entry(&mut ctx, &mut scene, frontend_resources);
        press(&mut ctx, &mut scene, frontend_resources, "Tab");
        for _ in 0..16 {
            step(&mut ctx, &mut scene, frontend_resources, &[]).unwrap();
            if ctx.session.briefing_screen().is_some() {
                break;
            }
        }
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase3);
    assert!(!ctx.driver.state().transition_pending());
    let briefing = ctx.session.briefing_screen().expect("level-1 briefing");
    assert_eq!(briefing.ticks(), 1);
    assert_eq!(briefing.runtime().unwrap().program().records.len(), 50);
    assert_eq!(
        ctx.session
            .offline_game()
            .unwrap()
            .mission
            .mission_selection,
        18
    );
    let loaded_epoch = ctx.epoch;

    // Rebind the software renderer to the winning level residency, just as
    // the app does on its resource epoch change. Frontend strings cannot
    // substitute for the authored level's Handler-0 text.
    let world_snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&world_snapshot);
    assert!(resources.string(875).is_ok());
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    let native_clear = presentation_clear_rgba(
        resources,
        crate::frontend_display::PresentationClear::Standard,
        false,
    )
    .unwrap();
    let blank = SoftwareFrame::new(640, 480, native_clear);
    let mut hasher = DefaultHasher::new();
    blank.rgba().hash(&mut hasher);
    let blank_hash = hasher.finish();
    let glyphs_before = ctx.frontend_state.menu.glyph_passes;
    for _ in 0..100 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
        let briefing = ctx.session.briefing_screen().unwrap();
        let runtime = briefing.runtime().unwrap();
        let text = runtime.visibility(crate::briefing_runtime::BriefingPanel::Text);
        let row_age = briefing
            .controls()
            .unwrap()
            .text
            .current()
            .unwrap()
            .data()
            .records()
            .items[0]
            .age;
        if runtime.menu_text_id() == Some(875)
            && text.current_stage == 2
            && text.requested_stage == 2
            && row_age >= 500
        {
            break;
        }
    }
    let briefing = ctx.session.briefing_screen().unwrap();
    let runtime = briefing.runtime().unwrap();
    assert_eq!(runtime.menu_text_id(), Some(875));
    let text = runtime.visibility(crate::briefing_runtime::BriefingPanel::Text);
    assert_eq!((text.current_stage, text.requested_stage), (2, 2));
    let text_records = briefing
        .controls()
        .unwrap()
        .text
        .current()
        .unwrap()
        .data()
        .records();
    assert_eq!(text_records.items[0].draw_arguments[0], 875);
    assert!(text_records.items[0].age >= 500);
    drop(text_records);
    assert!(ctx.frontend_state.menu.glyph_passes > glyphs_before);
    assert_eq!(ctx.presented.last().unwrap().screen, PortScreen::Briefing);
    assert_ne!(ctx.presented.last().unwrap().hash, blank_hash);
    assert_eq!(ctx.presented.last().unwrap().cleared, native_clear);
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase3);
    assert!(!ctx.driver.state().transition_pending());

    // Esc is an authored Start shortcut (0040D660), separate from Back.
    // C6A0 scales a 125 ms tick by 7/8, then C770 truncates to 109 ms.
    press(&mut ctx, &mut scene, resources, "Escape");
    assert_eq!(
        ctx.session
            .briefing_screen()
            .unwrap()
            .runtime()
            .unwrap()
            .start_remaining_millis(),
        2000 - 2 * 109
    );
    for _ in 0..16 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
        assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase3);
        assert!(!ctx.driver.state().transition_pending());
    }
    assert_eq!(
        ctx.session
            .briefing_screen()
            .unwrap()
            .runtime()
            .unwrap()
            .start_remaining_millis(),
        38
    );
    step(&mut ctx, &mut scene, resources, &[]).unwrap();
    assert!(ctx.driver.state().transition_pending());
    assert_eq!(ctx.driver.state().requested_phase(), SessionPhase::Gameplay);
    assert_eq!(
        ctx.session
            .briefing_screen()
            .unwrap()
            .runtime()
            .unwrap()
            .finished(),
        Some(crate::briefing_runtime::BriefingFinishReason::StartTimer)
    );
    step(&mut ctx, &mut scene, resources, &[]).unwrap();
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Gameplay);
    assert!(ctx.session.briefing_screen().is_none());
    assert!(ctx.session.gameplay_screen().is_some());
    assert_eq!(
        ctx.epoch, loaded_epoch,
        "gameplay retains the briefing-loaded world"
    );
    let world = ctx.session.world().unwrap();
    assert!(world.intro_runtime.is_none());
    assert!(world.native_camera.is_some());
    assert_eq!(world.camera_failure, None);
    let draw = world
        .last_draw
        .expect("ordinary gameplay drew through shared camera");
    assert!(draw.native_camera);
    assert_eq!(draw.coordinate_shift, 16);
    assert_eq!(
        world
            .arena
            .camera_draw_request(world.native_camera.unwrap())
            .unwrap()
            .eye,
        draw.eye,
    );
    assert_eq!(
        ctx.session
            .offline_game()
            .unwrap()
            .mission
            .mission_selection,
        18
    );
    ctx.session.shutdown().unwrap();
}

#[test]
fn services_must_match_the_linked_screen_before_anything_runs() {
    let (mut ctx, _tables, _startup) = setup("services");
    let mut state = ctx.driver.state().clone();
    let PortSplit {
        mut linked,
        platform,
        ..
    } = ctx.session.split();
    let TestPlatform {
        settings,
        profiles,
        random_state,
        identity,
        exit_requested,
        ..
    } = platform;
    let mut text = SettingsTextEntryPlatform {
        settings,
        identity,
        exit_requested,
    };
    let services: LinkedServices<'_, TestEffects<'_>, _> = LinkedServices::TextEntry {
        platform: &mut text,
        profiles,
        rng: random_state,
        keyboard: None,
    };
    let tick = SchedulerTick {
        delta_micros: 8_000,
        elapsed_micros: 8_000,
    };
    let mut completions = Vec::new();
    assert!(matches!(
        linked.tick(&mut state, tick, services, &mut completions),
        Err(PortLinkedError::Services {
            screen: PortScreen::Frontend,
            services: "text-entry",
        })
    ));
    let age = ctx
        .session
        .frontend_screen()
        .unwrap()
        .screen_input()
        .age_millis();
    assert_eq!(age, 0, "the frontend did not tick");
}

#[test]
fn results_retirement_uses_the_loaded_vehicle_and_retains_stopped_prefixes() {
    use crate::object_runtime::{
        ActorExtension, ActorVehicleCleanupNotificationError, DerivedActorClass,
        PlayerActorLookupError, WorldCleanupError,
    };
    use crate::world_backend::WorldOwnerRetirement;

    for missing_process_null in [false, true] {
        let (mut ctx, _tables, _startup) = setup("results-retirement");
        let mut controller = crate::session::SessionController::default();
        controller.request_phase(SessionPhase::Frontend);
        controller.transition(&mut ctx.session).unwrap();
        ctx.session
            .prepare_offline_player(&mut SessionState::default())
            .unwrap();
        let mut state = controller.state().clone();
        ctx.session
            .transition_effect(TransitionEffect::LoadWorld, &mut state)
            .unwrap();
        let input_before = (
            ctx.session.input.clock_micros(),
            ctx.session.input.queued_len(),
            ctx.session.input.stopped(),
            ctx.session.input.selected_context(),
        );
        let null = ctx.session.factory_process.process_null_handle.unwrap();
        let (map, first, second, vehicle, second_before, mode, signed, values) = {
            let world = ctx.session.world_mut().unwrap();
            let first = world.arena.first_player().unwrap();
            let vehicle = world.arena.player_actor(first).unwrap().unwrap();
            assert_eq!(world.respawn.as_ref().unwrap().vehicle, Some(vehicle));
            let ActorExtension::Derived(derived) = &world.arena.actor(vehicle).unwrap().extension
            else {
                panic!("loaded derived vehicle");
            };
            let DerivedActorClass::Selector2(component) = &derived.class else {
                panic!("loaded selector2");
            };
            let mode = component.dwords_1e8_1f4[0];
            assert_eq!(component.handles_1a4.handles, vec![null; 3]);
            let behavior = derived.behavior_140.as_ref().unwrap();
            let signed = [behavior.byte_10, behavior.byte_11].map(|b| i32::from(b as i8) as u32);
            let values = std::array::from_fn::<_, 16, _>(|i| behavior.entries_18[i][0]);
            let second = world.arena.add_world_player([0x7654_3210; 6]);
            let second_before = world.arena.world_player(second).unwrap().clone();
            let mut saved = world.arena.export_player_save_state(first).unwrap();
            saved.owner_144 = mode.wrapping_add(1);
            saved.owner_148 = 0x2222_2222;
            saved.owner_14c = 0x3333_3333;
            saved.owner_150 = [0x4444_4444; 16];
            saved.owner_190 = [0x5555; 3];
            world
                .arena
                .restore_player_save_prefix(first, &saved)
                .unwrap();
            world.player = Some(second);
            (
                world.map,
                first,
                second,
                vehicle,
                second_before,
                mode,
                signed,
                values,
            )
        };
        if missing_process_null {
            // The controlled lookups/copy do not consume this owner. Actual
            // vehicle +30/event10 reaches it only inside the later manager.
            ctx.session.factory_process.process_null_handle = None;
        }
        ctx.session.camera_shake_enabled = 1;
        ctx.session
            .transition_effect(TransitionEffect::PrepareResults, &mut state)
            .unwrap();
        assert_eq!(ctx.session.map_load.current(), None);
        assert!(ctx.session.maps.jitter(map).is_err());
        assert_eq!(ctx.session.camera_shake_enabled, 0);
        assert_eq!(
            (
                ctx.session.input.clock_micros(),
                ctx.session.input.queued_len(),
                ctx.session.input.stopped(),
                ctx.session.input.selected_context(),
            ),
            input_before
        );
        let world = ctx.session.world_mut().unwrap();
        assert_eq!(world.mission.byte_12, Some(0));
        assert_eq!(world.arena.first_player(), Some(first));
        assert_eq!(world.player, Some(second));
        assert_eq!(world.arena.world_player(second).unwrap(), &second_before);
        let mut saved = world.arena.export_player_save_state(first).unwrap();
        assert_eq!(saved.owner_144, mode);
        assert_eq!(saved.owner_148, signed[0]);
        assert_eq!(saved.owner_14c, signed[1]);
        assert_eq!(saved.owner_150, values);
        assert_eq!(saved.owner_190, [0; 3]);
        if missing_process_null {
            assert!(matches!(
                world.owner_retirement,
                WorldOwnerRetirement::Stopped(WorldCleanupError::Vehicle(
                    ActorVehicleCleanupNotificationError::PlayerLookup(
                        PlayerActorLookupError::MissingProcessNullHandle
                    )
                ))
            ));
            assert_eq!(world.mission.inhibit_byte_08, 1);
            assert!(!world.arena.actor_list().is_empty());
            // A retry would overwrite this later genuine save write before
            // reentering the manager. Preserve it after the source is repaired.
            saved.owner_144 = mode.wrapping_add(2);
            world
                .arena
                .restore_player_save_prefix(first, &saved)
                .unwrap();
        } else {
            assert!(matches!(
                world.owner_retirement,
                WorldOwnerRetirement::Completed
            ));
            assert!(world.arena.actor_list().is_empty());
            assert_eq!(world.mission.inhibit_byte_08, 0);
            assert!(world.arena.object(vehicle).is_err());
        }
        let arena_before_repeat = format!("{:?}", world.arena);
        let status_before_repeat = format!("{:?}", world.owner_retirement);
        ctx.session.factory_process.process_null_handle = Some(null);
        ctx.session
            .transition_effect(TransitionEffect::PrepareResults, &mut state)
            .unwrap();
        let world = ctx.session.world().unwrap();
        assert_eq!(format!("{:?}", world.arena), arena_before_repeat);
        assert_eq!(
            format!("{:?}", world.owner_retirement),
            status_before_repeat
        );
        assert_eq!(world.arena.export_player_save_state(first).unwrap(), saved);
        ctx.session.shutdown().unwrap();
    }
}

#[test]
fn actual_results_restore_keeps_its_prefix_across_the_late_identity_write() {
    use crate::object_runtime::{PlayerMessageOperation, PlayerMessageRequest};
    use crate::screen_registry::ScreenLifecycle;
    use inf_formats::{PlayerSaveState, SaveRecord};

    for fail_write in [false, true] {
        let (mut ctx, _tables, _startup) = setup("results-restore-identity");
        let mut controller = crate::session::SessionController::default();
        controller.request_phase(SessionPhase::Frontend);
        controller.transition(&mut ctx.session).unwrap();
        ctx.session
            .prepare_offline_player(&mut SessionState::default())
            .unwrap();
        let mut load_state = controller.state().clone();
        ctx.session
            .transition_effect(TransitionEffect::LoadWorld, &mut load_state)
            .unwrap();
        controller.request_phase(SessionPhase::Gameplay);
        controller.transition(&mut ctx.session).unwrap();
        // This remainder comes from the existing actual controller tick; no
        // copied clock/default field is introduced for the Restore request.
        controller.tick(&mut ctx.session, 125_000, 125_000).unwrap();
        let before_time = controller.state().gameplay_time();
        assert!(before_time.remainder_micros > 0);
        // Retire the actual loaded owners before isolating the Restore
        // constructor custody. The real transition test covers this prelude;
        // the later phase change must not replay non-resumable callbacks.
        ctx.session
            .prepare_intro_results(&SessionState::default())
            .unwrap();
        let retirement_before = format!("{:?}", ctx.session.world().unwrap().owner_retirement);
        assert!(!matches!(
            ctx.session.world().unwrap().owner_retirement,
            crate::world_backend::WorldOwnerRetirement::NotStarted
        ));

        let mut saved_mission = MissionSaveState::new_game();
        saved_mission.mission_selection = 18;
        saved_mission.elapsed_seconds = 73;
        saved_mission.owner_b0[7] = 0x8000_1234;
        saved_mission.owner_240[99] = 0xf123_4567;
        let mut saved_player = PlayerSaveState::new_player([0x8000_0001; 6]);
        saved_player.owner_100 = [0xffff_fffd; 6];
        saved_player.owner_118 = 0xf123_4567;
        saved_player.owner_11c = 0xffff;
        saved_player.owner_11e = 0x8001;
        saved_player.owner_120 = [1, 2, 0x8000_0000, u32::MAX];
        saved_player.owner_144 = 0x8000_0011;
        saved_player.owner_148 = 0x1122_3344;
        saved_player.owner_14c = 0xfedc_ba98;
        saved_player.owner_150 = std::array::from_fn(|index| (index as u32).wrapping_sub(8));
        saved_player.owner_13c = [0xff00_0080, 0x807f_0100];
        saved_player.owner_190 = [0x8001, 0xffff, 0x7ffe];
        saved_player.owner_130 = [0x1234_5678, 0x8123_4567, 0xfedc_ba98];
        let source = SaveRecord::from_state(
            b"Restored player",
            &saved_mission,
            &saved_player,
            &[0x5c; 16],
        )
        .unwrap();
        let retail_path = ctx.directory.join("restore-retail.sav");
        let profile_path = ctx.directory.join("restore-port.sav");
        std::fs::write(
            &retail_path,
            inf_formats::encode_save_records(std::slice::from_ref(&source)).unwrap(),
        )
        .unwrap();
        let platform = ctx.session.platform_mut();
        platform.profiles = ProfileStore::open_copy(&retail_path, &profile_path).unwrap();
        platform.profiles.load(0).unwrap();
        platform
            .profiles
            .prepare_new_name(b"Prior current")
            .unwrap();
        platform.settings.set_player_name(b"Saved player");
        platform.settings.set_net_player_name(b"Saved network");
        platform.settings.set_player_initials(b"OLD");
        platform.settings.persist().unwrap();
        platform.identity.clear_loaded();
        read_identity(
            &mut platform.identity,
            &platform.settings,
            SessionPhase::Phase9,
            1,
        )
        .unwrap();
        let cache = platform.identity.clone();
        platform.settings.set_player_name(b"Live player");
        platform.settings.set_player_initials(b"LIVE");
        platform.exit_requested = true;
        let words = *platform.settings.words();
        let selected = platform.profiles.selected();
        let control = platform.profiles.control_token();
        let profile_bytes = std::fs::read(&profile_path).unwrap();
        let settings_path = ctx.directory.join("settings.json");
        let settings_bytes = std::fs::read(&settings_path).unwrap();
        if fail_write {
            std::fs::write(settings_path.with_extension("json.tmp"), b"occupied").unwrap();
        }

        let world = ctx.session.world_mut().unwrap();
        let first = world.arena.first_player().unwrap();
        let mut first_saved = world.arena.export_player_save_state(first).unwrap();
        first_saved.owner_11c = 0x8000;
        first_saved.owner_130 = [0x1020_3040, 4, 0x8877_6655];
        world
            .arena
            .restore_player_save_prefix(first, &first_saved)
            .unwrap();
        let message = world.arena.player_message(first).unwrap().identity();
        world
            .arena
            .apply_player_message(PlayerMessageRequest {
                recipient: message,
                operation: PlayerMessageOperation::Enable,
            })
            .unwrap();
        let first_before = world.arena.world_player(first).unwrap().clone();
        let second = world.arena.add_world_player([0x7654_3210; 6]);
        let second_before = world.arena.world_player(second).unwrap().clone();
        world.player = Some(second);
        world.mission.save.mission_selection = 18;
        world.mission.bytes_14_16 = Some([0x31, 0x80, 0x73]);
        let mission_header = (
            world.mission.current_world_a0,
            world.mission.bytes_14_16,
            world.mission.byte_13,
            world.mission.dword_1c,
        );
        let arena_before = format!("{:?}", world.arena);
        let mut expected_first = first_before.clone();
        expected_first.bits_100 = saved_player.owner_100;
        expected_first.dword_118 = saved_player.owner_118;
        expected_first.word_11c = saved_player.owner_11c;
        expected_first.word_11e = saved_player.owner_11e;
        expected_first
            .dwords_120_128
            .copy_from_slice(&saved_player.owner_120[..3]);
        expected_first.dword_12c = saved_player.owner_120[3];
        expected_first.dword_144 = saved_player.owner_144;
        expected_first.dword_148 = saved_player.owner_148;
        expected_first.dword_14c = saved_player.owner_14c;
        expected_first.values_150 = saved_player.owner_150.map(|word| word as i32);
        expected_first.bytes_13c_143[..4].copy_from_slice(&saved_player.owner_13c[0].to_le_bytes());
        expected_first.bytes_13c_143[4..].copy_from_slice(&saved_player.owner_13c[1].to_le_bytes());
        expected_first.items_190 = saved_player.owner_190;
        expected_first.command_mask_130 = saved_player.owner_130;
        expected_first.grants_9c = saved_player.owner_a0;
        // ObjectArena's complete Debug custody includes allocation/list/cache
        // owners. Replace only this unique player's declared expected state.
        let before_player = format!("{first_before:?}");
        assert_eq!(arena_before.matches(&before_player).count(), 1);
        let expected_arena = arena_before.replace(&before_player, &format!("{expected_first:?}"));
        let process = ctx.session.factory_process;
        let classification = ctx.session.profile_name_classification.high_bytes;
        controller.request_phase(SessionPhase::Phase8);
        controller.transition(&mut ctx.session).unwrap();

        assert_eq!(controller.state().current_phase(), SessionPhase::Phase8);
        let time = controller.state().gameplay_time();
        assert_eq!(time.seconds, saved_mission.elapsed_seconds);
        assert_eq!(time.remainder_micros, before_time.remainder_micros);
        let screen = ctx.session.intro_results_screen().unwrap();
        assert_eq!(screen.lifecycle(), ScreenLifecycle::Ready);
        assert_eq!(screen.player_word_144(), 0x8000, "copied before Restore");
        let id = screen.id().unwrap();
        assert_eq!(ctx.session.registry.selected_screen(), Some(id));
        assert_eq!(
            ctx.session.input.selected_context(),
            ctx.session.registry.screen(id).unwrap().input_context
        );
        let world = ctx.session.world().unwrap();
        assert_eq!(world.mission.save, saved_mission);
        assert_eq!(
            (
                world.mission.current_world_a0,
                world.mission.bytes_14_16,
                world.mission.byte_13,
                world.mission.dword_1c
            ),
            mission_header
        );
        assert_eq!(world.player, Some(second));
        assert_eq!(world.arena.world_player(first).unwrap(), &expected_first);
        assert_eq!(format!("{:?}", world.owner_retirement), retirement_before);
        assert_eq!(world.arena.world_player(second).unwrap(), &second_before);
        assert_eq!(format!("{:?}", world.arena), expected_arena);
        assert_eq!(ctx.session.factory_process, process);
        assert_eq!(
            ctx.session.profile_name_classification.high_bytes,
            classification
        );
        let platform = ctx.session.platform();
        assert_eq!(
            platform.profiles.current_name(),
            Some(b"Restored player".as_slice())
        );
        assert_eq!(platform.profiles.working_record(), Some(&source));
        assert_eq!(platform.profiles.selected(), selected);
        assert_eq!(platform.profiles.control_token(), control);
        assert!(!platform.profiles.write_attempted());
        assert_eq!(std::fs::read(&profile_path).unwrap(), profile_bytes);
        assert_eq!(platform.settings.words(), &words);
        assert_eq!(
            platform.settings.stored_net_player_name(),
            Some(b"Saved network".as_slice())
        );
        assert_eq!(
            platform.settings.stored_player_initials(),
            Some(b"LIVE".as_slice())
        );
        assert!(platform.exit_requested);
        let mut expected_cache = cache;
        if !fail_write {
            expected_cache.clear_loaded();
        }
        assert_eq!(platform.identity, expected_cache);
        assert_eq!(
            platform.settings.stored_player_name(),
            Some(if fail_write {
                b"Live player".as_slice()
            } else {
                b"Restored player".as_slice()
            })
        );
        let assets = FrontendAssets::open(&root()).unwrap();
        let saved_settings = FrontendSettings::load_or_defaults(&settings_path, &assets).unwrap();
        assert_eq!(
            saved_settings.stored_player_initials(),
            Some(b"OLD".as_slice())
        );
        assert_eq!(
            saved_settings.stored_net_player_name(),
            Some(b"Saved network".as_slice())
        );
        assert_eq!(
            saved_settings.stored_player_name(),
            Some(if fail_write {
                b"Saved player".as_slice()
            } else {
                b"Restored player".as_slice()
            })
        );
        if fail_write {
            assert_eq!(std::fs::read(&settings_path).unwrap(), settings_bytes);
        }
    }
}

#[test]
fn settings_identity_keep_clears_only_the_loaded_flag_after_success() {
    for target in [IdentityTarget::PlayerName, IdentityTarget::NetPlayerName] {
        let (mut ctx, _tables, _startup) = setup("identity-keep-cache");
        let platform = ctx.session.platform_mut();
        platform.settings.set_player_name(b"Saved player");
        platform.settings.set_net_player_name(b"Saved network");
        platform.settings.set_player_initials(b"OLD");
        platform.settings.persist().unwrap();
        platform.identity.clear_loaded();
        read_identity(
            &mut platform.identity,
            &platform.settings,
            SessionPhase::Phase9,
            1,
        )
        .unwrap();
        assert!(platform.identity.loaded());
        let cache = platform.identity.clone();
        platform.settings.set_player_initials(b"LIVE");
        let exit = platform.exit_requested;
        SettingsTextEntryPlatform {
            settings: &mut platform.settings,
            identity: &mut platform.identity,
            exit_requested: &mut platform.exit_requested,
        }
        .persist_identity(IdentityWriteRequest {
            target,
            name: b"Replacement",
            initials: IdentityInitialsWrite::Keep,
        })
        .unwrap();

        let mut expected_cache = cache;
        expected_cache.clear_loaded();
        assert_eq!(platform.identity, expected_cache);
        assert_eq!(
            platform.settings.stored_player_initials(),
            Some(b"LIVE".as_slice())
        );
        assert_eq!(platform.exit_requested, exit);
        let assets = FrontendAssets::open(&root()).unwrap();
        let saved =
            FrontendSettings::load_or_defaults(ctx.directory.join("settings.json"), &assets)
                .unwrap();
        assert_eq!(saved.stored_player_initials(), Some(b"OLD".as_slice()));
        let selected = match target {
            IdentityTarget::PlayerName => saved.stored_player_name(),
            IdentityTarget::NetPlayerName => saved.stored_net_player_name(),
        };
        assert_eq!(selected, Some(b"Replacement".as_slice()));
    }
}

#[test]
fn settings_identity_failure_preserves_loaded_cache_for_both_initials_modes() {
    for target in [IdentityTarget::PlayerName, IdentityTarget::NetPlayerName] {
        for initials in [
            IdentityInitialsWrite::Keep,
            IdentityInitialsWrite::Write(b"NEW"),
        ] {
            let (mut ctx, _tables, _startup) = setup("identity-failed-cache");
            let settings_path = ctx.directory.join("settings.json");
            let platform = ctx.session.platform_mut();
            platform.settings.set_player_name(b"Saved player");
            platform.settings.set_net_player_name(b"Saved network");
            platform.settings.set_player_initials(b"OLD");
            platform.settings.persist().unwrap();
            platform.identity.clear_loaded();
            read_identity(
                &mut platform.identity,
                &platform.settings,
                SessionPhase::Phase9,
                1,
            )
            .unwrap();
            assert!(platform.identity.loaded());
            let cache = platform.identity.clone();
            let saved_bytes = std::fs::read(&settings_path).unwrap();
            platform.settings.set_player_name(b"Live player");
            platform.settings.set_net_player_name(b"Live network");
            platform.settings.set_player_initials(b"LIVE");
            platform.settings.words_mut()[0] = 1;
            let words = *platform.settings.words();
            let exit = platform.exit_requested;
            std::fs::write(settings_path.with_extension("json.tmp"), b"occupied").unwrap();

            assert!(SettingsTextEntryPlatform {
                settings: &mut platform.settings,
                identity: &mut platform.identity,
                exit_requested: &mut platform.exit_requested,
            }
            .persist_identity(IdentityWriteRequest {
                target,
                name: b"Candidate",
                initials
            })
            .is_err());

            assert_eq!(platform.identity, cache);
            assert_eq!(platform.settings.words(), &words);
            assert_eq!(
                platform.settings.stored_player_name(),
                Some(b"Live player".as_slice())
            );
            assert_eq!(
                platform.settings.stored_net_player_name(),
                Some(b"Live network".as_slice())
            );
            assert_eq!(
                platform.settings.stored_player_initials(),
                Some(b"LIVE".as_slice())
            );
            assert_eq!(platform.exit_requested, exit);
            assert_eq!(std::fs::read(&settings_path).unwrap(), saved_bytes);
        }
    }
}

#[test]
fn alt_escape_requests_exit_from_the_frontend() {
    let (mut ctx, tables, startup) = setup("exit");
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    for _ in 0..4 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    step(&mut ctx, &mut scene, resources, &[("Left Alt", true)]).unwrap();
    assert!(!ctx.session.platform().exit_requested);
    step(&mut ctx, &mut scene, resources, &[("Escape", true)]).unwrap();
    assert!(ctx.session.platform().exit_requested);
}

#[test]
fn the_player_submenu_leaves_name_entry_with_the_empty_name_cache() {
    let (mut ctx, tables, startup) = setup("identity-cache");
    ctx.session.platform_mut().settings.set_player_name(b"Bob");
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    let depth = |ctx: &Ctx| {
        ctx.session
            .frontend_screen()
            .and_then(|screen| screen.control())
            .map_or(0, |control| control.menus().len())
    };
    for _ in 0..100 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    // Root -> Options -> its fourth child, the player submenu 00539458.
    press(&mut ctx, &mut scene, resources, "Down");
    press(&mut ctx, &mut scene, resources, "Return");
    settle(&mut ctx, &mut scene, resources);
    assert_eq!(depth(&ctx), 2);
    // Options rows: 1..=3 Video, Sound and the third submenu; row 4 is the
    // 00453C70 phase-0 action; row 5 pushes the player submenu 00539458.
    for _ in 0..4 {
        press(&mut ctx, &mut scene, resources, "Down");
    }
    assert!(!ctx.session.platform().identity.loaded());
    press(&mut ctx, &mut scene, resources, "Return");
    settle(&mut ctx, &mut scene, resources);
    assert_eq!(depth(&ctx), 3);
    // Drawing 004639D0 ran 00463D10: the flag is set, the name cache empty.
    let identity = &ctx.session.platform().identity;
    assert!(identity.loaded());
    assert_eq!(identity.name_cache(), b"");
    assert_eq!(identity.net_name_cache(), b"Jameson");
    // Back out to the root and select Start Game again.
    for _ in 0..4 {
        if depth(&ctx) == 1 {
            break;
        }
        press(&mut ctx, &mut scene, resources, "Escape");
        settle(&mut ctx, &mut scene, resources);
    }
    assert_eq!(depth(&ctx), 1);
    press(&mut ctx, &mut scene, resources, "Up");
    settle(&mut ctx, &mut scene, resources);
    enter_name_entry_from_root(&mut ctx, &mut scene, resources);
    // 00463550 took the cached branch: the saved "Bob" was never read.
    let entry = ctx.session.text_screen().unwrap().entry().unwrap();
    assert_eq!(entry.name(), b"");
    assert_eq!(ctx.session.platform().identity.initials_cache(), b"0up");
}

/// Let the frontend camera settle after a menu change.
fn settle<'f, 'r>(
    ctx: &mut Ctx,
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
) {
    for _ in 0..40 {
        step(ctx, scene, resources, &[]).unwrap();
    }
}

#[test]
fn selector12_surface_snap_is_mapped_once_and_retained_by_actual_session_owners() {
    let (mut ctx, _tables, startup) = setup("selector12-surface-snap-retention");
    let authored = FrontendExecutable::parse(ctx.session.assets().executable_bytes())
        .unwrap()
        .mapped(0x0053_a128, 1)
        .unwrap()
        .bytes[0];
    assert_eq!(ctx.session.surface_snap_53a128, authored);

    // A supplied process mutation discriminates retention from reseeding.
    // No recovered native writer or production mode-to-zero claim is made.
    let supplied = authored ^ 0xa5;
    ctx.session.surface_snap_53a128 = supplied;
    let mut controller = crate::session::SessionController::default();
    controller.request_phase(SessionPhase::Frontend);
    controller.transition(&mut ctx.session).unwrap();
    let previous_frontend = ctx.session.frontend_screen().unwrap().id();
    assert_eq!(ctx.session.surface_snap_53a128, supplied);

    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut ctx.session).unwrap();
    assert!(ctx.session.frontend_screen().is_none());
    assert!(ctx.session.text_screen().is_some());
    assert_eq!(ctx.session.surface_snap_53a128, supplied);
    controller.request_phase(SessionPhase::Frontend);
    controller.transition(&mut ctx.session).unwrap();
    assert!(ctx.session.text_screen().is_none());
    assert_ne!(
        ctx.session.frontend_screen().unwrap().id(),
        previous_frontend
    );
    assert_eq!(ctx.session.surface_snap_53a128, supplied);

    ctx.session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    let mut state = controller.state().clone();
    ctx.session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    assert!(ctx.session.world().is_some());
    assert_eq!(ctx.session.surface_snap_53a128, supplied);

    let replacement = ctx.session.residency().clone();
    let generation = ctx.session.residency_generation();
    ctx.session
        .install_display(
            replacement,
            FrontendRenderContext {
                projection: startup.projection,
            },
        )
        .unwrap();
    assert_eq!(ctx.session.residency_generation(), generation + 1);
    assert_eq!(ctx.session.surface_snap_53a128, supplied);
}

#[test]
fn load_game_resumes_a_retail_save_with_its_carried_units() {
    // Retail Infest.sav record 4, "Jameson - Terra Nova": mission 2, player
    // items (2,2,2). H13 slot 2 constructs selector 12, which 0046A890
    // carries back into the vehicle's +1A4 slots.
    let (mut ctx, tables, startup) = setup("load-game");
    let snapshot = ctx.session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: ctx.session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    for _ in 0..100 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    press(&mut ctx, &mut scene, resources, "Return");
    for _ in 0..80 {
        if top_profile(&ctx).is_some() {
            break;
        }
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    for _ in 0..40 {
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    for _ in 0..16 {
        if top_profile(&ctx).unwrap().base().selection() == 4 {
            break;
        }
        press(&mut ctx, &mut scene, resources, "Down");
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    assert_eq!(top_profile(&ctx).unwrap().base().selection(), 4);
    press(&mut ctx, &mut scene, resources, "Return");
    for _ in 0..40 {
        if ctx.session.world().is_some() {
            break;
        }
        step(&mut ctx, &mut scene, resources, &[]).unwrap();
    }
    let world = ctx.session.world_mut().expect("the save's world loaded");
    assert_eq!(world.mission.save.mission_selection, 2);
    let player = world.arena.first_player().unwrap();
    assert_eq!(
        world.arena.world_player(player).unwrap().items_190,
        [2, 2, 2]
    );
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let slots = world
        .arena
        .selector2_actor(vehicle)
        .unwrap()
        .handles_1a4
        .clone();
    assert_eq!(slots.states, [Some(2); 3]);
    for handle in slots.handles {
        let item = world.arena.resolve_actor_handle(handle).unwrap().unwrap();
        assert!(world.arena.selector12_actor(item).is_ok());
        assert_eq!(world.arena.object(item).unwrap().base.flags & 0xc0, 0xc0);
    }
}

/// A world's end (00487560, as a world gate's pass calls it) runs the
/// results after a real world, then the next world's briefing. Retail save
/// record 2 ("Jameson", world 11) ends toward world 2 from start 1: Space
/// shows the stats, Space again arms the one-second exit, and phase 3 loads
/// world 2.
#[test]
fn a_world_end_reaches_the_next_worlds_briefing_through_the_results() {
    let (mut ctx, tables, startup) = setup("world-end");
    let run = |ctx: &mut Ctx, keys: &[(&str, bool)]| {
        step_rebinding(ctx, &tables, &startup, keys).unwrap();
    };
    for _ in 0..100 {
        run(&mut ctx, &[]);
    }
    run(&mut ctx, &[("Return", true)]);
    run(&mut ctx, &[("Return", false)]);
    for _ in 0..80 {
        if top_profile(&ctx).is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    for _ in 0..40 {
        run(&mut ctx, &[]);
    }
    for _ in 0..16 {
        if top_profile(&ctx).unwrap().base().selection() == 2 {
            break;
        }
        run(&mut ctx, &[("Down", true)]);
        run(&mut ctx, &[("Down", false)]);
    }
    run(&mut ctx, &[("Return", true)]);
    run(&mut ctx, &[("Return", false)]);
    for _ in 0..40 {
        if ctx.session.briefing_screen().is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    assert_eq!(ctx.session.world().unwrap().mission.current_world_a0, 11);
    for _ in 0..120 {
        if ctx.session.gameplay_screen().is_some() {
            break;
        }
        run(&mut ctx, &[("Escape", true)]);
        run(&mut ctx, &[("Escape", false)]);
    }
    assert!(ctx.session.gameplay_screen().is_some());

    {
        let session = &mut ctx.session;
        let world = session.world.as_mut().unwrap();
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        let outcome = crate::world_completion::complete_world(
            &mut world.mission,
            &mut world.arena,
            crate::world_completion::WorldCompletionRequest {
                next_selection: 2,
                start_ordinal: 1,
                actor: Some(vehicle),
            },
            crate::world_completion::WorldCompletionInputs {
                network_active: false,
                process: Some(&session.factory_process),
            },
            &mut session.sounds,
        )
        .unwrap();
        assert_eq!(
            outcome,
            crate::world_completion::WorldCompletionOutcome::Accepted {
                request_phase: SessionPhase::Phase8
            }
        );
    }
    ctx.driver.request_phase(SessionPhase::Phase8);
    for _ in 0..4 {
        if ctx.session.intro_results_screen().is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase8);
    run(&mut ctx, &[("Space", true)]);
    run(&mut ctx, &[("Space", false)]);
    let results = ctx.session.intro_results_screen().unwrap();
    assert!(results.stats_shown());
    assert!(results.panels().iter().all(Option::is_some));
    run(&mut ctx, &[("Space", true)]);
    run(&mut ctx, &[("Space", false)]);
    assert_eq!(
        ctx.session.intro_results_screen().unwrap().exit_millis() % 125,
        0
    );
    for _ in 0..16 {
        if ctx.session.briefing_screen().is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    assert!(ctx.session.intro_results_screen().is_none());
    assert!(ctx.session.briefing_screen().is_some());
    let world = ctx.session.world().unwrap();
    assert_eq!(world.mission.current_world_a0, 2);
    let phases: Vec<_> = ctx
        .driver
        .transitions()
        .iter()
        .map(|transition| (transition.previous, transition.effective))
        .collect();
    assert!(phases.contains(&(SessionPhase::Gameplay, SessionPhase::Phase8)));
    assert!(phases.contains(&(SessionPhase::Phase8, SessionPhase::Phase3)));
}

/// The computer's "Map and Objectives" (00415170) requests phase 7: the
/// gameplay screen's reuse refuses it, so the map (00450FC0) replaces it over
/// the live world, centred on the vehicle. Insert zooms it; Esc (004520A0)
/// requests phase 4, whose screen is built afresh. Retail save record 2
/// ("Jameson") is in world 11.
#[test]
fn the_map_screen_opens_over_gameplay_and_returns_to_it() {
    let (mut ctx, tables, startup) = setup("map-screen");
    let run = |ctx: &mut Ctx, keys: &[(&str, bool)]| {
        step_rebinding(ctx, &tables, &startup, keys).unwrap();
    };
    for _ in 0..100 {
        run(&mut ctx, &[]);
    }
    run(&mut ctx, &[("Return", true)]);
    run(&mut ctx, &[("Return", false)]);
    for _ in 0..80 {
        if top_profile(&ctx).is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    for _ in 0..40 {
        run(&mut ctx, &[]);
    }
    for _ in 0..16 {
        if top_profile(&ctx).unwrap().base().selection() == 2 {
            break;
        }
        run(&mut ctx, &[("Down", true)]);
        run(&mut ctx, &[("Down", false)]);
    }
    run(&mut ctx, &[("Return", true)]);
    run(&mut ctx, &[("Return", false)]);
    for _ in 0..40 {
        if ctx.session.briefing_screen().is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    for _ in 0..120 {
        if ctx.session.gameplay_screen().is_some() {
            break;
        }
        run(&mut ctx, &[("Escape", true)]);
        run(&mut ctx, &[("Escape", false)]);
    }
    assert!(ctx.session.gameplay_screen().is_some());
    let world = ctx.session.world_mut().unwrap();
    assert_eq!(world.mission.current_world_a0, 11);
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    let words = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();

    ctx.driver.request_phase(SessionPhase::Phase7);
    for _ in 0..4 {
        if ctx.session.map_screen().is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Phase7);
    assert!(ctx.session.gameplay_screen().is_none());
    let map = ctx.session.map_screen().unwrap();
    assert_eq!(map.state().center, [words[0], words[2]]);
    assert_eq!(map.state().zoom_f8, 1);
    assert!(!map.note_draw_failure_reported(), "the map draws whole");
    assert_eq!(ctx.presented.last().unwrap().screen, PortScreen::Map);
    // The world stands still under the map: the session runs the player
    // callback (00438950), not the world update.
    let seconds = ctx.driver.state().gameplay_time().seconds;
    run(&mut ctx, &[("Insert", true)]);
    run(&mut ctx, &[]);
    run(&mut ctx, &[("Insert", false)]);
    let map = ctx.session.map_screen().unwrap();
    assert!(map.state().zoom_f8 > 1, "{}", map.state().zoom_f8);
    assert!(map.state().age_c0 >= 3 * 125_000);
    assert_eq!(ctx.driver.state().gameplay_time().seconds, seconds);
    run(&mut ctx, &[("Escape", true)]);
    run(&mut ctx, &[("Escape", false)]);
    for _ in 0..4 {
        if ctx.session.gameplay_screen().is_some() {
            break;
        }
        run(&mut ctx, &[]);
    }
    assert!(ctx.session.map_screen().is_none());
    assert!(ctx.session.gameplay_screen().is_some());
    assert_eq!(ctx.driver.state().current_phase(), SessionPhase::Gameplay);
    let phases: Vec<_> = ctx
        .driver
        .transitions()
        .iter()
        .map(|transition| (transition.previous, transition.effective))
        .collect();
    assert!(phases.contains(&(SessionPhase::Gameplay, SessionPhase::Phase7)));
    assert!(phases.contains(&(SessionPhase::Phase7, SessionPhase::Gameplay)));
}
