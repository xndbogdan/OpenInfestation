//! The linked +08 draw and +0C presentation over the persistent CPU renderer.
//!
//! The host owns the renderer, its residency snapshot and the surface; this
//! glue resolves the registry's linked screen for every callback and hands the
//! actual screen to its concrete frame owner. The SDL loop and the headless
//! session tests share it.

use thiserror::Error;

use super::{LinkedScreenError, PortLinkedDrawError, PortScreen, PortSession};
use crate::session::SessionPhase;
use crate::{
    briefing_renderer::{BriefingRendererError, SoftwareBriefingFrame},
    frontend_display::{presentation_clear_rgba, PresentationClear},
    frontend_frame::{FrontendFrameError, FrontendFrameOutcome},
    frontend_renderer::{FrontendRendererError, FrontendRendererState, SoftwareFrontendFrame},
    frontend_scene::{FrontendScene, SceneProjection},
    frontend_screen::FrontendScreenDraw,
    frontend_settings::FrontendSettings,
    frontend_video::FrontendVideoState,
    intro_results_screen::{
        IntroResultsDrawHost, IntroResultsDrawOutcome, IntroResultsDrawRequest,
    },
    menu::{
        MenuBasis, MenuColor, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
        MenuError, MenuRowDrawRequest, MenuState, MenuVisibility,
    },
    menu_text::INITIAL_GLYPH_BASE_WORD,
    name_entry_frame::NameEntryFrameError,
    name_entry_renderer::{
        NameEntryRendererError, NameEntryRendererState, SoftwareNameEntryContext,
        SoftwareNameEntryFrame,
    },
    player_identity::PlayerIdentity,
    profile_draw::{self, ProfileRowDraw},
    profile_menu::{ProfileMenuState, ProfileTimer},
    profile_store::ProfileStore,
    render_resources::{RenderResourceError, RenderResources},
    scene_resources::{ResidentSceneHost, ResolvedSceneCommand, SceneResourceBackend},
    screen_renderer_preparation::ScreenRendererPreparationHost,
    software_frame::{SoftwareFrame, SoftwareSurface},
    software_menu::{SoftwareMenu, SoftwareMenuRequest, SoftwareMenuState},
    software_scene::{SoftwareRendererId, SoftwareScene, SoftwareSceneError},
    text_entry_screen::TextEntryScreenDraw,
    title_screen::{TitleDrawOutcome, TitleEffects, TitleScreenError},
};

/// Process owners the frontend rows read while drawing.
pub struct FrontendDrawParts<'a> {
    pub profiles: &'a mut ProfileStore,
    pub settings: &'a FrontendSettings,
    pub identity: &'a mut PlayerIdentity,
    pub video: &'a mut FrontendVideoState,
}

pub trait PortDrawPlatform {
    fn frontend_draw_parts(&mut self) -> FrontendDrawParts<'_>;
    /// Title-video player state plus the mixer for the linked draw. Platforms
    /// without video report none, which keeps the preamble-only draw exactly
    /// as before.
    fn title_video_sink(&mut self) -> Option<TitleVideoSink<'_>> {
        None
    }
    /// Gameplay borrows the actual settings and optional mixer together, so
    /// later light callbacks do not require a second mutable platform loan.
    fn world_draw_parts(&mut self) -> WorldDrawParts<'_>;
}

pub struct WorldDrawParts<'a> {
    pub settings: &'a FrontendSettings,
    pub audio: Option<&'a mut crate::platform_audio::PlatformAudio>,
    /// The video devices, whose name the in-world Options' device row shows.
    pub video: Option<&'a mut crate::frontend_video::FrontendVideoState>,
    /// The network session, whose game the network HUD shows.
    pub network: Option<&'a mut crate::network::link::NetworkLink>,
}

impl PortDrawPlatform for crate::frontend_runtime::SdlPlatform {
    fn frontend_draw_parts(&mut self) -> FrontendDrawParts<'_> {
        FrontendDrawParts {
            profiles: &mut self.profiles,
            settings: &self.frontend.settings,
            identity: &mut self.frontend.identity,
            video: &mut self.frontend.video,
        }
    }

    fn title_video_sink(&mut self) -> Option<TitleVideoSink<'_>> {
        Some(TitleVideoSink {
            state: &mut self.title_video,
            audio: &mut self.frontend.audio,
        })
    }
    fn world_draw_parts(&mut self) -> WorldDrawParts<'_> {
        WorldDrawParts {
            settings: &self.frontend.settings,
            audio: Some(&mut self.frontend.audio),
            video: Some(&mut self.frontend.video),
            network: Some(&mut self.frontend.network),
        }
    }
}

/// Concrete title-video inputs for the linked draw: player state plus the
/// mixer, borrowed disjointly from the platform by the frame host.
pub struct TitleVideoSink<'a> {
    pub state: &'a mut crate::title_playback::TitleVideoState,
    pub audio: &'a mut dyn crate::platform_audio::TitleAudioSink,
}

