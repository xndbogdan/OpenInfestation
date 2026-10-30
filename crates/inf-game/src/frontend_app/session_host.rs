//! The scheduled session's callbacks over SDL and the persistent renderer.
//!
//! `AppFrame` is built for each driver call because it borrows the SDL window
//! and the renderer bound to its residency snapshot. Routing, session-owned
//! effects and CPU drawing are the shared session-platform owners; this module
//! adds only the SDL surface, the display-reset commit and presentation.

use sdl2::{
    render::{Texture, TextureCreator},
    video::WindowContext,
};
use thiserror::Error;

use super::display::{copy_framebuffer, InheritedScene, Output, PreparedDisplay, SdlDisplayReset};
use crate::{
    briefing_runtime::BriefingTickEffect,
    briefing_screen::BriefingAudioRequest,
    frontend_dispatch::FrontendEffects,
    frontend_display::PresentationClear,
    frontend_renderer::{FrontendRenderContext, FrontendRendererState},
    frontend_runtime::{
        FrontendPlatformEffects, FrontendRuntimeError, LinkedTitleVideo, SdlPlatform,
        SettingsTextEntryPlatform,
    },
    frontend_scene::{FrontendScene, SceneProjection},
    menu_basis::MenuBasisMath,
    model_primitives::PrimitiveTables,
    name_entry::NameCompletion,
    name_entry_renderer::NameEntryRendererState,
    platform_input::{InputNotification, SdlInput},
    presentation::OutputPresentation,
    render_resources::RenderResources,
    scheduler::{SchedulerActivity, SchedulerTick},
    screen_registry::{RegistryError, ScreenAttachment},
    session::{
        ScreenFactory, ScreenKind, SessionFrameHost, SessionHost, SessionPhase, SessionState,
        SessionTransitionAdmission, TickCallback, TransitionEffect,
    },
    session_platform::{
        linked::{
            KeyConfigLinkedError, LinkedServices, PortInputPass, PortLinkedError,
            StartupLinkedError, TextEntryPlatform, TitleLinkedError,
        },
        software::{
            present_linked, present_linked_clear, SoftwareLinkedError, SoftwareLinkedFrame,
        },
        KeyConfigSessionEffects, LinkedActivityError, LinkedScreenError, PortLinked, PortScreen,
        PortSession, PortSessionError, PortSessionErrorOf, PortSplit, SessionBoundary,
        StartupSessionEffects,
    },
    software_frame::SoftwareFrame,
    software_scene::SoftwareScene,
};

#[derive(Debug, Error)]
pub enum AppFrameError {
    #[error(transparent)]
    Session(PortSessionErrorOf<SdlPlatform>),
    #[error(transparent)]
    Linked(PortLinkedError<FrontendRuntimeError, FrontendRuntimeError>),
    #[error(transparent)]
    TitleLinked(TitleLinkedError<FrontendRuntimeError>),
    #[error(transparent)]
    StartupLinked(StartupLinkedError),
    #[error(transparent)]
    KeyConfigLinked(KeyConfigLinkedError),
    #[error("briefing tick failed: {0}")]
    Briefing(String),
    #[error("gameplay tick failed: {0}")]
    Gameplay(String),
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error(transparent)]
    Activity(#[from] LinkedActivityError),
    #[error(transparent)]
    Draw(SoftwareLinkedError<String>),
    #[error("installing the prepared display failed: {0}")]
    Install(RegistryError),
    #[error("linked draw/presentation ran without this iteration's renderer")]
    MissingRenderer,
}

impl AppFrameError {
    /// The port-only transition refusal: the end of what is implemented.
    pub fn boundary(&self) -> Option<SessionBoundary> {
        match self {
            Self::Session(PortSessionError::Boundary(boundary)) => Some(*boundary),
            _ => None,
        }
    }
}

/// Display reset output after the session installed its residency.
pub struct StagedDisplay<'t> {
    pub frame: SoftwareFrame,
    pub texture: Option<Texture<'t>>,
    pub startup: super::FrontendAppStartup,
    pub dimensions: [u32; 2],
    /// The window's new physical size.
    pub output: [u32; 2],
}

