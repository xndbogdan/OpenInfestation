//! Port option, Vulkan only, the remaster's picture: the spray the player's
//! craft throws up driving through the rain's puddles (2026-10-29: at a high
//! speed through puddles, for example the buggy with turbo on, they splash a little). A short
//! fan of drops off each of the craft's ground points that stands in a
//! puddle, nothing at a crawl, growing with speed and largest at a turbo's,
//! back in the puddle within a second, where some of its drops ring the
//! water.
//!
//! Only a picture, outside the game's particle system: the craft's
//! position, hull and velocity are read, never written, and the drops are
//! the remaster's own (no draw of the game's generator). Each lattice step
//! of the animation clock ([`SLOT_MICROS`]) throws the same drops from the
//! same places, so a run at a fixed step draws the same spray every time.
//! Vulkan draws the drops over the world, traced or not
//! (`SoftwareFrame::add_spray_fans`), and the puddles' shading takes the
//! rings (`pt_ground.glsl`'s `waked`).

use crate::d3d_viewport::D3dTlVertex;
use crate::scene_feed::SceneCamera;

/// How often the craft throws its drops, in microseconds of the animation
/// clock.
const SLOT_MICROS: u64 = 16_000;
/// The most steps one update catches up on (a stall or a long pause).
const MOST_SLOTS: u64 = 8;
/// How long a throw is kept: its drops' flight and their rings' life.
const KEEP_MICROS: u64 = 2_500_000;
/// Gravity on the drops, world units a second squared (`scene_spray`'s).
const GRAVITY: f32 = 980.0;
/// The craft's speed over the ground (units a second: velocity `+8E..+92`
/// moves `v * delta / 2^20` a tick) below which it throws nothing, and the
/// speed of a full spray; a turbo's speed throws up to `MOST` of it. The
/// buggy's top speed is about 850 driven, 1,700 boosted.
const CRAWL: f32 = 150.0;
const FULL: f32 = 850.0;
const MOST: f32 = 2.0;
/// Drops a ground point throws each step at a full spray, in whole water.
const DROPS: f32 = 6.0;
/// How far out from a ground point the drops leave: the tyre's outer face.
const TYRE_FACE: f64 = 14.0;
/// How far above the ground under it a hull point may ride and still touch
/// it (its samples rest about 8 above it).
const TOUCH: i32 = 30;
/// How long a ring spreads on the water, and the most the puddles take at
/// once, the nearest (`pt_frame.glsl`'s `spray_rings`).
const RING_SECONDS: f32 = 0.9;
pub const MOST_RINGS: usize = 16;
/// How long a drop's streak is, in seconds of its motion.
const STREAK_SECONDS: f32 = 0.03;
/// The smallest a drop is drawn, in native pixels; a smaller one keeps its
/// share of the cover as its opacity.
const LEAST_PIXELS: f32 = 0.7;
/// How opaque a drop's middle is, and a puff of fine mist's.
const DROP_ALPHA: f32 = 0.8;
const MIST_ALPHA: f32 = 0.25;

/// One of the craft's ground points this frame: in world units (X and Z on
/// the 16-bit wrap, Y the water's surface there, the uncarved ground), the
/// direction out from the craft across it (X/Z, unit), and how much
/// standing water lies there (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SprayPoint {
    pub world: [f64; 3],
    pub out: [f32; 2],
    pub water: f32,
}

/// The craft as it throws spray now: its ground points (each in a puddle or
/// not) and its velocity in units a second.
#[derive(Clone, Debug, PartialEq)]
pub struct SprayCraft {
    pub points: Vec<SprayPoint>,
    pub velocity: [f32; 3],
}

/// A drop thrown: its velocity at launch, size, whether it is fine mist,
/// and whether it rings the water where it falls back.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Throw {
    velocity: [f32; 3],
    radius: f32,
    mist: bool,
    ring: bool,
}

/// The drops one ground point threw at one step.
#[derive(Clone, Debug, PartialEq)]
struct Burst {
    born: u64,
    at: [f64; 3],
    throws: Vec<Throw>,
}

/// The spray in the air and on the water, kept from frame to frame with
/// the world.
#[derive(Clone, Debug, Default)]
pub struct CraftSpray {
    bursts: Vec<Burst>,
    /// The last update's clock, and the craft then if there was one.
    clock: Option<u64>,
    last: Option<(u64, SprayCraft)>,
}

