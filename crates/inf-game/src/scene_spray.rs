//! The water a blast throws up under the tracer (modern mode, 2026-10-22:
//! grenades exploding in water should make it splash out):
//! jets of white water, each a stream of small drops flying together, and
//! clear drops thrown highest and widest, all traced as drops of their own
//! that catch the sun, and that leave rings and foam where they fall back
//! ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#water)).
//!
//! Retail draws nothing of it. Its water is translucent, so a grenade that
//! sank before going off shows through it, fireball and all; the traced
//! water hides what lies under its surface, so here the water answers the
//! blast instead. Only a picture: the blasts come from the particle pool
//! through the feed, and nothing here reaches the game.

use crate::scene_feed::SceneBlast;

/// How long an eruption shows from its blast: its water falls back within
/// about three seconds, its foam fading after.
pub const ERUPTION_SECONDS: f32 = 4.5;
/// The most eruptions at once; a new one takes the oldest's place.
const ERUPTIONS: usize = 6;
/// Fireballs this close (world units, across) to an eruption younger than
/// [`SAME_BLAST_SECONDS`] are its blast's: one explosion scatters its eight
/// up to 300 units, and they live under two seconds.
const SAME_BLAST_REACH: f64 = 800.0;
const SAME_BLAST_SECONDS: f32 = 2.0;
/// A blast deeper than this (world units) moves too little water to show;
/// one this high above the water (negative depth) throws none.
const DEEPEST_BLAST: f32 = 1500.0;
const HIGHEST_BLAST: f32 = 150.0;
/// Gravity on the water, in world units a second squared.
const GRAVITY: f32 = 980.0;
/// How long a drop's streak is, in seconds of its motion, and how many
/// times its width it may be.
const STREAK_SECONDS: f32 = 0.06;
const MOST_STRETCH: f32 = 5.0;
/// How long the ring a falling jet or drop leaves on the water lasts.
const FALL_SECONDS: f32 = 1.2;
/// The most clear drops an eruption throws, and how many fly in each of
/// its jets, the large and the small.
const DROPS: usize = 160;
const LARGE_JET: usize = 96;
const SMALL_JET: usize = 48;
/// The clear drops are gathered by their bearing into this many sources.
const SECTORS: usize = 8;
/// The share of a jet's water that flies as fine spray round its column.
const FINE_SHARE: f32 = 0.35;
/// What a drop is: clear water, which glints, or white water.
const CLEAR: f32 = 0.0;
const WHITE: f32 = 1.0;

/// One blast in the water: where it rises, when it went off on the game's
/// clock (microseconds), how strongly (1 just under the surface, less
/// deeper or above it, more for the larger explosions), how far under the
/// water's plane its fireballs went off (their middle; negative above it)
/// and what shape its water takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eruption {
    pub x: u16,
    pub z: u16,
    pub born: u64,
    pub strength: f32,
    pub depth: f32,
    pub seed: u32,
}

/// The eruptions showing, kept from frame to frame by the renderer.
#[derive(Clone, Debug, Default)]
pub struct Eruptions {
    list: Vec<Eruption>,
    seeds: u32,
}

/// What the spray looks like (the tuning file's `water`): an eruption's
/// height at full strength (world units, 0: none) and the share of its clear
/// drops thrown (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SprayLook {
    pub plume: f32,
    pub drops: f32,
}

/// One frame's spray for the tracer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spray {
    /// The sources, then their drops, eight floats each (`pt_spray.glsl`'s
    /// `SprayRecord`). A source: its centre and the radius holding its
    /// drops, then the index of its first drop and how many. A drop: its
    /// centre and radius, how it draws out along its motion (along that
    /// direction it is one plus the vector's length times as long), and what
    /// it is: 0 clear water, 1 white water. Eye-relative, the nearest
    /// sources first.
    pub records: Vec<[f32; 8]>,
    pub sources: usize,
    /// The water's surface where it erupts and where its water falls back,
    /// in the splash buffer's form: the centre (eye-relative), then for an
    /// eruption (kind 1) the seconds since its blast and its size (1 at full
    /// strength and the default height), and for a fall (kind 0) how far
    /// through its ring's life it is and its scale; then the kind, and how
    /// deep under the surface the eruption's blast went off (0 for a fall,
    /// and for a blast above the water), where it glows from.
    pub surface: Vec<[f32; 7]>,
}

fn seconds(micros: u64) -> f32 {
    micros as f32 / 1_000_000.0
}

