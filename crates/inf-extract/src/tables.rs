use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    decode_sample_grid, parse_handler10_bank, parse_handler5_bank, parse_overlay, ByteSpan,
    Error as FormatError, HandlerRequest, OverlayKey, Preload, RecordTableError, RequestKind,
    SampleGrid, SampleGridError, SampleGridRole, HANDLER_10, HANDLER_5,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableExportReport {
    pub sources: usize,
    pub entries_by_handler: [usize; 2],
    pub payload_bytes: usize,
    pub handler10_trailing_bytes: usize,
    pub sample_grids: usize,
    pub sample_bytes: usize,
}

#[derive(Debug, Error)]
pub enum TableExportError {
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

    #[error("failed to decode Handler-{handler} table in {path}: {source}")]
    Table {
        path: PathBuf,
        handler: u8,
        source: RecordTableError,
    },

    #[error("failed to decode Handler-10 global slot {global_slot} in {path}: {source}")]
    Samples {
        path: PathBuf,
        global_slot: u16,
        source: SampleGridError,
    },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} Handler-{handler} local slot {local_slot} has no global mapping")]
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
struct TableManifest {
    schema_version: u32,
    kind: &'static str,
    provenance: &'static str,
    totals: TableTotals,
    sources: Vec<TableSource>,
}

#[derive(Default, Serialize)]
struct TableTotals {
    source_regions: usize,
    handler_5_entries: usize,
    handler_10_entries: usize,
    payload_bytes: usize,
    handler_10_trailing_bytes: usize,
    sample_grids: usize,
    sample_bytes: usize,
}

#[derive(Serialize)]
#[serde(tag = "handler", rename_all = "snake_case")]
enum TableSource {
    Handler5 {
        ovl: String,
        level: u8,
        payload_span: ByteSpan,
        request_kind: RequestKind,
        request: HandlerRequest,
        raw_payload: String,
        records: Vec<Handler5Record>,
    },
    Handler10 {
        ovl: String,
        level: u8,
        payload_span: ByteSpan,
        request_kind: RequestKind,
        request: HandlerRequest,
        raw_payload: String,
        trailing_data: String,
        records: Vec<Handler10Record>,
    },
}

#[derive(Serialize)]
struct Handler5Record {
    local_slot: u32,
    global_slot: u16,
    words: [u32; 5],
}

#[derive(Serialize)]
struct Handler10Record {
    local_slot: u32,
    global_slot: u16,
    words_00_to_0c: [u32; 4],
    allocation_relative_offsets: [u32; 3],
    /// None means no proven consumer role for this global registration.
    sample_grid: Option<SampleGrid>,
}

