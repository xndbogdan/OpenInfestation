//! Concrete particle arms of 0048D820. The cache retains the row identity;
//! this adapter owns actual factory/copy/deleting composition, not actor
//! resources, network encoding, impacts or production actor dispatch.

use inf_formats::FrontendExecutable;
use thiserror::Error;

use super::{
    BehaviorComponentQuery, BehaviorCreationRequest, BehaviorPacketCopy,
    BehaviorPacketSourceRequest, BehaviorPredicateContext, BehaviorPredicateHost,
    BehaviorTemporary, ProcessActorBehaviorTables,
};
use crate::{
    object_runtime::{ActorHandle, ObjectArena, ObjectId},
    process_particles::{
        factories::{
            ParticleCreationNotice, ParticleDeleteRequest, ParticleFactoryContext,
            ParticleFactoryError, ParticleFactoryHost, ParticleFactoryImpactRequest,
            ParticleFactoryKind, ParticleFactoryRequest, ParticleFactorySources,
            ParticleVelocitySource,
        },
        ParticleId, ParticleVectorBytes, ProcessParticleRuntime,
    },
};

/// The genuine call-local eight-byte source is partitioned into the six
/// vector bytes read by factory construction and its two unread bytes.
/// SamePosition refers to vectors.position for both native pointers.
pub struct BehaviorParticleCreationSources<'a> {
    pub vectors: &'a mut ParticleFactorySources,
    pub scratch_tail: &'a mut [Option<u8>; 2],
}

/// All mutable owners visible at actual network/impact call boundaries.
/// Factory context contains only process/RNG/EXE; impact has no creation
/// source pointer and receives its real particle through the request.
pub struct BehaviorParticleServiceContext<'a, 'factory, 'exe> {
    pub arena: &'a mut ObjectArena,
    pub tables: &'a mut ProcessActorBehaviorTables,
    pub particles: &'a mut ProcessParticleRuntime,
    pub factory: &'a mut ParticleFactoryContext<'factory, 'exe>,
}

