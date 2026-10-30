//! SDL window, input and presentation around the scheduled port session.
//!
//! The session owns the frontend and text-entry screens, their transitions
//! and effects; this loop supplies the clock, physical input, the persistent
//! renderer (the CPU raster or the OpenGL device under the same submissions)
//! and the SDL surface. Transitions the port has not implemented end the run
//! with an explicit boundary instead of a manufactured phase.

use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use sdl2::{
    event::{Event, WindowEvent},
    keyboard::Scancode,
};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    display_selection::{original_size, tier_for, DisplaySelection, ORIGINAL_SIZES},
    frontend_assets::{FrontendAssetError, FrontendAssets},
    frontend_audio::MusicState,
    frontend_camera::AnimationClock,
    frontend_display::{frontend_display_dimensions, frontend_startup, FrontendDisplayError},
    frontend_renderer::{FrontendRenderContext, FrontendRendererState},
    frontend_runtime::{FrontendPlatform, FrontendRuntimeError, SdlPlatform},
    frontend_scene::{FrontendScene, SceneProjection},
    frontend_settings::FrontendSettings,
    menu_basis::MenuBasisMath,
    model_primitives::PrimitiveTables,
    name_entry::NameCompletion,
    name_entry_renderer::NameEntryRendererState,
    platform_audio::PlatformAudio,
    platform_input::InputNotification,
    presentation::{
        ClassicScaling, NativeFrame, OutputPresentation, RendererBackend, RendererChoice,
    },
    profile_store::{ProfileStore, ProfileStoreError},
    render_resources::RenderResources,
    scheduler::MIN_TICK_MICROS,
    session::{PhaseTransition, SessionDriver, SessionError, SessionPhase},
    session_platform::{PortBootError, PortSessionErrorOf, SessionBoundary},
    software_frame::SoftwarePixelStats,
    software_scene::{
        SoftwareScene, SoftwareSceneConfig, SoftwareSceneError, SoftwareSceneRequest,
        SoftwareSceneResume, SoftwareSceneState, SoftwareSceneStats,
    },
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

#[cfg(test)]
use crate::{
    frontend_camera::{CameraMotion, FrontendCamera, FrontendEntry},
    frontend_scene::{draw_footer, FrontendSceneHost, SceneCommand, SceneFrame},
    scene_resources::{ResidentSceneHost, SceneResourceError},
    software_frame::SoftwareFrame,
};

/// Reproducible platform key notifications before a one-based rendered frame.
/// These traverse the physical device, original bindings, FIFO and screen input.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendAppInput {
    pub frame: u64,
    /// SDL physical scancode name, e.g. "Down", "Return", or "Escape".
    pub key: String,
    pub pressed: bool,
}

/// Presentation-clock policy, separate from the native camera/animation math.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendAppClock {
    /// Millisecond host clock, matching 004A4890's timeGetTime * 1000 units.
    Realtime,
    /// Reproducible smoke/export sequence; not a claim about retail frame rate.
    FixedStep { micros: u32 },
}

#[derive(Clone, Debug)]
pub struct FrontendAppRequest {
    pub data_root: PathBuf,
    pub settings_path: PathBuf,
    pub profiles_path: PathBuf,
    pub input_replay: Vec<FrontendAppInput>,
    /// Explicit Resolution in physical pixels, at least 320x240. Without it
    /// or a variant override, startup uses the saved Resolution (the authored
    /// default is 640x480). The Display mode stays the saved one.
    pub dimensions: Option<[u32; 2]>,
    /// Port-only completed-frame scaling. None retains the saved scaling.
    pub scaling: Option<ClassicScaling>,
    /// The port's high frame-rate drive fix ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub vehicle_position_carry: Option<bool>,
    /// The port's gun mount fix ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub vehicle_gun_mounts: Option<bool>,
    /// The port's hovercraft fan fix ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub hovercraft_fans: Option<bool>,
    /// The port's exact world tick fix ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub exact_world_ticks: Option<bool>,
    /// Ultra ground detail ([`crate::modern_settings::ModernSettings`]).
    /// None retains the saved setting, which is off by default.
    pub ultra_ground: Option<bool>,
    /// The Ambience volume, 0 to 15 ([`crate::modern_settings::ModernSettings`]).
    /// None retains the saved setting, 10 by default.
    pub ambience: Option<u8>,
    /// Headphones ([`crate::modern_settings::ModernSettings`]). None retains
    /// the saved setting, off by default.
    pub headphones: Option<bool>,
    /// The port's database model fix ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub database_models: Option<bool>,
    /// The port's name entry tick fix ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub exact_frontend_ticks: Option<bool>,
    /// The port's guided missile mount fix
    /// ([`crate::frontend_settings::PortFixes`]). None retains the saved
    /// setting, which is on by default.
    pub guided_mounts: Option<bool>,
    /// The port's completion bonus fix
    /// ([`crate::frontend_settings::PortFixes`]). None retains the saved
    /// setting, which is on by default.
    pub completion_bonus: Option<bool>,
    /// The port's paced scientists, alien ships and creatures
    /// ([`crate::frontend_settings::PortFixes`]). None retains the saved
    /// setting, which is on by default.
    pub paced_actors: Option<bool>,
    /// The port's 64 sound voices ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub splash_voice_cap: Option<bool>,
    pub mission_completion: Option<bool>,
    /// The port's open Cheats Menu ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is off by default.
    pub all_cheats: Option<bool>,
    /// Pelagia, the prototype world ([`crate::frontend_settings::PortFixes`]).
    /// None retains the saved setting, which is on by default.
    pub prototype_world: Option<bool>,
    /// Pelagia's rework ([`crate::frontend_settings::PortFixes`]): world 1
    /// gains its lagoon, crystals, return gate and factory stock. None
    /// retains the saved setting, which is on by default.
    pub pelagia_rework: Option<bool>,
    /// Port-only renderer: OpenGL, falling back to the CPU raster, or one
    /// of them explicitly. Software keeps frames deterministic. None retains
    /// the saved renderer, Auto by default.
    pub renderer: Option<RendererChoice>,
    /// The pad's layout: PC retail's joystick or the PlayStation pad. None
    /// retains the saved layout, PC original by default.
    pub controller_layout: Option<crate::platform_pad::ControllerLayout>,
    /// Explicit OVL display tier, selecting its original size; it must be
    /// the tier of explicit dimensions.
    pub variant: Option<u8>,
    /// None selects settings +64 as 004389B0 does: above 5 selects 0.
    pub language: Option<u8>,
    pub hidden: bool,
    pub max_frames: Option<u64>,
    /// Save the last successfully presented frame on normal termination.
    pub snapshot: Option<PathBuf>,
    /// Save the frame presented as the frame count reaches each number.
    pub snapshots_at: Vec<(u64, PathBuf)>,
    /// Observe actual full-resolution presentation and mixer bytes in this new directory.
    pub record_session: Option<PathBuf>,
    /// Port-only `--audio-out`: render the mix from the frames, each frame
    /// its fixed step's share of samples, into this WAV file (16-bit stereo,
    /// 44.1 kHz), so a fixed-step run gives the same file every time.
    pub audio_out: Option<PathBuf>,
    /// Debug-only new-game mission override; None keeps retail 22.
    pub mission: Option<u32>,
    /// The debug start harness's state ([`crate::harness_start`]).
    pub start: Option<crate::harness_start::HarnessStart>,
    pub clock: FrontendAppClock,
    /// Port-only `--session-log`: record the session's clock samples and
    /// keys to this `.replay` file, and print status lines (phase changes,
    /// keys, a periodic summary) to stderr for the log.
    pub session_record: Option<SessionRecordRequest>,
    /// Port-only `--replay-session`: drive the clock and keys from a
    /// recording instead of the host. The run ends when it runs out.
    pub session_replay: Option<crate::session_log::SessionReplay>,
}

#[derive(Clone, Debug)]
pub struct SessionRecordRequest {
    pub path: PathBuf,
    /// The command line, stored in the recording for its replay.
    pub args: Vec<String>,
}

/// The startup owner supplies recovered display/global values explicitly.
pub struct FrontendAppStartup {
    pub scene: SoftwareSceneConfig,
    pub projection: SceneProjection,
    pub clear_rgba: [u8; 4],
}

