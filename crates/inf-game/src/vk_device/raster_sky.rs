//! The modern sky over Vulkan's raster, when no rays are traced
//! ([the plan](../../../../docs/rt-plans/modern-raster.md)'s stage 2). At
//! the world's first draw the frame block is written from the world's
//! light (`world_scene::raster_sky_feed`), the sky's clouds are marched
//! (`Sky`, shared with the tracer) and the frame is cleared to black; the
//! painting then draws only its land, its painted sky cut away, and the art
//! the sky keeps (`scene_capture::raster_sky_draw`). Once the painting is
//! drawn, its land's triangles are drawn again into a mask of how much sky
//! shows through them, and `vk_sky.comp` lays the gradient and the clouds
//! behind the land and the art, before the world's draws come over them;
//! after the anti-aliasing or the upscaler, at the display's size, it lays
//! the clouds over the world beyond them, the lightning and the snow or
//! rain, as the tracer's composite does. The frame block comes with the
//! water's splash buffer, as the tracer's, and is written again with the
//! vehicles' lamps once the light pass has kept them, for the water's pass
//! (`mirror.rs`, stage 4).

use super::trace::{
    bytes_of, compute_pipeline, frame_block, pass_barrier, shader_module, water_splashes,
    FrameShape, PreviousView, COLOR_IMAGE, SPLASHES,
};
use super::*;

static COMPOSITE: &[u8] = include_bytes!("../../shaders/vk_sky.comp.spv");

/// `vk_sky.comp`'s passes: behind the painting's land, before the world;
/// over the world before its blended draws (its fade and clouds); over it
/// all after its anti-aliasing (the arcs and the falling rain or snow).
const BACKDROP_PASS: u32 = 0;
pub(super) const WORLD_PASS: u32 = 1;
pub(super) const OVER_PASS: u32 = 2;

/// The composite's bindings, in `vk_sky.comp`'s order: the frame block, the
/// frame's colour and depth, the land's mask, the view's clouds and their
/// distance, and the result.
const KINDS: [vk::DescriptorType; 7] = [
    vk::DescriptorType::UNIFORM_BUFFER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::STORAGE_IMAGE,
];

/// The land's triangles drawn this frame, each batch with its texture.
pub(super) type BandBatch = Vec<(vk::DescriptorSet, Vec<DeviceVertex>)>;

pub(super) struct RasterSky {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
    pass: vk::Pipeline,
    /// The land's textures, each texel taking its alpha's share of what
    /// still shows of the sky.
    band_pipeline: vk::Pipeline,
    /// One for each frame in flight: the device records the next frame
    /// while the GPU still reads the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    frame: [Option<Buffer>; FRAMES_IN_FLIGHT],
    /// The water's splashes, as the tracer's splash buffer, and the
    /// eruptions kept from frame to frame.
    splashes: [Option<Buffer>; FRAMES_IN_FLIGHT],
    eruptions: crate::scene_spray::Eruptions,
    /// What this frame's block was written with beside its feed, for the
    /// lamps the light pass finds later ([`State::raster_lamps`]).
    shape: Option<FrameShape<'static>>,
    band: Option<Image>,
    /// Each pass's result, sampled for the copy back.
    results: [Option<Image>; 3],
    /// The last frame's view, which the clouds' history follows.
    previous: Option<PreviousView>,
    frame_index: u32,
    /// This frame's clouds and their distance (`Sky`'s), in the general
    /// layout.
    clouds: Option<[vk::ImageView; 2]>,
    /// This frame's clouds' shadow map (`Sky`'s), in the general layout.
    shadow: Option<vk::ImageView>,
    /// This frame's sky cube (`Sky`'s) and the sampler that reads its
    /// levels, made while puddles stand on the wet ground: they mirror it.
    cube: Option<(vk::ImageView, vk::Sampler)>,
    /// The cube made later in this frame for its metal, where the puddles
    /// made none.
    metal_cube: Option<(vk::ImageView, vk::Sampler)>,
    /// The frame block was written this frame, for its slot.
    written: bool,
    /// This frame has rain, snow or arcs for the over pass.
    over: bool,
}

