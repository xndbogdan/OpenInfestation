//! The modern raster's water mirrors the world, when no rays are traced
//! ([the plan](../../../../docs/rt-plans/modern-raster.md)): after the
//! world's last draw and before its anti-aliasing, the water's triangles are
//! drawn again over the world's depth into a mask holding each water
//! pixel's own depth, `vk_mirror_depth.comp` builds the depth's pyramid of
//! nearest and farthest depths (its one-pixel holes closed), and `vk_mirror.comp` sends each water pixel's reflection
//! across the frame's colour and depth through it, as hierarchical-depth
//! screen-space reflections do. The result goes back into the frame. With
//! the eye under the water ([`Murk`]) the pass runs on every pixel: the
//! murk over what the view shows, and the surface seen from below. While the
//! rain wets the ground, retail's ground is drawn again over the world's
//! depth into a ground image (the colour its fog took and how much of it
//! shows through), and the same pass gives it the tracer's wet look and
//! puddles, from the tracer's ground grid ([`crate::scene_terrain`]).

use super::raster_aa::{motion_pipeline, MotionVertex, MOTION_FRAGMENT, MOTION_VERTEX};
use super::raster_sky::BandBatch;
use super::trace::{
    bytes_of, compute_pipeline, pass_barrier, shader_module, COLOR_IMAGE, LIGHT_IMAGE,
};
use super::*;
use crate::scene_feed::SceneCamera;
use crate::scene_terrain::TerrainMesh;

static MIRROR: &[u8] = include_bytes!("../../shaders/vk_mirror.comp.spv");
static PYRAMID: &[u8] = include_bytes!("../../shaders/vk_mirror_depth.comp.spv");
static GROUND: &[u8] = include_bytes!("../../shaders/vk_ground.frag.spv");

/// A water corner's reciprocal depth in the mask, scaled so that the
/// farthest water stays a normal half float (`vk_mirror.comp`'s).
pub(super) const RHW_SCALE: f32 = 4096.0;

/// The pass's push constants, in `vk_mirror.comp`'s order.
const MIRROR_FLOATS: usize = 32;

/// The passes' bindings, in their shaders' order: the frame's colour and
/// depth, the water's mask, the result, the depth's pyramid (which the
/// pyramid's pass writes), the frame block, the clouds' shadow map, the
/// water's own colour, as retail laid its texture, the wet ground's image,
/// the ground's grid points and its cells' diagonals, the sky's cube, and
/// the water's splashes.
const KINDS: [vk::DescriptorType; 13] = [
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::STORAGE_IMAGE,
    vk::DescriptorType::STORAGE_IMAGE,
    vk::DescriptorType::STORAGE_BUFFER,
    vk::DescriptorType::UNIFORM_BUFFER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::STORAGE_IMAGE,
    vk::DescriptorType::STORAGE_BUFFER,
    vk::DescriptorType::STORAGE_BUFFER,
    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    vk::DescriptorType::STORAGE_BUFFER,
];

/// How the rain wets the ground this frame, as `vk_mirror.comp`'s
/// `camera.wet` takes it: not at all, or its ground image drawn, its puddles
/// mirroring the painted sky's colour or the modern sky's cube above the
/// frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Wetness {
    Dry = 0,
    Wet = 1,
    UnderCube = 2,
}

impl Wetness {
    /// As `camera.wet` takes it, below zero where the water waits for the
    /// frame's later pass ([`MirrorStage::GroundFirst`]).
    fn constant(self, stage: MirrorStage) -> f32 {
        let value = f32::from(self as u8);
        if stage == MirrorStage::GroundFirst {
            -value
        } else {
            value
        }
    }
}

/// What a frame's pass shades: its water or its wet ground, or, with both
/// in view, the ground first, the water left as the frame holds it, then,
/// the draws held over the wet ground (`VkFrame`'s `late_ground`) laid over
/// it, the water, which mirrors them and the wet ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MirrorStage {
    Whole,
    GroundFirst,
    WaterAfter,
}

/// The ground's grid on the device, for the mesh it was made from.
struct GroundGrid {
    generation: u64,
    points: Buffer,
    diagonals: Buffer,
}

/// A level's cells across and down for a world drawn `size` pixels
/// (`vk_mirror_levels.glsl`'s).
fn level_size(size: [u32; 2], level: u32) -> [u32; 2] {
    size.map(|side| side.div_ceil(1 << level).max(1))
}

/// The pyramid's top level (its single cell), and its cells in all (each
/// a nearest and farthest depth).
fn pyramid_levels(size: [u32; 2]) -> (u32, u64) {
    let mut level = 0;
    let mut cells = u64::from(size[0].max(1)) * u64::from(size[1].max(1));
    while level_size(size, level).iter().any(|&side| side > 1) {
        level += 1;
        let [across, down] = level_size(size, level);
        cells += u64::from(across) * u64::from(down);
    }
    (level, cells)
}

