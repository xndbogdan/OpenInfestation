use super::*;
use crate::name_entry::LoadedIdentity;
use crate::player_identity::{IdentityRead, PlayerIdentity};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
    assets: FrontendAssets,
}

impl Fixture {
    fn new() -> Self {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let assets = FrontendAssets::open(&manifest.join("../../retail"))
            .expect("retail Infest.exe is required; copy the game into retail/");
        let directory = manifest.join("../../.tmp/inf-settings-tests");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!(
            "{}-{}.json",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed),
        ));
        assert!(!path.exists());
        Self { path, assets }
    }

    fn load(&self) -> Result<FrontendSettings, FrontendSettingsError> {
        FrontendSettings::load_or_defaults(&self.path, &self.assets)
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&fs::read(&self.path).unwrap()).unwrap()
    }

    fn write_json(&self, value: &serde_json::Value) {
        fs::write(&self.path, serde_json::to_vec(value).unwrap()).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only these two uniquely named fixture files are owned by the test.
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.path.with_extension("json.tmp"));
    }
}

#[test]
fn fresh_settings_copy_all_startup_words_and_preserve_registry_alias_and_unmapped_word() {
    let fixture = Fixture::new();
    let source = FrontendExecutable::parse(fixture.assets.executable_bytes()).unwrap();
    assert_eq!(source.word(0x0052_e0d8).unwrap(), 0x0046_3a80);
    let settings = fixture.load().unwrap();
    assert!(
        !fixture.path.exists(),
        "loading defaults must not persist early"
    );
    assert_eq!(
        settings.words(),
        &[
            // Fresh installs filter Anisotropic 8x (+3C 5), not retail's
            // Bilinear, and draw Detail Level High (+70 1), not retail's Low;
            // the other 32 words are the authored startup words.
            7, 7, 0, 2, 1, 0, 0, 0, 1, 1, 0, 1, 10_000, 0, 1, 5, 1, 0, 0, 16, 1, 10, 6, 1, 1, 0, 0,
            0, 1, 15, 1, 1, 1, 0,
        ]
    );
    assert_eq!(settings.fx_volume(), 7);
    assert_eq!(settings.definitions().len(), 34);
    assert_eq!(
        settings
            .definitions()
            .iter()
            .filter(|row| row.byte_offset == 0x10)
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["Game type", "Game Type"]
    );
    assert!(!settings
        .definitions()
        .iter()
        .any(|row| row.byte_offset == 0x74));
    assert_eq!(settings.words()[0x74 / 4], 15);
    assert_eq!(
        settings.definitions()[0],
        NativeSettingDefinition {
            name: "SoundFX".into(),
            byte_offset: 0,
        }
    );
    assert_eq!(
        settings.definitions()[33],
        NativeSettingDefinition {
            name: "Z Buffering".into(),
            byte_offset: 0x50,
        }
    );
}

/// A fresh install opens Borderless over the desktop (2026-10-27),
/// while a saved display keeps its own mode, retail's Full Screen included.
#[test]
fn a_fresh_install_starts_borderless_and_a_saved_display_keeps_its_own() {
    let fixture = Fixture::new();
    let settings = fixture.load().unwrap();
    assert_eq!(
        settings.display_selection(),
        DisplaySelection {
            mode: DisplayMode::Borderless,
            size: [640, 480],
        }
    );
    assert_eq!(settings.words()[0x40 / 4], 1);
    assert!(
        !fixture.path.exists(),
        "loading defaults must not persist early"
    );

    let mut saved = settings;
    saved.set_display_selection(DisplaySelection {
        mode: DisplayMode::FullScreen,
        size: [640, 480],
    });
    saved.persist().unwrap();
    assert_eq!(
        fixture.load().unwrap().display_selection(),
        DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [640, 480],
        }
    );
}

/// 00463B70's offline branch copies all 34 authored words but writes the
/// saved language (+64) and cheat word (+84) back; it persists nothing.
#[test]
fn restoring_defaults_keeps_language_and_cheat_word() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    let defaults = *settings.words();
    for (index, value) in settings.words_mut().iter_mut().enumerate() {
        *value = 0x100 + index as u32;
    }
    settings.restore_startup_defaults();
    let mut expected = defaults;
    expected[0x64 / 4] = 0x100 + 0x64 / 4;
    expected[0x84 / 4] = 0x100 + 0x84 / 4;
    // 00463B70 restores the authored words: a fresh install's Anisotropic 8x
    // reverts to retail's Bilinear here, while language and cheat stay.
    expected[0x3c / 4] = 1;
    // Detail Level does not revert: High is the port's startup default (the
    // user, 2026-10-27), so Restore Defaults keeps the fine ground.
    expected[DETAIL_CELL] = DETAIL_DEFAULT;
    assert_eq!(settings.words(), &expected);
    assert_eq!(settings.words()[DETAIL_CELL], 1);
    assert!(!fixture.path.exists(), "restore must not persist");
}

/// The port's Detail Level default is High (2026-10-27); a saved
/// Low is the player's own word and stays Low across loads.
#[test]
fn a_fresh_install_starts_on_high_detail_and_a_saved_low_stays_low() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    assert_eq!(settings.words()[DETAIL_CELL], DETAIL_DEFAULT);
    assert_eq!(settings.words()[DETAIL_CELL], 1);

    settings.words_mut()[DETAIL_CELL] = 0;
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.words()[DETAIL_CELL], 0);
}

#[test]
fn fresh_gameplay_binding_snapshots_follow_both_tables_and_named_button_descriptors() {
    let fixture = Fixture::new();
    let settings = fixture.load().unwrap();
    let keys = settings.stored_keys();
    let buttons = settings.stored_buttons(&[]);
    assert_eq!(keys.len(), 44);
    assert_eq!(buttons.len(), 16);
    assert_eq!(
        keys[0],
        KeySelection {
            label_resource: 903,
            action_index: 0,
            native_code: 0xc8,
            key_name: "Up".into(),
        }
    );
    assert_eq!(
        keys[14],
        KeySelection {
            label_resource: 937,
            action_index: 41,
            native_code: 0x32,
            key_name: "M".into(),
        }
    );
    assert_eq!(
        keys[43],
        KeySelection {
            label_resource: 939,
            action_index: 43,
            native_code: 0x58,
            key_name: "F12".into(),
        }
    );
    assert_eq!(
        buttons[0],
        ButtonAction {
            button: 1,
            descriptor_address: 0x0053_a388,
            action_name: "Fire".into(),
        }
    );
    assert_eq!(
        buttons[15],
        ButtonAction {
            button: 16,
            descriptor_address: 0x0053_a430,
            action_name: "Grenade Ring".into(),
        }
    );
    let mut indices: Vec<_> = keys.iter().map(|row| row.action_index).collect();
    indices.sort_unstable();
    assert_eq!(indices, (0..44).collect::<Vec<_>>());
}

