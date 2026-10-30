use super::TerrainProbeCache;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TerrainProbeError<E> {
    #[error("00429EE0 terrain/collision miss service failed: {0}")]
    Host(E),
    #[error("terrain probe DWORD {word_index} is outside the current {words}-DWORD allocation")]
    StorageRange { word_index: usize, words: usize },
}

pub(super) enum PreparedTerrainProbe {
    Hit(u8),
    Miss(TerrainProbeMiss),
}

/// Opaque and non-cloneable. Consumed exactly once on normal completion.
pub(super) struct TerrainProbeMiss {
    pub(super) position_words: [u16; 2],
    relative: [u16; 2],
}

#[derive(Clone, Copy)]
enum WindowDirection {
    Advance,
    Retreat,
}

impl TerrainProbeCache {
    /// 00454C20 with BYTE 1. Clear packed samples without moving the origin or
    /// initializing header bytes +08/+0B.
    pub fn clear_samples(&mut self) {
        self.words_00.fill(0);
    }

    /// 00454ED0: X/Z are unsigned world WORDs; SHR6 selects 64-unit cells.
    /// A nonzero encoded nibble returns nibble-1 without calling the service.
    /// On a miss the service receives this same live cache, after all window
    /// mutations. It may mutate the cache synchronously. Failure preserves
    /// reached writes. Completion uses captured relative cells with the
    /// current stride/storage, exactly as 00454BE0 does after the callback.
    ///
    /// This intrinsic owner supplies no actor, terrain or collision result.
    /// The caller must provide the actual 00429EE0 service and its context.
    pub fn probe<E>(
        &mut self,
        position_words: [u16; 2],
        miss: impl FnOnce(&mut Self, [u16; 2]) -> Result<u32, E>,
    ) -> Result<u8, TerrainProbeError<E>> {
        match self.prepare_probe(position_words)? {
            PreparedTerrainProbe::Hit(value) => Ok(value),
            PreparedTerrainProbe::Miss(prepared) => {
                let sample =
                    miss(self, prepared.position_words).map_err(TerrainProbeError::Host)?;
                self.finish_probe(prepared, sample)
            }
        }
    }

    /// Window mutations and read precede 00429EE0. The miss token retains
    /// original WORD coordinates and captured relative cells, not cache data.
    pub(super) fn prepare_probe<E>(
        &mut self,
        position_words: [u16; 2],
    ) -> Result<PreparedTerrainProbe, TerrainProbeError<E>> {
        let cells = position_words.map(|word| word >> 6);
        let mut relative = [
            cells[0].wrapping_sub(self.word_04),
            cells[1].wrapping_sub(self.word_06),
        ];
        let dimension = u16::from(self.dimension_09);
        if relative.iter().any(|&cell| cell >= dimension) {
            let nearby = relative.iter().all(|&cell| {
                cell < dimension * 2 - 1 || u32::from(cell) > 65536 - u32::from(dimension)
            });
            if nearby {
                for axis in 0..2 {
                    if relative[axis] < dimension {
                        continue;
                    }
                    let (amount, direction, new_relative) = if relative[axis] < dimension * 2 - 1 {
                        (
                            relative[axis] - dimension + 1,
                            WindowDirection::Advance,
                            dimension - 1,
                        )
                    } else {
                        (relative[axis].wrapping_neg(), WindowDirection::Retreat, 0)
                    };
                    let origin = if axis == 0 {
                        &mut self.word_04
                    } else {
                        &mut self.word_06
                    };
                    *origin = match direction {
                        WindowDirection::Advance => origin.wrapping_add(amount),
                        WindowDirection::Retreat => origin.wrapping_sub(amount),
                    };
                    relative[axis] = new_relative;
                    if axis == 0 {
                        self.slide_x(amount, direction);
                    } else {
                        self.slide_z(amount, direction);
                    }
                }
            } else {
                self.clear_samples();
                self.word_04 = cells[0];
                self.word_06 = cells[1];
                relative = [0, 0];
            }
        }
        let encoded = self.read_sample(relative)?;
        if encoded != 0 {
            return Ok(PreparedTerrainProbe::Hit(encoded - 1));
        }
        Ok(PreparedTerrainProbe::Miss(TerrainProbeMiss {
            position_words,
            relative,
        }))
    }

    /// Called only after normal callback return. Re-read current stride and
    /// storage using the captured cells; do not quantize coordinates again.
    pub(super) fn finish_probe<E>(
        &mut self,
        prepared: TerrainProbeMiss,
        sample: u32,
    ) -> Result<u8, TerrainProbeError<E>> {
        let relative = prepared.relative;
        let sample = sample as u8;
        // 00454BE0 ORs the full BYTE+1 DWORD: it neither clears the destination
        // nibble nor masks the encoded value to four bits. Preserve spill.
        let index = self.sample_index(relative);
        let words = self.words_00.len();
        let destination = self
            .words_00
            .get_mut(index)
            .ok_or(TerrainProbeError::StorageRange {
                word_index: index,
                words,
            })?;
        *destination |= (u32::from(sample) + 1).wrapping_shl(u32::from(relative[0] & 7) * 4);
        Ok(sample)
    }

    fn sample_index(&self, relative: [u16; 2]) -> usize {
        usize::from(relative[1]) * usize::from(self.stride_words_0a) + usize::from(relative[0] / 8)
    }

    fn read_sample<E>(&self, relative: [u16; 2]) -> Result<u8, TerrainProbeError<E>> {
        let index = self.sample_index(relative);
        let word = self
            .words_00
            .get(index)
            .ok_or(TerrainProbeError::StorageRange {
                word_index: index,
                words: self.words_00.len(),
            })?;
        Ok(((word >> (u32::from(relative[0] & 7) * 4)) & 15) as u8)
    }

    /// 00454D70/00454E30 operate on adjacent DWORDs and mask x86 shift counts
    /// modulo 32. A shift of eight cells is therefore not a whole-word move.
    fn slide_x(&mut self, amount: u16, direction: WindowDirection) {
        let stride = usize::from(self.stride_words_0a);
        let shift = u32::from(amount) * 4;
        let carry = 32_u32.wrapping_sub(shift);
        for row in self.words_00.chunks_exact_mut(stride) {
            match direction {
                WindowDirection::Advance => {
                    for column in 0..stride - 1 {
                        row[column] =
                            row[column].wrapping_shr(shift) | row[column + 1].wrapping_shl(carry);
                    }
                    row[stride - 1] = row[stride - 1].wrapping_shr(shift);
                }
                WindowDirection::Retreat => {
                    for column in (1..stride).rev() {
                        row[column] =
                            row[column].wrapping_shl(shift) | row[column - 1].wrapping_shr(carry);
                    }
                    row[0] = row[0].wrapping_shl(shift);
                }
            }
        }
    }

    /// 00454C60/00454CF0 copy rows in the direction that preserves overlap,
    /// then zero the newly exposed rows.
    fn slide_z(&mut self, amount: u16, direction: WindowDirection) {
        let shift = usize::from(amount) * usize::from(self.stride_words_0a);
        let words = self.words_00.len();
        match direction {
            WindowDirection::Advance => {
                self.words_00.copy_within(shift..words, 0);
                self.words_00[words - shift..].fill(0);
            }
            WindowDirection::Retreat => {
                self.words_00.copy_within(0..words - shift, shift);
                self.words_00[..shift].fill(0);
            }
        }
    }
}

#[cfg(test)]
mod tests;
