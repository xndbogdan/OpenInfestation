//! 00429E20, the probe-owning sibling of the scalar 004293D0 family.
//! 00429E70 frees the probe buffer and header before common-family cleanup.

use super::{
    family4236d0::Family4236d0State, family428650::Family428650Class, selector12::Selector12State,
    terrain_probe::TerrainProbeAllocation, DerivedActorConstructionRequest,
};
use crate::object_runtime::{ActorConstructionError, ObjectArena, ObjectError, ObjectId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Family429e20Class {
    Base,
    Family4236d0(Box<Family4236d0State>),
    Selector12(Box<Selector12State>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family429e20State {
    pub class: Family429e20Class,
    /// 00429D60 consumes the low WORD. 00429E20 leaves it unwritten;
    /// above-water decay does not establish a fresh value.
    pub word_230: Option<u16>,
    pub probe_234: Option<Box<TerrainProbeAllocation>>,
    pub word_238: u16,
    pub word_23a: Option<u16>,
}

impl ObjectArena {
    pub fn allocate_family429e20_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_family428650_actor(request, rng)?;
        let word_230 = self.actor_allocation.initial_word();
        self.family428650_actor_mut(object)?.class =
            Family428650Class::Family429e20(Box::new(Family429e20State {
                class: Family429e20Class::Base,
                word_230,
                probe_234: None,
                word_238: 0,
                word_23a: None,
            }));
        Ok(object)
    }

    pub fn family429e20_actor(&self, object: ObjectId) -> Result<&Family429e20State, ObjectError> {
        match &self.family428650_actor(object)?.class {
            Family428650Class::Family429e20(state) => Ok(state),
            _ => Err(ObjectError::NotFamily429e20(object)),
        }
    }

    pub(in crate::object_runtime) fn family429e20_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Family429e20State, ObjectError> {
        match &mut self.family428650_actor_mut(object)?.class {
            Family428650Class::Family429e20(state) => Ok(state),
            _ => Err(ObjectError::NotFamily429e20(object)),
        }
    }
}
