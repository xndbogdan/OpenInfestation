//! Install from disc image, after OpenV2K's `setup::install`: copy the files
//! the port reads off the retail disc's data track into a folder, then its
//! music ([`DiscImage::extract_music`]). Port setup; retail's own installer
//! (InstallShield, `SETUP.EXE`) is never run.
//!
//! The disc keeps everything the port reads loose in its ISO9660 volume:
//! `INFEST.EXE`, `PRELOAD.DAT`, `TITLE.AVI` and `OVERLAY/`, which install as
//! retail's installer names them (`Infest.exe`, `PreLoad.dat`, `Title.avi`,
//! `Overlay/0d0.ovl`). The rest (the installer, DirectX) is ignored.
//!
//! Nothing in the destination changes until a staged copy, each file read
//! back against the disc's SHA-256, passes the launcher's own check.
//! Files it replaces move to `infestation-backups/<time>/`; a failed move
//! puts every earlier one back.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::disc_image::{DataTrack, DiscImage, MusicExtraction, DATA_SECTOR};
use crate::launcher_setup::{validate_installation, InstallationReport};
use crate::retail_path;

/// The largest file the port reads off the disc is `Title.avi` (9 MB).
const MAX_FILE_BYTES: u32 = 128 << 20;
/// Directory bytes and entries a sane volume stays far below.
const MAX_DIRECTORY_BYTES: u32 = 8 << 20;
const MAX_ENTRIES: usize = 16_384;

/// What Install from disc image did.
#[derive(Debug)]
pub struct InstallResult {
    pub root: PathBuf,
    /// Files moved into place; the rest were already identical.
    pub files_written: usize,
    pub files_total: usize,
    /// Where replaced files went, when any were.
    pub backup: Option<PathBuf>,
    /// The music copied after the game files, or why none was.
    pub music: Result<MusicExtraction, String>,
    /// The installation as the launcher checks it afterwards.
    pub report: InstallationReport,
}

impl InstallResult {
    /// The launcher's and the command line's report.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!(
            "Installed to {}: wrote {} of {} game files{}.",
            self.root.display(),
            self.files_written,
            self.files_total,
            if self.files_written < self.files_total {
                " (the rest were already there)"
            } else {
                ""
            }
        )];
        if let Some(backup) = &self.backup {
            lines.push(format!(
                "Files it replaced are kept in {}.",
                backup.display()
            ));
        }
        lines.push(match &self.music {
            Ok(music) => music.summary(),
            Err(error) => format!("No music: {error}"),
        });
        lines.join("\n")
    }
}

/// A file of the ISO9660 volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IsoFile {
    extent: u32,
    bytes: u32,
}

