//! Anti-aliasing for the raster's world on the Vulkan device, when no rays
//! are traced ([the plan](../../../../docs/INFESTATION-RT-IMPLEMENTATION.md#settings-and-menus)):
//! the Anti-aliasing row's FXAA and TAA, and the upscalers at the player's
//! quality. An upscaler's world is drawn at its render size, at the
//! frame's top left (Native AA: the whole frame), and brought up to the
//! frame. For TAA and the upscalers the world's draws move a sub-pixel each
//! frame (a Halton 2,3 jitter, as the tracer's). After the world's last
//! draw and before the HUD, a pass turns the drawn colour and depth into
//! the inputs the tracer gives them, with each pixel's motion, and the
//! result goes back into the frame. A pixel moves with the camera, unless a
//! moving model drew it: its solid triangles are drawn again over the
//! world's depth, each corner carrying its own motion
//! ([`crate::raster_motion`]).

use super::trace::{
    bytes_of, compute_pipeline, frame_barrier, halton, pass_barrier, shader_module, FXAA,
    LIGHT_IMAGE, MOTION_IMAGE, TAA, TRACED_DEPTH,
};
use super::upscale::{UpscaleFrame, Upscaler, UpscalerChoice};
use super::*;
use crate::modern_settings::{AntiAliasing, ModernSettings};
use crate::scene_feed::SceneCamera;

static PREPARE: &[u8] = include_bytes!("../../shaders/vk_raster_prepare.comp.spv");
pub(super) static MOTION_VERTEX: &[u8] = include_bytes!("../../shaders/vk_motion.vert.spv");
pub(super) static MOTION_FRAGMENT: &[u8] = include_bytes!("../../shaders/vk_motion.frag.spv");

/// A moving model's corner for the motion pass: where the raster draws it
/// (native position and depth), and where its point was in the last frame
/// less where it is now, in native pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MotionVertex {
    position: [f32; 3],
    motion: [f32; 2],
}

/// A water corner for the mirror's mask, drawn by the motion pass's
/// pipeline: in place of a motion, its reciprocal depth scaled into half
/// floats' range ([`super::mirror`]) and its fog's visibility (`fog`, the
/// specular alpha retail's fog takes).
pub(super) fn water_vertex(vertex: &D3dTlVertex, fog: u8) -> MotionVertex {
    MotionVertex {
        position: [vertex.x, vertex.y, vertex.depth],
        motion: [
            vertex.rhw * super::mirror::RHW_SCALE,
            f32::from(fog) / 255.0,
        ],
    }
}

/// A raster vertex's point from the eye in world axes (Y up), taken back
/// through `camera`'s projection; none behind the eye.
pub(super) fn view_point(vertex: &D3dTlVertex, camera: &SceneCamera) -> Option<[f32; 3]> {
    let view = camera_point(vertex, camera)?;
    Some(std::array::from_fn(|axis| {
        (0..3).map(|row| camera.basis[row][axis] * view[row]).sum()
    }))
}

/// [`view_point`] in the camera's own axes (X right, Y up, Z ahead), as a
/// model's slots lie in its view.
pub(super) fn camera_point(vertex: &D3dTlVertex, camera: &SceneCamera) -> Option<[f32; 3]> {
    if !(vertex.rhw.is_finite() && vertex.rhw > 0.0) {
        return None;
    }
    let depth = 1.0 / vertex.rhw;
    Some([
        (vertex.x - camera.centre[0]) * depth / camera.focal[0],
        (camera.centre[1] - vertex.y) * depth / camera.focal[1],
        depth,
    ])
}

/// `vertex` of a moving model as the motion pass takes it: its point in
/// `now`'s view, from its native position and reciprocal depth, carried by
/// `motion` into the last view and seen through `last`'s projection. None
/// for a corner with no depth, or at or behind the last view's eye.
pub(super) fn motion_vertex(
    vertex: &D3dTlVertex,
    now: &SceneCamera,
    last: &SceneCamera,
    motion: &crate::raster_motion::ViewMotion,
) -> Option<MotionVertex> {
    if !(vertex.rhw.is_finite() && vertex.rhw > 0.0) {
        return None;
    }
    let depth = 1.0 / f64::from(vertex.rhw);
    let [x, y] = [f64::from(vertex.x), f64::from(vertex.y)];
    let view = [
        (x - f64::from(now.centre[0])) * depth / f64::from(now.focal[0]),
        (f64::from(now.centre[1]) - y) * depth / f64::from(now.focal[1]),
        depth,
    ];
    let then: [f64; 3] = std::array::from_fn(|row| {
        (0..3)
            .map(|k| f64::from(motion[row][k]) * view[k])
            .sum::<f64>()
            + f64::from(motion[row][3])
    });
    if then[2] < 1.0 {
        return None;
    }
    let was = [
        f64::from(last.centre[0]) + f64::from(last.focal[0]) * then[0] / then[2],
        f64::from(last.centre[1]) - f64::from(last.focal[1]) * then[1] / then[2],
    ];
    Some(MotionVertex {
        position: [vertex.x, vertex.y, vertex.depth],
        motion: [(was[0] - x) as f32, (was[1] - y) as f32],
    })
}

