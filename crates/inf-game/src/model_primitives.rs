//! Native model primitive preparation. Original callback payloads retain their
//! defined bytes and borrowed resources; emitting them is not rasterization.

use inf_formats::{
    FrontendExecutable, FrontendExecutableError, ModelSlotRef, FRONTEND_SINE_TABLE_ADDRESS,
};
use std::sync::Arc;
use thiserror::Error;

use crate::{
    model_context::ModelProjection,
    model_execution::ModelPrimitiveRequest,
    model_lighting::ModelLightingError,
    model_values::{ModelValueCallback, ModelValueError},
    model_vertices::ModelVertexFault,
    render_resources::{RenderResource, RenderResourceData, RenderResourceError},
    terrain_draw::{TerrainWaterTables, TerrainWaterTile},
};

mod clipping;
pub mod d0;
pub mod d2;
pub mod d4;
pub mod d6;
pub mod d8;
pub mod da;
pub mod dc;
pub mod de;
pub mod e0;
pub mod e4;
pub mod e6;
pub mod edge;
pub mod effect;
pub mod fan;
pub mod ground;
mod material_clipping;
mod material_per_corner_clipping;
mod per_corner_clipping;
pub mod quad;
mod shared_shade_clipping;
mod uv_only_clipping;

pub fn prepare<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    values: &mut crate::model_values::ModelValues,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    match request.instruction.raw_command as u8 {
        0x01 => fan::prepare_fan(request, state, tables, host),
        0x07 => edge::prepare_edge(request, state, tables, host),
        0xd0 => d0::prepare_d0(request, state, tables, host),
        0xd2 => d2::prepare_d2(request, state, tables, host),
        0xd4 => d4::prepare_d4(request, state, tables, host),
        0xd6 => d6::prepare_d6(request, state, tables, host),
        0xd8 => d8::prepare_d8(request, state, tables, host),
        0xda => da::prepare_da(request, state, tables, host),
        0xdc => dc::prepare_dc(request, state, tables, host),
        0xde => de::prepare_de(request, state, tables, host),
        0xe0 => e0::prepare_e0(request, state, tables, host),
        0xe4 => e4::prepare_e4(request, state, tables, host),
        0xe2 => quad::prepare_quad(request, state, tables, host),
        0xe6 => e6::prepare_e6(request, state, tables, host),
        0x1a => ground::prepare_ground(request, state, tables, values, host),
        0x1b => effect::prepare_effect(request, state, tables, values, host),
        opcode => Err(PrimitiveError::Unsupported {
            opcode,
            reason: "a recovered primitive handler",
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveTables {
    /// Original 00528BD0, indexed by the OR of vertex clip bytes. This is an
    /// admission table, not a replacement polygon clipping algorithm.
    pub clip_acceptance: [u8; 256],
    pub sine_table: Arc<[u16; 4096]>,
    pub quad_u: [i32; 4],
    pub quad_v: [i32; 4],
    /// 00528F18/1C, Y then X multipliers for near-plane intersections.
    pub clip_projection_bits: [u32; 2],
    /// Original `00537010` shoreline records, independent of the ground blend LUT.
    pub water: TerrainWaterTables,
}

impl PrimitiveTables {
    pub fn from_executable(
        source: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        let sine = source.mapped(FRONTEND_SINE_TABLE_ADDRESS, 8192)?.bytes;
        //0044DBB0 consumes a signed sprite WORD at +0 and orientation DWORD
        //at +4 of each 8-byte entry. The intervening WORD has no owned consumer.
        let shoreline = source.mapped(0x0053_7010, 16 * 8)?.bytes;
        let words = |address| -> Result<[i32; 4], FrontendExecutableError> {
            let bytes = source.mapped(address, 16)?.bytes;
            Ok(std::array::from_fn(|i| {
                i32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().expect("four bytes"))
            }))
        };
        Ok(Self {
            clip_acceptance: source
                .mapped(0x0052_8bd0, 256)?
                .bytes
                .try_into()
                .expect("256-byte mapping"),
            sine_table: Arc::new(std::array::from_fn(|i| {
                u16::from_le_bytes([sine[i * 2], sine[i * 2 + 1]])
            })),
            quad_u: words(0x0052_8eb8)?,
            quad_v: words(0x0052_8ec8)?,
            clip_projection_bits: [source.word(0x0052_8f18)?, source.word(0x0052_8f1c)?],
            water: TerrainWaterTables {
                shoreline: std::array::from_fn(|index| {
                    let entry = &shoreline[index * 8..index * 8 + 8];
                    TerrainWaterTile {
                        sprite_slot: i16::from_le_bytes(entry[..2].try_into().expect("two bytes")),
                        orientation: u32::from_le_bytes(entry[4..].try_into().expect("four bytes")),
                    }
                }),
            },
        })
    }
}

/// Live invocation values; resource selectors and backend callbacks remain
/// separate from intrinsic material/vertex decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimitiveState {
    /// Model context +5C. The low bit selects the Handler-3 physical variant.
    pub mode: u32,
    /// Context +54, copied by far-table primitives.
    pub far_color: u32,
    /// Live 0055F268, copied in full by effect color getter 00490350.
    pub effect_color: u32,
    pub projection: ModelProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveCallback {
    TexturedTriangleNear,
    TexturedTriangleFar,
    FlatTriangleNear,
    FlatTriangleFar,
    FlatQuadNear,
    FlatQuadFar,
    TexturedQuadNear,
    TexturedQuadFar,
    TexturedUvTriangleNear,
    TexturedUvTriangleFar,
    TexturedUvQuadNear,
    TexturedUvQuadFar,
    SharedShadeUvTriangleNear,
    SharedShadeUvTriangleFar,
    SharedShadeUvQuadNear,
    SharedShadeUvQuadFar,
    TexturedShadeUvQuadNear,
    TexturedShadeUvQuadFar,
    EffectQuadNear,
    EffectQuadFar,
    SharedShadeTriangleNear,
    SharedShadeTriangleFar,
    SharedShadeQuadNear,
    SharedShadeQuadFar,
    DaTriangleNear,
    DaTriangleFar,
    DcTriangleNear,
    DcTriangleFar,
    D2TriangleNear,
    D2TriangleFar,
    D6QuadNear,
    D6QuadFar,
    /// 0044EA80/90: D4's per-corner material quads and terrain's colored
    /// quads (0044D030).
    ColoredQuadNear,
    ColoredQuadFar,
    /// 004A38E0/004A3920: D8's material triangles.
    SolidTriangleNear,
    SolidTriangleFar,
}

impl PrimitiveCallback {
    pub const fn native_address(self) -> u32 {
        match self {
            Self::TexturedTriangleNear => 0x004a_38f0,
            Self::TexturedTriangleFar => 0x0044_ea50,
            Self::FlatTriangleNear => 0x004a_39b0,
            Self::FlatTriangleFar => 0x004a_39e0,
            Self::FlatQuadNear => 0x0046_6170,
            Self::FlatQuadFar => 0x0046_6180,
            Self::TexturedQuadNear => 0x0046_6150,
            Self::TexturedQuadFar => 0x0046_6160,
            Self::TexturedUvTriangleNear => 0x004a_3990,
            Self::TexturedUvTriangleFar => 0x004a_39c0,
            Self::TexturedUvQuadNear => 0x004a_3a20,
            Self::TexturedUvQuadFar => 0x004a_3a60,
            Self::SharedShadeUvTriangleNear => 0x004a_3a10,
            Self::SharedShadeUvTriangleFar => 0x004a_3a50,
            Self::SharedShadeUvQuadNear => 0x004a_38d0,
            Self::SharedShadeUvQuadFar => 0x004a_3910,
            Self::TexturedShadeUvQuadNear => 0x004a_3940,
            Self::TexturedShadeUvQuadFar => 0x004a_3970,
            Self::EffectQuadNear => 0x0040_ef30,
            Self::EffectQuadFar => 0x0041_1d50,
            Self::SharedShadeTriangleNear => 0x004a_38c0,
            Self::SharedShadeTriangleFar => 0x004a_3900,
            Self::SharedShadeQuadNear => 0x004a_39a0,
            Self::SharedShadeQuadFar => 0x004a_39d0,
            Self::DaTriangleNear => 0x004a_3a00,
            Self::DaTriangleFar => 0x004a_3a40,
            Self::DcTriangleNear => 0x004a_3950,
            Self::DcTriangleFar => 0x004a_3980,
            Self::D2TriangleNear => 0x004a_3930,
            Self::D2TriangleFar => 0x004a_3960,
            Self::D6QuadNear => 0x0044_ea60,
            Self::D6QuadFar => 0x0044_ea70,
            Self::ColoredQuadNear => 0x0044_ea80,
            Self::ColoredQuadFar => 0x0044_ea90,
            Self::SolidTriangleNear => 0x004a_38e0,
            Self::SolidTriangleFar => 0x004a_3920,
        }
    }

    /// Renderer-virtual slot behind each deferred thunk, proven by retail
    /// disassembly (`mov eax,[ecx]` then `jmp [eax+offset]`).
    /// The pixel backend behind every slot remains a required
    /// host operation, and this mapping only preserves native dispatch
    /// identity.
    pub const fn renderer_virtual_offset(self) -> Option<u32> {
        match self {
            Self::TexturedTriangleNear => Some(0x7c),
            Self::TexturedTriangleFar => Some(0x80),
            Self::FlatTriangleNear => Some(0x4c),
            Self::FlatTriangleFar => Some(0x50),
            Self::FlatQuadNear => Some(0x84),
            Self::FlatQuadFar => Some(0x88),
            Self::TexturedQuadNear => Some(0x9c),
            Self::TexturedQuadFar => Some(0xa0),
            Self::TexturedUvTriangleNear => Some(0x6c),
            Self::TexturedUvTriangleFar => Some(0x70),
            Self::TexturedUvQuadNear => Some(0xb4),
            Self::TexturedUvQuadFar => Some(0xb8),
            Self::SharedShadeUvTriangleNear => Some(0x74),
            Self::SharedShadeUvTriangleFar => Some(0x78),
            Self::SharedShadeUvQuadNear => Some(0xbc),
            Self::SharedShadeUvQuadFar => Some(0xc0),
            Self::TexturedShadeUvQuadNear => Some(0xc4),
            Self::TexturedShadeUvQuadFar => Some(0xc8),
            Self::EffectQuadNear => Some(0xa4),
            Self::EffectQuadFar => Some(0xa8),
            Self::SharedShadeTriangleNear => Some(0x5c),
            Self::SharedShadeTriangleFar => Some(0x60),
            Self::SharedShadeQuadNear => Some(0x8c),
            Self::SharedShadeQuadFar => Some(0x90),
            Self::DaTriangleNear => Some(0x54),
            Self::DaTriangleFar => Some(0x58),
            Self::DcTriangleNear => Some(0x44),
            Self::DcTriangleFar => Some(0x48),
            Self::D2TriangleNear => Some(0x64),
            Self::D2TriangleFar => Some(0x68),
            Self::D6QuadNear => Some(0xac),
            Self::D6QuadFar => Some(0xb0),
            Self::ColoredQuadNear => Some(0x94),
            Self::ColoredQuadFar => Some(0x98),
            Self::SolidTriangleNear => Some(0x3c),
            Self::SolidTriangleFar => Some(0x40),
        }
    }
}

/// One native callback allocation. Payload length is the exact 16-byte block
/// reservation, including unknown padding and color high bytes. A texture
/// pointer is represented by a borrowed relocation instead of a fabricated
/// process address. The pixel backend must follow the callback's read contract.
#[derive(Clone, Debug)]
pub struct PrimitiveCommand<'a> {
    pub callback: PrimitiveCallback,
    /// Native descriptor DWORDs +04/+08/+0C, separate from payload bytes.
    pub header_words: [u32; 3],
    /// The native signed sort word; it need not represent geometric depth.
    pub depth: u32,
    pub payload_blocks: usize,
    pub payload: Vec<Option<u8>>,
    pub texture: Option<(usize, RenderResource<'a>)>,
    /// Non-native: the scene feed took the primitive this came from
    /// ([`crate::scene_capture`]), so a frame the tracer drew leaves it
    /// unless its texture is translucent.
    pub scene_fed: bool,
    /// Non-native: the scene feed already has this additive primitive in 3D
    /// (as a [`crate::scene_feed::SceneSurface::Glow`] face, or a laser's
    /// 3D segments), or it is water spray (`pt_spray.glsl`), so the raster
    /// still draws it for the primary view while queue emitter capture
    /// skips it.
    pub glow_fed: bool,
    /// Non-native: where the moving model this came from stood in the last
    /// frame, for the Vulkan raster's motion ([`crate::raster_motion`]).
    pub motion: Option<crate::raster_motion::ViewMotion>,
    /// The water's surface, which Vulkan's modern raster mirrors the world
    /// in (its screen-space reflections, `docs/rt-plans/modern-raster.md`).
    pub water: bool,
    /// Non-native: retail's ground, which Vulkan's modern raster wets while
    /// it rains (`docs/rt-plans/modern-raster.md`).
    pub ground: bool,
    /// Non-native: how Vulkan's raster draws the sky's models while it
    /// draws the modern sky itself; none for every other primitive.
    pub sky: Option<crate::scene_capture::RasterSkyDraw>,
    /// Non-native, under Vulkan: a spark its faces are laid in again,
    /// added (a struck crystal's).
    pub tint: Option<crate::crystal_flares::CrystalSpark>,
    /// Non-native: the metal face it draws, which Vulkan's modern raster
    /// mirrors the sky in ([`crate::raster_metal`]).
    pub metal: Option<std::rc::Rc<crate::raster_metal::MetalFace>>,
    /// Non-native, under Vulkan: the exact native screen point of each
    /// corner, by the packed whole-pixel XY the payload holds
    /// ([`crate::model_vertices::exact`]). A corner the clipper made keeps
    /// its whole pixels.
    pub exact_corners: Vec<(u32, [f32; 2])>,
}

impl<'a> PrimitiveCommand<'a> {
    pub fn new(callback: PrimitiveCallback, depth: u32, payload_blocks: usize) -> Self {
        Self {
            callback,
            header_words: [0; 3],
            depth,
            payload_blocks,
            payload: recycled_payload(payload_blocks * 16),
            texture: None,
            scene_fed: false,
            glow_fed: false,
            motion: None,
            water: false,
            ground: false,
            sky: None,
            tint: None,
            metal: None,
            exact_corners: PAYLOAD_POOL
                .with_borrow_mut(|pool| pool.1.pop())
                .unwrap_or_default(),
        }
    }

    /// The native screen point a corner whose payload XY is `xy` draws at:
    /// its exact twin where the command carries one, else its whole pixels.
    pub fn corner(&self, xy: u32) -> [f32; 2] {
        self.exact_corners
            .iter()
            .find(|(whole, _)| *whole == xy)
            .map_or(
                [f32::from(xy as i16), f32::from((xy >> 16) as i16)],
                |(_, exact)| *exact,
            )
    }
    pub fn write_u32(&mut self, offset: usize, value: u32) {
        for (target, byte) in self.payload[offset..offset + 4]
            .iter_mut()
            .zip(value.to_le_bytes())
        {
            *target = Some(byte);
        }
    }
    pub fn write_rgb(&mut self, offset: usize, rgb: [u8; 3]) {
        for (target, byte) in self.payload[offset..offset + 3].iter_mut().zip(rgb) {
            *target = Some(byte);
        }
    }
    pub fn write_byte(&mut self, offset: usize, byte: u8) {
        self.payload[offset] = Some(byte);
    }
    pub fn write_texture(&mut self, offset: usize, resource: Option<RenderResource<'a>>) {
        if let Some(resource) = resource {
            self.payload[offset..offset + 4].fill(None);
            self.texture = Some((offset, resource));
        } else {
            self.write_u32(offset, 0);
            self.texture = None;
        }
    }
    pub fn read_u32(&self, offset: usize) -> Option<u32> {
        let bytes = self.payload.get(offset..offset.checked_add(4)?)?;
        Some(u32::from_le_bytes([
            bytes[0]?, bytes[1]?, bytes[2]?, bytes[3]?,
        ]))
    }
}

// A frame's commands drop with the draw queue's reset; their payload and
// corner lists come back here for the next frame's commands, so a primitive
// costs no heap allocation once the pool has filled.
const PAYLOAD_POOL_LIMIT: usize = 1 << 16;
type PayloadPool = (Vec<Vec<Option<u8>>>, Vec<Vec<(u32, [f32; 2])>>);
thread_local! {
    static PAYLOAD_POOL: std::cell::RefCell<PayloadPool> =
        const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
}

fn recycled_payload(len: usize) -> Vec<Option<u8>> {
    let mut payload = PAYLOAD_POOL
        .with_borrow_mut(|pool| pool.0.pop())
        .unwrap_or_default();
    payload.resize(len, None);
    payload
}

impl Drop for PrimitiveCommand<'_> {
    fn drop(&mut self) {
        let mut payload = std::mem::take(&mut self.payload);
        let mut corners = std::mem::take(&mut self.exact_corners);
        // try_with: a command dropped while the thread exits just frees.
        let _ = PAYLOAD_POOL.try_with(|pool| {
            let Ok(mut pool) = pool.try_borrow_mut() else {
                return;
            };
            if pool.0.len() < PAYLOAD_POOL_LIMIT && payload.capacity() != 0 {
                payload.clear();
                pool.0.push(payload);
            }
            if pool.1.len() < PAYLOAD_POOL_LIMIT && corners.capacity() != 0 {
                corners.clear();
                pool.1.push(corners);
            }
        });
    }
}

/// Opaque texture grouping is either exact native pointer arithmetic or an
/// explicitly portable allocation order. Portable ordinals are never addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureSortIdentity {
    /// Base of the logical Handler-3 entry, before +4+variant*1C.
    NativeEntryAddress(u32),
    /// Stable backend-owned logical entry allocation, in first-use order.
    /// Each entry reserves 256 keys for physical selectors. This preserves
    /// descriptor grouping but does not claim retail cross-allocation order.
    PortableEntryOrdinal(u32),
}

