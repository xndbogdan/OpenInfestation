//! Opt-in observation of actual SDL submissions and final mixer samples.
//!
//! Capture never supplies game timing. Both streams use one monotonic epoch;
//! all disk/PNG work runs on a bounded worker queue. Overflow is an explicit
//! incomplete capture, never a reason to block the audio callback.

use serde_json::json;
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        mpsc::{self, SyncSender, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const QUEUE_CAPACITY: usize = 16;
const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CaptureError {
    #[error("session capture: {0}")]
    Invalid(String),
    #[error("session capture I/O: {0}")]
    Io(#[from] std::io::Error),
}

struct State {
    epoch: Instant,
    stop: AtomicBool,
    complete: AtomicBool,
    finished_ns: AtomicU64,
    failure: AtomicU8,
}

#[derive(Clone)]
pub(crate) struct CaptureHandle {
    state: Arc<State>,
    sender: SyncSender<Event>,
}

/// State observed after the real linked draw, before SDL presentation. These
/// fields explain the recorded frame; they never drive simulation or drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct WorldPresentationState {
    pub session_phase: u8,
    pub current_world: u32,
    pub forced_black: bool,
    pub animation_clock: u32,
    pub timing_average_microseconds: u32,
    pub particle_load_factor_16_16: u32,
    pub manager_far_1c: u32,
    pub manager_header_20: Option<u32>,
    pub retained_predraw_5c8_5cc: [i32; 2],
    pub terrain_spacing_units: u16,
    pub renderer_view_eye: [i32; 3],
    pub renderer_projection_fade: [i32; 2],
}

enum Event {
    Frame {
        elapsed_ns: u64,
        dimensions: [u32; 2],
        rgba: Vec<u8>,
        phase: &'static str,
        world: Option<WorldPresentationState>,
        readback_ns: u64,
    },
    Audio {
        elapsed_ns: u64,
        samples: Vec<f32>,
    },
}

impl CaptureHandle {
    pub(crate) fn elapsed_ns(&self) -> u64 {
        self.state
            .epoch
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64
    }
    fn submit(&self, event: Event) {
        if self.state.stop.load(Ordering::Relaxed)
            || self.state.failure.load(Ordering::Relaxed) != 0
        {
            return;
        }
        match self.sender.try_send(event) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.state.failure.store(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.state.failure.store(2, Ordering::Relaxed);
            }
        }
    }
    pub(crate) fn frame(
        &self,
        dimensions: [u32; 2],
        rgba: Vec<u8>,
        phase: &'static str,
        world: Option<WorldPresentationState>,
        readback_ns: u64,
    ) {
        self.submit(Event::Frame {
            elapsed_ns: self.elapsed_ns(),
            dimensions,
            rgba,
            phase,
            world,
            readback_ns,
        });
    }
    pub(crate) fn audio(&self, elapsed_ns: u64, samples: &[f32]) {
        if self.state.stop.load(Ordering::Relaxed)
            || self.state.failure.load(Ordering::Relaxed) != 0
        {
            return;
        }
        self.submit(Event::Audio {
            elapsed_ns,
            samples: samples.to_vec(),
        });
    }
    pub(crate) fn readback_failed(&self) {
        self.state.failure.store(3, Ordering::Relaxed);
    }
    pub(crate) fn check(&self) -> Result<(), CaptureError> {
        match self.state.failure.load(Ordering::Relaxed) {
            0 => Ok(()),
            1 => Err(CaptureError::Invalid(
                "bounded observer queue overflow; recording is incomplete".into(),
            )),
            2 => Err(CaptureError::Invalid(
                "capture writer failed; recording is incomplete".into(),
            )),
            3 => Err(CaptureError::Invalid(
                "SDL framebuffer readback failed; recording is incomplete".into(),
            )),
            _ => Err(CaptureError::Invalid("capture failed".into())),
        }
    }
}

pub(crate) struct SessionCapture {
    handle: CaptureHandle,
    worker: Option<JoinHandle<Result<(), CaptureError>>>,
    directory: PathBuf,
}

impl SessionCapture {
    pub(crate) fn start(directory: &Path, dimensions: [u32; 2]) -> Result<Self, CaptureError> {
        if directory.exists() {
            return Err(CaptureError::Invalid(
                "output directory already exists".into(),
            ));
        }
        // This observer is specifically for the uncropped matched retail review.
        if dimensions != [1024, 768] {
            return Err(CaptureError::Invalid(
                "comparison recording requires 1024x768 output".into(),
            ));
        }
        fs::create_dir_all(directory)?;
        fs::create_dir(directory.join("frames"))?;
        let directory = fs::canonicalize(directory)?;
        let epoch = Instant::now();
        let utc_unix_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| CaptureError::Invalid(e.to_string()))?
            .as_nanos()
            .to_string();
        let state = Arc::new(State {
            epoch,
            stop: AtomicBool::new(false),
            complete: AtomicBool::new(false),
            finished_ns: AtomicU64::new(0),
            failure: AtomicU8::new(0),
        });
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let worker_state = state.clone();
        let worker_directory = directory.clone();
        let worker = thread::Builder::new()
            .name("session-capture".into())
            .spawn(move || {
                let result = write_capture(
                    &worker_directory,
                    dimensions,
                    &worker_state,
                    &receiver,
                    &utc_unix_ns,
                );
                if let Err(error) = &result {
                    worker_state.failure.store(2, Ordering::Relaxed);
                    let _ = fs::write(worker_directory.join("failure.txt"), error.to_string());
                }
                result
            })?;
        Ok(Self {
            handle: CaptureHandle { state, sender },
            worker: Some(worker),
            directory,
        })
    }
    pub(crate) fn handle(&self) -> CaptureHandle {
        self.handle.clone()
    }
    pub(crate) fn finish(mut self) -> Result<PathBuf, CaptureError> {
        self.stop(true)?;
        self.handle.check()?;
        Ok(self.directory.clone())
    }
    fn stop(&mut self, complete: bool) -> Result<(), CaptureError> {
        self.handle
            .state
            .complete
            .store(complete, Ordering::Release);
        self.handle
            .state
            .finished_ns
            .store(self.handle.elapsed_ns(), Ordering::Release);
        self.handle.state.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| CaptureError::Invalid("capture writer panicked".into()))??;
        }
        Ok(())
    }
}
impl Drop for SessionCapture {
    fn drop(&mut self) {
        if self.worker.is_some() {
            if let Err(error) = self.stop(false) {
                eprintln!("Infestation: {error}");
            }
        }
    }
}