/// How much the water mirrors: the Fresnel term's least share (looking
/// straight down) and its gain, as the tracer's water takes them; and with
/// the eye under the water, its murk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WaterMirror {
    pub least: f32,
    pub gain: f32,
    pub murk: Option<Murk>,
}

/// Port option, the remastered camera: with the eye under the water's live
/// surface the water's pass lays the murk over all the view shows, and the
/// draws held for after it (`VkFrame::defer`) take it too: its colour, and
/// how far from the eye it starts and where it is whole, as the world
/// draw's own underwater fog lays them (`world_scene::UNDERWATER_FAR`): the
/// port's own underwater look, not the world's clarity (the tracer's
/// `water_clear`), which hid the craft and the surface at dive
/// depths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Murk {
    pub(crate) colour: [f32; 3],
    pub(crate) fade: [f32; 2],
}

impl Murk {
    /// How much of what lies `t` from the eye, along the light's way
    /// through the water, shows through the murk: all of it to the fade's
    /// start, none past its end, evenly between, as retail's fog fades
    /// (`vk_mirror.comp`'s `murk_shows`).
    fn shows(&self, t: f32) -> f32 {
        let [near, far] = self.fade;
        1.0 - ((t - near) / (far - near).max(1.0)).clamp(0.0, 1.0)
    }

    /// `corner`, `t` from the eye, seen through the murk: what adds light
    /// or darkens (`fades`) fades to nothing, the rest into the murk's
    /// colour. A textured draw takes it as fog over retail's own, the two
    /// laid as one; a flat one has its fog in its colours.
    pub(super) fn through(
        &self,
        mut corner: DeviceVertex,
        t: f32,
        textured: bool,
        fades: bool,
    ) -> DeviceVertex {
        let clear = self.shows(t);
        let murk = if fades { [0.0; 3] } else { self.colour };
        let unit = |byte: u8| f32::from(byte) / 255.0;
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        if textured {
            // mix(murk, mix(fog, c, seen), clear) as mix(fog', c, shows).
            let seen = unit(corner.fog[3]);
            let shows = clear * seen;
            if shows < 1.0 {
                for channel in 0..3 {
                    let fog = unit(corner.fog[channel]);
                    let behind = murk[channel] * (1.0 - clear) + clear * (1.0 - seen) * fog;
                    corner.fog[channel] = byte(behind / (1.0 - shows));
                }
            }
            corner.fog[3] = byte(shows);
        } else {
            for channel in 0..3 {
                let colour = unit(corner.diffuse[channel]);
                corner.diffuse[channel] = byte(murk[channel] + (colour - murk[channel]) * clear);
            }
        }
        corner
    }
}

pub(super) struct Mirror {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
    pass: vk::Pipeline,
    pyramid_module: vk::ShaderModule,
    pyramid_pass: vk::Pipeline,
    mask_vertex: vk::ShaderModule,
    mask_fragment: vk::ShaderModule,
    mask_pipeline: vk::Pipeline,
    /// Retail's ground again, its fog into the ground image.
    ground_fragment: vk::ShaderModule,
    ground_pipeline: vk::Pipeline,
    /// One for each frame in flight, a set for each of its passes: the
    /// device records the next frame while the GPU still reads the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    mask: Option<Image>,
    /// The mirrored world, sampled for the copy back.
    result: Option<Image>,
    /// The water's own colour.
    surface: Option<Image>,
    /// The depth's pyramid, for the size it was made for.
    pyramid: Option<(Buffer, [u32; 2])>,
    /// The wet ground's image, and the ground's grid.
    ground: Option<Image>,
    grid: Option<GroundGrid>,
    /// Bound for the sky's cube where none is made.
    stand_in_cube: Option<Image>,
}

