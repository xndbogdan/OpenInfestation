//! Thermal vision's heat by object ([the plan](../../../../docs/rt-plans/thermal-vision.md),
//! its step 4): what each object drawn does warms it. A moving object runs
//! warmer, a new shot warms the guns that fired it, and a wreck or a corpse
//! whose object was seen alive cools from the heat it had. Each object's
//! warmth eases over a third of a second, so it never flickers. The game
//! sees none of it.

use std::collections::HashMap;

use crate::modern_tuning::ThermalTuning;
use crate::object_runtime::ObjectId;
use crate::scene_feed::SceneObject;

/// The time an object's warmth takes to ease most of the way, in seconds.
const EASE: f32 = 1.0 / 3.0;
/// A step longer than this (seconds) is a gap, the object out of sight:
/// it gives no speed.
const GAP: f32 = 0.25;
/// A move farther than this in one step is a jump (a respawn), not speed.
const JUMP: f32 = 2000.0;
/// How near a shot's birth an object's origin lies for it to have fired.
const REACH: f32 = 500.0;
/// Objects unseen this long are forgotten (microseconds).
const FORGET: u64 = 120_000_000;

/// What thermal vision adds to an object's faces: `warmth` to all of them,
/// `fired` to its guns' too.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ObjectWarmth {
    pub warmth: f32,
    pub fired: f32,
}

/// An object's root model as the tuning sees it: its heat, and whether it
/// is a wreck.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct RootLook {
    pub heat: f32,
    pub wreck: bool,
}

