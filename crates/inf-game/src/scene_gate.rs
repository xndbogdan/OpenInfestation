//! The active warpgate's lightning, redrawn over the traced world
//! ([the plan](../../../docs/rt-plans/warpgate-effect.md), option (b)): a
//! crisp jagged bolt in place of each of retail's arcs and a glint in place
//! of each of its flares, at retail's own ends and in its sprites' colours,
//! a new shape each tick. The raster draws them right after the traced
//! world, added to it and depth-tested against it, so no upscaler smears
//! them; it skips retail's arcs and flares only while the world is traced.
//! The gate's program still runs every instruction, its random numbers
//! included.

use crate::d3d_viewport::D3dTlVertex;
use crate::modern_tuning::GateTuning;
use crate::scene_feed::{SceneBolt, SceneCamera, SceneGlint};

/// Segments along a bolt, a power of two: its shape halves its steps.
const SEGMENTS: usize = 32;
/// Each finer step of a bolt's shape strays this share of the last, so
/// the fine kinks of lightning stay.
const FINER: f32 = 0.7;
/// Nothing nearer the eye than this many near planes is drawn.
const NEAREST: f32 = 1.5;
/// The thinnest core, in native pixels.
const THINNEST: f32 = 0.6;
/// One spoke in this many throws a fork each tick.
const FORKS: u64 = 6;
/// Spokes meeting this close (world units) share a hub.
const SAME_POINT: f32 = 4.0;

type Point = [f32; 3];

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scaled(a: Point, by: f32) -> Point {
    a.map(|v| v * by)
}

fn dot(a: Point, b: Point) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: Point) -> f32 {
    dot(a, a).sqrt()
}