impl Mirror {
    unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let mut mirror = Self {
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            module: vk::ShaderModule::null(),
            pass: vk::Pipeline::null(),
            pyramid_module: vk::ShaderModule::null(),
            pyramid_pass: vk::Pipeline::null(),
            mask_vertex: vk::ShaderModule::null(),
            mask_fragment: vk::ShaderModule::null(),
            mask_pipeline: vk::Pipeline::null(),
            ground_fragment: vk::ShaderModule::null(),
            ground_pipeline: vk::Pipeline::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            mask: None,
            result: None,
            surface: None,
            pyramid: None,
            ground: None,
            grid: None,
            stand_in_cube: None,
        };
        if let Err(error) = mirror.build(core) {
            mirror.destroy(core, &mut None);
            return Err(error);
        }
        Ok(mirror)
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
            .map_err(call("mirror descriptor layout"))?;
        let sets = [self.set_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size((MIRROR_FLOATS * 4) as u32)];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
            .map_err(call("mirror pipeline layout"))?;
        self.module = shader_module(core, MIRROR, "mirror shader")?;
        self.pass = compute_pipeline(core, self.layout, self.module)?;
        self.pyramid_module = shader_module(core, PYRAMID, "mirror depth shader")?;
        self.pyramid_pass = compute_pipeline(core, self.layout, self.pyramid_module)?;
        self.mask_vertex = shader_module(core, MOTION_VERTEX, "motion vertex shader")?;
        self.mask_fragment = shader_module(core, MOTION_FRAGMENT, "motion fragment shader")?;
        self.mask_pipeline = motion_pipeline(core, self.mask_vertex, self.mask_fragment)?;
        self.ground_fragment = shader_module(core, GROUND, "ground mask shader")?;
        self.ground_pipeline = graphics_pipeline_with(
            core,
            core.primitive_layout,
            [core.modules.primitive, self.ground_fragment],
            LIGHT_IMAGE,
            true,
            Some(PipelineKey {
                textured: false,
                blend: D3dTextureBlend::Copy,
                depth_test: true,
                depth_write: false,
                traced_fog: false,
                depth_always: false,
            }),
            None,
        )?;

