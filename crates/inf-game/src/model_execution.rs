//! Executed Handler-8 geometry/control path, ending at required native primitive
//! and deferred-queue operations. This is not a diagnostic static-mesh visitor.

use std::convert::Infallible;
use std::sync::Arc;

use inf_formats::{ModelEntry, ModelInstruction, ModelInstructionKind, ModelProgram};
use thiserror::Error;

use crate::{
    frontend_scene::SceneView,
    menu_basis::MenuBasisMath,
    model_context::{ModelContext, ModelDispatch, ModelProjection, ModelTransform},
    model_lighting::{ModelLighting, ModelLightingError, ModelShades},
    model_projection::ModelProjectionBackend,
    model_scratch::{
        ModelScratchAllocation, ModelScratchDrawExit, ModelScratchDrawScope, ModelScratchError,
        ProcessModelScratch,
    },
    model_values::{
        ModelRegisterBlock, ModelValueBranch, ModelValueCallback, ModelValueError, ModelValueFault,
        ModelValues,
    },
    model_vertices::{
        ModelCustomVertexError, ModelCustomVertexRequest, ModelSurfaceBinding,
        ModelSurfaceCallbackInputs, ModelVertexError, ModelVertexFault, ModelVertices,
    },
    render_resources::{
        RenderResourceData, RenderResourceError, RenderResourceKind, RenderResourceRequest,
        RenderResources, RenderSource,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct ModelEnvironment {
    pub view: SceneView,
    pub projection: ModelProjection,
    pub coordinate_shift: u8,
    /// Live 0055F244, established by 0048FB00, not recomputed on each vertex.
    pub fog_factor: i32,
    pub lighting: ModelLighting,
}

/// Explicit Rust containment limits, not recovered retail timing or draw policy.
#[derive(Clone, Copy, Debug)]
pub struct ModelExecutionLimits {
    pub instructions: u64,
    pub invocation_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelRootMode {
    /// 0049FEA0/0049FEF0, with visibility, projection and shade initialization.
    CommonVisual,
    /// 004ED610, a required resource and the distinct direct table00544764.
    ManagerLate,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelDrawRequest<'a> {
    /// Actual retained process arena, shared unchanged by both root modes and children.
    pub scratch: &'a ProcessModelScratch,
    pub root_mode: ModelRootMode,
    pub model_argument: i32,
    pub context: ModelContext,
    pub environment: ModelEnvironment,
    /// Actual renderer+50 callback binding, retained unchanged by child0E.
    pub surface: ModelSurfaceBinding<'a>,
}

/// Inputs to the selected native primitive handler. The required backend owns
/// clipping, face/fan culling, material resolution, 1B effects and actual queue
/// submission. It must not substitute a static mesh or silently ignore a record.
pub struct ModelPrimitiveRequest<'a> {
    pub model: &'a ModelEntry,
    pub source: RenderSource<'a>,
    pub instruction: &'a ModelInstruction,
    pub context: &'a ModelContext,
    pub transform: ModelTransform,
    pub dispatch: ModelDispatch,
    pub vertices: &'a ModelVertices,
    pub shades: &'a ModelShades,
    /// Child context +6C references the original 0E state words, not registers.
    pub state_words: &'a [u16],
}

pub trait ModelExecutionHost: ModelValueCallback {
    /// Publish 0048FB40's world and composed-view state at the native boundary.
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error>;
    /// 004A00E0 selects the projection entries on the shared callback table;
    /// normal child return restores the parent's selection separately from view.
    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error>;
    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error>;
    /// EE -> 004A4740(depth, 0), a deferred queue group, not projection bias.
    fn begin_depth_group(&mut self, depth: i32, sorting: bool) -> Result<(), Self::Error>;
    /// EF -> 004A44A0, closes/sorts/pops the actual group owner.
    fn end_depth_group(&mut self) -> Result<(), Self::Error>;
    /// Original sine table for spacing-sample controls (opcode 0x2C).
    /// Defaults to none, preserving the previous unsupported boundary;
    /// the scene backend overrides it from its primitive tables.
    fn sine_quarter_table(&self) -> Option<&[u16; 4096]> {
        None
    }
    /// Process RNG word 0055F18C for jitter controls (opcode 0x22).
    /// Defaults to none, preserving the previous unsupported boundary;
    /// the world backend overrides it with the live session word.
    fn process_rng(&mut self) -> Option<&mut u32> {
        None
    }
    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        crate::model_vertices::ModelCustomVertexPolicy::Unowned
    }
    /// Furniture 004358A0 requires its scoped process services. A selected
    /// Furniture policy must never silently inherit an inert compute callback.
    fn furniture_custom_vertex(
        &mut self,
        _: ModelCustomVertexRequest<'_>,
        _: &mut ModelVertices,
        _: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        Err(ModelVertexFault::UnownedCustomVertex.into())
    }

    /// Direct-table2D forces the actual submitted source's compute callback.
    /// This is independent of the common visual/furniture policy and retains
    /// its temporary and already reached effects on failure.
    fn required_custom_vertex(
        &mut self,
        _: ModelCustomVertexRequest<'_>,
        _: &mut ModelVertices,
        _: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        Err(ModelVertexFault::UnownedCustomVertex.into())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelExecutionOutcome {
    pub invocations: usize,
    pub culled: usize,
    pub instructions: u64,
    pub primitive_calls: u64,
    pub depth_groups: usize,
    /// Register files seeded by `ModelStackRegisterPolicy`.
    pub stack_register_seeds: usize,
}

/// A root context is a stack local of 0049FEA0 that writes no register, and
/// a child context is a stack local of 004A0ED0 that copies only registers
/// 0..3 from its parent. The rest hold whatever that stack held before.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelStackRegisterPolicy {
    /// Unwritten registers stay unknown, so reading one stops the invocation.
    #[default]
    Unknown,
    /// The briefing preview after the map panel's frame. Before 004A00E0,
    /// 0049FEF0 builds five identity bases (0040D6A0) in its context at
    /// +14, +A0, +D4, +108 and +13C, none over its own registers. The map
    /// frame's call (0040C390, 00455080) runs 0x50 bytes below the
    /// preview's (0040B840), so its second base's last row (0, 0, 1.0f)
    /// becomes the preview root's registers 0..3, and its third base's
    /// leading zero and first two rows registers 10..21. Every one of the
    /// 131 root reads of thumper H8 46's register 1 in Infest02's Terra
    /// Nova briefing found these 16 values, the last write being that
    /// construction (MODEL_RUNTIME.md). Not an approximation; registers
    /// 4..9 stay unknown.
    BriefingPreviewMapFrameBases,
    /// WingModel457StackHistoryApproximation: global H8 457 (`gryfwing2`,
    /// the middle of each griffin wing, in every world that loads it) reads
    /// register 9 before writing it. Its child context 1AF4E0 holds whatever
    /// last ran at that stack depth: Infest01's intro read 56 (a scratch-arena
    /// pointer's high word) in 796 of 830 reads; Infest04's Oolbryte and
    /// Helniazine read heap pointers' high words (C9F, C92, F73, C98) in 313
    /// of 415, and 0 or C7 in the rest. The port starts it at 56 everywhere.
    WingModel457StackHistoryApproximation,
    /// LoadingModel117StackHistoryApproximation: the loading model's root
    /// (level 1 H8 117) never writes register 3 before child 131, entered
    /// through child 119, reads its copy at +40. All 68 such reads in
    /// Infest01's post-intro loading screen used child context 1AF394, whose
    /// copy held 6EBB in 64 reads and 0 in 4. The port starts the root's
    /// register 3 at 6EBB.
    LoadingModel117StackHistoryApproximation,
    /// The object draw's seeds: the griffin wing's child 457 as
    /// `WingModel457StackHistoryApproximation`, and
    /// VehicleModel117StackHistoryApproximation: the vehicle draws the same
    /// H8 117, whose child 131 runs while a mode change is in progress and
    /// reads the root's unwritten register 3 as on the loading screen. In
    /// gameplay that copy changes from frame to frame (Infest01's first
    /// morph: 7420, 8BE0, 7882, 625E, B124, 4EDC), so no recorded value is
    /// the native one; the port starts it at the loading screen's 6EBB, so
    /// the morph draws with one fixed turn instead of losing the vehicle.
    /// SkimmerModel127StackHistoryApproximation: the settled skimmer, the
    /// root's child 127, branches on its register 15 before writing it. All
    /// 16 such reads in Infest03 (child context 1AF690) found 0, which skips
    /// three triangles; the port starts it at 0.
    /// SkimmerMorphModel126StackHistoryApproximation: while the vehicle
    /// turns into the skimmer (the mode table's 5, progress not FFFF), the
    /// root runs child 126 instead, which branches twice on its register 15
    /// before writing it, and 126's child 141 branches once on its own,
    /// which it never writes. Infest03's two morph frames read 0 in all
    /// six (126 in child context 1AF690, 141 in 1AF460): 126 draws its two
    /// fins in a depth group and 141 skips its block. The port starts both
    /// at 0.
    /// LaserMineModel85StackHistoryApproximation: an unarmed laser mine
    /// (model value 1, the mine's +138, is 0) jumps over the pair that
    /// writes its root's register 3, then branches on it before its third
    /// fin and its base. All 17 such reads in Infest03 (root context 1AF940,
    /// both throws) found it other than 1, 0xCF2 in 13 and 0 in 4, and
    /// jumped: the fin's three lights stayed dark and the rest drew. The
    /// port starts it at 0xCF2.
    /// ProximityMineModel84StackHistoryApproximation: the proximity mine's
    /// model 84 has the same unarmed path, its `0016 [8, C3, 1]` reading
    /// register 3 ahead of the lights and the body. All 9 such reads of the
    /// lab's retail throw (`proximity-mine-throw.json`, root context
    /// 1AF940) found 0x2861 and jumped; the port starts it at 0x2861.
    /// ThumperModel46StackHistoryApproximation: thumper H8 46 (Terra Nova's
    /// generator, the selector-39 course target) opens with three `0A26`
    /// lerps whose factor is its root register 1, written only after them.
    /// In gameplay the slot holds what last ran at that depth: all 21
    /// gameplay draws of the lab's Terra Nova scene beside the generator
    /// read 0x1A, the high word of the stack address 001AF558 left in
    /// registers 0..1 (`terranova-generator.json`, `004A00E0`'s context
    /// +70), and Infest02's briefing terrain pane read 0 in eight of ten
    /// and 0x1A in two. The port starts it at 0x1A.
    /// ExplosionWaveModel112StackHistoryApproximation: the blast wave H8 112
    /// (`exwave4`, selector 22's resource 0x38, the Alien Brain attack's
    /// blast) only runs its child 115 (`exwave7`), eight of whose `0A26`
    /// lerps take register 1 as their factor; neither model writes it,
    /// and the child's registers 0..3 are the root's. All 22 draws of the
    /// lab's Foldrak Noir blasts (`foldrak-blast-wave.json`) found registers
    /// 0..1 holding the stack address 001AF570 at both, register 1 its high
    /// word 0x1A, as the thumper's. The port starts the root's register 1 at
    /// 0x1A.
    ObjectStackHistoryApproximations,
    /// ShopPreviewModel127StackHistoryApproximation: the factory shop's
    /// turning preview (`0042FD70`) of Skimmer mode draws H8 117 in form 5,
    /// whose settled child 127 branches on its unwritten register 15 as in
    /// gameplay, but from child context 1AF488, not gameplay's 1AF690. There
    /// it is residue that differs even between twin runs: three lab runs of
    /// retail's Artoria shop read 0xA195, 0x05F1 and 0xD39A (104 reads,
    /// `artoria-skimmer-register.json`), never 0, and every branch fell
    /// through to the three triangles gameplay skips. The port starts it at
    /// 0xA195; its hook-free preview frames match retail's
    /// (`artoria-skimmer-shop.json`).
    ShopPreviewModel127StackHistoryApproximation,
    /// Terrain furniture: every item's root runs at the same depth
    /// (0044CEF0 -> 004359A0 -> 0049FEA0), so a root that reads a register
    /// before writing it finds what the previous furniture root left there.
    /// Helniazine's thumper (H8 46) read model 380's last register 1 in all
    /// 126 draws of the lab's three views (`helniazine-thumper.json`); the
    /// next frame's first root found another value, so the caller clears
    /// the block at each walk ([`ModelExecutor::set_root_registers`]).
    FurnitureRootCarry,
}

/// The seeded register and value of the wing approximation above.
pub const WING_MODEL_457_REGISTER_9: (i16, u16) = (9, 0x56);
/// The seeded root register and value of the loading approximation above.
pub const LOADING_MODEL_117_REGISTER_3: (i16, u16) = (3, 0x6ebb);
/// The seeded child register and value of the skimmer approximation above.
pub const SKIMMER_MODEL_127_REGISTER_15: (i16, u16) = (15, 0);
/// The seeded child register and value of the shop preview's skimmer.
pub const SHOP_PREVIEW_MODEL_127_REGISTER_15: (i16, u16) = (15, 0xa195);
/// The seeded child register and value of the skimmer morph approximation,
/// for 126 and its child 141.
pub const SKIMMER_MORPH_MODEL_126_REGISTER_15: (i16, u16) = (15, 0);
/// The seeded root register and value of the laser mine approximation above.
pub const LASER_MINE_MODEL_85_REGISTER_3: (i16, u16) = (3, 0xcf2);
/// The seeded root register and value of the proximity mine approximation.
pub const PROXIMITY_MINE_MODEL_84_REGISTER_3: (i16, u16) = (3, 0x2861);
/// The seeded root register and value of the thumper approximation above.
pub const THUMPER_MODEL_46_REGISTER_1: (i16, u16) = (1, 0x1a);
/// The seeded root register and value of the blast wave approximation above.
pub const EXPLOSION_WAVE_MODEL_112_REGISTER_1: (i16, u16) = (1, 0x1a);
/// The briefing preview's root registers from the map frame's bases:
/// row (0, 0, 1.0f)'s Y and Z, then a zero DWORD, row (1.0f, 0, 0) and
/// row (0, 1.0f, ...)'s X and Y, as WORDs.
pub const BRIEFING_PREVIEW_MAP_FRAME_REGISTERS: [(i16, u16); 16] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0x3f80),
    (10, 0),
    (11, 0),
    (12, 0),
    (13, 0x3f80),
    (14, 0),
    (15, 0),
    (16, 0),
    (17, 0),
    (18, 0),
    (19, 0),
    (20, 0),
    (21, 0x3f80),
];

