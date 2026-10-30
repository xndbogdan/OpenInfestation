//! Video-menu device state and portable Direct3D presentation policy.
//!
//! 00462DE0/00462EE0 enumerate at most four DirectDraw adapters. SDL output
//! devices replace that platform list; the CPU backend still implements the
//! recovered Direct3D renderer, not the retail software renderer.

use thiserror::Error;

use crate::modern_settings::{
    AntiAliasing, Lighting, ModernSettings, ModernSupport, UpscaleQuality, Upscaler,
};

use crate::{
    frontend_settings::SETTINGS_WORDS, menu::MenuCommand, settings_row::adjust_value,
    software_scene::SoftwareSceneConfig,
};

pub const VIDEO_DEVICE_CELL: u32 = 0x0054_EC50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoDevice {
    /// Portable output identity, supplied by SDL display enumeration.
    pub id: i32,
    /// Display description bytes, excluding NUL.
    pub name: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct VideoResetRequest<'a> {
    pub variant: u32,
    pub words: &'a [u32; SETTINGS_WORDS],
    pub device: i32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VideoError {
    #[error("no display output is available")]
    NoDevices,
    #[error("video device row has not initialized its device list")]
    Uninitialized,
    #[error("video device selection {0} is outside the enumerated list")]
    Selection(u32),
    #[error("video device action uses unknown cell {0:#010x}")]
    Cell(u32),
    #[error("retail renderer selector {0} is not implemented by the portable Direct3D backend")]
    Renderer(u8),
}

#[derive(Debug)]
pub struct FrontendVideoState {
    devices: Vec<VideoDevice>,
    selected: u32,
    initialized: bool,
    published_device: i32,
    /// Diagnostic custody for caught native reset failures, not a UI timer.
    pub last_reset_error: Option<String>,
    pub last_device_save_error: Option<String>,
}

impl FrontendVideoState {
    pub fn new(
        mut devices: Vec<VideoDevice>,
        preferred: Option<&[u8]>,
    ) -> Result<Self, VideoError> {
        if devices.is_empty() {
            return Err(VideoError::NoDevices);
        }
        for device in &mut devices {
            if let Some(nul) = device.name.iter().position(|&byte| byte == 0) {
                device.name.truncate(nul);
            }
        }
        let selected = preferred
            .map(|name| {
                let end = name
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(name.len());
                &name[..end]
            })
            .filter(|name| !name.is_empty())
            .and_then(|name| devices.iter().rposition(|device| device.name == name))
            .unwrap_or(0) as u32;
        let published_device = devices[selected as usize].id;
        // Startup 00463320 matches the entire list and retains the last match.
        // Row enumeration 00462EE0 later retains only the first four; its first
        // draw clamps the row selection without changing the active device.
        devices.truncate(4);
        Ok(Self {
            devices,
            selected,
            initialized: false,
            published_device,
            last_reset_error: None,
            last_device_save_error: None,
        })
    }

    pub fn selected(&self) -> u32 {
        self.selected
    }

    pub fn published_device(&self) -> i32 {
        self.published_device
    }

    /// 00462DE0's first draw publishes an inclusive maximum, then clamps only
    /// a signed selection above that maximum. The portable list's metadata
    /// was obtained at startup; row initialization still happens on this draw.
    pub fn device_name_for_draw(&mut self) -> Result<Vec<u8>, VideoError> {
        if !self.initialized {
            let maximum = self.devices.len() as u32 - 1;
            if self.selected as i32 > maximum as i32 {
                self.selected = maximum;
            }
            self.initialized = true;
        }
        let device = self
            .devices
            .get(self.selected as usize)
            .ok_or(VideoError::Selection(self.selected))?;
        Ok(device_row_name(&device.name))
    }

    /// 00463000 always publishes the selected device and runs its tail,
    /// including unchanged and ignored commands. The tail persists the full
    /// description before resetting. On failure only the selection rolls back;
    /// the published device and any completed persistence remain changed.
    pub fn adjust_device<E>(
        &mut self,
        address: u32,
        command: MenuCommand,
        words: &[u32; SETTINGS_WORDS],
        mut tail: impl FnMut(&VideoDevice, VideoResetRequest<'_>) -> Result<(), E>,
    ) -> Result<Result<(), E>, VideoError> {
        if address != VIDEO_DEVICE_CELL {
            return Err(VideoError::Cell(address));
        }
        if !self.initialized {
            return Err(VideoError::Uninitialized);
        }
        let old = self.selected;
        adjust_value(command, self.devices.len() as u32 - 1, &mut self.selected);
        let device = self
            .devices
            .get(self.selected as usize)
            .ok_or(VideoError::Selection(self.selected))?;
        self.published_device = device.id;
        // Unlike 00462F40, >=5 becomes local five, without a settings store.
        let variant = if words[3] as i32 > 4 { 5 } else { words[3] };
        let result = tail(
            device,
            VideoResetRequest {
                variant,
                words,
                device: device.id,
            },
        );
        if result.is_err() {
            self.selected = old;
        }
        Ok(result)
    }
}

/// Exactly 25 bytes also get an ellipsis: native tests the copied strlen,
/// not whether the source had more bytes after its 25-byte prefix.
pub fn device_row_name(name: &[u8]) -> Vec<u8> {
    let end = name
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(name.len());
    let mut text = name[..end.min(25)].to_vec();
    if text.len() == 25 {
        text.extend_from_slice(b"...");
    }
    text
}

/// 004631E0 repairs the old retail-software selection after loading settings.
/// This is a DWORD-zero test; truncation to a renderer byte happens later.
pub fn prepare_video_startup(words: &mut [u32; SETTINGS_WORDS]) {
    if words[0x38 / 4] == 0 {
        words[0x38 / 4] = 1;
    }
}

/// Settings `+3C`, the Filtering row (retail's Bilinear Filtering): 0 Point
/// and 1 Bilinear on every renderer, as retail; Vulkan adds 2 Trilinear and
/// 3 to 6 Anisotropic 2x, 4x, 8x and 16x, for the traced world.
pub const FILTERING_CELL: usize = 0x3c / 4;
/// The Filtering row's highest value: retail's Bilinear, or Vulkan's
/// Anisotropic 16x.
pub const FILTERING_RETAIL_MOST: u32 = 1;
pub const FILTERING_VULKAN_MOST: u32 = 6;

/// The Filtering a fresh install starts at: Anisotropic 8x. Bilinear is what
/// retail offers, but a modern default filters trilinearly with 8x
/// anisotropy (2026-10-27); under OpenGL and Software it still
/// draws as Bilinear (see [`filtering_most`]).
pub const FILTERING_FRESH_DEFAULT: u32 = 5;

/// The highest Filtering value `backend` offers. A saved value beyond it
/// stays saved and draws as it: OpenGL and Software draw any nonzero value
/// bilinear.
pub fn filtering_most(backend: crate::presentation::RendererBackend) -> u32 {
    if backend == crate::presentation::RendererBackend::Vulkan {
        FILTERING_VULKAN_MOST
    } else {
        FILTERING_RETAIL_MOST
    }
}

/// The anisotropy a Filtering value samples with: 2x to 16x from value 3,
/// none below it.
pub fn filtering_anisotropy(level: u32) -> u32 {
    if level >= 3 {
        1 << (level.min(FILTERING_VULKAN_MOST) - 2)
    } else {
        1
    }
}

/// The Filtering row's text for `level` in `language`.
pub fn filtering_text(level: u32, language: u8) -> Vec<u8> {
    use crate::port_text::PortText;
    match level {
        0 => PortText::Point.text(language).to_vec(),
        1 => PortText::Bilinear.text(language).to_vec(),
        2 => PortText::Trilinear.text(language).to_vec(),
        _ => {
            let mut text = PortText::Anisotropic.text(language).to_vec();
            text.extend_from_slice(format!(" {}x", filtering_anisotropy(level)).as_bytes());
            text
        }
    }
}

/// The Video menu's rows the port adds under Vulkan, after retail's: Ray
/// Tracing, Upscaler, Upscaling, Anti-aliasing and Lighting.
/// Their cells lie past the native settings words, where only the port's
/// rows read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModernRow {
    RayTracing,
    Upscaler,
    Upscaling,
    AntiAliasing,
    Lighting,
}

/// The first modern row's cell address.
pub const MODERN_CELLS: u32 = crate::frontend_settings::SETTINGS_STATE_ADDRESS + 0x1000;

/// Retail's Video menu.
pub const VIDEO_MENU: u32 = 0x0053_9088;
/// The in-world Options' Video menu: the same rows (`00538EC8`) under the
/// in-world chain (`00538D58`).
pub const IN_WORLD_VIDEO_MENU: u32 = 0x0053_90a0;

/// `value`'s place among a row's values (the first when absent).
fn index_of<T: PartialEq + Copy>(all: &[T], value: T) -> u32 {
    all.iter().position(|&item| item == value).unwrap_or(0) as u32
}

/// The row value at `index`, the last when past them.
fn pick<T: Copy>(all: &[T], index: u32) -> T {
    all[index.min(all.len() as u32 - 1) as usize]
}

/// Add the modern rows to a Video menu's definition, before Back: copies
/// of the Filtering row on the port's cells, at addresses no retail record
/// has. The last layout counts them too, so its visible rows (thirteen in
/// the frontend, ten in the world) scroll, as retail's longer lists do.
pub fn append_modern_rows(menu: &mut inf_formats::FrontendMenu) {
    let filtering = crate::frontend_settings::SETTINGS_STATE_ADDRESS + 0x3c;
    let Some(template) = menu
        .items
        .iter()
        .find(|row| row.draw_arguments[1] == filtering)
        .copied()
    else {
        return;
    };
    let back = menu.items.pop();
    for (index, row) in ModernRow::ALL.into_iter().enumerate() {
        menu.items.push(inf_formats::FrontendMenuItem {
            virtual_address: 0xfff0_0000 + 0x20 * index as u32,
            span: inf_formats::ByteSpan { start: 0, end: 0 },
            draw_arguments: [template.draw_arguments[0], row.address()],
            action_arguments: [ModernRow::ALL.len() as u32, row.address()],
            ..template
        });
    }
    menu.items.extend(back);
    if let Some(layout) = menu.layouts.last_mut() {
        layout.item_count += ModernRow::ALL.len() as u8;
    }
}

impl ModernRow {
    pub const ALL: [Self; 5] = [
        Self::RayTracing,
        Self::Upscaler,
        Self::Upscaling,
        Self::AntiAliasing,
        Self::Lighting,
    ];

    pub fn address(self) -> u32 {
        MODERN_CELLS + 4 * self as u32
    }

    pub fn from_address(address: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|row| row.address() == address)
    }

    pub fn label(self, language: u8) -> &'static [u8] {
        use crate::port_text::PortText;
        match self {
            Self::RayTracing => PortText::RayTracing,
            Self::Upscaler => PortText::Upscaler,
            Self::Upscaling => PortText::Upscaling,
            Self::AntiAliasing => PortText::AntiAliasing,
            Self::Lighting => PortText::Lighting,
        }
        .text(language)
    }

    /// The row's value and its highest, as the device runs them
    /// ([`ModernSettings::effective`]). Ray Tracing has none to move
    /// without ray tracing, the Upscaler offers only the upscalers that
    /// run, Upscaling moves whenever an upscaler runs, traced or not (Off
    /// with none), and Anti-aliasing has none while an upscaler brings its
    /// own. Lighting moves traced or not: the raster grades its world too.
    pub fn value(self, modern: &ModernSettings, support: ModernSupport) -> (u32, u32) {
        let modern = modern.effective(support);
        match self {
            Self::RayTracing if !support.ray_tracing => (0, 0),
            Self::RayTracing => (u32::from(modern.ray_tracing), 1),
            Self::Upscaler => {
                let runs = row_upscalers(&modern, support);
                (index_of(&runs, modern.upscaler), runs.len() as u32 - 1)
            }
            Self::Upscaling if modern.upscaler == Upscaler::Off => {
                (index_of(&UpscaleQuality::ALL, modern.quality), 0)
            }
            Self::Upscaling => (
                index_of(&UpscaleQuality::ALL, modern.quality),
                UpscaleQuality::ALL.len() as u32 - 1,
            ),
            Self::AntiAliasing if modern.upscaler != Upscaler::Off => (0, 0),
            Self::AntiAliasing => (
                index_of(&AntiAliasing::ALL, modern.anti_aliasing),
                AntiAliasing::ALL.len() as u32 - 1,
            ),
            Self::Lighting => (
                index_of(&Lighting::ALL, modern.lighting),
                Lighting::ALL.len() as u32 - 1,
            ),
        }
    }

    /// Move the row by `command`, as retail's value rows move (clamped
    /// sideways, wrapping on Return), from what it shows; whether anything
    /// changed.
    pub fn adjust(
        self,
        modern: &mut ModernSettings,
        support: ModernSupport,
        command: crate::menu::MenuCommand,
    ) -> bool {
        let (mut value, most) = self.value(modern, support);
        if most == 0 {
            return false;
        }
        let old = value;
        crate::settings_row::adjust_value(command, most, &mut value);
        if value == old {
            return false;
        }
        match self {
            Self::RayTracing => modern.ray_tracing = value != 0,
            Self::Upscaler => {
                let runs = row_upscalers(&modern.effective(support), support);
                modern.upscaler = pick(&runs, value);
            }
            Self::Upscaling => modern.quality = pick(&UpscaleQuality::ALL, value),
            Self::AntiAliasing => modern.anti_aliasing = pick(&AntiAliasing::ALL, value),
            Self::Lighting => modern.lighting = pick(&Lighting::ALL, value),
        }
        value != old
    }

    /// The row's value as text in `language`, as the device runs it.
    /// Anti-aliasing names the running upscaler in place of its own value,
    /// Upscaling is Off without one, and Ray Tracing says when the GPU has
    /// none.
    pub fn value_text(
        self,
        modern: &ModernSettings,
        support: ModernSupport,
        language: u8,
    ) -> Vec<u8> {
        use crate::port_text::PortText;
        let modern = modern.effective(support);
        let text: &[u8] = match self {
            Self::RayTracing if !support.ray_tracing => PortText::Unavailable.text(language),
            Self::RayTracing if modern.ray_tracing => PortText::PathTracing.text(language),
            Self::Upscaler | Self::AntiAliasing if modern.upscaler != Upscaler::Off => {
                modern.upscaler.name().unwrap_or_default()
            }
            Self::Upscaling if modern.upscaler == Upscaler::Off => PortText::Off.text(language),
            Self::Upscaling => match modern.quality {
                UpscaleQuality::NativeAa => PortText::NativeAa.text(language),
                UpscaleQuality::Quality => PortText::Quality.text(language),
                UpscaleQuality::Balanced => PortText::Balanced.text(language),
                UpscaleQuality::Performance => PortText::Performance.text(language),
                UpscaleQuality::UltraPerformance => PortText::UltraPerformance.text(language),
            },
            Self::AntiAliasing => match modern.anti_aliasing {
                AntiAliasing::Off => PortText::Off.text(language),
                AntiAliasing::Fxaa => b"FXAA",
                AntiAliasing::Smaa => b"SMAA",
                AntiAliasing::Taa => b"TAA",
                AntiAliasing::Ssaa2x => b"SSAA 2x",
                AntiAliasing::Ssaa4x => b"SSAA 4x",
            },
            Self::Lighting => match modern.lighting {
                Lighting::Remaster => PortText::Remaster.text(language),
                Lighting::EyeCandy => PortText::EyeCandy.text(language),
            },
            Self::RayTracing | Self::Upscaler => PortText::Off.text(language),
        };
        text.to_vec()
    }
}

