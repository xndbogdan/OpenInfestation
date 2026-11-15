//! The holes in the modern sky's clouds (Vulkan only, both renderers;
//! `pt_sky.glsl`'s `cloud_hole`), and the blasts' light inside them. Only
//! a look: the world keeps it beside the game, which never reads it.
//!
//! - The craft the camera follows: where it has flown, a point every
//!   quarter of a second for three seconds, along which the clouds thin, a
//!   tube that widens and closes again as the cloud heals, as an aircraft's
//!   exhaust and wake clear a path through real cloud.
//! - Anything else flying fast through the clouds' layer (a missile, the
//!   Cranial Implanter, a rocket; user, 2026-11-15: "if they fly at high
//!   speed through clouds, how do real clouds act?"): a narrower tunnel the
//!   same way, as its hot exhaust and the air it churns evaporate the cloud.
//! - A blast in the layer or near it: a hole the heat and the blast clear,
//!   opening at once, rising with the hot air and filling in again over
//!   twenty seconds; and its flash lighting the cloud round it from inside
//!   for half a second, as an explosion lights real cloud.
//!
//! Each hole goes to the device as a capsule (eye-relative ends, its
//! radius, how much of the cloud it clears), nearest first; each flash as
//! a point light (where, how far its light reaches, its light).

use std::collections::VecDeque;

use crate::fast_hash::FastMap;
use crate::object_runtime::ObjectId;

/// The most holes and flashes the frame block carries (`pt_frame.glsl`'s
/// `CLOUD_HOLES` and `CLOUD_FLASHES`).
pub const MOST_HOLES: usize = 32;
pub const MOST_FLASHES: usize = 4;
/// How often a tube or a tunnel lays a point, in the game clock's
/// microseconds.
const EVERY_MICROS: u64 = 250_000;
/// The craft's tube: how wide where it has just passed, how fast it widens
/// a second as the cloud closes in, and how long it lasts (seconds).
const CRAFT_RADIUS: f32 = 120.0;
const CRAFT_GROWTH: f32 = 150.0;
const CRAFT_LIFE: f32 = 3.0;
/// What flies faster than this (units a second) through the layer leaves a
/// tunnel: about a buggy's top speed driven, under the missiles'. Moving
/// faster than `LEAP` it was put there, not flown.
const FAST: f32 = 600.0;
const LEAP: f32 = 12_000.0;
/// A tunnel, as the craft's tube: narrower, lasting a little longer.
const TUNNEL_RADIUS: f32 = 70.0;
const TUNNEL_GROWTH: f32 = 90.0;
const TUNNEL_LIFE: f32 = 4.0;
/// How far above or below the layer a flier is still followed.
const LAYER_MARGIN: f32 = 200.0;
/// A blast's hole, small and large: how wide, the seconds it takes to open
/// (to 1 - 1/e of it), how fast the hot air lifts it, and how long the
/// cloud takes to fill it in again (seconds).
const BLAST_RADIUS: [f32; 2] = [240.0, 520.0];
const BLAST_OPEN: f32 = 0.25;
const BLAST_RISE: f32 = 40.0;
const BLAST_LIFE: f32 = 20.0;
/// A fireball within this of one seen within `SAME_BLAST_SECONDS` is the
/// same blast still burning (each lives a second or so; a frame is 16 ms).
const SAME_BLAST: f32 = 96.0;
const SAME_BLAST_SECONDS: f32 = 0.2;
/// A blast's flash in the clouds, small and large: how long it lasts
/// (seconds), its colour, and how far its light carries into the cloud
/// (to 1/e) and how bright it is there (the lightning's flash is 3).
const FLASH_SECONDS: f32 = 0.5;
const FLASH_COLOUR: [f32; 3] = [1.0, 0.72, 0.42];
const FLASH_REACH: [f32; 2] = [900.0, 2000.0];
const FLASH_GAIN: [f32; 2] = [1.5, 3.0];

/// The clouds' layer as the holes take it: its lowest and highest world
/// heights, its clouds' rise either way included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudLayer {
    pub low: f32,
    pub high: f32,
}

impl CloudLayer {
    fn near(&self, height: f32, margin: f32) -> bool {
        (self.low - margin..=self.high + margin).contains(&height)
    }
}

