//! The launcher on Linux: the Windows launcher's installation check, Play,
//! Options and Quit in an egui window (OpenGL through glow, on X11 or
//! Wayland). The rows and their rules are the Windows launcher's; the file
//! check runs on a worker thread as there.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use inf_game::display_selection::{DisplayMode, DisplaySelection, MonitorModes};
use inf_game::frontend_assets::FrontendAssets;
use inf_game::frontend_settings::{repair, reset, FrontendSettings, SettingsHealth};
use inf_game::launcher_setup::{validate_installation, InstallationReport};
use inf_game::modern_settings::{AntiAliasing, ModernSettings, ModernSupport, Upscaler};
use inf_game::presentation::RendererChoice;

use super::choices::*;
use super::header_animation::HeaderAnimation;
use super::LauncherRequest;

const TITLE: &str = "Infestation";

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;

pub fn show_error(message: &str) {
    let _ = sdl2::messagebox::show_simple_message_box(
        sdl2::messagebox::MessageBoxFlag::ERROR,
        TITLE,
        message,
        None,
    );
}

pub fn run(request: LauncherRequest) -> Result<Option<PathBuf>, String> {
    let header = HeaderAnimation::load()?;
    // SDL reads the modes before the window opens, so the two never share
    // the display connection's start.
    let modes = monitor_modes();
    let selection = Rc::new(RefCell::new(None));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_app_id("infestation-port")
            .with_inner_size([660.0, 470.0])
            .with_min_inner_size([560.0, 400.0]),
        renderer: eframe::Renderer::Glow,
        // Wayland's swap waits for a frame callback, which a hidden or
        // blanked window never gets: the check and close would stall.
        vsync: false,
        ..Default::default()
    };
    let root = request.root.clone();
    let shared = selection.clone();
    let opened = eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Launcher::new(
                request,
                header,
                modes,
                shared,
                FrontendSettings::default_path(),
            )))
        }),
    );
    if let Err(error) = opened {
        // No window (no display server): start as the launcher-less build
        // did, when the installation is ready.
        eprintln!("The launcher window cannot open: {error}");
        let report = validate_installation(&root, &FrontendSettings::default_path());
        return if report.is_ready() {
            Ok(Some(report.root))
        } else {
            Err(report.description())
        };
    }
    Ok(selection.take())
}

/// What the primary display offers, as the game's SDL start reads it.
fn monitor_modes() -> MonitorModes {
    let Ok(video) = sdl2::init().and_then(|sdl| sdl.video()) else {
        return MonitorModes::default();
    };
    let size = |mode: sdl2::video::DisplayMode| [mode.w as u32, mode.h as u32];
    let desktop = video.desktop_display_mode(0).ok().map(size);
    let count = video.num_display_modes(0).unwrap_or(0);
    MonitorModes::new(
        desktop,
        (0..count).filter_map(|index| video.display_mode(0, index).ok().map(size)),
    )
}

struct Notice {
    title: String,
    text: String,
    /// Play's warning: OK then starts the game.
    then_play: bool,
}

/// Options' rows while open, as entry indices like the Windows combos.
struct Pending {
    settings: FrontendSettings,
    modes: MonitorModes,
    display: DisplaySelection,
    renderer: usize,
    scaling: usize,
    filtering: u32,
    ray_tracing: usize,
    upscaler: Upscaler,
    quality: usize,
    anti_aliasing: usize,
    lighting: usize,
    effects: u32,
    music: u32,
    headphones: usize,
    ambience: u32,
    controller: usize,
    camera: usize,
    fixes: [bool; FIXES],
    skip: bool,
    support: ModernSupport,
}

struct Launcher {
    request: LauncherRequest,
    root: PathBuf,
    root_text: String,
    report: Option<InstallationReport>,
    details: String,
    progress: String,
    worker: Option<(Receiver<Result<InstallationReport, String>>, bool)>,
    notice: Option<Notice>,
    options: Option<Pending>,
    modes: MonitorModes,
    support: Option<ModernSupport>,
    header: HeaderAnimation,
    banner: Option<egui::TextureHandle>,
    header_tick: Instant,
    started: Instant,
    close_requested: bool,
    selection: Rc<RefCell<Option<PathBuf>>>,
    /// The game's settings.json (beside the executable; a test's own).
    settings_path: PathBuf,
    /// `INF_LAUNCHER_SNAPSHOT`: a PNG of the launcher's own window once the
    /// check is done (`INF_LAUNCHER_SNAPSHOT_PAGE=options` shows Options).
    snapshot: Option<PathBuf>,
    snapshot_frames: u32,
}

