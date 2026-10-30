//! The port's ambience under Vulkan ([the plan](../../../docs/rt-plans/atmosphere.md)): each world's bed, a
//! quiet, never-ending background of its kind of place (open air, cold
//! wind, desert, volcanic, storm, airless, station, the clarifier city, the
//! Brain), under retail's music; the Brain's beds play instead of it
//! ([`replaces_music`]). Retail has none: away from its machines a world
//! makes no sound of its own.
//!
//! A bed takes turns between two or three long recordings: one plays
//! looping from a point chosen by the world and the clock, and every few
//! minutes the next fades in over it. Its sweeteners, one-shot details,
//! come now and then from around the listener, never the same one twice
//! running, each a little higher or lower and louder or softer; the Brain's
//! pulse beats ahead. A bed can follow the world's light on retail's tint
//! schedule (Artoria's drone powers down when its generators die) and have
//! another bed beneath it. Over the beds, the machines, fields, barriers and
//! shores near the camera are heard from where they are ([`placed`]). The
//! recordings decode on a thread of their own when a world starts (the
//! Brain's from its load, ready when its play begins). The beds fade out in
//! the in-world menu (not the Brain's: they are its music) and stop outside
//! play. Only a sound: it
//! reads the session, never writes it, and never draws the game's random
//! numbers.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use crate::modern_tuning::{AtmosphereTuning, BedTuning};
use crate::platform_audio::remastered_pak::{DecodedSound, RemasteredPak};
use crate::platform_audio::PlatformAudio;

mod placed;
mod weather;

pub(crate) use weather::gust_swell;
pub use weather::WeatherInputs;

/// What the controller reads each frame: the world in play (none outside
/// it), whether its in-world menu is up, the app's clock (microseconds),
/// and the world's light on retail's tint schedule, 0 at its darkest style
/// to 1 at its brightest (none for a world of one light).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbienceInputs {
    pub world: Option<u32>,
    /// The world loaded whose beds play instead of its music
    /// ([`replaces_music`]): its recordings decode from its load, so they
    /// are ready when play begins, and its in-world menu keeps its beds up,
    /// as it keeps the music.
    pub music: Option<u32>,
    pub menu: bool,
    pub now: u64,
    pub light: Option<f32>,
    /// The rain and the ACAM's speed, for the weather's sound.
    pub weather: WeatherInputs,
}

/// A recording playing: its place in the bed's list, its voice, and since
/// when it fades in (none once it has).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layer {
    recording: usize,
    voice: u32,
    since: u64,
}

/// A bed's one-shots: its sweeteners, placed around the listener, or its
/// pulse, ahead.
#[derive(Debug)]
struct Shots {
    names: Vec<String>,
    /// Seconds between two, a span.
    every: [f32; 2],
    placed: bool,
    /// Of the bed's level.
    gain: f32,
    /// When the next plays, how many have, and the last one played.
    next: u64,
    played: u32,
    last: Option<usize>,
    /// Keeps the streams' choices apart.
    salt: u32,
}

impl Shots {
    /// The one-shot of `turn`: any but the last one played.
    fn choose(&self, world: u32, turn: u32) -> usize {
        let count = self.names.len();
        match self.last {
            Some(last) if count > 1 => (last + 1 + pick(world, turn, count - 1)) % count,
            _ => pick(world, turn, count),
        }
    }
}

/// A bed playing.
#[derive(Debug)]
struct Bed {
    world: u32,
    name: String,
    loops: Vec<String>,
    gain: f32,
    /// The recordings' level and pitch at the world's darkest light.
    light: Option<[f32; 2]>,
    /// Keeps the choices of a bed and of the one beneath it apart.
    salt: u32,
    /// The recording playing, and the one it fades out from.
    current: Option<Layer>,
    leaving: Option<Layer>,
    /// When the next recording takes over.
    next_change: u64,
    turns: u32,
    shots: Vec<Shots>,
    /// The one-shots sounding: each one's voice, gain before the menu's
    /// fade, pan, and when it ends.
    sounding: Vec<(u32, f32, f32, u64)>,
}

/// What the beds share each frame.
struct Frame<'a> {
    sounds: &'a HashMap<String, Arc<DecodedSound>>,
    next_voice: &'a mut u32,
    atmosphere: &'a AtmosphereTuning,
    now: u64,
    menu: bool,
    presence: f32,
    light: f32,
    /// The share a sweetener sends to the reverb.
    send: f32,
}