/// A flier being followed: where and when it was last seen, and the points
/// its tunnel has laid.
#[derive(Clone, Debug, Default)]
struct Flier {
    seen: ([u16; 3], u64),
    fast: bool,
    points: VecDeque<([u16; 3], u64)>,
}

/// A blast seen: where, when it was born and last seen burning, and
/// whether a large one.
#[derive(Clone, Copy, Debug)]
struct Blast {
    at: [u16; 3],
    born: u64,
    seen: u64,
    large: bool,
}

/// The holes kept from frame to frame with the world.
#[derive(Clone, Debug, Default)]
pub struct CloudWake {
    /// The craft's tube: each point where the craft was (16-bit world
    /// words) and when.
    points: VecDeque<([u16; 3], u64)>,
    fliers: FastMap<ObjectId, Flier>,
    blasts: Vec<Blast>,
    clock: u64,
}

/// Eye-relative across the world's 16-bit wrap.
fn relative(words: [u16; 3], eye: [i32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| f32::from(i32::from(words[axis]).wrapping_sub(eye[axis]) as i16))
}

/// From `a` to `b` across the wrap, world units.
fn between(a: [u16; 3], b: [u16; 3]) -> f32 {
    let step = relative(b, a.map(i32::from));
    step.iter().map(|axis| axis * axis).sum::<f32>().sqrt()
}

/// Seconds from `then` to `now`, game-clock microseconds.
fn seconds(then: u64, now: u64) -> f32 {
    now.saturating_sub(then) as f32 / 1.0e6
}

/// Lay a point at `at` now when one is due, and drop those past `life`.
fn lay(points: &mut VecDeque<([u16; 3], u64)>, at: Option<[u16; 3]>, micros: u64, life: f32) {
    let life = (life * 1.0e6) as u64;
    while points
        .front()
        .is_some_and(|&(_, laid)| micros - laid > life)
    {
        points.pop_front();
    }
    let due = points
        .back()
        .is_none_or(|&(_, laid)| micros - laid >= EVERY_MICROS);
    if let (Some(at), true) = (at, due) {
        points.push_back((at, micros));
    }
}

/// The capsules along a path from `now` (where it is, if still laying)
/// through its `points`, newest first: each from one point to the next,
/// as wide and as clearing as its two ends' mean age gives.
fn path(
    now: Option<[u16; 3]>,
    points: &VecDeque<([u16; 3], u64)>,
    micros: u64,
    eye: [i32; 3],
    [radius, growth, life]: [f32; 3],
    holes: &mut Vec<[f32; 8]>,
) {
    let ends: Vec<([f32; 3], f32)> = now
        .map(|at| (relative(at, eye), 0.0))
        .into_iter()
        .chain(
            points
                .iter()
                .rev()
                .map(|&(at, laid)| (relative(at, eye), seconds(laid, micros))),
        )
        .collect();
    for pair in ends.windows(2) {
        let [(a, young), (b, old)] = [pair[0], pair[1]];
        let age = 0.5 * (young + old);
        let fade = (1.0 - age / life).max(0.0);
        if fade > 0.0 {
            holes.push([
                a[0],
                a[1],
                a[2],
                radius + growth * age,
                b[0],
                b[1],
                b[2],
                fade * fade,
            ]);
        }
    }
}