/// Removes the staging folder however the install ends.
struct Stage(PathBuf);

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl DiscImage {
    /// Install the game files to `destination`, then the music, telling
    /// `progress` each step. `settings` is the game's settings file, which
    /// the check reads but install never writes.
    pub fn install(
        &self,
        destination: &Path,
        settings: &Path,
        mut progress: impl FnMut(&str),
    ) -> Result<InstallResult, String> {
        progress("Reading the disc's files...");
        let data = self.data_track()?;
        let files = install_list(&iso_files(&data)?)?;
        fs::create_dir_all(destination)
            .map_err(|error| format!("Cannot create {}: {error}", destination.display()))?;
        // As given (made absolute), for the launcher to show and remember;
        // the real path only bounds where files may go.
        let root = std::path::absolute(destination).map_err(|error| error.to_string())?;
        let real_root = fs::canonicalize(&root).map_err(|error| error.to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
            .to_string();
        let stage = Stage(root.join(format!(".infestation-stage-{nonce}")));
        fs::create_dir(&stage.0)
            .map_err(|error| format!("Cannot write in {}: {error}", root.display()))?;

        let total = files.len();
        let mut hashes = Vec::with_capacity(total);
        for (index, (name, file)) in files.iter().enumerate() {
            progress(&format!("Copying {name} ({} of {total})...", index + 1));
            let bytes = data
                .read(u64::from(file.extent), u64::from(file.bytes))
                .map_err(|error| format!("Cannot read {name} off the disc: {error}"))?;
            let staged = stage.0.join(name);
            fs::create_dir_all(staged.parent().unwrap())
                .and_then(|()| fs::write(&staged, &bytes))
                .map_err(|error| format!("Cannot write {}: {error}", staged.display()))?;
            let hash = Sha256::digest(&bytes);
            let written = fs::read(&staged).map_err(|error| error.to_string())?;
            if Sha256::digest(&written) != hash {
                return Err(format!("{name} did not copy intact"));
            }
            hashes.push(hash);
        }

        progress("Checking the copied files...");
        let staged = validate_installation(&stage.0, settings);
        if !staged.is_ready() && !staged.only_settings_broken() {
            let problems: Vec<String> = staged
                .issues
                .iter()
                .filter(|issue| issue.severity == crate::launcher_setup::IssueSeverity::Error)
                .map(|issue| issue.message.clone())
                .collect();
            return Err(format!(
                "The disc's files fail the check, so nothing was installed:\n{}",
                problems.join("\n")
            ));
        }

        progress("Moving the files into place...");
        let backup = root.join("infestation-backups").join(&nonce);
        let mut written = 0;
        let mut applied: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
        let moved = (|| -> Result<(), String> {
            for ((name, _), hash) in files.iter().zip(&hashes) {
                let target = safe_target(&root, &real_root, name)?;
                if fs::read(&target).is_ok_and(|bytes| Sha256::digest(bytes) == *hash) {
                    continue;
                }
                let kept = if target.exists() {
                    let kept = backup.join(name);
                    fs::create_dir_all(kept.parent().unwrap())
                        .and_then(|()| fs::rename(&target, &kept))
                        .map_err(|error| format!("Cannot keep {}: {error}", target.display()))?;
                    Some(kept)
                } else {
                    None
                };
                applied.push((target.clone(), kept));
                fs::create_dir_all(target.parent().unwrap())
                    .and_then(|()| fs::rename(stage.0.join(name), &target))
                    .map_err(|error| format!("Cannot install {}: {error}", target.display()))?;
                written += 1;
            }
            Ok(())
        })();
        if let Err(error) = moved {
            let mut failures = Vec::new();
            for (target, kept) in applied.into_iter().rev() {
                let _ = fs::remove_file(&target);
                if let Some(kept) = kept {
                    if let Err(failure) = fs::rename(&kept, &target) {
                        failures.push(format!("{}: {failure}", target.display()));
                    }
                }
            }
            return Err(if failures.is_empty() {
                format!("{error}; the folder is as it was")
            } else {
                format!(
                    "{error}; these could not be put back from {}: {}",
                    backup.display(),
                    failures.join("; ")
                )
            });
        }
        drop(stage);

        let music = self.extract_music(&root, |track| {
            progress(&format!("Extracting music track {track:02}..."));
        });
        progress("Checking the installation...");
        let report = validate_installation(&root, settings);
        Ok(InstallResult {
            root,
            files_written: written,
            files_total: total,
            backup: backup.exists().then_some(backup),
            music,
            report,
        })
    }
}

/// The disc files the port reads, under the names they install as.
fn install_list(files: &BTreeMap<String, IsoFile>) -> Result<Vec<(String, IsoFile)>, String> {
    let mut list = Vec::new();
    for (disc, installed, required) in [
        ("INFEST.EXE", "Infest.exe", true),
        ("PRELOAD.DAT", "PreLoad.dat", true),
        ("TITLE.AVI", "Title.avi", false),
    ] {
        match files.get(disc) {
            Some(file) => list.push((installed.to_string(), *file)),
            None if required => {
                return Err(format!(
                    "This image has no {disc}; is it the Infestation CD?"
                ))
            }
            None => {}
        }
    }
    let overlays: Vec<_> = files
        .iter()
        .filter_map(|(path, file)| {
            let name = path.strip_prefix("OVERLAY/")?;
            let stem = name.strip_suffix(".OVL")?;
            (!stem.is_empty() && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
                .then(|| (format!("Overlay/{}", name.to_ascii_lowercase()), *file))
        })
        .collect();
    if overlays.is_empty() {
        return Err("This image has no OVERLAY folder; is it the Infestation CD?".to_string());
    }
    list.extend(overlays);
    for (name, file) in &list {
        if file.bytes > MAX_FILE_BYTES {
            return Err(format!("The disc's {name} is far larger than the game's"));
        }
    }
    Ok(list)
}

/// `root`'s `relative` (in any case, as retail matches names), refusing a
/// path that a link or junction leads out of `real_root`, `root`'s real path.
fn safe_target(root: &Path, real_root: &Path, relative: &str) -> Result<PathBuf, String> {
    let target = retail_path(root, relative);
    let mut ancestor = target.parent();
    while let Some(path) = ancestor {
        if path.exists() {
            let real = fs::canonicalize(path).map_err(|error| error.to_string())?;
            if !real.starts_with(real_root) {
                return Err(format!(
                    "{} leads outside {}",
                    path.display(),
                    root.display()
                ));
            }
            break;
        }
        ancestor = path.parent();
    }
    if target.exists() && !target.is_file() {
        return Err(format!("{} is not a file", target.display()));
    }
    Ok(target)
}

/// Both-endian 32-bit field at `at`, as ISO9660 stores them.
fn both32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let field = bytes.get(at..at + 8).ok_or("truncated ISO field")?;
    let little = u32::from_le_bytes(field[..4].try_into().unwrap());
    if little != u32::from_be_bytes(field[4..].try_into().unwrap()) {
        return Err("the ISO's two copies of a number disagree".to_string());
    }
    Ok(little)
}

/// The volume's root files and `OVERLAY/` files, by upper-case path without
/// version (`OVERLAY/0D0.OVL`). Only the primary volume descriptor's tree is
/// read, bounded in depth, size and entries; other folders are not entered.
fn iso_files(data: &DataTrack) -> Result<BTreeMap<String, IsoFile>, String> {
    let bad = |what: &str| format!("Not an ISO9660 data track: {what}");
    let mut primary = None;
    for sector in 16..80 {
        let descriptor = data.read(sector, DATA_SECTOR)?;
        if &descriptor[1..6] != b"CD001" {
            return Err(bad("no volume descriptor"));
        }
        match descriptor[0] {
            1 => primary = Some(descriptor),
            255 => break,
            _ => {}
        }
    }
    let primary = primary.ok_or_else(|| bad("no primary volume descriptor"))?;
    if u16::from_le_bytes([primary[128], primary[129]]) != DATA_SECTOR as u16 {
        return Err(bad("blocks are not 2048 bytes"));
    }
    let volume = both32(&primary, 80)?;
    if u64::from(volume) > data.sectors {
        return Err("The ISO volume is longer than its data track (truncated image?)".into());
    }
    let root = &primary[156..190];
    if root[25] & 2 == 0 {
        return Err(bad("the root is not a directory"));
    }
    let mut files = BTreeMap::new();
    let mut pending = vec![(String::new(), both32(root, 2)?, both32(root, 10)?)];
    let mut visited = HashSet::new();
    let mut entries = 0;
    while let Some((prefix, extent, length)) = pending.pop() {
        if length > MAX_DIRECTORY_BYTES || !visited.insert(extent) {
            return Err(bad("a directory is too large or repeats"));
        }
        if u64::from(extent) + u64::from(length).div_ceil(DATA_SECTOR) > u64::from(volume) {
            return Err(bad("a directory lies past the volume"));
        }
        let bytes = data.read(u64::from(extent), u64::from(length))?;
        let mut at = 0;
        while at < bytes.len() {
            let size = usize::from(bytes[at]);
            if size == 0 {
                // Records never cross a sector; the rest of this one is padding.
                at = (at / DATA_SECTOR as usize + 1) * DATA_SECTOR as usize;
                continue;
            }
            let record = bytes
                .get(at..at + size)
                .filter(|_| size >= 34 && at % DATA_SECTOR as usize + size <= DATA_SECTOR as usize)
                .ok_or_else(|| bad("a directory record is cut short"))?;
            at += size;
            let name = record
                .get(33..33 + usize::from(record[32]))
                .ok_or_else(|| bad("a file name is cut short"))?;
            if name == [0] || name == [1] {
                continue;
            }
            entries += 1;
            if entries > MAX_ENTRIES {
                return Err(bad("too many files"));
            }
            let name = String::from_utf8_lossy(name);
            let name = name.split(';').next().unwrap().trim_end_matches('.');
            let path = if prefix.is_empty() {
                name.to_ascii_uppercase()
            } else {
                format!("{prefix}/{}", name.to_ascii_uppercase())
            };
            let (extent, bytes) = (both32(record, 2)?, both32(record, 10)?);
            if record[25] & 2 != 0 {
                if path == "OVERLAY" {
                    pending.push((path, extent, bytes));
                }
                continue;
            }
            // Multi-extent and interleaved files are not the port's.
            if record[25] & 0x80 != 0 || record[26] != 0 || record[27] != 0 {
                continue;
            }
            if u64::from(extent) + u64::from(bytes).div_ceil(DATA_SECTOR) > u64::from(volume) {
                return Err(format!("The disc's {path} lies past the volume"));
            }
            if Path::new(&path)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            {
                files.insert(path, IsoFile { extent, bytes });
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
#[path = "disc_install_tests.rs"]
mod tests;