#[derive(Default)]
pub struct Ambience {
    /// The world the beds are for, and its beds: its own, then the one
    /// beneath it.
    world: Option<u32>,
    beds: Vec<Bed>,
    /// The world whose recordings these are, decoded and to come.
    decoding: Option<u32>,
    sounds: HashMap<String, Arc<DecodedSound>>,
    decoded: Option<Receiver<(String, Result<Arc<DecodedSound>, String>)>>,
    next_voice: u32,
    /// The bus's gain under the menu's fade, 0 to 1.
    presence: f32,
    /// The world's light as the beds follow it, 0 to 1.
    light: f32,
    last: u64,
    /// When the buses' levels are next logged.
    levels_at: u64,
    /// The placed sounds: what is near the camera, and what plays.
    surveyor: placed::Surveyor,
    placed: placed::Placed,
    /// How walled in the camera is as the reverb follows it, 0 to 1.
    enclosure: f32,
    /// The wind, the rain and the rain on the hull.
    weather: weather::Weather,
    /// Wait for a world's recordings to decode rather than play them as
    /// they come: an offline render (`--audio-out`) then repeats exactly.
    pub wait: bool,
}

/// The rule for the Alien Brain's worlds (0 and 19): with the
/// ambience playing (`ambience`, the Ambience in effect, above 0: Vulkan's)
/// and the pak holding (`present`) a loop of the world's bed or the one
/// beneath it, the beds play instead of the world's music (track 5).
pub fn replaces_music(
    world: u32,
    ambience: u8,
    atmosphere: &AtmosphereTuning,
    present: impl Fn(&str) -> bool,
) -> bool {
    let Some((_, bed)) = atmosphere
        .bed(world)
        .filter(|_| ambience > 0 && matches!(world, 0 | 19))
    else {
        return false;
    };
    let beneath = bed
        .under
        .as_ref()
        .and_then(|under| atmosphere.beds.get(under));
    std::iter::once(bed)
        .chain(beneath)
        .flat_map(|bed| &bed.loops)
        .any(|name| present(name))
}

/// How fast the beds fade for the in-world menu, and follow the light, a
/// second.
const MENU_FADE: f32 = 2.0;
const LIGHT_FOLLOW: f32 = 1.5;

impl Ambience {
    /// What is near the camera in `world` (none outside the ambience's play),
    /// read on the game thread after the world's draw.
    pub fn survey(
        &mut self,
        world: Option<&crate::world_backend::WorldSession>,
        residency: &crate::ResidentRuntime,
    ) {
        let tuning = crate::modern_tuning::current();
        self.surveyor.survey(world, residency, tuning.atmosphere());
    }

    /// The crystals struck this frame: a
    /// shield's ring from each, while the ambience plays, held with the
    /// crystals' `flares` (each flaring crystal's cell and flare, 0 to 1)
    /// where the rings have an attack. Called every frame.
    pub fn strike(
        &mut self,
        strikes: &[crate::crystal_flares::Strike],
        flares: &[([u16; 2], f32)],
        audio: &mut PlatformAudio,
    ) {
        let (Some(world), false) = (self.world, self.beds.is_empty()) else {
            return;
        };
        let tuning = crate::modern_tuning::current();
        let atmosphere = tuning.atmosphere();
        let Some(shields) = &atmosphere.shields else {
            return;
        };
        let survey = &self.surveyor.survey;
        let axes = [
            survey.right.unwrap_or([1.0, 0.0, 0.0]),
            survey.ahead.unwrap_or([0.0, 0.0, 1.0]),
        ];
        let mut frame = placed::PlacedFrame {
            world,
            atmosphere,
            sounds: &self.sounds,
            next_voice: &mut self.next_voice,
            now: self.last,
            elapsed: 0.0,
            menu: self.presence < 0.5,
            presence: self.presence,
            send: atmosphere.reverb.as_ref().map_or(0.0, |reverb| reverb.send),
        };
        for strike in strikes {
            self.placed
                .ring(&mut frame, shields, strike.cell, strike.offset, axes, audio);
        }
        if shields.attack.is_some() {
            self.placed
                .hold_rings(&mut frame, shields, flares, axes, audio);
        }
    }