#[derive(Debug)]
pub struct FrontendAppReport {
    pub frames: u64,
    /// Session phase when the run ended.
    pub phase: SessionPhase,
    /// Completed transitions in order, beginning with the title-to-frontend
    /// transition.
    pub transitions: Vec<PhaseTransition>,
    /// 0045AD30 completions of the text-entry screen, in execution order.
    pub name_completions: Vec<NameCompletion>,
    pub menu_rows: usize,
    pub glyph_passes: usize,
    pub name_entry_draws: usize,
    pub exit_requested: bool,
    pub music: MusicState,
    pub audio: crate::platform_audio::AudioPlaybackStats,
    pub scene: SoftwareSceneStats,
    pub pixels: SoftwarePixelStats,
    /// Frontend camera and animation, when a frontend screen remains.
    pub camera_eye: Option<[i32; 3]>,
    pub camera_look_at: Option<[i32; 3]>,
    pub world_draw: Option<crate::world_scene::WorldDrawReceipt>,
    pub animation: Option<AnimationClock>,
    /// Base screen age of the remaining screen.
    pub age_millis: Option<i32>,
    pub snapshot: Option<PathBuf>,
    pub dimensions: [u32; 2],
    /// Physical SDL output size; `dimensions` remains the native CPU framebuffer.
    pub output_dimensions: [u32; 2],
    /// The Display and Resolution the session ended with.
    pub display: DisplaySelection,
    pub scaling: ClassicScaling,
    /// The renderer that drew the session.
    pub renderer: RendererBackend,
    pub display_rebuilds: usize,
    pub loading: LoadingPresentationStats,
    /// The window covers its display: Full Screen or Borderless.
    pub fullscreen: bool,
}

mod cold_start;
mod display;
mod loading;
pub use loading::LoadingPresentationStats;
mod session_host;

use display::InheritedScene;
use session_host::{AppFrame, AppFrameError, AppIo, AppRender, StagedDisplay};