#[test]
fn sidecar_round_trip_replaces_existing_file_with_owned_audio_and_explicit_presentation() {
    let fixture = Fixture::new();
    let original = fixture.assets.executable_bytes().to_vec();
    let mut settings = fixture.load().unwrap();
    settings.persist().unwrap();
    *settings.music_mut() = 3;
    settings.select_presentation(4, 2).unwrap();
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.words(), settings.words());
    assert_eq!(loaded.words()[1], 3);
    assert_eq!(loaded.words()[3], 4);
    assert_eq!(loaded.words()[0x64 / 4], 2);
    assert_eq!(loaded.words()[0x74 / 4], 15);
    assert_eq!(loaded.stored_keys(), settings.stored_keys());
    assert_eq!(loaded.key_bindings(), settings.key_bindings());
    assert_eq!(fixture.json()["native_words"].as_array().unwrap().len(), 34);
    assert!(!fixture.path.with_extension("json.tmp").exists());
    assert_eq!(fixture.assets.executable_bytes(), original);
}

#[test]
fn invalid_existing_sidecars_do_not_fall_back_to_defaults_or_overwrite_evidence() {
    let fixture = Fixture::new();
    fs::write(&fixture.path, b"not json").unwrap();
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::Json { .. })
    ));
    assert_eq!(fs::read(&fixture.path).unwrap(), b"not json");
    fs::remove_file(&fixture.path).unwrap();
    fixture.load().unwrap().persist().unwrap();
    let valid = fixture.json();
    let mut invalid = valid.clone();
    invalid["schema"] = (SCHEMA_VERSION + 1).into();
    fixture.write_json(&invalid);
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::Schema(schema)) if schema == SCHEMA_VERSION + 1
    ));
    invalid = valid.clone();
    invalid["native_words"].as_array_mut().unwrap().pop();
    fixture.write_json(&invalid);
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::WordCount(33))
    ));
    invalid = valid.clone();
    invalid["native_words"][1] = 16.into();
    fixture.write_json(&invalid);
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::Value {
            offset: 4,
            value: 16
        })
    ));
    invalid = valid.clone();
    invalid["key_selections"][0]["label_resource"] = 1.into();
    fixture.write_json(&invalid);
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::KeyRow {
            label_resource: 1,
            action_index: 0
        })
    ));
    invalid = valid.clone();
    invalid["button_actions"][0]["button"] = 17.into();
    fixture.write_json(&invalid);
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::Button(17))
    ));
    invalid = valid;
    invalid["guessed_setting"] = 0.into();
    fixture.write_json(&invalid);
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::Json { .. })
    ));
}

#[test]
fn invalid_updates_and_staging_collisions_preserve_the_last_successful_sidecar() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    let before = *settings.words();
    assert!(matches!(
        settings.select_presentation(5, 2),
        Err(FrontendSettingsError::Value {
            offset: 0x0c,
            value: 5
        })
    ));
    assert_eq!(settings.words(), &before);
    settings.persist().unwrap();
    let saved = fs::read(&fixture.path).unwrap();
    *settings.music_mut() = 16;
    assert!(matches!(
        settings.persist(),
        Err(FrontendSettingsError::Value {
            offset: 4,
            value: 16
        })
    ));
    assert_eq!(fs::read(&fixture.path).unwrap(), saved);
    *settings.music_mut() = 1;
    let temporary = fixture.path.with_extension("json.tmp");
    fs::write(&temporary, b"prior unfinished write").unwrap();
    assert!(matches!(
        settings.persist(),
        Err(FrontendSettingsError::Io { .. })
    ));
    assert_eq!(fs::read(&fixture.path).unwrap(), saved);
    assert_eq!(fs::read(&temporary).unwrap(), b"prior unfinished write");
    assert!(matches!(
        FrontendSettings::load_or_defaults("Infest.sav", &fixture.assets),
        Err(FrontendSettingsError::Path(_))
    ));
}

/// One 00463550/00463D10 read through the process caches over the sidecar,
/// with 0055F2F4 at its boot value, 1.
fn read(
    identity: &mut PlayerIdentity,
    settings: &FrontendSettings,
    kind: IdentityRead,
) -> LoadedIdentity {
    identity
        .read(kind, &mut SidecarIdentityStore(settings), 1)
        .unwrap_or_else(|never| match never {})
}

/// The first read of a fresh process, whose caches are loader-zero.
fn first_read(settings: &FrontendSettings, kind: IdentityRead) -> LoadedIdentity {
    read(&mut PlayerIdentity::default(), settings, kind)
}

#[test]
fn missing_identity_values_use_the_frontend_created_key_defaults() {
    let fixture = Fixture::new();
    let settings = fixture.load().unwrap();
    assert_eq!(settings.stored_player_name(), None);
    assert_eq!(settings.stored_player_initials(), None);
    let identity = first_read(&settings, IdentityRead::PlayerName);
    assert_eq!(identity.name, b"Jameson");
    assert_eq!(identity.initials, b"1up");
    // Normal frontend startup creates the settings key before name entry.
    // Saving ordinary options with absent identity values keeps that policy.
    assert!(!fixture.path.exists());
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.stored_player_name(), None);
    assert_eq!(loaded.stored_player_initials(), None);
    let identity = first_read(&loaded, IdentityRead::PlayerName);
    assert_eq!(identity.name, b"Jameson");
    assert_eq!(identity.initials, b"1up");
}

#[test]
fn stored_empty_name_reads_jameson_while_empty_initials_use_the_dup_default() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.set_player_name(b"");
    settings.set_player_initials(b"");
    let identity = first_read(&settings, IdentityRead::PlayerName);
    assert_eq!(identity.name, b"Jameson");
    // sprintf("%dup", 0055F2F4), 1 from boot: `1up`.
    assert_eq!(identity.initials, b"1up");
    settings.set_player_name(b"Pilot\0trailing");
    settings.set_player_initials(b"AB\0C");
    assert_eq!(settings.stored_player_name(), Some(b"Pilot".as_slice()));
    assert_eq!(settings.stored_player_initials(), Some(b"AB".as_slice()));
    assert_eq!(
        first_read(&settings, IdentityRead::PlayerName).initials,
        b"AB"
    );
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.stored_player_name(), Some(b"Pilot".as_slice()));
    assert_eq!(loaded.stored_player_initials(), Some(b"AB".as_slice()));
    assert_eq!(first_read(&loaded, IdentityRead::PlayerName).name, b"Pilot");
    assert_eq!(fixture.json()["schema"], SCHEMA_VERSION);
    assert_eq!(
        fixture.json()["player_name"],
        serde_json::json!([80, 105, 108, 111, 116])
    );
}

#[test]
fn explicitly_saved_novice_is_kept_as_a_name_not_treated_as_missing() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.set_player_name(b"Novice");
    settings.set_player_initials(b"NV");
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    let identity = first_read(&loaded, IdentityRead::PlayerName);
    assert_eq!(identity.name, b"Novice");
    assert_eq!(identity.initials, b"NV");
}

