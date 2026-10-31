//! CPU target for native Direct3D near flat/textured submissions, not the retail
//! software scan converter. Native vertex conversion belongs to `d3d_flat`;
//! this target implements integer pixel centers, top-left coverage, Gouraud
//! interpolation and explicit inherited depth/blend state. Device precision
//! and matched retail frame acceptance remain separate boundaries.
//!
//! A frame made by [`SoftwareFrame::with_device`] hands the same validated and
//! clipped TL fans to a [hardware device](crate::frame_device) instead of this
//! raster. The CPU frame stays the deterministic pixel reference.

use std::borrow::Cow;

use inf_formats::MaterialValue;
use thiserror::Error;

use crate::{
    d3d_d2::{prepare_d2, read_d2, D2Continuation, D2PixelError},
    d3d_d6::{prepare_d6, read_d6, D6PixelError, D6QuadContinuation},
    d3d_da::{prepare_da, read_da, DaPixelError},
    d3d_effect::{
        prepare_effect_quad, read_effect_quad, settle_effect_mode, EffectPixelError, EffectQuad,
        EffectQuadContinuation,
    },
    d3d_flat::{
        prepare_flat_quad, prepare_flat_triangle, read_shared_shade_quad, D3dFlatBlend,
        D3dFlatClip, D3dFlatState, D3dFlatSubmission,
    },
    d3d_shared_shade_uv::{
        prepare_shared_shade_uv, read_shared_shade_uv, SharedShadeUvContinuation,
        SharedShadeUvPixelError,
    },
    d3d_solid_quad::{
        prepare_solid_quad, prepare_solid_triangle, read_solid_quad, read_solid_triangle,
    },
    d3d_terrain::{colored_terrain_fog, prepare_colored_terrain, read_colored_terrain},
    d3d_texture::{CpuTextureStore, TextureUploadError},
    d3d_textured::{
        prepare_textured_quad, read_textured_quad, sprite_mode, D3dTextureBlend, D3dTextureFog,
        D3dTexturedSubmission, TexturedPixelError,
    },
    d3d_textured_shade_uv::{
        prepare_shade_uv_quad, read_shade_uv_quad, TexturedShadeUvContinuation,
        TexturedShadeUvPixelError,
    },
    d3d_textured_triangle::{
        prepare_de_triangle, prepare_textured_triangle, read_de_triangle, read_textured_triangle,
        TexturedTriangleContinuation,
    },
    d3d_textured_uv::{
        prepare_textured_uv, read_textured_uv, D3dTexturedUvSubmission, TexturedUvContinuation,
    },
    d3d_viewport::{
        clip_polygon_into, log_depth_pole_approximation, zero_screen_area, ClippedPolygon,
        D3dTlVertex, D3dViewportError, D3dViewportState,
    },
    draw_queue::DrawInvocation,
    frame_device::{Device, DeviceDraw, DeviceError, FrameDevice},
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    pixel_backend::PixelBackend,
    presentation::PresentationRect,
    render_resources::RenderResource,
};

