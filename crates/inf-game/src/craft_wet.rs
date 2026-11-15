//! Port option, Vulkan's remaster ([the water's memory](../../../docs/rt-plans/water-memory.md)):
//! the player's craft wet where the water reached it. While the water
//! stands over its hull's bottom it is wet up to a little over the
//! surface (the ripples and the splashes reach higher), the highest line
//! kept while it stays wet; out of the water it dries over
//! [`DRY_SECONDS`]. The shading (`pt_wet.glsl`'s `craft_wetness`, the
//! tracer's and the raster's metal) wets what lies under that line round
//! the craft as the rain wets the models. Only a picture: nothing here
//! reaches the game.

/// How long the craft takes to dry to 1/e of its wet.
const DRY_SECONDS: f32 = 20.0;
/// How far under the craft's middle the water may stand and still reach
/// its hull.
const REACH_BELOW: f32 = 64.0;
/// How far over the water's surface the wet reaches, its ripples and
/// splashes.
const SPLASHED: f32 = 16.0;
/// Below this the craft is dry again.
const DRY: f32 = 0.02;

/// The craft's wet line (its height over the craft's middle) and how wet
/// it is, kept from frame to frame with the world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftWet {
    clock: Option<u64>,
    mark: f32,
    wet: f32,
}

impl CraftWet {
    /// The craft at `now` (the game's clock, microseconds), its middle at
    /// world height `middle` (none: no craft), the water's surface under it
    /// at `water` (none: no water there).
    pub fn update(&mut self, now: u64, middle: Option<f32>, water: Option<f32>) {
        let seconds = match self.clock {
            // A clock that ran backwards (another visit) starts it dry.
            Some(then) if then > now => {
                *self = Self::default();
                0.0
            }
            Some(then) => (now - then) as f32 / 1_000_000.0,
            None => 0.0,
        };
        self.clock = Some(now);
        match (middle, water) {
            (Some(middle), Some(water)) if water - middle >= -REACH_BELOW => {
                let line = water - middle + SPLASHED;
                self.mark = if self.wet > 0.0 {
                    self.mark.max(line)
                } else {
                    line
                };
                self.wet = 1.0;
            }
            _ => {
                self.wet *= (-seconds / DRY_SECONDS).exp();
                if self.wet < DRY {
                    self.wet = 0.0;
                }
            }
        }
    }

    /// The wet line over the craft's middle and how wet it is (0: dry).
    pub fn look(&self) -> [f32; 2] {
        [self.mark, self.wet]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wading wets it to over the surface, the highest line kept; out of
    /// the water it dries over seconds; a dry craft or one far over the
    /// water is dry.
    #[test]
    fn the_craft_wets_to_its_highest_water_and_dries() {
        let mut wet = CraftWet::default();
        wet.update(0, Some(100.0), Some(50.0));
        assert_eq!(wet.look(), [-50.0 + SPLASHED, 1.0]);
        wet.update(16_000, Some(100.0), Some(80.0));
        wet.update(32_000, Some(100.0), Some(60.0));
        assert_eq!(wet.look(), [-20.0 + SPLASHED, 1.0]);
        wet.update(20_032_000, Some(300.0), None);
        let [line, left] = wet.look();
        assert_eq!(line, -20.0 + SPLASHED);
        assert!((left - (-1.0f32).exp()).abs() < 1.0e-4, "{left}");
        wet.update(120_032_000, Some(300.0), None);
        assert_eq!(wet.look()[1], 0.0);
        // High over the water: dry.
        let mut high = CraftWet::default();
        high.update(0, Some(400.0), Some(100.0));
        assert_eq!(high.look()[1], 0.0);
        // The clock ran back: dry again.
        wet.update(0, Some(100.0), Some(80.0));
        wet.update(10, None, None);
        assert!(wet.look()[1] > 0.99);
        wet.update(5, None, None);
        assert_eq!(wet.look()[1], 0.0);
    }
}
