//! 00408560, the eyes' death: `+AC` of selector 77's table 0051FBB0 and
//! selector 79's 0051FC8C. A line of bursts runs from the eye down its stalk
//! to the root `+192`, then at the stalk's tip (`+D8`, 00408310) a sound
//! plays and the eye leaves a child (`+4C`, 00407E70) turned as the stalk
//! is, and it is disposed.

use inf_formats::native_basis::{isqrt_48efc0, NativeBasis, NativeBasisMath, RotationPlane};
use thiserror::Error;

use crate::object_runtime::{mark_flag_change, ObjectArena, ObjectError, ObjectId};

use super::super::DerivedActorError;

/// 00408647: the bursts' kinds, by the burst's index modulo four.
const BURST_KINDS: [u32; 4] = [2, 3, 6, 9];
/// 00408570..0040857C: the bursts' velocity, 50 up.
const BURST_VELOCITY: [u16; 3] = [0, 0x32, 0];
/// 004085EE..00408612: a burst every 50 units of the stalk, 5 to 100.
const BURST_SPACING: i32 = 50;
const BURSTS: std::ops::RangeInclusive<i32> = 5..=100;
/// 004086F7: the child the eye leaves at the tip.
pub const EYE_DEATH_CHILD: u32 = 0x89;
/// 00408718: the child's flag.
const CHILD_FLAG: u32 = 0x40_0000;
/// 0040841E: the stalk's model, whose header WORD +0E is its length.
pub const EYE_STALK_MODEL: u16 = 0x2be;

/// What the eyes' death reaches beyond the arena.
pub trait EyeDeathHost {
    type Error;
    /// 00489D90(kind, position, velocity, 0), a burst.
    fn burst(
        &mut self,
        kind: u32,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// The resident stalk model's header WORD +0E (00408428..0040845C);
    /// None when the model is not resident, which leaves the tip at the
    /// root.
    fn stalk_length(&mut self) -> Result<Option<u16>, Self::Error>;
    /// 00487F10(0x28, position, 0x10000, 0x10000), then 004923C0.
    fn sound(&mut self, position: [u16; 3]) -> Result<(), Self::Error>;
    /// 0047AAF0(&out, 0x89, position) from the eye: the child it builds.
    fn child(
        &mut self,
        arena: &mut ObjectArena,
        eye: ObjectId,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, Self::Error>;
    /// The sine table 00491530 and 00491290 turn with.
    fn basis_math(&self) -> &NativeBasisMath;
}

#[derive(Debug, Error)]
pub enum EyeDeathError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    /// 0040858B: the stalk's root, which the eye's +1C writes.
    #[error("eye {0:?} dies before its +1C wrote the stalk's root +192")]
    NoRoot(ObjectId),
    #[error("eye {eye:?} death service at {address:08X}: {error}")]
    Host {
        eye: ObjectId,
        address: u32,
        error: E,
    },
}

impl ObjectArena {
    /// 00408560, the eyes' +AC.
    pub fn retire_selector77<H: EyeDeathHost>(
        &mut self,
        eye: ObjectId,
        host: &mut H,
    ) -> Result<(), EyeDeathError<H::Error>> {
        let fail = |address| {
            move |error| EyeDeathError::Host {
                eye,
                address,
                error,
            }
        };
        let position = self
            .object(eye)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(eye))?;
        let root = self
            .selector77_actor(eye)?
            .position_192
            .ok_or(EyeDeathError::NoRoot(eye))?;
        // 0040858B..004085CF: the stalk, its squared length a DWORD.
        let stalk: [i16; 3] =
            std::array::from_fn(|axis| root[axis].wrapping_sub(position[axis]) as i16);
        let squared = stalk.iter().fold(0_i32, |sum, &axis| {
            sum.wrapping_add(i32::from(axis) * i32::from(axis))
        });
        let length = if squared < 0 {
            (isqrt_48efc0(((squared as u32) >> 2) as i32) << 1) as i32
        } else {
            isqrt_48efc0(squared) as i32
        };
        let count = (length / BURST_SPACING).clamp(*BURSTS.start(), *BURSTS.end());
        let mut walked = [0_i32; 3];
        for index in 0..=count {
            let point: [u16; 3] = std::array::from_fn(|axis| {
                ((walked[axis] / count) as u16).wrapping_add(position[axis])
            });
            host.burst(BURST_KINDS[index as usize & 3], point, BURST_VELOCITY)
                .map_err(fail(0x0040_868c))?;
            for axis in 0..3 {
                walked[axis] = walked[axis].wrapping_add(i32::from(stalk[axis]));
            }
        }
        // 004086C7, +D8 (00408310): the tip.
        let turned = self.selector77_turned_basis(eye, host.basis_math())?;
        let length = host.stalk_length().map_err(fail(0x0040_8450))?;
        let tip = self.selector77_tip(eye, root, length, host.basis_math())?;
        host.sound(tip).map_err(fail(0x0040_86e0))?;
        // 004086FF, +4C (00407E70): the child takes the turned basis.
        if let Some(child) = host.child(self, eye, tip).map_err(fail(0x0040_86ff))? {
            self.actor_mut(child)?.rotating.basis = turned;
            let base = &mut self.owned_mut(child)?.state.base;
            let flags = base.flags | CHILD_FLAG;
            mark_flag_change(base, flags);
        }
        Ok(self.dispose(eye)?)
    }

    /// 00408310, the eyes' +D8: the stalk's tip, `length` (the stalk
    /// model's header WORD +0E, None when it is not resident) up the turned
    /// stalk from the root, through 00490B20 (00490900 on WORDs, its WORD
    /// results); the root itself without the model.
    pub fn selector77_tip(
        &self,
        eye: ObjectId,
        root: [u16; 3],
        length: Option<u16>,
        math: &NativeBasisMath,
    ) -> Result<[u16; 3], DerivedActorError> {
        let turned = self.selector77_turned_basis(eye, math)?;
        Ok(match length {
            Some(length) => {
                let lift = turned.transform_point([0, 0, i32::from(length as i16)]);
                std::array::from_fn(|axis| root[axis].wrapping_sub(lift[axis] as u16))
            }
            None => root,
        })
    }

    /// 00408465..004084E9 and 00407FA1..0040802C: a copy of the eye's
    /// basis (+48) turned by its +18A, +188, +18E and +186 (00491530 and
    /// 00491290 alternately).
    fn selector77_turned_basis(
        &self,
        eye: ObjectId,
        math: &NativeBasisMath,
    ) -> Result<NativeBasis, DerivedActorError> {
        let angles = self.selector77_actor(eye)?.words_184_190;
        let mut basis = self.actor(eye)?.rotating.basis;
        for (index, plane) in [
            (3, RotationPlane::Xz),
            (2, RotationPlane::Yz),
            (5, RotationPlane::Xz),
            (1, RotationPlane::Yz),
        ] {
            math.rotate_in_plane(&mut basis, i32::from(angles[index] as i16), plane);
        }
        Ok(basis)
    }
}

#[cfg(test)]
#[path = "death_tests.rs"]
mod tests;
