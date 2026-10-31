//! Vulkan device for the recovered Direct3D submissions, beside OpenGL.
//!
//! It draws the same validated and clipped TL fans and render states as the
//! [OpenGL device](crate::gl_device), with Vulkan 1.3 (dynamic rendering and
//! synchronization2), into an offscreen colour and depth target of the
//! presented size. Presentation draws that target into an output image of the
//! window's size with black bars; the swap copies the output image to the
//! swapchain. A hidden window has no swapchain and keeps the output image
//! alone, which captures read either way. Device precision, rounding and
//! fill-rule ties belong to the driver and are not native claims.
//!
//! A command buffer records a frame's work and is submitted at the swap or a
//! readback. The swap does not wait for it: the next frame records on the
//! other of [`FRAMES_IN_FLIGHT`] buffers while the GPU finishes the last,
//! and waits for that one only where it would touch what the GPU still uses
//! (the tracer's and the passes' host-written buffers and descriptor sets,
//! the swapchain's semaphores) or reads the GPU's pixels back. Each
//! submission keeps the staging buffers, images and descriptor sets it
//! released until its fence signals; each buffer slot has its own vertex
//! memory. The GPU runs the same commands in the same order as when every
//! submission was waited for.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{c_void, CStr, CString},
    fmt,
    rc::Rc,
};

use ash::vk::{self, Handle};
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator, AllocatorCreateDesc},
    MemoryLocation,
};
use sdl2::video::Window;
use thiserror::Error;

mod blue_noise;
mod dlss;
mod emissive;
mod grade;
mod heat_memory;
mod metal;
mod mirror;
pub(crate) use mirror::Murk;
mod object_heat;
mod perf;
mod raster_aa;
mod raster_sky;
mod sky;
mod smaa;
mod thermal;
mod trace;
mod upscale;
mod xess;

use crate::{
    d3d_texture::{CpuTexture, TEXTURE_SURFACE_SIZE},
    d3d_textured::D3dTextureBlend,
    d3d_viewport::D3dTlVertex,
    frame_device::{device_vertex, DeviceDraw, DeviceVertex},
    presentation::PresentationRect,
    software_frame::D3dTextureSampling,
};

/// Device textures unused for this many presentations are released, as the
/// OpenGL device's are.
const TEXTURE_IDLE_PRESENTS: u64 = 600;
const COLOR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
/// Host-visible vertex memory is taken in chunks of this many bytes.
const VERTEX_CHUNK: u64 = 8 << 20;
/// `vk_primitive.vert`'s push constants: the scale, the depth test and the
/// jitter, which std430 places at byte 16.
const PRIMITIVE_PUSH: usize = 24;
const DESCRIPTOR_POOL_SETS: u32 = 1024;
/// Recordings that may be submitted and not yet waited for: the next
/// frame's CPU work runs while the GPU finishes the last.
const FRAMES_IN_FLIGHT: usize = 2;

static PRIMITIVE_VERTEX: &[u8] = include_bytes!("../shaders/vk_primitive.vert.spv");
static FLAT_FRAGMENT: &[u8] = include_bytes!("../shaders/vk_flat.frag.spv");
static TEXTURED_FRAGMENT: &[u8] = include_bytes!("../shaders/vk_textured.frag.spv");
static TEXTURED_FOG_FRAGMENT: &[u8] = include_bytes!("../shaders/vk_textured_fog.frag.spv");
static BLIT_VERTEX: &[u8] = include_bytes!("../shaders/vk_blit.vert.spv");
static BLIT_FRAGMENT: &[u8] = include_bytes!("../shaders/vk_blit.frag.spv");
static DOWNSAMPLE_FRAGMENT: &[u8] = include_bytes!("../shaders/vk_downsample.frag.spv");

#[derive(Debug, Error)]
pub enum VkDeviceError {
    #[error("Vulkan loader: {0}")]
    Loader(String),
    #[error("Vulkan {operation}: {result}")]
    Call {
        operation: &'static str,
        result: vk::Result,
    },
    #[error("Vulkan surface: {0}")]
    Surface(String),
    #[error("no Vulkan 1.3 device with graphics{0}, dynamic rendering and synchronization2")]
    NoDevice(&'static str),
    #[error("Vulkan memory: {0}")]
    Memory(String),
}

fn call(operation: &'static str) -> impl FnOnce(vk::Result) -> VkDeviceError {
    move |result| VkDeviceError::Call { operation, result }
}

/// Handles fixed for the device's lifetime.
struct Core {
    _entry: ash::Entry,
    instance: ash::Instance,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical: vk::PhysicalDevice,
    device: ash::Device,
    swapchain_loader: Option<ash::khr::swapchain::Device>,
    /// Present when the device traces rays ([`ray_tracing_support`]).
    acceleration_loader: Option<ash::khr::acceleration_structure::Device>,
    /// The device takes the scalar block layout, which the frame block's
    /// passes need: the tracer's, and the raster's own modern sky.
    scalar_layout: bool,
    /// The device enables `VK_KHR_get_memory_requirements2`, whose KHR names
    /// FSR's library calls without checking; without it FSR is unavailable.
    upscaler_functions: bool,
    /// The NGX bridge for DLSS, when it is beside the executable; the
    /// instance and device enable the extensions it asked for.
    ngx: Option<dlss::Ngx>,
    /// XeSS's library, when it is beside the executable; the instance and
    /// device enable what it asked for.
    xess: Option<xess::XessLibrary>,
    queue: vk::Queue,
    texture_layout: vk::DescriptorSetLayout,
    primitive_layout: vk::PipelineLayout,
    blit_layout: vk::PipelineLayout,
    modules: Modules,
    /// Point and linear filtering, each wrapping or clamped.
    samplers: [vk::Sampler; 4],
    command_pool: vk::CommandPool,
    /// The command buffer being recorded: one of `commands`.
    command: Cell<vk::CommandBuffer>,
    /// Each frame slot's command buffer, its submission's fence, and the
    /// semaphore its swapchain image is acquired with.
    commands: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
    fences: [vk::Fence; FRAMES_IN_FLIGHT],
    acquired: [vk::Semaphore; FRAMES_IN_FLIGHT],
}

struct Modules {
    primitive: vk::ShaderModule,
    flat: vk::ShaderModule,
    textured: vk::ShaderModule,
    textured_fog: vk::ShaderModule,
    blit_vertex: vk::ShaderModule,
    blit_fragment: vk::ShaderModule,
    downsample_fragment: vk::ShaderModule,
}

struct Image {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<Allocation>,
    extent: [u32; 2],
    aspect: vk::ImageAspectFlags,
    layout: Cell<vk::ImageLayout>,
    /// Sampled through a point-filtered, clamped descriptor (blits), with
    /// the pool it came from.
    set: Option<Descriptor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Descriptor {
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
}

struct Buffer {
    buffer: vk::Buffer,
    allocation: Option<Allocation>,
}

struct Texture {
    image: Image,
    /// Descriptor sets by sampler index, allocated on first use.
    sets: [Option<Descriptor>; 4],
    last_used: u64,
}

/// A frame's colour and depth target and its native surface for
/// replacements, kept in a pool between frames.
struct Target {
    color: Image,
    depth: Image,
    surface: Option<Image>,
}

struct Swapchain {
    handle: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    extent: [u32; 2],
    presented: Vec<vk::Semaphore>,
    stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipelineKey {
    textured: bool,
    blend: D3dTextureBlend,
    depth_test: bool,
    depth_write: bool,
    /// A textured draw over a traced world, which takes the tracer's fog
    /// at its depth in place of retail's vertex fog.
    traced_fog: bool,
    /// An opaque draw of the modern raster's world with Z Buffering off:
    /// it writes its depth untested, so the painter's order still decides
    /// what shows and the passes after the world (the water's mirror, the
    /// sky's, the anti-aliasing's motion) have the depth they read.
    depth_always: bool,
}

/// What a submitted recording released: freed once its fence signals.
struct Released {
    transient: Vec<Buffer>,
    retired: Vec<Image>,
    retired_sets: Vec<Descriptor>,
    retired_accelerations: Vec<vk::AccelerationStructureKHR>,
}

struct State {
    allocator: Option<Allocator>,
    recording: bool,
    /// The frames' timing, reported when the device closes.
    perf: perf::FramePerf,
    /// The colour image of the active dynamic rendering, if any.
    rendering: Option<vk::Image>,
    /// The size the raster's world is drawn at, at the frame's top left,
    /// while an upscaler brings it up untraced: its draws' viewport.
    raster_viewport: Option<[u32; 2]>,
    /// What the recording released, kept until its submission is done.
    transient: Vec<Buffer>,
    retired: Vec<Image>,
    retired_sets: Vec<Descriptor>,
    /// Acceleration structures outgrown while a frame in flight may still
    /// trace them (their buffers go to `transient`).
    retired_accelerations: Vec<vk::AccelerationStructureKHR>,
    /// The frame slot being recorded, and each slot's submission not yet
    /// waited for, with what it released.
    slot: usize,
    in_flight: [Option<Released>; FRAMES_IN_FLIGHT],
    /// Each slot's vertex memory; the recording fills its slot's.
    vertex_chunks: [Vec<(Buffer, *mut u8)>; FRAMES_IN_FLIGHT],
    vertex_chunk: usize,
    vertex_offset: u64,
    /// The last frame copied for a snapshot ([`VkFrame::stage_rgba`]): its
    /// readback buffer and the size it was drawn at.
    staged: Option<(Buffer, [u32; 2])>,
    textures: HashMap<u64, Texture>,
    presents: u64,
    pools: Vec<vk::DescriptorPool>,
    targets: Vec<Target>,
    output: Option<Image>,
    swapchain: Option<Swapchain>,
    pipelines: HashMap<PipelineKey, vk::Pipeline>,
    /// Blit pipelines into a colour-and-depth target and into the output.
    blit_target: vk::Pipeline,
    blit_output: vk::Pipeline,
    /// A supersampled frame onto the output, averaged down.
    downsample_output: vk::Pipeline,
    /// The path tracer, made on first use on a device that traces rays.
    tracer: Option<Box<trace::Tracer>>,
    /// Thermal vision's camera, made on first use.
    thermal: Option<Box<thermal::ThermalCamera>>,
    /// Anti-aliasing for the raster's world without traced rays, made on
    /// first use.
    raster_aa: Option<Box<raster_aa::RasterAa>>,
    /// The raster's water mirror, made on first use.
    mirror: Option<Box<mirror::Mirror>>,
    /// The raster's metal, made on first use.
    metal: Option<Box<metal::Metal>>,
    /// The modern sky's clouds, the tracer's and the raster's, made on first
    /// use.
    sky: Option<Box<sky::Sky>>,
    /// The raster's own modern sky, made on first use.
    raster_sky: Option<Box<raster_sky::RasterSky>>,
    /// The lighting preset's grade, made on first use.
    grade: Option<Box<grade::Grade>>,
    /// The grade this frame's traced world waits for, once its last draw
    /// is in.
    graded: Option<crate::modern_tuning::LightingTuning>,
    /// This frame's traced world has a distance fog, which the raster's
    /// late draws over it take until its last draw is in.
    traced_fog: bool,
    /// This frame's traced world is thermal vision's: the raster's smoke and
    /// darkening sprites over it draw at half strength, as a thermal camera
    /// sees through them, until the camera is laid over it.
    thermal_effects: bool,
    /// Settings `+3C`'s Filtering, which picks the tracer's sampler.
    filtering: u32,
    /// The modern mode the player set: path tracing, upscaling and
    /// anti-aliasing.
    modern: crate::modern_settings::ModernSettings,
    /// A failed call stops further work; the message was logged once.
    failed: bool,
}

/// One Vulkan instance and device with its pipelines, textures and output.
/// Frames share it through `Rc`.
pub struct VkDevice {
    description: String,
    core: Core,
    state: RefCell<State>,
    /// What it runs of the modern mode, once asked.
    support: Cell<Option<crate::modern_settings::ModernSupport>>,
    /// The saved upscaler last logged as not running here.
    fallback_logged: Cell<Option<crate::modern_settings::Upscaler>>,
}

impl fmt::Debug for VkDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VkDevice")
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

unsafe extern "system" fn debug_message(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _: *mut c_void,
) -> vk::Bool32 {
    if !data.is_null() && !(*data).p_message.is_null() {
        let message = CStr::from_ptr((*data).p_message).to_string_lossy();
        eprintln!("Vulkan validation ({severity:?}): {message}");
    }
    vk::FALSE
}

/// Where NGX's device extensions are kept between starts: the file
/// `INF_NGX_EXTENSIONS` names, else, for a shown window, the port's
/// per-user cache folder beside `launcher.json`, since the answer belongs
/// to the machine's GPU. Hidden runs (probes and tests) keep nothing.
fn ngx_kept_path(hidden: bool) -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("INF_NGX_EXTENSIONS") {
        return Some(path.into());
    }
    (!hidden).then(|| {
        crate::launcher_setup::default_preferences_path()
            .with_file_name("cache")
            .join("ngx-device-extensions.json")
    })
}

/// The validation layer is on in debug builds, or with
/// `INF_VULKAN_VALIDATION=1`, when the Vulkan SDK provides it.
fn validation_wanted() -> bool {
    cfg!(debug_assertions) || std::env::var("INF_VULKAN_VALIDATION").is_ok_and(|value| value == "1")
}

impl VkDevice {
    /// Create an instance, device and pipelines for `window`, which SDL must
    /// have built as a Vulkan window. A hidden window gets no surface: frames
    /// present into the output image, which captures read. Any failure leaves
    /// the caller free to fall back to another renderer.
    pub fn new(window: &Window, hidden: bool) -> Result<Rc<Self>, VkDeviceError> {
        // SAFETY: the loader is the system's vulkan-1; every handle created
        // below is destroyed in Drop (or on the error paths), in reverse order.
        unsafe { Self::create(window, hidden) }
    }

    /// What the device a shown game would make runs of the modern mode, for
    /// the launcher's Options before any game: one made on a hidden window
    /// (with a surface, so it is the same GPU and shares the kept NGX
    /// answers), asked and dropped. None when Vulkan does not start.
    pub fn probe_modern_support() -> Option<crate::modern_settings::ModernSupport> {
        let sdl = sdl2::init().ok()?;
        let video = sdl.video().ok()?;
        let window = video
            .window("Infestation", 64, 64)
            .hidden()
            .vulkan()
            .build()
            .ok()?;
        let device = Self::new(&window, false).ok()?;
        Some(device.modern_support())
    }

