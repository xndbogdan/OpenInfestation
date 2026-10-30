//! Complete shared angular response 0047D440. Input axes belong to the caller;
//! this leaf owns dot thresholds, sign-dependent gain and local-Z recovery.

use super::derived::{transform_local_dwords, transform_world_dwords};
use super::native_trunc_low32;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorAngularResponseRequest {
    /// Original first float vector, which need not alias actor basis-Y.
    pub current_axis: [f32; 3],
    /// Original second float vector; no normalization or default is implied.
    pub desired_axis: [f32; 3],
    /// Full signed DWORD FILD input. Callers own elapsed/power conversions.
    pub gain: i32,
}

impl ObjectArena {
    pub fn apply_actor_angular_response(
        &mut self,
        actor: ObjectId,
        request: ActorAngularResponseRequest,
    ) -> Result<(), ObjectError> {
        let [cx, cy, cz] = request.current_axis.map(f64::from);
        let [dx, dy, dz] = request.desired_axis.map(f64::from);
        // 0043B2B0 accumulates Z/Y then X and spills before both comparisons.
        let dot = ((dz * cz + dy * cy) + dx * cx) as f32;
        // 00527CD0/CCC are source constants, not a caller-selected tolerance.
        const UPPER: f32 = f32::from_bits(0x3f7f_f000);
        const LOWER: f32 = f32::from_bits(0xbefa_0000);
        if dot >= UPPER {
            // 0047D46A returns before reading any actor angular/basis storage.
            return Ok(());
        }
        if dot >= LOWER {
            // Current cross desired: X/Z stay retained; Y is the sole f32
            // spill at0047D50D. A generic f32 cross would change this phase.
            let cross = [
                cy * dz - dy * cz,
                f64::from((dx * cz - cx * dz) as f32),
                dy * cx - dx * cy,
            ];
            let mut increment = [0; 3];
            for axis in 0..3 {
                let angular = self.actor(actor)?.words_94_9c[axis] as i32;
                // TEST AH,41h treats zero/unordered as nonpositive. Every
                // axis reloads the original gain, not the earlier reduced one.
                let reduce = if cross[axis] > 0.0 {
                    angular >= 0
                } else {
                    angular <= 0
                };
                let gain = if reduce {
                    request.gain / 3
                } else {
                    request.gain
                };
                increment[axis] = native_trunc_low32(cross[axis] * f64::from(gain)) as u32;
            }
            // All CRT conversions precede 0047B470's ordered world additions.
            self.add_actor_angular_velocity(actor, increment)?;
        } else {
            // The original C0 comparisons route an unordered dot here too.
            // 0047A680 projects full signed DWORDs, not narrowed WORDs.
            let state = self.actor(actor)?;
            let basis = state.rotating.basis;
            let local = transform_world_dwords(basis, state.words_94_9c.map(|word| word as i32));
            if local[2] > -0x4800 && local[2] < 0x4800 {
                // Recovery ignores gain, including gain0; zero selects -2000h.
                let response = if local[2] > 0 { 0x2000 } else { -0x2000 };
                let world = transform_local_dwords(basis, [0, 0, response]);
                self.add_actor_angular_velocity(actor, world.map(|word| word as u32))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
