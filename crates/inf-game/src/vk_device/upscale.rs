//! Upscalers for the traced world ([the plan](../../../../docs/INFESTATION-RT-IMPLEMENTATION.md#upscaling-and-denoising)):
//! the tracer renders smaller, with its primary rays jittered, and an
//! upscaler brings the colour up to the display from its depth and motion.
//! Without traced rays they anti-alias the raster's world at its native size
//! ([`super::raster_aa`]).
//!
//! AMD FidelityFX Super Resolution 3.1 runs through the FidelityFX API's
//! signed library (`amd_fidelityfx_vk.dll`, MIT), loaded when wanted: the
//! game starts without it, and FSR is then unavailable.

use std::ffi::c_void;
use std::path::PathBuf;

use super::dlss::{Dlss, DlssFeature};
use super::xess::Xess;
use super::*;

/// Which upscaler, and how far below the display it renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpscalerChoice {
    Off,
    Fsr(UpscaleQuality),
    Dlss(UpscaleQuality),
    /// DLSS Ray Reconstruction: denoises the bounce light as it upscales.
    DlssRr(UpscaleQuality),
    Xess(UpscaleQuality),
}

pub(crate) use crate::modern_settings::UpscaleQuality;

impl UpscalerChoice {
    /// The choice the player's settings make.
    pub(crate) fn from_settings(modern: crate::modern_settings::ModernSettings) -> Self {
        use crate::modern_settings::Upscaler;
        let quality = modern.quality;
        match modern.upscaler {
            Upscaler::Off => Self::Off,
            Upscaler::Fsr => Self::Fsr(quality),
            Upscaler::Dlss => Self::Dlss(quality),
            Upscaler::DlssRayReconstruction => Self::DlssRr(quality),
            Upscaler::Xess => Self::Xess(quality),
        }
    }
}

/// Whether `upscaler` runs on this device: FSR where its library loads,
/// XeSS where its runtime starts on the device, DLSS and Ray Reconstruction
/// on an NVIDIA GPU whose NGX offers the feature (kept per GPU, driver and
/// libraries, since asking starts NGX). One that still fails to start falls
/// back to the tracer's own scaling. The caller offers Ray Reconstruction
/// only where the device traces rays. `INF_SIMULATE_GPU_VENDOR` (a PCI
/// vendor ID in hex, as `1002` for AMD) stands in for the GPU's vendor, so
/// tests on one card can check another's list.
pub(super) unsafe fn runs(core: &Core, upscaler: crate::modern_settings::Upscaler) -> bool {
    use crate::modern_settings::Upscaler as Choice;
    const NVIDIA: u32 = 0x10de;
    match upscaler {
        Choice::Off => true,
        Choice::Fsr => {
            core.upscaler_functions
                && fsr_library_path().is_some_and(|path| libloading::Library::new(path).is_ok())
        }
        Choice::Dlss | Choice::DlssRayReconstruction => {
            let vendor = std::env::var("INF_SIMULATE_GPU_VENDOR")
                .ok()
                .and_then(|vendor| u32::from_str_radix(vendor.trim_start_matches("0x"), 16).ok())
                .unwrap_or_else(|| {
                    core.instance
                        .get_physical_device_properties(core.physical)
                        .vendor_id
                });
            vendor == NVIDIA
                && core.ngx.as_ref().is_some_and(|ngx| {
                    ngx.offers(core)[usize::from(upscaler == Choice::DlssRayReconstruction)]
                })
        }
        Choice::Xess => core
            .xess
            .as_ref()
            .is_some_and(|xess| xess.context(core).is_ok()),
    }
}

/// One frame's inputs to an upscaler, at the render size, in the general
/// layout; the output is at the display's size.
pub(super) struct UpscaleFrame<'a> {
    pub(super) color: &'a Image,
    pub(super) depth: &'a Image,
    pub(super) motion: &'a Image,
    pub(super) output: &'a Image,
    /// The image's offset in render pixels.
    pub(super) jitter: [f32; 2],
    pub(super) reset: bool,
    pub(super) near: f32,
    pub(super) vertical_fov: f32,
    pub(super) frame_ms: f32,
    /// Ray Reconstruction's guides, at the render size.
    pub(super) guides: Option<Guides<'a>>,
}

