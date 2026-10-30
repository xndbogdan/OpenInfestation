//! Port-only session logging for `--session-log`: everything the game prints
//! (stdout and stderr) is copied, line by line with the time since launch,
//! into `run-<local time>.log`, while the console still shows it. Beside it
//! the run keeps `run-<local time>.replay` (the clock samples and keys of the
//! session, see [`SessionRecorder`]) and copies of the settings and profiles
//! it started from, so a reported session can be replayed with
//! `--replay-session`.
//!
//! The copy works at the operating-system level: the process's standard
//! handles are swapped for pipes that a thread drains. Native code that
//! wrote through the C runtime's own descriptors (SDL's few messages) is not
//! captured.

use std::fs::File;
use std::io::{self, BufRead, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

/// The files of one logged session, sharing the `run-<stamp>` stem.
#[derive(Clone, Debug)]
pub struct SessionFiles {
    pub log: PathBuf,
    pub replay: PathBuf,
    pub settings: PathBuf,
    pub profiles: PathBuf,
}

impl SessionFiles {
    /// `dir/run-<stamp>.*`.
    pub fn new(dir: &Path, stamp: &str) -> Self {
        let file = |suffix: &str| dir.join(format!("run-{stamp}{suffix}"));
        Self {
            log: file(".log"),
            replay: file(".replay"),
            settings: file(".settings.json"),
            profiles: file(".profiles.sav"),
        }
    }

    /// The sibling files of a `.replay` path.
    pub fn from_replay(replay: &Path) -> Self {
        let stem = replay.with_extension("");
        let file = |suffix: &str| {
            let mut name = stem.clone().into_os_string();
            name.push(suffix);
            PathBuf::from(name)
        };
        Self {
            log: file(".log"),
            replay: replay.to_path_buf(),
            settings: file(".settings.json"),
            profiles: file(".profiles.sav"),
        }
    }
}

/// Local wall time as `YYYYMMDD-HHMMSS` (UTC where the platform has no
/// local clock here).
pub fn local_stamp() -> String {
    let [year, month, day, hour, minute, second] = os::local_time();
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// A running capture. Dropping it restores the standard handles and waits
/// until every captured byte is in the log.
pub struct SessionLog {
    path: PathBuf,
    file: Arc<Mutex<File>>,
    streams: Vec<(os::Redirect, JoinHandle<()>)>,
}

impl SessionLog {
    /// Create the log, write `header` and start copying stdout and stderr.
    pub fn start(path: &Path, header: &[String]) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = File::create(path)?;
        for line in header {
            writeln!(file, "{line}")?;
        }
        writeln!(file)?;
        let file = Arc::new(Mutex::new(file));
        let streams = capture("session-log", Some(&file))?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            streams,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write a line straight to the log, bypassing the console.
    pub fn note(&self, line: &str) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    }
}

impl Drop for SessionLog {
    fn drop(&mut self) {
        release(&mut self.streams);
    }
}

/// Without a session log the output still passes through the same pipes,
/// echoed to the console, so a crash log can end with the last lines the
/// game printed ([`crate::crash_log`]). Dropping it restores the standard
/// handles once every captured line is remembered.
pub struct OutputTail {
    streams: Vec<(os::Redirect, JoinHandle<()>)>,
}

impl OutputTail {
    pub fn start() -> io::Result<Self> {
        Ok(Self {
            streams: capture("output-tail", None)?,
        })
    }
}

impl Drop for OutputTail {
    fn drop(&mut self) {
        release(&mut self.streams);
    }
}

/// Swap stdout and stderr for pipes that a thread each drains into
/// [`copy_lines`].
fn capture(
    name: &str,
    file: Option<&Arc<Mutex<File>>>,
) -> io::Result<Vec<(os::Redirect, JoinHandle<()>)>> {
    let epoch = Instant::now();
    let mut streams = Vec::new();
    for stream in [os::Stream::Stdout, os::Stream::Stderr] {
        // Rust's stdout buffers whole lines; write them out before the
        // handle changes underneath it.
        io::stdout().flush()?;
        let (redirect, reader, echo) = os::redirect(stream)?;
        let file = file.map(Arc::clone);
        let thread = std::thread::Builder::new()
            .name(format!("{name}-{stream:?}"))
            .spawn(move || copy_lines(reader, echo, file.as_deref(), epoch))?;
        streams.push((redirect, thread));
    }
    Ok(streams)
}

fn release(streams: &mut Vec<(os::Redirect, JoinHandle<()>)>) {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    for (redirect, thread) in streams.drain(..) {
        // Restoring closes the pipe's last writer; the thread then sees
        // the end of the stream.
        drop(redirect);
        let _ = thread.join();
    }
}

/// Echo the raw bytes to the console and each complete line, stamped with
/// the time since launch, to the log (when there is one) and the crash
/// log's last lines. A partial last line is written too.
fn copy_lines(reader: File, mut echo: Option<File>, file: Option<&Mutex<File>>, epoch: Instant) {
    let mut reader = io::BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if let Some(echo) = echo.as_mut() {
            let _ = echo.write_all(&line);
            let _ = echo.flush();
        }
        let elapsed = epoch.elapsed();
        let text = String::from_utf8_lossy(&line);
        let stamped = format!(
            "[{:>5}:{:02}.{:03}] {}",
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60,
            elapsed.subsec_millis(),
            text.trim_end_matches(['\r', '\n'])
        );
        if let Some(Ok(mut file)) = file.map(Mutex::lock) {
            let _ = writeln!(file, "{stamped}");
            let _ = file.flush();
        }
        crate::crash_log::remember(stamped);
    }
}

