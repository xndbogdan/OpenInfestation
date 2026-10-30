//! Clicks the Linux launcher's controls headlessly (egui_kittest). The
//! desktop folder chooser (the xdg portal) is the one control left out.

use super::*;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inf_game::launcher_setup::LauncherPreferences;
use std::fs;
use std::path::Path;

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inf-launcher-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A GPU that cannot trace and runs FSR alone.
const FSR_ONLY: ModernSupport = ModernSupport {
    ray_tracing: false,
    upscalers: [true, true, false, false, false],
    modern_sky: true,
};

/// The launcher on `root`, its settings and preferences in `dir`, after its
/// first file check. Tall enough that Options never scrolls.
fn launcher(dir: &Path, root: PathBuf) -> Harness<'static, Launcher> {
    let request = LauncherRequest {
        root,
        preferences: LauncherPreferences::default(),
        preferences_path: dir.join("launcher.json"),
        smoke_timeout: None,
    };
    let mut launcher = Launcher::new(
        request,
        HeaderAnimation::load().unwrap(),
        MonitorModes::default(),
        Rc::new(RefCell::new(None)),
        dir.join("settings.json"),
    );
    launcher.support = Some(FSR_ONLY);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(660.0, 2400.0))
        .build_state(|ctx, launcher: &mut Launcher| launcher.frame(ctx), launcher);
    settle(&mut harness);
    harness
}