    unsafe fn create(window: &Window, hidden: bool) -> Result<Rc<Self>, VkDeviceError> {
        let entry = ash::Entry::load().map_err(|error| VkDeviceError::Loader(error.to_string()))?;
        let mut extensions: Vec<CString> = window
            .vulkan_instance_extensions()
            .map_err(VkDeviceError::Surface)?
            .into_iter()
            .map(|name| CString::new(name).expect("extension names have no NUL"))
            .collect();
        // The upscalers' extensions, those the instance offers.
        let ngx = dlss::Ngx::load();
        let xess = xess::XessLibrary::load();
        let wanted: Vec<CString> = ngx
            .as_ref()
            .map(|ngx| ngx.instance_extensions())
            .into_iter()
            .chain(xess.as_ref().map(|xess| xess.instance_extensions()))
            .flatten()
            .collect();
        if !wanted.is_empty() {
            let offered = entry
                .enumerate_instance_extension_properties(None)
                .unwrap_or_default();
            for name in wanted {
                if !extensions.contains(&name)
                    && offered
                        .iter()
                        .any(|extension| extension.extension_name_as_c_str() == Ok(name.as_c_str()))
                {
                    extensions.push(name);
                }
            }
        }
        let validation = c"VK_LAYER_KHRONOS_validation";
        let available = entry
            .enumerate_instance_layer_properties()
            .unwrap_or_default();
        let layers: Vec<*const i8> = if validation_wanted()
            && available
                .iter()
                .any(|layer| layer.layer_name_as_c_str() == Ok(validation))
        {
            extensions.push(ash::ext::debug_utils::NAME.to_owned());
            vec![validation.as_ptr()]
        } else {
            Vec::new()
        };
        let extension_pointers: Vec<*const i8> =
            extensions.iter().map(|name| name.as_ptr()).collect();
        let application = vk::ApplicationInfo::default()
            .application_name(c"Infestation")
            .api_version(vk::API_VERSION_1_3);
        let instance = entry
            .create_instance(
                &vk::InstanceCreateInfo::default()
                    .application_info(&application)
                    .enabled_layer_names(&layers)
                    .enabled_extension_names(&extension_pointers),
                None,
            )
            .map_err(call("instance creation"))?;
        let debug = if layers.is_empty() {
            None
        } else {
            let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let messenger = loader
                .create_debug_utils_messenger(
                    &vk::DebugUtilsMessengerCreateInfoEXT::default()
                        .message_severity(
                            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                        )
                        .message_type(
                            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                        )
                        .pfn_user_callback(Some(debug_message)),
                    None,
                )
                .ok();
            messenger.map(|messenger| (loader, messenger))
        };
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let surface = if hidden {
            vk::SurfaceKHR::null()
        } else {
            match window.vulkan_create_surface(instance.handle().as_raw() as usize) {
                Ok(raw) => vk::SurfaceKHR::from_raw(raw),
                Err(error) => {
                    destroy_instance(&instance, &debug);
                    return Err(VkDeviceError::Surface(error));
                }
            }
        };
        let picked = pick_device(&instance, &surface_loader, surface);
        let Some((physical, family, name)) = picked else {
            if surface != vk::SurfaceKHR::null() {
                surface_loader.destroy_surface(surface, None);
            }
            destroy_instance(&instance, &debug);
            return Err(VkDeviceError::NoDevice(if hidden {
                ""
            } else {
                " and presentation"
            }));
        };
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let ray_tracing = ray_tracing_support(&instance, physical);
        let kept = ngx_kept_path(hidden);
        let upscaler_functions = device_has(
            &instance,
            physical,
            ash::khr::get_memory_requirements2::NAME,
        );
        let mut device_extensions: Vec<*const i8> = Vec::new();
        if !hidden {
            device_extensions.push(ash::khr::swapchain::NAME.as_ptr());
        }
        if upscaler_functions {
            device_extensions.push(ash::khr::get_memory_requirements2::NAME.as_ptr());
        }
        let upscaler_extensions: Vec<CString> = ngx
            .as_ref()
            .map(|ngx| ngx.kept_device_extensions(&instance, physical, kept.as_deref()))
            .into_iter()
            .chain(
                xess.as_ref()
                    .map(|xess| xess.device_extensions(instance.handle(), physical)),
            )
            .flatten()
            .filter(|name| device_has(&instance, physical, name))
            .collect();
        for name in &upscaler_extensions {
            if !device_extensions
                .iter()
                .any(|known| CStr::from_ptr(*known) == name.as_c_str())
            {
                device_extensions.push(name.as_ptr());
            }
        }
        if ray_tracing {
            device_extensions.extend(RAY_TRACING_EXTENSIONS.map(CStr::as_ptr));
        }
        // FSR's shaders take 16-bit types wherever the device has them.
        let mut supported12 = vk::PhysicalDeviceVulkan12Features::default();
        instance.get_physical_device_features2(
            physical,
            &mut vk::PhysicalDeviceFeatures2::default().push_next(&mut supported12),
        );
        let supported = instance.get_physical_device_features(physical);
        // Demotion is how `discard` compiles for Vulkan 1.3, which requires it.
        let mut features13 = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true)
            .shader_demote_to_helper_invocation(true);
        let mut features12 = vk::PhysicalDeviceVulkan12Features::default()
            .shader_float16(supported12.shader_float16 == vk::TRUE)
            .buffer_device_address(ray_tracing)
            .descriptor_indexing(ray_tracing)
            .runtime_descriptor_array(ray_tracing)
            .shader_sampled_image_array_non_uniform_indexing(ray_tracing)
            .descriptor_binding_partially_bound(ray_tracing)
            .descriptor_binding_variable_descriptor_count(ray_tracing)
            .scalar_block_layout(supported12.scalar_block_layout == vk::TRUE);
        let mut acceleration = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
            .acceleration_structure(true);
        let mut ray_query = vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(true);
        // The tracer filters textures anisotropically wherever it can.
        let core_features = vk::PhysicalDeviceFeatures::default()
            .sampler_anisotropy(supported.sampler_anisotropy == vk::TRUE)
            .shader_int16(supported.shader_int16 == vk::TRUE);
        // The features ride in one chain under VkPhysicalDeviceFeatures2,
        // where XeSS turns on what it needs.
        let mut features = vk::PhysicalDeviceFeatures2::default()
            .features(core_features)
            .push_next(&mut features13)
            .push_next(&mut features12);
        if ray_tracing {
            features = features
                .push_next(&mut acceleration)
                .push_next(&mut ray_query);
        }
        let xess =
            xess.filter(|xess| xess.patch_features(instance.handle(), physical, &mut features));
        let create = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&device_extensions)
            .push_next(&mut features);
        let device = match instance.create_device(physical, &create, None) {
            Ok(device) => device,
            Err(result) => {
                if surface != vk::SurfaceKHR::null() {
                    surface_loader.destroy_surface(surface, None);
                }
                destroy_instance(&instance, &debug);
                return Err(call("device creation")(result));
            }
        };
        let properties = instance.get_physical_device_properties(physical);
        let version = properties.api_version;
        let description = format!(
            "Vulkan {}.{}.{} ({name})",
            vk::api_version_major(version),
            vk::api_version_minor(version),
            vk::api_version_patch(version)
        );
        let queue = device.get_device_queue(family, 0);
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: physical,
            debug_settings: Default::default(),
            buffer_device_address: ray_tracing,
            allocation_sizes: Default::default(),
        })
        .map_err(|error| VkDeviceError::Memory(error.to_string()));
        let allocator = match allocator {
            Ok(allocator) => allocator,
            Err(error) => {
                device.destroy_device(None);
                if surface != vk::SurfaceKHR::null() {
                    surface_loader.destroy_surface(surface, None);
                }
                destroy_instance(&instance, &debug);
                return Err(error);
            }
        };
        let swapchain_loader =
            (!hidden).then(|| ash::khr::swapchain::Device::new(&instance, &device));
        let acceleration_loader =
            ray_tracing.then(|| ash::khr::acceleration_structure::Device::new(&instance, &device));
        // From here every object is owned by the device, whose Drop releases
        // whatever was made, so errors simply return.
        let mut built = Self {
            description,
            core: Core {
                _entry: entry,
                instance,
                debug,
                surface_loader,
                surface,
                physical,
                device,
                swapchain_loader,
                acceleration_loader,
                scalar_layout: supported12.scalar_block_layout == vk::TRUE,
                upscaler_functions,
                ngx,
                xess,
                queue,
                texture_layout: vk::DescriptorSetLayout::null(),
                primitive_layout: vk::PipelineLayout::null(),
                blit_layout: vk::PipelineLayout::null(),
                modules: Modules {
                    primitive: vk::ShaderModule::null(),
                    flat: vk::ShaderModule::null(),
                    textured: vk::ShaderModule::null(),
                    textured_fog: vk::ShaderModule::null(),
                    blit_vertex: vk::ShaderModule::null(),
                    blit_fragment: vk::ShaderModule::null(),
                    downsample_fragment: vk::ShaderModule::null(),
                },
                samplers: [vk::Sampler::null(); 4],
                command_pool: vk::CommandPool::null(),
                command: Cell::new(vk::CommandBuffer::null()),
                commands: [vk::CommandBuffer::null(); FRAMES_IN_FLIGHT],
                fences: [vk::Fence::null(); FRAMES_IN_FLIGHT],
                acquired: [vk::Semaphore::null(); FRAMES_IN_FLIGHT],
            },
            support: Cell::new(None),
            fallback_logged: Cell::new(None),
            state: RefCell::new(State {
                allocator: Some(allocator),
                recording: false,
                perf: Default::default(),
                rendering: None,
                raster_viewport: None,
                transient: Vec::new(),
                retired: Vec::new(),
                retired_sets: Vec::new(),
                retired_accelerations: Vec::new(),
                slot: 0,
                in_flight: Default::default(),
                vertex_chunks: Default::default(),
                vertex_chunk: 0,
                vertex_offset: 0,
                staged: None,
                textures: HashMap::new(),
                presents: 0,
                pools: Vec::new(),
                targets: Vec::new(),
                output: None,
                swapchain: None,
                pipelines: HashMap::new(),
                blit_target: vk::Pipeline::null(),
                blit_output: vk::Pipeline::null(),
                downsample_output: vk::Pipeline::null(),
                tracer: None,
                thermal: None,
                raster_aa: None,
                mirror: None,
                metal: None,
                sky: None,
                raster_sky: None,
                grade: None,
                graded: None,
                traced_fog: false,
                thermal_effects: false,
                filtering: crate::frontend_video::FILTERING_RETAIL_MOST,
                modern: Default::default(),
                failed: false,
            }),
        };
        built.prepare(family)?;
        Ok(Rc::new(built))
    }

    unsafe fn prepare(&mut self, family: u32) -> Result<(), VkDeviceError> {
        let core = &mut self.core;
        let device = &core.device;
        let binding = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        core.texture_layout = device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&binding),
                None,
            )
            .map_err(call("descriptor layout"))?;
        let set_layouts = [core.texture_layout];
        let primitive_push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(PRIMITIVE_PUSH as u32)];
        core.primitive_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(&primitive_push),
                None,
            )
            .map_err(call("pipeline layout"))?;
        let blit_push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .size(4)];
        core.blit_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(&blit_push),
                None,
            )
            .map_err(call("pipeline layout"))?;
        let module = |bytes: &[u8]| -> Result<vk::ShaderModule, VkDeviceError> {
            let words = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
                .expect("embedded SPIR-V is whole words");
            device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
                .map_err(call("shader module"))
        };
        core.modules.primitive = module(PRIMITIVE_VERTEX)?;
        core.modules.flat = module(FLAT_FRAGMENT)?;
        core.modules.textured = module(TEXTURED_FRAGMENT)?;
        core.modules.textured_fog = module(TEXTURED_FOG_FRAGMENT)?;
        core.modules.blit_vertex = module(BLIT_VERTEX)?;
        core.modules.blit_fragment = module(BLIT_FRAGMENT)?;
        core.modules.downsample_fragment = module(DOWNSAMPLE_FRAGMENT)?;
        for (index, sampler) in core.samplers.iter_mut().enumerate() {
            let filter = if index & 1 == 0 {
                vk::Filter::NEAREST
            } else {
                vk::Filter::LINEAR
            };
            let address = if index & 2 == 0 {
                vk::SamplerAddressMode::REPEAT
            } else {
                vk::SamplerAddressMode::CLAMP_TO_EDGE
            };
            *sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(filter)
                        .min_filter(filter)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(address)
                        .address_mode_v(address)
                        .address_mode_w(address)
                        .max_lod(0.0),
                    None,
                )
                .map_err(call("sampler"))?;
        }
        core.command_pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
            .map_err(call("command pool"))?;
        let commands = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(core.command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(FRAMES_IN_FLIGHT as u32),
            )
            .map_err(call("command buffer"))?;
        core.commands.copy_from_slice(&commands);
        core.command.set(core.commands[0]);
        for slot in 0..FRAMES_IN_FLIGHT {
            core.fences[slot] = device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .map_err(call("fence"))?;
            core.acquired[slot] = device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                .map_err(call("semaphore"))?;
        }
        let core = &self.core;
        let mut state = self.state.borrow_mut();
        state.perf = perf::FramePerf::new(core);
        state.blit_target = blit_pipeline(core, true)?;
        state.blit_output = blit_pipeline(core, false)?;
        state.downsample_output = graphics_pipeline(
            core,
            core.blit_layout,
            core.modules.blit_vertex,
            core.modules.downsample_fragment,
            COLOR_FORMAT,
            false,
            None,
        )?;
        Ok(())
    }

    /// Driver version and device name, for the startup log.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Whether the device traces rays: acceleration structures, ray queries
    /// and a texture array indexed in shaders.
    pub fn ray_tracing(&self) -> bool {
        self.core.acceleration_loader.is_some()
    }

    /// Settings `+3C`'s Filtering for the traced world's textures.
    pub fn set_filtering(&self, level: u32) {
        self.state.borrow_mut().filtering = level;
    }

    /// The player's modern mode, from the next frame on.
    /// The player's modern mode, as this device runs it.
    pub fn set_modern(&self, modern: crate::modern_settings::ModernSettings) {
        let support = self.modern_support();
        let effective = modern.effective(support);
        // The saved choice stays saved, for a GPU that runs it.
        if !support.runs(modern.upscaler)
            && self.fallback_logged.replace(Some(modern.upscaler)) != Some(modern.upscaler)
        {
            eprintln!(
                "Infestation: this GPU does not run the saved {:?} upscaler; using {:?} (the saved choice is kept)",
                modern.upscaler, effective.upscaler
            );
        }
        self.state.borrow_mut().modern = effective;
    }

    /// How much larger, on each side, frames are drawn than shown: the
    /// player's supersampling, while no upscaler brings its own
    /// anti-aliasing.
    fn supersampling(&self) -> f32 {
        let modern = self.state.borrow().modern;
        if modern.upscaler == crate::modern_settings::Upscaler::Off {
            modern.anti_aliasing.supersampling()
        } else {
            1.0
        }
    }

    /// The target a frame shown `presented` large draws at now.
    fn frame_target(&self, presented: [u32; 2]) -> [u32; 2] {
        // SAFETY: the instance and the physical device live with the device.
        let largest = unsafe {
            self.core
                .instance
                .get_physical_device_properties(self.core.physical)
                .limits
                .max_image_dimension2_d
        };
        supersampled(presented, self.supersampling(), largest)
    }

    /// What this device runs of the modern mode, asked once: ray tracing,
    /// and each upscaler whose library loads and which the GPU takes.
    pub fn modern_support(&self) -> crate::modern_settings::ModernSupport {
        if let Some(support) = self.support.get() {
            return support;
        }
        let ray_tracing = self.ray_tracing();
        // Ray Reconstruction denoises traced light; the others also
        // anti-alias the raster's world at its native size.
        let upscalers = crate::modern_settings::Upscaler::ALL.map(|upscaler| {
            use crate::modern_settings::Upscaler;
            // SAFETY: the probes ask the libraries about this device only.
            upscaler == Upscaler::Off
                || ((ray_tracing || upscaler != Upscaler::DlssRayReconstruction)
                    && unsafe { upscale::runs(&self.core, upscaler) })
        });
        let support = crate::modern_settings::ModernSupport {
            ray_tracing,
            upscalers,
            modern_sky: self.core.scalar_layout,
        };
        self.support.set(Some(support));
        support
    }

    /// The presented output's pixels, top row first: the output image the
    /// last [`VkFrame::present`] drew, or black before any.
    pub(crate) fn read_output(&self, output: [u32; 2]) -> Vec<u8> {
        let mut state = self.state.borrow_mut();
        let core = &self.core;
        let pixels = output[0] as usize * output[1] as usize * 4;
        let result = (|| -> Result<Option<Vec<u8>>, VkDeviceError> {
            let Some(image) = state.output.take() else {
                return Ok(None);
            };
            let read = if image.extent == output {
                // SAFETY: the image belongs to the device and is idle or
                // recorded on its own buffer.
                Some(unsafe { state.read_image(core, &image)? })
            } else {
                None
            };
            state.output = Some(image);
            Ok(read)
        })();
        match result {
            Ok(Some(rgba)) => rgba,
            Ok(None) => [0, 0, 0, 255].repeat(pixels / 4),
            Err(error) => {
                state.fail(&error);
                vec![0; pixels]
            }
        }
    }

    /// The window's drawable size in pixels.
    pub(crate) fn drawable_size(window: &Window) -> [u32; 2] {
        window.vulkan_drawable_size().into()
    }

    /// Copy the presented output to the swapchain and show it, or with no
    /// swapchain (a hidden window) just finish the frame's work.
    pub(crate) fn swap(&self, window: &Window) {
        let _swap = crate::cpu_profile::scope("swap (submit, wait, present)");
        let mut state = self.state.borrow_mut();
        let core = &self.core;
        // SAFETY: the recording and the swapchain belong to this device.
        let result = unsafe {
            if core.swapchain_loader.is_none() {
                state.submit(core, None)
            } else {
                state.present_swapchain(core, Self::drawable_size(window))
            }
        };
        if let Err(error) = result {
            state.fail(&error);
        }
        state.perf.swapped();
    }
}

