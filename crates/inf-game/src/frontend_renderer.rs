//! Full frontend-frame effects on the live surface and persistent CPU renderer.

use thiserror::Error;

use crate::{
    frontend_frame::{FrameResourceState, FrontendFrameHost, FrontendSceneEnvironment},
    frontend_scene::{FrontendSceneHost, SceneCommand, SceneProjection, SceneSetup},
    frontend_screen::FrontendProcessState,
    frontend_settings::{FrontendSettings, SidecarIdentityStore},
    frontend_video::{FrontendVideoState, VideoError},
    input::InputContextId,
    menu::{
        MenuBasis, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuError,
        MenuRowDrawRequest, MenuState,
    },
    menu_text::{MenuRowTextRequest, MenuTextError, MenuTextOutcome, RawMenuTextRequest},
    player_identity::{IdentityRead, PlayerIdentity},
    profile_draw::{self, ProfileDrawError, ProfileRowDraw, ProfileTextHost},
    profile_menu::{ProfileMenuState, ProfileTimer},
    profile_store::ProfileStore,
    render_resources::{
        RenderResourceError, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    scene_resources::{ResidentSceneHost, SceneResourceError},
    screen_registry::{ContextOwnerId, RegistryError, ScreenAttachment, ScreenRegistry},
    screen_renderer_preparation::ScreenRendererPreparationHost,
    settings_row::{self, RowSideError, SettingsRowError},
    software_frame::SoftwareSurface,
    software_menu::{SoftwareMenu, SoftwareMenuError, SoftwareMenuRequest, SoftwareMenuState},
    software_scene::{SoftwareRendererId, SoftwareScene, SoftwareSceneError},
};

/// The live caller context is owned by the registry, not copied at capture time.
#[derive(Clone, Copy, Debug)]
pub struct FrontendRenderContext {
    pub projection: SceneProjection,
}

#[derive(Default, Debug)]
pub struct FrontendRendererState {
    pub published_renderer: Option<SoftwareRendererId>,
    pub caller_context: Option<ContextOwnerId>,
    pub disposed_native_errors: usize,
    pub menu: SoftwareMenuState,
    pub menu_trace: MenuTrace,
}

/// The main menu backdrop's models under the tracer (Vulkan with Path
/// Tracing on): the scene's light and setup, and where the
/// trace stands. While it collects, the models' batches wait, to be traced
/// together before the footer; the flush after the footer then draws what
/// the trace left (blended parts, the footer) over it.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuTrace {
    setup: Option<SceneSetup>,
    light: Option<([f32; 3], [u8; 3])>,
    ambient: [i32; 3],
    phase: MenuTracePhase,
    /// The batch the collected models share is open.
    open: bool,
    /// The views published since the scene's setup: the first is the far
    /// backdrop's, which stays the raster's.
    views: u32,
    /// Where the menu's sun is, and the sum and count of the other models'
    /// places: the sun shines on them from its side.
    sun: Option<[i32; 3]>,
    bodies: ([i64; 3], i64),
}

/// The menu's sun, `planetsunspin` (`0053BED8`'s ninth model): the traced
/// menu lights its planets from where it stands, off the screen's right.
const MENU_SUN_MODEL: i16 = 757;

/// How far the traced menu's shadow rays start above a planet's faces.
const MENU_SHADOW_LIFT: f32 = 600.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum MenuTracePhase {
    #[default]
    Idle,
    /// The main menu's scene is about to draw, with tracing on: its second
    /// view begins the feed. Other screens never arm it.
    Armed,
    Collecting,
    Traced,
}

