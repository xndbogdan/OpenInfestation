//! Concrete frontend scene state, native model commands and CPU pixels.
//!
//! Display/camera inputs are supplied explicitly. The CPU target implements
//! the Direct3D consumer policy; it is not the retail software renderer.

use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static NEXT_RENDERER: AtomicU64 = AtomicU64::new(1);

mod current_resources;
mod furniture;
mod object_models;
mod pickup;
pub use pickup::SoftwarePickupServices;
mod barrier;
mod flash;
mod force_field;
mod gate;
mod laser;
mod particles;
mod ribbon;
mod terrain;
pub use current_resources::{
    CurrentResourceDraw, ResourceBatchPolicy, ResourceLoan, ResourceLoanError,
    ResourceLoanPhaseError,
};
pub use flash::SoftwareFlashError;
pub use furniture::{
    FurnitureSceneOmission, FurnitureSceneReport, SoftwareFurnitureInputs,
    SoftwareTerrainDrawRequest,
};
pub use object_models::{
    SoftwareCustomWorld, SoftwareObjectDrawHost, SoftwareObjectEffectServices,
    SoftwareObjectModelDrawRequest,
};
pub use particles::SoftwareParticleError;
pub use terrain::{SceneTerrainOutcome, SoftwareTerrainError, TerrainOmission};

/// Process-owned identity, never a native pointer or fabricated address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoftwareRendererId(u64);

use inf_formats::SpriteRenderMode;
use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    draw_queue::{DrawInvocation, DrawQueue, DrawQueueError, DrawQueueFault, DrawSubmission},
    frontend_scene::{
        FooterQuad, ModelFailurePolicy, ModelPrimitiveContext, ModelSurfaceTableOrigin,
        SceneModelSubmission, StarQuad,
    },
    model_commands::{ModelCommandBackend, ModelCommandError, ModelCommandServices},
    model_context::{ModelContext, ModelProjection},
    model_execution::{
        ModelDrawRequest, ModelEnvironment, ModelExecutionError, ModelExecutionLimits,
        ModelExecutor,
    },
    model_primitives::{
        PrimitiveCallback, PrimitiveCommand, PrimitiveState, PrimitiveTables, TextureSortIdentity,
        MAX_PORTABLE_TEXTURE_ENTRY_ORDINAL,
    },
    model_scratch::{
        ModelScratchDrawState, ModelScratchError, ProcessModelScratch,
        RegisteredMarkUnwindApproximation,
    },
    model_values::{ModelValueCallback, ModelValueError, ModelValueFault, ModelValues},
    pixel_backend::{flush_pixels, PixelBackend},
    render_resources::{
        NativeColorLayout, RenderResource, RenderResourceData, RenderResourceError,
        RenderResourceKind, RenderResourceRequest, RenderResources, RenderSource, RenderSprite,
    },
    scene_quad::{DirectFlatQuad, DirectShadeUvQuad},
    scene_resources::{ResolvedSceneCommand, SceneResourceBackend},
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
    software_frame::{
        D3dRasterState, SoftwareFrame, SoftwarePixelBackend, SoftwarePixelError,
        SoftwarePixelStats, SoftwareTextureState,
    },
    terrain_runtime::TerrainRuntime,
};

/// Inherited renderer/globals not supplied by 00483C60. In particular, initial
/// ambient/effect words are live state: the light setter uses the OLD ambient
/// before the scene changes it. These values are not inferred from viewport size.
#[derive(Clone, Copy)]
pub struct SoftwareSceneConfig {
    pub environment: ModelEnvironment,
    pub primitive_mode: u32,
    pub far_color: u32,
    pub effect_color: u32,
    pub color_layout: NativeColorLayout,
    pub renderer: D3dFlatState,
    pub pipeline: D3dRasterState,
    pub texture_state: SoftwareTextureState,
    pub queue_capacity: u32,
    pub execution_limits: ModelExecutionLimits,
}

pub struct SoftwareSceneRequest<'f, 'r> {
    /// Same process byte owner across renderer epochs and resource loans.
    pub model_process: crate::model_process::ModelProcessOwners,
    pub resources: RenderResources<'r>,
    pub tables: &'r PrimitiveTables,
    pub frame: &'f mut SoftwareFrame,
    pub config: SoftwareSceneConfig,
}

/// A renderer between residency snapshots, without borrowed custody. See
/// [`SoftwareScene::into_state`].
pub struct SoftwareSceneState {
    scratch: ProcessModelScratch,
    graphics: crate::process_model_graphics::ProcessModelGraphics,
    id: SoftwareRendererId,
    execution_limits: ModelExecutionLimits,
    color_layout: NativeColorLayout,
    queue_capacity: u32,
    pub environment: ModelEnvironment,
    pub primitive_state: PrimitiveState,
    pub renderer: D3dFlatState,
    pub pipeline: D3dRasterState,
    pub texture_state: SoftwareTextureState,
    pub stats: SoftwareSceneStats,
    pub pixel_stats: SoftwarePixelStats,
    pub terrain: Option<TerrainRuntime>,
    surface_callbacks: RendererSurfaceCallbacks,
    furniture_omissions: HashSet<(u16, usize, String)>,
    water_omissions: HashSet<String>,
    stopped: bool,
    aborted_presentation: bool,
    canvas_viewport: Option<[i16; 4]>,
}

impl SoftwareSceneState {
    pub fn id(&self) -> SoftwareRendererId {
        self.id
    }
}

pub struct SoftwareSceneResume<'f, 'r> {
    pub resources: RenderResources<'r>,
    pub tables: &'r PrimitiveTables,
    pub frame: &'f mut SoftwareFrame,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftwareSceneStats {
    /// CompletedResourceBatchRetirementApproximation, not native arena frees.
    pub retired_resource_batches: usize,
    pub frame_begins: usize,
    pub presentation_clears: usize,
    pub begins: usize,
    pub flushes: usize,
    pub models: usize,
    /// UninitializedValuePrefixApproximation, independently of complete models.
    pub model_value_prefix_skips: usize,
    /// ObjectDrawGroupClosureApproximation; no native callback/completion claim.
    pub object_group_prefix_closures: usize,
    pub stars: usize,
    pub footers: usize,
    /// Direct renderer +9C/+A4 submissions from any scene owner.
    pub direct_sprites: usize,
    /// Direct renderer +84 submissions, independently of deferred flat quads.
    pub direct_flat_quads: usize,
    /// Direct renderer +C4 submissions (startup carousel cloths).
    pub direct_shade_uv_quads: usize,
    /// Native terrain packets enqueued, including a committed prefix before
    /// an unsupported terrain/model service is reached.
    pub terrain_commands: usize,
    /// Subset of terrain commands submitted by native water, including
    /// committed prefixes before a later reached water-strip failure.
    pub water_commands: usize,
    /// Renderer virtual+14(NULL) calls, including the outer frame reset.
    pub depth_resets: usize,
    pub model_commands: usize,
    pub rejected_primitives: usize,
}

