//! Port-only crash log. When the game stops on a failure (a panic on any
//! thread, an error that ends the run, or on Windows an unhandled native
//! exception such as a driver's access violation), it writes
//! `infest-crash-log-<time>.txt` (local time, `20261017-193715`) in its log
//! folder: `--session-log`'s, otherwise `<data-root>/logs`. Every crash keeps
//! its own file; a second one within the same second adds `-2`, and the
//! panic hook's report, written again once the output is drained, keeps
//! the file it first took.
//!
//! It also writes one when the game gets stuck without crashing: when a
//! frame has not finished after [`STALL`] (a watchdog thread notices, and
//! the log names the step and the object the frame was on, from the
//! breadcrumbs the main loop leaves with [`step`]), and when the player's
//! own update stops, which leaves the game running with nothing to drive.
//!
//! The log says what failed (with the backtrace), describes the run (time,
//! executable, command line, data root, platform, session log), then the
//! game as the session last noted it (phase, world, the player's unit and
//! its weapon rows), and ends with the last lines the game printed. Those lines come from the output capture
//! in [`crate::session_log`], which runs whether or not a session log is
//! written.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant};

/// The crash log's name in the log folder: each log adds its time before
/// the extension (`infest-crash-log-20261017-193715.txt`).
pub const FILE_NAME: &str = "infest-crash-log.txt";
/// How many of the last printed lines the log keeps.
const RECENT_LINES: usize = 300;

static RECENT: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static CONTEXT: Mutex<Option<CrashContext>> = Mutex::new(None);
static LAST_PANIC: Mutex<Option<Vec<String>>> = Mutex::new(None);
/// The last report written and its file, which the same report reuses.
static LAST_WRITTEN: Mutex<Option<(Vec<String>, PathBuf)>> = Mutex::new(None);
static GAME: Mutex<Option<GameState>> = Mutex::new(None);
static WORLD_NAME: Mutex<Option<(u32, String)>> = Mutex::new(None);

/// What the game was doing, which the session notes after each world tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameState {
    /// The session phase: 4 gameplay, 6 paused, and so on.
    pub phase: u8,
    /// The world's selection (mission `+A0`).
    pub world: Option<u32>,
    /// The mission's elapsed seconds (`+34`).
    pub elapsed_seconds: u32,
    /// The first player's unit.
    pub unit: Option<UnitState>,
}

/// The first player's unit when the session last noted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitState {
    pub position: Option<[u16; 3]>,
    pub health: u16,
    pub maximum_health: Option<u16>,
    /// A vehicle's form (`+1E8`) and its `+7C` behavior's primary (`+10`)
    /// and secondary (`+11`) weapon rows.
    pub vehicle: Option<(u32, u8, u8)>,
}

/// The session's game state, for the next crash log.
pub fn note_game(state: GameState) {
    if let Ok(mut game) = GAME.lock() {
        *game = Some(state);
    }
}

/// The current world's name, noted once per world.
pub fn note_world_name(world: u32, name: String) {
    if let Ok(mut slot) = WORLD_NAME.lock() {
        *slot = Some((world, name));
    }
}

/// Whether the world's name is already noted.
pub fn world_name_noted(world: u32) -> bool {
    WORLD_NAME
        .lock()
        .is_ok_and(|slot| slot.as_ref().is_some_and(|(noted, _)| *noted == world))
}
/// How a native crash tells the player where its log is (the launcher's
/// message box); the process ends right after.
static NOTIFY: OnceLock<fn(&str)> = OnceLock::new();

/// How long a frame may run before the watchdog calls the game stuck.
pub const STALL: Duration = Duration::from_secs(15);

/// What the main loop was doing: the breadcrumb a stall's log names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Step {
    /// Input and window events, between frames.
    Events = 1,
    /// Loading a world.
    Loading,
    /// The world tick, outside its actors.
    WorldTick,
    /// One actor's update.
    ActorUpdate,
    /// The world's contact passes.
    WorldContact,
    /// The furniture effects.
    Furniture,
    /// The world draw, outside its objects.
    WorldDraw,
    /// One object's draw.
    ObjectDraw,
}

impl Step {
    fn name(code: u8) -> &'static str {
        match code {
            1 => "the input and window events",
            2 => "loading a world",
            3 => "the world tick",
            4 => "an actor's update",
            5 => "the world's contact passes",
            6 => "the furniture effects",
            7 => "the world draw",
            8 => "an object's draw",
            _ => "starting up",
        }
    }
}

