//! Shared native terrain-to-deferred-pixel bridge. Gameplay and briefing
//! supply distinct views and retained projection/light inputs to one owner.

use super::*;
use crate::terrain_draw::{
    clip_terrain_tile, draw_terrain, draw_water_strip, emit_terrain_particles, TerrainClipRequest,
    TerrainClippingHost, TerrainDrawError, TerrainDrawFault, TerrainDrawHost, TerrainDrawOutcome,
    TerrainFeature, TerrainFurnitureRequest, TerrainParticleEmissionRequest, TerrainQuadCallback,
    TerrainQuadCommand, TerrainStripRequest,
};

#[derive(Debug, Error)]
pub enum SoftwareTerrainError {
    #[error("terrain source custody: {source}")]
    Custody {
        #[source]
        source: Box<dyn std::error::Error>,
    },
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    TextureIdentity(#[from] SoftwareSceneServiceError),
    #[error(transparent)]
    Native(#[from] TerrainDrawFault),
    #[error("native terrain service {0} has no production owner")]
    Unowned(&'static str),
    #[error("native furniture draw: {0}")]
    Furniture(String),
    #[error("native terrain particle birth: {0}")]
    Particle(String),
}

#[derive(Debug)]
pub struct TerrainOmission {
    pub feature: TerrainFeature,
    pub reason: String,
}

#[derive(Debug)]
pub struct SceneTerrainOutcome {
    pub traversal: TerrainDrawOutcome,
    pub omissions: Vec<TerrainOmission>,
    pub furniture: FurnitureSceneReport,
    /// Water queue commands, including a prefix before a later strip error.
    pub water_commands: usize,
}

struct SceneTerrainHost<'s, 'a, 'f, 'r> {
    scene: &'s mut SoftwareScene<'f, 'r>,
    furniture: SoftwareFurnitureInputs<'a>,
    furniture_report: FurnitureSceneReport,
    omissions: Vec<TerrainOmission>,
    first_hard: Option<TerrainDrawError<SoftwareTerrainError>>,
    /// While a water strip draws.
    water: bool,
}

impl SceneTerrainHost<'_, '_, '_, '_> {
    /// Whether the scene feed's tracer draws this ground or water command
    /// instead of the raster: under a traced world, whatever the feed holds.
    fn traced_ground(&self) -> bool {
        self.scene.scene_feed.as_ref().is_some_and(|builder| {
            if self.water {
                builder.feed.water
            } else {
                builder.feed.terrain.is_some()
            }
        })
    }
}

impl<'f, 'r> SoftwareScene<'f, 'r> {
    pub fn primitive_tables(&self) -> &'r PrimitiveTables {
        self.tables
    }

    /// Uses the caller's explicit terrain/view/lighting and the retained
    /// renderer/texture identities/queue. No mission selection is consulted.
    /// Missing subtree services preserve native strip failure containment.
    pub fn draw_terrain(
        &mut self,
        request: SoftwareTerrainDrawRequest<'_>,
    ) -> Result<SceneTerrainOutcome, TerrainDrawError<SoftwareTerrainError>> {
        let SoftwareTerrainDrawRequest {
            traversal,
            furniture,
        } = request;
        let mut host = SceneTerrainHost {
            scene: self,
            furniture,
            furniture_report: FurnitureSceneReport::default(),
            omissions: Vec::new(),
            first_hard: None,
            water: false,
        };
        let water_before = host.scene.stats.water_commands;
        let traversal_result = draw_terrain(traversal, &mut host);
        // A strip-level omission must not erase the first hard cause. Required
        // successors stop before publications/RNG/submission; preserve301 even
        // when the traversal subsequently returns Stopped from that guard.
        if let Some(error) = host.first_hard.take() {
            return Err(error);
        }
        let mut traversal = traversal_result.map_err(|error| {
            if terrain_error_is_hard(&error) {
                host.scene.latch_stop(&error);
            }
            error
        })?;
        let water_commands = host.scene.stats.water_commands - water_before;
        traversal.omitted_features += host.furniture_report.omissions.len();
        for omission in &host.furniture_report.omissions {
            host.omissions.push(TerrainOmission {
                feature: TerrainFeature::Furniture,
                reason: format!("sparse {:?}: {}", omission.record, omission.reason),
            });
        }
        Ok(SceneTerrainOutcome {
            traversal,
            omissions: host.omissions,
            furniture: host.furniture_report,
            water_commands,
        })
    }
}

fn terrain_error_is_hard(error: &TerrainDrawError<SoftwareTerrainError>) -> bool {
    match error {
        // 44AA10's strip frame catches a full arena (0x301) like any error.
        TerrainDrawError::Host(SoftwareTerrainError::Scene(error)) => {
            error.native_error_code() != Some(0x301) && error.is_renderer_custody_fault()
        }
        TerrainDrawError::Host(SoftwareTerrainError::Resource(error)) => !matches!(
            error,
            RenderResourceError::NotResident { .. } | RenderResourceError::SpriteVariant { .. }
        ),
        TerrainDrawError::Host(
            SoftwareTerrainError::TextureIdentity(_) | SoftwareTerrainError::Custody { .. },
        ) => true,
        // Native geometry and existing unowned services retain per-strip
        // containment. Diagnostic strings never classify renderer custody.
        _ => false,
    }
}

impl SceneTerrainHost<'_, '_, '_, '_> {
    fn required_entry(&self) -> Result<(), SoftwareTerrainError> {
        if self.first_hard.is_some() {
            Err(SoftwareSceneError::Stopped.into())
        } else {
            Ok(())
        }
    }
}

impl<'r> TerrainDrawHost<'r> for SceneTerrainHost<'_, '_, '_, 'r> {
    type Error = SoftwareTerrainError;

    fn material(&mut self, slot: i16) -> Result<u32, Self::Error> {
        self.required_entry()?;
        // Port option: under the water the near, water and far-strip fog
        // take the water's colour.
        if let (Some(fog), 0x73 | 0x74) = (self.scene.water_fog, slot) {
            return Ok(fog);
        }
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument: i32::from(slot),
        };
        match self.scene.resources.resolve(request)?.data {
            RenderResourceData::Material(value) => {
                Ok(self.scene.color_layout.convert_material(value))
            }
            _ => Err(RenderResourceError::Registration { request }.into()),
        }
    }

    fn texture(&mut self, slot: i16) -> Result<Option<RenderResource<'r>>, Self::Error> {
        self.required_entry()?;
        //00405540 returns null only outside its inclusive signed low-WORD
        // bound. An accepted vacant slot reaches the native lazy loader.
        // Missing loader custody and the unsafe index==capacity access fail
        // explicitly; never replace either with a fabricated nullable texture.
        if slot < 0
            || slot as usize
                > self
                    .scene
                    .resources
                    .pool_capacity(RenderResourceKind::Sprite)
        {
            return Ok(None);
        }
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: i32::from(slot),
        };
        self.scene
            .resources
            .resolve(request)
            .map(Some)
            .map_err(Into::into)
    }

    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.required_entry()?;
        Ok(self.scene.material_sorting())
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        self.required_entry()?;
        Ok(self.scene.texture_identities.resolve(texture)?)
    }
    fn publish_far_color(&mut self, color: u32) -> Result<(), Self::Error> {
        self.required_entry()?;
        self.scene.submit(ResolvedSceneCommand::FarColor(color))?;
        Ok(())
    }
    fn publish_fade(&mut self, fade: [i32; 2]) -> Result<(), Self::Error> {
        self.required_entry()?;
        self.scene.submit(ResolvedSceneCommand::PublishFade(
            crate::frontend_scene::SceneFade::new(fade),
        ))?;
        Ok(())
    }
    fn begin_timing(&mut self) -> Result<(), Self::Error> {
        self.required_entry()?;
        //004395A0 is a profiling marker; it never resets the draw queue.
        // Independent-feature containment explicitly uses RegisteredMarkUnwindApproximation
        // through continue_after_feature_error; this is not native profiling cleanup.
        self.scene.continue_after_feature_error()?;
        Ok(())
    }
    fn end_timing(&mut self) -> Result<(), Self::Error> {
        self.required_entry()?;
        //004395C0 closes profiling; the caller owns the eventual flush.
        Ok(())
    }
    fn furniture(&mut self, request: TerrainFurnitureRequest<'_>) -> Result<(), Self::Error> {
        self.required_entry()?;
        let _scope = crate::cpu_profile::scope("terrain: furniture");
        furniture::draw_furniture(
            self.scene,
            request,
            &mut self.furniture,
            &mut self.furniture_report,
        )
        .map_err(|error| match error {
            furniture::FurniturePhaseError::Scene(error) => SoftwareTerrainError::Scene(error),
            furniture::FurniturePhaseError::Custody { source } => {
                SoftwareTerrainError::Custody { source }
            }
            furniture::FurniturePhaseError::Feature(reason) => {
                SoftwareTerrainError::Furniture(reason)
            }
        })
    }
    fn particles(
        &mut self,
        request: TerrainParticleEmissionRequest<'_>,
    ) -> Result<(), Self::Error> {
        self.required_entry()?;
        let _scope = crate::cpu_profile::scope("terrain: particles");
        let mut rng = self
            .scene
            .process_rng()
            .ok_or(SoftwareTerrainError::Unowned(
                "0044C4A0 retained process RNG",
            ))?;
        let executable = self.furniture.executable;
        let particles = &mut *self.furniture.particles;
        let result = emit_terrain_particles(
            request,
            self.furniture.birth_inputs,
            &mut rng,
            |birth, rng| {
                particles
                    .birth_trail(birth, executable, rng)
                    .map(|_| ())
                    .map_err(|error| SoftwareTerrainError::Particle(error.to_string()))
            },
        );
        // Preserve all reached row/selector/position/velocity and birth RNG
        // writes even when the containing strip stops at a service boundary.
        self.scene.set_process_rng(Some(rng));
        result.map(|_| ())
    }
    fn water(
        &mut self,
        request: TerrainStripRequest<'_>,
    ) -> Result<TerrainDrawOutcome, Self::Error> {
        self.required_entry()?;
        // Port option: under the water its surface is not drawn from below;
        // it fades into the murk as the far ground does.
        if self.scene.water_fog.is_some() {
            return Ok(TerrainDrawOutcome::default());
        }
        let _scope = crate::cpu_profile::scope("terrain: water strips");
        let before = self.scene.stats.terrain_commands;
        self.water = true;
        let result = draw_water_strip(request, self);
        self.water = false;
        let result = result.map_err(|error| match error {
            TerrainDrawError::Fault(error) => error.into(),
            TerrainDrawError::Host(error) => error,
        });
        // Count actual deferred allocations even when the next tile fails.
        self.scene.stats.water_commands += self.scene.stats.terrain_commands - before;
        result
    }
    fn clipped_tile(&mut self, request: TerrainClipRequest<'r>) -> Result<usize, Self::Error> {
        self.required_entry()?;
        let tables = self.scene.tables;
        clip_terrain_tile(request, tables, self).map_err(|error| match error {
            TerrainDrawError::Fault(error) => error.into(),
            TerrainDrawError::Host(error) => error,
        })
    }
    fn submit_quad(&mut self, packet: TerrainQuadCommand<'r>) -> Result<(), Self::Error> {
        self.required_entry()?;
        let callback = match packet.callback {
            TerrainQuadCallback::TexturedNear => PrimitiveCallback::D6QuadNear,
            TerrainQuadCallback::TexturedFar => PrimitiveCallback::D6QuadFar,
            TerrainQuadCallback::ColoredNear => PrimitiveCallback::ColoredQuadNear,
            TerrainQuadCallback::ColoredFar => PrimitiveCallback::ColoredQuadFar,
        };
        let mut command = PrimitiveCommand::new(callback, packet.depth, 4);
        command.payload.copy_from_slice(&packet.payload);
        command.scene_fed = self.traced_ground();
        command.water = self.water;
        command.ground = !self.water;
        if let Some(texture) = packet.texture {
            command.write_texture(0x24, Some(texture));
        }
        self.scene
            .queue
            .submit(DrawSubmission {
                depth: packet.depth,
                payload_blocks: 4,
                receiver_offset: 0,
                command,
            })
            .map_err(SoftwareSceneError::Queue)?;
        self.scene.stats.terrain_commands += 1;
        Ok(())
    }
    fn omitted(&mut self, feature: TerrainFeature, error: TerrainDrawError<Self::Error>) {
        if self.first_hard.is_some() {
            return;
        }
        if terrain_error_is_hard(&error) {
            self.scene.latch_stop(&error);
            self.first_hard = Some(error);
            return;
        }
        // 004923C0 disposes the strip's error; after a full arena the next
        // strip allocates what still fits.
        self.scene.recover_native_301();
        let reason = match error {
            TerrainDrawError::Host(error) => error.to_string(),
            TerrainDrawError::Fault(error) => error.to_string(),
        };
        if feature == TerrainFeature::WaterStrip
            && self.scene.water_omissions.insert(reason.clone())
        {
            // A newly reached water failure must be reported even when this
            // is not the caller's first screen/frame. Renderer epochs reset it.
            eprintln!("[terrain] skipped water strip: {reason}");
        }
        if !self
            .omissions
            .iter()
            .any(|old| old.feature == feature && old.reason == reason)
        {
            self.omissions.push(TerrainOmission { feature, reason });
        }
    }
}

impl<'r> TerrainClippingHost<'r> for SceneTerrainHost<'_, '_, '_, 'r> {
    type Error = SoftwareTerrainError;
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.required_entry()?;
        Ok(self.scene.material_sorting())
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        self.required_entry()?;
        Ok(self.scene.texture_identities.resolve(texture)?)
    }
    fn submit(&mut self, mut command: PrimitiveCommand<'r>) -> Result<(), Self::Error> {
        self.required_entry()?;
        command.scene_fed = self.traced_ground();
        command.water = self.water;
        command.ground = !self.water;
        self.scene
            .queue
            .submit(DrawSubmission {
                depth: command.depth,
                payload_blocks: command.payload_blocks,
                receiver_offset: 0,
                command,
            })
            .map_err(SoftwareSceneError::Queue)?;
        // Count actual queue commands, including a prefix before later failure.
        self.scene.stats.terrain_commands += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