/// One decoded near flat-triangle packet (`004A39B0`, three 16-byte blocks).
/// Screen words are signed 16-bit pairs; depths are the original projected
/// DWORDs, interpreted as signed by the Direct3D FILD consumer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatTriangle {
    pub vertices: [FlatVertex; 3],
    pub mode: u32,
    pub material: MaterialValue,
    pub rgb: [[u8; 3]; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatVertex {
    pub x: f32,
    pub y: f32,
    pub depth: i32,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum SoftwarePixelError {
    #[error("software rasterizer requires {expected:?}, received {callback:?}")]
    Unsupported {
        callback: PrimitiveCallback,
        expected: PrimitiveCallback,
    },
    #[error("flat triangle packet requires {expected} payload blocks, received {actual}")]
    Blocks { expected: usize, actual: usize },
    #[error("flat triangle packet requires {expected} payload bytes, received {actual}")]
    PayloadLength { expected: usize, actual: usize },
    #[error(
        "software rasterizer supports only renderer receiver offset zero, received {offset:#010x}"
    )]
    Receiver { offset: u32 },
    #[error("flat triangle packet is missing {field} at payload offset {offset:#04x}")]
    Unknown { field: &'static str, offset: usize },
    #[error(transparent)]
    Viewport(#[from] D3dViewportError),
    #[error("CPU target requires finite TL vertex coordinates and depth")]
    NonFinite,
    #[error("CPU target does not support native {field} state {value}")]
    State { field: &'static str, value: u32 },
    #[error(transparent)]
    Texture(#[from] TexturedPixelError),
    #[error(transparent)]
    Upload(#[from] TextureUploadError),
    #[error(transparent)]
    Effect(#[from] EffectPixelError),
    #[error(transparent)]
    SharedShadeUv(#[from] SharedShadeUvPixelError),
    #[error(transparent)]
    D2(#[from] D2PixelError),
    #[error(transparent)]
    D6(#[from] D6PixelError),
    #[error(transparent)]
    TexturedShadeUv(#[from] TexturedShadeUvPixelError),
    #[error(transparent)]
    Da(#[from] DaPixelError),
}

/// The far flat triangle's fourth block: color DWORD `+2C` and the three
/// corner fog bytes `+30..+32` (`004A39E0`). Far E4 quads carry four.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatFog<const N: usize = 3> {
    pub color: u32,
    pub fog: [u8; N],
}

/// Read the exact near flat-triangle layout written by the native fan
/// producer: interleaved XY/depth DWORDs, mode, material word, then three
/// corner RGB triples. Anything else fails closed.
pub fn read_flat_triangle(
    command: &PrimitiveCommand<'_>,
) -> Result<FlatTriangle, SoftwarePixelError> {
    read_flat_layout(command, PrimitiveCallback::FlatTriangleNear, 3)
}

/// The far layout appends one block: far color then three fog bytes.
pub fn read_flat_triangle_far(
    command: &PrimitiveCommand<'_>,
) -> Result<(FlatTriangle, FlatFog), SoftwarePixelError> {
    let triangle = read_flat_layout(command, PrimitiveCallback::FlatTriangleFar, 4)?;
    let unknown =
        |field: &'static str, offset: usize| SoftwarePixelError::Unknown { field, offset };
    let color = command
        .read_u32(0x2c)
        .ok_or_else(|| unknown("far color", 0x2c))?;
    let mut fog = [0u8; 3];
    for (index, byte) in fog.iter_mut().enumerate() {
        *byte = command.payload[0x30 + index].ok_or_else(|| unknown("fog byte", 0x30 + index))?;
    }
    Ok((triangle, FlatFog { color, fog }))
}

/// The DC record (`004A3950` near, `004A3980` far): three corner XY/depth
/// pairs, mode, material word and one shared shade DWORD; Far adds the far
/// color and three fog bytes. Its consumers `004C03E0`/`004C0A30` are the flat
/// triangle's `004C12F0`/`004C1940` reading that one shade for every corner.
pub fn read_dc_triangle(
    command: &PrimitiveCommand<'_>,
) -> Result<(FlatTriangle, Option<FlatFog>), SoftwarePixelError> {
    let far = match command.callback {
        PrimitiveCallback::DcTriangleNear => false,
        PrimitiveCallback::DcTriangleFar => true,
        callback => {
            return Err(SoftwarePixelError::Unsupported {
                callback,
                expected: PrimitiveCallback::DcTriangleNear,
            })
        }
    };
    if command.payload_blocks != 3 {
        return Err(SoftwarePixelError::Blocks {
            expected: 3,
            actual: command.payload_blocks,
        });
    }
    if command.payload.len() != 48 {
        return Err(SoftwarePixelError::PayloadLength {
            expected: 48,
            actual: command.payload.len(),
        });
    }
    let unknown =
        |field: &'static str, offset: usize| SoftwarePixelError::Unknown { field, offset };
    let mut vertices = [FlatVertex {
        x: 0.0,
        y: 0.0,
        depth: 0,
    }; 3];
    for (index, vertex) in vertices.iter_mut().enumerate() {
        let xy = command
            .read_u32(index * 8)
            .ok_or_else(|| unknown("projected XY", index * 8))?;
        let depth = command
            .read_u32(index * 8 + 4)
            .ok_or_else(|| unknown("projected depth", index * 8 + 4))?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: depth as i32,
        };
    }
    let mode = command
        .read_u32(0x18)
        .ok_or_else(|| unknown("mode word", 0x18))?;
    let material = command
        .read_u32(0x1c)
        .ok_or_else(|| unknown("material word", 0x1c))?;
    let mut rgb = [0u8; 3];
    for (channel, slot) in rgb.iter_mut().enumerate() {
        *slot = command.payload[0x20 + channel]
            .ok_or_else(|| unknown("shared shade RGB", 0x20 + channel))?;
    }
    let fog = if far {
        let color = command
            .read_u32(0x24)
            .ok_or_else(|| unknown("far color", 0x24))?;
        let mut fog = [0u8; 3];
        for (index, byte) in fog.iter_mut().enumerate() {
            *byte =
                command.payload[0x28 + index].ok_or_else(|| unknown("fog byte", 0x28 + index))?;
        }
        Some(FlatFog { color, fog })
    } else {
        None
    };
    Ok((
        FlatTriangle {
            vertices,
            mode,
            material: MaterialValue::from_raw(material),
            rgb: [rgb; 3],
        },
        fog,
    ))
}

fn read_flat_layout(
    command: &PrimitiveCommand<'_>,
    expected: PrimitiveCallback,
    blocks: usize,
) -> Result<FlatTriangle, SoftwarePixelError> {
    if command.callback != expected {
        return Err(SoftwarePixelError::Unsupported {
            callback: command.callback,
            expected,
        });
    }
    if command.payload_blocks != blocks {
        return Err(SoftwarePixelError::Blocks {
            expected: blocks,
            actual: command.payload_blocks,
        });
    }
    if command.payload.len() != blocks * 16 {
        return Err(SoftwarePixelError::PayloadLength {
            expected: blocks * 16,
            actual: command.payload.len(),
        });
    }
    let unknown =
        |field: &'static str, offset: usize| SoftwarePixelError::Unknown { field, offset };
    let mut vertices = [FlatVertex {
        x: 0.0,
        y: 0.0,
        depth: 0,
    }; 3];
    for (index, vertex) in vertices.iter_mut().enumerate() {
        let xy = command
            .read_u32(index * 8)
            .ok_or_else(|| unknown("projected XY", index * 8))?;
        let depth = command
            .read_u32(index * 8 + 4)
            .ok_or_else(|| unknown("projected depth", index * 8 + 4))?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: depth as i32,
        };
    }
    let mode = command
        .read_u32(0x18)
        .ok_or_else(|| unknown("mode word", 0x18))?;
    let material = command
        .read_u32(0x1c)
        .ok_or_else(|| unknown("material word", 0x1c))?;
    let mut rgb = [[0u8; 3]; 3];
    for (index, corner) in rgb.iter_mut().enumerate() {
        let offset = 0x20 + index * 4;
        for (channel, slot) in corner.iter_mut().enumerate() {
            *slot = command.payload[offset + channel]
                .ok_or_else(|| unknown("corner RGB", offset + channel))?;
        }
    }
    Ok(FlatTriangle {
        vertices,
        mode,
        material: MaterialValue::from_raw(material),
        rgb,
    })
}

/// Where a frame owner reads live surface dimensions. CPU pixels are
/// permanently accessible; a window additionally supplies its own size, so a
/// resized SDL surface requires backend recreation.
#[derive(Clone, Copy)]
pub enum SoftwareSurface<'w> {
    Pixels,
    Window(&'w sdl2::video::Window),
    /// Port-only completed-frame scaling. Draw/layout reads the native CPU
    /// surface; validation separately checks the selected physical output.
    ClassicFramebuffer {
        window: &'w sdl2::video::Window,
        output_dimensions: [u32; 2],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoftwareSurfaceMismatch {
    pub actual: [u32; 2],
    pub expected: [u32; 2],
}

impl SoftwareSurface<'_> {
    pub fn dimensions(self, frame: &SoftwareFrame) -> [u32; 2] {
        match self {
            Self::Pixels | Self::ClassicFramebuffer { .. } => [frame.width(), frame.height()],
            Self::Window(window) => {
                let (width, height) = window.size();
                [width, height]
            }
        }
    }

    pub fn validate_dimensions(self, frame: &SoftwareFrame) -> Result<(), SoftwareSurfaceMismatch> {
        let (actual, expected) = match self {
            Self::Pixels => return Ok(()),
            Self::Window(window) => (window.size().into(), [frame.width(), frame.height()]),
            Self::ClassicFramebuffer {
                window,
                output_dimensions,
            } => (window.size().into(), output_dimensions),
        };
        if actual != expected {
            return Err(SoftwareSurfaceMismatch { actual, expected });
        }
        Ok(())
    }
}

/// A native 16-bit surface WORD as the frame shows it: each five-bit
/// channel placed by the renderer's material shifts (`[6, 1, 4]` for RGB555)
/// and widened by repeating its top three bits, as `004D1C79` expands a
/// solid colour.
pub fn native_word_rgba(word: u16, shifts: [u32; 3]) -> [u8; 4] {
    let [red, green, blue] = shifts;
    let word = u32::from(word);
    let packed = (word.wrapping_shl(15u32.wrapping_sub(red)) & 0xf8_0000)
        | (word.wrapping_shl(7u32.wrapping_sub(green)) & 0xf800)
        | (word.wrapping_shl(blue.wrapping_sub(1)) & 0xf8);
    let packed = packed | ((packed >> 5) & 0x07_0707);
    [
        (packed >> 16) as u8,
        (packed >> 8) as u8,
        packed as u8,
        0xff,
    ]
}

/// The WORD a 16-bit surface holds for a frame pixel: each channel's top
/// five bits, repacked as `00471D40` packs its colours.
pub fn rgba_native_word(rgba: [u8; 4], shifts: [u32; 3]) -> u16 {
    let [red, green, blue] = shifts;
    let channel = |value: u8| u32::from(value) & 0xf8;
    (channel(rgba[2]).wrapping_shr(blue.wrapping_sub(1))
        | channel(rgba[0]).wrapping_shl(red.wrapping_add(1))
        | channel(rgba[1]).wrapping_shl(green.wrapping_add(1))) as u16
}

/// Port-only: the part of a widescreen frame that a screen authored for a
/// 4:3 display draws on as if it were the whole display. Layout reads its
/// size, clipping keeps primitives inside it and drawing moves them by its
/// origin; clears and presentation still cover the whole frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameCanvas {
    pub origin: [u32; 2],
    pub size: [u32; 2],
}

/// A presented frame's pixels for a snapshot ([`SoftwareFrame::stage_rgba`]).
pub(crate) enum StagedPixels<'a> {
    /// Read at once: the CPU raster's, or OpenGL's.
    Read(Cow<'a, [u8]>),
    /// Copied on the Vulkan device, read back when the snapshot is saved.
    Device(crate::vk_device::StagedRgba),
}

/// RGBA framebuffer and transformed Direct3D depth, cleared to 1.0.
#[derive(Debug)]
pub struct SoftwareFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    depth: Vec<f32>,
    /// The hardware device drawing in place of the CPU raster. Its frames
    /// keep no CPU pixels: `rgba` and `depth` stay empty.
    device: Option<FrameDevice>,
    canvas: Option<FrameCanvas>,
    /// The tracer drew this frame's world: the replay leaves what the scene
    /// feed took until [`Self::end_traced_world`].
    traced: bool,
    /// Where the traced world's moving models were in the last frame, lent
    /// to each frame's feed.
    motion: crate::scene_feed::MotionHistory,
    /// Where the raster world's moving models stood in the last frame, lent
    /// to each untraced world the device follows over frames.
    raster_motion: crate::raster_motion::RasterMotion,
    /// The last traced frame's vertex and triangle lists, emptied, for the
    /// next feed to fill without growing them from nothing.
    spare_feed: Option<(
        Vec<crate::scene_feed::SceneVertex>,
        Vec<crate::scene_feed::SceneTriangle>,
    )>,
    /// Port option: the world's draw under the water ([`WetFrame`]).
    wet: Option<WetFrame>,
}

/// Port option, the remastered camera under the water: V2000's retail
/// underwater projectors (as OpenV2K's `v2k-render` projection carries
/// them) move each projected vertex by -2..1
/// pixels in slow diagonal bands. Infestation has no underwater view of its
/// own, so its remaster takes its sibling's. The offsets are pixels of a
/// 480-line view, scaled to the frame; the world's colours dim toward the
/// water's with them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WetFrame {
    /// The wobble at each `(x + y) % 64` of the 480-line view's pixels.
    pub offsets: [[i8; 2]; 64],
    /// Frame pixels per 480-line pixel.
    pub scale: f32,
    /// What each channel of the vertex colours keeps.
    pub dim: [f32; 3],
}

impl WetFrame {
    /// The wobble at animation `tick` (20 ms steps): phase `(tick + 2 *
    /// (x + y)) * 0x200`, `dx = sine(phase + 0x4000)`, `dy = sine(phase)`,
    /// each `* 0x10001 >> 30`.
    pub fn new(tick: u32, sine: &[u16; 4096], frame_height: u32, dim: [f32; 3]) -> Self {
        let step = |angle: u32| {
            let sine = inf_formats::native_basis::sine_q15((angle & 0xffff) as i32, sine);
            // Retail's 32-bit product: the shift gives -2..1.
            (sine.wrapping_mul(0x1_0001) >> 30) as i8
        };
        Self {
            offsets: std::array::from_fn(|index| {
                let phase = tick.wrapping_add(2 * index as u32).wrapping_mul(0x200);
                [step(phase.wrapping_add(0x4000)), step(phase)]
            }),
            scale: frame_height.max(1) as f32 / 480.0,
            dim,
        }
    }

    fn apply(&self, mut vertex: D3dTlVertex) -> D3dTlVertex {
        let at = |value: f32| (value / self.scale).round() as i32;
        let [dx, dy] = self.offsets[(at(vertex.x) + at(vertex.y)).rem_euclid(64) as usize];
        vertex.x += f32::from(dx) * self.scale;
        vertex.y += f32::from(dy) * self.scale;
        for (channel, dim) in vertex.diffuse[..3].iter_mut().zip(self.dim) {
            *channel = (f32::from(*channel) * dim).round() as u8;
        }
        vertex
    }
}

impl SoftwareFrame {
    pub fn new(width: u32, height: u32, clear: [u8; 4]) -> Self {
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .expect("software frame dimensions fit in memory");
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for _ in 0..pixel_count {
            rgba.extend_from_slice(&clear);
        }
        Self {
            width,
            height,
            rgba,
            depth: vec![1.0; pixel_count],
            device: None,
            canvas: None,
            traced: false,
            motion: Default::default(),
            raster_motion: Default::default(),
            spare_feed: None,
            wet: None,
        }
    }

    /// A frame of the same native size and clear whose pixels live on a
    /// hardware device, drawn at `target` pixels: the native size, or the
    /// presented size above it. Validation, clipping, layout and state reads
    /// stay native and shared with the CPU raster; only coverage, sampling
    /// and blending move to the device.
    pub fn with_device(
        device: &Device,
        width: u32,
        height: u32,
        target: [u32; 2],
        clear: [u8; 4],
    ) -> Result<Self, DeviceError> {
        Ok(Self {
            width,
            height,
            rgba: Vec::new(),
            depth: Vec::new(),
            device: Some(device.frame([width, height], target, clear)?),
            canvas: None,
            traced: false,
            motion: Default::default(),
            raster_motion: Default::default(),
            spare_feed: None,
            wet: None,
        })
    }

    /// The traced world's motion history, lent to a frame's feed.
    pub fn take_motion(&mut self) -> crate::scene_feed::MotionHistory {
        std::mem::take(&mut self.motion)
    }

    /// The feed's history back, for the next frame.
    pub fn keep_motion(&mut self, motion: crate::scene_feed::MotionHistory) {
        self.motion = motion;
    }

    /// The spare feed lists, emptied ([`Self::keep_spare_feed`]).
    pub fn take_spare_feed(
        &mut self,
    ) -> Option<(
        Vec<crate::scene_feed::SceneVertex>,
        Vec<crate::scene_feed::SceneTriangle>,
    )> {
        self.spare_feed.take()
    }

    /// A traced frame's feed lists, kept for the next feed.
    pub fn keep_spare_feed(
        &mut self,
        mut vertices: Vec<crate::scene_feed::SceneVertex>,
        mut triangles: Vec<crate::scene_feed::SceneTriangle>,
    ) {
        vertices.clear();
        triangles.clear();
        self.spare_feed = Some((vertices, triangles));
    }

    /// The raster world's motion history, lent to a world's draws.
    pub fn take_raster_motion(&mut self) -> crate::raster_motion::RasterMotion {
        std::mem::take(&mut self.raster_motion)
    }

    /// The raster world's history back, for the next frame.
    pub fn keep_raster_motion(&mut self, motion: crate::raster_motion::RasterMotion) {
        self.raster_motion = motion;
    }

    /// The moving model the next submissions come from, or none: the device
    /// follows their motion where it follows the image over frames.
    pub fn set_draw_motion(&mut self, motion: Option<crate::raster_motion::ViewMotion>) {
        if let Some(device) = &mut self.device {
            device.set_draw_motion(motion);
        }
    }

    /// The next submissions are covered by a traced face drawn after them
    /// over the same corners (where the device traces).
    pub fn set_draw_covered(&mut self, covered: bool) {
        if let Some(device) = &mut self.device {
            device.set_draw_covered(covered);
        }
    }

    /// The colour the next submissions are laid in again, added, or none
    /// (a flaring crystal's, where the device does).
    pub fn set_draw_tint(&mut self, tint: Option<crate::crystal_flares::CrystalSpark>) {
        if let Some(device) = &mut self.device {
            device.set_draw_tint(tint);
        }
    }

    /// The metal face the next submissions draw, or none: the device
    /// mirrors the sky in it where it does (Vulkan's modern raster).
    pub fn set_draw_metal(&mut self, metal: Option<std::rc::Rc<crate::raster_metal::MetalFace>>) {
        if let Some(device) = &mut self.device {
            device.set_draw_metal(metal);
        }
    }

    /// Whether this world's metal mirrors the sky: Vulkan's raster, where
    /// it writes the world's frame block.
    pub fn reflects_metal(&self) -> bool {
        self.device
            .as_ref()
            .is_some_and(FrameDevice::reflects_metal)
    }

    /// Whether the next submissions are the water's surface, which the
    /// device mirrors the world in where it does (Vulkan's modern raster).
    pub fn set_draw_water(&mut self, water: bool) {
        if let Some(device) = &mut self.device {
            device.set_draw_water(water);
        }
    }

    /// Whether the next submissions are retail's ground, which the device
    /// wets in the rain where it does (Vulkan's modern raster), with the
    /// colour a fogged colored quad mixed into its corners.
    pub fn set_draw_ground(&mut self, ground: Option<[u8; 3]>) {
        if let Some(device) = &mut self.device {
            device.set_draw_ground(ground);
        }
    }

    /// Whether the next submissions are the modern sky's land band, which
    /// the device lays its own sky behind (Vulkan's modern raster).
    pub fn set_draw_band(&mut self, band: bool) {
        if let Some(device) = &mut self.device {
            device.set_draw_band(band);
        }
    }

    /// The surface screens lay out on: the canvas while one is set,
    /// otherwise the whole frame.
    pub fn width(&self) -> u32 {
        self.canvas.map_or(self.width, |canvas| canvas.size[0])
    }

    pub fn height(&self) -> u32 {
        self.canvas.map_or(self.height, |canvas| canvas.size[1])
    }

    /// The whole frame, which presentation shows.
    pub const fn buffer_size(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    /// The centred 4:3 region of a frame wider than 4:3, at its full height.
    pub fn four_three_canvas(&self) -> Option<FrameCanvas> {
        let width = (u64::from(self.height) * 4 / 3) as u32;
        (self.width > width).then(|| FrameCanvas {
            origin: [(self.width - width) / 2, 0],
            size: [width, self.height],
        })
    }

    /// The 4:3 region against the left edge of a frame wider than 4:3.
    pub fn left_four_three_canvas(&self) -> Option<FrameCanvas> {
        self.four_three_canvas().map(|canvas| FrameCanvas {
            origin: [0, 0],
            ..canvas
        })
    }

    pub fn canvas(&self) -> Option<FrameCanvas> {
        self.canvas
    }

    /// Set or clear the canvas later draws use; it must lie inside the frame.
    pub fn set_canvas(&mut self, canvas: Option<FrameCanvas>) {
        if let Some(canvas) = canvas {
            assert!(
                canvas.origin[0] + canvas.size[0] <= self.width
                    && canvas.origin[1] + canvas.size[1] <= self.height,
                "canvas {canvas:?} outside the {}x{} frame",
                self.width,
                self.height
            );
        }
        self.canvas = canvas;
    }

    /// The clip bounds for `viewport`: inside the canvas while one is set,
    /// as the whole 4:3 display bounds retail's viewport.
    fn clip_bounds(&self, viewport: [i16; 4]) -> [i16; 4] {
        match self.canvas {
            None => viewport,
            Some(canvas) => [
                viewport[0].max(0),
                viewport[1].max(0),
                viewport[2].min(canvas.size[0] as i16),
                viewport[3].min(canvas.size[1] as i16),
            ],
        }
    }

    /// Port option: wobble and dim the world's draw under the water until
    /// set to None ([`WetFrame`]).
    pub fn set_wet(&mut self, wet: Option<WetFrame>) {
        self.wet = wet;
    }

    /// `vertex` as the world under the water draws it, else as it is.
    pub(crate) fn wet_vertex(&self, vertex: D3dTlVertex) -> D3dTlVertex {
        self.wet.map_or(vertex, |wet| wet.apply(vertex))
    }

    /// A textured draw's fog under the water, dimmed as its vertex colours
    /// are: a flat draw bakes its fog into those colours, so both reach the
    /// same colour where the fog is whole.
    pub(crate) fn wet_fog(&self, fog: D3dTextureFog) -> D3dTextureFog {
        match (self.wet, fog) {
            (Some(wet), D3dTextureFog::Vertex { color }) => D3dTextureFog::Vertex {
                color: std::array::from_fn(|c| (f32::from(color[c]) * wet.dim[c]).round() as u8),
            },
            _ => fog,
        }
    }

    /// Move clipped canvas coordinates onto the frame.
    fn place(&self, polygon: &mut [D3dTlVertex]) {
        if let Some(canvas) = self.canvas {
            for vertex in polygon {
                vertex.x += canvas.origin[0] as f32;
                vertex.y += canvas.origin[1] as f32;
            }
        }
    }

    pub fn pixel_rgba(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let offset = self.pixel_offset(x, y)?;
        Some(
            self.rgba[offset..offset + 4]
                .try_into()
                .expect("RGBA pixel"),
        )
    }

    pub fn pixel_depth(&self, x: u32, y: u32) -> Option<f32> {
        let index = self.pixel_index(x, y)?;
        Some(self.depth[index])
    }

    /// CPU pixels, top row first. A device frame keeps none; read it with
    /// [`Self::read_rgba`].
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// The size of [`Self::read_rgba`]'s image: the device's target, or the
    /// native size of CPU pixels.
    pub fn read_size(&self) -> [u32; 2] {
        match &self.device {
            Some(device) => device.target(),
            None => [self.width, self.height],
        }
    }

    /// The completed pixels, top row first, from either target, at
    /// [`Self::read_size`]. A device frame first draws its pending batch and
    /// reads the device back.
    pub fn read_rgba(&mut self) -> Cow<'_, [u8]> {
        match &mut self.device {
            Some(device) => Cow::Owned(device.read_rgba()),
            None => Cow::Borrowed(&self.rgba),
        }
    }

    /// The completed pixels for a snapshot, as [`Self::read_rgba`] gives
    /// them: read now, or on the Vulkan device copied now and read back when
    /// the snapshot is saved, so a run reads back only the frames it saves.
    pub(crate) fn stage_rgba(&mut self) -> StagedPixels<'_> {
        match &mut self.device {
            Some(device) => match device.stage_rgba() {
                Ok(pixels) => StagedPixels::Read(Cow::Owned(pixels)),
                Err(staged) => StagedPixels::Device(staged),
            },
            None => StagedPixels::Read(Cow::Borrowed(&self.rgba)),
        }
    }

    /// Thermal vision's camera over the world drawn so far, before the
    /// HUD, on a device that has one; other frames keep retail's infrared.
    pub fn finish_thermal(&mut self, ironbow: bool) {
        if let Some(device) = &mut self.device {
            device.finish_thermal(ironbow);
        }
    }

    /// The world's draws begin, with `camera`: a device that anti-aliases
    /// an untraced world (Vulkan) may move them a sub-pixel for it. True
    /// when it follows the image over frames, and so moving models' motion.
    /// Its water mirrors the world with the least share `water_reflect`
    /// (none: lava, which does not), and the eye under its live surface
    /// sees through it from below, through the `murk`; a device that
    /// takes the world's `light` draws its modern sky, where `sky`, in the
    /// painting's place.
    pub(crate) fn begin_raster_world(
        &mut self,
        camera: &crate::scene_feed::SceneCamera,
        water_reflect: Option<f32>,
        murk: Option<crate::vk_device::Murk>,
        light: Option<&crate::scene_feed::SceneFeed>,
        sky: bool,
    ) -> bool {
        self.device.as_mut().is_some_and(|device| {
            device.begin_raster_world(camera, water_reflect, murk, light, sky)
        })
    }

    /// The raster world's `light` again, now with the vehicles' lamps the
    /// light pass kept, for the water a device shades.
    pub(crate) fn raster_lamps(&mut self, light: &crate::scene_feed::SceneFeed) {
        if let Some(device) = &mut self.device {
            device.raster_lamps(light);
        }
    }

    /// Whether the frame's device shades an untraced world's water itself
    /// (Vulkan's raster, where it can): the view from under the water's
    /// surface then is the device's to draw.
    pub fn shades_water(&self) -> bool {
        self.device.as_ref().is_some_and(FrameDevice::shades_water)
    }

    /// The painting is drawn: a device that draws the modern sky lays it
    /// behind the painting's land and art, before the world's draws.
    pub fn draw_raster_sky(&mut self) {
        if let Some(device) = &mut self.device {
            device.draw_raster_sky();
        }
    }

    /// Whether the device draws this world's modern sky in the painting's
    /// place.
    pub fn draws_raster_sky(&self) -> bool {
        self.device
            .as_ref()
            .is_some_and(|device| device.draws_raster_sky())
    }

    /// The world's last draw is in, before the HUD's: anti-alias it where
    /// the device does.
    pub fn finish_raster_world(&mut self) {
        if let Some(device) = &mut self.device {
            device.finish_raster_world();
        }
    }

    /// Whether the frame draws through Vulkan.
    pub fn is_vulkan(&self) -> bool {
        matches!(self.device, Some(FrameDevice::Vulkan(_)))
    }

    /// Whether the frame's device traces the world (the modern mode).
    pub fn traces_rays(&self) -> bool {
        self.device.as_ref().is_some_and(FrameDevice::traces_rays)
    }

    /// Whether the world drawn next is under infrared vision, which a
    /// device that traces rays traces whatever the player's Ray Tracing.
    pub fn set_trace_infrared(&mut self, on: bool) {
        if let Some(device) = &mut self.device {
            device.set_trace_infrared(on);
        }
    }

    /// Make the device's tracer and upscaler ahead of the first traced
    /// frame, so a world's first frames do not wait on them.
    pub fn prepare_trace(&mut self) {
        if let Some(device) = &mut self.device {
            device.prepare_trace();
        }
    }

    /// Trace `feed` (the world in world space, its texture indices into
    /// `textures`) into a device frame that traces rays: true when it did,
    /// and the replay then leaves what the feed took. Other frames are left
    /// as they are.
    pub fn trace_world(
        &mut self,
        feed: &crate::scene_feed::SceneFeed,
        textures: &[&crate::d3d_texture::CpuTexture],
    ) -> bool {
        let traced = self
            .device
            .as_mut()
            .is_some_and(|device| device.trace_world(feed, textures));
        self.traced |= traced;
        traced
    }

    /// Fans added over the traced world, depth-tested against it and
    /// writing no depth: the gates' redrawn lightning.
    pub fn add_light_fans(&mut self, fans: &[Vec<crate::d3d_viewport::D3dTlVertex>]) {
        if !self.traced {
            return;
        }
        let Some(device) = self.device.as_mut() else {
            return;
        };
        let draw = DeviceDraw {
            blend: D3dTextureBlend::Additive,
            depth_test: true,
            depth_write: false,
            texture: None,
        };
        for fan in fans {
            device.fan(fan, &draw);
        }
    }

    /// Fans laid over Vulkan's world, traced or not, by their alpha,
    /// depth-tested and writing no depth: the craft's spray
    /// ([`crate::craft_spray`]). The raster holds them till its wet
    /// ground and water are shaded, as it holds retail's blended draws.
    /// OpenGL and the software raster draw none.
    pub fn add_spray_fans(&mut self, fans: &[Vec<crate::d3d_viewport::D3dTlVertex>]) {
        let Some(device @ FrameDevice::Vulkan(_)) = self.device.as_mut() else {
            return;
        };
        let draw = DeviceDraw {
            blend: D3dTextureBlend::SourceAlpha,
            depth_test: true,
            depth_write: false,
            texture: None,
        };
        for fan in fans {
            device.fan(fan, &draw);
        }
    }

    /// The tracer drew the world, and the feed's draws are left.
    pub fn traced_world(&self) -> bool {
        self.traced
    }

    /// A traced world's camera while its draws come in, or none once it is
    /// done: its raster crystals find their spark with it.
    pub fn set_world_camera(&mut self, camera: Option<crate::scene_feed::SceneCamera>) {
        if let Some(device) = &mut self.device {
            device.set_world_camera(camera);
        }
    }

    /// The world's camera while its draws come in (Vulkan, traced or
    /// raster), else none.
    pub fn world_camera(&self) -> Option<crate::scene_feed::SceneCamera> {
        self.device
            .as_ref()
            .and_then(|device| device.world_camera())
    }

    /// The world is done: the traced one takes its lighting preset's grade,
    /// and later draws (the HUD) are the raster's again.
    pub fn end_traced_world(&mut self) {
        if self.traced {
            if let Some(device) = &mut self.device {
                device.finish_traced_world();
            }
        }
        self.traced = false;
    }

    /// Draw a device frame into `destination` of the window's back buffer,
    /// whose drawable size is `output`. A CPU frame has no device to
    /// present through and reports false.
    pub fn present_device(&mut self, output: [u32; 2], destination: PresentationRect) -> bool {
        let Some(device) = &mut self.device else {
            return false;
        };
        device.present(output, destination);
        true
    }

    /// Replace the color surface without changing depth or pipeline state.
    /// Port option: under the water the world's backdrop is its fog's
    /// colour, cleared in place of the sky; a frame holding a 4:3 canvas
    /// keeps its bars, and the world its sky (false).
    pub fn clear_backdrop(&mut self, rgba: [u8; 4]) -> bool {
        if self.canvas.is_some() {
            return false;
        }
        self.clear_color(rgba);
        true
    }

    pub fn clear_color(&mut self, rgba: [u8; 4]) {
        if let Some(device) = &mut self.device {
            device.clear_color(rgba);
            return;
        }
        for pixel in self.rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&rgba);
        }
    }

    /// Replace the whole color surface from caller-supplied top-down RGBA
    /// bytes. Returns true when the lengths match exactly; on mismatch the
    /// surface is left untouched and false is reported for the caller to log.
    pub fn replace_surface(&mut self, rgba: &[u8]) -> bool {
        if rgba.len() as u64 != u64::from(self.width) * u64::from(self.height) * 4 {
            return false;
        }
        match &mut self.device {
            Some(device) => device.replace_surface(rgba),
            None => self.rgba.copy_from_slice(rgba),
        }
        true
    }

    /// Fit top-down RGBA bytes of the given source dimensions inside this
    /// frame at their own aspect, centred, with black around them and
    /// nearest-neighbour sampling. Retail presents the title video through
    /// the fullscreen video renderer whatever the display mode, so a source
    /// smaller or larger than the frame still draws: a source of the frame's
    /// aspect fills it, and a widescreen frame shows a 4:3 title video with
    /// black sides. Returns false (leaving the surface untouched) for zero or
    /// mismatched sources.
    pub fn fit_surface(&mut self, src_width: u32, src_height: u32, rgba: &[u8]) -> bool {
        if src_width == 0 || src_height == 0 || self.width == 0 || self.height == 0 {
            return false;
        }
        let expected = (src_width as u64)
            .checked_mul(src_height as u64)
            .and_then(|pixels| pixels.checked_mul(4));
        if expected != Some(rgba.len() as u64) {
            return false;
        }
        let destination = crate::presentation::ClassicScaling::FourThree
            .destination([src_width, src_height], [self.width, self.height]);
        let mut fitted = [0, 0, 0, 255].repeat(self.width as usize * self.height as usize);
        for y in 0..destination.height {
            let src_y =
                (u64::from(y) * u64::from(src_height) / u64::from(destination.height)) as usize;
            for x in 0..destination.width {
                let src_x =
                    (u64::from(x) * u64::from(src_width) / u64::from(destination.width)) as usize;
                let source = (src_y * src_width as usize + src_x) * 4;
                let target = ((destination.y as usize + y as usize) * self.width as usize
                    + destination.x as usize
                    + x as usize)
                    * 4;
                fitted[target..target + 4].copy_from_slice(&rgba[source..source + 4]);
            }
        }
        match &mut self.device {
            Some(device) => device.replace_surface(&fitted),
            None => self.rgba = fitted,
        }
        true
    }

    /// Native full depth-surface clear used by renderer reset 004BCA20.
    /// The color target and live renderer policy remain unchanged.
    pub fn clear_depth(&mut self) {
        if let Some(device) = &mut self.device {
            device.clear_depth();
            return;
        }
        self.depth.fill(1.0);
    }

    /// Convert the native packet and apply only the states this callback
    /// actually writes. The caller supplies all persistent pipeline state.
    pub fn fill_flat_triangle(
        &mut self,
        triangle: &FlatTriangle,
        renderer: D3dFlatState,
        pipeline: &mut D3dRasterState,
    ) -> Result<usize, SoftwarePixelError> {
        let submission = prepare_flat_triangle(triangle, renderer);
        self.fill_flat_submission(&submission, renderer.near_depth, pipeline)
    }

    /// Shared TL coverage for separately recovered solid triangle/quad consumers.
    pub fn fill_flat_submission<const V: usize, const T: usize>(
        &mut self,
        submission: &D3dFlatSubmission<V, T>,
        near_depth: u32,
        pipeline: &mut D3dRasterState,
    ) -> Result<usize, SoftwarePixelError> {
        if submission.policy.fog_enabled {
            return Err(SoftwarePixelError::State {
                field: "FOGENABLE",
                value: 1,
            });
        }
        if let Some(value) = submission.policy.depth_enable {
            pipeline.depth_enabled = native_bool("ZENABLE", value)?;
        }
        if let Some(value) = submission.policy.depth_write {
            pipeline.depth_write = native_bool("ZWRITEENABLE", value)?;
        }
        // Mode 02 inherits ZENABLE, and only overrides ZWRITEENABLE when
        // the display owns a depth target. It never changes ZFUNC or
        // ALPHABLENDENABLE (004C1309..004C13B6).
        for (field, value, supported) in [
            ("ZFUNC", pipeline.depth_function, 4),
            ("SHADEMODE", pipeline.shade_mode, 2),
            ("CULLMODE", pipeline.cull_mode, 1),
        ] {
            if value != supported && (field != "ZFUNC" || pipeline.depth_enabled) {
                return Err(SoftwarePixelError::State { field, value });
            }
        }
        if submission.vertices.iter().any(|v| {
            !v.x.is_finite() || !v.y.is_finite() || !v.depth.is_finite() || !v.rhw.is_finite()
        }) {
            return depth_pole(&submission.vertices, &submission.triangles, false);
        }
        let additive =
            pipeline.alpha_blend_enabled && submission.policy.blend == D3dFlatBlend::Additive;
        let mut polygons: [ClippedPolygon; T] = std::array::from_fn(|_| ClippedPolygon::new());
        for (polygon, indices) in polygons.iter_mut().zip(submission.triangles) {
            let vertices = indices.map(|i| self.wet_vertex(submission.vertices[i]));
            if submission.clip == D3dFlatClip::Bypass {
                polygon.extend_from_slice(&vertices);
            } else {
                clip_polygon_into(
                    &vertices,
                    D3dViewportState {
                        bounds: self.clip_bounds(pipeline.viewport),
                        near_depth,
                        mode: submission.mode,
                    },
                    polygon,
                )?;
            }
        }
        for polygon in &mut polygons {
            self.place(polygon);
        }
        // Complete all native clipping before writing any triangle.
        if let Some(device) = &mut self.device {
            let draw = DeviceDraw {
                blend: if additive {
                    D3dTextureBlend::Additive
                } else {
                    D3dTextureBlend::Copy
                },
                depth_test: pipeline.depth_enabled,
                depth_write: pipeline.depth_write,
                texture: None,
            };
            for polygon in &polygons {
                device.fan(polygon, &draw);
            }
            // The device does not count fragments.
            return Ok(0);
        }
        Ok(polygons
            .iter()
            .filter(|p| p.len() >= 3)
            .map(|polygon| {
                polygon[1..]
                    .windows(2)
                    .map(|edge| {
                        raster::rasterize(self, &[polygon[0], edge[0], edge[1]], pipeline, additive)
                    })
                    .sum::<usize>()
            })
            .sum())
    }

    /// Whether the frame keeps CPU pixels a native surface read can see.
    pub fn has_cpu_pixels(&self) -> bool {
        self.device.is_none()
    }

    /// A canvas position on the frame, if inside the canvas.
    fn direct_position(&self, [x, y]: [i32; 2]) -> Option<[u32; 2]> {
        let inside =
            (0..self.width() as i32).contains(&x) && (0..self.height() as i32).contains(&y);
        let origin = self.canvas.map_or([0, 0], |canvas| canvas.origin);
        inside.then(|| [x as u32 + origin[0], y as u32 + origin[1]])
    }

    /// The WORD a native read of the locked back buffer finds at a canvas
    /// position: [`rgba_native_word`] of the CPU pixel. None on a device
    /// frame, which keeps no CPU pixels, or outside the canvas.
    pub fn native_word(&self, position: [i32; 2], shifts: [u32; 3]) -> Option<u16> {
        let [x, y] = self.direct_position(position)?;
        self.pixel_rgba(x, y)
            .map(|rgba| rgba_native_word(rgba, shifts))
    }

    /// Native WORDs stored straight into the locked back buffer (the
    /// radar's `004719E0` and `00470170`), at canvas positions: no depth
    /// test or write and no clipping. A CPU frame takes each WORD's
    /// [`native_word_rgba`]; a device frame draws it as a one-pixel quad.
    /// With `alpha` below 255 a device frame instead blends the colour over
    /// the pixel by that alpha, the port's approximation where retail reads
    /// the pixel back (a device frame keeps none). Positions outside the
    /// canvas, which retail would write past the surface, are skipped and
    /// counted.
    pub fn put_native_words(&mut self, words: &[([i32; 2], u16, u8)], shifts: [u32; 3]) -> usize {
        let mut outside = 0;
        for &(position, word, alpha) in words {
            let Some([x, y]) = self.direct_position(position) else {
                outside += 1;
                continue;
            };
            let [red, green, blue, _] = native_word_rgba(word, shifts);
            match &mut self.device {
                Some(device) => {
                    let vertex = |dx: u32, dy: u32| D3dTlVertex {
                        x: (x + dx) as f32,
                        y: (y + dy) as f32,
                        depth: 0.0,
                        rhw: 1.0,
                        diffuse: [red, green, blue, alpha],
                        specular: 0xff00_0000,
                        uv: [0.0; 2],
                    };
                    device.fan(
                        &[vertex(0, 0), vertex(1, 0), vertex(1, 1), vertex(0, 1)],
                        &DeviceDraw {
                            blend: if alpha == 0xff {
                                D3dTextureBlend::Copy
                            } else {
                                D3dTextureBlend::SourceAlpha
                            },
                            depth_test: false,
                            depth_write: false,
                            texture: None,
                        },
                    );
                }
                None => {
                    let offset = (y as usize * self.width as usize + x as usize) * 4;
                    self.rgba[offset..offset + 4].copy_from_slice(&[red, green, blue, 0xff]);
                }
            }
        }
        outside
    }

    /// CPU pixels only: a device frame keeps none.
    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        (self.device.is_none() && x < self.width && y < self.height)
            .then_some(y as usize * self.width as usize + x as usize)
    }

    fn pixel_offset(&self, x: u32, y: u32) -> Option<usize> {
        self.pixel_index(x, y).map(|index| index * 4)
    }

    fn write_fragment(
        &mut self,
        x: u32,
        y: u32,
        depth: f32,
        rgba: [u8; 4],
        pipeline: &D3dRasterState,
        blend: D3dTextureBlend,
    ) -> bool {
        let Some(index) = self.pixel_index(x, y) else {
            return false;
        };
        if pipeline.depth_enabled && depth > self.depth[index] {
            return false;
        }
        let target = &mut self.rgba[index * 4..index * 4 + 4];
        for (target, source) in target.iter_mut().zip(rgba) {
            let destination = u32::from(*target);
            *target = match blend {
                D3dTextureBlend::Copy => source,
                D3dTextureBlend::Additive => target.saturating_add(source),
                D3dTextureBlend::InverseSourceColor => {
                    ((destination * (255 - u32::from(source)) + 127) / 255) as u8
                }
                D3dTextureBlend::SourceAlpha => {
                    ((u32::from(source) * u32::from(rgba[3])
                        + destination * (255 - u32::from(rgba[3]))
                        + 127)
                        / 255) as u8
                }
            };
        }
        if pipeline.depth_enabled && pipeline.depth_write {
            self.depth[index] = depth;
        }
        true
    }
}

/// Persistent device inputs not written by 004C12F0. Native enum values
/// are retained so unsupported W-buffer, compare, shade and cull policies
/// cannot accidentally select a convenient approximation.
#[derive(Clone, Copy, Debug)]
pub struct D3dRasterState {
    pub viewport: [i16; 4],
    pub depth_enabled: bool,
    pub depth_write: bool,
    pub depth_function: u32,
    pub alpha_blend_enabled: bool,
    pub shade_mode: u32,
    pub cull_mode: u32,
}

fn native_bool(field: &'static str, value: u32) -> Result<bool, SoftwarePixelError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(SoftwarePixelError::State { field, value }),
    }
}

#[path = "software_raster.rs"]
mod raster;

#[path = "software_textured.rs"]
mod textured;
pub use textured::{D3dTextureSampling, SoftwareTextureState};

#[derive(Debug, Default)]
pub struct SoftwarePixelStats {
    pub triangles: usize,
    pub solid_quads: usize,
    pub solid_triangles: usize,
    pub textured_triangles: usize,
    pub shared_shade_far_triangles: usize,
    pub textured_quads: usize,
    pub ordinary_far_quads: usize,
    pub textured_uv_triangles: usize,
    pub textured_uv_quads: usize,
    pub shared_shade_uv_triangles: usize,
    pub shared_shade_uv_quads: usize,
    pub effect_quads: usize,
    pub d2_triangles: usize,
    pub d6_quads: usize,
    pub textured_shade_uv_quads: usize,
    pub colored_quads: usize,
    pub da_triangles: usize,
    pub flat_shade_quads: usize,
    pub skipped: usize,
    pub fragments: usize,
    pub unsupported: usize,
    /// Commands the tracer drew in place of the raster.
    pub traced: usize,
}

/// A submission with a non-finite vertex. D3D draws no pixel of a fan with
/// no screen area, so its depth is never consumed. A textured primitive
/// whose every corner sits at packet depth 0, as an explosion's debris
/// sprite can, draws nothing as an approximation. Any other non-finite
/// vertex stops the queue.
pub(crate) fn depth_pole(
    vertices: &[D3dTlVertex],
    triangles: &[[usize; 3]],
    textured: bool,
) -> Result<usize, SoftwarePixelError> {
    if zero_screen_area(vertices, triangles) {
        return Ok(0);
    }
    if textured && vertices.iter().all(|v| v.at_depth_pole()) {
        log_depth_pole_approximation();
        return Ok(0);
    }
    Err(SoftwarePixelError::NonFinite)
}

/// [`PixelBackend`] for recovered solid/textured submissions and Near/Far
/// effect quads. Other callbacks explicitly stop the shared queue; remaining
/// far and subdivided consumers retain their separate ownership boundaries.
pub struct SoftwarePixelBackend<'f, 'r> {
    pub frame: &'f mut SoftwareFrame,
    pub stats: SoftwarePixelStats,
    pub renderer: D3dFlatState,
    pub pipeline: D3dRasterState,
    pub texture_state: SoftwareTextureState,
    pub textures: CpuTextureStore<'r>,
    /// The modern sky's land band the command drawn now is, its painted
    /// sky cut away (Vulkan's raster under its own sky).
    band: Option<crate::modern_tuning::BandTuning>,
}

impl<'f, 'r> SoftwarePixelBackend<'f, 'r> {
    pub fn new(
        frame: &'f mut SoftwareFrame,
        renderer: D3dFlatState,
        pipeline: D3dRasterState,
        texture_state: SoftwareTextureState,
    ) -> Self {
        Self {
            frame,
            stats: SoftwarePixelStats::default(),
            renderer,
            pipeline,
            texture_state,
            textures: CpuTextureStore::new(),
            band: None,
        }
    }

    /// The packet+20 `quad` will draw under ([`settle_effect_mode`]), for a
    /// producer that leaves +20 to the allocation's carry to check before
    /// it enqueues the quad.
    pub(crate) fn effect_mode(&mut self, quad: &EffectQuad<'r>) -> Result<u32, SoftwarePixelError> {
        let texture = self
            .textures
            .resolve(quad.resource, quad.sprite_mode, self.texture_state.upload)?
            .expect("selected descriptor was checked before upload");
        Ok(settle_effect_mode(
            quad,
            texture,
            self.frame.clip_bounds(self.pipeline.viewport),
        )?)
    }

    fn render_texture<const V: usize, const T: usize>(
        &mut self,
        resource: RenderResource<'r>,
        mode: u32,
        prepare: impl FnOnce(
            [f32; 4],
            D3dFlatState,
            SoftwareTextureState,
        ) -> Result<D3dTexturedSubmission<V, T>, SoftwarePixelError>,
    ) -> Result<usize, SoftwarePixelError> {
        let texture = match self.band {
            Some(band) => self.textures.resolve_band(
                resource,
                sprite_mode(mode),
                self.texture_state.upload,
                band,
            )?,
            None => {
                self.textures
                    .resolve(resource, sprite_mode(mode), self.texture_state.upload)?
            }
        }
        .expect("selected descriptor was checked before upload");
        let mut submission = prepare(texture.uv_bounds, self.renderer, self.texture_state)?;
        if self.band.is_some() {
            submission.policy.blend = D3dTextureBlend::SourceAlpha;
        }
        self.frame.fill_textured_submission(
            &submission,
            texture,
            &mut self.pipeline,
            self.texture_state.sampling,
        )
    }
}

impl<'r> PixelBackend<'r> for SoftwarePixelBackend<'_, 'r> {
    type Error = SoftwarePixelError;

    fn draw(
        &mut self,
        invocation: DrawInvocation,
        command: &mut PrimitiveCommand<'r>,
    ) -> Result<(), Self::Error> {
        // Under the raster's own modern sky the painting goes, but for its
        // land, laid over the sky by its cut's alpha, and the art it keeps.
        let band = match command.sky {
            Some(crate::scene_capture::RasterSkyDraw::Skip) => return Ok(()),
            Some(crate::scene_capture::RasterSkyDraw::Land(band)) => Some(band),
            _ => None,
        };
        self.band = band;
        self.frame.set_draw_band(band.is_some());
        // A moving model's fans carry its motion to a device following it,
        // and a flaring crystal's its tint.
        self.frame.set_draw_motion(command.motion);
        self.frame.set_draw_tint(command.tint);
        self.frame.set_draw_covered(command.covered);
        // A metal face's, whose metal the device mirrors the sky in.
        let metal = command.metal.is_some();
        if metal {
            self.frame.set_draw_metal(command.metal.clone());
        }
        self.frame.set_draw_water(command.water);
        // Retail's ground, which the device wets in the rain, with the
        // colour a far ring's fogged quad mixed into its corners.
        let ground = command.ground.then(|| match command.callback {
            PrimitiveCallback::ColoredQuadFar => read_colored_terrain(command)
                .ok()
                .and_then(|quad| colored_terrain_fog(&quad, self.renderer))
                .unwrap_or_default(),
            _ => [0; 3],
        });
        self.frame.set_draw_ground(ground);
        let drawn = self.draw_command(invocation, command);
        self.frame.set_draw_motion(None);
        self.frame.set_draw_tint(None);
        self.frame.set_draw_covered(false);
        if metal {
            self.frame.set_draw_metal(None);
        }
        self.frame.set_draw_water(false);
        self.frame.set_draw_ground(None);
        self.frame.set_draw_band(false);
        self.band = None;
        drawn
    }
}

impl<'r> SoftwarePixelBackend<'_, 'r> {
    fn draw_command(
        &mut self,
        invocation: DrawInvocation,
        command: &mut PrimitiveCommand<'r>,
    ) -> Result<(), SoftwarePixelError> {
        if invocation.receiver_offset != 0 {
            return Err(SoftwarePixelError::Receiver {
                offset: invocation.receiver_offset,
            });
        }
        // The tracer drew this primitive with the world.
        if command.scene_fed && self.frame.traced_world() {
            self.stats.traced += 1;
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::FlatQuadNear | PrimitiveCallback::FlatQuadFar
        ) {
            let quad = read_solid_quad(command)?;
            let submission = prepare_solid_quad(&quad, self.renderer);
            self.stats.fragments += self.frame.fill_flat_submission(
                &submission,
                self.renderer.near_depth,
                &mut self.pipeline,
            )?;
            self.stats.solid_quads += 1;
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::SolidTriangleNear | PrimitiveCallback::SolidTriangleFar
        ) {
            let triangle = read_solid_triangle(command)?;
            let submission = prepare_solid_triangle(&triangle, self.renderer);
            self.stats.fragments += self.frame.fill_flat_submission(
                &submission,
                self.renderer.near_depth,
                &mut self.pipeline,
            )?;
            self.stats.solid_triangles += 1;
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::SharedShadeUvTriangleNear
                | PrimitiveCallback::SharedShadeUvTriangleFar
                | PrimitiveCallback::SharedShadeUvQuadNear
                | PrimitiveCallback::SharedShadeUvQuadFar
        ) {
            let Some(primitive) = read_shared_shade_uv(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let texture = self
                .textures
                .resolve(
                    primitive.resource,
                    sprite_mode(primitive.mode),
                    self.texture_state.upload,
                )?
                .expect("selected descriptor was checked before upload");
            let submission = prepare_shared_shade_uv(
                &primitive,
                self.renderer,
                self.texture_state.upload.global_policy,
                self.texture_state.secondary_texture,
                texture.uv_bounds,
            );
            let fragments = match submission.primary {
                D3dTexturedUvSubmission::Triangle(submission) => {
                    let fragments = self.frame.fill_textured_submission(
                        &submission,
                        texture,
                        &mut self.pipeline,
                        self.texture_state.sampling,
                    )?;
                    self.stats.shared_shade_uv_triangles += 1;
                    fragments
                }
                D3dTexturedUvSubmission::Quad(submission) => {
                    let fragments = self.frame.fill_textured_submission(
                        &submission,
                        texture,
                        &mut self.pipeline,
                        self.texture_state.sampling,
                    )?;
                    self.stats.shared_shade_uv_quads += 1;
                    fragments
                }
            };
            self.stats.fragments += fragments;
            // The native primary fan(s) precede the descriptor0200/+44
            // secondary setup. A later boundary cannot erase this prefix.
            if submission.continuation == SharedShadeUvContinuation::SecondaryPassRequired {
                return Err(SharedShadeUvPixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::TexturedUvTriangleNear
                | PrimitiveCallback::TexturedUvTriangleFar
                | PrimitiveCallback::TexturedUvQuadNear
                | PrimitiveCallback::TexturedUvQuadFar
        ) {
            let Some(primitive) = read_textured_uv(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let texture = self
                .textures
                .resolve(
                    primitive.resource,
                    sprite_mode(primitive.mode),
                    self.texture_state.upload,
                )?
                .expect("selected descriptor was checked before upload");
            let submission = prepare_textured_uv(
                &primitive,
                self.renderer,
                self.texture_state.upload.global_policy,
                self.texture_state.secondary_texture,
                texture.uv_bounds,
            );
            let fragments = match submission.primary {
                D3dTexturedUvSubmission::Triangle(submission) => {
                    let fragments = self.frame.fill_textured_submission(
                        &submission,
                        texture,
                        &mut self.pipeline,
                        self.texture_state.sampling,
                    )?;
                    self.stats.textured_uv_triangles += 1;
                    fragments
                }
                D3dTexturedUvSubmission::Quad(submission) => {
                    let fragments = self.frame.fill_textured_submission(
                        &submission,
                        texture,
                        &mut self.pipeline,
                        self.texture_state.sampling,
                    )?;
                    self.stats.textured_uv_quads += 1;
                    fragments
                }
            };
            self.stats.fragments += fragments;
            if submission.continuation == TexturedUvContinuation::SecondaryPassRequired {
                return Err(TexturedPixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::TexturedTriangleNear | PrimitiveCallback::TexturedTriangleFar
        ) {
            let Some(triangle) = read_textured_triangle(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let texture = self
                .textures
                .resolve(
                    triangle.resource,
                    sprite_mode(triangle.mode),
                    self.texture_state.upload,
                )?
                .expect("selected descriptor was checked before upload");
            let submission = prepare_textured_triangle(
                &triangle,
                self.renderer,
                self.texture_state.upload.global_policy,
                self.texture_state.secondary_texture,
                texture.uv_bounds,
            );
            let fragments = self.frame.fill_textured_submission(
                &submission.primary,
                texture,
                &mut self.pipeline,
                self.texture_state.sampling,
            )?;
            self.stats.textured_triangles += 1;
            self.stats.fragments += fragments;
            if submission.continuation == TexturedTriangleContinuation::SecondaryPassRequired {
                return Err(TexturedPixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::SharedShadeTriangleNear | PrimitiveCallback::SharedShadeTriangleFar
        ) {
            let Some(triangle) = read_de_triangle(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let fragments = self.render_texture(
                triangle.resource,
                triangle.mode,
                |bounds, renderer, state| {
                    Ok(prepare_de_triangle(
                        &triangle,
                        renderer,
                        state.upload.global_policy,
                        state.secondary_texture,
                        bounds,
                    )?)
                },
            )?;
            self.stats.textured_triangles += 1;
            if command.callback == PrimitiveCallback::SharedShadeTriangleFar {
                self.stats.shared_shade_far_triangles += 1;
            }
            self.stats.fragments += fragments;
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::EffectQuadNear | PrimitiveCallback::EffectQuadFar
        ) {
            let Some(quad) = read_effect_quad(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let texture = self
                .textures
                .resolve(quad.resource, quad.sprite_mode, self.texture_state.upload)?
                .expect("selected descriptor was checked before upload");
            let mode = settle_effect_mode(
                &quad,
                texture,
                self.frame.clip_bounds(self.pipeline.viewport),
            )?;
            let submission = prepare_effect_quad(
                &quad,
                mode,
                self.renderer,
                self.texture_state.upload.global_policy,
                self.texture_state.secondary_texture,
                texture.uv_bounds,
            );
            let fragments = self.frame.fill_textured_submission(
                &submission.primary,
                texture,
                &mut self.pipeline,
                self.texture_state.sampling,
            )?;
            self.stats.effect_quads += 1;
            self.stats.fragments += fragments;
            // The descriptor0200/+44 branch commits its primary fan(s) before
            // secondary setup. Keep pixels, device state and stats on failure.
            if submission.continuation == EffectQuadContinuation::SecondaryPassRequired {
                return Err(EffectPixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::TexturedQuadNear | PrimitiveCallback::TexturedQuadFar
        ) {
            let Some(quad) = read_textured_quad(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let fragments =
                self.render_texture(quad.resource, quad.mode, |bounds, renderer, state| {
                    Ok(prepare_textured_quad(
                        &quad,
                        renderer,
                        state.upload.global_policy,
                        state.secondary_texture,
                        bounds,
                    )?)
                })?;
            self.stats.textured_quads += 1;
            if command.callback == PrimitiveCallback::TexturedQuadFar {
                self.stats.ordinary_far_quads += 1;
            }
            self.stats.fragments += fragments;
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::D2TriangleNear | PrimitiveCallback::D2TriangleFar
        ) {
            let Some(triangle) = read_d2(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let texture = self
                .textures
                .resolve(
                    triangle.resource,
                    sprite_mode(triangle.mode),
                    self.texture_state.upload,
                )?
                .expect("selected descriptor was checked before upload");
            let submission = prepare_d2(
                &triangle,
                self.renderer,
                self.texture_state.upload.global_policy,
                self.texture_state.secondary_texture,
                texture.uv_bounds,
            );
            let fragments = self.frame.fill_textured_submission(
                &submission.primary,
                texture,
                &mut self.pipeline,
                self.texture_state.sampling,
            )?;
            self.stats.d2_triangles += 1;
            self.stats.fragments += fragments;
            if submission.continuation == D2Continuation::SecondaryPassRequired {
                return Err(D2PixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::ColoredQuadNear | PrimitiveCallback::ColoredQuadFar
        ) {
            let quad = read_colored_terrain(command)?;
            let submission = prepare_colored_terrain(&quad, self.renderer);
            self.stats.fragments += self.frame.fill_flat_submission(
                &submission,
                self.renderer.near_depth,
                &mut self.pipeline,
            )?;
            self.stats.colored_quads += 1;
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::TexturedShadeUvQuadNear | PrimitiveCallback::TexturedShadeUvQuadFar
        ) {
            let Some(quad) = read_shade_uv_quad(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let texture = self
                .textures
                .resolve(
                    quad.resource,
                    sprite_mode(quad.mode),
                    self.texture_state.upload,
                )?
                .expect("selected descriptor checked before upload");
            let submission = prepare_shade_uv_quad(
                &quad,
                self.renderer,
                self.texture_state.upload.global_policy,
                self.texture_state.secondary_texture,
                texture.uv_bounds,
            );
            let fragments = self.frame.fill_textured_submission(
                &submission.primary,
                texture,
                &mut self.pipeline,
                self.texture_state.sampling,
            )?;
            self.stats.textured_shade_uv_quads += 1;
            self.stats.fragments += fragments;
            if submission.continuation == TexturedShadeUvContinuation::SecondaryPassRequired {
                return Err(TexturedShadeUvPixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::D6QuadNear | PrimitiveCallback::D6QuadFar
        ) {
            let Some(quad) = read_d6(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let texture = self
                .textures
                .resolve(
                    quad.resource,
                    sprite_mode(quad.mode),
                    self.texture_state.upload,
                )?
                .expect("selected descriptor was checked before upload");
            let submission = prepare_d6(
                &quad,
                self.renderer,
                self.texture_state.upload.global_policy,
                self.texture_state.secondary_texture,
                texture.uv_bounds,
            );
            let fragments = self.frame.fill_textured_submission(
                &submission.primary,
                texture,
                &mut self.pipeline,
                self.texture_state.sampling,
            )?;
            self.stats.d6_quads += 1;
            self.stats.fragments += fragments;
            if submission.continuation == D6QuadContinuation::SecondaryPassRequired {
                return Err(D6PixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::DaTriangleNear | PrimitiveCallback::DaTriangleFar
        ) {
            let Some(triangle) = read_da(command)? else {
                self.stats.skipped += 1;
                return Ok(());
            };
            let fragments = self.render_texture(
                triangle.resource,
                triangle.mode,
                |bounds, renderer, state| {
                    Ok(prepare_da(
                        &triangle,
                        renderer,
                        state.upload.global_policy,
                        bounds,
                    )?)
                },
            )?;
            self.stats.da_triangles += 1;
            self.stats.fragments += fragments;
            // 004C266F/004C3383: the primary draw precedes the variant
            // 0x200 + renderer +44 second pass.
            if triangle.descriptor_0200 && self.texture_state.secondary_texture {
                return Err(DaPixelError::SecondaryPass.into());
            }
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::SharedShadeQuadNear | PrimitiveCallback::SharedShadeQuadFar
        ) {
            let quad = read_shared_shade_quad(command)?;
            // 004D3260 pre-blends far fog into the corner colors like
            // 004C1940, with device fog off.
            let submission = prepare_flat_quad(&quad, self.renderer);
            self.stats.fragments += self.frame.fill_flat_submission(
                &submission,
                self.renderer.near_depth,
                &mut self.pipeline,
            )?;
            self.stats.flat_shade_quads += 1;
            return Ok(());
        }
        if matches!(
            command.callback,
            PrimitiveCallback::DcTriangleNear | PrimitiveCallback::DcTriangleFar
        ) {
            // 004C03E0/004C0A30 are 004C12F0/004C1940 with one shared shade.
            let (triangle, fog) = read_dc_triangle(command)?;
            let submission = match fog {
                Some(fog) => {
                    crate::d3d_flat::prepare_flat_triangle_far(&triangle, fog, self.renderer)
                }
                None => prepare_flat_triangle(&triangle, self.renderer),
            };
            self.stats.fragments += self.frame.fill_flat_submission(
                &submission,
                self.renderer.near_depth,
                &mut self.pipeline,
            )?;
            self.stats.triangles += 1;
            return Ok(());
        }
        if command.callback == PrimitiveCallback::FlatTriangleFar {
            // 004C1940 pre-blends fog into the corner colors with device fog
            // off, so the shared flat submission renders it exactly.
            let (triangle, fog) = read_flat_triangle_far(command)?;
            let submission =
                crate::d3d_flat::prepare_flat_triangle_far(&triangle, fog, self.renderer);
            let fragments = self.frame.fill_flat_submission(
                &submission,
                self.renderer.near_depth,
                &mut self.pipeline,
            )?;
            self.stats.triangles += 1;
            self.stats.fragments += fragments;
            return Ok(());
        }
        let triangle = read_flat_triangle(command).inspect_err(|error| {
            if matches!(error, SoftwarePixelError::Unsupported { .. }) {
                self.stats.unsupported += 1;
            }
        })?;
        let fragments =
            self.frame
                .fill_flat_triangle(&triangle, self.renderer, &mut self.pipeline)?;
        self.stats.triangles += 1;
        self.stats.fragments += fragments;
        Ok(())
    }
}

#[cfg(test)]
#[path = "software_frame_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "software_uv_corpus_tests.rs"]
mod uv_corpus_tests;

#[cfg(test)]
#[path = "software_raster_tests.rs"]
mod raster_tests;

#[cfg(test)]
#[path = "software_frame_corpus_tests.rs"]
mod corpus_tests;

#[cfg(test)]
#[path = "software_textured_tests.rs"]
mod textured_tests;

#[cfg(test)]
#[path = "software_textured_corpus_tests.rs"]
mod textured_corpus_tests;

#[cfg(test)]
#[path = "software_device_tests.rs"]
mod device_tests;
