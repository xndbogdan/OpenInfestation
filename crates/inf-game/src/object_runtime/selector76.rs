//! Selector 76, 00431ED0 over the 0047A490 rotating visual (final
//! 00521A40, 0x84 bytes): +70 = 1 and +74 = 0. It is the nonactor the
//! behavior registry names for kind 10 (H13 130); 0048D820 builds one,
//! finds no actor at +18 (00401300) and disposes it. Construction runs
//! 00479DB0 (+08 = 0047A610, then payload flags) and the RET +1C 00401310.
//!
//! It is the ACAM's flamethrower (weapon row 3). 00431EF0 builds one for
//! the emitter's resource 0x82, keeps its thrower's handle at +6C and runs
//! its update once with no time. The update (+24, 00431FE0) follows the
//! thrower and grows the flame by a quarter of the microseconds in +70, up
//! to 0x10000 (600 units, +74): flames where it touches the ground, and a
//! burn every 60 units along it. At full length it disposes itself. Its
//! light (+28, 00432490) glows red with +70, and its draw (+2C, 004323B0)
//! shows its model at the thrower and at the flame's root.

use super::actor::native_trunc_low32 as ftol;
use super::{ActorHandle, ObjectArena, ObjectError, ObjectId, RotatingVisual};
use inf_formats::native_basis::{NativeBasisMath, RotationPlane};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector76State {
    pub rotating: RotatingVisual,
    /// +6C, the thrower's handle 00431EF0 stores after construction.
    pub owner_6c: Option<ActorHandle>,
    pub dword_70: u32,
    pub dword_74: u32,
    /// +78..+7D, the thrower's position the update copies.
    pub owner_position_78: Option<[u16; 3]>,
    /// +7E..+83, the flame's root: the thrower's position plus its
    /// forward row times its model radius (WORD +40).
    pub root_7e: Option<[u16; 3]>,
}

impl Selector76State {
    pub(super) fn new() -> Self {
        Self {
            rotating: RotatingVisual::new(),
            owner_6c: None,
            dword_70: 1,
            dword_74: 0,
            owner_position_78: None,
            root_7e: None,
        }
    }
}

/// 00432103..00432114: the growth's cap, and its length scale
/// (0043211E..0043212A, `+70 * 600 >> 16`).
const FULL: u32 = 0x1_0000;
const LENGTH_SCALE: i32 = 600;
/// 00432134/0043227C: the step between burns, and the least growth that
/// makes flames.
const STEP: i32 = 60;
/// 00431FFF: a flame rises at 30.
const FLAME_VELOCITY: [u16; 3] = [0, 30, 0];
/// 004321D9: a flame's point sits 80 under the flame.
const FLAME_DROP: u16 = 0x50;

/// The flamethrower's services.
pub trait Selector76Host {
    type Error;
    /// One draw from 0048F060.
    fn random(&mut self) -> u32;
    /// H12/0's height (00450A30) at X and Z.
    fn terrain_height(&mut self, arena: &mut ObjectArena, xz: [u16; 2])
        -> Result<i16, Self::Error>;
    /// 00489D90(9, point, (0, 30, 0), 0x800), a flame.
    fn flame(
        &mut self,
        arena: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// 004887D0(block, point, thrower, 0): the block at 00532768, or
    /// 00532778 online, with the inner (+4) and outer (+6) radii written.
    fn burn(
        &mut self,
        arena: &mut ObjectArena,
        point: [u16; 3],
        radii: [u16; 2],
        thrower: ActorHandle,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Selector76Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("flamethrower service failed")]
    Host(E),
}

/// The light 00432490 splats at the flame's X and Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector76Light {
    pub position_xz: [i32; 2],
    pub radius: i32,
    pub rgb: [u8; 3],
}

/// `x / 600` as 0043217F..00432199 and 00432299..004322AC divide it: the
/// signed quotient, rounded toward zero.
fn per_600(value: i32) -> i32 {
    value / 600
}

impl ObjectArena {
    pub fn selector76(&self, object: ObjectId) -> Result<&Selector76State, ObjectError> {
        match &self.object(object)?.class {
            super::ObjectClass::Selector76(state) => Ok(state),
            _ => Err(ObjectError::NotRotatingVisual(object)),
        }
    }

