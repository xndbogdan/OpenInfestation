use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_overlay, parse_string_bank, Error as FormatError, OverlayKey, Preload, StringError,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StringExportReport {
    pub sources: usize,
    pub strings: usize,
    pub payload_bytes: u64,
}

#[derive(Debug, Error)]
pub enum StringExportError {
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

    #[error("failed to decode strings in {path}: {source}")]
    Strings { path: PathBuf, source: StringError },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} requests Handler 0 but its framed OVL has no Handler-0 region")]
    MissingStringRegion { path: PathBuf },

    #[error("{path} Handler-0 local slot {local_slot} has no global resource mapping")]
    MissingGlobalMapping { path: PathBuf, local_slot: u32 },

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
struct StringManifest {
    schema_version: u32,
    kind: &'static str,
    text_projection: TextProjection,
    totals: StringTotals,
    sources: Vec<StringSource>,
    strings: Vec<StringRecord>,
}

#[derive(Serialize)]
struct TextProjection {
    encoding: &'static str,
    note: &'static str,
}

#[derive(Default, Serialize)]
struct StringTotals {
    source_ovls: usize,
    strings: usize,
    payload_bytes: u64,
}

#[derive(Serialize)]
struct StringSource {
    ovl: String,
    raw_file: String,
    slots: usize,
    payload_bytes: usize,
    trailing_bytes: usize,
}

#[derive(Serialize)]
struct StringRecord {
    ovl: String,
    local_slot: u32,
    global_slot: u16,
    raw_file: String,
    raw_offset: usize,
    byte_len: usize,
    text: String,
}

pub fn export(
    data_root: &Path,
    output_root: &Path,
) -> Result<StringExportReport, StringExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| StringExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("strings");
    let staged = StagedOutput::new(&target)?;
    let source_root = staged.path().join("sources");
    create_dir_all(&source_root)?;

    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| StringExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let mut totals = StringTotals::default();
    let mut sources = Vec::new();
    let mut strings = Vec::new();

    for path in overlay_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| StringExportError::InvalidSourcePath(path.clone()))?;
        let key = OverlayKey::parse_filename(name).map_err(|source| StringExportError::Format {
            path: path.clone(),
            source,
        })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| StringExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        let string_request = requests.get(0).expect("PreLoad has exactly 16 handlers");
        if string_request.count == 0 {
            continue;
        }

        let bytes = read(&path)?;
        let layout =
            parse_overlay(&bytes, requests).map_err(|source| StringExportError::Format {
                path: path.clone(),
                source,
            })?;
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 0)
            .ok_or_else(|| StringExportError::MissingStringRegion { path: path.clone() })?;
        let bank =
            parse_string_bank(&bytes, region).map_err(|source| StringExportError::Strings {
                path: path.clone(),
                source,
            })?;
        let source_key = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| StringExportError::InvalidSourcePath(path.clone()))?;
        let raw_file = format!("sources/{source_key}.bin");
        let payload = bank
            .payload
            .get(&bytes)
            .expect("parse_string_bank validated the payload span");
        write(&staged.path().join(Path::new(&raw_file)), payload)?;

        for entry in &bank.entries {
            let global_slot = string_request
                .global_index(entry.local_index as u16)
                .ok_or_else(|| StringExportError::MissingGlobalMapping {
                    path: path.clone(),
                    local_slot: entry.local_index,
                })?;
            strings.push(StringRecord {
                ovl: name.to_owned(),
                local_slot: entry.local_index,
                global_slot,
                raw_file: raw_file.clone(),
                raw_offset: entry.bytes.start - bank.payload.start,
                byte_len: entry.bytes.len(),
                text: entry
                    .byte_preserving_text(&bytes)
                    .expect("parse_string_bank validated the string span"),
            });
        }
        totals.source_ovls += 1;
        totals.strings += bank.entries.len();
        totals.payload_bytes += bank.payload.len() as u64;
        sources.push(StringSource {
            ovl: name.to_owned(),
            raw_file,
            slots: bank.entries.len(),
            payload_bytes: bank.payload.len(),
            trailing_bytes: bank.trailing.len(),
        });
    }

    let report = StringExportReport {
        sources: totals.source_ovls,
        strings: totals.strings,
        payload_bytes: totals.payload_bytes,
    };
    let manifest = StringManifest {
        schema_version: 1,
        kind: "infestation_handler_0_strings",
        text_projection: TextProjection {
            encoding: "byte_preserving_latin1",
            note: "Raw .bin files are authoritative; active Windows code page is not stored in OVL data.",
        },
        totals,
        sources,
        strings,
    };
    let manifest_path = staged.path().join("manifest.json");
    let file = fs::File::create(&manifest_path).map_err(|source| StringExportError::Write {
        path: manifest_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &manifest).map_err(|source| {
        StringExportError::Json {
            path: manifest_path.clone(),
            source,
        }
    })?;
    writer.flush().map_err(|source| StringExportError::Write {
        path: manifest_path,
        source,
    })?;
    drop(writer);
    staged.commit()?;
    Ok(report)
}

fn read(path: &Path) -> Result<Vec<u8>, StringExportError> {
    fs::read(path).map_err(|source| StringExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), StringExportError> {
    fs::create_dir_all(path).map_err(|source| StringExportError::Write {
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), StringExportError> {
    fs::write(path, bytes).map_err(|source| StringExportError::Write {
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
            std::env::temp_dir().join(format!("infestation-string-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            StringExportReport {
                sources: 164,
                strings: 8_126,
                payload_bytes: 375_362,
            }
        );
        let first_manifest = fs::read(output_root.join("strings/manifest.json")).unwrap();
        assert_eq!(
            fs::read_dir(output_root.join("strings/sources"))
                .unwrap()
                .count(),
            164
        );
        assert_eq!(
            fs::read(output_root.join("strings/sources/0.bin")).unwrap(),
            fs::read(data_root.join("Overlay/0.ovl")).unwrap()[4..57]
        );

        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("strings/manifest.json")).unwrap(),
            first_manifest
        );
        let _ = fs::remove_dir_all(output_root);
    }
}
