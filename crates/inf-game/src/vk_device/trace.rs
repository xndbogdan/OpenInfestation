//! The modern mode's path tracer on the Vulkan device
//! ([the plan](../../../../docs/INFESTATION-RT-IMPLEMENTATION.md)): the
//! frame's [scene feed](crate::scene_feed) models in a bottom-level
//! acceleration structure rebuilt every frame, the ground in one built once
//! per terrain mesh, a top level with the models and nine copies of the
//! ground around the eye (the world wraps), and a compute shader tracing it
//! with ray queries: each pixel's surface, its direct light and one bounce
//! of light. The bounce light is denoised (a temporal pass following each
//! point back to the last frame, then an edge-aware a-trous filter), and a
//! composite draws the lit result and its depth into the frame's target
//! before the raster's later draws. Under thermal vision the trace gives
//! each surface's heat instead, which the resolve shows in grey over the
//! camera's range.

use super::upscale::{Guides, UpscaleFrame, Upscaler, UpscalerChoice};
use super::*;
use crate::scene_feed::{SceneFeed, SceneSurface};
use crate::scene_terrain::TerrainMeshVertex;
use std::{collections::HashMap, rc::Rc};

static TRACE: &[u8] = include_bytes!("../../shaders/pt_trace.comp.spv");
/// The same source built as the path pass (`PATHS_PASS`): the bounce light
/// from the primary hits the trace left (the full path tracing plan's
/// stage 4).
static PATHS: &[u8] = include_bytes!("../../shaders/pt_paths.comp.spv");
/// And built as the emitters' pass (`EMITTERS_PASS`): the emissive faces and
/// the lava sampled as lights for the path pass's bounces (stage 5).
static EMITTER_SAMPLING: &[u8] = include_bytes!("../../shaders/pt_emitters.comp.spv");
/// The radiance cache's resolve (`pt_cache.glsl`).
static CACHE: &[u8] = include_bytes!("../../shaders/pt_cache.comp.spv");
/// The raster's draws over the traced world, which find their light with a
/// ray (the full path tracing plan's stage 7).
static RASTER_LIGHT: &[u8] = include_bytes!("../../shaders/vk_primitive_lit.vert.spv");
/// The radiance cache's slots and their size (`pt_cache.glsl`'s
/// `CACHE_SIZE` and `CacheEntry`).
const CACHE_SLOTS: u64 = 1 << 20;
const CACHE_BYTES: u64 = CACHE_SLOTS * 36;
static TEMPORAL: &[u8] = include_bytes!("../../shaders/pt_temporal.comp.spv");
static ATROUS: &[u8] = include_bytes!("../../shaders/pt_atrous.comp.spv");
/// The same two built as the glossy lobe's passes (`GLOSSY_PASS`), which
/// run over the pixels with a glossy lobe beside the diffuse passes, so
/// those keep their registers (the full path tracing plan's stage 6).
static TEMPORAL_GLOSSY: &[u8] = include_bytes!("../../shaders/pt_temporal_glossy.comp.spv");
static ATROUS_GLOSSY: &[u8] = include_bytes!("../../shaders/pt_atrous_glossy.comp.spv");
static RESOLVE: &[u8] = include_bytes!("../../shaders/pt_resolve.comp.spv");
static DEPTH: &[u8] = include_bytes!("../../shaders/pt_depth.comp.spv");
pub(super) static FXAA: &[u8] = include_bytes!("../../shaders/pt_fxaa.comp.spv");
pub(super) static TAA: &[u8] = include_bytes!("../../shaders/pt_taa.comp.spv");
static FOG_INJECT: &[u8] = include_bytes!("../../shaders/pt_fog_inject.comp.spv");
static FOG_INTEGRATE: &[u8] = include_bytes!("../../shaders/pt_fog_integrate.comp.spv");
static THERMAL_GAIN: &[u8] = include_bytes!("../../shaders/pt_thermal_gain.comp.spv");
static COMPOSITE: &[u8] = include_bytes!("../../shaders/pt_composite.frag.spv");

/// The most textures one frame's feed may name.
const TEXTURE_SLOTS: u32 = 4096;
const TEXTURE_BINDING: u32 = 63;
const NO_LIGHT: u32 = u32::MAX;
const NO_TEXTURE: u32 = u32::MAX;
/// A matte material: no metal, fully rough (packed as the shader reads it).
const MATTE: u32 = 0xff00;

/// A material packed as the shader reads it: metalness in the low byte,
/// roughness in the next (255 is one).
fn packed_material(material: crate::modern_tuning::Material) -> u32 {
    let byte = |value: f32| (value * 255.0).round() as u32;
    byte(material.metal) | byte(material.rough) << 8
}

/// A model's look by its name: the heat `names` gives it (none: its
/// object's, or `models`), its material packed as metalness and roughness
/// bytes, whether it is a gun or a wreck, and how many times brighter than
/// retail its unlit faces glow (`materials.emitters`).
#[derive(Clone, Copy, Debug)]
struct ModelLook {
    heat: Option<f32>,
    material: u32,
    gun: bool,
    wreck: bool,
    emission: f32,
    crystal: bool,
}
/// Albedo and fog: colours.
pub(super) const COLOR_IMAGE: vk::Format = vk::Format::R8G8B8A8_UNORM;
/// Light, the bounce light's history and the geometry (normal and
/// distance).
pub(super) const LIGHT_IMAGE: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
pub(super) const TRACED_DEPTH: vk::Format = vk::Format::R32_SFLOAT;
/// Motion: where each pixel's point was in the last frame, in render pixels.
pub(super) const MOTION_IMAGE: vk::Format = vk::Format::R16G16_SFLOAT;
/// The frame block's floats, in `pt_frame.glsl`'s scalar layout.
const FRAME_FLOATS: usize = 267
    + 1
    + 4 * MAX_SPARKS
    + MAX_SPARKS
    + 4 * MAX_SPARKS
    + 1
    + 1
    + 4 * MAX_WAKES
    + 1
    + 4 * crate::craft_spray::MOST_RINGS
    + MAX_LAMPS * 16
    + 4 * MAX_SPARKS
    + 4 * MAX_SPARKS * 2 * crate::crystal_flares::ARC_PAIRS
    + 9
    + 4 * crate::cloud_wake::WAKE_POINTS;
/// The struck crystals' sparks the tracer takes at once.
const MAX_SPARKS: usize = 4;
/// How far past the craft's middle the clouds' near band reaches, in world
/// units: past its far side, so all the craft keeps in sight.
const CRAFT_REACH: f32 = 64.0;
/// The places a craft stirs puddles and water from at once: a wheeled
/// form's four wheels and its hull (`pt_frame.glsl`'s `wakes`).
const MAX_WAKES: usize = 6;
/// The most lamps a frame lights (`pt_frame.glsl`'s `MAX_LAMPS`).
const MAX_LAMPS: usize = 16;
/// The most fields of energy a frame shows (`scene_field`): the nearest.
const FIELDS: usize = 2;
/// The most splashes the water shows a frame: the eruptions and where their
/// water falls back, then the nearest of retail's.
pub(super) const SPLASHES: usize = 128;
/// The most deposited lights the ground shadows a frame: the nearest.
const LIGHT_SOURCES: usize = 64;
/// The most blended sprites and beam segments the water, puddles and metal
/// mirror, the nearest first.
const EMITTERS: usize = 256;

/// Retail's light grid's cells (90 by 90), which the heat memory follows in
/// the same buffer (`pt_light_grid.glsl`).
const GRID_CELLS: usize = 90 * 90;
/// The mist's grid: cells across a view (down follows its shape) and
/// slices deep.
const MIST_ACROSS: u32 = 160;
const MIST_SLICES: u32 = 64;
/// The a-trous passes: step, source layer, destination layer (layer 0 is
/// the frame's history, 1 and 2 the filtered pair) and whether it is the
/// last: each of the others is followed by the glossy lobe's pass at its
/// step, and the last weighs the glossy light in with the diffuse's. The
/// result lands in 1.
const ATROUS_PASSES: [[i32; 4]; 3] = [[1, 0, 1, 0], [2, 1, 2, 0], [4, 2, 1, 1]];
/// Presents a sprite image stays unused before it is released.
const TEXTURE_KEEP_PRESENTS: u64 = 1800;
/// The game time (microseconds) a ground detail patch builds on the worker
/// before it takes the last one's place, waited for then if not done (it
/// takes about 20 ms): at a set point of the game's clock, not whenever the
/// worker finishes, so runs at a fixed step trace alike.
const PATCH_MICROS: u64 = 100_000;
/// The traced frame's stages, each timed from the timestamp before it to
/// the one after: the sprites' uploads, the geometry's uploads and builds,
/// the modern sky's clouds, the trace, the denoiser (the bounce light's
/// history and filter, or thermal vision's gain), the resolve (and the
/// display's depth), the anti-aliasing, the upscaler, the mist and the
/// composite. A stage a frame skips times nothing.
const STAGES: [&str; 10] = [
    "textures",
    "geometry",
    "sky",
    "trace",
    "denoise",
    "resolve",
    "aa",
    "upscale",
    "mist",
    "composite",
];
const TIMESTAMPS: u32 = STAGES.len() as u32 + 1;

struct Acceleration {
    handle: vk::AccelerationStructureKHR,
    buffer: Buffer,
    capacity: u64,
}

/// A host-written buffer kept between frames and grown when a frame needs
/// more.
struct Grown {
    buffer: Buffer,
    capacity: u64,
}

/// The ground's buffers in device memory, for one terrain mesh: its points
/// in grid order (which the shaders also read by position), and its
/// triangles by tile ([`crate::scene_ground_detail::tile_ground`]).
struct Ground {
    generation: u64,
    vertices: Buffer,
    indices: Buffer,
    triangles: Buffer,
    /// The points and tiles the detail patch is built from, shared with the
    /// thread that builds it.
    source: std::sync::Arc<crate::scene_ground_detail::DetailGround>,
    /// The tiles whose [`Tracer::ground_tiles`] do not hold this mesh yet.
    unbuilt: Vec<usize>,
}

/// The ground's detail patch round the eye in device memory.
struct Detail {
    key: crate::scene_ground_detail::PatchKey,
    vertices: Buffer,
    indices: Buffer,
    triangles: Buffer,
    count: u32,
    vertex_count: u32,
    /// Whether [`Tracer::detail_blas`] holds this patch yet.
    built: bool,
}

/// A sprite as the tracer samples it: its own image without the page's
/// border, with a mip chain, so distant surfaces filter instead of
/// shimmering and no mip reaches a neighbouring sprite.
struct TracedTexture {
    image: Image,
    last_used: u64,
    /// Its texels' mean light, which an emissive face's power counts.
    brightness: f32,
}

/// The tracer's GPU time, from timestamps read back a frame later.
#[derive(Default)]
struct TraceTiming {
    /// Each frame slot's last timestamps wait to be read.
    pending: [bool; FRAMES_IN_FLIGHT],
    /// Frames whose timestamps were read, and those measured after the
    /// warm-up ([`super::perf::WARMUP_FRAMES`]).
    read: u64,
    frames: u64,
    /// Nanoseconds in each of [`STAGES`], summed and at most.
    sums: [f64; STAGES.len()],
    most: [f64; STAGES.len()],
    worst: f64,
    extent: [u32; 2],
}

/// The last traced frame's view, which the bounce light's history follows.
#[derive(Clone, Copy)]
pub(super) struct PreviousView {
    basis: [[f32; 3]; 3],
    focal: [f32; 2],
    centre: [f32; 2],
    /// X and Z within the wrap.
    eye: [f32; 3],
    native: [u32; 2],
    extent: [u32; 2],
    /// The depth the mist's slices reached.
    volume_far: f32,
}

impl PreviousView {
    /// `camera`'s view, for images `extent` pixels.
    pub(super) fn of(camera: &crate::scene_feed::SceneCamera, extent: [u32; 2]) -> Self {
        let eye = camera.eye;
        Self {
            basis: camera.basis,
            focal: camera.focal,
            centre: camera.centre,
            eye: [
                eye[0].rem_euclid(WORLD_WRAP) as f32,
                eye[1] as f32,
                eye[2].rem_euclid(WORLD_WRAP) as f32,
            ],
            native: camera.native,
            extent,
            volume_far: 0.0,
        }
    }

    /// Whether a frame `extent` pixels with `native` coordinates can follow
    /// this view.
    pub(super) fn fits(&self, extent: [u32; 2], native: [u32; 2]) -> bool {
        self.extent == extent && self.native == native
    }
}

/// The world's wrap: the ground repeats every 65,536 units on X and Z.
const WORLD_WRAP: f64 = 65536.0;
/// Instance custom indices, as `pt_trace.comp` reads them.
const INSTANCE_MODELS: u32 = 0;
const INSTANCE_GROUND: u32 = 1;
const INSTANCE_WATER: u32 = 2;
const INSTANCE_SKY: u32 = 3;
/// The models' triangles in masked sprites, which rays test texel by texel.
const INSTANCE_CUTOUT: u32 = 4;
/// The additive sprites sunk under the water, which only the water's look
/// below it sums (pt_trace.comp's sunk_glow).
const INSTANCE_GLOW: u32 = 5;
/// The ground's detail patch round the eye ([crate::scene_ground_detail]),
/// in place of the coarse ground's tiles under it.
const INSTANCE_DETAIL: u32 = 6;
/// How many of a new ground's tiles build together.
const TILE_BATCH: usize = 32;
/// Instance masks: water casts no shadow, so shadow rays leave it out; the
/// ground also answers to its own bit, for rays only it may stop.
const MASK_SOLID: u8 = 0x01;
const MASK_WATER: u8 = 0x02;
const MASK_GROUND: u8 = 0x04;
/// Retail's painted backdrop, which only reflections look for.
const MASK_SKY: u8 = 0x08;
/// The sunk additive sprites, which nothing but the water looks for.
const MASK_GLOW: u8 = 0x10;
/// The models and their masked sprites without the ground: the local
/// lights' shadow rays take the ground against its smooth surface instead
/// (pt_trace.comp's `ground_through`).
const MASK_MODEL: u8 = 0x20;
/// The models outside the view and the craft round a first-person eye,
/// drawn for the tracer alone (world_scene's outside the view): every ray
/// but the eye's own looks for them (pt_scene.glsl).
const MASK_OUTSIDE: u8 = 0x40;
/// Their custom index's bit beside their kind: the radiance cache leaves
/// them out (pt_trace.comp's `shade_ray`).
const INSTANCE_OUTSIDE: u32 = 0x80;
/// A sky triangle's material bit: its sprite is masked, an opaque object.
const SKY_OBJECT_MASKED: u32 = 1 << 16;
/// A model triangle's material bit: its draw fogs it in a fog of its own
/// (the actor flash), which the distance fog leaves it.
const OWN_FOG: u32 = 1 << 17;
/// A model triangle's material bit: its surface is bare, no paint under a
/// clear coat (a sprite's own material, the tyres' rubber), which the rain
/// darkens and smooths rather than coats.
const BARE: u32 = 1 << 18;
/// A model triangle's material bit: one of the scenery crystals, the only
/// faces a struck crystal's spark runs over (pt_trace.comp's `sparks_at`).
const CRYSTAL: u32 = 1 << 19;

pub(super) struct Tracer {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    composite_set_layout: vk::DescriptorSetLayout,
    composite_layout: vk::PipelineLayout,
    composite: vk::Pipeline,
    /// The raster's textured draws over the traced world: their texture,
    /// then the composite's set, whose fog grid they take at their depth.
    raster_fog_layout: vk::PipelineLayout,
    /// This frame's composite set, while its world's draws go on.
    composite_frame_set: vk::DescriptorSet,
    composite_module: vk::ShaderModule,
    trace_module: vk::ShaderModule,
    paths: vk::Pipeline,
    paths_module: vk::ShaderModule,
    emitters_pass: vk::Pipeline,
    emitters_module: vk::ShaderModule,
    /// The vertex shader of the raster's draws over the traced world.
    raster_light_vertex: vk::ShaderModule,
    temporal_set_layout: vk::DescriptorSetLayout,
    temporal_layout: vk::PipelineLayout,
    temporal: vk::Pipeline,
    temporal_module: vk::ShaderModule,
    temporal_glossy: vk::Pipeline,
    temporal_glossy_module: vk::ShaderModule,
    atrous_set_layout: vk::DescriptorSetLayout,
    atrous_layout: vk::PipelineLayout,
    atrous: vk::Pipeline,
    atrous_module: vk::ShaderModule,
    atrous_glossy: vk::Pipeline,
    atrous_glossy_module: vk::ShaderModule,
    resolve_set_layout: vk::DescriptorSetLayout,
    resolve_layout: vk::PipelineLayout,
    resolve: vk::Pipeline,
    resolve_module: vk::ShaderModule,
    /// The display's depth, on the trace's own set and layout.
    depth_pass: vk::Pipeline,
    depth_module: vk::ShaderModule,
    /// The traced world's own anti-aliasing (one set layout for both).
    aa_set_layout: vk::DescriptorSetLayout,
    aa_layout: vk::PipelineLayout,
    fxaa: vk::Pipeline,
    fxaa_module: vk::ShaderModule,
    taa: vk::Pipeline,
    taa_module: vk::ShaderModule,
    /// The mist: lighting its cells, then gathering them along the view.
    mist_set_layout: vk::DescriptorSetLayout,
    mist_layout: vk::PipelineLayout,
    mist: vk::Pipeline,
    mist_module: vk::ShaderModule,
    gather_set_layout: vk::DescriptorSetLayout,
    gather_layout: vk::PipelineLayout,
    gather: vk::Pipeline,
    gather_module: vk::ShaderModule,
    /// Thermal vision's automatic gain.
    gain_set_layout: vk::DescriptorSetLayout,
    gain_layout: vk::PipelineLayout,
    gain: vk::Pipeline,
    gain_module: vk::ShaderModule,
    /// The radiance cache's resolve, and the cache (the light arriving at
    /// the surfaces the paths reach), emptied when the world changes.
    cache_set_layout: vk::DescriptorSetLayout,
    cache_layout: vk::PipelineLayout,
    cache_resolve: vk::Pipeline,
    cache_module: vk::ShaderModule,
    radiance_cache: Option<Grown>,
    cache_world: Option<u32>,
    /// The paths' random numbers (blue_noise.rs), on the device.
    blue_noise: Option<Buffer>,
    /// One pool for each frame in flight, as every buffer the CPU writes
    /// for a frame: the device records the next frame while the GPU still
    /// reads the last.
    pool: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
    vertices: [Option<Grown>; FRAMES_IN_FLIGHT],
    indices: [Option<Grown>; FRAMES_IN_FLIGHT],
    triangles: [Option<Grown>; FRAMES_IN_FLIGHT],
    frame: [Option<Grown>; FRAMES_IN_FLIGHT],
    instances: [Option<Grown>; FRAMES_IN_FLIGHT],
    scratch: Option<Grown>,
    blends: [Option<Grown>; FRAMES_IN_FLIGHT],
    grid: [Option<Grown>; FRAMES_IN_FLIGHT],
    model_lights: [Option<Grown>; FRAMES_IN_FLIGHT],
    /// Retail's splashes on the water, the nearest first.
    splashes: [Option<Grown>; FRAMES_IN_FLIGHT],
    /// The frame's deposited lights, the nearest first.
    sources: [Option<Grown>; FRAMES_IN_FLIGHT],
    /// The spray: its sources, then their drops ([`crate::scene_spray`]).
    spray: [Option<Grown>; FRAMES_IN_FLIGHT],
    /// The thrusters' trails' puffs ([`crate::scene_trails`]), after a
    /// header: how many, and the box they fill.
    trail_puffs: [Option<Grown>; FRAMES_IN_FLIGHT],
    /// The blended sprites that add light, the nearest first, which Eye
    /// candy mirrors.
    emitters: [Option<Grown>; FRAMES_IN_FLIGHT],
    /// The emissive light list's faces and groups (emissive.rs).
    emissive_faces: [Option<Grown>; FRAMES_IN_FLIGHT],
    emissive_groups: [Option<Grown>; FRAMES_IN_FLIGHT],
    /// Each triangle's face in the list, apart from its record.
    triangle_emitters: [Option<Grown>; FRAMES_IN_FLIGHT],
    blas: Option<Acceleration>,
    water_blas: Option<Acceleration>,
    sky_blas: Option<Acceleration>,
    /// The masked sprites' triangles, not opaque: rays test their texels.
    cutout_blas: Option<Acceleration>,
    /// The sunk additive sprites' triangles, not opaque: the water sums
    /// every one its ray crosses.
    glow_blas: Option<Acceleration>,
    /// The models outside the view and their masked sprites, which only
    /// the scene's own rays meet (`MASK_OUTSIDE`).
    outside_blas: Option<Acceleration>,
    outside_cutout_blas: Option<Acceleration>,
    tlas: Option<Acceleration>,
    ground: Option<Ground>,
    /// Each ground tile's bottom level, X-major (none for an empty tile).
    ground_tiles: Vec<Option<Acceleration>>,
    detail: Option<Detail>,
    detail_blas: Option<Acceleration>,
    /// The next patch, while a worker builds it, and the game's clock when
    /// it started; the last stays drawn.
    patch_job: Option<(
        u64,
        std::thread::JoinHandle<crate::scene_ground_detail::DetailPatch>,
    )>,
    /// Sprite images by content key.
    textures: HashMap<u64, TracedTexture>,
    /// A sampler a Filtering value (settings `+3C`): Point and Bilinear on
    /// the top level only, as retail, then trilinear, then anisotropic 2x
    /// to 16x where the device allows; all clamped at the edges.
    samplers: [vk::Sampler; 7],
    timestamps: [vk::QueryPool; FRAMES_IN_FLIGHT],
    /// Nanoseconds a timestamp tick.
    timestamp_period: f64,
    timing: TraceTiming,
    /// This frame's surfaces: albedo, direct light, one bounce sample, fog
    /// and the raster's depth.
    albedo: Option<Image>,
    direct: Option<Image>,
    bounce: Option<Image>,
    fog: Option<Image>,
    depth: Option<Image>,
    /// This frame's geometry and the last frame's, by `parity`.
    geometry: [Option<Image>; 2],
    /// The bounce light's history, by `parity`.
    history: [Option<Image>; 2],
    /// The anti-lag's record beside it, by `parity`: the share of the
    /// samples it took that lay above its mean, and the evidence of the run
    /// of frames whose samples all lay on one side, by which it gives way
    /// where the light changed (pt_temporal.comp's `lag_test`).
    lag_record: [Option<Image>; 2],
    /// Which lobe the path pass's bounce took at each pixel it traced: the
    /// glossy ray's length, or -1 where it was diffuse.
    glossy_hit: Option<Image>,
    /// The glossy bounce light's history, by `parity` (a metal's, apart from
    /// the diffuse), and the filter's pair for it.
    glossy: [Option<Image>; 2],
    glossy_filtered: [Option<Image>; 2],
    /// Beside the glossy history, by `parity`: its short history, and how
    /// far off the light it holds lies along its rays (the mean of the
    /// glossy rays' lengths as its light's).
    glossy_track: [Option<Image>; 2],
    /// The sun's visibility history, by `parity`, and the primary hits' sun
    /// before its shadow.
    shadows: [Option<Image>; 2],
    sun: Option<Image>,
    /// Each primary hit's own motion: where its model's point was in the
    /// last frame, less where it is (zero for what keeps still).
    object_motion: Option<Image>,
    /// Each primary hit's flat face, metalness and roughness, which the path
    /// pass reads.
    path_surface: Option<Image>,
    /// Each primary hit's specular light from the sun, before its shadow.
    specular: Option<Image>,
    /// The filter's ping-pong pair; the first ends up filtered.
    filtered: [Option<Image>; 2],
    /// The resolved colour and its motion, at the render size.
    color: Option<Image>,
    motion: Option<Image>,
    /// The depth at the display's size, when the tracer renders smaller.
    display_depth: Option<Image>,
    /// FSR's context and its output at the display's size.
    upscaler: Option<Upscaler>,
    upscaled: Option<Image>,
    /// Ray Reconstruction's guides at the render size (one pixel without
    /// it): diffuse albedo, normals with roughness, specular albedo.
    guides: [Option<Image>; 3],
    /// The anti-aliased colour, by `parity` (TAA's history; one pixel
    /// without anti-aliasing).
    aa_history: [Option<Image>; 2],
    /// The size TAA last wrote its history at; none when the last frame
    /// ran none.
    taa_extent: Option<[u32; 2]>,
    /// SMAA's passes, made on its first use.
    smaa: Option<super::smaa::Smaa>,
    /// Each sprite's mean colour by its content, for the glows' light, and
    /// how much light it holds.
    texture_means: HashMap<u64, [f32; 3]>,
    texture_energy: HashMap<u64, f32>,
    /// The blasts in the water showing.
    eruptions: crate::scene_spray::Eruptions,
    /// The thrusters' trails showing.
    trails: crate::scene_trails::Trails,
    /// The thruster lights' glow.
    thrust_glow: crate::scene_trails::ThrustGlow,
    /// The mist's cells by `parity` (each frame blends into the last's),
    /// and the mist gathered along the view.
    mist_cells: [Option<Image>; 2],
    mist_gathered: Option<Image>,
    /// The last frame lit the mist, so its cells can be followed.
    mist_ran: bool,
    /// The last frame's mist held the water's light (the remastered
    /// camera's eye under the water): its cells hold another thing than
    /// the air's.
    mist_under: bool,
    /// Thermal vision's heat (one pixel without it), and the camera's range
    /// between frames.
    heat: Option<Image>,
    gain_range: Option<Grown>,
    /// The last frame ran thermal vision, so its range can be followed.
    thermal_ran: bool,
    /// The heat the warming lights left on the ground, for thermal vision.
    heat_memory: super::heat_memory::HeatMemory,
    /// Each model name's look, under the tuning it was taken from.
    model_heat: HashMap<Box<str>, ModelLook>,
    /// What each object drawn does to its heat.
    object_heat: super::object_heat::ObjectHeat,
    heat_tuning: Option<Rc<crate::modern_tuning::ModernTuning>>,
    /// When the last traced frame began, for the upscalers' frame time.
    last_frame: Option<std::time::Instant>,
    /// The upscalers that failed (each logged once): chosen again, the
    /// tracer scales by itself.
    failed_upscalers: Vec<crate::modern_settings::Upscaler>,
    /// Counts the jittered frames, for the jitter's sequence.
    jitter_index: u32,
    parity: usize,
    frame_index: u32,
    previous: Option<PreviousView>,
    scratch_alignment: u64,
}

