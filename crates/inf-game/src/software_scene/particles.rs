//! Process particles use the same renderer allocation/sort/pixel custody as
//! world models. Light-grid effects remain a required caller-owned service.

use super::*;
use crate::process_particles::{
    draw::{ParticleDrawHost, ParticleDrawLighting, ParticleDrawReport, ParticleDrawRequest},
    ProcessParticleRuntime,
};
use crate::world_lighting::TerrainLightRequest;

#[derive(Debug, Error)]
pub enum SoftwareParticleError<E> {
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Identity(#[from] SoftwareSceneServiceError),
    #[error(transparent)]
    Queue(#[from] DrawQueueFault),
    #[error("required particle terrain-light service failed")]
    Light(E),
}

struct SceneParticleHost<'s, 'f, 'r, L> {
    scene: &'s mut SoftwareScene<'f, 'r>,
    lighting: &'s mut L,
}

impl<'f, 'r> SoftwareScene<'f, 'r> {
    /// Submit one explicit bank into the existing deferred arena. Caller owns
    /// native batch boundaries, pending transfer and the light-grid lifetime.
    pub fn draw_particles<L: ParticleDrawLighting>(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        request: ParticleDrawRequest<'_>,
        executable: inf_formats::FrontendExecutable<'_>,
        lighting: &mut L,
    ) -> ParticleDrawReport<SoftwareParticleError<L::Error>> {
        particles.draw_particles(
            request,
            executable,
            &mut SceneParticleHost {
                scene: self,
                lighting,
            },
        )
    }
}

impl<L: ParticleDrawLighting> ParticleDrawLighting for SceneParticleHost<'_, '_, '_, L> {
    type Error = SoftwareParticleError<L::Error>;
    fn terrain_light(
        &mut self,
        request: TerrainLightRequest,
        emitter: [i32; 3],
    ) -> Result<(), Self::Error> {
        self.lighting
            .terrain_light(request, emitter)
            .map_err(SoftwareParticleError::Light)
    }
}

impl<'r, L: ParticleDrawLighting> ParticleDrawHost<'r> for SceneParticleHost<'_, '_, 'r, L> {
    fn sprite(&mut self, slot: i16) -> Result<Option<RenderResource<'r>>, Self::Error> {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: i32::from(slot),
        };
        match self.scene.resources.resolve(request) {
            Ok(resource) => Ok(Some(resource)),
            Err(
                RenderResourceError::NotResident { .. } | RenderResourceError::NegativeIndex { .. },
            ) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        Ok(self.scene.material_sorting())
    }
    fn shadow_material(&mut self, slot: i16) -> Result<Option<u32>, Self::Error> {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument: i32::from(slot),
        };
        let resource = self.scene.resources.resolve(request)?;
        match resource.data {
            RenderResourceData::Material(value) => {
                Ok(Some(self.scene.color_layout.convert_material(value)))
            }
            _ => Err(RenderResourceError::Registration { request }.into()),
        }
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        Ok(self.scene.texture_identities.resolve(texture)?)
    }
    fn submit(&mut self, command: PrimitiveCommand<'r>) -> Result<(), Self::Error> {
        self.scene.queue.submit(DrawSubmission {
            depth: command.depth,
            payload_blocks: command.payload_blocks,
            receiver_offset: 0,
            command,
        })?;
        Ok(())
    }
}