static STEP: AtomicU8 = AtomicU8::new(0);
/// The step's object slot plus one; zero for none.
static STEP_OBJECT: AtomicUsize = AtomicUsize::new(0);
/// When the step began, in milliseconds since [`START`].
static STEP_AT: AtomicU64 = AtomicU64::new(0);
/// The frame the main loop began last, and when (milliseconds plus one;
/// zero before the first).
static FRAME: AtomicU64 = AtomicU64::new(0);
static FRAME_AT: AtomicU64 = AtomicU64::new(0);
/// The frame whose stall has been logged.
static STALLED: AtomicU64 = AtomicU64::new(u64::MAX);
static START: OnceLock<Instant> = OnceLock::new();

fn now() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Leave a breadcrumb: the main loop is now in `step`.
pub fn step(step: Step) {
    STEP.store(step as u8, Ordering::Relaxed);
    STEP_OBJECT.store(0, Ordering::Relaxed);
    STEP_AT.store(now(), Ordering::Relaxed);
}

/// Leave a breadcrumb naming the object (its arena slot) the step is on.
pub fn step_object(step: Step, slot: usize) {
    STEP.store(step as u8, Ordering::Relaxed);
    STEP_OBJECT.store(slot.wrapping_add(1), Ordering::Relaxed);
    STEP_AT.store(now(), Ordering::Relaxed);
}

/// The main loop begins `frame`. The frame after a logged stall says how
/// long that one took.
pub fn heartbeat(frame: u64) {
    let at = now();
    let previous = FRAME_AT.swap(at + 1, Ordering::Relaxed);
    let last = FRAME.swap(frame, Ordering::Relaxed);
    if previous != 0 && STALLED.load(Ordering::Relaxed) == last {
        eprintln!(
            "The game is responding again: frame {last} took {:.1} s.",
            at.saturating_sub(previous - 1) as f64 / 1000.0
        );
        STALLED.store(u64::MAX, Ordering::Relaxed);
    }
    step(Step::Events);
}

/// The one-based frame the main loop is on (zero before the first), as
/// `--input-replay` numbers frames.
pub fn frame() -> u64 {
    FRAME.load(Ordering::Relaxed)
}

/// What a stall's log says: the frame, how long it has run, and the step.
fn stall_report(frame: u64, frame_ms: u64, step: u8, object: usize, step_ms: u64) -> Vec<String> {
    let mut doing = format!(
        "It has been in {} for {:.1} s",
        Step::name(step),
        step_ms as f64 / 1000.0
    );
    if object != 0 {
        let _ = write!(doing, " (object slot {})", object - 1);
    }
    doing.push('.');
    vec![
        format!(
            "The game stopped responding: frame {frame} has not finished after {:.1} s.",
            frame_ms as f64 / 1000.0
        ),
        doing,
        "The game was still running when this was written; if it recovers, it carries on."
            .to_string(),
    ]
}

/// The watchdog: once a second, a frame running past [`STALL`] gets one log.
fn watch() {
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let began = FRAME_AT.load(Ordering::Relaxed);
        if began == 0 {
            continue;
        }
        let now = now();
        let frame_ms = now.saturating_sub(began - 1);
        let frame = FRAME.load(Ordering::Relaxed);
        if frame_ms < STALL.as_millis() as u64 || STALLED.load(Ordering::Relaxed) == frame {
            continue;
        }
        STALLED.store(frame, Ordering::Relaxed);
        let report = stall_report(
            frame,
            frame_ms,
            STEP.load(Ordering::Relaxed),
            STEP_OBJECT.load(Ordering::Relaxed),
            now.saturating_sub(STEP_AT.load(Ordering::Relaxed)),
        );
        for line in &report {
            eprintln!("{line}");
        }
        if let Some(path) = write(&report) {
            let message = format!(
                "Infestation has stopped responding.\n\nA crash log was saved to:\n{}",
                path.display()
            );
            eprintln!("{message}");
            if let Some(notify) = NOTIFY.get() {
                notify(&message);
            }
        }
    }
}

/// The player's own update stopped: the game runs on, but the vehicle no
/// longer answers the controls. The first in a run writes the crash log.
pub fn player_stuck(reason: &str) {
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    let report = [
        "The player's update stopped: the game kept running, but the vehicle no longer answers the controls."
            .to_string(),
        reason.to_string(),
    ];
    if let Some(path) = write(&report) {
        eprintln!("A crash log was saved to: {}", path.display());
    }
}