pub fn export(data_root: &Path, output_root: &Path) -> Result<TableExportReport, TableExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| TableExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("record_tables");
    let staged = StagedOutput::new(&target)?;
    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| TableExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let mut totals = TableTotals::default();
    let mut sources = Vec::new();

    for path in overlay_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| TableExportError::InvalidSourcePath(path.clone()))?;
        let key = OverlayKey::parse_filename(name).map_err(|source| TableExportError::Format {
            path: path.clone(),
            source,
        })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| TableExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        let h5_request = requests
            .get(HANDLER_5 as usize)
            .expect("PreLoad has exactly 16 handlers");
        let h10_request = requests
            .get(HANDLER_10 as usize)
            .expect("PreLoad has exactly 16 handlers");
        if h5_request.count == 0 && h10_request.count == 0 {
            continue;
        }

        let bytes = read(&path)?;
        let layout =
            parse_overlay(&bytes, requests).map_err(|source| TableExportError::Format {
                path: path.clone(),
                source,
            })?;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| TableExportError::InvalidSourcePath(path.clone()))?;

        if let Some(region) = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == HANDLER_5)
        {
            let bank =
                parse_handler5_bank(&bytes, region).map_err(|source| TableExportError::Table {
                    path: path.clone(),
                    handler: HANDLER_5,
                    source,
                })?;
            let raw_relative = format!("{stem}/handler_5_payload.bin");
            write_bytes(
                &staged.path().join(&raw_relative),
                bank.payload
                    .get(&bytes)
                    .expect("record-table parser validated its payload"),
            )?;
            let mut records = Vec::with_capacity(bank.records.len());
            for record in bank.records {
                let global_slot = h5_request
                    .global_index(record.local_index as u16)
                    .ok_or_else(|| TableExportError::MissingGlobalMapping {
                        path: path.clone(),
                        handler: HANDLER_5,
                        local_slot: record.local_index,
                    })?;
                records.push(Handler5Record {
                    local_slot: record.local_index,
                    global_slot,
                    words: record.words,
                });
            }
            totals.source_regions += 1;
            totals.handler_5_entries += records.len();
            totals.payload_bytes += bank.payload.len();
            sources.push(TableSource::Handler5 {
                ovl: name.to_owned(),
                level: key.level,
                payload_span: bank.payload,
                request_kind: key.request_kind(),
                request: h5_request.clone(),
                raw_payload: raw_relative,
                records,
            });
        }

        if let Some(region) = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == HANDLER_10)
        {
            let bank =
                parse_handler10_bank(&bytes, region).map_err(|source| TableExportError::Table {
                    path: path.clone(),
                    handler: HANDLER_10,
                    source,
                })?;
            let raw_relative = format!("{stem}/handler_10_payload.bin");
            let trailing_relative = format!("{stem}/handler_10_trailing.bin");
            write_bytes(
                &staged.path().join(&raw_relative),
                bank.payload
                    .get(&bytes)
                    .expect("record-table parser validated its payload"),
            )?;
            write_bytes(
                &staged.path().join(&trailing_relative),
                bank.trailing_data
                    .get(&bytes)
                    .expect("record-table parser validated trailing data"),
            )?;
            let mut records = Vec::with_capacity(bank.records.len());
            for record in bank.records {
                let global_slot = h10_request
                    .global_index(record.local_index as u16)
                    .ok_or_else(|| TableExportError::MissingGlobalMapping {
                        path: path.clone(),
                        handler: HANDLER_10,
                        local_slot: record.local_index,
                    })?;
                records.push(Handler10Record {
                    local_slot: record.local_index,
                    global_slot,
                    words_00_to_0c: record.words_00_to_0c,
                    allocation_relative_offsets: record.relative_offsets,
                    sample_grid: SampleGridRole::from_global_slot(global_slot)
                        .map(|role| {
                            let grid = decode_sample_grid(&bytes, bank.payload, &record, role)
                                .map_err(|source| TableExportError::Samples {
                                    path: path.clone(),
                                    global_slot,
                                    source,
                                })?;
                            totals.sample_grids += 1;
                            totals.sample_bytes += grid.samples_span.len();
                            Ok::<_, TableExportError>(grid)
                        })
                        .transpose()?,
                });
            }
            totals.source_regions += 1;
            totals.handler_10_entries += records.len();
            totals.payload_bytes += bank.payload.len();
            totals.handler_10_trailing_bytes += bank.trailing_data.len();
            sources.push(TableSource::Handler10 {
                ovl: name.to_owned(),
                level: key.level,
                payload_span: bank.payload,
                request_kind: key.request_kind(),
                request: h10_request.clone(),
                raw_payload: raw_relative,
                trailing_data: trailing_relative,
                records,
            });
        }
    }

    let report = TableExportReport {
        sources: totals.source_regions,
        entries_by_handler: [totals.handler_5_entries, totals.handler_10_entries],
        payload_bytes: totals.payload_bytes,
        handler10_trailing_bytes: totals.handler_10_trailing_bytes,
        sample_grids: totals.sample_grids,
        sample_bytes: totals.sample_bytes,
    };
    let manifest = TableManifest {
        schema_version: 2,
        kind: "infestation_handlers_5_10_fixed_record_tables",
        provenance: "Exact record words and allocation-relative offsets are preserved. Handler-10 typed views follow the explicit global-slot roles consumed by 00470170; unsupported roles remain raw. Metadata constraints delimit the installed layout, not a proven generic array format. All spans are OVL-absolute; subtract payload_span.start to address raw_payload. Signed samples are not converted into final screen pixels.",
        totals,
        sources,
    };
    write_json(&staged.path().join("manifest.json"), &manifest)?;
    staged.commit()?;
    Ok(report)
}