/// A raster world begun under the player's anti-aliasing.
#[derive(Clone, Copy)]
pub(super) struct RasterBegin {
    /// Its draws' offset this frame, in the pixels it is drawn at.
    pub offset: [f32; 2],
    /// The image is followed over frames (TAA, the upscalers).
    pub follows: bool,
    /// The last frame's camera, when that frame drew a world followed so.
    pub last: Option<SceneCamera>,
}

/// The prepare pass's push constants, in `vk_raster_prepare.comp`'s order.
const PREPARE_FLOATS: usize = 28;

/// The jitter's phases for a world drawn at `render` and shown at
/// `display`, as the tracer's: `ceil(8 ratio^2)`, 8 at the native size.
fn jitter_phases(display: [u32; 2], render: [u32; 2]) -> u32 {
    let ratio = display[0] as f32 / render[0].max(1) as f32;
    ((8.0 * ratio * ratio).ceil() as u32).max(8)
}

/// The view a world was drawn with, for the next frame's motion.
#[derive(Clone, Copy)]
struct RasterView {
    camera: SceneCamera,
    extent: [u32; 2],
    /// The presentation it was drawn for.
    present: u64,
}

pub(super) struct RasterAa {
    prepare_set_layout: vk::DescriptorSetLayout,
    prepare_layout: vk::PipelineLayout,
    prepare_module: vk::ShaderModule,
    prepare: vk::Pipeline,
    aa_set_layout: vk::DescriptorSetLayout,
    aa_layout: vk::PipelineLayout,
    fxaa_module: vk::ShaderModule,
    fxaa: vk::Pipeline,
    taa_module: vk::ShaderModule,
    taa: vk::Pipeline,
    /// The pass writing moving models' own motion.
    motion_vertex: vk::ShaderModule,
    motion_fragment: vk::ShaderModule,
    motion_pipeline: vk::Pipeline,
    /// A frame's three sets, reset each use: the prepare pass's, the
    /// anti-aliasing's, and the result's for the copy back. One pool for
    /// each frame in flight: the device records the next frame while the
    /// GPU still anti-aliases the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    /// The frame's colour, depth and motion, as the tracer's resolve leaves
    /// its own.
    color: Option<Image>,
    depth: Option<Image>,
    motion: Option<Image>,
    /// Moving models' own motion in native pixels, and in alpha where they
    /// drew it.
    objects: Option<Image>,
    /// The anti-aliased colour by `parity` (TAA's history).
    history: [Option<Image>; 2],
    /// The upscaler and its output.
    upscaler: Option<Upscaler>,
    upscaled: Option<Image>,
    /// The size this frame's world is drawn at, between its first and last
    /// draw.
    render: Option<[u32; 2]>,
    /// Upscalers that failed (each logged once): chosen again, the world
    /// stays as drawn.
    failed: Vec<crate::modern_settings::Upscaler>,
    jitter_index: u32,
    parity: usize,
    previous: Option<RasterView>,
    /// The size TAA last wrote its history at; none when the last frame
    /// ran none.
    taa_extent: Option<[u32; 2]>,
    /// SMAA's passes, made on its first use.
    smaa: Option<super::smaa::Smaa>,
    /// When the last world finished, for the upscalers' frame time.
    last_frame: Option<std::time::Instant>,
}