#[derive(Debug, Error)]
pub enum FrontendRendererError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Submission(#[from] SceneResourceError<SoftwareSceneError>),
    #[error(transparent)]
    Menu(#[from] SoftwareMenuError),
    #[error(transparent)]
    Profile(#[from] ProfileDrawError<RenderResourceError, SoftwareMenuError>),
    #[error("settings value-row cell {address:#010x} is outside the 34-word state")]
    SettingsCell { address: u32 },
    #[error(transparent)]
    Video(#[from] VideoError),
    #[error(transparent)]
    SettingsRow(#[from] RowSideError),
    #[error("selected renderer does not own this pixel surface")]
    Renderer,
    #[error("frontend session has no caller context")]
    CallerContext,
    #[error("SDL surface dimensions {actual:?} differ from renderer {expected:?}; recreation is required")]
    Surface {
        actual: [u32; 2],
        expected: [u32; 2],
    },
    #[error("required lazy resource H{handler}/{slot} has no active registration")]
    LazyResource { handler: u8, slot: u16 },
    #[error("unsupported frame resource handler {0}")]
    Handler(u8),
    #[error("native inclusive bound addresses one past H{handler}'s allocation at {slot}")]
    PoolBoundary { handler: u8, slot: u16 },
    #[error("draw arena size does not fit a native DWORD")]
    ArenaSize,
    #[error("attempted to dispose an error outside the native 301 catch")]
    ErrorDisposal,
}

pub struct SoftwareFrontendFrame<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    pub registry: &'a ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub attachment: ScreenAttachment,
    pub process: &'a FrontendProcessState,
    pub state: &'a mut FrontendRendererState,
    pub surface: SoftwareSurface<'a>,
    pub profiles: &'a mut ProfileStore,
    pub settings: &'a FrontendSettings,
    /// Process identity caches the player-submenu rows read and refresh.
    pub identity: &'a mut PlayerIdentity,
    pub video: &'a mut FrontendVideoState,
    pub owner_flag: bool,
    /// `0053DF00`, the quarter-wave sine the arena previews' arc reads.
    pub sine_quarter: &'a [u16; 4096],
}

impl<'f, 'r> SoftwareFrontendFrame<'_, 'f, 'r> {
    fn selected(&self, renderer: SoftwareRendererId) -> Result<(), FrontendRendererError> {
        if renderer != self.scene.id() {
            return Err(FrontendRendererError::Renderer);
        }
        Ok(())
    }

    /// Follow the menu scene's commands for its trace: its light and setup,
    /// then, once armed, the second view (the first model's after the
    /// backdrop), which begins the feed, and while it collects, the
    /// batches, which wait for the trace (true: held).
    fn follow_menu_trace(&mut self, command: SceneCommand) -> bool {
        let trace = self.state.menu_trace;
        match command {
            SceneCommand::Setup(setup) => {
                self.state.menu_trace.setup = Some(setup);
                self.state.menu_trace.views = 0;
            }
            SceneCommand::PublishView(_) if trace.views == 0 => self.state.menu_trace.views = 1,
            SceneCommand::DirectionalLight { direction, rgb } => {
                self.state.menu_trace.light = Some((direction, rgb));
            }
            SceneCommand::AmbientLight(ambient) => self.state.menu_trace.ambient = ambient,
            SceneCommand::PublishView(view) if trace.phase == MenuTracePhase::Armed => {
                let (Some(setup), Some((direction, rgb))) = (trace.setup, trace.light) else {
                    return false;
                };
                if !self
                    .scene
                    .begin_scene_feed(crate::world_scene::scene_camera(view, setup), false)
                {
                    self.state.menu_trace.phase = MenuTracePhase::Idle;
                    return false;
                }
                if let Some(builder) = self.scene.scene_feed_mut() {
                    let length = direction.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
                    // The planets take the tracer's light alone: the menu's
                    // light, full at its brightest, from the sun's side (set
                    // when the models are in), under a black sky with no
                    // haze.
                    builder.lit_by_trace = true;
                    let feed = &mut builder.feed;
                    // Above the planets' low-poly hulls (a sphere's faces
                    // stand up to a fourteenth of its radius inside it).
                    feed.shadow_lift = MENU_SHADOW_LIFT;
                    feed.sun = crate::scene_feed::SceneSun {
                        direction: direction.map(|axis| axis / length.max(1.0e-6)),
                        color: rgb.map(|channel| f32::from(channel) / 255.0),
                        ambient: trace
                            .ambient
                            .map(|channel| channel.clamp(0, 255) as f32 / 255.0),
                    };
                    feed.sky = [0.0; 3];
                    feed.fog = crate::scene_feed::SceneFog {
                        start: 1.0e7,
                        end: 2.0e7,
                        ..Default::default()
                    };
                }
                self.state.menu_trace.phase = MenuTracePhase::Collecting;
                self.state.menu_trace.open = false;
                self.state.menu_trace.sun = None;
                self.state.menu_trace.bodies = ([0; 3], 0);
            }
            SceneCommand::Model(model) if trace.phase == MenuTracePhase::Collecting => {
                if model.model_slot == MENU_SUN_MODEL {
                    self.state.menu_trace.sun = Some(model.position);
                } else {
                    let (sum, count) = &mut self.state.menu_trace.bodies;
                    for (total, axis) in sum.iter_mut().zip(model.position) {
                        *total += i64::from(axis);
                    }
                    *count += 1;
                }
            }
            // The first model's batch opens the one they all share.
            SceneCommand::BeginBatch if trace.phase == MenuTracePhase::Collecting => {
                if !trace.open {
                    self.state.menu_trace.open = true;
                    return false;
                }
                return true;
            }
            SceneCommand::FlushBatch if trace.phase == MenuTracePhase::Collecting => return true,
            _ => {}
        }
        false
    }

    fn context(&self) -> Result<&FrontendRenderContext, FrontendRendererError> {
        let id = self
            .state
            .caller_context
            .ok_or(FrontendRendererError::CallerContext)?;
        Ok(self.registry.backend(id)?)
    }

    fn menu(&mut self) -> Result<SoftwareMenu<'_, 'f, 'r>, FrontendRendererError> {
        let focal = self.context()?.projection.focal;
        Ok(SoftwareMenu::new(SoftwareMenuRequest {
            scene: self.scene,
            resources: self.resources,
            focal,
            process: self.process,
            state: &mut self.state.menu,
        })?)
    }

    /// Settings-family rows share one 00453810 composition: value rows
    /// 00453AE0 and Sound sliders 00453D50 read their cell's live settings
    /// word; identity rows 004639D0/00463A30 pair fixed H0 labels with the
    /// process net-name and initials caches (their draw arguments are unused).
    /// 004639D0 first refreshes the caches through 00463D10; 00463A30 does
    /// not read, so it shows whatever the last read left. Inputs
    /// resolve before any renderer state changes; cells outside the settings
    /// state are an explicit boundary, matching the adjust host. Arithmetic
    /// and text stay in `settings_row`.
    fn draw_settings_family(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), FrontendRendererError> {
        enum Row<'s> {
            Value(u32),
            Volume(u32),
            Identity(i32, std::borrow::Cow<'s, [u8]>),
            Port(Vec<u8>, Vec<u8>),
        }
        // Copy the shared custody reference first so the text borrows live
        // independently of the mutable scene borrows below.
        let settings = self.settings;
        let resources = self.resources;
        let row = match request.callback_address {
            // Resolution and Display draw port text for an exact size and
            // for Borderless, which have no authored string.
            settings_row::VALUE_ROW | settings_row::VOLUME_ROW => {
                match settings_row::settings_row_content(
                    settings,
                    request.callback_address,
                    request.arguments,
                )
                .expect("a value row or slider")
                .map_err(|address| FrontendRendererError::SettingsCell { address })?
                {
                    settings_row::SettingsRowContent::Value(value) => Row::Value(value),
                    settings_row::SettingsRowContent::Volume(value) => Row::Volume(value),
                    settings_row::SettingsRowContent::Text(label, text) => {
                        Row::Identity(label, std::borrow::Cow::Owned(text))
                    }
                    settings_row::SettingsRowContent::Port(label, value) => Row::Port(label, value),
                }
            }
            // 004639D0 calls 00463D10 (into discarded 8-byte buffers) and
            // draws the net-name cache 0054EBC8.
            0x0046_39D0 => {
                self.identity
                    .read(
                        IdentityRead::NetPlayerName,
                        &mut SidecarIdentityStore(settings),
                        0,
                    )
                    .unwrap_or_else(|never| match never {});
                Row::Identity(
                    0x7A,
                    std::borrow::Cow::Owned(self.identity.net_name_cache().to_vec()),
                )
            }
            // 0045EE20: Arena name and the arena's own name, else its
            // world's (004889C0), after the previews (`draw_arena_previews`).
            crate::network_menu::ARENA_ROW => {
                let arena = settings.setting_word(request.arguments[1]).ok_or(
                    FrontendRendererError::SettingsCell {
                        address: request.arguments[1],
                    },
                )?;
                let name = match crate::network_menu::arena_name(arena) {
                    Ok(name) => name.as_bytes().to_vec(),
                    Err(text) => {
                        crate::menu_text::MenuTextResources::string(&resources, text)?.to_vec()
                    }
                };
                Row::Identity(request.arguments[0] as i32, std::borrow::Cow::Owned(name))
            }
            // 0045F280: Winning Score and its number, in thousands in mode 4.
            crate::network_menu::SCORE_ROW_DRAW => {
                let cell = |address| {
                    settings
                        .setting_word(address)
                        .ok_or(FrontendRendererError::SettingsCell { address })
                };
                let value = cell(request.arguments[1])?;
                let arena = cell(
                    crate::network_menu::SETTINGS_ADDRESS + crate::network_menu::SETTING_ARENA,
                )?;
                let mode = crate::network_menu::arena_at(arena).mode;
                Row::Identity(
                    request.arguments[0] as i32,
                    std::borrow::Cow::Owned(
                        crate::network_menu::winning_score_text(mode, value).into_bytes(),
                    ),
                )
            }
            // 00463A30 reads nothing: it draws the initials cache 0054EBC0 as
            // the last read left it.
            0x0046_3A30 => Row::Identity(
                0x7B,
                std::borrow::Cow::Owned(self.identity.initials_cache().to_vec()),
            ),
            0x0046_2DE0 => {
                if request.arguments[1] != crate::frontend_video::VIDEO_DEVICE_CELL {
                    return Err(VideoError::Cell(request.arguments[1]).into());
                }
                Row::Identity(
                    request.arguments[0] as i32,
                    std::borrow::Cow::Owned(self.video.device_name_for_draw()?),
                )
            }
            _ => unreachable!("settings interception covers five callbacks"),
        };
        let focal = self.context()?.projection.focal;
        let mut host = SoftwareMenu::new(SoftwareMenuRequest {
            scene: self.scene,
            resources,
            focal,
            process: self.process,
            state: &mut self.state.menu,
        })?;
        host.publish_row_origin(request.context_origin_pixels)?;
        match row {
            Row::Value(value) => {
                settings_row::draw_settings_row(request, value, &resources, &mut host)
            }
            Row::Volume(value) => {
                settings_row::draw_volume_row(request, value, &resources, &mut host)
            }
            Row::Identity(label, text) => settings_row::draw_identity_row(
                request,
                label,
                text.as_ref(),
                &resources,
                &mut host,
            ),
            Row::Port(label, value) => {
                if crate::frontend_video::modern_row_locked(
                    &settings.modern(),
                    settings.modern_support(),
                    request.arguments,
                ) {
                    request.descriptor.color =
                        crate::frontend_video::dimmed_row_color(request.descriptor.color);
                }
                let mut records = menu.data().records_mut();
                crate::frontend_video::refresh_disabled_rows(
                    &mut records.items,
                    &settings.modern(),
                    settings.modern_support(),
                );
                settings_row::draw_port_row(request, &label, &value, &resources, &mut host)
            }
        }
        .map_err(|error| match error {
            SettingsRowError::Resource(error) => FrontendRendererError::Resource(error),
            SettingsRowError::Host(error) => {
                FrontendRendererError::Menu(SoftwareMenuError::Text(MenuTextError::Backend(error)))
            }
            SettingsRowError::Layout(error) => {
                FrontendRendererError::Menu(SoftwareMenuError::Text(MenuTextError::Layout(error)))
            }
            SettingsRowError::Side(error) => FrontendRendererError::SettingsRow(error),
        })
    }

    /// `0045EE20`'s previews before its label: with the row's age above
    /// 0 or the menu's first layout (`+48`) without flag 2, the clip opens
    /// to the whole surface (renderer `+24`, left as it is) and the arenas'
    /// sprites go on their arc ([`crate::network_menu::arena_previews`]).
    fn draw_arena_previews(
        &mut self,
        request: &MenuRowDrawRequest,
        menu: &MenuState,
    ) -> Result<(), FrontendRendererError> {
        let first_layout = menu
            .data()
            .records()
            .layouts
            .first()
            .map(|layout| layout.flags);
        if request.descriptor.age_millis <= 0 && first_layout.unwrap_or(0) & 2 != 0 {
            return Ok(());
        }
        let cell = |address| {
            self.settings
                .setting_word(address)
                .ok_or(FrontendRendererError::SettingsCell { address })
        };
        let selected = cell(request.arguments[1])?;
        let cheats =
            cell(crate::network_menu::SETTINGS_ADDRESS + crate::network_menu::SETTING_CHEATS)?;
        let [width, height] = self.surface.dimensions(self.scene.pixels.frame);
        let surface = [width as i32, height as i32];
        MenuDrawHost::set_clip_rect(
            &mut self.menu()?,
            [0, 0, surface[0] as i16, surface[1] as i16],
        )?;
        for quad in crate::network_menu::arena_previews(
            surface,
            selected,
            self.process.animation().value(),
            cheats & 0x80 != 0,
            self.sine_quarter,
        ) {
            self.scene.draw_sprite_direct(quad)?;
        }
        Ok(())
    }

    fn scene_error(error: &FrontendRendererError) -> Option<&SoftwareSceneError> {
        match error {
            FrontendRendererError::Scene(scene)
            | FrontendRendererError::Submission(SceneResourceError::Backend(scene))
            | FrontendRendererError::Menu(SoftwareMenuError::Scene(scene)) => Some(scene),
            FrontendRendererError::Profile(ProfileDrawError::Host(SoftwareMenuError::Scene(
                scene,
            ))) => Some(scene),
            _ => None,
        }
    }

    fn resource_kind(handler: u8) -> Result<RenderResourceKind, FrontendRendererError> {
        match handler {
            4 => Ok(RenderResourceKind::Font),
            6 => Ok(RenderResourceKind::Style),
            _ => Err(FrontendRendererError::Handler(handler)),
        }
    }
}

impl MenuDrawHost for SoftwareFrontendFrame<'_, '_, '_> {
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
        self.process.colors().expect("initialized frontend")
    }
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error> {
        if matches!(
            request.callback_address,
            0x0045_3AE0
                | 0x0045_3D50
                | 0x0046_39D0
                | 0x0046_3A30
                | 0x0046_2DE0
                | crate::network_menu::ARENA_ROW
                | crate::network_menu::SCORE_ROW_DRAW
        ) {
            if request.callback_address == crate::network_menu::ARENA_ROW {
                self.draw_arena_previews(request, menu)?;
            }
            return self.draw_settings_family(request, menu);
        }
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
        let focal = self.context().map_err(MenuError::Host)?.projection.focal;
        let [width, height] = self.surface.dimensions(self.scene.pixels.frame);
        let mut host = SoftwareMenu::new(SoftwareMenuRequest {
            scene: self.scene,
            resources: self.resources,
            focal,
            process: self.process,
            state: &mut self.state.menu,
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
                owner_flag: self.owner_flag,
                surface_pixels: [width as i32, height as i32],
            },
            &self.resources,
            &mut host,
        )
        .map_err(FrontendRendererError::from)
        .map_err(MenuError::Host)
    }
    fn draw_text_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        text: &[u8],
    ) -> Result<(), MenuError<Self::Error>> {
        let origin = request.context_origin_pixels;
        self.menu()
            .and_then(|mut menu| {
                Ok(menu.draw_row_text(crate::menu_text::MenuRowTextRequest {
                    descriptor: &mut request.descriptor,
                    text,
                    placement: crate::menu_text::MenuTextPlacement::Left,
                    context_origin_pixels: origin,
                })?)
            })
            .map(|_| ())
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

impl ProfileTextHost for SoftwareMenu<'_, '_, '_> {
    type Error = SoftwareMenuError;
    fn row_text(
        &mut self,
        request: MenuRowTextRequest<'_>,
    ) -> Result<MenuTextOutcome, Self::Error> {
        self.draw_row_text(request)
    }
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        self.draw_raw_text(request)
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        MenuDrawHost::set_clip_rect(self, clip)
    }
    fn published_origin_pixels(&self) -> [i32; 2] {
        SoftwareMenu::published_origin_pixels(self)
    }
}