impl Drop for VkDevice {
    fn drop(&mut self) {
        let core = &self.core;
        let state = self.state.get_mut();
        // SAFETY: the device is idle after the wait; each handle is released
        // once, the allocator before the device and the device before the
        // instance.
        unsafe {
            let _ = core.device.device_wait_idle();
            let mut allocator = state.allocator.take();
            if let Some(tracer) = state.tracer.take() {
                tracer.destroy(core, &mut allocator);
            }
            state.perf.report();
            state.perf.destroy(core);
            crate::cpu_profile::report();
            if let Some(camera) = state.thermal.take() {
                camera.destroy(core, &mut allocator);
            }
            if let Some(aa) = state.raster_aa.take() {
                aa.destroy(core, &mut allocator);
            }
            if let Some(mirror) = state.mirror.take() {
                mirror.destroy(core, &mut allocator);
            }
            if let Some(metal) = state.metal.take() {
                metal.destroy(core, &mut allocator);
            }
            if let Some(sky) = state.raster_sky.take() {
                sky.destroy(core, &mut allocator);
            }
            if let Some(sky) = state.sky.take() {
                sky.destroy(core, &mut allocator);
            }
            if let Some(grade) = state.grade.take() {
                grade.destroy(core, &mut allocator);
            }
            if let Some(ngx) = &core.ngx {
                ngx.stop(core.device.handle());
            }
            if let Some(xess) = &core.xess {
                xess.stop();
            }
            let free_image = |image: Image, allocator: &mut Option<Allocator>| {
                core.device.destroy_image_view(image.view, None);
                core.device.destroy_image(image.image, None);
                if let (Some(allocation), Some(allocator)) = (image.allocation, allocator.as_mut())
                {
                    let _ = allocator.free(allocation);
                }
            };
            let free_buffer = |buffer: Buffer, allocator: &mut Option<Allocator>| {
                core.device.destroy_buffer(buffer.buffer, None);
                if let (Some(allocation), Some(allocator)) = (buffer.allocation, allocator.as_mut())
                {
                    let _ = allocator.free(allocation);
                }
            };
            for released in state.in_flight.iter_mut().filter_map(Option::take) {
                state.transient.extend(released.transient);
                state.retired.extend(released.retired);
                state
                    .retired_accelerations
                    .extend(released.retired_accelerations);
            }
            if let Some(loader) = core.acceleration_loader.as_ref() {
                for handle in state.retired_accelerations.drain(..) {
                    loader.destroy_acceleration_structure(handle, None);
                }
            }
            state
                .transient
                .extend(state.staged.take().map(|(buffer, _)| buffer));
            for buffer in state.transient.drain(..) {
                free_buffer(buffer, &mut allocator);
            }
            for (buffer, _) in state
                .vertex_chunks
                .iter_mut()
                .flat_map(|chunks| chunks.drain(..))
            {
                free_buffer(buffer, &mut allocator);
            }
            for image in state.retired.drain(..) {
                free_image(image, &mut allocator);
            }
            for (_, texture) in state.textures.drain() {
                free_image(texture.image, &mut allocator);
            }
            for target in state.targets.drain(..) {
                free_image(target.color, &mut allocator);
                free_image(target.depth, &mut allocator);
                if let Some(surface) = target.surface {
                    free_image(surface, &mut allocator);
                }
            }
            if let Some(output) = state.output.take() {
                free_image(output, &mut allocator);
            }
            if let (Some(swapchain), Some(loader)) =
                (state.swapchain.take(), core.swapchain_loader.as_ref())
            {
                for semaphore in swapchain.presented {
                    core.device.destroy_semaphore(semaphore, None);
                }
                loader.destroy_swapchain(swapchain.handle, None);
            }
            for pipeline in state.pipelines.values() {
                core.device.destroy_pipeline(*pipeline, None);
            }
            core.device.destroy_pipeline(state.blit_target, None);
            core.device.destroy_pipeline(state.blit_output, None);
            core.device.destroy_pipeline(state.downsample_output, None);
            for pool in state.pools.drain(..) {
                core.device.destroy_descriptor_pool(pool, None);
            }
            drop(allocator);
            for slot in 0..FRAMES_IN_FLIGHT {
                core.device.destroy_semaphore(core.acquired[slot], None);
                core.device.destroy_fence(core.fences[slot], None);
            }
            core.device.destroy_command_pool(core.command_pool, None);
            for sampler in core.samplers {
                core.device.destroy_sampler(sampler, None);
            }
            let modules = &core.modules;
            for module in [
                modules.primitive,
                modules.flat,
                modules.textured,
                modules.textured_fog,
                modules.blit_vertex,
                modules.blit_fragment,
                modules.downsample_fragment,
            ] {
                core.device.destroy_shader_module(module, None);
            }
            core.device
                .destroy_pipeline_layout(core.primitive_layout, None);
            core.device.destroy_pipeline_layout(core.blit_layout, None);
            core.device
                .destroy_descriptor_set_layout(core.texture_layout, None);
            core.device.destroy_device(None);
            if core.surface != vk::SurfaceKHR::null() {
                core.surface_loader.destroy_surface(core.surface, None);
            }
            destroy_instance(&core.instance, &core.debug);
        }
    }
}

unsafe fn destroy_instance(
    instance: &ash::Instance,
    debug: &Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
) {
    if let Some((loader, messenger)) = debug {
        loader.destroy_debug_utils_messenger(*messenger, None);
    }
    instance.destroy_instance(None);
}

/// The device extensions ray tracing takes.
const RAY_TRACING_EXTENSIONS: [&CStr; 3] = [
    ash::khr::acceleration_structure::NAME,
    ash::khr::ray_query::NAME,
    ash::khr::deferred_host_operations::NAME,
];

/// Whether `physical` offers the device extension `name`.
unsafe fn device_has(instance: &ash::Instance, physical: vk::PhysicalDevice, name: &CStr) -> bool {
    instance
        .enumerate_device_extension_properties(physical)
        .unwrap_or_default()
        .iter()
        .any(|extension| extension.extension_name_as_c_str() == Ok(name))
}

/// Whether `physical` traces rays: the acceleration structure and ray query
/// extensions with their features, buffer device addresses, and the
/// descriptor indexing an array of every texture needs.
unsafe fn ray_tracing_support(instance: &ash::Instance, physical: vk::PhysicalDevice) -> bool {
    let extensions = instance
        .enumerate_device_extension_properties(physical)
        .unwrap_or_default();
    let has = |name: &CStr| {
        extensions
            .iter()
            .any(|extension| extension.extension_name_as_c_str() == Ok(name))
    };
    if !RAY_TRACING_EXTENSIONS.iter().all(|name| has(name)) {
        return false;
    }
    let mut acceleration = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
    let mut ray_query = vk::PhysicalDeviceRayQueryFeaturesKHR::default();
    let mut features12 = vk::PhysicalDeviceVulkan12Features::default();
    {
        let mut features = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut acceleration)
            .push_next(&mut ray_query)
            .push_next(&mut features12);
        instance.get_physical_device_features2(physical, &mut features);
    }
    [
        acceleration.acceleration_structure,
        ray_query.ray_query,
        features12.buffer_device_address,
        features12.descriptor_indexing,
        features12.runtime_descriptor_array,
        features12.shader_sampled_image_array_non_uniform_indexing,
        features12.descriptor_binding_partially_bound,
        features12.descriptor_binding_variable_descriptor_count,
        features12.scalar_block_layout,
    ]
    .iter()
    .all(|feature| *feature == vk::TRUE)
}

/// A Vulkan 1.3 device with dynamic rendering and synchronization2, and a
/// queue family that draws (and presents to `surface`, unless it is null);
/// discrete devices first.
unsafe fn pick_device(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Option<(vk::PhysicalDevice, u32, String)> {
    let mut best: Option<(bool, vk::PhysicalDevice, u32, String)> = None;
    for physical in instance.enumerate_physical_devices().ok()? {
        let properties = instance.get_physical_device_properties(physical);
        if properties.api_version < vk::API_VERSION_1_3 {
            continue;
        }
        let mut features13 = vk::PhysicalDeviceVulkan13Features::default();
        let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut features13);
        instance.get_physical_device_features2(physical, &mut features);
        if features13.dynamic_rendering == vk::FALSE
            || features13.synchronization2 == vk::FALSE
            || features13.shader_demote_to_helper_invocation == vk::FALSE
        {
            continue;
        }
        let families = instance.get_physical_device_queue_family_properties(physical);
        let family = families.iter().enumerate().find_map(|(index, family)| {
            let index = index as u32;
            let graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let presents = surface == vk::SurfaceKHR::null()
                || surface_loader
                    .get_physical_device_surface_support(physical, index, surface)
                    .unwrap_or(false);
            (graphics && presents).then_some(index)
        });
        let Some(family) = family else {
            continue;
        };
        let discrete = properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU;
        let name = properties
            .device_name_as_c_str()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if best.as_ref().is_none_or(|(chosen, ..)| discrete && !chosen) {
            best = Some((discrete, physical, family, name));
        }
    }
    best.map(|(_, physical, family, name)| (physical, family, name))
}

unsafe fn graphics_pipeline(
    core: &Core,
    layout: vk::PipelineLayout,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
    color_format: vk::Format,
    depth: bool,
    key: Option<PipelineKey>,
) -> Result<vk::Pipeline, VkDeviceError> {
    let factors = key.and_then(|key| blend_factors(key.blend));
    graphics_pipeline_with(
        core,
        layout,
        [vertex, fragment],
        color_format,
        depth,
        key,
        factors,
    )
}

/// [`DeviceVertex`]'s size and its attributes' locations, formats and
/// offsets, as the primitives' vertex shaders read them.
const DEVICE_VERTEX: VertexLayout = (
    std::mem::size_of::<DeviceVertex>() as u32,
    &[
        (0, vk::Format::R32G32B32_SFLOAT, 0),
        (1, vk::Format::R32G32B32_SFLOAT, 12),
        (2, vk::Format::R8G8B8A8_UNORM, 24),
        (3, vk::Format::R8G8B8A8_UNORM, 28),
    ],
);

/// A vertex's size and its attributes' locations, formats and offsets.
type VertexLayout = (u32, &'static [(u32, vk::Format, u32)]);

/// [`graphics_pipeline`] blending by source and destination `factors`
/// (none: a copy) in place of its key's blend.
unsafe fn graphics_pipeline_with(
    core: &Core,
    layout: vk::PipelineLayout,
    modules: [vk::ShaderModule; 2],
    color_format: vk::Format,
    depth: bool,
    key: Option<PipelineKey>,
    factors: Option<(vk::BlendFactor, vk::BlendFactor)>,
) -> Result<vk::Pipeline, VkDeviceError> {
    graphics_pipeline_of(
        core,
        layout,
        modules,
        color_format,
        depth,
        key,
        factors,
        DEVICE_VERTEX,
    )
}

/// [`graphics_pipeline_with`] for vertices laid out as `vertex` says.
#[allow(clippy::too_many_arguments)]
unsafe fn graphics_pipeline_of(
    core: &Core,
    layout: vk::PipelineLayout,
    [vertex, fragment]: [vk::ShaderModule; 2],
    color_format: vk::Format,
    depth: bool,
    key: Option<PipelineKey>,
    factors: Option<(vk::BlendFactor, vk::BlendFactor)>,
    (stride, attributes): VertexLayout,
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
        .stride(stride)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes: Vec<_> = attributes
        .iter()
        .map(|&(location, format, offset)| {
            vk::VertexInputAttributeDescription::default()
                .location(location)
                .binding(0)
                .format(format)
                .offset(offset)
        })
        .collect();
    let vertex_input = if key.is_some() {
        vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes)
    } else {
        vk::PipelineVertexInputStateCreateInfo::default()
    };
    let topology = if key.is_some() {
        vk::PrimitiveTopology::TRIANGLE_LIST
    } else {
        vk::PrimitiveTopology::TRIANGLE_STRIP
    };
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(topology);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let (depth_test, depth_write, compare) = match key {
        Some(key) if key.depth_always => (true, true, vk::CompareOp::ALWAYS),
        Some(key) => (
            key.depth_test,
            key.depth_test && key.depth_write,
            vk::CompareOp::LESS_OR_EQUAL,
        ),
        None => (false, false, vk::CompareOp::LESS_OR_EQUAL),
    };
    let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(depth_test)
        .depth_write_enable(depth_write)
        .depth_compare_op(compare);
    let mut attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    if let Some((source, destination)) = factors {
        attachment = attachment
            .blend_enable(true)
            .src_color_blend_factor(source)
            .dst_color_blend_factor(destination)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(source)
            .dst_alpha_blend_factor(destination)
            .alpha_blend_op(vk::BlendOp::ADD);
    }
    let attachments = [attachment];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic);
    let formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&formats)
        .depth_attachment_format(if depth {
            DEPTH_FORMAT
        } else {
            vk::Format::UNDEFINED
        });
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
        .layout(layout)
        .push_next(&mut rendering);
    core.device
        .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
        .map(|pipelines| pipelines[0])
        .map_err(|(_, result)| call("pipeline creation")(result))
}

unsafe fn blit_pipeline(core: &Core, depth: bool) -> Result<vk::Pipeline, VkDeviceError> {
    graphics_pipeline(
        core,
        core.blit_layout,
        core.modules.blit_vertex,
        core.modules.blit_fragment,
        COLOR_FORMAT,
        depth,
        None,
    )
}

/// Source and destination factors; `None` is the CPU raster's copy.
/// How much farther a covered blended face is laid ([`VkFrame::draw_covered`]).
const COVERED_DEPTH: f32 = 1.0015;

fn blend_factors(blend: D3dTextureBlend) -> Option<(vk::BlendFactor, vk::BlendFactor)> {
    match blend {
        D3dTextureBlend::Copy => None,
        D3dTextureBlend::Additive => Some((vk::BlendFactor::ONE, vk::BlendFactor::ONE)),
        D3dTextureBlend::InverseSourceColor => {
            Some((vk::BlendFactor::ZERO, vk::BlendFactor::ONE_MINUS_SRC_COLOR))
        }
        D3dTextureBlend::SourceAlpha => Some((
            vk::BlendFactor::SRC_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        )),
    }
}

/// The sampler for a submission: point (1) or linear (2) filtering, wrapped
/// (1) or clamped (3), as the shared consumer admits them.
fn sampler_index(sampling: D3dTextureSampling) -> usize {
    usize::from(sampling.min_filter == 2) | (usize::from(sampling.address != 1) << 1)
}

/// The point-filtered, clamped sampler blits use.
const BLIT_SAMPLER: usize = 2;
/// The linearly filtered, clamped sampler: the traced world's colour comes up
/// from a smaller render size through it.
const SMOOTH_SAMPLER: usize = 3;

/// The target a frame shown `presented` large draws at, `scale` larger on
/// each side (supersampling), within the device's `largest` image side.
fn supersampled(presented: [u32; 2], scale: f32, largest: u32) -> [u32; 2] {
    let longest = presented[0].max(presented[1]).max(1) as f32;
    let scale = scale.min(largest as f32 / longest).max(1.0);
    presented.map(|side| (side as f32 * scale).round() as u32)
}

/// Top-down RGBA `pixels`, `from` large, averaged down to `to`: each pixel
/// the mean of those under it, each weighted by how much of it the pixel
/// covers, as `vk_downsample.frag` shows them.
fn downsampled(pixels: &[u8], from: [u32; 2], to: [u32; 2]) -> Vec<u8> {
    let scale = [0, 1].map(|axis| f64::from(from[axis]) / f64::from(to[axis].max(1)));
    // Each output row's or column's source texels and their weights.
    let spans = |axis: usize, index: u32| {
        let low = f64::from(index) * scale[axis];
        let high = low + scale[axis];
        (low.floor() as u32..(high.ceil() as u32).min(from[axis])).map(move |texel| {
            let texel_low = f64::from(texel);
            (
                texel,
                (high.min(texel_low + 1.0) - low.max(texel_low)).max(0.0),
            )
        })
    };
    let mut out = Vec::with_capacity(to[0] as usize * to[1] as usize * 4);
    for y in 0..to[1] {
        for x in 0..to[0] {
            let mut sum = [0.0_f64; 4];
            let mut total = 0.0;
            for (source_y, weight_y) in spans(1, y) {
                for (source_x, weight_x) in spans(0, x) {
                    let weight = weight_x * weight_y;
                    let at = (source_y as usize * from[0] as usize + source_x as usize) * 4;
                    for (channel, value) in sum.iter_mut().zip(&pixels[at..at + 4]) {
                        *channel += weight * f64::from(*value);
                    }
                    total += weight;
                }
            }
            out.extend(sum.map(|channel| (channel / total.max(1.0e-9)).round() as u8));
        }
    }
    out
}

impl State {
    fn fail(&mut self, error: &VkDeviceError) {
        if !self.failed {
            eprintln!("Infestation: Vulkan renderer stopped: {error}");
            self.failed = true;
        }
    }

    /// The command buffer being recorded, opening the frame slot's when
    /// nothing is: once the submission it last held is done.
    unsafe fn begin(&mut self, core: &Core) -> Result<vk::CommandBuffer, VkDeviceError> {
        if !self.recording {
            self.wait_slot(core, self.slot)?;
            let command = core.commands[self.slot];
            core.command.set(command);
            core.device
                .begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(call("command recording"))?;
            self.recording = true;
            self.vertex_chunk = 0;
            self.vertex_offset = 0;
            self.perf.opened(core, command, self.slot);
        }
        Ok(core.command.get())
    }

    unsafe fn end_rendering(&mut self, core: &Core) {
        if self.rendering.take().is_some() {
            core.device.cmd_end_rendering(core.command.get());
        }
    }

