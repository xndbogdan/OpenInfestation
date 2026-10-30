//! The modern raster's metal, when no rays are traced
//! ([the plan](../../../../docs/rt-plans/modern-raster.md)'s stage 3): a
//! metal face's solid fans ([`crate::raster_metal`]) are drawn by pipelines
//! of their own, which lay retail's colour as the others do and mirror the
//! modern sky in it at the face's roughness (`vk_metal.glsl`), from the
//! world's frame block and the sky's cube, made where the frame's first
//! metal is drawn if the puddles made none ([`State::raster_metal_cube`]).

use super::raster_aa::camera_point;
use super::trace::shader_module;
use super::*;
use crate::{raster_metal::MetalFace, scene_feed::SceneCamera};

static VERTEX: &[u8] = include_bytes!("../../shaders/vk_metal.vert.spv");
static TEXTURED: &[u8] = include_bytes!("../../shaders/vk_metal.frag.spv");
static FLAT: &[u8] = include_bytes!("../../shaders/vk_metal_flat.frag.spv");

/// A metal fan's corner: retail's own, then its normal and its colour and V
/// in its sprite, each times its reciprocal depth, beside that depth (the
/// fragment divides it out: the tracer blends them across the face in the
/// world), and the face's material ([`MetalFace`]'s, as words: metalness
/// and roughness below the split and above it; the split, and the bare
/// surface's bit at 16).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MetalVertex {
    corner: DeviceVertex,
    normal: [f32; 4],
    albedo: [f32; 4],
    material: [u32; 2],
}

/// [`MetalVertex`] as `vk_metal.vert` reads it.
const METAL_VERTEX: VertexLayout = (
    std::mem::size_of::<MetalVertex>() as u32,
    &[
        (0, vk::Format::R32G32B32_SFLOAT, 0),
        (1, vk::Format::R32G32B32_SFLOAT, 12),
        (2, vk::Format::R8G8B8A8_UNORM, 24),
        (3, vk::Format::R8G8B8A8_UNORM, 28),
        (4, vk::Format::R32G32B32A32_SFLOAT, 32),
        (5, vk::Format::R32G32B32A32_SFLOAT, 48),
        (6, vk::Format::R32G32_UINT, 64),
    ],
);