#[derive(Debug, Error)]
pub enum SoftwareSceneServiceError {
    #[error(transparent)]
    ObjectModelCallback(#[from] crate::object_runtime::ObjectModelCallbackError),
    #[error("captured object {object:?} custom selector {selector} (00478950): {reason}")]
    ObjectCustomVertex {
        object: crate::object_runtime::ObjectId,
        selector: i32,
        reason: crate::object_runtime::ObjectCustomVertexError,
    },
    #[error("model value selector {selector} reaches a pure virtual callback (005116FF)")]
    PureVirtualCallback { selector: u32 },
    #[error("opaque texture identity requires a Handler-3 entry")]
    TextureResource,
    #[error("portable opaque texture identity capacity exhausted")]
    TextureCapacity,
    #[error("furniture packed query has no live sparse record {record:?}")]
    FurnitureRecord {
        record: crate::terrain_runtime::TerrainSparseRecordId,
    },
    #[error("furniture model callback requires the shared process RNG")]
    FurnitureRng,
    #[error(transparent)]
    FurnitureBirth(
        #[from] crate::process_particles::births::ParticleBirthError<std::convert::Infallible>,
    ),
}

#[derive(Debug, Error)]
pub enum SoftwareSceneError {
    #[error(transparent)]
    Process(#[from] crate::process_model_graphics::ProcessModelGraphicsError),
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Queue(#[from] DrawQueueFault),
    #[error(transparent)]
    Flush(#[from] DrawQueueError<SoftwarePixelError>),
    #[error(transparent)]
    Pixel(#[from] SoftwarePixelError),
    #[error(transparent)]
    Model(#[from] ModelExecutionError<ModelCommandError<SoftwareSceneServiceError>>),
    #[error("model {model_slot} stopped after {reason:?}; entry_root={root_was_open}, open_groups={open_groups}; object group recovery is unavailable")]
    ObjectModelGroupPrefix {
        model_slot: i16,
        root_was_open: bool,
        open_groups: usize,
        reason: crate::object_runtime::ObjectDrawCallbackBoundary,
    },
    #[error("resolved scene resource does not belong to this backend's resident registration")]
    Custody,
    #[error("model material conversion currently requires canonical RGB555 display shifts")]
    ColorLayout,
    #[error("scene backend stopped after an earlier failure; begin a new batch before reuse")]
    Stopped,
    #[error("renderer +10 depth policy takes only (0,0) or (1,1), not ({enable},{write})")]
    DepthPolicy { enable: u32, write: u32 },
}

impl SoftwareSceneError {
    /// Byte-pool identity/range/borrow faults are hard custody errors.
    /// Existing unsupported model features keep their separate omission policy.
    /// A signed shade reference below every mapped byte the port models.
    /// The read is refused before anything is written, so the arena's
    /// custody is intact: it stops only its model, as a feature omission.
    pub(crate) fn is_unmapped_shade_read(&self) -> bool {
        use crate::model_lighting::ModelLightingError::Scratch;
        use crate::model_scratch::ModelScratchError::MappedShade;
        matches!(
            self,
            Self::Model(ModelExecutionError::Lighting(Scratch(MappedShade { .. })))
                | Self::Model(ModelExecutionError::Host(ModelCommandError::Primitive(
                    crate::model_primitives::PrimitiveError::Lighting(Scratch(MappedShade { .. }))
                )))
        )
    }

    pub(crate) fn is_model_scratch_fault(&self) -> bool {
        if self.is_unmapped_shade_read() {
            return false;
        }
        matches!(
            self,
            Self::Scratch(_)
                | Self::Model(ModelExecutionError::Scratch(_))
                | Self::Model(ModelExecutionError::Lighting(
                    crate::model_lighting::ModelLightingError::Scratch(_)
                        | crate::model_lighting::ModelLightingError::ScratchAllocation { .. }
                ))
                | Self::Model(ModelExecutionError::Vertex(
                    crate::model_vertices::ModelVertexError::Fault(
                        crate::model_vertices::ModelVertexFault::Scratch(_)
                    )
                ))
                | Self::Model(ModelExecutionError::Vertex(
                    crate::model_vertices::ModelVertexError::Custom(
                        crate::model_vertices::ModelCustomVertexError::Fault(
                            crate::model_vertices::ModelVertexFault::Scratch(_)
                        )
                    )
                ))
                | Self::Model(ModelExecutionError::Host(ModelCommandError::Primitive(
                    crate::model_primitives::PrimitiveError::Vertex(
                        crate::model_vertices::ModelVertexFault::Scratch(_)
                    ) | crate::model_primitives::PrimitiveError::Lighting(
                        crate::model_lighting::ModelLightingError::Scratch(_)
                            | crate::model_lighting::ModelLightingError::ScratchAllocation { .. }
                    )
                )))
        )
    }
    /// Hard renderer custody is distinct from existing per-model omissions.
    /// Furniture and terrain retain typed queue/pixel/pool failures; their
    /// unowned callback and geometry policies remain separate producer owners.
    pub(crate) fn is_renderer_custody_fault(&self) -> bool {
        if self.is_unmapped_shade_read() {
            return false;
        }
        if self.is_model_scratch_fault() {
            return true;
        }
        match self {
            Self::Model(ModelExecutionError::Host(error))
            | Self::Model(ModelExecutionError::Value(ModelValueError::Callback(error)))
            | Self::Model(ModelExecutionError::Vertex(
                crate::model_vertices::ModelVertexError::Value(ModelValueError::Callback(error)),
            ))
            | Self::Model(ModelExecutionError::Vertex(
                crate::model_vertices::ModelVertexError::Custom(
                    crate::model_vertices::ModelCustomVertexError::Host(error),
                ),
            )) => Self::model_command_is_custody_fault(error),
            Self::Model(ModelExecutionError::Resource(error)) | Self::Resource(error) => !matches!(
                error,
                RenderResourceError::NotResident { .. } | RenderResourceError::SpriteVariant { .. }
            ),
            // Existing model callback/value/geometry omissions have their own
            // guarded policies. This predicate never authorizes their recovery.
            Self::Model(_) => false,
            _ => true,
        }
    }

    fn model_command_is_custody_fault(
        error: &ModelCommandError<SoftwareSceneServiceError>,
    ) -> bool {
        match error {
            ModelCommandError::Primitive(
                crate::model_primitives::PrimitiveError::Unsupported { .. },
            ) => false,
            ModelCommandError::Service(error) => matches!(
                error,
                SoftwareSceneServiceError::TextureResource
                    | SoftwareSceneServiceError::TextureCapacity
                    | SoftwareSceneServiceError::FurnitureRecord { .. }
                    | SoftwareSceneServiceError::FurnitureRng
            ),
            ModelCommandError::Resource(error) => !matches!(
                error,
                RenderResourceError::NotResident { .. } | RenderResourceError::SpriteVariant { .. }
            ),
            _ => true,
        }
    }

    /// Only native queue faults carry native exception codes. Keep model
    /// execution wrappers so an arena overflow reaches the outer301 catch.
    pub fn native_error_code(&self) -> Option<u32> {
        match self {
            Self::Queue(fault)
            | Self::Flush(DrawQueueError::Fault(fault))
            | Self::Model(ModelExecutionError::Host(ModelCommandError::Queue(fault))) => {
                fault.native_error_code()
            }
            _ => None,
        }
    }
}

/// First-use allocation order is an explicit portable backend policy. The
/// retained borrows prevent residency/source reuse while identities are live.
/// Aliased slots for the same source entry share an ordinal; identical bytes
/// in another overlay remain a separate allocation. Coplanar opaque overlap
/// still requires retail image comparison because native allocation order differs.
#[derive(Clone, Default)]
struct TextureIdentities<'r> {
    entries: Vec<RenderResource<'r>>,
    /// Each entry's index, by its source bytes and sprite entry addresses.
    ordinals: crate::fast_hash::FastMap<(usize, usize, usize), usize>,
}

impl<'r> TextureIdentities<'r> {
    fn resolve(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, SoftwareSceneServiceError> {
        let RenderResourceData::Sprite(entry) = texture.data else {
            return Err(SoftwareSceneServiceError::TextureResource);
        };
        let key = (
            texture.source.bytes.as_ptr() as usize,
            texture.source.bytes.len(),
            std::ptr::from_ref(entry) as usize,
        );
        let ordinal = if let Some(&index) = self.ordinals.get(&key) {
            index
        } else {
            if self.entries.len() > MAX_PORTABLE_TEXTURE_ENTRY_ORDINAL as usize {
                return Err(SoftwareSceneServiceError::TextureCapacity);
            }
            let index = self.entries.len();
            self.entries.push(texture);
            self.ordinals.insert(key, index);
            index
        };
        Ok(TextureSortIdentity::PortableEntryOrdinal(ordinal as u32))
    }
}

enum ModelObjectCallbacks<'s> {
    Ordinary(SceneModelSubmission),
    Furniture(furniture::FurnitureModelCallbacks<'s>),
    Object(object_models::ObjectModelServiceLoan<'s>),
}

#[derive(Clone, Copy)]
pub enum ModelTerrainRead<'s> {
    /// Existing ordinary model grounding custody, refreshed at renderer epochs.
    RendererEpoch,
    /// World objects and furniture observe the current shared terrain.
    Live(&'s TerrainRuntime),
}

impl<'a> ModelTerrainRead<'a> {
    fn resolve(self, epoch: Option<&'a TerrainRuntime>) -> Option<&'a TerrainRuntime> {
        match self {
            Self::RendererEpoch => epoch,
            Self::Live(terrain) => Some(terrain),
        }
    }
}

/// Selected renderer+50 table00546E60, independently of template00544740.
/// Current H14 flags and005471C0 are refreshed by the frame owner; retaining
/// callback mode across a closed pane does not freeze those process inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RendererSurfaceCallbacks {
    Default,
    Terrain { world_flags: u32, wave_clock: u32 },
}

/// A selection set aside by [`SoftwareScene::select_default_surface_callbacks`].
#[must_use]
pub(crate) struct SurfaceCallbackSelection(RendererSurfaceCallbacks);

struct SoftwareModelDrawRequest<'s> {
    root_mode: crate::model_execution::ModelRootMode,
    environment: ModelEnvironment,
    argument: i32,
    context: ModelContext,
    callbacks: ModelObjectCallbacks<'s>,
    terrain: ModelTerrainRead<'s>,
    surface_table: ModelSurfaceTableOrigin,
    primitive_context: ModelPrimitiveContext,
    failure_policy: ModelFailurePolicy,
    stack_registers: crate::model_execution::ModelStackRegisterPolicy,
    rng: Option<&'s mut u32>,
}

/// Logs the first use of each stack register approximation in this process.
fn log_stack_register_seed(policy: crate::model_execution::ModelStackRegisterPolicy) {
    use crate::model_execution::ModelStackRegisterPolicy::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    static WING: AtomicBool = AtomicBool::new(false);
    static LOADING: AtomicBool = AtomicBool::new(false);
    static OBJECTS: AtomicBool = AtomicBool::new(false);
    static SHOP: AtomicBool = AtomicBool::new(false);
    let (logged, message) = match policy {
        Unknown | BriefingPreviewMapFrameBases | FurnitureRootCarry => return,
        ObjectStackHistoryApproximations => (&OBJECTS, "WingModel457StackHistoryApproximation, VehicleModel117StackHistoryApproximation, SkimmerModel127StackHistoryApproximation, SkimmerMorphModel126StackHistoryApproximation, LaserMineModel85StackHistoryApproximation, ProximityMineModel84StackHistoryApproximation, ThumperModel46StackHistoryApproximation and ExplosionWaveModel112StackHistoryApproximation: object draws start the griffin wing model 457's register 9 at 0x56 in every world (Infest01's intro read; gameplay reads heap pointer residue); during a vehicle mode change, model 117's register 3 at the loading screen's 0x6EBB (Infest01's gameplay copy was per-frame residue); the skimmer's model 127 register 15 at 0, as all 16 of Infest03's reads found it, and its morph's models 126 and 141 register 15 at 0, as all 6 morph reads found it; the laser mine's model 85 register 3 at 0xCF2, which 13 of Infest03's 17 unarmed reads found and none found 1; the proximity mine's model 84 register 3 at 0x2861, as all 9 of the lab's retail in-flight reads found it; the thumper's model 46 register 1 at 0x1A, as all 21 gameplay draws beside Terra Nova's generator read it; and the blast wave's model 112 register 1 at 0x1A, as all 22 of the lab's Foldrak Noir blast draws read it (MODEL_RUNTIME.md)"),
        WingModel457StackHistoryApproximation => (&WING, "WingModel457StackHistoryApproximation: the griffin wing model 457 starts register 9 at 0x56 in every world, the stack value Infest01's intro read in 796 of 830 draws (MODEL_RUNTIME.md)"),
        ShopPreviewModel127StackHistoryApproximation => (&SHOP, "ShopPreviewModel127StackHistoryApproximation: the factory shop's skimmer preview starts model 127's register 15 at 0xA195; retail's is residue (0xA195, 0x05F1 and 0xD39A in three lab runs of Artoria's shop), never 0, so all 104 reads fell through as the seed does (MODEL_RUNTIME.md)"),
        LoadingModel117StackHistoryApproximation => (&LOADING, "LoadingModel117StackHistoryApproximation: loading model 117 starts register 3 at 0x6EBB, the stack value Infest01's child 131 read in 64 of 68 reads (MODEL_RUNTIME.md)"),
    };
    if !logged.swap(true, Ordering::Relaxed) {
        eprintln!("{message}");
    }
}

struct ModelServices<'s, 'r> {
    callbacks: ModelObjectCallbacks<'s>,
    sorting: bool,
    textures: &'s mut TextureIdentities<'r>,
    process_rng: Option<&'s mut u32>,
    feed: Option<&'s mut crate::scene_feed::SceneFeedBuilder<'r>>,
    motion: Option<&'s mut crate::raster_motion::RasterMotion>,
    tint: Option<crate::crystal_flares::CrystalSpark>,
    sky: Option<&'s crate::modern_tuning::SkyTuning>,
    metal: Option<&'s mut crate::raster_metal::RasterMetals>,
}

impl ModelValueCallback for ModelServices<'_, '_> {
    type Error = SoftwareSceneServiceError;

    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        match &self.callbacks {
            ModelObjectCallbacks::Ordinary(submission) => submission
                .callback_value(selector)
                .ok_or(SoftwareSceneServiceError::PureVirtualCallback { selector }),
            ModelObjectCallbacks::Furniture(callbacks) => callbacks.packed_value(selector),
            ModelObjectCallbacks::Object(callbacks) => {
                callbacks.packed_value(selector).map_err(Into::into)
            }
        }
    }
}

impl<'a> ModelCommandServices<'a> for ModelServices<'_, 'a> {
    fn prepare_draw(
        &mut self,
        _: &mut DrawQueue<PrimitiveCommand<'a>>,
    ) -> Result<bool, Self::Error> {
        // Direct3D virtual+04 = 004EB490. It only reads display+0E and
        // renderer+3C; unlike other renderer classes it publishes no work.
        Ok(self.sorting)
    }

    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        self.textures.resolve(texture)
    }

    fn process_rng(&mut self) -> Option<&mut u32> {
        self.process_rng.as_deref_mut()
    }

    fn scene_feed(&mut self) -> Option<&mut crate::scene_feed::SceneFeedBuilder<'a>> {
        self.feed.as_deref_mut()
    }

    fn draw_motion(
        &mut self,
        model: u16,
        transform: crate::model_context::ModelTransform,
    ) -> Option<crate::raster_motion::ViewMotion> {
        self.motion.as_deref_mut()?.motion(model, transform)
    }

    fn draw_tint(&self) -> Option<crate::crystal_flares::CrystalSpark> {
        self.tint
    }

    fn raster_sky(&self) -> Option<&crate::modern_tuning::SkyTuning> {
        self.sky
    }

    fn raster_metal(
        &mut self,
        request: &crate::model_execution::ModelPrimitiveRequest<'_>,
        resources: &RenderResources<'a>,
    ) -> Option<std::rc::Rc<crate::raster_metal::MetalFace>> {
        self.metal.as_deref_mut()?.face(request, resources)
    }

    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        use crate::frontend_scene::ModelCallbackPolicy;
        use crate::model_vertices::ModelCustomVertexPolicy;
        let submission = match &self.callbacks {
            ModelObjectCallbacks::Ordinary(submission) => submission,
            ModelObjectCallbacks::Furniture(_) => return ModelCustomVertexPolicy::Furniture,
            // Packet+4=478950 -> captured object+10 is a separate owner.
            ModelObjectCallbacks::Object(_) => return ModelCustomVertexPolicy::RequiredObject,
        };
        match submission.policy {
            // Vtables00528518/00522360/00520494 share +4=0043A2D0. Panel
            // vtable00520378 and world actors have different compute owners.
            ModelCallbackPolicy::Frontend
            | ModelCallbackPolicy::NameEntry { .. }
            | ModelCallbackPolicy::Loading { .. } => ModelCustomVertexPolicy::UnsignedRowSource,
            // Vtable00520384's +4 is _purecall as well.
            ModelCallbackPolicy::WorldSkyStaticValuesApproximation
            | ModelCallbackPolicy::Panel { .. }
            | ModelCallbackPolicy::PureVirtual => ModelCustomVertexPolicy::Unowned,
        }
    }

    fn required_custom_vertex(
        &mut self,
        request: crate::model_vertices::ModelCustomVertexRequest<'_>,
        vertices: &mut crate::model_vertices::ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), crate::model_vertices::ModelCustomVertexError<Self::Error>> {
        match &mut self.callbacks {
            //478950 forwards the actual packet+3C object's virtual+10.
            ModelObjectCallbacks::Object(callbacks) => callbacks
                .custom_vertex(request, vertices, output, self.process_rng.as_deref_mut())
                .map_err(|reason| {
                    crate::model_vertices::ModelCustomVertexError::Host(
                        SoftwareSceneServiceError::ObjectCustomVertex {
                            object: callbacks.source,
                            selector: request.selector,
                            reason,
                        },
                    )
                }),
            _ => Err(crate::model_vertices::ModelVertexFault::UnownedCustomVertex.into()),
        }
    }

    fn furniture_custom_vertex(
        &mut self,
        request: crate::model_vertices::ModelCustomVertexRequest<'_>,
        vertices: &mut crate::model_vertices::ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), crate::model_vertices::ModelCustomVertexError<Self::Error>> {
        use crate::model_vertices::ModelVertexFault;
        let ModelObjectCallbacks::Furniture(callbacks) = &mut self.callbacks else {
            return Err(ModelVertexFault::UnownedCustomVertex.into());
        };
        callbacks.custom_vertex(request, vertices, output, self.process_rng.as_deref_mut())
    }
}

/// The process globals [`SoftwareScene::save_globals`] copied.
pub(crate) struct SceneGlobals<'r> {
    process_rng: Option<u32>,
    environment: ModelEnvironment,
    primitive_state: PrimitiveState,
    furniture_root_registers: crate::model_values::ModelRegisterBlock,
    draw_tint: Option<crate::crystal_flares::CrystalSpark>,
    graphics: crate::process_model_graphics::ProcessModelGraphicsSave,
    /// The textures' sort ordinals, given in the order the draws meet them.
    texture_identities: TextureIdentities<'r>,
}

/// What the game's draw sets aside while it draws
/// ([`SoftwareScene::set_game_draw`]): the view shown's arena, the device's
/// capture of the world and the frame's clip.
struct GameDrawKept<'r> {
    queue: DrawQueue<PrimitiveCommand<'r>>,
    feed: Option<crate::scene_feed::SceneFeedBuilder<'r>>,
    motion: Option<crate::raster_motion::RasterMotion>,
    viewport: [i16; 4],
}

