//! Current H12/0 custody for live actor height consumers. Preparation owns a
//! mutable terrain copy; acquisition must authenticate its original allocation.

use std::sync::Arc;

use thiserror::Error;

use crate::{
    object_runtime::{ActorTerrainHost, ObjectArena, ObjectId},
    terrain_runtime::TerrainRuntime,
    LoadedOverlay, ResidentRegistration, ResidentResource, ResidentRuntime,
};

#[derive(Debug)]
pub(crate) struct WorldTerrainSource {
    registration: ResidentRegistration,
    overlay: Arc<LoadedOverlay>,
}

impl WorldTerrainSource {
    pub(crate) fn retain(source: &ResidentResource<'_>) -> Self {
        Self {
            registration: source.registration.clone(),
            overlay: Arc::clone(source.overlay),
        }
    }

    pub(crate) fn query_record(
        &self,
        record: crate::terrain_runtime::TerrainSparseRecordId,
    ) -> crate::world_furniture_effect::FurnitureQueryRecord {
        crate::world_furniture_effect::FurnitureQueryRecord::sparse(
            &self.registration,
            &self.overlay,
            record,
        )
    }

    /// Shared registration/allocation identity check; callers still acquire
    /// their actual current H12 at the reached native resource phase.
    pub(crate) fn is_current(&self, current: &ResidentResource<'_>) -> bool {
        current.registration == &self.registration && Arc::ptr_eq(current.overlay, &self.overlay)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorldActorTerrainError {
    #[error("actor terrain requires current H12/0; canonical virtual+0 returns NULL")]
    RequiredTerrain,
    #[error("current H12/0 replaced the prepared world terrain's registration or allocation")]
    PreparedTerrainReplaced,
}

pub(crate) struct WorldActorTerrainHost<'a> {
    pub residency: &'a ResidentRuntime,
    pub source: &'a WorldTerrainSource,
    pub prepared: &'a TerrainRuntime,
}

impl ActorTerrainHost for WorldActorTerrainHost<'_> {
    type Error = WorldActorTerrainError;

    fn terrain(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        // 004402A9/004402D9 independently acquire the actual H12 slot. Both
        // startup tables527778/52778C have genuine NULL leaf 004EFBF0 at+0.
        let current = self
            .residency
            .resource(12, 0)
            .ok_or(WorldActorTerrainError::RequiredTerrain)?;
        if !self.source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced);
        }
        // Return the retained mutable world's current terrain state, preserving
        // prepared diagonals and deformation. Never re-decode per acquisition.
        Ok(self.prepared)
    }
}
