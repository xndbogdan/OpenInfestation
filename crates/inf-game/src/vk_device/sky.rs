//! The modern sky's clouds on the Vulkan device
//! ([the sky plan](../../../../docs/rt-plans/sky-per-world.md)'s base).
//! Tiling noise is made once (`pt_sky_noise.comp`). Then each traced frame
//! whose world has clouds takes their shadow map, and a world showing the
//! modern sky also takes the clouds marched along the view at a quarter of
//! the display's size, with their distance and a history of both, and the
//! sky cube map the tracer's reflections look up (`pt_sky_clouds.comp`).
//! Every image stays in the general layout.

use super::trace::{compute_pipeline, pass_barrier, shader_module};
use super::*;

static NOISE: &[u8] = include_bytes!("../../shaders/pt_sky_noise.comp.spv");
static CLOUDS: &[u8] = include_bytes!("../../shaders/pt_sky_clouds.comp.spv");

/// The noise volumes' sides, in voxels.
const SHAPE_SIDE: u32 = 128;
const DETAIL_SIDE: u32 = 32;
/// The cube map's face and the shadow map's side, in texels.
const CUBE_SIDE: u32 = 128;
/// The cube's mip levels, down to a texel a face: rough reflections and the
/// sky's light read its coarse levels (the full path tracing plan).
const CUBE_LEVELS: u32 = CUBE_SIDE.ilog2() + 1;
const SHADOW_SIDE: u32 = 512;
/// The view's clouds are marched at this fraction of the display's width
/// and height: twice as many rows as columns, as a cloud near the horizon
/// is flattened to a sliver a quarter-height row broke into blocks (Exxodus
/// from a high point).
const VIEW_DIVISOR: [u32; 2] = [4, 2];
const NOISE_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const LIGHT_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const SHADOW_FORMAT: vk::Format = vk::Format::R16_SFLOAT;
/// Where the view's clouds lie: the far light's distance, the near band's
/// light through and share, all the light's distance (`clouds_toward`).
const AWAY_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// `pt_sky_clouds.comp`'s passes.
const VIEW_PASS: u32 = 0;
const CUBE_PASS: u32 = 1;
const SHADOW_PASS: u32 = 2;

/// How a sky image is laid out.
#[derive(Clone, Copy)]
enum Shape {
    Flat([u32; 2]),
    Volume(u32),
    Cube(u32),
}

/// What the tracer binds this frame, each in the general layout: the
/// view's clouds and their distance (the composite), the cube map with its
/// mip levels and the sampler that reads them, and the shadow map (the
/// trace).
pub(super) struct SkyViews {
    pub clouds: vk::ImageView,
    pub away: vk::ImageView,
    pub cube: vk::ImageView,
    pub cube_sampler: vk::Sampler,
    pub shadow: vk::ImageView,
}

pub(super) struct Sky {
    noise_set_layout: vk::DescriptorSetLayout,
    noise_layout: vk::PipelineLayout,
    noise_module: vk::ShaderModule,
    noise: vk::Pipeline,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    clouds_module: vk::ShaderModule,
    clouds: vk::Pipeline,
    /// A frame's set, and the noise's once; reset each frame.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    /// Linear and repeating: the noise tiles.
    repeat: vk::Sampler,
    /// Linear between and within the cube's mip levels.
    levels: vk::Sampler,
    /// The cube with all its mip levels, for sampling; its image's own view
    /// is its first level, which the cube pass writes.
    cube_levels: vk::ImageView,
    shape: Option<Image>,
    detail: Option<Image>,
    /// The view's clouds and their distance, this frame's and the last's
    /// by `parity`.
    views: [Option<Image>; 2],
    aways: [Option<Image>; 2],
    cube: Option<Image>,
    shadow: Option<Image>,
    noise_made: bool,
    parity: usize,
    /// The size the last frame marched the view's clouds at, if it did.
    marched: Option<[u32; 2]>,
    /// This frame's set, which a cube made later in the frame takes.
    set: Option<vk::DescriptorSet>,
}

