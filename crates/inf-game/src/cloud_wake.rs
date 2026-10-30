//! The craft's wake through the modern sky's clouds (Vulkan only): where
//! the player's craft the camera follows has flown, a point every quarter
//! of a second for three seconds, along which the clouds thin
//! (`pt_sky.glsl`'s `cloud_wake`), a tube that widens and closes again as
//! the cloud heals, as an aircraft's exhaust and wake clear a path through
//! real cloud. Only a look: the world keeps it beside the game, which never
//! reads it.

use std::collections::VecDeque;

/// The most points the frame block carries: the craft now and those laid
/// behind it (`pt_frame.glsl`'s `cloud_wake`).
pub const WAKE_POINTS: usize = 12;
/// How often a point is laid, in the game clock's microseconds.
const EVERY_MICROS: u64 = 250_000;
/// How long the cloud takes to heal behind the craft, in seconds.
pub const LIFE_SECONDS: f32 = 3.0;

/// The points laid: each where the craft was (16-bit world words) and when.
#[derive(Clone, Debug, Default)]
pub struct CloudWake {
    points: VecDeque<([u16; 3], u64)>,
}

impl CloudWake {
    /// Follow the craft at `craft` (none: no craft followed) at the game
    /// clock's `micros`: a point once every [`EVERY_MICROS`], the oldest
    /// dropped past their life. A clock that went back (a new world, a
    /// load) starts the wake afresh.
    pub fn follow(&mut self, craft: Option<[u16; 3]>, micros: u64) {
        if self.points.back().is_some_and(|&(_, laid)| laid > micros) {
            self.points.clear();
        }
        let life = (LIFE_SECONDS * 1.0e6) as u64;
        while self
            .points
            .front()
            .is_some_and(|&(_, laid)| micros - laid > life)
        {
            self.points.pop_front();
        }
        let due = self
            .points
            .back()
            .is_none_or(|&(_, laid)| micros - laid >= EVERY_MICROS);
        if let (Some(craft), true) = (craft, due) {
            self.points.push_back((craft, micros));
            if self.points.len() > WAKE_POINTS - 1 {
                self.points.pop_front();
            }
        }
    }

    /// The wake from `eye` at `micros`, newest first: the craft now
    /// (`craft`, eye-relative, age 0) where one is followed, then each point
    /// laid, eye-relative across the world's wrap, with its age in seconds.
    pub fn from_eye(&self, eye: [i32; 3], craft: Option<[f32; 3]>, micros: u64) -> Vec<[f32; 4]> {
        let laid = self.points.iter().rev().map(|&(words, at)| {
            let [x, y, z] = std::array::from_fn(|axis| {
                f32::from(i32::from(words[axis]).wrapping_sub(eye[axis]) as i16)
            });
            [x, y, z, micros.saturating_sub(at) as f32 / 1.0e6]
        });
        craft
            .map(|[x, y, z]| [x, y, z, 0.0])
            .into_iter()
            .chain(laid)
            .take(WAKE_POINTS)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wake_lays_a_point_each_quarter_second_and_heals_after_three() {
        let mut wake = CloudWake::default();
        for frame in 0..200u64 {
            // A craft flying along X at 16 ms frames.
            wake.follow(Some([(frame * 10) as u16, 3000, 0]), frame * 16_000);
        }
        let micros = 199 * 16_000;
        let points = wake.from_eye([0, 3000, 0], Some([1990.0, 0.0, 0.0]), micros);
        // The craft now, then the points of the last three seconds, newest
        // first, each a quarter of a second older than the next.
        assert_eq!(points[0], [1990.0, 0.0, 0.0, 0.0]);
        assert_eq!(points.len(), WAKE_POINTS);
        assert!(points.windows(2).all(|pair| pair[1][3] > pair[0][3]));
        assert!(points.iter().all(|point| point[3] <= LIFE_SECONDS));
        // Long after the craft is no longer followed, the wake is gone.
        wake.follow(None, micros + 4_000_000);
        assert!(wake
            .from_eye([0, 3000, 0], None, micros + 4_000_000)
            .is_empty());
    }

    #[test]
    fn the_wake_meets_itself_across_the_wrap_and_restarts_with_the_clock() {
        let mut wake = CloudWake::default();
        wake.follow(Some([65_500, 0, 10]), 1_000_000);
        // From an eye just across the wrap, the point lies 46 units behind.
        let points = wake.from_eye([10, 0, 10], None, 1_000_000);
        assert_eq!(points, vec![[-46.0, 0.0, 0.0, 0.0]]);
        // A clock that went back (a new world) starts afresh.
        wake.follow(Some([5, 0, 5]), 0);
        assert_eq!(
            wake.from_eye([0, 0, 0], None, 0),
            vec![[5.0, 0.0, 5.0, 0.0]]
        );
    }
}