impl Tracer {
    pub(super) unsafe fn new(core: &Core) -> Result<Self, VkDeviceError> {
        let device = &core.device;
        let binding = |index: u32, kind: vk::DescriptorType, count: u32| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(index)
                .descriptor_type(kind)
                .descriptor_count(count)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        };
        let bindings = [
            binding(0, vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, 1),
            binding(1, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(2, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(3, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(4, vk::DescriptorType::UNIFORM_BUFFER, 1),
            binding(5, vk::DescriptorType::STORAGE_IMAGE, 1),
            binding(6, vk::DescriptorType::STORAGE_IMAGE, 1),
            binding(7, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(8, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(9, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(10, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(11, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(12, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(13, vk::DescriptorType::STORAGE_IMAGE, 1),
            binding(14, vk::DescriptorType::STORAGE_IMAGE, 1),
            binding(15, vk::DescriptorType::STORAGE_IMAGE, 1),
            binding(16, vk::DescriptorType::STORAGE_IMAGE, 1),
            binding(17, vk::DescriptorType::STORAGE_IMAGE, 1),
            binding(18, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(19, vk::DescriptorType::STORAGE_BUFFER, 1),
            // The display's depth, which the depth pass writes.
            binding(20, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The primary hits' sun before its shadow.
            binding(21, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The spray's drops.
            binding(22, vk::DescriptorType::STORAGE_BUFFER, 1),
            // The primary hits' own motion.
            binding(25, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The primary hits' faces and materials, for the path pass.
            binding(27, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The radiance cache (pt_cache.glsl).
            binding(31, vk::DescriptorType::STORAGE_BUFFER, 1),
            // The primary hits' specular light from the sun.
            binding(32, vk::DescriptorType::STORAGE_IMAGE, 1),
            // Which lobe the path pass's bounce took.
            binding(36, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The blended sprites the water and metal mirror.
            binding(26, vk::DescriptorType::STORAGE_BUFFER, 1),
            // The emissive light list's faces and groups, and each
            // triangle's face in it (emissive.rs).
            binding(33, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(34, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(35, vk::DescriptorType::STORAGE_BUFFER, 1),
            // The paths' blue noise.
            binding(37, vk::DescriptorType::STORAGE_BUFFER, 1),
            // The ground's detail patch round the eye: its points, indices
            // and triangle words (scene_ground_detail.rs).
            binding(28, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(29, vk::DescriptorType::STORAGE_BUFFER, 1),
            binding(30, vk::DescriptorType::STORAGE_BUFFER, 1),
            // The modern sky's cube map and the clouds' shadow map.
            binding(23, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, 1),
            binding(24, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, 1),
            // The variable-size array must take the highest binding.
            binding(
                TEXTURE_BINDING,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                TEXTURE_SLOTS,
            ),
        ];
        // One flag set per binding, in the bindings' order: the textures'
        // array alone partly bound and of variable size.
        let flags: Vec<vk::DescriptorBindingFlags> = bindings
            .iter()
            .map(|entry| {
                if entry.binding == TEXTURE_BINDING {
                    vk::DescriptorBindingFlags::PARTIALLY_BOUND
                        | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
                } else {
                    vk::DescriptorBindingFlags::empty()
                }
            })
            .collect();
        let mut binding_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);
        let mut tracer = Self {
            set_layout: device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&bindings)
                        .push_next(&mut binding_flags),
                    None,
                )
                .map_err(call("tracer descriptor layout"))?,
            layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            composite_set_layout: vk::DescriptorSetLayout::null(),
            composite_layout: vk::PipelineLayout::null(),
            raster_fog_layout: vk::PipelineLayout::null(),
            composite_frame_set: vk::DescriptorSet::null(),
            composite: vk::Pipeline::null(),
            composite_module: vk::ShaderModule::null(),
            trace_module: vk::ShaderModule::null(),
            paths: vk::Pipeline::null(),
            paths_module: vk::ShaderModule::null(),
            emitters_pass: vk::Pipeline::null(),
            emitters_module: vk::ShaderModule::null(),
            raster_light_vertex: vk::ShaderModule::null(),
            temporal_set_layout: vk::DescriptorSetLayout::null(),
            temporal_layout: vk::PipelineLayout::null(),
            temporal: vk::Pipeline::null(),
            temporal_module: vk::ShaderModule::null(),
            temporal_glossy: vk::Pipeline::null(),
            temporal_glossy_module: vk::ShaderModule::null(),
            atrous_set_layout: vk::DescriptorSetLayout::null(),
            atrous_layout: vk::PipelineLayout::null(),
            atrous: vk::Pipeline::null(),
            atrous_module: vk::ShaderModule::null(),
            atrous_glossy: vk::Pipeline::null(),
            atrous_glossy_module: vk::ShaderModule::null(),
            resolve_set_layout: vk::DescriptorSetLayout::null(),
            resolve_layout: vk::PipelineLayout::null(),
            resolve: vk::Pipeline::null(),
            resolve_module: vk::ShaderModule::null(),
            depth_pass: vk::Pipeline::null(),
            depth_module: vk::ShaderModule::null(),
            aa_set_layout: vk::DescriptorSetLayout::null(),
            aa_layout: vk::PipelineLayout::null(),
            fxaa: vk::Pipeline::null(),
            fxaa_module: vk::ShaderModule::null(),
            taa: vk::Pipeline::null(),
            taa_module: vk::ShaderModule::null(),
            mist_set_layout: vk::DescriptorSetLayout::null(),
            mist_layout: vk::PipelineLayout::null(),
            mist: vk::Pipeline::null(),
            mist_module: vk::ShaderModule::null(),
            gather_set_layout: vk::DescriptorSetLayout::null(),
            gather_layout: vk::PipelineLayout::null(),
            gather: vk::Pipeline::null(),
            gather_module: vk::ShaderModule::null(),
            gain_set_layout: vk::DescriptorSetLayout::null(),
            gain_layout: vk::PipelineLayout::null(),
            cache_set_layout: vk::DescriptorSetLayout::null(),
            cache_layout: vk::PipelineLayout::null(),
            cache_resolve: vk::Pipeline::null(),
            cache_module: vk::ShaderModule::null(),
            radiance_cache: None,
            cache_world: None,
            blue_noise: None,
            gain: vk::Pipeline::null(),
            gain_module: vk::ShaderModule::null(),
            pool: [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT],
            vertices: Default::default(),
            indices: Default::default(),
            triangles: Default::default(),
            frame: Default::default(),
            instances: Default::default(),
            scratch: None,
            blends: Default::default(),
            grid: Default::default(),
            model_lights: Default::default(),
            splashes: Default::default(),
            sources: Default::default(),
            spray: Default::default(),
            trail_puffs: Default::default(),
            emitters: Default::default(),
            emissive_faces: Default::default(),
            emissive_groups: Default::default(),
            triangle_emitters: Default::default(),
            blas: None,
            water_blas: None,
            sky_blas: None,
            cutout_blas: None,
            glow_blas: None,
            outside_blas: None,
            outside_cutout_blas: None,
            tlas: None,
            ground: None,
            ground_tiles: Vec::new(),
            detail: None,
            detail_blas: None,
            patch_job: None,
            textures: HashMap::new(),
            samplers: [vk::Sampler::null(); 7],
            timestamps: [vk::QueryPool::null(); FRAMES_IN_FLIGHT],
            timestamp_period: 0.0,
            timing: TraceTiming::default(),
            albedo: None,
            direct: None,
            bounce: None,
            fog: None,
            depth: None,
            geometry: [None, None],
            history: [None, None],
            lag_record: [None, None],
            glossy_hit: None,
            glossy: [None, None],
            glossy_filtered: [None, None],
            glossy_track: [None, None],
            shadows: [None, None],
            sun: None,
            object_motion: None,
            path_surface: None,
            specular: None,
            filtered: [None, None],
            color: None,
            motion: None,
            display_depth: None,
            upscaler: None,
            upscaled: None,
            guides: [None, None, None],
            aa_history: [None, None],
            taa_extent: None,
            smaa: None,
            texture_means: HashMap::new(),
            texture_energy: HashMap::new(),
            eruptions: Default::default(),
            trails: Default::default(),
            thrust_glow: Default::default(),
            mist_cells: [None, None],
            mist_gathered: None,
            mist_ran: false,
            mist_under: false,
            heat: None,
            gain_range: None,
            thermal_ran: false,
            heat_memory: Default::default(),
            model_heat: HashMap::new(),
            object_heat: Default::default(),
            heat_tuning: None,
            last_frame: None,
            failed_upscalers: Vec::new(),
            jitter_index: 0,
            parity: 0,
            frame_index: 0,
            previous: None,
            scratch_alignment: 256,
        };
        // From here a failure leaves `tracer` holding what it made; the
        // caller destroys it.
        let made = tracer.build(core);
        match made {
            Ok(()) => Ok(tracer),
            Err(error) => {
                let mut none = None;
                tracer.destroy(core, &mut none);
                Err(error)
            }
        }
    }

    unsafe fn build(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        let device = &core.device;
        let set_layouts = [self.set_layout];
        self.layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts),
                None,
            )
            .map_err(call("tracer pipeline layout"))?;
        let words = ash::util::read_spv(&mut std::io::Cursor::new(TRACE))
            .expect("embedded SPIR-V is whole words");
        self.trace_module = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
            .map_err(call("tracer shader"))?;
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(self.trace_module)
            .name(c"main");
        self.pipeline = device
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(stage)
                    .layout(self.layout)],
                None,
            )
            .map(|pipelines| pipelines[0])
            .map_err(|(_, result)| call("tracer pipeline")(result))?;
        self.paths_module = shader_module(core, PATHS, "path pass shader")?;
        self.paths = compute_pipeline(core, self.layout, self.paths_module)?;
        self.emitters_module = shader_module(core, EMITTER_SAMPLING, "emitters pass shader")?;
        self.emitters_pass = compute_pipeline(core, self.layout, self.emitters_module)?;
        self.raster_light_vertex = shader_module(core, RASTER_LIGHT, "raster light shader")?;
        let sampler = |index: u32| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(index)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        };
        let composite_bindings = [
            sampler(0),
            sampler(1),
            sampler(2),
            // The modern sky's clouds along the view, and their distance.
            sampler(4),
            sampler(5),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::VERTEX),
            // The scene, where the raster's draws over it find their light.
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX),
        ];
        self.composite_set_layout = device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&composite_bindings),
                None,
            )
            .map_err(call("composite descriptor layout"))?;
        let composite_sets = [self.composite_set_layout];
        self.composite_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&composite_sets),
                None,
            )
            .map_err(call("composite pipeline layout"))?;
        let raster_fog_sets = [core.texture_layout, self.composite_set_layout];
        let raster_push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(PRIMITIVE_PUSH as u32)];
        self.raster_fog_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&raster_fog_sets)
                    .push_constant_ranges(&raster_push),
                None,
            )
            .map_err(call("raster fog pipeline layout"))?;
        let words = ash::util::read_spv(&mut std::io::Cursor::new(COMPOSITE))
            .expect("embedded SPIR-V is whole words");
        self.composite_module = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
            .map_err(call("composite shader"))?;
        self.composite = composite_pipeline(core, self.composite_layout, self.composite_module)?;
        // The denoiser's passes: the frame block, then their images.
        let compute = |index: u32, kind: vk::DescriptorType, count: u32| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(index)
                .descriptor_type(kind)
                .descriptor_count(count)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        };
        let temporal_bindings = [
            compute(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            compute(1, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(2, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(3, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(4, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(5, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(6, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(7, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The direct light: how much bounce light each pixel takes.
            compute(8, vk::DescriptorType::STORAGE_IMAGE, 1),
            // Each primary hit's own motion, and its face and material.
            compute(9, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(10, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The short history, the last frame's and this frame's.
            compute(11, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(12, vk::DescriptorType::STORAGE_IMAGE, 1),
            // Which lobe each traced bounce took, and the glossy lobe's
            // history, the last frame's and this frame's.
            compute(13, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(14, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(15, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The filter's second glossy layer, where it leaves each glossy
            // lobe's footprint.
            compute(16, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The glossy history's short history and how far its light
            // lies, the last frame's and this frame's.
            compute(17, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(18, vk::DescriptorType::STORAGE_IMAGE, 1),
        ];
        let atrous_bindings = [
            compute(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            compute(1, vk::DescriptorType::STORAGE_IMAGE, 3),
            compute(2, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(3, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The glossy lobe's layers, and the primary hits' materials.
            compute(4, vk::DescriptorType::STORAGE_IMAGE, 3),
            compute(5, vk::DescriptorType::STORAGE_IMAGE, 1),
        ];
        let resolve_bindings = [
            compute(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            compute(1, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(2, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(3, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(4, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(5, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(6, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(7, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(8, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(9, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(10, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(11, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(12, vk::DescriptorType::STORAGE_BUFFER, 1),
            compute(13, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(14, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The primary hits' specular light from the sun.
            compute(15, vk::DescriptorType::STORAGE_IMAGE, 1),
            // The primary hits' faces and materials, for Ray
            // Reconstruction's guides.
            compute(16, vk::DescriptorType::STORAGE_IMAGE, 1),
        ];
        // The colour, its motion, the last frame's anti-aliased colour and
        // this frame's (FXAA reads the first and writes the last).
        let aa_bindings = [
            compute(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            compute(1, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(2, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(3, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(4, vk::DescriptorType::STORAGE_IMAGE, 1),
        ];
        // The mist's light: the scene for the sun's shadow, the ground's
        // heights and retail's lights, the last frame's cells and this one's.
        let mist_bindings = [
            compute(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            compute(1, vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, 1),
            compute(2, vk::DescriptorType::STORAGE_BUFFER, 1),
            compute(3, vk::DescriptorType::STORAGE_BUFFER, 1),
            compute(4, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, 1),
            compute(5, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(6, vk::DescriptorType::STORAGE_BUFFER, 1),
        ];
        let gather_bindings = [
            compute(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            compute(1, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(2, vk::DescriptorType::STORAGE_IMAGE, 1),
        ];
        // Thermal vision's gain: the heat, and the range it keeps.
        let gain_bindings = [
            compute(0, vk::DescriptorType::UNIFORM_BUFFER, 1),
            compute(1, vk::DescriptorType::STORAGE_IMAGE, 1),
            compute(2, vk::DescriptorType::STORAGE_BUFFER, 1),
            compute(3, vk::DescriptorType::STORAGE_IMAGE, 1),
        ];
        // The radiance cache's resolve: the cache alone.
        let cache_bindings = [compute(0, vk::DescriptorType::STORAGE_BUFFER, 1)];
        for (bindings, set_layout) in [
            (&temporal_bindings[..], &mut self.temporal_set_layout),
            (&atrous_bindings[..], &mut self.atrous_set_layout),
            (&resolve_bindings[..], &mut self.resolve_set_layout),
            (&aa_bindings[..], &mut self.aa_set_layout),
            (&mist_bindings[..], &mut self.mist_set_layout),
            (&gather_bindings[..], &mut self.gather_set_layout),
            (&gain_bindings[..], &mut self.gain_set_layout),
            (&cache_bindings[..], &mut self.cache_set_layout),
        ] {
            *set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings),
                    None,
                )
                .map_err(call("denoiser descriptor layout"))?;
        }
        let temporal_sets = [self.temporal_set_layout];
        self.temporal_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&temporal_sets),
                None,
            )
            .map_err(call("temporal pipeline layout"))?;
        let atrous_sets = [self.atrous_set_layout];
        let atrous_push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(std::mem::size_of::<[i32; 4]>() as u32)];
        self.atrous_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&atrous_sets)
                    .push_constant_ranges(&atrous_push),
                None,
            )
            .map_err(call("filter pipeline layout"))?;
        self.temporal_module = shader_module(core, TEMPORAL, "temporal shader")?;
        self.temporal = compute_pipeline(core, self.temporal_layout, self.temporal_module)?;
        self.atrous_module = shader_module(core, ATROUS, "filter shader")?;
        self.atrous = compute_pipeline(core, self.atrous_layout, self.atrous_module)?;
        self.temporal_glossy_module =
            shader_module(core, TEMPORAL_GLOSSY, "glossy temporal shader")?;
        self.temporal_glossy =
            compute_pipeline(core, self.temporal_layout, self.temporal_glossy_module)?;
        self.atrous_glossy_module = shader_module(core, ATROUS_GLOSSY, "glossy filter shader")?;
        self.atrous_glossy = compute_pipeline(core, self.atrous_layout, self.atrous_glossy_module)?;
        for (set_layout, layout, what) in [
            (
                self.resolve_set_layout,
                &mut self.resolve_layout,
                "resolve pipeline layout",
            ),
            (
                self.gather_set_layout,
                &mut self.gather_layout,
                "mist gathering pipeline layout",
            ),
        ] {
            let sets = [set_layout];
            *layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&sets),
                    None,
                )
                .map_err(call(what))?;
        }
        self.resolve_module = shader_module(core, RESOLVE, "resolve shader")?;
        self.resolve = compute_pipeline(core, self.resolve_layout, self.resolve_module)?;
        self.depth_module = shader_module(core, DEPTH, "depth shader")?;
        // The depth pass reads the trace's scene and sprites, for masked
        // sprites' holes.
        self.depth_pass = compute_pipeline(core, self.layout, self.depth_module)?;
        let aa_sets = [self.aa_set_layout];
        let aa_push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(4)];
        self.aa_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&aa_sets)
                    .push_constant_ranges(&aa_push),
                None,
            )
            .map_err(call("anti-aliasing pipeline layout"))?;
        self.fxaa_module = shader_module(core, FXAA, "FXAA shader")?;
        self.fxaa = compute_pipeline(core, self.aa_layout, self.fxaa_module)?;
        self.taa_module = shader_module(core, TAA, "TAA shader")?;
        self.taa = compute_pipeline(core, self.aa_layout, self.taa_module)?;
        // The mist's pass takes whether the last frame's cells can be
        // followed, as TAA does.
        let mist_sets = [self.mist_set_layout];
        self.mist_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&mist_sets)
                    .push_constant_ranges(&aa_push),
                None,
            )
            .map_err(call("mist pipeline layout"))?;
        self.mist_module = shader_module(core, FOG_INJECT, "mist shader")?;
        self.mist = compute_pipeline(core, self.mist_layout, self.mist_module)?;
        self.gather_module = shader_module(core, FOG_INTEGRATE, "mist gathering shader")?;
        self.gather = compute_pipeline(core, self.gather_layout, self.gather_module)?;
        let gain_sets = [self.gain_set_layout];
        // Whether the range starts afresh, and the shares of the scene it
        // leaves below black and above white.
        let gain_push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(12)];
        self.gain_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&gain_sets)
                    .push_constant_ranges(&gain_push),
                None,
            )
            .map_err(call("thermal gain pipeline layout"))?;
        self.gain_module = shader_module(core, THERMAL_GAIN, "thermal gain shader")?;
        self.gain = compute_pipeline(core, self.gain_layout, self.gain_module)?;
        let cache_sets = [self.cache_set_layout];
        self.cache_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&cache_sets),
                None,
            )
            .map_err(call("radiance cache pipeline layout"))?;
        self.cache_module = shader_module(core, CACHE, "radiance cache shader")?;
        self.cache_resolve = compute_pipeline(core, self.cache_layout, self.cache_module)?;
        // A frame's sets: the trace (which the display's depth shares), the
        // temporal pass, the filter, the resolve, the anti-aliasing, the
        // mist's two passes, thermal vision's gain and the composite.
        let sizes = [
            (vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, 3),
            (vk::DescriptorType::STORAGE_BUFFER, 27),
            (vk::DescriptorType::UNIFORM_BUFFER, 9),
            (
                vk::DescriptorType::STORAGE_IMAGE,
                13 + 18 + 9 + 15 + 4 + 1 + 2 + 2,
            ),
            (
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                TEXTURE_SLOTS + 8,
            ),
        ]
        .map(|(ty, descriptor_count)| vk::DescriptorPoolSize {
            ty,
            descriptor_count,
        });
        for pool in &mut self.pool {
            *pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(10)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(call("tracer descriptor pool"))?;
        }
        let limits = core
            .instance
            .get_physical_device_properties(core.physical)
            .limits;
        // The device enables anisotropy wherever it is supported.
        let anisotropy = core
            .instance
            .get_physical_device_features(core.physical)
            .sampler_anisotropy
            == vk::TRUE;
        for (level, sampler) in self.samplers.iter_mut().enumerate() {
            let level = level as u32;
            let filter = if level == 0 {
                vk::Filter::NEAREST
            } else {
                vk::Filter::LINEAR
            };
            let ratio = crate::frontend_video::filtering_anisotropy(level) as f32;
            let anisotropic = anisotropy && ratio > 1.0;
            *sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(filter)
                        .min_filter(filter)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .anisotropy_enable(anisotropic)
                        .max_anisotropy(if anisotropic {
                            ratio.min(limits.max_sampler_anisotropy)
                        } else {
                            1.0
                        })
                        // Point and Bilinear read the top level only.
                        .max_lod(if level < 2 { 0.0 } else { vk::LOD_CLAMP_NONE }),
                    None,
                )
                .map_err(call("tracer sampler"))?;
        }
        if limits.timestamp_compute_and_graphics == vk::TRUE {
            self.timestamp_period = f64::from(limits.timestamp_period);
            for timestamps in &mut self.timestamps {
                *timestamps = device
                    .create_query_pool(
                        &vk::QueryPoolCreateInfo::default()
                            .query_type(vk::QueryType::TIMESTAMP)
                            .query_count(TIMESTAMPS),
                        None,
                    )
                    .map_err(call("tracer timestamps"))?;
            }
        }
        let mut acceleration = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
        let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut acceleration);
        core.instance
            .get_physical_device_properties2(core.physical, &mut properties);
        self.scratch_alignment = u64::from(
            acceleration
                .min_acceleration_structure_scratch_offset_alignment
                .max(1),
        );
        Ok(())
    }

    /// The layout and set the raster's textured draws over this frame's
    /// traced world take its fog through (`vk_textured_fog.frag`).
    pub(super) fn raster_fog(&self) -> (vk::PipelineLayout, vk::DescriptorSet) {
        (self.raster_fog_layout, self.composite_frame_set)
    }

    /// The vertex shader of the raster's draws over the traced world, which
    /// light their half-transparent faces (`vk_primitive_lit.vert`).
    pub(super) fn raster_light_vertex(&self) -> vk::ShaderModule {
        self.raster_light_vertex
    }

    /// Release everything, freeing memory through `allocator` when given
    /// (the device's teardown passes it; a failed construction has none).
    pub(super) unsafe fn destroy(mut self, core: &Core, allocator: &mut Option<Allocator>) {
        self.report_timing(core);
        if let Some(upscaler) = self.upscaler.take() {
            upscaler.destroy(core);
        }
        if let Some(smaa) = self.smaa.take() {
            smaa.destroy(core, allocator);
        }
        let device = &core.device;
        for (_, texture) in self.textures.drain() {
            let image = texture.image;
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            if let (Some(allocation), Some(allocator)) = (image.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        }
        for sampler in self.samplers {
            device.destroy_sampler(sampler, None);
        }
        for timestamps in self.timestamps {
            device.destroy_query_pool(timestamps, None);
        }
        let free_buffer = |buffer: Buffer, allocator: &mut Option<Allocator>| {
            device.destroy_buffer(buffer.buffer, None);
            if let (Some(allocation), Some(allocator)) = (buffer.allocation, allocator.as_mut()) {
                let _ = allocator.free(allocation);
            }
        };
        if let Some(loader) = core.acceleration_loader.as_ref() {
            for acceleration in [
                self.blas.take(),
                self.water_blas.take(),
                self.sky_blas.take(),
                self.cutout_blas.take(),
                self.glow_blas.take(),
                self.outside_blas.take(),
                self.outside_cutout_blas.take(),
                self.tlas.take(),
                self.detail_blas.take(),
            ]
            .into_iter()
            .chain(std::mem::take(&mut self.ground_tiles))
            .flatten()
            {
                loader.destroy_acceleration_structure(acceleration.handle, None);
                free_buffer(acceleration.buffer, allocator);
            }
        }
        let per_frame = [
            &mut self.vertices,
            &mut self.indices,
            &mut self.triangles,
            &mut self.frame,
            &mut self.instances,
            &mut self.blends,
            &mut self.grid,
            &mut self.model_lights,
            &mut self.splashes,
            &mut self.sources,
            &mut self.spray,
            &mut self.trail_puffs,
            &mut self.emitters,
            &mut self.emissive_faces,
            &mut self.emissive_groups,
            &mut self.triangle_emitters,
        ]
        .into_iter()
        .flat_map(|slots| slots.iter_mut().map(Option::take).collect::<Vec<_>>())
        .collect::<Vec<_>>();
        for grown in [
            self.scratch.take(),
            self.gain_range.take(),
            self.radiance_cache.take(),
        ]
        .into_iter()
        .chain(per_frame)
        .flatten()
        {
            free_buffer(grown.buffer, allocator);
        }
        if let Some(ground) = self.ground.take() {
            for buffer in [ground.vertices, ground.indices, ground.triangles] {
                free_buffer(buffer, allocator);
            }
        }
        if let Some(buffer) = self.blue_noise.take() {
            free_buffer(buffer, allocator);
        }
        if let Some(detail) = self.detail.take() {
            for buffer in [detail.vertices, detail.indices, detail.triangles] {
                free_buffer(buffer, allocator);
            }
        }
        let [geometry, history, filtered, shadows, lag_record, glossy, glossy_filtered, glossy_track] = [
            std::mem::take(&mut self.geometry),
            std::mem::take(&mut self.history),
            std::mem::take(&mut self.filtered),
            std::mem::take(&mut self.shadows),
            std::mem::take(&mut self.lag_record),
            std::mem::take(&mut self.glossy),
            std::mem::take(&mut self.glossy_filtered),
            std::mem::take(&mut self.glossy_track),
        ];
        for image in [
            self.albedo.take(),
            self.direct.take(),
            self.bounce.take(),
            self.fog.take(),
            self.depth.take(),
            self.color.take(),
            self.motion.take(),
            self.display_depth.take(),
            self.upscaled.take(),
            self.guides[0].take(),
            self.guides[1].take(),
            self.guides[2].take(),
            self.aa_history[0].take(),
            self.aa_history[1].take(),
            self.mist_cells[0].take(),
            self.mist_cells[1].take(),
            self.mist_gathered.take(),
            self.heat.take(),
            self.sun.take(),
            self.object_motion.take(),
            self.path_surface.take(),
            self.specular.take(),
            self.glossy_hit.take(),
        ]
        .into_iter()
        .chain(geometry)
        .chain(history)
        .chain(filtered)
        .chain(shadows)
        .chain(lag_record)
        .chain(glossy)
        .chain(glossy_filtered)
        .chain(glossy_track)
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
        for pipeline in [
            self.pipeline,
            self.paths,
            self.emitters_pass,
            self.cache_resolve,
            self.composite,
            self.temporal,
            self.atrous,
            self.temporal_glossy,
            self.atrous_glossy,
            self.resolve,
            self.depth_pass,
            self.fxaa,
            self.taa,
            self.mist,
            self.gather,
            self.gain,
        ] {
            device.destroy_pipeline(pipeline, None);
        }
        for layout in [
            self.layout,
            self.composite_layout,
            self.raster_fog_layout,
            self.temporal_layout,
            self.atrous_layout,
            self.resolve_layout,
            self.aa_layout,
            self.mist_layout,
            self.gather_layout,
            self.gain_layout,
            self.cache_layout,
        ] {
            device.destroy_pipeline_layout(layout, None);
        }
        for set_layout in [
            self.set_layout,
            self.composite_set_layout,
            self.temporal_set_layout,
            self.atrous_set_layout,
            self.resolve_set_layout,
            self.aa_set_layout,
            self.mist_set_layout,
            self.gather_set_layout,
            self.gain_set_layout,
            self.cache_set_layout,
        ] {
            device.destroy_descriptor_set_layout(set_layout, None);
        }
        for module in [
            self.trace_module,
            self.paths_module,
            self.emitters_module,
            self.cache_module,
            self.raster_light_vertex,
            self.composite_module,
            self.temporal_module,
            self.atrous_module,
            self.temporal_glossy_module,
            self.atrous_glossy_module,
            self.resolve_module,
            self.depth_module,
            self.fxaa_module,
            self.taa_module,
            self.mist_module,
            self.gather_module,
            self.gain_module,
        ] {
            device.destroy_shader_module(module, None);
        }
    }

    /// `upscaler` is out for the rest of the run: say why, once.
    fn upscaler_unavailable(&mut self, upscaler: crate::modern_settings::Upscaler, why: &str) {
        if !self.failed_upscalers.contains(&upscaler) {
            self.failed_upscalers.push(upscaler);
            eprintln!(
                "Infestation: the {upscaler:?} upscaler is unavailable, the tracer scales by itself: {why}"
            );
        }
    }

    /// Read the last traced frame's timestamps, when they wait (its
    /// submission has been waited for by now).
    unsafe fn read_timestamps(&mut self, core: &Core, slot: usize) {
        if !std::mem::take(&mut self.timing.pending[slot]) {
            return;
        }
        let mut ticks = [0_u64; TIMESTAMPS as usize];
        if core
            .device
            .get_query_pool_results(
                self.timestamps[slot],
                0,
                &mut ticks,
                vk::QueryResultFlags::TYPE_64,
            )
            .is_err()
        {
            return;
        }
        self.timing.read += 1;
        if self.timing.read <= super::perf::WARMUP_FRAMES {
            return;
        }
        let period = self.timestamp_period;
        let span = |from: usize| ticks[from + 1].wrapping_sub(ticks[from]) as f64 * period;
        let parts: [f64; STAGES.len()] = std::array::from_fn(span);
        let timing = &mut self.timing;
        timing.frames += 1;
        for (stage, part) in parts.into_iter().enumerate() {
            timing.sums[stage] += part;
            timing.most[stage] = timing.most[stage].max(part);
        }
        timing.worst = timing.worst.max(parts.iter().sum());
    }

    /// One line on the tracer's GPU time, once it has traced.
    unsafe fn report_timing(&mut self, core: &Core) {
        for slot in 0..FRAMES_IN_FLIGHT {
            self.read_timestamps(core, slot);
        }
        let timing = &self.timing;
        if timing.frames == 0 {
            return;
        }
        let mean = |nanoseconds: f64| nanoseconds / timing.frames as f64 / 1e6;
        let builds = timing.sums[..2].iter().sum::<f64>();
        let trace = timing.sums[2..9].iter().sum::<f64>();
        let composite = timing.sums[9];
        eprintln!(
            "Vulkan tracer: {} frames at {}x{}, GPU {:.2} ms a frame (uploads and builds {:.2}, \
             trace {:.2}, composite {:.2}), worst {:.2} ms",
            timing.frames,
            timing.extent[0],
            timing.extent[1],
            mean(builds + trace + composite),
            mean(builds),
            mean(trace),
            mean(composite),
            timing.worst / 1e6,
        );
        let stages: Vec<String> = STAGES
            .iter()
            .zip(timing.sums.iter().zip(&timing.most))
            .map(|(name, (sum, most))| format!("{name} {:.2}/{:.2}", mean(*sum), most / 1e6))
            .collect();
        eprintln!("Vulkan tracer stages, mean/most ms: {}", stages.join(", "));
    }
}

pub(super) unsafe fn shader_module(
    core: &Core,
    code: &[u8],
    what: &'static str,
) -> Result<vk::ShaderModule, VkDeviceError> {
    let words = ash::util::read_spv(&mut std::io::Cursor::new(code))
        .expect("embedded SPIR-V is whole words");
    core.device
        .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
        .map_err(call(what))
}

pub(super) unsafe fn compute_pipeline(
    core: &Core,
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
) -> Result<vk::Pipeline, VkDeviceError> {
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(c"main");
    core.device
        .create_compute_pipelines(
            vk::PipelineCache::null(),
            &[vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(layout)],
            None,
        )
        .map(|pipelines| pipelines[0])
        .map_err(|(_, result)| call("denoiser pipeline")(result))
}

/// Storage writes from one compute pass reach the next pass and the
/// composite's sampling.
pub(super) unsafe fn pass_barrier(core: &Core, command: vk::CommandBuffer) {
    let barrier = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
        .dst_stage_mask(
            vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::FRAGMENT_SHADER,
        )
        .dst_access_mask(
            vk::AccessFlags2::SHADER_STORAGE_READ
                | vk::AccessFlags2::SHADER_STORAGE_WRITE
                | vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
    core.device.cmd_pipeline_barrier2(
        command,
        &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&barrier)),
    );
}

/// Every earlier command (the frames in flight before this one) before any
/// later one: this frame rewrites what they read (the bottom and top
/// levels, the images, the scratch), and the device no longer waits for
/// them before recording it.
pub(super) unsafe fn frame_barrier(core: &Core, command: vk::CommandBuffer) {
    let barrier = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE);
    core.device.cmd_pipeline_barrier2(
        command,
        &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&barrier)),
    );
}

unsafe fn composite_pipeline(
    core: &Core,
    layout: vk::PipelineLayout,
    fragment: vk::ShaderModule,
) -> Result<vk::Pipeline, VkDeviceError> {
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(core.modules.blit_vertex)
            .name(c"main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment)
            .name(c"main"),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    // Depth writes need the test on; ALWAYS keeps every traced depth.
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::ALWAYS);
    // The traced world comes opaque; the mist over the sky lets the sky
    // through by what still shows.
    let attachments = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic);
    let formats = [COLOR_FORMAT];
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
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    core.device
        .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
        .map(|pipelines| pipelines[0])
        .map_err(|(_, result)| call("composite pipeline")(result))
}

/// The size the tracer renders at for a `display` of that size:
/// `INF_RENDER_SCALE` (0.25 to 1, read once) of it, or all of it.
fn render_extent(display: [u32; 2]) -> [u32; 2] {
    static SCALE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    let scale = *SCALE.get_or_init(|| {
        std::env::var("INF_RENDER_SCALE")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .map_or(1.0, |scale| scale.clamp(0.25, 1.0))
    });
    display.map(|side| ((side as f32 * scale).round() as u32).clamp(1, side.max(1)))
}

/// How much light a crystal's sprites hold ([`sprite_energy`]): a glow's
/// strength is tuned at it, and sprites holding more or less shine so much
/// more or less (the tokens' core pulses).
const GLOW_ENERGY: f32 = 0.5;

/// How much light `texture` holds: its texels' brightest channel times
/// their alpha, on average, 0 to 1.
fn sprite_energy(texture: &CpuTexture) -> f32 {
    let [width, height] = texture.dimensions.map(u32::from);
    let mut sum = 0.0_f64;
    let mut count = 0_u32;
    for y in 1..=height {
        for x in 1..=width {
            if let Some([r, g, b, a]) = texture.texel(x, y) {
                sum += f64::from(r.max(g).max(b)) * f64::from(a) / (255.0 * 255.0);
            }
            count += 1;
        }
    }
    (sum / f64::from(count.max(1))) as f32
}

/// What the frame block takes beside the feed: the view's sizes and jitter,
/// the frame's index and last view, and the geometry the trace uploaded
/// (its counts and offsets; none for the raster's own passes).
pub(super) struct FrameShape<'a> {
    pub display: [u32; 2],
    pub extent: [u32; 2],
    pub upscaled: bool,
    pub ray_reconstruction: bool,
    pub jitter: [f32; 2],
    pub view: PreviousView,
    pub previous: Option<PreviousView>,
    pub frame_index: u32,
    pub water_count: u32,
    pub sky_count: u32,
    pub cutout_first: u32,
    pub glow_first: u32,
    pub source_count: usize,
    pub splash_count: usize,
    pub spray_sources: usize,
    pub emitter_count: usize,
    /// The emissive groups the path pass samples (emissive.rs).
    pub emissive_groups: usize,
    pub effect_mirrors: f32,
    /// The water's texture slot (-1: none) and its mean colour.
    pub water_texture: f32,
    pub water_hue: [f32; 3],
    pub grid: Option<&'a crate::scene_feed::SceneLightGrid>,
    /// A world's view, which the modern sky's clouds hang over (not the
    /// briefing's pane): the tracer's has its ground, the raster's sky is
    /// always one.
    pub world_view: bool,
}

/// The frame block (`pt_frame.glsl`), and what its writing decided for the
/// frame's passes: its view (with the mist's far end), the distance fog,
/// whether the mist runs and its grid, and the modern sky and its clouds.
pub(super) struct FrameBlock {
    pub block: [f32; FRAME_FLOATS],
    pub view: PreviousView,
    pub previous: Option<PreviousView>,
    pub distance_fog: bool,
    pub mist: bool,
    pub volume: [u32; 3],
    pub modern_sky: bool,
    pub clouded: bool,
    /// Rain, snow or a struck crystal's arcs to lay over the frame (the
    /// raster sky's over pass, skipped without them).
    pub over: bool,
}

/// Write the frame block for `feed` as `shape` has it, under the player's
/// `modern` settings and the built-in tuning: the tracer's, and the
/// raster's own passes'.
pub(super) fn frame_block(
    feed: &SceneFeed,
    shape: &FrameShape<'_>,
    modern: crate::modern_settings::ModernSettings,
) -> FrameBlock {
    let tuning = crate::modern_tuning::current();
    let lighting = tuning.lighting(modern.lighting);
    let thermal = tuning.thermal();
    let spray_look = tuning.water(feed.world);
    let camera = &feed.camera;
    // The fog as tuned for this world. A world's view takes the
    // distance fog, which the mist's grid carries, its slices ending
    // just past the fog's end F; the briefing's pane (whose circle is
    // retail's fog) and thermal vision (which has no mist) keep the haze,
    // complete under thermal vision at F, else where retail's far terrain
    // ends. No mist under infrared vision or without ground.
    let fog = tuning.fog_at(feed.world, feed.darkened);
    let fog_end = feed.fog.end.max(camera.near * 2.0);
    let terrain = feed.terrain.as_deref();
    let distance_fog = feed.fog.far_ring && !feed.infrared && terrain.is_some();
    let haze_reach = if feed.infrared {
        fog_end + 1.0
    } else if feed.fog.far_ring {
        fog_end * fog.reach.max(1.0)
    } else {
        fog_end
    };
    // With the eye under the water the grid holds the water's light
    // instead (the remastered camera), out to where nothing seen through
    // it shows: e^-6, six times the water's 1/e distance.
    let volume_far = if feed.eye_under {
        6.0 * crate::world_scene::MURK_REACH
    } else if distance_fog {
        fog_end * 1.05
    } else {
        haze_reach
    };
    let mist_density = match terrain {
        Some(_) if !feed.infrared => fog.density.max(0.0) * 1000.0 / fog_end,
        _ => 0.0,
    };
    // The drawn wind's drift (item 12): its density now, swelling with
    // the gusts the wind's sound follows, its height, the wind's
    // velocity and its colour; none under infrared or without ground.
    let drift_words: [f32; 7] = match (tuning.atmosphere().weather.as_ref(), terrain) {
        (Some(weather), Some(_)) if !feed.infrared => {
            match (
                weather.drift.get(&feed.world),
                weather.winds.get(&feed.world),
            ) {
                (Some(drift), Some(&[strength, gusty, bearing])) => {
                    let seconds = feed.clock_micros as f32 / 1.0e6;
                    let gust = crate::ambience::gust_swell(feed.world, seconds);
                    let blow = strength * (1.0 - gusty + gusty * gust);
                    let speed = 120.0 + 480.0 * blow;
                    let (sin, cos) = bearing.to_radians().sin_cos();
                    let [r, g, b] = drift.color.map(|channel| channel.clamp(0.0, 2.0));
                    [
                        drift.density.max(0.0) * blow,
                        drift.height.max(1.0),
                        sin * speed,
                        cos * speed,
                        r,
                        g,
                        b,
                    ]
                }
                _ => [0.0; 7],
            }
        }
        _ => [0.0; 7],
    };
    let mist_rows =
        (MIST_ACROSS * shape.display[1] / shape.display[0].max(1)).clamp(1, MIST_ACROSS);
    let volume = [MIST_ACROSS, mist_rows, MIST_SLICES];
    let view = PreviousView {
        volume_far,
        ..shape.view
    };
    let previous = shape.previous;
    let mut block = [0.0_f32; FRAME_FLOATS];
    let mut at = 0;
    let mut put = |values: &[f32]| {
        block[at..at + values.len()].copy_from_slice(values);
        at += values.len();
    };
    put(&camera.basis[0]);
    put(&camera.basis[1]);
    put(&camera.basis[2]);
    put(&camera.focal);
    put(&camera.centre);
    put(&[camera.native[0] as f32, camera.native[1] as f32]);
    put(&[shape.extent[0] as f32, shape.extent[1] as f32]);
    put(&[camera.near, camera.far]);
    put(&feed.sun.direction);
    put(&feed.sun.color);
    put(&feed.sun.ambient);
    // Under a modern sky the fog's colour falls with the world's light
    // as the sky does (Artoria's dead generators, Pelagia's night), so
    // the haze darkens with what it veils (Stage 2's E4).
    let fog_follow: [f32; 3] = match feed.light_base {
        Some(base)
            if tuning.sky(feed.world).style == crate::modern_tuning::SkyStyle::Modern
                && !feed.infrared =>
        {
            std::array::from_fn(|channel| {
                (feed.sun.color[channel] / base[channel].max(1.0e-3)).clamp(0.08, 1.0)
            })
        }
        _ => [1.0; 3],
    };
    let fog_followed = |rgb: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|channel| rgb[channel] * fog_follow[channel])
    };
    put(&fog_followed(feed.fog.color));
    put(&[feed.fog.start, feed.fog.end]);
    put(&feed.sky);
    put(&view.eye);
    put(&shape
        .grid
        .map_or([0.0; 2], |grid| grid.anchors.map(f32::from)));
    put(&[shape.grid.map_or(128.0, |grid| f32::from(grid.spacing))]);
    put(&shape
        .grid
        .map_or([0.0; 3], |grid| grid.outside.map(f32::from)));
    // 0: no grid, 1: models only, 2: models and ground.
    put(&[shape
        .grid
        .map_or(0.0, |grid| if grid.ground { 2.0 } else { 1.0 })]);
    // Kept below 2^24 so the float stays exact.
    put(&[(shape.frame_index & 0x00ff_ffff) as f32]);
    put(&[if previous.is_some() { 1.0 } else { 0.0 }]);
    let then = previous.unwrap_or(view);
    put(&then.basis[0]);
    put(&then.basis[1]);
    put(&then.basis[2]);
    put(&then.focal);
    put(&then.centre);
    put(&then.eye);
    put(&[feed.water_clock as f32]);
    put(&[(shape.water_count + shape.sky_count) as f32]);
    put(&shape.jitter);
    put(&[shape.display[0] as f32, shape.display[1] as f32]);
    put(&[if shape.upscaled {
        0.5 * shape.extent[0] as f32 / shape.display[0] as f32
    } else {
        1.0
    }]);
    put(&[if shape.ray_reconstruction { 1.0 } else { 0.0 }]);
    put(&fog_followed(feed.fog.far_color));
    put(&[haze_reach, fog.haze.max(0.0)]);
    put(&[
        mist_density,
        fog.height,
        terrain.map_or(0.0, |terrain| terrain.low) + fog.base,
        fog.floor.clamp(0.0, 1.0),
        fog.anisotropy.clamp(-0.95, 0.95),
        (fog.sun * lighting.shafts.max(0.0)).clamp(0.0, 1.0),
        fog.glow.max(0.0) * lighting.glows.max(0.0),
        fog.sky.clamp(0.0, 1.0),
    ]);
    put(&volume.map(|cells| cells as f32));
    put(&[volume_far, then.volume_far.max(camera.near * 2.0)]);
    let mist_index = shape.frame_index % 64 + 1;
    put(&[
        halton(mist_index, 2),
        halton(mist_index, 3),
        halton(mist_index, 5),
    ]);
    // The ground's grid: the tracer's ground, or the raster's wet ground's.
    let grid = terrain.or(feed.raster_wet_ground().map(|grid| &**grid));
    put(&grid.map_or([0.0; 2], |grid| [grid.points as f32, grid.spacing]));
    // Worlds without water hold its plane at -4096 or below.
    let water = grid.filter(|grid| grid.plane > -4096.0);
    put(&[
        water
            .map(|terrain| terrain.plane)
            .or(feed.water_level)
            .unwrap_or(-1.0e9),
        water.map_or(0.0, |_| feed.water_reach),
        shape.water_texture,
    ]);
    put(&[
        if feed.infrared { 1.0 } else { 0.0 },
        thermal.sky,
        thermal.water,
        thermal.ground,
        thermal.lamp,
        thermal.lights,
        thermal.glowing,
        thermal.rate.clamp(0.0, 1.0),
    ]);
    let water_look = tuning.water(feed.world);
    put(&[shape.source_count as f32]);
    put(&[shape.water_count as f32]);
    // The sky: retail's painted backdrop, or the modern one from the
    // world's own colours.
    let sky = tuning.sky(feed.world);
    let modern_sky = sky.style == crate::modern_tuning::SkyStyle::Modern && !feed.infrared;
    put(&[if modern_sky { 1.0 } else { 0.0 }]);
    // The modern sky follows the world's light as it falls from its
    // first (Artoria's generators, world 1's night), down to a floor
    // so dark is never black.
    let follow: [f32; 3] = match feed.light_base {
        Some(base) if modern_sky => std::array::from_fn(|channel| {
            (feed.sun.color[channel] / base[channel].max(1.0e-3)).clamp(0.08, 1.0)
        }),
        _ => [1.0; 3],
    };
    let followed = |rgb: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|channel| rgb[channel] * follow[channel])
    };
    // A raining or snowing world's weather: in its clear spells the
    // sky lightens a little and its clouds thin and brighten, nearer
    // retail's painted sky; its storm brings them back (world_weather).
    let storm = feed
        .weather
        .map_or(1.0, |weather| weather.fall.clamp(0.0, 1.0));
    let clear = 1.0 - storm;
    let brighter = |rgb: [f32; 3]| rgb.map(|channel| channel * (1.0 + 0.15 * clear));
    let zenith = brighter(followed(sky.zenith.unwrap_or(feed.sky)));
    put(&zenith);
    put(&brighter(followed(
        sky.horizon.unwrap_or(feed.fog.far_color),
    )));
    put(&[
        sky.curve.max(0.1),
        sky.sun.glow.max(0.0),
        sky.sun.disc.max(0.0),
        sky.stars.max(0.0),
        sky.horizon_glow.max(0.0),
    ]);
    put(&[
        shape.splash_count as f32,
        water_look.rings.max(0.0),
        water_look.foam.clamp(0.0, 1.0),
        water_look.spread.max(0.0),
        water_look.width.max(1.0),
    ]);
    put(&[shape.cutout_first as f32]);
    put(&[sky.sun.radius.clamp(0.0, 10.0).to_radians().cos()]);
    put(&[sky.sun.shadow.clamp(0.0, 10.0).to_radians().cos()]);
    put(&[shape.spray_sources as f32, spray_look.flash.max(0.0)]);
    put(&[
        spray_look.clarity.max(1.0),
        (spray_look.reflect * lighting.reflections.max(0.0)).clamp(0.0, 1.0),
        spray_look.glow.max(0.0),
    ]);
    // The fields of energy the active warpgates' arcs span, the nearest
    // first; none under thermal vision.
    let fields = if feed.infrared {
        Vec::new()
    } else {
        crate::scene_field::fields(&feed.arcs)
    };
    put(&[fields.len().min(FIELDS) as f32]);
    for slot in 0..FIELDS {
        match fields.get(slot) {
            Some(field) => {
                put(&field.centre);
                put(&field.across);
                put(&field.up);
                put(&[field.strength]);
                put(&field.color);
            }
            None => put(&[0.0; 13]),
        }
    }
    // The clouds, with their shadows, come only with the modern sky: the
    // painted one lights the ground as retail does (2026-10-23);
    // none under thermal vision. Without colours of their own they take
    // the world's light, near white, on their lit side and the zenith's
    // colour, darker, on their shaded one, as dim as the world's light:
    // dark shapes in a night sky.
    let clouds = sky.clouds;
    let clouded = modern_sky && clouds.cover > 0.0 && !feed.infrared && shape.world_view;
    let light = feed.sun.color;
    let peak = light.into_iter().fold(1.0e-3, f32::max);
    let daylight = ((0.3 * light[0] + 0.6 * light[1] + 0.1 * light[2]) * 2.0).clamp(0.0, 1.0);
    let dim = 0.25 + 0.75 * daylight;
    let lit = clouds.lit.map(followed).unwrap_or_else(|| {
        std::array::from_fn(|channel| (0.4 + 0.6 * light[channel] / peak) * 0.85 * dim)
    });
    let shade = clouds
        .shade
        .map(followed)
        .unwrap_or_else(|| std::array::from_fn(|channel| (zenith[channel] * 0.55 + 0.08) * dim));
    let shade: [f32; 3] = std::array::from_fn(|channel| {
        shade[channel] + (lit[channel] - shade[channel]) * 0.3 * clear
    });
    let cloud_shape = match clouds.shape {
        crate::modern_tuning::CloudShape::Cumulus => 0.0,
        crate::modern_tuning::CloudShape::Stratus => 1.0,
        crate::modern_tuning::CloudShape::Storm => 2.0,
        crate::modern_tuning::CloudShape::Ash => 3.0,
    };
    put(&[
        if clouded {
            (clouds.cover * (1.0 - 0.45 * clear)).clamp(0.0, 1.0)
        } else {
            0.0
        },
        cloud_shape,
        clouds.base,
        clouds.top.max(clouds.base + 100.0),
        clouds.size.max(256.0),
        clouds.speed,
    ]);
    put(&lit.map(|channel| channel.clamp(0.0, 2.0)));
    put(&shade.map(|channel| channel.clamp(0.0, 2.0)));
    put(&[clouds.shadow.clamp(0.0, 1.0)]);
    put(&[feed.shadow_lift.max(0.0)]);
    put(&[if feed.infrared {
        1.0
    } else {
        lighting.reflections.max(0.0)
    }]);
    // What falls: only from the modern sky.
    let precip = sky.precip;
    // A fall that waits for the night (Pelagia's rain) comes over the
    // painted sky too, as thick as night has fallen.
    let precip_share =
        crate::world_scene::precip_share(&precip, modern_sky, feed.nightfall, feed.night_weather);
    let precip_kind = match precip.kind {
        _ if precip_share <= 0.0 => 0.0,
        crate::modern_tuning::PrecipKind::None => 0.0,
        crate::modern_tuning::PrecipKind::Snow => 1.0,
        crate::modern_tuning::PrecipKind::Rain => 2.0,
        crate::modern_tuning::PrecipKind::Ash => 3.0,
        crate::modern_tuning::PrecipKind::Embers => 4.0,
    };
    let precip_density = precip.density.clamp(0.0, 1.0) * precip_share * storm;
    put(&[precip_kind, precip_density, precip.fall]);
    put(&precip.wind);
    put(&[precip.size.max(0.1), precip.length.max(0.1)]);
    put(&precip.color.map(|channel| channel.clamp(0.0, 2.0)));
    // The storm: its veins and lightning, only with the modern sky.
    let veins = clouds
        .veins
        .filter(|_| modern_sky && clouded)
        .unwrap_or([0.0; 3]);
    put(&veins.map(|channel| channel.clamp(0.0, 4.0)));
    let under = clouds
        .under
        .filter(|_| modern_sky && clouded)
        .unwrap_or([0.0; 3]);
    put(&followed(under).map(|channel| channel.clamp(0.0, 4.0)));
    put(&[clouds.streak.clamp(1.0, 8.0)]);
    put(&[clouds.haze.max(1000.0)]);
    put(&[clouds.albedo.clamp(0.0, 1.0)]);
    put(&[clouds.round.clamp(0.0, 1.0)]);
    let lightning = sky.lightning;
    put(&[if modern_sky {
        lightning.rate.clamp(0.0, 2.0) * storm
    } else {
        0.0
    }]);
    put(&lightning.color.map(|channel| channel.clamp(0.0, 4.0)));
    put(&[lightning.light.max(0.0)]);
    put(&[tuning.bounce().colour.clamp(0.0, 1.0)]);
    put(&[shape.effect_mirrors, shape.emitter_count as f32]);
    put(&[
        // The raster's world view (whose ground is retail's, no `terrain`)
        // takes the distance fog's far fade and its sky below the horizon
        // (vk_sky.comp) on the same worlds.
        if feed.fog.far_ring && !feed.infrared && shape.world_view {
            1.0
        } else {
            0.0
        },
        feed.fog.start * fog.start.max(0.0),
        -(1.0 - fog.opacity.clamp(0.0, 0.9999)).ln(),
        fog.curve.clamp(0.25, 4.0),
        // The fog's top follows an eye that climbs past it (the VTOL high
        // up), `lag` of its band below the eye, so the haze stays at the
        // eye's height, thinner, and the horizon stays veiled
        // (2026-10-26: from high enough one could still see past the fog),
        // without the wall it stood as up to the eye (2026-10-29: the top
        // part of the fog rose a bit too much); the sky below the horizon
        // takes the fog's colour in the composite.
        (terrain.map_or(0.0, |terrain| terrain.high) + fog.top)
            .max(view.eye[1] - fog.lag.clamp(0.0, 1.0) * fog.band.max(1.0)),
        fog.band.max(1.0),
        fog.above.clamp(0.0, 1.0),
        fog_end * fog.dissolve.clamp(0.0, 1.0),
    ]);
    // The shore and the spray's look (the plan's items 2 and 3).
    put(&[
        water_look.shore.max(0.0),
        water_look.wobble.max(0.0),
        water_look.lap.max(0.0),
        water_look.wet.max(0.0),
        water_look.damp.clamp(0.0, 1.0),
        water_look.see_through.clamp(0.0, 1.0),
        water_look.tint.clamp(0.0, 1.0),
        water_look.churn.max(0.0),
    ]);
    // The vehicles' lamps: how many, the
    // headlamps' beam and the rear lamps', what they share, then each.
    let lamp = tuning.headlight();
    let lamps = &feed.lamps[..feed.lamps.len().min(MAX_LAMPS)];
    put(&[lamps.len() as f32]);
    for look in [lamp.look(false), lamp.look(true)] {
        put(&[
            look.intensity,
            look.spread,
            look.inner,
            look.cutoff,
            look.soft,
            look.hot,
            look.near,
            look.reach,
            look.size,
        ]);
    }
    put(&[
        lamp.wrap.max(0.0),
        lamp.water_scatter.max(0.0),
        lamp.water_gloss.clamp(0.02, 1.0),
        lamp.air.max(0.0),
        lamp.air_floor.max(0.0),
    ]);
    // The water's own colour, which the beams glow in under the water.
    put(&shape.water_hue);
    put(&[lamp.water_glow.max(0.0)]);
    for index in 0..MAX_LAMPS {
        let placed = lamps.get(index).copied().unwrap_or_default();
        put(&placed.position);
        put(&placed.axis);
        put(&placed.across);
        put(&placed.up);
        put(&placed.color.map(|channel| channel.max(0.0)));
        put(&[f32::from(u8::from(placed.rear))]);
    }
    put(&[shape.glow_first as f32]);
    put(&[water_look.sunk.max(0.0)]);
    put(&drift_words);
    // The world in the modern sky (E6), none under the painted one.
    match sky.planet.filter(|_| modern_sky) {
        Some(planet) => {
            let [round, up] = planet.direction.map(f32::to_radians);
            let (sin_round, cos_round) = round.sin_cos();
            let (sin_up, cos_up) = up.sin_cos();
            put(&[sin_round * cos_up, sin_up, cos_round * cos_up]);
            put(&[planet.size.clamp(0.1, 80.0).to_radians().cos()]);
            put(&planet.color.map(|channel| channel.clamp(0.0, 4.0)));
            put(&[planet.lights.max(0.0)]);
        }
        None => put(&[0.0; 8]),
    }
    // The weather's wet: the ground's (rain's), the models' (a sheen on
    // metal and paint) and how far the puddles have filled. Under
    // thermal vision the models' none, the puddles kept: they read as
    // the water.
    let wet = feed.weather.map_or([0.0; 3], |weather| {
        [
            weather.wet_ground,
            if feed.infrared {
                0.0
            } else {
                weather.wet_models
            },
            weather.puddles,
        ]
    });
    put(&wet.map(|share| share.clamp(0.0, 1.0)));
    // The sparks running through struck crystals, and the craft
    // stirring the puddles (none under thermal vision).
    let sparks = if feed.infrared {
        &[][..]
    } else {
        &feed.sparks[..feed.sparks.len().min(MAX_SPARKS)]
    };
    put(&[sparks.len() as f32]);
    for slot in 0..MAX_SPARKS {
        match sparks.get(slot) {
            Some(spark) => put(&[spark.hit[0], spark.hit[1], spark.hit[2], spark.age]),
            None => put(&[0.0; 4]),
        }
    }
    for slot in 0..MAX_SPARKS {
        put(&[sparks.get(slot).map_or(0.0, |spark| spark.radius)]);
    }
    // Each spark's light: the flare's colour for what struck it.
    for slot in 0..MAX_SPARKS {
        let [r, g, b] = sparks.get(slot).map_or([0.0; 3], |spark| spark.light);
        put(&[r, g, b, 0.0]);
    }
    put(&[sparks.first().map_or(1.0, |spark| spark.seconds)]);
    // Where the craft stirs the puddles: each wheel on the ground, or
    // its middle (none under thermal vision).
    let wakes = if feed.infrared {
        &[][..]
    } else {
        &feed.wakes[..feed.wakes.len().min(MAX_WAKES)]
    };
    put(&[wakes.len() as f32]);
    for slot in 0..MAX_WAKES {
        let wake = wakes.get(slot).copied().unwrap_or_default();
        put(&wake.position);
        // Negative: on the water (`craft_wakes`).
        put(&[wake.strength.clamp(-1.0, 1.0)]);
    }
    // Where the craft's spray rings the puddles it falls back on.
    let rings = if feed.infrared {
        &[][..]
    } else {
        &feed.spray_rings[..feed.spray_rings.len().min(crate::craft_spray::MOST_RINGS)]
    };
    put(&[rings.len() as f32]);
    for slot in 0..crate::craft_spray::MOST_RINGS {
        put(&rings.get(slot).copied().unwrap_or_default());
    }
    // Each spark's crystal's middle, and the points on its faces its arcs
    // jump between.
    for slot in 0..MAX_SPARKS {
        let [x, y, z] = sparks.get(slot).map_or([0.0; 3], |spark| spark.middle);
        put(&[x, y, z, 0.0]);
    }
    for slot in 0..MAX_SPARKS {
        let anchors = sparks
            .get(slot)
            .map_or([[[0.0; 3]; 2]; crate::crystal_flares::ARC_PAIRS], |spark| {
                spark.anchors
            });
        for point in anchors.iter().flatten() {
            put(point);
            put(&[0.0]);
        }
    }
    // Full path tracing's ground and sky as light: how the ground's
    // baked shade rose with the sun's facing (its reflectance's source),
    // and how strongly the modern sky lights what it shines on.
    put(&[terrain.map_or(0.5, |terrain| terrain.shade_slope)]);
    put(&[sky.light.max(0.0)]);
    put(&[if shape.upscaled { 1.0 } else { 0.0 }]);
    put(&[shape.emissive_groups as f32]);
    // The remastered camera's eye under the water, and how far one sees
    // through the water round it.
    put(&[
        if feed.eye_under { 1.0 } else { 0.0 },
        crate::world_scene::MURK_REACH,
    ]);
    // The clouds' rise either way, a share of the layer's thickness, and
    // the near band, just past the craft the camera follows.
    put(&[
        clouds.rise.clamp(0.0, 0.5) * (clouds.top - clouds.base).max(100.0),
        feed.craft.map_or(0.0, |craft| {
            (craft.iter().map(|axis| axis * axis).sum::<f32>().sqrt() + CRAFT_REACH).min(4000.0)
        }),
    ]);
    // The craft's wake through them, only under the modern sky's clouds.
    let wake = if clouded {
        &feed.cloud_wake[..feed.cloud_wake.len().min(crate::cloud_wake::WAKE_POINTS)]
    } else {
        &[][..]
    };
    put(&[wake.len() as f32]);
    for slot in 0..crate::cloud_wake::WAKE_POINTS {
        put(&wake.get(slot).copied().unwrap_or_default());
    }
    debug_assert_eq!(at, FRAME_FLOATS);
    FrameBlock {
        block,
        view,
        previous,
        distance_fog,
        mist: mist_density > 0.0 || distance_fog || drift_words[0] > 0.0 || feed.eye_under,
        volume,
        modern_sky,
        clouded,
        over: (precip_kind > 0.0 && precip_density > 0.0) || !sparks.is_empty(),
    }
}

/// The water's spray and its splash buffer (`pt_splash.glsl`), the tracer's
/// and the raster's: the blasts in the water over its plane at `plane`
/// thrown up by `eruptions` at the feed's clock (none over lava or under
/// thermal vision), their eruptions (kind 1) and where their water falls
/// back first, then retail's splashes, the nearest first, as rings on the
/// water.
pub(super) fn water_splashes(
    eruptions: &mut crate::scene_spray::Eruptions,
    feed: &SceneFeed,
    plane: Option<f32>,
) -> (crate::scene_spray::Spray, Vec<[f32; 7]>) {
    let spray_look = crate::modern_tuning::current().water(feed.world);
    // Lava throws up no water.
    let spray = match plane {
        Some(plane) if !feed.infrared && spray_look.glow <= 0.0 => {
            eruptions.update(feed.clock_micros, &feed.blasts);
            eruptions.spray(
                feed.clock_micros,
                feed.camera.eye,
                plane,
                crate::scene_spray::SprayLook {
                    plume: spray_look.plume,
                    drops: spray_look.drops,
                },
            )
        }
        _ => crate::scene_spray::Spray::default(),
    };
    let mut splashes: Vec<[f32; 7]> = feed
        .splashes
        .iter()
        .map(|splash| {
            let [x, y, z] = splash.position;
            [x, y, z, splash.age, splash.scale, 0.0, 0.0]
        })
        .collect();
    splashes.sort_by(|a, b| (a[0] * a[0] + a[2] * a[2]).total_cmp(&(b[0] * b[0] + b[2] * b[2])));
    splashes.truncate(SPLASHES.saturating_sub(spray.surface.len()));
    splashes.splice(0..0, spray.surface.iter().copied().take(SPLASHES));
    (spray, splashes)
}

/// The radical inverse of `index` in `base`: the Halton sequence the
/// jitter walks.
pub(super) fn halton(mut index: u32, base: u32) -> f32 {
    let mut fraction = 1.0;
    let mut result = 0.0;
    while index > 0 {
        fraction /= base as f32;
        result += fraction * (index % base) as f32;
        index /= base;
    }
    result
}

/// The bytes of `values`, which are plain `repr(C)` data without padding.
pub(super) fn bytes_of<T: Copy>(values: &[T]) -> &[u8] {
    // SAFETY: T is plain data (vertices, indices, floats) and the slice is
    // read only as bytes.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}

impl State {
    unsafe fn address(core: &Core, buffer: &Buffer) -> u64 {
        core.device.get_buffer_device_address(
            &vk::BufferDeviceAddressInfo::default().buffer(buffer.buffer),
        )
    }

    /// `slot` holding at least `size` bytes of `usage`, the old buffer kept
    /// for release after the next wait when it is replaced.
    unsafe fn grown(
        &mut self,
        core: &Core,
        slot: &mut Option<Grown>,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Result<(), VkDeviceError> {
        if slot.as_ref().is_some_and(|grown| grown.capacity >= size) {
            return Ok(());
        }
        if let Some(old) = slot.take() {
            self.transient.push(old.buffer);
        }
        let capacity = size.max(1 << 16).next_power_of_two();
        let buffer = self.buffer(core, capacity, usage, location)?;
        *slot = Some(Grown { buffer, capacity });
        Ok(())
    }

    /// A device-local buffer of `usage` holding `bytes`, copied in through a
    /// staging buffer by the frame's commands; [`upload_barrier`] orders the
    /// copy before its readers.
    pub(super) unsafe fn device_buffer(
        &mut self,
        core: &Core,
        bytes: &[u8],
        usage: vk::BufferUsageFlags,
    ) -> Result<Buffer, VkDeviceError> {
        let size = bytes.len().max(4) as u64;
        let mut staging = self.buffer(
            core,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        let mapped = staging
            .allocation
            .as_mut()
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("staging memory is not mapped".into()))?;
        mapped[..bytes.len()].copy_from_slice(bytes);
        let buffer = self.buffer(
            core,
            size,
            usage | vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuOnly,
        )?;
        let command = self.begin(core)?;
        core.device.cmd_copy_buffer(
            command,
            staging.buffer,
            buffer.buffer,
            &[vk::BufferCopy::default().size(size)],
        );
        self.transient.push(staging);
        Ok(buffer)
    }

    /// Write `bytes` at the start of `slot`'s host-visible buffer.
    fn write(slot: &mut Option<Grown>, bytes: &[u8]) -> Result<(), VkDeviceError> {
        let mapped = slot
            .as_mut()
            .and_then(|grown| grown.buffer.allocation.as_mut())
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("tracer memory is not mapped".into()))?;
        mapped[..bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// An acceleration structure of `ty` holding `size` bytes in `slot`,
    /// made anew when it is too small.
    unsafe fn acceleration(
        &mut self,
        core: &Core,
        slot: &mut Option<Acceleration>,
        ty: vk::AccelerationStructureTypeKHR,
        size: u64,
    ) -> Result<(), VkDeviceError> {
        let loader = core
            .acceleration_loader
            .as_ref()
            .expect("tracing devices load acceleration structures");
        if slot.as_ref().is_some_and(|built| built.capacity >= size) {
            return Ok(());
        }
        if let Some(old) = slot.take() {
            // A frame in flight may still trace it.
            self.retired_accelerations.push(old.handle);
            self.transient.push(old.buffer);
        }
        let capacity = size.next_power_of_two();
        let buffer = self.buffer(
            core,
            capacity,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            MemoryLocation::GpuOnly,
        )?;
        let handle = loader
            .create_acceleration_structure(
                &vk::AccelerationStructureCreateInfoKHR::default()
                    .buffer(buffer.buffer)
                    .size(capacity)
                    .ty(ty),
                None,
            )
            .map_err(call("acceleration structure"))?;
        *slot = Some(Acceleration {
            handle,
            buffer,
            capacity,
        });
        Ok(())
    }

    /// A traced output image of `format` at `extent` in `slot`.
    pub(super) unsafe fn traced_image(
        &mut self,
        core: &Core,
        slot: &mut Option<Image>,
        format: vk::Format,
        extent: [u32; 2],
    ) -> Result<(), VkDeviceError> {
        self.storage_image(core, slot, format, extent, vk::ImageUsageFlags::empty())
    }

    /// An upscaler's output at `extent` in `slot`, which DLSS also clears
    /// (at its native size).
    pub(super) unsafe fn upscaled_image(
        &mut self,
        core: &Core,
        slot: &mut Option<Image>,
        extent: [u32; 2],
    ) -> Result<(), VkDeviceError> {
        self.storage_image(
            core,
            slot,
            LIGHT_IMAGE,
            extent,
            vk::ImageUsageFlags::TRANSFER_DST,
        )
    }

    pub(super) unsafe fn storage_image(
        &mut self,
        core: &Core,
        slot: &mut Option<Image>,
        format: vk::Format,
        extent: [u32; 2],
        usage: vk::ImageUsageFlags,
    ) -> Result<(), VkDeviceError> {
        if slot.as_ref().is_some_and(|image| image.extent == extent) {
            return Ok(());
        }
        if let Some(old) = slot.take() {
            self.retired.push(old);
        }
        *slot = Some(self.image(
            core,
            extent,
            format,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED | usage,
            false,
        )?);
        Ok(())
    }

    /// Trace `feed` into `target`: colour and depth from the tracer, ready
    /// for the raster's later draws.
    pub(super) unsafe fn trace(
        &mut self,
        core: &Core,
        target: &Target,
        feed: &SceneFeed,
        textures: &[&CpuTexture],
    ) -> Result<(), VkDeviceError> {
        if core.acceleration_loader.is_none()
            || (feed.triangles.is_empty() && feed.terrain.is_none())
        {
            return Ok(());
        }
        let mut tracer = match self.tracer.take() {
            Some(tracer) => tracer,
            None => Box::new(Tracer::new(core)?),
        };
        let _scope = crate::cpu_profile::scope("trace: record");
        // No wait for the frames in flight: the buffers, descriptor sets and
        // timestamps the CPU writes are this frame slot's own, which the
        // device waited for when it opened the slot's recording.
        let recording = std::time::Instant::now();
        let result = self.trace_with(core, &mut tracer, target, feed, textures);
        self.perf.traced(recording.elapsed());
        self.tracer = Some(tracer);
        result
    }

    /// Make the tracer, and the upscaler the settings choose for `target`,
    /// before a traced frame needs them. Made on a world's first traced
    /// frame, they stalled it: DLSS's feature takes about 0.9 s, and a new
    /// build's shaders compile when the tracer is made (the
    /// sound starts while the picture waits). Nothing once both are made.
    pub(super) unsafe fn prepare_trace(
        &mut self,
        core: &Core,
        target: &Target,
    ) -> Result<(), VkDeviceError> {
        if core.acceleration_loader.is_none() {
            return Ok(());
        }
        let mut tracer = match self.tracer.take() {
            Some(tracer) => tracer,
            None => Box::new(Tracer::new(core)?),
        };
        let result = self.prepare_upscaler(core, &mut tracer, target);
        self.tracer = Some(tracer);
        result
    }

    /// [`Self::prepare_trace`]'s upscaler, as [`Self::trace_with`] chooses
    /// and sizes it.
    unsafe fn prepare_upscaler(
        &mut self,
        core: &Core,
        tracer: &mut Tracer,
        target: &Target,
    ) -> Result<(), VkDeviceError> {
        let display = target.color.extent;
        let modern = self.modern;
        let choice = if tracer.failed_upscalers.contains(&modern.upscaler) {
            UpscalerChoice::Off
        } else {
            UpscalerChoice::from_settings(modern)
        };
        let Ok(Some(extent)) = Upscaler::render_size(core, choice, display) else {
            return Ok(());
        };
        if tracer
            .upscaler
            .as_ref()
            .is_some_and(|upscaler| upscaler.fits(choice, display, extent))
        {
            return Ok(());
        }
        // The old upscaler may still be at work on the last frame.
        self.settle(core)?;
        let command = self.begin(core)?;
        self.end_rendering(core);
        if let Some(old) = tracer.upscaler.take() {
            old.destroy(core);
        }
        match Upscaler::new(core, command, choice, display, extent) {
            Ok(upscaler) => tracer.upscaler = Some(upscaler),
            Err(error) => tracer.upscaler_unavailable(modern.upscaler, &error),
        }
        Ok(())
    }

    unsafe fn trace_with(
        &mut self,
        core: &Core,
        tracer: &mut Tracer,
        target: &Target,
        feed: &SceneFeed,
        textures: &[&CpuTexture],
    ) -> Result<(), VkDeviceError> {
        let loader = core
            .acceleration_loader
            .as_ref()
            .expect("checked by the caller");
        let command = self.begin(core)?;
        self.end_rendering(core);
        // This frame slot's buffers and sets; the GPU may still be reading
        // the other slot's. What the GPU itself rewrites (the bottom and top
        // levels, the images, the scratch) waits for the frames before.
        let slot = self.slot;
        frame_barrier(core, command);
        // GPU time: this slot's last timestamps, then this frame's.
        let timestamps = tracer.timestamps[slot];
        let timed = timestamps != vk::QueryPool::null();
        let stamp = |index: u32| {
            if timed {
                core.device.cmd_write_timestamp2(
                    command,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    timestamps,
                    index,
                );
            }
        };
        if timed {
            tracer.read_timestamps(core, slot);
            core.device
                .cmd_reset_query_pool(command, timestamps, 0, TIMESTAMPS);
        }
        stamp(0);

        // The feed's textures, each sprite in its own mipmapped image, in
        // slots of the shader's array; one the frame did not resolve reads
        // as untextured.
        let resolved: HashMap<u64, &CpuTexture> = textures
            .iter()
            .map(|texture| (texture.content_key(), *texture))
            .collect();
        let sampler = tracer.samplers[self.filtering.min(6) as usize];
        let mut slots = Vec::with_capacity(feed.textures.len());
        // Each feed texture's mean light (1: none, its faces untextured).
        let mut brightness = Vec::with_capacity(feed.textures.len());
        let mut images = Vec::new();
        for (key, _) in &feed.textures {
            let view = match resolved.get(key) {
                Some(texture) if (images.len() as u32) < TEXTURE_SLOTS => {
                    Some(self.traced_texture(core, tracer, command, texture)?)
                }
                _ => None,
            };
            match view {
                Some((view, mean)) => {
                    slots.push(images.len() as u32);
                    brightness.push(mean);
                    images.push(
                        vk::DescriptorImageInfo::default()
                            .sampler(sampler)
                            .image_view(view)
                            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                    );
                }
                None => {
                    slots.push(NO_TEXTURE);
                    brightness.push(1.0);
                }
            }
        }
        // Sprites unsampled for a while go once this frame's work is done.
        let presents = self.presents;
        let stale: Vec<u64> = tracer
            .textures
            .iter()
            .filter(|(_, texture)| texture.last_used + TEXTURE_KEEP_PRESENTS < presents)
            .map(|(key, _)| *key)
            .collect();
        for key in stale {
            if let Some(texture) = tracer.textures.remove(&key) {
                self.retired.push(texture.image);
            }
        }
        stamp(1);
        // The water's triangles first, then the painted sky's, then the
        // models': each gets its own bottom level over its range, so the
        // water and the sky can have their own masks.
        let (water, rest): (Vec<&crate::scene_feed::SceneTriangle>, Vec<_>) = feed
            .triangles
            .iter()
            .partition(|triangle| triangle.surface == SceneSurface::Water);
        let (sky, models): (Vec<_>, Vec<_>) = rest
            .into_iter()
            .partition(|triangle| triangle.surface == SceneSurface::Sky);
        // The sunk glows last of all, after the masked sprites' triangles,
        // each in a bottom level rays test.
        // The models outside the view lay no sunk glow: only the view
        // through the water sums those.
        let (glows, models): (Vec<_>, Vec<_>) =
            models
                .into_iter()
                .partition(|triangle: &&crate::scene_feed::SceneTriangle| {
                    triangle.surface == SceneSurface::Glow
                });
        let glows: Vec<_> = glows
            .into_iter()
            .filter(|triangle: &&crate::scene_feed::SceneTriangle| !triangle.culled)
            .collect();
        let (cutouts, models): (Vec<_>, Vec<_>) = models
            .into_iter()
            .partition(|triangle: &&crate::scene_feed::SceneTriangle| triangle.cutout);
        // Each kind's models outside the view after the view's own, in a
        // bottom level of their own the eye's rays leave out; its record
        // offset is the view's count, so the shaders find their triangles
        // past the view's (pt_trace.comp's hit_surface).
        let (outside, models): (Vec<_>, Vec<_>) = models
            .into_iter()
            .partition(|triangle: &&crate::scene_feed::SceneTriangle| triangle.culled);
        let (outside_cutouts, cutouts): (Vec<_>, Vec<_>) = cutouts
            .into_iter()
            .partition(|triangle: &&crate::scene_feed::SceneTriangle| triangle.culled);
        let water_count = water.len() as u32;
        let sky_count = sky.len() as u32;
        let triangle_count = models.len() as u32;
        let outside_count = outside.len() as u32;
        let cutout_count = cutouts.len() as u32;
        let outside_cutout_count = outside_cutouts.len() as u32;
        let glow_count = glows.len() as u32;
        let ordered = || {
            water
                .iter()
                .chain(&sky)
                .chain(&models)
                .chain(&outside)
                .chain(&cutouts)
                .chain(&outside_cutouts)
                .chain(&glows)
                .copied()
        };
        // Each model's heat for thermal vision, by its name, kept until the
        // tuning changes.
        let tuning = crate::modern_tuning::current();
        // The player's lighting preset; thermal vision shows heat, ungraded.
        let lighting = tuning.lighting(self.modern.lighting);
        if !tracer
            .heat_tuning
            .as_ref()
            .is_some_and(|known| Rc::ptr_eq(known, &tuning))
        {
            tracer.model_heat.clear();
            tracer.heat_tuning = Some(tuning.clone());
        }
        let thermal = tuning.thermal();
        let materials = tuning.materials();
        let model_looks: HashMap<u16, ModelLook> = feed
            .model_names
            .iter()
            .map(|(slot, name)| {
                let look = *tracer
                    .model_heat
                    .entry(name.clone())
                    .or_insert_with(|| ModelLook {
                        heat: thermal.listed(name),
                        material: packed_material(materials.of(name)),
                        gun: thermal.is_gun(name),
                        wreck: thermal.is_wreck(name),
                        emission: materials.emitter_of(name),
                        crystal: tuning.crystals().is_crystal(name),
                    });
                (*slot, look)
            })
            .collect();
        // Each texture's own material by its sprite (materials.sprites),
        // bare, and the rows above a share of its height with theirs, that
        // share in 65536ths over their material.
        let sprite_looks: Vec<Option<(u32, u32)>> = feed
            .textures
            .iter()
            .map(|&(_, sprite)| {
                materials.of_sprite(sprite).map(|(material, above)| {
                    let rows = above.map_or(0, |(v, material)| {
                        packed_material(material) | ((v * 65536.0).round() as u32).min(65535) << 16
                    });
                    (packed_material(material) | BARE, rows)
                })
            })
            .collect();
        // Each object's heat: its root model's, and what it does.
        let roots: Vec<_> = feed
            .objects
            .iter()
            .map(|object| {
                let look = object.root.and_then(|root| model_looks.get(&root));
                let root = super::object_heat::RootLook {
                    heat: look.and_then(|look| look.heat).unwrap_or(thermal.models),
                    wreck: look.is_some_and(|look| look.wreck),
                };
                (*object, root)
            })
            .collect();
        let warmth = tracer
            .object_heat
            .step(feed.clock_micros, &roots, &feed.shots, thermal);
        // The faces retail drew unlit, as lights the path pass samples
        // (emissive.rs), and each triangle's place among them.
        // The thruster lights while their vehicle thrusts: faces lit as
        // retail drew them that glow, as its unlit faces do, up to their gain.
        let thrust =
            tracer
                .thrust_glow
                .update(feed.clock_micros, &feed.thrust_glows, &feed.thrusts);
        let thrust_gains: HashMap<[u32; 3], f32> = if thrust > 0.02 {
            feed.thrust_glows
                .iter()
                .flat_map(|glow| {
                    let gain = 1.0 + (glow.gain - 1.0) * thrust;
                    feed.triangles[glow.triangles.clone()]
                        .iter()
                        .map(move |triangle| (triangle.vertices, gain))
                })
                .collect()
        } else {
            HashMap::new()
        };
        let lit_as_glowing: Vec<crate::scene_feed::SceneTriangle> = ordered()
            .map(|triangle| {
                let mut triangle = *triangle;
                if thrust_gains.contains_key(&triangle.vertices) {
                    triangle.surface = SceneSurface::Emissive;
                }
                triangle
            })
            .collect();
        let emissive = super::emissive::emissive_list(
            &lit_as_glowing,
            &feed.vertices,
            |triangle| {
                thrust_gains
                    .get(&triangle.vertices)
                    .copied()
                    .unwrap_or_else(|| {
                        triangle
                            .model
                            .and_then(|model| model_looks.get(&model))
                            .map_or(1.0, |look| look.emission)
                    })
            },
            |triangle| {
                triangle
                    .texture
                    .and_then(|index| brightness.get(index as usize).copied())
                    .unwrap_or(1.0)
            },
        );
        let triangle_words: Vec<[u32; 6]> = ordered()
            .map(|triangle| {
                let texture = triangle
                    .texture
                    .and_then(|index| slots.get(index as usize).copied())
                    .unwrap_or(NO_TEXTURE);
                let surface = match triangle.surface {
                    SceneSurface::Opaque if thrust_gains.contains_key(&triangle.vertices) => 2,
                    SceneSurface::Opaque => 0,
                    SceneSurface::Water => 1,
                    SceneSurface::Emissive | SceneSurface::Shown => 2,
                    SceneSurface::Sky => 3,
                    SceneSurface::Glow => 4,
                };
                let look = triangle.model.and_then(|model| model_looks.get(&model));
                let mut material = look.map_or(MATTE, |look| look.material);
                let mut above = 0;
                if let Some(&Some((own, rows))) = triangle
                    .texture
                    .and_then(|index| sprite_looks.get(index as usize))
                {
                    material = own;
                    above = rows;
                }
                // A model no name lists takes its object's heat, so no
                // attachment reads apart from what it is on.
                let object = triangle.object.map(|index| index as usize);
                let heat = look
                    .and_then(|look| look.heat)
                    .or(object.map(|index| roots[index].1.heat))
                    .unwrap_or(thermal.models)
                    + object.map_or(0.0, |index| {
                        let gun = look.is_some_and(|look| look.gun);
                        warmth[index].warmth + if gun { warmth[index].fired } else { 0.0 }
                    });
                // A masked sky object (the Moon) covers what lies behind it
                // in the sky's sum (pt_trace.comp's sky_objects).
                if triangle.surface == SceneSurface::Sky && triangle.cutout {
                    material |= SKY_OBJECT_MASKED;
                }
                if triangle.own_fog {
                    material |= OWN_FOG;
                }
                if look.is_some_and(|look| look.crystal) {
                    material |= CRYSTAL;
                }
                [
                    texture,
                    surface,
                    triangle.light.unwrap_or(NO_LIGHT),
                    heat.to_bits(),
                    material,
                    above,
                ]
            })
            .collect();
        let indices: Vec<u32> = ordered().flat_map(|triangle| triangle.vertices).collect();
        let geometry = vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR;
        let vertex_bytes = bytes_of(&feed.vertices);
        let index_bytes = bytes_of(&indices);
        let triangle_bytes = bytes_of(&triangle_words);
        self.grown(
            core,
            &mut tracer.vertices[slot],
            vertex_bytes.len() as u64,
            geometry,
            MemoryLocation::CpuToGpu,
        )?;
        self.grown(
            core,
            &mut tracer.indices[slot],
            index_bytes.len() as u64,
            geometry,
            MemoryLocation::CpuToGpu,
        )?;
        self.grown(
            core,
            &mut tracer.triangles[slot],
            triangle_bytes.len() as u64,
            geometry,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.vertices[slot], vertex_bytes)?;
        Self::write(&mut tracer.indices[slot], index_bytes)?;
        Self::write(&mut tracer.triangles[slot], triangle_bytes)?;
        for (grown, bytes) in [
            (&mut tracer.emissive_faces[slot], bytes_of(&emissive.faces)),
            (
                &mut tracer.emissive_groups[slot],
                bytes_of(&emissive.groups),
            ),
            (
                &mut tracer.triangle_emitters[slot],
                bytes_of(&emissive.of_triangle),
            ),
        ] {
            self.grown(
                core,
                grown,
                bytes.len() as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
            )?;
            Self::write(grown, bytes)?;
        }
        let vertex_address =
            Self::address(core, &tracer.vertices[slot].as_ref().expect("grown").buffer);
        let index_address =
            Self::address(core, &tracer.indices[slot].as_ref().expect("grown").buffer);

        // The ground: uploaded once per terrain mesh, its triangles by tile,
        // and each tile's bottom level built once after, for the device's
        // fastest tracing.
        let ground_mesh = feed.terrain.as_deref();
        if let Some(mesh) = ground_mesh {
            if tracer
                .ground
                .as_ref()
                .is_none_or(|ground| ground.generation != mesh.generation)
            {
                let _scope = crate::cpu_profile::scope("trace: ground upload");
                let vertices = self.device_buffer(core, bytes_of(&mesh.vertices), geometry)?;
                // A mesh that moved the last one's points (a bridge rising)
                // keeps its triangles and tiles: new points, and the tiles
                // they moved built again.
                let patch = mesh.changed.as_ref().and_then(|(previous, moved)| {
                    tracer
                        .ground
                        .take_if(|ground| ground.generation == *previous)
                        .map(|ground| (ground, moved))
                });
                tracer.ground = Some(match patch {
                    Some((old, moved)) => {
                        self.transient.push(old.vertices);
                        let mut unbuilt = old.unbuilt;
                        for &tile in moved {
                            if old.source.tiles.ranges[tile][1] > 0 && !unbuilt.contains(&tile) {
                                unbuilt.push(tile);
                            }
                        }
                        Ground {
                            generation: mesh.generation,
                            vertices,
                            indices: old.indices,
                            triangles: old.triangles,
                            source: std::sync::Arc::new(
                                crate::scene_ground_detail::DetailGround::moved(&old.source, mesh),
                            ),
                            unbuilt,
                        }
                    }
                    None => {
                        if let Some(old) = tracer.ground.take() {
                            self.transient
                                .extend([old.vertices, old.indices, old.triangles]);
                        }
                        let source = std::sync::Arc::new(
                            crate::scene_ground_detail::DetailGround::new(mesh),
                        );
                        let indices =
                            self.device_buffer(core, bytes_of(&source.tiles.indices), geometry)?;
                        let triangles =
                            self.device_buffer(core, bytes_of(&source.tiles.triangles), geometry)?;
                        Ground {
                            generation: mesh.generation,
                            vertices,
                            indices,
                            triangles,
                            unbuilt: (0..source.tiles.ranges.len())
                                .filter(|&tile| source.tiles.ranges[tile][1] > 0)
                                .collect(),
                            source,
                        }
                    }
                });
                upload_barrier(core, command);
            }
        }
        // The detail patch round the eye, while puddles stand to carve: built
        // again when the eye reaches another tile, the ground changes or the
        // puddles' fill moves a step (scene_ground_detail.rs).
        let wanted_patch = ground_mesh.and_then(|mesh| {
            let look = crate::scene_ground_detail::CarveLook::for_world(
                feed.weather.map_or(0.0, |weather| weather.puddles),
                mesh.plane,
                &tuning.water(feed.world),
            )?;
            Some(crate::scene_ground_detail::PatchKey::new(
                mesh.generation,
                crate::scene_ground_detail::patch_first([feed.camera.eye[0], feed.camera.eye[2]]),
                look,
            ))
        }); // A patch built on the worker takes the place of the last
            // PATCH_MICROS after it started, if it is still the one wanted; the
            // last stays drawn till then.
        if tracer
            .patch_job
            .as_ref()
            .is_some_and(|(started, _)| feed.clock_micros.abs_diff(*started) >= PATCH_MICROS)
        {
            let (_, job) = tracer.patch_job.take().expect("checked above");
            let built = job.join().ok();
            if let Some(patch) = built.filter(|patch| Some(patch.key) == wanted_patch) {
                if let Some(old) = tracer.detail.take() {
                    self.transient
                        .extend([old.vertices, old.indices, old.triangles]);
                }
                if !patch.triangles.is_empty() {
                    let vertices = self.device_buffer(core, bytes_of(&patch.vertices), geometry)?;
                    let indices = self.device_buffer(core, bytes_of(&patch.indices), geometry)?;
                    let triangles =
                        self.device_buffer(core, bytes_of(&patch.triangles), geometry)?;
                    tracer.detail = Some(Detail {
                        key: patch.key,
                        vertices,
                        indices,
                        triangles,
                        count: patch.triangles.len() as u32,
                        vertex_count: patch.vertices.len() as u32,
                        built: false,
                    });
                    upload_barrier(core, command);
                }
            }
        }
        if wanted_patch.is_none() {
            if let Some(old) = tracer.detail.take() {
                self.transient
                    .extend([old.vertices, old.indices, old.triangles]);
            }
        }
        if let (Some(key), Some(ground)) = (wanted_patch, tracer.ground.as_ref()) {
            let current = tracer.detail.as_ref().map(|detail| detail.key);
            if current != Some(key) && tracer.patch_job.is_none() {
                let source = ground.source.clone();
                tracer.patch_job = Some((
                    feed.clock_micros,
                    std::thread::spawn(move || {
                        crate::scene_ground_detail::build_patch(&source, key)
                    }),
                ));
            }
        } // The bottom levels still to build: each tile of a new ground, then
          // a new patch.
        let tile_count = crate::scene_ground_detail::TILES * crate::scene_ground_detail::TILES;
        tracer.ground_tiles.resize_with(tile_count, || None);
        let ground_stride = std::mem::size_of::<TerrainMeshVertex>() as u64;
        let tile_geometries: Vec<(usize, u32, [vk::AccelerationStructureGeometryKHR; 1])> =
            match (ground_mesh, tracer.ground.as_ref()) {
                (Some(mesh), Some(ground)) => {
                    let vertices = Self::address(core, &ground.vertices);
                    let indices = Self::address(core, &ground.indices);
                    ground
                        .unbuilt
                        .iter()
                        .map(|&tile| {
                            let [first, count] = ground.source.tiles.ranges[tile];
                            (
                                tile,
                                count,
                                [triangle_geometry(
                                    vertices,
                                    ground_stride,
                                    mesh.vertices.len().saturating_sub(1) as u32,
                                    indices + u64::from(first) * 12,
                                    true,
                                )],
                            )
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
        let slow_build = |geometries| {
            vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(geometries)
        };
        let mut ground_scratch = 0;
        let mut tile_scratch = 0;
        for (tile, count, geometries) in &tile_geometries {
            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &slow_build(geometries),
                &[*count],
                &mut sizes,
            );
            self.acceleration(
                core,
                &mut tracer.ground_tiles[*tile],
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                sizes.acceleration_structure_size,
            )?;
            tile_scratch = tile_scratch.max(sizes.build_scratch_size);
        }
        let detail_geometries =
            tracer
                .detail
                .as_ref()
                .filter(|detail| !detail.built)
                .map(|detail| {
                    (
                        detail.count,
                        [triangle_geometry(
                            Self::address(core, &detail.vertices),
                            std::mem::size_of::<crate::scene_ground_detail::DetailVertex>() as u64,
                            detail.vertex_count.saturating_sub(1),
                            Self::address(core, &detail.indices),
                            true,
                        )],
                    )
                });
        if let Some((count, geometries)) = &detail_geometries {
            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &slow_build(geometries),
                &[*count],
                &mut sizes,
            );
            self.acceleration(
                core,
                &mut tracer.detail_blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                sizes.acceleration_structure_size,
            )?;
            ground_scratch = ground_scratch.max(sizes.build_scratch_size);
        }

        // The models', the water's, the sky's, the masked sprites' and the
        // sunk glows' bottom levels, rebuilt each frame over their ranges of
        // the feed's triangles; all opaque but the masked sprites' and the
        // glows'.
        let outside_first = water_count + sky_count + triangle_count;
        let cutout_first = outside_first + outside_count;
        let outside_cutout_first = cutout_first + cutout_count;
        let glow_first = outside_cutout_first + outside_cutout_count;
        let vertex_stride = std::mem::size_of::<crate::scene_feed::SceneVertex>() as u64;
        let max_vertex = feed.vertices.len().saturating_sub(1) as u32;
        let model_geometries = [triangle_geometry(
            vertex_address,
            vertex_stride,
            max_vertex,
            index_address + u64::from(water_count + sky_count) * 12,
            true,
        )];
        let sky_geometries = [triangle_geometry(
            vertex_address,
            vertex_stride,
            max_vertex,
            index_address + u64::from(water_count) * 12,
            true,
        )];
        let water_geometries = [triangle_geometry(
            vertex_address,
            vertex_stride,
            max_vertex,
            index_address,
            true,
        )];
        let cutout_geometries = [triangle_geometry(
            vertex_address,
            vertex_stride,
            max_vertex,
            index_address + u64::from(cutout_first) * 12,
            false,
        )];
        let glow_geometries = [triangle_geometry(
            vertex_address,
            vertex_stride,
            max_vertex,
            index_address + u64::from(glow_first) * 12,
            false,
        )];
        let outside_geometries = [triangle_geometry(
            vertex_address,
            vertex_stride,
            max_vertex,
            index_address + u64::from(outside_first) * 12,
            true,
        )];
        let outside_cutout_geometries = [triangle_geometry(
            vertex_address,
            vertex_stride,
            max_vertex,
            index_address + u64::from(outside_cutout_first) * 12,
            false,
        )];
        let fast_build = |geometries| {
            vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(geometries)
        };
        let bottom = (triangle_count > 0).then(|| {
            fast_build(&model_geometries)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        });
        let water_bottom = (water_count > 0).then(|| fast_build(&water_geometries));
        let sky_bottom = (sky_count > 0).then(|| fast_build(&sky_geometries));
        let cutout_bottom = (cutout_count > 0).then(|| fast_build(&cutout_geometries));
        let glow_bottom = (glow_count > 0).then(|| fast_build(&glow_geometries));
        let outside_bottom = (outside_count > 0).then(|| fast_build(&outside_geometries));
        let outside_cutout_bottom =
            (outside_cutout_count > 0).then(|| fast_build(&outside_cutout_geometries));
        let mut bottom_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        if let Some(info) = &bottom {
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                info,
                &[triangle_count],
                &mut bottom_sizes,
            );
            self.acceleration(
                core,
                &mut tracer.blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                bottom_sizes.acceleration_structure_size,
            )?;
        }
        let mut water_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        if let Some(info) = &water_bottom {
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                info,
                &[water_count],
                &mut water_sizes,
            );
            self.acceleration(
                core,
                &mut tracer.water_blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                water_sizes.acceleration_structure_size,
            )?;
        }
        let mut sky_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        if let Some(info) = &sky_bottom {
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                info,
                &[sky_count],
                &mut sky_sizes,
            );
            self.acceleration(
                core,
                &mut tracer.sky_blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                sky_sizes.acceleration_structure_size,
            )?;
        }
        let mut cutout_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        if let Some(info) = &cutout_bottom {
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                info,
                &[cutout_count],
                &mut cutout_sizes,
            );
            self.acceleration(
                core,
                &mut tracer.cutout_blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                cutout_sizes.acceleration_structure_size,
            )?;
        }
        let mut glow_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        if let Some(info) = &glow_bottom {
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                info,
                &[glow_count],
                &mut glow_sizes,
            );
            self.acceleration(
                core,
                &mut tracer.glow_blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                glow_sizes.acceleration_structure_size,
            )?;
        }
        let mut outside_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        if let Some(info) = &outside_bottom {
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                info,
                &[outside_count],
                &mut outside_sizes,
            );
            self.acceleration(
                core,
                &mut tracer.outside_blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                outside_sizes.acceleration_structure_size,
            )?;
        }
        let mut outside_cutout_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        if let Some(info) = &outside_cutout_bottom {
            loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                info,
                &[outside_cutout_count],
                &mut outside_cutout_sizes,
            );
            self.acceleration(
                core,
                &mut tracer.outside_cutout_blas,
                vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                outside_cutout_sizes.acceleration_structure_size,
            )?;
        }

        // The top level: the models at the eye, and the ground's tiles round
        // it, each at its copy across the wrap nearest the eye.
        let address_of = |slot: &Option<Acceleration>| {
            slot.as_ref().map(|built| {
                loader.get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(built.handle),
                )
            })
        };
        let mut instances = Vec::with_capacity(10);
        if bottom.is_some() {
            let address = address_of(&tracer.blas).expect("made above");
            instances.push(instance(
                [0.0; 3],
                INSTANCE_MODELS,
                MASK_SOLID | MASK_MODEL,
                address,
            ));
        }
        if water_bottom.is_some() {
            let address = address_of(&tracer.water_blas).expect("made above");
            instances.push(instance([0.0; 3], INSTANCE_WATER, MASK_WATER, address));
        }
        if sky_bottom.is_some() {
            let address = address_of(&tracer.sky_blas).expect("made above");
            instances.push(instance([0.0; 3], INSTANCE_SKY, MASK_SKY, address));
        }
        if cutout_bottom.is_some() {
            let address = address_of(&tracer.cutout_blas).expect("made above");
            instances.push(instance(
                [0.0; 3],
                INSTANCE_CUTOUT,
                MASK_SOLID | MASK_MODEL,
                address,
            ));
        }
        if glow_bottom.is_some() {
            let address = address_of(&tracer.glow_blas).expect("made above");
            instances.push(instance([0.0; 3], INSTANCE_GLOW, MASK_GLOW, address));
        }
        // The models outside the view go by their kind's custom index, marked
        // outside, their first triangle past the view's in the record offset.
        for (built, slot, kind, first) in [
            (
                outside_bottom.is_some(),
                &tracer.outside_blas,
                INSTANCE_MODELS,
                triangle_count,
            ),
            (
                outside_cutout_bottom.is_some(),
                &tracer.outside_cutout_blas,
                INSTANCE_CUTOUT,
                cutout_count,
            ),
        ] {
            if built {
                let address = address_of(slot).expect("made above");
                let mut placed = instance([0.0; 3], kind | INSTANCE_OUTSIDE, MASK_OUTSIDE, address);
                placed.instance_shader_binding_table_record_offset_and_flags = vk::Packed24_8::new(
                    first,
                    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                );
                instances.push(placed);
            }
        }
        // The ground's tiles, each at its copy across the wrap nearest the
        // eye, its first triangle in the record offset (pt_trace.comp's
        // ground_surface); the patch round the eye in place of its tiles.
        if let (Some(_), Some(ground)) = (ground_mesh, tracer.ground.as_ref()) {
            let eye = feed.camera.eye;
            let patch = tracer
                .detail
                .as_ref()
                .zip(address_of(&tracer.detail_blas))
                .map(|(detail, address)| (detail.key.first, address));
            let covered = patch.map(|(first, _)| crate::scene_ground_detail::patch_tiles(first));
            for tile in 0..tile_count {
                let [first, count] = ground.source.tiles.ranges[tile];
                if count == 0 || covered.is_some_and(|covered| covered.contains(&tile)) {
                    continue;
                }
                let Some(address) = address_of(&tracer.ground_tiles[tile]) else {
                    continue;
                };
                let mut placed = instance(
                    crate::scene_ground_detail::tile_translation(tile, eye),
                    INSTANCE_GROUND,
                    MASK_SOLID | MASK_GROUND,
                    address,
                );
                placed.instance_shader_binding_table_record_offset_and_flags = vk::Packed24_8::new(
                    first,
                    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                );
                instances.push(placed);
            }
            if let Some((first, address)) = patch {
                instances.push(instance(
                    crate::scene_ground_detail::patch_translation(first, eye),
                    INSTANCE_DETAIL,
                    MASK_SOLID | MASK_GROUND,
                    address,
                ));
            }
        }
        let instance_bytes = bytes_of(&instances);
        self.grown(
            core,
            &mut tracer.instances[slot],
            instance_bytes.len() as u64,
            geometry,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.instances[slot], instance_bytes)?;
        let instances_address = Self::address(
            core,
            &tracer.instances[slot].as_ref().expect("grown").buffer,
        );
        let instance_data = vk::AccelerationStructureGeometryInstancesDataKHR::default().data(
            vk::DeviceOrHostAddressConstKHR {
                device_address: instances_address,
            },
        );
        let top_geometries = [vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                instances: instance_data,
            })];
        let top = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(&top_geometries);
        let instance_count = instances.len() as u32;
        let mut top_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        loader.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &top,
            &[instance_count],
            &mut top_sizes,
        );
        self.acceleration(
            core,
            &mut tracer.tlas,
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
            top_sizes.acceleration_structure_size,
        )?;
        let tlas = tracer.tlas.as_ref().expect("made above").handle;

        // One scratch buffer serves every build, one after the other; a new
        // ground's tiles build TILE_BATCH at a time, each in its own part.
        let alignment = tracer.scratch_alignment;
        let tile_step = tile_scratch.next_multiple_of(alignment);
        let scratch_size = ground_scratch
            .max(tile_step * TILE_BATCH as u64)
            .max(bottom_sizes.build_scratch_size)
            .max(water_sizes.build_scratch_size)
            .max(sky_sizes.build_scratch_size)
            .max(cutout_sizes.build_scratch_size)
            .max(glow_sizes.build_scratch_size)
            .max(outside_sizes.build_scratch_size)
            .max(outside_cutout_sizes.build_scratch_size)
            .max(top_sizes.build_scratch_size)
            + alignment;
        self.grown(
            core,
            &mut tracer.scratch,
            scratch_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            MemoryLocation::GpuOnly,
        )?;
        let scratch_address = Self::address(core, &tracer.scratch.as_ref().expect("grown").buffer)
            .next_multiple_of(alignment);
        let scratch = vk::DeviceOrHostAddressKHR {
            device_address: scratch_address,
        };
        let build = |info: vk::AccelerationStructureBuildGeometryInfoKHR,
                     target: vk::AccelerationStructureKHR,
                     count: u32| {
            let info = info
                .dst_acceleration_structure(target)
                .scratch_data(scratch);
            let range =
                [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(count)];
            loader.cmd_build_acceleration_structures(command, &[info], &[&range]);
            build_barrier(core, command);
        };
        for batch in tile_geometries.chunks(TILE_BATCH) {
            let infos: Vec<_> = batch
                .iter()
                .enumerate()
                .map(|(k, (tile, _, geometries))| {
                    slow_build(geometries)
                        .dst_acceleration_structure(
                            tracer.ground_tiles[*tile]
                                .as_ref()
                                .expect("made above")
                                .handle,
                        )
                        .scratch_data(vk::DeviceOrHostAddressKHR {
                            device_address: scratch_address + k as u64 * tile_step,
                        })
                })
                .collect();
            let ranges: Vec<[vk::AccelerationStructureBuildRangeInfoKHR; 1]> =
                batch
                    .iter()
                    .map(|(_, count, _)| {
                        [vk::AccelerationStructureBuildRangeInfoKHR::default()
                            .primitive_count(*count)]
                    })
                    .collect();
            let range_refs: Vec<&[vk::AccelerationStructureBuildRangeInfoKHR]> =
                ranges.iter().map(|range| &range[..]).collect();
            loader.cmd_build_acceleration_structures(command, &infos, &range_refs);
            build_barrier(core, command);
        }
        if !tile_geometries.is_empty() {
            tracer
                .ground
                .as_mut()
                .expect("uploaded above")
                .unbuilt
                .clear();
        }
        if let Some((count, geometries)) = &detail_geometries {
            build(
                slow_build(geometries),
                tracer.detail_blas.as_ref().expect("made above").handle,
                *count,
            );
            tracer.detail.as_mut().expect("uploaded above").built = true;
        }
        if let Some(info) = bottom {
            build(
                info,
                tracer.blas.as_ref().expect("made above").handle,
                triangle_count,
            );
        }
        if let Some(info) = water_bottom {
            build(
                info,
                tracer.water_blas.as_ref().expect("made above").handle,
                water_count,
            );
        }
        if let Some(info) = sky_bottom {
            build(
                info,
                tracer.sky_blas.as_ref().expect("made above").handle,
                sky_count,
            );
        }
        if let Some(info) = cutout_bottom {
            build(
                info,
                tracer.cutout_blas.as_ref().expect("made above").handle,
                cutout_count,
            );
        }
        if let Some(info) = glow_bottom {
            build(
                info,
                tracer.glow_blas.as_ref().expect("made above").handle,
                glow_count,
            );
        }
        if let Some(info) = outside_bottom {
            build(
                info,
                tracer.outside_blas.as_ref().expect("made above").handle,
                outside_count,
            );
        }
        if let Some(info) = outside_cutout_bottom {
            build(
                info,
                tracer
                    .outside_cutout_blas
                    .as_ref()
                    .expect("made above")
                    .handle,
                outside_cutout_count,
            );
        }
        build(top, tlas, instance_count);
        stamp(2);

        // Each blend rank's texture slot, for the ground.
        let blends: Vec<u32> = feed
            .terrain_textures
            .iter()
            .map(|texture| {
                texture
                    .and_then(|index| slots.get(index as usize).copied())
                    .unwrap_or(NO_TEXTURE)
            })
            .collect();
        let blend_bytes = bytes_of(&blends);
        self.grown(
            core,
            &mut tracer.blends[slot],
            blend_bytes.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.blends[slot], blend_bytes)?;

        // Retail's light grid, a signed RGB word a cell.
        let grid = feed.light_grid.as_ref();
        let mut grid_words: Vec<u32> = grid.map_or_else(Vec::new, |grid| {
            grid.cells
                .iter()
                .map(|rgb| u32::from_le_bytes([rgb[0] as u8, rgb[1] as u8, rgb[2] as u8, 0]))
                .collect()
        });
        // Thermal vision's heat memory behind the cells: what the warming
        // particles' lights (blasts, hits, muzzle flashes) put on the
        // ground, gathered every frame and cooling, then the cells round the
        // eye with their first cell's world cell.
        let eye = feed.camera.eye;
        let warm: Vec<super::heat_memory::WarmLight> = grid.map_or_else(Vec::new, |grid| {
            grid.sources
                .iter()
                .filter(|source| source.particle && source.heat > 0.0)
                .map(|source| super::heat_memory::WarmLight {
                    world: [
                        eye[0] + f64::from(source.position[0]),
                        eye[2] + f64::from(source.position[2]),
                    ],
                    radius: source.radius,
                    heat: source.heat,
                })
                .collect()
        });
        tracer
            .heat_memory
            .step(feed.clock_micros, thermal.memory, &warm);
        if feed.infrared {
            let (origin, cells) = tracer.heat_memory.window([eye[0], eye[2]]);
            grid_words.resize(GRID_CELLS, 0);
            grid_words.extend([origin[0] as u32, origin[1] as u32, 1, 0]);
            grid_words.extend(cells.iter().map(|heat| heat.to_bits()));
        }
        let grid_bytes = bytes_of(&grid_words);
        self.grown(
            core,
            &mut tracer.grid[slot],
            grid_bytes.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.grid[slot], grid_bytes)?;

        // The lights models were shaded with.
        let model_lights: Vec<[f32; 10]> = feed
            .model_lights
            .iter()
            .map(|light| {
                let mut words = [0.0; 10];
                words[..3].copy_from_slice(&light.ambient);
                words[3..6].copy_from_slice(&light.color);
                words[6..9].copy_from_slice(&light.direction);
                words[9] = light.own.clamp(0.0, 1.0);
                words
            })
            .collect();
        let model_light_bytes = bytes_of(&model_lights);
        self.grown(
            core,
            &mut tracer.model_lights[slot],
            model_light_bytes.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.model_lights[slot], model_light_bytes)?;

        // The blasts in the water throw it up, over the water's plane
        // (worlds without water hold it at -4096 or below), and the
        // splashes ring it.
        let water_plane = feed
            .terrain
            .as_deref()
            .map(|terrain| terrain.plane)
            .filter(|plane| *plane > -4096.0);
        let (spray, splashes) = water_splashes(&mut tracer.eruptions, feed, water_plane);
        let spray_bytes = bytes_of(&spray.records);
        self.grown(
            core,
            &mut tracer.spray[slot],
            spray_bytes.len().max(32) as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.spray[slot], spray_bytes)?;

        // The thrusters' trails, which the
        // mist carries; none under thermal vision.
        let trail_looks = &tuning.trails().looks;
        tracer.trails.update(
            feed.clock_micros,
            feed.camera.eye,
            &feed.exhausts,
            &feed.thrusts,
            trail_looks,
        );
        let trail_strength = if feed.infrared {
            0.0
        } else {
            lighting.trails.max(0.0)
        };
        let puffs = tracer.trails.records(
            feed.clock_micros,
            feed.camera.eye,
            trail_looks,
            trail_strength,
        );
        let mut low = [f32::MAX; 3];
        let mut high = [f32::MIN; 3];
        for puff in &puffs {
            for axis in 0..3 {
                low[axis] = low[axis].min(puff[axis] - puff[3]);
                high[axis] = high[axis].max(puff[axis] + puff[3]);
            }
        }
        let mut trail_records = vec![
            [puffs.len() as f32, 0.0, 0.0, 0.0],
            [low[0], low[1], low[2], 0.0],
            [high[0], high[1], high[2], 0.0],
        ];
        trail_records.extend(puffs.iter().flat_map(|puff| {
            [
                [puff[0], puff[1], puff[2], puff[3]],
                [puff[4], puff[5], puff[6], puff[7]],
            ]
        }));
        let trail_bytes = bytes_of(&trail_records);
        self.grown(
            core,
            &mut tracer.trail_puffs[slot],
            trail_bytes.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.trail_puffs[slot], trail_bytes)?;

        let splash_count = splashes.len();
        let splash_bytes = bytes_of(&splashes);
        self.grown(
            core,
            &mut tracer.splashes[slot],
            splash_bytes.len().max(4) as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.splashes[slot], splash_bytes)?;

        // The frame's deposited lights, the nearest first, for the ground's
        // shadowed light.
        let mut sources: Vec<[f32; 12]> = grid.map_or_else(Vec::new, |grid| {
            grid.sources
                .iter()
                .map(|source| {
                    let [x, y, z] = source.position;
                    let [r, g, b] = source.rgb;
                    let particle = if source.particle { 1.0 } else { 0.0 };
                    let [ex, ey, ez] = source.emitter;
                    [
                        x,
                        y,
                        z,
                        source.radius,
                        r,
                        g,
                        b,
                        particle,
                        ex,
                        ey,
                        ez,
                        source.heat,
                    ]
                })
                .collect()
        });
        // The glowing models' lights (kind 2), in their sprites' mean colours
        // (each sprite's by how much light its faces hold), as bright as
        // those sprites hold light (the tokens glow, but not
        // constantly, rather in a pulse): a token's core shrinks and
        // swells.
        for glow in &feed.glows {
            let mut tint = [0.0_f32; 3];
            let mut held = 0.0_f32;
            let mut faces = 0.0_f32;
            for &(index, count) in glow.sprites.iter().filter(|(_, count)| *count > 0) {
                let Some(texture) = textures.get(index as usize) else {
                    continue;
                };
                let key = texture.content_key();
                let mean = *tracer
                    .texture_means
                    .entry(key)
                    .or_insert_with(|| texture.mean_colour());
                let energy = *tracer
                    .texture_energy
                    .entry(key)
                    .or_insert_with(|| sprite_energy(texture));
                let light = f32::from(count) * energy;
                for (sum, channel) in tint.iter_mut().zip(mean) {
                    *sum += channel * light;
                }
                held += light;
                faces += f32::from(count);
            }
            let brightest = tint.iter().copied().fold(0.0, f32::max);
            let mean = if brightest > 0.0 {
                tint.map(|channel| channel / brightest)
            } else {
                [1.0; 3]
            };
            let energy = if faces > 0.0 { held / faces } else { 1.0 };
            // A glow shines as much as its sprites hold light against a
            // crystal's (GLOW_ENERGY); the shots' and the tinted ones keep
            // the tuning's strength.
            let shine = if faces > 0.0 && glow.model != crate::world_scene::SHOT_GLOWS {
                energy / GLOW_ENERGY
            } else {
                1.0
            };
            let [x, y, z] = glow.position;
            let [ex, ey, ez] = glow.end.unwrap_or(glow.position);
            let [r, g, b] = std::array::from_fn(|channel| {
                mean[channel] * f32::from(glow.color[channel]) / 255.0
                    * glow.strength
                    * shine
                    * lighting.glows.max(0.0)
            });
            // The guns' shots warm what they light under thermal vision.
            let heat = if glow.model == crate::world_scene::SHOT_GLOWS {
                glow.strength.min(1.0)
            } else {
                0.0
            };
            sources.push([x, y, z, glow.reach, r, g, b, 2.0, ex, ey, ez, heat]);
        }
        let source_dist2 = |s: &[f32; 12]| {
            let [dx, dz] = [s[8] - s[0], s[10] - s[2]];
            let len2 = dx * dx + dz * dz;
            let t = if len2 > 0.0 && s[7] > 1.5 {
                (-(s[0] * dx + s[2] * dz) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let [px, pz] = [s[0] + dx * t, s[2] + dz * t];
            px * px + pz * pz
        };
        sources.sort_by(|a, b| source_dist2(a).total_cmp(&source_dist2(b)));
        sources.truncate(LIGHT_SOURCES);
        let source_count = sources.len();
        let source_bytes = bytes_of(&sources);
        self.grown(
            core,
            &mut tracer.sources[slot],
            source_bytes.len().max(4) as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.sources[slot], source_bytes)?;

        // The blended sprites and beam segments that add light, the nearest
        // first, which the water, puddles and metal mirror at the lighting
        // preset's share (none under thermal vision).
        let effect_mirrors = if feed.infrared {
            0.0
        } else {
            lighting.effects.max(0.0)
        };
        // Under the water nothing mirrors it; a round billboard lying on the
        // water (the spray and wake round a craft) would mirror onto itself,
        // where the raster already draws it.
        let under_water = |emitter: &crate::scene_feed::SceneEmitter| {
            water_plane.is_some_and(|plane| {
                let mid_y = emitter.position[1] + 0.5 * emitter.span[1];
                let height = feed.camera.eye[1] + f64::from(mid_y) - f64::from(plane);
                let round = emitter.span == [0.0; 3];
                height
                    < if round {
                        f64::from(emitter.radius)
                    } else {
                        0.0
                    }
            })
        };
        let mut emitters: Vec<[f32; 12]> = if effect_mirrors > 0.0 {
            feed.emitters
                .iter()
                .filter(|emitter| !under_water(emitter))
                .filter_map(|emitter| {
                    let slot = emitter
                        .texture
                        .and_then(|index| slots.get(index as usize).copied())
                        .filter(|slot| *slot != NO_TEXTURE)?;
                    let [x, y, z] = emitter.position;
                    let [sx, sy, sz] = emitter.span;
                    let [r, g, b] = emitter.color.map(|channel| f32::from(channel) / 255.0);
                    Some([
                        x,
                        y,
                        z,
                        emitter.radius,
                        r,
                        g,
                        b,
                        slot as f32,
                        sx,
                        sy,
                        sz,
                        0.0,
                    ])
                })
                .collect()
        } else {
            Vec::new()
        };
        let emitter_dist2 = |e: &[f32; 12]| {
            let [sx, sy, sz] = [e[8], e[9], e[10]];
            let len2 = sx * sx + sy * sy + sz * sz;
            let t = if len2 > 0.0 {
                (-(e[0] * sx + e[1] * sy + e[2] * sz) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let [px, py, pz] = [e[0] + sx * t, e[1] + sy * t, e[2] + sz * t];
            px * px + py * py + pz * pz
        };
        emitters.sort_by(|a, b| emitter_dist2(a).total_cmp(&emitter_dist2(b)));
        emitters.truncate(EMITTERS);
        let emitter_count = emitters.len();

        let emitter_bytes = bytes_of(&emitters);
        self.grown(
            core,
            &mut tracer.emitters[slot],
            emitter_bytes.len().max(48) as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.emitters[slot], emitter_bytes)?;

        // The frame block: the view, the light, and the last frame's view,
        // which the bounce light's history follows when the size and the
        // projection stayed.
        // The render size: the display's, or smaller for an upscaler, the
        // primary rays then jittered so frames sample between pixels.
        let display = target.color.extent;
        // An upscaler picks its own render size; its jitter runs at any.
        // Under infrared none: the camera's sensor is coarser than any render
        // size, and the heat has no noise to reconstruct.
        let modern = self.modern;
        let choice = if feed.infrared || tracer.failed_upscalers.contains(&modern.upscaler) {
            UpscalerChoice::Off
        } else {
            UpscalerChoice::from_settings(modern)
        };
        let upscale_extent = match Upscaler::render_size(core, choice, display) {
            Ok(extent) => extent,
            Err(error) => {
                tracer.upscaler_unavailable(modern.upscaler, &error);
                None
            }
        };
        let extent = upscale_extent.unwrap_or_else(|| {
            if feed.infrared {
                // Thermal vision's camera has a lower resolution of its own.
                display.map(|side| side.div_ceil(2))
            } else {
                render_extent(display)
            }
        });
        let smaller = extent != display;
        let jittered = smaller
            || upscale_extent.is_some()
            || modern.anti_aliasing == crate::modern_settings::AntiAliasing::Taa;
        let began = std::time::Instant::now();
        let frame_ms = tracer
            .last_frame
            .replace(began)
            .map_or(1000.0 / 60.0, |then| (began - then).as_secs_f32() * 1000.0);
        let jitter = if jittered {
            let ratio = display[0] as f32 / extent[0] as f32;
            let phases = (8.0 * ratio * ratio).ceil() as u32;
            let index = tracer.jitter_index % phases.max(1) + 1;
            tracer.jitter_index = tracer.jitter_index.wrapping_add(1);
            [halton(index, 2) - 0.5, halton(index, 3) - 0.5]
        } else {
            [0.0; 2]
        };
        let camera = &feed.camera;
        let view = PreviousView::of(camera, extent);
        let water_texture = feed
            .water_texture
            .and_then(|index| slots.get(index as usize).copied())
            .filter(|slot| *slot != NO_TEXTURE)
            .map_or(-1.0, |slot| slot as f32);
        let previous = tracer
            .previous
            .filter(|previous| previous.extent == extent && previous.native == camera.native)
            .filter(|_| tracer.thermal_ran == feed.infrared);
        let ray_reconstruction = matches!(choice, UpscalerChoice::DlssRr(_));
        let FrameBlock {
            block,
            view,
            previous,
            distance_fog,
            mist,
            volume,
            modern_sky,
            clouded,
            ..
        } = frame_block(
            feed,
            &FrameShape {
                display,
                extent,
                upscaled: upscale_extent.is_some(),
                ray_reconstruction,
                jitter,
                view,
                previous,
                frame_index: tracer.frame_index,
                water_count,
                sky_count,
                cutout_first,
                glow_first,
                source_count,
                splash_count,
                spray_sources: spray.sources,
                emitter_count,
                emissive_groups: emissive.sampled,
                effect_mirrors,
                water_texture,
                water_hue: feed
                    .water_texture
                    .and_then(|index| textures.get(index as usize))
                    .map_or([0.3, 0.5, 0.55], |texture| texture.mean_colour()),
                grid,
                world_view: feed.terrain.is_some(),
            },
            self.modern,
        );
        self.grown(
            core,
            &mut tracer.frame[slot],
            std::mem::size_of_val(&block) as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Self::write(&mut tracer.frame[slot], bytes_of(&block))?;
        // The preset's grade waits for the world's last draw (the raster's
        // effects over the traced world), before the HUD.
        self.graded = (!feed.infrared && lighting.grades()).then_some(lighting);
        // The raster's late draws over this world take its distance fog.
        self.traced_fog = distance_fog;
        // Thermal vision sees through the raster's smoke by half.
        self.thermal_effects = feed.infrared;
        // The modern sky's clouds, cube map and shadow map, which the trace
        // and the composite read.
        let mut sky = match self.sky.take() {
            Some(sky) => sky,
            None => Box::new(super::sky::Sky::new(core)?),
        };
        let sky_views = sky.record(
            self,
            core,
            command,
            tracer.frame[slot]
                .as_ref()
                .expect("written above")
                .buffer
                .buffer,
            display,
            modern_sky,
            clouded,
            true,
        );
        self.sky = Some(sky);
        let sky_views = sky_views?;
        stamp(3);
        // The images, at the target's size, all kept in the general layout.
        self.traced_image(core, &mut tracer.albedo, COLOR_IMAGE, extent)?;
        self.traced_image(core, &mut tracer.direct, LIGHT_IMAGE, extent)?;
        self.traced_image(core, &mut tracer.bounce, LIGHT_IMAGE, extent)?;
        self.traced_image(core, &mut tracer.fog, COLOR_IMAGE, extent)?;
        self.traced_image(core, &mut tracer.depth, TRACED_DEPTH, extent)?;
        for slot in tracer
            .geometry
            .iter_mut()
            .chain(tracer.history.iter_mut())
            .chain(tracer.filtered.iter_mut())
            .chain(std::iter::once(&mut tracer.sun))
            .chain(std::iter::once(&mut tracer.object_motion))
            .chain(std::iter::once(&mut tracer.path_surface))
            .chain(std::iter::once(&mut tracer.specular))
            .chain(tracer.glossy.iter_mut())
            .chain(tracer.glossy_filtered.iter_mut())
        {
            self.traced_image(core, slot, LIGHT_IMAGE, extent)?;
        }
        for slot in tracer
            .shadows
            .iter_mut()
            .chain(tracer.glossy_track.iter_mut())
            .chain(tracer.lag_record.iter_mut())
        {
            self.traced_image(core, slot, MOTION_IMAGE, extent)?;
        }
        self.traced_image(core, &mut tracer.glossy_hit, vk::Format::R16_SFLOAT, extent)?;
        self.traced_image(core, &mut tracer.color, LIGHT_IMAGE, extent)?;
        self.traced_image(core, &mut tracer.motion, MOTION_IMAGE, extent)?;
        // The mist's cells, their slices stacked down one image.
        let mist_extent = [volume[0], volume[1] * volume[2]];
        for slot in tracer.mist_cells.iter_mut() {
            self.traced_image(core, slot, LIGHT_IMAGE, mist_extent)?;
        }
        self.traced_image(core, &mut tracer.mist_gathered, LIGHT_IMAGE, mist_extent)?;
        let heat_extent = if feed.infrared { extent } else { [1, 1] };
        self.traced_image(core, &mut tracer.heat, TRACED_DEPTH, heat_extent)?;
        // The camera's range lives on the device between frames.
        self.grown(
            core,
            &mut tracer.gain_range,
            16,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        // The paths' blue noise, uploaded once.
        if tracer.blue_noise.is_none() {
            tracer.blue_noise = Some(self.device_buffer(
                core,
                super::blue_noise::BLUE_NOISE,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            upload_barrier(core, command);
        }
        // The radiance cache, empty when made and when the world changes: its
        // cells sit on the world's grid, and another world's would light
        // this one.
        let made_cache = tracer.radiance_cache.is_none();
        self.grown(
            core,
            &mut tracer.radiance_cache,
            CACHE_BYTES,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuOnly,
        )?;
        if made_cache || tracer.cache_world != Some(feed.world) {
            let cache = &tracer.radiance_cache.as_ref().expect("grown above").buffer;
            core.device
                .cmd_fill_buffer(command, cache.buffer, 0, vk::WHOLE_SIZE, 0);
            let cleared = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::CLEAR)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_access_mask(
                    vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                );
            core.device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&cleared)),
            );
            tracer.cache_world = Some(feed.world);
        }
        if smaller {
            self.traced_image(core, &mut tracer.display_depth, TRACED_DEPTH, display)?;
        }
        // The upscaler for this choice and these sizes, and its output.
        let mut upscaling = false;
        if upscale_extent.is_some() {
            if tracer
                .upscaler
                .as_ref()
                .is_some_and(|upscaler| !upscaler.fits(choice, display, extent))
            {
                if let Some(old) = tracer.upscaler.take() {
                    old.destroy(core);
                }
            }
            if tracer.upscaler.is_none() {
                match Upscaler::new(core, command, choice, display, extent) {
                    Ok(upscaler) => tracer.upscaler = Some(upscaler),
                    Err(error) => tracer.upscaler_unavailable(modern.upscaler, &error),
                }
            }
            if tracer.upscaler.is_some() {
                self.upscaled_image(core, &mut tracer.upscaled, display)?;
                upscaling = true;
            }
        }
        // Without its upscaler the guides shrink to a pixel, which the
        // resolve leaves alone, and the denoiser runs.
        let ray_reconstruction = ray_reconstruction && upscaling;
        let guide_extent = if ray_reconstruction { extent } else { [1, 1] };
        // The traced world's own anti-aliasing pass, when no upscaler brings
        // one (supersampling enlarged the whole frame instead).
        let anti_aliasing = if upscaling {
            crate::modern_settings::AntiAliasing::Off
        } else {
            modern.anti_aliasing.pass()
        };
        let aa_extent = if anti_aliasing == crate::modern_settings::AntiAliasing::Off {
            [1, 1]
        } else {
            extent
        };
        for slot in tracer.aa_history.iter_mut() {
            self.traced_image(core, slot, LIGHT_IMAGE, aa_extent)?;
        }
        for (slot, format) in tracer
            .guides
            .iter_mut()
            .zip([COLOR_IMAGE, LIGHT_IMAGE, COLOR_IMAGE])
        {
            self.traced_image(core, slot, format, guide_extent)?;
        }
        fn made(slot: &Option<Image>) -> &Image {
            slot.as_ref().expect("made above")
        }
        let [now, before] = [tracer.parity, tracer.parity ^ 1];
        let albedo = made(&tracer.albedo);
        let direct = made(&tracer.direct);
        let bounce = made(&tracer.bounce);
        let fog = made(&tracer.fog);
        let depth = made(&tracer.depth);
        let geometry = made(&tracer.geometry[now]);
        let previous_geometry = made(&tracer.geometry[before]);
        let history = made(&tracer.history[now]);
        let previous_history = made(&tracer.history[before]);
        let filtered = [made(&tracer.filtered[0]), made(&tracer.filtered[1])];
        let [shadows, previous_shadows] =
            [made(&tracer.shadows[now]), made(&tracer.shadows[before])];
        let [lag_record, previous_lag_record] = [
            made(&tracer.lag_record[now]),
            made(&tracer.lag_record[before]),
        ];
        let glossy_hit = made(&tracer.glossy_hit);
        let [glossy, previous_glossy] = [made(&tracer.glossy[now]), made(&tracer.glossy[before])];
        let glossy_filtered = [
            made(&tracer.glossy_filtered[0]),
            made(&tracer.glossy_filtered[1]),
        ];
        let [glossy_track, previous_glossy_track] = [
            made(&tracer.glossy_track[now]),
            made(&tracer.glossy_track[before]),
        ];
        let sun = made(&tracer.sun);
        let object_motion = made(&tracer.object_motion);
        let color = made(&tracer.color);
        let motion = made(&tracer.motion);
        let display_depth = smaller.then(|| made(&tracer.display_depth));
        let upscaled = upscaling.then(|| made(&tracer.upscaled));
        let guides = tracer.guides.each_ref().map(made);
        let aa_history = [
            made(&tracer.aa_history[now]),
            made(&tracer.aa_history[before]),
        ];
        let aa_output =
            (anti_aliasing != crate::modern_settings::AntiAliasing::Off).then_some(aa_history[0]);
        let mist_cells = [
            made(&tracer.mist_cells[now]),
            made(&tracer.mist_cells[before]),
        ];
        let mist_gathered = made(&tracer.mist_gathered);
        let heat = made(&tracer.heat);
        for image in [color, motion, mist_gathered, heat]
            .into_iter()
            .chain(display_depth)
            .chain(upscaled)
            .chain(guides)
            .chain(aa_history)
            .chain(mist_cells)
        {
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }
        for image in [
            albedo,
            direct,
            bounce,
            fog,
            depth,
            geometry,
            previous_geometry,
            history,
            previous_history,
            filtered[0],
            filtered[1],
            shadows,
            previous_shadows,
            lag_record,
            previous_lag_record,
            glossy_hit,
            glossy,
            previous_glossy,
            glossy_filtered[0],
            glossy_filtered[1],
            glossy_track,
            previous_glossy_track,
            sun,
            object_motion,
            made(&tracer.path_surface),
            made(&tracer.specular),
        ] {
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }

        // This frame's descriptor sets; the last frame's work has finished.
        core.device
            .reset_descriptor_pool(tracer.pool[slot], vk::DescriptorPoolResetFlags::empty())
            .map_err(call("tracer descriptor reset"))?;
        let counts = [images.len().max(1) as u32];
        let mut variable = vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
            .descriptor_counts(&counts);
        let layouts = [tracer.set_layout];
        let set = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(tracer.pool[slot])
                    .set_layouts(&layouts)
                    .push_next(&mut variable),
            )
            .map_err(call("tracer descriptor set"))?[0];
        let pass_layouts = [
            tracer.temporal_set_layout,
            tracer.atrous_set_layout,
            tracer.resolve_set_layout,
            tracer.composite_set_layout,
            tracer.aa_set_layout,
            tracer.mist_set_layout,
            tracer.gather_set_layout,
            tracer.gain_set_layout,
            tracer.cache_set_layout,
        ];
        let [temporal_set, atrous_set, resolve_set, composite_set, aa_set, mist_set, gather_set, gain_set, cache_set]: [vk::DescriptorSet;
            9] = core
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(tracer.pool[slot])
                    .set_layouts(&pass_layouts),
            )
            .map_err(call("denoiser descriptor sets"))?
            .try_into()
            .expect("nine layouts give nine sets");
        tracer.composite_frame_set = composite_set;
        let structures = [tlas];
        let mut structure_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&structures);
        let buffer_info = |grown: &Option<Grown>| {
            [vk::DescriptorBufferInfo::default()
                .buffer(grown.as_ref().expect("grown").buffer.buffer)
                .range(vk::WHOLE_SIZE)]
        };
        let vertices_info = buffer_info(&tracer.vertices[slot]);
        let indices_info = buffer_info(&tracer.indices[slot]);
        let triangles_info = buffer_info(&tracer.triangles[slot]);
        let frame_info = buffer_info(&tracer.frame[slot]);
        let blends_info = buffer_info(&tracer.blends[slot]);
        let grid_info = buffer_info(&tracer.grid[slot]);
        let model_lights_info = buffer_info(&tracer.model_lights[slot]);
        let gain_info = buffer_info(&tracer.gain_range);
        let cache_info = buffer_info(&tracer.radiance_cache);
        let splashes_info = buffer_info(&tracer.splashes[slot]);
        let sources_info = buffer_info(&tracer.sources[slot]);
        let spray_info = buffer_info(&tracer.spray[slot]);
        let trail_info = buffer_info(&tracer.trail_puffs[slot]);
        let emitters_info = buffer_info(&tracer.emitters[slot]);
        let emissive_faces_info = buffer_info(&tracer.emissive_faces[slot]);
        let emissive_groups_info = buffer_info(&tracer.emissive_groups[slot]);
        let triangle_emitters_info = buffer_info(&tracer.triangle_emitters[slot]);
        let whole = |buffer: &Buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer.buffer)
                .range(vk::WHOLE_SIZE)]
        };
        let blue_noise_info = whole(tracer.blue_noise.as_ref().expect("uploaded"));
        // Without ground instances nothing reads the ground's bindings: the
        // models' buffers stand in.
        let (ground_vertices_info, ground_indices_info, ground_triangles_info) =
            match tracer.ground.as_ref() {
                Some(ground) => (
                    whole(&ground.vertices),
                    whole(&ground.indices),
                    whole(&ground.triangles),
                ),
                None => (vertices_info, indices_info, triangles_info),
            };
        let (detail_vertices_info, detail_indices_info, detail_triangles_info) =
            match tracer.detail.as_ref() {
                Some(detail) => (
                    whole(&detail.vertices),
                    whole(&detail.indices),
                    whole(&detail.triangles),
                ),
                None => (vertices_info, indices_info, triangles_info),
            };
        let storage = |image: &Image| {
            [vk::DescriptorImageInfo::default()
                .image_view(image.view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        let [albedo_info, direct_info, bounce_info, fog_info, depth_info, geometry_info] =
            [albedo, direct, bounce, fog, depth, geometry].map(storage);
        let [history_info, previous_history_info, previous_geometry_info] =
            [history, previous_history, previous_geometry].map(storage);
        let [shadows_info, previous_shadows_info, sun_info, object_motion_info] =
            [shadows, previous_shadows, sun, object_motion].map(storage);
        let [lag_record_info, previous_lag_record_info] =
            [lag_record, previous_lag_record].map(storage);
        let [glossy_hit_info, glossy_info, previous_glossy_info] =
            [glossy_hit, glossy, previous_glossy].map(storage);
        let [glossy_track_info, previous_glossy_track_info] =
            [glossy_track, previous_glossy_track].map(storage);
        let path_surface_info = storage(made(&tracer.path_surface));
        let specular_info = storage(made(&tracer.specular));
        let layer = |image: &Image| {
            vk::DescriptorImageInfo::default()
                .image_view(image.view)
                .image_layout(vk::ImageLayout::GENERAL)
        };
        let layers_info = [history, filtered[0], filtered[1]].map(layer);
        let glossy_layers_info = [glossy, glossy_filtered[0], glossy_filtered[1]].map(layer);
        let glossy_footprint_info = storage(glossy_filtered[1]);
        let sampled = |image: &Image, sampler: usize| {
            [vk::DescriptorImageInfo::default()
                .sampler(core.samplers[sampler])
                .image_view(image.view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        // The colour comes up smoothly from a smaller render; the depth is
        // the display's own, read as it is.
        let composite_infos = [
            sampled(upscaled.or(aa_output).unwrap_or(color), SMOOTH_SAMPLER),
            sampled(display_depth.unwrap_or(depth), BLIT_SAMPLER),
            sampled(mist_gathered, SMOOTH_SAMPLER),
        ];
        let sky_info = |view: vk::ImageView| {
            [vk::DescriptorImageInfo::default()
                .sampler(core.samplers[SMOOTH_SAMPLER])
                .image_view(view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        let [sky_clouds_info, sky_away_info, sky_shadow_info] =
            [sky_views.clouds, sky_views.away, sky_views.shadow].map(sky_info);
        // The cube with its levels, for rough reflections and the sky's light.
        let sky_cube_info = [vk::DescriptorImageInfo::default()
            .sampler(sky_views.cube_sampler)
            .image_view(sky_views.cube)
            .image_layout(vk::ImageLayout::GENERAL)];
        let mist_history_info = sampled(mist_cells[1], SMOOTH_SAMPLER);
        let [mist_cells_info, mist_gathered_info, heat_info] =
            [mist_cells[0], mist_gathered, heat].map(storage);
        let mut composite_structure = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&structures);
        let mut mist_structure = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&structures);
        let [filtered_info, color_info, motion_info] = [filtered[0], color, motion].map(storage);
        let guide_infos = guides.map(storage);
        let aa_infos = aa_history.map(storage);
        let display_depth_info = storage(display_depth.unwrap_or(depth));
        let write = |set: vk::DescriptorSet, binding: u32, kind: vk::DescriptorType| {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .descriptor_type(kind)
        };
        let buffer = vk::DescriptorType::STORAGE_BUFFER;
        let image = vk::DescriptorType::STORAGE_IMAGE;
        let uniform = vk::DescriptorType::UNIFORM_BUFFER;
        let mut writes = vec![
            write(set, 0, vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .push_next(&mut structure_write),
            write(set, 1, buffer).buffer_info(&vertices_info),
            write(set, 2, buffer).buffer_info(&indices_info),
            write(set, 3, buffer).buffer_info(&triangles_info),
            write(set, 4, uniform).buffer_info(&frame_info),
            write(set, 5, image).image_info(&albedo_info),
            write(set, 6, image).image_info(&depth_info),
            write(set, 7, buffer).buffer_info(&ground_vertices_info),
            write(set, 8, buffer).buffer_info(&ground_indices_info),
            write(set, 9, buffer).buffer_info(&ground_triangles_info),
            write(set, 10, buffer).buffer_info(&blends_info),
            write(set, 11, buffer).buffer_info(&grid_info),
            write(set, 12, buffer).buffer_info(&model_lights_info),
            write(set, 13, image).image_info(&direct_info),
            write(set, 14, image).image_info(&bounce_info),
            write(set, 15, image).image_info(&fog_info),
            write(set, 16, image).image_info(&geometry_info),
            write(set, 17, image).image_info(&heat_info),
            write(set, 18, buffer).buffer_info(&splashes_info),
            write(set, 19, buffer).buffer_info(&sources_info),
            write(set, 20, image).image_info(&display_depth_info),
            write(set, 21, image).image_info(&sun_info),
            write(set, 22, buffer).buffer_info(&spray_info),
            write(set, 25, image).image_info(&object_motion_info),
            write(set, 27, image).image_info(&path_surface_info),
            write(set, 32, image).image_info(&specular_info),
            write(set, 36, image).image_info(&glossy_hit_info),
            write(set, 26, buffer).buffer_info(&emitters_info),
            write(set, 33, buffer).buffer_info(&emissive_faces_info),
            write(set, 34, buffer).buffer_info(&emissive_groups_info),
            write(set, 35, buffer).buffer_info(&triangle_emitters_info),
            write(set, 37, buffer).buffer_info(&blue_noise_info),
            write(set, 28, buffer).buffer_info(&detail_vertices_info),
            write(set, 29, buffer).buffer_info(&detail_indices_info),
            write(set, 30, buffer).buffer_info(&detail_triangles_info),
            write(temporal_set, 0, uniform).buffer_info(&frame_info),
            write(temporal_set, 1, image).image_info(&bounce_info),
            write(temporal_set, 2, image).image_info(&geometry_info),
            write(temporal_set, 3, image).image_info(&previous_history_info),
            write(temporal_set, 4, image).image_info(&previous_geometry_info),
            write(temporal_set, 5, image).image_info(&history_info),
            write(temporal_set, 6, image).image_info(&previous_shadows_info),
            write(temporal_set, 7, image).image_info(&shadows_info),
            write(temporal_set, 8, image).image_info(&direct_info),
            write(temporal_set, 9, image).image_info(&object_motion_info),
            write(temporal_set, 10, image).image_info(&path_surface_info),
            write(temporal_set, 11, image).image_info(&previous_lag_record_info),
            write(temporal_set, 12, image).image_info(&lag_record_info),
            write(temporal_set, 13, image).image_info(&glossy_hit_info),
            write(temporal_set, 14, image).image_info(&previous_glossy_info),
            write(temporal_set, 15, image).image_info(&glossy_info),
            write(temporal_set, 16, image).image_info(&glossy_footprint_info),
            write(temporal_set, 17, image).image_info(&previous_glossy_track_info),
            write(temporal_set, 18, image).image_info(&glossy_track_info),
            write(atrous_set, 0, uniform).buffer_info(&frame_info),
            write(atrous_set, 1, image).image_info(&layers_info),
            write(atrous_set, 2, image).image_info(&geometry_info),
            write(atrous_set, 3, image).image_info(&shadows_info),
            write(atrous_set, 4, image).image_info(&glossy_layers_info),
            write(atrous_set, 5, image).image_info(&path_surface_info),
            write(resolve_set, 0, uniform).buffer_info(&frame_info),
            write(resolve_set, 1, image).image_info(&albedo_info),
            write(resolve_set, 2, image).image_info(&direct_info),
            // Ray Reconstruction denoises the bounce light itself.
            write(resolve_set, 3, image).image_info(if ray_reconstruction {
                &bounce_info
            } else {
                &filtered_info
            }),
            write(resolve_set, 4, image).image_info(&fog_info),
            write(resolve_set, 5, image).image_info(&geometry_info),
            write(resolve_set, 6, image).image_info(&color_info),
            write(resolve_set, 7, image).image_info(&motion_info),
            write(resolve_set, 8, image).image_info(&guide_infos[0]),
            write(resolve_set, 9, image).image_info(&guide_infos[1]),
            write(resolve_set, 10, image).image_info(&guide_infos[2]),
            write(resolve_set, 11, image).image_info(&heat_info),
            write(resolve_set, 12, buffer).buffer_info(&gain_info),
            write(resolve_set, 13, image).image_info(&sun_info),
            write(resolve_set, 15, image).image_info(&specular_info),
            write(resolve_set, 16, image).image_info(&path_surface_info),
            write(resolve_set, 14, image).image_info(&object_motion_info),
            write(aa_set, 0, uniform).buffer_info(&frame_info),
            write(aa_set, 1, image).image_info(&color_info),
            write(aa_set, 2, image).image_info(&motion_info),
            write(aa_set, 3, image).image_info(&aa_infos[1]),
            write(aa_set, 4, image).image_info(&aa_infos[0]),
            write(mist_set, 0, uniform).buffer_info(&frame_info),
            write(mist_set, 1, vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .push_next(&mut mist_structure),
            write(mist_set, 2, buffer).buffer_info(&ground_vertices_info),
            write(mist_set, 3, buffer).buffer_info(&grid_info),
            write(mist_set, 4, vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&mist_history_info),
            write(mist_set, 5, image).image_info(&mist_cells_info),
            write(mist_set, 6, buffer).buffer_info(&trail_info),
            write(gather_set, 0, uniform).buffer_info(&frame_info),
            write(gather_set, 1, image).image_info(&mist_cells_info),
            write(gather_set, 2, image).image_info(&mist_gathered_info),
            write(composite_set, 3, uniform).buffer_info(&frame_info),
            write(
                composite_set,
                6,
                vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            )
            .descriptor_count(1)
            .push_next(&mut composite_structure),
            write(cache_set, 0, buffer).buffer_info(&cache_info),
            write(set, 31, buffer).buffer_info(&cache_info),
            write(gain_set, 0, uniform).buffer_info(&frame_info),
            write(gain_set, 1, image).image_info(&heat_info),
            write(gain_set, 2, buffer).buffer_info(&gain_info),
            write(gain_set, 3, image).image_info(&depth_info),
            write(set, 23, vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&sky_cube_info),
            write(set, 24, vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&sky_shadow_info),
            write(composite_set, 4, vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&sky_clouds_info),
            write(composite_set, 5, vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&sky_away_info),
        ];
        for (binding, info) in composite_infos.iter().enumerate() {
            writes.push(
                write(
                    composite_set,
                    binding as u32,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                )
                .image_info(info),
            );
        }
        if !images.is_empty() {
            writes.push(
                write(
                    set,
                    TEXTURE_BINDING,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                )
                .image_info(&images),
            );
        }
        core.device.update_descriptor_sets(&writes, &[]);

        // The trace, then the bounce light's history and its filter.
        let groups = [extent[0].div_ceil(8), extent[1].div_ceil(8)];
        let run = |pipeline: vk::Pipeline, layout: vk::PipelineLayout, set: vk::DescriptorSet| {
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
        };
        run(tracer.pipeline, tracer.layout, set);
        core.device.cmd_dispatch(command, groups[0], groups[1], 1);
        pass_barrier(core, command);
        // The bounce light, in a pass of its own: at one pixel of each two
        // by two (the one turning each frame), which the temporal pass fills
        // out, or at every pixel for Ray Reconstruction, which takes the
        // samples as they are.
        if !feed.infrared {
            // As the frame block's `guides` says, which the pass reads.
            let paths = if matches!(choice, UpscalerChoice::DlssRr(_)) {
                extent
            } else {
                extent.map(|side| side.div_ceil(2))
            };
            run(tracer.paths, tracer.layout, set);
            core.device
                .cmd_dispatch(command, paths[0].div_ceil(8), paths[1].div_ceil(8), 1);
            pass_barrier(core, command);
            // The emitters and the lava as lights for those bounces, while
            // the frame has any (as the frame block's emissive groups and
            // water glow say, which the passes read).
            if emissive.sampled > 0 || tuning.water(feed.world).glow > 0.0 {
                run(tracer.emitters_pass, tracer.layout, set);
                core.device
                    .cmd_dispatch(command, paths[0].div_ceil(8), paths[1].div_ceil(8), 1);
                pass_barrier(core, command);
            }
            // The radiance cache takes this frame's samples.
            run(tracer.cache_resolve, tracer.cache_layout, cache_set);
            core.device
                .cmd_dispatch(command, (CACHE_SLOTS / 256) as u32, 1, 1);
            pass_barrier(core, command);
        }
        stamp(4);
        if feed.infrared {
            run(tracer.gain, tracer.gain_layout, gain_set);
            let fresh = i32::from(!tracer.thermal_ran);
            let [low, high] = thermal.gain;
            let mut push = [0_u8; 12];
            push[0..4].copy_from_slice(&fresh.to_le_bytes());
            push[4..8].copy_from_slice(&low.clamp(0.0, 0.5).to_le_bytes());
            push[8..12].copy_from_slice(&high.clamp(0.5, 1.0).to_le_bytes());
            core.device.cmd_push_constants(
                command,
                tracer.gain_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &push,
            );
            core.device.cmd_dispatch(command, 1, 1, 1);
            pass_barrier(core, command);
        }
        tracer.thermal_ran = feed.infrared;
        if !ray_reconstruction && !feed.infrared {
            // The diffuse history, then the glossy lobe's beside it, which
            // marks the pixels that have one.
            for pipeline in [tracer.temporal, tracer.temporal_glossy] {
                run(pipeline, tracer.temporal_layout, temporal_set);
                core.device.cmd_dispatch(command, groups[0], groups[1], 1);
                pass_barrier(core, command);
            }
            // Each step's diffuse pass, then before the last its glossy one.
            for pass in ATROUS_PASSES {
                let pipelines: &[vk::Pipeline] = if pass[3] == 0 {
                    &[tracer.atrous, tracer.atrous_glossy]
                } else {
                    &[tracer.atrous]
                };
                for &pipeline in pipelines {
                    run(pipeline, tracer.atrous_layout, atrous_set);
                    core.device.cmd_push_constants(
                        command,
                        tracer.atrous_layout,
                        vk::ShaderStageFlags::COMPUTE,
                        0,
                        bytes_of(&pass),
                    );
                    core.device.cmd_dispatch(command, groups[0], groups[1], 1);
                    pass_barrier(core, command);
                }
            }
        }
        stamp(5);
        // The colour and its motion at the render size, then the display's
        // depth when the tracer renders smaller.
        run(tracer.resolve, tracer.resolve_layout, resolve_set);
        core.device.cmd_dispatch(command, groups[0], groups[1], 1);
        pass_barrier(core, command);
        if smaller {
            run(tracer.depth_pass, tracer.layout, set);
            core.device
                .cmd_dispatch(command, display[0].div_ceil(8), display[1].div_ceil(8), 1);
            pass_barrier(core, command);
        }
        stamp(6);
        // The traced world's own anti-aliasing, into this frame's history.
        if anti_aliasing == crate::modern_settings::AntiAliasing::Smaa {
            if tracer.smaa.is_none() {
                tracer.smaa = Some(super::smaa::Smaa::new(core)?);
            }
            let smaa = tracer.smaa.as_mut().expect("made above");
            smaa.run(self, core, color, aa_history[0], extent)?;
            tracer.taa_extent = None;
        } else if anti_aliasing != crate::modern_settings::AntiAliasing::Off {
            let taa = anti_aliasing == crate::modern_settings::AntiAliasing::Taa;
            run(
                if taa { tracer.taa } else { tracer.fxaa },
                tracer.aa_layout,
                aa_set,
            );
            let fresh = i32::from(tracer.taa_extent != Some(extent) || previous.is_none());
            core.device.cmd_push_constants(
                command,
                tracer.aa_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytes_of(&[fresh]),
            );
            core.device.cmd_dispatch(command, groups[0], groups[1], 1);
            pass_barrier(core, command);
            tracer.taa_extent = taa.then_some(extent);
        } else {
            tracer.taa_extent = None;
        }
        stamp(7);
        // The upscaler brings the colour up to the display.
        if let (Some(upscaler), Some(output)) = (tracer.upscaler.as_mut(), upscaled) {
            let frame = UpscaleFrame {
                color,
                depth,
                motion,
                output,
                // The upscalers' jitter moves the image; the tracer's moves
                // the sample.
                jitter: jitter.map(|offset| -offset),
                reset: previous.is_none(),
                near: camera.near,
                vertical_fov: 2.0 * (camera.native[1] as f32 * 0.5 / camera.focal[1]).atan(),
                frame_ms,
                guides: ray_reconstruction.then(|| Guides {
                    diffuse_albedo: guides[0],
                    normal_roughness: guides[1],
                    specular_albedo: guides[2],
                    world_to_view: world_to_view(camera.basis),
                    view_to_clip: view_to_clip(camera),
                }),
            };
            let result = upscaler.dispatch(core, command, &frame);
            pass_barrier(core, command);
            if let Err(error) = result {
                tracer.upscaler_unavailable(modern.upscaler, &error);
            }
        }
        stamp(8);
        // The mist and the distance fog: their cells lit, then gathered
        // along the view.

        if mist {
            run(tracer.mist, tracer.mist_layout, mist_set);
            let fresh = i32::from(
                !(tracer.mist_ran && previous.is_some() && tracer.mist_under == feed.eye_under),
            );
            core.device.cmd_push_constants(
                command,
                tracer.mist_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytes_of(&[fresh]),
            );
            core.device.cmd_dispatch(
                command,
                volume[0].div_ceil(8),
                (volume[1] * volume[2]).div_ceil(8),
                1,
            );
            pass_barrier(core, command);
            run(tracer.gather, tracer.gather_layout, gather_set);
            core.device
                .cmd_dispatch(command, volume[0].div_ceil(8), volume[1].div_ceil(8), 1);
            pass_barrier(core, command);
        }
        tracer.mist_ran = mist;
        tracer.mist_under = feed.eye_under;
        stamp(9);

        // The composite into the target, behind the raster's later draws:
        // a pane's view into its own pixels only.
        self.render_to(core, target)?;
        let clip = feed.clip.map(|[left, top, right, bottom]| {
            // Native pixel x covers target pixels x k to (x + 1) k.
            let scale = [0, 1].map(|axis| display[axis] as f32 / camera.native[axis].max(1) as f32);
            let edge = |native: i16, axis: usize, after: bool| {
                let at = (f32::from(native.max(0)) + f32::from(u8::from(after))) * scale[axis];
                let at = if after { at.ceil() } else { at.floor() };
                (at as u32).min(display[axis])
            };
            let start = [edge(left, 0, false), edge(top, 1, false)];
            let end = [
                edge(right, 0, true).max(start[0]),
                edge(bottom, 1, true).max(start[1]),
            ];
            rect(
                [start[0] as i32, start[1] as i32],
                [end[0] - start[0], end[1] - start[1]],
            )
        });
        if let Some(clip) = clip {
            core.device.cmd_set_scissor(command, 0, &[clip]);
        }
        core.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, tracer.composite);
        core.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::GRAPHICS,
            tracer.composite_layout,
            0,
            &[composite_set],
            &[],
        );
        core.device.cmd_draw(command, 4, 1, 0, 0);
        if clip.is_some() {
            set_viewport(core, command, [0, 0], display, display);
        }
        // The next frame follows this one.
        tracer.previous = Some(view);
        tracer.parity ^= 1;
        tracer.frame_index = tracer.frame_index.wrapping_add(1);
        stamp(10);
        if timed {
            tracer.timing.pending[slot] = true;
            tracer.timing.extent = extent;
        }
        Ok(())
    }

    /// `texture`'s sprite as the tracer samples it, uploaded with its mip
    /// chain by `command` on first use, and its texels' mean light.
    unsafe fn traced_texture(
        &mut self,
        core: &Core,
        tracer: &mut Tracer,
        command: vk::CommandBuffer,
        texture: &CpuTexture,
    ) -> Result<(vk::ImageView, f32), VkDeviceError> {
        let key = texture.content_key();
        let presents = self.presents;
        if let Some(found) = tracer.textures.get_mut(&key) {
            found.last_used = presents;
            return Ok((found.image.view, found.brightness));
        }
        let [width, height] = texture
            .dimensions
            .map(|side| u32::from(side).clamp(1, TEXTURE_SURFACE_SIZE - 2));
        let levels = u32::BITS - width.max(height).leading_zeros();
        // The sprite's texels, inside the page's one-pixel border.
        let rgba = texture.rgba();
        let mut texels = Vec::with_capacity((width * height * 4) as usize);
        for y in 1..=height {
            let row = (y * TEXTURE_SURFACE_SIZE + 1) as usize;
            texels.extend_from_slice(rgba[row..row + width as usize].as_flattened());
        }
        let mut staging = self.buffer(
            core,
            texels.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        staging
            .allocation
            .as_mut()
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("staging memory is not mapped".into()))?
            [..texels.len()]
            .copy_from_slice(&texels);
        let image = self.mipmapped_image(core, [width, height], levels)?;
        let levels_from = |base: u32, count: u32| vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: base,
            level_count: count,
            base_array_layer: 0,
            layer_count: 1,
        };
        let level = |mip_level: u32| vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level,
            base_array_layer: 0,
            layer_count: 1,
        };
        let barrier =
            |range: vk::ImageSubresourceRange,
             layouts: [vk::ImageLayout; 2],
             source: (vk::PipelineStageFlags2, vk::AccessFlags2),
             destination: (vk::PipelineStageFlags2, vk::AccessFlags2)| {
                let barrier = vk::ImageMemoryBarrier2::default()
                    .src_stage_mask(source.0)
                    .src_access_mask(source.1)
                    .dst_stage_mask(destination.0)
                    .dst_access_mask(destination.1)
                    .old_layout(layouts[0])
                    .new_layout(layouts[1])
                    .image(image.image)
                    .subresource_range(range);
                core.device.cmd_pipeline_barrier2(
                    command,
                    &vk::DependencyInfo::default()
                        .image_memory_barriers(std::slice::from_ref(&barrier)),
                );
            };
        let transfer = vk::PipelineStageFlags2::TRANSFER;
        let sampling = (
            vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
        barrier(
            levels_from(0, levels),
            [
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ],
            (vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE),
            (transfer, vk::AccessFlags2::TRANSFER_WRITE),
        );
        core.device.cmd_copy_buffer_to_image(
            command,
            staging.buffer,
            image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::BufferImageCopy::default()
                .image_subresource(level(0))
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })],
        );
        // Each level from the one above, filtered.
        let size = |mip: u32| vk::Offset3D {
            x: (width >> mip).max(1) as i32,
            y: (height >> mip).max(1) as i32,
            z: 1,
        };
        for mip in 1..levels {
            barrier(
                levels_from(mip - 1, 1),
                [
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ],
                (transfer, vk::AccessFlags2::TRANSFER_WRITE),
                (transfer, vk::AccessFlags2::TRANSFER_READ),
            );
            core.device.cmd_blit_image(
                command,
                image.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit {
                    src_subresource: level(mip - 1),
                    src_offsets: [vk::Offset3D::default(), size(mip - 1)],
                    dst_subresource: level(mip),
                    dst_offsets: [vk::Offset3D::default(), size(mip)],
                }],
                vk::Filter::LINEAR,
            );
        }
        if levels > 1 {
            barrier(
                levels_from(0, levels - 1),
                [
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ],
                (transfer, vk::AccessFlags2::TRANSFER_READ),
                sampling,
            );
        }
        barrier(
            levels_from(levels - 1, 1),
            [
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ],
            (transfer, vk::AccessFlags2::TRANSFER_WRITE),
            sampling,
        );
        image.layout.set(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        self.transient.push(staging);
        let view = image.view;
        let brightness = super::emissive::texture_brightness(&texels);
        tracer.textures.insert(
            key,
            TracedTexture {
                image,
                last_used: presents,
                brightness,
            },
        );
        Ok((view, brightness))
    }

    /// A sampled colour image of `extent` with `levels` mip levels.
    unsafe fn mipmapped_image(
        &mut self,
        core: &Core,
        extent: [u32; 2],
        levels: u32,
    ) -> Result<Image, VkDeviceError> {
        let image = core
            .device
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(COLOR_FORMAT)
                    .extent(vk::Extent3D {
                        width: extent[0],
                        height: extent[1],
                        depth: 1,
                    })
                    .mip_levels(levels)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(
                        vk::ImageUsageFlags::SAMPLED
                            | vk::ImageUsageFlags::TRANSFER_SRC
                            | vk::ImageUsageFlags::TRANSFER_DST,
                    )
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )
            .map_err(call("image creation"))?;
        let requirements = core.device.get_image_memory_requirements(image);
        let allocation = self
            .allocator
            .as_mut()
            .expect("allocator lives with the device")
            .allocate(&AllocationCreateDesc {
                name: "traced texture",
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
            .map_err(call("image binding"))?;
        let view = core
            .device
            .create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(COLOR_FORMAT)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: levels,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )
            .map_err(call("image view"))?;
        Ok(Image {
            image,
            view,
            allocation: Some(allocation),
            extent,
            aspect: vk::ImageAspectFlags::COLOR,
            layout: Cell::new(vk::ImageLayout::UNDEFINED),
            set: None,
        })
    }
}

/// Triangle geometry for a bottom level: positions at the start of each
/// `stride`-byte vertex, 32-bit indices, opaque.
fn triangle_geometry<'a>(
    vertices: u64,
    stride: u64,
    max_vertex: u32,
    indices: u64,
    opaque: bool,
) -> vk::AccelerationStructureGeometryKHR<'a> {
    let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
        .vertex_format(vk::Format::R32G32B32_SFLOAT)
        .vertex_data(vk::DeviceOrHostAddressConstKHR {
            device_address: vertices,
        })
        .vertex_stride(stride)
        .max_vertex(max_vertex)
        .index_type(vk::IndexType::UINT32)
        .index_data(vk::DeviceOrHostAddressConstKHR {
            device_address: indices,
        });
    vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .geometry(vk::AccelerationStructureGeometryDataKHR { triangles })
        .flags(if opaque {
            vk::GeometryFlagsKHR::OPAQUE
        } else {
            // A ray tests each candidate once.
            vk::GeometryFlagsKHR::NO_DUPLICATE_ANY_HIT_INVOCATION
        })
}

/// A top-level instance of the bottom level at `address`, moved by
/// `translation`, its triangles hit from both sides, seen by rays whose cull
/// mask meets `mask`.
fn instance(
    translation: [f32; 3],
    custom_index: u32,
    mask: u8,
    address: u64,
) -> vk::AccelerationStructureInstanceKHR {
    let [x, y, z] = translation;
    vk::AccelerationStructureInstanceKHR {
        transform: vk::TransformMatrixKHR {
            matrix: [1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y, 0.0, 0.0, 1.0, z],
        },
        instance_custom_index_and_mask: vk::Packed24_8::new(custom_index, mask),
        instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
            0,
            vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
        ),
        acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
            device_handle: address,
        },
    }
}

/// Buffer copies finish before acceleration structure builds and shaders
/// read the buffers.
unsafe fn upload_barrier(core: &Core, command: vk::CommandBuffer) {
    let barrier = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(
            vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                | vk::PipelineStageFlags2::COMPUTE_SHADER,
        )
        .dst_access_mask(vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_STORAGE_READ);
    core.device.cmd_pipeline_barrier2(
        command,
        &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&barrier)),
    );
}