    /// Bring the bus to what `inputs` call for.
    pub fn update(&mut self, inputs: AmbienceInputs, audio: &mut PlatformAudio) {
        let tuning = crate::modern_tuning::current();
        let atmosphere = tuning.atmosphere();
        let elapsed = inputs.now.saturating_sub(self.last) as f32 / 1.0e6;
        self.last = inputs.now;
        let world = inputs.world.filter(|_| audio.remastered().is_some());
        let light = inputs.light.unwrap_or(1.0).clamp(0.0, 1.0);
        let decoding = world.or(inputs.music.filter(|_| audio.remastered().is_some()));
        if self.decoding != decoding {
            self.decoding = decoding;
            self.decode(decoding, atmosphere, audio);
        }
        if self.world != world {
            self.world = world;
            self.enter(world, atmosphere, audio, inputs.now);
            // A world reached in the dark starts dark.
            self.light = light;
        }
        self.receive();
        let target = if inputs.menu { 0.0 } else { 1.0 };
        self.presence += (target - self.presence).clamp(-MENU_FADE * elapsed, MENU_FADE * elapsed);
        self.light += (light - self.light).clamp(-LIGHT_FOLLOW * elapsed, LIGHT_FOLLOW * elapsed);
        let music = inputs.music.is_some();
        let mut frame = Frame {
            sounds: &self.sounds,
            next_voice: &mut self.next_voice,
            atmosphere,
            now: inputs.now,
            menu: inputs.menu && !music,
            presence: if music { 1.0 } else { self.presence },
            light: self.light,
            send: atmosphere.reverb.as_ref().map_or(0.0, |reverb| reverb.send),
        };
        for bed in &mut self.beds {
            bed.update(&mut frame, audio);
        }
        if let (Some(world), false) = (self.world, self.beds.is_empty()) {
            let reverb = atmosphere.reverb.as_ref();
            let mut frame = placed::PlacedFrame {
                world,
                atmosphere,
                sounds: &self.sounds,
                next_voice: &mut self.next_voice,
                now: inputs.now,
                elapsed,
                menu: inputs.menu,
                presence: self.presence,
                send: reverb.map_or(0.0, |reverb| reverb.send),
            };
            self.placed.update(&self.surveyor.survey, &mut frame, audio);
            // The place's reverb, its tail growing as the walls close in.
            let walls = self.surveyor.survey.enclosure;
            self.enclosure += (walls - self.enclosure).clamp(-elapsed, elapsed);
            if let Some((reverb, preset)) =
                reverb.and_then(|reverb| Some((reverb, reverb.preset(world)?)))
            {
                let [open, walled] = reverb.enclosure;
                audio.shape_reverb(
                    crate::platform_audio::reverb::ReverbShape {
                        seconds: preset.seconds,
                        damping: preset.damping,
                        wet: preset.wet * (open + (walled - open) * self.enclosure),
                    },
                    reverb.retail,
                );
            }
            audio.set_shell(self.surveyor.survey.first_person);
            if let Some(tuning) = &atmosphere.weather {
                let inputs = WeatherInputs {
                    first_person: self.surveyor.survey.first_person,
                    ..inputs.weather
                };
                self.weather.update(
                    world,
                    tuning,
                    inputs,
                    &self.sounds,
                    &mut self.next_voice,
                    self.last,
                    elapsed,
                    self.presence,
                    audio,
                );
            }
        }
        // Each second of play, the buses' levels for the event log.
        if !self.beds.is_empty() && inputs.now >= self.levels_at {
            if let Some(levels) = audio.take_levels().filter(|_| self.levels_at > 0) {
                crate::event_log::ambience_levels(&levels);
            }
            self.levels_at = inputs.now + 1_000_000;
        }
    }

    /// The world in play changed: the old beds fade out, and the new ones
    /// start as their recordings decode.
    fn enter(
        &mut self,
        world: Option<u32>,
        atmosphere: &AtmosphereTuning,
        audio: &mut PlatformAudio,
        now: u64,
    ) {
        // The first levels after this hold the time before: not logged.
        self.levels_at = 0;
        for bed in self.beds.drain(..) {
            crate::event_log::ambience(&bed.name, "", "stop");
        }
        audio.stop_ambience(None);
        self.placed.clear();
        self.weather = weather::Weather::default();
        if let (Some(world), Some(pak)) = (world, audio.remastered()) {
            self.beds = beds(world, atmosphere, pak, now);
        }
    }