/// Where a crash log goes and the run it describes.
#[derive(Clone, Debug)]
pub struct CrashContext {
    pub path: PathBuf,
    /// The run: executable, command line, data root, platform, ...
    pub run: Vec<String>,
}

/// Keep one printed line (already stamped); the oldest falls out past
/// [`RECENT_LINES`].
pub fn remember(line: String) {
    if let Ok(mut recent) = RECENT.lock() {
        keep(&mut recent, line);
    }
}

fn keep(recent: &mut VecDeque<String>, line: String) {
    if recent.len() == RECENT_LINES {
        recent.pop_front();
    }
    recent.push_back(line);
}

/// Set how a native crash shows where its log is.
pub fn set_notifier(notify: fn(&str)) {
    let _ = NOTIFY.set(notify);
}

/// Set where a crash log goes, and on the first call install the panic
/// hook (and on Windows the unhandled-exception filter). Later calls only
/// replace the context, as when the launcher has chosen the data root or a
/// session log has started.
pub fn install(context: CrashContext) {
    if let Ok(mut slot) = CONTEXT.lock() {
        *slot = Some(context);
    }
    static HOOKS: Once = Once::new();
    HOOKS.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let backtrace = std::backtrace::Backtrace::force_capture();
            // The console and the session log get it as before.
            eprintln!("PANIC: {info}\n{backtrace}");
            let thread = std::thread::current();
            let mut failure = vec![format!(
                "The game panicked on thread '{}'.",
                thread.name().unwrap_or("unnamed")
            )];
            failure.extend(info.to_string().lines().map(str::to_string));
            failure.push(String::new());
            failure.push("Backtrace:".to_string());
            failure.extend(backtrace.to_string().lines().map(str::to_string));
            if let Ok(mut last) = LAST_PANIC.lock() {
                *last = Some(failure.clone());
            }
            // Written now: a panic off the main thread never reaches main's
            // handler. Main writes it again once the output is drained.
            write(&failure);
        }));
        #[cfg(windows)]
        os::install_exception_filter();
        let _ = std::thread::Builder::new()
            .name("watchdog".to_string())
            .spawn(watch);
    });
}

/// The last panic's report, for main to write again after draining.
pub fn last_panic() -> Option<Vec<String>> {
    LAST_PANIC.lock().ok().and_then(|last| last.clone())
}

/// Write the crash log: what failed, the run, then the recent output.
/// Returns its path, or `None` when there is no context or it cannot be
/// written.
pub fn write(failure: &[String]) -> Option<PathBuf> {
    let context = CONTEXT.lock().ok()?.clone()?;
    let recent: Vec<String> = RECENT
        .lock()
        .map(|recent| recent.iter().cloned().collect())
        .unwrap_or_default();
    write_with(&context, failure, &recent)
}

/// Write one log. Every lock here is tried, never waited for: the native
/// exception filter calls it on a thread that may hold one.
fn write_with(context: &CrashContext, failure: &[String], recent: &[String]) -> Option<PathBuf> {
    let game = GAME.try_lock().ok().and_then(|game| *game);
    let world_name = WORLD_NAME.try_lock().ok().and_then(|name| name.clone());
    let text = render(context, failure, game, world_name.as_ref(), recent);
    let dir = context.path.parent()?;
    std::fs::create_dir_all(dir).ok()?;
    let reused = LAST_WRITTEN.try_lock().ok().and_then(|last| {
        last.as_ref()
            .filter(|(written, _)| written.as_slice() == failure)
            .map(|(_, path)| path.clone())
    });
    let path = match reused {
        Some(path) => path,
        None => stamped_path(&context.path, &crate::session_log::local_stamp(), |path| {
            path.exists()
        }),
    };
    std::fs::write(&path, text).ok()?;
    if let Ok(mut last) = LAST_WRITTEN.try_lock() {
        *last = Some((failure.to_vec(), path.clone()));
    }
    Some(path)
}