/// A drop now: eye-relative, its velocity, size and how opaque its middle
/// is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SprayDrop {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub radius: f32,
    pub alpha: f32,
}

/// SplitMix32's finaliser over `x`, as `scene_spray`'s.
struct Random(u32);

impl Random {
    fn next(&mut self) -> f32 {
        let mut x = self.0.wrapping_add(0x9e37_79b9);
        self.0 = x;
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846c_a68b);
        x ^= x >> 16;
        (x >> 8) as f32 / 16_777_216.0
    }

    fn between(&mut self, [low, high]: [f32; 2]) -> f32 {
        low + (high - low) * self.next()
    }
}

/// The distance across the 16-bit wrap from `from` to `to`.
fn wrapped(to: f64, from: f64) -> f64 {
    (to - from + 32768.0).rem_euclid(65536.0) - 32768.0
}

/// How hard a craft moving at `velocity` throws spray: none at a crawl, 1
/// at a full drive, up to `MOST` boosted.
fn strength(velocity: [f32; 3]) -> f32 {
    let speed = velocity[0].hypot(velocity[2]);
    ((speed - CRAWL) / (FULL - CRAWL)).clamp(0.0, MOST)
}

/// How long a throw rising at `up` flies before it is back at the water.
fn flight(up: f32) -> f32 {
    2.0 * up / GRAVITY
}

impl SprayCraft {
    /// The player's craft on `world` as it throws spray, if it is a form
    /// that rides the ground (the buggy, the truck and the skimmer): its
    /// four hull points ([`crate::object_runtime::ObjectArena::vehicle_hull_points`]),
    /// each that touches the ground with the standing water under it as
    /// `water` gives it (the puddles' shading's, at world X/Z).
    pub fn of_player(
        world: &crate::world_backend::WorldSession,
        executable: inf_formats::FrontendExecutable<'_>,
        water: impl Fn([f32; 2]) -> f32,
    ) -> Option<Self> {
        let arena = &world.arena;
        let player = arena.first_player()?;
        let state = arena.world_player(player).ok()?;
        let handles = arena.actor_handles()?;
        let unit = handles
            .peek(state.handle_d8)
            .or_else(|| handles.peek(state.handle_d4))?;
        let form = arena.selector2_actor(unit).ok()?.dwords_1e8_1f4[0];
        if !matches!(form, 1 | 2 | 5) {
            return None;
        }
        let velocity = arena
            .actor(unit)
            .ok()?
            .words_8e_92
            .map(|word| f32::from(word as i16));
        let centre = arena.object(unit).ok()?.base.position_words?;
        let points = arena.vehicle_hull_points(unit, executable)?;
        let points = points
            .iter()
            .map(|&[x, y, z]| {
                let ground = world.terrain.height_at([x, z]);
                let touches = i32::from(y as i16) - i32::from(ground) <= TOUCH;
                let across = [
                    wrapped(f64::from(x), f64::from(centre[0])) as f32,
                    wrapped(f64::from(z), f64::from(centre[2])) as f32,
                ];
                let length = across[0].hypot(across[1]).max(1.0);
                SprayPoint {
                    world: [f64::from(x), f64::from(ground), f64::from(z)],
                    out: across.map(|axis| axis / length),
                    water: if touches {
                        water([f32::from(x), f32::from(z)])
                    } else {
                        0.0
                    },
                }
            })
            .collect();
        Some(Self { points, velocity })
    }
}

