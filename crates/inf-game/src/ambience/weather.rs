//! The weather's sound (the plan's "Weather:
//! one state for the sky and the sound"): the wind, gusting with the
//! world's own wind and rushing past the ACAM as it speeds up; the rain, as
//! thick as the sky draws it; and the rain drumming on the hull, a close
//! patter around the camera that drums inside the shell in first person.
//!
//! Each layer plays a recording the pak names for it, or, without one, a
//! loop synthesized when the world starts (the plan's synthesized wind
//! layer; for the rain, until the pak holds recordings): noise shaped by
//! filters, single drops and the ring of drops on metal, seeded by the
//! world, so a fixed-step run sounds the same every time. Nothing here
//! reads the game's random numbers.

use std::f32::consts::TAU;
use std::sync::Arc;

use crate::modern_tuning::WeatherSoundTuning;
use crate::platform_audio::remastered_pak::DecodedSound;
use crate::platform_audio::{PlatformAudio, OUTPUT_RATE};

use super::{next_voice, pick};

/// The synthesized loops' names among the ambience's sounds.
pub(super) const WIND: &str = "synth/wind";
pub(super) const RAIN: &str = "synth/rain";
pub(super) const HULL: &str = "synth/hull";

/// What the weather's sound follows each frame: the rain's share as the
/// sky draws it (0 to 1), the ACAM's speed in velocity units, whether the
/// camera is inside its shell, the modern sky's lightning (flashes a
/// second, 0 for none), the game's animation clock it flashes on (50 a
/// second) and whether the camera is under the water.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherInputs {
    pub rain: f32,
    pub speed: f32,
    pub first_person: bool,
    pub lightning: f32,
    pub clock: u32,
    pub under_water: bool,
}

/// A layer playing: its voice.
#[derive(Debug, Default)]
struct Layer {
    voice: Option<u32>,
}

#[derive(Debug, Default)]
pub(super) struct Weather {
    wind: Layer,
    rain: Layer,
    hull: Layer,
    /// The rain's share as it eases toward the sky's.
    rain_share: f32,
    /// The animation clock's seconds last frame, the thunders waiting for
    /// their time, and those sounding (voice, gain, ends).
    flashes_seen: Option<f32>,
    rumbles: Vec<Rumble>,
    thundering: Vec<(u32, f32, u64)>,
    thunders: u32,
    /// The last thunder played, which the next never repeats.
    last_thunder: Option<usize>,
    /// When the last thunder at its full level sounded: within
    /// [`LOUD_THUNDER_GAP`] of it the next ones roll muted.
    loud_at: Option<u64>,
}

/// The least time between two thunders at their full level, in
/// microseconds; the rest in between roll muted (2026-10-26: only some thunders play at their intended
/// volume, at most once every 10 seconds, so it does not get annoying).
const LOUD_THUNDER_GAP: u64 = 10_000_000;
/// A muted thunder's share of the thunder's level.
const MUTED_THUNDER: f32 = 0.25;

/// A thunder on its way: when, near or far, and from which side.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rumble {
    due: u64,
    near: bool,
    pan: f32,
}

/// A flash of the modern sky's lightning (`pt_sky.glsl`'s
/// `lightning_flash`, the same hash and times): its start in the animation
/// clock's seconds, the direction toward it across the ground (X, Z), and
/// whether it is a near strike (one in four) rather than a flash inside
/// the clouds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Flash {
    pub start: f32,
    pub toward: [f32; 2],
    pub near: bool,
}

fn sky_hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// The flash of slot `slot` at `rate` flashes a second, if it flashes
/// (three slots in four).
pub(crate) fn flash_in_slot(rate: f32, slot: u32) -> Option<Flash> {
    if rate <= 0.0 {
        return None;
    }
    let spacing = 1.0 / rate;
    let h = sky_hash(slot.wrapping_mul(747_796_405).wrapping_add(2_891_336_453));
    if h & 3 == 0 {
        return None;
    }
    let start = (slot as f32 + 0.1 + 0.6 * ((h >> 2) & 0xffff) as f32 / 65_536.0) * spacing;
    let turn = (h >> 18) as f32 / 16_384.0 * TAU;
    Some(Flash {
        start,
        toward: [turn.cos(), turn.sin()],
        near: sky_hash(h) & 3 == 0,
    })
}

