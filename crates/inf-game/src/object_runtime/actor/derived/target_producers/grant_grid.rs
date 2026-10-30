//! Selector12 periodic 00468E30 and shared terrain grant lookup 00436BC0.
//! The first H12 pointer supplies height; each grant lookup independently
//! acquires current H12 and, after the sparse gates, current H11. No RNG.

use inf_formats::{grant_is_owned, GrantClosureError, TerrainCell};
use thiserror::Error;

use super::super::{Family429e20Class, FamilyBehaviorProfile, FamilyPeriodicProbeRequest};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSpacing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector12GrantGridRequest {
    periodic: FamilyPeriodicProbeRequest,
}
impl Selector12GrantGridRequest {
    pub fn from_periodic(periodic: FamilyPeriodicProbeRequest) -> Option<Self> {
        if !matches!(periodic.profile, FamilyBehaviorProfile::Selector12 { .. })
            || periodic.index != 4
            || periodic.profile.periodic_record(periodic.index)? != periodic.record
        {
            return None;
        }
        Some(Self { periodic })
    }
    pub fn periodic(self) -> FamilyPeriodicProbeRequest {
        self.periodic
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyTerrainGrantRequest {
    pub actor: ObjectId,
    /// Captured before the mutable H12 acquisition. Native ECX/EDX use LOWWORD.
    pub xz: [u16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyGrantGridPointRequest {
    pub actor: ObjectId,
    /// Actual six-byte stack point; None is native pointer NULL.
    pub position_words: Option<[u16; 3]>,
    pub score: u32,
}

/// Required current resource/target services, with short terrain loans.
/// Production tokens authenticate the actual prepared registration/allocation;
/// cloning a decoded terrain is not a substitute for this identity.
pub trait FamilyGrantGridHost {
    type Error;
    type Terrain: Copy;
    /// Read cached H12/0 and perform its reached virtual+0 lookup when NULL.
    /// None is a genuine NULL result. The refresh-only call in 436BC0 ignores it.
    fn acquire_terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<Self::Terrain>, Self::Error>;
    fn terrain(&self, source: Self::Terrain) -> Result<&TerrainRuntime, Self::Error>;
    /// Read current 5372F0. The supplied domain is the loader's 128/256 values.
    fn spacing(&mut self) -> TerrainSpacing;
    /// Independently read current 5372F4, represented by its 7F/FF policy.
    fn fraction_mask(&mut self) -> TerrainSpacing;
    /// Independently read current 5372F8, represented by its 4/5 policy.
    fn height_scale(&mut self) -> TerrainHeightScale;
    /// Lazily acquire actual H11/0 and read BYTE +2 of the indexed four-byte row.
    /// All nonzero u8 lookups, including FD/FE/FF, are ordinary indices here.
    fn placement_kind(&mut self, lookup: u8) -> Result<u8, Self::Error>;
    /// Required WORD at 532F88 + kind*16 + C; no H8 acquisition.
    fn furniture_grant_id(&mut self, kind: u8) -> Result<u16, Self::Error>;
    /// Delegate to actual shared 427390/Target284 publisher. It owns retained
    /// allocation and the live process NULL-source read after allocation.
    fn publish_grid_point(
        &mut self,
        arena: &mut ObjectArena,
        request: FamilyGrantGridPointRequest,
    ) -> Result<bool, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyGrantGridError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Grant(#[from] GrantClosureError),
    #[error("actor {actor:?} requires its position at WORD +{offset:X}")]
    MissingPosition { actor: ObjectId, offset: u16 },
    #[error("required H12 backing at {phase}")]
    RequiredTerrain { phase: &'static str },
    #[error("H12 cell {index} outside {count} retained cells")]
    Cell { index: usize, count: usize },
    #[error("H12 bucket {index} outside {count} retained buckets")]
    Bucket { index: usize, count: usize },
    #[error("grant-grid host failed at {phase}")]
    Host { phase: &'static str, error: E },
}

fn position<E>(
    arena: &ObjectArena,
    actor: ObjectId,
    axis: usize,
) -> Result<u16, FamilyGrantGridError<E>> {
    Ok(arena
        .object(actor)?
        .base
        .position_words
        .ok_or(FamilyGrantGridError::MissingPosition {
            actor,
            offset: 0x32 + (axis as u16) * 2,
        })?[axis])
}

fn cell<H: FamilyGrantGridHost>(
    host: &H,
    source: Option<H::Terrain>,
    index: usize,
    phase: &'static str,
) -> Result<TerrainCell, FamilyGrantGridError<H::Error>> {
    let source = source.ok_or(FamilyGrantGridError::RequiredTerrain { phase })?;
    let terrain = host
        .terrain(source)
        .map_err(|error| FamilyGrantGridError::Host {
            phase: "retained H12 view",
            error,
        })?;
    terrain
        .cells()
        .get(index)
        .copied()
        .ok_or(FamilyGrantGridError::Cell {
            index,
            count: terrain.cells().len(),
        })
}

/// Complete 436BC0 on canonical H12 sorted unique buckets and loader policies.
/// The binary search is equivalent to its one-based power-of-two search within
/// that decoded domain; it never mutates bucket X/order/count.
pub fn terrain_furniture_grant<H: FamilyGrantGridHost>(
    arena: &mut ObjectArena,
    request: FamilyTerrainGrantRequest,
    host: &mut H,
) -> Result<u16, FamilyGrantGridError<H::Error>> {
    let source = host
        .acquire_terrain(arena, request.actor)
        .map_err(|error| FamilyGrantGridError::Host {
            phase: "436BC0 H12 acquisition",
            error,
        })?;
    let mask = u32::from(host.fraction_mask().units() - 1);
    let grid_mask = ((!(mask as i32)) >> 7) as u32;
    let [x, z] = request
        .xz
        .map(|word| ((u32::from(word) >> 7) & grid_mask) as u16);
    if !cell(
        host,
        source,
        usize::from(x) * 512 + usize::from(z),
        "436BC0 sparse marker",
    )?
    .has_sparse_record()
    {
        return Ok(0);
    }
    let source = source.expect("successful marker read required the actual H12 token");
    let terrain = host
        .terrain(source)
        .map_err(|error| FamilyGrantGridError::Host {
            phase: "436BC0 sparse bucket",
            error,
        })?;
    let buckets = terrain.buckets();
    let bucket = buckets
        .get(usize::from(z))
        .ok_or(FamilyGrantGridError::Bucket {
            index: usize::from(z),
            count: buckets.len(),
        })?;
    let Ok(index) = bucket.records.binary_search_by_key(&x, |record| record.x) else {
        return Ok(0);
    };
    let lookup = bucket.records[index].resource_lookup.raw();
    if lookup == 0 {
        return Ok(0);
    }
    // 436C9C..CB3 rechecks the cache; even a genuine getter NULL is ignored.
    let _ = host
        .acquire_terrain(arena, request.actor)
        .map_err(|error| FamilyGrantGridError::Host {
            phase: "436BC0 H12 refresh",
            error,
        })?;
    let kind = host
        .placement_kind(lookup)
        .map_err(|error| FamilyGrantGridError::Host {
            phase: "436BC0 H11 row",
            error,
        })?;
    host.furniture_grant_id(kind)
        .map_err(|error| FamilyGrantGridError::Host {
            phase: "436BC0 furniture grant WORD",
            error,
        })
}

impl ObjectArena {
    /// Full 468E30 scan and publisher cadence. Supported spacing/mask policy
    /// projects native upper-register carries to LOWWORD without changing counts
    /// or any consumer coordinate. It does not admit arbitrary corrupted globals.
    pub fn produce_selector12_grant_grid<H: FamilyGrantGridHost>(
        &mut self,
        actor: ObjectId,
        _request: Selector12GrantGridRequest,
        host: &mut H,
    ) -> Result<bool, FamilyGrantGridError<H::Error>> {
        let height_source =
            host.acquire_terrain(self, actor)
                .map_err(|error| FamilyGrantGridError::Host {
                    phase: "468E30 initial H12",
                    error,
                })?;
        let origin_x = u32::from(position(self, actor, 0)?);
        let initial_step = u32::from(host.spacing().units());
        let origin_z = u32::from(position(self, actor, 2)?);
        let alignment = !u32::from(host.fraction_mask().units() - 1);
        let x_start = origin_x.wrapping_add(initial_step).wrapping_sub(1201) & alignment;
        let z_start = origin_z.wrapping_add(initial_step).wrapping_sub(1201) & alignment;
        let z_width = (origin_z.wrapping_add(1200) & alignment).wrapping_sub(z_start) & 0xffff;
        let z_count = z_width / u32::from(host.spacing().units()) + 1;
        let x_width = (origin_x.wrapping_add(1200) & alignment).wrapping_sub(x_start) & 0xffff;
        let mut last_step = u32::from(host.spacing().units());
        let x_count = x_width / last_step + 1;
        let mut x = x_start;
        let mut best = 0_i32;
        let mut best_point = None;
        for _ in 0..x_count {
            let mut z = z_start;
            for _ in 0..z_count {
                let grant = terrain_furniture_grant(
                    self,
                    FamilyTerrainGrantRequest {
                        actor,
                        xz: [x as u16, z as u16],
                    },
                    host,
                )?;
                if grant != 0 {
                    let Family429e20Class::Selector12(state) =
                        &self.family429e20_actor(actor)?.class
                    else {
                        return Err(ObjectError::NotSelector12(actor).into());
                    };
                    if !grant_is_owned(&state.grants_24c.mask_04, u32::from(grant))? {
                        let mask = u32::from(host.fraction_mask().units() - 1);
                        let alignment = !mask;
                        let point_x = (x & alignment) as u16;
                        let point_z = (z & alignment) as u16;
                        let dx = i32::from(point_x.wrapping_sub(position(self, actor, 0)?) as i16);
                        let grid_mask = ((!(mask as i32)) >> 7) as u32;
                        let index = (((u32::from(point_x) >> 7) & grid_mask) * 512
                            + ((u32::from(point_z) >> 7) & grid_mask))
                            as usize;
                        let height_byte =
                            cell(host, height_source, index, "468E30 captured height")?.height_i8();
                        let shift = match host.height_scale() {
                            TerrainHeightScale::Units16 => 4,
                            TerrainHeightScale::Units32 => 5,
                        };
                        let point_y = (i32::from(height_byte) << shift) as u16;
                        let dy = i32::from(point_y.wrapping_sub(position(self, actor, 1)?) as i16);
                        let dz = i32::from(point_z.wrapping_sub(position(self, actor, 2)?) as i16);
                        let squared = [dx, dy, dz].into_iter().fold(0_u32, |sum, delta| {
                            sum.wrapping_add(delta.wrapping_mul(delta) as u32)
                        });
                        let score = self.family_radius_score(actor, squared)? as i32;
                        // Signed strict maximum preserves first candidate on ties.
                        if score > best {
                            best = score;
                            best_point = Some([point_x, point_y, point_z]);
                        }
                    }
                }
                // Even zero/owned/no-improvement paths reread 5372F0 here.
                last_step = u32::from(host.spacing().units());
                z = z.wrapping_add(last_step);
            }
            x = x.wrapping_add(last_step);
        }
        let found = best != 0;
        let score = if found {
            // Native radius is freshly read before signed SAR of positive best.
            let radius = self.derived_actor(actor)?.word_164 as u32;
            ((best >> 1) as u32).wrapping_add(radius.wrapping_mul(2))
        } else {
            0
        };
        let _ = host
            .publish_grid_point(
                self,
                FamilyGrantGridPointRequest {
                    actor,
                    position_words: best_point,
                    score,
                },
            )
            .map_err(|error| FamilyGrantGridError::Host {
                phase: "427390 Target284 publisher",
                error,
            })?;
        Ok(found)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "grant_grid/native_guards.rs"]
mod native_guards;
