use inf_formats::{
    FrontendExecutable, FrontendExecutableError, MissionPlacementSlot, MissionPlacementTable,
    TerrainResourceLookup,
};
use thiserror::Error;

use super::{
    DamageResistance, FurnitureDamageResources, FurnitureDamageRule, FurnitureEffectProgram,
};

pub const FURNITURE_DAMAGE_TABLE_ADDRESS: u32 = 0x0053_2f88;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum FurnitureDamageResourceError {
    #[error("H11 slot 0 lookup {lookup} is outside {count} retained entries")]
    Placement { lookup: u8, count: usize },
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
}

/// Concrete H11/EXE resolver. H11 slot 0 is the caller's retained resource
/// registration; its raw second WORD is interpreted only at this consumer.
pub struct ExecutableFurnitureDamageResources<'a> {
    pub placement_table: &'a MissionPlacementTable,
    pub executable: FrontendExecutable<'a>,
}

impl FurnitureDamageResources for ExecutableFurnitureDamageResources<'_> {
    type Error = FurnitureDamageResourceError;

    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<MissionPlacementSlot>, Self::Error> {
        self.placement_table
            .slots
            .get(usize::from(lookup.raw()))
            .copied()
            .map(Some)
            .ok_or(FurnitureDamageResourceError::Placement {
                lookup: lookup.raw(),
                count: self.placement_table.slots.len(),
            })
    }

    fn damage_rule(
        &mut self,
        furniture_kind: u8,
        channel: u8,
    ) -> Result<FurnitureDamageRule, Self::Error> {
        let address = FURNITURE_DAMAGE_TABLE_ADDRESS + u32::from(furniture_kind) * 16;
        let resistance_address = self.executable.word(address)?;
        let threshold = self.executable.word(address + 4)? as i32;
        let program = FurnitureEffectProgram::new(self.executable.word(address + 8)?);
        let channel_address = resistance_address.wrapping_add(u32::from(channel) * 4);
        let source = self.executable.mapped(channel_address, 4)?;
        let resistance = DamageResistance {
            threshold: u16::from_le_bytes(source.bytes[..2].try_into().unwrap()),
            multiplier: u16::from_le_bytes(source.bytes[2..4].try_into().unwrap()),
        };
        Ok(FurnitureDamageRule {
            resistance,
            threshold,
            program,
        })
    }
}