    /// The recordings of `world`'s beds, placed sounds and weather start
    /// decoding, those before dropped.
    fn decode(&mut self, world: Option<u32>, atmosphere: &AtmosphereTuning, audio: &PlatformAudio) {
        self.sounds.clear();
        self.decoded = None;
        let (Some(world), Some(pak)) = (world, audio.remastered().cloned()) else {
            return;
        };
        let beds = beds(world, atmosphere, &pak, 0);
        if beds.is_empty() {
            return;
        }
        let mut names: Vec<String> = Vec::new();
        let placing = placed::Placed::names(atmosphere, world);
        let raining = crate::modern_tuning::current().sky(world).precip.kind
            == crate::modern_tuning::PrecipKind::Rain;
        let weather = atmosphere.weather.as_ref().map_or_else(Vec::new, |tuning| {
            weather::Weather::names(tuning, world, raining)
        });
        for name in beds.iter().flat_map(Bed::names).chain(&placing) {
            if !names.contains(name) && pak.contains(name) {
                names.push(name.clone());
            }
        }
        // The weather's: the pak's recordings, else the loops synthesized.
        for name in weather {
            if !names.contains(&name) && (pak.contains(&name) || weather::synthesize_known(&name)) {
                names.push(name);
            }
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || decode_all(&pak, world, &names, &sender));
        self.decoded = Some(receiver);
    }

    /// The recordings decoded so far.
    fn receive(&mut self) {
        let Some(receiver) = &self.decoded else {
            return;
        };
        loop {
            let next = if self.wait {
                receiver.recv().ok()
            } else {
                receiver.try_recv().ok()
            };
            let Some((name, sound)) = next else {
                break;
            };
            match sound {
                Ok(sound) => {
                    self.sounds.insert(name, sound);
                }
                Err(error) => eprintln!("The ambience's {name} did not decode: {error}"),
            }
        }
    }
}

impl Bed {
    /// Bed `name` of `world` as `tuning` has it, with the recordings the pak
    /// holds; none without any of its loops.
    fn new(
        world: u32,
        name: &str,
        tuning: &BedTuning,
        atmosphere: &AtmosphereTuning,
        pak: &RemasteredPak,
        now: u64,
        salt: u32,
    ) -> Option<Self> {
        let present = |names: &[String]| -> Vec<String> {
            names
                .iter()
                .filter(|name| pak.contains(name))
                .cloned()
                .collect()
        };
        let loops = present(&tuning.loops);
        if loops.is_empty() {
            return None;
        }
        let mut shots = Vec::new();
        let sweeteners = present(&tuning.sweeteners);
        if !sweeteners.is_empty() {
            let every = tuning.every.unwrap_or(atmosphere.every);
            shots.push(Shots {
                names: sweeteners,
                every,
                placed: true,
                gain: 1.0,
                next: now + span(every, world, salt + 2_999),
                played: 0,
                last: None,
                salt: salt + 3_000,
            });
        }
        if let Some(pulse) = &tuning.pulse {
            let beats = present(&pulse.beats);
            if !beats.is_empty() && pulse.per_minute > 0.0 {
                let period = 60.0 / pulse.per_minute;
                shots.push(Shots {
                    names: beats,
                    every: [period, period],
                    placed: false,
                    gain: pulse.gain.max(0.0),
                    next: now,
                    played: 0,
                    last: None,
                    salt: salt + 5_000,
                });
            }
        }
        Some(Self {
            world,
            name: name.to_string(),
            loops,
            gain: tuning.gain.max(0.0),
            light: tuning.light,
            salt,
            current: None,
            leaving: None,
            next_change: now,
            turns: 0,
            shots,
            sounding: Vec::new(),
        })
    }

    /// Every recording the bed plays.
    fn names(&self) -> impl Iterator<Item = &String> {
        self.loops
            .iter()
            .chain(self.shots.iter().flat_map(|shots| &shots.names))
    }

