//! The thrusters' trails under the tracer (the
//! user, 2026-10-23: thrusting "could leave a small wake or smoke trail
//! behind the ACAM that dissipates, not too intrusive"): while a vehicle
//! thrusts, its exhausts leave puffs of smoke or vapour that drift, grow and
//! thin to nothing within about a second, carried by the mist and lit as it
//! is ([`crate::modern_tuning::TrailTuning`]).
//!
//! Retail draws none. Only a picture: the exhausts and who thrusts come
//! through the feed, and nothing here reaches the game.

use std::collections::VecDeque;

use crate::modern_tuning::TrailLook;
use crate::scene_feed::SceneExhaust;

/// The most puffs at once; a new one takes the oldest's place.
pub const PUFFS: usize = 256;
/// An exhaust within this (world units) of a thrusting vehicle's middle is
/// that vehicle's.
const VEHICLE_REACH: f32 = 400.0;
/// An exhaust within this of one last frame is the same one, moved.
const SAME_EXHAUST: f64 = 120.0;
/// A thrusting exhaust that hardly moves still leaves a puff this often
/// (microseconds).
const STANDING_PUFF: u64 = 33_333;
/// The most puffs one exhaust leaves a frame.
const FRAME_PUFFS: usize = 32;
/// How fast a puff's push dies away, a second.
const DRAG: f32 = 3.0;
/// The share of its life a puff takes to thicken: hot and clear at the
/// exhaust, so a trail seen from behind never veils its own vehicle.
const FADE_IN: f32 = 0.15;
/// A clock that jumps more than this (microseconds) starts the exhausts
/// afresh: a load, a pause's end.
const JUMP: u64 = 500_000;

/// A puff: where it left its exhaust (world units, X and Z on the 16-bit
/// wrap), its push, when (the game's clock, microseconds) and its look.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Puff {
    at: [f64; 3],
    push: [f32; 3],
    born: u64,
    look: u8,
}

/// An exhaust as last seen: where (world), its look, and when it last left
/// a puff.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    at: [f64; 3],
    look: u8,
    puffed: u64,
}

/// The puffs showing, kept from frame to frame by the renderer.
#[derive(Clone, Debug, Default)]
pub struct Trails {
    puffs: VecDeque<Puff>,
    seen: Vec<Seen>,
    clock: u64,
}

/// A puff as the mist takes it: its eye-relative middle and radius, then
/// its colour and density (per thousand units).
pub type PuffRecord = [f32; 8];

impl Trails {
    /// Age the puffs to `clock`, and let each exhaust of a thrusting vehicle
    /// leave puffs along its way since the last frame.
    pub fn update(
        &mut self,
        clock: u64,
        eye: [f64; 3],
        exhausts: &[SceneExhaust],
        thrusts: &[[f32; 3]],
        looks: &[TrailLook],
    ) {
        // A clock gone back is another world's or a loaded game's: its
        // puffs go too.
        if clock < self.clock {
            self.puffs.clear();
        }
        if clock < self.clock || clock - self.clock > JUMP {
            self.seen.clear();
        }
        let since = self.clock.min(clock);
        self.puffs.retain(|puff| {
            looks
                .get(usize::from(puff.look))
                .is_some_and(|look| (clock.saturating_sub(puff.born) as f32) < look.seconds * 1.0e6)
        });
        let mut seen = Vec::with_capacity(exhausts.len());
        for exhaust in exhausts {
            let Some(look) = looks.get(usize::from(exhaust.look)) else {
                continue;
            };
            let at = world_of(eye, exhaust.position);
            let thrusting = thrusts.iter().any(|vehicle| {
                (0..3)
                    .map(|axis| (vehicle[axis] - exhaust.position[axis]).powi(2))
                    .sum::<f32>()
                    < VEHICLE_REACH * VEHICLE_REACH
            });
            let before = self
                .seen
                .iter()
                .filter(|last| last.look == exhaust.look)
                .map(|last| (last, between(last.at, at)))
                .map(|(last, way)| (last, way, way.iter().map(|axis| axis * axis).sum::<f64>()))
                .filter(|(_, _, apart)| *apart < SAME_EXHAUST * SAME_EXHAUST)
                .min_by(|a, b| a.2.total_cmp(&b.2));
            let mut puffed = before.map_or(0, |(last, _, _)| last.puffed);
            if thrusting {
                let mut puff = |at: [f64; 3], born: u64| {
                    self.puffs.push_back(Puff {
                        at,
                        push: exhaust.push,
                        born,
                        look: exhaust.look,
                    })
                };
                match before {
                    // Along the way it came, a puff each `spacing`, each as
                    // old as the time since it passed there.
                    Some((last, way, apart)) if apart.sqrt() >= f64::from(look.spacing) * 0.5 => {
                        let count = ((apart.sqrt() / f64::from(look.spacing.max(1.0))).round()
                            as usize)
                            .clamp(1, FRAME_PUFFS);
                        for step in 1..=count {
                            let share = step as f64 / count as f64;
                            let point =
                                std::array::from_fn(|axis| last.at[axis] + way[axis] * share);
                            let born = since + ((clock - since) as f64 * share) as u64;
                            puff(wrapped(point), born);
                        }
                        puffed = clock;
                    }
                    _ if clock.saturating_sub(puffed) >= STANDING_PUFF || before.is_none() => {
                        puff(at, clock);
                        puffed = clock;
                    }
                    _ => {}
                }
            }
            seen.push(Seen {
                at,
                look: exhaust.look,
                puffed,
            });
        }
        self.seen = seen;
        self.clock = clock;
        while self.puffs.len() > PUFFS {
            self.puffs.pop_front();
        }
    }