        // A frame's passes: the wet ground's, then the water's.
        const PASSES: u32 = 2;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(2 * PASSES),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(3 * PASSES),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(4 * PASSES),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(PASSES),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(3 * PASSES),
        ];
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(PASSES)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("mirror descriptor pool"))?;
        }
        Ok(())
    }

    /// Release everything, freeing memory through `allocator` when given;
    /// the device must be past its last use.
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        let device = &core.device;
        for image in [
            self.mask.take(),
            self.result.take(),
            self.surface.take(),
            self.ground.take(),
            self.stand_in_cube.take(),
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
        let grid = self.grid.take().map(|grid| [grid.points, grid.diagonals]);
        let pyramid = self.pyramid.take().map(|(buffer, _)| buffer);
        for buffer in pyramid.into_iter().chain(grid.into_iter().flatten()) {
            device.destroy_buffer(buffer.buffer, None);
            if let (Some(allocation), Some(allocator)) = (buffer.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for pool in self.pool {
            device.destroy_descriptor_pool(pool, None);
        }
        for pipeline in [
            self.pass,
            self.pyramid_pass,
            self.mask_pipeline,
            self.ground_pipeline,
        ] {
            device.destroy_pipeline(pipeline, None);
        }
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        for module in [
            self.module,
            self.pyramid_module,
            self.mask_vertex,
            self.mask_fragment,
            self.ground_fragment,
        ] {
            device.destroy_shader_module(module, None);
        }
    }
}

impl State {
    /// Mirror the world the raster drew into `target` with `camera` in its
    /// `water`'s triangles, as much as `look` says, with its draws moved by
    /// `offset`, and in the rain wet its `ground` (retail's ground's
    /// corners, [`super::ground_corner`], and the ground's grid); the
    /// anti-aliasing and the HUD are not drawn yet.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn finish_mirror(
        &mut self,
        core: &Core,
        target: &Target,
        camera: &SceneCamera,
        look: WaterMirror,
        offset: [f32; 2],
        water: &[MotionVertex],
        (water_draws, fog): (&BandBatch, [f32; 3]),
        ground: Option<(&[DeviceVertex], &TerrainMesh)>,
        native: [u32; 2],
        stage: MirrorStage,
    ) -> Result<(), VkDeviceError> {
        // The water's surface takes the world's frame block, written with
        // its light where the device has the scalar block layout.
        let Some((light, splashes)) = self
            .raster_sky
            .as_ref()
            .and_then(|sky| sky.light(self.slot).zip(sky.splashes(self.slot)))
        else {
            return Ok(());
        };
        let cube = self.raster_sky.as_ref().and_then(|sky| sky.cube());
        let mut mirror = match self.mirror.take() {
            Some(mirror) => mirror,
            None => Box::new(Mirror::new(core)?),
        };
        let result = self.finish_mirror_with(
            core,
            &mut mirror,
            target,
            camera,
            look,
            offset,
            water,
            (water_draws, fog),
            ground,
            native,
            (light, cube, splashes),
            stage,
        );
        self.mirror = Some(mirror);
        result
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn finish_mirror_with(
        &mut self,
        core: &Core,
        mirror: &mut Mirror,
        target: &Target,
        camera: &SceneCamera,
        look: WaterMirror,
        offset: [f32; 2],
        water: &[MotionVertex],
        (water_draws, fog): (&BandBatch, [f32; 3]),
        ground: Option<(&[DeviceVertex], &TerrainMesh)>,
        native: [u32; 2],
        ((frame, shadow), cube, splashes): (
            (vk::Buffer, Option<vk::ImageView>),
            Option<(vk::ImageView, vk::Sampler)>,
            vk::Buffer,
        ),
        stage: MirrorStage,
    ) -> Result<(), VkDeviceError> {
        let command = self.begin(core)?;
        self.end_rendering(core);
        if mirror.stand_in_cube.is_none() {
            mirror.stand_in_cube = Some(super::sky::stand_in_cube(self, core, command)?);
        }
        if let Some((_, mesh)) = ground {
            self.ground_grid(core, mirror, mesh)?;
            self.storage_image(
                core,
                &mut mirror.ground,
                LIGHT_IMAGE,
                self.raster_viewport.unwrap_or(target.color.extent),
                vk::ImageUsageFlags::COLOR_ATTACHMENT,
            )?;
        }
        // An upscaler's world is drawn at its render size, at the top left.
        let render = self.raster_viewport.unwrap_or(target.color.extent);
        self.storage_image(
            core,
            &mut mirror.mask,
            LIGHT_IMAGE,
            render,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
        )?;
        if mirror
            .result
            .as_ref()
            .is_none_or(|image| image.extent != render)
        {
            if let Some(old) = mirror.result.take() {
                self.retired.push(old);
            }
            mirror.result = Some(self.image(
                core,
                render,
                COLOR_IMAGE,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                true,
            )?);
        }
        let (top, cells) = pyramid_levels(render);
        if mirror
            .pyramid
            .as_ref()
            .is_none_or(|(_, size)| *size != render)
        {
            if let Some((old, _)) = mirror.pyramid.take() {
                self.transient.push(old);
            }
            let buffer = self.buffer(
                core,
                cells * 8,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::GpuOnly,
            )?;
            mirror.pyramid = Some((buffer, render));
        }
        self.storage_image(
            core,
            &mut mirror.surface,
            COLOR_FORMAT,
            render,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
        )?;
        let mask = mirror.mask.as_ref().expect("made above");
        let result = mirror.result.as_ref().expect("made above");
        let surface = mirror.surface.as_ref().expect("made above");
        self.draw_corners(
            core,
            mask,
            mirror.mask_pipeline,
            target,
            camera.native,
            render,
            offset,
            water,
        )?;
        // The water's own colour: retail's strips as it laid them, over the
        // world's depth, its fog and all; untextured strips (no set) as
        // untextured.
        let mut strips = Vec::with_capacity(water_draws.len());
        for (set, vertices) in water_draws {
            let pipeline = self.pipeline(
                core,
                PipelineKey {
                    textured: *set != vk::DescriptorSet::null(),
                    blend: D3dTextureBlend::Copy,
                    depth_test: true,
                    depth_write: false,
                    traced_fog: false,
                    depth_always: false,
                },
            )?;
            strips.push((pipeline, *set, vertices.as_slice()));
        }
        self.draw_over_depth(core, surface, target, native, render, offset, &strips)?;
        // Retail's ground in the rain: the colour its fog took and how much
        // of it shows through.
        let wet = ground.zip(mirror.ground.as_ref());
        if let Some(((corners, _), image)) = wet {
            let batch = [(mirror.ground_pipeline, vk::DescriptorSet::null(), corners)];
            self.draw_over_depth(core, image, target, native, render, offset, &batch)?;
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }
        for image in [surface, &target.color, &target.depth] {
            transition(
                core,
                command,
                image,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
        transition(core, command, result, vk::ImageLayout::GENERAL);

        let pool = mirror.pool[self.slot];
        if stage != MirrorStage::WaterAfter {
            core.device
                .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::empty())
                .map_err(call("mirror descriptor reset"))?;
        }
        let layouts = [mirror.set_layout];
        let set = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
            .map_err(call("mirror descriptor set"))?[0];
        let sampled = |image: &Image| {
            [vk::DescriptorImageInfo::default()
                .sampler(core.samplers[BLIT_SAMPLER])
                .image_view(image.view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)]
        };
        let storage = |image: &Image| {
            [vk::DescriptorImageInfo::default()
                .image_view(image.view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        let infos = [
            sampled(&target.color),
            sampled(&target.depth),
            storage(mask),
            storage(result),
        ];
        let mut writes: Vec<_> = infos
            .iter()
            .zip(KINDS)
            .enumerate()
            .map(|(binding, (info, kind))| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(binding as u32)
                    .descriptor_type(kind)
                    .image_info(info)
            })
            .collect();
        let pyramid = [vk::DescriptorBufferInfo::default()
            .buffer(mirror.pyramid.as_ref().expect("made above").0.buffer)
            .range(vk::WHOLE_SIZE)];
        writes.push(
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(4)
                .descriptor_type(KINDS[4])
                .buffer_info(&pyramid),
        );
        let frame = [vk::DescriptorBufferInfo::default()
            .buffer(frame)
            .range(vk::WHOLE_SIZE)];
        writes.push(
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(5)
                .descriptor_type(KINDS[5])
                .buffer_info(&frame),
        );
        // Without clouds marched, the frame block holds no cover and the
        // map is never read: the mask stands in.
        let shadow = [vk::DescriptorImageInfo::default()
            .sampler(core.samplers[SMOOTH_SAMPLER])
            .image_view(shadow.unwrap_or(mask.view))
            .image_layout(vk::ImageLayout::GENERAL)];
        writes.push(
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(6)
                .descriptor_type(KINDS[6])
                .image_info(&shadow),
        );
        let surface_info = [vk::DescriptorImageInfo::default()
            .sampler(core.samplers[BLIT_SAMPLER])
            .image_view(surface.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        writes.push(
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(7)
                .descriptor_type(KINDS[7])
                .image_info(&surface_info),
        );
        // Dry, nothing reads the ground's bindings: the mask and the
        // pyramid stand in.
        let pyramid_buffer = mirror.pyramid.as_ref().expect("made above").0.buffer;
        let (ground_view, points, diagonals) = match (wet, mirror.grid.as_ref()) {
            (Some((_, image)), Some(grid)) => {
                (image.view, grid.points.buffer, grid.diagonals.buffer)
            }
            _ => (mask.view, pyramid_buffer, pyramid_buffer),
        };
        let ground_info = [vk::DescriptorImageInfo::default()
            .image_view(ground_view)
            .image_layout(vk::ImageLayout::GENERAL)];
        writes.push(
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(8)
                .descriptor_type(KINDS[8])
                .image_info(&ground_info),
        );
        let grid_infos = [points, diagonals].map(|buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .range(vk::WHOLE_SIZE)]
        });
        for (binding, info) in [9, 10].into_iter().zip(&grid_infos) {
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(binding)
                    .descriptor_type(KINDS[binding as usize])
                    .buffer_info(info),
            );
        }
        let (cube_view, cube_sampler) = match cube.filter(|_| wet.is_some()) {
            Some(cube) => cube,
            None => (
                mirror.stand_in_cube.as_ref().expect("made above").view,
                core.samplers[SMOOTH_SAMPLER],
            ),
        };
        let cube_info = [vk::DescriptorImageInfo::default()
            .sampler(cube_sampler)
            .image_view(cube_view)
            .image_layout(vk::ImageLayout::GENERAL)];
        writes.push(
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(11)
                .descriptor_type(KINDS[11])
                .image_info(&cube_info),
        );
        let splash_info = [vk::DescriptorBufferInfo::default()
            .buffer(splashes)
            .range(vk::WHOLE_SIZE)];
        writes.push(
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(12)
                .descriptor_type(KINDS[12])
                .buffer_info(&splash_info),
        );
        core.device.update_descriptor_sets(&writes, &[]);
        core.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::COMPUTE,
            mirror.layout,
            0,
            &[set],
            &[],
        );

        // The depth's pyramid: its holes closed, then each level from the
        // one below; with the eye under the water, the water's pixels kept.
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, mirror.pyramid_pass);
        for level in 0..=top {
            let build = [render[0], render[1], level, u32::from(look.murk.is_some())];
            core.device.cmd_push_constants(
                command,
                mirror.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytes_of(&build),
            );
            let groups = level_size(render, level).map(|side| side.div_ceil(8));
            core.device.cmd_dispatch(command, groups[0], groups[1], 1);
            pass_barrier(core, command);
        }

        let wetness = match (wet, cube) {
            (None, _) => Wetness::Dry,
            (Some(_), None) => Wetness::Wet,
            (Some(_), Some(_)) => Wetness::UnderCube,
        };
        let push = mirror_constants(camera, look, fog, wetness.constant(stage), render, offset);
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, mirror.pass);
        core.device.cmd_push_constants(
            command,
            mirror.layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes_of(&push),
        );
        let groups = render.map(|side| side.div_ceil(8));
        core.device.cmd_dispatch(command, groups[0], groups[1], 1);
        pass_barrier(core, command);
        transition(
            core,
            command,
            result,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );

        // The result back into the world's part of the frame, its alpha
        // kept.
        self.render_to(core, target)?;
        blit(core, self.blit_target, result.set.expect("sampled"), false);
        Ok(())
    }
}

