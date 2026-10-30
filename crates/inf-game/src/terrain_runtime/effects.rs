//! Mutable H12 effects behind 00436390 and 0047E190.
//!
//! The cells and sparse payload bytes belong to TerrainRuntime's copy. H11/H8
//! lookup and 00436020 furniture damage are required services. Source identity
//! stays live across those services: later reads may observe actor/block changes.

use inf_formats::{TerrainRecord, TerrainResourceLookup, TERRAIN_GRID_SIDE};
use thiserror::Error;

use super::TerrainRuntime;
use crate::name_entry::name_entry_rng_next;
use crate::world_radius_effect::{
    radius_word_distance, scale_radius_effect, RadiusEffectBlock, ScaledRadiusEffect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainSparseRecordId {
    pub bucket: u16,
    pub index: usize,
}

/// Bytes +2/+3 of one four-byte sparse record. X and bucket ordering are fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainSparsePayload {
    pub state: u8,
    pub resource_lookup: TerrainResourceLookup,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("terrain sparse record {record:?} is outside the retained runtime buckets")]
pub struct TerrainSparseWriteError {
    pub record: TerrainSparseRecordId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureDamageRequest {
    /// Original candidate coordinates, before cell alignment. Only low WORDs
    /// reach 00436020 and its later 00436A60 packed coordinate key.
    pub xz: [u16; 2],
    /// 0042A800's copied/scaled first DWORD; no impulse destination was passed.
    pub effect: ScaledRadiusEffect,
}

pub trait TerrainFurnitureEffectHost {
    type Error;
    /// Identifies the original block/position allocations, rather than their
    /// snapshots. A callback may mutate these before later native rereads.
    type Source: Copy;