    /// Submit the recording (waiting on `wait` at the transfer stage and
    /// signalling `signal`, for presentation) without waiting for it; what
    /// it released waits with it. The next recording takes the next slot.
    unsafe fn submit(
        &mut self,
        core: &Core,
        semaphores: Option<(vk::Semaphore, vk::Semaphore)>,
    ) -> Result<(), VkDeviceError> {
        if !self.recording && semaphores.is_none() {
            return Ok(());
        }
        let command = self.begin(core)?;
        self.end_rendering(core);
        self.perf.closing(core, command, self.slot);
        core.device
            .end_command_buffer(command)
            .map_err(call("command recording"))?;
        self.recording = false;
        let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(command)];
        let waits;
        let signals;
        let mut submit = vk::SubmitInfo2::default().command_buffer_infos(&commands);
        if let Some((wait, signal)) = semaphores {
            waits = [vk::SemaphoreSubmitInfo::default()
                .semaphore(wait)
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
            signals = [vk::SemaphoreSubmitInfo::default()
                .semaphore(signal)
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
            submit = submit
                .wait_semaphore_infos(&waits)
                .signal_semaphore_infos(&signals);
        }
        core.device
            .queue_submit2(core.queue, &[submit], core.fences[self.slot])
            .map_err(call("queue submission"))?;
        self.in_flight[self.slot] = Some(Released {
            transient: std::mem::take(&mut self.transient),
            retired: std::mem::take(&mut self.retired),
            retired_sets: std::mem::take(&mut self.retired_sets),
            retired_accelerations: std::mem::take(&mut self.retired_accelerations),
        });
        self.slot = (self.slot + 1) % FRAMES_IN_FLIGHT;
        Ok(())
    }

    /// Wait for `slot`'s submission, if one is in flight, and release what
    /// it used.
    unsafe fn wait_slot(&mut self, core: &Core, slot: usize) -> Result<(), VkDeviceError> {
        let Some(released) = self.in_flight[slot].take() else {
            return Ok(());
        };
        let waiting = std::time::Instant::now();
        core.device
            .wait_for_fences(&[core.fences[slot]], true, u64::MAX)
            .map_err(call("fence wait"))?;
        self.perf.waited(core, slot, waiting.elapsed());
        core.device
            .reset_fences(&[core.fences[slot]])
            .map_err(call("fence reset"))?;
        self.release(core, released);
        Ok(())
    }

    /// Wait for every submission in flight: before the CPU rewrites what
    /// the last frame's GPU work reads (the tracer's and the passes'
    /// host-written buffers and descriptor sets) or reads the GPU's work.
    unsafe fn settle(&mut self, core: &Core) -> Result<(), VkDeviceError> {
        for slot in 0..FRAMES_IN_FLIGHT {
            self.wait_slot(core, slot)?;
        }
        Ok(())
    }

    /// Submit the recording and wait for it and every other submission.
    unsafe fn submit_and_wait(
        &mut self,
        core: &Core,
        semaphores: Option<(vk::Semaphore, vk::Semaphore)>,
    ) -> Result<(), VkDeviceError> {
        self.submit(core, semaphores)?;
        self.settle(core)
    }

    /// After a submission's wait: free its staging buffers, retired images
    /// and descriptor sets.
    unsafe fn release(&mut self, core: &Core, released: Released) {
        if let Some(loader) = core.acceleration_loader.as_ref() {
            for handle in released.retired_accelerations {
                loader.destroy_acceleration_structure(handle, None);
            }
        }
        let allocator = self
            .allocator
            .as_mut()
            .expect("allocator lives with the device");
        for buffer in released.transient {
            core.device.destroy_buffer(buffer.buffer, None);
            if let Some(allocation) = buffer.allocation {
                let _ = allocator.free(allocation);
            }
        }
        let mut sets = released.retired_sets;
        for image in released.retired {
            core.device.destroy_image_view(image.view, None);
            core.device.destroy_image(image.image, None);
            if let Some(allocation) = image.allocation {
                let _ = allocator.free(allocation);
            }
            sets.extend(image.set);
        }
        for descriptor in sets {
            let _ = core
                .device
                .free_descriptor_sets(descriptor.pool, &[descriptor.set]);
        }
    }
    unsafe fn buffer(
        &mut self,
        core: &Core,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Result<Buffer, VkDeviceError> {
        let buffer = core
            .device
            .create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size.max(4))
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
            .map_err(call("buffer creation"))?;
        let requirements = core.device.get_buffer_memory_requirements(buffer);
        let allocation = self
            .allocator
            .as_mut()
            .expect("allocator lives with the device")
            .allocate(&AllocationCreateDesc {
                name: "buffer",
                requirements,
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            });
        let allocation = match allocation {
            Ok(allocation) => allocation,
            Err(error) => {
                core.device.destroy_buffer(buffer, None);
                return Err(VkDeviceError::Memory(error.to_string()));
            }
        };
        core.device
            .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            .map_err(call("buffer binding"))?;
        Ok(Buffer {
            buffer,
            allocation: Some(allocation),
        })
    }

    unsafe fn image(
        &mut self,
        core: &Core,
        extent: [u32; 2],
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        sampled: bool,
    ) -> Result<Image, VkDeviceError> {
        let aspect = if format == DEPTH_FORMAT {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        };
        let image = core
            .device
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: extent[0].max(1),
                        height: extent[1].max(1),
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
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
                name: "image",
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
                    .format(format)
                    .subresource_range(range(aspect)),
                None,
            )
            .map_err(call("image view"))?;
        let mut built = Image {
            image,
            view,
            allocation: Some(allocation),
            extent,
            aspect,
            layout: Cell::new(vk::ImageLayout::UNDEFINED),
            set: None,
        };
        if sampled {
            built.set = Some(self.descriptor(core, view, core.samplers[BLIT_SAMPLER])?);
        }
        Ok(built)
    }

