use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_mission_parameters, parse_mission_placements, parse_overlay, Error as FormatError,
    MissionError, OverlayKey, Preload,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissionExportReport {
    pub sources: usize,
    pub placement_tables: usize,
    pub parameter_records: usize,
    pub nonzero_placement_slots: usize,
}

#[derive(Debug, Error)]
pub enum MissionExportError {
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

    #[error("failed to decode mission data in {path}: {source}")]
    Mission { path: PathBuf, source: MissionError },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} requests Handler {handler} but its framed OVL has no such region")]
    MissingRegion { path: PathBuf, handler: u8 },

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
struct MissionManifest {
    schema_version: u32,
    kind: &'static str,
    totals: MissionTotals,
    sources: Vec<MissionSource>,
}

#[derive(Default, Serialize)]
struct MissionTotals {
    source_ovls: usize,
    placement_tables: usize,
    parameter_records: usize,
    nonzero_placement_slots: usize,
}

#[derive(Serialize)]
struct MissionSource {
    ovl: String,
    level: u8,
    placement_tables: Vec<PlacementTableRecord>,
    parameter_records: Vec<ParameterRecord>,
}

#[derive(Serialize)]
struct PlacementTableRecord {
    local_slot: u32,
    global_slot: u16,
    slots: Vec<PlacementSlotRecord>,
}

#[derive(Serialize)]
struct PlacementSlotRecord {
    slot: u16,
    resource_word: u16,
    parameter_word: u16,
}

#[derive(Serialize)]
struct ParameterRecord {
    local_slot: u32,
    global_slot: u16,
    fields: [u32; 5],
}

pub fn export(
    data_root: &Path,
    output_root: &Path,
) -> Result<MissionExportReport, MissionExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| MissionExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("missions");
    let staged = StagedOutput::new(&target)?;
    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| MissionExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let mut totals = MissionTotals::default();
    let mut sources = Vec::new();

    for path in overlay_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| MissionExportError::InvalidSourcePath(path.clone()))?;
        let key =
            OverlayKey::parse_filename(name).map_err(|source| MissionExportError::Format {
                path: path.clone(),
                source,
            })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| MissionExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        let placements_request = requests.get(11).expect("PreLoad has exactly 16 handlers");
        let parameters_request = requests.get(15).expect("PreLoad has exactly 16 handlers");
        if placements_request.count == 0 && parameters_request.count == 0 {
            continue;
        }
        let bytes = read(&path)?;
        let layout =
            parse_overlay(&bytes, requests).map_err(|source| MissionExportError::Format {
                path: path.clone(),
                source,
            })?;
        let mut placement_tables = Vec::new();
        let mut parameter_records = Vec::new();

        if placements_request.count != 0 {
            let region = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == 11)
                .ok_or_else(|| MissionExportError::MissingRegion {
                    path: path.clone(),
                    handler: 11,
                })?;
            let bank = parse_mission_placements(&bytes, region).map_err(|source| {
                MissionExportError::Mission {
                    path: path.clone(),
                    source,
                }
            })?;
            for table in bank.entries {
                let global_slot = placements_request
                    .global_index(table.local_index as u16)
                    .ok_or_else(|| MissionExportError::MissingGlobalMapping {
                        path: path.clone(),
                        handler: 11,
                        local_slot: table.local_index,
                    })?;
                let slots = table
                    .slots
                    .into_iter()
                    .map(|slot| PlacementSlotRecord {
                        slot: slot.slot,
                        resource_word: slot.resource_word,
                        parameter_word: slot.parameter_word,
                    })
                    .collect::<Vec<_>>();
                totals.nonzero_placement_slots += slots
                    .iter()
                    .filter(|slot| slot.resource_word != 0 || slot.parameter_word != 0)
                    .count();
                totals.placement_tables += 1;
                placement_tables.push(PlacementTableRecord {
                    local_slot: table.local_index,
                    global_slot,
                    slots,
                });
            }
        }

        if parameters_request.count != 0 {
            let region = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == 15)
                .ok_or_else(|| MissionExportError::MissingRegion {
                    path: path.clone(),
                    handler: 15,
                })?;
            let bank = parse_mission_parameters(&bytes, region).map_err(|source| {
                MissionExportError::Mission {
                    path: path.clone(),
                    source,
                }
            })?;
            for record in bank.entries {
                let global_slot = parameters_request
                    .global_index(record.local_index as u16)
                    .ok_or_else(|| MissionExportError::MissingGlobalMapping {
                        path: path.clone(),
                        handler: 15,
                        local_slot: record.local_index,
                    })?;
                totals.parameter_records += 1;
                parameter_records.push(ParameterRecord {
                    local_slot: record.local_index,
                    global_slot,
                    fields: record.fields,
                });
            }
        }

        totals.source_ovls += 1;
        sources.push(MissionSource {
            ovl: name.to_owned(),
            level: key.level,
            placement_tables,
            parameter_records,
        });
    }

    let report = MissionExportReport {
        sources: totals.source_ovls,
        placement_tables: totals.placement_tables,
        parameter_records: totals.parameter_records,
        nonzero_placement_slots: totals.nonzero_placement_slots,
    };
    let manifest = MissionManifest {
        schema_version: 1,
        kind: "infestation_handlers_11_15_missions",
        totals,
        sources,
    };
    let manifest_path = staged.path().join("manifest.json");
    let file = fs::File::create(&manifest_path).map_err(|source| MissionExportError::Write {
        path: manifest_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &manifest).map_err(|source| {
        MissionExportError::Json {
            path: manifest_path.clone(),
            source,
        }
    })?;
    writer.flush().map_err(|source| MissionExportError::Write {
        path: manifest_path,
        source,
    })?;
    drop(writer);
    staged.commit()?;
    Ok(report)
}

fn read(path: &Path) -> Result<Vec<u8>, MissionExportError> {
    fs::read(path).map_err(|source| MissionExportError::Read {
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
            std::env::temp_dir().join(format!("infestation-mission-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            MissionExportReport {
                sources: 8,
                placement_tables: 8,
                parameter_records: 8,
                nonzero_placement_slots: 536,
            }
        );
        let first_manifest = fs::read(output_root.join("missions/manifest.json")).unwrap();
        assert!(first_manifest
            .windows(b"\"level\": 3".len())
            .any(|window| window == b"\"level\": 3"));

        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("missions/manifest.json")).unwrap(),
            first_manifest
        );
        let _ = fs::remove_dir_all(output_root);
    }
}
