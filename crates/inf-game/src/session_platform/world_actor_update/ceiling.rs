//! Production services for the altitude ceiling's update 0043C8F0
//! (selector 48): its shots through the particle factory 00466710 and its
//! hint to the first player (0046C760).

use crate::{
    object_runtime::{
        ObjectArena, PlayerCommandContext, PlayerCommandOperation, PlayerCommandRequest, PlayerId,
        Selector48Host, CEILING_HINT,
    },
    process_particles::{
        factories::{
            ParticleCreationNotice, ParticleFactoryContext, ParticleFactoryHost,
            ParticleFactoryImpactRequest, ParticleFactoryKind, ParticleFactoryProcessState,
            ParticleFactoryRequest, ParticleFactorySources, ParticleVelocitySource,
        },
        ParticleVectorBytes, ProcessParticleRuntime,
    },
    ResidentRuntime,
};

pub(crate) struct WorldCeilingHost<'a, 'exe> {
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'exe>,
    pub process: &'a ParticleFactoryProcessState,
    pub residency: &'a ResidentRuntime,
    pub phase: u32,
}

/// 00466710's shot reaches only the phase read offline.
struct ShotServices {
    phase: Option<u32>,
}

impl ParticleFactoryHost for ShotServices {
    type Error = String;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase)
    }
    fn network_notice(
        &mut self,
        notice: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        crate::process_particles::factories::queue_creation_notice(particles, notice, sources)
            .map_err(String::from)
    }
    fn impact_489600(
        &mut self,
        _: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        Err("the ceiling shot's impact 00489600".into())
    }
}

impl Selector48Host for WorldCeilingHost<'_, '_> {
    type Error = String;

    fn shot(
        &mut self,
        _: &mut ObjectArena,
        kind: u32,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        // 0043CBFB: the owner is the process NULL 0054E894.
        let owner = self
            .process
            .process_null_handle
            .ok_or("the ceiling's shot needs the process NULL 0054E894")?;
        let mut sources = ParticleFactorySources {
            position: ParticleVectorBytes::from_words(position),
            separate_velocity: Some(ParticleVectorBytes::from_words(velocity)),
        };
        let mut process = *self.process;
        self.particles
            .create_factory_particle(
                ParticleFactoryRequest {
                    kind: ParticleFactoryKind::Factory466710,
                    selector: kind,
                    source_handle: owner,
                    flags: 0,
                    velocity_source: ParticleVelocitySource::Separate,
                },
                &mut ParticleFactoryContext {
                    process: &mut process,
                    rng,
                    executable: self.executable,
                },
                &mut sources,
                &mut ShotServices {
                    phase: Some(self.phase),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("00466710 ceiling shot: {error:?}"))
    }

    fn hint(&mut self, arena: &mut ObjectArena, player: PlayerId) -> Result<(), String> {
        arena
            .apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set {
                        raw_id: CEILING_HINT,
                        silent: 0,
                    },
                },
                &PlayerCommandContext {
                    process: Some(self.process),
                    executable: Some(self.executable),
                    resident: Some(self.residency),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0046C760(0x2E, 0): {error}"))
    }
}