/// `infest-crash-log-<stamp>.txt` beside `base`, or with `-2`, `-3`... when
/// that name is taken.
fn stamped_path(
    base: &std::path::Path,
    stamp: &str,
    taken: impl Fn(&std::path::Path) -> bool,
) -> PathBuf {
    let stem = base
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("infest-crash-log");
    let extension = base
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("txt");
    let dir = base.parent().unwrap_or(std::path::Path::new(""));
    let mut attempt = 1_u32;
    loop {
        let name = if attempt == 1 {
            format!("{stem}-{stamp}.{extension}")
        } else {
            format!("{stem}-{stamp}-{attempt}.{extension}")
        };
        let path = dir.join(name);
        if !taken(&path) {
            return path;
        }
        attempt += 1;
    }
}

fn render(
    context: &CrashContext,
    failure: &[String],
    game: Option<GameState>,
    world_name: Option<&(u32, String)>,
    recent: &[String],
) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "Infestation port crash log");
    let _ = writeln!(
        text,
        "Written: {} (local)",
        crate::session_log::local_stamp()
    );
    let _ = writeln!(text);
    let _ = writeln!(text, "What failed:");
    for line in failure {
        let _ = writeln!(text, "  {line}");
    }
    let _ = writeln!(text);
    let _ = writeln!(text, "The run:");
    for line in &context.run {
        let _ = writeln!(text, "  {line}");
    }
    let _ = writeln!(text);
    if let Some(game) = game {
        let _ = writeln!(text, "The game, as last noted:");
        for line in game_lines(game, world_name) {
            let _ = writeln!(text, "  {line}");
        }
        let _ = writeln!(text);
    }
    if recent.is_empty() {
        let _ = writeln!(text, "The game printed nothing before it stopped.");
    } else {
        let _ = writeln!(
            text,
            "The last {} lines it printed, oldest first:",
            recent.len()
        );
        for line in recent {
            let _ = writeln!(text, "  {line}");
        }
    }
    text
}

fn game_lines(game: GameState, world_name: Option<&(u32, String)>) -> Vec<String> {
    let world = match game.world {
        Some(world) => match world_name.filter(|(noted, _)| *noted == world) {
            Some((_, name)) => format!("world {world} ({name})"),
            None => format!("world {world}"),
        },
        None => "no world".to_string(),
    };
    let mut lines = vec![format!(
        "Phase {}, {world}, {} s into the mission.",
        game.phase, game.elapsed_seconds
    )];
    match game.unit {
        Some(unit) => {
            let position = unit.position.map_or_else(
                || "at an unknown position".to_string(),
                |[x, y, z]| format!("at ({x}, {}, {z})", y as i16),
            );
            let health = match unit.maximum_health {
                Some(maximum) => format!("health {} of {maximum}", unit.health),
                None => format!("health {}", unit.health),
            };
            let vehicle = unit.vehicle.map_or_else(String::new, |(form, primary, secondary)| {
                format!("; vehicle form {form}, primary weapon row {primary}, secondary row {secondary}")
            });
            lines.push(format!("The player's unit: {position}, {health}{vehicle}."));
        }
        None => lines.push("The player has no unit.".to_string()),
    }
    lines
}