    unsafe fn descriptor(
        &mut self,
        core: &Core,
        view: vk::ImageView,
        sampler: vk::Sampler,
    ) -> Result<Descriptor, VkDeviceError> {
        let layouts = [core.texture_layout];
        let mut found = None;
        if let Some(pool) = self.pools.last() {
            found = core
                .device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(*pool)
                        .set_layouts(&layouts),
                )
                .ok()
                .map(|sets| Descriptor {
                    pool: *pool,
                    set: sets[0],
                });
        }
        let descriptor = match found {
            Some(descriptor) => descriptor,
            None => {
                let sizes = [vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(DESCRIPTOR_POOL_SETS)];
                let pool = core
                    .device
                    .create_descriptor_pool(
                        &vk::DescriptorPoolCreateInfo::default()
                            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                            .max_sets(DESCRIPTOR_POOL_SETS)
                            .pool_sizes(&sizes),
                        None,
                    )
                    .map_err(call("descriptor pool"))?;
                self.pools.push(pool);
                let set = core
                    .device
                    .allocate_descriptor_sets(
                        &vk::DescriptorSetAllocateInfo::default()
                            .descriptor_pool(pool)
                            .set_layouts(&layouts),
                    )
                    .map_err(call("descriptor set"))?[0];
                Descriptor { pool, set }
            }
        };
        let image = [vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor.set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image);
        core.device.update_descriptor_sets(&[write], &[]);
        Ok(descriptor)
    }

    /// Upload top-down RGBA into `image`, leaving it ready to sample.
    unsafe fn upload(
        &mut self,
        core: &Core,
        image: &Image,
        rgba: &[u8],
    ) -> Result<(), VkDeviceError> {
        let mut staging = self.buffer(
            core,
            rgba.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        let mapped = staging
            .allocation
            .as_mut()
            .and_then(|allocation| allocation.mapped_slice_mut())
            .ok_or_else(|| VkDeviceError::Memory("staging memory is not mapped".into()))?;
        mapped[..rgba.len()].copy_from_slice(rgba);
        let command = self.begin(core)?;
        self.end_rendering(core);
        transition(core, command, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
        let region = vk::BufferImageCopy::default()
            .image_subresource(layers(image.aspect))
            .image_extent(vk::Extent3D {
                width: image.extent[0],
                height: image.extent[1],
                depth: 1,
            });
        core.device.cmd_copy_buffer_to_image(
            command,
            staging.buffer,
            image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
        transition(
            core,
            command,
            image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        self.transient.push(staging);
        Ok(())
    }

    /// Read `image`'s pixels back, top row first, submitting and waiting for
    /// everything recorded so far.
    unsafe fn read_image(&mut self, core: &Core, image: &Image) -> Result<Vec<u8>, VkDeviceError> {
        let size = u64::from(image.extent[0]) * u64::from(image.extent[1]) * 4;
        let readback = self.buffer(
            core,
            size,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
        )?;
        let command = self.begin(core)?;
        self.end_rendering(core);
        transition(core, command, image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        let region = vk::BufferImageCopy::default()
            .image_subresource(layers(image.aspect))
            .image_extent(vk::Extent3D {
                width: image.extent[0],
                height: image.extent[1],
                depth: 1,
            });
        core.device.cmd_copy_image_to_buffer(
            command,
            image.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            readback.buffer,
            &[region],
        );
        // The wait below frees transient buffers, so this one stays out of
        // that list until it has been read.
        self.submit_and_wait(core, None)?;
        let pixels = readback
            .allocation
            .as_ref()
            .and_then(|allocation| allocation.mapped_slice())
            .map(|mapped| mapped[..size as usize].to_vec())
            .ok_or_else(|| VkDeviceError::Memory("readback memory is not mapped".into()));
        self.transient.push(readback);
        pixels
    }

    /// Copy `bytes` into the frame's vertex memory: its buffer and offset.
    unsafe fn vertices(
        &mut self,
        core: &Core,
        bytes: &[u8],
    ) -> Result<(vk::Buffer, u64), VkDeviceError> {
        let size = bytes.len() as u64;
        // The recording's slot: its last submission is done.
        self.begin(core)?;
        let slot = self.slot;
        loop {
            if let Some((buffer, mapped)) = self.vertex_chunks[slot].get(self.vertex_chunk) {
                let capacity = buffer
                    .allocation
                    .as_ref()
                    .map_or(0, |allocation| allocation.size());
                if self.vertex_offset + size <= capacity {
                    let offset = self.vertex_offset;
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        mapped.add(offset as usize),
                        bytes.len(),
                    );
                    self.vertex_offset += size;
                    return Ok((buffer.buffer, offset));
                }
                if self.vertex_offset > 0 {
                    self.vertex_chunk += 1;
                    self.vertex_offset = 0;
                    continue;
                }
            }
            let mut chunk = self.buffer(
                core,
                VERTEX_CHUNK.max(size),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
            )?;
            let mapped = chunk
                .allocation
                .as_mut()
                .and_then(|allocation| allocation.mapped_slice_mut())
                .map(|slice| slice.as_mut_ptr())
                .ok_or_else(|| VkDeviceError::Memory("vertex memory is not mapped".into()))?;
            self.vertex_chunks[slot].insert(self.vertex_chunk, (chunk, mapped));
            self.vertex_offset = 0;
        }
    }

    /// Begin dynamic rendering on `target` unless it is already active,
    /// keeping its contents.
    unsafe fn render_to(&mut self, core: &Core, target: &Target) -> Result<(), VkDeviceError> {
        let command = self.begin(core)?;
        if self.rendering == Some(target.color.image) {
            return Ok(());
        }
        self.end_rendering(core);
        transition(
            core,
            command,
            &target.color,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        transition(
            core,
            command,
            &target.depth,
            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
        );
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(target.color.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)];
        let depth = vk::RenderingAttachmentInfo::default()
            .image_view(target.depth.view)
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);
        let extent = target.color.extent;
        core.device.cmd_begin_rendering(
            command,
            &vk::RenderingInfo::default()
                .render_area(rect([0, 0], extent))
                .layer_count(1)
                .color_attachments(&color)
                .depth_attachment(&depth),
        );
        let viewport = self.raster_viewport.unwrap_or(extent);
        set_viewport(core, command, [0, 0], viewport, viewport);
        self.rendering = Some(target.color.image);
        Ok(())
    }

    unsafe fn pipeline(
        &mut self,
        core: &Core,
        key: PipelineKey,
    ) -> Result<vk::Pipeline, VkDeviceError> {
        if let Some(pipeline) = self.pipelines.get(&key) {
            return Ok(*pipeline);
        }
        let (vertex, fragment, layout) = match (key.textured, &self.tracer) {
            (true, Some(tracer)) if key.traced_fog => (
                tracer.raster_light_vertex(),
                core.modules.textured_fog,
                tracer.raster_fog().0,
            ),
            (true, _) => (
                core.modules.primitive,
                core.modules.textured,
                core.primitive_layout,
            ),
            _ => (
                core.modules.primitive,
                core.modules.flat,
                core.primitive_layout,
            ),
        };
        let pipeline = graphics_pipeline(
            core,
            layout,
            vertex,
            fragment,
            COLOR_FORMAT,
            true,
            Some(key),
        )?;
        self.pipelines.insert(key, pipeline);
        Ok(pipeline)
    }

    /// The device texture of `texture` and its descriptor for `sampler`.
    unsafe fn texture(
        &mut self,
        core: &Core,
        texture: &CpuTexture,
        sampler: usize,
    ) -> Result<vk::DescriptorSet, VkDeviceError> {
        let key = texture.content_key();
        let presents = self.presents;
        if !self.textures.contains_key(&key) {
            let image = self.image(
                core,
                [TEXTURE_SURFACE_SIZE as u32; 2],
                COLOR_FORMAT,
                vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                false,
            )?;
            self.upload(core, &image, texture.rgba().as_flattened())?;
            self.textures.insert(
                key,
                Texture {
                    image,
                    sets: [None; 4],
                    last_used: presents,
                },
            );
        }
        let (view, existing) = {
            let entry = self.textures.get_mut(&key).expect("inserted above");
            entry.last_used = presents;
            (entry.image.view, entry.sets[sampler])
        };
        if let Some(existing) = existing {
            return Ok(existing.set);
        }
        let descriptor = self.descriptor(core, view, core.samplers[sampler])?;
        self.textures.get_mut(&key).expect("inserted above").sets[sampler] = Some(descriptor);
        Ok(descriptor.set)
    }

    fn retire_idle_textures(&mut self) {
        self.presents += 1;
        let presents = self.presents;
        let idle: Vec<u64> = self
            .textures
            .iter()
            .filter(|(_, texture)| texture.last_used + TEXTURE_IDLE_PRESENTS < presents)
            .map(|(key, _)| *key)
            .collect();
        for key in idle {
            let texture = self.textures.remove(&key).expect("listed above");
            self.retired_sets.extend(texture.sets.into_iter().flatten());
            self.retired.push(texture.image);
        }
    }

    /// The output image of `extent`, made anew when the size changes.
    unsafe fn output(&mut self, core: &Core, extent: [u32; 2]) -> Result<(), VkDeviceError> {
        if self
            .output
            .as_ref()
            .is_some_and(|image| image.extent == extent)
        {
            return Ok(());
        }
        if let Some(old) = self.output.take() {
            self.retired.push(old);
        }
        let image = self.image(
            core,
            extent,
            COLOR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            false,
        )?;
        self.output = Some(image);
        Ok(())
    }

    unsafe fn present_swapchain(
        &mut self,
        core: &Core,
        drawable: [u32; 2],
    ) -> Result<(), VkDeviceError> {
        let loader = core
            .swapchain_loader
            .as_ref()
            .expect("a visible window has a swapchain loader");
        if self
            .swapchain
            .as_ref()
            .is_none_or(|swapchain| swapchain.stale || swapchain.extent != drawable)
        {
            self.submit_and_wait(core, None)?;
            self.recreate_swapchain(core, drawable)?;
        }
        let Some(output) = self.output.take() else {
            return self.submit(core, None);
        };
        // The slot's semaphore is free once its last submission is done.
        self.wait_slot(core, self.slot)?;
        let semaphore = core.acquired[self.slot];
        let acquiring = std::time::Instant::now();
        let acquired = {
            let swapchain = self.swapchain.as_ref().expect("made above");
            loader.acquire_next_image(swapchain.handle, u64::MAX, semaphore, vk::Fence::null())
        };
        self.perf.presented(acquiring.elapsed());
        let index = match acquired {
            Ok((index, suboptimal)) => {
                if suboptimal {
                    self.swapchain.as_mut().expect("made above").stale = true;
                }
                index as usize
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.swapchain.as_mut().expect("made above").stale = true;
                self.output = Some(output);
                return self.submit(core, None);
            }
            Err(result) => {
                self.output = Some(output);
                return Err(call("swapchain acquisition")(result));
            }
        };
        let command = self.begin(core)?;
        self.end_rendering(core);
        transition(
            core,
            command,
            &output,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let (image, extent, signal) = {
            let swapchain = self.swapchain.as_ref().expect("made above");
            (
                swapchain.images[index],
                swapchain.extent,
                swapchain.presented[index],
            )
        };
        raw_transition(
            core,
            command,
            image,
            vk::ImageAspectFlags::COLOR,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let corner = |size: [u32; 2]| vk::Offset3D {
            x: size[0] as i32,
            y: size[1] as i32,
            z: 1,
        };
        let region = vk::ImageBlit::default()
            .src_subresource(layers(vk::ImageAspectFlags::COLOR))
            .src_offsets([vk::Offset3D::default(), corner(output.extent)])
            .dst_subresource(layers(vk::ImageAspectFlags::COLOR))
            .dst_offsets([vk::Offset3D::default(), corner(extent)]);
        core.device.cmd_blit_image(
            command,
            output.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
            vk::Filter::NEAREST,
        );
        raw_transition(
            core,
            command,
            image,
            vk::ImageAspectFlags::COLOR,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        self.output = Some(output);
        self.submit(core, Some((semaphore, signal)))?;
        let swapchains = [self.swapchain.as_ref().expect("made above").handle];
        let indices = [index as u32];
        let waits = [signal];
        let presenting = std::time::Instant::now();
        let presented = loader.queue_present(
            core.queue,
            &vk::PresentInfoKHR::default()
                .wait_semaphores(&waits)
                .swapchains(&swapchains)
                .image_indices(&indices),
        );
        self.perf.presented(presenting.elapsed());
        match presented {
            Ok(false) => Ok(()),
            Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.swapchain.as_mut().expect("made above").stale = true;
                Ok(())
            }
            Err(result) => Err(call("presentation")(result)),
        }
    }

    unsafe fn recreate_swapchain(
        &mut self,
        core: &Core,
        drawable: [u32; 2],
    ) -> Result<(), VkDeviceError> {
        let loader = core
            .swapchain_loader
            .as_ref()
            .expect("a visible window has a swapchain loader");
        let capabilities = core
            .surface_loader
            .get_physical_device_surface_capabilities(core.physical, core.surface)
            .map_err(call("surface capabilities"))?;
        let formats = core
            .surface_loader
            .get_physical_device_surface_formats(core.physical, core.surface)
            .map_err(call("surface formats"))?;
        let format = formats
            .iter()
            .find(|format| {
                format.format == vk::Format::B8G8R8A8_UNORM
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| {
                formats
                    .iter()
                    .find(|format| format.format == vk::Format::R8G8B8A8_UNORM)
            })
            .or(formats.first())
            .copied()
            .ok_or(VkDeviceError::Surface(
                "the surface reports no formats".into(),
            ))?;
        let extent = if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else {
            vk::Extent2D {
                width: drawable[0].clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: drawable[1].clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        };
        let mut count = capabilities.min_image_count + 1;
        if capabilities.max_image_count > 0 {
            count = count.min(capabilities.max_image_count);
        }
        let old = self.swapchain.take();
        let handle = loader
            .create_swapchain(
                &vk::SwapchainCreateInfoKHR::default()
                    .surface(core.surface)
                    .min_image_count(count)
                    .image_format(format.format)
                    .image_color_space(format.color_space)
                    .image_extent(extent)
                    .image_array_layers(1)
                    .image_usage(
                        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::COLOR_ATTACHMENT,
                    )
                    .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .pre_transform(capabilities.current_transform)
                    .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                    .present_mode(vk::PresentModeKHR::FIFO)
                    .clipped(true)
                    .old_swapchain(
                        old.as_ref()
                            .map_or(vk::SwapchainKHR::null(), |old| old.handle),
                    ),
                None,
            )
            .map_err(call("swapchain creation"));
        if let Some(old) = old {
            for semaphore in old.presented {
                core.device.destroy_semaphore(semaphore, None);
            }
            loader.destroy_swapchain(old.handle, None);
        }
        let handle = handle?;
        let images = loader
            .get_swapchain_images(handle)
            .map_err(call("swapchain images"))?;
        let mut presented = Vec::with_capacity(images.len());
        for _ in &images {
            presented.push(
                core.device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(call("semaphore"))?,
            );
        }
        self.swapchain = Some(Swapchain {
            handle,
            images,
            extent: [extent.width, extent.height],
            presented,
            stale: false,
        });
        Ok(())
    }
}

/// The whole of an image: its one level and every layer (a cube's six).
fn range(aspect: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: aspect,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: vk::REMAINING_ARRAY_LAYERS,
    }
}

fn layers(aspect: vk::ImageAspectFlags) -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: aspect,
        mip_level: 0,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn rect(offset: [i32; 2], extent: [u32; 2]) -> vk::Rect2D {
    vk::Rect2D {
        offset: vk::Offset2D {
            x: offset[0],
            y: offset[1],
        },
        extent: vk::Extent2D {
            width: extent[0],
            height: extent[1],
        },
    }
}

/// A viewport over `extent` at `offset`, scissored to the whole `surface`.
unsafe fn set_viewport(
    core: &Core,
    command: vk::CommandBuffer,
    offset: [i32; 2],
    extent: [u32; 2],
    surface: [u32; 2],
) {
    core.device.cmd_set_viewport(
        command,
        0,
        &[vk::Viewport {
            x: offset[0] as f32,
            y: offset[1] as f32,
            width: extent[0] as f32,
            height: extent[1] as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }],
    );
    core.device
        .cmd_set_scissor(command, 0, &[rect([0, 0], surface)]);
}

/// A full barrier moving `image` to `layout`; the tracked layout follows.
unsafe fn transition(
    core: &Core,
    command: vk::CommandBuffer,
    image: &Image,
    layout: vk::ImageLayout,
) {
    let old = image.layout.get();
    if old == layout {
        return;
    }
    raw_transition(core, command, image.image, image.aspect, old, layout);
    image.layout.set(layout);
}

unsafe fn raw_transition(
    core: &Core,
    command: vk::CommandBuffer,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
) {
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .old_layout(old)
        .new_layout(new)
        .image(image)
        .subresource_range(range(aspect));
    core.device.cmd_pipeline_barrier2(
        command,
        &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier)),
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatchKey {
    pipeline: PipelineKey,
    set: vk::DescriptorSet,
    /// A metal face's fans, which mirror the sky ([`metal`]).
    metal: bool,
}

/// A batch held for after one of the device's passes: its key and its
/// triangles, retail's or (its key's `metal`) a metal face's.
type HeldBatch = (BatchKey, Vec<DeviceVertex>, Vec<metal::MetalVertex>);

/// A frame's offscreen colour and depth target on the Vulkan device, and the
/// pending triangles that share one device state. Submissions are in native
/// pixels; the target is `target` pixels, the native size or larger.
pub struct VkFrame {
    device: Rc<VkDevice>,
    native: [u32; 2],
    /// The size the frame is shown at. Under supersampling the target is
    /// larger, and [`Self::read_rgba`] averages it down to this.
    presented: [u32; 2],
    /// What a new target starts as.
    clear: [u8; 4],
    /// Presented since the last draw: the next draw first follows the
    /// player's supersampling.
    presented_since: bool,
    target: Option<Target>,
    batch: Vec<DeviceVertex>,
    key: Option<BatchKey>,
    /// The raster world being drawn under the player's anti-aliasing.
    raster_world: Option<RasterWorld>,
    /// The world's camera while its draws come in, traced or not: a struck
    /// crystal's spark finds its faces' corners in the view with it. The
    /// traced world's half-transparent crystals (Terra Nova's white clumps)
    /// are raster draws over the traced image too.
    world_camera: Option<crate::scene_feed::SceneCamera>,
    /// The moving model the fans come from now: its point in this view to
    /// the last view's.
    draw_motion: Option<crate::raster_motion::ViewMotion>,
    /// The spark the fans now are laid in again, added, each corner as
    /// near it as it lies (a struck crystal's).
    draw_tint: Option<crate::crystal_flares::CrystalSpark>,
    /// The fans now are a blended face a traced face drawn after it over
    /// the same corners covers but for its keyed texels.
    draw_covered: bool,
    /// The metal face the fans now draw.
    draw_metal: Option<Rc<crate::raster_metal::MetalFace>>,
    /// This raster world's metal mirrors the sky, wet by the rain as much
    /// as this says (the weather's `wet_models`).
    metal_world: Option<f32>,
    /// The metal faces' pending triangles, under `key`.
    metal_batch: Vec<metal::MetalVertex>,
    /// A metal fan's corners, kept between fans.
    metal_scratch: Vec<metal::MetalVertex>,
    /// The raster world's moving models' triangles, each corner with its
    /// motion, for the pass that writes it under their pixels.
    motion_batch: Vec<raster_aa::MotionVertex>,
    /// The fans now are the water's surface.
    draw_water: bool,
    /// The raster world whose water mirrors it, with how much.
    mirror_world: Option<(crate::scene_feed::SceneCamera, mirror::WaterMirror)>,
    /// That world's water triangles, for the mirror's mask.
    water_batch: Vec<raster_aa::MotionVertex>,
    /// And as retail laid them, each batch with its texture: the water's
    /// own colour.
    water_draws: raster_sky::BandBatch,
    /// The device shades this world's water itself (its mirror pass, with
    /// the world's frame block): retail's strips are kept, not drawn.
    water_shaded: bool,
    /// The colour those strips fog into.
    water_fog: [u8; 3],
    /// The water's plane from the eye (up), where the world has water.
    water_plane: Option<f32>,
    /// The fans now are retail's ground, with the colour a fogged colored
    /// quad mixed into its corners.
    draw_ground: Option<[u8; 3]>,
    /// The rain wets this raster world's ground: the ground's grid its
    /// wet ground and puddles read.
    ground_grid: Option<Rc<crate::scene_terrain::TerrainMesh>>,
    /// That world's ground, each corner with the colour its fog took and
    /// how much of it shows through ([`ground_corner`]), for the water's
    /// pass's ground image.
    ground_batch: Vec<DeviceVertex>,
    /// What came after the water's first strip, blended or without its
    /// depth, in order: drawn over the shaded water.
    late: Vec<HeldBatch>,
    /// In the rain, the rest of what came after the ground's first strip,
    /// blended or without its depth, in order: drawn over the wet ground,
    /// before the water is shaded, which mirrors it as dry.
    late_ground: Vec<HeldBatch>,
    /// Once the modern sky's backdrop is in, what is blended or without its
    /// depth (crystals, orbs, smoke), in order: drawn after the sky's world
    /// pass, which fades and clouds the ground by its depth and would take
    /// theirs as the ground's behind them.
    over_sky: Vec<HeldBatch>,
    /// The modern sky's backdrop is in this world.
    sky_begun: bool,
    /// The fans now are the modern sky's land band.
    draw_band: bool,
    /// The raster world whose modern sky the device draws, at the size its
    /// draws are made at.
    sky_world: Option<[u32; 2]>,
    /// That world's land, for the sky's mask.
    band_batch: raster_sky::BandBatch,
    /// This frame's world is under infrared vision: traced wherever the
    /// device traces rays, Ray Tracing on or off, so both Vulkan modes show
    /// the lamp at the eye alike.
    trace_infrared: bool,
    /// The modern raster's world is being drawn: its opaque draws write
    /// their depth even with Z Buffering off.
    raster_depth: bool,
}

/// A raster world drawn under the player's anti-aliasing.
#[derive(Clone, Copy)]
struct RasterWorld {
    camera: crate::scene_feed::SceneCamera,
    /// Its draws' offset this frame in target pixels.
    offset: [f32; 2],
    /// The last frame's camera, when the image is followed over frames and
    /// that frame was drawn: moving models' motion is taken from it.
    last: Option<crate::scene_feed::SceneCamera>,
}

impl fmt::Debug for VkFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VkFrame")
            .field("native", &self.native)
            .field("pending_vertices", &self.batch.len())
            .finish_non_exhaustive()
    }
}

impl VkFrame {
    /// A frame for `native`-pixel submissions shown at `presented` pixels,
    /// cleared to `clear` with depth 1. It draws at that size, or larger
    /// under the player's supersampling.
    pub(crate) fn new(
        device: &Rc<VkDevice>,
        native: [u32; 2],
        presented: [u32; 2],
        clear: [u8; 4],
    ) -> Result<Self, VkDeviceError> {
        let made = Self::target_of(device, device.frame_target(presented))?;
        let mut frame = Self {
            device: Rc::clone(device),
            native,
            presented,
            clear,
            presented_since: false,
            target: Some(made),
            batch: Vec::new(),
            key: None,
            raster_world: None,
            world_camera: None,
            draw_motion: None,
            draw_tint: None,
            draw_covered: false,
            draw_metal: None,
            metal_world: None,
            metal_batch: Vec::new(),
            metal_scratch: Vec::new(),
            motion_batch: Vec::new(),
            draw_water: false,
            mirror_world: None,
            water_batch: Vec::new(),
            water_draws: Vec::new(),
            water_shaded: false,
            water_fog: [0; 3],
            water_plane: None,
            draw_ground: None,
            ground_grid: None,
            ground_batch: Vec::new(),
            late: Vec::new(),
            late_ground: Vec::new(),
            over_sky: Vec::new(),
            sky_begun: false,
            draw_band: false,
            sky_world: None,
            band_batch: Vec::new(),
            trace_infrared: false,
            raster_depth: false,
        };
        frame.clear_color(clear);
        frame.clear_depth();
        Ok(frame)
    }

    /// A target of `size`, from the pool or made.
    fn target_of(device: &Rc<VkDevice>, size: [u32; 2]) -> Result<Target, VkDeviceError> {
        let core = &device.core;
        let mut state = device.state.borrow_mut();
        let pooled = state
            .targets
            .iter()
            .position(|pooled| pooled.color.extent == size);
        Ok(match pooled {
            Some(index) => state.targets.swap_remove(index),
            // SAFETY: the device is alive; the images are owned by the
            // target, which returns to the pool when the frame drops.
            None => unsafe {
                let color = state.image(
                    core,
                    size,
                    COLOR_FORMAT,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST,
                    true,
                )?;
                // The raster's anti-aliasing reads the world's depth.
                let depth = state.image(
                    core,
                    size,
                    DEPTH_FORMAT,
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                    false,
                )?;
                Target {
                    color,
                    depth,
                    surface: None,
                }
            },
        })
    }

    /// After a present, draw on at the player's supersampling: when it
    /// changed, a target of the new size, cleared, in place of the old,
    /// whose memory goes.
    fn follow_supersampling(&mut self) {
        if !std::mem::take(&mut self.presented_since) {
            return;
        }
        let wanted = self.device.frame_target(self.presented);
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.color.extent == wanted)
        {
            return;
        }
        let device = Rc::clone(&self.device);
        match Self::target_of(&device, wanted) {
            Ok(made) => {
                if let Some(old) = self.target.replace(made) {
                    let mut state = device.state.borrow_mut();
                    state.retired.extend([old.color, old.depth]);
                    state.retired.extend(old.surface);
                }
                self.clear_color(self.clear);
                self.clear_depth();
            }
            Err(error) => device.state.borrow_mut().fail(&error),
        }
    }

    /// Run `work` on the device state with this frame's target, logging and
    /// stopping the device on the first failure.
    fn with_state(
        &mut self,
        work: impl FnOnce(&mut State, &Core, &mut Target) -> Result<(), VkDeviceError>,
    ) {
        let device = Rc::clone(&self.device);
        let mut state = device.state.borrow_mut();
        if state.failed {
            return;
        }
        let target = self.target.as_mut().expect("a live frame has its target");
        if let Err(error) = work(&mut state, &device.core, target) {
            state.fail(&error);
        }
    }

    /// Append one clipped TL fan, drawing the pending batch first when the
    /// device state changes.
    pub(crate) fn fan(&mut self, polygon: &[D3dTlVertex], draw: &DeviceDraw<'_>) {
        if polygon.len() < 3 {
            return;
        }
        self.follow_supersampling();
        let mut set = vk::DescriptorSet::null();
        if let Some(texturing) = &draw.texture {
            let mut found = None;
            self.with_state(|state, core, _| {
                // SAFETY: recording happens on the device's own buffer.
                found = Some(unsafe {
                    state.texture(core, texturing.texture, sampler_index(texturing.sampling))?
                });
                Ok(())
            });
            let Some(found) = found else {
                return;
            };
            set = found;
        }
        // Over a traced world, its fog in place of retail's vertex fog.
        let traced_fog =
            self.device.state.borrow().traced_fog && draw.texture.is_some() && draw.depth_test;
        // The modern raster's world always has its depth.
        let depth_always =
            self.raster_depth && !draw.depth_test && draw.blend == D3dTextureBlend::Copy;
        let texturing = draw.texture.as_ref();
        let mut corners: Vec<DeviceVertex> = polygon
            .iter()
            .map(|vertex| device_vertex(vertex, texturing))
            .collect();
        // A metal face's solid fans, opaque (or keyed: blended by their
        // texels' alpha, at full, as retail draws its sprites, not half
        // transparent), each corner with what it takes of the face, for the
        // sky they mirror.
        let solid = (draw.depth_test && draw.depth_write) || depth_always;
        let opaque = match draw.blend {
            D3dTextureBlend::Copy => true,
            D3dTextureBlend::SourceAlpha => corners.iter().all(|corner| corner.diffuse[3] == 255),
            D3dTextureBlend::Additive | D3dTextureBlend::InverseSourceColor => false,
        };
        let wants_metal = self.draw_metal.is_some()
            && opaque
            && solid
            && !self.draw_water
            && self.draw_ground.is_none()
            && self.draw_tint.is_none();
        let mut metal = if wants_metal {
            std::mem::take(&mut self.metal_scratch)
        } else {
            Vec::new()
        };
        let is_metal = wants_metal && self.metal_corners(polygon, &corners, &mut metal);
        let key = BatchKey {
            pipeline: PipelineKey {
                textured: draw.texture.is_some(),
                blend: draw.blend,
                depth_test: draw.depth_test,
                depth_write: draw.depth_write,
                traced_fog,
                depth_always,
            },
            set,
            metal: is_metal,
        };
        if self.key != Some(key) {
            self.flush();
            self.key = Some(key);
        }
        if traced_fog {
            for corner in &mut corners {
                corner.fog = [0, 0, 0, 255];
            }
        }
        // Behind the face that covers it (the traced depth lies a
        // thousandth back, `raster_depth`) and before the face under that
        // one's keyed texels (a thousandth further, `settle_faces`): the
        // hangar's green glass shows through its windows alone.
        if self.draw_covered {
            for corner in &mut corners {
                corner.position[2] = 1.0 - (1.0 - corner.position[2]) / COVERED_DEPTH;
            }
        }
        if self.device.state.borrow().thermal_effects {
            for corner in &mut corners {
                match draw.blend {
                    D3dTextureBlend::SourceAlpha => corner.diffuse[3] /= 2,
                    D3dTextureBlend::InverseSourceColor => {
                        for channel in &mut corner.diffuse[..3] {
                            *channel /= 2;
                        }
                    }
                    D3dTextureBlend::Copy | D3dTextureBlend::Additive => {}
                }
            }
        }
        // The water the device shades itself: kept for its pass, not drawn
        // (the frame keeps the bed under it), its depth and fog for the
        // mask and its strips, as retail laid them, for the water's own
        // colour.
        if self.draw_water && self.water_shaded && (draw.depth_test || self.raster_depth) {
            let points: Vec<_> = polygon
                .iter()
                .zip(&corners)
                .map(|(vertex, corner)| raster_aa::water_vertex(vertex, corner.fog[3]))
                .collect();
            for edge in points[1..].windows(2) {
                self.water_batch.extend([points[0], edge[0], edge[1]]);
            }
            if corners[0].fog[3] < u8::MAX {
                self.water_fog = [corners[0].fog[0], corners[0].fog[1], corners[0].fog[2]];
            }
            if self.water_draws.last().is_none_or(|(last, _)| *last != set) {
                self.water_draws.push((set, Vec::new()));
            }
            let (_, triangles) = self.water_draws.last_mut().expect("pushed above");
            for edge in corners[1..].windows(2) {
                triangles.extend([corners[0], edge[0], edge[1]]);
            }
            return;
        }
        // Retail's ground in the rain, again for the water's pass; drawn in
        // the frame, never held for after it, though its textured fans are
        // blended: the pass wets them.
        let wet_ground = self.draw_ground.is_some() && self.ground_grid.is_some() && solid;
        if let (true, Some(fog)) = (wet_ground, self.draw_ground) {
            {
                let marked: Vec<_> = polygon
                    .iter()
                    .zip(&corners)
                    .map(|(vertex, corner)| {
                        ground_corner(vertex, *corner, texturing.is_some(), fog)
                    })
                    .collect();
                for edge in marked[1..].windows(2) {
                    self.ground_batch.extend([marked[0], edge[0], edge[1]]);
                }
            }
        }
        let held = (!wet_ground)
            .then(|| self.holds(&key.pipeline, polygon, solid))
            .flatten();
        if let Some(held) = held {
            self.defer(key, &corners, polygon, held, is_metal.then_some(&metal[..]));
        } else if is_metal {
            for edge in metal[1..].windows(2) {
                self.metal_batch.extend([metal[0], edge[0], edge[1]]);
            }
        } else {
            for edge in corners[1..].windows(2) {
                self.batch.extend([corners[0], edge[0], edge[1]]);
            }
        }
        if wants_metal {
            self.metal_scratch = metal;
        }
        // The modern sky's land again, for its mask.
        if self.draw_band && self.sky_world.is_some() && set != vk::DescriptorSet::null() {
            if self.band_batch.last().is_none_or(|(last, _)| *last != set) {
                self.band_batch.push((set, Vec::new()));
            }
            let (_, triangles) = self.band_batch.last_mut().expect("pushed above");
            for edge in corners[1..].windows(2) {
                triangles.extend([corners[0], edge[0], edge[1]]);
            }
        }
        // A struck crystal's faces again, their texture (or, untextured as
        // Terra Nova's white clumps are, their plain faces) in the spark's
        // light, added, each corner as near the spark's front as it lies:
        // it runs through the crystal from where the bolt struck.
        let spark = self.draw_tint.zip(self.world_camera);
        if let Some((spark, camera)) = spark {
            let added = BatchKey {
                pipeline: PipelineKey {
                    blend: D3dTextureBlend::Additive,
                    depth_write: false,
                    depth_always: false,
                    ..key.pipeline
                },
                set,
                metal: false,
            };
            let late = self.holds(&added.pipeline, polygon, false);
            if late.is_none() && self.key != Some(added) {
                self.flush();
                self.key = Some(added);
            }
            for (corner, vertex) in corners.iter_mut().zip(polygon.iter()) {
                let glow = raster_aa::view_point(vertex, &camera).map_or(0.0, |point| {
                    let away: f32 = (0..3)
                        .map(|axis| (point[axis] - spark.hit[axis]).powi(2))
                        .sum::<f32>()
                        .sqrt();
                    spark.glow(away)
                });
                let lit = spark
                    .light
                    .map(|channel| ((channel * glow).clamp(0.0, 1.0) * 255.0) as u8);
                corner.diffuse = [lit[0], lit[1], lit[2], 255];
            }
            if let Some(held) = late {
                self.defer(added, &corners, polygon, held, None);
            } else {
                for edge in corners[1..].windows(2) {
                    self.batch.extend([corners[0], edge[0], edge[1]]);
                }
            }
        }
        // A moving model's solid faces also write their own motion.
        if let (Some(motion), Some(world)) = (self.draw_motion, self.raster_world) {
            if let (Some(last), true) = (world.last, solid) {
                let corners: Option<Vec<_>> = polygon
                    .iter()
                    .map(|vertex| raster_aa::motion_vertex(vertex, &world.camera, &last, &motion))
                    .collect();
                if let Some(corners) = corners {
                    for edge in corners[1..].windows(2) {
                        self.motion_batch.extend([corners[0], edge[0], edge[1]]);
                    }
                }
            }
        }
    }

    /// Which of the device's own passes `polygon`, drawn keyed `key`,
    /// waits for, if any. Its water: once the world's first water strip is
    /// in (retail's painter laid everything after it over its water), what
    /// is laid over the world blended or without its depth (smoke, sparks,
    /// splashes) and reaches above the water's plane goes over the water;
    /// what lies wholly under the plane is the water's bed, seen through
    /// it. With the eye under the water the pass lays its murk over all the
    /// frame holds, so what reaches under the plane waits instead, whenever
    /// it comes, and takes the murk itself ([`Self::defer`]). In the rain
    /// the rest of what is so laid, once the ground's first strip is in,
    /// waits for the wet ground, which would darken it and mirror over it,
    /// and goes in before the water is shaded: the water mirrors what it
    /// mirrors dry.
    fn waits_for_water(&self, key: &PipelineKey, polygon: &[D3dTlVertex]) -> Option<Held> {
        let camera = self.world_camera;
        if !self.water_shaded {
            return None;
        }
        if holds_for_water(
            key,
            self.murk().is_some(),
            !self.water_batch.is_empty(),
            self.water_plane.filter(|_| camera.is_some()),
            polygon.iter().map(|vertex| {
                camera
                    .and_then(|camera| raster_aa::view_point(vertex, &camera))
                    .map(|point| point[1])
            }),
        ) {
            return Some(Held::Water);
        }
        // No plane: any such draw, wherever it lies.
        (!self.ground_batch.is_empty()
            && holds_for_water(key, false, true, None, std::iter::empty()))
        .then_some(Held::Ground)
    }

    /// Which pass `polygon`, keyed `key` (`solid`: with its depth), waits
    /// for: once the modern sky's backdrop is in, what is blended or
    /// without its depth waits for the sky's world pass ([`Held::Sky`]),
    /// but for what the water would lie over (as [`Self::waits_for_water`]
    /// takes it); else the water's.
    fn holds(&self, key: &PipelineKey, polygon: &[D3dTlVertex], solid: bool) -> Option<Held> {
        let water = self.waits_for_water(key, polygon);
        if solid || !self.sky_begun || self.sky_world.is_none() || self.murk().is_some() {
            return water;
        }
        let camera = self.world_camera;
        let over_water = !self.water_shaded
            || holds_for_water(
                key,
                false,
                true,
                self.water_plane.filter(|_| camera.is_some()),
                polygon.iter().map(|vertex| {
                    camera
                        .and_then(|camera| raster_aa::view_point(vertex, &camera))
                        .map(|point| point[1])
                }),
            );
        if over_water {
            Some(Held::Sky)
        } else {
            water
        }
    }

    /// With the eye under the water's live surface, the murk the device
    /// lays there.
    fn murk(&self) -> Option<Murk> {
        self.mirror_world.as_ref().and_then(|(_, look)| look.murk)
    }

    /// `corners`' fan (`polygon`'s), keyed `key`, held until the water is
    /// shaded, or the wet ground (`held`); under the water, seen through
    /// the murk over each corner's distance, as the pass lays it over what
    /// the frame holds. A metal face's fan is held as its `metal` corners
    /// (never under the water).
    fn defer(
        &mut self,
        key: BatchKey,
        corners: &[DeviceVertex],
        polygon: &[D3dTlVertex],
        held: Held,
        metal: Option<&[metal::MetalVertex]>,
    ) {
        let murk = self.murk().zip(self.world_camera);
        let textured = key.pipeline.textured;
        let fades = matches!(
            key.pipeline.blend,
            D3dTextureBlend::Additive | D3dTextureBlend::InverseSourceColor
        );
        let seen = |index: usize| match murk {
            Some((murk, camera)) => {
                let t = raster_aa::view_point(&polygon[index], &camera).map_or(0.0, |point| {
                    point.iter().map(|axis| axis * axis).sum::<f32>().sqrt()
                });
                murk.through(corners[index], t, textured, fades)
            }
            None => corners[index],
        };
        let late = match held {
            Held::Water => &mut self.late,
            Held::Ground => &mut self.late_ground,
            Held::Sky => &mut self.over_sky,
        };
        if late.last().is_none_or(|(last, ..)| *last != key) {
            late.push((key, Vec::new(), Vec::new()));
        }
        let (_, triangles, metal_triangles) = late.last_mut().expect("pushed above");
        if let Some(metal) = metal {
            for edge in metal[1..].windows(2) {
                metal_triangles.extend([metal[0], edge[0], edge[1]]);
            }
            return;
        }
        let first = seen(0);
        let mut last = seen(1);
        for index in 2..corners.len() {
            let next = seen(index);
            triangles.extend([first, last, next]);
            last = next;
        }
    }

    /// The moving model the next fans come from, or none.
    pub(crate) fn set_draw_motion(&mut self, motion: Option<crate::raster_motion::ViewMotion>) {
        self.draw_motion = motion;
    }

    /// Whether the next fans are the water's surface.
    pub(crate) fn set_draw_water(&mut self, water: bool) {
        self.draw_water = water;
    }

    /// Whether the next fans are retail's ground, with the colour a fogged
    /// colored quad mixed into its corners.
    pub(crate) fn set_draw_ground(&mut self, ground: Option<[u8; 3]>) {
        self.draw_ground = ground;
    }

    /// Whether the next fans are the modern sky's land band.
    pub(crate) fn set_draw_band(&mut self, band: bool) {
        self.draw_band = band;
    }

    /// Whether the device draws this raster world's modern sky.
    pub(crate) fn draws_raster_sky(&self) -> bool {
        self.sky_world.is_some()
    }

    /// The painting is drawn: the modern sky goes behind its land and art,
    /// before the world's draws.
    pub(crate) fn draw_raster_sky(&mut self) {
        let Some(render) = self.sky_world else {
            return;
        };
        self.sky_begun = true;
        self.flush();
        let band = std::mem::take(&mut self.band_batch);
        let native = self.native;
        let offset = self.raster_world.map_or([0.0; 2], |world| world.offset);
        // SAFETY: the pass records on the device's own buffer.
        self.with_state(|state, core, target| unsafe {
            state.raster_sky_backdrop(core, target, &band, native, render, offset)
        });
    }

    /// The colour the next fans are laid in again, added, or none.
    pub(crate) fn set_draw_tint(&mut self, tint: Option<crate::crystal_flares::CrystalSpark>) {
        self.draw_tint = tint;
    }

    pub(crate) fn set_draw_covered(&mut self, covered: bool) {
        self.draw_covered = covered;
    }

    /// The metal face the next fans draw, or none.
    pub(crate) fn set_draw_metal(&mut self, metal: Option<Rc<crate::raster_metal::MetalFace>>) {
        self.draw_metal = metal;
    }

    /// Whether this raster world's metal mirrors the sky.
    pub(crate) fn reflects_metal(&self) -> bool {
        self.metal_world.is_some()
    }

    /// The metal face's corners for the fan `polygon` (drawn as `corners`)
    /// into `out`, each with what it takes of the face where it lies; false
    /// (and `out` empty) where the fans draw no metal face, or it shows
    /// nothing of its own this frame (dry rubber), or a corner lies on none
    /// of its triangles.
    fn metal_corners(
        &self,
        polygon: &[D3dTlVertex],
        corners: &[DeviceVertex],
        out: &mut Vec<metal::MetalVertex>,
    ) -> bool {
        out.clear();
        let made = self
            .draw_metal
            .as_deref()
            .zip(self.metal_world)
            .zip(self.world_camera.as_ref())
            .and_then(|((face, wet), camera)| {
                metal::fan_corners(face, wet, polygon, corners, camera, out)
            })
            .is_some();
        if !made {
            out.clear();
        }
        made
    }

    /// A traced world's camera while its draws come in, or none once it is
    /// done: its half-transparent crystals are raster draws over the traced
    /// image and find their spark with it. An untraced world's is set by
    /// [`Self::begin_raster_world`].
    pub(crate) fn set_world_camera(&mut self, camera: Option<crate::scene_feed::SceneCamera>) {
        self.world_camera = camera;
    }

    /// The world's camera while its draws come in, traced or raster.
    pub(crate) fn world_camera(&self) -> Option<crate::scene_feed::SceneCamera> {
        self.world_camera
    }

    fn flush(&mut self) {
        if !self.metal_batch.is_empty() {
            let key = self.key.expect("pending vertices have a device state");
            let batch = std::mem::take(&mut self.metal_batch);
            let native = self.native;
            let jitter = self.raster_world.map_or([0.0; 2], |world| world.offset);
            // SAFETY: the draw records on the device's own buffer.
            self.with_state(|state, core, target| unsafe {
                state.draw_metal(
                    core,
                    target,
                    (key.pipeline, key.set),
                    &batch,
                    native,
                    jitter,
                )
            });
            let mut batch = batch;
            batch.clear();
            self.metal_batch = batch;
        }
        if self.batch.is_empty() {
            return;
        }
        let key = self.key.expect("pending vertices have a device state");
        let batch = std::mem::take(&mut self.batch);
        let native = self.native;
        let jitter = self.raster_world.map_or([0.0; 2], |world| world.offset);
        self.with_state(|state, core, target| unsafe {
            state.render_to(core, target)?;
            let pipeline = state.pipeline(core, key.pipeline)?;
            let fog = state
                .tracer
                .as_ref()
                .filter(|_| key.pipeline.traced_fog)
                .map(|tracer| tracer.raster_fog());
            let layout = fog.map_or(core.primitive_layout, |(layout, _)| layout);
            // SAFETY: DeviceVertex is plain repr(C) data without padding.
            let bytes = std::slice::from_raw_parts(
                batch.as_ptr().cast::<u8>(),
                std::mem::size_of_val(batch.as_slice()),
            );
            let (buffer, offset) = state.vertices(core, bytes)?;
            let command = core.command.get();
            core.device
                .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, pipeline);
            let mut push = [0_u8; PRIMITIVE_PUSH];
            push[0..4].copy_from_slice(&(2.0 / native[0] as f32).to_le_bytes());
            push[4..8].copy_from_slice(&(2.0 / native[1] as f32).to_le_bytes());
            let depth_test: f32 = if key.pipeline.depth_test { 1.0 } else { 0.0 };
            push[8..12].copy_from_slice(&depth_test.to_le_bytes());
            // An additive or darkening draw fades to nothing in the fog,
            // the others into its colour.
            let fades = matches!(
                key.pipeline.blend,
                D3dTextureBlend::Additive | D3dTextureBlend::InverseSourceColor
            );
            push[12..16].copy_from_slice(&(if fades { 2.0_f32 } else { 1.0 }).to_le_bytes());
            // The jitter in NDC: pixels times 2 / the size drawn at.
            let extent = state.raster_viewport.unwrap_or(target.color.extent);
            push[16..20].copy_from_slice(&(2.0 * jitter[0] / extent[0] as f32).to_le_bytes());
            push[20..24].copy_from_slice(&(2.0 * jitter[1] / extent[1] as f32).to_le_bytes());
            core.device.cmd_push_constants(
                command,
                layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &push,
            );
            if key.pipeline.textured {
                core.device.cmd_bind_descriptor_sets(
                    command,
                    vk::PipelineBindPoint::GRAPHICS,
                    layout,
                    0,
                    &[key.set],
                    &[],
                );
            }
            if let Some((layout, set)) = fog {
                core.device.cmd_bind_descriptor_sets(
                    command,
                    vk::PipelineBindPoint::GRAPHICS,
                    layout,
                    1,
                    &[set],
                    &[],
                );
            }
            core.device
                .cmd_bind_vertex_buffers(command, 0, &[buffer], &[offset]);
            core.device.cmd_draw(command, batch.len() as u32, 1, 0, 0);
            Ok(())
        });
        let mut batch = batch;
        batch.clear();
        self.batch = batch;
    }

    fn clear(&mut self, aspect: vk::ImageAspectFlags, value: vk::ClearValue) {
        self.follow_supersampling();
        self.flush();
        self.with_state(|state, core, target| unsafe {
            state.render_to(core, target)?;
            let attachment = vk::ClearAttachment {
                aspect_mask: aspect,
                color_attachment: 0,
                clear_value: value,
            };
            let area = vk::ClearRect {
                rect: rect([0, 0], target.color.extent),
                base_array_layer: 0,
                layer_count: 1,
            };
            core.device
                .cmd_clear_attachments(core.command.get(), &[attachment], &[area]);
            Ok(())
        });
    }

    pub(crate) fn clear_color(&mut self, rgba: [u8; 4]) {
        let float32 = rgba.map(|channel| f32::from(channel) / 255.0);
        self.clear(
            vk::ImageAspectFlags::COLOR,
            vk::ClearValue {
                color: vk::ClearColorValue { float32 },
            },
        );
    }

    pub(crate) fn clear_depth(&mut self) {
        self.clear(
            vk::ImageAspectFlags::DEPTH,
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        );
    }

    /// The size the frame is shown at, and of [`Self::read_rgba`]'s image.
    pub(crate) fn target(&self) -> [u32; 2] {
        self.presented
    }

    /// Replace every colour pixel with top-down native-sized RGBA, keeping
    /// depth. A larger target shows it enlarged with nearest sampling.
    pub(crate) fn replace_surface(&mut self, rgba: &[u8]) {
        debug_assert_eq!(
            rgba.len(),
            self.native[0] as usize * self.native[1] as usize * 4
        );
        self.follow_supersampling();
        self.flush();
        let native = self.native;
        self.with_state(|state, core, target| unsafe {
            if target.surface.is_none() {
                target.surface = Some(state.image(
                    core,
                    native,
                    COLOR_FORMAT,
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    true,
                )?);
            }
            let surface = target.surface.as_ref().expect("made above");
            state.upload(core, surface, rgba)?;
            state.render_to(core, target)?;
            let surface = target.surface.as_ref().expect("made above");
            blit(
                core,
                state.blit_target,
                surface.set.expect("sampled"),
                false,
            );
            Ok(())
        });
    }

    /// Trace `feed` into the frame: its colour and depth replace the
    /// target's, under the raster's later draws. A device that cannot trace
    /// rays leaves the frame as it is.
    pub(crate) fn trace_world(
        &mut self,
        feed: &crate::scene_feed::SceneFeed,
        textures: &[&CpuTexture],
    ) -> bool {
        if !self.traces_rays() {
            return false;
        }
        self.follow_supersampling();
        self.flush();
        // SAFETY: the tracer records on the device's own buffer.
        self.with_state(|state, core, target| unsafe { state.trace(core, target, feed, textures) });
        true
    }

    /// Make the tracer and its upscaler ahead of the first traced frame,
    /// where this frame's world would be traced: nothing once they are.
    pub(crate) fn prepare_trace(&mut self) {
        if !self.traces_rays() {
            return;
        }
        // SAFETY: the tracer and the upscaler record on the device's own
        // buffer.
        self.with_state(|state, core, target| unsafe { state.prepare_trace(core, target) });
    }

    /// The traced world's last draw is in: its lighting preset's grade over
    /// it, before the HUD, which its fog leaves.
    pub(crate) fn finish_traced_world(&mut self) {
        self.device.state.borrow_mut().traced_fog = false;
        self.device.state.borrow_mut().thermal_effects = false;
        if self.device.state.borrow().graded.is_none() {
            return;
        }
        self.flush();
        // SAFETY: the grade records on the device's own buffer.
        self.with_state(|state, core, target| unsafe { state.finish_grade(core, target) });
    }

    /// Thermal vision's camera over the world drawn so far, before the
    /// HUD: infrared vision under Vulkan.
    pub(crate) fn finish_thermal(&mut self, ironbow: bool) {
        self.flush();
        self.device.state.borrow_mut().thermal_effects = false;
        let tuning = crate::modern_tuning::current();
        // SAFETY: the camera records on the device's own buffer.
        self.with_state(|state, core, target| unsafe {
            state.finish_thermal(core, target, tuning.thermal(), ironbow)
        });
    }

    /// The world's draws begin, with `camera`: on a frame whose world is
    /// not traced, the player's anti-aliasing then moves them a sub-pixel
    /// each frame where it follows the image over frames. True when it does:
    /// moving models then report their own motion. Its water mirrors the
    /// world with the least share `water_reflect` (none: lava's), lit by
    /// the world's `light` (its light alone: the frame block the water and
    /// the sky take), and where the device can and `sky` asks, the modern
    /// sky takes the painting's place: the frame starts black, the painting
    /// draws only its land and the art the sky keeps, and the sky goes
    /// behind them before the world's draws. With the eye under the water's
    /// live surface, the water's pass shows the view from there through the
    /// `murk`, where the device shades the water.
    pub(crate) fn begin_raster_world(
        &mut self,
        camera: &crate::scene_feed::SceneCamera,
        water_reflect: Option<f32>,
        murk: Option<Murk>,
        light: Option<&crate::scene_feed::SceneFeed>,
        sky: bool,
    ) -> bool {
        self.motion_batch.clear();
        self.water_batch.clear();
        self.water_draws.clear();
        self.ground_batch.clear();
        self.ground_grid = None;
        self.water_shaded = false;
        self.late.clear();
        self.late_ground.clear();
        self.over_sky.clear();
        self.sky_begun = false;
        self.band_batch.clear();
        self.sky_world = None;
        self.raster_depth = false;
        self.world_camera = Some(*camera);
        self.mirror_world = None;
        self.metal_world = None;
        if self.traces_rays() {
            return false;
        }
        self.flush();
        self.raster_depth = true;
        // As the tracer's water: its tuned share times the preset's
        // reflections, at least water's own, and the preset's gain.
        let reflections = {
            let state = self.device.state.borrow();
            crate::modern_tuning::current()
                .lighting(state.modern.lighting)
                .reflections
                .max(0.0)
        };
        self.mirror_world = water_reflect.map(|reflect| {
            let look = mirror::WaterMirror {
                least: (reflect * reflections).clamp(0.0, 1.0).max(0.02),
                gain: reflections.max(1.0),
                murk: None,
            };
            (*camera, look)
        });
        let mut begun = None;
        self.with_state(|state, core, target| {
            begun = state.begin_raster(core, target.color.extent)?;
            Ok(())
        });
        self.raster_world = begun.map(|begun| RasterWorld {
            camera: *camera,
            offset: begun.offset,
            last: begun.last,
        });
        if let Some(feed) = light {
            let mut render = None;
            let offset = self.raster_world.map_or([0.0; 2], |world| world.offset);
            // SAFETY: the sky's passes record on the device's own buffer.
            self.with_state(|state, core, target| unsafe {
                let display = target.color.extent;
                let size = state.raster_viewport.unwrap_or(display);
                if state.begin_raster_sky(core, feed, display, size, offset, sky)? {
                    render = Some(size);
                }
                Ok(())
            });
            if render.is_some() {
                self.clear_color([0, 0, 0, 255]);
                self.sky_world = render;
            }
        }
        // The water's pass takes the world's frame block, written above.
        self.water_plane = light
            .and_then(|feed| feed.water_level)
            .map(|level| level - camera.eye[1] as f32);
        self.water_shaded = self.mirror_world.is_some() && {
            let state = self.device.state.borrow();
            state
                .raster_sky
                .as_ref()
                .is_some_and(|sky| sky.light(state.slot).is_some())
        };
        // The world draw asks for the murk only where the device shades the
        // water ([`Self::shades_water`]): the frame block goes unwritten only
        // on a device error, which stops the renderer (logged once).
        debug_assert!(murk.is_none() || self.water_shaded || self.device.state.borrow().failed);
        if let Some((_, look)) = &mut self.mirror_world {
            look.murk = murk.filter(|_| self.water_shaded);
        }
        // The rain wets the ground in the water's pass, above the water;
        // its grid goes up from the world's load, not when the rain first
        // wets it.
        if let Some(feed) = light {
            if self.water_shaded && self.murk().is_none() {
                self.ground_grid = feed.raster_wet_ground().cloned();
            }
            if let Some(mesh) = &feed.raster_ground {
                // SAFETY: the upload records on the device's own buffer.
                self.with_state(|state, core, _| unsafe { state.upload_ground_grid(core, mesh) });
            }
        }
        // Metal mirrors the sky where the world's frame block is written,
        // the eye above the water, out of infrared vision.
        let written = {
            let state = self.device.state.borrow();
            state
                .raster_sky
                .as_ref()
                .is_some_and(|sky| sky.light(state.slot).is_some())
        };
        self.metal_world = light
            .filter(|feed| written && murk.is_none() && !feed.infrared)
            .map(|feed| feed.weather.map_or(0.0, |weather| weather.wet_models));
        if self.metal_world.is_some() {
            self.with_state(|state, _, _| {
                state.begin_raster_metal();
                Ok(())
            });
        }
        begun.is_some_and(|begun| begun.follows)
    }

    /// Whether the device shades an untraced world's water itself (not
    /// lava, which [`Self::begin_raster_world`] is told of): Vulkan's
    /// raster, whose water's pass takes the frame block, which needs the
    /// scalar block layout. The world's begin writes that block; where it
    /// cannot, the device has failed and draws nothing more, so a world
    /// asked to show the murk never shows the view from above in its place.
    /// A capability, not this frame's outcome: the look under the water
    /// decides the game's fade.
    pub(crate) fn shades_water(&self) -> bool {
        !self.traces_rays() && self.device.core.scalar_layout
    }

    /// The raster world's `light` again, now with the vehicles' lamps the
    /// light pass kept: where the device shades the water, its frame block
    /// is written anew for the water's pass.
    pub(crate) fn raster_lamps(&mut self, light: &crate::scene_feed::SceneFeed) {
        if self.water_shaded {
            self.with_state(|state, _, _| state.raster_lamps(light));
        }
    }

    /// The world's last draw is in: mirror it in its water (under the
    /// water, lay the murk over it and show the surface from below),
    /// anti-alias it, lay the modern sky's clouds and weather over it and
    /// grade it, before the HUD's, and draw on unmoved.
    pub(crate) fn finish_raster_world(&mut self) {
        self.world_camera = None;
        self.flush();
        self.raster_depth = false;
        self.metal_world = None;
        let mirror = self.mirror_world.take();
        let under_water = mirror.is_some_and(|(_, look)| look.murk.is_some());
        let water = std::mem::take(&mut self.water_batch);
        let water_draws = std::mem::take(&mut self.water_draws);
        let ground = std::mem::take(&mut self.ground_batch);
        let grid = self.ground_grid.take();
        self.water_shaded = false;
        let water_fog = self.water_fog.map(|channel| f32::from(channel) / 255.0);
        let late = std::mem::take(&mut self.late);
        let mut late_ground = std::mem::take(&mut self.late_ground);
        let sky = self.sky_world.take();
        self.band_batch.clear();
        let lighting = {
            let state = self.device.state.borrow();
            crate::modern_tuning::current().lighting(state.modern.lighting)
        };
        let graded = lighting.grades();
        if self.raster_world.is_none() && mirror.is_none() && sky.is_none() && !graded {
            return;
        }
        self.flush();
        let offset = self.raster_world.map_or([0.0; 2], |world| world.offset);
        // The water mirrors the world first, so the anti-aliasing takes it;
        // under the water its pass runs with no surface in view too, and
        // in the rain for the wet ground. With water in view the ground is
        // wet first and the draws held over it laid over it, and then the
        // water shaded, mirroring them and the wet ground.
        let wet = grid.as_deref().filter(|_| !ground.is_empty());
        if let (Some((camera, look)), false) =
            (mirror, water.is_empty() && !under_water && wet.is_none())
        {
            let stage = if wet.is_some() && !water.is_empty() {
                mirror::MirrorStage::GroundFirst
            } else {
                mirror::MirrorStage::Whole
            };
            // SAFETY: the pass records on the device's own buffer.
            let native = self.native;
            self.with_state(|state, core, target| unsafe {
                state.finish_mirror(
                    core,
                    target,
                    &camera,
                    look,
                    offset,
                    &water,
                    (&water_draws, water_fog),
                    wet.map(|grid| (ground.as_slice(), grid)),
                    native,
                    stage,
                )
            });
            if stage == mirror::MirrorStage::GroundFirst {
                self.draw_late(std::mem::take(&mut late_ground));
                // SAFETY: as above.
                self.with_state(|state, core, target| unsafe {
                    state.finish_mirror(
                        core,
                        target,
                        &camera,
                        look,
                        offset,
                        &water,
                        (&water_draws, water_fog),
                        None,
                        native,
                        mirror::MirrorStage::WaterAfter,
                    )
                });
            }
        }
        // What was laid over the wet ground and over the water, over them
        // again.
        self.draw_late(late_ground);
        self.draw_late(late);
        // The sky's world pass over the ground as drawn, then the blended
        // draws held for it over that.
        let over_sky = std::mem::take(&mut self.over_sky);
        self.sky_begun = false;
        if let Some(render) = sky {
            // SAFETY: the pass records on the device's own buffer.
            self.with_state(|state, core, target| unsafe {
                state.finish_raster_sky(core, target, render, raster_sky::WORLD_PASS, under_water)
            });
        }
        self.draw_late(over_sky);
        if let Some(world) = self.raster_world.take() {
            let moving = std::mem::take(&mut self.motion_batch);
            // SAFETY: the pass records on the device's own buffer.
            self.with_state(|state, core, target| unsafe {
                state.finish_raster(core, target, &world.camera, world.offset, &moving)
            });
        }
        // The modern sky's arcs and its snow or rain over it all, at the
        // display's size; none under the water.
        if let Some(render) = sky {
            // SAFETY: the pass records on the device's own buffer.
            self.with_state(|state, core, target| unsafe {
                state.finish_raster_sky(core, target, render, raster_sky::OVER_PASS, under_water)
            });
        }
        // The lighting preset's grade over it all, before the HUD, as over
        // a traced world.
        if graded {
            // SAFETY: the grade records on the device's own buffer.
            self.with_state(|state, core, target| unsafe {
                state.graded = Some(lighting);
                state.finish_grade(core, target)
            });
        }
    }

    /// Draw `late`'s held draws, in order.
    fn draw_late(&mut self, late: Vec<HeldBatch>) {
        for (key, triangles, metal) in late {
            self.key = Some(key);
            self.batch = triangles;
            self.metal_batch = metal;
            self.flush();
        }
    }

    /// Whether this frame's world is traced: a device that traces rays,
    /// with the player's Ray Tracing on, or under infrared vision.
    pub(crate) fn traces_rays(&self) -> bool {
        self.device.ray_tracing()
            && (self.device.state.borrow().modern.ray_tracing || self.trace_infrared)
    }

    /// Whether the world drawn next is under infrared vision.
    pub(crate) fn set_trace_infrared(&mut self, on: bool) {
        self.trace_infrared = on;
    }

    /// The frame's pixels at the size it is shown at, top row first: a
    /// supersampled frame averaged down as the screen shows it.
    pub(crate) fn read_rgba(&mut self) -> Vec<u8> {
        self.flush();
        let drawn = self
            .target
            .as_ref()
            .map_or([0, 0], |target| target.color.extent);
        let mut pixels = vec![0; drawn[0] as usize * drawn[1] as usize * 4];
        self.with_state(|state, core, target| unsafe {
            pixels = state.read_image(core, &target.color)?;
            Ok(())
        });
        if drawn == self.presented {
            pixels
        } else {
            downsampled(&pixels, drawn, self.presented)
        }
    }

    /// Copy the frame's pixels on the device for a snapshot, without
    /// waiting: [`StagedRgba::read`] reads them back, as [`Self::read_rgba`]
    /// would have now, when the snapshot is saved. Each copy replaces the
    /// last.
    pub(crate) fn stage_rgba(&mut self) -> StagedRgba {
        self.flush();
        let drawn = self
            .target
            .as_ref()
            .map_or([0, 0], |target| target.color.extent);
        self.with_state(|state, core, target| unsafe {
            let size = u64::from(drawn[0]) * u64::from(drawn[1]) * 4;
            if state
                .staged
                .as_ref()
                .is_none_or(|(_, extent)| *extent != drawn)
            {
                if let Some((old, _)) = state.staged.take() {
                    state.transient.push(old);
                }
                let buffer = state.buffer(
                    core,
                    size,
                    vk::BufferUsageFlags::TRANSFER_DST,
                    MemoryLocation::GpuToCpu,
                )?;
                state.staged = Some((buffer, drawn));
            }
            let command = state.begin(core)?;
            state.end_rendering(core);
            let barrier = |src_access, dst_stage, dst_access| {
                let memory = [vk::MemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::COPY)
                    .src_access_mask(src_access)
                    .dst_stage_mask(dst_stage)
                    .dst_access_mask(dst_access)];
                core.device.cmd_pipeline_barrier2(
                    command,
                    &vk::DependencyInfo::default().memory_barriers(&memory),
                );
            };
            // The last copy into the buffer, perhaps still in flight, first.
            barrier(
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_WRITE,
            );
            transition(
                core,
                command,
                &target.color,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            let region = vk::BufferImageCopy::default()
                .image_subresource(layers(target.color.aspect))
                .image_extent(vk::Extent3D {
                    width: drawn[0],
                    height: drawn[1],
                    depth: 1,
                });
            let (buffer, _) = state.staged.as_ref().expect("made above");
            core.device.cmd_copy_image_to_buffer(
                command,
                target.color.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer.buffer,
                &[region],
            );
            barrier(
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::PipelineStageFlags2::HOST,
                vk::AccessFlags2::HOST_READ,
            );
            Ok(())
        });
        StagedRgba {
            device: Rc::clone(&self.device),
            drawn,
            presented: self.presented,
        }
    }

    /// Draw the completed frame into `destination` of the output image,
    /// whose size is the window's `output`, with black bars and nearest
    /// sampling, ready for the swap.
    pub(crate) fn present(&mut self, output: [u32; 2], destination: PresentationRect) {
        self.flush();
        self.with_state(|state, core, target| unsafe {
            state.output(core, output)?;
            let command = state.begin(core)?;
            state.end_rendering(core);
            transition(
                core,
                command,
                &target.color,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
            let image = state.output.as_ref().expect("made above");
            transition(
                core,
                command,
                image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(image.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                })];
            core.device.cmd_begin_rendering(
                command,
                &vk::RenderingInfo::default()
                    .render_area(rect([0, 0], output))
                    .layer_count(1)
                    .color_attachments(&color),
            );
            set_viewport(
                core,
                command,
                [destination.x, destination.y],
                [destination.width, destination.height],
                output,
            );
            // A supersampled frame is averaged down; one at the shown size
            // or smaller (retail's pixels) keeps nearest sampling.
            let pipeline = if target.color.extent[0] > destination.width {
                state.downsample_output
            } else {
                state.blit_output
            };
            blit(core, pipeline, target.color.set.expect("sampled"), true);
            core.device.cmd_end_rendering(command);
            state.retire_idle_textures();
            Ok(())
        });
        self.presented_since = true;
    }
}

/// A frame's pixels copied on the device for a snapshot
/// ([`VkFrame::stage_rgba`]), read back when it is saved.
pub(crate) struct StagedRgba {
    device: Rc<VkDevice>,
    drawn: [u32; 2],
    presented: [u32; 2],
}

impl fmt::Debug for StagedRgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagedRgba")
            .field("drawn", &self.drawn)
            .field("presented", &self.presented)
            .finish_non_exhaustive()
    }
}

