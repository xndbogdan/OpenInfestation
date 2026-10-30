//! World visibility: 004875A0 timing feedback, 0048E2E0 manager distance,
//! and 00488380 retained predraw slew. All distances are signed native DWORDs;
//! they are not terrain heights or the frontend's effectively infinite fade.

/// 0048D9F0 initializes manager W+1C to this distance. W+20 starts at 0x400
/// before 0048DC30 replaces it with min(H14+8, W+1C*3/4).
pub const INITIAL_WORLD_FAR: u32 = 0x2800;

use crate::process_particles::timing::WorldTimingRing;

/// Fields that 004874D0/ 0048DC30 do not reset on the retained world manager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldVisibilityReload {
    pub manager_far: u32,
    pub performance: WorldTimingRing,
}

/// G+5C8/+5CC plus the timing feedback that advances manager W+1C.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldFadeState {
    smoothed_far: i32,
    smoothed_header: i32,
    /// Port option: the same pair slewed for the view shown where it
    /// differs from the game's (the tracer's longer draw distance), apart
    /// from the game's.
    shown: [i32; 2],
    pub performance: WorldTimingRing,
}

impl WorldFadeState {
    /// Read-only observation of G+5C8/+5CC after the last actual predraw.
    /// Capture must not call predraw again merely to obtain these values.
    pub(crate) fn retained_predraw_words(&self) -> [i32; 2] {
        [self.smoothed_far, self.smoothed_header]
    }

    /// 004874D0 calls 0048E7D0 after loading: copy the current manager+1C/+20
    /// into G+5C8/+5CC. Header clamping has already run, including on reload.
    pub fn loaded(far: u32, header: u32, performance: WorldTimingRing) -> Self {
        Self {
            smoothed_far: far as i32,
            smoothed_header: header as i32,
            shown: [far as i32, header as i32],
            performance,
        }
    }

    /// 00488380 adjusts H by the same G+5C0 view/zoom scale as 00442080,
    /// clamps it below F-800, then slews each retained value by 64 only when
    /// the target lies outside its inclusive +/-64 deadband. It never snaps
    /// to a target within that band. Publish near=(storedF-storedH)/4, far=F.
    pub fn predraw(&mut self, far: u32, header: u32, scale_5c0: i16) -> [i32; 2] {
        let mut pair = [self.smoothed_far, self.smoothed_header];
        let fade = predraw_pair(&mut pair, far, header, scale_5c0);
        [self.smoothed_far, self.smoothed_header] = pair;
        fade
    }

    /// Port option: [`Self::predraw`] for the view shown, on its own pair.
    pub fn shown_predraw(&mut self, far: u32, header: u32, scale_5c0: i16) -> [i32; 2] {
        predraw_pair(&mut self.shown, far, header, scale_5c0)
    }
}

fn predraw_pair(pair: &mut [i32; 2], far: u32, header: u32, scale_5c0: i16) -> [i32; 2] {
    let far = far as i32;
    let adjustment = ((i32::from(scale_5c0).wrapping_mul(4000) >> 15) as i16) as i32;
    let header = (header as i32)
        .wrapping_add(adjustment)
        .min(far.wrapping_sub(800));
    pair[0] = slew(pair[0], far);
    pair[1] = slew(pair[1], header);
    [pair[0].wrapping_sub(pair[1]) / 4, pair[0]]
}

/// Signed arithmetic and low-WORD feedback conversion at 0048E6BF..738.
pub fn update_manager_far(far: u32, header: u32, budget: i32) -> u32 {
    let far = far as i32;
    let next = if budget < 0xd000 {
        let decrease =
            (((0xd000_i32.wrapping_sub(budget) / 2).wrapping_shl(8) >> 15) as i16) as i32;
        far.wrapping_sub(decrease)
            .max(0x1400)
            .max((header as i32).wrapping_add(0x100))
    } else {
        let increase = ((budget.wrapping_sub(0xd000).wrapping_shl(8) >> 15) as i16) as i32;
        far.wrapping_add(increase).min(0x3100)
    };
    next as u32
}

fn slew(current: i32, target: i32) -> i32 {
    if target > current.wrapping_add(64) {
        current.wrapping_add(64)
    } else if target < current.wrapping_sub(64) {
        current.wrapping_sub(64)
    } else {
        current
    }
}

#[cfg(test)]
#[path = "world_fade_tests.rs"]
mod tests;