impl RasterAa {
    pub(super) unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut aa = Self {
            prepare_set_layout: vk::DescriptorSetLayout::null(),
            prepare_layout: vk::PipelineLayout::null(),
            prepare_module: vk::ShaderModule::null(),
            prepare: vk::Pipeline::null(),
            aa_set_layout: vk::DescriptorSetLayout::null(),
            aa_layout: vk::PipelineLayout::null(),
            fxaa_module: vk::ShaderModule::null(),
            fxaa: vk::Pipeline::null(),
            taa_module: vk::ShaderModule::null(),
            taa: vk::Pipeline::null(),
            motion_vertex: vk::ShaderModule::null(),
            motion_fragment: vk::ShaderModule::null(),
            motion_pipeline: vk::Pipeline::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            color: None,
            depth: None,
            motion: None,
            objects: None,
            history: [None, None],
            upscaler: None,
            upscaled: None,
            failed: Vec::new(),
            jitter_index: 0,
            parity: 0,
            previous: None,
            taa_extent: None,
            render: None,
            smaa: None,
            last_frame: None,
        };
        match aa.build(core) {
            Ok(()) => Ok(aa),
            Err(error) => {
                aa.destroy(core, &mut None);
                Err(error)
            }
        }
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        let compute = |index: u32, kind: vk::DescriptorType| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(index)
                .descriptor_type(kind)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        };
        // The frame's colour and depth, then the colour, depth and motion
        // the upscalers take, and the moving models' own motion.
        let prepare_bindings = [
            compute(0, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            compute(1, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            compute(2, vk::DescriptorType::STORAGE_IMAGE),
            compute(3, vk::DescriptorType::STORAGE_IMAGE),
            compute(4, vk::DescriptorType::STORAGE_IMAGE),
            compute(5, vk::DescriptorType::STORAGE_IMAGE),
        ];
        // The tracer's anti-aliasing bindings, less its frame block: the
        // colour, its motion, the last frame's anti-aliased colour and this
        // frame's.
        let aa_bindings =
            [1, 2, 3, 4].map(|index| compute(index, vk::DescriptorType::STORAGE_IMAGE));
        for (bindings, set_layout) in [
            (&prepare_bindings[..], &mut self.prepare_set_layout),
            (&aa_bindings[..], &mut self.aa_set_layout),
        ] {
            *set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings),
                    None,
                )
                .map_err(call("raster anti-aliasing descriptor layout"))?;
        }
        for (set_layout, size, layout) in [
            (
                self.prepare_set_layout,
                PREPARE_FLOATS * 4,
                &mut self.prepare_layout,
            ),
            (self.aa_set_layout, 4, &mut self.aa_layout),
        ] {
            let sets = [set_layout];
            let push = [vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .size(size as u32)];
            *layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&sets)
                        .push_constant_ranges(&push),
                    None,
                )
                .map_err(call("raster anti-aliasing pipeline layout"))?;
        }
        self.prepare_module = shader_module(core, PREPARE, "raster prepare shader")?;
        self.prepare = compute_pipeline(core, self.prepare_layout, self.prepare_module)?;
        self.fxaa_module = shader_module(core, FXAA, "FXAA shader")?;
        self.fxaa = compute_pipeline(core, self.aa_layout, self.fxaa_module)?;
        self.taa_module = shader_module(core, TAA, "TAA shader")?;
        self.taa = compute_pipeline(core, self.aa_layout, self.taa_module)?;
        self.motion_vertex = shader_module(core, MOTION_VERTEX, "motion vertex shader")?;
        self.motion_fragment = shader_module(core, MOTION_FRAGMENT, "motion fragment shader")?;
        self.motion_pipeline = motion_pipeline(core, self.motion_vertex, self.motion_fragment)?;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(3),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(8),
        ];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(3)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("raster anti-aliasing descriptor pool"))?;
        }
        Ok(())
    }

    /// Release everything, freeing memory through `allocator` when given;
    /// the device must be past its last use.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        if let Some(upscaler) = self.upscaler.take() {
            upscaler.destroy(core);
        }
        if let Some(smaa) = self.smaa.take() {
            smaa.destroy(core, allocator);
        }
        let [first, second] = self.history;
        for image in [
            self.color,
            self.depth,
            self.motion,
            self.objects,
            first,
            second,
            self.upscaled,
        ]
        .into_iter()
        .flatten()
        {
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            if let (Some(allocation), Some(allocator)) = (image.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for pool in self.pool {
            device.destroy_descriptor_pool(pool, None);
        }
        for pipeline in [self.prepare, self.fxaa, self.taa, self.motion_pipeline] {
            device.destroy_pipeline(pipeline, None);
        }
        for layout in [self.prepare_layout, self.aa_layout] {
            device.destroy_pipeline_layout(layout, None);
        }
        for set_layout in [self.prepare_set_layout, self.aa_set_layout] {
            device.destroy_descriptor_set_layout(set_layout, None);
        }
        for module in [
            self.prepare_module,
            self.fxaa_module,
            self.taa_module,
            self.motion_vertex,
            self.motion_fragment,
        ] {
            device.destroy_shader_module(module, None);
        }
    }

    /// `upscaler` is out for the rest of the run: say why, once.
    fn unavailable(&mut self, upscaler: crate::modern_settings::Upscaler, why: &str) {
        if !self.failed.contains(&upscaler) {
            self.failed.push(upscaler);
            eprintln!(
                "Infestation: the {upscaler:?} upscaler is unavailable over the raster's world: {why}"
            );
        }
    }

    /// The upscaler `modern` runs over the raster, at its quality: none
    /// without one, or once it failed.
    fn choice(&self, modern: ModernSettings) -> UpscalerChoice {
        if self.failed.contains(&modern.upscaler) {
            return UpscalerChoice::Off;
        }
        UpscalerChoice::from_settings(modern)
    }
}

