//! The water's memory on the device, Vulkan's remaster traced or not
//! ([the plan](../../../../docs/rt-plans/water-memory.md)): two images of
//! the water's cells round the eye, its height, rise, foam and churn
//! (`vk_waves.comp`), stepped in turn on the game's clock from
//! [`crate::scene_waves`]'s frame, which the water's shading samples
//! (`pt_splash.glsl`, the tracer's and the raster's). The game sees none of
//! it.

use super::trace::{bytes_of, compute_pipeline, pass_barrier, shader_module};
use super::*;
use crate::scene_waves::{WaveFrame, WaveGround, Waves, DEPTH_SIDE, SIDE};

static STEP: &[u8] = include_bytes!("../../shaders/vk_waves.comp.spv");

/// Height, rise, foam and churn, half floats.
const FIELD_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// The input's most floats: its header, the depths and 256 stamps.
const INPUT_FLOATS: usize = 12 + DEPTH_SIDE * DEPTH_SIDE + 12 * 256;
/// `vk_waves.comp`'s bindings: the cells before a step, after it, and the
/// frame's input.
const KINDS: [vk::DescriptorType; 3] = [
    vk::DescriptorType::STORAGE_IMAGE,
    vk::DescriptorType::STORAGE_IMAGE,
    vk::DescriptorType::STORAGE_BUFFER,
];

pub(super) struct WaveField {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
    pass: vk::Pipeline,
    /// One for each frame in flight: a set each way between the images.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    input: [Option<Buffer>; FRAMES_IN_FLIGHT],
    images: [Option<Image>; 2],
    /// The image the last step wrote, which the water samples.
    current: usize,
    waves: Waves,
}

impl WaveField {
    unsafe fn new(state: &mut State, core: &Core) -> Result<Self, VkDeviceError> {
        let mut field = Self {
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            module: vk::ShaderModule::null(),
            pass: vk::Pipeline::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            input: [const { None }; FRAMES_IN_FLIGHT],
            images: [None, None],
            current: 0,
            waves: Waves::default(),
        };
        if let Err(error) = field.build(state, core) {
            field.destroy(core, &mut None);
            return Err(error);
        }
        Ok(field)
    }

