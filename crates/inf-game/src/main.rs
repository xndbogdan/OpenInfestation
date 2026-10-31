// A desktop start opens the launcher with no console window; a shell's
// console still receives the output (launcher::prepare_console).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod launcher;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use inf_game::frontend_settings::FrontendSettings;
use inf_game::launcher_setup::{
    default_preferences_path, discover_installation, holds_retail_data, validate_installation,
    LauncherPreferences,
};
use inf_game::world_inspection::{
    inspect_world_assets, ConstructorReference, InitialModelReference, ResidentWorldInspection,
};
use inf_game::{
    Installation, ResidencyChange, ResidencyRequest, ResidentRuntime, ResidentSummary,
    ResourceSelection, Runtime, SceneContext, SceneSummary,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Mode {
    Frontend,
    Independent,
    Resident,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Copy the CD music off a disc image (.ccd/.img or .cue/.bin) into
    /// <DATA_ROOT>/cdaudio as lossless WAV, as the launcher's Options do.
    ExtractMusic {
        /// The image's .ccd or .cue, or the .img or .bin beside it.
        #[arg(long)]
        image: PathBuf,
        /// The installation, holding PreLoad.dat and Overlay.
        #[arg(long)]
        data_root: PathBuf,
    },
    /// Install the game files (Infest.exe, PreLoad.dat, Title.avi, Overlay)
    /// and the music off a disc image (.ccd/.img, .cue/.bin or .iso) into
    /// DESTINATION, as the launcher's Install from disc image does.
    Install {
        /// The image's .ccd, .cue or .iso, or the .img or .bin beside one.
        #[arg(long)]
        image: PathBuf,
        /// The folder to install to; made if missing.
        #[arg(long)]
        destination: PathBuf,
    },
}

#[derive(Debug, Parser)]
#[command(about = "Infestation port: frontend menu and resource inspection")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Directory containing the installed PreLoad.dat and Overlay directory.
    /// Without the flag, the executable's own directory wins when it holds
    /// retail data (double-click beside the installed game); then, for a
    /// desktop start, the launcher's remembered installation; otherwise `..`
    /// (`cargo run` from ``). A desktop start opens the
    /// launcher with it.
    #[arg(long)]
    data_root: Option<PathBuf>,

    /// Open the pre-game launcher even when its Options skip it, or with a
    /// direct-run flag.
    #[arg(long, conflicts_with = "no_launcher")]
    launcher: bool,

    /// Start without the launcher, as a direct run: the data root is
    /// --data-root, else the executable's directory when it holds the game,
    /// else `..`.
    #[arg(long)]
    no_launcher: bool,

    /// The launcher's preferences file; defaults to
    /// %LOCALAPPDATA%\Infestation Port\launcher.json.
    #[arg(long, value_name = "PATH")]
    launcher_state: Option<PathBuf>,

    /// Close the launcher as Quit after this many milliseconds (smoke checks).
    #[arg(long, hide = true, value_name = "MS")]
    launcher_smoke_ms: Option<u64>,

    /// Frontend menu, independent OVL preparation, or scene-tree residency.
    /// The default opens the game, like the retail executable.
    #[arg(long, value_enum, default_value_t = Mode::Frontend)]
    mode: Mode,

    /// OVL scene ID; distinct from session phase and campaign mission number.
    #[arg(long)]
    scene: Option<u8>,

    /// In resident mode, request these OVL leaves after boot, in order.
    #[arg(long, value_delimiter = ',')]
    activate: Vec<u8>,

    /// OVL selector: frontend display tier 0..4 overrides the saved
    /// resolution with the tier's original size; resource modes default to
    /// the PreLoad.dat difficulty selector.
    #[arg(long)]
    difficulty: Option<u8>,

    /// Language OVL selector. The game defaults to the saved settings
    /// language (above 5 selects 0); inspection modes to PreLoad.dat.
    #[arg(long)]
    language: Option<u8>,

    /// Inspect one resident Handler-14 global slot and its constructor/model links.
    #[arg(long, value_name = "SLOT")]
    inspect_world: Option<u16>,

    /// Emit the preparation receipt as JSON on stdout.
    #[arg(long)]
    json: bool,

    /// Frontend settings file; defaults to settings.json beside the executable.
    #[arg(long)]
    settings: Option<PathBuf>,

    /// Save file (profiles), shared with retail; defaults to <data-root>/Infest.sav.
    #[arg(long)]
    profiles: Option<PathBuf>,

    /// JSON array of physical key notifications {frame,key,pressed} for deterministic checks.
    #[arg(long)]
    input_replay: Option<PathBuf>,

    /// Override the saved Resolution with an exact size in physical pixels
    /// (requires --height). Sizes other than the original five draw the
    /// largest original layout inside them, enlarged.
    #[arg(long, requires = "height")]
    width: Option<u32>,

    /// Override the saved Resolution (requires --width).
    #[arg(long, requires = "width")]
    height: Option<u32>,

    /// Output scaling: widescreen (the default: a frame at the output's
    /// aspect), four-three (the original frame with black bars), stretched,
    /// or integer. Replaces the saved scaling.
    #[arg(long, value_enum)]
    scaling: Option<inf_game::presentation::ClassicScaling>,

    /// Port fix for a retail bug, on unless saved off: carry the fractions
    /// the vehicle drive's position step drops, so the ACAM does not hang
    /// above the ground at high frame rates. `false` restores retail's step;
    /// either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    vehicle_position_carry: Option<bool>,

    /// Port fix for a retail bug, on unless saved off: join the ACAM's
    /// right-side gun mounts to the right of the hull, so no black band
    /// crosses its roof while it turns into or out of the hovercraft.
    /// `false` restores retail's mounts; either value replaces the saved
    /// setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    vehicle_gun_mounts: Option<bool>,

    /// Port fix for a retail bug, on unless saved off: draw the settled
    /// hovercraft's rear fans the right way out, where retail's mirrored
    /// basis turns them inside out and black. `false` restores retail's
    /// fans; either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    hovercraft_fans: Option<bool>,

    /// Port fix for a retail bug, on unless saved off: step the world in
    /// whole 16.384 ms quanta, carrying the rest of each frame, so gravity,
    /// timers and other `delta >> n` steps keep their rate at high frame
    /// rates. `false` restores retail's per-frame step; either value
    /// replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    exact_world_ticks: Option<bool>,

    /// Port setting, off unless saved on: Ultra ground detail, the ground
    /// smoother than retail's High for play and the traced world alike.
    /// It takes path tracing on Vulkan, offline, on High; either value
    /// replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    ultra_ground: Option<bool>,

    /// Port setting, 10 unless saved otherwise: the Ambience volume, 0 to
    /// 15, of the sound the port adds on Vulkan (`Remastered.pak`); 0 keeps
    /// retail's sound exactly. Either value replaces the saved setting.
    #[arg(long, value_name = "LEVEL", value_parser = clap::value_parser!(u8).range(0..=15))]
    ambience: Option<u8>,

    /// Port setting, off unless saved on: the placed sounds reach each ear
    /// apart, for headphones (Vulkan). Either value replaces the saved
    /// setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    headphones: Option<bool>,

    /// Port fix for a retail bug, on unless saved off: the computer's
    /// database draws an entry's model from a world that loads it when the
    /// current world does not, instead of the entry's text alone. `false`
    /// restores retail's draw; either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    database_models: Option<bool>,

    /// Port fix for a retail bug, on unless saved off: the name entry and the
    /// language carousel step their animations in whole 16.384 ms quanta, so
    /// a flashed glyph still fades at high frame rates. `false` restores
    /// retail's every-frame step; either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    exact_frontend_ticks: Option<bool>,

    /// Port fix, on unless saved off: the guided missiles' mounts fold and
    /// unfold through a change between the ground forms (buggy, truck) and
    /// the others: the roof launcher folds away with the old form and the
    /// side launchers come out with the new, where retail swaps them at
    /// once. Only the drawing changes. `false` restores retail's swap; either
    /// value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    guided_mounts: Option<bool>,

    /// Port fix, on unless saved off: while a save in the list has finished
    /// the campaign, every game gets the completion bonus (the Cranial
    /// Implanter at each respawn), where retail gives it only while its
    /// settings keep the flag finishing set. `false` restores retail's rule;
    /// either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    completion_bonus: Option<bool>,

    /// Port fix, on unless saved off: the scientists, the alien ships and
    /// the creatures update at retail's 30 fps step, where at 60 fps retail's
    /// scientists stand, its ships fly at half speed and its creatures in
    /// contact call twice as often. `false` updates them every world tick,
    /// as retail does; either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    paced_actors: Option<bool>,

    /// Port fix, on unless saved off: of retail's 24 sounds at once, at most
    /// 12 are water splashes. `false` lets splashes take them all, as
    /// retail does; either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    splash_voice_cap: Option<bool>,

    /// Port fix, on unless saved off: a rescue completes (its message and
    /// grant) when the last scientist boards the transport, where retail
    /// waits for the player to come near the transport. `false` keeps
    /// retail's; either value replaces the saved setting. Off in a network
    /// game.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    mission_completion: Option<bool>,

    /// Port option, off unless saved on: the computer's Cheats Menu (Tab,
    /// Cheats Menu) shows all its rows, as if each name cheat that opens one
    /// had been entered, without changing the saved cheat word. `false`
    /// restores retail's rule; either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    all_cheats: Option<bool>,

    /// Port option, on unless saved off: Pelagia, the prototype world. The
    /// Command Centre's dead warpgate wakes for a scientist dropped by it and
    /// leads to world 1, which has a name, a briefing and a way back.
    /// `false` keeps world 1 and the gate retail's; either value replaces
    /// the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    prototype_world: Option<bool>,

    /// Port option, on unless saved off: Pelagia's rework. World 1 gains
    /// its lagoon, egg crystals and tokens, return gate and factory stock.
    /// `false` keeps world 1 retail's, with only the port's briefing and
    /// name; either value replaces the saved setting.
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    pelagia_rework: Option<bool>,

    /// Renderer: auto (OpenGL, falling back to software), opengl, or
    /// software. Software is the CPU pixel reference: identical frames on
    /// every machine, for snapshots and decompilation checks. Replaces the
    /// saved renderer (auto unless the launcher's Options chose another).
    #[arg(long, value_enum)]
    renderer: Option<inf_game::presentation::RendererChoice>,

    /// How a game controller plays: pc (retail's joystick: the left stick
    /// steers, the triggers thrust, its buttons are the key configuration's
    /// Joystick 1 to 16) or console (the PlayStation release's pad layout).
    /// Replaces the saved layout (pc unless the launcher's Options chose
    /// another).
    #[arg(long, value_enum)]
    controller_layout: Option<inf_game::platform_pad::ControllerLayout>,

    /// Stop frontend presentation after this many rendered frames.
    #[arg(long)]
    frames: Option<u64>,

    /// Save the last frontend frame as a PNG.
    #[arg(long)]
    snapshot: Option<PathBuf>,

    /// Save the frame after FRAME (counted as --frames counts) as a PNG at
    /// PATH, as --snapshot does; repeat it for several frames of one run.
    #[arg(long, value_name = "FRAME:PATH", value_parser = parse_snapshot_at)]
    snapshot_at: Vec<(u64, PathBuf)>,
    /// Record actual uncropped 1024x768 presentation and audio for a realtime comparison.
    #[arg(long)]
    record_session: Option<PathBuf>,

    /// Render the mix from the frames into this WAV file (16-bit stereo,
    /// 44.1 kHz), each frame its step's share, so a run with
    /// --fixed-step-micros gives the same sound every time. The device
    /// plays nothing meanwhile.
    #[arg(long, value_name = "FILE")]
    audio_out: Option<PathBuf>,

    /// Keep the frontend window hidden for visual smoke checks, and its
    /// sound silent.
    #[arg(long)]
    hidden: bool,

    /// Use a deterministic frontend clock step instead of elapsed wall time.
    #[arg(long)]
    fixed_step_micros: Option<u32>,

    /// Debug-only new-game mission override (retail always pushes 22).
    /// Selects the OVL world for the offline session; rendering,
    /// briefing and simulation fidelity are unchanged by the flag.
    #[arg(long)]
    mission: Option<u32>,

    /// Debug start harness: skip the menus and the briefing and play one
    /// world with the game state in STATE (JSON: world, start or exact
    /// position and heading, upgrades, known grants, tokens, crystals,
    /// weapons and ammunition, hints, ...), loaded as a saved game would
    /// be. Without STATE: a new game's player at Level 1's first start.
    /// The fields are in the harness_start module's documentation.
    #[arg(long, value_name = "STATE", num_args = 0..=1)]
    start: Option<Option<PathBuf>>,

    /// Write the --start state to FILE as a one-profile Infest.sav (profile
    /// "Lab") and exit, for the retail scenario lab
    /// (runtime_re/lab). Position and heading are not
    /// part of a saved game.
    #[arg(long, value_name = "FILE", requires = "start")]
    write_start_save: Option<PathBuf>,

    /// Write the retail scenario lab's event lines (world ticks and the
    /// player's vehicle updates) to FILE, for its compare.py
    /// (runtime_re/lab).
    #[arg(long, value_name = "FILE")]
    event_log: Option<PathBuf>,

    /// Write a retail recording's first-player controls (turn, pitch,
    /// thrust, fire, grenade) over each world tick's own, from FILE (JSON
    /// lines, one per tick), for the scenario lab's comparisons with TTD
    /// recordings (runtime_re/lab).
    #[arg(long, value_name = "FILE", requires = "start")]
    replay_controls: Option<PathBuf>,

    /// Write a retail run's RNG word over the port's at each world tick's
    /// start, from FILE (JSON lines, one per tick, from the session's first
    /// world tick; a new game's intro is one), for the scenario lab's
    /// lockstep comparisons (runtime_re/lab).
    #[arg(long, value_name = "FILE")]
    replay_rng: Option<PathBuf>,

    /// Log this session for a bug report: everything the game prints goes
    /// to DIR/run-<local time>.log (default <data-root>/logs), with phase
    /// changes, keys, a status line every 600 frames and any crash. Beside
    /// it: run-<time>.replay (the clock and keys, for --replay-session) and
    /// copies of the settings and profiles the session started from.
    #[arg(long, value_name = "DIR", num_args = 0..=1)]
    session_log: Option<Option<PathBuf>>,

    /// Replay a recorded run-<time>.replay: the recorded command line,
    /// clock and keys, from copies of its settings and profiles. This run's
    /// --hidden, --frames, --snapshot, --session-log, --event-log and a
    /// non-auto --renderer still apply. The game ends where the recording does.
    #[arg(long, value_name = "FILE")]
    replay_session: Option<PathBuf>,

    /// With --replay-session: when the recording ends, the game goes on with
    /// the live keys and clock, so a crashed session can be played on from
    /// where it ended. Saves then go to the replay's own copy of the
    /// profiles (run-<time>.replay-profiles.sav beside it).
    #[arg(long, requires = "replay_session")]
    resume: bool,
}