    fn update(&mut self, frame: &mut Frame<'_>, audio: &mut PlatformAudio) {
        let (now, atmosphere) = (frame.now, frame.atmosphere);
        // The first recording, once decoded.
        if self.current.is_none() {
            let first = pick(self.world, self.salt + self.turns, self.loops.len());
            if let Some(sound) = frame.sounds.get(&self.loops[first]) {
                let voice = next_voice(frame.next_voice);
                let start = pick(
                    self.world,
                    self.salt + 1_000 + self.turns,
                    sound.frames().max(1),
                )
                .min(sound.frames().saturating_sub(1));
                audio.play_ambience_from(voice, Arc::clone(sound), true, 0.0, 0.0, start);
                self.current = Some(Layer {
                    recording: first,
                    voice,
                    since: now,
                });
                self.next_change =
                    now + change_after(atmosphere, self.world, self.salt + self.turns);
                crate::event_log::ambience(&self.name, &self.loops[first], "start");
            }
        }
        // The next recording takes over, fading in over the last.
        if let Some(current) = self.current {
            if self.loops.len() > 1 && now >= self.next_change && self.leaving.is_none() {
                let next = (current.recording + 1) % self.loops.len();
                if let Some(sound) = frame.sounds.get(&self.loops[next]) {
                    self.turns += 1;
                    let voice = next_voice(frame.next_voice);
                    let start = pick(
                        self.world,
                        self.salt + 1_000 + self.turns,
                        sound.frames().max(1),
                    )
                    .min(sound.frames().saturating_sub(1));
                    audio.play_ambience_from(voice, Arc::clone(sound), true, 0.0, 0.0, start);
                    self.leaving = Some(current);
                    self.current = Some(Layer {
                        recording: next,
                        voice,
                        since: now,
                    });
                    self.next_change =
                        now + change_after(atmosphere, self.world, self.salt + self.turns);
                    crate::event_log::ambience(&self.name, &self.loops[next], "change");
                }
            }
        }
        // The bed's gain and the level; the light and the menu's fade over
        // them.
        let still = self.gain * atmosphere.level.max(0.0);
        let [dark_level, dark_pitch] = self.light.unwrap_or([1.0, 1.0]);
        let lit = |dark: f32| dark + (1.0 - dark) * frame.light;
        let level = still * lit(dark_level) * frame.presence;
        // The layers' gains: an equal-power crossfade, the first fading in.
        let crossfade = (atmosphere.crossfade.max(0.1) * 1.0e6) as u64;
        let mut arrival = 1.0;
        if let Some(current) = self.current {
            let t = (now.saturating_sub(current.since) as f32 / crossfade as f32).min(1.0);
            let gain = if self.leaving.is_some() || self.turns == 0 {
                (t * FRAC_PI_2).sin()
            } else {
                1.0
            };
            if self.turns == 0 {
                arrival = gain;
            }
            audio.steer_ambience(current.voice, gain * level, 0.0);
            let mut layers = vec![current.voice];
            if let Some(leaving) = self.leaving {
                if t >= 1.0 {
                    audio.stop_ambience(Some(leaving.voice));
                    self.leaving = None;
                } else {
                    audio.steer_ambience(leaving.voice, (t * FRAC_PI_2).cos() * level, 0.0);
                    layers.push(leaving.voice);
                }
            }
            if self.light.is_some() {
                for voice in layers {
                    audio.pitch_ambience(voice, lit(dark_pitch));
                }
            }
        }
        // The one-shots: not in the menu, and fading with the beds for it.
        if self.current.is_some() {
            for shots in &mut self.shots {
                if now < shots.next {
                    continue;
                }
                let turn = shots.salt + shots.played;
                let index = shots.choose(self.world, turn);
                if let (Some(sound), false) = (frame.sounds.get(&shots.names[index]), frame.menu) {
                    // A sweetener somewhere around, nearer or farther; the
                    // pulse ahead. Each a little higher or lower (4%) and
                    // louder or softer (2 dB).
                    let (pan, near) = if shots.placed {
                        (
                            pick(self.world, turn + 1, 2_001) as f32 / 1_000.0 - 1.0,
                            0.4 + 0.6 * pick(self.world, turn + 2, 1_000) as f32 / 1_000.0,
                        )
                    } else {
                        (0.0, 1.0)
                    };
                    let pitch = 0.96 + pick(self.world, turn + 3, 801) as f32 / 10_000.0;
                    let decibels = pick(self.world, turn + 4, 401) as f32 / 100.0 - 2.0;
                    let gain = still * shots.gain * near * 10_f32.powf(decibels / 20.0) * arrival;
                    let voice = next_voice(frame.next_voice);
                    audio.play_ambience(
                        voice,
                        Arc::clone(sound),
                        false,
                        gain * frame.presence,
                        pan,
                    );
                    audio.pitch_ambience(voice, pitch);
                    if shots.placed {
                        audio.send_ambience(voice, frame.send);
                    }
                    let seconds =
                        sound.frames() as f32 / crate::platform_audio::OUTPUT_RATE as f32 / pitch;
                    self.sounding
                        .push((voice, gain, pan, now + (seconds * 1.0e6) as u64));
                    shots.last = Some(index);
                    let what = if shots.placed { "sweetener" } else { "pulse" };
                    crate::event_log::ambience(&self.name, &shots.names[index], what);
                }
                shots.played += 1;
                shots.next = now + span(shots.every, self.world, turn);
            }
        }
        self.sounding.retain(|&(.., ends)| ends > now);
        for &(voice, gain, pan, _) in &self.sounding {
            audio.steer_ambience(voice, gain * frame.presence, pan);
        }
    }
}