/// Whether the raster's world is anti-aliased with `upscaling` (an
/// upscaler runs) and `modern`'s Anti-aliasing pass, and whether its draws
/// are jittered for it: TAA and the upscalers follow the image over frames.
/// Supersampling runs no pass: the whole frame is drawn larger.
fn raster_plan(upscaling: bool, modern: ModernSettings) -> Option<bool> {
    (upscaling || modern.anti_aliasing.pass() != AntiAliasing::Off)
        .then_some(upscaling || modern.anti_aliasing == AntiAliasing::Taa)
}

impl State {
    fn raster_aa(&mut self, core: &Core) -> Result<Box<RasterAa>, VkDeviceError> {
        match self.raster_aa.take() {
            Some(aa) => Ok(aa),
            // SAFETY: the pipelines are made on the device's own handles.
            None => Ok(Box::new(unsafe { RasterAa::new(core)? })),
        }
    }

    /// Begin a raster world under the player's anti-aliasing, shown at
    /// `display`: none when it has none, else this frame's offset of the
    /// world's draws in the pixels it is drawn at (the jitter's next phase,
    /// or none for FXAA), and whether the image is followed over frames. An
    /// upscaler's world is drawn at its render size, at the frame's top
    /// left.
    pub(super) fn begin_raster(
        &mut self,
        core: &Core,
        display: [u32; 2],
    ) -> Result<Option<RasterBegin>, VkDeviceError> {
        let modern = self.modern;
        if raster_plan(
            modern.upscaler != crate::modern_settings::Upscaler::Off,
            modern,
        )
        .is_none()
        {
            return Ok(None);
        }
        let mut aa = self.raster_aa(core)?;
        let choice = aa.choice(modern);
        let mut upscaling = choice != UpscalerChoice::Off;
        let mut render = display;
        if upscaling {
            // SAFETY: the query runs on the device's own handles.
            match unsafe { Upscaler::render_size(core, choice, display) } {
                Ok(size) => render = size.unwrap_or(display),
                Err(error) => {
                    aa.unavailable(modern.upscaler, &error);
                    upscaling = false;
                }
            }
        }
        let phases = jitter_phases(display, render);
        let plan = raster_plan(upscaling, modern);
        let offset = plan.map(|jittered| {
            if !jittered {
                return [0.0; 2];
            }
            aa.jitter_index = aa.jitter_index.wrapping_add(1);
            let index = aa.jitter_index % phases + 1;
            [halton(index, 2) - 0.5, halton(index, 3) - 0.5]
        });
        aa.render = offset.map(|_| render);
        // The last frame's view, as the finish follows it.
        let last = aa
            .previous
            .filter(|previous| previous.present + 1 == self.presents)
            .map(|previous| previous.camera);
        self.raster_aa = Some(aa);
        let smaller = (render != display && offset.is_some()).then_some(render);
        if smaller.is_some() {
            // The world's next draw begins its rendering at its own size.
            // SAFETY: ending the rendering records on the device's buffer.
            unsafe { self.end_rendering(core) };
        }
        self.raster_viewport = smaller;
        let follows = plan == Some(true);
        Ok(offset.map(|offset| RasterBegin {
            offset,
            follows,
            last: last.filter(|_| follows),
        }))
    }

    /// Anti-alias the world the raster drew into `target` with `camera`,
    /// its draws moved by `offset` and its moving models' triangles
    /// `moving`; the HUD is not drawn yet.
    pub(super) unsafe fn finish_raster(
        &mut self,
        core: &Core,
        target: &Target,
        camera: &SceneCamera,
        offset: [f32; 2],
        moving: &[MotionVertex],
    ) -> Result<(), VkDeviceError> {
        let mut aa = self.raster_aa(core)?;
        let result = self.finish_raster_with(core, &mut aa, target, camera, offset, moving);
        self.raster_aa = Some(aa);
        result
    }

