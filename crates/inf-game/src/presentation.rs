//! Port-only output presentation, independent from native display tiers.
//!
//! H5/OVL resolution and focal words keep their original tier. A window, mode
//! or desktop larger than the tier (see `display_selection`) presents the
//! completed frame with this policy. Widescreen draws a wider native frame
//! at the tier's height, as V2000's port does; the classic policies enlarge
//! the original 4:3 frame.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ClassicScaling {
    /// Draw the native frame at the output's aspect: the tier's height and a
    /// wider frame, so the world shows more to each side and the frame fills
    /// a widescreen output. An output narrower than 4:3 keeps the tier's own
    /// frame and fits it like `FourThree`.
    #[default]
    Widescreen,
    /// Fit the original 4:3 surface with black bars and point sampling.
    FourThree,
    /// Fill the output; a widescreen output stretches the original 4:3 image.
    Stretched,
    /// Largest whole-number enlargement that fits; fractional fit when smaller.
    Integer,
}

/// Port-only renderer selection. Retail always draws through Direct3D: it
/// rewrites settings `+38` (registry `Renderer`) 0 to 1 at startup and has no
/// Video row for it. The CPU raster reproduces that device's pixel rules;
/// OpenGL and Vulkan draw the same submissions on the GPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum RendererChoice {
    /// OpenGL, or the CPU raster when OpenGL cannot start.
    #[default]
    Auto,
    /// OpenGL only; startup fails when it cannot start.
    #[serde(rename = "opengl")]
    #[value(name = "opengl")]
    OpenGl,
    /// The CPU raster: deterministic frames, the pixel reference.
    Software,
    /// Vulkan, or OpenGL and then the CPU raster when it cannot start.
    Vulkan,
}

/// The renderer a session actually runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererBackend {
    OpenGl,
    Software,
    Vulkan,
}

impl RendererBackend {
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenGl => "opengl",
            Self::Software => "software",
            Self::Vulkan => "vulkan",
        }
    }
}

/// The game a session plays (AGENTS.md, the renderers' roles): retail's on
/// the software raster and OpenGL whatever the camera or the scaling, and
/// the remaster's in Vulkan single player, where the view shown drives the
/// game. Decided when the session's window opens and kept in its replay, so
/// the recorded game replays on any renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameView {
    #[default]
    Retail,
    /// `traced`: the session started path traced, whose longer draw
    /// distance the game then takes.
    Remaster { traced: bool },
}

impl GameView {
    /// The game of a session whose window opened on `backend` (a Vulkan
    /// choice that fell back runs OpenGL), tracing rays in effect or not.
    pub fn at_start(backend: RendererBackend, ray_tracing: bool) -> Self {
        match backend {
            RendererBackend::Vulkan => Self::Remaster {
                traced: ray_tracing,
            },
            RendererBackend::OpenGl | RendererBackend::Software => Self::Retail,
        }
    }

    /// The game one frame plays: retail's in a network game (the process's
    /// network flag set), whatever the session's.
    pub fn for_frame(self, network_flag: u32) -> Self {
        if network_flag != 0 {
            Self::Retail
        } else {
            self
        }
    }
}

/// A session's game, and what that game reads of the machine whose window
/// it opened in, which a replay keeps (`session_log`'s `g` line) to play
/// the same game on any renderer and output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionGame {
    pub view: GameView,
    /// The frame the window opened with ([`NativeFrame::size`]): the
    /// remaster's game sees by its width, retail's by its height.
    pub frame: Option<[u32; 2]>,
    /// What the device ran of the modern mode: the remaster's Vulkan rows,
    /// which a cursor's path through the Options follows (locked rows are
    /// skipped).
    pub support: Option<crate::modern_settings::ModernSupport>,
    /// The fog's `far` and `past` the traced remaster's game sees by.
    pub far: Option<crate::modern_tuning::TracedFar>,
}

/// A replay's recorded frame in place of the frame this run's window opened
/// with (`live`): the remaster's game reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePin {
    pub live: [u32; 2],
    pub pinned: [u32; 2],
}

impl FramePin {
    /// `frame`, the recorded size while this run's frame is the one its
    /// window opened with. A display change that gives another frame takes
    /// it (a replay keeps only the frame its recording opened with), and
    /// says so.
    pub fn apply(self, frame: NativeFrame) -> NativeFrame {
        if frame.size == self.live {
            return NativeFrame {
                size: self.pinned,
                ..frame
            };
        }
        let [width, height] = frame.size;
        eprintln!(
            "Infestation replay: the display now draws {width}x{height}; the recording's \
             frame after this change is unknown, so the remaster's game may part from it"
        );
        frame
    }
}

