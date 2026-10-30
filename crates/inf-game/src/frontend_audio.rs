//! Session music selection (00487230..00487370) and CD retry service (00487B90).
//!
//! One owner must be shared across the session's frontend/world screens: retail
//! +5D0 retains its selection while 0065EE20/24/2C/30 own one process CD device.
//! This implements the single session-slot lifecycle, not multiple competing
//! music selections, sound effects, media decoding, or an actual audio backend.

use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicSelection {
    /// Full native word used by the track-length query; do not narrow it early.
    pub track_word: u32,
    /// Native TMSF: track/minute/second/frame in increasing byte significance.
    pub saved_tmsf: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdPlayRange {
    pub from_tmsf: u32,
    /// Next track's low byte; the other three bytes are zero.
    pub to_tmsf: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdPlaybackMode {
    Playing,
    /// All native mode strings other than the exact string "playing".
    Other,
}

/// Required transport operations corresponding to the MCI/CD consumers. A
/// missing backend must fail explicitly. These methods have no default stubs.
/// OS handle custody, MCI-string parsing and real playback belong to the backend.
pub trait FrontendAudioBackend {
    type Error;

    fn open_cd(&mut self) -> Result<(), Self::Error>;
    fn set_tmsf_format(&mut self) -> Result<(), Self::Error>;
    fn close_cd(&mut self) -> Result<(), Self::Error>;
    fn media_present(&mut self) -> Result<bool, Self::Error>;
    /// Native probes track 2, regardless of the requested music track.
    fn track_two_is_audio(&mut self) -> Result<bool, Self::Error>;
    fn playback_mode(&mut self) -> Result<CdPlaybackMode, Self::Error>;
    fn position_tmsf(&mut self) -> Result<u32, Self::Error>;
    /// MSF length: minutes at bits 0..7, seconds at 8..15, frames at 16..23.
    /// 004F68A0 deliberately substitutes 3600 seconds on this query's failure.
    fn track_length_msf(&mut self, track_word: u32) -> Result<u32, Self::Error>;
    fn play_range(&mut self, range: CdPlayRange) -> Result<(), Self::Error>;
    fn stop_cd(&mut self) -> Result<(), Self::Error>;
    /// Packed stereo AUX volume produced by 004872A0 then 004F7D80.
    fn set_cd_volume(&mut self, stereo_word: u32) -> Result<(), Self::Error>;
    /// Signed low 32-bit wall-clock seconds from native CRT time(0), 00513260.
    /// This is independent of scheduler elapsed microseconds.
    fn clock_seconds(&mut self) -> Result<i32, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioOperation {
    Open,
    SetTimeFormat,
    Close,
    MediaPresent,
    TrackTwoType,
    Mode,
    Position,
    Play,
    Stop,
    Volume,
    Clock,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum FrontendAudioError<E> {
    #[error("CD backend {operation:?} failed: {error}")]
    Backend { operation: AudioOperation, error: E },
    #[error("music owner stopped after a partial backend failure")]
    Stopped,
}

/// This is observed native state, not an assertion that sound is audible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MusicState {
    pub selection: Option<MusicSelection>,
    pub device_open: bool,
    pub active_selection: bool,
    pub audio_media_present: bool,
    /// Native zero sentinel is retained, including a wrapping addition to zero.
    pub deadline_seconds: i32,
    pub retry_pending: bool,
    pub stopped: bool,
}

#[derive(Debug, Default)]
pub struct FrontendAudio {
    state: MusicState,
}

impl FrontendAudio {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> MusicState {
        self.state
    }

    /// 00487310: release the old selection, then store argument+1. The
    /// frontend phase helper 00487360 supplies 7, selecting disc track 8.
    /// Selection itself neither opens the CD device nor starts playback.
    pub fn select_track<B: FrontendAudioBackend>(
        &mut self,
        native_argument: u32,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        self.ensure_running()?;
        self.release(backend)?;
        let track_word = native_argument.wrapping_add(1);
        // 004F60B0 over [1, track]: the word at +0, its low byte the saved
        // TMSF +4.
        self.state.selection = Some(MusicSelection {
            track_word,
            saved_tmsf: track_word & 0xff,
        });
        Ok(())
    }

    /// 00487230 -> 004F7500. Save position before stop/close; keep the selected
    /// track, retry flag and deadline for the later 00487370 resume call.
    pub fn pause<B: FrontendAudioBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        self.ensure_running()?;
        if self.state.selection.is_none() {
            return Ok(());
        }
        self.refresh_active(backend)?;
        if self.state.active_selection {
            let result = backend.position_tmsf();
            let position = self.effect(AudioOperation::Position, result)?;
            self.state.selection.as_mut().unwrap().saved_tmsf = position;
            self.stop_and_close(backend)?;
        }
        Ok(())
    }

    /// 004872A0, also reached by the exact JMP thunk at 00487370. Volume is
    /// the shared configuration word at 0054E8A8+4, not a cached owner setting.
    pub fn resume<B: FrontendAudioBackend>(
        &mut self,
        volume_word: &mut u32,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        self.ensure_running()?;
        if self.state.selection.is_none() {
            return Ok(());
        }
        if *volume_word == 0 {
            return self.pause(backend);
        }
        let expanded = expanded_volume(*volume_word);
        let result = backend.set_cd_volume(expanded | expanded.wrapping_shl(16));
        self.effect(AudioOperation::Volume, result)?;
        let result = self.play_selected(backend);
        if result.is_err() {
            *volume_word = 0;
        }
        result
    }

    /// 00463B00's tail after its 00453B80 adjust of the same word. The linear
    /// level is computed BEFORE 00487370 resumes, because a failed native
    /// play clears the word; 004F7D80 then duplicates that level into both
    /// AUX channels whether or not a selection exists. Native ignores the
    /// auxSetVolume results; a Rust backend failure stops the owner, like
    /// the resume path's own volume write.
    pub fn apply_volume_setting<B: FrontendAudioBackend>(
        &mut self,
        volume_word: &mut u32,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        let level = music_aux_level(*volume_word);
        self.resume(volume_word, backend)?;
        let result = backend.set_cd_volume(level | level.wrapping_shl(16));
        self.effect(AudioOperation::Volume, result)
    }

    /// 00487B90 -> 004F7E20/60: strict signed deadline check, then clear retry
    /// BEFORE another play attempt. Service never reapplies volume.
    pub fn service<B: FrontendAudioBackend>(
        &mut self,
        volume_word: &mut u32,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        self.ensure_running()?;
        if self.state.selection.is_none() || *volume_word == 0 {
            return Ok(());
        }
        if self.state.deadline_seconds != 0 {
            let result = backend.clock_seconds();
            let now = self.effect(AudioOperation::Clock, result)?;
            if self.state.deadline_seconds < now {
                self.state.deadline_seconds = 0;
                self.state.retry_pending = true;
            }
        }
        if !self.state.retry_pending {
            return Ok(());
        }
        self.state.retry_pending = false;
        let result = self.play_selected(backend);
        if result.is_err() {
            *volume_word = 0;
        }
        result
    }

    /// 004F7DE0: native MCI notification values, not scheduler activity codes.
    /// 2 is recognized without clearing or setting an existing retry flag.
    pub fn notify(&mut self, native_code: u32) -> bool {
        match native_code {
            1 | 4 | 8 => {
                self.state.retry_pending = true;
                true
            }
            2 => true,
            _ => false,
        }
    }

    /// 004F60E0 / session teardown: reset position, stop/close if still active,
    /// and release the selection. The process retry/deadline globals survive.
    pub fn release<B: FrontendAudioBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        self.ensure_running()?;
        if self.state.selection.is_none() {
            return Ok(());
        }
        self.refresh_active(backend)?;
        self.reset_position();
        if self.state.active_selection {
            self.stop_and_close(backend)?;
        }
        self.state.selection = None;
        Ok(())
    }

    fn ensure_running<E>(&self) -> Result<(), FrontendAudioError<E>> {
        if self.state.stopped {
            Err(FrontendAudioError::Stopped)
        } else {
            Ok(())
        }
    }

    fn effect<T, E>(
        &mut self,
        operation: AudioOperation,
        result: Result<T, E>,
    ) -> Result<T, FrontendAudioError<E>> {
        result.map_err(|error| {
            self.state.stopped = true;
            FrontendAudioError::Backend { operation, error }
        })
    }

    fn ensure_open<B: FrontendAudioBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        if !self.state.device_open {
            let result = backend.open_cd();
            self.effect(AudioOperation::Open, result)?;
            let result = backend.set_tmsf_format();
            if let Err(error) = result {
                // 004F5DF0 closes the just-opened device on format failure;
                // the first failure remains the reported one.
                let _ = backend.close_cd();
                return self.effect(AudioOperation::SetTimeFormat, Err(error));
            }
            self.state.device_open = true;
        }
        Ok(())
    }

    fn reset_position(&mut self) {
        if let Some(selection) = &mut self.state.selection {
            selection.saved_tmsf = selection.track_word & 0xff;
        }
    }

    fn refresh_active<B: FrontendAudioBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        if self.state.active_selection {
            self.ensure_open(backend)?;
            let result = backend.playback_mode();
            if self.effect(AudioOperation::Mode, result)? != CdPlaybackMode::Playing {
                self.reset_position();
                self.state.active_selection = false;
            }
        }
        Ok(())
    }

    fn stop_and_close<B: FrontendAudioBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        let result = backend.stop_cd();
        self.effect(AudioOperation::Stop, result)?;
        let result = backend.close_cd();
        self.effect(AudioOperation::Close, result)?;
        self.state.device_open = false;
        self.state.active_selection = false;
        Ok(())
    }

    fn play_selected<B: FrontendAudioBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), FrontendAudioError<B::Error>> {
        // 004F68A0 probes both availability queries even if media_present=false.
        self.state.audio_media_present = false;
        self.ensure_open(backend)?;
        let result = backend.media_present();
        self.state.audio_media_present = self.effect(AudioOperation::MediaPresent, result)?;
        let result = backend.track_two_is_audio();
        if !self.effect(AudioOperation::TrackTwoType, result)? {
            self.state.audio_media_present = false;
        }
        if !self.state.audio_media_present {
            return Ok(());
        }
        self.refresh_active(backend)?;
        if self.state.active_selection {
            return Ok(());
        }
        let selection = self
            .state
            .selection
            .expect("play requires a selected track");
        let duration_seconds = match backend.track_length_msf(selection.track_word) {
            Ok(msf) => ((msf & 0xff) * 60 + ((msf >> 8) & 0xff) + 2) as i32,
            Err(_) => 3600,
        };
        let result = backend.play_range(CdPlayRange {
            from_tmsf: selection.saved_tmsf,
            to_tmsf: u32::from((selection.track_word as u8).wrapping_add(1)),
        });
        self.effect(AudioOperation::Play, result)?;
        self.state.active_selection = true;
        self.refresh_active(backend)?;
        if self.state.active_selection {
            let result = backend.clock_seconds();
            let now = self.effect(AudioOperation::Clock, result)?;
            self.state.deadline_seconds = now.wrapping_add(duration_seconds);
        } else {
            self.state.retry_pending = true;
        }
        Ok(())
    }
}

