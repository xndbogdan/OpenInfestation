//! The ambience's placed sounds (step 3 of
//! [the plan](../../../../docs/rt-plans/atmosphere.md)): the machines the
//! tuning names (generators, power transformers, Foldrak Noir's lightning
//! spheres, steam vents), the force fields' domes and the barriers' lines,
//! and the shores of a world's water, red liquid or lava, each heard from
//! where it is.
//!
//! Each frame the survey, on the game thread after the world's draw, lists
//! what is near the camera: the arena's actors and the terrain's furniture
//! by model name, a field by its fade, a barrier's nearest point on its
//! line, and the nearest shore in each quarter around the camera, from a
//! map of the world made as it loads (64 by 64 cells, a shore where the
//! ground crosses the plane; again when the plane moves, as world 12's
//! reservoir rises). The sixteen loudest play, each a looping recording
//! from a point its place picks, panned by its direction from the camera
//! (retail's listener), quieter and duller with distance, fading in and out
//! over half a second. A ridge between a sound and the camera makes it 6 dB
//! quieter and much duller over 0.3 s, and every placed sound sends a share
//! to the place's reverb, whose tail grows as the ground walls the camera
//! in.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::modern_tuning::{AtmosphereTuning, PlacedTuning};
use crate::object_runtime::ObjectId;
use crate::platform_audio::remastered_pak::DecodedSound;
use crate::platform_audio::PlatformAudio;
use crate::residency::ResidentRuntime;
use crate::terrain_runtime::TerrainRuntime;
use crate::world_backend::WorldSession;

/// The shore map's side in cells, each 1,024 world units square.
const MAP: usize = 64;
/// The voices placed sounds take at most.
const VOICES: usize = 16;
/// How fast a placed sound fades in or out, a second.
const FADE: f32 = 2.0;
/// A sound playing ranks this much louder when the loudest are chosen, so
/// voices do not flicker in and out.
const KEEP: f32 = 1.5;
/// Nearer than this a sound pans toward the middle, and from inside a
/// field's dome its hum is all around.
const NEAR: f32 = 300.0;
/// The plane moved this far: the shores are found again.
const PLANE_MOVED: i32 = 8;
/// A sound comes from this far above its place (a machine's body, not its
/// base), as the ground is tested between it and the camera.
const SOURCE_RISE: f32 = 100.0;
/// How fast a sound goes behind a ridge or comes out, a second (0.3 s).
const OCCLUSION_RATE: f32 = 1.0 / 0.3;
/// The walls the enclosure looks for: the ground this far around the camera.
const ENCLOSURE_REACH: f32 = 800.0;

/// Where a placed sound comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Place {
    Actor(ObjectId),
    Furniture(usize),
    /// The nearest shore in a quarter around the camera.
    Shore(u8),
}

/// A sound to place this frame: its tuning (a source's index, none for
/// the world's shore), where it is from the camera (world units, Y up) and
/// how far, how strongly it sounds (a field's fade), and whether the ground
/// rises between it and the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Candidate {
    pub place: Place,
    pub source: Option<usize>,
    pub offset: [f32; 3],
    pub distance: f32,
    pub strength: f32,
    pub occluded: bool,
}

/// What the survey found this frame: the camera's right axis (none before
/// a draw), the sounds to place, how walled in the camera is (0 to 1: the
/// share of eight points around it whose ground stands above its eye), and
/// whether it looks from inside the vehicle (first person).
#[derive(Debug, Default)]
pub(crate) struct Survey {
    pub right: Option<[f32; 3]>,
    /// The camera's forward axis, for the headphones' front and back.
    pub ahead: Option<[f32; 3]>,
    pub candidates: Vec<Candidate>,
    pub enclosure: f32,
    pub first_person: bool,
}

/// The survey, with what a world keeps: the sources its model names match,
/// its furniture's, and its shores.
#[derive(Debug, Default)]
pub(crate) struct Surveyor {
    world: Option<u32>,
    names: HashMap<u16, Option<usize>>,
    furniture: Vec<(usize, [u16; 3])>,
    plane: Option<i32>,
    shores: Vec<[f32; 3]>,
    pub survey: Survey,
}