/// The native frame the game draws: its OVL tier's authored size, which
/// selects resources and focal words, and the size it draws at, which is
/// wider under [`ClassicScaling::Widescreen`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeFrame {
    pub tier_size: [u32; 2],
    pub size: [u32; 2],
}

impl NativeFrame {
    /// The tier's own frame, as retail draws it.
    pub fn original(tier_size: [u32; 2]) -> Self {
        Self {
            tier_size,
            size: tier_size,
        }
    }

    /// The frame `scaling` draws for an `output`: under Widescreen the
    /// tier's height at the output's aspect, rounded to the nearest pixel and
    /// never narrower than the tier.
    pub fn for_output(tier_size: [u32; 2], output: [u32; 2], scaling: ClassicScaling) -> Self {
        let [tier_width, tier_height] = tier_size;
        let width = match scaling {
            ClassicScaling::Widescreen => wide_width(tier_height, output).max(tier_width),
            _ => tier_width,
        };
        Self {
            tier_size,
            size: [width, tier_height],
        }
    }
}

/// The width of a frame `height` pixels tall at `output`'s aspect, rounded
/// to the nearest pixel.
fn wide_width(height: u32, output: [u32; 2]) -> u32 {
    if output[1] == 0 {
        return 0;
    }
    ((u64::from(height) * u64::from(output[0]) + u64::from(output[1]) / 2) / u64::from(output[1]))
        as u32
}

/// The completed frame's physical output: the window's client size and how
/// the native frame fills it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputPresentation {
    pub output: [u32; 2],
    pub scaling: ClassicScaling,
}

