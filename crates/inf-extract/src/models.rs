//! Lossless Handler-8 export plus a conservative static-command OBJ aid.
//!
//! The JSON and aligned entry binaries preserve the audited parser result.
//! OBJ output is diagnostic only and is omitted whenever producing it would
//! require an animation frame, runtime transform, model catalog, or guess.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    analyze_model_diagnostic, parse_model_bank, parse_overlay, ByteSpan, Error as FormatError,
    ModelBlock, ModelDiagnosticCompleteness, ModelDiagnosticFaceFlags, ModelDiagnosticFaceLighting,
    ModelDiagnosticFacePresentation, ModelDiagnosticGeometry, ModelDiagnosticOmission,
    ModelDiagnosticProjection, ModelDiagnosticUnavailableKind, ModelDynamicDependency, ModelEntry,
    ModelError, ModelMaterialDomain, OverlayKey, Preload, MODEL_DIAGNOSTIC_BASIS,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

const MODEL_HANDLER: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelExportOptions {
    pub ovl_filters: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelExportReport {
    pub sources: usize,
    pub entries: usize,
    pub raw_entry_files: usize,
    pub diagnostic_objs: usize,
    pub omitted_diagnostic_objs: usize,
}

#[derive(Debug, Error)]
pub enum ModelExportError {
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

    #[error("failed to decode Handler-8 models in {path}: {source}")]
    Models { path: PathBuf, source: ModelError },

    #[error("invalid --ovl filter {filter:?}: {source}")]
    InvalidOverlayFilter { filter: String, source: FormatError },

    #[error("no installed OVL matches these --ovl filters: {filters:?}")]
    UnknownOverlayFilters { filters: Vec<String> },

    #[error("requested OVL {path} has no Handler-8 models")]
    FilteredOverlayHasNoModels { path: PathBuf },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} requests Handler 8 but its framed OVL has no Handler-8 region")]
    MissingModelRegion { path: PathBuf },

    #[error("source path cannot be represented in the manifest: {0}")]
    InvalidSourcePath(PathBuf),

    #[error("{section} span {span:?} in {path} is outside the source OVL")]
    SpanOutOfBounds {
        path: PathBuf,
        section: &'static str,
        span: ByteSpan,
    },

    #[error("failed to serialize {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error(transparent)]
    ManagedOutput(#[from] ManagedOutputError),
}

#[derive(Serialize)]
struct ModelManifest {
    schema_version: u32,
    kind: &'static str,
    lossless_policy: LosslessPolicy,
    diagnostic_obj_policy: DiagnosticObjPolicy,
    ovl_filters: Vec<String>,
    totals: ModelTotals,
    global_slot_occurrences: Vec<GlobalSlotOccurrences>,
    sources: Vec<ModelSource>,
}

#[derive(Serialize)]
struct LosslessPolicy {
    serialization: &'static str,
    raw_entry_files: &'static str,
    manifest_bytes: &'static str,
    spans: &'static str,
    occurrence_order: &'static str,
    auxiliary_programs: &'static str,
}

#[derive(Serialize)]
struct DiagnosticObjPolicy {
    format: &'static str,
    basis: &'static str,
    warning: &'static str,
    slot_mapping: &'static str,
    omissions: &'static str,
}

#[derive(Default, Serialize)]
struct ModelTotals {
    source_ovls: usize,
    blocks: usize,
    entries: usize,
    raw_entry_files: usize,
    raw_entry_bytes: u64,
    diagnostic_objs: usize,
    omitted_diagnostic_objs: usize,
}

#[derive(Serialize)]
struct GlobalSlotOccurrences {
    global_index: u16,
    occurrences_in_manifest_source_order: Vec<GlobalOccurrence>,
}

#[derive(Clone, Serialize)]
struct GlobalOccurrence {
    ovl: String,
    local_index: u16,
    raw_file: String,
}

#[derive(Serialize)]
struct ModelSource {
    ovl: String,
    overlay: OverlayKey,
    region_span: ByteSpan,
    requested_entries: i16,
    blocks: Vec<ModelBlock>,
    entries: Vec<ModelEntryRecord>,
}

#[derive(Serialize)]
struct ModelEntryRecord {
    block_index: usize,
    raw_file: String,
    raw: RawEntryRecord,
    decoded: ModelEntry,
    unresolved_dependencies: Vec<ModelDynamicDependency>,
    diagnostic_obj: DiagnosticObjRecord,
}

#[derive(Serialize)]
struct RawEntryRecord {
    entry_bytes: usize,
    content_bytes: usize,
    header_bytes_le_hex: String,
    auxiliary_pool_bytes_le_hex: String,
    vertex_pool_bytes_le_hex: String,
    normal_pool_bytes_le_hex: String,
    command_pool_bytes_le_hex: String,
    name_bytes_hex: Option<String>,
    name_ascii: Option<String>,
    name_record_bytes_hex: Option<String>,
    padding_bytes_hex: String,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum DiagnosticObjRecord {
    Written {
        file: String,
        basis: &'static str,
        vertex_count: usize,
        face_count: usize,
        completeness: ModelDiagnosticCompleteness,
        face_presentation: Vec<ModelDiagnosticFacePresentation>,
        ignored_edge_commands: usize,
        omitted_effects: Vec<ModelDiagnosticOmission>,
        unresolved_dependencies: Vec<ModelDynamicDependency>,
    },
    Omitted {
        basis: &'static str,
        kind: ModelDiagnosticUnavailableKind,
        reason: String,
        referenced_face_slots: Vec<u16>,
        unresolved_dependencies: Vec<ModelDynamicDependency>,
        omitted_effects: Vec<ModelDiagnosticOmission>,
    },
}

pub fn export(
    data_root: &Path,
    output_root: &Path,
    options: &ModelExportOptions,
) -> Result<ModelExportReport, ModelExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| ModelExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("models");
    let staged = StagedOutput::new(&target)?;
    create_dir_all(&staged.path().join("sources"))?;

    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| ModelExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let (selected_paths, selected_filter_names, filtering) =
        select_overlay_paths(overlay_paths, &options.ovl_filters)?;

    let mut totals = ModelTotals::default();
    let mut sources = Vec::new();
    let mut occurrences = BTreeMap::<u16, Vec<GlobalOccurrence>>::new();

    for path in selected_paths {
        let name = source_file_name(&path)?;
        let overlay =
            OverlayKey::parse_filename(name).map_err(|source| ModelExportError::Format {
                path: path.clone(),
                source,
            })?;
        let leaf = preload
            .leaf(overlay.level)
            .ok_or_else(|| ModelExportError::MissingLeaf {
                path: path.clone(),
                level: overlay.level,
            })?;
        let requests = leaf.requests(overlay.request_kind());
        let model_request = requests
            .get(MODEL_HANDLER)
            .expect("PreLoad has exactly 16 handlers");
        if model_request.count == 0 {
            if filtering {
                return Err(ModelExportError::FilteredOverlayHasNoModels { path });
            }
            continue;
        }

        let source = read(&path)?;
        let layout =
            parse_overlay(&source, requests).map_err(|source_error| ModelExportError::Format {
                path: path.clone(),
                source: source_error,
            })?;
        let region = layout
            .regions
            .iter()
            .find(|region| usize::from(region.handler.get()) == MODEL_HANDLER)
            .ok_or_else(|| ModelExportError::MissingModelRegion { path: path.clone() })?;
        let bank =
            parse_model_bank(&source, region, model_request, &overlay).map_err(|source_error| {
                ModelExportError::Models {
                    path: path.clone(),
                    source: source_error,
                }
            })?;
        let source_key = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| ModelExportError::InvalidSourcePath(path.clone()))?;
        let relative_source_dir = format!("sources/{source_key}");
        create_dir_all(&staged.path().join(Path::new(&relative_source_dir)))?;

        let mut entry_records = Vec::with_capacity(bank.entries.len());
        for entry in bank.entries {
            let raw_file = format!(
                "{relative_source_dir}/entry_{:04}_global_{:04}.bin",
                entry.identity.local_index, entry.identity.global_index
            );
            let entry_bytes = span_bytes(&source, &path, "entry", entry.span)?;
            write(&staged.path().join(Path::new(&raw_file)), entry_bytes)?;

            let analysis = analyze_model_diagnostic(
                inf_formats::ModelDiagnosticRequest::unconditioned(&entry),
            );
            let unresolved_dependencies = match &analysis {
                ModelDiagnosticProjection::Ready(geometry) => {
                    geometry.unresolved_dependencies.clone()
                }
                ModelDiagnosticProjection::Unavailable(unavailable) => {
                    unavailable.unresolved_dependencies.clone()
                }
            };
            let diagnostic_obj =
                export_diagnostic_obj(staged.path(), &relative_source_dir, &entry, analysis)?;
            match diagnostic_obj {
                DiagnosticObjRecord::Written { .. } => totals.diagnostic_objs += 1,
                DiagnosticObjRecord::Omitted { .. } => totals.omitted_diagnostic_objs += 1,
            }
            let block_index = block_index_for_entry(&bank.blocks, entry.identity.local_index)
                .expect("parse_model_bank assigned every entry to a block");
            let raw = raw_entry_record(&source, &path, &entry)?;
            totals.raw_entry_bytes += entry_bytes.len() as u64;
            totals.raw_entry_files += 1;
            occurrences
                .entry(entry.identity.global_index)
                .or_default()
                .push(GlobalOccurrence {
                    ovl: name.to_owned(),
                    local_index: entry.identity.local_index,
                    raw_file: raw_file.clone(),
                });
            entry_records.push(ModelEntryRecord {
                block_index,
                raw_file,
                raw,
                decoded: entry,
                unresolved_dependencies,
                diagnostic_obj,
            });
        }

        totals.source_ovls += 1;
        totals.blocks += bank.blocks.len();
        totals.entries += entry_records.len();
        sources.push(ModelSource {
            ovl: name.to_owned(),
            overlay,
            region_span: bank.region_span,
            requested_entries: model_request.count,
            blocks: bank.blocks,
            entries: entry_records,
        });
    }

    let report = ModelExportReport {
        sources: totals.source_ovls,
        entries: totals.entries,
        raw_entry_files: totals.raw_entry_files,
        diagnostic_objs: totals.diagnostic_objs,
        omitted_diagnostic_objs: totals.omitted_diagnostic_objs,
    };
    let manifest = ModelManifest {
        schema_version: 4,
        kind: "infestation_handler_8_models",
        lossless_policy: LosslessPolicy {
            serialization: "Compact JSON keeps the complete typed command corpus practical; no records are omitted or deduplicated.",
            raw_entry_files: "Each .bin is the exact aligned model-entry span, including its optional name terminator and padding.",
            manifest_bytes: "Raw header/pool/name/padding bytes and the command pool are emitted as uppercase byte-exact hexadecimal alongside the typed structural decode.",
            spans: "All spans are absolute half-open byte offsets in the source OVL.",
            occurrence_order: "Global-slot occurrences retain deterministic installed-OVL manifest order; no runtime replacement precedence is inferred.",
            auxiliary_programs: "A nonzero header raw_0e enables the strictly decoded auxiliary program, including typed commands, exact argument bytes, terminator and padding spans. A zero raw_0e leaves the auxiliary pool inactive and raw. Structural decoding does not execute callbacks or establish collision outcomes.",
        },
        diagnostic_obj_policy: DiagnosticObjPolicy {
            format: "wavefront_obj",
            basis: MODEL_DIAGNOSTIC_BASIS,
            warning: "Diagnostic OBJs are proven static command projections, not canonical animation frame 0, shadow geometry, or inferred material/texture reconstruction.",
            slot_mapping: "Recovered static header slots and deterministic vertex generators are eligible; mirrored odd slots use the loader's proven X reflection.",
            omissions: "Runtime-dependent faces and child model instances are omitted and reported while independent local polygon geometry remains available; exact repeated conditional faces may contribute one explicitly partial representative. Nested geometry and commands with unresolved topology effects remain unavailable.",
        },
        ovl_filters: selected_filter_names,
        totals,
        global_slot_occurrences: occurrences
            .into_iter()
            .map(|(global_index, occurrences_in_manifest_source_order)| {
                GlobalSlotOccurrences {
                    global_index,
                    occurrences_in_manifest_source_order,
                }
            })
            .collect(),
        sources,
    };
    write_manifest(staged.path(), &manifest)?;
    staged.commit()?;
    Ok(report)
}

