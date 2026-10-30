//! Retained horizontal sampling globals 005372F0/005372F4.
//!
//! The PE initializer is coarse. 00450350's seven direct callers are temporary
//! fine scopes, each paired with 00450370; it is not a startup constructor.
//! World loading retains this owner unless H14 forces coarse. Screen callbacks
//! subsequently select and prepare the same mutable H12 cells.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::{TerrainRuntime, TerrainSpacing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessTerrainPolicy {
    spacing: TerrainSpacing,
}

#[derive(Debug, Error)]
pub enum TerrainPolicyError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("unsupported native terrain startup pair {spacing:#x}/{mask:#x}")]
    StartupPair { spacing: u32, mask: u32 },
    #[error("004506A0 requires the current mutable H12 terrain after sampling publication")]
    MissingTerrain,
    #[error("00440B20 requires the current Settings_GetPtr +70 word")]
    MissingDetailSetting,
}

impl ProcessTerrainPolicy {
    pub fn native_startup(executable: FrontendExecutable<'_>) -> Result<Self, TerrainPolicyError> {
        let bytes = executable.mapped(0x0053_72f0, 8)?.bytes;
        let spacing = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let mask = u32::from_le_bytes(bytes[4..].try_into().unwrap());
        let spacing = match (spacing, mask) {
            (128, 127) => TerrainSpacing::Units128,
            (256, 255) => TerrainSpacing::Units256,
            _ => return Err(TerrainPolicyError::StartupPair { spacing, mask }),
        };
        Ok(Self { spacing })
    }

    /// An explicitly supplied process boundary, for imported/synthetic owners.
    pub const fn supplied(spacing: TerrainSpacing) -> Self {
        Self { spacing }
    }

    pub const fn spacing(&self) -> TerrainSpacing {
        self.spacing
    }

    /// 0048DDA5: only flag2000 changes the horizontal pair during loading.
    /// The store remains reached even if a later H12 lookup fails.
    pub fn load_world(&mut self, world_flags: u32) -> TerrainSpacing {
        self.spacing = self.spacing.for_world(world_flags);
        self.spacing
    }

    /// Shared raw selection at 004502EA and 004503A1. Unlike the gameplay
    /// constructor's Boolean argument, values other than 0/1 retain the pair.
    fn select(&mut self, detail: u32, world_flags: Option<u32>) {
        let detail = if world_flags.is_some_and(|flags| flags & 0x2000 != 0) {
            0
        } else {
            detail
        };
        match detail {
            0 => self.spacing = TerrainSpacing::Units256,
            1 => self.spacing = TerrainSpacing::Units128,
            _ => {}
        }
    }

    /// 00450350 publishes only the pair; it never regenerates triangle bits.
    pub fn temporary_fine(&mut self, terrain: &mut TerrainRuntime) {
        self.spacing = TerrainSpacing::Units128;
        terrain.sampling.spacing = self.spacing;
    }

    /// 00450370 rereads the raw settings word, rather than restoring a saved
    /// pair. No preparation occurs, including when an invalid word retains fine.
    pub fn restore_detail(
        &mut self,
        detail: u32,
        world_flags: Option<u32>,
        terrain: &mut TerrainRuntime,
    ) {
        self.select(detail, world_flags);
        terrain.sampling.spacing = self.spacing;
    }
}

/// Real screen loan. None denotes the native absent H14/H12 return, not a
/// replacement world. H12 absence fails only after the pair has been published.
pub struct TerrainDetailContext<'a> {
    pub process: &'a mut ProcessTerrainPolicy,
    pub world_flags: Option<u32>,
    pub terrain: Option<&'a mut TerrainRuntime>,
}

impl TerrainDetailContext<'_> {
    /// 004502C0 selects first, then always runs 004506A0(0), even if unchanged.
    pub fn configure(&mut self, detail: u32) -> Result<(), TerrainPolicyError> {
        self.process.select(detail, self.world_flags);
        let terrain = self
            .terrain
            .as_deref_mut()
            .ok_or(TerrainPolicyError::MissingTerrain)?;
        terrain.sampling.spacing = self.process.spacing;
        terrain.prepare_diagonals();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