    unsafe fn finish_raster_with(
        &mut self,
        core: &Core,
        aa: &mut RasterAa,
        target: &Target,
        camera: &SceneCamera,
        offset: [f32; 2],
        moving: &[MotionVertex],
    ) -> Result<(), VkDeviceError> {
        let modern = self.modern;
        let display = target.color.extent;
        let command = self.begin(core)?;
        self.end_rendering(core);
        // The device no longer waits for the frames in flight before this
        // one: what they wrote (the upscalers' own images among it) comes
        // before this frame's passes.
        frame_barrier(core, command);
        // The world's draws are in; what follows draws on the whole frame.
        self.raster_viewport = None;
        let render = aa.render.take().unwrap_or(display);
        let view = RasterView {
            camera: *camera,
            extent: render,
            present: self.presents,
        };
        let previous = aa.previous.take().filter(|previous| {
            previous.extent == render
                && previous.camera.native == camera.native
                && previous.present + 1 == self.presents
        });
        aa.previous = Some(view);
        let began = std::time::Instant::now();
        let frame_ms = aa
            .last_frame
            .replace(began)
            .map_or(1000.0 / 60.0, |then| (began - then).as_secs_f32() * 1000.0);

        // The images: the inputs, then the upscaler's output or TAA's
        // history.
        self.traced_image(core, &mut aa.color, LIGHT_IMAGE, render)?;
        self.traced_image(core, &mut aa.depth, TRACED_DEPTH, render)?;
        self.traced_image(core, &mut aa.motion, MOTION_IMAGE, render)?;
        let choice = aa.choice(modern);
        let mut upscaling = false;
        if choice != UpscalerChoice::Off {
            if aa
                .upscaler
                .as_ref()
                .is_some_and(|upscaler| !upscaler.fits(choice, display, render))
            {
                if let Some(old) = aa.upscaler.take() {
                    old.destroy(core);
                }
            }
            if aa.upscaler.is_none() {
                match Upscaler::new(core, command, choice, display, render) {
                    Ok(upscaler) => aa.upscaler = Some(upscaler),
                    Err(error) => aa.unavailable(modern.upscaler, &error),
                }
            }
            if aa.upscaler.is_some() {
                self.upscaled_image(core, &mut aa.upscaled, display)?;
                upscaling = true;
            }
        }
        let anti_aliasing = if choice == UpscalerChoice::Off {
            modern.anti_aliasing.pass()
        } else {
            AntiAliasing::Off
        };
        if anti_aliasing != AntiAliasing::Off {
            for slot in aa.history.iter_mut() {
                self.traced_image(core, slot, LIGHT_IMAGE, display)?;
            }
        }
        // An upscaler that failed after its world was drawn smaller: that
        // world is stretched over the frame, this once.
        let stretch = !upscaling && render != display;
        if !upscaling && anti_aliasing == AntiAliasing::Off && !stretch {
            // An upscaler that failed this frame: the world stays as drawn.
            aa.taa_extent = None;
            return Ok(());
        }
        fn made(slot: &Option<Image>) -> &Image {
            slot.as_ref().expect("made above")
        }
        let [now, before] = [aa.parity, aa.parity ^ 1];
        aa.parity ^= 1;
        let color = made(&aa.color);
        let depth = made(&aa.depth);
        let motion = made(&aa.motion);
        for image in [color, depth, motion] {
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }
        let result = if upscaling {
            made(&aa.upscaled)
        } else if stretch {
            color
        } else {
            made(&aa.history[now])
        };
        transition(core, command, result, vk::ImageLayout::GENERAL);
        if anti_aliasing != AntiAliasing::Off {
            transition(
                core,
                command,
                made(&aa.history[before]),
                vk::ImageLayout::GENERAL,
            );
        }
        // The moving models' own motion, where the frame's depth shows them.
        self.storage_image(
            core,
            &mut aa.objects,
            LIGHT_IMAGE,
            render,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
        )?;
        let objects = aa.objects.as_ref().expect("made above");
        self.draw_corners(
            core,
            objects,
            aa.motion_pipeline,
            target,
            camera.native,
            render,
            offset,
            moving,
        )?;
        // The frame's colour and depth are read from here on.
        for image in [&target.color, &target.depth] {
            transition(
                core,
                command,
                image,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }

        // The sets: the prepare pass's, the anti-aliasing's, and the
        // result's for the copy back.
        let pool = aa.pool[self.slot];
        core.device
            .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::empty())
            .map_err(call("raster anti-aliasing descriptor reset"))?;
        let layouts = [aa.prepare_set_layout, aa.aa_set_layout, core.texture_layout];
        let [prepare_set, aa_set, result_set]: [vk::DescriptorSet; 3] = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
            .map_err(call("raster anti-aliasing descriptor sets"))?
            .try_into()
            .expect("three layouts give three sets");
        let sampled = |image: &Image, layout: vk::ImageLayout| {
            [vk::DescriptorImageInfo::default()
                .sampler(core.samplers[BLIT_SAMPLER])
                .image_view(image.view)
                .image_layout(layout)]
        };
        let storage = |image: &Image| {
            [vk::DescriptorImageInfo::default()
                .image_view(image.view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        let frame_color = sampled(&target.color, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let frame_depth = sampled(&target.depth, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let result_info = sampled(result, vk::ImageLayout::GENERAL);
        let [color_info, depth_info, motion_info, objects_info] =
            [color, depth, motion, made(&aa.objects)].map(storage);
        let history_infos = if anti_aliasing == AntiAliasing::Off {
            None
        } else {
            Some([made(&aa.history[before]), made(&aa.history[now])].map(storage))
        };
        let mut writes = vec![
            image_write(
                prepare_set,
                0,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                &frame_color,
            ),
            image_write(
                prepare_set,
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                &frame_depth,
            ),
            image_write(
                prepare_set,
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &color_info,
            ),
            image_write(
                prepare_set,
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &depth_info,
            ),
            image_write(
                prepare_set,
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &motion_info,
            ),
            image_write(
                prepare_set,
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &objects_info,
            ),
            image_write(
                result_set,
                0,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                &result_info,
            ),
        ];
        if let Some([history_info, out_info]) = &history_infos {
            writes.extend([
                image_write(aa_set, 1, vk::DescriptorType::STORAGE_IMAGE, &color_info),
                image_write(aa_set, 2, vk::DescriptorType::STORAGE_IMAGE, &motion_info),
                image_write(aa_set, 3, vk::DescriptorType::STORAGE_IMAGE, history_info),
                image_write(aa_set, 4, vk::DescriptorType::STORAGE_IMAGE, out_info),
            ]);
        }
        core.device.update_descriptor_sets(&writes, &[]);

        // The colour, depth and motion.
        let push = prepare_constants(camera, previous.as_ref(), render, offset);
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, aa.prepare);
        core.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::COMPUTE,
            aa.prepare_layout,
            0,
            &[prepare_set],
            &[],
        );
        core.device.cmd_push_constants(
            command,
            aa.prepare_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes_of(&push),
        );
        let groups = render.map(|side| side.div_ceil(8));
        core.device.cmd_dispatch(command, groups[0], groups[1], 1);
        pass_barrier(core, command);
        if anti_aliasing == AntiAliasing::Smaa {
            if aa.smaa.is_none() {
                aa.smaa = Some(super::smaa::Smaa::new(core)?);
            }
            let smaa = aa.smaa.as_mut().expect("made above");
            smaa.run(self, core, color, made(&aa.history[now]), display)?;
            aa.taa_extent = None;
        } else if anti_aliasing != AntiAliasing::Off {
            let taa = anti_aliasing == AntiAliasing::Taa;
            core.device.cmd_bind_pipeline(
                command,
                vk::PipelineBindPoint::COMPUTE,
                if taa { aa.taa } else { aa.fxaa },
            );
            core.device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::COMPUTE,
                aa.aa_layout,
                0,
                &[aa_set],
                &[],
            );
            let fresh = i32::from(aa.taa_extent != Some(display) || previous.is_none());
            core.device.cmd_push_constants(
                command,
                aa.aa_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytes_of(&[fresh]),
            );
            core.device.cmd_dispatch(command, groups[0], groups[1], 1);
            pass_barrier(core, command);
            aa.taa_extent = taa.then_some(display);
        } else {
            aa.taa_extent = None;
        }
        if let (Some(upscaler), true) = (aa.upscaler.as_mut(), upscaling) {
            let frame = UpscaleFrame {
                color,
                depth,
                motion,
                output: result,
                // The draws moved the image, as the upscalers' jitter means.
                jitter: offset,
                reset: previous.is_none(),
                near: camera.near,
                vertical_fov: 2.0 * (camera.native[1] as f32 * 0.5 / camera.focal[1]).atan(),
                frame_ms,
                guides: None,
            };
            let dispatched = upscaler.dispatch(core, command, &frame);
            pass_barrier(core, command);
            if let Err(error) = dispatched {
                // Its output holds nothing: the world stays as drawn.
                aa.unavailable(modern.upscaler, &error);
                return Ok(());
            }
        }

        // The result back into the frame, under the HUD's draws.
        self.render_to(core, target)?;
        blit(
            core,
            self.blit_target,
            Descriptor {
                pool,
                set: result_set,
            },
            true,
        );
        Ok(())
    }
}

impl State {
    /// Clear `objects` and draw `moving` into it with `pipeline` (the
    /// motion pass's), over the depth of the world drawn at `render` in
    /// `target`, with its draws' `offset` and `native` projection, as the
    /// raster drew them: the moving models' motion, or the water's mask.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn draw_corners(
        &mut self,
        core: &Core,
        objects: &Image,
        pipeline: vk::Pipeline,
        target: &Target,
        native: [u32; 2],
        render: [u32; 2],
        offset: [f32; 2],
        moving: &[MotionVertex],
    ) -> Result<(), VkDeviceError> {
        let command = self.begin(core)?;
        transition(
            core,
            command,
            objects,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        transition(
            core,
            command,
            &target.depth,
            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
        );
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(objects.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0; 4] },
            })];
        let depth = vk::RenderingAttachmentInfo::default()
            .image_view(target.depth.view)
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);
        core.device.cmd_begin_rendering(
            command,
            &vk::RenderingInfo::default()
                .render_area(rect([0, 0], render))
                .layer_count(1)
                .color_attachments(&color)
                .depth_attachment(&depth),
        );
        if !moving.is_empty() {
            set_viewport(core, command, [0, 0], render, render);
            // SAFETY: MotionVertex is plain repr(C) data without padding.
            let bytes = std::slice::from_raw_parts(
                moving.as_ptr().cast::<u8>(),
                std::mem::size_of_val(moving),
            );
            let (buffer, at) = self.vertices(core, bytes)?;
            core.device
                .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, pipeline);
            // The primitives' constants: their scale and jitter.
            let mut push = [0_u8; PRIMITIVE_PUSH];
            push[0..4].copy_from_slice(&(2.0 / native[0] as f32).to_le_bytes());
            push[4..8].copy_from_slice(&(2.0 / native[1] as f32).to_le_bytes());
            push[16..20].copy_from_slice(&(2.0 * offset[0] / render[0] as f32).to_le_bytes());
            push[20..24].copy_from_slice(&(2.0 * offset[1] / render[1] as f32).to_le_bytes());
            core.device.cmd_push_constants(
                command,
                core.primitive_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &push,
            );
            core.device
                .cmd_bind_vertex_buffers(command, 0, &[buffer], &[at]);
            core.device.cmd_draw(command, moving.len() as u32, 1, 0, 0);
        }
        core.device.cmd_end_rendering(command);
        transition(core, command, objects, vk::ImageLayout::GENERAL);
        Ok(())
    }
}