/// What a layer plays: the pak's recording if it holds one, else the synth.
pub(super) fn sound_name(recording: &Option<String>, synth: &'static str) -> String {
    recording.clone().unwrap_or_else(|| synth.to_string())
}

impl Weather {
    /// The names a world's weather plays (to decode or synthesize): the
    /// wind where the world has air, the rain and the hull everywhere it
    /// may rain.
    pub(super) fn names(tuning: &WeatherSoundTuning, world: u32, may_rain: bool) -> Vec<String> {
        let mut names = Vec::new();
        if tuning.winds.contains_key(&world) {
            names.push(sound_name(&tuning.wind_recording, WIND));
        }
        if may_rain {
            names.push(sound_name(&tuning.rain_recording, RAIN));
            names.push(sound_name(&tuning.hull_recording, HULL));
        }
        names.extend(tuning.thunder.iter().cloned());
        names
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update(
        &mut self,
        world: u32,
        tuning: &WeatherSoundTuning,
        inputs: WeatherInputs,
        sounds: &std::collections::HashMap<String, Arc<DecodedSound>>,
        next: &mut u32,
        now: u64,
        elapsed: f32,
        presence: f32,
        audio: &mut PlatformAudio,
    ) {
        let seconds = now as f32 / 1.0e6;
        self.rain_share +=
            (inputs.rain.clamp(0.0, 1.0) - self.rain_share).clamp(-0.25 * elapsed, 0.25 * elapsed);
        let rush = smoothstep(tuning.rush[0], tuning.rush[1], inputs.speed);
        // Under the water no air blows past and no rain strikes the hull
        // (2026-10-28); the
        // rain on the surface and the thunder still carry, muffled.
        let in_air = if inputs.under_water { 0.0 } else { 1.0 };
        // The wind: the world's strength, swelling with its gusts, and the
        // air rushing past at speed; brighter and a little higher as it
        // blows harder.
        if let Some(&[strength, gusty, _]) = tuning.winds.get(&world) {
            let gust = gust_swell(world, seconds);
            let blow = strength * (1.0 - gusty + gusty * gust);
            let gain = (tuning.wind * blow + tuning.rush_level * rush) * presence * in_air;
            let pitch = 0.8 + 0.3 * blow + 0.3 * rush;
            let open = (0.2 + 0.5 * blow + 0.3 * rush).min(1.0);
            let name = sound_name(&tuning.wind_recording, WIND);
            self.wind
                .steer(world, &name, sounds, next, gain, pitch, open, audio);
        }
        // The rain around, as thick as it falls; on the hull, closer and
        // louder in first person, and harder driving into it.
        let rain = self.rain_share;
        let name = sound_name(&tuning.rain_recording, RAIN);
        let gain = tuning.rain * rain.sqrt() * presence;
        self.rain
            .steer(world, &name, sounds, next, gain, 1.0, 1.0, audio);
        self.thunder(world, tuning, inputs, sounds, next, now, presence, audio);
        let hull = if inputs.first_person {
            tuning.hull_inside
        } else {
            tuning.hull
        };
        let name = sound_name(&tuning.hull_recording, HULL);
        let gain = hull * rain * (1.0 + 0.5 * rush) * presence * in_air;
        self.hull
            .steer(world, &name, sounds, next, gain, 1.0, 1.0, audio);
    }
}

impl Weather {
    /// Each flash since the last frame sends its thunder: a near strike's
    /// crack and boom 0.3 to 1.5 s after, full; a flash inside the clouds
    /// a low roll 2.5 to 6 s after, half as loud and duller; from the
    /// flash's side. At most two sound together, a new one turning the
    /// older down.
    #[allow(clippy::too_many_arguments)]
    fn thunder(
        &mut self,
        world: u32,
        tuning: &WeatherSoundTuning,
        inputs: WeatherInputs,
        sounds: &std::collections::HashMap<String, Arc<DecodedSound>>,
        next: &mut u32,
        now: u64,
        presence: f32,
        audio: &mut PlatformAudio,
    ) {
        let seconds = inputs.clock as f32 / 50.0;
        let seen = self.flashes_seen.replace(seconds);
        if let (Some(seen), true) = (seen, inputs.lightning > 0.0 && !tuning.thunder.is_empty()) {
            let spacing = 1.0 / inputs.lightning;
            let first = (seen / spacing).floor().max(0.0) as u32;
            let last = (seconds / spacing).floor().max(0.0) as u32;
            for slot in first..=last.min(first + 8) {
                let Some(flash) = flash_in_slot(inputs.lightning, slot) else {
                    continue;
                };
                if flash.start <= seen || flash.start > seconds {
                    continue;
                }
                self.thunders += 1;
                let turn = 8_000 + self.thunders;
                let share = pick(world, turn, 1_000) as f32 / 1_000.0;
                let delay = if flash.near {
                    0.3 + 1.2 * share
                } else {
                    2.5 + 3.5 * share
                };
                self.rumbles.push(Rumble {
                    due: now + (delay * 1.0e6) as u64,
                    near: flash.near,
                    pan: flash.toward[0] * 0.8,
                });
            }
        }
        let (due, waiting): (Vec<Rumble>, Vec<Rumble>) =
            self.rumbles.iter().partition(|rumble| rumble.due <= now);
        self.rumbles = waiting;
        for rumble in due {
            let count = tuning.thunder.len();
            let turn = 8_500 + self.thunders + self.rumbles.len() as u32;
            let index = match self.last_thunder {
                Some(last) if count > 1 => (last + 1 + pick(world, turn, count - 1)) % count,
                _ => pick(world, turn, count),
            };
            self.last_thunder = Some(index);
            // A recording not yet decoded gives way to one that is.
            let Some((index, sound)) = std::iter::once(index)
                .chain(0..count)
                .find_map(|index| Some((index, sounds.get(&tuning.thunder[index])?)))
            else {
                continue;
            };
            // The older ones give way.
            if self.thundering.len() >= 2 {
                let (voice, ..) = self.thundering.remove(0);
                audio.stop_ambience(Some(voice));
            }
            for (voice, gain, _) in &mut self.thundering {
                *gain *= 0.5;
                audio.steer_ambience(*voice, *gain * presence, 0.0);
            }
            let loud = self
                .loud_at
                .is_none_or(|at| now.saturating_sub(at) >= LOUD_THUNDER_GAP);
            let (gain, pitch, open) = match (loud, rumble.near) {
                (true, true) => (tuning.thunder_level, 1.0, 1.0),
                (true, false) => (0.6 * tuning.thunder_level, 0.85, 0.4),
                (false, _) => (MUTED_THUNDER * tuning.thunder_level, 0.85, 0.3),
            };
            if loud {
                self.loud_at = Some(now);
            }
            let voice = next_voice(next);
            audio.play_ambience(voice, Arc::clone(sound), false, gain * presence, rumble.pan);
            audio.pitch_ambience(voice, pitch);
            audio.filter_ambience(voice, open);
            let seconds = sound.frames() as f32 / OUTPUT_RATE as f32 / pitch;
            self.thundering
                .push((voice, gain, now + (seconds * 1.0e6) as u64));
            let what = if rumble.near {
                "thunder near"
            } else {
                "thunder far"
            };
            crate::event_log::ambience("weather", &tuning.thunder[index], what);
        }
        self.thundering.retain(|&(.., ends)| ends > now);
    }
}

impl Layer {
    /// Bring the layer to `gain`, `pitch` and its low-pass `open` share,
    /// starting its loop once decoded and gain is called for.
    #[allow(clippy::too_many_arguments)]
    fn steer(
        &mut self,
        world: u32,
        name: &str,
        sounds: &std::collections::HashMap<String, Arc<DecodedSound>>,
        next: &mut u32,
        gain: f32,
        pitch: f32,
        open: f32,
        audio: &mut PlatformAudio,
    ) {
        if self.voice.is_none() && gain > 1.0e-4 {
            if let Some(sound) = sounds.get(name) {
                let voice = next_voice(next);
                let start = pick(world, 7_000 + voice, sound.frames().max(1));
                audio.play_ambience_from(voice, Arc::clone(sound), true, 0.0, 0.0, start);
                self.voice = Some(voice);
                crate::event_log::ambience("weather", name, "start");
            }
        }
        if let Some(voice) = self.voice {
            audio.steer_ambience(voice, gain.max(0.0), 0.0);
            audio.pitch_ambience(voice, pitch);
            audio.filter_ambience(voice, open);
        }
    }
}

/// The wind's gust swell at `seconds`, 0 between gusts to 1 at their peak:
/// two slow waves at unrelated periods, so the gusts never fall into a
/// beat, sharpened so the calm lasts longer than the gust.
pub(crate) fn gust_swell(world: u32, seconds: f32) -> f32 {
    let phase = |turn| pick(world, turn, 1_000) as f32 / 1_000.0 * TAU;
    let wave = 0.6 * (TAU * seconds / 11.0 + phase(9_001)).sin()
        + 0.4 * (TAU * seconds / 4.3 + phase(9_002)).sin();
    let share = (0.5 + 0.5 * wave).clamp(0.0, 1.0);
    share * share
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1.0e-3)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The synthesized loop `name` for `world`, or none for a name not
/// synthesized here.
pub(super) fn synthesize(name: &str, world: u32) -> Option<DecodedSound> {
    let seed = 0x9e37_79b9_u32 ^ world.wrapping_mul(0x85eb_ca6b);
    let frames = match name {
        WIND => wind(seed, 16.0),
        RAIN => rain(seed ^ 0x51ed, 12.0),
        HULL => hull(seed ^ 0xa11d, 10.0),
        _ => return None,
    };
    Some(finish(frames))
}

/// Whether `name` is a loop synthesized here.
pub(super) fn synthesize_known(name: &str) -> bool {
    matches!(name, WIND | RAIN | HULL)
}

/// A tiny generator of its own (xorshift), never the game's.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32
    }

    /// White noise, -1 to 1.
    fn white(&mut self) -> f32 {
        2.0 * self.next() - 1.0
    }
}

