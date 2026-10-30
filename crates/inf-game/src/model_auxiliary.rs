//! Reached 004F2BE0/004F2C50 pair execution, separate from renderer grammar.
//! Owned operations are child05, vertices07/18/2F, saved-basis30, callback8E+00,
//! the groups 89..8D, branch95 and88. Terrain table 00537090 also owns 8F/90's
//! constant slots, and the ray table 00544C20 its sphere, box and empty slots
//! (8E/8F/90).
//! No model IDs admit an operation. Other authored operations stop at their
//! exact reached callback, retaining process/scratch/contact publication.

use crate::{
    frontend_scene::ModelSurfaceTableOrigin,
    menu::MenuBasis,
    model_context::ModelContext,
    model_projection::ProcessModelProjector,
    model_scratch::{
        ModelScratchDrawExit, ModelScratchDrawScope, ModelScratchError, ProcessModelScratch,
    },
    model_vertices::{ModelVertexCommand, ModelVertexError, ModelVertexFault, ModelVertices},
    object_runtime::{
        apply_contact_geometry, terrain_sphere_contact, CollisionContact, ContactGeometryRequest,
        ContactSphere, TerrainSphereError, TerrainSphereGrid,
    },
    process_model_graphics::{
        ProcessModelGraphics, ProcessModelGraphicsError, PublishedModelWorld,
    },
    world_furniture_effect::ProcessFurnitureQueryState,
};
use inf_formats::{ModelAuxiliaryInstructionKind as Op, ModelEntry, TerrainCell};
use thiserror::Error;

mod child;
mod group;
pub(crate) mod ray;
mod rotation;

/// The actual H8 loan stays captured through recursion. Qualification is copied
/// only into retained B+64; it does not keep a released resource alive.
#[derive(Clone, Copy)]
pub(crate) struct AuxiliaryModelSource<'a> {
    pub model: &'a ModelEntry,
    pub qualification: &'a crate::world_furniture_effect::AuxiliaryRawSource,
}

/// Actual request+1C table identity. This is not the renderer surface table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallbackTable {
    Composite544c68,
    Point544bf0,
    Sphere544c08,
    /// Installed by 0044EC50 for the world contact pass 004791F0.
    Terrain537090,
    /// Installed by 0047D080 and 0043A7E0 for the sight test 0043AD90.
    Ray544c20,
    /// Installed by the composite's box wrapper 004F2690 for the
    /// counterpart's program, with the 8F half extents it copies to request
    /// +28/+2C/+30 (its +20 stays unwritten, and no box callback reads it).
    Box544c50 {
        half_extents: [i32; 3],
    },
}

/// Prepared H12 cells and the live 005372F0/F4/F8 globals read by 0044EC90.
#[derive(Clone, Copy)]
pub(crate) struct AuxiliaryTerrain<'a> {
    pub cells: &'a [TerrainCell],
    pub grid: TerrainSphereGrid,
}

/// 0044EC50's request as 004791F0 builds it: request +08 is the actor's H8
/// model, +10/+14/+18 its sign-extended position WORDs, and the context basis
/// its virtual +48 basis. The 00527B18 wrapper captures the actor for values.
pub(crate) struct AuxiliaryTerrainRequest<'a> {
    pub model: AuxiliaryModelSource<'a>,
    pub basis: MenuBasis,
    pub origin: [i32; 3],
    pub terrain: AuxiliaryTerrain<'a>,
    pub values: AuxiliaryValueSource<'a>,
    pub residency: Option<&'a crate::ResidentRuntime>,
}

pub(crate) struct AuxiliaryPairRequest<'a> {
    /// Stack A, prepared once by47C1D0, relative to the current terrain cell.
    pub actor_origin: [i32; 3],
    pub actor_basis: MenuBasis,
    pub actor_model: AuxiliaryModelSource<'a>,
    pub counterpart: AuxiliaryModelSource<'a>,
    pub residency: Option<&'a crate::ResidentRuntime>,
}

///The captured native packet source, independent of the current H8 child.
///Absent custody is a required boundary only if a packed callback is reached.
#[derive(Clone, Copy)]
pub(crate) enum AuxiliaryValueSource<'a> {
    Required,
    Object(crate::object_runtime::ObjectModelCallbacks<'a>),
    /// The furniture wrapper 005220B8, whose getter 00437AE0 reads zero.
    Zero,
}

impl AuxiliaryValueSource<'_> {
    fn evaluate(self, raw: u16, address: usize) -> Result<u16, AuxiliaryModelError> {
        if let Some(value) = crate::model_values::packed_immediate(raw) {
            return Ok(value);
        }
        if raw & 0x40 == 0 {
            match self {
                Self::Object(source) => {
                    return source
                        .packed_value(u32::from(raw & 0xff3f))
                        .map_err(Into::into);
                }
                Self::Zero => return Ok(0),
                Self::Required => {}
            }
        }
        Err(AuxiliaryModelError::RequiredPackedValue { raw, address })
    }
}

/// Direct 004F2C40 callers select a point or sphere policy explicitly. Pair
/// recursion still uses its separate outer544C68 table and two model owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuxiliaryQueryShape {
    Point,
    Sphere { radius: i32 },
}

pub(crate) struct AuxiliaryDirectRequest<'a> {
    pub model: AuxiliaryModelSource<'a>,
    pub basis: MenuBasis,
    /// World-WORD difference, sign-extended by the actual actor/furniture caller.
    pub relative_center: [i32; 3],
    pub shape: AuxiliaryQueryShape,
    pub values: AuxiliaryValueSource<'a>,
    pub residency: Option<&'a crate::ResidentRuntime>,
}

