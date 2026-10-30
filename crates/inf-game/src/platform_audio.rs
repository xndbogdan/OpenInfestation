//! SDL playback for the recovered frontend music and H9 cue owners.
//!
//! Ogg files replace the CD transport, retaining disc track numbers and TMSF
//! positions. The frontend music owner still decides when to retry/loop. SDL
//! mixing/resampling is an explicit portable backend policy, not a claim of
//! sample-identical DirectSound output.

use std::{
    collections::BTreeMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use inf_formats::{FrontendExecutable, SoundEntryKind};
use lewton::inside_ogg::OggStreamReader;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use thiserror::Error;

use crate::{
    frontend_assets::FrontendAssets,
    frontend_audio::{CdPlayRange, CdPlaybackMode, FrontendAudioBackend},
    HandlerResources, LoadedOverlay, ResidentRuntime,
};

pub(crate) const OUTPUT_RATE: u32 = 44_100;
mod lifecycle;
pub mod remastered_pak;
pub mod reverb;
// Original INDEX1 lengths; authority: the retail disc's track table.
const TRACK_SECTORS: [u32; 8] = [9351, 18023, 20251, 18088, 19412, 16191, 16706, 9518];
const SAMPLES_PER_SECTOR: usize = 588;

#[derive(Debug, Error)]
pub enum PlatformAudioError {
    #[error("SDL audio device: {0}")]
    Device(String),
    #[error("cannot read audio {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot decode Ogg {path}: {reason}")]
    Decode { path: PathBuf, reason: String },
    #[error("audio track {track} must be 44,100 Hz stereo with {expected} frames, found {actual}")]
    TrackFormat {
        track: u32,
        expected: usize,
        actual: usize,
    },
    #[error("unsupported original CD track {0}; installed disc tracks are 2 through 9")]
    Track(u32),
    #[error("CD transport has not been opened and configured for TMSF")]
    CdClosed,
    #[error("unsupported CD play range {0:?}")]
    Range(CdPlayRange),
    #[error("frontend sound parameter table: {0}")]
    Parameters(String),
    #[error("H9 sound slot {0} is not resident")]
    MissingSound(i16),
    #[error("H9 slot {0} has an unsupported or malformed resident record")]
    SoundRecord(i16),
    #[error("H9 alias at {0} crosses allocations; load-time pointer custody is not implemented")]
    AliasCustody(i16),
    #[error("H9 alias cycle at slot {0}")]
    AliasCycle(i16),
    #[error("system clock is before the Unix epoch: {0}")]
    Clock(String),
    #[error(transparent)]
    Voice(#[from] crate::process_sound::VoiceExecutionError<crate::process_sound::VoicePoolFault>),
    #[error("empty or non-WORD PCM cannot create a native sound buffer")]
    InvalidPcm,
    #[error("sound lifecycle: {0}")]
    Lifecycle(String),
    #[error(transparent)]
    Record(#[from] crate::process_sound::ProcessSoundRecordError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueReceipt {
    Disabled {
        requested_slot: i16,
    },
    Capacity2202 {
        requested_slot: i16,
    },
    Started {
        requested_slot: i16,
        pcm_slot: i16,
        sample_count: usize,
        frequency_hz: u32,
        attenuation_hundredth_db: i32,
        alias_count: usize,
    },
}

/// The buses' levels over a stretch (the ambience's event-log lines):
/// retail's effects, its music and title, and the ambience, each in dBFS
/// RMS, and the ambience's deepest dip under the effects (a share).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BusLevels {
    pub effects: f32,
    pub music: f32,
    pub ambience: f32,
    pub dip: f32,
}

/// The sums [`BusLevels`] come from.
#[derive(Clone, Copy, Debug, Default)]
struct LevelSums {
    effects: f64,
    music: f64,
    ambience: f64,
    frames: u64,
    dip: f32,
}

impl LevelSums {
    fn levels(&self) -> Option<BusLevels> {
        let decibels = |sum: f64| {
            let mean = sum / (2 * self.frames.max(1)) as f64;
            if mean > 0.0 {
                (10.0 * mean.log10()) as f32
            } else {
                -120.0
            }
        };
        (self.frames > 0).then(|| BusLevels {
            effects: decibels(self.effects),
            music: decibels(self.music),
            ambience: decibels(self.ambience),
            dip: self.dip,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioPlaybackStats {
    pub output_frames: u64,
    pub music_frames: u64,
    pub effect_frames: u64,
    pub title_frames: u64,
    pub peak: f32,
    pub cue_requests: u64,
    pub cues_started: u64,
    pub music_starts: u64,
}

struct MusicVoice {
    track: u8,
    samples: Arc<[i16]>,
    frame: usize,
    playing: bool,
}

struct EffectVoice {
    voice: crate::process_sound::ProcessSoundVoiceRef,
    playing: bool,
    looping: bool,
    samples: Arc<[i16]>,
    position: f64,
    step: f64,
    attenuation_hundredth_db: i32,
    pan: i32,
    /// Retail's gains for the attenuation and pan they were worked out for.
    gains: Option<(i32, i32, f32, [f32; 2])>,
    /// Remastered's placement of a positional sound (Ambience above 0), and
    /// its low-pass's last output, and its ears with headphones.
    placed: Option<PlacedEffect>,
    low: f32,
    ears: Ears,
}

/// A placed sound reaching each ear, for headphones (a cheap model,
/// no measured set): the far ear later, by up to
/// 0.66 ms, and in the head's shadow, losing its highs and 3 dB; the near
/// ear 2 dB up; behind, both ears duller.
#[derive(Clone, Debug, Default)]
struct Ears {
    history: [f32; EAR_HISTORY],
    at: usize,
    shadow: f32,
    rear: [f32; 2],
}

const EAR_HISTORY: usize = 32;
/// The far ear's lag at the side, in frames (0.66 ms).
const EAR_DELAY: f32 = 29.0;

impl Ears {
    /// Each ear's share of mono `sample` from `aim`: its share to the right
    /// (-1 left to 1 right) and ahead (-1 behind to 1 ahead).
    fn hear(&mut self, sample: f32, [side, ahead]: [f32; 2]) -> [f32; 2] {
        self.history[self.at] = sample;
        let away = side.abs().min(1.0);
        let lag = away * EAR_DELAY;
        let whole = lag.floor() as usize;
        let read = |back: usize| self.history[(self.at + EAR_HISTORY - back) % EAR_HISTORY];
        let late = read(whole) + (read(whole + 1) - read(whole)) * (lag - whole as f32);
        self.at = (self.at + 1) % EAR_HISTORY;
        // The head's shadow: down to about 2 kHz at the side.
        self.shadow += (late - self.shadow) * (1.0 - 0.75 * away);
        let far = self.shadow * (1.0 - 0.3 * away);
        // The near ear facing the sound gains a little (the head's baffle),
        // so a sound keeps its loudness as the head turns.
        let near = sample * (1.0 + 0.3 * away);
        let mut ears = if side >= 0.0 {
            [far, near]
        } else {
            [near, far]
        };
        // Behind, the ear's own shadow: both duller.
        let share = 1.0 - 0.6 * (-ahead).clamp(0.0, 1.0);
        for (ear, rear) in ears.iter_mut().zip(&mut self.rear) {
            *rear += (*ear - *rear) * share;
            *ear = *rear;
        }
        ears
    }
}

/// Remastered's placement of one of retail's positional sounds: panned by
/// its direction at constant power (as loud as retail's in the middle),
/// duller with distance as the ambience's are, muffled deep under the water.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PlacedEffect {
    amplitudes: [f32; 2],
    /// Its share to the right and ahead, for the headphones' ears.
    aim: [f32; 2],
    /// The one-pole low-pass's share of each new sample.
    cut: f32,
    /// Beyond the vehicle: in first person, heard through its shell.
    outside: bool,
}

impl PlacedEffect {
    fn new(placement: crate::process_sound::SoundPlacement) -> Self {
        let [x, y, z] = placement.rotated.map(f32::from);
        let distance = (x * x + y * y + z * z).sqrt();
        let side = if distance > 0.0 {
            (x / distance).clamp(-1.0, 1.0) * (distance / PLACED_NEAR).min(1.0)
        } else {
            0.0
        };
        let angle = (side + 1.0) * std::f32::consts::FRAC_PI_4;
        let amplitudes = [angle.cos(), angle.sin()].map(|share| share * std::f32::consts::SQRT_2);
        let far = ((distance - 256.0) / (8192.0 - 256.0)).clamp(0.0, 1.0);
        let cutoff = if placement.under_water {
            PLACED_UNDER_WATER
        } else {
            16_000.0 * (3_000.0_f32 / 16_000.0).powf(far)
        };
        let cut = 1.0 - (-std::f32::consts::TAU * cutoff / OUTPUT_RATE as f32).exp();
        let ahead = if distance > 0.0 { z / distance } else { 1.0 };
        Self {
            amplitudes,
            aim: [side, ahead],
            cut,
            outside: distance > PLACED_NEAR,
        }
    }
}

/// Nearer than this a placed sound of retail's pans toward the middle; under
/// the water it keeps what is below this many hertz.
const PLACED_NEAR: f32 = 300.0;
const PLACED_UNDER_WATER: f32 = 400.0;
/// Heard through the vehicle's shell (first person): 2 dB down, and what is
/// below about 5 kHz (the one-pole low-pass's share of each new sample).
const SHELL_GAIN: f32 = 0.794;
const SHELL_CUT: f32 = 0.51;
/// Port option, the remastered camera under the water: the game heard
/// through it keeps what is below this many hertz, this much of its level,
/// and comes and goes over this many seconds. The music stays clear.
const UNDER_WATER_HERTZ: f32 = 600.0;
const UNDER_WATER_GAIN: f32 = 0.8;
const UNDER_WATER_SECONDS: f32 = 0.2;

impl EffectVoice {
    /// The voice's volume and its pan's left and right amplitudes, worked
    /// out again only when they change rather than for every sample: the
    /// same numbers.
    fn gains(&mut self) -> (f32, [f32; 2]) {
        let key = (self.attenuation_hundredth_db, self.pan);
        match self.gains {
            Some((attenuation, pan, volume, amplitudes)) if (attenuation, pan) == key => {
                (volume, amplitudes)
            }
            _ => {
                let volume = ds_amplitude(self.attenuation_hundredth_db);
                // DirectSound pan is attenuation of the opposite channel.
                let amplitudes = [
                    ds_amplitude(-self.pan.max(0)),
                    ds_amplitude(self.pan.min(0)),
                ];
                self.gains = Some((key.0, key.1, volume, amplitudes));
                (volume, amplitudes)
            }
        }
    }
}

/// Title-video sound: stereo 22050 Hz 16-bit interleaved frames from the
/// AVI `01wb` audio, resampled to the 44100 Hz mixer. Unity gain is an
/// explicit portable policy: no retail title-volume consumer is recovered.
struct TitleVoice {
    samples: Arc<[i16]>,
    position: f64,
}

/// `Title.avi` audio input rate in Hz; the mixer runs at `OUTPUT_RATE`.
const TITLE_INPUT_RATE_HZ: f64 = 22_050.0;
const TITLE_OUTPUT_STEP: f64 = TITLE_INPUT_RATE_HZ / OUTPUT_RATE as f64;

/// Session-facing contract for title-video sound: the graph starts sound
/// with playback and stops it on teardown (`00494DC0`). Replaces any
/// playing title audio; a trailing orphan half-frame is ignored.
pub trait TitleAudioSink {
    fn play_title_audio(&mut self, frames: Vec<i16>);
    fn stop_title_audio(&mut self);
}

impl TitleAudioSink for PlatformAudio {
    fn play_title_audio(&mut self, frames: Vec<i16>) {
        self.device.lock().title = Some(TitleVoice {
            samples: frames.into(),
            position: 0.0,
        });
    }

    fn stop_title_audio(&mut self) {
        self.device.lock().title = None;
    }
}

/// A voice of the port's ambience bus: a sound
/// from `Remastered.pak`, never retail's pool. A looping voice crossfades
/// its end into its start, so a recording needs no loop point; its gain and
/// pan ramp, and a stopped voice fades out before it goes.
struct AmbienceVoice {
    id: u32,
    sound: Arc<remastered_pak::DecodedSound>,
    position: usize,
    looping: bool,
    gain: f32,
    target: f32,
    /// -1 left to 1 right, for a mono sound; a stereo one keeps its own.
    pan: f32,
    target_pan: f32,
    stopping: bool,
    /// The sound's frames a mix frame (1 as recorded), and how far between
    /// `position` and the next frame the voice is.
    step: f32,
    target_step: f32,
    fraction: f32,
    /// A one-pole low-pass's share of each new sample (1: open, no filter)
    /// and its last output: a far sound loses its highs.
    cut: f32,
    target_cut: f32,
    low: [f32; 2],
    /// The share of its output sent to the reverb.
    send: f32,
    /// A placed voice's share to the right and ahead (none for the beds and
    /// the sweeteners), moving toward the target; its ears with headphones.
    aim: Option<[f32; 2]>,
    target_aim: Option<[f32; 2]>,
    ears: Ears,
    /// An effect of the port's (a crystal's ring): as loud as the Sound
    /// slider sets retail's effects, not the Ambience slider, and never
    /// dipped under them, being one.
    effect: bool,
}

/// How far an ambience voice's gain and pan move a frame: across their
/// whole range in about one 23 ms buffer, so nothing clicks.
const AMBIENCE_RAMP: f32 = 1.0 / 1024.0;
/// The longest crossfade a looping ambience voice takes at its seam
/// (frames: 1.5 s), at most a quarter of the sound.
const AMBIENCE_CROSSFADE: usize = 66_150;
/// The limiter's ceiling (full scale: only what would clip is touched), how
/// fast it lets go (a frame's share: back from half in 0.05 s), and how far
/// it looks ahead (frames: 1.5 ms, the output's delay).
const LIMITER_CEILING: f32 = 1.0;
const LIMITER_RELEASE: f32 = 1.0 / 4410.0;
const LIMITER_AHEAD: usize = 64;

/// A port fix over the whole mix, in every renderer (2026-10-27: a limiter
/// so sound does not distort, where the mix used to be cut at full scale): with more voices than retail's 24, loud moments
/// summed past it and clipped. It looks [LIMITER_AHEAD] frames ahead: each
/// frame's gain is the average over that many frames of the lowest gain the
/// next ones need, so it eases down to exactly what a peak needs as the peak
/// arrives, and never lets one past the ceiling. A mix that never reaches
/// full scale passes untouched, only delayed.
struct Limiter {
    /// The frames waiting to go out, and the gain each needs.
    delay: [[f32; 2]; LIMITER_AHEAD],
    need: [f32; LIMITER_AHEAD],
    /// The lowest need over each of the last windows, to average.
    lowest: [f32; LIMITER_AHEAD],
    at: usize,
    gain: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self {
            delay: [[0.0; 2]; LIMITER_AHEAD],
            need: [1.0; LIMITER_AHEAD],
            lowest: [1.0; LIMITER_AHEAD],
            at: 0,
            gain: 1.0,
        }
    }
}

impl Limiter {
    /// Take `frame` in and give back the one [`LIMITER_AHEAD`] - 1 frames
    /// before it, limited.
    fn next(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let peak = frame[0].abs().max(frame[1].abs());
        self.need[self.at] = if peak > LIMITER_CEILING {
            LIMITER_CEILING / peak
        } else {
            1.0
        };
        self.delay[self.at] = frame;
        self.lowest[self.at] = self.need.iter().copied().fold(1.0, f32::min);
        self.at = (self.at + 1) % LIMITER_AHEAD;
        let wanted = self.lowest.iter().sum::<f32>() / LIMITER_AHEAD as f32;
        self.gain = wanted.min(self.gain + LIMITER_RELEASE);
        let [left, right] = self.delay[self.at];
        [left * self.gain, right * self.gain]
    }
}
/// The ambience's dip under retail's effects: from this level of theirs
/// (-8 dBFS: a blast, not the machines' steady hum), this much per unit
/// above it, at most this share (6 dB, at full scale); let go over about
/// 0.8 s from full scale (a frame's share).
const DUCK_FLOOR: f32 = 0.4;
const DUCK_SLOPE: f32 = 0.85;
const DUCK_MOST: f32 = 0.5;
const DUCK_RELEASE: f32 = 0.999_974;

impl AmbienceVoice {
    /// The voice's next frame, left and right, before the bus volume.
    fn next(&mut self, headphones: bool) -> Option<[f32; 2]> {
        let frames = self.sound.frames();
        let channels = usize::from(self.sound.channels.max(1));
        if self.position >= frames {
            return None;
        }
        let at = |frame: usize, channel: usize| {
            f32::from(self.sound.samples[frame * channels + channel.min(channels - 1)]) / 32768.0
        };
        let fade = (frames / 4).min(AMBIENCE_CROSSFADE).max(1);
        let seam = frames - fade;
        let sample = |position: usize, channel: usize| {
            if self.looping && position >= seam {
                // Equal power: the tail and the head are different sound.
                let t = (position - seam) as f32 / fade as f32 * std::f32::consts::FRAC_PI_2;
                at(position, channel) * t.cos() + at(position - seam, channel) * t.sin()
            } else {
                at(position, channel)
            }
        };
        let [mut left, mut right] = [sample(self.position, 0), sample(self.position, 1)];
        if self.fraction > 0.0 {
            // Between two frames: the next one's share, past the end the
            // head again (looping) or the last frame.
            let next = match self.position + 1 {
                next if next < frames => next,
                _ if self.looping => fade,
                _ => self.position,
            };
            let share = self.fraction;
            left += (sample(next, 0) - left) * share;
            right += (sample(next, 1) - right) * share;
        }
        self.gain += (self.target - self.gain).clamp(-AMBIENCE_RAMP, AMBIENCE_RAMP);
        self.pan += (self.target_pan - self.pan).clamp(-AMBIENCE_RAMP, AMBIENCE_RAMP);
        self.step += (self.target_step - self.step).clamp(-AMBIENCE_RAMP, AMBIENCE_RAMP);
        self.cut += (self.target_cut - self.cut).clamp(-AMBIENCE_RAMP, AMBIENCE_RAMP);
        if self.cut < 1.0 {
            self.low[0] += (left - self.low[0]) * self.cut;
            self.low[1] += (right - self.low[1]) * self.cut;
            [left, right] = self.low;
        }
        if let (Some(aim), Some(target)) = (&mut self.aim, self.target_aim) {
            for (now, wanted) in aim.iter_mut().zip(target) {
                *now += (wanted - *now).clamp(-AMBIENCE_RAMP, AMBIENCE_RAMP);
            }
        }
        match self.aim.filter(|_| headphones && channels == 1) {
            // Each ear its own sound, as loud as the pan's in the middle.
            Some(aim) => {
                let [l, r] = self.ears.hear(left, aim);
                (left, right) = (
                    l * std::f32::consts::FRAC_1_SQRT_2,
                    r * std::f32::consts::FRAC_1_SQRT_2,
                );
            }
            None if channels == 1 => {
                let angle = (self.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                (left, right) = (left * angle.cos(), right * angle.sin());
            }
            None => {}
        }
        self.fraction += self.step;
        let whole = self.fraction.floor();
        self.fraction -= whole;
        self.position += whole as usize;
        if self.looping && self.position >= frames {
            // The head played as the seam's fade-in.
            self.position = fade + (self.position - frames);
        }
        Some([left * self.gain, right * self.gain])
    }

    fn finished(&self) -> bool {
        (self.stopping && self.gain <= 0.0) || self.position >= self.sound.frames()
    }
}

#[derive(Default)]
struct Mixer {
    capture: Option<crate::session_capture::CaptureHandle>,
    music: Option<MusicVoice>,
    effects: Vec<EffectVoice>,
    title: Option<TitleVoice>,
    cd_volume: [f32; 2],
    effects_volume: u32,
    completion: bool,
    stats: AudioPlaybackStats,
    /// The speakers get silence; the mix, its stats and any capture stay.
    muted: bool,
    /// The ambience bus and its volume (0: off, retail's sound exactly).
    ambience: Vec<AmbienceVoice>,
    ambience_volume: f32,
    /// The whole mix's limiter ([`Limiter`]).
    limiter: Limiter,
    /// How loud retail's effects have been, held and let go slowly.
    duck: f32,
    levels: LevelSums,
    /// The place's reverb, which the ambience's voices send a share to, and
    /// the share retail's placed sounds send.
    reverb: reverb::Reverb,
    retail_send: f32,
    /// In first person the camera is inside the vehicle's shell: the
    /// ambience and retail's placed sounds from outside are 2 dB down and
    /// duller (the bus's low-pass's last output).
    shell: bool,
    shell_low: [f32; 2],
    /// Headphones: the placed sounds reach each ear apart.
    headphones: bool,
    /// The remastered camera is under the water: the game's sound, all but
    /// the music, muffled ([`Mixer::muffle`]); how far the muffling has
    /// come (0 to 1), and the low-pass's last output.
    under_water: bool,
    under_mix: f32,
    under_low: [f32; 2],
    /// The music track hushed (the Alien Brain's beds play instead,
    /// [`crate::ambience::replaces_music`]), and how far, 0 to 1: the voice
    /// still runs its frames, so the CD's position and end stay retail's.
    hushed_track: Option<u8>,
    music_hush: f32,
}

impl Mixer {
    /// `play_range`'s voice: a voice of the hushed track starts hushed.
    fn start_music(&mut self, voice: MusicVoice) {
        self.music_hush = f32::from(u8::from(self.hushed_track == Some(voice.track)));
        self.music = Some(voice);
        self.stats.music_starts += 1;
        self.completion = false;
    }

    /// One frame under the water: the game's sound (`frame` less `music`)
    /// through a one-pole low-pass taking `cut` of each sample, at
    /// [`UNDER_WATER_GAIN`], blended in as the muffling ramps by `step`.
    fn muffle(&mut self, frame: &mut [f32], music: [f32; 2], cut: f32, step: f32) {
        let target = if self.under_water { 1.0 } else { 0.0 };
        self.under_mix += (target - self.under_mix) * step;
        for channel in 0..2 {
            let game = frame[channel] - music[channel];
            self.under_low[channel] += (game - self.under_low[channel]) * cut;
            let muffled = self.under_low[channel] * UNDER_WATER_GAIN;
            frame[channel] = music[channel] + game + (muffled - game) * self.under_mix;
        }
    }
}

impl AudioCallback for Mixer {
    type Channel = f32;

    fn callback(&mut self, output: &mut [f32]) {
        let capture_time = self.capture.as_ref().map(|capture| capture.elapsed_ns());
        output.fill(0.0);
        let rate = OUTPUT_RATE as f32;
        let under_cut = 1.0 - (-std::f32::consts::TAU * UNDER_WATER_HERTZ / rate).exp();
        let under_step = 1.0 - (-1.0 / (UNDER_WATER_SECONDS * rate)).exp();
        for frame in output.chunks_exact_mut(2) {
            if let Some(music) = &mut self.music {
                if music.playing {
                    let hush = f32::from(u8::from(self.hushed_track == Some(music.track)));
                    self.music_hush +=
                        (hush - self.music_hush).clamp(-AMBIENCE_RAMP, AMBIENCE_RAMP);
                    if music.frame < music.samples.len() / 2 {
                        for channel in 0..2 {
                            frame[channel] += f32::from(music.samples[music.frame * 2 + channel])
                                / 32768.0
                                * self.cd_volume[channel]
                                * (1.0 - self.music_hush);
                        }
                        music.frame += 1;
                        self.stats.music_frames += 1;
                    }
                    if music.frame == music.samples.len() / 2 {
                        music.playing = false;
                        self.completion = true;
                    }
                }
            }
            // Retail's effects alone, for the ambience to duck under; under
            // Remastered, placed by direction, and sending to the reverb.
            let mut effects = [0.0_f32; 2];
            let remastered = self.ambience_volume > 0.0;
            let mut retail_send = 0.0;
            for voice in &mut self.effects {
                if !voice.playing {
                    continue;
                }
                if voice.looping && !voice.samples.is_empty() {
                    voice.position %= voice.samples.len() as f64;
                }
                let index = voice.position as usize;
                if index >= voice.samples.len() {
                    voice.playing = false;
                    continue;
                }
                let a = f64::from(voice.samples[index]);
                let b = f64::from(*voice.samples.get(index + 1).unwrap_or(if voice.looping {
                    &voice.samples[0]
                } else {
                    &voice.samples[index]
                }));
                let mut sample = (a + (b - a) * voice.position.fract()) as f32 / 32768.0;
                let (volume, [mut left, mut right]) = voice.gains();
                if let (true, Some(placed)) = (remastered, voice.placed) {
                    let shell = self.shell && placed.outside;
                    let cut = if shell {
                        placed.cut.min(SHELL_CUT)
                    } else {
                        placed.cut
                    };
                    voice.low += (sample - voice.low) * cut;
                    sample = voice.low * if shell { SHELL_GAIN } else { 1.0 };
                    [left, right] = placed.amplitudes;
                    retail_send += sample * volume * self.retail_send;
                    if self.headphones {
                        // Each ear its own sound: as loud as retail's in the
                        // middle.
                        let [l, r] = voice.ears.hear(sample, placed.aim);
                        (sample, left, right) = (1.0, l, r);
                    }
                }
                let share = [sample * volume * left, sample * volume * right];
                frame[0] += share[0];
                frame[1] += share[1];
                effects[0] += share[0];
                effects[1] += share[1];
                voice.position += voice.step;
                self.stats.effect_frames += 1;
            }
            let mut ambience = [0.0_f32; 2];
            let exhausted = if let Some(title) = &mut self.title {
                let frames = title.samples.len() / 2;
                let index = title.position as usize;
                if index < frames {
                    let frac = title.position.fract();
                    for channel in 0..2 {
                        let a = f64::from(title.samples[index * 2 + channel]);
                        let b = f64::from(
                            *title
                                .samples
                                .get((index + 1) * 2 + channel)
                                .unwrap_or(&title.samples[index * 2 + channel]),
                        );
                        frame[channel] += ((a + (b - a) * frac) as f32) / 32768.0;
                    }
                    title.position += TITLE_OUTPUT_STEP;
                    self.stats.title_frames += 1;
                }
                title.position as usize >= frames
            } else {
                false
            };
            if exhausted {
                self.title = None;
            }
            // Retail's music and title, before the ambience and the limiter.
            let music = [frame[0] - effects[0], frame[1] - effects[1]];
            if self.ambience_volume > 0.0 && !self.ambience.is_empty() {
                // The bed dips under retail's loud effects (a blast near
                // by), at once, and comes back over about 0.8 s.
                let loud = effects[0].abs().max(effects[1].abs());
                self.duck = loud.max(self.duck * DUCK_RELEASE);
                let dip = ((self.duck - DUCK_FLOOR).max(0.0) * DUCK_SLOPE).min(DUCK_MOST);
                self.levels.dip = self.levels.dip.max(dip);
                let volume = self.ambience_volume * (1.0 - dip);
                let effect_volume = ds_amplitude(attenuation(65536, self.effects_volume));
                let mut send = retail_send;
                for voice in &mut self.ambience {
                    let volume = if voice.effect { effect_volume } else { volume };
                    if let Some([left, right]) = voice.next(self.headphones) {
                        ambience[0] += left * volume;
                        ambience[1] += right * volume;
                        send += (left + right) * 0.5 * voice.send * volume;
                    }
                }
                let [left, right] = self.reverb.next(send);
                ambience[0] += left;
                ambience[1] += right;
                if self.shell {
                    for channel in 0..2 {
                        self.shell_low[channel] +=
                            (ambience[channel] - self.shell_low[channel]) * SHELL_CUT;
                        ambience[channel] = self.shell_low[channel] * SHELL_GAIN;
                    }
                }
                frame[0] += ambience[0];
                frame[1] += ambience[1];
            }
            self.muffle(frame, music, under_cut, under_step);
            let limited = self.limiter.next([frame[0], frame[1]]);
            frame.copy_from_slice(&limited);
            let power = |pair: [f32; 2]| f64::from(pair[0] * pair[0] + pair[1] * pair[1]);
            self.levels.effects += power(effects);
            self.levels.music += power(music);
            self.levels.ambience += power(ambience);
            self.levels.frames += 1;
            for sample in frame {
                *sample = sample.clamp(-1.0, 1.0);
                self.stats.peak = self.stats.peak.max(sample.abs());
            }
            self.stats.output_frames += 1;
        }
        self.ambience.retain(|voice| !voice.finished());
        // SDL completion changes playback only. Native4F4100 owns logical removal.
        if let (Some(capture), Some(time)) = (&self.capture, capture_time) {
            capture.audio(time, output);
        }
        if self.muted {
            output.fill(0.0);
        }
    }
}

/// One live SDL device, shared by CD music and mono H9 sound effects. Dropping
/// it closes the physical device; closing the logical CD leaves effects alive.
pub struct PlatformAudio {
    device: AudioDevice<Mixer>,
    voices: crate::process_sound::ProcessSoundVoices,
    lifecycle: lifecycle::PlatformSoundLifecycle,
    pcm_sources: PreparedPcmSources,
    directory: PathBuf,
    tracks: BTreeMap<u32, Arc<[i16]>>,
    cue_parameters: [u32; 4],
    cd_open: bool,
    tmsf: bool,
    /// The port's sounds, from `Remastered.pak` beside the game's data;
    /// none without it, when the remastered sounds stay off.
    remastered: Option<remastered_pak::RemasteredPak>,
}

impl PlatformAudio {
    /// The process voice pool's limits: all voices and the splashes'
    /// ([`crate::process_sound::ProcessSoundVoices::set_limit`]).
    pub fn set_voice_limits(&mut self, limit: usize, splashes: Option<usize>) {
        self.voices.set_limit(limit);
        self.voices.set_splash_limit(splashes);
    }

    pub(crate) fn set_capture(&mut self, capture: Option<crate::session_capture::CaptureHandle>) {
        self.device.lock().capture = capture;
    }

    /// Silence the speakers (a hidden run, or a window without focus), the
    /// mix and its stats unchanged.
    pub fn set_muted(&mut self, muted: bool) {
        self.device.lock().muted = muted;
    }

    /// Mix from the frames (`--audio-out`): the device's own thread
    /// pauses, and [`Self::render`] mixes instead.
    pub fn set_offline(&mut self, offline: bool) {
        if offline {
            self.device.pause();
        } else {
            self.device.resume();
        }
    }

    /// The next `frames` of the mix, left and right, unmuted.
    pub fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut mixer = self.device.lock();
        let muted = std::mem::replace(&mut mixer.muted, false);
        let mut output = vec![0.0; frames * 2];
        mixer.callback(&mut output);
        mixer.muted = muted;
        output
    }

    /// The Ambience volume, 0 to 15 on retail's own curve (the Sound
    /// slider's): 0 silences the ambience bus.
    pub fn set_ambience_volume(&mut self, level: u8) {
        self.device.lock().ambience_volume = ambience_gain(level);
    }

    /// The port's sounds (`Remastered.pak`), when they were found.
    pub fn remastered(&self) -> Option<&remastered_pak::RemasteredPak> {
        self.remastered.as_ref()
    }

    /// Start ambience voice `id`, replacing one of that id, its gain and
    /// pan ramping in from silence.
    pub fn play_ambience(
        &mut self,
        id: u32,
        sound: Arc<remastered_pak::DecodedSound>,
        looping: bool,
        gain: f32,
        pan: f32,
    ) {
        self.play_ambience_from(id, sound, looping, gain, pan, 0);
    }

    /// [`Self::play_ambience`] from frame `start` of the sound.
    pub fn play_ambience_from(
        &mut self,
        id: u32,
        sound: Arc<remastered_pak::DecodedSound>,
        looping: bool,
        gain: f32,
        pan: f32,
        start: usize,
    ) {
        let mut mixer = self.device.lock();
        mixer.ambience.retain(|voice| voice.id != id);
        mixer.ambience.push(AmbienceVoice {
            id,
            position: start.min(sound.frames().saturating_sub(1)),
            sound,
            looping,
            gain: 0.0,
            target: gain.max(0.0),
            pan,
            target_pan: pan,
            stopping: false,
            step: 1.0,
            target_step: 1.0,
            fraction: 0.0,
            cut: 1.0,
            target_cut: 1.0,
            low: [0.0; 2],
            send: 0.0,
            aim: None,
            target_aim: None,
            ears: Ears::default(),
            effect: false,
        });
    }

    /// Make ambience voice `id` an effect of the port's, at the effects'
    /// volume and never dipped ([`AmbienceVoice::effect`]), starting at
    /// its gain at once as retail's effects do, rather than ramping in.
    pub fn effect_ambience(&mut self, id: u32) {
        for voice in self.device.lock().ambience.iter_mut() {
            if voice.id == id {
                voice.effect = true;
                voice.gain = voice.target;
                voice.pan = voice.target_pan;
            }
        }
    }

    /// Move ambience voice `id`'s gain and pan, ramping.
    pub fn steer_ambience(&mut self, id: u32, gain: f32, pan: f32) {
        for voice in self.device.lock().ambience.iter_mut() {
            if voice.id == id && !voice.stopping {
                voice.target = gain.max(0.0);
                voice.target_pan = pan;
            }
        }
    }

    /// Send `share` of ambience voice `id`'s output to the reverb.
    pub fn send_ambience(&mut self, id: u32, share: f32) {
        for voice in self.device.lock().ambience.iter_mut() {
            if voice.id == id {
                voice.send = share.max(0.0);
            }
        }
    }

    /// Headphones (the Sound menus' row, Vulkan): the placed sounds reach
    /// each ear apart ([`Ears`]); otherwise they pan at constant power.
    pub fn set_headphones(&mut self, on: bool) {
        self.device.lock().headphones = on;
    }

    /// Aim ambience voice `id`, a placed sound: its share to the right (-1
    /// left to 1 right) and ahead (-1 behind to 1 ahead), for headphones.
    pub fn aim_ambience(&mut self, id: u32, aim: [f32; 2]) {
        for voice in self.device.lock().ambience.iter_mut() {
            if voice.id == id {
                voice.target_aim = Some(aim);
                voice.aim.get_or_insert(aim);
            }
        }
    }

    /// Whether the camera is inside the vehicle's shell (first person).
    pub fn set_shell(&mut self, shell: bool) {
        self.device.lock().shell = shell;
    }

    /// Port option: whether the remastered camera is under the water, which
    /// muffles the game's sound (the music stays clear).
    pub fn set_under_water(&mut self, under: bool) {
        self.device.lock().under_water = under;
    }

    /// Remastered: hush the music while it plays `track` (none: retail's
    /// music), ramping, as a voice of it starts already hushed.
    pub fn hush_music(&mut self, track: Option<u8>) {
        self.device.lock().hushed_track = track;
    }

    /// The place's reverb: its decay, darkness and loudness, and the share
    /// of retail's placed sounds sent to it.
    pub fn shape_reverb(&mut self, shape: reverb::ReverbShape, retail_send: f32) {
        let mut mixer = self.device.lock();
        mixer.reverb.shape(shape);
        mixer.retail_send = retail_send.max(0.0);
    }

    /// Dull ambience voice `id`: a one-pole low-pass taking `share` of each
    /// new sample (1 leaves it open), ramping.
    pub fn filter_ambience(&mut self, id: u32, share: f32) {
        for voice in self.device.lock().ambience.iter_mut() {
            if voice.id == id {
                voice.target_cut = share.clamp(0.01, 1.0);
            }
        }
    }

    /// The buses' levels since the last call; none before a frame mixed.
    pub fn take_levels(&mut self) -> Option<BusLevels> {
        std::mem::take(&mut self.device.lock().levels).levels()
    }

    /// Move ambience voice `id`'s pitch (1 as recorded, 0.5 an octave down),
    /// ramping.
    pub fn pitch_ambience(&mut self, id: u32, pitch: f32) {
        for voice in self.device.lock().ambience.iter_mut() {
            if voice.id == id {
                voice.target_step = pitch.clamp(0.25, 4.0);
            }
        }
    }

    /// Fade ambience voice `id` out, or every one for none.
    pub fn stop_ambience(&mut self, id: Option<u32>) {
        for voice in self.device.lock().ambience.iter_mut() {
            if id.is_none_or(|id| voice.id == id) {
                voice.stopping = true;
                voice.target = 0.0;
            }
        }
    }
    pub fn open(
        audio: &sdl2::AudioSubsystem,
        data_root: &Path,
        assets: &FrontendAssets,
    ) -> Result<Self, PlatformAudioError> {
        let mapped = FrontendExecutable::parse(assets.executable_bytes())
            .map_err(|e| PlatformAudioError::Parameters(e.to_string()))?;
        let mut cue_parameters = [0; 4];
        for (i, value) in cue_parameters.iter_mut().enumerate() {
            *value = mapped
                .word(0x0053_33f8 + i as u32 * 4)
                .map_err(|e| PlatformAudioError::Parameters(e.to_string()))?;
        }
        // 004391D0 is the bounded frontend cue path. Other sound parameter
        // shapes (looping/3D owners) must be implemented before accepting them.
        if cue_parameters[2] != 0 || cue_parameters[3] != 1 {
            return Err(PlatformAudioError::Parameters(format!(
                "unsupported cue parameters {cue_parameters:?}"
            )));
        }
        let mut voices = crate::process_sound::ProcessSoundVoices::native_startup(mapped)
            .map_err(|e| PlatformAudioError::Parameters(e.to_string()))?;
        let (lifecycle, device) = lifecycle::PlatformSoundLifecycle::open_handler(
            audio,
            assets.executable_bytes(),
            &mut voices,
        )?;
        device.resume();
        let pak = data_root.join("Remastered.pak");
        let remastered = match remastered_pak::RemasteredPak::open(&pak) {
            Ok(opened) => Some(opened),
            Err(error) => {
                eprintln!("The remastered sounds are off: {error}");
                None
            }
        };
        Ok(Self {
            device,
            voices,
            lifecycle,
            pcm_sources: PreparedPcmSources::default(),
            directory: crate::retail_path(data_root, "cdaudio"),
            tracks: BTreeMap::new(),
            cue_parameters,
            cd_open: false,
            tmsf: false,
            remastered,
        })
    }

    /// Application 5220F0 reaches the shared record pool through 488000
    /// before its direct voice reference. Loan the actual session records.
    pub fn acquire_session_sounds(
        &mut self,
        sounds: &mut crate::process_sound::ProcessSoundRuntime,
    ) -> Result<(), PlatformAudioError> {
        self.lifecycle
            .acquire_session(&mut self.voices, &mut self.device, sounds.records_mut())
    }

    /// Explicit native FX shutdown. CD/title share the SDL transport, whose
    /// physical device remains owned until PlatformAudio is dropped.
    pub fn shutdown_sounds(
        &mut self,
        sounds: &mut crate::process_sound::ProcessSoundRuntime,
    ) -> Result<(), PlatformAudioError> {
        self.lifecycle
            .shutdown(&mut self.voices, &mut self.device, sounds.records_mut())?;
        self.pcm_sources.rows.clear();
        Ok(())
    }

    /// Deliver to FrontendAudio::notify before its next service call. Playback
    /// completion does not silently start the track again in this backend.
    pub fn poll_notification(&mut self) -> Option<u32> {
        let mut mixer = self.device.lock();
        std::mem::take(&mut mixer.completion).then_some(1)
    }

    pub fn stats(&mut self) -> AudioPlaybackStats {
        self.device.lock().stats
    }

    /// Native4F4780 first publishes global FX volume, then re-attenuates the
    /// linked list in order, including persistent and automatic voices. Mixer
    /// callbacks use only each voice's last published Gain-phase attenuation.
    pub fn set_effects_volume(&mut self, settings_word: u32) -> Result<(), PlatformAudioError> {
        self.device.lock().effects_volume = settings_word;
        self.voices.reattenuate(
            settings_word,
            &mut SdlVoicePlayback {
                device: &mut self.device,
            },
        )?;
        Ok(())
    }

    /// 004391D0 -> 004F46E0 -> 004F4230. The RNG is caller-owned shared
    /// DAT_0055F18C, not a second audio seed. PCM is copied into the voice so
    /// a later resource unload cannot invalidate samples in the audio thread.
    pub fn play_resident_cue(
        &mut self,
        resident: &ResidentRuntime,
        slot: i16,
        settings_word: u32,
        random_seed: &mut u32,
    ) -> Result<CueReceipt, PlatformAudioError> {
        self.play_one_shot_cue(
            &ResidentSoundResource::capture(resident, slot),
            self.cue_parameters,
            None,
            settings_word,
            random_seed,
        )
    }

    fn create_voice(
        &mut self,
        resource: &ResidentSoundResource,
        parameters: [u32; 4],
        placement: Option<crate::process_sound::SoundPlacement>,
        master: u32,
        looping: bool,
        rng: &mut u32,
    ) -> Result<(crate::process_sound::VoicePublication, Option<ResolvedCue>), PlatformAudioError>
    {
        create_voice_with(
            VoiceSourceLoans {
                voices: &mut self.voices,
                pcm: &mut self.pcm_sources,
                backend: &mut SdlVoicePlayback {
                    device: &mut self.device,
                },
            },
            VoiceSourceRequest {
                resource,
                parameters,
                placement,
                master,
                looping,
                rng,
            },
        )
    }
    fn play_one_shot_cue(
        &mut self,
        resource: &ResidentSoundResource,
        parameters: [u32; 4],
        placement: Option<crate::process_sound::SoundPlacement>,
        master: u32,
        rng: &mut u32,
    ) -> Result<CueReceipt, PlatformAudioError> {
        self.device.lock().stats.cue_requests += 1;
        let receipt = play_one_shot_with(
            VoiceSourceLoans {
                voices: &mut self.voices,
                pcm: &mut self.pcm_sources,
                backend: &mut SdlVoicePlayback {
                    device: &mut self.device,
                },
            },
            OneShotSourceRequest {
                resource,
                parameters,
                placement,
                master,
                rng,
            },
        )?;
        self.device.lock().stats.cues_started +=
            u64::from(matches!(receipt, CueReceipt::Started { .. }));
        Ok(receipt)
    }

    /// Title-video sound from the AVI `01wb` audio: stereo 22050 Hz 16-bit
    /// interleaved frames, resampled to the mixer through the title voice.
    /// See `TitleAudioSink` for the session-facing contract.
    fn track_path(&self, track: u32) -> Result<PathBuf, PlatformAudioError> {
        track_sectors(track)?;
        Ok(crate::retail_path(
            &self.directory,
            format!("track{track:02}.ogg"),
        ))
    }

    fn load_track(&mut self, track: u32) -> Result<Arc<[i16]>, PlatformAudioError> {
        if let Some(pcm) = self.tracks.get(&track) {
            return Ok(pcm.clone());
        }
        let path = self.track_path(track)?;
        let pcm = decode_track(&path, track)?;
        self.tracks.insert(track, pcm.clone());
        Ok(pcm)
    }

    fn require_cd(&self) -> Result<(), PlatformAudioError> {
        if self.cd_open && self.tmsf {
            Ok(())
        } else {
            Err(PlatformAudioError::CdClosed)
        }
    }
}

/// SDL CPU buffer preparation is keyed by the actual immutable allocation,
/// region and local row. Weak keys never keep released H9 allocations alive.
/// This represents successful raw-buffer publication before60-byte allocation;
/// it does not encode a native DirectSound pointer or handler-refcount cadence.
#[derive(Default)]
struct PreparedPcmSources {
    rows: Vec<PreparedPcmSource>,
}
struct PreparedPcmSource {
    allocation: std::sync::Weak<LoadedOverlay>,
    region: usize,
    local: u16,
    samples: Arc<[i16]>,
}
impl PreparedPcmSources {
    fn prepare(
        &mut self,
        allocation: &Arc<LoadedOverlay>,
        region: usize,
        local: u16,
        bytes: &[u8],
        configure: impl FnOnce() -> Result<(), PlatformAudioError>,
    ) -> Result<Arc<[i16]>, PlatformAudioError> {
        self.rows.retain(|r| r.allocation.strong_count() != 0);
        if let Some(r) = self.rows.iter().find(|r| {
            r.region == region
                && r.local == local
                && r.allocation.ptr_eq(&Arc::downgrade(allocation))
        }) {
            return Ok(r.samples.clone());
        }
        // Actual 00510450 configures only when creating a raw PCM buffer.
        // The successful configuration store survives later allocation errors.
        configure()?;
        if bytes.is_empty() || bytes.len() % 2 != 0 {
            return Err(PlatformAudioError::InvalidPcm);
        }
        self.rows
            .try_reserve(1)
            .map_err(|_| crate::process_sound::VoicePoolFault::Allocation)?;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(bytes.len() / 2)
            .map_err(|_| crate::process_sound::VoicePoolFault::Allocation)?;
        samples.extend(
            bytes
                .chunks_exact(2)
                .map(|p| i16::from_le_bytes([p[0], p[1]])),
        );
        let samples: Arc<[i16]> = samples.into();
        self.rows.push(PreparedPcmSource {
            allocation: Arc::downgrade(allocation),
            region,
            local,
            samples: samples.clone(),
        });
        Ok(samples)
    }
}
trait SoundSourcePreparation {
    fn enter(&mut self) -> Result<bool, PlatformAudioError>;
    fn pcm(
        &mut self,
        allocation: &Arc<LoadedOverlay>,
        region: usize,
        local: u16,
        bytes: &[u8],
    ) -> Result<Arc<[i16]>, PlatformAudioError>;
}
struct VoiceSourceLoans<'a, B> {
    voices: &'a mut crate::process_sound::ProcessSoundVoices,
    pcm: &'a mut PreparedPcmSources,
    backend: &'a mut B,
}
impl<
        B: crate::process_sound::VoicePlaybackBackend<Error = crate::process_sound::VoicePoolFault>,
    > SoundSourcePreparation for VoiceSourceLoans<'_, B>
{
    fn enter(&mut self) -> Result<bool, PlatformAudioError> {
        self.voices
            .prepare_create(self.backend)
            .map_err(PlatformAudioError::from)
    }
    fn pcm(
        &mut self,
        a: &Arc<LoadedOverlay>,
        r: usize,
        l: u16,
        b: &[u8],
    ) -> Result<Arc<[i16]>, PlatformAudioError> {
        self.pcm.prepare(a, r, l, b, || {
            self.voices
                .configure_raw_source(self.backend)
                .map_err(PlatformAudioError::from)
        })
    }
}
struct VoiceSourceRequest<'a> {
    resource: &'a ResidentSoundResource,
    parameters: [u32; 4],
    placement: Option<crate::process_sound::SoundPlacement>,
    master: u32,
    looping: bool,
    rng: &'a mut u32,
}
fn create_voice_with<
    B: crate::process_sound::VoicePlaybackBackend<Error = crate::process_sound::VoicePoolFault>,
>(
    mut services: VoiceSourceLoans<'_, B>,
    request: VoiceSourceRequest<'_>,
) -> Result<(crate::process_sound::VoicePublication, Option<ResolvedCue>), PlatformAudioError> {
    let VoiceSourceRequest {
        resource,
        parameters,
        placement,
        master,
        looping,
        rng,
    } = request;
    let resolved = resolve_sound(resource, parameters, rng, &mut services)?;
    let Some(resolved) = resolved else {
        return Ok((crate::process_sound::VoicePublication::Unwritten, None));
    };
    if resolved.samples.is_empty() {
        return Err(PlatformAudioError::InvalidPcm);
    }
    // Port fix: a splash past the cap is refused as a full pool refuses a
    // voice, after the alias walk has drawn its RNG.
    if !looping && services.voices.splash_refused(resource.slot)? {
        return Err(crate::process_sound::VoicePoolFault::Capacity2202.into());
    }
    let parameters = crate::process_sound::ProcessSoundParameters {
        volume: resolved.parameters[0],
        pitch: resolved.parameters[1],
        pan: resolved.parameters[2] as i32,
        control_flag: resolved.parameters[3] as u8 != 0,
        placement,
    };
    let voice = services.voices.allocate(
        crate::process_sound::VoiceCreateRequest {
            resource: resolved.pcm_resource.clone(),
            samples: resolved.samples.clone(),
            parameters,
            master_volume: master,
            looping,
        },
        services.backend,
    )?;
    Ok((
        crate::process_sound::VoicePublication::Written(voice),
        Some(resolved),
    ))
}
struct OneShotSourceRequest<'a> {
    resource: &'a ResidentSoundResource,
    parameters: [u32; 4],
    placement: Option<crate::process_sound::SoundPlacement>,
    master: u32,
    rng: &'a mut u32,
}
fn play_one_shot_with<
    B: crate::process_sound::VoicePlaybackBackend<Error = crate::process_sound::VoicePoolFault>,
>(
    services: VoiceSourceLoans<'_, B>,
    request: OneShotSourceRequest<'_>,
) -> Result<CueReceipt, PlatformAudioError> {
    let OneShotSourceRequest {
        resource,
        parameters,
        placement,
        master,
        rng,
    } = request;
    let created = create_voice_with(
        VoiceSourceLoans {
            voices: &mut *services.voices,
            pcm: &mut *services.pcm,
            backend: &mut *services.backend,
        },
        VoiceSourceRequest {
            resource,
            parameters,
            placement,
            master,
            looping: false,
            rng,
        },
    );
    let (publication, resolved) = match created {
        Err(PlatformAudioError::Voice(crate::process_sound::VoiceExecutionError::Pool(
            crate::process_sound::VoicePoolFault::Capacity2202,
        ))) => {
            return Ok(CueReceipt::Capacity2202 {
                requested_slot: resource.slot,
            });
        }
        other => other?,
    };
    // Disabled leaves the local native output unwritten. The wrapper's gate
    // normally returned before Create; here no source fields were demanded.
    if !services.voices.enabled() {
        return Ok(CueReceipt::Disabled {
            requested_slot: resource.slot,
        });
    }
    let crate::process_sound::VoicePublication::Written(voice) = publication else {
        return Err(crate::process_sound::VoicePoolFault::RequiredVoicePointer.into());
    };
    let resolved = resolved.expect("written publication owns actual PCM");
    services.voices.start_one_shot(voice, services.backend)?;
    services.voices.note_requested_slot(voice, resource.slot)?;
    Ok(CueReceipt::Started {
        requested_slot: resource.slot,
        pcm_slot: resolved.slot,
        sample_count: resolved.samples.len(),
        frequency_hz: frequency(resolved.parameters[1]),
        attenuation_hundredth_db: attenuation(resolved.parameters[0], master),
        alias_count: resolved.alias_count,
    })
}
impl From<crate::process_sound::VoicePoolFault> for PlatformAudioError {
    fn from(e: crate::process_sound::VoicePoolFault) -> Self {
        Self::Voice(crate::process_sound::VoiceExecutionError::Pool(e))
    }
}
/// Physical SDL voice buffers. Native list/counter custody lives on PlatformAudio.
struct SdlVoicePlayback<'a> {
    device: &'a mut AudioDevice<Mixer>,
}
impl crate::process_sound::VoicePlaybackBackend for SdlVoicePlayback<'_> {
    type Error = crate::process_sound::VoicePoolFault;
    fn configure_device(
        &mut self,
    ) -> Result<(), crate::process_sound::VoicePlaybackFault<Self::Error>> {
        // DSSCL_NORMAL is a shared device configuration. SDL has already
        // negotiated the actual transport; no exclusive/window mode is needed.
        if self.device.spec().freq != OUTPUT_RATE as i32 || self.device.spec().channels != 2 {
            return Err(crate::process_sound::VoicePlaybackFault::Required(
                crate::process_sound::VoicePoolFault::RequiredDevice,
            ));
        }
        Ok(())
    }
    fn operation(
        &mut self,
        request: crate::process_sound::VoicePlaybackRequest<'_>,
    ) -> Result<
        crate::process_sound::VoicePlaybackReply,
        crate::process_sound::VoicePlaybackFault<Self::Error>,
    > {
        mixer_voice_operation(&mut self.device.lock(), request)
    }
}
fn mixer_voice_operation(
    mixer: &mut Mixer,
    request: crate::process_sound::VoicePlaybackRequest<'_>,
) -> Result<
    crate::process_sound::VoicePlaybackReply,
    crate::process_sound::VoicePlaybackFault<crate::process_sound::VoicePoolFault>,