#[test]
fn schema_1_sidecars_migrate_with_unset_identity() {
    let fixture = Fixture::new();
    fixture.load().unwrap().persist().unwrap();
    let mut legacy = fixture.json();
    legacy["schema"] = 1.into();
    legacy.as_object_mut().unwrap().remove("player_name");
    legacy.as_object_mut().unwrap().remove("player_initials");
    legacy.as_object_mut().unwrap().remove("net_player_name");
    fixture.write_json(&legacy);
    let migrated = fixture.load().unwrap();
    assert_eq!(migrated.stored_player_name(), None);
    let identity = first_read(&migrated, IdentityRead::PlayerName);
    assert_eq!(identity.name, b"Jameson");
    assert_eq!(identity.initials, b"1up");
    assert_eq!(
        first_read(&migrated, IdentityRead::NetPlayerName).name,
        b"Jameson"
    );
    migrated.persist().unwrap();
    assert_eq!(fixture.json()["schema"], SCHEMA_VERSION);
}

#[test]
fn schema_2_sidecars_migrate_keeping_identity_and_gaining_net_name() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.set_player_name(b"Pilot");
    settings.persist().unwrap();
    let mut legacy = fixture.json();
    legacy["schema"] = 2.into();
    // A genuine schema-2 file carries no net-name field at all.
    legacy.as_object_mut().unwrap().remove("net_player_name");
    fixture.write_json(&legacy);
    let migrated = fixture.load().unwrap();
    assert_eq!(migrated.stored_player_name(), Some(b"Pilot".as_slice()));
    assert_eq!(migrated.stored_net_player_name(), None);
    // Without a stored net name 00463D10 falls back to the name CACHE, which
    // only a prior 00463550 read fills; the saved player name is not queried.
    assert_eq!(
        first_read(&migrated, IdentityRead::NetPlayerName).name,
        b"Jameson"
    );
    let mut identity = PlayerIdentity::default();
    assert_eq!(
        read(&mut identity, &migrated, IdentityRead::PlayerName).name,
        b"Pilot"
    );
    identity.clear_loaded();
    assert_eq!(
        read(&mut identity, &migrated, IdentityRead::NetPlayerName).name,
        b"Pilot"
    );
}

#[test]
fn schema_3_migrates_without_selecting_an_output() {
    let fixture = Fixture::new();
    fixture.load().unwrap().persist().unwrap();
    let mut legacy = fixture.json();
    legacy["schema"] = 3.into();
    legacy.as_object_mut().unwrap().remove("current_display");
    fixture.write_json(&legacy);
    let settings = fixture.load().unwrap();
    assert_eq!(settings.current_display(), None);
    settings.persist().unwrap();
    assert_eq!(fixture.json()["schema"], SCHEMA_VERSION);
}

/// A schema-`schema` file as an older port wrote it: no display, no
/// scaling, no fixes, and optionally schema 5's output.
fn legacy_file(fixture: &Fixture, schema: u32, output: Option<serde_json::Value>) {
    let mut legacy = fixture.json();
    legacy["schema"] = schema.into();
    let fields = legacy.as_object_mut().unwrap();
    fields.remove("display");
    fields.remove("scaling");
    fields.remove("fixes");
    if let Some(output) = output {
        fields.insert("output_presentation".into(), output);
    }
    fixture.write_json(&legacy);
}

#[test]
fn schema_4_migrates_to_a_window_at_the_saved_tier() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.select_presentation(4, 2).unwrap();
    settings.words_mut()[0x40 / 4] = 0;
    settings.persist().unwrap();
    legacy_file(&fixture, 4, None);
    let migrated = fixture.load().unwrap();
    assert_eq!(
        migrated.display_selection(),
        DisplaySelection {
            mode: DisplayMode::Window,
            size: [1024, 768]
        }
    );
    // A sidecar without scaling had the old default fit: today's default.
    assert_eq!(migrated.scaling(), ClassicScaling::Widescreen);
    assert_eq!(migrated.words(), settings.words());
    migrated.persist().unwrap();
    let json = fixture.json();
    assert_eq!(json["schema"], SCHEMA_VERSION);
    assert_eq!(
        json["display"],
        serde_json::json!({"width": 1024, "height": 768, "borderless": false})
    );
    assert!(json.get("output_presentation").is_none());
}

#[test]
fn schema_5_output_folds_into_the_resolution() {
    for (resolution, saved_scaling, scaling, size, tier) in [
        ("authored", "stretched", "stretched", [640, 480], 2),
        ("720p", "integer", "integer", [1280, 720], 3),
        // The old default fit loads as today's default.
        ("1080p", "four-three", "widescreen", [1920, 1080], 4),
    ] {
        let fixture = Fixture::new();
        let mut settings = fixture.load().unwrap();
        settings.select_presentation(2, 0).unwrap();
        settings.persist().unwrap();
        legacy_file(
            &fixture,
            5,
            Some(serde_json::json!({"resolution": resolution, "scaling": saved_scaling})),
        );
        let mut migrated = fixture.load().unwrap();
        // The authored default +40 is 1: full screen at the folded size.
        assert_eq!(
            migrated.display_selection(),
            DisplaySelection {
                mode: DisplayMode::FullScreen,
                size
            },
            "{resolution}"
        );
        assert_eq!(migrated.words()[0x0c / 4], tier, "{resolution}");
        assert_eq!(
            serde_json::to_value(migrated.scaling()).unwrap(),
            scaling,
            "{resolution}"
        );
        // A partial write keeps the folded display; schema 5's output is gone.
        migrated.persist_current_display(b"Primary").unwrap();
        let json = fixture.json();
        assert_eq!(json["schema"], SCHEMA_VERSION);
        assert_eq!(json["display"]["width"], size[0], "{resolution}");
        assert_eq!(json["scaling"], scaling);
        assert!(json.get("output_presentation").is_none());
        assert_eq!(
            fixture.load().unwrap().display_selection(),
            migrated.display_selection()
        );
    }
}

#[test]
fn display_round_trip_keeps_retail_words_and_partial_write_checkpoint() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    let borderless = DisplaySelection {
        mode: DisplayMode::Borderless,
        size: [1920, 1080],
    };
    settings.set_display_selection(borderless);
    settings.set_scaling(ClassicScaling::Integer);
    // Retail words keep retail values: covering +40 and the layout's tier.
    assert_eq!(settings.words()[0x40 / 4], 1);
    assert_eq!(settings.words()[0x0c / 4], 4);
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.display_selection(), borderless);
    assert_eq!(loaded.scaling(), ClassicScaling::Integer);
    assert_eq!(loaded.words(), settings.words());
    assert_eq!(
        fixture.json()["display"],
        serde_json::json!({"width": 1920, "height": 1080, "borderless": true})
    );
    assert_eq!(fixture.json()["scaling"], "integer");

    let window = DisplaySelection {
        mode: DisplayMode::Window,
        size: [800, 600],
    };
    settings.set_display_selection(window);
    assert_eq!(settings.words()[0x40 / 4], 0);
    assert_eq!(settings.words()[0x0c / 4], 3);
    settings.persist_current_display(b"Primary").unwrap();
    settings
        .persist_identity(IdentityWriteRequest {
            target: IdentityTarget::PlayerName,
            name: b"Pilot",
            initials: IdentityInitialsWrite::Write(b"PL"),
        })
        .unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(
        loaded.display_selection(),
        borderless,
        "partial native writes keep the last persisted display"
    );

    // Restore Defaults: the authored tier's own size, Borderless cleared.
    settings.set_display_selection(borderless);
    settings.restore_startup_defaults();
    assert_eq!(
        settings.display_selection(),
        DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [640, 480]
        }
    );
}