    fn source_block(&mut self, source: Self::Source) -> Result<RadiusEffectBlock, Self::Error>;
    fn source_position_words(&mut self, source: Self::Source) -> Result<[u16; 3], Self::Error>;
    /// Resolve H11 slot 0's indexed first WORD through H8, returning raw model
    /// WORD +0E. `None` means a proven native null model, not unowned loading.
    fn furniture_model_radius(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<u16>, Self::Error>;
    /// Required 00436020. Its result increments the native local count. It can
    /// queue 00436A60 effects; returning true does not imply immediate deletion.
    fn apply_furniture_damage(
        &mut self,
        terrain: &mut TerrainRuntime,
        request: FurnitureDamageRequest,
        rng: &mut u32,
    ) -> Result<bool, Self::Error>;
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum TerrainFurnitureEffectError<E> {
    #[error("terrain furniture effect host operation failed")]
    Host(E),
    #[error("native signed-WORD radius {radius} walk repeats with spacing {spacing}")]
    NonterminatingRadius { radius: i16, spacing: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainFlagScatterRequest {
    pub position_words: [u16; 3],
    /// Native argument is tested before decrement. Two performs three writes;
    /// zero and negative arguments still execute the initial do/while body.
    pub count_minus_one: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainFlagScatterOutcome {
    pub height_gate_passed: bool,
    pub operations: u32,
}

impl TerrainRuntime {
    ///436FFE/4370CE read the aligned cell's low flags nibble, but store it
    ///at the original unaligned grid coordinate. The destination height byte
    ///survives; coarse spacing must not silently align that destination too.
    pub(crate) fn copy_aligned_low_flags_to_cell(&mut self, xz: [u16; 2]) {
        let flags = self.cell_at(xz).raw() & 0x0f00;
        let index = usize::from(xz[0] >> 7) * TERRAIN_GRID_SIDE + usize::from(xz[1] >> 7);
        self.cells[index] =
            inf_formats::TerrainCell::from_raw((self.cells[index].raw() & 0x00ff) | flags);
    }

    /// 00435260 lookup after the caller's flag-4/alignment gates. The canonical
    /// H12 decoder proves strictly sorted unique X records in each bucket.
    pub fn sparse_record_at(&self, xz: [u16; 2]) -> Option<(TerrainSparseRecordId, TerrainRecord)> {
        if !self.cell_at(xz).has_sparse_record() {
            return None;
        }
        self.sparse_record_at_grid(self.effect_grid_coordinates(xz))
    }

    ///435260 after an already reached flag04 gate and live grid conversion.
    ///No second flag read can silently turn a missing row into fallback.
    pub(crate) fn sparse_record_at_grid(
        &self,
        [x, z]: [u16; 2],
    ) -> Option<(TerrainSparseRecordId, TerrainRecord)> {
        let records = &self.buckets[usize::from(z)].records;
        let index = records.binary_search_by_key(&x, |record| record.x).ok()?;
        Some((TerrainSparseRecordId { bucket: z, index }, records[index]))
    }

    /// Write only sparse state/lookup bytes in the runtime copy. This cannot
    /// reorder records, change their X words or rewrite an original allocation.
    pub fn write_sparse_payload(
        &mut self,
        record: TerrainSparseRecordId,
        payload: TerrainSparsePayload,
    ) -> Result<(), TerrainSparseWriteError> {
        let target = self
            .buckets
            .get_mut(usize::from(record.bucket))
            .and_then(|bucket| bucket.records.get_mut(record.index))
            .ok_or(TerrainSparseWriteError { record })?;
        target.state = payload.state;
        target.resource_lookup = payload.resource_lookup;
        Ok(())
    }

    /// 00436390: full X-outer/Z-inner signed-WORD square traversal. Required
    /// host failures preserve all prior sparse writes and callback effects.
    /// The initially cached radius controls traversal; falloff rereads the
    /// original block, and each reached origin/vertical calculation rereads
    /// the original position allocation.
    pub fn apply_furniture_radius_effect<H: TerrainFurnitureEffectHost>(
        &mut self,
        source: H::Source,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<u32, TerrainFurnitureEffectError<H::Error>> {
        let radius = host
            .source_block(source)
            .map_err(TerrainFurnitureEffectError::Host)?
            .outer_radius as i16;
        let start = radius.wrapping_neg();
        if start > radius {
            return Ok(0);
        }
        let spacing = self.sampling.spacing.units();
        let mut count = 0_u32;
        let mut x_offset = start;
        loop {
            let mut z_offset = start;
            loop {
                let origin = host
                    .source_position_words(source)
                    .map_err(TerrainFurnitureEffectError::Host)?;
                let xz = [
                    origin[0].wrapping_add(x_offset as u16),
                    origin[2].wrapping_add(z_offset as u16),
                ];
                if let Some((record_id, record)) = self.sparse_record_at(xz) {
                    if record.resource_lookup.raw() != 0 {
                        match host
                            .furniture_model_radius(record.resource_lookup)
                            .map_err(TerrainFurnitureEffectError::Host)?
                        {
                            None => {
                                // 004366EF/004366F3 clear only bytes +2/+3.
                                self.write_sparse_payload(record_id, TerrainSparsePayload {
                                    state: 0,
                                    resource_lookup: TerrainResourceLookup::from_raw(0),
                                }).expect("record was resolved in this runtime without a mutable host call");
                            }
                            Some(model_radius) => {
                                let mut y = self.furniture_corner_average(xz);
                                let origin = host
                                    .source_position_words(source)
                                    .map_err(TerrainFurnitureEffectError::Host)?;
                                let origin_y = origin[1] as i16;
                                if y < origin_y {
                                    y = y.wrapping_add(model_radius as i16);
                                    if origin_y < y {
                                        y = origin_y;
                                    }
                                }
                                let direction = [
                                    origin[0].wrapping_sub(xz[0]) as i16,
                                    origin[1].wrapping_sub(y as u16) as i16,
                                    origin[2].wrapping_sub(xz[1]) as i16,
                                ];
                                let distance = radius_word_distance(direction) as i32;
                                let block = host
                                    .source_block(source)
                                    .map_err(TerrainFurnitureEffectError::Host)?;
                                if let Some(effect) = scale_radius_effect(block, distance, None) {
                                    if effect.accepted()
                                        && host
                                            .apply_furniture_damage(
                                                self,
                                                FurnitureDamageRequest { xz, effect },
                                                rng,
                                            )
                                            .map_err(TerrainFurnitureEffectError::Host)?
                                    {
                                        count = count.wrapping_add(1);
                                    }
                                }
                            }
                        }
                    }
                }
                match next_radius_offset(z_offset, start, radius, spacing)? {
                    Some(next) => z_offset = next,
                    None => break,
                }
            }
            match next_radius_offset(x_offset, start, radius, spacing)? {
                Some(next) => x_offset = next,
                None => return Ok(count),
            }
        }
    }

    /// 0047E190. With retained H12, the interpolated signed height gate runs
    /// before any RNG draw. Each operation clears the selected high-byte
    /// nibble, preserving the low height byte and the flags' low nibble.
    pub fn scatter_cell_flags(
        &mut self,
        request: TerrainFlagScatterRequest,
        rng: &mut u32,
    ) -> TerrainFlagScatterOutcome {
        let [x, y, z] = request.position_words;
        let height = self.height_at([x, z]);
        if (i32::from(y as i16) - i32::from(height)).abs() > 128 {
            return TerrainFlagScatterOutcome {
                height_gate_passed: false,
                operations: 0,
            };
        }
        let mask = self.sampling.spacing.units() - 1;
        let operations = if request.count_minus_one > 0 {
            request.count_minus_one as u32 + 1
        } else {
            1
        };
        for _ in 0..operations {
            let sample = name_entry_rng_next(rng);
            let signed_offset = |bits: u32| {
                let sign = if bits & 4 != 0 { 1_i32 } else { -1_i32 };
                (sign * (bits & 3) as i32 * 64) as u16
            };
            let xz = [
                x.wrapping_add(signed_offset(sample)) & !mask,
                z.wrapping_add(signed_offset(sample >> 3)) & !mask,
            ];
            let [gx, gz] = self.effect_grid_coordinates(xz);
            let index = usize::from(gx) * TERRAIN_GRID_SIDE + usize::from(gz);
            self.cells[index] =
                inf_formats::TerrainCell::from_raw(self.cells[index].raw() & 0x0fff);
        }
        TerrainFlagScatterOutcome {
            height_gate_passed: true,
            operations,
        }
    }

    fn effect_grid_coordinates(&self, [x, z]: [u16; 2]) -> [u16; 2] {
        let mask = !((self.sampling.spacing.units() - 1) >> 7);
        [(x >> 7) & mask, (z >> 7) & mask]
    }

    fn furniture_corner_average(&self, [x, z]: [u16; 2]) -> i16 {
        let spacing = self.sampling.spacing.units();
        let shift = self.sampling.height_scale.shift();
        let sum: i32 = [
            [x, z],
            [x.wrapping_add(spacing), z],
            [x.wrapping_add(spacing), z.wrapping_add(spacing)],
            [x, z.wrapping_add(spacing)],
        ]
        .into_iter()
        .map(|xz| {
            let shifted_word = (i32::from(self.cell_at(xz).height_i8()) << shift) as i16;
            i32::from(shifted_word)
        })
        .sum();
        // Native adds the negative correction before SAR, truncating /4.
        (sum / 4) as i16
    }
}

fn next_radius_offset<E>(
    current: i16,
    start: i16,
    radius: i16,
    spacing: u16,
) -> Result<Option<i16>, TerrainFurnitureEffectError<E>> {
    let next = current.wrapping_add(spacing as i16);
    if next > radius {
        Ok(None)
    } else if next == start {
        // Some malformed/saturated radii produce a native endless loop.
        // Stop only after retaining the first complete reached cycle.
        Err(TerrainFurnitureEffectError::NonterminatingRadius { radius, spacing })
    } else {
        Ok(Some(next))
    }
}

#[cfg(test)]
mod tests;
