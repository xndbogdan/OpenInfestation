//! Thermal vision's camera on the Vulkan device
//! ([the plan](../../../../docs/INFESTATION-RT-IMPLEMENTATION.md#infrared-vision)).
//! After the world's last draw and before the HUD, the frame comes down to
//! the camera's own resolution as one grey. That frame is the tracer's heat
//! with the raster's effects over it, or retail's infrared image without the
//! tracer. It then goes back up over the whole view with an uncooled
//! camera's character: sharpened edges, halos round hot things, the sensor's
//! fixed stripes, grain each frame and a vignette.

use super::*;
use crate::modern_tuning::ThermalTuning;

static SENSOR: &[u8] = include_bytes!("../../shaders/vk_thermal_sensor.frag.spv");
static FINISH: &[u8] = include_bytes!("../../shaders/vk_thermal_finish.frag.spv");

/// The finishing pass's push constants, in `vk_thermal_finish.frag`'s
/// order; the sensor pass reads the first two.
const CAMERA_FLOATS: usize = 11;

pub(super) struct ThermalCamera {
    layout: vk::PipelineLayout,
    sensor_module: vk::ShaderModule,
    finish_module: vk::ShaderModule,
    sensor: vk::Pipeline,
    finish: vk::Pipeline,
    /// A frame's two sets, reset each use: the frame for the sensor, the
    /// sensor for the finish. One pool for each frame in flight: the device
    /// records the next frame while the GPU still finishes the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    /// The sensor's image.
    image: Option<Image>,
    /// Counts finished frames, for the grain.
    frame: u32,
}

impl ThermalCamera {
    pub(super) unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut camera = Self {
            layout: vk::PipelineLayout::null(),
            sensor_module: vk::ShaderModule::null(),
            finish_module: vk::ShaderModule::null(),
            sensor: vk::Pipeline::null(),
            finish: vk::Pipeline::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            image: None,
            frame: 0,
        };
        match camera.build(core) {
            Ok(()) => Ok(camera),
            Err(error) => {
                camera.destroy(core, &mut None);
                Err(error)
            }
        }
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        let sets = [core.texture_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .size((CAMERA_FLOATS * 4) as u32)];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
            .map_err(call("thermal camera pipeline layout"))?;
        let module = |bytes: &[u8], what| {
            let words = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
                .expect("embedded SPIR-V is whole words");
            device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
                .map_err(call(what))
        };
        self.sensor_module = module(SENSOR, "thermal sensor shader")?;
        self.finish_module = module(FINISH, "thermal finish shader")?;
        let pipeline = |fragment| {
            graphics_pipeline(
                core,
                self.layout,
                core.modules.blit_vertex,
                fragment,
                COLOR_FORMAT,
                false,
                None,
            )
        };
        self.sensor = pipeline(self.sensor_module)?;
        self.finish = pipeline(self.finish_module)?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(2)];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(2)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("thermal camera descriptor pool"))?;
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
        device.destroy_pipeline(self.sensor, None);
        device.destroy_pipeline(self.finish, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_shader_module(self.sensor_module, None);
        device.destroy_shader_module(self.finish_module, None);
    }
}

impl State {
    /// Thermal vision's camera over the frame so far, which holds the world
    /// and nothing above it yet.
    pub(super) unsafe fn finish_thermal(
        &mut self,
        core: &Core,
        target: &Target,
        look: &ThermalTuning,
        ironbow: bool,
    ) -> Result<(), VkDeviceError> {
        let mut camera = match self.thermal.take() {
            Some(camera) => camera,
            None => Box::new(ThermalCamera::new(core)?),
        };
        let result = self.finish_thermal_with(core, &mut camera, target, look, ironbow);
        self.thermal = Some(camera);
        result
    }

    unsafe fn finish_thermal_with(
        &mut self,
        core: &Core,
        camera: &mut ThermalCamera,
        target: &Target,
        look: &ThermalTuning,
        ironbow: bool,
    ) -> Result<(), VkDeviceError> {
        let command = self.begin(core)?;
        self.end_rendering(core);
        let display = target.color.extent;
        let width = (look.resolution.round() as u32).clamp(16, display[0].max(16));
        let height = (width * display[1] / display[0].max(1)).max(1);
        let size = [width, height];
        if camera
            .image
            .as_ref()
            .is_some_and(|image| image.extent != size)
        {
            if let Some(old) = camera.image.take() {
                self.retired.push(old);
            }
        }
        if camera.image.is_none() {
            camera.image = Some(self.image(
                core,
                size,
                COLOR_FORMAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                false,
            )?);
        }
        let sensor = camera.image.as_ref().expect("made above");
        // This slot's last frame has finished with its pool.
        let pool = camera.pool[self.slot];
        core.device
            .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::empty())
            .map_err(call("thermal camera descriptor reset"))?;
        let layouts = [core.texture_layout; 2];
        let [frame_set, sensor_set]: [vk::DescriptorSet; 2] = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
            .map_err(call("thermal camera descriptor sets"))?
            .try_into()
            .expect("two layouts give two sets");
        let sampled = |view: vk::ImageView| {
            [vk::DescriptorImageInfo::default()
                .sampler(core.samplers[SMOOTH_SAMPLER])
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)]
        };
        let [frame_info, sensor_info] = [target.color.view, sensor.view].map(sampled);
        let writes = [(frame_set, &frame_info), (sensor_set, &sensor_info)].map(|(set, info)| {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(info)
        });
        core.device.update_descriptor_sets(&writes, &[]);
        let mut push = [0.0_f32; CAMERA_FLOATS];
        push[0] = width as f32;
        push[1] = height as f32;
        push[2] = (camera.frame & 0x00ff_ffff) as f32;
        push[3] = look.grain.max(0.0);
        push[4] = look.stripes.max(0.0);
        push[5] = look.sharpen.max(0.0);
        push[6] = look.halo.max(0.0);
        push[7] = look.vignette.clamp(0.0, 1.0);
        push[8] = display[0] as f32 / display[1].max(1) as f32;
        push[9] = look.detail.max(0.0);
        // White hot, black and white only, or ironbow by the lamps' cycle.
        push[10] = f32::from(u8::from(ironbow));
        camera.frame = camera.frame.wrapping_add(1);
        let pass = |into: &Image, pipeline: vk::Pipeline, set: vk::DescriptorSet| {
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(into.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)];
            core.device.cmd_begin_rendering(
                command,
                &vk::RenderingInfo::default()
                    .render_area(rect([0, 0], into.extent))
                    .layer_count(1)
                    .color_attachments(&color),
            );
            set_viewport(core, command, [0, 0], into.extent, into.extent);
            core.device
                .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, pipeline);
            core.device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::GRAPHICS,
                camera.layout,
                0,
                &[set],
                &[],
            );
            core.device.cmd_push_constants(
                command,
                camera.layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                bytes_of(&push),
            );
            core.device.cmd_draw(command, 4, 1, 0, 0);
            core.device.cmd_end_rendering(command);
        };
        // The frame down to the sensor, then the sensor back over the view.
        transition(
            core,
            command,
            &target.color,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        transition(
            core,
            command,
            sensor,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        pass(sensor, camera.sensor, frame_set);
        transition(
            core,
            command,
            sensor,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        transition(
            core,
            command,
            &target.color,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        pass(&target.color, camera.finish, sensor_set);
        Ok(())
    }
}

/// The bytes of `values`, plain floats.
fn bytes_of(values: &[f32]) -> &[u8] {
    // SAFETY: f32 is plain data, read only as bytes.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}
