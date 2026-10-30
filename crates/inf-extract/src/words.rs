use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_overlay, parse_word_resource_bank, Error as FormatError, OverlayKey, Preload,
    WordResourceError, WORD_RESOURCE_HANDLERS,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordExportReport {
    pub sources: usize,
    pub entries: usize,
    pub entries_by_handler: [usize; 3],
}

#[derive(Debug, Error)]
pub enum WordExportError {
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

    #[error("failed to decode Handler {handler} words in {path}: {source}")]
    Words {
        path: PathBuf,
        handler: u8,
        source: WordResourceError,
    },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} Handler {handler} local slot {local_slot} has no global mapping")]
    MissingGlobalMapping {
        path: PathBuf,
        handler: u8,
        local_slot: u32,
    },

    #[error("source path cannot be represented in the manifest: {0}")]
    InvalidSourcePath(PathBuf),

    #[error("failed to serialize {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error(transparent)]
    ManagedOutput(#[from] ManagedOutputError),
}

#[derive(Serialize)]
struct WordManifest {
    schema_version: u32,
    kind: &'static str,
    handlers: [u8; 3],
    totals: WordTotals,
    sources: Vec<WordSource>,
}

#[derive(Default, Serialize)]
struct WordTotals {
    source_regions: usize,
    entries: usize,
    entries_by_handler: [usize; 3],
}

#[derive(Serialize)]
struct WordSource {
    ovl: String,
    handler: u8,
    entries: Vec<WordRecord>,
}

#[derive(Serialize)]
struct WordRecord {
    local_slot: u32,
    global_slot: u16,
    raw_u32: u32,
    raw_hex: String,
}

pub fn export(data_root: &Path, output_root: &Path) -> Result<WordExportReport, WordExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| WordExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("word_resources");
    let staged = StagedOutput::new(&target)?;
    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| WordExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let mut totals = WordTotals::default();
    let mut sources = Vec::new();

    for path in overlay_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| WordExportError::InvalidSourcePath(path.clone()))?;
        let key = OverlayKey::parse_filename(name).map_err(|source| WordExportError::Format {
            path: path.clone(),
            source,
        })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| WordExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        if !WORD_RESOURCE_HANDLERS
            .into_iter()
            .any(|handler| requests.get(handler as usize).unwrap().count != 0)
        {
            continue;
        }
        let bytes = read(&path)?;
        let layout = parse_overlay(&bytes, requests).map_err(|source| WordExportError::Format {
            path: path.clone(),
            source,
        })?;
        for (handler_slot, handler) in WORD_RESOURCE_HANDLERS.into_iter().enumerate() {
            let Some(region) = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == handler)
            else {
                continue;
            };
            let request = requests
                .get(handler as usize)
                .expect("PreLoad has exactly 16 handlers");
            let bank = parse_word_resource_bank(&bytes, region).map_err(|source| {
                WordExportError::Words {
                    path: path.clone(),
                    handler,
                    source,
                }
            })?;
            let mut records = Vec::with_capacity(bank.entries.len());
            for entry in bank.entries {
                let global_slot =
                    request
                        .global_index(entry.local_index as u16)
                        .ok_or_else(|| WordExportError::MissingGlobalMapping {
                            path: path.clone(),
                            handler,
                            local_slot: entry.local_index,
                        })?;
                records.push(WordRecord {
                    local_slot: entry.local_index,
                    global_slot,
                    raw_u32: entry.raw,
                    raw_hex: format!("0x{:08X}", entry.raw),
                });
            }
            totals.source_regions += 1;
            totals.entries += records.len();
            totals.entries_by_handler[handler_slot] += records.len();
            sources.push(WordSource {
                ovl: name.to_owned(),
                handler,
                entries: records,
            });
        }
    }

    let report = WordExportReport {
        sources: totals.source_regions,
        entries: totals.entries,
        entries_by_handler: totals.entries_by_handler,
    };
    let manifest = WordManifest {
        schema_version: 1,
        kind: "infestation_handlers_1_2_6_u32_resources",
        handlers: WORD_RESOURCE_HANDLERS,
        totals,
        sources,
    };
    let manifest_path = staged.path().join("manifest.json");
    let file = fs::File::create(&manifest_path).map_err(|source| WordExportError::Write {
        path: manifest_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &manifest).map_err(|source| {
        WordExportError::Json {
            path: manifest_path.clone(),
            source,
        }
    })?;
    writer.flush().map_err(|source| WordExportError::Write {
        path: manifest_path,
        source,
    })?;
    drop(writer);
    staged.commit()?;
    Ok(report)
}

fn read(path: &Path) -> Result<Vec<u8>, WordExportError> {
    fs::read(path).map_err(|source| WordExportError::Read {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retail_export_is_complete_and_repeatable() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let output_root =
            std::env::temp_dir().join(format!("infestation-word-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            WordExportReport {
                sources: 73,
                entries: 297,
                entries_by_handler: [152, 10, 135],
            }
        );
        let first_manifest = fs::read(output_root.join("word_resources/manifest.json")).unwrap();
        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("word_resources/manifest.json")).unwrap(),
            first_manifest
        );
        let _ = fs::remove_dir_all(output_root);
    }
}
