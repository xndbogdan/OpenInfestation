//! Intel XeSS Super Resolution for the traced world
//! ([the plan](../../../../docs/INFESTATION-RT-IMPLEMENTATION.md#upscaling-and-denoising)):
//! `libxess.dll` (Intel's Simplified Software License, shipped unmodified)
//! loaded from beside the executable when present. It asks the instance and
//! device for extensions and features at creation; without it XeSS is
//! unavailable.

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;

use super::trace::{LIGHT_IMAGE, MOTION_IMAGE, TRACED_DEPTH};
use super::upscale::{UpscaleFrame, UpscaleQuality};
use super::*;

const RESULT_SUCCESS: i32 = 0;
/// The colour is display-referred, not scene-referred.
const INIT_LDR_INPUT_COLOR: u32 = 1 << 6;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Size2 {
    x: u32,
    y: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImageViewInfo {
    view: vk::ImageView,
    image: vk::Image,
    range: vk::ImageSubresourceRange,
    format: vk::Format,
    width: u32,
    height: u32,
}

impl ImageViewInfo {
    fn of(image: &Image, format: vk::Format) -> Self {
        Self {
            view: image.view,
            image: image.image,
            range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            format,
            width: image.extent[0],
            height: image.extent[1],
        }
    }
}

/// `xess_vk_execute_params_t`.
#[repr(C)]
#[derive(Default)]
struct ExecuteParams {
    color: ImageViewInfo,
    velocity: ImageViewInfo,
    depth: ImageViewInfo,
    exposure_scale_texture: ImageViewInfo,
    responsive_mask: ImageViewInfo,
    output: ImageViewInfo,
    jitter_x: f32,
    jitter_y: f32,
    exposure_scale: f32,
    reset_history: u32,
    input_width: u32,
    input_height: u32,
    input_color_base: Size2,
    input_motion_base: Size2,
    input_depth_base: Size2,
    input_responsive_base: Size2,
    reserved: Size2,
    output_color_base: Size2,
}

/// `xess_vk_init_params_t`.
#[repr(C)]
#[derive(Default)]
struct InitParams {
    output: Size2,
    quality: i32,
    flags: u32,
    creation_node_mask: u32,
    visible_node_mask: u32,
    buffer_heap: vk::DeviceMemory,
    buffer_heap_offset: u64,
    texture_heap: vk::DeviceMemory,
    texture_heap_offset: u64,
    pipeline_cache: vk::PipelineCache,
}

type InstanceExtensions = unsafe extern "C" fn(
    count: *mut u32,
    names: *mut *const *const c_char,
    api_version: *mut u32,
) -> i32;
type DeviceExtensions = unsafe extern "C" fn(
    instance: vk::Instance,
    physical: vk::PhysicalDevice,
    count: *mut u32,
    names: *mut *const *const c_char,
) -> i32;
type DeviceFeatures = unsafe extern "C" fn(
    instance: vk::Instance,
    physical: vk::PhysicalDevice,
    features: *mut *mut c_void,
) -> i32;
type CreateContext = unsafe extern "C" fn(
    instance: vk::Instance,
    physical: vk::PhysicalDevice,
    device: vk::Device,
    context: *mut *mut c_void,
) -> i32;
type Init = unsafe extern "C" fn(context: *mut c_void, params: *const InitParams) -> i32;
type Execute = unsafe extern "C" fn(
    context: *mut c_void,
    command: vk::CommandBuffer,
    params: *const ExecuteParams,
) -> i32;
type OptimalInput = unsafe extern "C" fn(
    context: *mut c_void,
    output: *const Size2,
    quality: i32,
    optimal: *mut Size2,
    min: *mut Size2,
    max: *mut Size2,
) -> i32;
type DestroyContext = unsafe extern "C" fn(context: *mut c_void) -> i32;

/// The library, loaded for one device's life, with the device's one context.
pub(super) struct XessLibrary {
    _library: libloading::Library,
    instance_extensions: InstanceExtensions,
    device_extensions: DeviceExtensions,
    device_features: DeviceFeatures,
    create_context: CreateContext,
    init: Init,
    execute: Execute,
    optimal_input: OptimalInput,
    destroy_context: DestroyContext,
    /// Made when XeSS is first wanted (null until then, or when that failed).
    context: Cell<*mut c_void>,
    tried: Cell<bool>,
}

fn names(count: u32, list: *const *const c_char) -> Vec<CString> {
    if list.is_null() {
        return Vec::new();
    }
    // SAFETY: XeSS returns `count` NUL-terminated names it owns.
    unsafe { std::slice::from_raw_parts(list, count as usize) }
        .iter()
        .map(|&name| unsafe { CStr::from_ptr(name) }.to_owned())
        .collect()
}

impl XessLibrary {
    /// `libxess.dll` beside the executable (or where `INF_XESS_DLL` says).
    pub(super) fn load() -> Option<Self> {
        let path = match std::env::var_os("INF_XESS_DLL") {
            Some(path) => PathBuf::from(path),
            None => std::env::current_exe().ok()?.parent()?.join("libxess.dll"),
        };
        if !path.exists() {
            return None;
        }
        // SAFETY: XeSS's entry points have exactly these signatures
        // (xess.h, xess_vk.h); the library outlives every call.
        unsafe {
            let library = libloading::Library::new(&path)
                .map_err(|error| {
                    eprintln!("Infestation: {} does not load: {error}", path.display())
                })
                .ok()?;
            let symbol = |name: &[u8]| {
                library
                    .get::<*const c_void>(name)
                    .ok()
                    .map(|symbol| *symbol)
            };
            Some(Self {
                instance_extensions: std::mem::transmute(symbol(
                    b"xessVKGetRequiredInstanceExtensions\0",
                )?),
                device_extensions: std::mem::transmute(symbol(
                    b"xessVKGetRequiredDeviceExtensions\0",
                )?),
                device_features: std::mem::transmute(symbol(b"xessVKGetRequiredDeviceFeatures\0")?),
                create_context: std::mem::transmute(symbol(b"xessVKCreateContext\0")?),
                init: std::mem::transmute(symbol(b"xessVKInit\0")?),
                execute: std::mem::transmute(symbol(b"xessVKExecute\0")?),
                optimal_input: std::mem::transmute(symbol(b"xessGetOptimalInputResolution\0")?),
                destroy_context: std::mem::transmute(symbol(b"xessDestroyContext\0")?),
                context: Cell::new(std::ptr::null_mut()),
                tried: Cell::new(false),
                _library: library,
            })
        }
    }

    /// The instance extensions XeSS asks for.
    pub(super) unsafe fn instance_extensions(&self) -> Vec<CString> {
        let (mut count, mut list, mut version) = (0, std::ptr::null(), 0);
        if (self.instance_extensions)(&mut count, &mut list, &mut version) != RESULT_SUCCESS {
            return Vec::new();
        }
        names(count, list)
    }

    /// The device extensions XeSS asks for on `physical`.
    pub(super) unsafe fn device_extensions(
        &self,
        instance: vk::Instance,
        physical: vk::PhysicalDevice,
    ) -> Vec<CString> {
        let (mut count, mut list) = (0, std::ptr::null());
        if (self.device_extensions)(instance, physical, &mut count, &mut list) != RESULT_SUCCESS {
            return Vec::new();
        }
        names(count, list)
    }

    /// Let XeSS turn on the features it needs in the device's feature chain
    /// (headed by a `VkPhysicalDeviceFeatures2`); it may append structures
    /// of its own. False when it refused.
    pub(super) unsafe fn patch_features(
        &self,
        instance: vk::Instance,
        physical: vk::PhysicalDevice,
        chain: *mut vk::PhysicalDeviceFeatures2,
    ) -> bool {
        let mut head = chain.cast::<c_void>();
        (self.device_features)(instance, physical, &mut head) == RESULT_SUCCESS
            && head == chain.cast()
    }

    /// The device's XeSS context, made the first time XeSS is wanted.
    pub(super) unsafe fn context(&self, core: &Core) -> Result<*mut c_void, String> {
        if !self.tried.replace(true) {
            let mut context = std::ptr::null_mut();
            let result = (self.create_context)(
                core.instance.handle(),
                core.physical,
                core.device.handle(),
                &mut context,
            );
            if result != RESULT_SUCCESS {
                eprintln!("Infestation: XeSS did not start ({result})");
            } else {
                self.context.set(context);
            }
        }
        let context = self.context.get();
        if context.is_null() {
            Err("XeSS did not start".into())
        } else {
            Ok(context)
        }
    }

    /// Release the context, before the device goes.
    pub(super) unsafe fn stop(&self) {
        let context = self.context.replace(std::ptr::null_mut());
        if !context.is_null() {
            (self.destroy_context)(context);
        }
        self.tried.set(false);
    }
}

/// XeSS's quality settings, from Ultra Performance (100) to native
/// anti-aliasing (106).
fn quality_setting(quality: UpscaleQuality) -> i32 {
    match quality {
        UpscaleQuality::UltraPerformance => 100,
        UpscaleQuality::Performance => 101,
        UpscaleQuality::Balanced => 102,
        UpscaleQuality::Quality => 103,
        UpscaleQuality::NativeAa => 106,
    }
}

/// XeSS set up for one display size and quality.
pub(super) struct Xess {
    pub(super) display: [u32; 2],
    pub(super) render: [u32; 2],
}

impl Xess {
    fn library(core: &Core) -> Result<&XessLibrary, String> {
        core.xess
            .as_ref()
            .ok_or_else(|| "libxess.dll is not beside the executable".to_string())
    }

    /// The render size XeSS asks for at `quality` for this `display`.
    pub(super) unsafe fn render_size(
        core: &Core,
        display: [u32; 2],
        quality: UpscaleQuality,
    ) -> Result<[u32; 2], String> {
        let library = Self::library(core)?;
        let context = library.context(core)?;
        let output = Size2 {
            x: display[0],
            y: display[1],
        };
        let (mut optimal, mut min, mut max) =
            (Size2::default(), Size2::default(), Size2::default());
        let result = (library.optimal_input)(
            context,
            &output,
            quality_setting(quality),
            &mut optimal,
            &mut min,
            &mut max,
        );
        if result != RESULT_SUCCESS || optimal.x == 0 || optimal.y == 0 {
            return Err(format!("XeSS gave no render size ({result})"));
        }
        Ok([optimal.x, optimal.y])
    }

    /// Set XeSS up for this display and quality (building its pipelines the
    /// first time).
    pub(super) unsafe fn new(
        core: &Core,
        quality: UpscaleQuality,
        display: [u32; 2],
        render: [u32; 2],
    ) -> Result<Self, String> {
        let library = Self::library(core)?;
        let context = library.context(core)?;
        let params = InitParams {
            output: Size2 {
                x: display[0],
                y: display[1],
            },
            quality: quality_setting(quality),
            flags: INIT_LDR_INPUT_COLOR,
            ..InitParams::default()
        };
        let result = (library.init)(context, &params);
        if result != RESULT_SUCCESS {
            return Err(format!("XeSS initialisation returned {result}"));
        }
        Ok(Self { display, render })
    }

    /// Record the upscale of the frame's colour into its output. XeSS reads
    /// its inputs in the shader-read layout, so they move there and back.
    pub(super) unsafe fn dispatch(
        &mut self,
        core: &Core,
        command: vk::CommandBuffer,
        frame: &UpscaleFrame,
    ) -> Result<(), String> {
        let library = Self::library(core)?;
        let context = library.context(core)?;
        let inputs = [frame.color, frame.motion, frame.depth];
        for image in inputs {
            transition(
                core,
                command,
                image,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
        let params = ExecuteParams {
            color: ImageViewInfo::of(frame.color, LIGHT_IMAGE),
            velocity: ImageViewInfo::of(frame.motion, MOTION_IMAGE),
            depth: ImageViewInfo::of(frame.depth, TRACED_DEPTH),
            output: ImageViewInfo::of(frame.output, LIGHT_IMAGE),
            jitter_x: frame.jitter[0],
            jitter_y: frame.jitter[1],
            exposure_scale: 1.0,
            reset_history: u32::from(frame.reset),
            input_width: frame.color.extent[0],
            input_height: frame.color.extent[1],
            ..ExecuteParams::default()
        };
        let result = (library.execute)(context, command, &params);
        for image in inputs {
            transition(core, command, image, vk::ImageLayout::GENERAL);
        }
        if result == RESULT_SUCCESS {
            Ok(())
        } else {
            Err(format!("XeSS execution returned {result}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UpscaleQuality, Xess};

    /// XeSS starts on the device (its extensions and features enabled) and
    /// gives a render size below the display. Skipped without the library.
    #[test]
    #[ignore = "opens a Vulkan device; run explicitly"]
    fn vulkan_xess_gives_a_render_size() {
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video
            .window("XeSS check", 64, 64)
            .hidden()
            .vulkan()
            .build()
            .unwrap();
        let device = super::VkDevice::new(&window, true).unwrap();
        if device.core.xess.is_none() {
            return;
        }
        let size =
            unsafe { Xess::render_size(&device.core, [3840, 2160], UpscaleQuality::Performance) }
                .unwrap();
        assert!(
            size[0] > 0 && size[0] < 3840 && size[1] > 0 && size[1] < 2160,
            "{size:?}"
        );
    }
}