impl Launcher {
    fn new(
        request: LauncherRequest,
        header: HeaderAnimation,
        modes: MonitorModes,
        selection: Rc<RefCell<Option<PathBuf>>>,
        settings_path: PathBuf,
    ) -> Self {
        let root = request.root.clone();
        let mut launcher = Self {
            root_text: root.display().to_string(),
            root,
            request,
            report: None,
            details: "Checking files...".to_string(),
            progress: String::new(),
            worker: None,
            notice: None,
            options: None,
            modes,
            support: None,
            header,
            banner: None,
            header_tick: Instant::now(),
            started: Instant::now(),
            close_requested: false,
            selection,
            settings_path,
            snapshot: std::env::var_os("INF_LAUNCHER_SNAPSHOT").map(PathBuf::from),
            snapshot_frames: 0,
        };
        launcher.check_files(false);
        launcher
    }

    fn busy(&self) -> bool {
        self.worker.is_some()
    }

    fn can_play(&self) -> bool {
        !self.busy()
            && self
                .report
                .as_ref()
                .is_some_and(InstallationReport::is_ready)
    }

    fn settings_fix(&self) -> Option<&SettingsHealth> {
        self.report
            .as_ref()
            .filter(|report| report.only_settings_broken())
            .map(|report| &report.settings)
    }