fn select_overlay_paths(
    overlay_paths: Vec<PathBuf>,
    filters: &[String],
) -> Result<(Vec<PathBuf>, Vec<String>, bool), ModelExportError> {
    if filters.is_empty() {
        return Ok((overlay_paths, Vec::new(), false));
    }

    let mut requested = BTreeSet::new();
    for filter in filters {
        OverlayKey::parse_filename(filter).map_err(|source| {
            ModelExportError::InvalidOverlayFilter {
                filter: filter.clone(),
                source,
            }
        })?;
        requested.insert(filter.to_ascii_lowercase());
    }

    let mut unmatched = requested.clone();
    let mut selected = Vec::new();
    let mut selected_names = Vec::new();
    for path in overlay_paths {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let normalized = name.to_ascii_lowercase();
        if requested.contains(&normalized) {
            unmatched.remove(&normalized);
            selected_names.push(name.to_owned());
            selected.push(path);
        }
    }
    if !unmatched.is_empty() {
        return Err(ModelExportError::UnknownOverlayFilters {
            filters: unmatched.into_iter().collect(),
        });
    }
    Ok((selected, selected_names, true))
}

fn raw_entry_record(
    source: &[u8],
    path: &Path,
    entry: &ModelEntry,
) -> Result<RawEntryRecord, ModelExportError> {
    let name_bytes_hex = entry
        .name
        .as_ref()
        .map(|name| span_bytes(source, path, "model name", name.bytes).map(hex_bytes))
        .transpose()?;
    let name_record_bytes_hex = entry
        .name
        .as_ref()
        .map(|name| span_bytes(source, path, "model name record", name.record).map(hex_bytes))
        .transpose()?;
    let name_ascii = entry
        .name
        .as_ref()
        .and_then(|name| name.ascii(source))
        .map(str::to_owned);
    let command_bytes = span_bytes(source, path, "command pool", entry.command_span)?;
    Ok(RawEntryRecord {
        entry_bytes: entry.span.len(),
        content_bytes: entry.content_span.len(),
        header_bytes_le_hex: hex_bytes(span_bytes(source, path, "header", entry.header_span)?),
        auxiliary_pool_bytes_le_hex: hex_bytes(span_bytes(
            source,
            path,
            "auxiliary pool",
            entry.auxiliary_span,
        )?),
        vertex_pool_bytes_le_hex: hex_bytes(span_bytes(
            source,
            path,
            "vertex pool",
            entry.vertices_span,
        )?),
        normal_pool_bytes_le_hex: hex_bytes(span_bytes(
            source,
            path,
            "normal pool",
            entry.normals_span,
        )?),
        command_pool_bytes_le_hex: hex_bytes(command_bytes),
        name_bytes_hex,
        name_ascii,
        name_record_bytes_hex,
        padding_bytes_hex: hex_bytes(span_bytes(source, path, "entry padding", entry.padding)?),
    })
}

