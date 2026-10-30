//! Selector 60 construction 00480800, retained command 00480B30, after-load
//! 00480BB0 and row lifetime 00480460/00480500/00480880. Update/spawning is
//! owned by the update child module over the same retained row allocations.

use std::sync::Arc;

use thiserror::Error;

use super::{DerivedActorClass, DerivedActorConstructionRequest};
use crate::object_runtime::{
    actor::{family40d800::FamilyLinkChange, ActorStateChangeRequest},
    allocation::{RuntimePointer, Selector60RowId},
    ActorConstructionError, ActorHandle, InstanceCommandAllocation, ObjectArena, ObjectError,
    ObjectId, RuntimeAllocationError,
};

mod update;
pub use update::{
    Selector60SpawnResult, Selector60SpawnRow, Selector60UpdateError, Selector60UpdateHost,
};

#[derive(Clone, Debug)]
pub struct RetainedSpawnList {
    allocation: Arc<InstanceCommandAllocation>,
    pub allocation_offset: u32,
}

impl RetainedSpawnList {
    pub fn allocation(&self) -> &Arc<InstanceCommandAllocation> {
        &self.allocation
    }

    fn count(&self) -> Result<i32, Selector60Error> {
        Ok(self
            .allocation
            .runtime_allocation()
            .dword(add(self.allocation_offset, 4)?)? as i32)
    }

    fn row_offset(&self, index: u32) -> Result<u32, Selector60Error> {
        let field = add(self.allocation_offset, 8)?;
        let value = self.allocation.runtime_allocation().dword(field)?;
        let main = self.allocation.main_payload();
        if !self.allocation.relocations().iter().any(|relocation| {
            relocation.field_offset == field
                && relocation.field_span.start == main.start + field as usize
                && relocation.field_span.end == main.start + field as usize + 4
                && relocation.target_offset == value
        }) {
            return Err(Selector60Error::MissingRowRelocation { field });
        }
        add(
            value,
            index
                .checked_mul(0x18)
                .ok_or(Selector60Error::OffsetOverflow)?,
        )
    }
}

impl PartialEq for RetainedSpawnList {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
            && self.allocation_offset == other.allocation_offset
    }
}
impl Eq for RetainedSpawnList {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector60State {
    /// Portable per-allocation containment; never an emulated native byte.
    pub update_stopped: bool,
    pub byte_184: u8,
    pub byte_188: u8,
    pub byte_189: u8,
    pub word_18a: u16,
    pub word_18c: u32,
    pub retained_190: Option<RetainedSpawnList>,
    pub word_194: u32,
    pub position_198: Option<[u16; 3]>,
    pub word_19e: u16,
    pub word_1a0: u16,
    pub word_1a4: u32,
    pub byte_1a8: u8,
}

impl Selector60State {
    fn new() -> Self {
        Self {
            update_stopped: false,
            byte_184: 0,
            byte_188: 0,
            byte_189: 0,
            word_18a: 1,
            word_18c: 1000,
            retained_190: None,
            word_194: 0,
            position_198: None,
            word_19e: 0,
            word_1a0: 0,
            word_1a4: 0,
            byte_1a8: 0,
        }
    }
}

/// Real 20-byte row state, plus owned native +8 handle storage. Padding
/// +2/+3 is unwritten and intentionally has no fabricated scalar value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector60Row {
    pub byte_00: u8,
    pub byte_01: u8,
    pub word_04: u32,
    pub handles_08: Vec<ActorHandle>,
    pub word_0c: u32,
    pub word_10: u32,
}

