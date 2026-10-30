//! Shared current H12/0 acquisition for actor consumers. Each caller owns its
//! sample policy and the actor fields captured before or after this service.

use crate::{
    object_runtime::{ObjectArena, ObjectId},
    terrain_runtime::TerrainRuntime,
};

pub trait ActorTerrainHost {
    type Error;

    /// Acquire the current H12 slot zero through the 0055EE38 owner. The
    /// service may mutate live actors; a caller must preserve its native read
    /// order. Canonical startup virtual+0 returns NULL on a missing slot.
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error>;
}

/// An explicitly supplied resident owner, useful for isolated arithmetic.
/// This domain does not implement native missing-slot acquisition.
impl ActorTerrainHost for &TerrainRuntime {
    type Error = std::convert::Infallible;

    fn terrain(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        Ok(self)
    }
}