/// One entry of a `.replay` file, in session order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayEntry {
    /// A clock sample in microseconds (`AppClock::sample`).
    Sample(u64),
    /// A physical key, by its SDL scancode name.
    Key { name: String, pressed: bool },
    /// The window lost focus (the keyboard releases every key).
    FocusLost,
    /// The pad's reading from this frame on, until the next one.
    Pad {
        native_type: u8,
        axes: u8,
        sample: crate::platform_input::PadSample,
    },
}

const REPLAY_MAGIC: &str = "inf-game session replay 1";

/// Writes a `.replay` file. The format is text, one entry per line, in the
/// order the session saw them:
///
/// ```text
/// inf-game session replay 1
/// args <the original command line, as JSON>
/// g <retail|remaster|remaster-traced> <W>x<H> [rows <7 bits>] [far <far>,<past> [<world>:<far>,<past>]...]
///                              the game the session plays, the frame its
///                              window opened with, what the device ran of
///                              the Vulkan rows, the traced fog's reach
/// s <microseconds> [repeats]   a clock sample, optionally repeated
/// k <0|1> <SDL scancode name>  a key released or pressed
/// f                            the window lost focus
/// p <type> <axes> <buttons> <six axes>   the pad's new reading
/// p <type> <axes> failed       the pad's read failed
/// ```
///
/// The pad is read every frame; only a reading that differs from the last
/// one is written, and a replay repeats the last one each frame.
///
/// Keys are written just before the clock sample of the frame that
/// received them, so a replay feeds both back in the same order.
pub struct SessionRecorder {
    out: BufWriter<File>,
    pending: Option<(u64, u64)>,
    pad: Option<(u8, u8, crate::platform_input::PadSample)>,
}

impl SessionRecorder {
    pub fn create(path: &Path, args: &[String]) -> io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "{REPLAY_MAGIC}")?;
        writeln!(
            out,
            "args {}",
            serde_json::to_string(args).map_err(io::Error::other)?
        )?;
        Ok(Self {
            out,
            pending: None,
            pad: None,
        })
    }

    /// The game the session plays and what it reads of the machine
    /// ([`crate::presentation::SessionGame`]), which its replay plays
    /// whatever the renderer and the output.
    pub fn game(&mut self, game: &crate::presentation::SessionGame) {
        self.flush_pending();
        let _ = writeln!(self.out, "g {}", game_line(game));
    }

    pub fn sample(&mut self, micros: u64) {
        match &mut self.pending {
            Some((value, count)) if *value == micros => *count += 1,
            _ => {
                self.flush_pending();
                self.pending = Some((micros, 1));
            }
        }
    }

    pub fn key(&mut self, name: &str, pressed: bool) {
        self.flush_pending();
        let _ = writeln!(self.out, "k {} {name}", u8::from(pressed));
    }

    pub fn focus_lost(&mut self) {
        self.flush_pending();
        let _ = writeln!(self.out, "f");
    }

    pub fn pad(&mut self, native_type: u8, axes: u8, sample: crate::platform_input::PadSample) {
        if self.pad == Some((native_type, axes, sample)) {
            return;
        }
        self.pad = Some((native_type, axes, sample));
        self.flush_pending();
        let _ = match sample {
            crate::platform_input::PadSample::Read {
                buttons,
                axes: [x, y, z, r, u, v],
            } => writeln!(
                self.out,
                "p {native_type} {axes} {buttons:x} {x} {y} {z} {r} {u} {v}"
            ),
            crate::platform_input::PadSample::Failed => {
                writeln!(self.out, "p {native_type} {axes} failed")
            }
        };
    }

    /// Push everything written so far to the file, so a crash loses
    /// nothing before it.
    pub fn flush(&mut self) {
        self.flush_pending();
        let _ = self.out.flush();
    }

    fn flush_pending(&mut self) {
        if let Some((value, count)) = self.pending.take() {
            let _ = if count == 1 {
                writeln!(self.out, "s {value}")
            } else {
                writeln!(self.out, "s {value} {count}")
            };
        }
    }
}

