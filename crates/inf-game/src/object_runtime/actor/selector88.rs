//! Selector 88's exact construction and retained command pointer,
//! 00443890/00443900. The later 00443930 rewrite of shared H14 data is gated.
//!
//! Allocation ownership preserves the prior decision matrix: shared commands
//! through 15 keep their own operand rules; code 16 requires exact source
//! custody but reads no operand and admits its one-past address; higher codes
//! reject. Repeated 16 replaces only that actor's retained pointer. Aliased
//! actors retain one owner, and equal bytes in a different allocation do not
//! establish identity. The owner now also carries a separate mutable image;
//! original source identity and the no-read code-16 rule are unchanged.

use std::sync::Arc;

use inf_formats::{
    AssetRelocation, ByteSpan, InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView,
    PlacementInstanceAssets,
};
use thiserror::Error;

mod intro;
pub use intro::{Selector88IntroError, Selector88IntroScript};

use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension,
};
use crate::object_runtime::{
    CameraFactoryInputs, ObjectArena, ObjectError, ObjectId, ObjectInstanceCommands,
    RuntimeAllocation, WorldLoadError,
};

/// One owned H14 command allocation. Construct once and share its Arc among
/// placements, so retained references keep allocation identity across actors.
/// Source offsets remain canonical file offsets; retained offsets are relative
/// to main_payload, matching native relocated command pointers.
#[derive(Debug)]
pub struct InstanceCommandAllocation {
    allocation: Arc<RuntimeAllocation>,
    relocations: Arc<[AssetRelocation]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum InstanceCommandAllocationError {
    #[error("instance command allocation span is outside its source")]
    SourceRange,
    #[error("instance suffix does not borrow this command allocation")]
    SourceIdentity,
    #[error("selector 88 command 16 requires an owned command allocation")]
    MissingOwner,
    #[error("selector 86 command 2 requires an owned command allocation")]
    MissingSelector86Owner,
    #[error("retained command offset {allocation_offset:#x} is outside its allocation")]
    RetainedOffset { allocation_offset: u32 },
}

impl InstanceCommandAllocation {
    pub fn new(
        source: Arc<[u8]>,
        main_payload: ByteSpan,
        relocations: Arc<[AssetRelocation]>,
    ) -> Result<Self, InstanceCommandAllocationError> {
        if source.get(main_payload.start..main_payload.end).is_none() {
            return Err(InstanceCommandAllocationError::SourceRange);
        }
        Ok(Self {
            allocation: Arc::new(
                RuntimeAllocation::with_relocations(source, main_payload, &relocations)
                    .map_err(|_| InstanceCommandAllocationError::SourceRange)?,
            ),
            relocations,
        })
    }

    pub fn source(&self) -> &[u8] {
        self.allocation.source()
    }

    pub fn main_payload(&self) -> ByteSpan {
        self.allocation.main_payload()
    }

    pub fn runtime_allocation(&self) -> &Arc<RuntimeAllocation> {
        &self.allocation
    }

    pub fn relocations(&self) -> &[AssetRelocation] {
        &self.relocations
    }

    pub fn commands<'a>(
        &'a self,
        placement: &'a PlacementInstanceAssets,
    ) -> ObjectInstanceCommands<'a> {
        ObjectInstanceCommands {
            placement,
            source: self.source(),
            main_payload: self.main_payload(),
            relocations: &self.relocations,
        }
    }

    /// Equal bytes from a different source do not establish pointer custody.
    /// Relocations are canonical metadata, compared by value; the allocation
    /// bytes and their main span must come from this owner.
    pub fn validate_commands(
        &self,
        commands: ObjectInstanceCommands<'_>,
    ) -> Result<(), InstanceCommandAllocationError> {
        if !std::ptr::eq(commands.source, self.source())
            || commands.main_payload != self.main_payload()
            || commands.relocations != self.relocations.as_ref()
        {
            return Err(InstanceCommandAllocationError::SourceIdentity);
        }
        Ok(())
    }
}