/// SplitMix64.
fn hash(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A value in -1..1 for `seed`.
fn signed(seed: u64) -> f32 {
    (hash(seed) >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// The points from `ends[0]` to `ends[1]` jagged across the line by
/// midpoint displacement, the first step `jag` of its length at most and
/// each after it [`FINER`] of the last, pinned at both ends.
fn jagged(ends: [Point; 2], jag: f32, seed: u64) -> [Point; SEGMENTS + 1] {
    let along = sub(ends[1], ends[0]);
    let span = length(along);
    let mut points = [ends[0]; SEGMENTS + 1];
    if span <= 0.0 {
        return points;
    }
    let forward = scaled(along, 1.0 / span);
    // Two directions across the line.
    let helper = if forward[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let across = cross(forward, helper);
    let across = scaled(across, 1.0 / length(across));
    let other = cross(forward, across);
    let mut offsets = [[0.0_f32; 2]; SEGMENTS + 1];
    let mut step = SEGMENTS;
    let mut amplitude = jag * span;
    while step > 1 {
        let half = step / 2;
        for start in (0..SEGMENTS).step_by(step) {
            let middle = start + half;
            for axis in 0..2 {
                offsets[middle][axis] = 0.5 * (offsets[start][axis] + offsets[start + step][axis])
                    + amplitude * signed(seed ^ (((middle * 2 + axis) as u64) << 8));
            }
        }
        amplitude *= FINER;
        step = half;
    }
    for (index, point) in points.iter_mut().enumerate() {
        let [a, b] = offsets[index];
        *point = add(
            add(ends[0], scaled(along, index as f32 / SEGMENTS as f32)),
            add(scaled(across, a), scaled(other, b)),
        );
    }
    points
}

/// Where `camera` sees eye-relative `point`: native pixels and view depth,
/// none at or behind the nearest drawn.
pub(crate) fn project(camera: &SceneCamera, point: Point) -> Option<[f32; 3]> {
    let [right, up, forward] = camera.basis;
    let depth = dot(point, forward);
    if depth < camera.near.max(1.0) * NEAREST {
        return None;
    }
    Some([
        camera.centre[0] + camera.focal[0] * dot(point, right) / depth,
        camera.centre[1] - camera.focal[1] * dot(point, up) / depth,
        depth,
    ])
}

/// A raster corner at native `x`, `y` and view depth `depth`, in `colour`
/// (0 to 1, added to what lies behind).
fn corner(camera: &SceneCamera, [x, y]: [f32; 2], depth: f32, colour: [f32; 3]) -> D3dTlVertex {
    let [r, g, b] = colour.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
    D3dTlVertex {
        x,
        y,
        depth: 1.0 - camera.near.max(1.0) / depth,
        rhw: 1.0 / depth,
        diffuse: [r, g, b, 255],
        specular: 0xff00_0000,
        uv: [0.0, 0.0],
    }
}

/// A bolt's look: its core's and halo's half-widths (world units) and
/// colours, and whether it fades out towards its far end (a fork).
struct Strand {
    core: f32,
    halo: f32,
    core_colour: [f32; 3],
    halo_colour: [f32; 3],
    fades: bool,
}

/// How many points to each side a strand's direction on screen is taken
/// over: the jag's finest kinks would swing a wide halo's corners across
/// each other.
const STEADY: usize = 3;
/// The shares of a strand's length its halo narrows over at each end.
const TAPER: f32 = 0.15;

/// The fans of a strand along `points`: across it, from nothing at the
/// halo's edge to the halo's colour at the core's, and the core's colour
/// along its middle.
fn strand(camera: &SceneCamera, points: &[Point], look: &Strand, fans: &mut Vec<Vec<D3dTlVertex>>) {
    let seen: Vec<Option<[f32; 3]>> = points.iter().map(|&point| project(camera, point)).collect();
    // Its run on screen: a strand seen end on is shorter than its own halo
    // and folds into slivers, so it is left out; every point's normal turns
    // the run's way.
    let (Some(first), Some(last)) = (seen.iter().flatten().next(), seen.iter().flatten().last())
    else {
        return;
    };
    let run = [last[0] - first[0], last[1] - first[1]];
    let across_run = [-run[1], run[0]];
    let middle = 0.5 * (first[2] + last[2]);
    let widest = (look.halo * camera.focal[0] / middle).max(THINNEST * 3.0);
    if run[0].hypot(run[1]) < 2.0 * widest {
        return;
    }
    let count = points.len();
    let across: Vec<Option<[D3dTlVertex; 5]>> = (0..count)
        .map(|index| {
            let [x, y, depth] = seen[index]?;
            let before = seen[index.saturating_sub(STEADY)].unwrap_or([x, y, depth]);
            let after = seen[(index + STEADY).min(count - 1)].unwrap_or([x, y, depth]);
            let (dx, dy) = (after[0] - before[0], after[1] - before[1]);
            let length = dx.hypot(dy);
            if length <= 0.0 {
                return None;
            }
            let mut normal = [-dy / length, dx / length];
            if normal[0] * across_run[0] + normal[1] * across_run[1] < 0.0 {
                normal = normal.map(|v| -v);
            }
            let along = index as f32 / (count - 1).max(1) as f32;
            let taper = (along / TAPER).min((1.0 - along) / TAPER).clamp(0.2, 1.0);
            let fade = if look.fades { 1.0 - along } else { 1.0 };
            let pixels = camera.focal[0] / depth;
            let core = (look.core * pixels).max(THINNEST);
            let halo = (look.halo * pixels * taper).max(core * 3.0);
            // Each point's five corners across the strand, and their colours.
            let lanes = [
                ([0.0; 3], -halo),
                (look.halo_colour, -core),
                (look.core_colour, 0.0),
                (look.halo_colour, core),
                ([0.0; 3], halo),
            ];
            Some(lanes.map(|(colour, out)| {
                corner(
                    camera,
                    [x + normal[0] * out, y + normal[1] * out],
                    depth,
                    colour.map(|channel| channel * fade),
                )
            }))
        })
        .collect();
    for pair in across.windows(2) {
        let (Some(from), Some(to)) = (pair[0], pair[1]) else {
            continue;
        };
        for lane in 0..4 {
            fans.push(vec![from[lane], from[lane + 1], to[lane + 1], to[lane]]);
        }
    }
}

/// A glint where retail drew a flare, `size` its quad's half-diagonal in
/// native pixels: a soft hot core and four fine rays, turned a new way
/// each tick.
fn glint(
    camera: &SceneCamera,
    glint: &SceneGlint,
    colour: [f32; 3],
    look: &GateTuning,
    seed: u64,
    fans: &mut Vec<Vec<D3dTlVertex>>,
) {
    let [x, y] = glint.centre;
    let depth = glint.depth;
    if depth < camera.near.max(1.0) * NEAREST {
        return;
    }
    star(
        camera,
        [x, y],
        depth,
        glint.size * look.glint,
        colour,
        look.brightness,
        signed(seed) * std::f32::consts::PI,
        fans,
    );
}

/// A star at native `[x, y]` and view depth `depth`, `radius` native pixels
/// across, in `colour` at `brightness`, its rays turned by `turn`: a disc in
/// the colour itself (a flare's at the hub keeps retail's cycle through the
/// spokes' white), a small hot middle, and four fine rays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn star(
    camera: &SceneCamera,
    [x, y]: [f32; 2],
    depth: f32,
    radius: f32,
    colour: [f32; 3],
    brightness: f32,
    turn: f32,
    fans: &mut Vec<Vec<D3dTlVertex>>,
) {
    let disc = |reach: f32, colour: [f32; 3], fans: &mut Vec<Vec<D3dTlVertex>>| {
        let mut fan = vec![corner(camera, [x, y], depth, colour)];
        for step in 0..=8 {
            let angle = step as f32 * std::f32::consts::TAU / 8.0;
            fan.push(corner(
                camera,
                [x + reach * angle.cos(), y + reach * angle.sin()],
                depth,
                [0.0; 3],
            ));
        }
        fans.push(fan);
    };
    disc(
        radius * 0.5,
        colour.map(|channel| channel * 0.9 * brightness),
        fans,
    );
    disc(
        radius * 0.12,
        colour.map(|channel| (0.5 + 0.5 * channel) * brightness),
        fans,
    );
    let middle = corner(
        camera,
        [x, y],
        depth,
        colour.map(|channel| channel * 0.7 * brightness),
    );
    let half = colour.map(|channel| channel * 0.5 * brightness);
    for ray in 0..4 {
        let angle = turn + ray as f32 * std::f32::consts::FRAC_PI_2;
        let (along, side) = ([angle.cos(), angle.sin()], [-angle.sin(), angle.cos()]);
        let reach = radius * if ray % 2 == 0 { 1.0 } else { 0.7 };
        let base = radius * 0.12;
        let width = (radius * 0.03).max(THINNEST);
        let at = |out: f32, wide: f32| {
            [
                x + along[0] * out + side[0] * wide,
                y + along[1] * out + side[1] * wide,
            ]
        };
        fans.push(vec![
            middle,
            corner(camera, at(base, width), depth, half),
            corner(camera, at(reach, 0.0), depth, [0.0; 3]),
            corner(camera, at(base, -width), depth, half),
        ]);
    }
}

/// The fans that draw `bolts` and `glints` (each with its sprite's colour,
/// brightest channel one) under `camera` at the tick `clock` names.
pub fn fans(
    camera: &SceneCamera,
    bolts: &[(SceneBolt, [f32; 3])],
    glints: &[(SceneGlint, [f32; 3])],
    look: &GateTuning,
    clock: u64,
) -> Vec<Vec<D3dTlVertex>> {
    let mut fans = Vec::new();
    let tick = hash(clock);
    for (index, (bolt, colour)) in bolts.iter().enumerate() {
        let seed = hash(tick ^ ((index as u64) << 32));
        let points = jagged(bolt.ends, look.jag, seed);
        // Each bolt flickers from tick to tick, as retail's arcs do.
        let flicker = look.brightness * (0.85 + 0.15 * signed(seed ^ 0xe0));
        let strand_look = Strand {
            core: bolt.half_width * look.core,
            halo: bolt.half_width * look.halo,
            core_colour: colour.map(|channel| (0.45 + 0.55 * channel) * flicker),
            halo_colour: colour.map(|channel| channel * 0.45 * flicker),
            fades: false,
        };
        strand(camera, &points, &strand_look, &mut fans);
        // Now and then a short fork off the middle of the bolt.
        if hash(seed ^ 0xf0) % FORKS == 0 {
            let from = points[SEGMENTS / 4 + (hash(seed ^ 0xf1) % (SEGMENTS as u64 / 2)) as usize];
            let along = sub(bolt.ends[1], bolt.ends[0]);
            let span = length(along);
            let reach = [0xf2, 0xf3, 0xf4].map(|salt| signed(seed ^ salt) * 0.15 * span);
            let to = add(add(from, scaled(along, 0.2)), reach);
            let fork = jagged([from, to], look.jag, seed ^ 0xf5);
            let fork_look = Strand {
                core: strand_look.core * 0.6,
                halo: strand_look.halo * 0.6,
                core_colour: strand_look.core_colour.map(|channel| channel * 0.8),
                halo_colour: strand_look.halo_colour.map(|channel| channel * 0.8),
                fades: true,
            };
            strand(camera, &fork, &fork_look, &mut fans);
        }
    }
    for (index, (spark, colour)) in glints.iter().enumerate() {
        glint(
            camera,
            spark,
            *colour,
            look,
            hash(tick ^ 0xa0 ^ ((index as u64) << 24)),
            &mut fans,
        );
    }
    fans
}

/// The middles of the openings `bolts` span: for each hub three or more
/// bolts meet at (the gate's spokes), the mean of their far ends, the
/// ring's corners, which stand still while retail jitters the hub.
pub fn openings(bolts: &[SceneBolt]) -> Vec<Point> {
    let mut hubs: Vec<(Point, Vec<Point>)> = Vec::new();
    for bolt in bolts {
        for (hub, far) in [(bolt.ends[0], bolt.ends[1]), (bolt.ends[1], bolt.ends[0])] {
            if let Some((_, corners)) = hubs
                .iter_mut()
                .find(|(at, _)| length(sub(*at, hub)) < SAME_POINT)
            {
                corners.push(far);
            } else {
                hubs.push((hub, vec![far]));
            }
        }
    }
    hubs.into_iter()
        .filter(|(_, corners)| corners.len() >= 3)
        .map(|(_, corners)| {
            let sum = corners
                .iter()
                .fold([0.0; 3], |sum, &corner| add(sum, corner));
            scaled(sum, 1.0 / corners.len() as f32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> SceneCamera {
        SceneCamera {
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [960.0, 960.0],
            centre: [960.0, 540.0],
            native: [1920, 1080],
            near: 16.0,
            far: 30000.0,
            ..SceneCamera::default()
        }
    }

    fn bolt(ends: [Point; 2]) -> SceneBolt {
        SceneBolt {
            ends,
            half_width: 40.0,
            sprite: 0,
            look: 0,
        }
    }

    /// A bolt keeps retail's ends, strays from its line within its steps'
    /// sum (the first `jag` of its length, each finer one [`FINER`] of the
    /// last), and takes a new shape on the next tick but the same one again
    /// on the same tick.
    #[test]
    fn a_bolt_keeps_its_ends_and_changes_each_tick() {
        let ends = [[0.0, 0.0, 600.0], [400.0, 0.0, 600.0]];
        let first = jagged(ends, 0.05, 7);
        assert_eq!((first[0], first[SEGMENTS]), (ends[0], ends[1]));
        let strays = first
            .iter()
            .map(|point| length(sub(*point, [point[0], 0.0, 600.0])));
        let most = strays.fold(0.0, f32::max);
        assert!(most > 2.0 && most < 0.05 * 400.0 / (1.0 - FINER), "{most}");
        assert_eq!(jagged(ends, 0.05, 7), first);
        assert_ne!(jagged(ends, 0.05, 8), first);
    }

    /// Bolts in front of the eye give fans; one behind it gives none.
    #[test]
    fn only_what_lies_ahead_is_drawn() {
        let look = crate::modern_tuning::ModernTuning::defaults()
            .materials()
            .gate_of("gatebod")
            .map(|(_, look)| look.clone())
            .expect("the gate's tuning");
        let ahead = [(
            bolt([[0.0, 0.0, 600.0], [400.0, 0.0, 600.0]]),
            [0.4, 0.6, 1.0],
        )];
        let behind = [(
            bolt([[0.0, 0.0, -600.0], [400.0, 0.0, -600.0]]),
            [0.4, 0.6, 1.0],
        )];
        assert!(!fans(&camera(), &ahead, &[], &look, 1).is_empty());
        assert!(fans(&camera(), &behind, &[], &look, 1).is_empty());
    }

    /// Spokes meeting at one hub give the opening at their far ends'
    /// middle, wherever the hub is; a chain of arcs alone gives none.
    #[test]
    fn the_opening_is_the_middle_of_the_spokes_far_ends() {
        let corners = [
            [-300.0, 0.0, 0.0],
            [300.0, 0.0, 0.0],
            [0.0, 300.0, 0.0],
            [0.0, -300.0, 0.0],
        ];
        let hub = [12.0, -7.0, 0.0];
        let spokes: Vec<SceneBolt> = corners.iter().map(|&corner| bolt([hub, corner])).collect();
        assert_eq!(openings(&spokes), vec![[0.0, 0.0, 0.0]]);
        let chain = [
            bolt([[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]),
            bolt([[100.0, 0.0, 0.0], [200.0, 50.0, 0.0]]),
        ];
        assert!(openings(&chain).is_empty());
    }
}