#[derive(Debug)]
pub enum LinkedDrawOutcome {
    Startup(crate::startup_draw::StartupDrawOutcome),
    KeyConfig(crate::menu::MenuDrawOutcome),
    Title(TitleDrawOutcome),
    Frontend(FrontendFrameOutcome),
    NameEntry,
    Briefing,
    Gameplay,
    IntroResults,
    Map,
}

#[derive(Debug, Error)]
pub enum SoftwareLinkedError<E> {
    #[error("intro results draw: {0}")]
    IntroResults(String),
    #[error("map draw: {0}")]
    Map(String),
    #[error("startup draw: {0}")]
    Startup(String),
    #[error("key configuration draw: {0}")]
    KeyConfig(String),
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error("the session owns no {0:?} screen to draw")]
    MissingScreen(PortScreen),
    #[error(transparent)]
    Frontend(PortLinkedDrawError<FrontendFrameError<FrontendRendererError>>),
    #[error(transparent)]
    NameEntry(PortLinkedDrawError<NameEntryFrameError<NameEntryRendererError>>),
    #[error(transparent)]
    TitleDraw(PortLinkedDrawError<TitlePreambleError>),
    #[error(transparent)]
    BriefingDraw(PortLinkedDrawError<BriefingRendererError>),
    #[error(transparent)]
    Clear(#[from] RenderResourceError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Executable(#[from] inf_formats::FrontendExecutableError),
    #[error("surface presentation failed")]
    Present(E),
}

/// Combined title-draw host: the arena preamble through the CPU scene plus
/// real frame presentation through the player state and the mixer. Linked
/// draws never open, poll, close, or request phases through this host.
struct TitleDrawHost<'a, 'f, 'r> {
    preamble: TitleScenePreamble<'a, 'f, 'r>,
    state: &'a mut crate::title_playback::TitleVideoState,
    audio: &'a mut dyn crate::platform_audio::TitleAudioSink,
}

impl TitleEffects for TitleDrawHost<'_, '_, '_> {
    type Error = TitlePreambleError;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        self.state.present_frame(self.audio);
        Ok(())
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        self.preamble.draw_preamble()
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn request_phase(
        &mut self,
        _previous: SessionPhase,
        _requested: SessionPhase,
    ) -> Result<(), Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
}

/// The title draw's `00439270` preamble over the persistent CPU renderer:
/// replace the command arena with `0xC8000` bytes. The native preamble also
/// resets the device batch (`004A4400`) and publishes the renderer
/// (`00546E88`): the replaced queue drops any pending batch contents, and
/// the single CPU backend needs no publication. No model, text or video
/// submission happens here; a held video frame is presented by the missing
/// DirectShow backend, never through this surface.
struct TitleScenePreamble<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
}

#[derive(Debug, Error)]
pub enum TitlePreambleError {
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error("the linked title screen stopped before its draw")]
    ScreenNotReady,
    #[error("the title draw invokes only the arena preamble on this backend")]
    UnexpectedCall,
}

impl TitleEffects for TitleScenePreamble<'_, '_, '_> {
    type Error = TitlePreambleError;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        self.scene.replace_command_arena(0xc8000)?;
        Ok(())
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
    fn request_phase(
        &mut self,
        _previous: SessionPhase,
        _requested: SessionPhase,
    ) -> Result<(), Self::Error> {
        Err(TitlePreambleError::UnexpectedCall)
    }
}

/// One linked callback's view of the persistent renderer and its surface.
pub struct SoftwareLinkedFrame<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    /// The renderer's own residency snapshot, never the session's live one.
    pub resources: RenderResources<'r>,
    /// The database's second residency snapshot (a port fix), if loaded.
    pub database_resources: Option<RenderResources<'r>>,
    pub frontend_scene: &'a FrontendScene,
    pub sine_quarter: &'a [u16; 4096],
    /// Shared template 005474E8 inputs (focal 00546F20/24 and the display)
    /// for the name-entry backdrop, never the caller context's projection.
    pub template_projection: SceneProjection,
    pub frontend_state: &'a mut FrontendRendererState,
    pub name_entry_state: &'a mut NameEntryRendererState,
    pub surface: SoftwareSurface<'a>,
}

/// The complete 00439270/0042C2B0 intro prefix consumes no frontend menu,
/// profile or caller context. Publication uses the existing retained slot.
struct SoftwareIntroResultsDraw<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    state: &'a mut FrontendRendererState,
    surface: SoftwareSurface<'a>,
}

impl SoftwareIntroResultsDraw<'_, '_, '_> {
    fn selected(&self, renderer: SoftwareRendererId) -> Result<(), FrontendRendererError> {
        if renderer != self.scene.id() {
            return Err(FrontendRendererError::Renderer);
        }
        Ok(())
    }
}