/// A two-pole band-pass (a state-variable filter).
#[derive(Default)]
struct Band {
    low: f32,
    band: f32,
}

impl Band {
    fn run(&mut self, input: f32, centre: f32, damping: f32) -> f32 {
        let f = 2.0 * (std::f32::consts::PI * centre / OUTPUT_RATE as f32).sin();
        self.low += f * self.band;
        let high = input - self.low - damping * self.band;
        self.band += f * high;
        self.band
    }
}

/// The wind: rumbling low noise under a breathy band whose centre wanders,
/// a faint whistle in it now and then, each channel its own air.
fn wind(seed: u32, seconds: f32) -> Vec<[f32; 2]> {
    let count = frames_for(seconds);
    let mut out = vec![[0.0; 2]; count];
    for (channel, salt) in [(0usize, 0x1234_u32), (1, 0x8765)] {
        let mut noise = Noise(seed ^ salt | 1);
        let (mut brown, mut low) = (0.0_f32, 0.0_f32);
        let (mut breath, mut whistle) = (Band::default(), Band::default());
        let drift = noise.next() * TAU;
        for (i, frame) in out.iter_mut().enumerate() {
            let t = i as f32 / OUTPUT_RATE as f32;
            let white = noise.white();
            brown = (brown + 0.02 * white) * 0.998;
            low += 0.05 * (brown - low);
            // Whole cycles over the loop, so its ends meet.
            let wander = (TAU * 2.0 * t / seconds + drift).sin();
            let centre = 420.0 + 220.0 * wander;
            let air = breath.run(white, centre, 1.4);
            let edge = whistle.run(white, 2.6 * centre, 0.08)
                * (0.5 + 0.5 * (TAU * 3.0 * t / seconds + drift).sin()).powi(4);
            frame[channel] = 3.0 * low + 0.35 * air + 0.04 * edge;
        }
    }
    looped(out, 1.0)
}