/// What Ray Reconstruction reads besides the colour.
pub(super) struct Guides<'a> {
    pub(super) diffuse_albedo: &'a Image,
    pub(super) normal_roughness: &'a Image,
    pub(super) specular_albedo: &'a Image,
    /// Row-major, for row vectors (NGX's convention).
    pub(super) world_to_view: [f32; 16],
    pub(super) view_to_clip: [f32; 16],
}

/// The upscaler running.
pub(super) enum Upscaler {
    Fsr(Fsr),
    Dlss(Dlss),
    Xess(Xess),
}

impl Upscaler {
    /// The render size `choice` asks for at this `display`; none when off.
    pub(super) unsafe fn render_size(
        core: &Core,
        choice: UpscalerChoice,
        display: [u32; 2],
    ) -> Result<Option<[u32; 2]>, String> {
        match choice {
            UpscalerChoice::Off => Ok(None),
            UpscalerChoice::Fsr(quality) => Ok(Some(Fsr::render_size(display, quality))),
            UpscalerChoice::Dlss(quality) => {
                Dlss::render_size(core, DlssFeature::SuperResolution, display, quality).map(Some)
            }
            UpscalerChoice::DlssRr(quality) => {
                Dlss::render_size(core, DlssFeature::RayReconstruction, display, quality).map(Some)
            }
            UpscalerChoice::Xess(quality) => Xess::render_size(core, display, quality).map(Some),
        }
    }

    /// Make `choice`'s upscaler for these sizes, recording any setup into
    /// `command`.
    pub(super) unsafe fn new(
        core: &Core,
        command: vk::CommandBuffer,
        choice: UpscalerChoice,
        display: [u32; 2],
        render: [u32; 2],
    ) -> Result<Self, String> {
        match choice {
            UpscalerChoice::Off => Err("no upscaler was chosen".into()),
            UpscalerChoice::Fsr(_) => Fsr::new(core, display, render).map(Self::Fsr),
            UpscalerChoice::Dlss(quality) => Dlss::new(
                core,
                command,
                DlssFeature::SuperResolution,
                quality,
                display,
                render,
            )
            .map(Self::Dlss),
            UpscalerChoice::DlssRr(quality) => Dlss::new(
                core,
                command,
                DlssFeature::RayReconstruction,
                quality,
                display,
                render,
            )
            .map(Self::Dlss),
            UpscalerChoice::Xess(quality) => {
                Xess::new(core, quality, display, render).map(Self::Xess)
            }
        }
    }

    /// Whether it is `choice`'s upscaler, made for these sizes.
    pub(super) fn fits(&self, choice: UpscalerChoice, display: [u32; 2], render: [u32; 2]) -> bool {
        match (self, choice) {
            (Self::Fsr(fsr), UpscalerChoice::Fsr(_)) => {
                fsr.display == display && fsr.render == render
            }
            (Self::Dlss(dlss), UpscalerChoice::Dlss(_) | UpscalerChoice::DlssRr(_)) => {
                let feature = match choice {
                    UpscalerChoice::DlssRr(_) => DlssFeature::RayReconstruction,
                    _ => DlssFeature::SuperResolution,
                };
                dlss.feature == feature && dlss.display == display && dlss.render == render
            }
            (Self::Xess(xess), UpscalerChoice::Xess(_)) => {
                xess.display == display && xess.render == render
            }
            _ => false,
        }
    }

    pub(super) unsafe fn dispatch(
        &mut self,
        core: &Core,
        command: vk::CommandBuffer,
        frame: &UpscaleFrame,
    ) -> Result<(), String> {
        match self {
            Self::Fsr(fsr) => fsr.dispatch(command, frame),
            Self::Dlss(dlss) => dlss.dispatch(core, command, frame),
            Self::Xess(xess) => xess.dispatch(core, command, frame),
        }
    }

    /// Release it; the device must be past its last use.
    pub(super) unsafe fn destroy(self, core: &Core) {
        match self {
            Self::Fsr(fsr) => fsr.destroy(),
            Self::Dlss(dlss) => dlss.destroy(core),
            // The device's one context stays for the next set-up.
            Self::Xess(_) => {}
        }
    }
}

// ----------------------------------------------------------------------
// The FidelityFX API (ffx_api.h, ffx_upscale.h, vk/ffx_api_vk.h), the parts
// the upscale uses.

#[repr(C)]
struct Header {
    kind: u64,
    next: *mut Header,
}