#[test]
fn invalid_port_display_preferences_preserve_existing_sidecar() {
    let fixture = Fixture::new();
    fixture.load().unwrap().persist().unwrap();
    let valid = fixture.json();
    for (field, value) in [
        (
            "output_presentation",
            serde_json::json!({"resolution": "1440p"}),
        ),
        (
            "display",
            serde_json::json!({"width": "wide", "height": 768}),
        ),
        ("scaling", serde_json::json!("zoomed")),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        fixture.write_json(&invalid);
        let original = fs::read(&fixture.path).unwrap();
        assert!(
            matches!(fixture.load(), Err(FrontendSettingsError::Json { .. })),
            "{field}"
        );
        assert_eq!(fs::read(&fixture.path).unwrap(), original);
    }
}

/// A live Resolution edit to `tier`'s own size, as the Video row makes it.
fn select_tier(settings: &mut FrontendSettings, tier: u32) {
    let mut display = settings.display_selection();
    display.size = original_size(tier);
    settings.set_display_selection(display);
}

#[test]
fn current_display_is_saved_immediately_without_saving_other_live_edits() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    let defaults = *settings.words();
    select_tier(&mut settings, 4);
    settings.set_player_name(b"Unsaved");
    settings
        .persist_current_display(b"Second display\0ignored")
        .unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.words(), &defaults);
    assert_eq!(loaded.stored_player_name(), None);
    assert_eq!(loaded.current_display(), Some(b"Second display".as_slice()));

    // Once Back has saved other edits, the next device change preserves that
    // checkpoint rather than reverting it to defaults or saving newer edits.
    settings.persist().unwrap();
    select_tier(&mut settings, 1);
    settings.set_player_name(b"Later unsaved");
    settings.persist_current_display(b"Primary").unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.words()[3], 4);
    assert_eq!(loaded.stored_player_name(), Some(b"Unsaved".as_slice()));
    assert_eq!(loaded.current_display(), Some(b"Primary".as_slice()));
}

#[test]
fn failed_device_save_still_publishes_and_resets_but_back_keeps_the_saved_preference() {
    use crate::frontend_video::{FrontendVideoState, VideoDevice, VIDEO_DEVICE_CELL};
    use crate::menu::MenuCommand;

    for preferred in [None, Some(b"Primary".as_slice())] {
        let fixture = Fixture::new();
        let mut settings = fixture.load().unwrap();
        if let Some(name) = preferred {
            settings.persist_current_display(name).unwrap();
        } else {
            settings.persist().unwrap();
        }
        let mut video = FrontendVideoState::new(
            vec![
                VideoDevice {
                    id: 100,
                    name: b"Primary".to_vec(),
                },
                VideoDevice {
                    id: 101,
                    name: b"Second display".to_vec(),
                },
            ],
            preferred,
        )
        .unwrap();
        video.device_name_for_draw().unwrap();
        select_tier(&mut settings, 4);
        settings.set_player_name(b"Unsaved");
        let words = *settings.words();
        // An occupied writer temporary fails before replacing the sidecar. Its
        // path is owned by this fixture, not a retail or user file.
        let temporary = fixture.path.with_extension("json.tmp");
        fs::write(&temporary, b"occupied").unwrap();
        let mut saved_error = None;
        let mut reset_device = None;
        video
            .adjust_device(
                VIDEO_DEVICE_CELL,
                MenuCommand::AdjustNext,
                &words,
                |device, reset| {
                    saved_error = settings.persist_current_display(&device.name).err();
                    reset_device = Some(reset.device);
                    Ok::<_, ()>(())
                },
            )
            .unwrap()
            .unwrap();
        assert!(matches!(
            saved_error,
            Some(FrontendSettingsError::Io { .. })
        ));
        assert_eq!(reset_device, Some(101));
        assert_eq!(video.published_device(), 101);
        assert_eq!(video.selected(), 1);
        assert_eq!(settings.current_display(), preferred);
        assert_eq!(fixture.load().unwrap().current_display(), preferred);

        fs::remove_file(&temporary).unwrap();
        settings.persist().unwrap(); // Back/00463100 saves the ordinary edits.
        let loaded = fixture.load().unwrap();
        assert_eq!(loaded.current_display(), preferred);
        assert_eq!(loaded.words()[3], 4);
        assert_eq!(loaded.stored_player_name(), Some(b"Unsaved".as_slice()));
    }
}

#[test]
fn net_name_read_prefers_net_then_the_name_cache_then_jameson() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    // A fresh process has an empty name cache, so a saved player name is not
    // the net-name fallback.
    settings.set_player_name(b"Pilot");
    assert_eq!(
        first_read(&settings, IdentityRead::NetPlayerName).name,
        b"Jameson"
    );
    // After 00463550 filled the name cache and a writer cleared the flag, the
    // empty net name copies that cache.
    let mut identity = PlayerIdentity::default();
    assert_eq!(
        read(&mut identity, &settings, IdentityRead::PlayerName).name,
        b"Pilot"
    );
    identity.clear_loaded();
    assert_eq!(
        read(&mut identity, &settings, IdentityRead::NetPlayerName).name,
        b"Pilot"
    );
    settings.set_net_player_name(b"NetPilot\0trailing");
    assert_eq!(
        settings.stored_net_player_name(),
        Some(b"NetPilot".as_slice())
    );
    // Without a writer the flag keeps serving the cached copy.
    assert_eq!(
        read(&mut identity, &settings, IdentityRead::NetPlayerName).name,
        b"Pilot"
    );
    identity.clear_loaded();
    assert_eq!(
        read(&mut identity, &settings, IdentityRead::NetPlayerName).name,
        b"NetPilot"
    );
    assert_eq!(identity.net_name_cache(), b"NetPilot");
    settings.set_net_player_name(b"");
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.stored_net_player_name(), Some(b"".as_slice()));
    assert_eq!(
        first_read(&loaded, IdentityRead::NetPlayerName).name,
        b"Jameson"
    );
}

#[test]
fn phase_9_after_the_player_submenu_copies_the_empty_name_cache() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.set_player_name(b"Bob");
    settings.set_player_initials(b"BB");
    let mut identity = PlayerIdentity::default();
    // 004639D0's 00463D10 sets 0054EC59 without writing 0054E930.
    let submenu = read(&mut identity, &settings, IdentityRead::NetPlayerName);
    assert_eq!(submenu.name, b"Jameson");
    assert_eq!(identity.initials_cache(), b"BB");
    assert!(identity.loaded());
    // 00459171's 00463550 then takes the cached branch at 00463725.
    let entry = read(&mut identity, &settings, IdentityRead::PlayerName);
    assert_eq!(entry.name, b"");
    assert_eq!(entry.initials, b"BB");
}

