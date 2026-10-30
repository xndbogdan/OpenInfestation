//! The Vulkan renderer's modern mode, as the player sets it (the settings
//! sidecar's schema 11, the launcher's Options and the Video menu's added
//! rows): path tracing, the upscaler and its quality, anti-aliasing, the
//! traced world's lighting preset, and Ultra ground detail.
//! OpenGL and the CPU raster keep retail's look and ignore them.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModernSettings {
    /// The world path traced, on a device that traces rays.
    pub ray_tracing: bool,
    pub upscaler: Upscaler,
    pub quality: UpscaleQuality,
    /// Used without an upscaler, which brings its own.
    pub anti_aliasing: AntiAliasing,
    /// The traced world's lighting preset.
    pub lighting: Lighting,
    /// Ultra ground detail: the ground
    /// smoother than retail's High, for play and the traced world alike
    /// ([`crate::terrain_runtime::ultra`]). Offline, on High, with path
    /// tracing on Vulkan ([`crate::frontend_settings::FrontendSettings::ultra_ground`]).
    pub ultra_ground: bool,
    /// The Ambience volume, 0 to 15 like retail's Sound and Music: the sound the port adds, on Vulkan only. 0 gives
    /// back retail's sound exactly.
    pub ambience: u8,
    /// Headphones: the placed sounds reach
    /// each ear apart, the far one later and in the head's shadow, behind
    /// duller; off for speakers, where it sounds odd.
    pub headphones: bool,
    /// Wheels sink into puddles (a gameplay setting that comes with the
    /// remaster, [the plan](../../../docs/rt-plans/ground-detail.md)): the
    /// buggy's and the truck's hull samples meet the puddles' carved bed,
    /// as the traced ground draws it. With path tracing on Vulkan, offline
    /// ([`crate::frontend_settings::FrontendSettings::puddle_wheels`]).
    pub puddle_wheels: bool,
}

impl Default for ModernSettings {
    fn default() -> Self {
        Self {
            ray_tracing: true,
            upscaler: Upscaler::Off,
            quality: UpscaleQuality::Quality,
            // SMAA: the modern default with path tracing (2026-10-27):
            // single-frame like FXAA but with real edge
            // detection, so no temporal blur or ghosting, and far cheaper
            // than supersampling. Off stays the row's own first entry and
            // what an older file without the key reads back as.
            anti_aliasing: AntiAliasing::Smaa,
            lighting: Lighting::Remaster,
            ultra_ground: false,
            ambience: DEFAULT_AMBIENCE,
            headphones: false,
            puddle_wheels: true,
        }
    }
}

/// The Ambience volume a new or older settings file starts at: low, under
/// the game's own sound (2026-10-26: so it is
/// not intrusive).
pub const DEFAULT_AMBIENCE: u8 = 4;
/// The Ambience volume's top, as retail's two volumes.
pub const MOST_AMBIENCE: u8 = 15;

/// What a device can run of the modern mode, asked of it once: whether it
/// traces rays, and each upscaler whose library loads and which the GPU
/// takes. Without traced rays the upscalers anti-alias the raster's world
/// at its native size, and Ray Reconstruction, which denoises traced light,
/// runs nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModernSupport {
    pub ray_tracing: bool,
    /// By [`Upscaler::ALL`]'s order; Off always runs.
    pub upscalers: [bool; 5],
    /// The modern sky (and its rain and lightning) is drawn, traced or not:
    /// the device takes the scalar block layout its passes need.
    pub modern_sky: bool,
}

impl Default for ModernSupport {
    /// Everything, until a device says otherwise.
    fn default() -> Self {
        Self {
            ray_tracing: true,
            upscalers: [true; 5],
            modern_sky: true,
        }
    }
}

impl ModernSupport {
    /// Nothing but Off: a GPU whose Vulkan did not start.
    pub const NONE: Self = Self {
        ray_tracing: false,
        upscalers: [true, false, false, false, false],
        modern_sky: false,
    };

    pub fn runs(&self, upscaler: Upscaler) -> bool {
        upscaler == Upscaler::Off
            || Upscaler::ALL
                .iter()
                .position(|known| *known == upscaler)
                .is_some_and(|index| self.upscalers[index])
    }

    /// The upscalers that run, in the row's order.
    pub fn upscalers(&self) -> Vec<Upscaler> {
        Upscaler::ALL
            .into_iter()
            .filter(|upscaler| self.runs(*upscaler))
            .collect()
    }
}