/// The first failure that stopped a batch.
#[derive(Clone, Debug)]
struct StopCause {
    text: String,
    /// The draw arena overflowed there (native 0x301): a 0x301 catch lifts it.
    overflow: bool,
}

pub struct SoftwareScene<'f, 'r> {
    pub scratch: ProcessModelScratch,
    pub graphics: crate::process_model_graphics::ProcessModelGraphics,
    id: SoftwareRendererId,
    resources: RenderResources<'r>,
    tables: &'r PrimitiveTables,
    execution_limits: ModelExecutionLimits,
    color_layout: NativeColorLayout,
    pub environment: ModelEnvironment,
    pub primitive_state: PrimitiveState,
    pub queue: DrawQueue<PrimitiveCommand<'r>>,
    pub pixels: SoftwarePixelBackend<'f, 'r>,
    pub stats: SoftwareSceneStats,
    texture_identities: TextureIdentities<'r>,
    terrain: Option<TerrainRuntime>,
    surface_callbacks: RendererSurfaceCallbacks,
    furniture_omissions: HashSet<(u16, usize, String)>,
    water_omissions: HashSet<String>,
    /// Process RNG word 0055F18C for jitter controls (opcode 0x22),
    /// copied in per draw from the session and written back after.
    /// Frontend scenes leave it unset and keep the unsupported boundary.
    process_rng: Option<u32>,
    /// Opcode 2A's mirror copy, set per frame like `process_rng`: the world
    /// draw chooses it from the port fixes, other scenes keep retail's.
    mirror_copy: crate::model_vertices::MirrorCopySource,
    /// Port fix `hovercraft_fans`, set per frame as `mirror_copy` is.
    hovercraft_fans: bool,
    /// The furniture roots' shared register block (`FurnitureRootCarry`),
    /// cleared at each furniture walk.
    furniture_root_registers: crate::model_values::ModelRegisterBlock,
    /// Port fix `guided_mounts`: the vehicle's value 7 follows its morph
    /// through a change between the ground forms and the others.
    guided_mounts: bool,
    stopped: bool,
    /// The first failure that stopped this batch, for the frame's report:
    /// later submissions only answer `Stopped`.
    stop_cause: Option<StopCause>,
    aborted_presentation: bool,
    /// Port widescreen: the clip rectangle of the 4:3 canvas, kept across
    /// the frames its screens draw, apart from the whole frame's.
    canvas_viewport: Option<[i16; 4]>,
    /// The whole frame's clip rectangle while a canvas draw holds it.
    frame_viewport: Option<[i16; 4]>,
    /// The world's scene feed while the world draw collects one, for a
    /// frame whose device traces rays ([`crate::scene_feed`]).
    scene_feed: Option<crate::scene_feed::SceneFeedBuilder<'r>>,
    /// The untraced world's moving models while the Vulkan raster follows
    /// them ([`crate::raster_motion`]).
    raster_motion: Option<crate::raster_motion::RasterMotion>,
    /// Vulkan draws this untraced world's models from their slots' exact
    /// twins (the plan's item 15).
    exact_corners: bool,
    /// Thermal vision shows ironbow (the lamps' cycle under Vulkan).
    ironbow: bool,
    /// The colour the model drawn now is laid in again, added, under Vulkan
    /// (a crystal's flare, set by the furniture draw for its model).
    pub(crate) draw_tint: Option<crate::crystal_flares::CrystalSpark>,
    /// Port option: the fog's colour under the water, which the terrain's
    /// fog materials (0x73, 0x74) take too ([`Self::set_underwater`]).
    water_fog: Option<u32>,
    /// The world's modern sky while Vulkan's raster draws it in place of
    /// the painting ([`crate::scene_capture::raster_sky_draw`]).
    raster_sky: Option<crate::modern_tuning::SkyTuning>,
    /// The models' materials, read while Vulkan's raster mirrors the sky in
    /// the world's metal ([`crate::raster_metal`]).
    raster_metals: crate::raster_metal::RasterMetals,
    /// Port option: set while the world draws the game's draw apart from
    /// the view shown ([`Self::set_game_draw`]).
    game_draw: Option<GameDrawKept<'r>>,
    /// The game's draw's own arena between its frames, which keeps its
    /// size (retail's) apart from the view shown's.
    game_queue: Option<DrawQueue<PrimitiveCommand<'r>>>,
}

#[cfg(test)]
thread_local! {
    /// Tests: the world draw collects its scene feed on a frame that cannot
    /// trace it, as a traced frame's draw does (its picture the raster's).
    pub(crate) static FEED_UNTRACED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Tests: the last scene feed as its frame ended: the objects whose
    /// faces it holds drawn for the tracer alone
    /// ([`crate::scene_feed::SceneTriangle::culled`]), and the count of
    /// those faces, its wheels and its exhausts.
    pub(crate) static FEED_ENDED: std::cell::RefCell<(Vec<crate::object_runtime::ObjectId>, [usize; 3])> =
        const { std::cell::RefCell::new((Vec::new(), [0; 3])) };
}

fn feed_untraced() -> bool {
    #[cfg(test)]
    return FEED_UNTRACED.get();
    #[cfg(not(test))]
    false
}