    pub(in crate::object_runtime) fn selector76_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector76State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            super::ObjectClass::Selector76(state) => Ok(state),
            _ => Err(ObjectError::NotRotatingVisual(object)),
        }
    }

    /// 00431F9A..00431FAC: the thrower's handle at +6C.
    pub fn arm_selector76(
        &mut self,
        object: ObjectId,
        thrower: ActorHandle,
    ) -> Result<(), ObjectError> {
        self.selector76_mut(object)?.owner_6c = Some(thrower);
        Ok(())
    }

    /// 00431FE0 up to its time check: the thrower's position and basis,
    /// and the flame's root along its forward row. A lost thrower disposes
    /// the flamethrower (0043200B) and answers None.
    pub fn selector76_follow(
        &mut self,
        object: ObjectId,
    ) -> Result<Option<ActorHandle>, ObjectError> {
        let handle = self
            .selector76(object)?
            .owner_6c
            .ok_or(ObjectError::UnarmedFlamethrower(object))?;
        let thrower = self
            .resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        let Some(thrower) = thrower else {
            self.dispose(object)?;
            return Ok(None);
        };
        // 00432011..0043206D: the thrower's position and basis.
        let position = self
            .object(thrower)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(thrower))?;
        let basis = self.actor(thrower)?.rotating.basis;
        // +40, the model radius (header WORD +0E).
        let reach = self.actor(thrower)?.rotating.visual.radius_word as i16;
        {
            let state = self.selector76_mut(object)?;
            state.owner_position_78 = Some(position);
            state.rotating.basis = basis;
        }
        self.owned_mut(object)?.state.base.position_words = Some(position);
        // 00432072..004320ED: the root along the forward row.
        let along = self.selector76_forward(object)?;
        let root: [u16; 3] = std::array::from_fn(|axis| {
            (ftol(f64::from(reach) * along[axis]) as u16).wrapping_add(position[axis])
        });
        self.selector76_mut(object)?.root_7e = Some(root);
        Ok(Some(handle))
    }

    /// Its own basis's forward row (its +48 virtual's +1C), as doubles.
    fn selector76_forward(&self, object: ObjectId) -> Result<[f64; 3], ObjectError> {
        let basis = self.selector76(object)?.rotating.basis;
        Ok(std::array::from_fn(|axis| {
            f64::from(f32::from_bits(basis.words[7 + axis]))
        }))
    }

    /// 00431FE0, the flamethrower's +24.
    pub fn tick_selector76<H: Selector76Host>(
        &mut self,
        object: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector76Error<H::Error>> {
        let previous = self.selector76(object)?.dword_74 as i32;
        let Some(handle) = self.selector76_follow(object)? else {
            return Ok(());
        };
        if delta == 0 {
            return Ok(());
        }
        // 00432100..0043212D: the growth and its length.
        let grown = {
            let state = self.selector76_mut(object)?;
            let grown = state.dword_70.wrapping_add(delta >> 2);
            state.dword_70 = if (grown as i32) >= FULL as i32 {
                FULL
            } else {
                grown
            };
            state.dword_70 as i32
        };
        let length = grown.wrapping_mul(LENGTH_SCALE) >> 16;
        self.selector76_mut(object)?.dword_74 = length as u32;
        let mut reached = previous;
        if length.wrapping_sub(previous) >= STEP {
            // 0043213D..0043224E: flames where the new length meets the
            // ground, the count rising with the growth.
            let growth = length.wrapping_sub(previous);
            let count = grown.wrapping_mul(8) >> 16;
            for _ in 0..count.max(0) {
                let depth = (((host.random() & 0xffff) as i32).wrapping_mul(growth) >> 16)
                    .wrapping_add(previous);
                let spread = per_600(depth.wrapping_mul(320));
                let side = (((host.random() & 0xffff) as i32).wrapping_mul(spread) >> 17) as u16;
                let across = side.wrapping_neg();
                let local = [
                    if depth & 1 != 0 { across } else { side },
                    across,
                    depth as u16,
                ];
                let offset = self.transform_visual_vector_words(object, local)?;
                let [x, y, z] = self
                    .object(object)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(object))?;
                let mut point = [
                    offset[0].wrapping_add(x),
                    offset[1].wrapping_sub(FLAME_DROP).wrapping_add(y),
                    offset[2].wrapping_add(z),
                ];
                let height = host
                    .terrain_height(self, [point[0], point[2]])
                    .map_err(Selector76Error::Host)?;
                if (point[1] as i16) < height {
                    point[1] = height as u16;
                    host.flame(self, point, FLAME_VELOCITY)
                        .map_err(Selector76Error::Host)?;
                }
            }
            // 00432254..00432381: a burn and a flame every 60 along it,
            // wider the farther out.
            let mut scaled = previous.wrapping_mul(320);
            while self.selector76(object)?.dword_74 as i32 - reached >= STEP {
                scaled = scaled.wrapping_add(STEP * 320);
                let half = per_600(scaled) / 2;
                reached = reached.wrapping_add(STEP);
                let radius = if half > STEP { half } else { STEP };
                let radii = [(radius * 5 / 4) as u16, (radius * 6 / 4) as u16];
                let along = self.selector76_forward(object)?;
                let root = self.selector76(object)?.root_7e.expect("written above");
                let distance = f64::from(reached as i16);
                let point: [u16; 3] = std::array::from_fn(|axis| {
                    (ftol(distance * along[axis]) as u16).wrapping_add(root[axis])
                });
                host.burn(self, point, radii, handle)
                    .map_err(Selector76Error::Host)?;
                host.flame(self, point, FLAME_VELOCITY)
                    .map_err(Selector76Error::Host)?;
            }
        }
        // 00432387..0043239D: the length reached, and at full growth the end.
        let state = self.selector76_mut(object)?;
        state.dword_74 = reached as u32;
        if state.dword_70 == FULL {
            state.dword_70 = 0xffff;
            self.dispose(object)?;
        }
        Ok(())
    }

    /// 00432490, the flamethrower's +28: red `+70 * 127 >> 16`, green and
    /// blue `+70 * 80 >> 16`, radius `(+70 >> 6) + 256`, at its X and Z.
    pub fn selector76_light(&self, object: ObjectId) -> Result<Selector76Light, ObjectError> {
        let growth = self.selector76(object)?.dword_70 as i32;
        let [x, _, z] = self
            .object(object)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(object))?;
        let red = (growth.wrapping_mul(127) >> 16) as u8;
        let rest = (growth.wrapping_mul(80) >> 16) as u8;
        Ok(Selector76Light {
            position_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius: (growth >> 6).wrapping_add(0x100),
            rgb: [red, rest, rest],
        })
    }

    /// 004323B0's preamble: the basis turned a quarter in its Y-Z plane
    /// (0047A720(0x4000)) for both draws; the saved basis comes back after.
    pub fn selector76_turn_for_draw(
        &mut self,
        object: ObjectId,
        math: &NativeBasisMath,
    ) -> Result<inf_formats::native_basis::NativeBasis, ObjectError> {
        let saved = self.selector76(object)?.rotating.basis;
        self.rotate_visual(object, RotationPlane::Yz, 0x4000, math)?;
        Ok(saved)
    }

    /// 0043241A..00432482 between and after the draws: the root for the
    /// second draw, then the thrower's position and the saved basis.
    pub fn selector76_draw_at_root(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        let root = self.selector76(object)?.root_7e;
        if let Some(root) = root {
            self.owned_mut(object)?.state.base.position_words = Some(root);
        }
        Ok(())
    }

    pub fn selector76_draw_restore(
        &mut self,
        object: ObjectId,
        saved: inf_formats::native_basis::NativeBasis,
    ) -> Result<(), ObjectError> {
        let back = self.selector76(object)?.owner_position_78;
        if let Some(back) = back {
            self.owned_mut(object)?.state.base.position_words = Some(back);
        }
        self.selector76_mut(object)?.rotating.basis = saved;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector76_tests.rs"]
mod tests;
