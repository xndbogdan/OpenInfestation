//! Selector36, 00481110/00482480/00481E80, vtable00527EE0.

use inf_formats::native_basis::NativeBasisMath;

use super::{
    super::behavior::select_actor_behavior, DerivedActorConstructionRequest, DerivedActorError,
    Family4236d0Class, Family4236d0InitializationRequest, RetainedActorAllocation,
};
use crate::{
    object_runtime::{ActorConstructionError, ObjectArena, ObjectId},
    ResidentRuntime,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector36State {
    pub word_268: u32,
    pub word_26c: u32,
    pub word_270: u32,
    pub payload_274: Option<RetainedActorAllocation>,
    pub words_278_27c: [Option<u16>; 3],
    pub word_27e: u16,
    pub word_280: u32,
    pub word_284: u32,
}

impl ObjectArena {
    pub fn allocate_selector36_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_family4236d0_actor(request, rng)?;
        self.family4236d0_actor_mut(object)?.class =
            Family4236d0Class::Selector36(Box::new(Selector36State {
                word_268: 0,
                word_26c: 0,
                word_270: 0,
                payload_274: None,
                words_278_27c: [None; 3],
                word_27e: 0,
                word_280: 0,
                word_284: 0,
            }));
        Ok(object)
    }

    pub fn selector36_actor(
        &self,
        object: ObjectId,
    ) -> Result<&Selector36State, DerivedActorError> {
        match &self.family4236d0_actor(object)?.class {
            Family4236d0Class::Selector36(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector36(object)),
        }
    }

    pub fn initialize_selector36_actor(
        &mut self,
        object: ObjectId,
        request: Family4236d0InitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), DerivedActorError> {
        self.selector36_actor(object)?;
        self.initialize_family4236d0_actor(object, request, runtime, math)?;
        let retained = RetainedActorAllocation::new(request.allocation, request.payload_offset)?;
        let Family4236d0Class::Selector36(state) = &mut self.family4236d0_actor_mut(object)?.class
        else {
            unreachable!("class checked")
        };
        state.payload_274 = Some(retained);
        Ok(())
    }

    pub fn finish_selector36_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        self.selector36_actor(object)?;
        let flags = self.object(object)?.base.flags;
        let selector = self.actor(object)?.word_b4;
        if let Some(behavior) = &mut self.derived_actor_mut(object)?.behavior_140 {
            select_actor_behavior(behavior, flags, selector);
        }
        self.finish_family4236d0_initialization(object)
    }
}