impl<'f, 'r> SoftwareScene<'f, 'r> {
    /// Begin the world's scene feed when the frame's device traces rays
    /// (always, under `infrared` vision, where it can): true when it does,
    /// and the world draw then traces before its last flush
    /// ([`Self::trace_scene_feed`]).
    pub fn begin_scene_feed(
        &mut self,
        camera: crate::scene_feed::SceneCamera,
        infrared: bool,
    ) -> bool {
        self.pixels.frame.set_trace_infrared(infrared);
        if !self.pixels.frame.traces_rays() && !feed_untraced() {
            self.scene_feed = None;
            return false;
        }
        let mut builder =
            crate::scene_feed::SceneFeedBuilder::new(camera, self.pixels.renderer.material_shifts);
        builder.motion = self.pixels.frame.take_motion();
        // Last frame's lists, emptied: the feed fills them without growing.
        if let Some((vertices, triangles)) = self.pixels.frame.take_spare_feed() {
            builder.feed.vertices = vertices;
            builder.feed.triangles = triangles;
        }
        builder.target_height = self.pixels.frame.read_size()[1] as f32;
        self.scene_feed = Some(builder);
        // The raster draws over the traced image (Terra Nova's
        // half-transparent crystals) find a struck crystal's spark in this
        // view; the raster world's begin sets it only when untraced.
        self.pixels.frame.set_world_camera(Some(camera));
        true
    }

    /// Whether the frame's device traces rays (the modern mode).
    pub fn traces_rays(&self) -> bool {
        self.pixels.frame.traces_rays()
    }

    /// Make the frame device's tracer and upscaler ahead of a world's first
    /// traced frame ([`SoftwareFrame::prepare_trace`]).
    pub fn prepare_trace(&mut self) {
        self.pixels.frame.prepare_trace();
    }