impl State {
    /// The ground's grid of `mesh` on the device, ahead of the rain wetting
    /// the ground ([`Self::ground_grid`]).
    pub(super) unsafe fn upload_ground_grid(
        &mut self,
        core: &Core,
        mesh: &TerrainMesh,
    ) -> Result<(), VkDeviceError> {
        let mut mirror = match self.mirror.take() {
            Some(mirror) => mirror,
            None => Box::new(Mirror::new(core)?),
        };
        let result = self.ground_grid(core, &mut mirror, mesh);
        self.mirror = Some(mirror);
        result
    }

    /// The ground's grid of `mesh` in `mirror`, uploaded once for each mesh.
    unsafe fn ground_grid(
        &mut self,
        core: &Core,
        mirror: &mut Mirror,
        mesh: &TerrainMesh,
    ) -> Result<(), VkDeviceError> {
        if mirror
            .grid
            .as_ref()
            .is_some_and(|grid| grid.generation == mesh.generation)
        {
            return Ok(());
        }
        let command = self.begin(core)?;
        self.end_rendering(core);
        if let Some(old) = mirror.grid.take() {
            self.transient.extend([old.points, old.diagonals]);
        }
        // Each point's normal and height, all the pass reads, packed.
        let points: Vec<[f32; 4]> = mesh
            .vertices
            .iter()
            .map(|vertex| {
                let [x, y, z] = vertex.normal;
                [x, y, z, vertex.position[1]]
            })
            .collect();
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let points = self.device_buffer(core, bytes_of(&points), usage)?;
        let diagonals = self.device_buffer(core, bytes_of(&mesh.rising_diagonals()), usage)?;
        mirror.grid = Some(GroundGrid {
            generation: mesh.generation,
            points,
            diagonals,
        });
        let barrier = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ);
        core.device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&barrier)),
        );
        Ok(())
    }

    /// Draw `batches` (each its pipeline, its texture's set or none, and its
    /// triangles) into `image`, cleared, over the world's depth in
    /// `target`, placed as the raster placed them: `native` coordinates at
    /// `render` pixels, moved by `offset`, depth-tested.
    #[allow(clippy::too_many_arguments)]
    unsafe fn draw_over_depth(
        &mut self,
        core: &Core,
        image: &Image,
        target: &Target,
        native: [u32; 2],
        render: [u32; 2],
        offset: [f32; 2],
        batches: &[(vk::Pipeline, vk::DescriptorSet, &[DeviceVertex])],
    ) -> Result<(), VkDeviceError> {
        let command = self.begin(core)?;
        transition(
            core,
            command,
            image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        transition(
            core,
            command,
            &target.depth,
            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
        );
        let colour = [vk::RenderingAttachmentInfo::default()
            .image_view(image.view)
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
                .color_attachments(&colour)
                .depth_attachment(&depth),
        );
        if !batches.is_empty() {
            set_viewport(core, command, [0, 0], render, render);
            // The primitives' constants: their scale, depth-tested, and
            // jitter.
            let mut push = [0_u8; PRIMITIVE_PUSH];
            push[0..4].copy_from_slice(&(2.0 / native[0] as f32).to_le_bytes());
            push[4..8].copy_from_slice(&(2.0 / native[1] as f32).to_le_bytes());
            push[8..12].copy_from_slice(&1.0_f32.to_le_bytes());
            push[16..20].copy_from_slice(&(2.0 * offset[0] / render[0] as f32).to_le_bytes());
            push[20..24].copy_from_slice(&(2.0 * offset[1] / render[1] as f32).to_le_bytes());
            core.device.cmd_push_constants(
                command,
                core.primitive_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &push,
            );
            for &(pipeline, set, vertices) in batches {
                core.device
                    .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, pipeline);
                // SAFETY: DeviceVertex is plain repr(C) data without padding.
                let bytes = std::slice::from_raw_parts(
                    vertices.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(vertices),
                );
                let (buffer, at) = self.vertices(core, bytes)?;
                if set != vk::DescriptorSet::null() {
                    core.device.cmd_bind_descriptor_sets(
                        command,
                        vk::PipelineBindPoint::GRAPHICS,
                        core.primitive_layout,
                        0,
                        &[set],
                        &[],
                    );
                }
                core.device
                    .cmd_bind_vertex_buffers(command, 0, &[buffer], &[at]);
                core.device
                    .cmd_draw(command, vertices.len() as u32, 1, 0, 0);
            }
        }
        core.device.cmd_end_rendering(command);
        Ok(())
    }
}