    unsafe fn build(&mut self, state: &mut State, core: &Core) -> Result<(), VkDeviceError> {
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
            .map_err(call("wave field descriptor layout"))?;
        let sets = [self.set_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(12)];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
            .map_err(call("wave field pipeline layout"))?;
        self.module = shader_module(core, STEP, "wave field shader")?;
        self.pass = compute_pipeline(core, self.layout, self.module)?;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(4),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(2),
        ];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(2)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("wave field descriptor pool"))?;
        }
        for image in &mut self.images {
            *image = Some(state.image(
                core,
                [SIDE as u32; 2],
                FIELD_FORMAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                false,
            )?);
        }
        for input in &mut self.input {
            *input = Some(state.buffer(
                core,
                (INPUT_FLOATS * 4) as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
            )?);
        }
        Ok(())
    }

    /// Release everything, freeing memory through `allocator` when given;
    /// the device must be past its last use.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        for image in self.images.iter_mut().filter_map(Option::take) {
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            if let (Some(allocation), Some(allocator)) = (image.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for buffer in self.input.iter_mut().filter_map(Option::take) {
            device.destroy_buffer(buffer.buffer, None);
            if let (Some(allocation), Some(allocator)) = (buffer.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for pool in self.pool {
            device.destroy_descriptor_pool(pool, None);
        }
        device.destroy_pipeline(self.pass, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        device.destroy_shader_module(self.module, None);
    }
}

/// Whether the water shows its memory for `feed`: a world with water, not
/// lava, not under thermal vision.
pub(super) fn shows_waves(feed: &crate::scene_feed::SceneFeed) -> bool {
    feed.waves.is_some()
        && !feed.infrared
        && crate::modern_tuning::current().water(feed.world).glow <= 0.0
}

impl State {
    /// Step the water's memory for `feed` into `command`, stirred by
    /// `splashes` (the splash buffer's records, eruptions and all) and the
    /// craft's wakes: true where the water shows it this frame (a world
    /// with water, not lava, not under thermal vision). Its image is made
    /// either way, for [`Self::wave_view`].
    pub(super) unsafe fn step_waves(
        &mut self,
        core: &Core,
        command: vk::CommandBuffer,
        feed: &crate::scene_feed::SceneFeed,
        splashes: &[[f32; 7]],
    ) -> Result<bool, VkDeviceError> {
        let mut field = match self.waves.take() {
            Some(field) => field,
            None => Box::new(WaveField::new(self, core)?),
        };
        for image in field.images.iter().flatten() {
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }
        let ground = feed.waves.as_ref().filter(|_| shows_waves(feed));
        let result = match ground {
            Some(ground) => self
                .record_waves(core, command, &mut field, ground, feed, splashes)
                .map(|()| true),
            None => {
                // The next water starts still.
                field.waves = Waves::default();
                Ok(false)
            }
        };
        self.waves = Some(field);
        result
    }

    unsafe fn record_waves(
        &mut self,
        core: &Core,
        command: vk::CommandBuffer,
        field: &mut WaveField,
        ground: &WaveGround,
        feed: &crate::scene_feed::SceneFeed,
        splashes: &[[f32; 7]],
    ) -> Result<(), VkDeviceError> {
        let stamps = crate::scene_waves::stamps(ground, splashes, &feed.wakes, &feed.spray_rings);
        let WaveFrame {
            floats,
            steps,
            seconds,
        } = field.waves.frame(feed.clock_micros, ground, &stamps);
        if steps == 0 {
            return Ok(());
        }
        let slot = self.slot;
        let bytes = bytes_of(&floats);
        let input = field.input[slot].as_mut().expect("made with the field");
        input
            .allocation
            .as_mut()
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("wave field memory is not mapped".into()))?
            [..bytes.len()]
            .copy_from_slice(bytes);
        let input = input.buffer;

        let pool = field.pool[slot];
        core.device
            .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::empty())
            .map_err(call("wave field descriptor reset"))?;
        let layouts = [field.set_layout; 2];
        let sets = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
            .map_err(call("wave field descriptor sets"))?;
        let views = field
            .images
            .each_ref()
            .map(|image| image.as_ref().expect("made with the field").view);
        let storage = |view: vk::ImageView| {
            [vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        // Set k steps from image k to the other.
        let infos = [
            [storage(views[0]), storage(views[1])],
            [storage(views[1]), storage(views[0])],
        ];
        let buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(input)
            .range(vk::WHOLE_SIZE)];
        let mut writes = Vec::with_capacity(6);
        for (set, [from, to]) in sets.iter().zip(&infos) {
            for (binding, info) in [(0, from), (1, to)] {
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_set(*set)
                        .dst_binding(binding)
                        .descriptor_type(KINDS[binding as usize])
                        .image_info(info),
                );
            }
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(*set)
                    .dst_binding(2)
                    .descriptor_type(KINDS[2])
                    .buffer_info(&buffer_info),
            );
        }
        core.device.update_descriptor_sets(&writes, &[]);

        // The last frame's reads of the images (its water's) are done
        // before they are written again.
        let before = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .src_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_access_mask(
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
            );
        core.device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&before)),
        );
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, field.pass);
        let groups = (SIDE as u32).div_ceil(16);
        for step in 0..steps {
            let push = [
                seconds,
                (step as f32 + 0.5) / steps as f32,
                if step == 0 { 1.0 } else { 0.0 },
            ];
            core.device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::COMPUTE,
                field.layout,
                0,
                &[sets[field.current]],
                &[],
            );
            core.device.cmd_push_constants(
                command,
                field.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytes_of(&push),
            );
            core.device.cmd_dispatch(command, groups, groups, 1);
            pass_barrier(core, command);
            field.current ^= 1;
        }
        Ok(())
    }

    /// The image the water samples its memory from (made by the first
    /// [`Self::step_waves`]), in the general layout.
    pub(super) fn wave_view(&self) -> Option<vk::ImageView> {
        let field = self.waves.as_ref()?;
        field.images[field.current].as_ref().map(|image| image.view)
    }
}