impl Drop for SessionRecorder {
    fn drop(&mut self) {
        self.flush();
    }
}

fn game_view_word(view: crate::presentation::GameView) -> &'static str {
    match view {
        crate::presentation::GameView::Retail => "retail",
        crate::presentation::GameView::Remaster { traced: false } => "remaster",
        crate::presentation::GameView::Remaster { traced: true } => "remaster-traced",
    }
}

/// A `g` line's fields after the tag.
fn game_line(game: &crate::presentation::SessionGame) -> String {
    let mut line = game_view_word(game.view).to_owned();
    if let Some([width, height]) = game.frame {
        line += &format!(" {width}x{height}");
    }
    if let Some(support) = game.support {
        let bits = std::iter::once(support.ray_tracing)
            .chain(support.upscalers)
            .chain([support.modern_sky]);
        line += " rows ";
        line.extend(bits.map(|on| if on { '1' } else { '0' }));
    }
    if let Some(far) = &game.far {
        let pair = |[far, past]: [f32; 2]| format!("{far},{past}");
        line += &format!(" far {}", pair(far.all));
        for (world, own) in &far.worlds {
            line += &format!(" {world}:{}", pair(*own));
        }
    }
    line
}

/// A `g` line's fields, as [`game_line`] writes them; an older line holds
/// the game alone.
fn parse_game(rest: &str) -> Option<crate::presentation::SessionGame> {
    use crate::presentation::GameView;
    let mut fields = rest.split(' ');
    let word = fields.next()?;
    let view = [
        GameView::Retail,
        GameView::Remaster { traced: false },
        GameView::Remaster { traced: true },
    ]
    .into_iter()
    .find(|view| game_view_word(*view) == word)?;
    let mut game = crate::presentation::SessionGame {
        view,
        ..Default::default()
    };
    let pair = |text: &str| -> Option<[f32; 2]> {
        let (far, past) = text.split_once(',')?;
        Some([far.parse().ok()?, past.parse().ok()?])
    };
    while let Some(field) = fields.next() {
        match field {
            "rows" => {
                let bits = fields
                    .next()?
                    .chars()
                    .map(|bit| match bit {
                        '0' => Some(false),
                        '1' => Some(true),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let [ray_tracing, a, b, c, d, e, modern_sky] = bits.try_into().ok()?;
                game.support = Some(crate::modern_settings::ModernSupport {
                    ray_tracing,
                    upscalers: [a, b, c, d, e],
                    modern_sky,
                });
            }
            "far" => {
                let mut far = crate::modern_tuning::TracedFar {
                    all: pair(fields.next()?)?,
                    worlds: Default::default(),
                };
                for own in fields.by_ref() {
                    let (world, own) = own.split_once(':')?;
                    far.worlds.insert(world.parse().ok()?, pair(own)?);
                }
                game.far = Some(far);
            }
            size => {
                let (width, height) = size.split_once('x')?;
                game.frame = Some([width.parse().ok()?, height.parse().ok()?]);
            }
        }
    }
    Some(game)
}

/// A parsed `.replay` file.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionReplay {
    pub args: Vec<String>,
    /// The game the session played; None in a replay made before it was
    /// recorded, which then plays the game of the renderer replaying it.
    pub game: Option<crate::presentation::SessionGame>,
    pub entries: Vec<ReplayEntry>,
    /// `--resume`: when the entries run out, the host's clock and keys take
    /// over instead of the run ending.
    pub resume: bool,
}

impl SessionReplay {
    pub fn read(path: &Path) -> io::Result<Self> {
        let mut text = String::new();
        File::open(path)?.read_to_string(&mut text)?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> io::Result<Self> {
        let invalid = |line: usize, why: &str| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("replay line {}: {why}", line + 1),
            )
        };
        let mut lines = text.lines().enumerate();
        match lines.next() {
            Some((_, magic)) if magic.trim_end() == REPLAY_MAGIC => {}
            _ => return Err(invalid(0, "not an inf-game session replay")),
        }
        let mut args = Vec::new();
        let mut game = None;
        let mut entries = Vec::new();
        for (index, line) in lines {
            let line = line.trim_end_matches('\r');
            let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
            match tag {
                "" => {}
                "args" => {
                    args = serde_json::from_str(rest)
                        .map_err(|_| invalid(index, "bad argument list"))?;
                }
                "g" => {
                    game = Some(parse_game(rest).ok_or_else(|| invalid(index, "bad game view"))?);
                }
                "s" => {
                    let mut fields = rest.split(' ');
                    let value = fields
                        .next()
                        .and_then(|value| value.parse().ok())
                        .ok_or_else(|| invalid(index, "bad clock sample"))?;
                    let count: u64 = match fields.next() {
                        Some(count) => count
                            .parse()
                            .map_err(|_| invalid(index, "bad repeat count"))?,
                        None => 1,
                    };
                    entries.extend((0..count).map(|_| ReplayEntry::Sample(value)));
                }
                "k" => {
                    let (pressed, name) = rest
                        .split_once(' ')
                        .ok_or_else(|| invalid(index, "bad key"))?;
                    let pressed = match pressed {
                        "0" => false,
                        "1" => true,
                        _ => return Err(invalid(index, "bad key state")),
                    };
                    entries.push(ReplayEntry::Key {
                        name: name.to_string(),
                        pressed,
                    });
                }
                "f" => entries.push(ReplayEntry::FocusLost),
                "p" => {
                    let bad = || invalid(index, "bad pad reading");
                    let mut fields = rest.split(' ');
                    let native_type = fields.next().and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                    let axes = fields.next().and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                    let sample = match fields.next() {
                        Some("failed") => crate::platform_input::PadSample::Failed,
                        Some(buttons) => {
                            let buttons = u32::from_str_radix(buttons, 16).map_err(|_| bad())?;
                            let mut positions = [0; 6];
                            for position in &mut positions {
                                *position =
                                    fields.next().and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                            }
                            crate::platform_input::PadSample::Read {
                                buttons,
                                axes: positions,
                            }
                        }
                        None => return Err(bad()),
                    };
                    entries.push(ReplayEntry::Pad {
                        native_type,
                        axes,
                        sample,
                    });
                }
                _ => return Err(invalid(index, "unknown entry")),
            }
        }
        Ok(Self {
            args,
            game,
            entries,
            resume: false,
        })
    }
}

