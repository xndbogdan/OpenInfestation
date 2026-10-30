//! Title-video frame store: offline-transcoded lossless frames plus timing.
//!
//! Retail decodes `Title.avi` (640x480 IV50, 15 fps, 315 frames) through its
//! DirectShow graph, which on the recorded machine resolves to LAV Video
//! (FFmpeg lineage); `ir50_32.dll` never loads there. The port plays
//! pre-transcoded RGB PNGs with no runtime codec dependency (decision record
//! in [frontend runtime](../../../docs/re/FRONTEND_RUNTIME.md#title-handoff)):
//!
//! ```sh
//! ffmpeg -hide_banner -i Title.avi -map 0:v -vsync passthrough \
//!     -start_number 0 <data-root>/video/title_%03d.png
//! ```
//!
//! That exact command under ffmpeg 7.1.1 yields 315 PNGs
//! (`title_000.png`..`title_314.png`); decoding `title_000.png` back to
//! raw RGB24 reproduces the transcoder's own `rgb24 framemd5` frame hash
//! bit-exactly, so the store preserves the transcoder presentation without
//! loss. Frames live next to `Title.avi`, local and uncommitted; a missing
//! store is an explicit error the session maps to the native failed-open
//! path (first tick finishes to phase 2).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;

use thiserror::Error;

/// Transcoded title-video frames and their presentation timing.
#[derive(Clone, Debug)]
pub struct TitleFrameStore {
    dir: PathBuf,
    width: u32,
    height: u32,
    fps: f32,
    count: u32,
}

impl TitleFrameStore {
    /// Frame layout `title_000.png`.. for `count` frames of `width`x`height`
    /// at `fps`. Rejects an absent directory before any frame is read.
    pub fn open(
        dir: &Path,
        width: u32,
        height: u32,
        fps: f32,
        count: u32,
    ) -> Result<Self, TitleFramesError> {
        if !dir.is_dir() {
            return Err(TitleFramesError::MissingDirectory(dir.into()));
        }
        Ok(Self {
            dir: dir.into(),
            width,
            height,
            fps,
            count,
        })
    }

