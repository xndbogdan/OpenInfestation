//! The pre-game launcher's port policy: which installation a desktop start
//! plays, whether that installation holds everything the port reads, and the
//! launcher's own preferences. Retail has no launcher: its installer's
//! directory holds the data and `Infest.exe` starts the title at once. The
//! window itself belongs to the executable (`src/launcher`).

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use inf_formats::{RequestKind, RELOCATION_PHASE_ORDER};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::frontend_assets::FrontendAssets;
use crate::frontend_settings::{diagnose, SettingsHealth};
use crate::installation::OverlayPhysicalReads;
use crate::title_playback::TitlePlayer;
use crate::{retail_path, Installation, SceneContext};

/// SHA-256 of the retail `Infest.exe` whose tables the port reads.
pub const RETAIL_EXECUTABLE_SHA256: &str =
    "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";

/// The CD player's music: `cdaudio/track02` through `track09`, each an OGG
/// or the WAV Extract music copies off the disc.
pub const MUSIC_TRACKS: RangeInclusive<u32> = 2..=9;

/// The overlay selectors a session can load: the display tiers 0..4 (the
/// files' difficulty selector) and the languages 0..5.
const OVERLAY_TIERS: RangeInclusive<u8> = 0..=4;
const OVERLAY_LANGUAGES: RangeInclusive<u8> = 0..=5;

/// Missing overlays a report names before it counts the rest.
const LISTED_MISSING_OVERLAYS: usize = 12;

/// The data files of each retail release the port knows (`PreLoad.dat` and
/// every overlay the scenes request), as written by
/// `the_embedded_checksums_cover_the_reference_installation`.
const RETAIL_RELEASES: [&str; 1] = [include_str!("retail-checksums.json")];