    /// The feed being collected, to add the world's light, fog and sky.
    pub fn scene_feed_mut(&mut self) -> Option<&mut crate::scene_feed::SceneFeedBuilder<'r>> {
        self.scene_feed.as_mut()
    }

    /// Resolve the feed's sprites to the textures the raster would sample
    /// and trace the world into the frame. The next flush then replays only
    /// what the feed did not take.
    pub fn trace_scene_feed(&mut self) {
        let Some(builder) = self.scene_feed.as_mut() else {
            return;
        };
        // Every additive quad in the deferred queue (particles, model
        // billboards and edges, barriers, ribbons, race gates, force fields)
        // that was not already fed in 3D is captured for mirrored effects.
        for command in self.queue.payloads() {
            crate::scene_capture::capture_emitter(builder, command);
        }
        builder.settle_faces();
        // The raster's blended faces a traced face was drawn after: retail's
        // later draw covered them but for its keyed texels, so the device
        // lays them behind it.
        if !builder.covered.is_empty() {
            let covered = std::mem::take(&mut builder.covered);
            for command in self.queue.payloads_mut() {
                if command
                    .feed_order
                    .is_some_and(|order| covered.contains(&order))
                {
                    command.covered = true;
                }
            }
        }
        let Some(builder) = self.scene_feed.as_mut() else {
            return;
        };
        let upload = self.pixels.texture_state.upload;
        let resolved: Vec<_> = builder
            .sprites
            .iter()
            .enumerate()
            .map(|(index, (sprite, resource, mode))| {
                // The modern sky's land band takes its sprite with the
                // painted sky cut away.
                let band = builder
                    .band_cuts
                    .iter()
                    .find(|(sprite, _)| *sprite as usize == index)
                    .map(|(_, band)| *band);
                match band {
                    Some(band) => self
                        .pixels
                        .textures
                        .resolve_band(*resource, *mode, upload, band),
                    None => self.pixels.textures.resolve(*resource, *mode, upload),
                }
                .ok()
                .flatten()
                .map(|texture| (texture.allocation_id, texture.content_key(), *sprite))
            })
            .collect();
        let feed = &mut builder.feed;
        feed.textures.clear();
        // Every resolved sprite takes a slot of the feed's textures.
        let mut allocations = Vec::new();
        let slots: Vec<Option<u32>> = resolved
            .iter()
            .map(|texture| {
                texture.map(|(allocation, key, sprite)| {
                    feed.textures.push((key, sprite));
                    allocations.push(allocation);
                    (feed.textures.len() - 1) as u32
                })
            })
            .collect();
        for triangle in &mut feed.triangles {
            triangle.texture = triangle
                .texture
                .and_then(|sprite| slots.get(sprite as usize).copied().flatten());
        }
        feed.terrain_textures = builder
            .terrain_sprites
            .iter()
            .map(|sprite| sprite.and_then(|sprite| slots.get(sprite as usize).copied().flatten()))
            .collect();
        feed.water_texture = builder
            .water_sprite
            .and_then(|sprite| slots.get(sprite as usize).copied().flatten());
        for glow in &mut feed.glows {
            glow.texture = glow
                .texture
                .and_then(|sprite| slots.get(sprite as usize).copied().flatten());
        }
        for emitter in &mut feed.emitters {
            emitter.texture = emitter
                .texture
                .and_then(|sprite| slots.get(sprite as usize).copied().flatten());
        }
        let textures: Vec<_> = allocations
            .iter()
            .filter_map(|allocation| self.pixels.textures.get(*allocation))
            .collect();
        // The gates' redrawn lightning, each bolt and glint in its sprite's
        // colour, and a steady light at each opening its spokes show.
        let tuning = crate::modern_tuning::current();
        let gates = &tuning.materials().gates;
        let colour = |sprite: u32| {
            resolved
                .get(sprite as usize)
                .copied()
                .flatten()
                .and_then(|(allocation, _, _)| self.pixels.textures.get(allocation))
                .map_or([1.0; 3], |texture| texture.mean_colour())
        };
        let bolts: Vec<_> = builder
            .feed
            .bolts
            .iter()
            .map(|bolt| (*bolt, colour(bolt.sprite)))
            .collect();
        let glints: Vec<_> = builder
            .feed
            .glints
            .iter()
            .map(|glint| (*glint, colour(glint.sprite)))
            .collect();
        for (look, gate) in gates.iter().enumerate() {
            let (strength, reach) = gate.light;
            let own: Vec<_> = bolts.iter().filter(|(bolt, _)| bolt.look == look).collect();
            if strength <= 0.0 || reach <= 0.0 || own.is_empty() {
                continue;
            }
            let tint = own.iter().fold([0.0; 3], |sum: [f32; 3], (_, colour)| {
                std::array::from_fn(|channel| sum[channel] + colour[channel] / own.len() as f32)
            });
            let [r, g, b] = tint.map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8);
            let spokes: Vec<_> = own.iter().map(|(bolt, _)| *bolt).collect();
            for centre in crate::scene_gate::openings(&spokes) {
                builder.lone_glow(
                    centre,
                    crate::scene_feed::SceneGlow {
                        color: [r, g, b, 255],
                        strength,
                        reach,
                        ..Default::default()
                    },
                );
            }
        }
        if self.pixels.frame.trace_world(&builder.feed, &textures) {
            for (look, gate) in gates.iter().enumerate() {
                let own_bolts: Vec<_> = bolts
                    .iter()
                    .filter(|(bolt, _)| bolt.look == look)
                    .copied()
                    .collect();
                let own_glints: Vec<_> = glints
                    .iter()
                    .filter(|(glint, _)| glint.look == look)
                    .copied()
                    .collect();
                let fans = crate::scene_gate::fans(
                    &builder.feed.camera,
                    &own_bolts,
                    &own_glints,
                    gate,
                    builder.feed.clock_micros,
                );
                self.pixels.frame.add_light_fans(&fans);
            }
            let glare = crate::world_scene::lamp_glare(
                &builder.feed.camera,
                &builder.feed.lamps,
                tuning.headlight(),
            );
            self.pixels.frame.add_light_fans(&glare);
        }
    }

    /// An untraced world's draws begin, with `camera`, for the device's
    /// anti-aliasing; where it follows the image over frames, its moving
    /// models report their own motion. Its water mirrors the world with the
    /// least share `water_reflect` (none: lava, which does not), and the eye
    /// under its live surface sees through it from below, through the
    /// `murk`. A device that takes the world's `light` (its light alone,
    /// for its own passes) draws its modern sky, where `sky`, in the
    /// painting's place.
    pub(crate) fn begin_raster_world(
        &mut self,
        camera: &crate::scene_feed::SceneCamera,
        water_reflect: Option<f32>,
        murk: Option<crate::vk_device::Murk>,
        light: Option<&crate::scene_feed::SceneFeed>,
        sky: bool,
    ) {
        let follows = self
            .pixels
            .frame
            .begin_raster_world(camera, water_reflect, murk, light, sky);
        self.raster_sky = light
            .filter(|_| self.pixels.frame.draws_raster_sky())
            .map(|feed| crate::modern_tuning::current().sky(feed.world));
        if self.pixels.frame.reflects_metal() {
            self.raster_metals
                .begin(self.pixels.renderer.material_shifts);
        }
        self.exact_corners = self.pixels.frame.is_vulkan();
        // A world not followed leaves no history for the next one.
        let mut motion = self.pixels.frame.take_raster_motion();
        self.raster_motion = follows.then(|| {
            motion.begin_frame(*camera);
            motion
        });
    }

    /// The painting is drawn and the depth cleared: a device that draws
    /// the modern sky lays it behind the painting's land and art.
    pub fn draw_raster_sky(&mut self) {
        if self.raster_sky.is_some() {
            self.pixels.frame.draw_raster_sky();
        }
    }

    /// An untraced world's last draw is in, before the HUD's.
    pub fn finish_raster_world(&mut self) {
        self.exact_corners = false;
        self.raster_sky = None;
        self.raster_metals.end();
        if let Some(mut motion) = self.raster_motion.take() {
            motion.finish_frame();
            self.pixels.frame.keep_raster_motion(motion);
        }
        self.pixels.frame.finish_raster_world();
    }

    /// Thermal vision's camera over the world, once its last draw is in and
    /// before the HUD's, in ironbow where the lamps' cycle says.
    pub fn finish_thermal(&mut self) {
        self.pixels.frame.finish_thermal(self.ironbow);
    }

    /// Non-native: thermal vision's palette for the frames to come.
    pub fn set_ironbow(&mut self, ironbow: bool) {
        self.ironbow = ironbow;
    }

    /// End the world's scene feed: later draws are the raster's.
    pub fn end_scene_feed(&mut self) {
        if let Some(mut builder) = self.scene_feed.take() {
            #[cfg(test)]
            FEED_ENDED.set({
                let culled = builder.feed.triangles.iter().filter(|t| t.culled);
                let mut objects: Vec<_> = culled
                    .clone()
                    .filter_map(|t| Some(builder.feed.objects.get(t.object? as usize)?.id))
                    .collect();
                objects.dedup();
                (
                    objects,
                    [
                        culled.count(),
                        builder.feed.wheels.len(),
                        builder.feed.exhausts.len(),
                    ],
                )
            });
            builder.motion.finish_frame();
            self.pixels.frame.keep_motion(builder.motion);
            self.pixels
                .frame
                .keep_spare_feed(builder.feed.vertices, builder.feed.triangles);
            self.pixels.frame.set_world_camera(None);
        }
        self.pixels.frame.end_traced_world();
    }

    /// Port widescreen: draw on `canvas` until [`Self::leave_canvas`]. Layout
    /// sees its size and clipping its own clip rectangle, which starts as the
    /// whole canvas and survives to its next draw; the frame's clip waits.
    pub fn enter_canvas(&mut self, canvas: crate::software_frame::FrameCanvas) {
        let canvas_viewport =
            self.canvas_viewport
                .unwrap_or([0, 0, canvas.size[0] as i16, canvas.size[1] as i16]);
        self.frame_viewport = Some(std::mem::replace(
            &mut self.pixels.pipeline.viewport,
            canvas_viewport,
        ));
        self.pixels.frame.set_canvas(Some(canvas));
    }

    /// End a canvas draw: keep the canvas's clip rectangle and restore the
    /// frame's.
    pub fn leave_canvas(&mut self) {
        if let Some(frame_viewport) = self.frame_viewport.take() {
            self.canvas_viewport = Some(std::mem::replace(
                &mut self.pixels.pipeline.viewport,
                frame_viewport,
            ));
            self.pixels.frame.set_canvas(None);
        }
    }

    /// Queue/presentation custody must be healthy before feature containment.
    pub(crate) fn feature_continuation_allowed(&self) -> bool {
        !self.aborted_presentation && !self.queue.state().stopped
    }

    /// Port feature containment after a failed producer: retain all queued
    /// commands and rendering state so later independent features can submit.
    /// Callers must not use this to recover a failed pixel flush/presentation.
    pub fn continue_after_feature_error(&mut self) -> Result<(), SoftwareSceneError> {
        if !self.feature_continuation_allowed() {
            return Err(SoftwareSceneError::Stopped);
        }
        self.recover_stopped_model_scratch()?;
        self.stopped = false;
        self.stop_cause = None;
        Ok(())
    }

    /// Explicit portable retirement at an already admitted feature stop.
    /// The registered marks rewind; every reached scratch byte is retained.
    fn recover_stopped_model_scratch(&self) -> Result<(), SoftwareSceneError> {
        match self.scratch.state()? {
            ModelScratchDrawState::Ready => Ok(()),
            ModelScratchDrawState::Stopped(scope) => {
                self.scratch
                    .recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)?;
                Ok(())
            }
            actual @ ModelScratchDrawState::Running(_) => Err(ModelScratchError::DrawState {
                expected: "Ready or Stopped",
                actual,
            }
            .into()),
        }
    }

    pub fn new(request: SoftwareSceneRequest<'f, 'r>) -> Result<Self, SoftwareSceneError> {
        let SoftwareSceneRequest {
            model_process,
            resources,
            tables,
            frame,
            config,
        } = request;
        let crate::model_process::ModelProcessOwners {
            scratch, graphics, ..
        } = model_process;
        // ModelCommandBackend currently resolves unchanged material DWORDs.
        // Reject a display conversion that it cannot yet perform faithfully.
        if config.color_layout
            != (NativeColorLayout {
                red_shift: 10,
                green_shift: 5,
                blue_shift: 0,
            })
        {
            return Err(SoftwareSceneError::ColorLayout);
        }
        Ok(Self {
            scratch,
            graphics,
            id: SoftwareRendererId(
                NEXT_RENDERER
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                    .expect("renderer identity exhausted"),
            ),
            resources,
            tables,
            execution_limits: config.execution_limits,
            color_layout: config.color_layout,
            environment: config.environment,
            primitive_state: PrimitiveState {
                mode: config.primitive_mode,
                far_color: config.far_color,
                effect_color: config.effect_color,
                projection: config.environment.projection,
            },
            queue: DrawQueue::with_native_memory(
                config.queue_capacity,
                flash::record_primitive_memory,
            )?,
            pixels: SoftwarePixelBackend::new(
                frame,
                config.renderer,
                config.pipeline,
                config.texture_state,
            ),
            stats: SoftwareSceneStats::default(),
            texture_identities: TextureIdentities::default(),
            terrain: None,
            surface_callbacks: RendererSurfaceCallbacks::Default,
            furniture_omissions: HashSet::new(),
            water_omissions: HashSet::new(),
            process_rng: None,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            furniture_root_registers: Default::default(),
            guided_mounts: false,
            stopped: false,
            stop_cause: None,
            aborted_presentation: false,
            canvas_viewport: None,
            frame_viewport: None,
            scene_feed: None,
            raster_motion: None,
            exact_corners: false,
            ironbow: false,
            draw_tint: None,
            water_fog: None,
            raster_sky: None,
            raster_metals: Default::default(),
            game_draw: None,
            game_queue: None,
        })
    }

    /// Port option, the remastered camera under the water: the world's
    /// draw wobbles and dims ([`crate::software_frame::WetFrame`]) and the
    /// terrain's fog takes `fog`, the far colour the world publishes; None
    /// ends it.
    pub fn set_underwater(&mut self, wet: Option<(crate::software_frame::WetFrame, u32)>) {
        self.water_fog = wet.map(|(_, fog)| fog);
        self.pixels.frame.set_wet(wet.map(|(frame, _)| frame));
    }

    /// The water's own colour, its brightest channel 1: the texture of
    /// retail's all-water tile (`00537010`'s sixteenth record), as the
    /// tracer takes it.
    pub fn water_colour(&mut self) -> Option<[f32; 3]> {
        let slot = self.tables.water.shoreline[15].sprite_slot;
        let resource = self
            .resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: i32::from(slot),
            })
            .ok()?;
        let upload = self.pixels.texture_state.upload;
        let texture = self
            .pixels
            .textures
            .resolve(resource, SpriteRenderMode::Mode0, upload)
            .ok()??;
        Some(texture.mean_colour())
    }

    /// Settings `+3C`'s Filtering for the raster's textures, as
    /// [`crate::frontend_video::apply_video_pipeline`] sets it at a display
    /// reset: point at 0, else linear.
    pub fn set_filtering(&mut self, level: u32) {
        let filter = if level == 0 { 1 } else { 2 };
        self.pixels.texture_state.sampling.min_filter = filter;
        self.pixels.texture_state.sampling.mag_filter = filter;
    }

    /// Explicit epoch snapshot for ordinary scene/template callers. World
    /// objects, pickups and furniture instead loan current terrain at each
    /// model invocation, including policy/deformation writes. Availability
    /// does not choose callback mode; template callbacks use no terrain.
    pub fn set_terrain(&mut self, terrain: Option<TerrainRuntime>) {
        self.furniture_omissions.clear();
        self.water_omissions.clear();
        self.terrain = terrain;
    }

    /// Reached 004501A0 replaces selected+10/+14. The template table retains
    /// its own defaults, even for later model draws in this same scene.
    pub fn install_terrain_surface_callbacks(&mut self, world_flags: u32, wave_clock: u32) {
        self.surface_callbacks = RendererSurfaceCallbacks::Terrain {
            world_flags,
            wave_clock,
        };
    }

    /// The hold display draws its carried actors over the default pair: in
    /// the lab their Ground calls are `004EBCD0`'s (Y = -300), never the
    /// world's `0044FF50`. The caller restores the world's selection after.
    pub(crate) fn select_default_surface_callbacks(&mut self) -> SurfaceCallbackSelection {
        SurfaceCallbackSelection(std::mem::replace(
            &mut self.surface_callbacks,
            RendererSurfaceCallbacks::Default,
        ))
    }

    pub(crate) fn restore_surface_callbacks(&mut self, selection: SurfaceCallbackSelection) {
        self.surface_callbacks = selection.0;
    }

    /// Observe the current immutable H14 flags and process clock without
    /// changing callbacks. A closed/disabled briefing pane retains its alias.
    pub fn refresh_terrain_surface_globals(&mut self, world_flags: u32, wave_clock: u32) {
        if matches!(
            self.surface_callbacks,
            RendererSurfaceCallbacks::Terrain { .. }
        ) {
            self.surface_callbacks = RendererSurfaceCallbacks::Terrain {
                world_flags,
                wave_clock,
            };
        }
    }

    /// Process RNG word 0055F18C for jitter controls (opcode 0x22).
    /// Copied in per draw by the frame owner (the world draw from the
    /// session word); consumed draws advance it, and the owner reads
    /// it back after the draw. Frontend scenes leave it unset and
    /// keep the previous unsupported boundary.
    pub fn set_process_rng(&mut self, rng: Option<u32>) {
        self.process_rng = rng;
    }

    pub fn process_rng(&self) -> Option<u32> {
        self.process_rng
    }

    /// Port option: the process globals a model draw publishes or steps
    /// (the RNG word, the view, projection, fade and lights, the furniture
    /// roots' registers, the last model's world, the textures' sort
    /// ordinals), which [`Self::restore_globals`] puts back (the world
    /// draw's view shown, drawn apart from the game's).
    pub(crate) fn save_globals(&self) -> Result<SceneGlobals<'r>, SoftwareSceneError> {
        Ok(SceneGlobals {
            process_rng: self.process_rng,
            environment: self.environment,
            primitive_state: self.primitive_state,
            furniture_root_registers: self.furniture_root_registers,
            draw_tint: self.draw_tint,
            graphics: self.graphics.save()?,
            texture_identities: self.texture_identities.clone(),
        })
    }

    /// What [`Self::save_globals`] copies, as text, one part each: the
    /// textures by their requests (two loads of a world have their own
    /// sources).
    #[cfg(test)]
    pub(crate) fn globals_text(&self) -> Vec<String> {
        let textures: Vec<_> = self
            .texture_identities
            .entries
            .iter()
            .map(|texture| texture.request)
            .collect();
        vec![
            format!("{:?}", self.process_rng),
            format!("{:?}", self.environment),
            format!("{:?}", self.primitive_state),
            format!("{:?}", self.furniture_root_registers),
            format!("{:?}", self.draw_tint),
            format!("{:?}", self.graphics.save()),
            format!("{textures:?}"),
        ]
    }

    /// Put back what [`Self::save_globals`] copied.
    pub(crate) fn restore_globals(
        &mut self,
        globals: SceneGlobals<'r>,
    ) -> Result<(), SoftwareSceneError> {
        self.graphics.restore(globals.graphics)?;
        self.process_rng = globals.process_rng;
        self.environment = globals.environment;
        self.primitive_state = globals.primitive_state;
        self.furniture_root_registers = globals.furniture_root_registers;
        self.draw_tint = globals.draw_tint;
        self.texture_identities = globals.texture_identities;
        Ok(())
    }

    /// Port option: the world's game drawn apart from the view shown, on
    /// its own arena (`set_game_draw(Some(viewport))`), or the view drawn
    /// again (`None`). The game's draw shows nothing: its flushes replay the
    /// queue without drawing, depth is never cleared, the device's capture
    /// of the world waits, and the clip is the game's `viewport`, retail's
    /// 4:3 surface in retail's game (the world flash's quads are clipped to
    /// it).
    pub(crate) fn set_game_draw(
        &mut self,
        game: Option<[i32; 2]>,
    ) -> Result<(), SoftwareSceneError> {
        if game.is_some() == self.game_draw.is_some() {
            return Ok(());
        }
        let Some([width, height]) = game else {
            if let Some(kept) = self.game_draw.take() {
                self.game_queue = Some(std::mem::replace(&mut self.queue, kept.queue));
                self.scene_feed = kept.feed;
                self.raster_motion = kept.motion;
                self.pixels.pipeline.viewport = kept.viewport;
            }
            return Ok(());
        };
        let queue = match self.game_queue.take() {
            Some(queue) => queue,
            // Renewed at the game's draw's start (`begin_frame_with_arena`).
            None => DrawQueue::with_native_memory(0xc8000, flash::record_primitive_memory)?,
        };
        self.game_draw = Some(GameDrawKept {
            queue: std::mem::replace(&mut self.queue, queue),
            feed: self.scene_feed.take(),
            motion: self.raster_motion.take(),
            viewport: std::mem::replace(
                &mut self.pixels.pipeline.viewport,
                [0, 0, width as i16, height as i16],
            ),
        });
        Ok(())
    }

    /// Port option: `draw` for the tracer's feed alone (the world draw's
    /// models outside the view, [`crate::scene_feed::SceneFeedBuilder::culled`]):
    /// its models pass no frustum gate, the feed marks what they lay, and
    /// what they queue for the raster goes to an arena of its own, dropped
    /// after with any stop it latched, so the raster draws nothing of them.
    /// Nor does the frame draw anything of theirs over the view: the gates'
    /// lightning, flares and fields they laid go too, and the exhausts and
    /// wheels (the trails in the mist, the wakes by wheel) that retail's
    /// draw never laid for the hull in first person. None without a feed.
    pub(crate) fn feed_only<T>(&mut self, draw: impl FnOnce(&mut Self) -> T) -> Option<T> {
        let feed = self.scene_feed.as_mut()?;
        let overlays = (
            feed.feed.bolts.len(),
            feed.feed.glints.len(),
            feed.feed.arcs.len(),
            feed.feed.exhausts.len(),
            feed.feed.wheels.len(),
        );
        feed.culled = true;
        let first = feed.feed.triangles.len();
        let capacity = self.queue.state().capacity;
        let queue = std::mem::replace(
            &mut self.queue,
            DrawQueue::new(capacity).expect("an arena's aligned size"),
        );
        let stop = (self.stopped, self.stop_cause.take());
        let drawn = draw(self);
        self.queue = queue;
        (self.stopped, self.stop_cause) = stop;
        if let Some(feed) = self.scene_feed.as_mut() {
            feed.culled = false;
            for triangle in &mut feed.feed.triangles[first..] {
                triangle.culled = true;
            }
            feed.feed.bolts.truncate(overlays.0);
            feed.feed.glints.truncate(overlays.1);
            feed.feed.arcs.truncate(overlays.2);
            feed.feed.exhausts.truncate(overlays.3);
            feed.feed.wheels.truncate(overlays.4);
        }
        Some(drawn)
    }

    /// The executable's sine table 0053DF00 the scene's models use.
    pub fn sine_table(&self) -> &[u16; 4096] {
        &self.tables.sine_table
    }

    /// Opcode 2A's mirror copy for the models drawn after this call (see
    /// [`crate::model_vertices::MirrorCopySource`]).
    pub fn set_mirror_copy(&mut self, source: crate::model_vertices::MirrorCopySource) {
        self.mirror_copy = source;
    }

    /// Port fix `hovercraft_fans` for the models drawn after this call (see
    /// [`crate::model_execution::hovercraft_fans_unmirrored`]).
    pub fn set_hovercraft_fans(&mut self, enabled: bool) {
        self.hovercraft_fans = enabled;
    }

    /// Forgets the furniture roots' block: only roots of one walk share it.
    pub(crate) fn clear_furniture_root_registers(&mut self) {
        self.furniture_root_registers = Default::default();
    }

    /// Port fix `guided_mounts` for the frame's object draws.
    pub fn set_guided_mounts(&mut self, enabled: bool) {
        self.guided_mounts = enabled;
    }

    ///Actual six-DWORD projection-window publisher, without fade/table reset.
    pub(crate) fn publish_model_window(
        &mut self,
        window: crate::process_model_graphics::ModelProjectionWindow,
    ) -> Result<(), SoftwareSceneError> {
        self.environment.projection.focal = window.focal;
        self.environment.projection.viewport = window.viewport;
        self.environment.projection.center = window.center;
        self.primitive_state.projection = self.environment.projection;
        self.graphics.publish_window(window)?;
        Ok(())
    }

    /// Retire this renderer's residency custody while keeping the renderer.
    /// Texture identities, CPU texture uploads and the command arena borrow
    /// the retired registrations and are dropped; identity, process model
    /// globals, saved renderer policy, pipeline and texture state survive.
    /// Both frame owners start every draw with an arena replacement
    /// (00439270), so the retired arena's flushed contents are unobservable.
    pub fn into_state(self) -> SoftwareSceneState {
        SoftwareSceneState {
            scratch: self.scratch,
            graphics: self.graphics,
            id: self.id,
            execution_limits: self.execution_limits,
            color_layout: self.color_layout,
            queue_capacity: self.queue.state().capacity,
            environment: self.environment,
            primitive_state: self.primitive_state,
            renderer: self.pixels.renderer,
            pipeline: self.pixels.pipeline,
            texture_state: self.pixels.texture_state,
            stats: self.stats,
            pixel_stats: self.pixels.stats,
            terrain: self.terrain,
            surface_callbacks: self.surface_callbacks,
            furniture_omissions: self.furniture_omissions,
            water_omissions: self.water_omissions,
            stopped: self.stopped,
            aborted_presentation: self.aborted_presentation,
            canvas_viewport: self.canvas_viewport,
        }
    }

    /// Rebind a retained renderer to another residency snapshot and surface.
    /// A new renderer (display reset) uses [`Self::new`] instead.
    pub fn resume(
        state: SoftwareSceneState,
        request: SoftwareSceneResume<'f, 'r>,
    ) -> Result<Self, SoftwareSceneError> {
        let mut pixels = SoftwarePixelBackend::new(
            request.frame,
            state.renderer,
            state.pipeline,
            state.texture_state,
        );
        pixels.stats = state.pixel_stats;
        Ok(Self {
            scratch: state.scratch,
            graphics: state.graphics,
            id: state.id,
            resources: request.resources,
            tables: request.tables,
            execution_limits: state.execution_limits,
            color_layout: state.color_layout,
            environment: state.environment,
            primitive_state: state.primitive_state,
            queue: DrawQueue::with_native_memory(
                state.queue_capacity,
                flash::record_primitive_memory,
            )?,
            pixels,
            stats: state.stats,
            texture_identities: TextureIdentities::default(),
            terrain: state.terrain,
            surface_callbacks: state.surface_callbacks,
            furniture_omissions: state.furniture_omissions,
            water_omissions: state.water_omissions,
            process_rng: None,
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            furniture_root_registers: Default::default(),
            guided_mounts: false,
            stopped: state.stopped,
            stop_cause: None,
            aborted_presentation: state.aborted_presentation,
            canvas_viewport: state.canvas_viewport,
            frame_viewport: None,
            scene_feed: None,
            raster_motion: None,
            exact_corners: false,
            ironbow: false,
            draw_tint: None,
            water_fog: None,
            raster_sky: None,
            raster_metals: Default::default(),
            game_draw: None,
            game_queue: None,
        })
    }

    pub fn id(&self) -> SoftwareRendererId {
        self.id
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// The failure that stopped the current batch, if one did.
    pub fn stop_cause(&self) -> Option<&str> {
        self.stop_cause.as_ref().map(|cause| cause.text.as_str())
    }

    /// Stop the batch, keeping its first cause.
    pub(crate) fn latch_stop(&mut self, cause: &dyn std::error::Error) {
        if !self.stopped {
            // Debug, since host errors such as the terrain's carry no source.
            self.stop_cause = Some(StopCause {
                text: format!("{cause} ({cause:?})"),
                overflow: self.queue.overflowed(),
            });
        }
        self.stopped = true;
    }

    /// 00459623 calls renderer virtual+84 directly for name-entry stars.
    /// This neither consults the model context nor allocates/flushes the
    /// deferred batch, which still contains the backdrop's model commands.
    pub fn draw_star_direct(&mut self, quad: StarQuad) -> Result<(), SoftwareSceneError> {
        if self.stopped {
            return Err(SoftwareSceneError::Stopped);
        }
        let result = (|| {
            let resource = self.resources.resolve(RenderResourceRequest {
                kind: RenderResourceKind::Material,
                argument: quad.color_argument,
            })?;
            let RenderResourceData::Material(value) = resource.data else {
                unreachable!("typed material lookup")
            };
            let mut command = star_command(quad, self.color_layout.convert_material(value), 0);
            self.pixels.draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )?;
            self.stats.stars += 1;
            Ok(())
        })();
        if let Err(error) = &result {
            self.latch_stop(error);
        }
        result
    }

    /// Frontend draw 00484940 first calls 00439270: replace the arena with
    /// C8000 bytes and reset it. Its guarded draw then resets that batch again
    /// and calls renderer virtual+14(NULL), 004BCA20 -> 0050DC60. Neither call
    /// clears color or reinitializes the persistent model/renderer globals.
    pub fn begin_frame(&mut self) -> Result<(), SoftwareSceneError> {
        self.begin_frame_with_arena(0xc8000)
    }

    /// [`Self::begin_frame`] with an arena of `bytes`: the world draw sizes
    /// its arena for the frame's view (`world_scene::world_arena_bytes`).
    pub fn begin_frame_with_arena(&mut self, bytes: u32) -> Result<(), SoftwareSceneError> {
        self.replace_command_arena(bytes)?;
        self.reset_renderer();
        Ok(())
    }

    /// 004A43A0 replacement, separate from +14(NULL) for the full frame owner.
    pub fn replace_command_arena(&mut self, bytes: u32) -> Result<(), SoftwareSceneError> {
        self.queue
            .renew_with_native_memory(bytes, flash::record_primitive_memory)?;
        self.stopped = false;
        self.stop_cause = None;
        self.aborted_presentation = false;
        self.stats.frame_begins += 1;
        Ok(())
    }

    /// 004BCA20(NULL) -> 0050DC60; saved renderer flags and color survive.
    pub fn reset_renderer(&mut self) {
        self.reset_depth_surface();
        self.stats.depth_resets += 1;
    }

    /// Renderer virtual `+10` (`004BC9B0`): the device enable/write pair and
    /// the saved renderer `+3C/+40` words. Gameplay draw `00488020` calls it
    /// with `(0,0)` or `(1,1)` from settings `+50` (Z Buffering); the
    /// frontend never issues it. Values other than 0/1 fail, like the
    /// native strict pair.
    pub fn set_depth_policy(&mut self, enable: u32, write: u32) -> Result<(), SoftwareSceneError> {
        if enable > 1 || write > 1 {
            return Err(SoftwareSceneError::DepthPolicy { enable, write });
        }
        self.pixels.renderer.depth_enable = enable;
        self.pixels.renderer.depth_write = write;
        self.pixels.pipeline.depth_enabled = enable != 0;
        self.pixels.pipeline.depth_write = write != 0;
        Ok(())
    }

    /// Presentation tail 0040D1D0 -> 00438120 -> 00495810 presents FIRST,
    /// then 0050D960 clears the next color surface. Call only after the host
    /// has presented/saved the completed pixels. The caller resolves H7/32
    /// (or the explicit gameplay +5C4 forced-black case) and converts the
    /// display color to the CPU target format; black is not implied here.
    ///
    /// The Direct3D branch clears both surfaces and writes device ZENABLE=0,
    /// ZWRITEENABLE=1. The non-depth DirectDraw branch only fills color.
    /// Saved renderer+38/+3C/+40 and all model globals survive both paths.
    pub fn after_present(&mut self, clear_rgba: [u8; 4]) -> Result<(), SoftwareSceneError> {
        if self.stopped && !self.aborted_presentation {
            return Err(SoftwareSceneError::Stopped);
        }
        self.aborted_presentation = false;
        self.pixels.frame.clear_color(clear_rgba);
        self.reset_depth_surface();
        self.stats.presentation_clears += 1;
        Ok(())
    }

    /// The outer native301 catch still presents/clears the partial surface.
    /// Permit that tail once; keep further scene submission stopped until the
    /// next real batch/arena reset. Unrelated failures never gain this permit.
    pub(crate) fn recover_frame_error(&mut self, error: &SoftwareSceneError) -> bool {
        if error.native_error_code() != Some(0x301) {
            return false;
        }
        self.aborted_presentation = true;
        true
    }

    /// The draw arena overflowed (native 0x301) and the queue stopped there.
    pub fn arena_overflowed(&self) -> bool {
        self.queue.overflowed()
    }

    /// A native 0x301 catch disposing its error (004923C0): the queue
    /// resumes, and a scene stop that overflow caused lifts; others stay.
    pub fn recover_native_301(&mut self) -> bool {
        if !self.queue.recover_overflow() {
            return false;
        }
        if self.stop_cause.as_ref().is_some_and(|cause| cause.overflow) {
            self.stopped = false;
            self.stop_cause = None;
        }
        true
    }

    fn reset_depth_surface(&mut self) {
        if self.pixels.renderer.manages_depth && self.game_draw.is_none() {
            self.pixels.pipeline.depth_enabled = false;
            self.pixels.pipeline.depth_write = true;
            self.pixels.frame.clear_depth();
        }
    }

    fn material_sorting(&self) -> bool {
        self.pixels.renderer.manages_depth && self.pixels.renderer.depth_enable != 0
    }

    fn draw_model(
        &mut self,
        submission: SceneModelSubmission,
        source: RenderSource<'r>,
        model: &'r inf_formats::ModelEntry,
    ) -> Result<(), SoftwareSceneError> {
        let resource = self.resources.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: i32::from(submission.model_slot),
        })?;
        let RenderResourceData::Model(original) = resource.data else {
            unreachable!("typed model")
        };
        if !same_source(source, resource.source) || !std::ptr::eq(model, original) {
            return Err(SoftwareSceneError::Custody);
        }
        // Commit the exact consumed RNG prefix even when a later command fails.
        let mut rng = self.process_rng;
        let result = self.execute_model_program(SoftwareModelDrawRequest {
            root_mode: crate::model_execution::ModelRootMode::CommonVisual,
            environment: self.environment,
            argument: i32::from(submission.model_slot),
            context: ModelContext::new(submission.position, submission.basis),
            callbacks: ModelObjectCallbacks::Ordinary(submission),
            terrain: ModelTerrainRead::RendererEpoch,
            surface_table: submission.surface_table,
            primitive_context: submission.primitive_context,
            failure_policy: submission.failure_policy,
            stack_registers: match submission.failure_policy {
                ModelFailurePolicy::LoadingModel117StackHistoryApproximation => {
                    crate::model_execution::ModelStackRegisterPolicy::LoadingModel117StackHistoryApproximation
                }
                ModelFailurePolicy::ObjectStackHistoryApproximations => {
                    crate::model_execution::ModelStackRegisterPolicy::ObjectStackHistoryApproximations
                }
                ModelFailurePolicy::ShopPreviewStackHistoryApproximation => {
                    crate::model_execution::ModelStackRegisterPolicy::ShopPreviewModel127StackHistoryApproximation
                }
                ModelFailurePolicy::BriefingPreviewMapFrameBases => {
                    crate::model_execution::ModelStackRegisterPolicy::BriefingPreviewMapFrameBases
                }
                ModelFailurePolicy::StopScene
                | ModelFailurePolicy::UninitializedValuePrefixApproximation => {
                    crate::model_execution::ModelStackRegisterPolicy::Unknown
                }
            },
            rng: rng.as_mut(),
        });
        self.process_rng = rng;
        result
    }

    /// One complete geometry/backend phase for ordinary and furniture objects.
    /// The callback and terrain modes are explicit; parser/model bytes remain
    /// independent of scene policy and the renderer owns the same shared queue.
    fn execute_model_program(
        &mut self,
        request: SoftwareModelDrawRequest<'_>,
    ) -> Result<(), SoftwareSceneError> {
        let _scope = crate::cpu_profile::scope("model: execute program");
        let terrain = request.terrain.resolve(self.terrain.as_ref());
        let surface = self.model_surface_binding(request.surface_table, terrain);
        // Context+6C/+70 are invocation-local packet inputs, not persistent
        // renderer globals. Keep existing callers' inherited values explicit.
        let mut primitive_state = self.primitive_state;
        if let ModelPrimitiveContext::Caller { far_color, mode } = request.primitive_context {
            primitive_state.far_color = far_color;
            primitive_state.mode = mode;
        }
        let root_was_open = self.queue.has_only_implicit_root();
        // Vulkan draws the models from their slots' exact twins: the
        // tracer, and the raster's world when untraced.
        let exact = self.scene_feed.is_some() || self.exact_corners;
        let outside_view = self.scene_feed.as_ref().is_some_and(|feed| feed.culled);
        let services = ModelServices {
            callbacks: request.callbacks,
            sorting: self.material_sorting(),
            textures: &mut self.texture_identities,
            process_rng: request.rng,
            feed: self.scene_feed.as_mut(),
            motion: self.raster_motion.as_mut(),
            tint: self.draw_tint,
            sky: self.raster_sky.as_ref(),
            // The game's draw shows nothing.
            metal: if self.game_draw.is_none() {
                self.raster_metals.drawing()
            } else {
                None
            },
        };
        let mut commands = ModelCommandBackend::new(
            self.resources,
            self.tables,
            primitive_state,
            &mut self.queue,
            services,
        );
        let mut executor = ModelExecutor::new(
            self.resources,
            Arc::clone(&self.tables.sine_table),
            self.execution_limits,
        );
        executor.set_stack_register_policy(request.stack_registers);
        executor.set_root_registers(self.furniture_root_registers);
        executor.set_mirror_copy(self.mirror_copy);
        executor.set_hovercraft_fans(self.hovercraft_fans);
        executor.set_exact_twins(exact);
        executor.set_outside_view(outside_view);
        let result = {
            let mut publication = crate::model_process_publication::ModelProcessPublication {
                inner: &mut commands,
                process: &self.graphics,
                table: request.surface_table,
            };
            executor.draw(
                ModelDrawRequest {
                    scratch: &self.scratch,
                    root_mode: request.root_mode,
                    model_argument: request.argument,
                    context: request.context,
                    environment: request.environment,
                    surface,
                },
                &mut publication,
            )
        };
        if request.stack_registers
            == crate::model_execution::ModelStackRegisterPolicy::FurnitureRootCarry
        {
            self.furniture_root_registers = executor.root_registers();
        }
        self.stats.model_commands += commands.outcome().commands;
        self.stats.rejected_primitives += commands.outcome().rejected;
        if result
            .as_ref()
            .is_ok_and(|outcome| outcome.stack_register_seeds != 0)
        {
            log_stack_register_seed(request.stack_registers);
        }
        if let Err(ModelExecutionError::Value(ModelValueError::Fault(
            ModelValueFault::UninitializedRegister { index },
        ))) = &result
        {
            if matches!(
                request.failure_policy,
                ModelFailurePolicy::UninitializedValuePrefixApproximation
                    | ModelFailurePolicy::LoadingModel117StackHistoryApproximation
                    | ModelFailurePolicy::ObjectStackHistoryApproximations
                    | ModelFailurePolicy::ShopPreviewStackHistoryApproximation
            ) && root_was_open
                && self.queue.has_only_implicit_root()
            {
                // The named portable unwind retires registered marks without
                // clearing reached bytes. Submitted packets, publications and RNG
                // remain owned; the caller must reach its actual common flush.
                self.recover_stopped_model_scratch()?;
                self.stats.model_value_prefix_skips += 1;
                if self.stats.model_value_prefix_skips == 1 {
                    eprintln!("Infestation: model {} remainder skipped at uninitialized register {index} (UninitializedValuePrefixApproximation; MODEL_RUNTIME.md)", request.argument);
                }
                return Ok(());
            }
        }
        result?;
        self.stats.models += 1;
        Ok(())
    }

    /// Resolve the original table origin before borrowing the command backend.
    /// Template callbacks ignore terrain availability and the selected table;
    /// inherited callbacks preserve the reached installer and current globals.
    fn model_surface_binding<'t>(
        &self,
        table: ModelSurfaceTableOrigin,
        terrain: Option<&'t TerrainRuntime>,
    ) -> crate::model_vertices::ModelSurfaceBinding<'t> {
        match table {
            ModelSurfaceTableOrigin::TemplateDefault => {
                crate::model_vertices::ModelSurfaceBinding::Default
            }
            ModelSurfaceTableOrigin::InheritedRenderer => match self.surface_callbacks {
                RendererSurfaceCallbacks::Default => {
                    crate::model_vertices::ModelSurfaceBinding::Default
                }
                RendererSurfaceCallbacks::Terrain {
                    world_flags,
                    wave_clock,
                } => crate::model_vertices::ModelSurfaceBinding::Terrain {
                    terrain,
                    world_flags,
                    wave_clock,
                    // Covered renderer copies publish these same fields to
                    // the view eye before draw; model origin is separate.
                    renderer_xyz: self.environment.view.eye,
                },
            },
        }
    }

    fn star(&mut self, quad: StarQuad, color_word: u32) -> Result<(), SoftwareSceneError> {
        let depth = if self.material_sorting() {
            0x4000_0000
        } else {
            0
        };
        let command = star_command(quad, color_word, depth);
        self.queue.submit(DrawSubmission {
            depth,
            payload_blocks: 3,
            receiver_offset: 0,
            command,
        })?;
        self.stats.stars += 1;
        Ok(())
    }

    fn footer(
        &mut self,
        quad: FooterQuad,
        sprite: RenderSprite<'r>,
    ) -> Result<(), SoftwareSceneError> {
        let resource = self.resources.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: i32::from(quad.sprite_slot),
        })?;
        let selected = resource.select_sprite(SpriteRenderMode::Mode0)?;
        if !same_source(sprite.source, selected.source)
            || !std::ptr::eq(sprite.entry, selected.entry)
            || !std::ptr::eq(sprite.variant, selected.variant)
        {
            return Err(SoftwareSceneError::Custody);
        }
        // Preserve the footer's exact admission/resource checks above, mode0,
        // equal vertex DWORDs and +A4 effect color while sharing submission.
        self.draw_sprite_direct(DirectSpriteQuad {
            sprite_slot: quad.sprite_slot,
            corners: quad.corners,
            vertex_depths: [quad.vertex_attribute; 4],
            descriptor_word: 0,
            mode: DirectSpriteMode::Effect {
                color_word: quad.style_word,
            },
        })?;
        self.stats.footers += 1;
        Ok(())
    }

    /// Native direct +84 packet, preserving per-vertex depths and +20/+24.
    /// A failure retains reached pixel state and leaves the independent
    /// deferred arena available for its required later flush.
    pub fn draw_flat_quad_direct(
        &mut self,
        quad: DirectFlatQuad,
    ) -> Result<(), SoftwareSceneError> {
        if self.stopped {
            return Err(SoftwareSceneError::Stopped);
        }
        let mut command = PrimitiveCommand::new(PrimitiveCallback::FlatQuadNear, 0, 3);
        for (index, corner) in quad.corners.into_iter().enumerate() {
            command.write_u32(
                index * 8,
                u32::from(corner[0] as u16) | (u32::from(corner[1] as u16) << 16),
            );
            command.write_u32(index * 8 + 4, quad.vertex_depths[index]);
        }
        command.write_u32(0x20, quad.descriptor_word);
        command.write_u32(0x24, quad.color_word);
        self.pixels.draw(
            DrawInvocation {
                command_offset: 0,
                receiver_offset: 0,
            },
            &mut command,
        )?;
        self.stats.direct_flat_quads += 1;
        Ok(())
    }

    /// Native direct +C4 packet (004A3940 -> 004E6B70). Like the other direct
    /// packets it neither allocates nor flushes the deferred arena; the
    /// explicit-UV consumer selects the sprite variant and blend.
    pub fn draw_shade_uv_quad_direct(
        &mut self,
        quad: DirectShadeUvQuad,
    ) -> Result<(), SoftwareSceneError> {
        let result = (|| {
            if self.stopped {
                return Err(SoftwareSceneError::Stopped);
            }
            let resource = self.resources.resolve(RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: quad.sprite_argument,
            })?;
            let mut command =
                PrimitiveCommand::new(PrimitiveCallback::TexturedShadeUvQuadNear, 0, 6);
            for (index, corner) in quad.corners.into_iter().enumerate() {
                command.write_u32(
                    index * 8,
                    u32::from(corner[0] as u16) | (u32::from(corner[1] as u16) << 16),
                );
                command.write_u32(index * 8 + 4, quad.vertex_depths[index]);
            }
            command.write_u32(0x20, quad.mode);
            command.write_texture(0x24, Some(resource));
            for (index, shade) in quad.shades.into_iter().enumerate() {
                command.write_u32(0x28 + index * 4, shade);
            }
            for (index, [u, v]) in quad.uv.into_iter().enumerate() {
                command.write_u32(0x38 + index * 8, u as u32);
                command.write_u32(0x3c + index * 8, v as u32);
            }
            self.pixels.draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )?;
            self.stats.direct_shade_uv_quads += 1;
            Ok(())
        })();
        if let Err(error) = &result {
            self.latch_stop(error);
        }
        result
    }

    /// Native direct +9C/+A4 quads. This neither allocates nor flushes the
    /// deferred arena; sprite selection, blend and clipping execute through
    /// the same ordinary/effect consumers used by H8 primitive commands.
    pub fn draw_sprite_direct(&mut self, quad: DirectSpriteQuad) -> Result<(), SoftwareSceneError> {
        let result = (|| {
            if self.stopped {
                return Err(SoftwareSceneError::Stopped);
            }
            let resource = self.resources.resolve(RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: i32::from(quad.sprite_slot),
            })?;
            let callback = match quad.mode {
                DirectSpriteMode::Textured => PrimitiveCallback::TexturedQuadNear,
                DirectSpriteMode::Effect { .. } => PrimitiveCallback::EffectQuadNear,
            };
            let mut command = PrimitiveCommand::new(callback, 0, 3);
            for (index, corner) in quad.corners.into_iter().enumerate() {
                command.write_u32(
                    index * 8,
                    u32::from(corner[0] as u16) | (u32::from(corner[1] as u16) << 16),
                );
                command.write_u32(index * 8 + 4, quad.vertex_depths[index]);
            }
            command.write_u32(0x20, quad.descriptor_word);
            command.write_texture(0x24, Some(resource));
            if let DirectSpriteMode::Effect { color_word } = quad.mode {
                command.write_u32(0x28, color_word);
            }
            self.pixels.draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )?;
            self.stats.direct_sprites += 1;
            Ok(())
        })();
        if let Err(error) = &result {
            self.latch_stop(error);
        }
        result
    }

    fn submit_inner(
        &mut self,
        command: ResolvedSceneCommand<'r>,
    ) -> Result<(), SoftwareSceneError> {
        match command {
            ResolvedSceneCommand::BeginBatch => {
                self.queue.reset();
                self.stopped = false;
                self.stop_cause = None;
                self.aborted_presentation = false;
                self.stats.begins += 1;
                return Ok(());
            }
            _ if self.stopped => return Err(SoftwareSceneError::Stopped),
            _ => {}
        }
        match command {
            ResolvedSceneCommand::BeginBatch => unreachable!(),
            ResolvedSceneCommand::CoordinateShift(shift) => {
                self.environment.coordinate_shift = shift;
                self.graphics.publish_shift(shift)?;
            }
            ResolvedSceneCommand::FarColor(color) => self.primitive_state.far_color = color,
            ResolvedSceneCommand::DirectionalLight { direction, rgb } => {
                self.environment.lighting.direction = direction;
                // 004901B0 reads only the low three directional bytes. The
                // scene source specifies no fourth byte, so it is not an input.
                self.environment.lighting.directional_word =
                    u32::from_le_bytes([rgb[0], rgb[1], rgb[2], 0]);
                let ambient = self.environment.lighting.ambient_word.to_le_bytes();
                let effect = std::array::from_fn(|i| ambient[i].saturating_add(rgb[i] >> 1));
                self.primitive_state.effect_color =
                    replace_rgb(self.primitive_state.effect_color, effect);
            }
            ResolvedSceneCommand::AmbientLight(rgb) => {
                self.environment.lighting.ambient = rgb;
                self.environment.lighting.ambient_word = replace_rgb(
                    self.environment.lighting.ambient_word,
                    rgb.map(|value| value.clamp(0, 255) as u8),
                );
            }
            ResolvedSceneCommand::Setup(setup) => {
                self.environment.projection = ModelProjection {
                    focal: setup.focal,
                    viewport: setup.viewport,
                    center: setup.center,
                    fade: setup.fade,
                };
                self.environment.fog_factor = setup.fade_factor;
                self.primitive_state.projection = self.environment.projection;
                self.graphics.publish_setup(setup)?;
            }
            ResolvedSceneCommand::PublishFade(fade) => {
                self.environment.projection.fade = fade.bounds;
                self.environment.fog_factor = fade.factor;
                self.primitive_state.projection.fade = fade.bounds;
                self.graphics.publish_fade(fade)?;
            }
            ResolvedSceneCommand::FlushBatch => {
                if self.game_draw.is_some() {
                    // The game's draw: the queue closes its groups and
                    // replays as a drawn one does, and draws nothing.
                    self.queue.flush(|_, _| Ok::<_, SoftwarePixelError>(()))?;
                } else {
                    flush_pixels(&mut self.queue, &mut self.pixels)?;
                }
                self.stats.flushes += 1;
            }
            ResolvedSceneCommand::StarView(view) | ResolvedSceneCommand::PublishView(view) => {
                self.environment.view = view;
                self.graphics.publish_view(view)?;
            }
            ResolvedSceneCommand::Star {
                quad, color_word, ..
            } => self.star(quad, color_word)?,
            ResolvedSceneCommand::Model {
                submission,
                source,
                model,
            } => self.draw_model(submission, source, model)?,
            ResolvedSceneCommand::ResetAfterFirstModel => {
                // 004BCA20(NULL) ->0050DC60: does not change renderer+38/+3C/+40.
                self.reset_renderer();
            }
            ResolvedSceneCommand::Footer { quad, sprite } => self.footer(quad, sprite)?,
        }
        Ok(())
    }
}

