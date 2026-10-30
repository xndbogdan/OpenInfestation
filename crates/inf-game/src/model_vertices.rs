//! Per-invocation 24-byte vertices: 004EBE60, 004EBFE0, 004EC0A0,
//! midpoint 004EC410 (23), origin offset 004EC810 (25), and interpolation
//! 004EC950 (26).
//!
//! Allocation does not initialize native memory. Process views load the actual
//! retained arena bytes; isolated fixtures explicitly retain unknown backing.
//! Reached writes commit exact fields to the shared pool, including on errors.
//! No pool borrow spans a projector or required callback.

mod auxiliary_raw;
pub(crate) mod exact;

use std::convert::Infallible;

use inf_formats::{ModelEntry, ModelInstruction, ModelInstructionKind};
use thiserror::Error;

use crate::{
    model_context::{fistp_low32, ModelContext},
    model_scratch::{ModelScratchAllocation, ModelScratchError, ProcessModelScratch},
    model_values::{ModelValueCallback, ModelValueError, ModelValues},
};

/// Native slot +0C..+17. Projection writes are byte-granular: for example the
/// clip byte can be known while fog and the two following bytes remain unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelVertexProjection {
    pub bytes: [Option<u8>; 12],
}

impl ModelVertexProjection {
    pub fn read_u32(&self, offset: usize) -> Option<u32> {
        let bytes = self.bytes.get(offset..offset.checked_add(4)?)?;
        Some(u32::from_le_bytes([
            bytes[0]?, bytes[1]?, bytes[2]?, bytes[3]?,
        ]))
    }

