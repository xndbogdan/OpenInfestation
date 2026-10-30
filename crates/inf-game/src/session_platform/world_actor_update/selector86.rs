//! Current process camera, H12/H14 and particle owners for 0042DF40.
use crate::{
    object_runtime::{
        ActorContactHeaderHost, CameraGlobalState, ObjectArena, ObjectId, Selector86Trail,
        Selector86UpdateHost,
    },
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    },
    terrain_runtime::{TerrainRuntime, TerrainWaterSurfaceRequest},
    world_actor_contact::WorldActorContactHeaderHost,
    world_actor_terrain::WorldTerrainSource,
    ResidentRuntime,
};
use inf_formats::FrontendExecutable;

pub(crate) struct WorldSelector86Host<'a, 'exe> {
    pub residency: &'a ResidentRuntime,
    pub terrain: &'a TerrainRuntime,
    pub terrain_source: &'a WorldTerrainSource,
    pub camera: &'a CameraGlobalState,
    pub particles: &'a mut ProcessParticleRuntime,
    pub process: &'a ParticleFactoryProcessState,
    pub executable: FrontendExecutable<'exe>,
    pub sine: &'a [u16; 4096],
    pub phase: u32,
}
impl Selector86UpdateHost for WorldSelector86Host<'_, '_> {
    type Error = String;
    fn cached_camera_position(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<[u16; 3], String> {
        self.camera
            .cached_position
            .ok_or_else(|| "selector86 requires retained process camera XYZ at005469E8".into())
    }
    fn surface_height(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        xz: [u16; 2],
    ) -> Result<i16, String> {
        let current = self
            .residency
            .resource(12, 0)
            .ok_or("selector86 requires current H12/0")?;
        if !self.terrain_source.is_current(&current) {
            return Err("selector86 H12 allocation differs from prepared terrain".into());
        }
        let height = self.terrain.height_at(xz);
        // The actual H14 acquisition occurs after height, including dry worlds.
        let mut header = WorldActorContactHeaderHost {
            residency: self.residency,
        };
        let flags = u32::from(
            *header
                .header(arena, object)
                .map_err(|e| e.to_string())?
                .get(4)
                .ok_or("selector86 H14 flags unavailable")?,
        );
        let water = self.terrain.water_surface_height(
            TerrainWaterSurfaceRequest {
                xz,
                wave_clock: 0,
                world_flags: flags,
                terrain_height: height,
            },
            self.sine,
        );
        Ok(height.max(water))
    }
    fn emit_trail(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        trail: Selector86Trail,
        rng: &mut u32,
    ) -> Result<(), String> {
        self.particles
            .birth_trail(
                ParticleBirthRequest {
                    particle_type: trail.particle_type,
                    position: trail.position,
                    velocity: Some([0, 100, 0]),
                    flags: 0x800,
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
            .map_err(|e| e.to_string())
    }
}
