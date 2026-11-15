//! The water's memory, the CPU's half (Vulkan's remaster,
//! [the plan](../../../docs/rt-plans/water-memory.md)): the window of world
//! cells round the eye that `vk_waves.comp` moves the water's waves, foam
//! and churn in, the water's depth under it, the steps the game's clock
//! gives it, and the stamps that stir it: retail's splashes, the blasts'
//! eruptions and where their water falls back, and the player's craft in
//! the water. Only a picture: nothing here reaches the game.

use crate::scene_feed::SceneWake;

/// A cell's side in world units, and the window's side in cells
/// (`vk_waves.comp`'s `SIDE`).
pub const CELL: f32 = 8.0;
pub const SIDE: i32 = 1024;
/// World cells along an axis before the 16-bit wrap.
const WRAP: i32 = 65536 / CELL as i32;
/// The depth grid's points a side (`vk_waves.comp`'s `DEPTH_SIDE`) and
/// their spacing, the terrain's cells: 66 cover the window wherever it
/// starts.
pub const DEPTH_SIDE: usize = 66;
const DEPTH_SPACING: i32 = 128;
/// One step of the waves on the game's clock, and the most one frame takes
/// (a slower frame slows them).
const STEP_MICROS: u64 = 4_000;
const MOST_STEPS: u64 = 12;
/// The most stamps a frame lays, the nearest.
const MOST_STAMPS: usize = 256;
/// How far above or below the water's plane a splash may lie and still
/// stir it.
const NEAR_PLANE: f32 = 256.0;

/// A splash (retail's, or where an eruption's water falls back): for the
/// first `SPLASH_YOUNG` of its life it presses the water down in its middle
/// and up round it (`vk_waves.comp` adds no water) over its radius
/// (`SPLASH_RADIUS` plus `SPLASH_SCALE_RADIUS` at its scale), the press
/// `SPLASH_PRESS` at its scale, and lays foam for the first
/// `SPLASH_FOAMING` of it.
const SPLASH_YOUNG: f32 = 0.12;
const SPLASH_RADIUS: f32 = 20.0;
const SPLASH_SCALE_RADIUS: f32 = 24.0;
const SPLASH_PRESS: f32 = 1800.0;
const SPLASH_FOAMING: f32 = 0.3;
const SPLASH_FOAM: f32 = 6.0;
/// The scale of a splash where the craft's spray falls back.
const SPRAY_SCALE: f32 = 0.25;
/// A blast's eruption: its dome swells over `DOME_SECONDS` over
/// `DOME_RADIUS` at its size, lifting the water `DOME_LIFT` hard (drawing
/// it in round it), white and churned; then a patch of foam and churn
/// spreads till `PATCH_SECONDS`.
const DOME_SECONDS: f32 = 0.35;
const DOME_RADIUS: f32 = 260.0;
const DOME_LIFT: f32 = 1500.0;
const DOME_FOAM: f32 = 4.0;
const DOME_CHURN: f32 = 3.0;
const PATCH_SECONDS: f32 = 1.5;
const PATCH_FOAM: f32 = 2.0;
const PATCH_CHURN: f32 = 1.5;
/// The craft through the water: a wheel over `WHEEL_RADIUS`, the hull over
/// `HULL_RADIUS`, each lifting the water ahead of it and pressing it down
/// behind, `PUSH` times its speed (units a second, at most `FASTEST`) at
/// most, so a craft at rest leaves it flat and a moving one throws a bow
/// wave and leaves a wake; each lays foam by its share of a brisk drive's
/// speed (`BRISK`) squared, and the hull churn by the share.
const WHEEL_RADIUS: f32 = 36.0;
const WHEEL_PUSH: f32 = 3.0;
const WHEEL_FOAM: f32 = 3.0;
const HULL_RADIUS: f32 = 120.0;
const HULL_PUSH: f32 = 6.0;
const HULL_FOAM: f32 = 5.0;
const HULL_CHURN: f32 = 0.6;
const BRISK: f32 = 850.0;
const FASTEST: f32 = 1800.0;

