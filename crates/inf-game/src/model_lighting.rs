//! Native shade allocation/initialization at 004A0185, 004EBD30 and 004901B0.
//! Shade words retain unknown bytes; the normal consumer writes RGB only.

use inf_formats::{ModelEntry, ModelSlotRef};
use thiserror::Error;

use crate::{
    menu::MenuBasis,
    model_context::fistp_low32,
    model_scratch::{
        ModelScratchAllocation as ProcessAllocation, ModelScratchError, ProcessModelScratch,
    },
};

/// Live lighting globals. The ambient DWORD is distinct from the signed
/// ambient components used in normal lighting (00490130 preserves its high byte).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelLighting {
    /// 0055F2A0/A4/A8, consumed as single-precision memory operands.
    pub direction: [f32; 3],
    /// 0055F254; only the three low bytes participate in 004901B0.
    pub directional_word: u32,
    /// 0055F258/25C/260, before the packed ambient color's clamping.
    pub ambient: [i32; 3],
    /// 0055F264, copied in full to shade slots 0 and 1.
    pub ambient_word: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelShade {
    pub rgb: Option<[u8; 3]>,
    pub high_byte: Option<u8>,
}

impl ModelShade {
    pub fn from_word(word: u32) -> Self {
        let [r, g, b, high] = word.to_le_bytes();
        Self {
            rgb: Some([r, g, b]),
            high_byte: Some(high),
        }
    }

