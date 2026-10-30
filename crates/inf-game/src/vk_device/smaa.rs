//! SMAA over the world's colour, traced or the raster's
//! ([the shaders](../../../shaders/smaa/README.md)): edges by luma, each
//! edge's blending weights from the reference's area and search tables,
//! then each pixel blended with its neighbours by them.

use super::trace::{bytes_of, compute_pipeline, pass_barrier, shader_module, COLOR_IMAGE};
use super::*;

static EDGES: &[u8] = include_bytes!("../../shaders/smaa_edges.comp.spv");
static WEIGHTS: &[u8] = include_bytes!("../../shaders/smaa_weights.comp.spv");
static BLEND: &[u8] = include_bytes!("../../shaders/smaa_blend.comp.spv");
/// The reference's tables: areas by edge shape (160 by 560, two channels)
/// and search lengths (64 by 16, one).
static AREA: &[u8] = include_bytes!("../../shaders/smaa/area.bin");
static SEARCH: &[u8] = include_bytes!("../../shaders/smaa/search.bin");
const AREA_SIZE: [u32; 2] = [160, 560];
const SEARCH_SIZE: [u32; 2] = [64, 16];

pub(super) struct Smaa {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    modules: [vk::ShaderModule; 3],
    passes: [vk::Pipeline; 3],
    /// A run's one set, reset each run.
    /// One for each frame in flight: the device records the next frame
    /// while the GPU still reads the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    area: Option<Image>,
    search: Option<Image>,
    edges: Option<Image>,
    weights: Option<Image>,
}

impl Smaa {
    pub(super) unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut smaa = Self {
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            modules: [vk::ShaderModule::null(); 3],
            passes: [vk::Pipeline::null(); 3],
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            area: None,
            search: None,
            edges: None,
            weights: None,
        };
        if let Err(error) = smaa.build(core) {
            smaa.destroy(core, &mut None);
            return Err(error);
        }
        Ok(smaa)
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        // The colour, the edges written and read, the weights written and
        // read, the two tables, and the result (smaa.glsl's passes).
        let kinds = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
        ];
        let bindings: Vec<_> = kinds
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
            .map_err(call("SMAA descriptor layout"))?;
        let sets = [self.set_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(16)];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
            .map_err(call("SMAA pipeline layout"))?;
        for (index, code) in [EDGES, WEIGHTS, BLEND].into_iter().enumerate() {
            self.modules[index] = shader_module(core, code, "SMAA shader")?;
            self.passes[index] = compute_pipeline(core, self.layout, self.modules[index])?;
        }
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(5),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(3),
        ];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("SMAA descriptor pool"))?;
        }
        Ok(())
    }

    /// Release everything, freeing memory through `allocator` when given;
    /// the device must be past its last use.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        for image in [
            self.area.take(),
            self.search.take(),
            self.edges.take(),
            self.weights.take(),
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
        for pass in self.passes {
            device.destroy_pipeline(pass, None);
        }
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        for module in self.modules {
            device.destroy_shader_module(module, None);
        }
    }

    /// Record SMAA from `input` into `output`, both `extent` and in the
    /// general layout: `input` sampled, `output` written. The tables upload
    /// on the first run; the work images follow `extent`.
    pub(super) unsafe fn run(
        &mut self,
        state: &mut State,
        core: &Core,
        input: &Image,
        output: &Image,
        extent: [u32; 2],
    ) -> Result<(), VkDeviceError> {
        for (slot, bytes, size, format) in [
            (&mut self.area, AREA, AREA_SIZE, vk::Format::R8G8_UNORM),
            (&mut self.search, SEARCH, SEARCH_SIZE, vk::Format::R8_UNORM),
        ] {
            if slot.is_none() {
                let table = state.image(
                    core,
                    size,
                    format,
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    false,
                )?;
                state.upload(core, &table, bytes)?;
                *slot = Some(table);
            }
        }
        for slot in [&mut self.edges, &mut self.weights] {
            state.traced_image(core, slot, COLOR_IMAGE, extent)?;
        }
        let command = state.begin(core)?;
        state.end_rendering(core);
        fn made(slot: &Option<Image>) -> &Image {
            slot.as_ref().expect("made above")
        }
        let [area, search, edges, weights] =
            [&self.area, &self.search, &self.edges, &self.weights].map(made);
        for image in [edges, weights] {
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }
        core.device
            .reset_descriptor_pool(self.pool[state.slot], vk::DescriptorPoolResetFlags::empty())
            .map_err(call("SMAA descriptor reset"))?;
        let layouts = [self.set_layout];
        let set = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool[state.slot])
                    .set_layouts(&layouts),
            )
            .map_err(call("SMAA descriptor set"))?[0];
        // Linear and clamped, as the reference's samplers all are.
        let sampled = |image: &Image, layout: vk::ImageLayout| {
            [vk::DescriptorImageInfo::default()
                .sampler(core.samplers[SMOOTH_SAMPLER])
                .image_view(image.view)
                .image_layout(layout)]
        };
        let storage = |image: &Image| {
            [vk::DescriptorImageInfo::default()
                .image_view(image.view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        let general = vk::ImageLayout::GENERAL;
        let table = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        let infos = [
            sampled(input, general),
            storage(edges),
            sampled(edges, general),
            storage(weights),
            sampled(weights, general),
            sampled(area, table),
            sampled(search, table),
            storage(output),
        ];
        let kinds = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
        ];
        let writes: Vec<_> = infos
            .iter()
            .zip(kinds)
            .enumerate()
            .map(|(binding, (info, kind))| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(binding as u32)
                    .descriptor_type(kind)
                    .image_info(info)
            })
            .collect();
        core.device.update_descriptor_sets(&writes, &[]);
        let metrics = [
            1.0 / extent[0] as f32,
            1.0 / extent[1] as f32,
            extent[0] as f32,
            extent[1] as f32,
        ];
        let groups = extent.map(|side| side.div_ceil(8));
        for pass in self.passes {
            core.device
                .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, pass);
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
                bytes_of(&metrics),
            );
            core.device.cmd_dispatch(command, groups[0], groups[1], 1);
            pass_barrier(core, command);
        }
        Ok(())
    }
}