fn export_diagnostic_obj(
    staged_root: &Path,
    relative_source_dir: &str,
    entry: &ModelEntry,
    analysis: ModelDiagnosticProjection,
) -> Result<DiagnosticObjRecord, ModelExportError> {
    let geometry = match analysis {
        ModelDiagnosticProjection::Ready(geometry) => geometry,
        ModelDiagnosticProjection::Unavailable(unavailable) => {
            return Ok(DiagnosticObjRecord::Omitted {
                basis: MODEL_DIAGNOSTIC_BASIS,
                kind: unavailable.kind,
                reason: unavailable.reason,
                referenced_face_slots: unavailable
                    .referenced_face_slots
                    .into_iter()
                    .map(|slot| slot.raw())
                    .collect(),
                unresolved_dependencies: unavailable.unresolved_dependencies,
                omitted_effects: unavailable.omitted_effects,
            })
        }
    };

    let file = format!(
        "{relative_source_dir}/entry_{:04}_global_{:04}.diagnostic_raw_slots.obj",
        entry.identity.local_index, entry.identity.global_index
    );
    let obj = write_obj(&geometry);
    write(&staged_root.join(Path::new(&file)), obj.as_bytes())?;
    Ok(DiagnosticObjRecord::Written {
        file,
        basis: MODEL_DIAGNOSTIC_BASIS,
        vertex_count: geometry.vertices.len(),
        face_count: geometry.faces.len(),
        completeness: geometry.completeness,
        face_presentation: geometry.face_presentation,
        ignored_edge_commands: geometry.ignored_edge_commands,
        omitted_effects: geometry.omitted_effects,
        unresolved_dependencies: geometry.unresolved_dependencies,
    })
}