fn read(path: &Path) -> Result<Vec<u8>, TableExportError> {
    fs::read(path).map_err(|source| TableExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), TableExportError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| TableExportError::Write {
            path: parent.to_owned(),
            source,
        })?;
    }
    fs::write(path, bytes).map_err(|source| TableExportError::Write {
        path: path.to_owned(),
        source,
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), TableExportError> {
    let file = fs::File::create(path).map_err(|source| TableExportError::Write {
        path: path.to_owned(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|source| TableExportError::Json {
        path: path.to_owned(),
        source,
    })?;
    writer.flush().map_err(|source| TableExportError::Write {
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
            std::env::temp_dir().join(format!("infestation-table-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            TableExportReport {
                sources: 6,
                entries_by_handler: [7, 15],
                payload_bytes: 288_580,
                handler10_trailing_bytes: 288_020,
                sample_grids: 15,
                sample_bytes: 287_700,
            }
        );
        let first_manifest = fs::read(output_root.join("record_tables/manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&first_manifest).unwrap();
        assert_eq!(manifest["schema_version"], 2);
        let mut decoded_grids = 0;
        for source in manifest["sources"].as_array().unwrap() {
            let bytes = fs::read(
                data_root
                    .join("Overlay")
                    .join(source["ovl"].as_str().unwrap()),
            )
            .unwrap();
            let span = &source["payload_span"];
            let start = span["start"].as_u64().unwrap() as usize;
            let end = span["end"].as_u64().unwrap() as usize;
            let raw = fs::read(
                output_root
                    .join("record_tables")
                    .join(source["raw_payload"].as_str().unwrap()),
            )
            .unwrap();
            assert_eq!(raw, bytes[start..end]);
            for record in source["records"].as_array().unwrap() {
                let grid = &record["sample_grid"];
                if grid.is_null() {
                    assert_eq!(source["handler"], "handler5");
                    continue;
                }
                let sample_start = grid["samples_span"]["start"].as_u64().unwrap() as usize;
                let sample_end = grid["samples_span"]["end"].as_u64().unwrap() as usize;
                let values = grid["samples"]["values"].as_array().unwrap();
                assert_eq!(
                    values.len() as u64,
                    grid["width"].as_u64().unwrap() * grid["height"].as_u64().unwrap()
                );
                let encoded: Vec<u8> = match grid["samples"]["encoding"].as_str().unwrap() {
                    "signed_words8" => values
                        .iter()
                        .flat_map(|sample| {
                            let components = sample.as_array().unwrap();
                            assert_eq!(components.len(), 8);
                            components.iter().flat_map(|word| {
                                i16::try_from(word.as_i64().unwrap()).unwrap().to_le_bytes()
                            })
                        })
                        .collect(),
                    "signed_bytes" => values
                        .iter()
                        .map(|value| i8::try_from(value.as_i64().unwrap()).unwrap() as u8)
                        .collect(),
                    encoding => panic!("unexpected sample encoding {encoding}"),
                };
                assert_eq!(encoded, bytes[sample_start..sample_end]);
                assert_eq!(encoded, raw[sample_start - start..sample_end - start]);
                decoded_grids += 1;
            }
        }
        assert_eq!(decoded_grids, 15);
        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("record_tables/manifest.json")).unwrap(),
            first_manifest
        );
        let _ = fs::remove_dir_all(output_root);
    }
}