impl ScreenRendererPreparationHost<FrontendRendererError> for SoftwareFrontendFrame<'_, '_, '_> {
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
        FrontendFrameHost::scene_submit(self, renderer, SceneCommand::BeginBatch)
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

impl FrontendFrameHost for SoftwareFrontendFrame<'_, '_, '_> {
    fn capture_caller_context(&mut self) -> Result<(), Self::Error> {
        let screen = self
            .registry
            .current_screen(self.attachment.session)?
            .ok_or(FrontendRendererError::CallerContext)?;
        let owner = self.registry.screen(screen)?.attachment.context_owner;
        self.registry.backend(owner)?;
        self.state.caller_context = Some(owner);
        Ok(())
    }
    fn reset_renderer(&mut self, renderer: Self::Renderer) -> Result<(), Self::Error> {
        self.selected(renderer)?;
        self.scene.reset_renderer();
        Ok(())
    }
    fn validate_surface(&mut self) -> Result<(), Self::Error> {
        self.surface
            .validate_dimensions(self.scene.pixels.frame)
            .map_err(|mismatch| FrontendRendererError::Surface {
                actual: mismatch.actual,
                expected: mismatch.expected,
            })
    }
    fn scene_environment(&mut self) -> Result<FrontendSceneEnvironment, Self::Error> {
        Ok(FrontendSceneEnvironment {
            projection: self.context()?.projection,
            inherited_view: self.scene.environment.view,
        })
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        let [width, height] = self.surface.dimensions(self.scene.pixels.frame);
        Ok([width as i32, height as i32])
    }
    fn scene_submit(
        &mut self,
        renderer: Self::Renderer,
        command: SceneCommand,
    ) -> Result<(), Self::Error> {
        self.selected(renderer)?;
        if self.follow_menu_trace(command) {
            return Ok(());
        }
        ResidentSceneHost {
            resources: self.resources,
            backend: &mut *self.scene,
        }
        .submit(command)?;
        // The flush after the footer drew what the trace left: later draws
        // (the menu's rows) are the raster's.
        if matches!(command, SceneCommand::FlushBatch)
            && self.state.menu_trace.phase == MenuTracePhase::Traced
        {
            self.scene.end_scene_feed();
            self.state.menu_trace.phase = MenuTracePhase::Idle;
        }
        Ok(())
    }
    fn scene_starting(&mut self) -> Result<(), Self::Error> {
        // Traced wherever the frame traces rays (Vulkan with Path Tracing
        // on): elsewhere the feed does not begin and
        // the menu draws as retail's.
        self.state.menu_trace.phase = MenuTracePhase::Armed;
        Ok(())
    }
    fn scene_submitted(&mut self) -> Result<(), Self::Error> {
        match self.state.menu_trace.phase {
            MenuTracePhase::Collecting => {
                // The sun shines from where it stands, seen from the
                // models' middle.
                let trace = self.state.menu_trace;
                if let (Some(sun), (sum, count @ 1..)) = (trace.sun, trace.bodies) {
                    let toward: [f32; 3] = std::array::from_fn(|axis| {
                        (i64::from(sun[axis]) - sum[axis] / count) as f32
                    });
                    let length = toward.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
                    if let (Some(builder), true) = (self.scene.scene_feed_mut(), length > 0.0) {
                        builder.feed.sun.direction = toward.map(|axis| axis / length);
                    }
                }
                self.scene.trace_scene_feed();
                self.state.menu_trace.phase = MenuTracePhase::Traced;
            }
            MenuTracePhase::Armed => self.state.menu_trace.phase = MenuTracePhase::Idle,
            _ => {}
        }
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        Ok(ResidentSceneHost {
            resources: self.resources,
            backend: &mut *self.scene,
        }
        .footer_style_word()?)
    }
    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Self::Error> {
        Ok(ResidentSceneHost {
            resources: self.resources,
            backend: &mut *self.scene,
        }
        .sprite_dimensions(slot)?)
    }
    fn resource_state(
        &mut self,
        handler: u8,
        slot: u16,
    ) -> Result<FrameResourceState, Self::Error> {
        let kind = Self::resource_kind(handler)?;
        let capacity = self.resources.pool_capacity(kind);
        if usize::from(slot) > capacity {
            return Ok(FrameResourceState::OutOfRange);
        }
        if usize::from(slot) == capacity {
            return Err(FrontendRendererError::PoolBoundary { handler, slot });
        }
        match self.resources.resolve(RenderResourceRequest {
            kind,
            argument: i32::from(slot),
        }) {
            Ok(_) => Ok(FrameResourceState::Resident),
            Err(RenderResourceError::NotResident { .. }) => Ok(FrameResourceState::Vacant),
            Err(error) => Err(error.into()),
        }
    }
    fn load_resource(&mut self, handler: u8, slot: u16) -> Result<(), Self::Error> {
        Err(FrontendRendererError::LazyResource { handler, slot })
    }
    fn native_error_code(error: &Self::Error) -> Option<u32> {
        Self::scene_error(error).and_then(SoftwareSceneError::native_error_code)
    }
    fn dispose_caught_error(&mut self, error: Self::Error) -> Result<(), Self::Error> {
        let scene_error = Self::scene_error(&error).ok_or(FrontendRendererError::ErrorDisposal)?;
        if !self.scene.recover_frame_error(scene_error) {
            return Err(FrontendRendererError::ErrorDisposal);
        }
        drop(error);
        self.state.disposed_native_errors += 1;
        Ok(())
    }
}
