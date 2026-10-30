//! Rotating visual prefix 0047A490/0047A610 and its three angle wrappers.
//!
//! This is the intermediate 00527BB4 class, not a derived actor constructor.
//! The original startup 00439630 (the CRT table's jump 00439620)
//! initializes the shared basis 00547590 to identity; 0047A490 copies all ten words, including its zero counter.

use inf_formats::native_basis::{NativeBasis, NativeBasisMath, RotationPlane};

use super::{BaseContext, ObjectArena, ObjectClass, ObjectError, ObjectId, VisualObject};
use crate::ResidentRuntime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotatingVisual {
    pub visual: VisualObject,
    /// Native +44 counter and +48..+68 float vectors, retained as exact bits.
    pub basis: NativeBasis,
}

impl RotatingVisual {
    pub(super) fn new() -> Self {
        Self {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            basis: NativeBasis::INITIAL,
        }
    }
}

/// Fields read by 0047A610, including the visual initializer it calls first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotatingVisualContext {
    pub base: BaseContext,
    /// Optional pointer at native context +8; only the low WORD is read.
    pub model_argument: Option<u32>,
    /// Optional pointer at context +4. Its signed WORD angles are consumed
    /// in order [1], [0], [2], with 65536 units per turn. None preserves the
    /// existing basis; Some([0; 3]) still performs three counter increments.
    pub rotation_words: Option<[u16; 3]>,
}

impl ObjectArena {
    /// 0047A610: visual context first, then accumulating Xz, Yz, Xy rotations.
    /// The context never resets the basis and the angle wrappers do not touch
    /// common dirty bits. The original sine table is supplied by the caller.
    pub fn initialize_rotating_visual_context(
        &mut self,
        object: ObjectId,
        context: RotatingVisualContext,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), ObjectError> {
        self.rotating_visual(object)?;
        self.initialize_visual_context(object, context.base, context.model_argument, runtime)?;
        if let Some(words) = context.rotation_words {
            let basis = &mut self.rotating_visual_mut(object)?.basis;
            for (word, plane) in [
                (words[1], RotationPlane::Xz),
                (words[0], RotationPlane::Yz),
                (words[2], RotationPlane::Xy),
            ] {
                math.rotate_in_plane(basis, i32::from(word as i16), plane);
            }
        }
        Ok(())
    }

    /// 0047A720, 0047A730 and 0047A740 forward the full angle DWORD to the
    /// shared native Yz/Xz/Xy kernel, and 0047A750 to the world-Y one
    /// (00491A70). The lookup uses its low 16 bits and ignores bits 0..1.
    pub fn rotate_visual(
        &mut self,
        object: ObjectId,
        plane: RotationPlane,
        angle: i32,
        math: &NativeBasisMath,
    ) -> Result<(), ObjectError> {
        math.rotate_in_plane(&mut self.rotating_visual_mut(object)?.basis, angle, plane);
        Ok(())
    }

    /// 0047A6C0 -> 004909E0 transforms a signed-WORD local vector to raw
    /// WORDs. It neither adds the object position nor changes orientation.
    /// Taking the source by value preserves the native in-place call policy.
    pub fn transform_visual_vector_words(
        &self,
        object: ObjectId,
        local: [u16; 3],
    ) -> Result<[u16; 3], ObjectError> {
        let basis = self.rotating_visual(object)?.basis;
        let [x, y, z] = local.map(|word| f64::from(word as i16));
        Ok(std::array::from_fn(|component| {
            let coefficient = |first| f64::from(f32::from_bits(basis.words[first + component]));
            // 00490A0E..00490A21 sums X+Y, then Z. transform_point's
            // DWORD-input kernel sums Z+Y, then X, so it cannot be reused.
            let value = (x * coefficient(1) + y * coefficient(4)) + z * coefficient(7);
            fistp_low_word(value)
        }))
    }

    fn rotating_visual(&self, object: ObjectId) -> Result<&RotatingVisual, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::RotatingVisual(rotating)
            | ObjectClass::RotatingTerrainLight { rotating, .. } => Ok(rotating),
            ObjectClass::Selector58(pad) => Ok(&pad.rotating),
            ObjectClass::Selector45(marker) => Ok(&marker.rotating),
            ObjectClass::Selector46(territory) => Ok(&territory.rotating),
            ObjectClass::Race(race) => Ok(&race.rotating),
            ObjectClass::Selector83(patch) => Ok(&patch.rotating),
            ObjectClass::Selector64(gate) => Ok(&gate.rotating),
            ObjectClass::Selector76(state) => Ok(&state.rotating),
            ObjectClass::Actor(actor) => Ok(&actor.rotating),
            _ => Err(ObjectError::NotRotatingVisual(object)),
        }
    }

    fn rotating_visual_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut RotatingVisual, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::RotatingVisual(rotating)
            | ObjectClass::RotatingTerrainLight { rotating, .. } => Ok(rotating),
            ObjectClass::Selector58(pad) => Ok(&mut pad.rotating),
            ObjectClass::Selector45(marker) => Ok(&mut marker.rotating),
            ObjectClass::Selector46(territory) => Ok(&mut territory.rotating),
            ObjectClass::Race(race) => Ok(&mut race.rotating),
            ObjectClass::Selector83(patch) => Ok(&mut patch.rotating),
            ObjectClass::Selector64(gate) => Ok(&mut gate.rotating),
            ObjectClass::Selector76(state) => Ok(&mut state.rotating),
            ObjectClass::Actor(actor) => Ok(&mut actor.rotating),
            _ => Err(ObjectError::NotRotatingVisual(object)),
        }
    }
}

fn fistp_low_word(value: f64) -> u16 {
    let integral = value.round_ties_even();
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
        integral as i64 as u16
    } else {
        // Masked x87 integer-indefinite has zero in its low WORD.
        0
    }
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