/// `world`'s beds as the pak holds them: its own, then the one beneath it.
fn beds(world: u32, atmosphere: &AtmosphereTuning, pak: &RemasteredPak, now: u64) -> Vec<Bed> {
    let Some(own) = atmosphere.bed(world) else {
        return Vec::new();
    };
    let beneath = own.1.under.as_ref().and_then(|under| {
        atmosphere
            .beds
            .get_key_value(under)
            .map(|(name, bed)| (name.as_str(), bed))
    });
    std::iter::once(own)
        .chain(beneath)
        .enumerate()
        .filter_map(|(salt, (name, tuning))| {
            Bed::new(
                world,
                name,
                tuning,
                atmosphere,
                pak,
                now,
                salt as u32 * 10_000,
            )
        })
        .collect()
}

fn decode_all(
    pak: &RemasteredPak,
    world: u32,
    names: &[String],
    sender: &mpsc::Sender<(String, Result<Arc<DecodedSound>, String>)>,
) {
    for name in names {
        let sound = match weather::synthesize(name, world).filter(|_| !pak.contains(name)) {
            Some(sound) => Ok(Arc::new(sound)),
            None => pak.decode(name).map_err(|error| error.to_string()),
        };
        if sender.send((name.clone(), sound)).is_err() {
            return;
        }
    }
}

fn next_voice(counter: &mut u32) -> u32 {
    *counter = counter.wrapping_add(1);
    *counter
}