#[test]
fn values_longer_than_the_query_cap_fail_like_error_more_data() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    // Stored with the terminator, seven initials fill the 8-byte cap.
    settings.set_player_initials(b"ABCDEFG");
    assert_eq!(
        first_read(&settings, IdentityRead::PlayerName).initials,
        b"ABCD"
    );
    settings.set_player_initials(b"ABCDEFGH");
    assert_eq!(
        first_read(&settings, IdentityRead::PlayerName).initials,
        b"1up"
    );
}

#[test]
fn identity_writes_preserve_unsaved_words_and_the_other_name() {
    use crate::name_entry::IdentityTarget;

    for target in [IdentityTarget::PlayerName, IdentityTarget::NetPlayerName] {
        let fixture = Fixture::new();
        let mut settings = fixture.load().unwrap();
        settings.set_player_name(b"Saved player");
        settings.set_net_player_name(b"Saved network");
        settings.set_player_initials(b"OLD");
        settings.persist().unwrap();
        let checkpoint = *settings.words();
        settings.words_mut()[0] = 1;
        settings.words_mut()[0x84 / 4] |= 0x800;
        settings.set_player_name(b"Unsaved player");
        settings.set_net_player_name(b"Unsaved network");
        settings
            .persist_identity(IdentityWriteRequest {
                target,
                name: b"Pilot\0ignored",
                initials: IdentityInitialsWrite::Write(b"AB\0ignored"),
            })
            .unwrap();
        let loaded = fixture.load().unwrap();
        assert_eq!(loaded.words(), &checkpoint);
        assert_eq!(settings.words()[0], 1);
        assert_ne!(settings.words()[0x84 / 4] & 0x800, 0);
        assert_eq!(loaded.stored_player_initials(), Some(b"AB".as_slice()));
        match target {
            IdentityTarget::PlayerName => {
                assert_eq!(loaded.stored_player_name(), Some(b"Pilot".as_slice()));
                assert_eq!(
                    loaded.stored_net_player_name(),
                    Some(b"Saved network".as_slice())
                );
                assert_eq!(
                    settings.stored_net_player_name(),
                    Some(b"Unsaved network".as_slice())
                );
            }
            IdentityTarget::NetPlayerName => {
                assert_eq!(loaded.stored_net_player_name(), Some(b"Pilot".as_slice()));
                assert_eq!(
                    loaded.stored_player_name(),
                    Some(b"Saved player".as_slice())
                );
                assert_eq!(
                    settings.stored_player_name(),
                    Some(b"Unsaved player".as_slice())
                );
            }
        }
    }
}

#[test]
fn failed_identity_write_does_not_change_the_remembered_pair() {
    use crate::name_entry::IdentityTarget;

    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.set_player_name(b"Saved");
    settings.set_player_initials(b"OLD");
    settings.persist().unwrap();
    let temporary = fixture.path.with_extension("json.tmp");
    fs::write(&temporary, b"occupied").unwrap();
    assert!(settings
        .persist_identity(IdentityWriteRequest {
            target: IdentityTarget::PlayerName,
            name: b"Candidate",
            initials: IdentityInitialsWrite::Write(b"NEW"),
        })
        .is_err());
    assert_eq!(settings.stored_player_name(), Some(b"Saved".as_slice()));
    assert_eq!(settings.stored_player_initials(), Some(b"OLD".as_slice()));
    fs::remove_file(temporary).unwrap();
    settings.persist().unwrap();
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.stored_player_name(), Some(b"Saved".as_slice()));
    assert_eq!(loaded.stored_player_initials(), Some(b"OLD".as_slice()));
}

#[test]
fn identity_keep_preserves_saved_and_live_initials_independently() {
    for target in [IdentityTarget::PlayerName, IdentityTarget::NetPlayerName] {
        for saved_initials in [None, Some(b"".as_slice()), Some(b"\x80OLD".as_slice())] {
            let fixture = Fixture::new();
            let mut settings = fixture.load().unwrap();
            settings.set_player_name(b"Saved player");
            settings.set_net_player_name(b"Saved network");
            if let Some(initials) = saved_initials {
                settings.set_player_initials(initials);
            }
            settings.persist().unwrap();
            let saved_words = *settings.words();
            settings.words_mut()[0] = 1;
            settings.words_mut()[0x84 / 4] |= 0x800;
            settings.set_player_name(b"Live player");
            settings.set_net_player_name(b"Live network");
            settings.set_player_initials(b"LIVE");
            let live_words = *settings.words();

            settings
                .persist_identity(IdentityWriteRequest {
                    target,
                    name: b"Replacement\0unread tail",
                    initials: IdentityInitialsWrite::Keep,
                })
                .unwrap();

            let loaded = fixture.load().unwrap();
            assert_eq!(loaded.stored_player_initials(), saved_initials);
            assert_eq!(settings.stored_player_initials(), Some(b"LIVE".as_slice()));
            assert_eq!(loaded.words(), &saved_words);
            assert_eq!(settings.words(), &live_words);
            match target {
                IdentityTarget::PlayerName => {
                    assert_eq!(loaded.stored_player_name(), Some(b"Replacement".as_slice()));
                    assert_eq!(settings.stored_player_name(), loaded.stored_player_name());
                    assert_eq!(
                        loaded.stored_net_player_name(),
                        Some(b"Saved network".as_slice())
                    );
                    assert_eq!(
                        settings.stored_net_player_name(),
                        Some(b"Live network".as_slice())
                    );
                }
                IdentityTarget::NetPlayerName => {
                    assert_eq!(
                        loaded.stored_net_player_name(),
                        Some(b"Replacement".as_slice())
                    );
                    assert_eq!(
                        settings.stored_net_player_name(),
                        loaded.stored_net_player_name()
                    );
                    assert_eq!(
                        loaded.stored_player_name(),
                        Some(b"Saved player".as_slice())
                    );
                    assert_eq!(
                        settings.stored_player_name(),
                        Some(b"Live player".as_slice())
                    );
                }
            }
        }
    }
}

#[test]
fn identity_empty_write_differs_from_keep_and_raw_bytes_use_only_the_c_prefix() {
    for target in [IdentityTarget::PlayerName, IdentityTarget::NetPlayerName] {
        let fixture = Fixture::new();
        let mut settings = fixture.load().unwrap();
        settings.set_player_initials(b"OLD");
        settings.persist().unwrap();
        settings
            .persist_identity(IdentityWriteRequest {
                target,
                name: b"Kept",
                initials: IdentityInitialsWrite::Keep,
            })
            .unwrap();
        assert_eq!(
            fixture.load().unwrap().stored_player_initials(),
            Some(b"OLD".as_slice())
        );

        settings
            .persist_identity(IdentityWriteRequest {
                target,
                name: b"Empty",
                initials: IdentityInitialsWrite::Write(b""),
            })
            .unwrap();
        assert_eq!(settings.stored_player_initials(), Some(b"".as_slice()));
        assert_eq!(
            fixture.load().unwrap().stored_player_initials(),
            Some(b"".as_slice())
        );

        let raw_initials = b"\x80\xffABCD";
        settings
            .persist_identity(IdentityWriteRequest {
                target,
                name: b"\x81Pilot\0ignored",
                initials: IdentityInitialsWrite::Write(b"\x80\xffABCD\0ignored"),
            })
            .unwrap();
        let loaded = fixture.load().unwrap();
        assert_eq!(
            settings.stored_player_initials(),
            Some(raw_initials.as_slice())
        );
        assert_eq!(
            loaded.stored_player_initials(),
            Some(raw_initials.as_slice())
        );
        let selected = match target {
            IdentityTarget::PlayerName => loaded.stored_player_name(),
            IdentityTarget::NetPlayerName => loaded.stored_net_player_name(),
        };
        assert_eq!(selected, Some(b"\x81Pilot".as_slice()));
    }
}