/// Allocations have independent heap lifetime. Overwriting a shared row
/// pointer leaks the old native row; dropping an object cannot reclaim it.
#[derive(Clone, Debug, Default)]
pub(in crate::object_runtime) struct Selector60Rows(Vec<Option<Selector60Row>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum Selector60Error {
    #[error("object {0:?} is not a selector-60 actor")]
    NotSelector60(ObjectId),
    #[error("selector-60 operation reached invalid object {0:?}")]
    InvalidObject(ObjectId),
    #[error("selector-60 row construction requires the arena actor registry")]
    MissingRegistry,
    #[error("selector 60 command 19 requires an owned command allocation")]
    MissingCommandOwner,
    #[error("selector-60 rows pointer field {field:#x} lacks its exact relocation")]
    MissingRowRelocation { field: u32 },
    #[error("selector-60 retained address arithmetic overflow")]
    OffsetOverflow,
    #[error("selector-60 signed capacity {0} requests unsupported negative-size allocation")]
    NegativeCapacity(i16),
    #[error("selector-60 row would store an initial actor handle outside zero capacity")]
    InitialHandleOutsideCapacity,
    #[error("selector-60 row {0:?} is foreign, absent or already freed")]
    UnknownRow(Selector60RowId),
    #[error(transparent)]
    Allocation(#[from] RuntimeAllocationError),
}

fn add(offset: u32, amount: u32) -> Result<u32, Selector60Error> {
    offset
        .checked_add(amount)
        .ok_or(Selector60Error::OffsetOverflow)
}

impl ObjectArena {
    /// 00480F40, selector 60's +98, for the link codes. Message 7 counts a
    /// link in DWORD +1A4, sets byte +1A8 and, in state 2, locks through
    /// 0047D670(0, +84, 3). Message 8 uncounts; the last link of a linked
    /// actor in state 3 unlocks through 0047D670(0, +84, 2).
    pub(in crate::object_runtime) fn selector60_link_message(
        &mut self,
        object: ObjectId,
        code: i32,
    ) -> Result<FamilyLinkChange, ObjectError> {
        let lifecycle = self.actor(object)?.word_c4 as i32;
        let source = self.actor(object)?.handle;
        let DerivedActorClass::Selector60(state) = &mut self.derived_actor_mut(object)?.class
        else {
            return Err(ObjectError::LinkMessageBoundary(object));
        };
        let request = |target_state| ActorStateChangeRequest {
            target_state,
            argument: 0,
            source,
        };
        match code {
            7 => {
                state.byte_1a8 = 1;
                state.word_1a4 = state.word_1a4.wrapping_add(1);
                if lifecycle == 2 && (state.word_1a4 as i32) > 0 {
                    return Ok(FamilyLinkChange::Lock(request(3)));
                }
            }
            8 => {
                state.word_1a4 = state.word_1a4.wrapping_sub(1);
                if (state.word_1a4 as i32) < 1 && state.byte_1a8 != 0 && lifecycle == 3 {
                    return Ok(FamilyLinkChange::Unlock(request(2)));
                }
            }
            _ => return Err(ObjectError::LinkMessageBoundary(object)),
        }
        Ok(FamilyLinkChange::None)
    }

    pub fn allocate_selector60_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_derived_actor(request, rng)?;
        self.derived_actor_mut(object)?.class =
            DerivedActorClass::Selector60(Box::new(Selector60State::new()));
        Ok(object)
    }

    pub fn selector60_actor(&self, object: ObjectId) -> Result<&Selector60State, Selector60Error> {
        match &self
            .derived_actor(object)
            .map_err(|_| Selector60Error::NotSelector60(object))?
            .class
        {
            DerivedActorClass::Selector60(state) => Ok(state),
            _ => Err(Selector60Error::NotSelector60(object)),
        }
    }

    fn selector60_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector60State, Selector60Error> {
        match &mut self
            .derived_actor_mut(object)
            .map_err(|_| Selector60Error::NotSelector60(object))?
            .class
        {
            DerivedActorClass::Selector60(state) => Ok(state),
            _ => Err(Selector60Error::NotSelector60(object)),
        }
    }

    /// 00480C80, selector 60's +B4, before the derived reaction 004283E0.
    /// With a retained spawn list +190 it clears +144 and switches on the
    /// list's first WORD: 0 never reacts, 2 reacts while byte +188 is set, 3
    /// while WORD +18A is zero and 4 while byte +189 is set. 1, any other
    /// value (a negative WORD compares above 4) and no list react.
    pub fn selector60_reacts_to_damage(
        &mut self,
        object: ObjectId,
    ) -> Result<bool, Selector60Error> {
        let Some(list) = self.selector60_actor(object)?.retained_190.clone() else {
            return Ok(true);
        };
        self.derived_actor_mut(object)
            .map_err(|_| Selector60Error::NotSelector60(object))?
            .word_144 = 0;
        let mode = list
            .allocation
            .runtime_allocation()
            .word(list.allocation_offset)? as i16;
        let state = self.selector60_actor(object)?;
        Ok(match mode {
            0 => false,
            2 => state.byte_188 != 0,
            3 => state.word_18a == 0,
            4 => state.byte_189 != 0,
            _ => true,
        })
    }

    /// The caller has already read code 19 and authenticated allocation
    /// custody. No inspection-time row snapshot drives runtime mutation.
    pub fn set_selector60_spawn_list(
        &mut self,
        object: ObjectId,
        allocation: &Arc<InstanceCommandAllocation>,
        operand_offset: u32,
    ) -> Result<(), Selector60Error> {
        self.selector60_actor(object)?;
        let retained = RetainedSpawnList {
            allocation: Arc::clone(allocation),
            allocation_offset: operand_offset,
        };
        // 00480B5D retains the address before even its first count read.
        // Later rejection preserves this retention and all earlier writes.
        self.selector60_actor_mut(object)?.retained_190 = Some(retained.clone());
        let image = allocation.runtime_allocation();
        let mut index = 0_u32;
        while index < retained.count()?.max(0) as u32 {
            let capacity = add(retained.row_offset(index)?, 6)?;
            if image.word(capacity)? == 0 {
                image.write_word(capacity, 1)?;
            }
            // 00480B86 reloads the retained header's pointer after the WORD
            // write. The following zero clears only the shared pointer slot;
            // an earlier allocated runtime row is not freed here.
            let pointer = add(retained.row_offset(index)?, 0x14)?;
            image.write_dword(pointer, 0)?;
            index += 1;
        }
        Ok(())
    }

    pub fn selector60_row(&self, id: Selector60RowId) -> Result<&Selector60Row, Selector60Error> {
        if id.arena != self.identity {
            return Err(Selector60Error::UnknownRow(id));
        }
        self.selector60_rows
            .0
            .get(id.index)
            .and_then(Option::as_ref)
            .ok_or(Selector60Error::UnknownRow(id))
    }

    pub fn selector60_live_rows(&self) -> usize {
        self.selector60_rows
            .0
            .iter()
            .filter(|row| row.is_some())
            .count()
    }

    fn construct_selector60_row(
        &mut self,
        retained: &RetainedSpawnList,
        row: u32,
    ) -> Result<Selector60RowId, Selector60Error> {
        let image = retained.allocation.runtime_allocation();
        let timer = image.word(add(row, 0x0a)?)? as i16 as i32 as u32;
        let capacity = image.word(add(row, 6)?)? as i16;
        if capacity < 0 {
            return Err(Selector60Error::NegativeCapacity(capacity));
        }
        let reference = image.dword(row)? as i32;
        let null = self
            .actor_handles()
            .ok_or(Selector60Error::MissingRegistry)?
            .null_handle();
        let handle = self
            .actor_handle_by_placement(reference)
            .map_err(|_| Selector60Error::MissingRegistry)?;
        // 0047AD90 resolves through the real registry, including its cache.
        let actor = self
            .resolve_actor_handle(handle)
            .map_err(|_| Selector60Error::MissingRegistry)?;
        let flags = image.byte(add(row, 0x10)?)?;
        if flags & 2 != 0 && actor.is_some() && capacity == 0 {
            return Err(Selector60Error::InitialHandleOutsideCapacity);
        }
        let mut handles = vec![null; capacity as usize];
        let mut live = 0;
        if let Some(actor) = actor {
            if flags & 2 != 0 {
                handles[0] = self
                    .actor_handle_by_placement(reference)
                    .map_err(|_| Selector60Error::MissingRegistry)?;
                live = 1;
            } else {
                self.dispose(actor)
                    .map_err(|_| Selector60Error::InvalidObject(actor))?;
            }
        }
        let id = Selector60RowId {
            arena: self.identity,
            index: self.selector60_rows.0.len(),
        };
        self.selector60_rows.0.push(Some(Selector60Row {
            byte_00: 0,
            byte_01: 0,
            word_04: timer,
            handles_08: handles,
            word_0c: live,
            word_10: 0,
        }));
        Ok(id)
    }

    /// Exact +34 body. The enclosing 00401320 wrapper publishes 00800000
    /// only after this returns. Missing retained input queues real disposal.
    pub fn initialize_selector60_after_load(
        &mut self,
        object: ObjectId,
    ) -> Result<(), Selector60Error> {
        let Some(retained) = self.selector60_actor(object)?.retained_190.clone() else {
            self.dispose(object)
                .map_err(|_| Selector60Error::InvalidObject(object))?;
            return Ok(());
        };
        let mut index = 0_u32;
        while index < retained.count()?.max(0) as u32 {
            let row = retained.row_offset(index)?;
            let id = self.construct_selector60_row(&retained, row)?;
            // Replacing a prior row pointer does not free that prior row.
            let pointer = add(retained.row_offset(index)?, 0x14)?;
            retained
                .allocation
                .runtime_allocation()
                .set_runtime_pointer(pointer, RuntimePointer::Selector60Row(id))?;
            index += 1;
        }
        self.owned_mut(object)
            .map_err(|_| Selector60Error::InvalidObject(object))?
            .state
            .base
            .flags |= 0x0100_0000;
        Ok(())
    }

    /// 00480880 visits the CURRENT shared row pointers before derived/shared
    /// actor destruction. Freed pointer slots remain dangling, as in native;
    /// a second alias's deletion returns an explicit error instead of a fake
    /// successful free. Prior frees are not rolled back on a later failure.
    pub(in crate::object_runtime) fn destroy_selector60_rows(
        &mut self,
        object: ObjectId,
    ) -> Result<(), Selector60Error> {
        let Some(retained) = self.selector60_actor(object)?.retained_190.clone() else {
            return Ok(());
        };
        let mut index = 0_u32;
        while index < retained.count()?.max(0) as u32 {
            let pointer = add(retained.row_offset(index)?, 0x14)?;
            if let Some(RuntimePointer::Selector60Row(id)) = retained
                .allocation
                .runtime_allocation()
                .runtime_pointer(pointer)?
            {
                self.selector60_row(id)?;
                self.selector60_rows.0[id.index].take();
            }
            index += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector60_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "selector60_native_tests.rs"]
mod native_tests;
