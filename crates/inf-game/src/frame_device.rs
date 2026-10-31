//! The hardware devices a [`SoftwareFrame`](crate::software_frame::SoftwareFrame)
//! draws on in place of the CPU raster. The shared consumers prepare,
//! validate and clip the same TL fans and render states for every device;
//! each device only covers, samples and blends them. The CPU raster stays the
//! pixel reference.

use std::rc::Rc;

use sdl2::video::Window;
use thiserror::Error;

use crate::{
    d3d_texture::CpuTexture,
    d3d_textured::{D3dTextureBlend, D3dTextureFog},
    d3d_viewport::D3dTlVertex,
    gl_device::{GlDevice, GlDeviceError, GlFrame},
    presentation::{PresentationRect, RendererBackend},
    software_frame::D3dTextureSampling,
    vk_device::{VkDevice, VkDeviceError, VkFrame},
};

/// One submission's device state: what the CPU raster reads from the TL
/// fan, the persistent pipeline and the bound texture.
pub(crate) struct DeviceDraw<'t> {
    pub blend: D3dTextureBlend,
    pub depth_test: bool,
    pub depth_write: bool,
    pub texture: Option<DeviceTexturing<'t>>,
}

pub(crate) struct DeviceTexturing<'t> {
    pub texture: &'t CpuTexture,
    pub sampling: D3dTextureSampling,
    pub perspective: bool,
    pub fog: D3dTextureFog,
}

/// The vertex both devices draw: native position and depth, UV over its
/// reciprocal-depth divisor, diffuse, and fog colour with visibility.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceVertex {
    pub position: [f32; 3],
    pub texture: [f32; 3],
    pub diffuse: [u8; 4],
    pub fog: [u8; 4],
}