#[derive(Debug, Error)]
pub enum FrontendAppError {
    #[error("session recording: {0}")]
    Capture(String),
    #[error("frontend request: {0}")]
    Request(&'static str),
    #[error("session log: {0}")]
    SessionLog(String),
    #[error("audio render: {0}")]
    AudioOut(String),
    #[error(transparent)]
    Resource(#[from] crate::Error),
    #[error(transparent)]
    Assets(#[from] FrontendAssetError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    LoadingModel(#[from] crate::loading_model::LoadingModelStartupError),
    #[error(transparent)]
    Backend(#[from] SoftwareSceneError),
    #[error(transparent)]
    Display(#[from] FrontendDisplayError),
    #[error(transparent)]
    Runtime(#[from] FrontendRuntimeError),
    #[error(transparent)]
    Profiles(#[from] ProfileStoreError),
    #[error(transparent)]
    Boot(#[from] PortBootError),
    #[error("session frame {frame} failed: {source}")]
    Session {
        frame: u64,
        #[source]
        source: SessionError<AppFrameError>,
    },
    /// The session requested a transition the port has not implemented.
    /// The report describes the run up to that point.
    #[error("session reached an unimplemented transition {boundary:?} after {} frames", report.frames)]
    SessionBoundary {
        boundary: SessionBoundary,
        report: Box<FrontendAppReport>,
    },
    /// The title factory 00404A10 (construction, device enumeration, video
    /// selection) failed before the scheduled loop.
    #[error("title boot failed: {0}")]
    Entry(#[source] SessionError<PortSessionErrorOf<SdlPlatform>>),
    #[error("session teardown failed: {0}")]
    Teardown(String),
    #[error("SDL frontend presentation: {0}")]
    Sdl(String),
    #[error("cannot write frontend snapshot {path}: {source}")]
    Snapshot {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Png(#[from] png::EncodingError),
}

/// How the renderer epoch ended.
enum Stop {
    /// Retire the renderer's residency snapshot and draw this iteration with
    /// the rebound (or, after a display reset, replacement) renderer.
    Rebind,
    Closed,
    Finished,
    Failed(SessionError<AppFrameError>),
}

/// The renderer the next epoch starts from.
enum Carry {
    Fresh(SoftwareSceneConfig),
    Retained(SoftwareSceneState),
}

/// Boot through the real title factory and run the scheduled session: title,
/// frontend, the Start Game profile menu and phase-9 name entry, retaining
/// renderer, input, menu and audio state. The title video backend is absent,
/// so the title finishes to phase 2 on its first tick; world phases are not
/// implied.
pub fn run_frontend(request: FrontendAppRequest) -> Result<FrontendAppReport, FrontendAppError> {
    validate_request(&request)?;
    let assets = FrontendAssets::open(&request.data_root)?;
    let loading_model_template =
        crate::loading_model::LoadingModelTemplate::native_startup(assets.executable_bytes())?;
    let mut profiles = ProfileStore::open(&request.profiles_path)?;
    let executable = FrontendExecutable::parse(assets.executable_bytes())?;
    let tables = PrimitiveTables::from_executable(executable)?;
    let mut settings = FrontendSettings::load_or_defaults(&request.settings_path, &assets)
        .map_err(FrontendRuntimeError::from)?;
    if let Some(scaling) = request.scaling {
        settings.set_scaling(scaling);
    }
    if let Some(renderer) = request.renderer {
        settings.set_renderer(renderer);
    }
    if let Some(layout) = request.controller_layout {
        settings.set_controller_layout(layout);
    }
    if let Some(carry) = request.vehicle_position_carry {
        let mut fixes = settings.fixes();
        fixes.vehicle_position_carry = carry;
        settings.set_fixes(fixes);
    }
    if let Some(mounts) = request.vehicle_gun_mounts {
        let mut fixes = settings.fixes();
        fixes.vehicle_gun_mounts = mounts;
        settings.set_fixes(fixes);
    }
    if let Some(fans) = request.hovercraft_fans {
        let mut fixes = settings.fixes();
        fixes.hovercraft_fans = fans;
        settings.set_fixes(fixes);
    }
    if let Some(exact) = request.exact_world_ticks {
        let mut fixes = settings.fixes();
        fixes.exact_world_ticks = exact;
        settings.set_fixes(fixes);
    }
    if let Some(ultra) = request.ultra_ground {
        let mut modern = settings.modern();
        modern.ultra_ground = ultra;
        settings.set_modern(modern);
    }
    if let Some(level) = request.ambience {
        let mut modern = settings.modern();
        modern.ambience = level.min(crate::modern_settings::MOST_AMBIENCE);
        settings.set_modern(modern);
    }
    if let Some(on) = request.headphones {
        let mut modern = settings.modern();
        modern.headphones = on;
        settings.set_modern(modern);
    }
    if let Some(models) = request.database_models {
        let mut fixes = settings.fixes();
        fixes.database_models = models;
        settings.set_fixes(fixes);
    }
    if let Some(exact) = request.exact_frontend_ticks {
        let mut fixes = settings.fixes();
        fixes.exact_frontend_ticks = exact;
        settings.set_fixes(fixes);
    }
    if let Some(mounts) = request.guided_mounts {
        let mut fixes = settings.fixes();
        fixes.guided_mounts = mounts;
        settings.set_fixes(fixes);
    }
    if let Some(bonus) = request.completion_bonus {
        let mut fixes = settings.fixes();
        fixes.completion_bonus = bonus;
        settings.set_fixes(fixes);
    }
    if let Some(paced) = request.paced_actors {
        let mut fixes = settings.fixes();
        fixes.paced_actors = paced;
        settings.set_fixes(fixes);
    }
    if let Some(more) = request.splash_voice_cap {
        let mut fixes = settings.fixes();
        fixes.splash_voice_cap = more;
        settings.set_fixes(fixes);
    }
    if let Some(on) = request.mission_completion {
        let mut fixes = settings.fixes();
        fixes.mission_completion = on;
        settings.set_fixes(fixes);
    }
    if let Some(all) = request.all_cheats {
        let mut fixes = settings.fixes();
        fixes.all_cheats = all;
        settings.set_fixes(fixes);
    }
    if let Some(on) = request.prototype_world {
        let mut fixes = settings.fixes();
        fixes.prototype_world = on;
        settings.set_fixes(fixes);
    }
    if let Some(on) = request.pelagia_rework {
        let mut fixes = settings.fixes();
        fixes.pelagia_rework = on;
        settings.set_fixes(fixes);
    }
    // Port option: with Pelagia on, a game saved there loads into it.
    profiles.set_pelagia_loads(settings.fixes().prototype_world);
    if settings.fixes().completion_bonus {
        let completed = profiles.completed_campaigns();
        if completed > 0 {
            eprintln!(
                "Finished campaigns in the save list: {completed}; every game gets the completion bonus (the Cranial Implanter at each respawn)"
            );
        }
    }
    let saved = requested_display(&request, settings.display_selection())?;
    crate::frontend_video::prepare_video_startup(settings.words_mut());
    let installation = Installation::open(&request.data_root)?;
    // Startup display setup (00438D1B..00438D35) reselects with 004389B0's
    // settings language, so a language chosen on the startup screen survives.
    let language = request.language.unwrap_or(settings.residency_language());
    settings.select_language(language);
    let frontend_scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let basis = MenuBasisMath::new(assets.sine_table().values);
    let sine = assets.sine_table().values;

    display::declare_dpi_awareness();
    let sdl = sdl2::init().map_err(FrontendAppError::Sdl)?;
    let video = sdl.video().map_err(FrontendAppError::Sdl)?;
    let audio = sdl.audio().map_err(FrontendAppError::Sdl)?;
    let mut output = display::Output::open(&video, settings.renderer(), saved.size, request.hidden)
        .map_err(FrontendAppError::Sdl)?;
    // A Vulkan that fails to start runs OpenGL, and the remaster's sound
    // follows what runs; the saved choice stays.
    settings.set_running(output.backend());
    let creator = output.texture_creator();
    let mut events = sdl.event_pump().map_err(FrontendAppError::Sdl)?;
    // A hidden run reads no pad, so probes stay repeatable.
    let mut pads = if request.hidden {
        None
    } else {
        crate::platform_pad::SdlPads::new(&sdl, settings.controller_layout())
            .map_err(|error| eprintln!("Pads unavailable: {error}"))
            .ok()
    };
    let mut platform = SdlPlatform {
        frontend: FrontendPlatform::new(
            {
                let mut sound = PlatformAudio::open(&audio, &request.data_root, &assets)
                    .map_err(FrontendRuntimeError::from)?;
                let fixes = settings.fixes();
                sound.set_voice_limits(
                    crate::process_sound::RETAIL_VOICE_LIMIT,
                    fixes.splash_voice_limit(),
                );
                // A hidden run is a probe: silent (probes
                // at full volume are too loud), its mix and stats kept.
                sound.set_muted(request.hidden);
                sound.set_ambience_volume(settings.ambience());
                sound.set_headphones(settings.headphones());
                settings.set_ambience_sounds(sound.remastered().is_some());
                sound
            },
            settings,
            display::display_devices(&video)?,
        )?,
        keyboard: None,
        profiles,
        data_root: request.data_root.clone(),
        title_video: crate::title_playback::TitleVideoState::default(),
    };
    // 00438AD0 settles the display before the residency follows its tier.
    let device = platform.frontend.video.published_device();
    let modes = display::monitor_modes(&video, device);
    let (chosen, output_dimensions) =
        display::start_display(output.window_mut(), device, &modes, saved)?;
    let frontend = &mut platform.frontend;
    frontend.settings.set_desktop(modes.desktop);
    frontend.settings.set_display_selection(chosen);
    // Filtering beyond Bilinear is Vulkan's; elsewhere it draws as Bilinear
    // and stays saved.
    frontend
        .settings
        .set_filtering_most(crate::frontend_video::filtering_most(output.backend()));
    output.set_filtering(frontend.settings.filtering());
    // The game the session plays, now that the renderer is known: a
    // replay's as recorded, whatever renders it, with the rows its device
    // offered.
    let replayed = request
        .session_replay
        .as_ref()
        .and_then(|replay| replay.game.clone());
    let game_view = replayed.as_ref().map_or_else(
        || {
            crate::presentation::GameView::at_start(
                output.backend(),
                frontend
                    .settings
                    .modern()
                    .effective(output.modern_support())
                    .ray_tracing,
            )
        },
        |game| game.view,
    );
    let remaster = game_view != crate::presentation::GameView::Retail;
    let support = replayed
        .as_ref()
        .and_then(|game| game.support)
        .unwrap_or(output.modern_support());
    frontend.settings.set_modern_support(support);
    output.set_modern(frontend.settings.modern());
    // The Filtering and Vulkan rows the device runs now, which a change in
    // the in-world Options replaces from the next frame.
    let mut applied_options = (frontend.settings.filtering(), frontend.settings.modern());
    let tier = chosen.tier(modes.desktop);
    frontend.display_variant = tier;
    if request.record_session.is_some() && output_dimensions != [1024, 768] {
        return Err(FrontendAppError::Request(
            "session recording requires a 1024x768 window or full screen",
        ));
    }
    let mut presentation = OutputPresentation {
        output: output_dimensions,
        scaling: frontend.settings.scaling(),
    };
    // The tier's frame, or a wider one at its height for a widescreen output.
    let mut native_frame = NativeFrame::for_output(
        original_size(tier),
        presentation.output,
        presentation.scaling,
    );
    // A replay draws the remaster's game at its recording's frame, which
    // that game sees by; retail's sees by the height alone.
    if let Some(pinned) = replayed.as_ref().and_then(|game| game.frame) {
        let live = native_frame.size;
        let [width, height] = live;
        let [recorded_width, recorded_height] = pinned;
        if pinned[1] != live[1] {
            eprintln!(
                "Infestation replay: the recording's game opened at {recorded_width}x{recorded_height}, \
                 this run's frame is {width}x{height} (another resolution tier): its game may part \
                 from the recording"
            );
        } else if remaster && pinned != live {
            eprintln!(
                "Infestation replay: the remaster's game draws at its recording's \
                 {recorded_width}x{recorded_height}, not this output's {width}x{height}"
            );
            let pin = crate::presentation::FramePin { live, pinned };
            native_frame = pin.apply(native_frame);
            frontend.settings.set_frame_pin(Some(pin));
        }
    }
    let session_game = crate::presentation::SessionGame {
        view: game_view,
        frame: Some(native_frame.size),
        support: remaster.then_some(support),
        far: replayed.and_then(|game| game.far).or_else(|| {
            (game_view == (crate::presentation::GameView::Remaster { traced: true }))
                .then(|| crate::modern_tuning::current().traced_far())
        }),
    };
    frontend.settings.set_game(session_game.clone());
    let mut dimensions = native_frame.size;
    let selection = ResourceSelection {
        difficulty: tier as u8,
        language,
    };
    let mut runtime = ResidentRuntime::new(installation)?;
    runtime.apply(ResidencyRequest::Select { selection })?;
    runtime.apply(ResidencyRequest::Boot)?;
    // Display resources belong to the booted root. The real screen factory
    // activates leaf47 between music pause and resume.
    let mut startup = frontend_startup(&runtime, native_frame)?;
    crate::frontend_video::apply_video_pipeline(frontend.settings.words(), &mut startup.scene)
        .map_err(|error| FrontendRuntimeError::DisplayReset(error.to_string()))?;
    let mut texture = display::texture(creator.as_ref(), dimensions)?;
    let mut frame = output
        .frame(
            dimensions,
            presentation.output,
            presentation.scaling,
            startup.clear_rgba,
        )
        .map_err(FrontendAppError::Sdl)?;
    let capture = request
        .record_session
        .as_ref()
        .map(|directory| {
            crate::session_capture::SessionCapture::start(directory, output_dimensions)
                .map_err(|error| FrontendAppError::Capture(error.to_string()))
        })
        .transpose()?;
    platform
        .frontend
        .audio
        .set_capture(capture.as_ref().map(|owner| owner.handle()));
    let mut session = crate::session_platform::PortSession::new(
        assets,
        runtime,
        platform,
        FrontendRenderContext {
            projection: startup.projection,
        },
        request.mission,
    )?;
    // The Vulkan rows are the remaster's game's, which a replay navigates
    // as recorded on any renderer.
    session.set_modern_rows(remaster);
    if let Some(start) = request.start.clone() {
        session.set_harness_start(start);
    }
    {
        let loans = session.split();
        loans
            .platform
            .frontend
            .audio
            .acquire_session_sounds(loans.sounds)
            .map_err(FrontendRuntimeError::from)?;
    }
    let model_process = session.model_process().clone();
    // The tracer and its upscaler are made now, before the title and the
    // session's clock: the title video's clock starts when the title is
    // built and its sound with its first frame. Made after that frame they
    // held its picture still under the running sound, for 0.7 s, and for 3 s
    // when a new build's shaders compiled cold (plan item 24). Retail's
    // loading line shows meanwhile.
    if frame.traces_rays() {
        let resources = RenderResources::new(session.residency());
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: model_process.clone(),
            resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })?;
        if let Err(error) = cold_start::show_loading_line(
            &mut scene,
            resources,
            language,
            dimensions,
            startup.clear_rgba,
            &mut output,
            texture.as_mut(),
            presentation.scaling,
        ) {
            eprintln!("Infestation: the loading line before the title failed: {error}");
        }
        scene.prepare_trace();
    }
    let mut clock = AppClock::new(request.clock);
    if let Some(record) = &request.session_record {
        let mut recorder = crate::session_log::SessionRecorder::create(&record.path, &record.args)
            .map_err(|error| FrontendAppError::SessionLog(error.to_string()))?;
        recorder.game(&session_game);
        clock.recorder = Some(recorder);
    }
    if let Some(replay) = &request.session_replay {
        clock.replay = Some(ReplayCursor {
            entries: replay.entries.iter().cloned().collect(),
            pad: None,
            exhausted: false,
            resume: replay.resume,
        });
    }
    let mut status = request.session_record.as_ref().map(|_| SessionStatus {
        transitions: 0,
        frames: 0,
        seconds: 0.0,
        wall: Instant::now(),
    });
    // 004944D0 samples the clock when the session is constructed.
    let mut driver = SessionDriver::new(clock.sample());
    // Factory 00404A10 over the initial Title state: with the video absent
    // this makes no phase request, and the first linked tick finishes to
    // phase 2 through the real scheduler transition.
    session
        .boot_title()
        .map_err(SessionError::Host)
        .map_err(FrontendAppError::Entry)?;

    let window_id = output.window().id();
    // As retail, input only while the game's window has focus, and no pointer
    // over it: the pad is read, and the mouse held in relative mode (hidden,
    // kept in the window), only then.
    let mouse = sdl.mouse();
    let mut focused = !request.hidden && output.window().has_input_focus();
    mouse.set_relative_mouse_mode(focused);
    let mut snapshot = (request.snapshot.is_some() || !request.snapshots_at.is_empty())
        .then(|| session_host::FrameSnapshot::new(dimensions));
    let mut frontend_state = FrontendRendererState::default();
    let mut name_entry_state = NameEntryRendererState::default();
    let mut loading_stats = LoadingPresentationStats::default();
    let mut scene_stats = SoftwareSceneStats::default();
    let mut pixel_stats = SoftwarePixelStats::default();
    let mut display_rebuilds = 0;
    let mut staged: Option<StagedDisplay<'_>> = None;
    let mut notifications = Vec::new();
    let mut completions = Vec::new();
    let mut frames = 0_u64;
    let mut replay_cursor = 0_usize;
    // The port's ambience, from the session's state each frame.
    let mut ambience = crate::ambience::Ambience::default();
    ambience.wait = request.audio_out.is_some();
    // The crystals the guns' bolts strike (item 26), found per world.
    let mut crystal_survey = crate::crystal_flares::CrystalSurvey::default();
    // The mix rendered from the frames: the device's own thread pauses.
    let mut audio_out = match &request.audio_out {
        Some(path) => {
            session.platform_mut().frontend.audio.set_offline(true);
            Some(
                crate::audio_out::AudioOut::create(path)
                    .map_err(|error| FrontendAppError::AudioOut(error.to_string()))?,
            )
        }
        None => None,
    };
    // After a rebind the admitted iteration continues at its draw.
    let mut present_pending = false;
    let mut carry = Carry::Fresh(startup.scene);

    let stop = 'epochs: loop {
        let snapshot_runtime = session.residency().clone();
        // The database's second residency (a port fix) rebinds the same way.
        let database_runtime = session.database_model_residency().cloned();
        // Pelagia's borrowed model bank, while world 1 is loaded: world
        // loads retire the epoch through the main residency, so no extra
        // generation is needed to pick the loan up or drop it.
        let pelagia_runtime = session.pelagia_model_residency().cloned();
        let epoch = (
            session.residency_generation(),
            snapshot_runtime.revision(),
            session.database_models_generation(),
        );
        let mut resources = RenderResources::new(&snapshot_runtime);
        if let Some(strings) = session.world_strings() {
            resources = resources.with_world_strings(strings);
        }
        if let Some(models) = pelagia_runtime.as_ref() {
            resources = resources.with_model_fallback(models);
        }
        let database_resources = database_runtime.as_ref().map(RenderResources::new);
        let mut backend = match carry {
            Carry::Fresh(config) => SoftwareScene::new(SoftwareSceneRequest {
                model_process: model_process.clone(),
                resources,
                tables: &tables,
                frame: &mut frame,
                config,
            })?,
            Carry::Retained(state) => SoftwareScene::resume(
                state,
                SoftwareSceneResume {
                    resources,
                    tables: &tables,
                    frame: &mut frame,
                },
            )?,
        };
        // Resident terrain snapshot for grounding controls, refreshed every
        // epoch like the resource snapshot (world loads retire the epoch).
        backend.set_terrain(session.world().map(|world| world.terrain.clone()));
        let stop = 'frames: loop {
            // The frame the [input] and [phase] lines number.
            crate::crash_log::heartbeat(frames + 1);
            if !present_pending {
                let polled = notifications.len();
                for event in events.poll_iter() {
                    if let (
                        true,
                        Event::KeyDown {
                            repeat: false,
                            scancode: Some(scancode),
                            ..
                        }
                        | Event::KeyUp {
                            scancode: Some(scancode),
                            ..
                        },
                    ) = (status.is_some(), &event)
                    {
                        let pressed = matches!(event, Event::KeyDown { .. });
                        eprintln!(
                            "[input] frame {}: {} {}",
                            frames + 1,
                            scancode.name(),
                            if pressed { "down" } else { "up" }
                        );
                    }
                    let notification = match event {
                        // WM_CLOSE clears the running flag before another iteration.
                        Event::Quit { .. } => break 'frames Stop::Closed,
                        Event::Window {
                            window_id: id,
                            win_event: WindowEvent::Close,
                            ..
                        } if id == window_id => break 'frames Stop::Closed,
                        Event::KeyDown {
                            window_id: id,
                            scancode: Some(scancode),
                            ..
                        } if id == window_id => Some(InputNotification::Key {
                            scancode,
                            pressed: true,
                        }),
                        Event::KeyUp {
                            window_id: id,
                            scancode: Some(scancode),
                            ..
                        } if id == window_id => Some(InputNotification::Key {
                            scancode,
                            pressed: false,
                        }),
                        Event::Window {
                            window_id: id,
                            win_event: WindowEvent::FocusGained,
                            ..
                        } if id == window_id && !request.hidden => {
                            focused = true;
                            mouse.set_relative_mouse_mode(true);
                            None
                        }
                        Event::Window {
                            window_id: id,
                            win_event: WindowEvent::FocusLost,
                            ..
                        } if id == window_id => {
                            focused = false;
                            mouse.set_relative_mouse_mode(false);
                            if let Some(pads) = pads.as_mut() {
                                pads.release(&mut notifications);
                            }
                            Some(InputNotification::FocusLost)
                        }
                        _ => None,
                    };
                    notifications.extend(notification);
                }
                if let Some(pads) = pads.as_mut().filter(|_| focused) {
                    pads.poll(&mut notifications);
                }
                // Sound only while the window has focus, as input (all
                // sound is cut off when the window is not focused); a hidden run stays silent. The mix runs on, so
                // the music and sounds come back where they are. Set each
                // frame: a reopened device starts unmuted.
                session
                    .platform_mut()
                    .frontend
                    .audio
                    .set_muted(request.hidden || !focused);
                if clock.replay.is_some() {
                    // The recording's keys stand in for the host's.
                    notifications.truncate(polled);
                    let replayed = clock.replay_inputs();
                    notifications.extend(replayed);
                }
                let scripted = notifications.len();
                while let Some(key) = request
                    .input_replay
                    .get(replay_cursor)
                    .filter(|key| key.frame == frames + 1)
                {
                    notifications.push(InputNotification::Key {
                        scancode: Scancode::from_name(&key.key).expect("validated replay scancode"),
                        pressed: key.pressed,
                    });
                    replay_cursor += 1;
                }
                if status.is_some() {
                    // Host keys were logged as they arrived; these came from
                    // a replay or the scripted input.
                    let start = if clock.replay.is_some() {
                        polled
                    } else {
                        scripted
                    };
                    for input in &notifications[start..] {
                        // The pad's reading repeats every frame.
                        if !matches!(input, InputNotification::Pad { .. }) {
                            eprintln!("[input] frame {}: {input:?} (replayed)", frames + 1);
                        }
                    }
                }
                // The frame's keys precede its sample in the recording.
                clock.record_inputs(&notifications[polled..]);
                let now_micros = clock.sample();
                clock.flush_recording();
                if clock.replay_exhausted() {
                    if !clock.resume_live() {
                        eprintln!("Session replay: the recording ends after {frames} frames");
                        break 'frames Stop::Finished;
                    }
                    eprintln!(
                        "Session replay: the recording ends after {frames} frames; the keys and \
                         the clock are live from here (--resume)"
                    );
                }
                let inherited = InheritedScene {
                    environment: backend.environment,
                    primitive: backend.primitive_state,
                };
                macro_rules! host {
                    () => {
                        AppFrame {
                            session: &mut session,
                            io: AppIo {
                                model_process: model_process.clone(),
                                output: &mut output,
                                creator: creator.as_ref(),
                                tables: &tables,
                                basis: &basis,
                                inherited,
                                successful_resets: &mut display_rebuilds,
                                staged: &mut staged,
                                notifications: &mut notifications,
                                now_micros,
                                clock: &mut clock,
                                loading_stats: &mut loading_stats,
                                completions: &mut completions,
                            },
                            render: Some(AppRender {
                                scene: &mut backend,
                                resources,
                                database_resources,
                                texture: &mut texture,
                                dimensions,
                                presentation,
                                frontend_scene: &frontend_scene,
                                sine_quarter: &sine,
                                template_projection: startup.projection,
                                loading_model_template: &loading_model_template,
                                frontend_state: &mut frontend_state,
                                name_entry_state: &mut name_entry_state,
                                snapshot: snapshot.as_mut(),
                                capture: capture.as_ref().map(|owner| owner.handle()),
                            }),
                        }
                    };
                }
                let first_pass = crate::cpu_profile::scope("frame: first pass (input, tick)");
                let ticked = driver.first_pass(now_micros, &mut host!());
                drop(first_pass);
                match ticked {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => break 'frames Stop::Failed(error),
                }
                let linked_update = crate::cpu_profile::scope("frame: linked update");
                let updated = driver.linked_update(&mut host!());
                drop(linked_update);
                if let Err(error) = updated {
                    break 'frames Stop::Failed(error);
                }
                // A display reset or residency change retires this renderer
                // before the iteration's draw.
                if staged.is_some()
                    || (
                        session.residency_generation(),
                        session.residency().revision(),
                        session.database_models_generation(),
                    ) != epoch
                {
                    present_pending = true;
                    break 'frames Stop::Rebind;
                }
            }
            present_pending = false;
            let linked_present = crate::cpu_profile::scope("frame: linked present (draw)");
            let result = driver.linked_present(&mut AppFrame {
                session: &mut session,
                io: AppIo {
                    model_process: model_process.clone(),
                    output: &mut output,
                    creator: creator.as_ref(),
                    tables: &tables,
                    basis: &basis,
                    inherited: InheritedScene {
                        environment: backend.environment,
                        primitive: backend.primitive_state,
                    },
                    successful_resets: &mut display_rebuilds,
                    staged: &mut staged,
                    notifications: &mut notifications,
                    now_micros: 0,
                    clock: &mut clock,
                    loading_stats: &mut loading_stats,
                    completions: &mut completions,
                },
                render: Some(AppRender {
                    scene: &mut backend,
                    resources,
                    database_resources,
                    texture: &mut texture,
                    dimensions,
                    presentation,
                    frontend_scene: &frontend_scene,
                    sine_quarter: &sine,
                    template_projection: startup.projection,
                    loading_model_template: &loading_model_template,
                    frontend_state: &mut frontend_state,
                    name_entry_state: &mut name_entry_state,
                    snapshot: snapshot.as_mut(),
                    capture: capture.as_ref().map(|owner| owner.handle()),
                }),
            });
            drop(linked_present);
            if let Err(error) = result {
                break 'frames Stop::Failed(error);
            }
            frames += 1;
            let after_present = crate::cpu_profile::scope("frame: after present (audio, ambience)");
            // The bed of the world in play, under Vulkan with the Ambience up.
            let playing = session
                .gameplay_screen()
                .map(|screen| screen.serves_in_world_menu());
            let world = session.world().map(|world| world.mission.current_world_a0);
            let light = session
                .world()
                .and_then(|world| crate::world_tint::light(world, session.residency()));
            // The weather's sound: the rain as the sky draws it, and how fast
            // the player's vehicle goes.
            let fixes = session.platform().frontend.settings.fixes();
            let animation_clock = session.animation_value();
            // Rain and lightning are drawn only under the modern sky (Vulkan):
            // without it, none is heard either.
            let drawn = if session.platform().frontend.settings.draws_modern_sky() {
                1.0
            } else {
                0.0
            };
            let weather = session.world().map_or_else(Default::default, |world| {
                crate::ambience::WeatherInputs {
                    lightning: crate::world_scene::lightning_now(world.mission.current_world_a0)
                        * world.weather.storm()
                        * drawn,
                    clock: animation_clock,
                    rain: crate::world_scene::rain_now(
                        world,
                        session.residency(),
                        fixes.prototype_world && fixes.pelagia_rework,
                    ) * drawn,
                    speed: vehicle_speed(world),
                    first_person: false,
                    // As the muffle below takes it: the last world draw's.
                    under_water: playing == Some(false)
                        && world.last_draw.is_some_and(|draw| draw.under_water),
                }
            });
            // The crystals drinking the guns' bolts and lasers (item 26):
            // their flares for the tracer, and the fresh strikes for the
            // ambience to ring.
            let strikes = match session.world().filter(|_| playing.is_some()) {
                Some(world) => {
                    let bolts = FrontendExecutable::parse(session.assets().executable_bytes())
                        .map(|executable| session.process_particles().bolts(executable))
                        .unwrap_or_default();
                    crystal_survey.survey(
                        world,
                        session.residency(),
                        session.furniture_draw(),
                        &bolts,
                        clock.last,
                    )
                }
                None => Vec::new(),
            };
            let seconds = crate::modern_tuning::current()
                .crystals()
                .flare
                .map_or(1.0, |flare| flare.seconds);
            let (struck, flares) = session
                .world_mut()
                .map(|world| {
                    let struck = world.crystal_flares.update(&strikes, clock.last, seconds);
                    (struck, world.crystal_flares.levels())
                })
                .unwrap_or_default();
            let active = playing.is_some() && session.platform().frontend.settings.ambience() > 0;
            ambience.survey(session.world().filter(|_| active), session.residency());
            // Port option: the remastered camera under the water, while the
            // world plays, muffles the game's sound.
            let under_water = playing == Some(false)
                && session
                    .world()
                    .and_then(|world| world.last_draw)
                    .is_some_and(|draw| draw.under_water);
            // Remastered: the Alien Brain's beds play instead of its world's
            // track (the word 00487310 stores, argument + 1), the music's
            // state untouched: hushed from the world's load (its briefing,
            // before the track starts) through play and its in-world menu;
            // its results (phase 8) have it back.
            let brain = session.world().filter(|world| {
                let frontend = &session.platform().frontend;
                frontend.audio.remastered().is_some_and(|pak| {
                    crate::ambience::replaces_music(
                        world.mission.current_world_a0,
                        frontend.settings.ambience(),
                        crate::modern_tuning::current().atmosphere(),
                        |name| pak.contains(name),
                    )
                })
            });
            // The map's screen (phase 7) and the results (phase 8) leave
            // the world's play, where the beds stop: there track 5 plays, as
            // retail's music does through them.
            let hushed = brain
                .filter(|_| {
                    !matches!(
                        driver.state().current_phase(),
                        SessionPhase::Phase7 | SessionPhase::Phase8
                    )
                })
                .map(|world| world.music_native_argument.wrapping_add(1) as u8);
            let brain = brain.map(|world| world.mission.current_world_a0);
            let platform = &mut session.platform_mut().frontend;
            platform.audio.set_under_water(under_water);
            platform.audio.hush_music(hushed);
            let inputs = crate::ambience::AmbienceInputs {
                world: world.filter(|_| playing.is_some() && platform.settings.ambience() > 0),
                music: brain,
                menu: playing == Some(true),
                now: clock.last,
                light,
                weather,
            };
            ambience.update(inputs, &mut platform.audio);
            ambience.strike(&struck, &flares, &mut platform.audio);
            // The frame's share of the mix, rendered here rather than by the
            // device's thread.
            if let (Some(out), FrontendAppClock::FixedStep { micros }) =
                (audio_out.as_mut(), request.clock)
            {
                let audio = &mut session.platform_mut().frontend.audio;
                if let Err(error) = out.frame(micros, audio) {
                    eprintln!("The audio render stopped: {error}");
                    audio_out = None;
                }
            }
            // Options changed while playing take effect at once (user,
            // 2026-10-22), as the frontend's display reset applies them.
            let settings = &session.platform().frontend.settings;
            let options = (settings.filtering(), settings.modern());
            if options != applied_options {
                backend.set_filtering(options.0);
                output.set_filtering(options.0);
                output.set_modern(options.1);
                applied_options = options;
            }
            // The tracer and its upscaler are made again behind the Options
            // after a change, not on a world's first frame.
            backend.prepare_trace();
            drop(after_present);
            crate::cpu_profile::frame_end();
            for (_, path) in request.snapshots_at.iter().filter(|(at, _)| *at == frames) {
                let (pixels, size) = snapshot
                    .as_mut()
                    .expect("requested snapshot buffer")
                    .pixels();
                if presentation.output == dimensions {
                    save_snapshot(size, pixels, path)?;
                } else {
                    let (output_size, output_pixels) = output
                        .export_scaled(texture.as_mut(), pixels, size, presentation.scaling)
                        .map_err(FrontendAppError::Sdl)?;
                    save_snapshot(output_size, &output_pixels, path)?;
                }
            }
            if let Some(status) = status.as_mut() {
                let transitions = driver.transitions();
                for transition in transitions.iter().skip(status.transitions) {
                    eprintln!("[phase] frame {frames}: {transition:?}");
                }
                status.transitions = transitions.len();
                if frames % STATUS_EVERY_FRAMES == 0 {
                    let wall = status.wall.elapsed().as_secs_f64();
                    let rate = (frames - status.frames) as f64 / (wall - status.seconds).max(1e-6);
                    status.frames = frames;
                    status.seconds = wall;
                    let world = session.world().map(|world| {
                        format!(
                            " world {} mission {} elapsed {}s rng {:08X} eye {:?}",
                            world.mission.current_world_a0,
                            world.mission.save.mission_selection,
                            world.mission.save.elapsed_seconds,
                            world.rng,
                            // The game's eye, the view shown's where
                            // the game plays it.
                            world.last_draw.map(|draw| draw.game_eye),
                        )
                    });
                    eprintln!(
                        "[status] frame {frames} clock {}us {:.1} fps phase {:?}{}",
                        clock.last,
                        rate,
                        driver.state().current_phase(),
                        world.unwrap_or_default(),
                    );
                }
            }
            // 004F4F0C: an exit request ends the loop after the iteration.
            if session.platform().frontend.exit_requested
                || request.max_frames.is_some_and(|limit| frames >= limit)
            {
                break 'frames Stop::Finished;
            }
        };
        let state = backend.into_state();
        if let Some(next) = staged.take() {
            // 00437DF0 built a new renderer and surface; the old one retires.
            display::add_scene_stats(&mut scene_stats, state.stats);
            display::add_pixel_stats(&mut pixel_stats, state.pixel_stats);
            frame = next.frame;
            texture = next.texture;
            startup = next.startup;
            dimensions = next.dimensions;
            presentation.output = next.output;
            carry = Carry::Fresh(startup.scene);
        } else {
            carry = Carry::Retained(state);
        }
        if !matches!(stop, Stop::Rebind) {
            break 'epochs stop;
        }
    };
    if let Carry::Retained(state) = carry {
        display::add_scene_stats(&mut scene_stats, state.stats);
        display::add_pixel_stats(&mut pixel_stats, state.pixel_stats);
    }
    let failure = match stop {
        Stop::Failed(error) => Some(error),
        Stop::Closed | Stop::Finished | Stop::Rebind => None,
    };
    let boundary = failure.as_ref().and_then(|error| match error {
        SessionError::Host(error) => error.boundary(),
        _ => None,
    });
    if let Some(error) = failure.filter(|_| boundary.is_none()) {
        return Err(FrontendAppError::Session {
            frame: frames + 1,
            source: error,
        });
    }
    if request.snapshots_at.iter().any(|(at, _)| *at > frames) {
        return Err(FrontendAppError::Request(
            "a --snapshot-at frame is past the run's last presented frame",
        ));
    }
    if let (Some(path), None) = (&request.snapshot, boundary) {
        let (pixels, size) = snapshot
            .as_mut()
            .expect("requested snapshot buffer")
            .pixels();
        if frames == 0 {
            return Err(FrontendAppError::Request(
                "no rendered frame is available for the snapshot",
            ));
        }
        if presentation.output == dimensions {
            save_snapshot(size, pixels, path)?;
        } else {
            // Render the remembered last surface once for export. SDL readback
            // is slow; ordinary frames never read a larger output back to CPU.
            let (output_size, output_pixels) = output
                .export_scaled(texture.as_mut(), pixels, size, presentation.scaling)
                .map_err(FrontendAppError::Sdl)?;
            save_snapshot(output_size, &output_pixels, path)?;
        }
    }
    let control = session
        .frontend_screen()
        .and_then(|screen| screen.control());
    let age_millis = session
        .frontend_screen()
        .map(|screen| screen.screen_input().age_millis())
        .or_else(|| {
            session
                .text_screen()
                .map(|screen| screen.screen_input().age_millis())
        });
    let camera_eye = control.map(|control| control.camera().eye().position);
    let camera_look_at = control.map(|control| control.camera().look_at().position);
    let animation = control.map(|control| control.animation());
    let world_draw = session.world().and_then(|world| world.last_draw);
    let platform = &mut session.platform_mut().frontend;
    let report = FrontendAppReport {
        frames,
        phase: driver.state().current_phase(),
        transitions: driver.transitions().to_vec(),
        name_completions: completions,
        menu_rows: frontend_state.menu.rows,
        glyph_passes: frontend_state.menu.glyph_passes,
        name_entry_draws: name_entry_state.finished_draws,
        exit_requested: platform.exit_requested,
        music: platform.music.state(),
        audio: platform.audio.stats(),
        scene: scene_stats,
        pixels: pixel_stats,
        camera_eye,
        camera_look_at,
        world_draw,
        animation,
        age_millis,
        snapshot: request.snapshot.filter(|_| boundary.is_none()),
        dimensions,
        output_dimensions: output.output_size().map_err(FrontendAppError::Sdl)?,
        display: platform.settings.display_selection(),
        scaling: presentation.scaling,
        renderer: output.backend(),
        display_rebuilds,
        loading: loading_stats,
        fullscreen: output.window().fullscreen_state() != sdl2::video::FullscreenType::Off,
    };
    session
        .shutdown()
        .map_err(|error| FrontendAppError::Teardown(error.to_string()))?;
    {
        let loans = session.split();
        loans.platform.frontend.stop(loans.sounds)?;
        loans.platform.frontend.audio.set_capture(None);
    }
    if let Some(capture) = capture {
        let directory = capture
            .finish()
            .map_err(|error| FrontendAppError::Capture(error.to_string()))?;
        println!("Session recording: {}", directory.display());
    }
    match boundary {
        Some(boundary) => Err(FrontendAppError::SessionBoundary {
            boundary,
            report: Box::new(report),
        }),
        None => Ok(report),
    }
}

/// The loop's 004A4890 samples: realtime milliseconds scaled to microseconds,
/// or a fixed step per frame for reproducible runs. Samples inside a frame
/// (the resource reader's between chunks) then read the frame's own time, as
/// the scenario lab's scripted clock stands still within a retail pass.
struct AppClock {
    policy: FrontendAppClock,
    epoch: Instant,
    fixed_micros: u64,
    started: bool,
    /// `--session-log`: every sample, in order.
    recorder: Option<crate::session_log::SessionRecorder>,
    /// `--replay-session`: the recorded entries, consumed in order.
    replay: Option<ReplayCursor>,
    last: u64,
}

/// `--session-log`'s status lines: each phase change, and a summary every
/// [`STATUS_EVERY_FRAMES`] frames (frame-counted, so a replay's lines line up
/// with the recording's).
struct SessionStatus {
    transitions: usize,
    frames: u64,
    seconds: f64,
    wall: Instant,
}

const STATUS_EVERY_FRAMES: u64 = 600;

struct ReplayCursor {
    entries: std::collections::VecDeque<crate::session_log::ReplayEntry>,
    /// The pad's last recorded reading, repeated each frame as the live
    /// pad is read each frame.
    pad: Option<(u8, u8, crate::platform_input::PadSample)>,
    exhausted: bool,
    /// `--resume`: hand the game to the host when the entries run out.
    resume: bool,
}

/// After a replay runs out, each further sample advances by one exact world
/// tick, so a loading screen still finishes before the loop stops.
const REPLAY_TAIL_STEP_MICROS: u64 = 16_384;

impl AppClock {
    fn new(policy: FrontendAppClock) -> Self {
        Self {
            policy,
            epoch: Instant::now(),
            fixed_micros: 0,
            started: false,
            recorder: None,
            replay: None,
            last: 0,
        }
    }

    fn sample(&mut self) -> u64 {
        self.sample_at(true)
    }

    /// A sample inside the current frame (see [`AppClock`]).
    fn sample_in_frame(&mut self) -> u64 {
        self.sample_at(false)
    }

    fn sample_at(&mut self, frame: bool) -> u64 {
        let value = match self.replay.as_mut() {
            Some(replay) => replay.next_sample().unwrap_or_else(|| {
                replay.exhausted = true;
                self.last.wrapping_add(REPLAY_TAIL_STEP_MICROS)
            }),
            None => self.host_sample(frame),
        };
        self.last = value;
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.sample(value);
        }
        value
    }

    /// The replay's keys recorded before its next frame's sample.
    fn replay_inputs(&mut self) -> Vec<InputNotification> {
        let Some(replay) = self.replay.as_mut() else {
            return Vec::new();
        };
        let mut inputs = Vec::new();
        while let Some(entry) = replay.entries.front() {
            let input = match entry {
                crate::session_log::ReplayEntry::Sample(_) => break,
                crate::session_log::ReplayEntry::Key { name, pressed } => Scancode::from_name(name)
                    .map(|scancode| InputNotification::Key {
                        scancode,
                        pressed: *pressed,
                    }),
                crate::session_log::ReplayEntry::FocusLost => Some(InputNotification::FocusLost),
                crate::session_log::ReplayEntry::Pad {
                    native_type,
                    axes,
                    sample,
                } => {
                    replay.pad = Some((*native_type, *axes, *sample));
                    replay.entries.pop_front();
                    continue;
                }
            };
            if input.is_none() {
                eprintln!("Session replay: unknown key {entry:?} skipped");
            }
            inputs.extend(input);
            replay.entries.pop_front();
        }
        if let Some((native_type, axes, sample)) = replay.pad {
            inputs.push(InputNotification::Pad {
                native_type,
                axes,
                sample,
            });
        }
        inputs
    }

    fn replay_exhausted(&self) -> bool {
        self.replay.as_ref().is_some_and(|replay| replay.exhausted)
    }

    /// `--resume`: an exhausted replay gives way to the host, whose clock
    /// continues from the last sample (rounded up to its millisecond, so it
    /// never steps back). False when the replay was not to be resumed.
    fn resume_live(&mut self) -> bool {
        if !self.replay.as_ref().is_some_and(|replay| replay.resume) {
            return false;
        }
        self.replay = None;
        match self.policy {
            FrontendAppClock::Realtime => {
                let elapsed = std::time::Duration::from_millis(self.last.div_ceil(1000));
                self.epoch = Instant::now()
                    .checked_sub(elapsed)
                    .unwrap_or_else(Instant::now);
            }
            FrontendAppClock::FixedStep { .. } => {
                self.fixed_micros = self.last;
                self.started = true;
            }
        }
        true
    }

    /// Record the frame's keys, before its sample.
    fn record_inputs(&mut self, inputs: &[InputNotification]) {
        let Some(recorder) = self.recorder.as_mut() else {
            return;
        };
        for input in inputs {
            match input {
                InputNotification::Key { scancode, pressed } => {
                    recorder.key(scancode.name(), *pressed)
                }
                InputNotification::FocusLost => recorder.focus_lost(),
                InputNotification::Pad {
                    native_type,
                    axes,
                    sample,
                } => recorder.pad(*native_type, *axes, *sample),
            }
        }
    }

    fn flush_recording(&mut self) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.flush();
        }
    }

    fn host_sample(&mut self, frame: bool) -> u64 {
        match self.policy {
            FrontendAppClock::Realtime => {
                (self.epoch.elapsed().as_millis() as u64).wrapping_mul(1000)
            }
            // The construction sample is zero; every later frame advances.
            FrontendAppClock::FixedStep { micros } => {
                if self.started && frame {
                    self.fixed_micros = self.fixed_micros.wrapping_add(u64::from(micros));
                }
                self.started = true;
                self.fixed_micros
            }
        }
    }
}

impl ReplayCursor {
    /// The next recorded sample. Keys left before it belong to a frame the
    /// replay no longer reaches the same way; they are reported and dropped.
    fn next_sample(&mut self) -> Option<u64> {
        while let Some(entry) = self.entries.pop_front() {
            match entry {
                crate::session_log::ReplayEntry::Sample(value) => return Some(value),
                other => eprintln!(
                    "Session replay: {other:?} arrived outside a frame's input poll; dropped"
                ),
            }
        }
        None
    }
}

/// Native scene/footer/flush phase. Reset and post-presentation clear remain
/// explicit so presentation and sustained camera regression share this owner.
#[cfg(test)]
fn draw_background_frame<'r>(
    scene: &FrontendScene,
    frame: SceneFrame,
    resources: RenderResources<'r>,
    backend: &mut SoftwareScene<'_, 'r>,
) -> Result<(), SceneResourceError<SoftwareSceneError>> {
    let mut host = ResidentSceneHost { resources, backend };
    scene.draw_scene(frame, &mut host)?;
    draw_footer(&mut host)?;
    host.submit(SceneCommand::FlushBatch)
}

fn validate_request(request: &FrontendAppRequest) -> Result<(), FrontendAppError> {
    if request.record_session.is_some()
        && (request.clock != FrontendAppClock::Realtime
            || request.hidden
            || request.dimensions != Some([1024, 768]))
    {
        return Err(FrontendAppError::Request(
            "session recording requires a visible realtime window with --width 1024 --height 768",
        ));
    }
    if request.max_frames == Some(0) {
        return Err(FrontendAppError::Request(
            "maximum frame count must be positive",
        ));
    }
    if request.hidden && request.max_frames.is_none() && request.session_replay.is_none() {
        return Err(FrontendAppError::Request(
            "a hidden window requires a finite frame count or a session replay",
        ));
    }
    if request.audio_out.is_some() && !matches!(request.clock, FrontendAppClock::FixedStep { .. }) {
        return Err(FrontendAppError::Request(
            "an audio render (--audio-out) needs a fixed step",
        ));
    }
    if let FrontendAppClock::FixedStep { micros } = request.clock {
        if micros < MIN_TICK_MICROS as u32 || micros > i32::MAX as u32 || micros % 1000 != 0 {
            return Err(FrontendAppError::Request("fixed step must be a whole number of milliseconds between 8,000 and i32::MAX microseconds"));
        }
    }
    let mut previous = 0;
    for key in &request.input_replay {
        if key.frame == 0 || key.frame < previous || Scancode::from_name(&key.key).is_none() {
            return Err(FrontendAppError::Request(
                "input replay needs ordered positive frame numbers and valid SDL scancode names",
            ));
        }
        previous = key.frame;
    }
    Ok(())
}

/// The display the startup search begins from: the saved one, with an
/// explicit Resolution or tier in place of its size (which Borderless keeps
/// for the next mode).
fn requested_display(
    request: &FrontendAppRequest,
    saved: DisplaySelection,
) -> Result<DisplaySelection, FrontendDisplayError> {
    let size = match (request.dimensions, request.variant) {
        (None, None) => return Ok(saved),
        (None, Some(variant)) => frontend_display_dimensions(u32::from(variant))?,
        (Some(dimensions), variant) => {
            let [smallest_width, smallest_height] = ORIGINAL_SIZES[0];
            if dimensions[0] < smallest_width || dimensions[1] < smallest_height {
                return Err(FrontendDisplayError::Dimensions(dimensions));
            }
            let expected = tier_for(dimensions) as u8;
            if let Some(actual) = variant.filter(|&actual| actual != expected) {
                return Err(FrontendDisplayError::Variant {
                    dimensions,
                    expected,
                    actual,
                });
            }
            dimensions
        }
    };
    Ok(DisplaySelection {
        mode: saved.mode,
        size,
    })
}

fn save_snapshot(size: [u32; 2], pixels: &[u8], path: &Path) -> Result<(), FrontendAppError> {
    let file = File::create(path).map_err(|source| FrontendAppError::Snapshot {
        path: path.to_owned(),
        source,
    })?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), size[0], size[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)?;
    writer.finish()?;
    Ok(())
}

#[cfg(test)]
#[path = "frontend_background_tests.rs"]
mod background_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// Under the fixed step only a frame's sample advances: the resource
    /// reader's samples inside the frame read its time, so a load runs one
    /// loading callback, as retail does under the lab's scripted clock.
    /// `--resume`: the replay's samples, its tail step, then the host's
    /// clock from the last sample on; without it the run would stop there.
    #[test]
    fn a_resumed_replay_hands_the_clock_on_without_stepping_back() {
        let replay = |entries: &[u64], resume| ReplayCursor {
            entries: entries
                .iter()
                .map(|&value| crate::session_log::ReplayEntry::Sample(value))
                .collect(),
            pad: None,
            exhausted: false,
            resume,
        };
        let mut clock = AppClock::new(FrontendAppClock::Realtime);
        clock.replay = Some(replay(&[1_000, 2_500_250], true));
        assert_eq!((clock.sample(), clock.sample()), (1_000, 2_500_250));
        let tail = clock.sample();
        assert_eq!(tail, 2_500_250 + REPLAY_TAIL_STEP_MICROS);
        assert!(clock.replay_exhausted());
        assert!(clock.resume_live());
        assert!(clock.replay.is_none() && !clock.replay_exhausted());
        let live = clock.sample();
        assert!(
            live >= tail && live < tail + 1_000_000,
            "{live} after {tail}"
        );
        let mut clock = AppClock::new(FrontendAppClock::FixedStep { micros: 16_000 });
        clock.replay = Some(replay(&[5_000], true));
        clock.sample();
        clock.sample();
        assert!(clock.resume_live());
        assert_eq!(clock.sample(), 5_000 + REPLAY_TAIL_STEP_MICROS + 16_000);
        let mut clock = AppClock::new(FrontendAppClock::Realtime);
        clock.replay = Some(replay(&[], false));
        clock.sample();
        assert!(clock.replay_exhausted() && !clock.resume_live());
    }

    #[test]
    fn fixed_step_samples_inside_a_frame_keep_its_time() {
        let mut clock = AppClock::new(FrontendAppClock::FixedStep { micros: 125_000 });
        assert_eq!(clock.sample(), 0);
        assert_eq!(clock.sample(), 125_000);
        assert_eq!(clock.sample_in_frame(), 125_000);
        assert_eq!(clock.sample_in_frame(), 125_000);
        assert_eq!(clock.sample(), 250_000);
    }

    #[test]
    fn startup_resolution_uses_saved_setting_unless_explicitly_overridden() {
        use crate::display_selection::DisplayMode;
        let mut request = FrontendAppRequest {
            data_root: PathBuf::new(),
            settings_path: PathBuf::new(),
            profiles_path: PathBuf::new(),
            input_replay: Vec::new(),
            dimensions: None,
            scaling: None,
            vehicle_position_carry: None,
            vehicle_gun_mounts: None,
            hovercraft_fans: None,
            exact_world_ticks: None,
            ultra_ground: None,
            ambience: None,
            headphones: None,
            database_models: None,
            exact_frontend_ticks: None,
            guided_mounts: None,
            completion_bonus: None,
            paced_actors: None,
            splash_voice_cap: None,
            mission_completion: None,
            all_cheats: None,
            prototype_world: None,
            pelagia_rework: None,
            renderer: Some(RendererChoice::Software),
            controller_layout: None,
            variant: None,
            language: None,
            hidden: false,
            max_frames: None,
            snapshot: None,
            snapshots_at: Vec::new(),
            record_session: None,
            audio_out: None,
            mission: None,
            start: None,
            clock: FrontendAppClock::Realtime,
            session_record: None,
            session_replay: None,
        };
        let saved = DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [1920, 1080],
        };
        assert_eq!(requested_display(&request, saved).unwrap(), saved);
        for (variant, expected) in ORIGINAL_SIZES.into_iter().enumerate() {
            request.variant = Some(variant as u8);
            assert_eq!(
                requested_display(&request, saved).unwrap(),
                DisplaySelection {
                    mode: DisplayMode::FullScreen,
                    size: expected
                }
            );
        }
        // An exact size keeps the mode; its tier must agree with a variant.
        request.variant = None;
        request.dimensions = Some([1280, 720]);
        assert_eq!(
            requested_display(&request, saved).unwrap().size,
            [1280, 720]
        );
        request.variant = Some(3);
        assert_eq!(requested_display(&request, saved).unwrap().tier(None), 3);
        request.variant = Some(4);
        assert!(matches!(
            requested_display(&request, saved),
            Err(FrontendDisplayError::Variant {
                expected: 3,
                actual: 4,
                ..
            })
        ));
        request.variant = None;
        request.dimensions = Some([300, 240]);
        assert!(matches!(
            requested_display(&request, saved),
            Err(FrontendDisplayError::Dimensions([300, 240]))
        ));
        // Borderless stays Borderless and keeps the size for the next mode.
        let borderless = DisplaySelection {
            mode: DisplayMode::Borderless,
            size: [1920, 1080],
        };
        request.dimensions = None;
        request.variant = Some(3);
        assert_eq!(
            requested_display(&request, borderless).unwrap(),
            DisplaySelection {
                mode: DisplayMode::Borderless,
                size: [800, 600]
            }
        );
    }
}

/// How fast the first player's vehicle goes, in its velocity units (about
/// 530 at the buggy's full thrust); 0 without one.
fn vehicle_speed(world: &crate::world_backend::WorldSession) -> f32 {
    let speed = || {
        let player = world.arena.first_player()?;
        let state = world.arena.world_player(player).ok()?;
        let handles = world.arena.actor_handles()?;
        let unit = handles
            .peek(state.handle_d8)
            .or_else(|| handles.peek(state.handle_d4))?;
        let velocity = world.arena.actor(unit).ok()?.words_8e_92;
        Some(
            velocity
                .map(|word| f32::from(word as i16))
                .iter()
                .map(|axis| axis * axis)
                .sum::<f32>()
                .sqrt(),
        )
    };
    speed().unwrap_or(0.0)
}