fn write_capture(
    directory: &Path,
    expected: [u32; 2],
    state: &State,
    receiver: &mpsc::Receiver<Event>,
    utc_unix_ns: &str,
) -> Result<(), CaptureError> {
    let mut audio = BufWriter::new(File::create(directory.join("audio.f32"))?);
    let mut frames = Vec::new();
    let mut chunks = Vec::new();
    let mut audio_frames = 0_u64;
    loop {
        let event = match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) if state.stop.load(Ordering::Acquire) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match event {
            Event::Frame {
                elapsed_ns,
                dimensions,
                rgba,
                phase,
                world,
                readback_ns,
            } => {
                if dimensions != expected
                    || rgba.len() != dimensions[0] as usize * dimensions[1] as usize * 4
                {
                    return Err(CaptureError::Invalid(
                        "presented dimensions changed or pixel length is invalid".into(),
                    ));
                }
                let name = format!("frames/{:08}.png", frames.len());
                let file = BufWriter::new(File::create(directory.join(&name))?);
                let mut encoder = png::Encoder::new(file, dimensions[0], dimensions[1]);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.set_compression(png::Compression::Fast);
                let mut writer = encoder
                    .write_header()
                    .map_err(|e| CaptureError::Invalid(e.to_string()))?;
                writer
                    .write_image_data(&rgba)
                    .map_err(|e| CaptureError::Invalid(e.to_string()))?;
                writer
                    .finish()
                    .map_err(|e| CaptureError::Invalid(e.to_string()))?;
                frames.push(
                    json!({"path":name,"elapsed_ns":elapsed_ns,"presentation":phase,"world":world,"readback_ns":readback_ns}),
                );
            }
            Event::Audio {
                elapsed_ns,
                samples,
            } => {
                if samples.len() % CHANNELS as usize != 0 {
                    return Err(CaptureError::Invalid("partial audio sample frame".into()));
                }
                for sample in &samples {
                    audio.write_all(&sample.to_le_bytes())?;
                }
                let count = samples.len() as u64 / u64::from(CHANNELS);
                chunks.push(json!({"callback_elapsed_ns":elapsed_ns,"first_sample_frame":audio_frames,"sample_frames":count}));
                audio_frames += count;
            }
        }
    }
    audio.flush()?;
    let failure = state.failure.load(Ordering::Acquire);
    let complete = state.complete.load(Ordering::Acquire)
        && failure == 0
        && !frames.is_empty()
        && audio_frames != 0;
    let manifest = json!({
        "schema_version":1,"complete":complete,"observer_failure":failure,
        "completion_scope":"observer finished normally with both streams; this does not assert intro completion or retail acceptance",
        "executable":std::env::current_exe().ok().map(|path| path.to_string_lossy().into_owned()),
        "arguments":std::env::args_os().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
        "started_utc_unix_ns":utc_unix_ns,"finished_elapsed_ns":state.finished_ns.load(Ordering::Acquire),
        "dimensions":expected,"clock":"shared monotonic Instant; realtime game clock required",
        "source":"actual SDL canvas pixels submitted before Present, timestamped immediately after Present; loading and normal callbacks included",
        "frames":frames,
        "audio":{"path":"audio.f32","format":"f32le","sample_rate":SAMPLE_RATE,"channels":CHANNELS,
                  "sample_frames":audio_frames,"chunks":chunks,
                  "timing_scope":"actual final mixer samples, timestamped at callback entry; SDL/device speaker latency is not measured"},
        "limits":["Observation can add readback/queue overhead; it never changes simulation clock policy.",
                  "No image crop, resize, reconstructed draw, or synthesized sound is performed.",
                  "An interrupted run or bounded queue overflow is incomplete and cannot be muxed as a complete capture."]
    });
    let mut file = BufWriter::new(File::create(directory.join("capture.json"))?);
    serde_json::to_writer_pretty(&mut file, &manifest)
        .map_err(|e| CaptureError::Invalid(e.to_string()))?;
    file.write_all(b"\n")?;
    file.flush()?;
    if !complete {
        return Err(CaptureError::Invalid(
            "recording is incomplete; see capture.json".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
