//! Original 004657E0 frame addressing. Tables and frames remain EXE data.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParticleAnimationError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("particle animation table {table:08X} has zero frames")]
    ZeroFrames { table: u32 },
}

pub fn frame_address(
    executable: FrontendExecutable<'_>,
    table: u32,
    lifetime: u16,
) -> Result<u32, ParticleAnimationError> {
    let bytes = executable.mapped(table, 8)?.bytes;
    let base = u32::from_le_bytes(bytes[..4].try_into().expect("DWORD"));
    let frames = u16::from_le_bytes(bytes[4..6].try_into().expect("WORD"));
    if frames == 0 {
        return Err(ParticleAnimationError::ZeroFrames { table });
    }
    let rate = u16::from_le_bytes(bytes[6..8].try_into().expect("WORD"));
    let index = ((u32::from(rate) * u32::from(lifetime)) >> 14) % u32::from(frames);
    Ok(base.wrapping_sub(index * 10))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleAnimationFrame {
    pub sprite_slot: i16,
    pub light_amount: i16,
    pub light_rgb: [i8; 3],
    pub scale: i16,
}

impl ParticleAnimationFrame {
    pub fn read(
        executable: FrontendExecutable<'_>,
        address: u32,
    ) -> Result<Self, FrontendExecutableError> {
        let bytes = executable.mapped(address, 10)?.bytes;
        Ok(Self {
            sprite_slot: i16::from_le_bytes(bytes[..2].try_into().expect("WORD")),
            light_amount: i16::from_le_bytes(bytes[2..4].try_into().expect("WORD")),
            light_rgb: [bytes[4] as i8, bytes[5] as i8, bytes[6] as i8],
            scale: i16::from_le_bytes(bytes[8..10].try_into().expect("WORD")),
        })
    }
}