/// The executable's own directory.
fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

/// Resolve `--data-root` for a direct run: an explicit flag always wins;
/// otherwise the executable's own directory is used when it holds retail
/// data (double-click beside the installed game); otherwise `retail` covers
/// `cargo run` from the repository root with the game copied into it.
fn resolve_data_root(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(root) = explicit {
        return root;
    }
    if let Some(dir) = executable_dir().filter(|dir| holds_retail_data(dir)) {
        return dir;
    }
    PathBuf::from("retail")
}

/// A run the launcher stays out of: resource inspection, a portable settings
/// or profiles file, or a harness, capture or replay flag.
fn is_direct_run(args: &Args) -> bool {
    args.mode != Mode::Frontend
        || args.scene.is_some()
        || !args.activate.is_empty()
        || args.inspect_world.is_some()
        || args.json
        || args.settings.is_some()
        || args.profiles.is_some()
        || args.input_replay.is_some()
        || args.frames.is_some()
        || args.snapshot.is_some()
        || !args.snapshot_at.is_empty()
        || args.record_session.is_some()
        || args.hidden
        || args.fixed_step_micros.is_some()
        || args.mission.is_some()
        || args.start.is_some()
        || args.replay_session.is_some()
        || args.event_log.is_some()
}

/// A desktop start: the launcher's policy picks the installation, and
/// errors also show in a message box, as there may be no console.
fn is_desktop_start(args: &Args) -> bool {
    args.launcher || !(args.no_launcher || is_direct_run(args))
}