// Leave 40000000 (the native untextured opaque key) below all portable textures.
pub const MAX_PORTABLE_TEXTURE_ENTRY_ORDINAL: u32 = 0x003f_fffe;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("portable texture entry ordinal {ordinal} exceeds opaque sort-band capacity")]
pub struct TextureSortError {
    pub ordinal: u32,
}

impl TextureSortIdentity {
    pub fn sort_word(self, selector: u8) -> Result<u32, TextureSortError> {
        match self {
            Self::NativeEntryAddress(base) => {
                let descriptor = base.wrapping_add(4 + u32::from(selector) * 0x1c);
                Ok(((descriptor as i32 >> 1) as u32).wrapping_add(0x4000_0000))
            }
            Self::PortableEntryOrdinal(ordinal) => {
                if ordinal > MAX_PORTABLE_TEXTURE_ENTRY_ORDINAL {
                    return Err(TextureSortError { ordinal });
                }
                Ok(0x4000_0100 + (ordinal << 8) + u32::from(selector))
            }
        }
    }
}

/// Required resource/backend effects at their actual native call boundaries.
/// Texture identities are requested only for opaque material sorting.
pub trait PrimitiveHost<'a>: ModelValueCallback {
    fn texture(&mut self, argument: i32) -> Result<Option<RenderResource<'a>>, Self::Error>;
    fn material(&mut self, argument: i32) -> Result<u32, Self::Error>;
    fn material_sorting(&mut self) -> Result<bool, Self::Error>;
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error>;
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrimitiveOutcome {
    pub commands: usize,
    pub rejected: usize,
}