impl StagedRgba {
    /// The copied pixels at the size the frame is shown at, top row first,
    /// as [`VkFrame::read_rgba`] gives them: everything recorded so far is
    /// submitted and waited for.
    pub(crate) fn read(&self) -> Vec<u8> {
        let drawn = self.drawn;
        let mut pixels = vec![0; drawn[0] as usize * drawn[1] as usize * 4];
        {
            let mut state = self.device.state.borrow_mut();
            let core = &self.device.core;
            // SAFETY: the buffer belongs to the device; the wait finishes the
            // copy into it.
            let result = unsafe { state.submit_and_wait(core, None) };
            match result {
                Err(error) => state.fail(&error),
                Ok(()) if !state.failed => {
                    if let Some(mapped) = state
                        .staged
                        .as_ref()
                        .filter(|(_, extent)| *extent == drawn)
                        .and_then(|(buffer, _)| buffer.allocation.as_ref())
                        .and_then(|allocation| allocation.mapped_slice())
                    {
                        let length = pixels.len();
                        pixels.copy_from_slice(&mapped[..length]);
                    }
                }
                Ok(()) => {}
            }
        }
        if drawn == self.presented {
            pixels
        } else {
            downsampled(&pixels, drawn, self.presented)
        }
    }
}

/// What a draw held from the frame waits for ([`VkFrame::waits_for_water`]).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Held {
    /// The water shaded: drawn over it.
    Water,
    /// The rain's wet ground: drawn over it, before the water is shaded.
    Ground,
    /// The modern sky's world pass: drawn after it, before the
    /// anti-aliasing.
    Sky,
}