    /// 004EBDE0 independently modulates RGB bytes by unsigned record bytes.
    /// It does not write or synthesize a fourth byte.
    pub fn modulated_rgb(self, factors: [u8; 3]) -> Option<[u8; 3]> {
        self.rgb.map(|rgb| {
            std::array::from_fn(|i| {
                ((u32::from(rgb[i]) * u32::from(factors[i])) >> 7).min(255) as u8
            })
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ModelLightingError {
    #[error("model shade allocation needs two ambient slots, received {0}")]
    AmbientSlots(u16),
    #[error("model shade slot {0} is out of bounds")]
    Slot(u16),
    #[error("model scratch allocation ends at {end:#x}, beyond the native 0xFA000-byte arena")]
    ScratchAllocation { end: usize },
    #[error("model shade reference {slot} reads outside the mapped native scratch arena")]
    ScratchRead { slot: i32 },
    #[error("model shade preprocessing boundaries {0:?} are invalid")]
    Ranges([u16; 3]),
    #[error("model normal source for shade slot {0} is outside its source entry")]
    Normal(u16),
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error("model shading stopped after an execution failure")]
    Stopped,
}

/// Explicit address-only intrinsic fixture layout. Its surrounding arena bytes
/// are unknown. Production uses model_scratch::ModelScratchAllocation custody.
#[cfg(test)]
pub(crate) use crate::model_scratch::MODEL_SCRATCH_BYTES;

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ModelScratchAllocation {
    shade_base: usize,
    shade_count: u16,
    pub end: usize,
}

#[cfg(test)]
impl ModelScratchAllocation {
    pub fn new(cursor: usize, vertices: u16, shades: u16) -> Result<Self, ModelLightingError> {
        let shade_base = cursor + usize::from(vertices) * 24;
        let end = shade_base + usize::from(shades) * 4;
        if end > MODEL_SCRATCH_BYTES {
            return Err(ModelLightingError::ScratchAllocation { end });
        }
        Ok(Self {
            shade_base,
            shade_count: shades,
            end,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ModelShades {
    slots: Vec<ModelShade>,
    storage: ShadeStorage,
    stopped: bool,
    /// Non-native, for the scene feed: the world normal (about 4096 long)
    /// each shade slot's colour was lit along, where 004EBD30 computed one.
    normals: Vec<Option<[i32; 3]>>,
    /// Non-native, for the scene feed: the light those normals were shaded
    /// with, the model's own (`004901B0`'s inputs).
    lighting: Option<ModelLighting>,
}

#[derive(Clone, Debug)]
enum ShadeStorage {
    IsolatedFixture,
    #[cfg(test)]
    AddressedFixture {
        base: usize,
    },
    ProcessView {
        scratch: ProcessModelScratch,
        allocation: ProcessAllocation,
    },
}

fn decode_shade(bytes: [Option<u8>; 4]) -> ModelShade {
    let [r, g, b, high_byte] = bytes;
    ModelShade {
        rgb: match (r, g, b) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        },
        high_byte,
    }
}

impl ModelShades {
    /// 004EBE60 does not clear allocation memory. Only the two subsequent
    /// 00490340 calls establish complete initial words.
    pub fn new_invocation(count: u16, ambient_word: u32) -> Result<Self, ModelLightingError> {
        if count < 2 {
            return Err(ModelLightingError::AmbientSlots(count));
        }
        let mut slots = vec![ModelShade::default(); usize::from(count)];
        slots[..2].fill(ModelShade::from_word(ambient_word));
        Ok(Self {
            normals: vec![None; slots.len()],
            lighting: None,
            slots,
            storage: ShadeStorage::IsolatedFixture,
            stopped: false,
        })
    }

    /// An isolated fixture whose every shade is the given full DWORD.
    #[cfg(test)]
    pub(crate) fn from_words_for_tests(words: &[u32]) -> Self {
        Self {
            slots: words
                .iter()
                .map(|word| ModelShade::from_word(*word))
                .collect(),
            storage: ShadeStorage::IsolatedFixture,
            stopped: false,
            normals: vec![None; words.len()],
            lighting: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn in_scratch(
        allocation: ModelScratchAllocation,
        ambient_word: u32,
    ) -> Result<Self, ModelLightingError> {
        let mut shades = Self::new_invocation(allocation.shade_count, ambient_word)?;
        shades.storage = ShadeStorage::AddressedFixture {
            base: allocation.shade_base,
        };
        Ok(shades)
    }

    /// Native common-model initialization writes only ambient DWORDs 0 and 1.
    /// Every other shade byte comes from the same retained process arena.
    pub(crate) fn in_process_scratch(
        scratch: &ProcessModelScratch,
        allocation: ProcessAllocation,
        ambient_word: u32,
    ) -> Result<Self, ModelLightingError> {
        let count = allocation.layout.shade_count;
        if count < 2 {
            return Err(ModelLightingError::AmbientSlots(count));
        }
        let bytes = ambient_word.to_le_bytes().map(Some);
        scratch.write_shade(&allocation, 0, 0, bytes)?;
        scratch.write_shade(&allocation, 1, 0, bytes)?;
        let slots = (0..count)
            .map(|slot| scratch.read_shade(&allocation, slot, 0).map(decode_shade))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            normals: vec![None; slots.len()],
            lighting: None,
            slots,
            storage: ShadeStorage::ProcessView {
                scratch: scratch.clone(),
                allocation,
            },
            stopped: false,
        })
    }

    fn store_rgb(&mut self, slot: u16, rgb: [u8; 3]) -> Result<(), ModelLightingError> {
        if let ShadeStorage::ProcessView {
            scratch,
            allocation,
        } = &self.storage
        {
            scratch.write_shade(allocation, slot, 0, rgb.map(Some))?;
        }
        self.slots[usize::from(slot)].rgb = Some(rgb);
        Ok(())
    }

    /// Native E2/E6 producers read signed shade WORDs, including E2/E6 clipped
    /// references after the record. Outside the declared local shades, a full
    /// DWORD can still name mapped stale arena bytes. Preserve unknown values;
    /// address validity is not proof of initialized RGB or a zero high byte.
    /// E2 UV packets omit RGB; E6 consumers must validate any reached RGB input.
    pub(crate) fn scratch_shade(&self, slot: i32) -> Result<ModelShade, ModelLightingError> {
        match &self.storage {
            ShadeStorage::ProcessView {
                scratch,
                allocation,
            } => Ok(decode_shade(scratch.read_mapped_shade(allocation, slot)?)),
            #[cfg(test)]
            ShadeStorage::AddressedFixture { base } => {
                let offset = *base as i64 + i64::from(slot) * 4;
                if !(0..=(MODEL_SCRATCH_BYTES - 4) as i64).contains(&offset) {
                    return Err(ModelLightingError::ScratchRead { slot });
                }
                Ok(usize::try_from(slot)
                    .ok()
                    .and_then(|index| self.slots.get(index))
                    .copied()
                    .unwrap_or_default())
            }
            ShadeStorage::IsolatedFixture => {
                let slot =
                    u16::try_from(slot).map_err(|_| ModelLightingError::ScratchRead { slot })?;
                self.shade(ModelSlotRef::from_raw(slot))
            }
        }
    }

    pub fn shade(&self, slot: ModelSlotRef) -> Result<ModelShade, ModelLightingError> {
        self.slots
            .get(usize::from(slot.raw()))
            .copied()
            .ok_or(ModelLightingError::Slot(slot.raw()))
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// Non-native, for the scene feed: the world normal shade `slot` was lit
    /// along, where this invocation computed one.
    pub fn normal(&self, slot: i32) -> Option<[i32; 3]> {
        usize::try_from(slot)
            .ok()
            .and_then(|slot| self.normals.get(slot).copied().flatten())
    }

    /// Non-native, for the scene feed: the light this invocation shaded its
    /// normals with.
    pub fn lighting(&self) -> Option<ModelLighting> {
        self.lighting
    }

    /// 004EBD30 reads raw normals at normal_pool - 6 + (slot/2)*6.
    /// In particular slot zero reads the six bytes BEFORE the normal pool;
    /// silently starting at normals[0] shifts every shade by one pair.
    pub fn initialize_normals(
        &mut self,
        model: &ModelEntry,
        source: &[u8],
        basis: &MenuBasis,
        lighting: ModelLighting,
    ) -> Result<(), ModelLightingError> {
        if self.stopped {
            return Err(ModelLightingError::Stopped);
        }
        let result = self.initialize_inner(model, source, basis, lighting);
        if result.is_err() {
            self.stopped = true;
        }
        result
    }

    /// Complete 004EBD30 phase for Common05. No ambient reseeding and no
    /// preflight of unrelated raw/world ranges. Root header checks stay intact.
    pub(crate) fn initialize_normal_ranges(
        &mut self,
        model: &ModelEntry,
        source: &[u8],
        basis: &MenuBasis,
        lighting: ModelLighting,
        ranges: [i32; 3],
    ) -> Result<(), ModelLightingError> {
        if self.stopped {
            return Err(ModelLightingError::Stopped);
        }
        let result = (|| {
            let [start, single_end, paired_end] = ranges;
            if start < 0
                || !(start <= single_end && single_end <= paired_end)
                || ranges.iter().any(|v| v & 1 != 0)
                || paired_end as usize > self.slots.len()
            {
                return Err(ModelLightingError::Ranges(ranges.map(|v| v as u16)));
            }
            self.initialize_ranges_inner(model, source, basis, lighting, ranges.map(|v| v as u16))
        })();
        if result.is_err() {
            self.stopped = true;
        }
        result
    }

    fn initialize_inner(
        &mut self,
        model: &ModelEntry,
        source: &[u8],
        basis: &MenuBasis,
        lighting: ModelLighting,
    ) -> Result<(), ModelLightingError> {
        let ranges = [
            model.header.reserved_1a,
            model.header.shade_ranges[0],
            model.header.shade_ranges[1],
        ];
        let [start, single_end, paired_end] = ranges;
        if ranges.iter().any(|v| v & 1 != 0)
            || start > single_end
            || single_end > paired_end
            || usize::from(paired_end) > self.slots.len()
        {
            return Err(ModelLightingError::Ranges(ranges));
        }
        self.initialize_ranges_inner(model, source, basis, lighting, ranges)
    }

    fn initialize_ranges_inner(
        &mut self,
        model: &ModelEntry,
        source: &[u8],
        basis: &MenuBasis,
        lighting: ModelLighting,
        [start, single_end, paired_end]: [u16; 3],
    ) -> Result<(), ModelLightingError> {
        self.lighting = Some(lighting);
        for slot in (start..paired_end).step_by(2) {
            let offset = model
                .normals_span
                .start
                .checked_add(usize::from(slot / 2) * 6)
                .and_then(|value| value.checked_sub(6))
                .ok_or(ModelLightingError::Normal(slot))?;
            let end = offset
                .checked_add(6)
                .ok_or(ModelLightingError::Normal(slot))?;
            if offset < model.span.start || end > model.content_span.end {
                return Err(ModelLightingError::Normal(slot));
            }
            let bytes = source
                .get(offset..end)
                .ok_or(ModelLightingError::Normal(slot))?;
            let normal: [i16; 3] =
                std::array::from_fn(|i| i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]));
            self.store_rgb(slot, shade_normal(normal, basis, lighting))?;
            self.normals[usize::from(slot)] = Some(transform_normal(normal, basis));
            if slot >= single_end {
                // EDX is negated as i32; 004901B0 sign-extends DX again.
                let mirrored = [normal[0].wrapping_neg(), normal[1], normal[2]];
                self.store_rgb(slot + 1, shade_normal(mirrored, basis, lighting))?;
                self.normals[usize::from(slot) + 1] = Some(transform_normal(mirrored, basis));
            }
        }
        Ok(())
    }
}

/// 004901B0: retain 53-bit x87 products/additions, FISTP QWORD then low DWORD,
/// reload integer components, and preserve wrapping signed color arithmetic.
pub fn shade_normal(normal: [i16; 3], basis: &MenuBasis, lighting: ModelLighting) -> [u8; 3] {
    shade_world(transform_normal(normal, basis), lighting)
}

/// 004901B0's second half, on a normal already in the world (as
/// [`transform_normal`] gives it): for the scene feed's round parts, whose
/// new corners take retail's shade at their own normals.
pub fn shade_world(normal: [i32; 3], lighting: ModelLighting) -> [u8; 3] {
    let [x, y, z] = normal.map(f64::from);
    let [dx, dy, dz] = lighting.direction.map(f64::from);
    let intensity = fistp_low32((y * dy + z * dz) + x * dx).max(0);
    let directional = lighting.directional_word.to_le_bytes();
    let mut rgb: [i32; 3] = std::array::from_fn(|i| {
        (i32::from(directional[i]).wrapping_mul(intensity) >> 12).wrapping_add(lighting.ambient[i])
    });
    let maximum = *rgb.iter().max().expect("three components");
    if maximum > 255 {
        rgb = rgb.map(|value| value.wrapping_mul(255) / maximum);
    }
    rgb.map(|value| value.max(0) as u8)
}

/// 004901B0's first half: the model normal rotated into the world by the
/// object's basis, rounded as the x87 code rounds it.
fn transform_normal(normal: [i16; 3], basis: &MenuBasis) -> [i32; 3] {
    let [x, y, z] = normal.map(f64::from);
    std::array::from_fn(|i| {
        let a = f64::from(f32::from_bits(basis.words[1 + i]));
        let b = f64::from(f32::from_bits(basis.words[4 + i]));
        let c = f64::from(f32::from_bits(basis.words[7 + i]));
        fistp_low32((z * c + y * b) + x * a)
    })
}

#[cfg(test)]
#[path = "model_lighting_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "model_lighting_process_tests.rs"]
mod process_tests;