    fn check_files(&mut self, show_result: bool) {
        let root = self.root.clone();
        let settings_path = self.settings_path.clone();
        self.progress = "Checking Infest.exe, PreLoad.dat, every overlay, the settings, the title video and the music...".to_string();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let checked = std::panic::catch_unwind(|| validate_installation(&root, &settings_path))
                .map_err(|_| "The file check stopped unexpectedly.".to_string());
            let _ = sender.send(checked);
        });
        self.worker = Some((receiver, show_result));
    }

    fn poll_worker(&mut self) {
        let Some((receiver, show_result)) = &self.worker else {
            return;
        };
        let show_result = *show_result;
        let Ok(checked) = receiver.try_recv() else {
            return;
        };
        self.worker = None;
        match checked {
            Ok(report) => {
                let ready = report.is_ready();
                self.details = report.description();
                self.progress = match &report.settings {
                    _ if ready => "",
                    _ if !report.only_settings_broken() => {
                        "Choose the folder holding Infest.exe, PreLoad.dat and Overlay."
                    }
                    SettingsHealth::Unreadable { .. } => {
                        "settings.json cannot be read. Reset it to start from the original settings."
                    }
                    _ => "settings.json is malformed. Repair it to play.",
                }
                .to_string();
                if show_result && !self.close_requested {
                    self.notice("Infestation file check", &popup_description(&report), false);
                }
                self.report = Some(report);
            }
            Err(error) => {
                self.report = None;
                self.details = error;
                self.progress.clear();
            }
        }
    }

    fn notice(&mut self, title: &str, text: &str, then_play: bool) {
        self.notice = Some(Notice {
            title: title.to_string(),
            text: text.to_string(),
            then_play,
        });
    }

    fn change_installation(&mut self, root: PathBuf) {
        self.root = root;
        self.root_text = self.root.display().to_string();
        self.details = "Checking files...".to_string();
        self.report = None;
        self.check_files(false);
    }

    fn choose_installation(&mut self) {
        // The xdg desktop portal's folder chooser; without a portal it
        // returns nothing and the path field still takes a typed folder.
        if let Some(root) = rfd::FileDialog::new()
            .set_title("Select the Infestation folder holding Infest.exe, PreLoad.dat and Overlay.")
            .set_directory(&self.root)
            .pick_folder()
        {
            self.change_installation(root);
        }
    }

    fn fix_settings(&mut self) {
        let Some(health) = self.settings_fix().cloned() else {
            return;
        };
        let path = self.settings_path.clone();
        let (done, result) = match health {
            SettingsHealth::Unreadable { .. } => ("reset", reset(&path)),
            _ => (
                "repaired",
                FrontendAssets::open(&self.root)
                    .map_err(|error| error.to_string())
                    .and_then(|assets| repair(&path, &assets).map(drop)),
            ),
        };
        if let Err(error) = result {
            let text = format!("{} could not be {done}:\n\n{error}", path.display());
            self.notice(TITLE, &text, false);
        }
        self.check_files(false);
    }

    fn play(&mut self) {
        self.request.preferences.data_root = Some(self.root.clone());
        match self.request.preferences.save(&self.request.preferences_path) {
            Ok(()) => self.start(),
            Err(error) => self.notice(
                TITLE,
                &format!("The game can start, but the launcher could not remember this installation:\n\n{error}"),
                true,
            ),
        }
    }

    fn start(&mut self) {
        *self.selection.borrow_mut() = Some(self.root.clone());
        self.close_requested = true;
    }

    fn open_options(&mut self) {
        let settings = FrontendAssets::open(&self.root)
            .map_err(|error| error.to_string())
            .and_then(|assets| {
                FrontendSettings::load_or_defaults(&self.settings_path, &assets)
                    .map_err(|error| error.to_string())
            });
        let settings = match settings {
            Ok(settings) => settings,
            Err(error) => {
                let text = format!("The game's settings cannot be read:\n\n{error}");
                return self.notice("Infestation Options", &text, false);
            }
        };
        let modes = self.modes.clone();
        let support = *self.support.get_or_insert_with(|| {
            inf_game::vk_device::VkDevice::probe_modern_support().unwrap_or_default()
        });
        let modern = settings.modern();
        let words = settings.words();
        let position = |found: Option<usize>| found.unwrap_or(0);
        self.options = Some(Pending {
            display: settings.display_selection(),
            modes,
            renderer: position(
                RENDERERS
                    .iter()
                    .position(|&(c, _)| c == settings.renderer()),
            ),
            scaling: position(SCALINGS.iter().position(|&(c, _)| c == settings.scaling())),
            filtering: words[inf_game::frontend_video::FILTERING_CELL],
            ray_tracing: usize::from(modern.ray_tracing && support.ray_tracing),
            upscaler: modern.upscaler,
            quality: position(
                UPSCALE_QUALITIES
                    .iter()
                    .position(|&(q, _)| q == modern.quality),
            ),
            anti_aliasing: position(
                ANTI_ALIASINGS
                    .iter()
                    .position(|&(c, _)| c == modern.anti_aliasing),
            ),
            lighting: position(LIGHTINGS.iter().position(|&(l, _)| l == modern.lighting)),
            effects: words[0].min(15),
            music: words[1].min(15),
            headphones: usize::from(modern.headphones),
            ambience: u32::from(modern.ambience),
            controller: position(
                CONTROLLER_LAYOUTS
                    .iter()
                    .position(|&(l, _)| l == settings.controller_layout()),
            ),
            camera: (words[inf_game::camera_clearance::CAMERA_STYLE_CELL] as usize)
                .min(CAMERA_STYLES.len() - 1),
            fixes: fix_values(settings.fixes()),
            skip: self.request.preferences.skip_launcher,
            support,
            settings,
        });
    }

    fn save_options(&mut self) {
        let Some(pending) = self.options.as_mut() else {
            return;
        };
        let settings = &mut pending.settings;
        settings.set_desktop(pending.modes.desktop);
        settings.set_display_selection(pending.display);
        settings.set_renderer(RENDERERS[pending.renderer].0);
        settings.words_mut()[inf_game::frontend_video::FILTERING_CELL] = pending.filtering;
        settings.words_mut()[inf_game::camera_clearance::CAMERA_STYLE_CELL] = pending.camera as u32;
        settings.set_modern(ModernSettings {
            // A GPU that cannot trace keeps the saved choice for one that can.
            ray_tracing: if pending.support.ray_tracing {
                pending.ray_tracing != 0
            } else {
                settings.modern().ray_tracing
            },
            upscaler: pending.upscaler,
            quality: UPSCALE_QUALITIES[pending.quality].0,
            anti_aliasing: ANTI_ALIASINGS[pending.anti_aliasing].0,
            lighting: LIGHTINGS[pending.lighting].0,
            ultra_ground: false,
            ambience: pending
                .ambience
                .min(u32::from(inf_game::modern_settings::MOST_AMBIENCE))
                as u8,
            headphones: pending.headphones != 0,
            puddle_wheels: settings.modern().puddle_wheels,
        });
        settings.set_scaling(SCALINGS[pending.scaling].0);
        settings.set_controller_layout(CONTROLLER_LAYOUTS[pending.controller].0);
        let words = settings.words_mut();
        words[0] = pending.effects;
        words[1] = pending.music;
        settings.set_fixes(fixes_from(pending.fixes));
        if let Err(error) = settings.persist() {
            let text = format!("The game's settings could not be saved:\n\n{error}");
            return self.notice("Save Infestation options", &text, false);
        }
        self.request.preferences.skip_launcher = pending.skip;
        if let Err(error) = self
            .request
            .preferences
            .save(&self.request.preferences_path)
        {
            let text = format!(
                "The game's settings were saved, but the launcher's preference could not be:\n\n{error}"
            );
            return self.notice("Save Infestation options", &text, false);
        }
        self.options = None;
        self.progress = "Options saved.".to_string();
    }

    fn banner(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        let elapsed = self.header_tick.elapsed();
        self.header_tick = Instant::now();
        let changed = self.header.advance(elapsed.as_micros() as u64);
        let frame = self.header.frame();
        let image = || {
            let rgba: Vec<u8> = frame
                .bgra
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0], 255])
                .collect();
            egui::ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &rgba,
            )
        };
        let banner = self.banner.get_or_insert_with(|| {
            ctx.load_texture("banner", image(), egui::TextureOptions::LINEAR)
        });
        if changed {
            banner.set(image(), egui::TextureOptions::LINEAR);
        }
        banner.clone()
    }

    fn main_page(&mut self, ui: &mut egui::Ui) {
        group(ui, |ui| {
            ui.label("Installation");
            ui.horizontal(|ui| {
                let field = ui.add_enabled(
                    !self.busy(),
                    egui::TextEdit::singleline(&mut self.root_text)
                        .desired_width(ui.available_width() - 100.0),
                );
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.change_installation(PathBuf::from(self.root_text.trim()));
                }
                if ui
                    .add_enabled(!self.busy(), egui::Button::new("Change..."))
                    .clicked()
                {
                    self.choose_installation();
                }
            });
        });
        group(ui, |ui| {
            ui.label("Game files");
            egui::ScrollArea::vertical()
                .max_height(110.0)
                .auto_shrink([false, true])
                .show(ui, |ui| ui.label(&self.details));
        });
        ui.label(&self.progress);
    }

    fn main_buttons(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2((ui.available_width() - 16.0) / 3.0, 40.0);
        ui.horizontal(|ui| {
            if let Some(health) = self.settings_fix() {
                let label = match health {
                    SettingsHealth::Unreadable { .. } => "Reset settings",
                    _ => "Repair settings",
                };
                let wide = egui::vec2(size.x * 2.0 + 8.0, size.y);
                if ui
                    .add_enabled(!self.busy(), egui::Button::new(label).min_size(wide))
                    .clicked()
                {
                    self.fix_settings();
                }
            } else {
                let ready = self.can_play();
                if ui
                    .add_enabled(ready, egui::Button::new("Play").min_size(size))
                    .clicked()
                {
                    self.play();
                }
                if ui
                    .add_enabled(ready, egui::Button::new("Options...").min_size(size))
                    .clicked()
                {
                    self.open_options();
                }
            }
            if ui.add(egui::Button::new("Quit").min_size(size)).clicked() {
                self.close_requested = true;
            }
        });
    }

    fn options_page(&mut self, ui: &mut egui::Ui) {
        let progress = self.progress.clone();
        let Some(p) = self.options.as_mut() else {
            return;
        };
        egui::ScrollArea::vertical().show(ui, |ui| {
            group(ui, |ui| {
                ui.strong("Display");
                egui::Grid::new("display").num_columns(2).show(ui, |ui| {
                    let mut mode = p.display.mode.index() as usize;
                    ui.label("Display:");
                    if combo(ui, "Display", "display", &mut mode, &DISPLAY_LABELS) {
                        p.display = p.modes.select_mode(p.display, DisplayMode::from_index(mode as u32));
                    }
                    ui.end_row();
                    ui.label("Resolution:");
                    let sizes: Vec<[u32; 2]> = p.modes.menu_sizes(p.display.mode).into_iter().rev().collect();
                    let labels: Vec<String> = sizes.iter().map(|[w, h]| format!("{w} x {h}")).collect();
                    let current = p.display.resolution(p.modes.desktop);
                    let mut index = sizes
                        .iter()
                        .position(|&size| size == current)
                        .or_else(|| {
                            let resolved = p.modes.resolve(p.display.mode, current)?;
                            sizes.iter().position(|&size| size == resolved)
                        })
                        .unwrap_or(usize::MAX);
                    let enabled = p.display.mode != DisplayMode::Borderless && !sizes.is_empty();
                    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                    if ui.add_enabled_ui(enabled, |ui| combo(ui, "Resolution", "resolution", &mut index, &labels)).inner {
                        p.display.adopt(p.display.mode, sizes[index]);
                    }
                    ui.end_row();
                    ui.label("Renderer:");
                    if combo(ui, "Renderer", "renderer", &mut p.renderer, &RENDERERS.map(|(_, l)| l)) {
                        // First sight of Vulkan: SMAA and Anisotropic 8x, as on Windows.
                        if RENDERERS[p.renderer].0 == RendererChoice::Vulkan {
                            if p.filtering <= inf_game::frontend_video::FILTERING_RETAIL_MOST {
                                p.filtering = inf_game::frontend_video::FILTERING_FRESH_DEFAULT;
                            }
                            if ANTI_ALIASINGS[p.anti_aliasing].0 == AntiAliasing::Off {
                                p.anti_aliasing = ANTI_ALIASINGS
                                    .iter()
                                    .position(|&(c, _)| c == AntiAliasing::Smaa)
                                    .unwrap_or(0);
                            }
                        }
                    }
                    ui.end_row();
                    ui.label("Image scaling:");
                    combo(ui, "Image scaling", "scaling", &mut p.scaling, &SCALINGS.map(|(_, l)| l));
                    ui.end_row();
                    ui.label("Filtering:");
                    let vulkan = RENDERERS[p.renderer].0 == RendererChoice::Vulkan;
                    let most = if vulkan {
                        inf_game::frontend_video::FILTERING_VULKAN_MOST
                    } else {
                        inf_game::frontend_video::FILTERING_RETAIL_MOST
                    };
                    let labels: Vec<String> = (0..=most)
                        .map(|level| {
                            inf_game::frontend_video::filtering_text(level, 0)
                                .into_iter()
                                .map(char::from)
                                .collect()
                        })
                        .collect();
                    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                    // A Vulkan level under another renderer shows as its most and stays chosen.
                    let mut level = p.filtering.min(most) as usize;
                    if combo(ui, "Filtering", "filtering", &mut level, &labels) {
                        p.filtering = level as u32;
                    }
                    ui.end_row();
                });
                ui.weak("Display settings apply when Play starts the game. Widescreen shows more of the world at the sides.");
            });
            let vulkan = RENDERERS[p.renderer].0 == RendererChoice::Vulkan;
            if vulkan {
                group(ui, |ui| {
                    ui.strong("Vulkan");
                    ui.weak(
                        "Vanilla (OpenGL) draws Infestation as the original did. Remastered (Vulkan) \
                         adds traced light, reflections, water, fog and thermal vision, as your \
                         graphics card allows; these rows apply to it only. Ray tracing switched in \
                         game affects gameplay from the next session.",
                    );
                    egui::Grid::new("vulkan").num_columns(2).show(ui, |ui| {
                        ui.label("Ray tracing:");
                        let tracing = ["Off", "Path tracing"];
                        combo(ui, "Ray tracing", "ray_tracing", &mut p.ray_tracing, &tracing[..if p.support.ray_tracing { 2 } else { 1 }]);
                        ui.end_row();
                        ui.label("Upscaler:");
                        let traced = p.ray_tracing != 0;
                        let listed = listed_upscalers(traced, p.support);
                        let shown = shown_upscaler(traced, p.upscaler, p.support);
                        let mut index = listed.iter().position(|&(c, _)| c == shown).unwrap_or(0);
                        let labels: Vec<&str> = listed.iter().map(|&(_, l)| l).collect();
                        if combo(ui, "Upscaler", "upscaler", &mut index, &labels) {
                            p.upscaler = listed[index].0;
                        }
                        ui.end_row();
                        let upscaling = shown != Upscaler::Off;
                        // Upscaling picks the size an upscaler brings up: none without one.
                        ui.label("Upscaling:");
                        ui.add_enabled_ui(upscaling, |ui| {
                            combo(ui, "Upscaling", "quality", &mut p.quality, &UPSCALE_QUALITIES.map(|(_, l)| l))
                        });
                        ui.end_row();
                        ui.label("Anti-aliasing:");
                        ui.add_enabled_ui(!upscaling, |ui| {
                            combo(ui, "Anti-aliasing", "anti_aliasing", &mut p.anti_aliasing, &ANTI_ALIASINGS.map(|(_, l)| l))
                        });
                        ui.end_row();
                        ui.label("Lighting:");
                        combo(ui, "Lighting", "lighting", &mut p.lighting, &LIGHTINGS.map(|(_, l)| l));
                        ui.end_row();
                    });
                });
            }
            group(ui, |ui| {
                ui.strong("Sound, controller and camera");
                egui::Grid::new("sound").num_columns(2).show(ui, |ui| {
                    let label = ui.label("Effects volume:");
                    ui.add(egui::Slider::new(&mut p.effects, 0..=15)).labelled_by(label.id);
                    ui.end_row();
                    let label = ui.label("Music volume:");
                    ui.add(egui::Slider::new(&mut p.music, 0..=15)).labelled_by(label.id);
                    ui.end_row();
                    if vulkan {
                        ui.label("Headphones:");
                        combo(ui, "Headphones", "headphones", &mut p.headphones, &["Off", "On"]);
                        ui.end_row();
                        let label = ui.label("Ambience:");
                        let most = u32::from(inf_game::modern_settings::MOST_AMBIENCE);
                        ui.add(egui::Slider::new(&mut p.ambience, 0..=most)).labelled_by(label.id);
                        ui.end_row();
                    }
                    ui.label("Controller layout:");
                    combo(ui, "Controller layout", "controller", &mut p.controller, &CONTROLLER_LAYOUTS.map(|(_, l)| l));
                    ui.end_row();
                    ui.label("Camera style:");
                    combo(ui, "Camera style", "camera", &mut p.camera, &CAMERA_STYLES);
                    ui.end_row();
                });
            });
            group(ui, |ui| {
                ui.strong("Fixes for retail bugs (off restores retail)");
                for index in 0..BUG_FIX_ROWS {
                    ui.checkbox(&mut p.fixes[index], FIX_LABELS[index]);
                }
            });
            group(ui, |ui| {
                ui.strong("Cheats and extras (off is retail)");
                for index in BUG_FIX_ROWS..FIXES {
                    // Pelagia's rework needs its gate: off and dark without it.
                    if index == FIX_PELAGIA_REWORK && !p.fixes[FIX_PROTOTYPE_WORLD] {
                        p.fixes[index] = false;
                    }
                    let enabled = index != FIX_PELAGIA_REWORK || p.fixes[FIX_PROTOTYPE_WORLD];
                    ui.add_enabled(enabled, egui::Checkbox::new(&mut p.fixes[index], FIX_LABELS[index]));
                }
            });
            ui.checkbox(&mut p.skip, "Skip the launcher while this installation is ready");
            ui.weak("Start with --launcher to open it again. Missing game files still open it.");
            ui.label(&progress);
        });
    }

    fn options_buttons(&mut self, ui: &mut egui::Ui) {
        let busy = self.busy();
        let mut verify = false;
        let mut close = None;
        ui.horizontal(|ui| {
            verify = ui
                .add_enabled(!busy, egui::Button::new("Verify files"))
                .clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    close = Some(false);
                }
                if ui.add_enabled(!busy, egui::Button::new("Save")).clicked() {
                    close = Some(true);
                }
            });
        });
        if verify {
            self.check_files(true);
        }
        match close {
            Some(true) => self.save_options(),
            Some(false) => self.options = None,
            None => {}
        }
    }

    fn take_snapshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.snapshot.clone() else {
            return;
        };
        let shot = ctx.input(|input| {
            input.raw.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [width, height] = image.size;
            let rgba: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|p| p.to_srgba_unmultiplied())
                .collect();
            let saved = std::fs::File::create(&path)
                .map_err(|e| e.to_string())
                .and_then(|file| {
                    let mut encoder = png::Encoder::new(
                        std::io::BufWriter::new(file),
                        width as u32,
                        height as u32,
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
                    writer.write_image_data(&rgba).map_err(|e| e.to_string())
                });
            eprintln!("launcher snapshot {}: {saved:?}", path.display());
            self.snapshot = None;
            return;
        }
        if self.report.is_none() || self.busy() {
            return;
        }
        if self.snapshot_frames == 0
            && self.options.is_none()
            && std::env::var("INF_LAUNCHER_SNAPSHOT_PAGE").as_deref() == Ok("options")
        {
            self.open_options();
        }
        self.snapshot_frames += 1;
        if self.snapshot_frames == 30 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
    }
}