/// The motion pass's pipeline: moving models' triangles with their corners'
/// motion, depth-tested against the world without writing it, into the
/// motion image.
pub(super) unsafe fn motion_pipeline(
    core: &Core,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
) -> Result<vk::Pipeline, VkDeviceError> {
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex)
            .name(c"main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment)
            .name(c"main"),
    ];
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<MotionVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [
        (0, vk::Format::R32G32B32_SFLOAT, 0),
        (1, vk::Format::R32G32_SFLOAT, 12),
    ]
    .map(|(location, format, offset)| {
        vk::VertexInputAttributeDescription::default()
            .location(location)
            .binding(0)
            .format(format)
            .offset(offset)
    });
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes);
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    let attachments = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic);
    let formats = [LIGHT_IMAGE];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&formats)
        .depth_attachment_format(DEPTH_FORMAT);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth_state)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(core.primitive_layout)
        .push_next(&mut rendering);
    core.device
        .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
        .map(|pipelines| pipelines[0])
        .map_err(|(_, result)| call("motion pipeline creation")(result))
}

fn image_write<'a>(
    set: vk::DescriptorSet,
    binding: u32,
    kind: vk::DescriptorType,
    info: &'a [vk::DescriptorImageInfo],
) -> vk::WriteDescriptorSet<'a> {
    vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(kind)
        .image_info(info)
}