> {
    use crate::process_sound::{
        VoicePlaybackFault as Fault, VoicePlaybackReply as Reply, VoicePlaybackRequest as R,
        VoicePoolFault,
    };

    if let R::Create { voice, samples } = request {
        mixer
            .effects
            .try_reserve(1)
            .map_err(|_| Fault::Required(VoicePoolFault::Allocation))?;
        mixer.effects.push(EffectVoice {
            voice,
            playing: false,
            looping: false,
            samples: samples.clone(),
            position: 0.0,
            step: 0.0,
            attenuation_hundredth_db: -10000,
            pan: 0,
            gains: None,
            placed: None,
            low: 0.0,
            ears: Ears::default(),
        });
        return Ok(Reply::Done);
    }
    let voice = match &request {
        R::Status { voice }
        | R::Restore { voice }
        | R::Play { voice, .. }
        | R::Stop { voice }
        | R::Release { voice }
        | R::Frequency { voice, .. }
        | R::Pan { voice, .. }
        | R::Gain { voice, .. } => *voice,
        R::Create { .. } => unreachable!(),
    };
    let index = mixer
        .effects
        .iter()
        .position(|v| v.voice == voice)
        .ok_or(Fault::Required(VoicePoolFault::Unknown(voice)))?;
    match request {
        R::Status { .. } => {
            let v = &mixer.effects[index];
            return Ok(Reply::Playing(
                v.playing && (v.looping || v.position < v.samples.len() as f64),
            ));
        }
        R::Restore { .. } => {} //SDL buffers are retained CPU samples, never DS lost buffers.
        R::Play { looping, .. } => {
            let v = &mut mixer.effects[index];
            v.looping = looping;
            v.playing = true;
        }
        R::Stop { .. } => mixer.effects[index].playing = false,
        R::Release { .. } => {
            mixer.effects.remove(index);
        }
        R::Frequency { pitch, .. } => {
            mixer.effects[index].step = f64::from(frequency(pitch)) / f64::from(OUTPUT_RATE)
        }
        R::Pan { pan, placement, .. } => {
            let voice = &mut mixer.effects[index];
            voice.pan = pan.clamp(-10000, 10000);
            voice.placed = placement.map(PlacedEffect::new);
        }
        R::Gain { volume, master, .. } => {
            mixer.effects[index].attenuation_hundredth_db = attenuation(volume, master);
        }
        R::Create { .. } => unreachable!(),
    }
    Ok(Reply::Done)
}
impl crate::process_sound::ProcessSoundAudio for PlatformAudio {
    type Error = PlatformAudioError;
    fn operation(
        &mut self,
        request: crate::process_sound::ProcessSoundVoiceRequest<'_>,
    ) -> Result<
        crate::process_sound::ProcessSoundVoiceReply,
        crate::process_sound::ProcessSoundVoiceServiceFault<Self::Error>,
    > {
        use crate::process_sound::{
            ProcessSoundVoiceReply as Reply, ProcessSoundVoiceRequest as R,
        };
        let result: Result<Reply, PlatformAudioError> = (|| match request {
            R::OneShot {
                resource,
                parameters,
                effects_volume,
                rng,
            } => Ok(Reply::OneShot(matches!(
                self.play_one_shot_cue(
                    resource,
                    parameters.words(),
                    parameters.placement,
                    effects_volume,
                    rng
                )?,
                CueReceipt::Started { .. }
            ))),
            R::Create {
                resource,
                parameters,
                effects_volume,
                looping,
                rng,
            } => Ok(Reply::Created(
                self.create_voice(
                    resource,
                    parameters.words(),
                    parameters.placement,
                    effects_volume,
                    looping,
                    rng,
                )?
                .0,
            )),
            R::Start { voice } => {
                self.voices.start(
                    voice,
                    &mut SdlVoicePlayback {
                        device: &mut self.device,
                    },
                )?;
                Ok(Reply::Completed)
            }
            R::Update {
                voice,
                parameters,
                effects_volume,
            } => {
                self.voices.update(
                    voice,
                    parameters,
                    effects_volume,
                    &mut SdlVoicePlayback {
                        device: &mut self.device,
                    },
                )?;
                Ok(Reply::Completed)
            }
        })();
        result.map_err(crate::process_sound::ProcessSoundVoiceServiceFault::Backend)
    }
}
impl crate::process_sound::ProcessSoundVoiceTeardown for PlatformAudio {
    fn delete_voice(
        &mut self,
        _: crate::process_sound::ProcessSoundRef,
        voice: crate::process_sound::ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        self.voices
            .delete(
                voice,
                &mut SdlVoicePlayback {
                    device: &mut self.device,
                },
            )
            .map_err(crate::process_sound::ProcessSoundRecordError::VoiceExecution)
    }
}