/// SDL destination in physical output pixels. This owns no model/UI transforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ClassicScaling {
    pub fn destination(self, framebuffer: [u32; 2], output: [u32; 2]) -> PresentationRect {
        // Runtime framebuffers/output sizes are validated nonzero display modes.
        assert!(framebuffer.into_iter().all(|value| value != 0));
        assert!(output.into_iter().all(|value| value != 0));
        let [source_width, source_height] = framebuffer.map(u64::from);
        let [output_width, output_height] = output.map(u64::from);
        let integer_scale = (output_width / source_width).min(output_height / source_height);
        let [width, height] = match self {
            Self::Stretched => output,
            // A widescreen frame has its output's aspect to the pixel: fill.
            Self::Widescreen if framebuffer[0] == wide_width(framebuffer[1], output) => output,
            Self::Integer if integer_scale != 0 => [
                (source_width * integer_scale) as u32,
                (source_height * integer_scale) as u32,
            ],
            Self::Widescreen | Self::FourThree | Self::Integer => {
                if output_width * source_height <= output_height * source_width {
                    [
                        output[0],
                        (source_height * output_width / source_width) as u32,
                    ]
                } else {
                    [
                        (source_width * output_height / source_height) as u32,
                        output[1],
                    ]
                }
            }
        };
        PresentationRect {
            x: ((output[0] - width) / 2) as i32,
            y: ((output[1] - height) / 2) as i32,
            width,
            height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_policy_never_changes_native_tier_dimensions() {
        for framebuffer in [[320, 240], [512, 384], [640, 480], [800, 600], [1024, 768]] {
            for scaling in [
                ClassicScaling::Widescreen,
                ClassicScaling::FourThree,
                ClassicScaling::Stretched,
                ClassicScaling::Integer,
            ] {
                assert_eq!(
                    scaling.destination(framebuffer, framebuffer),
                    PresentationRect {
                        x: 0,
                        y: 0,
                        width: framebuffer[0],
                        height: framebuffer[1],
                    }
                );
            }
        }
    }

    #[test]
    fn fit_stretch_and_integer_are_distinct_output_policies() {
        let framebuffer = [640, 480];
        assert_eq!(
            ClassicScaling::FourThree.destination(framebuffer, [1280, 720]),
            PresentationRect {
                x: 160,
                y: 0,
                width: 960,
                height: 720,
            }
        );
        assert_eq!(
            ClassicScaling::FourThree.destination(framebuffer, [1920, 1080]),
            PresentationRect {
                x: 240,
                y: 0,
                width: 1440,
                height: 1080,
            }
        );
        assert_eq!(
            ClassicScaling::Stretched.destination(framebuffer, [1280, 720]),
            PresentationRect {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            }
        );
        assert_eq!(
            ClassicScaling::Integer.destination(framebuffer, [1280, 720]),
            PresentationRect {
                x: 320,
                y: 120,
                width: 640,
                height: 480,
            }
        );
        assert_eq!(
            ClassicScaling::Integer.destination(framebuffer, [1920, 1080]),
            PresentationRect {
                x: 320,
                y: 60,
                width: 1280,
                height: 960,
            }
        );
    }

    #[test]
    fn integer_output_falls_back_to_fractional_fit_for_larger_native_framebuffer() {
        assert_eq!(
            ClassicScaling::Integer.destination([1024, 768], [1280, 720]),
            PresentationRect {
                x: 160,
                y: 0,
                width: 960,
                height: 720,
            }
        );
        assert_eq!(
            ClassicScaling::FourThree.destination([640, 480], [601, 901]),
            PresentationRect {
                x: 0,
                y: 225,
                width: 601,
                height: 450,
            }
        );
    }

    #[test]
    fn widescreen_frames_keep_the_tier_height_at_the_output_aspect() {
        let frame =
            |tier, output| NativeFrame::for_output(tier, output, ClassicScaling::Widescreen);
        assert_eq!(frame([1024, 768], [1920, 1080]).size, [1365, 768]);
        assert_eq!(frame([1024, 768], [3840, 2160]).size, [1365, 768]);
        assert_eq!(frame([800, 600], [1280, 720]).size, [1067, 600]);
        assert_eq!(frame([1024, 768], [3440, 1440]).size, [1835, 768]);
        assert_eq!(frame([1024, 768], [2560, 1600]).size, [1229, 768]);
        // A 4:3 output keeps the tier; a narrower one never shrinks it.
        assert_eq!(frame([640, 480], [640, 480]).size, [640, 480]);
        assert_eq!(frame([1024, 768], [1280, 1024]).size, [1024, 768]);
        // The tier selects resources whatever the frame.
        assert_eq!(frame([1024, 768], [1920, 1080]).tier_size, [1024, 768]);
        for scaling in [
            ClassicScaling::FourThree,
            ClassicScaling::Stretched,
            ClassicScaling::Integer,
        ] {
            assert_eq!(
                NativeFrame::for_output([1024, 768], [1920, 1080], scaling),
                NativeFrame::original([1024, 768])
            );
        }
    }

    #[test]
    fn only_vulkan_single_player_plays_the_remaster() {
        assert_eq!(
            GameView::at_start(RendererBackend::Vulkan, true),
            GameView::Remaster { traced: true }
        );
        assert_eq!(
            GameView::at_start(RendererBackend::Vulkan, false),
            GameView::Remaster { traced: false }
        );
        // A Vulkan choice that fell back to OpenGL, or the software raster,
        // plays retail's game, traced or not in the settings.
        for backend in [RendererBackend::OpenGl, RendererBackend::Software] {
            assert_eq!(GameView::at_start(backend, true), GameView::Retail);
        }
        let remaster = GameView::Remaster { traced: true };
        assert_eq!(remaster.for_frame(0), remaster);
        assert_eq!(remaster.for_frame(1), GameView::Retail);
        assert_eq!(GameView::Retail.for_frame(0), GameView::Retail);
    }

    /// A replay's frame holds while the display gives the frame its window
    /// opened with, and gives way to any other.
    #[test]
    fn a_pinned_frame_holds_until_the_display_changes_it() {
        let pin = FramePin {
            live: [1229, 768],
            pinned: [1365, 768],
        };
        let opened = NativeFrame::for_output([1024, 768], [1920, 1200], ClassicScaling::Widescreen);
        assert_eq!(opened.size, pin.live);
        assert_eq!(pin.apply(opened).size, [1365, 768]);
        assert_eq!(pin.apply(opened).tier_size, [1024, 768]);
        let changed = NativeFrame::original([800, 600]);
        assert_eq!(pin.apply(changed), changed);
    }

    #[test]
    fn a_widescreen_frame_fills_its_output_and_fits_a_narrower_one() {
        let fill = ClassicScaling::Widescreen.destination([1365, 768], [1920, 1080]);
        assert_eq!(
            fill,
            PresentationRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            ClassicScaling::Widescreen.destination([1024, 768], [1280, 1024]),
            ClassicScaling::FourThree.destination([1024, 768], [1280, 1024])
        );
    }
}
