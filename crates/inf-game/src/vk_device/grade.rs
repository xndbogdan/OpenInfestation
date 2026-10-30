//! The lighting preset's grade on the Vulkan device
//! (the plan).
//! After the world's last draw, traced or the raster's, and before the HUD,
//! the frame is copied aside and drawn back with the preset's exposure,
//! saturation and tone curve. Remaster's look takes none.

use super::*;
use crate::modern_tuning::LightingTuning;

static GRADE: &[u8] = include_bytes!("../../shaders/vk_grade.frag.spv");

/// The pass's push constants, in `vk_grade.frag`'s order.
const GRADE_FLOATS: usize = 3;

pub(super) struct Grade {
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
    pipeline: vk::Pipeline,
    /// The frame's copy's set, reset each use; one pool for each frame in
    /// flight: the device records the next frame while the GPU still
    /// grades the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    /// The frame's copy, at its size.
    image: Option<Image>,
}

impl Grade {
    pub(super) unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut grade = Self {
            layout: vk::PipelineLayout::null(),
            module: vk::ShaderModule::null(),
            pipeline: vk::Pipeline::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            image: None,
        };
        match grade.build(core) {
            Ok(()) => Ok(grade),
            Err(error) => {
                grade.destroy(core, &mut None);
                Err(error)
            }
        }
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        let sets = [core.texture_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .size((GRADE_FLOATS * 4) as u32)];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
            .map_err(call("grade pipeline layout"))?;
        let words = ash::util::read_spv(&mut std::io::Cursor::new(GRADE))
            .expect("embedded SPIR-V is whole words");
        self.module = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
            .map_err(call("grade shader"))?;
        self.pipeline = graphics_pipeline(
            core,
            self.layout,
            core.modules.blit_vertex,
            self.module,
            COLOR_FORMAT,
            false,
            None,
        )?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("grade descriptor pool"))?;
        }
        Ok(())
    }

    /// Release everything, freeing memory through `allocator` when given.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        if let Some(image) = self.image.take() {
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            if let (Some(allocation), Some(allocator)) = (image.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for pool in self.pool {
            device.destroy_descriptor_pool(pool, None);
        }
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_shader_module(self.module, None);
    }
}

impl State {
    /// The pending grade over the frame so far: the traced world and the
    /// raster's effects over it, and nothing above them yet.
    pub(super) unsafe fn finish_grade(
        &mut self,
        core: &Core,
        target: &Target,
    ) -> Result<(), VkDeviceError> {
        let Some(look) = self.graded.take() else {
            return Ok(());
        };
        let mut grade = match self.grade.take() {
            Some(grade) => grade,
            None => Box::new(Grade::new(core)?),
        };
        let result = self.finish_grade_with(core, &mut grade, target, &look);
        self.grade = Some(grade);
        result
    }

    unsafe fn finish_grade_with(
        &mut self,
        core: &Core,
        grade: &mut Grade,
        target: &Target,
        look: &LightingTuning,
    ) -> Result<(), VkDeviceError> {
        let command = self.begin(core)?;
        self.end_rendering(core);
        let size = target.color.extent;
        if grade
            .image
            .as_ref()
            .is_some_and(|image| image.extent != size)
        {
            if let Some(old) = grade.image.take() {
                self.retired.push(old);
            }
        }
        if grade.image.is_none() {
            grade.image = Some(self.image(
                core,
                size,
                COLOR_FORMAT,
                vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                false,
            )?);
        }
        let copy = grade.image.as_ref().expect("made above");
        // The frame aside, as it stands.
        transition(
            core,
            command,
            &target.color,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transition(core, command, copy, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
        let layers = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        core.device.cmd_copy_image(
            command,
            target.color.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            copy.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageCopy::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .extent(vk::Extent3D {
                    width: size[0],
                    height: size[1],
                    depth: 1,
                })],
        );
        transition(
            core,
            command,
            copy,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        transition(
            core,
            command,
            &target.color,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        // This slot's last frame has finished with its pool.
        let pool = grade.pool[self.slot];
        core.device
            .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::empty())
            .map_err(call("grade descriptor reset"))?;
        let layouts = [core.texture_layout];
        let set = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
            .map_err(call("grade descriptor set"))?[0];
        let info = [vk::DescriptorImageInfo::default()
            .sampler(core.samplers[BLIT_SAMPLER])
            .image_view(copy.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        core.device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&info)],
            &[],
        );
        let push: [f32; GRADE_FLOATS] = [
            look.exposure.max(0.0),
            look.saturation.max(0.0),
            look.curve.clamp(0.0, 1.0),
        ];
        // The copy back over the frame, graded; every pixel is drawn.
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(target.color.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)];
        core.device.cmd_begin_rendering(
            command,
            &vk::RenderingInfo::default()
                .render_area(rect([0, 0], size))
                .layer_count(1)
                .color_attachments(&color),
        );
        set_viewport(core, command, [0, 0], size, size);
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, grade.pipeline);
        core.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::GRAPHICS,
            grade.layout,
            0,
            &[set],
            &[],
        );
        core.device.cmd_push_constants(
            command,
            grade.layout,
            vk::ShaderStageFlags::FRAGMENT,
            0,
            bytes_of(&push),
        );
        core.device.cmd_draw(command, 4, 1, 0, 0);
        core.device.cmd_end_rendering(command);
        Ok(())
    }
}

/// The bytes of `values`, plain floats.
fn bytes_of(values: &[f32]) -> &[u8] {
    // SAFETY: f32 is plain data, read only as bytes.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}