#[derive(Debug, Error)]
pub enum ModelExecutionError<E> {
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error("model packed value: {0}")]
    Value(ModelValueError<E>),
    #[error("model vertex: {0}")]
    Vertex(ModelVertexError<E>),
    #[error(transparent)]
    Lighting(#[from] ModelLightingError),
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error("required model backend operation failed")]
    Host(E),
    #[error("model opcode {opcode:#04x} at source byte {offset:#x} is unsupported")]
    Unsupported { opcode: u8, offset: usize },
    #[error("invalid model record at source byte {0:#x}")]
    Record(usize),
    #[error("child model slot {0} is invalid at the native model-header dereference")]
    InvalidChild(i16),
    #[error("late root model slot {0} is NULL at the required native model-header dereference")]
    InvalidLateRoot(i16),
    #[error("branch at source byte {offset:#x} does not target a decoded instruction boundary")]
    Branch { offset: usize },
    #[error("model execution reached its explicit instruction/depth limit")]
    Limit,
    #[error("model execution stopped after an earlier failure")]
    Stopped,
}

/// Resident bytes are borrowed for the entire execution. Each root and child
/// owns its actual transient values and range views into the borrowed process
/// scratch. An error stops this owner, retaining registered marks, bytes and
/// backend effects; no implicit view, queue, resource or scratch recovery runs.
pub struct ModelExecutor<'a> {
    resources: RenderResources<'a>,
    sine_table: Arc<[u16; 4096]>,
    basis_math: MenuBasisMath,
    limits: ModelExecutionLimits,
    stack_registers: ModelStackRegisterPolicy,
    /// The carried root block under `FurnitureRootCarry`.
    root_registers: ModelRegisterBlock,
    mirror_copy: crate::model_vertices::MirrorCopySource,
    /// Port fix `hovercraft_fans` ([`hovercraft_fans_unmirrored`]).
    hovercraft_fans: bool,
    /// Non-native: the slots keep exact twins for Vulkan's drawing (the
    /// plan's item 15).
    exact: bool,
    /// Non-native: the tracer's models outside the view
    /// ([`Self::set_outside_view`]).
    outside_view: bool,
    stopped: bool,
}