    pub fn frame_count(&self) -> u32 {
        self.count
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Presentation index for seconds since the first presented frame:
    /// `floor(elapsed * fps)`, or `None` once past the final frame (the
    /// native status-2 finish). A nonpositive rate never advances.
    pub fn frame_index_for_elapsed(&self, elapsed_secs: f64) -> Option<usize> {
        if !(self.fps > 0.0) || elapsed_secs < 0.0 {
            return None;
        }
        let index = (elapsed_secs * f64::from(self.fps)) as usize;
        if index < self.count as usize {
            Some(index)
        } else {
            None
        }
    }

    /// Path of one transcoded frame: `title_<index:03>.png`.
    pub fn frame_path(&self, index: usize) -> PathBuf {
        self.dir.join(format!("title_{index:03}.png"))
    }

    /// Decode one frame to top-down RGBA bytes, verifying dimensions and
    /// accepting only 8-bit RGB/RGBA sources. Anything else fails before
    /// any pixel reaches the renderer.
    pub fn load_rgba(&self, index: usize) -> Result<Vec<u8>, TitleFramesError> {
        if index >= self.count as usize {
            return Err(TitleFramesError::FrameOutOfRange {
                index,
                count: self.count,
            });
        }
        let path = self.frame_path(index);
        let file = File::open(&path).map_err(|source| TitleFramesError::Io {
            path: path.clone(),
            source,
        })?;
        let decoder = png::Decoder::new(file);
        let mut reader = decoder
            .read_info()
            .map_err(|reason| TitleFramesError::Decode {
                path: path.clone(),
                reason: reason.to_string(),
            })?;
        let mut raw = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut raw)
            .map_err(|reason| TitleFramesError::Decode {
                path: path.clone(),
                reason: reason.to_string(),
            })?;
        if info.width != self.width || info.height != self.height {
            return Err(TitleFramesError::Dimensions {
                path,
                expected: (self.width, self.height),
                actual: (info.width, info.height),
            });
        }
        let stride = self.width as usize * self.height as usize;
        match info.color_type {
            png::ColorType::Rgba => {
                if raw.len() != stride * 4 {
                    return Err(TitleFramesError::Decode {
                        path,
                        reason: format!(
                            "RGBA frame has {} bytes, expected {}",
                            raw.len(),
                            stride * 4
                        ),
                    });
                }
                Ok(raw)
            }
            png::ColorType::Rgb => {
                if raw.len() != stride * 3 {
                    return Err(TitleFramesError::Decode {
                        path,
                        reason: format!(
                            "RGB frame has {} bytes, expected {}",
                            raw.len(),
                            stride * 3
                        ),
                    });
                }
                let mut rgba = Vec::with_capacity(stride * 4);
                for pixel in raw.chunks_exact(3) {
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
                Ok(rgba)
            }
            other => Err(TitleFramesError::ColorType {
                path,
                color_type: format!("{other:?}"),
            }),
        }
    }
}

/// Errors opening or reading the transcoded title frames.
#[derive(Debug, Error)]
pub enum TitleFramesError {
    #[error("transcoded title directory is absent: {}", .0.display())]
    MissingDirectory(PathBuf),
    #[error("title frame {index} is outside 0..{count}")]
    FrameOutOfRange { index: usize, count: u32 },
    #[error("cannot read title frame {path}: {source}", path = .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot decode title frame {path}: {reason}", path = .path.display())]
    Decode { path: PathBuf, reason: String },
    #[error("title frame {path} is {actual:?}, expected {expected:?}", path = .path.display())]
    Dimensions {
        path: PathBuf,
        expected: (u32, u32),
        actual: (u32, u32),
    },
    #[error("title frame {path} has unsupported PNG color type {color_type}", path = .path.display())]
    ColorType { path: PathBuf, color_type: String },
}

use inf_formats::{AviContainer, AviError};

use crate::platform_audio::TitleAudioSink;
use crate::title_screen::PLAYBACK_FINISHED;

/// Title-video session player: validated container, frame store and PCM,
/// driven by an explicit elapsed clock. The owner (a later session host)
/// maps construction failure to the native failed-open path: the first
/// tick finishes to phase 2 exactly as with a missing `Title.avi`.
#[derive(Debug)]
pub struct TitlePlayer {
    store: TitleFrameStore,
    audio_frames: Vec<i16>,
    audio_started: bool,
}

/// Errors opening or presenting the title video. Missing files stay
/// distinct from malformed bytes so the session can preserve the native
/// failed-open outcome without hiding corrupt data.
#[derive(Debug, Error)]
pub enum TitlePlayerError {
    #[error("Title.avi is absent: {}", .0.display())]
    MissingAvi(PathBuf),
    #[error("cannot read Title.avi at {path}: {source}", path = .path.display())]
    AviIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Container(#[from] AviError),
    #[error(transparent)]
    Frames(#[from] TitleFramesError),
    #[error("Title.avi audio has {0} bytes, not whole 16-bit samples")]
    OddAudioBytes(usize),
}

impl TitlePlayer {
    /// Validate `Title.avi` through the canonical container, open the
    /// transcoded `video/` store against its geometry, and decode the PCM
    /// audio to stereo 16-bit frames.
    pub fn open(data_root: &Path) -> Result<Self, TitlePlayerError> {
        let avi_path = crate::retail_path(data_root, "Title.avi");
        let bytes = std::fs::read(&avi_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                TitlePlayerError::MissingAvi(avi_path.clone())
            } else {
                TitlePlayerError::AviIo {
                    path: avi_path.clone(),
                    source,
                }
            }
        })?;
        let avi = AviContainer::open(&bytes)?;
        let info = avi.info;
        let store = TitleFrameStore::open(
            &data_root.join("video"),
            info.width,
            info.height,
            info.fps,
            info.total_frames,
        )?;
        let pcm = avi.audio_pcm();
        if pcm.len() % 2 != 0 {
            return Err(TitlePlayerError::OddAudioBytes(pcm.len()));
        }
        let audio_frames = pcm
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        Ok(Self {
            store,
            audio_frames,
            audio_started: false,
        })
    }

    pub fn frame_count(&self) -> u32 {
        self.store.frame_count()
    }

    /// Transcoded frame dimensions; the surface may differ (the native
    /// fullscreen video renderer stretches regardless of display mode).
    pub fn frame_dimensions(&self) -> (u32, u32) {
        (self.store.width(), self.store.height())
    }

