//! Actor+234 allocation custody around decoded 00454B30 cache storage.
//! Capturing identity never keeps the destroyed decoded cache or words alive.

use super::{
    access::{PreparedTerrainProbe, TerrainProbeMiss},
    TerrainProbeCache, TerrainProbeError,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use std::{fmt, rc::Rc};
use thiserror::Error;

/// The actual installed header owner. Access to decoded state is explicit;
/// replacing the decoded header/buffer in place preserves this allocation.
pub struct TerrainProbeAllocation {
    cache: TerrainProbeCache,
    identity: Rc<()>,
}

impl TerrainProbeAllocation {
    pub fn new(cache: TerrainProbeCache) -> Self {
        Self {
            cache,
            identity: Rc::new(()),
        }
    }

    pub fn decoded_cache(&self) -> &TerrainProbeCache {
        &self.cache
    }

    pub fn decoded_cache_mut(&mut self) -> &mut TerrainProbeCache {
        &mut self.cache
    }

    fn prepare<E>(
        &mut self,
        position_words: [u16; 2],
    ) -> Result<CapturedTerrainProbe, TerrainProbeError<E>> {
        Ok(match self.cache.prepare_probe(position_words)? {
            PreparedTerrainProbe::Hit(value) => CapturedTerrainProbe::Hit(value),
            PreparedTerrainProbe::Miss(prepared) => {
                CapturedTerrainProbe::Miss(CapturedTerrainProbeMiss {
                    identity: Rc::clone(&self.identity),
                    prepared,
                })
            }
        })
    }
}

/// A decoded snapshot creates a new owned allocation, never a custody alias.
impl Clone for TerrainProbeAllocation {
    fn clone(&self) -> Self {
        Self::new(self.cache.clone())
    }
}
impl fmt::Debug for TerrainProbeAllocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.cache, formatter)
    }
}
impl PartialEq for TerrainProbeAllocation {
    fn eq(&self, other: &Self) -> bool {
        self.cache == other.cache
    }
}
impl Eq for TerrainProbeAllocation {}

enum CapturedTerrainProbe {
    Hit(u8),
    Miss(CapturedTerrainProbeMiss),
}

/// This token retains only lightweight identity plus captured native locals.
/// It is not Clone and has no storage/view access or automatic completion.
struct CapturedTerrainProbeMiss {
    identity: Rc<()>,
    prepared: TerrainProbeMiss,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyTerrainProbeError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires its installed +234 cache")]
    MissingCache(ObjectId),
    #[error("actor {0:?} replaced its captured +234 allocation before normal completion")]
    ReplacedAllocation(ObjectId),
    #[error(transparent)]
    Cache(TerrainProbeError<E>),
}

impl ObjectArena {
    ///00454ED0/00454BE0 around the actual actor and installed +234 owner.
    /// A hit bypasses the service. Errors retain all reached writes and do
    /// not encode a sample; only normal return validates and completes.
    pub fn probe_family_terrain_cache<E>(
        &mut self,
        actor: ObjectId,
        position_words: [u16; 2],
        miss: impl FnOnce(&mut Self, ObjectId, [u16; 2]) -> Result<u32, E>,
    ) -> Result<u8, FamilyTerrainProbeError<E>> {
        let prepared = self
            .family429e20_actor_mut(actor)?
            .probe_234
            .as_mut()
            .ok_or(FamilyTerrainProbeError::MissingCache(actor))?
            .prepare(position_words)
            .map_err(FamilyTerrainProbeError::Cache)?;
        let token = match prepared {
            CapturedTerrainProbe::Hit(value) => return Ok(value),
            CapturedTerrainProbe::Miss(token) => token,
        };
        // The short cache borrow ends here. The callback sees the same
        // actual arena/source and the original +234 remains installed.
        let sample = miss(self, actor, token.prepared.position_words)
            .map_err(|error| FamilyTerrainProbeError::Cache(TerrainProbeError::Host(error)))?;
        let current = self
            .family429e20_actor_mut(actor)?
            .probe_234
            .as_mut()
            .ok_or(FamilyTerrainProbeError::MissingCache(actor))?;
        if !Rc::ptr_eq(&token.identity, &current.identity) {
            return Err(FamilyTerrainProbeError::ReplacedAllocation(actor));
        }
        current
            .cache
            .finish_probe(token.prepared, sample)
            .map_err(FamilyTerrainProbeError::Cache)
    }
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