/// The distance across the 16-bit wrap from `from` to `to`.
fn wrapped(to: f64, from: f64) -> f64 {
    (to - from + 32768.0).rem_euclid(65536.0) - 32768.0
}

/// How strongly `blast` throws the water up, if it does.
fn strength(blast: &SceneBlast) -> Option<f32> {
    let depth = blast.depth?;
    if !(-HIGHEST_BLAST..=DEEPEST_BLAST).contains(&depth) {
        return None;
    }
    let base = if depth < 0.0 {
        1.0 + depth / HIGHEST_BLAST
    } else {
        1.0 - 0.5 * ((depth - 300.0) / (DEEPEST_BLAST - 300.0)).clamp(0.0, 1.0)
    };
    Some(base * if blast.large { 1.25 } else { 1.0 })
}

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

    fn between(&mut self, range: [f32; 2]) -> f32 {
        range[0] + (range[1] - range[0]) * self.next()
    }
}

/// Water thrown from the surface at `launch` seconds with `velocity`,
/// `size` across, falling under gravity until it is back at the surface: a
/// clear drop, or a jet of white water of `drops` drops.
struct Throw {
    launch: f32,
    velocity: [f32; 3],
    size: f32,
    kind: f32,
    drops: usize,
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let length = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if length > 0.0 {
        v.map(|x| x / length)
    } else {
        [0.0, 1.0, 0.0]
    }
}

impl Throw {
    /// How long it flies.
    fn life(&self) -> f32 {
        2.0 * self.velocity[1] / GRAVITY
    }

    /// The drop at `time` seconds, from `centre`, drawn out along its
    /// motion; white water thins after its height as it breaks up.
    fn drop(&self, time: f32, centre: [f32; 3]) -> Option<[f32; 8]> {
        let tau = time - self.launch;
        let life = self.life();
        if tau <= 0.0 || tau >= life {
            return None;
        }
        let up = self.velocity[1];
        let falling = ((tau - 0.5 * life) / (0.5 * life)).max(0.0);
        let radius = if self.kind == WHITE {
            self.size * (1.0 - 0.6 * falling)
        } else {
            self.size
        };
        let moving = [self.velocity[0], up - GRAVITY * tau, self.velocity[2]];
        let speed = moving.iter().map(|v| v * v).sum::<f32>().sqrt();
        let stretch = (speed * STREAK_SECONDS / radius).min(MOST_STRETCH - 1.0);
        let along = moving.map(|v| {
            if speed > 0.0 {
                v / speed * stretch
            } else {
                0.0
            }
        });
        Some([
            centre[0] + self.velocity[0] * tau,
            centre[1] + up * tau - 0.5 * GRAVITY * tau * tau,
            centre[2] + self.velocity[2] * tau,
            radius,
            along[0],
            along[1],
            along[2],
            self.kind,
        ])
    }

    /// A jet at `time` seconds, from `centre`: water thrown at once at
    /// speeds from the head's down to half of it, so the slower water
    /// trails the head back to the surface as a column. Its pieces are
    /// large blobs at the base, smaller towards the head, thinning once
    /// past the top of their flight, with a fringe of fine spray; the
    /// column widens as it flies. Each piece is drawn out along its own
    /// motion; those back under the surface are gone. `seed` places them,
    /// the same each frame.
    fn jet(&self, time: f32, centre: [f32; 3], seed: u32) -> Vec<[f32; 8]> {
        let tau = time - self.launch;
        if tau <= 0.0 || tau >= self.life() {
            return Vec::new();
        }
        let axis = normalized(self.velocity);
        let side = normalized(if axis[1].abs() > 0.99 {
            [1.0, 0.0, 0.0]
        } else {
            cross(axis, [0.0, 1.0, 0.0])
        });
        let other = cross(axis, side);
        let mut random = Random(seed);
        let mut drops = Vec::with_capacity(self.drops);
        for _ in 0..self.drops {
            // Its numbers drawn whether or not it shows, so the others keep
            // their places: how far back in the column (0 the head), a
            // point of the unit disc across it, and whether it is spray.
            let back = random.next();
            let turn = std::f32::consts::TAU * random.next();
            let across = random.next().sqrt();
            let fine = random.next() < FINE_SHARE;
            let size = random.next();
            let share = 1.0 - 0.5 * back;
            let velocity = self.velocity.map(|v| v * share);
            let up = velocity[1];
            let life = 2.0 * up / GRAVITY;
            if tau >= life {
                continue;
            }
            let spread = self.size * (0.3 + 1.2 * tau) * if fine { 2.0 } else { 1.0 };
            let [a, b] = [across * turn.cos(), across * turn.sin()];
            let position: [f32; 3] = std::array::from_fn(|i| {
                centre[i] + velocity[i] * tau + (side[i] * a + other[i] * b) * spread
                    - if i == 1 {
                        0.5 * GRAVITY * tau * tau
                    } else {
                        0.0
                    }
            });
            if position[1] < centre[1] {
                continue;
            }
            let falling = ((tau - 0.5 * life) / (0.5 * life)).max(0.0);
            let radius = if fine {
                1.0 + 1.5 * size
            } else {
                (self.size * (0.12 + 0.5 * back) * (0.7 + 0.6 * size)).clamp(2.5, 30.0)
            } * (1.0 - 0.6 * falling);
            let moving = [velocity[0], up - GRAVITY * tau, velocity[2]];
            let speed = moving.iter().map(|v| v * v).sum::<f32>().sqrt();
            let stretch = (speed * STREAK_SECONDS / radius).min(MOST_STRETCH - 1.0);
            let drawn = normalized(moving).map(|v| v * stretch);
            drops.push([
                position[0],
                position[1],
                position[2],
                radius,
                drawn[0],
                drawn[1],
                drawn[2],
                WHITE,
            ]);
        }
        drops
    }