/// Steps until the file check's worker is done.
fn settle(harness: &mut Harness<'_, Launcher>) {
    for _ in 0..6000 {
        harness.step();
        if !harness.state().busy() {
            harness.step();
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("the file check never finished");
}

fn click(harness: &mut Harness<'_, Launcher>, label: &str) {
    harness.get_by_label(label).click();
    harness.step();
    harness.step();
}

fn open_combo(harness: &mut Harness<'_, Launcher>, name: &str) {
    harness.get_by_role_and_label(Role::ComboBox, name).click();
    harness.step();
    harness.step();
}

fn saved_settings(dir: &Path) -> FrontendSettings {
    let assets = FrontendAssets::open(&data_root()).unwrap();
    FrontendSettings::load_or_defaults(dir.join("settings.json"), &assets).unwrap()
}

#[test]
fn launcher_play_returns_the_installation_and_closes() {
    let dir = scratch("play");
    let mut harness = launcher(&dir, data_root());
    assert!(harness.state().can_play(), "{}", harness.state().details);
    click(&mut harness, "Play");
    let state = harness.state();
    assert!(state.close_requested);
    assert_eq!(
        state.selection.borrow().as_deref(),
        Some(data_root().as_path())
    );
    let saved = LauncherPreferences::load(&dir.join("launcher.json"));
    assert_eq!(saved.data_root, Some(data_root()));
}

#[test]
fn launcher_quit_closes_without_a_selection() {
    let dir = scratch("quit");
    let mut harness = launcher(&dir, data_root());
    click(&mut harness, "Quit");
    assert!(harness.state().close_requested);
    assert_eq!(*harness.state().selection.borrow(), None);
}

#[test]
fn launcher_options_save_writes_the_chosen_rows() {
    let dir = scratch("save");
    let mut harness = launcher(&dir, data_root());
    click(&mut harness, "Options...");
    assert!(harness.query_by_label("Vulkan").is_none());
    open_combo(&mut harness, "Renderer");
    click(&mut harness, "Vulkan (Remastered)");
    assert!(harness.query_by_label("Vulkan").is_some());
    open_combo(&mut harness, "Upscaler");
    assert!(harness.query_by_label("AMD FSR 3.1").is_some());
    for absent in [
        "NVIDIA DLSS",
        "NVIDIA DLSS Ray Reconstruction",
        "Path tracing",
    ] {
        assert!(harness.query_by_label(absent).is_none(), "{absent}");
    }
    click(&mut harness, "AMD FSR 3.1");
    let fix = harness.state().options.as_ref().unwrap().fixes[0];
    click(&mut harness, FIX_LABELS[0]);
    let music = harness.state().options.as_ref().unwrap().music;
    harness
        .get_by_role_and_label(Role::Slider, "Music volume:")
        .focus();
    harness.step();
    harness.key_press(egui::Key::ArrowLeft);
    harness.step();
    click(
        &mut harness,
        "Skip the launcher when this installation is ready",
    );
    click(&mut harness, "Save");
    assert!(harness.state().options.is_none());
    let settings = saved_settings(&dir);
    assert_eq!(settings.renderer(), RendererChoice::Vulkan);
    assert_eq!(settings.modern().upscaler, Upscaler::Fsr);
    assert_eq!(fix_values(settings.fixes())[0], !fix);
    assert_eq!(settings.words()[1], music.saturating_sub(1));
    assert_ne!(settings.words()[1], music);
    assert!(LauncherPreferences::load(&dir.join("launcher.json")).skip_launcher);
}

#[test]
fn launcher_options_cancel_and_escape_discard_changes() {
    let dir = scratch("cancel");
    let mut harness = launcher(&dir, data_root());
    click(&mut harness, "Options...");
    click(&mut harness, FIX_LABELS[0]);
    click(&mut harness, "Cancel");
    assert!(harness.state().options.is_none());
    assert!(!dir.join("settings.json").exists());
    click(&mut harness, "Options...");
    harness.key_press(egui::Key::Escape);
    harness.step();
    assert!(harness.state().options.is_none());
    assert!(!harness.state().close_requested);
    harness.key_press(egui::Key::Escape);
    harness.step();
    assert!(harness.state().close_requested);
    assert_eq!(*harness.state().selection.borrow(), None);
}

#[test]
fn launcher_verify_files_shows_the_result() {
    let dir = scratch("verify");
    let mut harness = launcher(&dir, data_root());
    click(&mut harness, "Options...");
    click(&mut harness, "Verify files");
    settle(&mut harness);
    assert!(harness.query_by_label("Infestation file check").is_some());
    // Escape leaves Options alone while the notice is up.
    harness.key_press(egui::Key::Escape);
    harness.step();
    assert!(harness.state().options.is_some());
    click(&mut harness, "OK");
    assert!(harness.state().notice.is_none());
    assert!(harness.state().options.is_some());
}

#[test]
fn launcher_repairs_a_malformed_settings_file() {
    let dir = scratch("repair");
    let path = dir.join("settings.json");
    let assets = FrontendAssets::open(&data_root()).unwrap();
    FrontendSettings::load_or_defaults(&path, &assets)
        .unwrap()
        .persist()
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["camera"] = serde_json::json!("remastered");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let mut harness = launcher(&dir, data_root());
    assert!(harness.query_by_label("Play").is_none());
    click(&mut harness, "Repair settings");
    settle(&mut harness);
    assert!(harness.state().can_play(), "{}", harness.state().details);
    assert!(matches!(
        inf_game::frontend_settings::diagnose(&path, &assets),
        SettingsHealth::Fine
    ));
    assert!(path.exists());
}

#[test]
fn launcher_resets_an_unreadable_settings_file() {
    let dir = scratch("reset");
    fs::write(dir.join("settings.json"), b"{ incomplete").unwrap();
    let mut harness = launcher(&dir, data_root());
    click(&mut harness, "Reset settings");
    settle(&mut harness);
    assert!(harness.state().can_play(), "{}", harness.state().details);
    assert!(!dir.join("settings.json").exists());
    assert!(dir.join("settings.json.broken").exists());
}

#[test]
fn launcher_typed_folder_and_enter_checks_it() {
    let dir = scratch("typed");
    let mut harness = launcher(&dir, dir.clone());
    assert!(!harness.state().can_play());
    let field = harness.get_by_role(Role::TextInput);
    field.focus();
    harness.step();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.step();
    harness
        .get_by_role(Role::TextInput)
        .type_text(&format!(" {} ", data_root().display()));
    harness.step();
    harness.key_press(egui::Key::Enter);
    harness.step();
    assert_eq!(harness.state().root, data_root());
    settle(&mut harness);
    assert!(harness.state().can_play(), "{}", harness.state().details);
}
