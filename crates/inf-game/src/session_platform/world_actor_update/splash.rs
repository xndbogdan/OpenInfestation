//! Production services for the water splash's update 0042DC90 (selector
//! 65): the terrain's cell under a ring point and the splash particle
//! 00489600 through the process pool.

use crate::{
    object_runtime::Selector65Host,
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime},
};

pub(crate) struct WorldSplashHost<'a, 'exe> {
    pub terrain: &'a TerrainRuntime,
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'exe>,
    pub process: &'a ParticleFactoryProcessState,
    pub phase: u32,
}

impl Selector65Host for WorldSplashHost<'_, '_> {
    type Error = String;

    fn under_water(&self, xz: [u16; 2]) -> Result<bool, String> {
        let height_shift = match self.terrain.sampling().height_scale {
            TerrainHeightScale::Units16 => 4,
            TerrainHeightScale::Units32 => 5,
        };
        let height = i32::from(self.terrain.cell_at(xz).height_i8()) << height_shift;
        Ok(height < self.terrain.plane_base() >> (self.terrain.plane_shift() & 31))
    }

    fn splash(&mut self, position: [u16; 3], rng: &mut u32) -> Result<(), String> {
        self.particles
            .birth_impact(
                ParticleBirthRequest {
                    particle_type: 0,
                    position,
                    velocity: Some([0; 3]),
                    flags: 0,
                    inputs: ParticleBirthInputs {
                        network_active: self.process.network_flag != 0,
                        phase: Some(self.phase),
                        process: self.process,
                    },
                },
                self.executable,
                rng,
            )
            .map(|_| ())
            .map_err(|error| format!("00489600 splash: {error:?}"))
    }
}