    /// Offsets are relative to slot +0C. Only a complete native DWORD is set.
    pub fn write_u32(&mut self, offset: usize, value: u32) {
        for (target, byte) in self.bytes[offset..offset + 4]
            .iter_mut()
            .zip(value.to_le_bytes())
        {
            *target = Some(byte);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelVertexSlot {
    /// Native +00/+04/+08, in world coordinates, not raw model coordinates.
    pub world: Option<[i32; 3]>,
    pub projection: ModelVertexProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelVertexInput {
    /// 004EBFE0 calls the selected projection entry at context+3C -> +1C.
    Local([i32; 3]),
    /// Generated world vertices call context+3C -> +20 (23/26/2C/2D).
    World([i32; 3]),
}

/// Object callback +4 behind opcode2D, separate from its packed-value callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelCustomVertexPolicy {
    /// Actor compute 00478950 and its simulation inputs remain unowned. The
    /// executor retains its documented nibble2 skip approximation for this mode.
    Unowned,
    /// Frontend/name-entry object vtables select 0043A2D0. It fetches low WORDs
    /// from invocation slots through 00439E10 and sign-extends each output.
    /// Its default arm's stack residue leaves the destination unknown.
    UnsignedRowSource,
    /// Furniture object callback+4 selects 004358A0. Selector7 emits
    /// through required live process services; all other selectors write zero.
    Furniture,
    /// Actual object source+10 is required when a common2D mode reaches it.
    /// It shares 004EC670's compute/store/project packet, without the legacy
    /// Unowned nibble2 omission or furniture callback policy.
    RequiredObject,
}

/// The parent slot opcode 2A copies into destination + 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MirrorCopySource {
    /// 004ED39D: the passed slot's partner, `slot ^ 1`.
    #[default]
    Partner,
    /// Port fix: a copy that writes only destination + 1 reads the pair's
    /// mirror slot, `slot | 1`. A right-side gun mount copies its hull
    /// points that way from the even slots its parent passes; the hover
    /// morph model 120 and the buggy model 118 pass it the odd slots
    /// instead, so retail joins it to the left side's points.
    MirrorSide,
}

/// 004EC670 sign-extends the complete selector WORD before calling object+4.
/// The extras remain unsigned WORDs and refer to the current invocation.
#[derive(Clone, Copy, Debug)]
pub struct ModelCustomVertexRequest<'a> {
    pub selector: i32,
    pub extras: &'a [u16],
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ModelCustomVertexError<E> {
    #[error(transparent)]
    Fault(#[from] ModelVertexFault),
    #[error("required custom vertex service failed: {0}")]
    Host(E),
}

/// Actual 00489D90 arguments captured by 004358A0 or selector60's 00480CF0. The
/// scoped service supplies phase/network/source-handle custody and reuses the
/// real trail producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureCustomTrail {
    pub particle_type: u32,
    pub position: [u16; 3],
    pub velocity: [u16; 3],
    pub flags: u32,
}

/// Reached process operands for 004358A0, deliberately without inert defaults.
pub trait FurnitureCustomVertexHost {
    type Error;

    /// Raw signedDWORD0055F154, not capped simulation delta.
    fn load_factor(&mut self) -> Result<i32, Self::Error>;
    /// Actual 0048F060 result and shared process RNG mutation.
    fn random(&mut self) -> Result<u32, Self::Error>;
    fn birth_trail(&mut self, request: FurnitureCustomTrail) -> Result<(), Self::Error>;
}

/// 004358A0. Output writes precede the load-factor gate and actual birth.
/// This is the callback temporary, not opcode2D's destination vertex: the
/// caller publishes that destination only after this function returns normally.
pub fn compute_furniture_custom_vertex<H: FurnitureCustomVertexHost + ?Sized>(
    request: ModelCustomVertexRequest<'_>,
    vertices: &mut ModelVertices,
    output: &mut Option<[i32; 3]>,
    host: &mut H,
) -> Result<(), ModelCustomVertexError<H::Error>> {
    if request.selector != 7 {
        *output = Some([0; 3]);
        return Ok(());
    }
    let extra = |index| {
        request
            .extras
            .get(index)
            .copied()
            .ok_or(ModelVertexFault::InvalidInstruction { opcode: 0x2d })
    };
    let position = vertices.row_words_439e10(extra(0)?)?;
    *output = Some(position.map(|word| i32::from(word as i16)));
    let factor = host.load_factor().map_err(ModelCustomVertexError::Host)?;
    // SUB wraps in a DWORD; the sign-adjusted SAR implements truncation
    // toward zero. Negative and small divisors admit without a gate draw.
    let probability = 0xc000_i32.wrapping_sub(factor) / 1024;
    if probability >= 2 {
        let random = host.random().map_err(ModelCustomVertexError::Host)?;
        if i32::from(random as u16) % probability != 0 {
            return Ok(());
        }
    }
    let particle_type = match extra(1)? {
        1 => 6,
        2 => 8,
        3 => 11,
        _ => 5,
    };
    let random = host.random().map_err(ModelCustomVertexError::Host)?;
    host.birth_trail(FurnitureCustomTrail {
        particle_type,
        position,
        velocity: [0, 128 + ((random as u16) >> 9), 0],
        flags: 0x800,
    })
    .map_err(ModelCustomVertexError::Host)
}

/// Projection is required, including its partial writes on failure. Returning
/// EAX causes the caller to write the full DWORD at slot+10 only on success.
pub trait ModelVertexProjector {
    type Error;

    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error>;

    /// Non-native, for the scene feed: the view-space `(x, y, depth)` that
    /// [`Self::project`] places `input` at, where the projector can say.
    fn view_point(&mut self, _input: ModelVertexInput) -> Option<[i32; 3]> {
        None
    }

    /// Non-native, for Vulkan's drawing (the plan's item 15): that point
    /// without retail's rounding, from `exact`, a local point's model
    /// origin or a world point's twin, where the projector can say.
    fn exact_view_point(&mut self, _input: ModelVertexInput, _exact: [f64; 3]) -> Option<[f64; 3]> {
        None
    }

    /// Non-native: the native screen point of exact view point `view`,
    /// unrounded, where [`Self::project`] would place it on the screen.
    fn exact_screen_point(&mut self, _view: [f64; 3]) -> Option<[f64; 2]> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ModelVertexFault {
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error("model vertex slot {index} is outside {count} allocated slots")]
    InvalidSlot { index: i32, count: usize },
    #[error("world coordinates for model vertex slot {index} are uninitialized")]
    UnknownWorld { index: i32 },
    #[error("model vertex allocation or initial ranges do not match its raw vertex source")]
    InvalidInitialization,
    #[error("model opcode {opcode:#04x} is not supported by the vertex owner")]
    UnsupportedOpcode { opcode: u8 },
    #[error("selected renderer {callback:?} surface callback requires current terrain backing")]
    MissingSurfaceTerrain { callback: ModelSurfaceCallback },
    #[error("model vertex opcode {opcode:#04x} has an incompatible decoded record")]
    InvalidInstruction { opcode: u8 },
    #[error("custom vertex callback selector {selector} leaves its output unwritten")]
    UnwrittenCustomVertex { selector: i32 },
    #[error("required custom vertex callback is unowned")]
    UnownedCustomVertex,
    #[error("opcode 2C projector reached an enabled world wave and needs the sine table")]
    MissingSineTable,
    #[error("model vertex invocation stopped after an execution failure")]
    Stopped,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ModelVertexError<C = Infallible, P = Infallible> {
    #[error(transparent)]
    Fault(#[from] ModelVertexFault),
    #[error("model vertex packed value failed: {0}")]
    Value(ModelValueError<C>),
    #[error("model vertex projection failed: {0}")]
    Projection(P),
    #[error(transparent)]
    Custom(ModelCustomVertexError<C>),
}

#[derive(Clone, Debug)]
enum ModelVertexStorage {
    /// Supplied intrinsic fixture backing, without surrounding process history.
    IsolatedFixture,
    /// Cloning this storage shares the same owner and registered allocation.
    ProcessView {
        scratch: ProcessModelScratch,
        allocation: ModelScratchAllocation,
    },
}

#[derive(Clone, Debug)]
pub struct ModelVertices {
    // Existing primitive consumers borrow this semantic cache. Raw parent copy
    // separately reads the actual shared bytes, including partial world fields.
    slots: Vec<ModelVertexSlot>,
    storage: ModelVertexStorage,
    stopped: bool,
    /// Slots whose projected tail or world XYZ came from 0043A2D0's stack
    /// residue (CustomVertexStackResidueApproximation), until a later write.
    residue_tail: std::collections::BTreeSet<usize>,
    residue_world: std::collections::BTreeSet<usize>,
    /// Non-native, for the scene feed: each slot's view-space point as its
    /// last projection placed it, kept with the projected tail through copies
    /// and residue, the slots projected with a depth bias (opcode 2C's
    /// ground shadows), and the slots projected where opcode 2B dropped
    /// them onto the ground.
    views: Vec<Option<[i32; 3]>>,
    biased: std::collections::BTreeSet<usize>,
    grounded: std::collections::BTreeSet<usize>,
    /// Non-native, while the frame draws from them: each slot's exact twin
    /// ([`exact`]).
    exact: Option<Box<exact::ExactTwins>>,
}

/// Renderer+50's selected +10/+14 pair, independently of packed-value
/// callbacks and terrain availability. 004501A0 installs the Terrain pair;
/// the shared default table retains 004EBCD0/004EBCF0.
#[derive(Clone, Copy, Debug)]
pub enum ModelSurfaceBinding<'a> {
    Default,
    Terrain {
        /// Missing residency is a reached service boundary, not Default mode.
        terrain: Option<&'a crate::terrain_runtime::TerrainRuntime>,
        /// Actual H14 world entry-zero +04 and process 005471C0.
        world_flags: u32,
        wave_clock: u32,
        /// Retained renderer+04/+08/+0C, reached through model-context+58.
        renderer_xyz: [i32; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelSurfaceCallback {
    /// Renderer table+10: 004EBCD0 or 0044FF50.
    Ground,
    /// Renderer table+14: 004EBCF0 or 0044FFA0.
    Project,
}

pub struct ModelSurfaceCallbackInputs<'a> {
    pub binding: ModelSurfaceBinding<'a>,
    /// Original sine table; read only at a reached 00410F60 wave gate.
    pub table: Option<&'a [u16; 4096]>,
}

/// The two native command producers use different integer conversions.
/// Renderer004A0212 sign-extends a WORD; auxiliary004F2D3C zero-extends its
/// opcode BYTE before shifting it left eight bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelVertexCommand {
    RendererWord(u16),
    AuxiliaryOpcode(u8),
}

impl ModelVertexCommand {
    pub fn native_value(self) -> i32 {
        match self {
            Self::RendererWord(word) => i32::from(word as i16),
            Self::AuxiliaryOpcode(opcode) => i32::from(opcode) << 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelPairOperation {
    Midpoint,
    Interpolate {
        weight: u16,
    },
    /// 004EC810: both sources summed, minus the origin published at
    /// 0055F1C4..CC (00490900 turns the zero vector, adding nothing).
    OriginOffset {
        origin: [i32; 3],
    },
}

///Actual 004EC410/004EC950/004EC810 operands after any packed-weight callback.
///Command origin remains explicit; auxiliary18/2F/2C never masquerades as a
///render WORD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModelPairVertexRequest {
    pub command: ModelVertexCommand,
    pub destination: i16,
    pub sources: [i16; 2],
    pub operation: ModelPairOperation,
}

/// Ground 004EC510/Project 004EC5C0 consume signed vertex-slot WORDs,
/// independently of command origin. Auxiliary49/96 preserve their actual
/// positive4900/9600 commands; renderer WORDs retain MOVSX signedness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelSurfaceVertexRequest {
    pub command: ModelVertexCommand,
    pub destination: i16,
    pub source: i16,
}

impl ModelSurfaceVertexRequest {
    pub fn from_renderer(
        instruction: &ModelInstruction,
        callback: ModelSurfaceCallback,
    ) -> Result<Self, ModelVertexFault> {
        let opcode = instruction.raw_command as u8;
        let expected = match callback {
            ModelSurfaceCallback::Ground => 0x2b,
            ModelSurfaceCallback::Project => 0x2c,
        };
        if opcode != expected {
            return Err(ModelVertexFault::UnsupportedOpcode { opcode });
        }
        let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
            return Err(ModelVertexFault::InvalidInstruction { opcode });
        };
        let [destination, source] = command.arguments.as_slice() else {
            return Err(ModelVertexFault::InvalidInstruction { opcode });
        };
        Ok(Self {
            command: ModelVertexCommand::RendererWord(instruction.raw_command),
            destination: *destination as i16,
            source: *source as i16,
        })
    }

    fn native_command(self, callback: ModelSurfaceCallback) -> Result<i32, ModelVertexFault> {
        let (renderer_opcode, auxiliary_opcode) = match callback {
            ModelSurfaceCallback::Ground => (0x2b, 0x49),
            ModelSurfaceCallback::Project => (0x2c, 0x96),
        };
        match self.command {
            ModelVertexCommand::RendererWord(word) if word as u8 == renderer_opcode => {}
            ModelVertexCommand::AuxiliaryOpcode(opcode) if opcode == auxiliary_opcode => {}
            ModelVertexCommand::RendererWord(word) => {
                return Err(ModelVertexFault::UnsupportedOpcode { opcode: word as u8 });
            }
            ModelVertexCommand::AuxiliaryOpcode(opcode) => {
                return Err(ModelVertexFault::UnsupportedOpcode { opcode });
            }
        }
        Ok(self.command.native_value())
    }
}

/// Required context+58 -> object+50 -> +10/+14 callback for active2B/2C chunks.
/// Source access belongs to that callback and precedes destination access.
/// Mutable service state and already reached effects survive a failure; the
/// caller never publishes this chunk's temporary output on failure.
/// This typed domain borrows the current owned slots; native context/table
/// alias mutation and vertex-buffer rebinding remain outside it. Unpublished
/// partial temporary writes belong to the service, not the destination slot.
pub trait ModelSurfaceVertexCallbacks {
    fn generate(
        &mut self,
        vertices: &ModelVertices,
        source: i32,
        callback: ModelSurfaceCallback,
    ) -> Result<[i32; 3], ModelVertexFault>;
}

impl ModelSurfaceVertexCallbacks for ModelSurfaceCallbackInputs<'_> {
    fn generate(
        &mut self,
        vertices: &ModelVertices,
        source: i32,
        callback: ModelSurfaceCallback,
    ) -> Result<[i32; 3], ModelVertexFault> {
        // Native selects the required callback before passing its source
        // pointer. InvalidSlot/UnknownWorld are portable custody boundaries,
        // not claims about native invalid-pointer fault ordering inside44FFA0.
        match self.binding {
            ModelSurfaceBinding::Default => {
                // This portable domain admits complete owned XYZ vectors.
                // 004EBCD0 does not read native source Y; partial/aliased raw
                // storage receipts do not imply typed pointer-fault parity.
                let [x, y, z] = vertices.read_world(source)?;
                let adjustment = match callback {
                    ModelSurfaceCallback::Ground => 0,
                    ModelSurfaceCallback::Project => y.wrapping_add(300) / 2,
                };
                Ok([x.wrapping_add(adjustment), -300, z.wrapping_add(adjustment)])
            }
            ModelSurfaceBinding::Terrain {
                terrain,
                world_flags,
                wave_clock,
                renderer_xyz,
            } => {
                let terrain =
                    terrain.ok_or(ModelVertexFault::MissingSurfaceTerrain { callback })?;
                let world = vertices.read_world(source)?;
                match callback {
                    ModelSurfaceCallback::Ground => Ok([
                        world[0],
                        i32::from(terrain.height_at([world[0] as u16, world[2] as u16])),
                        world[2],
                    ]),
                    ModelSurfaceCallback::Project => terrain
                        .project_44ffa0(
                            world,
                            &crate::terrain_runtime::Project44FFA0Inputs {
                                world_flags,
                                wave_clock,
                                renderer_xyz,
                                table: self.table,
                            },
                        )
                        .map_err(|_| ModelVertexFault::MissingSineTable),
                }
            }
        }
    }
}

impl ModelVertices {
    /// 004EBE60 reserves exactly 24 bytes per slot without clearing them.
    pub fn new_invocation(slot_count: u16) -> Self {
        Self {
            slots: vec![ModelVertexSlot::default(); usize::from(slot_count)],
            storage: ModelVertexStorage::IsolatedFixture,
            stopped: false,
            residue_tail: Default::default(),
            residue_world: Default::default(),
            views: vec![None; usize::from(slot_count)],
            biased: Default::default(),
            grounded: Default::default(),
            exact: None,
        }
    }

    /// Load the actual retained allocation. Header counts were already used by
    /// 004EBE60; derive the view's count from that genuine descriptor.
    pub(crate) fn in_scratch(
        scratch: &ProcessModelScratch,
        allocation: ModelScratchAllocation,
    ) -> Result<Self, ModelVertexFault> {
        scratch.validate_allocation(&allocation)?;
        let mut slots = Vec::with_capacity(usize::from(allocation.layout.vertex_count));
        scratch.read_vertices(&allocation, |raw| slots.push(decode_slot(raw)))?;
        Ok(Self {
            views: vec![None; slots.len()],
            slots,
            storage: ModelVertexStorage::ProcessView {
                scratch: scratch.clone(),
                allocation,
            },
            stopped: false,
            residue_tail: Default::default(),
            residue_world: Default::default(),
            biased: Default::default(),
            grounded: Default::default(),
            exact: None,
        })
    }

    /// Seeds one slot's world triple in an isolated fixture.
    #[cfg(test)]
    pub(crate) fn seed_world(&mut self, index: usize, world: [i32; 3]) {
        self.slots[index].world = Some(world);
    }

    pub fn slots(&self) -> &[ModelVertexSlot] {
        &self.slots
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// The slot's projected tail is 0043A2D0 stack residue, which the port
    /// leaves unknown (CustomVertexStackResidueApproximation).
    pub fn residue_projection(&self, index: i16) -> bool {
        usize::try_from(index).is_ok_and(|index| self.residue_tail.contains(&index))
    }

    /// The slot's world XYZ is 0043A2D0 stack residue, left unknown.
    pub fn residue_world(&self, index: i16) -> bool {
        usize::try_from(index).is_ok_and(|index| self.residue_world.contains(&index))
    }

    /// Non-native, for the scene feed: the view-space point the slot's last
    /// projection placed it at, where the projector said.
    pub fn view_point(&self, index: i16) -> Option<[i32; 3]> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.views.get(index).copied().flatten())
    }

    /// Non-native, for the scene feed: the slot was projected with a depth
    /// bias, as opcode 2C's ground shadows are.
    pub fn depth_biased(&self, index: i16) -> bool {
        usize::try_from(index).is_ok_and(|index| self.biased.contains(&index))
    }

    /// Non-native, for the scene feed: the slot was last projected where
    /// opcode 2B (the Ground callback) dropped it onto the ground.
    pub fn ground_projected(&self, index: i16) -> bool {
        usize::try_from(index).is_ok_and(|index| self.grounded.contains(&index))
    }

    /// Keep exact twins of the slots from here on, the model's origin
    /// unrounded at `origin`.
    pub(crate) fn keep_exact(&mut self, origin: [f64; 3]) {
        self.exact = Some(Box::new(exact::ExactTwins::new(origin, self.slots.len())));
    }

    /// Non-native: the slot's exact world point, where an exact writer set
    /// one.
    pub fn exact_world(&self, index: i16) -> Option<[f64; 3]> {
        let exact = self.exact.as_deref()?;
        exact
            .worlds
            .get(usize::try_from(index).ok()?)
            .copied()
            .flatten()
    }

    /// Non-native: the slot's exact view-space point, as its last
    /// projection placed it.
    pub fn exact_view(&self, index: i16) -> Option<[f64; 3]> {
        let exact = self.exact.as_deref()?;
        exact
            .views
            .get(usize::try_from(index).ok()?)
            .copied()
            .flatten()
    }

    /// Non-native: the exact screen points of the projections so far, by
    /// the whole-pixel XY each rounds to, while twins are kept.
    pub(crate) fn exact_screens(
        &self,
    ) -> Option<std::rc::Rc<crate::fast_hash::FastMap<u32, [f32; 2]>>> {
        self.exact
            .as_deref()
            .map(|exact| std::rc::Rc::clone(&exact.screens))
    }

    /// Non-native, for the scene feed: the view-space point to draw the
    /// slot at, its exact twin where kept, else its whole-unit one.
    pub fn drawn_view(&self, index: i16) -> Option<[f64; 3]> {
        self.exact_view(index)
            .or_else(|| self.view_point(index).map(|view| view.map(f64::from)))
    }

    /// The twin a writer starts from: the slot's own, else its whole-unit
    /// point, while twins are kept.
    fn twin(&self, index: i32) -> Option<[f64; 3]> {
        let exact = self.exact.as_deref()?;
        let at = usize::try_from(index).ok()?;
        exact
            .worlds
            .get(at)
            .copied()
            .flatten()
            .or_else(|| self.slots.get(at)?.world.map(|world| world.map(f64::from)))
    }

    fn set_twin(&mut self, index: usize, twin: Option<[f64; 3]>) {
        if let Some(exact) = self.exact.as_deref_mut() {
            exact.worlds[index] = twin;
        }
    }

    /// A writer's twin for `result` from its sources' twins, while twins
    /// are kept: `exact` given them and the model's exact origin.
    fn twin_from<const N: usize>(
        &self,
        sources: [i32; N],
        exact: impl FnOnce([[f64; 3]; N], [f64; 3]) -> [f64; 3],
    ) -> Option<[f64; 3]> {
        let origin = self.exact.as_deref()?.origin;
        let mut twins = [[0.0; 3]; N];
        for (twin, source) in twins.iter_mut().zip(sources) {
            *twin = self.twin(source)?;
        }
        Some(exact(twins, origin))
    }

    pub fn slot(&self, index: i16) -> Result<&ModelVertexSlot, ModelVertexFault> {
        Ok(&self.slots[self.checked_index(i32::from(index))?])
    }

    pub fn world(&self, index: i16) -> Result<[i32; 3], ModelVertexFault> {
        self.read_world(i32::from(index))
    }

    /// 00439E10 takes an unsigned WORD row and reads LOWWORDs at byte offsets
    /// +00/+04/+08 of the current 24-byte invocation slot. Whole-world unknown
    /// custody is portable containment, not a claim of native per-axis faults.
    pub fn row_words_439e10(&self, index: u16) -> Result<[u16; 3], ModelVertexFault> {
        Ok(self.read_world(i32::from(index))?.map(|axis| axis as u16))
    }

    /// 004EC670: each two-bit mode invokes the same compute callback, then
    /// optionally stores world XYZ and/or projects them. The second pass is
    /// destination+1, with no XOR of the compute selector or source extras.
    pub fn execute_custom_2d<C, P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        policy: ModelCustomVertexPolicy,
        projector: &mut P,
        callback: &mut impl FnMut(
            ModelCustomVertexRequest<'_>,
            &mut ModelVertices,
            &mut Option<[i32; 3]>,
        ) -> Result<(), ModelCustomVertexError<C>>,
    ) -> Result<(), ModelVertexError<C, P::Error>> {
        let result = (|| {
            self.ensure_running()?;
            let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
                return Err(ModelVertexFault::InvalidInstruction { opcode: 0x2d }.into());
            };
            if instruction.raw_command as u8 != 0x2d
                || command.arguments.len() != 2 + usize::from(instruction.raw_command >> 12)
            {
                return Err(ModelVertexFault::InvalidInstruction { opcode: 0x2d }.into());
            }
            let destination = i32::from(command.arguments[0] as i16);
            let selector = i32::from(command.arguments[1] as i16);
            let extras = &command.arguments[2..];
            let mut modes = (instruction.raw_command >> 8) & 15;
            let mut parity = 0;
            // Native uses the same stack temporary across both passes.
            // A later callback failing before its writes retains that prefix.
            let mut output = None;
            while modes != 0 {
                if modes & 3 != 0 {
                    let target = self.checked_index(destination + parity)?;
                    let world = match policy {
                        ModelCustomVertexPolicy::Unowned => {
                            return Err(ModelVertexFault::UnsupportedOpcode { opcode: 0x2d }.into());
                        }
                        ModelCustomVertexPolicy::UnsignedRowSource => {
                            let extra = |index| {
                                extras
                                    .get(index)
                                    .copied()
                                    .ok_or(ModelVertexFault::InvalidInstruction { opcode: 0x2d })
                            };
                            //0043A2D0 deliberately masks to a byte; furniture
                            //004358A0 compares the full supplied DWORD instead.
                            let source = match selector & 0xff {
                                0 => Some(extra(0)? >> 8),
                                1 => Some(extra(1)? >> 8),
                                2 | 3 => Some(extra(0)?),
                                5 => Some(extra(0)? & 0xff),
                                // The default arm skips 00439E10 and returns
                                // the callback's uninitialized stack locals.
                                _ => None,
                            };
                            // 00439E10 uses an unsigned row index and reads
                            // byte offsets0/4/8, not a contiguous WORD triple.
                            match source {
                                Some(source) => Some(
                                    self.row_words_439e10(source)?
                                        .map(|component| i32::from(component as i16)),
                                ),
                                None => None,
                            }
                        }
                        ModelCustomVertexPolicy::Furniture
                        | ModelCustomVertexPolicy::RequiredObject => {
                            callback(
                                ModelCustomVertexRequest { selector, extras },
                                self,
                                &mut output,
                            )
                            .map_err(ModelVertexError::Custom)?;
                            Some(
                                output
                                    .ok_or(ModelVertexFault::UnwrittenCustomVertex { selector })?,
                            )
                        }
                    };
                    match world {
                        Some(world) => {
                            if modes & 1 != 0 {
                                self.write_world(target, world)?;
                            }
                            if modes & 2 != 0 {
                                self.project(target, ModelVertexInput::World(world), projector)?;
                            }
                        }
                        None => self.forget_stack_residue_vertex(target, modes & 3)?,
                    }
                }
                modes >>= 2;
                parity += 1;
            }
            Ok(())
        })();
        self.finish(result)
    }