impl FrontendAudioBackend for PlatformAudio {
    type Error = PlatformAudioError;
    fn open_cd(&mut self) -> Result<(), Self::Error> {
        // A missing cdaudio directory is an absent disc, not a broken
        // device: the transport still opens and the later media/track
        // probes report no media, so boot continues silent (retail with
        // no CD) instead of failing the frontend factory. An existing
        // but unreadable directory stays an explicit backend failure.
        match std::fs::read_dir(&self.directory) {
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(PlatformAudioError::Io {
                    path: self.directory.clone(),
                    source,
                });
            }
        }
        self.cd_open = true;
        Ok(())
    }
    fn set_tmsf_format(&mut self) -> Result<(), Self::Error> {
        if !self.cd_open {
            return Err(PlatformAudioError::CdClosed);
        }
        self.tmsf = true;
        Ok(())
    }
    fn close_cd(&mut self) -> Result<(), Self::Error> {
        self.device.lock().music = None;
        self.cd_open = false;
        self.tmsf = false;
        Ok(())
    }
    fn media_present(&mut self) -> Result<bool, Self::Error> {
        self.require_cd()?;
        Ok((2..=9).all(|track| self.track_path(track).is_ok_and(|path| path.is_file())))
    }
    fn track_two_is_audio(&mut self) -> Result<bool, Self::Error> {
        self.require_cd()?;
        let path = self.track_path(2)?;
        // A track file that was never ripped is not an audio track; the
        // caller folds this into "no media" and stays silent. A present
        // but undecodable file stays an explicit backend failure.
        if !path.is_file() {
            return Ok(false);
        }
        let decoder = open_decoder(&path)?;
        Ok(decoder.ident_hdr.audio_channels == 2
            && decoder.ident_hdr.audio_sample_rate == OUTPUT_RATE)
    }
    fn playback_mode(&mut self) -> Result<CdPlaybackMode, Self::Error> {
        self.require_cd()?;
        Ok(
            if self.device.lock().music.as_ref().is_some_and(|m| m.playing) {
                CdPlaybackMode::Playing
            } else {
                CdPlaybackMode::Other
            },
        )
    }
    fn position_tmsf(&mut self) -> Result<u32, Self::Error> {
        self.require_cd()?;
        Ok(self.device.lock().music.as_ref().map_or(0, |music| {
            u32::from(music.track) | (frames_to_msf(music.frame) << 8)
        }))
    }
    fn track_length_msf(&mut self, track_word: u32) -> Result<u32, Self::Error> {
        self.require_cd()?;
        let path = self.track_path(track_word)?;
        File::open(&path).map_err(|source| PlatformAudioError::Io { path, source })?;
        Ok(sectors_to_msf(track_sectors(track_word)?))
    }
    fn play_range(&mut self, range: CdPlayRange) -> Result<(), Self::Error> {
        self.require_cd()?;
        let track = range.from_tmsf & 0xff;
        if range.to_tmsf != track + 1 {
            return Err(PlatformAudioError::Range(range));
        }
        let seconds = ((range.from_tmsf >> 16) & 0xff) as usize;
        let sectors = (range.from_tmsf >> 24) as usize;
        if seconds >= 60 || sectors >= 75 {
            return Err(PlatformAudioError::Range(range));
        }
        let minutes = ((range.from_tmsf >> 8) & 0xff) as usize;
        let frame = ((minutes * 60 + seconds) * 75 + sectors) * SAMPLES_PER_SECTOR;
        let samples = self.load_track(track)?;
        if frame >= samples.len() / 2 {
            return Err(PlatformAudioError::Range(range));
        }
        self.device.lock().start_music(MusicVoice {
            track: track as u8,
            samples,
            frame,
            playing: true,
        });
        Ok(())
    }
    fn stop_cd(&mut self) -> Result<(), Self::Error> {
        self.require_cd()?;
        if let Some(music) = &mut self.device.lock().music {
            music.playing = false;
        }
        Ok(())
    }
    fn set_cd_volume(&mut self, stereo_word: u32) -> Result<(), Self::Error> {
        self.device.lock().cd_volume = [
            (stereo_word & 0xffff) as f32 / 65535.0,
            (stereo_word >> 16) as f32 / 65535.0,
        ];
        Ok(())
    }
    fn clock_seconds(&mut self) -> Result<i32, Self::Error> {
        Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| PlatformAudioError::Clock(e.to_string()))?
            .as_secs() as u32 as i32)
    }
}