    /// Where and when it falls back on the water.
    fn fall(&self, centre: [f32; 3]) -> ([f32; 3], f32) {
        let life = self.life();
        (
            [
                centre[0] + self.velocity[0] * life,
                centre[1],
                centre[2] + self.velocity[2] * life,
            ],
            self.launch + life,
        )
    }
}

/// How far a drop record reaches from its centre.
fn reach(drop: &[f32; 8]) -> f32 {
    let stretch = (drop[4] * drop[4] + drop[5] * drop[5] + drop[6] * drop[6]).sqrt();
    drop[3] * (1.0 + stretch)
}

/// What an eruption throws, from the blast's `seed`, at `height` and
/// `strength`, with its share of clear drops `drops`: white jets straight
/// up as the dome breaks, a crown of smaller ones flung up and out, and the
/// clear drops, thrown highest and widest. Each with the scale of the ring
/// it leaves where it falls back.
fn throws(seed: u32, height: f32, strength: f32, drops: f32) -> Vec<(Throw, f32)> {
    let h = height * strength.min(1.25);
    let speed = (2.0 * GRAVITY * h).sqrt();
    let mut random = Random(seed);
    let mut throws = Vec::new();
    let mut throw = |random: &mut Random,
                     launch: [f32; 2],
                     elevation: [f32; 2],
                     pace: [f32; 2],
                     size: f32,
                     drops: usize,
                     ring: f32| {
        let launch = random.between(launch);
        let angle = random.between(elevation).to_radians();
        let pace = speed * random.between(pace);
        let bearing = std::f32::consts::TAU * random.next();
        throws.push((
            Throw {
                launch,
                velocity: [
                    bearing.cos() * pace * angle.cos(),
                    pace * angle.sin(),
                    bearing.sin() * pace * angle.cos(),
                ],
                size,
                kind: if drops > 0 { WHITE } else { CLEAR },
                drops,
            },
            ring,
        ));
    };
    for _ in 0..12 {
        let size = h * random.between([0.035, 0.06]);
        throw(
            &mut random,
            [0.03, 0.2],
            [75.0, 90.0],
            [0.55, 1.0],
            size,
            LARGE_JET,
            0.6,
        );
    }
    for _ in 0..12 {
        let size = h * random.between([0.02, 0.035]);
        throw(
            &mut random,
            [0.04, 0.14],
            [35.0, 60.0],
            [0.45, 0.7],
            size,
            SMALL_JET,
            0.4,
        );
    }
    let clear = (DROPS as f32 * drops.clamp(0.0, 1.0)).round() as usize;
    for index in 0..clear {
        let size = random.between([2.5, 5.0]) * (0.6 + 0.4 * strength.min(1.0));
        // Every fourth leaves a ring of its own.
        let ring = if index % 4 == 0 { 0.12 } else { 0.0 };
        throw(
            &mut random,
            [0.02, 0.35],
            [30.0, 88.0],
            [0.3, 1.1],
            size,
            0,
            ring,
        );
    }
    throws
}

