//! Acceptance for OpenGL drawing above the native resolution: the same game
//! state through `--renderer software` at an original size (the reference)
//! and through OpenGL in a 1280x720 or 1920x1080 window, which draws that
//! original's layout, in every scaling, on five screens.

use inf_game::display_selection::{original_size, tier_for, DisplayMode, MonitorModes};
use inf_game::frontend_app::{
    run_frontend, FrontendAppClock, FrontendAppInput, FrontendAppReport, FrontendAppRequest,
};
use inf_game::presentation::{ClassicScaling, NativeFrame, RendererChoice};
use inf_game::{
    frontend_assets::FrontendAssets, frontend_settings::FrontendSettings, session::SessionPhase,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn key(frame: u64, name: &str, pressed: bool) -> FrontendAppInput {
    FrontendAppInput {
        frame,
        key: name.into(),
        pressed,
    }
}

struct Screen {
    name: &'static str,
    frames: u64,
    mission: Option<u32>,
    keys: &'static [(u64, &'static str)],
    /// Where the scripted keys lead.
    phase: SessionPhase,
}

/// Title skip, root, Start Game, first empty profile, then name entry, as
/// the window smokes drive it.
const NEW_GAME: &[(u64, &str)] = &[
    (5, "Return"),
    (100, "Return"),
    (200, "Return"),
    (230, "Backspace"),
    (240, "A"),
    (260, "Tab"),
];

const SCREENS: &[Screen] = &[
    // Options row 4 constructs the phase-0 startup (language) carousel.
    Screen {
        name: "startup",
        frames: 260,
        mission: None,
        phase: SessionPhase::Startup,
        keys: &[
            (5, "Return"),
            (100, "Down"),
            (105, "Return"),
            (200, "Down"),
            (203, "Down"),
            (206, "Down"),
            (220, "Return"),
        ],
    },
    // A Colour Depth change resets the display at the same size, then Back
    // to Options.
    Screen {
        name: "options-after-video-reset",
        frames: 430,
        mission: None,
        phase: SessionPhase::Frontend,
        keys: &[
            (5, "Return"),
            (100, "Down"),
            (105, "Return"),
            (200, "Return"),
            (300, "Down"),
            (320, "Down"),
            (330, "Down"),
            (340, "Return"),
            (410, "Escape"),
        ],
    },
    Screen {
        name: "engine-intro",
        frames: 300,
        mission: None,
        phase: SessionPhase::Gameplay,
        keys: NEW_GAME,
    },
    Screen {
        name: "level1-briefing",
        frames: 380,
        mission: None,
        phase: SessionPhase::Phase3,
        keys: &[
            (5, "Return"),
            (100, "Return"),
            (200, "Return"),
            (230, "Backspace"),
            (240, "A"),
            (260, "Tab"),
            (320, "Escape"),
        ],
    },
    Screen {
        name: "level1-driving",
        frames: 360,
        mission: Some(18),
        phase: SessionPhase::Gameplay,
        keys: &[
            (5, "Return"),
            (100, "Return"),
            (200, "Return"),
            (230, "Backspace"),
            (240, "A"),
            (260, "Tab"),
            (300, "Escape"),
            (330, "Up"),
            (340, "Right"),
        ],
    },
];

fn request(screen: &Screen, tag: &str, size: [u32; 2]) -> (FrontendAppRequest, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root.join(format!("../.tmp/inf-above-native-{tag}-{nonce}"));
    let settings = directory.join("settings.json");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut saved = FrontendSettings::load_or_defaults(&settings, &assets).unwrap();
    saved.words_mut()[0x40 / 4] = 0;
    saved.persist().unwrap();
    let profiles = directory.join("profiles.sav");
    std::fs::write(&profiles, []).unwrap();
    let mut input_replay = Vec::new();
    for &(frame, name) in screen.keys {
        input_replay.extend([key(frame, name, true), key(frame + 1, name, false)]);
    }
    let snapshot = directory.join("snapshot.png");
    (
        FrontendAppRequest {
            data_root: root,
            settings_path: settings,
            profiles_path: profiles,
            input_replay,
            dimensions: Some(size),
            scaling: None,
            vehicle_position_carry: None,
            vehicle_gun_mounts: None,
            hovercraft_fans: None,
            exact_world_ticks: None,
            ultra_ground: None,
            ambience: None,
            headphones: None,
            database_models: None,
            exact_frontend_ticks: None,
            guided_mounts: None,
            completion_bonus: None,
            paced_actors: None,
            mission_completion: None,
            splash_voice_cap: None,
            all_cheats: None,
            prototype_world: None,
            pelagia_rework: None,
            renderer: Some(RendererChoice::Software),
            controller_layout: None,
            variant: None,
            language: None,
            hidden: true,
            max_frames: Some(screen.frames),
            snapshot: Some(snapshot.clone()),
            snapshots_at: Vec::new(),
            record_session: None,
            audio_out: None,
            mission: screen.mission,
            start: None,
            clock: FrontendAppClock::FixedStep { micros: 125_000 },
            session_record: None,
            session_replay: None,
        },
        snapshot,
    )
}