    /// The puffs at `clock` as the mist takes them, their density times
    /// `strength`: each drifts as pushed, its push dying away, rises, grows,
    /// thickens from clear and thins to nothing by its look's end.
    pub fn records(
        &self,
        clock: u64,
        eye: [f64; 3],
        looks: &[TrailLook],
        strength: f32,
    ) -> Vec<PuffRecord> {
        self.puffs
            .iter()
            .filter_map(|puff| {
                let look = looks.get(usize::from(puff.look))?;
                let age = clock.saturating_sub(puff.born) as f32 / 1.0e6;
                let gone = age / look.seconds.max(1.0e-3);
                let density = look.density
                    * strength
                    * (gone / FADE_IN).min(1.0)
                    * (1.0 - gone).max(0.0).powi(2);
                if density <= 0.0 {
                    return None;
                }
                let drift = (1.0 - (-DRAG * age).exp()) / DRAG;
                let from_eye = between(eye, puff.at);
                let [x, y, z] =
                    std::array::from_fn(|axis| from_eye[axis] as f32 + puff.push[axis] * drift);
                let radius = look.radius[0] + (look.radius[1] - look.radius[0]) * gone.sqrt();
                let [r, g, b] = look.color;
                Some([x, y + look.rise * age, z, radius, r, g, b, density])
            })
            .collect()
    }
}

/// The world point `from_eye` away from `eye`.
fn world_of(eye: [f64; 3], from_eye: [f32; 3]) -> [f64; 3] {
    wrapped(std::array::from_fn(|axis| {
        eye[axis] + f64::from(from_eye[axis])
    }))
}

/// X and Z on the 16-bit wrap.
fn wrapped(point: [f64; 3]) -> [f64; 3] {
    [
        point[0].rem_euclid(65536.0),
        point[1],
        point[2].rem_euclid(65536.0),
    ]
}

/// The way from world point `from` to `to`, the short way round the wrap.
fn between(from: [f64; 3], to: [f64; 3]) -> [f64; 3] {
    let short = |way: f64| (way + 32768.0).rem_euclid(65536.0) - 32768.0;
    [
        short(to[0] - from[0]),
        to[1] - from[1],
        short(to[2] - from[2]),
    ]
}

/// The thruster lights' glow ([`crate::scene_feed::SceneThrustGlow`]), 0
/// to 1, kept from frame to frame: up while a vehicle near the lit faces
/// thrusts, down when it lets go, each way over the tuning's seconds.
// shortcut: one level for every model's lights, the player's skimmer the
// only one the tuning names; keep one per vehicle if others get lights.
#[derive(Debug, Default)]
pub struct ThrustGlow {
    level: f32,
    clock: Option<u64>,
}