#[repr(C)]
struct BackendVk {
    header: Header,
    device: *mut c_void,
    physical_device: *mut c_void,
    get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Dimensions {
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Coords {
    x: f32,
    y: f32,
}

type Message = unsafe extern "C" fn(kind: u32, message: *const u16);

#[repr(C)]
struct CreateUpscale {
    header: Header,
    flags: u32,
    max_render_size: Dimensions,
    max_upscale_size: Dimensions,
    message: Option<Message>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ResourceDescription {
    kind: u32,
    format: u32,
    width: u32,
    height: u32,
    depth: u32,
    mip_count: u32,
    flags: u32,
    usage: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Resource {
    resource: *mut c_void,
    description: ResourceDescription,
    state: u32,
}

#[repr(C)]
struct DispatchUpscale {
    header: Header,
    command_list: *mut c_void,
    color: Resource,
    depth: Resource,
    motion_vectors: Resource,
    exposure: Resource,
    reactive: Resource,
    transparency_and_composition: Resource,
    output: Resource,
    jitter_offset: Coords,
    motion_vector_scale: Coords,
    render_size: Dimensions,
    upscale_size: Dimensions,
    enable_sharpening: bool,
    sharpness: f32,
    frame_time_delta: f32,
    pre_exposure: f32,
    reset: bool,
    camera_near: f32,
    camera_far: f32,
    camera_fov_angle_vertical: f32,
    view_space_to_meters_factor: f32,
    flags: u32,
}

const DESC_BACKEND_VK: u64 = 0x0000_0003;
const DESC_CREATE_UPSCALE: u64 = 0x0001_0000;
const DESC_DISPATCH_UPSCALE: u64 = 0x0001_0001;
const RETURN_OK: u32 = 0;
const ENABLE_DEPTH_INFINITE: u32 = 1 << 4;
const ENABLE_NON_LINEAR_COLORSPACE: u32 = 1 << 8;
const RESOURCE_TEXTURE_2D: u32 = 2;
const USAGE_UAV: u32 = 1 << 1;
const STATE_UNORDERED_ACCESS: u32 = 1 << 1;
const FORMAT_RGBA16F: u32 = 4;
const FORMAT_RG16F: u32 = 18;
const FORMAT_R32F: u32 = 28;

type CreateContext = unsafe extern "C" fn(
    context: *mut *mut c_void,
    desc: *mut Header,
    allocation: *const c_void,
) -> u32;
type DestroyContext =
    unsafe extern "C" fn(context: *mut *mut c_void, allocation: *const c_void) -> u32;
type Dispatch = unsafe extern "C" fn(context: *mut *mut c_void, desc: *const Header) -> u32;

/// FidelityFX's messages, to the log (a few, not a flood).
unsafe extern "C" fn fsr_message(kind: u32, message: *const u16) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SHOWN: AtomicU32 = AtomicU32::new(0);
    if message.is_null() || SHOWN.fetch_add(1, Ordering::Relaxed) >= 20 {
        return;
    }
    let mut length = 0;
    while *message.add(length) != 0 && length < 4096 {
        length += 1;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(message, length));
    let kind = if kind == 0 { "error" } else { "warning" };
    eprintln!("Infestation: FSR {kind}: {text}");
}

/// The library, found beside the executable or where `INF_FFX_DLL` says.
fn fsr_library_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("INF_FFX_DLL") {
        return Some(PathBuf::from(path));
    }
    let directory = std::env::current_exe().ok()?.parent()?.to_path_buf();
    Some(directory.join("amd_fidelityfx_vk.dll"))
}

/// An FSR 3.1 upscale context for one display and render size.
pub(super) struct Fsr {
    /// Kept loaded while the context lives.
    _library: libloading::Library,
    destroy_context: DestroyContext,
    dispatch: Dispatch,
    context: *mut c_void,
    /// The display and largest render size it was made for.
    pub(super) display: [u32; 2],
    pub(super) render: [u32; 2],
}

impl Fsr {
    /// The render size FSR's quality modes give a `display` of that size:
    /// each side divided by the mode's ratio, rounded down as FSR rounds.
    pub(super) fn render_size(display: [u32; 2], quality: UpscaleQuality) -> [u32; 2] {
        display.map(|side| ((side as f32 / quality.ratio()) as u32).max(1))
    }

    pub(super) unsafe fn new(
        core: &Core,
        display: [u32; 2],
        render: [u32; 2],
    ) -> Result<Self, String> {
        if !core.upscaler_functions {
            return Err("the device lacks VK_KHR_get_memory_requirements2".into());
        }
        let path = fsr_library_path().ok_or("no path for the FidelityFX library")?;
        let library = libloading::Library::new(&path)
            .map_err(|error| format!("{} does not load: {error}", path.display()))?;
        let symbol = |name: &[u8]| {
            library
                .get::<*const c_void>(name)
                .map(|symbol| *symbol)
                .map_err(|error| {
                    format!(
                        "FidelityFX library lacks {}: {error}",
                        String::from_utf8_lossy(name)
                    )
                })
        };
        let create: CreateContext = std::mem::transmute(symbol(b"ffxCreateContext\0")?);
        let destroy_context: DestroyContext = std::mem::transmute(symbol(b"ffxDestroyContext\0")?);
        let dispatch: Dispatch = std::mem::transmute(symbol(b"ffxDispatch\0")?);
        let mut backend = BackendVk {
            header: Header {
                kind: DESC_BACKEND_VK,
                next: std::ptr::null_mut(),
            },
            device: core.device.handle().as_raw() as usize as *mut c_void,
            physical_device: core.physical.as_raw() as usize as *mut c_void,
            get_device_proc_addr: core.instance.fp_v1_0().get_device_proc_addr,
        };
        let mut desc = CreateUpscale {
            header: Header {
                kind: DESC_CREATE_UPSCALE,
                next: &mut backend.header,
            },
            flags: ENABLE_DEPTH_INFINITE | ENABLE_NON_LINEAR_COLORSPACE,
            max_render_size: Dimensions {
                width: render[0],
                height: render[1],
            },
            max_upscale_size: Dimensions {
                width: display[0],
                height: display[1],
            },
            message: Some(fsr_message),
        };
        let mut context = std::ptr::null_mut();
        let result = create(&mut context, &mut desc.header, std::ptr::null());
        if result != RETURN_OK || context.is_null() {
            return Err(format!("FSR context creation returned {result}"));
        }
        Ok(Self {
            _library: library,
            destroy_context,
            dispatch,
            context,
            display,
            render,
        })
    }

    /// Record the upscale of the frame's colour into its output.
    pub(super) unsafe fn dispatch(
        &mut self,
        command: vk::CommandBuffer,
        frame: &UpscaleFrame,
    ) -> Result<(), String> {
        let UpscaleFrame {
            color,
            depth,
            motion,
            output,
            jitter,
            reset,
            near,
            vertical_fov,
            frame_ms,
            ..
        } = *frame;
        let image = |image: &Image, format: u32| Resource {
            resource: image.image.as_raw() as usize as *mut c_void,
            description: ResourceDescription {
                kind: RESOURCE_TEXTURE_2D,
                format,
                width: image.extent[0],
                height: image.extent[1],
                depth: 1,
                mip_count: 1,
                flags: 0,
                usage: USAGE_UAV,
            },
            state: STATE_UNORDERED_ACCESS,
        };
        let none = Resource {
            resource: std::ptr::null_mut(),
            description: ResourceDescription {
                kind: 0,
                format: 0,
                width: 0,
                height: 0,
                depth: 0,
                mip_count: 0,
                flags: 0,
                usage: 0,
            },
            state: 0,
        };
        let desc = DispatchUpscale {
            header: Header {
                kind: DESC_DISPATCH_UPSCALE,
                next: std::ptr::null_mut(),
            },
            command_list: command.as_raw() as usize as *mut c_void,
            color: image(color, FORMAT_RGBA16F),
            depth: image(depth, FORMAT_R32F),
            motion_vectors: image(motion, FORMAT_RG16F),
            exposure: none,
            reactive: none,
            transparency_and_composition: none,
            output: image(output, FORMAT_RGBA16F),
            jitter_offset: Coords {
                x: jitter[0],
                y: jitter[1],
            },
            // The motion is in render pixels already.
            motion_vector_scale: Coords { x: 1.0, y: 1.0 },
            render_size: Dimensions {
                width: color.extent[0],
                height: color.extent[1],
            },
            upscale_size: Dimensions {
                width: output.extent[0],
                height: output.extent[1],
            },
            enable_sharpening: false,
            sharpness: 0.0,
            frame_time_delta: frame_ms,
            pre_exposure: 1.0,
            reset,
            camera_near: near,
            camera_far: f32::MAX,
            camera_fov_angle_vertical: vertical_fov,
            // A mutant (132 units) stands about 2 m tall.
            view_space_to_meters_factor: 0.015,
            flags: 0,
        };
        let result = (self.dispatch)(&mut self.context, &desc.header);
        if result == RETURN_OK {
            Ok(())
        } else {
            Err(format!("FSR dispatch returned {result}"))
        }
    }

    /// Release the context; the device must be idle or past its last use.
    pub(super) unsafe fn destroy(mut self) {
        (self.destroy_context)(&mut self.context, std::ptr::null());
    }
}

#[cfg(test)]
mod tests {
    /// FSR's library calls Vulkan functions by names the device must enable
    /// (a NULL call otherwise). Skipped where the library is absent.
    #[test]
    #[ignore = "opens a Vulkan device; run explicitly"]
    fn vulkan_fsr_context_creates() {
        if !super::fsr_library_path().is_some_and(|path| path.exists()) {
            return;
        }
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video
            .window("FSR check", 64, 64)
            .hidden()
            .vulkan()
            .build()
            .unwrap();
        let device = super::VkDevice::new(&window, true).unwrap();
        let fsr = unsafe { super::Fsr::new(&device.core, [1920, 1080], [960, 540]) }.unwrap();
        unsafe { fsr.destroy() };
    }

    /// The device says what it runs once, the same each time: no upscaler
    /// without ray tracing, and FSR wherever its library loads on a device
    /// that traces.
    #[test]
    #[ignore = "opens a Vulkan device; run explicitly"]
    fn vulkan_says_what_it_runs() {
        use crate::modern_settings::Upscaler;
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video
            .window("Support check", 64, 64)
            .hidden()
            .vulkan()
            .build()
            .unwrap();
        let device = super::VkDevice::new(&window, true).unwrap();
        let support = device.modern_support();
        assert_eq!(device.modern_support(), support);
        assert!(support.runs(Upscaler::Off));
        assert_eq!(support.ray_tracing, device.ray_tracing());
        if !support.ray_tracing {
            assert_eq!(support.upscalers(), [Upscaler::Off]);
        } else if super::fsr_library_path().is_some_and(|path| path.exists()) {
            assert!(support.runs(Upscaler::Fsr));
        }
    }

    /// The same card as another vendor's (`INF_SIMULATE_GPU_VENDOR`, AMD's
    /// 1002) offers no DLSS, and a saved DLSS runs as FSR there while it
    /// stays saved; as itself an NVIDIA card with the DLSS libraries offers
    /// DLSS (and Ray Reconstruction where it traces).
    #[test]
    #[ignore = "opens a Vulkan device; run explicitly"]
    fn vulkan_offers_dlss_only_on_nvidia() {
        use crate::modern_settings::{ModernSettings, Upscaler};
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video
            .window("Support check", 64, 64)
            .hidden()
            .vulkan()
            .build()
            .unwrap();
        let support = |vendor: Option<&str>| {
            match vendor {
                Some(vendor) => std::env::set_var("INF_SIMULATE_GPU_VENDOR", vendor),
                None => std::env::remove_var("INF_SIMULATE_GPU_VENDOR"),
            }
            let support = super::VkDevice::new(&window, true)
                .unwrap()
                .modern_support();
            std::env::remove_var("INF_SIMULATE_GPU_VENDOR");
            support
        };
        let amd = support(Some("1002"));
        assert!(!amd.runs(Upscaler::Dlss) && !amd.runs(Upscaler::DlssRayReconstruction));
        let saved = ModernSettings {
            ray_tracing: true,
            upscaler: Upscaler::Dlss,
            ..ModernSettings::default()
        };
        if amd.runs(Upscaler::Fsr) {
            assert_eq!(saved.effective(amd).upscaler, Upscaler::Fsr);
        }
        assert_eq!(saved.upscaler, Upscaler::Dlss);
        let own = super::VkDevice::new(&window, true).unwrap();
        // SAFETY: a query of the device's own GPU.
        let vendor = unsafe {
            own.core
                .instance
                .get_physical_device_properties(own.core.physical)
                .vendor_id
        };
        let own = own.modern_support();
        if vendor == 0x10de && super::dlss::Ngx::load().is_some() {
            assert!(own.runs(Upscaler::Dlss), "{own:?}");
            assert_eq!(own.runs(Upscaler::DlssRayReconstruction), own.ray_tracing);
        }
    }
}