#[cfg(windows)]
mod os {
    use std::ffi::c_void;
    use std::fs::File;
    use std::io;
    use std::os::windows::io::FromRawHandle;

    type Handle = *mut c_void;
    const INVALID_HANDLE: Handle = -1isize as Handle;

    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> Handle;
        fn SetStdHandle(which: u32, handle: Handle) -> i32;
        fn CreatePipe(
            read: *mut Handle,
            write: *mut Handle,
            attributes: *mut c_void,
            size: u32,
        ) -> i32;
        fn GetLocalTime(time: *mut SystemTime);
    }

    #[derive(Clone, Copy, Debug)]
    pub enum Stream {
        Stdout,
        Stderr,
    }

    impl Stream {
        fn id(self) -> u32 {
            match self {
                Self::Stdout => -11i32 as u32,
                Self::Stderr => -12i32 as u32,
            }
        }
    }

    /// Restores the original handle on drop, then closes the pipe's writer.
    pub struct Redirect {
        stream: Stream,
        original: Handle,
        writer: Option<File>,
    }

    // The handles are process-wide values, not thread-bound.
    unsafe impl Send for Redirect {}

    impl Drop for Redirect {
        fn drop(&mut self) {
            unsafe {
                SetStdHandle(self.stream.id(), self.original);
            }
            self.writer.take();
        }
    }

    pub fn redirect(stream: Stream) -> io::Result<(Redirect, File, Option<File>)> {
        unsafe {
            let original = GetStdHandle(stream.id());
            let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
            if CreatePipe(&mut read, &mut write, std::ptr::null_mut(), 0) == 0 {
                return Err(io::Error::last_os_error());
            }
            if SetStdHandle(stream.id(), write) == 0 {
                let error = io::Error::last_os_error();
                drop(File::from_raw_handle(read));
                drop(File::from_raw_handle(write));
                return Err(error);
            }
            // The console (if any) keeps receiving the output. The echo is
            // a second owner of a handle the process keeps; duplicating it
            // keeps the original open when the echo closes.
            let echo = if original.is_null() || original == INVALID_HANDLE {
                None
            } else {
                duplicate(original)
            };
            Ok((
                Redirect {
                    stream,
                    original,
                    writer: Some(File::from_raw_handle(write)),
                },
                File::from_raw_handle(read),
                echo,
            ))
        }
    }

    fn duplicate(handle: Handle) -> Option<File> {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> Handle;
            fn DuplicateHandle(
                source_process: Handle,
                source: Handle,
                target_process: Handle,
                target: *mut Handle,
                access: u32,
                inherit: i32,
                options: u32,
            ) -> i32;
        }
        const DUPLICATE_SAME_ACCESS: u32 = 2;
        unsafe {
            let process = GetCurrentProcess();
            let mut copy = std::ptr::null_mut();
            if DuplicateHandle(
                process,
                handle,
                process,
                &mut copy,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            ) == 0
            {
                return None;
            }
            Some(File::from_raw_handle(copy))
        }
    }

    pub fn local_time() -> [u16; 6] {
        let mut time = SystemTime::default();
        unsafe { GetLocalTime(&mut time) };
        [
            time.year,
            time.month,
            time.day,
            time.hour,
            time.minute,
            time.second,
        ]
    }
}