#[derive(Debug, Serialize, Deserialize)]
struct ReleaseManifest {
    version: u32,
    source: String,
    files: Vec<ReleaseFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReleaseFile {
    path: String,
    bytes: u64,
    sha256: String,
}

/// Lowercase path, size and SHA-256 of every data file of every known release.
fn known_fingerprints() -> &'static HashSet<(String, u64, String)> {
    static KNOWN: OnceLock<HashSet<(String, u64, String)>> = OnceLock::new();
    KNOWN.get_or_init(|| {
        RETAIL_RELEASES
            .iter()
            .flat_map(|release| {
                serde_json::from_str::<ReleaseManifest>(release)
                    .expect("the embedded retail checksums are valid")
                    .files
            })
            .map(|file| (file.path.to_ascii_lowercase(), file.bytes, file.sha256))
            .collect()
    })
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `bytes` at `relative` (as `Overlay/1.ovl`) is a known release's file.
fn known_file(relative: &str, bytes: &[u8]) -> bool {
    known_fingerprints().contains(&(
        relative.to_ascii_lowercase(),
        bytes.len() as u64,
        sha256(bytes),
    ))
}

/// Every overlay the scene tree requests over every display tier and
/// language, with one scene and phase that loads it.
fn requested_overlays(
    installation: &Installation,
) -> BTreeMap<String, (SceneContext, RequestKind)> {
    let mut names = BTreeMap::new();
    for level in 0..=u8::MAX {
        for tier in OVERLAY_TIERS {
            for language in OVERLAY_LANGUAGES {
                let context = SceneContext::new(level, tier, language);
                let Some(tables) = context.request_tables(installation.preload()) else {
                    continue;
                };
                for ((table, name), phase) in tables
                    .into_iter()
                    .zip(context.overlay_filenames())
                    .zip(RELOCATION_PHASE_ORDER)
                {
                    // A phase without requests reads no file (004EE4E0).
                    if table.has_active_handlers() {
                        names.entry(name).or_insert((context, phase));
                    }
                }
            }
        }
    }
    names
}

/// The loader's first file: a directory holding it is an installation.
pub fn holds_retail_data(dir: &Path) -> bool {
    retail_path(dir, "PreLoad.dat").is_file()
}

/// The first candidate that holds retail data.
pub fn discover_installation<'a>(
    candidates: impl IntoIterator<Item = &'a Path>,
) -> Option<&'a Path> {
    candidates.into_iter().find(|dir| holds_retail_data(dir))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueSeverity {
    /// The game cannot start from this installation.
    Error,
    /// The game starts without it: the title video or some music.
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationIssue {
    pub severity: IssueSeverity,
    pub path: PathBuf,
    pub message: String,
}

/// What a directory holds of the files the port reads.
#[derive(Clone, Debug)]
pub struct InstallationReport {
    pub root: PathBuf,
    pub issues: Vec<InstallationIssue>,
    /// The distinct overlays the scene tree requests over every display
    /// tier and language; zero when `PreLoad.dat` cannot be read.
    pub checked_overlays: usize,
    /// `Infest.exe` is the retail build ([`RETAIL_EXECUTABLE_SHA256`]).
    pub retail_executable: bool,
    /// `PreLoad.dat` and every requested overlay match a known release.
    pub checksums_verified: bool,
    /// `Title.avi` and its transcoded `video/` frames open.
    pub title_video: bool,
    /// The [`MUSIC_TRACKS`] in `cdaudio/` that the CD player can read.
    pub music_tracks: Vec<u32>,
    /// `Remastered.pak` beside the data or the executable (the port's own sounds, the final
    /// stretch's item 12): how many files it holds, why it could not be
    /// read, or none when it is not there.
    pub remastered: Option<Result<usize, String>>,
    /// Whether the game loads the saved settings, or what fixes them.
    pub settings: SettingsHealth,
}

impl InstallationReport {
    /// Nothing stops the game from starting.
    pub fn is_ready(&self) -> bool {
        self.issues
            .iter()
            .all(|issue| issue.severity == IssueSeverity::Warning)
    }

    /// The game files are ready and only the settings file stops the game:
    /// the launcher offers its repair or reset in Play's place.
    pub fn only_settings_broken(&self) -> bool {
        self.settings != SettingsHealth::Fine
            && self
                .issues
                .iter()
                .filter(|issue| issue.severity == IssueSeverity::Error)
                .count()
                == 1
    }

    /// Some music track is missing or unreadable: the launcher's Options
    /// offer Extract music.
    pub fn music_incomplete(&self) -> bool {
        self.music_tracks.len() < MUSIC_TRACKS.count()
    }

    /// The launcher's summary: one fact per line, each issue's path on its
    /// own line before the message.
    pub fn description(&self) -> String {
        let mut lines = vec![if self.is_ready() || self.only_settings_broken() {
            format!(
                "Game files: ready. Checked {} overlays.",
                self.checked_overlays
            )
        } else {
            "Game files: missing or damaged.".to_string()
        }];
        if self.retail_executable {
            lines.push("Infest.exe: the retail build.".to_string());
        }
        if self.checksums_verified {
            lines.push(format!(
                "Checksums: PreLoad.dat and all {} overlays match a known retail release.",
                self.checked_overlays
            ));
        }
        if self.title_video {
            lines.push("Title video: ready.".to_string());
        }
        let total = MUSIC_TRACKS.count();
        if self.music_tracks.len() == total {
            lines.push(format!("Music: all {total} CD audio tracks."));
        }
        lines.push(match &self.remastered {
            Some(Ok(files)) => format!("Remastered.pak: found, {files} files."),
            Some(Err(error)) => format!("Remastered.pak: cannot be read ({error})."),
            None => "Remastered.pak: not found. The remastered ambience and weather sounds stay \
                     off; build it with scripts/build-remastered-pak.py."
                .to_string(),
        });
        for issue in &self.issues {
            lines.push(issue.path.display().to_string());
            lines.push(issue.message.clone());
        }
        lines.join("\n")
    }

    fn issue(&mut self, severity: IssueSeverity, path: PathBuf, message: String) {
        self.issues.push(InstallationIssue {
            severity,
            path,
            message,
        });
    }

    fn check_executable(&mut self) -> Option<FrontendAssets> {
        let path = retail_path(&self.root, "Infest.exe");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                let message = if error.kind() == io::ErrorKind::NotFound {
                    "Missing. The port reads the retail game's tables from it.".to_string()
                } else {
                    format!("Cannot read it: {error}")
                };
                self.issue(IssueSeverity::Error, path, message);
                return None;
            }
        };
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.retail_executable = digest == RETAIL_EXECUTABLE_SHA256;
        if !self.retail_executable {
            self.issue(
                IssueSeverity::Warning,
                path.clone(),
                format!(
                    "Not the retail Infest.exe the port reads (SHA-256 {digest}); its tables may differ."
                ),
            );
        }
        match FrontendAssets::from_executable(bytes) {
            Ok(assets) => Some(assets),
            Err(error) => {
                self.issue(
                    IssueSeverity::Error,
                    path,
                    format!("Cannot read its tables: {error}"),
                );
                None
            }
        }
    }

    fn check_overlays(&mut self) {
        let path = retail_path(&self.root, "PreLoad.dat");
        if !path.is_file() {
            let message = "Missing. It lists every scene and the overlays each loads.".to_string();
            self.issue(IssueSeverity::Error, path, message);
            return;
        }
        let installation = match Installation::open(&self.root) {
            Ok(installation) => installation,
            Err(error) => {
                self.issue(IssueSeverity::Error, path, error.to_string());
                return;
            }
        };
        let mut unknown = Vec::new();
        if !known_file("PreLoad.dat", installation.preload_bytes()) {
            unknown.push("PreLoad.dat".to_string());
        }
        let names = requested_overlays(&installation);
        self.checked_overlays = names.len();
        let directory = retail_path(&self.root, "Overlay");
        if !directory.is_dir() {
            let message = format!(
                "Missing. The {} overlays the scenes load belong here.",
                names.len()
            );
            self.issue(IssueSeverity::Error, directory, message);
            return;
        }
        let missing: Vec<&str> = names
            .keys()
            .filter(|name| !retail_path(&directory, name).is_file())
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            let listed = missing[..missing.len().min(LISTED_MISSING_OVERLAYS)].join(", ");
            let more = match missing.len().saturating_sub(LISTED_MISSING_OVERLAYS) {
                0 => String::new(),
                rest => format!(" and {rest} more"),
            };
            let message = format!(
                "{} of {} overlays are missing: {listed}{more}.",
                missing.len(),
                names.len()
            );
            self.issue(IssueSeverity::Error, directory.clone(), message);
        }
        // Each present overlay loads as a scene would: its layout, then each
        // handler's resources. A failure stops play; bytes no known release
        // has only warn, since other builds may load fine.
        let mut damaged = 0;
        for (name, &(context, phase)) in &names {
            if missing.contains(&name.as_str()) {
                continue;
            }
            let mut reads = OverlayPhysicalReads::NoCallbacks;
            match installation.prepare_overlay(context, phase, &mut reads) {
                Ok(Some(overlay)) => {
                    if !known_file(&format!("Overlay/{name}"), &overlay.bytes) {
                        unknown.push(name.clone());
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    damaged += 1;
                    if damaged <= LISTED_MISSING_OVERLAYS {
                        self.issue(
                            IssueSeverity::Error,
                            crate::retail_path(&directory, name),
                            error.to_string(),
                        );
                    }
                }
            }
        }
        if damaged > LISTED_MISSING_OVERLAYS {
            let message = format!(
                "{} more overlays cannot be loaded.",
                damaged - LISTED_MISSING_OVERLAYS
            );
            self.issue(IssueSeverity::Error, directory, message);
        }
        self.checksums_verified = missing.is_empty() && damaged == 0 && unknown.is_empty();
        if !unknown.is_empty() {
            let listed = unknown[..unknown.len().min(LISTED_MISSING_OVERLAYS)].join(", ");
            let more = match unknown.len().saturating_sub(LISTED_MISSING_OVERLAYS) {
                0 => String::new(),
                rest => format!(" and {rest} more"),
            };
            let message = format!(
                "Checksum not recognized for {} files: {listed}{more}. They are not from a retail release this port knows, or were changed. Structural checks passed, so the game can start; expect unexpected behaviour.",
                unknown.len()
            );
            self.issue(IssueSeverity::Warning, self.root.clone(), message);
        }
    }

    /// The game loads its settings at startup and stops on one it cannot read.
    fn check_settings(&mut self, assets: &FrontendAssets, path: &Path) {
        self.settings = diagnose(path, assets);
        let message = match &self.settings {
            SettingsHealth::Fine => return,
            SettingsHealth::Repairable { problems } => format!(
                "Malformed: the game cannot start with it. Repair settings keeps the old file as \
                 settings.json.broken and: {}",
                problems.join(" ")
            ),
            SettingsHealth::Unreadable { error } => format!(
                "Cannot be read: {error}. Reset settings sets it aside as settings.json.broken, \
                 and the game starts from the original settings."
            ),
        };
        self.issue(IssueSeverity::Error, path.to_path_buf(), message);
    }

    fn check_title_video(&mut self) {
        let avi = retail_path(&self.root, "Title.avi");
        if !avi.is_file() {
            let message = "Missing. The title video is skipped.".to_string();
            self.issue(IssueSeverity::Warning, avi, message);
            return;
        }
        match TitlePlayer::open(&self.root) {
            Ok(_) => self.title_video = true,
            Err(error) => {
                let message = format!(
                    "{error}. The title video is skipped; scripts/ensure-title-video.ps1 transcodes Title.avi into video/."
                );
                self.issue(IssueSeverity::Warning, self.root.join("video"), message);
            }
        }
    }

    fn check_music(&mut self) {
        let directory = retail_path(&self.root, "cdaudio");
        let mut unreadable = Vec::new();
        for number in MUSIC_TRACKS {
            match crate::platform_audio::cd_music::music_track(&directory, number) {
                Ok(Some(_)) => self.music_tracks.push(number),
                Ok(None) => {}
                Err(error) => unreadable.push(error),
            }
        }
        let extract = " Options' Extract music copies it from the disc image.";
        // Without track02 the CD player finds no media and stays silent.
        let mut message = if !self.music_tracks.contains(MUSIC_TRACKS.start()) {
            format!("No music tracks. The game plays without music.{extract}")
        } else if self.music_incomplete() {
            let missing: Vec<String> = MUSIC_TRACKS
                .filter(|number| !self.music_tracks.contains(number))
                .map(|number| format!("track{number:02}"))
                .collect();
            format!("Missing music: {}.{extract}", missing.join(", "))
        } else {
            return;
        };
        for error in unreadable {
            message.push_str(&format!("\nUnreadable: {error}."));
        }
        self.issue(IssueSeverity::Warning, directory, message);
    }
}

