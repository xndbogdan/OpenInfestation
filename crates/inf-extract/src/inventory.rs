use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{parse_overlay, HandlerFraming, HandlerRegion, OverlayKey, Preload};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {source}")]
    Format {
        path: PathBuf,
        source: inf_formats::Error,
    },

    #[error("PreLoad.dat contains duplicate leaf id {0}")]
    DuplicateLeaf(u8),

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("empty overlay {path} has active handler requests")]
    EmptyOverlayHasRequests { path: PathBuf },

    #[error("non-empty overlay {path} has no active handler requests")]
    NonEmptyOverlayHasNoRequests { path: PathBuf },

    #[error("failed to serialize inventory: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
pub struct InventoryReport {
    pub summary: InventorySummary,
    pub files: Vec<InventoryFile>,
}

#[derive(Debug, Serialize)]
pub struct InventorySummary {
    pub preload_bytes: usize,
    pub preload_leaves: usize,
    pub files: usize,
    pub non_empty_files: usize,
    pub empty_files: usize,
    pub input_bytes: u64,
    pub covered_bytes: u64,
    pub handler_regions: usize,
    pub framing: FramingSummary,
}

#[derive(Debug, Default, Serialize)]
pub struct FramingSummary {
    pub standard_regions: usize,
    pub atlas_regions: usize,
    pub atlas_records: usize,
    pub model_regions: usize,
    pub model_sub_blocks: usize,
    pub model_entries: u64,
    pub relocation_regions: usize,
    pub relocation_records: u64,
}

#[derive(Debug, Serialize)]
pub struct InventoryFile {
    pub name: String,
    pub key: OverlayKey,
    pub size: usize,
    pub regions: Vec<HandlerRegion>,
}

pub fn build(data_root: &Path) -> Result<InventoryReport, InventoryError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| InventoryError::Format {
        path: preload_path.clone(),
        source,
    })?;

    let mut leaf_ids = HashSet::new();
    for leaf in preload.leaves() {
        if !leaf_ids.insert(leaf.overlay_id) {
            return Err(InventoryError::DuplicateLeaf(leaf.overlay_id));
        }
    }

    let overlay_dir = data_root.join("Overlay");
    let mut paths = fs::read_dir(&overlay_dir)
        .map_err(|source| InventoryError::Io {
            path: overlay_dir.clone(),
            source,
        })?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|source| InventoryError::Io {
                    path: overlay_dir.clone(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ovl"))
    });
    paths.sort();

    let mut files = Vec::with_capacity(paths.len());
    let mut non_empty_files = 0;
    let mut input_bytes = 0u64;
    let mut covered_bytes = 0u64;
    let mut handler_regions = 0usize;
    let mut framing = FramingSummary::default();

    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| InventoryError::Format {
                path: path.clone(),
                source: inf_formats::Error::InvalidOverlayFilename {
                    name: path.display().to_string(),
                },
            })?;
        let key = OverlayKey::parse_filename(name).map_err(|source| InventoryError::Format {
            path: path.clone(),
            source,
        })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| InventoryError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        let bytes = read(&path)?;
        input_bytes += bytes.len() as u64;

        let regions = if bytes.is_empty() {
            if requests.has_active_handlers() {
                return Err(InventoryError::EmptyOverlayHasRequests { path: path.clone() });
            }
            Vec::new()
        } else {
            if !requests.has_active_handlers() {
                return Err(InventoryError::NonEmptyOverlayHasNoRequests { path: path.clone() });
            }
            non_empty_files += 1;
            let layout =
                parse_overlay(&bytes, requests).map_err(|source| InventoryError::Format {
                    path: path.clone(),
                    source,
                })?;
            covered_bytes += layout.covered_bytes() as u64;
            handler_regions += layout.regions.len();
            for region in &layout.regions {
                match &region.framing {
                    HandlerFraming::Standard { .. } => framing.standard_regions += 1,
                    HandlerFraming::SpriteAtlas { records, .. } => {
                        framing.atlas_regions += 1;
                        framing.atlas_records += records.len();
                    }
                    HandlerFraming::ModelSubBlocks {
                        total_entries,
                        blocks,
                    } => {
                        framing.model_regions += 1;
                        framing.model_sub_blocks += blocks.len();
                        framing.model_entries += u64::from(*total_entries);
                    }
                    HandlerFraming::RelocationTail(tail) => {
                        framing.relocation_regions += 1;
                        framing.relocation_records += u64::from(tail.relocation_count);
                    }
                }
            }
            layout.regions
        };

        files.push(InventoryFile {
            name: name.to_owned(),
            key,
            size: bytes.len(),
            regions,
        });
    }

    Ok(InventoryReport {
        summary: InventorySummary {
            preload_bytes: preload_bytes.len(),
            preload_leaves: leaf_ids.len(),
            files: files.len(),
            non_empty_files,
            empty_files: files.len() - non_empty_files,
            input_bytes,
            covered_bytes,
            handler_regions,
            framing,
        },
        files,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, InventoryError> {
    fs::read(path).map_err(|source| InventoryError::Io {
        path: path.to_owned(),
        source,
    })
}
