//! Shared process fallback005483C4 and lazy query storage0055F0DC.
//!
//! Supported PE loader-zero proof applies once. World/screen/residency/renderer
//! replacement and furniture queue cleanup retain these bytes. The genuine
//! reached47C1D0 entry owns lazy construction, not the earlier terrain gate.

use inf_formats::{FrontendExecutable, FrontendExecutableError};

use crate::{
    frontend_scene::ModelSurfaceTableOrigin,
    menu::MenuBasis,
    terrain_runtime::{TerrainSparsePayload, TerrainSparseRecordId},
    LoadedOverlay, ResidentRegistration,
};
use std::sync::{Arc, Weak};

/// Required explicit adapter:47C264 attempts to register a literalRET exit
/// callback and ignores EAX. This records that reached attempt only; CRT
/// vector/heap timing and allocator fidelity remain unowned.
#[derive(Clone, Copy, Debug)]
pub struct LiteralRetExitRegistrationApproximation;

/// Qualified546B84 record publication. A Weak loan never keeps a released H12
/// allocation alive or resolves an old pointer through a new world's terrain.
#[derive(Clone, Debug)]
pub enum FurnitureQueryRecord {
    NativeAddress(u32),
    Fallback,
    Sparse {
        registration: ResidentRegistration,
        allocation: Weak<LoadedOverlay>,
        record: TerrainSparseRecordId,
    },
}
impl PartialEq for FurnitureQueryRecord {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NativeAddress(a), Self::NativeAddress(b)) => a == b,
            (Self::Fallback, Self::Fallback) => true,
            (
                Self::Sparse {
                    registration: a,
                    allocation: b,
                    record: c,
                },
                Self::Sparse {
                    registration: d,
                    allocation: e,
                    record: f,
                },
            ) => a == d && Weak::ptr_eq(b, e) && c == f,
            _ => false,
        }
    }
}
impl Eq for FurnitureQueryRecord {}
impl FurnitureQueryRecord {
    pub(crate) fn sparse(
        registration: &ResidentRegistration,
        allocation: &Arc<LoadedOverlay>,
        record: TerrainSparseRecordId,
    ) -> Self {
        Self::Sparse {
            registration: registration.clone(),
            allocation: Arc::downgrade(allocation),
            record,
        }
    }
}

///Actual B+3C pointer state: loaderNULL then native4F2CA7 template alias.
///Selected entries belong to the shared table owner, not this context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FurnitureQueryTableAlias {
    NativeNull,
    Table(ModelSurfaceTableOrigin),
}

///Current B+64 raw source pointer, qualified to the captured H8 allocation.
///A retained Weak identity does not keep unloaded OVL data alive.
#[derive(Clone, Debug)]
pub(crate) struct AuxiliaryRawSource {
    pub registration: ResidentRegistration,
    pub allocation: Weak<LoadedOverlay>,
    pub vertices_span: inf_formats::ByteSpan,
}
impl PartialEq for AuxiliaryRawSource {
    fn eq(&self, other: &Self) -> bool {
        self.registration == other.registration
            && Weak::ptr_eq(&self.allocation, &other.allocation)
            && self.vertices_span == other.vertices_span
    }
}
impl Eq for AuxiliaryRawSource {}

const QUERY_BASIS_ADDRESSES: [u32; 5] = [
    0x0055_ef44,
    0x0055_efd0,
    0x0055_f004,
    0x0055_f038,
    0x0055_f06c,
];

/// One actual process owner. Cloning/default construction would obscure its
/// retained writer history, so neither operation is supplied.
#[derive(Debug, PartialEq, Eq)]
pub struct ProcessFurnitureQueryState {
    //Native record WORD+0, BYTE+2 state, BYTE+3 lookup; no owned writer
    //changes the first WORD. Keep all four physical bytes nonetheless.
    fallback: [u8; 4],
    //Zero physical storage is distinct from the later reached40D6A0 bases.
    query_init_mask: u8,
    query_bases: [MenuBasis; 5],
    ///B+08/+0C/+10, known PE loader-zero ONCE; query entry is a later writer.
    pair_origin: [i32; 3],
    ///B+3C identity only; never a copied selected projection mode.
    pair_table_alias: FurnitureQueryTableAlias,
    ///B+00/+04 last published actual allocation. Retired marks remain retained
    ///pointer history; no active view is inferred from an old descriptor.
    auxiliary_allocation: Option<crate::model_scratch::ModelScratchAllocation>,
    ///B+64 is loaderNULL then a qualified captured raw source.
    auxiliary_raw_source: Option<AuxiliaryRawSource>,
    ///Portable diagnostic of the named literalRET registration policy only.
    literal_ret_registration_attempted: bool,
    current_record: FurnitureQueryRecord,
    ///Actual55EF74 loaderNULL then546B48 publication, distinct from selected
    ///projection table+1C/+20 and retained B+3C projection alias history.
    pair_alias_is_546b48: bool,
    ///Actual 00435D50 pointer publication to one retained00546B88 basis.
    ///This file-backed process DWORD is independent of the queryB basis.
    probe_basis_address: u32,
}