#[test]
fn failed_identity_keep_or_write_retains_atomic_checkpoint_and_live_identity() {
    for target in [IdentityTarget::PlayerName, IdentityTarget::NetPlayerName] {
        for initials in [
            IdentityInitialsWrite::Keep,
            IdentityInitialsWrite::Write(b"NEW"),
        ] {
            let fixture = Fixture::new();
            let mut settings = fixture.load().unwrap();
            settings.set_player_name(b"Saved player");
            settings.set_net_player_name(b"Saved network");
            settings.set_player_initials(b"OLD");
            settings.persist().unwrap();
            let saved_bytes = fs::read(&fixture.path).unwrap();
            settings.set_player_name(b"Live player");
            settings.set_net_player_name(b"Live network");
            settings.set_player_initials(b"LIVE");
            settings.words_mut()[0] = 1;
            let live_words = *settings.words();
            fs::write(fixture.path.with_extension("json.tmp"), b"occupied").unwrap();

            assert!(settings
                .persist_identity(IdentityWriteRequest {
                    target,
                    name: b"Candidate",
                    initials
                })
                .is_err());

            assert_eq!(fs::read(&fixture.path).unwrap(), saved_bytes);
            assert_eq!(settings.words(), &live_words);
            assert_eq!(
                settings.stored_player_name(),
                Some(b"Live player".as_slice())
            );
            assert_eq!(
                settings.stored_net_player_name(),
                Some(b"Live network".as_slice())
            );
            assert_eq!(settings.stored_player_initials(), Some(b"LIVE".as_slice()));
        }
    }
}

#[test]
fn settings_word_cells_resolve_only_inside_the_native_state() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    assert_eq!(
        settings.setting_word(crate::frontend_settings::SETTINGS_STATE_ADDRESS),
        Some(settings.words()[0])
    );
    assert_eq!(
        settings.setting_word(0x0054_E908),
        Some(settings.words()[0x60 / 4])
    );
    assert_eq!(
        settings.setting_word(0x0054_E92C),
        Some(settings.words()[33])
    );
    assert_eq!(settings.setting_word(0x0054_E930), None);
    assert_eq!(settings.setting_word(0x0054_E909), None);
    assert_eq!(settings.setting_word(0x0053_8CD0), None);
    let before = settings.words()[0x0C / 4];
    *settings
        .setting_word_mut(0x0054_E8B4)
        .expect("display variant word") += 1;
    assert_eq!(settings.words()[0x0C / 4], before + 1);
    assert!(settings.setting_word_mut(0x0054_EC50).is_none());
}

#[test]
fn original_settings_offsets_and_unresolved_button_names_are_validated() {
    let fixture = Fixture::new();
    let mut bytes = fixture.assets.executable_bytes().to_vec();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let offset_span = source.mapped(0x0052_6c54, 4).unwrap().file_span;
    bytes[offset_span.start..offset_span.end].copy_from_slice(&0x88u32.to_le_bytes());
    let assets = FrontendAssets::from_executable(bytes).unwrap();
    assert!(matches!(
        FrontendSettings::load_or_defaults(&fixture.path, &assets),
        Err(FrontendSettingsError::Evidence {
            address: 0x0052_6c50,
            ..
        })
    ));

    let mut bytes = fixture.assets.executable_bytes().to_vec();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let base = source.word(0x0053_4f6c).unwrap();
    let descriptor_span = source.mapped(base, 4).unwrap().file_span;
    bytes[descriptor_span.start..descriptor_span.end].copy_from_slice(&0u32.to_le_bytes());
    let assets = FrontendAssets::from_executable(bytes).unwrap();
    assert!(matches!(
        FrontendSettings::load_or_defaults(&fixture.path, &assets),
        Err(FrontendSettingsError::KeyBindings(
            crate::key_bindings::KeyBindingError::UnnamedButton(address)
        )) if address == base
    ));
}

#[test]
fn edited_keys_reload_by_first_code_name_as_00445680_does() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    let (tables, bindings) = settings.key_bindings_mut();
    let initial = tables.initial[1];
    // Action 0 (Up) takes Space; action 1 a chord of A and Left Shift; action
    // 2 an unnamed code, whose empty name first matches code 0.
    bindings.set_codes(0, [0x39, 0, 0, 0]).unwrap();
    bindings.set_codes(1, [0x1e, 0x2a, 0, 0]).unwrap();
    let unnamed = (1..0xde_u8)
        .find(|code| tables.key_name(*code) == Some(&b""[..]))
        .unwrap();
    bindings.set_codes(2, [unnamed, 0, 0, 0]).unwrap();
    settings.persist().unwrap();
    let json = fixture.json();
    let rows = json["key_selections"].as_array().unwrap();
    assert_eq!(rows[0]["key_name"], "Space");
    assert_eq!(rows[0]["native_code"], 0x39);
    let loaded = fixture.load().unwrap();
    let bindings = loaded.key_bindings();
    assert_eq!(bindings.record(0).unwrap().codes, [0x39, 0, 0, 0]);
    // Only the first code is stored: the chord's later bytes revert.
    assert_eq!(
        bindings.record(1).unwrap().codes,
        [0x1e, initial.codes[1], initial.codes[2], initial.codes[3]]
    );
    assert_eq!(bindings.record(2).unwrap().codes[0], 0);
    assert!(loaded.loaded_at_startup());
}

#[test]
fn an_unbound_button_keeps_its_previous_stored_value() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.persist().unwrap();
    let (tables, bindings) = settings.key_bindings_mut();
    let fire = tables.default_buttons[0];
    assert_eq!(tables.action_name(fire), Some(&b"Fire"[..]));
    // Button 3 takes Fire; button 1 loses it to the no-op descriptor, which
    // has no action name, so its stored value is not rewritten.
    bindings.bind_button(2, fire, 3).unwrap();
    assert_eq!(
        bindings.buttons()[0],
        crate::key_bindings::UNBOUND_BUTTON_DESCRIPTOR
    );
    assert_eq!(bindings.button_tail(), 3);
    settings.persist().unwrap();
    let json = fixture.json();
    let buttons = json["button_actions"].as_array().unwrap();
    assert_eq!(buttons.len(), 16);
    assert_eq!(buttons[0]["action_name"], "Fire");
    assert_eq!(buttons[2]["action_name"], "Fire");
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.key_bindings().buttons()[0], fire);
    assert_eq!(loaded.key_bindings().buttons()[2], fire);
}