/// The source holding `drops`: a sphere round them all.
fn source(first: usize, drops: &[[f32; 8]]) -> [f32; 8] {
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];
    for drop in drops {
        for axis in 0..3 {
            low[axis] = low[axis].min(drop[axis] - reach(drop));
            high[axis] = high[axis].max(drop[axis] + reach(drop));
        }
    }
    let centre: [f32; 3] = std::array::from_fn(|axis| 0.5 * (low[axis] + high[axis]));
    let radius = drops
        .iter()
        .map(|drop| {
            let d: f32 = (0..3).map(|axis| (drop[axis] - centre[axis]).powi(2)).sum();
            d.sqrt() + reach(drop)
        })
        .fold(0.0, f32::max);
    [
        centre[0],
        centre[1],
        centre[2],
        radius,
        first as f32,
        drops.len() as f32,
        0.0,
        0.0,
    ]
}

impl Eruptions {
    /// Take this frame's blasts at `now` (the game's clock, microseconds):
    /// a blast in the water away from any young eruption starts one.
    pub fn update(&mut self, now: u64, blasts: &[SceneBlast]) {
        // A clock that ran backwards (another game) starts them again.
        self.list.retain(|eruption| {
            eruption.born <= now && seconds(now - eruption.born) < ERUPTION_SECONDS
        });
        let near = |x: u16, z: u16, other: [u16; 3]| {
            let across = wrapped(f64::from(other[0]), f64::from(x));
            let along = wrapped(f64::from(other[2]), f64::from(z));
            across.hypot(along) < SAME_BLAST_REACH
        };
        for blast in blasts {
            if strength(blast).is_none() {
                continue;
            }
            let [x, _, z] = blast.world;
            let known = self.list.iter().any(|eruption| {
                seconds(now - eruption.born) < SAME_BLAST_SECONDS
                    && near(eruption.x, eruption.z, blast.world)
            });
            if known {
                continue;
            }
            // It rises from the middle of its blast's fireballs, as strongly
            // as the strongest.
            let cluster: Vec<&SceneBlast> = blasts
                .iter()
                .filter(|other| strength(other).is_some() && near(x, z, other.world))
                .collect();
            let middle = |axis: usize, from: u16| {
                let sum: f64 = cluster
                    .iter()
                    .map(|other| wrapped(f64::from(other.world[axis]), f64::from(from)))
                    .sum();
                (f64::from(from) + sum / cluster.len() as f64).rem_euclid(65536.0) as u16
            };
            let strength = cluster
                .iter()
                .filter_map(|other| strength(other))
                .fold(0.0, f32::max);
            let depth =
                cluster.iter().filter_map(|other| other.depth).sum::<f32>() / cluster.len() as f32;
            if self.list.len() == ERUPTIONS {
                self.list.remove(0);
            }
            self.seeds = self.seeds.wrapping_add(0x6d2b_79f5);
            self.list.push(Eruption {
                x: middle(0, x),
                z: middle(2, z),
                born: now,
                strength,
                depth,
                seed: self.seeds,
            });
        }
    }

    pub fn list(&self) -> &[Eruption] {
        &self.list
    }

