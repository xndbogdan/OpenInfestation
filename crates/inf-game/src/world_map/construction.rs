//! Complete known-source 00471AE0 and its48DE41 publication. Successful
//! logical allocations are retained before reaching resource/list boundaries.
//! No malloc-NULL, physical pointer fault, callback cadence or whole48E890
//! claim is attached to these typed phases.

use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use super::{
    markers::{LocalId, MarkerPool},
    MarkerStorageError, WorldMapState,
};
use crate::{
    terrain_map::{TerrainMap, TerrainMapPlane},
    terrain_runtime::TerrainRuntime,
    world_actor_terrain::WorldTerrainSource,
    ResidentResource, ResidentRuntime,
};

static NEXT_PROCESS_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldMapId {
    process: u64,
    allocation: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum WorldMapLookupError {
    #[error("map identity belongs to another retained process")]
    WrongProcess(WorldMapId),
    #[error("map allocation is unavailable or retired")]
    Unavailable(WorldMapId),
    #[error("map construction has not completed")]
    Pending(WorldMapId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConstructionPhase {
    Initialized,
    PlaneStored,
    GridStored,
    Completed,
}

#[derive(Debug)]
struct MapAllocation {
    local: LocalId,
    /// 471B1C and471B20 initialize exactly BYTE+1C/DWORD+20; only the
    /// tests read them.
    #[cfg(test)]
    byte_1c: u8,
    #[cfg(test)]
    dword_20: u32,
    jitter: WorldMapState,
    phase: ConstructionPhase,
}

/// One actual process lifetime; never recreate this header per world load.
#[derive(Debug)]
pub struct ProcessWorldMapRuntime {
    identity: u64,
    allocations: Vec<Option<MapAllocation>>,
    markers: MarkerPool,
    plane_byte: Option<u8>,
    grid: Option<TerrainMap>,
}

impl ProcessWorldMapRuntime {
    pub fn native_startup() -> Self {
        let identity = NEXT_PROCESS_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(identity, 0, "retained map process identities exhausted");
        Self {
            identity,
            allocations: Vec::new(),
            markers: MarkerPool::native_startup(),
            plane_byte: None,
            grid: None,
        }
    }

    pub fn plane_byte(&self) -> Option<u8> {
        self.plane_byte
    }
    pub fn grid(&self) -> Option<&TerrainMap> {
        self.grid.as_ref()
    }

    fn allocation(&self, id: WorldMapId) -> Result<&MapAllocation, WorldMapLookupError> {
        if id.process != self.identity {
            return Err(WorldMapLookupError::WrongProcess(id));
        }
        self.allocations
            .get(id.allocation)
            .and_then(Option::as_ref)
            .ok_or(WorldMapLookupError::Unavailable(id))
    }

    fn allocation_mut(
        &mut self,
        id: WorldMapId,
    ) -> Result<&mut MapAllocation, WorldMapLookupError> {
        if id.process != self.identity {
            return Err(WorldMapLookupError::WrongProcess(id));
        }
        self.allocations
            .get_mut(id.allocation)
            .and_then(Option::as_mut)
            .ok_or(WorldMapLookupError::Unavailable(id))
    }

    pub fn jitter(&self, id: WorldMapId) -> Result<&WorldMapState, WorldMapLookupError> {
        let allocation = self.allocation(id)?;
        if allocation.phase != ConstructionPhase::Completed {
            return Err(WorldMapLookupError::Pending(id));
        }
        Ok(&allocation.jitter)
    }

    pub fn jitter_mut(
        &mut self,
        id: WorldMapId,
    ) -> Result<&mut WorldMapState, WorldMapLookupError> {
        let allocation = self.allocation_mut(id)?;
        if allocation.phase != ConstructionPhase::Completed {
            return Err(WorldMapLookupError::Pending(id));
        }
        Ok(&mut allocation.jitter)
    }
}

/// The sole current mission+04 binding plus a port-only stopped carrier.
#[derive(Debug, PartialEq, Eq)]
pub struct MapLoadState {
    current: Option<WorldMapId>,
    pending: Option<WorldMapId>,
}

impl MapLoadState {
    /// Actual mission48D9F0 initializes+04 NULL at48DA17. Pending has no
    /// native pointer counterpart; it retains typed incomplete construction.
    pub const fn native_mission_constructor() -> Self {
        Self {
            current: None,
            pending: None,
        }
    }
    pub const fn current(&self) -> Option<WorldMapId> {
        self.current
    }
    pub const fn pending(&self) -> Option<WorldMapId> {
        self.pending
    }
}

pub struct MapLoadContext<'a> {
    pub process: &'a mut ProcessWorldMapRuntime,
    pub state: &'a mut MapLoadState,
}

/// Actual complete terrain prepared by44B590, with its original H12 custody.
/// This loan supplies known sampling/plane data; it is not a lazy loader.
pub struct PreparedWorldMapTerrain<'a> {
    source: WorldTerrainSource,
    terrain: &'a TerrainRuntime,
    plane_height_fixed: i32,
}

impl<'a> PreparedWorldMapTerrain<'a> {
    pub fn new(
        source: &ResidentResource<'_>,
        terrain: &'a TerrainRuntime,
        plane_height_fixed: i32,
    ) -> Self {
        Self {
            source: WorldTerrainSource::retain(source),
            terrain,
            plane_height_fixed,
        }
    }
}

pub struct WorldMapTerrainInputs<'a> {
    pub residency: &'a ResidentRuntime,
    pub prepared: Option<PreparedWorldMapTerrain<'a>>,
}