fn write_obj(geometry: &ModelDiagnosticGeometry) -> String {
    let mut output = String::new();
    output.push_str("# Infestation Handler-8 proven static-command topology projection\n");
    output
        .push_str("# NOT canonical animation frame 0; no shadows or resolved material payloads\n");
    output.push_str("# Coordinates retain raw archive units and axes\n");
    output.push_str(&format!("# completeness {:?}\n", geometry.completeness));
    for vertex in &geometry.vertices {
        output.push_str(&format!(
            "# slot {}\nv {} {} {}\n",
            vertex.slot.raw(),
            vertex.position[0],
            vertex.position[1],
            vertex.position[2]
        ));
    }
    debug_assert_eq!(geometry.faces.len(), geometry.face_presentation.len());
    for (face, presentation) in geometry.faces.iter().zip(&geometry.face_presentation) {
        output.push_str(&obj_face_presentation_comment(presentation));
        output.push('f');
        for index in face {
            output.push_str(&format!(" {}", index + 1));
        }
        output.push('\n');
    }
    output
}

fn obj_face_presentation_comment(presentation: &ModelDiagnosticFacePresentation) -> String {
    let material_domain = match presentation.material.domain {
        ModelMaterialDomain::Handler3Texture => "handler3_texture",
        ModelMaterialDomain::Handler7Material => "handler7_material",
    };
    let flags = match presentation.source_flags {
        ModelDiagnosticFaceFlags::Face(flags) => flags.raw(),
        ModelDiagnosticFaceFlags::Fan(flags) => flags.raw(),
    };
    let lighting = match &presentation.source_lighting {
        ModelDiagnosticFaceLighting::Shared(slot) => format!("shared:{}", slot.raw()),
        ModelDiagnosticFaceLighting::PerVertex(slots) => format!(
            "per_vertex:{}",
            slots
                .iter()
                .map(|slot| slot.raw().to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    format!(
        "# face_source opcode=0x{:02X} material={material_domain}:{} flags=0x{flags:04X} lighting={lighting} mirrored={} fan_attributes={}\n",
        presentation.source_opcode,
        presentation.material.global_id,
        presentation.mirrored_instance,
        presentation.fan_vertex_attributes.is_some(),
    )
}

fn block_index_for_entry(blocks: &[ModelBlock], local_index: u16) -> Option<usize> {
    blocks.iter().position(|block| {
        let start = u32::from(block.first_entry);
        let end = start + u32::from(block.entry_count);
        let index = u32::from(local_index);
        start <= index && index < end
    })
}

fn source_file_name(path: &Path) -> Result<&str, ModelExportError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ModelExportError::InvalidSourcePath(path.to_owned()))
}

fn span_bytes<'a>(
    source: &'a [u8],
    path: &Path,
    section: &'static str,
    span: ByteSpan,
) -> Result<&'a [u8], ModelExportError> {
    span.get(source)
        .ok_or_else(|| ModelExportError::SpanOutOfBounds {
            path: path.to_owned(),
            section,
            span,
        })
}