    /// This frame's spray at `now`, seen from `eye` (world units), over
    /// water whose plane stands at `water_level`.
    pub fn spray(&self, now: u64, eye: [f64; 3], water_level: f32, look: SprayLook) -> Spray {
        let mut groups: Vec<Vec<[f32; 8]>> = Vec::new();
        let mut surface = Vec::new();
        let mut falls = Vec::new();
        for eruption in &self.list {
            let time = seconds(now.saturating_sub(eruption.born));
            let centre = [
                wrapped(f64::from(eruption.x), eye[0]) as f32,
                (f64::from(water_level) - eye[1]) as f32,
                wrapped(f64::from(eruption.z), eye[2]) as f32,
            ];
            let size = eruption.strength * look.plume / 800.0;
            surface.push([
                centre[0],
                centre[1],
                centre[2],
                time,
                size,
                1.0,
                eruption.depth.max(0.0),
            ]);
            if look.plume <= 0.0 {
                continue;
            }
            // Each jet a source of its own, the clear drops gathered by
            // their bearing.
            let mut sectors = vec![Vec::new(); SECTORS];
            let throws = throws(eruption.seed, look.plume, eruption.strength, look.drops);
            for (index, (throw, ring)) in throws.into_iter().enumerate() {
                if throw.drops > 0 {
                    let seed = eruption.seed ^ (index as u32).wrapping_mul(0x9e37_79b9);
                    groups.push(throw.jet(time, centre, seed));
                } else if let Some(drop) = throw.drop(time, centre) {
                    let bearing = throw.velocity[2].atan2(throw.velocity[0]);
                    let sector = ((bearing / std::f32::consts::TAU + 0.5) * SECTORS as f32)
                        as usize
                        % SECTORS;
                    sectors[sector].push(drop);
                }
                let ([x, y, z], landing) = throw.fall(centre);
                let age = (time - landing) / FALL_SECONDS;
                if ring > 0.0 && (0.0..1.0).contains(&age) {
                    falls.push([x, y, z, age, ring * look.plume / 800.0, 0.0, 0.0]);
                }
            }
            groups.extend(sectors);
        }
        groups.retain(|drops| !drops.is_empty());
        falls.sort_by(|a, b| (a[0] * a[0] + a[2] * a[2]).total_cmp(&(b[0] * b[0] + b[2] * b[2])));
        surface.extend(falls);
        // The nearest first, so the tracer can stop at the first it meets.
        groups.sort_by(|a, b| {
            let reach = |drops: &Vec<[f32; 8]>| {
                let [x, y, z, radius, ..] = source(0, drops);
                (x * x + y * y + z * z).sqrt() - radius
            };
            reach(a).total_cmp(&reach(b))
        });
        let sources = groups.len();
        let mut records = Vec::with_capacity(sources + groups.iter().map(Vec::len).sum::<usize>());
        let mut first = sources;
        for drops in &groups {
            records.push(source(first, drops));
            first += drops.len();
        }
        for drops in groups {
            records.extend(drops);
        }
        Spray {
            records,
            sources,
            surface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOK: SprayLook = SprayLook {
        plume: 800.0,
        drops: 1.0,
    };

    fn blast(x: u16, depth: f32) -> SceneBlast {
        SceneBlast {
            world: [x, 0, 5000],
            depth: Some(depth),
            large: false,
        }
    }

    /// An explosion's fireballs start one eruption, from their middle, and
    /// later ones of the same blast join it; one in the air, on dry ground
    /// or too deep starts none; another blast elsewhere starts its own.
    #[test]
    fn a_blast_in_the_water_starts_one_eruption() {
        let mut eruptions = Eruptions::default();
        eruptions.update(1_000_000, &[blast(1000, 300.0), blast(1200, 310.0)]);
        assert_eq!(eruptions.list().len(), 1);
        assert_eq!(eruptions.list()[0].x, 1100);
        assert_eq!(eruptions.list()[0].depth, 305.0);
        eruptions.update(1_500_000, &[blast(1100, 280.0)]);
        assert_eq!(eruptions.list().len(), 1);
        let dry = SceneBlast {
            depth: None,
            ..blast(9000, 0.0)
        };
        eruptions.update(1_600_000, &[blast(9000, -400.0), blast(9000, 3000.0), dry]);
        assert_eq!(eruptions.list().len(), 1);
        eruptions.update(1_700_000, &[blast(20000, 50.0)]);
        assert_eq!(eruptions.list().len(), 2);
        // A blast across the wrap is near.
        eruptions.update(1_800_000, &[blast(65400, 50.0), blast(65535, 60.0)]);
        assert_eq!(eruptions.list().len(), 3);
        // Gone after their span; a clock that runs backwards clears them.
        eruptions.update(1_000_000 + 4_600_000, &[]);
        assert_eq!(eruptions.list().len(), 2);
        eruptions.update(0, &[]);
        assert!(eruptions.list().is_empty());
    }

    /// Deeper blasts throw less water; the larger explosions more.
    #[test]
    fn deeper_blasts_are_weaker() {
        let shallow = strength(&blast(0, 100.0)).unwrap();
        let deep = strength(&blast(0, 1400.0)).unwrap();
        let above = strength(&blast(0, -100.0)).unwrap();
        assert_eq!(shallow, 1.0);
        assert!(deep < 0.6 && deep > 0.4, "{deep}");
        assert!(above < 0.4, "{above}");
        let large = SceneBlast {
            large: true,
            ..blast(0, 100.0)
        };
        assert_eq!(strength(&large), Some(1.25));
    }

    /// The water rises and falls back: nothing under the surface, the jets
    /// near the plume's height about one and a half seconds in, and nothing
    /// left but the eruption's own entry once its water has fallen.
    #[test]
    fn the_water_rises_and_falls_back() {
        let mut eruptions = Eruptions::default();
        eruptions.update(0, &[blast(1000, 200.0)]);
        let eye = [1000.0, 500.0, 4000.0];
        let top = |time: f32| {
            let spray = eruptions.spray((time * 1e6) as u64, eye, 0.0, LOOK);
            assert_eq!(spray.surface[0][5], 1.0);
            let drops = &spray.records[spray.sources..];
            for drop in drops {
                assert!(drop[1] + drop[3] > -500.0, "under the water: {drop:?}");
            }
            drops
                .iter()
                .map(|drop| drop[1] + 500.0)
                .fold(f32::MIN, f32::max)
        };
        assert!(top(0.3) < top(1.2));
        let highest = top(1.3);
        assert!(highest > 600.0 && highest < 1200.0, "{highest}");
        assert!(top(2.6) < highest);
        let late = eruptions.spray(4_400_000, eye, 0.0, LOOK);
        assert!(late.records.is_empty(), "{:?}", late.records);
        assert_eq!(
            late.surface,
            vec![[0.0, -500.0, 1000.0, 4.4, 1.0, 1.0, 200.0]]
        );
    }

    /// A jet is a column: its slower water trails the head down towards
    /// the surface, large pieces among fine spray.
    #[test]
    fn a_jet_is_a_column_of_water() {
        let throw = Throw {
            launch: 0.0,
            velocity: [0.0, 1000.0, 0.0],
            size: 40.0,
            kind: WHITE,
            drops: LARGE_JET,
        };
        let drops = throw.jet(0.5, [0.0, 0.0, 0.0], 7);
        let heights = drops.iter().map(|drop| drop[1]);
        let low = heights.clone().fold(f32::MAX, f32::min);
        let high = heights.fold(f32::MIN, f32::max);
        // The head at 1000 x 0.5 - 490 x 0.25 = 377.5, the slowest water
        // (half the speed) at 127.5.
        assert!(high > 300.0 && low < 200.0, "{low} to {high}");
        assert!(drops.iter().any(|drop| drop[3] < 2.6));
        assert!(drops.iter().any(|drop| drop[3] > 8.0));
    }

    /// Where the jets and some drops fall back the water rings, each for
    /// its own span after it lands.
    #[test]
    fn falling_water_rings_the_surface() {
        let mut eruptions = Eruptions::default();
        eruptions.update(0, &[blast(1000, 200.0)]);
        let rings = |time: f32| {
            let spray = eruptions.spray((time * 1e6) as u64, [1000.0, 500.0, 5000.0], 0.0, LOOK);
            spray.surface[1..].to_vec()
        };
        assert!(rings(0.2).is_empty());
        let later = rings(2.5);
        assert!(!later.is_empty());
        for ring in &later {
            assert_eq!(ring[5], 0.0);
            assert_eq!(ring[1], -500.0);
            assert!((0.0..1.0).contains(&ring[3]), "{ring:?}");
        }
    }

    /// Each source's sphere holds its drops, the nearest source first;
    /// every drop belongs to one source.
    #[test]
    fn sources_hold_their_drops_nearest_first() {
        let mut eruptions = Eruptions::default();
        eruptions.update(0, &[blast(9000, 100.0)]);
        eruptions.update(10_000, &[blast(2000, 100.0)]);
        let spray = eruptions.spray(400_000, [0.0, 200.0, 5000.0], 0.0, LOOK);
        assert!(spray.sources > 20, "{}", spray.sources);
        let mut last = f32::MIN;
        let mut counted = 0;
        for source in &spray.records[..spray.sources] {
            let first = source[4] as usize;
            let count = source[5] as usize;
            assert!(count > 0);
            assert_eq!(first, spray.sources + counted);
            counted += count;
            for drop in &spray.records[first..first + count] {
                let d = ((drop[0] - source[0]).powi(2)
                    + (drop[1] - source[1]).powi(2)
                    + (drop[2] - source[2]).powi(2))
                .sqrt();
                assert!(
                    d + reach(drop) <= source[3] * 1.0001,
                    "{d} {drop:?} {source:?}"
                );
            }
            let reach =
                (source[0].powi(2) + source[1].powi(2) + source[2].powi(2)).sqrt() - source[3];
            assert!(reach >= last);
            last = reach;
        }
        assert_eq!(spray.sources + counted, spray.records.len());
    }
}