/// The corners of the fan `polygon` (drawn as `corners`) of the metal
/// `face`, seen with `camera` in a world `wet` with rain as much as it says,
/// into `out`: each with what it takes of the face where it lies, its normal
/// turned to the side of the fan the eye sees, as the tracer turns it (a
/// corner with none takes the fan's). None where the face shows nothing of
/// its own (no metal, its bare rubber dry), or a corner lies behind the eye
/// or on none of its triangles.
pub(super) fn fan_corners(
    face: &MetalFace,
    wet: f32,
    polygon: &[D3dTlVertex],
    corners: &[DeviceVertex],
    camera: &SceneCamera,
    out: &mut Vec<MetalVertex>,
) -> Option<()> {
    let [metal, _, above, _] = face.material;
    if metal == 0 && above == 0 && !(face.bare && wet > 0.0) {
        return None;
    }
    let views: tinyvec::TinyVec<[[f32; 3]; 8]> = polygon
        .iter()
        .map(|vertex| camera_point(vertex, camera))
        .collect::<Option<_>>()?;
    // The fan's normal (Newell's), toward the eye, in the world.
    let mut across = [0.0_f32; 3];
    for (k, a) in views.iter().enumerate() {
        let b = views[(k + 1) % views.len()];
        across[0] += (a[1] - b[1]) * (a[2] + b[2]);
        across[1] += (a[2] - b[2]) * (a[0] + b[0]);
        across[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let toward: f32 = (0..3)
        .map(|axis| across[axis] * views.iter().map(|view| view[axis]).sum::<f32>())
        .sum();
    let length = across.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
    if length <= 0.0 {
        return None;
    }
    let side = if toward > 0.0 { -length } else { length };
    let fan: [f32; 3] = std::array::from_fn(|axis| {
        (0..3)
            .map(|row| camera.basis[row][axis] * across[row] / side)
            .sum()
    });
    let material = [
        u32::from_le_bytes(face.material),
        u32::from(face.split) | u32::from(face.bare) << 16,
    ];
    for ((vertex, corner), view) in polygon.iter().zip(corners).zip(&views) {
        let at = face.at(*view)?;
        let normal = turned(at.normal, fan);
        let rhw = vertex.rhw;
        out.push(MetalVertex {
            corner: *corner,
            normal: [normal[0] * rhw, normal[1] * rhw, normal[2] * rhw, rhw],
            albedo: [
                at.albedo[0] * rhw,
                at.albedo[1] * rhw,
                at.albedo[2] * rhw,
                at.v * rhw,
            ],
            material,
        });
    }
    Some(())
}

/// `normal` turned to the side of a fan facing `fan` (the eye's), as the
/// tracer's `orient` turns it; the fan's own where it has none.
fn turned(normal: [f32; 3], fan: [f32; 3]) -> [f32; 3] {
    let length = normal.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
    if length < 0.5 {
        return fan;
    }
    let facing: f32 = (0..3).map(|axis| normal[axis] * fan[axis]).sum();
    let sign = if facing < 0.0 { -length } else { length };
    normal.map(|axis| axis / sign)
}

pub(super) struct Metal {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    vertex: vk::ShaderModule,
    textured: vk::ShaderModule,
    flat: vk::ShaderModule,
    pipelines: HashMap<PipelineKey, vk::Pipeline>,
    /// One for each frame in flight: the device records the next frame
    /// while the GPU still reads the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    /// Bound for the sky's cube under a painted sky.
    stand_in_cube: Option<Image>,
    /// This world's set (its frame block and the sky's cube), once its
    /// first metal is drawn, and whether the cube is the modern sky's.
    set: Option<(vk::DescriptorSet, bool)>,
}

impl Metal {
    unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut metal = Self {
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            vertex: vk::ShaderModule::null(),
            textured: vk::ShaderModule::null(),
            flat: vk::ShaderModule::null(),
            pipelines: HashMap::new(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            stand_in_cube: None,
            set: None,
        };
        if let Err(error) = metal.build(core) {
            metal.destroy(core, &mut None);
            return Err(error);
        }
        Ok(metal)
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        // The frame block and the sky's cube.
        let bindings = [
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        ]
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(index as u32)
                .descriptor_type(*kind)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        })
        .collect::<Vec<_>>();
        self.set_layout = device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
            .map_err(call("metal descriptor layout"))?;
        let sets = [core.texture_layout, self.set_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(PRIMITIVE_PUSH as u32)];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
            .map_err(call("metal pipeline layout"))?;
        self.vertex = shader_module(core, VERTEX, "metal vertex shader")?;
        self.textured = shader_module(core, TEXTURED, "metal shader")?;
        self.flat = shader_module(core, FLAT, "flat metal shader")?;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1),
        ];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("metal descriptor pool"))?;
        }
        Ok(())
    }

    /// The pipeline for a metal batch keyed `key`, made on first use.
    unsafe fn pipeline(
        &mut self,
        core: &Core,
        key: PipelineKey,
    ) -> Result<vk::Pipeline, VkDeviceError> {
        if let Some(pipeline) = self.pipelines.get(&key) {
            return Ok(*pipeline);
        }
        let fragment = if key.textured {
            self.textured
        } else {
            self.flat
        };
        let pipeline = graphics_pipeline_of(
            core,
            self.layout,
            [self.vertex, fragment],
            COLOR_FORMAT,
            true,
            Some(key),
            blend_factors(key.blend),
            METAL_VERTEX,
        )?;
        self.pipelines.insert(key, pipeline);
        Ok(pipeline)
    }

    /// Release everything, freeing memory through `allocator` when given;
    /// the device must be past its last use.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        if let Some(image) = self.stand_in_cube.take() {
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            if let (Some(allocation), Some(allocator)) = (image.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for pool in self.pool {
            device.destroy_descriptor_pool(pool, None);
        }
        for pipeline in self.pipelines.into_values() {
            device.destroy_pipeline(pipeline, None);
        }
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        for module in [self.vertex, self.textured, self.flat] {
            device.destroy_shader_module(module, None);
        }
    }
}

impl State {
    /// A raster world begins: its first metal makes its set.
    pub(super) fn begin_raster_metal(&mut self) {
        if let Some(metal) = &mut self.metal {
            metal.set = None;
        }
    }