#[test]
fn an_identity_checkpoint_writes_no_binding_values() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    let checkpoint = settings.persisted_checkpoint().unwrap();
    assert!(checkpoint.key_selections.is_empty());
    assert!(checkpoint.button_actions.is_empty());
    // Missing values keep the startup records on load.
    fixture.write_json(&serde_json::to_value(&checkpoint).unwrap());
    let loaded = fixture.load().unwrap();
    assert_eq!(loaded.key_bindings(), settings.key_bindings_mut().1);
}

#[test]
fn port_fixes_default_on_and_older_sidecars_load_with_them_on() {
    let fixture = Fixture::new();
    let settings = fixture.load().unwrap();
    assert_eq!(settings.fixes(), PortFixes::default());
    assert_eq!(
        settings.fixes().vehicle_position_step(),
        VehiclePositionStep::CarryRemainder
    );
    assert_eq!(
        settings.fixes().model_mirror_copy(),
        MirrorCopySource::MirrorSide
    );
    settings.persist().unwrap();
    assert_eq!(
        fixture.json()["fixes"],
        serde_json::json!({
            "vehicle_position_carry": true,
            "vehicle_gun_mounts": true,
            "hovercraft_fans": true,
            "exact_world_ticks": true,
            "database_models": true,
            "exact_frontend_ticks": true,
            "guided_mounts": true,
            "completion_bonus": true,
            "paced_actors": true,
            "splash_voice_cap": true,
            "mission_completion": true,
            "all_cheats": false,
            "prototype_world": true,
            "pelagia_rework": true
        })
    );
    // A sidecar saved before the gun mount fix keeps the carry it chose.
    let mut earlier = fixture.json();
    earlier["fixes"] = serde_json::json!({"vehicle_position_carry": false});
    fixture.write_json(&earlier);
    let fixes = fixture.load().unwrap().fixes();
    assert!(
        !fixes.vehicle_position_carry
            && fixes.vehicle_gun_mounts
            && fixes.hovercraft_fans
            && fixes.exact_world_ticks
            && fixes.database_models
            && fixes.guided_mounts
            && fixes.completion_bonus
            && fixes.paced_actors
            && fixes.splash_voice_cap
            && fixes.mission_completion
            && fixes.prototype_world
            && fixes.pelagia_rework
    );
    let mut legacy = fixture.json();
    legacy["schema"] = 6.into();
    legacy.as_object_mut().unwrap().remove("fixes");
    fixture.write_json(&legacy);
    assert_eq!(fixture.load().unwrap().fixes(), PortFixes::default());
}

#[test]
fn a_fix_switched_off_persists_and_selects_the_retail_step() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.set_fixes(PortFixes {
        vehicle_position_carry: false,
        vehicle_gun_mounts: false,
        hovercraft_fans: false,
        exact_world_ticks: false,
        database_models: false,
        exact_frontend_ticks: false,
        guided_mounts: false,
        completion_bonus: false,
        paced_actors: false,
        splash_voice_cap: false,
        mission_completion: false,
        all_cheats: false,
        prototype_world: false,
        pelagia_rework: false,
    });
    settings.persist().unwrap();
    let reloaded = fixture.load().unwrap();
    assert!(!reloaded.fixes().vehicle_position_carry);
    assert_eq!(
        reloaded.fixes().actor_pacing(),
        crate::session_platform::ActorPacing::Retail
    );
    assert_eq!(reloaded.fixes().splash_voice_limit(), None);
    assert_eq!(
        reloaded.fixes().vehicle_position_step(),
        VehiclePositionStep::Retail
    );
    assert_eq!(
        reloaded.fixes().model_mirror_copy(),
        MirrorCopySource::Partner
    );
    assert_eq!(
        reloaded.fixes().world_tick_quantum(),
        crate::session_platform::WorldTickQuantum::Retail
    );
    // Identity checkpoints keep it, as they keep every saved preference.
    let mut reloaded = reloaded;
    reloaded.persist_current_display(b"Primary").unwrap();
    assert_eq!(
        fixture.json()["fixes"],
        serde_json::json!({
            "vehicle_position_carry": false,
            "vehicle_gun_mounts": false,
            "hovercraft_fans": false,
            "exact_world_ticks": false,
            "database_models": false,
            "exact_frontend_ticks": false,
            "guided_mounts": false,
            "completion_bonus": false,
            "paced_actors": false,
            "splash_voice_cap": false,
            "mission_completion": false,
            "all_cheats": false,
            "prototype_world": false,
            "pelagia_rework": false
        })
    );
    let mut unknown = fixture.json();
    unknown["fixes"]["unknown_fix"] = true.into();
    fixture.write_json(&unknown);
    assert!(matches!(
        fixture.load(),
        Err(FrontendSettingsError::Json { .. })
    ));
}

#[test]
fn the_controller_layout_persists_and_older_sidecars_load_pc() {
    use crate::platform_pad::ControllerLayout;
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    assert_eq!(settings.controller_layout(), ControllerLayout::Pc);
    settings.set_controller_layout(ControllerLayout::Console);
    settings.persist().unwrap();
    assert_eq!(fixture.json()["controller_layout"], "console");
    assert_eq!(
        fixture.load().unwrap().controller_layout(),
        ControllerLayout::Console
    );
    let mut older = fixture.json();
    older["schema"] = 9.into();
    older.as_object_mut().unwrap().remove("controller_layout");
    fixture.write_json(&older);
    assert_eq!(
        fixture.load().unwrap().controller_layout(),
        ControllerLayout::Pc
    );
}

#[test]
fn the_renderer_persists_and_older_sidecars_load_auto() {
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    assert_eq!(settings.renderer(), RendererChoice::Auto);
    settings.set_renderer(RendererChoice::OpenGl);
    settings.persist().unwrap();
    assert_eq!(fixture.json()["renderer"], "opengl");
    let mut reloaded = fixture.load().unwrap();
    assert_eq!(reloaded.renderer(), RendererChoice::OpenGl);
    // Identity checkpoints keep it, as they keep every saved preference.
    reloaded.persist_current_display(b"Primary").unwrap();
    assert_eq!(fixture.json()["renderer"], "opengl");
    let mut older = fixture.json();
    older["schema"] = 8.into();
    older.as_object_mut().unwrap().remove("renderer");
    fixture.write_json(&older);
    assert_eq!(fixture.load().unwrap().renderer(), RendererChoice::Auto);
    let mut software = fixture.json();
    software["renderer"] = "software".into();
    fixture.write_json(&software);
    assert_eq!(fixture.load().unwrap().renderer(), RendererChoice::Software);
}

#[test]
fn schema_7_four_three_loads_as_widescreen_and_explicit_fits_stay() {
    for (saved, expected) in [
        ("four-three", ClassicScaling::Widescreen),
        ("stretched", ClassicScaling::Stretched),
        ("integer", ClassicScaling::Integer),
    ] {
        let fixture = Fixture::new();
        fixture.load().unwrap().persist().unwrap();
        let mut older = fixture.json();
        older["schema"] = 7.into();
        older["scaling"] = saved.into();
        fixture.write_json(&older);
        let mut loaded = fixture.load().unwrap();
        assert_eq!(loaded.scaling(), expected, "{saved}");
        // A partial write migrates the checkpoint the same way.
        loaded.persist_current_display(b"Primary").unwrap();
        assert_eq!(
            fixture.json()["scaling"],
            serde_json::to_value(expected).unwrap(),
            "{saved}"
        );
        assert_eq!(fixture.json()["schema"], SCHEMA_VERSION);
    }
    // Today's four-three is a choice and stays.
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    assert_eq!(settings.scaling(), ClassicScaling::Widescreen);
    settings.set_scaling(ClassicScaling::FourThree);
    settings.persist().unwrap();
    assert_eq!(fixture.load().unwrap().scaling(), ClassicScaling::FourThree);
}