/// Check `root` for the files the port reads: `Infest.exe`'s tables,
/// `PreLoad.dat` and every overlay its scenes request, the title video and
/// the music; and the saved settings at `settings` (the game's own,
/// [`crate::frontend_settings::FrontendSettings::default_path`]).
pub fn validate_installation(root: &Path, settings: &Path) -> InstallationReport {
    let mut report = InstallationReport {
        root: root.to_path_buf(),
        issues: Vec::new(),
        checked_overlays: 0,
        retail_executable: false,
        checksums_verified: false,
        title_video: false,
        music_tracks: Vec::new(),
        remastered: None,
        settings: SettingsHealth::Fine,
    };
    let assets = report.check_executable();
    report.check_overlays();
    if let Some(assets) = &assets {
        report.check_settings(assets, settings);
    }
    report.check_title_video();
    report.check_music();
    let pak = crate::platform_audio::remastered_pak::locate(root);
    if pak.is_file() {
        report.remastered = Some(
            crate::platform_audio::remastered_pak::RemasteredPak::open(&pak)
                .map(|pak| pak.len())
                .map_err(|error| error.to_string()),
        );
    }
    report
}

/// The launcher's own choices, apart from the game's settings sidecar.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherPreferences {
    /// The installation Play last started.
    pub data_root: Option<PathBuf>,
    /// Start the game without the launcher while that installation is ready.
    pub skip_launcher: bool,
}

impl LauncherPreferences {
    /// The saved preferences; a missing or unreadable file gives defaults.
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Replace the file whole: write a new one beside it, then rename it over
    /// the old, so a failure keeps the previous preferences.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

/// `%LOCALAPPDATA%\Infestation Port\launcher.json`, or the XDG or home
/// equivalent, else beside the executable.
pub fn default_preferences_path() -> PathBuf {
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(root)
            .join("Infestation Port")
            .join("launcher.json");
    }
    if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(root)
            .join("infestation-port")
            .join("launcher.json");
    }
    if let Some(root) = std::env::var_os("HOME") {
        return PathBuf::from(root).join(".config/infestation-port/launcher.json");
    }
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("launcher.json")
}

#[cfg(test)]
#[path = "launcher_setup_tests.rs"]
mod tests;