#[cfg(windows)]
mod os {
    use std::ffi::c_void;

    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        record: *mut ExceptionRecord,
        address: *mut c_void,
        parameters: u32,
        information: [usize; 15],
    }

    #[repr(C)]
    struct ExceptionPointers {
        record: *mut ExceptionRecord,
        context: *mut c_void,
    }

    type Filter = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetUnhandledExceptionFilter(filter: Option<Filter>) -> Option<Filter>;
    }

    /// Only exceptions nothing else handles reach it: Rust's own panics
    /// unwind through their handlers first.
    pub fn install_exception_filter() {
        unsafe {
            SetUnhandledExceptionFilter(Some(filter));
        }
    }

    unsafe extern "system" fn filter(pointers: *mut ExceptionPointers) -> i32 {
        const CONTINUE_SEARCH: i32 = 0;
        let Some(record) = pointers
            .as_ref()
            .and_then(|pointers| pointers.record.as_ref())
        else {
            return CONTINUE_SEARCH;
        };
        let mut failure = vec![format!(
            "The game crashed in native code: exception {:#010X} at {:#X} on thread '{}'.",
            record.code,
            record.address as usize,
            std::thread::current().name().unwrap_or("unnamed")
        )];
        // An access violation names the access and the address.
        if record.code == 0xC000_0005 && record.parameters >= 2 {
            let access = match record.information[0] {
                0 => "reading",
                1 => "writing",
                8 => "executing",
                _ => "accessing",
            };
            failure.push(format!(
                "Access violation {access} {:#X}.",
                record.information[1]
            ));
        }
        // The crashing thread may hold a lock: never wait for one here.
        let context = super::CONTEXT
            .try_lock()
            .ok()
            .and_then(|context| context.clone());
        if let Some(context) = context {
            let recent: Vec<String> = super::RECENT
                .try_lock()
                .map(|recent| recent.iter().cloned().collect())
                .unwrap_or_default();
            if let Some(path) = super::write_with(&context, &failure, &recent) {
                // The process ends after the filter; say where the log is.
                let message = format!(
                    "Infestation crashed.\n\nA crash log was saved to:\n{}",
                    path.display()
                );
                eprintln!("{message}");
                if let Some(notify) = super::NOTIFY.get() {
                    notify(&message);
                }
            }
        }
        CONTINUE_SEARCH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_says_what_failed_then_the_run_then_the_last_lines() {
        let context = CrashContext {
            path: PathBuf::from("logs").join(FILE_NAME),
            run: vec!["Command line: [\"inf-game\"]".to_string()],
        };
        let text = render(
            &context,
            &["Infestation bootstrap failed: no data".to_string()],
            None,
            None,
            &["[    0:01.000] Start harness: world 12".to_string()],
        );
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Infestation port crash log");
        assert!(lines[1].starts_with("Written: "));
        assert_eq!(
            &lines[3..],
            [
                "What failed:",
                "  Infestation bootstrap failed: no data",
                "",
                "The run:",
                "  Command line: [\"inf-game\"]",
                "",
                "The last 1 lines it printed, oldest first:",
                "  [    0:01.000] Start harness: world 12",
            ]
        );
        assert!(render(&context, &[], None, None, &[])
            .contains("The game printed nothing before it stopped."));
    }

    /// The game section names the world, the unit and its weapon rows.
    #[test]
    fn the_log_describes_the_game_as_last_noted() {
        let context = CrashContext {
            path: PathBuf::from("logs").join(FILE_NAME),
            run: Vec::new(),
        };
        let game = GameState {
            phase: 4,
            world: Some(3),
            elapsed_seconds: 812,
            unit: Some(UnitState {
                position: Some([16645, 0xf91c, 18695]),
                health: 700,
                maximum_health: Some(1000),
                vehicle: Some((1, 7, 8)),
            }),
        };
        let text = render(
            &context,
            &[],
            Some(game),
            Some(&(3, "Stendahl Alpha".to_string())),
            &[],
        );
        assert!(text.contains(
            "The game, as last noted:\n  Phase 4, world 3 (Stendahl Alpha), 812 s into the mission.\n  The player's unit: at (16645, -1764, 18695), health 700 of 1000; vehicle form 1, primary weapon row 7, secondary row 8.\n"
        ), "{text}");
    }

    /// Each crash takes its own file, numbered within the same second.
    #[test]
    fn each_crash_takes_its_own_stamped_file() {
        let base = PathBuf::from("logs").join(FILE_NAME);
        let first = stamped_path(&base, "20261017-193715", |_| false);
        assert_eq!(
            first,
            PathBuf::from("logs").join("infest-crash-log-20261017-193715.txt")
        );
        let second = stamped_path(&base, "20261017-193715", |path| *path == first);
        assert_eq!(
            second,
            PathBuf::from("logs").join("infest-crash-log-20261017-193715-2.txt")
        );
    }

    #[test]
    fn a_stall_names_the_frame_the_step_and_the_object() {
        assert_eq!(
            stall_report(812, 15_400, Step::ActorUpdate as u8, 184, 15_350),
            [
                "The game stopped responding: frame 812 has not finished after 15.4 s.",
                "It has been in an actor's update for 15.3 s (object slot 183).",
                "The game was still running when this was written; if it recovers, it carries on.",
            ]
        );
        assert_eq!(
            stall_report(3, 20_000, Step::Loading as u8, 0, 19_000)[1],
            "It has been in loading a world for 19.0 s."
        );
    }

    #[test]
    fn only_the_last_lines_are_kept() {
        let mut recent = VecDeque::new();
        for index in 0..RECENT_LINES + 5 {
            keep(&mut recent, format!("line {index}"));
        }
        assert_eq!(recent.len(), RECENT_LINES);
        assert_eq!(recent.front().unwrap(), "line 5");
        assert_eq!(
            recent.back().unwrap(),
            &format!("line {}", RECENT_LINES + 4)
        );
    }
}