/// The water round the eye this frame, as the world gives it: the window's
/// first cell (0 to 8191 on X and Z), the eye in the window and the water's
/// plane under it (window-relative world units; the plane eye-relative),
/// and the water's depth over the depth grid, X first, from its first point
/// (window-relative; at or below zero: land).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WaveGround {
    pub origin: [i32; 2],
    pub eye: [f32; 2],
    pub plane: f32,
    pub depth_origin: [f32; 2],
    pub depths: Vec<f32>,
}

impl WaveGround {
    /// The window round `eye` (world X, Y and Z) over water whose plane
    /// stands at `plane`, its depths from the `ground` height at a world
    /// X/Z.
    pub fn around(eye: [i32; 3], plane: f32, ground: impl Fn([u16; 2]) -> f32) -> Self {
        let flat = [eye[0], eye[2]];
        let origin = flat.map(|axis| (axis.div_euclid(CELL as i32) - SIDE / 2).rem_euclid(WRAP));
        let start = origin.map(|cell| cell * CELL as i32);
        let first = start.map(|at| at.div_euclid(DEPTH_SPACING) * DEPTH_SPACING);
        let mut depths = Vec::with_capacity(DEPTH_SIDE * DEPTH_SIDE);
        for z in 0..DEPTH_SIDE as i32 {
            for x in 0..DEPTH_SIDE as i32 {
                let at = [
                    (first[0] + x * DEPTH_SPACING).rem_euclid(65536) as u16,
                    (first[1] + z * DEPTH_SPACING).rem_euclid(65536) as u16,
                ];
                depths.push(plane - ground(at));
            }
        }
        Self {
            origin,
            eye: [0, 1].map(|axis| (flat[axis] - start[axis]).rem_euclid(65536) as f32),
            plane: plane - eye[1] as f32,
            depth_origin: [0, 1].map(|axis| (first[axis] - start[axis]) as f32),
            depths,
        }
    }
}

/// Something stirring the water this frame: where it stands at the frame's
/// first step and its last (window-relative), its radius, how hard it lifts
/// the water (units a second squared; below zero it presses it down), the
/// foam and churn it lays a second, and the lift that tilts with its motion
/// (along `push`, as hard as its length; as much pressed behind), as
/// `vk_waves.comp`'s `Stamp`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamp {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub radius: f32,
    pub lift: f32,
    pub foam: f32,
    pub churn: f32,
    pub push: [f32; 2],
}

impl Stamp {
    fn at(position: [f32; 2], radius: f32, lift: f32, foam: f32, churn: f32) -> Self {
        Self {
            from: position,
            to: position,
            radius,
            lift,
            foam,
            churn,
            push: [0.0, 0.0],
        }
    }
}