/// The prepare pass's constants: this view in the last one (rows of the
/// turn, each with the eye's move along it across the world's wrap), both
/// projections, the sizes, the jitter, the near plane, and whether the last
/// view can be followed.
fn prepare_constants(
    camera: &SceneCamera,
    previous: Option<&RasterView>,
    display: [u32; 2],
    offset: [f32; 2],
) -> [f32; PREPARE_FLOATS] {
    let then = previous.map_or(camera, |previous| &previous.camera);
    let wrap = |delta: f64| (delta + 32768.0).rem_euclid(65536.0) - 32768.0;
    let moved = [
        wrap(camera.eye[0] - then.eye[0]),
        camera.eye[1] - then.eye[1],
        wrap(camera.eye[2] - then.eye[2]),
    ];
    let dot =
        |a: [f32; 3], b: [f64; 3]| (0..3).map(|axis| f64::from(a[axis]) * b[axis]).sum::<f64>();
    let mut constants = [0.0_f32; PREPARE_FLOATS];
    for row in 0..3 {
        for column in 0..3 {
            constants[row * 4 + column] =
                dot(then.basis[row], camera.basis[column].map(f64::from)) as f32;
        }
        constants[row * 4 + 3] = dot(then.basis[row], moved) as f32;
    }
    constants[12..16].copy_from_slice(&[
        camera.focal[0],
        camera.focal[1],
        camera.centre[0],
        camera.centre[1],
    ]);
    constants[16..20].copy_from_slice(&[
        then.focal[0],
        then.focal[1],
        then.centre[0],
        then.centre[1],
    ]);
    constants[20..24].copy_from_slice(&[
        camera.native[0] as f32,
        camera.native[1] as f32,
        display[0] as f32,
        display[1] as f32,
    ]);
    constants[24..28].copy_from_slice(&[
        offset[0],
        offset[1],
        camera.near,
        if previous.is_some() { 1.0 } else { 0.0 },
    ]);
    constants
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modern_settings::UpscaleQuality;

    fn camera(eye: [f64; 3], basis: [[f32; 3]; 3]) -> SceneCamera {
        SceneCamera {
            eye,
            basis,
            focal: [320.0, 320.0],
            centre: [320.0, 240.0],
            native: [640, 480],
            near: 64.0,
            far: 20000.0,
        }
    }

    const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    #[test]
    fn a_still_view_turns_nothing() {
        let view = camera([100.0, 50.0, 200.0], IDENTITY);
        let previous = RasterView {
            camera: view,
            extent: [640, 480],
            present: 0,
        };
        let constants = prepare_constants(&view, Some(&previous), [640, 480], [0.25, -0.5]);
        assert_eq!(
            constants[0..12],
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        );
        assert_eq!(constants[24..28], [0.25, -0.5, 64.0, 1.0]);
    }

    #[test]
    fn the_eye_moves_across_the_wrap_and_into_the_last_view() {
        // Last frame the eye stood just short of the wrap, looking along +X
        // (right is -Z): this frame it stands just past it.
        let turned = [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
        let then = RasterView {
            camera: camera([65530.0, 0.0, 10.0], turned),
            extent: [640, 480],
            present: 0,
        };
        let now = camera([4.0, 0.0, 10.0], turned);
        let constants = prepare_constants(&now, Some(&then), [640, 480], [0.0; 2]);
        // Ten units along the last view's forward axis, none across it.
        assert_eq!(
            [constants[3], constants[7], constants[11]],
            [0.0, 0.0, 10.0]
        );
        // The same axes: no turn.
        assert_eq!(constants[0], 1.0);
        assert_eq!(constants[5], 1.0);
        assert_eq!(constants[10], 1.0);
    }

    /// A corner of a model that moved 30 units right since the last frame
    /// was 30 units left of where it is, seen at its depth; one carried
    /// behind the last view's eye has no motion.
    #[test]
    fn a_moving_corner_reports_where_it_was() {
        let view = camera([0.0; 3], IDENTITY);
        let vertex = D3dTlVertex {
            x: 352.0,
            y: 240.0,
            depth: 1.0 - 64.0 / 500.0,
            rhw: 1.0 / 500.0,
            diffuse: [255; 4],
            specular: 0,
            uv: [0.0; 2],
        };
        let moved = [
            [1.0, 0.0, 0.0, -30.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let corner = motion_vertex(&vertex, &view, &view, &moved).expect("in front of the eye");
        assert_eq!(corner.position, [352.0, 240.0, vertex.depth]);
        assert!(
            (corner.motion[0] + 19.2).abs() < 1.0e-3,
            "{:?}",
            corner.motion
        );
        assert!(corner.motion[1].abs() < 1.0e-3, "{:?}", corner.motion);
        let behind = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, -600.0],
        ];
        assert!(motion_vertex(&vertex, &view, &view, &behind).is_none());
    }

    #[test]
    fn without_a_last_view_nothing_is_followed() {
        let view = camera([0.0; 3], IDENTITY);
        assert_eq!(
            prepare_constants(&view, None, [640, 480], [0.0; 2])[27],
            0.0
        );
    }

    #[test]
    fn only_temporal_methods_jitter_the_raster() {
        let modern = |anti_aliasing| ModernSettings {
            ray_tracing: false,
            upscaler: crate::modern_settings::Upscaler::Off,
            quality: UpscaleQuality::Quality,
            anti_aliasing,
            ..ModernSettings::default()
        };
        assert_eq!(raster_plan(false, modern(AntiAliasing::Off)), None);
        assert_eq!(raster_plan(false, modern(AntiAliasing::Fxaa)), Some(false));
        assert_eq!(raster_plan(false, modern(AntiAliasing::Taa)), Some(true));
        assert_eq!(raster_plan(false, modern(AntiAliasing::Smaa)), Some(false));
        // Supersampling draws the whole frame larger, with no pass.
        assert_eq!(raster_plan(false, modern(AntiAliasing::Ssaa4x)), None);
        assert_eq!(raster_plan(true, modern(AntiAliasing::Ssaa2x)), Some(true));
        assert_eq!(raster_plan(true, modern(AntiAliasing::Fxaa)), Some(true));
    }
}