#[cfg(unix)]
mod os {
    use std::fs::File;
    use std::io;
    use std::os::unix::io::FromRawFd;

    extern "C" {
        fn pipe(fds: *mut i32) -> i32;
        fn dup(fd: i32) -> i32;
        fn dup2(from: i32, to: i32) -> i32;
        fn close(fd: i32) -> i32;
    }

    #[derive(Clone, Copy, Debug)]
    pub enum Stream {
        Stdout,
        Stderr,
    }

    impl Stream {
        fn fd(self) -> i32 {
            match self {
                Self::Stdout => 1,
                Self::Stderr => 2,
            }
        }
    }

    /// Points the descriptor back at the original on drop, which closes the
    /// pipe's last writer.
    pub struct Redirect {
        stream: Stream,
        original: i32,
    }

    impl Drop for Redirect {
        fn drop(&mut self) {
            unsafe {
                dup2(self.original, self.stream.fd());
                close(self.original);
            }
        }
    }

    pub fn redirect(stream: Stream) -> io::Result<(Redirect, File, Option<File>)> {
        unsafe {
            let original = dup(stream.fd());
            if original < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut fds = [0; 2];
            if pipe(fds.as_mut_ptr()) != 0 {
                let error = io::Error::last_os_error();
                close(original);
                return Err(error);
            }
            if dup2(fds[1], stream.fd()) < 0 {
                let error = io::Error::last_os_error();
                close(fds[0]);
                close(fds[1]);
                close(original);
                return Err(error);
            }
            close(fds[1]);
            let echo = match dup(original) {
                fd if fd >= 0 => Some(File::from_raw_fd(fd)),
                _ => None,
            };
            Ok((
                Redirect { stream, original },
                File::from_raw_fd(fds[0]),
                echo,
            ))
        }
    }