fn read_rgba(path: &Path) -> ([u32; 2], Vec<u8>) {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    pixels.truncate(info.buffer_size());
    ([info.width, info.height], pixels)
}

fn rgb(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
    let offset = ((y * width + x) * 4) as usize;
    [pixels[offset], pixels[offset + 1], pixels[offset + 2]]
}

fn differs(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
    a.iter().zip(b).any(|(a, b)| a.abs_diff(b) > tolerance)
}

/// How an OpenGL frame drawn at its presented size compares with the
/// native software frame.
#[derive(Debug)]
struct Comparison {
    native_pixels: usize,
    /// Native pixels whose OpenGL sample (the drawn pixel under the native
    /// pixel's centre) differs from the software pixel.
    downscaled_differences: usize,
    /// Drawn pixels inside a uniform 3x3 native region that differ from it:
    /// a seam or gap would show here.
    interior_pixels: usize,
    interior_differences: usize,
}

fn compare(
    native: ([u32; 2], &[u8]),
    output: ([u32; 2], &[u8]),
    scaling: ClassicScaling,
) -> Comparison {
    let ([width, height], software) = native;
    let (size, pixels) = output;
    let rect = scaling.destination([width, height], size);
    // The presented rectangle holds the drawn frame one to one.
    let drawn = |x: u32, y: u32| {
        rgb(
            pixels,
            size[0],
            (rect.x as u32 + x).min(size[0] - 1),
            (rect.y as u32 + y).min(size[1] - 1),
        )
    };
    let mut downscaled_differences = 0;
    for y in 0..height {
        let ty = ((f64::from(y) + 0.5) * f64::from(rect.height) / f64::from(height)) as u32;
        for x in 0..width {
            let tx = ((f64::from(x) + 0.5) * f64::from(rect.width) / f64::from(width)) as u32;
            if differs(drawn(tx, ty), rgb(software, width, x, y), 24) {
                downscaled_differences += 1;
            }
        }
    }
    let uniform = |x: u32, y: u32| {
        if x == 0 || y == 0 || x + 1 >= width || y + 1 >= height {
            return false;
        }
        let centre = rgb(software, width, x, y);
        (y - 1..=y + 1).all(|ny| (x - 1..=x + 1).all(|nx| rgb(software, width, nx, ny) == centre))
    };
    let (mut interior_pixels, mut interior_differences) = (0, 0);
    for ty in 0..rect.height {
        let y = (u64::from(ty) * u64::from(height) / u64::from(rect.height)) as u32;
        for tx in 0..rect.width {
            let x = (u64::from(tx) * u64::from(width) / u64::from(rect.width)) as u32;
            if uniform(x, y) {
                interior_pixels += 1;
                if differs(drawn(tx, ty), rgb(software, width, x, y), 24) {
                    interior_differences += 1;
                }
            }
        }
    }
    Comparison {
        native_pixels: (width * height) as usize,
        downscaled_differences,
        interior_pixels,
        interior_differences,
    }
}

fn run(
    screen: &Screen,
    renderer: RendererChoice,
    size: [u32; 2],
    scaling: Option<ClassicScaling>,
) -> (FrontendAppReport, ([u32; 2], Vec<u8>)) {
    let tag = format!("{}-{renderer:?}-{size:?}-{scaling:?}", screen.name)
        .to_lowercase()
        .replace([' ', '[', ']', ','], "");
    let (mut request, snapshot) = request(screen, &tag, size);
    request.renderer = Some(renderer);
    request.scaling = scaling;
    let report = run_frontend(request).unwrap_or_else(|error| panic!("{tag}: {error}"));
    assert_eq!(report.frames, screen.frames, "{tag}");
    assert_eq!(report.pixels.unsupported, 0, "{tag}");
    let image = read_rgba(&snapshot);
    (report, image)
}

/// The window sizes the desktop lists among the widescreen candidates.
fn listed_widescreen_sizes() -> Vec<[u32; 2]> {
    // Hidden windows take any size the Resolution row lists.
    sdl2::hint::set("SDL_WINDOWS_DPI_AWARENESS", "permonitorv2");
    let listed = {
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let size = |mode: sdl2::video::DisplayMode| [mode.w as u32, mode.h as u32];
        MonitorModes::new(
            video.desktop_display_mode(0).ok().map(size),
            (0..video.num_display_modes(0).unwrap())
                .map(|index| size(video.display_mode(0, index).unwrap())),
        )
        .menu_sizes(DisplayMode::Window)
    };
    let sizes: Vec<[u32; 2]> = [[1280, 720], [1920, 1080]]
        .into_iter()
        .filter(|size| listed.contains(size))
        .collect();
    assert!(!sizes.is_empty(), "the desktop lists no widescreen window");
    sizes
}