/// The rain: a fine hiss under thousands of small drops, each a short
/// burst of noise, brighter or duller, struck anywhere between the ears.
fn rain(seed: u32, seconds: f32) -> Vec<[f32; 2]> {
    let count = frames_for(seconds);
    let mut out = vec![[0.0; 2]; count];
    let mut noise = Noise(seed | 1);
    let (mut high, mut low) = ([0.0_f32; 2], [0.0_f32; 2]);
    for frame in out.iter_mut() {
        for (channel, sample) in frame.iter_mut().enumerate() {
            let white = noise.white();
            // A band from about 1 kHz to 8 kHz.
            high[channel] += 0.14 * (white - high[channel]);
            low[channel] += 0.7 * ((white - high[channel]) - low[channel]);
            *sample = 0.25 * low[channel];
        }
    }
    let rate = 900.0 / OUTPUT_RATE as f32;
    let mut at = 0usize;
    while at < count {
        at += (-(1.0 - noise.next()).max(1.0e-6).ln() / rate) as usize + 1;
        let loudness = 0.15 + 0.85 * noise.next().powi(3);
        let decay = (0.0015 + 0.0045 * noise.next()) * OUTPUT_RATE as f32;
        let bright = 0.2 + 0.7 * noise.next();
        let pan = noise.next();
        let gains = [(1.0 - pan).sqrt(), pan.sqrt()];
        let mut smooth = 0.0;
        for k in 0..(decay * 5.0) as usize {
            let index = (at + k) % count;
            smooth += bright * (noise.white() - smooth);
            let sample = loudness * smooth * (-(k as f32) / decay).exp();
            out[index][0] += gains[0] * sample;
            out[index][1] += gains[1] * sample;
        }
    }
    looped(out, 0.5)
}