impl CloudWake {
    /// Follow the frame at the game clock's `micros`: the craft the camera
    /// follows at `craft` (none: no craft followed), the world's objects
    /// (`objects`: each and where it is) that fly within the clouds' `layer`
    /// (none: no clouds, which keeps nothing), and the blasts' fireballs
    /// (`blasts`: where, and whether large). A clock that went back (a new
    /// world, a load) starts afresh.
    pub fn follow(
        &mut self,
        craft: Option<[u16; 3]>,
        objects: impl IntoIterator<Item = (ObjectId, [u16; 3])>,
        blasts: &[([u16; 3], bool)],
        layer: Option<CloudLayer>,
        micros: u64,
    ) {
        if self.clock > micros {
            *self = Self::default();
        }
        self.clock = micros;
        lay(&mut self.points, craft, micros, CRAFT_LIFE);
        let Some(layer) = layer else {
            self.fliers.clear();
            self.blasts.clear();
            return;
        };
        // The fliers in the layer, the craft's own tube aside.
        for (id, at) in objects {
            if Some(at) == craft || !layer.near(f32::from(at[1] as i16), LAYER_MARGIN) {
                continue;
            }
            let flier = self.fliers.entry(id).or_default();
            // On both of the arena's lists: once a frame.
            if flier.seen == (at, micros) {
                continue;
            }
            let (then, when) = flier.seen;
            let speed = if when > 0 && micros > when {
                between(then, at) / seconds(when, micros)
            } else {
                0.0
            };
            flier.fast = (FAST..LEAP).contains(&speed);
            flier.seen = (at, micros);
        }
        for flier in self.fliers.values_mut() {
            let still = flier.seen.1 == micros && flier.fast;
            lay(
                &mut flier.points,
                still.then_some(flier.seen.0),
                micros,
                TUNNEL_LIFE,
            );
            if !still {
                flier.fast = false;
            }
        }
        // Those gone from the layer and whose tunnels have closed.
        self.fliers
            .retain(|_, flier| flier.seen.1 == micros || !flier.points.is_empty());
        let life = (BLAST_LIFE * 1.0e6) as u64;
        self.blasts.retain(|blast| micros - blast.born <= life);
        for &(at, large) in blasts {
            let reach = BLAST_RADIUS[usize::from(large)] + FLASH_REACH[usize::from(large)];
            if !layer.near(f32::from(at[1] as i16), reach) {
                continue;
            }
            let burning = self.blasts.iter_mut().find(|blast| {
                between(blast.at, at) < SAME_BLAST
                    && seconds(blast.seen, micros) < SAME_BLAST_SECONDS
            });
            match burning {
                Some(blast) => blast.seen = micros,
                None => self.blasts.push(Blast {
                    at,
                    born: micros,
                    seen: micros,
                    large,
                }),
            }
        }
    }

    /// The holes from `eye` at `micros`, nearest first, at most
    /// [`MOST_HOLES`]: each `[a, radius, b, clears]`, its ends eye-relative,
    /// how much of the cloud it clears 0 to 1. The craft's tube runs from
    /// `craft` (eye-relative) where one is followed. Only blasts in the
    /// layer or by it (`layer`) hole it.
    pub fn holes(
        &self,
        eye: [i32; 3],
        craft: Option<[f32; 3]>,
        layer: Option<CloudLayer>,
        micros: u64,
    ) -> Vec<[f32; 8]> {
        let mut holes = Vec::new();
        let mut tube = Vec::new();
        path(
            None,
            &self.points,
            micros,
            eye,
            [CRAFT_RADIUS, CRAFT_GROWTH, CRAFT_LIFE],
            &mut tube,
        );
        // The craft now, at its tube's head.
        if let (Some([x, y, z]), Some(&(newest, laid))) = (craft, self.points.back()) {
            let [bx, by, bz] = relative(newest, eye);
            let age = 0.5 * seconds(laid, micros);
            let fade = (1.0 - age / CRAFT_LIFE).max(0.0);
            tube.insert(
                0,
                [
                    x,
                    y,
                    z,
                    CRAFT_RADIUS + CRAFT_GROWTH * age,
                    bx,
                    by,
                    bz,
                    fade * fade,
                ],
            );
        }
        holes.extend(tube);
        for flier in self.fliers.values() {
            let now = (flier.fast && flier.seen.1 == micros).then_some(flier.seen.0);
            path(
                now,
                &flier.points,
                micros,
                eye,
                [TUNNEL_RADIUS, TUNNEL_GROWTH, TUNNEL_LIFE],
                &mut holes,
            );
        }
        if let Some(layer) = layer {
            for blast in &self.blasts {
                let size = usize::from(blast.large);
                let age = seconds(blast.born, micros);
                let [x, y, z] = relative(blast.at, eye);
                let y = y + BLAST_RISE * age;
                let radius = BLAST_RADIUS[size] * (1.0 - (-age / BLAST_OPEN).exp());
                let fade = 1.0 - age / BLAST_LIFE;
                if !layer.near(eye[1] as f32 + y, BLAST_RADIUS[size])
                    || radius <= 1.0
                    || fade <= 0.0
                {
                    continue;
                }
                holes.push([x, y, z, radius, x, y, z, fade * fade]);
            }
        }
        let away = |hole: &[f32; 8]| {
            let middle = [0, 1, 2].map(|axis| 0.5 * (hole[axis] + hole[axis + 4]));
            middle.iter().map(|axis| axis * axis).sum::<f32>()
        };
        holes.sort_by(|a, b| away(a).total_cmp(&away(b)));
        holes.truncate(MOST_HOLES);
        holes
    }