#[derive(Debug, Error)]
pub enum AuxiliaryModelError {
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error(transparent)]
    Graphics(#[from] ProcessModelGraphicsError),
    #[error(transparent)]
    Vertex(#[from] ModelVertexFault),
    #[error("auxiliary vertex producer failed: {0}")]
    VertexExecution(ModelVertexError<std::convert::Infallible, ProcessModelGraphicsError>),
    #[error("active model at {model:#x} requires decoded auxiliary program")]
    RequiredProgram { model: usize },
    #[error("required auxiliary opcode {opcode:02X} at {address:#x}")]
    RequiredOperation { opcode: u8, address: usize },
    #[error("auxiliary packed value {raw:04X} at {address:#x} requires the original register/callback context")]
    RequiredPackedValue { raw: u16, address: usize },
    #[error(transparent)]
    ObjectValue(#[from] crate::object_runtime::ObjectModelCallbackError),
    #[error("auxiliary child model {slot} requires current H8 residency")]
    RequiredChildResources { slot: u16 },
    #[error("auxiliary child model {slot} resolved to native NULL")]
    RequiredChildModel { slot: u16 },
    #[error(transparent)]
    ChildResource(#[from] crate::object_runtime::ObjectError),
    #[error("required auxiliary callback {table:08X}+{offset:02X} at {address:#x}")]
    RequiredCallback {
        table: u32,
        offset: u8,
        address: usize,
    },
    #[error("auxiliary operation {opcode:02X} has incompatible decoded data at {address:#x}")]
    InvalidOperation { opcode: u8, address: usize },
    #[error("auxiliary branch target {address:#x} is not an instruction or terminator")]
    BranchTarget { address: usize },
    #[error("reached backward auxiliary95 at {address:#x} to {target:#x} requires an owned termination/progress contract")]
    RequiredBackwardBranch { address: usize, target: usize },
    #[error("auxiliary contact DWORD+{offset:02X} is still unwritten")]
    RequiredContact { offset: u8 },
    #[error(transparent)]
    TerrainSphere(#[from] TerrainSphereError),
}

impl AuxiliaryModelError {
    pub(crate) fn is_required_feature_boundary(&self) -> bool {
        matches!(
            self,
            Self::RequiredProgram { .. }
                | Self::RequiredOperation { .. }
                | Self::RequiredPackedValue { .. }
                | Self::ObjectValue(_)
                | Self::RequiredChildResources { .. }
                | Self::RequiredChildModel { .. }
                | Self::RequiredCallback { .. }
                | Self::RequiredContact { .. }
                | Self::RequiredBackwardBranch { .. }
                | Self::TerrainSphere(_)
                | Self::Graphics(
                    ProcessModelGraphicsError::RequiredRendererTable
                        | ProcessModelGraphicsError::NativeNullProjection { .. }
                )
                | Self::VertexExecution(ModelVertexError::Projection(
                    ProcessModelGraphicsError::RequiredRendererTable
                        | ProcessModelGraphicsError::NativeNullProjection { .. }
                ))
        )
    }
}

struct StackContext {
    origin: [i32; 3],
    basis: MenuBasis,
    table: ModelSurfaceTableOrigin,
    saved_basis: MenuBasis,
}
enum Working<'a> {
    Stack(&'a mut StackContext),
    ProcessB,
}
struct Run<'a> {
    graphics: &'a ProcessModelGraphics,
    scratch: &'a ProcessModelScratch,
    scope: ModelScratchDrawScope,
    process: &'a mut ProcessFurnitureQueryState,
    basis_math: &'a crate::menu_basis::MenuBasisMath,
    residency: Option<&'a crate::ResidentRuntime>,
    /// Present only for terrain table 00537090 queries.
    terrain: Option<AuxiliaryTerrain<'a>>,
    /// 0047B610's B context on its own stack. Without one, the composite
    /// table recurses into the process B context of the furniture query.
    counterpart: Option<CounterpartStack<'a>>,
    /// Present only for ray table 00544C20 queries: request +34 and +38.
    ray: Option<ray::RayRequest>,
}

/// The counterpart's stack context and its own 00527B18 value wrapper.
struct CounterpartStack<'a> {
    context: StackContext,
    values: AuxiliaryValueSource<'a>,
}

/// 0047B610's 004F2BE0 request: A's context origin is the WORD delta A - B,
/// B's origin zero; each context holds its actor's virtual +48 basis.
pub(crate) struct AuxiliaryActorPairRequest<'a> {
    pub actor_origin: [i32; 3],
    pub actor_basis: MenuBasis,
    pub actor_model: AuxiliaryModelSource<'a>,
    pub actor_values: AuxiliaryValueSource<'a>,
    pub counterpart_basis: MenuBasis,
    pub counterpart_model: AuxiliaryModelSource<'a>,
    pub counterpart_values: AuxiliaryValueSource<'a>,
    pub residency: Option<&'a crate::ResidentRuntime>,
}

/// One draw scope, shared by both native recursive invocations. Error leaves
/// stopped registered marks; containment is a separate caller policy. No Drop
/// retirement or automatic byte reset is used.
pub(crate) fn query_auxiliary_pair(
    request: AuxiliaryPairRequest<'_>,
    process: &mut ProcessFurnitureQueryState,
    owners: &crate::model_process::ModelProcessOwners,
    output: &mut CollisionContact,
) -> Result<(), AuxiliaryModelError> {
    //4F2BE0 captures B-A before4F2C50 clears either working origin.
    let relative =
        std::array::from_fn(|i| process.pair_origin()[i].wrapping_sub(request.actor_origin[i]));
    let scope = owners.scratch.begin_draw()?;
    let mut run = Run {
        graphics: &owners.graphics,
        scratch: &owners.scratch,
        scope: scope.clone(),
        process,
        basis_math: &owners.basis_math,
        residency: request.residency,
        terrain: None,
        counterpart: None,
        ray: None,
    };
    let mut stack = StackContext {
        origin: request.actor_origin,
        basis: request.actor_basis,
        table: ModelSurfaceTableOrigin::TemplateDefault,
        saved_basis: MenuBasis::INITIAL,
    };
    let result = run.execute(
        Working::Stack(&mut stack),
        request.actor_model,
        request.counterpart,
        relative,
        0,
        CallbackTable::Composite544c68,
        AuxiliaryValueSource::Required,
        output,
    );
    let exit = if result.is_ok() {
        ModelScratchDrawExit::NormalReturn
    } else {
        ModelScratchDrawExit::StoppedOnError
    };
    //A genuine finish failure remains hard, including when it supersedes a
    //producer error; reached bytes/contact/process state remain unchanged.
    owners.scratch.finish_draw(&scope, exit)?;
    result
}

///004F2C40 ->004F2C50 over one captured model. Like the pair caller, this
/// publishes into the retained graphics/scratch owners; it does not initialize
/// renderer vertices or restore reached mutations after a stopped callback.
pub(crate) fn query_auxiliary_direct(
    request: AuxiliaryDirectRequest<'_>,
    process: &mut ProcessFurnitureQueryState,
    owners: &crate::model_process::ModelProcessOwners,
    output: &mut CollisionContact,
) -> Result<(), AuxiliaryModelError> {
    let (callbacks, radius) = match request.shape {
        AuxiliaryQueryShape::Point => (CallbackTable::Point544bf0, 0),
        AuxiliaryQueryShape::Sphere { radius } => (CallbackTable::Sphere544c08, radius),
    };
    let scope = owners.scratch.begin_draw()?;
    let mut run = Run {
        graphics: &owners.graphics,
        scratch: &owners.scratch,
        scope: scope.clone(),
        process,
        basis_math: &owners.basis_math,
        residency: request.residency,
        terrain: None,
        counterpart: None,
        ray: None,
    };
    let mut stack = StackContext {
        origin: [0; 3],
        basis: request.basis,
        table: ModelSurfaceTableOrigin::TemplateDefault,
        saved_basis: MenuBasis::INITIAL,
    };
    let result = run.execute(
        Working::Stack(&mut stack),
        request.model,
        request.model,
        request.relative_center,
        radius,
        callbacks,
        request.values,
        output,
    );
    let exit = if result.is_ok() {
        ModelScratchDrawExit::NormalReturn
    } else {
        ModelScratchDrawExit::StoppedOnError
    };
    owners.scratch.finish_draw(&scope, exit)?;
    result
}

/// 0047B610's 004F2BE0 call: relative is B's zero origin minus A's, and the
/// composite table recurses into B's stack context with B's values.
pub(crate) fn query_auxiliary_actor_pair(
    request: AuxiliaryActorPairRequest<'_>,
    process: &mut ProcessFurnitureQueryState,
    owners: &crate::model_process::ModelProcessOwners,
    output: &mut CollisionContact,
) -> Result<(), AuxiliaryModelError> {
    let relative = request.actor_origin.map(|origin| 0i32.wrapping_sub(origin));
    let scope = owners.scratch.begin_draw()?;
    let mut run = Run {
        graphics: &owners.graphics,
        scratch: &owners.scratch,
        scope: scope.clone(),
        process,
        basis_math: &owners.basis_math,
        residency: request.residency,
        terrain: None,
        counterpart: Some(CounterpartStack {
            context: StackContext {
                origin: [0; 3],
                basis: request.counterpart_basis,
                table: ModelSurfaceTableOrigin::TemplateDefault,
                saved_basis: MenuBasis::INITIAL,
            },
            values: request.counterpart_values,
        }),
        ray: None,
    };
    let mut stack = StackContext {
        origin: request.actor_origin,
        basis: request.actor_basis,
        table: ModelSurfaceTableOrigin::TemplateDefault,
        saved_basis: MenuBasis::INITIAL,
    };
    let result = run.execute(
        Working::Stack(&mut stack),
        request.actor_model,
        request.counterpart_model,
        relative,
        0,
        CallbackTable::Composite544c68,
        request.actor_values,
        output,
    );
    let exit = if result.is_ok() {
        ModelScratchDrawExit::NormalReturn
    } else {
        ModelScratchDrawExit::StoppedOnError
    };
    owners.scratch.finish_draw(&scope, exit)?;
    result
}

/// 0044EC50: install terrain table 00537090 at request +1C, then run the same
/// 004F2C40 -> 004F2C50 direct execution into the context's +1E8 contact.
/// 004791F0 never writes request +20, and no slot of this table reads it.
pub(crate) fn query_auxiliary_terrain(
    request: AuxiliaryTerrainRequest<'_>,
    process: &mut ProcessFurnitureQueryState,
    owners: &crate::model_process::ModelProcessOwners,
    output: &mut CollisionContact,
) -> Result<(), AuxiliaryModelError> {
    let scope = owners.scratch.begin_draw()?;
    let mut run = Run {
        graphics: &owners.graphics,
        scratch: &owners.scratch,
        scope: scope.clone(),
        process,
        basis_math: &owners.basis_math,
        residency: request.residency,
        terrain: Some(request.terrain),
        counterpart: None,
        ray: None,
    };
    let mut stack = StackContext {
        origin: [0; 3],
        basis: request.basis,
        table: ModelSurfaceTableOrigin::TemplateDefault,
        saved_basis: MenuBasis::INITIAL,
    };
    let result = run.execute(
        Working::Stack(&mut stack),
        request.model,
        request.model,
        request.origin,
        0,
        CallbackTable::Terrain537090,
        request.values,
        output,
    );
    let exit = if result.is_ok() {
        ModelScratchDrawExit::NormalReturn
    } else {
        ModelScratchDrawExit::StoppedOnError
    };
    owners.scratch.finish_draw(&scope, exit)?;
    result
}

/// 0047D080's and 0043A7E0's request: the model's origin in the ray's
/// frame (+10..+18), its basis there (00490DC0), the ray's basis and length
/// (+34, +38) and the model's values.
pub(crate) struct AuxiliaryRayRequest<'a> {
    pub offset: [i32; 3],
    pub basis: MenuBasis,
    pub model: AuxiliaryModelSource<'a>,
    pub values: AuxiliaryValueSource<'a>,
    pub ray: ray::RayRequest,
    pub residency: Option<&'a crate::ResidentRuntime>,
}