impl ModernSettings {
    /// The settings as a device with `support` runs them, and as its rows
    /// show them. A saved upscaler it cannot run gives way to the next best
    /// it can: Ray Reconstruction to DLSS, DLSS and XeSS to FSR, FSR to none.
    /// Without ray tracing nothing is traced: an upscaler brings the
    /// raster's world up from its quality's size (Ray Reconstruction, with
    /// no traced light to denoise, as DLSS). The saved choice stays saved,
    /// for a device that runs it.
    pub fn effective(self, support: ModernSupport) -> Self {
        let ray_tracing = self.ray_tracing && support.ray_tracing;
        let mut upscaler = self.upscaler;
        if !ray_tracing && upscaler == Upscaler::DlssRayReconstruction {
            upscaler = Upscaler::Dlss;
        }
        while !support.runs(upscaler) {
            upscaler = match upscaler {
                Upscaler::DlssRayReconstruction => Upscaler::Dlss,
                Upscaler::Dlss | Upscaler::Xess => Upscaler::Fsr,
                Upscaler::Fsr | Upscaler::Off => Upscaler::Off,
            };
        }
        Self {
            ray_tracing,
            upscaler,
            ..self
        }
    }
}

/// Which library brings the traced world up to the display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Upscaler {
    #[default]
    Off,
    Fsr,
    Dlss,
    /// DLSS Ray Reconstruction, which also denoises the bounce light.
    DlssRayReconstruction,
    Xess,
}

impl Upscaler {
    pub const ALL: [Self; 5] = [
        Self::Off,
        Self::Fsr,
        Self::Dlss,
        Self::DlssRayReconstruction,
        Self::Xess,
    ];

    /// The upscaler's own name, the same in every language.
    pub fn name(self) -> Option<&'static [u8]> {
        match self {
            Self::Off => None,
            Self::Fsr => Some(b"FSR"),
            Self::Dlss => Some(b"DLSS"),
            Self::DlssRayReconstruction => Some(b"DLSS RR"),
            Self::Xess => Some(b"XeSS"),
        }
    }
}

/// The upscalers' shared quality modes, by their render scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpscaleQuality {
    NativeAa,
    #[default]
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}

impl UpscaleQuality {
    pub const ALL: [Self; 5] = [
        Self::NativeAa,
        Self::Quality,
        Self::Balanced,
        Self::Performance,
        Self::UltraPerformance,
    ];

    /// How many times smaller each side renders, as FSR defines the modes
    /// (DLSS and XeSS give their own sizes).
    pub fn ratio(self) -> f32 {
        match self {
            Self::NativeAa => 1.0,
            Self::Quality => 1.5,
            Self::Balanced => 1.7,
            Self::Performance => 2.0,
            Self::UltraPerformance => 3.0,
        }
    }
}

/// The world's own anti-aliasing, when no upscaler runs: a pass over its
/// image (FXAA, SMAA, TAA), or supersampling, which draws the whole frame at
/// twice the pixels (each side the square root of two larger) or four times
/// (each side twice) and averages them down to the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AntiAliasing {
    #[default]
    Off,
    Fxaa,
    Smaa,
    Taa,
    Ssaa2x,
    Ssaa4x,
}

impl AntiAliasing {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Fxaa,
        Self::Smaa,
        Self::Taa,
        Self::Ssaa2x,
        Self::Ssaa4x,
    ];

    /// The pass it runs over the world's image: none for supersampling.
    pub fn pass(self) -> Self {
        match self {
            Self::Ssaa2x | Self::Ssaa4x => Self::Off,
            pass => pass,
        }
    }

    /// How much larger, on each side, it draws the frame than the screen
    /// shows it.
    pub fn supersampling(self) -> f32 {
        match self {
            Self::Ssaa2x => std::f32::consts::SQRT_2,
            Self::Ssaa4x => 2.0,
            _ => 1.0,
        }
    }
}

/// How the traced world looks (2026-10-23): Remaster, the
/// restrained look, or Eye candy, a little darker, more saturated and
/// contrasty, with stronger reflections, light shafts and glows. Only a
/// traced world takes it; the tuning file holds each preset's numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lighting {
    #[default]
    Remaster,
    EyeCandy,
}

impl Lighting {
    pub const ALL: [Self; 2] = [Self::Remaster, Self::EyeCandy];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_settings_read_their_names_and_default_the_rest() {
        let saved: ModernSettings = serde_json::from_str(
            r#"{"upscaler": "dlss-ray-reconstruction", "quality": "performance"}"#,
        )
        .unwrap();
        assert_eq!(
            saved,
            ModernSettings {
                upscaler: Upscaler::DlssRayReconstruction,
                quality: UpscaleQuality::Performance,
                ..ModernSettings::default()
            }
        );
        assert!(saved.ray_tracing);
        assert_eq!(
            serde_json::to_string(&ModernSettings::default()).unwrap(),
            r#"{"ray_tracing":true,"upscaler":"off","quality":"quality","anti_aliasing":"smaa","lighting":"remaster","ultra_ground":false,"ambience":4,"headphones":false,"puddle_wheels":true}"#
        );
    }

