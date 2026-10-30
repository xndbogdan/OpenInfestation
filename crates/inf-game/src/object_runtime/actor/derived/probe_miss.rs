//! Cache-miss callback 00429EE0. Collision 0047C1D0 stays a required mutable
//! service; a successful terrain query does not establish a clear obstacle.

use crate::{
    object_runtime::{ObjectArena, ObjectError, ObjectId},
    terrain_runtime::{TerrainProbePlanePolicy, TerrainRuntime, TerrainSlopeProbeRequest},
};
use thiserror::Error;

pub trait FamilyTerrainProbeMissHost {
    type Error;

    /// H12 slot zero, resident or obtained by its actual synchronous handler.
    /// Acquisition may mutate the arena; +160 was already read by the caller.
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error>;

    /// Actual 0047C1D0 against the actor's installed temporary XYZ and 8000
    /// guard. The reached collision callback clears that guard on obstruction.
    fn collide(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyTerrainProbeMissError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires its position at 00429F8C")]
    MissingPosition(ObjectId),
    #[error("family terrain probe host failed at {effect}: {error}")]
    Host { effect: &'static str, error: E },
}

impl ObjectArena {
    /// Complete 00429EE0. No cache borrow is retained across services; callers
    /// own 00454ED0/00454BE0 separately. Failure preserves all reached writes.
    pub fn probe_family_terrain_miss<H: FamilyTerrainProbeMissHost>(
        &mut self,
        actor: ObjectId,
        xz: [u16; 2],
        host: &mut H,
    ) -> Result<bool, FamilyTerrainProbeMissError<H::Error>> {
        self.family429e20_actor(actor)?;
        let plane = if self.derived_actor(actor)?.byte_160 == 0 {
            TerrainProbePlanePolicy::RejectBelow
        } else {
            TerrainProbePlanePolicy::ClampToPlane
        };
        let result = host
            .terrain(self, actor)
            .map_err(|error| FamilyTerrainProbeMissError::Host {
                effect: "H12 terrain acquisition",
                error,
            })?
            .probe_slope(TerrainSlopeProbeRequest {
                xz,
                maximum_step: 128,
                plane,
            });
        if result.blocked {
            return Ok(true);
        }
        let y = result
            .position
            .y
            .expect("non-rejected native probe writes Y");
        // 00429F81 precedes the old-position read, including on a missing
        // position failure. Direct stores never dirty XYZ or update its basis.
        self.family428650_actor_mut(actor)?.word_22e |= 0x8000;
        let previous = self
            .object(actor)?
            .base
            .position_words
            .ok_or(FamilyTerrainProbeMissError::MissingPosition(actor))?;
        self.owned_mut(actor)?.state.base.position_words =
            Some([result.position.xz[0], y, result.position.xz[1]]);
        host.collide(self, actor)
            .map_err(|error| FamilyTerrainProbeMissError::Host {
                effect: "0047C1D0 collision",
                error,
            })?;
        let guard_survives = self.family428650_actor(actor)?.word_22e & 0x8000 != 0;
        self.owned_mut(actor)?.state.base.position_words = Some(previous);
        if guard_survives {
            // Clear only the guard, after restoring XYZ. All other callback
            // flags and mutations survive; there is no failure cleanup guard.
            self.family428650_actor_mut(actor)?.word_22e &= 0x7fff;
        }
        Ok(!guard_survives)
    }
}

#[cfg(test)]
mod tests;