/// 004F2C40 over the ray table 00544C20. The output's flag is 1 for a hit,
/// whose +10 is the ray's length left past the nearest entry.
pub(crate) fn query_auxiliary_ray(
    request: AuxiliaryRayRequest<'_>,
    process: &mut ProcessFurnitureQueryState,
    owners: &crate::model_process::ModelProcessOwners,
    output: &mut CollisionContact,
) -> Result<(), AuxiliaryModelError> {
    let scope = owners.scratch.begin_draw()?;
    let mut run = Run {
        graphics: &owners.graphics,
        scratch: &owners.scratch,
        scope: scope.clone(),
        process,
        basis_math: &owners.basis_math,
        residency: request.residency,
        terrain: None,
        counterpart: None,
        ray: Some(request.ray),
    };
    let mut stack = StackContext {
        origin: [0; 3],
        basis: request.basis,
        table: ModelSurfaceTableOrigin::TemplateDefault,
        saved_basis: MenuBasis::INITIAL,
    };
    let result = run.execute(
        Working::Stack(&mut stack),
        request.model,
        request.model,
        request.offset,
        0,
        CallbackTable::Ray544c20,
        request.values,
        output,
    );
    let exit = if result.is_ok() {
        ModelScratchDrawExit::NormalReturn
    } else {
        ModelScratchDrawExit::StoppedOnError
    };
    owners.scratch.finish_draw(&scope, exit)?;
    result
}