    /// Each method keeps its name in the sidecar; supersampling runs no
    /// pass over the image but draws larger, each side by the square root
    /// of its sample count.
    #[test]
    fn anti_aliasing_names_passes_and_sizes() {
        let names: Vec<String> = AntiAliasing::ALL
            .iter()
            .map(|method| serde_json::to_string(method).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                r#""off""#,
                r#""fxaa""#,
                r#""smaa""#,
                r#""taa""#,
                r#""ssaa2x""#,
                r#""ssaa4x""#
            ]
        );
        let passes = AntiAliasing::ALL.map(AntiAliasing::pass);
        assert_eq!(
            passes,
            [
                AntiAliasing::Off,
                AntiAliasing::Fxaa,
                AntiAliasing::Smaa,
                AntiAliasing::Taa,
                AntiAliasing::Off,
                AntiAliasing::Off,
            ]
        );
        let sides = AntiAliasing::ALL.map(AntiAliasing::supersampling);
        assert_eq!(sides[..4], [1.0; 4]);
        assert!((sides[4] * sides[4] - 2.0).abs() < 1.0e-6);
        assert_eq!(sides[5], 2.0);
    }

    /// On an AMD card, saved DLSS runs as FSR, and Ray Reconstruction as
    /// FSR too; without FSR's library, as none; without ray tracing nothing
    /// is traced, while the saved choices stay as they were.
    #[test]
    fn a_saved_choice_the_device_cannot_run_gives_way_to_the_next_best() {
        let amd = ModernSupport {
            ray_tracing: true,
            upscalers: [true, true, false, false, true],
            modern_sky: true,
        };
        let saved = ModernSettings {
            upscaler: Upscaler::DlssRayReconstruction,
            ..ModernSettings::default()
        };
        assert_eq!(saved.effective(amd).upscaler, Upscaler::Fsr);
        assert_eq!(
            amd.upscalers(),
            [Upscaler::Off, Upscaler::Fsr, Upscaler::Xess]
        );
        let bare = ModernSupport {
            ray_tracing: false,
            upscalers: [true, false, false, false, false],
            modern_sky: true,
        };
        let shown = saved.effective(bare);
        assert_eq!((shown.upscaler, shown.ray_tracing), (Upscaler::Off, false));
        assert_eq!(saved.upscaler, Upscaler::DlssRayReconstruction);
        assert_eq!(saved.effective(ModernSupport::default()), saved);
    }

    /// With Ray Tracing off an upscaler still brings the raster's world up
    /// from its quality's size (upscale quality can be
    /// adjusted regardless of the ray tracing option), and Ray
    /// Reconstruction, with no traced light to denoise, runs as DLSS.
    #[test]
    fn without_ray_tracing_an_upscaler_keeps_its_quality() {
        let saved = ModernSettings {
            ray_tracing: false,
            upscaler: Upscaler::DlssRayReconstruction,
            quality: UpscaleQuality::Performance,
            anti_aliasing: AntiAliasing::Taa,
            lighting: Lighting::EyeCandy,
            ultra_ground: false,
            ambience: 0,
            headphones: false,
            puddle_wheels: true,
        };
        let shown = saved.effective(ModernSupport::default());
        assert_eq!(
            (shown.upscaler, shown.quality),
            (Upscaler::Dlss, UpscaleQuality::Performance)
        );
        let traced = ModernSettings {
            ray_tracing: true,
            ..saved
        };
        assert_eq!(traced.effective(ModernSupport::default()), traced);
        // A GPU without ray tracing and without NGX: FSR, at its quality.
        let untraced = ModernSupport {
            ray_tracing: false,
            upscalers: [true, true, false, false, true],
            modern_sky: true,
        };
        let shown = traced.effective(untraced);
        assert_eq!(
            (shown.ray_tracing, shown.upscaler, shown.quality),
            (false, Upscaler::Fsr, UpscaleQuality::Performance)
        );
    }

    /// The presets keep their names in the sidecar; a sidecar without one
    /// reads as Remaster.
    #[test]
    fn lighting_presets_keep_their_names() {
        let names = Lighting::ALL.map(|preset| serde_json::to_string(&preset).unwrap());
        assert_eq!(names, [r#""remaster""#, r#""eye-candy""#]);
        let saved: ModernSettings = serde_json::from_str(r#"{"lighting": "eye-candy"}"#).unwrap();
        assert_eq!(saved.lighting, Lighting::EyeCandy);
        let bare: ModernSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(bare.lighting, Lighting::Remaster);
    }
}