/// Loop-owned SDL and input state lent to one driver call.
pub struct AppIo<'a, 't> {
    /// Process scratch survives input-only passes and renderer replacement.
    pub model_process: crate::model_process::ModelProcessOwners,
    pub(super) output: &'a mut Output,
    pub creator: Option<&'t TextureCreator<WindowContext>>,
    pub tables: &'a PrimitiveTables,
    pub basis: &'a MenuBasisMath,
    /// Process model globals a replacement renderer inherits.
    pub inherited: InheritedScene,
    pub successful_resets: &'a mut usize,
    pub staged: &'a mut Option<StagedDisplay<'t>>,
    pub notifications: &'a mut Vec<InputNotification>,
    pub now_micros: u64,
    pub(super) clock: &'a mut super::AppClock,
    pub(super) loading_stats: &'a mut super::loading::LoadingPresentationStats,
    pub completions: &'a mut Vec<NameCompletion>,
}

/// The renderer epoch lent to the linked +08/+0C callbacks.
pub struct AppRender<'a, 't, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    /// The database's second residency (a port fix), while one is loaded.
    pub database_resources: Option<RenderResources<'r>>,
    pub texture: &'a mut Option<Texture<'t>>,
    pub dimensions: [u32; 2],
    pub presentation: OutputPresentation,
    pub frontend_scene: &'a FrontendScene,
    pub sine_quarter: &'a [u16; 4096],
    pub template_projection: SceneProjection,
    pub loading_model_template: &'a crate::loading_model::LoadingModelTemplate,
    pub frontend_state: &'a mut FrontendRendererState,
    pub name_entry_state: &'a mut NameEntryRendererState,
    /// Copy of the last presented pixels and their size, when requested.
    pub snapshot: Option<&'a mut FrameSnapshot>,
    pub(super) capture: Option<crate::session_capture::CaptureHandle>,
}

/// The last presented frame's pixels and their size, for a run's
/// snapshots. A Vulkan frame's stay on the device, copied at each present,
/// until a snapshot reads them: a run reads back only the frames it saves.
#[derive(Debug)]
pub struct FrameSnapshot {
    pixels: Vec<u8>,
    size: [u32; 2],
    staged: Option<crate::vk_device::StagedRgba>,
}

impl FrameSnapshot {
    pub fn new(size: [u32; 2]) -> Self {
        Self {
            pixels: Vec::new(),
            size,
            staged: None,
        }
    }

    /// Keep `frame`'s pixels, just presented.
    fn keep(&mut self, frame: &mut SoftwareFrame) {
        match frame.stage_rgba() {
            crate::software_frame::StagedPixels::Read(pixels) => {
                self.pixels.clear();
                self.pixels.extend_from_slice(&pixels);
                self.staged = None;
            }
            crate::software_frame::StagedPixels::Device(staged) => self.staged = Some(staged),
        }
        self.size = frame.read_size();
    }

    /// The last presented frame's pixels and their size.
    pub fn pixels(&mut self) -> (&[u8], [u32; 2]) {
        if let Some(staged) = self.staged.take() {
            self.pixels = staged.read();
        }
        (&self.pixels, self.size)
    }
}

pub struct AppFrame<'a, 't, 'f, 'r> {
    pub session: &'a mut PortSession<SdlPlatform>,
    pub io: AppIo<'a, 't>,
    pub render: Option<AppRender<'a, 't, 'f, 'r>>,
}

#[derive(Clone, Copy)]
enum LinkedCall {
    Poll,
    Tick(SchedulerTick),
}