#[derive(Debug, Default)]
pub(super) struct ObjectHeat {
    objects: HashMap<ObjectId, Tracked>,
    clock: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Tracked {
    position: [i32; 3],
    seen: u64,
    wreck: bool,
    /// Its root's heat and warmth at its last step.
    heat: f32,
    /// World units a second, eased.
    speed: f32,
    /// What the last shot left its guns, cooling.
    fired: f32,
    /// A wreck's heat over its model's, cooling.
    residual: f32,
    shown: ObjectWarmth,
}

/// The distance between two world points, wrapping at 16 bits.
fn distance(a: [i32; 3], b: [i32; 3]) -> f32 {
    let along = |axis: usize| f32::from(a[axis].wrapping_sub(b[axis]) as i16);
    (along(0).powi(2) + along(1).powi(2) + along(2).powi(2)).sqrt()
}

impl ObjectHeat {
    /// One frame at game clock `clock` (microseconds): each object with its
    /// root's look steps by the time since it was last seen, and each shot
    /// born at `shots` warms the guns of the object nearest it. Gives each
    /// object's warmth, in `objects`' order.
    pub fn step(
        &mut self,
        clock: u64,
        objects: &[(SceneObject, RootLook)],
        shots: &[[u16; 3]],
        thermal: &ThermalTuning,
    ) -> Vec<ObjectWarmth> {
        // A new world, or a replay started again.
        if self.clock.is_some_and(|last| clock < last) {
            self.objects.clear();
        }
        self.clock = Some(clock);
        let mut fired = vec![false; objects.len()];
        for shot in shots {
            let shot = shot.map(i32::from);
            let nearest = objects
                .iter()
                .map(|(object, _)| distance(object.position, shot))
                .enumerate()
                .filter(|&(_, away)| away < REACH)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((index, _)) = nearest {
                fired[index] = true;
            }
        }
        let warmth = objects
            .iter()
            .zip(fired)
            .map(|((object, root), fired)| {
                let tracked = self.objects.entry(object.id).or_insert(Tracked {
                    position: object.position,
                    seen: clock,
                    wreck: root.wreck,
                    heat: root.heat,
                    speed: 0.0,
                    fired: 0.0,
                    residual: 0.0,
                    shown: ObjectWarmth::default(),
                });
                tracked.step(clock, object.position, *root, fired, thermal)
            })
            .collect();
        self.objects
            .retain(|_, tracked| clock.saturating_sub(tracked.seen) < FORGET);
        warmth
    }
}

impl Tracked {
    fn step(
        &mut self,
        clock: u64,
        position: [i32; 3],
        root: RootLook,
        fired: bool,
        thermal: &ThermalTuning,
    ) -> ObjectWarmth {
        let seconds = clock.saturating_sub(self.seen) as f32 / 1.0e6;
        self.seen = clock;
        let halve = |half_life: f32| 0.5_f32.powf(seconds / half_life.max(0.1));
        let ease = 1.0 - (-seconds / EASE).exp();
        let moved = distance(position, self.position);
        self.position = position;
        if seconds > GAP || moved > JUMP {
            self.speed = 0.0;
        } else if seconds > 0.0 {
            self.speed += (moved / seconds - self.speed) * ease;
        }
        let (warm_by, fired_by) = thermal.fired;
        self.fired *= halve(fired_by);
        if fired {
            self.fired = warm_by;
        }
        // It died: what it had over the wreck's own heat stays, cooling.
        if root.wreck && !self.wreck {
            self.residual = (self.heat - root.heat).max(0.0);
        }
        self.wreck = root.wreck;
        self.shown = if root.wreck {
            self.residual *= halve(thermal.cooling);
            ObjectWarmth {
                warmth: self.residual,
                fired: 0.0,
            }
        } else {
            let (rise, full) = thermal.moving;
            let warmth = rise * (self.speed / full.max(1.0)).min(1.0);
            ObjectWarmth {
                warmth: self.shown.warmth + (warmth - self.shown.warmth) * ease,
                fired: self.shown.fired + (self.fired - self.shown.fired) * ease,
            }
        };
        self.heat = root.heat + self.shown.warmth;
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modern_tuning::ModernTuning;

    const FRAME: u64 = 20_000;

    fn object(id: ObjectId, x: i32) -> SceneObject {
        SceneObject {
            id,
            position: [x, 0, 0],
            root: Some(1),
        }
    }

    fn ids() -> [ObjectId; 2] {
        let mut arena = crate::object_runtime::ObjectArena::new();
        [(); 2].map(|_| arena.allocate(crate::object_runtime::ObjectClassKind::VisualBase))
    }

    /// A machine driving at full speed runs warmer by `moving`, easing up
    /// to it, and cools back once it stops; one standing still stays as it
    /// was.
    #[test]
    fn a_moving_object_runs_warmer() {
        let defaults = ModernTuning::defaults();
        let thermal = defaults.thermal();
        let (rise, full) = thermal.moving;
        let [driving, parked] = ids();
        let root = RootLook {
            heat: 0.45,
            wreck: false,
        };
        let mut heat = ObjectHeat::default();
        let per_frame = (full * FRAME as f32 / 1.0e6) as i32;
        let mut last = Vec::new();
        let mut at = 0;
        for frame in 0..200 {
            at = (frame * per_frame).rem_euclid(65536);
            last = heat.step(
                frame as u64 * FRAME,
                &[(object(driving, at), root), (object(parked, 9000), root)],
                &[],
                thermal,
            );
        }
        assert!((last[0].warmth - rise).abs() < 0.01 * rise, "{last:?}");
        assert_eq!(last[1], ObjectWarmth::default());
        for frame in 200..400 {
            last = heat.step(
                frame as u64 * FRAME,
                &[(object(driving, at), root)],
                &[],
                thermal,
            );
        }
        assert!(last[0].warmth < 0.01 * rise, "{last:?}");
    }

    /// A shot born by an object's guns warms them by `fired` (the nearest
    /// object fired it), cooling by half each half-life.
    #[test]
    fn a_shot_warms_the_guns_that_fired_it() {
        let defaults = ModernTuning::defaults();
        let thermal = defaults.thermal();
        let (warm_by, half_life) = thermal.fired;
        let [shooter, other] = ids();
        let root = RootLook {
            heat: 0.45,
            wreck: false,
        };
        let objects = [(object(shooter, 1000), root), (object(other, 1300), root)];
        let mut heat = ObjectHeat::default();
        heat.step(0, &objects, &[[1100, 0, 0]], thermal);
        let mut last = Vec::new();
        let frames = (half_life * 1.0e6 / FRAME as f32) as u64;
        for frame in 1..=frames {
            last = heat.step(frame * FRAME, &objects, &[], thermal);
        }
        // Easing lags the cooling a little.
        assert!((last[0].fired / warm_by - 0.5).abs() < 0.05, "{last:?}");
        assert_eq!(last[1].fired, 0.0);
        let far = heat.step((frames + 1) * FRAME, &objects, &[[30000, 0, 0]], thermal);
        assert!(far[1].fired == 0.0 && far[0].fired < last[0].fired);
    }

    /// A machine that dies into its wreck keeps what it had over the
    /// wreck's heat, losing half each `cooling`; a wreck first seen as one
    /// is only its own heat.
    #[test]
    fn a_wreck_cools_by_its_half_life() {
        let defaults = ModernTuning::defaults();
        let thermal = defaults.thermal();
        let [machine, found] = ids();
        let alive = RootLook {
            heat: 0.48,
            wreck: false,
        };
        let wreck = RootLook {
            heat: 0.25,
            wreck: true,
        };
        let mut heat = ObjectHeat::default();
        heat.step(0, &[(object(machine, 0), alive)], &[], thermal);
        let died = heat.step(
            FRAME,
            &[(object(machine, 0), wreck), (object(found, 50), wreck)],
            &[],
            thermal,
        );
        assert!((died[0].warmth - 0.23).abs() < 0.001, "{died:?}");
        assert_eq!(died[1].warmth, 0.0);
        let frames = (thermal.cooling * 1.0e6 / FRAME as f32) as u64;
        let mut last = Vec::new();
        for frame in 2..=frames + 1 {
            last = heat.step(frame * FRAME, &[(object(machine, 0), wreck)], &[], thermal);
        }
        assert!(
            (last[0].warmth / died[0].warmth - 0.5).abs() < 0.01,
            "{last:?}"
        );
    }
}