impl CraftSpray {
    /// Take the craft at `now` (the animation clock, microseconds), or none
    /// where there is no craft to throw spray: each lattice step since the
    /// last update throws from the craft as it was then (between the last
    /// update's and this one's), each ground point in a puddle its drops,
    /// those that fall back on standing water (as `water` says, at world
    /// X/Z) ringing it.
    pub fn update(&mut self, now: u64, craft: Option<SprayCraft>, water: impl Fn([f32; 2]) -> f32) {
        // A clock that ran backwards (another visit) starts it again.
        if self.clock.is_some_and(|then| then > now) {
            *self = Self::default();
        }
        self.clock = Some(now);
        self.bursts
            .retain(|burst| burst.born <= now && now - burst.born < KEEP_MICROS);
        let Some(craft) = craft else {
            self.last = None;
            return;
        };
        let first = match &self.last {
            Some((then, _)) => then / SLOT_MICROS + 1,
            None => now / SLOT_MICROS,
        }
        .max((now / SLOT_MICROS).saturating_sub(MOST_SLOTS - 1));
        for slot in first..=now / SLOT_MICROS {
            let time = slot * SLOT_MICROS;
            // Where the craft was at the step, between the last update and
            // this one.
            let share = match &self.last {
                Some((then, _)) if now > *then && time > *then => {
                    (time - then) as f64 / (now - then) as f64
                }
                _ => 1.0,
            };
            let before = self
                .last
                .as_ref()
                .map(|(_, last)| last)
                .filter(|last| last.points.len() == craft.points.len());
            let velocity: [f32; 3] = std::array::from_fn(|axis| match before {
                Some(last) => {
                    last.velocity[axis]
                        + (craft.velocity[axis] - last.velocity[axis]) * share as f32
                }
                None => craft.velocity[axis],
            });
            let strength = strength(velocity);
            if strength <= 0.0 {
                continue;
            }
            for (index, point) in craft.points.iter().enumerate() {
                let earlier = before.map_or(point, |last| &last.points[index]);
                if point.water <= 0.0 || earlier.water <= 0.0 {
                    continue;
                }
                let at: [f64; 3] = std::array::from_fn(|axis| {
                    let from = earlier.world[axis];
                    let step = if axis == 1 {
                        point.world[axis] - from
                    } else {
                        wrapped(point.world[axis], from)
                    };
                    from + step * share
                });
                let seed = (slot as u32)
                    .wrapping_mul(0x9e37_79b9)
                    .wrapping_add((index as u32).wrapping_mul(0x85eb_ca6b));
                self.bursts.push(burst(
                    time,
                    at,
                    point.out,
                    velocity,
                    strength * point.water.min(earlier.water),
                    seed,
                    &water,
                ));
            }
        }
        self.last = Some((now, craft));
    }

    /// The drops in the air at `now`, seen from `eye` (world units).
    pub fn drops(&self, now: u64, eye: [f64; 3]) -> Vec<SprayDrop> {
        let mut drops = Vec::new();
        for burst in &self.bursts {
            let tau = now.saturating_sub(burst.born) as f32 / 1_000_000.0;
            if burst.born > now {
                continue;
            }
            let at = [
                wrapped(burst.at[0], eye[0]) as f32,
                (burst.at[1] - eye[1]) as f32,
                wrapped(burst.at[2], eye[2]) as f32,
            ];
            for throw in &burst.throws {
                let life = flight(throw.velocity[1]);
                if tau <= 0.0 || tau >= life {
                    continue;
                }
                let [vx, vy, vz] = throw.velocity;
                // Mist thins as it falls back; a drop stays whole till it
                // meets the water.
                let alpha = if throw.mist {
                    MIST_ALPHA * (1.0 - tau / life).sqrt()
                } else {
                    DROP_ALPHA
                };
                drops.push(SprayDrop {
                    position: [
                        at[0] + vx * tau,
                        at[1] + vy * tau - 0.5 * GRAVITY * tau * tau,
                        at[2] + vz * tau,
                    ],
                    velocity: [vx, vy - GRAVITY * tau, vz],
                    radius: throw.radius,
                    alpha,
                });
            }
        }
        drops
    }

    /// The rings the drops leave where they fall back on the water at
    /// `now`, from `eye`: each one's place (eye-relative, at the water) and
    /// how far through its life it is (0 to 1), the nearest
    /// [`MOST_RINGS`].
    pub fn rings(&self, now: u64, eye: [f64; 3]) -> Vec<[f32; 4]> {
        let mut rings = Vec::new();
        for burst in &self.bursts {
            let tau = now.saturating_sub(burst.born) as f32 / 1_000_000.0;
            if burst.born > now {
                continue;
            }
            for throw in burst.throws.iter().filter(|throw| throw.ring) {
                let life = flight(throw.velocity[1]);
                let age = (tau - life) / RING_SECONDS;
                if !(0.0..1.0).contains(&age) {
                    continue;
                }
                rings.push([
                    (wrapped(burst.at[0], eye[0]) + f64::from(throw.velocity[0] * life)) as f32,
                    (burst.at[1] - eye[1]) as f32,
                    (wrapped(burst.at[2], eye[2]) + f64::from(throw.velocity[2] * life)) as f32,
                    age,
                ]);
            }
        }
        rings.sort_by(|a, b| (a[0] * a[0] + a[2] * a[2]).total_cmp(&(b[0] * b[0] + b[2] * b[2])));
        rings.truncate(MOST_RINGS);
        rings
    }
}