/// Navigation bit of a row the cursor skips. Retail's header rows carry
/// the same bit; the framework draws them without a highlight.
pub const DISABLED_ROW: u32 = 8;

/// Whether the port row with `arguments` is locked under `modern` and
/// `support`: the same `most == 0` that keeps [`ModernRow::adjust`] from
/// moving it. Anything else (retail rows, Filtering, the sound rows) never
/// locks.
pub fn modern_row_locked(
    modern: &ModernSettings,
    support: ModernSupport,
    arguments: [u32; 2],
) -> bool {
    ModernRow::from_address(arguments[1]).is_some_and(|row| row.value(modern, support).1 == 0)
}

/// Refresh the port rows' navigation bit from the live settings, on every
/// draw of a port row: locked rows carry bit 8 so the cursor skips them like
/// a header, unlocked rows clear it. Retail rows are untouched.
pub fn refresh_disabled_rows(
    items: &mut [inf_formats::FrontendMenuItem],
    modern: &ModernSettings,
    support: ModernSupport,
) {
    for item in items.iter_mut() {
        if ModernRow::from_address(item.draw_arguments[1]).is_some() {
            if modern_row_locked(modern, support, item.draw_arguments) {
                item.flags |= DISABLED_ROW;
            } else {
                item.flags &= !DISABLED_ROW;
            }
        }
    }
}

