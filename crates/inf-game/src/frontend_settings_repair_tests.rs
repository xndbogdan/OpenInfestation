use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

const CAMERA: usize = 0x6c / 4;
const FILTERING: usize = 0x3c / 4;

/// A settings file of this test's own, saved by the game with music at 7
/// and the Cinematic camera.
struct Saved {
    path: PathBuf,
    assets: FrontendAssets,
}

impl Saved {
    fn new() -> Self {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let assets = FrontendAssets::open(&manifest.join("../../retail"))
            .expect("retail Infest.exe is required; copy the game into retail/");
        let directory = manifest.join("../../.tmp/inf-settings-repair-tests");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!(
            "{}-{}.json",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed),
        ));
        let mut settings = FrontendSettings::load_or_defaults(&path, &assets).unwrap();
        settings.words_mut()[0] = 7;
        settings.words_mut()[CAMERA] = 1;
        settings.persist().unwrap();
        Self { path, assets }
    }

    fn edit(&self, change: impl FnOnce(&mut serde_json::Value)) {
        let mut value = serde_json::from_slice(&fs::read(&self.path).unwrap()).unwrap();
        change(&mut value);
        fs::write(&self.path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn diagnose(&self) -> SettingsHealth {
        diagnose(&self.path, &self.assets)
    }

    /// Repairs the file and loads it as the game does.
    fn repaired(&self) -> (FrontendSettings, Vec<String>) {
        let before = fs::read(&self.path).unwrap();
        let problems = repair(&self.path, &self.assets).unwrap();
        assert_eq!(fs::read(broken_copy(&self.path)).unwrap(), before);
        let settings = FrontendSettings::load_or_defaults(&self.path, &self.assets).unwrap();
        assert_eq!(self.diagnose(), SettingsHealth::Fine);
        (settings, problems)
    }

    fn startup_default(&self, index: usize) -> u32 {
        FrontendSettings::defaults(self.path.clone(), &self.assets)
            .unwrap()
            .startup_defaults[index]
    }
}

impl Drop for Saved {
    fn drop(&mut self) {
        // Only these uniquely named files are owned by the test.
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(broken_copy(&self.path));
        let _ = fs::remove_file(self.path.with_extension("json.tmp"));
    }
}

fn mentions(problems: &[String], text: &str) -> bool {
    problems.iter().any(|problem| problem.contains(text))
}

#[test]
fn a_file_the_game_loads_or_none_at_all_is_fine_and_left_alone() {
    let saved = Saved::new();
    let before = fs::read(&saved.path).unwrap();
    assert_eq!(saved.diagnose(), SettingsHealth::Fine);
    assert_eq!(fs::read(&saved.path).unwrap(), before);
    assert!(!broken_copy(&saved.path).exists());
    fs::remove_file(&saved.path).unwrap();
    assert_eq!(saved.diagnose(), SettingsHealth::Fine);
}

/// A player's file (2026-10-28): a build from the camera work wrote a
/// `camera` field and schema 12.
#[test]
fn a_newer_builds_field_and_schema_are_repaired_and_the_rest_kept() {
    let saved = Saved::new();
    saved.edit(|value| {
        value["camera"] = json!("remastered");
        value["schema"] = json!(12);
    });
    let SettingsHealth::Repairable { problems } = saved.diagnose() else {
        panic!("{:?}", saved.diagnose());
    };
    assert!(mentions(&problems, "Removed `camera`"), "{problems:?}");
    assert!(mentions(&problems, "Schema 12"), "{problems:?}");
    let (settings, repaired) = saved.repaired();
    assert_eq!(repaired, problems);
    assert_eq!(settings.words()[0], 7);
    assert_eq!(settings.words()[CAMERA], 1);
    let written: serde_json::Value =
        serde_json::from_slice(&fs::read(&saved.path).unwrap()).unwrap();
    assert_eq!(written["schema"], json!(SCHEMA_VERSION));
    assert!(written.get("camera").is_none());
}

/// Camera Style 10 is past Remastered (2) and Filtering 99 past Vulkan's
/// Anisotropic 16x: each takes its default, and the music keeps its 7.
#[test]
fn words_past_their_limits_take_their_defaults() {
    let saved = Saved::new();
    saved.edit(|value| {
        value["native_words"][CAMERA] = json!(10);
        value["native_words"][FILTERING] = json!(99);
    });
    let SettingsHealth::Repairable { problems } = saved.diagnose() else {
        panic!("{:?}", saved.diagnose());
    };
    assert!(
        mentions(&problems, " was 10; it is reset to "),
        "{problems:?}"
    );
    assert!(
        mentions(&problems, " was 99; it is reset to "),
        "{problems:?}"
    );
    let (settings, _) = saved.repaired();
    assert_eq!(settings.words()[CAMERA], saved.startup_default(CAMERA));
    assert_eq!(
        settings.words()[FILTERING],
        saved.startup_default(FILTERING)
    );
    assert_eq!(settings.words()[0], 7);
}

#[test]
fn unknown_and_unreadable_keys_inside_the_fixes_and_vulkan_settings_go() {
    let saved = Saved::new();
    saved.edit(|value| {
        value["fixes"]["vehicle_gun_mounts"] = json!(false);
        value["fixes"]["a_later_fix"] = json!(true);
        value["fixes"]["vehicle_position_carry"] = json!("sometimes");
        value["modern"]["ray_tracing"] = json!(false);
        value["modern"]["a_later_option"] = json!(1);
    });
    let SettingsHealth::Repairable { problems } = saved.diagnose() else {
        panic!("{:?}", saved.diagnose());
    };
    assert!(
        mentions(&problems, "Removed `fixes.a_later_fix`"),
        "{problems:?}"
    );
    assert!(
        mentions(&problems, "Removed `modern.a_later_option`"),
        "{problems:?}"
    );
    assert!(
        mentions(
            &problems,
            "`fixes.vehicle_position_carry` could not be read"
        ),
        "{problems:?}"
    );
    let (settings, _) = saved.repaired();
    assert!(!settings.fixes().vehicle_gun_mounts);
    assert!(settings.fixes().vehicle_position_carry);
    assert!(!settings.modern().ray_tracing);
    assert_eq!(settings.words()[CAMERA], 1);
}

#[test]
fn a_short_word_list_is_filled_and_unusable_bindings_are_the_originals() {
    let saved = Saved::new();
    saved.edit(|value| {
        value["native_words"].as_array_mut().unwrap().truncate(30);
        value["key_selections"] = json!([
            "not a key",
            {"label_resource": 9999, "action_index": 0, "native_code": 1, "key_name": "A"},
        ]);
    });
    let SettingsHealth::Repairable { problems } = saved.diagnose() else {
        panic!("{:?}", saved.diagnose());
    };
    assert!(
        mentions(&problems, "It held 30 native settings"),
        "{problems:?}"
    );
    assert!(
        mentions(&problems, "Dropped 1 of `key_selections`"),
        "{problems:?}"
    );
    assert!(
        mentions(&problems, "The key bindings could not be used"),
        "{problems:?}"
    );
    let (settings, _) = saved.repaired();
    assert_eq!(settings.words()[0], 7);
    for index in 30..SETTINGS_WORDS {
        assert_eq!(settings.words()[index], saved.startup_default(index));
    }
}

#[test]
fn a_file_with_nothing_readable_is_reset_to_the_original_settings() {
    let saved = Saved::new();
    // An object, however wrong, still repairs: each field takes its default.
    fs::write(&saved.path, b"{\"schema\": \"eleven\"}").unwrap();
    assert!(matches!(
        saved.diagnose(),
        SettingsHealth::Repairable { .. }
    ));
    fs::write(&saved.path, b"{ incomplete").unwrap();
    let SettingsHealth::Unreadable { error } = saved.diagnose() else {
        panic!("{:?}", saved.diagnose());
    };
    assert!(error.starts_with("not JSON"), "{error}");
    fs::write(&saved.path, b"[]").unwrap();
    assert!(matches!(
        saved.diagnose(),
        SettingsHealth::Unreadable { .. }
    ));
    reset(&saved.path).unwrap();
    assert!(!saved.path.exists());
    assert_eq!(fs::read(broken_copy(&saved.path)).unwrap(), b"[]");
    let settings = FrontendSettings::load_or_defaults(&saved.path, &saved.assets).unwrap();
    assert_eq!(settings.words()[0], saved.startup_default(0));
}
