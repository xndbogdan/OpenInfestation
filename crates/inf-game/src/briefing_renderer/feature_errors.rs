//! Typed briefing feature containment with explicit queue/scratch ownership.
//! Known unowned producers retain their reached prefix; renderer custody stays
//! hard. Portable model-group/mark retirement is not native error unwind.

use super::{
    BriefingRendererError, MapMarkerDrawError, SoftwareBriefingFrame, StartTextError,
    TerrainStaticDrawError,
};
use crate::{
    briefing_terrain::BriefingTerrainError,
    menu::MenuError,
    menu_text::MenuTextError,
    model_commands::ModelCommandError,
    model_execution::ModelExecutionError,
    model_primitives::PrimitiveError,
    model_values::{ModelValueError, ModelValueFault},
    model_vertices::{ModelCustomVertexError, ModelVertexError, ModelVertexFault},
    render_resources::RenderResourceError,
    scene_resources::SceneResourceError,
    software_menu::{SoftwareGlyphError, SoftwareMenuError},
    software_scene::{SoftwareSceneError, SoftwareTerrainError},
    terrain_draw::TerrainDrawError,
};

pub(super) type FeatureSource = Box<dyn std::error::Error>;

/// ProducerOnly cannot retire stopped draw marks. CurrentModel requires an
/// authenticated implicit-root entry and current-draw scope ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FeatureRecovery {
    ProducerOnly,
    CurrentModel,
}

pub(super) enum FeatureDecision {
    Omit {
        source: FeatureSource,
        recovery: FeatureRecovery,
    },
    Hard(BriefingRendererError),
}

