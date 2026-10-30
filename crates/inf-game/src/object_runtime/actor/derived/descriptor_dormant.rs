//! Dormant descriptor wrappers 00469380/004693A0 dispatch the actual actor
//! +E0/+DC. The shared10/36 and selector12 bodies retain distinct flag/terrain
//! policies; selector35 particle callbacks are a reached required owner.

use super::{FamilyUpdateClass, FamilyUpdateError};
use crate::object_runtime::{
    mark_flag_change, ActorTerrainHost, ObjectArena, ObjectError, ObjectId,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyDormantRequest {
    Change { entering: bool },
    Update { microseconds: u32 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyDormantError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} has no owned dormant descriptor virtual table")]
    UnsupportedClass(ObjectId),
    #[error("actor {actor:?} requires dormant callback {address:08X}")]
    RequiredCallback { actor: ObjectId, address: u32 },
    #[error("dormant ground acquisition failed: {0}")]
    Terrain(E),
}

impl ObjectArena {
    /// The wrappers load the actual actor vtable even when entering is false.
    /// No descriptor/model/mission name substitutes for the final class table.
    pub fn dispatch_family_dormant<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        request: FamilyDormantRequest,
        host: &mut H,
    ) -> Result<(), FamilyDormantError<H::Error>> {
        let class = self
            .family_update_class(actor)
            .map_err(|error| match error {
                FamilyUpdateError::Object(error) => FamilyDormantError::Object(error),
                _ => FamilyDormantError::UnsupportedClass(actor),
            })?;
        if class == FamilyUpdateClass::Selector35 {
            return Err(FamilyDormantError::RequiredCallback {
                actor,
                address: match request {
                    FamilyDormantRequest::Change { .. } => 0x0043_e120,
                    FamilyDormantRequest::Update { .. } => 0x0043_e1c0,
                },
            });
        }
        match request {
            FamilyDormantRequest::Change { entering: false } => Ok(()),
            FamilyDormantRequest::Change { entering: true } => {
                self.family428650_actor_mut(actor)?.word_228 = 0x0053_0d44;
                if class != FamilyUpdateClass::Selector12 {
                    //00424790 leaves list bits0/1 intact; only low flag changes
                    //mark byte38 bit2. Do not use the list-membership setters.
                    let base = &mut self.owned_mut(actor)?.state.base;
                    let flags = base.flags & 0xffff_fefb;
                    mark_flag_change(base, flags);
                }
                Ok(())
            }
            FamilyDormantRequest::Update { .. } if class == FamilyUpdateClass::Selector12 => {
                // Actual 00404840 is RET4: no delta, terrain, XYZ or RNG read.
                Ok(())
            }
            FamilyDormantRequest::Update { .. } => {
                //004247C0 captures Z/X before lazy H12 virtual00, but captures
                //Y after that callback. The returned terrain loan is current;
                //a host mutation of Y must be visible to the subsequent read.
                let position = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                let xz = [position[0], position[2]];
                let terrain = host
                    .terrain(self, actor)
                    .map_err(FamilyDormantError::Terrain)?;
                let y = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?[1]
                    as i16;
                let ground = terrain.height_at(xz);
                let threshold = i32::from(self.object(actor)?.base.word_28) * 5 / 4;
                if i32::from(ground) - i32::from(y) > threshold {
                    //00479FA0 queues disposal; no destruction, death callback
                    //or implicit target release occurs in this body.
                    self.dispose(actor)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "descriptor_dormant/tests.rs"]
mod tests;