impl RasterSky {
    unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut sky = Self {
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            module: vk::ShaderModule::null(),
            pass: vk::Pipeline::null(),
            band_pipeline: vk::Pipeline::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            frame: [const { None }; FRAMES_IN_FLIGHT],
            splashes: [const { None }; FRAMES_IN_FLIGHT],
            eruptions: Default::default(),
            shape: None,
            band: None,
            results: [None, None, None],
            previous: None,
            frame_index: 0,
            clouds: None,
            shadow: None,
            cube: None,
            metal_cube: None,
            written: false,
            over: false,
        };
        if let Err(error) = sky.build(core) {
            sky.destroy(core, &mut None);
            return Err(error);
        }
        Ok(sky)
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        let bindings: Vec<_> = KINDS
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(index as u32)
                    .descriptor_type(*kind)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        self.set_layout = device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
            .map_err(call("raster sky descriptor layout"))?;
        let sets = [self.set_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(20)];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
            .map_err(call("raster sky pipeline layout"))?;
        self.module = shader_module(core, COMPOSITE, "raster sky shader")?;
        self.pass = compute_pipeline(core, self.layout, self.module)?;
        // The land as the raster drew it, untested; what it leaves of the
        // sky, multiplied in.
        self.band_pipeline = graphics_pipeline_with(
            core,
            core.primitive_layout,
            [core.modules.primitive, core.modules.textured],
            COLOR_FORMAT,
            false,
            Some(PipelineKey {
                textured: true,
                blend: D3dTextureBlend::Copy,
                depth_test: false,
                depth_write: false,
                traced_fog: false,
                depth_always: false,
            }),
            Some((vk::BlendFactor::ZERO, vk::BlendFactor::ONE_MINUS_SRC_ALPHA)),
        )?;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(3),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(15),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(3),
        ];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(3)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("raster sky descriptor pool"))?;
        }
        Ok(())
    }

    /// Release everything, freeing memory through `allocator` when given;
    /// the device must be past its last use.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        let [backdrop, world, over] = std::mem::take(&mut self.results);
        for image in [self.band.take(), backdrop, world, over]
            .into_iter()
            .flatten()
        {
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            if let (Some(allocation), Some(allocator)) = (image.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for buffer in self
            .frame
            .iter_mut()
            .chain(&mut self.splashes)
            .filter_map(Option::take)
        {
            device.destroy_buffer(buffer.buffer, None);
            if let (Some(allocation), Some(allocator)) = (buffer.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for pool in self.pool {
            device.destroy_descriptor_pool(pool, None);
        }
        for pipeline in [self.pass, self.band_pipeline] {
            device.destroy_pipeline(pipeline, None);
        }
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        device.destroy_shader_module(self.module, None);
    }
}

impl RasterSky {
    /// This frame's frame block in `slot`, where it was written, and the
    /// clouds' shadow map, where the modern sky's clouds were marched.
    pub(super) fn light(&self, slot: usize) -> Option<(vk::Buffer, Option<vk::ImageView>)> {
        self.written
            .then(|| {
                self.frame[slot]
                    .as_ref()
                    .map(|frame| (frame.buffer, self.shadow))
            })
            .flatten()
    }

    /// This frame's splash buffer in `slot`, where the frame block was
    /// written.
    pub(super) fn splashes(&self, slot: usize) -> Option<vk::Buffer> {
        self.written
            .then(|| self.splashes[slot].as_ref().map(|buffer| buffer.buffer))
            .flatten()
    }

    /// This frame's sky cube, where it was made (the modern sky, puddles
    /// standing).
    pub(super) fn cube(&self) -> Option<(vk::ImageView, vk::Sampler)> {
        self.cube.filter(|_| self.written)
    }
}

impl State {
    /// Begin the raster world `feed` (its camera and its light alone),
    /// drawn at `render` pixels moved by `offset` and shown at `display`:
    /// the frame block its own passes read (the water's, the sky's)
    /// written, and where `sky_wanted` and the world's sky is modern, the
    /// clouds marched. True where the modern sky is drawn; nothing where
    /// the device cannot (no scalar block layout).
    pub(super) unsafe fn begin_raster_sky(
        &mut self,
        core: &Core,
        feed: &crate::scene_feed::SceneFeed,
        display: [u32; 2],
        render: [u32; 2],
        offset: [f32; 2],
        sky_wanted: bool,
    ) -> Result<bool, VkDeviceError> {
        if !core.scalar_layout {
            return Ok(false);
        }
        let mut sky = match self.raster_sky.take() {
            Some(sky) => sky,
            None => Box::new(RasterSky::new(core)?),
        };
        let result =
            self.begin_raster_sky_with(core, &mut sky, feed, display, render, offset, sky_wanted);
        self.raster_sky = Some(sky);
        result
    }

    unsafe fn begin_raster_sky_with(
        &mut self,
        core: &Core,
        sky: &mut RasterSky,
        feed: &crate::scene_feed::SceneFeed,
        display: [u32; 2],
        render: [u32; 2],
        offset: [f32; 2],
        sky_wanted: bool,
    ) -> Result<bool, VkDeviceError> {
        let camera = &feed.camera;
        // The water's splashes and the blasts' eruptions, as the tracer's.
        let (_, splashes) = water_splashes(&mut sky.eruptions, feed, feed.water_level);
        let slot = self.slot;
        if sky.splashes[slot].is_none() {
            sky.splashes[slot] = Some(self.buffer(
                core,
                (SPLASHES * std::mem::size_of::<[f32; 7]>()) as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
            )?);
        }
        let splash_bytes = bytes_of(&splashes);
        sky.splashes[slot]
            .as_mut()
            .and_then(|buffer| buffer.allocation.as_mut())
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("raster splash memory is not mapped".into()))?
            [..splash_bytes.len()]
            .copy_from_slice(splash_bytes);
        let shape = FrameShape {
            display,
            extent: render,
            upscaled: render != display,
            ray_reconstruction: false,
            // The raster's draws moved by `offset`: a pixel shows the
            // view that far back (pt_frame.glsl's pixel_direction).
            jitter: offset.map(|side| -side),
            view: PreviousView::of(camera, render),
            previous: sky
                .previous
                .filter(|previous| previous.fits(render, camera.native)),
            frame_index: sky.frame_index,
            water_count: 0,
            sky_count: 0,
            cutout_first: 0,
            glow_first: 0,
            source_count: 0,
            splash_count: splashes.len(),
            spray_sources: 0,
            emitter_count: 0,
            emissive_groups: 0,
            effect_mirrors: 0.0,
            water_texture: -1.0,
            water_hue: feed.water_hue.unwrap_or_default(),
            grid: None,
            world_view: true,
        };
        let written = frame_block(feed, &shape, self.modern);
        sky.over = written.over;
        sky.shape = Some(shape);
        sky.clouds = None;
        sky.shadow = None;
        sky.cube = None;
        sky.metal_cube = None;
        sky.written = false;
        let drawn = sky_wanted && written.modern_sky;
        sky.previous = drawn.then_some(written.view);
        sky.frame_index = sky.frame_index.wrapping_add(1);
        let bytes = bytes_of(&written.block);
        if sky.frame[slot].is_none() {
            sky.frame[slot] = Some(self.buffer(
                core,
                bytes.len() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
            )?);
        }
        let frame = sky.frame[slot].as_mut().expect("made above");
        frame
            .allocation
            .as_mut()
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("raster sky memory is not mapped".into()))?
            [..bytes.len()]
            .copy_from_slice(bytes);
        let frame = frame.buffer;
        sky.written = true;
        if !drawn {
            return Ok(false);
        }
        let command = self.begin(core)?;
        self.end_rendering(core);
        let mut clouds = match self.sky.take() {
            Some(clouds) => clouds,
            None => Box::new(super::sky::Sky::new(core)?),
        };
        // The cube too while puddles stand on the ground the frame wets
        // (its grid comes only where the device wets it: above its water,
        // not lava): they mirror the sky above the frame.
        let wet = feed.raster_wet_ground().is_some()
            && feed.weather.is_some_and(|weather| weather.puddles > 0.0);
        let views = clouds.record(
            self,
            core,
            command,
            frame,
            display,
            true,
            written.clouded,
            wet,
        );
        self.sky = Some(clouds);
        let views = views?;
        sky.clouds = Some([views.clouds, views.away]);
        sky.shadow = written.clouded.then_some(views.shadow);
        sky.cube = wet.then_some((views.cube, views.cube_sampler));
        Ok(true)
    }

    /// This raster world's frame block written again from its `feed`, now
    /// with the vehicles' lamps the light pass kept, for its water's pass:
    /// the same block with its lamps and their hue in the water.
    pub(super) fn raster_lamps(
        &mut self,
        feed: &crate::scene_feed::SceneFeed,
    ) -> Result<(), VkDeviceError> {
        let slot = self.slot;
        let modern = self.modern;
        let Some(sky) = self.raster_sky.as_mut().filter(|sky| sky.written) else {
            return Ok(());
        };
        let (Some(shape), Some(frame)) = (sky.shape.as_mut(), sky.frame[slot].as_mut()) else {
            return Ok(());
        };
        shape.water_hue = feed.water_hue.unwrap_or_default();
        let written = frame_block(feed, shape, modern);
        sky.over = written.over;
        let bytes = bytes_of(&written.block);
        frame
            .allocation
            .as_mut()
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("raster sky memory is not mapped".into()))?
            [..bytes.len()]
            .copy_from_slice(bytes);
        Ok(())
    }

    /// The modern sky's cube for this raster world's metal (`metal.rs`):
    /// the puddles', where they made one, else made now, once, where the
    /// modern sky is drawn; none under a painted sky. Recorded between the
    /// world's draws.
    pub(super) unsafe fn raster_metal_cube(
        &mut self,
        core: &Core,
    ) -> Result<Option<(vk::ImageView, vk::Sampler)>, VkDeviceError> {
        let (made, drawn) = match self.raster_sky.as_ref().filter(|sky| sky.written) {
            Some(sky) => (sky.cube.or(sky.metal_cube), sky.clouds.is_some()),
            None => return Ok(None),
        };
        if made.is_some() || !drawn {
            return Ok(made);
        }
        let command = self.begin(core)?;
        self.end_rendering(core);
        let cube = self
            .sky
            .as_ref()
            .and_then(|clouds| clouds.record_cube(core, command));
        if let Some(sky) = self.raster_sky.as_mut() {
            sky.metal_cube = cube;
        }
        Ok(cube)
    }

    /// Lay the modern sky behind the painting's land and art the raster
    /// drew into `target` (`band`: the land's triangles, drawn at `render`
    /// pixels with `native` coordinates, moved by `offset`), before the
    /// world's draws come over it.
    pub(super) unsafe fn raster_sky_backdrop(
        &mut self,
        core: &Core,
        target: &Target,
        band: &BandBatch,
        native: [u32; 2],
        render: [u32; 2],
        offset: [f32; 2],
    ) -> Result<(), VkDeviceError> {
        let Some(mut sky) = self.raster_sky.take() else {
            return Ok(());
        };
        let result =
            self.raster_sky_backdrop_with(core, &mut sky, target, band, native, render, offset);
        self.raster_sky = Some(sky);
        result
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn raster_sky_backdrop_with(
        &mut self,
        core: &Core,
        sky: &mut RasterSky,
        target: &Target,
        band: &BandBatch,
        native: [u32; 2],
        render: [u32; 2],
        offset: [f32; 2],
    ) -> Result<(), VkDeviceError> {
        if sky.clouds.is_none() {
            return Ok(());
        }
        let command = self.begin(core)?;
        self.end_rendering(core);
        self.storage_image(
            core,
            &mut sky.band,
            COLOR_FORMAT,
            render,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
        )?;
        let mask = sky.band.as_ref().expect("made above");
        // What of the sky shows through the land: all of it, less each
        // land texel's alpha, as the raster laid the land over the black.
        transition(
            core,
            command,
            mask,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        let attachment = [vk::RenderingAttachmentInfo::default()
            .image_view(mask.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [1.0; 4] },
            })];
        core.device.cmd_begin_rendering(
            command,
            &vk::RenderingInfo::default()
                .render_area(rect([0, 0], render))
                .layer_count(1)
                .color_attachments(&attachment),
        );
        if !band.is_empty() {
            set_viewport(core, command, [0, 0], render, render);
            core.device.cmd_bind_pipeline(
                command,
                vk::PipelineBindPoint::GRAPHICS,
                sky.band_pipeline,
            );
            // The primitives' constants: their scale, untested, and jitter.
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
            for (set, vertices) in band {
                // SAFETY: DeviceVertex is plain repr(C) data without padding.
                let bytes = std::slice::from_raw_parts(
                    vertices.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(vertices.as_slice()),
                );
                let (buffer, at) = self.vertices(core, bytes)?;
                core.device.cmd_bind_descriptor_sets(
                    command,
                    vk::PipelineBindPoint::GRAPHICS,
                    core.primitive_layout,
                    0,
                    &[*set],
                    &[],
                );
                core.device
                    .cmd_bind_vertex_buffers(command, 0, &[buffer], &[at]);
                core.device
                    .cmd_draw(command, vertices.len() as u32, 1, 0, 0);
            }
        }
        core.device.cmd_end_rendering(command);
        self.sky_pass(core, sky, target, BACKDROP_PASS, render, render)
    }

    /// Lay the modern sky's pass `which` over the raster world in `target`
    /// (drawn at `render` pixels): [`WORLD_PASS`], the far ground's fade,
    /// the clouds before it and the lightning, at `render` before the
    /// world's blended draws; [`OVER_PASS`], the arcs and the snow or rain,
    /// anti-aliased and at the display's size, the HUD not drawn yet. None
    /// of it with the eye `under_water`: the view there is the water's.
    pub(super) unsafe fn finish_raster_sky(
        &mut self,
        core: &Core,
        target: &Target,
        render: [u32; 2],
        which: u32,
        under_water: bool,
    ) -> Result<(), VkDeviceError> {
        let Some(mut sky) = self.raster_sky.take() else {
            return Ok(());
        };
        // The over pass has nothing to lay without rain, snow or arcs.
        let wanted = which != OVER_PASS || sky.over;
        let result = if wanted && sky.clouds.is_some() && sky.band.is_some() && !under_water {
            let size = if which == OVER_PASS {
                target.color.extent
            } else {
                render
            };
            self.sky_pass(core, &mut sky, target, which, size, render)
        } else {
            Ok(())
        };
        if which == OVER_PASS {
            sky.clouds = None;
        }
        self.raster_sky = Some(sky);
        result
    }

    /// Run `vk_sky.comp`'s pass `which` over `target` at `size`, the world
    /// drawn at `render`, and put the result back over the viewport.
    unsafe fn sky_pass(
        &mut self,
        core: &Core,
        sky: &mut RasterSky,
        target: &Target,
        which: u32,
        size: [u32; 2],
        render: [u32; 2],
    ) -> Result<(), VkDeviceError> {
        let [clouds, away] = sky.clouds.expect("checked by the caller");
        let command = self.begin(core)?;
        self.end_rendering(core);
        let slot = &mut sky.results[which as usize];
        if slot.as_ref().is_none_or(|image| image.extent != size) {
            if let Some(old) = slot.take() {
                self.retired.push(old);
            }
            *slot = Some(self.image(
                core,
                size,
                COLOR_IMAGE,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                true,
            )?);
        }
        let result = sky.results[which as usize].as_ref().expect("made above");
        let mask = sky.band.as_ref().expect("made by the backdrop");
        for image in [mask, &target.color, &target.depth] {
            transition(
                core,
                command,
                image,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
        transition(core, command, result, vk::ImageLayout::GENERAL);
        let pool = sky.pool[self.slot];
        if which == BACKDROP_PASS {
            core.device
                .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::empty())
                .map_err(call("raster sky descriptor reset"))?;
        }
        let layouts = [sky.set_layout];
        let set = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
            .map_err(call("raster sky descriptor set"))?[0];
        let frame = [vk::DescriptorBufferInfo::default()
            .buffer(
                sky.frame[self.slot]
                    .as_ref()
                    .expect("written at the begin")
                    .buffer,
            )
            .range(vk::WHOLE_SIZE)];
        let image = |view: vk::ImageView, sampler: usize, layout: vk::ImageLayout| {
            [vk::DescriptorImageInfo::default()
                .sampler(core.samplers[sampler])
                .image_view(view)
                .image_layout(layout)]
        };
        let read = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        let general = vk::ImageLayout::GENERAL;
        let images = [
            image(target.color.view, BLIT_SAMPLER, read),
            image(target.depth.view, BLIT_SAMPLER, read),
            image(mask.view, SMOOTH_SAMPLER, read),
            image(clouds, SMOOTH_SAMPLER, general),
            image(away, SMOOTH_SAMPLER, general),
            [vk::DescriptorImageInfo::default()
                .image_view(result.view)
                .image_layout(general)],
        ];
        let mut writes = vec![vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(KINDS[0])
            .buffer_info(&frame)];
        writes.extend(images.iter().enumerate().map(|(index, info)| {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(index as u32 + 1)
                .descriptor_type(KINDS[index + 1])
                .image_info(info)
        }));
        core.device.update_descriptor_sets(&writes, &[]);
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, sky.pass);
        core.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::COMPUTE,
            sky.layout,
            0,
            &[set],
            &[],
        );
        let mut push = [0_u32; 5];
        for (index, value) in [size[0], size[1], render[0], render[1]]
            .into_iter()
            .enumerate()
        {
            push[index] = (value as f32).to_bits();
        }
        push[4] = which;
        core.device.cmd_push_constants(
            command,
            sky.layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes_of(&push),
        );
        let groups = size.map(|side| side.div_ceil(8));
        core.device.cmd_dispatch(command, groups[0], groups[1], 1);
        pass_barrier(core, command);
        transition(
            core,
            command,
            result,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );

        // The result back over the viewport (the world's, for the
        // backdrop), its alpha kept.
        self.render_to(core, target)?;
        blit(core, self.blit_target, result.set.expect("sampled"), false);
        Ok(())
    }
}