/// The settled hovercraft's model, `hoverjet2` (H8/121).
const SETTLED_HOVERCRAFT: u16 = 121;

/// Whether opcode 03 of model `model`, saving with `orientation`, takes
/// the port fix: `hoverjet2` saves its rear fans' basis (`scatfan`, H8/123)
/// with orientation `0x20`, X negated, a mirror, then turns it by the steer
/// in its X-Z plane. The mirror makes their faces cull inside out and draw
/// the fans' black insides over them, in every renderer. With `fix`, the
/// turned basis takes its X axis back: the fans are symmetric across X, so
/// their corners stand where retail's do, swivel included, and only their
/// culling changes. Saving with orientation 0 instead, as the transforming
/// `hoverjet1` does, turned them the other way (2026-10-24: a
/// full left made them turn to the right), since a mirror reverses a turn
/// in its plane.
pub fn hovercraft_fans_unmirrored(model: u16, orientation: u16, fix: bool) -> bool {
    fix && model == SETTLED_HOVERCRAFT && orientation == 0x20
}

impl<'a> ModelExecutor<'a> {
    pub fn new(
        resources: RenderResources<'a>,
        sine_table: Arc<[u16; 4096]>,
        limits: ModelExecutionLimits,
    ) -> Self {
        Self {
            resources,
            basis_math: MenuBasisMath::shared(Arc::clone(&sine_table)),
            sine_table,
            limits,
            stack_registers: ModelStackRegisterPolicy::Unknown,
            root_registers: ModelRegisterBlock::default(),
            mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
            hovercraft_fans: false,
            exact: false,
            outside_view: false,
            stopped: false,
        }
    }

    /// Keep exact twins of every model's slots, a child's origin its
    /// parent slot's twin: the tracer then draws the models rigid and
    /// smooth, while the game reads retail's whole units.
    pub fn set_exact_twins(&mut self, on: bool) {
        self.exact = on;
    }

    /// The models draw for the tracer's feed alone, outside the view
    /// (`world_scene`'s culled models): no frustum gate (0049FEF0's), and
    /// the near and far tables and the far cut-off by distance from the eye
    /// rather than by depth along the view, which behind the eye is
    /// negative.
    pub fn set_outside_view(&mut self, on: bool) {
        self.outside_view = on;
    }

    pub fn set_stack_register_policy(&mut self, policy: ModelStackRegisterPolicy) {
        self.stack_registers = policy;
    }

    /// The block a `FurnitureRootCarry` root starts over.
    pub fn set_root_registers(&mut self, block: ModelRegisterBlock) {
        self.root_registers = block;
    }

    /// The block after the last draw: a culled root leaves it as it was and
    /// a failed one leaves nothing known.
    pub fn root_registers(&self) -> ModelRegisterBlock {
        self.root_registers
    }

    /// Opcode 2A's mirror copy: retail's partner slot unless the port fix
    /// is chosen.
    pub fn set_mirror_copy(&mut self, source: crate::model_vertices::MirrorCopySource) {
        self.mirror_copy = source;
    }