#[test]
fn a_finished_campaign_in_the_saves_sets_the_respawns_bonus_bit() {
    let fixes = PortFixes::default();
    assert!(fixes.completion_bonus);
    // The gate's BUGGY RACING bit alone, as a saved registry holds it.
    assert_eq!(fixes.respawn_cheat_word(0x80, 0), 0x80);
    assert_eq!(fixes.respawn_cheat_word(0x80, 2), 0x80 | COMPLETION_BONUS);
    assert_eq!(fixes.respawn_cheat_word(COMPLETION_BONUS, 0), 0x40);
    let retail = PortFixes {
        completion_bonus: false,
        ..fixes
    };
    assert_eq!(retail.respawn_cheat_word(0x80, 2), 0x80);
    assert_eq!(retail.respawn_cheat_word(0x40, 0), 0x40);
}

/// The Cheats Menu option, off by default, opens the four name cheats'
/// rows on top of the saved word and leaves the other bits alone.
#[test]
fn the_all_cheats_option_opens_every_cheats_menu_row() {
    let fixes = PortFixes::default();
    assert!(!fixes.all_cheats);
    assert_eq!(fixes.menu_cheat_word(0x80), 0x80);
    let all = PortFixes {
        all_cheats: true,
        ..fixes
    };
    assert_eq!(all.menu_cheat_word(0), 0x27);
    assert_eq!(all.menu_cheat_word(0xc0), 0xe7);
    assert_eq!(
        all.respawn_cheat_word(0, 0),
        0,
        "the respawn's bonus is not a menu row"
    );
}

/// 004636AE in a network game's phase 9: the default initials take the slot
/// the transport gave (0055F2F4), so a guest in slot 2 is `2up`.
#[test]
fn the_default_initials_take_the_network_slot() {
    let fixture = Fixture::new();
    let settings = fixture.load().unwrap();
    let mut identity = PlayerIdentity::default();
    let loaded = crate::frontend_runtime::read_identity(
        &mut identity,
        &settings,
        crate::session::SessionPhase::Phase9,
        2,
    )
    .unwrap();
    assert_eq!(loaded.initials, b"2up");
}

/// Filtering (`+3C`) past Bilinear is Vulkan's: under another renderer a
/// saved Vulkan choice draws and shows as Bilinear, in the menu's language,
/// and stays saved for Vulkan.
#[test]
fn a_vulkan_filtering_choice_draws_as_bilinear_elsewhere_and_stays_saved() {
    use crate::frontend_video::{FILTERING_CELL, FILTERING_VULKAN_MOST};
    use crate::settings_row::{settings_row_content, SettingsRowContent, VALUE_ROW};
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    settings.words_mut()[FILTERING_CELL] = 6;
    settings.select_language(1);
    let row = |settings: &FrontendSettings| {
        settings_row_content(settings, VALUE_ROW, [0x40, SETTINGS_STATE_ADDRESS + 0x3c])
            .unwrap()
            .unwrap()
    };
    // OpenGL or Software: Bilinear, in French.
    assert_eq!(settings.filtering(), 1);
    assert_eq!(
        row(&settings),
        SettingsRowContent::Port(b"Filtrage".to_vec(), b"Bi-lin\xe9aire".to_vec())
    );
    assert_eq!(settings.words()[FILTERING_CELL], 6);
    // Vulkan: the saved Anisotropic 16x.
    settings.set_filtering_most(FILTERING_VULKAN_MOST);
    assert_eq!(settings.filtering(), 6);
    assert_eq!(
        row(&settings),
        SettingsRowContent::Port(b"Filtrage".to_vec(), b"Anisotrope 16x".to_vec())
    );
}

/// Camera Style (`+6C`): retail's Normal and Cinematic, then the port's
/// Remastered (2), which the row steps on to in the frontend and in play and
/// names in the menu's language; retail's own text names 0 and 1.
#[test]
fn camera_style_steps_on_to_remastered() {
    use crate::camera_clearance::{CameraStyle, CAMERA_STYLE_CELL};
    use crate::menu::MenuCommand;
    use crate::settings_row::{
        adjust_value, row_most, settings_row_content, SettingsRowContent, VALUE_ROW,
    };
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    let address = SETTINGS_STATE_ADDRESS + 0x6c;
    assert_eq!(settings.words()[CAMERA_STYLE_CELL], 0);
    assert_eq!(settings.camera_style(), CameraStyle::Original);
    // Retail's row stops at Cinematic (1); the port's goes on to 2.
    assert_eq!(row_most(address, 1), 2);
    assert_eq!(row_most(SETTINGS_STATE_ADDRESS + 0x68, 1), 1);
    for expected in [1, 2, 2] {
        let most = row_most(address, 1);
        adjust_value(
            MenuCommand::AdjustNext,
            most,
            &mut settings.words_mut()[CAMERA_STYLE_CELL],
        );
        assert_eq!(settings.words()[CAMERA_STYLE_CELL], expected);
    }
    assert_eq!(settings.camera_style(), CameraStyle::Remastered);
    settings.select_language(1);
    let row = |settings: &FrontendSettings| {
        settings_row_content(settings, VALUE_ROW, [0x51, address])
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        row(&settings),
        SettingsRowContent::Text(0x51, b"Remasteris\xe9e".to_vec())
    );
    settings.words_mut()[CAMERA_STYLE_CELL] = 1;
    assert_eq!(settings.camera_style(), CameraStyle::Original);
    assert_eq!(row(&settings), SettingsRowContent::Value(1));
    settings.persist().unwrap();
    settings.words_mut()[CAMERA_STYLE_CELL] = 2;
    settings.persist().unwrap();
    assert_eq!(
        fixture.load().unwrap().camera_style(),
        CameraStyle::Remastered
    );
}

/// The puddles' wheels follow the session's game, decided when its window
/// opened, not the renderer chosen since (a replay's `--renderer`).
#[test]
fn puddle_wheels_follow_the_game_not_the_renderer() {
    use crate::presentation::GameView;
    let fixture = Fixture::new();
    let mut settings = fixture.load().unwrap();
    assert!(settings.modern().puddle_wheels);
    settings.set_renderer(RendererChoice::Vulkan);
    assert!(!settings.puddle_wheels());
    settings.set_game_view(GameView::Remaster { traced: true });
    for renderer in [RendererChoice::OpenGl, RendererChoice::Software] {
        settings.set_renderer(renderer);
        assert!(settings.puddle_wheels());
    }
    settings.set_game_view(GameView::Remaster { traced: false });
    assert!(!settings.puddle_wheels());
}