impl Surveyor {
    /// This frame's sounds to place in `world` (none outside play).
    pub(crate) fn survey(
        &mut self,
        world: Option<&WorldSession>,
        residency: &ResidentRuntime,
        atmosphere: &AtmosphereTuning,
    ) {
        self.survey.right = None;
        self.survey.ahead = None;
        self.survey.candidates.clear();
        self.survey.first_person = false;
        let Some(world) = world else {
            self.world = None;
            return;
        };
        let number = world.mission.current_world_a0;
        if self.world != Some(number) {
            *self = Self {
                world: Some(number),
                ..Self::default()
            };
            self.furniture = furniture(world, residency, atmosphere, &mut self.names);
        }
        let plane = world.terrain.plane_base() >> world.terrain.plane_shift();
        if atmosphere.shores.contains_key(&number)
            && self
                .plane
                .is_none_or(|last| (plane - last).abs() >= PLANE_MOVED)
        {
            self.plane = Some(plane);
            self.shores = shores(&world.terrain, plane);
        }
        let Some(draw) = world.last_draw else {
            return;
        };
        self.survey.right = Some([1, 2, 3].map(|word| f32::from_bits(draw.basis_words[word])));
        self.survey.ahead = Some([7, 8, 9].map(|word| f32::from_bits(draw.basis_words[word])));
        let eye = draw.eye;
        let mut push = |place, source: usize, offset: [f32; 3], distance, strength| {
            if distance < atmosphere.sources[source].place.range {
                self.survey.candidates.push(Candidate {
                    place,
                    source: Some(source),
                    offset,
                    distance,
                    strength,
                    occluded: false,
                });
            }
        };
        let kind = |name: &str| {
            atmosphere
                .sources
                .iter()
                .position(|source| source.kind.as_deref() == Some(name))
        };
        let (field, barrier) = (kind("force_field"), kind("barrier"));
        let arena = &world.arena;
        for &id in arena.actor_list() {
            let Some(position) = arena
                .object(id)
                .ok()
                .and_then(|object| object.base.position_words)
            else {
                continue;
            };
            let offset = offset_from(eye, position);
            if let (Some(source), Ok(state)) = (field, arena.selector18(id)) {
                // Its dome's wall: within it, the hum is all around.
                let across = offset[0].hypot(offset[2]);
                let distance = (across - state.radius_154 as f32).max(0.0);
                let strength = (state.fade_160 as f32 / 500_000.0).clamp(0.0, 1.0);
                push(Place::Actor(id), source, offset, distance, strength);
            } else if let (Some(source), Ok(posts)) = (barrier, arena.selector69_posts(id)) {
                let line = nearest_on_line(offset_from(eye, posts[0]), offset_from(eye, posts[1]));
                push(Place::Actor(id), source, line, length(line), 1.0);
            } else if let Ok(actor) = arena.actor(id) {
                let word = actor.rotating.visual.model_word;
                if let Some(source) = named(residency, atmosphere, &mut self.names, word) {
                    push(Place::Actor(id), source, offset, length(offset), 1.0);
                }
            }
        }
        for (index, &(source, position)) in self.furniture.iter().enumerate() {
            let offset = offset_from(eye, position);
            push(Place::Furniture(index), source, offset, length(offset), 1.0);
        }
        if let Some(shore) = atmosphere.shores.get(&number) {
            let mut quarters: [Option<([f32; 3], f32)>; 4] = [None; 4];
            for &point in &self.shores {
                let position = point.map(|value| value as i32 as u16);
                let offset = offset_from(eye, [position[0], point[1] as i16 as u16, position[2]]);
                let distance = length(offset);
                let quarter = usize::from(offset[0] >= 0.0) | usize::from(offset[2] >= 0.0) << 1;
                if distance < shore.range
                    && quarters[quarter].is_none_or(|(_, nearest)| distance < nearest)
                {
                    quarters[quarter] = Some((offset, distance));
                }
            }
            for (quarter, nearest) in quarters.into_iter().enumerate() {
                if let Some((offset, distance)) = nearest {
                    self.survey.candidates.push(Candidate {
                        place: Place::Shore(quarter as u8),
                        source: None,
                        offset,
                        distance,
                        strength: 1.0,
                        occluded: false,
                    });
                }
            }
        }
        let terrain = &world.terrain;
        for candidate in &mut self.survey.candidates {
            candidate.occluded = occluded(terrain, eye, candidate.offset);
        }
        self.survey.enclosure = enclosure(terrain, eye);
        self.survey.first_person = world.arena.camera_first_person_peek();
    }
}

