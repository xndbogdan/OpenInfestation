//! 00437110 and 00437150: a furniture marker's (selector 75) removal of a
//! chain of furniture of one kind.
//!
//! 00437110 runs 00437150 under the temporary fine spacing (00450350, then
//! 00450370 rereads the detail setting). 00437150 searches squares round its
//! point, 0x80 a side larger each time out to 0x200 (X outer, Z inner, each
//! from the low edge to the high). A cell with the sparse flag reads its
//! record; one without reads the process fallback record 005483C4. A record
//! whose H11 entry (byte +3) has the kind (that entry's byte +2) is removed:
//! its bytes +2 and +3 are cleared and it counts. The square's first removal
//! becomes the next point; each later one in the same square starts a search
//! of its own there (a recursion) whose count adds. After a square with a
//! removal the search starts over from the next point; after the 0x200
//! square without one it ends. So the walk follows furniture up to four
//! cells apart and returns how many pieces it took.

use inf_formats::TerrainResourceLookup;

use super::{
    FurnitureChainLoan, FurnitureChainResources, ProcessFurnitureQueryState,
    ResidentFurnitureChainResources,
};
use crate::terrain_runtime::{TerrainRuntime, TerrainSparsePayload, TerrainSparseWriteError};

/// 0043717E: the last square's low edge.
const REACH: i16 = -0x200;
const STEP: i16 = 0x80;

#[derive(Debug, thiserror::Error)]
pub enum FurnitureMarkerError<E> {
    #[error("furniture marker resource failed: {0:?}")]
    Resource(E),
    #[error("cell {xz:?} carries the sparse flag without a record (0043728F reads NULL)")]
    FlaggedCellWithoutRecord { xz: [u16; 2] },
    #[error(transparent)]
    Write(#[from] TerrainSparseWriteError),
}

/// 00437150 at the current spacing (the fine one under 00437110). `fallback`
/// is the process record 005483C4, which a match also clears.
pub fn remove_furniture_chain<R: FurnitureChainResources>(
    terrain: &mut TerrainRuntime,
    mut point: [u16; 2],
    kind: u8,
    fallback: &mut TerrainSparsePayload,
    resources: &mut R,
) -> Result<u32, FurnitureMarkerError<R::Error>> {
    let mut removed = 0_u32;
    loop {
        let mut next = None;
        let (mut low, mut high) = (0_i16, 0_i16);
        while next.is_none() && low >= REACH {
            let mut dx = low;
            while dx <= high {
                let mut dz = low;
                while dz <= high {
                    let xz = [
                        point[0].wrapping_add(dx as u16),
                        point[1].wrapping_add(dz as u16),
                    ];
                    if take(terrain, xz, kind, fallback, resources)? {
                        removed = removed.wrapping_add(1);
                        if next.is_none() {
                            next = Some(xz);
                        } else {
                            let more =
                                remove_furniture_chain(terrain, xz, kind, fallback, resources)?;
                            removed = removed.wrapping_add(more);
                        }
                    }
                    dz += STEP;
                }
                dx += STEP;
            }
            low -= STEP;
            high += STEP;
        }
        match next {
            Some(xz) => point = xz,
            None => return Ok(removed),
        }
    }
}

/// One cell of the search: whether its record (or the fallback) had the
/// kind, in which case its bytes +2 and +3 are now clear.
fn take<R: FurnitureChainResources>(
    terrain: &mut TerrainRuntime,
    xz: [u16; 2],
    kind: u8,
    fallback: &mut TerrainSparsePayload,
    resources: &mut R,
) -> Result<bool, FurnitureMarkerError<R::Error>> {
    let record = if terrain.cell_at(xz).has_sparse_record() {
        let (id, record) = terrain
            .sparse_record_at(xz)
            .ok_or(FurnitureMarkerError::FlaggedCellWithoutRecord { xz })?;
        Some((id, record.resource_lookup))
    } else {
        None
    };
    let lookup = record.map_or(fallback.resource_lookup, |(_, lookup)| lookup);
    if lookup.raw() == 0
        || resources
            .kind(lookup)
            .map_err(FurnitureMarkerError::Resource)?
            != kind
    {
        return Ok(false);
    }
    let clear = TerrainSparsePayload {
        state: 0,
        resource_lookup: TerrainResourceLookup::from_raw(0),
    };
    match record {
        Some((id, _)) => terrain.write_sparse_payload(id, clear)?,
        None => *fallback = clear,
    }
    Ok(true)
}

/// 00437110: 00437150 at `xz` under the temporary fine spacing, then the
/// detail setting's spacing again.
pub fn remove_marked_furniture<R: FurnitureChainResources>(
    terrain: &mut TerrainRuntime,
    xz: [u16; 2],
    kind: u8,
    fallback: &mut TerrainSparsePayload,
    loan: &mut FurnitureChainLoan<'_>,
    resources: &mut R,
) -> Result<u32, FurnitureMarkerError<R::Error>> {
    loan.policy.temporary_fine(terrain);
    let removed = remove_furniture_chain(terrain, xz, kind, fallback, resources);
    loan.policy
        .restore_detail(loan.detail, loan.world_flags, terrain);
    removed
}

/// The world's loans a marker's removal takes: the world's terrain, the
/// spacing policy with the detail setting 00450370 rereads, the resident H11
/// kinds and the process fallback record 005483C4.
pub struct MarkerFurniture<'a, R = ResidentFurnitureChainResources<'a>> {
    pub terrain: &'a mut TerrainRuntime,
    pub chain: FurnitureChainLoan<'a>,
    pub resources: R,
    pub query: &'a mut ProcessFurnitureQueryState,
}

impl<R: FurnitureChainResources<Error = String>> MarkerFurniture<'_, R> {
    /// 00437110 at `xz` for `kind`. A removal that takes the fallback record
    /// clears the process's.
    pub fn remove(&mut self, xz: [u16; 2], kind: u8) -> Result<u32, FurnitureMarkerError<String>> {
        let before = self.query.fallback_payload();
        let mut fallback = before;
        let removed = remove_marked_furniture(
            self.terrain,
            xz,
            kind,
            &mut fallback,
            &mut self.chain,
            &mut self.resources,
        );
        if fallback != before {
            self.query.clear_fallback_payload();
        }
        removed
    }
}

#[cfg(test)]
#[path = "marker_tests.rs"]
mod tests;