/// Whether a draw keyed `key`, its corners `heights` up from the eye (none:
/// not placed), waits for the water's pass ([`VkFrame::waits_for_water`]):
/// never an opaque draw with its depth; with the eye `under` the water, one
/// with a corner under the water's `plane`; above it, once the water's
/// first strip is in (`water_begun`), one with a corner above the plane.
/// An unplaced corner, or no plane known, holds it.
fn holds_for_water(
    key: &PipelineKey,
    under: bool,
    water_begun: bool,
    plane: Option<f32>,
    mut heights: impl Iterator<Item = Option<f32>>,
) -> bool {
    if (key.blend == D3dTextureBlend::Copy && (key.depth_write || key.depth_always))
        || !(under || water_begun)
    {
        return false;
    }
    let Some(plane) = plane else {
        return true;
    };
    heights.any(|height| height.is_none_or(|y| if under { y < plane } else { y > plane }))
}

/// A corner of retail's ground for the water's pass's ground image
/// (`vk_ground.frag`): the colour its fog took and its share in its fog
/// (`textured`: its own, `corner.fog`; else a colored quad's, which mixed
/// `far` into its colours in 256ths and keeps 255 less the fog's in its
/// specular alpha, its U set to say so).
fn ground_corner(
    vertex: &D3dTlVertex,
    corner: DeviceVertex,
    textured: bool,
    far: [u8; 3],
) -> DeviceVertex {
    if textured {
        return DeviceVertex {
            texture: [0.0, 0.0, 1.0],
            ..corner
        };
    }
    DeviceVertex {
        texture: [1.0, 0.0, 1.0],
        fog: [far[0], far[1], far[2], (vertex.specular >> 24) as u8],
        ..corner
    }
}