/// A titled box across the page's whole width.
fn group(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        contents(ui);
    });
}

/// One drop-down row; true when the choice changed.
fn combo(ui: &mut egui::Ui, name: &str, id: &str, selected: &mut usize, entries: &[&str]) -> bool {
    let before = *selected;
    let shown = egui::ComboBox::from_id_salt(id)
        .width(240.0)
        .selected_text(entries.get(*selected).copied().unwrap_or(""))
        .show_ui(ui, |ui| {
            for (index, entry) in entries.iter().enumerate() {
                ui.selectable_value(selected, index, *entry);
            }
        });
    // Its row's label as the accessible name, for screen readers and tests.
    let enabled = ui.is_enabled();
    shown
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, name));
    *selected != before
}

/// An empty directory has many issues; keep the notice short. The main
/// window's list keeps them all.
fn popup_description(report: &InstallationReport) -> String {
    let description = report.description();
    let lines: Vec<&str> = description.lines().collect();
    if lines.len() <= 12 {
        description
    } else {
        format!(
            "{}\n\n{} more lines are in the main window's list.",
            lines[..12].join("\n"),
            lines.len() - 12
        )
    }
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }
}

impl Launcher {
    /// One frame of the window: eframe's update, and the click tests' step.
    fn frame(&mut self, ctx: &egui::Context) {
        self.poll_worker();
        if self
            .request
            .smoke_timeout
            .is_some_and(|timeout| self.started.elapsed() >= timeout)
        {
            self.close_requested = true;
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.busy() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_requested = true;
        }
        if self.close_requested {
            if self.busy() {
                self.progress = "Finishing the file check before closing...".to_string();
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.notice.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.options.is_some() {
                self.options = None;
            } else {
                self.close_requested = true;
            }
        }
        let banner = self.banner(ctx);
        egui::TopBottomPanel::top("banner")
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let width = ui.available_width().min(banner.size_vec2().x);
                    let size = egui::vec2(width, width * banner.aspect_ratio().recip());
                    ui.add(egui::Image::new(&banner).fit_to_exact_size(size));
                });
            });
        egui::TopBottomPanel::bottom("buttons").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.add_enabled_ui(self.notice.is_none(), |ui| {
                if self.options.is_some() {
                    self.options_buttons(ui);
                } else {
                    self.main_buttons(ui);
                }
            });
            ui.add_space(6.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(self.notice.is_none(), |ui| {
                if self.options.is_some() {
                    self.options_page(ui);
                } else {
                    self.main_page(ui);
                }
            });
        });
        if let Some(notice) = &self.notice {
            let mut ok = false;
            egui::Modal::new(egui::Id::new("notice")).show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.heading(&notice.title);
                ui.label(&notice.text);
                ok = ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
            if ok {
                if self.notice.take().is_some_and(|notice| notice.then_play) {
                    self.start();
                }
            }
        }
        self.take_snapshot(ctx);
        // The banner's flipbook: one baked frame a 20 ms tick.
        ctx.request_repaint_after(Duration::from_millis(20));
    }
}
