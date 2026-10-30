//! Shared actor constructor's terrain classification, 0047A8A7..0047A9F3.
//!
//! This runs before placement context initializes position. The caller must
//! supply observed or explicitly owned allocation WORDs, never substitute the
//! later placement position. Resource lookup and allocation custody are owned
//! by the caller; this policy preserves the native order of data consumption.

use inf_formats::native_basis::sine_q15;
use thiserror::Error;

use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime};

#[derive(Clone, Copy, Debug)]
pub struct ActorTerrainSurface<'a> {
    /// The current H12 grid and explicit process spacing/height policy.
    pub terrain: &'a TerrainRuntime,
    /// Signed DWORD at H12 +0, before the process coordinate shift.
    pub plane_height_fixed: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct ActorTerrainInputs<'a> {
    /// None is an actual H12/0 miss. Its real 004EFBF0 miss callback returns
    /// null, selecting flag 0x40000000 without reading the allocation position.
    pub terrain: Option<ActorTerrainSurface<'a>>,
    /// Current 005483BC, independent from the grid's signed-height scale.
    /// Native shifts mask this DWORD to five bits; it is not inferred here.
    pub coordinate_shift: u32,
    /// Constructor's captured 005471C0, consumed by the plane wave formula.
    pub clock: u32,
    /// Current H14/0 +4. Only bit 0x40 is consumed, and only after the early
    /// plane/cell tests. None is a missing resource, not disabled waves.
    pub world_flags: Option<u32>,
    pub sine_quarter: &'a [u16; 4096],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorTerrainClassification {
    Unchanged,
    Set20000000,
    Set40000000,
}

impl ActorTerrainClassification {
    /// The constructor ORs this mask; it never clears the opposite high bit
    /// or dirties the common low-nine-bit state because of classification.
    pub const fn flags_to_or(self) -> u32 {
        match self {
            Self::Unchanged => 0,
            Self::Set20000000 => 0x2000_0000,
            Self::Set40000000 => 0x4000_0000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ActorTerrainError {
    #[error("actor terrain classification requires the allocation's pre-context position WORDs")]
    UnknownAllocationPosition,
    #[error("actor terrain classification reached the missing H14 slot-0 flags read")]
    MissingWorldResource,
    /// 004156E0 dereferences the H12 miss callback's null result.
    #[error("the selector-2 vehicle constructor requires the current H12 surface")]
    MissingSurfaceForVehicle,
    /// 004793A9 lazily loads H12 instead of taking the constructor's miss arm.
    #[error("the world contact pass requires the resident H12 surface")]
    MissingContactSurface,
}

pub fn classify_actor_terrain(
    allocation_position: Option<[u16; 3]>,
    inputs: ActorTerrainInputs<'_>,
) -> Result<ActorTerrainClassification, ActorTerrainError> {
    let Some(surface) = inputs.terrain else {
        return Ok(ActorTerrainClassification::Set40000000);
    };
    let position = allocation_position.ok_or(ActorTerrainError::UnknownAllocationPosition)?;
    classify_extent(position, 32, surface, inputs)
}

/// 004791F0's classification at 0047938E..00479492: the constructor policy
/// with the actor's unsigned radius WORD +40 in place of its fixed 32. The
/// pass reads the current position, after any terrain response moved it. Its
/// ground-below-plane test is 0047F640 (00479413), which decides as the
/// constructor's 00437AF0/0047F600 reads do.
pub fn classify_actor_contact(
    position: [u16; 3],
    radius: u16,
    inputs: ActorTerrainInputs<'_>,
) -> Result<ActorTerrainClassification, ActorTerrainError> {
    let surface = inputs
        .terrain
        .ok_or(ActorTerrainError::MissingContactSurface)?;
    classify_extent(position, i32::from(radius), surface, inputs)
}

fn classify_extent(
    [x, y, z]: [u16; 3],
    extent: i32,
    surface: ActorTerrainSurface<'_>,
    inputs: ActorTerrainInputs<'_>,
) -> Result<ActorTerrainClassification, ActorTerrainError> {
    let y = i32::from(y as i16);
    let upper = y.wrapping_add(extent);
    let lower = y.wrapping_sub(extent);
    let plane = surface
        .plane_height_fixed
        .wrapping_shr(inputs.coordinate_shift);
    let margin = 512_i32.wrapping_shr(inputs.coordinate_shift.wrapping_sub(8));
    if upper + margin < plane {
        return Ok(ActorTerrainClassification::Set20000000);
    }
    if lower - margin > plane {
        return Ok(ActorTerrainClassification::Set40000000);
    }

    // 00437AF0's raw origin-cell test, then 0047F600's identical nearest
    // height read. Neither calls the triangle sampler 00450A30.
    let height_shift = match surface.terrain.sampling().height_scale {
        TerrainHeightScale::Units16 => 4,
        TerrainHeightScale::Units32 => 5,
    };
    let ground = i32::from(surface.terrain.cell_at([x, z]).height_i8()) << height_shift;
    if ground >= plane {
        return Ok(ActorTerrainClassification::Set40000000);
    }
    let flags = inputs
        .world_flags
        .ok_or(ActorTerrainError::MissingWorldResource)?;
    let height = i32::from(plane_height_at(
        [x, z],
        plane,
        ground as i16,
        inputs.clock,
        flags & 0x40 != 0,
        inputs.sine_quarter,
    ));
    Ok(if lower > height {
        ActorTerrainClassification::Set40000000
    } else if upper < height {
        ActorTerrainClassification::Set20000000
    } else {
        ActorTerrainClassification::Unchanged
    })
}

/// 00410F60's observable low AX. Its disabled/above-ground path returns the
/// plane directly. The enabled branch compares before the final WORD wrap.
/// Crate-visible for the world-load robustness probe and spawn-equivalence
/// analysis; duplicating this arithmetic outside its owner would drift.
pub(crate) fn plane_height_at(
    [x, z]: [u16; 2],
    plane: i32,
    ground: i16,
    clock: u32,
    waves: bool,
    sine_quarter: &[u16; 4096],
) -> i16 {
    let ground = i32::from(ground);
    if !waves || plane <= ground {
        return plane as i16;
    }
    let x = i32::from(x);
    let z = i32::from(z);
    let time = (clock as i32).wrapping_shl(6);
    let first_angle = time.wrapping_add(x * 12).wrapping_shl(1);
    let second_angle = ((x + z) << 5).wrapping_add(time.wrapping_mul(5));
    let third_angle = time.wrapping_add((z - x) * 6).wrapping_shl(3);
    let first = sine_q15(first_angle, sine_quarter);
    let second = sine_q15(second_angle, sine_quarter);
    let third = sine_q15(third_angle, sine_quarter);
    let wave = (((first + second) >> 5) + (third >> 6)) as i16;
    let depth = plane.wrapping_sub(ground).wrapping_add(512);
    let offset = (i32::from(wave).wrapping_mul(depth) >> 15) as i16;
    let candidate = plane.wrapping_add(i32::from(offset));
    candidate.max(ground) as i16
}

#[cfg(test)]
mod tests;