/// Draw `set`'s image over the current viewport with no blending.
unsafe fn blit(core: &Core, pipeline: vk::Pipeline, set: Descriptor, opaque: bool) {
    let command = core.command.get();
    core.device
        .cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, pipeline);
    core.device.cmd_bind_descriptor_sets(
        command,
        vk::PipelineBindPoint::GRAPHICS,
        core.blit_layout,
        0,
        &[set.set],
        &[],
    );
    let push = if opaque { 1.0_f32 } else { 0.0 };
    core.device.cmd_push_constants(
        command,
        core.blit_layout,
        vk::ShaderStageFlags::FRAGMENT,
        0,
        &push.to_le_bytes(),
    );
    core.device.cmd_draw(command, 4, 1, 0, 0);
}

impl Drop for VkFrame {
    fn drop(&mut self) {
        // As the OpenGL frame's, a pending batch is dropped with the frame.
        if let Some(target) = self.target.take() {
            self.device.state.borrow_mut().targets.push(target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        downsampled, ground_corner, holds_for_water, supersampled, D3dTextureBlend, D3dTlVertex,
        DeviceVertex, PipelineKey,
    };

    /// The ground's corners carry the fog the ground took: a textured
    /// corner its own, a colored quad's the colour it mixed in and its
    /// specular alpha, marked in U; the rest as drawn.
    #[test]
    fn the_grounds_corners_carry_its_fog() {
        let vertex = D3dTlVertex {
            x: 3.0,
            y: 4.0,
            depth: 0.5,
            rhw: 0.01,
            diffuse: [10, 20, 30, 255],
            specular: 0x4000_0000,
            uv: [0.25, 0.75],
        };
        let corner = DeviceVertex {
            position: [3.0, 4.0, 0.5],
            texture: [0.0025, 0.0075, 0.01],
            diffuse: [10, 20, 30, 255],
            fog: [90, 100, 110, 200],
        };
        let textured = ground_corner(&vertex, corner, true, [1, 2, 3]);
        assert_eq!(textured.texture, [0.0, 0.0, 1.0]);
        assert_eq!(
            (textured.fog, textured.diffuse),
            (corner.fog, corner.diffuse)
        );
        let colored = ground_corner(&vertex, corner, false, [1, 2, 3]);
        assert_eq!(colored.texture[0], 1.0);
        assert_eq!(colored.fog, [1, 2, 3, 0x40]);
        assert_eq!(
            (colored.position, colored.diffuse),
            (corner.position, corner.diffuse)
        );
    }

    /// A far ring's colored quad mixes its fog's colour into its corners
    /// in 256ths (`prepare_colored_terrain`); its ground corner carries
    /// that colour and its specular alpha, whose share, (alpha + 1) / 256
    /// (`vk_ground.frag`), is the quad's own colour's in what was drawn, as
    /// `wet_ground` takes the fog back out: to within the corners'
    /// rounding.
    #[test]
    fn the_far_rings_corners_mix_its_fog_at_the_share_the_ground_reads() {
        use crate::d3d_terrain::{
            colored_terrain_fog, prepare_colored_terrain, ColoredTerrainQuad,
        };
        let state = crate::d3d_flat::D3dFlatState {
            material_shifts: [6, 1, 4],
            near_depth: 64,
            manages_depth: true,
            depth_enable: 1,
            depth_write: 1,
        };
        let quad = ColoredTerrainQuad {
            vertices: [crate::software_frame::FlatVertex {
                x: 0.0,
                y: 0.0,
                depth: 128,
            }; 4],
            mode: 0x4000,
            material: 0x7fff,
            shades: [[128, 64, 32], [100, 90, 80], [60, 128, 10], [128, 128, 128]],
            far: Some((0x1f, [0, 64, 191, 255])),
        };
        let unfogged = prepare_colored_terrain(&ColoredTerrainQuad { far: None, ..quad }, state);
        let far = colored_terrain_fog(&quad, state).expect("a fogged quad");
        let fogged = prepare_colored_terrain(&quad, state);
        for (index, vertex) in fogged.vertices.iter().enumerate() {
            let drawn = crate::frame_device::device_vertex(vertex, None);
            let corner = ground_corner(vertex, drawn, false, far);
            let shows = (f32::from(corner.fog[3]) + 1.0) / 256.0;
            for channel in 0..3 {
                let own = f32::from(unfogged.vertices[index].diffuse[channel]);
                let haze = f32::from(corner.fog[channel]) * (1.0 - shows);
                let mixed = own * shows + haze;
                let got = f32::from(drawn.diffuse[channel]);
                assert!(
                    (got - mixed).abs() <= 0.5 + 0.5 * shows,
                    "corner {index}, channel {channel}: {got} for {mixed}"
                );
            }
        }
    }

    /// Supersampling enlarges each side by its scale, within the device's
    /// largest image, and never shrinks a frame.
    #[test]
    fn supersampled_targets_grow_within_the_largest_image() {
        assert_eq!(supersampled([3840, 2160], 2.0, 16384), [7680, 4320]);
        assert_eq!(supersampled([3840, 2160], 1.0, 16384), [3840, 2160]);
        assert_eq!(
            supersampled([1920, 1080], std::f32::consts::SQRT_2, 16384),
            [2715, 1527]
        );
        // The longer side meets the device's limit; the shape stays.
        assert_eq!(supersampled([3840, 2160], 2.0, 4096), [4096, 2304]);
        assert_eq!(supersampled([8192, 4096], 2.0, 4096), [8192, 4096]);
    }

    /// Averaging down weighs each frame pixel by how much of it a screen
    /// pixel covers: whole blocks at twice the size, shared edges between.
    #[test]
    fn downsampling_weighs_each_pixel_by_its_cover() {
        let grey = |value: u8| [value, value, value, 255];
        // Two by two blocks of 0, 100, 200 and 40 average whole.
        let mut pixels = Vec::new();
        for row in [
            [0, 0, 100, 100],
            [0, 0, 100, 100],
            [200, 200, 40, 40],
            [200, 200, 40, 40],
        ] {
            for value in row {
                pixels.extend(grey(value));
            }
        }
        assert_eq!(
            downsampled(&pixels, [4, 4], [2, 2]),
            [grey(0), grey(100), grey(200), grey(40)].concat()
        );
        // Three across to two: the middle pixel splits its half between them.
        let row: Vec<u8> = [grey(0), grey(90), grey(180)].concat();
        assert_eq!(
            downsampled(&row, [3, 1], [2, 1]),
            [grey(30), grey(150)].concat()
        );
    }

    /// Which draws wait for the water's pass: above the water (unchanged by
    /// the view from below), what is blended or without its depth and
    /// reaches above the plane, once the water's first strip is in; with the
    /// eye under the water, what reaches under the plane, whenever it comes.
    #[test]
    fn draws_wait_for_the_water_by_the_side_of_the_plane_the_eye_is_on() {
        let key = |blend, depth_write, depth_always| PipelineKey {
            textured: true,
            blend,
            depth_test: true,
            depth_write,
            traced_fog: false,
            depth_always,
        };
        let smoke = key(D3dTextureBlend::SourceAlpha, false, false);
        let spark = key(D3dTextureBlend::Additive, false, false);
        let flat = key(D3dTextureBlend::Copy, false, false);
        let solid = key(D3dTextureBlend::Copy, true, false);
        let painted = key(D3dTextureBlend::Copy, false, true);
        let plane = Some(-100.0);
        let above = [Some(-50.0), Some(0.0), Some(20.0)];
        let below = [Some(-150.0), Some(-200.0), Some(-120.0)];
        let across = [Some(-150.0), Some(-50.0), Some(-120.0)];
        let lost = [Some(-150.0), None, Some(-120.0)];
        let holds = |key: &PipelineKey, under, begun, plane, heights: [Option<f32>; 3]| {
            holds_for_water(key, under, begun, plane, heights.into_iter())
        };
        for key in [&smoke, &spark, &flat] {
            // Above the water: nothing before the water's first strip.
            for heights in [above, below, across, lost] {
                assert!(!holds(key, false, false, plane, heights));
            }
            assert!(holds(key, false, true, plane, above));
            assert!(!holds(key, false, true, plane, below));
            assert!(holds(key, false, true, plane, across));
            assert!(holds(key, false, true, plane, lost));
            assert!(holds(key, false, true, None, below));
            // Under it: the other side, the water's strips in or not.
            for begun in [false, true] {
                assert!(!holds(key, true, begun, plane, above));
                assert!(holds(key, true, begun, plane, below));
                assert!(holds(key, true, begun, plane, across));
                assert!(holds(key, true, begun, plane, lost));
                assert!(holds(key, true, begun, None, above));
            }
        }
        // What writes its depth is never held.
        for key in [&solid, &painted] {
            for (under, begun) in [(false, true), (true, false), (true, true)] {
                for heights in [above, below, across, lost] {
                    assert!(!holds(key, under, begun, plane, heights));
                }
            }
        }
    }
}