/// The installation a desktop start plays: the launcher's choice, or the
/// remembered one when Options skip the launcher and it is ready. None after
/// Quit.
fn desktop_installation(args: &Args) -> Result<Option<PathBuf>, String> {
    let preferences_path = args
        .launcher_state
        .clone()
        .unwrap_or_else(default_preferences_path);
    let preferences = LauncherPreferences::load(&preferences_path);
    let root = match &args.data_root {
        Some(root) => root.clone(),
        None => {
            let executable = executable_dir();
            let remembered = preferences.data_root.as_deref();
            let candidates = executable
                .as_deref()
                .into_iter()
                .chain(remembered)
                .chain([Path::new("retail")]);
            discover_installation(candidates)
                .or(remembered)
                .or(executable.as_deref())
                .unwrap_or(Path::new("."))
                .to_path_buf()
        }
    };
    let root = std::path::absolute(&root).unwrap_or(root);
    if skips_launcher(args, &preferences, &root) {
        return Ok(Some(root));
    }
    launcher::run(launcher::LauncherRequest {
        root,
        preferences,
        preferences_path,
        smoke_timeout: args.launcher_smoke_ms.map(Duration::from_millis),
    })
}

/// Options' "skip the launcher" holds while the installation is ready,
/// unless --launcher asks for the window.
fn skips_launcher(args: &Args, preferences: &LauncherPreferences, root: &Path) -> bool {
    preferences.skip_launcher
        && !args.launcher
        && validate_installation(root, &FrontendSettings::default_path()).is_ready()
}

/// The command line a session log records after a desktop start: any
/// `--data-root` replaced by the launcher's installation, so a replay reads
/// the same data.
fn with_data_root(raw: &[String], root: &Path) -> Vec<String> {
    let mut result = Vec::with_capacity(raw.len() + 2);
    let mut arguments = raw.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--data-root" {
            arguments.next();
        } else if !argument.starts_with("--data-root=") {
            result.push(argument.clone());
        }
    }
    result.push("--data-root".to_string());
    result.push(root.display().to_string());
    result
}

/// Report a failure on stderr and, for a desktop start, in a message box.
/// What `--session-log` and `--replay-session` hand the frontend.
#[derive(Default)]
struct SessionRequests {
    record: Option<inf_game::frontend_app::SessionRecordRequest>,
    replay: Option<inf_game::session_log::SessionReplay>,
}