impl<'t> AppFrame<'_, 't, '_, '_> {
    /// A display reset made inside the callback is committed afterwards, even
    /// when the callback then failed: 00437DF0 is synchronous in retail and
    /// the physical window already changed, so the transition, cues and the
    /// draw that follow must see the new display.
    fn run_linked(
        &mut self,
        state: &mut SessionState,
        call: LinkedCall,
    ) -> Result<(), AppFrameError> {
        let screen = self.session.linked_screen()?;
        let mut pending: Option<PreparedDisplay<'t>> = None;
        let result = {
            let PortSplit {
                mut linked,
                residency,
                platform,
                world,
                sounds,
            } = self.session.split();
            let SdlPlatform {
                frontend,
                keyboard,
                profiles,
                ..
            } = platform;
            let io = &mut self.io;
            if matches!(call, LinkedCall::Poll) {
                // The network threads' bodies (00493C00, 00493CC0, 004934C0).
                if let Err(error) = frontend
                    .network
                    .pump_default(io.now_micros, std::time::Instant::now())
                {
                    eprintln!("Network: the session stopped: {error}");
                }
            }
            if keyboard.is_none() {
                // Read again next frame; there is no device to take it yet.
                io.notifications
                    .retain(|notification| !matches!(notification, InputNotification::Pad { .. }));
            }
            match screen {
                PortScreen::Startup => {
                    // The startup screen borrows the keyboard for notifications
                    // and the settings words beside it; its accept reselects
                    // the live residency (004EE160) and requests phase 1.
                    let mut effects = StartupSessionEffects {
                        settings: &mut frontend.settings,
                        residency,
                        state: &mut *state,
                    };
                    match call {
                        LinkedCall::Poll => linked.poll_startup(
                            PortInputPass {
                                keyboard: keyboard.as_mut(),
                                notifications: &mut *io.notifications,
                                now_micros: io.now_micros,
                            },
                            &mut effects,
                        ),
                        LinkedCall::Tick(tick) => linked.tick_startup(tick, &mut effects),
                    }
                    .map_err(AppFrameError::StartupLinked)?;
                    Ok(())
                }
                PortScreen::KeyConfig => {
                    // The keyboard serves both the notifications and the
                    // capture's raw state (adapter cache slot 0).
                    let adapter = keyboard
                        .as_ref()
                        .map(|keyboard| keyboard.frontend_device().source);
                    let process = linked.process;
                    let owner_byte = linked.owner_byte;
                    let mut effects = KeyConfigSessionEffects {
                        settings: &mut frontend.settings,
                        state: &mut *state,
                        process,
                        owner_byte,
                        basis: io.basis,
                    };
                    match call {
                        LinkedCall::Poll => linked.poll_key_config(
                            PortInputPass {
                                keyboard: keyboard.as_mut(),
                                notifications: &mut *io.notifications,
                                now_micros: io.now_micros,
                            },
                            &mut effects,
                            &mut frontend.exit_requested,
                        ),
                        LinkedCall::Tick(tick) => {
                            linked.tick_key_config(tick, adapter, &mut effects)
                        }
                    }
                    .map_err(AppFrameError::KeyConfigLinked)?;
                    Ok(())
                }
                PortScreen::Frontend => {
                    let basis = io.basis;
                    let mut display = SdlDisplayReset {
                        model_process: io.model_process.clone(),
                        active: residency,
                        pending: &mut pending,
                        output: &mut *io.output,
                        creator: io.creator,
                        tables: io.tables,
                        inherited: io.inherited,
                        successful_resets: &mut *io.successful_resets,
                        scaling: frontend.settings.scaling(),
                        frame_pin: frontend.settings.frame_pin(),
                    };
                    let mut effects = FrontendPlatformEffects {
                        platform: frontend,
                        display: &mut display,
                    };
                    let services: LinkedServices<'_, _, SettingsTextEntryPlatform<'_>> =
                        LinkedServices::Frontend {
                            effects: &mut effects,
                            profiles,
                            basis,
                        };
                    let keyboard = keyboard.as_mut();
                    // The display host holds the output; dispatch needs only
                    // the notification/completion fields.
                    let mut rest = AppIoRest {
                        notifications: &mut *io.notifications,
                        now_micros: io.now_micros,
                        completions: &mut *io.completions,
                    };
                    dispatch(&mut linked, state, call, services, keyboard, &mut rest)
                }
                PortScreen::TextEntry => {
                    let adapter = keyboard
                        .as_ref()
                        .map(|keyboard| keyboard.frontend_device().source);
                    let mut platform = SettingsTextEntryPlatform {
                        settings: &mut frontend.settings,
                        identity: &mut frontend.identity,
                        exit_requested: &mut frontend.exit_requested,
                    };
                    let services: LinkedServices<
                        '_,
                        FrontendPlatformEffects<'_, SdlDisplayReset<'_, '_>>,
                        _,
                    > = LinkedServices::TextEntry {
                        platform: &mut platform,
                        profiles,
                        rng: &mut frontend.random_state,
                        keyboard: adapter,
                    };
                    let mut rest = AppIoRest {
                        notifications: &mut *io.notifications,
                        now_micros: io.now_micros,
                        completions: &mut *io.completions,
                    };
                    dispatch(
                        &mut linked,
                        state,
                        call,
                        services,
                        keyboard.as_mut(),
                        &mut rest,
                    )
                }
                PortScreen::Title => {
                    // The linked title borrows the keyboard adapter for
                    // notifications, the exit flag beside it, and the player
                    // state plus the mixer disjointly: it never borrows the
                    // whole platform while the keyboard is out.
                    let SdlPlatform {
                        frontend,
                        keyboard,
                        title_video,
                        ..
                    } = &mut *platform;
                    let pass = PortInputPass {
                        keyboard: keyboard.as_mut(),
                        notifications: &mut *io.notifications,
                        now_micros: io.now_micros,
                    };
                    let mut video = LinkedTitleVideo {
                        state: title_video,
                        audio: &mut frontend.audio,
                    };
                    match call {
                        LinkedCall::Poll => linked
                            .poll_title(state, pass, &mut video, &mut frontend.exit_requested)
                            .map_err(AppFrameError::TitleLinked)?,
                        LinkedCall::Tick(tick) => {
                            linked
                                .tick_title(state, tick, &mut video)
                                .map_err(AppFrameError::TitleLinked)?;
                        }
                    }
                    Ok(())
                }
                PortScreen::Briefing => {
                    let world = world.as_mut().ok_or_else(|| {
                        AppFrameError::Briefing("briefing world is not retained".into())
                    })?;
                    let mut audio = |request, rng: &mut u32| match request {
                        BriefingAudioRequest::ResumeMusic => frontend
                            .music
                            .resume(frontend.settings.music_mut(), &mut frontend.audio)
                            .map_err(|error| error.to_string()),
                        BriefingAudioRequest::Cue { slot } => frontend
                            .audio
                            .play_resident_cue(
                                residency,
                                slot as i16,
                                frontend.settings.fx_volume(),
                                rng,
                            )
                            .map(|_| ())
                            .map_err(|error| error.to_string()),
                    };
                    let effects = match call {
                        LinkedCall::Poll => linked
                            .poll_briefing(
                                PortInputPass {
                                    keyboard: keyboard.as_mut(),
                                    notifications: &mut *io.notifications,
                                    now_micros: io.now_micros,
                                },
                                &mut frontend.exit_requested,
                            )
                            .map_err(|error| AppFrameError::Briefing(error.to_string()))?
                            .into_iter()
                            .map(|cue| BriefingTickEffect::PlayCue { cue })
                            .collect(),
                        LinkedCall::Tick(tick) => {
                            // 0040D54E: after its phase-4 request the finish
                            // calls 0048E7F0, whose 00439530 resets the shared
                            // animation clock; tick_briefing applies it in
                            // order with the tick's own advance.
                            let outcome = linked
                                .tick_briefing(state, tick, world, &mut audio)
                                .map_err(|error| AppFrameError::Briefing(error.to_string()))?;
                            // 0040CE1F..0040CE25: the briefing tick ends in
                            // the music service.
                            frontend
                                .service_music()
                                .map_err(|error| AppFrameError::Briefing(error.to_string()))?;
                            outcome.effects
                        }
                    };
                    for effect in effects {
                        match effect {
                            BriefingTickEffect::PlayCue { cue } => {
                                frontend.audio.play_resident_cue(residency, cue as i16,
                                    frontend.settings.fx_volume(), &mut world.rng)
                                    .map_err(|error| AppFrameError::Briefing(error.to_string()))?;
                            }
                            BriefingTickEffect::UnownedRecord { index, raw_kind } =>
                                eprintln!("Briefing record {index} kind {raw_kind} skipped: native consumer is unowned"),
                            BriefingTickEffect::UnownedTextIncrement { index } =>
                                eprintln!("Briefing record {index} skipped: increment-only text has no defined predecessor"),
                            _ => {}
                        }
                    }
                    Ok(())
                }
                // Gameplay (`00442B50`): Poll feeds the bound keys to the
                // player's input references; Tick runs the screen tick, whose
                // world tick fills the control block and updates the vehicle.
                PortScreen::Gameplay => {
                    match call {
                        LinkedCall::Poll => {
                            if let Some(world) = world.as_mut() {
                                let cheats_84 = frontend
                                    .settings
                                    .fixes()
                                    .menu_cheat_word(frontend.settings.words()[0x84 / 4]);
                                linked
                                    .poll_gameplay(
                                        state,
                                        PortInputPass {
                                            keyboard: keyboard.as_mut(),
                                            notifications: &mut *io.notifications,
                                            now_micros: io.now_micros,
                                        },
                                        world,
                                        sounds,
                                        &mut frontend.exit_requested,
                                        frontend.settings.words_mut(),
                                        cheats_84,
                                    )
                                    .map_err(|error| AppFrameError::Gameplay(error.to_string()))?;
                            } else {
                                io.notifications.clear();
                            }
                        }
                        LinkedCall::Tick(tick) => {
                            // 00442B50 gates the devices before its worker.
                            if crate::gameplay_screen::gameplay_device_gates(
                                keyboard.is_some(),
                                keyboard.as_ref().is_some_and(SdlInput::joystick_live),
                                &mut frontend.settings.words_mut()[0x8 / 4],
                            ) {
                                state.request_phase(SessionPhase::Phase5);
                            }
                            linked
                                .tick_gameplay(state, tick, world.as_mut())
                                .map_err(|error| AppFrameError::Gameplay(error.to_string()))?;
                        }
                    }
                    // The shown message's interface cues (004391D0).
                    if let Some(world) = world.as_mut() {
                        for cue in world.arena.take_ui_cues() {
                            frontend
                                .audio
                                .play_resident_cue(
                                    residency,
                                    cue as i16,
                                    frontend.settings.fx_volume(),
                                    &mut world.rng,
                                )
                                .map_err(|error| AppFrameError::Gameplay(error.to_string()))?;
                        }
                    }
                    Ok(())
                }
                PortScreen::IntroResults => {
                    let world = world
                        .as_mut()
                        .ok_or_else(|| AppFrameError::Gameplay("results world missing".into()))?;
                    match call {
                        LinkedCall::Poll => linked.poll_intro_results(
                            PortInputPass {
                                keyboard: keyboard.as_mut(),
                                notifications: &mut *io.notifications,
                                now_micros: io.now_micros,
                            },
                            &mut frontend.exit_requested,
                            Some(&mut *profiles),
                        ),
                        LinkedCall::Tick(tick) => {
                            linked.tick_intro_results(state, tick, world, Some(&mut *profiles))
                        }
                    }
                    .map_err(|error| AppFrameError::Gameplay(error.to_string()))?;
                    // The profile menu's interface cues (004391D0).
                    for cue in linked.intro_results_cues() {
                        frontend
                            .audio
                            .play_resident_cue(
                                residency,
                                cue,
                                frontend.settings.fx_volume(),
                                &mut world.rng,
                            )
                            .map_err(|error| AppFrameError::Gameplay(error.to_string()))?;
                    }
                    Ok(())
                }
                // The map (`00451020`): its own callbacks and tick `00451F30`.
                PortScreen::Map => {
                    match call {
                        LinkedCall::Poll => linked.poll_map(
                            state,
                            PortInputPass {
                                keyboard: keyboard.as_mut(),
                                notifications: &mut *io.notifications,
                                now_micros: io.now_micros,
                            },
                            &mut frontend.exit_requested,
                        ),
                        LinkedCall::Tick(tick) => linked.tick_map(tick),
                    }
                    .map_err(|error| AppFrameError::Gameplay(error.to_string()))?;
                    Ok(())
                }
            }
        };
        let committed = self.commit_display(pending);
        result.map_err(AppFrameError::Linked)?;
        committed
    }

    fn commit_display(
        &mut self,
        pending: Option<PreparedDisplay<'t>>,
    ) -> Result<(), AppFrameError> {
        let Some(next) = pending else {
            return Ok(());
        };
        self.session
            .install_display(
                next.runtime,
                FrontendRenderContext {
                    projection: next.startup.projection,
                },
            )
            .map_err(AppFrameError::Install)?;
        *self.io.staged = Some(StagedDisplay {
            frame: next.frame,
            texture: next.texture,
            startup: next.startup,
            dimensions: next.dimensions,
            output: next.output,
        });
        Ok(())
    }
}

