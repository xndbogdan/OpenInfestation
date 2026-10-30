//! Selector 22: constructor 0042DBE0 over the visual base 0047A030, final
//! vtable 00521830. It differs from the visual base 00527B68 only in update
//! +24 (0042DC00) and model value +14 (0046E050, whose value 1 is WORD +44).
//! The dying Selector36 creates it through H13 argument 0x3A.

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, VisualObject};

/// The visual base plus the two DWORDs 0042DBE0 writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector22State {
    pub visual: VisualObject,
    /// DWORD +44, 0xFFFF at construction. Model value 1 reads its low WORD.
    pub fade_44: u32,
    /// DWORD +48, 0 at construction: the signed disposal threshold.
    pub limit_48: u32,
}

impl Selector22State {
    pub(super) fn new() -> Self {
        Self {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            fade_44: 0xffff,
            limit_48: 0,
        }
    }
}

impl ObjectArena {
    pub fn selector22(&self, object: ObjectId) -> Result<&Selector22State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector22(state) => Ok(state),
            _ => Err(ObjectError::NotSelector22(object)),
        }
    }

    /// 0042DC00. The step scales the unsigned delta's high bits by +44
    /// clamped (signed) to 0x1000..0xD000, with a wrapped product and logical
    /// shifts. A result below +48 (signed) disposes through 00479FA0 and
    /// leaves +44 unchanged.
    pub fn tick_selector22(
        &mut self,
        object: ObjectId,
        microseconds: u32,
    ) -> Result<(), ObjectError> {
        let state = *self.selector22(object)?;
        let rate = (state.fade_44 as i32).clamp(0x1000, 0xd000) as u32;
        let step = (microseconds >> 12).wrapping_mul(rate) >> 6;
        let fade = state.fade_44.wrapping_sub(step);
        if (fade as i32) < state.limit_48 as i32 {
            return self.dispose(object);
        }
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector22(state) => state.fade_44 = fade,
            _ => return Err(ObjectError::NotSelector22(object)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