    /// 004A019B..004A01CC projects raw sources before populating world ranges.
    /// Shade seeding precedes this method; normal/shade preprocessing follows it.
    /// Invalid authored bounds are rejected before callbacks as Rust containment.
    pub fn initialize<P: ModelVertexProjector>(
        &mut self,
        model: &ModelEntry,
        context: &ModelContext,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = self.initialize_inner(model, context, projector);
        self.finish(result)
    }

    /// Complete 004EBFE0 phase for Common05's independent signed range.
    /// Later world/normal phases are not preflighted here; reached projector
    /// bytes and successful EAX stores use the existing write-through owner.
    pub(crate) fn initialize_projection_ranges<P: ModelVertexProjector>(
        &mut self,
        model: &ModelEntry,
        ranges: [i32; 3],
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = (|| {
            let [start, single_end, paired_end] = ranges;
            if start < 0
                || !(start <= single_end && single_end <= paired_end)
                || ranges.iter().any(|v| v & 1 != 0)
                || paired_end as usize > self.slots.len()
                || paired_end as usize / 2 > model.vertices.len()
            {
                return Err(ModelVertexFault::InvalidInitialization.into());
            }
            self.initialize_projection_ranges_inner(model, ranges.map(|v| v as usize), projector)
        })();
        self.finish(result)
    }

