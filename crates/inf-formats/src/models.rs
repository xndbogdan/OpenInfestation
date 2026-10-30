//! Handler-8 model resources.
//!
//! `FUN_0050F5A0` loads repeated size/count sub-blocks and walks variable-size
//! model entries. `FUN_004A00E0` then interprets each entry's wordcode through
//! two 256-entry render tables. This module preserves that disk structure and
//! decodes the proven command grammar without inventing an animated frame,
//! view state, or shadow pass.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion, HandlerRequest, ModelSubBlock, OverlayKey};

pub const MODEL_HANDLER: u8 = 8;
pub const MODEL_HEADER_BYTES: usize = 0x20;
pub const MODEL_MAX_PROGRAM_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelBank {
    pub overlay: OverlayKey,
    pub region_span: ByteSpan,
    pub blocks: Vec<ModelBlock>,
    pub entries: Vec<ModelEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelBlock {
    pub span: ByteSpan,
    pub payload: ByteSpan,
    pub first_entry: u16,
    pub entry_count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelEntry {
    pub identity: ModelIdentity,
    /// Complete aligned entry span, including padding.
    pub span: ByteSpan,
    /// Entry bytes through the optional name terminator, excluding padding.
    pub content_span: ByteSpan,
    pub header_span: ByteSpan,
    pub header: ModelHeader,
    pub auxiliary_span: ByteSpan,
    pub auxiliary: Vec<ModelAuxiliaryRecord>,
    /// Separate bytecode for FUN_004F2C50. Header +0E gates admission;
    /// inactive pools remain losslessly framed without pretending to execute.
    pub auxiliary_program: Option<crate::ModelAuxiliaryProgram>,
    pub vertices_span: ByteSpan,
    pub vertices: Vec<ModelVectorI16>,
    pub normals_span: ByteSpan,
    pub normals: Vec<ModelVectorI16>,
    pub command_span: ByteSpan,
    pub program: ModelProgram,
    pub name: Option<ModelName>,
    pub padding: ByteSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct ModelIdentity {
    pub local_index: u16,
    pub global_index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelName {
    /// Name bytes excluding the terminating NUL.
    pub bytes: ByteSpan,
    /// Complete name record including the terminating NUL.
    pub record: ByteSpan,
}

impl ModelName {
    pub fn raw<'a>(&self, source: &'a [u8]) -> Option<&'a [u8]> {
        self.bytes.get(source)
    }

    /// Retail names are ASCII, but the loader only requires a NUL-terminated
    /// byte string. Do not guess a text encoding for other inputs.
    pub fn ascii<'a>(&self, source: &'a [u8]) -> Option<&'a str> {
        let bytes = self.raw(source)?;
        bytes
            .is_ascii()
            .then(|| std::str::from_utf8(bytes).expect("ASCII is valid UTF-8"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModelHeader {
    /// Compared against the renderer's near/far visibility thresholds.
    pub visibility_extent: u32,
    pub command_word_count: u16,
    /// Auxiliary byte-span length in DWORD units, not an instruction count.
    pub auxiliary_count: u8,
    pub flags: ModelFlags,
    /// Number of 24-byte runtime vertex slots allocated by `FUN_004EBE60`.
    pub vertex_slot_count: u16,
    pub raw_vertex_count: u16,
    /// Number of four-byte runtime shade slots allocated by `FUN_004EBE60`.
    /// The raw normal-pool count is `(shade_slot_count - 2) / 2`.
    pub shade_slot_count: u16,
    /// FUN_004F2C50 admits its separate auxiliary program when this is nonzero.
    /// Visual initializer 0047A050 copies it to radius WORD +40; terrain furniture
    /// producer 004508E0 compares it as an unsigned radius. Auxiliary admission
    /// itself does not interpret that magnitude. Retain the lossless field name.
    pub raw_0e: u16,
    /// Raw-projection start passed to `004EBFE0`; zero throughout retail.
    /// The historical field name is retained in the lossless header schema.
    pub reserved_10: u16,
    /// Even, monotone preprocessing boundaries at offsets 12, 14, 16, 18.
    /// World XYZ uses [0,1,2]; projection uses reserved_10 plus [1,3].
    pub vertex_ranges: [u16; 4],
    /// Zero throughout the retail corpus, but not imposed by the loader.
    pub reserved_1a: u16,
    /// Even, monotone preprocessing boundaries at offsets 1c and 1e.
    pub shade_ranges: [u16; 2],
}

impl ModelHeader {
    pub const fn raw_normal_count(self) -> usize {
        (self.shade_slot_count.saturating_sub(2) / 2) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ModelFlags(u8);

impl ModelFlags {
    pub const FORCE_NEAR_TABLE: u8 = 0x20;
    pub const OMIT_NAME: u8 = 0x40;

    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn forces_near_table(self) -> bool {
        self.0 & Self::FORCE_NEAR_TABLE != 0
    }

    pub const fn omits_name(self) -> bool {
        self.0 & Self::OMIT_NAME != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModelAuxiliaryRecord {
    /// Lossless DWORD framing retained for exports; instruction boundaries
    /// belong to auxiliary_program and may cross these records.
    pub words: [u16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModelVectorI16 {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelProgram {
    /// Includes this program's terminating low-byte-zero word.
    pub span: ByteSpan,
    pub instructions: Vec<ModelInstruction>,
    pub terminator: ByteSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelInstruction {
    pub span: ByteSpan,
    pub raw_command: u16,
    pub kind: ModelInstructionKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ModelInstructionKind {
    Triangle(ModelTriangle),
    Quad(ModelQuad),
    TriangleFan(ModelTriangleFan),
    Edge(ModelEdge),
    ModelInstance(ModelInstance),
    NestedProgram(ModelNestedProgram),
    SentinelList(ModelSentinelList),
    VariableWords(ModelRawCommand),
    RegisterOperation(ModelUnresolvedCommand),
    UnresolvedDynamic(ModelUnresolvedCommand),
    Opaque(ModelRawCommand),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelTriangle {
    pub material: ModelMaterialRef,
    pub vertices: [ModelSlotRef; 3],
    pub lighting: ModelTriangleLighting,
    pub flags: ModelFaceFlags,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "refs", rename_all = "snake_case")]
pub enum ModelTriangleLighting {
    Shared(ModelSlotRef),
    PerVertex([ModelSlotRef; 3]),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelQuad {
    pub material: ModelMaterialRef,
    pub vertices: [ModelSlotRef; 4],
    pub lighting: ModelQuadLighting,
    pub flags: ModelFaceFlags,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "refs", rename_all = "snake_case")]
pub enum ModelQuadLighting {
    Shared(ModelSlotRef),
    PerVertex([ModelSlotRef; 4]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModelMaterialRef {
    pub domain: ModelMaterialDomain,
    /// Global runtime resource ID encoded directly in the Handler-8 command.
    ///
    /// `FUN_004EFC00` maps each handler-local payload entry into its global
    /// handler table when the OVL is loaded. The face handlers do not apply a
    /// second local-to-global translation: they reject negative IDs and IDs
    /// beyond the selected handler table's capacity, then index that table
    /// directly. A negative value therefore represents a missing resource.
    pub global_id: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMaterialDomain {
    /// Handler 3 logical sprite/texture resource.
    Handler3Texture,
    /// Handler 7 one-u32 material resource.
    Handler7Material,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ModelSlotRef(u16);

impl ModelSlotRef {
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn mirrored(self) -> Self {
        Self(self.0 ^ 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ModelFaceFlags(u16);

impl ModelFaceFlags {
    pub const CULL_ENABLE: u16 = 0x0100;
    pub const CULL_DIRECTION: u16 = 0x0200;
    pub const MIRROR_INSTANCE: u16 = 0x0400;

    pub const fn from_command(command: u16) -> Self {
        Self(command & 0xff00)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn cull_enabled(self) -> bool {
        self.0 & Self::CULL_ENABLE != 0
    }

    pub const fn reverse_cull_direction(self) -> bool {
        self.0 & Self::CULL_DIRECTION != 0
    }

    pub const fn emits_mirror_instance(self) -> bool {
        self.0 & Self::MIRROR_INSTANCE != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelTriangleFan {
    pub flags: ModelFanFlags,
    pub material: ModelMaterialRef,
    pub vertices: Vec<ModelFanVertex>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ModelFanFlags(u16);

impl ModelFanFlags {
    /// Reverse the projected-area test used for fan culling.
    pub const CULL_DIRECTION: u16 = 0x0001;
    /// Test the first triangle and terminate the fan when it is hidden. When
    /// combined with per-triangle culling, any later rejection also terminates
    /// the remaining fan.
    pub const CULL_FROM_FIRST_TRIANGLE: u16 = 0x0002;
    /// Test every triangle independently instead of gating the complete fan.
    pub const CULL_EACH_TRIANGLE: u16 = 0x0004;
    pub const TEXTURED_RECORDS: u16 = 0x0008;
    pub const MIRROR_INSTANCE: u16 = 0x0010;

    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn has_textured_records(self) -> bool {
        self.0 & Self::TEXTURED_RECORDS != 0
    }

    pub const fn reverse_cull_direction(self) -> bool {
        self.0 & Self::CULL_DIRECTION != 0
    }

    pub const fn culls_from_first_triangle(self) -> bool {
        self.0 & Self::CULL_FROM_FIRST_TRIANGLE != 0
    }

    pub const fn culls_each_triangle(self) -> bool {
        self.0 & Self::CULL_EACH_TRIANGLE != 0
    }

    pub const fn emits_mirror_instance(self) -> bool {
        self.0 & Self::MIRROR_INSTANCE != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelFanVertex {
    pub vertex: ModelSlotRef,
    pub lighting_ref: ModelSlotRef,
    pub attributes: ModelFanVertexAttributes,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelFanVertexAttributes {
    Untextured {
        lighting_and_end: [u16; 2],
    },
    Textured {
        /// Unsigned Q0.16 UV values. `FUN_004B4660` multiplies these words by
        /// the selected Handler-3 texture width/height to produce 16.16 texel
        /// coordinates without flipping either axis.
        uv_words: [u16; 2],
        lighting_and_end: [u16; 2],
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelEdge {
    pub edge_type: u16,
    pub parameter: u16,
    pub vertices: [ModelSlotRef; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelInstance {
    pub orientation: ModelOrientation,
    pub model: ModelResourceRef,
    /// Byte count measured from the first argument, exactly as opcode 0e
    /// advances its argument pointer.
    pub argument_bytes: u16,
    pub state_words: Vec<u16>,
    pub dependencies: Vec<ModelDynamicDependency>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ModelResourceRef(i16);

impl ModelResourceRef {
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw as i16)
    }

    pub const fn raw(self) -> i16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ModelOrientation(u16);

impl ModelOrientation {
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn uses_saved_basis(self) -> bool {
        self.0 & 7 == 6
    }

    /// Source axis assigned to each destination axis. Opcode value 6 selects
    /// a separately saved runtime basis and therefore has no fixed mapping.
    pub const fn destination_sources(self) -> Option<[ModelAxis; 3]> {
        match self.0 & 7 {
            0 | 7 => Some([ModelAxis::X, ModelAxis::Y, ModelAxis::Z]),
            1 => Some([ModelAxis::Z, ModelAxis::X, ModelAxis::Y]),
            2 => Some([ModelAxis::Y, ModelAxis::Z, ModelAxis::X]),
            3 => Some([ModelAxis::Z, ModelAxis::Y, ModelAxis::X]),
            4 => Some([ModelAxis::Y, ModelAxis::X, ModelAxis::Z]),
            5 => Some([ModelAxis::X, ModelAxis::Z, ModelAxis::Y]),
            6 => None,
            _ => unreachable!(),
        }
    }

    /// Signs applied to source X, Y, and Z before permutation.
    pub const fn source_signs(self) -> [i8; 3] {
        [
            if self.0 & 0x20 == 0 { 1 } else { -1 },
            if self.0 & 0x10 == 0 { 1 } else { -1 },
            if self.0 & 0x08 == 0 { 1 } else { -1 },
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelAxis {
    X,
    Y,
    Z,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelNestedProgram {
    pub kind: ModelNestedKind,
    pub arguments: Vec<u16>,
    pub program: Box<ModelProgram>,
    pub dependencies: Vec<ModelDynamicDependency>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelNestedKind {
    Transform,
    OrientedTransform,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelSentinelList {
    pub values: Vec<u16>,
    pub terminator: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelRawCommand {
    pub arguments: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelUnresolvedCommand {
    pub arguments: Vec<u16>,
    pub dependencies: Vec<ModelDynamicDependency>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDynamicDependency {
    ViewPlane,
    CameraDistance,
    PackedValueCallback,
    RegisterFile,
    RandomSource,
    ParentVertexSlots,
    RendererProjection,
    CustomVertexCallback,
    TransformState,
    ExternalModelCatalog,
    SavedTransformBasis,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelError {
    #[error(transparent)]
    Auxiliary(#[from] crate::ModelAuxiliaryError),
    #[error("model parser requires handler 8, received handler {0}")]
    WrongHandler(u8),

    #[error("handler 8 requires model-sub-block framing")]
    WrongFraming,

    #[error("handler 8 request count {request} does not match framed count {framed}")]
    RequestCountMismatch { request: i16, framed: u32 },

    #[error("handler 8 span {span:?} is outside the {input_len}-byte OVL")]
    SpanOutOfBounds { span: ByteSpan, input_len: usize },

    #[error("model entry {entry} header at byte {offset:#x} is truncated")]
    TruncatedHeader { entry: u16, offset: usize },

    #[error(
        "model entry {entry} at byte {offset:#x} has invalid shade-slot count {count}; expected an even value of at least 2"
    )]
    InvalidShadeSlotCount {
        entry: u16,
        offset: usize,
        count: u16,
    },

    #[error("model entry {entry} at byte {offset:#x} has invalid vertex preprocessing ranges {ranges:?} for {slots} slots")]
    InvalidVertexRanges {
        entry: u16,
        offset: usize,
        ranges: [u16; 4],
        slots: u16,
    },

    #[error("model entry {entry} at byte {offset:#x} has invalid shade preprocessing ranges {ranges:?} for {slots} slots")]
    InvalidShadeRanges {
        entry: u16,
        offset: usize,
        ranges: [u16; 2],
        slots: u16,
    },

    #[error("integer overflow at byte {offset:#x} while computing {context}")]
    IntegerOverflow {
        offset: usize,
        context: &'static str,
    },

    #[error("model entry {entry} at byte {offset:#x} needs {needed} bytes, but its sub-block has {available}")]
    TruncatedEntry {
        entry: u16,
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error(
        "named model entry {entry} at byte {offset:#x} has no NUL terminator in its sub-block"
    )]
    UnterminatedName { entry: u16, offset: usize },

    #[error("model sub-block at byte {block_offset:#x} has {remaining} trailing bytes after its declared entries")]
    TrailingBlockBytes {
        block_offset: usize,
        remaining: usize,
    },

    #[error("model local index {local_index} has no global handler-8 mapping")]
    MissingGlobalMapping { local_index: u16 },

    #[error("model entry count exceeds the on-disk u16 index range")]
    TooManyEntries,

    #[error("model command {opcode:#04x} at byte {offset:#x} needs {needed} argument words, only {remaining} remain")]
    TruncatedCommand {
        offset: usize,
        opcode: u8,
        needed: usize,
        remaining: usize,
    },

    #[error("model program starting at byte {program_offset:#x} has no low-byte-zero terminator before byte {offset:#x}")]
    MissingProgramTerminator {
        program_offset: usize,
        offset: usize,
    },

    #[error("model command nesting exceeds {max_depth} levels at byte {offset:#x}")]
    ProgramTooDeep { offset: usize, max_depth: usize },

    #[error("unassigned model opcode {opcode:#04x} at byte {offset:#x}")]
    UnknownOpcode { offset: usize, opcode: u8 },

    #[error("model program terminates at byte {offset:#x} with {remaining} trailing words")]
    TrailingProgramWords { offset: usize, remaining: usize },

    #[error("triangle-fan command at byte {offset:#x} terminates after {records} records; the runtime requires at least 3")]
    FanTooShort { offset: usize, records: usize },

    #[error("triangle-fan command at byte {offset:#x} has no record end marker")]
    UnterminatedFan { offset: usize },

    #[error("sentinel-list opcode {opcode:#04x} at byte {offset:#x} has no FFFF terminator")]
    UnterminatedSentinelList { offset: usize, opcode: u8 },

    #[error(
        "model-instance command at byte {offset:#x} has invalid argument byte count {byte_len}"
    )]
    InvalidInstanceLength { offset: usize, byte_len: u16 },
}

/// Parse one fully framed Handler-8 region and resolve every entry through the
/// signed PreLoad resource mapping used by `FUN_004EFC00`.
pub fn parse_model_bank(
    source: &[u8],
    region: &HandlerRegion,
    request: &HandlerRequest,
    overlay: &OverlayKey,
) -> Result<ModelBank, ModelError> {
    if region.handler.get() != MODEL_HANDLER {
        return Err(ModelError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::ModelSubBlocks {
        total_entries,
        blocks,
    } = &region.framing
    else {
        return Err(ModelError::WrongFraming);
    };
    let request_count =
        u32::try_from(request.count).map_err(|_| ModelError::RequestCountMismatch {
            request: request.count,
            framed: *total_entries,
        })?;
    if request_count != region.requested_count || request_count != *total_entries {
        return Err(ModelError::RequestCountMismatch {
            request: request.count,
            framed: *total_entries,
        });
    }

    let mut parsed_blocks = Vec::with_capacity(blocks.len());
    let mut entries = Vec::with_capacity(request_count as usize);
    for block in blocks {
        let first_entry = u16::try_from(entries.len()).map_err(|_| ModelError::TooManyEntries)?;
        parse_block(source, block, request, &mut entries)?;
        let parsed_count = u16::try_from(entries.len() - usize::from(first_entry))
            .map_err(|_| ModelError::TooManyEntries)?;
        debug_assert_eq!(parsed_count, block.entry_count);
        parsed_blocks.push(ModelBlock {
            span: block.span,
            payload: block.payload,
            first_entry,
            entry_count: parsed_count,
        });
    }
    if entries.len() != request_count as usize {
        return Err(ModelError::RequestCountMismatch {
            request: request.count,
            framed: entries.len() as u32,
        });
    }

    Ok(ModelBank {
        overlay: overlay.clone(),
        region_span: region.span,
        blocks: parsed_blocks,
        entries,
    })
}

fn parse_block(
    source: &[u8],
    block: &ModelSubBlock,
    request: &HandlerRequest,
    entries: &mut Vec<ModelEntry>,
) -> Result<(), ModelError> {
    let payload = block
        .payload
        .get(source)
        .ok_or(ModelError::SpanOutOfBounds {
            span: block.payload,
            input_len: source.len(),
        })?;
    let mut cursor = 0usize;
    for _ in 0..block.entry_count {
        let local_index = u16::try_from(entries.len()).map_err(|_| ModelError::TooManyEntries)?;
        let entry_offset = block.payload.start + cursor;
        let header_end =
            cursor
                .checked_add(MODEL_HEADER_BYTES)
                .ok_or(ModelError::IntegerOverflow {
                    offset: entry_offset,
                    context: "model header end",
                })?;
        let Some(header_bytes) = payload.get(cursor..header_end) else {
            return Err(ModelError::TruncatedHeader {
                entry: local_index,
                offset: entry_offset,
            });
        };
        let header = parse_header(header_bytes);
        validate_header(header, local_index, entry_offset)?;

        let auxiliary_bytes = usize::from(header.auxiliary_count).checked_mul(4).ok_or(
            ModelError::IntegerOverflow {
                offset: entry_offset,
                context: "model auxiliary byte count",
            },
        )?;
        let vertex_bytes = usize::from(header.raw_vertex_count).checked_mul(6).ok_or(
            ModelError::IntegerOverflow {
                offset: entry_offset,
                context: "model raw vertex byte count",
            },
        )?;
        let normal_bytes =
            header
                .raw_normal_count()
                .checked_mul(6)
                .ok_or(ModelError::IntegerOverflow {
                    offset: entry_offset,
                    context: "model raw normal byte count",
                })?;
        let command_bytes = usize::from(header.command_word_count)
            .checked_mul(2)
            .ok_or(ModelError::IntegerOverflow {
                offset: entry_offset,
                context: "model command byte count",
            })?;
        let content_bytes = MODEL_HEADER_BYTES
            .checked_add(auxiliary_bytes)
            .and_then(|value| value.checked_add(vertex_bytes))
            .and_then(|value| value.checked_add(normal_bytes))
            .and_then(|value| value.checked_add(command_bytes))
            .ok_or(ModelError::IntegerOverflow {
                offset: entry_offset,
                context: "model fixed entry byte count",
            })?;
        let fixed_end = cursor
            .checked_add(content_bytes)
            .ok_or(ModelError::IntegerOverflow {
                offset: entry_offset,
                context: "model fixed entry end",
            })?;
        if fixed_end > payload.len() {
            return Err(ModelError::TruncatedEntry {
                entry: local_index,
                offset: entry_offset,
                needed: content_bytes,
                available: payload.len().saturating_sub(cursor),
            });
        }

        let auxiliary_start = header_end;
        let vertices_start = auxiliary_start + auxiliary_bytes;
        let normals_start = vertices_start + vertex_bytes;
        let commands_start = normals_start + normal_bytes;
        let header_span = absolute_span(block.payload.start, cursor, header_end);
        let auxiliary_span = absolute_span(block.payload.start, auxiliary_start, vertices_start);
        let vertices_span = absolute_span(block.payload.start, vertices_start, normals_start);
        let normals_span = absolute_span(block.payload.start, normals_start, commands_start);
        let command_span = absolute_span(block.payload.start, commands_start, fixed_end);

        let auxiliary = payload[auxiliary_start..vertices_start]
            .chunks_exact(4)
            .map(|record| ModelAuxiliaryRecord {
                words: [u16_at(record, 0), u16_at(record, 2)],
            })
            .collect();
        let auxiliary_program = if header.raw_0e != 0 {
            Some(crate::model_auxiliary::decode_model_auxiliary_at(
                &payload[auxiliary_start..vertices_start],
                auxiliary_span.start,
            )?)
        } else {
            None
        };
        let vertices = parse_vectors(&payload[vertices_start..normals_start]);
        let normals = parse_vectors(&payload[normals_start..commands_start]);
        let command_words = payload[commands_start..fixed_end]
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect::<Vec<_>>();
        let program = decode_model_program_at(&command_words, command_span.start)?;

        let (name, content_end) = if header.flags.omits_name() {
            (None, fixed_end)
        } else {
            let name_bytes = &payload[fixed_end..];
            let Some(name_len) = name_bytes.iter().position(|byte| *byte == 0) else {
                return Err(ModelError::UnterminatedName {
                    entry: local_index,
                    offset: block.payload.start + fixed_end,
                });
            };
            let end = fixed_end + name_len + 1;
            let bytes = absolute_span(block.payload.start, fixed_end, fixed_end + name_len);
            let record = absolute_span(block.payload.start, fixed_end, end);
            (Some(ModelName { bytes, record }), end)
        };
        let aligned_end = align_four(content_end, entry_offset)?;
        if aligned_end > payload.len() {
            return Err(ModelError::TruncatedEntry {
                entry: local_index,
                offset: entry_offset,
                needed: aligned_end - cursor,
                available: payload.len().saturating_sub(cursor),
            });
        }
        let global_index = request
            .global_index(local_index)
            .ok_or(ModelError::MissingGlobalMapping { local_index })?;
        entries.push(ModelEntry {
            identity: ModelIdentity {
                local_index,
                global_index,
            },
            span: absolute_span(block.payload.start, cursor, aligned_end),
            content_span: absolute_span(block.payload.start, cursor, content_end),
            header_span,
            header,
            auxiliary_span,
            auxiliary,
            auxiliary_program,
            vertices_span,
            vertices,
            normals_span,
            normals,
            command_span,
            program,
            name,
            padding: absolute_span(block.payload.start, content_end, aligned_end),
        });
        cursor = aligned_end;
    }

    if cursor != payload.len() {
        return Err(ModelError::TrailingBlockBytes {
            block_offset: block.span.start,
            remaining: payload.len() - cursor,
        });
    }
    Ok(())
}

fn parse_header(bytes: &[u8]) -> ModelHeader {
    ModelHeader {
        visibility_extent: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        command_word_count: u16_at(bytes, 0x04),
        auxiliary_count: bytes[0x06],
        flags: ModelFlags::from_raw(bytes[0x07]),
        vertex_slot_count: u16_at(bytes, 0x08),
        raw_vertex_count: u16_at(bytes, 0x0a),
        shade_slot_count: u16_at(bytes, 0x0c),
        raw_0e: u16_at(bytes, 0x0e),
        reserved_10: u16_at(bytes, 0x10),
        vertex_ranges: [
            u16_at(bytes, 0x12),
            u16_at(bytes, 0x14),
            u16_at(bytes, 0x16),
            u16_at(bytes, 0x18),
        ],
        reserved_1a: u16_at(bytes, 0x1a),
        shade_ranges: [u16_at(bytes, 0x1c), u16_at(bytes, 0x1e)],
    }
}

fn validate_header(header: ModelHeader, entry: u16, offset: usize) -> Result<(), ModelError> {
    if header.shade_slot_count < 2 || header.shade_slot_count % 2 != 0 {
        return Err(ModelError::InvalidShadeSlotCount {
            entry,
            offset,
            count: header.shade_slot_count,
        });
    }
    let [a, b, c, d] = header.vertex_ranges;
    if header.vertex_slot_count % 2 != 0
        || a % 2 != 0
        || b % 2 != 0
        || c % 2 != 0
        || d % 2 != 0
        || !(a <= b && b <= c && c <= d && d <= header.vertex_slot_count)
    {
        return Err(ModelError::InvalidVertexRanges {
            entry,
            offset,
            ranges: header.vertex_ranges,
            slots: header.vertex_slot_count,
        });
    }
    let [a, b] = header.shade_ranges;
    if a % 2 != 0 || b % 2 != 0 || !(a <= b && b <= header.shade_slot_count) {
        return Err(ModelError::InvalidShadeRanges {
            entry,
            offset,
            ranges: header.shade_ranges,
            slots: header.shade_slot_count,
        });
    }
    Ok(())
}

fn parse_vectors(bytes: &[u8]) -> Vec<ModelVectorI16> {
    bytes
        .chunks_exact(6)
        .map(|vector| ModelVectorI16 {
            x: i16::from_le_bytes([vector[0], vector[1]]),
            y: i16::from_le_bytes([vector[2], vector[3]]),
            z: i16::from_le_bytes([vector[4], vector[5]]),
        })
        .collect()
}

fn align_four(value: usize, offset: usize) -> Result<usize, ModelError> {
    value
        .checked_add(3)
        .map(|aligned| aligned & !3)
        .ok_or(ModelError::IntegerOverflow {
            offset,
            context: "aligned model entry end",
        })
}

const fn absolute_span(base: usize, start: usize, end: usize) -> ByteSpan {
    ByteSpan::new(base + start, base + end)
}

const fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// Decode a standalone command program. Returned spans are byte offsets from
/// the first supplied word.
pub fn decode_model_program(words: &[u16]) -> Result<ModelProgram, ModelError> {
    decode_model_program_at(words, 0)
}

fn decode_model_program_at(words: &[u16], byte_offset: usize) -> Result<ModelProgram, ModelError> {
    let mut cursor = 0usize;
    let program = decode_program(words, &mut cursor, byte_offset, 0)?;
    if cursor != words.len() {
        return Err(ModelError::TrailingProgramWords {
            offset: byte_offset + cursor * 2,
            remaining: words.len() - cursor,
        });
    }
    Ok(program)
}

fn decode_program(
    words: &[u16],
    cursor: &mut usize,
    byte_offset: usize,
    depth: usize,
) -> Result<ModelProgram, ModelError> {
    let start = *cursor;
    if depth >= MODEL_MAX_PROGRAM_DEPTH {
        return Err(ModelError::ProgramTooDeep {
            offset: byte_offset + start * 2,
            max_depth: MODEL_MAX_PROGRAM_DEPTH,
        });
    }
    let mut instructions = Vec::new();
    loop {
        let Some(&raw_command) = words.get(*cursor) else {
            return Err(ModelError::MissingProgramTerminator {
                program_offset: byte_offset + start * 2,
                offset: byte_offset + *cursor * 2,
            });
        };
        let opcode = raw_command as u8;
        if opcode == 0 {
            let terminator = word_span(byte_offset, *cursor, *cursor + 1);
            *cursor += 1;
            return Ok(ModelProgram {
                span: word_span(byte_offset, start, *cursor),
                instructions,
                terminator,
            });
        }

        let instruction_start = *cursor;
        *cursor += 1;
        let kind = decode_instruction(words, cursor, byte_offset, depth, raw_command)?;
        instructions.push(ModelInstruction {
            span: word_span(byte_offset, instruction_start, *cursor),
            raw_command,
            kind,
        });
    }
}

fn decode_instruction(
    words: &[u16],
    cursor: &mut usize,
    byte_offset: usize,
    depth: usize,
    raw_command: u16,
) -> Result<ModelInstructionKind, ModelError> {
    let opcode = raw_command as u8;
    match opcode {
        0x01 => decode_fan(words, cursor, byte_offset, raw_command),
        0x05 | 0x09 => {
            let argument_count = if opcode == 0x05 { 12 } else { 11 };
            let arguments = take_arguments(words, cursor, argument_count, opcode, byte_offset)?;
            let program = decode_program(words, cursor, byte_offset, depth + 1)?;
            let dependencies = if opcode == 0x05 {
                vec![
                    ModelDynamicDependency::TransformState,
                    ModelDynamicDependency::PackedValueCallback,
                    ModelDynamicDependency::RegisterFile,
                ]
            } else {
                // FUN_004EDB10 constructs its basis from three live vertex
                // slots. It does not decode packed values or read registers.
                vec![ModelDynamicDependency::TransformState]
            };
            Ok(ModelInstructionKind::NestedProgram(ModelNestedProgram {
                kind: if opcode == 0x05 {
                    ModelNestedKind::Transform
                } else {
                    ModelNestedKind::OrientedTransform
                },
                arguments,
                program: Box::new(program),
                dependencies,
            }))
        }
        0x06 | 0xe8 | 0xe9 => {
            let list_start = *cursor;
            while words.get(*cursor).is_some_and(|word| *word != u16::MAX) {
                *cursor += 1;
            }
            if *cursor == words.len() {
                return Err(ModelError::UnterminatedSentinelList {
                    offset: byte_offset + (list_start.saturating_sub(1)) * 2,
                    opcode,
                });
            }
            let values = words[list_start..*cursor].to_vec();
            *cursor += 1;
            Ok(ModelInstructionKind::SentinelList(ModelSentinelList {
                values,
                terminator: u16::MAX,
            }))
        }
        0x07 => {
            let arguments = take_arguments(words, cursor, 4, opcode, byte_offset)?;
            Ok(ModelInstructionKind::Edge(ModelEdge {
                edge_type: arguments[0],
                parameter: arguments[1],
                vertices: [
                    ModelSlotRef::from_raw(arguments[2]),
                    ModelSlotRef::from_raw(arguments[3]),
                ],
            }))
        }
        0x08 => {
            let count = usize::from(raw_command >> 8);
            let arguments = take_arguments(words, cursor, count, opcode, byte_offset)?;
            Ok(ModelInstructionKind::VariableWords(ModelRawCommand {
                arguments,
            }))
        }
        0x0e => decode_instance(words, cursor, byte_offset),
        0xd0 | 0xd2 | 0xd4 | 0xd6 | 0xd8 | 0xda | 0xdc | 0xde | 0xe0 | 0xe2 | 0xe4 | 0xe6 => {
            decode_face(words, cursor, byte_offset, raw_command)
        }
        0xf0..=0xff => {
            let arguments = take_arguments(words, cursor, 3, opcode, byte_offset)?;
            Ok(ModelInstructionKind::RegisterOperation(
                ModelUnresolvedCommand {
                    arguments,
                    dependencies: vec![
                        ModelDynamicDependency::RegisterFile,
                        ModelDynamicDependency::PackedValueCallback,
                    ],
                },
            ))
        }
        0x0b | 0x13 | 0x16 | 0x22 | 0x2a | 0x2b | 0x2c => {
            let argument_count = fixed_argument_count(opcode).expect("matched fixed opcode");
            let arguments = take_arguments(words, cursor, argument_count, opcode, byte_offset)?;
            Ok(ModelInstructionKind::UnresolvedDynamic(
                ModelUnresolvedCommand {
                    arguments,
                    dependencies: dynamic_dependencies(opcode),
                },
            ))
        }
        0x2d => {
            let argument_count = 2 + usize::from(raw_command >> 12);
            let arguments = take_arguments(words, cursor, argument_count, opcode, byte_offset)?;
            Ok(ModelInstructionKind::UnresolvedDynamic(
                ModelUnresolvedCommand {
                    arguments,
                    dependencies: vec![ModelDynamicDependency::CustomVertexCallback],
                },
            ))
        }
        _ => {
            // The dispatch table's unassigned commands run 004A0240, a jmp
            // to itself: where retail would hang, decoding stops here.
            let Some(argument_count) = fixed_argument_count(opcode) else {
                return Err(ModelError::UnknownOpcode {
                    offset: byte_offset + (*cursor - 1) * 2,
                    opcode,
                });
            };
            let arguments = take_arguments(words, cursor, argument_count, opcode, byte_offset)?;
            Ok(ModelInstructionKind::Opaque(ModelRawCommand { arguments }))
        }
    }
}

fn decode_fan(
    words: &[u16],
    cursor: &mut usize,
    byte_offset: usize,
    raw_command: u16,
) -> Result<ModelInstructionKind, ModelError> {
    let command_word = *cursor - 1;
    let prefix = take_arguments(words, cursor, 2, raw_command as u8, byte_offset)?;
    let flags = ModelFanFlags::from_raw(prefix[0]);
    let record_words = if flags.has_textured_records() { 6 } else { 4 };
    let mut vertices = Vec::new();
    loop {
        if words.len().saturating_sub(*cursor) < record_words {
            return Err(ModelError::UnterminatedFan {
                offset: byte_offset + command_word * 2,
            });
        }
        let record = &words[*cursor..*cursor + record_words];
        *cursor += record_words;
        let attributes = if record_words == 6 {
            ModelFanVertexAttributes::Textured {
                uv_words: [record[2], record[3]],
                lighting_and_end: [record[4], record[5]],
            }
        } else {
            ModelFanVertexAttributes::Untextured {
                lighting_and_end: [record[2], record[3]],
            }
        };
        let is_last = record[record_words - 1] & 0xff00 != 0;
        vertices.push(ModelFanVertex {
            vertex: ModelSlotRef::from_raw(record[0]),
            lighting_ref: ModelSlotRef::from_raw(record[1]),
            attributes,
        });
        if is_last {
            if vertices.len() < 3 {
                return Err(ModelError::FanTooShort {
                    offset: byte_offset + command_word * 2,
                    records: vertices.len(),
                });
            }
            break;
        }
    }
    let domain = if flags.has_textured_records() {
        ModelMaterialDomain::Handler3Texture
    } else {
        ModelMaterialDomain::Handler7Material
    };
    Ok(ModelInstructionKind::TriangleFan(ModelTriangleFan {
        flags,
        material: ModelMaterialRef {
            domain,
            global_id: prefix[1] as i16,
        },
        vertices,
    }))
}

fn decode_instance(
    words: &[u16],
    cursor: &mut usize,
    byte_offset: usize,
) -> Result<ModelInstructionKind, ModelError> {
    let command_word = *cursor - 1;
    if words.len().saturating_sub(*cursor) < 3 {
        return Err(ModelError::TruncatedCommand {
            offset: byte_offset + command_word * 2,
            opcode: 0x0e,
            needed: 3,
            remaining: words.len().saturating_sub(*cursor),
        });
    }
    let argument_bytes = words[*cursor + 2];
    if argument_bytes < 6 || argument_bytes % 2 != 0 {
        return Err(ModelError::InvalidInstanceLength {
            offset: byte_offset + command_word * 2,
            byte_len: argument_bytes,
        });
    }
    let argument_count = usize::from(argument_bytes / 2);
    let arguments = take_arguments(words, cursor, argument_count, 0x0e, byte_offset)?;
    let orientation = ModelOrientation::from_raw(arguments[0]);
    let mut dependencies = vec![ModelDynamicDependency::ExternalModelCatalog];
    if orientation.uses_saved_basis() {
        dependencies.push(ModelDynamicDependency::SavedTransformBasis);
    }
    Ok(ModelInstructionKind::ModelInstance(ModelInstance {
        orientation,
        model: ModelResourceRef::from_raw(arguments[1]),
        argument_bytes,
        state_words: arguments[3..].to_vec(),
        dependencies,
    }))
}

fn decode_face(
    words: &[u16],
    cursor: &mut usize,
    byte_offset: usize,
    raw_command: u16,
) -> Result<ModelInstructionKind, ModelError> {
    let opcode = raw_command as u8;
    let domain = match opcode {
        0xd2 | 0xd6 | 0xda | 0xde | 0xe2 | 0xe6 => ModelMaterialDomain::Handler3Texture,
        _ => ModelMaterialDomain::Handler7Material,
    };
    let material = |raw| ModelMaterialRef {
        domain,
        global_id: raw as i16,
    };
    let slot = |raw| ModelSlotRef::from_raw(raw);
    let flags = ModelFaceFlags::from_command(raw_command);
    match opcode {
        0xd0 | 0xd2 => {
            let args = take_arguments(words, cursor, 7, opcode, byte_offset)?;
            Ok(ModelInstructionKind::Triangle(ModelTriangle {
                material: material(args[0]),
                vertices: [slot(args[1]), slot(args[2]), slot(args[3])],
                lighting: ModelTriangleLighting::PerVertex([
                    slot(args[4]),
                    slot(args[5]),
                    slot(args[6]),
                ]),
                flags,
            }))
        }
        0xd4 | 0xd6 => {
            let args = take_arguments(words, cursor, 9, opcode, byte_offset)?;
            Ok(ModelInstructionKind::Quad(ModelQuad {
                material: material(args[0]),
                vertices: [slot(args[1]), slot(args[2]), slot(args[3]), slot(args[4])],
                lighting: ModelQuadLighting::PerVertex([
                    slot(args[5]),
                    slot(args[6]),
                    slot(args[7]),
                    slot(args[8]),
                ]),
                flags,
            }))
        }
        0xd8 | 0xda | 0xdc | 0xde => {
            let args = take_arguments(words, cursor, 5, opcode, byte_offset)?;
            Ok(ModelInstructionKind::Triangle(ModelTriangle {
                material: material(args[0]),
                vertices: [slot(args[2]), slot(args[3]), slot(args[4])],
                lighting: ModelTriangleLighting::Shared(slot(args[1])),
                flags,
            }))
        }
        0xe0 | 0xe2 | 0xe4 | 0xe6 => {
            let args = take_arguments(words, cursor, 6, opcode, byte_offset)?;
            Ok(ModelInstructionKind::Quad(ModelQuad {
                material: material(args[0]),
                vertices: [slot(args[2]), slot(args[3]), slot(args[4]), slot(args[5])],
                lighting: ModelQuadLighting::Shared(slot(args[1])),
                flags,
            }))
        }
        _ => unreachable!("face decoder called for non-face opcode"),
    }
}

fn take_arguments(
    words: &[u16],
    cursor: &mut usize,
    count: usize,
    opcode: u8,
    byte_offset: usize,
) -> Result<Vec<u16>, ModelError> {
    let remaining = words.len().saturating_sub(*cursor);
    if remaining < count {
        return Err(ModelError::TruncatedCommand {
            offset: byte_offset + (*cursor).saturating_sub(1) * 2,
            opcode,
            needed: count,
            remaining,
        });
    }
    let arguments = words[*cursor..*cursor + count].to_vec();
    *cursor += count;
    Ok(arguments)
}

const fn fixed_argument_count(opcode: u8) -> Option<usize> {
    match opcode {
        0x02..=0x04 | 0x0b | 0x13 | 0x16 | 0x22 | 0x23 | 0x25 => Some(3),
        0x0f | 0x10 | 0xeb | 0xec | 0xef => Some(0),
        0x15 | 0x20 | 0xea => Some(1),
        0x1c | 0x21 | 0x2a..=0x2c | 0xed => Some(2),
        0x18..=0x1b | 0x24 | 0x26 => Some(4),
        0x27 => Some(6),
        0x28 => Some(5),
        0x29 => Some(7),
        0xee => Some(3),
        _ => None,
    }
}

fn dynamic_dependencies(opcode: u8) -> Vec<ModelDynamicDependency> {
    match opcode {
        0x0b => vec![ModelDynamicDependency::ViewPlane],
        0x13 => vec![ModelDynamicDependency::CameraDistance],
        0x16 => vec![
            ModelDynamicDependency::RegisterFile,
            ModelDynamicDependency::PackedValueCallback,
        ],
        0x22 => vec![ModelDynamicDependency::RandomSource],
        0x2a => vec![ModelDynamicDependency::ParentVertexSlots],
        0x2b | 0x2c => vec![ModelDynamicDependency::RendererProjection],
        _ => Vec::new(),
    }
}

const fn word_span(byte_offset: usize, start: usize, end: usize) -> ByteSpan {
    ByteSpan::new(byte_offset + start * 2, byte_offset + end * 2)
}