    /// The blasts' flashes lighting the clouds from `eye` at `micros`, the
    /// brightest first, at most [`MOST_FLASHES`]: each `[at, reach, light,
    /// share]`, eye-relative, how far its light carries into the cloud,
    /// its light, and how much of its flash is left (0 to 1).
    pub fn flashes(&self, eye: [i32; 3], micros: u64) -> Vec<[f32; 8]> {
        let mut flashes: Vec<[f32; 8]> = self
            .blasts
            .iter()
            .filter_map(|blast| {
                let left = 1.0 - seconds(blast.born, micros) / FLASH_SECONDS;
                if left <= 0.0 {
                    return None;
                }
                let size = usize::from(blast.large);
                let share = left * left;
                let [x, y, z] = relative(blast.at, eye);
                let [r, g, b] = FLASH_COLOUR.map(|channel| channel * FLASH_GAIN[size] * share);
                Some([x, y, z, FLASH_REACH[size], r, g, b, share])
            })
            .collect();
        flashes.sort_by(|a, b| (b[4] + b[5] + b[6]).total_cmp(&(a[4] + a[5] + a[6])));
        flashes.truncate(MOST_FLASHES);
        flashes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYER: CloudLayer = CloudLayer {
        low: 3000.0,
        high: 5000.0,
    };

    fn id(slot: usize) -> ObjectId {
        ObjectId::for_test(slot)
    }

    #[test]
    fn the_crafts_tube_lays_a_point_each_quarter_second_and_heals_after_three() {
        let mut wake = CloudWake::default();
        for frame in 0..200u64 {
            // A craft flying along X at 16 ms frames.
            let at = [(frame * 10) as u16, 4000, 0];
            wake.follow(Some(at), [], &[], Some(LAYER), frame * 16_000);
        }
        let micros = 199 * 16_000;
        let holes = wake.holes([1990, 4000, 0], Some([0.0, 0.0, 0.0]), Some(LAYER), micros);
        // From the craft now back along its way, as wide as it is old and
        // clearing the less, none older than its life.
        assert_eq!(holes[0][..3], [0.0, 0.0, 0.0]);
        assert!(holes.len() >= 11, "{}", holes.len());
        let mut by_age = holes.clone();
        by_age.sort_by(|a, b| b[0].total_cmp(&a[0]));
        assert!(by_age
            .windows(2)
            .all(|pair| pair[1][3] >= pair[0][3] && pair[1][7] <= pair[0][7]));
        assert!(holes
            .iter()
            .all(|hole| hole[3] <= CRAFT_RADIUS + CRAFT_GROWTH * CRAFT_LIFE));
        // Long after the craft is no longer followed, the tube is gone.
        let later = micros + 4_000_000;
        wake.follow(None, [], &[], Some(LAYER), later);
        assert!(wake
            .holes([1990, 4000, 0], None, Some(LAYER), later)
            .is_empty());
    }

    #[test]
    fn the_tube_meets_itself_across_the_wrap_and_restarts_with_the_clock() {
        let mut wake = CloudWake::default();
        wake.follow(Some([65_500, 0, 10]), [], &[], None, 1_000_000);
        // From an eye just across the wrap, the craft's point lies 46 units
        // behind it.
        let holes = wake.holes([10, 0, 10], Some([0.0, 0.0, 0.0]), None, 1_000_000);
        assert_eq!(holes.len(), 1);
        assert_eq!(holes[0][4..7], [-46.0, 0.0, 0.0]);
        // A clock that went back (a new world) starts afresh.
        wake.follow(Some([5, 0, 5]), [], &[], None, 0);
        let holes = wake.holes([0, 0, 0], Some([5.0, 0.0, 5.0]), None, 0);
        assert_eq!(holes[0][4..7], [5.0, 0.0, 5.0]);
    }

    /// A missile flying fast through the layer leaves a tunnel that closes
    /// after its life; a slow flier, one beside the layer, or one put
    /// somewhere far leaves none.
    #[test]
    fn fast_fliers_tunnel_through_the_layer() {
        let mut wake = CloudWake::default();
        let fly = |wake: &mut CloudWake, speed: f32, height: u16, frames: u64| {
            for frame in 0..frames {
                let x = (speed * frame as f32 * 0.016) as u16;
                wake.follow(
                    None,
                    [(id(7), [x, height, 0])],
                    &[],
                    Some(LAYER),
                    frame * 16_000,
                );
            }
            wake.holes([0, 4000, 0], None, Some(LAYER), (frames - 1) * 16_000)
        };
        let tunnel = fly(&mut wake, 1500.0, 4000, 120);
        assert!(tunnel.len() >= 6, "{}", tunnel.len());
        assert!(tunnel
            .iter()
            .all(|hole| hole[3] >= TUNNEL_RADIUS && hole[7] > 0.0));
        assert!(fly(&mut CloudWake::default(), 300.0, 4000, 120).is_empty());
        assert!(fly(&mut CloudWake::default(), 1500.0, 1000, 120).is_empty());
        assert!(fly(&mut CloudWake::default(), 20_000.0, 4000, 3).is_empty());
        // Gone from the layer, the tunnel closes after its life.
        let end = 119 * 16_000 + (TUNNEL_LIFE * 1.0e6) as u64 + 1;
        wake.follow(None, [], &[], Some(LAYER), end);
        assert!(wake.holes([0, 4000, 0], None, Some(LAYER), end).is_empty());
    }

    /// A blast in the layer holes it, opening fast, rising and filling in
    /// over its life; its flash lights the cloud for half a second. One
    /// far under the layer flashes nothing and holes nothing; a fireball
    /// seen over several frames is one blast.
    #[test]
    fn a_blast_holes_the_clouds_and_lights_them() {
        let mut wake = CloudWake::default();
        for frame in 0..10u64 {
            wake.follow(
                None,
                [],
                &[([0, 4000, 0], true)],
                Some(LAYER),
                frame * 16_000,
            );
        }
        assert_eq!(wake.blasts.len(), 1);
        let at = |seconds: f32| {
            let micros = (seconds * 1.0e6) as u64;
            wake.holes([0, 4000, 0], None, Some(LAYER), micros)
        };
        let [opening, open, filling] = [0.05, 1.0, 15.0].map(at);
        assert!(opening[0][3] < open[0][3] && open[0][3] <= BLAST_RADIUS[1]);
        assert!(open[0][3] > 0.95 * BLAST_RADIUS[1]);
        assert!(filling[0][1] > open[0][1] && filling[0][7] < open[0][7]);
        assert!(at(BLAST_LIFE + 1.0).is_empty());
        let flash = wake.flashes([0, 4000, 0], 100_000);
        assert_eq!(flash.len(), 1);
        assert_eq!(flash[0][3], FLASH_REACH[1]);
        assert!(flash[0][7] > 0.5);
        assert!(wake.flashes([0, 4000, 0], 600_000).is_empty());
        let mut far = CloudWake::default();
        far.follow(None, [], &[([0, 0, 0], true)], Some(LAYER), 0);
        assert!(far.flashes([0, 4000, 0], 0).is_empty());
        assert!(far
            .holes([0, 4000, 0], None, Some(LAYER), 500_000)
            .is_empty());
    }

    /// A fireball is one blast for as long as it burns; another at the same
    /// place after it is gone is another blast.
    #[test]
    fn a_fireball_is_one_blast_while_it_burns() {
        let mut wake = CloudWake::default();
        let fireball = [([100, 4000, 100], false)];
        for frame in 0..190u64 {
            wake.follow(None, [], &fireball, Some(LAYER), frame * 16_000);
        }
        assert_eq!(wake.blasts.len(), 1);
        wake.follow(None, [], &[], Some(LAYER), 190 * 16_000);
        wake.follow(None, [], &fireball, Some(LAYER), 250 * 16_000);
        assert_eq!(wake.blasts.len(), 2);
    }

    /// No more holes than the frame block takes, the nearest kept.
    #[test]
    fn the_nearest_holes_are_kept() {
        let mut wake = CloudWake::default();
        let blasts: Vec<([u16; 3], bool)> =
            (0..40u16).map(|k| ([k * 600, 4000, 0], false)).collect();
        wake.follow(None, [], &blasts, Some(LAYER), 0);
        let holes = wake.holes([0, 4000, 0], None, Some(LAYER), 500_000);
        assert_eq!(holes.len(), MOST_HOLES);
        assert!(holes.iter().all(|hole| hole[0] < 600.0 * MOST_HOLES as f32));
    }
}