/// Volume nibble expansion `v | v<<4 | v<<8 | v<<12` over the full DWORD, as
/// 004872A0 computes the AUX word and the 004F4780 callers (00438768 session
/// construction, 00463AC0, 00487B59) compute the FX master that 004F4455 cue
/// gain later reads from 0065ECC4. Out-of-range words keep their OR-merged
/// high bits; nothing narrows or clamps them.
pub fn expanded_volume(word: u32) -> u32 {
    word | word.wrapping_shl(4) | word.wrapping_shl(8) | word.wrapping_shl(12)
}

/// 00463B00's linear AUX level: wrapping `word * 65535` (`shl 16; sub`),
/// then signed division by 15 truncating toward zero. Below the multiply wrap
/// it is `word * 0x1111`, so it matches the nibble expansion for 0..=15 and
/// for other words whose shifted copies share no set bits; overlapping copies
/// (17) or a wrapped multiply (0x10000) diverge.
pub fn music_aux_level(word: u32) -> u32 {
    (word.wrapping_mul(65535) as i32 / 15) as u32
}

// Native 004923C0 destroys returned error objects and several CD paths continue.
// Apart from the proven length-query fallback, Rust stops on a backend failure
// with completed effects retained. This is explicit containment, not a claim
// to reproduce every MCI error/recovery path or release a failed backend handle.

#[cfg(test)]
#[path = "frontend_audio_tests.rs"]
mod tests;