impl Run<'_> {
    /// The wrappers' recursion (004F18B0, 004F2690): B's program on its
    /// captured stack context (or the process B) with `table`.
    fn run_counterpart(
        &mut self,
        counterpart: AuxiliaryModelSource<'_>,
        relative: [i32; 3],
        radius: i32,
        table: CallbackTable,
        output: &mut CollisionContact,
    ) -> Result<(), AuxiliaryModelError> {
        if let Some(mut stack) = self.counterpart.take() {
            let result = self.execute(
                Working::Stack(&mut stack.context),
                counterpart,
                counterpart,
                relative,
                radius,
                table,
                stack.values,
                output,
            );
            self.counterpart = Some(stack);
            result
        } else {
            self.execute(
                Working::ProcessB,
                counterpart,
                counterpart,
                relative,
                radius,
                table,
                AuxiliaryValueSource::Required,
                output,
            )
        }
    }

    fn execute(
        &mut self,
        mut working: Working<'_>,
        source: AuxiliaryModelSource<'_>,
        counterpart: AuxiliaryModelSource<'_>,
        relative: [i32; 3],
        radius: i32,
        callbacks: CallbackTable,
        values: AuxiliaryValueSource<'_>,
        output: &mut CollisionContact,
    ) -> Result<(), AuxiliaryModelError> {
        set_flag(output, 0); //4F2C5E, before header0E and every context store.
        let model = source.model;
        if model.header.raw_0e == 0 {
            return Ok(());
        }
        match &mut working {
            Working::Stack(context) => {
                context.origin[0] = 0;
                context.origin[1] = 0;
                context.origin[2] = 0;
                context.table = ModelSurfaceTableOrigin::TemplateDefault;
            }
            Working::ProcessB => self.process.publish_b_auxiliary_entry(),
        }
        //Unsigned WORD+08, exactly two shades, no ambient/vertex init phase.
        let allocation = self
            .scratch
            .allocate(&self.scope, model.header.vertex_slot_count, 2)?;
        if matches!(working, Working::ProcessB) {
            self.process
                .publish_b_auxiliary_allocation(allocation.clone());
            self.process
                .publish_b_auxiliary_raw_source(source.qualification.clone());
        }
        let mut vertices = ModelVertices::in_scratch(self.scratch, allocation.clone())?;
        let context = self.context(&working);
        self.publish(&context)?;
        let program =
            model
                .auxiliary_program
                .as_ref()
                .ok_or(AuxiliaryModelError::RequiredProgram {
                    model: model.span.start,
                })?;
        let mut previous =
            CollisionContact::from_native_words([None, None, None, None, Some(u32::MAX)]);
        let mut best = previous;
        // 8C's +28 and +3C: whether a group is open, and where it ends.
        let mut group_open = false;
        let mut group_end: Option<usize> = None;
        let mut pc = 0;
        while let Some(instruction) = program.instructions.get(pc) {
            let next = pc + 1;
            match &instruction.kind {
                Op::SharedVertexHandler { arguments, .. }
                    if matches!(instruction.opcode, 0x18 | 0x2f) =>
                {
                    let count = if instruction.opcode == 0x18 { 3 } else { 4 };
                    if arguments.len() != count {
                        return Err(AuxiliaryModelError::InvalidOperation {
                            opcode: instruction.opcode,
                            address: instruction.span.start,
                        });
                    }
                    //4EC950 resolves packed weight before mode or source-slot reads.
                    let operation = if instruction.opcode == 0x18 {
                        crate::model_vertices::ModelPairOperation::Midpoint
                    } else {
                        crate::model_vertices::ModelPairOperation::Interpolate {
                            weight: values.evaluate(arguments[3], instruction.span.start)?,
                        }
                    };
                    vertices
                        .execute_pair(
                            crate::model_vertices::ModelPairVertexRequest {
                                command: ModelVertexCommand::AuxiliaryOpcode(instruction.opcode),
                                destination: arguments[0] as i16,
                                sources: [arguments[1] as i16, arguments[2] as i16],
                                operation,
                            },
                            &mut ProcessModelProjector {
                                process: self.graphics,
                                table: ModelSurfaceTableOrigin::TemplateDefault,
                            },
                        )
                        .map_err(AuxiliaryModelError::VertexExecution)?;
                }
                Op::ChildModel {
                    model,
                    orientation,
                    state_words,
                    ..
                } if instruction.opcode == 5 => {
                    clear_callback_result(output);
                    let attachment = state_words.first().copied().ok_or(
                        AuxiliaryModelError::InvalidOperation {
                            opcode: 5,
                            address: instruction.span.start,
                        },
                    )? as i16;
                    self.execute_child(
                        &working,
                        &vertices,
                        child::AuxiliaryChildRequest {
                            slot: model.raw() as u16,
                            orientation: orientation.raw(),
                            attachment,
                            counterpart,
                            relative,
                            radius,
                            callbacks,
                            values,
                        },
                        output,
                    )?;
                    self.retain_callback_result(
                        output,
                        &mut previous,
                        &mut best,
                        program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                    )?;
                }
                Op::Rotate { first, second, .. } if instruction.opcode == 0x30 => {
                    self.rotate_saved(
                        &mut working,
                        first,
                        second.as_ref(),
                        instruction.span.start,
                        values,
                    )?;
                }
                Op::SharedVertexHandler { arguments, .. } if instruction.opcode == 7 => {
                    let [destination] = arguments.as_slice() else {
                        return Err(AuxiliaryModelError::InvalidOperation {
                            opcode: 7,
                            address: instruction.span.start,
                        });
                    };
                    let mut projector = ProcessModelProjector {
                        process: self.graphics,
                        table: ModelSurfaceTableOrigin::TemplateDefault,
                    };
                    vertices
                        .execute_auxiliary_raw(
                            model,
                            &context,
                            ModelVertexCommand::AuxiliaryOpcode(7),
                            *destination as i16,
                            &mut projector,
                        )
                        .map_err(AuxiliaryModelError::VertexExecution)?;
                }
                Op::Callback {
                    callback_offset,
                    parameters,
                    vertex_slot,
                } => {
                    //8E/8F reset flag/score before signed slot resolution/callback.
                    clear_callback_result(output);
                    if callbacks == CallbackTable::Ray544c20 {
                        let ray = self.ray.ok_or(AuxiliaryModelError::RequiredCallback {
                            table: table_address(callbacks),
                            offset: *callback_offset,
                            address: instruction.span.start,
                        })?;
                        let world = vertices.world(*vertex_slot)?;
                        let hit = match (instruction.opcode, parameters.as_slice()) {
                            (0x8e, [radius]) => ray::sphere(ray, relative, world, *radius),
                            (0x8f, [x, y, z]) => ray::box_hit(ray, relative, world, [*x, *y, *z]),
                            // 004F3870 stores the cleared flag.
                            (0x90, _) => None,
                            _ => {
                                return Err(AuxiliaryModelError::InvalidOperation {
                                    opcode: instruction.opcode,
                                    address: instruction.span.start,
                                })
                            }
                        };
                        if let Some(score) = hit {
                            let mut words = output.native_words();
                            words[0] = Some(1);
                            words[4] = Some(score as u32);
                            *output = CollisionContact::from_native_words(words);
                        }
                        self.retain_callback_result(
                            output,
                            &mut previous,
                            &mut best,
                            program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                        )?;
                        pc = next;
                        continue;
                    }
                    if callbacks == CallbackTable::Terrain537090 && instruction.opcode != 0x8e {
                        //8F +08 0044FBD0 (RET 8) and 90 +04 0044FBC0 (RET 0C)
                        //store flag and +10 one; neither reads its operands.
                        let mut words = output.native_words();
                        words[0] = Some(1);
                        words[4] = Some(1);
                        *output = CollisionContact::from_native_words(words);
                        self.retain_callback_result(
                            output,
                            &mut previous,
                            &mut best,
                            program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                        )?;
                        pc = next;
                        continue;
                    }
                    if instruction.opcode == 0x8f
                        && *callback_offset == 8
                        && callbacks == CallbackTable::Sphere544c08
                    {
                        let [x, y, z] = parameters.as_slice() else {
                            return Err(AuxiliaryModelError::InvalidOperation {
                                opcode: 0x8f,
                                address: instruction.span.start,
                            });
                        };
                        apply_contact_geometry(
                            output,
                            ContactGeometryRequest::SphereBox {
                                request: ContactSphere {
                                    center: relative,
                                    radius,
                                },
                                center: vertices.world(*vertex_slot)?,
                                half_extents: [*x, *y, *z],
                                normals: crate::object_runtime::ContactAxisNormals::NATIVE_STARTUP,
                            },
                        );
                        self.retain_callback_result(
                            output,
                            &mut previous,
                            &mut best,
                            program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                        )?;
                        pc = next;
                        continue;
                    }
                    if instruction.opcode == 0x8f
                        && *callback_offset == 8
                        && callbacks == CallbackTable::Point544bf0
                    {
                        //4F1D20: the zero-radius request's point against the box.
                        let [x, y, z] = parameters.as_slice() else {
                            return Err(AuxiliaryModelError::InvalidOperation {
                                opcode: 0x8f,
                                address: instruction.span.start,
                            });
                        };
                        apply_contact_geometry(
                            output,
                            ContactGeometryRequest::PointBox {
                                point: relative,
                                center: vertices.world(*vertex_slot)?,
                                half_extents: [*x, *y, *z],
                                normals: crate::object_runtime::ContactAxisNormals::NATIVE_STARTUP,
                            },
                        );
                        self.retain_callback_result(
                            output,
                            &mut previous,
                            &mut best,
                            program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                        )?;
                        pc = next;
                        continue;
                    }
                    if let (0x8f, 8, CallbackTable::Box544c50 { half_extents }) =
                        (instruction.opcode, *callback_offset, callbacks)
                    {
                        //4F23B0: the carried box against B's box.
                        let [x, y, z] = parameters.as_slice() else {
                            return Err(AuxiliaryModelError::InvalidOperation {
                                opcode: 0x8f,
                                address: instruction.span.start,
                            });
                        };
                        apply_contact_geometry(
                            output,
                            ContactGeometryRequest::BoxBox {
                                center: relative,
                                half_extents,
                                other_center: vertices.world(*vertex_slot)?,
                                other_half_extents: [*x, *y, *z],
                                normals: crate::object_runtime::ContactAxisNormals::NATIVE_STARTUP,
                            },
                        );
                        self.retain_callback_result(
                            output,
                            &mut previous,
                            &mut best,
                            program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                        )?;
                        pc = next;
                        continue;
                    }
                    if instruction.opcode == 0x8f
                        && *callback_offset == 8
                        && callbacks == CallbackTable::Composite544c68
                    {
                        //4F2690: 4F18B0's recursion with the box table and
                        //the 8F half extents in place of the radius.
                        let [x, y, z] = parameters.as_slice() else {
                            return Err(AuxiliaryModelError::InvalidOperation {
                                opcode: 0x8f,
                                address: instruction.span.start,
                            });
                        };
                        let world = vertices.world(*vertex_slot)?;
                        let child_relative =
                            std::array::from_fn(|i| world[i].wrapping_sub(relative[i]));
                        self.run_counterpart(
                            counterpart,
                            child_relative,
                            0,
                            CallbackTable::Box544c50 {
                                half_extents: [*x, *y, *z],
                            },
                            output,
                        )?;
                        //4F26FB..2700 normal-only parent A publication.
                        self.publish(&context)?;
                        self.retain_callback_result(
                            output,
                            &mut previous,
                            &mut best,
                            program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                        )?;
                        pc = next;
                        continue;
                    }
                    if instruction.opcode != 0x8e || *callback_offset != 0 {
                        return Err(AuxiliaryModelError::RequiredCallback {
                            table: table_address(callbacks),
                            offset: *callback_offset,
                            address: instruction.span.start,
                        });
                    }
                    let [scalar] = parameters.as_slice() else {
                        return Err(AuxiliaryModelError::InvalidOperation {
                            opcode: 0x8e,
                            address: instruction.span.start,
                        });
                    };
                    let world = vertices.world(*vertex_slot)?;
                    match callbacks {
                        CallbackTable::Composite544c68 => {
                            //4F18B0 child+08 is captured B, +0C the captured
                            //counterpart H8. Header flag08 does not skip entry.
                            let child_relative =
                                std::array::from_fn(|i| world[i].wrapping_sub(relative[i]));
                            // B's own program uses the sphere table, so no
                            // nested composite call reaches this slot.
                            self.run_counterpart(
                                counterpart,
                                child_relative,
                                *scalar,
                                CallbackTable::Sphere544c08,
                                output,
                            )?;
                            //4F1909..190E normal-only parent A publication.
                            self.publish(&context)?;
                        }
                        //Point+00=004F1400 first writes request+20=0, then
                        //executes the same4F1420 kernel. The explicit Point
                        //policy supplied that value before this call.
                        //0044EC70 forwards to 0044EC90: request +10/+14/+18
                        //plus the vertex, wrapped per DWORD; radius is the 8E scalar.
                        CallbackTable::Terrain537090 => {
                            let terrain =
                                self.terrain.ok_or(AuxiliaryModelError::RequiredCallback {
                                    table: table_address(callbacks),
                                    offset: *callback_offset,
                                    address: instruction.span.start,
                                })?;
                            let center =
                                std::array::from_fn(|i| relative[i].wrapping_add(world[i]));
                            terrain_sphere_contact(
                                output,
                                terrain.cells,
                                terrain.grid,
                                center,
                                *scalar,
                            )?;
                        }
                        // The ray table's callbacks returned above.
                        CallbackTable::Ray544c20 => {
                            return Err(AuxiliaryModelError::RequiredCallback {
                                table: table_address(callbacks),
                                offset: *callback_offset,
                                address: instruction.span.start,
                            })
                        }
                        //4F15F0: the carried box against B's sphere.
                        CallbackTable::Box544c50 { half_extents } => apply_contact_geometry(
                            output,
                            ContactGeometryRequest::BoxSphere {
                                center: relative,
                                half_extents,
                                sphere: ContactSphere {
                                    center: world,
                                    radius: *scalar,
                                },
                                normals: crate::object_runtime::ContactAxisNormals::NATIVE_STARTUP,
                            },
                        ),
                        CallbackTable::Point544bf0 | CallbackTable::Sphere544c08 => {
                            apply_contact_geometry(
                                output,
                                ContactGeometryRequest::SpherePair {
                                    request: ContactSphere {
                                        center: relative,
                                        radius,
                                    },
                                    candidate: ContactSphere {
                                        center: world,
                                        radius: *scalar,
                                    },
                                },
                            )
                        }
                    }
                    self.retain_callback_result(
                        output,
                        &mut previous,
                        &mut best,
                        program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                    )?;
                }
                Op::GroupBegin { target, .. } => {
                    //4F33CF: +10 = -1, the group opens, +3C takes its end.
                    let mut words = output.native_words();
                    words[4] = Some(u32::MAX);
                    *output = CollisionContact::from_native_words(words);
                    group_open = true;
                    group_end = Some(*target);
                }
                Op::GroupCallback {
                    callback_offset,
                    vertex_slots,
                } => {
                    //4F347E/4F34AB: the table's +0C (89) or +10 (8A); a cleared
                    //flag skips to the group's end and closes it.
                    self.group_callback(
                        callbacks,
                        *callback_offset,
                        vertex_slots,
                        &vertices,
                        relative,
                        radius,
                        instruction,
                        output,
                    )?;
                    if output.flag() == Some(0) {
                        group_open = false;
                        match skip_group(program, group_end, pc, instruction)? {
                            Some(target) => {
                                pc = target;
                                continue;
                            }
                            None => break,
                        }
                    }
                }
                Op::GroupRequireHit { .. } => {
                    //4F34D8: a cleared flag skips to the group's end.
                    if output.flag() == Some(0) {
                        group_open = false;
                        match skip_group(program, group_end, pc, instruction)? {
                            Some(target) => {
                                pc = target;
                                continue;
                            }
                            None => break,
                        }
                    }
                }
                Op::GroupEnd => {
                    //4F33F4: an open group publishes as a hit callback does.
                    if group_open {
                        set_flag(output, 1);
                        self.retain_callback_result(
                            output,
                            &mut previous,
                            &mut best,
                            program.instructions.get(next).map(|i| i.opcode) == Some(0x95),
                        )?;
                    }
                }
                Op::BranchUnlessHit { target, .. } if instruction.opcode == 0x95 => {
                    if output.flag() != Some(1) {
                        if *target == program.terminator.start {
                            break;
                        }
                        let target_index = program
                            .instructions
                            .iter()
                            .position(|i| i.span.start == *target)
                            .ok_or(AuxiliaryModelError::BranchTarget { address: *target })?;
                        //Authenticated complete owned-grammar programs and
                        //inspected owned prefixes use forward branches.
                        //No arbitrary portable execution budget is native behavior.
                        if target_index <= pc {
                            return Err(AuxiliaryModelError::RequiredBackwardBranch {
                                address: instruction.span.start,
                                target: *target,
                            });
                        }
                        pc = target_index;
                        continue;
                    }
                }
                _ => {
                    return Err(AuxiliaryModelError::RequiredOperation {
                        opcode: instruction.opcode,
                        address: instruction.span.start,
                    })
                }
            }
            pc = next;
        }
        //88 retirement precedes final contact publication. Native strictly
        //positive best displacement publishes hit; rejection clears ONLY flag.
        self.scratch.retire(&self.scope, allocation.mark())?;
        if best.displacement().expect("initialized native best") > 0 {
            let mut words = best.native_words();
            words[0] = Some(1);
            *output = CollisionContact::from_native_words(words);
        } else {
            set_flag(output, 0);
        }
        Ok(())
    }

    fn context(&self, working: &Working<'_>) -> ModelContext {
        match working {
            Working::Stack(context) => ModelContext::new(context.origin, context.basis),
            Working::ProcessB => {
                ModelContext::new(self.process.pair_origin(), self.process.query_bases()[0])
            }
        }
        //This ephemeral publication adapter initializes no saved/register bytes
        //in either backing context. Unsupported operations cannot read it.
    }
    fn publish(&self, context: &ModelContext) -> Result<(), AuxiliaryModelError> {
        let transform = context.publish(self.graphics.view()?, self.graphics.shift()?);
        self.graphics.publish_model(PublishedModelWorld {
            position: context.position,
            basis: context.basis,
            transform,
        })?;
        Ok(())
    }
    /// 89 (`+0C`) and 8A (`+10`) of `callbacks`, over the opcode's slots.
    #[allow(clippy::too_many_arguments)]
    fn group_callback(
        &self,
        callbacks: CallbackTable,
        callback_offset: u8,
        slots: &[i16],
        vertices: &ModelVertices,
        relative: [i32; 3],
        radius: i32,
        instruction: &inf_formats::ModelAuxiliaryInstruction,
        output: &mut CollisionContact,
    ) -> Result<(), AuxiliaryModelError> {
        let resolved = slots
            .iter()
            .map(|&slot| vertices.world(slot))
            .collect::<Result<Vec<_>, _>>()?;
        let words = group_words(
            callbacks,
            callback_offset,
            &resolved,
            relative,
            radius,
            self.ray.map(|ray| ray.length),
            output.native_words(),
        )
        .map_err(|error| match error {
            GroupError::Ray => AuxiliaryModelError::RequiredCallback {
                table: table_address(callbacks),
                offset: callback_offset,
                address: instruction.span.start,
            },
            GroupError::Operands => AuxiliaryModelError::InvalidOperation {
                opcode: instruction.opcode,
                address: instruction.span.start,
            },
            GroupError::Score => AuxiliaryModelError::RequiredContact { offset: 0x10 },
        })?;
        *output = CollisionContact::from_native_words(words);
        Ok(())
    }
    fn retain_callback_result(
        &self,
        output: &mut CollisionContact,
        previous: &mut CollisionContact,
        best: &mut CollisionContact,
        suppress_best: bool,
    ) -> Result<(), AuxiliaryModelError> {
        if output.flag() != Some(1) {
            return Ok(());
        }
        let displacement = output
            .displacement()
            .ok_or(AuxiliaryModelError::RequiredContact { offset: 0x10 })?;
        if displacement == -1 {
            let old = previous.native_words();
            let mut words = output.native_words();
            //4F31D7..31F0 inherits score and three normal DWORDs, even if their
            //constructor backing is unknown. No invented unit normal.
            words[4] = old[4];
            words[1..4].copy_from_slice(&old[1..4]);
            *output = CollisionContact::from_native_words(words);
        }
        *previous = *output;
        if !suppress_best
            && output.displacement().expect("callback score owned")
                > best.displacement().expect("native best initialized")
        {
            *best = *output;
        }
        Ok(())
    }
}
/// Why a group test could not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupError {
    /// The ray table without the sight's ray.
    Ray,
    /// An offset other than `+0C`/`+10`, or too few vertices.
    Operands,
    /// The sphere and point tests compare the unknown `+10`.
    Score,
}

