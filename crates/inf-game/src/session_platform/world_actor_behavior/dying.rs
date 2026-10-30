//! Selector12's actual +120/+11C services over the existing world owners.

use super::{WorldFamilyBehaviorError, WorldFamilyBehaviorHost};
use crate::{
    object_runtime::{
        ActorHandle, ActorStateChangeEffect, ActorStateChangeHost, ActorStateChangeQuery,
        ActorStateChangeRequest, ObjectArena, ObjectId, Selector12DyingHost,
        Selector12DyingRequest, TargetNullHandleSource,
    },
    process_particles::{
        births::{ParticleBirthInputs, ParticleBurst, ParticleBurstRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    },
    process_sound::{ProcessParticleSoundQueue, ProcessSoundRuntime},
    session::SessionState,
    ResidentRuntime,
};
use inf_formats::FrontendExecutable;
use thiserror::Error;

impl WorldFamilyBehaviorHost<'_, '_> {
    pub(super) fn selector12_dying(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: Selector12DyingRequest,
    ) -> Result<(), WorldFamilyBehaviorError> {
        let mut host = DyingServices {
            residency: self.residency,
            executable: self.executable,
            process: self.factory_process,
            session: self.session,
            particles: self.particles,
            sounds: self.sounds,
            load_factor: self.particle_load_factor,
        };
        arena
            .run_selector12_dying(actor, request, &mut host, self.process_rng)
            .map_err(|error| {
                WorldFamilyBehaviorError::owner("Selector12 dying descriptor", error)
            })?;
        if matches!(request, Selector12DyingRequest::Change { entering } if entering as u8 != 0) {
            eprintln!(
                "World actor dying: clock {}, {actor:?} selector12 model248 and particle burst completed",
                self.animation_clock.animation_value(),
            );
        }
        Ok(())
    }
}

struct DyingServices<'a, 'exe> {
    residency: &'a ResidentRuntime,
    executable: FrontendExecutable<'exe>,
    process: &'a ParticleFactoryProcessState,
    session: &'a SessionState,
    particles: &'a mut ProcessParticleRuntime,
    sounds: &'a mut ProcessSoundRuntime,
    load_factor: u32,
}

impl TargetNullHandleSource for DyingServices<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.process.process_null_handle
    }
}

impl Selector12DyingHost for DyingServices<'_, '_> {
    type Error = WorldFamilyBehaviorError;

    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _rng: &mut u32,
    ) -> Result<(), Self::Error> {
        arena
            .refresh_visual_model(actor, self.residency)
            .map_err(|error| WorldFamilyBehaviorError::owner("dying model refresh", error))
    }

    fn birth_burst(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.particles
            .birth_burst(
                ParticleBurstRequest {
                    burst,
                    load_factor: self.load_factor,
                    inputs: ParticleBirthInputs {
                        network_active: self.process.network_flag != 0,
                        phase: Some(u32::from(self.session.current_phase().id())),
                        process: self.process,
                    },
                },
                self.executable,
                rng,
                &mut ProcessParticleSoundQueue {
                    sounds: self.sounds,
                    residency: self.residency,
                },
            )
            .map(|_| ())
            .map_err(|error| WorldFamilyBehaviorError::owner("dying particle burst", error))
    }

    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorStateChangeRequest,
        _rng: &mut u32,
    ) -> Result<(), Self::Error> {
        arena
            .change_actor_state(actor, request, &mut DyingStateZero)
            .map_err(|error| WorldFamilyBehaviorError::owner("dying state-zero transition", error))
    }
}

/// The actual descriptor is selected by +C4=1. Other downward transitions
/// retain the shared state prefix and require their own attribution/effects.
pub(in crate::session_platform) struct DyingStateZero;

#[derive(Debug, Error)]
pub(in crate::session_platform) enum DyingStateZeroError {
    #[error(transparent)]
    Object(#[from] crate::object_runtime::ObjectError),
    #[error(transparent)]
    Derived(#[from] crate::object_runtime::DerivedActorError),
    #[error("dying actor {actor:?} requires lifecycle query {query:?}")]
    Query {
        actor: ObjectId,
        query: ActorStateChangeQuery,
    },
    #[error("dying actor {0:?} requires lifecycle attribution")]
    Attribution(ObjectId),
    #[error("selector35 {0:?} requires +251 at 0043E250")]
    Selector35Alternate(ObjectId),
    #[error("dying actor requires lifecycle effect {0:?}")]
    Effect(ActorStateChangeEffect<()>),
}

impl ActorStateChangeHost for DyingStateZero {
    type Error = DyingStateZeroError;
    type AttributionOwner = ();

    fn query(
        &mut self,
        _arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, Self::Error> {
        Err(DyingStateZeroError::Query { actor, query })
    }

    fn attribute(
        &mut self,
        _arena: &mut ObjectArena,
        actor: ObjectId,
        _source: ActorHandle,
    ) -> Result<(), Self::Error> {
        Err(DyingStateZeroError::Attribution(actor))
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), Self::Error> {
        match effect {
            ActorStateChangeEffect::StateZero { actor, .. } => {
                if arena.selector12_actor(actor).is_ok() || arena.selector34_actor(actor).is_ok() {
                    // Finals 527058 and 521EAC share +B0=4026E0→479FA0, which
                    // queues disposal. The next 478F4B manager pass destroys.
                    arena.dispose(actor)?;
                    Ok(())
                } else if let Ok(state) = arena.selector35_actor(actor) {
                    // Actual 43E250 skips disposal only for a nonzero +251.
                    // The unknown constructor carry is not equivalent to zero.
                    let alternate = state
                        .byte_251
                        .ok_or(DyingStateZeroError::Selector35Alternate(actor))?;
                    if alternate == 0 {
                        arena.dispose(actor)?;
                    }
                    Ok(())
                } else {
                    match arena.family4236d0_actor(actor)?.class {
                        // Final520B84/520CC8/527EE0+B0=4026D0 is RET8. These
                        // corpses do not inherit selector12's disposal rule.
                        crate::object_runtime::Family4236d0Class::Base
                        | crate::object_runtime::Family4236d0Class::Selector10(_)
                        | crate::object_runtime::Family4236d0Class::Selector36(_) => Ok(()),
                    }
                }
            }
            other => Err(DyingStateZeroError::Effect(other)),
        }
    }
}
