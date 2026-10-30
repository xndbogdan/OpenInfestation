//! Lossless archive for Handler-4/13/14 source regions. Typed font metadata
//! belongs to `fonts`; proven constructor fields and world placements to `worlds`.
//!
//! This deliberately stops at the framing recovered from the retail loader.
//! The emitted `.bin` files include the framing bytes themselves, while the
//! manifest preserves the PreLoad request and local-to-global registration
//! provenance needed for further decoding without returning to an older
//! extractor. H4 remains here for compatibility with earlier raw archives.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_overlay, ByteSpan, Error as FormatError, HandlerFraming, HandlerRequest, OverlayKey,
    Preload, RequestKind,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

pub const OPAQUE_HANDLERS: [u8; 3] = [4, 13, 14];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpaqueExportReport {
    pub source_ovls: usize,
    pub regions: usize,
    pub regions_by_handler: [usize; 3],
    pub requested_resources: usize,
    pub requested_resources_by_handler: [usize; 3],
    pub framed_bytes: usize,
    pub framed_bytes_by_handler: [usize; 3],
}

#[derive(Debug, Error)]
pub enum OpaqueExportError {
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

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} requests Handler {handler}, but its framed OVL has no such region")]
    MissingRegion { path: PathBuf, handler: u8 },

    #[error("{path} Handler {handler} local slot {local_slot} has no global mapping")]
    MissingGlobalMapping {
        path: PathBuf,
        handler: u8,
        local_slot: u16,
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
struct OpaqueManifest {
    schema_version: u32,
    kind: &'static str,
    status: &'static str,
    handlers: [u8; 3],
    provenance: OpaqueProvenance,
    totals: OpaqueTotals,
    sources: Vec<OpaqueSource>,
}

#[derive(Serialize)]
struct OpaqueProvenance {
    scope: &'static str,
    framed_region_bytes: &'static str,
    global_mapping: &'static str,
    handler_4_framing: &'static str,
    handler_13_14_framing: &'static str,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
struct OpaqueTotals {
    source_ovls: usize,
    regions: usize,
    regions_by_handler: [usize; 3],
    requested_resources: usize,
    requested_resources_by_handler: [usize; 3],
    framed_bytes: usize,
    framed_bytes_by_handler: [usize; 3],
}

#[derive(Serialize)]
struct OpaqueSource {
    ovl: String,
    ovl_bytes: usize,
    key: OverlayKey,
    request_kind: RequestKind,
    handler: u8,
    request: OpaqueRequest,
    local_to_global: Vec<GlobalMapping>,
    framed_region: OpaqueRegion,
}

#[derive(Serialize)]
struct OpaqueRequest {
    count: i16,
    segments: Vec<RequestSegment>,
}

#[derive(Serialize)]
struct RequestSegment {
    segment_index: usize,
    local_start: u16,
    global_delta: i16,
    packed_u32: u32,
    packed_hex: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct GlobalMapping {
    local_slot: u16,
    global_slot: u16,
    selected_segment_index: usize,
    selected_local_start: u16,
    selected_global_delta: i16,
}

#[derive(Serialize)]
struct OpaqueRegion {
    ovl_span: ByteSpan,
    framed_bytes: usize,
    raw_framed_region: String,
    framing: HandlerFraming,
}

pub fn export(
    data_root: &Path,
    output_root: &Path,
) -> Result<OpaqueExportReport, OpaqueExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| OpaqueExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("opaque_resources");
    let staged = StagedOutput::new(&target)?;
    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| OpaqueExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let mut totals = OpaqueTotals::default();
    let mut sources = Vec::new();

    for path in overlay_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| OpaqueExportError::InvalidSourcePath(path.clone()))?;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| OpaqueExportError::InvalidSourcePath(path.clone()))?;
        let key = OverlayKey::parse_filename(name).map_err(|source| OpaqueExportError::Format {
            path: path.clone(),
            source,
        })?;
        let request_kind = key.request_kind();
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| OpaqueExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(request_kind);
        if !OPAQUE_HANDLERS
            .into_iter()
            .any(|handler| requests.get(handler as usize).unwrap().count != 0)
        {
            continue;
        }

        let bytes = read(&path)?;
        let layout =
            parse_overlay(&bytes, requests).map_err(|source| OpaqueExportError::Format {
                path: path.clone(),
                source,
            })?;
        let mut source_has_region = false;

        for (handler_index, handler) in OPAQUE_HANDLERS.into_iter().enumerate() {
            let request = requests
                .get(handler as usize)
                .expect("PreLoad has exactly 16 handlers");
            if request.count == 0 {
                continue;
            }
            let region = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == handler)
                .ok_or_else(|| OpaqueExportError::MissingRegion {
                    path: path.clone(),
                    handler,
                })?;
            let mappings = global_mappings(request).ok_or_else(|| {
                let local_slot = first_unmapped_slot(request)
                    .expect("a failed complete mapping has an unmapped local slot");
                OpaqueExportError::MissingGlobalMapping {
                    path: path.clone(),
                    handler,
                    local_slot,
                }
            })?;
            let framed = region
                .span
                .get(&bytes)
                .expect("overlay parser validated the region span");
            let source_dir = staged.path().join("sources").join(stem);
            fs::create_dir_all(&source_dir).map_err(|source| OpaqueExportError::Write {
                path: source_dir.clone(),
                source,
            })?;
            let file_name = format!("handler_{handler:02}_framed.bin");
            let raw_path = source_dir.join(&file_name);
            fs::write(&raw_path, framed).map_err(|source| OpaqueExportError::Write {
                path: raw_path,
                source,
            })?;

            let requested_resources = usize::try_from(region.requested_count)
                .expect("positive i16 request counts always fit usize");
            totals.regions += 1;
            totals.regions_by_handler[handler_index] += 1;
            totals.requested_resources += requested_resources;
            totals.requested_resources_by_handler[handler_index] += requested_resources;
            totals.framed_bytes += framed.len();
            totals.framed_bytes_by_handler[handler_index] += framed.len();
            source_has_region = true;

            sources.push(OpaqueSource {
                ovl: name.to_owned(),
                ovl_bytes: bytes.len(),
                key: key.clone(),
                request_kind,
                handler,
                request: request_record(request),
                local_to_global: mappings,
                framed_region: OpaqueRegion {
                    ovl_span: region.span,
                    framed_bytes: framed.len(),
                    raw_framed_region: format!("sources/{stem}/{file_name}"),
                    framing: region.framing.clone(),
                },
            });
        }
        totals.source_ovls += usize::from(source_has_region);
    }

    let report = OpaqueExportReport {
        source_ovls: totals.source_ovls,
        regions: totals.regions,
        regions_by_handler: totals.regions_by_handler,
        requested_resources: totals.requested_resources,
        requested_resources_by_handler: totals.requested_resources_by_handler,
        framed_bytes: totals.framed_bytes,
        framed_bytes_by_handler: totals.framed_bytes_by_handler,
    };
    let manifest = OpaqueManifest {
        schema_version: 1,
        kind: "infestation_handlers_4_13_14_opaque_framed_regions",
        status: "raw_archive_with_typed_exports_available",
        handlers: OPAQUE_HANDLERS,
        provenance: OpaqueProvenance {
            scope: "This archive assigns no payload records. The fonts family owns Handler-4 metadata; world_assets owns Handler-13 prefixes, initial model bindings, shared class/damage-response fields and Handler-14 placement/instance-command graphs. Derived class and specialized command payload semantics remain open.",
            framed_region_bytes: "Each raw file is the exact HandlerRegion span from the source OVL, including its size prefix and any recovered framing tail.",
            global_mapping: "FUN_004EFC00 selects the final request segment whose local_start does not exceed the local slot, then adds that segment's signed global_delta.",
            handler_4_framing: "Standard loader framing: u32 payload byte count followed by that exact payload.",
            handler_13_14_framing: "FUN_004eddb0 framing only: u32 main size, main bytes, u32 relocation count, relocation offsets, then one entry offset per requested resource.",
        },
        totals,
        sources,
    };
    write_json(&staged.path().join("manifest.json"), &manifest)?;
    staged.commit()?;
    Ok(report)
}