/// The stamps `splashes` (the splash buffer's records: eye-relative
/// position, age, scale, kind 1 an eruption, depth), the craft's `wakes`
/// (those on the water, negative) and where its spray falls back
/// (`spray_rings`: eye-relative, how far through its ring's life, splashes
/// of their own at [`SPRAY_SCALE`]) lay on `ground`'s window, the nearest
/// first.
pub fn stamps(
    ground: &WaveGround,
    splashes: &[[f32; 7]],
    wakes: &[SceneWake],
    spray_rings: &[[f32; 4]],
) -> Vec<Stamp> {
    let window = |x: f32, z: f32| [x + ground.eye[0], z + ground.eye[1]];
    let near = |y: f32| (y - ground.plane).abs() <= NEAR_PLANE;
    let mut stamps = Vec::new();
    let falls = spray_rings
        .iter()
        .map(|&[x, y, z, age]| [x, y, z, age, SPRAY_SCALE, 0.0, 0.0]);
    for [x, y, z, age, scale, kind, _] in splashes.iter().copied().chain(falls) {
        if !near(y) {
            continue;
        }
        let at = window(x, z);
        if kind > 0.5 {
            let size = scale.max(0.05);
            if age < DOME_SECONDS {
                stamps.push(Stamp::at(
                    at,
                    DOME_RADIUS * size,
                    DOME_LIFT * size,
                    DOME_FOAM * size,
                    DOME_CHURN * size,
                ));
            } else if age < PATCH_SECONDS {
                stamps.push(Stamp::at(
                    at,
                    (150.0 + 260.0 * age.sqrt()) * size,
                    0.0,
                    PATCH_FOAM * size,
                    PATCH_CHURN * size,
                ));
            }
        } else if age < SPLASH_FOAMING {
            let press = if age < SPLASH_YOUNG {
                -SPLASH_PRESS * scale
            } else {
                0.0
            };
            stamps.push(Stamp::at(
                at,
                SPLASH_RADIUS + SPLASH_SCALE_RADIUS * scale,
                press,
                SPLASH_FOAM * scale,
                0.0,
            ));
        }
    }
    for wake in wakes.iter().filter(|wake| wake.strength < 0.0) {
        let at = window(wake.position[0], wake.position[2]);
        let [vx, vz] = wake.velocity;
        let speed = vx.hypot(vz);
        let pace = speed.min(FASTEST);
        let along = if speed > 0.0 {
            [vx / speed * pace, vz / speed * pace]
        } else {
            [0.0, 0.0]
        };
        let brisk = pace / BRISK;
        stamps.push(if wake.hull {
            Stamp {
                push: along.map(|axis| axis * HULL_PUSH),
                ..Stamp::at(
                    at,
                    HULL_RADIUS,
                    0.0,
                    HULL_FOAM * brisk * brisk,
                    HULL_CHURN * brisk,
                )
            }
        } else {
            Stamp {
                push: along.map(|axis| axis * WHEEL_PUSH),
                ..Stamp::at(at, WHEEL_RADIUS, 0.0, WHEEL_FOAM * brisk * brisk, 0.0)
            }
        });
    }
    let away = |stamp: &Stamp| {
        let [x, z] = [stamp.to[0] - ground.eye[0], stamp.to[1] - ground.eye[1]];
        x * x + z * z
    };
    stamps.sort_by(|a, b| away(a).total_cmp(&away(b)));
    stamps.truncate(MOST_STAMPS);
    stamps
}

/// One frame of the memory for the device: its buffer (`vk_waves.comp`'s
/// `Waves`: the window, the stamps' count, a fresh start, the cell and the
/// depth spacing, the depth grid's origin, the depths, then the stamps),
/// how many steps it runs and each one's seconds (0: a step that only
/// moves the window).
#[derive(Clone, Debug, PartialEq)]
pub struct WaveFrame {
    pub floats: Vec<f32>,
    pub steps: u32,
    pub seconds: f32,
}

/// The memory's clock and window, kept from frame to frame.
#[derive(Clone, Debug, Default)]
pub struct Waves {
    last: Option<(u64, [i32; 2])>,
}

