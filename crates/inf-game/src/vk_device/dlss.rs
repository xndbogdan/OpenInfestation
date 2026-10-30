//! NVIDIA DLSS for the traced world, through NGX
//! ([the plan](../../../../docs/INFESTATION-RT-IMPLEMENTATION.md#upscaling-and-denoising)).
//!
//! NGX's Vulkan API ships only as an MSVC static library and the game builds
//! with the GNU toolchain, so a small bridge links it
//! (`ngx-bridge/inf_ngx.cpp`, built by `scripts/build-upscalers.ps1`). The
//! game loads `inf_ngx.dll` from beside the executable when present, and NGX
//! loads NVIDIA's `nvngx_dlss.dll` from there; without them DLSS is
//! unavailable.

use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::trace::{COLOR_IMAGE, LIGHT_IMAGE, MOTION_IMAGE, TRACED_DEPTH};
use super::upscale::{UpscaleFrame, UpscaleQuality};
use super::*;

/// The bridge's features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DlssFeature {
    SuperResolution = 0,
    RayReconstruction = 1,
}

/// NGX's results: failures carry `0xBAD` in the top twelve bits.
fn succeeded(result: u32) -> bool {
    result & 0xfff0_0000 != 0xbad0_0000
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NgxImage {
    image: vk::Image,
    view: vk::ImageView,
    format: vk::Format,
    width: u32,
    height: u32,
}

impl NgxImage {
    fn of(image: &Image, format: vk::Format) -> Self {
        Self {
            image: image.image,
            view: image.view,
            format,
            width: image.extent[0],
            height: image.extent[1],
        }
    }
}

/// One frame's inputs, as the bridge's `InfNgxFrame`.
#[repr(C)]
#[derive(Default)]
struct NgxFrame {
    color: NgxImage,
    depth: NgxImage,
    motion: NgxImage,
    output: NgxImage,
    diffuse_albedo: NgxImage,
    specular_albedo: NgxImage,
    normal_roughness: NgxImage,
    jitter: [f32; 2],
    render: [u32; 2],
    reset: i32,
    frame_ms: f32,
    world_to_view: [f32; 16],
    view_to_clip: [f32; 16],
}

type Log = unsafe extern "C" fn(message: *const c_char);
type InstanceExtensions = unsafe extern "C" fn(
    feature: u32,
    data_path: *const u16,
    count: *mut u32,
    properties: *mut *mut vk::ExtensionProperties,
) -> u32;
type DeviceExtensions = unsafe extern "C" fn(
    instance: vk::Instance,
    physical: vk::PhysicalDevice,
    feature: u32,
    data_path: *const u16,
    count: *mut u32,
    properties: *mut *mut vk::ExtensionProperties,
) -> u32;
type Init = unsafe extern "C" fn(
    instance: vk::Instance,
    physical: vk::PhysicalDevice,
    device: vk::Device,
    gipa: vk::PFN_vkGetInstanceProcAddr,
    gdpa: vk::PFN_vkGetDeviceProcAddr,
    data_path: *const u16,
    log: Option<Log>,
) -> u32;
type Available = unsafe extern "C" fn(feature: u32) -> i32;
type RenderSize = unsafe extern "C" fn(
    quality: i32,
    out_width: u32,
    out_height: u32,
    width: *mut u32,
    height: *mut u32,
) -> u32;
type Create = unsafe extern "C" fn(
    device: vk::Device,
    command: vk::CommandBuffer,
    feature: u32,
    quality: i32,
    width: u32,
    height: u32,
    out_width: u32,
    out_height: u32,
    handle: *mut *mut c_void,
) -> u32;
type Evaluate = unsafe extern "C" fn(
    command: vk::CommandBuffer,
    handle: *mut c_void,
    feature: u32,
    frame: *const NgxFrame,
) -> u32;
type Release = unsafe extern "C" fn(handle: *mut c_void) -> u32;
type Shutdown = unsafe extern "C" fn(device: vk::Device) -> u32;

/// NGX's messages, to the log (a few, not a flood).
unsafe extern "C" fn ngx_log(message: *const c_char) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SHOWN: AtomicU32 = AtomicU32::new(0);
    if message.is_null() || SHOWN.fetch_add(1, Ordering::Relaxed) >= 40 {
        return;
    }
    let text = CStr::from_ptr(message).to_string_lossy();
    eprintln!("Infestation: NGX: {}", text.trim_end());
}

