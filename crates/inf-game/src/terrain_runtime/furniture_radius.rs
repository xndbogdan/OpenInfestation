//! Ordered sparse-record radius publication, 004508E0.
//!
//! The process WORD 005483C0 is an explicit output. H11/H8 lookup is a
//! required service; missing H11 cannot supply a replacement table or radius.
//! Runtime sparse storage/counts/X ordering retain the loader-proven shape.
//! Services may change payloads and the output through the same live request,
//! but cannot replace that fixed storage while a bucket pointer is retained.

use inf_formats::{TerrainResourceLookup, TERRAIN_GRID_SIDE};
use thiserror::Error;

use super::{TerrainRuntime, TerrainSparsePayload, TerrainSparseRecordId, TerrainSparseWriteError};
use crate::{
    world_furniture_effect::resident::{
        resident_furniture_model_radius, ResidentFurnitureResourceError,
    },
    ResidentRuntime,
};

mod process;
pub use process::ProcessFurnitureRadius;

pub struct TerrainFurnitureRadiusRequest<'a> {
    terrain: &'a mut TerrainRuntime,
    process_radius_word: &'a mut u16,
}

impl<'a> TerrainFurnitureRadiusRequest<'a> {
    pub fn new(terrain: &'a mut TerrainRuntime, process_radius_word: &'a mut u16) -> Self {
        Self {
            terrain,
            process_radius_word,
        }
    }

    pub fn terrain(&self) -> &TerrainRuntime {
        self.terrain
    }

    pub fn process_radius_word(&self) -> u16 {
        *self.process_radius_word
    }

    pub fn write_process_radius_word(&mut self, word: u16) {
        *self.process_radius_word = word;
    }

    pub fn write_sparse_payload(
        &mut self,
        record: TerrainSparseRecordId,
        payload: TerrainSparsePayload,
    ) -> Result<(), TerrainSparseWriteError> {
        self.terrain.write_sparse_payload(record, payload)
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum TerrainFurnitureRadiusError<E> {
    /// 45094D supplies NULL, then 45094F unconditionally reads its first WORD.
    #[error("terrain radius producer reached zero lookup at {record:?}")]
    ZeroLookup { record: TerrainSparseRecordId },
    #[error("terrain radius producer is missing retained bucket {bucket}")]
    MissingBucket { bucket: u16 },
    #[error("terrain radius resolver failed at {record:?}")]
    Resolver {
        record: TerrainSparseRecordId,
        error: E,
    },
}

/// 004508E0 walks all 512 buckets and every stored record in order. The
/// resolver receives the lookup BYTE cached before resource acquisition.
/// `None` must describe a proven native skip (negative/out-of-bound H8 slot
/// or loader-null model). Resolver errors preserve reset and earlier writes;
/// the final signed-WORD cap is reached only on normal completion.
pub fn produce_furniture_radius<E>(
    mut request: TerrainFurnitureRadiusRequest<'_>,
    mut resolve: impl FnMut(
        &mut TerrainFurnitureRadiusRequest<'_>,
        TerrainResourceLookup,
    ) -> Result<Option<u16>, E>,
) -> Result<(), TerrainFurnitureRadiusError<E>> {
    request.write_process_radius_word(0); //4508F5: precedes every record read.
    for bucket in 0..TERRAIN_GRID_SIDE as u16 {
        let mut index = 0;
        loop {
            let records = &request
                .terrain
                .buckets
                .get(usize::from(bucket))
                .ok_or(TerrainFurnitureRadiusError::MissingBucket { bucket })?
                .records;
            //4509D6 rereads the count after each record. The runtime API keeps
            //that loader-fixed count stable, while payload reads remain live.
            let Some(record) = records.get(index) else {
                break;
            };
            let lookup = record.resource_lookup;
            let record = TerrainSparseRecordId { bucket, index };
            if lookup.raw() == 0 {
                return Err(TerrainFurnitureRadiusError::ZeroLookup { record });
            }
            if let Some(radius) = resolve(&mut request, lookup)
                .map_err(|error| TerrainFurnitureRadiusError::Resolver { record, error })?
            {
                //4509B2 sign-extends current WORD; 4509B9 zero-extends radius.
                //A negative stored radius can therefore be replaced by zero.
                if i32::from(radius) > i32::from(request.process_radius_word() as i16) {
                    request.write_process_radius_word(radius);
                }
            }
            index += 1;
        }
    }
    if (request.process_radius_word() as i16) > 512 {
        request.write_process_radius_word(512);
    }
    Ok(())
}

/// Concrete H11/0 and H8 metadata binding. Native manager +00 is NULL for an
/// absent model; missing H11 and the inclusive H8 pointer-table boundary are
/// errors through the existing resident policy. The world backend borrows its
/// retained process output and calls this before retained presentation lighting/map preparation.
pub fn produce_resident_furniture_radius(
    request: TerrainFurnitureRadiusRequest<'_>,
    resident: &ResidentRuntime,
) -> Result<(), TerrainFurnitureRadiusError<ResidentFurnitureResourceError>> {
    produce_furniture_radius(request, |_, lookup| {
        resident_furniture_model_radius(resident, lookup)
    })
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