impl Waves {
    /// The frame at `now` (the game's clock, microseconds) over `ground`,
    /// stirred by `stamps`: the steps since the last frame, none for a
    /// paused clock unless the window moved, and from still water after a
    /// clock that ran back (another visit) or on the first frame.
    pub fn frame(&mut self, now: u64, ground: &WaveGround, stamps: &[Stamp]) -> WaveFrame {
        let (fresh, moved, steps) = match self.last {
            Some((then, origin)) if then <= now => {
                let moved = [0, 1].map(|axis| {
                    (ground.origin[axis] - origin[axis] + WRAP / 2).rem_euclid(WRAP) - WRAP / 2
                });
                let steps = (now / STEP_MICROS - then / STEP_MICROS).min(MOST_STEPS) as u32;
                (false, moved, steps)
            }
            _ => (true, [0, 0], 0),
        };
        self.last = Some((now, ground.origin));
        let mut floats = vec![
            ground.origin[0] as f32,
            ground.origin[1] as f32,
            moved[0] as f32,
            moved[1] as f32,
            stamps.len() as f32,
            if fresh { 1.0 } else { 0.0 },
            CELL,
            DEPTH_SPACING as f32,
            ground.depth_origin[0],
            ground.depth_origin[1],
            0.0,
            0.0,
        ];
        floats.extend_from_slice(&ground.depths);
        for stamp in stamps {
            floats.extend_from_slice(&[
                stamp.from[0],
                stamp.from[1],
                stamp.to[0],
                stamp.to[1],
                stamp.radius,
                stamp.lift,
                stamp.foam,
                stamp.churn,
                stamp.push[0],
                stamp.push[1],
                0.0,
                0.0,
            ]);
        }
        let (steps, seconds) = match steps {
            0 if fresh || moved != [0, 0] => (1, 0.0),
            steps => (steps, STEP_MICROS as f32 / 1_000_000.0),
        };
        WaveFrame {
            floats,
            steps,
            seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(depth: f32) -> WaveGround {
        WaveGround::around([30000, 100, 40000], 0.0, |_| -depth)
    }

    #[test]
    fn the_window_centres_the_eye_and_wraps_with_the_world() {
        let ground = flat(50.0);
        assert_eq!(ground.origin, [30000 / 8 - 512, 40000 / 8 - 512]);
        assert_eq!(ground.eye, [4096.0, 4096.0]);
        assert_eq!(ground.plane, -100.0);
        // Past the wrap's start, the window starts near its end.
        let wrapped = WaveGround::around([100, 0, 65500], 0.0, |_| -50.0);
        assert_eq!(wrapped.origin, [(12 - 512 + WRAP) % WRAP, 8187 - 512]);
        assert_eq!(wrapped.eye, [4100.0, 4100.0]);
        // The depth grid starts on the terrain's cells, at or before the
        // window, and covers it.
        for axis in 0..2 {
            assert!((-128.0..=0.0).contains(&wrapped.depth_origin[axis]));
            let last = wrapped.depth_origin[axis] + (DEPTH_SIDE - 1) as f32 * 128.0;
            assert!(last >= SIDE as f32 * CELL);
        }
        assert_eq!(ground.depths.len(), DEPTH_SIDE * DEPTH_SIDE);
        assert!(ground.depths.iter().all(|depth| *depth == 50.0));
    }

    #[test]
    fn the_clock_gives_the_steps_and_a_move_or_a_new_visit_steps_once() {
        let ground = flat(50.0);
        let mut waves = Waves::default();
        let first = waves.frame(1_000_000, &ground, &[]);
        assert_eq!((first.steps, first.seconds, first.floats[5]), (1, 0.0, 1.0));
        let next = waves.frame(1_016_000, &ground, &[]);
        assert_eq!((next.steps, next.floats[5]), (4, 0.0));
        assert_eq!(next.seconds, 0.004);
        // A paused clock with the eye still: nothing to run.
        assert_eq!(waves.frame(1_016_000, &ground, &[]).steps, 0);
        // Moved one cell while paused: a step that only moves the window.
        let moved = WaveGround::around([30008, 100, 40000], 0.0, |_| -50.0);
        let step = waves.frame(1_016_000, &moved, &[]);
        assert_eq!((step.steps, step.seconds), (1, 0.0));
        assert_eq!(&step.floats[2..4], &[1.0, 0.0]);
        // A long stall runs at most MOST_STEPS.
        assert_eq!(waves.frame(9_000_000, &moved, &[]).steps, 12);
        // The clock ran back: still water again.
        assert_eq!(waves.frame(10, &moved, &[]).floats[5], 1.0);
        // Across the wrap a move is the short way round.
        let mut waves = Waves::default();
        waves.frame(0, &WaveGround::around([65530, 0, 0], 0.0, |_| -1.0), &[]);
        let across = waves.frame(0, &WaveGround::around([2, 0, 0], 0.0, |_| -1.0), &[]);
        assert_eq!(across.floats[2], 1.0);
    }

    #[test]
    fn splashes_eruptions_and_the_craft_stamp_the_water_near_its_plane() {
        let ground = flat(50.0);
        let young = [10.0, -100.0, 0.0, 0.05, 1.0, 0.0, 0.0];
        let old = [20.0, -100.0, 0.0, 0.5, 1.0, 0.0, 0.0];
        let high = [30.0, 400.0, 0.0, 0.05, 1.0, 0.0, 0.0];
        let dome = [0.0, -100.0, 500.0, 0.1, 1.0, 1.0, 300.0];
        let patch = [0.0, -100.0, 600.0, 1.0, 1.0, 1.0, 300.0];
        let gone = [0.0, -100.0, 700.0, 3.0, 1.0, 1.0, 300.0];
        let wakes = [
            SceneWake {
                position: [5.0, -100.0, 5.0],
                strength: -0.5,
                hull: false,
                velocity: [0.0, 0.0],
            },
            SceneWake {
                position: [0.0, -100.0, 0.0],
                strength: -1.0,
                hull: true,
                velocity: [850.0, 0.0],
            },
            // A puddle's: none.
            SceneWake {
                position: [0.0, 0.0, 0.0],
                strength: 1.0,
                hull: false,
                velocity: [850.0, 0.0],
            },
        ];
        let stamps = stamps(&ground, &[young, old, high, dome, patch, gone], &wakes, &[]);
        assert_eq!(stamps.len(), 5);
        // The hull first (nearest), lifting ahead of it and churning at a
        // brisk drive; the still wheel stirs nothing but is kept.
        assert_eq!(stamps[0].to, [4096.0, 4096.0]);
        assert_eq!(stamps[0].radius, HULL_RADIUS);
        assert_eq!(stamps[0].push, [850.0 * HULL_PUSH, 0.0]);
        assert_eq!((stamps[0].foam, stamps[0].churn), (HULL_FOAM, HULL_CHURN));
        assert_eq!(stamps[1].radius, WHEEL_RADIUS);
        assert_eq!(
            (stamps[1].push, stamps[1].lift, stamps[1].foam),
            ([0.0, 0.0], 0.0, 0.0)
        );
        // The young splash presses and foams; the old one is past it.
        assert_eq!(stamps[2].to, [4106.0, 4096.0]);
        assert_eq!(stamps[2].lift, -SPLASH_PRESS);
        // The eruption's dome lifts.
        assert_eq!(stamps[3].to, [4096.0, 4596.0]);
        assert_eq!(stamps[3].lift, DOME_LIFT);
        // Its patch (1 s on) only foams and churns, its end nothing.
        let later = super::stamps(&ground, &[patch, gone], &[], &[]);
        // Spray falling back splashes too, smaller.
        let fall = super::stamps(&ground, &[], &[], &[[0.0, -100.0, 0.0, 0.05]]);
        assert_eq!(fall.len(), 1);
        assert_eq!(fall[0].lift, -SPLASH_PRESS * SPRAY_SCALE);
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].lift, 0.0);
        assert!(later[0].foam > 0.0);
    }

    #[test]
    fn a_frame_lays_out_its_buffer_as_the_shader_reads_it() {
        let ground = flat(50.0);
        let stamp = Stamp::at([1.0, 2.0], 3.0, 4.0, 5.0, 6.0);
        let frame = Waves::default().frame(0, &ground, &[stamp]);
        assert_eq!(frame.floats.len(), 12 + DEPTH_SIDE * DEPTH_SIDE + 12);
        assert_eq!(frame.floats[4], 1.0);
        assert_eq!(&frame.floats[6..8], &[CELL, 128.0]);
        assert_eq!(
            &frame.floats[12 + DEPTH_SIDE * DEPTH_SIDE..],
            &[1.0, 2.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0, 0.0, 0.0, 0.0]
        );
    }
}