/// The native frame read back from an output image: the pixel under each
/// native pixel's centre in the presented rectangle.
fn native_samples(
    native: [u32; 2],
    (size, pixels): ([u32; 2], &[u8]),
    scaling: ClassicScaling,
) -> Vec<u8> {
    let rect = scaling.destination(native, size);
    let mut samples = Vec::with_capacity((native[0] * native[1] * 4) as usize);
    for y in 0..native[1] {
        let ty = rect.y as u32
            + ((f64::from(y) + 0.5) * f64::from(rect.height) / f64::from(native[1])) as u32;
        for x in 0..native[0] {
            let tx = rect.x as u32
                + ((f64::from(x) + 0.5) * f64::from(rect.width) / f64::from(native[0])) as u32;
            let offset = ((ty * size[0] + tx) * 4) as usize;
            samples.extend_from_slice(&pixels[offset..offset + 4]);
        }
    }
    samples
}

#[test]
#[ignore = "opens hidden OpenGL windows and an audio device; run explicitly in release mode"]
fn opengl_above_native_stays_close_to_the_software_reference() {
    let sizes = listed_widescreen_sizes();
    let mut failures = Vec::new();
    for screen in SCREENS {
        for &size in &sizes {
            let original = original_size(tier_for(size));
            let (reference, native) = run(screen, RendererChoice::Software, original, None);
            assert_eq!(reference.phase, screen.phase, "{}", screen.name);
            assert_eq!(reference.dimensions, original, "{}", screen.name);
            assert_eq!(native.0, reference.dimensions, "{}", screen.name);
            for scaling in [
                ClassicScaling::FourThree,
                ClassicScaling::Stretched,
                ClassicScaling::Integer,
            ] {
                let (report, output) = run(screen, RendererChoice::OpenGl, size, Some(scaling));
                let label = format!("{}/{}x{}/{scaling:?}", screen.name, size[0], size[1]);
                assert_eq!(report.renderer.name(), "opengl", "{label}");
                assert_eq!(report.phase, reference.phase, "{label}");
                assert_eq!(report.dimensions, reference.dimensions, "{label}");
                assert_eq!(output.0, size, "{label}");
                let comparison = compare((native.0, &native.1), (output.0, &output.1), scaling);
                eprintln!("{label}: {comparison:?}");
                let downscaled =
                    comparison.downscaled_differences as f64 / comparison.native_pixels as f64;
                let interior = comparison.interior_differences as f64
                    / comparison.interior_pixels.max(1) as f64;
                if downscaled > 0.02 || interior > 0.002 {
                    failures.push(format!("{label}: {comparison:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Widescreen, the default: each screen draws a frame at the window's
/// aspect, the CPU raster natively and OpenGL at the presented size, and
/// both agree under each native pixel's centre as at native size. Gameplay
/// and the intro fill the window edge to edge; the language carousel and
/// the briefing draw on the centred 4:3 canvas.
#[test]
#[ignore = "opens hidden OpenGL windows and an audio device; run explicitly in release mode"]
fn widescreen_frames_fill_the_window_on_both_devices() {
    let mut failures = Vec::new();
    for &size in &listed_widescreen_sizes() {
        let tier = original_size(tier_for(size));
        let frame = NativeFrame::for_output(tier, size, ClassicScaling::Widescreen).size;
        for screen in SCREENS {
            let label = format!("{}/{}x{}", screen.name, size[0], size[1]);
            let (software, cpu) = run(screen, RendererChoice::Software, size, None);
            let (opengl, gpu) = run(screen, RendererChoice::OpenGl, size, None);
            for report in [&software, &opengl] {
                assert_eq!(report.scaling, ClassicScaling::Widescreen, "{label}");
                assert_eq!(report.dimensions, frame, "{label}");
                assert_eq!(report.phase, screen.phase, "{label}");
            }
            assert_eq!(cpu.0, size, "{label}");
            assert_eq!(gpu.0, size, "{label}");
            // The frame fills the window: no bar columns.
            let column_has_content =
                |x: u32| (0..size[1]).any(|y| rgb(&cpu.1, size[0], x, y) != [0, 0, 0]);
            let canvas = matches!(screen.name, "startup" | "level1-briefing");
            if canvas {
                // The 4:3 canvas sits in the middle of the frame.
                let side = (frame[0] - frame[1] * 4 / 3) / 2;
                let edge = side * size[0] / frame[0];
                if screen.name == "level1-briefing" {
                    assert!(
                        (0..edge.saturating_sub(2)).all(|x| !column_has_content(x)),
                        "{label}: the briefing's left side is clear"
                    );
                }
                assert!(column_has_content(size[0] / 2), "{label}");
            } else if matches!(screen.name, "level1-driving" | "engine-intro") {
                assert!(column_has_content(0), "{label}: left edge");
                assert!(column_has_content(size[0] - 1), "{label}: right edge");
            }
            let reference = native_samples(frame, (cpu.0, &cpu.1), ClassicScaling::Widescreen);
            let comparison = compare(
                (frame, &reference),
                (gpu.0, &gpu.1),
                ClassicScaling::Widescreen,
            );
            eprintln!("{label}: {comparison:?}");
            let downscaled =
                comparison.downscaled_differences as f64 / comparison.native_pixels as f64;
            let interior =
                comparison.interior_differences as f64 / comparison.interior_pixels.max(1) as f64;
            if downscaled > 0.02 || interior > 0.002 {
                failures.push(format!("{label}: {comparison:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