/// A choice among `count` for the world's `turn`: the same in every run, so
/// a fixed-step run sounds the same.
fn pick(world: u32, turn: u32, count: usize) -> usize {
    let mut hash = 0x811c_9dc5_u32;
    for byte in world.to_le_bytes().into_iter().chain(turn.to_le_bytes()) {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    // FNV's low bits follow the input's parity; murmur3's finish mixes them.
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^= hash >> 16;
    hash as usize % count.max(1)
}

/// How long the recording of `turn` plays before the next takes over.
fn change_after(atmosphere: &AtmosphereTuning, world: u32, turn: u32) -> u64 {
    span(atmosphere.change, world, 2_000 + turn)
}

/// A time within `[least, most]` seconds, in microseconds, the same each run.
fn span([least, most]: [f32; 2], world: u32, turn: u32) -> u64 {
    let share = pick(world, turn, 1_000) as f32 / 1_000.0;
    ((least + (most - least).max(0.0) * share).max(0.25) * 1.0e6) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The choices are the same every run and spread over their range.
    #[test]
    fn the_choices_repeat_and_spread() {
        assert_eq!(pick(9, 3, 7), pick(9, 3, 7));
        let spread: std::collections::BTreeSet<_> = (0..64).map(|turn| pick(9, turn, 3)).collect();
        assert_eq!(spread.len(), 3);
        let tuning = crate::modern_tuning::ModernTuning::defaults();
        let atmosphere = tuning.atmosphere();
        for turn in 0..32 {
            let after = change_after(atmosphere, 2, turn);
            assert!((120_000_000..=240_000_000).contains(&after));
            let sweetener = span(atmosphere.every, 2, 3_000 + turn);
            assert!((8_000_000..=40_000_000).contains(&sweetener));
        }
    }

    /// A stream of one-shots never plays the same one twice running, and
    /// reaches every one.
    #[test]
    fn one_shots_take_turns() {
        let mut shots = Shots {
            names: ["a", "b", "c"].map(String::from).to_vec(),
            every: [8.0, 40.0],
            placed: true,
            gain: 1.0,
            next: 0,
            played: 0,
            last: None,
            salt: 3_000,
        };
        let mut seen = std::collections::BTreeSet::new();
        for turn in 0..60 {
            let index = shots.choose(16, 3_000 + turn);
            assert_ne!(Some(index), shots.last);
            seen.insert(index);
            shots.last = Some(index);
        }
        assert_eq!(seen.len(), 3);
    }

    /// The Alien Brain's beds play instead of its music (0 and 19) with the
    /// pak holding them and the Ambience up under Vulkan; without them, at
    /// Ambience 0, under OpenGL or software (also standing in for a Vulkan
    /// that failed to start), and on every other world, the music plays as
    /// retail's.
    #[test]
    fn the_brains_beds_replace_its_music() {
        use crate::presentation::{RendererBackend, RendererChoice};
        let tuning = crate::modern_tuning::ModernTuning::defaults();
        let atmosphere = tuning.atmosphere();
        let all = |_: &str| true;
        let no_brain = |name: &str| !name.starts_with("beds/brain");
        for world in 0..=33 {
            let brain = matches!(world, 0 | 19);
            assert_eq!(replaces_music(world, 10, atmosphere, all), brain);
            assert!(!replaces_music(world, 10, atmosphere, no_brain));
            assert!(!replaces_music(world, 0, atmosphere, all));
        }
        // The swirl beneath alone is the Brain's ambience too.
        let swirl = |name: &str| name == "beds/brain-swirl.ogg";
        assert!(replaces_music(19, 1, atmosphere, swirl));
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let assets =
            crate::frontend_assets::FrontendAssets::open(&manifest.join("../../retail")).unwrap();
        // Never written: a missing file loads the defaults.
        let directory = manifest.join("../../.tmp/inf-settings-tests");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("brain-music-{}.json", std::process::id()));
        assert!(!path.exists());
        let mut settings =
            crate::frontend_settings::FrontendSettings::load_or_defaults(path, &assets).unwrap();
        let mut modern = settings.modern();
        modern.ambience = 10;
        settings.set_modern(modern);
        // The renderer chosen, and the one running (none: the window not yet
        // open).
        for (renderer, running, replaced) in [
            (RendererChoice::Vulkan, None, true),
            (RendererChoice::OpenGl, None, false),
            (RendererChoice::Software, None, false),
            (RendererChoice::Vulkan, Some(RendererBackend::Vulkan), true),
            (RendererChoice::Vulkan, Some(RendererBackend::OpenGl), false),
            (
                RendererChoice::Vulkan,
                Some(RendererBackend::Software),
                false,
            ),
            (RendererChoice::OpenGl, Some(RendererBackend::OpenGl), false),
        ] {
            settings.set_renderer(renderer);
            if let Some(backend) = running {
                settings.set_running(backend);
            }
            for world in [0, 19] {
                let ambience = settings.ambience();
                assert_eq!(replaces_music(world, ambience, atmosphere, all), replaced);
            }
        }
    }

    /// Every world 0 to 24 has a bed, and each bed's recordings, the bed
    /// beneath it and its beats, and every placed sound and its one-shots,
    /// are in the manifest the pak is built from.
    #[test]
    fn every_world_has_a_bed_from_the_manifest() {
        let tuning = crate::modern_tuning::ModernTuning::defaults();
        let atmosphere = tuning.atmosphere();
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../ambience/manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let names: Vec<&str> = manifest["sounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sound| sound["name"].as_str().unwrap())
            .collect();
        for world in 0..=24 {
            let (_, bed) = atmosphere.bed(world).expect("a bed");
            let beneath = bed.under.as_ref().map(|under| &atmosphere.beds[under]);
            for bed in std::iter::once(bed).chain(beneath) {
                let beats = bed.pulse.iter().flat_map(|pulse| &pulse.beats);
                assert!(bed
                    .loops
                    .iter()
                    .chain(&bed.sweeteners)
                    .chain(beats)
                    .all(|name| names.contains(&name.as_str())));
            }
        }
        let placed = atmosphere.sources.iter().map(|source| &source.place);
        let shields = atmosphere.shields.iter();
        for place in placed.chain(atmosphere.shores.values()).chain(shields) {
            assert!(
                place
                    .sounds
                    .iter()
                    .chain(&place.shots)
                    .all(|name| names.contains(&name.as_str())),
                "{place:?}"
            );
        }
    }
}