/// How walled in the camera at `eye` is: the share of eight points around
/// it whose ground stands above its eye.
fn enclosure(terrain: &TerrainRuntime, eye: [i32; 3]) -> f32 {
    let walls = (0..8)
        .filter(|&index| {
            let angle = index as f32 * std::f32::consts::FRAC_PI_4;
            let x = eye[0] as f32 + ENCLOSURE_REACH * angle.cos();
            let z = eye[2] as f32 + ENCLOSURE_REACH * angle.sin();
            i32::from(terrain.height_at([x as i32 as u16, z as i32 as u16])) > eye[1]
        })
        .count();
    walls as f32 / 8.0
}

/// Whether the ground rises between the camera at `eye` and a sound
/// `offset` from it: fifteen points along the line between, short of its
/// ends, against the terrain's height.
fn occluded(terrain: &TerrainRuntime, eye: [i32; 3], offset: [f32; 3]) -> bool {
    (1..16).any(|step| {
        let share = step as f32 / 16.0;
        let x = (eye[0] as f32 + offset[0] * share) as i32 as u16;
        let z = (eye[2] as f32 + offset[2] * share) as i32 as u16;
        let line = eye[1] as f32 + (offset[1] + SOURCE_RISE) * share;
        f32::from(terrain.height_at([x, z])) > line + 32.0
    })
}

/// The sources of the terrain's furniture, and where each stands.
fn furniture(
    world: &WorldSession,
    residency: &ResidentRuntime,
    atmosphere: &AtmosphereTuning,
    names: &mut HashMap<u16, Option<usize>>,
) -> Vec<(usize, [u16; 3])> {
    let Ok(table) = crate::world_furniture_effect::resident::resident_placement_table(residency)
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for bucket in world.terrain.buckets() {
        for record in &bucket.records {
            let lookup = usize::from(record.resource_lookup.raw());
            let Some(slot) = table.slots.get(lookup).filter(|_| lookup != 0) else {
                continue;
            };
            if let Some(source) = named(residency, atmosphere, names, slot.resource_word) {
                // 0044CEF0's place: the record's cell, at its height.
                let [x, z] = [record.x << 7, bucket.y << 7];
                found.push((source, [x, world.terrain.raw_height_at([x, z]) as u16, z]));
            }
        }
    }
    found
}

/// The source whose models name H8 model `word`, if any.
fn named(
    residency: &ResidentRuntime,
    atmosphere: &AtmosphereTuning,
    names: &mut HashMap<u16, Option<usize>>,
    word: u16,
) -> Option<usize> {
    *names.entry(word).or_insert_with(|| {
        let binding = crate::object_runtime::resident_model_resource(residency, word).ok()??;
        let name = binding
            .model
            .name
            .as_ref()
            .and_then(|name| name.ascii(binding.allocation.bytes()))?;
        atmosphere
            .sources
            .iter()
            .position(|source| source.models.iter().any(|model| model == name))
    })
}

/// The world's shores at `plane`: in each of 64 by 64 cells the ground
/// crosses, the middle of the grid points where it does (a point wet, below
/// the plane as retail's water draw has it, beside a dry one), at the
/// plane's height.
fn shores(terrain: &TerrainRuntime, plane: i32) -> Vec<[f32; 3]> {
    const SIDE: usize = 512;
    let step = usize::from(terrain.sampling().spacing.units() >> 7).max(1);
    let wet = |x: usize, z: usize| {
        i32::from(terrain.raw_height_at([((x % SIDE) << 7) as u16, ((z % SIDE) << 7) as u16]))
            < plane
    };
    let per = SIDE / MAP;
    let mut found = Vec::new();
    for cell_x in 0..MAP {
        for cell_z in 0..MAP {
            let (mut sum, mut count) = ([0.0_f32; 2], 0.0_f32);
            for x in (cell_x * per..(cell_x + 1) * per).step_by(step) {
                for z in (cell_z * per..(cell_z + 1) * per).step_by(step) {
                    let here = wet(x, z);
                    if here != wet(x + step, z) || here != wet(x, z + step) {
                        sum = [sum[0] + x as f32, sum[1] + z as f32];
                        count += 1.0;
                    }
                }
            }
            if count > 0.0 {
                found.push([sum[0] / count * 128.0, plane as f32, sum[1] / count * 128.0]);
            }
        }
    }
    found
}

