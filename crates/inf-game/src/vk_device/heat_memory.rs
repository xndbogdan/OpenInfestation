//! Thermal vision's heat memory ([the plan](../../../../docs/rt-plans/thermal-vision.md),
//! its step 3): the heat retail's warming lights put on the ground,
//! cooling with a half-life, so an explosion leaves a scorch mark and a hit
//! a warm spot that fade. It is kept by world cells of 64 units and gathers
//! all the time, so what burned before infrared was on is still warm; the
//! tracer gets the 128 by 128 cells round the eye each frame, behind the
//! light grid's cells in the same buffer. The game sees none of it.

use std::collections::HashMap;

/// A cell's side in world units, and the window's side in cells.
pub(super) const CELL: f32 = 64.0;
pub(super) const SIDE: usize = 128;
/// World cells along an axis before the 16-bit wrap.
const WRAP: i32 = 65536 / CELL as i32;
/// How fast a cell gathers a light's heat, per second at the light's middle,
/// over the inner half of its reach, and the most a cell holds: a scorch
/// mark stays below body heat.
const GATHER: f32 = 0.6;
const MOST: f32 = 0.4;
const FOOTPRINT: f32 = 0.5;
/// Below this a cell is cold again and forgotten.
const FORGOTTEN: f32 = 0.005;

#[derive(Debug, Default)]
pub(super) struct HeatMemory {
    cells: HashMap<(i32, i32), f32>,
    clock: Option<u64>,
}

/// A warming light: its world X and Z, its reach, and its heat.
#[derive(Clone, Copy, Debug)]
pub(super) struct WarmLight {
    pub world: [f64; 2],
    pub radius: f32,
    pub heat: f32,
}

fn cell_of(world: f64) -> i32 {
    (world.rem_euclid(65536.0) / f64::from(CELL)).floor() as i32 % WRAP
}

impl HeatMemory {
    /// One frame at game clock `clock_micros`: every cell cools by the time
    /// since the last frame (half-life `half_life` seconds), then gathers
    /// what `lights` put on it, falling off to nothing at each one's reach.
    pub fn step(&mut self, clock_micros: u64, half_life: f32, lights: &[WarmLight]) {
        let seconds = self.clock.map_or(0.0, |last| {
            (clock_micros.saturating_sub(last) as f32 / 1.0e6).min(0.25)
        });
        self.clock = Some(clock_micros);
        if seconds <= 0.0 {
            return;
        }
        let keep = 0.5_f32.powf(seconds / half_life.max(0.1));
        self.cells.retain(|_, heat| {
            *heat *= keep;
            *heat > FORGOTTEN
        });
        for light in lights
            .iter()
            .filter(|light| light.heat > 0.0 && light.radius > 0.0)
        {
            let radius = light.radius * FOOTPRINT;
            let reach = (radius / CELL).ceil() as i32;
            let [centre_x, centre_z] = light.world.map(cell_of);
            for dx in -reach..=reach {
                for dz in -reach..=reach {
                    let offset = [dx, dz].map(|cells| (cells as f32 + 0.5) * CELL);
                    let distance = (offset[0] * offset[0] + offset[1] * offset[1]).sqrt();
                    let falloff = 1.0 - (distance / radius).powi(2);
                    if falloff <= 0.0 {
                        continue;
                    }
                    let key = (
                        (centre_x + dx).rem_euclid(WRAP),
                        (centre_z + dz).rem_euclid(WRAP),
                    );
                    let heat = self.cells.entry(key).or_default();
                    *heat = (*heat + light.heat * falloff * GATHER * seconds).min(MOST);
                }
            }
        }
    }

    /// The window round world X/Z `eye`: its first cell's world cell, then
    /// its SIDE by SIDE cells, X-major.
    pub fn window(&self, eye: [f64; 2]) -> ([i32; 2], Vec<f32>) {
        let half = SIDE as i32 / 2;
        let origin = eye.map(|axis| (cell_of(axis) - half).rem_euclid(WRAP));
        let mut cells = vec![0.0; SIDE * SIDE];
        for (&(x, z), &heat) in &self.cells {
            let along = [x - origin[0], z - origin[1]].map(|cells| cells.rem_euclid(WRAP));
            if along.iter().all(|&cells| cells < SIDE as i32) {
                cells[along[0] as usize * SIDE + along[1] as usize] = heat;
            }
        }
        (origin, cells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An explosion's heat stays on the ground after it, halving every
    /// half-life, and only round where it was.
    #[test]
    fn a_blast_leaves_heat_that_halves_each_half_life() {
        let mut memory = HeatMemory::default();
        let blast = [WarmLight {
            world: [1000.0, 2000.0],
            radius: 500.0,
            heat: 1.0,
        }];
        memory.step(0, 8.0, &blast);
        for frame in 1..=25 {
            memory.step(frame * 20_000, 8.0, &blast);
        }
        let at = |memory: &HeatMemory, world: [f64; 2]| {
            let (origin, cells) = memory.window([1000.0, 2000.0]);
            let along = [0, 1].map(|axis| (cell_of(world[axis]) - origin[axis]).rem_euclid(WRAP));
            cells[along[0] as usize * SIDE + along[1] as usize]
        };
        let warm = at(&memory, [1000.0, 2000.0]);
        assert!(warm > 0.25 && warm <= MOST, "{warm}");
        assert_eq!(at(&memory, [2000.0, 2000.0]), 0.0);
        // Eight seconds on, half of it.
        let mut clock = 25 * 20_000;
        for _ in 0..400 {
            clock += 20_000;
            memory.step(clock, 8.0, &[]);
        }
        let later = at(&memory, [1000.0, 2000.0]);
        assert!((later / warm - 0.5).abs() < 0.02, "{later} {warm}");
    }

    /// Heat across the world's wrap lands beside the eye.
    #[test]
    fn the_window_wraps_with_the_world() {
        let mut memory = HeatMemory::default();
        let light = [WarmLight {
            world: [65500.0, 30.0],
            radius: 200.0,
            heat: 1.0,
        }];
        memory.step(0, 8.0, &light);
        memory.step(100_000, 8.0, &light);
        let (_, cells) = memory.window([40.0, 30.0]);
        assert!(cells.iter().any(|&heat| heat > 0.0));
    }
}
