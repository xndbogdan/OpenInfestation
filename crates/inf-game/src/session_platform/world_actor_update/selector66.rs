//! Live selector66 services share the current H12, particle allocator and RNG.
use crate::{
    object_runtime::{ObjectArena, ObjectId, Selector66Host, Selector66Particle},
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    },
    terrain_runtime::TerrainRuntime,
    world_actor_terrain::WorldTerrainSource,
    ResidentRuntime,
};
use inf_formats::FrontendExecutable;

pub(crate) struct WorldSelector66Host<'a, 'exe> {
    pub residency: &'a ResidentRuntime,
    pub terrain: &'a TerrainRuntime,
    pub terrain_source: &'a WorldTerrainSource,
    pub particles: &'a mut ProcessParticleRuntime,
    pub process: &'a ParticleFactoryProcessState,
    pub executable: FrontendExecutable<'exe>,
    pub phase: u32,
}
impl Selector66Host for WorldSelector66Host<'_, '_> {
    type Error = String;
    fn refresh_model(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        arena
            .refresh_visual_model(actor, self.residency)
            .map_err(|e| e.to_string())
    }
    fn ground_height(&mut self, _: &mut ObjectArena, xz: [u16; 2]) -> Result<u16, String> {
        let current = self
            .residency
            .resource(12, 0)
            .ok_or("selector66 requires current H12/0")?;
        if !self.terrain_source.is_current(&current) {
            return Err("selector66 H12 allocation differs from prepared terrain".into());
        }
        Ok(self.terrain.height_at(xz) as u16)
    }
    fn particle(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        kind: Selector66Particle,
        rng: &mut u32,
    ) -> Result<(), String> {
        let position = arena
            .selector66_position(actor)
            .map_err(|e| e.to_string())?;
        let request = ParticleBirthRequest {
            particle_type: match kind {
                Selector66Particle::Smoke(kind) | Selector66Particle::Flash(kind) => kind,
            },
            position,
            velocity: None,
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: self.process.network_flag != 0,
                phase: Some(self.phase),
                process: self.process,
            },
        };
        match kind {
            Selector66Particle::Smoke(_) => {
                self.particles
                    .birth_short_smoke(request, self.executable, rng)
            }
            Selector66Particle::Flash(_) => {
                self.particles
                    .birth_pickup_flash(request, self.executable, rng)
            }
        }
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}
