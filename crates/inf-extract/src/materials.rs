use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_material_bank, parse_overlay, Error as FormatError, MaterialError, OverlayKey, Preload,
    MATERIAL_HANDLER, TERRAIN_HANDLER, TERRAIN_MATERIAL_PALETTE_ENTRIES,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialExportReport {
    pub sources: usize,
    pub entries: usize,
    pub payload_bytes: usize,
    pub terrain_palettes: usize,
}

#[derive(Debug, Error)]
pub enum MaterialExportError {
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

    #[error("failed to decode Handler-7 materials in {path}: {source}")]
    Materials {
        path: PathBuf,
        source: MaterialError,
    },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} Handler-7 local slot {local_slot} has no global mapping")]
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
struct MaterialManifest {
    schema_version: u32,
    kind: &'static str,
    provenance: &'static str,
    totals: MaterialTotals,
    sources: Vec<MaterialSource>,
}

#[derive(Default, Serialize)]
struct MaterialTotals {
    source_regions: usize,
    entries: usize,
    payload_bytes: usize,
    exact_eight_entry_terrain_palettes: usize,
}

#[derive(Serialize)]
struct MaterialSource {
    ovl: String,
    level: u8,
    raw_payload: String,
    entries: Vec<MaterialRecord>,
    exact_eight_entry_terrain_palette: bool,
}

#[derive(Serialize)]
struct MaterialRecord {
    local_slot: u32,
    global_slot: u16,
    raw_u32: u32,
    raw_hex: String,
    low_rgb555: u16,
    rgb8: [u8; 3],
    high_word: u16,
    recovered_terrain_role: Option<&'static str>,
}

pub fn export(
    data_root: &Path,
    output_root: &Path,
) -> Result<MaterialExportReport, MaterialExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| MaterialExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("materials");
    let staged = StagedOutput::new(&target)?;
    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| MaterialExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let mut totals = MaterialTotals::default();
    let mut sources = Vec::new();

    for path in overlay_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| MaterialExportError::InvalidSourcePath(path.clone()))?;
        let key =
            OverlayKey::parse_filename(name).map_err(|source| MaterialExportError::Format {
                path: path.clone(),
                source,
            })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| MaterialExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        let request = requests
            .get(MATERIAL_HANDLER as usize)
            .expect("PreLoad has exactly 16 handlers");
        if request.count == 0 {
            continue;
        }

        let bytes = read(&path)?;
        let layout =
            parse_overlay(&bytes, requests).map_err(|source| MaterialExportError::Format {
                path: path.clone(),
                source,
            })?;
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == MATERIAL_HANDLER)
            .expect("active Handler-7 request has a framed region");
        let bank = parse_material_bank(&bytes, region).map_err(|source| {
            MaterialExportError::Materials {
                path: path.clone(),
                source,
            }
        })?;
        let has_same_ovl_terrain = requests
            .get(TERRAIN_HANDLER as usize)
            .expect("PreLoad has exactly 16 handlers")
            .count
            != 0;
        let exact_terrain_palette =
            has_same_ovl_terrain && bank.entries.len() == TERRAIN_MATERIAL_PALETTE_ENTRIES;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| MaterialExportError::InvalidSourcePath(path.clone()))?;
        let source_dir = staged.path().join(stem);
        fs::create_dir_all(&source_dir).map_err(|source| MaterialExportError::Write {
            path: source_dir.clone(),
            source,
        })?;
        let raw_path = source_dir.join("payload.bin");
        let payload = bank
            .payload
            .get(&bytes)
            .expect("material parser validated its payload span");
        fs::write(&raw_path, payload).map_err(|source| MaterialExportError::Write {
            path: raw_path,
            source,
        })?;

        let mut entries = Vec::with_capacity(bank.entries.len());
        for (local_slot, value) in bank.entries.into_iter().enumerate() {
            let global_slot = request.global_index(local_slot as u16).ok_or_else(|| {
                MaterialExportError::MissingGlobalMapping {
                    path: path.clone(),
                    local_slot: local_slot as u32,
                }
            })?;
            let rgb = value.rgb555();
            entries.push(MaterialRecord {
                local_slot: local_slot as u32,
                global_slot,
                raw_u32: value.raw(),
                raw_hex: format!("0x{:08X}", value.raw()),
                low_rgb555: rgb.raw(),
                rgb8: [
                    expand_5_to_8(rgb.red5()),
                    expand_5_to_8(rgb.green5()),
                    expand_5_to_8(rgb.blue5()),
                ],
                high_word: value.high_word(),
                recovered_terrain_role: exact_terrain_palette.then(|| terrain_role(local_slot)),
            });
        }

        totals.source_regions += 1;
        totals.entries += entries.len();
        totals.payload_bytes += payload.len();
        totals.exact_eight_entry_terrain_palettes += usize::from(exact_terrain_palette);
        sources.push(MaterialSource {
            ovl: name.to_owned(),
            level: key.level,
            raw_payload: format!("{stem}/payload.bin"),
            entries,
            exact_eight_entry_terrain_palette: exact_terrain_palette,
        });
    }

    let report = MaterialExportReport {
        sources: totals.source_regions,
        entries: totals.entries,
        payload_bytes: totals.payload_bytes,
        terrain_palettes: totals.exact_eight_entry_terrain_palettes,
    };
    let manifest = MaterialManifest {
        schema_version: 1,
        kind: "infestation_handler_7_material_values",
        provenance: "Four-byte stride and low-word RGB555 consumption are recovered from the retail loader and renderer; terrain roles are assigned only to eight-entry H7 banks loaded beside H12 in the same OVL.",
        totals,
        sources,
    };
    write_json(&staged.path().join("manifest.json"), &manifest)?;
    staged.commit()?;
    Ok(report)
}

fn terrain_role(index: usize) -> &'static str {
    match index {
        0..=3 => "surface_material",
        4 => "plane_or_water",
        5..=7 => "unresolved",
        _ => unreachable!("terrain roles are emitted only for eight-entry banks"),
    }
}

const fn expand_5_to_8(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn read(path: &Path) -> Result<Vec<u8>, MaterialExportError> {
    fs::read(path).map_err(|source| MaterialExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), MaterialExportError> {
    let file = fs::File::create(path).map_err(|source| MaterialExportError::Write {
        path: path.to_owned(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|source| {
        MaterialExportError::Json {
            path: path.to_owned(),
            source,
        }
    })?;
    writer.flush().map_err(|source| MaterialExportError::Write {
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
        let output_root = std::env::temp_dir().join(format!(
            "infestation-material-export-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            MaterialExportReport {
                sources: 35,
                entries: 378,
                payload_bytes: 1_512,
                terrain_palettes: 33,
            }
        );
        let first_manifest = fs::read(output_root.join("materials/manifest.json")).unwrap();
        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("materials/manifest.json")).unwrap(),
            first_manifest
        );
        let _ = fs::remove_dir_all(output_root);
    }

    #[test]
    fn rgb555_expansion_uses_bit_replication() {
        assert_eq!(expand_5_to_8(0), 0);
        assert_eq!(expand_5_to_8(31), 255);
        assert_eq!(expand_5_to_8(16), 132);
    }
}