/// One data-bearing adapter for genuine producer wrappers currently in use.
#[derive(Debug, thiserror::Error)]
pub(super) enum FeatureError {
    #[error(transparent)]
    Renderer(#[from] BriefingRendererError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Resident(#[from] SceneResourceError<SoftwareSceneError>),
    #[error(transparent)]
    Menu(#[from] SoftwareMenuError),
    #[error(transparent)]
    MenuDraw(#[from] MenuError<SoftwareMenuError>),
    #[error(transparent)]
    Start(#[from] StartTextError),
    #[error(transparent)]
    Static(#[from] TerrainStaticDrawError),
    #[error(transparent)]
    Marker(#[from] MapMarkerDrawError),
    #[error(transparent)]
    Terrain(#[from] TerrainDrawError<SoftwareTerrainError>),
    #[error(transparent)]
    Pane(#[from] BriefingTerrainError),
}

fn hard(source: impl std::error::Error + 'static) -> FeatureDecision {
    FeatureDecision::Hard(BriefingRendererError::Feature {
        source: Box::new(source),
    })
}

fn omit(source: impl std::error::Error + 'static, recovery: FeatureRecovery) -> FeatureDecision {
    FeatureDecision::Omit {
        source: Box::new(source),
        recovery,
    }
}

pub(super) fn resource_is_unowned(error: &RenderResourceError) -> bool {
    matches!(
        error,
        RenderResourceError::NotResident { .. } | RenderResourceError::SpriteVariant { .. }
    )
}

fn value_is_unowned<C>(error: &ModelValueError<C>) -> bool {
    matches!(
        error,
        ModelValueError::Fault(
            ModelValueFault::UninitializedRegister { .. }
                | ModelValueFault::ContextAliasedRegister { .. }
                | ModelValueFault::UnsupportedOpcode { .. }
        )
    )
}

fn vertex_fault_is_unowned(error: &ModelVertexFault) -> bool {
    matches!(
        error,
        ModelVertexFault::UnwrittenCustomVertex { .. }
            | ModelVertexFault::UnownedCustomVertex
            | ModelVertexFault::UnsupportedOpcode { .. }
    )
}

fn vertex_is_unowned<C, P>(error: &ModelVertexError<C, P>) -> bool {
    match error {
        ModelVertexError::Fault(error)
        | ModelVertexError::Custom(ModelCustomVertexError::Fault(error)) => {
            vertex_fault_is_unowned(error)
        }
        ModelVertexError::Value(error) => value_is_unowned(error),
        // Projection and required host/callback failures remain hard.
        _ => false,
    }
}

fn model_is_unowned<E>(error: &ModelExecutionError<ModelCommandError<E>>) -> bool {
    match error {
        ModelExecutionError::Unsupported { .. } => true,
        ModelExecutionError::Host(ModelCommandError::Primitive(PrimitiveError::Unsupported {
            ..
        })) => true,
        ModelExecutionError::Value(error) => value_is_unowned(error),
        ModelExecutionError::Vertex(error) => vertex_is_unowned(error),
        // Required child/command/service resource failures are not broadened
        // into omissions. Record, ranges, branch, limit and stopped are hard.
        _ => false,
    }
}

impl FeatureError {
    pub(super) fn classify(self) -> FeatureDecision {
        match self {
            Self::Renderer(BriefingRendererError::Scene(error)) => Self::Scene(error).classify(),
            Self::Renderer(error) => FeatureDecision::Hard(error),
            Self::Resource(error) => {
                if resource_is_unowned(&error) {
                    omit(error, FeatureRecovery::ProducerOnly)
                } else {
                    hard(error)
                }
            }
            Self::Scene(error) => {
                if let SoftwareSceneError::Resource(resource) = &error {
                    if resource_is_unowned(resource) {
                        return omit(error, FeatureRecovery::ProducerOnly);
                    }
                }
                if let SoftwareSceneError::Model(model) = &error {
                    if model_is_unowned(model) {
                        return omit(error, FeatureRecovery::CurrentModel);
                    }
                }
                // Preserve native301 and every hard scene wrapper unchanged.
                FeatureDecision::Hard(BriefingRendererError::Scene(error))
            }
            Self::Resident(SceneResourceError::Resource(error)) => Self::Resource(error).classify(),
            Self::Resident(SceneResourceError::Backend(error)) => Self::Scene(error).classify(),
            Self::Menu(SoftwareMenuError::Scene(error)) => Self::Scene(error).classify(),
            Self::Menu(SoftwareMenuError::Text(error)) => match error {
                MenuTextError::Resource(error)
                | MenuTextError::Backend(SoftwareGlyphError::Resource(error)) => {
                    Self::Resource(error).classify()
                }
                MenuTextError::Backend(SoftwareGlyphError::Pixel(error)) => {
                    Self::Scene(SoftwareSceneError::Pixel(error)).classify()
                }
                error @ MenuTextError::UnsupportedCallback(_) => {
                    omit(error, FeatureRecovery::ProducerOnly)
                }
                error => hard(error),
            },
            Self::Menu(error) => hard(error),
            Self::MenuDraw(MenuError::Host(error)) => Self::Menu(error).classify(),
            Self::MenuDraw(error) => hard(error),
            Self::Start(StartTextError::Resource(error)) => Self::Resource(error).classify(),
            Self::Start(StartTextError::Menu(error)) => Self::Menu(error).classify(),
            Self::Start(error) => hard(error),
            Self::Static(TerrainStaticDrawError::Scene(error)) => Self::Scene(error).classify(),
            Self::Static(TerrainStaticDrawError::Noise(
                crate::briefing_terrain::BriefingNoiseError::Resource(error),
            )) => Self::Resource(error).classify(),
            Self::Static(error) => hard(error),
            Self::Marker(MapMarkerDrawError::Resource(error)) => Self::Resource(error).classify(),
            Self::Marker(MapMarkerDrawError::Marker(
                crate::briefing_markers::BriefingMarkerError::Resource(error),
            )) => Self::Resource(error).classify(),
            Self::Marker(MapMarkerDrawError::Annotation(
                crate::briefing_map_annotations::BriefingAnnotationError::Resource(error),
            )) => Self::Resource(error).classify(),
            Self::Marker(MapMarkerDrawError::Menu(error)) => Self::Menu(error).classify(),
            Self::Marker(MapMarkerDrawError::Scene(error)) => Self::Scene(error).classify(),
            Self::Marker(error) => hard(error),
            Self::Terrain(TerrainDrawError::Host(SoftwareTerrainError::Scene(error))) => {
                Self::Scene(error).classify()
            }
            Self::Terrain(TerrainDrawError::Host(SoftwareTerrainError::Resource(error))) => {
                Self::Resource(error).classify()
            }
            // Returned native arithmetic/ranges, opaque furniture/particle,
            // required texture identities and unknown services stay hard.
            Self::Terrain(error) => hard(error),
            Self::Pane(error) => hard(error),
        }
    }
}

/// Captured custody facts before an admitted producer, independently of its
/// later model callback or direct-pixel failure. Root closure is a queue fact.
pub(super) struct FeatureEntry {
    root_was_open: bool,
    open_groups: usize,
    scratch_was_ready: bool,
    scene_was_running: bool,
}

impl SoftwareBriefingFrame<'_, '_, '_> {
    pub(super) fn feature_entry(&self) -> Result<FeatureEntry, SoftwareSceneError> {
        Ok(FeatureEntry {
            root_was_open: self.scene.queue.has_only_implicit_root(),
            open_groups: self.scene.queue.state().open_groups,
            scratch_was_ready: matches!(
                self.scene
                    .scratch
                    .state()
                    .map_err(SoftwareSceneError::Scratch)?,
                crate::model_scratch::ModelScratchDrawState::Ready
            ),
            scene_was_running: !self.scene.stopped(),
        })
    }

    pub(super) fn omit_feature(
        &mut self,
        label: &str,
        entry: FeatureEntry,
        error: impl Into<FeatureError>,
    ) -> Result<(), BriefingRendererError> {
        let (source, recovery) = match error.into().classify() {
            FeatureDecision::Hard(error) => return Err(error),
            FeatureDecision::Omit { source, recovery } => (source, recovery),
        };
        if !self.scene.feature_continuation_allowed() {
            return Err(SoftwareSceneError::Stopped.into());
        }
        if !entry.scene_was_running || !entry.scratch_was_ready {
            return Err(BriefingRendererError::Feature { source });
        }
        match recovery {
            FeatureRecovery::ProducerOnly => {
                // The native immediate tail follows a successfully closed
                // decoration root. Both closed and sole-implicit entry are valid;
                // no producer-only omission may mutate group or scratch custody.
                if (!entry.root_was_open && entry.open_groups != 0)
                    || self.scene.queue.state().open_groups != entry.open_groups
                    || !matches!(
                        self.scene
                            .scratch
                            .state()
                            .map_err(SoftwareSceneError::Scratch)?,
                        crate::model_scratch::ModelScratchDrawState::Ready
                    )
                {
                    return Err(BriefingRendererError::Feature { source });
                }
            }
            FeatureRecovery::CurrentModel => {
                if !entry.root_was_open || self.scene.queue.state().open_groups == 0 {
                    return Err(BriefingRendererError::Feature { source });
                }
                // BriefingModelGroupClosureApproximation: only groups created
                // above this producer's proven implicit-root entry are closed.
                // end_group owns actual sort/copy/pop; payloads are not replayed
                // or reset, and native model completion is not fabricated.
                while self.scene.queue.state().open_groups > 1 {
                    self.scene
                        .queue
                        .end_group()
                        .map_err(SoftwareSceneError::Queue)?;
                }
            }
        }
        // CurrentModel alone may retire its stopped registered draw marks.
        // ProducerOnly reached this call only with Ready scratch, retaining it.
        self.scene.continue_after_feature_error()?;
        eprintln!("briefing draw: {label} remainder omitted after reached writes ({source}; BriefingModelFeatureOmissionApproximation; BRIEFING_RUNTIME.md)");
        Ok(())
    }
}
