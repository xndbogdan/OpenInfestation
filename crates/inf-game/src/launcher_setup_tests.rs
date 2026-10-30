use std::path::{Path, PathBuf};

use super::*;
use crate::frontend_settings::FrontendSettings;

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn scratch(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "inf-launcher-{name}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn messages(report: &InstallationReport, path: &Path) -> Vec<(IssueSeverity, String)> {
    report
        .issues
        .iter()
        .filter(|issue| issue.path == path)
        .map(|issue| (issue.severity, issue.message.clone()))
        .collect()
}

#[test]
fn the_retail_installation_is_ready_with_every_requested_overlay() {
    let report = validate_installation(&data_root(), &FrontendSettings::default_path());
    assert!(report.is_ready(), "{}", report.description());
    assert!(report.retail_executable);
    // Every display tier and language over the 46 scenes PreLoad.dat names:
    // of the 1932 files on disk, only these 257 carry requests and are read.
    assert_eq!(report.checked_overlays, 257);
    assert!(report.checksums_verified);
    assert!(report.description().starts_with(
        "Game files: ready. Checked 257 overlays.\nInfest.exe: the retail build.\n\
         Checksums: PreLoad.dat and all 257 overlays match a known retail release."
    ));
}

/// The embedded checksums are the reference installation's: `PreLoad.dat`
/// and every requested overlay. `INF_WRITE_RETAIL_CHECKSUMS=1` rewrites them.
#[test]
fn the_embedded_checksums_cover_the_reference_installation() {
    let root = data_root();
    let installation = Installation::open(&root).unwrap();
    let mut files = vec![ReleaseFile {
        path: "PreLoad.dat".to_string(),
        bytes: installation.preload_bytes().len() as u64,
        sha256: sha256(installation.preload_bytes()),
    }];
    for name in requested_overlays(&installation).keys() {
        let bytes = fs::read(root.join("Overlay").join(name)).unwrap();
        files.push(ReleaseFile {
            path: format!("Overlay/{name}"),
            bytes: bytes.len() as u64,
            sha256: sha256(&bytes),
        });
    }
    let lines: Vec<String> = files
        .iter()
        .map(|file| serde_json::to_string(file).unwrap())
        .collect();
    let text = format!(
        "{{\n  \"version\": 1,\n  \"source\": \"Retail Infestation installation (Infest.exe SHA-256 {RETAIL_EXECUTABLE_SHA256})\",\n  \"files\": [\n    {}\n  ]\n}}\n",
        lines.join(",\n    ")
    );
    if std::env::var_os("INF_WRITE_RETAIL_CHECKSUMS").is_some() {
        fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/retail-checksums.json"),
            &text,
        )
        .unwrap();
    }
    assert_eq!(RETAIL_RELEASES[0], text);
}

/// Copy the reference installation's executable, `PreLoad.dat` and requested
/// overlays into `root`, hard-linked where possible; `changed` is copied, so
/// writing it leaves the reference alone.
fn installation_copy(root: &Path, changed: &str) {
    let source = data_root();
    let link = |relative: &str| {
        let (from, to) = (source.join(relative), root.join(relative));
        if relative == changed || fs::hard_link(&from, &to).is_err() {
            fs::copy(&from, &to).unwrap();
        }
    };
    link("Infest.exe");
    link("PreLoad.dat");
    fs::create_dir(root.join("Overlay")).unwrap();
    let installation = Installation::open(&source).unwrap();
    for name in requested_overlays(&installation).keys() {
        link(&format!("Overlay/{name}"));
    }
}