fn run(args: Args, sessions: SessionRequests) -> Result<(), Box<dyn std::error::Error>> {
    if args.mode != Mode::Frontend && args.scaling.is_some() {
        return Err("--scaling requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.vehicle_position_carry.is_some() {
        return Err("--vehicle-position-carry requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.vehicle_gun_mounts.is_some() {
        return Err("--vehicle-gun-mounts requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.hovercraft_fans.is_some() {
        return Err("--hovercraft-fans requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.exact_world_ticks.is_some() {
        return Err("--exact-world-ticks requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.ultra_ground.is_some() {
        return Err("--ultra-ground requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.headphones.is_some() {
        return Err("--headphones requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.ambience.is_some() {
        return Err("--ambience requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.audio_out.is_some() {
        return Err("--audio-out requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.database_models.is_some() {
        return Err("--database-models requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.exact_frontend_ticks.is_some() {
        return Err("--exact-frontend-ticks requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.guided_mounts.is_some() {
        return Err("--guided-mounts requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.completion_bonus.is_some() {
        return Err("--completion-bonus requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.paced_actors.is_some() {
        return Err("--paced-actors requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.splash_voice_cap.is_some() {
        return Err("--splash-voice-cap requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.mission_completion.is_some() {
        return Err("--mission-completion requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.all_cheats.is_some() {
        return Err("--all-cheats requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.prototype_world.is_some() {
        return Err("--prototype-world requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.pelagia_rework.is_some() {
        return Err("--pelagia-rework requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.renderer.is_some() {
        return Err("--renderer requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && args.controller_layout.is_some() {
        return Err("--controller-layout requires frontend mode".into());
    }
    if args.mode != Mode::Frontend && (args.session_log.is_some() || args.replay_session.is_some())
    {
        return Err("--session-log and --replay-session require frontend mode".into());
    }
    match args.mode {
        Mode::Frontend
            if args.scene.is_some()
                || !args.activate.is_empty()
                || args.inspect_world.is_some()
                || args.json =>
        {
            return Err("frontend mode does not accept resource-inspection options".into());
        }
        Mode::Independent if args.inspect_world.is_some() => {
            return Err("--inspect-world requires --mode resident".into());
        }
        Mode::Independent if !args.activate.is_empty() => {
            return Err("--activate requires --mode resident".into());
        }
        Mode::Resident if args.scene.is_some() => {
            return Err("resident mode uses --activate for tree requests; --scene is independent preparation".into());
        }
        _ => {}
    }
    let data_root = resolve_data_root(args.data_root.clone());
    if let Some(out) = &args.write_start_save {
        let start =
            inf_game::harness_start::HarnessStart::load(args.start.clone().flatten().as_deref())?;
        let bytes = std::fs::read(inf_game::retail_path(&data_root, "Infest.exe"))?;
        let grants =
            inf_formats::parse_grant_table(inf_formats::FrontendExecutable::parse(&bytes)?)?;
        let record = start.save_record(b"Lab", &grants)?;
        std::fs::write(out, inf_formats::encode_save_records(&[record])?)?;
        return Ok(());
    }
    let installation = Installation::open(&data_root)?;
    let header = &installation.preload().header;
    if args.mode == Mode::Frontend {
        use inf_game::frontend_app::{run_frontend, FrontendAppClock, FrontendAppRequest};
        eprintln!("Infestation frontend. Arrow keys navigate; Return confirms; Escape opens Back/Quit. Specialized menus and game phases remain under development.");
        let settings_path = args.settings.unwrap_or_else(FrontendSettings::default_path);
        let input_replay = match args.input_replay {
            Some(path) => serde_json::from_reader(std::fs::File::open(path)?)?,
            None => Vec::new(),
        };
        let profiles_path = args
            .profiles
            .unwrap_or_else(|| inf_game::profile_store::ProfileStore::default_path(&data_root));
        let start = args
            .start
            .as_ref()
            .map(|path| inf_game::harness_start::HarnessStart::load(path.as_deref()))
            .transpose()?;
        let report = run_frontend(FrontendAppRequest {
            data_root: data_root.clone(),
            settings_path,
            profiles_path,
            input_replay,
            dimensions: args
                .width
                .zip(args.height)
                .map(|(width, height)| [width, height]),
            scaling: args.scaling,
            vehicle_position_carry: args.vehicle_position_carry,
            vehicle_gun_mounts: args.vehicle_gun_mounts,
            hovercraft_fans: args.hovercraft_fans,
            exact_world_ticks: args.exact_world_ticks,
            ultra_ground: args.ultra_ground,
            ambience: args.ambience,
            headphones: args.headphones,
            database_models: args.database_models,
            exact_frontend_ticks: args.exact_frontend_ticks,
            guided_mounts: args.guided_mounts,
            completion_bonus: args.completion_bonus,
            paced_actors: args.paced_actors,
            splash_voice_cap: args.splash_voice_cap,
            mission_completion: args.mission_completion,
            all_cheats: args.all_cheats,
            prototype_world: args.prototype_world,
            pelagia_rework: args.pelagia_rework,
            renderer: args.renderer,
            controller_layout: args.controller_layout,
            variant: args.difficulty,
            language: args.language,
            hidden: args.hidden,
            max_frames: args.frames,
            snapshot: args.snapshot,
            snapshots_at: args.snapshot_at,
            record_session: args.record_session,
            audio_out: args.audio_out,
            mission: args.mission,
            start,
            session_record: sessions.record,
            session_replay: sessions.replay,
            clock: args
                .fixed_step_micros
                .map_or(FrontendAppClock::Realtime, |micros| {
                    FrontendAppClock::FixedStep { micros }
                }),
        })?;
        println!(
            "Presented {} frames ({} renderer); {} model draws, {} stars, {} footer quads; {} CPU fragments.",
            report.frames,
            report.renderer.name(),
            report.scene.models,
            report.scene.stars,
            report.scene.footers,
            report.pixels.fragments
        );
        println!(
            "Menu: {} rows, {} glyph passes; exit={}. Audio: {:?}; music={:?}",
            report.menu_rows,
            report.glyph_passes,
            report.exit_requested,
            report.audio,
            report.music
        );
        if report.loading.callbacks != 0 {
            println!("Loading: {:?}", report.loading);
        }
        if report.output_dimensions != report.dimensions {
            println!(
                "Classic framebuffer: {}x{} -> {}x{} output, {:?} scaling.",
                report.dimensions[0],
                report.dimensions[1],
                report.output_dimensions[0],
                report.output_dimensions[1],
                report.scaling,
            );
        }
        if let Some(receipt) = report.world_draw {
            println!("Last world draw: {receipt:?}");
        }
        if let Some(snapshot) = report.snapshot {
            println!("Snapshot: {}", snapshot.display());
        }
        return Ok(());
    }
    let selection = ResourceSelection {
        difficulty: args.difficulty.unwrap_or(header.difficulty_flag),
        language: args.language.unwrap_or(header.language_flag),
    };
    if args.mode == Mode::Resident {
        return run_resident(
            installation,
            selection,
            &args.activate,
            args.inspect_world,
            args.json,
        );
    }
    let context = SceneContext::new(
        args.scene.unwrap_or(0),
        selection.difficulty,
        selection.language,
    );
    let mut runtime = Runtime::new(installation);
    let summary = runtime.load_scene(context)?.summary();
    let mut output = io::stdout().lock();
    if args.json {
        serde_json::to_writer_pretty(&mut output, &summary)?;
        writeln!(output)?;
        return Ok(());
    }
    print_independent(&mut output, &summary)?;
    Ok(())
}

fn print_independent(output: &mut impl Write, summary: &SceneSummary) -> io::Result<()> {
    let context = summary.context;
    writeln!(output, "Infestation runtime resource foundation")?;
    writeln!(
        output,
        "Scene {} / difficulty {} / language {}",
        context.level, context.difficulty, context.language
    )?;
    writeln!(
        output,
        "Prepared {} OVLs, {} bytes, {} registrations, {} occupied slots.",
        summary.overlays.len(),
        summary.total_bytes,
        summary.registrations,
        summary.active_slots
    )?;
    for overlay in &summary.overlays {
        writeln!(
            output,
            "  {} ({} bytes)",
            overlay.path.display(),
            overlay.bytes
        )?;
    }
    for handler in summary
        .handlers
        .iter()
        .filter(|entry| entry.registrations > 0)
    {
        writeln!(
            output,
            "  Handler {:2}: {} registrations -> {}/{} occupied slots{}",
            handler.handler,
            handler.registrations,
            handler.active_slots,
            handler.capacity,
            if handler.opaque {
                " (opaque payloads)"
            } else {
                ""
            }
        )?;
    }
    writeln!(
        output,
        "Scope: independent scene resources. Use --mode resident for authored resource lifetime. Gameplay execution is pending."
    )?;
    Ok(())
}

#[derive(Serialize)]
struct ResidentRun {
    summary: ResidentSummary,
    transitions: Vec<ResidencyChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    world_assets: Option<ResidentWorldInspection>,
}

fn run_resident(
    installation: Installation,
    selection: ResourceSelection,
    requests: &[u8],
    world_slot: Option<u16>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = ResidentRuntime::new(installation)?;
    // Selection while inactive changes only selector bytes; boot then loads
    // the authored root, including default branches and zero-request leaves.
    runtime.apply(ResidencyRequest::Select { selection })?;
    let mut transitions = vec![runtime.apply(ResidencyRequest::Boot)?];
    for &level in requests {
        transitions.push(runtime.apply(ResidencyRequest::Activate { level })?);
    }
    let report = ResidentRun {
        summary: runtime.summary(),
        transitions,
        world_assets: world_slot
            .map(|slot| inspect_world_assets(&runtime, slot))
            .transpose()?,
    };
    let mut output = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut output, &report)?;
        writeln!(output)?;
    } else {
        writeln!(output, "Infestation native resource residency")?;
        writeln!(output, "Active leaves: {:?}", report.summary.active_leaves)?;
        writeln!(
            output,
            "Difficulty {} / language {} / {} OVLs / {} bytes / {} occupied slots",
            selection.difficulty,
            selection.language,
            report.summary.overlays.len(),
            report.summary.total_bytes,
            report.summary.active_slots.iter().sum::<usize>()
        )?;
        for transition in &report.transitions {
            writeln!(output, "{:?}", transition.request)?;
            for event in &transition.events {
                writeln!(output, "  {event:?}")?;
            }
        }
        if let Some(assets) = &report.world_assets {
            print_world_assets(&mut output, assets)?;
        }
        writeln!(output, "Scope: native resource lifetime. Screen, actor, and backend callbacks remain unexecuted.")?;
    }
    Ok(())
}

fn print_world_assets(output: &mut impl Write, assets: &ResidentWorldInspection) -> io::Result<()> {
    writeln!(
        output,
        "World H14 {} from {}: {} authored placements",
        assets.source.registration.global_index,
        assets.source.ovl.display(),
        assets.world.placements.len()
    )?;
    for link in &assets.placement_links {
        match &link.constructor {
            ConstructorReference::Resolved(constructor) => {
                write!(
                    output,
                    "  Placement {} -> H13 {} ({}, selector {})",
                    link.placement_index,
                    constructor.source.registration.global_index,
                    constructor.source.ovl.display(),
                    constructor.resource.constructor_selector
                )?;
                match &constructor.initial_model {
                    InitialModelReference::Resolved(model) => writeln!(
                        output,
                        " -> initial H8 {} ({}, {})",
                        model.source.registration.global_index,
                        model.source.ovl.display(),
                        model.name_ascii.as_deref().unwrap_or("unnamed/non-ASCII")
                    )?,
                    status => writeln!(output, " -> initial model {status:?}")?,
                }
            }
            status => writeln!(output, "  Placement {} -> {status:?}", link.placement_index)?,
        }
    }
    writeln!(output, "Scope: authored asset relationships; object initialization and final model selection have not executed. Use --json for complete provenance and decoded fields.")
}

/// `--replay-session`: the recorded command line, with this run's own
/// presentation and logging flags, over working copies of the recorded
/// settings and profiles (the replay may save into them).
fn replay_args(
    current: Args,
    path: &Path,
) -> Result<(Args, inf_game::session_log::SessionReplay), Box<dyn std::error::Error>> {
    use inf_game::session_log::{SessionFiles, SessionReplay};
    let replay = SessionReplay::read(path)?;
    if replay.args.is_empty() {
        return Err("the replay holds no command line".into());
    }
    let mut args = Args::try_parse_from(&replay.args)?;
    let files = SessionFiles::from_replay(path);
    let working = |source: &Path, suffix: &str| -> std::io::Result<Option<PathBuf>> {
        if !source.is_file() {
            return Ok(None);
        }
        let mut name = path.with_extension("").into_os_string();
        name.push(suffix);
        let copy = PathBuf::from(name);
        std::fs::copy(source, &copy)?;
        Ok(Some(copy))
    };
    args.settings = match current.settings {
        Some(settings) => Some(settings),
        None => working(&files.settings, ".replay-settings.json")?.or(args.settings),
    };
    args.profiles = match current.profiles {
        Some(profiles) => Some(profiles),
        None => working(&files.profiles, ".replay-profiles.sav")?.or(args.profiles),
    };
    if current.data_root.is_some() {
        args.data_root = current.data_root;
    }
    if current.renderer.is_some() {
        args.renderer = current.renderer;
    }
    args.hidden = current.hidden;
    args.frames = current.frames;
    args.snapshot = current.snapshot;
    args.snapshot_at = current.snapshot_at;
    args.session_log = current.session_log;
    args.event_log = current.event_log;
    args.input_replay = None;
    args.fixed_step_micros = None;
    args.replay_session = Some(path.to_path_buf());
    args.resume = current.resume;
    Ok((args, replay))
}

/// How many runs `<data-root>/logs/replays` keeps.
const KEPT_REPLAYS: usize = 10;

/// Keep this run's replay among the newest [`KEPT_REPLAYS`] in
/// `<data-root>/logs/replays`: its clock and keys, beside copies of the
/// settings and profiles it starts from, as `--session-log` keeps them.
fn start_rolling_replay(
    args: &Args,
    raw: &[String],
) -> std::io::Result<inf_game::frontend_app::SessionRecordRequest> {
    use inf_game::session_log::{local_stamp, SessionFiles};
    let data_root = resolve_data_root(args.data_root.clone());
    let dir = data_root.join("logs").join("replays");
    std::fs::create_dir_all(&dir)?;
    prune_replays(&dir, KEPT_REPLAYS - 1)?;
    let files = SessionFiles::new(&dir, &local_stamp());
    let settings = args
        .settings
        .clone()
        .unwrap_or_else(FrontendSettings::default_path);
    let profiles = args
        .profiles
        .clone()
        .unwrap_or_else(|| inf_game::profile_store::ProfileStore::default_path(&data_root));
    for (source, copy) in [(&settings, &files.settings), (&profiles, &files.profiles)] {
        if source.is_file() {
            std::fs::copy(source, copy)?;
        }
    }
    Ok(inf_game::frontend_app::SessionRecordRequest {
        path: files.replay,
        args: raw.to_vec(),
    })
}

/// Remove every file of all but the newest `keep` runs (`run-<time>.replay`
/// and what shares its stem) from `dir`.
fn prune_replays(dir: &Path, keep: usize) -> std::io::Result<()> {
    let names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    let mut stems: Vec<&str> = names
        .iter()
        .filter_map(|name| name.strip_suffix(".replay"))
        .collect();
    stems.sort_unstable();
    let excess = stems.len().saturating_sub(keep);
    for stem in &stems[..excess] {
        for name in names.iter().filter(|name| name.starts_with(stem)) {
            std::fs::remove_file(dir.join(name))?;
        }
    }
    Ok(())
}

/// Start `--session-log`: the log, copies of the starting settings and
/// profiles, and the recording request.
fn start_session_log(
    args: &Args,
    raw: &[String],
    dir: Option<&Path>,
) -> Result<
    (
        inf_game::session_log::SessionLog,
        inf_game::frontend_app::SessionRecordRequest,
    ),
    Box<dyn std::error::Error>,
> {
    use inf_game::session_log::{local_stamp, SessionFiles, SessionLog};
    let data_root = resolve_data_root(args.data_root.clone());
    let dir = dir.map_or_else(|| data_root.join("logs"), Path::to_path_buf);
    std::fs::create_dir_all(&dir)?;
    let stamp = local_stamp();
    let files = SessionFiles::new(&dir, &stamp);
    let settings = args
        .settings
        .clone()
        .unwrap_or_else(FrontendSettings::default_path);
    let profiles = args
        .profiles
        .clone()
        .unwrap_or_else(|| inf_game::profile_store::ProfileStore::default_path(&data_root));
    let copied = |source: &Path, copy: &Path| match std::fs::copy(source, copy) {
        Ok(_) => format!("{} (copied to {})", source.display(), copy.display()),
        Err(error) => format!("{} (not copied: {error})", source.display()),
    };
    let header = vec![
        "Infestation port session log".to_string(),
        format!("Started: {stamp} (local)"),
        executable_line(),
        format!("Command line: {}", serde_json::to_string(raw)?),
        format!("Data root: {}", data_root.display()),
        format!("Settings: {}", copied(&settings, &files.settings)),
        format!("Profiles: {}", copied(&profiles, &files.profiles)),
        format!(
            "Replay: {}{}",
            files.replay.display(),
            args.replay_session
                .as_ref()
                .map(|source| format!(" (this run replays {})", source.display()))
                .unwrap_or_default()
        ),
        platform_line(),
    ];
    let log = SessionLog::start(&files.log, &header)?;
    let record = inf_game::frontend_app::SessionRecordRequest {
        path: files.replay,
        args: raw.to_vec(),
    };
    Ok((log, record))
}

/// The executable with its size and time, and the port's version.
fn executable_line() -> String {
    let exe = std::env::current_exe().ok();
    let build = exe
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|meta| {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |time| time.as_secs());
            format!("{} bytes, modified unix {modified}", meta.len())
        })
        .unwrap_or_default();
    format!(
        "Executable: {} ({build}), inf-game {}",
        exe.map(|path| path.display().to_string())
            .unwrap_or_default(),
        env!("CARGO_PKG_VERSION")
    )
}

fn platform_line() -> String {
    format!(
        "Platform: {} {}, {} logical CPUs",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, |count| count.get())
    )
}

/// `--snapshot-at FRAME:PATH`.
fn parse_snapshot_at(text: &str) -> Result<(u64, PathBuf), String> {
    let (frame, path) = text
        .split_once(':')
        .ok_or_else(|| format!("{text:?} is not FRAME:PATH"))?;
    let frame = frame
        .parse::<u64>()
        .map_err(|error| format!("{frame:?} is not a frame: {error}"))?;
    if frame == 0 || path.is_empty() {
        return Err(format!("{text:?} needs a frame from 1 and a path"));
    }
    Ok((frame, PathBuf::from(path)))
}

/// Where a crash log goes (beside the session log, otherwise in
/// `<data-root>/logs`) and the run it describes.
fn crash_context(
    args: &Args,
    raw: &[String],
    session_log: Option<&Path>,
) -> inf_game::crash_log::CrashContext {
    let data_root = resolve_data_root(args.data_root.clone());
    let dir = session_log
        .and_then(Path::parent)
        .map_or_else(|| data_root.join("logs"), Path::to_path_buf);
    inf_game::crash_log::CrashContext {
        path: dir.join(inf_game::crash_log::FILE_NAME),
        run: vec![
            executable_line(),
            format!(
                "Command line: {}",
                serde_json::to_string(raw).unwrap_or_default()
            ),
            format!("Data root: {}", data_root.display()),
            platform_line(),
            match session_log {
                Some(path) => format!("Session log: {}", path.display()),
                None => {
                    "Session log: none (start with --session-log to keep the whole run)".to_string()
                }
            },
        ],
    }
}

/// A failure's message, and for a desktop start where its crash log is.
fn crash_message(message: &str, crash: Option<&Path>) -> String {
    match crash {
        Some(path) => format!("{message}\n\nA crash log was saved to:\n{}", path.display()),
        None => format!("{message}\n\nStart it with --session-log to keep a log for a bug report."),
    }
}

/// A failure before the game runs: the console, and for a desktop start the
/// crash log and a message box naming it.
fn report_crash(desktop: bool, message: &str) {
    eprintln!("{message}");
    if desktop {
        let crash = inf_game::crash_log::write(&[message.to_string()]);
        if let Some(path) = &crash {
            eprintln!("Crash log written: {}", path.display());
        }
        launcher::show_error(&crash_message(message, crash.as_deref()));
    }
}

fn install(image: &Path, destination: &Path) -> ExitCode {
    let installed = inf_game::disc_image::DiscImage::open(image).and_then(|disc| {
        disc.install(destination, &FrontendSettings::default_path(), |line| {
            println!("{line}")
        })
    });
    match installed {
        Ok(result) => {
            println!("{}\n\n{}", result.summary(), result.report.description());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Install failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn extract_music(image: &Path, data_root: &Path) -> ExitCode {
    let extracted = inf_game::disc_image::DiscImage::open(image).and_then(|disc| {
        disc.extract_music(data_root, |track| {
            println!("Extracting track {track:02}...")
        })
    });
    match extracted {
        Ok(result) => {
            println!("{}", result.summary());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Extract music failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    launcher::prepare_console();
    let mut raw: Vec<String> = std::env::args().collect();
    let mut args = Args::parse();
    match args.command.take() {
        Some(Command::ExtractMusic { image, data_root }) => {
            return extract_music(&image, &data_root)
        }
        Some(Command::Install { image, destination }) => return install(&image, &destination),
        None => {}
    }
    // Any failure from here on leaves a crash log (infest-crash-log-<time>.txt).
    inf_game::crash_log::install(crash_context(&args, &raw, None));
    inf_game::crash_log::set_notifier(launcher::show_error);
    let desktop = is_desktop_start(&args);
    if desktop {
        // Before the launcher's first window.
        launcher::declare_dpi_awareness();
        match desktop_installation(&args) {
            Ok(Some(root)) => {
                raw = with_data_root(&raw, &root);
                args.data_root = Some(root);
                inf_game::crash_log::install(crash_context(&args, &raw, None));
            }
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                report_crash(
                    desktop,
                    &format!("The Infestation launcher failed: {error}"),
                );
                return ExitCode::FAILURE;
            }
        }
    }
    let mut sessions = SessionRequests::default();
    if let Some(path) = args.replay_session.clone() {
        match replay_args(args, &path) {
            Ok((replayed, mut replay)) => {
                replay.resume = replayed.resume;
                args = replayed;
                sessions.replay = Some(replay);
            }
            Err(error) => {
                report_crash(
                    desktop,
                    &format!(
                        "Infestation bootstrap failed: --replay-session {}: {error}",
                        path.display()
                    ),
                );
                return ExitCode::FAILURE;
            }
        }
    }
    let log = match args.session_log.clone() {
        Some(dir) => match start_session_log(&args, &raw, dir.as_deref()) {
            Ok((log, record)) => {
                sessions.record = Some(record);
                Some(log)
            }
            Err(error) => {
                report_crash(
                    desktop,
                    &format!("Infestation bootstrap failed: --session-log: {error}"),
                );
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    // A desktop start, or a resumed replay, keeps its own replay among the
    // newest under <data-root>/logs/replays, which its crash log names.
    if log.is_none() && (desktop || args.resume) {
        match start_rolling_replay(&args, &raw) {
            Ok(record) => {
                let mut context = crash_context(&args, &raw, None);
                context.run.push(format!(
                    "Replay: {} (inf-game --replay-session FILE reruns it; add --resume to play on \
                     from where it ends)",
                    record.path.display()
                ));
                inf_game::crash_log::install(context);
                sessions.record = Some(record);
            }
            Err(error) => eprintln!("This run keeps no replay: {error}"),
        }
    }
    if let Some(path) = &args.replay_controls {
        if let Err(error) = inf_game::replay_controls::load(path) {
            report_crash(
                desktop,
                &format!(
                    "Infestation bootstrap failed: --replay-controls {}: {error}",
                    path.display()
                ),
            );
            return ExitCode::FAILURE;
        }
    }
    if let Some(path) = &args.replay_rng {
        if let Err(error) = inf_game::replay_controls::load_rng(path) {
            report_crash(
                desktop,
                &format!(
                    "Infestation bootstrap failed: --replay-rng {}: {error}",
                    path.display()
                ),
            );
            return ExitCode::FAILURE;
        }
    }
    if let Some(path) = &args.event_log {
        if let Err(error) = inf_game::event_log::open(path, &raw) {
            report_crash(
                desktop,
                &format!(
                    "Infestation bootstrap failed: --event-log {}: {error}",
                    path.display()
                ),
            );
            return ExitCode::FAILURE;
        }
    }
    // Without a session log the output still passes through a capture, so
    // a crash log can end with the last lines the game printed.
    let tail = match &log {
        Some(log) => {
            eprintln!("Session log: {}", log.path().display());
            inf_game::crash_log::install(crash_context(&args, &raw, Some(log.path())));
            None
        }
        None => inf_game::session_log::OutputTail::start().ok(),
    };
    let detailed = log.is_some();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(args, sessions)));
    let failure = match result {
        Ok(Ok(())) => None,
        Ok(Err(error)) => {
            let message = format!("Infestation bootstrap failed: {error}");
            eprintln!("{message}");
            let mut failure = vec![message.clone()];
            let mut source = error.source();
            while let Some(cause) = source {
                failure.push(format!("  caused by: {cause}"));
                source = cause.source();
            }
            failure.push(format!("Error detail: {error:?}"));
            if detailed {
                for line in &failure[1..] {
                    eprintln!("{line}");
                }
            }
            // A direct run's error stays on its console; a desktop start's
            // or a logged session's also leaves a crash log.
            Some((message, failure, desktop || detailed))
        }
        Err(_) => {
            // The panic hook has printed and logged it (PANIC above).
            let message = "Infestation stopped: the game panicked.".to_string();
            eprintln!("Infestation stopped: the game panicked (see PANIC above)");
            let failure =
                inf_game::crash_log::last_panic().unwrap_or_else(|| vec![message.clone()]);
            Some((message, failure, true))
        }
    };
    // Drain the captured output, so a crash log ends with its last lines.
    drop(tail);
    if let Some(log) = log {
        let path = log.path().to_path_buf();
        drop(log);
        eprintln!("Session log written: {}", path.display());
    }
    let Some((message, failure, logged)) = failure else {
        return ExitCode::SUCCESS;
    };
    let crash = logged
        .then(|| inf_game::crash_log::write(&failure))
        .flatten();
    if let Some(path) = &crash {
        eprintln!("Crash log written: {}", path.display());
    }
    if desktop {
        launcher::show_error(&crash_message(&message, crash.as_deref()));
    }
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_dimensions_distinguish_defaults_from_explicit_overrides() {
        let saved = Args::try_parse_from(["inf-game", "--mode", "frontend"]).unwrap();
        assert_eq!((saved.width, saved.height), (None, None));
        assert_eq!(saved.difficulty, None);
        let explicit = Args::try_parse_from([
            "inf-game", "--mode", "frontend", "--width", "640", "--height", "480",
        ])
        .unwrap();
        assert_eq!((explicit.width, explicit.height), (Some(640), Some(480)));
        assert!(
            Args::try_parse_from(["inf-game", "--mode", "frontend", "--width", "800"]).is_err()
        );
        assert!(
            Args::try_parse_from(["inf-game", "--mode", "frontend", "--height", "600"]).is_err()
        );
        let headless = Args::try_parse_from(["inf-game"]).unwrap();
        assert_eq!(headless.mode, Mode::Frontend);
        assert_eq!(headless.data_root, None);
        assert_eq!((headless.difficulty, headless.language), (None, None));
    }

    /// The panic hook writes the crash log at once, so a panic off the main
    /// thread (which main never sees) still leaves one.
    #[test]
    fn a_panic_on_any_thread_leaves_a_crash_log() {
        use inf_game::crash_log;
        let dir = std::env::temp_dir().join(format!("inf-game-crash-{}", std::process::id()));
        let path = dir.join(crash_log::FILE_NAME);
        crash_log::install(crash_log::CrashContext {
            path: path.clone(),
            run: vec!["Command line: [\"inf-game\"]".to_string()],
        });
        crash_log::remember("[    0:01.000] the line before".to_string());
        let worker = std::thread::Builder::new()
            .name("worker".to_string())
            .spawn(|| panic!("the worker gave up"))
            .unwrap();
        assert!(worker.join().is_err());
        // The log takes its time in its name, beside the base name.
        let logs: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(logs.len(), 1, "{logs:?}");
        let name = logs[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("infest-crash-log-") && name.ends_with(".txt"),
            "{name}"
        );
        let text = std::fs::read_to_string(&logs[0]).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(text.contains("  The game panicked on thread 'worker'.\n"));
        assert!(text.contains("the worker gave up"));
        assert!(text.contains("  Backtrace:\n"));
        assert!(text.contains("  Command line: [\"inf-game\"]\n"));
        assert!(text.contains("  [    0:01.000] the line before\n"));
        assert_eq!(
            crash_log::last_panic().unwrap()[0],
            "The game panicked on thread 'worker'."
        );
    }

    #[test]
    fn resume_needs_a_replay_session() {
        assert!(Args::try_parse_from(["inf-game", "--resume"]).is_err());
        let args = Args::try_parse_from(["inf-game", "--replay-session", "run.replay", "--resume"])
            .unwrap();
        assert!(args.resume);
    }

    #[test]
    fn the_replays_folder_keeps_the_newest_runs() {
        let dir = std::env::temp_dir().join(format!("inf-replays-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for second in 0..12 {
            for suffix in [".replay", ".settings.json", ".profiles.sav"] {
                std::fs::write(
                    dir.join(format!("run-20261018-0000{second:02}{suffix}")),
                    b"",
                )
                .unwrap();
            }
        }
        std::fs::write(dir.join("notes.txt"), b"").unwrap();
        prune_replays(&dir, 9).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(left.len(), 9 * 3 + 1);
        assert_eq!(left[0], "notes.txt");
        assert!(left[1].starts_with("run-20261018-000003"));
    }

    #[test]
    fn a_crash_log_sits_beside_the_session_log_or_in_the_data_roots_logs() {
        let args = Args::try_parse_from(["inf-game", "--data-root", "gamedir"]).unwrap();
        let raw = vec!["inf-game".to_string()];
        let plain = crash_context(&args, &raw, None);
        assert_eq!(
            plain.path,
            Path::new("gamedir")
                .join("logs")
                .join("infest-crash-log.txt")
        );
        assert!(plain
            .run
            .iter()
            .any(|line| line.starts_with("Session log: none")));
        let logged = crash_context(&args, &raw, Some(Path::new("elsewhere/run-1.log")));
        assert_eq!(
            logged.path,
            Path::new("elsewhere").join("infest-crash-log.txt")
        );
        assert!(logged.run.contains(&format!(
            "Session log: {}",
            Path::new("elsewhere/run-1.log").display()
        )));
    }

    #[test]
    fn session_log_takes_an_optional_directory() {
        let bare = Args::try_parse_from(["inf-game", "--session-log"]).unwrap();
        assert_eq!(bare.session_log, Some(None));
        let named = Args::try_parse_from(["inf-game", "--session-log", "logs"]).unwrap();
        assert_eq!(named.session_log, Some(Some(PathBuf::from("logs"))));
        let followed =
            Args::try_parse_from(["inf-game", "--session-log", "--renderer", "software"]).unwrap();
        assert_eq!(followed.session_log, Some(None));
        let absent = Args::try_parse_from(["inf-game"]).unwrap();
        assert_eq!(absent.session_log, None);
    }

    /// The frame ends at the first colon, so a Windows path keeps its drive.
    #[test]
    fn snapshot_at_takes_repeated_frame_and_path_pairs() {
        let args = Args::try_parse_from([
            "inf-game",
            "--snapshot-at",
            r"12:D:\shots\a.png",
            "--snapshot-at",
            "30:b.png",
        ])
        .unwrap();
        assert_eq!(
            args.snapshot_at,
            [
                (12, PathBuf::from(r"D:\shots\a.png")),
                (30, PathBuf::from("b.png"))
            ]
        );
        for refused in ["a.png", "0:a.png", "x:a.png", "12:"] {
            assert!(
                Args::try_parse_from(["inf-game", "--snapshot-at", refused]).is_err(),
                "{refused}"
            );
        }
    }

    #[test]
    fn explicit_data_root_wins_over_executable_directory() {
        let explicit = Args::try_parse_from(["inf-game", "--data-root", "gamedir"]).unwrap();
        assert_eq!(
            resolve_data_root(explicit.data_root),
            PathBuf::from("gamedir")
        );
    }

    #[test]
    fn the_launcher_opens_for_desktop_starts_and_stays_out_of_direct_runs() {
        let desktop = |arguments: &[&str]| {
            let args = Args::try_parse_from([&["inf-game"], arguments].concat()).unwrap();
            is_desktop_start(&args)
        };
        // A double-click, a data root, presentation choices and a bug-report
        // log all start from the launcher.
        assert!(desktop(&[]));
        assert!(desktop(&["--data-root", "gamedir"]));
        assert!(desktop(&["--renderer", "opengl", "--scaling", "stretched"]));
        assert!(desktop(&["--width", "1280", "--height", "720"]));
        assert!(desktop(&["--guided-mounts", "false", "--session-log"]));
        // Inspection, harness, capture and replay runs never wait on it.
        for direct in [
            &["--mode", "resident"][..],
            &["--activate", "47"],
            &["--inspect-world", "0"],
            &["--json"],
            &["--scene", "11"],
            &["--settings", "s.json"],
            &["--profiles", "p.sav"],
            &["--input-replay", "keys.json"],
            &["--frames", "10"],
            &["--snapshot", "frame.png"],
            &["--record-session", "capture"],
            &["--hidden"],
            &["--fixed-step-micros", "16000"],
            &["--mission", "18"],
            &["--start"],
            &["--replay-session", "run.replay"],
            &["--event-log", "events.jsonl"],
            &["--no-launcher"],
        ] {
            assert!(!desktop(direct), "{direct:?}");
        }
        // --launcher opens it whatever else is given.
        assert!(desktop(&["--launcher", "--frames", "10"]));
        assert!(Args::try_parse_from(["inf-game", "--launcher", "--no-launcher"]).is_err());
    }

    #[test]
    fn a_saved_skip_starts_a_ready_installation_unless_launcher_asks() {
        let retail = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let skip = LauncherPreferences {
            data_root: None,
            skip_launcher: true,
        };
        let plain = Args::try_parse_from(["inf-game"]).unwrap();
        let forced = Args::try_parse_from(["inf-game", "--launcher"]).unwrap();
        assert!(skips_launcher(&plain, &skip, &retail));
        assert!(!skips_launcher(&forced, &skip, &retail));
        assert!(!skips_launcher(
            &plain,
            &LauncherPreferences::default(),
            &retail
        ));
        // Missing files open the launcher whatever Options chose.
        let empty = std::env::temp_dir().join(format!("inf-skip-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        assert!(!skips_launcher(&plain, &skip, &empty));
        std::fs::remove_dir_all(empty).unwrap();
    }

    #[test]
    fn a_desktop_start_records_the_launchers_installation() {
        let root = Path::new("games/Infestation");
        let raw = |arguments: &[&str]| -> Vec<String> {
            [&["inf-game"], arguments]
                .concat()
                .into_iter()
                .map(String::from)
                .collect()
        };
        let recorded = with_data_root(&raw(&["--session-log"]), root);
        assert_eq!(
            recorded,
            raw(&["--session-log", "--data-root", "games/Infestation"])
        );
        for explicit in [
            &["--data-root", "old", "--session-log"][..],
            &["--data-root=old", "--session-log"],
        ] {
            assert_eq!(with_data_root(&raw(explicit), root), recorded);
        }
        let replayed = Args::try_parse_from(&recorded).unwrap();
        assert_eq!(replayed.data_root.as_deref(), Some(root));
    }

    #[test]
    fn the_renderer_flag_is_an_override() {
        assert_eq!(Args::try_parse_from(["inf-game"]).unwrap().renderer, None);
        assert_eq!(
            Args::try_parse_from(["inf-game", "--renderer", "opengl"])
                .unwrap()
                .renderer,
            Some(inf_game::presentation::RendererChoice::OpenGl)
        );
    }
}