    fn initialize_projection_ranges_inner<P: ModelVertexProjector>(
        &mut self,
        model: &ModelEntry,
        [start, single_end, paired_end]: [usize; 3],
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        for index in (start..paired_end).step_by(2) {
            let raw = model.vertices[index / 2];
            let local = [i32::from(raw.x), i32::from(raw.y), i32::from(raw.z)];
            self.project(index, ModelVertexInput::Local(local), projector)?;
            if index >= single_end {
                self.project(
                    index + 1,
                    ModelVertexInput::Local([-local[0], local[1], local[2]]),
                    projector,
                )?;
            }
        }
        Ok(())
    }

    /// Executes the actual render-table handlers; stream wrappers which strip
    /// projection bits are a different caller and are not silently substituted.
    /// A 26 packed callback runs even when the command requests no output.
    pub fn execute<C: ModelValueCallback, P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        values: &mut ModelValues,
        callback: &mut C,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<C::Error, P::Error>> {
        self.ensure_running()?;
        let result = self.execute_inner(instruction, values, callback, projector);
        self.finish(result)
    }

    /// Opcodes 21 (004EC1E0), 24 (004EC710), 27 (004ECCE0), 28 (004ECAE0)
    /// and 29 (004ECFE0) from the renderer tables. The late table's wrappers
    /// run these handlers with the projection bits masked (world_stream.rs
    /// keeps that arithmetic); here a pair may also project through the
    /// renderer's +20. Weights resolve before the mode loop; 21 reflects
    /// through the origin last published to 0055F1C4..CC.
    pub fn execute_shaped<C: ModelValueCallback, P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        origin: [i32; 3],
        values: &mut ModelValues,
        callback: &mut C,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<C::Error, P::Error>> {
        self.ensure_running()?;
        let result = self.execute_shaped_inner(instruction, origin, values, callback, projector);
        self.finish(result)
    }