/// Acceleration structure builds finish before the next build or a shader
/// reads them.
unsafe fn build_barrier(core: &Core, command: vk::CommandBuffer) {
    let barrier = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR)
        .src_access_mask(vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR)
        .dst_stage_mask(
            vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                | vk::PipelineStageFlags2::COMPUTE_SHADER,
        )
        .dst_access_mask(
            vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR
                | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR,
        );
    core.device.cmd_pipeline_barrier2(
        command,
        &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&barrier)),
    );
}

/// The view's rotation as a row-major matrix for row vectors (NGX's
/// convention), about the eye: the tracer's world is eye-relative.
fn world_to_view(basis: [[f32; 3]; 3]) -> [f32; 16] {
    let [right, up, forward] = basis;
    [
        right[0], up[0], forward[0], 0.0, //
        right[1], up[1], forward[1], 0.0, //
        right[2], up[2], forward[2], 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// The projection to clip space for row vectors: the native pixel grid's
/// focal lengths and centre (pixel centres at whole native coordinates), Y
/// up, and the raster's depth `1 - near/z` with no far plane.
fn view_to_clip(camera: &crate::scene_feed::SceneCamera) -> [f32; 16] {
    let [width, height] = camera.native.map(|side| side as f32);
    let [focal_x, focal_y] = camera.focal;
    let [centre_x, centre_y] = camera.centre;
    [
        2.0 * focal_x / width,
        0.0,
        0.0,
        0.0, //
        0.0,
        2.0 * focal_y / height,
        0.0,
        0.0, //
        2.0 * (centre_x + 0.5) / width - 1.0,
        1.0 - 2.0 * (centre_y + 0.5) / height,
        1.0,
        1.0, //
        0.0,
        0.0,
        -camera.near,
        0.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::{sprite_energy, FRAME_FLOATS};
    use crate::d3d_texture::test_texture;

    /// The floats `pt_frame.glsl` declares for one member type, its structs
    /// and `#define`d array lengths read from the same file (scalar layout:
    /// every member is floats, packed without padding).
    fn glsl_floats(source: &str, block: &str) -> usize {
        let defines = |name: &str| -> usize {
            source
                .lines()
                .find_map(|line| line.trim().strip_prefix(&format!("#define {name} ")))
                .unwrap_or_else(|| panic!("no #define {name}"))
                .trim()
                .parse()
                .unwrap()
        };
        let length = |expression: &str| -> usize {
            expression
                .split('*')
                .map(|factor| {
                    let factor = factor.trim();
                    factor.parse().unwrap_or_else(|_| defines(factor))
                })
                .product()
        };
        let members = |body: &str| -> Vec<(String, usize)> {
            body.lines()
                .map(|line| line.split("//").next().unwrap().trim())
                .filter(|line| line.ends_with(';'))
                .map(|line| {
                    let (kind, name) = line.trim_end_matches(';').split_once(' ').unwrap();
                    let count = name
                        .split_once('[')
                        .map_or(1, |(_, size)| length(size.trim_end_matches(']')));
                    (kind.to_string(), count)
                })
                .collect()
        };
        let body_of = |opening: &str| -> &str {
            let start = source
                .find(opening)
                .unwrap_or_else(|| panic!("no {opening}"));
            let open = start + source[start..].find('{').unwrap() + 1;
            &source[open..open + source[open..].find('}').unwrap()]
        };
        fn size(kind: &str, structs: &dyn Fn(&str) -> usize) -> usize {
            match kind {
                "float" => 1,
                "vec2" => 2,
                "vec3" => 3,
                "vec4" => 4,
                other => structs(other),
            }
        }
        let structs = |name: &str| -> usize {
            members(body_of(&format!("struct {name} {{")))
                .iter()
                .map(|(kind, count)| count * size(kind, &|_| panic!("nested struct in {name}")))
                .sum()
        };
        members(body_of(block))
            .iter()
            .map(|(kind, count)| count * size(kind, &structs))
            .sum()
    }

    #[test]
    fn the_frame_block_has_as_many_floats_as_the_shaders_read() {
        let source = include_str!("../../shaders/pt_frame.glsl");
        assert_eq!(glsl_floats(source, "uniform Frame {"), FRAME_FLOATS);
    }

    #[test]
    fn a_sprite_holds_as_much_light_as_its_lit_texels() {
        // A token's core at its brightest, and half of it gone.
        let full = test_texture([4, 4], |_, _| [255, 128, 0, 255]);
        let half = test_texture([4, 4], |x, _| [255, 255, 255, if x < 2 { 255 } else { 0 }]);
        assert_eq!(sprite_energy(&full), 1.0);
        assert_eq!(sprite_energy(&half), 0.5);
    }
}
