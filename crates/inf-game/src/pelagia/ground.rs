//! Pelagia's sculpted ground, the rework only: the south-east island raised
//! into a crater isle, a road spiralling up its flanks to a notch in the
//! rim, and a sandbar joining it to the gate's island, so the walk from the
//! gate has a second way to go (2026-10-25: a second look at
//! Pelagia, taking inspiration from other maps). Retail's islands are
//! flat sand a few steps over the water; its levels climb (Level 1's river
//! course, world 3's craters, world 10's rings), so the egg's last cache
//! sits in a crater the player has to climb to.
//!
//! The ground is written whole: heights, materials (world 1's sand 0, rock
//! 1, grass 2) and the baked shade the draws read, from the slope against
//! the world's own light, scaled from the shade the island had.

use crate::terrain_runtime::TerrainRuntime;

/// The crater's centre, world X/Z.
pub(super) const CRATER: [i32; 2] = [51400, 40200];
/// The cone's foot, its rim's crest and the crater floor's edge, as radii.
const CONE_FOOT: f32 = 4400.0;
const RIM: f32 = 2000.0;
const FLOOR_EDGE: f32 = 1300.0;
/// What the rim and the floor stand over the island they rise from.
const RIM_RISE: f32 = 750.0;
const FLOOR_RISE: f32 = 380.0;

/// The road's turns round the crater: bearing in degrees (0 east, 90
/// south), radius, and height over the water plane. From the sandbar's end
/// on the north shore it climbs clockwise round the east and south flanks,
/// through the notch in the west rim and down onto the floor.
const ROAD_TURNS: &[[f32; 3]] = &[
    [-95.0, 5300.0, 150.0],
    [-60.0, 4600.0, 200.0],
    [-25.0, 4100.0, 280.0],
    [10.0, 3700.0, 370.0],
    [45.0, 3400.0, 460.0],
    [80.0, 3100.0, 545.0],
    [115.0, 2800.0, 640.0],
    [150.0, 2400.0, 700.0],
    [170.0, 1900.0, 700.0],
    [180.0, 1300.0, 540.0],
    [190.0, 700.0, 530.0],
];
/// The road's flat half-width and the shoulder blending it into the slope.
const ROAD_HALF: f32 = 300.0;
const ROAD_SHOULDER: f32 = 500.0;
/// How far apart the road's tokens lie, and how far short of the notch
/// they stop (the crystals take over inside).
const ROAD_TOKEN_STEP: f32 = 1150.0;
const ROAD_TOKEN_TURNS: usize = 8;

/// The sandbar from the gate island's south knob to the road's foot, a
/// shallow S across the strait.
const SANDBAR: &[[f32; 2]] = &[
    [49300.0, 32300.0],
    [49800.0, 33150.0],
    [50450.0, 33900.0],
    [50938.0, 34920.0],
];
/// The sandbar's crest over the water plane, its flat half-width and its
/// shoulders down to the strait's bed.
const SANDBAR_TOP: f32 = 40.0;
const SANDBAR_HALF: f32 = 300.0;
const SANDBAR_SHOULDER: f32 = 600.0;

const SAND: u8 = 0;
const ROCK: u8 = 1;
const GRASS: u8 = 2;

/// How many pieces the road's line takes between bearings `from` and `to`:
/// one every two degrees.
fn turn_steps(from: f32, to: f32) -> usize {
    ((to - from).abs() / 2.0).ceil().max(1.0) as usize
}

/// The road's centre line, every two degrees of its turns: X, Z, height.
fn road_line() -> Vec<[f32; 3]> {
    let mut line = Vec::new();
    for pair in ROAD_TURNS.windows(2) {
        let [[a0, r0, h0], [a1, r1, h1]] = [pair[0], pair[1]];
        let steps = turn_steps(a0, a1);
        for step in 0..steps {
            let t = step as f32 / steps as f32;
            let angle = (a0 + (a1 - a0) * t).to_radians();
            let radius = r0 + (r1 - r0) * t;
            line.push([
                CRATER[0] as f32 + radius * angle.cos(),
                CRATER[1] as f32 + radius * angle.sin(),
                h0 + (h1 - h0) * t,
            ]);
        }
    }
    let [a, r, h] = ROAD_TURNS[ROAD_TURNS.len() - 1];
    let angle = a.to_radians();
    line.push([
        CRATER[0] as f32 + r * angle.cos(),
        CRATER[1] as f32 + r * angle.sin(),
        h,
    ]);
    line
}