/// `position` from the camera at `eye`, the ground's X and Z wrapping at 16
/// bits as retail's do.
fn offset_from(eye: [i32; 3], position: [u16; 3]) -> [f32; 3] {
    let across = |axis: usize| f32::from(position[axis].wrapping_sub(eye[axis] as u16) as i16);
    [
        across(0),
        (i32::from(position[1] as i16) - eye[1]) as f32,
        across(2),
    ]
}

fn length(offset: [f32; 3]) -> f32 {
    (offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2]).sqrt()
}

/// The point of the line from `a` to `b` nearest the camera (the origin).
fn nearest_on_line(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let along = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let squared = along[0] * along[0] + along[1] * along[1] + along[2] * along[2];
    let t = if squared > 0.0 {
        (-(a[0] * along[0] + a[1] * along[1] + a[2] * along[2]) / squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [0, 1, 2].map(|axis| a[axis] + along[axis] * t)
}

/// How loud a sound `distance` away is, out of its `range`: all of it at
/// its place, fading smoothly to none at its range.
fn loudness(distance: f32, range: f32) -> f32 {
    let near = (1.0 - distance / range.max(1.0)).clamp(0.0, 1.0);
    near * near
}

/// The low-pass's share of each new sample for a sound `distance` away: a
/// cutoff from 16 kHz at its place down to 2.4 kHz at its range.
fn dullness(distance: f32, range: f32) -> f32 {
    let cutoff = 16_000.0 * 0.15_f32.powf((distance / range.max(1.0)).clamp(0.0, 1.0));
    1.0 - (-std::f32::consts::TAU * cutoff / crate::platform_audio::OUTPUT_RATE as f32).exp()
}

/// How far ahead of the camera a sound `distance` away is (-1 behind to 1
/// ahead), along its forward axis `ahead`: the headphones' front and back.
fn ahead_share(offset: [f32; 3], distance: f32, ahead: [f32; 3]) -> f32 {
    if distance < 1.0 {
        return 1.0;
    }
    let along = offset[0] * ahead[0] + offset[1] * ahead[1] + offset[2] * ahead[2];
    (along / distance).clamp(-1.0, 1.0)
}

/// Where a sound sits between the speakers (-1 left to 1 right): its
/// direction along the camera's right axis, toward the middle when near.
fn pan(offset: [f32; 3], distance: f32, right: [f32; 3]) -> f32 {
    if distance < 1.0 {
        return 0.0;
    }
    let side = (offset[0] * right[0] + offset[1] * right[1] + offset[2] * right[2]) / distance;
    side.clamp(-1.0, 1.0) * (distance / NEAR).min(1.0)
}

/// A placed sound playing.
#[derive(Debug)]
struct Voice {
    id: u32,
    source: Option<usize>,
    sound: String,
    /// Its fade in or out, 0 to 1.
    presence: f32,
    /// Its loudness, pan and dullness as last heard, whether a ridge hides
    /// it, and how far its sound has gone behind one (0 to 1).
    loudness: f32,
    pan: f32,
    cut: f32,
    occluded: bool,
    occlusion: f32,
    /// Its share to the right and ahead, for the headphones' ears.
    aim: [f32; 2],
    /// When its next one-shot plays, how many have, and the last.
    next_shot: u64,
    shots: u32,
    last_shot: Option<usize>,
}

/// What the placed sounds share each frame.
pub(crate) struct PlacedFrame<'a> {
    pub world: u32,
    pub atmosphere: &'a AtmosphereTuning,
    pub sounds: &'a HashMap<String, Arc<DecodedSound>>,
    pub next_voice: &'a mut u32,
    pub now: u64,
    pub elapsed: f32,
    pub menu: bool,
    pub presence: f32,
    /// The share each placed sound sends to the reverb.
    pub send: f32,
}

/// The placed sounds playing, in the order they started (so a fixed-step
/// run mixes them the same way each time), and their one-shots.
#[derive(Debug, Default)]
pub(crate) struct Placed {
    voices: Vec<(Place, Voice)>,
    sounding: Vec<(u32, f32, f32, f32, u64)>,
    /// The crystals' rings so far, and the last one rung.
    rung: u32,
    last_rung: Option<usize>,
    /// The crystals' rings held with their flares ([`PlacedTuning::attack`]).
    rings: Vec<Ring>,
    /// The crystals' one-shot rings sounding: each one's voice, when it
    /// began and when it runs out.
    ringing: Vec<(u32, u64, u64)>,
}

/// The most crystal rings sounding at once (2026-10-31: a burst into a
/// cluster stacked a ring per bolt); a newer one cuts off the oldest.
const MOST_RINGS: usize = 3;
/// A strike this soon (microseconds) after the last ring began adds none:
/// a twin cannon's pair of bolts rings once.
const RING_GAP: u64 = 80_000;

/// Whether a crystal's new ring at `now` plays beside the rings sounding
/// (`ringing`, oldest first: voice, began, ends): none within the gap after
/// the last, unless its crystal is `taking_over` its own ring; otherwise
/// whether it cuts off the oldest.
fn admit_ring(ringing: &[(u32, u64, u64)], now: u64, taking_over: bool) -> Option<bool> {
    if taking_over {
        return Some(false);
    }
    if ringing
        .last()
        .is_some_and(|&(_, began, _)| now.saturating_sub(began) < RING_GAP)
    {
        return None;
    }
    Some(ringing.len() >= MOST_RINGS)
}

/// A crystal's ring that lasts as long as its flash: it rises over the
/// attack, then follows the crystal's flare down to nothing; a newer ring of
/// the same crystal takes over from it over the attack.
#[derive(Debug)]
struct Ring {
    id: u32,
    cell: [u16; 2],
    offset: [f32; 3],
    gain: f32,
    pan: f32,
    /// When it began, and when its recording runs out at its pitch.
    started: u64,
    ends: u64,
    /// When a newer ring of its crystal began to take over from it.
    released: Option<u64>,
}

/// A held ring's loudness, 0 to 1, `age` seconds after it began: up over
/// `attack`, times its crystal's flare `level`, and down over `attack`
/// again `released` seconds after a newer ring took over.
fn ring_envelope(age: f32, attack: f32, level: f32, released: Option<f32>) -> f32 {
    // No attack: whole at once, and gone at once when a newer ring takes
    // over (the mixer's own short fade keeps it from clicking).
    let share = |seconds: f32| {
        if attack > 0.0 {
            (seconds / attack).clamp(0.0, 1.0)
        } else {
            1.0
        }
    };
    let up = share(age);
    let down = released.map_or(1.0, |since| 1.0 - share(since));
    up * level.clamp(0.0, 1.0) * down
}

impl Placed {
    /// The voice playing from `place`, if any.
    fn voice(&mut self, place: Place) -> Option<&mut Voice> {
        self.voices
            .iter_mut()
            .find(|(playing, _)| *playing == place)
            .map(|(_, voice)| voice)
    }

    /// Forget every voice (the bus has stopped them).
    pub(crate) fn clear(&mut self) {
        self.voices.clear();
        self.sounding.clear();
        self.rings.clear();
        self.ringing.clear();
    }

    /// Every recording a world's placed sounds may play.
    pub(crate) fn names(atmosphere: &AtmosphereTuning, world: u32) -> Vec<String> {
        let tunings = atmosphere
            .sources
            .iter()
            .map(|source| &source.place)
            .chain(atmosphere.shores.get(&world))
            .chain(&atmosphere.shields);
        tunings
            .flat_map(|place| place.sounds.iter().chain(&place.shots))
            .cloned()
            .collect()
    }

    /// Bring the placed sounds to what `survey` found.
    pub(crate) fn update(
        &mut self,
        survey: &Survey,
        frame: &mut PlacedFrame<'_>,
        audio: &mut PlatformAudio,
    ) {
        let atmosphere = frame.atmosphere;
        let tuning = |source: Option<usize>| -> Option<&PlacedTuning> {
            match source {
                Some(index) => atmosphere.sources.get(index).map(|source| &source.place),
                None => atmosphere.shores.get(&frame.world),
            }
        };
        // The loudest, a sound playing ranking higher.
        let mut ranked: Vec<(f32, &Candidate)> = survey
            .candidates
            .iter()
            .filter_map(|candidate| {
                let place = tuning(candidate.source)?;
                let mut rank = place.gain * loudness(candidate.distance, place.range);
                rank *= candidate.strength;
                if self.voice(candidate.place).is_some() {
                    rank *= KEEP;
                }
                (rank > 0.0).then_some((rank, candidate))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        ranked.truncate(VOICES);
        let right = survey.right.unwrap_or([1.0, 0.0, 0.0]);
        let forward = survey.ahead.unwrap_or([0.0, 0.0, 1.0]);
        for (_, voice) in &mut self.voices {
            voice.loudness = 0.0;
        }
        for (_, candidate) in ranked {
            let Some(place) = tuning(candidate.source) else {
                continue;
            };
            let loud = place.gain * loudness(candidate.distance, place.range) * candidate.strength;
            let pan = pan(candidate.offset, candidate.distance, right);
            let aim = [
                pan,
                ahead_share(candidate.offset, candidate.distance, forward),
            ];
            let cut = dullness(candidate.distance, place.range);
            if let Some(voice) = self.voice(candidate.place) {
                (voice.loudness, voice.pan, voice.cut, voice.aim) = (loud, pan, cut, aim);
                voice.occluded = candidate.occluded;
                continue;
            }
            if self.voices.len() >= VOICES || place.sounds.is_empty() {
                continue;
            }
            let key = key(candidate.place);
            let name = &place.sounds[super::pick(frame.world, key, place.sounds.len())];
            let Some(sound) = frame.sounds.get(name) else {
                continue;
            };
            let id = super::next_voice(frame.next_voice);
            let start = super::pick(frame.world, key ^ 0x5bd1_e995, sound.frames().max(1))
                .min(sound.frames().saturating_sub(1));
            audio.play_ambience_from(id, Arc::clone(sound), true, 0.0, pan, start);
            audio.filter_ambience(id, cut);
            audio.send_ambience(id, frame.send);
            audio.aim_ambience(id, aim);
            let label = candidate
                .source
                .and_then(|index| atmosphere.sources.get(index))
                .map_or("shore", |source| {
                    source
                        .models
                        .first()
                        .map(String::as_str)
                        .or(source.kind.as_deref())
                        .unwrap_or("source")
                });
            crate::event_log::ambience(label, name, "placed");
            let every = place.every.unwrap_or(atmosphere.every);
            self.voices.push((
                candidate.place,
                Voice {
                    id,
                    source: candidate.source,
                    sound: name.clone(),
                    presence: 0.0,
                    loudness: loud,
                    pan,
                    cut,
                    aim,
                    occluded: candidate.occluded,
                    occlusion: if candidate.occluded { 1.0 } else { 0.0 },
                    next_shot: frame.now + super::span(every, frame.world, key),
                    shots: 0,
                    last_shot: None,
                },
            ));
        }
        // Each voice fades toward being heard or not, and goes once silent.
        let level = atmosphere.level.max(0.0);
        let step = FADE * frame.elapsed;
        let mut gone = Vec::new();
        for (place, voice) in &mut self.voices {
            let target = if voice.loudness > 0.0 { 1.0 } else { 0.0 };
            voice.presence += (target - voice.presence).clamp(-step, step);
            if voice.presence <= 0.0 && voice.loudness <= 0.0 {
                audio.stop_ambience(Some(voice.id));
                crate::event_log::ambience("", &voice.sound, "gone");
                gone.push(*place);
                continue;
            }
            let hidden = if voice.occluded { 1.0 } else { 0.0 };
            let rate = OCCLUSION_RATE * frame.elapsed;
            voice.occlusion += (hidden - voice.occlusion).clamp(-rate, rate);
            // Behind a ridge: 6 dB down, and much duller.
            let (shade, cut) = (
                1.0 - 0.5 * voice.occlusion,
                voice.cut * (1.0 - 0.8 * voice.occlusion),
            );
            let gain = level * voice.loudness.max(0.0) * voice.presence * frame.presence * shade;
            audio.steer_ambience(voice.id, gain, voice.pan);
            audio.aim_ambience(voice.id, voice.aim);
            audio.filter_ambience(voice.id, cut);
            // Its one-shots from the same place, not in the menu.
            let Some(place_tuning) = tuning(voice.source) else {
                continue;
            };
            if place_tuning.shots.is_empty() || frame.now < voice.next_shot {
                continue;
            }
            let key = key(*place).wrapping_add(voice.shots);
            let count = place_tuning.shots.len();
            let index = match voice.last_shot {
                Some(last) if count > 1 => {
                    (last + 1 + super::pick(frame.world, key, count - 1)) % count
                }
                _ => super::pick(frame.world, key, count),
            };
            let name = &place_tuning.shots[index];
            if let (Some(sound), false, true) =
                (frame.sounds.get(name), frame.menu, voice.loudness > 0.0)
            {
                let shot = super::next_voice(frame.next_voice);
                let pitch = 0.96 + super::pick(frame.world, key ^ 3, 801) as f32 / 10_000.0;
                let gain = level * voice.loudness * shade;
                audio.play_ambience(
                    shot,
                    Arc::clone(sound),
                    false,
                    gain * frame.presence,
                    voice.pan,
                );
                audio.pitch_ambience(shot, pitch);
                audio.filter_ambience(shot, cut);
                audio.send_ambience(shot, frame.send);
                audio.aim_ambience(shot, voice.aim);
                let seconds =
                    sound.frames() as f32 / crate::platform_audio::OUTPUT_RATE as f32 / pitch;
                self.sounding.push((
                    shot,
                    gain,
                    voice.pan,
                    cut,
                    frame.now + (seconds * 1.0e6) as u64,
                ));
                voice.last_shot = Some(index);
                crate::event_log::ambience(&voice.sound, name, "shot");
            }
            voice.shots += 1;
            let every = place_tuning.every.unwrap_or(atmosphere.every);
            voice.next_shot = frame.now + super::span(every, frame.world, key);
        }
        self.voices.retain(|(place, _)| !gone.contains(place));
        let now = frame.now;
        self.sounding.retain(|&(.., ends)| ends > now);
        for &(shot, gain, pan, cut, _) in &self.sounding {
            audio.steer_ambience(shot, gain * frame.presence, pan);
            audio.filter_ambience(shot, cut);
        }
    }
}

impl Placed {
    /// A crystal's ring from `offset`: one of
    /// `place`'s sounds, any but the last rung, a little higher or lower,
    /// placed as the placed sounds are; not in the menu. With an attack it
    /// is held with the flare of the crystal at `cell`
    /// ([`Self::hold_rings`]), taking over from that crystal's last ring;
    /// without, a one-shot.
    pub(crate) fn ring(
        &mut self,
        frame: &mut PlacedFrame<'_>,
        place: &PlacedTuning,
        cell: [u16; 2],
        offset: [f32; 3],
        [right, forward]: [[f32; 3]; 2],
        audio: &mut PlatformAudio,
    ) {
        let distance = length(offset);
        if place.sounds.is_empty() || frame.menu || distance >= place.range {
            return;
        }
        let held = place.attack.is_some();
        let now = frame.now;
        let rings = &self.rings;
        self.ringing.retain(|&(id, _, ends)| {
            ends > now && (!held || rings.iter().any(|ring| ring.id == id))
        });
        // A crystal already ringing takes its new ring over from the last;
        // another one waits out the gap.
        let taking_over = held && self.rings.iter().any(|ring| ring.cell == cell);
        let Some(evict) = admit_ring(&self.ringing, now, taking_over) else {
            return;
        };
        if evict {
            let (oldest, ..) = self.ringing.remove(0);
            audio.stop_ambience(Some(oldest));
            self.sounding.retain(|&(id, ..)| id != oldest);
            self.rings.retain(|ring| ring.id != oldest);
        }
        self.rung = self.rung.wrapping_add(1);
        let turn = 7_000_u32.wrapping_add(self.rung);
        let count = place.sounds.len();
        let index = match self.last_rung {
            Some(last) if count > 1 => {
                (last + 1 + super::pick(frame.world, turn, count - 1)) % count
            }
            _ => super::pick(frame.world, turn, count),
        };
        let name = &place.sounds[index];
        let Some(sound) = frame.sounds.get(name) else {
            return;
        };
        self.last_rung = Some(index);
        // An impact, heard as retail's effects are (the Sound slider's
        // volume, not dipped under them; `AmbienceVoice::effect`), not at
        // the beds' level: under the ambience's level, the Ambience slider
        // and the gunfire's dip it sat 40 dB under the guns that struck it.
        let gain = place.gain * loudness(distance, place.range);
        let (pan, cut) = (
            pan(offset, distance, right),
            dullness(distance, place.range),
        );
        let pitch = 0.94 + super::pick(frame.world, turn ^ 0x51, 1_201) as f32 / 10_000.0;
        let id = super::next_voice(frame.next_voice);
        // A held ring starts where its envelope does: silent with an attack,
        // whole without (the flare is whole as it is struck).
        let attack = place.attack.unwrap_or(0.0);
        let start = gain * frame.presence * ring_envelope(0.0, attack, 1.0, None);
        audio.play_ambience(id, Arc::clone(sound), false, start, pan);
        audio.effect_ambience(id);
        audio.pitch_ambience(id, pitch);
        audio.filter_ambience(id, cut);
        audio.send_ambience(id, frame.send);
        audio.aim_ambience(id, [pan, ahead_share(offset, distance, forward)]);
        let seconds = sound.frames() as f32 / crate::platform_audio::OUTPUT_RATE as f32 / pitch;
        let ends = frame.now + (seconds * 1.0e6) as u64;
        if held {
            for ring in self.rings.iter_mut().filter(|ring| ring.cell == cell) {
                ring.released.get_or_insert(frame.now);
            }
            self.rings.push(Ring {
                id,
                cell,
                offset,
                gain,
                pan,
                started: frame.now,
                ends,
                released: None,
            });
        } else {
            self.sounding.push((id, gain, pan, cut, ends));
        }
        self.ringing.push((id, frame.now, ends));
        crate::event_log::ambience("crystal", name, "shield");
    }

    /// The held rings with their crystals' flares now (`flares`: each
    /// flaring crystal's cell and flare, 0 to 1): each as loud as its
    /// envelope ([`ring_envelope`]), gone once its flash or its recording
    /// is. A flash that outlasts its ring's recording (a laser held on the
    /// crystal) rings again, the next taking over before the last runs out.
    pub(crate) fn hold_rings(
        &mut self,
        frame: &mut PlacedFrame<'_>,
        place: &PlacedTuning,
        flares: &[([u16; 2], f32)],
        axes: [[f32; 3]; 2],
        audio: &mut PlatformAudio,
    ) {
        let attack = place.attack.unwrap_or(0.0);
        let now = frame.now;
        let seconds = |since: u64| now.saturating_sub(since) as f32 / 1.0e6;
        let level_of = |cell: [u16; 2]| {
            flares
                .iter()
                .find(|(flaring, _)| *flaring == cell)
                .map_or(0.0, |(_, level)| *level)
        };
        let mut again = Vec::new();
        for ring in &mut self.rings {
            let level = level_of(ring.cell);
            let running_out = now + (attack * 1.0e6) as u64 >= ring.ends;
            if level > 0.0 && ring.released.is_none() && running_out {
                ring.released = Some(now);
                again.push((ring.cell, ring.offset));
            }
        }
        for (cell, offset) in again {
            self.ring(frame, place, cell, offset, axes, audio);
        }
        let mut gone = Vec::new();
        for ring in &self.rings {
            let level = level_of(ring.cell);
            let envelope = ring_envelope(
                seconds(ring.started),
                attack,
                level,
                ring.released.map(seconds),
            );
            let done = now >= ring.ends
                || level <= 0.0
                || ring.released.is_some_and(|at| seconds(at) >= attack);
            if done {
                audio.stop_ambience(Some(ring.id));
                gone.push(ring.id);
                continue;
            }
            audio.steer_ambience(ring.id, ring.gain * envelope * frame.presence, ring.pan);
        }
        self.rings.retain(|ring| !gone.contains(&ring.id));
    }
}
/// A place's choices' key, the same in every run.
fn key(place: Place) -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    place.hash(&mut hasher);
    hasher.finish() as u32
}

#[cfg(test)]
#[path = "placed_tests.rs"]
mod tests;