    fn execute_shaped_inner<C: ModelValueCallback, P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        origin: [i32; 3],
        values: &mut ModelValues,
        callback: &mut C,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<C::Error, P::Error>> {
        let opcode = instruction.raw_command as u8;
        let count = match opcode {
            0x21 => 2,
            0x24 => 4,
            0x27 => 6,
            0x28 => 5,
            0x29 => 7,
            _ => return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into()),
        };
        let args = match &instruction.kind {
            ModelInstructionKind::Opaque(command) => &command.arguments,
            ModelInstructionKind::UnresolvedDynamic(command) => &command.arguments,
            _ => return Err(ModelVertexFault::InvalidInstruction { opcode }.into()),
        };
        if args.len() != count {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        }
        let mut packed = |at: usize| {
            values
                .evaluate_packed(args[at], callback)
                .map(i32::from)
                .map_err(ModelVertexError::Value)
        };
        let weight = match opcode {
            0x27 => packed(5)?.wrapping_shl(14),
            0x28 => (packed(3)? + packed(4)?).wrapping_shl(14),
            0x29 => (packed(5)? + packed(6)?).wrapping_shl(14),
            _ => 0,
        };
        let destination = i32::from(args[0] as i16);
        let mut mode = u32::from((instruction.raw_command >> 8) & 15);
        let mut parity = 0;
        while mode != 0 {
            if mode & 3 != 0 {
                let source =
                    |this: &Self, at: usize| this.read_world(i32::from(args[at] as i16) ^ parity);
                let world: [i32; 3] = match opcode {
                    0x21 => {
                        let base = source(self, 1)?;
                        std::array::from_fn(|i| origin[i].wrapping_mul(2).wrapping_sub(base[i]))
                    }
                    0x24 => {
                        let [left, right, third] =
                            [source(self, 1)?, source(self, 2)?, source(self, 3)?];
                        std::array::from_fn(|i| {
                            left[i].wrapping_sub(third[i]).wrapping_add(right[i])
                        })
                    }
                    0x28 => {
                        let [left, right] = [source(self, 1)?, source(self, 2)?];
                        std::array::from_fn(|i| {
                            left[i].wrapping_add(
                                world_stream::mul_q31(right[i].wrapping_sub(left[i]), weight)
                                    .wrapping_mul(2),
                            )
                        })
                    }
                    _ => {
                        let points = [
                            source(self, 1)?,
                            source(self, 2)?,
                            source(self, 3)?,
                            source(self, 4)?,
                        ];
                        std::array::from_fn(|i| world_stream::cubic(points.map(|p| p[i]), weight))
                    }
                };
                let target = self.checked_index(destination + parity)?;
                let at = |slot: usize| i32::from(args[slot] as i16) ^ parity;
                let twin = match opcode {
                    0x21 => self.twin_from([at(1)], |[base], origin| {
                        std::array::from_fn(|i| 2.0 * origin[i] - base[i])
                    }),
                    0x24 => self.twin_from([at(1), at(2), at(3)], |[left, right, third], _| {
                        std::array::from_fn(|i| left[i] - third[i] + right[i])
                    }),
                    0x28 => self.twin_from([at(1), at(2)], |[left, right], _| {
                        std::array::from_fn(|i| {
                            left[i] + (right[i] - left[i]) * 2.0 * exact::q31(weight)
                        })
                    }),
                    _ => self.twin_from([at(1), at(2), at(3), at(4)], |points, _| {
                        std::array::from_fn(|i| exact::cubic(points.map(|p| p[i]), weight))
                    }),
                };
                if mode & 1 != 0 {
                    self.write_world(target, world)?;
                    self.set_twin(target, twin);
                }
                if mode & 2 != 0 {
                    self.project_twin(target, ModelVertexInput::World(world), twin, projector)?;
                }
            }
            mode >>= 2;
            parity = 1;
        }
        Ok(())
    }

    /// Opcode 25 (004EC810) from the renderer tables. The caller passes the
    /// origin it last published to 0055F1C4..CC; no value callback runs.
    pub fn execute_origin_offset<P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        origin: [i32; 3],
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = self.execute_origin_offset_inner(instruction, origin, projector);
        self.finish(result)
    }

    fn execute_origin_offset_inner<P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        origin: [i32; 3],
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        let opcode = instruction.raw_command as u8;
        let ModelInstructionKind::Opaque(command) = &instruction.kind else {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        };
        let (0x25, [destination, first, second]) = (opcode, command.arguments.as_slice()) else {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        };
        self.execute_pair_inner(
            ModelPairVertexRequest {
                command: ModelVertexCommand::RendererWord(instruction.raw_command),
                destination: *destination as i16,
                sources: [*first as i16, *second as i16],
                operation: ModelPairOperation::OriginOffset { origin },
            },
            projector,
        )
    }

    fn initialize_inner<P: ModelVertexProjector>(
        &mut self,
        model: &ModelEntry,
        context: &ModelContext,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        let header = model.header;
        let [a, b, c, d] = header.vertex_ranges.map(usize::from);
        let start = usize::from(header.reserved_10);
        if usize::from(header.vertex_slot_count) != self.slots.len()
            || usize::from(header.raw_vertex_count) != model.vertices.len()
            || [start, a, b, c, d, self.slots.len()]
                .iter()
                .any(|value| value % 2 != 0)
            || !(start <= b && a <= b && b <= c && c <= d && d <= self.slots.len())
            || d / 2 > model.vertices.len()
        {
            return Err(ModelVertexFault::InvalidInitialization.into());
        }
        self.initialize_projection_ranges_inner(model, [start, b, d], projector)?;
        self.initialize_world_ranges_inner(model, context, [a as i32, b as i32, c as i32])
    }

    /// 004EC510 masks four mode bits, calls the selected renderer+10 callback,
    /// and preserves an unbiased successful world-projector return.
    pub fn execute_ground_2b<T: ModelSurfaceVertexCallbacks, P: ModelVertexProjector>(
        &mut self,
        request: ModelSurfaceVertexRequest,
        callbacks: &mut T,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = self.execute_ground_2b_inner(request, callbacks, projector);
        self.finish(result)
    }

    fn execute_ground_2b_inner<T: ModelSurfaceVertexCallbacks, P: ModelVertexProjector>(
        &mut self,
        request: ModelSurfaceVertexRequest,
        callbacks: &mut T,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        let mut destination = i32::from(request.destination);
        let source = i32::from(request.source);
        let mut mode = ((request.native_command(ModelSurfaceCallback::Ground)? >> 8) as u32) & 15;
        let mut parity = 0;
        while mode != 0 {
            if mode & 3 != 0 {
                let grounded =
                    callbacks.generate(self, source ^ parity, ModelSurfaceCallback::Ground)?;
                let target = self.checked_index(destination)?;
                let twin = self.ground_twin(source ^ parity, grounded);
                if mode & 1 != 0 {
                    self.write_world(target, grounded)?;
                    self.set_twin(target, twin);
                }
                if mode & 2 != 0 {
                    self.project_twin(target, ModelVertexInput::World(grounded), twin, projector)?;
                    self.grounded.insert(target);
                }
            }
            mode >>= 2;
            destination = destination.wrapping_add(1);
            parity = 1;
        }
        Ok(())
    }

    /// 004EC5C0 consumes the whole signed-shifted command with logical pair
    /// shifts. Every chunk advances its destination, including empty chunks;
    /// source XOR is zero for the first chunk, then one for every later chunk.
    /// Successful projection alone applies the native wrapping depth bias20.
    pub fn execute_project_2c<T: ModelSurfaceVertexCallbacks, P: ModelVertexProjector>(
        &mut self,
        request: ModelSurfaceVertexRequest,
        terrain: &mut T,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = self.execute_project_2c_inner(request, terrain, projector);
        self.finish(result)
    }

    fn execute_project_2c_inner<T: ModelSurfaceVertexCallbacks, P: ModelVertexProjector>(
        &mut self,
        request: ModelSurfaceVertexRequest,
        terrain: &mut T,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        let mut mode = (request.native_command(ModelSurfaceCallback::Project)? >> 8) as u32;
        let mut destination = i32::from(request.destination);
        let mut source_selector = 0;
        while mode != 0 {
            if mode & 3 != 0 {
                let projected = terrain.generate(
                    self,
                    i32::from(request.source) ^ source_selector,
                    ModelSurfaceCallback::Project,
                )?;
                let target = self.checked_index(destination)?;
                let twin = self.ground_twin(i32::from(request.source) ^ source_selector, projected);
                if mode & 1 != 0 {
                    self.write_world(target, projected)?;
                    self.set_twin(target, twin);
                }
                if mode & 2 != 0 {
                    // 004EC642/649 only run after a normal +20 return. Other
                    // vertex owners retain their unbiased projector result.
                    self.project_with_depth_bias(
                        target,
                        ModelVertexInput::World(projected),
                        twin,
                        projector,
                        20,
                    )?;
                }
            }
            mode >>= 2;
            destination += 1;
            source_selector = 1;
        }
        Ok(())
    }

    fn execute_inner<C: ModelValueCallback, P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        values: &mut ModelValues,
        callback: &mut C,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<C::Error, P::Error>> {
        let opcode = instruction.raw_command as u8;
        let count = match opcode {
            0x23 => 3,
            0x26 => 4,
            _ => return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into()),
        };
        let ModelInstructionKind::Opaque(command) = &instruction.kind else {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        };
        if command.arguments.len() != count {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        }
        let args = &command.arguments;
        let weight = if opcode == 0x26 {
            Some(
                values
                    .evaluate_packed(args[3], callback)
                    .map_err(ModelVertexError::Value)?,
            )
        } else {
            None
        };
        self.execute_pair_inner(
            ModelPairVertexRequest {
                command: ModelVertexCommand::RendererWord(instruction.raw_command),
                destination: args[0] as i16,
                sources: [args[1] as i16, args[2] as i16],
                operation: match weight {
                    Some(weight) => ModelPairOperation::Interpolate { weight },
                    None => ModelPairOperation::Midpoint,
                },
            },
            projector,
        )
    }

    pub(crate) fn execute_pair<P: ModelVertexProjector>(
        &mut self,
        request: ModelPairVertexRequest,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = self.execute_pair_inner(request, projector);
        self.finish(result)
    }

    fn execute_pair_inner<E, P: ModelVertexProjector>(
        &mut self,
        request: ModelPairVertexRequest,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<E, P::Error>> {
        let (renderer, auxiliary) = match request.operation {
            ModelPairOperation::Midpoint => (0x23, 0x18),
            ModelPairOperation::Interpolate { .. } => (0x26, 0x2f),
            ModelPairOperation::OriginOffset { .. } => (0x25, 0x2c),
        };
        match request.command {
            ModelVertexCommand::RendererWord(word) if word as u8 == renderer => {}
            ModelVertexCommand::AuxiliaryOpcode(opcode) if opcode == auxiliary => {}
            _ => return Err(ModelVertexFault::InvalidInstruction { opcode: renderer }.into()),
        }
        let destination = i32::from(request.destination);
        let sources = request.sources.map(i32::from);
        let mut mode = (request.command.native_value() >> 8) & 15;
        let mut parity = 0;
        while mode != 0 {
            if mode & 3 != 0 {
                let target = self.checked_index(destination + parity)?;
                let left = self.read_world(sources[0] ^ parity)?;
                let right = self.read_world(sources[1] ^ parity)?;
                let result = std::array::from_fn(|component| match request.operation {
                    ModelPairOperation::Interpolate { weight } => {
                        interpolate(left[component], right[component], weight)
                    }
                    ModelPairOperation::Midpoint => {
                        left[component].wrapping_add(right[component]) / 2
                    }
                    ModelPairOperation::OriginOffset { origin } => left[component]
                        .wrapping_add(right[component])
                        .wrapping_sub(origin[component]),
                });
                let twin = self.twin_from(
                    [sources[0] ^ parity, sources[1] ^ parity],
                    |[left, right], origin| {
                        std::array::from_fn(|i| match request.operation {
                            ModelPairOperation::Interpolate { weight } => {
                                exact::interpolate(left[i], right[i], weight)
                            }
                            ModelPairOperation::Midpoint => (left[i] + right[i]) / 2.0,
                            ModelPairOperation::OriginOffset { .. } => {
                                left[i] + right[i] - origin[i]
                            }
                        })
                    },
                );
                if mode & 1 != 0 {
                    self.write_world(target, result)?;
                    self.set_twin(target, twin);
                }
                if mode & 2 != 0 {
                    self.project_twin(target, ModelVertexInput::World(result), twin, projector)?;
                }
            }
            mode >>= 2;
            parity = 1;
        }
        Ok(())
    }

    fn project<C, P: ModelVertexProjector>(
        &mut self,
        index: usize,
        input: ModelVertexInput,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<C, P::Error>> {
        self.project_with_depth_bias(index, input, None, projector, 0)
    }

    /// [`Self::project`] of a world point whose exact twin is `twin`.
    fn project_twin<C, P: ModelVertexProjector>(
        &mut self,
        index: usize,
        input: ModelVertexInput,
        twin: Option<[f64; 3]>,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<C, P::Error>> {
        self.project_with_depth_bias(index, input, twin, projector, 0)
    }

    /// The twin of a callback's point over source slot `source`: across
    /// it follows the source's twin, so a wheel set on the ground stays
    /// under its hull; its height stays the callback's whole units.
    fn ground_twin(&self, source: i32, point: [i32; 3]) -> Option<[f64; 3]> {
        let twin = self.twin(source)?;
        let whole = self.read_world(source).ok()?;
        Some([
            f64::from(point[0]) + twin[0] - f64::from(whole[0]),
            f64::from(point[1]),
            f64::from(point[2]) + twin[2] - f64::from(whole[2]),
        ])
    }

    fn project_with_depth_bias<C, P: ModelVertexProjector>(
        &mut self,
        index: usize,
        input: ModelVertexInput,
        twin: Option<[f64; 3]>,
        projector: &mut P,
        bias: u32,
    ) -> Result<(), ModelVertexError<C, P::Error>> {
        // Reacquire current tail before the call; no RefCell borrow is retained
        // while a projector or callback runs. Pure owned projector changes are
        // committed bytewise on either return, without replacing padding.
        self.residue_tail.remove(&index);
        self.views[index] = None;
        self.refresh_projection(index)?;
        let before = self.slots[index].projection.bytes;
        let result = projector.project(input, &mut self.slots[index].projection);
        self.commit_projection_changes(index, before)?;
        let value = result.map_err(ModelVertexError::Projection)?;
        self.views[index] = projector.view_point(input);
        if let Some(exact) = self.exact.as_deref_mut() {
            let from = match input {
                ModelVertexInput::Local(_) => exact.origin,
                ModelVertexInput::World(world) => twin.unwrap_or(world.map(f64::from)),
            };
            exact.views[index] = projector.exact_view_point(input, from);
            let screen = exact.views[index].and_then(|view| projector.exact_screen_point(view));
            if let (Some([x, y]), Some(xy)) = (screen, self.slots[index].projection.read_u32(0)) {
                std::rc::Rc::make_mut(&mut exact.screens).insert(xy, [x as f32, y as f32]);
            }
        }
        self.grounded.remove(&index);
        if bias == 0 {
            self.biased.remove(&index);
        } else {
            self.biased.insert(index);
        }
        // The native caller's EAX store is a full DWORD and occurs only after
        // normal projector return (004EC642 additionally subtracts20).
        self.write_projection_depth(index, value.wrapping_sub(bias))?;
        Ok(())
    }

    fn refresh_projection(&mut self, index: usize) -> Result<(), ModelVertexFault> {
        if let ModelVertexStorage::ProcessView {
            scratch,
            allocation,
        } = &self.storage
        {
            self.slots[index].projection.bytes =
                scratch.read_vertex::<12>(allocation, index as u16, 12)?;
        }
        Ok(())
    }

    fn commit_projection_changes(
        &self,
        index: usize,
        before: [Option<u8>; 12],
    ) -> Result<(), ModelVertexFault> {
        if let ModelVertexStorage::ProcessView {
            scratch,
            allocation,
        } = &self.storage
        {
            for (offset, old) in before.into_iter().enumerate() {
                let current = self.slots[index].projection.bytes[offset];
                if current != old {
                    scratch.write_vertex::<1>(allocation, index as u16, 12 + offset, [current])?;
                }
            }
        }
        Ok(())
    }

    fn write_projection_depth(&mut self, index: usize, value: u32) -> Result<(), ModelVertexFault> {
        if let ModelVertexStorage::ProcessView {
            scratch,
            allocation,
        } = &self.storage
        {
            scratch.write_vertex::<4>(
                allocation,
                index as u16,
                16,
                value.to_le_bytes().map(Some),
            )?;
        }
        self.slots[index].projection.write_u32(4, value);
        Ok(())
    }

    /// CustomVertexStackResidueApproximation. 0043A2D0's default arm returns
    /// its own uninitialized stack locals: residue of earlier calls at that
    /// depth, which changes every frame (BRIEFING_RUNTIME.md). The fields
    /// native writes from that residue become unknown here: world XYZ for
    /// mode bit 1, the projected tail for bit 2. Primitives skip a vertex
    /// whose clip byte is unknown, as they skip any unprojected dynamic slot.
    fn forget_stack_residue_vertex(
        &mut self,
        index: usize,
        mode: u16,
    ) -> Result<(), ModelVertexFault> {
        match &self.storage {
            ModelVertexStorage::IsolatedFixture => {
                if mode & 1 != 0 {
                    self.slots[index].world = None;
                }
                if mode & 2 != 0 {
                    self.slots[index].projection.bytes = [None; 12];
                }
            }
            ModelVertexStorage::ProcessView {
                scratch,
                allocation,
            } => {
                if mode & 1 != 0 {
                    scratch.write_vertex::<12>(allocation, index as u16, 0, [None; 12])?;
                }
                if mode & 2 != 0 {
                    scratch.write_vertex::<12>(allocation, index as u16, 12, [None; 12])?;
                }
                self.slots[index] =
                    decode_slot(scratch.read_vertex::<24>(allocation, index as u16, 0)?);
            }
        }
        if let Some(exact) = self.exact.as_deref_mut() {
            if mode & 1 != 0 {
                exact.worlds[index] = None;
            }
            if mode & 2 != 0 {
                exact.views[index] = None;
            }
        }
        if mode & 1 != 0 {
            self.residue_world.insert(index);
        }
        if mode & 2 != 0 {
            self.residue_tail.insert(index);
            self.views[index] = None;
            self.biased.remove(&index);
            self.grounded.remove(&index);
        }
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("CustomVertexStackResidueApproximation: 0043A2D0's default arm returns stack residue; its vertex stays unknown and the primitives, depth-group key and child anchor that read it are skipped or sorted farthest (MODEL_RUNTIME.md)");
        }
        Ok(())
    }

    fn write_world(&mut self, index: usize, world: [i32; 3]) -> Result<(), ModelVertexFault> {
        self.residue_world.remove(&index);
        // A writer with a twin sets it after; the others leave whole units.
        self.set_twin(index, None);
        match &self.storage {
            ModelVertexStorage::IsolatedFixture => self.slots[index].world = Some(world),
            ModelVertexStorage::ProcessView {
                scratch,
                allocation,
            } => {
                // Each native axis is a distinct DWORD. Refresh after each
                // successful store so any later custody failure retains prefix.
                // The slot is read once; each store then lands in that copy.
                let mut raw = scratch.read_vertex::<24>(allocation, index as u16, 0)?;
                for (axis, value) in world.into_iter().enumerate() {
                    let bytes = value.to_le_bytes().map(Some);
                    scratch.write_vertex::<4>(allocation, index as u16, axis * 4, bytes)?;
                    raw[axis * 4..axis * 4 + 4].copy_from_slice(&bytes);
                    self.slots[index] = decode_slot(raw);
                }
            }
        }
        Ok(())
    }

    fn raw_slot(&self, index: usize) -> Result<[Option<u8>; 24], ModelVertexFault> {
        match &self.storage {
            ModelVertexStorage::IsolatedFixture => Ok(encode_slot(self.slots[index])),
            ModelVertexStorage::ProcessView {
                scratch,
                allocation,
            } => Ok(scratch.read_vertex::<24>(allocation, index as u16, 0)?),
        }
    }

    fn copy_parent_slot(
        &mut self,
        to: usize,
        parent: &Self,
        from: usize,
    ) -> Result<(), ModelVertexFault> {
        // Copy actual24 bytes to a local value before taking any destination
        // borrow. Parent ProcessView clones retain the same live registration.
        let raw = parent.raw_slot(from)?;
        if let ModelVertexStorage::ProcessView {
            scratch,
            allocation,
        } = &self.storage
        {
            scratch.write_vertex::<24>(allocation, to as u16, 0, raw)?;
        }
        self.slots[to] = decode_slot(raw);
        self.views[to] = parent.views.get(from).copied().flatten();
        if let Some(exact) = self.exact.as_deref_mut() {
            let parent = parent.exact.as_deref();
            exact.worlds[to] = parent.and_then(|parent| parent.worlds.get(from).copied().flatten());
            exact.views[to] = parent.and_then(|parent| parent.views.get(from).copied().flatten());
        }
        for (set, parent_set) in [
            (&mut self.residue_tail, &parent.residue_tail),
            (&mut self.residue_world, &parent.residue_world),
            (&mut self.biased, &parent.biased),
            (&mut self.grounded, &parent.grounded),
        ] {
            if parent_set.contains(&from) {
                set.insert(to);
            } else {
                set.remove(&to);
            }
        }
        Ok(())
    }

    fn checked_index(&self, index: i32) -> Result<usize, ModelVertexFault> {
        usize::try_from(index)
            .ok()
            .filter(|index| *index < self.slots.len())
            .ok_or(ModelVertexFault::InvalidSlot {
                index,
                count: self.slots.len(),
            })
    }

    fn read_world(&self, index: i32) -> Result<[i32; 3], ModelVertexFault> {
        let slot = self.checked_index(index)?;
        let world = match &self.storage {
            ModelVertexStorage::IsolatedFixture => self.slots[slot].world,
            ModelVertexStorage::ProcessView {
                scratch,
                allocation,
            } => decode_world(scratch.read_vertex::<12>(allocation, slot as u16, 0)?),
        };
        world.ok_or(ModelVertexFault::UnknownWorld { index })
    }

    fn ensure_running(&self) -> Result<(), ModelVertexFault> {
        if self.stopped {
            Err(ModelVertexFault::Stopped)
        } else {
            Ok(())
        }
    }

    fn finish<C, P>(
        &mut self,
        result: Result<(), ModelVertexError<C, P>>,
    ) -> Result<(), ModelVertexError<C, P>> {
        if result.is_err() {
            self.stopped = true;
        }
        result
    }

    /// Opcode 2A (`004ED350`): parent-slot copy engine. Bits 8-9 copy
    /// one parent slot into the destination slot; bits 10-11 copy the
    /// mirror-indexed parent slot (`^1`) into destination + 1. Each
    /// copy moves the full 24-byte slot (world triple plus the
    /// 12-byte projection tail). The parent index resolves through
    /// the invoker's word table, which the port reads as the child
    /// frame's own state words (proven for grandchild 554: table
    /// `[14,12,10]` equals 553's `0x0E` state words; single-case
    /// evidence, fail closed outside it). A clear mode word is an
    /// exact no-op. Without a parent the destination takes the
    /// published origin and its projected local zero point instead
    /// (see `execute_root_2a`). `mirror` picks the mirror copy's parent
    /// slot (see [`MirrorCopySource`]).
    pub fn execute_copy_2a<P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        state_words: &[u16],
        parent: Option<&ModelVertices>,
        mirror: MirrorCopySource,
        origin: [i32; 3],
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result =
            self.execute_copy_2a_inner(instruction, state_words, parent, mirror, origin, projector);
        self.finish(result)
    }

    fn execute_copy_2a_inner<P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        state_words: &[u16],
        parent: Option<&ModelVertices>,
        mirror: MirrorCopySource,
        origin: [i32; 3],
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        let opcode = instruction.raw_command as u8;
        if opcode != 0x2a {
            return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into());
        }
        let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        };
        if command.arguments.len() != 2 {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        }
        let raw = instruction.raw_command;
        if raw & 0x0f00 == 0 {
            return Ok(());
        }
        let destination = i32::from(command.arguments[0] as i16);
        let Some(parent) = parent else {
            return self.execute_root_2a(raw, destination, origin, projector);
        };
        let reference = i32::from(command.arguments[1] as i16);
        let table_index = usize::try_from(reference)
            .ok()
            .filter(|index| *index < state_words.len())
            .ok_or(ModelVertexFault::InvalidInstruction { opcode })?;
        let parent_slot = i32::from(state_words[table_index] as i16);
        let fault = |error| ModelVertexError::Fault(error);
        if raw & 0x0300 != 0 {
            let from = parent.checked_index(parent_slot).map_err(fault)?;
            let to = self.checked_index(destination).map_err(fault)?;
            self.copy_parent_slot(to, parent, from).map_err(fault)?;
        }
        if raw & 0x0c00 != 0 {
            let source = match mirror {
                MirrorCopySource::MirrorSide if raw & 0x0300 == 0 => parent_slot | 1,
                _ => parent_slot ^ 1,
            };
            let from = parent.checked_index(source).map_err(fault)?;
            let to = self.checked_index(destination + 1).map_err(fault)?;
            self.copy_parent_slot(to, parent, from).map_err(fault)?;
        }
        Ok(())
    }

    /// 004ED3CD, the branch for a context without a parent. World bits
    /// 8/10 store the published origin in the destination (00490900 turns
    /// the zero vector there, then 0055F1C4..CC is added); bit 10 then
    /// copies those 12 bytes to destination + 1. Projection bits 9/11
    /// project the local zero point through context+3C -> +1C into the
    /// destination; bit 11 then copies its whole 12-byte tail to
    /// destination + 1. Mirror-only bits therefore write the destination
    /// first.
    fn execute_root_2a<P: ModelVertexProjector>(
        &mut self,
        raw: u16,
        destination: i32,
        origin: [i32; 3],
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        let to = self.checked_index(destination)?;
        let twin = self.exact.as_deref().map(|exact| exact.origin);
        if raw & 0x0500 != 0 {
            self.write_world(to, origin)?;
            self.set_twin(to, twin);
            if raw & 0x0400 != 0 {
                let mirror = self.checked_index(destination + 1)?;
                self.write_world(mirror, origin)?;
                self.set_twin(mirror, twin);
            }
        }
        if raw & 0x0a00 != 0 {
            self.project(to, ModelVertexInput::Local([0; 3]), projector)?;
            if raw & 0x0800 != 0 {
                let mirror = self.checked_index(destination + 1)?;
                self.copy_projection_tail(to, mirror)?;
            }
        }
        Ok(())
    }

    fn copy_projection_tail(&mut self, from: usize, to: usize) -> Result<(), ModelVertexFault> {
        let raw = self.raw_slot(from)?;
        let tail: [Option<u8>; 12] = raw[12..].try_into().expect("12-byte projection tail");
        if let ModelVertexStorage::ProcessView {
            scratch,
            allocation,
        } = &self.storage
        {
            scratch.write_vertex::<12>(allocation, to as u16, 12, tail)?;
        }
        self.slots[to].projection.bytes = tail;
        self.views[to] = self.views[from];
        if let Some(exact) = self.exact.as_deref_mut() {
            exact.views[to] = exact.views[from];
        }
        for set in [&mut self.residue_tail, &mut self.biased, &mut self.grounded] {
            if set.contains(&from) {
                set.insert(to);
            } else {
                set.remove(&to);
            }
        }
        Ok(())
    }

    /// Opcode 22 (`004EC300`): jittered-vertex engine. The mode nibble
    /// runs up to two passes (low pair first), each over one
    /// destination slot (`args[0] + pass`) sourced from
    /// `args[1] ^ pass`. Every active pass draws the shared process
    /// RNG three times (x, y, z through `0048F060`, the
    /// `name_entry_rng_next` LCG) and adds
    /// `(draw << 16) >> (args[2] & 31)` to the source triple,
    /// wrapping. Bit 0 stores the jittered world; bit 1 projects it
    /// through the selected world entry (`context+3C -> +20`, the
    /// same virtual 23/26 use) and stores the depth DWORD. A clear
    /// mode nibble is an exact advance. Set bits without the process
    /// word stay fail-closed: the jitter path is an explicitly open
    /// owner until the caller threads it.
    pub fn execute_jitter_22<P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        rng: Option<&mut u32>,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = self.execute_jitter_22_inner(instruction, rng, projector);
        self.finish(result)
    }

    fn execute_jitter_22_inner<P: ModelVertexProjector>(
        &mut self,
        instruction: &ModelInstruction,
        rng: Option<&mut u32>,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        let opcode = instruction.raw_command as u8;
        if opcode != 0x22 {
            return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into());
        }
        let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        };
        if command.arguments.len() != 3 {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        }
        let mut mode = (instruction.raw_command >> 8) & 15;
        if mode == 0 {
            return Ok(());
        }
        let Some(rng) = rng else {
            return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into());
        };
        let destination = i32::from(command.arguments[0] as i16);
        let source = i32::from(command.arguments[1] as i16);
        let shift = u32::from(command.arguments[2] & 31);
        let mut parity = 0;
        while mode != 0 {
            if mode & 3 != 0 {
                let target = self.checked_index(destination + parity)?;
                let base = self.read_world(source ^ parity)?;
                let jittered: [i32; 3] = std::array::from_fn(|component| {
                    let draw = crate::name_entry::name_entry_rng_next(&mut *rng);
                    let jitter = (draw as i32).wrapping_shl(16).wrapping_shr(shift);
                    base[component].wrapping_add(jitter)
                });
                // The jitter stays whole; it follows its source's twin.
                let twin = self.twin(source ^ parity).map(|twin| {
                    std::array::from_fn(|i| twin[i] + f64::from(jittered[i].wrapping_sub(base[i])))
                });
                if mode & 1 != 0 {
                    self.write_world(target, jittered)?;
                    self.set_twin(target, twin);
                }
                if mode & 2 != 0 {
                    self.project_twin::<Infallible, P>(
                        target,
                        ModelVertexInput::World(jittered),
                        twin,
                        projector,
                    )?;
                }
            }
            mode >>= 2;
            parity = 1;
        }
        Ok(())
    }
}