/// Rain on the hull: drops striking metal plate, each a click and a short
/// ring at the plate's tones, a dull body under it; close, so near the
/// middle.
fn hull(seed: u32, seconds: f32) -> Vec<[f32; 2]> {
    let count = frames_for(seconds);
    let mut out = vec![[0.0; 2]; count];
    let mut noise = Noise(seed | 1);
    let rate = 32.0 / OUTPUT_RATE as f32;
    let mut at = 0usize;
    while at < count {
        at += (-(1.0 - noise.next()).max(1.0e-6).ln() / rate) as usize + 1;
        let loudness = 0.2 + 0.8 * noise.next().powi(2);
        let ring = 1_800.0 + 2_600.0 * noise.next();
        let body = 480.0 + 420.0 * noise.next();
        let ring_decay = (0.02 + 0.04 * noise.next()) * OUTPUT_RATE as f32;
        let body_decay = 0.012 * OUTPUT_RATE as f32;
        let pan = 0.3 + 0.4 * noise.next();
        let gains = [(1.0 - pan).sqrt(), pan.sqrt()];
        for k in 0..(ring_decay * 5.0) as usize {
            let t = k as f32 / OUTPUT_RATE as f32;
            let click = if k < 40 {
                noise.white() * (1.0 - k as f32 / 40.0)
            } else {
                0.0
            };
            let sample = loudness
                * (0.5 * (TAU * ring * t).sin() * (-(k as f32) / ring_decay).exp()
                    + 0.6 * (TAU * body * t).sin() * (-(k as f32) / body_decay).exp()
                    + 0.4 * click);
            let index = (at + k) % count;
            out[index][0] += gains[0] * sample;
            out[index][1] += gains[1] * sample;
        }
    }
    looped(out, 0.3)
}

fn frames_for(seconds: f32) -> usize {
    (seconds * OUTPUT_RATE as f32) as usize
}

/// The loop's last `seconds` folded over its first at equal power, then
/// cut, so it repeats without a seam.
fn looped(mut frames: Vec<[f32; 2]>, seconds: f32) -> Vec<[f32; 2]> {
    let fold = frames_for(seconds).min(frames.len() / 2);
    let length = frames.len() - fold;
    for i in 0..fold {
        let share = i as f32 / fold as f32;
        let (fade_in, fade_out) = ((share * TAU / 4.0).sin(), (share * TAU / 4.0).cos());
        for channel in 0..2 {
            frames[i][channel] =
                frames[i][channel] * fade_in + frames[length + i][channel] * fade_out;
        }
    }
    frames.truncate(length);
    frames
}