fn track_sectors(track: u32) -> Result<u32, PlatformAudioError> {
    track
        .checked_sub(2)
        .and_then(|i| TRACK_SECTORS.get(i as usize))
        .copied()
        .ok_or(PlatformAudioError::Track(track))
}
fn sectors_to_msf(sectors: u32) -> u32 {
    let seconds = sectors / 75;
    (seconds / 60) | ((seconds % 60) << 8) | ((sectors % 75) << 16)
}
fn frames_to_msf(frames: usize) -> u32 {
    sectors_to_msf((frames / SAMPLES_PER_SECTOR) as u32)
}
fn open_decoder(path: &Path) -> Result<OggStreamReader<BufReader<File>>, PlatformAudioError> {
    let file = File::open(path).map_err(|source| PlatformAudioError::Io {
        path: path.into(),
        source,
    })?;
    OggStreamReader::new(BufReader::new(file)).map_err(|e| PlatformAudioError::Decode {
        path: path.into(),
        reason: e.to_string(),
    })
}
fn decode_track(path: &Path, track: u32) -> Result<Arc<[i16]>, PlatformAudioError> {
    let expected = track_sectors(track)? as usize * SAMPLES_PER_SECTOR;
    let mut decoder = open_decoder(path)?;
    if decoder.ident_hdr.audio_channels != 2 || decoder.ident_hdr.audio_sample_rate != OUTPUT_RATE {
        return Err(PlatformAudioError::TrackFormat {
            track,
            expected,
            actual: 0,
        });
    }
    let mut pcm = Vec::with_capacity(expected * 2);
    while let Some(packet) =
        decoder
            .read_dec_packet_itl()
            .map_err(|e| PlatformAudioError::Decode {
                path: path.into(),
                reason: e.to_string(),
            })?
    {
        pcm.extend(packet);
        if pcm.len() > expected * 2 {
            return Err(PlatformAudioError::TrackFormat {
                track,
                expected,
                actual: pcm.len() / 2,
            });
        }
    }
    if pcm.len() != expected * 2 {
        return Err(PlatformAudioError::TrackFormat {
            track,
            expected,
            actual: pcm.len() / 2,
        });
    }
    Ok(pcm.into())
}