/// The dispatch inputs that do not alias the display host's output.
struct AppIoRest<'a> {
    notifications: &'a mut Vec<InputNotification>,
    now_micros: u64,
    completions: &'a mut Vec<NameCompletion>,
}

fn dispatch<E: FrontendEffects, N: TextEntryPlatform>(
    linked: &mut PortLinked<'_>,
    state: &mut SessionState,
    call: LinkedCall,
    services: LinkedServices<'_, E, N>,
    keyboard: Option<&mut SdlInput>,
    io: &mut AppIoRest<'_>,
) -> Result<(), PortLinkedError<E::Error, N::Error>> {
    match call {
        LinkedCall::Poll => linked.poll(
            state,
            PortInputPass {
                keyboard,
                notifications: &mut *io.notifications,
                now_micros: io.now_micros,
            },
            services,
            &mut *io.completions,
        ),
        LinkedCall::Tick(tick) => linked.tick(state, tick, services, &mut *io.completions),
    }
}

impl SessionHost for AppFrame<'_, '_, '_, '_> {
    type Screen = PortScreen;
    type ScreenContext = ScreenAttachment;
    type Error = AppFrameError;

    fn current_screen(&self) -> Option<Self::Screen> {
        self.session.current_screen()
    }
    fn screen_kind(
        &mut self,
        screen: Self::Screen,
        state: &mut SessionState,
    ) -> Result<ScreenKind, Self::Error> {
        self.session
            .screen_kind(screen, state)
            .map_err(AppFrameError::Session)
    }
    fn screen_context(&self, screen: Self::Screen) -> Self::ScreenContext {
        self.session.screen_context(screen)
    }
    fn reuse_screen(
        &mut self,
        screen: Self::Screen,
        requested: SessionPhase,
        state: &mut SessionState,
    ) -> Result<bool, Self::Error> {
        self.session
            .reuse_screen(screen, requested, state)
            .map_err(AppFrameError::Session)
    }
    fn destroy_screen(
        &mut self,
        screen: Self::Screen,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        self.session
            .destroy_screen(screen, state)
            .map_err(AppFrameError::Session)
    }
    fn network_session_active(&self) -> bool {
        SessionHost::network_session_active(&*self.session)
    }
    fn transition_effect(
        &mut self,
        effect: TransitionEffect,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        if effect == TransitionEffect::LoadWorld {
            let mut loading = super::loading::SdlLoadingPresentation {
                clock: &mut *self.io.clock,
                output: &mut *self.io.output,
                render: self.render.as_mut(),
                stats: &mut *self.io.loading_stats,
                basis: self.io.basis,
            };
            self.session
                .load_world_effect(
                    state,
                    crate::session_platform::loading::WorldLoadPresentation::Interactive(
                        &mut loading,
                    ),
                )
                .map_err(AppFrameError::Session)
        } else {
            self.session
                .transition_effect(effect, state)
                .map_err(AppFrameError::Session)
        }
    }
    fn create_screen(
        &mut self,
        factory: ScreenFactory,
        context: Self::ScreenContext,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        self.session
            .create_screen(factory, context, state)
            .map_err(AppFrameError::Session)
    }
    fn tick(
        &mut self,
        callback: TickCallback,
        delta_micros: i32,
        elapsed_micros: i32,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        SessionHost::tick(
            &mut *self.session,
            callback,
            delta_micros,
            elapsed_micros,
            state,
        )
        .map_err(AppFrameError::Session)
    }
}