/// The drops one ground point at `at` throws at `born`, out along `out`
/// from a craft moving at `velocity`, `strength` hard (`strength`'s scale
/// times the water there): a fan out to the side and up, the drops keeping
/// part of the craft's motion so they trail behind the tyre, a third of
/// them fine mist. Every other drop rings the water it falls back on.
fn burst(
    born: u64,
    at: [f64; 3],
    out: [f32; 2],
    velocity: [f32; 3],
    strength: f32,
    seed: u32,
    water: &impl Fn([f32; 2]) -> f32,
) -> Burst {
    let mut random = Random(seed);
    let count = (DROPS * strength).round() as usize;
    let speed = velocity[0].hypot(velocity[2]).max(1.0);
    let along = [velocity[0] / speed, velocity[2] / speed];
    let at = [
        at[0] + f64::from(out[0]) * TYRE_FACE,
        at[1],
        at[2] + f64::from(out[1]) * TYRE_FACE,
    ];
    let lift_scale = 0.7 + 0.3 * strength;
    let throws = (0..count)
        .map(|index| {
            let mist = index % 3 == 2;
            let up = random.between([120.0, 300.0]) * lift_scale * if mist { 0.7 } else { 1.0 };
            let across =
                random.between([80.0, 260.0]) * strength.sqrt() * if mist { 0.6 } else { 1.0 };
            let carry = random.between([0.4, 0.75]);
            let fan = random.between([-0.35, 0.35]) * across;
            let radius = if mist {
                random.between([6.0, 12.0])
            } else {
                random.between([1.2, 3.0])
            };
            let thrown = [
                velocity[0] * carry + out[0] * across + along[0] * fan,
                up,
                velocity[2] * carry + out[1] * across + along[1] * fan,
            ];
            let life = flight(up);
            let lands = [
                (at[0] + f64::from(thrown[0] * life)).rem_euclid(65536.0) as f32,
                (at[2] + f64::from(thrown[2] * life)).rem_euclid(65536.0) as f32,
            ];
            Throw {
                velocity: thrown,
                radius,
                mist,
                ring: !mist && index % 2 == 0 && water(lands) > 0.0,
            }
        })
        .collect();
    Burst { born, at, throws }
}