impl ThrustGlow {
    /// The glow at `clock` for `glows`, given where the thrusting vehicles
    /// stand (both eye-relative).
    pub fn update(
        &mut self,
        clock: u64,
        glows: &[crate::scene_feed::SceneThrustGlow],
        thrusts: &[[f32; 3]],
    ) -> f32 {
        let seconds = self
            .clock
            .filter(|&then| then <= clock && clock - then <= JUMP)
            .map_or(0.0, |then| (clock - then) as f32 / 1.0e6);
        self.clock = Some(clock);
        let thrusting = glows.iter().any(|glow| {
            thrusts.iter().any(|vehicle| {
                (0..3)
                    .map(|axis| (vehicle[axis] - glow.position[axis]).powi(2))
                    .sum::<f32>()
                    < VEHICLE_REACH * VEHICLE_REACH
            })
        });
        let ease = glows.first().map_or(0.0, |glow| glow.seconds);
        self.level = ease_toward(self.level, f32::from(u8::from(thrusting)), seconds, ease);
        self.level
    }
}

/// `level` moved toward `target` by `seconds` of an ease taking `ease`
/// seconds end to end (none: at once).
fn ease_toward(level: f32, target: f32, seconds: f32, ease: f32) -> f32 {
    if ease <= 0.0 {
        return target;
    }
    let step = seconds / ease;
    if target > level {
        (level + step).min(target)
    } else {
        (level - step).max(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The thruster lights rise while their vehicle thrusts and fade when
    /// it lets go, each over the tuning's seconds; a vehicle far off, or a
    /// jump in the clock, moves nothing.
    #[test]
    fn thruster_lights_ease_with_the_thrust() {
        let glows = [crate::scene_feed::SceneThrustGlow {
            triangles: 0..2,
            position: [0.0, 0.0, 100.0],
            gain: 2.0,
            seconds: 0.3,
        }];
        let near = [[0.0, 0.0, 150.0]];
        let mut glow = ThrustGlow::default();
        assert_eq!(glow.update(0, &glows, &near), 0.0, "no time has passed");
        assert!((glow.update(150_000, &glows, &near) - 0.5).abs() < 1.0e-5);
        assert_eq!(glow.update(400_000, &glows, &near), 1.0);
        assert_eq!(glow.update(400_000 + JUMP + 1, &glows, &[]), 1.0, "a load");
        let far = [[0.0, 0.0, 100.0 + VEHICLE_REACH + 1.0]];
        let at = 400_000 + JUMP + 1;
        assert!((glow.update(at + 150_000, &glows, &far) - 0.5).abs() < 1.0e-5);
        assert_eq!(glow.update(at + 400_000, &glows, &far), 0.0);
    }

    const SMOKE: TrailLook = TrailLook {
        color: [0.8, 0.8, 0.8],
        density: 3.0,
        seconds: 1.0,
        radius: [7.0, 26.0],
        rise: 30.0,
        spacing: 7.0,
    };

    fn exhaust(position: [f32; 3]) -> SceneExhaust {
        SceneExhaust {
            position,
            push: [0.0; 3],
            look: 0,
        }
    }

    /// A thrusting exhaust leaves a puff each `spacing` along its way, each
    /// as old as the time since it passed there; one far from any thrusting
    /// vehicle leaves none. A puff is clear when it leaves.
    #[test]
    fn a_moving_exhaust_leaves_puffs_along_its_way() {
        let mut trails = Trails::default();
        let eye = [1000.0, 0.0, 1000.0];
        let vehicle = [[0.0, 0.0, 50.0]];
        trails.update(
            1_000_000,
            eye,
            &[exhaust([0.0, 0.0, 0.0])],
            &vehicle,
            &[SMOKE],
        );
        assert_eq!(trails.puffs.len(), 1);
        // 70 units in a frame: ten puffs, 7 apart.
        trails.update(
            1_016_000,
            eye,
            &[exhaust([0.0, 0.0, 70.0])],
            &vehicle,
            &[SMOKE],
        );
        assert_eq!(trails.puffs.len(), 11);
        assert_eq!(trails.records(1_016_000, eye, &[SMOKE], 1.0).len(), 10);
        let records = trails.records(1_200_000, eye, &[SMOKE], 1.0);
        assert_eq!(records.len(), 11);
        assert!((records[1][2] - 7.0).abs() < 1.0e-3 && (records[10][2] - 70.0).abs() < 1.0e-3);
        // Once thick, the newest, at the exhaust, the thickest; the first,
        // oldest, risen highest.
        assert!(records[10][7] > records[0][7] && records[0][1] > records[10][1]);
        let far = [[0.0, 0.0, 1000.0]];
        let mut idle = Trails::default();
        idle.update(1_000_000, eye, &[exhaust([0.0; 3])], &far, &[SMOKE]);
        assert!(idle.puffs.is_empty());
    }

    /// A thrusting exhaust standing still leaves a puff every 1/30 s, and
    /// every puff is gone by its look's end, thinning to it.
    #[test]
    fn puffs_thin_out_and_go() {
        let mut trails = Trails::default();
        let eye = [0.0; 3];
        let vehicle = [[0.0; 3]];
        for frame in 0..10_u64 {
            trails.update(
                frame * 16_000,
                eye,
                &[exhaust([5.0, 0.0, 5.0])],
                &vehicle,
                &[SMOKE],
            );
        }
        // Frames 0, 3, 6 and 9 (at 0, 48, 96 and 144 ms).
        assert_eq!(trails.puffs.len(), 4);
        let young = trails.records(200_000, eye, &[SMOKE], 1.0)[0][7];
        let old = trails.records(900_000, eye, &[SMOKE], 1.0)[0][7];
        assert!(old < young * 0.05);
        trails.update(1_200_000, eye, &[], &[], &[SMOKE]);
        assert!(trails.puffs.is_empty());
        assert!(trails.records(1_200_000, eye, &[SMOKE], 1.0).is_empty());
    }

    /// An exhaust crossing the 16-bit wrap leaves its puffs the short way,
    /// and the records stay eye-relative across it.
    #[test]
    fn a_trail_crosses_the_wrap() {
        let mut trails = Trails::default();
        let vehicle = [[0.0; 3]];
        trails.update(
            0,
            [65530.0, 0.0, 0.0],
            &[exhaust([0.0; 3])],
            &vehicle,
            &[SMOKE],
        );
        trails.update(
            16_000,
            [4.0, 0.0, 0.0],
            &[exhaust([0.0; 3])],
            &vehicle,
            &[SMOKE],
        );
        let records = trails.records(200_000, [4.0, 0.0, 0.0], &[SMOKE], 1.0);
        assert_eq!(records.len(), 2);
        assert!(records
            .iter()
            .all(|record| record[0] <= 0.0 && record[0] >= -10.0));
    }

    /// A clock gone back (another world, a loaded game) clears the puffs.
    #[test]
    fn a_clock_gone_back_clears_the_puffs() {
        let mut trails = Trails::default();
        let vehicle = [[0.0; 3]];
        trails.update(
            5_000_000,
            [0.0; 3],
            &[exhaust([0.0; 3])],
            &vehicle,
            &[SMOKE],
        );
        assert_eq!(trails.puffs.len(), 1);
        trails.update(16_000, [0.0; 3], &[], &[], &[SMOKE]);
        assert!(trails.puffs.is_empty());
    }

    /// No more than [`PUFFS`] at once, the oldest giving way.
    #[test]
    fn the_oldest_puffs_give_way() {
        let mut trails = Trails::default();
        let eye = [0.0; 3];
        let vehicle = [[0.0; 3]];
        for frame in 0..40_u64 {
            let along = frame as f32 * 70.0 % 300.0;
            trails.update(
                frame * 16_000,
                eye,
                &[exhaust([along - 150.0, 0.0, 0.0])],
                &vehicle,
                &[SMOKE],
            );
        }
        assert_eq!(trails.puffs.len(), PUFFS);
    }
}