impl ScreenRendererPreparationHost<FrontendRendererError> for SoftwareIntroResultsDraw<'_, '_, '_> {
    type Renderer = SoftwareRendererId;

    fn renderer(&mut self) -> Result<Self::Renderer, FrontendRendererError> {
        Ok(self.scene.id())
    }

    fn replace_command_arena(
        &mut self,
        renderer: Self::Renderer,
        bytes: usize,
    ) -> Result<(), FrontendRendererError> {
        self.selected(renderer)?;
        self.scene.replace_command_arena(
            bytes
                .try_into()
                .map_err(|_| FrontendRendererError::ArenaSize)?,
        )?;
        Ok(())
    }

    fn begin_batch(&mut self, renderer: Self::Renderer) -> Result<(), FrontendRendererError> {
        self.selected(renderer)?;
        self.scene.submit(ResolvedSceneCommand::BeginBatch)?;
        Ok(())
    }

    fn publish_current_renderer(
        &mut self,
        renderer: Self::Renderer,
    ) -> Result<(), FrontendRendererError> {
        self.selected(renderer)?;
        self.state.published_renderer = Some(renderer);
        Ok(())
    }
}

impl IntroResultsDrawHost<FrontendRendererError> for SoftwareIntroResultsDraw<'_, '_, '_> {
    fn surface_width_pixels(&mut self) -> Result<u32, FrontendRendererError> {
        Ok(self.surface.dimensions(self.scene.pixels.frame)[0])
    }
}

/// 0042CA9C: the results screen's controller draw 00453610 over +100, whose
/// only menu is the profile menu 00449A00 pushes.
struct SoftwareResultsMenuDraw<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    /// The display globals 00546F20/24.
    focal: [i32; 2],
    process: &'a crate::frontend_screen::FrontendProcessState,
    state: &'a mut SoftwareMenuState,
    profiles: &'a mut ProfileStore,
    owner_flag: bool,
}

impl<'f, 'r> SoftwareResultsMenuDraw<'_, 'f, 'r> {
    fn menu(&mut self) -> Result<SoftwareMenu<'_, 'f, 'r>, FrontendRendererError> {
        Ok(SoftwareMenu::new(SoftwareMenuRequest {
            scene: self.scene,
            resources: self.resources,
            focal: self.focal,
            process: self.process,
            state: &mut *self.state,
        })?)
    }
}

impl MenuDrawHost for SoftwareResultsMenuDraw<'_, '_, '_> {
    type Error = FrontendRendererError;
    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        Ok(self.menu()?.begin_controller(setup)?)
    }
    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        Ok(self.menu()?.publish_menu_basis(basis)?)
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok(self.menu()?.clip_rect()?)
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        Ok(MenuDrawHost::set_clip_rect(&mut self.menu()?, clip)?)
    }
    fn colors(&self) -> MenuDrawColors {
        self.process.colors().expect("initialized menu colors")
    }
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error> {
        Ok(self.menu()?.draw_row(request, menu)?)
    }
    fn begin_profile_draw(&mut self) -> Result<(), MenuError<Self::Error>> {
        profile_draw::prepare_profile_draw(&self.resources)
            .map_err(FrontendRendererError::from)
            .map_err(MenuError::Host)
    }
    fn profile_store(&mut self) -> Result<&mut ProfileStore, MenuError<Self::Error>> {
        Ok(self.profiles)
    }
    fn draw_profile_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        base: &mut MenuState,
        state: &mut ProfileMenuState,
    ) -> Result<(), MenuError<Self::Error>> {
        let surface_pixels = [
            self.scene.pixels.frame.width() as i32,
            self.scene.pixels.frame.height() as i32,
        ];
        let resources = self.resources;
        let owner_flag = self.owner_flag;
        let mut host = SoftwareMenu::new(SoftwareMenuRequest {
            scene: self.scene,
            resources,
            focal: self.focal,
            process: self.process,
            state: &mut *self.state,
        })
        .map_err(FrontendRendererError::from)
        .map_err(MenuError::Host)?;
        host.publish_row_origin(request.context_origin_pixels)
            .map_err(FrontendRendererError::from)
            .map_err(MenuError::Host)?;
        profile_draw::draw_profile_row(
            ProfileRowDraw {
                request,
                base,
                state,
                store: self.profiles,
                owner_flag,
                surface_pixels,
            },
            &resources,
            &mut host,
        )
        .map_err(FrontendRendererError::from)
        .map_err(MenuError::Host)
    }
    fn draw_profile_status(
        &mut self,
        timer: ProfileTimer,
        surface: [i32; 2],
    ) -> Result<(), MenuError<Self::Error>> {
        let resources = self.resources;
        profile_draw::draw_profile_status(
            timer,
            surface,
            &resources,
            &mut self.menu().map_err(MenuError::Host)?,
        )
        .map_err(FrontendRendererError::from)
        .map_err(MenuError::Host)
    }
}