/// The fans `camera` draws `drops` as, in `colour` (0 to 1): each a soft
/// disc drawn out along its motion, opaque in its middle and gone at its
/// edge, none at or behind the near plane.
pub fn fans(camera: &SceneCamera, drops: &[SprayDrop], colour: [f32; 3]) -> Vec<Vec<D3dTlVertex>> {
    let [r, g, b] = colour.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
    let near = camera.near.max(1.0);
    let mut fans = Vec::with_capacity(drops.len());
    for drop in drops {
        let Some([x, y, depth]) = crate::scene_gate::project(camera, drop.position) else {
            continue;
        };
        let tail_point: [f32; 3] =
            std::array::from_fn(|axis| drop.position[axis] - drop.velocity[axis] * STREAK_SECONDS);
        let tail = crate::scene_gate::project(camera, tail_point).unwrap_or([x, y, depth]);
        let mut radius = drop.radius * camera.focal[0] / depth;
        let mut alpha = drop.alpha;
        if radius < LEAST_PIXELS {
            alpha *= (radius / LEAST_PIXELS).powi(2);
            radius = LEAST_PIXELS;
        }
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        if a == 0 {
            continue;
        }
        let run = [x - tail[0], y - tail[1]];
        let length = run[0].hypot(run[1]);
        let major = if length > 1.0e-3 {
            [run[0] / length, run[1] / length]
        } else {
            [1.0, 0.0]
        };
        let minor = [-major[1], major[0]];
        let middle = [x - 0.5 * run[0], y - 0.5 * run[1]];
        let corner = |[sx, sy]: [f32; 2], alpha: u8| D3dTlVertex {
            x: sx,
            y: sy,
            depth: 1.0 - near / depth,
            rhw: 1.0 / depth,
            diffuse: [r, g, b, alpha],
            specular: 0xff00_0000,
            uv: [0.0, 0.0],
        };
        let mut fan = vec![corner(middle, a)];
        for step in 0..=8 {
            let turn = step as f32 * std::f32::consts::TAU / 8.0;
            let along = turn.cos() * (radius + 0.5 * length);
            let across = turn.sin() * radius;
            fan.push(corner(
                [
                    middle[0] + major[0] * along + minor[0] * across,
                    middle[1] + major[1] * along + minor[1] * across,
                ],
                0,
            ));
        }
        fans.push(fan);
    }
    fans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn craft(x: f64, speed: f32, water: f32) -> SprayCraft {
        SprayCraft {
            points: vec![
                SprayPoint {
                    world: [x, 100.0, 5000.0],
                    out: [0.0, 1.0],
                    water,
                },
                SprayPoint {
                    world: [x, 100.0, 4856.0],
                    out: [0.0, -1.0],
                    water: 0.0,
                },
            ],
            velocity: [speed, 0.0, 0.0],
        }
    }

    fn wet(_: [f32; 2]) -> f32 {
        1.0
    }

    /// Driven through a puddle at `speed`, from 0.5 s in, how many drops
    /// fly at 1 s and the highest of them above the water.
    fn spray_at(speed: f32) -> (usize, f32) {
        let mut spray = CraftSpray::default();
        for frame in 0..=62 {
            let now = 500_000 + frame * 8_000;
            let x = 1000.0 + f64::from(speed) * (now as f64 / 1e6);
            spray.update(now, Some(craft(x, speed, 1.0)), wet);
        }
        let drops = spray.drops(996_000, [0.0, 0.0, 0.0]);
        let top = drops
            .iter()
            .map(|drop| drop.position[1] - 100.0)
            .fold(0.0, f32::max);
        (drops.len(), top)
    }

    /// Nothing at a crawl; more and higher with speed, the most boosted;
    /// none from the point on dry ground.
    #[test]
    fn spray_grows_with_speed_and_none_at_a_crawl() {
        assert_eq!(spray_at(100.0).0, 0);
        let (slow, slow_top) = spray_at(400.0);
        let (full, full_top) = spray_at(850.0);
        let (boost, boost_top) = spray_at(1700.0);
        assert!(
            slow > 0 && slow < full && full < boost,
            "{slow} {full} {boost}"
        );
        assert!(
            slow_top < full_top && full_top < boost_top,
            "{slow_top} {full_top} {boost_top}"
        );
        assert!(boost_top < 80.0, "{boost_top}");
        let mut spray = CraftSpray::default();
        spray.update(0, Some(craft(0.0, 850.0, 0.0)), wet);
        spray.update(100_000, Some(craft(85.0, 850.0, 0.0)), wet);
        assert!(spray.drops(100_000, [0.0; 3]).is_empty());
    }

    /// The drops are back in the water within a second, and those that
    /// ring it ring it after they land, for less than a second; the same
    /// frames throw the same drops; a clock that runs backwards clears it.
    #[test]
    fn spray_falls_back_and_rings_the_water() {
        let run = || {
            let mut spray = CraftSpray::default();
            spray.update(0, Some(craft(0.0, 1700.0, 1.0)), wet);
            spray.update(48_000, Some(craft(82.0, 1700.0, 1.0)), wet);
            spray.update(96_000, None, wet);
            spray
        };
        let spray = run();
        assert!(!spray.drops(100_000, [0.0; 3]).is_empty());
        assert!(spray.rings(100_000, [0.0; 3]).is_empty());
        assert!(spray.drops(1_100_000, [0.0; 3]).is_empty());
        let rings = spray.rings(700_000, [0.0; 3]);
        assert!(!rings.is_empty() && rings.len() <= MOST_RINGS);
        assert!(rings
            .iter()
            .all(|ring| ring[1] == 100.0 && (0.0..1.0).contains(&ring[3])));
        assert!(spray.rings(2_100_000, [0.0; 3]).is_empty());
        assert_eq!(
            run().drops(300_000, [0.0; 3]),
            spray.drops(300_000, [0.0; 3])
        );
        let mut back = run();
        back.update(10_000, None, wet);
        assert!(back.drops(10_000, [0.0; 3]).is_empty());
        // No ring where the water is not.
        let mut dry = CraftSpray::default();
        dry.update(0, Some(craft(0.0, 1700.0, 1.0)), |_| 0.0);
        dry.update(48_000, Some(craft(82.0, 1700.0, 1.0)), |_| 0.0);
        assert!(dry.rings(700_000, [0.0; 3]).is_empty());
    }
}