/// An operand address stored by 00443900, with no operand bytes consumed.
#[derive(Clone, Debug)]
pub struct RetainedInstanceCommand {
    allocation: Arc<InstanceCommandAllocation>,
    pub allocation_offset: u32,
}

impl RetainedInstanceCommand {
    pub(in crate::object_runtime) fn new(
        allocation: Arc<InstanceCommandAllocation>,
        allocation_offset: u32,
    ) -> Self {
        Self {
            allocation,
            allocation_offset,
        }
    }

    pub fn allocation(&self) -> &Arc<InstanceCommandAllocation> {
        &self.allocation
    }
}

impl PartialEq for RetainedInstanceCommand {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
            && self.allocation_offset == other.allocation_offset
    }
}

impl Eq for RetainedInstanceCommand {}

/// Only the six constructor-written fields have initial values. Unwritten
/// timers stay unknown, and the later helper-actor pointers start null.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector88State {
    pub word_138: Option<u32>,
    pub retained_13c: Option<RetainedInstanceCommand>,
    pub word_140: Option<u32>,
    pub word_144: u32,
    pub word_148: u32,
    pub word_14c: u32,
    pub word_150: Option<u32>,
    pub actor_154: Option<ObjectId>,
    pub actor_158: Option<ObjectId>,
}

impl Selector88State {
    fn new() -> Self {
        Self {
            word_138: None,
            retained_13c: None,
            word_140: None,
            word_144: 0,
            word_148: 0,
            word_14c: 0,
            word_150: None,
            actor_154: None,
            actor_158: None,
        }
    }
}

impl ObjectArena {
    /// Shared actor construction followed by 00443890's six DWORD writes.
    pub fn allocate_selector88(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector88(Box::new(Selector88State::new()));
        Ok(object)
    }

    pub fn selector88(&self, object: ObjectId) -> Result<&Selector88State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector88(state) => Ok(state),
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Derived(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::BehaviorResource(_) => Err(ObjectError::NotSelector88(object)),
        }
    }

    /// Exact virtual +04: shared commands through 15, then pointer retention
    /// for 16. Neither the retained header nor any pointed-to arrays are read.
    /// The enclosing command walker must validate that the decoded suffix
    /// borrows this allocation before calling with code 16.
    pub(in crate::object_runtime) fn apply_selector88_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
        allocation: Option<&Arc<InstanceCommandAllocation>>,
    ) -> Result<bool, ActorCommandError> {
        self.selector88(object)?;
        if command.target != InstanceCommandTarget::Constructor(88) {
            return Err(ActorCommandError::WrongTarget);
        }
        if command.code.value <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        if command.code.value != 16 {
            return Ok(false);
        }
        let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } = command.payload
        else {
            return Err(ActorCommandError::Payload(16));
        };
        let allocation = allocation.ok_or(InstanceCommandAllocationError::MissingOwner)?;
        // A decoded four-byte command proves its operand address, including
        // the one-past address. This checks custody without reading operands.
        if command.command_offset.checked_add(4) != Some(allocation_offset)
            || allocation_offset as usize
                > allocation.main_payload().end - allocation.main_payload().start
        {
            return Err(
                InstanceCommandAllocationError::RetainedOffset { allocation_offset }.into(),
            );
        }
        let ActorExtension::Selector88(state) = &mut self.actor_mut(object)?.extension else {
            unreachable!("selector checked before command")
        };
        state.retained_13c = Some(RetainedInstanceCommand {
            allocation: Arc::clone(allocation),
            allocation_offset,
        });
        Ok(true)
    }
}

/// Offsets of 00443930's three plain count/pointer pairs.
const REFERENCE_ARRAYS: [(u32, u32); 3] = [(0x14, 0x18), (0x1c, 0x20), (0x24, 0x28)];