impl Sky {
    pub(super) unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut sky = Self {
            noise_set_layout: vk::DescriptorSetLayout::null(),
            noise_layout: vk::PipelineLayout::null(),
            noise_module: vk::ShaderModule::null(),
            noise: vk::Pipeline::null(),
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            clouds_module: vk::ShaderModule::null(),
            clouds: vk::Pipeline::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            repeat: vk::Sampler::null(),
            levels: vk::Sampler::null(),
            cube_levels: vk::ImageView::null(),
            shape: None,
            detail: None,
            views: [None, None],
            aways: [None, None],
            cube: None,
            shadow: None,
            noise_made: false,
            parity: 0,
            marched: None,
            set: None,
        };
        match sky.build(core) {
            Ok(()) => Ok(sky),
            Err(error) => {
                sky.destroy(core, &mut None);
                Err(error)
            }
        }
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        let binding = |index: u32, kind: vk::DescriptorType| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(index)
                .descriptor_type(kind)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        };
        let storage = vk::DescriptorType::STORAGE_IMAGE;
        let sampled = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
        let noise_bindings = [binding(0, storage), binding(1, storage)];
        // The frame, the two noises, the view's clouds and their history,
        // the cube map, the shadow map, and the clouds' distance and its
        // history.
        let bindings = [
            binding(0, vk::DescriptorType::UNIFORM_BUFFER),
            binding(1, sampled),
            binding(2, sampled),
            binding(3, storage),
            binding(4, sampled),
            binding(5, storage),
            binding(6, storage),
            binding(7, storage),
            binding(8, sampled),
        ];
        for (bindings, set_layout) in [
            (&noise_bindings[..], &mut self.noise_set_layout),
            (&bindings[..], &mut self.set_layout),
        ] {
            *set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings),
                    None,
                )
                .map_err(call("sky descriptor layout"))?;
        }
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(8)];
        for (set_layout, layout) in [
            (self.noise_set_layout, &mut self.noise_layout),
            (self.set_layout, &mut self.layout),
        ] {
            let sets = [set_layout];
            *layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&sets)
                        .push_constant_ranges(&push),
                    None,
                )
                .map_err(call("sky pipeline layout"))?;
        }
        self.noise_module = shader_module(core, NOISE, "sky noise shader")?;
        self.noise = compute_pipeline(core, self.noise_layout, self.noise_module)?;
        self.clouds_module = shader_module(core, CLOUDS, "sky clouds shader")?;
        self.clouds = compute_pipeline(core, self.layout, self.clouds_module)?;
        let sizes = [
            (vk::DescriptorType::UNIFORM_BUFFER, 1),
            (sampled, 4),
            (storage, 6),
        ]
        .map(|(ty, descriptor_count)| vk::DescriptorPoolSize {
            ty,
            descriptor_count,
        });
        // One for each frame in flight: the device records the next frame
        // while the GPU still reads the last.
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(2)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("sky descriptor pool"))?;
        }
        self.repeat = device
            .create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::REPEAT)
                    .address_mode_v(vk::SamplerAddressMode::REPEAT)
                    .address_mode_w(vk::SamplerAddressMode::REPEAT)
                    .max_lod(0.0),
                None,
            )
            .map_err(call("sky sampler"))?;
        self.levels = device
            .create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .max_lod(vk::LOD_CLAMP_NONE),
                None,
            )
            .map_err(call("sky cube sampler"))?;
        Ok(())
    }

    /// Release everything, freeing memory through `allocator` when given.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        let [first, second] = std::mem::take(&mut self.views);
        let [first_away, second_away] = std::mem::take(&mut self.aways);
        for image in [
            self.shape.take(),
            self.detail.take(),
            self.cube.take(),
            self.shadow.take(),
            first,
            second,
            first_away,
            second_away,
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
        device.destroy_sampler(self.repeat, None);
        device.destroy_sampler(self.levels, None);
        device.destroy_image_view(self.cube_levels, None);
        for pool in self.pool {
            device.destroy_descriptor_pool(pool, None);
        }
        device.destroy_pipeline(self.noise, None);
        device.destroy_pipeline(self.clouds, None);
        device.destroy_pipeline_layout(self.noise_layout, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.noise_set_layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        device.destroy_shader_module(self.noise_module, None);
        device.destroy_shader_module(self.clouds_module, None);
    }

    /// This frame's passes on `command`, after the frame block `frame` is
    /// written and before the trace: the view's clouds where the world shows
    /// the `modern` sky at `display` size, the shadow map where it has
    /// `clouded` (the ground's and the water's sun), and for a `traced`
    /// world (whose reflections read it) the cube map.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn record(
        &mut self,
        state: &mut State,
        core: &Core,
        command: vk::CommandBuffer,
        frame: vk::Buffer,
        display: [u32; 2],
        modern: bool,
        clouded: bool,
        traced: bool,
    ) -> Result<SkyViews, VkDeviceError> {
        let view_size = [0, 1].map(|axis| display[axis].div_ceil(VIEW_DIVISOR[axis]).max(1));
        if self.shape.is_none() {
            self.shape = Some(sky_image(
                state,
                core,
                Shape::Volume(SHAPE_SIDE),
                NOISE_FORMAT,
            )?);
            self.detail = Some(sky_image(
                state,
                core,
                Shape::Volume(DETAIL_SIDE),
                NOISE_FORMAT,
            )?);
            let cube = sky_image(state, core, Shape::Cube(CUBE_SIDE), LIGHT_FORMAT)?;
            let levels = vk::ImageSubresourceRange {
                level_count: CUBE_LEVELS,
                ..range(vk::ImageAspectFlags::COLOR)
            };
            self.cube_levels = core
                .device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(cube.image)
                        .view_type(vk::ImageViewType::CUBE)
                        .format(LIGHT_FORMAT)
                        .subresource_range(levels),
                    None,
                )
                .map_err(call("sky cube levels view"))?;
            // Every level into the general layout, where it stays.
            let barrier = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(cube.image)
                .subresource_range(levels);
            core.device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default()
                    .image_memory_barriers(std::slice::from_ref(&barrier)),
            );
            cube.layout.set(vk::ImageLayout::GENERAL);
            self.cube = Some(cube);
            self.shadow = Some(sky_image(
                state,
                core,
                Shape::Flat([SHADOW_SIDE; 2]),
                SHADOW_FORMAT,
            )?);
        }
        if self.views[0]
            .as_ref()
            .is_none_or(|image| image.extent != view_size)
        {
            for (slot, format) in self
                .views
                .iter_mut()
                .map(|slot| (slot, LIGHT_FORMAT))
                .chain(self.aways.iter_mut().map(|slot| (slot, AWAY_FORMAT)))
            {
                if let Some(old) = slot.take() {
                    state.retired.push(old);
                }
                *slot = Some(sky_image(state, core, Shape::Flat(view_size), format)?);
            }
            self.marched = None;
        }
        for image in [&self.shape, &self.detail, &self.cube, &self.shadow]
            .into_iter()
            .chain(&self.views)
            .chain(&self.aways)
            .flatten()
        {
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }
        // The last frame's sets have finished with the pool.
        core.device
            .reset_descriptor_pool(self.pool[state.slot], vk::DescriptorPoolResetFlags::empty())
            .map_err(call("sky descriptor reset"))?;
        let general = |view: vk::ImageView, sampler: vk::Sampler| {
            [vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        let made = |slot: &Option<Image>| slot.as_ref().expect("made above").view;
        let write = |set: vk::DescriptorSet, binding: u32, kind: vk::DescriptorType| {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .descriptor_type(kind)
        };
        let storage = vk::DescriptorType::STORAGE_IMAGE;
        let sampled = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
        let dispatch = |set: vk::DescriptorSet,
                        layout: vk::PipelineLayout,
                        pipeline: vk::Pipeline,
                        push: [u32; 2],
                        groups: [u32; 3]| {
            core.device
                .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, pipeline);
            core.device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::COMPUTE,
                layout,
                0,
                &[set],
                &[],
            );
            core.device.cmd_push_constants(
                command,
                layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &[push[0].to_le_bytes(), push[1].to_le_bytes()].concat(),
            );
            core.device
                .cmd_dispatch(command, groups[0], groups[1], groups[2]);
        };
        if !self.noise_made {
            let layouts = [self.noise_set_layout];
            let set = core
                .device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.pool[state.slot])
                        .set_layouts(&layouts),
                )
                .map_err(call("sky noise descriptor set"))?[0];
            let shape = general(made(&self.shape), vk::Sampler::null());
            let detail = general(made(&self.detail), vk::Sampler::null());
            core.device.update_descriptor_sets(
                &[
                    write(set, 0, storage).image_info(&shape),
                    write(set, 1, storage).image_info(&detail),
                ],
                &[],
            );
            dispatch(
                set,
                self.noise_layout,
                self.noise,
                [0, 0],
                [SHAPE_SIDE / 4; 3],
            );
            dispatch(
                set,
                self.noise_layout,
                self.noise,
                [1, 0],
                [DETAIL_SIDE / 4; 3],
            );
            pass_barrier(core, command);
            self.noise_made = true;
        }
        self.parity ^= 1;
        let now = self.parity;
        let history = u32::from(self.marched == Some(view_size));
        let layouts = [self.set_layout];
        let set = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool[state.slot])
                    .set_layouts(&layouts),
            )
            .map_err(call("sky descriptor set"))?[0];
        let frame_info = [vk::DescriptorBufferInfo::default()
            .buffer(frame)
            .range(vk::WHOLE_SIZE)];
        let shape = general(made(&self.shape), self.repeat);
        let detail = general(made(&self.detail), self.repeat);
        let view = general(made(&self.views[now]), vk::Sampler::null());
        let last = general(made(&self.views[now ^ 1]), core.samplers[SMOOTH_SAMPLER]);
        let cube = general(made(&self.cube), vk::Sampler::null());
        let shadow = general(made(&self.shadow), vk::Sampler::null());
        let away = general(made(&self.aways[now]), vk::Sampler::null());
        let last_away = general(made(&self.aways[now ^ 1]), core.samplers[SMOOTH_SAMPLER]);
        core.device.update_descriptor_sets(
            &[
                write(set, 0, vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&frame_info),
                write(set, 1, sampled).image_info(&shape),
                write(set, 2, sampled).image_info(&detail),
                write(set, 3, storage).image_info(&view),
                write(set, 4, sampled).image_info(&last),
                write(set, 5, storage).image_info(&cube),
                write(set, 6, storage).image_info(&shadow),
                write(set, 7, storage).image_info(&away),
                write(set, 8, sampled).image_info(&last_away),
            ],
            &[],
        );
        if clouded {
            let groups = SHADOW_SIDE.div_ceil(8);
            dispatch(
                set,
                self.layout,
                self.clouds,
                [SHADOW_PASS, 0],
                [groups, groups, 1],
            );
        }
        if modern {
            let groups = view_size.map(|side| side.div_ceil(8));
            dispatch(
                set,
                self.layout,
                self.clouds,
                [VIEW_PASS, history],
                [groups[0], groups[1], 1],
            );
        }
        self.set = Some(set);
        if modern && traced {
            self.cube_pass(core, command, set);
        }
        self.marched = modern.then_some(view_size);
        pass_barrier(core, command);
        Ok(SkyViews {
            clouds: made(&self.views[now]),
            away: made(&self.aways[now]),
            cube: self.cube_levels,
            cube_sampler: self.levels,
            shadow: made(&self.shadow),
        })
    }

    /// This frame's cube map, made after its passes, once the raster's
    /// metal is drawn (it needs the cube only where metal shows), with the
    /// sampler that reads its levels; none where the frame made none of
    /// them. What the frames before it read of the cube comes first.
    pub(super) unsafe fn record_cube(
        &self,
        core: &Core,
        command: vk::CommandBuffer,
    ) -> Option<(vk::ImageView, vk::Sampler)> {
        let set = self.set?;
        super::trace::frame_barrier(core, command);
        self.cube_pass(core, command, set);
        Some((self.cube_levels, self.levels))
    }

    /// The cube map with its mip levels, from the frame block in `set`, on
    /// `command`; sampled after it by the compute and fragment shaders.
    unsafe fn cube_pass(&self, core: &Core, command: vk::CommandBuffer, set: vk::DescriptorSet) {
        let side = CUBE_SIDE.div_ceil(8);
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, self.clouds);
        core.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::COMPUTE,
            self.layout,
            0,
            &[set],
            &[],
        );
        core.device.cmd_push_constants(
            command,
            self.layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &[CUBE_PASS.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
        );
        core.device.cmd_dispatch(command, side, side, 6);
        // Each level from the one above, all six faces at once.
        let cube = self.cube.as_ref().expect("made with the sky").image;
        let step = |src_stage, src_access| {
            vk::MemoryBarrier2::default()
                .src_stage_mask(src_stage)
                .src_access_mask(src_access)
                .dst_stage_mask(vk::PipelineStageFlags2::BLIT)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ | vk::AccessFlags2::TRANSFER_WRITE)
        };
        let mut barrier = step(
            vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::SHADER_STORAGE_WRITE,
        );
        for level in 1..CUBE_LEVELS {
            core.device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&barrier)),
            );
            let size = |level: u32| (CUBE_SIDE >> level).max(1) as i32;
            let corner = |level: u32| {
                [
                    vk::Offset3D::default(),
                    vk::Offset3D {
                        x: size(level),
                        y: size(level),
                        z: 1,
                    },
                ]
            };
            let face = |level: u32| vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: level,
                base_array_layer: 0,
                layer_count: 6,
            };
            let region = vk::ImageBlit::default()
                .src_subresource(face(level - 1))
                .src_offsets(corner(level - 1))
                .dst_subresource(face(level))
                .dst_offsets(corner(level));
            core.device.cmd_blit_image(
                command,
                cube,
                vk::ImageLayout::GENERAL,
                cube,
                vk::ImageLayout::GENERAL,
                &[region],
                vk::Filter::LINEAR,
            );
            barrier = step(
                vk::PipelineStageFlags2::BLIT,
                vk::AccessFlags2::TRANSFER_WRITE,
            );
        }
        let sampled = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::BLIT)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(
                vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::FRAGMENT_SHADER,
            )
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ);
        core.device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&sampled)),
        );
    }
}