    /// Native status poll (`00494DB0`): 0 while a frame remains, 2 once
    /// past the final frame.
    pub fn status_for_elapsed(&self, elapsed_secs: f64) -> u32 {
        match self.store.frame_index_for_elapsed(elapsed_secs) {
            Some(_) => 0,
            None => PLAYBACK_FINISHED,
        }
    }

    /// Present one frame: decode RGBA, start sound once on the first
    /// frame, stop sound past the end. `Finished` carries no pixels.
    pub fn present(
        &mut self,
        elapsed_secs: f64,
        audio: &mut dyn TitleAudioSink,
    ) -> Result<TitlePresentOutcome, TitlePlayerError> {
        match self.store.frame_index_for_elapsed(elapsed_secs) {
            Some(index) => {
                let rgba = self.store.load_rgba(index)?;
                if !self.audio_started {
                    audio.play_title_audio(self.audio_frames.clone());
                    self.audio_started = true;
                }
                Ok(TitlePresentOutcome::Frame(rgba))
            }
            None => {
                audio.stop_title_audio();
                Ok(TitlePresentOutcome::Finished)
            }
        }
    }

    /// Graph teardown (`00494DC0`): stop sound; the owner drops the player.
    pub fn close(&mut self, audio: &mut dyn TitleAudioSink) {
        audio.stop_title_audio();
        self.audio_started = false;
    }
}

/// One per-frame draw outcome (`00494F80`): pixels or the finished tail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TitlePresentOutcome {
    Frame(Vec<u8>),
    Finished,
}

/// Title-video graph state behind `TitleVideoHost`: the player while a
/// handle is held, the open instant for elapsed timing, the last presented
/// RGBA frame for the surface blit, and a failure latch. A mid-playback
/// frame failure latches `failed`, after which status reports finished:
/// that mirrors the native setjmp handler, which finishes to phase 2
/// instead of stopping the screen.
#[derive(Debug, Default)]
pub struct TitleVideoState {
    player: Option<TitlePlayer>,
    start: Option<Instant>,
    current: Option<Vec<u8>>,
    failed: bool,
}

impl TitleVideoState {
    /// A video handle is held (`+F0` nonzero in native terms).
    pub fn is_present(&self) -> bool {
        self.player.is_some()
    }

    /// Last presented RGBA frame for the surface blit, if any.
    pub fn presented_frame(&self) -> Option<&[u8]> {
        self.current.as_deref()
    }

    /// Dimensions of the presented frame; the player validates every
    /// decoded frame against these, so the blit can trust them.
    pub fn presented_dimensions(&self) -> Option<(u32, u32)> {
        self.player.as_ref().map(TitlePlayer::frame_dimensions)
    }