impl<'r> SceneResourceBackend<'r> for SoftwareScene<'_, 'r> {
    type Error = SoftwareSceneError;

    fn color_layout(&self) -> NativeColorLayout {
        self.color_layout
    }

    fn sprite_mode(&self) -> SpriteRenderMode {
        SpriteRenderMode::Mode0
    }

    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok([
            self.pixels.frame.width() as i32,
            self.pixels.frame.height() as i32,
        ])
    }

    fn submit(&mut self, command: ResolvedSceneCommand<'r>) -> Result<(), Self::Error> {
        let result = self.submit_inner(command);
        if let Err(error) = &result {
            self.latch_stop(error);
        }
        result
    }
}

fn same_source(a: RenderSource<'_>, b: RenderSource<'_>) -> bool {
    std::ptr::eq(a.registration, b.registration) && std::ptr::eq(a.bytes, b.bytes)
}

fn star_command(quad: StarQuad, color_word: u32, depth: u32) -> PrimitiveCommand<'static> {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::FlatQuadNear, depth, 3);
    write_quad(&mut command, quad.corners, 0x7fff_ffff);
    command.write_u32(0x20, 0);
    command.write_u32(0x24, color_word);
    command
}

fn replace_rgb(word: u32, rgb: [u8; 3]) -> u32 {
    u32::from_le_bytes([rgb[0], rgb[1], rgb[2], word.to_le_bytes()[3]])
}

fn write_quad(command: &mut PrimitiveCommand<'_>, corners: [[i16; 2]; 4], depth: u32) {
    for (i, [x, y]) in corners.into_iter().enumerate() {
        command.write_u32(i * 8, u32::from(x as u16) | (u32::from(y as u16) << 16));
        command.write_u32(i * 8 + 4, depth);
    }
}

#[cfg(test)]
#[path = "software_scene_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "software_scene/scratch_tests.rs"]
mod scratch_tests;