pub struct WorldMapConstructionRequest<'a> {
    /// Native second argument LOWBYTE; first argument1 is genuinely unused.
    pub plane_byte: u8,
    pub terrain: WorldMapTerrainInputs<'a>,
    pub rng: &'a mut u32,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum WorldMapConstructionError {
    #[error("stopped map construction must be resolved explicitly, not retried")]
    PendingConstruction(WorldMapId),
    #[error("map constructor requires H12/0; canonical virtual+0 returns NULL")]
    RequiredTerrain,
    #[error("map constructor requires complete prepared H12 terrain")]
    RequiredPreparedTerrain,
    #[error("current H12/0 differs from the prepared terrain's registration/allocation")]
    PreparedTerrainReplaced,
    #[error(transparent)]
    Marker(#[from] MarkerStorageError),
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum WorldMapCleanupError {
    #[error(transparent)]
    Map(#[from] WorldMapLookupError),
    #[error(transparent)]
    Marker(#[from] MarkerStorageError),
}

pub fn construct_world_map(
    context: MapLoadContext<'_>,
    request: WorldMapConstructionRequest<'_>,
) -> Result<WorldMapId, WorldMapConstructionError> {
    let MapLoadContext { process, state } = context;
    if let Some(id) = state.pending {
        return Err(WorldMapConstructionError::PendingConstruction(id));
    }
    // Successful allocation/storage domain, retained before any reached H12
    // or registration failure. No fallback for native map/marker mallocNULL.
    let local = process.markers.allocate_local();
    let id = WorldMapId {
        process: process.identity,
        allocation: process.allocations.len(),
    };
    process.allocations.push(Some(MapAllocation {
        local,
        #[cfg(test)]
        byte_1c: 0,
        #[cfg(test)]
        dword_20: 0,
        jitter: WorldMapState::new(),
        phase: ConstructionPhase::Initialized,
    }));
    state.pending = Some(id);
    process.plane_byte = Some(request.plane_byte);
    process.allocations[id.allocation]
        .as_mut()
        .expect("retained allocated map")
        .phase = ConstructionPhase::PlaneStored;
    // 471B3B: this is after the initialized local/scalars and plane store.
    // Canonical pool miss4EFBF0 returnsNULL and offers no success bypass.
    let current = request
        .terrain
        .residency
        .resource(12, 0)
        .ok_or(WorldMapConstructionError::RequiredTerrain)?;
    let prepared = request
        .terrain
        .prepared
        .ok_or(WorldMapConstructionError::RequiredPreparedTerrain)?;
    if !prepared.source.is_current(&current) {
        return Err(WorldMapConstructionError::PreparedTerrainReplaced);
    }
    let plane = if request.plane_byte == 0 {
        TerrainMapPlane::Ignore
    } else {
        TerrainMapPlane::Classify {
            height_fixed: prepared.plane_height_fixed,
        }
    };
    // The complete known/exclusive source domain has no modeled physical
    // per-cell fault. Same RNG, X-outer/Z-inner, one draw each cell.
    process.grid = Some(TerrainMap::new(prepared.terrain, plane, request.rng));
    process.allocations[id.allocation]
        .as_mut()
        .expect("retained allocated map")
        .phase = ConstructionPhase::GridStored;
    process.markers.register(local)?;
    process.allocations[id.allocation]
        .as_mut()
        .expect("retained allocated map")
        .phase = ConstructionPhase::Completed;
    // 48DE41 only follows completed471AE0; existing current may be replaced
    // without implicit retirement, exactly as this native constructor caller.
    state.current = Some(id);
    state.pending = None;
    Ok(id)
}

/// Only48E89D..BB's map phase. Callers own the NULL gate and subsequent byte12,
/// tasks/player/manager order. No Drop cleanup or process-global44FD30 here.
pub fn retire_current_world_map(context: MapLoadContext<'_>) -> Result<(), WorldMapCleanupError> {
    let MapLoadContext { process, state } = context;
    let Some(id) = state.current else {
        return Ok(());
    };
    let local = process.allocation(id)?.local;
    process.markers.cleanup_local(local)?;
    process.markers.retire_local(local)?;
    process.allocations[id.allocation] = None;
    state.current = None;
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