    fn elapsed_secs(&self) -> f64 {
        self.start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0)
    }

    /// Factory open (`00404890`): validate and build the player, stamp the
    /// start clock. Any failure maps to the native failed-open outcome
    /// (`None`, `+F0` stays clear, no phase request); the caller logs.
    /// Run `.\scripts\ensure-title-video.ps1` (one ffmpeg command, see the
    /// module docs) to transcode `Title.avi` into `video/` so the video shows
    /// on a normal launch.
    pub fn open_title(&mut self, data_root: &Path) -> Option<()> {
        match TitlePlayer::open(data_root) {
            Ok(player) => {
                self.player = Some(player);
                self.start = Some(Instant::now());
                self.current = None;
                self.failed = false;
                Some(())
            }
            Err(error) => {
                eprintln!("Title video unavailable ({error}); continuing without video. Transcode with `.\\scripts\\ensure-title-video.ps1 -DataRoot '{}'` so the video shows on launch", data_root.display());
                self.player = None;
                self.start = None;
                self.current = None;
                self.failed = false;
                None
            }
        }
    }

    /// Status poll (`00494DB0`): 0 while frames remain, 2 once finished or
    /// after a latched frame failure. Unreachable without a held handle,
    /// like the native read at handle `+18`.
    pub fn playback_status(&self) -> u32 {
        self.playback_status_for_elapsed(self.elapsed_secs())
    }

    fn playback_status_for_elapsed(&self, elapsed_secs: f64) -> u32 {
        match &self.player {
            Some(player) if !self.failed => player.status_for_elapsed(elapsed_secs),
            Some(_) => PLAYBACK_FINISHED,
            None => 0,
        }
    }

    /// Per-frame draw (`00494F80`): decode RGBA into the held frame and
    /// start sound once; past the end, stop sound. A decode failure latches
    /// `failed` (logged) so the next status poll finishes to phase 2.
    pub fn present_frame(&mut self, audio: &mut dyn TitleAudioSink) {
        self.present_for_elapsed(self.elapsed_secs(), audio);
    }

    fn present_for_elapsed(&mut self, elapsed_secs: f64, audio: &mut dyn TitleAudioSink) {
        let Some(player) = &mut self.player else {
            return;
        };
        match player.present(elapsed_secs, audio) {
            Ok(TitlePresentOutcome::Frame(rgba)) => {
                self.current = Some(rgba);
            }
            Ok(TitlePresentOutcome::Finished) => {}
            Err(error) => {
                eprintln!("Title video frame failed ({error}); finishing to the frontend");
                audio.stop_title_audio();
                self.failed = true;
                self.current = None;
            }
        }
    }

    /// Finish/close (`004049C0`/`00494DC0`): stop sound and release the
    /// handle. Idempotent: stopping unplayed sound changes nothing.
    pub fn close_title(&mut self, audio: &mut dyn TitleAudioSink) {
        if let Some(player) = &mut self.player {
            player.close(audio);
        } else {
            audio.stop_title_audio();
        }
        self.player = None;
        self.start = None;
        self.current = None;
        self.failed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufWriter;

    fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) {
        let file = File::create(path).unwrap();
        let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
    }

    fn store(dir: &Path) -> TitleFrameStore {
        // 2x1 RGB: red, green.
        write_png(&dir.join("title_000.png"), 2, 1, &[255, 0, 0, 0, 255, 0]);
        write_png(
            &dir.join("title_001.png"),
            2,
            1,
            &[0, 0, 255, 255, 255, 255],
        );
        TitleFrameStore::open(dir, 2, 1, 15.0, 2).unwrap()
    }

    #[test]
    fn frame_timing_matches_fifteen_fps() {
        let dir = std::env::temp_dir().join("inf-title-timing");
        std::fs::create_dir_all(&dir).unwrap();
        let store = TitleFrameStore::open(&dir, 640, 480, 15.0, 315).unwrap();
        assert_eq!(store.frame_index_for_elapsed(0.0), Some(0));
        assert_eq!(store.frame_index_for_elapsed(1.0 / 15.0 - 1e-9), Some(0));
        assert_eq!(store.frame_index_for_elapsed(1.0 / 15.0), Some(1));
        assert_eq!(store.frame_index_for_elapsed(21.0 - 1e-6), Some(314));
        // 315 frames at 15 fps end at exactly 21 s: the finish boundary.
        assert_eq!(store.frame_index_for_elapsed(21.0), None);
        assert_eq!(store.frame_index_for_elapsed(-1.0), None);
        assert_eq!(
            TitleFrameStore::open(&dir, 640, 480, 0.0, 315)
                .unwrap()
                .frame_index_for_elapsed(5.0),
            None
        );
    }

    #[test]
    fn frames_decode_to_top_down_rgba() {
        let dir = std::env::temp_dir().join("inf-title-frames");
        std::fs::create_dir_all(&dir).unwrap();
        let store = store(&dir);
        assert_eq!(
            store.load_rgba(0).unwrap(),
            vec![255, 0, 0, 255, 0, 255, 0, 255]
        );
        assert_eq!(
            store.load_rgba(1).unwrap(),
            vec![0, 0, 255, 255, 255, 255, 255, 255]
        );
        assert!(matches!(
            store.load_rgba(2),
            Err(TitleFramesError::FrameOutOfRange { index: 2, count: 2 })
        ));
    }

    #[test]
    fn missing_directory_and_dimensions_fail_closed() {
        let absent = std::env::temp_dir().join("inf-title-absent-dir-xyz");
        assert!(matches!(
            TitleFrameStore::open(&absent, 2, 1, 15.0, 1),
            Err(TitleFramesError::MissingDirectory(_))
        ));
        let dir = std::env::temp_dir().join("inf-title-frames-dim");
        std::fs::create_dir_all(&dir).unwrap();
        write_png(&dir.join("title_000.png"), 4, 4, &vec![0; 48]);
        let store = TitleFrameStore::open(&dir, 2, 1, 15.0, 1).unwrap();
        assert!(matches!(
            store.load_rgba(0),
            Err(TitleFramesError::Dimensions { .. })
        ));
    }

    #[derive(Default)]
    struct FakeAudio {
        starts: Vec<Vec<i16>>,
        stops: u32,
    }

    impl TitleAudioSink for FakeAudio {
        fn play_title_audio(&mut self, frames: Vec<i16>) {
            self.starts.push(frames);
        }

        fn stop_title_audio(&mut self) {
            self.stops += 1;
        }
    }

    fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(tag);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        if out.len() % 2 != 0 {
            out.push(0);
        }
        out
    }

    /// Minimal flat AVI: headers then two opaque video chunks plus audio.
    fn write_test_avi(path: &Path, audio: &[u8]) {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(b"AVI ");
        let mut avih = vec![0u8; 56];
        avih[0..4].copy_from_slice(&66666u32.to_le_bytes());
        avih[16..20].copy_from_slice(&2u32.to_le_bytes());
        avih[32..36].copy_from_slice(&2u32.to_le_bytes());
        avih[36..40].copy_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&chunk(b"avih", &avih));
        let mut strh = vec![0u8; 56];
        strh[0..4].copy_from_slice(b"vids");
        strh[4..8].copy_from_slice(b"IV50");
        strh[20..24].copy_from_slice(&1u32.to_le_bytes());
        strh[24..28].copy_from_slice(&15u32.to_le_bytes());
        data.extend_from_slice(&chunk(b"strh", &strh));
        let mut strf = vec![0u8; 40];
        strf[0..4].copy_from_slice(&40u32.to_le_bytes());
        strf[4..8].copy_from_slice(&2u32.to_le_bytes());
        strf[8..12].copy_from_slice(&1i32.to_le_bytes());
        strf[12..14].copy_from_slice(&1u16.to_le_bytes());
        strf[14..16].copy_from_slice(&24u16.to_le_bytes());
        strf[16..20].copy_from_slice(b"IV50");
        data.extend_from_slice(&chunk(b"strf", &strf));
        let mut astrh = vec![0u8; 56];
        astrh[0..4].copy_from_slice(b"auds");
        astrh[20..24].copy_from_slice(&4u32.to_le_bytes());
        astrh[24..28].copy_from_slice(&88200u32.to_le_bytes());
        data.extend_from_slice(&chunk(b"strh", &astrh));
        let mut astrf = vec![0u8; 16];
        astrf[0..2].copy_from_slice(&1u16.to_le_bytes());
        astrf[2..4].copy_from_slice(&2u16.to_le_bytes());
        astrf[4..8].copy_from_slice(&22050u32.to_le_bytes());
        astrf[8..12].copy_from_slice(&88200u32.to_le_bytes());
        astrf[12..14].copy_from_slice(&4u16.to_le_bytes());
        astrf[14..16].copy_from_slice(&16u16.to_le_bytes());
        data.extend_from_slice(&chunk(b"strf", &astrf));
        data.extend_from_slice(&chunk(b"00dc", &[9, 9, 9, 9]));
        data.extend_from_slice(&chunk(b"00dc", &[8, 8, 8, 8]));
        data.extend_from_slice(&chunk(b"01wb", audio));
        let riff = (data.len() - 8) as u32;
        data[4..8].copy_from_slice(&riff.to_le_bytes());
        std::fs::write(path, &data).unwrap();
    }

    fn player_dir(name: &str, audio: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("video")).unwrap();
        write_test_avi(&dir.join("Title.avi"), audio);
        write_png(
            &dir.join("video/title_000.png"),
            2,
            1,
            &[255, 0, 0, 0, 255, 0],
        );
        write_png(
            &dir.join("video/title_001.png"),
            2,
            1,
            &[0, 0, 255, 255, 255, 255],
        );
        dir
    }

    #[test]
    fn player_absent_paths_stay_distinct() {
        let empty = std::env::temp_dir().join("inf-title-no-avi");
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        assert!(matches!(
            TitlePlayer::open(&empty),
            Err(TitlePlayerError::MissingAvi(_))
        ));
        std::fs::write(empty.join("Title.avi"), b"NOT_AN_AVI").unwrap();
        assert!(matches!(
            TitlePlayer::open(&empty),
            Err(TitlePlayerError::Container(_))
        ));
        let no_frames = std::env::temp_dir().join("inf-title-no-frames");
        let _ = std::fs::remove_dir_all(&no_frames);
        std::fs::create_dir_all(&no_frames).unwrap();
        write_test_avi(&no_frames.join("Title.avi"), &[0, 0, 0, 0]);
        assert!(matches!(
            TitlePlayer::open(&no_frames),
            Err(TitlePlayerError::Frames(
                TitleFramesError::MissingDirectory(_)
            ))
        ));
    }

    #[test]
    fn player_rejects_odd_audio_bytes() {
        let dir = std::env::temp_dir().join("inf-title-odd-audio");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("video")).unwrap();
        write_test_avi(&dir.join("Title.avi"), &[1, 2, 3]);
        assert!(matches!(
            TitlePlayer::open(&dir),
            Err(TitlePlayerError::OddAudioBytes(3))
        ));
    }

    #[test]
    fn player_plays_frames_starts_sound_once_and_finishes() {
        let dir = player_dir("inf-title-play", &[10, 20, 30, 40, 50, 60, 70, 80]);
        let mut player = TitlePlayer::open(&dir).unwrap();
        assert_eq!(player.frame_count(), 2);
        assert_eq!(player.status_for_elapsed(0.0), 0);
        assert_eq!(player.status_for_elapsed(2.0 / 15.0), 2);
        let mut audio = FakeAudio::default();
        let first = player.present(0.0, &mut audio).unwrap();
        assert_eq!(
            first,
            TitlePresentOutcome::Frame(vec![255, 0, 0, 255, 0, 255, 0, 255])
        );
        assert_eq!(audio.starts, vec![vec![0x140A, 0x281E, 0x3C32, 0x5046]]);
        let second = player.present(1.0 / 15.0, &mut audio).unwrap();
        assert_eq!(
            second,
            TitlePresentOutcome::Frame(vec![0, 0, 255, 255, 255, 255, 255, 255])
        );
        assert_eq!(audio.starts.len(), 1);
        assert_eq!(audio.stops, 0);
        assert_eq!(
            player.present(2.0 / 15.0, &mut audio).unwrap(),
            TitlePresentOutcome::Finished
        );
        assert_eq!(audio.stops, 1);
        player.close(&mut audio);
        assert_eq!(audio.stops, 2);
    }

    #[test]
    fn video_state_absent_paths_report_no_handle() {
        let mut state = TitleVideoState::default();
        assert!(!state.is_present());
        assert_eq!(state.presented_frame(), None);
        assert_eq!(state.playback_status_for_elapsed(99.0), 0);
        let mut audio = FakeAudio::default();
        state.present_for_elapsed(0.0, &mut audio);
        assert_eq!(audio.starts.len(), 0);
        assert_eq!(audio.stops, 0);
        state.close_title(&mut audio);
        assert_eq!(audio.stops, 1);
    }

    #[test]
    fn video_state_plays_and_latches_failure_to_finished() {
        let dir = player_dir("inf-title-state", &[10, 20, 30, 40]);
        let mut state = TitleVideoState::default();
        assert!(state.open_title(&dir).is_some());
        assert!(state.is_present());
        assert_eq!(state.playback_status_for_elapsed(0.0), 0);
        assert_eq!(
            state.playback_status_for_elapsed(2.0 / 15.0),
            PLAYBACK_FINISHED
        );
        let mut audio = FakeAudio::default();
        state.present_for_elapsed(0.0, &mut audio);
        assert_eq!(
            state.presented_frame().unwrap(),
            &[255, 0, 0, 255, 0, 255, 0, 255]
        );
        assert_eq!(audio.starts.len(), 1);
        // Corrupt the store mid-playback: presenting the missing frame
        // latches failure, and status finishes even for frame 0.
        std::fs::remove_file(dir.join("video/title_001.png")).unwrap();
        state.present_for_elapsed(1.0 / 15.0, &mut audio);
        assert_eq!(state.presented_frame(), None);
        assert_eq!(
            state.playback_status_for_elapsed(1.0 / 15.0),
            PLAYBACK_FINISHED
        );
        assert_eq!(state.playback_status_for_elapsed(0.0), PLAYBACK_FINISHED);
        state.close_title(&mut audio);
        assert!(!state.is_present());
    }
}
