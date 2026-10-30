//! Lazy current H14/0 loan for shared actor contact. H12 identity is checked
//! independently by WorldActorTerrainHost; this service never replaces it.

use crate::{
    object_runtime::{ActorContactHeaderHost, ObjectArena, ObjectId},
    ResidentRuntime,
};
use inf_formats::{parse_world_bank, WorldAssetError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorldActorContactHeaderError {
    #[error("actor contact requires current H14/0; native virtual+0 returned NULL")]
    RequiredHeader,
    #[error("current H14/0 registration has no matching canonical world header")]
    Registration,
    #[error(transparent)]
    World(#[from] WorldAssetError),
}

pub(crate) struct WorldActorContactHeaderHost<'a> {
    pub residency: &'a ResidentRuntime,
}
impl ActorContactHeaderHost for WorldActorContactHeaderHost<'_> {
    type Error = WorldActorContactHeaderError;
    fn header(&mut self, _arena: &mut ObjectArena, _actor: ObjectId) -> Result<&[u8], Self::Error> {
        let current = self
            .residency
            .resource(14, 0)
            .ok_or(Self::Error::RequiredHeader)?;
        let region = current
            .overlay
            .layout()
            .regions
            .get(current.registration.region_index)
            .filter(|r| r.handler.get() == 14)
            .ok_or(Self::Error::Registration)?;
        // Reuse canonical structural validation and source spans. This is
        // portable registration custody, not a new H14/parser fallback.
        let bank = parse_world_bank(current.overlay.bytes(), region)?;
        let world = bank
            .entries
            .get(usize::from(current.registration.local_index))
            .ok_or(Self::Error::Registration)?;
        world
            .header_span
            .get(current.overlay.bytes())
            .ok_or(Self::Error::Registration)
    }
}