fn request_record(request: &HandlerRequest) -> OpaqueRequest {
    OpaqueRequest {
        count: request.count,
        segments: request
            .segments
            .iter()
            .enumerate()
            .map(|(segment_index, segment)| RequestSegment {
                segment_index,
                local_start: segment.local_start,
                global_delta: segment.global_delta,
                packed_u32: segment.packed(),
                packed_hex: format!("0x{:08X}", segment.packed()),
            })
            .collect(),
    }
}

fn global_mappings(request: &HandlerRequest) -> Option<Vec<GlobalMapping>> {
    let count = u16::try_from(request.count).ok()?;
    (0..count)
        .map(|local_slot| resolve_mapping(request, local_slot))
        .collect()
}

fn first_unmapped_slot(request: &HandlerRequest) -> Option<u16> {
    let count = u16::try_from(request.count).ok()?;
    (0..count).find(|&local_slot| resolve_mapping(request, local_slot).is_none())
}

fn resolve_mapping(request: &HandlerRequest, local_slot: u16) -> Option<GlobalMapping> {
    let (selected_segment_index, segment) = request
        .segments
        .iter()
        .enumerate()
        .rev()
        .find(|(_, segment)| segment.local_start <= local_slot)?;
    let global_slot = request.global_index(local_slot)?;
    Some(GlobalMapping {
        local_slot,
        global_slot,
        selected_segment_index,
        selected_local_start: segment.local_start,
        selected_global_delta: segment.global_delta,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, OpaqueExportError> {
    fs::read(path).map_err(|source| OpaqueExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), OpaqueExportError> {
    let file = fs::File::create(path).map_err(|source| OpaqueExportError::Write {
        path: path.to_owned(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|source| OpaqueExportError::Json {
        path: path.to_owned(),
        source,
    })?;
    writer.flush().map_err(|source| OpaqueExportError::Write {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::ResourceSegment;

    #[test]
    fn mapping_records_the_selected_signed_delta_segment() {
        let request = HandlerRequest {
            count: 4,
            segments: vec![
                ResourceSegment {
                    local_start: 0,
                    global_delta: 20,
                },
                ResourceSegment {
                    local_start: 2,
                    global_delta: -1,
                },
            ],
        };

        assert_eq!(
            global_mappings(&request).unwrap(),
            [
                GlobalMapping {
                    local_slot: 0,
                    global_slot: 20,
                    selected_segment_index: 0,
                    selected_local_start: 0,
                    selected_global_delta: 20,
                },
                GlobalMapping {
                    local_slot: 1,
                    global_slot: 21,
                    selected_segment_index: 0,
                    selected_local_start: 0,
                    selected_global_delta: 20,
                },
                GlobalMapping {
                    local_slot: 2,
                    global_slot: 1,
                    selected_segment_index: 1,
                    selected_local_start: 2,
                    selected_global_delta: -1,
                },
                GlobalMapping {
                    local_slot: 3,
                    global_slot: 2,
                    selected_segment_index: 1,
                    selected_local_start: 2,
                    selected_global_delta: -1,
                },
            ]
        );
    }

    #[test]
    fn retail_export_is_lossless_complete_and_repeatable() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let output_root =
            std::env::temp_dir().join(format!("infestation-opaque-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            OpaqueExportReport {
                source_ovls: 40,
                regions: 43,
                regions_by_handler: [5, 5, 33],
                requested_resources: 200,
                requested_resources_by_handler: [5, 162, 33],
                framed_bytes: 362_964,
                framed_bytes_by_handler: [14_164, 12_504, 336_296],
            }
        );

        let first_manifest = fs::read(output_root.join("opaque_resources/manifest.json")).unwrap();
        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("opaque_resources/manifest.json")).unwrap(),
            first_manifest
        );

        let manifest: serde_json::Value = serde_json::from_slice(&first_manifest).unwrap();
        let sources = manifest["sources"].as_array().unwrap();
        assert_eq!(sources.len(), first.regions);
        for source in sources {
            let relative = source["framed_region"]["raw_framed_region"]
                .as_str()
                .unwrap();
            let raw = fs::read(output_root.join("opaque_resources").join(relative)).unwrap();
            let ovl = source["ovl"].as_str().unwrap();
            let ovl_bytes = fs::read(data_root.join("Overlay").join(ovl)).unwrap();
            let start = source["framed_region"]["ovl_span"]["start"]
                .as_u64()
                .unwrap() as usize;
            let end = source["framed_region"]["ovl_span"]["end"].as_u64().unwrap() as usize;
            assert_eq!(raw, ovl_bytes[start..end], "{ovl}");
            assert_eq!(
                raw.len() as u64,
                source["framed_region"]["framed_bytes"].as_u64().unwrap(),
                "{ovl}"
            );
            assert_eq!(
                source["local_to_global"].as_array().unwrap().len() as i64,
                source["request"]["count"].as_i64().unwrap(),
                "{ovl}"
            );
            let handler = source["handler"].as_u64().unwrap();
            let framing = source["framed_region"]["framing"]["kind"].as_str().unwrap();
            assert!(matches!(
                (handler, framing),
                (4, "standard") | (13 | 14, "relocation_tail")
            ));
        }
        let _ = fs::remove_dir_all(output_root);
    }
}