impl ObjectArena {
    /// 00443930, selector 88's +34. Static byte 00535510 is an initialized
    /// 1 whose only code reference is this read, so only a null +13C disposes.
    /// Otherwise it clears 0054ECE4, binds the first player to this actor via
    /// 0046AE90 (result ignored), and rewrites retained H14 references in
    /// place through 0047F770: DWORD +10, then each signed count/pointer pair.
    /// Counts and array pointers are reread per row, as native rereads +13C.
    /// The last pair (+2C/+30) also clears referenced actors' flag 0x20 and
    /// disables their flag-1 membership through 00479F20(false).
    pub fn initialize_selector88_after_load(
        &mut self,
        object: ObjectId,
        camera: &mut CameraFactoryInputs<'_>,
    ) -> Result<(), WorldLoadError> {
        let Some(retained) = self.selector88(object)?.retained_13c.clone() else {
            self.dispose(object)?;
            return Ok(());
        };
        self.world_load.flag_54ece4 = false;
        if let Some(player) = self.first_player() {
            let handle = self.actor(object)?.handle;
            self.attach_player(player, handle, camera)?;
        }
        let owner = retained.allocation();
        let image = owner.runtime_allocation();
        let base = retained.allocation_offset;
        let at = |offset: u32| {
            base.checked_add(offset)
                .ok_or(WorldLoadError::OffsetOverflow)
        };
        let reference = image.dword(at(0x10)?)? as i32;
        let handle = self.selector88_reference_handle(reference)?.0;
        image.write_dword(at(0x10)?, handle)?;
        for (count, pointer) in REFERENCE_ARRAYS {
            let mut index = 0_u32;
            while (index as i32) < image.dword(at(count)?)? as i32 {
                let row = selector88_row(owner, at(pointer)?, index)?;
                let reference = image.dword(row)? as i32;
                let handle = self.selector88_reference_handle(reference)?.0;
                image.write_dword(row, handle)?;
                index += 1;
            }
        }
        let mut index = 0_u32;
        while (index as i32) < image.dword(at(0x2c)?)? as i32 {
            let row = selector88_row(owner, at(0x30)?, index)?;
            let reference = image.dword(row)? as i32;
            let (handle, actor) = self.selector88_reference_handle(reference)?;
            image.write_dword(row, handle)?;
            if let Some(actor) = actor {
                let base = &mut self.owned_mut(actor)?.state.base;
                let flags = base.flags & !0x20;
                crate::object_runtime::mark_flag_change(base, flags);
                self.set_flag1(actor, false)?;
            }
            index += 1;
        }
        Ok(())
    }

    /// 0047F770's actor, stored as its +84 or the global null handle.
    fn selector88_reference_handle(
        &self,
        reference: i32,
    ) -> Result<(u32, Option<ObjectId>), WorldLoadError> {
        let actor = self.find_actor_by_placement(reference);
        let handle = match actor {
            Some(actor) => self.actor(actor)?.handle,
            None => self
                .actor_handles()
                .ok_or(ObjectError::MissingActorRegistry)?
                .null_handle(),
        };
        Ok((handle.raw(), actor))
    }
}

/// A row of a retained array. The pointer field must carry its exact H14
/// relocation; its value is then an allocation-relative offset.
pub(super) fn selector88_row(
    owner: &InstanceCommandAllocation,
    field: u32,
    index: u32,
) -> Result<u32, WorldLoadError> {
    let value = owner.runtime_allocation().dword(field)?;
    let main = owner.main_payload();
    if !owner.relocations().iter().any(|relocation| {
        relocation.field_offset == field
            && relocation.field_span.start == main.start + field as usize
            && relocation.field_span.end == main.start + field as usize + 4
            && relocation.target_offset == value
    }) {
        return Err(WorldLoadError::MissingArrayRelocation { field });
    }
    index
        .checked_mul(4)
        .and_then(|delta| value.checked_add(delta))
        .ok_or(WorldLoadError::OffsetOverflow)
}

#[cfg(test)]
mod tests;
