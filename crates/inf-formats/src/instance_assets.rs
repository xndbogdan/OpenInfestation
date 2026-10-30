//! Handler-14 instance command graphs consumed by 00479E10 after class initialization.
//! Counts and pointers are authored data, not evidence that commands executed.

use std::collections::BTreeSet;

use serde::Serialize;
use thiserror::Error;

use crate::{parse_world_bank, ByteSpan, HandlerRegion, WorldAssetError, WorldBank};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InstanceField<T> {
    pub span: ByteSpan,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstanceAssetBank {
    /// Complete allocation and relocation provenance, including unknown suffix bytes.
    pub worlds: WorldBank,
    /// Same local world registration order as `worlds.entries`.
    pub instances: Vec<WorldInstanceAssets>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorldInstanceAssets {
    pub world_local_index: u32,
    /// Same order and aliases as the world's placement array.
    pub placements: Vec<PlacementInstanceAssets>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlacementInstanceAssets {
    pub placement_index: u32,
    pub suffix_offset: u32,
    /// Native 00479E10 skips the walk when this signed DWORD is <= 0.
    pub count: InstanceField<i32>,
    pub command_pointer_span: ByteSpan,
    pub commands: Vec<InstanceCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstanceCommand {
    /// Authored order in this placement's command array; aliases stay separate.
    pub index: u32,
    pub pointer_span: ByteSpan,
    pub allocation_offset: u32,
    /// Virtual +4 command selector, not a constructor selector or resource slot.
    pub code: InstanceField<i32>,
    /// Address passed as the second virtual +4 argument. Extent is unproven.
    pub payload_offset: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstanceAssetError {
    #[error(transparent)]
    World(#[from] WorldAssetError),
    #[error("instance allocation {span:?} is outside the {input_len}-byte source")]
    InvalidAllocation { span: ByteSpan, input_len: usize },
    #[error("{context} at allocation offset {offset:#x} needs {length} bytes, allocation has {allocation_bytes}")]
    OutOfBounds {
        context: &'static str,
        offset: u32,
        length: usize,
        allocation_bytes: usize,
    },
    #[error("instance command pointer field {0:#x} is absent from the native relocation table")]
    MissingCommandRelocation(u32),
    #[error("integer overflow while reading {0}")]
    IntegerOverflow(&'static str),
}

pub fn parse_world_instance_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<InstanceAssetBank, InstanceAssetError> {
    let worlds = parse_world_bank(source, region)?;
    let allocation = InstanceAllocation::new(source, worlds.main_payload)?;
    let relocation_fields: BTreeSet<_> =
        worlds.relocations.iter().map(|r| r.field_offset).collect();
    let mut instances = Vec::with_capacity(worlds.entries.len());
    for world in &worlds.entries {
        let mut placements = Vec::with_capacity(world.placements.len());
        for placement in &world.placements {
            let suffix_offset = placement.payload_offset;
            let count = allocation.i32(suffix_offset, "instance command count")?;
            let walked_count = count.value.max(0) as usize;
            let pointer_bytes =
                walked_count
                    .checked_mul(4)
                    .ok_or(InstanceAssetError::IntegerOverflow(
                        "instance command table",
                    ))?;
            let pointer_offset = add(suffix_offset, 4, "instance command table")?;
            let command_pointer_span = allocation.range(
                pointer_offset,
                pointer_bytes,
                "instance command pointer table",
            )?;
            let mut commands = Vec::with_capacity(walked_count);
            for index in 0..walked_count {
                let field_offset = add(pointer_offset, index * 4, "instance command pointer")?;
                if !relocation_fields.contains(&field_offset) {
                    return Err(InstanceAssetError::MissingCommandRelocation(field_offset));
                }
                let pointer = allocation.u32(field_offset, "instance command pointer")?;
                let code = allocation.i32(pointer.value, "instance command code")?;
                commands.push(InstanceCommand {
                    index: index as u32,
                    pointer_span: pointer.span,
                    allocation_offset: pointer.value,
                    code,
                    payload_offset: add(pointer.value, 4, "instance command payload")?,
                });
            }
            placements.push(PlacementInstanceAssets {
                placement_index: placement.index,
                suffix_offset,
                count,
                command_pointer_span,
                commands,
            });
        }
        instances.push(WorldInstanceAssets {
            world_local_index: world.local_index,
            placements,
        });
    }
    Ok(InstanceAssetBank { worlds, instances })
}

pub(crate) fn add(
    offset: u32,
    length: usize,
    context: &'static str,
) -> Result<u32, InstanceAssetError> {
    let length = u32::try_from(length).map_err(|_| InstanceAssetError::IntegerOverflow(context))?;
    offset
        .checked_add(length)
        .ok_or(InstanceAssetError::IntegerOverflow(context))
}

pub(crate) struct InstanceAllocation<'a> {
    main_payload: ByteSpan,
    bytes: &'a [u8],
}

impl<'a> InstanceAllocation<'a> {
    pub(crate) fn new(
        source: &'a [u8],
        main_payload: ByteSpan,
    ) -> Result<Self, InstanceAssetError> {
        let bytes = main_payload
            .get(source)
            .ok_or(InstanceAssetError::InvalidAllocation {
                span: main_payload,
                input_len: source.len(),
            })?;
        Ok(Self {
            main_payload,
            bytes,
        })
    }

    pub(crate) fn range(
        &self,
        offset: u32,
        length: usize,
        context: &'static str,
    ) -> Result<ByteSpan, InstanceAssetError> {
        let start = offset as usize;
        let end = start
            .checked_add(length)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(InstanceAssetError::OutOfBounds {
                context,
                offset,
                length,
                allocation_bytes: self.bytes.len(),
            })?;
        Ok(ByteSpan::new(
            self.main_payload.start + start,
            self.main_payload.start + end,
        ))
    }

    pub(crate) fn u16(
        &self,
        offset: u32,
        context: &'static str,
    ) -> Result<InstanceField<u16>, InstanceAssetError> {
        let span = self.range(offset, 2, context)?;
        let start = offset as usize;
        let value = u16::from_le_bytes(self.bytes[start..start + 2].try_into().unwrap());
        Ok(InstanceField { span, value })
    }

    pub(crate) fn u32(
        &self,
        offset: u32,
        context: &'static str,
    ) -> Result<InstanceField<u32>, InstanceAssetError> {
        let span = self.range(offset, 4, context)?;
        let start = offset as usize;
        let value = u32::from_le_bytes(self.bytes[start..start + 4].try_into().unwrap());
        Ok(InstanceField { span, value })
    }

    pub(crate) fn i32(
        &self,
        offset: u32,
        context: &'static str,
    ) -> Result<InstanceField<i32>, InstanceAssetError> {
        let field = self.u32(offset, context)?;
        Ok(InstanceField {
            span: field.span,
            value: field.value as i32,
        })
    }
}

#[cfg(test)]
#[path = "instance_assets_tests.rs"]
mod tests;