#[test]
fn an_unknown_overlay_that_loads_only_warns() {
    let root = scratch("unknown");
    let installation = Installation::open(&data_root()).unwrap();
    let (name, (context, phase)) = requested_overlays(&installation)
        .into_iter()
        .next()
        .unwrap();
    let relative = format!("Overlay/{name}");
    installation_copy(&root, &relative);
    // Flip the last byte the loader accepts changed: the bytes no longer
    // match a known release, but the scene still loads them.
    let copy = Installation::open(&root).unwrap();
    let mut bytes = fs::read(root.join(&relative)).unwrap();
    let offset = (0..bytes.len())
        .rev()
        .find(|&offset| {
            bytes[offset] ^= 0x01;
            fs::write(root.join(&relative), &bytes).unwrap();
            let loads = copy
                .prepare_overlay(context, phase, &mut OverlayPhysicalReads::NoCallbacks)
                .is_ok();
            if !loads {
                bytes[offset] ^= 0x01;
            }
            loads
        })
        .unwrap();
    assert!(offset < bytes.len());
    let report = validate_installation(&root, &root.join("settings.json"));
    assert!(report.is_ready(), "{}", report.description());
    assert!(!report.checksums_verified);
    let [(severity, message)] = &messages(&report, &root)[..] else {
        panic!("{:?}", report.issues);
    };
    assert_eq!(*severity, IssueSeverity::Warning);
    assert!(
        message.starts_with(&format!("Checksum not recognized for 1 files: {name}.")),
        "{message}"
    );
    assert!(message.contains("the game can start"), "{message}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_overlay_that_does_not_load_stops_play() {
    let root = scratch("truncated");
    let installation = Installation::open(&data_root()).unwrap();
    let name = requested_overlays(&installation)
        .into_keys()
        .next()
        .unwrap();
    let relative = format!("Overlay/{name}");
    installation_copy(&root, &relative);
    let bytes = fs::read(root.join(&relative)).unwrap();
    fs::write(root.join(&relative), &bytes[..bytes.len() / 2]).unwrap();
    let report = validate_installation(&root, &root.join("settings.json"));
    assert!(!report.is_ready());
    assert!(!report.checksums_verified);
    let [(severity, _)] = &messages(&report, &root.join("Overlay").join(&name))[..] else {
        panic!("{:?}", report.issues);
    };
    assert_eq!(*severity, IssueSeverity::Error);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn warnings_leave_the_installation_ready_and_errors_stop_it() {
    let issue = |severity, name: &str| InstallationIssue {
        severity,
        path: PathBuf::from(name),
        message: String::new(),
    };
    let mut report = InstallationReport {
        root: PathBuf::new(),
        issues: vec![issue(IssueSeverity::Warning, "Title.avi")],
        checked_overlays: 257,
        retail_executable: true,
        checksums_verified: true,
        title_video: false,
        music_tracks: Vec::new(),
        remastered: None,
        settings: SettingsHealth::Fine,
    };
    assert!(report.is_ready());
    report.issues.push(issue(IssueSeverity::Warning, "cdaudio"));
    assert!(report.is_ready());
    report.issues.push(issue(IssueSeverity::Error, "Overlay"));
    assert!(!report.is_ready());
}

#[test]
fn an_empty_directory_names_each_missing_part() {
    let root = scratch("empty");
    let report = validate_installation(&root, &root.join("settings.json"));
    assert!(!report.is_ready());
    assert_eq!(report.checked_overlays, 0);
    assert!(!report.retail_executable && !report.title_video);
    assert!(report.music_tracks.is_empty());
    let severities: Vec<_> = report
        .issues
        .iter()
        .map(|issue| (issue.path.strip_prefix(&root).unwrap(), issue.severity))
        .collect();
    assert_eq!(
        severities,
        [
            (Path::new("Infest.exe"), IssueSeverity::Error),
            (Path::new("PreLoad.dat"), IssueSeverity::Error),
            (Path::new("Title.avi"), IssueSeverity::Warning),
            (Path::new("cdaudio"), IssueSeverity::Warning),
        ]
    );
    let description = report.description();
    assert!(description.starts_with("Game files: missing or damaged.\n"));
    assert!(description.contains(&format!(
        "{}\nMissing. The port reads the retail game's tables from it.",
        root.join("Infest.exe").display()
    )));
    assert!(description.contains(&format!(
        "{}\nMissing. It lists every scene and the overlays each loads.",
        root.join("PreLoad.dat").display()
    )));
    assert!(description.contains(&format!(
        "{}\nMissing. The title video is skipped.",
        root.join("Title.avi").display()
    )));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_overlays_are_counted_and_the_first_named() {
    let root = scratch("overlays");
    fs::copy(data_root().join("PreLoad.dat"), root.join("PreLoad.dat")).unwrap();
    let report = validate_installation(&root, &root.join("settings.json"));
    let overlay = root.join("Overlay");
    assert_eq!(
        messages(&report, &overlay),
        [(
            IssueSeverity::Error,
            "Missing. The 257 overlays the scenes load belong here.".to_string()
        )]
    );
    fs::create_dir(&overlay).unwrap();
    fs::write(overlay.join("0.ovl"), []).unwrap();
    let report = validate_installation(&root, &root.join("settings.json"));
    let [(severity, message)] = &messages(&report, &overlay)[..] else {
        panic!("{:?}", report.issues);
    };
    assert_eq!(*severity, IssueSeverity::Error);
    assert!(
        message.starts_with("256 of 257 overlays are missing: "),
        "{message}"
    );
    assert!(message.ends_with(" and 244 more."), "{message}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_settings_sidecar_the_game_cannot_read_stops_play() {
    let root = scratch("settings");
    fs::copy(data_root().join("Infest.exe"), root.join("Infest.exe")).unwrap();
    // The game's settings live beside its executable; this test's own.
    let path = root.join("settings.json");
    assert_eq!(
        FrontendSettings::default_path().file_name().unwrap(),
        "settings.json"
    );
    fs::write(&path, b"{ incomplete").unwrap();
    let report = validate_installation(&root, &path);
    assert!(report.retail_executable);
    let [(severity, message)] = &messages(&report, &path)[..] else {
        panic!("{:?}", report.issues);
    };
    assert_eq!(*severity, IssueSeverity::Error);
    assert!(message.starts_with("Cannot be read: not JSON"), "{message}");
    assert!(message.contains("Reset settings"), "{message}");
    assert!(matches!(report.settings, SettingsHealth::Unreadable { .. }));
    fs::remove_file(&path).unwrap();
    let report = validate_installation(&root, &path);
    assert!(messages(&report, &path).is_empty());
    assert_eq!(report.settings, SettingsHealth::Fine);
    fs::remove_dir_all(root).unwrap();
}

/// Reported 2026-10-28: with the game files ready, a settings
/// field from a newer build is the only thing stopping the game, and the
/// launcher names it and offers Repair settings, not the game's folder.
#[test]
fn a_repairable_settings_file_is_the_only_thing_a_ready_installation_lacks() {
    let root = scratch("repairable");
    let path = root.join("settings.json");
    let assets = FrontendAssets::open(&data_root()).unwrap();
    FrontendSettings::load_or_defaults(&path, &assets)
        .unwrap()
        .persist()
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["camera"] = serde_json::json!("remastered");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let report = validate_installation(&data_root(), &path);
    assert!(!report.is_ready());
    assert!(report.only_settings_broken());
    assert!(matches!(report.settings, SettingsHealth::Repairable { .. }));
    let [(severity, message)] = &messages(&report, &path)[..] else {
        panic!("{:?}", report.issues);
    };
    assert_eq!(*severity, IssueSeverity::Error);
    assert!(message.starts_with("Malformed: "), "{message}");
    assert!(message.contains("Removed `camera`"), "{message}");
    assert!(report
        .description()
        .starts_with("Game files: ready. Checked 257 overlays."));
    crate::frontend_settings::repair(&path, &assets).unwrap();
    assert!(validate_installation(&data_root(), &path).is_ready());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn music_needs_track_two_and_names_the_missing_tracks() {
    let root = scratch("music");
    let directory = root.join("cdaudio");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("track05.ogg"), []).unwrap();
    let report = validate_installation(&root, &root.join("settings.json"));
    assert_eq!(report.music_tracks, [5]);
    assert_eq!(
        messages(&report, &directory),
        [(
            IssueSeverity::Warning,
            "No music tracks. The game plays without music.".to_string()
        )]
    );
    fs::write(directory.join("track02.ogg"), []).unwrap();
    let report = validate_installation(&root, &root.join("settings.json"));
    assert_eq!(report.music_tracks, [2, 5]);
    assert_eq!(
        messages(&report, &directory),
        [(
            IssueSeverity::Warning,
            "Missing music: track03.ogg, track04.ogg, track06.ogg, track07.ogg, track08.ogg, track09.ogg."
                .to_string()
        )]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discovery_takes_the_first_candidate_holding_preload() {
    let root = scratch("discovery");
    let empty = root.join("empty");
    let installed = root.join("installed");
    fs::create_dir_all(&empty).unwrap();
    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("PreLoad.dat"), []).unwrap();
    assert!(!holds_retail_data(&empty));
    assert!(holds_retail_data(&installed));
    assert_eq!(
        discover_installation([empty.as_path(), installed.as_path()]),
        Some(installed.as_path())
    );
    assert_eq!(discover_installation([empty.as_path()]), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn preferences_round_trip_and_unreadable_ones_load_as_defaults() {
    let root = scratch("preferences");
    let path = root.join("nested").join("launcher.json");
    assert_eq!(
        LauncherPreferences::load(&path),
        LauncherPreferences::default()
    );
    let preferences = LauncherPreferences {
        data_root: Some(PathBuf::from("games/Infestation Ω")),
        skip_launcher: true,
    };
    preferences.save(&path).unwrap();
    assert_eq!(LauncherPreferences::load(&path), preferences);
    let changed = LauncherPreferences {
        skip_launcher: false,
        ..preferences
    };
    changed.save(&path).unwrap();
    assert_eq!(LauncherPreferences::load(&path), changed);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    fs::write(&path, b"{ incomplete").unwrap();
    assert_eq!(
        LauncherPreferences::load(&path),
        LauncherPreferences::default()
    );
    fs::remove_dir_all(root).unwrap();
}

/// The game files' summary says whether `Remastered.pak` is there: its
/// size in files, why it cannot be read, or how to build it
/// (2026-10-26). Without it the game still starts.
#[test]
fn the_summary_says_whether_the_remastered_pak_is_there() {
    let root = scratch("remastered-pak");
    let report = validate_installation(&root, &root.join("settings.json"));
    assert_eq!(report.remastered, None);
    assert!(report.description().contains(
        "Remastered.pak: not found. The remastered ambience and weather sounds stay off"
    ));
    std::fs::write(root.join("Remastered.pak"), b"not a pak").unwrap();
    let report = validate_installation(&root, &root.join("settings.json"));
    assert!(matches!(report.remastered, Some(Err(_))));
    assert!(report
        .description()
        .contains("Remastered.pak: cannot be read ("));
    let mut found = report.clone();
    found.remastered = Some(Ok(12));
    assert!(found
        .description()
        .contains("Remastered.pak: found, 12 files."));
}

/// Retail names in another case, as a case-sensitive filesystem may hold a
/// copy: the top level lower-cased, the music upper-cased as from a disc.
#[test]
fn a_copy_named_in_another_case_is_ready() {
    let root = scratch("any-case");
    installation_copy(&root, "");
    for name in ["Infest.exe", "PreLoad.dat", "Overlay"] {
        fs::rename(root.join(name), root.join(name.to_ascii_lowercase())).unwrap();
    }
    fs::write(root.join("title.avi"), b"NOT_AN_AVI").unwrap();
    fs::create_dir(root.join("CDAUDIO")).unwrap();
    for number in MUSIC_TRACKS {
        fs::write(root.join(format!("CDAUDIO/TRACK{number:02}.OGG")), []).unwrap();
    }
    assert!(holds_retail_data(&root));
    let report = validate_installation(&root, &root.join("settings.json"));
    assert!(report.is_ready(), "{}", report.description());
    assert_eq!(report.checked_overlays, 257);
    assert!(report.retail_executable && report.checksums_verified);
    assert_eq!(report.music_tracks, MUSIC_TRACKS.collect::<Vec<_>>());
    // Present: only the absent transcoded video/ warns, and not for want of
    // Title.avi.
    assert!(messages(&report, &root.join("Title.avi")).is_empty());
    assert!(messages(&report, &root.join("video"))
        .iter()
        .all(|(_, message)| !message.contains("Title.avi is absent")));
    fs::remove_dir_all(root).unwrap();
}