    /// Draw the metal triangles `vertices` of a batch keyed `key` (with
    /// `texture`'s set; none untextured) into `target`, `native`
    /// coordinates moved by `jitter`, as the world's other draws.
    pub(super) unsafe fn draw_metal(
        &mut self,
        core: &Core,
        target: &Target,
        (key, texture): (PipelineKey, vk::DescriptorSet),
        vertices: &[MetalVertex],
        native: [u32; 2],
        jitter: [f32; 2],
    ) -> Result<(), VkDeviceError> {
        let mut metal = match self.metal.take() {
            Some(metal) => metal,
            None => Box::new(Metal::new(core)?),
        };
        let result = self.draw_metal_with(
            core,
            &mut metal,
            target,
            (key, texture),
            vertices,
            native,
            jitter,
        );
        self.metal = Some(metal);
        result
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn draw_metal_with(
        &mut self,
        core: &Core,
        metal: &mut Metal,
        target: &Target,
        (key, texture): (PipelineKey, vk::DescriptorSet),
        vertices: &[MetalVertex],
        native: [u32; 2],
        jitter: [f32; 2],
    ) -> Result<(), VkDeviceError> {
        let Some((frame, _)) = self
            .raster_sky
            .as_ref()
            .and_then(|sky| sky.light(self.slot))
        else {
            return Ok(());
        };
        if metal.set.is_none() {
            let cube = self.raster_metal_cube(core)?;
            let command = self.begin(core)?;
            if metal.stand_in_cube.is_none() {
                self.end_rendering(core);
                metal.stand_in_cube = Some(super::sky::stand_in_cube(self, core, command)?);
            }
            let pool = metal.pool[self.slot];
            core.device
                .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::empty())
                .map_err(call("metal descriptor reset"))?;
            let layouts = [metal.set_layout];
            let set = core
                .device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(pool)
                        .set_layouts(&layouts),
                )
                .map_err(call("metal descriptor set"))?[0];
            let frame = [vk::DescriptorBufferInfo::default()
                .buffer(frame)
                .range(vk::WHOLE_SIZE)];
            let (view, sampler) = cube.unwrap_or((
                metal.stand_in_cube.as_ref().expect("made above").view,
                core.samplers[SMOOTH_SAMPLER],
            ));
            let cube_info = [vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::GENERAL)];
            core.device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(&frame),
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&cube_info),
                ],
                &[],
            );
            metal.set = Some((set, cube.is_some()));
        }
        let (set, modern) = metal.set.expect("made above");
        let pipeline = metal.pipeline(core, key)?;
        self.render_to(core, target)?;
        // SAFETY: MetalVertex is plain repr(C) data without padding.
        let bytes = std::slice::from_raw_parts(
            vertices.as_ptr().cast::<u8>(),
            std::mem::size_of_val(vertices),
        );
        let (buffer, offset) = self.vertices(core, bytes)?;
        let command = core.command.get();
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, pipeline);
        // The primitives' constants: their scale, depth test and jitter,
        // and whether the modern sky's cube is bound.
        let mut push = [0_u8; PRIMITIVE_PUSH];
        push[0..4].copy_from_slice(&(2.0 / native[0] as f32).to_le_bytes());
        push[4..8].copy_from_slice(&(2.0 / native[1] as f32).to_le_bytes());
        let depth_test: f32 = if key.depth_test { 1.0 } else { 0.0 };
        push[8..12].copy_from_slice(&depth_test.to_le_bytes());
        push[12..16].copy_from_slice(&(if modern { 1.0_f32 } else { 0.0 }).to_le_bytes());
        let extent = self.raster_viewport.unwrap_or(target.color.extent);
        push[16..20].copy_from_slice(&(2.0 * jitter[0] / extent[0] as f32).to_le_bytes());
        push[20..24].copy_from_slice(&(2.0 * jitter[1] / extent[1] as f32).to_le_bytes());
        core.device.cmd_push_constants(
            command,
            metal.layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &push,
        );
        if key.textured {
            core.device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::GRAPHICS,
                metal.layout,
                0,
                &[texture],
                &[],
            );
        }
        core.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::GRAPHICS,
            metal.layout,
            1,
            &[set],
            &[],
        );
        core.device
            .cmd_bind_vertex_buffers(command, 0, &[buffer], &[offset]);
        core.device
            .cmd_draw(command, vertices.len() as u32, 1, 0, 0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vertex lies as `vk_metal.vert` reads it: no padding, the
    /// attributes where the layout puts them.
    #[test]
    fn the_metal_vertex_lies_as_its_shader_reads_it() {
        assert_eq!(std::mem::size_of::<MetalVertex>(), 72);
        assert_eq!(std::mem::offset_of!(MetalVertex, normal), 32);
        assert_eq!(std::mem::offset_of!(MetalVertex, albedo), 48);
        assert_eq!(std::mem::offset_of!(MetalVertex, material), 64);
        assert_eq!(METAL_VERTEX.0, 72);
    }

    /// A corner's normal turns to the fan's side the eye sees, as the
    /// tracer's `orient`; one with none takes the fan's.
    #[test]
    fn a_corners_normal_turns_to_the_fans_seen_side() {
        let fan = [0.0, 0.0, -1.0];
        assert_eq!(turned([0.0, 0.6, 0.8], fan), [0.0, -0.6, -0.8]);
        assert_eq!(turned([0.0, 0.0, -2.0], fan), [0.0, 0.0, -1.0]);
        assert_eq!(turned([0.0; 3], fan), fan);
    }
}
