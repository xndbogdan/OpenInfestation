use std::num::NonZeroU32;

use thiserror::Error;

use crate::terrain_runtime::TerrainRuntime;

/// A nonnull native program allocation. This is a preferred virtual address,
/// not a portable function pointer, callback or offset into an overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureEffectProgram(NonZeroU32);

impl FurnitureEffectProgram {
    pub const fn new(address: u32) -> Option<Self> {
        match NonZeroU32::new(address) {
            Some(address) => Some(Self(address)),
            None => None,
        }
    }

    pub const fn address(self) -> u32 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureQueueRequest {
    /// Original unaligned low WORD coordinates retained at record +8/+C.
    pub xz: [u16; 2],
    pub program: FurnitureEffectProgram,
}

/// Reached fields of a 36-byte allocation queued by 00436A60. Linked-node
/// addresses belong to the Rust container; unwritten native padding is not
/// represented as initialized data. No program callback has run at admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedFurnitureEffect {
    /// WORDs +8/+A/+C: original X, current interpolated terrain Y, original Z.
    pub position_words: [u16; 3],
    /// DWORD +10: aligned X in low WORD and aligned Z in high WORD.
    pub aligned_key: u32,
    /// Wrapping DWORD +14; 0048770C passes raw world microseconds.
    pub age_microseconds: u32,
    pub program: FurnitureEffectProgram,
    pub removal_flag: u32,
    /// DWORD +20; signed comparison/decrement in 00436755..00436765.
    pub remaining_microseconds: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnitureQueueOutcome {
    Duplicate,
    Enqueued { index: usize },
}

#[derive(Debug, Error)]
#[error("00436A60 pending furniture effect allocation failed")]
pub struct FurnitureQueueError;

/// Process queue corresponding to 00546E08/00546E10. Preserve this across
/// world callbacks; 00436B80's explicit clear belongs to world teardown.
#[derive(Debug, Default)]
pub struct FurnitureEffectQueue {
    pub(super) records: Vec<QueuedFurnitureEffect>,
}

impl FurnitureEffectQueue {
    pub fn records(&self) -> &[QueuedFurnitureEffect] {
        &self.records
    }

    /// Callback receiver (+14/+18/+1C/+20), addressed by the current executor
    /// index. Native effect callbacks may change these fields and enqueue new
    /// effects; they do not unlink existing queue records themselves.
    pub fn record_mut(&mut self, index: usize) -> Option<&mut QueuedFurnitureEffect> {
        self.records.get_mut(index)
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// 00436A60: scan duplicates before allocation/height query, retain the
    /// unaligned position, initialize reached timer/flag DWORDs, append last.
    pub fn enqueue(
        &mut self,
        terrain: &TerrainRuntime,
        request: FurnitureQueueRequest,
    ) -> Result<FurnitureQueueOutcome, FurnitureQueueError> {
        let mask = !(terrain.sampling().spacing.units() - 1);
        let [x, z] = request.xz;
        let aligned_key = u32::from(x & mask) | (u32::from(z & mask) << 16);
        if self
            .records
            .iter()
            .any(|record| record.aligned_key == aligned_key)
        {
            return Ok(FurnitureQueueOutcome::Duplicate);
        }
        self.records
            .try_reserve(1)
            .map_err(|_| FurnitureQueueError)?;
        let position_words = [x, terrain.height_at(request.xz) as u16, z];
        let index = self.records.len();
        self.records.push(QueuedFurnitureEffect {
            position_words,
            aligned_key,
            age_microseconds: 0,
            program: request.program,
            removal_flag: 0,
            remaining_microseconds: 0,
        });
        Ok(FurnitureQueueOutcome::Enqueued { index })
    }
}
