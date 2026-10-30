//! Selector 65, the water splash: constructor 0042DC60 over the visual base
//! 0047A030, final vtable 0052187C. Its table is selector 22's but for its
//! update +24 (0042DC90); the model value +14 (0046E050, value 1 is WORD
//! +44) draws its model fading as +44 falls. The vehicle creates it through
//! H13 argument 0x3B when it leaves the water fast (00417B5A).

use inf_formats::native_basis::sine_q15;
use thiserror::Error;

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, VisualObject};

/// `00531FB0`, the threshold 0042DC75 copies into +48. No code writes it.
pub const SELECTOR65_LIMIT: u32 = 0xa000;

/// The visual base plus the two DWORDs 0042DC60 writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector65State {
    pub visual: VisualObject,
    /// DWORD +44, 0xFFFF at construction. Model value 1 reads its low WORD.
    pub fade_44: u32,
    /// DWORD +48, [`SELECTOR65_LIMIT`]: the signed disposal threshold.
    pub limit_48: u32,
}

impl Selector65State {
    pub(super) fn new() -> Self {
        Self {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            fade_44: 0xffff,
            limit_48: SELECTOR65_LIMIT,
        }
    }
}

/// What the splash's ring reaches.
pub trait Selector65Host {
    type Error;
    /// `0042DDB1..0042DE0B`: H12 slot 0's cell under the X/Z WORDs
    /// (`00437AF0`'s aligned cell), its signed height shifted by `005372F8`,
    /// lies below the water plane `H12 +0 >> 005483BC`.
    fn under_water(&self, xz: [u16; 2]) -> Result<bool, Self::Error>;
    /// `00489600(0, position, zero velocity, 0)`: one splash particle.
    fn splash(&mut self, position: [u16; 3], rng: &mut u32) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector65Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("the splash's services failed: {0}")]
    Host(E),
}

impl ObjectArena {
    pub fn selector65(&self, object: ObjectId) -> Result<&Selector65State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector65(state) => Ok(state),
            _ => Err(ObjectError::NotSelector65(object)),
        }
    }

    /// 0042DC90. The fade steps as selector 22's does but with +44 clamped
    /// (signed) to 0x1000..0x4000 and shifted by 7; below +48 (signed) the
    /// splash disposes through 00479FA0, leaving +44. Otherwise a process
    /// RNG draw masked with 0x15 counts the tries; each draws again, and on
    /// an odd draw throws a splash particle from the X/Z WORDs at a random
    /// angle (a third draw, the sine table's quarter wave, 0x10000 a turn)
    /// and radius `(0x8000 - (+44 >> 1)) >> 5`, the ring widening as the
    /// fade falls, wherever that cell lies below the water.
    pub fn tick_selector65<H: Selector65Host>(
        &mut self,
        object: ObjectId,
        microseconds: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        mut next: impl FnMut(&mut u32) -> u32,
        host: &mut H,
    ) -> Result<(), Selector65Error<H::Error>> {
        let state = *self.selector65(object)?;
        let rate = (state.fade_44 as i32).clamp(0x1000, 0x4000) as u32;
        let step = (microseconds >> 12).wrapping_mul(rate) >> 7;
        let fade = state.fade_44.wrapping_sub(step);
        if (fade as i32) < state.limit_48 as i32 {
            return Ok(self.dispose(object)?);
        }
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector65(state) => state.fade_44 = fade,
            _ => return Err(ObjectError::NotSelector65(object).into()),
        }
        let tries = next(rng) & 0x15;
        let radius = 0x8000_i32.wrapping_sub((fade as i32) >> 1) >> 5;
        for _ in 0..tries {
            if next(rng) & 1 == 0 {
                continue;
            }
            let angle = (next(rng) & 0xffff) as i32;
            let [x, y, z] = self
                .object(object)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(object))?;
            let across = sine_q15(angle + 0x4000, sine_quarter).wrapping_mul(radius) >> 15;
            let along = sine_q15(angle, sine_quarter).wrapping_mul(radius) >> 15;
            let x = x.wrapping_add(across as u16);
            let z = z.wrapping_add(along as u16);
            if host.under_water([x, z]).map_err(Selector65Error::Host)? {
                host.splash([x, y, z], rng).map_err(Selector65Error::Host)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
