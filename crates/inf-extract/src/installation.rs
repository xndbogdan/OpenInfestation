use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{Error as FormatError, Preload};
use serde::Serialize;
use thiserror::Error;

use crate::inventory::{self, InventoryError};
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstallationExportReport {
    pub preload_bytes: usize,
    pub overlay_files: usize,
    pub handler_regions: usize,
    pub covered_overlay_bytes: u64,
}

#[derive(Debug, Error)]
pub enum InstallationExportError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {source}")]
    Format { path: PathBuf, source: FormatError },

    #[error("failed to inventory the installation: {0}")]
    Inventory(#[from] InventoryError),

    #[error("failed to serialize {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error(transparent)]
    ManagedOutput(#[from] ManagedOutputError),
}

/// Return every physical OVL in deterministic path order. Semantic exporters
/// still consult PreLoad.dat before reading or decoding a file.
pub fn overlay_paths(data_root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(data_root.join("Overlay"))? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ovl"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Preserve the exact PreLoad.dat alongside its parsed scene tree and a full
/// OVL framing inventory. This is intentionally an exported data family even
/// though the viewer consumes PreLoad.dat internally instead of displaying it.
pub fn export(
    data_root: &Path,
    output_root: &Path,
) -> Result<InstallationExportReport, InstallationExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes =
        fs::read(&preload_path).map_err(|source| InstallationExportError::Read {
            path: preload_path.clone(),
            source,
        })?;
    let preload =
        Preload::parse(&preload_bytes).map_err(|source| InstallationExportError::Format {
            path: preload_path,
            source,
        })?;
    let inventory = inventory::build(data_root)?;
    let report = InstallationExportReport {
        preload_bytes: preload_bytes.len(),
        overlay_files: inventory.summary.files,
        handler_regions: inventory.summary.handler_regions,
        covered_overlay_bytes: inventory.summary.covered_bytes,
    };

    let target = output_root.join("installation");
    let staged = StagedOutput::new(&target)?;
    write_bytes(&staged.path().join("PreLoad.dat"), &preload_bytes)?;
    write_json(&staged.path().join("preload.json"), &preload)?;
    write_json(&staged.path().join("inventory.json"), &inventory)?;
    staged.commit()?;
    Ok(report)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), InstallationExportError> {
    fs::write(path, bytes).map_err(|source| InstallationExportError::Write {
        path: path.to_owned(),
        source,
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), InstallationExportError> {
    let file = fs::File::create(path).map_err(|source| InstallationExportError::Write {
        path: path.to_owned(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|source| {
        InstallationExportError::Json {
            path: path.to_owned(),
            source,
        }
    })?;
    writer
        .flush()
        .map_err(|source| InstallationExportError::Write {
            path: path.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retail_overlay_inventory_is_complete_and_sorted() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let paths = overlay_paths(&root).unwrap();
        assert_eq!(paths.len(), 1_932);
        assert!(paths.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn retail_installation_export_preserves_preload_and_is_repeatable() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let output_root = std::env::temp_dir().join(format!(
            "infestation-installation-export-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            InstallationExportReport {
                preload_bytes: 18_280,
                overlay_files: 1_932,
                handler_regions: 494,
                covered_overlay_bytes: 158_707_834,
            }
        );
        assert_eq!(
            fs::read(output_root.join("installation/PreLoad.dat")).unwrap(),
            fs::read(data_root.join("PreLoad.dat")).unwrap()
        );
        let first_inventory = fs::read(output_root.join("installation/inventory.json")).unwrap();
        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("installation/inventory.json")).unwrap(),
            first_inventory
        );
        let _ = fs::remove_dir_all(output_root);
    }
}
