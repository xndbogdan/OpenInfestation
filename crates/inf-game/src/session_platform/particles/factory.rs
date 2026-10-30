//! World factory services that do not require collision geometry. Reclamation
//! still executes actual impact births on the retained particle cell.

use crate::{
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        factories::{
            ParticleCreationNotice, ParticleFactoryContext, ParticleFactoryHost,
            ParticleFactoryImpactRequest, ParticleFactorySources,
        },
        ProcessParticleRuntime,
    },
    session::SessionState,
};

pub(in crate::session_platform) struct WorldParticleFactoryHost<'a> {
    pub session: &'a SessionState,
}

impl ParticleFactoryHost for WorldParticleFactoryHost<'_> {
    type Error = String;
    fn current_phase(&mut self) -> Result<Option<u32>, String> {
        Ok(Some(u32::from(self.session.current_phase().id())))
    }
    fn network_notice(
        &mut self,
        notice: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), String> {
        crate::process_particles::factories::queue_creation_notice(particles, notice, sources)
            .map_err(String::from)
    }
    fn impact_489600(
        &mut self,
        request: ParticleFactoryImpactRequest,
        particles: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), String> {
        birth_factory_impact(request, particles, context, self.session).map(|_| ())
    }
}

pub(super) fn birth_factory_impact(
    request: ParticleFactoryImpactRequest,
    particles: &mut ProcessParticleRuntime,
    context: &mut ParticleFactoryContext<'_, '_>,
    session: &SessionState,
) -> Result<bool, String> {
    let record = particles
        .record(request.particle)
        .map_err(|error| error.to_string())?;
    let position = record.position().map_err(|error| error.to_string())?;
    let velocity = [
        record.read_word(10).map_err(|error| error.to_string())?,
        record.read_word(12).map_err(|error| error.to_string())?,
        record.read_word(14).map_err(|error| error.to_string())?,
    ];
    let born = particles
        .birth_impact(
            ParticleBirthRequest {
                particle_type: request.selector,
                position,
                velocity: Some(velocity),
                flags: request.flags,
                inputs: ParticleBirthInputs {
                    network_active: context.process.network_flag != 0,
                    phase: Some(u32::from(session.current_phase().id())),
                    process: context.process,
                },
            },
            context.executable,
            context.rng,
        )
        .map_err(|error| format!("{error:?}"))?;
    Ok(born.is_some())
}