#[derive(Debug, Error)]
pub enum PrimitiveError<E> {
    #[error("required primitive backend failed")]
    Host(E),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    TextureSort(#[from] TextureSortError),
    #[error("primitive packed value: {0}")]
    Value(ModelValueError<E>),
    #[error(transparent)]
    Vertex(#[from] ModelVertexFault),
    #[error(transparent)]
    Lighting(#[from] ModelLightingError),
    #[error("primitive requires unwritten {field} at slot {index}")]
    Unknown { field: &'static str, index: usize },
    #[error("incompatible primitive record at source byte {0:#x}")]
    Record(usize),
    #[error("primitive opcode {opcode:#04x} requires {reason}")]
    Unsupported { opcode: u8, reason: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeProjectedVertex {
    pub xy: u32,
    pub depth: u32,
    pub clip: u8,
    pub fog: Option<u8>,
}

pub(crate) fn projected_vertex<E>(
    request: &ModelPrimitiveRequest<'_>,
    index: usize,
) -> Result<NativeProjectedVertex, PrimitiveError<E>> {
    let slot = request
        .vertices
        .slots()
        .get(index)
        .ok_or(ModelVertexFault::InvalidSlot {
            index: index as i32,
            count: request.vertices.slots().len(),
        })?;
    let unknown = |field| PrimitiveError::Unknown { field, index };
    Ok(NativeProjectedVertex {
        xy: slot
            .projection
            .read_u32(0)
            .ok_or_else(|| unknown("projected XY"))?,
        depth: slot
            .projection
            .read_u32(4)
            .ok_or_else(|| unknown("projected depth"))?,
        clip: slot.projection.bytes[8].ok_or_else(|| unknown("clip byte"))?,
        fog: slot.projection.bytes[9],
    })
}

fn projection_bytes<'r, E>(
    request: &'r ModelPrimitiveRequest<'_>,
    index: usize,
) -> Result<&'r crate::model_vertices::ModelVertexProjection, PrimitiveError<E>> {
    request
        .vertices
        .slots()
        .get(index)
        .map(|slot| &slot.projection)
        .ok_or_else(|| {
            ModelVertexFault::InvalidSlot {
                index: index as i32,
                count: request.vertices.slots().len(),
            }
            .into()
        })
}
pub(crate) fn vertex_xy<E>(
    request: &ModelPrimitiveRequest<'_>,
    index: usize,
) -> Result<u32, PrimitiveError<E>> {
    projection_bytes(request, index)?
        .read_u32(0)
        .ok_or(PrimitiveError::Unknown {
            field: "projected XY",
            index,
        })
}
pub(crate) fn vertex_depth<E>(
    request: &ModelPrimitiveRequest<'_>,
    index: usize,
) -> Result<u32, PrimitiveError<E>> {
    projection_bytes(request, index)?
        .read_u32(4)
        .ok_or(PrimitiveError::Unknown {
            field: "projected depth",
            index,
        })
}
/// Clip byte for the gate, or `None` when the slot was never projected.
/// Dynamic-only slots (written by control ops like 0x2A/0x2D, outside
/// the static vertex ranges) read `None` until their owner runs.
/// Retail never exhibits these reads — its engine invariant projects
/// every consumed slot — so skipping the face models no retail
/// behavior either way; it fails closed per face (counted as
/// rejected) instead of aborting the whole model walk, preserving
/// downstream faces the walk can draw. Callers must route the skipped
/// face through `rejected`, never invent a gate value: an invented
/// byte would corrupt the acceptance lookup. Out-of-range slots still
/// error via `projection_bytes`.
pub(crate) fn clip_or_skip<E>(
    request: &ModelPrimitiveRequest<'_>,
    index: usize,
) -> Result<Option<u8>, PrimitiveError<E>> {
    Ok(projection_bytes(request, index)?.bytes[8])
}
pub(crate) fn vertex_fog<E>(
    request: &ModelPrimitiveRequest<'_>,
    index: usize,
) -> Result<u8, PrimitiveError<E>> {
    projection_bytes(request, index)?.bytes[9].ok_or(PrimitiveError::Unknown {
        field: "fog byte",
        index,
    })
}

pub(crate) fn shade_rgb<E>(
    request: &ModelPrimitiveRequest<'_>,
    index: usize,
) -> Result<[u8; 3], PrimitiveError<E>> {
    let index16 = u16::try_from(index).map_err(|_| PrimitiveError::Unknown {
        field: "shade index",
        index,
    })?;
    request
        .shades
        .shade(ModelSlotRef::from_raw(index16))?
        .rgb
        .ok_or(PrimitiveError::Unknown {
            field: "shade RGB",
            index,
        })
}

/// 4968xx and equivalent primitive sites query the renderer for every admitted
/// triangle. Depth uses SHR; a selected texture descriptor address uses SAR.
pub(crate) fn textured_sort_depth<'a, H: PrimitiveHost<'a>>(
    depth: u32,
    mode: u32,
    texture: Option<RenderResource<'a>>,
    host: &mut H,
) -> Result<u32, PrimitiveError<H::Error>> {
    if !host.material_sorting().map_err(PrimitiveError::Host)? {
        return Ok(depth);
    }
    let Some(texture) = texture else {
        return Ok(depth);
    };
    let RenderResourceData::Sprite(entry) = texture.data else {
        return Err(RenderResourceError::Registration {
            request: texture.request,
        }
        .into());
    };
    let selector = if mode & 1 == 0 {
        entry.selectors.mode0
    } else {
        entry.selectors.mode1
    };
    if selector == 255 {
        return Ok((depth >> 1).wrapping_add(0xc000_0000));
    }
    let variant =
        entry
            .variants
            .get(usize::from(selector))
            .ok_or(RenderResourceError::Registration {
                request: texture.request,
            })?;
    if variant.flags.raw() & 0x39 != 0 {
        return Ok((depth >> 1).wrapping_add(0xc000_0000));
    }
    let identity = host
        .texture_sort_identity(texture)
        .map_err(PrimitiveError::Host)?;
    Ok(identity.sort_word(selector)?)
}

pub(crate) fn flat_sort_depth<'a, H: PrimitiveHost<'a>>(
    depth: u32,
    mode: u32,
    host: &mut H,
) -> Result<u32, PrimitiveError<H::Error>> {
    if !host.material_sorting().map_err(PrimitiveError::Host)? {
        return Ok(depth);
    }
    Ok(if mode & 0x16 == 0 {
        0x4000_0000
    } else {
        (depth >> 1).wrapping_add(0xc000_0000)
    })
}

#[cfg(test)]
#[path = "model_primitives_tests.rs"]
mod tests;