/// The pass's constants: the camera's axes with the near plane and the
/// Fresnel term's least share and gain, the colour the water's strips fog
/// into and how the rain wets the ground, the murk's colour with the eye
/// under the water (w 1), its projection, the native size and the world's,
/// the draws' offset, and the murk's fade.
fn mirror_constants(
    camera: &SceneCamera,
    look: WaterMirror,
    fog: [f32; 3],
    wet: f32,
    render: [u32; 2],
    offset: [f32; 2],
) -> [f32; MIRROR_FLOATS] {
    let [right, up, forward] = camera.basis;
    let mut constants = [0.0_f32; MIRROR_FLOATS];
    constants[0..4].copy_from_slice(&[right[0], right[1], right[2], camera.near]);
    constants[4..8].copy_from_slice(&[up[0], up[1], up[2], look.least]);
    constants[8..12].copy_from_slice(&[forward[0], forward[1], forward[2], look.gain]);
    constants[12..16].copy_from_slice(&[fog[0], fog[1], fog[2], wet]);
    if let Some(murk) = look.murk {
        constants[16..20].copy_from_slice(&[murk.colour[0], murk.colour[1], murk.colour[2], 1.0]);
        constants[30..32].copy_from_slice(&murk.fade);
    }
    constants[20..30].copy_from_slice(&[
        camera.focal[0],
        camera.focal[1],
        camera.centre[0],
        camera.centre[1],
        camera.native[0] as f32,
        camera.native[1] as f32,
        render[0] as f32,
        render[1] as f32,
        offset[0],
        offset[1],
    ]);
    constants
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each level halves the one below, rounding up, to a single cell.
    #[test]
    fn the_pyramid_halves_down_to_one_cell() {
        assert_eq!(level_size([5, 3], 1), [3, 2]);
        assert_eq!(pyramid_levels([4, 2]), (2, 8 + 2 + 1));
        assert_eq!(pyramid_levels([1920, 1080]).0, 11);
        assert_eq!(pyramid_levels([1, 1]), (0, 1));
    }

    /// The byte offsets of the members of `module`'s push-constant block
    /// (SPIR-V: each `OpMemberDecorate ... Offset` of the block an
    /// `OpTypePointer PushConstant` points at), in their order.
    fn push_constant_offsets(module: &[u8]) -> Vec<u32> {
        let words: Vec<u32> = module
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect();
        let (mut blocks, mut offsets) = (Vec::new(), Vec::new());
        let mut at = 5;
        while at < words.len() {
            let (count, opcode) = ((words[at] >> 16) as usize, words[at] & 0xffff);
            let operands = &words[at + 1..at + count];
            match opcode {
                32 if operands[1] == 9 => blocks.push(operands[2]),
                72 if operands[2] == 35 => offsets.push((operands[0], operands[1], operands[3])),
                _ => {}
            }
            at += count;
        }
        let block = blocks
            .into_iter()
            .find(|block| offsets.iter().any(|(of, ..)| of == block))
            .expect("a push-constant block");
        offsets.retain(|(of, ..)| *of == block);
        offsets.sort_by_key(|(_, member, _)| *member);
        offsets.into_iter().map(|(.., offset)| offset).collect()
    }

    /// The names of the members of the push-constant block `block` in the
    /// GLSL `source`, in their order.
    fn push_constant_names<'a>(source: &'a str, block: &str) -> Vec<&'a str> {
        let start = source.find(block).expect("the block");
        let body = &source[start + block.len()..];
        body[..body.find('}').unwrap()]
            .lines()
            .filter_map(|line| line.trim().strip_suffix(';'))
            .map(|line| line.rsplit(' ').next().unwrap())
            .collect()
    }

    /// The pass's constants lie where `vk_mirror.comp` reads them, member
    /// by member as compiled, so that a slip cannot move the water's look
    /// above the surface; the depth pyramid's too.
    #[test]
    fn the_constants_lie_where_the_shaders_read_them() {
        let camera = SceneCamera {
            eye: [0.0; 3],
            basis: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
            focal: [430.0, 431.0],
            centre: [320.5, 240.5],
            native: [640, 480],
            near: 10.0,
            far: 20_000.0,
        };
        let look = WaterMirror {
            least: 0.25,
            gain: 1.5,
            murk: Some(MURK),
        };
        let fog = [0.4, 0.5, 0.6];
        let constants = mirror_constants(
            &camera,
            look,
            fog,
            Wetness::UnderCube.constant(MirrorStage::Whole),
            [1600, 900],
            [0.125, -0.375],
        );
        let members: [(&str, &[f32]); 15] = [
            ("right", &camera.basis[0]),
            ("near", &[camera.near]),
            ("up", &camera.basis[1]),
            ("least", &[look.least]),
            ("forward", &camera.basis[2]),
            ("gain", &[look.gain]),
            ("fog", &[0.4, 0.5, 0.6]),
            ("wet", &[2.0]),
            ("murk", &[0.1, 0.2, 0.3, 1.0]),
            ("focal", &camera.focal),
            ("centre", &camera.centre),
            ("native", &[640.0, 480.0]),
            ("world", &[1600.0, 900.0]),
            ("offset", &[0.125, -0.375]),
            ("murk_fade", &MURK.fade),
        ];
        let source = include_str!("../../shaders/vk_mirror.comp");
        let names = push_constant_names(source, "layout(push_constant) uniform Camera {");
        assert_eq!(names, members.map(|(name, _)| name));
        let offsets = push_constant_offsets(MIRROR);
        assert_eq!(offsets.len(), members.len());
        for (offset, (name, values)) in offsets.into_iter().zip(members) {
            let at = offset as usize / 4;
            assert_eq!(&constants[at..at + values.len()], values, "{name}");
            if name == "murk_fade" {
                assert_eq!(at + values.len(), MIRROR_FLOATS);
            }
        }
        // Above the water the murk's members are zero: its w is the
        // shader's switch; dry, the ground's switch is.
        let above = mirror_constants(
            &camera,
            WaterMirror { murk: None, ..look },
            fog,
            Wetness::Dry.constant(MirrorStage::Whole),
            [1600, 900],
            [0.125, -0.375],
        );
        assert_eq!(above[15..20], [0.0; 5]);
        assert_eq!(above[30..32], [0.0; 2]);
        assert_eq!(above[..15], constants[..15]);
        assert_eq!(above[20..30], constants[20..30]);
        // The wet ground's pass with the water left for the next: its
        // switch below zero.
        assert_eq!(Wetness::UnderCube.constant(MirrorStage::GroundFirst), -2.0);
        assert_eq!(Wetness::Wet.constant(MirrorStage::GroundFirst), -1.0);

        // The pyramid's: the world's size, the level and the eye under the
        // water, as `finish_mirror_with` pushes them.
        let source = include_str!("../../shaders/vk_mirror_depth.comp");
        let names = push_constant_names(source, "layout(push_constant) uniform Build {");
        assert_eq!(names, ["size", "level", "under"]);
        assert_eq!(push_constant_offsets(PYRAMID), [0, 8, 12]);
    }

    const MURK: Murk = Murk {
        colour: [0.1, 0.2, 0.3],
        fade: [256.0, 4096.0],
    };

    fn corner(diffuse: [u8; 4], fog: [u8; 4]) -> DeviceVertex {
        DeviceVertex {
            position: [0.0; 3],
            texture: [0.0; 3],
            diffuse,
            fog,
        }
    }

    /// The murk closes as retail's fog does: all shows to its start, none
    /// past its end, evenly between.
    #[test]
    fn the_murk_closes_evenly_over_its_fade() {
        assert_eq!(MURK.shows(0.0), 1.0);
        assert_eq!(MURK.shows(256.0), 1.0);
        assert_eq!(MURK.shows(256.0 + 960.0), 0.75);
        assert_eq!(MURK.shows(4096.0), 0.0);
        assert_eq!(MURK.shows(1.0e6), 0.0);
    }

    /// A textured corner's fog becomes retail's fog and the murk in front
    /// of it as one: laid over a colour, the same as the two in turn.
    #[test]
    fn a_held_textured_corner_takes_the_murk_over_its_own_fog() {
        let fogged = corner([255; 4], [200, 100, 50, 128]);
        let t = 1500.0;
        let held = MURK.through(fogged, t, true, false);
        let clear = MURK.shows(t);
        let unit = |byte: u8| f32::from(byte) / 255.0;
        let mix = |a: f32, b: f32, share: f32| a + (b - a) * share;
        for (channel, colour) in [0.9_f32, 0.5, 0.2].into_iter().enumerate() {
            let in_turn = mix(
                MURK.colour[channel],
                mix(unit(fogged.fog[channel]), colour, unit(fogged.fog[3])),
                clear,
            );
            let as_one = mix(unit(held.fog[channel]), colour, unit(held.fog[3]));
            assert!(
                (in_turn - as_one).abs() < 2.0 / 255.0,
                "{channel}: {in_turn} {as_one}"
            );
        }
        assert_eq!(held.diffuse, fogged.diffuse);
        // Unfogged at the eye, it stays so.
        let clear_corner = corner([255; 4], [0, 0, 0, 255]);
        assert_eq!(MURK.through(clear_corner, 0.0, true, false), clear_corner);
    }

    /// What adds light fades to nothing; a flat corner takes the murk in
    /// its colours.
    #[test]
    fn held_light_fades_out_and_flat_colours_take_the_murk() {
        let t = 2000.0;
        let clear = MURK.shows(t);
        let added = MURK.through(corner([255; 4], [0, 0, 0, 255]), t, true, true);
        assert_eq!(added.fog, [0, 0, 0, (clear * 255.0).round() as u8]);
        let flat = MURK.through(corner([255, 0, 128, 77], [0; 4]), t, false, false);
        let expect =
            |colour: f32, murk: f32| ((murk + (colour - murk) * clear) * 255.0).round() as u8;
        assert_eq!(
            flat.diffuse,
            [
                expect(1.0, 0.1),
                expect(0.0, 0.2),
                expect(128.0 / 255.0, 0.3),
                77
            ]
        );
        let flat_added = MURK.through(corner([200, 100, 0, 255], [0; 4]), t, false, true);
        assert_eq!(
            flat_added.diffuse[0],
            (200.0 / 255.0 * clear * 255.0_f32).round() as u8
        );
    }
}