pub enum BehaviorParticleEffectRequest<'a> {
    NetworkNotice {
        notice: ParticleCreationNotice,
        sources: BehaviorParticleCreationSources<'a>,
    },
    Impact489600(ParticleFactoryImpactRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorActorResourceRequest {
    pub resource: i16,
    pub null_handle: ActorHandle,
    pub scratch_bytes: [Option<u8>; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorComponentPacketSourceRequest<C> {
    Channel148(C),
    Strength146(C),
    Complete13e(C),
}

/// Required bodies not supplied by the particle kernel. A nullable result
/// means actual native null; unavailable owners must return an error.
pub trait BehaviorParticleServices {
    type Error;
    type Component: Copy;

    /// Actual live manager phase, demanded only by an offline factory entry.
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error>;

    fn create_actor_resource(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorActorResourceRequest,
    ) -> Result<Option<ObjectId>, Self::Error>;

    fn query_component(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorComponentQuery<Self::Component>,
    ) -> Result<Option<Self::Component>, Self::Error>;

    fn component_packet_source(
        &mut self,
        arena: &ObjectArena,
        context: &BehaviorPredicateContext<'_>,
        request: BehaviorComponentPacketSourceRequest<Self::Component>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error>;

    fn particle_effect(
        &mut self,
        context: BehaviorParticleServiceContext<'_, '_, '_>,
        request: BehaviorParticleEffectRequest<'_>,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum BehaviorParticleAdapterError<E> {
    #[error("unowned particle factory callback {0:08X}")]
    UnsupportedFactory(u32),
    #[error("particle factory/deleting owner stopped: {0}")]
    Factory(ParticleFactoryError<E>),
    #[error("particle packet source owner stopped: {0}")]
    PacketSource(ParticleFactoryError<std::convert::Infallible>),
    #[error("required actor/component service stopped: {0}")]
    Lower(E),
    #[error("deleting invocation has no creation-notice source")]
    UnexpectedCreationNotice,
}

pub struct BehaviorParticleAdapter<'a, 'exe, S> {
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: FrontendExecutable<'exe>,
    pub services: &'a mut S,
}

enum Invocation<'a> {
    Creation {
        scratch_tail: &'a mut [Option<u8>; 2],
    },
    Deletion,
}

struct FactoryBridge<'a, S> {
    arena: &'a mut ObjectArena,
    tables: &'a mut ProcessActorBehaviorTables,
    services: &'a mut S,
    invocation: Invocation<'a>,
}

impl<S: BehaviorParticleServices> ParticleFactoryHost for FactoryBridge<'_, S> {
    type Error = BehaviorParticleAdapterError<S::Error>;

    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        self.services
            .current_phase()
            .map_err(BehaviorParticleAdapterError::Lower)
    }

    fn network_notice(
        &mut self,
        notice: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        factory: &mut ParticleFactoryContext<'_, '_>,
        vectors: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        let Invocation::Creation { scratch_tail } = &mut self.invocation else {
            return Err(BehaviorParticleAdapterError::UnexpectedCreationNotice);
        };
        self.services
            .particle_effect(
                BehaviorParticleServiceContext {
                    arena: self.arena,
                    tables: self.tables,
                    particles,
                    factory,
                },
                BehaviorParticleEffectRequest::NetworkNotice {
                    notice,
                    sources: BehaviorParticleCreationSources {
                        vectors,
                        scratch_tail,
                    },
                },
            )
            .map_err(BehaviorParticleAdapterError::Lower)
    }

    fn impact_489600(
        &mut self,
        request: ParticleFactoryImpactRequest,
        particles: &mut ProcessParticleRuntime,
        factory: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        self.services
            .particle_effect(
                BehaviorParticleServiceContext {
                    arena: self.arena,
                    tables: self.tables,
                    particles,
                    factory,
                },
                BehaviorParticleEffectRequest::Impact489600(request),
            )
            .map_err(BehaviorParticleAdapterError::Lower)
    }
}

// Bridge errors already carry lower-service identity; flatten that one
// layer while retaining the actual particle/error phase.
fn factory_error<E>(
    error: ParticleFactoryError<BehaviorParticleAdapterError<E>>,
) -> BehaviorParticleAdapterError<E> {
    use ParticleFactoryError as F;
    match error {
        F::Particle(error) => BehaviorParticleAdapterError::Factory(F::Particle(error)),
        F::Executable(error) => BehaviorParticleAdapterError::Factory(F::Executable(error)),
        F::MissingProcessNull => BehaviorParticleAdapterError::Factory(F::MissingProcessNull),
        F::MissingEmitterProcessNull { address } => {
            BehaviorParticleAdapterError::Factory(F::MissingEmitterProcessNull { address })
        }
        F::MissingVelocitySource => BehaviorParticleAdapterError::Factory(F::MissingVelocitySource),
        F::Network(BehaviorParticleAdapterError::Lower(error)) => {
            BehaviorParticleAdapterError::Factory(F::Network(error))
        }
        F::Impact(BehaviorParticleAdapterError::Lower(error)) => {
            BehaviorParticleAdapterError::Factory(F::Impact(error))
        }
        F::Phase(BehaviorParticleAdapterError::Lower(error)) => {
            BehaviorParticleAdapterError::Factory(F::Phase(error))
        }
        F::Network(error) | F::Impact(error) | F::Phase(error) => error,
    }
}

impl<S: BehaviorParticleServices> BehaviorPredicateHost for BehaviorParticleAdapter<'_, '_, S> {
    type Error = BehaviorParticleAdapterError<S::Error>;
    type Particle = ParticleId;
    type Component = S::Component;

    fn create(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorCreationRequest,
    ) -> Result<Option<BehaviorTemporary<ParticleId>>, Self::Error> {
        match request {
            BehaviorCreationRequest::ActorResource {
                resource,
                null_handle,
                scratch_bytes,
            } => self
                .services
                .create_actor_resource(
                    arena,
                    context,
                    BehaviorActorResourceRequest {
                        resource,
                        null_handle,
                        scratch_bytes,
                    },
                )
                .map(|actor| actor.map(BehaviorTemporary::Actor))
                .map_err(BehaviorParticleAdapterError::Lower),
            BehaviorCreationRequest::ParticleFactory {
                callback,
                context_word,
                null_handle,
                scratch_bytes,
            } => {
                let kind = match callback {
                    0x0046_65d0 => ParticleFactoryKind::Factory4665D0,
                    0x0046_6710 => ParticleFactoryKind::Factory466710,
                    0x0046_6850 => ParticleFactoryKind::Factory466850,
                    _ => return Err(BehaviorParticleAdapterError::UnsupportedFactory(callback)),
                };
                let [a, b, c, d, e, f, g, h] = scratch_bytes;
                let mut sources = ParticleFactorySources {
                    position: ParticleVectorBytes([a, b, c, d, e, f]),
                    separate_velocity: None,
                };
                let mut scratch_tail = [g, h];
                let mut factory = ParticleFactoryContext {
                    process: context.process,
                    rng: context.rng,
                    executable: self.executable,
                };
                let mut bridge = FactoryBridge {
                    arena,
                    tables: context.tables,
                    services: self.services,
                    invocation: Invocation::Creation {
                        scratch_tail: &mut scratch_tail,
                    },
                };
                self.particles
                    .create_factory_particle(
                        ParticleFactoryRequest {
                            kind,
                            selector: context_word,
                            source_handle: null_handle,
                            flags: 0,
                            velocity_source: ParticleVelocitySource::SamePosition,
                        },
                        &mut factory,
                        &mut sources,
                        &mut bridge,
                    )
                    .map(|id| id.map(BehaviorTemporary::Particle))
                    .map_err(factory_error)
            }
        }
    }

    fn query_component(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorComponentQuery<S::Component>,
    ) -> Result<Option<S::Component>, Self::Error> {
        self.services
            .query_component(arena, context, request)
            .map_err(BehaviorParticleAdapterError::Lower)
    }

    fn packet_source(
        &mut self,
        arena: &ObjectArena,
        context: &BehaviorPredicateContext<'_>,
        request: BehaviorPacketSourceRequest<ParticleId, S::Component>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error> {
        let component = match request {
            BehaviorPacketSourceRequest::ParticlePointer { particle, slot } => {
                return self
                    .particles
                    .factory_packet_source(particle, slot, self.executable)
                    .map(|bytes| bytes.map(BehaviorPacketCopy::Full))
                    .map_err(BehaviorParticleAdapterError::PacketSource);
            }
            BehaviorPacketSourceRequest::Channel148(token) => {
                BehaviorComponentPacketSourceRequest::Channel148(token)
            }
            BehaviorPacketSourceRequest::Strength146(token) => {
                BehaviorComponentPacketSourceRequest::Strength146(token)
            }
            BehaviorPacketSourceRequest::Complete13e(token) => {
                BehaviorComponentPacketSourceRequest::Complete13e(token)
            }
        };
        self.services
            .component_packet_source(arena, context, component)
            .map_err(BehaviorParticleAdapterError::Lower)
    }

    fn delete_particle(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        particle: ParticleId,
        flags: u32,
    ) -> Result<(), Self::Error> {
        let mut factory = ParticleFactoryContext {
            process: context.process,
            rng: context.rng,
            executable: self.executable,
        };
        let mut bridge = FactoryBridge {
            arena,
            tables: context.tables,
            services: self.services,
            invocation: Invocation::Deletion,
        };
        self.particles
            .delete_factory_particle(
                ParticleDeleteRequest { particle, flags },
                &mut factory,
                &mut bridge,
            )
            .map_err(factory_error)
    }
}

#[cfg(test)]
mod tests;