/// The bridge, loaded for one device's life.
pub(super) struct Ngx {
    _library: libloading::Library,
    /// Where the bridge loaded from; NVIDIA's DLSS libraries sit beside it.
    path: PathBuf,
    instance_extensions: InstanceExtensions,
    device_extensions: DeviceExtensions,
    init: Init,
    available: Available,
    render_size: RenderSize,
    create: Create,
    evaluate: Evaluate,
    release: Release,
    shutdown: Shutdown,
    /// Where NGX keeps its logs and files (NUL-terminated UTF-16).
    data_path: Vec<u16>,
    /// None until DLSS is first wanted; then whether NGX started.
    started: Cell<Option<bool>>,
    /// The kept file and this device's key in it, once known.
    kept: RefCell<Option<(PathBuf, String)>>,
}

impl Ngx {
    /// The bridge beside the executable (or where `INF_NGX_DLL` says), if
    /// it is there and complete.
    pub(super) fn load() -> Option<Self> {
        let path = match std::env::var_os("INF_NGX_DLL") {
            Some(path) => PathBuf::from(path),
            None => std::env::current_exe().ok()?.parent()?.join("inf_ngx.dll"),
        };
        if !path.exists() {
            return None;
        }
        // SAFETY: the bridge's entry points have exactly these signatures
        // (ngx-bridge/inf_ngx.cpp); the library outlives every call.
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
            let data_path = std::env::temp_dir().join("Infestation NGX");
            let _ = std::fs::create_dir_all(&data_path);
            Some(Self {
                instance_extensions: std::mem::transmute(symbol(b"inf_ngx_instance_extensions\0")?),
                device_extensions: std::mem::transmute(symbol(b"inf_ngx_device_extensions\0")?),
                init: std::mem::transmute(symbol(b"inf_ngx_init\0")?),
                available: std::mem::transmute(symbol(b"inf_ngx_available\0")?),
                render_size: std::mem::transmute(symbol(b"inf_ngx_render_size\0")?),
                create: std::mem::transmute(symbol(b"inf_ngx_create\0")?),
                evaluate: std::mem::transmute(symbol(b"inf_ngx_evaluate\0")?),
                release: std::mem::transmute(symbol(b"inf_ngx_release\0")?),
                shutdown: std::mem::transmute(symbol(b"inf_ngx_shutdown\0")?),
                data_path: data_path
                    .to_string_lossy()
                    .encode_utf16()
                    .chain([0])
                    .collect(),
                started: Cell::new(None),
                kept: RefCell::new(None),
                _library: library,
                path,
            })
        }
    }

    /// The instance extensions DLSS's features ask for.
    pub(super) unsafe fn instance_extensions(&self) -> Vec<CString> {
        let mut names = Vec::new();
        for feature in [DlssFeature::SuperResolution, DlssFeature::RayReconstruction] {
            let (mut count, mut properties) = (0, std::ptr::null_mut());
            let result = (self.instance_extensions)(
                feature as u32,
                self.data_path.as_ptr(),
                &mut count,
                &mut properties,
            );
            Self::collect(result, count, properties, &mut names);
        }
        names
    }

    /// The device extensions DLSS's features ask for on `physical`.
    pub(super) unsafe fn device_extensions(
        &self,
        instance: vk::Instance,
        physical: vk::PhysicalDevice,
    ) -> Vec<CString> {
        let mut names = Vec::new();
        for feature in [DlssFeature::SuperResolution, DlssFeature::RayReconstruction] {
            let (mut count, mut properties) = (0, std::ptr::null_mut());
            let result = (self.device_extensions)(
                instance,
                physical,
                feature as u32,
                self.data_path.as_ptr(),
                &mut count,
                &mut properties,
            );
            Self::collect(result, count, properties, &mut names);
        }
        names
    }

    unsafe fn collect(
        result: u32,
        count: u32,
        properties: *const vk::ExtensionProperties,
        names: &mut Vec<CString>,
    ) {
        if !succeeded(result) || properties.is_null() {
            return;
        }
        for property in std::slice::from_raw_parts(properties, count as usize) {
            if let Ok(name) = property.extension_name_as_c_str() {
                if !names.iter().any(|known| known.as_c_str() == name) {
                    names.push(name.to_owned());
                }
            }
        }
    }

    /// [`Self::device_extensions`], kept in the file at `kept` (none: ask
    /// NGX). Asking takes about 0.6 s, most of a Vulkan start, and the
    /// answer changes only with the GPU, its driver or the DLSS libraries,
    /// so a file entry for those is used while it holds.
    pub(super) unsafe fn kept_device_extensions(
        &self,
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        kept: Option<&Path>,
    ) -> Vec<CString> {
        let Some(path) = kept else {
            return self.device_extensions(instance.handle(), physical);
        };
        let properties = instance.get_physical_device_properties(physical);
        let libraries = [self.path.clone()]
            .into_iter()
            .chain(["nvngx_dlss.dll", "nvngx_dlssd.dll"].map(|name| self.path.with_file_name(name)))
            .map(|library| {
                std::fs::metadata(&library).map_or_else(
                    |_| "none".to_string(),
                    |metadata| {
                        let modified = metadata
                            .modified()
                            .ok()
                            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                            .map_or(0, |since| since.as_secs());
                        format!("{}@{modified}", metadata.len())
                    },
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let key = format!(
            "{:04x}:{:04x}:{:x}:{libraries}",
            properties.vendor_id, properties.device_id, properties.driver_version
        );
        *self.kept.borrow_mut() = Some((path.to_path_buf(), key.clone()));
        if let Some(names) = kept_extensions(path, &key) {
            return names;
        }
        let names = self.device_extensions(instance.handle(), physical);
        keep_extensions(path, &key, &names);
        names
    }

    /// Whether NGX offers Super Resolution and Ray Reconstruction on this
    /// device, kept beside the extensions: asking starts NGX (over a
    /// second), so a kept answer for this GPU, driver and libraries is used.
    pub(super) unsafe fn offers(&self, core: &Core) -> [bool; 2] {
        let kept = self.kept.borrow().clone();
        if let Some(features) = kept
            .as_ref()
            .and_then(|(path, key)| kept_features(path, key))
        {
            return features;
        }
        let features = if self.start(core).is_ok() {
            [DlssFeature::SuperResolution, DlssFeature::RayReconstruction]
                .map(|feature| (self.available)(feature as u32) != 0)
        } else {
            [false; 2]
        };
        if let Some((path, key)) = &kept {
            keep_features(path, key, features);
        }
        features
    }

    /// Start NGX on the device the first time DLSS is wanted.
    unsafe fn start(&self, core: &Core) -> Result<(), String> {
        if self.started.get().is_none() {
            let result = (self.init)(
                core.instance.handle(),
                core.physical,
                core.device.handle(),
                core._entry.static_fn().get_instance_proc_addr,
                core.instance.fp_v1_0().get_device_proc_addr,
                self.data_path.as_ptr(),
                Some(ngx_log),
            );
            if !succeeded(result) {
                eprintln!("Infestation: NGX did not start ({result:#x})");
            }
            self.started.set(Some(succeeded(result)));
        }
        match self.started.get() {
            Some(true) => Ok(()),
            _ => Err("NGX did not start".into()),
        }
    }

    /// Shut NGX down once its features are released, before the device goes.
    pub(super) unsafe fn stop(&self, device: vk::Device) {
        if self.started.replace(None) == Some(true) {
            (self.shutdown)(device);
        }
    }
}

/// The kept answers to NGX's device extensions, one per GPU, driver and
/// set of DLSS libraries, the newest first.
#[derive(Default, Serialize, Deserialize)]
struct KeptExtensions {
    entries: Vec<KeptEntry>,
}

#[derive(Serialize, Deserialize)]
struct KeptEntry {
    key: String,
    extensions: Vec<String>,
    /// [`Ngx::offers`]' answer, once asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    features: Option<[bool; 2]>,
}

/// Entries kept at most: one per installation and GPU in use.
const KEPT_ENTRIES: usize = 4;

/// The extensions kept in `path` for `key`, if any.
fn kept_extensions(path: &Path, key: &str) -> Option<Vec<CString>> {
    let kept: KeptExtensions = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let entry = kept.entries.into_iter().find(|entry| entry.key == key)?;
    entry
        .extensions
        .into_iter()
        .map(|name| CString::new(name).ok())
        .collect()
}

/// Keep `names` for `key` in `path`, first among the entries. A file that
/// cannot be written only means asking NGX again next time.
fn keep_extensions(path: &Path, key: &str, names: &[CString]) {
    let mut kept = read_kept(path);
    kept.entries.retain(|entry| entry.key != key);
    kept.entries.insert(
        0,
        KeptEntry {
            key: key.to_string(),
            extensions: names
                .iter()
                .map(|name| name.to_string_lossy().into_owned())
                .collect(),
            features: None,
        },
    );
    kept.entries.truncate(KEPT_ENTRIES);
    write_kept(path, &kept);
}

fn kept_features(path: &Path, key: &str) -> Option<[bool; 2]> {
    read_kept(path)
        .entries
        .into_iter()
        .find(|entry| entry.key == key)?
        .features
}

/// Keep NGX's `features` in `key`'s entry, when it is there.
fn keep_features(path: &Path, key: &str, features: [bool; 2]) {
    let mut kept = read_kept(path);
    if let Some(entry) = kept.entries.iter_mut().find(|entry| entry.key == key) {
        entry.features = Some(features);
        write_kept(path, &kept);
    }
}

fn read_kept(path: &Path) -> KeptExtensions {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_kept(path: &Path, kept: &KeptExtensions) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(bytes) = serde_json::to_vec_pretty(kept) else {
        return;
    };
    // A new file renamed over the old, so a failure keeps the old one.
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if std::fs::write(&temporary, bytes).is_err() || std::fs::rename(&temporary, path).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
}

/// NGX's quality values: MaxPerf, Balanced, MaxQuality, UltraPerformance,
/// UltraQuality, DLAA.
fn perf_quality(quality: UpscaleQuality) -> i32 {
    match quality {
        UpscaleQuality::Performance => 0,
        UpscaleQuality::Balanced => 1,
        UpscaleQuality::Quality => 2,
        UpscaleQuality::UltraPerformance => 3,
        UpscaleQuality::NativeAa => 5,
    }
}

/// One DLSS feature for one display and render size.
pub(super) struct Dlss {
    pub(super) feature: DlssFeature,
    handle: *mut c_void,
    pub(super) display: [u32; 2],
    pub(super) render: [u32; 2],
}

impl Dlss {
    fn ngx(core: &Core) -> Result<&Ngx, String> {
        core.ngx
            .as_ref()
            .ok_or_else(|| "inf_ngx.dll is not beside the executable".to_string())
    }

    /// The render size DLSS asks for at `quality` for this `display`
    /// (Ray Reconstruction takes no other).
    pub(super) unsafe fn render_size(
        core: &Core,
        feature: DlssFeature,
        display: [u32; 2],
        quality: UpscaleQuality,
    ) -> Result<[u32; 2], String> {
        let ngx = Self::ngx(core)?;
        ngx.start(core)?;
        if (ngx.available)(feature as u32) == 0 {
            return Err(format!("the driver does not offer {feature:?}"));
        }
        let mut render = [0; 2];
        let result = (ngx.render_size)(
            perf_quality(quality),
            display[0],
            display[1],
            &mut render[0],
            &mut render[1],
        );
        if !succeeded(result) || render.contains(&0) {
            return Err(format!("DLSS gave no render size ({result:#x})"));
        }
        Ok(render)
    }

    /// Create the feature, recording its setup into `command`.
    pub(super) unsafe fn new(
        core: &Core,
        command: vk::CommandBuffer,
        feature: DlssFeature,
        quality: UpscaleQuality,
        display: [u32; 2],
        render: [u32; 2],
    ) -> Result<Self, String> {
        let ngx = Self::ngx(core)?;
        ngx.start(core)?;
        let mut handle = std::ptr::null_mut();
        let result = (ngx.create)(
            core.device.handle(),
            command,
            feature as u32,
            perf_quality(quality),
            render[0],
            render[1],
            display[0],
            display[1],
            &mut handle,
        );
        if !succeeded(result) || handle.is_null() {
            return Err(format!("DLSS creation returned {result:#x}"));
        }
        Ok(Self {
            feature,
            handle,
            display,
            render,
        })
    }

    /// Record the upscale of the frame's colour into its output.
    pub(super) unsafe fn dispatch(
        &mut self,
        core: &Core,
        command: vk::CommandBuffer,
        frame: &UpscaleFrame,
    ) -> Result<(), String> {
        let ngx = Self::ngx(core)?;
        let mut inputs = NgxFrame {
            color: NgxImage::of(frame.color, LIGHT_IMAGE),
            depth: NgxImage::of(frame.depth, TRACED_DEPTH),
            motion: NgxImage::of(frame.motion, MOTION_IMAGE),
            output: NgxImage::of(frame.output, LIGHT_IMAGE),
            jitter: frame.jitter,
            render: frame.color.extent,
            reset: i32::from(frame.reset),
            frame_ms: frame.frame_ms,
            ..NgxFrame::default()
        };
        if let Some(guides) = &frame.guides {
            inputs.diffuse_albedo = NgxImage::of(guides.diffuse_albedo, COLOR_IMAGE);
            inputs.normal_roughness = NgxImage::of(guides.normal_roughness, LIGHT_IMAGE);
            inputs.specular_albedo = NgxImage::of(guides.specular_albedo, COLOR_IMAGE);
            inputs.world_to_view = guides.world_to_view;
            inputs.view_to_clip = guides.view_to_clip;
        }
        let result = (ngx.evaluate)(command, self.handle, self.feature as u32, &inputs);
        if succeeded(result) {
            Ok(())
        } else {
            Err(format!("DLSS evaluation returned {result:#x}"))
        }
    }

    /// Release the feature; the device must be past its last use.
    pub(super) unsafe fn destroy(self, core: &Core) {
        if let Ok(ngx) = Self::ngx(core) {
            (ngx.release)(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        keep_extensions, keep_features, kept_extensions, kept_features, Dlss, DlssFeature,
        UpscaleQuality,
    };
    use std::ffi::CString;

    /// An entry is found by its key alone, the newest first, and a key the
    /// file lacks (another GPU, driver or DLSS) asks NGX again.
    #[test]
    fn kept_extensions_hold_per_key() {
        let directory = std::env::temp_dir().join(format!("inf-ngx-kept-{}", std::process::id()));
        let path = directory.join("cache").join("ngx-device-extensions.json");
        let names = |list: &[&str]| -> Vec<CString> {
            list.iter()
                .map(|name| CString::new(*name).unwrap())
                .collect()
        };
        assert_eq!(kept_extensions(&path, "a"), None);
        keep_extensions(&path, "a", &names(&["VK_NVX_binary_import"]));
        keep_extensions(
            &path,
            "b",
            &names(&["VK_NVX_image_view_handle", "VK_KHR_push_descriptor"]),
        );
        assert_eq!(
            kept_extensions(&path, "a"),
            Some(names(&["VK_NVX_binary_import"]))
        );
        assert_eq!(
            kept_extensions(&path, "b"),
            Some(names(&[
                "VK_NVX_image_view_handle",
                "VK_KHR_push_descriptor"
            ]))
        );
        assert_eq!(kept_extensions(&path, "c"), None);
        for key in ["c", "d", "e", "f"] {
            keep_extensions(&path, key, &[]);
        }
        assert_eq!(
            kept_extensions(&path, "a"),
            None,
            "only the newest four stay"
        );
        assert_eq!(kept_extensions(&path, "f"), Some(Vec::new()));
        // NGX's answer rides on its key's entry, and only there.
        assert_eq!(kept_features(&path, "f"), None);
        keep_features(&path, "f", [true, false]);
        keep_features(&path, "z", [true, true]);
        assert_eq!(kept_features(&path, "f"), Some([true, false]));
        assert_eq!(kept_features(&path, "z"), None);
        assert_eq!(kept_extensions(&path, "f"), Some(Vec::new()));
        let _ = std::fs::remove_dir_all(directory);
    }

    /// NGX starts on the device (its extensions enabled) and gives DLSS's
    /// render sizes. Skipped without the bridge beside the test or off
    /// NVIDIA hardware.
    #[test]
    #[ignore = "opens a Vulkan device; run explicitly"]
    fn vulkan_dlss_gives_its_render_sizes() {
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video
            .window("DLSS check", 64, 64)
            .hidden()
            .vulkan()
            .build()
            .unwrap();
        let device = super::VkDevice::new(&window, true).unwrap();
        if device.core.ngx.is_none() || !device.description().contains("NVIDIA") {
            return;
        }
        let size = |feature, quality| unsafe {
            Dlss::render_size(&device.core, feature, [3840, 2160], quality)
        };
        assert_eq!(
            size(DlssFeature::SuperResolution, UpscaleQuality::Performance),
            Ok([1920, 1080])
        );
        assert_eq!(
            size(DlssFeature::SuperResolution, UpscaleQuality::Quality),
            Ok([2560, 1440])
        );
        assert_eq!(
            size(DlssFeature::RayReconstruction, UpscaleQuality::NativeAa),
            Ok([3840, 2160])
        );
    }
}