/// The nearest point of polyline `line` (X, Z, value) to `at`: its
/// distance and the value there.
fn nearest(line: &[[f32; 3]], at: [f32; 2]) -> (f32, f32) {
    let mut best = (f32::MAX, 0.0);
    for pair in line.windows(2) {
        let [[x0, z0, v0], [x1, z1, v1]] = [pair[0], pair[1]];
        let [dx, dz] = [x1 - x0, z1 - z0];
        let length = dx * dx + dz * dz;
        let t = if length > 0.0 {
            (((at[0] - x0) * dx + (at[1] - z0) * dz) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let [px, pz] = [x0 + dx * t, z0 + dz * t];
        let distance = ((at[0] - px).powi(2) + (at[1] - pz).powi(2)).sqrt();
        if distance < best.0 {
            best = (distance, v0 + (v1 - v0) * t);
        }
    }
    best
}

/// The road's tokens: one every [`ROAD_TOKEN_STEP`] along its line from
/// the foot, up to its last climbing turn.
pub(super) fn road_tokens() -> Vec<[u16; 2]> {
    let line = road_line();
    let stop_at: usize = ROAD_TURNS[..ROAD_TOKEN_TURNS]
        .windows(2)
        .map(|pair| turn_steps(pair[0][0], pair[1][0]))
        .sum();
    let mut tokens = Vec::new();
    let mut walked = ROAD_TOKEN_STEP * 0.5;
    let mut next = walked;
    for pair in line[..=stop_at.min(line.len() - 1)].windows(2) {
        let [[x0, z0, _], [x1, z1, _]] = [pair[0], pair[1]];
        let length = ((x1 - x0).powi(2) + (z1 - z0).powi(2)).sqrt();
        while next <= walked + length {
            let t = (next - walked) / length;
            tokens.push([(x0 + (x1 - x0) * t) as u16, (z0 + (z1 - z0) * t) as u16]);
            next += ROAD_TOKEN_STEP;
        }
        walked += length;
    }
    tokens
}

/// Smooth 0..1 as `value` goes from `from` to `to` (either way round).
fn smooth(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in -1..1 at world X/Z over cells of `scale` units, the same
/// on every load.
fn noise(x: f32, z: f32, scale: f32, seed: u32) -> f32 {
    let hash = |i: i32, j: i32| {
        let mut h = (i as u32).wrapping_mul(0x8da6_b343)
            ^ (j as u32).wrapping_mul(0xd816_3841)
            ^ seed.wrapping_mul(0xcb1a_b31f);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 32767.5 - 1.0
    };
    let [fx, fz] = [x / scale, z / scale];
    let [i, j] = [fx.floor() as i32, fz.floor() as i32];
    let [s, t] = [fx - fx.floor(), fz - fz.floor()];
    let [s, t] = [s * s * (3.0 - 2.0 * s), t * t * (3.0 - 2.0 * t)];
    let top = hash(i, j) + (hash(i + 1, j) - hash(i, j)) * s;
    let bottom = hash(i, j + 1) + (hash(i + 1, j + 1) - hash(i, j + 1)) * s;
    top + (bottom - top) * t
}

/// The cone's rise over the island at `at`, its rim a little uneven.
fn cone_rise(at: [f32; 2]) -> f32 {
    let [dx, dz] = [at[0] - CRATER[0] as f32, at[1] - CRATER[1] as f32];
    let angle = dz.atan2(dx);
    let wobble = 1.0 + 0.08 * (3.0 * angle).sin() + 0.05 * (5.0 * angle + 1.3).sin();
    let r = (dx * dx + dz * dz).sqrt() / wobble;
    let crest = RIM_RISE + 70.0 * noise(at[0], at[1], 900.0, 1);
    if r >= CONE_FOOT {
        0.0
    } else if r >= RIM {
        crest * smooth(CONE_FOOT, RIM, r) + 40.0 * noise(at[0], at[1], 500.0, 2)
    } else if r >= FLOOR_EDGE {
        FLOOR_RISE + (crest - FLOOR_RISE) * smooth(FLOOR_EDGE, RIM, r)
    } else {
        FLOOR_RISE + 12.0 * noise(at[0], at[1], 400.0, 3)
    }
}

/// One sculpted cell: its height over the plane, and whether the road or
/// the sandbar paves it.
struct Sculpted {
    height: f32,
    paved: bool,
}

/// The crater isle and the sandbar over cell centre `at`, whose ground
/// stands `ground` over the plane; None leaves the cell as it is.
fn sculpt_at(
    road: &[[f32; 3]],
    sandbar: &[[f32; 3]],
    at: [f32; 2],
    ground: f32,
) -> Option<Sculpted> {
    let mut height = ground;
    let mut touched = false;
    let mut paved = false;
    // The cone rises from the island's own ground, so its shore stays the
    // island's.
    let rise = cone_rise(at);
    if rise > 0.0 && ground > 0.0 {
        height += rise * smooth(0.0, 96.0, ground);
        touched = true;
    }
    let (distance, road_height) = nearest(road, at);
    if distance < ROAD_HALF + ROAD_SHOULDER && ground > -64.0 {
        let blend = smooth(ROAD_HALF, ROAD_HALF + ROAD_SHOULDER, distance);
        height = road_height + (height - road_height) * blend;
        paved |= distance < ROAD_HALF;
        touched = true;
    }
    let (distance, _) = nearest(sandbar, at);
    if distance < SANDBAR_HALF + SANDBAR_SHOULDER {
        let crest = SANDBAR_TOP + 10.0 * noise(at[0], at[1], 600.0, 4);
        let blend = smooth(SANDBAR_HALF, SANDBAR_HALF + SANDBAR_SHOULDER, distance);
        let bar = crest + (ground - crest) * blend;
        if bar > height {
            height = bar;
            paved |= distance < SANDBAR_HALF;
            touched = true;
        }
    }
    touched.then_some(Sculpted { height, paved })
}

/// The material a sculpted cell takes from its height over the plane, its
/// slope and how far it lies from the crater's centre.
fn material_at(at: [f32; 2], height: f32, slope: f32, paved: bool, original: u8) -> u8 {
    if paved {
        return SAND;
    }
    let [dx, dz] = [at[0] - CRATER[0] as f32, at[1] - CRATER[1] as f32];
    let r = (dx * dx + dz * dz).sqrt();
    let patch = noise(at[0], at[1], 700.0, 5);
    if r < FLOOR_EDGE + 150.0 {
        return GRASS;
    }
    if slope > 0.42 || height > 760.0 + 60.0 * patch {
        ROCK
    } else if height > 240.0 + 80.0 * patch {
        if patch > 0.55 {
            ROCK
        } else {
            GRASS
        }
    } else {
        original
    }
}

/// Raise the crater isle and the sandbar. Returns how many cells changed.
pub(super) fn sculpt(terrain: &mut TerrainRuntime) -> usize {
    let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
    let road = road_line();
    let sandbar: Vec<[f32; 3]> = SANDBAR.iter().map(|&[x, z]| [x, z, 0.0]).collect();
    let [x0, z0, x1, z1] = [
        (CRATER[0] - 5600).min(48800),
        (CRATER[1] - 5600).min(31800),
        CRATER[0] + 5600,
        CRATER[1] + 5600,
    ]
    .map(|v| (v.clamp(0, 65535) as u32 >> 7) as u16);
    let side = usize::from(x1 - x0 + 1);
    let rows = usize::from(z1 - z0 + 1);
    // Pass one: heights.
    let mut heights = vec![0f32; side * rows];
    let mut sculpted = vec![None; side * rows];
    for i in 0..side {
        for j in 0..rows {
            let cell = [(x0 + i as u16) << 7, (z0 + j as u16) << 7];
            let ground = (i32::from(terrain.raw_height_at(cell)) - plane) as f32;
            let at = [f32::from(cell[0]) + 64.0, f32::from(cell[1]) + 64.0];
            heights[i * rows + j] = ground;
            if let Some(shape) = sculpt_at(&road, &sandbar, at, ground) {
                heights[i * rows + j] = shape.height;
                sculpted[i * rows + j] = Some(shape.paved);
            }
        }
    }
    // Pass two: materials and shade from the new slopes, then the cells.
    let light = crate::terrain_runtime::normalize_terrain_light(terrain.light_direction_raw());
    let mut changed = 0;
    for i in 1..side - 1 {
        for j in 1..rows - 1 {
            let Some(paved) = sculpted[i * rows + j] else {
                continue;
            };
            let cell = [(x0 + i as u16) << 7, (z0 + j as u16) << 7];
            let at = [f32::from(cell[0]) + 64.0, f32::from(cell[1]) + 64.0];
            let height = heights[i * rows + j];
            let gx = (heights[(i + 1) * rows + j] - heights[(i - 1) * rows + j]) / 256.0;
            let gz = (heights[i * rows + j + 1] - heights[i * rows + j - 1]) / 256.0;
            let slope = (gx * gx + gz * gz).sqrt();
            let normal = {
                let length = (gx * gx + 1.0 + gz * gz).sqrt();
                [-gx / length, 1.0 / length, -gz / length]
            };
            let lit = normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2];
            let original = terrain.cell_at_grid([cell[0] >> 7, cell[1] >> 7]);
            let flat_shade = f32::from(original.shade4().max(5));
            let shade = (flat_shade * lit / light[1].max(0.1))
                .round()
                .clamp(1.0, 15.0) as u8;
            let material = material_at(at, height, slope, paved, original.material());
            terrain.write_sculpted_cell(cell, height.round() as i32 + plane, material, shade);
            changed += 1;
        }
    }
    terrain.refresh_sculpted();
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_road_climbs_gently_and_its_tokens_follow_it() {
        let line = road_line();
        for pair in line.windows(2) {
            let run =
                ((pair[1][0] - pair[0][0]).powi(2) + (pair[1][1] - pair[0][1]).powi(2)).sqrt();
            let grade = (pair[1][2] - pair[0][2]).abs() / run;
            assert!(grade < 0.3, "grade {grade} at {:?}", pair[0]);
        }
        let tokens = road_tokens();
        assert!((10..=16).contains(&tokens.len()), "{} tokens", tokens.len());
        for token in &tokens {
            let (distance, _) = nearest(&line, token.map(f32::from));
            assert!(distance < 2.0, "token {token:?} off the road by {distance}");
        }
    }

    #[test]
    fn the_cone_rises_to_its_rim_and_sinks_to_its_floor() {
        let at = |r: f32| [CRATER[0] as f32 + r, CRATER[1] as f32];
        assert_eq!(cone_rise(at(CONE_FOOT + 400.0)), 0.0);
        assert!(cone_rise(at(RIM)) > 600.0);
        assert!((cone_rise(at(0.0)) - FLOOR_RISE).abs() < 20.0);
    }
}