/// The result's five DWORDs after 89 (`callback_offset` 12) or 8A (16) of
/// `callbacks`, with the opcode's vertices resolved. The ray table needs the
/// sight's length.
pub(crate) fn group_words(
    callbacks: CallbackTable,
    callback_offset: u8,
    vertices: &[[i32; 3]],
    relative: [i32; 3],
    radius: i32,
    ray_length: Option<i32>,
    mut words: [Option<u32>; 5],
) -> Result<[Option<u32>; 5], GroupError> {
    let vertex = |index: usize| vertices.get(index).copied().ok_or(GroupError::Operands);
    match callbacks {
        CallbackTable::Ray544c20 => {
            let length = ray_length.ok_or(GroupError::Ray)?;
            //4F3B31/4F3C6F clear the flag before any test; a hit stores the
            //flag and +10 only.
            words[0] = Some(0);
            let hit = match callback_offset {
                12 => ray::group4(
                    length,
                    relative,
                    [vertex(0)?, vertex(1)?, vertex(2)?, vertex(3)?],
                ),
                16 => ray::group3(length, relative, [vertex(0)?, vertex(1)?, vertex(2)?]),
                _ => return Err(GroupError::Operands),
            };
            if let Some(score) = hit {
                words[0] = Some(1);
                words[4] = Some(score as u32);
            }
        }
        // The box table's +0C/+10 are the composite's 4F2B30.
        CallbackTable::Composite544c68 | CallbackTable::Box544c50 { .. } => {
            //4F2B30: the flag and +10 one, then the request's normal.
            let mut contact = CollisionContact::from_native_words(words);
            apply_contact_geometry(
                &mut contact,
                ContactGeometryRequest::GroupDirection {
                    relative_xyz: relative,
                },
            );
            words = contact.native_words();
        }
        CallbackTable::Terrain537090 => {
            //44FBE0: the flag and +10 one.
            words[0] = Some(1);
            words[4] = Some(1);
        }
        CallbackTable::Sphere544c08 | CallbackTable::Point544bf0 => {
            //4F2720/4F2710 forward 89 to the +10 test, which reads three
            //slots; the point table's 4F2730 negates the distance alone.
            let radius = if callbacks == CallbackTable::Point544bf0 {
                0
            } else {
                radius
            };
            match group::plane([vertex(0)?, vertex(1)?, vertex(2)?], relative, radius) {
                group::PlaneHit::Miss => words[0] = Some(0),
                group::PlaneHit::Hit {
                    penetration,
                    normal,
                } => {
                    let score = words[4].ok_or(GroupError::Score)? as i32;
                    words[0] = Some(1);
                    if score < 0 || penetration < score {
                        for (word, value) in words[1..4].iter_mut().zip(normal) {
                            *word = Some(value);
                        }
                        words[4] = Some(penetration as u32);
                    }
                }
            }
        }
    }
    Ok(words)
}