impl SessionFrameHost for AppFrame<'_, '_, '_, '_> {
    fn input_is_selected(&self) -> bool {
        self.session.input_is_selected()
    }
    fn poll_input(&mut self, state: &mut SessionState) -> Result<(), Self::Error> {
        self.session.sync_network_process();
        self.run_linked(state, LinkedCall::Poll)?;
        self.session.sync_network_process();
        self.session.after_input_poll(state);
        Ok(())
    }
    fn admit_transition(
        &mut self,
        state: &SessionState,
    ) -> Result<SessionTransitionAdmission, Self::Error> {
        // A menu callback can start or end the session before its request.
        self.session.sync_network_process();
        self.session
            .admit_frame_transition(state)
            .map_err(|boundary| AppFrameError::Session(PortSessionError::Boundary(boundary)))
    }
    fn linked_tick(
        &mut self,
        tick: SchedulerTick,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        // The start harness makes its phase requests before the screen ticks.
        self.session.advance_harness(state);
        self.run_linked(state, LinkedCall::Tick(tick))
    }
    fn linked_callback_08(&mut self, state: &mut SessionState) -> Result<(), Self::Error> {
        if self.session.holds_gameplay_frame(state) {
            return Ok(());
        }
        let render = self.render.as_mut().ok_or(AppFrameError::MissingRenderer)?;
        SoftwareLinkedFrame {
            scene: &mut *render.scene,
            resources: render.resources,
            database_resources: render.database_resources,
            frontend_scene: render.frontend_scene,
            sine_quarter: render.sine_quarter,
            template_projection: render.template_projection,
            frontend_state: &mut *render.frontend_state,
            name_entry_state: &mut *render.name_entry_state,
            surface: render
                .presentation
                .surface(self.io.output.window(), render.dimensions),
        }
        .draw(self.session, state)
        .map_err(AppFrameError::Draw)?;
        Ok(())
    }
    fn linked_callback_0c(&mut self, state: &mut SessionState) -> Result<(), Self::Error> {
        // The window keeps the frame the world's last tick drew.
        if self.session.holds_gameplay_frame(state) {
            return Ok(());
        }
        // Title presentation 00404840 is an empty `ret 4`: DirectShow
        // presented directly, so the game presented nothing. The port
        // presents the blitted title frame here instead and clears packed
        // black (no H7 lookup under title residency); with no video held
        // the early return stands, preserving the old behavior.
        let linked = self.session.linked_screen()?;
        let title = linked == PortScreen::Title;
        if title && !self.session.platform().title_video.is_present() {
            return Ok(());
        }
        // 00433620: the startup presents and clears with its +1A8 word.
        let startup_clear = match self.session.startup_screen() {
            Some(screen) if linked == PortScreen::Startup => Some(
                crate::frontend_display::presentation_word_rgba(screen.state().clear_color),
            ),
            _ => None,
        };
        let render = self.render.as_mut().ok_or(AppFrameError::MissingRenderer)?;
        let output = &mut *self.io.output;
        let texture = render.texture.as_mut();
        let presentation = render.presentation;
        let snapshot = render.snapshot.as_deref_mut();
        let capture = render.capture.as_ref();
        let forced_black = title || self.session.gameplay_forces_black();
        let world_observation = capture.and_then(|_| {
            self.session
                .world()
                .map(|world| crate::session_capture::WorldPresentationState {
                    session_phase: state.current_phase().id(),
                    current_world: world.mission.current_world_a0,
                    forced_black,
                    animation_clock: self.session.frontend_process().animation().value(),
                    timing_average_microseconds: world.fade.performance.average,
                    particle_load_factor_16_16: world.fade.performance.particle_load_factor,
                    manager_far_1c: world.mission.dword_1c,
                    manager_header_20: world.mission.dword_20,
                    retained_predraw_5c8_5cc: world.fade.retained_predraw_words(),
                    terrain_spacing_units: world.terrain.sampling().spacing.units(),
                    renderer_view_eye: render.scene.environment.view.eye,
                    renderer_projection_fade: render.scene.environment.projection.fade,
                })
        });
        let present = |frame: &mut SoftwareFrame| {
            copy_framebuffer(output, texture, frame, presentation.scaling)?;
            if let Some(snapshot) = snapshot {
                // Native CPU pixels, or the device frame at its drawn size.
                snapshot.keep(frame);
            }
            super::display::present_observed(output, capture, "linked", world_observation);
            if let Some(capture) = capture {
                capture.check().map_err(|error| error.to_string())?;
            }
            Ok(())
        };
        match startup_clear {
            Some(clear) => present_linked_clear(&mut *render.scene, clear, present),
            None => present_linked(
                &mut *render.scene,
                render.resources,
                if linked == PortScreen::Gameplay {
                    PresentationClear::Gameplay
                } else {
                    PresentationClear::Standard
                },
                forced_black,
                present,
            ),
        }
        .map_err(AppFrameError::Draw)
    }
    fn linked_activity(
        &mut self,
        activity: SchedulerActivity,
        _: &mut SessionState,
    ) -> Result<(), Self::Error> {
        Ok(self.session.linked_activity(activity)?)
    }
}
