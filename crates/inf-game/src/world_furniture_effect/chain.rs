//! 004373B0, a fence segment's chain, and 00436240, the enqueue it calls.
//!
//! Under the temporary fine spacing (00450350) 004373B0 walks the seven by
//! seven cells 0x80 apart around the callback's XYZ, X outer and Z inner from
//! -0x180 to 0x180. A cell without a record reads the process fallback record
//! 005483C4. Each lookup whose H11 entry has kind 0x0E (byte +2) gets
//! 00436240 at that point: offline, the record at that cell queues its kind's
//! program (the descriptor at 00532F88 + kind * 16, +8) through 00436A60.
//! 00450370 then rereads the detail setting, and AL is 1. The fence
//! segments' program 00532E30 calls it before clearing its own record, so one
//! destroyed segment takes the whole fence down. Its first lookup at the XYZ
//! (00435260 under the current spacing) is a search whose result goes unused.

use inf_formats::{FrontendExecutable, TerrainResourceLookup};

use super::{
    resident::resident_placement_table, FurnitureEffectProgram, FurnitureEffectQueue,
    FurnitureQueueRequest, FURNITURE_DAMAGE_TABLE_ADDRESS,
};
use crate::{
    terrain_runtime::{ProcessTerrainPolicy, TerrainRuntime, TerrainSparsePayload},
    ResidentRuntime,
};

/// The furniture kind 004373B0 chains to: the fence segments.
pub const FENCE_KIND: u8 = 0x0e;
/// The walk's reach either side, in steps of 0x80.
const REACH: i16 = 0x180;
const STEP: i16 = 0x80;

/// The world's loans for the chain: the spacing owner the fine scope
/// borrows, settings +70 and the world flags 00450370 rereads, and the
/// network flag 0055F448 that 00436240 tests.
pub struct FurnitureChainLoan<'a> {
    pub policy: &'a mut ProcessTerrainPolicy,
    pub detail: u32,
    pub world_flags: Option<u32>,
    pub network_active: bool,
}

/// H11 slot 0's kind byte and the kind descriptors' programs.
pub trait FurnitureChainResources {
    type Error;
    /// Byte +2 of the H11 entry `lookup` indexes: its second WORD's low byte.
    fn kind(&mut self, lookup: TerrainResourceLookup) -> Result<u8, Self::Error>;
    /// The kind descriptor's DWORD +8, None when zero.
    fn program(&mut self, kind: u8) -> Result<Option<FurnitureEffectProgram>, Self::Error>;
}

/// 004373B0. `fallback` is the process record 005483C4 as the walk finds it.
pub fn run_furniture_chain<R: FurnitureChainResources>(
    queue: &mut FurnitureEffectQueue,
    terrain: &mut TerrainRuntime,
    position_words: [u16; 3],
    fallback: TerrainSparsePayload,
    loan: &mut FurnitureChainLoan<'_>,
    resources: &mut R,
) -> Result<bool, FurnitureChainError<R::Error>> {
    loan.policy.temporary_fine(terrain);
    let walked = walk(queue, terrain, position_words, fallback, loan, resources);
    // 0043758E restores the pair before the return; nothing ends the walk
    // early natively, so a failure here is only a port boundary.
    loan.policy
        .restore_detail(loan.detail, loan.world_flags, terrain);
    walked.map(|()| true)
}

fn walk<R: FurnitureChainResources>(
    queue: &mut FurnitureEffectQueue,
    terrain: &mut TerrainRuntime,
    [x, _, z]: [u16; 3],
    fallback: TerrainSparsePayload,
    loan: &FurnitureChainLoan<'_>,
    resources: &mut R,
) -> Result<(), FurnitureChainError<R::Error>> {
    let mut dx = -REACH;
    while dx <= REACH {
        let mut dz = -REACH;
        while dz <= REACH {
            let xz = [x.wrapping_add(dx as u16), z.wrapping_add(dz as u16)];
            let lookup = if terrain.cell_at(xz).has_sparse_record() {
                // 00437515 is reached only without the flag; a flagged cell
                // whose bucket lacks the record reads byte +3 of NULL.
                terrain
                    .sparse_record_at(xz)
                    .ok_or(FurnitureChainError::FlaggedCellWithoutRecord { xz })?
                    .1
                    .resource_lookup
            } else {
                fallback.resource_lookup
            };
            if lookup.raw() != 0
                && resources
                    .kind(lookup)
                    .map_err(FurnitureChainError::Resource)?
                    == FENCE_KIND
            {
                queue_furniture_program(queue, terrain, xz, loan.network_active, resources)?;
            }
            dz += STEP;
        }
        dx += STEP;
    }
    Ok(())
}

/// 00436240(x, z): offline, the record at that cell queues its kind's
/// program. A cell without the flag or the record, a zero lookup and a kind
/// without a program queue nothing. Returns whether a record was queued.
pub fn queue_furniture_program<R: FurnitureChainResources>(
    queue: &mut FurnitureEffectQueue,
    terrain: &TerrainRuntime,
    xz: [u16; 2],
    network_active: bool,
    resources: &mut R,
) -> Result<bool, FurnitureChainError<R::Error>> {
    if network_active || !terrain.cell_at(xz).has_sparse_record() {
        return Ok(false);
    }
    let Some((_, record)) = terrain.sparse_record_at(xz) else {
        return Ok(false);
    };
    if record.resource_lookup.raw() == 0 {
        return Ok(false);
    }
    let kind = resources
        .kind(record.resource_lookup)
        .map_err(FurnitureChainError::Resource)?;
    let Some(program) = resources
        .program(kind)
        .map_err(FurnitureChainError::Resource)?
    else {
        return Ok(false);
    };
    let outcome = queue.enqueue(terrain, FurnitureQueueRequest { xz, program })?;
    Ok(matches!(
        outcome,
        super::FurnitureQueueOutcome::Enqueued { .. }
    ))
}

#[derive(Debug, thiserror::Error)]
pub enum FurnitureChainError<E> {
    #[error("furniture chain resource failed: {0:?}")]
    Resource(E),
    #[error("cell {xz:?} carries the sparse flag without a record (004374F5 reads NULL)")]
    FlaggedCellWithoutRecord { xz: [u16; 2] },
    #[error(transparent)]
    Queue(#[from] super::FurnitureQueueError),
}

/// The resident H11 table and the executable's kind descriptors.
pub struct ResidentFurnitureChainResources<'a> {
    pub resident: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'a>,
}

impl FurnitureChainResources for ResidentFurnitureChainResources<'_> {
    type Error = String;
    fn kind(&mut self, lookup: TerrainResourceLookup) -> Result<u8, String> {
        let table = resident_placement_table(self.resident).map_err(|error| error.to_string())?;
        table
            .slots
            .get(usize::from(lookup.raw()))
            .map(|slot| slot.parameter_word as u8)
            .ok_or_else(|| {
                format!(
                    "H11 lookup {} is outside {} entries",
                    lookup.raw(),
                    table.slots.len()
                )
            })
    }
    fn program(&mut self, kind: u8) -> Result<Option<FurnitureEffectProgram>, String> {
        self.executable
            .word(FURNITURE_DAMAGE_TABLE_ADDRESS + u32::from(kind) * 16 + 8)
            .map(FurnitureEffectProgram::new)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;