fn hex_bytes(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn write_manifest(root: &Path, manifest: &ModelManifest) -> Result<(), ModelExportError> {
    let path = root.join("manifest.json");
    let file = fs::File::create(&path).map_err(|source| ModelExportError::Write {
        path: path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, manifest).map_err(|source| ModelExportError::Json {
        path: path.clone(),
        source,
    })?;
    writer
        .flush()
        .map_err(|source| ModelExportError::Write { path, source })
}

fn read(path: &Path) -> Result<Vec<u8>, ModelExportError> {
    fs::read(path).map_err(|source| ModelExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), ModelExportError> {
    fs::create_dir_all(path).map_err(|source| ModelExportError::Write {
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ModelExportError> {
    fs::write(path, bytes).map_err(|source| ModelExportError::Write {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::{
        decode_model_program, ModelFlags, ModelHeader, ModelIdentity, ModelName, ModelVectorI16,
    };

    fn diagnostic_entry(command_words: &[u16], ranges: [u16; 4]) -> ModelEntry {
        ModelEntry {
            identity: ModelIdentity {
                local_index: 3,
                global_index: 17,
            },
            span: ByteSpan::new(0, 0),
            content_span: ByteSpan::new(0, 0),
            header_span: ByteSpan::new(0, 0),
            header: ModelHeader {
                visibility_extent: 0,
                command_word_count: command_words.len() as u16,
                auxiliary_count: 0,
                flags: ModelFlags::from_raw(0x40),
                vertex_slot_count: ranges[3],
                raw_vertex_count: 3,
                shade_slot_count: 2,
                raw_0e: 0,
                reserved_10: 0,
                vertex_ranges: ranges,
                reserved_1a: 0,
                shade_ranges: [0, 0],
            },
            auxiliary_span: ByteSpan::new(0, 0),
            auxiliary: Vec::new(),
            auxiliary_program: None,
            vertices_span: ByteSpan::new(0, 0),
            vertices: vec![
                ModelVectorI16 { x: 10, y: 0, z: 0 },
                ModelVectorI16 { x: 20, y: 1, z: 0 },
                ModelVectorI16 { x: 30, y: 0, z: 1 },
            ],
            normals_span: ByteSpan::new(0, 0),
            normals: Vec::new(),
            command_span: ByteSpan::new(0, command_words.len() * 2),
            program: decode_model_program(command_words).unwrap(),
            name: None::<ModelName>,
            padding: ByteSpan::new(0, 0),
        }
    }

    #[test]
    fn diagnostic_obj_keeps_proven_faces_and_labels_itself_noncanonical() {
        let entry = diagnostic_entry(&[0x00d0, 0xffff, 0, 2, 4, 0, 0, 0, 0], [0, 4, 6, 6]);
        let ModelDiagnosticProjection::Ready(geometry) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
        else {
            panic!("expected static-command diagnostic")
        };
        assert_eq!(
            geometry
                .vertices
                .iter()
                .map(|vertex| vertex.slot.raw())
                .collect::<Vec<_>>(),
            [0, 2, 4]
        );
        assert_eq!(geometry.faces, [vec![0, 1, 2]]);
        let obj = write_obj(&geometry);
        assert!(obj.contains("NOT canonical animation frame 0"));
        assert!(obj.contains("face_source opcode=0xD0"));
        assert!(obj.contains("material=handler7_material:-1"));
        assert!(obj.contains("v 10 0 0"));
        assert!(obj.contains("f 1 2 3"));
        let serialized_material =
            serde_json::to_value(geometry.face_presentation[0].material).unwrap();
        assert_eq!(serialized_material["domain"], "handler7_material");
        assert_eq!(serialized_material["global_id"], -1);
        assert!(serialized_material.get("local_index").is_none());

        let mut dynamic = diagnostic_entry(&[0x00d0, 0xffff, 0, 2, 4, 0, 0, 0, 0], [2, 4, 6, 6]);
        // Exclude slot zero from projection initialization too. An unknown
        // world field alone does not make its separate face input dynamic.
        dynamic.header.reserved_10 = 2;
        let ModelDiagnosticProjection::Unavailable(unavailable) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&dynamic))
        else {
            panic!("dynamic slot must not produce OBJ")
        };
        assert_eq!(
            unavailable.kind,
            ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
        );
        assert!(unavailable
            .referenced_face_slots
            .iter()
            .any(|slot| slot.raw() == 0));
        assert_eq!(
            unavailable
                .omitted_effects
                .iter()
                .find(|omission| {
                    omission.kind == inf_formats::ModelDiagnosticOmissionKind::UnresolvedFaceSlots
                })
                .map(|omission| omission.count),
            Some(1)
        );

        let opaque = diagnostic_entry(
            &[0x0004, 1, 2, 3, 0x00d0, 0xffff, 0, 2, 4, 0, 0, 0, 0],
            [0, 4, 6, 6],
        );
        let ModelDiagnosticProjection::Ready(geometry) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&opaque))
        else {
            panic!("proven state/setup opcode 0x04 must not hide independent geometry")
        };
        assert_eq!(geometry.faces.len(), 1);
    }

    #[test]
    fn filtered_retail_export_preserves_every_entry_and_is_repeatable() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let output_root =
            std::env::temp_dir().join(format!("infestation-model-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&output_root);
        let options = ModelExportOptions {
            ovl_filters: vec!["1.OVL".to_owned(), "1.ovl".to_owned()],
        };

        let first = export(&data_root, &output_root, &options).unwrap();
        assert_eq!(first.sources, 1);
        assert_eq!(first.entries, 278);
        assert_eq!(first.raw_entry_files, first.entries);
        assert_eq!(
            first.diagnostic_objs + first.omitted_diagnostic_objs,
            first.entries
        );
        let manifest_path = output_root.join("models/manifest.json");
        let first_manifest = fs::read(&manifest_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&first_manifest).unwrap();
        assert_eq!(manifest["schema_version"], 4);
        assert_eq!(manifest["ovl_filters"], serde_json::json!(["1.ovl"]));
        assert_eq!(
            manifest["sources"][0]["entries"].as_array().unwrap().len(),
            278
        );
        let first_entry = &manifest["sources"][0]["entries"][0];
        assert_eq!(first_entry["decoded"]["identity"]["global_index"], 2);
        assert_eq!(
            first_entry["raw"]["header_bytes_le_hex"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        assert!(first_entry["decoded"]["program"]["instructions"].is_array());

        let gunbase = manifest["sources"][0]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["decoded"]["identity"]["global_index"] == 42)
            .expect("1.ovl GUNBASE entry");
        let expected_dependencies = serde_json::json!([
            "custom_vertex_callback",
            "external_model_catalog",
            "saved_transform_basis"
        ]);
        assert_eq!(gunbase["unresolved_dependencies"], expected_dependencies);
        assert_eq!(
            gunbase["diagnostic_obj"]["unresolved_dependencies"],
            expected_dependencies
        );

        let raw_file = first_entry["raw_file"].as_str().unwrap();
        let raw = fs::read(output_root.join("models").join(raw_file)).unwrap();
        let start = first_entry["decoded"]["span"]["start"].as_u64().unwrap() as usize;
        let end = first_entry["decoded"]["span"]["end"].as_u64().unwrap() as usize;
        assert_eq!(
            raw,
            fs::read(data_root.join("Overlay/1.ovl")).unwrap()[start..end]
        );

        let second = export(&data_root, &output_root, &options).unwrap();
        assert_eq!(second, first);
        assert_eq!(fs::read(&manifest_path).unwrap(), first_manifest);
        let _ = fs::remove_dir_all(output_root);
    }

    #[test]
    fn retail_auxiliary_export_preserves_typed_spans_and_inactive_raw_pools() {
        fn span(value: &serde_json::Value) -> std::ops::Range<usize> {
            value["start"].as_u64().unwrap() as usize..value["end"].as_u64().unwrap() as usize
        }

        let data_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../retail")
            .canonicalize()
            .unwrap();
        let scratch = data_root.join("../.tmp");
        fs::create_dir_all(&scratch).unwrap();
        let scratch = scratch.canonicalize().unwrap();
        let output_root = scratch.join(format!(
            "infestation-model-aux-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(!output_root.exists());
        export(
            &data_root,
            &output_root,
            &ModelExportOptions {
                ovl_filters: vec!["32.ovl".to_owned()],
            },
        )
        .unwrap();
        let source = fs::read(data_root.join("Overlay/32.ovl")).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output_root.join("models/manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["schema_version"], 4);
        assert!(manifest["lossless_policy"]["auxiliary_programs"]
            .as_str()
            .unwrap()
            .contains("does not execute callbacks"));
        let entries = manifest["sources"][0]["entries"].as_array().unwrap();
        let mut active = 0;
        let mut inactive = 0;
        for entry in entries {
            let decoded = &entry["decoded"];
            let auxiliary = span(&decoded["auxiliary_span"]);
            assert_eq!(
                entry["raw"]["auxiliary_pool_bytes_le_hex"],
                hex_bytes(&source[auxiliary.clone()])
            );
            let program = &decoded["auxiliary_program"];
            if decoded["header"]["raw_0e"] == 0 {
                inactive += 1;
                assert!(program.is_null());
                continue;
            }
            active += 1;
            assert_eq!(span(&program["span"]).start, auxiliary.start);
            let mut next = auxiliary.start;
            for instruction in program["instructions"].as_array().unwrap() {
                let command = span(&instruction["span"]);
                let arguments = span(&instruction["arguments_span"]);
                assert_eq!(command.start, next);
                assert!((command.start + 1..=command.start + 4).contains(&arguments.start));
                assert_eq!(arguments.end, command.end);
                assert_eq!(instruction["opcode"], source[command.start]);
                assert_eq!(
                    instruction["raw_arguments"],
                    serde_json::json!(&source[arguments])
                );
                assert!(instruction["kind"]["kind"].is_string());
                next = command.end;
            }
            let terminator = span(&program["terminator"]);
            let padding = span(&program["padding"]);
            assert_eq!(terminator, next..next + 1);
            assert_eq!(span(&program["span"]).end, terminator.end);
            assert_eq!(source[terminator.start], 0x88);
            assert_eq!(padding, terminator.end..auxiliary.end);
        }
        assert!(active > 0 && inactive > 0);
        let parent = entries
            .iter()
            .find(|entry| entry["decoded"]["identity"]["global_index"] == 553)
            .unwrap();
        let first = &parent["decoded"]["auxiliary_program"]["instructions"][0];
        assert_eq!(first["opcode"], 7);
        assert_eq!(first["kind"]["kind"], "shared_vertex_handler");
        assert_eq!(first["kind"]["data"]["renderer_opcode"], 0x20);
        assert_eq!(first["kind"]["data"]["handler_command"], 0x0700);
        let raw = fs::read(
            output_root
                .join("models")
                .join(parent["raw_file"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(raw, source[span(&parent["decoded"]["span"])]);
        assert_eq!(output_root.parent(), Some(scratch.as_path()));
        fs::remove_dir_all(output_root).unwrap();
    }
}