/// A cube of a texel a face in the general layout: what a pass binds for
/// the sky's cube where no sky is made, never reading it.
pub(super) unsafe fn stand_in_cube(
    state: &mut State,
    core: &Core,
    command: vk::CommandBuffer,
) -> Result<Image, VkDeviceError> {
    let cube = sky_image(state, core, Shape::Cube(1), LIGHT_FORMAT)?;
    transition(core, command, &cube, vk::ImageLayout::GENERAL);
    Ok(cube)
}

/// A sky image of `shape` in `format`, for storage and sampling, in device
/// memory.
unsafe fn sky_image(
    state: &mut State,
    core: &Core,
    shape: Shape,
    format: vk::Format,
) -> Result<Image, VkDeviceError> {
    let levels = match shape {
        Shape::Cube(side) => side.ilog2() + 1,
        _ => 1,
    };
    let usage = match shape {
        // Its levels are blitted down from the first.
        Shape::Cube(_) => {
            vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
        }
        _ => vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
    };
    let (image_type, view_type, extent, layers, flags) = match shape {
        Shape::Flat([width, height]) => (
            vk::ImageType::TYPE_2D,
            vk::ImageViewType::TYPE_2D,
            vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            1,
            vk::ImageCreateFlags::empty(),
        ),
        Shape::Volume(side) => (
            vk::ImageType::TYPE_3D,
            vk::ImageViewType::TYPE_3D,
            vk::Extent3D {
                width: side,
                height: side,
                depth: side,
            },
            1,
            vk::ImageCreateFlags::empty(),
        ),
        Shape::Cube(side) => (
            vk::ImageType::TYPE_2D,
            vk::ImageViewType::CUBE,
            vk::Extent3D {
                width: side,
                height: side,
                depth: 1,
            },
            6,
            vk::ImageCreateFlags::CUBE_COMPATIBLE,
        ),
    };
    let image = core
        .device
        .create_image(
            &vk::ImageCreateInfo::default()
                .flags(flags)
                .image_type(image_type)
                .format(format)
                .extent(extent)
                .mip_levels(levels)
                .array_layers(layers)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            None,
        )
        .map_err(call("sky image creation"))?;
    let requirements = core.device.get_image_memory_requirements(image);
    let allocation = state
        .allocator
        .as_mut()
        .expect("allocator lives with the device")
        .allocate(&AllocationCreateDesc {
            name: "sky image",
            requirements,
            location: MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        });
    let allocation = match allocation {
        Ok(allocation) => allocation,
        Err(error) => {
            core.device.destroy_image(image, None);
            return Err(VkDeviceError::Memory(error.to_string()));
        }
    };
    core.device
        .bind_image_memory(image, allocation.memory(), allocation.offset())
        .map_err(call("sky image binding"))?;
    let view = core
        .device
        .create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(view_type)
                .format(format)
                .subresource_range(range(vk::ImageAspectFlags::COLOR)),
            None,
        )
        .map_err(call("sky image view"))?;
    Ok(Image {
        image,
        view,
        allocation: Some(allocation),
        extent: [extent.width, extent.height],
        aspect: vk::ImageAspectFlags::COLOR,
        layout: Cell::new(vk::ImageLayout::UNDEFINED),
        set: None,
    })
}