    /// Port fix `hovercraft_fans`: retail's mirrored fans unless chosen.
    pub fn set_hovercraft_fans(&mut self, enabled: bool) {
        self.hovercraft_fans = enabled;
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    pub fn draw<H: ModelExecutionHost>(
        &mut self,
        request: ModelDrawRequest<'_>,
        host: &mut H,
    ) -> Result<ModelExecutionOutcome, ModelExecutionError<H::Error>> {
        if self.stopped {
            return Err(ModelExecutionError::Stopped);
        }
        let scratch_scope = match request.scratch.begin_draw() {
            Ok(scope) => scope,
            Err(error) => {
                self.stopped = true;
                return Err(ModelExecutionError::Scratch(error));
            }
        };
        let mut run = Run {
            executor: self,
            environment: request.environment,
            surface: request.surface,
            outcome: ModelExecutionOutcome::default(),
            scratch: request.scratch,
            scratch_scope,
            root_exit: None,
        };
        let result = (|| {
            if matches!(request.root_mode, ModelRootMode::ManagerLate) {
                return run.late_root(request.model_argument, request.context, host);
            }
            // 0049FEA0 skips 0049FEF0 entirely for a known invalid signed slot.
            let Some(resource) = run.resolve(request.model_argument)? else {
                return Ok(());
            };
            let sine_table = Arc::clone(&run.executor.sine_table);
            let mut values = match run.executor.stack_registers {
                ModelStackRegisterPolicy::FurnitureRootCarry => {
                    ModelValues::over_register_block(sine_table, run.executor.root_registers)
                }
                _ => ModelValues::new_invocation(sine_table),
            };
            run.seed_root_registers(&resource, &mut values)?;
            run.invoke(
                InvocationSeed {
                    resource,
                    context: request.context,
                    values,
                    state_words: &[],
                    parent_vertices: None,
                    depth: 1,
                    publications: 1,
                    exact_origin: None,
                },
                host,
            )?;
            Ok(())
        })();
        if run.executor.stack_registers == ModelStackRegisterPolicy::FurnitureRootCarry {
            match (&result, run.root_exit) {
                (Ok(()), Some(block)) => run.executor.root_registers = block,
                (Ok(()), None) => {}
                (Err(_), _) => run.executor.root_registers = ModelRegisterBlock::default(),
            }
        }
        let exit = if result.is_err() {
            run.executor.stopped = true;
            ModelScratchDrawExit::StoppedOnError
        } else {
            ModelScratchDrawExit::NormalReturn
        };
        // Only normal invocation returns retired their registered marks.
        // A failed draw holds its actual marks/cursor/bytes for the caller
        // to retain or explicitly recover under the named approximation.
        if let Err(error) = run.scratch.finish_draw(&run.scratch_scope, exit) {
            run.executor.stopped = true;
            return Err(ModelExecutionError::Scratch(error));
        }
        result.map(|()| run.outcome)
    }
}

#[derive(Clone, Copy)]
struct ModelResource<'a> {
    model: &'a ModelEntry,
    source: RenderSource<'a>,
}

/// Per-entry state. Root and child construction deliberately differ in inherited
/// words, state-pointer ownership, and the number of native publications.
/// `parent_vertices` retains the invoker's range view for 0x2A. Process
/// views share the same arena; the copy handler reads all 24 actual bytes,
/// independently of the decoded cache or partially known world triple.
struct InvocationSeed<'a, 's> {
    resource: ModelResource<'a>,
    context: ModelContext,
    values: ModelValues,
    state_words: &'s [u16],
    parent_vertices: Option<ModelVertices>,
    depth: usize,
    publications: usize,
    /// Non-native: the model's origin unrounded, a child's from its parent
    /// slot's twin (none: its whole-unit position).
    exact_origin: Option<[f64; 3]>,
}

struct Run<'a, 'b, 't> {
    executor: &'b mut ModelExecutor<'a>,
    environment: ModelEnvironment,
    surface: ModelSurfaceBinding<'t>,
    outcome: ModelExecutionOutcome,
    scratch: &'t ProcessModelScratch,
    scratch_scope: ModelScratchDrawScope,
    /// The root's registers when its program returned.
    root_exit: Option<ModelRegisterBlock>,
}