    /// UTC: the standard library has no local time zone.
    pub fn local_time() -> [u16; 6] {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as i64);
        let days = seconds.div_euclid(86_400);
        let rest = seconds.rem_euclid(86_400);
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        [
            year as u16,
            month as u16,
            day as u16,
            (rest / 3600) as u16,
            (rest / 60 % 60) as u16,
            (rest % 60) as u16,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_reads_back_in_session_order() {
        let dir = std::env::temp_dir().join(format!("inf-session-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("run.replay");
        {
            let mut recorder =
                SessionRecorder::create(&path, &["inf-game".into(), "--hidden".into()]).unwrap();
            recorder.game(&traced_game());
            recorder.sample(0);
            recorder.sample(1000);
            recorder.sample(1000);
            recorder.key("Left Ctrl", true);
            recorder.sample(2000);
            recorder.focus_lost();
            recorder.sample(2000);
            let read = crate::platform_input::PadSample::Read {
                buttons: 0x201,
                axes: [0, 65535, 32768, 1, 2, 3],
            };
            // A reading is written only when it changes.
            recorder.pad(2, 5, read);
            recorder.sample(3000);
            recorder.pad(2, 5, read);
            recorder.sample(3000);
            recorder.pad(2, 5, crate::platform_input::PadSample::Failed);
        }
        let replay = SessionReplay::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(replay.args, ["inf-game", "--hidden"]);
        assert_eq!(replay.game, Some(traced_game()));
        assert_eq!(
            replay.entries,
            [
                ReplayEntry::Sample(0),
                ReplayEntry::Sample(1000),
                ReplayEntry::Sample(1000),
                ReplayEntry::Key {
                    name: "Left Ctrl".into(),
                    pressed: true
                },
                ReplayEntry::Sample(2000),
                ReplayEntry::FocusLost,
                ReplayEntry::Sample(2000),
                ReplayEntry::Pad {
                    native_type: 2,
                    axes: 5,
                    sample: crate::platform_input::PadSample::Read {
                        buttons: 0x201,
                        axes: [0, 65535, 32768, 1, 2, 3],
                    },
                },
                ReplayEntry::Sample(3000),
                ReplayEntry::Sample(3000),
                ReplayEntry::Pad {
                    native_type: 2,
                    axes: 5,
                    sample: crate::platform_input::PadSample::Failed,
                },
            ]
        );
    }

    #[test]
    fn replay_siblings_share_the_run_stem() {
        let files = SessionFiles::from_replay(Path::new("logs/run-20261010-201500.replay"));
        assert_eq!(files.log, Path::new("logs/run-20261010-201500.log"));
        assert_eq!(
            files.profiles,
            Path::new("logs/run-20261010-201500.profiles.sav")
        );
        let made = SessionFiles::new(Path::new("logs"), "20261010-201500");
        assert_eq!(made.replay, files.replay);
        assert_eq!(made.settings, files.settings);
    }

    /// A traced remaster's game on a wide frame, a device without one
    /// upscaler, and a world seeing further than the rest.
    fn traced_game() -> crate::presentation::SessionGame {
        crate::presentation::SessionGame {
            view: crate::presentation::GameView::Remaster { traced: true },
            frame: Some([1365, 768]),
            support: Some(crate::modern_settings::ModernSupport {
                ray_tracing: true,
                upscalers: [true, true, false, true, true],
                modern_sky: true,
            }),
            far: Some(crate::modern_tuning::TracedFar {
                all: [1.0, 1.15],
                worlds: [(18, [1.5, 1.15])].into(),
            }),
        }
    }

    /// The `g` line as written, and as read back; a replay made before the
    /// game was recorded has none, one made before the frame was holds the
    /// game alone; anything this build does not know is refused.
    #[test]
    fn a_replay_reads_its_game() {
        use crate::presentation::{GameView, SessionGame};
        assert_eq!(
            game_line(&traced_game()),
            "remaster-traced 1365x768 rows 1110111 far 1,1.15 18:1.5,1.15"
        );
        let parse = |body: &str| SessionReplay::parse(&format!("{REPLAY_MAGIC}\n{body}"));
        assert_eq!(parse("args []\ns 0").unwrap().game, None);
        assert_eq!(
            parse("g retail 1024x768\ns 0").unwrap().game,
            Some(SessionGame {
                view: GameView::Retail,
                frame: Some([1024, 768]),
                ..Default::default()
            })
        );
        assert_eq!(
            parse("g remaster").unwrap().game.unwrap().view,
            GameView::Remaster { traced: false }
        );
        for bad in [
            "g sideways",
            "g retail 1024",
            "g remaster 1365x768 rows 11",
            "g remaster-traced 1365x768 far 1",
            "g remaster-traced 1365x768 far 1,1.15 18",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_replay_rejects_other_files() {
        assert!(SessionReplay::parse("{}").is_err());
        assert!(SessionReplay::parse(&format!("{REPLAY_MAGIC}\nk 2 Z")).is_err());
    }
}