impl ProcessFurnitureQueryState {
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        executable.loader_zero_data(0x0054_83c4, 4)?;
        executable.loader_zero_data(0x0055_f0dc, 1)?;
        executable.loader_zero_data(0x0055_ef74, 4)?;
        executable.loader_zero_data(0x0055_ef38, 12)?;
        executable.loader_zero_data(0x0055_ef6c, 4)?;
        executable.loader_zero_data(0x0055_ef30, 8)?;
        executable.loader_zero_data(0x0055_ef94, 4)?;
        for address in QUERY_BASIS_ADDRESSES {
            executable.loader_zero_data(address, 40)?;
        }
        Ok(Self {
            fallback: [0; 4],
            query_init_mask: 0,
            query_bases: [MenuBasis { words: [0; 10] }; 5],
            pair_origin: [0; 3],
            pair_table_alias: FurnitureQueryTableAlias::NativeNull,
            auxiliary_allocation: None,
            auxiliary_raw_source: None,
            literal_ret_registration_attempted: false,
            current_record: FurnitureQueryRecord::NativeAddress(executable.word(0x0054_6b84)?),
            pair_alias_is_546b48: false,
            probe_basis_address: executable.word(0x0054_6e14)?,
        })
    }

    /// Decoded view of the current shared state/lookup bytes. Collision and
    /// furniture effects borrow this same owner; a query does not reset it.
    pub fn fallback_payload(&self) -> TerrainSparsePayload {
        TerrainSparsePayload {
            state: self.fallback[2],
            resource_lookup: inf_formats::TerrainResourceLookup::from_raw(self.fallback[3]),
        }
    }

    pub fn query_init_mask(&self) -> u8 {
        self.query_init_mask
    }
    pub fn query_bases(&self) -> &[MenuBasis; 5] {
        &self.query_bases
    }
    pub fn literal_ret_registration_attempted(&self) -> bool {
        self.literal_ret_registration_attempted
    }
    pub fn current_record(&self) -> &FurnitureQueryRecord {
        &self.current_record
    }
    pub fn pair_alias_is_546b48(&self) -> bool {
        self.pair_alias_is_546b48
    }
    pub fn probe_basis_address(&self) -> u32 {
        self.probe_basis_address
    }
    pub(crate) fn publish_probe_basis(&mut self, state: u8) {
        self.probe_basis_address = 0x0054_6b88 + u32::from(state >> 4) * 40;
    }

    ///Auxiliary30 writes retained B+A0, independently of the current B+14 basis.
    pub(crate) fn publish_b_saved_basis(&mut self, basis: MenuBasis) {
        self.query_bases[1] = basis;
    }

    ///Fresh current B-origin read. The wrapper performs X/Y/Z subtraction
    ///without any mutation-capable callback between these reads.
    pub(crate) fn pair_origin(&self) -> [i32; 3] {
        self.pair_origin
    }
    #[cfg(test)]
    pub(crate) fn pair_table_alias(&self) -> FurnitureQueryTableAlias {
        self.pair_table_alias
    }

    ///Only when process B is the working context at4F2C9C/9F/A2 then4F2CA7. Not lazy47C initialization,
    ///not first ACTIVE preparation, not world/renderer/epoch replacement.
    pub(crate) fn publish_b_auxiliary_entry(&mut self) {
        self.pair_origin[0] = 0;
        self.pair_origin[1] = 0;
        self.pair_origin[2] = 0;
        self.pair_table_alias =
            FurnitureQueryTableAlias::Table(ModelSurfaceTableOrigin::TemplateDefault);
    }

    pub(crate) fn publish_b_auxiliary_allocation(
        &mut self,
        allocation: crate::model_scratch::ModelScratchAllocation,
    ) {
        self.auxiliary_allocation = Some(allocation);
    }
    pub(crate) fn publish_b_auxiliary_raw_source(&mut self, source: AuxiliaryRawSource) {
        self.auxiliary_raw_source = Some(source);
    }
    #[cfg(test)]
    pub(crate) fn auxiliary_allocation(
        &self,
    ) -> Option<&crate::model_scratch::ModelScratchAllocation> {
        self.auxiliary_allocation.as_ref()
    }
    #[cfg(test)]
    pub(crate) fn auxiliary_raw_source(&self) -> Option<&AuxiliaryRawSource> {
        self.auxiliary_raw_source.as_ref()
    }

    ///47C22C bit publication precedes five complete40D6A0 constructors;
    ///47C264 registration attempt follows them. With bit01set no stores run.
    pub(crate) fn initialize_query(&mut self, _: LiteralRetExitRegistrationApproximation) {
        if self.query_init_mask & 1 != 0 {
            return;
        }
        self.query_init_mask |= 1;
        for basis in &mut self.query_bases {
            *basis = MenuBasis::INITIAL;
        }
        self.literal_ret_registration_attempted = true;
    }
    ///All ten words at actual B+14, including the retained basis counter.
    pub(crate) fn publish_pair_basis(&mut self, basis: MenuBasis) {
        self.query_bases[0] = basis;
    }
    pub(crate) fn publish_pair_alias_546b48(&mut self) {
        self.pair_alias_is_546b48 = true;
    }
    pub(crate) fn publish_record(&mut self, record: FurnitureQueryRecord) {
        self.current_record = record;
    }

    ///43773B/437868 store only state BYTE+2. No stale lookup write is hidden
    ///behind a whole decoded-payload assignment.
    pub fn write_fallback_state(&mut self, state: u8) {
        self.fallback[2] = state;
    }

    ///436F8D publishes lookup before the first RNG/state mutation.
    pub(crate) fn write_fallback_lookup(&mut self, lookup: u8) {
        self.fallback[3] = lookup;
    }

    ///436EF8 then436EFB: clear state before lookup, without a previous read.
    ///The record WORD+0 and query storage survive this item operation.
    pub fn clear_fallback_payload(&mut self) {
        self.fallback[2] = 0;
        self.fallback[3] = 0;
    }
}

#[cfg(test)]
mod collision_tests;
#[cfg(test)]
mod tests;