/// 89/8A/8B's miss: the instruction at 8C's end, or None for the
/// terminator. A miss before any 8C would read unwritten +3C.
fn skip_group(
    program: &inf_formats::ModelAuxiliaryProgram,
    group_end: Option<usize>,
    pc: usize,
    instruction: &inf_formats::ModelAuxiliaryInstruction,
) -> Result<Option<usize>, AuxiliaryModelError> {
    let target = group_end.ok_or(AuxiliaryModelError::RequiredOperation {
        opcode: instruction.opcode,
        address: instruction.span.start,
    })?;
    if target == program.terminator.start {
        return Ok(None);
    }
    let index = program
        .instructions
        .iter()
        .position(|i| i.span.start == target)
        .ok_or(AuxiliaryModelError::BranchTarget { address: target })?;
    if index <= pc {
        return Err(AuxiliaryModelError::RequiredBackwardBranch {
            address: instruction.span.start,
            target,
        });
    }
    Ok(Some(index))
}

fn table_address(table: CallbackTable) -> u32 {
    match table {
        CallbackTable::Composite544c68 => 0x0054_4c68,
        CallbackTable::Point544bf0 => 0x0054_4bf0,
        CallbackTable::Sphere544c08 => 0x0054_4c08,
        CallbackTable::Terrain537090 => 0x0053_7090,
        CallbackTable::Ray544c20 => 0x0054_4c20,
        CallbackTable::Box544c50 { .. } => 0x0054_4c50,
    }
}
fn set_flag(output: &mut CollisionContact, flag: u32) {
    let mut words = output.native_words();
    words[0] = Some(flag);
    *output = CollisionContact::from_native_words(words);
}
fn clear_callback_result(output: &mut CollisionContact) {
    let mut words = output.native_words();
    words[0] = Some(0);
    words[4] = Some(u32::MAX);
    *output = CollisionContact::from_native_words(words);
}

#[cfg(test)]
mod tests;
