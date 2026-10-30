//! Resource-backed frontend submission adapter. Native scene math is upstream;
//! actual rasterization and Handler-8 program execution are required downstream.

use inf_formats::{ModelEntry, SpriteRenderMode};
use thiserror::Error;

use crate::frontend_scene::{
    FooterQuad, FrontendSceneHost, SceneCommand, SceneFade, SceneModelSubmission, SceneSetup,
    SceneView, StarQuad,
};
use crate::render_resources::{
    NativeColorLayout, RenderResourceData, RenderResourceError, RenderResourceKind,
    RenderResourceRequest, RenderResources, RenderSource, RenderSprite,
};

#[derive(Clone, Copy, Debug)]
pub enum ResolvedSceneCommand<'a> {
    CoordinateShift(u8),
    DirectionalLight {
        direction: [f32; 3],
        rgb: [u8; 3],
    },
    AmbientLight([i32; 3]),
    Setup(SceneSetup),
    PublishFade(SceneFade),
    FarColor(u32),
    BeginBatch,
    FlushBatch,
    StarView(SceneView),
    /// +20=0; +24 is the converted H7 resource DWORD, not source RGB555.
    Star {
        quad: StarQuad,
        source: RenderSource<'a>,
        color_word: u32,
    },
    PublishView(SceneView),
    Model {
        submission: SceneModelSubmission,
        source: RenderSource<'a>,
        model: &'a ModelEntry,
    },
    ResetAfterFirstModel,
    Footer {
        quad: FooterQuad,
        sprite: RenderSprite<'a>,
    },
}

pub trait SceneResourceBackend<'a> {
    type Error;
    /// Read the selected backend's live display policy at each resource use.
    fn color_layout(&self) -> NativeColorLayout;
    fn sprite_mode(&self) -> SpriteRenderMode;
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error>;
    /// Every accepted operation must execute its native effect. In particular,
    /// Model requires a runtime interpreter; a static diagnostic mesh is not it.
    fn submit(&mut self, command: ResolvedSceneCommand<'a>) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum SceneResourceError<E> {
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error("scene backend operation failed")]
    Backend(E),
}

/// Scoped to one resident state and one explicitly selected backend. The frame
/// owner must obtain fresh backend identities at native lookup boundaries.
pub struct ResidentSceneHost<'r, 'b, B> {
    pub resources: RenderResources<'r>,
    pub backend: &'b mut B,
}

impl<'a, B: SceneResourceBackend<'a>> FrontendSceneHost for ResidentSceneHost<'a, '_, B> {
    type Error = SceneResourceError<B::Error>;
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        self.backend
            .surface_pixels()
            .map_err(SceneResourceError::Backend)
    }
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        let resolved = match command {
            SceneCommand::CoordinateShift(value) => ResolvedSceneCommand::CoordinateShift(value),
            SceneCommand::DirectionalLight { direction, rgb } => {
                ResolvedSceneCommand::DirectionalLight { direction, rgb }
            }
            SceneCommand::AmbientLight(rgb) => ResolvedSceneCommand::AmbientLight(rgb),
            SceneCommand::Setup(setup) => ResolvedSceneCommand::Setup(setup),
            SceneCommand::PublishFade(fade) => ResolvedSceneCommand::PublishFade(fade),
            SceneCommand::FarColor(color) => ResolvedSceneCommand::FarColor(color),
            SceneCommand::BeginBatch => ResolvedSceneCommand::BeginBatch,
            SceneCommand::FlushBatch => ResolvedSceneCommand::FlushBatch,
            SceneCommand::StarView(view) => ResolvedSceneCommand::StarView(view),
            SceneCommand::PublishView(view) => ResolvedSceneCommand::PublishView(view),
            SceneCommand::ResetAfterFirstModel => ResolvedSceneCommand::ResetAfterFirstModel,
            SceneCommand::Star(quad) => {
                let resource = self.resources.resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Material,
                    argument: quad.color_argument,
                })?;
                let RenderResourceData::Material(value) = resource.data else {
                    unreachable!("typed material lookup")
                };
                ResolvedSceneCommand::Star {
                    quad,
                    source: resource.source,
                    color_word: self.backend.color_layout().convert_material(value),
                }
            }
            SceneCommand::Model(submission) => {
                let resource = self.resources.resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: i32::from(submission.model_slot),
                })?;
                let RenderResourceData::Model(model) = resource.data else {
                    unreachable!("typed model lookup")
                };
                ResolvedSceneCommand::Model {
                    submission,
                    source: resource.source,
                    model,
                }
            }
            SceneCommand::Footer(quad) => {
                let resource = self.resources.resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: i32::from(quad.sprite_slot),
                })?;
                ResolvedSceneCommand::Footer {
                    quad,
                    sprite: resource.select_sprite(self.backend.sprite_mode())?,
                }
            }
        };
        self.backend
            .submit(resolved)
            .map_err(SceneResourceError::Backend)
    }

    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        let resource = self.resources.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Style,
            argument: 14,
        })?;
        let RenderResourceData::Style(word) = resource.data else {
            unreachable!("typed style lookup")
        };
        Ok(word)
    }

    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Self::Error> {
        Ok(self
            .resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: i32::from(slot),
            })?
            .sprite_dimensions()?)
    }
}