/// TL vertex to device vertex. Perspective submissions carry `uv*rhw, rhw`
/// so the fragment divides by the interpolated reciprocal depth, as the CPU
/// raster does; affine ones carry `uv, 1`. Fog colour rides with the vertex
/// so state changes do not split batches; visibility is specular alpha.
pub(crate) fn device_vertex(
    vertex: &D3dTlVertex,
    texturing: Option<&DeviceTexturing<'_>>,
) -> DeviceVertex {
    let (texture, fog) = match texturing {
        None => ([0.0, 0.0, 1.0], [0, 0, 0, 255]),
        Some(texturing) => {
            let q = if texturing.perspective {
                vertex.rhw
            } else {
                1.0
            };
            let fog = match texturing.fog {
                D3dTextureFog::Disabled => [0, 0, 0, 255],
                D3dTextureFog::Vertex { color } => {
                    [color[0], color[1], color[2], (vertex.specular >> 24) as u8]
                }
            };
            ([vertex.uv[0] * q, vertex.uv[1] * q, q], fog)
        }
    };
    DeviceVertex {
        position: [vertex.x, vertex.y, vertex.depth],
        texture,
        diffuse: vertex.diffuse,
        fog,
    }
}

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error(transparent)]
    OpenGl(#[from] GlDeviceError),
    #[error(transparent)]
    Vulkan(#[from] VkDeviceError),
}

/// A window's hardware device.
#[derive(Clone, Debug)]
pub enum Device {
    OpenGl(Rc<GlDevice>),
    Vulkan(Rc<VkDevice>),
}

impl Device {
    pub fn backend(&self) -> RendererBackend {
        match self {
            Self::OpenGl(_) => RendererBackend::OpenGl,
            Self::Vulkan(_) => RendererBackend::Vulkan,
        }
    }

    /// Settings `+3C`'s Filtering, for what the device samples itself: the
    /// Vulkan tracer's textures. OpenGL samples through the scene's state.
    pub fn set_filtering(&self, level: u32) {
        if let Self::Vulkan(device) = self {
            device.set_filtering(level);
        }
    }

    /// The player's modern mode, which only Vulkan draws.
    pub fn set_modern(&self, modern: crate::modern_settings::ModernSettings) {
        if let Self::Vulkan(device) = self {
            device.set_modern(modern);
        }
    }

    /// What the device runs of the modern mode; OpenGL shows no such rows.
    pub fn modern_support(&self) -> crate::modern_settings::ModernSupport {
        match self {
            Self::OpenGl(_) => crate::modern_settings::ModernSupport::default(),
            Self::Vulkan(device) => device.modern_support(),
        }
    }

    /// Driver and device names, for the startup log.
    pub fn description(&self) -> &str {
        match self {
            Self::OpenGl(device) => device.description(),
            Self::Vulkan(device) => device.description(),
        }
    }

    /// The window's presented pixels, top row first: call it after a
    /// frame's [`FrameDevice::present`] and before [`Self::swap`].
    pub(crate) fn read_output(&self, output: [u32; 2]) -> Vec<u8> {
        match self {
            Self::OpenGl(device) => device.read_output(output),
            Self::Vulkan(device) => device.read_output(output),
        }
    }

    /// A frame for `native`-pixel submissions drawn at `target` pixels.
    pub(crate) fn frame(
        &self,
        native: [u32; 2],
        target: [u32; 2],
        clear: [u8; 4],
    ) -> Result<FrameDevice, DeviceError> {
        Ok(match self {
            Self::OpenGl(device) => {
                FrameDevice::OpenGl(GlFrame::new(device, native, target, clear)?)
            }
            Self::Vulkan(device) => {
                FrameDevice::Vulkan(VkFrame::new(device, native, target, clear)?)
            }
        })
    }

    /// Show the presented frame in `window`.
    pub(crate) fn swap(&self, window: &Window) {
        match self {
            Self::OpenGl(_) => window.gl_swap_window(),
            Self::Vulkan(device) => device.swap(window),
        }
    }

    /// The window's drawable size in pixels.
    pub(crate) fn drawable_size(&self, window: &Window) -> [u32; 2] {
        match self {
            Self::OpenGl(_) => window.drawable_size().into(),
            Self::Vulkan(_) => VkDevice::drawable_size(window),
        }
    }
}

/// One frame's surfaces and pending draws on a device.
#[derive(Debug)]
pub enum FrameDevice {
    OpenGl(GlFrame),
    Vulkan(VkFrame),
}

impl FrameDevice {
    /// Append one clipped TL fan.
    pub(crate) fn fan(&mut self, polygon: &[D3dTlVertex], draw: &DeviceDraw<'_>) {
        match self {
            Self::OpenGl(frame) => frame.fan(polygon, draw),
            Self::Vulkan(frame) => frame.fan(polygon, draw),
        }
    }

    pub(crate) fn clear_color(&mut self, rgba: [u8; 4]) {
        match self {
            Self::OpenGl(frame) => frame.clear_color(rgba),
            Self::Vulkan(frame) => frame.clear_color(rgba),
        }
    }

    pub(crate) fn clear_depth(&mut self) {
        match self {
            Self::OpenGl(frame) => frame.clear_depth(),
            Self::Vulkan(frame) => frame.clear_depth(),
        }
    }

    /// The size of the drawn surface and of [`Self::read_rgba`]'s image.
    pub(crate) fn target(&self) -> [u32; 2] {
        match self {
            Self::OpenGl(frame) => frame.target(),
            Self::Vulkan(frame) => frame.target(),
        }
    }

    /// Replace every colour pixel with top-down native-sized RGBA, keeping
    /// depth.
    pub(crate) fn replace_surface(&mut self, rgba: &[u8]) {
        match self {
            Self::OpenGl(frame) => frame.replace_surface(rgba),
            Self::Vulkan(frame) => frame.replace_surface(rgba),
        }
    }

    /// The traced world's last draw is in: its lighting preset's grade over
    /// it, where the device traces (Vulkan).
    pub(crate) fn finish_traced_world(&mut self) {
        if let Self::Vulkan(frame) = self {
            frame.finish_traced_world();
        }
    }

    /// Thermal vision's camera over the world drawn so far, where the
    /// device has one (Vulkan); OpenGL keeps retail's green.
    pub(crate) fn finish_thermal(&mut self, ironbow: bool) {
        if let Self::Vulkan(frame) = self {
            frame.finish_thermal(ironbow);
        }
    }

    /// The world's draws begin, with `camera`, on a device that anti-aliases
    /// an untraced world (Vulkan): true when it follows the image over
    /// frames. The water mirrors the world with its least share
    /// `water_reflect` (none: lava, which does not), the eye under its live
    /// surface sees through it from below, through the `murk`, the
    /// world's `light` lights the device's own passes, and its modern sky,
    /// where `sky`, takes the painting's place where the device draws it.
    pub(crate) fn begin_raster_world(
        &mut self,
        camera: &crate::scene_feed::SceneCamera,
        water_reflect: Option<f32>,
        murk: Option<crate::vk_device::Murk>,
        light: Option<&crate::scene_feed::SceneFeed>,
        sky: bool,
    ) -> bool {
        match self {
            Self::OpenGl(_) => false,
            Self::Vulkan(frame) => {
                frame.begin_raster_world(camera, water_reflect, murk, light, sky)
            }
        }
    }

    /// The raster world's `light` again, with its lamps (Vulkan's water).
    pub(crate) fn raster_lamps(&mut self, light: &crate::scene_feed::SceneFeed) {
        if let Self::Vulkan(frame) = self {
            frame.raster_lamps(light);
        }
    }

    /// Whether the device shades an untraced world's water itself
    /// (Vulkan's raster, where it can).
    pub(crate) fn shades_water(&self) -> bool {
        match self {
            Self::OpenGl(_) => false,
            Self::Vulkan(frame) => frame.shades_water(),
        }
    }

    /// Whether this world's metal mirrors the sky (Vulkan's raster).
    pub(crate) fn reflects_metal(&self) -> bool {
        match self {
            Self::OpenGl(_) => false,
            Self::Vulkan(frame) => frame.reflects_metal(),
        }
    }

    /// The metal face the next fans draw, or none (Vulkan mirrors the sky
    /// in it).
    pub(crate) fn set_draw_metal(
        &mut self,
        metal: Option<std::rc::Rc<crate::raster_metal::MetalFace>>,
    ) {
        if let Self::Vulkan(frame) = self {
            frame.set_draw_metal(metal);
        }
    }

    /// Whether the device draws this world's modern sky (Vulkan).
    pub(crate) fn draws_raster_sky(&self) -> bool {
        match self {
            Self::OpenGl(_) => false,
            Self::Vulkan(frame) => frame.draws_raster_sky(),
        }
    }

    /// The painting is drawn: the device's modern sky goes behind it
    /// (Vulkan).
    pub(crate) fn draw_raster_sky(&mut self) {
        if let Self::Vulkan(frame) = self {
            frame.draw_raster_sky();
        }
    }

    /// Whether the next fans are the modern sky's land band (Vulkan).
    pub(crate) fn set_draw_band(&mut self, band: bool) {
        if let Self::Vulkan(frame) = self {
            frame.set_draw_band(band);
        }
    }

    /// The moving model the next fans come from, for a device following the
    /// image over frames (Vulkan).
    pub(crate) fn set_draw_motion(&mut self, motion: Option<crate::raster_motion::ViewMotion>) {
        if let Self::Vulkan(frame) = self {
            frame.set_draw_motion(motion);
        }
    }

    /// Whether the next fans are the water's surface (Vulkan mirrors the
    /// world in it).
    pub(crate) fn set_draw_water(&mut self, water: bool) {
        if let Self::Vulkan(frame) = self {
            frame.set_draw_water(water);
        }
    }

    /// Whether the next fans are retail's ground, which Vulkan wets in the
    /// rain, with the colour a fogged colored quad mixed into its corners.
    pub(crate) fn set_draw_ground(&mut self, ground: Option<[u8; 3]>) {
        if let Self::Vulkan(frame) = self {
            frame.set_draw_ground(ground);
        }
    }

    /// The next fans are covered by a traced face drawn after them over the
    /// same corners (Vulkan).
    pub(crate) fn set_draw_covered(&mut self, covered: bool) {
        if let Self::Vulkan(frame) = self {
            frame.set_draw_covered(covered);
        }
    }

    /// The colour the next fans are laid in again, added (Vulkan).
    pub(crate) fn set_draw_tint(&mut self, tint: Option<crate::crystal_flares::CrystalSpark>) {
        if let Self::Vulkan(frame) = self {
            frame.set_draw_tint(tint);
        }
    }

    /// A traced world's camera while its draws come in, or none once it is
    /// done (Vulkan): its raster crystals find their spark with it.
    pub(crate) fn set_world_camera(&mut self, camera: Option<crate::scene_feed::SceneCamera>) {
        if let Self::Vulkan(frame) = self {
            frame.set_world_camera(camera);
        }
    }

    /// The world's camera while its draws come in (Vulkan), else none.
    pub(crate) fn world_camera(&self) -> Option<crate::scene_feed::SceneCamera> {
        match self {
            Self::OpenGl(_) => None,
            Self::Vulkan(frame) => frame.world_camera(),
        }
    }

    /// The world's last draw is in: anti-alias it where the device does.
    pub(crate) fn finish_raster_world(&mut self) {
        if let Self::Vulkan(frame) = self {
            frame.finish_raster_world();
        }
    }

    /// Whether the device traces the world (the modern mode).
    pub(crate) fn traces_rays(&self) -> bool {
        match self {
            Self::OpenGl(_) => false,
            Self::Vulkan(frame) => frame.traces_rays(),
        }
    }

    /// Whether the world drawn next is under infrared vision, which a
    /// device that traces rays traces whatever the player's Ray Tracing.
    pub(crate) fn set_trace_infrared(&mut self, on: bool) {
        if let Self::Vulkan(frame) = self {
            frame.set_trace_infrared(on);
        }
    }

    /// Make the tracer and its upscaler ahead of the first traced frame.
    pub(crate) fn prepare_trace(&mut self) {
        if let Self::Vulkan(frame) = self {
            frame.prepare_trace();
        }
    }

    /// Trace `feed` into the frame, where the device traces rays: true when
    /// it did.
    pub(crate) fn trace_world(
        &mut self,
        feed: &crate::scene_feed::SceneFeed,
        textures: &[&CpuTexture],
    ) -> bool {
        match self {
            Self::OpenGl(_) => false,
            Self::Vulkan(frame) => frame.trace_world(feed, textures),
        }
    }

    /// The frame's pixels at the target size, top row first.
    pub(crate) fn read_rgba(&mut self) -> Vec<u8> {
        match self {
            Self::OpenGl(frame) => frame.read_rgba(),
            Self::Vulkan(frame) => frame.read_rgba(),
        }
    }

    /// The frame's pixels for a snapshot: OpenGL's read now, Vulkan's
    /// copied on the device and read back when the snapshot is saved.
    pub(crate) fn stage_rgba(&mut self) -> Result<Vec<u8>, crate::vk_device::StagedRgba> {
        match self {
            Self::OpenGl(frame) => Ok(frame.read_rgba()),
            Self::Vulkan(frame) => Err(frame.stage_rgba()),
        }
    }

    /// Draw the completed frame into `destination` of the window's output,
    /// whose drawable size is `output`, with black bars.
    pub(crate) fn present(&mut self, output: [u32; 2], destination: PresentationRect) {
        match self {
            Self::OpenGl(frame) => frame.present(output, destination),
            Self::Vulkan(frame) => frame.present(output, destination),
        }
    }
}