struct ResolvedCue {
    pcm_resource: ResidentSoundResource,
    slot: i16,
    samples: Arc<[i16]>,
    parameters: [u32; 4],
    alias_count: usize,
}

/// Resolved H9 identity retained by the process sound queue. Missing/null H9
/// stays null until an admitted voice consumer reports the required resource.
#[derive(Clone, Debug)]
pub struct ResidentSoundResource {
    slot: i16,
    allocation: Option<(Arc<LoadedOverlay>, usize, u16)>,
}

impl ResidentSoundResource {
    pub fn capture(resident: &ResidentRuntime, slot: i16) -> Self {
        let allocation = u16::try_from(slot)
            .ok()
            .and_then(|index| resident.resource(9, index))
            .map(|resource| {
                (
                    resource.overlay.clone(),
                    resource.registration.region_index,
                    resource.registration.local_index,
                )
            });
        Self { slot, allocation }
    }
    pub fn requested_slot(&self) -> i16 {
        self.slot
    }
}

fn resolve_sound(
    resource: &ResidentSoundResource,
    mut parameters: [u32; 4],
    random_seed: &mut u32,
    prepare: &mut impl SoundSourcePreparation,
) -> Result<Option<ResolvedCue>, PlatformAudioError> {
    if !prepare.enter()? {
        return Ok(None);
    }
    let (allocation, region_index, mut local_index) = resource
        .allocation
        .as_ref()
        .map(|(allocation, region_index, local_index)| (allocation, *region_index, *local_index))
        .ok_or(PlatformAudioError::MissingSound(resource.slot))?;
    let Some(HandlerResources::Sounds(bank)) = allocation.handlers().get(region_index) else {
        return Err(PlatformAudioError::SoundRecord(resource.slot));
    };
    let mapping = allocation
        .requests
        .get(9)
        .ok_or(PlatformAudioError::SoundRecord(resource.slot))?;
    let mut visited = Vec::new();
    let mut slot = resource.slot;
    loop {
        if visited.contains(&local_index) {
            return Err(PlatformAudioError::AliasCycle(slot));
        }
        visited.push(local_index);
        let entry = bank
            .entries
            .get(usize::from(local_index))
            .ok_or(PlatformAudioError::SoundRecord(slot))?;
        match entry.kind {
            SoundEntryKind::Pcm { data, .. } => {
                let bytes = data
                    .get(allocation.bytes())
                    .ok_or(PlatformAudioError::SoundRecord(slot))?;
                let samples = prepare.pcm(allocation, region_index, local_index, bytes)?;
                return Ok(Some(ResolvedCue {
                    //Recursive Create copies the actual final raw PCM row,
                    //not the original alias's five DWORDs.
                    pcm_resource: ResidentSoundResource {
                        slot,
                        allocation: Some((allocation.clone(), region_index, local_index)),
                    },
                    slot,
                    samples,
                    parameters,
                    alias_count: visited.len() - 1,
                }));
            }
            SoundEntryKind::Alias {
                target_global_slot, ..
            } => {
                // 0050F760 fixes alias pointers at load time. Same-allocation
                // target registration is the last matching local entry in
                // this allocation's actual piecewise mapping. Current global
                // residency must not redirect it after replacement/unload.
                let target_local = u16::try_from(target_global_slot)
                    .ok()
                    .and_then(|target| {
                        (0..bank.entries.len()).rev().find_map(|local| {
                            let local = u16::try_from(local).ok()?;
                            (mapping.global_index(local) == Some(target)).then_some(local)
                        })
                    })
                    .ok_or(PlatformAudioError::AliasCustody(slot))?;
                let random = crate::name_entry::name_entry_rng_next(random_seed);
                parameters = alias_parameters(parameters, entry.fields_04_to_10, random);
                slot = target_global_slot;
                local_index = target_local;
                if !prepare.enter()? {
                    return Ok(None);
                }
            }
            SoundEntryKind::Unknown { .. } => return Err(PlatformAudioError::SoundRecord(slot)),
        }
    }
}