/// A locked row's text color: the row's own color at half brightness, so it
/// reads disabled in every menu theme.
pub(crate) fn dimmed_row_color(color: crate::menu::MenuColor) -> crate::menu::MenuColor {
    crate::menu::MenuColor {
        rgb: color.rgb.map(|channel| channel / 2),
        high_byte: color.high_byte,
    }
}

/// What a Video menu row that resets the display (`00462F40`) does when the
/// in-world Options move it while playing (2026-10-22).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayingDisplayRow {
    /// A Vulkan row: it moves as the frontend's does, from the next frame.
    Modern(ModernRow),
    /// Filtering (`+3C`): it moves from what it draws, from the next frame.
    Filtering,
    /// Resolution, colour depth and display mode need a display reset,
    /// which waits for the frontend.
    Frontend,
}

/// The in-world Options' display row at settings cell `address`.
pub fn playing_display_row(address: u32) -> PlayingDisplayRow {
    if let Some(row) = ModernRow::from_address(address) {
        PlayingDisplayRow::Modern(row)
    } else if address == crate::frontend_settings::SETTINGS_STATE_ADDRESS + 0x3c {
        PlayingDisplayRow::Filtering
    } else {
        PlayingDisplayRow::Frontend
    }
}

/// The upscalers the Upscaler row steps through, under the `effective`
/// settings: those the device runs, less Ray Reconstruction while nothing is
/// traced (it runs as DLSS there).
fn row_upscalers(effective: &ModernSettings, support: ModernSupport) -> Vec<Upscaler> {
    support
        .upscalers()
        .into_iter()
        .filter(|upscaler| effective.ray_tracing || *upscaler != Upscaler::DlssRayReconstruction)
        .collect()
}

/// 00437CA0 narrows these settings to bytes. The portable target implements
/// native renderer1; bilinear maps to D3D states17/18 at 0050C957/0050CA06.
/// Z-buffer setting +50 belongs to world draw 00488020, not frontend draw.
/// ColourDepth +44 is not read by this display construction path.
pub fn apply_video_pipeline(
    words: &[u32; SETTINGS_WORDS],
    scene: &mut SoftwareSceneConfig,
) -> Result<(), VideoError> {
    let renderer = words[0x38 / 4] as u8;
    if renderer != 1 {
        return Err(VideoError::Renderer(renderer));
    }
    let filter = if words[0x3c / 4] as u8 == 0 { 1 } else { 2 };
    scene.texture_state.sampling.min_filter = filter;
    scene.texture_state.sampling.mag_filter = filter;
    Ok(())
}

#[cfg(test)]
mod tests;
