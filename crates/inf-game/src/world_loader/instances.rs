//! Retained H14 identity and command image, shared by placements and live spawns.

use std::sync::Arc;

use crate::{
    object_runtime::InstanceCommandAllocation, world_placement::WorldPlacements, LoadedOverlay,
    ResidentRegistration, ResidentResource,
};
use inf_formats::PlacementInstanceAssets;

#[derive(Debug)]
pub(crate) struct WorldInstanceAllocation {
    source: Arc<LoadedOverlay>,
    registration: ResidentRegistration,
    world_offset: u32,
    placements: Vec<PlacementInstanceAssets>,
    pub allocation: Arc<InstanceCommandAllocation>,
}

impl WorldInstanceAllocation {
    pub(super) fn retain(
        placements: &WorldPlacements<'_>,
        allocation: Arc<InstanceCommandAllocation>,
    ) -> Self {
        let source = placements.world_source();
        Self {
            source: Arc::clone(source.overlay),
            registration: source.registration.clone(),
            world_offset: placements.world().allocation_offset,
            placements: placements.assets().instances[usize::from(source.registration.local_index)]
                .placements
                .clone(),
            allocation,
        }
    }

    pub fn is_current(&self, source: &ResidentResource<'_>) -> bool {
        self.registration == *source.registration && Arc::ptr_eq(&self.source, source.overlay)
    }

    /// The world's placement count (H14 world `+38`), which `0048E170`
    /// bounds its index by.
    pub fn placement_count(&self) -> Result<u32, String> {
        let field = self
            .world_offset
            .checked_add(0x38)
            .ok_or("H14 placement count offset overflow")?;
        self.allocation
            .runtime_allocation()
            .dword(field)
            .map_err(|error| error.to_string())
    }

    /// 00480643/697/6A4/6B1 reread the same one-based H14 pointer field.
    pub fn placement_offset(&self, reference: u32) -> Result<u32, String> {
        let field = reference
            .checked_mul(4)
            .and_then(|offset| offset.checked_add(self.world_offset))
            .and_then(|offset| offset.checked_add(0x38))
            .ok_or("selector60 placement pointer offset overflow")?;
        if !self
            .allocation
            .relocations()
            .iter()
            .any(|entry| entry.field_offset == field)
        {
            return Err(format!(
                "selector60 H14 placement pointer {field:#X} lacks relocation"
            ));
        }
        self.allocation
            .runtime_allocation()
            .dword(field)
            .map_err(|error| error.to_string())
    }

    /// Commands keep their original source identity and mutable allocation.
    /// Only the captured suffix pointer selects metadata; commands themselves
    /// are still read lazily by the shared constructor from that live image.
    pub fn suffix(&self, offset: u32) -> Result<&PlacementInstanceAssets, String> {
        self.placements
            .iter()
            .find(|placement| placement.suffix_offset == offset)
            .ok_or_else(|| {
                format!("selector60 H14 suffix {offset:#X} has no owned command metadata")
            })
    }
}