/// Literal wrapping arithmetic at 004F428A..004F4368:
/// +08 spread, +0C pitch, +10 gain.
fn alias_parameters(mut parameters: [u32; 4], fields: [u32; 4], random: u32) -> [u32; 4] {
    let pitch = mul_16_16(parameters[1], fields[2]);
    let spread = mul_16_16(pitch, fields[1]);
    let variation = ((random & 0xffff).wrapping_mul(spread) as i32 >> 16) as u32;
    parameters[1] = if variation & 1 == 0 {
        pitch.wrapping_sub(variation)
    } else {
        pitch.wrapping_add(variation)
    };
    if fields[3] != 0x10000 {
        let gain = parameters[0];
        let lo = fields[3] & 0xffff;
        let hi = fields[3] >> 16;
        parameters[0] = if gain < 0x10000 {
            hi.wrapping_mul(gain)
                .wrapping_add(lo.wrapping_mul(gain) >> 16)
        } else {
            (gain >> 16)
                .wrapping_mul(lo)
                .wrapping_add(hi.wrapping_mul(gain))
                .wrapping_add((gain & 0xff).wrapping_mul(lo) >> 16)
        };
    }
    parameters
}
fn mul_16_16(a: u32, b: u32) -> u32 {
    ((u64::from(a) * u64::from(b)) >> 16) as u32
}
fn frequency(pitch: u32) -> u32 {
    (pitch.wrapping_mul(22_050) >> 16).clamp(100, 100_000)
}

// 004F40B0 stores ln(65536) as f32; 004F4455 applies SoundFX's expanded
// nibble, signed division, logarithm and DirectSound hundredths-of-dB volume.
fn attenuation(gain: u32, settings: u32) -> i32 {
    let expanded = crate::frontend_audio::expanded_volume(settings);
    let level = (((expanded >> 8).wrapping_mul(gain) as i32) / 255).min(65536);
    if level <= 0 {
        return -10_000;
    }
    let reference = f64::from(65536.0f64.ln() as f32);
    (((reference - f64::from(level).ln()) / reference) * -10_000.0).round_ties_even() as i32
}
/// The Ambience volume's amplitude: retail's curve for a 0 to 15 setting
/// at full gain, as its Sound slider gives an effect.
fn ambience_gain(level: u8) -> f32 {
    if level == 0 {
        0.0
    } else {
        ds_amplitude(attenuation(65536, u32::from(level.min(15))))
    }
}

fn ds_amplitude(volume: i32) -> f32 {
    if volume <= -10_000 {
        0.0
    } else {
        10.0f32.powf(volume as f32 / 2000.0)
    }
}

#[cfg(test)]
#[path = "platform_audio_tests.rs"]
mod tests;