impl SoftwareLinkedFrame<'_, '_, '_> {
    /// Linked +08: 004047F0, 00484940 or 00459310 for the registry's linked
    /// screen. The title runs only its arena preamble here; its empty +0C
    /// presentation never presents or clears.
    ///
    /// On a widescreen frame, the screens retail lays out at fixed 4:3
    /// positions (the language carousel, the briefing and the map, whose
    /// tiles scale with the width and the height apart) draw on the frame's
    /// centred 4:3 canvas; the others lay out on the whole frame.
    pub fn draw<P: PortDrawPlatform, E>(
        &mut self,
        session: &mut PortSession<P>,
        state: &crate::session::SessionState,
    ) -> Result<LinkedDrawOutcome, SoftwareLinkedError<E>> {
        let screen = session.linked_screen()?;
        let canvas = match screen {
            PortScreen::Startup | PortScreen::Briefing | PortScreen::Map => {
                self.scene.pixels.frame.four_three_canvas()
            }
            _ => None,
        };
        if let Some(canvas) = canvas {
            self.scene.enter_canvas(canvas);
        }
        let outcome = self.draw_screen(screen, session, state);
        self.scene.leave_canvas();
        outcome
    }

    fn draw_screen<P: PortDrawPlatform, E>(
        &mut self,
        screen: PortScreen,
        session: &mut PortSession<P>,
        state: &crate::session::SessionState,
    ) -> Result<LinkedDrawOutcome, SoftwareLinkedError<E>> {
        match screen {
            // 004330E0: the 00439270 preamble, then the language carousel.
            PortScreen::Startup => {
                let focal = self.template_projection.focal;
                let state = session
                    .startup_state_mut()
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::Startup))?;
                let outcome = crate::startup_draw::software::draw(
                    &mut *self.scene,
                    self.resources,
                    &mut *self.frontend_state,
                    state,
                    focal,
                    self.sine_quarter,
                )
                .map_err(|error| SoftwareLinkedError::Startup(error.to_string()))?;
                Ok(LinkedDrawOutcome::Startup(outcome))
            }
            // 004450B0: the preamble, then the menu controller's rows.
            PortScreen::KeyConfig => {
                let sine_quarter = self.sine_quarter;
                let outcome = session
                    .with_key_config_draw(|draw| {
                        let settings = draw.platform.frontend_draw_parts().settings;
                        crate::key_config_draw::software::draw(
                            crate::key_config_draw::software::KeyConfigDrawRequest {
                                scene: &mut *self.scene,
                                resources: self.resources,
                                registry: &*draw.registry,
                                attachment: draw.attachment,
                                process: draw.process,
                                published: &mut *self.frontend_state,
                                tables: settings.key_binding_tables(),
                                bindings: settings.key_bindings(),
                                visibility: crate::menu::MenuVisibility::from_owner_flag(
                                    draw.owner_byte,
                                ),
                                sine_quarter,
                            },
                            draw.screen,
                        )
                    })
                    .map_err(|error| SoftwareLinkedError::KeyConfig(error.to_string()))?
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::KeyConfig))?;
                Ok(LinkedDrawOutcome::KeyConfig(outcome))
            }
            PortScreen::Title => {
                let outcome = session
                    .with_title_draw(|draw| {
                        let Some(sink) = draw.platform.title_video_sink() else {
                            let mut preamble = TitleScenePreamble {
                                scene: &mut *self.scene,
                            };
                            return draw.screen.draw(&mut preamble).map_err(|error| {
                                match error {
                                    TitleScreenError::Host(error) => error,
                                    _ => TitlePreambleError::ScreenNotReady,
                                }
                            });
                        };
                        let mut host = TitleDrawHost {
                            preamble: TitleScenePreamble {
                                scene: &mut *self.scene,
                            },
                            state: sink.state,
                            audio: sink.audio,
                        };
                        let outcome =
                            draw.screen.draw(&mut host).map_err(|error| match error {
                                TitleScreenError::Host(error) => error,
                                _ => TitlePreambleError::ScreenNotReady,
                            })?;
                        if matches!(outcome, TitleDrawOutcome::Presented) {
                            if let Some(rgba) = host.state.presented_frame() {
                                let frame = &mut host.preamble.scene.pixels.frame;
                                let placed = frame.replace_surface(rgba)
                                    || host.state.presented_dimensions().is_some_and(
                                        |(width, height)| frame.fit_surface(width, height, rgba),
                                    );
                                if !placed {
                                    eprintln!(
                                        "title frame does not fit the {}x{} surface; keeping cleared",
                                        frame.width(),
                                        frame.height()
                                    );
                                }
                            }
                        }
                        Ok(outcome)
                    })
                    .map_err(SoftwareLinkedError::TitleDraw)?
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::Title))?;
                Ok(LinkedDrawOutcome::Title(outcome))
            }
            PortScreen::Frontend => {
                let outcome = session
                    .with_frontend_draw(|draw| {
                        let parts = draw.platform.frontend_draw_parts();
                        let visibility = MenuVisibility::from_owner_flag(draw.owner_byte);
                        draw.screen.draw_frame(
                            FrontendScreenDraw {
                                scene: self.frontend_scene,
                                visibility,
                                sine_quarter: self.sine_quarter,
                            },
                            &mut SoftwareFrontendFrame {
                                scene: &mut *self.scene,
                                resources: self.resources,
                                registry: &*draw.registry,
                                attachment: draw.attachment,
                                process: draw.process,
                                state: &mut *self.frontend_state,
                                surface: self.surface,
                                profiles: parts.profiles,
                                settings: parts.settings,
                                identity: parts.identity,
                                video: parts.video,
                                owner_flag: draw.owner_byte,
                                sine_quarter: self.sine_quarter,
                            },
                        )
                    })
                    .map_err(SoftwareLinkedError::Frontend)?
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::Frontend))?;
                Ok(LinkedDrawOutcome::Frontend(outcome))
            }
            PortScreen::TextEntry => {
                let projection = self.template_projection;
                // Word 00533760 for the name-entry objects' 0043A270.
                let selector_one = session.process.object_selector_one();
                session
                    .with_name_entry_draw(|draw| {
                        let renderer = self.scene.id();
                        draw.screen.draw_frame(
                            TextEntryScreenDraw {
                                animation: draw.animation,
                                selector_one,
                            },
                            &mut SoftwareNameEntryFrame {
                                scene: &mut *self.scene,
                                resources: self.resources,
                                registry: &*draw.registry,
                                attachment: draw.attachment,
                                // One CPU backend serves the one linked context.
                                context: SoftwareNameEntryContext {
                                    owner: draw.attachment.context_owner,
                                    renderer,
                                },
                                projection,
                                sine_quarter: self.sine_quarter,
                                // The text sinks never add this origin to
                                // pixels; the backdrop's 0043B4A0 half size.
                                context_origin_pixels: projection.viewport.map(|v| v / 2),
                                base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
                                state: &mut *self.name_entry_state,
                                surface: self.surface,
                            },
                        )
                    })
                    .map_err(SoftwareLinkedError::NameEntry)?
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::TextEntry))?;
                Ok(LinkedDrawOutcome::NameEntry)
            }
            // 0040B0F0's native resident presentation phases.
            PortScreen::Briefing => {
                let selector_one = session.process.object_selector_one();
                // The world manager's +5C3: the last world's infrared vision
                // until the first tick of this one.
                let infrared = session.camera_globals.infrared_5c3;
                session
                    .with_briefing_draw(state.current_phase(), |draw| {
                        let settings = draw.platform.frontend_draw_parts().settings;
                        let dynamic_lighting = settings.words()[0x80 / 4] != 0;
                        let mut object_light_policy =
                            crate::world_lighting::SettingsObjectLightPolicySource {
                                settings,
                                spacing: draw.world.terrain.sampling().spacing,
                                color: crate::world_lighting::TerrainLightColor::for_infrared(
                                    infrared,
                                ),
                            };
                        SoftwareBriefingFrame {
                            scene: &mut *self.scene,
                            resources: self.resources,
                            sine_quarter: self.sine_quarter,
                            template_projection: self.template_projection,
                            state: &mut *self.frontend_state,
                            selector_one,
                            world: draw.world,
                            effects: draw.effects,
                            object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
                            dynamic_lighting,
                            object_light_policy: &mut object_light_policy,
                            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
                            infrared,
                        }
                        .draw(draw.screen, draw.process, draw.process_rng)
                    })
                    .map_err(SoftwareLinkedError::BriefingDraw)?
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::Briefing))?;
                Ok(LinkedDrawOutcome::Briefing)
            }
            // Gameplay native world draw (`00442080` → `00440F00` →
            // `00488020`): view setup, depth state and entity models through
            // the shared scene backend. Sky executes with the selected eye;
            // terrain, HUD, text and simulation retain per-feature boundaries.
            PortScreen::Gameplay => {
                let screen = session
                    .gameplay_screen()
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::Gameplay))?;
                let (screen_age_millis, scores_held) =
                    (screen.screen_age_millis(), screen.scores_f8);
                // Settings are process-global; the parts getter is the live
                // Z Buffering (`+50`) boundary for the `+10` depth decision.
                let animation_clock = session.process.animation().value();
                let executable = inf_formats::FrontendExecutable::parse(
                    session.catalog.assets().executable_bytes(),
                )?;
                let mut parts = session.platform.world_draw_parts();
                let settings = parts.settings.words();
                let z_buffering = settings[0x50 / 4] != 0;
                let dynamic_lighting = settings[0x80 / 4] != 0;
                let camera_settings = crate::object_runtime::CameraSettings::from_words(settings);
                // 0041C41C/00441499: the "HUD" (+60) and "First Person" (+68).
                let hud_settings = [settings[0x60 / 4], settings[0x68 / 4]];
                let (audio, effects_volume) = match parts.audio {
                    Some(audio) => (
                        Some(
                            audio
                                as &mut dyn crate::process_sound::ProcessSoundAudio<
                                    Error = crate::platform_audio::PlatformAudioError,
                                >,
                        ),
                        parts.settings.fx_volume(),
                    ),
                    None => (None, 0),
                };
                if let Some(world) = session.world.as_mut() {
                    let mut object_light_policy =
                        crate::world_lighting::SettingsObjectLightPolicySource {
                            settings: parts.settings,
                            spacing: world.terrain.sampling().spacing,
                            color: crate::world_lighting::TerrainLightColor::for_infrared(
                                session.camera_globals.infrared_5c3,
                            ),
                        };
                    let surface = [
                        self.scene.pixels.frame.width() as i32,
                        self.scene.pixels.frame.height() as i32,
                    ];
                    let style = parts.settings.camera_style();
                    let focal_src = crate::camera_clearance::focal(
                        style,
                        session.camera_globals.special_view_active,
                        self.template_projection.focal,
                    );
                    let focal_kick = if style == crate::camera_clearance::CameraStyle::Remastered {
                        crate::camera_clearance::speed_kick(
                            &mut session.camera_clearance,
                            &world.arena,
                            crate::frontend_camera::AnimationClockSource::animation_micros(
                                &session.process,
                            ),
                            session.camera_globals.special_view_active,
                        )
                    } else {
                        1.0
                    };
                    let frame = crate::world_scene::WorldFrameInputs {
                        template_focal: self.template_projection.focal,
                        focal_src,
                        focal_kick,
                        scale_5c0: session.camera_globals.view_zoom_5c0.get() as i16,
                        special_view_active: session.camera_globals.special_view_active,
                        surface,
                        z_buffering,
                        dynamic_lighting,
                        arena_bytes: crate::world_scene::world_arena_bytes(
                            surface,
                            focal_src.map(|focal| (focal as f32 * focal_kick).round() as i32),
                        ),
                        mirror_copy: parts.settings.fixes().model_mirror_copy(),
                        hovercraft_fans: parts.settings.fixes().hovercraft_fans,
                        guided_mounts: parts.settings.fixes().guided_mounts,
                        camera_settings: crate::object_runtime::CameraSettings::from_words(
                            settings,
                        ),
                        settings_7c: settings[0x7c / 4],
                        settings_70: settings[0x70 / 4],
                        network_initials: session.network_initials,
                        network_team_flag: session.network_team_flag,
                        infrared: session.camera_globals.infrared_5c3,
                        game: parts
                            .settings
                            .game_view()
                            .for_frame(session.factory_process.network_flag),
                        game_far: parts.settings.game_far(world.mission.current_world_a0),
                    };
                    // Non-native: the remaster's thermal camera adds the
                    // lamps' ironbow step, and shows the palette it left.
                    session
                        .camera_globals
                        .thermal_palettes
                        .set(frame.game != crate::presentation::GameView::Retail);
                    self.scene.set_ironbow(session.camera_globals.ironbow.get());
                    let mut host = ResidentSceneHost {
                        resources: self.resources,
                        backend: &mut *self.scene,
                    };
                    let pickup_text_data = crate::grant_text::PickupTextData {
                        common: super::world_owners::current_grant_text(
                            &session.profile_name_classification,
                        ),
                        hyphen_high: session
                            .profile_name_classification
                            .high_bytes
                            .as_ref()
                            .and_then(|bytes| bytes[b'-' as usize]),
                    };
                    match crate::world_scene::draw_world_frame(
                        crate::world_scene::WorldDrawRequest {
                            pickup_text_data: Some(pickup_text_data),
                            weapon_tables: Some(&mut session.actor_behavior_tables),
                            terrain_event_5483b0: session.camera_shake_enabled,
                            map: session.maps.jitter_mut(world.map).ok(),
                            // Not in a special view (a factory's dock, the
                            // scope), which places the camera itself.
                            camera_clearance: (parts.settings.camera_style()
                                == crate::camera_clearance::CameraStyle::Remastered
                                && !session.camera_globals.special_view_active)
                                .then_some(crate::camera_clearance::CameraClearanceLoan {
                                    model_process: &session.model_process,
                                    model_fallback: session
                                        .pelagia_models
                                        .as_ref()
                                        .map(|models| &models.runtime),
                                    query: &mut session.camera_furniture_query,
                                    state: &mut session.camera_clearance,
                                }),
                            world,
                            animation: &session.process,
                            object_draw_process: &mut session.object_draw_process,
                            attract_demo: &session.attract,
                            particles: &mut session.particles,
                            light_grid: &mut session.light_grid,
                            executable,
                            object_light_policy: &mut object_light_policy,
                            object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
                            camera: super::WorldCameraInputs {
                                residency: &session.residency,
                                sine_quarter: &session.catalog.assets().sine_table().values,
                                settings: camera_settings,
                                process: Some(&session.factory_process),
                                animation_clock,
                            },
                            furniture: crate::world_scene::WorldFurnitureInputs {
                                state: &mut session.furniture_draw,
                                pending: &session.furniture,
                                birth_inputs:
                                    crate::process_particles::births::ParticleBirthInputs {
                                        network_active: session.factory_process.network_flag != 0,
                                        phase: Some(u32::from(state.current_phase().id())),
                                        process: &session.factory_process,
                                    },
                            },
                            sound: Some(crate::world_scene::WorldDrawSound {
                                queue: &mut session.sounds,
                                sink: audio,
                                effects_volume,
                                phase: u32::from(state.current_phase().id()),
                            }),
                        },
                        frame,
                        &mut host,
                    ) {
                        Ok(outcome) => {
                            if !world.draw_skip_logged {
                                world.draw_skip_logged = true;
                                eprintln!(
                                    "gameplay draw: native world frame ({} models, skips {:?}, focals {:?}, depth {:?}; skipped_sky={}, skipped_terrain={}, see GAMEPLAY_RUNTIME.md)",
                                    outcome.submitted_models,
                                    outcome.skips,
                                    outcome.focals,
                                    outcome.depth,
                                    outcome.skipped_sky,
                                    outcome.skipped_terrain,
                                );
                            }
                        }
                        Err(error) => {
                            eprintln!(
                                "gameplay draw: native world draw failed ({error}: {error:?}; the scene stopped at: {}), retaining completed pixels and reached state",
                                self.scene.stop_cause().unwrap_or("no recorded cause")
                            );
                            // The HUD's draws go unmoved by the world's
                            // anti-aliasing jitter.
                            self.scene.finish_raster_world();
                        }
                    }
                    // 00441003 follows the world/player flush. These are
                    // immediate pixel masks, leaving world projection intact.
                    if let Err(error) = crate::gameplay_overlay::software::draw(
                        self.scene,
                        self.resources,
                        world.mission.current_world_a0,
                        screen_age_millis,
                    ) {
                        eprintln!("gameplay overlay: {error}; retaining completed submissions");
                    }
                    let hud_inputs = crate::player_hud::PlayerHudInputs {
                        process: &session.process,
                        factory_process: &session.factory_process,
                        executable: inf_formats::FrontendExecutable::parse(
                            session.catalog.assets().executable_bytes(),
                        )
                        .ok(),
                        template_projection: self.template_projection,
                        sine_quarter: self.sine_quarter,
                        hud_setting: hud_settings[0],
                        first_person_setting: hud_settings[1],
                        settings_7c: settings[0x7c / 4],
                        special_view: session.camera_globals.special_view_active,
                        settings: Some(parts.settings),
                        database_resources: self.database_resources,
                        radar_jitter: session
                            .maps
                            .jitter(world.map)
                            .ok()
                            .map(|map| map.jitter_radius()),
                        radar_map: session.maps.grid(),
                        object_draw_process: Some(&session.object_draw_process),
                        light_grid: Some(&session.light_grid),
                        device_name: parts
                            .video
                            .as_mut()
                            .and_then(|video| video.device_name_for_draw().ok()),
                    };
                    // 0044121E..00441456: the world captions; then each
                    // player's unit draws its HUD (00441456..004414E5).
                    crate::player_hud::draw_world_captions(
                        world,
                        self.scene,
                        self.resources,
                        &mut self.frontend_state.menu,
                        &hud_inputs,
                        session.attract,
                    );
                    crate::player_hud::draw_player_huds(
                        world,
                        self.scene,
                        self.resources,
                        &mut self.frontend_state.menu,
                        &hud_inputs,
                        parts.network.as_deref_mut(),
                    );
                    // 004421E6: then, online, the network game's overlay.
                    crate::player_hud::draw_network_overlay(
                        world,
                        self.scene,
                        self.resources,
                        &mut self.frontend_state.menu,
                        &hud_inputs,
                        parts.network.as_deref(),
                        scores_held,
                    );
                } else {
                    self.scene.pixels.frame.clear_color([16, 24, 64, 255]);
                }
                Ok(LinkedDrawOutcome::Gameplay)
            }
            // 00451250: the map in the right three quarters, its frame,
            // the legend and the text. A failed draw keeps what it reached,
            // as the world draw does, and reports once.
            PortScreen::Map => {
                let screen = *session
                    .map
                    .as_ref()
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::Map))?
                    .state();
                let world = session
                    .world
                    .as_mut()
                    .ok_or_else(|| SoftwareLinkedError::Map("the map has no world".into()))?;
                let map_state = session
                    .maps
                    .jitter(world.map)
                    .map_err(|error| SoftwareLinkedError::Map(error.to_string()))?;
                let player = world.arena.first_player();
                let result = crate::map_screen::software::SoftwareMapFrame {
                    scene: &mut *self.scene,
                    resources: self.resources,
                    state: &mut *self.frontend_state,
                    world: crate::map_screen::software::MapWorld {
                        arena: &world.arena,
                        map_state,
                        network_active: session.factory_process.network_flag != 0,
                        rng: &mut world.rng,
                        player,
                    },
                }
                .draw(&screen, state.gameplay_time().seconds);
                if let Err(error) = result {
                    let first = session
                        .map
                        .as_mut()
                        .is_some_and(crate::map_screen::MapScreen::note_draw_failure);
                    if first {
                        eprintln!(
                            "map draw: {error}; retaining completed pixels (see GAMEPLAY_RUNTIME.md)"
                        );
                    }
                    if !self.scene.feature_continuation_allowed() {
                        return Err(SoftwareLinkedError::Map(error.to_string()));
                    }
                    self.scene.continue_after_feature_error()?;
                }
                Ok(LinkedDrawOutcome::Map)
            }
            PortScreen::IntroResults => {
                // 0042C2B0 draws the panels' frames before its panel draws
                // build the next panel, so the page takes the panels as the
                // draw found them. The draw's state changes precede its
                // text.
                let found = session
                    .intro_results
                    .as_ref()
                    .map(|screen| *screen.panels())
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::IntroResults))?;
                if let (Some(screen), Some(world)) =
                    (session.intro_results.as_mut(), session.world.as_mut())
                {
                    screen
                        .draw_effects(session.attract, &world.mission, &mut world.arena)
                        .map_err(|error| SoftwareLinkedError::IntroResults(error.to_string()))?;
                }
                let screen = session
                    .intro_results_screen()
                    .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::IntroResults))?;
                let request = IntroResultsDrawRequest {
                    attract: session.attract,
                    mission: session.world().map(|world| &world.mission),
                };
                let outcome = screen
                    .draw(
                        request,
                        &mut SoftwareIntroResultsDraw {
                            scene: &mut *self.scene,
                            state: &mut *self.frontend_state,
                            surface: self.surface,
                        },
                    )
                    .map_err(|error| SoftwareLinkedError::IntroResults(error.to_string()))?;
                let menu_shown = screen.menu_shown();
                if outcome == IntroResultsDrawOutcome::Page {
                    let world = session.world().ok_or(SoftwareLinkedError::IntroResults(
                        "results page without a world".into(),
                    ))?;
                    let surface_pixels = [
                        self.scene.pixels.frame.width() as i32,
                        self.scene.pixels.frame.height() as i32,
                    ];
                    let page = crate::results_page::ResultsPage {
                        state: screen.page_state(found),
                        mission: &world.mission,
                        rank_word: world
                            .arena
                            .first_player()
                            .and_then(|player| world.arena.world_player(player).ok())
                            .map(|player| player.word_11e as i16),
                        tokens: world.arena.pickup_kind7_count(),
                        surface_pixels,
                        projection: SceneProjection {
                            viewport: surface_pixels,
                            ..self.template_projection
                        },
                        animation: session.process.animation(),
                        selector_one: session.process.object_selector_one(),
                        glyph_base_color: crate::menu::MenuColor::from_word(
                            crate::menu_text::INITIAL_GLYPH_BASE_WORD,
                        ),
                        credits: screen.credits(),
                    };
                    crate::results_page::software::draw(
                        self.scene,
                        self.resources,
                        &page,
                        &crate::menu_basis::MenuBasisMath::new(*self.sine_quarter),
                    )
                    .map_err(|error| SoftwareLinkedError::IntroResults(error.to_string()))?;
                    if menu_shown {
                        // 0042CA85: the shown profile menu draws instead
                        // of the page's text.
                        let surface_pixels = [
                            self.scene.pixels.frame.width() as i32,
                            self.scene.pixels.frame.height() as i32,
                        ];
                        let owner_flag = session.owner_byte;
                        let profiles = session.platform.frontend_draw_parts().profiles;
                        let menus = session
                            .intro_results
                            .as_mut()
                            .and_then(
                                crate::intro_results_screen::IntroResultsScreen::shown_menu_mut,
                            )
                            .ok_or(SoftwareLinkedError::MissingScreen(PortScreen::IntroResults))?;
                        menus
                            .draw(
                                MenuDrawInput {
                                    surface_pixels,
                                    visibility: MenuVisibility::from_owner_flag(owner_flag),
                                    sine_quarter: self.sine_quarter,
                                },
                                &mut SoftwareResultsMenuDraw {
                                    scene: &mut *self.scene,
                                    resources: self.resources,
                                    focal: self.template_projection.focal,
                                    process: &session.process,
                                    state: &mut self.frontend_state.menu,
                                    profiles,
                                    owner_flag,
                                },
                            )
                            .map_err(|error| {
                                SoftwareLinkedError::IntroResults(error.to_string())
                            })?;
                    }
                }
                Ok(LinkedDrawOutcome::IntroResults)
            }
        }
    }
}

/// Linked +0C 0040D1D0 or 00440930: 00438120 resolves the caller's H7 slot
/// (or packed black) on this call from the renderer's residency, then
/// 00495810 presents (0050D330) before 0050D960 clears the next colour
/// surface.
pub fn present_linked<E>(
    scene: &mut SoftwareScene<'_, '_>,
    resources: RenderResources<'_>,
    clear: PresentationClear,
    forced_black: bool,
    present: impl FnOnce(&mut SoftwareFrame) -> Result<(), E>,
) -> Result<(), SoftwareLinkedError<E>> {
    let clear = presentation_clear_rgba(resources, clear, forced_black)?;
    present_linked_clear(scene, clear, present)
}

/// Linked +0C with an already chosen clear: the startup's 00433620 passes
/// its +1A8 word straight to 00495810, without 00438120's H7 lookup.
pub fn present_linked_clear<E>(
    scene: &mut SoftwareScene<'_, '_>,
    clear: [u8; 4],
    present: impl FnOnce(&mut SoftwareFrame) -> Result<(), E>,
) -> Result<(), SoftwareLinkedError<E>> {
    present(&mut *scene.pixels.frame).map_err(SoftwareLinkedError::Present)?;
    scene.after_present(clear)?;
    Ok(())
}