fn decode_world(bytes: [Option<u8>; 12]) -> Option<[i32; 3]> {
    let axis = |at| {
        Some(i32::from_le_bytes([
            bytes[at]?,
            bytes[at + 1]?,
            bytes[at + 2]?,
            bytes[at + 3]?,
        ]))
    };
    Some([axis(0)?, axis(4)?, axis(8)?])
}

fn decode_slot(bytes: [Option<u8>; 24]) -> ModelVertexSlot {
    ModelVertexSlot {
        world: decode_world(std::array::from_fn(|i| bytes[i])),
        projection: ModelVertexProjection {
            bytes: std::array::from_fn(|i| bytes[12 + i]),
        },
    }
}

fn encode_slot(slot: ModelVertexSlot) -> [Option<u8>; 24] {
    let mut bytes = [None; 24];
    if let Some(world) = slot.world {
        for (axis, value) in world.into_iter().enumerate() {
            bytes[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes().map(Some));
        }
    }
    bytes[12..].copy_from_slice(&slot.projection.bytes);
    bytes
}

/// 00490900 uses the configured 53-bit x87 arithmetic, no f32 intermediates,
/// and direct nearest-even FISTP QWORD. It retains only the resulting low DWORD.
fn transform_world(context: &ModelContext, local: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|component| {
        let coefficient = |first| f64::from(f32::from_bits(context.basis.words[first + component]));
        let [x, y, z] = local.map(f64::from);
        let value = (z * coefficient(7) + y * coefficient(4)) + x * coefficient(1);
        fistp_low32(value).wrapping_add(context.position[component])
    })
}

