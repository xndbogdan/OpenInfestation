//! 00436700 pending-effect walk. Instruction provenance is attached to each
//! phase; callbacks not implemented here are required live host operations.

use inf_formats::{FrontendExecutable, FrontendExecutableError, TerrainResourceLookup};
use thiserror::Error;

use super::{FurnitureEffectProgram, FurnitureEffectQueue};
use crate::terrain_runtime::{TerrainRuntime, TerrainSparsePayload, TerrainSparseRecordId};

const SET_REMOVAL_POLICY: u32 = 0x0043_6d00;
const CLEAR_FURNITURE: u32 = 0x0043_6e00;
const SET_FURNITURE_STATE: u32 = 0x0043_7650;
const TERMINATE: u32 = 0x0043_7750;
const START_FURNITURE_TIMER: u32 = 0x0043_7780;

/// Original file-backed program words. Unmapped addresses (including live
/// allocations) remain errors; a runtime-memory owner may supply them instead.
pub struct ExecutableFurnitureEffectPrograms<'a> {
    pub executable: FrontendExecutable<'a>,
}

impl ExecutableFurnitureEffectPrograms<'_> {
    pub fn word(&self, address: u32) -> Result<u32, FrontendExecutableError> {
        self.executable.word(address)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureCallbackContext {
    /// Mutable stack WORDs at 004368A5..AF. X/Z are the aligned queue key;
    /// Y is signed raw H12 height shifted to a low WORD, not interpolation.
    /// Changes by one callback are visible to later callbacks in this burst.
    pub position_words: [u16; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureExecutionPlacement<P> {
    pub identity: P,
    /// Signed H11 first WORD, read before the H8 preload gate.
    pub model_word: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureCallbackRequest<P> {
    pub callback_address: u32,
    pub argument: u32,
    /// 00436A11 reloads the fixed cached H11 identity into EDX for every
    /// callback. Changes to registrations or mutable stack position do not
    /// replace this by-value userdata within the current callback burst.
    pub placement: Option<P>,
}

pub trait FurnitureEffectExecutorHost {
    type Error;
    /// Stable identity of the actual retained H11 entry allocation. Callback
    /// userdata aliases this entry; a slot number alone is insufficient when
    /// resource registrations can change during a callback.
    type Placement: Copy;

    /// Live DWORD read: delay is signed; callback and argument keep raw bits.
    fn program_word(&mut self, address: u32) -> Result<u32, Self::Error>;
    /// H11 slot0 indexed by sparse lookup. None is a proven native null slot.
    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<FurnitureExecutionPlacement<Self::Placement>>, Self::Error>;
    /// 0043680C..38: skip indices beyond the native inclusive H8 bound or
    /// already cached; invoke the real loader only for a null valid entry.
    /// The executor calls this only for nonnegative signed H11 model WORDs.
    fn preload_model_if_required(&mut self, model_slot: u16) -> Result<(), Self::Error>;
    /// Shared mutable process record005483C4 for cells without flag4. These
    /// must use the same owner as other terrain consumers, not constant zero.
    fn fallback_sparse_payload(&mut self) -> Result<TerrainSparsePayload, Self::Error>;
    ///43773B/437868 store only state BYTE+2, retaining lookup and WORD+0.
    fn write_fallback_sparse_state(&mut self, state: u8) -> Result<(), Self::Error>;
    /// 00436EEF..EFB writes only005483C4 bytes+2 then+3 to zero, without reading
    /// the previous payload. A failure retains any completed byte write.
    fn clear_fallback_sparse_payload(&mut self) -> Result<(), Self::Error>;
    /// Required native callback. This can mutate the current receiver, stack
    /// position, terrain and RNG, and genuinely enqueue nested radius effects.
    /// Existing nodes must not be cleared/unlinked here (native known callbacks
    /// do not do that); the executor owns unlinking and its saved-next walk.
    fn callback(
        &mut self,
        queue: &mut FurnitureEffectQueue,
        record_index: usize,
        terrain: &mut TerrainRuntime,
        context: &mut FurnitureCallbackContext,
        request: FurnitureCallbackRequest<Self::Placement>,
        rng: &mut u32,
    ) -> Result<bool, Self::Error>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FurnitureExecutorOutcome {
    pub visited_records: usize,
    pub callbacks: usize,
    pub skipped_program_records: usize,
    pub removed_records: usize,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum FurnitureEffectExecutorError<E> {
    #[error("required furniture effect executor service failed")]
    Host(E),
    #[error("00436700 reached native null sparse record at {xz:?}")]
    NullSparseRecord { xz: [u16; 2] },
    #[error("00436700 program cursor wrapped to native null")]
    NullProgram,
    #[error("furniture callback removed its active queue record")]
    RemovedActiveRecord,
}

impl FurnitureEffectQueue {
    /// Raw microseconds from0048770C, before actor-manager+24. Failures retain
    /// reached timer/cursor, callback, resource and terrain effects. Admission
    /// and successful execution of an empty queue imply no missing service ran.
    pub fn tick<H: FurnitureEffectExecutorHost>(
        &mut self,
        terrain: &mut TerrainRuntime,
        microseconds: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<FurnitureExecutorOutcome, FurnitureEffectExecutorError<H::Error>> {
        let mut outcome = FurnitureExecutorOutcome::default();
        let mut index = 0;
        while index < self.records.len() {
            // 00436748 caches next before timers/callbacks. An append while
            // processing the original tail waits until the following tick.
            let had_successor = index + 1 < self.records.len();
            let record = &mut self.records[index];
            record.age_microseconds = record.age_microseconds.wrapping_add(microseconds);
            record.remaining_microseconds =
                if (record.remaining_microseconds as i32) > microseconds as i32 {
                    record.remaining_microseconds.wrapping_sub(microseconds)
                } else {
                    0
                };
            outcome.visited_records += 1;
            let removed = self.tick_record(index, terrain, host, rng, &mut outcome)?;
            if removed {
                self.records.remove(index);
                outcome.removed_records += 1;
            } else {
                index += 1;
            }
            if !had_successor {
                break;
            }
        }
        Ok(outcome)
    }

    fn tick_record<H: FurnitureEffectExecutorHost>(
        &mut self,
        index: usize,
        terrain: &mut TerrainRuntime,
        host: &mut H,
        rng: &mut u32,
        outcome: &mut FurnitureExecutorOutcome,
    ) -> Result<bool, FurnitureEffectExecutorError<H::Error>> {
        loop {
            // Native outer due test precedes sparse/H11/H8 operations.
            let record = &self.records[index];
            let delay = read(host, record.program.address())?;
            if (record.age_microseconds as i32) < delay as i32 {
                return Ok(false);
            }
            let key = record.aligned_key;
            let xz = [key as u16, (key >> 16) as u16];
            let placement = match terrain.sparse_record_at(xz) {
                Some((_, record)) if record.resource_lookup.raw() != 0 => {
                    let lookup = record.resource_lookup;
                    host.placement_slot(lookup)
                        .map_err(FurnitureEffectExecutorError::Host)?
                }
                _ => None,
            };
            if let Some(slot) = placement {
                if slot.model_word >= 0 {
                    host.preload_model_if_required(slot.model_word as u16)
                        .map_err(FurnitureEffectExecutorError::Host)?;
                }
            } else if self.records[index].removal_flag != 0 {
                return Ok(true);
            }
            // Native computes the raw-height stack snapshot only after the
            // sparse/resource/removal gates; original +8/+A/+C remain intact.
            let y = terrain.raw_height_at(xz) as u16;
            let mut context = FurnitureCallbackContext {
                position_words: [xz[0], y, xz[1]],
            };
            let cached_placement = placement.map(|slot| slot.identity);
            loop {
                let address = self.records[index].program.address();
                let delay = read(host, address)?;
                if (self.records[index].age_microseconds as i32) < delay as i32 {
                    break;
                }
                // 004368C8..D3 consumes only this delay, before state skipping
                // and before a required callback can fail.
                self.records[index].age_microseconds =
                    self.records[index].age_microseconds.wrapping_sub(delay);
                if read(host, address.wrapping_add(4))? == SET_FURNITURE_STATE {
                    let state = sparse_payload(terrain, host, context.position_words)?
                        .ok_or(FurnitureEffectExecutorError::NullSparseRecord {
                            xz: [context.position_words[0], context.position_words[2]],
                        })?
                        .1
                        .state
                        & 3;
                    loop {
                        let address = self.records[index].program.address();
                        let callback = read(host, address.wrapping_add(4))?;
                        if callback == 0 {
                            break;
                        }
                        if callback == SET_FURNITURE_STATE
                            && i32::from(state) < read(host, address.wrapping_add(8))? as i32
                        {
                            break;
                        }
                        advance(&mut self.records[index].program)?;
                        outcome.skipped_program_records += 1;
                    }
                }
                let address = self.records[index].program.address();
                let callback = read(host, address.wrapping_add(4))?;
                if callback == 0 {
                    return Ok(true);
                }
                let request = FurnitureCallbackRequest {
                    callback_address: callback,
                    argument: read(host, address.wrapping_add(8))?,
                    placement: cached_placement,
                };
                let keep = match callback {
                    SET_REMOVAL_POLICY => {
                        self.records[index].removal_flag = request.argument;
                        request.argument == 0 || cached_placement.is_some()
                    }
                    CLEAR_FURNITURE => {
                        clear_furniture(terrain, host, context.position_words)?;
                        true
                    }
                    SET_FURNITURE_STATE => {
                        if let Some((id, mut payload)) =
                            sparse_payload(terrain, host, context.position_words)?
                        {
                            payload.state = (payload.state & !3) | (request.argument as u8 & 3);
                            write_payload(terrain, host, id, payload)?;
                        }
                        true
                    }
                    TERMINATE => false,
                    START_FURNITURE_TIMER => {
                        if let Some((id, mut payload)) =
                            sparse_payload(terrain, host, context.position_words)?
                        {
                            payload.state |= 8;
                            write_payload(terrain, host, id, payload)?;
                            self.records[index].remaining_microseconds = 7_000_000;
                        }
                        true
                    }
                    _ => host
                        .callback(self, index, terrain, &mut context, request, rng)
                        .map_err(FurnitureEffectExecutorError::Host)?,
                };
                outcome.callbacks += 1;
                if index >= self.records.len() {
                    return Err(FurnitureEffectExecutorError::RemovedActiveRecord);
                }
                if !keep {
                    return Ok(true);
                }
                // The callback can change +18; advance its reached current
                // value, rather than a pre-call snapshot (00436A23).
                advance(&mut self.records[index].program)?;
            }
        }
    }
}

fn read<H: FurnitureEffectExecutorHost>(
    host: &mut H,
    address: u32,
) -> Result<u32, FurnitureEffectExecutorError<H::Error>> {
    host.program_word(address)
        .map_err(FurnitureEffectExecutorError::Host)
}

fn advance<E>(program: &mut FurnitureEffectProgram) -> Result<(), FurnitureEffectExecutorError<E>> {
    *program = FurnitureEffectProgram::new(program.address().wrapping_add(12))
        .ok_or(FurnitureEffectExecutorError::NullProgram)?;
    Ok(())
}

fn clear_furniture<H: FurnitureEffectExecutorHost>(
    terrain: &mut TerrainRuntime,
    host: &mut H,
    position: [u16; 3],
) -> Result<(), FurnitureEffectExecutorError<H::Error>> {
    let xz = [position[0], position[2]];
    if !terrain.cell_at(xz).has_sparse_record() {
        return host
            .clear_fallback_sparse_payload()
            .map_err(FurnitureEffectExecutorError::Host);
    }
    let (id, _) = terrain
        .sparse_record_at(xz)
        .ok_or(FurnitureEffectExecutorError::NullSparseRecord { xz })?;
    write_payload(
        terrain,
        host,
        Some(id),
        TerrainSparsePayload {
            state: 0,
            resource_lookup: TerrainResourceLookup::from_raw(0),
        },
    )
}

/// None id identifies the real process fallback record, not a missing record.
fn sparse_payload<H: FurnitureEffectExecutorHost>(
    terrain: &TerrainRuntime,
    host: &mut H,
    position: [u16; 3],
) -> Result<
    Option<(Option<TerrainSparseRecordId>, TerrainSparsePayload)>,
    FurnitureEffectExecutorError<H::Error>,
> {
    let xz = [position[0], position[2]];
    if !terrain.cell_at(xz).has_sparse_record() {
        return host
            .fallback_sparse_payload()
            .map(|payload| Some((None, payload)))
            .map_err(FurnitureEffectExecutorError::Host);
    }
    Ok(terrain.sparse_record_at(xz).map(|(id, record)| {
        (
            Some(id),
            TerrainSparsePayload {
                state: record.state,
                resource_lookup: record.resource_lookup,
            },
        )
    }))
}

fn write_payload<H: FurnitureEffectExecutorHost>(
    terrain: &mut TerrainRuntime,
    host: &mut H,
    id: Option<TerrainSparseRecordId>,
    payload: TerrainSparsePayload,
) -> Result<(), FurnitureEffectExecutorError<H::Error>> {
    if let Some(id) = id {
        terrain
            .write_sparse_payload(id, payload)
            .expect("record identity is retained without intervening callback");
        Ok(())
    } else {
        host.write_fallback_sparse_state(payload.state)
            .map_err(FurnitureEffectExecutorError::Host)
    }
}

#[cfg(test)]
mod tests;