/// The loop at the beds' loudness (their -26 LUFS, near an RMS of 0.05),
/// its peaks held under 0.8, as 16-bit stereo.
fn finish(frames: Vec<[f32; 2]>) -> DecodedSound {
    let count = (frames.len() * 2).max(1) as f32;
    let rms = (frames
        .iter()
        .flat_map(|frame| frame.iter())
        .map(|sample| sample * sample)
        .sum::<f32>()
        / count)
        .sqrt()
        .max(1.0e-9);
    let peak = frames
        .iter()
        .flat_map(|frame| frame.iter())
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let scale = (0.05 / rms).min(0.8 / peak.max(1.0e-9));
    DecodedSound {
        channels: 2,
        samples: frames
            .iter()
            .flat_map(|frame| frame.iter())
            .map(|sample| (sample * scale * 32_767.0).clamp(-32_768.0, 32_767.0) as i16)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each loop is the same every run, of its length, at the beds'
    /// loudness, and its seam no louder a jump than its inside.
    #[test]
    fn the_loops_repeat_and_meet_at_their_seams() {
        for (name, made, fold) in [(WIND, 16.0, 1.0), (RAIN, 12.0, 0.5), (HULL, 10.0, 0.3)] {
            let sound = synthesize(name, 9).unwrap();
            assert_eq!(
                sound.samples,
                synthesize(name, 9).unwrap().samples,
                "{name}"
            );
            assert_eq!(sound.channels, 2);
            assert_eq!(
                sound.frames(),
                frames_for(made) - frames_for(fold),
                "{name}"
            );
            let samples: Vec<f32> = sound
                .samples
                .iter()
                .map(|&s| f32::from(s) / 32_768.0)
                .collect();
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            assert!((0.03..0.06).contains(&rms), "{name} rms {rms}");
            let jump = |a: usize, b: usize| (samples[a] - samples[b]).abs();
            let seam = jump(samples.len() - 2, 0).max(jump(samples.len() - 1, 1));
            let typical = (2..samples.len()).map(|i| jump(i, i - 2)).sum::<f32>()
                / (samples.len() - 2) as f32;
            assert!(
                seam < 8.0 * typical + 0.02,
                "{name} seam {seam} typical {typical}"
            );
        }
        assert_ne!(
            synthesize(WIND, 9).unwrap().samples,
            synthesize(WIND, 10).unwrap().samples
        );
        assert!(synthesize("beds/open-1.ogg", 9).is_none());
    }

    /// The flashes keep the sky's schedule: about three slots in four
    /// flash, each within its slot, and about one in four is a near strike.
    #[test]
    fn the_flashes_keep_the_skys_schedule() {
        let rate = 0.25;
        let flashes: Vec<Flash> = (0..4_000)
            .filter_map(|slot| flash_in_slot(rate, slot))
            .collect();
        assert!((2_850..3_150).contains(&flashes.len()), "{}", flashes.len());
        let near = flashes.iter().filter(|flash| flash.near).count();
        assert!((600..900).contains(&near), "{near}");
        for slot in 0..200 {
            if let Some(flash) = flash_in_slot(rate, slot) {
                let start = slot as f32 / rate;
                assert!(flash.start >= start + 0.1 / rate - 1.0e-3);
                assert!(flash.start <= start + 0.7 / rate + 1.0e-3);
            }
        }
        assert!(flash_in_slot(0.0, 5).is_none());
    }

    /// Gusts come and go: the swell spans most of its range over a minute,
    /// and spends longer calm than gusting.
    #[test]
    fn gusts_come_and_go() {
        let swell: Vec<f32> = (0..600)
            .map(|tenth| gust_swell(9, tenth as f32 / 10.0))
            .collect();
        let (low, high) = swell
            .iter()
            .fold((1.0_f32, 0.0_f32), |(l, h), &s| (l.min(s), h.max(s)));
        assert!(low < 0.1 && high > 0.8, "{low} {high}");
        let calm = swell.iter().filter(|&&s| s < 0.5).count();
        assert!(calm > 300, "{calm}");
    }
}
