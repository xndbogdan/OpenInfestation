//! Direct fixed-class factory 004400D0, distinct from generic 004603B0.
//!
//! The genuine constructor publishes class93 before current H13[52] is read.
//! Required errors retain reached constructor and callback effects.
//! This completed-constructor domain borrows current process NULL independently
//! from the registry's null sentinel. Unknown NULL,
//! native malloc failure, physical fault/SEH and arbitrary rejected suffixes
//! are excluded; their original prefixes are not simulated by this API.

use inf_formats::native_basis::NativeBasisMath;
use thiserror::Error;

use super::construction::actor_factory::{
    complete_actor_initialization, ActorCompletionNetwork, ActorCompletionRequest,
};
use super::construction::{run_commands, selector93_initialization_plan, CommandPolicy};
use super::{
    ActorConstructionError, ActorConstructionRequest, ActorConstructorNull,
    ActorInitializationRequest, BaseContext, CreationListenerEffects, CreationListeners,
    ObjectArena, ObjectConstructionError, ObjectId, RotatingVisualContext,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::ResidentRuntime;

/// Inputs of the literal mode0, NULL-position/rotation/suffix factory.
/// Allocation position belongs to the inherited constructor and is retained.
pub struct Selector93FactoryRequest<'a> {
    pub allocation: ActorConstructionRequest<'a>,
    pub process: &'a ParticleFactoryProcessState,
    pub local_owner: u8,
    pub runtime: &'a ResidentRuntime,
    pub math: &'a NativeBasisMath,
    pub listeners: &'a mut CreationListeners,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
    pub network_world_active: bool,
}

#[derive(Debug, Error)]
pub enum Selector93FactoryError {
    #[error("direct class93 completed construction requires the current process NULL")]
    MissingProcessNull,
    #[error(transparent)]
    Allocation(#[from] ActorConstructionError),
    #[error("direct class93 initialization stopped after publishing actor {object:?}: {source}")]
    Published {
        object: ObjectId,
        #[source]
        source: ObjectConstructionError,
    },
}

/// Constructor93 -> fixed H13[52] shared initialization -> current completion.
/// No generic cleanup is performed on a reached required-source failure.
/// The caller owns any eventual listener unregistration and real destruction.
pub fn construct_selector93_direct(
    arena: &mut ObjectArena,
    request: Selector93FactoryRequest<'_>,
) -> Result<ObjectId, Selector93FactoryError> {
    request
        .process
        .process_null_handle
        .ok_or(Selector93FactoryError::MissingProcessNull)?;
    let object = arena.allocate_selector93(ActorConstructionRequest {
        constructor_null: ActorConstructorNull::Process(request.process),
        ..request.allocation
    })?;
    let initialization = (|| -> Result<(), ObjectConstructionError> {
        // Unlike generic construction, header +0 cannot choose this class.
        // The registered allocation and shared payload are acquired only now.
        let plan = selector93_initialization_plan(arena, request.runtime)?;
        arena.initialize_actor(
            object,
            ActorInitializationRequest {
                context: RotatingVisualContext {
                    base: BaseContext {
                        position_words: None,
                        placement_index: u32::MAX,
                    },
                    model_argument: Some(u32::from(plan.model_word)),
                    rotation_words: None,
                },
                payload: &plan.payload,
                local_owner: request.local_owner,
            },
            request.runtime,
            request.math,
        )?;
        // The actual shared guard is true and the literal suffix is NULL.
        // Thus this domain has no command rejection / native AL-false result.
        run_commands(
            arena,
            request.runtime,
            object,
            CommandPolicy::SharedActor {
                first_player_grants: None,
                allocation: None,
            },
            0x34,
            None,
        )?;
        arena.finish_actor_initialization(object)?;
        complete_actor_initialization(
            arena,
            object,
            ActorCompletionRequest {
                listeners: request.listeners,
                listener_effects: request.listener_effects,
                network: ActorCompletionNetwork::CurrentShared {
                    process: request.process,
                    world_active: request.network_world_active,
                },
            },
        )
    })();
    initialization.map_err(|source| Selector93FactoryError::Published { object, source })?;
    Ok(object)
}

#[cfg(test)]
#[path = "selector93_factory_tests.rs"]
mod tests;