fn interpolate(left: i32, right: i32, weight: u16) -> i32 {
    let delta = right.wrapping_sub(left);
    let product = i64::from(delta) * i64::from(i32::from(weight) << 14);
    // Signed IMUL EDX:EAX; SHL EAX,1; RCL EDX,1; LEA [left+EDX*2].
    // The first shift discards 31 low bits, then doubling retains even steps.
    left.wrapping_add(((product >> 31) as i32).wrapping_mul(2))
}

#[cfg(test)]
#[path = "model_vertices_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "model_vertices/combat_auxiliary_pair_tests.rs"]
mod combat_auxiliary_pair_tests;

#[cfg(test)]
#[path = "model_vertices/furniture_tests.rs"]
mod furniture_tests;

#[cfg(test)]
#[path = "model_vertices/copy_2a_tests.rs"]
mod copy_2a_tests;

#[cfg(test)]
#[path = "model_vertices/offset_25_tests.rs"]
mod offset_25_tests;

#[cfg(test)]
#[path = "model_vertices/shapes_tests.rs"]
mod shapes_tests;

#[cfg(test)]
#[path = "model_vertices/project_2c_tests.rs"]
mod project_2c_tests;

#[cfg(test)]
#[path = "model_vertices/surface_tests.rs"]
mod surface_tests;

mod world_stream;
pub(crate) use world_stream::ModelWorldStreamRequest;

#[cfg(test)]
#[path = "model_vertices/scratch_tests.rs"]
mod scratch_tests;