impl<'a, 't> Run<'a, '_, 't> {
    /// Applies the selected stack register policy to a new root file.
    fn seed_root_registers<E>(
        &mut self,
        resource: &ModelResource<'_>,
        values: &mut ModelValues,
    ) -> Result<(), ModelExecutionError<E>> {
        let registration = resource.source.registration;
        if self.executor.stack_registers == ModelStackRegisterPolicy::BriefingPreviewMapFrameBases {
            for (index, value) in BRIEFING_PREVIEW_MAP_FRAME_REGISTERS {
                values
                    .write_register(index, value)
                    .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            }
            self.outcome.stack_register_seeds += 1;
        }
        if matches!(
            self.executor.stack_registers,
            ModelStackRegisterPolicy::LoadingModel117StackHistoryApproximation
                | ModelStackRegisterPolicy::ObjectStackHistoryApproximations
        ) && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 117
        {
            let (index, value) = LOADING_MODEL_117_REGISTER_3;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        if self.executor.stack_registers
            == ModelStackRegisterPolicy::ObjectStackHistoryApproximations
            && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 85
        {
            let (index, value) = LASER_MINE_MODEL_85_REGISTER_3;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        if self.executor.stack_registers
            == ModelStackRegisterPolicy::ObjectStackHistoryApproximations
            && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 84
        {
            let (index, value) = PROXIMITY_MINE_MODEL_84_REGISTER_3;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        if self.executor.stack_registers
            == ModelStackRegisterPolicy::ObjectStackHistoryApproximations
            && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 46
        {
            let (index, value) = THUMPER_MODEL_46_REGISTER_1;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        if self.executor.stack_registers
            == ModelStackRegisterPolicy::ObjectStackHistoryApproximations
            && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 112
        {
            let (index, value) = EXPLOSION_WAVE_MODEL_112_REGISTER_1;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        Ok(())
    }

    /// Applies the selected stack register policy to a new child file.
    fn seed_child_registers<E>(
        &mut self,
        resource: &ModelResource<'_>,
        values: &mut ModelValues,
    ) -> Result<(), ModelExecutionError<E>> {
        let registration = resource.source.registration;
        // Every world's copy: both griffin roots' wings reach it.
        if matches!(
            self.executor.stack_registers,
            ModelStackRegisterPolicy::WingModel457StackHistoryApproximation
                | ModelStackRegisterPolicy::ObjectStackHistoryApproximations
        ) && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 457
        {
            let (index, value) = WING_MODEL_457_REGISTER_9;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        if self.executor.stack_registers
            == ModelStackRegisterPolicy::ObjectStackHistoryApproximations
            && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 127
        {
            let (index, value) = SKIMMER_MODEL_127_REGISTER_15;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        if self.executor.stack_registers
            == ModelStackRegisterPolicy::ShopPreviewModel127StackHistoryApproximation
            && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && registration.global_index == 127
        {
            let (index, value) = SHOP_PREVIEW_MODEL_127_REGISTER_15;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        if self.executor.stack_registers
            == ModelStackRegisterPolicy::ObjectStackHistoryApproximations
            && registration.level == 1
            && registration.phase == inf_formats::RequestKind::Base
            && registration.handler == 8
            && matches!(registration.global_index, 126 | 141)
        {
            let (index, value) = SKIMMER_MORPH_MODEL_126_REGISTER_15;
            values
                .write_register(index, value)
                .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
            self.outcome.stack_register_seeds += 1;
        }
        Ok(())
    }

    fn resolve<E>(
        &self,
        argument: i32,
    ) -> Result<Option<ModelResource<'a>>, ModelExecutionError<E>> {
        if (argument as i16) < 0 {
            return Ok(None);
        }
        let resource = self.executor.resources.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument,
        })?;
        let RenderResourceData::Model(model) = resource.data else {
            return Err(ModelExecutionError::Record(0));
        };
        Ok(Some(ModelResource {
            model,
            source: resource.source,
        }))
    }

    fn publish<H: ModelExecutionHost>(
        &self,
        context: &ModelContext,
        host: &mut H,
    ) -> Result<ModelTransform, ModelExecutionError<H::Error>> {
        let transform = context.publish(self.environment.view, self.environment.coordinate_shift);
        host.publish_transform(context, transform)
            .map_err(ModelExecutionError::Host)?;
        Ok(transform)
    }

    /// Returns outer visibility separately: the child wrapper restores parent
    /// projection selection after entering 004A00E0 even if its far gate rejects.
    fn invoke<H: ModelExecutionHost>(
        &mut self,
        seed: InvocationSeed<'a, '_>,
        host: &mut H,
    ) -> Result<bool, ModelExecutionError<H::Error>> {
        let InvocationSeed {
            resource,
            context,
            values,
            state_words,
            parent_vertices,
            depth,
            publications,
            exact_origin,
        } = seed;
        if depth > self.executor.limits.invocation_depth {
            return Err(ModelExecutionError::Limit);
        }
        self.outcome.invocations += 1;
        let mut transform = self.publish(&context, host)?;
        for _ in 1..publications {
            transform = self.publish(&context, host)?;
        }
        let gated = if self.executor.outside_view {
            transform.at_distance()
        } else if transform.in_frustum(
            resource.model.header.visibility_extent,
            &self.environment.projection,
        ) {
            transform
        } else {
            self.outcome.culled += 1;
            return Ok(false);
        };
        let Some(dispatch) = gated.dispatch(resource.model.header, &self.environment.projection)
        else {
            self.outcome.culled += 1;
            return Ok(true);
        };
        let allocation: ModelScratchAllocation = self
            .scratch
            .allocate(
                &self.scratch_scope,
                resource.model.header.vertex_slot_count,
                resource.model.header.shade_slot_count,
            )
            .map_err(scratch_allocation_error)?;
        let mut frame = Invocation {
            resource,
            context,
            values,
            state_words,
            parent_vertices,
            surface: self.surface,
            dispatch,
            transform,
            vertices: ModelVertices::in_scratch(self.scratch, allocation.clone())
                .map_err(vertex_fault)?,
            shades: ModelShades::in_process_scratch(
                self.scratch,
                allocation.clone(),
                self.environment.lighting.ambient_word,
            )?,
            projector: ModelProjectionBackend {
                transform,
                view: self.environment.view,
                projection: self.environment.projection,
                dispatch,
                coordinate_shift: self.environment.coordinate_shift,
                fog_factor: self.environment.fog_factor,
            },
        };
        host.select_projection(dispatch)
            .map_err(ModelExecutionError::Host)?;
        if self.executor.exact {
            frame
                .vertices
                .keep_exact(exact_origin.unwrap_or(frame.context.position.map(f64::from)));
        }
        frame
            .vertices
            .initialize(resource.model, &frame.context, &mut frame.projector)
            .map_err(lift_initialization)?;
        frame.shades.initialize_normals(
            resource.model,
            resource.source.bytes,
            &frame.context.basis,
            self.environment.lighting,
        )?;
        self.program(&mut frame, &resource.model.program, depth, host)?;
        if depth == 1 {
            self.root_exit = Some(frame.values.register_block());
        }
        self.scratch
            .retire(&self.scratch_scope, allocation.mark())?;
        Ok(true)
    }

    fn program<H: ModelExecutionHost>(
        &mut self,
        frame: &mut Invocation<'a, '_, '_>,
        program: &ModelProgram,
        depth: usize,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        // Existing portable invocation limit also bounds inline program recursion.
        if depth > self.executor.limits.invocation_depth {
            return Err(ModelExecutionError::Limit);
        }
        let mut cursor = 0usize;
        while let Some(instruction) = program.instructions.get(cursor) {
            if self.outcome.instructions >= self.executor.limits.instructions {
                return Err(ModelExecutionError::Limit);
            }
            self.outcome.instructions += 1;
            let opcode = instruction.raw_command as u8;
            match opcode {
                0xf0..=0xff => {
                    frame
                        .values
                        .execute_register(instruction, host)
                        .map_err(ModelExecutionError::Value)?;
                }
                // The view-plane branch; only the retail zone signs use it.
                0x0b => {
                    let [jump, slot, normal] = decoded_arguments::<3, EOf<H>>(instruction)?;
                    if view_plane_jumps(
                        ViewPlaneOperands {
                            resource: frame.resource,
                            basis: &frame.context.basis,
                            vertices: &frame.vertices,
                            environment: &self.environment,
                        },
                        instruction,
                        slot,
                        normal,
                    )? {
                        cursor = branch_target(program, instruction, jump as i16)?;
                        continue;
                    }
                }
                // 004A0C50, all three tables' 13: jump when "origin Z
                // (0055F200) >= the signed high:low literal" differs from the
                // command's SAR 8, else the next instruction.
                0x13 => {
                    let [jump, low, high] = decoded_arguments::<3, EOf<H>>(instruction)?;
                    let threshold =
                        (i32::from(high as i16).wrapping_shl(16)).wrapping_add(i32::from(low));
                    let comparison = i32::from(frame.transform.view_position[2] >= threshold);
                    if comparison ^ (i32::from(instruction.raw_command as i16) >> 8) != 0 {
                        cursor = branch_target(program, instruction, jump as i16)?;
                        continue;
                    }
                }
                0x16 => {
                    if let ModelValueBranch::Jump {
                        words_from_arguments,
                    } = frame
                        .values
                        .branch_decision(instruction, host)
                        .map_err(ModelExecutionError::Value)?
                    {
                        cursor = branch_target(program, instruction, words_from_arguments)?;
                        continue;
                    }
                }
                0x03 => {
                    let [orientation, axis, angle] = arguments::<3, EOf<H>>(instruction)?;
                    frame.context.save_oriented_basis(orientation);
                    if angle != 0 {
                        let angle = frame
                            .values
                            .evaluate_packed(angle, host)
                            .map_err(ModelExecutionError::Value)?;
                        frame.context.rotate_saved_basis(
                            axis as i16,
                            angle,
                            &self.executor.basis_math,
                        );
                    }
                    if hovercraft_fans_unmirrored(
                        frame.resource.model.identity.global_index,
                        orientation,
                        self.executor.hovercraft_fans,
                    ) {
                        frame.context.unmirror_saved_basis();
                    }
                }
                0x1c => {
                    // 004A0DA0 rotates the saved basis with no oriented
                    // copy and no raw-zero skip: the raw first word
                    // selects the axis (signed 1/2, else fallback) and
                    // the packed second word always resolves first
                    // (evaluating callbacks when selected).
                    let [axis, angle] = arguments::<2, EOf<H>>(instruction)?;
                    let angle = frame
                        .values
                        .evaluate_packed(angle, host)
                        .map_err(ModelExecutionError::Value)?;
                    frame
                        .context
                        .rotate_saved_basis(axis as i16, angle, &self.executor.basis_math);
                }
                0x0f => {
                    arguments::<0, EOf<H>>(instruction)?;
                    frame.context.save_identity_basis();
                }
                0x10 => {
                    arguments::<0, EOf<H>>(instruction)?;
                    frame.context.restore_saved_basis();
                    frame.transform = self.publish(&frame.context, host)?;
                    frame.projector.transform = frame.transform;
                }
                0x23 | 0x26 => frame
                    .vertices
                    .execute(instruction, &mut frame.values, host, &mut frame.projector)
                    .map_err(ModelExecutionError::Vertex)?,
                0x21 | 0x24 | 0x27..=0x29 => frame
                    .vertices
                    .execute_shaped(
                        instruction,
                        frame.context.position,
                        &mut frame.values,
                        host,
                        &mut frame.projector,
                    )
                    .map_err(ModelExecutionError::Vertex)?,
                // 004EC810 subtracts the origin it reads back from the last
                // publication; the context position is that published value.
                0x25 => frame
                    .vertices
                    .execute_origin_offset(
                        instruction,
                        frame.context.position,
                        &mut frame.projector,
                    )
                    .map_err(lift_ground_2b)?,
                // Both owners call the actual renderer+50 binding. Their
                // mode masks and successful projection return differ.
                0x2b => frame
                    .vertices
                    .execute_ground_2b(
                        crate::model_vertices::ModelSurfaceVertexRequest::from_renderer(
                            instruction,
                            crate::model_vertices::ModelSurfaceCallback::Ground,
                        )
                        .map_err(vertex_fault)?,
                        &mut ModelSurfaceCallbackInputs {
                            binding: frame.surface,
                            table: host.sine_quarter_table(),
                        },
                        &mut frame.projector,
                    )
                    .map_err(lift_ground_2b)?,
                0x2c => frame
                    .vertices
                    .execute_project_2c(
                        crate::model_vertices::ModelSurfaceVertexRequest::from_renderer(
                            instruction,
                            crate::model_vertices::ModelSurfaceCallback::Project,
                        )
                        .map_err(vertex_fault)?,
                        &mut ModelSurfaceCallbackInputs {
                            binding: frame.surface,
                            table: host.sine_quarter_table(),
                        },
                        &mut frame.projector,
                    )
                    .map_err(lift_ground_2b)?,
                0x2a => {
                    // 004ED350 advances past the two argument words in
                    // every branch (all epilogues `lea eax,[ecx+0x4]`).
                    // A clear mode word is an exact no-op. Set bits
                    // copy parent slots through the invoker's word
                    // table (see `execute_copy_2a`); a root takes the
                    // published origin and its projection instead.
                    frame
                        .vertices
                        .execute_copy_2a(
                            instruction,
                            frame.state_words,
                            frame.parent_vertices.as_ref(),
                            self.executor.mirror_copy,
                            frame.context.position,
                            &mut frame.projector,
                        )
                        .map_err(lift_ground_2b)?;
                }
                0x05 => self.common_block_05(frame, instruction, depth, host)?,
                0x0e => self.child(frame, instruction, depth, host)?,
                0x22 => {
                    // 004EC300 advances past the three argument words in
                    // every branch (`return param_2 + 3`). A clear mode
                    // nibble is an exact no-op. Set bits jitter a source
                    // slot through the process RNG word (see
                    // `execute_jitter_22`); without it the set-bits path
                    // stays an explicitly open owner (fail closed per
                    // feature).
                    frame
                        .vertices
                        .execute_jitter_22(instruction, host.process_rng(), &mut frame.projector)
                        .map_err(lift_ground_2b)?;
                }
                0x01 | 0x07 | 0xe0 | 0xe2 | 0xe4 | 0xe6 | 0x1a | 0x1b | 0xd0 | 0xd2 | 0xd4
                | 0xd6 | 0xd8 | 0xda | 0xdc | 0xde => {
                    // Both dispatch tables have distinct primitive handlers;
                    // the selected mode and original full command stay intact.
                    let request = ModelPrimitiveRequest {
                        model: frame.resource.model,
                        source: frame.resource.source,
                        instruction,
                        context: &frame.context,
                        transform: frame.transform,
                        dispatch: frame.dispatch,
                        vertices: &frame.vertices,
                        shades: &frame.shades,
                        state_words: frame.state_words,
                    };
                    host.primitive(request, &mut frame.values)
                        .map_err(ModelExecutionError::Host)?;
                    if frame.values.stopped() {
                        return Err(ModelExecutionError::Value(ModelValueError::Fault(
                            ModelValueFault::Stopped,
                        )));
                    }
                    self.outcome.primitive_calls += 1;
                }
                // 004A08B0: the slot's projected depth plus the signed
                // high:low displacement opens an unsorted group.
                0xee => {
                    let [slot, low, high] = arguments::<3, EOf<H>>(instruction)?;
                    let vertex = frame.vertices.slot(slot as i16).map_err(vertex_fault)?;
                    let depth = match vertex.projection.read_u32(4) {
                        Some(z) => z
                            .wrapping_add(u32::from(low))
                            .wrapping_add((i32::from(high as i16) as u32) << 16)
                            as i32,
                        None => residue_group_depth(&frame.vertices, slot, instruction)?,
                    };
                    host.begin_depth_group(depth, false)
                        .map_err(ModelExecutionError::Host)?;
                    self.outcome.depth_groups += 1;
                }
                // 0xEA/0x15 (004A0810/004A08F0): begin a depth group on a
                // vertex slot depth, unsorted/sorted. Same shape as 0xEE
                // with no low/high bias words.
                0xea | 0x15 => {
                    let [slot] = arguments::<1, EOf<H>>(instruction)?;
                    let vertex = frame.vertices.slot(slot as i16).map_err(vertex_fault)?;
                    let depth = match vertex.projection.read_u32(4) {
                        Some(depth) => depth as i32,
                        None => residue_group_depth(&frame.vertices, slot, instruction)?,
                    };
                    host.begin_depth_group(depth, opcode == 0x15)
                        .map_err(ModelExecutionError::Host)?;
                    self.outcome.depth_groups += 1;
                }
                // 0xE8/0xE9 (004A0770/004A07C0): an unsorted group at the
                // farthest (E8) or nearest (E9) slot depth of a 0xFFFF-ended
                // list, from -0x7FFFFFFF/0x7FFFFFFF. The residue stand-in
                // above is EE's evidence only, so an unknown depth fails.
                0xe8 | 0xe9 => {
                    let ModelInstructionKind::SentinelList(list) = &instruction.kind else {
                        return Err(ModelExecutionError::Record(instruction.span.start));
                    };
                    let mut depth = if opcode == 0xe8 {
                        -0x7fff_ffff
                    } else {
                        0x7fff_ffff
                    };
                    for &slot in &list.values {
                        let vertex = frame.vertices.slot(slot as i16).map_err(vertex_fault)?;
                        let value = vertex
                            .projection
                            .read_u32(4)
                            .ok_or(ModelExecutionError::Record(instruction.span.start))?
                            as i32;
                        depth = if opcode == 0xe8 {
                            depth.max(value)
                        } else {
                            depth.min(value)
                        };
                    }
                    host.begin_depth_group(depth, false)
                        .map_err(ModelExecutionError::Host)?;
                    self.outcome.depth_groups += 1;
                }
                // 004A0840/004A0860 open unsorted groups without reading
                // a vertex. EB uses published origin Z (0055F200); EC uses
                // the literal signed depth -1. Both consume zero words.
                0xeb | 0xec => {
                    arguments::<0, EOf<H>>(instruction)?;
                    let depth = if opcode == 0xeb {
                        frame.transform.view_position[2]
                    } else {
                        -1
                    };
                    host.begin_depth_group(depth, false)
                        .map_err(ModelExecutionError::Host)?;
                    self.outcome.depth_groups += 1;
                }
                // 004A0920 closes the group (004A44A0).
                0xef => {
                    arguments::<0, EOf<H>>(instruction)?;
                    host.end_depth_group().map_err(ModelExecutionError::Host)?;
                }
                // 004A0880: an unsorted group at the immediate signed
                // high:low depth.
                0xed => {
                    let [low, high] = arguments::<2, EOf<H>>(instruction)?;
                    let depth = (i32::from(high as i16) << 16).wrapping_add(i32::from(low));
                    host.begin_depth_group(depth, false)
                        .map_err(ModelExecutionError::Host)?;
                    self.outcome.depth_groups += 1;
                }
                // 004A3880 only steps past its 0xFFFF-ended list.
                0x06 => {}
                // 004A0250, as the late table's 08: the full signed command
                // SAR 8, in WORDs from the first argument.
                0x08 => {
                    let words = (i32::from(instruction.raw_command as i16) >> 8) as i16;
                    cursor = branch_target(program, instruction, words)?;
                    continue;
                }
                // Opcode2D (004EC670) delegates its compute callback by the
                // submitted object's explicit policy. Shared frontend callbacks
                // run 0043A2D0; RequiredObject reaches source+10. Only the
                // explicit Unowned policy retains the labeled approximation
                // skip-advance-2d-nibble2 until its simulation state is owned.
                // That skip leaves the computed slot unprojected; later owned
                // readers may fail, and this does not establish retail pixels.
                // Native frame1 DA submissions and the remaining world compute
                // inputs are documented in GAMEPLAY_RUNTIME.md (0x2D owner).
                0x2d => {
                    let policy = host.custom_vertex_policy();
                    if policy != crate::model_vertices::ModelCustomVertexPolicy::Unowned {
                        frame
                            .vertices
                            .execute_custom_2d(
                                instruction,
                                policy,
                                &mut frame.projector,
                                &mut |request, vertices, output| {
                                    if policy == crate::model_vertices::ModelCustomVertexPolicy::RequiredObject {
                                        host.required_custom_vertex(request, vertices, output)
                                    } else {
                                        host.furniture_custom_vertex(request, vertices, output)
                                    }
                                },
                            )
                            .map_err(ModelExecutionError::Vertex)?;
                        cursor += 1;
                        continue;
                    }
                    let ModelInstructionKind::UnresolvedDynamic(_) = &instruction.kind else {
                        return Err(ModelExecutionError::Record(instruction.span.start));
                    };
                    match (instruction.raw_command >> 8) & 15 {
                        0 | 2 => {}
                        _ => {
                            return Err(ModelExecutionError::Unsupported {
                                opcode,
                                offset: instruction.span.start,
                            });
                        }
                    }
                }
                _ => {
                    return Err(ModelExecutionError::Unsupported {
                        opcode,
                        offset: instruction.span.start,
                    })
                }
            }
            cursor += 1;
        }
        Ok(())
    }

    fn child<H: ModelExecutionHost>(
        &mut self,
        frame: &mut Invocation<'a, '_, '_>,
        instruction: &ModelInstruction,
        depth: usize,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        let ModelInstructionKind::ModelInstance(instance) = &instruction.kind else {
            return Err(ModelExecutionError::Record(instruction.span.start));
        };
        // Resource lookup precedes transform/origin access at 004A0E70.
        let resource = self
            .resolve(i32::from(instance.model.raw()))?
            .ok_or(ModelExecutionError::InvalidChild(instance.model.raw()))?;
        let &slot = instance
            .state_words
            .first()
            .ok_or(ModelExecutionError::Record(instruction.span.start))?;
        let position = match frame.vertices.world(slot as i16) {
            Ok(position) => position,
            // CustomVertexStackResidueApproximation: native anchors the child
            // at 0043A2D0's stack residue. The port skips that child.
            Err(ModelVertexFault::UnknownWorld { .. })
                if frame.vertices.residue_world(slot as i16) =>
            {
                return Ok(());
            }
            Err(fault) => return Err(vertex_fault(fault)),
        };
        let context = frame.context.child(position, instance.orientation.raw());
        let mut values = frame
            .values
            .child_invocation()
            .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
        self.seed_child_registers(&resource, &mut values)?;
        if self.invoke(
            InvocationSeed {
                resource,
                context,
                values,
                state_words: &instance.state_words,
                parent_vertices: Some(frame.vertices.clone()),
                depth: depth + 1,
                publications: 2,
                exact_origin: frame.vertices.exact_world(slot as i16),
            },
            host,
        )? {
            host.select_projection(frame.dispatch)
                .map_err(ModelExecutionError::Host)?;
        }
        self.publish(&frame.context, host)?;
        Ok(())
    }
}

type EOf<H> = <H as ModelValueCallback>::Error;

struct Invocation<'a, 's, 't> {
    resource: ModelResource<'a>,
    context: ModelContext,
    values: ModelValues,
    state_words: &'s [u16],
    parent_vertices: Option<ModelVertices>,
    surface: ModelSurfaceBinding<'t>,
    vertices: ModelVertices,
    shades: ModelShades,
    dispatch: ModelDispatch,
    transform: ModelTransform,
    projector: ModelProjectionBackend,
}

fn arguments<const N: usize, E>(
    instruction: &ModelInstruction,
) -> Result<[u16; N], ModelExecutionError<E>> {
    let ModelInstructionKind::Opaque(command) = &instruction.kind else {
        return Err(ModelExecutionError::Record(instruction.span.start));
    };
    command
        .arguments
        .as_slice()
        .try_into()
        .map_err(|_| ModelExecutionError::Record(instruction.span.start))
}

/// Argument words of an opaque or unresolved dynamic instruction.
fn decoded_arguments<const N: usize, E>(
    instruction: &ModelInstruction,
) -> Result<[u16; N], ModelExecutionError<E>> {
    let arguments = match &instruction.kind {
        ModelInstructionKind::Opaque(command) => &command.arguments,
        ModelInstructionKind::UnresolvedDynamic(command) => &command.arguments,
        _ => return Err(ModelExecutionError::Record(instruction.span.start)),
    };
    arguments
        .as_slice()
        .try_into()
        .map_err(|_| ModelExecutionError::Record(instruction.span.start))
}

/// The invocation state 004A0CE0 reads: the raw normal pool (context+68),
/// the current basis at 0055F1D0, vertex slots (context+00) and, through
/// context+58, renderer+04/+08/+0C, the view eye.
struct ViewPlaneOperands<'r, 'f> {
    resource: ModelResource<'r>,
    basis: &'f crate::menu::MenuBasis,
    vertices: &'f ModelVertices,
    environment: &'f ModelEnvironment,
}

/// 004A0CE0: whether the view-plane branch jumps. The vector at the signed
/// `index * 6` from the raw normal pool (no lighting /2-6 correction) is
/// shifted by the coordinate shift and turned by 00490900 without
/// translation; its dot with the slot's world point minus the eye has a
/// sign, and the instruction's signed high byte names the sign that falls
/// through. A vector outside this model's source bytes stops the model.
fn view_plane_jumps<E>(
    operands: ViewPlaneOperands<'_, '_>,
    instruction: &ModelInstruction,
    slot: u16,
    index: u16,
) -> Result<bool, ModelExecutionError<E>> {
    let world = operands.vertices.world(slot as i16).map_err(vertex_fault)?;
    let at = operands
        .resource
        .model
        .normals_span
        .start
        .checked_add_signed(isize::from(index as i16) * 6);
    let bytes = at
        .and_then(|at| operands.resource.source.bytes.get(at..at.checked_add(6)?))
        .ok_or(ModelExecutionError::Record(instruction.span.start))?;
    let raw =
        std::array::from_fn(|axis| i16::from_le_bytes([bytes[axis * 2], bytes[axis * 2 + 1]]));
    let (_, _, jumps) = view_plane_decision(
        world,
        operands.environment.view.eye,
        raw,
        operands.basis,
        operands.environment.coordinate_shift,
        instruction.raw_command,
    );
    Ok(jumps)
}

/// The arithmetic of 004A0CE0 and 00490900: the turned vector, the wrapped
/// dot product (Z, then Y, then X) and whether the branch jumps.
fn view_plane_decision(
    world: [i32; 3],
    eye: [i32; 3],
    raw: [i16; 3],
    basis: &crate::menu::MenuBasis,
    coordinate_shift: u8,
    raw_command: u16,
) -> ([i32; 3], i32, bool) {
    let shift = u32::from(coordinate_shift & 31);
    let [x, y, z] = raw.map(|value| f64::from(i32::from(value).wrapping_shl(shift) >> shift));
    let turned: [i32; 3] = std::array::from_fn(|axis| {
        let coefficient = |first: usize| f64::from(f32::from_bits(basis.words[first + axis]));
        crate::model_context::fistp_low32(
            (z * coefficient(7) + y * coefficient(4)) + x * coefficient(1),
        )
    });
    let products: [i32; 3] = std::array::from_fn(|axis| {
        world[axis]
            .wrapping_sub(eye[axis])
            .wrapping_mul(turned[axis])
    });
    let dot = products[2]
        .wrapping_add(products[1])
        .wrapping_add(products[0]);
    let high = i32::from(raw_command as i16) >> 8;
    (turned, dot, i32::from(dot < 0) != high)
}

fn branch_target<E>(
    program: &ModelProgram,
    instruction: &ModelInstruction,
    words: i16,
) -> Result<usize, ModelExecutionError<E>> {
    let error = || ModelExecutionError::Branch {
        offset: instruction.span.start,
    };
    let target = instruction
        .span
        .start
        .checked_add(2)
        .and_then(|v| v.checked_add_signed(isize::from(words) * 2))
        .ok_or_else(error)?;
    if target == program.terminator.start {
        return Ok(program.instructions.len());
    }
    program
        .instructions
        .binary_search_by_key(&target, |record| record.span.start)
        .map_err(|_| error())
}

/// Preserve the existing allocation-capacity boundary; custody/state faults
/// remain distinct hard scratch errors. No other kernel error is softened.
fn scratch_allocation_error<E>(error: ModelScratchError) -> ModelExecutionError<E> {
    match error {
        ModelScratchError::Capacity { end } => {
            ModelExecutionError::Lighting(ModelLightingError::ScratchAllocation { end })
        }
        error => ModelExecutionError::Scratch(error),
    }
}

fn vertex_fault<E>(fault: ModelVertexFault) -> ModelExecutionError<E> {
    ModelExecutionError::Vertex(ModelVertexError::Fault(fault))
}

/// CustomVertexStackResidueApproximation for EE/EA/15 keyed on a vertex whose
/// projection came from 0043A2D0's stack residue. Infest01's Level 1 gate
/// preview keyed its EE group on pointer residue every frame (depth 22140),
/// behind all preview geometry. The farthest signed key keeps that order.
/// Any other unknown depth remains a record failure.
fn residue_group_depth<E>(
    vertices: &ModelVertices,
    slot: u16,
    instruction: &ModelInstruction,
) -> Result<i32, ModelExecutionError<E>> {
    if vertices.residue_projection(slot as i16) {
        Ok(i32::MAX)
    } else {
        Err(ModelExecutionError::Record(instruction.span.start))
    }
}
fn lift_ground_2b<E>(error: ModelVertexError<Infallible, Infallible>) -> ModelExecutionError<E> {
    lift_initialization(error)
}
fn lift_initialization<E>(error: ModelVertexError) -> ModelExecutionError<E> {
    match error {
        ModelVertexError::Fault(fault) => vertex_fault(fault),
        ModelVertexError::Value(ModelValueError::Fault(fault)) => {
            ModelExecutionError::Value(ModelValueError::Fault(fault))
        }
        ModelVertexError::Value(ModelValueError::Callback(value))
        | ModelVertexError::Projection(value) => match value {},
        ModelVertexError::Custom(ModelCustomVertexError::Fault(fault)) => vertex_fault(fault),
        ModelVertexError::Custom(ModelCustomVertexError::Host(value)) => match value {},
    }
}

#[cfg(test)]
#[path = "model_execution_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "model_execution_corpus_tests.rs"]
mod corpus_tests;

#[cfg(test)]
#[path = "model_execution/view_plane_tests.rs"]
mod view_plane_tests;

mod common;
mod late;
